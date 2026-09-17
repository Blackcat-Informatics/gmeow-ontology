// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

#[test]
fn breadth_admission_requires_exact_action_product_and_current_inventory() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    let path = root.join(DIVERGENCE_CORPUS).join("synthetic/input.nq");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::create_dir_all(root.join(CORPORA[1])).unwrap();
    std::fs::write(&path, "<urn:s> <urn:p> <urn:o> .\n").unwrap();
    std::fs::write(
        path.parent().unwrap().join("profile.json"),
        r#"{
          "mode": "native",
          "verdict_mode": "class-source-admission",
          "w3c_published_verdict": "inconsistent",
          "source_admission_contract": "nativeClassExpressionListAdmission",
          "source_admission_status": "outside-selection"
        }"#,
    )
    .unwrap();
    let key = path
        .strip_prefix(root)
        .unwrap()
        .to_string_lossy()
        .into_owned();
    let context = ActionContext::new(
        "logic-conformance",
        ACTION,
        ProducerIdentity::new("synthetic-producer-profile"),
        CODEC,
        corpus_inputs(root).unwrap(),
    );
    let store = ActionStore::open(
        ActionStore::default_root(root),
        STORE_FORMAT_VERSION,
        StoreLimits::default(),
    )
    .unwrap();
    // Synthetic error evidence exercises admission without running an engine.
    let expected = Observations {
        consistency: ConsistencyObservations::from([(
            key,
            Err(super::super::record_failure(super::super::stage_err(
                "execution failure",
            ))),
        )]),
        class_diagnostics: super::super::native_cases::ClassDiagnostics::new(),
    };
    let bytes = serde_json::to_vec(&expected).unwrap();
    let receipt = store
        .publish(&context, bytes_digest(&bytes), (), &bytes)
        .unwrap();
    let selected = SelectedAction::from_receipt(&receipt);
    assert_eq!(admit(root, &selected).unwrap(), expected);
    let mut wrong = selected.clone();
    wrong.product_digest = "0".repeat(64);
    assert!(admit(root, &wrong).is_err());
    wrong = selected.clone();
    wrong.context.action = "different-operation".to_owned();
    assert!(admit(root, &wrong).is_err());
    std::fs::write(&path, "<urn:changed> <urn:p> <urn:o> .\n").unwrap();
    assert!(admit(root, &selected).is_err());
    std::fs::remove_file(&path).unwrap();
    assert!(admit(root, &selected).is_err());
}

#[test]
fn absent_breadth_receipt_cannot_create_a_store() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    let path = root.join(DIVERGENCE_CORPUS).join("synthetic/input.nq");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::create_dir_all(root.join(CORPORA[1])).unwrap();
    std::fs::write(&path, "<urn:s> <urn:p> <urn:o> .\n").unwrap();
    let selected = SelectedAction {
        context: ActionContext::new(
            "logic-conformance",
            ACTION,
            ProducerIdentity::new("synthetic-producer"),
            CODEC,
            corpus_inputs(root).unwrap(),
        ),
        receipt_digest: "0".repeat(64),
        product_digest: "0".repeat(64),
    };
    assert!(admit(root, &selected).is_err());
    assert!(!ActionStore::default_root(root).exists());
}
