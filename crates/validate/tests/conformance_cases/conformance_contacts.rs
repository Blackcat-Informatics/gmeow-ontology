// SPDX-License-Identifier: AGPL-3.0-only

//! Conformance twins migrated from tests/test_contact_fields.py (whole file; the
//! Python file is deleted).
//!
//! All three twins run over the merged ontology (`GraphStore::ontology()`):
//!   - `new_small_terms_exist`: gmeow:description / hasWebPage / subOrganizationOf
//!     are defined in slices/core/entities and organization (cross-slice).
//!   - `membership_relator_completed`: gmeow:membershipMember / membershipOrganization
//!     are defined in the organization module (cross-slice).
//!   - `no_flat_contact_terms`: a whole-graph absence guard — the flat schema.org /
//!     vCard forms must never be canonical anywhere in the ontology.

use crate::conformance_support::*;

const GMEOW: &str = "https://blackcatinformatics.ca/gmeow/";
const OWL_OBJECT_PROPERTY: &str = "http://www.w3.org/2002/07/owl#ObjectProperty";
const OWL_DATATYPE_PROPERTY: &str = "http://www.w3.org/2002/07/owl#DatatypeProperty";
const OWL_ANNOTATION_PROPERTY: &str = "http://www.w3.org/2002/07/owl#AnnotationProperty";
const OWL_TRANSITIVE_PROPERTY: &str = "http://www.w3.org/2002/07/owl#TransitiveProperty";
const RDFS_DOMAIN: &str = "http://www.w3.org/2000/01/rdf-schema#domain";
const RDFS_RANGE: &str = "http://www.w3.org/2000/01/rdf-schema#range";

fn gm(local: &str) -> String {
    format!("{GMEOW}{local}")
}

/// The audited small-win contact terms are precisely-scoped structured terms:
/// gmeow:description (the one legitimately flat note), gmeow:hasWebPage (range
/// WebPage), and gmeow:subOrganizationOf (transitive, range Organization).
#[gmeow_test_batch_macros::batch_test]
fn new_small_terms_exist() {
    let g = GraphStore::ontology();
    assert!(
        g.has(
            Some(&gm("description")),
            Some(RDF_TYPE),
            Some(OWL_DATATYPE_PROPERTY)
        ),
        "gmeow:description must be an owl:DatatypeProperty"
    );

    let web = gm("hasWebPage");
    assert!(
        g.has(Some(&web), Some(RDF_TYPE), Some(OWL_OBJECT_PROPERTY)),
        "gmeow:hasWebPage must be an owl:ObjectProperty"
    );
    assert!(
        g.has(Some(&web), Some(RDFS_RANGE), Some(&gm("WebPage"))),
        "gmeow:hasWebPage range must be gmeow:WebPage"
    );

    let sub = gm("subOrganizationOf");
    assert!(
        g.has(Some(&sub), Some(RDF_TYPE), Some(OWL_OBJECT_PROPERTY)),
        "gmeow:subOrganizationOf must be an owl:ObjectProperty"
    );
    assert!(
        g.has(Some(&sub), Some(RDF_TYPE), Some(OWL_TRANSITIVE_PROPERTY)),
        "gmeow:subOrganizationOf must be an owl:TransitiveProperty"
    );
    assert!(
        g.has(Some(&sub), Some(RDFS_RANGE), Some(&gm("Organization"))),
        "gmeow:subOrganizationOf range must be gmeow:Organization"
    );
}

/// The gmeow:Membership relator's two roles are complete: each is a functional
/// property from Membership to the expected filler (Agent / Organization).
#[gmeow_test_batch_macros::batch_test]
fn membership_relator_completed() {
    let g = GraphStore::ontology();
    for (role, rng) in [
        ("membershipMember", "Agent"),
        ("membershipOrganization", "Organization"),
    ] {
        let node = gm(role);
        assert!(
            g.has(Some(&node), Some(RDFS_DOMAIN), Some(&gm("Membership"))),
            "gmeow:{role} domain must be gmeow:Membership"
        );
        assert!(
            g.has(Some(&node), Some(RDFS_RANGE), Some(&gm(rng))),
            "gmeow:{role} range must be gmeow:{rng}"
        );
        assert!(
            g.is_functional_carrier(&node),
            "gmeow:{role} must carry a logic: functionalProperty characteristic"
        );
    }
}

/// The flat schema.org / vCard forms are projection downcasts or deferred — never
/// canonical property terms in the base ontology (greenfield rule, Principle 9).
#[gmeow_test_batch_macros::batch_test]
fn no_flat_contact_terms() {
    let g = GraphStore::ontology();
    let property_types = [
        OWL_OBJECT_PROPERTY,
        OWL_DATATYPE_PROPERTY,
        OWL_ANNOTATION_PROPERTY,
    ];
    for banned in [
        "nickname",
        "nick",
        "birthDate",
        "jobTitle",
        "url",
        "image",
        "depiction",
    ] {
        let node = gm(banned);
        for pt in property_types {
            assert!(
                !g.has(Some(&node), Some(RDF_TYPE), Some(pt)),
                "gmeow:{banned} must not be a canonical property term"
            );
        }
    }
}
