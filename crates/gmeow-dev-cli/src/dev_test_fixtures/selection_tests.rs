// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Metadata-only publication controls; no fixture or corpus is constructed.

use super::*;
use gmeow_action_cache::{ActionContext, ProducerIdentity, selection::SelectedAction};
use serde_json::{Value, json};

fn prefix() -> Value {
    json!({
        "schema_version": 2,
        "build_fingerprint": "selected producer",
        "closure_receipts": {"ancestor": "retained ancestor receipt"},
        "stages": {"stage": "retained stage receipt"},
        "docs": {"retained": "earlier documentation selection"},
        "source_artifacts": {"stage": {"observation.json": "exact source selection"}},
    })
}

fn bundle() -> gmeow_bundle_import::BundleFixtureSelector {
    let receipt = gmeow_bundle_import::ImportReceipt {
        schema_version: 1,
        action_key: "selected import action".into(),
        build_fingerprint: "optimized producer".into(),
        codec: "selected graph-preserving codec".into(),
        source_digest: "selected source digest".into(),
        source_bytes: 11,
        pack_digest: "selected packed digest".into(),
        pack_bytes: 13,
        dataset_quads: 1,
        named_graphs: 1,
    };
    gmeow_bundle_import::BundleFixtureSelector {
        schema_version: 1,
        receipt_digest: receipt.receipt_digest(),
        receipt,
        corpus_artifacts: BTreeMap::new(),
    }
}

fn docs() -> gmeow_docs_model::fixture::DocsFixtureSelector {
    gmeow_docs_model::fixture::DocsFixtureSelector {
        schema_version: 1,
        model: SelectedAction {
            context: ActionContext::new(
                "docs",
                "model",
                ProducerIdentity::new("optimized producer"),
                "selected model codec",
                Vec::new(),
            ),
            receipt_digest: "selected model receipt".into(),
            product_digest: "selected model product".into(),
        },
        renders: BTreeMap::new(),
    }
}

fn seed(root: &Path) -> (std::path::PathBuf, Vec<u8>) {
    let path = root.join(gmeow_pipeline::fixture::STAGE_FIXTURE_MANIFEST_RELATIVE_PATH);
    write_json_atomic(&path, &prefix()).unwrap();
    let bytes = std::fs::read(&path).unwrap();
    (path, bytes)
}

#[test]
fn all_selection_is_in_memory_until_one_complete_publication() {
    let root = tempfile::tempdir().unwrap();
    let (path, old) = seed(root.path());
    let mut stages = prefix();
    stages.as_object_mut().unwrap().remove("docs");
    stages["stages"]["stage"] = json!("new stage receipt");
    stages["source_artifacts"]["stage"]["observation.json"] = json!("new source selection");
    let bundle = bundle();
    let docs = docs();
    let prepared = prepare_fixture_selector(
        root.path(),
        Some(stages.clone()),
        Some(&bundle),
        Some(&docs),
    )
    .unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), old);
    let expected = prepared.bytes.clone();
    let complete: Value = serde_json::from_slice(&expected).unwrap();
    assert_eq!(complete["stages"], stages["stages"]);
    assert_eq!(complete["closure_receipts"], stages["closure_receipts"]);
    assert_eq!(complete["source_artifacts"], stages["source_artifacts"]);
    assert_eq!(
        complete["bundle_import"],
        serde_json::to_value(&bundle).unwrap()
    );
    assert_eq!(complete["docs"], serde_json::to_value(&docs).unwrap());
    let finalized = prepared.publish().unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), expected);
    assert_eq!(finalized.sha256, ContentDigest::of(&expected).to_hex());
}

#[test]
fn rejected_selection_and_failed_telemetry_preserve_published_bytes() {
    let root = tempfile::tempdir().unwrap();
    let (path, old) = seed(root.path());
    assert!(
        prepare_fixture_selector(
            root.path(),
            Some(json!({"schema_version": 1})),
            Some(&bundle()),
            Some(&docs())
        )
        .is_err()
    );
    assert_eq!(std::fs::read(&path).unwrap(), old);
    let prepared =
        prepare_fixture_selector(root.path(), Some(prefix()), Some(&bundle()), Some(&docs()))
            .unwrap();
    let telemetry = root.path().join("not-a-directory");
    std::fs::write(&telemetry, b"existing file").unwrap();
    assert!(
        write_json_atomic(
            &telemetry.join("timings.json"),
            &json!({"selector": prepared.sha256})
        )
        .is_err()
    );
    drop(prepared);
    assert_eq!(std::fs::read(path).unwrap(), old);
}

#[test]
fn prefix_completion_preserves_stage_closure_and_selected_documentation() {
    for replace_docs in [false, true] {
        let root = tempfile::tempdir().unwrap();
        let (path, old) = seed(root.path());
        let bundle = bundle();
        let docs = docs();
        let selected_docs = replace_docs.then_some(&docs);
        let prepared =
            prepare_fixture_selector(root.path(), None, Some(&bundle), selected_docs).unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), old);
        prepared.publish().unwrap();
        let selected: Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
        assert_eq!(selected["stages"], prefix()["stages"]);
        assert_eq!(selected["closure_receipts"], prefix()["closure_receipts"]);
        assert_eq!(selected["source_artifacts"], prefix()["source_artifacts"]);
        assert_eq!(
            selected["bundle_import"],
            serde_json::to_value(bundle).unwrap()
        );
        let expected_docs = if replace_docs {
            serde_json::to_value(docs).unwrap()
        } else {
            prefix()["docs"].clone()
        };
        assert_eq!(selected["docs"], expected_docs);
    }
}

#[test]
fn independent_prefix_publishes_without_bundle_and_missing_or_corrupt_prefix_fails() {
    let root = tempfile::tempdir().unwrap();
    assert!(prepare_fixture_selector(root.path(), None, Some(&bundle()), None).is_err());
    let mut stages = prefix();
    stages.as_object_mut().unwrap().remove("docs");
    let prepared = prepare_fixture_selector(root.path(), Some(stages.clone()), None, None).unwrap();
    assert!(!prepared.path.exists());
    let finalized = prepared.publish().unwrap();
    let bytes = std::fs::read(&finalized.path).unwrap();
    assert_eq!(serde_json::from_slice::<Value>(&bytes).unwrap(), stages);
    std::fs::write(&finalized.path, b"corrupt selection").unwrap();
    assert!(prepare_fixture_selector(root.path(), None, Some(&bundle()), Some(&docs())).is_err());
    assert_eq!(std::fs::read(finalized.path).unwrap(), b"corrupt selection");
}

#[test]
fn heavy_extension_preserves_all_selected_actions_until_publication() {
    let root = tempfile::tempdir().unwrap();
    let (path, old) = seed(root.path());
    let selected = json!({"receipt": "new exhaustive conformance action"});
    let prepared = prepare_fixture_selector_fields(
        root.path(),
        None,
        BTreeMap::from([("conformance_heavy", selected.clone())]),
    )
    .unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), old);
    let expected: Value = serde_json::from_slice(&prepared.bytes).unwrap();
    assert_eq!(expected["conformance_heavy"], selected);
    for field in ["stages", "closure_receipts", "source_artifacts", "docs"] {
        assert_eq!(expected[field], prefix()[field]);
    }
    let finalized = prepared.publish().unwrap();
    let bytes = std::fs::read(path).unwrap();
    assert_eq!(finalized.sha256, ContentDigest::of(&bytes).to_hex());
    assert_eq!(serde_json::from_slice::<Value>(&bytes).unwrap(), expected);
}
