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

// ── Finding 1: witness-order determinism ─────────────────────────────────────

/// Two ReasoningResults with the SAME witness set but in DIFFERENT order must
/// produce the exact same RDF bytes (same digest, same blank-node numbering).
#[test]
fn witness_order_does_not_affect_projection() {
    let w1 = ContradictionWitness {
        individual: "http://ex/A".to_owned(),
        world: "http://ex/world/w1".to_owned(),
        premises: vec![(
            "http://ex/A".to_owned(),
            "http://www.w3.org/1999/02/22-rdf-syntax-ns#type".to_owned(),
            "http://ex/TypeA".to_owned(),
        )],
    };
    let w2 = ContradictionWitness {
        individual: "http://ex/B".to_owned(),
        world: "http://ex/world/w2".to_owned(),
        premises: vec![],
    };

    let mut base_prov = ResultProvenance::native("contract:witness-order", "http://ex/world/w1");
    base_prov.query = "ASK { }".to_owned();
    base_prov.conclusion = "test".to_owned();

    let r1 = ReasoningResult::new(
        InputStatus::Valid,
        EvaluationStatus::Completed,
        CompletenessStatus::CompleteForFragment,
        PreservationClaim::default(),
        InformationState::Supported,
        {
            let mut p = base_prov.clone();
            p.contradiction_witnesses = vec![w1.clone(), w2.clone()];
            p
        },
        ResultPayload::Empty,
    );
    let r2 = ReasoningResult::new(
        InputStatus::Valid,
        EvaluationStatus::Completed,
        CompletenessStatus::CompleteForFragment,
        PreservationClaim::default(),
        InformationState::Supported,
        {
            let mut p = base_prov.clone();
            // Reverse order relative to r1.
            p.contradiction_witnesses = vec![w2.clone(), w1.clone()];
            p
        },
        ResultPayload::Empty,
    );

    let body1 = project_reasoning_result(&r1);
    let body2 = project_reasoning_result(&r2);
    assert_eq!(
        body1, body2,
        "witness permutations must produce the same RDF projection"
    );
    // The content-addressed node IRI is also identical.
    assert_eq!(result_node_iri(&r1), result_node_iri(&r2));
}

// ── Finding 2: fail-closed provenance round-trip ──────────────────────────────

/// A body that is missing a required provenance field (resultQuery) must parse
/// to Err — no silent default substitution.
#[test]
fn parse_fails_closed_on_missing_result_query() {
    // Build a valid projection then strip out the resultQuery triple.
    let body = project_reasoning_result(&rich_result());
    let stripped: String = body
        .lines()
        .filter(|l| !l.contains("resultQuery"))
        .map(|l| format!("{l}\n"))
        .collect();
    let err = parse_reasoning_graph(&stripped).expect_err("missing resultQuery must return Err");
    assert!(
        err.contains("resultQuery"),
        "error must name the missing field: {err}"
    );
}

/// Same for resultConclusion.
#[test]
fn parse_fails_closed_on_missing_result_conclusion() {
    let body = project_reasoning_result(&rich_result());
    let stripped: String = body
        .lines()
        .filter(|l| !l.contains("resultConclusion"))
        .map(|l| format!("{l}\n"))
        .collect();
    let err =
        parse_reasoning_graph(&stripped).expect_err("missing resultConclusion must return Err");
    assert!(
        err.contains("resultConclusion"),
        "error must name the missing field: {err}"
    );
}

/// Same for resultBudgetConsumed.
#[test]
fn parse_fails_closed_on_missing_result_budget_consumed() {
    let body = project_reasoning_result(&rich_result());
    let stripped: String = body
        .lines()
        .filter(|l| !l.contains("resultBudgetConsumed"))
        .map(|l| format!("{l}\n"))
        .collect();
    let err =
        parse_reasoning_graph(&stripped).expect_err("missing resultBudgetConsumed must return Err");
    assert!(
        err.contains("resultBudgetConsumed"),
        "error must name the missing field: {err}"
    );
}

// ── Finding 3: derived-axiom blank-node and literal round-trip ───────────────

/// Axioms with blank-node subjects/objects and literal-summary objects must
/// survive the projection → parse round-trip (they were previously silently
/// dropped as empty strings because only object_iri() was consulted).
#[test]
fn derived_axiom_blank_and_literal_terms_round_trip() {
    use crate::reason::el::InferredAxiom;

    // An axiom whose object is a blank node (as the native chase would emit).
    let blank_axiom = InferredAxiom {
        subject: "http://ex/S".to_owned(),
        predicate: "http://ex/p".to_owned(),
        object: "_:anon0".to_owned(), // blank-node term
        world: "http://ex/world/w".to_owned(),
        is_edb: false,
        rule_name: None,
        premises: vec![],
    };
    // An axiom whose object is a literal-summary string.
    let lit_axiom = InferredAxiom {
        subject: "http://ex/S".to_owned(),
        predicate: "http://ex/q".to_owned(),
        object: "\"hello world\"".to_owned(), // literal-summary term
        world: "http://ex/world/w".to_owned(),
        is_edb: false,
        rule_name: None,
        premises: vec![],
    };

    let mut prov = ResultProvenance::native("contract:axiom-rt", "http://ex/world/w");
    prov.query = "ASK { }".to_owned();
    prov.conclusion = "test".to_owned();

    let result = ReasoningResult::new(
        InputStatus::Valid,
        EvaluationStatus::Completed,
        CompletenessStatus::CompleteForFragment,
        PreservationClaim::default(),
        InformationState::Supported,
        prov,
        ResultPayload::Inferred(vec![blank_axiom, lit_axiom]),
    );

    let body = project_reasoning_result(&result);
    let parsed = parse_reasoning_graph(&body).expect("parse must succeed");
    let ResultPayload::Inferred(rows) = &parsed.payload else {
        panic!("payload must be Inferred");
    };
    assert_eq!(rows.len(), 2, "both derived axioms must survive");

    // Find the axiom that had the blank-node object.
    let blank_row = rows
        .iter()
        .find(|r| r.predicate == "http://ex/p")
        .expect("blank-node axiom must survive");
    assert_eq!(
        blank_row.object, "_:anon0",
        "blank-node object must round-trip as _:label"
    );

    // Find the axiom that had the literal object.
    let lit_row = rows
        .iter()
        .find(|r| r.predicate == "http://ex/q")
        .expect("literal-summary axiom must survive");
    assert_eq!(
        lit_row.object, "\"hello world\"",
        "literal-summary object must round-trip verbatim"
    );
}
