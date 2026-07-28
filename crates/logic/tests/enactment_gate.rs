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

#[test]
fn a_frontier_whose_witness_records_a_budget_cut_fires_on_verify() {
    let report = run_verify(&scene(
        "
ex:frontier1 a logic:ActionableFrontier ;
    logic:frontierSaturationWitness ex:saturation1 .
ex:saturation1 logic:resultEvaluation logic:BudgetExhausted .
",
    ));
    assert_condemns_under(
        &report,
        "frontier1",
        "FrontierClosureRequiresSaturationConstraint",
    );
}

#[test]
fn a_frontier_whose_witness_ran_to_completion_passes_on_verify() {
    let report = run_verify(&scene(
        "
ex:frontier1 a logic:ActionableFrontier ;
    logic:frontierSaturationWitness ex:saturation1 .
ex:saturation1 logic:resultEvaluation logic:EvaluationCompleted .
",
    ));
    assert_law_silent(
        &report,
        "FrontierClosureRequiresSaturationConstraint",
        "a witness whose evaluation completed certifies the fixed point the frontier claims",
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
/// The gate's marker is one shared `logic:EnactmentIntegrityViolation` type across forty
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
