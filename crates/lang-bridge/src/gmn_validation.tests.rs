// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use purrdf_lex::json;

fn decode(text: &str) -> Result<GmnValidationVerdict, DecodeError> {
    json::record::from_slice(text.as_bytes())
}

#[test]
fn json_gmn_verdict_success_keeps_literal_compact_bytes() {
    let verdict = GmnValidationVerdict {
        conformant: true,
        detail: None,
        failure_class: None,
    };
    let bytes = json::write_compact(&verdict.to_json());
    assert_eq!(bytes, r#"{"conformant":true}"#);
    assert!(!bytes.ends_with('\n'));
    assert_eq!(decode(&bytes).unwrap(), verdict);
}

#[test]
fn json_gmn_verdict_failure_keeps_declaration_order_unicode_and_escaping() {
    let verdict = GmnValidationVerdict {
        conformant: false,
        detail: Some("é 猫\n\t\"\\\u{0001}".to_owned()),
        failure_class: Some("urn:failure:未覆盖".to_owned()),
    };
    let bytes = json::write_compact(&verdict.to_json());
    assert_eq!(
        bytes,
        "{\"conformant\":false,\"detail\":\"é 猫\\n\\t\\\"\\\\\\u0001\",\"failureClass\":\"urn:failure:未覆盖\"}"
    );
    assert_eq!(decode(&bytes).unwrap(), verdict);
}

#[test]
fn json_gmn_verdict_optionals_are_independently_omitted() {
    for (detail, failure_class, expected) in [
        (None, None, r#"{"conformant":false}"#),
        (Some(""), None, r#"{"conformant":false,"detail":""}"#),
        (
            None,
            Some("urn:failure"),
            r#"{"conformant":false,"failureClass":"urn:failure"}"#,
        ),
        (
            Some("detail"),
            Some("urn:failure"),
            r#"{"conformant":false,"detail":"detail","failureClass":"urn:failure"}"#,
        ),
    ] {
        let verdict = GmnValidationVerdict {
            conformant: false,
            detail: detail.map(str::to_owned),
            failure_class: failure_class.map(str::to_owned),
        };
        assert_eq!(json::write_compact(&verdict.to_json()), expected);
        assert_eq!(decode(expected).unwrap(), verdict);
    }
}

#[test]
fn json_gmn_verdict_missing_and_null_optionals_decode_to_none() {
    let expected = GmnValidationVerdict {
        conformant: true,
        detail: None,
        failure_class: None,
    };
    for text in [
        r#"{"conformant":true}"#,
        r#"{"conformant":true,"detail":null}"#,
        r#"{"conformant":true,"failureClass":null}"#,
        r#"{"failureClass":null,"detail":null,"conformant":true}"#,
    ] {
        assert_eq!(decode(text).unwrap(), expected, "{text}");
        assert_eq!(
            json::write_compact(&decode(text).unwrap().to_json()),
            r#"{"conformant":true}"#
        );
    }
}

#[test]
fn json_gmn_verdict_unknown_fields_are_ignored_without_changing_known_values() {
    let text = r#"{"unknown":{"nested":[1,null,true]},"conformant":false,"detail":"bad","failureClass":"urn:failure","unknown":false,"failure_class":"ignored spelling"}"#;
    let verdict = decode(text).unwrap();
    assert_eq!(
        verdict,
        GmnValidationVerdict {
            conformant: false,
            detail: Some("bad".to_owned()),
            failure_class: Some("urn:failure".to_owned()),
        }
    );
    assert_eq!(
        json::write_compact(&verdict.to_json()),
        r#"{"conformant":false,"detail":"bad","failureClass":"urn:failure"}"#
    );
}

#[test]
fn json_gmn_verdict_known_duplicates_are_refused_even_when_equal_or_null() {
    for (field, text) in [
        ("conformant", r#"{"conformant":true,"conformant":true}"#),
        ("conformant", r#"{"conformant":true,"conformant":false}"#),
        (
            "detail",
            r#"{"conformant":false,"detail":null,"detail":"bad"}"#,
        ),
        (
            "detail",
            r#"{"conformant":false,"detail":null,"detail":null}"#,
        ),
        (
            "failureClass",
            r#"{"conformant":false,"failureClass":"urn:a","failureClass":"urn:a"}"#,
        ),
        (
            "failureClass",
            r#"{"conformant":false,"failureClass":null,"failureClass":"urn:a"}"#,
        ),
    ] {
        let error = decode(text).unwrap_err().to_string();
        assert!(error.contains("duplicate field"), "{text}: {error}");
        assert!(error.contains(field), "{text}: {error}");
    }
}

#[test]
fn json_gmn_verdict_required_boolean_and_optional_string_types_are_enforced() {
    for text in [
        r#"{"conformant":null}"#,
        r#"{"conformant":0}"#,
        r#"{"conformant":"true"}"#,
        r#"{"conformant":[]}"#,
        r#"{"conformant":{}}"#,
    ] {
        let error = decode(text).unwrap_err().to_string();
        assert!(error.contains("conformant"), "{text}: {error}");
    }
    for field in ["detail", "failureClass"] {
        for token in ["true", "1", "[]", "{}"] {
            let text = format!("{{\"conformant\":false,\"{field}\":{token}}}");
            let error = decode(&text).unwrap_err().to_string();
            assert!(error.contains(field), "{text}: {error}");
        }
    }
    let error = decode(r#"{"detail":"bad","failureClass":"urn:failure"}"#)
        .unwrap_err()
        .to_string();
    assert!(error.contains("missing field"));
    assert!(error.contains("conformant"));
}

#[test]
fn json_gmn_verdict_only_object_records_and_complete_json_are_admitted() {
    for text in [
        "null",
        "true",
        "1",
        "\"verdict\"",
        "[]",
        "[true]",
        "[true,null,null]",
    ] {
        let error = decode(text).unwrap_err().to_string();
        assert!(error.contains("verdict object"), "{text}: {error}");
    }
    for text in [
        "",
        "{",
        r#"{"conformant":true,}"#,
        r#"{"conformant":TRUE}"#,
        r#"{"conformant":true} false"#,
        r#"{"conformant":true}{}"#,
    ] {
        assert!(decode(text).is_err(), "{text}");
    }
}
