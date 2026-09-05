// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! SHACL acceptance over the shipped bundle's authenticated `graph/norm-claims` product.
//!
//! The explicit pre-test producer imports the exact bundle and extracts the exact shape
//! union once. This test projects and validates those immutable products; it never reads
//! or assembles the source corpus.

use std::sync::Arc;

use purrdf::{RdfDataset, flat_dataset_from_quads, flat_rdf_quads_from_dataset};

#[path = "support/authenticated_bundle.rs"]
mod authenticated_bundle;

const GMEOW: &str = "https://blackcatinformatics.ca/gmeow/";
const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
const GRAPH_NORM_CLAIMS: &str = "https://blackcatinformatics.ca/gmeow/graph/norm-claims";
const ADVICE_FAMILY: &str = "advice.";

fn norm_claims_dataset() -> Arc<RdfDataset> {
    let scoped = authenticated_bundle::dataset().project_named_graph(GRAPH_NORM_CLAIMS);
    let mut quads = flat_rdf_quads_from_dataset(&scoped);
    for quad in &mut quads {
        quad.graph_name = None;
    }
    flat_dataset_from_quads(&quads).expect("authenticated norm-claims graph must freeze")
}

fn authenticated_shapes() -> purrdf::shapes::shapes::Shapes {
    let bytes = gmeow_bundle_import::load_authenticated_corpus_artifact(
        &authenticated_bundle::repo_root(),
        "validate-conformance-shapes.ttl",
    )
    .expect("authenticated validation-shapes product; tests never produce it");
    let ttl = String::from_utf8(bytes).expect("authenticated validation-shapes product is UTF-8");
    purrdf::shapes::engine::parse_shapes(&ttl, None).expect("authenticated validation shapes parse")
}

#[test]
fn shipped_norm_claims_conforms_and_carries_the_advisory_compliance_assessment() {
    let dataset = norm_claims_dataset();
    assert!(
        dataset.quad_count() > 0,
        "the authenticated graph/norm-claims product must be non-empty"
    );

    let triples = authenticated_bundle::graph_triples(GRAPH_NORM_CLAIMS);
    let assessment_class = format!("{GMEOW}ComplianceAssessment");
    let advisory_assessment_subjects: Vec<&str> = triples
        .iter()
        .filter(|(_, predicate, object)| predicate == RDF_TYPE && object == &assessment_class)
        .map(|(subject, _, _)| subject.as_str())
        .filter(|subject| subject.contains(ADVICE_FAMILY))
        .collect();
    assert!(
        !advisory_assessment_subjects.is_empty(),
        "the authenticated norm-claims graph must carry an advice-family ComplianceAssessment"
    );

    let report = purrdf::shapes::engine::validate_dataset(&dataset, &authenticated_shapes())
        .expect("run SHACL over authenticated graph/norm-claims");
    assert!(
        report.conforms,
        "the shipped graph/norm-claims dataset must SHACL-conform; violations: {:#?}",
        report.results
    );
}
