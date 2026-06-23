// SPDX-FileCopyrightText: 2026 Blackcat Informatics Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

#![cfg(feature = "gts")]

//! Integration tests for `gmeow-rdf::gts_write`.

// Rich colored line-diffs on assert_eq! failure (#871); shadows the std macro
// for this file. Identical behaviour on pass; insta snapshots are unaffected.
use gmeow_rdf_core::{
    gts_write::{to_gts, to_writer},
    RdfAnnotation, RdfLiteral, RdfQuad, RdfReifier, RdfTerm, RdfTriple, VecRdfStore,
};
use pretty_assertions::assert_eq;

fn roundtrip_graph(store: &VecRdfStore) -> gmeow_gts::model::Graph {
    let bytes = to_gts(store, "gmeow-rdf-test").expect("to_gts should succeed");
    let graph = gmeow_gts::reader::read(&bytes, false, None);
    assert!(graph.diagnostics.is_empty(), "{:?}", graph.diagnostics);
    graph
}

fn roundtrip_nquads(store: &VecRdfStore) -> String {
    let graph = roundtrip_graph(store);
    gmeow_gts::nquads::to_nquads(&graph)
}

#[test]
fn vec_store_roundtrips_through_gts_to_nquads() {
    let store = VecRdfStore::with_quads(vec![
        RdfQuad::new(
            RdfTerm::iri("https://example.org/s"),
            "https://example.org/p",
            RdfTerm::iri("https://example.org/o"),
        ),
        RdfQuad::new(
            RdfTerm::blank_node("b1"),
            "https://example.org/p2",
            RdfTerm::literal(RdfLiteral::language_tagged("hello", "en")),
        )
        .in_graph(RdfTerm::iri("https://example.org/g")),
    ]);

    let nquads = roundtrip_nquads(&store);
    assert!(nquads.contains("<https://example.org/s>"));
    assert!(nquads.contains("\"hello\"@en"));
    assert!(nquads.contains("<https://example.org/g>"));
}

#[test]
fn to_gts_is_deterministic() {
    let store = VecRdfStore::with_quads(vec![
        RdfQuad::new(
            RdfTerm::iri("https://example.org/s"),
            "https://example.org/p",
            RdfTerm::iri("https://example.org/o"),
        ),
        RdfQuad::new(
            RdfTerm::blank_node("b1"),
            "https://example.org/p2",
            RdfTerm::literal(RdfLiteral::typed(
                "42",
                "http://www.w3.org/2001/XMLSchema#integer",
            )),
        ),
    ]);

    let first = to_gts(&store, "gmeow-rdf-test").expect("first write");
    let second = to_gts(&store, "gmeow-rdf-test").expect("second write");
    assert_eq!(first, second);
}

#[test]
fn reifiers_and_annotations_roundtrip() {
    let statement = RdfTriple::new(
        RdfTerm::iri("https://example.org/s"),
        "https://example.org/p",
        RdfTerm::iri("https://example.org/o"),
    );
    let reifier = RdfTerm::blank_node("r1");

    let store = VecRdfStore {
        quads: vec![RdfQuad::new(
            reifier.clone(),
            "http://www.w3.org/1999/02/22-rdf-syntax-ns#reifies",
            RdfTerm::triple(statement.clone()),
        )],
        reifiers: vec![RdfReifier::new(reifier.clone(), statement)],
        annotations: vec![RdfAnnotation::new(
            reifier,
            "https://example.org/confidence",
            RdfTerm::literal(RdfLiteral::typed(
                "0.9",
                "http://www.w3.org/2001/XMLSchema#decimal",
            )),
        )],
        ..VecRdfStore::default()
    };

    let nquads = roundtrip_nquads(&store);
    assert!(nquads.contains("http://www.w3.org/1999/02/22-rdf-syntax-ns#reifies"));
    assert!(nquads.contains("https://example.org/confidence"));
    assert!(nquads.contains("0.9"));
}

#[test]
fn to_writer_returns_usable_writer() {
    let store = VecRdfStore::with_quads(vec![RdfQuad::new(
        RdfTerm::iri("https://example.org/s"),
        "https://example.org/p",
        RdfTerm::iri("https://example.org/o"),
    )]);

    let writer = to_writer(&store, "gmeow-rdf-test").expect("to_writer should succeed");
    let bytes = writer.to_bytes();
    let graph = gmeow_gts::reader::read(&bytes, false, None);
    assert!(graph.diagnostics.is_empty());
    assert_eq!(graph.quads.len(), 1);
}
