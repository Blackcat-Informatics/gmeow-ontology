// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Synthetic negative-cycle refusal; authored diagnostics run in the producer.
use gmeow_logic::reason::reason_program;
use gmeow_logic::reason::{
    DomainProfile, LogicalGraph, SelectedDomains, SelectedLogicalWorld, prepare_reasoning_input,
};
use gmeow_logic_compile::frontend::parse_logic_dataset;
use gmeow_ns::{GMEOW_NS, LOGIC_NS};
use purrdf::{NativeRdfFormat, RdfDatasetBuilder, RdfQuad, RdfTerm, dataset_from_bytes};
const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
const WORLD: &str = "https://blackcatinformatics.ca/gmeow/graph/diagnostics-meta-conformance";
fn gmeow(local: &str) -> String {
    format!("{GMEOW_NS}{local}")
}
fn push(builder: &mut RdfDatasetBuilder, s: &str, p: &str, o: &str) {
    builder.push_owned_quad(
        &RdfQuad::new(RdfTerm::iri(s), p, RdfTerm::iri(o)).in_graph(RdfTerm::iri(WORLD)),
    );
}

/// An unstratifiable program (a negative cycle `pingA :- Seed, ~pingB` and
/// `pingB :- Seed, ~pingA`) MUST make `reason_program` hard-error, never silently
/// under-derive or loop. This is the single biggest correctness risk of the fold: NAF
/// is only sound under stratification, so an unstratifiable rule set must be refused.
#[test]
fn unstratifiable_program_hard_fails() {
    // Authored inline (never a repo artifact): two rules forming a negative cycle over
    // helper predicates each rule derives and the other negates.
    let ttl = format!(
        r#"@prefix rdf:   <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .
@prefix logic: <{LOGIC_NS}> .
@prefix gmeow: <{GMEOW_NS}> .

logic:ruleUnstratA a logic:Rule ;
    logic:provenance logic:ruleUnstratA ;
    logic:head [ rdf:subject "?x" ; rdf:predicate gmeow:pingA ; rdf:object "?x" ] ;
    logic:body [ rdf:subject "?x" ; rdf:predicate rdf:type ; rdf:object gmeow:UnstratSeed ] ;
    logic:negatedBody [ rdf:subject "?x" ; rdf:predicate gmeow:pingB ; rdf:object "?x" ] .

logic:ruleUnstratB a logic:Rule ;
    logic:provenance logic:ruleUnstratB ;
    logic:head [ rdf:subject "?x" ; rdf:predicate gmeow:pingB ; rdf:object "?x" ] ;
    logic:body [ rdf:subject "?x" ; rdf:predicate rdf:type ; rdf:object gmeow:UnstratSeed ] ;
    logic:negatedBody [ rdf:subject "?x" ; rdf:predicate gmeow:pingA ; rdf:object "?x" ] .
"#
    );
    let dataset =
        dataset_from_bytes(ttl.as_bytes(), NativeRdfFormat::Turtle).expect("inline rules parse");
    let (program, _diags) =
        parse_logic_dataset(dataset.as_ref(), None).expect("inline rules lower");
    assert_eq!(
        program.rules.len(),
        2,
        "the inline unstratifiable program must carry both cycle rules"
    );

    // A non-vacuous seed so the negative cycle is live.
    let mut builder = RdfDatasetBuilder::new();
    push(
        &mut builder,
        &gmeow("examples/diagnostics/tests/unstratX"),
        RDF_TYPE,
        &gmeow("UnstratSeed"),
    );
    let edb = builder.freeze().expect("seed EDB must freeze");

    let reasoning_input = prepare_reasoning_input(&edb).expect("admit selected theory");
    let domains = SelectedDomains::new([SelectedLogicalWorld::new(
        LogicalGraph::Named(purrdf::TermValue::iri(WORLD)),
        DomainProfile::NonemptyObjectDomainV1,
        "gmeow-conformance.negative-cycle.v1".to_owned(),
        *reasoning_input.ingress_contract(),
    )
    .expect("admit selected theory")])
    .expect("admit selected theory");
    let outcome = reason_program(&program, reasoning_input, &domains);
    assert!(
        outcome.is_err(),
        "an unstratifiable (negative-cycle) program MUST hard-fail, got Ok: {:?}",
        outcome.map(|r| r.inferred().len())
    );
}
