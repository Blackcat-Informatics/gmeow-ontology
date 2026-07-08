// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Shared low-level SPARQL token / escaping primitives.
//!
//! Both the SHACL-AF **rule** projection ([`super::shacl_af`], which lowers Horn rule bodies
//! to `sh:SPARQLRule` CONSTRUCTs) and the procedural-**constraint** projection
//! ([`super::shapes::project_procedural_constraints`], which lowers a range-restricted
//! `logic:Constraint` integrity condition to a `sh:SPARQLConstraint` SELECT) render the same
//! kinds of SPARQL token: a predicate (`a` for `rdf:type`, else `<iri>`) and a data literal
//! double-escaped for the SPARQL layer inside a Turtle `"""…"""` carrier. Factoring the
//! primitives here means the two SPARQL surfaces cannot drift, and the single Formula→SPARQL
//! lowering machinery (NNF / BGP / `FILTER NOT EXISTS`) is not duplicated across them.

use super::RDF_TYPE;

/// Escape a string for a SPARQL single-quoted string literal (`STRING_LITERAL1`): the
/// backslash, the single quote, and the C0 control characters that may not appear raw.
pub(crate) fn sparql_escape(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for c in value.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '\'' => out.push_str("\\'"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{08}' => out.push_str("\\b"),
            '\u{0C}' => out.push_str("\\f"),
            _ => out.push(c),
        }
    }
    out
}

/// Escape a string for embedding inside a Turtle long string (`"""…"""`): the backslash and
/// the double quote. Escaping every `"` (so no `"""` can terminate the long string early) and
/// doubling every `\` makes the embedded text round-trip through the Turtle parser byte-for-byte.
pub(crate) fn turtle_long_string_escape(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

/// A SPARQL literal rendered for embedding inside a Turtle `"""…"""` SHACL string. The value
/// passes through TWO parsers — Turtle un-escapes the `"""…"""` carrier, then SHACL parses the
/// resulting SPARQL — so it is escaped for the SPARQL layer first, then for the Turtle
/// long-string carrier. A single-quoted SPARQL string keeps an inner `"` off the SPARQL quote;
/// the Turtle layer then neutralizes every `"` and `\` so a value containing `"""`, a newline,
/// a tab, or a backslash can never break either parser.
pub(crate) fn sparql_literal(value: &str) -> String {
    format!("'{}'", turtle_long_string_escape(&sparql_escape(value)))
}

/// The SPARQL predicate token (`a` for `rdf:type`, else `<iri>`). Predicates are always
/// IRIs in the `logic:` IR.
pub(crate) fn sparql_predicate(predicate: &str) -> String {
    if predicate == RDF_TYPE {
        "a".to_owned()
    } else {
        format!("<{predicate}>")
    }
}
