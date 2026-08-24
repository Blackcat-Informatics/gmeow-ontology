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

const SCHEMA_VERSION: u32 = 1;
const CODEC: &str = "gts-events-to-purrpack1-graph-preserving-v1";
const MAX_PACK_BYTES: u64 = 512 * 1024 * 1024;
const MAX_RECEIPT_BYTES: u64 = 1024 * 1024;
const RETAINED_IMPORTS: usize = 2;

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
    fn digest(&self) -> String {
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
    let source_digest = ContentDigest::of(gts_bytes).to_hex();
    let action_key = digest(&[
        b"gmeow:bundle-import-action:v1",
        BUILD_FINGERPRINT.as_bytes(),
        CODEC.as_bytes(),
        source_digest.as_bytes(),
    ]);
    let namespace = cache_root
        .join(&BUILD_FINGERPRINT[..16])
        .join(format!("v{SCHEMA_VERSION}"));
    for directory in ["receipts", "blobs", "locks"] {
        fs::create_dir_all(namespace.join(directory)).map_err(io_diag)?;
    }
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
        receipt_digest: receipt.digest(),
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

    prune(&namespace, &action_key)?;
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
    if envelope.receipt_digest != envelope.receipt.digest() {
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

fn prune(namespace: &Path, protected_action: &str) -> gmeow_errors::Result<()> {
    let store = open_lock(&namespace.join("locks/store.lock"))?;
    store.lock().map_err(io_diag)?;
    let mut receipts: Vec<(PathBuf, std::time::SystemTime, ReceiptEnvelope)> =
        fs::read_dir(namespace.join("receipts"))
            .map_err(io_diag)?
            .filter_map(|entry| entry.ok())
            .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "json"))
            .map(|entry| {
                let path = entry.path();
                let modified = entry
                    .metadata()
                    .and_then(|metadata| metadata.modified())
                    .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
                let bytes = read_bounded(&path, MAX_RECEIPT_BYTES, "GC receipt root")?;
                let envelope: ReceiptEnvelope = serde_json::from_slice(&bytes)
                    .map_err(|error| diag(format!("bundle import: corrupt GC root: {error}")))?;
                if envelope.receipt_digest != envelope.receipt.digest() {
                    return Err(diag(format!(
                        "bundle import: corrupt GC receipt envelope at {}",
                        path.display()
                    )));
                }
                let filename_action = path
                    .file_stem()
                    .and_then(|stem| stem.to_str())
                    .unwrap_or_default();
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
                Ok((path, modified, envelope))
            })
            .collect::<gmeow_errors::Result<_>>()?;
    receipts.sort_by(|left, right| (&right.1, &right.0).cmp(&(&left.1, &left.0)));
    let protected_index = receipts
        .iter()
        .position(|(_, _, envelope)| envelope.receipt.action_key == protected_action);
    let mut retained_indexes = BTreeSet::new();
    if let Some(index) = protected_index {
        retained_indexes.insert(index);
    }
    for index in 0..receipts.len() {
        if retained_indexes.len() == RETAINED_IMPORTS {
            break;
        }
        retained_indexes.insert(index);
    }
    let mut kept = BTreeSet::new();
    for (index, (path, _, envelope)) in receipts.into_iter().enumerate() {
        if retained_indexes.contains(&index) {
            kept.insert(envelope.receipt.pack_digest);
        } else {
            fs::remove_file(path).map_err(io_diag)?;
        }
    }
    for entry in fs::read_dir(namespace.join("blobs")).map_err(io_diag)? {
        let entry = entry.map_err(io_diag)?;
        if entry.path().is_file() && !kept.contains(entry.file_name().to_string_lossy().as_ref()) {
            fs::remove_file(entry.path()).map_err(io_diag)?;
        }
    }
    store.unlock().map_err(io_diag)
}

fn open_lock(path: &Path) -> gmeow_errors::Result<File> {
    OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(path)
        .map_err(io_diag)
}

fn read_bounded(path: &Path, max_bytes: u64, lane: &str) -> gmeow_errors::Result<Vec<u8>> {
    let metadata = fs::metadata(path).map_err(|error| {
        diag(format!(
            "bundle import: {lane} {} cannot be inspected: {error}",
            path.display()
        ))
    })?;
    if !metadata.is_file() || metadata.len() > max_bytes {
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
    gmeow_errors::Diag::of_kind(crate::error::Store {
        detail: detail.into(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

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
            .join(&BUILD_FINGERPRINT[..16])
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
            .join(&BUILD_FINGERPRINT[..16])
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
            .join(&BUILD_FINGERPRINT[..16])
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
            .join(&BUILD_FINGERPRINT[..16])
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
            receipt_digest: receipt.digest(),
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
            .join(&BUILD_FINGERPRINT[..16])
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
    fn oversized_referenced_pack_is_rejected_before_hydration() {
        let root = tempfile::tempdir().unwrap();
        let bytes = tiny_gts();
        let cold = import_graph_preserving_cached(root.path(), &bytes).unwrap();
        let namespace = root
            .path()
            .join(&BUILD_FINGERPRINT[..16])
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
