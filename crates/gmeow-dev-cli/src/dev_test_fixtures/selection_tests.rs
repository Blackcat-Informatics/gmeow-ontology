// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Metadata-only publication controls; no fixture or corpus is constructed.

use super::*;
use gmeow_action_cache::{ActionContext, ProducerIdentity, selection::SelectedAction};
use purrdf_lex::json::{Value, record::read_document};

fn prefix() -> Value {
    read_document(br#"{
        "schema_version": 2,
        "build_fingerprint": "selected producer",
        "closure_receipts": {"ancestor": "retained ancestor \u00e9\u4e2d\ud83d\ude00\n\"\\ receipt"},
        "stages": {"stage": "retained stage receipt"},
        "docs": {"retained": "earlier documentation selection"},
        "source_artifacts": {"stage": {"observation.json": "exact source selection"}}
    }"#).unwrap()
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
        corpus_artifacts: BTreeMap::from([(
            "artifact".into(),
            gmeow_bundle_import::CorpusArtifactPublication {
                schema_version: 1,
                action_key: "artifact action".into(),
                receipt_digest: "artifact receipt".into(),
                build_fingerprint: "bundle producer".into(),
                artifact_producer_fingerprint: "artifact producer".into(),
                name: "artifact".into(),
                source_sha256: "source sha".into(),
                product_digest: "product sha".into(),
                product_bytes: u64::MAX,
            },
        )]),
    }
}

fn docs() -> gmeow_docs_model::fixture::DocsFixtureSelector {
    let mut selector = gmeow_docs_model::fixture::DocsFixtureSelector {
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
    };
    selector
        .renders
        .insert("site:fr".into(), selector.model.clone());
    selector
}

fn seed(root: &Path) -> (std::path::PathBuf, Vec<u8>) {
    let path = root.join(gmeow_pipeline::fixture::STAGE_FIXTURE_MANIFEST_RELATIVE_PATH);
    write_bytes_atomic(&path, &encode_fixture_selector(&prefix()).unwrap()).unwrap();
    let bytes = std::fs::read(&path).unwrap();
    (path, bytes)
}

fn assert_reference(name: &str, selected: &PreparedFixtureSelector) {
    let (bytes, sha256) = match name {
        "retain-docs" => (
            include_bytes!("selector_reference/retain-docs.json").as_slice(),
            "a3b8d089f09d8046bdcd0508eb73d1f1393ae00389d273601e47819cc82c1e44",
        ),
        "replace-docs" => (
            include_bytes!("selector_reference/replace-docs.json").as_slice(),
            "751b849f4d5023f1ca5acf7a410157967ef30a8c02f14fc4072123c02f1ae77c",
        ),
        "heavy" => (
            include_bytes!("selector_reference/heavy.json").as_slice(),
            "3e7fabdba061042c2a9289c2abc63d14ceb05412c76d94fd72d11028dec7f601",
        ),
        "independent" => (
            include_bytes!("selector_reference/independent.json").as_slice(),
            "40e14692c69b44465fe9c1a9bfcfa42dc27cc4559d6dbbe9bbe3dc7ac0476a73",
        ),
        "all" => (
            include_bytes!("selector_reference/all.json").as_slice(),
            "3529da70e14ff75a12f940e0082c7834bb33a84fa76ddc067483c5287d2d2c3e",
        ),
        _ => panic!("unknown selector reference"),
    };
    assert_eq!(selected.bytes, bytes);
    assert_eq!(selected.sha256, sha256);
}

#[test]
fn all_selection_is_in_memory_until_one_complete_publication() {
    let root = tempfile::tempdir().unwrap();
    let (path, old) = seed(root.path());
    let mut stages = prefix();
    stages.as_object_mut().unwrap().remove("docs");
    stages["stages"]["stage"] = Value::from("new stage receipt");
    stages["source_artifacts"]["stage"]["observation.json"] = Value::from("new source selection");
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
    assert_reference("all", &prepared);
    let expected = prepared.bytes.clone();
    let complete: Value = read_document(&expected).unwrap();
    assert_eq!(complete["stages"], stages["stages"]);
    assert_eq!(complete["closure_receipts"], stages["closure_receipts"]);
    assert_eq!(complete["source_artifacts"], stages["source_artifacts"]);
    assert_eq!(complete["bundle_import"], bundle.to_json());
    assert_eq!(complete["docs"], docs.to_json());
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
            Some(read_document(br#"{"schema_version":1}"#).unwrap()),
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
            &serde_json::json!({"selector": prepared.sha256})
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
        assert_reference(
            if replace_docs {
                "replace-docs"
            } else {
                "retain-docs"
            },
            &prepared,
        );
        prepared.publish().unwrap();
        let selected: Value = read_document(&std::fs::read(path).unwrap()).unwrap();
        assert_eq!(selected["stages"], prefix()["stages"]);
        assert_eq!(selected["closure_receipts"], prefix()["closure_receipts"]);
        assert_eq!(selected["source_artifacts"], prefix()["source_artifacts"]);
        assert_eq!(selected["bundle_import"], bundle.to_json());
        let expected_docs = if replace_docs {
            docs.to_json()
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
    assert_reference("independent", &prepared);
    assert!(!prepared.path.exists());
    let finalized = prepared.publish().unwrap();
    let bytes = std::fs::read(&finalized.path).unwrap();
    assert_eq!(read_document(&bytes).unwrap(), stages);
    std::fs::write(&finalized.path, b"corrupt selection").unwrap();
    assert!(prepare_fixture_selector(root.path(), None, Some(&bundle()), Some(&docs())).is_err());
    assert_eq!(std::fs::read(finalized.path).unwrap(), b"corrupt selection");
}

#[test]
fn heavy_extension_preserves_all_selected_actions_until_publication() {
    let root = tempfile::tempdir().unwrap();
    let (path, old) = seed(root.path());
    let selected = read_document(br#"{"receipt":"new exhaustive conformance action"}"#).unwrap();
    let prepared = prepare_fixture_selector_fields(
        root.path(),
        None,
        BTreeMap::from([("conformance_heavy", selected.clone())]),
    )
    .unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), old);
    assert_reference("heavy", &prepared);
    let expected: Value = read_document(&prepared.bytes).unwrap();
    assert_eq!(expected["conformance_heavy"], selected);
    for field in ["stages", "closure_receipts", "source_artifacts", "docs"] {
        assert_eq!(expected[field], prefix()[field]);
    }
    let finalized = prepared.publish().unwrap();
    let bytes = std::fs::read(path).unwrap();
    assert_eq!(finalized.sha256, ContentDigest::of(&bytes).to_hex());
    assert_eq!(read_document(&bytes).unwrap(), expected);
}
