// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

const STAGE: &str = "synthetic-stage";
const ARTIFACT: &str = "observation.json";
const BYTES: &[u8] = b"tiny explicit source observation";

fn source() -> SourceArtifactOrigin {
    SourceArtifactOrigin {
        stage: STAGE.into(),
        action_key: "1".repeat(64),
        receipt_digest: "2".repeat(64),
        product_digest: "3".repeat(64),
        implementation: ProducerIdentity::new("synthetic-optimized-producer"),
    }
}

fn fixture() -> (tempfile::TempDir, ActionStore, SourceArtifactSelector) {
    let directory = tempfile::tempdir().unwrap();
    let store = ActionStore::open(
        ActionStore::default_root(directory.path()),
        STORE_FORMAT_VERSION,
        StoreLimits::default(),
    )
    .unwrap();
    let selected = publish(
        &store,
        source(),
        ARTIFACT,
        &bytes_digest(BYTES),
        BYTES.len() as u64,
        BYTES,
    )
    .unwrap();
    let mut selector = SourceArtifactSelector::default();
    selector
        .source_artifacts
        .entry(STAGE.into())
        .or_default()
        .insert(ARTIFACT.into(), selected);
    (directory, store, selector)
}

#[test]
fn selected_exports_require_exact_selector_membership_and_parent_commitments() {
    let (directory, store, selector) = fixture();
    let mut envelope = serde_json::to_value(&selector).unwrap();
    envelope["schema_version"] = super::super::MANIFEST_SCHEMA_VERSION.into();
    let selector_bytes = serde_json::to_vec(&envelope).unwrap();
    let path = directory.path().join("selector.json");
    std::fs::write(&path, &selector_bytes).unwrap();
    let selected: SourceArtifactSelector =
        super::super::read_manifest(&path, &bytes_digest(&selector_bytes)).unwrap();
    assert_eq!(
        load_selected(directory.path(), &selected, STAGE, ARTIFACT).unwrap(),
        BYTES
    );
    assert!(load_selected(directory.path(), &selected, "other-stage", ARTIFACT).is_err());
    assert!(load_selected(directory.path(), &selected, STAGE, "other.json").is_err());

    envelope["source_artifacts"][STAGE] = serde_json::json!({});
    let changed_bytes = serde_json::to_vec(&envelope).unwrap();
    std::fs::write(&path, &changed_bytes).unwrap();
    assert!(
        super::super::read_manifest::<SourceArtifactSelector>(
            &path,
            &bytes_digest(&selector_bytes)
        )
        .is_err()
    );
    let changed: SourceArtifactSelector =
        super::super::read_manifest(&path, &bytes_digest(&changed_bytes)).unwrap();
    assert!(load_selected(directory.path(), &changed, STAGE, ARTIFACT).is_err());

    let digest = bytes_digest(BYTES);
    assert!(
        publish(
            &store,
            source(),
            "bad.json",
            &digest,
            BYTES.len() as u64 + 1,
            BYTES
        )
        .is_err()
    );
    assert!(
        publish(
            &store,
            source(),
            "bad.json",
            &"0".repeat(64),
            BYTES.len() as u64,
            BYTES
        )
        .is_err()
    );
    assert!(
        publish(
            &store,
            source(),
            "bad.json",
            &digest,
            MAX_SOURCE_ARTIFACT_BYTES + 1,
            BYTES
        )
        .is_err()
    );
}

#[test]
fn source_identity_drift_cannot_reuse_or_rebuild_an_older_export() {
    let (directory, store, selector) = fixture();
    for mutation in 0..13 {
        let mut wrong = selector.clone();
        let item = wrong
            .source_artifacts
            .get_mut(STAGE)
            .unwrap()
            .get_mut(ARTIFACT)
            .unwrap();
        match mutation {
            0 => item.source.receipt_digest = "4".repeat(64),
            1 => item.source.action_key = "5".repeat(64),
            2 => item.source.implementation.profile = Some("different-consumer-profile".into()),
            3 => item.source.implementation.digest = "changed-source-code".into(),
            4 => item.source.implementation.toolchain = Some("different-toolchain".into()),
            5 => item.source.implementation.target = Some("different-target".into()),
            6 => item
                .source
                .implementation
                .features
                .push("different-feature".into()),
            7 => item.source.product_digest = "6".repeat(64),
            8 => item.source.stage = "different-stage".into(),
            9 => item.artifact = "different.json".into(),
            10 => item.digest = "7".repeat(64),
            11 => item.action.receipt_digest = "8".repeat(64),
            _ => item.bytes += 1,
        }
        assert!(
            load_selected(directory.path(), &wrong, STAGE, ARTIFACT).is_err(),
            "mutation {mutation} must fail"
        );
    }

    // Even a coherent selector for changed producer code cannot fall back to
    // the still-present old export, or publish the selected missing action.
    let mut next = selector.clone();
    let item = next
        .source_artifacts
        .get_mut(STAGE)
        .unwrap()
        .get_mut(ARTIFACT)
        .unwrap();
    item.source.implementation.digest = "next-source-code".into();
    item.action.context = Payload {
        source: item.source.clone(),
        artifact: item.artifact.clone(),
        digest: item.digest.clone(),
        bytes: item.bytes,
    }
    .context();
    let missing_receipt = store.receipt_path(&item.action.context.key());
    assert!(!missing_receipt.exists());
    assert!(
        load_selected(directory.path(), &next, STAGE, ARTIFACT)
            .unwrap_err()
            .to_string()
            .contains("action is missing")
    );
    assert!(!missing_receipt.exists());
    assert_eq!(
        load_selected(directory.path(), &selector, STAGE, ARTIFACT).unwrap(),
        BYTES
    );
}

#[test]
fn selected_exports_fail_closed_on_corrupt_or_missing_store_content() {
    let (directory, store, selector) = fixture();
    let selected = &selector.source_artifacts[STAGE][ARTIFACT];
    let entry = store
        .get::<Payload>(&selected.action.context)
        .unwrap()
        .unwrap();
    std::fs::write(
        store.blob_path(&entry.receipt.product_blob.digest),
        b"tampered",
    )
    .unwrap();
    assert!(load_selected(directory.path(), &selector, STAGE, ARTIFACT).is_err());
    std::fs::remove_file(store.blob_path(&entry.receipt.product_blob.digest)).unwrap();
    assert!(load_selected(directory.path(), &selector, STAGE, ARTIFACT).is_err());
    std::fs::remove_file(store.receipt_path(&selected.action.context.key())).unwrap();
    assert!(load_selected(directory.path(), &selector, STAGE, ARTIFACT).is_err());
}
