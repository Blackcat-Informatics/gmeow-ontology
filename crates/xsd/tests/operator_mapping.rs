// SPDX-FileCopyrightText: 2026 Blackcat Informatics Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The SPARQL operator-mapping table (acceptance artifact #2 for #907).
//!
//! Each row asserts `value_cmp(lhs, rhs)`, which is the value-space primitive behind
//! the SPARQL `=`, `<`, `>`, `<=`, `>=` operators (the evaluator derives those and
//! maps the incomparable `None` to a type error). Covers the numeric promotion tower
//! (`integer ⊂ decimal ⊂ float ⊂ double`), string, boolean, and the temporal families.

use gmeow_xsd::{parse, value_cmp, value_eq, XsdDatatype as D};
use std::cmp::Ordering;

const EQ: Option<Ordering> = Some(Ordering::Equal);
const LT: Option<Ordering> = Some(Ordering::Less);
const GT: Option<Ordering> = Some(Ordering::Greater);
const NC: Option<Ordering> = None; // incomparable / type error

/// (lhs_lexical, lhs_dt, rhs_lexical, rhs_dt, expected `value_cmp`).
#[allow(clippy::type_complexity)]
const TABLE: &[(&str, D, &str, D, Option<Ordering>)] = &[
    // ── numeric tower: promotion across types ──
    ("1", D::Integer, "1", D::Integer, EQ),
    ("1", D::Integer, "2", D::Integer, LT),
    ("1", D::Integer, "1.0", D::Decimal, EQ),
    ("3", D::Integer, "2.5", D::Decimal, GT),
    ("1", D::Integer, "1.0E0", D::Double, EQ),
    ("2", D::Integer, "2.5", D::Double, LT),
    ("1.5", D::Decimal, "1.25", D::Float, GT),
    ("1.5", D::Decimal, "1.5", D::Double, EQ),
    ("0.1", D::Float, "0.2", D::Double, LT),
    // ── IEEE specials ──
    ("INF", D::Double, "1.0E0", D::Double, GT),
    ("-INF", D::Double, "0", D::Integer, LT),
    ("NaN", D::Double, "NaN", D::Double, NC),
    ("NaN", D::Double, "1", D::Integer, NC),
    // ── string (codepoint order) ──
    ("abc", D::String, "abc", D::String, EQ),
    ("abc", D::String, "abd", D::String, LT),
    ("Z", D::String, "a", D::String, LT), // 'Z'(0x5A) < 'a'(0x61)
    // ── boolean ──
    ("false", D::Boolean, "true", D::Boolean, LT),
    ("true", D::Boolean, "true", D::Boolean, EQ),
    // ── dateTime / date / time ──
    (
        "2024-01-01T00:00:00Z",
        D::DateTime,
        "2024-01-01T01:00:00+01:00",
        D::DateTime,
        EQ,
    ),
    (
        "2024-01-01T00:00:00Z",
        D::DateTime,
        "2024-01-01T00:00:01Z",
        D::DateTime,
        LT,
    ),
    (
        "2024-01-01T12:00:00",
        D::DateTime,
        "2024-01-01T12:00:00Z",
        D::DateTime,
        NC,
    ),
    ("2023-12-31Z", D::Date, "2024-01-01Z", D::Date, LT),
    ("09:00:00Z", D::Time, "10:00:00Z", D::Time, LT),
    // ── duration partial order ──
    ("P1Y", D::Duration, "P13M", D::Duration, LT),
    ("P1M", D::Duration, "P30D", D::Duration, NC),
    ("PT1H", D::DayTimeDuration, "PT2H", D::DayTimeDuration, LT),
    // ── cross-family: incomparable ──
    ("1", D::Integer, "1", D::String, NC),
    ("true", D::Boolean, "1", D::Integer, NC),
    ("2024-01-01T00:00:00Z", D::DateTime, "P1Y", D::Duration, NC),
    // ── derived-integer cross-subtype equality/comparison ──
    // xsd:int 5 = xsd:long 5
    ("5", D::Int, "5", D::Long, EQ),
    // xsd:byte 5 = xsd:integer 5
    ("5", D::Byte, "5", D::Integer, EQ),
    // xsd:short 3 < xsd:int 4
    ("3", D::Short, "4", D::Int, LT),
    // xsd:unsignedByte 2 < xsd:double 2.5 (cross-tower promotion)
    ("2", D::UnsignedByte, "2.5", D::Double, LT),
];

#[test]
fn operator_mapping_table() {
    for (la, da, lb, db, want) in TABLE {
        let a = parse(la, *da).unwrap_or_else(|e| panic!("parse {la:?}^^{da:?}: {e}"));
        let b = parse(lb, *db).unwrap_or_else(|e| panic!("parse {lb:?}^^{db:?}: {e}"));
        assert_eq!(
            value_cmp(&a, &b),
            *want,
            "value_cmp({la:?}^^{da:?}, {lb:?}^^{db:?})"
        );
        // value_eq agrees with value_cmp == Equal.
        assert_eq!(
            value_eq(&a, &b),
            *want == EQ,
            "value_eq({la:?}^^{da:?}, {lb:?}^^{db:?})"
        );
    }
}

#[test]
fn value_cmp_is_antisymmetric_on_determinate_rows() {
    for (la, da, lb, db, want) in TABLE {
        let a = parse(la, *da).unwrap();
        let b = parse(lb, *db).unwrap();
        let forward = value_cmp(&a, &b);
        let backward = value_cmp(&b, &a);
        match want {
            Some(Ordering::Equal) => assert_eq!(backward, EQ),
            Some(Ordering::Less) => assert_eq!(backward, GT),
            Some(Ordering::Greater) => assert_eq!(backward, LT),
            None => assert_eq!(backward, NC), // incomparable both ways
        }
        let _ = forward;
    }
}

// ── Gregorian operator-mapping rows ──────────────────────────────────────────────

/// Additional Gregorian rows for the operator-mapping table.
#[allow(clippy::type_complexity)]
const GREGORIAN_TABLE: &[(&str, D, &str, D, Option<Ordering>)] = &[
    // same-type ordering
    ("2023", D::GYear, "2024", D::GYear, LT),
    ("--03", D::GMonth, "--11", D::GMonth, LT),
    ("---01", D::GDay, "---15", D::GDay, LT),
    ("2024-01", D::GYearMonth, "2024-05", D::GYearMonth, LT),
    ("--02-01", D::GMonthDay, "--03-01", D::GMonthDay, LT),
    // same value → Equal
    ("2024", D::GYear, "2024", D::GYear, EQ),
    ("--05", D::GMonth, "--05", D::GMonth, EQ),
    // cross-family incomparable
    ("2024", D::GYear, "--05", D::GMonth, NC),
    ("---15", D::GDay, "--02-15", D::GMonthDay, NC),
    // cross with other temporal families
    ("2024", D::GYear, "2024-01-01", D::Date, NC),
];

#[test]
fn gregorian_operator_mapping_table() {
    for (la, da, lb, db, want) in GREGORIAN_TABLE {
        let a = parse(la, *da).unwrap_or_else(|e| panic!("parse {la:?}^^{da:?}: {e}"));
        let b = parse(lb, *db).unwrap_or_else(|e| panic!("parse {lb:?}^^{db:?}: {e}"));
        assert_eq!(
            value_cmp(&a, &b),
            *want,
            "value_cmp({la:?}^^{da:?}, {lb:?}^^{db:?})"
        );
    }
}

#[test]
fn gregorian_effective_boolean_value_is_type_error() {
    use gmeow_xsd::effective_boolean_value;
    // Gregorian values have no EBV — must return None (SPARQL type error).
    let gyear = parse("2024", D::GYear).unwrap();
    assert_eq!(effective_boolean_value(&gyear), None, "gYear has no EBV");
    let gmonth = parse("--05", D::GMonth).unwrap();
    assert_eq!(effective_boolean_value(&gmonth), None, "gMonth has no EBV");
    let gday = parse("---15", D::GDay).unwrap();
    assert_eq!(effective_boolean_value(&gday), None, "gDay has no EBV");
    let gym = parse("2024-05", D::GYearMonth).unwrap();
    assert_eq!(effective_boolean_value(&gym), None, "gYearMonth has no EBV");
    let gmd = parse("--02-29", D::GMonthDay).unwrap();
    assert_eq!(effective_boolean_value(&gmd), None, "gMonthDay has no EBV");
}
