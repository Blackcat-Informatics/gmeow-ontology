// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Tests for the `graph/reasoning` RDF projection (#1132 C7): determinism, axis-IRI
//! presence, and the faithful verdict+provenance round-trip.

use super::*;
use crate::reason::el::InferredAxiom;
use crate::result::{
    Assumption, BudgetLimit, BudgetUsage, CompletenessStatus, ContradictionWitness, DerivationRef,
    EngineId, EvaluationStatus, InformationState, InputStatus, PreservationClaim, ReasoningResult,
    ResultContext, ResultPayload, ResultProvenance,
};
use gmeow_logic_compile::ir::PreservationKind;
use std::collections::BTreeSet;

/// A richly-populated result exercising every axis + provenance field that has a
/// projection: both Belnap-glut witnesses (so `Both` validates), proof+counterproof,
/// budget, assumptions, context, and an inferred closure with one derived axiom.
fn rich_result() -> ReasoningResult {
    let proof = DerivationRef {
        derivation_id: "deriv:proof-1".to_owned(),
        cited_iris: ["http://ex/a", "http://ex/b"]
            .into_iter()
            .map(String::from)
            .collect(),
    };
    let counterproof = DerivationRef {
        derivation_id: "deriv:counter-1".to_owned(),
        cited_iris: ["http://ex/c"].into_iter().map(String::from).collect(),
    };
    let mut assumptions = BTreeSet::new();
    assumptions.insert(Assumption::ClosedWorld);
    assumptions.insert(Assumption::SkolemWitness);

    let prov = ResultProvenance {
        contract_hash: "contract:abc123".to_owned(),
        query: "ASK { ?x a ex:Cat }".to_owned(),
        conclusion: "ex:Felix a ex:Cat".to_owned(),
        proof: Some(proof),
        counterproof: Some(counterproof),
        context: ResultContext {
            world: "http://ex/world/actual".to_owned(),
            standpoint: Some("http://ex/standpoint/s1".to_owned()),
            time: Some("2026-06-28".to_owned()),
            path: Some("http://ex/path/p1".to_owned()),
        },
        engine: EngineId {
            name: "gmeow-logic".to_owned(),
            version: "test.v9".to_owned(),
        },
        consumed_budget: BudgetUsage {
            consumed: 42,
            allowance: Some(1000),
            limit: Some(BudgetLimit::Inference),
        },
        certified_fragment: Some("http://ex/fragment/el".to_owned()),
        // projection_class mirrors the result's `preservation` axis in every real
        // construction (`from_dl_verdict`/`from_query` set it from preservation); the
        // parser reconstructs it from that axis, so the fixture matches the invariant.
        projection_class: {
            let mut c = PreservationClaim::default();
            c.insert(PreservationKind::SoundUnder).unwrap();
            c.unsupported_constructs
                .insert("http://ex/UnsupportedThing".to_owned());
            c
        },
        contradiction_witnesses: vec![ContradictionWitness {
            individual: "http://ex/Felix".to_owned(),
            world: "http://ex/world/actual".to_owned(),
            premises: vec![(
                "http://ex/Felix".to_owned(),
                "http://www.w3.org/1999/02/22-rdf-syntax-ns#type".to_owned(),
                "http://ex/Cat".to_owned(),
            )],
        }],
        assumptions,
    };

    let mut preservation = PreservationClaim::default();
    preservation
        .insert(PreservationKind::SoundUnder)
        .expect("polarity");
    preservation
        .unsupported_constructs
        .insert("http://ex/UnsupportedThing".to_owned());

    let payload = ResultPayload::Inferred(vec![
        InferredAxiom {
            subject: "http://ex/Felix".to_owned(),
            predicate: "http://www.w3.org/2000/01/rdf-schema#subClassOf".to_owned(),
            object: "http://ex/Animal".to_owned(),
            world: "http://ex/world/actual".to_owned(),
            is_edb: false,
            rule_name: Some("rule:subclass-transitive".to_owned()),
            premises: vec![],
        },
        InferredAxiom {
            subject: "http://ex/told".to_owned(),
            predicate: "http://ex/p".to_owned(),
            object: "http://ex/o".to_owned(),
            world: "http://ex/world/actual".to_owned(),
            is_edb: true, // an EDB axiom is NOT projected as a derived row.
            rule_name: None,
            premises: vec![],
        },
    ]);

    ReasoningResult::new(
        InputStatus::Valid,
        EvaluationStatus::Completed,
        CompletenessStatus::Incomplete,
        preservation,
        InformationState::Both,
        prov,
        payload,
    )
}

#[test]
fn projection_is_byte_deterministic() {
    let result = rich_result();
    let a = project_reasoning_result(&result);
    let b = project_reasoning_result(&result);
    assert_eq!(a, b, "the graph/reasoning projection must be byte-stable");
    // And re-projecting a freshly-constructed equal result is identical.
    let c = project_reasoning_result(&rich_result());
    assert_eq!(a, c, "structurally-equal results project identically");
}

#[test]
fn five_axes_appear_with_their_iris() {
    let result = rich_result();
    let body = project_reasoning_result(&result);
    for iri in [
        result.input.iri(),
        result.evaluation.iri(),
        result.completeness.iri(),
        result.information.iri(),
    ] {
        assert!(
            body.contains(&format!("<{iri}>")),
            "axis IRI {iri} must appear in the projection"
        );
    }
    // The preservation polarity link carries the PreservationKind IRI.
    assert!(body.contains(&PreservationKind::SoundUnder.iri()));
    // The result node is the content-addressed subject.
    assert!(body.contains(&format!("<{RESULT_IRI_BASE}")));
    assert!(body.contains(&logic("ReasoningResult")));
}

#[test]
fn content_address_is_stable_and_differs_with_content() {
    let a = result_node_iri(&rich_result());
    let b = result_node_iri(&rich_result());
    assert_eq!(a, b, "the content-addressed node IRI is reproducible");

    // A different verdict mints a different node.
    let mut other = rich_result();
    other.information = InformationState::Supported;
    other.provenance.counterproof = None;
    other.provenance.contradiction_witnesses.clear();
    let c = result_node_iri(&other);
    assert_ne!(a, c, "a different result mints a different node IRI");
}

#[test]
fn round_trip_recovers_axes_and_provenance() {
    let original = rich_result();
    let body = project_reasoning_result(&original);
    let parsed = parse_reasoning_graph(&body).expect("parse graph/reasoning");

    // The five axes round-trip exactly.
    assert_eq!(parsed.input, original.input);
    assert_eq!(parsed.evaluation, original.evaluation);
    assert_eq!(parsed.completeness, original.completeness);
    assert_eq!(parsed.information, original.information);
    assert_eq!(parsed.preservation, original.preservation);

    // The whole provenance bundle round-trips exactly (every scalar + set + witness).
    assert_eq!(parsed.provenance, original.provenance);

    // The payload DISCRIMINANT round-trips (Inferred); the derived-only rows survive
    // (the EDB row does not — only derived axioms are projected).
    let ResultPayload::Inferred(rows) = &parsed.payload else {
        panic!("payload discriminant must round-trip as Inferred");
    };
    assert_eq!(
        rows.len(),
        1,
        "only the one derived (non-EDB) axiom is carried"
    );
    assert_eq!(rows[0].subject, "http://ex/Felix");
    assert!(!rows[0].is_edb);
    // The re-parsed result is itself a valid ReasoningResult.
    parsed.validate().expect("re-parsed result validates");
}

#[test]
fn round_trip_for_an_invalid_request() {
    let original = ReasoningResult::invalid(
        "ill-formed request",
        ResultProvenance::native("contract:x", "http://ex/world/w"),
    );
    let body = project_reasoning_result(&original);
    let parsed = parse_reasoning_graph(&body).expect("parse");
    assert_eq!(parsed.input, InputStatus::Invalid);
    assert_eq!(parsed.evaluation, EvaluationStatus::Unsupported);
    assert_eq!(parsed.information, InformationState::NotEvaluated);
    assert_eq!(parsed.provenance.conclusion, "ill-formed request");
    assert!(matches!(parsed.payload, ResultPayload::Empty));
}

#[test]
fn parse_rejects_a_body_without_the_result_subject() {
    let err = parse_reasoning_graph("<http://ex/s> <http://ex/p> <http://ex/o> .\n")
        .expect_err("a body without a ReasoningResult subject must fail");
    assert!(err.contains("no logic:ReasoningResult subject"), "{err}");
}
