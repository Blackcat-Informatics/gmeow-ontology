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

#[test]
fn rational_interpretation_does_not_repair_a_malformed_decimal() {
    assert!(rational(".").is_none());
    assert!(rational("+.").is_none());
}
