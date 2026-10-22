// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use purrdf::{BlankScope, RdfTextDirection, TermBox, TermValue};
use purrdf_lex::json::record::{from_slice, to_vec};

#[test]
fn json_retained_term_complete_nested_literal_preserves_identity_and_order() {
    let term = TermValue::Triple {
        s: TermBox::new(TermValue::Blank {
            label: "same".into(),
            scope: BlankScope(17),
        }),
        p: TermBox::new(TermValue::iri("urn:states")),
        o: TermBox::new(TermValue::Triple {
            s: TermBox::new(TermValue::Blank {
                label: "same".into(),
                scope: BlankScope(19),
            }),
            p: TermBox::new(TermValue::iri("urn:quote")),
            o: TermBox::new(TermValue::Literal {
                lexical_form: "中\n\"\\".into(),
                datatype: "not normalized".into(),
                language: Some("ar".into()),
                direction: Some(RdfTextDirection::Rtl),
            }),
        }),
    };
    let expected = r#"{"Triple":{"s":{"Blank":{"label":"same","scope":17}},"p":{"Iri":"urn:states"},"o":{"Triple":{"s":{"Blank":{"label":"same","scope":19}},"p":{"Iri":"urn:quote"},"o":{"Literal":{"lexical_form":"中\n\"\\","datatype":"not normalized","language":"ar","direction":"Rtl"}}}}}}"#;
    let json = crate::term_serde::to_json(&term);
    assert_eq!(purrdf_lex::json::write_compact(&json), expected);
    assert_eq!(crate::term_serde::from_json(&json).unwrap(), term);
    let restored: Value = from_slice(expected.as_bytes()).unwrap();
    let restored_term: TermValue = restored.into();
    assert_eq!(restored_term, term);
    assert_eq!(
        serde_json::to_vec(&Value::from(&term)).unwrap(),
        expected.as_bytes(),
        "genuine retained Serde representation stays identical"
    );
}
#[test]
fn json_retained_term_options_direction_forms_and_verbatim_strings() {
    let expected = r#"{"Literal":{"lexical_form":"literal","datatype":"arbitrary datatype","language":null,"direction":null}}"#;
    for input in [
        expected,
        r#"{"Literal":{"lexical_form":"literal","datatype":"arbitrary datatype"}}"#,
    ] {
        let decoded: Value = from_slice(input.as_bytes()).unwrap();
        assert_eq!(to_vec(&decoded), expected.as_bytes());
    }
    for (tag, direction) in [
        ("Ltr", RdfTextDirection::Ltr),
        ("Rtl", RdfTextDirection::Rtl),
    ] {
        for payload in [format!("\"{tag}\""), format!("{{\"{tag}\":null}}")] {
            let literal =
                expected.replace("\"direction\":null", &format!("\"direction\":{payload}"));
            let decoded: Value = from_slice(literal.as_bytes()).unwrap();
            let term: TermValue = decoded.into();
            match term {
                TermValue::Literal {
                    direction: actual, ..
                } => assert_eq!(actual, Some(direction)),
                other => panic!("wrong retained term {other:?}"),
            };
        }
    }
    let iri: Value = from_slice(br#"{"Iri":"not an IRI"}"#).unwrap();
    assert_eq!(TermValue::from(iri), TermValue::iri("not an IRI"));
    let unknown = expected.replace("\"datatype\":", "\"extra\":[],\"datatype\":");
    assert_eq!(
        to_vec(&from_slice::<Value>(unknown.as_bytes()).unwrap()),
        expected.as_bytes()
    );
    for payload in [
        r#"{"Ltr":false}"#,
        r#"{"Ltr":null,"Rtl":null}"#,
        r#"{"Ltr":null,"Ltr":null}"#,
        r#""Other""#,
        "[]",
        "false",
    ] {
        let bad = expected.replace("\"direction\":null", &format!("\"direction\":{payload}"));
        assert!(from_slice::<Value>(bad.as_bytes()).is_err(), "{bad}");
    }
}
#[test]
fn json_retained_term_blank_scope_uses_strict_u32_tokens_and_nested_pointers() {
    for token in [
        "-0",
        "-1",
        "1.0",
        "1e0",
        "4294967296",
        "18446744073709551616",
        "null",
        "\"1\"",
    ] {
        let text = format!(
            "{{\"Triple\":{{\"s\":{{\"Iri\":\"s\"}},\"p\":{{\"Iri\":\"p\"}},\"o\":{{\"Blank\":{{\"label\":\"b\",\"scope\":{token}}}}}}}}}"
        );
        assert_eq!(
            from_slice::<Value>(text.as_bytes()).unwrap_err().pointer(),
            "/Triple/o/Blank/scope"
        );
    }
    let max: Value = from_slice(br#"{"Blank":{"label":"b","scope":4294967295}}"#).unwrap();
    assert_eq!(
        TermValue::from(max),
        TermValue::Blank {
            label: "b".into(),
            scope: BlankScope(u32::MAX)
        }
    );
}
#[test]
fn json_retained_term_refuses_bad_variants_missing_fields_and_known_duplicates() {
    for input in [
        "{}",
        "[]",
        "null",
        r#""Iri""#,
        r#"{"Iri":null}"#,
        r#"{"Iri":"a","Iri":"b"}"#,
        r#"{"Iri":"a","Blank":{"label":"b","scope":0}}"#,
        r#"{"Other":"a"}"#,
        r#"{"Blank":[]}"#,
        r#"{"Blank":{"label":"b"}}"#,
        r#"{"Blank":{"label":"b","scope":0,"scope":1}}"#,
        r#"{"Literal":{"lexical_form":"x"}}"#,
        r#"{"Literal":{"lexical_form":"x","datatype":"d","language":null,"language":null}}"#,
        r#"{"Triple":{"s":{"Iri":"s"},"p":{"Iri":"p"}}}"#,
        r#"{"Triple":{"s":{"Iri":"s"},"p":{"Iri":"p"},"o":null}}"#,
    ] {
        assert!(from_slice::<Value>(input.as_bytes()).is_err(), "{input}");
    }
    assert!(from_slice::<Value>(br#"{"Iri":"a"} null"#).is_err());
}
