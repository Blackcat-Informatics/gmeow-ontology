// SPDX-License-Identifier: AGPL-3.0-only

//! Conformance twin migrated from tests/test_versions.py (whole file; the Python
//! file is deleted).
//!
//! `version_label_domain_is_entity`: gmeow:versionLabel is defined in
//! slices/extensions/languages/module.ttl (cross-slice), so a scopeModule cell
//! over the versions module would silently miss it. Runs over the merged
//! ontology (`GraphStore::ontology()`).

use crate::conformance_support::*;

const GMEOW: &str = "https://blackcatinformatics.ca/gmeow/";
const OWL_DATATYPE_PROPERTY: &str = "http://www.w3.org/2002/07/owl#DatatypeProperty";
const RDFS_DOMAIN: &str = "http://www.w3.org/2000/01/rdf-schema#domain";

fn gm(local: &str) -> String {
    format!("{GMEOW}{local}")
}

/// gmeow:versionLabel is a datatype property whose domain is broadened to
/// gmeow:Entity (any entity may carry a version label).
#[gmeow_test_batch_macros::batch_test]
fn version_label_domain_is_entity() {
    let g = GraphStore::ontology();
    let node = gm("versionLabel");
    assert!(
        g.has(Some(&node), Some(RDF_TYPE), Some(OWL_DATATYPE_PROPERTY)),
        "gmeow:versionLabel must be an owl:DatatypeProperty"
    );
    assert!(
        g.has(Some(&node), Some(RDFS_DOMAIN), Some(&gm("Entity"))),
        "gmeow:versionLabel domain must be gmeow:Entity"
    );
}
