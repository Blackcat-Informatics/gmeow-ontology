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
/// Every selected fixture must be readable across processes without a producer.
fn selected_fixture_stages_are_stable_persistent_contributions() {
    let spec = full_spec();
    let graph = spec.validate().unwrap();
    let stages = bind(&spec, &graph, &default_registry()).unwrap();
    for id in AUTHENTICATED_TEST_STAGE_IDS {
        let stage = stages.iter().find(|stage| stage.id() == *id).unwrap();
        assert_eq!(stage.stability(), StageStability::StablePrefix, "{id}");
        assert_eq!(stage.cache_policy(), CachePolicy::Persistent, "{id}");
    }
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
        purrdf_lex::json::record::to_vec(&full),
        purrdf_lex::json::record::to_vec(&stage_fixture_manifest(&prefix).unwrap()),
    );
}

#[test]
fn fixture_selector_refuses_missing_ancestors_and_mismatched_receipt_identity() {
    let all = receipts();
    // Browser bootstrap must finish before source-load's shipped-asset reconciliation.
    // Its genuine codebook receipt cannot grant authority to a full test prefix.
    let mut codebook = selected_fixture_manifest(&all, WASM_CODEBOOK_STAGE_IDS).unwrap();
    validate_wasm_codebook_manifest(&codebook).unwrap();
    assert_eq!(
        codebook
            .closure_receipts
            .keys()
            .map(String::as_str)
            .collect::<BTreeSet<_>>(),
        BTreeSet::from(["stage-parse-sources", "stage-native-codebook"]),
    );
    let bound = bound_selected_fixture_closure(&["stage-mappings"]).unwrap();
    assert!(!bound.required.contains("stage-native-codebook"));
    let bound = bound_selected_fixture_closure(&["stage-archive-blobs"]).unwrap();
    assert!(bound.required.contains("stage-native-codebook"));
    assert!(validate_stage_fixture_manifest(&codebook).is_err());
    assert!(validate_consumer_manifest(&codebook).is_err());
    let compiler = selected_fixture_manifest(&all, COMPILER_OWNER_STAGE_IDS).unwrap();
    validate_compiler_owner_manifest(&compiler).unwrap();
    validate_consumer_manifest(&compiler).unwrap();
    assert!(validate_stage_fixture_manifest(&compiler).is_err());
    assert!(!compiler.stages.contains_key("stage-reason"));
    let mut changed = selected_fixture_manifest(&all, &["stage-mappings"]).unwrap();
    assert!(validate_consumer_manifest(&changed).is_err());
    changed = selected_fixture_manifest(&all, COMPILER_OWNER_STAGE_IDS).unwrap();
    changed.closure_receipts.remove("stage-parse-sources");
    assert!(validate_consumer_manifest(&changed).is_err());
    changed = selected_fixture_manifest(&all, COMPILER_OWNER_STAGE_IDS).unwrap();
    changed.closure_receipts.insert(
        "stage-reason".into(),
        all.iter()
            .find(|row| row.context.stage_id == "stage-reason")
            .unwrap()
            .clone(),
    );
    assert!(validate_consumer_manifest(&changed).is_err());
    codebook.closure_receipts.remove("stage-parse-sources");
    assert!(validate_wasm_codebook_manifest(&codebook).is_err());
    let missing_codebook_ancestor: Vec<_> = all
        .iter()
        .filter(|receipt| receipt.context.stage_id != "stage-parse-sources")
        .cloned()
        .collect();
    assert!(
        selected_fixture_manifest(&missing_codebook_ancestor, WASM_CODEBOOK_STAGE_IDS).is_err()
    );
    for missing in ["stage-snapshot", "stage-mappings", "stage-native-codebook"] {
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
    // Preserve the complete historical source-bound candidate byte identity.
    let historical_bytes =
        purrdf_hash::hex::decode(include_str!("fixture_reference/candidate.hex").trim()).unwrap();
    let historical: StageFixtureManifest =
        purrdf_lex::json::record::from_slice(&historical_bytes).unwrap();
    let actual_sha = write_stage_fixture_record(
        root.path(),
        STAGE_FIXTURE_CANDIDATE_RELATIVE_PATH,
        &historical,
    )
    .unwrap();
    assert_eq!(
        actual_sha,
        "713daca122c6132f7863258b294a6f4651c14756c9ae1a1cd0da4a11f3024f1a"
    );
    assert_eq!(
        std::fs::read(root.path().join(STAGE_FIXTURE_CANDIDATE_RELATIVE_PATH)).unwrap(),
        historical_bytes
    );
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

/// Scoped verification must reject source/owner drift and missing committed output
/// while consuming only tiny synthetic cache products, never the authored corpus.
#[test]
fn compiler_owner_admission_binds_current_sources_and_required_output() {
    use crate::cache::ReceiptEntity;
    use crate::node::{OutputFamily, Stage, StageInput, StageOutput};

    struct Consumer {
        producers: Vec<String>,
        entities: Vec<(String, Vec<String>)>,
    }
    impl Stage for Consumer {
        fn id(&self) -> &str {
            "stage-compile-logic"
        }
        fn output_family(&self) -> OutputFamily {
            OutputFamily::Generated
        }
        fn consumes(&self) -> &[String] {
            &self.producers
        }
        fn consumed_entities(&self) -> &[(String, Vec<String>)] {
            &self.entities
        }
        fn impl_version(&self) -> &str {
            "synthetic-current-owner"
        }
        fn input_files(&self, root: &Path) -> gmeow_errors::Result<Vec<std::path::PathBuf>> {
            Ok(vec![root.join("owner.ttl")])
        }
        fn run(&self, _input: StageInput<'_>) -> gmeow_errors::Result<StageOutput> {
            panic!("fixture verification must never execute a stage")
        }
    }
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("owner.ttl");
    std::fs::write(&source, b"synthetic owner input").unwrap();
    let graph = crate::stages::parse_sources::GRAPH_SOURCE_CATALOG;
    let stage = Consumer {
        producers: vec!["stage-parse-sources".into()],
        entities: vec![("stage-parse-sources".into(), vec![graph.into()])],
    };
    let mut all = receipts();
    let parser = all
        .iter_mut()
        .find(|row| row.context.stage_id == "stage-parse-sources")
        .unwrap();
    parser.graphs.push(ReceiptEntity {
        identity: graph.into(),
        digest: "1".repeat(64),
        structural_count: 3,
        decoded_bytes: 0,
    });
    parser.typed_handles.push(ReceiptEntity {
        identity: format!("source-catalog#{graph}"),
        digest: "2".repeat(64),
        structural_count: 1,
        decoded_bytes: 64,
    });
    let upstream = BTreeMap::from([("stage-parse-sources".into(), parser.clone())]);
    let current = |upstream: &BTreeMap<String, StageReceipt>| {
        action_key_context_from_receipts_cached(&stage, root.path(), upstream, &mut BTreeMap::new())
            .unwrap()
    };
    let context = current(&upstream);
    let path = crate::stages::compile_logic::validation_fixtures::CHANNEL;
    let product = StageProduct::from_artifacts(
        stage.id(),
        BTreeMap::from([(path.into(), b"synthetic module output".to_vec())]),
    )
    .unwrap();
    let cache = PipelineCache::open_default(root.path()).unwrap();
    let selection = ReceiptOutputSelection {
        logical_artifacts: vec![path.into()],
        content_store: crate::cache::content_store_commitment(&product).unwrap(),
        ..ReceiptOutputSelection::default()
    };
    let pending = PipelineCache::receipt_only(
        &context,
        stage.stability().iri(),
        stage.cache_policy().iri(),
        &selection,
        &product,
    )
    .unwrap();
    assert_eq!(
        admit_current_fixture_receipt(&cache, &stage, &context, &pending).unwrap(),
        FixtureReceiptAdmission::CacheEntryAbsent,
    );
    let recorded = cache
        .put(
            &context,
            stage.stability().iri(),
            stage.cache_policy().iri(),
            &selection,
            &product,
        )
        .unwrap();
    *all.iter_mut()
        .find(|row| row.context.stage_id == stage.id())
        .unwrap() = recorded.clone();
    let manifest = selected_fixture_manifest(&all, COMPILER_OWNER_STAGE_IDS).unwrap();
    drop(cache);
    let cache = PipelineCache::open_existing_default_read_only(root.path()).unwrap();
    let parse_stage = crate::stages::parse_sources::ParseSourcesStage;
    let parser = &upstream["stage-parse-sources"];
    let mut unsupported_parser = parser.clone();
    unsupported_parser.schema_version += 1;
    assert_eq!(
        admit_current_fixture_receipt(&cache, &parse_stage, &parser.context, &unsupported_parser)
            .unwrap(),
        FixtureReceiptAdmission::ContextChanged,
    );
    assert_eq!(
        admit_current_fixture_receipt(&cache, &stage, &current(&upstream), &recorded).unwrap(),
        FixtureReceiptAdmission::Current,
    );
    verify_compiler_owner_artifact(root.path(), &manifest).unwrap();
    for handles in [false, true] {
        let mut changed = upstream.clone();
        let parser = changed.get_mut("stage-parse-sources").unwrap();
        if handles {
            parser.typed_handles.clear();
        } else {
            parser.graphs.clear();
        }
        let result = action_key_context_from_receipts_cached(
            &stage,
            root.path(),
            &changed,
            &mut BTreeMap::new(),
        );
        assert!(
            result.is_err()
                || admit_current_fixture_receipt(&cache, &stage, &result.unwrap(), &recorded)
                    .unwrap()
                    != FixtureReceiptAdmission::Current
        );
    }
    std::fs::write(&source, b"changed synthetic owner input").unwrap();
    assert_eq!(
        admit_current_fixture_receipt(&cache, &stage, &current(&upstream), &recorded).unwrap(),
        FixtureReceiptAdmission::ContextChanged,
    );
    let mut changed = manifest;
    changed
        .stages
        .get_mut(stage.id())
        .unwrap()
        .logical_artifacts
        .clear();
    assert!(verify_compiler_owner_artifact(root.path(), &changed).is_err());
    changed
        .stages
        .get_mut(stage.id())
        .unwrap()
        .logical_artifacts = recorded.logical_artifacts.clone();
    changed
        .stages
        .get_mut(stage.id())
        .unwrap()
        .logical_artifacts[0]
        .digest = "0".repeat(64);
    assert!(verify_compiler_owner_artifact(root.path(), &changed).is_err());
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
    )
    .expect("the stage carrier digests");
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
