// SPDX-FileCopyrightText: 2026 Blackcat Informatics Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

#![cfg(feature = "gts")]

//! Integration tests for `gmeow-rdf::gts_write`.

use std::sync::Arc;

// Rich colored line-diffs on assert_eq! failure (#871); shadows the std macro
// for this file. Identical behaviour on pass; insta snapshots are unaffected.
use gmeow_rdf_core::{
    gts_write::{to_gts, to_writer},
    BlankScope, RdfAnnotation, RdfDataset, RdfDatasetBuilder, RdfLiteral, RdfLookaside, RdfQuad,
    RdfReifier, RdfTerm, RdfTriple, TermId,
};
use pretty_assertions::assert_eq;

/// Recursively intern an owned term into a builder (handles quoted triples).
fn intern_owned(b: &mut RdfDatasetBuilder, term: &RdfTerm) -> TermId {
    match term {
        RdfTerm::Iri(iri) => b.intern_iri(iri.clone()),
        RdfTerm::BlankNode(label) => b.intern_blank(label.clone(), BlankScope::DEFAULT),
        RdfTerm::Literal(lit) => b.intern_literal(lit.clone()),
        RdfTerm::Triple(t) => {
            let s = intern_owned(b, &t.subject);
            let p = b.intern_iri(t.predicate.clone());
            let o = intern_owned(b, &t.object);
            b.intern_triple(s, p, o)
        }
    }
}

/// Freeze owned rows (quads + RDF 1.2 statement layer) into the frozen IR the GTS
/// writer now consumes (#886 part 1) — replaces the retired `VecRdfStore` fixture.
fn freeze_rows(
    quads: &[RdfQuad],
    reifiers: &[RdfReifier],
    annotations: &[RdfAnnotation],
) -> Arc<RdfDataset> {
    let mut b = RdfDatasetBuilder::new();
    for q in quads {
        let s = intern_owned(&mut b, &q.subject);
        let p = b.intern_iri(q.predicate.clone());
        let o = intern_owned(&mut b, &q.object);
        let g = q.graph_name.as_ref().map(|g| intern_owned(&mut b, g));
        b.push_quad(s, p, o, g);
    }
    for r in reifiers {
        let rid = intern_owned(&mut b, &r.reifier);
        let s = intern_owned(&mut b, &r.statement.subject);
        let p = b.intern_iri(r.statement.predicate.clone());
        let o = intern_owned(&mut b, &r.statement.object);
        let triple = b.intern_triple(s, p, o);
        b.push_reifier(rid, triple);
    }
    for a in annotations {
        let rid = intern_owned(&mut b, &a.reifier);
        let p = b.intern_iri(a.predicate.clone());
        let o = intern_owned(&mut b, &a.object);
        b.push_annotation(rid, p, o);
    }
    b.freeze().expect("rows must freeze into a valid dataset")
}

fn roundtrip_graph(dataset: &RdfDataset) -> gmeow_gts::model::Graph {
    let bytes = to_gts(dataset, &RdfLookaside::default(), "gmeow-rdf-test").expect("to_gts");
    let graph = gmeow_gts::reader::read(&bytes, false, None);
    assert!(graph.diagnostics.is_empty(), "{:?}", graph.diagnostics);
    graph
}

fn roundtrip_nquads(dataset: &RdfDataset) -> String {
    let graph = roundtrip_graph(dataset);
    gmeow_gts::nquads::to_nquads(&graph)
}

#[test]
fn vec_store_roundtrips_through_gts_to_nquads() {
    let ds = freeze_rows(
        &[
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
        ],
        &[],
        &[],
    );

    let nquads = roundtrip_nquads(&ds);
    assert!(nquads.contains("<https://example.org/s>"));
    assert!(nquads.contains("\"hello\"@en"));
    assert!(nquads.contains("<https://example.org/g>"));
}

#[test]
fn to_gts_is_deterministic() {
    let ds = freeze_rows(
        &[
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
        ],
        &[],
        &[],
    );

    let first = to_gts(&ds, &RdfLookaside::default(), "gmeow-rdf-test").expect("first write");
    let second = to_gts(&ds, &RdfLookaside::default(), "gmeow-rdf-test").expect("second write");
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

    let ds = freeze_rows(
        &[RdfQuad::new(
            reifier.clone(),
            "http://www.w3.org/1999/02/22-rdf-syntax-ns#reifies",
            RdfTerm::triple(statement.clone()),
        )],
        &[RdfReifier::new(reifier.clone(), statement)],
        &[RdfAnnotation::new(
            reifier,
            "https://example.org/confidence",
            RdfTerm::literal(RdfLiteral::typed(
                "0.9",
                "http://www.w3.org/2001/XMLSchema#decimal",
            )),
        )],
    );

    let nquads = roundtrip_nquads(&ds);
    assert!(nquads.contains("http://www.w3.org/1999/02/22-rdf-syntax-ns#reifies"));
    assert!(nquads.contains("https://example.org/confidence"));
    assert!(nquads.contains("0.9"));
}

#[test]
fn to_writer_returns_usable_writer() {
    let ds = freeze_rows(
        &[RdfQuad::new(
            RdfTerm::iri("https://example.org/s"),
            "https://example.org/p",
            RdfTerm::iri("https://example.org/o"),
        )],
        &[],
        &[],
    );

    let writer = to_writer(&ds, &RdfLookaside::default(), "gmeow-rdf-test").expect("to_writer");
    let bytes = writer.to_bytes();
    let graph = gmeow_gts::reader::read(&bytes, false, None);
    assert!(graph.diagnostics.is_empty());
    assert_eq!(graph.quads.len(), 1);
}
