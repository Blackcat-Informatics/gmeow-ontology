// SPDX-FileCopyrightText: 2026 Blackcat Informatics Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Differential SPARQL parity: the native `gmeow-sparql-eval` engine (purrdf S6,
//! #912) vs the oxigraph baseline on the SAME data and queries.
//!
//! This test is the acceptance evidence for #912's two load-bearing lines:
//! - **byte-identical CONSTRUCT** output (compared at the RDFC-1.0 canonical
//!   N-Quads layer — `freeze` sorts/dedups and canonicalization relabels blanks),
//!   and
//! - SELECT/ASK results that match oxigraph as a multiset.
//!
//! It lives in `crates/rdf` (which already has oxigraph) as a dev-only diff, so the
//! native engine's own crate stays oxigraph-free (gated by `make rdf-core-hygiene`).
//! Both engines are driven from one IR dataset: oxigraph via a materialized `Store`,
//! the native engine over the `RdfDataset` directly.

#![cfg(feature = "oxigraph")]

use gmeow_rdf::oxigraph::{store_from_dataset, GraphPolicy};
use gmeow_rdf::{
    canonicalize, dataset_from_bytes, NativeRdfFormat, OxigraphBackend, SparqlRequest, SparqlResult,
};
use gmeow_sparql_eval::NativeSparqlEngine;
use oxigraph::store::Store;
use std::sync::Arc;

use gmeow_rdf::RdfDataset;
use gmeow_rdf_core::SparqlEngine;

/// A small but varied dataset exercising IRIs, typed/plain/lang literals, multiple
/// predicates, and a node that is the object of two `:knows` edges.
const DATA: &str = r#"
<http://ex/alice> <http://ex/knows> <http://ex/bob> .
<http://ex/alice> <http://ex/knows> <http://ex/carol> .
<http://ex/bob>   <http://ex/knows> <http://ex/carol> .
<http://ex/alice> <http://ex/name> "Alice" .
<http://ex/alice> <http://ex/age>  "30"^^<http://www.w3.org/2001/XMLSchema#integer> .
<http://ex/bob>   <http://ex/age>  "17"^^<http://www.w3.org/2001/XMLSchema#integer> .
<http://ex/carol> <http://ex/member> <http://ex/club> .
"#;

fn fixtures() -> (Arc<RdfDataset>, Store) {
    let dataset = dataset_from_bytes(DATA.as_bytes(), NativeRdfFormat::NTriples).expect("parse IR");
    let store = store_from_dataset(&dataset, GraphPolicy::PreserveNamedGraphs).expect("store");
    (dataset, store)
}

fn run_both(dataset: &Arc<RdfDataset>, store: &Store, query: &str) -> (SparqlResult, SparqlResult) {
    let request = SparqlRequest {
        query,
        base_iri: None,
    };
    let ox = OxigraphBackend
        .query(store, request)
        .unwrap_or_else(|e| panic!("oxigraph query failed for {query:?}: {e:?}"));
    let native = NativeSparqlEngine::new()
        .query(dataset, request)
        .unwrap_or_else(|e| panic!("native query failed for {query:?}: {e:?}"));
    (ox, native)
}

/// A stable, order-insensitive key for a solution row.
fn row_key(row: &[Option<gmeow_rdf::TermValue>]) -> String {
    format!("{row:?}")
}

/// Assert the two results agree. SELECT solutions are compared as a multiset
/// (sorted), CONSTRUCT graphs at the canonical N-Quads layer, ASK by value.
fn assert_parity(query: &str, ox: &SparqlResult, native: &SparqlResult) {
    match (ox, native) {
        (
            SparqlResult::Solutions {
                variables: ox_vars,
                rows: ox_rows,
            },
            SparqlResult::Solutions {
                variables: nat_vars,
                rows: nat_rows,
            },
        ) => {
            assert_eq!(ox_vars, nat_vars, "{query}: variable list differs");
            let mut ox_sorted: Vec<String> = ox_rows.iter().map(|r| row_key(r)).collect();
            let mut nat_sorted: Vec<String> = nat_rows.iter().map(|r| row_key(r)).collect();
            ox_sorted.sort();
            nat_sorted.sort();
            assert_eq!(ox_sorted, nat_sorted, "{query}: solution multiset differs");
        }
        (SparqlResult::Graph(ox_g), SparqlResult::Graph(nat_g)) => {
            assert_eq!(
                canonicalize(ox_g).nquads,
                canonicalize(nat_g).nquads,
                "{query}: CONSTRUCT canonical N-Quads differ"
            );
        }
        (SparqlResult::Boolean(ox_b), SparqlResult::Boolean(nat_b)) => {
            assert_eq!(ox_b, nat_b, "{query}: ASK boolean differs");
        }
        _ => panic!("{query}: result shape mismatch ({ox:?} vs {native:?})"),
    }
}

/// The representative corpus-shaped query set: BGP joins, FILTER (incl. NOT
/// EXISTS), OPTIONAL, UNION, MINUS, DISTINCT, typed-literal comparison, ASK, and
/// CONSTRUCT (the byte-parity line).
fn parity_queries() -> Vec<&'static str> {
    vec![
        // BGP — single and joined.
        "SELECT ?o WHERE { <http://ex/alice> <http://ex/knows> ?o }",
        "SELECT ?a ?b ?c WHERE { ?a <http://ex/knows> ?b . ?b <http://ex/knows> ?c }",
        // FILTER over a typed literal (value-space comparison).
        "SELECT ?s WHERE { ?s <http://ex/age> ?n FILTER(?n >= 18) }",
        // FILTER NOT EXISTS (the corpus-critical anti-join idiom).
        "SELECT ?s WHERE { ?s <http://ex/knows> ?o FILTER NOT EXISTS { ?s <http://ex/member> ?c } }",
        // OPTIONAL.
        "SELECT ?s ?m WHERE { ?s <http://ex/knows> ?o OPTIONAL { ?s <http://ex/member> ?m } }",
        // UNION.
        "SELECT ?x WHERE { { ?x <http://ex/knows> <http://ex/carol> } UNION { ?x <http://ex/member> <http://ex/club> } }",
        // MINUS.
        "SELECT ?s WHERE { ?s <http://ex/knows> ?o MINUS { ?s <http://ex/member> ?c } }",
        // DISTINCT over a projected variable.
        "SELECT DISTINCT ?o WHERE { ?s <http://ex/knows> ?o }",
        // String built-ins + BIND.
        "SELECT ?u WHERE { <http://ex/alice> <http://ex/name> ?nm BIND(UCASE(?nm) AS ?u) }",
        // ASK — true and false.
        "ASK { <http://ex/alice> <http://ex/knows> <http://ex/bob> }",
        "ASK { <http://ex/alice> <http://ex/knows> <http://ex/nobody> }",
        // CONSTRUCT — the byte-identical-output acceptance line.
        "CONSTRUCT { ?s <http://ex/related> ?o } WHERE { ?s <http://ex/knows> ?o }",
        "CONSTRUCT { ?o <http://ex/knownBy> ?s } WHERE { ?s <http://ex/knows> ?o }",
    ]
}

#[test]
fn native_matches_oxigraph_on_representative_queries() {
    let (dataset, store) = fixtures();
    for query in parity_queries() {
        let (ox, native) = run_both(&dataset, &store, query);
        assert_parity(query, &ox, &native);
    }
}

#[test]
fn order_by_matches_oxigraph_in_sequence() {
    // ORDER BY is sequence-sensitive: compare rows in order, not as a multiset.
    let (dataset, store) = fixtures();
    let query = "SELECT ?s ?n WHERE { ?s <http://ex/age> ?n } ORDER BY ?n";
    let (ox, native) = run_both(&dataset, &store, query);
    match (&ox, &native) {
        (
            SparqlResult::Solutions { rows: ox_rows, .. },
            SparqlResult::Solutions { rows: nat_rows, .. },
        ) => {
            let ox_seq: Vec<String> = ox_rows.iter().map(|r| row_key(r)).collect();
            let nat_seq: Vec<String> = nat_rows.iter().map(|r| row_key(r)).collect();
            assert_eq!(ox_seq, nat_seq, "ORDER BY sequence differs");
        }
        _ => panic!("expected solutions"),
    }
}
