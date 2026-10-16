// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

#[test]
fn compiled_gate_borrows_separate_wiring_and_rejects_missing_selected_wiring() {
    let source = dataset_from_bytes(SOURCE_GRAPH.as_bytes(), NativeRdfFormat::Turtle).unwrap();
    let theory = PreparedLogicSource::new(&source)
        .unwrap()
        .into_compiled(None)
        .unwrap();
    let empty = RdfDatasetBuilder::new().freeze().unwrap();
    assert!(GateProgram::from_compiled_theory_with_wiring(&theory, &empty).is_err());
    let wiring = dataset_from_bytes(
            b"<https://example.org/category> <https://blackcatinformatics.ca/gmeow/categoryBlocking> <https://blackcatinformatics.ca/gmeow/blockingBlocking> .",
            NativeRdfFormat::Turtle,
        ).unwrap();
    let gate = GateProgram::from_compiled_theory_with_wiring(&theory, &wiring)
        .unwrap()
        .unwrap();
    assert_eq!(gate.category_blocking().len(), 1);
    assert!(
        gate.category_blocking()
            .contains_key("https://example.org/category")
    );
}

#[test]
fn compiled_gate_reads_native_wiring_in_its_selected_graph() {
    let original = dataset_from_bytes(SOURCE_GRAPH.as_bytes(), NativeRdfFormat::Turtle).unwrap();
    let mut builder = RdfDatasetBuilder::new();
    builder.push_dataset(&original).unwrap();
    let category = builder
        .intern_iri("https://example.org/NativeCategory")
        .unwrap();
    let property = builder.intern_iri(CATEGORY_BLOCKING).unwrap();
    let blocking = builder
        .intern_iri("https://blackcatinformatics.ca/gmeow/blockingBlocking")
        .unwrap();
    builder
        .push_annotation(category, property, blocking)
        .unwrap();
    let outside = builder.intern_iri("https://example.org/outside").unwrap();
    builder
        .push_annotation_in_graph(category, property, outside, Some(outside))
        .unwrap();
    let dataset = builder.freeze().unwrap();
    let theory = PreparedLogicSource::new(&dataset)
        .unwrap()
        .into_compiled(None)
        .unwrap();
    let gate = GateProgram::from_compiled_theory(&theory).unwrap().unwrap();
    assert_eq!(
        gate.category_blocking
            .get("https://example.org/NativeCategory")
            .map(String::as_str),
        Some("https://blackcatinformatics.ca/gmeow/blockingBlocking")
    );
    assert_eq!(gate.category_blocking.len(), 3);
}

#[test]
fn declared_gate_rule_cannot_disappear_on_lowering_failure() {
    let source = "<https://blackcatinformatics.ca/logic/ruleGateFatalVerdict> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <https://blackcatinformatics.ca/logic/Rule> .";
    let error = GateProgram::from_source(source.as_bytes())
        .err()
        .expect("malformed selected rule");
    assert!(
        error
            .to_string()
            .contains("did not emit its required gate rule"),
        "{error}"
    );
}

/// A minimal authored source graph carrying the gate rule + the categoryBlocking
/// wiring, in the DEFAULT graph exactly like the pipeline base-graph bytes. The rule
/// mirrors the authored `logic:head`/`logic:body` reified-triple shape of
/// `slices/grounding/logic/module.ttl`'s `logic:ruleGateFatalVerdict` (never a
/// string body).
const SOURCE_GRAPH: &str = r#"
@prefix logic: <https://blackcatinformatics.ca/logic/> .
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .

logic:FindingDataShapeViolation gmeow:categoryBlocking gmeow:blockingBlocking .
logic:FindingPolicyWarning gmeow:categoryBlocking gmeow:blockingCoherent .

_:finding a logic:TermCarrier ; logic:termVariable "finding" .
_:category a logic:TermCarrier ; logic:termVariable "category" .
logic:ruleGateFatalVerdict a logic:Rule ;
    logic:head [ rdf:reifies <<( _:finding gmeow:findingGateVerdict gmeow:gateFatal )>> ] ;
    logic:body
        [ rdf:reifies <<( _:finding gmeow:findingSeverity gmeow:severityError )>> ],
        [ rdf:reifies <<( _:finding gmeow:findingCategory _:category )>> ],
        [ rdf:reifies <<( _:category gmeow:categoryBlocking gmeow:blockingBlocking )>> ],
        [ rdf:reifies <<( _:finding gmeow:findingStandpoint gmeow:standpointBinding )>> ] .
"#;

fn source_nquads(source: &str) -> Vec<u8> {
    let dataset = dataset_from_bytes(source.as_bytes(), NativeRdfFormat::Turtle).unwrap();
    purrdf::serialize_dataset(
        &dataset,
        "application/n-quads",
        purrdf::SerializeGraph::DefaultGraph,
    )
    .expect("serialize the complete native source for the production N-Quads reader")
}

/// Two findings in the graph/diagnostics named graph: one up-set (Error /
/// DataShapeViolation / Binding) that MUST derive gateFatal, and one non-up-set
/// (Error / PolicyWarning / Binding — coherent category) that must derive nothing.
fn finding_nq() -> String {
    let g = "https://blackcatinformatics.ca/gmeow/graph/diagnostics";
    let gm = GMEOW_NS;
    let logic = "https://blackcatinformatics.ca/logic/";
    let upset = "https://blackcatinformatics.ca/gmeow/examples/diagnostics/upset";
    let coherent = "https://blackcatinformatics.ca/gmeow/examples/diagnostics/coherent";
    format!(
        "<{upset}> <{gm}findingSeverity> <{gm}severityError> <{g}> .\n\
             <{upset}> <{gm}findingCategory> <{logic}FindingDataShapeViolation> <{g}> .\n\
             <{upset}> <{gm}findingStandpoint> <{gm}standpointBinding> <{g}> .\n\
             <{coherent}> <{gm}findingSeverity> <{gm}severityError> <{g}> .\n\
             <{coherent}> <{gm}findingCategory> <{logic}FindingPolicyWarning> <{g}> .\n\
             <{coherent}> <{gm}findingStandpoint> <{gm}standpointBinding> <{g}> .\n"
    )
}

#[test]
fn derives_gatefatal_only_for_the_upset_finding() {
    let gate = GateProgram::from_source(&source_nquads(SOURCE_GRAPH))
        .expect("gate source is valid")
        .expect("the source graph carries the authored logic:ruleGateFatalVerdict + wiring");
    let graph = "https://blackcatinformatics.ca/gmeow/graph/diagnostics";
    let derived = gate
        .derived_verdict_nquads(&finding_nq(), graph)
        .expect("derivation must succeed over well-formed finding N-Quads");

    let lines: Vec<&str> = derived.lines().filter(|l| !l.is_empty()).collect();
    assert_eq!(
        lines.len(),
        1,
        "exactly one gateFatal must be derived (the up-set finding only): {derived:?}"
    );
    let line = lines[0];
    assert!(
        line.starts_with("<https://blackcatinformatics.ca/gmeow/examples/diagnostics/upset>"),
        "the derived verdict must be for the up-set finding: {line}"
    );
    assert!(
        line.contains("<https://blackcatinformatics.ca/gmeow/findingGateVerdict>")
            && line.contains("<https://blackcatinformatics.ca/gmeow/gateFatal>"),
        "the derived line must carry findingGateVerdict gateFatal: {line}"
    );
    assert!(
        line.ends_with("<https://blackcatinformatics.ca/gmeow/graph/diagnostics> ."),
        "the derived verdict must land in the findings' graph: {line}"
    );
    assert!(
        !line.contains("/coherent>"),
        "the coherent (non-up-set) finding must NOT be derived gateFatal: {derived}"
    );
}

#[test]
fn diagnostic_renderer_preserves_projection_bytes_and_native_gate_union() {
    use crate::stages::diag_render::{DiagnosticsPaths, render_diagnostics_artifacts};
    use gmeow_errors::{Finding, FindingCategory, Report, Severity};
    let gate = GateProgram::from_source(&source_nquads(SOURCE_GRAPH))
        .unwrap()
        .unwrap();
    let mut report = Report::new("validate");
    report.add_finding(
        Finding::new(Severity::Error, "synthetic.constraint", "retained evidence")
            .with_category(FindingCategory::DataShapeViolation)
            .with_standpoint(gmeow_errors::grade::Standpoint::Binding),
    );
    let paths = DiagnosticsPaths {
        json: "report.json",
        sarif: "report.sarif",
        html: "report.html",
        rdf: "report.nq",
    };
    let artifacts =
        render_diagnostics_artifacts("validate", report.clone(), &paths, Some(&gate), None, None)
            .unwrap();
    let mut expected = gmeow_errors::render::to_gmeow_rdf(&report);
    let derived = gate
        .derived_verdict_nquads(&expected, crate::stages::carrier::GRAPH_DIAGNOSTICS)
        .unwrap();
    assert!(!derived.is_empty());
    expected.push_str(&derived);
    let dataset = dataset_from_bytes(expected.as_bytes(), NativeRdfFormat::NQuads).unwrap();
    assert_eq!(
        artifacts.artifacts["report.nq"],
        crate::stages::superset::canonical_ntriples(&dataset).unwrap()
    );
    assert_eq!(
        artifacts.artifacts["report.nq"],
        crate::stages::superset::canonical_ntriples(&artifacts.dataset).unwrap(),
        "the carrier retains the same authored gate conclusions as the artifact"
    );
    let rendered: Report = serde_json::from_slice(&artifacts.artifacts["report.json"]).unwrap();
    assert_eq!(rendered.findings[0].code, "synthetic.constraint");
    assert_eq!(rendered.findings[0].message, "retained evidence");
}

#[test]
fn absent_rule_yields_none() {
    // categoryBlocking wiring but NO gate rule → nothing to derive, byte-unchanged.
    let no_rule = concat!(
        "<https://blackcatinformatics.ca/logic/FindingDataShapeViolation> ",
        "<https://blackcatinformatics.ca/gmeow/categoryBlocking> ",
        "<https://blackcatinformatics.ca/gmeow/blockingBlocking> .\n",
    );
    assert!(
        GateProgram::from_source(no_rule.as_bytes())
            .unwrap()
            .is_none(),
        "a source without the authored gate rule must yield None"
    );
}

#[test]
fn malformed_or_incomplete_gate_semantics_fail_closed() {
    assert!(GateProgram::from_source(b"not RDF").is_err());
    let without_wiring = SOURCE_GRAPH
        .lines()
        .filter(|line| line.split_whitespace().nth(1) != Some("gmeow:categoryBlocking"))
        .collect::<Vec<_>>()
        .join("\n");
    let error = GateProgram::from_source(&source_nquads(&without_wiring))
        .err()
        .unwrap();
    assert!(
        error.to_string().contains("no categoryBlocking wiring"),
        "{error}"
    );
}
