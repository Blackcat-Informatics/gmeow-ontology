// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Production-surface acceptance harness for the enactment kernel's
//! observed-not-derived boundary.
//!
//! The kernel's hardest safety law is that the engine DESCRIBES, validates and certifies
//! external-effect records but never DERIVES one: a reasoner that could conclude an
//! attempt happened could conclude the world changed. `gmeow_logic::reason::enactment`
//! carries that law as a Rust-side guard, and `verify()` is where it is enforced.
//!
//! These tests exist because a guard is only as good as its INPUT, and that is precisely
//! what regressed once: `verify()` used to run the guard over the enactment gate's own
//! marker output, which is an unconditionally empty vector today, so the check was
//! provably vacuous while five in-module unit tests called the guard function directly and
//! stayed green. Unit tests prove the function works in isolation; only a test that drives
//! the real entry point proves the function is WIRED. So:
//!
//! * every case here calls the production [`verify`] entrypoint — the same one
//!   `make reason-verify` invokes — never `reject_banned_heads` directly;
//! * the banned head is not injected into some private row-set, it is DERIVED by the
//!   shipped EL closure from ordinary asserted RDFS, which is exactly the shape a real
//!   regression would take;
//! * a control case pins the guard's narrowness: the same scene with the offending
//!   subsumption removed must pass, so the test cannot go green by refusing everything.

use gmeow_logic::verify::verify;
use purrdf::{RdfDataset, RdfDatasetBuilder, RdfQuad, RdfTerm};
use std::sync::Arc;

/// The named graph the scene lives in. The production reasoning entrypoint reasons over
/// named-graph worlds, so the fixtures are built as quads rather than parsed from
/// default-graph Turtle.
const WORLD: &str = "http://gmeow.example/enactment-world";

const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
const SUBCLASS_OF: &str = "http://www.w3.org/2000/01/rdf-schema#subClassOf";

/// The two record kinds the engine may never derive.
const EFFECT_ATTEMPT: &str = "https://blackcatinformatics.ca/logic/EffectAttempt";
const EXTERNAL_EFFECT_RECEIPT: &str = "https://blackcatinformatics.ca/logic/ExternalEffectReceipt";

/// A class the kernel derives BY DESIGN — the control's benign superclass.
const FRONTIER_ENTRY: &str = "https://blackcatinformatics.ca/logic/FrontierEntry";

/// A locally-authored class standing in for whatever domain vocabulary a slice might
/// subsume under a kernel class.
const LOCAL_DISPATCH_RECORD: &str = "http://gmeow.example/LocalDispatchRecord";
/// The individual whose type the EL closure propagates upward.
const INDIVIDUAL: &str = "http://gmeow.example/dispatch-1";

fn scene(triples: &[(&str, &str, &str)]) -> Arc<RdfDataset> {
    let mut builder = RdfDatasetBuilder::new();
    for (s, p, o) in triples {
        builder.push_owned_quad(
            &RdfQuad::new(RdfTerm::iri(*s), *p, RdfTerm::iri(*o)).in_graph(RdfTerm::iri(WORLD)),
        );
    }
    builder.freeze().expect("valid enactment-gate scene")
}

/// Drive the production entrypoint with an EMPTY query set.
///
/// The guard runs before any verify query is evaluated, so the query list is irrelevant to
/// what is under test and an empty one keeps the failure attributable to the guard alone.
fn run_verify(dataset: &RdfDataset) -> gmeow_errors::Result<gmeow_errors::Report> {
    verify(dataset, &[])
}

/// A DERIVED `logic:EffectAttempt` hard-fails the production `verify()` path.
///
/// Nothing asserts the banned type. The scene asserts only that a local record class is a
/// subclass of `logic:EffectAttempt` and that an individual belongs to that local class;
/// the shipped EL type-propagation rule then concludes `dispatch-1 a logic:EffectAttempt`
/// as a non-EDB edge of the reasoned closure. That closure is what `verify()` guards, so
/// the run must abort with the enactment-gate diagnostic rather than returning a report.
#[test]
fn a_derived_effect_attempt_hard_fails_the_production_verify_path() {
    let dataset = scene(&[
        (LOCAL_DISPATCH_RECORD, SUBCLASS_OF, EFFECT_ATTEMPT),
        (INDIVIDUAL, RDF_TYPE, LOCAL_DISPATCH_RECORD),
    ]);
    let err = run_verify(dataset.as_ref())
        .expect_err("a derived effect attempt must abort verify, not merely be reported");
    let rendered = format!("{err:?}");
    assert!(
        rendered.contains("enactment gate"),
        "the failure must be attributed to the enactment gate: {rendered}"
    );
    assert!(
        rendered.contains("OBSERVED, never derived"),
        "the refusal must say WHY the inference is forbidden: {rendered}"
    );
    assert!(
        rendered.contains(INDIVIDUAL) && rendered.contains(EFFECT_ATTEMPT),
        "the refusal must name the offending subject and class: {rendered}"
    );
}

/// The same law for the other banned record kind: a derived external effect receipt is an
/// assertion about an outcome nobody observed.
#[test]
fn a_derived_external_effect_receipt_hard_fails_the_production_verify_path() {
    let dataset = scene(&[
        (LOCAL_DISPATCH_RECORD, SUBCLASS_OF, EXTERNAL_EFFECT_RECEIPT),
        (INDIVIDUAL, RDF_TYPE, LOCAL_DISPATCH_RECORD),
    ]);
    let err = run_verify(dataset.as_ref())
        .expect_err("a derived external effect receipt must abort verify");
    let rendered = format!("{err:?}");
    assert!(
        rendered.contains("enactment gate") && rendered.contains(EXTERNAL_EFFECT_RECEIPT),
        "the refusal must name the enactment gate and the receipt class: {rendered}"
    );
}

/// The control: strip the offending subsumption and the identical scene passes.
///
/// Without this the red cases above could be satisfied by a guard that refuses
/// everything. It also pins the guard's narrowness in the direction that matters most —
/// the class here is `logic:FrontierEntry`, which the kernel exists to DERIVE, so a guard
/// that condemned it would forbid the kernel's headline capability.
#[test]
fn a_derived_frontier_entry_passes_the_production_verify_path() {
    let dataset = scene(&[
        (LOCAL_DISPATCH_RECORD, SUBCLASS_OF, FRONTIER_ENTRY),
        (INDIVIDUAL, RDF_TYPE, LOCAL_DISPATCH_RECORD),
    ]);
    let report = run_verify(dataset.as_ref())
        .expect("a derived frontier entry is a legitimate kernel conclusion");
    assert!(
        report.ok(),
        "the control scene must be clean, not merely non-aborting: {:?}",
        report.findings
    );
}

/// An ASSERTED effect attempt is an observation and must pass untouched.
///
/// This is the boundary's other half, and the one a too-eager guard would break: the whole
/// point of the kernel is to reason ABOUT the records the dispatching organ wrote down.
/// `verify()` only hands the guard non-EDB edges, so an asserted `logic:EffectAttempt` —
/// even one the closure re-derives a supertype for — never reaches it.
#[test]
fn an_asserted_effect_attempt_is_an_observation_and_passes() {
    let dataset = scene(&[
        (INDIVIDUAL, RDF_TYPE, EFFECT_ATTEMPT),
        (EFFECT_ATTEMPT, SUBCLASS_OF, LOCAL_DISPATCH_RECORD),
    ]);
    let report = run_verify(dataset.as_ref())
        .expect("an asserted effect attempt is the observation the kernel reasons about");
    assert!(
        report.ok(),
        "an observed effect record must not be treated as a derived one: {:?}",
        report.findings
    );
}
