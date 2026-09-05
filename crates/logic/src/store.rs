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
}

impl WorldStore {
    /// Create a new, empty in-memory `WorldStore`.
    pub fn new() -> Self {
        Self {
            inner: RefCell::new(MutableDataset::new(Arc::new(RdfDataset::union(&[])))),
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

    /// Load a frozen RDF dataset into the world-indexed store, preserving named graphs.
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
        for quad in source.quads() {
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

    /// Return the distinct world IRIs (named graph IRIs) present in the store.
    pub fn worlds(&self) -> Vec<String> {
        let inner = self.inner.borrow();
        let all = inner.quads_for_pattern(None, None, None, GraphMatchValue::Any);
        let mut seen = std::collections::BTreeSet::new();
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

#[cfg(test)]
mod tests {
    use super::*;

    const WORLD_A: &str = "http://world/A";
    const WORLD_B: &str = "http://world/B";

    const S_A: &str = "http://example.org/s/a";
    const P_A: &str = "http://example.org/p/a";
    const O_A: &str = "http://example.org/o/a";

    const S_B: &str = "http://example.org/s/b";
    const P_B: &str = "http://example.org/p/b";
    const O_B: &str = "http://example.org/o/b";

    fn populated_store() -> WorldStore {
        let store = WorldStore::new();
        store.insert_quad(WORLD_A, S_A, P_A, O_A);
        store.insert_quad(WORLD_B, S_B, P_B, O_B);
        store
    }

    #[test]
    fn load_dataset_preserves_named_graph_worlds() {
        use purrdf::{RdfDatasetBuilder, RdfQuad, RdfTerm};

        let quad =
            RdfQuad::new(RdfTerm::iri(S_A), P_A, RdfTerm::iri(O_A)).in_graph(RdfTerm::iri(WORLD_A));
        let mut builder = RdfDatasetBuilder::new();
        builder.push_owned_quad(&quad);
        let source = builder.freeze().expect("valid dataset");
        let store = WorldStore::new();
        store
            .load_dataset(source.as_ref())
            .expect("RDF dataset should load into LOGIC");

        assert_eq!(store.quads_in_world(WORLD_A).len(), 1);
        assert!(store.quads_in_world(WORLD_B).is_empty());
    }

    /// Build a single-quad frozen dataset placing `(s, p, o)` in graph `world`.
    fn single_quad_dataset(world: &str, s: &str, p: &str, o: &str) -> std::sync::Arc<RdfDataset> {
        use purrdf::{RdfDatasetBuilder, RdfQuad, RdfTerm};
        let quad = RdfQuad::new(RdfTerm::iri(s), p, RdfTerm::iri(o)).in_graph(RdfTerm::iri(world));
        let mut builder = RdfDatasetBuilder::new();
        builder.push_owned_quad(&quad);
        builder.freeze().expect("valid dataset")
    }

    #[test]
    fn from_dataset_folds_arc_dataset_repo_root_free() {
        // The Arc-ergonomic constructor a runtime consumer uses on its own dataset.
        let arc = single_quad_dataset(WORLD_A, S_A, P_A, O_A);
        let store = WorldStore::from_dataset(&arc).expect("Arc<RdfDataset> should fold");
        assert_eq!(store.worlds(), vec![WORLD_A]);
        assert_eq!(store.quads_in_world(WORLD_A).len(), 1);
    }

    #[test]
    fn from_dataset_then_insert_quad_is_additive() {
        // Refresh shape 1: additive append on top of the folded base.
        let arc = single_quad_dataset(WORLD_A, S_A, P_A, O_A);
        let store = WorldStore::from_dataset(&arc).expect("fold base");
        store.insert_quad(WORLD_B, S_B, P_B, O_B);
        let mut worlds = store.worlds();
        worlds.sort();
        assert_eq!(worlds, vec![WORLD_A, WORLD_B], "append must not reset");
        assert_eq!(store.quads_in_world(WORLD_A).len(), 1);
        assert_eq!(store.quads_in_world(WORLD_B).len(), 1);
    }

    #[test]
    fn from_dataset_fresh_construct_is_wholesale_replace() {
        // Refresh shape 2: a fresh store from a re-folded dataset carries ONLY the
        // new fold's worlds — no in-place mutation, no double-count.
        let first = single_quad_dataset(WORLD_A, S_A, P_A, O_A);
        let store = WorldStore::from_dataset(&first).expect("fold first");
        assert_eq!(store.worlds(), vec![WORLD_A]);

        let second = single_quad_dataset(WORLD_B, S_B, P_B, O_B);
        let replaced = WorldStore::from_dataset(&second).expect("fold second");
        assert_eq!(
            replaced.worlds(),
            vec![WORLD_B],
            "wholesale replace carries only the re-folded world"
        );
        assert!(replaced.quads_in_world(WORLD_A).is_empty());
    }

    #[test]
    fn world_a_contains_its_own_quad() {
        let store = populated_store();
        let quads = store.quads_in_world(WORLD_A);
        assert_eq!(quads.len(), 1, "world A should have exactly 1 quad");
        let q = &quads[0];
        assert!(
            q[0].contains("s/a"),
            "subject should be A's subject, got {q:?}"
        );
    }

    #[test]
    fn world_b_contains_its_own_quad() {
        let store = populated_store();
        let quads = store.quads_in_world(WORLD_B);
        assert_eq!(quads.len(), 1, "world B should have exactly 1 quad");
        let q = &quads[0];
        assert!(
            q[0].contains("s/b"),
            "subject should be B's subject, got {q:?}"
        );
    }

    #[test]
    fn no_cross_world_leakage_a_to_b() {
        let store = populated_store();
        let a_quads = store.quads_in_world(WORLD_A);
        // none of world A's quads should appear in world B
        for q in &a_quads {
            assert!(
                !q[0].contains("s/b"),
                "world A contains B's subject — cross-world leak: {q:?}"
            );
        }
        // world B should not see A's triple
        let b_quads = store.quads_in_world(WORLD_B);
        for q in &b_quads {
            assert!(
                !q[0].contains("s/a"),
                "world B contains A's subject — cross-world leak: {q:?}"
            );
        }
    }

    #[test]
    fn worlds_lists_both_world_iris() {
        let store = populated_store();
        let mut worlds = store.worlds();
        worlds.sort();
        assert_eq!(worlds, vec![WORLD_A, WORLD_B]);
    }

    #[test]
    fn empty_store_has_no_worlds() {
        let store = WorldStore::new();
        assert!(store.worlds().is_empty());
    }

    #[test]
    fn quads_in_nonexistent_world_returns_empty() {
        let store = populated_store();
        let quads = store.quads_in_world("http://world/doesNotExist");
        assert!(quads.is_empty());
    }

    #[test]
    fn quad_world_column_matches_world_iri() {
        let store = populated_store();
        for q in store.quads_in_world(WORLD_A) {
            assert_eq!(q[3], WORLD_A, "fourth column must be the world IRI");
        }
        for q in store.quads_in_world(WORLD_B) {
            assert_eq!(q[3], WORLD_B, "fourth column must be the world IRI");
        }
    }

    // ── quads_for_pattern_in_world ────────────────────────────────────────────

    #[test]
    fn pattern_all_none_returns_all_quads_in_world() {
        let store = populated_store();
        let quads = store.quads_for_pattern_in_world(WORLD_A, None, None, None);
        assert_eq!(quads.len(), 1, "world A has exactly 1 quad");
        assert_eq!(
            quads[0].s.as_iri(),
            Some(S_A),
            "subject must be world A's subject"
        );
    }

    #[test]
    fn pattern_subject_filter_returns_match() {
        let store = populated_store();
        // Filter by the correct subject — should return the one quad.
        let quads = store.quads_for_pattern_in_world(WORLD_A, Some(S_A), None, None);
        assert_eq!(quads.len(), 1);
        // Filter by a wrong subject — should return empty.
        let quads_miss = store.quads_for_pattern_in_world(WORLD_A, Some(S_B), None, None);
        assert!(
            quads_miss.is_empty(),
            "wrong subject should return no results"
        );
    }

    #[test]
    fn pattern_predicate_filter_returns_match() {
        let store = populated_store();
        let quads = store.quads_for_pattern_in_world(WORLD_A, None, Some(P_A), None);
        assert_eq!(quads.len(), 1);
        let quads_miss = store.quads_for_pattern_in_world(WORLD_A, None, Some(P_B), None);
        assert!(quads_miss.is_empty());
    }

    #[test]
    fn pattern_nonexistent_world_returns_empty() {
        let store = populated_store();
        let quads = store.quads_for_pattern_in_world("http://world/doesNotExist", None, None, None);
        assert!(quads.is_empty());
    }

    #[test]
    fn pattern_invalid_world_iri_returns_empty() {
        let store = populated_store();
        let quads = store.quads_for_pattern_in_world("not a valid IRI", None, None, None);
        assert!(quads.is_empty());
    }

    #[test]
    fn pattern_no_cross_world_leak() {
        let store = populated_store();
        // World A's pattern should NOT see world B's quads.
        let quads_a = store.quads_for_pattern_in_world(WORLD_A, None, None, None);
        for q in &quads_a {
            assert!(
                !q.s.as_iri().unwrap_or_default().contains("s/b"),
                "world A pattern returned world B's quad: {q:?}"
            );
        }
        // World B's pattern should NOT see world A's quads.
        let quads_b = store.quads_for_pattern_in_world(WORLD_B, None, None, None);
        for q in &quads_b {
            assert!(
                !q.s.as_iri().unwrap_or_default().contains("s/a"),
                "world B pattern returned world A's quad: {q:?}"
            );
        }
    }

    // ── select (SPARQL SELECT helper) ─────────────────────────────────────────

    #[test]
    fn select_returns_canonical_bindings() {
        let store = populated_store();
        // Query only world A's objects for a known subject and predicate.
        let sparql = format!("SELECT ?o WHERE {{ GRAPH <{WORLD_A}> {{ <{S_A}> <{P_A}> ?o }} }}");
        let rows = store.select(&sparql).expect("select must succeed");
        assert_eq!(rows.len(), 1, "exactly one match expected: {rows:?}");
        let canonical_o = &rows[0]["o"];
        assert_eq!(
            canonical_o,
            &format!("<{O_A}>"),
            "canonical form must be <iri>: {canonical_o:?}"
        );
    }

    #[test]
    fn select_no_cross_world_results() {
        let store = populated_store();
        // Query world A but for world B's triple — should return nothing.
        let sparql = format!("SELECT ?o WHERE {{ GRAPH <{WORLD_A}> {{ <{S_B}> <{P_B}> ?o }} }}");
        let rows = store.select(&sparql).expect("select must succeed");
        assert!(rows.is_empty(), "no cross-world results expected: {rows:?}");
    }

    #[test]
    fn select_parse_error_returns_err() {
        let store = WorldStore::new();
        let result = store.select("NOT VALID SPARQL AT ALL");
        assert!(result.is_err(), "invalid SPARQL must return Err");
    }
}
