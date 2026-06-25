// SPDX-FileCopyrightText: 2026 Blackcat Informatics Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The gmeow-rdf public RDF canonicalization API (#910) — the native full W3C
//! RDFC-1.0 surface that replaces `oxrdf`'s `Dataset::canonicalize` across the
//! workspace (EPIC #906 oxigraph eviction).
//!
//! The canonicalization *engine* lives in the oxigraph-free kernel
//! ([`gmeow_rdf_core::ir::canon`]); this module is the thin oxigraph-facing adapter.
//! It bridges a flat oxigraph quad set / store into the IR, runs the native
//! canonicalizer, and either returns the canonical N-Quads document or maps the
//! canonical blank labels back onto the caller's oxigraph terms.
//!
//! **Flat, not overlay-folded.** Unlike [`crate::dataset_from_oxigraph_quads`] —
//! which interprets `rdf:reifies` rows as the IR reifier/annotation overlay — this
//! adapter bridges every quad *flat* (an `rdf:reifies` quad with a triple-term object
//! stays exactly that). That matches `oxrdf` `Dataset::canonicalize` semantics, so
//! this is a true drop-in: the only thing that changes is blank-node labeling.

use std::collections::HashMap;
use std::sync::Arc;

use oxigraph::model::{BlankNode, GraphName, NamedOrBlankNode, Quad, Term, Triple};
use oxigraph::store::Store;

use gmeow_rdf_core::{
    canonicalize as core_canonicalize, canonicalize_with as core_canonicalize_with, CanonHash,
    Canonicalized,
};

use crate::oxigraph::rdf_quad_from_oxigraph;
use crate::{RdfDataset, RdfDatasetBuilder, RdfDiagnostic, TermRef};

/// Bridge a flat oxigraph quad set into the IR (no reifier/annotation overlay).
fn flat_dataset<'a>(
    quads: impl IntoIterator<Item = &'a Quad>,
) -> Result<Arc<RdfDataset>, RdfDiagnostic> {
    let mut builder = RdfDatasetBuilder::new();
    for quad in quads {
        builder.push_owned_quad(&rdf_quad_from_oxigraph(quad));
    }
    builder.freeze()
}

/// Map each original oxigraph blank-node id to its canonical `c14nN` label.
fn label_map(ds: &RdfDataset, c: &Canonicalized) -> HashMap<String, String> {
    let mut map = HashMap::with_capacity(c.labels.len());
    for (&tid, label) in &c.labels {
        if let TermRef::Blank { label: orig, .. } = ds.resolve(tid) {
            map.insert(orig.to_owned(), label.to_string());
        }
    }
    map
}

/// The canonical N-Quads document for a flat oxigraph quad set, under native
/// RDFC-1.0. Lines are bytewise-sorted, deduplicated, and `'\n'`-terminated; only
/// blank-node labels are canonicalized (literal lexical forms are verbatim).
pub fn canonical_nquads<'a>(
    quads: impl IntoIterator<Item = &'a Quad>,
) -> Result<String, RdfDiagnostic> {
    let ds = flat_dataset(quads)?;
    Ok(core_canonicalize(&ds).nquads)
}

/// [`canonical_nquads`] with an explicit RDFC-1.0 hash algorithm
/// ([`CanonHash::Sha384`] selects the SHA-384 variant).
pub fn canonical_nquads_with<'a>(
    quads: impl IntoIterator<Item = &'a Quad>,
    hash: CanonHash,
) -> Result<String, RdfDiagnostic> {
    let ds = flat_dataset(quads)?;
    Ok(core_canonicalize_with(&ds, hash).nquads)
}

/// Canonicalize a quad set's blank-node labels under native RDFC-1.0, returning the
/// quads with canonical (`_:c14nN`) blanks, sorted by their N-Quads string. The
/// caller's literal/IRI term forms are preserved exactly (only blanks are relabeled).
pub fn canonicalize_quads(quads: Vec<Quad>) -> Result<Vec<Quad>, RdfDiagnostic> {
    let ds = flat_dataset(quads.iter())?;
    let canon = core_canonicalize(&ds);
    let map = label_map(&ds, &canon);
    let mut out: Vec<Quad> = quads.iter().map(|q| relabel_quad(q, &map)).collect();
    out.sort_by_key(Quad::to_string);
    out.dedup();
    Ok(out)
}

/// Canonicalize the blank-node labels of every quad in `store`, returning a fresh
/// store whose blanks carry their canonical `c14nN` labels.
pub fn canonicalize_store(store: &Store) -> Result<Store, RdfDiagnostic> {
    let quads: Vec<Quad> = store
        .iter()
        .collect::<Result<_, _>>()
        .map_err(|e| RdfDiagnostic::error("oxigraph-store-iter", e.to_string()))?;
    let canonical = canonicalize_quads(quads)?;
    let out =
        Store::new().map_err(|e| RdfDiagnostic::error("oxigraph-store-create", e.to_string()))?;
    for quad in &canonical {
        out.insert(quad)
            .map_err(|e| RdfDiagnostic::error("oxigraph-store-insert", e.to_string()))?;
    }
    Ok(out)
}

/// Rewrite a quad, replacing every blank-node label via `map` (recursing triple terms).
fn relabel_quad(quad: &Quad, map: &HashMap<String, String>) -> Quad {
    Quad::new(
        relabel_subject(&quad.subject, map),
        quad.predicate.clone(),
        relabel_term(&quad.object, map),
        relabel_graph(&quad.graph_name, map),
    )
}

fn relabel_subject(node: &NamedOrBlankNode, map: &HashMap<String, String>) -> NamedOrBlankNode {
    match node {
        NamedOrBlankNode::NamedNode(n) => NamedOrBlankNode::NamedNode(n.clone()),
        NamedOrBlankNode::BlankNode(b) => NamedOrBlankNode::BlankNode(relabel_blank(b, map)),
    }
}

fn relabel_term(term: &Term, map: &HashMap<String, String>) -> Term {
    match term {
        Term::NamedNode(n) => Term::NamedNode(n.clone()),
        Term::BlankNode(b) => Term::BlankNode(relabel_blank(b, map)),
        Term::Literal(l) => Term::Literal(l.clone()),
        Term::Triple(t) => Term::Triple(Box::new(Triple::new(
            relabel_subject(&t.subject, map),
            t.predicate.clone(),
            relabel_term(&t.object, map),
        ))),
    }
}

fn relabel_graph(graph: &GraphName, map: &HashMap<String, String>) -> GraphName {
    match graph {
        GraphName::NamedNode(n) => GraphName::NamedNode(n.clone()),
        GraphName::BlankNode(b) => GraphName::BlankNode(relabel_blank(b, map)),
        GraphName::DefaultGraph => GraphName::DefaultGraph,
    }
}

/// Relabel one blank node; an unmapped blank (should not occur — canonicalization
/// labels every blank) keeps its original id.
fn relabel_blank(blank: &BlankNode, map: &HashMap<String, String>) -> BlankNode {
    match map.get(blank.as_str()) {
        Some(canon) => BlankNode::new_unchecked(canon.as_str()),
        None => blank.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use oxigraph::io::{RdfFormat, RdfParser};

    fn parse(nq: &str) -> Vec<Quad> {
        RdfParser::from_format(RdfFormat::NQuads)
            .for_reader(nq.as_bytes())
            .map(|q| q.expect("valid quad"))
            .collect()
    }

    #[test]
    fn isomorphic_quad_sets_canonicalize_identically() {
        // Two isomorphic graphs with different blank labels must canonicalize to the
        // SAME quad strings (RDFC-1.0 determinism).
        let g1 = parse("_:a <https://example.org/p> _:b .\n_:b <https://example.org/q> _:a .\n");
        let g2 = parse("_:x <https://example.org/p> _:y .\n_:y <https://example.org/q> _:x .\n");
        let c1: Vec<String> = canonicalize_quads(g1)
            .unwrap()
            .iter()
            .map(Quad::to_string)
            .collect();
        let c2: Vec<String> = canonicalize_quads(g2)
            .unwrap()
            .iter()
            .map(Quad::to_string)
            .collect();
        assert_eq!(c1, c2, "isomorphic graphs canonicalize identically");
        assert!(
            c1.iter().any(|q| q.contains("_:c14n")),
            "canonical labels: {c1:?}"
        );
    }

    #[test]
    fn ground_quads_are_unchanged_and_sorted() {
        let quads = parse(
            "<https://e/s> <https://e/p> <https://e/o2> .\n<https://e/s> <https://e/p> <https://e/o1> .\n",
        );
        let out = canonicalize_quads(quads).unwrap();
        let strs: Vec<String> = out.iter().map(Quad::to_string).collect();
        assert_eq!(strs.len(), 2);
        assert!(
            strs[0].contains("o1") && strs[1].contains("o2"),
            "sorted: {strs:?}"
        );
    }

    #[test]
    fn canonical_nquads_is_deterministic() {
        let g1 = parse("_:a <https://e/p> _:b .\n");
        let g2 = parse("_:zzz <https://e/p> _:aaa .\n");
        assert_eq!(
            canonical_nquads(g1.iter()).unwrap(),
            canonical_nquads(g2.iter()).unwrap(),
        );
    }

    #[test]
    fn canonicalize_store_round_trips_through_a_store() {
        let store = Store::new().unwrap();
        for q in parse("_:a <https://e/p> _:b .\n_:b <https://e/q> _:a .\n") {
            store.insert(&q).unwrap();
        }
        let out = canonicalize_store(&store).unwrap();
        assert_eq!(out.len().unwrap(), 2);
        let strs: std::collections::BTreeSet<String> =
            out.iter().map(|q| q.unwrap().to_string()).collect();
        assert!(
            strs.iter().all(|q| q.contains("_:c14n")),
            "canonical blanks: {strs:?}"
        );
    }
}
