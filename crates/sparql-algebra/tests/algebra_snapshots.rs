// SPDX-FileCopyrightText: 2026 Blackcat Informatics Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Structural AST goldens (purrdf S5 / #911).
//!
//! The corpus suite asserts only that queries parse (`Ok`); it does not pin the
//! *shape* of the produced algebra, so a refactor could silently change what a
//! query lowers to while every test stays green (exactly how the aggregate /
//! ORDER BY gaps hid). These `insta` snapshots over the `{:#?}` of the parsed
//! `Query` lock the algebra shape for representative in-scope features. A
//! `proptest` additionally pins the no-panic contract on arbitrary input.

use gmeow_sparql_algebra::SparqlParser;
use proptest::prelude::*;

const PREFIXES: &str = "PREFIX gmeow: <https://x/>\n\
     PREFIX rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#>\n\
     PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#>\n";

fn parse(body: &str) -> impl std::fmt::Debug {
    SparqlParser::new()
        .parse_query(&format!("{PREFIXES}{body}"))
        .expect("snapshot fixture must parse")
}

#[test]
fn snapshot_quoted_triple_paren() {
    // RDF 1.2 quoted-triple term → TermPattern::Triple (codec shape).
    insta::assert_debug_snapshot!(parse("SELECT ?r WHERE { ?r rdf:reifies <<( ?s ?p ?o )>> }"));
}

#[test]
fn snapshot_quoted_triple_bare() {
    // The `<< s p o >>` spelling must lower identically to the paren form.
    insta::assert_debug_snapshot!(parse(
        "SELECT ?r WHERE { ?r rdf:reifies << ?s gmeow:p ?o >> }"
    ));
}

#[test]
fn snapshot_aggregate_group_by() {
    // COUNT lifts into Group; the projection references the synthetic agg var.
    insta::assert_debug_snapshot!(parse(
        "SELECT ?t (COUNT(?x) AS ?c) WHERE { ?x a ?t } GROUP BY ?t"
    ));
}

#[test]
fn snapshot_property_path() {
    // `/` + `*` property path → Path with a Sequence/ZeroOrMore expression.
    insta::assert_debug_snapshot!(parse(
        "SELECT ?x WHERE { ?d gmeow:members/rdf:rest*/rdf:first ?x }"
    ));
}

#[test]
fn snapshot_optional_union_bind() {
    // OPTIONAL → LeftJoin, UNION → Union, BIND → Extend in one query.
    insta::assert_debug_snapshot!(parse(
        "SELECT ?k WHERE { { ?a a gmeow:X } UNION { ?a a gmeow:Y } OPTIONAL { ?a gmeow:p ?b } BIND(\"x\" AS ?k) }"
    ));
}

proptest! {
    // The parser must never panic on arbitrary input — it returns Ok or a typed
    // ParseError. Restrict to a SPARQL-ish alphabet so the lexer is exercised
    // deeply rather than rejecting on the first non-ASCII byte.
    #[test]
    fn parse_never_panics(s in "[a-zA-Z0-9 ?<>{}().*+/^!|:_\"@-]{0,80}") {
        let _ = SparqlParser::new().parse_query(&s);
    }
}
