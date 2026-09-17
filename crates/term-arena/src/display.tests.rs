// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

/// The Display surface preserves the language tag verbatim while the N3 surface
/// lowercases it — the exact divergence the interner's dedup key depends on
/// (`"a"@EN` and `"a"@en` must stay DISTINCT atoms).
#[test]
fn display_preserves_lang_tag_case_and_n3_lowercases_it() {
    let upper = TermValue::Literal {
        lexical_form: "a".to_owned(),
        datatype: RDF_LANG_STRING.to_owned(),
        language: Some("EN".to_owned()),
        direction: None,
    };
    assert_eq!(term_display(&upper), "\"a\"@EN");
    assert_eq!(term_n3_unchecked(&upper), "\"a\"@en");
}

/// `xsd:string` and a lang-less `rdf:langString` render to the SAME Display bytes —
/// the historical collapse the atom dictionary preserves byte-exactly.
#[test]
fn display_elides_xsd_string_and_langless_lang_string_alike() {
    let plain = TermValue::simple_literal("a");
    let langless = TermValue::Literal {
        lexical_form: "a".to_owned(),
        datatype: RDF_LANG_STRING.to_owned(),
        language: None,
        direction: None,
    };
    assert_eq!(term_display(&plain), "\"a\"");
    assert_eq!(term_display(&langless), "\"a\"");
    assert_eq!(
        term_display(&TermValue::iri("http://ex/a")),
        "<http://ex/a>"
    );
}

/// A nested triple term renders iteratively in the RDF 1.2 non-asserting form.
#[test]
fn display_renders_nested_triple_terms() {
    let inner = TermValue::Triple {
        s: Box::new(TermValue::iri("http://ex/a")),
        p: Box::new(TermValue::iri("http://ex/p")),
        o: Box::new(TermValue::iri("http://ex/b")),
    };
    let outer = TermValue::Triple {
        s: Box::new(inner),
        p: Box::new(TermValue::iri("http://ex/q")),
        o: Box::new(TermValue::simple_literal("v")),
    };
    assert_eq!(
        term_display(&outer),
        "<<( <<( <http://ex/a> <http://ex/p> <http://ex/b> )>> <http://ex/q> \"v\" )>>"
    );
}

/// Escaping is exactly the rdflib set, and nothing else.
#[test]
fn display_escapes_exactly_the_rdflib_set() {
    let lit = TermValue::simple_literal("a\\b\"c\nd\re\tf");
    assert_eq!(term_display(&lit), "\"a\\\\b\\\"c\\nd\\re\\tf\"");
}
