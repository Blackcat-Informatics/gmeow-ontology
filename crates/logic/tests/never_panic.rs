// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! "Reject malformed, never panic" property gate (T7) for the gmeow-logic
//! frontends.
//!
//! `compile::frontend::parse_logic_str` parses untrusted logic-Turtle into a
//! `LogicProgram`; `query_ir::parse_query_program` parses the `.logic` query DSL
//! and documents "Never panics" — this PROVES it. Given arbitrary input both must
//! return `Ok`/`Err`, never panic. Inputs are bounded so a superlinear parse
//! cannot become a spurious timeout. See purrdf's `never_panic` test for
//! the contract rationale.

use gmeow_logic::query_ir::parse_query_program;
use gmeow_logic_compile::frontend::parse_logic_str;
use proptest::prelude::*;

fn arbitrary_bytes() -> impl Strategy<Value = Vec<u8>> {
    prop::collection::vec(any::<u8>(), 0..4096)
}

/// Structure-aware logic-Turtle: real `logic:` rule/axiom fragments + noise, so
/// the frontend's RDF→IR interpreter is reached, not just the Turtle lexer.
fn structured_logic_turtle() -> impl Strategy<Value = String> {
    let fragments: Vec<&'static str> = vec![
        "@prefix logic: <https://blackcatinformatics.ca/logic/> .\n",
        "@prefix ex: <https://example.org/> .\n",
        "@prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .\n",
        "ex:A logic:subClassOf ex:B .\n",
        "ex:r a logic:Rule ;\n  logic:head [ rdf:subject \"?x\" ; rdf:predicate logic:p ; rdf:object \"?y\" ] ;\n",
        "  logic:body [ rdf:subject \"?x\" ; rdf:predicate logic:q ; rdf:object \"?y\" ] .\n",
        "ex:A a logic:Kind .\n",
        "<<ex:A logic:subClassOf ex:B>> logic:confidence \"0.9\" .\n",
        "\u{0}\u{1}",
        "ex:r a logic:Rule ; logic:head",
        "@prefix logic:",
    ];
    prop::collection::vec(prop::sample::select(fragments), 0..24).prop_map(|parts| parts.concat())
}

/// Structure-aware query DSL: tokens of the `.logic` query format + noise.
fn structured_query() -> impl Strategy<Value = String> {
    let fragments: Vec<&'static str> = vec![
        "?- ",
        "ancestor(?x, ?y).\n",
        "parent(a, b).\n",
        ":- ",
        "cut.\n",
        "fail.\n",
        "foo(",
        ", ",
        ").\n",
        "\"str\"",
        "?var",
        "\u{0}",
        "(((",
        ":-:-:-",
    ];
    prop::collection::vec(prop::sample::select(fragments), 0..24).prop_map(|parts| parts.concat())
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 256, ..ProptestConfig::default() })]

    #[test]
    fn parse_logic_str_never_panics_raw(data in arbitrary_bytes()) {
        if let Ok(text) = std::str::from_utf8(&data) {
            let _ = parse_logic_str(text, None);
        }
    }

    #[test]
    fn parse_logic_str_never_panics_structured(text in structured_logic_turtle()) {
        let _ = parse_logic_str(&text, None);
    }

    #[test]
    fn parse_query_program_never_panics_raw(data in arbitrary_bytes()) {
        if let Ok(text) = std::str::from_utf8(&data) {
            let _ = parse_query_program(text);
        }
    }

    #[test]
    fn parse_query_program_never_panics_structured(text in structured_query()) {
        let _ = parse_query_program(&text);
    }
}
