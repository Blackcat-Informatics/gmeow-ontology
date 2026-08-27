// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! A content-keyed, graph-preserving GTS -> indexed [`purrdf::RdfDataset`] product.
//!
//! Required repository commands and whole-bundle tests run in separate processes. This
//! boundary makes them share the expensive container decode/freeze/index construction:
//! a first process imports normally and publishes an immutable `PURRPCK1` image; later
//! processes verify its receipt/blob and restore the exact indexed dataset. Producer
//! callers use [`import_graph_preserving_cached`], where a clean miss computes the
//! product. Test-facing callers use [`load_graph_preserving_cached`], where a clean miss
//! is terminal and can never invoke the importer. A referenced missing/truncated/tampered
//! receipt or pack hard fails. Per-key OS election locks prevent duplicate builders,
//! atomic rename prevents a partial publication, and a store lease makes bounded GC safe
//! against active readers.
//!
//! This cache never substitutes for the raw GTS frame/profile audit. Callers retain and
//! independently grade the original bytes where header/blob/compression semantics matter.

use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use gmeow_action_cache::{
    ActionContext, ActionInput, ActionStore, FileKind, ProducerIdentity, STORE_FORMAT_VERSION,
    StoreLimits,
};
use purrdf::{ContentDigest, PackBuilder, RdfDataset, restore_pack};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use gmeow_errors::{Code, FindingCategory, Grade, Severity, Standpoint, define_diag_kind};

#[cfg(test)]
#[path = "../../../build-support/path_dependency_inputs.rs"]
mod build_inputs;

define_diag_kind! {
    /// A content-keyed bundle import could not be built, verified, restored, or
    /// published atomically. Cached material is never trusted after this refusal.
    pub struct BundleImport { detail: String }
    code = "bundle-import.cache";
    grade = Grade::new(Severity::Error, FindingCategory::ModelingDisciplineViolation, Standpoint::Binding);
    message = "{}", detail;
    failure_class = "https://blackcatinformatics.ca/gmeow/BundleArtifactUnreadable";
}

/// The complete bundle-import diagnostic-code catalog, in registration order.
pub const BUNDLE_IMPORT_DIAG_CODES: &[&str] = &[BundleImport::CODE];

/// Eagerly intern every bundle-import diagnostic code (idempotent).
#[must_use]
pub fn register_all() -> Vec<Code> {
    vec![BundleImport::register()]
}

const SCHEMA_VERSION: u32 = 1;
const CODEC: &str = "gts-events-to-purrpack1-graph-preserving-v1";
const MAX_PACK_BYTES: u64 = 512 * 1024 * 1024;
const MAX_RECEIPT_BYTES: u64 = 1024 * 1024;
const RETAINED_IMPORTS: usize = 2;
const RETAINED_NAMESPACES: usize = 4;
const MAX_STORE_BYTES: u64 = 2 * 1024 * 1024 * 1024;
const STORE_SENTINEL: &str = ".gmeow-bundle-import-store-v1";
const STORE_SENTINEL_BYTES: &[u8] = b"gmeow-bundle-import-store:v1\n";
const CORPUS_ARTIFACT_CODEC: &str = "authenticated-corpus-artifact-v1";
const TEST_FIXTURE_MANIFEST_PATH_ENV: &str = "GMEOW_TEST_FIXTURE_MANIFEST";
const TEST_FIXTURE_MANIFEST_SHA256_ENV: &str = "GMEOW_TEST_FIXTURE_MANIFEST_SHA256";
const TEST_FIXTURE_MANIFEST_SCHEMA_VERSION: u32 = 2;
const BUNDLE_FIXTURE_SELECTOR_SCHEMA_VERSION: u32 = 1;
const MAX_TEST_FIXTURE_MANIFEST_BYTES: u64 = 16 * 1024 * 1024;

/// Exact producer identity over this implementation, dependency lock/configuration,
/// rustc, target, profile, features, and code-generation flags.
pub const BUILD_FINGERPRINT: &str = env!("GMEOW_BUNDLE_IMPORT_BUILD_FINGERPRINT");

/// Immutable identity and structural census for one imported dataset.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImportReceipt {
    pub schema_version: u32,
    pub action_key: String,
    pub build_fingerprint: String,
    pub codec: String,
    pub source_digest: String,
    pub source_bytes: u64,
    pub pack_digest: String,
    pub pack_bytes: u64,
    pub dataset_quads: u64,
    pub named_graphs: u64,
}

impl ImportReceipt {
    /// Deterministic identity of this immutable receipt, excluding run observations.
    #[must_use]
    pub fn receipt_digest(&self) -> String {
        digest(&[
            b"gmeow:bundle-import-receipt:v1",
            &serde_json::to_vec(self).expect("closed receipt JSON"),
        ])
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ReceiptEnvelope {
    receipt_digest: String,
    receipt: ImportReceipt,
}

/// A verified graph-preserving dataset plus observational cache telemetry.
#[derive(Debug)]
pub struct ImportOutcome {
    pub dataset: Arc<RdfDataset>,
    pub receipt: ImportReceipt,
    pub built: bool,
    pub transferred_bytes: u64,
}

/// Producer-side admission of one exact graph-preserving import action.
///
/// A warm admission authenticates the immutable receipt and packed-product bytes but
/// deliberately does not restore the indexed dataset. Tests load that dataset through
/// [`load_graph_preserving_cached`]; restoring it in the producer first would duplicate
/// the largest warm-path allocation without strengthening their selected identity.
#[derive(Debug)]
pub struct ImportAdmission {
    pub receipt: ImportReceipt,
    pub built: bool,
    pub transferred_bytes: u64,
}

/// Exact identity of one producer-published, bundle-derived test artifact.
///
/// The profile that executes tests may differ from the profile that admitted the
/// producer action. Consumers therefore use this producer-issued identity instead of
/// deriving a new action key from their own executable profile.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CorpusArtifactPublication {
    pub schema_version: u32,
    pub action_key: String,
    pub receipt_digest: String,
    pub build_fingerprint: String,
    pub name: String,
    pub source_sha256: String,
    pub product_digest: String,
    pub product_bytes: u64,
}

/// Producer-issued selection for every fixture derived from one exact GTS bundle.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BundleFixtureSelector {
    pub schema_version: u32,
    pub receipt_digest: String,
    pub receipt: ImportReceipt,
    pub corpus_artifacts: BTreeMap<String, CorpusArtifactPublication>,
}

#[derive(Debug, Deserialize)]
struct TestFixtureSelectorEnvelope {
    schema_version: u32,
    bundle_import: BundleFixtureSelector,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct CorpusArtifactPayload {
    schema_version: u32,
    name: String,
    source_sha256: String,
}

fn expected_repository_source_sha256() -> gmeow_errors::Result<String> {
    let expected = std::env::var("GMEOW_BUNDLE_IMPORT_SOURCE_SHA256").map_err(|_| {
        diag(
            "bundle import: GMEOW_BUNDLE_IMPORT_SOURCE_SHA256 is required; tests may only read \
             an explicitly selected corpus identity",
        )
    })?;
    if expected.len() != 64 || !expected.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(diag(
            "bundle import: GMEOW_BUNDLE_IMPORT_SOURCE_SHA256 must be a 64-digit SHA-256",
        ));
    }
    Ok(expected.to_ascii_lowercase())
}

fn validate_bundle_fixture_selector(selector: &BundleFixtureSelector) -> gmeow_errors::Result<()> {
    let receipt = &selector.receipt;
    if selector.schema_version != BUNDLE_FIXTURE_SELECTOR_SCHEMA_VERSION
        || receipt.schema_version != SCHEMA_VERSION
        || receipt.codec != CODEC
        || selector.receipt_digest != receipt.receipt_digest()
        || !is_digest(&selector.receipt_digest)
        || !is_digest(&receipt.action_key)
        || !is_digest(&receipt.build_fingerprint)
        || !is_digest(&receipt.source_digest)
        || !is_digest(&receipt.pack_digest)
    {
        return Err(diag(
            "bundle import: producer fixture selector has an invalid receipt identity",
        ));
    }
    let expected_action = digest(&[
        b"gmeow:bundle-import-action:v1",
        receipt.build_fingerprint.as_bytes(),
        CODEC.as_bytes(),
        receipt.source_digest.as_bytes(),
    ]);
    if receipt.action_key != expected_action {
        return Err(diag(
            "bundle import: producer fixture selector action key is not receipt-derived",
        ));
    }
    for (name, artifact) in &selector.corpus_artifacts {
        if artifact.schema_version != 1
            || artifact.name != *name
            || artifact.build_fingerprint != receipt.build_fingerprint
            || artifact.source_sha256 != receipt.source_digest
            || !is_digest(&artifact.action_key)
            || !is_digest(&artifact.receipt_digest)
            || !is_digest(&artifact.product_digest)
        {
            return Err(diag(format!(
                "bundle import: producer fixture selector has an invalid artifact identity for {name}"
            )));
        }
        validate_corpus_artifact_name(name)?;
        let context =
            corpus_artifact_context_for(&artifact.source_sha256, name, &artifact.build_fingerprint);
        if artifact.action_key != context.key().as_str() {
            return Err(diag(format!(
                "bundle import: producer fixture selector action key is not context-derived for {name}"
            )));
        }
    }
    Ok(())
}

fn load_bundle_fixture_selector() -> gmeow_errors::Result<BundleFixtureSelector> {
    let selected_path = std::env::var_os(TEST_FIXTURE_MANIFEST_PATH_ENV)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            diag(format!(
                "bundle import: {TEST_FIXTURE_MANIFEST_PATH_ENV} is required; tests may not discover a producer identity"
            ))
        })?;
    let selected_path = PathBuf::from(selected_path);
    let path = if selected_path.is_absolute() {
        selected_path
    } else {
        std::env::current_dir()
            .map_err(io_diag)?
            .join(selected_path)
    };
    let expected = std::env::var(TEST_FIXTURE_MANIFEST_SHA256_ENV)
        .ok()
        .map(|value| value.to_ascii_lowercase())
        .filter(|value| is_digest(value))
        .ok_or_else(|| {
            diag(format!(
                "bundle import: {TEST_FIXTURE_MANIFEST_SHA256_ENV} must select one exact SHA-256"
            ))
        })?;
    let bytes = read_bounded(
        &path,
        MAX_TEST_FIXTURE_MANIFEST_BYTES,
        "test fixture selector",
    )?;
    let actual = format!("{:x}", Sha256::digest(&bytes));
    if actual != expected {
        return Err(diag(format!(
            "bundle import: test fixture selector identity mismatch: expected {expected}, actual {actual}"
        )));
    }
    let envelope: TestFixtureSelectorEnvelope =
        serde_json::from_slice(&bytes).map_err(|error| {
            diag(format!(
                "bundle import: decode test fixture selector: {error}"
            ))
        })?;
    if envelope.schema_version != TEST_FIXTURE_MANIFEST_SCHEMA_VERSION {
        return Err(diag(format!(
            "bundle import: test fixture selector schema {} != {TEST_FIXTURE_MANIFEST_SCHEMA_VERSION}",
            envelope.schema_version
        )));
    }
    validate_bundle_fixture_selector(&envelope.bundle_import)?;
    Ok(envelope.bundle_import)
}

fn corpus_artifact_context_for(
    source_sha256: &str,
    name: &str,
    build_fingerprint: &str,
) -> ActionContext {
    ActionContext::new(
        "test-corpus",
        name,
        ProducerIdentity::new(format!("{build_fingerprint}:{CORPUS_ARTIFACT_CODEC}")),
        CORPUS_ARTIFACT_CODEC,
        vec![ActionInput::Raw {
            logical_path: "generated/dist/gmeow.gts".to_string(),
            file_kind: FileKind::File,
            executable: false,
            digest: source_sha256.to_string(),
        }],
    )
}

fn corpus_artifact_context(source_sha256: &str, name: &str) -> ActionContext {
    corpus_artifact_context_for(source_sha256, name, BUILD_FINGERPRINT)
}

fn validate_corpus_artifact_name(name: &str) -> gmeow_errors::Result<()> {
    if !name.is_empty()
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
    {
        Ok(())
    } else {
        Err(diag(format!(
            "bundle import: invalid authenticated corpus artifact name {name:?}"
        )))
    }
}

/// Publish a derived corpus artifact from an explicitly selected source bundle.
///
/// This is a producer-only API. The action key binds the exact source bytes, artifact
/// kind, extraction codec, and producer build fingerprint. Test processes must call
/// [`load_authenticated_corpus_artifact`] instead.
pub fn publish_authenticated_corpus_artifact(
    repo_root: &Path,
    gts_bytes: &[u8],
    name: &str,
    bytes: &[u8],
) -> gmeow_errors::Result<CorpusArtifactPublication> {
    validate_corpus_artifact_name(name)?;
    let source_sha256 = ContentDigest::of(gts_bytes).to_hex();
    let context = corpus_artifact_context(&source_sha256, name);
    let payload = CorpusArtifactPayload {
        schema_version: 1,
        name: name.to_string(),
        source_sha256: source_sha256.clone(),
    };
    let store = ActionStore::open(
        ActionStore::default_root(repo_root),
        STORE_FORMAT_VERSION,
        StoreLimits::default(),
    )
    .map_err(|error| {
        diag(format!(
            "bundle import: open corpus artifact store: {error}"
        ))
    })?;
    let receipt = store
        .publish(&context, ContentDigest::of(bytes).to_hex(), payload, bytes)
        .map_err(|error| diag(format!("bundle import: publish corpus artifact: {error}")))?;
    Ok(CorpusArtifactPublication {
        schema_version: 1,
        action_key: receipt.action_key.as_str().to_owned(),
        receipt_digest: receipt.digest(),
        build_fingerprint: BUILD_FINGERPRINT.to_string(),
        name: name.to_string(),
        source_sha256: source_sha256.to_string(),
        product_digest: receipt.product_digest,
        product_bytes: receipt.product_blob.bytes,
    })
}

/// Load one already-produced corpus artifact selected by the runner's exact bundle SHA.
///
/// A missing or corrupt action is terminal. This function has no producer callback and
/// never derives the requested bytes from source.
pub fn load_authenticated_corpus_artifact(
    repo_root: &Path,
    name: &str,
) -> gmeow_errors::Result<Vec<u8>> {
    validate_corpus_artifact_name(name)?;
    let source_sha256 = expected_repository_source_sha256()?;
    let selector = load_bundle_fixture_selector()?;
    if selector.receipt.source_digest != source_sha256 {
        return Err(diag(format!(
            "bundle import: fixture selector source {} != runner-selected {source_sha256}",
            selector.receipt.source_digest
        )));
    }
    let selected = selector.corpus_artifacts.get(name).ok_or_else(|| {
        diag(format!(
            "authenticated corpus artifact {name:?} is absent from the producer selector; tests may not rebuild it"
        ))
    })?;
    let context = corpus_artifact_context_for(&source_sha256, name, &selected.build_fingerprint);
    let expected_payload = CorpusArtifactPayload {
        schema_version: 1,
        name: name.to_string(),
        source_sha256,
    };
    let store = ActionStore::open_existing_read_only(
        ActionStore::default_root(repo_root),
        STORE_FORMAT_VERSION,
        StoreLimits::default(),
    )
    .map_err(|error| {
        diag(format!(
            "bundle import: open corpus artifact store read-only: {error}"
        ))
    })?;
    let entry = store
        .get::<CorpusArtifactPayload>(&context)
        .map_err(|error| diag(format!("bundle import: load corpus artifact: {error}")))?
        .ok_or_else(|| {
            diag(format!(
                "authenticated corpus artifact {name:?} is absent; tests may not rebuild it"
            ))
        })?;
    if entry.receipt.payload != expected_payload {
        return Err(diag(format!(
            "bundle import: authenticated corpus artifact {name:?} payload identity mismatch"
        )));
    }
    if entry.receipt.action_key.as_str() != selected.action_key
        || entry.receipt.digest() != selected.receipt_digest
        || entry.receipt.product_digest != selected.product_digest
        || entry.receipt.product_blob.bytes != selected.product_bytes
    {
        return Err(diag(format!(
            "bundle import: authenticated corpus artifact {name:?} differs from the producer selector"
        )));
    }
    Ok(entry.bytes)
}

/// Load and decode one producer-published deterministic ustar corpus archive.
///
/// This is a read-only test-consumer seam over [`load_authenticated_corpus_artifact`].
/// An absent, stale, corrupt, or malformed archive hard-fails; it never discovers
/// repository files or invokes an archive producer.
pub fn load_authenticated_corpus_archive(
    repo_root: &Path,
    name: &str,
) -> gmeow_errors::Result<BTreeMap<String, Vec<u8>>> {
    let bytes = load_authenticated_corpus_artifact(repo_root, name)?;
    let members = purrdf::ustar::read_archive(&bytes).map_err(|error| {
        diag(format!(
            "bundle import: decode corpus archive {name:?}: {error}"
        ))
    })?;
    Ok(members.into_iter().collect())
}

/// Read the repository's selected GTS source only after authenticating its exact identity.
///
/// Test runners must supply `GMEOW_BUNDLE_IMPORT_SOURCE_SHA256`; absence or mismatch is
/// terminal. This helper performs no import, cache publication, generation, or fallback and
/// is therefore suitable for tests that inspect wire bytes directly.
pub fn load_authenticated_source_bytes(repo_root: &Path) -> gmeow_errors::Result<Vec<u8>> {
    let expected = expected_repository_source_sha256()?;
    let path = repo_root.join("generated/dist/gmeow.gts");
    let bytes = fs::read(&path).map_err(|error| {
        diag(format!(
            "bundle import: read authenticated source {}: {error}",
            path.display()
        ))
    })?;
    let actual = ContentDigest::of(&bytes).to_hex();
    if actual != expected {
        return Err(diag(format!(
            "bundle import: selected source identity mismatch: expected {expected}, actual {actual}"
        )));
    }
    Ok(bytes)
}

/// Select the repository's materialized GTS path only after authenticating its bytes.
///
/// This is the test-facing seam for consumer commands whose public contract accepts a
/// filename rather than bytes. It performs no production or fallback. Callers must keep
/// the returned path read-only for the duration of the consumer invocation.
pub fn authenticated_source_path(repo_root: &Path) -> gmeow_errors::Result<PathBuf> {
    load_authenticated_source_bytes(repo_root)?;
    Ok(repo_root.join("generated/dist/gmeow.gts"))
}

/// Load the repository's already-produced, graph-preserving corpus product.
///
/// Both the raw source identity and the immutable import receipt/blob are authenticated.
/// A clean cache miss is terminal; this function never invokes the importer or publishes
/// cache state.
pub fn load_authenticated_repository_bundle(
    repo_root: &Path,
) -> gmeow_errors::Result<ImportOutcome> {
    let cache_root = std::env::var_os("GMEOW_BUNDLE_IMPORT_CACHE")
        .map(PathBuf::from)
        .ok_or_else(|| {
            diag(
                "bundle import: GMEOW_BUNDLE_IMPORT_CACHE is required; tests may not build a \
                 missing corpus fixture",
            )
        })?;
    let bytes = load_authenticated_source_bytes(repo_root)?;
    let selector = load_bundle_fixture_selector()?;
    let source_digest = ContentDigest::of(&bytes).to_hex();
    if selector.receipt.source_digest != source_digest {
        return Err(diag(format!(
            "bundle import: fixture selector source {} != authenticated repository source {source_digest}",
            selector.receipt.source_digest
        )));
    }
    let outcome = load_graph_preserving_selected(&cache_root, &bytes, &selector)?;
    if outcome.built {
        return Err(diag(
            "bundle import: test-facing repository loader unexpectedly produced a corpus fixture",
        ));
    }
    Ok(outcome)
}

/// Import `gts_bytes` once across processes, rooted under `cache_root`.
///
/// # Errors
///
/// A clean miss imports and publishes. An unreadable GTS input, corrupt receipt,
/// missing/tampered pack, structurally invalid restore, nondeterministic same-key
/// publication, or entry above the explicit 512-MiB bound is a hard error.
pub fn import_graph_preserving_cached(
    cache_root: &Path,
    gts_bytes: &[u8],
) -> gmeow_errors::Result<ImportOutcome> {
    fs::create_dir_all(cache_root).map_err(io_diag)?;
    ensure_real_directory(cache_root, "cache root")?;
    let root_lock = open_lock(&cache_root.join("store.lock"))?;
    // Admit or initialize this directory while holding the root exclusively. The
    // quota collector may recursively remove obsolete namespaces, so an accidentally
    // broad or unrelated cache root must be rejected before any entry is considered
    // evictable.
    root_lock.lock().map_err(io_diag)?;
    initialize_store_root(cache_root)?;
    root_lock.unlock().map_err(io_diag)?;
    root_lock.lock_shared().map_err(io_diag)?;
    let outcome = import_graph_preserving_under_root(cache_root, gts_bytes)?;
    root_lock.unlock().map_err(io_diag)?;
    // Enforce the bound after hits as well as publications. An outer CI transfer may
    // restore obsolete namespaces alongside a valid current entry; a warm hit cannot
    // make those bytes exempt from the store contract.
    prune_store(cache_root, BUILD_FINGERPRINT)?;
    Ok(outcome)
}

/// Admit the exact graph-preserving import before tests without eagerly restoring a
/// warm packed dataset.
///
/// A clean miss delegates to [`import_graph_preserving_cached`] and therefore produces
/// the action exactly once. A hit re-hashes the referenced pack and validates its
/// immutable receipt, returning only that identity. This is a producer API, not a test
/// fallback; test consumers remain read-only through [`load_graph_preserving_cached`].
pub fn admit_graph_preserving_cached(
    cache_root: &Path,
    gts_bytes: &[u8],
) -> gmeow_errors::Result<ImportAdmission> {
    if let Some(receipt) = inspect_graph_preserving_cached(cache_root, gts_bytes)? {
        return Ok(ImportAdmission {
            transferred_bytes: receipt.pack_bytes,
            receipt,
            built: false,
        });
    }
    let outcome = import_graph_preserving_cached(cache_root, gts_bytes)?;
    Ok(ImportAdmission {
        receipt: outcome.receipt,
        built: outcome.built,
        transferred_bytes: outcome.transferred_bytes,
    })
}

fn inspect_graph_preserving_cached(
    cache_root: &Path,
    gts_bytes: &[u8],
) -> gmeow_errors::Result<Option<ImportReceipt>> {
    if !cache_root.exists() {
        return Ok(None);
    }
    ensure_real_directory(cache_root, "cache root")?;
    let sentinel_path = cache_root.join(STORE_SENTINEL);
    if !sentinel_path.exists() {
        return Ok(None);
    }
    let sentinel = read_bounded(
        &sentinel_path,
        u64::try_from(STORE_SENTINEL_BYTES.len()).unwrap_or(u64::MAX),
        "store sentinel",
    )?;
    if sentinel != STORE_SENTINEL_BYTES {
        return Err(diag("bundle import: store sentinel identity mismatch"));
    }

    let source_digest = ContentDigest::of(gts_bytes).to_hex();
    let action_key = digest(&[
        b"gmeow:bundle-import-action:v1",
        BUILD_FINGERPRINT.as_bytes(),
        CODEC.as_bytes(),
        source_digest.as_bytes(),
    ]);
    let build_root = cache_root.join(BUILD_FINGERPRINT);
    if !build_root.exists() {
        return Ok(None);
    }
    let namespace = build_root.join(format!("v{SCHEMA_VERSION}"));
    if !namespace.exists() {
        return Ok(None);
    }
    ensure_real_directory(&build_root, "build namespace")?;
    validate_build_root(&build_root)?;
    ensure_real_directory(&namespace, "schema namespace")?;
    for lane in ["receipts", "blobs", "locks"] {
        ensure_real_directory(&namespace.join(lane), "cache lane")?;
    }
    validate_schema_root(&namespace)?;

    let receipt_path = namespace.join(format!("receipts/{action_key}.json"));
    if !receipt_path.is_file() {
        return Ok(None);
    }
    let root_lock = open_existing_lock(&cache_root.join("store.lock"))?;
    let store_lock = open_existing_lock(&namespace.join("locks/store.lock"))?;
    let action_lock = open_existing_lock(&namespace.join(format!("locks/{action_key}.lock")))?;
    root_lock.lock_shared().map_err(io_diag)?;
    store_lock.lock_shared().map_err(io_diag)?;
    action_lock.lock_shared().map_err(io_diag)?;
    let inspected = load_verified_pack(
        &namespace,
        &action_key,
        BUILD_FINGERPRINT,
        &source_digest,
        gts_bytes.len(),
    )
    .map(|entry| entry.map(|(receipt, _pack)| receipt));
    action_lock.unlock().map_err(io_diag)?;
    store_lock.unlock().map_err(io_diag)?;
    root_lock.unlock().map_err(io_diag)?;
    inspected
}

/// Load an already-produced graph-preserving import product without any fallback.
///
/// This is the test-facing consumer API. It opens only existing cache structures,
/// authenticates the action receipt and referenced pack, restores the indexed dataset,
/// and fails closed if the exact action is absent. It never creates a cache directory,
/// lock, receipt, or pack and never calls `purrdf::import_gts_events` or
/// [`PackBuilder::build_bytes`].
pub fn load_graph_preserving_cached(
    cache_root: &Path,
    gts_bytes: &[u8],
) -> gmeow_errors::Result<ImportOutcome> {
    let source_digest = ContentDigest::of(gts_bytes).to_hex();
    let selected = match (
        std::env::var_os(TEST_FIXTURE_MANIFEST_PATH_ENV),
        std::env::var_os(TEST_FIXTURE_MANIFEST_SHA256_ENV),
    ) {
        (None, None) => None,
        (Some(_), Some(_)) => {
            let selector = load_bundle_fixture_selector()?;
            (selector.receipt.source_digest == source_digest).then_some(selector)
        }
        _ => {
            return Err(diag(format!(
                "bundle import: {TEST_FIXTURE_MANIFEST_PATH_ENV} and {TEST_FIXTURE_MANIFEST_SHA256_ENV} must be configured together"
            )));
        }
    };
    if let Some(selector) = selected {
        return load_graph_preserving_selected(cache_root, gts_bytes, &selector);
    }
    load_graph_preserving_for_build(cache_root, gts_bytes, BUILD_FINGERPRINT, None)
}

fn load_graph_preserving_selected(
    cache_root: &Path,
    gts_bytes: &[u8],
    selector: &BundleFixtureSelector,
) -> gmeow_errors::Result<ImportOutcome> {
    validate_bundle_fixture_selector(selector)?;
    load_graph_preserving_for_build(
        cache_root,
        gts_bytes,
        &selector.receipt.build_fingerprint,
        Some(selector),
    )
}

fn load_graph_preserving_for_build(
    cache_root: &Path,
    gts_bytes: &[u8],
    build_fingerprint: &str,
    selector: Option<&BundleFixtureSelector>,
) -> gmeow_errors::Result<ImportOutcome> {
    ensure_real_directory(cache_root, "cache root")?;
    let sentinel = read_bounded(
        &cache_root.join(STORE_SENTINEL),
        u64::try_from(STORE_SENTINEL_BYTES.len()).unwrap_or(u64::MAX),
        "store sentinel",
    )?;
    if sentinel != STORE_SENTINEL_BYTES {
        return Err(diag("bundle import: store sentinel identity mismatch"));
    }

    let source_digest = ContentDigest::of(gts_bytes).to_hex();
    let action_key = digest(&[
        b"gmeow:bundle-import-action:v1",
        build_fingerprint.as_bytes(),
        CODEC.as_bytes(),
        source_digest.as_bytes(),
    ]);
    let build_root = cache_root.join(build_fingerprint);
    let namespace = build_root.join(format!("v{SCHEMA_VERSION}"));
    ensure_real_directory(&build_root, "build namespace")?;
    validate_build_root(&build_root)?;
    ensure_real_directory(&namespace, "schema namespace")?;
    for lane in ["receipts", "blobs", "locks"] {
        ensure_real_directory(&namespace.join(lane), "cache lane")?;
    }
    validate_schema_root(&namespace)?;

    let receipt_path = namespace.join(format!("receipts/{action_key}.json"));
    if !receipt_path.is_file() {
        return Err(diag(
            "authenticated bundle-import corpus fixture is absent; tests may not rebuild it",
        ));
    }
    let root_lock = open_existing_lock(&cache_root.join("store.lock"))?;
    let store_lock = open_existing_lock(&namespace.join("locks/store.lock"))?;
    let action_lock = open_existing_lock(&namespace.join(format!("locks/{action_key}.lock")))?;
    root_lock.lock_shared().map_err(io_diag)?;
    store_lock.lock_shared().map_err(io_diag)?;
    action_lock.lock_shared().map_err(io_diag)?;
    let loaded = load(
        &namespace,
        &action_key,
        build_fingerprint,
        &source_digest,
        gts_bytes.len(),
    );
    action_lock.unlock().map_err(io_diag)?;
    store_lock.unlock().map_err(io_diag)?;
    root_lock.unlock().map_err(io_diag)?;
    let outcome = loaded?.ok_or_else(|| {
        diag("authenticated bundle-import corpus fixture is absent; tests may not rebuild it")
    })?;
    if let Some(selector) = selector
        && (outcome.receipt != selector.receipt
            || outcome.receipt.receipt_digest() != selector.receipt_digest)
    {
        return Err(diag(
            "bundle import: authenticated import receipt differs from the producer selector",
        ));
    }
    Ok(outcome)
}

fn initialize_store_root(cache_root: &Path) -> gmeow_errors::Result<()> {
    for entry in fs::read_dir(cache_root).map_err(io_diag)? {
        let entry = entry.map_err(io_diag)?;
        let file_type = entry.file_type().map_err(io_diag)?;
        let filename = entry.file_name().to_string_lossy().into_owned();
        if filename == "store.lock" {
            if !file_type.is_file() || file_type.is_symlink() {
                return Err(diag(format!(
                    "bundle import: root lock is not a regular file: {}",
                    entry.path().display()
                )));
            }
            continue;
        }
        if filename == STORE_SENTINEL {
            if !file_type.is_file() || file_type.is_symlink() {
                return Err(diag(format!(
                    "bundle import: store sentinel is not a regular file: {}",
                    entry.path().display()
                )));
            }
            let bytes = read_bounded(
                &entry.path(),
                u64::try_from(STORE_SENTINEL_BYTES.len()).unwrap_or(u64::MAX),
                "store sentinel",
            )?;
            if bytes != STORE_SENTINEL_BYTES {
                return Err(diag(format!(
                    "bundle import: store sentinel identity mismatch at {}",
                    entry.path().display()
                )));
            }
            continue;
        }
        if file_type.is_dir() && !file_type.is_symlink() && is_namespace_name(&filename) {
            continue;
        }
        return Err(diag(format!(
            "bundle import: cache root contains an unrelated or unsafe entry {}; refusing quota GC",
            entry.path().display()
        )));
    }
    publish_identical(&cache_root.join(STORE_SENTINEL), STORE_SENTINEL_BYTES)
}

fn import_graph_preserving_under_root(
    cache_root: &Path,
    gts_bytes: &[u8],
) -> gmeow_errors::Result<ImportOutcome> {
    let source_digest = ContentDigest::of(gts_bytes).to_hex();
    let action_key = digest(&[
        b"gmeow:bundle-import-action:v1",
        BUILD_FINGERPRINT.as_bytes(),
        CODEC.as_bytes(),
        source_digest.as_bytes(),
    ]);
    let namespace = cache_root
        .join(BUILD_FINGERPRINT)
        .join(format!("v{SCHEMA_VERSION}"));
    let build_root = cache_root.join(BUILD_FINGERPRINT);
    fs::create_dir_all(&build_root).map_err(io_diag)?;
    ensure_real_directory(&build_root, "build namespace")?;
    validate_build_root(&build_root)?;
    fs::create_dir_all(&namespace).map_err(io_diag)?;
    ensure_real_directory(&namespace, "schema namespace")?;
    for directory in ["receipts", "blobs", "locks"] {
        let directory = namespace.join(directory);
        fs::create_dir_all(&directory).map_err(io_diag)?;
        ensure_real_directory(&directory, "cache lane")?;
    }
    validate_schema_root(&namespace)?;
    let store_lock = open_lock(&namespace.join("locks/store.lock"))?;
    let action_lock = open_lock(&namespace.join(format!("locks/{action_key}.lock")))?;

    store_lock.lock_shared().map_err(io_diag)?;
    action_lock.lock_shared().map_err(io_diag)?;
    if let Some(outcome) = load(
        &namespace,
        &action_key,
        BUILD_FINGERPRINT,
        &source_digest,
        gts_bytes.len(),
    )? {
        action_lock.unlock().map_err(io_diag)?;
        store_lock.unlock().map_err(io_diag)?;
        return Ok(outcome);
    }
    action_lock.unlock().map_err(io_diag)?;
    store_lock.unlock().map_err(io_diag)?;

    // Blocking election. The store stays shared so GC cannot remove anything while
    // this builder/rechecker is active; unrelated action keys can still proceed.
    store_lock.lock_shared().map_err(io_diag)?;
    action_lock.lock().map_err(io_diag)?;
    if let Some(outcome) = load(
        &namespace,
        &action_key,
        BUILD_FINGERPRINT,
        &source_digest,
        gts_bytes.len(),
    )? {
        action_lock.unlock().map_err(io_diag)?;
        store_lock.unlock().map_err(io_diag)?;
        return Ok(outcome);
    }

    let imported = purrdf::import_gts_events(gts_bytes)
        .map_err(|error| diag(format!("bundle import: decode GTS source: {error}")))?;
    let dataset = imported.dataset;
    let pack = PackBuilder::build_bytes(dataset.as_ref())
        .map_err(|error| diag(format!("bundle import: build PURRPCK1 image: {error}")))?;
    let pack_bytes = u64::try_from(pack.len()).unwrap_or(u64::MAX);
    if pack_bytes > MAX_PACK_BYTES {
        return Err(diag(format!(
            "bundle import: packed dataset is {pack_bytes} bytes, above the explicit \
             {MAX_PACK_BYTES}-byte admission bound"
        )));
    }
    let pack_digest = ContentDigest::of(&pack).to_hex();
    let named_graphs = u64::try_from(dataset.owned_named_graphs().count()).unwrap_or(u64::MAX);
    let receipt = ImportReceipt {
        schema_version: SCHEMA_VERSION,
        action_key: action_key.clone(),
        build_fingerprint: BUILD_FINGERPRINT.to_string(),
        codec: CODEC.to_string(),
        source_digest,
        source_bytes: u64::try_from(gts_bytes.len()).unwrap_or(u64::MAX),
        pack_digest: pack_digest.clone(),
        pack_bytes,
        dataset_quads: u64::try_from(dataset.quad_count()).unwrap_or(u64::MAX),
        named_graphs,
    };
    let envelope = ReceiptEnvelope {
        receipt_digest: receipt.receipt_digest(),
        receipt: receipt.clone(),
    };
    let receipt_bytes = serde_json::to_vec_pretty(&envelope)
        .map_err(|error| diag(format!("bundle import: encode receipt: {error}")))?;
    publish_identical(&namespace.join(format!("blobs/{pack_digest}")), &pack)?;
    publish_identical(
        &namespace.join(format!("receipts/{action_key}.json")),
        &receipt_bytes,
    )?;
    action_lock.unlock().map_err(io_diag)?;
    store_lock.unlock().map_err(io_diag)?;

    prune_namespace(&namespace, &action_key)?;
    Ok(ImportOutcome {
        dataset,
        receipt,
        built: true,
        transferred_bytes: pack_bytes,
    })
}

fn load(
    namespace: &Path,
    action_key: &str,
    build_fingerprint: &str,
    source_digest: &str,
    source_bytes: usize,
) -> gmeow_errors::Result<Option<ImportOutcome>> {
    let Some((receipt, pack)) = load_verified_pack(
        namespace,
        action_key,
        build_fingerprint,
        source_digest,
        source_bytes,
    )?
    else {
        return Ok(None);
    };
    let actual_bytes = u64::try_from(pack.len()).unwrap_or(u64::MAX);
    let dataset = restore_pack(&pack)
        .map_err(|error| diag(format!("bundle import: structurally invalid pack: {error}")))?;
    let quads = u64::try_from(dataset.quad_count()).unwrap_or(u64::MAX);
    let named_graphs = u64::try_from(dataset.owned_named_graphs().count()).unwrap_or(u64::MAX);
    if quads != receipt.dataset_quads || named_graphs != receipt.named_graphs {
        return Err(diag(format!(
            "bundle import: restored structure mismatch: expected quads/graphs {}/{}, \
             got {quads}/{named_graphs}",
            receipt.dataset_quads, receipt.named_graphs
        )));
    }
    Ok(Some(ImportOutcome {
        dataset,
        receipt,
        built: false,
        transferred_bytes: actual_bytes,
    }))
}

fn load_verified_pack(
    namespace: &Path,
    action_key: &str,
    build_fingerprint: &str,
    source_digest: &str,
    source_bytes: usize,
) -> gmeow_errors::Result<Option<(ImportReceipt, Vec<u8>)>> {
    let receipt_path = namespace.join(format!("receipts/{action_key}.json"));
    if !receipt_path.exists() {
        return Ok(None);
    }
    let bytes = read_bounded(&receipt_path, MAX_RECEIPT_BYTES, "receipt")?;
    let envelope: ReceiptEnvelope = serde_json::from_slice(&bytes)
        .map_err(|error| diag(format!("bundle import: corrupt receipt: {error}")))?;
    if envelope.receipt_digest != envelope.receipt.receipt_digest() {
        return Err(diag("bundle import: receipt envelope digest mismatch"));
    }
    let receipt = envelope.receipt;
    let expected_source_bytes = u64::try_from(source_bytes).unwrap_or(u64::MAX);
    if receipt.schema_version != SCHEMA_VERSION
        || receipt.action_key != action_key
        || receipt.build_fingerprint != build_fingerprint
        || receipt.codec != CODEC
        || receipt.source_digest != source_digest
        || receipt.source_bytes != expected_source_bytes
    {
        return Err(diag(
            "bundle import: receipt action/input identity mismatch",
        ));
    }
    let pack_path = namespace.join(format!("blobs/{}", receipt.pack_digest));
    if receipt.pack_bytes > MAX_PACK_BYTES {
        return Err(diag(format!(
            "bundle import: receipt declares {} pack bytes, above the explicit \
             {MAX_PACK_BYTES}-byte admission bound",
            receipt.pack_bytes
        )));
    }
    let pack = read_bounded(&pack_path, MAX_PACK_BYTES, "referenced pack")?;
    let actual_digest = ContentDigest::of(&pack).to_hex();
    let actual_bytes = u64::try_from(pack.len()).unwrap_or(u64::MAX);
    if actual_digest != receipt.pack_digest || actual_bytes != receipt.pack_bytes {
        return Err(diag(format!(
            "bundle import: pack digest/size mismatch: expected {}:{}, got \
             {actual_digest}:{actual_bytes}",
            receipt.pack_digest, receipt.pack_bytes
        )));
    }
    Ok(Some((receipt, pack)))
}

fn publish_identical(path: &Path, bytes: &[u8]) -> gmeow_errors::Result<()> {
    let expected_bytes = u64::try_from(bytes.len()).unwrap_or(u64::MAX);
    if path.exists() {
        let existing = read_bounded(path, expected_bytes, "existing publication")?;
        if existing == bytes {
            return Ok(());
        }
        return Err(diag(format!(
            "bundle import: same-key publication differs at {}",
            path.display()
        )));
    }
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    let temporary = path.with_extension(format!("{}.{}.tmp", std::process::id(), nonce));
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&temporary)
        .map_err(io_diag)?;
    file.write_all(bytes).map_err(io_diag)?;
    file.sync_all().map_err(io_diag)?;
    drop(file);

    // `rename(2)` replaces an existing destination on Unix. Different action keys
    // can legitimately converge on one content-addressed pack, so publish with an
    // atomic create-if-absent link instead. The loser verifies exact identity and
    // succeeds; it never overwrites bytes published by the winner.
    match fs::hard_link(&temporary, path) {
        Ok(()) => {
            fs::remove_file(&temporary).map_err(io_diag)?;
            if let Some(parent) = path.parent() {
                File::open(parent)
                    .and_then(|directory| directory.sync_all())
                    .map_err(io_diag)?;
            }
            Ok(())
        }
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            fs::remove_file(&temporary).map_err(io_diag)?;
            let existing = read_bounded(path, expected_bytes, "concurrent existing publication")?;
            if existing == bytes {
                Ok(())
            } else {
                Err(diag(format!(
                    "bundle import: concurrent same-key publication differs at {}",
                    path.display()
                )))
            }
        }
        Err(error) => {
            let _ = fs::remove_file(&temporary);
            Err(io_diag(error))
        }
    }
}

fn open_existing_lock(path: &Path) -> gmeow_errors::Result<File> {
    OpenOptions::new()
        .read(true)
        .write(true)
        .open(path)
        .map_err(|error| {
            diag(format!(
                "bundle import: required existing lock {} cannot be opened: {error}",
                path.display()
            ))
        })
}

fn prune_namespace(namespace: &Path, protected_action: &str) -> gmeow_errors::Result<()> {
    let store = open_lock(&namespace.join("locks/store.lock"))?;
    store.lock().map_err(io_diag)?;
    let mut receipts: Vec<(PathBuf, std::time::SystemTime, ReceiptEnvelope)> = Vec::new();
    for entry in fs::read_dir(namespace.join("receipts")).map_err(io_diag)? {
        let entry = entry.map_err(io_diag)?;
        let path = entry.path();
        let file_type = entry.file_type().map_err(io_diag)?;
        let filename = entry.file_name().to_string_lossy().into_owned();
        if !file_type.is_file() || file_type.is_symlink() {
            return Err(diag(format!(
                "bundle import: GC refuses non-regular receipt entry {}",
                path.display()
            )));
        }
        if filename.ends_with(".tmp") {
            continue;
        }
        let Some(filename_action) = filename.strip_suffix(".json") else {
            return Err(diag(format!(
                "bundle import: GC refuses unknown receipt entry {}",
                path.display()
            )));
        };
        if !is_digest(filename_action) {
            return Err(diag(format!(
                "bundle import: GC receipt name is not an action digest: {}",
                path.display()
            )));
        }
        let modified = entry
            .metadata()
            .and_then(|metadata| metadata.modified())
            .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
        let bytes = read_bounded(&path, MAX_RECEIPT_BYTES, "GC receipt root")?;
        let envelope: ReceiptEnvelope = serde_json::from_slice(&bytes)
            .map_err(|error| diag(format!("bundle import: corrupt GC root: {error}")))?;
        if envelope.receipt_digest != envelope.receipt.receipt_digest() {
            return Err(diag(format!(
                "bundle import: corrupt GC receipt envelope at {}",
                path.display()
            )));
        }
        if envelope.receipt.schema_version != SCHEMA_VERSION
            || envelope.receipt.build_fingerprint != BUILD_FINGERPRINT
            || envelope.receipt.codec != CODEC
            || envelope.receipt.action_key != filename_action
        {
            return Err(diag(format!(
                "bundle import: GC receipt identity mismatch at {}",
                path.display()
            )));
        }
        receipts.push((path, modified, envelope));
    }
    receipts.sort_by(|left, right| (&right.1, &right.0).cmp(&(&left.1, &left.0)));
    let protected_index = receipts
        .iter()
        .position(|(_, _, envelope)| envelope.receipt.action_key == protected_action)
        .ok_or_else(|| {
            diag(format!(
                "bundle import: protected action {protected_action} has no receipt root"
            ))
        })?;
    let mut retained_indexes = BTreeSet::new();
    retained_indexes.insert(protected_index);
    for index in 0..receipts.len() {
        if retained_indexes.len() == RETAINED_IMPORTS {
            break;
        }
        retained_indexes.insert(index);
    }
    let mut kept_blobs = BTreeSet::new();
    let mut kept_actions = BTreeSet::new();
    for (index, (path, _, envelope)) in receipts.into_iter().enumerate() {
        if retained_indexes.contains(&index) {
            kept_actions.insert(envelope.receipt.action_key);
            kept_blobs.insert(envelope.receipt.pack_digest);
        } else {
            fs::remove_file(path).map_err(io_diag)?;
        }
    }
    for entry in fs::read_dir(namespace.join("blobs")).map_err(io_diag)? {
        let entry = entry.map_err(io_diag)?;
        let path = entry.path();
        let file_type = entry.file_type().map_err(io_diag)?;
        let filename = entry.file_name().to_string_lossy().into_owned();
        if !file_type.is_file() || file_type.is_symlink() {
            return Err(diag(format!(
                "bundle import: GC refuses non-regular blob entry {}",
                path.display()
            )));
        }
        if filename.ends_with(".tmp") {
            continue;
        }
        if !is_digest(&filename) {
            return Err(diag(format!(
                "bundle import: GC blob name is not a content digest: {}",
                path.display()
            )));
        }
        if !kept_blobs.contains(&filename) {
            fs::remove_file(path).map_err(io_diag)?;
        }
    }
    remove_crash_leftovers(&namespace.join("receipts"))?;
    remove_crash_leftovers(&namespace.join("blobs"))?;
    for entry in fs::read_dir(namespace.join("locks")).map_err(io_diag)? {
        let entry = entry.map_err(io_diag)?;
        let path = entry.path();
        let file_type = entry.file_type().map_err(io_diag)?;
        let filename = entry.file_name().to_string_lossy().into_owned();
        if !file_type.is_file() || file_type.is_symlink() {
            return Err(diag(format!(
                "bundle import: GC refuses non-regular lock entry {}",
                path.display()
            )));
        }
        if filename == "store.lock" {
            continue;
        }
        let Some(action) = filename.strip_suffix(".lock") else {
            return Err(diag(format!(
                "bundle import: GC refuses unknown lock entry {}",
                path.display()
            )));
        };
        if !is_digest(action) {
            return Err(diag(format!(
                "bundle import: GC lock name is not an action digest: {}",
                path.display()
            )));
        }
        if !kept_actions.contains(action) {
            fs::remove_file(path).map_err(io_diag)?;
        }
    }
    store.unlock().map_err(io_diag)
}

fn remove_crash_leftovers(directory: &Path) -> gmeow_errors::Result<()> {
    for entry in fs::read_dir(directory).map_err(io_diag)? {
        let entry = entry.map_err(io_diag)?;
        let file_type = entry.file_type().map_err(io_diag)?;
        if !file_type.is_file() || file_type.is_symlink() {
            return Err(diag(format!(
                "bundle import: crash cleanup refuses non-regular entry {}",
                entry.path().display()
            )));
        }
        if entry.file_name().to_string_lossy().ends_with(".tmp") {
            fs::remove_file(entry.path()).map_err(io_diag)?;
        }
    }
    Ok(())
}

/// Enforce a store-wide quota after a successful publication. Every importer holds
/// the root lock shared while inspecting/building a namespace; GC takes it exclusive,
/// so removing an obsolete namespace cannot race a reader or writer.
fn prune_store(cache_root: &Path, protected_namespace: &str) -> gmeow_errors::Result<()> {
    prune_store_with_limits(
        cache_root,
        protected_namespace,
        RETAINED_NAMESPACES,
        MAX_STORE_BYTES,
    )
}

fn prune_store_with_limits(
    cache_root: &Path,
    protected_namespace: &str,
    retained_namespaces: usize,
    max_store_bytes: u64,
) -> gmeow_errors::Result<()> {
    let root_lock = open_lock(&cache_root.join("store.lock"))?;
    root_lock.lock().map_err(io_diag)?;
    let mut namespaces = Vec::new();
    let mut sentinel_seen = false;
    for entry in fs::read_dir(cache_root).map_err(io_diag)? {
        let entry = entry.map_err(io_diag)?;
        let file_type = entry.file_type().map_err(io_diag)?;
        if file_type.is_file() && entry.file_name() == "store.lock" {
            continue;
        }
        if file_type.is_file() && entry.file_name() == STORE_SENTINEL {
            let bytes = read_bounded(
                &entry.path(),
                u64::try_from(STORE_SENTINEL_BYTES.len()).unwrap_or(u64::MAX),
                "store sentinel",
            )?;
            if bytes != STORE_SENTINEL_BYTES {
                return Err(diag(format!(
                    "bundle import: store sentinel identity mismatch at {}",
                    entry.path().display()
                )));
            }
            sentinel_seen = true;
            continue;
        }
        if !file_type.is_dir() || file_type.is_symlink() {
            return Err(diag(format!(
                "bundle import: unexpected store entry {}",
                entry.path().display()
            )));
        }
        let filename = entry.file_name().to_string_lossy().into_owned();
        if !is_namespace_name(&filename) {
            return Err(diag(format!(
                "bundle import: unexpected non-namespace store directory {}",
                entry.path().display()
            )));
        }
        let (bytes, modified) = directory_census(&entry.path())?;
        namespaces.push((entry.path(), bytes, modified));
    }
    if !sentinel_seen {
        return Err(diag(format!(
            "bundle import: store sentinel is missing from {}; refusing quota GC",
            cache_root.display()
        )));
    }
    namespaces.sort_by(|left, right| (&right.2, &right.0).cmp(&(&left.2, &left.0)));

    let protected_path = cache_root.join(protected_namespace);
    let protected = namespaces
        .iter()
        .find(|(path, _, _)| *path == protected_path)
        .ok_or_else(|| {
            diag(format!(
                "bundle import: protected namespace {} disappeared before GC",
                protected_path.display()
            ))
        })?;
    if protected.1 > max_store_bytes || retained_namespaces == 0 {
        return Err(diag(format!(
            "bundle import: protected namespace requires {} bytes but store admits {} bytes across {} namespaces",
            protected.1, max_store_bytes, retained_namespaces
        )));
    }

    let mut retained = BTreeSet::from([protected_path]);
    let mut retained_bytes = protected.1;
    for (path, bytes, _) in &namespaces {
        if retained.contains(path) {
            continue;
        }
        if retained.len() < retained_namespaces
            && retained_bytes.saturating_add(*bytes) <= max_store_bytes
        {
            retained.insert(path.clone());
            retained_bytes = retained_bytes.saturating_add(*bytes);
        }
    }
    for (path, _, _) in namespaces {
        if !retained.contains(&path) {
            fs::remove_dir_all(path).map_err(io_diag)?;
        }
    }
    File::open(cache_root)
        .and_then(|directory| directory.sync_all())
        .map_err(io_diag)?;
    root_lock.unlock().map_err(io_diag)
}

fn directory_census(path: &Path) -> gmeow_errors::Result<(u64, std::time::SystemTime)> {
    let mut bytes = 0_u64;
    let mut modified = std::time::SystemTime::UNIX_EPOCH;
    for entry in fs::read_dir(path).map_err(io_diag)? {
        let entry = entry.map_err(io_diag)?;
        let file_type = entry.file_type().map_err(io_diag)?;
        if file_type.is_symlink() {
            return Err(diag(format!(
                "bundle import: store census refuses symlink {}",
                entry.path().display()
            )));
        }
        if file_type.is_dir() {
            let (child_bytes, child_modified) = directory_census(&entry.path())?;
            bytes = bytes
                .checked_add(child_bytes)
                .ok_or_else(|| diag("bundle import: store byte census overflowed its u64 bound"))?;
            modified = modified.max(child_modified);
        } else if file_type.is_file() {
            let metadata = entry.metadata().map_err(io_diag)?;
            bytes = bytes
                .checked_add(metadata.len())
                .ok_or_else(|| diag("bundle import: store byte census overflowed its u64 bound"))?;
            modified = modified.max(
                metadata
                    .modified()
                    .unwrap_or(std::time::SystemTime::UNIX_EPOCH),
            );
        } else {
            return Err(diag(format!(
                "bundle import: store census refuses special entry {}",
                entry.path().display()
            )));
        }
    }
    Ok((bytes, modified))
}

fn ensure_real_directory(path: &Path, lane: &str) -> gmeow_errors::Result<()> {
    let metadata = fs::symlink_metadata(path).map_err(io_diag)?;
    if !metadata.file_type().is_dir() || metadata.file_type().is_symlink() {
        return Err(diag(format!(
            "bundle import: {lane} is not a real directory: {}",
            path.display()
        )));
    }
    Ok(())
}

fn validate_build_root(build_root: &Path) -> gmeow_errors::Result<()> {
    for entry in fs::read_dir(build_root).map_err(io_diag)? {
        let entry = entry.map_err(io_diag)?;
        let file_type = entry.file_type().map_err(io_diag)?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        let known_schema = name.strip_prefix('v').is_some_and(|version| {
            !version.is_empty() && version.bytes().all(|byte| byte.is_ascii_digit())
        });
        if known_schema && file_type.is_dir() && !file_type.is_symlink() {
            continue;
        }
        return Err(diag(format!(
            "bundle import: build namespace contains an unrelated or unsafe entry {}",
            entry.path().display()
        )));
    }
    Ok(())
}

fn validate_schema_root(namespace: &Path) -> gmeow_errors::Result<()> {
    for entry in fs::read_dir(namespace).map_err(io_diag)? {
        let entry = entry.map_err(io_diag)?;
        let file_type = entry.file_type().map_err(io_diag)?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if matches!(name.as_ref(), "receipts" | "blobs" | "locks")
            && file_type.is_dir()
            && !file_type.is_symlink()
        {
            continue;
        }
        return Err(diag(format!(
            "bundle import: schema namespace contains an unrelated or unsafe entry {}",
            entry.path().display()
        )));
    }
    Ok(())
}

fn open_lock(path: &Path) -> gmeow_errors::Result<File> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if !metadata.file_type().is_file() || metadata.file_type().is_symlink() => {
            return Err(diag(format!(
                "bundle import: lock path is not a regular file: {}",
                path.display()
            )));
        }
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(io_diag(error)),
    }
    let file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(path)
        .map_err(io_diag)?;
    if !file.metadata().map_err(io_diag)?.is_file() {
        return Err(diag(format!(
            "bundle import: opened lock path is not a regular file: {}",
            path.display()
        )));
    }
    Ok(file)
}

fn read_bounded(path: &Path, max_bytes: u64, lane: &str) -> gmeow_errors::Result<Vec<u8>> {
    let metadata = fs::symlink_metadata(path).map_err(|error| {
        diag(format!(
            "bundle import: {lane} {} cannot be inspected: {error}",
            path.display()
        ))
    })?;
    if !metadata.file_type().is_file()
        || metadata.file_type().is_symlink()
        || metadata.len() > max_bytes
    {
        return Err(diag(format!(
            "bundle import: {lane} {} is not a regular file within the \
             {max_bytes}-byte bound",
            path.display()
        )));
    }
    let mut bytes = Vec::with_capacity(usize::try_from(metadata.len()).unwrap_or(0));
    File::open(path)
        .and_then(|file| {
            file.take(max_bytes.saturating_add(1))
                .read_to_end(&mut bytes)
        })
        .map_err(io_diag)?;
    if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > max_bytes {
        return Err(diag(format!(
            "bundle import: {lane} {} grew beyond the {max_bytes}-byte bound while being read",
            path.display()
        )));
    }
    Ok(bytes)
}

fn is_digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn is_namespace_name(value: &str) -> bool {
    (value.len() == 16 || value.len() == 64)
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn digest(fields: &[&[u8]]) -> String {
    let mut hash = Sha256::new();
    for field in fields {
        hash.update(field);
        hash.update([0x1f]);
    }
    hash.finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn io_diag(error: std::io::Error) -> gmeow_errors::Diag {
    diag(format!("bundle import I/O: {error}"))
}

fn diag(detail: impl Into<String>) -> gmeow_errors::Diag {
    gmeow_errors::Diag::of_kind(BundleImport {
        detail: detail.into(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_fingerprint_covers_transitive_path_dependencies() {
        let crate_root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let errors_root = crate_root
            .parent()
            .expect("bundle-import is below crates/")
            .join("errors");
        let closure = build_inputs::transitive_path_dependency_dirs(crate_root);
        assert!(
            closure.contains(&errors_root),
            "gmeow-errors must be in the bundle-import production dependency closure: {closure:?}"
        );
        let hashed = closure
            .iter()
            .flat_map(|crate_dir| build_inputs::crate_input_paths(crate_dir))
            .collect::<BTreeSet<_>>();
        assert!(
            hashed.contains(&errors_root.join("Cargo.toml"))
                && hashed
                    .iter()
                    .any(|path| path.starts_with(errors_root.join("src"))),
            "the derived fingerprint inputs must include the gmeow-errors manifest and sources"
        );
    }
    use gmeow_errors::intern_code;
    use std::collections::HashSet;

    fn tiny_gts() -> Vec<u8> {
        tiny_gts_with_object("o")
    }

    fn tiny_gts_with_object(object: &str) -> Vec<u8> {
        let dataset = purrdf::parse_dataset(
            format!(
                "<https://example.test/s> <https://example.test/p> \
                 <https://example.test/{object}> .\n"
            )
            .as_bytes(),
            "application/n-triples",
            None,
        )
        .expect("fixture dataset");
        // gmeow-test-input: synthetic-only
        gmeow_gts_profile::dataset_to_gmeow_gts(dataset.as_ref()).expect("fixture GTS")
    }

    /// Exercise the cache publisher only over a one-triple synthetic container.
    /// This is not a repository-corpus producer and can never resolve repository
    /// inputs, `generated/`, or the authenticated bundle selector.
    fn synthetic_import(root: &Path, bytes: &[u8]) -> gmeow_errors::Result<ImportOutcome> {
        import_graph_preserving_cached(root, bytes) // gmeow-test-input: synthetic-only
    }

    #[test]
    fn every_bundle_import_code_interns_with_no_collision() {
        let handles = register_all();
        assert_eq!(
            handles.len(),
            BUNDLE_IMPORT_DIAG_CODES.len(),
            "register_all() and BUNDLE_IMPORT_DIAG_CODES must enumerate the same kinds"
        );
        for code in BUNDLE_IMPORT_DIAG_CODES {
            assert!(
                intern_code(code).is_ok(),
                "bundle-import code `{code}` did not intern after register_all()"
            );
        }
        let distinct_strings: HashSet<&&str> = BUNDLE_IMPORT_DIAG_CODES.iter().collect();
        assert_eq!(distinct_strings.len(), BUNDLE_IMPORT_DIAG_CODES.len());
        let distinct_handles: HashSet<Code> = handles.iter().copied().collect();
        assert_eq!(distinct_handles.len(), handles.len());
    }

    #[test]
    fn cold_then_warm_import_is_structurally_identical() {
        let root = tempfile::tempdir().unwrap();
        let bytes = tiny_gts();
        let cold = synthetic_import(root.path(), &bytes).unwrap();
        let warm = synthetic_import(root.path(), &bytes).unwrap();
        assert!(cold.built);
        assert!(!warm.built);
        assert_eq!(cold.receipt, warm.receipt);
        assert_eq!(cold.dataset.quad_count(), warm.dataset.quad_count());
        assert_eq!(cold.transferred_bytes, warm.transferred_bytes);
    }

    #[test]
    fn referenced_tampered_pack_hard_fails() {
        let root = tempfile::tempdir().unwrap();
        let bytes = tiny_gts();
        let cold = synthetic_import(root.path(), &bytes).unwrap();
        let namespace = root
            .path()
            .join(BUILD_FINGERPRINT)
            .join(format!("v{SCHEMA_VERSION}"));
        fs::write(
            namespace.join(format!("blobs/{}", cold.receipt.pack_digest)),
            b"truncated",
        )
        .unwrap();
        let error =
            synthetic_import(root.path(), &bytes).expect_err("corruption cannot turn into a miss");
        assert!(error.to_string().contains("pack digest/size mismatch"));
    }

    #[test]
    fn referenced_missing_pack_hard_fails() {
        let root = tempfile::tempdir().unwrap();
        let bytes = tiny_gts();
        let cold = synthetic_import(root.path(), &bytes).unwrap();
        let namespace = root
            .path()
            .join(BUILD_FINGERPRINT)
            .join(format!("v{SCHEMA_VERSION}"));
        fs::remove_file(namespace.join(format!("blobs/{}", cold.receipt.pack_digest))).unwrap();
        let error = synthetic_import(root.path(), &bytes)
            .expect_err("a referenced missing pack cannot turn into a clean miss");
        assert!(
            error.to_string().contains("cannot be inspected"),
            "{error:?}"
        );
    }

    #[test]
    fn malformed_receipt_hard_fails() {
        let root = tempfile::tempdir().unwrap();
        let bytes = tiny_gts();
        let cold = synthetic_import(root.path(), &bytes).unwrap();
        let namespace = root
            .path()
            .join(BUILD_FINGERPRINT)
            .join(format!("v{SCHEMA_VERSION}"));
        fs::write(
            namespace.join(format!("receipts/{}.json", cold.receipt.action_key)),
            b"{not-json",
        )
        .unwrap();
        let error = synthetic_import(root.path(), &bytes)
            .expect_err("a malformed receipt cannot turn into a clean miss");
        assert!(error.to_string().contains("corrupt receipt"), "{error:?}");
    }

    #[test]
    fn structurally_invalid_digest_valid_pack_hard_fails() {
        let root = tempfile::tempdir().unwrap();
        let bytes = tiny_gts();
        let cold = synthetic_import(root.path(), &bytes).unwrap();
        let namespace = root
            .path()
            .join(BUILD_FINGERPRINT)
            .join(format!("v{SCHEMA_VERSION}"));
        let invalid_pack = b"PURRPCK1-invalid-structure";
        let invalid_digest = ContentDigest::of(invalid_pack).to_hex();
        fs::write(
            namespace.join(format!("blobs/{invalid_digest}")),
            invalid_pack,
        )
        .unwrap();
        let mut receipt = cold.receipt;
        receipt.pack_digest = invalid_digest;
        receipt.pack_bytes = u64::try_from(invalid_pack.len()).unwrap();
        let envelope = ReceiptEnvelope {
            receipt_digest: receipt.receipt_digest(),
            receipt,
        };
        fs::write(
            namespace.join(format!("receipts/{}.json", envelope.receipt.action_key)),
            serde_json::to_vec_pretty(&envelope).unwrap(),
        )
        .unwrap();
        let error = synthetic_import(root.path(), &bytes)
            .expect_err("a digest-valid but structurally invalid pack must fail closed");
        assert!(
            error.to_string().contains("structurally invalid pack"),
            "{error:?}"
        );
    }

    #[test]
    fn concurrent_import_elects_one_builder() {
        use std::sync::Barrier;

        let root = tempfile::tempdir().unwrap();
        let bytes = Arc::new(tiny_gts());
        let barrier = Arc::new(Barrier::new(2));
        let workers = (0..2)
            .map(|_| {
                let root = root.path().to_path_buf();
                let bytes = Arc::clone(&bytes);
                let barrier = Arc::clone(&barrier);
                std::thread::spawn(move || {
                    barrier.wait();
                    synthetic_import(&root, bytes.as_slice()).unwrap()
                })
            })
            .collect::<Vec<_>>();
        let outcomes = workers
            .into_iter()
            .map(|worker| worker.join().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(outcomes.iter().filter(|outcome| outcome.built).count(), 1);
        assert_eq!(outcomes[0].receipt, outcomes[1].receipt);
    }

    #[test]
    fn gc_retains_only_reachable_recent_imports() {
        let root = tempfile::tempdir().unwrap();
        for object in ["one", "two", "three"] {
            synthetic_import(root.path(), &tiny_gts_with_object(object)).unwrap();
        }
        let namespace = root
            .path()
            .join(BUILD_FINGERPRINT)
            .join(format!("v{SCHEMA_VERSION}"));
        let receipts = fs::read_dir(namespace.join("receipts"))
            .unwrap()
            .filter_map(Result::ok)
            .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "json"))
            .collect::<Vec<_>>();
        assert_eq!(receipts.len(), RETAINED_IMPORTS);
        let referenced = receipts
            .iter()
            .map(|entry| {
                let bytes = read_bounded(&entry.path(), MAX_RECEIPT_BYTES, "test receipt").unwrap();
                serde_json::from_slice::<ReceiptEnvelope>(&bytes)
                    .unwrap()
                    .receipt
                    .pack_digest
            })
            .collect::<BTreeSet<_>>();
        let blobs = fs::read_dir(namespace.join("blobs"))
            .unwrap()
            .filter_map(Result::ok)
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .collect::<BTreeSet<_>>();
        assert_eq!(
            blobs, referenced,
            "GC may keep only receipt-reachable packs"
        );
    }

    #[test]
    fn gc_removes_crash_leftovers_after_the_next_publication() {
        let root = tempfile::tempdir().unwrap();
        synthetic_import(root.path(), &tiny_gts()).unwrap();
        let namespace = root
            .path()
            .join(BUILD_FINGERPRINT)
            .join(format!("v{SCHEMA_VERSION}"));
        let abandoned_blob = namespace.join("blobs/abandoned.1.1.tmp");
        let abandoned_receipt = namespace.join("receipts/abandoned.json.1.1.tmp");
        fs::write(&abandoned_blob, b"partial").unwrap();
        fs::write(&abandoned_receipt, b"partial").unwrap();

        synthetic_import(root.path(), &tiny_gts_with_object("changed")).unwrap();
        assert!(!abandoned_blob.exists());
        assert!(!abandoned_receipt.exists());
    }

    #[test]
    fn store_gc_enforces_namespace_and_byte_quotas_while_protecting_current() {
        let root = tempfile::tempdir().unwrap();
        fs::write(root.path().join(STORE_SENTINEL), STORE_SENTINEL_BYTES).unwrap();
        for index in 0..6 {
            let namespace = root.path().join(format!("{index:016x}"));
            fs::create_dir(&namespace).unwrap();
            fs::write(namespace.join("payload"), b"four").unwrap();
        }
        let protected = "0000000000000000";
        prune_store_with_limits(root.path(), protected, 4, 8).unwrap();

        let retained = fs::read_dir(root.path())
            .unwrap()
            .map(Result::unwrap)
            .filter(|entry| entry.file_type().unwrap().is_dir())
            .map(|entry| entry.path())
            .collect::<Vec<_>>();
        assert!(retained.contains(&root.path().join(protected)));
        assert!(retained.len() <= 4);
        let retained_bytes = retained
            .iter()
            .map(|path| directory_census(path).unwrap().0)
            .sum::<u64>();
        assert!(retained_bytes <= 8);
    }

    #[test]
    fn unrelated_cache_root_is_refused_without_deleting_anything() {
        let root = tempfile::tempdir().unwrap();
        let unrelated = root.path().join("docs-fixture");
        fs::create_dir(&unrelated).unwrap();
        fs::write(unrelated.join("owned-by-another-cache"), b"preserve me").unwrap();

        let error = synthetic_import(root.path(), &tiny_gts())
            .expect_err("a broad or unrelated cache root must never become a GC authority");
        assert!(error.to_string().contains("refusing quota GC"), "{error:?}");
        assert_eq!(
            fs::read(unrelated.join("owned-by-another-cache")).unwrap(),
            b"preserve me"
        );
    }

    #[cfg(unix)]
    #[test]
    fn cache_root_and_internal_lanes_refuse_symlink_substitution() {
        use std::os::unix::fs::symlink;

        let parent = tempfile::tempdir().unwrap();
        let actual = parent.path().join("actual-cache");
        let selected = parent.path().join("selected-cache");
        fs::create_dir(&actual).unwrap();
        symlink(&actual, &selected).unwrap();
        let root_error = synthetic_import(&selected, &tiny_gts())
            .expect_err("a symlink cache root must never acquire cache or GC authority");
        assert!(
            root_error.to_string().contains("not a real directory"),
            "{root_error:?}"
        );

        let root = tempfile::tempdir().unwrap();
        synthetic_import(root.path(), &tiny_gts()).unwrap();
        let namespace = root
            .path()
            .join(BUILD_FINGERPRINT)
            .join(format!("v{SCHEMA_VERSION}"));
        fs::remove_dir_all(namespace.join("receipts")).unwrap();
        let outside = parent.path().join("outside-receipts");
        fs::create_dir(&outside).unwrap();
        symlink(&outside, namespace.join("receipts")).unwrap();
        let lane_error = synthetic_import(root.path(), &tiny_gts())
            .expect_err("a symlink cache lane must never be followed");
        assert!(
            lane_error.to_string().contains("not a real directory"),
            "{lane_error:?}"
        );
        assert!(fs::read_dir(&outside).unwrap().next().is_none());
    }

    #[test]
    fn warm_hit_still_enforces_the_store_wide_namespace_quota() {
        let root = tempfile::tempdir().unwrap();
        let bytes = tiny_gts();
        let cold = synthetic_import(root.path(), &bytes).unwrap();
        assert!(cold.built);
        for index in 0..6 {
            let obsolete = root.path().join(format!("{index:064x}"));
            if obsolete != root.path().join(BUILD_FINGERPRINT) {
                fs::create_dir(&obsolete).unwrap();
                fs::write(obsolete.join("payload"), b"obsolete").unwrap();
            }
        }

        let warm = synthetic_import(root.path(), &bytes).unwrap();
        assert!(!warm.built);
        let retained_namespaces = fs::read_dir(root.path())
            .unwrap()
            .map(Result::unwrap)
            .filter(|entry| entry.file_type().unwrap().is_dir())
            .count();
        assert!(retained_namespaces <= RETAINED_NAMESPACES);
        assert!(root.path().join(BUILD_FINGERPRINT).is_dir());
    }

    #[cfg(unix)]
    #[test]
    fn warm_hit_refuses_a_symlink_hidden_in_the_cache_store() {
        use std::os::unix::fs::symlink;

        let root = tempfile::tempdir().unwrap();
        let bytes = tiny_gts();
        synthetic_import(root.path(), &bytes).unwrap();
        let namespace = root
            .path()
            .join(BUILD_FINGERPRINT)
            .join(format!("v{SCHEMA_VERSION}"));
        symlink(
            namespace.join("receipts"),
            namespace.join("blobs/hidden-link"),
        )
        .unwrap();
        let error = synthetic_import(root.path(), &bytes)
            .expect_err("the root quota census must refuse cache symlinks even on a warm hit");
        assert!(error.to_string().contains("refuses symlink"), "{error:?}");
    }

    #[test]
    fn oversized_referenced_pack_is_rejected_before_hydration() {
        let root = tempfile::tempdir().unwrap();
        let bytes = tiny_gts();
        let cold = synthetic_import(root.path(), &bytes).unwrap();
        let namespace = root
            .path()
            .join(BUILD_FINGERPRINT)
            .join(format!("v{SCHEMA_VERSION}"));
        OpenOptions::new()
            .write(true)
            .open(namespace.join(format!("blobs/{}", cold.receipt.pack_digest)))
            .unwrap()
            .set_len(MAX_PACK_BYTES + 1)
            .unwrap();
        let error = synthetic_import(root.path(), &bytes)
            .expect_err("an oversized sparse cache pack must never be hydrated");
        assert!(error.to_string().contains("byte bound"), "{error:?}");
    }
}
