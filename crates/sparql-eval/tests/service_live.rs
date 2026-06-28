// SPDX-FileCopyrightText: 2026 Blackcat Informatics Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Live-network `SERVICE` federation test — **maintainer lane only**.
//!
//! Drives the real [`HttpRemoteQuerySource`] against a public SPARQL endpoint
//! (the Wikidata Query Service) to exercise the actual HTTP transport + SPARQL
//! Results JSON decode end-to-end. Both `#[ignore]` (excluded from `cargo test`
//! and `make check`) AND an env guard (`GMEOW_RUN_NETWORK=1`) gate it, so it never
//! runs in the automated gate and never hits the network unintentionally. Invoke
//! via `make maint-test-network-rust`.

#![cfg(not(target_arch = "wasm32"))]

use gmeow_rdf_core::{RdfDatasetBuilder, SparqlRequest, SparqlResult};
use gmeow_sparql_algebra::Variable;
use gmeow_sparql_eval::{HttpRemoteQuerySource, NativeSparqlEngine, RemoteQuerySource};

const WIKIDATA: &str = "https://query.wikidata.org/sparql";

/// True only when the maintainer network lane is explicitly enabled.
fn network_enabled() -> bool {
    std::env::var("GMEOW_RUN_NETWORK").is_ok()
}

#[test]
#[ignore = "network lane only: GMEOW_RUN_NETWORK=1 + --ignored (make maint-test-network-rust)"]
fn http_transport_decodes_remote_bindings() {
    if !network_enabled() {
        eprintln!("skipping http_transport_decodes_remote_bindings: GMEOW_RUN_NETWORK not set");
        return;
    }
    // A trivial, data-independent query: the transport + SRJ decode are what we
    // are testing, not Wikidata's content.
    let source = HttpRemoteQuerySource::new();
    let resolved = source
        .query(WIKIDATA, "SELECT ?x WHERE { BIND(1 AS ?x) }")
        .expect("wikidata transport");
    assert_eq!(resolved.variables, vec![Variable::new("x")]);
    assert_eq!(resolved.rows.len(), 1, "expected exactly one binding row");
    assert!(resolved.rows[0][0].is_some(), "?x must be bound");
}

#[test]
#[ignore = "network lane only: GMEOW_RUN_NETWORK=1 + --ignored (make maint-test-network-rust)"]
fn service_clause_federates_to_wikidata() {
    if !network_enabled() {
        eprintln!("skipping service_clause_federates_to_wikidata: GMEOW_RUN_NETWORK not set");
        return;
    }
    // A local one-row dataset whose value is forwarded into a SERVICE block that
    // binds a second variable at the remote endpoint, then joined back.
    let mut b = RdfDatasetBuilder::new();
    let p = b.intern_iri("http://ex/p".to_owned());
    let s = b.intern_iri("http://ex/s".to_owned());
    let o = b.intern_iri("http://ex/o".to_owned());
    b.push_quad(s, p, o, None);
    let dataset = b.freeze().expect("freeze");

    let query = "SELECT ?x WHERE { \
                 <http://ex/s> <http://ex/p> ?o \
                 SERVICE <https://query.wikidata.org/sparql> { BIND(1 AS ?x) } }";
    let engine = NativeSparqlEngine::new();
    let source = HttpRemoteQuerySource::new();
    let result = engine
        .query_with_source(
            &dataset,
            SparqlRequest {
                query,
                base_iri: None,
            },
            &source,
        )
        .expect("federated query");
    match result {
        SparqlResult::Solutions {
            variables, rows, ..
        } => {
            assert!(variables.contains(&"x".to_owned()));
            assert_eq!(rows.len(), 1, "the SERVICE bag joins the single local row");
        }
        other => panic!("expected solutions, got {other:?}"),
    }
}
