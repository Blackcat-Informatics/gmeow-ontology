// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Installed consumer reads its authenticated embedded owner; no corpus production.

use std::path::Path;
use std::process::{Command, Output};

fn input() -> String {
    // This harness consumes an exact pre-test producer artifact. The installed
    // process below still runs without a checkout or access to this selector.
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let artifact = gmeow_pipeline::fixture::authenticated_artifact(
        &root,
        "stage-compile-logic",
        "pipeline/module-validation-fixtures.json",
    )
    .expect("authenticated producer-selected financial acceptance inputs");
    #[derive(serde::Deserialize)]
    struct SelectedInputs {
        inputs: std::collections::BTreeMap<String, Vec<u8>>,
    }
    let mut selected: SelectedInputs = serde_json::from_slice(&artifact)
        .expect("complete selected validation fixture input schema");
    let bytes = selected
        .inputs
        .remove("slices/extensions/finance/examples/exact-computations.ttl")
        .expect("producer selected the financial example input");
    String::from_utf8(bytes).expect("producer-selected financial input is UTF-8")
}

fn run(directory: &Path, subject: &str) -> Output {
    Command::new(assert_cmd::cargo::cargo_bin("gmeow"))
        .current_dir(directory)
        .args([
            "finance",
            "evaluate",
            "source.ttl",
            "--subject",
            subject,
            "--out",
            "result.ttl",
        ])
        .output()
        .unwrap()
}

#[test]
fn finance_evaluation_cli_uses_embedded_recipes_without_a_checkout() {
    let directory = tempfile::tempdir().unwrap();
    std::fs::write(directory.path().join("source.ttl"), input()).unwrap();
    for subject in ["urn:finance:interest", "urn:finance:return"] {
        let output = run(directory.path(), subject);
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let result = std::fs::read_to_string(directory.path().join("result.ttl")).unwrap();
        for required in [
            "quantityValue",
            "observationResult",
            "resultDerivedAxiom",
            "parameterPredicate",
            "subClassOf",
        ] {
            assert!(
                result.contains(required),
                "missing native financial support {required}"
            );
        }
    }
    assert!(!directory.path().join("generated").exists());
}

#[test]
fn finance_evaluation_cli_refuses_incompatible_currency_without_replacing_output() {
    let directory = tempfile::tempdir().unwrap();
    let original = input();
    let input = original.replace(
        "m:quantityValue 1200.00 ; m:hasDimension m:dimensionless ; g:currency g:referenceFrameUSD",
        "m:quantityValue 1200.00 ; m:hasDimension m:dimensionless ; g:currency g:referenceFrameEUR",
    );
    assert_ne!(input, original);
    std::fs::write(directory.path().join("source.ttl"), input).unwrap();
    std::fs::write(
        directory.path().join("result.ttl"),
        "previous complete result",
    )
    .unwrap();
    let output = run(directory.path(), "urn:finance:return");
    assert!(!output.status.success());
    let diagnostic = String::from_utf8_lossy(&output.stderr);
    assert!(diagnostic.contains("gmeow finance evaluate"));
    assert!(diagnostic.contains("source.ttl"));
    assert_eq!(
        std::fs::read_to_string(directory.path().join("result.ttl")).unwrap(),
        "previous complete result"
    );
}
