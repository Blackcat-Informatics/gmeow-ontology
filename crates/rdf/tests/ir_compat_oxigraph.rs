// SPDX-FileCopyrightText: 2026 Blackcat Informatics Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Gate (#819 C1/C2 → C3): the IR→`RdfStore` compat adapter feeds the EXISTING
//! oxigraph materializer unchanged. A frozen `RdfDataset` adapted as an `RdfStore`
//! and materialized via `store_from_rdf_store` preserves quad count and the
//! named-graph policy — proving the coexistence bridge works without porting the
//! consumer.

#![cfg(feature = "oxigraph")]

use gmeow_rdf::ir::RdfDatasetBuilder;
use gmeow_rdf::oxigraph::{store_from_rdf_store, GraphPolicy};
use gmeow_rdf::{RdfLiteral, RdfStore, TermId};

fn iri(b: &mut RdfDatasetBuilder, n: &str) -> TermId {
    b.intern_iri(format!("http://example.org/{n}"))
}

/// Build a frozen dataset: a default-graph quad, a named-graph quad, a literal
/// object, plus a reifier + annotation. Adapt `&RdfDataset` as `RdfStore`, then
/// materialize into oxigraph and assert the quads land in the right graphs.
#[test]
fn compat_adapter_feeds_oxigraph_materializer() {
    let mut b = RdfDatasetBuilder::new();
    let (s, p, o) = (iri(&mut b, "s"), iri(&mut b, "p"), iri(&mut b, "o"));
    let g = iri(&mut b, "graph");
    let lit = b.intern_literal(RdfLiteral::language_tagged("Bonjour", "fr"));
    // Default-graph quad, named-graph quad, and a literal-object quad.
    b.push_quad(s, p, o, None);
    b.push_quad(s, p, o, Some(g));
    b.push_quad(s, p, lit, None);
    // A reifier + annotation (the materializer emits these as rdf:reifies / direct
    // triples, exercising the adapter's reifiers()/annotations()).
    let triple = b.intern_triple(s, p, o);
    let r = iri(&mut b, "r");
    let ap = iri(&mut b, "ap");
    let ao = iri(&mut b, "ao");
    b.push_reifier(r, triple);
    b.push_annotation(r, ap, ao);

    let ds = b.freeze().expect("valid dataset");

    // `&RdfDataset` IS an `RdfStore`. Materialize preserving named graphs.
    let store_ref: &gmeow_rdf::RdfDataset = &ds;
    assert_eq!(RdfStore::len_hint(&store_ref), Some(3));

    let store = store_from_rdf_store(&store_ref, GraphPolicy::PreserveNamedGraphs)
        .expect("materialize via compat adapter");

    // 3 base quads + 1 reifier quad + 1 annotation quad = 5 quads in the store.
    assert_eq!(
        store.len().expect("len"),
        5,
        "all base quads + reifier + annotation materialize"
    );

    // The named-graph quad landed in ex:graph, NOT the default graph.
    use oxigraph::model::{GraphNameRef, NamedNodeRef};
    let graph = NamedNodeRef::new("http://example.org/graph").expect("iri");
    let in_named = store
        .quads_for_pattern(None, None, None, Some(GraphNameRef::NamedNode(graph)))
        .count();
    assert_eq!(
        in_named, 1,
        "exactly the named-graph quad lands in ex:graph"
    );

    // And the default graph holds the rest (2 base + reifier + annotation = 4).
    let in_default = store
        .quads_for_pattern(None, None, None, Some(GraphNameRef::DefaultGraph))
        .count();
    assert_eq!(
        in_default, 4,
        "default-graph quads incl. reifier/annotation"
    );
}

/// Flattening collapses the named graph into the default graph: all 3 base quads +
/// reifier + annotation end up in the default graph.
#[test]
fn compat_adapter_honours_flatten_policy() {
    let mut b = RdfDatasetBuilder::new();
    let (s, p, o) = (iri(&mut b, "s"), iri(&mut b, "p"), iri(&mut b, "o"));
    let g = iri(&mut b, "graph");
    b.push_quad(s, p, o, None);
    b.push_quad(s, p, o, Some(g));
    let ds = b.freeze().expect("valid");

    let store_ref: &gmeow_rdf::RdfDataset = &ds;
    let store = store_from_rdf_store(&store_ref, GraphPolicy::FlattenToDefaultGraph)
        .expect("materialize flattened");

    // The two quads differ only in graph; flattening collapses them to ONE.
    use oxigraph::model::GraphNameRef;
    let in_default = store
        .quads_for_pattern(None, None, None, Some(GraphNameRef::DefaultGraph))
        .count();
    assert_eq!(
        in_default, 1,
        "named + default quad collapse when flattened"
    );
    assert_eq!(store.len().expect("len"), 1);
}
