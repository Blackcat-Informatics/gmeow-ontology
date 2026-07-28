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
/// Trips `logic:NoBlindRetryConstraint`.
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
/// Trips `logic:FrontierClosureRequiresSaturationConstraint`.
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
        "a lease carrying a fencing identity satisfies logic:LeaseExclusivityConstraint, \
         however its type was reached",
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
