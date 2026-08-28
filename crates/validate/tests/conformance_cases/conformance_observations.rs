// SPDX-License-Identifier: AGPL-3.0-only

//! Conformance twin migrated from tests/test_observations.py
//!
//! The KinRelationship sub-property bridges expose kinship roles as observation
//! roles. These bridges are asserted in the GENEALOGY module (not observations), so
//! they are checked over the merged graph (`GraphStore::ontology()`) rather than a
//! module-scoped observations cell.

use crate::conformance_support::*;

const GMEOW: &str = "https://blackcatinformatics.ca/gmeow/";
const RDFS_SUB_PROPERTY_OF: &str = "http://www.w3.org/2000/01/rdf-schema#subPropertyOf";

fn gm(local: &str) -> String {
    format!("{GMEOW}{local}")
}

/// Twin of `test_kin_relationship_bridges_fire`: relationshipParent /
/// relationshipChild / hasPartner are each rdfs:subPropertyOf observedFeature.
#[gmeow_test_batch_macros::batch_test]
fn kin_relationship_bridges_fire() {
    let g = GraphStore::ontology();
    let observed_feature = gm("observedFeature");
    for kin in ["relationshipParent", "relationshipChild", "hasPartner"] {
        assert!(
            g.has(
                Some(&gm(kin)),
                Some(RDFS_SUB_PROPERTY_OF),
                Some(&observed_feature)
            ),
            "gmeow:{kin} must be a subproperty of gmeow:observedFeature"
        );
    }
}
