// SPDX-License-Identifier: AGPL-3.0-only
//! Native SPARQL surface twins migrated from `tests/test_compat_rdflib.py`.
//!
//! The Python file was the rdflib-compat facade differential suite. Per user
//! ruling only its two SPARQL fns have an in-repo Rust twin; the other 24 fns are
//! external-purrdf PyO3-seam behaviours (str-subclassing, serialization
//! round-trips, RDF `Collection`, term equality, …) with no Rust home and are
//! dropped in the reconciliation manifest.
//!
//! Both twins run over a TINY inline Turtle fixture through the native purrdf
//! query engine (`GraphStore`), never the merged ontology — mirroring the
//! originals that built a two-triple in-memory graph.
//!
//! - `select_ask_construct_surface` twins
//!   `test_sparql_select_ask_construct_and_resultrow`: the SELECT variable list +
//!   projected subject IRIs (the native twin of rdflib `ResultRow` positional +
//!   named access), ASK truth, and the CONSTRUCT projection graph length + members
//!   (the native twin of `cg.graph`).
//! - `initbindings_binds_nonprojected_var` twins
//!   `test_query_initbindings_nonprojected_var`: a pre-bound NON-projected variable
//!   (`initBindings={"person": ex:alice}`), carried natively via
//!   `SparqlRequest.substitutions`, restricts an otherwise-open `knows` pattern.

use crate::conformance_support::*;
use purrdf::TermValue;

/// `http://example.org/` — the `EX` namespace of the rdflib-compat originals.
const EX: &str = "http://example.org/";

fn ex(local: &str) -> String {
    format!("{EX}{local}")
}

/// Twin of `test_sparql_select_ask_construct_and_resultrow`.
///
/// `ex:a a ex:T . ex:b a ex:T .` — SELECT projects the two subjects, ASK is true,
/// and CONSTRUCT re-projects the two `rdf:type` triples.
#[gmeow_test_batch_macros::batch_test]
fn select_ask_construct_surface() {
    let g = GraphStore::parse_ttl(
        "@prefix ex: <http://example.org/> .\n\
         ex:a a ex:T .\n\
         ex:b a ex:T .\n",
    );

    // SELECT ?s WHERE { ?s a ?t } — the native twin of iterating rdflib
    // `ResultRow`s and reading `r["s"]` / `r.s`.
    let (vars, rows) = g.select(&[], "SELECT ?s WHERE { ?s a ?t }");
    assert!(
        vars.contains(&"s".to_owned()),
        "SELECT projection must carry variable \"s\"; got {vars:?}"
    );
    let s_idx = vars
        .iter()
        .position(|v| v == "s")
        .expect("variable \"s\" is in the projection");
    let mut subjects: Vec<String> = rows
        .iter()
        .map(|row| match row.get(s_idx).and_then(|c| c.as_ref()) {
            Some(term) => match term {
                v if *v == iri(&ex("a")) => ex("a"),
                v if *v == iri(&ex("b")) => ex("b"),
                other => panic!("unexpected ?s binding {other:?}"),
            },
            None => panic!("row is missing a binding for ?s: {row:?}"),
        })
        .collect();
    subjects.sort();
    assert_eq!(subjects, vec![ex("a"), ex("b")]);

    // ASK { ?s a ?t } — a type triple exists, so the answer is true.
    assert!(
        g.ask(&[], "ASK { ?s a ?t }"),
        "ASK over an existing type triple must be true"
    );

    // CONSTRUCT { ?s a ?t } WHERE { ?s a ?t } — the native twin of `cg.graph`:
    // materialise the projection and assert its length + members.
    let constructed = g.construct(&[], "CONSTRUCT { ?s a ?t } WHERE { ?s a ?t }");
    assert_eq!(
        constructed.triple_count(),
        2,
        "CONSTRUCT graph must carry exactly the two type triples"
    );
    assert!(
        constructed.has(Some(&ex("a")), Some(RDF_TYPE), Some(&ex("T"))),
        "CONSTRUCT graph must contain ex:a a ex:T"
    );
    assert!(
        constructed.has(Some(&ex("b")), Some(RDF_TYPE), Some(&ex("T"))),
        "CONSTRUCT graph must contain ex:b a ex:T"
    );
}

/// Twin of `test_query_initbindings_nonprojected_var`.
///
/// Pre-binding the NON-projected `?person` to `ex:alice` restricts the open
/// `?person ex:knows ?friend` pattern to alice's edge, yielding only `ex:bob`.
#[gmeow_test_batch_macros::batch_test]
fn initbindings_binds_nonprojected_var() {
    let g = GraphStore::parse_ttl(
        "@prefix ex: <http://example.org/> .\n\
         ex:alice ex:knows ex:bob .\n\
         ex:carol ex:knows ex:dan .\n",
    );

    let (vars, rows) = g.select(
        &[("person".to_owned(), iri(&ex("alice")))],
        "SELECT ?friend WHERE { ?person <http://example.org/knows> ?friend }",
    );
    assert!(
        vars.contains(&"friend".to_owned()),
        "SELECT projection must carry variable \"friend\"; got {vars:?}"
    );
    let friend_idx = vars
        .iter()
        .position(|v| v == "friend")
        .expect("variable \"friend\" is in the projection");
    let friends: Vec<Option<TermValue>> = rows
        .iter()
        .map(|row| row.get(friend_idx).and_then(|c| c.clone()))
        .collect();
    assert_eq!(
        friends,
        vec![Some(iri(&ex("bob")))],
        "pre-binding ?person = ex:alice must yield exactly ex:bob"
    );
}
