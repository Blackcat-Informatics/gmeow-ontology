// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use gmeow_logic::reason::{
    DomainProfile, LogicalGraph, SelectedDomains, SelectedLogicalWorld, prepare_reasoning_input,
};
use purrdf::RdfTerm;

fn fixture() -> (Arc<RdfDataset>, gmeow_logic::result::ReasoningResult) {
    let edb = purrdf::parse_dataset(
        b"<urn:s> <urn:p> <urn:o> <urn:world> .\n",
        "application/n-quads",
        None,
    )
    .expect("fixture EDB");
    let reasoning_input = prepare_reasoning_input(&edb).expect("admit selected theory");
    let domains = SelectedDomains::new([SelectedLogicalWorld::new(
        LogicalGraph::Named(purrdf::TermValue::iri("urn:world")),
        DomainProfile::NonemptyObjectDomainV1,
        "gmeow.pipeline.verify-attestation.synthetic.v1".to_owned(),
        *reasoning_input.ingress_contract(),
    )
    .expect("admit selected theory")])
    .expect("admit selected theory");
    let reasoning =
        gmeow_logic::reason::reason_all(reasoning_input, &domains).expect("fixture closure");
    (edb, reasoning)
}

fn assessments(dataset: &RdfDataset) -> usize {
    dataset
        .project_named_graph(crate::stages::carrier::GRAPH_VERIFY)
        .owned_quads()
        .filter(|quad| {
            quad.predicate == "http://www.w3.org/1999/02/22-rdf-syntax-ns#type"
                && matches!(&quad.object, RdfTerm::Iri(iri) if iri == QUALITY_ASSESSMENT)
        })
        .count()
}

#[test]
fn clean_and_poisoned_queries_have_attestation_teeth_without_re_reasoning() {
    let (edb, reasoning) = fixture();
    let clean = vec![(
        "clean.rq".to_string(),
        "SELECT ?s WHERE { ?s <urn:missing> <urn:o> . }".to_string(),
    )];
    let (clean_graph, clean_report) =
        evaluate_attestation(edb.as_ref(), &reasoning, &clean).expect("clean verify");
    assert_eq!(assessments(clean_graph.as_ref()), 1);
    assert!(clean_report.ok(), "clean query must pass: {clean_report:?}");

    let poisoned = vec![(
        "poisoned.rq".to_string(),
        "SELECT ?s WHERE { ?s <urn:p> <urn:o> . }".to_string(),
    )];
    let (poisoned_graph, poisoned_report) =
        evaluate_attestation(edb.as_ref(), &reasoning, &poisoned).expect("poisoned verify");
    assert_eq!(assessments(poisoned_graph.as_ref()), 1);
    assert!(
        poisoned_report
            .findings
            .iter()
            .any(|finding| finding.code == "verify.poisoned"
                && finding.severity == gmeow_errors::Severity::Error),
        "the poisoned query must produce a hard verification finding: {poisoned_report:?}"
    );
}

#[test]
fn output_receipt_records_zero_closure_constructions() {
    let (edb, reasoning) = fixture();
    let queries = vec![(
        "clean.rq".to_string(),
        "SELECT ?s WHERE { ?s <urn:missing> <urn:o> . }".to_string(),
    )];
    let output = build_output(
        "stage-verify-attestation",
        edb.as_ref(),
        &reasoning,
        &queries,
        &gmeow_logic::verify::PreparedVerification::new(&queries, &test_gates())
            .expect("prepare tiny query"),
    )
    .expect("verify product");
    let report: serde_json::Value = serde_json::from_slice(
        output
            .product
            .artifact(VERIFY_JSON_PATH)
            .expect("verify JSON artifact"),
    )
    .expect("normalized report JSON");
    assert_eq!(report["metadata"]["closureConstructions"], 0);
    assert_eq!(report["metadata"]["verifyQueryCount"], 1);
    assert!(!output.product.diag_nodes().is_empty());
}

#[test]
fn independent_grader_accepts_exact_outputs_and_rejects_stale_projections() {
    let (edb, reasoning) = fixture();
    let queries = vec![(
        "clean.rq".to_string(),
        "SELECT ?s WHERE { ?s <urn:missing> <urn:o> . }".to_string(),
    )];
    let gates = test_gates();
    let report = gmeow_logic::verify::PreparedVerification::new(&queries, &gates)
        .expect("prepare tiny query")
        .verify_with_reasoning_result(edb.as_ref(), &reasoning)
        .expect("evaluate exact report");
    let output = build_output_from_report(
        "stage-verify-attestation",
        edb.as_ref(),
        &reasoning,
        &queries,
        report.clone(),
        &gates,
    )
    .expect("render producer outputs");
    let snapshot = output.product.dataset();
    let record = output
        .product
        .artifact(VERIFY_JSON_PATH)
        .expect("verify record");

    let exact = grade_shipped_attestation(
        snapshot,
        edb.as_ref(),
        &reasoning,
        &queries,
        &report,
        record,
        &gates,
    )
    .expect("exact producer projections grade fresh");
    assert_eq!(exact.query_count, 1);
    assert_eq!(exact.closure_constructions, 0);

    let mut tampered_record = record.to_vec();
    tampered_record.push(b' ');
    let record_error = grade_shipped_attestation(
        snapshot,
        edb.as_ref(),
        &reasoning,
        &queries,
        &report,
        &tampered_record,
        &gates,
    )
    .expect_err("tampered normalized record must fail");
    assert!(record_error.to_string().contains("verify record is stale"));

    let mut stale_value: serde_json::Value = serde_json::from_slice(record).expect("record JSON");
    stale_value["metadata"]["verifyQueryCount"] = serde_json::json!(999);
    let stale_record = serde_json::to_vec_pretty(&stale_value).expect("stale JSON");
    let value_error = grade_shipped_attestation(
        snapshot,
        edb.as_ref(),
        &reasoning,
        &queries,
        &report,
        &stale_record,
        &gates,
    )
    .expect_err("stale JSON value must fail");
    assert!(
        value_error
            .to_string()
            .contains("/metadata/verifyQueryCount: expected 1, shipped 999"),
        "{value_error}"
    );

    let empty_snapshot =
        purrdf::parse_dataset(b"", "application/n-quads", None).expect("empty snapshot dataset");
    let graph_error = grade_shipped_attestation(
        empty_snapshot.as_ref(),
        edb.as_ref(),
        &reasoning,
        &queries,
        &report,
        record,
        &gates,
    )
    .expect_err("missing graph/verify must fail");
    assert!(graph_error.to_string().contains("no graph/verify"));
}
