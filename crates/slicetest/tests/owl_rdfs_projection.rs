// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Regression for the consume-time OWL/RDFS **view** projection boundary
//! (`gmeow_slicetest::native_query::with_owl_rdfs_projection`).
//!
//! After the `owl:`→`logic:` authoring flip a slice writes `rdfs:range logic:Thing`
//! (an already-view class-position predicate carrying the canonical top-type marker),
//! yet the generated SHACL shapes are written against `rdfs:range owl:Thing`. The
//! projection must lower the marker OBJECT under a class-position predicate — but it
//! must NOT touch a `logic:GroundingCorrespondence`'s `logic:sourceEndpoint logic:Thing`,
//! where `logic:Thing` is correspondence DATA, not a class filler: lowering it would mint
//! a second `sourceEndpoint` value and break the single-endpoint (maxCount 1) invariant the
//! reason-verify correspondence exclusion relies on.

use gmeow_slicetest::native_query::{dataset_from_turtle, with_owl_rdfs_projection};
use purrdf::RdfTerm;

const RDFS_RANGE: &str = "http://www.w3.org/2000/01/rdf-schema#range";
const OWL_THING: &str = "http://www.w3.org/2002/07/owl#Thing";
const OWL_ONCLASS: &str = "http://www.w3.org/2002/07/owl#onClass";
const LOGIC_SOURCE_ENDPOINT: &str = "https://blackcatinformatics.ca/logic/sourceEndpoint";
const LOGIC_THING: &str = "https://blackcatinformatics.ca/logic/Thing";
const EX_P: &str = "https://blackcatinformatics.ca/gmeow/examples/proj/p";
const EX_R: &str = "https://blackcatinformatics.ca/gmeow/examples/proj/r";

fn has_triple(ds: &purrdf::RdfDataset, subject: &str, predicate: &str, object: &str) -> bool {
    ds.owned_quads().any(|q| {
        q.predicate == predicate
            && matches!(&q.subject, RdfTerm::Iri(s) if s.as_str() == subject)
            && matches!(&q.object, RdfTerm::Iri(o) if o.as_str() == object)
    })
}

#[test]
fn class_position_marker_projects_but_endpoint_data_does_not() {
    let ttl = concat!(
        "@prefix ex:    <https://blackcatinformatics.ca/gmeow/examples/proj/> .\n",
        "@prefix logic: <https://blackcatinformatics.ca/logic/> .\n",
        "@prefix rdfs:  <http://www.w3.org/2000/01/rdf-schema#> .\n",
        "ex:p rdfs:range logic:Thing .\n",
        "ex:r logic:onClass logic:Thing .\n",
        "ex:corr logic:sourceEndpoint logic:Thing .\n",
    );
    let ds = dataset_from_turtle(ttl).expect("parse fixture turtle");
    let projected = with_owl_rdfs_projection(&ds);

    // (1) `rdfs:range logic:Thing` — an already-view class-position predicate with the
    //     canonical marker object — GAINS `rdfs:range owl:Thing`, and keeps the canonical edge.
    assert!(
        has_triple(&projected, EX_P, RDFS_RANGE, OWL_THING),
        "rdfs:range logic:Thing must project to rdfs:range owl:Thing"
    );
    assert!(
        has_triple(&projected, EX_P, RDFS_RANGE, LOGIC_THING),
        "the canonical rdfs:range logic:Thing edge must be preserved (the projection only ADDS)"
    );

    // (2) `logic:onClass logic:Thing` — canonical class-position predicate + canonical object —
    //     GAINS `owl:onClass owl:Thing` (both predicate and object lower).
    assert!(
        has_triple(&projected, EX_R, OWL_ONCLASS, OWL_THING),
        "logic:onClass logic:Thing must project to owl:onClass owl:Thing"
    );

    // (3) `logic:sourceEndpoint logic:Thing` names logic:Thing as correspondence DATA under a
    //     NON-class predicate, so it stays a SINGLE unprojected endpoint — never a second value.
    let endpoints: Vec<String> = projected
        .owned_quads()
        .filter(|q| q.predicate == LOGIC_SOURCE_ENDPOINT)
        .filter_map(|q| match &q.object {
            RdfTerm::Iri(o) => Some(o.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(
        endpoints,
        vec![LOGIC_THING.to_owned()],
        "logic:sourceEndpoint logic:Thing must remain a single unprojected endpoint (no owl:Thing added)"
    );
}
