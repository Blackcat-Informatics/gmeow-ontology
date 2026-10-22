// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use purrdf_lex::json::record::{from_slice, to_vec};

#[test]
fn json_math_lowering_errors_all_variants_pin_exact_struct_payload_bytes() {
    use MathLoweringError::*;
    let cases = [
        (
            NumberLiteralMissingValue { node: "n".into() },
            r#"{"NumberLiteralMissingValue":{"node":"n"}}"#,
        ),
        (
            UnrecognizedExpressionType {
                node: "n".into(),
                types: vec!["z".into(), "a".into(), "z".into()],
            },
            r#"{"UnrecognizedExpressionType":{"node":"n","types":["z","a","z"]}}"#,
        ),
        (
            ArgumentSlotMissingIndex { slot: "s".into() },
            r#"{"ArgumentSlotMissingIndex":{"slot":"s"}}"#,
        ),
        (
            ArgumentSlotMultipleIndexes {
                slot: "s".into(),
                count: 2,
            },
            r#"{"ArgumentSlotMultipleIndexes":{"slot":"s","count":2}}"#,
        ),
        (
            ArgumentSlotIndexNotInteger {
                slot: "s".into(),
                lexical: "中\n\"\\".into(),
            },
            r#"{"ArgumentSlotIndexNotInteger":{"slot":"s","lexical":"中\n\"\\"}}"#,
        ),
        (
            ArgumentSlotMissingExpression { slot: "s".into() },
            r#"{"ArgumentSlotMissingExpression":{"slot":"s"}}"#,
        ),
        (
            NonContiguousArgumentSlots {
                node: "n".into(),
                index: 3,
                expected_position: 1,
            },
            r#"{"NonContiguousArgumentSlots":{"node":"n","index":3,"expected_position":1}}"#,
        ),
        (
            DuplicateArgumentSlotIndex {
                node: "n".into(),
                index: 0,
            },
            r#"{"DuplicateArgumentSlotIndex":{"node":"n","index":0}}"#,
        ),
        (
            NegativeArgumentSlotIndex {
                node: "n".into(),
                slot: "s".into(),
                index: -1,
            },
            r#"{"NegativeArgumentSlotIndex":{"node":"n","slot":"s","index":-1}}"#,
        ),
        (
            ApplicationMissingOperator { node: "n".into() },
            r#"{"ApplicationMissingOperator":{"node":"n"}}"#,
        ),
        (
            ApplicationMultipleOperators {
                node: "n".into(),
                count: 2,
            },
            r#"{"ApplicationMultipleOperators":{"node":"n","count":2}}"#,
        ),
        (
            BindingMissingOperator { node: "n".into() },
            r#"{"BindingMissingOperator":{"node":"n"}}"#,
        ),
        (
            BindingMultipleOperators {
                node: "n".into(),
                count: 2,
            },
            r#"{"BindingMultipleOperators":{"node":"n","count":2}}"#,
        ),
        (
            BindingMissingBoundVariable { node: "n".into() },
            r#"{"BindingMissingBoundVariable":{"node":"n"}}"#,
        ),
        (
            BindingMissingBody { node: "n".into() },
            r#"{"BindingMissingBody":{"node":"n"}}"#,
        ),
        (
            BindingMultipleBoundVariables {
                node: "n".into(),
                count: 2,
            },
            r#"{"BindingMultipleBoundVariables":{"node":"n","count":2}}"#,
        ),
        (
            VariableExpressionMissingOccurrence { node: "n".into() },
            r#"{"VariableExpressionMissingOccurrence":{"node":"n"}}"#,
        ),
        (
            VariableExpressionMultipleOccurrences {
                node: "n".into(),
                count: 2,
            },
            r#"{"VariableExpressionMultipleOccurrences":{"node":"n","count":2}}"#,
        ),
        (
            OccurrenceMissingDeclaredVariable {
                occurrence: "o".into(),
            },
            r#"{"OccurrenceMissingDeclaredVariable":{"occurrence":"o"}}"#,
        ),
        (
            OccurrenceMultipleDeclaredVariables {
                occurrence: "o".into(),
                count: 2,
            },
            r#"{"OccurrenceMultipleDeclaredVariables":{"occurrence":"o","count":2}}"#,
        ),
        (
            UnscopedOccurrence {
                occurrence: "o".into(),
                declaration: "d".into(),
            },
            r#"{"UnscopedOccurrence":{"occurrence":"o","declaration":"d"}}"#,
        ),
        (
            CyclicExpressionGraph { node: "n".into() },
            r#"{"CyclicExpressionGraph":{"node":"n"}}"#,
        ),
        (
            ExpressionDepthExceeded {
                node: "n".into(),
                depth: 7,
            },
            r#"{"ExpressionDepthExceeded":{"node":"n","depth":7}}"#,
        ),
    ];
    assert_eq!(cases.len(), 23);
    for (expected, literal) in cases {
        assert_eq!(to_vec(&expected), literal.as_bytes());
        assert_eq!(
            from_slice::<MathLoweringError>(literal.as_bytes()).unwrap(),
            expected
        );
    }
}
#[test]
fn json_math_lowering_errors_i128_extremes_and_signed_zero_follow_integer_owner() {
    for (token, expected) in [
        ("-170141183460469231731687303715884105728", i128::MIN),
        ("170141183460469231731687303715884105727", i128::MAX),
        ("-0", 0),
    ] {
        let text =
            format!("{{\"DuplicateArgumentSlotIndex\":{{\"node\":\"n\",\"index\":{token}}}}}");
        let decoded: MathLoweringError = from_slice(text.as_bytes()).unwrap();
        assert_eq!(
            decoded,
            MathLoweringError::DuplicateArgumentSlotIndex {
                node: "n".into(),
                index: expected
            }
        );
        let emitted = String::from_utf8(to_vec(&decoded)).unwrap();
        assert!(emitted.contains(&format!("\"index\":{expected}")));
    }
    for token in [
        "170141183460469231731687303715884105728",
        "-170141183460469231731687303715884105729",
        "1.0",
        "1e0",
        "null",
        "\"1\"",
    ] {
        let text = format!(
            "{{\"NegativeArgumentSlotIndex\":{{\"node\":\"n\",\"slot\":\"s\",\"index\":{token}}}}}"
        );
        assert_eq!(
            from_slice::<MathLoweringError>(text.as_bytes())
                .unwrap_err()
                .pointer(),
            "/NegativeArgumentSlotIndex/index"
        );
    }
}
#[test]
fn json_math_lowering_errors_unsigned_fields_refuse_coercion_with_variant_pointers() {
    for (tag, field, prefix) in [
        ("ArgumentSlotMultipleIndexes", "count", r#""slot":"s""#),
        ("ApplicationMultipleOperators", "count", r#""node":"n""#),
        ("BindingMultipleOperators", "count", r#""node":"n""#),
        ("BindingMultipleBoundVariables", "count", r#""node":"n""#),
        (
            "VariableExpressionMultipleOccurrences",
            "count",
            r#""node":"n""#,
        ),
        (
            "OccurrenceMultipleDeclaredVariables",
            "count",
            r#""occurrence":"o""#,
        ),
        ("ExpressionDepthExceeded", "depth", r#""node":"n""#),
        (
            "NonContiguousArgumentSlots",
            "expected_position",
            r#""node":"n","index":0"#,
        ),
    ] {
        for token in ["-0", "-1", "1e0", "1.0", "18446744073709551616", "null"] {
            let text = format!("{{\"{tag}\":{{{prefix},\"{field}\":{token}}}}}");
            assert_eq!(
                from_slice::<MathLoweringError>(text.as_bytes())
                    .unwrap_err()
                    .pointer(),
                format!("/{tag}/{field}")
            );
        }
        let text = format!("{{\"{tag}\":{{{prefix},\"{field}\":{}}}}}", usize::MAX);
        let decoded: MathLoweringError = from_slice(text.as_bytes()).unwrap();
        assert_eq!(to_vec(&decoded), text.as_bytes());
    }
}
#[test]
fn json_math_lowering_errors_struct_and_enum_admission_is_exact() {
    let valid = r#"{"UnscopedOccurrence":{"occurrence":"o","declaration":"d"}}"#;
    for invalid in [
        "{}",
        "[]",
        "null",
        r#""BindingMissingBody""#,
        r#"{"Other":{}}"#,
        r#"{"BindingMissingBody":null}"#,
        r#"{"BindingMissingBody":[]}"#,
        r#"{"BindingMissingBody":{}}"#,
        r#"{"BindingMissingBody":{"node":null}}"#,
        r#"{"BindingMissingBody":{"node":"a","node":"b"}}"#,
        r#"{"BindingMissingBody":{"node":"a"},"BindingMissingBody":{"node":"b"}}"#,
        r#"{"BindingMissingBody":{"node":"a"},"CyclicExpressionGraph":{"node":"a"}}"#,
    ] {
        assert!(
            from_slice::<MathLoweringError>(invalid.as_bytes()).is_err(),
            "{invalid}"
        );
    }
    let unknown = valid.replace("\"declaration\":", "\"extra\":null,\"declaration\":");
    assert_eq!(
        to_vec(&from_slice::<MathLoweringError>(unknown.as_bytes()).unwrap()),
        valid.as_bytes()
    );
    assert!(from_slice::<MathLoweringError>(format!("{valid} {{}}").as_bytes()).is_err());
}
