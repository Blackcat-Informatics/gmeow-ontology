// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use crate::cache::{ReceiptOutputSelection, StageKeyContext};

// Synthetic receipt metadata only: these tests run no stage and produce no
// corpus or authenticated product. Blob authentication stays with the loader.
fn receipts() -> Vec<StageReceipt> {
    let spec = full_spec();
    let graph = spec.validate().unwrap();
    bind(&spec, &graph, &default_registry())
        .unwrap()
        .iter()
        .map(|stage| {
            let context = StageKeyContext::new(stage.id(), "metadata-fixture", vec![], vec![]);
            StageReceipt {
                schema_version: context.schema_version,
                action_key: stage_key(&context),
                context,
                stability: stage.stability().iri().into(),
                cache_disposition: stage.cache_policy().iri().into(),
                product_digest: "0".repeat(64),
                product_blob_digest: None,
                product_blob_bytes: 0,
                dataset_quads: 0,
                default_graph: None,
                provenance: None,
                content_store: None,
                graphs: vec![],
                blob_representations: vec![],
                logical_artifacts: vec![],
                typed_handles: vec![],
            }
        })
        .collect()
}

#[test]
fn full_sync_receipts_select_the_same_fixture_closure_as_the_prefix_producer() {
    let all = receipts();
    let full = stage_fixture_manifest(&all).unwrap();
    assert_eq!(full.stages.len(), AUTHENTICATED_TEST_STAGE_IDS.len());
    assert!(full.closure_receipts.len() > full.stages.len());
    assert!(!full.closure_receipts.contains_key("stage-gts-sink"));
    let prefix: Vec<_> = all
        .into_iter()
        .filter(|receipt| {
            full.closure_receipts
                .contains_key(&receipt.context.stage_id)
        })
        .collect();
    assert_eq!(
        serde_json::to_vec(&full).unwrap(),
        serde_json::to_vec(&stage_fixture_manifest(&prefix).unwrap()).unwrap(),
    );
}

#[test]
fn fixture_selector_refuses_missing_ancestors_and_mismatched_receipt_identity() {
    let all = receipts();
    for missing in ["stage-snapshot", "stage-mappings"] {
        let partial: Vec<_> = all
            .iter()
            .filter(|receipt| receipt.context.stage_id != missing)
            .cloned()
            .collect();
        let error = stage_fixture_manifest(&partial).unwrap_err();
        assert!(error.message().contains(missing), "{error}");
    }
    let mut duplicate = all.clone();
    duplicate.push(
        all.iter()
            .find(|receipt| receipt.context.stage_id == "stage-mappings")
            .unwrap()
            .clone(),
    );
    assert!(stage_fixture_manifest(&duplicate).is_err());
    let mut changed = all;
    changed
        .iter_mut()
        .find(|receipt| receipt.context.stage_id == "stage-mappings")
        .unwrap()
        .context
        .impl_version = "changed-after-execution".into();
    assert!(stage_fixture_manifest(&changed).is_err());
}

#[test]
fn candidate_replay_and_changes_never_replace_finalized_bindings() {
    let root = tempfile::tempdir().unwrap();
    let selected = root.path().join(STAGE_FIXTURE_MANIFEST_RELATIVE_PATH);
    std::fs::create_dir_all(selected.parent().unwrap()).unwrap();
    let original = br#"{"schema_version":2,"source_artifacts":{"old":"selected"},"docs":{"old":"selected"},"bundle_import":{"old":"selected"}}"#;
    std::fs::write(&selected, original).unwrap();
    let mut all = receipts();
    let first = prepare_stage_fixture_candidate(root.path(), &all).unwrap();
    assert_eq!(std::fs::read(&selected).unwrap(), original);
    let replay = prepare_stage_fixture_candidate(root.path(), &all).unwrap();
    assert_eq!(first, replay);
    let changed = all
        .iter_mut()
        .find(|receipt| receipt.context.stage_id == "stage-mappings")
        .unwrap();
    changed.context = changed
        .context
        .clone()
        .with_dimension("selected-source", "changed");
    changed.action_key = stage_key(&changed.context);
    let next = prepare_stage_fixture_candidate(root.path(), &all).unwrap();
    assert_ne!(first, next);
    assert!(next.get("docs").is_none());
    assert!(next.get("bundle_import").is_none());
    assert!(next.get("source_artifacts").is_none());
    assert_eq!(std::fs::read(&selected).unwrap(), original);
    let partial: Vec<_> = all
        .iter()
        .filter(|receipt| receipt.context.stage_id != "stage-mappings")
        .cloned()
        .collect();
    assert!(prepare_stage_fixture_candidate(root.path(), &partial).is_err());
    let duplicate = all
        .iter()
        .find(|receipt| receipt.context.stage_id == "stage-mappings")
        .unwrap()
        .clone();
    all.push(duplicate);
    assert!(prepare_stage_fixture_candidate(root.path(), &all).is_err());
    assert_eq!(std::fs::read(selected).unwrap(), original);
}

#[test]
fn source_export_cannot_substitute_another_selected_parent_or_artifact() {
    use gmeow_action_cache::selection::source_artifacts::{SourceArtifactOrigin, publish};
    let root = tempfile::tempdir().unwrap();
    let store = ActionStore::open(
        ActionStore::default_root(root.path()),
        STORE_FORMAT_VERSION,
        StoreLimits::default(),
    )
    .unwrap();
    let path = "tiny.json";
    let bytes = b"synthetic observation";
    let product = StageProduct::from_artifacts(
        "synthetic-stage",
        BTreeMap::from([(path.into(), bytes.to_vec())]),
    );
    let receipt = PipelineCache::receipt_only(
        &StageKeyContext::new("synthetic-stage", "synthetic-v1", Vec::new(), Vec::new()),
        StageStability::StablePrefix.iri(),
        CachePolicy::Persistent.iri(),
        &ReceiptOutputSelection {
            logical_artifacts: vec![path.to_owned()],
            ..ReceiptOutputSelection::default()
        },
        &product,
    )
    .unwrap();
    let entity = receipt
        .logical_artifacts
        .first()
        .expect("the selected synthetic artifact has a receipt commitment");
    let selection = publish(
        &store,
        SourceArtifactOrigin {
            stage: receipt.context.stage_id.clone(),
            action_key: receipt.action_key.clone(),
            receipt_digest: receipt.digest(),
            product_digest: receipt.product_digest.clone(),
            implementation: receipt.context.action_context().implementation,
        },
        path,
        &entity.digest,
        entity.decoded_bytes,
        bytes,
    )
    .unwrap();
    verify_source_artifact_origin(&receipt, path, &selection).unwrap();
    for mutate in [
        |value: &mut gmeow_action_cache::selection::source_artifacts::SelectedSourceArtifact| {
            value.source.receipt_digest = "0".repeat(64);
        },
        |value: &mut gmeow_action_cache::selection::source_artifacts::SelectedSourceArtifact| {
            value.source.product_digest = "0".repeat(64);
        },
        |value: &mut gmeow_action_cache::selection::source_artifacts::SelectedSourceArtifact| {
            value.digest = "0".repeat(64);
        },
        |value: &mut gmeow_action_cache::selection::source_artifacts::SelectedSourceArtifact| {
            value.bytes += 1;
        },
        |value: &mut gmeow_action_cache::selection::source_artifacts::SelectedSourceArtifact| {
            value.artifact = "other.json".into();
        },
    ] {
        let mut changed = selection.clone();
        mutate(&mut changed);
        assert!(verify_source_artifact_origin(&receipt, path, &changed).is_err());
    }
}
