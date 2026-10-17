// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

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
