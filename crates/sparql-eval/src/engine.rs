// SPDX-FileCopyrightText: 2026 Blackcat Informatics Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The native [`SparqlEngine`] implementation and its parse-memoizing plan cache.
//!
//! [`NativeSparqlEngine`] is the single required impl of the `gmeow-rdf-core`
//! `SparqlEngine` seam (#887) — the native replacement for the oxigraph-family
//! `spareval` on the query path. Its `Dataset` is the concrete frozen
//! [`RdfDataset`]: the evaluator needs `term_id_by_value` (P4 #838), which is an
//! inherent method on the dataset rather than part of the `DatasetView` trait.
//!
//! The [`PlanCache`] memoizes parsing so the static `sparql_emit`-generated query
//! set compiles to algebra once, not per run. Full cost-based planning is out of S6
//! scope (S7b #929); the cache holds only the parsed [`Query`].

use std::cell::RefCell;
use std::sync::Arc;

use gmeow_rdf_core::{
    MutableDataset, RdfDataset, RdfDiagnostic, SparqlEngine, SparqlRequest, SparqlResult,
};
use gmeow_sparql_algebra::{Query, SparqlParser};

use crate::eval::{evaluate_query, EvalCtx, Outcome};
use crate::update::{eval_update, GraphResolver};
use crate::DetHashMap;

/// A parsed, ready-to-evaluate query (the cached unit of the [`PlanCache`]).
#[derive(Debug)]
pub struct PreparedQuery {
    /// The parsed algebra.
    pub query: Query,
}

/// A parse-memoizing cache keyed on `(base IRI, query text)`.
#[derive(Debug, Default)]
pub struct PlanCache {
    entries: DetHashMap<String, Arc<PreparedQuery>>,
}

impl PlanCache {
    /// A fresh, empty cache.
    pub fn new() -> Self {
        Self::default()
    }

    /// Parse `query` (memoized) into a [`PreparedQuery`].
    pub fn prepare(
        &mut self,
        query: &str,
        base_iri: Option<&str>,
    ) -> Result<Arc<PreparedQuery>, RdfDiagnostic> {
        // The cache key must include the base IRI: the same text under a different
        // base parses to different IRIs.
        let key = format!("{}\u{0}{}", base_iri.unwrap_or(""), query);
        if let Some(prepared) = self.entries.get(&key) {
            return Ok(prepared.clone());
        }
        let mut parser = SparqlParser::new();
        if let Some(base) = base_iri {
            parser = parser.with_base_iri(base);
        }
        let parsed = parser
            .parse_query(query)
            .map_err(|e| RdfDiagnostic::error("native-sparql-query-parse", e.to_string()))?;
        let prepared = Arc::new(PreparedQuery { query: parsed });
        self.entries.insert(key, prepared.clone());
        Ok(prepared)
    }
}

/// The native, RDF-1.2-first multiset SPARQL engine (purrdf S6, #912).
#[derive(Default)]
pub struct NativeSparqlEngine {
    cache: RefCell<PlanCache>,
    resolver: Option<Arc<dyn GraphResolver>>,
}

// `dyn GraphResolver` is not `Debug`, so derive can't apply; report its presence by
// hand (`Some(..)`/`None`) and keep the cache's own `Debug`.
impl std::fmt::Debug for NativeSparqlEngine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NativeSparqlEngine")
            .field("cache", &self.cache)
            .field(
                "resolver",
                match &self.resolver {
                    Some(_) => &"Some(..)",
                    None => &"None",
                },
            )
            .finish()
    }
}

impl NativeSparqlEngine {
    /// A fresh engine with an empty plan cache and no `LOAD` resolver.
    pub fn new() -> Self {
        Self::default()
    }

    /// Install a host `GraphResolver` so SPARQL `LOAD <iri>` can fetch its source.
    /// Without one, LOAD hard-fails (`native-sparql-load-no-resolver`) unless SILENT.
    pub fn with_resolver(mut self, resolver: Arc<dyn GraphResolver>) -> Self {
        self.resolver = Some(resolver);
        self
    }
}

impl SparqlEngine for NativeSparqlEngine {
    type Dataset = Arc<RdfDataset>;

    fn query(
        &self,
        dataset: &Self::Dataset,
        request: SparqlRequest<'_>,
    ) -> Result<SparqlResult, RdfDiagnostic> {
        let prepared = self
            .cache
            .borrow_mut()
            .prepare(request.query, request.base_iri)?;
        let mut ctx = EvalCtx::new(dataset);
        let outcome = evaluate_query(&prepared.query, &mut ctx)
            .map_err(|e| RdfDiagnostic::error("native-sparql-query-eval", e.to_string()))?;
        Ok(materialize(outcome, &ctx))
    }

    fn update(
        &self,
        dataset: &mut Self::Dataset,
        request: SparqlRequest<'_>,
    ) -> Result<(), RdfDiagnostic> {
        // UPDATE deliberately bypasses the plan cache: these requests are
        // side-effecting and are not the hot static-query set the cache exists for;
        // caching a mutating statement would be a correctness hazard.
        let mut parser = SparqlParser::new();
        if let Some(base) = request.base_iri {
            parser = parser.with_base_iri(base);
        }
        let update = parser
            .parse_update(request.query)
            .map_err(|e| RdfDiagnostic::error("native-sparql-update-parse", e.to_string()))?;
        // Atomicity is structural: branch a COW MutableDataset off the frozen base,
        // apply every op to the delta, and only on FULL success freeze back. Any
        // error drops `m` and leaves `*dataset` untouched.
        let mut m = MutableDataset::new(Arc::clone(dataset));
        eval_update(&update, &mut m, self.resolver.as_deref())?;
        *dataset = m.freeze()?;
        Ok(())
    }
}

/// Materialize an evaluation [`Outcome`] into the dataset-independent
/// `SparqlResult` egress model (the interned-id space ends here: every solution
/// cell becomes an owned [`TermValue`](gmeow_rdf_core::TermValue)).
fn materialize(outcome: Outcome, ctx: &EvalCtx<'_>) -> SparqlResult {
    match outcome {
        Outcome::Solutions(seq) => {
            let variables = seq
                .schema
                .vars()
                .iter()
                .map(|v| v.as_str().to_owned())
                .collect();
            let rows = seq
                .rows
                .iter()
                .map(|row| {
                    row.iter()
                        .map(|cell| cell.map(|t| ctx.scratch.value_of(ctx.dataset, t)))
                        .collect()
                })
                .collect();
            SparqlResult::Solutions { variables, rows }
        }
        Outcome::Graph(graph) => SparqlResult::Graph(graph),
        Outcome::Boolean(value) => SparqlResult::Boolean(value),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gmeow_rdf_core::{RdfDatasetBuilder, RdfLiteral, TermValue};

    fn social() -> Arc<RdfDataset> {
        // :a :knows :b ; :a :name "Ann" .
        let mut b = RdfDatasetBuilder::new();
        let knows = b.intern_iri("http://ex/knows".to_owned());
        let name = b.intern_iri("http://ex/name".to_owned());
        let a = b.intern_iri("http://ex/a".to_owned());
        let bb = b.intern_iri("http://ex/b".to_owned());
        let ann = b.intern_literal(RdfLiteral::simple("Ann"));
        b.push_quad(a, knows, bb, None);
        b.push_quad(a, name, ann, None);
        b.freeze().expect("freeze")
    }

    fn run(query: &str) -> SparqlResult {
        let ds = social();
        let engine = NativeSparqlEngine::new();
        engine
            .query(
                &ds,
                SparqlRequest {
                    query,
                    base_iri: None,
                },
            )
            .expect("query")
    }

    #[test]
    fn select_returns_solutions() {
        let result = run("SELECT ?o WHERE { <http://ex/a> <http://ex/knows> ?o }");
        match result {
            SparqlResult::Solutions { variables, rows } => {
                assert_eq!(variables, vec!["o".to_owned()]);
                assert_eq!(rows.len(), 1);
                assert_eq!(rows[0][0], Some(TermValue::Iri("http://ex/b".to_owned())));
            }
            other => panic!("expected solutions, got {other:?}"),
        }
    }

    /// A dataset with a default graph plus two named graphs that share a triple.
    ///   default: (a,p,dflt)
    ///   ex:g1:   (a,p,x), (a,p,shared)
    ///   ex:g2:   (a,p,y), (a,p,shared)
    fn multigraph() -> Arc<RdfDataset> {
        let mut b = RdfDatasetBuilder::new();
        let p = b.intern_iri("http://ex/p".to_owned());
        let a = b.intern_iri("http://ex/a".to_owned());
        let dflt = b.intern_iri("http://ex/dflt".to_owned());
        let x = b.intern_iri("http://ex/x".to_owned());
        let y = b.intern_iri("http://ex/y".to_owned());
        let shared = b.intern_iri("http://ex/shared".to_owned());
        let g1 = b.intern_iri("http://ex/g1".to_owned());
        let g2 = b.intern_iri("http://ex/g2".to_owned());
        b.push_quad(a, p, dflt, None);
        b.push_quad(a, p, x, Some(g1));
        b.push_quad(a, p, shared, Some(g1));
        b.push_quad(a, p, y, Some(g2));
        b.push_quad(a, p, shared, Some(g2));
        b.freeze().expect("freeze")
    }

    fn run_on(ds: &Arc<RdfDataset>, query: &str) -> SparqlResult {
        NativeSparqlEngine::new()
            .query(
                ds,
                SparqlRequest {
                    query,
                    base_iri: None,
                },
            )
            .expect("query")
    }

    /// The first-column values of a solutions result, as sorted debug strings.
    fn col0(result: SparqlResult) -> Vec<String> {
        match result {
            SparqlResult::Solutions { rows, .. } => {
                let mut v: Vec<String> = rows.iter().map(|r| format!("{:?}", r[0])).collect();
                v.sort();
                v
            }
            other => panic!("expected solutions, got {other:?}"),
        }
    }

    #[test]
    fn from_merges_named_graphs_into_default_excluding_store_default() {
        let ds = multigraph();
        // FROM g1 FROM g2 → active default = RDF-merge(g1, g2); the store default graph
        // (dflt) is excluded; the shared triple is unioned to a single solution.
        let got = col0(run_on(
            &ds,
            "SELECT ?o FROM <http://ex/g1> FROM <http://ex/g2> \
             WHERE { <http://ex/a> <http://ex/p> ?o }",
        ));
        assert_eq!(got.len(), 3, "x, y, shared (deduped), NOT dflt: {got:?}");
        assert!(
            !got.iter().any(|s| s.contains("dflt")),
            "store default excluded: {got:?}"
        );
        assert_eq!(
            got.iter().filter(|s| s.contains("shared")).count(),
            1,
            "RDF-merge unions the shared triple to one solution"
        );
    }

    #[test]
    fn no_from_clause_uses_store_default_graph() {
        let ds = multigraph();
        let got = col0(run_on(
            &ds,
            "SELECT ?o WHERE { <http://ex/a> <http://ex/p> ?o }",
        ));
        assert_eq!(got.len(), 1, "only the store default graph: {got:?}");
        assert!(got[0].contains("dflt"));
    }

    #[test]
    fn from_named_restricts_graph_var() {
        let ds = multigraph();
        // FROM NAMED g1 → GRAPH ?g binds only to g1 (g2 not addressable); the default
        // graph is empty (no plain FROM).
        let got = col0(run_on(
            &ds,
            "SELECT ?g FROM NAMED <http://ex/g1> \
             WHERE { GRAPH ?g { <http://ex/a> <http://ex/p> ?o } }",
        ));
        assert!(!got.is_empty(), "g1 IS addressable");
        assert!(got.iter().all(|s| s.contains("g1")), "only g1: {got:?}");
        assert!(
            !got.iter().any(|s| s.contains("g2")),
            "g2 not in FROM NAMED"
        );
    }

    #[test]
    fn from_nonexistent_graph_is_empty_not_error() {
        let ds = multigraph();
        let got = col0(run_on(
            &ds,
            "SELECT ?o FROM <http://ex/absent> WHERE { <http://ex/a> <http://ex/p> ?o }",
        ));
        assert!(
            got.is_empty(),
            "absent FROM graph → empty default → no rows"
        );
    }

    #[test]
    fn ask_returns_boolean() {
        let yes = run("ASK { <http://ex/a> <http://ex/knows> <http://ex/b> }");
        assert!(matches!(yes, SparqlResult::Boolean(true)));
        let no = run("ASK { <http://ex/a> <http://ex/knows> <http://ex/nobody> }");
        assert!(matches!(no, SparqlResult::Boolean(false)));
    }

    #[test]
    fn construct_returns_graph() {
        let result =
            run("CONSTRUCT { ?s <http://ex/related> ?o } WHERE { ?s <http://ex/knows> ?o }");
        match result {
            SparqlResult::Graph(g) => assert_eq!(g.quad_count(), 1),
            other => panic!("expected graph, got {other:?}"),
        }
    }

    #[test]
    fn plan_cache_memoizes_parse() {
        let mut cache = PlanCache::new();
        let q = "SELECT ?x WHERE { ?x ?p ?o }";
        let a = cache.prepare(q, None).expect("first");
        let b = cache.prepare(q, None).expect("second");
        // Same text → the same cached Arc.
        assert!(Arc::ptr_eq(&a, &b));
    }

    #[test]
    fn parse_error_becomes_diagnostic() {
        let ds = social();
        let engine = NativeSparqlEngine::new();
        let err = engine
            .query(
                &ds,
                SparqlRequest {
                    query: "this is not sparql",
                    base_iri: None,
                },
            )
            .unwrap_err();
        assert_eq!(err.code, "native-sparql-query-parse");
    }

    // ── UPDATE seam (engine end-to-end) ────────────────────────────────────────

    /// A test-only resolver returning a fixed one-quad dataset for any LOAD source.
    struct TestResolver {
        ds: Arc<RdfDataset>,
    }
    impl GraphResolver for TestResolver {
        fn resolve(&self, _iri: &str) -> Result<Arc<RdfDataset>, RdfDiagnostic> {
            Ok(self.ds.clone())
        }
    }

    fn loadable() -> Arc<RdfDataset> {
        // :loaded :p "v" .
        let mut b = RdfDatasetBuilder::new();
        let s = b.intern_iri("http://ex/loaded".to_owned());
        let p = b.intern_iri("http://ex/p".to_owned());
        let o = b.intern_literal(RdfLiteral::simple("v"));
        b.push_quad(s, p, o, None);
        b.freeze().expect("freeze loadable")
    }

    /// An empty default-graph dataset.
    fn empty() -> Arc<RdfDataset> {
        RdfDatasetBuilder::new().freeze().expect("freeze empty")
    }

    fn update(engine: &NativeSparqlEngine, ds: &mut Arc<RdfDataset>, query: &str) {
        engine
            .update(
                ds,
                SparqlRequest {
                    query,
                    base_iri: None,
                },
            )
            .expect("update applies");
    }

    /// The effective quads as a comparable set of value tuples.
    fn quad_set(ds: &RdfDataset) -> std::collections::BTreeSet<String> {
        ds.quads()
            .map(|q| {
                format!(
                    "{:?}|{:?}|{:?}|{:?}",
                    ds.resolve(q.s),
                    ds.resolve(q.p),
                    ds.resolve(q.o),
                    q.g.map(|g| format!("{:?}", ds.resolve(g)))
                )
            })
            .collect()
    }

    #[test]
    fn insert_data_adds_quad() {
        let engine = NativeSparqlEngine::new();
        let mut ds = empty();
        update(
            &engine,
            &mut ds,
            "INSERT DATA { <http://ex/a> <http://ex/p> <http://ex/b> }",
        );
        assert_eq!(ds.quad_count(), 1);
        assert!(ds
            .term_id_by_value(&TermValue::Iri("http://ex/a".to_owned()))
            .is_some());
    }

    #[test]
    fn delete_data_removes_quad() {
        let engine = NativeSparqlEngine::new();
        let mut ds = social();
        update(
            &engine,
            &mut ds,
            "DELETE DATA { <http://ex/a> <http://ex/knows> <http://ex/b> }",
        );
        // The :knows quad is gone; the :name quad survives.
        assert_eq!(ds.quad_count(), 1);
        assert!(ds
            .term_id_by_value(&TermValue::Iri("http://ex/knows".to_owned()))
            .is_none());
    }

    #[test]
    fn delete_insert_where_rewrites() {
        let engine = NativeSparqlEngine::new();
        let mut ds = social();
        update(
            &engine,
            &mut ds,
            "DELETE { ?s <http://ex/knows> ?o } INSERT { ?s <http://ex/met> ?o } \
             WHERE { ?s <http://ex/knows> ?o }",
        );
        // :knows replaced by :met; :name untouched.
        assert!(ds
            .term_id_by_value(&TermValue::Iri("http://ex/knows".to_owned()))
            .is_none());
        assert!(ds
            .term_id_by_value(&TermValue::Iri("http://ex/met".to_owned()))
            .is_some());
        assert_eq!(ds.quad_count(), 2);
    }

    #[test]
    fn clear_default_empties_target() {
        let engine = NativeSparqlEngine::new();
        let mut ds = social();
        update(&engine, &mut ds, "CLEAR DEFAULT");
        assert_eq!(ds.quad_count(), 0);
    }

    #[test]
    fn load_with_resolver_inserts_resolved_quads() {
        let engine =
            NativeSparqlEngine::new().with_resolver(Arc::new(TestResolver { ds: loadable() }));
        let mut ds = empty();
        update(&engine, &mut ds, "LOAD <http://ex/doc>");
        assert_eq!(ds.quad_count(), 1);
        assert!(ds
            .term_id_by_value(&TermValue::Iri("http://ex/loaded".to_owned()))
            .is_some());
    }

    #[test]
    fn load_without_resolver_is_a_hard_error() {
        let engine = NativeSparqlEngine::new();
        let mut ds = empty();
        let err = engine
            .update(
                &mut ds,
                SparqlRequest {
                    query: "LOAD <http://ex/doc>",
                    base_iri: None,
                },
            )
            .unwrap_err();
        assert_eq!(err.code, "native-sparql-load-no-resolver");
    }

    #[test]
    fn load_silent_without_resolver_is_a_noop_ok() {
        let engine = NativeSparqlEngine::new();
        let mut ds = social();
        let before = ds.quad_count();
        update(&engine, &mut ds, "LOAD SILENT <http://ex/doc>");
        assert_eq!(ds.quad_count(), before, "silent load no-ops");
    }

    #[test]
    fn update_is_atomic_on_a_later_op_failure() {
        // A two-operation request whose FIRST op would insert and whose SECOND op
        // hard-fails (LOAD with no resolver, not SILENT). Branch-then-freeze atomicity
        // requires the whole request to roll back: the dataset must be byte-identical
        // (same quad set) to before, with the first op's INSERT NOT leaked.
        let engine = NativeSparqlEngine::new();
        let mut ds = social();
        let before = quad_set(&ds);

        let err = engine
            .update(
                &mut ds,
                SparqlRequest {
                    query: "INSERT DATA { <http://ex/x> <http://ex/y> <http://ex/z> } ; \
                            LOAD <http://ex/doc>",
                    base_iri: None,
                },
            )
            .unwrap_err();
        assert_eq!(err.code, "native-sparql-load-no-resolver");

        let after = quad_set(&ds);
        assert_eq!(
            after, before,
            "the failed request left the dataset untouched"
        );
    }

    #[test]
    fn update_is_atomic_on_a_where_eval_failure() {
        // A second atomicity proof through a different failure mode: a modify whose
        // WHERE hits an unsupported construct (SERVICE → `native-sparql-update-eval`)
        // after a successful INSERT. The INSERT must not leak.
        let engine = NativeSparqlEngine::new();
        let mut ds = empty();
        let before = quad_set(&ds);

        let err = engine
            .update(
                &mut ds,
                SparqlRequest {
                    query: "INSERT DATA { <http://ex/x> <http://ex/y> <http://ex/z> } ; \
                            DELETE { ?s <http://ex/p> ?o } \
                            WHERE { SERVICE <http://ex/svc> { ?s <http://ex/p> ?o } }",
                    base_iri: None,
                },
            )
            .unwrap_err();
        assert_eq!(err.code, "native-sparql-update-eval");
        assert_eq!(
            quad_set(&ds),
            before,
            "INSERT must not leak past the failure"
        );
    }

    #[test]
    fn update_parse_error_becomes_diagnostic() {
        let engine = NativeSparqlEngine::new();
        let mut ds = empty();
        let err = engine
            .update(
                &mut ds,
                SparqlRequest {
                    query: "this is not an update",
                    base_iri: None,
                },
            )
            .unwrap_err();
        assert_eq!(err.code, "native-sparql-update-parse");
    }

    #[test]
    fn engine_has_no_resolver_by_default() {
        assert!(NativeSparqlEngine::new().resolver.is_none());
        assert!(NativeSparqlEngine::default().resolver.is_none());
    }

    // ── exotic aggregation (S6b #928) ────────────────────────────────────────

    const XSD_INT: &str = "http://www.w3.org/2001/XMLSchema#integer";

    /// A dataset for grouping/aggregation:
    /// `:r1 :a 1 ; :b 2`, `:r2 :a 1 ; :b 2`, `:r3 :a 2 ; :b 3`.
    fn numbers() -> Arc<RdfDataset> {
        let mut b = RdfDatasetBuilder::new();
        let pa = b.intern_iri("http://ex/a".to_owned());
        let pb = b.intern_iri("http://ex/b".to_owned());
        let int = |b: &mut RdfDatasetBuilder, n: &str| {
            b.intern_literal(RdfLiteral::typed(n.to_owned(), XSD_INT.to_owned()))
        };
        for (subj, a, bv) in [("r1", "1", "2"), ("r2", "1", "2"), ("r3", "2", "3")] {
            let s = b.intern_iri(format!("http://ex/{subj}"));
            let av = int(&mut b, a);
            b.push_quad(s, pa, av, None);
            let bvv = int(&mut b, bv);
            b.push_quad(s, pb, bvv, None);
        }
        b.freeze().expect("freeze")
    }

    fn run_on(ds: &Arc<RdfDataset>, query: &str) -> SparqlResult {
        NativeSparqlEngine::new()
            .query(
                ds,
                SparqlRequest {
                    query,
                    base_iri: None,
                },
            )
            .expect("query")
    }

    /// Render a result's rows as a sorted `Vec<Vec<String>>` for stable multiset
    /// comparison (IRIs as `<iri>`, literals as their lexical form).
    fn sorted_rows(result: SparqlResult) -> Vec<Vec<String>> {
        match result {
            SparqlResult::Solutions { rows, .. } => {
                let mut out: Vec<Vec<String>> = rows
                    .iter()
                    .map(|r| r.iter().map(render_cell).collect())
                    .collect();
                out.sort();
                out
            }
            other => panic!("expected solutions, got {other:?}"),
        }
    }

    fn render_cell(cell: &Option<TermValue>) -> String {
        match cell {
            None => "UNBOUND".to_owned(),
            Some(TermValue::Iri(i)) => format!("<{i}>"),
            Some(TermValue::Literal { lexical_form, .. }) => lexical_form.clone(),
            Some(TermValue::Blank { label, .. }) => format!("_:{label}"),
            Some(TermValue::Triple { .. }) => "<<triple>>".to_owned(),
        }
    }

    #[test]
    fn group_by_expression_with_as_binding() {
        // ?a+?b ∈ {3 (×2), 5 (×1)} → two groups counted.
        let r = run_on(
            &numbers(),
            "SELECT ?z (COUNT(*) AS ?c) WHERE { ?r <http://ex/a> ?a . ?r <http://ex/b> ?b } \
             GROUP BY (?a + ?b AS ?z)",
        );
        assert_eq!(sorted_rows(r), vec![vec!["3", "2"], vec!["5", "1"]]);
    }

    #[test]
    fn group_by_expression_without_projecting_the_synthetic_var() {
        // Selecting ONLY the aggregate must not leak the grouping column.
        let r = run_on(
            &numbers(),
            "SELECT (COUNT(*) AS ?c) WHERE { ?r <http://ex/a> ?a . ?r <http://ex/b> ?b } \
             GROUP BY (?a + ?b AS ?z)",
        );
        // Two groups → two count rows, single column each.
        assert_eq!(sorted_rows(r), vec![vec!["1"], vec!["2"]]);
    }

    #[test]
    fn group_by_bare_builtin_expression() {
        // `GROUP BY STR(?a)` (no AS → anonymous key) groups by the string form of
        // ?a ∈ {"1","1","2"} → two groups of sizes 2 and 1. The key is not
        // user-visible, so only the aggregate is projected.
        let r = run_on(
            &numbers(),
            "SELECT (COUNT(*) AS ?c) WHERE { ?r <http://ex/a> ?a } GROUP BY STR(?a)",
        );
        assert_eq!(sorted_rows(r), vec![vec!["1"], vec!["2"]]);
    }

    #[test]
    fn group_concat_with_separator() {
        let r = run_on(
            &numbers(),
            "SELECT (GROUP_CONCAT(?a; SEPARATOR=\"|\") AS ?g) \
             WHERE { ?r <http://ex/a> ?a }",
        );
        // Implicit single group; lexical values of ?a joined by '|', some order.
        match r {
            SparqlResult::Solutions { rows, .. } => {
                assert_eq!(rows.len(), 1);
                let Some(TermValue::Literal { lexical_form, .. }) = &rows[0][0] else {
                    panic!("expected a literal");
                };
                let mut parts: Vec<&str> = lexical_form.split('|').collect();
                parts.sort_unstable();
                assert_eq!(parts, vec!["1", "1", "2"]);
            }
            other => panic!("expected solutions, got {other:?}"),
        }
    }

    #[test]
    fn sample_returns_a_group_member() {
        let r = run_on(
            &numbers(),
            "SELECT (SAMPLE(?a) AS ?s) WHERE { ?r <http://ex/a> ?a }",
        );
        let rows = sorted_rows(r);
        assert_eq!(rows.len(), 1);
        assert!(
            rows[0][0] == "1" || rows[0][0] == "2",
            "got {:?}",
            rows[0][0]
        );
    }

    #[test]
    fn sum_of_expression_inside_aggregate() {
        // SUM(?a + ?b) over the three rows = (1+2)+(1+2)+(2+3) = 11.
        let r = run_on(
            &numbers(),
            "SELECT (SUM(?a + ?b) AS ?t) WHERE { ?r <http://ex/a> ?a . ?r <http://ex/b> ?b }",
        );
        assert_eq!(sorted_rows(r), vec![vec!["11"]]);
    }

    #[test]
    fn arithmetic_across_aggregate_results() {
        // (SUM(?a) / COUNT(?a)) = (1+1+2)/3 — exercises an Extend over two
        // aggregate-result variables. Assert it produces a single bound row.
        let r = run_on(
            &numbers(),
            "SELECT (SUM(?a) AS ?s) (COUNT(?a) AS ?n) ((SUM(?a)/COUNT(?a)) AS ?avg) \
             WHERE { ?r <http://ex/a> ?a }",
        );
        match r {
            SparqlResult::Solutions { rows, .. } => {
                assert_eq!(rows.len(), 1);
                assert_eq!(render_cell(&rows[0][0]), "4"); // SUM = 1+1+2
                assert_eq!(render_cell(&rows[0][1]), "3"); // COUNT = 3
                assert!(rows[0][2].is_some(), "the ratio must be bound");
            }
            other => panic!("expected solutions, got {other:?}"),
        }
    }

    #[test]
    fn having_over_an_aggregate_not_in_select() {
        // Group ?a; keep only groups whose SUM(?b) exceeds 3. Group ?a=1 has
        // rows r1,r2 (?b=2 each) → SUM=4 > 3 (kept); group ?a=2 has r3 (?b=3) →
        // SUM=3, not > 3 (dropped).
        let r = run_on(
            &numbers(),
            "SELECT ?a WHERE { ?r <http://ex/a> ?a . ?r <http://ex/b> ?b } \
             GROUP BY ?a HAVING (SUM(?b) > 3)",
        );
        assert_eq!(sorted_rows(r), vec![vec!["1"]]);
    }

    #[test]
    fn complex_having_conjunction() {
        // COUNT(*) > 1 && AVG(?b) < 5 — only the ?a=1 group (count 2, avg 2).
        let r = run_on(
            &numbers(),
            "SELECT ?a WHERE { ?r <http://ex/a> ?a . ?r <http://ex/b> ?b } \
             GROUP BY ?a HAVING (COUNT(*) > 1 && AVG(?b) < 5)",
        );
        assert_eq!(sorted_rows(r), vec![vec!["1"]]);
    }
}
