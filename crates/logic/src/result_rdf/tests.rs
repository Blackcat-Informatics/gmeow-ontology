// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Tests for the `graph/reasoning` RDF projection (C7): determinism, axis-IRI
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
fn modal_derived_row_round_trips_its_complete_receipt() {
    let mut result = rich_result();
    let ResultPayload::Inferred(axioms) = &mut result.payload else {
        panic!("fixture payload must be inferred");
    };
    let modal = &mut axioms[0];
    modal.subject = "https://example.org/modal/F".to_owned();
    modal.predicate = crate::modal::MODAL_NECESSITY_FAILS.to_owned();
    modal.object = "<https://example.org/modal/B>".to_owned();
    modal.world = "https://example.org/modal/w0".to_owned();
    modal.rule_name = Some(crate::modal::MODAL_RULE_IRI.to_owned());
    modal.premises = vec![(
        "https://example.org/modal/a".to_owned(),
        "https://example.org/modal/knows".to_owned(),
        "<https://example.org/modal/b>".to_owned(),
    )];
    let expected = modal.clone();
    let receipt = row_for_axiom(&expected);

    let body = project_reasoning_result(&result);
    assert!(body.contains(crate::modal::MODAL_RULE_IRI));
    assert!(body.contains(&receipt.derivation_id));
    for source in &receipt.source_quad_ids {
        assert!(body.contains(source));
    }
    assert!(body.contains(&format!("<{PROV_VALUE}>")));

    let parsed = parse_reasoning_graph(&body).expect("receipt-bearing graph parses");
    let ResultPayload::Inferred(rows) = parsed.payload else {
        panic!("payload must remain inferred");
    };
    let actual = rows
        .iter()
        .find(|row| row.predicate == crate::modal::MODAL_NECESSITY_FAILS)
        .expect("modal row round-trips");
    assert_eq!(actual, &expected);
}

#[test]
fn derived_row_parser_rejects_a_tampered_source_receipt() {
    let mut result = rich_result();
    let ResultPayload::Inferred(axioms) = &mut result.payload else {
        panic!("fixture payload must be inferred");
    };
    axioms[0].premises = vec![(
        "https://example.org/a".to_owned(),
        "https://example.org/p".to_owned(),
        "<https://example.org/o>".to_owned(),
    )];
    let body = project_reasoning_result(&result);
    let tampered: String = body
        .lines()
        .filter(|line| !line.contains(PROV_VALUE))
        .map(|line| format!("{line}\n"))
        .collect();
    let err = parse_reasoning_graph(&tampered).unwrap_err();
    assert!(err.message().contains("source reifier"), "got: {err}");
}

#[test]
fn derived_row_round_trip_preserves_duplicate_source_multiplicity() {
    let mut result = rich_result();
    let ResultPayload::Inferred(axioms) = &mut result.payload else {
        panic!("fixture payload must be inferred");
    };
    let premise = (
        "https://example.org/a".to_owned(),
        "https://example.org/p".to_owned(),
        "<https://example.org/o>".to_owned(),
    );
    axioms[0].premises = vec![premise.clone(), premise];
    axioms[0].object = "<http://ex/Animal>".to_owned();
    let expected = axioms[0].clone();
    let receipt = row_for_axiom(&expected);

    let body = project_reasoning_result(&result);
    assert!(body.contains(&receipt.derivation_id));
    let parsed = parse_reasoning_graph(&body).expect("duplicate-source receipt parses");
    let ResultPayload::Inferred(rows) = parsed.payload else {
        panic!("payload must remain inferred");
    };
    assert_eq!(rows[0], expected);
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
    assert!(
        err.message().contains("no logic:ReasoningResult subject"),
        "{err}"
    );
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
        err.message().contains("resultQuery"),
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
        err.message().contains("resultConclusion"),
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
        err.message().contains("resultBudgetConsumed"),
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

// ── Conjecture verdict → attributed RDF projection ──────────────────────────────

use crate::conjecture::{ConjectureAnswer, ConjectureDischarge, ConjectureLifecycleState};

/// A REFUTED conjecture answer: information=Opposed, a concrete `ContradictionWitness`
/// with an individual, a world, and two premises, lifecycle RefutedInStandpoint.
fn refuted_answer(standpoint: &str) -> ConjectureAnswer {
    let witness = ContradictionWitness {
        individual: "http://ex/felix".to_owned(),
        world: "http://ex/world1".to_owned(),
        premises: vec![
            (
                "http://ex/felix".to_owned(),
                "http://ex/type".to_owned(),
                "http://ex/Cat".to_owned(),
            ),
            (
                "http://ex/felix".to_owned(),
                "http://ex/type".to_owned(),
                "http://ex/Dog".to_owned(),
            ),
        ],
    };
    let mut prov = ResultProvenance::native("contract:conj-1".to_owned(), "http://ex/world1");
    prov.context.standpoint = Some(standpoint.to_owned());
    prov.contradiction_witnesses = vec![witness.clone()];
    prov.projection_class = PreservationClaim::exact();
    let verdict = ReasoningResult::new(
        InputStatus::Valid,
        EvaluationStatus::Completed,
        CompletenessStatus::CompleteForFragment,
        PreservationClaim::exact(),
        InformationState::Opposed,
        prov,
        ResultPayload::Empty,
    );
    verdict.validate().expect("refuted verdict must validate");
    ConjectureAnswer {
        verdict,
        witness: Some(witness),
        lifecycle: ConjectureLifecycleState::RefutedInStandpoint,
        discharge: ConjectureDischarge::Discharged,
        scenario_world: "http://ex/world1".to_owned(),
    }
}

/// The refuted conjecture's principal predicate — the anti-conjecture obligation's forbidden
/// predicate the caller supplies for a refuted answer.
const REFUTED_PREDICATE: &str = "http://ex/refutedRelation";

/// A CORROBORATED conjecture answer: information=Supported, no witness, lifecycle
/// Corroborated / discharge Discharged — the input that feeds the POSITIVE promotion leg.
fn corroborated_answer(standpoint: &str) -> ConjectureAnswer {
    let mut prov = ResultProvenance::native("contract:conj-1".to_owned(), "http://ex/world1");
    prov.context.standpoint = Some(standpoint.to_owned());
    prov.projection_class = PreservationClaim::exact();
    let verdict = ReasoningResult::new(
        InputStatus::Valid,
        EvaluationStatus::Completed,
        CompletenessStatus::CompleteForFragment,
        PreservationClaim::exact(),
        InformationState::Supported,
        prov,
        ResultPayload::Empty,
    );
    verdict
        .validate()
        .expect("corroborated verdict must validate");
    ConjectureAnswer {
        verdict,
        witness: None,
        lifecycle: ConjectureLifecycleState::Corroborated,
        discharge: ConjectureDischarge::Discharged,
        scenario_world: "http://ex/world1".to_owned(),
    }
}

/// An OPEN conjecture answer: a conclusive independence (information=Neither, complete for
/// the fragment), lifecycle Open / discharge Discharged — feeds NEITHER promotion leg.
fn open_answer(standpoint: &str) -> ConjectureAnswer {
    let mut prov = ResultProvenance::native("contract:conj-1".to_owned(), "http://ex/world1");
    prov.context.standpoint = Some(standpoint.to_owned());
    prov.projection_class = PreservationClaim::exact();
    let verdict = ReasoningResult::new(
        InputStatus::Valid,
        EvaluationStatus::Completed,
        CompletenessStatus::CompleteForFragment,
        PreservationClaim::exact(),
        InformationState::Neither,
        prov,
        ResultPayload::Empty,
    );
    verdict.validate().expect("open verdict must validate");
    ConjectureAnswer {
        verdict,
        witness: None,
        lifecycle: ConjectureLifecycleState::Open,
        discharge: ConjectureDischarge::Discharged,
        scenario_world: "http://ex/world1".to_owned(),
    }
}

#[test]
fn conjecture_verdict_round_trips() {
    let answer = refuted_answer("http://ex/standpointA");
    let input = ConjectureVerdictInput {
        content_key: "formula:phi-alpha-normalized",
        standpoint: "http://ex/standpointA",
        kb_world: "http://ex/kb-world-42",
        answer: &answer,
        math_conjecture: None,
        forbidden_predicate: Some(REFUTED_PREDICATE),
    };
    let body = project_conjecture_verdict(&input);
    let record = parse_conjecture_verdict(&body).expect("parse conjecture verdict");

    assert_eq!(record.content_key, "formula:phi-alpha-normalized");
    assert_eq!(record.standpoint, "http://ex/standpointA");
    assert_eq!(record.kb_world_hash, sha256_hex("http://ex/kb-world-42"));
    assert_eq!(
        record.lifecycle,
        ConjectureLifecycleState::RefutedInStandpoint
    );
    assert_eq!(record.discharge, ConjectureDischarge::Discharged);
    // The embedded reasoning-result info state survives.
    assert_eq!(record.verdict.information, InformationState::Opposed);
    // The witness (individual/world/2 sorted premises) is recovered.
    let w = record.witness.expect("refutation witness must round-trip");
    assert_eq!(w.individual, "http://ex/felix");
    assert_eq!(w.world, "http://ex/world1");
    assert_eq!(w.premises.len(), 2, "both premises must round-trip");
    assert!(w.premises.contains(&(
        "http://ex/felix".to_owned(),
        "http://ex/type".to_owned(),
        "http://ex/Cat".to_owned()
    )));
    assert!(w.premises.contains(&(
        "http://ex/felix".to_owned(),
        "http://ex/type".to_owned(),
        "http://ex/Dog".to_owned()
    )));
    // The anti-conjecture obligation leg round-trips; the promotion leg is absent.
    assert!(
        record.promotion_candidate.is_none(),
        "a refuted verdict must NOT carry a promotion leg"
    );
    let obligation = record
        .obligation_candidate
        .expect("a refuted verdict must carry the anti-conjecture obligation leg");
    assert!(obligation.node.starts_with(OBLIGATION_CANDIDATE_IRI_BASE));
    assert_eq!(obligation.forbidden_predicate, REFUTED_PREDICATE);
    assert_eq!(
        obligation.discharge_conditions,
        vec![logic("DischargeFiniteClosure")]
    );
}

#[test]
fn conjecture_verdict_escapes_control_bearing_formula_identity() {
    let answer = corroborated_answer("http://ex/standpointA");
    let content_key = "ATOM\0I\0http://ex/relation\u{001f}\u{007f}\u{0085}";
    let body = project_conjecture_verdict(&ConjectureVerdictInput {
        content_key,
        standpoint: "http://ex/standpointA",
        kb_world: "http://ex/kb-world-42",
        answer: &answer,
        math_conjecture: None,
        forbidden_predicate: None,
    });

    assert!(body.contains("ATOM\\u0000I\\u0000http://ex/relation\\u001F\\u007F\\u0085"));
    assert!(
        !body
            .chars()
            .any(|c| c.is_control() && !matches!(c, '\n' | '\r' | '\t')),
        "the N-Triples transport must not contain raw control scalars"
    );

    let record = parse_conjecture_verdict(&body)
        .expect("the escaped control-bearing conjecture verdict must remain valid N-Triples");
    assert_eq!(record.content_key, content_key);
}

#[test]
fn promotion_leg_emitted_exactly_on_corroboration() {
    // Corroborated → the POSITIVE promotion leg to a well-typed FormalizationCandidate that
    // carries all eight universal candidate carriers; and NO anti-conjecture obligation leg.
    let answer = corroborated_answer("http://ex/standpointA");
    let input = ConjectureVerdictInput {
        content_key: "formula:corroborated-phi",
        standpoint: "http://ex/standpointA",
        kb_world: "http://ex/kb-world-42",
        answer: &answer,
        math_conjecture: None,
        forbidden_predicate: None,
    };
    let body = project_conjecture_verdict(&input);
    let record = parse_conjecture_verdict(&body).expect("parse corroborated verdict");
    assert_eq!(record.lifecycle, ConjectureLifecycleState::Corroborated);

    // No anti-conjecture obligation leg on a corroboration.
    assert!(
        record.obligation_candidate.is_none()
            && !body.contains(&logic("antiConjectureObligationCandidate")),
        "a corroborated verdict must NOT carry an anti-conjecture obligation leg"
    );
    // The promotion edge is present and points to the well-typed candidate node.
    let subject = conjecture_subject(&body);
    let promo = record
        .promotion_candidate
        .expect("a corroborated verdict must carry the promotion leg");
    assert!(promo.node.starts_with(PROMOTION_CANDIDATE_IRI_BASE));
    let edge = format!(
        "<{subject}> <{}> <{}> .",
        logic("conjecturePromotionCandidate"),
        promo.node
    );
    assert!(
        body.contains(&edge),
        "the promotion edge must be present; body:\n{body}"
    );
    let type_triple = format!(
        "<{}> <{RDF_TYPE}> <{}> .",
        promo.node,
        logic("FormalizationCandidate")
    );
    assert!(
        body.contains(&type_triple),
        "the promotion target must be typed logic:FormalizationCandidate"
    );
    // All eight universal candidate carriers are populated (SHACL-valid, not a bare stub).
    assert_eq!(
        promo.source_hash,
        format!("sha256:{}", sha256_hex("formula:corroborated-phi"))
    );
    assert_eq!(promo.scope, "formula:corroborated-phi");
    assert_eq!(promo.contract, logic("StratifiedNAFProfile"));
    assert_eq!(promo.category, logic("CategoryDerivationRule"));
    assert_eq!(promo.lifecycle, logic("CandidateProposed"));
    assert_eq!(promo.projection_behavior, logic("SoundUnderApproximation"));
    assert_eq!(promo.semantic_risk, logic("RiskCoreContaminating"));
    assert!(
        promo
            .extraction_provenance
            .contains("conjecture-test activity"),
        "extraction provenance must record the engine run"
    );
}

#[test]
fn obligation_leg_emitted_exactly_on_refutation() {
    // Refuted-in-standpoint → the anti-conjecture obligation leg to a well-typed
    // NonEntailmentObligation; and NO promotion leg.
    let answer = refuted_answer("http://ex/standpointA");
    let input = ConjectureVerdictInput {
        content_key: "formula:refuted-phi",
        standpoint: "http://ex/standpointA",
        kb_world: "http://ex/kb-world-42",
        answer: &answer,
        math_conjecture: None,
        forbidden_predicate: Some(REFUTED_PREDICATE),
    };
    let body = project_conjecture_verdict(&input);
    let record = parse_conjecture_verdict(&body).expect("parse refuted verdict");
    assert!(
        record.promotion_candidate.is_none()
            && !body.contains(&logic("conjecturePromotionCandidate")),
        "a refuted verdict must NOT carry a promotion leg"
    );
    let subject = conjecture_subject(&body);
    let obl = record
        .obligation_candidate
        .expect("a refuted verdict must carry the anti-conjecture obligation leg");
    let edge = format!(
        "<{subject}> <{}> <{}> .",
        logic("antiConjectureObligationCandidate"),
        obl.node
    );
    assert!(
        body.contains(&edge),
        "the obligation edge must be present; body:\n{body}"
    );
    let type_triple = format!(
        "<{}> <{RDF_TYPE}> <{}> .",
        obl.node,
        logic("NonEntailmentObligation")
    );
    assert!(
        body.contains(&type_triple),
        "the obligation target must be typed logic:NonEntailmentObligation"
    );
    assert_eq!(obl.forbidden_predicate, REFUTED_PREDICATE);
    assert_eq!(
        obl.discharge_conditions,
        vec![logic("DischargeFiniteClosure")]
    );
}

#[test]
fn open_verdict_emits_neither_leg() {
    // Open (conclusive independence) → NEITHER leg.
    let answer = open_answer("http://ex/standpointA");
    let input = ConjectureVerdictInput {
        content_key: "formula:open-phi",
        standpoint: "http://ex/standpointA",
        kb_world: "http://ex/kb-world-42",
        answer: &answer,
        math_conjecture: None,
        forbidden_predicate: None,
    };
    let body = project_conjecture_verdict(&input);
    let record = parse_conjecture_verdict(&body).expect("parse open verdict");
    assert_eq!(record.lifecycle, ConjectureLifecycleState::Open);
    assert!(
        record.promotion_candidate.is_none() && record.obligation_candidate.is_none(),
        "an open verdict must carry neither promotion leg"
    );
    assert!(!body.contains(&logic("conjecturePromotionCandidate")));
    assert!(!body.contains(&logic("antiConjectureObligationCandidate")));
}

/// Extract the subject IRI of the `rdf:type logic:Conjecture` triple from a body.
fn conjecture_subject(body: &str) -> String {
    let type_pred = format!("<{RDF_TYPE}>");
    let type_obj = format!("<{}>", logic("Conjecture"));
    for line in body.lines() {
        if line.contains(&type_pred) && line.contains(&type_obj) {
            let rest = line.strip_prefix('<').expect("iri subject");
            let end = rest.find('>').expect("iri subject close");
            return rest[..end].to_owned();
        }
    }
    panic!("no logic:Conjecture subject in body");
}

#[test]
fn two_standpoints_mint_two_distinct_nodes() {
    let answer_a = refuted_answer("http://ex/standpointA");
    let answer_b = refuted_answer("http://ex/standpointB");
    let body_a = project_conjecture_verdict(&ConjectureVerdictInput {
        content_key: "formula:same-phi",
        standpoint: "http://ex/standpointA",
        kb_world: "http://ex/kb-world-shared",
        answer: &answer_a,
        math_conjecture: None,
        forbidden_predicate: Some(REFUTED_PREDICATE),
    });
    let body_b = project_conjecture_verdict(&ConjectureVerdictInput {
        content_key: "formula:same-phi",
        standpoint: "http://ex/standpointB",
        kb_world: "http://ex/kb-world-shared",
        answer: &answer_b,
        math_conjecture: None,
        forbidden_predicate: Some(REFUTED_PREDICATE),
    });
    let subj_a = conjecture_subject(&body_a);
    let subj_b = conjecture_subject(&body_b);
    assert_ne!(
        subj_a, subj_b,
        "the same formula in two standpoints must mint two distinct content-addressed nodes"
    );
    assert!(subj_a.starts_with(CONJECTURE_IRI_BASE));
    assert!(subj_b.starts_with(CONJECTURE_IRI_BASE));
}

#[test]
fn conjecture_projection_is_byte_deterministic() {
    let answer = refuted_answer("http://ex/standpointA");
    let input = ConjectureVerdictInput {
        content_key: "formula:det-phi",
        standpoint: "http://ex/standpointA",
        kb_world: "http://ex/kb-world-42",
        answer: &answer,
        math_conjecture: None,
        forbidden_predicate: Some(REFUTED_PREDICATE),
    };
    let one = project_conjecture_verdict(&input);
    let two = project_conjecture_verdict(&input);
    assert_eq!(
        one, two,
        "same (content_key, standpoint, kb_world) must be byte-identical"
    );
    // The output is sorted N-Triples: assert the lines are in sorted order.
    let mut sorted = one.lines().collect::<Vec<_>>();
    let original = sorted.clone();
    sorted.sort();
    assert_eq!(
        sorted, original,
        "conjecture body lines must already be sorted"
    );
}

#[test]
fn different_kb_world_mints_distinct_node() {
    let answer = refuted_answer("http://ex/standpointA");
    let body_1 = project_conjecture_verdict(&ConjectureVerdictInput {
        content_key: "formula:same-phi",
        standpoint: "http://ex/standpointA",
        kb_world: "http://ex/kb-world-one",
        answer: &answer,
        math_conjecture: None,
        forbidden_predicate: Some(REFUTED_PREDICATE),
    });
    let body_2 = project_conjecture_verdict(&ConjectureVerdictInput {
        content_key: "formula:same-phi",
        standpoint: "http://ex/standpointA",
        kb_world: "http://ex/kb-world-two",
        answer: &answer,
        math_conjecture: None,
        forbidden_predicate: Some(REFUTED_PREDICATE),
    });
    assert_ne!(
        conjecture_subject(&body_1),
        conjecture_subject(&body_2),
        "the same formula in two KB worlds must mint two distinct nodes"
    );
}

#[test]
fn math_twin_edge_names_the_witness() {
    let answer = refuted_answer("http://ex/standpointA");
    let math_iri = "https://blackcatinformatics.ca/math/conjecture/goldbach";
    let body = project_conjecture_verdict(&ConjectureVerdictInput {
        content_key: "formula:phi",
        standpoint: "http://ex/standpointA",
        kb_world: "http://ex/kb-world-42",
        answer: &answer,
        math_conjecture: Some(math_iri),
        forbidden_predicate: Some(REFUTED_PREDICATE),
    });
    // Find the witness blank node (object of conjectureRefutationWitness).
    let refutation_pred = format!("<{}>", logic("conjectureRefutationWitness"));
    let witness_node = body
        .lines()
        .find(|l| l.contains(&refutation_pred))
        .and_then(|l| l.split_whitespace().nth(2))
        .expect("a refutation-witness link must be present")
        .to_owned();
    assert!(
        witness_node.starts_with("_:"),
        "witness object must be a blank node, got {witness_node}"
    );
    let expected = format!(
        "<{math_iri}> <{}> {witness_node} .",
        math("hasCounterexample")
    );
    assert!(
        body.contains(&expected),
        "the math twin edge `{expected}` must be present; body:\n{body}"
    );

    // The always-present structural twin bridge `math:conjectureUnderTest` links the math
    // statement (domain math:Conjecture) to THIS content-addressed logic:Conjecture node
    // (range logic:Conjecture) — emitted even on this refuted verdict, alongside the
    // refutation-only counterexample edge.
    let subject = conjecture_subject(&body);
    let under_test = format!(
        "<{math_iri}> <{}> <{subject}> .",
        math("conjectureUnderTest")
    );
    assert!(
        body.contains(&under_test),
        "the math:conjectureUnderTest twin bridge `{under_test}` must be present; body:\n{body}"
    );
    // And it round-trips.
    let record = parse_conjecture_verdict(&body).expect("parse refuted verdict with math twin");
    assert_eq!(
        record.math_conjecture.as_deref(),
        Some(math_iri),
        "the math:conjectureUnderTest bridge must round-trip"
    );
}

#[test]
fn conjecture_under_test_bridge_emitted_on_corroboration_and_absent_without_math() {
    // The structural twin is emitted for a CORROBORATED verdict too (not only refutations),
    // in the declared direction: math:Conjecture --conjectureUnderTest--> logic:Conjecture.
    let answer = corroborated_answer("http://ex/standpointA");
    let math_iri = "https://blackcatinformatics.ca/math/conjecture/twin-primes";
    let body = project_conjecture_verdict(&ConjectureVerdictInput {
        content_key: "formula:corroborated-phi",
        standpoint: "http://ex/standpointA",
        kb_world: "http://ex/kb-world-42",
        answer: &answer,
        math_conjecture: Some(math_iri),
        forbidden_predicate: None,
    });
    let subject = conjecture_subject(&body);
    let under_test = format!(
        "<{math_iri}> <{}> <{subject}> .",
        math("conjectureUnderTest")
    );
    assert!(
        body.contains(&under_test),
        "a corroborated verdict must carry the math:conjectureUnderTest bridge; body:\n{body}"
    );
    // A corroboration has no witness, so NO counterexample edge is emitted.
    assert!(
        !body.contains(&math("hasCounterexample")),
        "a corroborated verdict must NOT carry a math:hasCounterexample edge; body:\n{body}"
    );
    let record =
        parse_conjecture_verdict(&body).expect("parse corroborated verdict with math twin");
    assert_eq!(record.math_conjecture.as_deref(), Some(math_iri));

    // When no math statement is named, NEITHER math edge is emitted and the record's
    // math_conjecture is absent.
    let bare = project_conjecture_verdict(&ConjectureVerdictInput {
        content_key: "formula:corroborated-phi",
        standpoint: "http://ex/standpointA",
        kb_world: "http://ex/kb-world-42",
        answer: &answer,
        math_conjecture: None,
        forbidden_predicate: None,
    });
    assert!(
        !bare.contains(&math("conjectureUnderTest")) && !bare.contains(&math("hasCounterexample")),
        "no math edges may be emitted when no math_conjecture is supplied; body:\n{bare}"
    );
    let bare_record =
        parse_conjecture_verdict(&bare).expect("parse corroborated verdict without math twin");
    assert!(
        bare_record.math_conjecture.is_none(),
        "math_conjecture must be None when no bridge edge is present"
    );
}

#[test]
fn parse_rejects_body_without_conjecture_subject() {
    let err = parse_conjecture_verdict("<http://ex/s> <http://ex/p> <http://ex/o> .\n")
        .expect_err("a body without a logic:Conjecture subject must fail closed");
    assert!(
        err.message().contains("no logic:Conjecture subject"),
        "err was: {err}"
    );
}
