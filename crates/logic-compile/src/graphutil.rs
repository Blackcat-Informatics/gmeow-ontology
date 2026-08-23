// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Shared wasm-clean RDF access helpers for the compiler front-end + adapter.
//!
//! These provide the RDF term/graph idioms the compiler relies on — `str(node)`,
//! `graph.value(s, p)`, `graph.objects(s, p)`, `graph.subjects(p, o)` — over the
//! oxigraph-free [`RdfDataset`] (the wasm-clean `purrdf` `gts` surface
//! ), so the frontend, adapter, and projections share one definition of node
//! stringification (the golden-pinned surface) and the whole compiler builds for
//! `wasm32-unknown-unknown` — no oxigraph Store, no RocksDB.
//!
//! The pure term model below ([`Node`] / [`Subject`] / [`Quad`]) replaces the
//! `oxigraph::model` types: a subject is always an IRI or blank node, an object may
//! additionally be a literal or an RDF 1.2 quoted-triple term. A literal carries its
//! lexical value AND its datatype IRI / language tag ([`Node::Lit`]), so a typed value
//! (`"1"^^xsd:integer`) round-trips its datatype into the derived `sh:hasValue` / `sh:in`
//! surfaces; [`term_str`] still yields the bare lexical form for the callers that want it.
//!
//! This is a shared toolkit built up across the tasks; a few helpers
//! (e.g. [`contains`]) land here for the projection back-ends before they have an
//! in-tree caller, so the module allows `dead_code` crate-internally rather than
//! scattering per-item attributes.
#![allow(dead_code)]

use gmeow_errors::Diag;
use purrdf::dataset_view::{DatasetView, GraphMatch};
use purrdf::{BlankScope, RdfDataset, TermId, TermRef, TermValue, canonicalize, parse_dataset};
use std::sync::Arc;

// Well-known RDF IRIs (string constants — avoids per-call interning at the source).
pub(crate) const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
pub(crate) const RDF_REIFIES: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#reifies";
pub(crate) const RDF_STATEMENT: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#Statement";
pub(crate) const RDF_SUBJECT: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#subject";
pub(crate) const RDF_PREDICATE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#predicate";
pub(crate) const RDF_OBJECT: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#object";
/// The implicit datatype of a plain literal — normalized to an untyped [`Node::Lit`]
/// (`datatype: None`) so an authored `"foo"` and an equivalent `"foo"^^xsd:string` collapse to
/// the same untyped carrier.
const XSD_STRING: &str = "http://www.w3.org/2001/XMLSchema#string";

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

/// An RDF term (object position): IRI, blank node, literal (lexical value plus its
/// datatype/language), or an RDF 1.2 quoted-triple term.
///
/// A literal carries its `datatype` IRI and `lang` tag so a typed value round-trips its type
/// (the `owl:hasValue "1"^^xsd:integer` / `owl:oneOf` value-equality path needs the datatype,
/// not just the lexical form). The two are normalized on resolution: a language-tagged literal
/// records `lang` and leaves `datatype` `None` (the datatype is the implied `rdf:langString`); a
/// plain `xsd:string` records neither (untyped); every other datatype records `datatype`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Node {
    Iri(String),
    Blank {
        label: String,
        scope: BlankScope,
    },
    Lit {
        lexical: String,
        datatype: Option<String>,
        lang: Option<String>,
    },
    Triple(Box<TripleTerm>),
}

impl Node {
    /// Construct a named-node (IRI) object term.
    pub(crate) fn iri(iri: impl Into<String>) -> Self {
        Node::Iri(iri.into())
    }

    /// Construct an untyped (plain `xsd:string`) literal object term — the datatype/language
    /// carriers are `None`. Used by the term-model constructors that never mint a typed literal.
    pub(crate) fn plain_lit(lexical: impl Into<String>) -> Self {
        Node::Lit {
            lexical: lexical.into(),
            datatype: None,
            lang: None,
        }
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
        Node::Lit { lexical, .. } => lexical.clone(),
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
    matches!(term, Node::Lit { .. })
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
pub(crate) fn iri_of(ds: &RdfDataset, id: TermId) -> Iri {
    match ds.resolve(id) {
        TermRef::Iri(s) => Iri(s.to_owned()),
        // A predicate is always an IRI; the remaining cases are unreachable for a
        // well-formed dataset. Render them losslessly rather than panic (the
        // never-panic fuzz gate must hold for any parsed input).
        other => Iri(render_term(ds, other)),
    }
}

/// Resolve a subject position to the pure [`Subject`] model.
pub(crate) fn subject_of(ds: &RdfDataset, id: TermId) -> Subject {
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
        TermRef::Literal {
            lexical,
            datatype,
            language,
            ..
        } => {
            // A language-tagged literal records its `lang`; its datatype is the implied
            // `rdf:langString`, so the datatype carrier stays `None`. A plain `xsd:string`
            // records neither; every other datatype resolves to its IRI and is preserved.
            let lang = language.map(str::to_owned);
            let datatype = if lang.is_some() {
                None
            } else {
                match ds.resolve(datatype) {
                    TermRef::Iri(dt) if dt != XSD_STRING => Some(dt.to_owned()),
                    _ => None,
                }
            };
            Node::Lit {
                lexical: lexical.to_owned(),
                datatype,
                lang,
            }
        }
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
        Node::Lit { .. } | Node::Triple(_) => return None,
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
/// IRIs, so they were already byte-stable; the *text* back-ends (Datalog /
/// N3) emit a blank node's raw label verbatim, so a random parse-time id leaked
/// straight into `gmeow.dl` / `gmeow.n3` and the conformance goldens,
/// making them differ on every run. Canonicalizing once at load fixes every
/// projection at the source (greenfield: one deterministic front door, not a
/// per-back-end patch).
///
/// Implementation: native full RDFC-1.0, the wasm-clean `purrdf::canonicalize`
/// (SHA-256), then re-parse the canonical N-Quads so the relabeled `_:c14nN` ids
/// become the dataset's blank labels. The labeling is identical to the oxigraph
/// `canonicalize_store` it replaces (both are conformant RDFC-1.0 / SHA-256), so the
/// text back-ends and conformance goldens are unchanged.
pub(crate) fn canonicalize_blank_nodes(ds: &RdfDataset) -> gmeow_errors::Result<Arc<RdfDataset>> {
    let canon = canonicalize(ds);
    parse_dataset(canon.nquads.as_bytes(), "application/n-quads", None).map_err(|e| {
        Diag::of_kind(crate::error::Graph {
            detail: format!("blank-node canonicalization re-parse: {e}"),
        })
    })
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
///
/// This and the pattern helpers below route through the dataset's INDEXED
/// [`DatasetView::quads_for_pattern`] (lazy permutation indexes + binary search)
/// rather than a full `ds.quads()` scan: the frontend calls these once per
/// class/record over the whole merged authored ontology, so a linear scan per call
/// is O(calls × total quads) and dominated the compile-stage wall time. Iteration
/// order is unchanged — the frozen quad table and every permutation run are sorted
/// on the same id axes, so for a fixed bound prefix the remaining axes iterate
/// ascending exactly as the sequential SPOG scan does (the projection bytes stay
/// deterministic and identical).
pub(crate) fn value(ds: &RdfDataset, subject: &Subject, predicate: &Iri) -> Option<Node> {
    let s_id = subject_id(ds, subject)?;
    let p_id = predicate_id(ds, predicate)?;
    ds.quads_for_pattern(Some(s_id), Some(p_id), None, GraphMatch::Default)
        .next()
        .map(|q| node_of(ds, q.o))
}

/// All objects of `(subject, predicate, *)` in the default graph.
pub(crate) fn objects(ds: &RdfDataset, subject: &Subject, predicate: &Iri) -> Vec<Node> {
    let (Some(s_id), Some(p_id)) = (subject_id(ds, subject), predicate_id(ds, predicate)) else {
        return Vec::new();
    };
    ds.quads_for_pattern(Some(s_id), Some(p_id), None, GraphMatch::Default)
        .map(|q| node_of(ds, q.o))
        .collect()
}

/// All subjects of `(*, predicate, object)` in the default graph.
pub(crate) fn subjects_with(ds: &RdfDataset, predicate: &Iri, object: &Node) -> Vec<Subject> {
    let (Some(p_id), Some(o_id)) = (predicate_id(ds, predicate), object_id(ds, object)) else {
        return Vec::new();
    };
    ds.quads_for_pattern(None, Some(p_id), Some(o_id), GraphMatch::Default)
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
    ds.quads_for_pattern(Some(s_id), Some(p_id), Some(o_id), GraphMatch::Default)
        .next()
        .is_some()
}

/// Whether any triple in the default graph has predicate `predicate`.
pub(crate) fn has_predicate(ds: &RdfDataset, predicate: &Iri) -> bool {
    let Some(p_id) = predicate_id(ds, predicate) else {
        return false;
    };
    ds.quads_for_pattern(None, Some(p_id), None, GraphMatch::Default)
        .next()
        .is_some()
}

/// Whether any triple in the default graph has predicate `predicate` and object `object`.
pub(crate) fn has_predicate_object(ds: &RdfDataset, predicate: &Iri, object: &Node) -> bool {
    let (Some(p_id), Some(o_id)) = (predicate_id(ds, predicate), object_id(ds, object)) else {
        return false;
    };
    ds.quads_for_pattern(None, Some(p_id), Some(o_id), GraphMatch::Default)
        .next()
        .is_some()
}

// --------------------------------------------------------------------------- //
// Content-addressed hashing
// --------------------------------------------------------------------------- //

/// First 12 hex chars of SHA-256 of `s` — the content-stable digest used to mint
/// deterministic IRIs (reifier keys, covering/union class nodes, and restriction
/// skolem nodes) so a projection is byte-identical across regenerate runs.
///
/// Shared by the projections (`projections::rdf`) and the restriction skolemizer
/// (`restriction`); both must mint the SAME id from the SAME content key.
pub(crate) fn sha256_12(s: &str) -> String {
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(s.as_bytes());
    let mut out = String::with_capacity(12);
    for b in digest.iter().take(6) {
        out.push_str(&format!("{b:02x}"));
    }
    out
}
