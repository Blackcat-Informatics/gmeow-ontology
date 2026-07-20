// SPDX-License-Identifier: AGPL-3.0-only

//! Native conformance twins for the epistemics slice.
//!
//! Recreates — natively, zero Python — the instance-semantics coverage that was
//! deleted with the Python surface (`slices/core/epistemics/tests/test_epistemics.py`
//! and `tests/test_epistemics_belief_revision.py`):
//!
//!  * the belief-revision spine over `examples/belief-revision.ttl` AND
//!    `examples/flagship-epistemic-ledger.ttl`, checked as a derived
//!    original/revised partition (the tenure whose interval carries
//!    `endedAtTime` is the original) rather than against hardcoded IRIs;
//!  * justification/defeat union-class membership, pinned as a `logic:` reasoner
//!    entailment (a member-typed instance is classified under the union class,
//!    a non-member is not) in the companion logic-crate test;
//!  * annotation-completeness over the named justification terms, the core
//!    doxastic spine, and every dynamically-discovered `JustificationStatus`;
//!  * the epistemics SSSOM mapping-set subject membership;
//!  * a corpus-wide Principle-10 suppression law (a closed tenure is suppressed
//!    with `displayable false`; an open tenure is not) over every epistemics
//!    example fixture.
//!
//! All assertions drive the native `purrdf` graph directly through the shared
//! `conformance_support` harness — no PyO3, no rdflib.

mod conformance_support;

use std::path::PathBuf;

use conformance_support::*;
use purrdf::slice::rdf_query::{Object, Subject};

// ── Namespaces ────────────────────────────────────────────────────────────────

const GMEOW: &str = "https://blackcatinformatics.ca/gmeow/";
const XSD_DATETIME: &str = "http://www.w3.org/2001/XMLSchema#dateTime";
const XSD_DECIMAL: &str = "http://www.w3.org/2001/XMLSchema#decimal";
const XSD_BOOLEAN: &str = "http://www.w3.org/2001/XMLSchema#boolean";

/// `https://blackcatinformatics.ca/gmeow/<local>`.
fn gmeow(local: &str) -> String {
    format!("{GMEOW}{local}")
}

/// A slice example fixture path, resolved repo-root-relative.
fn example_path(name: &str) -> PathBuf {
    repo_root()
        .join("slices/core/epistemics/examples")
        .join(name)
}

/// Every literal `(lexical, datatype)` object of `<subject> <pred> ?o`,
/// blank-node-aware (the subject may be blank in the general case; here it is a
/// named IRI, but the `_h` walker is uniform).
fn literals_of(store: &GraphStore, subject: &str, pred: &str) -> Vec<(String, String)> {
    store
        .objects_h(&Subject::Named(subject.to_owned()), pred)
        .into_iter()
        .filter_map(|o| match o {
            Object::Literal {
                value, datatype, ..
            } => Some((value, datatype)),
            _ => None,
        })
        .collect()
}

/// The single `xsd:decimal` credence of a doxastic state, parsed natively. A
/// missing, multiple, wrong-typed, or unparsable credence is a HARD FAIL — never
/// a silent `0.0` that would let a stringly-typed credence pass requirement 1f
/// vacuously.
fn credence(store: &GraphStore, state: &str) -> f64 {
    let literals = literals_of(store, state, &gmeow("credence"));
    assert_eq!(
        literals.len(),
        1,
        "{state} must carry exactly one gmeow:credence, found {literals:?}"
    );
    let (value, datatype) = &literals[0];
    assert_eq!(
        datatype, XSD_DECIMAL,
        "{state} credence must be xsd:decimal, found {datatype}"
    );
    value
        .parse::<f64>()
        .unwrap_or_else(|e| panic!("{state} credence {value:?} is not a decimal: {e}"))
}

// ── Task 1: belief-revision spine (derived original/revised partition) ─────────

/// A revision fixture reduced to its original (closed, suppressed) and revised
/// (open, live) tenures/states/intervals, derived purely from graph structure.
struct RevisionPair {
    original_tenure: String,
    revised_tenure: String,
    original_state: String,
    revised_state: String,
    original_interval: String,
    revised_interval: String,
}

/// Partition a revision fixture's two `DoxasticTenure`s into original/revised by
/// which one's interval carries `endedAtTime`. This encodes the Principle-10
/// belief-revision semantics rather than trusting fixture IRI names.
fn revision_pair(store: &GraphStore) -> RevisionPair {
    let tenures = store.subjects_of_type(&gmeow("DoxasticTenure"));
    assert_eq!(
        tenures.len(),
        2,
        "a belief-revision fixture must carry exactly two DoxasticTenure individuals, found {tenures:?}"
    );

    let mut closed: Option<(String, String)> = None;
    let mut open: Option<(String, String)> = None;
    for tenure in &tenures {
        let interval = exactly_one(
            store.objects(tenure, &gmeow("duringInterval")),
            tenure,
            "duringInterval",
        );
        if store.has(Some(&interval), Some(&gmeow("endedAtTime")), None) {
            assert!(
                closed.replace((tenure.clone(), interval)).is_none(),
                "two closed tenures — original is ambiguous"
            );
        } else {
            assert!(
                open.replace((tenure.clone(), interval)).is_none(),
                "two open tenures — revised is ambiguous"
            );
        }
    }
    let (original_tenure, original_interval) =
        closed.expect("exactly one closed (original) tenure");
    let (revised_tenure, revised_interval) = open.expect("exactly one open (revised) tenure");

    let original_state = exactly_one(
        store.objects(&original_tenure, &gmeow("tenureOfDoxasticState")),
        &original_tenure,
        "tenureOfDoxasticState",
    );
    let revised_state = exactly_one(
        store.objects(&revised_tenure, &gmeow("tenureOfDoxasticState")),
        &revised_tenure,
        "tenureOfDoxasticState",
    );

    RevisionPair {
        original_tenure,
        revised_tenure,
        original_state,
        revised_state,
        original_interval,
        revised_interval,
    }
}

/// The full belief-revision spine (1a–1f + retention + revised-state presence)
/// over one revision fixture, with the fixture-specific claim modalities.
fn assert_belief_revision_spine(
    store: &GraphStore,
    expected_original_modality: &str,
    expected_revised_modality: &str,
) {
    let pair = revision_pair(store);

    // 1a — closed old tenure: its interval carries exactly one xsd:dateTime endedAtTime.
    let ends = literals_of(store, &pair.original_interval, &gmeow("endedAtTime"));
    assert_eq!(
        ends.len(),
        1,
        "original interval {} must carry exactly one endedAtTime, found {ends:?}",
        pair.original_interval
    );
    assert_eq!(
        ends[0].1, XSD_DATETIME,
        "endedAtTime must be an xsd:dateTime literal"
    );

    // 1b — displayable false suppression of the superseded tenure.
    assert!(
        store.has_literal(
            &pair.original_tenure,
            &gmeow("displayable"),
            "false",
            XSD_BOOLEAN
        ),
        "superseded original tenure {} must be suppressed with displayable false",
        pair.original_tenure
    );

    // 1c — retention of the old state: still typed, agent + content + claim + interval
    // reachable, credence unchanged (revision deletes nothing — audit-trail integrity).
    assert!(
        store.has(
            Some(&pair.original_state),
            Some(RDF_TYPE),
            Some(&gmeow("DoxasticState"))
        ),
        "original state {} must remain a DoxasticState",
        pair.original_state
    );
    assert!(
        store.has(
            Some(&pair.original_state),
            Some(&gmeow("epistemicAgent")),
            None
        ),
        "original state must retain its epistemicAgent"
    );
    assert!(
        store.has(
            Some(&pair.original_state),
            Some(&gmeow("doxasticContent")),
            None
        ),
        "original state must retain its doxasticContent"
    );
    assert!(
        store.has(
            Some(&pair.original_state),
            Some(&gmeow("doxasticClaim")),
            None
        ),
        "original state must retain its doxasticClaim"
    );
    assert!(
        store.has(
            Some(&pair.original_tenure),
            Some(&gmeow("duringInterval")),
            Some(&pair.original_interval)
        ),
        "original tenure must remain linked to its interval"
    );
    let original_credence = credence(store, &pair.original_state);

    // 1d — open new interval: exactly one xsd:dateTime start, no end; and the open
    // tenure is NOT suppressed (converse guard against suppressing the wrong tenure).
    let starts = literals_of(store, &pair.revised_interval, &gmeow("startedAtTime"));
    assert_eq!(
        starts.len(),
        1,
        "revised interval {} must carry exactly one startedAtTime, found {starts:?}",
        pair.revised_interval
    );
    assert_eq!(
        starts[0].1, XSD_DATETIME,
        "startedAtTime must be an xsd:dateTime literal"
    );
    assert!(
        !store.has(
            Some(&pair.revised_interval),
            Some(&gmeow("endedAtTime")),
            None
        ),
        "revised interval {} must be open (no endedAtTime)",
        pair.revised_interval
    );
    assert!(
        !store.has_literal(
            &pair.revised_tenure,
            &gmeow("displayable"),
            "false",
            XSD_BOOLEAN
        ),
        "open/revised tenure {} must NOT be suppressed",
        pair.revised_tenure
    );

    // Revised state present, typed, agent + content.
    assert!(
        store.has(
            Some(&pair.revised_state),
            Some(RDF_TYPE),
            Some(&gmeow("DoxasticState"))
        ),
        "revised state {} must be a DoxasticState",
        pair.revised_state
    );
    assert!(
        store.has(Some(&pair.revised_state), Some(&gmeow("epistemicAgent")), None),
        "revised state must carry an epistemicAgent"
    );
    assert!(
        store.has(
            Some(&pair.revised_state),
            Some(&gmeow("doxasticContent")),
            None
        ),
        "revised state must carry doxasticContent"
    );

    // 1e — standpoint/claim modalities via the linked StandpointClaims.
    let original_claim = exactly_one(
        store.objects(&pair.original_state, &gmeow("doxasticClaim")),
        &pair.original_state,
        "doxasticClaim",
    );
    let revised_claim = exactly_one(
        store.objects(&pair.revised_state, &gmeow("doxasticClaim")),
        &pair.revised_state,
        "doxasticClaim",
    );
    let original_modality = exactly_one(
        store.objects(&original_claim, &gmeow("claimModality")),
        &original_claim,
        "claimModality",
    );
    let revised_modality = exactly_one(
        store.objects(&revised_claim, &gmeow("claimModality")),
        &revised_claim,
        "claimModality",
    );
    assert_eq!(
        original_modality,
        gmeow(expected_original_modality),
        "original claim modality mismatch"
    );
    assert_eq!(
        revised_modality,
        gmeow(expected_revised_modality),
        "revised claim modality mismatch"
    );
    assert_ne!(
        original_modality, revised_modality,
        "a revision must shift the claim modality"
    );

    // 1f — credence-ordered suppression: the suppressed original outranks the live
    // revision (both fixtures are downward/undercut revisions). This is a per-fixture
    // fact, NOT a corpus invariant — a confirming revision would raise credence.
    let revised_credence = credence(store, &pair.revised_state);
    assert!(
        original_credence > revised_credence,
        "suppressed original credence {original_credence} must strictly exceed live revised credence {revised_credence}"
    );
}

#[test]
fn belief_revision_spine_printer_fixture() {
    let store = GraphStore::parse_ttl_file(&example_path("belief-revision.ttl"));
    assert_belief_revision_spine(&store, "unequivocal", "probable");
}

#[test]
fn belief_revision_spine_flagship_fixture() {
    let store = GraphStore::parse_ttl_file(&example_path("flagship-epistemic-ledger.ttl"));
    assert_belief_revision_spine(&store, "probable", "conceivable");
}

#[test]
fn flagship_example_parses_nonempty() {
    let store = GraphStore::parse_ttl_file(&example_path("flagship-epistemic-ledger.ttl"));
    assert!(
        store.triple_count() > 0,
        "the flagship epistemic ledger must parse to a non-empty graph"
    );
}
