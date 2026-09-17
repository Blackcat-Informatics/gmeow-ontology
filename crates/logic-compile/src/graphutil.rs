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
use purrdf::{BlankScope, QuadIds, RdfDataset, TermId, TermRef, TermValue, canonical_relabel};
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
    Blank { label: String, scope: BlankScope },
    Lit(purrdf::RdfLiteral),
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
        Node::Lit(purrdf::RdfLiteral {
            lexical_form: lexical.into(),
            datatype: None,
            language: None,
            direction: None,
        })
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
        Node::Lit(purrdf::RdfLiteral {
            lexical_form: lexical,
            ..
        }) => lexical.clone(),
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

/// Preserve the native object kind; a domain literal is never a rule variable.
pub(crate) fn atomic_object(node: &Node) -> gmeow_errors::Result<crate::ir::AtomicTerm> {
    use crate::ir::AtomicTerm;
    match node {
        Node::Iri(value) => Ok(AtomicTerm::Iri(value.clone())),
        Node::Blank { label, .. } => Ok(AtomicTerm::Blank(label.clone())),
        Node::Lit(value) => Ok(AtomicTerm::Literal(value.clone())),
        Node::Triple(_) => Err(gmeow_errors::Diag::of_kind(crate::error::Frontend {
            detail: "a nested proposition requires typed formula lowering".to_owned(),
        })),
    }
}

/// Whether a term is a literal (rdflib `isinstance(o, Literal)`).
pub(crate) fn term_is_literal(term: &Node) -> bool {
    matches!(term, Node::Lit(purrdf::RdfLiteral { .. }))
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
pub(crate) fn iri_of<D: DatasetView + ?Sized>(ds: &D, id: D::Id) -> Iri {
    match ds.resolve(id) {
        TermRef::Iri(s) => Iri(s.to_owned()),
        // A predicate is always an IRI; the remaining cases are unreachable for a
        // well-formed dataset. Render them losslessly rather than panic (the
        // never-panic fuzz gate must hold for any parsed input).
        other => Iri(render_term(ds, other)),
    }
}

/// Resolve a subject position to the pure [`Subject`] model.
pub(crate) fn subject_of<D: DatasetView + ?Sized>(ds: &D, id: D::Id) -> Subject {
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
pub(crate) fn node_of<D: DatasetView + ?Sized>(ds: &D, id: D::Id) -> Node {
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
            direction,
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
            Node::Lit(purrdf::RdfLiteral {
                lexical_form: lexical.to_owned(),
                datatype,
                language: lang,
                direction,
            })
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
fn render_term<D: DatasetView + ?Sized>(ds: &D, term: TermRef<'_, D::Id>) -> String {
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
pub(crate) fn subject_id<D: DatasetView + ?Sized>(ds: &D, subject: &Subject) -> Option<D::Id> {
    let value = match subject {
        Subject::Iri(iri) => return ds.term_id_by_value(&TermValue::iri(iri)),
        Subject::Blank { label, scope } => TermValue::Blank {
            label: label.clone(),
            scope: *scope,
        },
    };
    ds.term_id_by_value(&value)
}

/// Intern a predicate IRI to its dataset [`TermId`].
fn predicate_id<D: DatasetView + ?Sized>(ds: &D, predicate: &Iri) -> Option<D::Id> {
    ds.term_id_by_value(&TermValue::iri(predicate.as_str()))
}

/// Intern an object term to its dataset [`TermId`]. Only IRI/blank objects are
/// interned as query keys here — the compiler only ever matches on IRI objects
/// (`rdf:type` class terms); a literal/triple object key cannot be reconstructed
/// without datatype/language and never occurs as a query key, so it yields `None`.
fn object_id<D: DatasetView + ?Sized>(ds: &D, object: &Node) -> Option<D::Id> {
    let value = match object {
        Node::Iri(iri) => return ds.term_id_by_value(&TermValue::iri(iri)),
        Node::Blank { label, scope } => TermValue::Blank {
            label: label.clone(),
            scope: *scope,
        },
        Node::Lit(purrdf::RdfLiteral { .. }) | Node::Triple(_) => return None,
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
/// Apply PurRDF's native RDFC-1.0 label mapping directly to the typed dataset.
/// This preserves the full RDF 1.2 statement layer and removes the former canonical
/// N-Quads allocation and immediate parser round trip. Canonicalization refusals remain
/// hard compiler errors; there is no weaker labeling fallback.
pub(crate) fn canonicalize_blank_nodes(ds: &RdfDataset) -> gmeow_errors::Result<Arc<RdfDataset>> {
    canonical_relabel(ds).map(Arc::new).map_err(|e| {
        Diag::of_kind(crate::error::Graph {
            detail: format!("native blank-node canonicalization: {e}"),
        })
    })
}

// --------------------------------------------------------------------------- //
// Default-graph queries
// --------------------------------------------------------------------------- //

/// Native default-graph statements across the ordinary and RDF 1.2 tables.
///
/// Ordinary rows retain PurRDF's indexed lookup. Subject-bound side-table probes
/// use its sorted runs; unbound probes stream the side tables. Duplicate physical
/// carriers are removed by exact indexed membership, without a growing seen set,
/// another dataset, or RDF text. Reifier rows expose the binding only: the quoted
/// triple remains an object and is never asserted by this read boundary.
pub fn default_graph_pattern(
    ds: &RdfDataset,
    s: Option<TermId>,
    p: Option<TermId>,
    o: Option<TermId>,
) -> impl Iterator<Item = QuadIds> + '_ {
    source_graph_pattern(ds, s, p, o, GraphMatch::Default)
}

/// Native assertions from the explicitly selected graph set. All physical
/// carriers retain their original graph and exact term identity; quoted triples
/// are never promoted into assertions. Duplicate carriers collapse only within
/// their identical source graph.
pub fn source_graph_pattern<D: DatasetView + ?Sized>(
    ds: &D,
    s: Option<D::Id>,
    p: Option<D::Id>,
    o: Option<D::Id>,
    graph: GraphMatch<D::Id>,
) -> impl Iterator<Item = QuadIds<D::Id>> + '_ {
    let reifies = ds.term_id_by_value(&TermValue::iri(RDF_REIFIES));
    let in_graph = move |g: Option<D::Id>| match graph {
        GraphMatch::Default => g.is_none(),
        GraphMatch::Named(selected) => g == Some(selected),
        GraphMatch::Any => true,
    };
    let matches = move |q: &QuadIds<D::Id>| {
        in_graph(q.g) && p.is_none_or(|p| q.p == p) && o.is_none_or(|o| q.o == o)
    };
    let in_base = move |q: &QuadIds<D::Id>| {
        ds.quads_for_pattern(
            Some(q.s),
            Some(q.p),
            Some(q.o),
            q.g.map_or(GraphMatch::Default, GraphMatch::Named),
        )
        .next()
        .is_some()
    };
    let reifiers = p
        .is_none_or(|p| Some(p) == reifies)
        .then_some(())
        .into_iter()
        .flat_map(move |()| {
            s.into_iter().flat_map(|s| ds.reifier_quads_of(s)).chain(
                s.is_none()
                    .then_some(())
                    .into_iter()
                    .flat_map(|()| ds.reifier_quads()),
            )
        })
        .filter(matches)
        .filter(move |q| !in_base(q));
    let annotations = s
        .into_iter()
        .flat_map(move |s| {
            ds.annotations_of_with_graph(s)
                .map(move |(p, o, g)| QuadIds { s, p, o, g })
        })
        .chain(
            s.is_none()
                .then_some(())
                .into_iter()
                .flat_map(|()| ds.annotation_quads()),
        )
        .filter(matches)
        .filter(move |q| {
            !in_base(q)
                && !(Some(q.p) == reifies
                    && ds.reifier_quads_of(q.s).any(|r| r.o == q.o && r.g == q.g))
        });
    ds.quads_for_pattern(s, p, o, graph)
        .chain(reifiers)
        .chain(annotations)
}

/// Stream default-graph statements, resolving only the current row. Callers do
/// not materialize an owned copy of the entire source on every compiler pass.
pub(crate) fn default_graph_quads(ds: &RdfDataset) -> impl Iterator<Item = Quad> + '_ {
    default_graph_quads_with_ids(ds).map(|(_, quad)| quad)
}

/// Resolve the current row while retaining its exact native source occurrence.
pub(crate) fn default_graph_quads_with_ids(
    ds: &RdfDataset,
) -> impl Iterator<Item = (QuadIds, Quad)> + '_ {
    default_graph_pattern(ds, None, None, None).map(|q| {
        (
            q,
            Quad {
                subject: subject_of(ds, q.s),
                predicate: iri_of(ds, q.p),
                object: node_of(ds, q.o),
            },
        )
    })
}

/// Whether the selected default graph has no ordinary or native statement rows.
pub(crate) fn is_empty(ds: &RdfDataset) -> bool {
    default_graph_pattern(ds, None, None, None).next().is_none()
}

/// `graph.value(subject, predicate)` — the first object of
/// `(subject, predicate, *)` in the default graph, or `None`.
///
/// All compiler lookups use the same native statement boundary. Ordinary-table
/// order is preserved; native-only bindings and annotations follow in their
/// frozen order. No scope is inferred from a named graph or a quoted term.
pub(crate) fn value(ds: &RdfDataset, subject: &Subject, predicate: &Iri) -> Option<Node> {
    let s_id = subject_id(ds, subject)?;
    let p_id = predicate_id(ds, predicate)?;
    default_graph_pattern(ds, Some(s_id), Some(p_id), None)
        .next()
        .map(|q| node_of(ds, q.o))
}

/// All objects of `(subject, predicate, *)` in the default graph.
pub(crate) fn objects(ds: &RdfDataset, subject: &Subject, predicate: &Iri) -> Vec<Node> {
    objects_in_graph(ds, subject, predicate, GraphMatch::Default)
}

/// Objects in one selected source graph, preserving all native assertion tables.
pub(crate) fn objects_in_graph<D: DatasetView + ?Sized>(
    ds: &D,
    subject: &Subject,
    predicate: &Iri,
    graph: GraphMatch<D::Id>,
) -> Vec<Node> {
    let (Some(s_id), Some(p_id)) = (subject_id(ds, subject), predicate_id(ds, predicate)) else {
        return Vec::new();
    };
    source_graph_pattern(ds, Some(s_id), Some(p_id), None, graph)
        .map(|q| node_of(ds, q.o))
        .collect()
}

/// All subjects of `(*, predicate, object)` in the default graph.
pub(crate) fn subjects_with(ds: &RdfDataset, predicate: &Iri, object: &Node) -> Vec<Subject> {
    let (Some(p_id), Some(o_id)) = (predicate_id(ds, predicate), object_id(ds, object)) else {
        return Vec::new();
    };
    default_graph_pattern(ds, None, Some(p_id), Some(o_id))
        .map(|q| subject_of(ds, q.s))
        .collect()
}

/// The admitted structural typing predicates for a compiler record's class.
/// Canonical logic classes admit native instance typing; external RDF grammar
/// classes retain their exact RDF typing. No relation is inferred or rewritten.
pub(crate) fn is_structural_type_predicate(predicate: &str, class: &str) -> bool {
    predicate == RDF_TYPE
        || (predicate == "https://blackcatinformatics.ca/logic/instanceOf"
            && class.starts_with(crate::ir::LOGIC_NAMESPACE))
}

/// Resolve the admitted predicates to this dataset's native term identities.
fn structural_type_predicates(ds: &RdfDataset, class: &Node) -> [Option<TermId>; 2] {
    let native = matches!(class, Node::Iri(iri) if is_structural_type_predicate(
        "https://blackcatinformatics.ca/logic/instanceOf", iri
    ));
    [
        predicate_id(ds, &nn(RDF_TYPE)),
        native
            .then(|| predicate_id(ds, &nn("https://blackcatinformatics.ca/logic/instanceOf")))
            .flatten(),
    ]
}

/// Discover structural records once across their admitted typing surfaces.
/// Keep RDF-only iteration order, deduplicating by native scoped term identity.
pub(crate) fn subjects_of_structural_class(ds: &RdfDataset, class: &Node) -> Vec<Subject> {
    let Some(class_id) = object_id(ds, class) else {
        return Vec::new();
    };
    let mut seen = std::collections::HashSet::new();
    structural_type_predicates(ds, class)
        .into_iter()
        .flatten()
        .flat_map(|predicate| default_graph_pattern(ds, None, Some(predicate), Some(class_id)))
        .filter(|quad| seen.insert(quad.s))
        .map(|quad| subject_of(ds, quad.s))
        .collect()
}

/// Test a compiler record's structural class without changing domain typing.
pub(crate) fn has_structural_class(ds: &RdfDataset, subject: &Subject, class: &Node) -> bool {
    let (Some(subject), Some(class_id)) = (subject_id(ds, subject), object_id(ds, class)) else {
        return false;
    };
    structural_type_predicates(ds, class)
        .into_iter()
        .flatten()
        .any(|predicate| {
            default_graph_pattern(ds, Some(subject), Some(predicate), Some(class_id))
                .next()
                .is_some()
        })
}

/// Read a compiler record's declared structural classes. Native instance typing
/// contributes canonical logic classes only; arbitrary domain sorts are not RDF types.
pub(crate) fn structural_classes(ds: &RdfDataset, subject: &Subject) -> Vec<Node> {
    let Some(subject) = subject_id(ds, subject) else {
        return Vec::new();
    };
    let mut seen = std::collections::HashSet::new();
    [RDF_TYPE, "https://blackcatinformatics.ca/logic/instanceOf"]
        .into_iter()
        .filter_map(|predicate| predicate_id(ds, &nn(predicate)).map(|id| (predicate, id)))
        .flat_map(|(predicate, id)| {
            default_graph_pattern(ds, Some(subject), Some(id), None).filter(move |quad| {
                matches!(ds.resolve(quad.o), TermRef::Iri(iri)
                        if is_structural_type_predicate(predicate, iri))
            })
        })
        .filter(|quad| seen.insert(quad.o))
        .map(|quad| node_of(ds, quad.o))
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
    default_graph_pattern(ds, Some(s_id), Some(p_id), Some(o_id))
        .next()
        .is_some()
}

/// Whether any triple in the default graph has predicate `predicate`.
pub(crate) fn has_predicate(ds: &RdfDataset, predicate: &Iri) -> bool {
    let Some(p_id) = predicate_id(ds, predicate) else {
        return false;
    };
    default_graph_pattern(ds, None, Some(p_id), None)
        .next()
        .is_some()
}

/// Whether any triple in the default graph has predicate `predicate` and object `object`.
pub(crate) fn has_predicate_object(ds: &RdfDataset, predicate: &Iri, object: &Node) -> bool {
    let (Some(p_id), Some(o_id)) = (predicate_id(ds, predicate), object_id(ds, object)) else {
        return false;
    };
    default_graph_pattern(ds, None, Some(p_id), Some(o_id))
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
