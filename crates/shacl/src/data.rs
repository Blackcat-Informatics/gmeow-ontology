// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The SHACL engine's data-access abstraction (#819 C4).
//!
//! [`ShaclDataGraph`] is the single seam through which the SHACL Core engine reads
//! the data graph. The engine, constraint evaluator, and path evaluator are all
//! generic over it, so there is exactly ONE engine implementation parameterized by
//! backend — conformance is identical by construction.
//!
//! Two backends implement the trait:
//!
//! - [`oxigraph::store::Store`] — the historical materialized backend. It is the
//!   differential oracle (`tests/ir_oxigraph_equivalence.rs`) and the SPARQL path.
//! - `&gmeow_rdf::RdfDataset` — the IR-native backend. It answers pattern lookups
//!   directly from the frozen IR's iteration surface
//!   ([`RdfDataset::quad_refs`](gmeow_rdf::RdfDataset::quad_refs)), without
//!   materializing the whole store, converting matched IR terms to oxigraph
//!   [`Term`] values at the boundary so the engine keeps its single oxigraph term
//!   value model.
//!
//! The trait deliberately exposes only pattern lookup plus a SPARQL-store escape
//! hatch; the higher-level helpers (`subjects_of`, `objects_of`,
//! `instances_of_class`, `subclass_closure`) stay free functions in
//! [`crate::engine`], generic over the trait, to minimize churn.

use std::borrow::Cow;

use oxigraph::model::{
    BaseDirection, BlankNode, GraphName, GraphNameRef, Literal, NamedNode, NamedOrBlankNode, Quad,
    Term, Triple,
};
use oxigraph::store::Store;

use gmeow_rdf::ir::{RdfDataset, TermRef};
use gmeow_rdf::{RdfTextDirection, TermId};

/// Which graph(s) a pattern lookup ranges over.
///
/// Mirrors the two graph arguments the engine historically passed to
/// `oxigraph::store::Store::quads_for_pattern`:
///
/// - `None`  ⇒ [`GraphFilter::AnyGraph`] (every graph, named and default);
/// - `Some(GraphNameRef::DefaultGraph)` ⇒ [`GraphFilter::DefaultGraph`].
///
/// The IR backends produced by [`crate::engine::validate_dataset`] flatten all
/// quads into the default graph, so the two filters coincide there; the [`Store`]
/// backend honors the distinction to remain a faithful oracle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GraphFilter {
    /// Match quads in any graph (named or default).
    AnyGraph,
    /// Match quads in the default graph only.
    DefaultGraph,
}

/// The data-access surface the SHACL Core engine reads through.
///
/// Implementations answer triple-pattern lookups, returning oxigraph [`Quad`]
/// values (the engine keeps its existing oxigraph term value model). They also
/// provide a [`Store`] for the SHACL-SPARQL paths, which genuinely require a
/// SPARQL 1.1 query engine.
pub trait ShaclDataGraph {
    /// All quads matching `(subject?, predicate?, object?)` under `graph`.
    ///
    /// A `None` position is a wildcard. Results carry an oxigraph graph name; for
    /// the default-graph filter that is always [`GraphName::DefaultGraph`].
    ///
    /// Returning an owned `Vec<Quad>` (rather than a borrowed iterator) keeps the
    /// trait object-safe-adjacent and dodges the lifetime gymnastics a borrowed
    /// iterator GAT would impose; every historical call site collected anyway.
    fn quads_for_pattern(
        &self,
        subject: Option<&Term>,
        predicate: Option<&Term>,
        object: Option<&Term>,
        graph: GraphFilter,
    ) -> Vec<Quad>;

    /// An oxigraph [`Store`] over the same data, for SHACL-SPARQL evaluation
    /// (`sh:select` targets and `sh:sparql` constraints).
    ///
    /// The [`Store`] backend returns itself by borrow (no copy); the IR backend
    /// lazily materializes a `Store` ONCE. Only the SPARQL paths call this.
    fn sparql_store(&self) -> Cow<'_, Store>;
}

// ── oxigraph::store::Store backend (the differential oracle + SPARQL path) ──────

impl ShaclDataGraph for Store {
    fn quads_for_pattern(
        &self,
        subject: Option<&Term>,
        predicate: Option<&Term>,
        object: Option<&Term>,
        graph: GraphFilter,
    ) -> Vec<Quad> {
        // oxigraph's pattern API wants typed subject/predicate refs; convert the
        // generic `Term` patterns, bailing to "no match" for positions a `Term`
        // cannot legally occupy (a literal subject, a non-IRI predicate).
        let subject_ref = match subject {
            Some(Term::NamedNode(n)) => Some(NamedOrBlankNode::NamedNode(n.clone())),
            Some(Term::BlankNode(b)) => Some(NamedOrBlankNode::BlankNode(b.clone())),
            // A literal/triple cannot be a subject in oxigraph's pattern API.
            Some(_) => return Vec::new(),
            None => None,
        };
        let predicate_node = match predicate {
            Some(Term::NamedNode(n)) => Some(n.clone()),
            Some(_) => return Vec::new(),
            None => None,
        };
        let graph_ref = match graph {
            GraphFilter::AnyGraph => None,
            GraphFilter::DefaultGraph => Some(GraphNameRef::DefaultGraph),
        };
        Store::quads_for_pattern(
            self,
            subject_ref.as_ref().map(NamedOrBlankNode::as_ref),
            predicate_node.as_ref().map(NamedNode::as_ref),
            object.map(Term::as_ref),
            graph_ref,
        )
        .flatten()
        .collect()
    }

    fn sparql_store(&self) -> Cow<'_, Store> {
        Cow::Borrowed(self)
    }
}

// ── &RdfDataset backend (the IR-native path) ───────────────────────────────────

impl ShaclDataGraph for &RdfDataset {
    fn quads_for_pattern(
        &self,
        subject: Option<&Term>,
        predicate: Option<&Term>,
        object: Option<&Term>,
        graph: GraphFilter,
    ) -> Vec<Quad> {
        let mut out = Vec::new();
        for q in self.quad_refs() {
            // Graph filter. A flattened IR has `g == None` (default graph) for every
            // quad, so `DefaultGraph` and `AnyGraph` coincide; we still honor the
            // distinction structurally for any named-graph IR.
            match graph {
                GraphFilter::AnyGraph => {}
                GraphFilter::DefaultGraph => {
                    if q.g.is_some() {
                        continue;
                    }
                }
            }

            let s = term_ref_to_oxigraph(self, q.s);
            let p = term_ref_to_oxigraph(self, q.p);
            let o = term_ref_to_oxigraph(self, q.o);

            // Pattern match: a `None` slot is a wildcard, else must equal.
            if let Some(want) = subject {
                if &s != want {
                    continue;
                }
            }
            if let Some(want) = predicate {
                if &p != want {
                    continue;
                }
            }
            if let Some(want) = object {
                if &o != want {
                    continue;
                }
            }

            let graph_name = match q.g {
                None => GraphName::DefaultGraph,
                Some(g) => match term_ref_to_oxigraph(self, g) {
                    Term::NamedNode(n) => GraphName::NamedNode(n),
                    Term::BlankNode(b) => GraphName::BlankNode(b),
                    // A literal/triple graph name is structurally impossible in a
                    // frozen IR; skip defensively rather than fabricate one.
                    _ => continue,
                },
            };

            // Subject must be a NamedNode/BlankNode; a frozen quad always satisfies
            // this, but guard rather than panic.
            let subject_pos = match s {
                Term::NamedNode(n) => NamedOrBlankNode::NamedNode(n),
                Term::BlankNode(b) => NamedOrBlankNode::BlankNode(b),
                _ => continue,
            };
            let predicate_pos = match p {
                Term::NamedNode(n) => n,
                _ => continue,
            };
            out.push(Quad::new(subject_pos, predicate_pos, o, graph_name));
        }
        out
    }

    fn sparql_store(&self) -> Cow<'_, Store> {
        // SHACL-SPARQL genuinely needs an oxigraph SPARQL engine; the IR cannot
        // answer arbitrary SPARQL itself. Materialize the WHOLE dataset into a Store.
        //
        // NOTE: this bare `&RdfDataset` backend has nowhere to cache the Store, so it
        // re-materializes per call. The engine drives validation through
        // [`CachedIrDataGraph`] (see `validate_dataset`), which materializes ONCE
        // per validation and shares the Store across every SPARQL target/constraint;
        // this impl remains correct for any direct `validate_with(&&RdfDataset)` user.
        let store = materialize_sparql_store(self);
        Cow::Owned(store)
    }
}

/// Materialize a frozen IR dataset into an oxigraph [`Store`] for SHACL-SPARQL,
/// flattening named graphs to the default graph to match the engine's
/// `FlattenToDefaultGraph` data policy. Shared by both IR backends.
fn materialize_sparql_store(dataset: &RdfDataset) -> Store {
    // Concrete-IR entrypoint (#886 part 1): materialize directly from the frozen
    // dataset.
    gmeow_rdf::oxigraph::store_from_dataset(
        dataset,
        gmeow_rdf::oxigraph::GraphPolicy::FlattenToDefaultGraph,
    )
    .expect("IR dataset must materialize into an oxigraph Store for SPARQL")
}

/// An IR-native [`ShaclDataGraph`] that owns a borrow of the frozen dataset PLUS a
/// per-validation cache of the materialized SPARQL [`Store`].
///
/// The SHACL engine asks for [`ShaclDataGraph::sparql_store`] once per `sh:sparql`
/// target/constraint. The bare `&RdfDataset` backend re-materializes the whole store
/// every time; this wrapper materializes it AT MOST ONCE per validation (lazily, only
/// if a SPARQL path is actually reached) via a [`OnceLock`], then hands every later
/// SPARQL evaluation the same store. Pattern lookups delegate to the underlying IR
/// backend unchanged.
pub struct CachedIrDataGraph<'a> {
    dataset: &'a RdfDataset,
    store: std::sync::OnceLock<Store>,
}

impl<'a> CachedIrDataGraph<'a> {
    /// Wrap a borrowed frozen dataset with a lazily-populated SPARQL-store cache.
    pub fn new(dataset: &'a RdfDataset) -> Self {
        Self {
            dataset,
            store: std::sync::OnceLock::new(),
        }
    }
}

impl ShaclDataGraph for CachedIrDataGraph<'_> {
    fn quads_for_pattern(
        &self,
        subject: Option<&Term>,
        predicate: Option<&Term>,
        object: Option<&Term>,
        graph: GraphFilter,
    ) -> Vec<Quad> {
        // Delegate to the IR-native pattern lookup; no materialization.
        ShaclDataGraph::quads_for_pattern(&self.dataset, subject, predicate, object, graph)
    }

    fn sparql_store(&self) -> Cow<'_, Store> {
        // Materialize ONCE per validation; every later SPARQL path reuses it.
        let store = self
            .store
            .get_or_init(|| materialize_sparql_store(self.dataset));
        Cow::Borrowed(store)
    }
}

// ── IR term → oxigraph term conversion ─────────────────────────────────────────

/// Convert a resolved IR [`TermRef`] into an oxigraph [`Term`].
///
/// Mirrors `gmeow_rdf::oxigraph`'s owned-model conversion but goes straight from
/// the borrowed IR view, recursing into triple terms via the dataset's
/// [`resolve`](RdfDataset::resolve). All node constructors are `_unchecked`: a
/// frozen IR has already validated lexical well-formedness at ingest, so re-checking
/// here would only re-reject the private-use language tags the engine deliberately
/// tolerates (see `engine::validate_graphs`).
fn term_ref_to_oxigraph(dataset: &RdfDataset, term: TermRef<'_>) -> Term {
    match term {
        TermRef::Iri(iri) => Term::NamedNode(NamedNode::new_unchecked(iri)),
        TermRef::Blank { label, scope } => {
            // Qualify the label by scope so two same-label blanks from different
            // BlankScopes never conflate in SHACL queries (C0.2); the DEFAULT scope
            // keeps the bare label so real single-scope data is byte-unchanged.
            Term::BlankNode(BlankNode::new_unchecked(scope.qualify_label(label)))
        }
        TermRef::Literal {
            lexical,
            datatype,
            language,
            direction,
        } => Term::Literal(literal_to_oxigraph(
            dataset, lexical, datatype, language, direction,
        )),
        TermRef::Triple { s, p, o } => {
            let subject = match term_ref_to_oxigraph(dataset, dataset.resolve(s)) {
                Term::NamedNode(n) => NamedOrBlankNode::NamedNode(n),
                Term::BlankNode(b) => NamedOrBlankNode::BlankNode(b),
                // A triple subject must be IRI/blank; a frozen triple always is.
                other => unreachable!("triple subject must be IRI/blank, got {other:?}"),
            };
            let predicate = match term_ref_to_oxigraph(dataset, dataset.resolve(p)) {
                Term::NamedNode(n) => n,
                other => unreachable!("triple predicate must be an IRI, got {other:?}"),
            };
            let object = term_ref_to_oxigraph(dataset, dataset.resolve(o));
            Term::Triple(Box::new(Triple::new(subject, predicate, object)))
        }
    }
}

/// Build an oxigraph [`Literal`] from a resolved IR literal view.
///
/// The IR always expands the datatype to an interned IRI (C0.1); resolve it back to
/// its string. Language and direction reproduce oxigraph's typed/directional/plain
/// literal constructors exactly as `gmeow_rdf::oxigraph::literal_from_rdf` does.
fn literal_to_oxigraph(
    dataset: &RdfDataset,
    lexical: &str,
    datatype: TermId,
    language: Option<&str>,
    direction: Option<RdfTextDirection>,
) -> Literal {
    if let Some(language) = language {
        return match direction {
            Some(RdfTextDirection::Ltr) => {
                Literal::new_directional_language_tagged_literal_unchecked(
                    lexical,
                    language,
                    BaseDirection::Ltr,
                )
            }
            Some(RdfTextDirection::Rtl) => {
                Literal::new_directional_language_tagged_literal_unchecked(
                    lexical,
                    language,
                    BaseDirection::Rtl,
                )
            }
            None => Literal::new_language_tagged_literal_unchecked(lexical, language),
        };
    }
    let datatype_iri = match dataset.resolve(datatype) {
        TermRef::Iri(iri) => iri,
        other => unreachable!("a literal datatype must resolve to an IRI, got {other:?}"),
    };
    Literal::new_typed_literal(lexical, NamedNode::new_unchecked(datatype_iri))
}
