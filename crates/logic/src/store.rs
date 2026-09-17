// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! World-indexed named-graph store.
//!
//! World-indexed semantics only: no dataset-union queries are provided.
//! Each world is isolated in its own named graph. The `WorldStore` wraps a
//! native [`MutableDataset`] (oxigraph-free) and routes every insert and query
//! through the named-graph IRI that identifies the world. Named graphs are
//! first-class in the dataset IR, so a world is exactly the dataset's named
//! graph whose graph term is that IRI.

use std::cell::RefCell;
use std::sync::Arc;

use purrdf::{
    DatasetMut, GraphMatchValue, MutableDataset, QuadValues, RdfDataset, TermValue, parse_dataset,
};

/// Wrap a world-store condition message as a typed diagnostic on the shared
/// substrate, preserving the authored text verbatim.
fn store_err(detail: String) -> gmeow_errors::Diag {
    gmeow_errors::Diag::of_kind(crate::error::Store { detail })
}

/// A world-indexed RDF store.
///
/// Each world is a named graph identified by an IRI string. Only world-indexed
/// (named-graph–scoped) operations are exposed; no cross-graph union queries
/// exist by design. This is the core isolation guarantee: a triple inserted into
/// world A is never visible through a query on world B.
///
/// The backing [`MutableDataset`] is wrapped in a [`RefCell`] so the historic
/// `&self` insert/query API is preserved without threading any `&mut` through the
/// reasoning call graph. The store is never shared across threads while mutated
/// (the multi-world chase reads facts out first, then parallelises over those).
pub struct WorldStore {
    inner: RefCell<MutableDataset>,
    /// Explicit execution scopes survive a selective source probe returning no rows.
    execution_worlds: RefCell<std::collections::BTreeSet<String>>,
}

impl WorldStore {
    /// Create a new, empty in-memory `WorldStore`.
    pub fn new() -> Self {
        Self {
            inner: RefCell::new(MutableDataset::new(Arc::new(RdfDataset::union(&[])))),
            execution_worlds: RefCell::default(),
        }
    }

    /// Construct a `WorldStore` folded from a caller-supplied frozen dataset,
    /// preserving named graphs as worlds.
    ///
    /// This is the supported entry for a runtime consumer that owns its own
    /// `Arc<RdfDataset>` — folded from its own source (e.g. a signed ledger), not
    /// from a repo checkout. `Arc<RdfDataset>` callers pass `&*arc` (or `&arc`,
    /// which derefs), so this one constructor serves both `&RdfDataset` and
    /// `Arc<RdfDataset>`, and stays signature-stable when a paged dataset backend
    /// lands behind the same [`RdfDataset`] type.
    ///
    /// Refresh has two shapes:
    /// * **additive** — call [`load_dataset`](Self::load_dataset),
    ///   [`insert_quad`](Self::insert_quad), or
    ///   [`insert_quad_terms`](Self::insert_quad_terms) again; every insert is a
    ///   delta, never a reset;
    /// * **wholesale replace** — construct a *fresh* store from the re-folded
    ///   dataset and drop the prior one. There is no in-place `clear`: replacement
    ///   is a new value, so a re-folded source never double-counts.
    ///
    /// # Errors
    ///
    /// Propagates any fold error from [`load_dataset`](Self::load_dataset).
    pub fn from_dataset(source: &RdfDataset) -> gmeow_errors::Result<Self> {
        let store = Self::new();
        store.load_dataset(source)?;
        Ok(store)
    }

    /// Load N-Quads text into the store, preserving named graphs (worlds).
    ///
    /// Each quad's graph component becomes its world. The default graph and
    /// blank-node graphs are folded as-is but are not addressable as worlds via the
    /// world-indexed API. The N-Quads text is parsed through the native codec
    /// (`parse_dataset`) into the frozen `RdfDataset` IR, then routed into the
    /// world-indexed store via [`load_dataset`](Self::load_dataset) — the same
    /// text-free IR → store path the GTS-backed EDB takes, so both sources fold
    /// identically (no codec drift).
    ///
    /// # Errors
    ///
    /// Returns `Err(String)` if the N-Quads text is malformed.
    pub fn load_nquads(&self, nquads: &str) -> gmeow_errors::Result<()> {
        let dataset = parse_dataset(nquads.as_bytes(), "application/n-quads", None)
            .map_err(|e| store_err(format!("N-Quads parse error: {e}")))?;
        self.load_dataset(dataset.as_ref())
    }

    /// Load a frozen RDF dataset into the world-indexed store, preserving named graphs
    /// and the complete RDF 1.2 reifier/annotation surface as world-scoped facts.
    ///
    /// GTS-backed sources and future sidecar-aware inputs all cross into LOGIC as
    /// the concrete `RdfDataset` IR. Named graphs are retained as worlds;
    /// default-graph quads are loaded but remain inaccessible through the
    /// world-only APIs by design.
    ///
    /// This method **appends**: each call inserts the source's quads as deltas on
    /// top of whatever the store already holds — it is not an idempotent reset.
    /// Calling it again with more quads is the additive refresh path. To replace
    /// the contents wholesale, construct a fresh store via
    /// [`from_dataset`](Self::from_dataset) and drop the prior one.
    ///
    /// # Errors
    ///
    /// A quad naming a relative IRI. The insert validates that every IRI is
    /// absolute, so a scheme-less reference is refused here rather than becoming
    /// an unresolvable term inside the store.
    pub fn load_dataset(&self, source: &RdfDataset) -> gmeow_errors::Result<()> {
        let mut inner = self.inner.borrow_mut();
        for quad in source
            .quads()
            .chain(purrdf::DatasetView::reifier_quads(source))
            .chain(purrdf::DatasetView::annotation_quads(source))
        {
            inner
                .insert(QuadValues {
                    s: source.term_value(quad.s),
                    p: source.term_value(quad.p),
                    o: source.term_value(quad.o),
                    g: quad.g.map(|g| source.term_value(g)),
                })
                .map_err(|e| store_err(format!("load_dataset: {e}")))?;
        }
        Ok(())
    }

    /// Insert the triple `(s, p, o)` — all IRI strings — into the named graph
    /// whose IRI is `world`.
    ///
    /// Appends a delta: repeated calls accumulate, they do not reset the store.
    ///
    /// # Panics
    ///
    /// If any of `world`/`s`/`p`/`o` is not an absolute IRI. Every argument is
    /// declared to be an IRI by this function's own signature, so a relative one
    /// is a caller bug, not a runtime condition — and a store that silently
    /// accepted it would hold a term nothing can resolve. Callers holding an IRI
    /// that might be relative must resolve it against a base first.
    pub fn insert_quad(&self, world: &str, s: &str, p: &str, o: &str) {
        self.inner
            .borrow_mut()
            .insert(QuadValues {
                s: TermValue::iri(s),
                p: TermValue::iri(p),
                o: TermValue::iri(o),
                g: Some(TermValue::iri(world)),
            })
            .expect("insert_quad requires absolute IRIs for world/subject/predicate/object");
    }

    /// Insert an already-materialized RDF triple into the named graph `world`.
    ///
    /// This is the term-preserving companion to [`Self::insert_quad`]. It is used
    /// by snapshot-style transitions that must copy existing RDF terms, including
    /// literal objects, without round-tripping through string-only IRI helpers.
    ///
    /// # Errors
    ///
    /// A relative IRI among the supplied terms or in `world`. The insert
    /// validates absoluteness, so a scheme-less reference is refused here rather
    /// than becoming an unresolvable term inside the store.
    pub fn insert_quad_terms(
        &self,
        world: &str,
        subject: TermValue,
        predicate: TermValue,
        object: TermValue,
    ) -> gmeow_errors::Result<()> {
        self.inner
            .borrow_mut()
            .insert(QuadValues {
                s: subject,
                p: predicate,
                o: object,
                g: Some(TermValue::iri(world)),
            })
            .map_err(|e| store_err(format!("insert_quad_terms: {e}")))?;
        Ok(())
    }

    /// Return all quads in the named graph `world`, in unspecified order.
    ///
    /// Returns `Vec<[String; 4]>` where each element is
    /// `[subject_n3, predicate_n3, object_n3, world_iri]`. Components are rendered
    /// in N3/Turtle term form (IRIs as `<iri>`, literals as `"lex"^^<dt>`), matching
    /// the prior oxigraph `Term::to_string()` rendering. Only the quads stored under
    /// that exact named graph are returned; no cross-world union is performed.
    pub fn quads_in_world(&self, world: &str) -> Vec<[String; 4]> {
        self.pattern(world, None, None, None)
            .into_iter()
            .map(|q| {
                [
                    crate::provenance::term_display(&q.s),
                    crate::provenance::term_display(&q.p),
                    crate::provenance::term_display(&q.o),
                    q.g.as_ref()
                        .and_then(|g| g.as_iri())
                        .unwrap_or("")
                        .to_owned(),
                ]
            })
            .collect()
    }

    /// Return the [`QuadValues`] in `world` matching the optional `(s, p, o)` IRI pattern.
    ///
    /// Each of `s`, `p`, `o` is an optional IRI string filter:
    /// - `Some(iri)` — restrict to quads where that component equals the IRI.
    /// - `None` — no restriction on that component.
    ///
    /// Queries are scoped exclusively to the named graph `world`; no cross-world
    /// union is performed (world-indexed only).
    ///
    /// Used by the SPARQL fast path and the facts-as-DB snapshot in the seam layer.
    pub fn quads_for_pattern_in_world(
        &self,
        world: &str,
        s: Option<&str>,
        p: Option<&str>,
        o: Option<&str>,
    ) -> Vec<QuadValues> {
        self.pattern(world, s, p, o)
    }

    /// Internal: resolve a world+pattern to the matching value-quads. The pattern
    /// components are IRI filters (the prior oxigraph path only ever bound IRI
    /// positions, never literals).
    fn pattern(
        &self,
        world: &str,
        s: Option<&str>,
        p: Option<&str>,
        o: Option<&str>,
    ) -> Vec<QuadValues> {
        let sv = s.map(TermValue::iri);
        let pv = p.map(TermValue::iri);
        let ov = o.map(TermValue::iri);
        let gv = TermValue::iri(world);
        self.inner.borrow().quads_for_pattern(
            sv.as_ref(),
            pv.as_ref(),
            ov.as_ref(),
            GraphMatchValue::Named(&gv),
        )
    }

    /// Run a SPARQL SELECT query over the world-indexed store.
    ///
    /// Returns each solution as a map of variable-name → canonical term string,
    /// where IRIs are `<iri>` and literals are n3 — matching
    /// [`crate::provenance::term_n3`] and the oracle's `Const` form.
    ///
    /// World-scoping is the caller's responsibility (include `GRAPH <world> { … }` in
    /// the query). Only SELECT queries are supported; returns `Err` for ASK/CONSTRUCT
    /// results or for any evaluation error.
    ///
    /// # Errors
    ///
    /// Returns `Err(String)` on SPARQL parse/evaluation errors, on a non-SELECT result
    /// type, or if `term_n3` fails on an RDF-star term.
    pub fn select(
        &self,
        sparql: &str,
    ) -> gmeow_errors::Result<Vec<std::collections::BTreeMap<String, String>>> {
        use purrdf::sparql::NativeSparqlEngine;
        use purrdf::{SparqlEngine, SparqlRequest, SparqlResult};

        let dataset = self
            .inner
            .borrow()
            .freeze()
            .map_err(|e| store_err(format!("freeze failed in select: {e}")))?;

        let engine = NativeSparqlEngine::new();
        let result = engine
            .query(
                &dataset,
                SparqlRequest {
                    query: sparql,
                    base_iri: None,
                    substitutions: &[],
                },
            )
            .map_err(|e| store_err(format!("SPARQL evaluation error: {e}")))?;

        match result {
            SparqlResult::Solutions {
                variables, rows, ..
            } => {
                let mut out = Vec::new();
                for row in rows {
                    let mut bindings = std::collections::BTreeMap::new();
                    for (var, cell) in variables.iter().zip(row.iter()) {
                        if let Some(term) = cell {
                            let canonical = crate::provenance::term_n3(term)
                                .map_err(|e| store_err(format!("term_n3 failed in select: {e}")))?;
                            bindings.insert(var.clone(), canonical);
                        }
                    }
                    out.push(bindings);
                }
                Ok(out)
            }
            SparqlResult::Boolean(_) | SparqlResult::Graph(_) => Err(store_err(
                "select() requires a SPARQL SELECT query; got ASK or CONSTRUCT/DESCRIBE".to_owned(),
            )),
        }
    }

    /// Retain a caller-selected world even when its admitted fact extension is empty.
    /// This is execution scope, not an asserted RDF statement or a fabricated fact.
    pub(crate) fn select_world(&self, world: &str) -> gmeow_errors::Result<()> {
        purrdf::iri::BaseIri::parse(world)
            .map_err(|error| store_err(format!("selected world: {error}")))?;
        self.execution_worlds.borrow_mut().insert(world.to_owned());
        Ok(())
    }

    /// Return named world IRIs with facts or an explicit execution selection.
    pub fn worlds(&self) -> Vec<String> {
        let inner = self.inner.borrow();
        let all = inner.quads_for_pattern(None, None, None, GraphMatchValue::Any);
        let mut seen = self.execution_worlds.borrow().clone();
        for q in all {
            if let Some(iri) = q.g.as_ref().and_then(|g| g.as_iri()) {
                seen.insert(iri.to_owned());
            }
        }
        seen.into_iter().collect()
    }
}

impl Default for WorldStore {
    fn default() -> Self {
        Self::new()
    }
}

#[path = "store.tests.rs"]
#[cfg(test)]
mod tests;
