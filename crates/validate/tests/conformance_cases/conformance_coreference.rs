// SPDX-License-Identifier: AGPL-3.0-only

//! Conformance twin migrated from tests/test_coreference.py (whole file; the
//! Python file is deleted).
//!
//! `no_preferred_or_primary_coreference_terms` is a whole-graph absence guard
//! (Principle 9): no "one slot to win" authority/coreference/identity selector is
//! declared anywhere in the merged ontology. Runs over `GraphStore::ontology()`.

use crate::conformance_support::*;

const GMEOW: &str = "https://blackcatinformatics.ca/gmeow/";
const OWL_CLASS: &str = "http://www.w3.org/2002/07/owl#Class";
const OWL_OBJECT_PROPERTY: &str = "http://www.w3.org/2002/07/owl#ObjectProperty";
const OWL_DATATYPE_PROPERTY: &str = "http://www.w3.org/2002/07/owl#DatatypeProperty";

fn gm(local: &str) -> String {
    format!("{GMEOW}{local}")
}

/// No preferred/primary authority-, coreference-, or identity-selector term is
/// declared as a class or property anywhere in the merged ontology.
#[gmeow_test_batch_macros::batch_test]
fn no_preferred_or_primary_coreference_terms() {
    let g = GraphStore::ontology();
    for banned in [
        "primaryAuthority",
        "preferredAuthority",
        "primaryCoreference",
        "preferredCoreference",
        "primaryIdentity",
        "preferredIdentity",
    ] {
        let node = gm(banned);
        assert!(
            !g.has(Some(&node), Some(RDF_TYPE), Some(OWL_CLASS)),
            "gmeow:{banned} must not be an owl:Class"
        );
        assert!(
            !g.has(Some(&node), Some(RDF_TYPE), Some(OWL_OBJECT_PROPERTY)),
            "gmeow:{banned} must not be an owl:ObjectProperty"
        );
        assert!(
            !g.has(Some(&node), Some(RDF_TYPE), Some(OWL_DATATYPE_PROPERTY)),
            "gmeow:{banned} must not be an owl:DatatypeProperty"
        );
    }
}
