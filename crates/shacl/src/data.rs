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
use std::sync::OnceLock;

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
///
/// The `Send + Sync` bound readies the seam for parallel focus-node validation
/// (a shared `&G` read concurrently across rayon threads). It is non-breaking:
/// every backend is already thread-safe — `Store` is `Send + Sync`, and the IR
/// backends (`&RdfDataset`, `CachedIrDataGraph`) hold only `Sync` frozen data
/// plus a `OnceLock<Store>`. The engine itself currently validates SERIALLY: a
/// rayon path over the focus loop regressed ~9% on `shacl_validate/large_hierarchy`
/// because per-focus work (~5 µs) is too cheap to amortize thread dispatch and
/// shared-`Store` contention. It re-enters once per-focus cost exceeds ~50–100 µs
/// (common SHACL-SPARQL constraints, or the IR-native backend run end-to-end).
/// See #828 (item 2).
pub trait ShaclDataGraph: Send + Sync {
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

// ── MergedGraph backend (read-only base ∪ overlay, the parallel example path) ───

/// A non-mutating [`ShaclDataGraph`] over a shared read-only base [`Store`] plus a
/// small per-validation overlay of quads.
///
/// It reads `base ∪ overlay` WITHOUT mutating the base, so a single `&Store` can be
/// shared across rayon threads and validated against many overlays in parallel. This
/// replaces the historical "insert the overlay into the shared store, validate, then
/// remove it" path, whose mutation forced every per-example validation to run
/// serially on one store.
///
/// The overlay is filtered at construction to exactly mirror the old
/// `scoped_overlay_insert` semantics: only quads NOT already present in the base
/// contribute, so a quad shared by base and overlay is counted once (cardinality
/// constraints stay correct). Pattern lookups append matching overlay quads to the
/// base's results with no per-lookup deduplication, because the retained overlay is
/// disjoint from the base by construction.
///
/// The SPARQL-store escape hatch ([`ShaclDataGraph::sparql_store`]) materializes an
/// INDEPENDENT `base ∪ overlay` store lazily, at most once per validation. The
/// oxigraph in-memory `Store` is `Clone`, but cloning shares the same `Arc<Content>`
/// (mutating a clone mutates the original), so the materialized store is built fresh
/// rather than cloned — keeping the shared base immutable under concurrency.
pub struct MergedGraph<'a> {
    base: &'a Store,
    /// Overlay quads not already present in `base` (mirrors `scoped_overlay_insert`).
    effective_overlay: Vec<Quad>,
    sparql_store: OnceLock<Store>,
}

impl<'a> MergedGraph<'a> {
    /// Build a merged view of `base ∪ overlay`, retaining only overlay quads that are
    /// not already in `base`.
    pub fn new(base: &'a Store, overlay: &[Quad]) -> Self {
        let effective_overlay = overlay
            .iter()
            .filter(|quad| {
                !base
                    .contains(*quad)
                    .expect("contains on an in-memory store is infallible")
            })
            .cloned()
            .collect();
        Self {
            base,
            effective_overlay,
            sparql_store: OnceLock::new(),
        }
    }

    /// Does `quad` match the `(subject?, predicate?, object?, graph)` pattern? A
    /// `None` slot is a wildcard. Mirrors the term-equality semantics of the
    /// [`Store`] backend's `quads_for_pattern`.
    fn overlay_quad_matches(
        quad: &Quad,
        subject: Option<&Term>,
        predicate: Option<&Term>,
        object: Option<&Term>,
        graph: GraphFilter,
    ) -> bool {
        if let GraphFilter::DefaultGraph = graph {
            if quad.graph_name != GraphName::DefaultGraph {
                return false;
            }
        }
        if let Some(want) = subject {
            let matches = match (want, &quad.subject) {
                (Term::NamedNode(w), NamedOrBlankNode::NamedNode(s)) => w == s,
                (Term::BlankNode(w), NamedOrBlankNode::BlankNode(s)) => w == s,
                // A literal/triple subject pattern cannot match a quad subject.
                _ => false,
            };
            if !matches {
                return false;
            }
        }
        if let Some(want) = predicate {
            match want {
                Term::NamedNode(w) if w == &quad.predicate => {}
                _ => return false,
            }
        }
        if let Some(want) = object {
            if want != &quad.object {
                return false;
            }
        }
        true
    }
}

impl ShaclDataGraph for MergedGraph<'_> {
    fn quads_for_pattern(
        &self,
        subject: Option<&Term>,
        predicate: Option<&Term>,
        object: Option<&Term>,
        graph: GraphFilter,
    ) -> Vec<Quad> {
        let mut out = <Store as ShaclDataGraph>::quads_for_pattern(
            self.base, subject, predicate, object, graph,
        );
        for quad in &self.effective_overlay {
            if Self::overlay_quad_matches(quad, subject, predicate, object, graph) {
                out.push(quad.clone());
            }
        }
        out
    }

    fn sparql_store(&self) -> Cow<'_, Store> {
        // Materialize an INDEPENDENT base ∪ overlay store ONCE per validation (only if
        // a SHACL-SPARQL target/constraint is actually reached). Built fresh rather
        // than cloned: an in-memory `Store` clone shares its `Arc<Content>`, so an
        // insert into a clone would mutate the shared base under concurrency.
        let store = self.sparql_store.get_or_init(|| {
            let merged = Store::new().expect("in-memory store creation is infallible");
            for quad in self.base.iter() {
                let quad = quad.expect("reading a base quad from an in-memory store is infallible");
                merged
                    .insert(&quad)
                    .expect("inserting into an in-memory store is infallible");
            }
            for quad in &self.effective_overlay {
                merged
                    .insert(quad)
                    .expect("inserting an overlay quad is infallible");
            }
            merged
        });
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

#[cfg(test)]
mod merged_graph_tests {
    use super::*;
    use crate::engine::{parse_shapes, validate, validate_with};

    /// Build an in-memory oxigraph store from a Turtle string (flattened to the
    /// default graph, matching the engine's data policy).
    fn store_from_ttl(ttl: &str) -> Store {
        let dataset = gmeow_rdf::parse_dataset(ttl.as_bytes(), "text/turtle", None).unwrap();
        gmeow_rdf::oxigraph::store_from_dataset(
            &dataset,
            gmeow_rdf::oxigraph::GraphPolicy::FlattenToDefaultGraph,
        )
        .unwrap()
    }

    /// A comparable, order-independent projection of a report's results.
    fn result_keys(report: &crate::report::ValidationReport) -> Vec<String> {
        let mut keys: Vec<String> = report
            .results
            .iter()
            .map(|r| {
                format!(
                    "{}|{}|{}|{}|{}|{:?}",
                    r.focus_node,
                    r.result_path
                        .as_ref()
                        .map(|t| t.to_string())
                        .unwrap_or_default(),
                    r.value.as_ref().map(|t| t.to_string()).unwrap_or_default(),
                    r.source_constraint_component,
                    r.source_shape,
                    r.severity,
                )
            })
            .collect();
        keys.sort();
        keys
    }

    /// `MergedGraph` over `base` + `overlay` must produce the SAME report as
    /// physically loading `base ∪ overlay` into one store and validating it — the
    /// exact equivalence the old "insert overlay into the shared store" path gave,
    /// now without mutation. The fixture deliberately shares one triple between base
    /// and overlay so the dedup branch (cardinality-sensitive) is exercised.
    #[test]
    fn merged_graph_matches_overlay_into_store() {
        const BASE: &str = r#"
            @prefix ex: <http://example.org/> .
            ex:a a ex:Thing ;
                 ex:p ex:x .
        "#;
        // The overlay re-asserts the SHARED `ex:a ex:p ex:x` (must NOT be double
        // counted) and adds a fresh conforming node `ex:b`.
        const OVERLAY: &str = r#"
            @prefix ex: <http://example.org/> .
            ex:a ex:p ex:x .
            ex:b a ex:Thing ;
                 ex:p ex:y .
        "#;
        // `ex:p` is sh:maxCount 1: if the shared triple were counted twice for ex:a,
        // MergedGraph would report a maxCount violation the merged store does not.
        const SHAPES: &str = r#"
            @prefix sh: <http://www.w3.org/ns/shacl#> .
            @prefix ex: <http://example.org/> .
            ex:ThingShape a sh:NodeShape ;
                sh:targetClass ex:Thing ;
                sh:property [ sh:path ex:p ; sh:maxCount 1 ] .
        "#;

        let base = store_from_ttl(BASE);
        let overlay_quads: Vec<Quad> = store_from_ttl(OVERLAY)
            .iter()
            .map(|q| q.expect("read overlay quad"))
            .collect();
        let shapes = parse_shapes(SHAPES).unwrap();

        // Path A: physically merged store (each quad present once).
        let combined = store_from_ttl(&format!("{BASE}\n{OVERLAY}"));
        let report_a = validate(&combined, &shapes);

        // Path B: non-mutating MergedGraph view.
        let merged = MergedGraph::new(&base, &overlay_quads);
        let report_b = validate_with(&merged, &shapes);

        assert_eq!(
            report_a.conforms, report_b.conforms,
            "conformance must match (both should conform: each Thing has exactly one ex:p)"
        );
        assert!(
            report_a.conforms,
            "fixture should conform when dedup is correct"
        );
        assert_eq!(
            result_keys(&report_a),
            result_keys(&report_b),
            "MergedGraph results must equal the physically-merged-store results"
        );
    }

    /// The retained overlay must drop quads already in the base (the
    /// `scoped_overlay_insert` "not already present" semantics).
    #[test]
    fn merged_graph_drops_overlay_quads_already_in_base() {
        const BASE: &str = r#"
            @prefix ex: <http://example.org/> .
            ex:a ex:p ex:x .
        "#;
        const OVERLAY: &str = r#"
            @prefix ex: <http://example.org/> .
            ex:a ex:p ex:x .
            ex:a ex:p ex:z .
        "#;
        let base = store_from_ttl(BASE);
        let overlay_quads: Vec<Quad> = store_from_ttl(OVERLAY)
            .iter()
            .map(|q| q.expect("read overlay quad"))
            .collect();
        let merged = MergedGraph::new(&base, &overlay_quads);
        // Only `ex:a ex:p ex:z` is new; `ex:a ex:p ex:x` already lives in the base.
        assert_eq!(
            merged.effective_overlay.len(),
            1,
            "the quad shared with the base must be filtered out of the overlay"
        );
    }
}
