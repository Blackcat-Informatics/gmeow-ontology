// SPDX-License-Identifier: AGPL-3.0-only

//! Conformance twin migrated from tests/test_aggregation.py (whole file; the
//! Python file is deleted).
//!
//! `test_contains_place_exists_and_is_inverse` asserts a cross-slice invariant:
//! gmeow:containsPlace and gmeow:containedInPlace are defined in
//! slices/core/places/module.ttl, not the aggregation module, so a scopeModule
//! cell over the aggregation module would silently miss them. The twin runs the
//! membership checks over the merged ontology (`GraphStore::ontology()`).

use crate::conformance_support::*;

const GMEOW: &str = "https://blackcatinformatics.ca/gmeow/";
const OWL_OBJECT_PROPERTY: &str = "http://www.w3.org/2002/07/owl#ObjectProperty";
const OWL_INVERSE_OF: &str = "http://www.w3.org/2002/07/owl#inverseOf";

fn gm(local: &str) -> String {
    format!("{GMEOW}{local}")
}

/// gmeow:containsPlace is an object property whose inverse is gmeow:containedInPlace.
#[gmeow_test_batch_macros::batch_test]
fn contains_place_exists_and_is_inverse() {
    let g = GraphStore::ontology();
    let prop = gm("containsPlace");
    assert!(
        g.has(Some(&prop), Some(RDF_TYPE), Some(OWL_OBJECT_PROPERTY)),
        "gmeow:containsPlace must be an owl:ObjectProperty"
    );
    assert!(
        g.has(
            Some(&prop),
            Some(OWL_INVERSE_OF),
            Some(&gm("containedInPlace"))
        ),
        "gmeow:containsPlace must be owl:inverseOf gmeow:containedInPlace"
    );
}
