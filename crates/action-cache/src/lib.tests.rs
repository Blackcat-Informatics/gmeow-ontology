// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct StructuredPayload {
    schema_version: u32,
    artifact: String,
    input_digest: String,
}

fn context(action: &str, inputs: Vec<ActionInput>) -> ActionContext {
    ActionContext::new(
        "test",
        action,
        ProducerIdentity::new("producer"),
        "json-v1",
        inputs,
    )
}

fn store(root: &Path) -> ActionStore {
    ActionStore::open(root, 1, StoreLimits::default()).unwrap()
}

#[test]
fn action_key_sorts_inputs_and_binds_dimensions() {
    let one = ActionInput::Raw {
        logical_path: "b".into(),
        file_kind: FileKind::File,
        executable: false,
        digest: "2".into(),
    };
    let two = ActionInput::Raw {
        logical_path: "a".into(),
        file_kind: FileKind::File,
        executable: false,
        digest: "1".into(),
    };
    assert_eq!(
        context("x", vec![one.clone(), two.clone()]).key(),
        context("x", vec![two, one]).key()
    );
    assert_ne!(
        context("x", vec![]).with_dimension("language", "en").key(),
        context("x", vec![]).with_dimension("language", "fr").key()
    );
}

#[test]
fn cold_publish_and_warm_read_share_one_receipt() {
    let temp = tempfile::tempdir().unwrap();
    let cache = store(temp.path());
    let context = context("round-trip", vec![]);
    let receipt = cache
        .publish(&context, "semantic", 7_u32, b"payload")
        .unwrap();
    let hit = cache.get::<u32>(&context).unwrap().unwrap();
    assert_eq!(hit.receipt, receipt);
    assert_eq!(hit.bytes, b"payload");
}

#[test]
fn existing_open_modes_separate_read_only_consumers_from_writable_workers() {
    let absent_parent = tempfile::tempdir().unwrap();
    let absent = absent_parent.path().join("absent");
    assert!(ActionStore::open_existing_read_only(&absent, 1, StoreLimits::default()).is_err());
    assert!(
        !absent.exists(),
        "a read-only miss must not initialize the cache root"
    );

    let temp = tempfile::tempdir().unwrap();
    let producer = store(temp.path()); // gmeow-test-input: synthetic-only
    let present = context("present", vec![]);
    producer
        .publish(&present, "semantic", 7_u32, b"payload")
        .unwrap();
    drop(producer);

    let consumer =
        ActionStore::open_existing_read_only(temp.path(), 1, StoreLimits::default()).unwrap();
    let hit = consumer.get::<u32>(&present).unwrap().unwrap();
    assert_eq!(hit.bytes, b"payload");
    assert!(
        consumer
            .publish(&present, "semantic", 7_u32, b"payload")
            .is_err(),
        "a read-only handle must reject publication even when bytes agree"
    );

    let missing = context("read-only-miss", vec![]);
    let stripe = missing.key().as_str()[..2].to_string();
    let lock_path = consumer
        .root()
        .join("locks")
        .join(format!("action-{stripe}.lock"));
    let existed_before = lock_path.exists();
    assert!(consumer.get::<u32>(&missing).unwrap().is_none());
    assert_eq!(
        lock_path.exists(),
        existed_before,
        "a read-only miss must not create an action lock"
    );
    let error = match consumer.coordinate::<_, ActionCacheError, _, _>(
        &missing.key(),
        || Ok(None),
        || Ok(9_u32),
    ) {
        Err(error) => error,
        Ok(_) => panic!("a read-only miss must never execute its build callback"),
    };
    assert!(error.to_string().contains("read-only action cache"));

    drop(consumer);
    let worker =
        ActionStore::open_existing_writable(temp.path(), 1, StoreLimits::default()).unwrap();
    worker
        .publish(&missing, "worker-semantic", 9_u32, b"worker-payload")
        .unwrap();
    assert_eq!(
        worker.get::<u32>(&missing).unwrap().unwrap().bytes,
        b"worker-payload"
    );
}

#[test]
fn structured_payload_receipt_is_canonical_across_generic_gc_reads() {
    let temp = tempfile::tempdir().unwrap();
    let cache = store(temp.path());
    let context = context("structured", vec![]);
    let payload = StructuredPayload {
        schema_version: 1,
        artifact: "site".to_string(),
        input_digest: "input".to_string(),
    };
    let receipt = cache
        .publish(&context, "semantic", payload.clone(), b"payload")
        .unwrap();
    let hit = cache
        .get::<StructuredPayload>(&context)
        .unwrap()
        .expect("structured receipt remains valid after publish-time GC");
    assert_eq!(hit.receipt, receipt);
    assert_eq!(hit.receipt.payload, payload);
}

#[test]
fn same_key_divergence_and_tampered_blob_hard_fail() {
    let temp = tempfile::tempdir().unwrap();
    let cache = store(temp.path());
    let context = context("integrity", vec![]);
    let receipt = cache.publish(&context, "one", 1_u32, b"one").unwrap();
    assert!(cache.publish(&context, "two", 2_u32, b"two").is_err());
    fs::write(cache.blob_path(&receipt.product_blob.digest), b"tampered").unwrap();
    assert!(cache.get::<u32>(&context).is_err());
}

#[test]
fn malformed_receipt_missing_blob_and_wrong_root_hard_fail() {
    let temp = tempfile::tempdir().unwrap();
    let cache = store(temp.path());
    let original = context("original", vec![]);
    let receipt = cache
        .publish(&original, "semantic", 1_u32, b"payload")
        .unwrap();

    fs::remove_file(cache.blob_path(&receipt.product_blob.digest)).unwrap();
    assert!(cache.get::<u32>(&original).is_err());

    let truncated_root = tempfile::tempdir().unwrap();
    let truncated = store(truncated_root.path());
    let truncated_context = context("truncated", vec![]);
    truncated
        .publish(&truncated_context, "semantic", 1_u32, b"payload")
        .unwrap();
    fs::write(truncated.receipt_path(&truncated_context.key()), b"{").unwrap();
    assert!(truncated.get::<u32>(&truncated_context).is_err());

    let wrong_root = tempfile::tempdir().unwrap();
    let wrong = store(wrong_root.path());
    let source = context("source", vec![]);
    wrong
        .publish(&source, "semantic", 1_u32, b"payload")
        .unwrap();
    let target = context("target", vec![]);
    fs::rename(
        wrong.receipt_path(&source.key()),
        wrong.receipt_path(&target.key()),
    )
    .unwrap();
    assert!(wrong.get::<u32>(&target).is_err());
}

#[test]
fn election_rechecks_before_building() {
    let temp = tempfile::tempdir().unwrap();
    let cache = store(temp.path());
    let context = context("election", vec![]);
    let key = context.key();
    let result = cache
        .coordinate(
            &key,
            || {
                cache
                    .get::<u32>(&context)
                    .map(|entry| entry.map(|entry| entry.payload()))
            },
            || {
                cache.publish(&context, "value", 9_u32, b"nine")?;
                Ok(9)
            },
        )
        .unwrap();
    assert!(result.built);
    let warm = cache
        .coordinate(
            &key,
            || {
                cache
                    .get::<u32>(&context)
                    .map(|entry| entry.map(|entry| entry.payload()))
            },
            || panic!("warm coordination must not rebuild"),
        )
        .unwrap();
    assert!(!warm.built);
    assert_eq!(warm.value, 9);
    let elections = fs::read_dir(cache.root().join("elections"))
        .unwrap()
        .map(Result::unwrap)
        .collect::<Vec<_>>();
    assert_eq!(elections.len(), 1);
    assert!(
        elections[0]
            .file_name()
            .to_string_lossy()
            .starts_with("action-")
    );
}

#[test]
fn inert_coordination_builds_without_touching_the_filesystem() {
    let cache = ActionStore::inert();
    let key = context("inert", vec![]).key();
    let result = cache
        .coordinate::<_, ActionCacheError, _, _>(&key, || Ok(None), || Ok(11_u32))
        .unwrap();
    assert!(result.built);
    assert_eq!(result.value, 11);
}

#[test]
fn unrelated_store_root_is_refused_without_deleting_anything() {
    let temp = tempfile::tempdir().unwrap();
    let unrelated = temp.path().join("owned-by-someone-else");
    fs::write(&unrelated, b"preserve me").unwrap();
    let error = ActionStore::open(temp.path(), STORE_FORMAT_VERSION, StoreLimits::default())
        .err()
        .expect("a broad unrelated root must not become a cache authority");
    assert!(error.to_string().contains("unrelated or unsafe entry"));
    assert_eq!(fs::read(unrelated).unwrap(), b"preserve me");
}

#[cfg(unix)]
#[test]
fn symlinked_blob_hard_fails_even_when_target_bytes_match() {
    use std::os::unix::fs::symlink;

    let temp = tempfile::tempdir().unwrap();
    let cache = store(temp.path());
    let context = context("symlink", vec![]);
    let receipt = cache
        .publish(&context, "semantic", 7_u32, b"payload")
        .unwrap();
    let external = temp.path().join("matching-external-bytes");
    fs::write(&external, b"payload").unwrap();
    let blob = cache.blob_path(&receipt.product_blob.digest);
    fs::remove_file(&blob).unwrap();
    symlink(&external, &blob).unwrap();
    assert!(cache.get::<u32>(&context).is_err());
}

#[test]
fn quota_gc_waits_for_live_reader_and_collects_only_unreachable_blobs() {
    use std::sync::mpsc;
    use std::time::Duration;

    let temp = tempfile::tempdir().unwrap();
    let limits = StoreLimits {
        max_entry_bytes: 1024,
        max_receipt_bytes: 4096,
        max_entries: 1,
        max_total_bytes: 1024,
    };
    let cache = ActionStore::open(temp.path(), STORE_FORMAT_VERSION, limits).unwrap();
    let first = context("first", vec![]);
    let second = context("second", vec![]);
    let first_receipt = cache.publish(&first, "first", 1_u32, b"first").unwrap();

    // Hold the same shared store lock a reader owns from receipt read through
    // blob verification. Publication can finish its immutable write, but quota GC
    // cannot delete any root/blob until this reader releases the lock.
    let reader = cache.lock_store(false).unwrap();
    let root = temp.path().to_path_buf();
    let (sent, received) = mpsc::channel();
    let worker = std::thread::spawn(move || {
        let cache = ActionStore::open(root, STORE_FORMAT_VERSION, limits).unwrap();
        cache.publish(&second, "second", 2_u32, b"second").unwrap();
        sent.send(()).unwrap();
    });
    assert!(
        received.recv_timeout(Duration::from_millis(50)).is_err(),
        "GC must wait while a live reader holds the shared store lease"
    );
    assert!(cache.receipt_path(&first_receipt.action_key).is_file());
    drop(reader);
    received.recv_timeout(Duration::from_secs(2)).unwrap();
    worker.join().unwrap();

    assert!(cache.get::<u32>(&first).unwrap().is_none());
    assert!(
        cache
            .get::<u32>(&context("second", vec![]))
            .unwrap()
            .is_some()
    );
    assert_eq!(
        fs::read_dir(cache.root().join("blobs"))
            .unwrap()
            .filter_map(Result::ok)
            .filter(|entry| entry.path().is_file())
            .count(),
        1,
    );
}

#[test]
fn total_quota_counts_receipts_as_well_as_unique_blobs() {
    let temp = tempfile::tempdir().unwrap();
    let first = context("first", vec![]);
    let initial = store(temp.path());
    let first_receipt = initial.publish(&first, "value", 1_u32, b"aaaa").unwrap();
    let first_total = fs::metadata(initial.receipt_path(&first_receipt.action_key))
        .unwrap()
        .len()
        + fs::metadata(initial.blob_path(&first_receipt.product_blob.digest))
            .unwrap()
            .len();
    drop(initial);

    let limits = StoreLimits {
        max_entry_bytes: 4,
        max_receipt_bytes: 4096,
        max_entries: 10,
        max_total_bytes: first_total * 2 - 1,
    };
    let bounded = ActionStore::open(temp.path(), STORE_FORMAT_VERSION, limits).unwrap();
    let second = context("other", vec![]);
    bounded.publish(&second, "value", 1_u32, b"bbbb").unwrap();
    assert!(bounded.get::<u32>(&first).unwrap().is_none());
    assert!(bounded.get::<u32>(&second).unwrap().is_some());
}

#[test]
fn opening_an_outer_restored_store_enforces_the_current_entry_quota() {
    let temp = tempfile::tempdir().unwrap();
    {
        let cache = store(temp.path());
        cache
            .publish(&context("first", vec![]), "first", 1_u32, b"first")
            .unwrap();
        cache
            .publish(&context("second", vec![]), "second", 2_u32, b"second")
            .unwrap();
        assert_eq!(cache.len(), 2);
    }
    let bounded = ActionStore::open(
        temp.path(),
        STORE_FORMAT_VERSION,
        StoreLimits {
            max_entries: 1,
            ..StoreLimits::default()
        },
    )
    .unwrap();
    assert_eq!(bounded.len(), 1);
}

#[test]
fn obsolete_format_roots_are_collected_only_after_live_leases_end() {
    let temp = tempfile::tempdir().unwrap();
    let first = ActionStore::open(temp.path(), 1, StoreLimits::default()).unwrap();
    first
        .publish(&context("old", vec![]), "old", 1_u32, b"old")
        .unwrap();

    let second = ActionStore::open(temp.path(), 2, StoreLimits::default()).unwrap();
    assert!(temp.path().join("v1").is_dir());
    assert!(temp.path().join("v2").is_dir());
    drop(first);

    let second_reader = ActionStore::open(temp.path(), 2, StoreLimits::default()).unwrap();
    assert!(!temp.path().join("v1").exists());
    assert!(temp.path().join("v2").is_dir());
    drop(second_reader);
    drop(second);
}

#[test]
fn zero_or_inverted_store_limits_are_rejected() {
    let temp = tempfile::tempdir().unwrap();
    assert!(
        ActionStore::open(
            temp.path(),
            STORE_FORMAT_VERSION,
            StoreLimits {
                max_entry_bytes: 2,
                max_receipt_bytes: 1,
                max_entries: 1,
                max_total_bytes: 1,
            },
        )
        .is_err()
    );
}
