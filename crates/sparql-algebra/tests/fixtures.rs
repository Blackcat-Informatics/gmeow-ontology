// SPDX-FileCopyrightText: 2026 Blackcat Informatics Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Curated SPARQL syntax fixtures — the in-house stand-in for the W3C SPARQL
//! syntax suite (the full official suite is deferred to a follow-up, the
//! option-3 deferral noted on #911).
//!
//! POSITIVE fixtures: one per in-scope feature; each MUST parse.
//! NEGATIVE fixtures: out-of-scope or malformed; each MUST hard-fail with the
//! expected typed [`ParseError`] variant (never a panic, never a silent parse).

use gmeow_sparql_algebra::{ParseError, SparqlParser};

const PREFIXES: &str = "PREFIX gmeow: <https://x/>\nPREFIX rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#>\nPREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#>\nPREFIX xsd: <http://www.w3.org/2001/XMLSchema#>\n";

fn ok(body: &str) {
    let q = format!("{PREFIXES}{body}");
    if let Err(e) = SparqlParser::new().parse_query(&q) {
        panic!("expected OK, got {e} for:\n{q}");
    }
}

#[test]
fn positive_in_scope_features() {
    // Query forms.
    ok("SELECT ?a WHERE { ?a a gmeow:T }");
    ok("SELECT * WHERE { ?a gmeow:p ?b }");
    ok("CONSTRUCT { ?s a gmeow:Out } WHERE { ?s a gmeow:In }");
    ok("ASK WHERE { ?a a gmeow:T }");
    ok("DESCRIBE ?a WHERE { ?a a gmeow:T }");
    ok("DESCRIBE gmeow:thing");

    // Group-pattern algebra.
    ok("SELECT ?a WHERE { ?a a gmeow:T . OPTIONAL { ?a gmeow:p ?b } }");
    ok("SELECT ?a WHERE { { ?a a gmeow:X } UNION { ?a a gmeow:Y } }");
    ok("SELECT ?a WHERE { ?a a gmeow:T MINUS { ?a a gmeow:U } }");
    ok("SELECT ?a WHERE { GRAPH ?g { ?a a gmeow:T } }");
    ok("SELECT ?a WHERE { ?a a gmeow:T . FILTER(?a != gmeow:x) }");
    ok("SELECT ?a WHERE { ?a a gmeow:T . FILTER NOT EXISTS { ?a gmeow:bad ?z } }");
    ok("SELECT ?a WHERE { ?a a gmeow:T . FILTER EXISTS { ?a gmeow:ok ?z } }");
    ok("SELECT ?k WHERE { ?a a gmeow:T . BIND(\"x\" AS ?k) }");

    // VALUES, single- and multi-column, with UNDEF.
    ok("SELECT ?x WHERE { VALUES ?x { gmeow:a gmeow:b } }");
    ok("SELECT ?x ?y WHERE { VALUES (?x ?y) { (gmeow:a 1) (gmeow:b UNDEF) } }");

    // Property paths.
    ok("SELECT ?x WHERE { ?x rdfs:subClassOf* gmeow:C }");
    ok("SELECT ?x WHERE { ?x gmeow:p+ ?y }");
    ok("SELECT ?x WHERE { ?x gmeow:p? ?y }");
    ok("SELECT ?x WHERE { ?x ^gmeow:p ?y }");
    ok("SELECT ?x WHERE { ?x gmeow:a/gmeow:b ?y }");
    ok("SELECT ?x WHERE { ?x gmeow:a|gmeow:b ?y }");
    ok("SELECT ?x WHERE { ?d gmeow:members/rdf:rest*/rdf:first ?x }");

    // Aggregation + modifiers.
    ok("SELECT ?m (COUNT(?c) AS ?n) WHERE { ?c gmeow:v ?m } GROUP BY ?m");
    ok("SELECT ?m (COUNT(DISTINCT ?c) AS ?n) WHERE { ?c gmeow:v ?m } GROUP BY ?m HAVING (COUNT(?c) >= 3)");
    ok("SELECT (SUM(?x) AS ?s) WHERE { ?a gmeow:x ?x }");
    ok("SELECT DISTINCT ?a WHERE { ?a a gmeow:T } ORDER BY DESC(?a) LIMIT 5 OFFSET 2");
    ok("SELECT ?a WHERE { ?a a gmeow:T } ORDER BY ?a");

    // Expressions: COALESCE / IF / IN / BOUND / functions / literals.
    ok("SELECT ?a WHERE { ?a gmeow:p ?b . FILTER(?b IN (1, 2, 3)) }");
    ok("SELECT (COALESCE(?b, \"d\") AS ?c) WHERE { OPTIONAL { ?a gmeow:p ?b } }");
    ok("SELECT (IF(BOUND(?b), 1, 0) AS ?c) WHERE { OPTIONAL { ?a gmeow:p ?b } }");
    ok("SELECT ?a WHERE { ?a gmeow:p ?b . FILTER(STRSTARTS(STR(?b), \"http\")) }");
    ok("SELECT ?a WHERE { ?a gmeow:p ?b . FILTER(isIRI(?b)) }");
    ok("SELECT ?a WHERE { ?a gmeow:p ?b . FILTER(gmeow:customFn(?b)) }");
    ok("SELECT ?a WHERE { ?a gmeow:p \"hi\"@en, \"x\"^^xsd:string, 3, 3.5, 1e9, true }");

    // RDF 1.2 quoted triple terms — both spellings.
    ok("SELECT ?r WHERE { ?r rdf:reifies <<( ?s ?p ?o )>> }");
    ok("SELECT ?r WHERE { ?r rdf:reifies << ?s gmeow:p ?o >> }");

    // BASE resolves a relative IRIREF to an absolute IRI in term position.
    ok("BASE <http://base/> SELECT ?a WHERE { ?a a <Thing> }");
}

#[test]
fn negative_out_of_scope_and_malformed() {
    fn err(q: &str) -> ParseError {
        let full = format!("{PREFIXES}{q}");
        SparqlParser::new()
            .parse_query(&full)
            .expect_err(&format!("expected an error for:\n{full}"))
    }

    // Out of scope → Unsupported (well-formed SPARQL, deliberately rejected).
    assert!(matches!(
        err("SELECT ?a FROM <http://g/> WHERE { ?a a gmeow:T }"),
        ParseError::Unsupported(_)
    ));
    assert!(matches!(
        err("CONSTRUCT { ?s a gmeow:O } WHERE { ?s gmeow:x ?x } GROUP BY ?s"),
        ParseError::Unsupported(_)
    ));

    // Undeclared prefix → Syntax.
    assert!(matches!(
        SparqlParser::new()
            .parse_query("SELECT ?a WHERE { ?a a nope:T }")
            .unwrap_err(),
        ParseError::Syntax { .. }
    ));

    // Trailing tokens / two concatenated queries → Syntax.
    assert!(matches!(
        err("SELECT ?a WHERE { ?a a gmeow:T } SELECT ?b WHERE { ?b a gmeow:U }"),
        ParseError::Syntax { .. }
    ));

    // Unclosed group → Syntax.
    assert!(matches!(
        err("SELECT ?a WHERE { ?a a gmeow:T "),
        ParseError::Syntax { .. }
    ));

    // No query form → Syntax.
    assert!(matches!(
        err("PREFIX p: <http://p/>"),
        ParseError::Syntax { .. }
    ));

    // Lexically broken input → Lex or Syntax (never a panic).
    let e = err("SELECT ?a WHERE { ?a a gmeow:T . FILTER(?a & ?b) }");
    assert!(matches!(
        e,
        ParseError::Lex { .. } | ParseError::Syntax { .. }
    ));

    // Relative IRIREF in term position with no in-scope BASE → Iri (term-position
    // IRIs must be absolute; the parser no longer admits a bare relative ref).
    assert!(matches!(
        err("SELECT ?a WHERE { ?a a <RelativeNoScheme> }"),
        ParseError::Iri { .. }
    ));

    // Empty variable name (`$` with no following name) → Lex (no empty token).
    assert!(matches!(
        err("SELECT ?a WHERE { ?a gmeow:p $ }"),
        ParseError::Lex { .. }
    ));

    // Empty blank-node label (`_:` with no label) → Lex.
    assert!(matches!(
        err("SELECT ?a WHERE { _: gmeow:p ?a }"),
        ParseError::Lex { .. }
    ));

    // Raw newline inside a SHORT string literal → Lex (only `'''`/`\"\"\"` allow it).
    assert!(matches!(
        err("SELECT ?a WHERE { ?a gmeow:p \"line1\nline2\" }"),
        ParseError::Lex { .. }
    ));

    // `PREFIX` with a non-empty local part → Syntax (must be a bare PNAME_NS).
    assert!(matches!(
        SparqlParser::new()
            .parse_query("PREFIX ex:local <http://e/>\nSELECT ?a WHERE { ?a a ex:T }")
            .unwrap_err(),
        ParseError::Syntax { .. }
    ));
}
