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

use gmeow_rdf_core::{RdfDataset, RdfDiagnostic, SparqlEngine, SparqlRequest, SparqlResult};
use gmeow_sparql_algebra::{Query, SparqlParser};

use crate::eval::{evaluate_query, EvalCtx, Outcome};
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
#[derive(Debug, Default)]
pub struct NativeSparqlEngine {
    cache: RefCell<PlanCache>,
}

impl NativeSparqlEngine {
    /// A fresh engine with an empty plan cache.
    pub fn new() -> Self {
        Self::default()
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
        _dataset: &mut Self::Dataset,
        _request: SparqlRequest<'_>,
    ) -> Result<(), RdfDiagnostic> {
        // SPARQL UPDATE is out of S6 scope (the read query path only).
        Err(RdfDiagnostic::error(
            "native-sparql-update-unsupported",
            "SPARQL UPDATE is not implemented in the native engine (S6 scope)",
        ))
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

    #[test]
    fn update_is_unsupported() {
        let engine = NativeSparqlEngine::new();
        let mut ds = social();
        let err = engine
            .update(
                &mut ds,
                SparqlRequest {
                    query: "INSERT DATA { <http://ex/a> <http://ex/p> <http://ex/o> }",
                    base_iri: None,
                },
            )
            .unwrap_err();
        assert_eq!(err.code, "native-sparql-update-unsupported");
    }
}
