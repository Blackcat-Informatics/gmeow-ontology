// SPDX-FileCopyrightText: 2026 Blackcat Informatics Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The value-space operator surface: SPARQL `=` / `<` and the effective boolean
//! value. These are **value-space** operations (distinct from RDF term identity —
//! see the crate docs). They are free functions, not trait impls, so they cannot be
//! confused with the structural `Eq`/`Ord` a `HashMap`/`BTreeMap` would use.

use std::cmp::Ordering;

use crate::numeric::{numeric_cmp, numeric_eq};
use crate::value::XsdValue;

/// SPARQL value-space comparison (`<` / `>` / `=` semantics).
///
/// Returns `None` when the two values are **incomparable** — a `NaN` operand, or two
/// values from different value-space families (e.g. a number vs a string). The
/// evaluator maps `None` to a SPARQL *type error* for the relational operators; it
/// must NOT be read as "not equal".
#[must_use]
pub fn value_cmp(a: &XsdValue, b: &XsdValue) -> Option<Ordering> {
    use XsdValue::{Boolean, Double, Float, Integer, String as Str};
    match (a, b) {
        // Numeric tower (with promotion); covers every numeric/numeric pair.
        (
            Integer(_) | XsdValue::Decimal(_) | Float(_) | Double(_),
            Integer(_) | XsdValue::Decimal(_) | Float(_) | Double(_),
        ) => numeric_cmp(a, b),
        // `false` < `true`.
        (Boolean(x), Boolean(y)) => Some(x.cmp(y)),
        // Codepoint (Unicode scalar) order — SPARQL string ordering.
        (Str(x), Str(y)) => Some(x.cmp(y)),
        // Different value-space families are incomparable.
        _ => None,
    }
}

/// SPARQL value-space equality (`=`). Convenience over [`value_cmp`]; returns
/// `false` for incomparable operands. When the error-vs-false distinction matters
/// (the SPARQL `=` operator raises a type error on incomparable operands), use
/// [`value_cmp`] and treat `None` as the error.
#[must_use]
pub fn value_eq(a: &XsdValue, b: &XsdValue) -> bool {
    use XsdValue::{Boolean, String as Str};
    match (a, b) {
        (Boolean(x), Boolean(y)) => x == y,
        (Str(x), Str(y)) => x == y,
        _ if is_numeric(a) && is_numeric(b) => numeric_eq(a, b),
        _ => false,
    }
}

/// SPARQL Effective Boolean Value (value-space rules).
///
/// `None` means **type error** (the value has no EBV — the evaluator raises), which
/// is distinct from `Some(false)`. A consumer must never read `None` as `false`.
#[must_use]
pub fn effective_boolean_value(v: &XsdValue) -> Option<bool> {
    Some(match v {
        XsdValue::Boolean(b) => *b,
        XsdValue::String(s) => !s.is_empty(),
        XsdValue::Integer(i) => *i != 0,
        XsdValue::Decimal(d) => d.mantissa() != 0,
        XsdValue::Float(f) => !f.is_nan() && *f != 0.0,
        XsdValue::Double(d) => !d.is_nan() && *d != 0.0,
    })
}

fn is_numeric(v: &XsdValue) -> bool {
    matches!(
        v,
        XsdValue::Integer(_) | XsdValue::Decimal(_) | XsdValue::Float(_) | XsdValue::Double(_)
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::value::parse;
    use crate::XsdDatatype::{Boolean, Decimal, Double, Float, Integer, String};

    fn v(lex: &str, dt: crate::XsdDatatype) -> XsdValue {
        parse(lex, dt).unwrap()
    }

    /// The SPARQL operator-mapping table (numeric tower + string + boolean). Each row
    /// asserts `value_cmp` (and hence `=`/`<`/`>`). Temporal rows are added in Task 4.
    #[test]
    fn operator_mapping_table() {
        let eq = Some(Ordering::Equal);
        let lt = Some(Ordering::Less);
        let gt = Some(Ordering::Greater);

        // (lhs_lex, lhs_dt, rhs_lex, rhs_dt, expected value_cmp)
        let rows = [
            ("1", Integer, "1", Integer, eq),
            ("1", Integer, "1.0", Decimal, eq), // promotion
            ("1", Integer, "2", Integer, lt),
            ("2.5", Decimal, "2", Integer, gt),
            ("1", Integer, "1.0E0", Double, eq), // promotion to double
            ("1.5", Decimal, "1.25", Float, gt), // promotion to float
            ("3", Integer, "2.9", Double, gt),
            ("abc", String, "abd", String, lt), // codepoint order
            ("abc", String, "abc", String, eq),
            ("false", Boolean, "true", Boolean, lt),
            ("true", Boolean, "true", Boolean, eq),
            // Cross-family: incomparable.
            ("1", Integer, "1", String, None),
            ("true", Boolean, "1", Integer, None),
        ];
        for (la, da, lb, db, want) in rows {
            assert_eq!(
                value_cmp(&v(la, da), &v(lb, db)),
                want,
                "value_cmp({la:?}^^{da:?}, {lb:?}^^{db:?})"
            );
        }
    }

    #[test]
    fn value_eq_incomparable_is_false_not_error() {
        assert!(value_eq(&v("1", Integer), &v("1.0", Decimal)));
        assert!(!value_eq(&v("1", Integer), &v("1", String)));
        // NaN: not equal, and value_cmp distinguishes the type-error (None).
        let nan = v("NaN", Double);
        assert!(!value_eq(&nan, &nan));
        assert_eq!(value_cmp(&nan, &nan), None);
    }

    #[test]
    fn effective_boolean_values() {
        assert_eq!(effective_boolean_value(&v("true", Boolean)), Some(true));
        assert_eq!(effective_boolean_value(&v("0", Boolean)), Some(false));
        assert_eq!(effective_boolean_value(&v("", String)), Some(false));
        assert_eq!(effective_boolean_value(&v("x", String)), Some(true));
        assert_eq!(effective_boolean_value(&v("0", Integer)), Some(false));
        assert_eq!(effective_boolean_value(&v("5", Integer)), Some(true));
        assert_eq!(effective_boolean_value(&v("NaN", Double)), Some(false));
    }
}
