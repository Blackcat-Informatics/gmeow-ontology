// SPDX-License-Identifier: AGPL-3.0-only

//! Conformance twins migrated from tests/test_genealogy.py (whole file; the
//! Python file is deleted).
//!
//! Source → dest map:
//!   - `test_contested_parentage_coexists` → the `contested_parentage_conforms`
//!     SHACL case + the `contested_parentage_coexists` GraphStore test.
//!   - `test_contested_birth_date_coexists` → the `contested_birth_date_conforms`
//!     SHACL case + the `contested_birth_date_coexists` GraphStore test.
//!   - `test_withdrawn_parentage_suppressed_not_deleted` → the
//!     `withdrawn_parentage_conforms` SHACL case + the
//!     `withdrawn_parentage_suppressed_not_deleted` GraphStore test.
//!   - `test_former_event_subclasses_are_not_reintroduced` →
//!     `former_event_subclasses_are_not_reintroduced` (GraphStore ontology sweep).
//!   - `test_no_preferred_or_primary_genealogy_term` →
//!     `no_preferred_or_primary_genealogy_term` (GraphStore ontology sweep).
//!
//! Module-local asserted-TBox invariants were already migrated to
//! slices/extensions/genealogy/tests/structural.ttl before this PR.

use crate::conformance_support::*;
use gmeow_test_batch_macros::batch_cases;

const G: &str = "https://blackcatinformatics.ca/gmeow/";
const EX: &str = "https://blackcatinformatics.ca/gmeow/examples/genealogy/";
const OWL_CLASS: &str = "http://www.w3.org/2002/07/owl#Class";
const OWL_OBJECT_PROPERTY: &str = "http://www.w3.org/2002/07/owl#ObjectProperty";
const OWL_DATATYPE_PROPERTY: &str = "http://www.w3.org/2002/07/owl#DatatypeProperty";
const OWL_ANNOTATION_PROPERTY: &str = "http://www.w3.org/2002/07/owl#AnnotationProperty";

const CONTESTED_FIXTURE: &str = "tests/fixtures/coverage/genealogy-contested.ttl";

fn g(local: &str) -> String {
    format!("{G}{local}")
}
fn ex(local: &str) -> String {
    format!("{EX}{local}")
}

// ── SHACL conformance twins (the three run_shacl calls on the fixture) ─────────

#[batch_cases]
// test_contested_parentage_coexists (run_shacl half).
#[case::contested_parentage_conforms(Case::repo_path(CONTESTED_FIXTURE))]
// test_contested_birth_date_coexists (run_shacl half).
#[case::contested_birth_date_conforms(Case::repo_path(CONTESTED_FIXTURE))]
// test_withdrawn_parentage_suppressed_not_deleted (run_shacl half).
#[case::withdrawn_parentage_conforms(Case::repo_path(CONTESTED_FIXTURE))]
fn genealogy(#[case] case: Case) {
    case.run();
}

// ── ABox / TBox twins (GraphStore) ────────────────────────────────────────────

/// `test_contested_parentage_coexists` (ABox half): two contradictory
/// standpoint-indexed `hasParent` claims are BOTH retained — no ground truth.
#[gmeow_test_batch_macros::batch_test]
fn contested_parentage_coexists() {
    let store = GraphStore::parse_ttl_file(&repo_root().join(CONTESTED_FIXTURE));
    let parents = store.objects(&ex("child"), &g("hasParent"));
    assert!(
        parents.contains(&ex("civilFather")),
        "civilFather missing: {parents:?}"
    );
    assert!(
        parents.contains(&ex("parishFather")),
        "parishFather missing: {parents:?}"
    );
}

/// `test_contested_birth_date_coexists`: two standpoint-indexed `eventTime`
/// claims on the same LifeEvent coexist.
#[gmeow_test_batch_macros::batch_test]
fn contested_birth_date_coexists() {
    let store = GraphStore::parse_ttl_file(&repo_root().join(CONTESTED_FIXTURE));
    let (_, rows) = store.select(
        &[],
        &format!(
            "SELECT ?d WHERE {{ <{}> <{}> ?d }}",
            ex("childBirth"),
            g("eventTime"),
        ),
    );
    assert_eq!(
        rows.len(),
        2,
        "expected exactly two coexisting eventTime claims"
    );
}

/// `test_withdrawn_parentage_suppressed_not_deleted`: a refuted/withdrawn claim
/// is retained with `displayable false` (Principle 10 — suppression, not erasure).
#[gmeow_test_batch_macros::batch_test]
fn withdrawn_parentage_suppressed_not_deleted() {
    let store = GraphStore::parse_ttl_file(&repo_root().join(CONTESTED_FIXTURE));
    assert!(
        store.has_literal(
            &ex("withdrawnClaim"),
            &g("displayable"),
            "false",
            "http://www.w3.org/2001/XMLSchema#boolean",
        ),
        "withdrawnClaim is not retained with displayable=false"
    );
}

/// `test_former_event_subclasses_are_not_reintroduced`: the LifeEvent subclasses
/// became `eventType` value individuals; genealogy must not re-mint them as classes.
#[gmeow_test_batch_macros::batch_test]
fn former_event_subclasses_are_not_reintroduced() {
    let store = GraphStore::ontology();
    for local in ["Birth", "Death", "Marriage", "Adoption", "Christening"] {
        assert!(
            !store.has(Some(&g(local)), Some(RDF_TYPE), Some(OWL_CLASS)),
            "{local} must not be an owl:Class"
        );
    }
}

/// `test_no_preferred_or_primary_genealogy_term` (Principle 9): genealogy mints no
/// preferred/primary selector term for a contested parent, kinship, or event.
#[gmeow_test_batch_macros::batch_test]
fn no_preferred_or_primary_genealogy_term() {
    let store = GraphStore::ontology();
    for banned in [
        "primaryParent",
        "preferredParent",
        "primaryKinship",
        "preferredKinship",
        "primaryBirth",
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
