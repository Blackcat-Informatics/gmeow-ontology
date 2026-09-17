// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Read-only preservation of the authored enactment gate contracts.

use super::{Contract, repair, report};
use gmeow_errors::Severity;

const VIOLATION_CODE: &str = "verify.enactment-integrity-violation";

fn violation_findings(report: &gmeow_errors::model::Report) -> Vec<&gmeow_errors::model::Finding> {
    report
        .findings
        .iter()
        .filter(|f| f.severity == Severity::Error && f.code == VIOLATION_CODE)
        .collect()
}

/// Assert the gate condemned `subject` — by name, not merely that SOMETHING was condemned.
fn assert_condemns(report: &gmeow_errors::model::Report, subject: &str) {
    let findings = violation_findings(report);
    assert!(
        !findings.is_empty(),
        "the enactment gate must raise a {VIOLATION_CODE} finding; got: {:?}",
        report
            .findings
            .iter()
            .map(|f| (f.code.as_str(), f.message.as_str()))
            .collect::<Vec<_>>()
    );
    let names_subject = findings.iter().any(|f| {
        f.detail.as_deref().is_some_and(|d| d.contains(subject)) || f.message.contains(subject)
    });
    assert!(
        names_subject,
        "the finding must name the offending record {subject}, not merely report that a law \
         fired; details were: {:?}",
        findings
            .iter()
            .map(|f| f.detail.as_deref())
            .collect::<Vec<_>>()
    );
}

/// The one row of the finding detail that names `subject`, if any.
///
/// The detail is `"; "`-joined, one row per condemned record, each row a sorted
/// `var=value` list. Splitting it back apart is what lets a test assert that a PARTICULAR
/// record was condemned under a PARTICULAR law — a substring test over the whole blob would
/// pass when one record broke law A and a different record broke law B, which is exactly the
/// confusion these paired presence/relational laws could otherwise hide.
fn rows_naming<'a>(report: &'a gmeow_errors::model::Report, subject: &str) -> Vec<&'a str> {
    violation_findings(report)
        .into_iter()
        .filter_map(|f| f.detail.as_deref())
        .flat_map(|detail| detail.split("; ").collect::<Vec<_>>())
        .filter(|row| row.contains(subject))
        .collect()
}

/// Assert the gate condemned `subject` AND named `law` as the law it broke.
///
/// Every relational test below goes through this rather than through [`assert_condemns`],
/// because "some law fired on this record" is precisely the weaker claim these laws were
/// rewritten to stop making: a presence law and its relational twin govern the same record
/// kind, so a test that only checks the record was condemned stays green when the
/// relational leg silently stops firing and its presence sibling picks up the slack.
fn assert_condemns_under(report: &gmeow_errors::model::Report, subject: &str, law: &str) {
    let rows = rows_naming(report, subject);
    assert!(
        !rows.is_empty(),
        "the enactment gate must condemn {subject}; findings were: {:?}",
        report
            .findings
            .iter()
            .map(|f| (f.code.as_str(), f.detail.as_deref()))
            .collect::<Vec<_>>()
    );
    assert!(
        rows.iter().any(|row| row.contains(law)),
        "the finding for {subject} must name the law {law} that condemned it — an operator \
         reading it otherwise learns only THAT enactment integrity broke; rows were: {rows:?}"
    );
}

/// Assert `law` fired on nothing at all in this scene.
///
/// The green half of each relational pair. Scoped to the law rather than to the whole
/// report so a scene may legitimately trip an unrelated completeness law (a stub proposal
/// binding one of its eight fields, say) without the green assertion becoming unwritable —
/// and so the assertion says exactly what it means: THIS relation holds.
fn assert_law_silent(report: &gmeow_errors::model::Report, law: &str, why: &str) {
    let fired: Vec<&str> = violation_findings(report)
        .into_iter()
        .filter_map(|f| f.detail.as_deref())
        .flat_map(|detail| detail.split("; ").collect::<Vec<_>>())
        .filter(|row| row.contains(law))
        .collect();
    assert!(fired.is_empty(), "{why}; but {law} fired on: {fired:?}");
}

fn assert_clean(report: &gmeow_errors::model::Report, why: &str) {
    assert!(
        violation_findings(report).is_empty(),
        "{why}; got: {:?}",
        violation_findings(report)
            .iter()
            .map(|f| (f.message.as_str(), f.detail.as_deref()))
            .collect::<Vec<_>>()
    );
}

fn a_receipt_with_no_attempt_fires_on_verify() {
    let report =
        report("slices/grounding/logic/tests/counter-examples/receipt-without-attempt.ttl");
    assert_condemns(report, "receiptNoAttempt");
}

fn an_unknown_outcome_with_no_attempt_fires_on_verify() {
    let report =
        report("slices/grounding/logic/tests/counter-examples/unknown-outcome-without-attempt.ttl");
    assert_condemns(report, "unknownNoAttempt");
}

fn a_frontier_claiming_closure_without_a_witness_fires_on_verify() {
    let report = report(
        "slices/grounding/logic/tests/counter-examples/frontier-closed-without-saturation-witness.ttl",
    );
    assert_condemns(report, "frontierNoWitness");
}

fn a_compensation_typed_as_its_own_forward_receipt_fires_on_verify() {
    let report = report(
        "slices/grounding/logic/tests/counter-examples/compensation-typed-as-its-forward-receipt.ttl",
    );
    assert_condemns(report, "compensationAsInverse");
}

fn the_shipped_content_free_witness_fixture_fires_on_verify() {
    let report = report(
        "slices/grounding/logic/tests/counter-examples/frontier-cites-a-content-free-witness.ttl",
    );
    assert_condemns_under(
        report,
        "frontierEmptyWitness",
        "FrontierClosureRequiresSaturationConstraint",
    );
}

fn a_pin_freezing_steps_its_method_never_yielded_fires_on_verify() {
    let report = report(
        "slices/grounding/logic/tests/counter-examples/pin-freezes-steps-its-method-never-yielded.ttl",
    );
    assert_condemns_under(
        report,
        "pinMismatchedWithMethod",
        "PinStepsMatchInstantiatedMethodConstraint",
    );
    assert_law_silent(
        report,
        "PinnedSubgraphCompletenessConstraint",
        "the fixture's pin binds all three mandatory fields, so only the relation is wrong",
    );
}

fn a_restore_against_a_drifted_fold_fires_on_verify() {
    let report = report(
        "slices/grounding/logic/tests/counter-examples/checkpoint-restored-under-a-drifted-fold.ttl",
    );
    assert_condemns_under(
        report,
        "resumeWeek13",
        "CheckpointRestoreIdentityConstraint",
    );
    assert_law_silent(
        report,
        "CheckpointCarriesFoldedIdentityConstraint",
        "the fixture's checkpoint carries its fold, so the presence sibling must stay \
         silent — otherwise this is the absence fixture wearing a mismatch's clothes",
    );
    assert_law_silent(
        report,
        "RestoreStaysWithinItsEnactmentConstraint",
        "the restore resumes the very run its checkpoint was taken from, so the second \
         identity axis holds and only the fold axis is wrong",
    );
}

fn an_unknown_outcome_retried_on_a_borrowed_licence_fires_on_verify() {
    let report = report(
        "slices/grounding/logic/tests/counter-examples/unknown-outcome-retried-on-a-borrowed-licence.ttl",
    );
    assert_condemns_under(report, "invoice901Retry", "NoBlindRetryConstraint");
    assert_law_silent(
        report,
        "RetryRequiresLicenceConstraint",
        "the retry names a licence, so the presence sibling passes and only the coverage \
         relation is wrong",
    );
    assert_law_silent(
        report,
        "UnknownOutcomeNamesItsAttemptConstraint",
        "the unknown outcome names its attempt; the defect is the retry, not the record of \
         the undetermined position",
    );
    assert_law_silent(
        report,
        "IdempotencyContractCompletenessConstraint",
        "the borrowed contract binds all five of its fields — a real licence for a real \
         attempt, which is exactly what makes borrowing it undetectable by a field check",
    );
}

fn a_version_revised_in_place_fires_on_verify() {
    let report = report(
        "slices/grounding/logic/tests/counter-examples/prescription-version-revised-under-a-running-enactment.ttl",
    );
    assert_condemns_under(
        report,
        "adrReviewPrescriptionV4",
        "PrescriptionVersionImmutabilityConstraint",
    );
    assert_law_silent(
        report,
        "PrescriptionVersionIsContentAddressedConstraint",
        "the version IS content-addressed — that is the precondition of the check, and the \
         absence sibling is the fixture that trips this one",
    );
    assert_law_silent(
        report,
        "EnactmentPinsPrescriptionAndSnapshotConstraint",
        "the enactment pins both its version and its input generation, so the run is \
         reproducible and only the version underneath it moved",
    );
}

fn a_frontier_closed_on_a_budget_cut_witness_fires_on_verify() {
    let report = report(
        "slices/grounding/logic/tests/counter-examples/frontier-closed-on-a-budget-cut-witness.ttl",
    );
    assert_condemns_under(
        report,
        "frontierCutForBudget",
        "FrontierClosureRequiresSaturationConstraint",
    );
    assert_law_silent(
        report,
        "FrontierCarriesSaturationWitnessConstraint",
        "the frontier CITES a witness, so the counting sibling passes — which is the whole \
         reason reading the witness had to become a separate law",
    );
}

fn an_ocr_gap_remedied_for_another_step_fires_on_verify() {
    let report = report(
        "slices/grounding/logic/tests/counter-examples/ocr-gap-remedied-for-a-different-step.ttl",
    );
    assert_condemns_under(
        report,
        "noOcrProvider",
        "OperationalGapCarriesProposalConstraint",
    );
    assert_law_silent(
        report,
        "OperationalGapNamesBlockedStepConstraint",
        "the gap names the step it blocks, so the presence sibling passes and the failure is \
         purely the join between the gap and the remedy",
    );
    assert_law_silent(
        report,
        "CapabilityGapProposalCompletenessConstraint",
        "the proposal in the scene binds all eight fields — it is a real remedy for a real \
         blockage, just not for THIS one",
    );
    assert_law_silent(
        report,
        "DispatchIntentCompletenessConstraint",
        "the intent that dispatched the weaker parser binds all nine fields, so the \
         substitution leaves a complete, well-formed record behind it — which is exactly why \
         a field check cannot see it",
    );
    // The SUBSTITUTION itself, condemned at the record that carries it. Every assertion
    // above condemns the GAP: between them they say the blockage was reported badly, and
    // none of them says anything about the dispatch that went ahead against a plain-text
    // extractor. Asserting this law BY NAME on the INTENT is what makes the difference
    // legible — the gap's own laws stay green on a scene where the gap arrives with its
    // remedy and the dispatch happens anyway.
    assert_condemns_under(
        report,
        "ocrStepIntent",
        "NoDispatchAgainstAnUnremediedGapConstraint",
    );
}

fn a_maintenance_goal_closed_by_one_good_week_fires_on_verify() {
    let report = report(
        "slices/grounding/logic/tests/counter-examples/maintenance-goal-closed-by-one-good-week.ttl",
    );
    assert_condemns_under(
        report,
        "maintenanceGoalClosedByWeek12",
        "MaintenanceGoalNeverConclusivelySatisfiedConstraint",
    );
}

fn a_maintenance_goal_held_so_far_but_undetermined_passes_on_verify() {
    let observed = repair(
        "slices/grounding/logic/tests/counter-examples/maintenance-goal-closed-by-one-good-week.ttl",
        "a_maintenance_goal_held_so_far_but_undetermined_passes_on_verify",
        1,
    );
    let report = &observed.report;
    assert_clean(
        report,
        "a maintenance goal that holds so far and says so — Satisfied, and UNDETERMINED — is \
         the record that keeps a continuing cluster open, and the kernel must raise nothing \
         on it",
    );
}

fn a_maintenance_goal_conclusively_violated_passes_on_verify() {
    let observed = repair(
        "slices/grounding/logic/tests/counter-examples/maintenance-goal-closed-by-one-good-week.ttl",
        "a_maintenance_goal_conclusively_violated_passes_on_verify",
        1,
    );
    let report = &observed.report;
    assert_law_silent(
        report,
        "MaintenanceGoalNeverConclusivelySatisfiedConstraint",
        "a maintenance target that FAILED has reached a conclusive judgment, and the law \
         forbids the satisfied-and-concluded pair alone",
    );
}

fn an_advisory_laundered_into_an_authorization_proof_fires_on_verify() {
    let report = report(
        "slices/grounding/logic/tests/counter-examples/advisory-laundered-into-an-authorization-proof.ttl",
    );
    assert_condemns_under(
        report,
        "modelSaysTheChargeIsInPolicy",
        "AdvisoryNeverAuthorityConstraint",
    );
}

fn a_compensation_naming_no_forward_effect_fires_on_verify() {
    let report = report(
        "slices/grounding/logic/tests/counter-examples/compensation-names-no-forward-effect.ttl",
    );
    assert_condemns_under(
        report,
        "invoice901Refund",
        "CompensationNamesForwardEffectConstraint",
    );
    assert_law_silent(
        report,
        "CompensationBindsExactForwardEffectConstraint",
        "the exactness law DEREFERENCES the binding, so its guard requires one; a \
         compensation naming nothing falls outside it and only the presence law can speak",
    );
    assert_law_silent(
        report,
        "CompensationNotInverseConstraint",
        "the refund is not typed as the receipt it addresses, so the prohibition holds and \
         the red above cannot be the double-typing law wearing another law's name",
    );
    assert_law_silent(
        report,
        "ReceiptRequiresAttemptConstraint",
        "the forward receipt in the scene names the attempt it reports on, so the effect \
         record beside the refund is well-formed",
    );
}

fn a_compensation_naming_its_forward_receipt_passes_on_verify() {
    let observed = repair(
        "slices/grounding/logic/tests/counter-examples/compensation-names-no-forward-effect.ttl",
        "a_compensation_naming_its_forward_receipt_passes_on_verify",
        0,
    );
    let report = &observed.report;
    assert_clean(
        report,
        "the SAME scene with the refund bound to the receipt of the charge it undoes is \
         exactly what the compensation layer models, and the kernel must raise nothing",
    );
}

fn an_assembly_naming_no_enactment_fires_on_verify() {
    let report = report(
        "slices/grounding/logic/tests/counter-examples/context-assembly-serving-no-enactment.ttl",
    );
    assert_condemns_under(
        report,
        "assemblyServingNobody",
        "ContextAssemblyNamesItsEnactmentConstraint",
    );
    assert_law_silent(
        report,
        "ContextAssemblyRecordsExclusionsConstraint",
        "the withheld item carries its reason, so the exclusion law is GUARDED here and \
         passes on its merits rather than vacuously — the assembly's defect is its subject, \
         not its bookkeeping",
    );
    assert_law_silent(
        report,
        "ContextAssemblyExclusionIsNotInclusionConstraint",
        "the surfaced item and the withheld item are different items, so the disjointness \
         law is guarded and holds",
    );
}

fn an_assembly_naming_the_run_it_served_passes_on_verify() {
    let observed = repair(
        "slices/grounding/logic/tests/counter-examples/context-assembly-serving-no-enactment.ttl",
        "an_assembly_naming_the_run_it_served_passes_on_verify",
        0,
    );
    let report = &observed.report;
    assert_clean(
        report,
        "an assembly bound to the run it surfaced material to is the record the kernel \
         models, and the kernel must raise nothing on it",
    );
}

fn a_journal_entry_establishing_no_head_fires_on_verify() {
    let report = report(
        "slices/grounding/logic/tests/counter-examples/journal-entry-establishing-no-head.ttl",
    );
    assert_condemns_under(
        report,
        "journalEntry9",
        "JournalEntryNamesBothHeadsConstraint",
    );
    assert_law_silent(
        report,
        "JournalChainIntegrityConstraint",
        "the entry's prior head IS its predecessor's new head, so the hash-chain invariant \
         holds where it can be evaluated and the missing end is the only defect",
    );
}

fn a_journal_entry_naming_both_of_its_heads_passes_on_verify() {
    let observed = repair(
        "slices/grounding/logic/tests/counter-examples/journal-entry-establishing-no-head.ttl",
        "a_journal_entry_naming_both_of_its_heads_passes_on_verify",
        0,
    );
    let report = &observed.report;
    assert_clean(
        report,
        "an entry naming the head it was applied against AND the head it established is \
         chainable in both directions, which is the whole of what the presence law asks",
    );
}

fn a_reconciliation_result_carrying_no_verdict_fires_on_verify() {
    let report = report(
        "slices/grounding/logic/tests/counter-examples/retry-licensed-by-a-verdictless-probe.ttl",
    );
    assert_condemns_under(
        report,
        "invoice903ProbeResult",
        "ReconciliationResultCarriesVerdictConstraint",
    );
    assert_law_silent(
        report,
        "RetryRequiresLicenceConstraint",
        "the retry NAMES a licence, so the presence half of the retry discipline passes — \
         which is exactly why a licence that records nothing had to become its own law",
    );
    assert_law_silent(
        report,
        "NoBlindRetryConstraint",
        "the licence carries logic:licenceCoversAttempt for the very attempt being \
         re-sent, so the coverage relation holds and the retry is not a BORROWED licence \
         but an EMPTY one",
    );
    assert_law_silent(
        report,
        "UnknownOutcomeNamesItsAttemptConstraint",
        "the undetermined position names the attempt it is undetermined about, so the \
         precondition of the whole probe-and-retry discipline is met",
    );
}

fn a_reconciliation_result_carrying_its_verdict_passes_on_verify() {
    let observed = repair(
        "slices/grounding/logic/tests/counter-examples/retry-licensed-by-a-verdictless-probe.ttl",
        "a_reconciliation_result_carrying_its_verdict_passes_on_verify",
        0,
    );
    let report = &observed.report;
    assert_clean(
        report,
        "a probe that established the charge never committed, recorded as the verdict it \
         reached, is the licence the retry was entitled to proceed under — and the kernel \
         must raise nothing on it",
    );
}

fn the_absence_fixtures_reach_their_presence_law_and_not_their_relational_twin() {
    for (fixture, subject, presence, relational) in [
        (
            "slices/grounding/logic/tests/counter-examples/checkpoint-no-folded-identity.ttl",
            "ckptNoIdentity",
            "CheckpointCarriesFoldedIdentityConstraint",
            "CheckpointRestoreIdentityConstraint",
        ),
        (
            "slices/grounding/logic/tests/counter-examples/unknown-outcome-without-attempt.ttl",
            "unknownNoAttempt",
            "UnknownOutcomeNamesItsAttemptConstraint",
            "NoBlindRetryConstraint",
        ),
        (
            "slices/grounding/logic/tests/counter-examples/prescription-version-not-content-addressed.ttl",
            "versionNoDigest",
            "PrescriptionVersionIsContentAddressedConstraint",
            "PrescriptionVersionImmutabilityConstraint",
        ),
        (
            "slices/grounding/logic/tests/counter-examples/frontier-closed-without-saturation-witness.ttl",
            "frontierNoWitness",
            "FrontierCarriesSaturationWitnessConstraint",
            "FrontierClosureRequiresSaturationConstraint",
        ),
        (
            "slices/grounding/logic/tests/counter-examples/capability-gap-without-blocked-step.ttl",
            "ocrGapNoStep",
            "OperationalGapNamesBlockedStepConstraint",
            "OperationalGapCarriesProposalConstraint",
        ),
    ] {
        let report = report(fixture);
        assert_condemns_under(report, subject, presence);
        assert_law_silent(
            report,
            relational,
            &format!(
                "{relational} cannot fire on {subject}: its guard requires the binding the \
                 fixture omits, so a conformance cell pinning this fixture to it would pin a \
                 law that has nothing to say about the record"
            ),
        );
    }
}

pub(super) fn contracts() -> Vec<Contract> {
    vec![
        (
            "a_receipt_with_no_attempt_fires_on_verify",
            a_receipt_with_no_attempt_fires_on_verify,
        ),
        (
            "an_unknown_outcome_with_no_attempt_fires_on_verify",
            an_unknown_outcome_with_no_attempt_fires_on_verify,
        ),
        (
            "a_frontier_claiming_closure_without_a_witness_fires_on_verify",
            a_frontier_claiming_closure_without_a_witness_fires_on_verify,
        ),
        (
            "a_compensation_typed_as_its_own_forward_receipt_fires_on_verify",
            a_compensation_typed_as_its_own_forward_receipt_fires_on_verify,
        ),
        (
            "the_shipped_content_free_witness_fixture_fires_on_verify",
            the_shipped_content_free_witness_fixture_fires_on_verify,
        ),
        (
            "a_pin_freezing_steps_its_method_never_yielded_fires_on_verify",
            a_pin_freezing_steps_its_method_never_yielded_fires_on_verify,
        ),
        (
            "a_restore_against_a_drifted_fold_fires_on_verify",
            a_restore_against_a_drifted_fold_fires_on_verify,
        ),
        (
            "an_unknown_outcome_retried_on_a_borrowed_licence_fires_on_verify",
            an_unknown_outcome_retried_on_a_borrowed_licence_fires_on_verify,
        ),
        (
            "a_version_revised_in_place_fires_on_verify",
            a_version_revised_in_place_fires_on_verify,
        ),
        (
            "a_frontier_closed_on_a_budget_cut_witness_fires_on_verify",
            a_frontier_closed_on_a_budget_cut_witness_fires_on_verify,
        ),
        (
            "an_ocr_gap_remedied_for_another_step_fires_on_verify",
            an_ocr_gap_remedied_for_another_step_fires_on_verify,
        ),
        (
            "a_maintenance_goal_closed_by_one_good_week_fires_on_verify",
            a_maintenance_goal_closed_by_one_good_week_fires_on_verify,
        ),
        (
            "a_maintenance_goal_held_so_far_but_undetermined_passes_on_verify",
            a_maintenance_goal_held_so_far_but_undetermined_passes_on_verify,
        ),
        (
            "a_maintenance_goal_conclusively_violated_passes_on_verify",
            a_maintenance_goal_conclusively_violated_passes_on_verify,
        ),
        (
            "an_advisory_laundered_into_an_authorization_proof_fires_on_verify",
            an_advisory_laundered_into_an_authorization_proof_fires_on_verify,
        ),
        (
            "a_compensation_naming_no_forward_effect_fires_on_verify",
            a_compensation_naming_no_forward_effect_fires_on_verify,
        ),
        (
            "a_compensation_naming_its_forward_receipt_passes_on_verify",
            a_compensation_naming_its_forward_receipt_passes_on_verify,
        ),
        (
            "an_assembly_naming_no_enactment_fires_on_verify",
            an_assembly_naming_no_enactment_fires_on_verify,
        ),
        (
            "an_assembly_naming_the_run_it_served_passes_on_verify",
            an_assembly_naming_the_run_it_served_passes_on_verify,
        ),
        (
            "a_journal_entry_establishing_no_head_fires_on_verify",
            a_journal_entry_establishing_no_head_fires_on_verify,
        ),
        (
            "a_journal_entry_naming_both_of_its_heads_passes_on_verify",
            a_journal_entry_naming_both_of_its_heads_passes_on_verify,
        ),
        (
            "a_reconciliation_result_carrying_no_verdict_fires_on_verify",
            a_reconciliation_result_carrying_no_verdict_fires_on_verify,
        ),
        (
            "a_reconciliation_result_carrying_its_verdict_passes_on_verify",
            a_reconciliation_result_carrying_its_verdict_passes_on_verify,
        ),
        (
            "the_absence_fixtures_reach_their_presence_law_and_not_their_relational_twin",
            the_absence_fixtures_reach_their_presence_law_and_not_their_relational_twin,
        ),
    ]
}
