// SPDX-License-Identifier: AGPL-3.0-only

//! Conformance twins migrated from tests/test_employment.py (whole file; the
//! Python file is deleted).
//!
//! Source → dest map:
//!   - `test_contested_employment_coexists` (run_shacl on the contested fixture +
//!     `memberOf` ABox check) → the `contested_employment_conforms` SHACL case
//!     plus the `contested_employment_coexists` GraphStore test.
//!   - `test_withdrawn_employment_suppressed_not_deleted` (run_shacl + `displayable
//!     false` ABox check) → the `withdrawn_employment_conforms` SHACL case plus the
//!     `withdrawn_employment_suppressed_not_deleted` GraphStore test.
//!   - `test_employment_event_types_are_values` (cross-slice EventType invariant) →
//!     the `employment_event_types_are_values` GraphStore test.
//!   - `test_no_preferred_or_primary_employment_term` (whole-graph negative sweep) →
//!     the `no_preferred_or_primary_employment_term` GraphStore test.
//!
//! Module-local asserted-TBox invariants were already migrated to
//! slices/extensions/employment/tests/structural.ttl before this PR.

use crate::conformance_support::*;
use gmeow_test_batch_macros::batch_cases;

const G: &str = "https://blackcatinformatics.ca/gmeow/";
const EX: &str = "https://blackcatinformatics.ca/gmeow/examples/employment/";
const RDFS_SUBCLASSOF: &str = "http://www.w3.org/2000/01/rdf-schema#subClassOf";
const OWL_CLASS: &str = "http://www.w3.org/2002/07/owl#Class";
const OWL_OBJECT_PROPERTY: &str = "http://www.w3.org/2002/07/owl#ObjectProperty";
const OWL_DATATYPE_PROPERTY: &str = "http://www.w3.org/2002/07/owl#DatatypeProperty";
const OWL_ANNOTATION_PROPERTY: &str = "http://www.w3.org/2002/07/owl#AnnotationProperty";

const CONTESTED_FIXTURE: &str = "tests/fixtures/coverage/employment-contested.ttl";

fn g(local: &str) -> String {
    format!("{G}{local}")
}
fn ex(local: &str) -> String {
    format!("{EX}{local}")
}

// ── SHACL conformance twins (the two run_shacl calls) ─────────────────────────

#[batch_cases]
// test_contested_employment_coexists (run_shacl half): the contested-employment
// fixture — two contradictory standpoint-indexed claims — SHACL-passes.
#[case::contested_employment_conforms(Case::repo_path(CONTESTED_FIXTURE))]
// test_withdrawn_employment_suppressed_not_deleted (run_shacl half): the same
// fixture, carrying the withdrawn (displayable false) Employment, SHACL-passes.
#[case::withdrawn_employment_conforms(Case::repo_path(CONTESTED_FIXTURE))]
fn employment(#[case] case: Case) {
    case.run();
}

// ── ABox / TBox twins (GraphStore) ────────────────────────────────────────────

/// `test_contested_employment_coexists` (ABox half): both contested orgs are
/// retained as `memberOf` targets — neither is the ground truth.
#[gmeow_test_batch_macros::batch_test]
fn contested_employment_coexists() {
    let store = GraphStore::parse_ttl_file(&repo_root().join(CONTESTED_FIXTURE));
    let orgs = store.objects(&ex("worker"), &g("memberOf"));
    assert!(
        orgs.contains(&ex("orgA")),
        "orgA not retained as memberOf: {orgs:?}"
    );
    assert!(
        orgs.contains(&ex("orgB")),
        "orgB not retained as memberOf: {orgs:?}"
    );
}

/// `test_withdrawn_employment_suppressed_not_deleted`: a closed Employment with
/// `displayable false` is retained (Principle 10), not deleted.
#[gmeow_test_batch_macros::batch_test]
fn withdrawn_employment_suppressed_not_deleted() {
    let store = GraphStore::parse_ttl_file(&repo_root().join(CONTESTED_FIXTURE));
    assert!(
        store.has_literal(
            &ex("withdrawnEmployment"),
            &g("displayable"),
            "false",
            "http://www.w3.org/2001/XMLSchema#boolean",
        ),
        "withdrawnEmployment is not retained with displayable=false"
    );
}

/// `test_employment_event_types_are_values` (Principle 9): employment events are
/// `EventType` values, never `Event` subclasses.
#[gmeow_test_batch_macros::batch_test]
fn employment_event_types_are_values() {
    let store = GraphStore::ontology();
    for evt in [
        "eventTypeHiring",
        "eventTypePromotion",
        "eventTypeTransfer",
        "eventTypeResignation",
        "eventTypeTermination",
    ] {
        assert!(
            store.has(Some(&g(evt)), Some(RDF_TYPE), Some(&g("EventType"))),
            "{evt} is not an EventType value"
        );
    }
    for banned in [
        "Hiring",
        "Promotion",
        "Transfer",
        "Resignation",
        "Termination",
    ] {
        assert!(
            !store.has(Some(&g(banned)), Some(RDFS_SUBCLASSOF), Some(&g("Event"))),
            "{banned} must not be an Event subclass"
        );
    }
}

/// `test_no_preferred_or_primary_employment_term` (Principle 9): no single winning
/// slot — employment mints no preferred/primary selector term.
#[gmeow_test_batch_macros::batch_test]
fn no_preferred_or_primary_employment_term() {
    let store = GraphStore::ontology();
    for banned in [
        "primaryEmployment",
        "preferredEmployment",
        "primaryJob",
        "preferredJob",
        "primaryRole",
        "preferredRole",
    ] {
        for pt in [
            OWL_OBJECT_PROPERTY,
            OWL_DATATYPE_PROPERTY,
            OWL_ANNOTATION_PROPERTY,
            OWL_CLASS,
        ] {
            assert!(
                !store.has(Some(&g(banned)), Some(RDF_TYPE), Some(pt)),
                "{banned} must not exist (found as {pt})"
            );
        }
    }
}
