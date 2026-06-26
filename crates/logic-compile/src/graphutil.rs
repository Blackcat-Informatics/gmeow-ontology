// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Shared wasm-clean RDF access helpers for the compiler front-end + adapter.
//!
//! These provide the RDF term/graph idioms the compiler relies on — `str(node)`,
//! `graph.value(s, p)`, `graph.objects(s, p)`, `graph.subjects(p, o)` — over the
//! oxigraph-free [`RdfDataset`] (the wasm-clean `gmeow-rdf` `gts` surface, #885 /
//! #909), so the frontend, adapter, and projections share one definition of node
//! stringification (the golden-pinned surface) and the whole compiler builds for
//! `wasm32-unknown-unknown` — no oxigraph Store, no RocksDB.
//!
//! The pure term model below ([`Node`] / [`Subject`] / [`Quad`]) replaces the
//! `oxigraph::model` types: a subject is always an IRI or blank node, an object may
//! additionally be a literal or an RDF 1.2 quoted-triple term. Only the lexical
//! value of a literal is carried — the compiler never inspects datatype or language
//! on the parse path (it stringifies via [`term_str`]).
//!
//! This is a shared toolkit built up across the #664 tasks; a few helpers
//! (e.g. [`contains`]) land here for the projection back-ends before they have an
//! in-tree caller, so the module allows `dead_code` crate-internally rather than
//! scattering per-item attributes.
#![allow(dead_code)]

use gmeow_rdf::{canonicalize, parse_dataset, BlankScope, RdfDataset, TermId, TermRef, TermValue};
use std::sync::Arc;

// Well-known RDF IRIs (string constants — avoids per-call interning at the source).
pub(crate) const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
pub(crate) const RDF_REIFIES: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#reifies";
pub(crate) const RDF_STATEMENT: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#Statement";
pub(crate) const RDF_SUBJECT: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#subject";
pub(crate) const RDF_PREDICATE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#predicate";
pub(crate) const RDF_OBJECT: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#object";

// --------------------------------------------------------------------------- //
// Pure term model (wasm-clean replacement for the oxigraph::model types)
// --------------------------------------------------------------------------- //

/// An IRI used in predicate position (the wasm-clean stand-in for the oxigraph
/// `NamedNode` predicate type, so predicate call-sites stay `&nn(iri)`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Iri(String);

impl Iri {
    /// The IRI string.
    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

/// A subject node: an IRI or a blank node (an RDF subject is never a literal; the
/// logic: vocabulary places quoted-triple terms only in object position).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Subject {
    Iri(String),
    Blank { label: String, scope: BlankScope },
}

/// An RDF term (object position): IRI, blank node, literal (lexical value only), or
/// an RDF 1.2 quoted-triple term.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Node {
    Iri(String),
    Blank { label: String, scope: BlankScope },
    Lit(String),
    Triple(Box<TripleTerm>),
}

impl Node {
    /// Construct a named-node (IRI) object term.
    pub(crate) fn iri(iri: impl Into<String>) -> Self {
        Node::Iri(iri.into())
    }
}

/// An RDF 1.2 quoted-triple term (the object of `rdf:reifies`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TripleTerm {
    pub subject: Subject,
    pub predicate: Iri,
    pub object: Node,
}

/// A default-graph triple, resolved to the pure term model.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Quad {
    pub subject: Subject,
    pub predicate: Iri,
    pub object: Node,
}

/// `str(node)` for an object term — matches rdflib: IRI → the IRI; blank node →
/// the bare id; literal → its lexical value (no datatype/quotes).
pub(crate) fn term_str(term: &Node) -> String {
    match term {
        Node::Iri(iri) => iri.clone(),
        Node::Blank { label, .. } => label.clone(),
        Node::Lit(value) => value.clone(),
        Node::Triple(_) => panic!(
            "RDF-star quoted-triple terms are not supported in gmeow-logic v1 \
             (a quoted triple cannot be stringified without silent data loss)"
        ),
    }
}

/// `str(node)` for a subject node.
pub(crate) fn subject_str(s: &Subject) -> String {
    match s {
        Subject::Iri(iri) => iri.clone(),
        Subject::Blank { label, .. } => label.clone(),
    }
}

/// Whether a term is a literal (rdflib `isinstance(o, Literal)`).
pub(crate) fn term_is_literal(term: &Node) -> bool {
    matches!(term, Node::Lit(_))
}

/// Whether a subject node is a blank node (rdflib `isinstance(s, BNode)`).
pub(crate) fn subject_is_blank(s: &Subject) -> bool {
    matches!(s, Subject::Blank { .. })
}

/// Whether an object term is a blank node.
pub(crate) fn term_is_blank(t: &Node) -> bool {
    matches!(t, Node::Blank { .. })
}

/// View a term as a subject node (for `graph.value(term, ...)` lookups), if it is
/// an IRI or blank node.
pub(crate) fn term_as_subject(term: &Node) -> Option<Subject> {
    match term {
        Node::Iri(iri) => Some(Subject::Iri(iri.clone())),
        Node::Blank { label, scope } => Some(Subject::Blank {
            label: label.clone(),
            scope: *scope,
        }),
        _ => None,
    }
}

/// Construct an [`Iri`] (predicate) from a known-valid IRI string.
pub(crate) fn nn(iri: &str) -> Iri {
    Iri(iri.to_owned())
}

// --------------------------------------------------------------------------- //
// Resolution: TermId → pure term model
// --------------------------------------------------------------------------- //

/// Resolve a predicate (always an IRI) to its string.
fn iri_of(ds: &RdfDataset, id: TermId) -> Iri {
    match ds.resolve(id) {
        TermRef::Iri(s) => Iri(s.to_owned()),
        // A predicate is always an IRI; the remaining cases are unreachable for a
        // well-formed dataset. Render them losslessly rather than panic (the
        // never-panic fuzz gate must hold for any parsed input).
        other => Iri(render_term(ds, other)),
    }
}

/// Resolve a subject position to the pure [`Subject`] model.
fn subject_of(ds: &RdfDataset, id: TermId) -> Subject {
    match ds.resolve(id) {
        TermRef::Iri(s) => Subject::Iri(s.to_owned()),
        TermRef::Blank { label, scope } => Subject::Blank {
            label: label.to_owned(),
            scope,
        },
        // A subject is always an IRI or blank node in the logic: source. A literal
        // or quoted-triple in subject position cannot arise from valid RDF; fall
        // back to an IRI-shaped rendering so the compiler never panics.
        other => Subject::Iri(render_term(ds, other)),
    }
}

/// Resolve an object position to the pure [`Node`] model.
fn node_of(ds: &RdfDataset, id: TermId) -> Node {
    match ds.resolve(id) {
        TermRef::Iri(s) => Node::Iri(s.to_owned()),
        TermRef::Blank { label, scope } => Node::Blank {
            label: label.to_owned(),
            scope,
        },
        TermRef::Literal { lexical, .. } => Node::Lit(lexical.to_owned()),
        TermRef::Triple { s, p, o } => Node::Triple(Box::new(TripleTerm {
            subject: subject_of(ds, s),
            predicate: iri_of(ds, p),
            object: node_of(ds, o),
        })),
    }
}

/// Best-effort lexical rendering of any term (used only for the unreachable
/// non-IRI predicate / non-node subject fallbacks above).
fn render_term(ds: &RdfDataset, term: TermRef<'_>) -> String {
    match term {
        TermRef::Iri(s) => s.to_owned(),
        TermRef::Blank { label, .. } => label.to_owned(),
        TermRef::Literal { lexical, .. } => lexical.to_owned(),
        TermRef::Triple { s, p, o } => format!(
            "<<{} {} {}>>",
            render_term(ds, ds.resolve(s)),
            render_term(ds, ds.resolve(p)),
            render_term(ds, ds.resolve(o)),
        ),
    }
}

// --------------------------------------------------------------------------- //
// Lookup: pure term model → TermId (for pattern queries)
// --------------------------------------------------------------------------- //

/// Intern a subject node to its dataset [`TermId`], or `None` if the dataset does
/// not contain it (the wasm-clean analogue of an oxigraph pattern miss).
fn subject_id(ds: &RdfDataset, subject: &Subject) -> Option<TermId> {
    let value = match subject {
        Subject::Iri(iri) => TermValue::Iri(iri.clone()),
        Subject::Blank { label, scope } => TermValue::Blank {
            label: label.clone(),
            scope: *scope,
        },
    };
    ds.term_id_by_value(&value)
}

/// Intern a predicate IRI to its dataset [`TermId`].
fn predicate_id(ds: &RdfDataset, predicate: &Iri) -> Option<TermId> {
    ds.term_id_by_value(&TermValue::Iri(predicate.0.clone()))
}

/// Intern an object term to its dataset [`TermId`]. Only IRI/blank objects are
/// interned as query keys here — the compiler only ever matches on IRI objects
/// (`rdf:type` class terms); a literal/triple object key cannot be reconstructed
/// without datatype/language and never occurs as a query key, so it yields `None`.
fn object_id(ds: &RdfDataset, object: &Node) -> Option<TermId> {
    let value = match object {
        Node::Iri(iri) => TermValue::Iri(iri.clone()),
        Node::Blank { label, scope } => TermValue::Blank {
            label: label.clone(),
            scope: *scope,
        },
        Node::Lit(_) | Node::Triple(_) => return None,
    };
    ds.term_id_by_value(&value)
}

// --------------------------------------------------------------------------- //
// Blank-node canonicalization
// --------------------------------------------------------------------------- //

/// Re-label every blank node in `ds` to its RDFC-1.0 canonical label, returning a
/// fresh dataset whose blank-node identifiers are a deterministic function of graph
/// structure rather than the parser's per-parse random ids.
///
/// This is the determinism source for the whole compiler. The RDF back-ends either
/// canonicalize on output or rewrite rule atoms to deterministic `rule/NNNN/...`
/// IRIs, so they were already byte-stable; the *text* back-ends (Datalog / Nemo /
/// N3) emit a blank node's raw label verbatim, so a random parse-time id leaked
/// straight into `gmeow.rls` / `gmeow.dl` / `gmeow.n3` and the conformance goldens,
/// making them differ on every run. Canonicalizing once at load fixes every
/// projection at the source (greenfield: one deterministic front door, not a
/// per-back-end patch).
///
/// Implementation: native full RDFC-1.0 (#910), the wasm-clean `gmeow_rdf::canonicalize`
/// (SHA-256), then re-parse the canonical N-Quads so the relabeled `_:c14nN` ids
/// become the dataset's blank labels. The labeling is identical to the oxigraph
/// `canonicalize_store` it replaces (both are conformant RDFC-1.0 / SHA-256), so the
/// text back-ends and conformance goldens are unchanged.
pub(crate) fn canonicalize_blank_nodes(ds: &RdfDataset) -> Result<Arc<RdfDataset>, String> {
    let canon = canonicalize(ds);
    parse_dataset(canon.nquads.as_bytes(), "application/n-quads", None)
        .map_err(|e| format!("blank-node canonicalization re-parse: {e}"))
}

// --------------------------------------------------------------------------- //
// Default-graph queries
// --------------------------------------------------------------------------- //

/// All triples in the default graph, materialized for repeated iteration.
pub(crate) fn default_graph_quads(ds: &RdfDataset) -> Vec<Quad> {
    ds.quads()
        .filter(|q| q.g.is_none())
        .map(|q| Quad {
            subject: subject_of(ds, q.s),
            predicate: iri_of(ds, q.p),
            object: node_of(ds, q.o),
        })
        .collect()
}

/// Whether the default graph is empty.
pub(crate) fn is_empty(ds: &RdfDataset) -> bool {
    !ds.quads().any(|q| q.g.is_none())
}

/// `graph.value(subject, predicate)` — the first object of
/// `(subject, predicate, *)` in the default graph, or `None`.
pub(crate) fn value(ds: &RdfDataset, subject: &Subject, predicate: &Iri) -> Option<Node> {
    let s_id = subject_id(ds, subject)?;
    let p_id = predicate_id(ds, predicate)?;
    ds.quads()
        .find(|q| q.g.is_none() && q.s == s_id && q.p == p_id)
        .map(|q| node_of(ds, q.o))
}

/// All objects of `(subject, predicate, *)` in the default graph.
pub(crate) fn objects(ds: &RdfDataset, subject: &Subject, predicate: &Iri) -> Vec<Node> {
    let (Some(s_id), Some(p_id)) = (subject_id(ds, subject), predicate_id(ds, predicate)) else {
        return Vec::new();
    };
    ds.quads()
        .filter(|q| q.g.is_none() && q.s == s_id && q.p == p_id)
        .map(|q| node_of(ds, q.o))
        .collect()
}

/// All subjects of `(*, predicate, object)` in the default graph.
pub(crate) fn subjects_with(ds: &RdfDataset, predicate: &Iri, object: &Node) -> Vec<Subject> {
    let (Some(p_id), Some(o_id)) = (predicate_id(ds, predicate), object_id(ds, object)) else {
        return Vec::new();
    };
    ds.quads()
        .filter(|q| q.g.is_none() && q.p == p_id && q.o == o_id)
        .map(|q| subject_of(ds, q.s))
        .collect()
}

/// Whether the triple `(subject, predicate, object)` exists in the default graph.
pub(crate) fn contains(ds: &RdfDataset, subject: &Subject, predicate: &Iri, object: &Node) -> bool {
    let (Some(s_id), Some(p_id), Some(o_id)) = (
        subject_id(ds, subject),
        predicate_id(ds, predicate),
        object_id(ds, object),
    ) else {
        return false;
    };
    ds.quads()
        .any(|q| q.g.is_none() && q.s == s_id && q.p == p_id && q.o == o_id)
}
