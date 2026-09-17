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
const CORPUS_ARTIFACT_CODEC: &str = "authenticated-corpus-artifact-v2";
const CORPUS_ARTIFACT_SCHEMA_VERSION: u32 = 2;
const TEST_FIXTURE_MANIFEST_PATH_ENV: &str = gmeow_action_cache::selection::MANIFEST_PATH_ENV;
const TEST_FIXTURE_MANIFEST_SHA256_ENV: &str = gmeow_action_cache::selection::MANIFEST_SHA256_ENV;
const BUNDLE_FIXTURE_SELECTOR_SCHEMA_VERSION: u32 = 1;

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
    /// Cold admissions retain the dataset they just produced for downstream
    /// artifact work. Warm admissions leave it absent and avoid hydration.
    pub produced_dataset: Option<Arc<RdfDataset>>,
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
    pub artifact_producer_fingerprint: String,
    pub name: String,
    pub source_sha256: String,
    pub product_digest: String,
    pub product_bytes: u64,
}

/// Producer admission of one exact bundle-derived artifact, without warm hydration.
#[derive(Debug)]
pub struct CorpusArtifactAdmission {
    pub publication: CorpusArtifactPublication,
    pub built: bool,
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
        if artifact.schema_version != CORPUS_ARTIFACT_SCHEMA_VERSION
            || artifact.name != *name
            || artifact.build_fingerprint != receipt.build_fingerprint
            || !is_digest(&artifact.artifact_producer_fingerprint)
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
        let context = corpus_artifact_context_for(
            &artifact.source_sha256,
            name,
            &artifact.build_fingerprint,
            &artifact.artifact_producer_fingerprint,
        );
        if artifact.action_key != context.key().as_str() {
            return Err(diag(format!(
                "bundle import: producer fixture selector action key is not context-derived for {name}"
            )));
        }
    }
    Ok(())
}

fn load_bundle_fixture_selector() -> gmeow_errors::Result<BundleFixtureSelector> {
    let root = std::env::current_dir().map_err(io_diag)?;
    let envelope: TestFixtureSelectorEnvelope = gmeow_action_cache::selection::load_manifest(&root)
        .map_err(|error| diag(format!("bundle import: {error}")))?;
    validate_bundle_fixture_selector(&envelope.bundle_import)?;
    Ok(envelope.bundle_import)
}

fn corpus_artifact_context_for(
    source_sha256: &str,
    name: &str,
    build_fingerprint: &str,
    artifact_producer_fingerprint: &str,
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
    .with_dimension("artifact-producer", artifact_producer_fingerprint)
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

/// Authenticate a bundle-derived artifact or elect one producer for a clean miss.
///
/// The already-admitted import binds the source and producer identities. Warm
/// artifact actions also bind the caller's authenticated executable recipe,
/// independently of the narrower import implementation identity. Changing an
/// extraction implementation invalidates artifacts without invalidating its pack.
/// admissions stream-authenticate the product and return only its receipt; they
/// never invoke `produce`, restore the dataset or allocate the artifact body.
/// Corrupt existing material fails closed. Test consumers must use
/// [`load_authenticated_corpus_artifact`] and cannot access this producer callback.
pub fn admit_authenticated_corpus_artifact(
    repo_root: &Path,
    import: &ImportReceipt,
    artifact_producer_fingerprint: &str,
    name: &str,
    produce: impl FnOnce() -> gmeow_errors::Result<Vec<u8>>,
) -> gmeow_errors::Result<CorpusArtifactAdmission> {
    validate_corpus_artifact_name(name)?;
    if import.schema_version != SCHEMA_VERSION
        || import.codec != CODEC
        || import.build_fingerprint != BUILD_FINGERPRINT
        || !is_digest(&import.source_digest)
        || !is_digest(artifact_producer_fingerprint)
        || !is_digest(&import.pack_digest)
        || import.pack_bytes > MAX_PACK_BYTES
        || import.action_key
            != digest(&[
                b"gmeow:bundle-import-action:v1",
                BUILD_FINGERPRINT.as_bytes(),
                CODEC.as_bytes(),
                import.source_digest.as_bytes(),
            ])
    {
        return Err(diag(
            "bundle artifact admission requires the current producer's exact import receipt",
        ));
    }
    let context = corpus_artifact_context_for(
        &import.source_digest,
        name,
        BUILD_FINGERPRINT,
        artifact_producer_fingerprint,
    );
    let payload = CorpusArtifactPayload {
        schema_version: CORPUS_ARTIFACT_SCHEMA_VERSION,
        name: name.to_owned(),
        source_sha256: import.source_digest.clone(),
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
    let admission = store
        .coordinate(
            &context.key(),
            || {
                let receipt = store.inspect::<CorpusArtifactPayload>(&context)?;
                if receipt.as_ref().is_some_and(|receipt| {
                    receipt.payload != payload
                        || receipt.product_digest != receipt.product_blob.digest
                }) {
                    return Err(gmeow_action_cache::ActionCacheError::message(
                        "bundle artifact payload identity mismatch",
                    ));
                }
                Ok(receipt)
            },
            || {
                let bytes = produce().map_err(|error| {
                    gmeow_action_cache::ActionCacheError::message(error.to_string())
                })?;
                store.publish(
                    &context,
                    ContentDigest::of(&bytes).to_hex(),
                    payload.clone(),
                    &bytes,
                )
            },
        )
        .map_err(|error: gmeow_action_cache::ActionCacheError| {
            diag(format!(
                "bundle import: admit corpus artifact {name}: {error}"
            ))
        })?;
    let receipt = admission.value;
    Ok(CorpusArtifactAdmission {
        built: admission.built,
        publication: CorpusArtifactPublication {
            schema_version: CORPUS_ARTIFACT_SCHEMA_VERSION,
            action_key: receipt.action_key.as_str().to_owned(),
            receipt_digest: receipt.digest(),
            build_fingerprint: BUILD_FINGERPRINT.to_owned(),
            artifact_producer_fingerprint: artifact_producer_fingerprint.to_owned(),
            name: name.to_owned(),
            source_sha256: import.source_digest.clone(),
            product_digest: receipt.product_digest,
            product_bytes: receipt.product_blob.bytes,
        },
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
    let context = corpus_artifact_context_for(
        &source_sha256,
        name,
        &selected.build_fingerprint,
        &selected.artifact_producer_fingerprint,
    );
    let expected_payload = CorpusArtifactPayload {
        schema_version: CORPUS_ARTIFACT_SCHEMA_VERSION,
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
    if entry.receipt.payload != expected_payload
        || entry.receipt.product_digest != entry.receipt.product_blob.digest
    {
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
    let admission = import_graph_preserving_with(
        cache_root,
        gts_bytes,
        ImportRead::Dataset,
        import_native_dataset,
    )?;
    Ok(ImportOutcome {
        dataset: admission.produced_dataset.ok_or_else(|| {
            diag("bundle import: dataset import completed without its requested dataset")
        })?,
        receipt: admission.receipt,
        built: admission.built,
        transferred_bytes: admission.transferred_bytes,
    })
}

#[derive(Clone, Copy)]
enum ImportRead {
    Dataset,
    Receipt,
}

fn import_native_dataset(gts_bytes: &[u8]) -> gmeow_errors::Result<Arc<RdfDataset>> {
    purrdf::import_gts_events(gts_bytes)
        .map(|imported| imported.dataset)
        .map_err(|error| diag(format!("bundle import: decode GTS source: {error}")))
}

/// All producer import variants share one election and publication path. Only
/// this module can supply an importer; public callers cannot substitute a dataset
/// unrelated to the source bytes authenticated by the receipt.
fn import_graph_preserving_with(
    cache_root: &Path,
    gts_bytes: &[u8],
    read: ImportRead,
    import: impl FnOnce(&[u8]) -> gmeow_errors::Result<Arc<RdfDataset>>,
) -> gmeow_errors::Result<ImportAdmission> {
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
    let outcome = import_graph_preserving_under_root(cache_root, gts_bytes, read, import)?;
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
/// A clean miss uses the shared native import election and produces the action
/// exactly once. A hit re-hashes the referenced pack and validates its
/// immutable receipt, returning only that identity. This is a producer API, not a test
/// fallback; test consumers remain read-only through [`load_graph_preserving_cached`].
pub fn admit_graph_preserving_cached(
    cache_root: &Path,
    gts_bytes: &[u8],
) -> gmeow_errors::Result<ImportAdmission> {
    admit_graph_preserving_with(cache_root, gts_bytes, import_native_dataset)
}

/// Admit the scoped native dataset and retain explicitly selected archive blobs
/// during the same cold import.
///
/// The existing packed action contains only the unchanged native dataset. On a
/// cold publication, the returned blob import shares that exact dataset allocation
/// with `ImportAdmission::produced_dataset`. A warm admission authenticates the
/// receipt and pack without importing, restoring or selecting any blobs, including
/// when another producer wins the action election. A caller with missing artifact
/// actions may subsequently initialize its own bounded selected import.
///
/// This is an explicit producer API; test readers retain their fail-closed load
/// path. The selectors and limits govern cold blob retention, not a different
/// dataset codec or a change to the packed action identity.
///
/// # Errors
/// Rejects the same corrupt import actions as [`admit_graph_preserving_cached`].
/// A cold import also rejects missing, ambiguous, corrupt or over-budget selected
/// blobs through PurRDF's authoritative selected-blob importer.
pub fn admit_graph_preserving_cached_with_blobs(
    cache_root: &Path,
    gts_bytes: &[u8],
    selectors: &[purrdf::GtsBlobSelector<'_>],
    limits: purrdf::GtsBlobLimits,
) -> gmeow_errors::Result<(ImportAdmission, Option<purrdf::GtsImportWithBlobs>)> {
    let mut selected = None;
    let admission = admit_graph_preserving_with(cache_root, gts_bytes, |bytes| {
        let imported =
            purrdf::import_gts_events_with_blobs(bytes, selectors, limits).map_err(|error| {
                diag(format!(
                    "bundle import: decode selected GTS source: {error}"
                ))
            })?;
        let dataset = Arc::clone(&imported.bundle.dataset);
        selected = Some(imported);
        Ok(dataset)
    })?;
    Ok((admission, selected))
}

fn admit_graph_preserving_with(
    cache_root: &Path,
    gts_bytes: &[u8],
    import: impl FnOnce(&[u8]) -> gmeow_errors::Result<Arc<RdfDataset>>,
) -> gmeow_errors::Result<ImportAdmission> {
    if let Some(receipt) = inspect_graph_preserving_cached(cache_root, gts_bytes)? {
        return Ok(ImportAdmission {
            transferred_bytes: receipt.pack_bytes,
            receipt,
            built: false,
            produced_dataset: None,
        });
    }
    import_graph_preserving_with(cache_root, gts_bytes, ImportRead::Receipt, import)
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
    let inspected = inspect_import(
        &namespace,
        &action_key,
        BUILD_FINGERPRINT,
        &source_digest,
        gts_bytes.len(),
    );
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
    read: ImportRead,
    import: impl FnOnce(&[u8]) -> gmeow_errors::Result<Arc<RdfDataset>>,
) -> gmeow_errors::Result<ImportAdmission> {
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
    if let Some(outcome) = read_import(
        &namespace,
        &action_key,
        BUILD_FINGERPRINT,
        &source_digest,
        gts_bytes.len(),
        read,
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
    if let Some(outcome) = read_import(
        &namespace,
        &action_key,
        BUILD_FINGERPRINT,
        &source_digest,
        gts_bytes.len(),
        read,
    )? {
        action_lock.unlock().map_err(io_diag)?;
        store_lock.unlock().map_err(io_diag)?;
        return Ok(outcome);
    }

    let dataset = import(gts_bytes)?;
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
    Ok(ImportAdmission {
        produced_dataset: Some(dataset),
        receipt,
        built: true,
        transferred_bytes: pack_bytes,
    })
}

/// Read a completed action under its caller-held locks. Receipt-only admission
/// takes this path again after election so a racing publisher cannot cause an
/// otherwise warm admission to restore the indexed dataset.
fn read_import(
    namespace: &Path,
    action_key: &str,
    build_fingerprint: &str,
    source_digest: &str,
    source_bytes: usize,
    read: ImportRead,
) -> gmeow_errors::Result<Option<ImportAdmission>> {
    match read {
        ImportRead::Dataset => load(
            namespace,
            action_key,
            build_fingerprint,
            source_digest,
            source_bytes,
        )
        .map(|outcome| {
            outcome.map(|outcome| ImportAdmission {
                receipt: outcome.receipt,
                built: outcome.built,
                transferred_bytes: outcome.transferred_bytes,
                produced_dataset: Some(outcome.dataset),
            })
        }),
        ImportRead::Receipt => inspect_import(
            namespace,
            action_key,
            build_fingerprint,
            source_digest,
            source_bytes,
        )
        .map(|receipt| {
            receipt.map(|receipt| ImportAdmission {
                transferred_bytes: receipt.pack_bytes,
                receipt,
                built: false,
                produced_dataset: None,
            })
        }),
    }
}

fn inspect_import(
    namespace: &Path,
    action_key: &str,
    build_fingerprint: &str,
    source_digest: &str,
    source_bytes: usize,
) -> gmeow_errors::Result<Option<ImportReceipt>> {
    let receipt = load_import_receipt(
        namespace,
        action_key,
        build_fingerprint,
        source_digest,
        source_bytes,
    )?;
    if let Some(receipt) = &receipt {
        let path = namespace.join(format!("blobs/{}", receipt.pack_digest));
        let (file, _) = open_bounded(&path, MAX_PACK_BYTES, "referenced pack")?;
        let mut reader = file.take(MAX_PACK_BYTES + 1);
        let mut buffer = [0_u8; 64 * 1024];
        let mut hash = Sha256::new();
        let mut bytes = 0_u64;
        loop {
            let count = reader.read(&mut buffer).map_err(io_diag)?;
            if count == 0 {
                break;
            }
            bytes += count as u64;
            hash.update(&buffer[..count]);
        }
        let digest = ContentDigest::from_raw(hash.finalize().into()).to_hex();
        if bytes != receipt.pack_bytes || digest != receipt.pack_digest {
            return Err(diag("bundle import: referenced pack digest/size mismatch"));
        }
    }
    Ok(receipt)
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

fn load_import_receipt(
    namespace: &Path,
    action_key: &str,
    build_fingerprint: &str,
    source_digest: &str,
    source_bytes: usize,
) -> gmeow_errors::Result<Option<ImportReceipt>> {
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
        || !is_digest(&receipt.pack_digest)
    {
        return Err(diag(
            "bundle import: receipt action/input identity mismatch",
        ));
    }
    if receipt.pack_bytes > MAX_PACK_BYTES {
        return Err(diag(format!(
            "bundle import: receipt declares {} pack bytes, above the explicit \
             {MAX_PACK_BYTES}-byte admission bound",
            receipt.pack_bytes
        )));
    }
    Ok(Some(receipt))
}

fn load_verified_pack(
    namespace: &Path,
    action_key: &str,
    build_fingerprint: &str,
    source_digest: &str,
    source_bytes: usize,
) -> gmeow_errors::Result<Option<(ImportReceipt, Vec<u8>)>> {
    let Some(receipt) = load_import_receipt(
        namespace,
        action_key,
        build_fingerprint,
        source_digest,
        source_bytes,
    )?
    else {
        return Ok(None);
    };
    let pack_path = namespace.join(format!("blobs/{}", receipt.pack_digest));
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

fn open_bounded(path: &Path, max_bytes: u64, lane: &str) -> gmeow_errors::Result<(File, usize)> {
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
    let file = File::open(path).map_err(io_diag)?;
    if !file.metadata().map_err(io_diag)?.is_file() {
        return Err(diag(format!(
            "bundle import: {lane} opened a non-regular file"
        )));
    }
    Ok((file, usize::try_from(metadata.len()).unwrap_or(0)))
}

fn read_bounded(path: &Path, max_bytes: u64, lane: &str) -> gmeow_errors::Result<Vec<u8>> {
    let (file, length) = open_bounded(path, max_bytes, lane)?;
    let mut bytes = Vec::with_capacity(length);
    file.take(max_bytes.saturating_add(1))
        .read_to_end(&mut bytes)
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

#[path = "lib.tests.rs"]
#[cfg(test)]
mod tests;
