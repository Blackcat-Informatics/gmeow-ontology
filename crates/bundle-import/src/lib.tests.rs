// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

#[test]
fn build_fingerprint_covers_transitive_path_dependencies() {
    let crate_root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let errors_root = crate_root
        .parent()
        .expect("bundle-import is below crates/")
        .join("errors");
    let workspace = crate_root.parent().unwrap().parent().unwrap();
    let closure = gmeow_build_inputs::declared_path_dependency_manifests(
        workspace,
        "crates/bundle-import/Cargo.toml",
    )
    .expect("typed dependency declarations");
    assert!(
        closure.contains("crates/errors/Cargo.toml"),
        "gmeow-errors must be in the bundle-import production dependency closure: {closure:?}"
    );
    let manifest = std::fs::read_to_string(errors_root.join("Cargo.toml")).unwrap();
    assert!(manifest.contains("[lib]"));
    assert!(
        errors_root.join("src/lib.rs").is_file(),
        "the declared error-library root must exist for the selected inventory"
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
    {
        let emission = gmeow_gts_profile::view_to_gmeow_gts(dataset.as_ref()).expect("fixture GTS");
        assert!(
            emission.ingestion.declarations_omitted.is_empty(),
            "unexpected GMEOW fixture graph omissions: {:?}",
            emission.ingestion.declarations_omitted
        );
        emission.bytes
    }
}

/// Exercise the cache publisher only over a one-triple synthetic container.
/// This is not a repository-corpus producer and can never resolve repository
/// inputs, `generated/`, or the authenticated bundle selector.
fn synthetic_import(root: &Path, bytes: &[u8]) -> gmeow_errors::Result<ImportOutcome> {
    import_graph_preserving_cached(root, bytes) // gmeow-test-input: synthetic-only
}

#[test]
fn warm_artifact_admission_never_produces_and_corruption_is_terminal() {
    let root = tempfile::tempdir().unwrap();
    let cache = tempfile::tempdir().unwrap();
    let import = synthetic_import(cache.path(), &tiny_gts()).unwrap();
    let calls = std::cell::Cell::new(0);
    let admit = || {
        // gmeow-test-input: synthetic-only; one constant byte payload.
        admit_authenticated_corpus_artifact(
            root.path(),
            &import.receipt,
            &"a".repeat(64),
            "tiny.json",
            || {
                calls.set(calls.get() + 1);
                Ok(b"tiny product".to_vec())
            },
        )
    };
    let cold = admit().unwrap();
    let warm = admit().unwrap();
    assert!(cold.built);
    assert!(!warm.built);
    assert_eq!(cold.publication, warm.publication);
    assert_eq!(calls.get(), 1);
    let store = ActionStore::open(
        ActionStore::default_root(root.path()),
        STORE_FORMAT_VERSION,
        StoreLimits::default(),
    )
    .unwrap();
    fs::write(
        store.blob_path(&cold.publication.product_digest),
        b"corrupt",
    )
    .unwrap();
    assert!(
        admit().is_err(),
        "a referenced corrupt artifact must not be regenerated"
    );
    assert_eq!(
        calls.get(),
        1,
        "corruption must not invoke the producer callback"
    );
}

#[test]
fn import_admission_keeps_cold_dataset_and_leaves_warm_dataset_unrestored() {
    let root = tempfile::tempdir().unwrap();
    let bytes = tiny_gts();
    // gmeow-test-input: synthetic-only
    let cold = admit_graph_preserving_cached(root.path(), &bytes).unwrap();
    assert!(cold.built && cold.produced_dataset.is_some());
    // gmeow-test-input: synthetic-only
    let warm = admit_graph_preserving_cached(root.path(), &bytes).unwrap();
    assert!(!warm.built && warm.produced_dataset.is_none());
    assert_eq!(cold.receipt, warm.receipt);
}

#[test]
fn selected_blob_admission_shares_cold_dataset_and_warm_admission_only_authenticates() {
    let root = tempfile::tempdir().unwrap();
    let bytes = tiny_gts();
    let limits = purrdf::GtsBlobLimits::new(1024, 1024);
    // gmeow-test-input: synthetic-only; one-triple container, no archive content.
    let (cold, selected) =
        admit_graph_preserving_cached_with_blobs(root.path(), &bytes, &[], limits).unwrap();
    let selected = selected.expect("cold selected import retained");
    assert!(cold.built);
    assert!(Arc::ptr_eq(
        cold.produced_dataset.as_ref().unwrap(),
        &selected.bundle.dataset,
    ));

    // This deliberately absent selector would fail any native import. A
    // completed packed action must only authenticate here, leaving artifact
    // selection to a caller that actually has a missing artifact action.
    // gmeow-test-input: synthetic-only
    let (warm, selected) = admit_graph_preserving_cached_with_blobs(
        root.path(),
        &bytes,
        &[purrdf::GtsBlobSelector::Representation(
            "not-retained-on-a-hit",
        )],
        limits,
    )
    .unwrap();
    assert!(!warm.built && warm.produced_dataset.is_none() && selected.is_none());
    assert_eq!(cold.receipt, warm.receipt);

    // Exercise the shared locked recheck directly, bypassing the optimistic
    // outer inspection as if a sibling published after that inspection.
    // gmeow-test-input: synthetic-only
    let rechecked = import_graph_preserving_with(root.path(), &bytes, ImportRead::Receipt, |_| {
        panic!("a completed action must not call its native importer")
    })
    .unwrap();
    assert!(!rechecked.built && rechecked.produced_dataset.is_none());
    assert_eq!(cold.receipt, rechecked.receipt);
    let restored = load_graph_preserving_cached(root.path(), &bytes).unwrap();
    assert_eq!(cold.receipt, restored.receipt);
    assert_eq!(restored.dataset.quad_count(), 1);
}

#[test]
fn extraction_identity_changes_artifact_action_without_rebuilding_import() {
    let root = tempfile::tempdir().unwrap();
    let cache = tempfile::tempdir().unwrap();
    let import = synthetic_import(cache.path(), &tiny_gts()).unwrap();
    let produce = |fingerprint: &str, bytes: &[u8]| {
        // gmeow-test-input: synthetic-only; constant bytes and one-triple receipt.
        admit_authenticated_corpus_artifact(
            root.path(),
            &import.receipt,
            fingerprint,
            "tiny.json",
            || Ok(bytes.to_vec()),
        )
        .unwrap()
    };
    let first = produce(&"a".repeat(64), b"first extraction");
    let changed = produce(&"b".repeat(64), b"changed extraction");
    assert!(first.built && changed.built);
    assert_ne!(first.publication.action_key, changed.publication.action_key);
    assert_eq!(
        first.publication.build_fingerprint,
        changed.publication.build_fingerprint
    );
    assert_eq!(
        first.publication.source_sha256,
        changed.publication.source_sha256
    );
    assert!(!produce(&"a".repeat(64), b"must not execute").built);
    let mut forged = import.receipt.clone();
    forged.action_key = "0".repeat(64);
    // gmeow-test-input: synthetic-only
    assert!(
        admit_authenticated_corpus_artifact(
            root.path(),
            &forged,
            &"a".repeat(64),
            "tiny.json",
            || panic!("invalid import must fail before production")
        )
        .is_err()
    );
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
