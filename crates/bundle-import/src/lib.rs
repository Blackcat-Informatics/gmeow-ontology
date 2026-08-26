// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! A content-keyed, graph-preserving GTS -> indexed [`purrdf::RdfDataset`] product.
//!
//! Required repository commands and whole-bundle tests run in separate processes. This
//! boundary makes them share the expensive container decode/freeze/index construction:
//! a first process imports normally and publishes an immutable `PURRPCK1` image; later
//! processes verify its receipt/blob and restore the exact indexed dataset. Missing
//! material recomputes. A referenced missing/truncated/tampered receipt or pack hard
//! fails. Per-key OS election locks prevent duplicate builders, atomic rename prevents a
//! partial publication, and a store lease makes bounded GC safe against active readers.
//!
//! This cache never substitutes for the raw GTS frame/profile audit. Callers retain and
//! independently grade the original bytes where header/blob/compression semantics matter.

use std::collections::BTreeSet;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use purrdf::{ContentDigest, PackBuilder, RdfDataset, restore_pack};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use gmeow_errors::{Code, FindingCategory, Grade, Severity, Standpoint, define_diag_kind};

#[cfg(test)]
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
    if let Some(outcome) = load(&namespace, &action_key, &source_digest, gts_bytes.len())? {
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
    if let Some(outcome) = load(&namespace, &action_key, &source_digest, gts_bytes.len())? {
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
    source_digest: &str,
    source_bytes: usize,
) -> gmeow_errors::Result<Option<ImportOutcome>> {
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
        || receipt.build_fingerprint != BUILD_FINGERPRINT
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
        gmeow_gts_profile::dataset_to_gmeow_gts(dataset.as_ref()).expect("fixture GTS")
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
        let cold = import_graph_preserving_cached(root.path(), &bytes).unwrap();
        let warm = import_graph_preserving_cached(root.path(), &bytes).unwrap();
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
        let cold = import_graph_preserving_cached(root.path(), &bytes).unwrap();
        let namespace = root
            .path()
            .join(BUILD_FINGERPRINT)
            .join(format!("v{SCHEMA_VERSION}"));
        fs::write(
            namespace.join(format!("blobs/{}", cold.receipt.pack_digest)),
            b"truncated",
        )
        .unwrap();
        let error = import_graph_preserving_cached(root.path(), &bytes)
            .expect_err("corruption cannot turn into a miss");
        assert!(error.to_string().contains("pack digest/size mismatch"));
    }

    #[test]
    fn referenced_missing_pack_hard_fails() {
        let root = tempfile::tempdir().unwrap();
        let bytes = tiny_gts();
        let cold = import_graph_preserving_cached(root.path(), &bytes).unwrap();
        let namespace = root
            .path()
            .join(BUILD_FINGERPRINT)
            .join(format!("v{SCHEMA_VERSION}"));
        fs::remove_file(namespace.join(format!("blobs/{}", cold.receipt.pack_digest))).unwrap();
        let error = import_graph_preserving_cached(root.path(), &bytes)
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
        let cold = import_graph_preserving_cached(root.path(), &bytes).unwrap();
        let namespace = root
            .path()
            .join(BUILD_FINGERPRINT)
            .join(format!("v{SCHEMA_VERSION}"));
        fs::write(
            namespace.join(format!("receipts/{}.json", cold.receipt.action_key)),
            b"{not-json",
        )
        .unwrap();
        let error = import_graph_preserving_cached(root.path(), &bytes)
            .expect_err("a malformed receipt cannot turn into a clean miss");
        assert!(error.to_string().contains("corrupt receipt"), "{error:?}");
    }

    #[test]
    fn structurally_invalid_digest_valid_pack_hard_fails() {
        let root = tempfile::tempdir().unwrap();
        let bytes = tiny_gts();
        let cold = import_graph_preserving_cached(root.path(), &bytes).unwrap();
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
        let error = import_graph_preserving_cached(root.path(), &bytes)
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
                    import_graph_preserving_cached(&root, bytes.as_slice()).unwrap()
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
            import_graph_preserving_cached(root.path(), &tiny_gts_with_object(object)).unwrap();
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
        import_graph_preserving_cached(root.path(), &tiny_gts()).unwrap();
        let namespace = root
            .path()
            .join(BUILD_FINGERPRINT)
            .join(format!("v{SCHEMA_VERSION}"));
        let abandoned_blob = namespace.join("blobs/abandoned.1.1.tmp");
        let abandoned_receipt = namespace.join("receipts/abandoned.json.1.1.tmp");
        fs::write(&abandoned_blob, b"partial").unwrap();
        fs::write(&abandoned_receipt, b"partial").unwrap();

        import_graph_preserving_cached(root.path(), &tiny_gts_with_object("changed")).unwrap();
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

        let error = import_graph_preserving_cached(root.path(), &tiny_gts())
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
        let root_error = import_graph_preserving_cached(&selected, &tiny_gts())
            .expect_err("a symlink cache root must never acquire cache or GC authority");
        assert!(
            root_error.to_string().contains("not a real directory"),
            "{root_error:?}"
        );

        let root = tempfile::tempdir().unwrap();
        import_graph_preserving_cached(root.path(), &tiny_gts()).unwrap();
        let namespace = root
            .path()
            .join(BUILD_FINGERPRINT)
            .join(format!("v{SCHEMA_VERSION}"));
        fs::remove_dir_all(namespace.join("receipts")).unwrap();
        let outside = parent.path().join("outside-receipts");
        fs::create_dir(&outside).unwrap();
        symlink(&outside, namespace.join("receipts")).unwrap();
        let lane_error = import_graph_preserving_cached(root.path(), &tiny_gts())
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
        let cold = import_graph_preserving_cached(root.path(), &bytes).unwrap();
        assert!(cold.built);
        for index in 0..6 {
            let obsolete = root.path().join(format!("{index:064x}"));
            if obsolete != root.path().join(BUILD_FINGERPRINT) {
                fs::create_dir(&obsolete).unwrap();
                fs::write(obsolete.join("payload"), b"obsolete").unwrap();
            }
        }

        let warm = import_graph_preserving_cached(root.path(), &bytes).unwrap();
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
        import_graph_preserving_cached(root.path(), &bytes).unwrap();
        let namespace = root
            .path()
            .join(BUILD_FINGERPRINT)
            .join(format!("v{SCHEMA_VERSION}"));
        symlink(
            namespace.join("receipts"),
            namespace.join("blobs/hidden-link"),
        )
        .unwrap();
        let error = import_graph_preserving_cached(root.path(), &bytes)
            .expect_err("the root quota census must refuse cache symlinks even on a warm hit");
        assert!(error.to_string().contains("refuses symlink"), "{error:?}");
    }

    #[test]
    fn oversized_referenced_pack_is_rejected_before_hydration() {
        let root = tempfile::tempdir().unwrap();
        let bytes = tiny_gts();
        let cold = import_graph_preserving_cached(root.path(), &bytes).unwrap();
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
        let error = import_graph_preserving_cached(root.path(), &bytes)
            .expect_err("an oversized sparse cache pack must never be hydrated");
        assert!(error.to_string().contains("byte bound"), "{error:?}");
    }
}
