// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

#[test]
fn native_xsd_admission_does_not_repair_source_whitespace() {
    use purrdf::xsd::datatype::{XSD_BOOLEAN, XSD_DATE_TIME, XSD_INTEGER};
    let mut values = NativeValues::default();
    for (lexical, datatype) in [
        ("1", XSD_DECIMAL),
        ("1", XSD_INTEGER),
        ("true", XSD_BOOLEAN),
        ("2026-10-17T12:00:00Z", XSD_DATE_TIME),
    ] {
        let valid = TermValue::typed_literal(lexical, datatype);
        assert_eq!(values.literal(&valid).named_datatype(datatype), Some(true));
        for boundary in [" ", "\t", "\r\n", "\u{a0}", "\u{2003}", "\u{202f}"] {
            let source =
                TermValue::typed_literal(format!("{boundary}{lexical}{boundary}"), datatype);
            let parsed = values.literal(&source);
            assert!(matches!(parsed.meaning, LiteralMeaning::Opaque(_)));
            assert_eq!(parsed.named_datatype(datatype), None);
            assert_eq!(values.same_value(&source, &valid), None);
        }
    }
}

#[test]
fn native_string_admission_preserves_valid_bytes_and_rejects_non_xml_characters() {
    use purrdf::xsd::datatype::XSD_STRING;
    let mut values = NativeValues::default();
    let text = " \té\r\n 文\u{a0} ";
    let valid = TermValue::typed_literal(text, XSD_STRING);
    let parsed = values.literal(&valid);
    assert!(
        matches!(&parsed.meaning, LiteralMeaning::Native(XsdValue::String(value)) if value == text)
    );
    assert_eq!(parsed.named_datatype(XSD_STRING), Some(true));
    for lexical in ["invalid\0", "\u{fffe}", "\u{ffff}"] {
        let invalid = values.literal(&TermValue::typed_literal(lexical, XSD_STRING));
        assert!(matches!(invalid.meaning, LiteralMeaning::Opaque(_)));
        assert_eq!(invalid.named_datatype(XSD_STRING), None);
        assert_eq!(invalid.facet_length(), None);
    }
    for lexical in ["\u{1}", "\u{b}", "\u{c}", "\u{1f}"] {
        let valid = values.literal(&TermValue::typed_literal(lexical, XSD_STRING));
        assert!(
            matches!(&valid.meaning, LiteralMeaning::Native(XsdValue::String(value)) if value == lexical)
        );
        assert_eq!(valid.named_datatype(XSD_STRING), Some(true));
    }
}

#[test]
fn native_any_uri_values_preserve_xml_whitespace_and_reject_invalid_characters() {
    use purrdf::xsd::datatype::{XSD_ANY_URI, XSD_STRING};
    let mut values = NativeValues::default();
    let alias = TermValue::typed_literal(" \té\r\n 文 ", XSD_ANY_URI);
    let uri = TermValue::typed_literal("é 文", XSD_ANY_URI);
    assert_eq!(values.same_value(&alias, &uri), Some(true));
    let parsed = values.literal(&alias);
    assert!(matches!(
        parsed.meaning,
        LiteralMeaning::Native(XsdValue::AnyUri(_))
    ));
    assert_eq!(parsed.named_datatype(XSD_ANY_URI), Some(true));
    assert_eq!(parsed.named_datatype(XSD_STRING), Some(false));
    assert_eq!(parsed.facet_length(), Some(3));
    assert_eq!(
        values.same_value(&uri, &TermValue::typed_literal("é%20文", XSD_ANY_URI)),
        Some(false)
    );
    assert_eq!(
        values.same_value(
            &uri,
            &TermValue::typed_literal("\u{a0}é 文\u{a0}", XSD_ANY_URI)
        ),
        Some(false)
    );
    let invalid = values.literal(&TermValue::typed_literal("invalid\0", XSD_ANY_URI));
    assert_eq!(invalid.named_datatype(XSD_ANY_URI), None);
    assert_eq!(invalid.facet_length(), None);
}

#[test]
fn cardinality_accepts_the_complete_native_count_domain() {
    let term = TermValue::Literal {
        lexical_form: u128::MAX.to_string(),
        datatype: "http://www.w3.org/2001/XMLSchema#integer".into(),
        language: None,
        direction: None,
    };
    assert_eq!(NativeValues::parse_cardinality(&term), Some(u128::MAX));
    let too_large = TermValue::Literal {
        lexical_form: "340282366920938463463374607431768211456".into(),
        datatype: "http://www.w3.org/2001/XMLSchema#integer".into(),
        language: None,
        direction: None,
    };
    assert_eq!(NativeValues::parse_cardinality(&too_large), None);
    for lexical in ["-0", "+000"] {
        let zero = TermValue::Literal {
            lexical_form: lexical.into(),
            datatype: "http://www.w3.org/2001/XMLSchema#integer".into(),
            language: None,
            direction: None,
        };
        assert_eq!(NativeValues::parse_cardinality(&zero), Some(0));
    }
}

#[test]
fn native_literal_membership_and_equality_preserve_large_exact_values() {
    let integer = TermValue::Literal {
        lexical_form: "340282366920938463463374607431768211507".into(),
        datatype: "http://www.w3.org/2001/XMLSchema#integer".into(),
        language: None,
        direction: None,
    };
    let rational = TermValue::Literal {
        lexical_form: "340282366920938463463374607431768211507/1".into(),
        datatype: OWL_RATIONAL.into(),
        language: None,
        direction: None,
    };
    let mut values = NativeValues::default();
    assert_eq!(values.same_value(&integer, &rational), Some(true));
    let value = values.literal(&integer);
    assert_eq!(
        value.named_datatype("http://www.w3.org/2001/XMLSchema#integer"),
        Some(true)
    );
    assert_eq!(
        value.named_datatype("http://www.w3.org/2001/XMLSchema#long"),
        Some(false)
    );
    assert_eq!(value.named_datatype(XSD_DECIMAL), Some(true));
}

/// A rejected decimal never acquires a value through another datatype grammar.
#[test]
fn native_literal_interpretation_does_not_repair_a_malformed_decimal() {
    let mut values = NativeValues::default();
    for lexical in [".", "+.", "1/2", "1/3", "-1/2", "1e2"] {
        let decimal = TermValue::typed_literal(lexical, XSD_DECIMAL);
        let rational = TermValue::typed_literal("1/2", OWL_RATIONAL);
        let value = values.literal(&decimal);
        assert_eq!(value.named_datatype(XSD_DECIMAL), None, "{lexical}");
        assert_eq!(value.named_datatype(OWL_RATIONAL), None, "{lexical}");
        assert_eq!(values.same_value(&decimal, &rational), None, "{lexical}");
    }
}

/// Native exact decimals retain precision beyond the obsolete fixed-scale limit.
#[test]
fn native_exact_decimal_interpretation_keeps_arbitrary_scale_without_a_retry() {
    let decimal = TermValue::typed_literal(format!("0.{}1", "0".repeat(79)), XSD_DECIMAL);
    let rational = TermValue::typed_literal(format!("1/1{}", "0".repeat(80)), OWL_RATIONAL);
    let mut values = NativeValues::default();
    assert_eq!(values.same_value(&decimal, &rational), Some(true));
    assert_eq!(
        values.literal(&decimal).named_datatype(XSD_DECIMAL),
        Some(true)
    );
}
