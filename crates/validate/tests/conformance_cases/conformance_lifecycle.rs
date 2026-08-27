// SPDX-License-Identifier: AGPL-3.0-only
//! Conformance twins migrated from tests/test_lifecycle.py (whole file; the
//! Python file is deleted).
//!
//! The two entity-existence SHACL cases were migrated in a prior batch. This PR
//! ports the remaining retained tests:
//!   - `test_contested_existence_claims_coexist_and_validate` → the
//!     `contested_existence_conforms` case + `contested_existence_claims_coexist`.
//!   - `test_coverage_fixture_loads_and_validates` → the
//!     `coverage_fixture_conforms` case + `coverage_fixture_abox`.
//!   - `test_supersession_properties_are_object_properties` →
//!     `supersession_properties_are_object_properties` (GraphStore ontology).
//!   - `test_lifecycle_event_types_are_individuals_not_classes` →
//!     `lifecycle_event_types_are_individuals_not_classes` (GraphStore ontology).
//!   - `test_no_lifecycle_event_subclasses_exist` →
//!     `no_lifecycle_event_subclasses_exist` (GraphStore ontology).
//!   - `test_no_preferred_or_primary_lifecycle_term` →
//!     `no_preferred_or_primary_lifecycle_term` (GraphStore ontology).

use crate::conformance_support::*;
use gmeow_test_batch_macros::batch_cases;

const G: &str = "https://blackcatinformatics.ca/gmeow/";
const EX: &str = "https://blackcatinformatics.ca/gmeow/examples/lifecycle/";
const RDFS_DOMAIN: &str = "http://www.w3.org/2000/01/rdf-schema#domain";
const RDFS_RANGE: &str = "http://www.w3.org/2000/01/rdf-schema#range";
const OWL_CLASS: &str = "http://www.w3.org/2002/07/owl#Class";
const OWL_OBJECT_PROPERTY: &str = "http://www.w3.org/2002/07/owl#ObjectProperty";
const OWL_DATATYPE_PROPERTY: &str = "http://www.w3.org/2002/07/owl#DatatypeProperty";
const OWL_ANNOTATION_PROPERTY: &str = "http://www.w3.org/2002/07/owl#AnnotationProperty";
const OWL_INVERSE_OF: &str = "http://www.w3.org/2002/07/owl#inverseOf";

const CONTESTED_FIXTURE: &str = "tests/fixtures/coverage/lifecycle-contested.ttl";
const COVERAGE_FIXTURE: &str = "tests/fixtures/coverage/lifecycle.ttl";

fn g(local: &str) -> String {
    format!("{G}{local}")
}
fn ex(local: &str) -> String {
    format!("{EX}{local}")
}

// ── Tests migrated from tests/test_lifecycle.py ───────────────────────────────

#[batch_cases]
#[case::wellformed_entity_existence_conforms(Case::file("shapes", "entity-existence-wellformed"))]
#[case::malformed_entity_existence_is_flagged(
    Case::file("shapes", "entity-existence-malformed")
        .fails()
        .violations(&["existenceEntity", "duringInterval"])
)]
// test_contested_existence_claims_coexist_and_validate (run_shacl half).
#[case::contested_existence_conforms(Case::repo_path(CONTESTED_FIXTURE))]
// test_coverage_fixture_loads_and_validates (run_shacl half).
#[case::coverage_fixture_conforms(Case::repo_path(COVERAGE_FIXTURE))]
fn lifecycle(#[case] case: Case) {
    case.run();
}

// ── ABox twins (GraphStore) ───────────────────────────────────────────────────

/// `test_contested_existence_claims_coexist_and_validate` (ABox half): two
/// contradictory standpoint-indexed existence intervals coexist — both retained.
#[gmeow_test_batch_macros::batch_test]
fn contested_existence_claims_coexist() {
    let store = GraphStore::parse_ttl_file(&repo_root().join(CONTESTED_FIXTURE));
    let existences = store.subjects_of_type(&g("EntityExistence"));
    assert_eq!(
        existences.len(),
        2,
        "expected two coexisting EntityExistence records; got {existences:?}"
    );
}

/// `test_coverage_fixture_loads_and_validates` (ABox half): destruction event and
/// supersession pair are present.
#[gmeow_test_batch_macros::batch_test]
fn coverage_fixture_abox() {
    let store = GraphStore::parse_ttl_file(&repo_root().join(COVERAGE_FIXTURE));
    assert!(
        store.has(
            Some(&ex("medievalVillage")),
            Some(&g("hasDestructionEvent")),
            Some(&ex("villageDestroyed")),
        ),
        "medievalVillage is missing its destruction event"
    );
    assert!(
        store.has(
            Some(&ex("oldCompany")),
            Some(&g("supersededBy")),
            Some(&ex("newCompany"))
        ),
        "oldCompany is not supersededBy newCompany"
    );
    assert!(
        store.has(
            Some(&ex("newCompany")),
            Some(&g("supersedes")),
            Some(&ex("oldCompany"))
        ),
        "newCompany does not supersede oldCompany"
    );
}

// ── TBox twins (GraphStore ontology) ──────────────────────────────────────────

/// `test_supersession_properties_are_object_properties`: cross-slice gUFO
/// grounding of the supersession pair.
#[gmeow_test_batch_macros::batch_test]
fn supersession_properties_are_object_properties() {
    let store = GraphStore::ontology();
    assert!(store.has(
        Some(&g("supersededBy")),
        Some(RDF_TYPE),
        Some(OWL_OBJECT_PROPERTY)
    ));
    assert!(store.has(
        Some(&g("supersedes")),
        Some(RDF_TYPE),
        Some(OWL_OBJECT_PROPERTY)
    ));
    assert!(
        store.has(
            Some(&g("supersedes")),
            Some(OWL_INVERSE_OF),
            Some(&g("supersededBy"))
        ) || store.has(
            Some(&g("supersededBy")),
            Some(OWL_INVERSE_OF),
            Some(&g("supersedes"))
        ),
        "supersedes/supersededBy must be declared inverses"
    );
    assert!(store.has(
        Some(&g("supersededBy")),
        Some(RDFS_DOMAIN),
        Some(&g("Entity"))
    ));
    assert!(store.has(
        Some(&g("supersededBy")),
        Some(RDFS_RANGE),
        Some(&g("Entity"))
    ));
}

/// `test_lifecycle_event_types_are_individuals_not_classes`: the lifecycle event
/// kinds are `EventType` value individuals, never classes (anti-overtyping lock).
#[gmeow_test_batch_macros::batch_test]
fn lifecycle_event_types_are_individuals_not_classes() {
    let store = GraphStore::ontology();
    for local in [
        "eventTypeCreation",
        "eventTypeDestruction",
        "eventTypeSupersession",
        "eventTypeDissolution",
    ] {
        assert!(
            store.has(Some(&g(local)), Some(RDF_TYPE), Some(&g("EventType"))),
            "{local} must be an EventType value"
        );
        assert!(
            !store.has(Some(&g(local)), Some(RDF_TYPE), Some(OWL_CLASS)),
            "{local} must not be a class"
        );
    }
}

/// `test_no_lifecycle_event_subclasses_exist`: no Creation/Destruction/etc Event
/// classes are introduced.
#[gmeow_test_batch_macros::batch_test]
fn no_lifecycle_event_subclasses_exist() {
    let store = GraphStore::ontology();
    for local in [
        "CreationEvent",
        "DestructionEvent",
        "SupersessionEvent",
        "DissolutionEvent",
    ] {
        assert!(
            !store.has(Some(&g(local)), Some(RDF_TYPE), Some(OWL_CLASS)),
            "{local} must not exist as a class"
        );
    }
}

/// `test_no_preferred_or_primary_lifecycle_term` (Principle 9): no preferred/primary
/// selector term.
#[gmeow_test_batch_macros::batch_test]
fn no_preferred_or_primary_lifecycle_term() {
    let store = GraphStore::ontology();
    for banned in [
        "primaryCreationEvent",
        "preferredCreationEvent",
        "primaryDestructionEvent",
        "preferredDestructionEvent",
        "primaryExistenceInterval",
        "preferredExistenceInterval",
        "preferredRank",
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
