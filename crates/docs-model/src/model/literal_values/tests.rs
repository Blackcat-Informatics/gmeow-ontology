// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Authored typed field records for the existing model envelope only.

use super::*;

#[derive(Debug, Serialize, Deserialize, PartialEq)]
struct Envelope {
    #[serde(with = "super")]
    values: Vec<RdfLiteral>,
}

#[test]
fn cq_rationale_literal_transport_has_complete_typed_fields_and_exact_utf8() {
    let literal = RdfLiteral {
        lexical_form: "nul\0\n中😀".to_owned(),
        datatype: None,
        language: Some("ar".to_owned()),
        direction: Some(RdfTextDirection::Rtl),
    };
    let envelope = Envelope {
        values: vec![literal],
    };
    let expected = "{\"values\":[{\"lexical_form\":\"nul\\u0000\\n中😀\",\"datatype\":null,\"language\":\"ar\",\"direction\":\"rtl\"}]}";
    assert_eq!(serde_json::to_string(&envelope).unwrap(), expected);
    assert_eq!(
        serde_json::from_str::<Envelope>(expected).unwrap(),
        envelope
    );
}

#[test]
fn cq_rationale_literal_transport_refuses_inconsistent_language_direction_and_datatype() {
    for literal in [
        r#"{"lexical_form":"x","datatype":null,"language":null,"direction":"rtl"}"#,
        r#"{"lexical_form":"x","datatype":null,"language":"en","direction":"sideways"}"#,
        r#"{"lexical_form":"x","datatype":"http://www.w3.org/2001/XMLSchema#string","language":"en","direction":null}"#,
        r#"{"lexical_form":"x","datatype":"http://www.w3.org/1999/02/22-rdf-syntax-ns#langString","language":null,"direction":null}"#,
        r#"{"lexical_form":"x","datatype":"relative","language":null,"direction":null}"#,
        r#"{"lexical_form":"x","datatype":null,"language":"","direction":null}"#,
        r#"{"lexical_form":null,"datatype":null,"language":null,"direction":null}"#,
        r#"{"lexical_form":"x","lexical_form":"y","datatype":null,"language":null,"direction":null}"#,
    ] {
        assert!(
            serde_json::from_str::<Envelope>(&format!("{{\"values\":[{literal}]}}")).is_err(),
            "{literal}"
        );
    }
    let malformed = Envelope {
        values: vec![RdfLiteral {
            lexical_form: "x".to_owned(),
            datatype: None,
            language: None,
            direction: Some(RdfTextDirection::Ltr),
        }],
    };
    assert!(serde_json::to_vec(&malformed).is_err());
    let error = validate(&malformed.values[0]).unwrap_err();
    assert!(matches!(
        error.downcast_ref::<LiteralTransportError>(),
        Some(LiteralTransportError::Components(_))
    ));

    let invalid_datatype = Envelope {
        values: vec![RdfLiteral {
            lexical_form: "x".to_owned(),
            datatype: Some("relative".to_owned()),
            language: None,
            direction: None,
        }],
    };
    assert!(serde_json::to_vec(&invalid_datatype).is_err());
    let error = validate(&invalid_datatype.values[0]).unwrap_err();
    let typed = error
        .downcast_ref::<LiteralTransportError>()
        .expect("literal validation retains its typed transport refusal");
    assert!(
        std::error::Error::source(typed)
            .expect("datatype refusal retains the native IRI cause")
            .is::<purrdf_core::IriError>()
    );
}
