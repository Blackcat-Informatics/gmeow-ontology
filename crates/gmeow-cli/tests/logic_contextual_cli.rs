// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Installed consumer behavior on supplied synthetic RDF 1.2. These processes
//! have no checkout environment and never produce or load a repository corpus.

use std::path::Path;
use std::process::{Command, Output};

use purrdf::{DatasetView, GraphMatch, TermValue};

const SOURCE: &str = r#"
@prefix logic: <https://blackcatinformatics.ca/logic/> .
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .
@prefix ex: <urn:contextual-cli:> .
ex:metadata {
  ex:view a logic:AttributedContext ; logic:contextWorld ex:world ;
    logic:contextStandpoint ex:author ; logic:evidenceClosure logic:ClosedWorldClosure .
  ex:query a logic:ContextualEvaluationRequest ;
    logic:queryFormula ex:formula ; logic:queryContext ex:view .
  ex:formula a logic:Formula ; logic:relation ex:ready ;
    logic:argument [ logic:termIndex 0 ; logic:termIri ex:task ],
                   [ logic:termIndex 1 ; logic:termIri ex:execution ] .
}
ex:world {
  ex:claim rdf:reifies <<( ex:task ex:ready ex:execution )>> ;
    gmeow:accordingTo ex:author ;
    gmeow:standpointSupportStatus gmeow:supportSupported .
}
"#;

fn run(cwd: &Path, arguments: &[&str]) -> Output {
    Command::new(assert_cmd::cargo::cargo_bin("gmeow"))
        .args(["--console", "silent", "logic", "evaluate"])
        .args(arguments)
        .current_dir(cwd)
        .env_clear()
        .output()
        .expect("execute the installed consumer on supplied data")
}

fn assert_assessment(output: &Output, evaluation: &str, information: &str) {
    let dataset = purrdf::parse_dataset(&output.stdout, "application/n-quads", None)
        .expect("stdout contains the complete RDF assessment");
    let id = |iri: &str| {
        dataset
            .term_id_by_value(&TermValue::iri(iri))
            .unwrap_or_else(|| panic!("assessment must contain {iri}"))
    };
    let logic = "https://blackcatinformatics.ca/logic/";
    let graph = GraphMatch::Named(id(gmeow_logic::result_rdf::GRAPH_REASONING));
    let results: Vec<_> = dataset
        .quads_for_pattern(
            Some(id("urn:contextual-cli:query")),
            Some(id(&format!("{logic}contextualResult"))),
            None,
            graph,
        )
        .collect();
    assert_eq!(results.len(), 1, "one selected request produces one result");
    for (property, value) in [
        ("resultEvaluation", format!("{logic}{evaluation}")),
        ("resultInformation", format!("{logic}{information}")),
        ("resultAttributedContext", "urn:contextual-cli:view".into()),
    ] {
        let values: Vec<_> = dataset
            .quads_for_pattern(
                Some(results[0].o),
                Some(id(&format!("{logic}{property}"))),
                None,
                graph,
            )
            .map(|quad| quad.o)
            .collect();
        assert_eq!(values, vec![id(&value)], "{property}");
    }
    if let Some(predicate) = dataset.term_id_by_value(&TermValue::iri("urn:contextual-cli:ready")) {
        assert!(
            dataset
                .quads_for_pattern(None, Some(predicate), None, GraphMatch::Any)
                .next()
                .is_none(),
            "assessment output cannot assert the queried proposition",
        );
    }
}

#[test]
fn installed_evaluator_emits_scoped_evidence_and_preserves_budget_exhaustion() {
    let scratch = tempfile::tempdir().expect("blinded working directory");
    std::fs::write(scratch.path().join("input.trig"), SOURCE).expect("supplied RDF");
    let arguments = ["input.trig", "--request", "urn:contextual-cli:query"];
    let completed = run(scratch.path(), &arguments);
    assert!(
        completed.status.success(),
        "{}",
        String::from_utf8_lossy(&completed.stderr)
    );
    assert_assessment(&completed, "EvaluationCompleted", "InfoSupported");

    let exhausted = run(
        scratch.path(),
        &[
            "input.trig",
            "--request",
            "urn:contextual-cli:query",
            "--max-steps",
            "0",
        ],
    );
    assert_eq!(exhausted.status.code(), Some(1));
    assert_assessment(&exhausted, "BudgetExhausted", "InfoUndetermined");
}

#[test]
fn installed_evaluator_rejects_missing_selection_without_manufacturing_an_assessment() {
    let scratch = tempfile::tempdir().expect("blinded working directory");
    std::fs::write(scratch.path().join("input.trig"), SOURCE).expect("supplied RDF");
    let absent = run(
        scratch.path(),
        &["input.trig", "--request", "urn:contextual-cli:absent"],
    );
    assert_eq!(absent.status.code(), Some(1));
    assert!(absent.stdout.is_empty());
    let usage = run(scratch.path(), &["input.trig"]);
    assert_eq!(usage.status.code(), Some(2));
    assert!(usage.stdout.is_empty());
}

#[test]
fn installed_evaluator_exports_fragment_refusal_as_diagnostics_and_a_non_result() {
    let scratch = tempfile::tempdir().expect("blinded working directory");
    let source = SOURCE
        .replace("logic:queryFormula ex:formula", "logic:queryFormula ex:quantified")
        .replace(
            "ex:metadata {",
            "ex:metadata { ex:quantified a logic:Formula ; logic:forall ex:formula ; logic:quantifiedVariable [ logic:termIndex 0 ; logic:termVariable \"x\" ] .",
        );
    std::fs::write(scratch.path().join("input.trig"), source).expect("supplied RDF");
    let output = run(
        scratch.path(),
        &["input.trig", "--request", "urn:contextual-cli:query"],
    );
    assert_eq!(output.status.code(), Some(1));
    assert_assessment(&output, "EvaluationUnsupported", "InfoNotEvaluated");
    let dataset = purrdf::parse_dataset(&output.stdout, "application/n-quads", None)
        .expect("structured fragment refusal");
    let diagnostics = dataset
        .term_id_by_value(&TermValue::iri(
            "https://blackcatinformatics.ca/gmeow/graph/diagnostics",
        ))
        .expect("shared diagnostic graph is emitted");
    assert!(
        dataset
            .quads_for_pattern(None, None, None, GraphMatch::Named(diagnostics))
            .next()
            .is_some(),
    );
}
