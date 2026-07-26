// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Unit tests for the typed `logic:ReasoningResult` model (ME2).

use super::*;
use gmeow_logic_compile::ir::PreservationKind;

// ── Enum string round-trips (wire ↔ local-name ↔ variant) ──────────────────────

#[test]
fn input_status_round_trips() {
    for &v in InputStatus::ALL {
        assert_eq!(InputStatus::from_wire(v.wire()), Some(v));
        assert_eq!(InputStatus::from_local(v.local_name()), Some(v));
        assert_eq!(v.iri(), format!("{LOGIC_NAMESPACE}{}", v.local_name()));
        assert_eq!(v.to_string(), v.wire());
    }
}

#[test]
fn evaluation_status_round_trips() {
    for &v in EvaluationStatus::ALL {
        assert_eq!(EvaluationStatus::from_wire(v.wire()), Some(v));
        assert_eq!(EvaluationStatus::from_local(v.local_name()), Some(v));
    }
    assert_eq!(EvaluationStatus::BudgetExhausted.wire(), "budget-exhausted");
    assert_eq!(
        EvaluationStatus::BudgetExhausted.local_name(),
        "BudgetExhausted"
    );
}

#[test]
fn completeness_status_round_trips() {
    for &v in CompletenessStatus::ALL {
        assert_eq!(CompletenessStatus::from_wire(v.wire()), Some(v));
        assert_eq!(CompletenessStatus::from_local(v.local_name()), Some(v));
    }
    // "unknown" is the wire form but the individual is disambiguated.
    assert_eq!(CompletenessStatus::Unknown.wire(), "unknown");
    assert_eq!(
        CompletenessStatus::Unknown.local_name(),
        "CompletenessUnknown"
    );
}

#[test]
fn information_state_round_trips() {
    for &v in InformationState::ALL {
        assert_eq!(InformationState::from_wire(v.wire()), Some(v));
        assert_eq!(InformationState::from_local(v.local_name()), Some(v));
    }
    assert_eq!(InformationState::NotEvaluated.wire(), "not-evaluated");
}

#[test]
fn unknown_strings_parse_to_none() {
    assert_eq!(InputStatus::from_wire("nope"), None);
    assert_eq!(EvaluationStatus::from_local("Nope"), None);
    assert_eq!(InformationState::from_wire("maybe"), None);
}

// ── The conclusiveness invariant ───────────────────────────────────────────────

fn prov() -> ResultProvenance {
    ResultProvenance::native("contract-hash-0", "http://gmeow.example/w")
}

#[test]
fn classify_empty_quadrant_is_neither_only_when_conclusive() {
    // Conclusive + no proof + no counterproof => Neither.
    assert_eq!(
        InformationState::classify(false, false, true, true),
        InformationState::Neither
    );
    // Non-conclusive + no witnesses => Undetermined (NOT Neither).
    assert_eq!(
        InformationState::classify(false, false, false, true),
        InformationState::Undetermined
    );
}

#[test]
fn classify_no_semantics_is_not_evaluated() {
    // semantics unavailable always wins, regardless of conclusiveness/witnesses.
    assert_eq!(
        InformationState::classify(true, false, true, false),
        InformationState::NotEvaluated
    );
    assert_eq!(
        InformationState::classify(false, false, true, false),
        InformationState::NotEvaluated
    );
}

#[test]
fn classify_belnap_quadrants() {
    assert_eq!(
        InformationState::classify(true, false, true, true),
        InformationState::Supported
    );
    assert_eq!(
        InformationState::classify(false, true, true, true),
        InformationState::Opposed
    );
    assert_eq!(
        InformationState::classify(true, true, true, true),
        InformationState::Both
    );
}

#[test]
fn budget_exhausted_no_witnesses_is_undetermined_never_neither() {
    // A budget-exhausted run with no witnesses must be Undetermined, never Neither
    // (SEMANTICS:312-318). Build it through the public constructor and validate.
    let result = ReasoningResult::new(
        InputStatus::Valid,
        EvaluationStatus::BudgetExhausted,
        CompletenessStatus::Incomplete,
        PreservationClaim::exact(),
        InformationState::Undetermined,
        prov(),
        ResultPayload::Empty,
    );
    assert!(!result.is_conclusive());
    assert!(result.validate().is_ok());
}

#[test]
fn neither_without_conclusive_fails_validate() {
    let result = ReasoningResult {
        input: InputStatus::Valid,
        evaluation: EvaluationStatus::BudgetExhausted,
        completeness: CompletenessStatus::Incomplete,
        preservation: PreservationClaim::exact(),
        information: InformationState::Neither,
        provenance: prov(),
        payload: ResultPayload::Empty,
        row_schema: None,
    };
    assert!(
        result.validate().is_err(),
        "Neither without a conclusive evaluation must be rejected"
    );
}

#[test]
fn undetermined_not_evaluated_neither_are_distinct() {
    // The three non-positive states are never interchangeable.
    assert_ne!(
        InformationState::Undetermined,
        InformationState::NotEvaluated
    );
    assert_ne!(InformationState::Undetermined, InformationState::Neither);
    assert_ne!(InformationState::NotEvaluated, InformationState::Neither);
    assert_ne!(
        InformationState::Undetermined.wire(),
        InformationState::NotEvaluated.wire()
    );
}

#[test]
fn both_without_witness_fails_validate() {
    let result = ReasoningResult {
        input: InputStatus::Valid,
        evaluation: EvaluationStatus::Completed,
        completeness: CompletenessStatus::CompleteForFragment,
        preservation: PreservationClaim::exact(),
        information: InformationState::Both,
        provenance: prov(),
        payload: ResultPayload::Empty,
        row_schema: None,
    };
    assert!(
        result.validate().is_err(),
        "Both must carry a justifying proof/counterproof or witness"
    );
}

#[test]
fn both_proof_only_no_counterproof_fails_validate() {
    // proof=Some, counterproof=None, witnesses=[] — a lone proof is not a glut.
    let mut prov_with_proof = prov();
    prov_with_proof.proof = Some(dref("lone-proof"));
    let result = ReasoningResult {
        input: InputStatus::Valid,
        evaluation: EvaluationStatus::Completed,
        completeness: CompletenessStatus::CompleteForFragment,
        preservation: PreservationClaim::exact(),
        information: InformationState::Both,
        provenance: prov_with_proof,
        payload: ResultPayload::Empty,
        row_schema: None,
    };
    assert!(
        result.validate().is_err(),
        "Both with only a proof (no counterproof, no witnesses) must be rejected"
    );
}

#[test]
fn both_witness_only_no_proof_counterproof_validates() {
    // proof=None, counterproof=None, witnesses=[one] — the DL path; must pass.
    let mut prov_with_witness = prov();
    prov_with_witness.contradiction_witnesses = vec![ContradictionWitness {
        individual: "http://gmeow.example/x".to_owned(),
        world: "http://gmeow.example/w".to_owned(),
        premises: vec![(
            "http://gmeow.example/x".to_owned(),
            "http://www.w3.org/1999/02/22-rdf-syntax-ns#type".to_owned(),
            "http://gmeow.example/A".to_owned(),
        )],
    }];
    let result = ReasoningResult {
        input: InputStatus::Valid,
        evaluation: EvaluationStatus::Completed,
        completeness: CompletenessStatus::CompleteForFragment,
        preservation: PreservationClaim::exact(),
        information: InformationState::Both,
        provenance: prov_with_witness,
        payload: ResultPayload::Empty,
        row_schema: None,
    };
    assert!(
        result.validate().is_ok(),
        "Both with a contradiction witness (DL path) must be valid"
    );
}

// ── PreservationClaim ──────────────────────────────────────────────────────────

#[test]
fn preservation_rejects_validation_only() {
    let mut claim = PreservationClaim::default();
    assert!(claim.insert(PreservationKind::Exact).is_ok());
    assert!(
        claim.insert(PreservationKind::ValidationOnly).is_err(),
        "ValidationOnly is not an answer-preservation polarity"
    );
    // A directly-constructed set containing ValidationOnly must fail validate().
    let mut bad = PreservationClaim::default();
    bad.polarities.insert(PreservationKind::ValidationOnly);
    assert!(bad.validate().is_err());
}

#[test]
fn preservation_loss_affected_is_derived() {
    let mut claim = PreservationClaim::exact();
    claim
        .unsupported_constructs
        .insert("http://www.w3.org/2002/07/owl#someValuesFrom".to_owned());
    let mut query_uses: BTreeSet<String> = BTreeSet::new();
    query_uses.insert("http://www.w3.org/2002/07/owl#someValuesFrom".to_owned());
    assert!(claim.is_loss_affected(&query_uses));

    let mut unrelated: BTreeSet<String> = BTreeSet::new();
    unrelated.insert("http://www.w3.org/2002/07/owl#hasValue".to_owned());
    assert!(!claim.is_loss_affected(&unrelated));
}

#[test]
fn preservation_exact_is_exact_singleton() {
    let claim = PreservationClaim::exact();
    assert_eq!(claim.polarities.len(), 1);
    assert!(claim.polarities.contains(&PreservationKind::Exact));
    assert!(claim.unsupported_constructs.is_empty());
}

// ── invalid() constructor ──────────────────────────────────────────────────────

#[test]
fn invalid_pins_vacuous_fields() {
    let r = ReasoningResult::invalid("bad syntax", prov());
    assert_eq!(r.input, InputStatus::Invalid);
    assert_eq!(r.evaluation, EvaluationStatus::Unsupported);
    assert_eq!(r.completeness, CompletenessStatus::Unknown);
    assert_eq!(r.information, InformationState::NotEvaluated);
    assert_eq!(r.provenance.conclusion, "bad syntax");
    assert!(r.validate().is_ok());
}

// ── from_dl_verdict fold ───────────────────────────────────────────────────────

use crate::reason::dl::{DlCoverage, DlVerdict, InconsistencyWitness};

fn verdict(consistent: bool, unsupported: Vec<&str>) -> DlVerdict {
    DlVerdict {
        consistent,
        unsatisfiable_classes: Vec::new(),
        inconsistencies: if consistent {
            Vec::new()
        } else {
            vec![InconsistencyWitness {
                individual: "http://gmeow.example/x".to_owned(),
                world: "http://gmeow.example/w".to_owned(),
                premises: vec![(
                    "http://gmeow.example/x".to_owned(),
                    "http://www.w3.org/1999/02/22-rdf-syntax-ns#type".to_owned(),
                    "http://gmeow.example/A".to_owned(),
                )],
            }]
        },
        coverage: DlCoverage {
            present: Vec::new(),
            decided: Vec::new(),
            unsupported: unsupported.into_iter().map(str::to_owned).collect(),
        },
        gaps: Vec::new(),
        boundary_findings: Vec::new(),
    }
}

#[test]
fn from_dl_verdict_consistent_complete_is_supported() {
    let r = ReasoningResult::from_dl_verdict(Vec::new(), &verdict(true, vec![]), prov());
    assert_eq!(r.input, InputStatus::Valid);
    assert_eq!(r.evaluation, EvaluationStatus::Completed);
    assert_eq!(r.completeness, CompletenessStatus::CompleteForFragment);
    assert_eq!(r.information, InformationState::Supported);
    assert!(r.preservation.polarities.contains(&PreservationKind::Exact));
    assert!(r.validate().is_ok());
}

#[test]
fn from_dl_verdict_inconsistent_is_both_with_witnesses() {
    let r = ReasoningResult::from_dl_verdict(Vec::new(), &verdict(false, vec![]), prov());
    assert_eq!(r.information, InformationState::Both);
    assert_eq!(r.provenance.contradiction_witnesses.len(), 1);
    assert_eq!(
        r.provenance.contradiction_witnesses[0].individual,
        "http://gmeow.example/x"
    );
    assert!(r.validate().is_ok());
}

#[test]
fn from_dl_verdict_unsupported_constructs_are_undetermined_not_wrong_consistent() {
    let r = ReasoningResult::from_dl_verdict(
        Vec::new(),
        &verdict(true, vec!["http://www.w3.org/2002/07/owl#someValuesFrom"]),
        prov(),
    );
    assert_eq!(r.completeness, CompletenessStatus::Incomplete);
    assert!(
        r.preservation
            .unsupported_constructs
            .contains("http://www.w3.org/2002/07/owl#someValuesFrom")
    );
    assert!(
        r.preservation
            .polarities
            .contains(&PreservationKind::SoundUnder)
    );
    // The "consistent" verdict here rests on IGNORING the undecided construct —
    // there is no genuine proof of satisfiability, so the honest state is
    // Undetermined (cannot-decide), NEVER a wrong Supported. (Incomplete-never-
    // wrong: a positive consistency verdict would be unsound.)
    assert_eq!(r.information, InformationState::Undetermined);
    assert!(!r.is_decided_consistent());
    assert!(r.validate().is_ok());
}

// ── Proof/counterproof schema (from_explanation + from_query) ──────────────────

fn dref(id: &str) -> DerivationRef {
    let mut cited = BTreeSet::new();
    cited.insert(format!("{id}-cite-a"));
    cited.insert(format!("{id}-cite-b"));
    DerivationRef {
        derivation_id: id.to_owned(),
        cited_iris: cited,
    }
}

#[test]
fn derivation_ref_from_explanation_copies_id_and_cites() {
    let mut cited = BTreeSet::new();
    cited.insert("http://gmeow.example/q1".to_owned());
    let explanation = crate::explain::Explanation {
        target_derivation_id: "http://gmeow.example/d1".to_owned(),
        target_quad_reifier: "http://gmeow.example/r1".to_owned(),
        world_iri: "http://gmeow.example/w".to_owned(),
        step_skeleton: vec![],
        cited_iris: cited.clone(),
    };
    let r = DerivationRef::from_explanation(&explanation);
    assert_eq!(r.derivation_id, "http://gmeow.example/d1");
    assert_eq!(r.cited_iris, cited);
}

fn query(
    proof: Option<DerivationRef>,
    counterproof: Option<DerivationRef>,
    evaluation: EvaluationStatus,
    completeness: CompletenessStatus,
    semantics_available: bool,
) -> ReasoningResult {
    ReasoningResult::from_query(
        ResultPayload::Bindings(vec![]),
        proof,
        counterproof,
        evaluation,
        completeness,
        PreservationClaim::exact(),
        semantics_available,
        prov(),
    )
}

#[test]
fn from_query_proof_only_is_supported() {
    let r = query(
        Some(dref("p")),
        None,
        EvaluationStatus::Completed,
        CompletenessStatus::CompleteForFragment,
        true,
    );
    assert_eq!(r.information, InformationState::Supported);
    assert!(r.provenance.proof.is_some());
    assert!(r.provenance.counterproof.is_none());
    assert!(r.validate().is_ok());
}

#[test]
fn from_query_counterproof_only_is_opposed() {
    let r = query(
        None,
        Some(dref("c")),
        EvaluationStatus::Completed,
        CompletenessStatus::CompleteForFragment,
        true,
    );
    assert_eq!(r.information, InformationState::Opposed);
}

#[test]
fn from_query_both_is_glut_and_validates() {
    let r = query(
        Some(dref("p")),
        Some(dref("c")),
        EvaluationStatus::Completed,
        CompletenessStatus::CompleteForFragment,
        true,
    );
    assert_eq!(r.information, InformationState::Both);
    // The proof + counterproof are themselves the glut witnesses validate() needs.
    assert!(r.validate().is_ok());
}

#[test]
fn from_query_none_conclusive_is_neither() {
    let r = query(
        None,
        None,
        EvaluationStatus::Completed,
        CompletenessStatus::CompleteForFragment,
        true,
    );
    assert_eq!(r.information, InformationState::Neither);
    assert!(r.validate().is_ok());
}

#[test]
fn from_query_none_nonconclusive_is_undetermined() {
    let r = query(
        None,
        None,
        EvaluationStatus::BudgetExhausted,
        CompletenessStatus::Incomplete,
        true,
    );
    assert_eq!(r.information, InformationState::Undetermined);
}

#[test]
fn from_query_no_semantics_is_not_evaluated() {
    let r = query(
        Some(dref("p")),
        None,
        EvaluationStatus::Unsupported,
        CompletenessStatus::Unknown,
        false,
    );
    assert_eq!(r.information, InformationState::NotEvaluated);
}

// ── BudgetLimit lossless discriminator ─────────────────────────────────────────

#[test]
fn budget_limit_wires() {
    assert_eq!(BudgetLimit::Answers.wire(), "answers");
    assert_eq!(BudgetLimit::Inference.wire(), "inference");
    assert_eq!(BudgetLimit::Depth.wire(), "depth");
}
