// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use purrdf_lex::json::record::{from_slice, to_vec};

const ATOM: &str =
    r#"{"Atom":{"relation":{"Iri":"urn:p"},"args":[{"Var":"x"},{"SequenceMarker":"rest"}]}}"#;
#[test]
fn json_compiler_ast_all_five_term_variants_pin_exact_stored_bytes() {
    let cases = [
        (Term::var(" x ").unwrap(), r#"{"Var":" x "}"#),
        (Term::iri("urn:中").unwrap(), r#"{"Iri":"urn:中"}"#),
        (
            Term::rdf_literal(purrdf::RdfLiteral {
                lexical_form: "01.0".into(),
                datatype: Some("http://www.w3.org/2001/XMLSchema#decimal".into()),
                language: None,
                direction: None,
            })
            .unwrap(),
            r#"{"Literal":["01.0","http://www.w3.org/2001/XMLSchema#decimal",null,null]}"#,
        ),
        (
            Term::sequence_marker("rest").unwrap(),
            r#"{"SequenceMarker":"rest"}"#,
        ),
        (
            Term::app(
                "urn:f",
                vec![
                    Term::var("x").unwrap(),
                    Term::app("urn:g", vec![Term::iri("urn:o").unwrap()]).unwrap(),
                ],
            )
            .unwrap(),
            r#"{"App":{"symbol":"urn:f","args":[{"Var":"x"},{"App":{"symbol":"urn:g","args":[{"Iri":"urn:o"}]}}]}}"#,
        ),
    ];
    for (term, literal) in cases {
        assert_eq!(to_vec(&term), literal.as_bytes());
        assert_eq!(serde_json::to_vec(&term).unwrap(), literal.as_bytes());
        let restored: Term = from_slice(literal.as_bytes()).unwrap();
        assert_eq!(restored, term);
        assert_eq!(restored.key_in(&[], None), term.key_in(&[], None));
    }
}
#[test]
fn json_compiler_ast_all_eight_formula_variants_preserve_operand_and_binder_order() {
    let atom = Formula::atom(
        Term::iri("urn:p").unwrap(),
        vec![
            Term::var("x").unwrap(),
            Term::sequence_marker("rest").unwrap(),
        ],
    )
    .unwrap();
    let cases = [
        (atom.clone(), ATOM.to_owned()),
        (
            Formula::Not(Box::new(atom.clone())),
            format!("{{\"Not\":{ATOM}}}"),
        ),
        (
            Formula::And(vec![atom.clone(), Formula::Not(Box::new(atom.clone()))]),
            format!("{{\"And\":[{ATOM},{{\"Not\":{ATOM}}}]}}"),
        ),
        (
            Formula::Or(vec![Formula::Not(Box::new(atom.clone())), atom.clone()]),
            format!("{{\"Or\":[{{\"Not\":{ATOM}}},{ATOM}]}}"),
        ),
        (
            Formula::Implies(
                Box::new(atom.clone()),
                Box::new(Formula::Not(Box::new(atom.clone()))),
            ),
            format!("{{\"Implies\":[{ATOM},{{\"Not\":{ATOM}}}]}}"),
        ),
        (
            Formula::Iff(
                Box::new(Formula::Not(Box::new(atom.clone()))),
                Box::new(atom.clone()),
            ),
            format!("{{\"Iff\":[{{\"Not\":{ATOM}}},{ATOM}]}}"),
        ),
        (
            Formula::Forall {
                vars: vec!["z".into(), "a".into()],
                body: Box::new(atom.clone()),
            },
            format!("{{\"Forall\":{{\"vars\":[\"z\",\"a\"],\"body\":{ATOM}}}}}"),
        ),
        (
            Formula::Exists {
                vars: vec!["a".into(), "z".into()],
                body: Box::new(atom),
            },
            format!("{{\"Exists\":{{\"vars\":[\"a\",\"z\"],\"body\":{ATOM}}}}}"),
        ),
    ];
    for (formula, literal) in cases {
        assert_eq!(to_vec(&formula), literal.as_bytes());
        assert_eq!(serde_json::to_vec(&formula).unwrap(), literal.as_bytes());
        let restored: Formula = from_slice(literal.as_bytes()).unwrap();
        assert_eq!(restored, formula);
        assert_eq!(restored.content_key(), formula.content_key());
    }
    for literal in [
        r#"{"And":[]}"#,
        r#"{"Or":[]}"#,
        r#"{"And":[{"Atom":{"relation":{"Iri":"urn:p"},"args":[]}}]}"#,
        r#"{"Or":[{"And":[]}]}"#,
        r#"{"Forall":{"vars":["x","x"],"body":{"Or":[]}}}"#,
        r#"{"Exists":{"vars":[],"body":{"And":[]}}}"#,
    ] {
        let restored: Formula = from_slice(literal.as_bytes()).unwrap();
        assert_eq!(
            to_vec(&restored),
            literal.as_bytes(),
            "transport does not add connective arity policy"
        );
    }
}
#[test]
fn json_compiler_ast_single_literal_four_tuple_reuses_direction_and_language_normalization() {
    let input = r#"{"Literal":["中\n\"\\","http://www.w3.org/1999/02/22-rdf-syntax-ns#dirLangString","AR","rtl"]}"#;
    let expected = r#"{"Literal":["中\n\"\\","http://www.w3.org/1999/02/22-rdf-syntax-ns#dirLangString","ar","rtl"]}"#;
    let restored: Term = from_slice(input.as_bytes()).unwrap();
    let serde_restored: Term = serde_json::from_str(input).unwrap();
    assert_eq!(restored, serde_restored);
    assert_eq!(to_vec(&restored), expected.as_bytes());
    for payload in [
        "[]",
        r#"["x","d",null]"#,
        r#"["x","d",null,null,null]"#,
        r#"{"lexical_form":"x","datatype":"d","language":null,"direction":null}"#,
    ] {
        let bad = format!("{{\"Literal\":{payload}}}");
        assert!(from_slice::<Term>(bad.as_bytes()).is_err(), "{bad}");
    }
    for (payload, pointer) in [
        (r#"[null,"d",null,null]"#, "/Literal/0"),
        (r#"["x",null,null,null]"#, "/Literal/1"),
        (r#"["x","d",42,null]"#, "/Literal/2"),
        (r#"["x","d",null,"Rtl"]"#, "/Literal/3"),
    ] {
        let bad = format!("{{\"Literal\":{payload}}}");
        assert_eq!(
            from_slice::<Term>(bad.as_bytes()).unwrap_err().pointer(),
            pointer
        );
    }
    for bad in [
        r#"{"Literal":["x"," ",null,null]}"#,
        r#"{"Literal":["x","http://www.w3.org/2001/XMLSchema#string",null,"rtl"]}"#,
        r#"{"Literal":["x","http://www.w3.org/2001/XMLSchema#string","en",null]}"#,
    ] {
        assert!(from_slice::<Term>(bad.as_bytes()).is_err(), "{bad}");
        assert!(serde_json::from_str::<Term>(bad).is_err(), "{bad}");
    }
    let empty = r#"{"Literal":["","http://www.w3.org/2001/XMLSchema#string",null,null]}"#;
    let native: Term = from_slice(empty.as_bytes()).unwrap();
    let retained: Term = serde_json::from_str(empty).unwrap();
    assert_eq!(native, retained);
    assert_eq!(to_vec(&native), empty.as_bytes());
    match native {
        Term::Literal(literal) => {
            assert!(literal.lexical_form.is_empty());
            assert_eq!(literal.datatype, None);
        }
        other => panic!("wrong empty literal {other:?}"),
    }
}
#[test]
fn json_compiler_ast_native_admission_uses_existing_checked_constructors() {
    for invalid in [
        r#"{"Var":""}"#,
        r#"{"Var":"  "}"#,
        r#"{"Iri":" "}"#,
        r#"{"SequenceMarker":""}"#,
        r#"{"App":{"symbol":"urn:f","args":[]}}"#,
        r#"{"App":{"symbol":" ","args":[{"Iri":"urn:o"}]}}"#,
    ] {
        assert!(from_slice::<Term>(invalid.as_bytes()).is_err(), "{invalid}");
        assert!(
            serde_json::from_str::<Term>(invalid).is_ok(),
            "retained derived Serde has distinct constructor admission"
        );
    }
    let invalid = r#"{"Atom":{"relation":{"Var":"predicate"},"args":[]}}"#;
    assert!(from_slice::<Formula>(invalid.as_bytes()).is_err());
    assert!(serde_json::from_str::<Formula>(invalid).is_ok());
    let contextual = r#"{"Atom":{"relation":{"Iri":"https://blackcatinformatics.ca/logic/contextualApplication"},"args":[{"Iri":"urn:context"},{"Iri":"urn:relation"}]}}"#;
    assert!(from_slice::<Formula>(contextual.as_bytes()).is_err());
    let raw = Term::Var(String::new());
    assert_eq!(
        to_vec(&raw),
        br#"{"Var":""}"#,
        "encoding preserves stored AST; admission is a decoder boundary"
    );
}
#[test]
fn json_compiler_ast_exact_binary_arity_and_nested_member_refusals() {
    for tag in ["Implies", "Iff"] {
        for payload in [
            "null".to_owned(),
            "[]".into(),
            format!("[{ATOM}]"),
            format!("[{ATOM},{ATOM},{ATOM}]"),
        ] {
            let bad = format!("{{\"{tag}\":{payload}}}");
            assert!(from_slice::<Formula>(bad.as_bytes()).is_err(), "{bad}");
        }
    }
    let bad = format!(
        "{{\"Implies\":[{ATOM},{{\"Not\":{{\"Atom\":{{\"relation\":{{\"Iri\":\"urn:p\"}},\"args\":[{{\"Var\":null}}]}}}}}}]}}"
    );
    assert_eq!(
        from_slice::<Formula>(bad.as_bytes()).unwrap_err().pointer(),
        "/Implies/1/Not/Atom/args/0/Var"
    );
    for bad in [
        "{}",
        "[]",
        r#"{"Not":null}"#,
        r#"{"Unknown":[]}"#,
        r#"{"And":[],"And":[]}"#,
        r#"{"And":[],"Or":[]}"#,
        r#"{"Forall":{"vars":[],"body":null}}"#,
        r#"{"Exists":{"vars":[],"vars":[],"body":{"And":[]}}}"#,
    ] {
        assert!(from_slice::<Formula>(bad.as_bytes()).is_err(), "{bad}");
    }
    let bad = r#"{"App":{"symbol":"urn:f","args":[{"Iri":"urn:o"}],"args":[{"Iri":"urn:o"}]}}"#;
    assert_eq!(
        from_slice::<Term>(bad.as_bytes()).unwrap_err().pointer(),
        "/App/args"
    );
    let unknown = ATOM.replace("\"relation\":", "\"unknown\":null,\"relation\":");
    assert_eq!(
        to_vec(&from_slice::<Formula>(unknown.as_bytes()).unwrap()),
        ATOM.as_bytes()
    );
    assert!(from_slice::<Formula>(format!("{ATOM} {{}}").as_bytes()).is_err());
}
