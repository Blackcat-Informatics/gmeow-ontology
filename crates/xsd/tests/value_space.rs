// SPDX-FileCopyrightText: 2026 Blackcat Informatics Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! XSD value-space conformance vectors (acceptance artifact #1 for #907):
//! per-datatype lexical → value → canonical round-trips, the parse-by-IRI contract,
//! the zero-dep numeric bounds, and the partial-order edge cases.

use gmeow_xsd::{parse, parse_by_iri, value_cmp, XsdDatatype as D, XsdError, XsdValue};
use std::cmp::Ordering;

/// (lexical, datatype, expected canonical lexical).
const CANONICAL_VECTORS: &[(&str, D, &str)] = &[
    // integer
    ("42", D::Integer, "42"),
    ("+007", D::Integer, "7"),
    ("-0", D::Integer, "0"),
    // decimal
    ("12.00", D::Decimal, "12.0"),
    (".5", D::Decimal, "0.5"),
    ("100", D::Decimal, "100.0"),
    ("-0.250", D::Decimal, "-0.25"),
    // double / float
    ("1.0E2", D::Double, "1.0E2"),
    ("0.005", D::Double, "5.0E-3"),
    ("INF", D::Double, "INF"),
    ("-INF", D::Float, "-INF"),
    ("NaN", D::Double, "NaN"),
    // boolean
    ("1", D::Boolean, "true"),
    ("false", D::Boolean, "false"),
    // string (lexical == value)
    ("héllo", D::String, "héllo"),
    // dateTime / date / time
    (
        "2024-02-29T12:30:00.500Z",
        D::DateTime,
        "2024-02-29T12:30:00.5Z",
    ),
    (
        "2024-02-29T00:00:00+00:00",
        D::DateTime,
        "2024-02-29T00:00:00Z",
    ),
    ("2024-02-29-05:00", D::Date, "2024-02-29-05:00"),
    ("12:00:00", D::Time, "12:00:00"),
    // duration + subtypes
    ("P1Y2M3DT4H5M6S", D::Duration, "P1Y2M3DT4H5M6S"),
    ("PT1.500S", D::DayTimeDuration, "PT1.5S"),
    ("P14M", D::YearMonthDuration, "P1Y2M"),
];

#[test]
fn canonical_lexical_round_trips() {
    for (lexical, dt, expected) in CANONICAL_VECTORS {
        let value = parse(lexical, *dt)
            .unwrap_or_else(|e| panic!("parse({lexical:?}, {dt:?}) failed: {e}"));
        assert_eq!(
            value.canonical_lexical(),
            *expected,
            "canonical_lexical({lexical:?}^^{dt:?})"
        );
        assert_eq!(value.datatype(), *dt, "datatype({lexical:?}^^{dt:?})");
    }
}

#[test]
fn canonical_is_idempotent() {
    // Re-parsing the canonical form yields the same canonical form.
    for (lexical, dt, _) in CANONICAL_VECTORS {
        let once = parse(lexical, *dt).unwrap().canonical_lexical();
        let twice = parse(&once, *dt).unwrap().canonical_lexical();
        assert_eq!(once, twice, "idempotent canonical for {lexical:?}^^{dt:?}");
    }
}

#[test]
fn parse_by_iri_contract() {
    // A known XSD value-space datatype IRI parses.
    let v = parse_by_iri("42", "http://www.w3.org/2001/XMLSchema#integer").unwrap();
    assert!(matches!(v, Some(XsdValue::Integer(42))));
    // A non-XSD datatype IRI is Ok(None) — caller treats as a plain term.
    // (XsdValue has no PartialEq by design, so assert on `is_none`.)
    assert!(parse_by_iri(
        "hi",
        "http://www.w3.org/1999/02/22-rdf-syntax-ns#langString"
    )
    .unwrap()
    .is_none());
    // An XSD datatype with a malformed lexical is Err (NOT None).
    assert!(parse_by_iri("nope", "http://www.w3.org/2001/XMLSchema#integer").is_err());
}

#[test]
fn numeric_bounds_are_hard_failed_not_saturated() {
    // i128::MAX round-trips.
    let max = i128::MAX.to_string();
    assert!(matches!(parse(&max, D::Integer), Ok(XsdValue::Integer(i)) if i == i128::MAX));
    // i128::MAX + 1 is a hard OutOfRange error, not a saturated value.
    let overflow = "170141183460469231731687303715884105728";
    assert!(matches!(
        parse(overflow, D::Integer),
        Err(XsdError::OutOfRange { .. })
    ));
    // NOTE: corpus-range exposure (that our actual literals stay within i128 /
    // scale-18) is proven downstream at S5 #911 / S6 #912 integration; this test
    // proves only that the bound itself hard-fails.
}

#[test]
fn partial_order_edge_cases() {
    let nan = parse("NaN", D::Double).unwrap();
    assert_eq!(value_cmp(&nan, &nan), None, "NaN is unordered");

    let p1m = parse("P1M", D::Duration).unwrap();
    let p30d = parse("P30D", D::Duration).unwrap();
    assert_eq!(value_cmp(&p1m, &p30d), None, "P1M vs P30D indeterminate");

    let no_tz = parse("2024-01-01T12:00:00", D::DateTime).unwrap();
    let tzd = parse("2024-01-01T12:00:00Z", D::DateTime).unwrap();
    assert_eq!(value_cmp(&no_tz, &tzd), None, "tz-indeterminate dateTime");

    // Cross-family is incomparable, never a silent ordering.
    let int = parse("1", D::Integer).unwrap();
    let s = parse("1", D::String).unwrap();
    assert_eq!(value_cmp(&int, &s), None);
}

#[test]
fn determinate_orderings() {
    let a = parse("1", D::Integer).unwrap();
    let b = parse("1.5", D::Decimal).unwrap();
    assert_eq!(value_cmp(&a, &b), Some(Ordering::Less)); // promotion
    let t1 = parse("2024-01-01T00:00:00Z", D::DateTime).unwrap();
    let t2 = parse("2024-01-01T01:00:00+01:00", D::DateTime).unwrap();
    assert_eq!(value_cmp(&t1, &t2), Some(Ordering::Equal)); // same instant
}

// ── Temporal calendar/time validation vectors ────────────────────────────────────

/// Negative: these must all parse as `Err` (out of calendar/time value space).
const TEMPORAL_INVALID: &[(&str, D)] = &[
    // Date: day exceeds month length
    ("2024-02-30", D::Date), // Feb has at most 29 days (2024 is leap)
    ("2023-02-29", D::Date), // 2023 is not a leap year
    ("2024-04-31", D::Date), // April has 30 days
    ("1900-02-29", D::Date), // 1900 is a century non-leap year
    // Same bad dates embedded in dateTime
    ("2024-02-30T00:00:00", D::DateTime),
    ("2023-02-29T12:00:00", D::DateTime),
    ("2024-04-31T00:00:00Z", D::DateTime),
    ("1900-02-29T00:00:00", D::DateTime),
    // Time: XSD has NO leap seconds
    ("23:59:60", D::Time),
    // Time: hour 24 only valid as 24:00:00
    ("24:30:00", D::Time),
    ("24:00:01", D::Time),
    // Time: trailing decimal point in seconds is ill-formed
    ("12:00:00.", D::Time),
    // Same bad times in dateTime
    ("2024-01-01T23:59:60", D::DateTime),
    ("2024-01-01T24:30:00", D::DateTime),
    ("2024-01-01T24:00:01", D::DateTime),
    ("2024-01-01T12:00:00.", D::DateTime),
];

/// Positive: these MUST parse successfully (boundary / edge-case controls).
const TEMPORAL_VALID: &[(&str, D)] = &[
    ("2024-02-29", D::Date),              // 2024 IS a leap year
    ("2000-02-29", D::Date),              // 2000 is a 400-year leap
    ("24:00:00", D::Time),                // end-of-day sentinel — valid
    ("23:59:59.999", D::Time),            // max valid fractional second
    ("2024-02-29T00:00:00", D::DateTime), // leap day in dateTime
    ("2000-02-29T12:00:00Z", D::DateTime),
    ("2024-01-01T24:00:00", D::DateTime), // end-of-day dateTime
];

#[test]
fn temporal_calendar_invalid_lexicals_are_hard_errors() {
    for (lexical, dt) in TEMPORAL_INVALID {
        assert!(
            parse(lexical, *dt).is_err(),
            "expected Err for {lexical:?}^^{dt:?} but got Ok"
        );
    }
}

#[test]
fn temporal_calendar_valid_lexicals_parse_ok() {
    for (lexical, dt) in TEMPORAL_VALID {
        assert!(
            parse(lexical, *dt).is_ok(),
            "expected Ok for {lexical:?}^^{dt:?} but got Err: {:?}",
            parse(lexical, *dt).unwrap_err()
        );
    }
}

// ── Year-width / XSD 1.1 year-zero vectors ──────────────────────────────────────

/// Negative: year field is >4 digits AND starts with '0' — must be Err.
const YEAR_WIDTH_INVALID: &[(&str, D)] = &[
    ("00044-03-15", D::Date),
    ("012345-01-01", D::Date),
    ("00044-03-15T00:00:00", D::DateTime),
    ("012345-01-01T00:00:00", D::DateTime),
];

/// Positive: year-zero (XSD 1.1 1 BCE), negative years, long years without leading
/// zeros, and a 4-digit leading-zero year — all must parse successfully.
const YEAR_WIDTH_VALID: &[(&str, D)] = &[
    ("0000-01-01", D::Date),   // XSD 1.1: 0000 = 1 BCE (forbidden in XSD 1.0)
    ("-0001-01-01", D::Date),  // negative year (1 BCE alt encoding / proleptic)
    ("12345-06-15", D::Date),  // 5-digit year, no leading zero — valid
    ("-12345-06-15", D::Date), // negative 5-digit year, no leading zero — valid
    ("0044-03-15", D::Date),   // exactly 4 digits with leading zero — valid
];

#[test]
fn year_width_invalid_lexicals_are_hard_errors() {
    for (lexical, dt) in YEAR_WIDTH_INVALID {
        assert!(
            parse(lexical, *dt).is_err(),
            "expected Err for {lexical:?}^^{dt:?} but got Ok"
        );
    }
}

#[test]
fn year_width_valid_lexicals_parse_ok() {
    for (lexical, dt) in YEAR_WIDTH_VALID {
        assert!(
            parse(lexical, *dt).is_ok(),
            "expected Ok for {lexical:?}^^{dt:?} but got Err: {:?}",
            parse(lexical, *dt).unwrap_err()
        );
    }
}

mod prop {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        /// Integer canonical form re-parses to the same value and is idempotent.
        #[test]
        fn integer_canonical_idempotent(n in any::<i64>()) {
            let v = parse(&n.to_string(), D::Integer).unwrap();
            let canon = v.canonical_lexical();
            prop_assert_eq!(&canon, &parse(&canon, D::Integer).unwrap().canonical_lexical());
        }

        /// Decimal canonical form is idempotent.
        #[test]
        fn decimal_canonical_idempotent(mantissa in any::<i64>(), scale in 0u8..=6) {
            let lexical = {
                let s = mantissa.unsigned_abs().to_string();
                let sign = if mantissa < 0 { "-" } else { "" };
                let scale = scale as usize;
                if scale == 0 {
                    format!("{sign}{s}")
                } else if s.len() > scale {
                    format!("{sign}{}.{}", &s[..s.len()-scale], &s[s.len()-scale..])
                } else {
                    format!("{sign}0.{}{s}", "0".repeat(scale - s.len()))
                }
            };
            let canon = parse(&lexical, D::Decimal).unwrap().canonical_lexical();
            prop_assert_eq!(&canon, &parse(&canon, D::Decimal).unwrap().canonical_lexical());
        }
    }
}
