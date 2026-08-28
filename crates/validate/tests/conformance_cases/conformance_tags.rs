// SPDX-License-Identifier: AGPL-3.0-only

//! Conformance twin migrated from tests/test_tags.py (whole file; the Python
//! file is deleted).
//!
//! `no_bridge_among_has_tag_is_about_and_rdf_type` keeps the three axes —
//! classification (rdf:type), aboutness (gmeow:isAbout), and tagging
//! (gmeow:hasTag) — orthogonal. Because rdf:type is not an owl:ObjectProperty,
//! OWL cannot express the disjointness, so this closed-world absence guard runs
//! over the merged ontology (`GraphStore::ontology()`).

use crate::conformance_support::*;

const GMEOW: &str = "https://blackcatinformatics.ca/gmeow/";
const RDFS_SUBPROPERTY_OF: &str = "http://www.w3.org/2000/01/rdf-schema#subPropertyOf";
const OWL_EQUIVALENT_PROPERTY: &str = "http://www.w3.org/2002/07/owl#equivalentProperty";

fn gm(local: &str) -> String {
    format!("{GMEOW}{local}")
}

/// No subPropertyOf or equivalentProperty bridge exists among gmeow:hasTag,
/// gmeow:isAbout, and rdf:type (in either direction).
#[gmeow_test_batch_macros::batch_test]
fn no_bridge_among_has_tag_is_about_and_rdf_type() {
    let g = GraphStore::ontology();
    let axes = [gm("hasTag"), gm("isAbout"), RDF_TYPE.to_owned()];
    for i in 0..axes.len() {
        for j in (i + 1)..axes.len() {
            let (a, b) = (&axes[i], &axes[j]);
            assert!(
                !g.has(Some(a), Some(RDFS_SUBPROPERTY_OF), Some(b)),
                "{a} rdfs:subPropertyOf {b} is forbidden"
            );
            assert!(
                !g.has(Some(b), Some(RDFS_SUBPROPERTY_OF), Some(a)),
                "{b} rdfs:subPropertyOf {a} is forbidden"
            );
            assert!(
                !g.has(Some(a), Some(OWL_EQUIVALENT_PROPERTY), Some(b)),
                "{a} owl:equivalentProperty {b} is forbidden"
            );
            assert!(
                !g.has(Some(b), Some(OWL_EQUIVALENT_PROPERTY), Some(a)),
                "{b} owl:equivalentProperty {a} is forbidden"
            );
        }
    }
}
