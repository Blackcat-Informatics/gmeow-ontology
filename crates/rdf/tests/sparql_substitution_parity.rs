// SPDX-FileCopyrightText: 2026 Blackcat Informatics Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Differential parity for SPARQL variable **pre-binding** / substitution
//! (purrdf S5–S6, EPIC #906 GAP-A): the native `gmeow-sparql-eval` engine driven by
//! `SparqlRequest.substitutions` vs the oxigraph baseline driven by the SAME
//! `SparqlRequest.substitutions` (which the `OxigraphBackend` forwards to
//! `PreparedSparqlQuery::substitute_variable`).
//!
//! This is the transitional oracle (it stops compiling once oxigraph is dropped). It
//! pins that the native substitution path is bit-for-bit the contract SHACL-AF relied
//! on oxigraph for: pre-binding `$this` (the focus node) into a constraint/target
//! query — propagating into FILTER / OPTIONAL / NOT EXISTS / sub-positions, keeping
//! the variable projectable, and admitting IRI **and blank-node** focus nodes.
//!
//! The dataset is built programmatically (not parsed from text) so the blank-node
//! label `bn` is identical on both sides — a text parse would relabel anonymous
//! blanks per source and break the cross-engine reference.

#![cfg(feature = "oxigraph")]

use std::sync::Arc;

use gmeow_rdf::oxigraph::{store_from_dataset, GraphPolicy};
use gmeow_rdf::{
    BlankScope, OxigraphBackend, RdfDataset, RdfDatasetBuilder, RdfLiteral, SparqlRequest,
    SparqlResult, TermValue,
};
use gmeow_rdf_core::SparqlEngine;
use gmeow_sparql_eval::NativeSparqlEngine;
use oxigraph::store::Store;

const XSD_INT: &str = "http://www.w3.org/2001/XMLSchema#integer";

/// A dataset exercising every focus-substitution position:
/// ```text
///   :alice :knows :bob ;   :age 30 .
///   :bob   :knows :carol ; :age 17 .
///   _:bn   :knows :carol .          # blank-node focus (a SHACL blank $this)
///   :carol :member :club .
/// ```
fn fixtures() -> (Arc<RdfDataset>, Store) {
    let mut b = RdfDatasetBuilder::new();
    let knows = b.intern_iri("http://ex/knows".to_owned());
    let age = b.intern_iri("http://ex/age".to_owned());
    let member = b.intern_iri("http://ex/member".to_owned());
    let alice = b.intern_iri("http://ex/alice".to_owned());
    let bob = b.intern_iri("http://ex/bob".to_owned());
    let carol = b.intern_iri("http://ex/carol".to_owned());
    let club = b.intern_iri("http://ex/club".to_owned());
    let bn = b.intern_blank("bn".to_owned(), BlankScope::DEFAULT);
    let i30 = b.intern_literal(RdfLiteral::typed("30".to_owned(), XSD_INT.to_owned()));
    let i17 = b.intern_literal(RdfLiteral::typed("17".to_owned(), XSD_INT.to_owned()));

    b.push_quad(alice, knows, bob, None);
    b.push_quad(alice, age, i30, None);
    b.push_quad(bob, knows, carol, None);
    b.push_quad(bob, age, i17, None);
    b.push_quad(bn, knows, carol, None);
    b.push_quad(carol, member, club, None);

    let dataset = b.freeze().expect("freeze");
    let store = store_from_dataset(&dataset, GraphPolicy::PreserveNamedGraphs).expect("store");
    (dataset, store)
}

/// A stable, order-insensitive key for a solution row.
fn row_key(row: &[Option<TermValue>]) -> String {
    format!("{row:?}")
}

/// Run BOTH engines with the same `query` + `substitutions` and assert identical
/// results (SELECT as a multiset, ASK by value).
fn assert_parity(query: &str, substitutions: &[(String, TermValue)]) {
    let (dataset, store) = fixtures();
    let request = SparqlRequest {
        query,
        base_iri: None,
        substitutions,
    };
    let ox = OxigraphBackend
        .query(&store, request)
        .unwrap_or_else(|e| panic!("oxigraph failed for {query:?}: {e:?}"));
    let native = NativeSparqlEngine::new()
        .query(&dataset, request)
        .unwrap_or_else(|e| panic!("native failed for {query:?}: {e:?}"));

    match (&ox, &native) {
        (
            SparqlResult::Solutions {
                variables: ov,
                rows: orows,
                ..
            },
            SparqlResult::Solutions {
                variables: nv,
                rows: nrows,
                ..
            },
        ) => {
            assert_eq!(ov, nv, "{query}: variable list differs");
            let mut o: Vec<String> = orows.iter().map(|r| row_key(r)).collect();
            let mut n: Vec<String> = nrows.iter().map(|r| row_key(r)).collect();
            o.sort();
            n.sort();
            assert_eq!(o, n, "{query}: solution multiset differs");
        }
        (SparqlResult::Boolean(ob), SparqlResult::Boolean(nb)) => {
            assert_eq!(ob, nb, "{query}: ASK boolean differs");
        }
        _ => panic!("{query}: result-shape mismatch ({ox:?} vs {native:?})"),
    }
}

fn iri(s: &str) -> TermValue {
    TermValue::Iri(s.to_owned())
}

fn alice() -> [(String, TermValue); 1] {
    [("this".to_owned(), iri("http://ex/alice"))]
}

fn blank_focus() -> [(String, TermValue); 1] {
    [(
        "this".to_owned(),
        TermValue::Blank {
            label: "bn".to_owned(),
            scope: BlankScope::DEFAULT,
        },
    )]
}

// NOTE: every SELECT projects `?this`. oxigraph's `substitute_variable` requires
// the substituted variable to appear in the result projection (it errors otherwise),
// which matches real SHACL-AF usage (`SELECT $this …`). Projecting `?this` keeps the
// oracle valid while still exercising every WHERE position.

#[test]
fn substitute_subject_position() {
    // $this in subject position.
    assert_parity(
        "SELECT ?this ?o WHERE { ?this <http://ex/knows> ?o }",
        &alice(),
    );
}

#[test]
fn substitute_object_position() {
    // $this in object position (who knows :carol?). Substitute the OBJECT.
    assert_parity(
        "SELECT ?this ?s WHERE { ?s <http://ex/knows> ?this }",
        &[("this".to_owned(), iri("http://ex/carol"))],
    );
}

#[test]
fn substitute_projects_only_the_focus_var() {
    // The substituted variable is the SOLE projected column — it must survive
    // projection as the pre-bound value.
    assert_parity(
        "SELECT ?this WHERE { ?this <http://ex/knows> ?o }",
        &alice(),
    );
}

#[test]
fn substitute_referenced_in_filter() {
    // $this also referenced in a FILTER (here via a value join through ?n).
    assert_parity(
        "SELECT ?this ?o WHERE { ?this <http://ex/knows> ?o . \
         ?this <http://ex/age> ?n FILTER(?n > 18) }",
        &alice(),
    );
}

#[test]
fn substitute_into_optional() {
    // $this constrains the required part; the OPTIONAL must see the binding.
    assert_parity(
        "SELECT ?this ?o ?m WHERE { ?this <http://ex/knows> ?o \
         OPTIONAL { ?o <http://ex/member> ?m } }",
        &alice(),
    );
}

#[test]
fn substitute_into_not_exists() {
    // The corpus-critical SHACL idiom: $this with a FILTER NOT EXISTS sub-pattern.
    assert_parity(
        "SELECT ?this ?o WHERE { ?this <http://ex/knows> ?o \
         FILTER NOT EXISTS { ?this <http://ex/member> ?c } }",
        &alice(),
    );
}

#[test]
fn substitute_ask() {
    // ASK form — pre-binding flows into the boolean result.
    assert_parity("ASK { ?this <http://ex/knows> <http://ex/bob> }", &alice());
    assert_parity(
        "ASK { ?this <http://ex/knows> <http://ex/nobody> }",
        &alice(),
    );
}

#[test]
fn substitute_blank_node_focus() {
    // A blank-node focus node — the case `GroundTerm`/`VALUES` cannot carry, handled
    // via the injection-only blank seed. _:bn knows :carol.
    assert_parity(
        "SELECT ?this ?o WHERE { ?this <http://ex/knows> ?o }",
        &blank_focus(),
    );
    assert_parity(
        "ASK { ?this <http://ex/knows> <http://ex/carol> }",
        &blank_focus(),
    );
}

#[test]
fn substitute_blank_focus_projects_the_blank() {
    // The blank must round-trip into the projected result identically on both sides.
    assert_parity(
        "SELECT ?this ?o WHERE { ?this <http://ex/knows> ?o }",
        &blank_focus(),
    );
}
