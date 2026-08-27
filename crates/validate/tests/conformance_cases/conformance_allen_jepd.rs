// SPDX-License-Identifier: AGPL-3.0-only

//! Conformance twin migrated from tests/test_allen_jepd.py
//!
//! Migrated tests:
//! - `test_no_owl_all_disjoint_properties_over_interval_relations` → `no_owl_all_disjoint_properties_over_interval_relations`
//!
//! A whole-graph sweep over every `owl:AllDisjointProperties` axiom to ensure no
//! interval-level Allen relation is grouped into an OWL disjoint-properties axiom
//! (OWL 2 DL forbids `DisjointObjectProperties` over non-simple/transitive
//! properties; JEPD enforcement lives in SHACL / the solver instead). The axiom's
//! members are an `owl:members` `rdf:List` whose head is a blank node, so the walk
//! goes: named/blank `owl:AllDisjointProperties` subject → `owl:members` head
//! (blank) → `rdf:List` members, exactly as the Python `g.items(list_head)` walk.

use crate::conformance_support::*;
use purrdf::slice::rdf_query::Object;

const GMEOW: &str = "https://blackcatinformatics.ca/gmeow/";
const OWL_ALL_DISJOINT_PROPERTIES: &str = "http://www.w3.org/2002/07/owl#AllDisjointProperties";
const OWL_MEMBERS: &str = "http://www.w3.org/2002/07/owl#members";

const INTERVAL_ALLEN: &[&str] = &[
    "intervalBefore",
    "intervalAfter",
    "intervalMeets",
    "intervalMetBy",
    "intervalOverlaps",
    "intervalOverlappedBy",
    "intervalStarts",
    "intervalStartedBy",
    "intervalDuring",
    "intervalContains",
    "intervalFinishes",
    "intervalFinishedBy",
    "intervalCoincidesWith",
];

#[gmeow_test_batch_macros::batch_test]
fn no_owl_all_disjoint_properties_over_interval_relations() {
    let g = GraphStore::ontology();

    let interval_members: Vec<String> = INTERVAL_ALLEN
        .iter()
        .map(|rel| format!("{GMEOW}{rel}"))
        .collect();

    for subject in g.subjects_of_type_h(OWL_ALL_DISJOINT_PROPERTIES) {
        for list_obj in g.objects_h(&subject, OWL_MEMBERS) {
            let head = GraphStore::object_as_subject(&list_obj)
                .expect("owl:members head is a named or blank list node");
            let members: Vec<String> = g
                .rdf_list_h(&head)
                .iter()
                .filter_map(|o| match o {
                    Object::Named(iri) => Some(iri.clone()),
                    _ => None,
                })
                .collect();
            let overlap: Vec<&String> = interval_members
                .iter()
                .filter(|m| members.contains(m))
                .collect();
            assert!(
                overlap.is_empty(),
                "owl:AllDisjointProperties must not cover interval relations: {overlap:?}"
            );
        }
    }
}
