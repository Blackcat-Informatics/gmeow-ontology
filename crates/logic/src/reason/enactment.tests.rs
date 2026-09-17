// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::{
    BANNED_DERIVED_HEADS, DERIVATION_IDENTIFIER, EvalTerm, RcViolationGap, VIOLATED_LAW,
    compile_cached, compiled_law_report, is_banned_derived_head, reject_banned_heads,
};
use std::collections::BTreeSet;

const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";

/// The 44 enactment-kernel laws the gate MUST compile, by local name.
///
/// Spelled out rather than counted, because the number alone would stay green if one
/// law silently dropped out of the fragment and an unrelated one was added. This is the
/// census the module doc's claim rests on: every enactment law authored in
/// `slices/grounding/logic/module.ttl` is a law the chase actually runs.
///
/// The set is deliberately PAIRED across most record kinds: a presence law whose name
/// says only that a binding exists, and a relational law whose name states the relation
/// and whose body joins the records that relation holds between. The pairing exists
/// because the two failures are independent — a lease with no fencing identity and a
/// lease double-claiming a held scope are different defects, and a corpus must be able
/// to trip each alone — and because a single law cannot do both: the relational body
/// needs the binding in its GUARD, which makes the missing-binding case fall outside it.
const ENACTMENT_LAWS: [&str; 44] = [
    "AdvisoryNeverAuthorityConstraint",
    "ApprovalCommitmentCompletenessConstraint",
    "ApprovalDigestBindsDispatchIntentConstraint",
    "ApprovalScopedToIntentEnactmentConstraint",
    "CapabilityGapProposalCompletenessConstraint",
    "CheckpointCarriesFoldedIdentityConstraint",
    "CheckpointRestoreIdentityConstraint",
    "ClockAttributionRequiredConstraint",
    "CompensationBindsExactForwardEffectConstraint",
    "CompensationNamesForwardEffectConstraint",
    "CompensationNotInverseConstraint",
    "CompensationOutcomeReceiptIsNotTheForwardReceiptConstraint",
    "CompensationSuccessRequiresReceiptConstraint",
    "ContextAssemblyExclusionIsNotInclusionConstraint",
    "ContextAssemblyNamesItsEnactmentConstraint",
    "ContextAssemblyRecordsExclusionsConstraint",
    "ContinuationKindDisjointnessConstraint",
    "ContinuationRepeatExcludesReviseConstraint",
    "DispatchIntentCompletenessConstraint",
    "EffectRecordsAreObservedNotDerivedConstraint",
    "EnactmentPinsPrescriptionAndSnapshotConstraint",
    "FrontierCarriesSaturationWitnessConstraint",
    "FrontierClosureRequiresSaturationConstraint",
    "IdempotencyContractCompletenessConstraint",
    "JournalChainIntegrityConstraint",
    "JournalEntryNamesBothHeadsConstraint",
    "LeaseCarriesFencingIdentityConstraint",
    "LeaseExclusivityConstraint",
    "MaintenanceGoalNeverConclusivelySatisfiedConstraint",
    "NoBlindRetryConstraint",
    "NoDispatchAgainstAnUnremediedGapConstraint",
    "OperationalGapCarriesProposalConstraint",
    "OperationalGapNamesBlockedStepConstraint",
    "PinStepsMatchInstantiatedMethodConstraint",
    "PinnedSubgraphCompletenessConstraint",
    "PrescriptionVersionImmutabilityConstraint",
    "PrescriptionVersionIsContentAddressedConstraint",
    "ReceiptRequiresAttemptConstraint",
    "ReconciliationResultCarriesVerdictConstraint",
    "RefinementEpisodeDeclaresSearchFragmentConstraint",
    "RefinementPinComesFromCandidateSetConstraint",
    "RestoreStaysWithinItsEnactmentConstraint",
    "RetryRequiresLicenceConstraint",
    "UnknownOutcomeNamesItsAttemptConstraint",
];

/// Every relational law — one whose body JOINS two or more records — compiles.
///
/// A subset of [`ENACTMENT_LAWS`], named separately because it is the half that would
/// be silently lost by a regression to presence checking: a constraint whose guard
/// carries the extra join atoms is exactly the shape that falls out of the Horn+NAF
/// fragment first, and losing one would restore the defect this census was rebuilt to
/// end — a law whose IRI asserts a relation and whose body tests a field.
const RELATIONAL_LAWS: [&str; 18] = [
    "ApprovalDigestBindsDispatchIntentConstraint",
    "ApprovalScopedToIntentEnactmentConstraint",
    "CheckpointRestoreIdentityConstraint",
    "CompensationBindsExactForwardEffectConstraint",
    "CompensationOutcomeReceiptIsNotTheForwardReceiptConstraint",
    "ContextAssemblyExclusionIsNotInclusionConstraint",
    "ContextAssemblyRecordsExclusionsConstraint",
    "FrontierClosureRequiresSaturationConstraint",
    "JournalChainIntegrityConstraint",
    "LeaseExclusivityConstraint",
    "MaintenanceGoalNeverConclusivelySatisfiedConstraint",
    "NoBlindRetryConstraint",
    "NoDispatchAgainstAnUnremediedGapConstraint",
    "OperationalGapCarriesProposalConstraint",
    "PinStepsMatchInstantiatedMethodConstraint",
    "PrescriptionVersionImmutabilityConstraint",
    "RefinementPinComesFromCandidateSetConstraint",
    "RestoreStaysWithinItsEnactmentConstraint",
];

/// The IRIs of the constraints that actually lowered into violation rules.
fn compiled_constraints() -> BTreeSet<String> {
    compiled_law_report()
        .rules
        .iter()
        .filter_map(|rule| rule.constraint_tag.clone())
        .collect()
}

/// Every enactment-kernel law compiles into at least one violation rule.
///
/// The census that makes the module doc auditable. A law that stopped compiling — a
/// consequent rewritten into a shape outside the Horn+NAF fragment, an accidentally
/// dropped `gmeow:enforcesFailureClass` — would leave the gate quietly enforcing less
/// than it says, which is the exact failure mode this whole module was rebuilt to end.
#[test]
fn every_enactment_kernel_law_compiles_into_a_violation_rule() {
    let compiled = compiled_constraints();
    let missing: Vec<&str> = ENACTMENT_LAWS
        .iter()
        .copied()
        .filter(|name| {
            !compiled
                .iter()
                .any(|iri| iri.ends_with(&format!("/{name}")))
        })
        .collect();
    assert!(
        missing.is_empty(),
        "these authored enactment-kernel laws did not lower into violation rules, so the \
             gate does not enforce them: {missing:?}"
    );
}

/// The variables an atom mentions, as an ordered set.
fn atom_vars(atom: &crate::rule_ir::EvalAtom) -> BTreeSet<&str> {
    [&atom.subject, &atom.object]
        .into_iter()
        .filter_map(|term| match term {
            EvalTerm::Var(v) => Some(v.as_str()),
            _ => None,
        })
        .collect()
}

/// The variables a body binds POSITIVELY, closed under join with the focus variable.
///
/// Seeded with the focus and grown by repeatedly absorbing every positive atom that
/// already shares a variable with the set. A body variable outside the result is one
/// the law reaches only through an atom that shares nothing with the focus record —
/// a cartesian product wearing a join's clothes.
fn focus_connected_vars<'a>(
    body: &'a [crate::rule_ir::EvalAtom],
    focus: &'a str,
) -> BTreeSet<&'a str> {
    let mut reached: BTreeSet<&str> = BTreeSet::new();
    reached.insert(focus);
    loop {
        let before = reached.len();
        for atom in body.iter().filter(|a| !a.negated) {
            let vars = atom_vars(atom);
            if vars.iter().any(|v| reached.contains(v)) {
                reached.extend(vars);
            }
        }
        if reached.len() == before {
            return reached;
        }
    }
}

/// Every relational law lowers into a rule whose body actually JOINS, and whose
/// CONCLUSION names the law that drew it.
///
/// The census above would stay green if a relational law's body were quietly reduced to
/// its presence sibling's — same IRI, same failure class, same rule count, and a gate
/// that reads as enforcing a relation while testing a field. Three properties are
/// pinned, and the third is the one this test used to be missing:
///
/// 1. **Its GUARD reaches past the focus record** — some POSITIVE body atom mentions a
///    variable other than the focus. This is the structural difference the census doc
///    above names: a presence law's guard is a bare `rdf:type` test and its only other
///    variable is the free object of the NAF probe, so a relational law reduced to its
///    presence sibling loses exactly this. The test is on the guard, and on EITHER term
///    position, because a relation between two of the focus record's own bindings —
///    `assemblyExcluded(?this, ?w) ∧ assemblyIncluded(?this, ?w)` — joins on the OBJECT
///    and is no less a relation for it.
/// 2. **The join is CONNECTED** — every variable a POSITIVE atom mentions is reachable
///    from the focus through shared variables. An unconnected positive atom is a
///    cartesian product: it makes the body look relational and constrains nothing about
///    the focus record, so a law reduced to one would satisfy property 1 alone. A
///    variable free in the NAF literal alone is excluded by design — that is the
///    existential probe the obligation shape is decided by, not a stray join.
/// 3. **The conclusion is LAW-DISTINGUISHING** — the head tuple names this constraint,
///    so a record condemned by this law and by another produces two distinct derived
///    tuples. When every law headed on the shared `rdf:type EnactmentIntegrityViolation`
///    marker instead, the chase's one-winner-per-tuple selection erased every law but
///    one on any record that broke more than one, and this test — checking only body
///    shape — stayed green throughout. A law whose finding another law can delete is
///    exactly a law that reads as relational and enforces nothing.
#[test]
fn every_relational_law_lowers_into_a_joining_body() {
    let report = compiled_law_report();
    let mut not_joining: Vec<&str> = Vec::new();
    let mut disconnected: Vec<String> = Vec::new();
    let mut anonymous_conclusion: Vec<String> = Vec::new();
    for name in RELATIONAL_LAWS {
        let suffix = format!("/{name}");
        let rules: Vec<&crate::rule_ir::EvalRule> = report
            .rules
            .iter()
            .filter(|rule| {
                rule.constraint_tag
                    .as_ref()
                    .is_some_and(|iri| iri.ends_with(&suffix))
            })
            .collect();
        if !rules.iter().any(|rule| {
            let EvalTerm::Var(focus) = &rule.head.subject else {
                return false;
            };
            rule.body
                .iter()
                .filter(|atom| !atom.negated)
                .any(|atom| atom_vars(atom).iter().any(|v| *v != focus.as_str()))
        }) {
            not_joining.push(name);
        }
        for rule in &rules {
            let EvalTerm::Var(focus) = &rule.head.subject else {
                anonymous_conclusion
                    .push(format!("{name}: head subject is not the focus variable"));
                continue;
            };
            let reached = focus_connected_vars(&rule.body, focus);
            // POSITIVE atoms only. A variable occurring solely in the NAF literal is
            // the existential probe this lowering is built on — `¬intentDigest(?intent,
            // ?digest)` with `?intent` free asks "does ANY intent carry this digest",
            // which is the obligation's meaning and not a stray join.
            let stranded: Vec<&str> = rule
                .body
                .iter()
                .filter(|atom| !atom.negated)
                .flat_map(|atom| atom_vars(atom))
                .filter(|v| !reached.contains(v))
                .collect();
            if !stranded.is_empty() {
                disconnected.push(format!("{name}: {stranded:?}"));
            }
            let names_this_law = rule.head.predicate == VIOLATED_LAW
                && matches!(&rule.head.object, EvalTerm::ConstNamed(iri) if iri.ends_with(&suffix));
            if !names_this_law {
                anonymous_conclusion.push(format!(
                    "{name}: head is <{}> {:?}",
                    rule.head.predicate, rule.head.object
                ));
            }
        }
    }
    assert!(
        not_joining.is_empty(),
        "these laws name a RELATION and lowered into a body that never leaves the focus \
             record, so their name asserts more than their body checks: {not_joining:?}"
    );
    assert!(
        disconnected.is_empty(),
        "these laws lowered into a body whose variables are not all reachable from the \
             focus record, so part of the body is a cartesian product constraining nothing \
             about the record the law condemns: {disconnected:?}"
    );
    assert!(
        anonymous_conclusion.is_empty(),
        "these laws lowered into a conclusion that does not name them, so a record they \
             condemn derives a tuple another law can derive too — and the chase keeps one \
             derivation per tuple, silently erasing every loser: {anonymous_conclusion:?}"
    );
}

/// No two laws share a head tuple shape, across the WHOLE compiled corpus.
///
/// The corpus-wide form of the property the relational census pins per law, and the
/// invariant `crate::relational_core::reject_colliding_heads` enforces at lowering
/// time. Stated here as well because the failure it prevents is silent by construction:
/// two laws sharing a conclusion still compile, still run, still appear in every census,
/// and still lose one finding per co-condemned record.
#[test]
fn no_two_laws_share_a_conclusion() {
    let mut owners: std::collections::BTreeMap<(String, String), BTreeSet<String>> =
        std::collections::BTreeMap::new();
    for rule in &compiled_law_report().rules {
        let object = match &rule.head.object {
            EvalTerm::ConstNamed(iri) => iri.clone(),
            other => format!("{other:?}"),
        };
        owners
            .entry((rule.head.predicate.clone(), object))
            .or_default()
            .insert(rule.constraint_tag.clone().unwrap_or_default());
    }
    let shared: Vec<String> = owners
        .iter()
        .filter(|(_, laws)| laws.len() > 1)
        .map(|(head, laws)| format!("{head:?} shared by {laws:?}"))
        .collect();
    assert!(
        shared.is_empty(),
        "these head tuple shapes are derived by more than one authored law, so a record \
             both condemn yields one derived tuple and one surviving law: {shared:?}"
    );
}

/// Every compiled rule is traceable to the authored law it came from.
///
/// The precondition of the finding carrying its law identity. An untagged rule would
/// derive a marker the gate cannot attribute, which `enactment_gate_markers_with_laws` refuses at
/// runtime — this catches it at the lowering instead, where the cause is visible.
#[test]
fn every_compiled_rule_carries_its_authored_law() {
    let untagged: Vec<&str> = compiled_law_report()
        .rules
        .iter()
        .filter(|rule| rule.constraint_tag.is_none())
        .map(|rule| rule.rule_iri.as_str())
        .collect();
    assert!(
        untagged.is_empty(),
        "these violation rules carry no source constraint, so a marker they derive names \
             no law an operator could read: {untagged:?}"
    );
}

/// Every declined constraint is declined for want of a FAILURE CLASS, never for want
/// of expressiveness.
///
/// The residue today is entirely `no-enforces-failure-class`: the `gmeow:` advice /
/// term-completeness constraints and the `logic:` IR-shape constraints name no failure
/// class, because their enforcement surface is the derived SHACL that `make validate`
/// runs, not this gate. That is the one legitimate reason to decline (alongside a
/// builtin-bound consequent, which [`super::super::math_gate`] owns and which this
/// module's `logic/module.ttl` authors none of).
///
/// [`super::prepare_rules`] already refuses to produce a report containing any other gap,
/// so this test names the invariant explicitly rather than leaving a reader of the
/// census to infer it from a panic message.
#[test]
fn declined_constraints_are_declined_only_for_want_of_a_failure_class() {
    let other_gaps: Vec<String> = compiled_law_report()
        .residue
        .iter()
        .filter(|r| r.gap != RcViolationGap::NoFailureClass)
        .map(|r| format!("{} :: {}", r.constraint_iri, r.gap.as_str()))
        .collect();
    assert!(
        other_gaps.is_empty(),
        "a constraint was declined for a reason other than naming no failure class: \
             {other_gaps:?}"
    );
}

/// The compiled law set is STRATIFIABLE, so the chase can actually run it.
///
/// Every violation rule's head is `rdf:type` and several bodies carry NAF literals; if
/// a law's negated literal were ever itself an `rdf:type` test, the predicate graph
/// would carry a negative self-edge and no finite stratification would exist — the gate
/// would then fail closed on EVERY dataset. Pinning it here catches that at the law's
/// authoring rather than on the first `verify()` run that trips it.
#[test]
fn the_compiled_law_set_is_stratifiable() {
    let rules = compiled_law_report().rules.clone();
    assert!(!rules.is_empty(), "the gate must compile at least one law");
    let lookup = compile_cached(
        "https://blackcatinformatics.ca/gmeow/reason/enactment-gate/stratification-probe",
        rules,
    );
    assert!(
        lookup.executable.is_some(),
        "the compiled enactment laws must be stratifiable — otherwise the gate refuses \
             every dataset and its findings go permanently dark"
    );
}

#[test]
fn both_effect_record_kinds_are_banned_heads() {
    assert_eq!(BANNED_DERIVED_HEADS.len(), 2);
    assert!(is_banned_derived_head(
        "https://blackcatinformatics.ca/logic/EffectAttempt"
    ));
    assert!(is_banned_derived_head(
        "https://blackcatinformatics.ca/logic/ExternalEffectReceipt"
    ));
}

#[test]
fn a_kernel_class_that_is_not_an_effect_record_is_derivable() {
    // The guard must be narrow: the frontier and its labels are DERIVED by design,
    // and a guard that refused them would forbid the kernel's own headline capability.
    assert!(!is_banned_derived_head(
        "https://blackcatinformatics.ca/logic/ActionableFrontier"
    ));
    assert!(!is_banned_derived_head(
        "https://blackcatinformatics.ca/logic/FrontierEntry"
    ));
}

#[test]
fn deriving_an_effect_attempt_is_refused() {
    let rows = vec![(
        "https://example.org/attempt-1".to_owned(),
        RDF_TYPE.to_owned(),
        "https://blackcatinformatics.ca/logic/EffectAttempt".to_owned(),
    )];
    let err = reject_banned_heads(&rows).expect_err("deriving an attempt must be refused");
    assert!(
        format!("{err:?}").contains("OBSERVED, never derived"),
        "the refusal must say WHY, not merely that it refused"
    );
}

#[test]
fn deriving_an_external_effect_receipt_is_refused() {
    let rows = vec![(
        "https://example.org/receipt-1".to_owned(),
        RDF_TYPE.to_owned(),
        "https://blackcatinformatics.ca/logic/ExternalEffectReceipt".to_owned(),
    )];
    assert!(
        reject_banned_heads(&rows).is_err(),
        "deriving a receipt asserts an outcome nobody observed"
    );
}

#[test]
fn stamping_derivation_provenance_on_a_kernel_effect_record_is_refused() {
    // The stamp row comes FIRST, so the guard reaches it before the typing row that
    // would independently condemn the same subject — proving the derivation-provenance
    // arm fires on its own terms and names the effect record in its message.
    let record = "https://example.org/attempt-7".to_owned();
    let rows = vec![
        (
            record.clone(),
            DERIVATION_IDENTIFIER.to_owned(),
            "derivation-42".to_owned(),
        ),
        (
            record.clone(),
            RDF_TYPE.to_owned(),
            "https://blackcatinformatics.ca/logic/EffectAttempt".to_owned(),
        ),
    ];
    let err = reject_banned_heads(&rows)
        .expect_err("derivation provenance is the machine-checkable mark of an inferred effect");
    assert!(
        format!("{err:?}").contains("stamp derivation provenance"),
        "the derivation-provenance arm must be the one that fired, not the typing arm"
    );
}

/// Effect-record identity is decided by TYPING, not by IRI namespace.
///
/// The retired predicate treated every `logic:`-namespaced subject as an effect
/// record, which condemned derivation provenance on the kernel's own by-design
/// derivations. A `logic:`-namespaced subject that nothing types as an effect attempt
/// or receipt must carry derivation provenance freely — that IS what a derived
/// frontier entry looks like.
#[test]
fn derivation_provenance_on_a_logic_subject_that_is_not_an_effect_record_passes() {
    let entry = "https://blackcatinformatics.ca/logic/frontierEntry-3".to_owned();
    let rows = vec![
        (
            entry.clone(),
            RDF_TYPE.to_owned(),
            "https://blackcatinformatics.ca/logic/FrontierEntry".to_owned(),
        ),
        (
            entry,
            DERIVATION_IDENTIFIER.to_owned(),
            "derivation-42".to_owned(),
        ),
    ];
    assert!(
        reject_banned_heads(&rows).is_ok(),
        "a derived frontier entry is the kernel's headline capability, not a violation"
    );
}

#[test]
fn an_ordinary_derivation_passes_the_guard() {
    let rows = vec![(
        "https://example.org/frontier-1".to_owned(),
        RDF_TYPE.to_owned(),
        "https://blackcatinformatics.ca/logic/ActionableFrontier".to_owned(),
    )];
    assert!(
        reject_banned_heads(&rows).is_ok(),
        "the guard must not obstruct the derivations the kernel exists to produce"
    );
}
