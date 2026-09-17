// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use crate::result::{
    InformationState, InputStatus, PreservationClaim, ResultPayload, ResultProvenance,
};

/// A consistent native result (no contradiction witnesses), with the given
/// evaluation/completeness axes.
fn consistent_result(
    evaluation: EvaluationStatus,
    completeness: CompletenessStatus,
) -> ReasoningResult {
    let mut provenance = ResultProvenance::native("contract:abc", "world:default");
    // A named certified fragment: a conclusive, violation-free check is only
    // entitled to a CoherenceCertificate when it names the fragment it ranges over.
    provenance.certified_fragment = Some("fragment:test".to_owned());
    ReasoningResult {
        input: InputStatus::Valid,
        evaluation,
        completeness,
        preservation: PreservationClaim::exact(),
        information: InformationState::Supported,
        provenance,
        payload: ResultPayload::Empty,
        row_schema: None,
    }
}

/// A glut result carrying one contradiction witness.
fn glut_result() -> ReasoningResult {
    let mut result = consistent_result(
        EvaluationStatus::Completed,
        CompletenessStatus::CompleteForFragment,
    );
    result.information = InformationState::Both;
    result
        .provenance
        .contradiction_witnesses
        .push(ContradictionWitness {
            individual: "https://example.org/clash".to_owned(),
            world: "world:default".to_owned(),
            premises: vec![],
        });
    result
}

#[test]
fn conclusive_consistent_yields_certificate() {
    let result = consistent_result(EvaluationStatus::Completed, CompletenessStatus::Unknown);
    let outcome = CoherenceOutcome::from_reasoning_result(
        &result,
        "blake3:bundle",
        ["blake3:axioms"],
        ContradictionPolicy::ForbidGapAndGlut,
        "2026-06-28T00:00:00Z",
        BTreeSet::new(),
    )
    .unwrap();
    assert!(outcome.issues_certificate());
    assert_eq!(outcome.class_local_name(), Some("CoherenceCertificate"));
}

#[test]
fn conclusive_consistent_without_fragment_downgrades_to_attestation() {
    // A conclusive, violation-free check that names NO certified fragment cannot
    // certify (no F to range over): it DOWNGRADES to an attestation, never a
    // fragment-less certificate.
    let mut result = consistent_result(
        EvaluationStatus::Completed,
        CompletenessStatus::CompleteForFragment,
    );
    result.provenance.certified_fragment = None;
    let outcome = CoherenceOutcome::from_reasoning_result(
        &result,
        "blake3:bundle",
        Vec::<String>::new(),
        ContradictionPolicy::ForbidGapAndGlut,
        "2026-06-28T00:00:00Z",
        BTreeSet::new(),
    )
    .unwrap();
    assert!(
        !outcome.issues_certificate(),
        "no fragment ⇒ no certificate"
    );
    assert!(matches!(outcome, CoherenceOutcome::Attestation(_)));
}

#[test]
fn bounded_incomplete_yields_attestation_never_certificate() {
    // GENUINELY non-conclusive: budget-exhausted AND incomplete (not merely
    // budget-exhausted, which is_conclusive() can still pass via complete-for-
    // fragment).
    let result = consistent_result(
        EvaluationStatus::BudgetExhausted,
        CompletenessStatus::Incomplete,
    );
    assert!(!result.is_conclusive());
    let outcome = CoherenceOutcome::from_reasoning_result(
        &result,
        "blake3:bundle",
        Vec::<String>::new(),
        ContradictionPolicy::ForbidGapAndGlut,
        "2026-06-28T00:00:00Z",
        BTreeSet::new(),
    )
    .unwrap();
    assert!(!outcome.issues_certificate());
    assert_eq!(
        outcome.class_local_name(),
        Some("CoherenceCheckAttestation")
    );
    assert!(matches!(outcome, CoherenceOutcome::Attestation(_)));
}

#[test]
fn budget_exhausted_but_complete_for_fragment_still_certifies() {
    // is_conclusive() is true via complete-for-fragment even though the run hit
    // a budget — the gate keys on is_conclusive(), not on evaluation alone.
    let result = consistent_result(
        EvaluationStatus::BudgetExhausted,
        CompletenessStatus::CompleteForFragment,
    );
    let outcome = CoherenceOutcome::from_reasoning_result(
        &result,
        "blake3:bundle",
        Vec::<String>::new(),
        ContradictionPolicy::ForbidGapAndGlut,
        "2026-06-28T00:00:00Z",
        BTreeSet::new(),
    )
    .unwrap();
    assert!(outcome.issues_certificate());
}

#[test]
fn permitted_glut_keeps_the_certificate() {
    // A glut under a glut-admitting contract is a permitted, disclosed conflict:
    // the certificate still issues and the conflict is recorded in the payload.
    let outcome = CoherenceOutcome::from_reasoning_result(
        &glut_result(),
        "blake3:bundle",
        Vec::<String>::new(),
        ContradictionPolicy::ForbidGap, // admits a glut
        "2026-06-28T00:00:00Z",
        BTreeSet::new(),
    )
    .unwrap();
    assert!(
        outcome.issues_certificate(),
        "permitted glut must still certify"
    );
    assert_eq!(outcome.payload().permitted_conflicts.len(), 1);
    assert!(outcome.payload().forbidden_violations.is_empty());
}

#[test]
fn forbidden_glut_refuses_and_issues_no_certificate() {
    // The same glut under a glut-FORBIDDING contract refutes coherence: no
    // certificate, no attestation; the violation is recorded for the findings.
    let outcome = CoherenceOutcome::from_reasoning_result(
        &glut_result(),
        "blake3:bundle",
        Vec::<String>::new(),
        ContradictionPolicy::ForbidGapAndGlut, // forbids a glut
        "2026-06-28T00:00:00Z",
        BTreeSet::new(),
    )
    .unwrap();
    assert!(outcome.is_refused());
    assert!(!outcome.issues_certificate());
    assert_eq!(outcome.class_local_name(), None);
    assert_eq!(outcome.payload().forbidden_violations.len(), 1);
    // A refusal emits no coherence artifact.
    assert!(outcome.to_nquads("https://example.org/g").is_empty());
}

#[test]
fn bounded_forbidden_glut_attests_and_discloses_the_forbidden_witness() {
    // A NON-conclusive (bounded/incomplete) check that ran into a glut under a
    // glut-FORBIDDING contract cannot refute coherence wholesale, but must
    // DISCLOSE what it found: an attestation carrying logic:forbiddenViolationWitness
    // (the property's producer — without this path the minted property is orphaned).
    let mut result = glut_result();
    result.evaluation = EvaluationStatus::BudgetExhausted;
    result.completeness = CompletenessStatus::Incomplete;
    assert!(!result.is_conclusive());
    let outcome = CoherenceOutcome::from_reasoning_result(
        &result,
        "blake3:bundle",
        Vec::<String>::new(),
        ContradictionPolicy::ForbidGapAndGlut, // forbids a glut
        "2026-06-28T00:00:00Z",
        BTreeSet::new(),
    )
    .unwrap();
    assert!(matches!(outcome, CoherenceOutcome::Attestation(_)));
    assert!(!outcome.issues_certificate());
    assert_eq!(outcome.payload().forbidden_violations.len(), 1);
    let nquads = outcome.to_nquads("https://example.org/g");
    assert!(nquads.contains(
            "<https://blackcatinformatics.ca/logic/forbiddenViolationWitness> <https://example.org/clash>"
        ));
}

#[test]
fn nquads_are_well_formed_and_byte_stable() {
    let outcome = CoherenceOutcome::from_reasoning_result(
        &glut_result(),
        "blake3:bundle",
        ["blake3:axioms"],
        ContradictionPolicy::ForbidGap,
        "2026-06-28T00:00:00Z",
        BTreeSet::new(),
    )
    .unwrap();
    let graph = "https://blackcatinformatics.ca/gmeow/graph/attestations";
    let nquads = outcome.to_nquads(graph);
    // Re-running with the same inputs is byte-identical (determinism).
    assert_eq!(nquads, outcome.to_nquads(graph));
    // Typed as a certificate, carries the payload, lands in the named graph.
    assert!(nquads.contains("<https://blackcatinformatics.ca/logic/CoherenceCertificate>"));
    assert!(nquads.contains("<https://blackcatinformatics.ca/logic/bundleHash>"));
    assert!(nquads.contains("<https://blackcatinformatics.ca/logic/contradictionPolicy> <https://blackcatinformatics.ca/logic/ForbidGap>"));
    assert!(nquads.contains("<https://blackcatinformatics.ca/logic/permittedConflictWitness> <https://example.org/clash>"));
    // The certificate links to the logic:ReasoningResult it summarizes (M2).
    assert!(nquads.contains("<https://blackcatinformatics.ca/logic/summarizesResult>"));
    assert!(nquads.contains("<https://blackcatinformatics.ca/logic/ReasoningResult>"));
    // The two completeness-gate axes ride the result node as status individuals, so
    // the full payload (including the axes that decide certificate-vs-attestation) is
    // faithfully recoverable from the carried quads.
    assert!(nquads.contains(
            "<https://blackcatinformatics.ca/logic/resultEvaluation> <https://blackcatinformatics.ca/logic/EvaluationCompleted>"
        ));
    assert!(nquads.contains(
            "<https://blackcatinformatics.ca/logic/resultCompleteness> <https://blackcatinformatics.ca/logic/CompleteForFragment>"
        ));
    for line in nquads.lines() {
        assert!(
            line.ends_with(&format!("<{graph}> .")),
            "line not in graph: {line}"
        );
    }
}

/// Two payloads differing ONLY in their axiom_hashes must produce different
/// content_ids — the blake3 digest covers the full payload so the former
/// FNV-1a collision (same class/bundle/contract/timestamp, different axioms)
/// cannot recur.
#[test]
fn content_id_discriminates_on_axiom_hashes() {
    let outcome_a = CoherenceOutcome::from_reasoning_result(
        &glut_result(),
        "blake3:bundle",
        ["blake3:axioms-set-A"],
        ContradictionPolicy::ForbidGap,
        "2026-06-28T00:00:00Z",
        BTreeSet::new(),
    )
    .unwrap();
    let outcome_b = CoherenceOutcome::from_reasoning_result(
        &glut_result(),
        "blake3:bundle",
        ["blake3:axioms-set-B"],
        ContradictionPolicy::ForbidGap,
        "2026-06-28T00:00:00Z",
        BTreeSet::new(),
    )
    .unwrap();
    let graph = "https://blackcatinformatics.ca/gmeow/graph/attestations";
    let nquads_a = outcome_a.to_nquads(graph);
    let nquads_b = outcome_b.to_nquads(graph);
    // The subject IRIs must differ (different axiom hashes → different content_id).
    assert_ne!(
        nquads_a, nquads_b,
        "payloads differing only in axiom_hashes must produce different content_ids"
    );
    // Both still carry their respective axiom hashes.
    assert!(nquads_a.contains("\"blake3:axioms-set-A\""));
    assert!(nquads_b.contains("\"blake3:axioms-set-B\""));
}

/// The `projection_losses` payload field comes from the CALLER-SUPPLIED loss
/// codes (genuine ledger codes from `pair_loss_ledger`), NOT from the DL
/// reasoner's `unsupported_constructs`. The two must never be conflated.
#[test]
fn projection_losses_sourced_from_ledger_codes_not_unsupported_constructs() {
    let mut result = consistent_result(
        EvaluationStatus::Completed,
        CompletenessStatus::CompleteForFragment,
    );
    // Inject a fake DL construct into preservation.unsupported_constructs.
    result
        .preservation
        .unsupported_constructs
        .insert("owl:someSpecialConstruct".to_owned());

    // The caller provides genuine ledger codes (what pair_loss_ledger returns).
    let ledger_codes: BTreeSet<String> = [
        "named-graph-dropped".to_owned(),
        "owl-dl-projection".to_owned(),
    ]
    .into_iter()
    .collect();

    let outcome = CoherenceOutcome::from_reasoning_result(
        &result,
        "blake3:bundle",
        Vec::<String>::new(),
        ContradictionPolicy::ForbidGapAndGlut,
        "2026-06-28T00:00:00Z",
        ledger_codes.clone(),
    )
    .unwrap();

    let payload = outcome.payload();

    // projection_losses must be exactly the caller-supplied ledger codes.
    assert_eq!(
        payload.projection_losses, ledger_codes,
        "projection_losses must equal the supplied ledger codes"
    );
    // projection_losses must NOT contain the DL unsupported construct.
    assert!(
        !payload
            .projection_losses
            .contains("owl:someSpecialConstruct"),
        "projection_losses must not be sourced from unsupported_constructs"
    );
    // unsupported_constructs must still carry the DL construct.
    assert!(
        payload
            .unsupported_constructs
            .contains("owl:someSpecialConstruct"),
        "unsupported_constructs must preserve the DL reasoner constructs"
    );
    // The two fields must not overlap in this scenario.
    assert!(
        payload
            .projection_losses
            .is_disjoint(&payload.unsupported_constructs),
        "projection_losses and unsupported_constructs must be disjoint here"
    );
}

/// The N-Quads projection emits `logic:unsupportedConstruct` for DL constructs
/// the reasoner could not decide, and `logic:projectionLoss` for ledger codes —
/// the two properties are distinct and carry the right values.
#[test]
fn nquads_emits_unsupported_construct_and_projection_loss_as_separate_properties() {
    let mut result = consistent_result(
        EvaluationStatus::Completed,
        CompletenessStatus::CompleteForFragment,
    );
    result
        .preservation
        .unsupported_constructs
        .insert("owl:NominalClass".to_owned());

    let ledger_codes: BTreeSet<String> = ["named-graph-dropped".to_owned()].into_iter().collect();

    let outcome = CoherenceOutcome::from_reasoning_result(
        &result,
        "blake3:bundle",
        Vec::<String>::new(),
        ContradictionPolicy::ForbidGapAndGlut,
        "2026-06-28T00:00:00Z",
        ledger_codes,
    )
    .unwrap();

    let graph = "https://blackcatinformatics.ca/gmeow/graph/attestations";
    let nquads = outcome.to_nquads(graph);

    // logic:projectionLoss carries the ledger code, not the DL construct.
    assert!(
        nquads.contains(
            "<https://blackcatinformatics.ca/logic/projectionLoss> \"named-graph-dropped\""
        ),
        "projectionLoss must contain the ledger code: {nquads}"
    );
    assert!(
        !nquads
            .contains("<https://blackcatinformatics.ca/logic/projectionLoss> \"owl:NominalClass\""),
        "projectionLoss must NOT contain the DL construct: {nquads}"
    );
    // logic:unsupportedConstruct carries the DL construct.
    assert!(
        nquads.contains(
            "<https://blackcatinformatics.ca/logic/unsupportedConstruct> \"owl:NominalClass\""
        ),
        "unsupportedConstruct must contain the DL construct: {nquads}"
    );
    assert!(
        !nquads.contains(
            "<https://blackcatinformatics.ca/logic/unsupportedConstruct> \"named-graph-dropped\""
        ),
        "unsupportedConstruct must NOT contain the ledger code: {nquads}"
    );
}
