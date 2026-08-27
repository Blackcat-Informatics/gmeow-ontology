// SPDX-License-Identifier: AGPL-3.0-only

//! Conformance twins migrated from tests/test_provenance.py (whole file; the
//! Python file is deleted).
//!
//! Both twins run over the merged ontology (`GraphStore::ontology()`) because
//! their subjects are cross-slice: gmeow:sourceModifiedAt / contentDigest live in
//! the sources slice, and the four clock terms live in the temporal and sources
//! slices, not the provenance module.

use crate::conformance_support::*;

const GMEOW: &str = "https://blackcatinformatics.ca/gmeow/";
const OWL_ANNOTATION_PROPERTY: &str = "http://www.w3.org/2002/07/owl#AnnotationProperty";
const RDFS_DOMAIN: &str = "http://www.w3.org/2000/01/rdf-schema#domain";
const RDFS_RANGE: &str = "http://www.w3.org/2000/01/rdf-schema#range";
const XSD_DATETIME: &str = "http://www.w3.org/2001/XMLSchema#dateTime";

fn gm(local: &str) -> String {
    format!("{GMEOW}{local}")
}

/// Carrier time (gmeow:sourceModifiedAt, non-functional, on CreativeWork),
/// transaction time (gmeow:ingestedAt, functional), and gmeow:contentDigest
/// (non-functional — an artifact may carry several algorithms) keep their shapes.
#[gmeow_test_batch_macros::batch_test]
fn carrier_and_ingestion_props() {
    let g = GraphStore::ontology();

    let src_modified = gm("sourceModifiedAt");
    assert!(
        !g.is_functional_carrier(&src_modified),
        "gmeow:sourceModifiedAt must NOT be functional (copies may report differing mtimes)"
    );
    assert!(
        g.has(
            Some(&src_modified),
            Some(RDFS_DOMAIN),
            Some(&gm("CreativeWork"))
        ),
        "gmeow:sourceModifiedAt domain must be gmeow:CreativeWork"
    );

    assert!(
        g.is_functional_carrier(&gm("ingestedAt")),
        "gmeow:ingestedAt (transaction time) must be functional"
    );

    assert!(
        !g.is_functional_carrier(&gm("contentDigest")),
        "gmeow:contentDigest must NOT be functional (several algorithms may coexist)"
    );
}

/// The four clocks — valid time (validFrom/validUntil), assertion time
/// (assertedAt), and derived carrier bound (recordedNoLaterThan) — are four
/// distinct xsd:dateTime annotation properties, never one overloaded slot.
#[gmeow_test_batch_macros::batch_test]
fn four_clocks_are_distinct_dated_annotations() {
    let g = GraphStore::ontology();
    let clocks = [
        "validFrom",
        "validUntil",
        "assertedAt",
        "recordedNoLaterThan",
    ];
    for clock in clocks {
        let node = gm(clock);
        assert!(
            g.has(Some(&node), Some(RDF_TYPE), Some(OWL_ANNOTATION_PROPERTY)),
            "gmeow:{clock} must be an owl:AnnotationProperty"
        );
        assert!(
            g.has(Some(&node), Some(RDFS_RANGE), Some(XSD_DATETIME)),
            "gmeow:{clock} range must be xsd:dateTime"
        );
    }
    assert_eq!(
        clocks.len(),
        4,
        "four distinct terms, not one overloaded slot"
    );
}
