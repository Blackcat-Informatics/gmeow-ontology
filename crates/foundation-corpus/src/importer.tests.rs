// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use serde_json::json;

// -- canonical_decimal ------------------------------------------------- //

#[test]
fn canonical_decimal_canonicalizes() {
    assert_eq!(canonical_decimal(0.9), "0.9");
    assert_eq!(canonical_decimal(0.4), "0.4");
    assert_eq!(canonical_decimal(1.0), "1");
    // Both signed and unsigned zero must canonicalize to "0" (no "-0"):
    // XSD-decimal has no signed zero, and "zeros are scores" here.
    assert_eq!(canonical_decimal(0.0), "0");
    assert_eq!(canonical_decimal(-0.0), "0");
}

// -- happy-path -------------------------------------------------------- //

#[test]
fn parse_score_accepts_json_number_float() {
    let v = json!(0.9_f64);
    let result = parse_score("P1", &v).expect("should parse");
    assert!((result - 0.9).abs() < 1e-9);
}

#[test]
fn parse_score_accepts_json_number_integer() {
    let v = json!(1_i64);
    let result = parse_score("P2", &v).expect("should parse");
    assert!((result - 1.0).abs() < 1e-9);
}

#[test]
fn parse_score_accepts_json_number_zero() {
    // Zero is a real score in this corpus — must not be silently swallowed.
    let v = json!(0_i64);
    let result = parse_score("P3", &v).expect("zero is a valid score");
    assert_eq!(result, 0.0);
}

#[test]
fn parse_score_accepts_numeric_string() {
    // Python `float("0.9")` succeeds; so must the Rust port.
    let v = json!("0.9");
    let result = parse_score("P4", &v).expect("numeric string should parse");
    assert!((result - 0.9).abs() < 1e-9);
}

#[test]
fn parse_score_numeric_string_parity_with_number() {
    // The graph value produced for the string "0.9" must equal the value
    // produced for the number 0.9 (the parity case from the reviewer).
    let as_number = parse_score("P5", &json!(0.9_f64)).expect("number");
    let as_string = parse_score("P5", &json!("0.9")).expect("string");
    let lex_number = canonical_decimal(as_number);
    let lex_string = canonical_decimal(as_string);
    assert_eq!(
        lex_number, lex_string,
        "canonical_decimal of 0.9 (number) vs \"0.9\" (string) must match"
    );
}

#[test]
fn parse_score_accepts_trimmed_numeric_string() {
    // Python float() accepts leading/trailing whitespace.
    let v = json!("  0.5  ");
    let result = parse_score("P6", &v).expect("trimmed numeric string");
    assert!((result - 0.5).abs() < 1e-9);
}

// -- hard-fail cases --------------------------------------------------- //

#[test]
fn parse_score_rejects_non_numeric_string() {
    let v = json!("not-a-number");
    let err = parse_score("P7", &v).expect_err("should fail");
    assert_eq!(err.kind(), io::ErrorKind::InvalidData);
    assert!(err.to_string().contains("P7"));
}

#[test]
fn parse_score_rejects_bool_true() {
    // JSON booleans are NOT numeric in this corpus (Python float(True) = 1.0,
    // but the corpus never sends bools; reject to surface data rot early).
    let v = json!(true);
    let err = parse_score("P8", &v).expect_err("bool should be rejected");
    assert_eq!(err.kind(), io::ErrorKind::InvalidData);
    assert!(err.to_string().contains("P8"));
}

#[test]
fn parse_score_rejects_null() {
    let v = json!(null);
    let err = parse_score("P9", &v).expect_err("null should be rejected");
    assert_eq!(err.kind(), io::ErrorKind::InvalidData);
    assert!(err.to_string().contains("P9"));
}

#[test]
fn parse_score_rejects_object() {
    let v = json!({"nested": 0.9});
    let err = parse_score("P10", &v).expect_err("object should be rejected");
    assert_eq!(err.kind(), io::ErrorKind::InvalidData);
    assert!(err.to_string().contains("P10"));
}

#[test]
fn parse_score_rejects_array() {
    let v = json!([0.9]);
    let err = parse_score("P11", &v).expect_err("array should be rejected");
    assert_eq!(err.kind(), io::ErrorKind::InvalidData);
    assert!(err.to_string().contains("P11"));
}
