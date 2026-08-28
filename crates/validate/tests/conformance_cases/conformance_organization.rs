// SPDX-License-Identifier: AGPL-3.0-only
//! Conformance tests for organization slice — migrated from tests/test_organization.py
//!
//! Migrated tests:
//!   - SHACL fixture cases (membership/org mismatch warning, legal-identity violation)
//!   - Standpoint coexistence fixture content assertions
//!   - Post/seat/holder assertions
//!   - Site location assertions
//!   - Change-event predecessor/successor assertions
//!   - Wellformed legal-identifier structure assertion
//!   - Principle-9 banned-term sweep
//!   - Cross-slice EventType seed existence

use crate::conformance_support::*;
use gmeow_test_batch_macros::batch_cases;
use std::collections::BTreeSet;

const GMEOW: &str = "https://blackcatinformatics.ca/gmeow/";
const EX_ORGS: &str = "https://blackcatinformatics.ca/gmeow/examples/organizations/";
const COVERAGE: &str = "tests/fixtures/coverage/";

// ── Parameterized SHACL conformance cases ────────────────────────────────────

#[batch_cases]
#[case::membership_fills_post_org_mismatch_warns(
    Case::file("coverage", "organization-posts")
        .warnings(&["fills a Post whose organization differs"])
)]
#[case::legal_identifier_requires_scheme(
    Case::file("coverage", "organization-legal-identity")
        .fails()
        .violations(&["must declare a gmeow:identifierScheme"])
)]
fn organization(#[case] case: Case) {
    case.run();
}

// ── Standpoint coexistence: contested membership / succession (#51) ───────────

#[gmeow_test_batch_macros::batch_test]
fn contested_membership_coexists() {
    let g = GraphStore::parse_ttl_file(
        &repo_root()
            .join(COVERAGE)
            .join("organization-contested.ttl"),
    );
    assert!(ok(&validate(&ttl_file_to_nt(
        &repo_root()
            .join(COVERAGE)
            .join("organization-contested.ttl")
    ))));
    let orgs: BTreeSet<String> =
        g.objects(&format!("{EX_ORGS}member"), &format!("{GMEOW}memberOf"));
    assert!(orgs.contains(&format!("{EX_ORGS}orgA")));
    assert!(orgs.contains(&format!("{EX_ORGS}orgB")));
}

#[gmeow_test_batch_macros::batch_test]
fn contested_succession_coexists() {
    let path = repo_root()
        .join(COVERAGE)
        .join("organization-contested.ttl");
    let g = GraphStore::parse_ttl_file(&path);
    assert!(ok(&validate(&ttl_file_to_nt(&path))));
    let parents: BTreeSet<String> = g.objects(
        &format!("{EX_ORGS}subsidiary"),
        &format!("{GMEOW}subOrganizationOf"),
    );
    assert!(parents.contains(&format!("{EX_ORGS}mergedCo")));
    assert!(parents.contains(&format!("{EX_ORGS}acquirerCo")));
}

#[gmeow_test_batch_macros::batch_test]
fn withdrawn_recognition_suppressed_not_deleted() {
    let path = repo_root()
        .join(COVERAGE)
        .join("organization-contested.ttl");
    let g = GraphStore::parse_ttl_file(&path);
    assert!(ok(&validate(&ttl_file_to_nt(&path))));
    assert!(g.has_literal(
        &format!("{EX_ORGS}withdrawnRecognition"),
        &format!("{GMEOW}displayable"),
        "false",
        "http://www.w3.org/2001/XMLSchema#boolean",
    ));
}

// ── Principle 9: no preferred/primary selector terms ──────────────────────────

#[gmeow_test_batch_macros::batch_test]
fn no_preferred_or_primary_org_term() {
    let g = GraphStore::ontology();
    let banned = [
        "primaryMember",
        "preferredMember",
        "primarySuccessor",
        "preferredSuccessor",
        "primaryRecognition",
        "preferredRank",
    ];
    for name in banned {
        let iri = format!("{GMEOW}{name}");
        assert!(
            !g.has(
                Some(&iri),
                Some(RDF_TYPE),
                Some("http://www.w3.org/2002/07/owl#ObjectProperty")
            ),
            "{name} must not be declared as an OWL ObjectProperty"
        );
        assert!(
            !g.has(
                Some(&iri),
                Some(RDF_TYPE),
                Some("http://www.w3.org/2002/07/owl#DatatypeProperty")
            ),
            "{name} must not be declared as an OWL DatatypeProperty"
        );
        assert!(
            !g.has(
                Some(&iri),
                Some(RDF_TYPE),
                Some("http://www.w3.org/2002/07/owl#AnnotationProperty")
            ),
            "{name} must not be declared as an OWL AnnotationProperty"
        );
        assert!(
            !g.has(
                Some(&iri),
                Some(RDF_TYPE),
                Some("http://www.w3.org/2002/07/owl#Class")
            ),
            "{name} must not be declared as an OWL Class"
        );
    }
}

// ── Post — seat independent of holder ────────────────────────────

#[gmeow_test_batch_macros::batch_test]
fn post_seat_independent_of_holder() {
    let path = repo_root().join(COVERAGE).join("organization-posts.ttl");
    let g = GraphStore::parse_ttl_file(&path);
    assert!(ok(&validate(&ttl_file_to_nt(&path))));
    assert!(g.has(
        Some(&format!("{EX_ORGS}cfoPost")),
        Some(RDF_TYPE),
        Some(&format!("{GMEOW}Post")),
    ));
    let fillers: BTreeSet<String> =
        g.subjects(&format!("{GMEOW}fillsPost"), &format!("{EX_ORGS}cfoPost"));
    assert!(fillers.is_empty(), "CFO post must be vacant");
}

#[gmeow_test_batch_macros::batch_test]
fn post_successive_holders() {
    let path = repo_root().join(COVERAGE).join("organization-posts.ttl");
    let g = GraphStore::parse_ttl_file(&path);
    assert!(ok(&validate(&ttl_file_to_nt(&path))));
    let holders: BTreeSet<String> =
        g.subjects(&format!("{GMEOW}fillsPost"), &format!("{EX_ORGS}ceoPost"));
    assert_eq!(
        holders,
        BTreeSet::from([
            format!("{EX_ORGS}aliceMembership"),
            format!("{EX_ORGS}bobMembership"),
        ])
    );
}

// ── Site — organizational location ───────────────────────────────

#[gmeow_test_batch_macros::batch_test]
fn site_location() {
    let path = repo_root().join(COVERAGE).join("organization-sites.ttl");
    let g = GraphStore::parse_ttl_file(&path);
    assert!(ok(&validate(&ttl_file_to_nt(&path))));
    let sites: BTreeSet<String> = g.objects(&format!("{EX_ORGS}acme"), &format!("{GMEOW}hasSite"));
    assert_eq!(
        sites,
        BTreeSet::from([
            format!("{EX_ORGS}hqBuilding"),
            format!("{EX_ORGS}branchOffice"),
        ])
    );
    assert!(g.has(
        Some(&format!("{EX_ORGS}hqBuilding")),
        Some(&format!("{GMEOW}siteType")),
        Some(&format!("{GMEOW}siteTypeHeadquarters")),
    ));
    assert!(g.has(
        Some(&format!("{EX_ORGS}branchOffice")),
        Some(&format!("{GMEOW}siteType")),
        Some(&format!("{GMEOW}siteTypeBranch")),
    ));
}

// ── Multi-organization change events ─────────────────────────────

#[gmeow_test_batch_macros::batch_test]
fn change_event_entailments() {
    let path = repo_root()
        .join(COVERAGE)
        .join("organization-change-events.ttl");
    let g = GraphStore::parse_ttl_file(&path);
    assert!(ok(&validate(&ttl_file_to_nt(&path))));

    let preds: BTreeSet<String> = g.objects(
        &format!("{EX_ORGS}mergerEvent"),
        &format!("{GMEOW}predecessorOrganization"),
    );
    assert_eq!(
        preds,
        BTreeSet::from([
            format!("{EX_ORGS}acquiredCo"),
            format!("{EX_ORGS}acquirerCo"),
        ])
    );

    let succs: BTreeSet<String> = g.objects(
        &format!("{EX_ORGS}mergerEvent"),
        &format!("{GMEOW}successorOrganization"),
    );
    assert_eq!(succs, BTreeSet::from([format!("{EX_ORGS}mergedEntity")]));

    let split_preds: BTreeSet<String> = g.objects(
        &format!("{EX_ORGS}splitEvent"),
        &format!("{GMEOW}predecessorOrganization"),
    );
    assert_eq!(split_preds, BTreeSet::from([format!("{EX_ORGS}parentCo")]));

    let split_succs: BTreeSet<String> = g.objects(
        &format!("{EX_ORGS}splitEvent"),
        &format!("{GMEOW}successorOrganization"),
    );
    assert_eq!(
        split_succs,
        BTreeSet::from([format!("{EX_ORGS}spinOffA"), format!("{EX_ORGS}spinOffB")])
    );
}

// ── Legal identity ───────────────────────────────────────────────

#[gmeow_test_batch_macros::batch_test]
fn wellformed_legal_identifier_structure() {
    let path = repo_root()
        .join(COVERAGE)
        .join("organization-legal-identity.ttl");
    let g = GraphStore::parse_ttl_file(&path);
    // The malformed fixture case is covered by the SHACL violation case above;
    // here we assert the wellformed acme structure is present.
    let id_nodes: BTreeSet<String> = g.objects(
        &format!("{EX_ORGS}acme"),
        &format!("{GMEOW}legalIdentifier"),
    );
    assert_eq!(
        id_nodes.len(),
        1,
        "acme must have exactly one legalIdentifier"
    );
    let id_node = id_nodes.into_iter().next().unwrap();
    assert!(g.has_literal(
        &id_node,
        &format!("{GMEOW}identifierValue"),
        "ROR-ABCDE",
        "http://www.w3.org/2001/XMLSchema#string"
    ));
    assert!(g.has_literal(
        &id_node,
        &format!("{GMEOW}identifierScheme"),
        "ror",
        "http://www.w3.org/2001/XMLSchema#string"
    ));
}

// ── Change event type values (cross-slice seeds in events slice) ──────────────

#[gmeow_test_batch_macros::batch_test]
fn change_event_type_values_exist() {
    let g = GraphStore::ontology();
    for val in [
        "eventTypeMerger",
        "eventTypeSplit",
        "eventTypeSpinOff",
        "eventTypeAcquisition",
        "eventTypeRename",
    ] {
        let iri = format!("{GMEOW}{val}");
        assert!(
            g.has(
                Some(&iri),
                Some(RDF_TYPE),
                Some(&format!("{GMEOW}EventType"))
            ),
            "{val} must be seeded as an EventType"
        );
    }
}
