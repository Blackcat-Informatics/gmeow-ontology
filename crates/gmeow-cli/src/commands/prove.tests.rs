// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

/// A parse refusal in a later document keeps every selected input identity.
#[test]
fn malformed_later_source_preserves_prior_and_failing_input_identity() {
    let directory = tempfile::tempdir().expect("scratch directory");
    let first = directory.path().join("a.ttl");
    let second = directory.path().join("b.ttl");
    std::fs::write(&first, "<urn:s> <urn:p> <urn:o> .").expect("source");
    std::fs::write(&second, "this is not turtle").expect("bad source");
    let error = match super::load_theory(&[first, second]) {
        Err(super::LoadFailure::Malformed(error)) => error,
        Err(super::LoadFailure::Execution(_)) => panic!("a parse refusal is malformed input"),
        Ok(_) => panic!("malformed source must fail"),
    };
    let report = super::ProveReport::blank(super::DecisionClass::Malformed, error);
    assert_eq!(report.common.inputs.len(), 2);
    assert_eq!(report.common.diagnostics[0].code, "SOURCE_PARSE_FAILED");
    let diagnostic = report.common.diagnostics[0]
        .diagnostic
        .as_ref()
        .expect("evidence");
    assert!(!diagnostic.causes.is_empty());
    assert!(diagnostic.observed.is_some());
}

/// A frontend refusal keeps its native diagnostic inside the consumer report.
#[test]
fn empty_selected_source_preserves_the_frontend_refusal_and_input_identity() {
    let directory = tempfile::tempdir().expect("synthetic source directory");
    let source = directory.path().join("empty.ttl");
    std::fs::write(&source, "@prefix ex: <urn:example:> .").expect("empty valid Turtle");
    let error = match super::load_theory(&[source]) {
        Err(super::LoadFailure::Malformed(error)) => error,
        Err(super::LoadFailure::Execution(_)) => panic!("an empty source is malformed input"),
        Ok(_) => panic!("an empty selected theory cannot authorize proof"),
    };
    let report = super::ProveReport::blank(super::DecisionClass::Malformed, error);
    assert_eq!(report.common.inputs.len(), 1);
    let diagnostic = &report.common.diagnostics[0];
    assert_eq!(diagnostic.code, "SOURCE_PREPARATION_FAILED");
    let evidence = diagnostic.diagnostic.as_ref().expect("structured evidence");
    assert_eq!(
        evidence.fields["evidence"]["cause_diagnostic"]["code"],
        "logic-compile.frontend"
    );
    assert_eq!(
        evidence.fields["evidence"]["cause_diagnostic"]["diagnostic"]["code"],
        "logic-compile.frontend"
    );
    assert!(evidence.causes.iter().any(|cause| cause.contains("empty")));
}

/// A lower parser refusal retains the same complete wire evidence in a prove report.
#[test]
fn source_refusal_retains_the_shared_diagnostic_projection() {
    let cause = gmeow_logic_compile::frontend::parse_logic_str("this is not turtle <<<", None)
        .expect_err("malformed selected source")
        .with_context("parse selected proof source");
    let expected = serde_json::to_value(super::ReportDiagnostic::from_diag(&cause))
        .expect("native diagnostic report");
    let failure = super::caused_diagnostic(
        "SOURCE_PREPARATION_FAILED",
        cause,
        "prepare selected proof source",
        serde_json::json!({"path": "selected.ttl"}),
    );
    let report = super::ProveReport::blank(super::DecisionClass::Malformed, failure);
    let diagnostic = &report.common.diagnostics[0];
    assert_eq!(diagnostic.code, "SOURCE_PREPARATION_FAILED");
    let evidence = diagnostic.diagnostic.as_ref().expect("prove diagnostic");
    assert_eq!(evidence.fields["evidence"]["path"], "selected.ttl");
    let retained = &evidence.fields["evidence"]["cause_diagnostic"];
    assert_eq!(retained, &expected);
    assert_eq!(
        retained["diagnostic"]["failure_class"],
        "https://blackcatinformatics.ca/logic/CompilerSourceRefusal"
    );
    assert!(
        retained["diagnostic"]["causes"]
            .as_array()
            .is_some_and(|causes| !causes.is_empty())
    );
    assert_eq!(
        serde_json::to_value(&report).expect("first report projection"),
        serde_json::to_value(&report).expect("repeated report projection")
    );
}
