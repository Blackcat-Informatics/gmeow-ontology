// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Shared oxigraph access helpers for the compiler front-end + adapter.
//!
//! These provide the RDF term/graph idioms the compiler relies on — `str(node)`,
//! `graph.value(s, p)`, `graph.objects(s, p)`, `graph.subjects(p, o)` — over an
//! oxigraph default-graph [`Store`], so the frontend, adapter, and projections
//! share one definition of node stringification (the golden-pinned surface).
//!
//! This is a shared toolkit built up across the #664 tasks; a few helpers
//! (e.g. [`contains`]) land here for the projection back-ends (Task 4) before
//! they have an in-tree caller, so the module allows `dead_code` crate-internally
//! rather than scattering per-item attributes.
#![allow(dead_code)]

use oxigraph::model::{
    GraphNameRef, NamedNode, NamedNodeRef, NamedOrBlankNode, NamedOrBlankNodeRef, Quad, Term,
};
use oxigraph::store::Store;

// Well-known RDF IRIs (string constants — avoids per-call `NamedNode::new`).
pub(crate) const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
pub(crate) const RDF_REIFIES: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#reifies";
pub(crate) const RDF_STATEMENT: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#Statement";
pub(crate) const RDF_SUBJECT: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#subject";
pub(crate) const RDF_PREDICATE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#predicate";
pub(crate) const RDF_OBJECT: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#object";

/// `str(node)` for an object term — matches rdflib: IRI → the IRI; blank node →
/// the bare id; literal → its lexical value (no datatype/quotes).
pub(crate) fn term_str(term: &Term) -> String {
    match term {
        Term::NamedNode(nn) => nn.as_str().to_owned(),
        Term::BlankNode(bn) => bn.as_str().to_owned(),
        Term::Literal(lit) => lit.value().to_owned(),
        Term::Triple(_) => panic!(
            "RDF-star quoted-triple terms are not supported in gmeow-logic v1 \
             (Term::Triple cannot be stringified without silent data loss)"
        ),
    }
}

/// `str(node)` for a subject node.
pub(crate) fn subject_str(s: &NamedOrBlankNode) -> String {
    match s {
        NamedOrBlankNode::NamedNode(nn) => nn.as_str().to_owned(),
        NamedOrBlankNode::BlankNode(bn) => bn.as_str().to_owned(),
    }
}

/// Whether a term is a literal (rdflib `isinstance(o, Literal)`).
pub(crate) fn term_is_literal(term: &Term) -> bool {
    matches!(term, Term::Literal(_))
}

/// Whether a subject node is a blank node (rdflib `isinstance(s, BNode)`).
pub(crate) fn subject_is_blank(s: &NamedOrBlankNode) -> bool {
    matches!(s, NamedOrBlankNode::BlankNode(_))
}

/// Whether an object term is a blank node.
pub(crate) fn term_is_blank(t: &Term) -> bool {
    matches!(t, Term::BlankNode(_))
}

/// View a term as a subject node (for `graph.value(term, ...)` lookups), if it
/// is an IRI or blank node.
pub(crate) fn term_as_subject(term: &Term) -> Option<NamedOrBlankNode> {
    match term {
        Term::NamedNode(nn) => Some(NamedOrBlankNode::NamedNode(nn.clone())),
        Term::BlankNode(bn) => Some(NamedOrBlankNode::BlankNode(bn.clone())),
        _ => None,
    }
}

/// Construct a [`NamedNode`] from a known-valid IRI string.
pub(crate) fn nn(iri: &str) -> NamedNode {
    NamedNode::new(iri).unwrap_or_else(|e| panic!("invalid built-in IRI {iri:?}: {e}"))
}

/// All triples in the default graph, materialized for repeated iteration.
pub(crate) fn default_graph_quads(store: &Store) -> Vec<Quad> {
    store
        .quads_for_pattern(None, None, None, Some(GraphNameRef::DefaultGraph))
        .filter_map(Result::ok)
        .collect()
}

/// Whether the default graph is empty.
pub(crate) fn is_empty(store: &Store) -> bool {
    store
        .quads_for_pattern(None, None, None, Some(GraphNameRef::DefaultGraph))
        .next()
        .is_none()
}

/// `graph.value(subject, predicate)` — the first object of
/// `(subject, predicate, *)` in the default graph, or `None`.
pub(crate) fn value(
    store: &Store,
    subject: &NamedOrBlankNode,
    predicate: &NamedNode,
) -> Option<Term> {
    let s: NamedOrBlankNodeRef<'_> = subject.as_ref();
    let p: NamedNodeRef<'_> = predicate.as_ref();
    store
        .quads_for_pattern(Some(s), Some(p), None, Some(GraphNameRef::DefaultGraph))
        .filter_map(Result::ok)
        .next()
        .map(|q| q.object)
}

/// All objects of `(subject, predicate, *)` in the default graph.
pub(crate) fn objects(
    store: &Store,
    subject: &NamedOrBlankNode,
    predicate: &NamedNode,
) -> Vec<Term> {
    let s: NamedOrBlankNodeRef<'_> = subject.as_ref();
    let p: NamedNodeRef<'_> = predicate.as_ref();
    store
        .quads_for_pattern(Some(s), Some(p), None, Some(GraphNameRef::DefaultGraph))
        .filter_map(Result::ok)
        .map(|q| q.object)
        .collect()
}

/// All subjects of `(*, predicate, object)` in the default graph.
pub(crate) fn subjects_with(
    store: &Store,
    predicate: &NamedNode,
    object: &Term,
) -> Vec<NamedOrBlankNode> {
    let p: NamedNodeRef<'_> = predicate.as_ref();
    store
        .quads_for_pattern(
            None,
            Some(p),
            Some(object.as_ref()),
            Some(GraphNameRef::DefaultGraph),
        )
        .filter_map(Result::ok)
        .map(|q| q.subject)
        .collect()
}

/// Whether the triple `(subject, predicate, object)` exists in the default graph.
pub(crate) fn contains(
    store: &Store,
    subject: &NamedOrBlankNode,
    predicate: &NamedNode,
    object: &Term,
) -> bool {
    store
        .quads_for_pattern(
            Some(subject.as_ref()),
            Some(predicate.as_ref()),
            Some(object.as_ref()),
            Some(GraphNameRef::DefaultGraph),
        )
        .next()
        .is_some()
}
