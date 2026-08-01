// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Production-surface acceptance harness for the reasoner-derived enactment-kernel gate.
//!
//! These tests drive the **production `verify()` entrypoint** — the same one
//! `make reason-verify` invokes — with real input datasets, never a hand-assembled EDB and
//! never `enactment_gate_markers` directly. That distinction is the entire reason this file
//! exists: the gate it covers previously compiled no laws and returned an unconditionally
//! empty marker set, and every in-module unit test stayed green throughout, because a unit
//! test proves a function works in isolation while only a test through the real entry point
//! proves the function is WIRED and can fail.
//!
//! The shape of the coverage:
//!
//! * **Red**, from the SHIPPED counter-example fixtures — the same
//!   `tests/counter-examples/*.ttl` the slice's own conformance corpus names as the fail
//!   witnesses for these laws. Driving the shipped artifact rather than an inline string is
//!   what keeps the fixture and the gate from drifting apart: if a fixture stops violating
//!   its law, this goes red rather than the fixture quietly becoming decorative.
//! * **Green**, from the same record made well-formed. Without the green half the red half
//!   would be satisfied by a gate that condemns everything, which is not a gate.
//! * **Reasoned-closure reach**, from a record whose kernel type is DERIVED by subclass
//!   inference and never asserted — the gate must see it, or its domain is the raw EDB and
//!   any modelling that types records through a domain subclass escapes the kernel entirely.
//! * **Boundary**, from a record the gate condemns that is ALSO an effect attempt: the
//!   marker is a legitimate derivation over an observed record, and the observed-not-derived
//!   guard must not mistake one for the other.

use gmeow_errors::Severity;
use gmeow_logic::verify::{embedded_verify_queries, verify};
use std::sync::Arc;

use purrdf::RdfDataset;

/// The finding code the verify query loop renders a returned row of
/// `enactment-integrity-violation.rq` as.
const VIOLATION_CODE: &str = "verify.enactment-integrity-violation";

// ── Shipped counter-example fixtures (the slice's own named fail witnesses) ───────────

/// N4 — a `logic:ExternalEffectReceipt` naming no attempt.
/// Trips `logic:ReceiptRequiresAttemptConstraint`.
const RECEIPT_WITHOUT_ATTEMPT: &str = include_str!(
    "../../../slices/grounding/logic/tests/counter-examples/receipt-without-attempt.ttl"
);

/// N5 — a `logic:ExternalOutcomeUnknown` naming no attempt.
/// Trips `logic:UnknownOutcomeNamesItsAttemptConstraint`.
const UNKNOWN_OUTCOME_WITHOUT_ATTEMPT: &str = include_str!(
    "../../../slices/grounding/logic/tests/counter-examples/unknown-outcome-without-attempt.ttl"
);

/// N6a — a `logic:CompensationAttempt` typed as the receipt it counteracts.
/// Trips `logic:CompensationNotInverseConstraint` — the PROHIBITION shape, whose violation
/// rule is a purely positive body, unlike the negation-as-failure obligations above.
const COMPENSATION_TYPED_AS_ITS_RECEIPT: &str = include_str!(
    "../../../slices/grounding/logic/tests/counter-examples/compensation-typed-as-its-forward-receipt.ttl"
);

/// N9 — a `logic:ActionableFrontier` claiming closure with no saturation witness.
/// Trips `logic:FrontierCarriesSaturationWitnessConstraint`.
const FRONTIER_WITHOUT_SATURATION_WITNESS: &str = include_str!(
    "../../../slices/grounding/logic/tests/counter-examples/frontier-closed-without-saturation-witness.ttl"
);

/// A `logic:ActionableFrontier` citing a witness that carries no settled predicate and no
/// budget. Trips `logic:FrontierClosureRequiresSaturationConstraint` — the presence sibling
/// passes, which is exactly why counting the witness was never enough.
const FRONTIER_WITH_CONTENT_FREE_WITNESS: &str = include_str!(
    "../../../slices/grounding/logic/tests/counter-examples/frontier-cites-a-content-free-witness.ttl"
);

/// A `logic:PinnedExecutableSubgraph` freezing a sequence its named method never yielded.
/// Trips `logic:PinStepsMatchInstantiatedMethodConstraint`.
const PIN_MISMATCHED_WITH_ITS_METHOD: &str = include_str!(
    "../../../slices/grounding/logic/tests/counter-examples/pin-freezes-steps-its-method-never-yielded.ttl"
);

// ── The RELATIONAL counter-example fixtures ──────────────────────────────────────────
//
// Each of the six below is the MISMATCH or SUBSTITUTION twin of an absence fixture above.
// Every record in them individually satisfies every presence law the kernel authors, so a
// red here can only have come from the relational body, and its absence sibling — which
// shares the record kind — can only reach the presence law.

/// A `logic:CheckpointRestore` whose fold disagrees with the checkpoint it restores.
/// Trips `logic:CheckpointRestoreIdentityConstraint`, which the absence sibling cannot
/// reach because its guard needs both folds bound.
const CHECKPOINT_RESTORED_UNDER_A_DRIFTED_FOLD: &str = include_str!(
    "../../../slices/grounding/logic/tests/counter-examples/checkpoint-restored-under-a-drifted-fold.ttl"
);

/// An undetermined outcome retried under the idempotency contract of a NEIGHBOURING
/// attempt. Trips `logic:NoBlindRetryConstraint` — the law the absence sibling's cell used
/// to name while tripping `logic:UnknownOutcomeNamesItsAttemptConstraint`.
const UNKNOWN_OUTCOME_RETRIED_ON_A_BORROWED_LICENCE: &str = include_str!(
    "../../../slices/grounding/logic/tests/counter-examples/unknown-outcome-retried-on-a-borrowed-licence.ttl"
);

/// A content-addressed `logic:PrescriptionVersion` whose address no longer matches what a
/// running enactment froze. Trips `logic:PrescriptionVersionImmutabilityConstraint`.
const PRESCRIPTION_VERSION_REVISED_IN_PLACE: &str = include_str!(
    "../../../slices/grounding/logic/tests/counter-examples/prescription-version-revised-under-a-running-enactment.ttl"
);

/// A frontier whose witness records `logic:BudgetExhausted` — the roster cut by a budget and
/// presented as closed. Trips `logic:FrontierClosureRequiresSaturationConstraint`.
const FRONTIER_CLOSED_ON_A_BUDGET_CUT: &str = include_str!(
    "../../../slices/grounding/logic/tests/counter-examples/frontier-closed-on-a-budget-cut-witness.ttl"
);

/// An OCR gap whose only proposal answers a different step, with the weaker parser recorded
/// as dispatched at the blocked one. Trips `logic:OperationalGapCarriesProposalConstraint`.
const OCR_GAP_REMEDIED_FOR_A_DIFFERENT_STEP: &str = include_str!(
    "../../../slices/grounding/logic/tests/counter-examples/ocr-gap-remedied-for-a-different-step.ttl"
);

/// A `logic:Advisory` retyped as a `logic:AuthorizationProof` and pointed at an intent.
/// Trips `logic:AdvisoryNeverAuthorityConstraint` through its PROOF leg — no pin is involved.
const ADVISORY_LAUNDERED_INTO_A_PROOF: &str = include_str!(
    "../../../slices/grounding/logic/tests/counter-examples/advisory-laundered-into-an-authorization-proof.ttl"
);

/// A continuing maintenance goal whose evaluation is BOTH `logic:Satisfied` and
/// `logic:GoalEvaluationCompleted` — one good week presented as the permanent closure of a
/// standing commitment. Trips `logic:MaintenanceGoalNeverConclusivelySatisfiedConstraint`.
const MAINTENANCE_GOAL_CLOSED_BY_ONE_GOOD_WEEK: &str = include_str!(
    "../../../slices/grounding/logic/tests/counter-examples/maintenance-goal-closed-by-one-good-week.ttl"
);

fn parse(ttl: &str) -> Arc<RdfDataset> {
    purrdf::parse_dataset(ttl.as_bytes(), "text/turtle", None).expect("fixture is valid Turtle")
}

/// Drive the production reason-verify entrypoint over `ttl` with the full embedded verify
/// query set.
fn run_verify(ttl: &str) -> gmeow_errors::model::Report {
    let ds = parse(ttl);
    verify(ds.as_ref(), &embedded_verify_queries()).expect("verify() must not error on the fixture")
}

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

// ── The obligation shape: a missing mandatory binding, decided by existential NAF ──────

/// A shipped counter-example that breaks an authored kernel law produces a real finding.
#[test]
fn a_receipt_with_no_attempt_fires_on_verify() {
    let report = run_verify(RECEIPT_WITHOUT_ATTEMPT);
    assert_condemns(&report, "receiptNoAttempt");
}

/// The green half: the SAME record with the missing binding supplied passes.
///
/// Only this makes the red case above informative. A gate that fired on every
/// `logic:ExternalEffectReceipt` would satisfy the red test and be worthless.
#[test]
fn a_receipt_that_names_its_attempt_passes_on_verify() {
    let satisfied = "\
@prefix logic: <https://blackcatinformatics.ca/logic/> .
@prefix ex:    <https://blackcatinformatics.ca/gmeow/examples/logic/tests/> .

ex:receiptNoAttempt a logic:ExternalEffectReceipt ;
    logic:receiptOfAttempt ex:attempt1 .
ex:attempt1 a logic:EffectAttempt .
";
    let report = run_verify(satisfied);
    assert_clean(
        &report,
        "a receipt that names the attempt it reports on satisfies \
         logic:ReceiptRequiresAttemptConstraint and must raise nothing",
    );
}

/// A second obligation law, over a different record kind, so the coverage is not one law
/// wearing the costume of a corpus.
#[test]
fn an_unknown_outcome_with_no_attempt_fires_on_verify() {
    let report = run_verify(UNKNOWN_OUTCOME_WITHOUT_ATTEMPT);
    assert_condemns(&report, "unknownNoAttempt");
}

/// The frontier-saturation law, whose violation is the headline enactment record claiming
/// a closedness it cannot witness.
#[test]
fn a_frontier_claiming_closure_without_a_witness_fires_on_verify() {
    let report = run_verify(FRONTIER_WITHOUT_SATURATION_WITNESS);
    assert_condemns(&report, "frontierNoWitness");
}

// ── The prohibition shape: a forbidden co-occurrence, decided positively ──────────────

/// A prohibition law fires: the violation rule's body is the forbidden pattern itself,
/// carrying no NAF literal at all, so this exercises the other half of the lowering.
#[test]
fn a_compensation_typed_as_its_own_forward_receipt_fires_on_verify() {
    let report = run_verify(COMPENSATION_TYPED_AS_ITS_RECEIPT);
    assert_condemns(&report, "compensationAsInverse");
}

/// The prohibition's green half: a compensation that is NOT typed as a receipt, and that
/// binds the forward receipt it addresses, passes.
#[test]
fn a_compensation_bound_to_its_forward_receipt_passes_on_verify() {
    let satisfied = "\
@prefix logic: <https://blackcatinformatics.ca/logic/> .
@prefix ex:    <https://blackcatinformatics.ca/gmeow/examples/logic/tests/> .

ex:compensationAsInverse a logic:CompensationAttempt ;
    logic:compensatesEffect ex:forwardReceipt .
ex:forwardReceipt a logic:ExternalEffectReceipt ;
    logic:receiptOfAttempt ex:forwardAttempt .
ex:forwardAttempt a logic:EffectAttempt .
";
    let report = run_verify(satisfied);
    assert_clean(
        &report,
        "a compensation that counteracts a receipt without BEING one is exactly what the \
         kernel models and must raise nothing",
    );
}

// ── The gate's domain is the reasoned closure, not the raw asserted EDB ───────────────

/// A record whose kernel type is DERIVED must still be gated.
///
/// Nothing types `ex:lease-7` as a `logic:ResourceLease`; the scene asserts only that a
/// locally-named class is a subclass of one and that the record belongs to the local class,
/// so the type exists ONLY in the reasoned closure. A gate deciding over the raw EDB would
/// never classify the record and would MISS the violation — and every domain slice that
/// specializes a kernel class (which is how the kernel is meant to be used at all) would
/// silently escape the kernel's own laws.
#[test]
fn a_derived_kernel_type_is_still_gated() {
    let derived_type = "\
@prefix logic: <https://blackcatinformatics.ca/logic/> .
@prefix rdfs:  <http://www.w3.org/2000/01/rdf-schema#> .
@prefix ex:    <https://blackcatinformatics.ca/gmeow/examples/logic/tests/> .

ex:ActuatorLease rdfs:subClassOf logic:ResourceLease .
ex:lease-7 a ex:ActuatorLease .
";
    let report = run_verify(derived_type);
    assert_condemns(&report, "lease-7");
}

/// The same scene with the obligation met passes — proving the previous test's red came
/// from the missing fencing identity and not from the subclass modelling itself.
#[test]
fn a_derived_kernel_type_meeting_its_obligation_passes() {
    let derived_type_satisfied = "\
@prefix logic: <https://blackcatinformatics.ca/logic/> .
@prefix rdfs:  <http://www.w3.org/2000/01/rdf-schema#> .
@prefix ex:    <https://blackcatinformatics.ca/gmeow/examples/logic/tests/> .

ex:ActuatorLease rdfs:subClassOf logic:ResourceLease .
ex:lease-7 a ex:ActuatorLease ;
    logic:fencingIdentity ex:fence-42 .
";
    let report = run_verify(derived_type_satisfied);
    assert_clean(
        &report,
        "a lease carrying a fencing identity satisfies \
         logic:LeaseCarriesFencingIdentityConstraint, however its type was reached",
    );
}

// ── The observed-not-derived boundary holds over the gate's own output ────────────────

/// Condemning an ASSERTED effect attempt is a legitimate derivation, not a boundary breach.
///
/// `logic:EffectRecordsAreObservedNotDerivedConstraint` forbids the engine from DERIVING an
/// effect attempt. It does not forbid the engine from deriving a FINDING about one the world
/// asserted — that is the kernel's whole purpose. The gate's markers are put through
/// `reject_banned_heads` on the way out, so a guard that read a marker on an effect record
/// as "the engine derived an effect record" would abort `verify()` here instead of reporting
/// the violation. This pins that it does not.
#[test]
fn condemning_an_asserted_effect_attempt_is_a_finding_not_a_boundary_breach() {
    let attempt_with_derivation_provenance = "\
@prefix logic: <https://blackcatinformatics.ca/logic/> .
@prefix ex:    <https://blackcatinformatics.ca/gmeow/examples/logic/tests/> .

ex:attempt-9 a logic:EffectAttempt ;
    logic:derivationIdentifier \"derivation-42\" .
";
    let report = run_verify(attempt_with_derivation_provenance);
    assert_condemns(&report, "attempt-9");
}

/// An ordinary, well-formed effect attempt raises nothing.
#[test]
fn an_observed_effect_attempt_with_no_derivation_provenance_passes() {
    let observed = "\
@prefix logic: <https://blackcatinformatics.ca/logic/> .
@prefix ex:    <https://blackcatinformatics.ca/gmeow/examples/logic/tests/> .

ex:attempt-9 a logic:EffectAttempt .
";
    let report = run_verify(observed);
    assert_clean(
        &report,
        "an effect attempt the dispatching organ wrote down is the observation the kernel \
         reasons about, not a violation",
    );
}

// ── An empty scene is clean, and clean means the laws RAN ─────────────────────────────

/// A dataset carrying no enactment record at all raises nothing.
///
/// The trivial case, and worth pinning precisely because it is the one an unwired gate also
/// passes: the census tests in `gmeow_logic::reason::enactment` are what separate "clean"
/// from "dark", and this pins that the clean path does not error, hang, or fabricate.
#[test]
fn a_scene_with_no_enactment_record_raises_nothing() {
    let unrelated = "\
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
@prefix ex:   <https://blackcatinformatics.ca/gmeow/examples/logic/tests/> .

ex:thing rdfs:label \"a datum no kernel law governs\" .
";
    let report = run_verify(unrelated);
    assert_clean(&report, "a scene with no enactment record is clean");
}

// ── The RELATIONAL laws ───────────────────────────────────────────────────────────────
//
// Every test below drives the same production `verify()` entrypoint over a scene whose
// records each individually satisfy every PRESENCE law the kernel authors, and whose only
// defect is the RELATION the law under test names. That construction is the point: a
// presence check cannot fail on any of these scenes, so a green red-half proves the
// relational body ran, and a red green-half proves it discriminates rather than condemning
// its record kind wholesale.

const PREFIXES: &str = "\
@prefix logic: <https://blackcatinformatics.ca/logic/> .
@prefix rdf:   <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .
@prefix xsd:   <http://www.w3.org/2001/XMLSchema#> .
@prefix ex:    <https://blackcatinformatics.ca/gmeow/examples/logic/tests/> .
";

fn scene(body: &str) -> String {
    format!("{PREFIXES}{body}")
}

// ── Approval: the digest binds an intent, and the intent belongs to the approved run ──

/// An approval addressed to a digest no dispatch intent carries authorizes nothing.
#[test]
fn an_approval_digest_matching_no_intent_fires_on_verify() {
    let report = run_verify(&scene(
        "
ex:approvalA a logic:ApprovalCommitment ;
    logic:approvalIntentDigest \"b3:aaaa\" .
ex:intentA a logic:DispatchIntent ;
    logic:intentDigest \"b3:bbbb\" .
",
    ));
    assert_condemns_under(
        &report,
        "approvalA",
        "ApprovalDigestBindsDispatchIntentConstraint",
    );
}

/// The same approval whose digest IS an intent's digest passes the binding law.
#[test]
fn an_approval_digest_matching_an_intent_passes_on_verify() {
    let report = run_verify(&scene(
        "
ex:approvalA a logic:ApprovalCommitment ;
    logic:approvalIntentDigest \"b3:aaaa\" .
ex:intentA a logic:DispatchIntent ;
    logic:intentDigest \"b3:aaaa\" .
",
    ));
    assert_law_silent(
        &report,
        "ApprovalDigestBindsDispatchIntentConstraint",
        "an approval whose digest is an actual intent's digest is exactly the exact binding \
         the kernel models",
    );
}

/// An approval scoped to one run may not authorize an intent belonging to another.
///
/// The digest matches, so this is unreachable for any check over the approval alone: the
/// two runs produced byte-identical intents, which is what makes cross-run authorization a
/// real failure rather than a hypothetical one.
#[test]
fn an_approval_for_another_runs_intent_fires_on_verify() {
    let report = run_verify(&scene(
        "
ex:approvalB a logic:ApprovalCommitment ;
    logic:approvalIntentDigest \"b3:cccc\" ;
    logic:approvalEnactment ex:runOne .
ex:intentB a logic:DispatchIntent ;
    logic:intentDigest \"b3:cccc\" ;
    logic:intentOfEnactment ex:runTwo .
",
    ));
    assert_condemns_under(
        &report,
        "approvalB",
        "ApprovalScopedToIntentEnactmentConstraint",
    );
}

/// A record that breaks TWO laws is reported under BOTH, not under whichever one wins.
///
/// The regression test for the defect that made the law above go dark. Every kernel law
/// shares one `gmeow:enforcesFailureClass`, so while each violation rule headed on
/// `record rdf:type logic:EnactmentIntegrityViolation` every law derived the SAME tuple —
/// and the chase keeps exactly one winning derivation per derived tuple. `ex:approvalB`
/// below breaks its completeness law (it binds two of six fields) AND its scoping law (it
/// authorizes an intent belonging to another run); the shared conclusion meant the chase
/// emitted one row, the completeness law won it, and the scoping law reported nothing while
/// compiling, running, joining, and appearing in every census.
///
/// Asserting both laws by name is the only form of this test that bites. "The record was
/// condemned" was true throughout the defect, and so was "the completeness law fired".
#[test]
fn a_record_breaking_two_laws_is_condemned_under_both() {
    let report = run_verify(&scene(
        "
ex:approvalB a logic:ApprovalCommitment ;
    logic:approvalIntentDigest \"b3:cccc\" ;
    logic:approvalEnactment ex:runOne .
ex:intentB a logic:DispatchIntent ;
    logic:intentDigest \"b3:cccc\" ;
    logic:intentOfEnactment ex:runTwo .
",
    ));
    assert_condemns_under(
        &report,
        "approvalB",
        "ApprovalCommitmentCompletenessConstraint",
    );
    assert_condemns_under(
        &report,
        "approvalB",
        "ApprovalScopedToIntentEnactmentConstraint",
    );
}

#[test]
fn an_approval_for_its_own_runs_intent_passes_on_verify() {
    let report = run_verify(&scene(
        "
ex:approvalB a logic:ApprovalCommitment ;
    logic:approvalIntentDigest \"b3:cccc\" ;
    logic:approvalEnactment ex:runOne .
ex:intentB a logic:DispatchIntent ;
    logic:intentDigest \"b3:cccc\" ;
    logic:intentOfEnactment ex:runOne .
",
    ));
    assert_law_silent(
        &report,
        "ApprovalScopedToIntentEnactmentConstraint",
        "an approval scoped to the run its intent belongs to is the ordinary case",
    );
}

// ── Restore: the identity gate, which needs two records to be a gate at all ───────────

#[test]
fn a_restore_whose_fold_differs_from_its_checkpoint_fires_on_verify() {
    let report = run_verify(&scene(
        "
ex:restore1 a logic:CheckpointRestore ;
    logic:restoresCheckpoint ex:ckpt1 ;
    logic:restoreDescriptorHash \"b3:enginev2\" .
ex:ckpt1 a logic:EnactmentCheckpoint ;
    logic:checkpointDescriptorHash \"b3:enginev1\" .
",
    ));
    assert_condemns_under(&report, "restore1", "CheckpointRestoreIdentityConstraint");
}

#[test]
fn a_restore_whose_fold_equals_its_checkpoint_passes_on_verify() {
    let report = run_verify(&scene(
        "
ex:restore1 a logic:CheckpointRestore ;
    logic:restoresCheckpoint ex:ckpt1 ;
    logic:restoreDescriptorHash \"b3:enginev1\" .
ex:ckpt1 a logic:EnactmentCheckpoint ;
    logic:checkpointDescriptorHash \"b3:enginev1\" .
",
    ));
    assert_law_silent(
        &report,
        "CheckpointRestoreIdentityConstraint",
        "a restore whose folded identity matches its checkpoint's is a passing identity gate",
    );
}

/// The second identity axis: the fold agrees and the RUN does not.
#[test]
fn a_restore_into_a_different_enactment_fires_on_verify() {
    let report = run_verify(&scene(
        "
ex:restore2 a logic:CheckpointRestore ;
    logic:restoresCheckpoint ex:ckpt2 ;
    logic:restoreDescriptorHash \"b3:same\" ;
    logic:restoreOfEnactment ex:runAlpha .
ex:ckpt2 a logic:EnactmentCheckpoint ;
    logic:checkpointDescriptorHash \"b3:same\" ;
    logic:checkpointOfEnactment ex:runBeta .
",
    ));
    assert_condemns_under(
        &report,
        "restore2",
        "RestoreStaysWithinItsEnactmentConstraint",
    );
    assert_law_silent(
        &report,
        "CheckpointRestoreIdentityConstraint",
        "the folds agree here, so the fold law must stay silent — otherwise the two identity \
         axes are not independently detectable",
    );
}

// ── Context assembly: the exclusions the law's own name promised to read ──────────────

#[test]
fn an_assembly_withholding_material_with_no_reason_fires_on_verify() {
    let report = run_verify(&scene(
        "
ex:assembly1 a logic:ContextAssembly ;
    logic:assemblyForEnactment ex:run1 ;
    logic:assemblyExcluded ex:staleObjection .
",
    ));
    assert_condemns_under(
        &report,
        "assembly1",
        "ContextAssemblyRecordsExclusionsConstraint",
    );
}

#[test]
fn an_assembly_recording_why_it_withheld_passes_on_verify() {
    let report = run_verify(&scene(
        "
ex:assembly1 a logic:ContextAssembly ;
    logic:assemblyForEnactment ex:run1 ;
    logic:assemblyExcluded ex:staleObjection ;
    logic:assemblyExclusionReason \"superseded by revision 4\" .
",
    ));
    assert_law_silent(
        &report,
        "ContextAssemblyRecordsExclusionsConstraint",
        "an exclusion carrying its reason is the record the assembly exists to make",
    );
}

#[test]
fn an_item_both_surfaced_and_withheld_fires_on_verify() {
    let report = run_verify(&scene(
        "
ex:assembly2 a logic:ContextAssembly ;
    logic:assemblyForEnactment ex:run1 ;
    logic:assemblyIncluded ex:adrNote ;
    logic:assemblyExcluded ex:adrNote ;
    logic:assemblyExclusionReason \"over budget\" .
",
    ));
    assert_condemns_under(
        &report,
        "assembly2",
        "ContextAssemblyExclusionIsNotInclusionConstraint",
    );
}

// ── The operational gap actually carrying its remedy ──────────────────────────────────

#[test]
fn a_capability_gap_with_no_proposal_for_its_step_fires_on_verify() {
    let report = run_verify(&scene(
        "
ex:gap1 a logic:OperationalCapabilityGap ;
    logic:gapBlockedStep ex:ocrStep .
",
    ));
    assert_condemns_under(&report, "gap1", "OperationalGapCarriesProposalConstraint");
}

#[test]
fn a_capability_gap_answered_by_a_proposal_passes_on_verify() {
    let report = run_verify(&scene(
        "
ex:gap1 a logic:OperationalCapabilityGap ;
    logic:gapBlockedStep ex:ocrStep .
ex:proposal1 a logic:CapabilityGapProposal ;
    logic:proposalMissingCapability ex:pdfOcr ;
    logic:proposalRequiredContract \"Page image in, positioned text out.\" ;
    logic:proposalExpectedInputs \"Rasterised page images.\" ;
    logic:proposalExpectedOutputs \"Per-page text with confidences.\" ;
    logic:proposalExpectedEffects \"Reads batch images; writes text alongside.\" ;
    logic:proposalVerificationMethod \"CER below 2% on the ground-truth set.\" ;
    logic:proposalSecurityLifecycle \"Runs inside the data boundary.\" ;
    logic:proposalBlockedStep ex:ocrStep .
",
    ));
    assert_law_silent(
        &report,
        "OperationalGapCarriesProposalConstraint",
        "a gap whose blocked step a complete proposal names is the discipline working",
    );
}

// ── No blind retry, over the retry record the kernel now carries ──────────────────────

#[test]
fn a_retry_under_a_licence_for_a_different_attempt_fires_on_verify() {
    let report = run_verify(&scene(
        "
ex:retry1 a logic:RetryDispatch ;
    logic:retryOfAttempt ex:attemptOne ;
    logic:retryLicence ex:contract1 .
ex:contract1 a logic:IdempotencyContract ;
    logic:idempotencyScope ex:acmePayments ;
    logic:idempotencyKey \"invoice:90232:charge\" ;
    logic:idempotencyRetention \"PT24H\" ;
    logic:idempotencyDuplicateSemantics ex:collapseWithinWindow ;
    logic:idempotencyProviderEvidence ex:acmeSpec ;
    logic:licenceCoversAttempt ex:attemptTwo .
ex:attemptOne a logic:EffectAttempt .
ex:attemptTwo a logic:EffectAttempt .
",
    ));
    assert_condemns_under(&report, "retry1", "NoBlindRetryConstraint");
}

#[test]
fn a_retry_under_a_licence_covering_its_own_attempt_passes_on_verify() {
    let report = run_verify(&scene(
        "
ex:retry1 a logic:RetryDispatch ;
    logic:retryOfAttempt ex:attemptOne ;
    logic:retryLicence ex:contract1 .
ex:contract1 a logic:IdempotencyContract ;
    logic:idempotencyScope ex:acmePayments ;
    logic:idempotencyKey \"invoice:90233:charge\" ;
    logic:idempotencyRetention \"PT24H\" ;
    logic:idempotencyDuplicateSemantics ex:collapseWithinWindow ;
    logic:idempotencyProviderEvidence ex:acmeSpec ;
    logic:licenceCoversAttempt ex:attemptOne .
ex:attemptOne a logic:EffectAttempt .
",
    ));
    assert_law_silent(
        &report,
        "NoBlindRetryConstraint",
        "a retry whose licence names the very attempt it re-sends is the licensed case",
    );
}

#[test]
fn a_retry_naming_no_licence_at_all_fires_on_verify() {
    let report = run_verify(&scene(
        "
ex:retry2 a logic:RetryDispatch ;
    logic:retryOfAttempt ex:attemptOne .
ex:attemptOne a logic:EffectAttempt .
",
    ));
    assert_condemns_under(&report, "retry2", "RetryRequiresLicenceConstraint");
}

// ── The journal chain: entry n linked to entry n−1 ────────────────────────────────────

#[test]
fn a_journal_entry_applied_against_an_unestablished_head_fires_on_verify() {
    let report = run_verify(&scene(
        "
ex:entry8 a logic:JournalEntry ;
    logic:journalPredecessor ex:entry7 ;
    logic:journalPrevHead \"b3:never-established\" ;
    logic:journalNewHead \"b3:eight\" .
ex:entry7 a logic:JournalEntry ;
    logic:journalPrevHead \"b3:six\" ;
    logic:journalNewHead \"b3:seven\" .
",
    ));
    assert_condemns_under(&report, "entry8", "JournalChainIntegrityConstraint");
}

#[test]
fn a_journal_entry_linked_to_its_predecessor_passes_on_verify() {
    let report = run_verify(&scene(
        "
ex:entry8 a logic:JournalEntry ;
    logic:journalPredecessor ex:entry7 ;
    logic:journalPrevHead \"b3:seven\" ;
    logic:journalNewHead \"b3:eight\" .
ex:entry7 a logic:JournalEntry ;
    logic:journalPrevHead \"b3:six\" ;
    logic:journalNewHead \"b3:seven\" .
",
    ));
    assert_law_silent(
        &report,
        "JournalChainIntegrityConstraint",
        "an entry whose prior head is its predecessor's new head IS the chain",
    );
}

// ── Lease exclusivity: two overlapping leases over one scope ──────────────────────────

/// Two leases, each impeccable on its own, over one scope. Only the non-holder is condemned.
#[test]
fn a_second_lease_over_a_held_scope_fires_on_verify() {
    let report = run_verify(&scene(
        "
ex:leaseA a logic:ResourceLease ;
    logic:leaseScope ex:valveFV104 ;
    logic:fencingIdentity 41 .
ex:leaseB a logic:ResourceLease ;
    logic:leaseScope ex:valveFV104 ;
    logic:fencingIdentity 42 .
ex:valveFV104 logic:scopeExclusiveHolder ex:leaseB .
",
    ));
    assert_condemns_under(&report, "leaseA", "LeaseExclusivityConstraint");
    let holder_rows = rows_naming(&report, "leaseB");
    assert!(
        !holder_rows
            .iter()
            .any(|row| row.contains("LeaseExclusivityConstraint")),
        "the scope's own holder must NOT be condemned by the exclusivity law — a law that \
         condemns both claimants has not decided anything; rows were: {holder_rows:?}"
    );
}

#[test]
fn the_sole_holder_of_a_scope_passes_on_verify() {
    let report = run_verify(&scene(
        "
ex:leaseB a logic:ResourceLease ;
    logic:leaseScope ex:valveFV104 ;
    logic:fencingIdentity 42 .
ex:valveFV104 logic:scopeExclusiveHolder ex:leaseB .
",
    ));
    assert_law_silent(
        &report,
        "LeaseExclusivityConstraint",
        "one lease over one scope, and it is the scope's holder, is exactly exclusivity",
    );
}

// ── Compensation: the EXACT forward effect, dereferenced ──────────────────────────────

/// Binding a reconciliation result rather than the forward receipt satisfies every presence
/// check and names a class of effects rather than the one that committed.
#[test]
fn a_compensation_bound_to_something_that_is_not_a_receipt_fires_on_verify() {
    let report = run_verify(&scene(
        "
ex:refund1 a logic:CompensationAttempt ;
    logic:compensatesEffect ex:probeResult .
ex:probeResult a logic:ReconciliationResult ;
    logic:reconciliationVerdict logic:ReconciledCommitted .
",
    ));
    assert_condemns_under(
        &report,
        "refund1",
        "CompensationBindsExactForwardEffectConstraint",
    );
}

#[test]
fn a_compensation_bound_to_a_real_forward_receipt_passes_on_verify() {
    let report = run_verify(&scene(
        "
ex:refund1 a logic:CompensationAttempt ;
    logic:compensatesEffect ex:chargeReceipt .
ex:chargeReceipt a logic:ExternalEffectReceipt ;
    logic:receiptOfAttempt ex:chargeAttempt .
ex:chargeAttempt a logic:EffectAttempt .
",
    ));
    assert_law_silent(
        &report,
        "CompensationBindsExactForwardEffectConstraint",
        "a compensation bound to a receipt that reports a real attempt is the exact binding",
    );
}

/// A compensation outcome evidenced by the very receipt it was undoing observed nothing new.
#[test]
fn a_compensation_outcome_reusing_the_forward_receipt_fires_on_verify() {
    let report = run_verify(&scene(
        "
ex:refundOutcome a logic:CompensationOutcome ;
    logic:outcomeOfCompensation ex:refund2 ;
    logic:compensationReceipt ex:chargeReceipt .
ex:refund2 a logic:CompensationAttempt ;
    logic:compensatesEffect ex:chargeReceipt .
ex:chargeReceipt a logic:ExternalEffectReceipt ;
    logic:receiptOfAttempt ex:chargeAttempt .
ex:chargeAttempt a logic:EffectAttempt .
",
    ));
    assert_condemns_under(
        &report,
        "refundOutcome",
        "CompensationOutcomeReceiptIsNotTheForwardReceiptConstraint",
    );
}

// ── Frontier closure: read the witness, do not count it ───────────────────────────────

/// The complete witness the green halves below are built from: settled predicates, the
/// budget they were proved under, and a completed evaluation.
const WITNESSED_CLOSURE: &str = "
ex:saturation1 a logic:SaturationWitness ;
    logic:settledPredicate logic:hasFrontierEntry ;
    logic:consumedBudget \"64\"^^xsd:nonNegativeInteger ;
    logic:resultEvaluation logic:EvaluationCompleted .
";

#[test]
fn a_frontier_whose_witness_records_a_budget_cut_fires_on_verify() {
    let report = run_verify(&scene(
        "
ex:frontier1 a logic:ActionableFrontier ;
    logic:frontierSaturationWitness ex:saturation1 .
ex:saturation1 a logic:SaturationWitness ;
    logic:settledPredicate logic:hasFrontierEntry ;
    logic:consumedBudget \"64\"^^xsd:nonNegativeInteger ;
    logic:resultEvaluation logic:BudgetExhausted .
",
    ));
    assert_condemns_under(
        &report,
        "frontier1",
        "FrontierClosureRequiresSaturationConstraint",
    );
}

/// The witness that is CITED but says nothing.
///
/// This is the case the law was extended to reach, and it is worse than the frontier that
/// cites nothing: the presence sibling passes, and the not-exhausted test passes vacuously
/// because there is no evaluation status to be exhausted. A citation that cannot be wrong
/// reads to an operator exactly like one that has been checked.
#[test]
fn a_frontier_citing_a_witness_with_no_settled_predicate_fires_on_verify() {
    let report = run_verify(&scene(
        "
ex:frontier1 a logic:ActionableFrontier ;
    logic:frontierSaturationWitness ex:saturation1 .
ex:saturation1 a logic:SaturationWitness ;
    logic:consumedBudget \"64\"^^xsd:nonNegativeInteger ;
    logic:resultEvaluation logic:EvaluationCompleted .
",
    ));
    assert_condemns_under(
        &report,
        "frontier1",
        "FrontierClosureRequiresSaturationConstraint",
    );
}

/// The other content leg: predicates named, but no budget they were proved under.
///
/// Separate from the settled-predicate case because they fail independently — a witness may
/// name what settled while omitting how much search that took, and "settled" without a
/// bound is not a fixed-point claim a reader can rerun.
#[test]
fn a_frontier_citing_a_witness_with_no_budget_fires_on_verify() {
    let report = run_verify(&scene(
        "
ex:frontier1 a logic:ActionableFrontier ;
    logic:frontierSaturationWitness ex:saturation1 .
ex:saturation1 a logic:SaturationWitness ;
    logic:settledPredicate logic:hasFrontierEntry ;
    logic:resultEvaluation logic:EvaluationCompleted .
",
    ));
    assert_condemns_under(
        &report,
        "frontier1",
        "FrontierClosureRequiresSaturationConstraint",
    );
}

/// The SHIPPED content-free-witness counter-example fires through the real entrypoint.
///
/// Driving the artifact rather than an inline string is what keeps the slice's own named
/// fail witness and the gate from drifting apart.
#[test]
fn the_shipped_content_free_witness_fixture_fires_on_verify() {
    let report = run_verify(FRONTIER_WITH_CONTENT_FREE_WITNESS);
    assert_condemns_under(
        &report,
        "frontierEmptyWitness",
        "FrontierClosureRequiresSaturationConstraint",
    );
}

#[test]
fn a_frontier_whose_witness_ran_to_completion_passes_on_verify() {
    let report = run_verify(&scene(&format!(
        "
ex:frontier1 a logic:ActionableFrontier ;
    logic:frontierSaturationWitness ex:saturation1 .
{WITNESSED_CLOSURE}"
    )));
    assert_law_silent(
        &report,
        "FrontierClosureRequiresSaturationConstraint",
        "a witness that names what settled, the budget it settled under, and a completed \
         evaluation certifies the fixed point the frontier claims",
    );
}

// ── The pin: it has content, and the content is what was validated ────────────────────

/// A pin carrying only a label breaks the completeness law.
///
/// "Immutable and content-addressed" is a claim about content, and a record with none is
/// immutable the way an empty box is locked.
#[test]
fn a_pin_with_no_frozen_content_fires_on_verify() {
    let report = run_verify(&scene(
        "
ex:labelOnlyPin a logic:PinnedExecutableSubgraph .
",
    ));
    assert_condemns_under(
        &report,
        "labelOnlyPin",
        "PinnedSubgraphCompletenessConstraint",
    );
}

/// The SHIPPED mismatch counter-example: the pin freezes steps its method never yielded.
///
/// Every record in that fixture is individually well-formed — the completeness law cannot
/// fire on it — so a green here proves the RELATIONAL body ran rather than its presence
/// sibling picking up the slack.
#[test]
fn a_pin_freezing_steps_its_method_never_yielded_fires_on_verify() {
    let report = run_verify(PIN_MISMATCHED_WITH_ITS_METHOD);
    assert_condemns_under(
        &report,
        "pinMismatchedWithMethod",
        "PinStepsMatchInstantiatedMethodConstraint",
    );
    assert_law_silent(
        &report,
        "PinnedSubgraphCompletenessConstraint",
        "the fixture's pin binds all three mandatory fields, so only the relation is wrong",
    );
}

/// The relational law's green half: the pin freezes the very list the method yields.
///
/// Without this, the red half above would be satisfied by a law condemning every pin that
/// names a method at all.
#[test]
fn a_pin_freezing_exactly_its_method_s_sequence_passes_on_verify() {
    let report = run_verify(&scene(
        "
ex:goodPin a logic:PinnedExecutableSubgraph ;
    logic:pinInstantiatesMethod ex:goodMethod ;
    logic:pinnedStepSequence ex:goodCell1 ;
    logic:pinDigest \"b3:0e5c\" .
ex:goodMethod a logic:DecompositionMethod ;
    logic:methodDecomposes ex:someTask ;
    logic:methodYields ex:goodCell1 .
ex:goodCell1 rdf:first ex:inspect ; rdf:rest ex:goodCell2 .
ex:goodCell2 rdf:first ex:extract ; rdf:rest rdf:nil .
",
    ));
    assert_law_silent(
        &report,
        "PinStepsMatchInstantiatedMethodConstraint",
        "a pin whose frozen sequence IS the method's yielded sequence is exactly what \
         pinning models",
    );
    assert_law_silent(
        &report,
        "PinnedSubgraphCompletenessConstraint",
        "the pin binds its step sequence, its digest and its method",
    );
}

// ── Immutability: the version's address against what a run froze ──────────────────────

#[test]
fn a_version_revised_under_a_running_enactment_fires_on_verify() {
    let report = run_verify(&scene(
        "
ex:reviewV4 a logic:PrescriptionVersion ;
    logic:prescriptionDigest \"b3:edited-in-place\" .
ex:weekThirteen a logic:Enactment ;
    logic:enactsPrescriptionVersion ex:reviewV4 ;
    logic:enactedPrescriptionDigest \"b3:as-pinned\" .
",
    ));
    assert_condemns_under(
        &report,
        "reviewV4",
        "PrescriptionVersionImmutabilityConstraint",
    );
}

#[test]
fn a_version_still_matching_what_its_run_pinned_passes_on_verify() {
    let report = run_verify(&scene(
        "
ex:reviewV4 a logic:PrescriptionVersion ;
    logic:prescriptionDigest \"b3:as-pinned\" .
ex:weekThirteen a logic:Enactment ;
    logic:enactsPrescriptionVersion ex:reviewV4 ;
    logic:enactedPrescriptionDigest \"b3:as-pinned\" .
",
    ));
    assert_law_silent(
        &report,
        "PrescriptionVersionImmutabilityConstraint",
        "a version whose address is the one its run froze has not been revised in place",
    );
}

// ── Continuation: the pair the resume-guarded law could never reach ───────────────────

/// Repeat AND revise. The resume-guarded disjointness law passes this vacuously, which is
/// exactly the gap: its message claims "exactly one" and its body only excludes two of the
/// three unordered pairs.
#[test]
fn a_continuation_claiming_both_repeat_and_revise_fires_on_verify() {
    let report = run_verify(&scene(
        "
ex:returnEntry logic:continuationKind logic:ContinuationRepeat , logic:ContinuationRevise .
",
    ));
    assert_condemns_under(
        &report,
        "returnEntry",
        "ContinuationRepeatExcludesReviseConstraint",
    );
}

#[test]
fn a_continuation_claiming_one_kind_passes_on_verify() {
    let report = run_verify(&scene(
        "
ex:returnEntry logic:continuationKind logic:ContinuationRepeat .
",
    ));
    assert_law_silent(
        &report,
        "ContinuationRepeatExcludesReviseConstraint",
        "a record carrying exactly one continuation kind conflates nothing",
    );
}

// ── Refinement: the pin was in the roster it was chosen from ──────────────────────────

#[test]
fn a_pin_outside_the_roster_it_was_selected_from_fires_on_verify() {
    let report = run_verify(&scene(
        "
ex:episode1 a logic:RefinementEpisode ;
    logic:producedCandidateSet ex:roster1 ;
    logic:selectedPin ex:ocrViaVendorX ;
    logic:searchFragment logic:FragmentBoundedDepth .
ex:roster1 a logic:RefinementCandidateSet ;
    logic:refinementCandidate ex:ocrViaTesseract .
",
    ));
    assert_condemns_under(
        &report,
        "episode1",
        "RefinementPinComesFromCandidateSetConstraint",
    );
}

#[test]
fn a_pin_drawn_from_its_own_roster_passes_on_verify() {
    let report = run_verify(&scene(
        "
ex:episode1 a logic:RefinementEpisode ;
    logic:producedCandidateSet ex:roster1 ;
    logic:selectedPin ex:ocrViaTesseract ;
    logic:searchFragment logic:FragmentBoundedDepth .
ex:roster1 a logic:RefinementCandidateSet ;
    logic:refinementCandidate ex:ocrViaTesseract .
",
    ));
    assert_law_silent(
        &report,
        "RefinementPinComesFromCandidateSetConstraint",
        "an episode that committed to a candidate its own roster contained is the ordinary case",
    );
}

// ── The finding carries the law's full identity, not merely its name ──────────────────

/// The marker-identity fix, pinned on the production surface.
///
/// The gate's marker is one shared `logic:EnactmentIntegrityViolation` type across forty-four
/// laws, so before the law identity was carried out of the chase an operator saw only that
/// a record breached enactment integrity. This asserts the finding carries the ABSOLUTE IRI
/// of the authored `logic:Constraint` — the thing whose `logic:message` states the
/// obligation in the author's own words — and that it reaches the structured citation
/// surface too, not only the rendered detail string.
#[test]
fn the_finding_names_the_authored_law_by_absolute_iri() {
    let report = run_verify(&scene(
        "
ex:lease-7 a logic:ResourceLease .
",
    ));
    let findings = violation_findings(&report);
    assert!(
        !findings.is_empty(),
        "the scene must raise a kernel finding"
    );
    let law_iri = "https://blackcatinformatics.ca/logic/LeaseCarriesFencingIdentityConstraint";
    assert!(
        findings
            .iter()
            .any(|f| f.detail.as_deref().is_some_and(|d| d.contains(law_iri))),
        "the rendered finding must carry the law's absolute IRI; details were: {:?}",
        findings
            .iter()
            .map(|f| f.detail.as_deref())
            .collect::<Vec<_>>()
    );
    assert!(
        findings
            .iter()
            .any(|f| f.cited_iris.iter().any(|iri| iri == law_iri)),
        "the law must reach the structured citation surface, so a consumer need not scrape \
         the detail string; citations were: {:?}",
        findings
            .iter()
            .map(|f| f.cited_iris.clone())
            .collect::<Vec<_>>()
    );
}

// ── The six relational negatives, red and green ───────────────────────────────────────
//
// Each pair drives the SHIPPED fixture through the production entrypoint and then drives
// the same scene with the one broken relation repaired. The red half proves the law can
// fire on the artifact the conformance corpus registers; the green half proves it
// discriminates rather than condemning its record kind wholesale. Both halves assert BY
// LAW NAME, because every one of these six record kinds is governed by a presence sibling
// that would otherwise pick up the slack invisibly.

/// N1 — the restore whose folded identity disagrees with its checkpoint's.
#[test]
fn a_restore_against_a_drifted_fold_fires_on_verify() {
    let report = run_verify(CHECKPOINT_RESTORED_UNDER_A_DRIFTED_FOLD);
    assert_condemns_under(
        &report,
        "resumeWeek13",
        "CheckpointRestoreIdentityConstraint",
    );
    assert_law_silent(
        &report,
        "CheckpointCarriesFoldedIdentityConstraint",
        "the fixture's checkpoint carries its fold, so the presence sibling must stay \
         silent — otherwise this is the absence fixture wearing a mismatch's clothes",
    );
    assert_law_silent(
        &report,
        "RestoreStaysWithinItsEnactmentConstraint",
        "the restore resumes the very run its checkpoint was taken from, so the second \
         identity axis holds and only the fold axis is wrong",
    );
}

#[test]
fn a_restore_whose_fold_agrees_with_its_checkpoint_passes_on_verify() {
    let report = run_verify(&scene(
        "
ex:resumeWeek13 a logic:CheckpointRestore ;
    logic:restoresCheckpoint ex:ckptWeek13 ;
    logic:restoreOfEnactment ex:adrReviewWeek13 ;
    logic:restoreDescriptorHash \"b3:8c15a02e7fb349d6015c8a37e04b2f91d6708ca3e52b19f7460d8b23a1c095e4\" .
ex:ckptWeek13 a logic:EnactmentCheckpoint ;
    logic:checkpointOfEnactment ex:adrReviewWeek13 ;
    logic:checkpointDescriptorHash \"b3:8c15a02e7fb349d6015c8a37e04b2f91d6708ca3e52b19f7460d8b23a1c095e4\" .
",
    ));
    assert_clean(
        &report,
        "the same scene with the restoring engine's fold equal to the checkpoint's is the \
         identity gate PASSING, and the kernel must raise nothing at all on it",
    );
}

/// N5 — the undetermined outcome retried under a neighbouring attempt's licence.
#[test]
fn an_unknown_outcome_retried_on_a_borrowed_licence_fires_on_verify() {
    let report = run_verify(UNKNOWN_OUTCOME_RETRIED_ON_A_BORROWED_LICENCE);
    assert_condemns_under(&report, "invoice901Retry", "NoBlindRetryConstraint");
    assert_law_silent(
        &report,
        "RetryRequiresLicenceConstraint",
        "the retry names a licence, so the presence sibling passes and only the coverage \
         relation is wrong",
    );
    assert_law_silent(
        &report,
        "UnknownOutcomeNamesItsAttemptConstraint",
        "the unknown outcome names its attempt; the defect is the retry, not the record of \
         the undetermined position",
    );
    assert_law_silent(
        &report,
        "IdempotencyContractCompletenessConstraint",
        "the borrowed contract binds all five of its fields — a real licence for a real \
         attempt, which is exactly what makes borrowing it undetectable by a field check",
    );
}

#[test]
fn a_retry_under_the_licence_for_its_own_attempt_passes_on_verify() {
    let report = run_verify(&scene(
        "
ex:invoice901Charge a logic:EffectAttempt .
ex:invoice901Unknown a logic:ExternalOutcomeUnknown ;
    logic:unknownOfAttempt ex:invoice901Charge .
ex:invoice901Retry a logic:RetryDispatch ;
    logic:retryOfAttempt ex:invoice901Charge ;
    logic:retryLicence ex:invoice901Idempotency .
ex:invoice901Idempotency a logic:IdempotencyContract ;
    logic:idempotencyScope ex:acmePayments ;
    logic:idempotencyKey \"invoice:901:charge\" ;
    logic:idempotencyRetention \"PT24H\" ;
    logic:idempotencyDuplicateSemantics ex:collapseWithinWindow ;
    logic:idempotencyProviderEvidence ex:acmeIdempotencySpec ;
    logic:licenceCoversAttempt ex:invoice901Charge .
",
    ));
    assert_clean(
        &report,
        "an undetermined outcome retried under the licence covering ITS OWN attempt is the \
         licensed case the whole commitment layer exists to make available",
    );
}

/// N3 — the landed version whose bytes were edited under a running occurrence.
#[test]
fn a_version_revised_in_place_fires_on_verify() {
    let report = run_verify(PRESCRIPTION_VERSION_REVISED_IN_PLACE);
    assert_condemns_under(
        &report,
        "adrReviewPrescriptionV4",
        "PrescriptionVersionImmutabilityConstraint",
    );
    assert_law_silent(
        &report,
        "PrescriptionVersionIsContentAddressedConstraint",
        "the version IS content-addressed — that is the precondition of the check, and the \
         absence sibling is the fixture that trips this one",
    );
    assert_law_silent(
        &report,
        "EnactmentPinsPrescriptionAndSnapshotConstraint",
        "the enactment pins both its version and its input generation, so the run is \
         reproducible and only the version underneath it moved",
    );
}

#[test]
fn a_version_whose_address_still_matches_what_its_run_froze_passes_on_verify() {
    let report = run_verify(&scene(
        "
ex:adrReviewPrescriptionV4 a logic:PrescriptionVersion ;
    logic:prescriptionDigest \"b3:b74e2a19c50d38f7ae6104b92cd75f038a1e6b4d09c27fa5813de640b2c9a731\" .
ex:adrReviewWeek13 a logic:Enactment ;
    logic:enactsPrescriptionVersion ex:adrReviewPrescriptionV4 ;
    logic:enactmentInputSnapshot ex:week13InputGeneration ;
    logic:enactedPrescriptionDigest \"b3:b74e2a19c50d38f7ae6104b92cd75f038a1e6b4d09c27fa5813de640b2c9a731\" .
",
    ));
    assert_clean(
        &report,
        "a version whose current address is the one its run froze at pin time has not been \
         revised in place, and nothing else in the scene is wrong",
    );
}

/// N9 — the roster cut by a budget and presented as closed.
#[test]
fn a_frontier_closed_on_a_budget_cut_witness_fires_on_verify() {
    let report = run_verify(FRONTIER_CLOSED_ON_A_BUDGET_CUT);
    assert_condemns_under(
        &report,
        "frontierCutForBudget",
        "FrontierClosureRequiresSaturationConstraint",
    );
    assert_law_silent(
        &report,
        "FrontierCarriesSaturationWitnessConstraint",
        "the frontier CITES a witness, so the counting sibling passes — which is the whole \
         reason reading the witness had to become a separate law",
    );
}

#[test]
fn a_frontier_whose_witness_reached_a_fixed_point_passes_on_verify() {
    let report = run_verify(&scene(
        "
ex:frontierCutForBudget a logic:ActionableFrontier ;
    logic:frontierSaturationWitness ex:truncatedSaturation .
ex:truncatedSaturation a logic:SaturationWitness ;
    logic:settledPredicate logic:hasFrontierEntry ;
    logic:consumedBudget \"64\"^^xsd:nonNegativeInteger ;
    logic:resultEvaluation logic:EvaluationCompleted .
",
    ));
    assert_clean(
        &report,
        "the same roster whose witness ran to completion rather than to its bound certifies \
         the fixed point the frontier claims, and raises nothing",
    );
}

/// The silent substitution — the OCR gap whose only proposal answers another step.
#[test]
fn an_ocr_gap_remedied_for_another_step_fires_on_verify() {
    let report = run_verify(OCR_GAP_REMEDIED_FOR_A_DIFFERENT_STEP);
    assert_condemns_under(
        &report,
        "noOcrProvider",
        "OperationalGapCarriesProposalConstraint",
    );
    assert_law_silent(
        &report,
        "OperationalGapNamesBlockedStepConstraint",
        "the gap names the step it blocks, so the presence sibling passes and the failure is \
         purely the join between the gap and the remedy",
    );
    assert_law_silent(
        &report,
        "CapabilityGapProposalCompletenessConstraint",
        "the proposal in the scene binds all eight fields — it is a real remedy for a real \
         blockage, just not for THIS one",
    );
    assert_law_silent(
        &report,
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
        &report,
        "ocrStepIntent",
        "NoDispatchAgainstAnUnremediedGapConstraint",
    );
}

/// The green half of the substitution law: the SUBSTITUTING INTENT IS STILL THERE.
///
/// This is the whole discipline of the pair. Deleting the dispatch intent would make the
/// scene green for a reason that has nothing to do with the law — no intent, no guard, no
/// finding — and would leave a law that fires on every scene carrying a dispatch intent
/// equally well. So the intent stays, whole and nine-field complete, dispatched at the very
/// step the gap blocks; what changes is that the gap is now ANSWERED. A blocked step whose
/// gap somebody is provisioning for is a scene the kernel models; a blocked step dispatched
/// into an unanswered gap is the silent substitution.
#[test]
fn a_dispatch_at_a_blocked_step_whose_gap_is_answered_passes_on_verify() {
    let report = run_verify(&scene(
        "
ex:noOcrProvider a logic:OperationalCapabilityGap ;
    logic:gapBlockedStep ex:ocrStep .
ex:ocrProposal a logic:CapabilityGapProposal ;
    logic:proposalMissingCapability ex:pdfOcr ;
    logic:proposalRequiredContract \"Page image in, positioned text out.\" ;
    logic:proposalExpectedInputs \"Rasterised page images, 300dpi or better.\" ;
    logic:proposalExpectedOutputs \"Per-page text with confidence scores.\" ;
    logic:proposalExpectedEffects \"Reads batch images; writes text alongside them.\" ;
    logic:proposalVerificationMethod \"CER below 2% on the ground-truth set.\" ;
    logic:proposalSecurityLifecycle \"Runs inside the data boundary; retains no page content.\" ;
    logic:proposalBlockedStep ex:ocrStep .
ex:ocrStepIntent a logic:DispatchIntent ;
    logic:intentOfEnactment ex:adrReviewWeek13 ;
    logic:intentOfStep ex:ocrStep ;
    logic:intentActionSchemaVersion ex:plainTextExtractV3 ;
    logic:intentNormalizedArguments ex:ocrStepArguments ;
    logic:intentTarget ex:documentIngestService ;
    logic:intentPolicy ex:ingestPolicy ;
    logic:intentApprovalRequired false ;
    logic:intentDigest \"b3:5a3e08c7b16d29f405ac83b71e0d64f2a97c5308be14d7a2609f3bc851d0e746\" ;
    logic:expectedJournalHead \"b3:c04b91e7a5d3268f0b17ce49d0a53f8261bd794c0e5a3f16b28d0c74a9e5312\" .
",
    ));
    assert_law_silent(
        &report,
        "NoDispatchAgainstAnUnremediedGapConstraint",
        "the intent that dispatched the blocked step is still in the scene, byte for byte — \
         what changed is that a complete proposal now answers the gap, so the green half \
         differs by the REMEDY and not by the evidence being deleted",
    );
    assert_clean(
        &report,
        "a gap whose blocked step a complete proposal names is the discipline working: the \
         blockage arrived with the statement of what would close it, and the dispatch \
         record that accompanies it is complete on all nine fields",
    );
}

// ── The continuing goal a successful occurrence may not close ─────────────────────────

/// The SHIPPED counter-example: one good week presented as the closure of a standing goal.
///
/// The pair `(logic:Satisfied, logic:GoalEvaluationCompleted)` is exactly what generates a
/// conclusive `gmeow:satisfiedBy` edge, so a maintenance goal that reaches it is retired —
/// and the recurrence then issues occurrences against a commitment the record says ended.
/// Every record in the fixture is individually well-formed: all four factored axes bound,
/// the criterion and time present, the goal carrying a well-formed condition. Only the
/// JOIN between the condition's kind and the evaluation's conclusiveness is wrong.
#[test]
fn a_maintenance_goal_closed_by_one_good_week_fires_on_verify() {
    let report = run_verify(MAINTENANCE_GOAL_CLOSED_BY_ONE_GOOD_WEEK);
    assert_condemns_under(
        &report,
        "maintenanceGoalClosedByWeek12",
        "MaintenanceGoalNeverConclusivelySatisfiedConstraint",
    );
}

/// The green half: the SAME scene judged honestly — held so far, not concluded.
///
/// The only edit is the conclusiveness axis. Without this the red half would be satisfied
/// by a law condemning every evaluation of a maintenance goal, which would make the
/// continuing goal unjudgeable rather than unclosable.
#[test]
fn a_maintenance_goal_held_so_far_but_undetermined_passes_on_verify() {
    let honest = MAINTENANCE_GOAL_CLOSED_BY_ONE_GOOD_WEEK.replace(
        "logic:goalEvaluationStatus logic:GoalEvaluationCompleted",
        "logic:goalEvaluationStatus logic:GoalEvaluationUndetermined",
    );
    assert_ne!(
        honest, MAINTENANCE_GOAL_CLOSED_BY_ONE_GOOD_WEEK,
        "the edit must actually change the fixture, or the green half proves nothing"
    );
    let report = run_verify(&honest);
    assert_clean(
        &report,
        "a maintenance goal that holds so far and says so — Satisfied, and UNDETERMINED — is \
         the record that keeps a continuing cluster open, and the kernel must raise nothing \
         on it",
    );
}

/// The FAILING maintenance goal still concludes, and must stay expressible.
///
/// `logic:GoalEvaluationCompleted`'s own definition says a maintenance target reaches a
/// conclusive judgment when its window closes (satisfied) or its target FAILS (violated).
/// A law that condemned every completed maintenance evaluation would make the second
/// unrepresentable — a broken standing commitment could then only be recorded as still
/// undetermined, which is the opposite of the honesty the law exists to enforce.
#[test]
fn a_maintenance_goal_conclusively_violated_passes_on_verify() {
    let failed = MAINTENANCE_GOAL_CLOSED_BY_ONE_GOOD_WEEK.replace(
        "logic:satisfactionStatus logic:Satisfied",
        "logic:satisfactionStatus logic:Violated",
    );
    assert_ne!(
        failed, MAINTENANCE_GOAL_CLOSED_BY_ONE_GOOD_WEEK,
        "the edit must actually change the fixture"
    );
    let report = run_verify(&failed);
    assert_law_silent(
        &report,
        "MaintenanceGoalNeverConclusivelySatisfiedConstraint",
        "a maintenance target that FAILED has reached a conclusive judgment, and the law \
         forbids the satisfied-and-concluded pair alone",
    );
}

/// A maintenance target under a CLOSING window is outside the law's guard.
///
/// The discrimination the whole law rests on. A windowed maintenance target is authored as
/// a `logic:DeadlineWindowGoal` whose operand carries the maintenance sub-expression, so
/// the goal's own `logic:hasGoalCondition` names the windowed kind. Once that window
/// closes the judgment may conclude — and a law whose guard read the operand instead would
/// forbid it, making a bounded maintenance commitment permanently unfinishable.
#[test]
fn a_windowed_maintenance_goal_may_conclude_on_verify() {
    let report = run_verify(&scene(
        "
ex:auditWindowGoal logic:hasGoalCondition ex:auditWindowCondition .
ex:auditWindowCondition a logic:GoalExpression ;
    logic:goalExpressionKind logic:DeadlineWindowGoal ;
    logic:operand ex:auditMaintenanceTarget ;
    logic:deadlineWindow ex:fiscalYear2026 .
ex:auditMaintenanceTarget a logic:GoalExpression ;
    logic:goalExpressionKind logic:MaintenanceGoal ;
    logic:boundSituationType ex:controlsOperating .
ex:auditWindowEvaluation a logic:GoalEvaluation ;
    logic:evaluatesGoal ex:auditWindowGoal ;
    logic:evaluatedAgainst ex:controlsOperating ;
    logic:satisfactionStatus logic:Satisfied ;
    logic:goalEvaluationStatus logic:GoalEvaluationCompleted .
",
    ));
    assert_law_silent(
        &report,
        "MaintenanceGoalNeverConclusivelySatisfiedConstraint",
        "the goal's own condition is the DEADLINE-WINDOW kind, so a closed window concludes \
         exactly as the kernel says it may; only the unbounded maintenance target — the one \
         whose interval is the life of the cluster — can never be conclusively satisfied",
    );
}

/// N8 — the model recommendation asserted as proof.
///
/// The PROOF leg of the advisory law, which no pin is involved in. Before the law's body
/// covered all three positions its own class definition names, this scene raised nothing.
#[test]
fn an_advisory_laundered_into_an_authorization_proof_fires_on_verify() {
    let report = run_verify(ADVISORY_LAUNDERED_INTO_A_PROOF);
    assert_condemns_under(
        &report,
        "modelSaysTheChargeIsInPolicy",
        "AdvisoryNeverAuthorityConstraint",
    );
}

#[test]
fn a_proof_that_is_not_an_advisory_passes_on_verify() {
    let report = run_verify(&scene(
        "
ex:modelSaysTheChargeIsInPolicy a logic:Advisory ;
    logic:advisoryAbout ex:payVendorStep .
ex:payVendorAuthProof a logic:AuthorizationProof ;
    logic:proofEstablishes ex:payVendorIntent .
",
    ));
    assert_clean(
        &report,
        "advice that stays advice, alongside a proof that is a proof, is the separation the \
         kernel models — the advisory law must not condemn a model output for existing",
    );
}

/// The advisory law's third leg: the decision position.
///
/// Separate from the proof leg because they fail independently — model output can reach the
/// decision seam of an approval without ever being typed a proof — and because a law whose
/// name says "any authority position" must be shown to hold at every position it claims.
#[test]
fn an_advisory_standing_as_an_approvals_decision_fires_on_verify() {
    let report = run_verify(&scene(
        "
ex:modelRecommendsApproval a logic:Advisory ;
    logic:advisoryAbout ex:payVendorStep .
ex:payVendorApproval a logic:ApprovalCommitment ;
    logic:approvalIntentDigest \"b3:aaaa\" ;
    logic:approvalEnactment ex:runOne ;
    logic:approvalOperator ex:onCallOperator ;
    logic:approvalPolicy ex:paymentsPolicy ;
    logic:approvalDecision ex:modelRecommendsApproval ;
    logic:approvalValidityWindow \"PT1H\" .
",
    ));
    assert_condemns_under(
        &report,
        "modelRecommendsApproval",
        "AdvisoryNeverAuthorityConstraint",
    );
}

#[test]
fn an_approval_decided_by_something_that_is_not_an_advisory_passes_on_verify() {
    let report = run_verify(&scene(
        "
ex:modelRecommendsApproval a logic:Advisory ;
    logic:advisoryAbout ex:payVendorStep .
ex:payVendorApproval a logic:ApprovalCommitment ;
    logic:approvalIntentDigest \"b3:aaaa\" ;
    logic:approvalEnactment ex:runOne ;
    logic:approvalOperator ex:onCallOperator ;
    logic:approvalPolicy ex:paymentsPolicy ;
    logic:approvalDecision ex:grantedByQuorum ;
    logic:approvalValidityWindow \"PT1H\" .
",
    ));
    assert_law_silent(
        &report,
        "AdvisoryNeverAuthorityConstraint",
        "an approval decided by an operator quorum, with the model's recommendation sitting \
         beside it as evidence, is the ordinary case",
    );
}

// ── The four laws that compiled, ran, and had never condemned anything ────────────────
//
// Each of the four below occurred ONLY in the compiled-law census and the translation
// catalogues: no counter-example named them, no conformance cell pinned them, and no
// assertion here drove them. A law in that position is prose that happens to lower into a
// rule — it is indistinguishable, from every artifact in the repository, from a law whose
// body can never be satisfied. Each pair therefore drives a SHIPPED single-defect fixture
// through the production entrypoint and then the same fixture with the one missing binding
// supplied, and each red also asserts that the sibling law governing the same record kind
// stays SILENT, so the red cannot be a neighbouring law picking up the slack.

/// A `logic:CompensationAttempt` with no `logic:compensatesEffect`.
const COMPENSATION_NAMING_NO_FORWARD_EFFECT: &str = include_str!(
    "../../../slices/grounding/logic/tests/counter-examples/compensation-names-no-forward-effect.ttl"
);

/// A `logic:ContextAssembly` with no `logic:assemblyForEnactment`.
const CONTEXT_ASSEMBLY_SERVING_NO_ENACTMENT: &str = include_str!(
    "../../../slices/grounding/logic/tests/counter-examples/context-assembly-serving-no-enactment.ttl"
);

/// A `logic:JournalEntry` carrying its prior head and no new head.
const JOURNAL_ENTRY_ESTABLISHING_NO_HEAD: &str = include_str!(
    "../../../slices/grounding/logic/tests/counter-examples/journal-entry-establishing-no-head.ttl"
);

/// A `logic:RetryDispatch` licensed by a `logic:ReconciliationResult` with no verdict.
const RETRY_LICENSED_BY_A_VERDICTLESS_PROBE: &str = include_str!(
    "../../../slices/grounding/logic/tests/counter-examples/retry-licensed-by-a-verdictless-probe.ttl"
);

/// The refund that counteracts nothing nameable.
///
/// The forward charge is receipted and attributed in the same scene, so the effect laws
/// pass and the ONLY defect is the empty binding — which is what makes the exactness
/// sibling's silence informative rather than incidental.
#[test]
fn a_compensation_naming_no_forward_effect_fires_on_verify() {
    let report = run_verify(COMPENSATION_NAMING_NO_FORWARD_EFFECT);
    assert_condemns_under(
        &report,
        "invoice901Refund",
        "CompensationNamesForwardEffectConstraint",
    );
    assert_law_silent(
        &report,
        "CompensationBindsExactForwardEffectConstraint",
        "the exactness law DEREFERENCES the binding, so its guard requires one; a \
         compensation naming nothing falls outside it and only the presence law can speak",
    );
    assert_law_silent(
        &report,
        "CompensationNotInverseConstraint",
        "the refund is not typed as the receipt it addresses, so the prohibition holds and \
         the red above cannot be the double-typing law wearing another law's name",
    );
    assert_law_silent(
        &report,
        "ReceiptRequiresAttemptConstraint",
        "the forward receipt in the scene names the attempt it reports on, so the effect \
         record beside the refund is well-formed",
    );
}

#[test]
fn a_compensation_naming_its_forward_receipt_passes_on_verify() {
    let repaired = COMPENSATION_NAMING_NO_FORWARD_EFFECT.replace(
        "ex:invoice901Refund a logic:CompensationAttempt ;",
        "ex:invoice901Refund a logic:CompensationAttempt ;\n    logic:compensatesEffect \
         ex:invoice901ChargeReceipt ;",
    );
    assert_ne!(
        repaired, COMPENSATION_NAMING_NO_FORWARD_EFFECT,
        "the edit must actually change the fixture, or the green half proves nothing"
    );
    let report = run_verify(&repaired);
    assert_clean(
        &report,
        "the SAME scene with the refund bound to the receipt of the charge it undoes is \
         exactly what the compensation layer models, and the kernel must raise nothing",
    );
}

/// The assembly that answers the audit question about nobody.
#[test]
fn an_assembly_naming_no_enactment_fires_on_verify() {
    let report = run_verify(CONTEXT_ASSEMBLY_SERVING_NO_ENACTMENT);
    assert_condemns_under(
        &report,
        "assemblyServingNobody",
        "ContextAssemblyNamesItsEnactmentConstraint",
    );
    assert_law_silent(
        &report,
        "ContextAssemblyRecordsExclusionsConstraint",
        "the withheld item carries its reason, so the exclusion law is GUARDED here and \
         passes on its merits rather than vacuously — the assembly's defect is its subject, \
         not its bookkeeping",
    );
    assert_law_silent(
        &report,
        "ContextAssemblyExclusionIsNotInclusionConstraint",
        "the surfaced item and the withheld item are different items, so the disjointness \
         law is guarded and holds",
    );
}

#[test]
fn an_assembly_naming_the_run_it_served_passes_on_verify() {
    let repaired = CONTEXT_ASSEMBLY_SERVING_NO_ENACTMENT.replace(
        "ex:assemblyServingNobody a logic:ContextAssembly ;",
        "ex:assemblyServingNobody a logic:ContextAssembly ;\n    logic:assemblyForEnactment \
         ex:adrReviewWeek13 ;",
    );
    assert_ne!(
        repaired, CONTEXT_ASSEMBLY_SERVING_NO_ENACTMENT,
        "the edit must actually change the fixture, or the green half proves nothing"
    );
    let report = run_verify(&repaired);
    assert_clean(
        &report,
        "an assembly bound to the run it surfaced material to is the record the kernel \
         models, and the kernel must raise nothing on it",
    );
}

/// The entry nothing can ever be appended after.
///
/// Its one link is CORRECT — the prior head reproduces the predecessor's new head — so
/// the chain-integrity law is guarded and passes, and the red can only be the presence
/// law. That is the whole discipline of this pair: an entry may be perfectly chained to
/// its past and still be unchainable to its future.
#[test]
fn a_journal_entry_establishing_no_head_fires_on_verify() {
    let report = run_verify(JOURNAL_ENTRY_ESTABLISHING_NO_HEAD);
    assert_condemns_under(
        &report,
        "journalEntry9",
        "JournalEntryNamesBothHeadsConstraint",
    );
    assert_law_silent(
        &report,
        "JournalChainIntegrityConstraint",
        "the entry's prior head IS its predecessor's new head, so the hash-chain invariant \
         holds where it can be evaluated and the missing end is the only defect",
    );
}

#[test]
fn a_journal_entry_naming_both_of_its_heads_passes_on_verify() {
    let repaired = JOURNAL_ENTRY_ESTABLISHING_NO_HEAD.replace(
        "    logic:journalPredecessor ex:journalEntry8 ;",
        "    logic:journalPredecessor ex:journalEntry8 ;\n    logic:journalNewHead \
         \"b3:3fb1a7c05e29d648b03c7a15f9e02d84c76b1350ae42f9d867b0c31de5a4028f\" ;",
    );
    assert_ne!(
        repaired, JOURNAL_ENTRY_ESTABLISHING_NO_HEAD,
        "the edit must actually change the fixture, or the green half proves nothing"
    );
    let report = run_verify(&repaired);
    assert_clean(
        &report,
        "an entry naming the head it was applied against AND the head it established is \
         chainable in both directions, which is the whole of what the presence law asks",
    );
}

/// The retry that proceeded on a probe with no verdict.
///
/// Every licensing law in the scene is guarded and passes on its merits: the undetermined
/// outcome names its attempt, the retry names both its attempt and its licence, and the
/// licence covers that exact attempt. Only the licence's CONTENT is absent — which is the
/// same shape of defect as the content-free saturation witness, in the other layer.
#[test]
fn a_reconciliation_result_carrying_no_verdict_fires_on_verify() {
    let report = run_verify(RETRY_LICENSED_BY_A_VERDICTLESS_PROBE);
    assert_condemns_under(
        &report,
        "invoice903ProbeResult",
        "ReconciliationResultCarriesVerdictConstraint",
    );
    assert_law_silent(
        &report,
        "RetryRequiresLicenceConstraint",
        "the retry NAMES a licence, so the presence half of the retry discipline passes — \
         which is exactly why a licence that records nothing had to become its own law",
    );
    assert_law_silent(
        &report,
        "NoBlindRetryConstraint",
        "the licence carries logic:licenceCoversAttempt for the very attempt being \
         re-sent, so the coverage relation holds and the retry is not a BORROWED licence \
         but an EMPTY one",
    );
    assert_law_silent(
        &report,
        "UnknownOutcomeNamesItsAttemptConstraint",
        "the undetermined position names the attempt it is undetermined about, so the \
         precondition of the whole probe-and-retry discipline is met",
    );
}

#[test]
fn a_reconciliation_result_carrying_its_verdict_passes_on_verify() {
    let repaired = RETRY_LICENSED_BY_A_VERDICTLESS_PROBE.replace(
        "    logic:resultOfProbe ex:invoice903Probe ;",
        "    logic:resultOfProbe ex:invoice903Probe ;\n    logic:reconciliationVerdict \
         logic:ReconciledNotCommitted ;",
    );
    assert_ne!(
        repaired, RETRY_LICENSED_BY_A_VERDICTLESS_PROBE,
        "the edit must actually change the fixture, or the green half proves nothing"
    );
    let report = run_verify(&repaired);
    assert_clean(
        &report,
        "a probe that established the charge never committed, recorded as the verdict it \
         reached, is the licence the retry was entitled to proceed under — and the kernel \
         must raise nothing on it",
    );
}

// ── The absence siblings reach their PRESENCE law, and only it ────────────────────────
//
// The adjudication these six mismatch fixtures rest on, pinned rather than asserted. Each
// of the five absence fixtures below shares a record kind with a mismatch fixture above,
// and each can only reach the presence law: the relational law's guard requires the very
// binding the absence fixture omits, so a cell pinning an absence fixture to a relational
// law pins it to a law that cannot fire on it.

#[test]
fn the_absence_fixtures_reach_their_presence_law_and_not_their_relational_twin() {
    for (fixture, subject, presence, relational) in [
        (
            include_str!(
                "../../../slices/grounding/logic/tests/counter-examples/checkpoint-no-folded-identity.ttl"
            ),
            "ckptNoIdentity",
            "CheckpointCarriesFoldedIdentityConstraint",
            "CheckpointRestoreIdentityConstraint",
        ),
        (
            include_str!(
                "../../../slices/grounding/logic/tests/counter-examples/unknown-outcome-without-attempt.ttl"
            ),
            "unknownNoAttempt",
            "UnknownOutcomeNamesItsAttemptConstraint",
            "NoBlindRetryConstraint",
        ),
        (
            include_str!(
                "../../../slices/grounding/logic/tests/counter-examples/prescription-version-not-content-addressed.ttl"
            ),
            "versionNoDigest",
            "PrescriptionVersionIsContentAddressedConstraint",
            "PrescriptionVersionImmutabilityConstraint",
        ),
        (
            include_str!(
                "../../../slices/grounding/logic/tests/counter-examples/frontier-closed-without-saturation-witness.ttl"
            ),
            "frontierNoWitness",
            "FrontierCarriesSaturationWitnessConstraint",
            "FrontierClosureRequiresSaturationConstraint",
        ),
        (
            include_str!(
                "../../../slices/grounding/logic/tests/counter-examples/capability-gap-without-blocked-step.ttl"
            ),
            "ocrGapNoStep",
            "OperationalGapNamesBlockedStepConstraint",
            "OperationalGapCarriesProposalConstraint",
        ),
    ] {
        let report = run_verify(fixture);
        assert_condemns_under(&report, subject, presence);
        assert_law_silent(
            &report,
            relational,
            &format!(
                "{relational} cannot fire on {subject}: its guard requires the binding the \
                 fixture omits, so a conformance cell pinning this fixture to it would pin a \
                 law that has nothing to say about the record"
            ),
        );
    }
}
