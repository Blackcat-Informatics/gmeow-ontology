// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

fn load(paths: &[std::path::PathBuf]) -> Result<super::LoadedTheory, super::LoadFailure> {
    let inputs = paths
        .iter()
        .map(|path| {
            super::OpenedInput::open(path.clone()).map_err(|error| {
                super::LoadFailure::Malformed(super::caused_failure(
                    "SOURCE_READ_FAILED",
                    error,
                    "open synthetic source",
                ))
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    super::load_theory(&inputs)
}

/// A later open failure retains the admitted prefix's actual input receipts;
/// a later parse refusal keeps the prior and failing document identities.
#[test]
fn malformed_later_source_preserves_prior_and_failing_input_identity() {
    let directory = tempfile::tempdir().expect("scratch directory");
    let first = directory.path().join("a.ttl");
    let second = directory.path().join("b.ttl");
    let first_bytes = b"<urn:s> <urn:p> <urn:o> .";
    std::fs::write(&first, first_bytes).expect("source");
    std::fs::write(&second, "this is not turtle").expect("bad source");
    let removed_after_discovery = directory.path().join("removed.ttl");
    let error = match super::files::open_sources(vec![first.clone(), removed_after_discovery]) {
        Err(super::LoadFailure::Malformed(error)) => error,
        Err(super::LoadFailure::Execution(_)) => panic!("unreadable input is malformed"),
        Ok(_) => panic!("the removed discovered file cannot open"),
    };
    let report = super::ProveReport::blank(super::DecisionClass::Malformed, error);
    assert_eq!(report.common.inputs.len(), 1);
    assert_eq!(report.common.inputs[0].path, first.display().to_string());
    assert_eq!(report.common.inputs[0].media_type, "text/turtle");
    assert_eq!(report.common.inputs[0].bytes, first_bytes.len() as u64);
    assert_eq!(report.common.diagnostics[0].code, "SOURCE_READ_FAILED");
    let error = match load(&[first, second]) {
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
    let error = match load(&[source]) {
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

/// PROTECTS: an engine that fails to execute does not erase the account made before
/// it. The report keeps the selection identities, the claims read and the drops named
/// before the failure, marks the selection incomplete, and stays an execution failure.
#[test]
fn an_engine_failure_keeps_the_selection_and_the_drops_named_before_it() {
    let directory = tempfile::tempdir().expect("scratch directory");
    let source = directory.path().join("a.logic.ttl");
    std::fs::write(
        &source,
        "<urn:a> <https://blackcatinformatics.ca/logic/instanceOf> <urn:A> .",
    )
    .expect("source");
    let Ok(loaded) = load(&[source]) else {
        panic!("a one-fact theory loads");
    };
    let selection = super::SourceSelection::all_classical_roots(&loaded.theory);
    let inventory = super::SourceInventory::new(&loaded.theory, selection);
    let resolved = super::resolve_requested_theory(&inventory, &[]).expect("top-level selection");
    let native_admission = super::NativeSourceAdmission::inspect_selected(&inventory, &resolved)
        .expect("native source admission");
    let admission = super::SourceAdmission::classical_fof_from_inventory(&inventory);
    let digest = inventory.selection_digest().to_owned();
    let program = inventory.program_digest().to_owned();
    let units = native_admission.units();
    let named = super::DropRecord::new(super::ENGINE_NATIVE, "SCOPED_CLAIM_NOT_READ", "before");
    let report = super::ProveReport::engine_failure(
        super::EngineFailure {
            diagnostic: super::failure("NATIVE_EXECUTION_FAILED", "the engine stopped"),
            claims_read: Vec::new(),
            native: None,
        },
        &loaded.identities,
        Some(admission),
        native_admission,
        &inventory,
        1,
        std::collections::BTreeSet::from([named.clone()]),
    );
    assert_eq!(
        report.common.decision,
        super::DecisionClass::ExecutionFailure
    );
    assert_eq!(report.common.inputs.len(), 1);
    assert_eq!(report.common.exit_code(true), 5);
    assert_eq!(report.common.program.content_digest, program);
    let selection = report.selection.expect("the selection block is kept");
    assert!(!selection.complete);
    assert_eq!(selection.selection_digest, digest);
    assert_eq!(selection.units, units);
    assert_eq!(selection.drops, [named]);
    assert!(report.admission.is_some());
}
