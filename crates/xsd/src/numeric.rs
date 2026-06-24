// SPDX-FileCopyrightText: 2026 Blackcat Informatics Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The XSD numeric value space: `integer`, `decimal`, `float`, `double`, their
//! lexical↔value parsing + canonical mapping, and the SPARQL numeric promotion
//! lattice (`integer ⊂ decimal ⊂ float ⊂ double`) used for cross-type comparison.

use std::cmp::Ordering;

use crate::datatype::XsdDatatype;
use crate::value::{XsdError, XsdValue};

/// An exact decimal: `value = mantissa × 10^(-scale)`. Mirrors `oxsdatatypes`'
/// `i128`-backed design (scale bounded so the mantissa stays in `i128`).
#[derive(Debug, Clone, Copy)]
pub struct Decimal {
    mantissa: i128,
    scale: u8,
}

/// Max fractional digits we retain; keeps the mantissa within `i128` headroom and
/// matches `oxsdatatypes`' precision.
const MAX_DECIMAL_SCALE: u8 = 18;

impl Decimal {
    /// Construct from raw mantissa + scale (internal/testing).
    #[must_use]
    pub(crate) fn from_parts(mantissa: i128, scale: u8) -> Self {
        Decimal { mantissa, scale }
    }

    /// The mantissa (signed significant digits).
    #[must_use]
    pub fn mantissa(&self) -> i128 {
        self.mantissa
    }

    /// The scale (number of fractional digits).
    #[must_use]
    pub fn scale(&self) -> u8 {
        self.scale
    }

    /// Lossy conversion to `f64` (for promotion to `double`/`float`).
    #[must_use]
    pub fn to_f64(&self) -> f64 {
        self.mantissa as f64 / 10f64.powi(i32::from(self.scale))
    }

    /// The integer (truncated-toward-zero) part of the value.
    #[must_use]
    pub fn whole_part(&self) -> i128 {
        self.mantissa / 10i128.pow(u32::from(self.scale))
    }

    /// The fractional part of the value as a `Decimal` (same scale).
    #[must_use]
    pub fn frac_part(&self) -> Decimal {
        Decimal {
            mantissa: self.mantissa % 10i128.pow(u32::from(self.scale)),
            scale: self.scale,
        }
    }

    /// True if the value is exactly zero.
    #[must_use]
    pub fn is_zero(&self) -> bool {
        self.mantissa == 0
    }

    /// Exact comparison of two decimals (total order — decimals are never NaN).
    ///
    /// ## Overflow-safety argument
    ///
    /// `scale` is a `u8` capped at `MAX_DECIMAL_SCALE` (= 18) at every construction
    /// site (`parse_decimal` enforces `frac_str.len() <= 18`; `frac_part` inherits the
    /// parent scale; `from_parts(_, 0)` for integer promotion is scale 0).
    ///
    /// For the fractional-alignment step the two frac mantissas satisfy:
    ///   `|frac_m| < 10^scale ≤ 10^18`
    /// After scaling to the common (higher) scale we multiply by at most `10^diff`
    /// where `diff ≤ 18`, giving a product `< 10^18 × 10^18 = 10^36`.
    /// `i128::MAX ≈ 1.7 × 10^38 > 10^36`, so the multiplication cannot overflow.
    ///
    /// The integer-part comparison uses `whole_part()` which returns `i128` and is
    /// exact (no multiplication); it is compared directly.
    ///
    /// There is NO `f64` path and NO `unwrap_or` swallowing a failure.
    #[must_use]
    pub fn cmp_exact(&self, other: &Decimal) -> Ordering {
        // Fast path: identical scale — single cmp, no arithmetic needed.
        if self.scale == other.scale {
            return self.mantissa.cmp(&other.mantissa);
        }

        // Step 1 — sign comparison.  Negative < zero < positive.
        let s_sign = self.mantissa.signum();
        let o_sign = other.mantissa.signum();
        if s_sign != o_sign {
            return s_sign.cmp(&o_sign);
        }
        // Both zero (mantissa == 0 regardless of scale) → Equal.
        if s_sign == 0 {
            return Ordering::Equal;
        }

        // Step 2 — integer part comparison (both same sign, non-zero).
        let s_whole = self.whole_part();
        let o_whole = other.whole_part();
        let whole_ord = s_whole.cmp(&o_whole);
        if whole_ord != Ordering::Equal {
            return whole_ord;
        }

        // Step 3 — fractional part comparison.
        // Each frac mantissa satisfies |frac_m| < 10^scale ≤ 10^18.
        // We scale the lower-scale fraction up to the higher scale by multiplying by
        // 10^diff (diff ≤ 18).  Product < 10^18 × 10^18 = 10^36 < i128::MAX → no
        // overflow.  (Debug assertion guards the invariant during development.)
        debug_assert!(
            self.scale <= MAX_DECIMAL_SCALE && other.scale <= MAX_DECIMAL_SCALE,
            "scale invariant violated: self.scale={}, other.scale={}",
            self.scale,
            other.scale,
        );
        let s_frac = self.frac_part().mantissa;
        let o_frac = other.frac_part().mantissa;
        let frac_ord = if self.scale > other.scale {
            let diff = u32::from(self.scale - other.scale);
            // SAFETY: o_frac < 10^other.scale ≤ 10^18; diff ≤ 18; product < 10^36 < i128::MAX
            let o_scaled = o_frac * 10i128.pow(diff);
            s_frac.cmp(&o_scaled)
        } else {
            let diff = u32::from(other.scale - self.scale);
            // SAFETY: s_frac < 10^self.scale ≤ 10^18; diff ≤ 18; product < 10^36 < i128::MAX
            let s_scaled = s_frac * 10i128.pow(diff);
            s_scaled.cmp(&o_frac)
        };
        // For negative numbers the frac mantissas are negative too (they inherit the
        // sign from `mantissa % 10^scale`), so the direct comparison is already
        // correct: a more-negative fraction means a smaller (more negative) value.
        frac_ord
    }

    /// XSD canonical lexical form: decimal point mandatory, no trailing fractional
    /// zeros except the one required to keep a digit after the point.
    #[must_use]
    pub fn canonical_lexical(&self) -> String {
        let neg = self.mantissa < 0;
        let digits = self.mantissa.unsigned_abs().to_string();
        let scale = usize::from(self.scale);

        let (int_part, frac_part) = if scale == 0 {
            (digits.clone(), String::new())
        } else if digits.len() > scale {
            let split = digits.len() - scale;
            (digits[..split].to_string(), digits[split..].to_string())
        } else {
            // value magnitude < 1: pad leading zeros in the fractional part.
            let pad = "0".repeat(scale - digits.len());
            ("0".to_string(), format!("{pad}{digits}"))
        };

        // Trim trailing zeros from the fractional part, keep at least one digit.
        let frac_trimmed = frac_part.trim_end_matches('0');
        let frac_final = if frac_trimmed.is_empty() {
            "0"
        } else {
            frac_trimmed
        };
        let sign = if neg { "-" } else { "" };
        format!("{sign}{int_part}.{frac_final}")
    }
}

fn invalid(dt: XsdDatatype, lexical: &str, reason: &'static str) -> XsdError {
    XsdError::InvalidLexical {
        datatype: dt,
        lexical: lexical.to_string(),
        reason,
    }
}

/// `xsd:integer`: optional leading `+`/`-`, then one or more ASCII digits.
/// Returns the raw `i128` value without any subtype range check — for range-checked
/// integer-family parsing use [`parse_integer_typed`].
pub fn parse_integer(s: &str) -> Result<i128, XsdError> {
    let dt = XsdDatatype::Integer;
    let body = s.strip_prefix(['+', '-']).unwrap_or(s);
    if body.is_empty() || !body.bytes().all(|b| b.is_ascii_digit()) {
        return Err(invalid(dt, s, "expected an optional sign then digits"));
    }
    s.parse::<i128>().map_err(|_| XsdError::OutOfRange {
        datatype: dt,
        lexical: s.to_string(),
    })
}

/// Parse a lexical integer form for the given `datatype`, hard-failing with
/// [`XsdError::OutOfRange`] if the value is outside the datatype's inclusive bounds.
///
/// This is the unified entry point for all integer-family datatypes; `parse` in
/// `value.rs` routes every integer-family IRI through here.
pub fn parse_integer_typed(lexical: &str, datatype: XsdDatatype) -> Result<i128, XsdError> {
    // First, parse as an unconstrained integer (which may itself fail with
    // InvalidLexical for malformed input, or OutOfRange for beyond-i128).
    // We call parse_integer but report the error under `datatype` for non-Integer
    // subtypes, so callers see the correct IRI in the error.
    let body = lexical.strip_prefix(['+', '-']).unwrap_or(lexical);
    if body.is_empty() || !body.bytes().all(|b| b.is_ascii_digit()) {
        return Err(XsdError::InvalidLexical {
            datatype,
            lexical: lexical.to_string(),
            reason: "expected an optional sign then digits",
        });
    }
    let value = lexical.parse::<i128>().map_err(|_| XsdError::OutOfRange {
        datatype,
        lexical: lexical.to_string(),
    })?;

    // Now range-check against the datatype's inclusive bounds.
    if let Some((min, max)) = datatype.integer_range() {
        if value < min || value > max {
            return Err(XsdError::OutOfRange {
                datatype,
                lexical: lexical.to_string(),
            });
        }
    }
    Ok(value)
}

/// `xsd:decimal`: optional sign, digits with an optional single `.` (at least one
/// digit overall; `.5`, `1.`, `1.5`, `12` all valid).
pub fn parse_decimal(s: &str) -> Result<Decimal, XsdError> {
    let dt = XsdDatatype::Decimal;
    let neg = s.starts_with('-');
    let body = s.strip_prefix(['+', '-']).unwrap_or(s);

    let (int_str, frac_str) = match body.split_once('.') {
        Some((i, f)) => (i, f),
        None => (body, ""),
    };
    if body.contains('.') && body.matches('.').count() > 1 {
        return Err(invalid(dt, s, "more than one decimal point"));
    }
    if int_str.is_empty() && frac_str.is_empty() {
        return Err(invalid(dt, s, "no digits"));
    }
    if !int_str.bytes().all(|b| b.is_ascii_digit()) || !frac_str.bytes().all(|b| b.is_ascii_digit())
    {
        return Err(invalid(dt, s, "non-digit character"));
    }
    if frac_str.len() > usize::from(MAX_DECIMAL_SCALE) {
        return Err(XsdError::OutOfRange {
            datatype: dt,
            lexical: s.to_string(),
        });
    }

    let digits = format!("{int_str}{frac_str}");
    let digits_trimmed = digits.trim_start_matches('0');
    let magnitude = if digits_trimmed.is_empty() {
        0i128
    } else {
        digits_trimmed
            .parse::<i128>()
            .map_err(|_| XsdError::OutOfRange {
                datatype: dt,
                lexical: s.to_string(),
            })?
    };
    let mantissa = if neg { -magnitude } else { magnitude };
    // `frac_str.len() <= MAX_DECIMAL_SCALE <= u8::MAX`, so the cast cannot truncate.
    Ok(Decimal::from_parts(mantissa, frac_str.len() as u8))
}

/// `xsd:double`: XSD numeric float lexical, or `INF`/`+INF`/`-INF`/`NaN`.
pub fn parse_double(s: &str) -> Result<f64, XsdError> {
    parse_ieee(s, XsdDatatype::Double)
}

/// `xsd:float`: as `double` but single-precision.
pub fn parse_float(s: &str) -> Result<f32, XsdError> {
    let dt = XsdDatatype::Float;
    match s {
        "INF" | "+INF" => return Ok(f32::INFINITY),
        "-INF" => return Ok(f32::NEG_INFINITY),
        "NaN" => return Ok(f32::NAN),
        _ => {}
    }
    reject_non_xsd_numeric(s, dt)?;
    s.parse::<f32>()
        .map_err(|_| invalid(dt, s, "not a valid float lexical"))
}

/// Shared finite-numeric parse for double; returns `f64`.
fn parse_ieee(s: &str, dt: XsdDatatype) -> Result<f64, XsdError> {
    match s {
        "INF" | "+INF" => return Ok(f64::INFINITY),
        "-INF" => return Ok(f64::NEG_INFINITY),
        "NaN" => return Ok(f64::NAN),
        _ => {}
    }
    reject_non_xsd_numeric(s, dt)?;
    s.parse::<f64>()
        .map_err(|_| invalid(dt, s, "not a valid double lexical"))
}

/// Reject lexicals Rust's float parser would accept but XSD forbids (`inf`,
/// `infinity`, `nan`, etc.): any ASCII letter other than the `e`/`E` exponent
/// marker disqualifies the form (the `INF`/`NaN` keywords are handled before here).
fn reject_non_xsd_numeric(s: &str, dt: XsdDatatype) -> Result<(), XsdError> {
    if s.bytes()
        .any(|b| b.is_ascii_alphabetic() && b != b'e' && b != b'E')
    {
        return Err(invalid(dt, s, "non-XSD numeric token"));
    }
    Ok(())
}

/// XSD canonical `double`: `m.dddEsexp`, mantissa in shortest round-trippable form,
/// `INF`/`-INF`/`NaN` for the specials.
#[must_use]
pub fn canonical_double(d: f64) -> String {
    canonical_ieee(d, d.is_nan(), d.is_infinite(), d.is_sign_negative(), || {
        format!("{d:e}")
    })
}

/// XSD canonical `float`.
#[must_use]
pub fn canonical_float(f: f32) -> String {
    canonical_ieee(
        f64::from(f),
        f.is_nan(),
        f.is_infinite(),
        f.is_sign_negative(),
        || format!("{f:e}"),
    )
}

fn canonical_ieee(
    value: f64,
    is_nan: bool,
    is_inf: bool,
    is_neg: bool,
    sci: impl Fn() -> String,
) -> String {
    if is_nan {
        return "NaN".to_string();
    }
    if is_inf {
        return if is_neg { "-INF" } else { "INF" }.to_string();
    }
    if value == 0.0 {
        return if is_neg { "-0.0E0" } else { "0.0E0" }.to_string();
    }
    // Rust's `{:e}` is the shortest round-trippable scientific form (e.g. `1e2`,
    // `1.5e0`, `5e-3`). Normalize to the XSD canonical `mantissa.frac E exp`.
    let raw = sci();
    let (mantissa, exp) = raw.split_once('e').unwrap_or((raw.as_str(), "0"));
    let mantissa = if mantissa.contains('.') {
        mantissa.to_string()
    } else {
        format!("{mantissa}.0")
    };
    format!("{mantissa}E{exp}")
}

/// SPARQL numeric promotion comparison. Promotes both operands to the least type
/// that contains them (`integer ⊂ decimal ⊂ float ⊂ double`) and compares. Returns
/// `None` when an operand is `NaN` (genuinely unordered) or non-numeric (the caller
/// — `value_cmp` — only routes numeric operands here; non-numeric → `None`).
///
/// Integer-vs-integer comparison is by value only (ignoring the subtype): per the
/// SPARQL promotion rules, `xsd:int 5 = xsd:long 5`.
#[must_use]
pub fn numeric_cmp(a: &XsdValue, b: &XsdValue) -> Option<Ordering> {
    use XsdValue::{Decimal as Dec, Double, Float, Integer};
    match (a, b) {
        // Same exact integer / decimal cases keep full precision.
        // Integer-vs-integer: compare by value, ignore subtype (xsd:int 5 == xsd:long 5).
        (Integer { value: x, .. }, Integer { value: y, .. }) => Some(x.cmp(y)),
        (Dec(x), Dec(y)) => Some(x.cmp_exact(y)),
        (Integer { value: x, .. }, Dec(y)) => Some(Decimal::from_parts(*x, 0).cmp_exact(y)),
        (Dec(x), Integer { value: y, .. }) => Some(x.cmp_exact(&Decimal::from_parts(*y, 0))),
        // Any `double` operand → compare as f64.
        (Double(_), _) | (_, Double(_)) => num_f64(a)?.partial_cmp(&num_f64(b)?),
        // Else any `float` operand → compare as f32.
        (Float(_), _) | (_, Float(_)) => num_f32(a)?.partial_cmp(&num_f32(b)?),
        // At least one operand is non-numeric.
        _ => None,
    }
}

/// SPARQL numeric value equality (`=`) via the promotion comparison.
#[must_use]
pub fn numeric_eq(a: &XsdValue, b: &XsdValue) -> bool {
    numeric_cmp(a, b) == Some(Ordering::Equal)
}

/// The numeric value as `f64`, or `None` if `v` is not a numeric value.
fn num_f64(v: &XsdValue) -> Option<f64> {
    Some(match v {
        XsdValue::Integer { value, .. } => *value as f64,
        XsdValue::Decimal(d) => d.to_f64(),
        XsdValue::Float(f) => f64::from(*f),
        XsdValue::Double(d) => *d,
        _ => return None,
    })
}

/// The numeric value as `f32`, or `None` if `v` is not a numeric value.
fn num_f32(v: &XsdValue) -> Option<f32> {
    Some(match v {
        XsdValue::Integer { value, .. } => *value as f32,
        XsdValue::Decimal(d) => d.to_f64() as f32,
        XsdValue::Float(f) => *f,
        XsdValue::Double(d) => *d as f32,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::XsdDatatype as D;
    use pretty_assertions::assert_eq;

    fn dec(s: &str) -> Decimal {
        parse_decimal(s).unwrap()
    }

    fn int_val(n: i128) -> XsdValue {
        XsdValue::Integer {
            value: n,
            datatype: D::Integer,
        }
    }

    #[test]
    fn integer_parse_and_bounds() {
        assert_eq!(parse_integer("42").unwrap(), 42);
        assert_eq!(parse_integer("-7").unwrap(), -7);
        assert_eq!(parse_integer("+7").unwrap(), 7);
        assert_eq!(parse_integer("007").unwrap(), 7);
        assert_eq!(parse_integer(&i128::MAX.to_string()).unwrap(), i128::MAX);
        // i128::MAX + 1 overflows -> hard OutOfRange, not saturation.
        assert!(matches!(
            parse_integer("170141183460469231731687303715884105728"),
            Err(XsdError::OutOfRange { .. })
        ));
        assert!(parse_integer("1.0").is_err());
        assert!(parse_integer("").is_err());
        assert!(parse_integer("abc").is_err());
    }

    #[test]
    fn parse_integer_typed_range_checks() {
        // xsd:byte: -128..127
        assert_eq!(parse_integer_typed("127", D::Byte).unwrap(), 127);
        assert_eq!(parse_integer_typed("-128", D::Byte).unwrap(), -128);
        assert!(parse_integer_typed("128", D::Byte).is_err());
        assert!(parse_integer_typed("-129", D::Byte).is_err());

        // xsd:unsignedByte: 0..255
        assert_eq!(parse_integer_typed("255", D::UnsignedByte).unwrap(), 255);
        assert_eq!(parse_integer_typed("0", D::UnsignedByte).unwrap(), 0);
        assert!(parse_integer_typed("256", D::UnsignedByte).is_err());
        assert!(parse_integer_typed("-1", D::UnsignedByte).is_err());

        // xsd:positiveInteger: >= 1
        assert_eq!(parse_integer_typed("1", D::PositiveInteger).unwrap(), 1);
        assert!(parse_integer_typed("0", D::PositiveInteger).is_err());

        // xsd:negativeInteger: <= -1
        assert_eq!(parse_integer_typed("-1", D::NegativeInteger).unwrap(), -1);
        assert!(parse_integer_typed("0", D::NegativeInteger).is_err());

        // xsd:nonNegativeInteger: >= 0
        assert_eq!(parse_integer_typed("0", D::NonNegativeInteger).unwrap(), 0);
        assert!(parse_integer_typed("-1", D::NonNegativeInteger).is_err());

        // xsd:nonPositiveInteger: <= 0
        assert_eq!(parse_integer_typed("0", D::NonPositiveInteger).unwrap(), 0);
        assert!(parse_integer_typed("1", D::NonPositiveInteger).is_err());

        // xsd:unsignedLong boundary: u64::MAX should pass; u64::MAX+1 should fail
        let u64max = u64::MAX.to_string();
        assert_eq!(
            parse_integer_typed(&u64max, D::UnsignedLong).unwrap(),
            u64::MAX as i128
        );
        assert!(parse_integer_typed("18446744073709551616", D::UnsignedLong).is_err());

        // xsd:int: 2147483647 ok, 2147483648 fails
        assert_eq!(
            parse_integer_typed("2147483647", D::Int).unwrap(),
            2147483647
        );
        assert!(parse_integer_typed("2147483648", D::Int).is_err());
    }

    #[test]
    fn decimal_parse_and_canonical() {
        assert_eq!(dec("12.34").canonical_lexical(), "12.34");
        assert_eq!(dec("12.00").canonical_lexical(), "12.0");
        assert_eq!(dec("100").canonical_lexical(), "100.0");
        assert_eq!(dec("-0.5").canonical_lexical(), "-0.5");
        assert_eq!(dec(".5").canonical_lexical(), "0.5");
        assert_eq!(dec("1.").canonical_lexical(), "1.0");
        assert_eq!(dec("0.005").canonical_lexical(), "0.005");
        assert!(parse_decimal("1.2.3").is_err());
        assert!(parse_decimal("").is_err());
    }

    #[test]
    fn decimal_exact_comparison_across_scales() {
        assert_eq!(dec("1.5").cmp_exact(&dec("1.50")), Ordering::Equal);
        assert_eq!(dec("1.5").cmp_exact(&dec("1.05")), Ordering::Greater);
        assert_eq!(dec("0.1").cmp_exact(&dec("0.2")), Ordering::Less);
    }

    // ── cmp_exact correctness tests ──────────────────────────────────────────────

    /// Cross-scale equality: 1.50 (mantissa=150, scale=2) == 1.5 (mantissa=15, scale=1).
    #[test]
    fn cmp_exact_cross_scale_equal() {
        let a = Decimal::from_parts(150, 2); // 1.50
        let b = Decimal::from_parts(15, 1); // 1.5
        assert_eq!(a.cmp_exact(&b), Ordering::Equal);
        assert_eq!(b.cmp_exact(&a), Ordering::Equal);
    }

    /// Cross-scale strict order: 1.5 < 1.50001.
    #[test]
    fn cmp_exact_cross_scale_strict() {
        let a = dec("1.5");
        let b = dec("1.50001");
        assert_eq!(a.cmp_exact(&b), Ordering::Less);
        assert_eq!(b.cmp_exact(&a), Ordering::Greater);
    }

    /// Negative cross-scale: -1.5 vs -1.50001.
    /// -1.50001 < -1.5 (more negative).
    #[test]
    fn cmp_exact_negative_cross_scale() {
        let a = dec("-1.5");
        let b = dec("-1.50001");
        assert_eq!(a.cmp_exact(&b), Ordering::Greater); // -1.5 > -1.50001
        assert_eq!(b.cmp_exact(&a), Ordering::Less);
    }

    /// Mixed signs: any positive > any negative.
    #[test]
    fn cmp_exact_mixed_signs() {
        assert_eq!(dec("0.001").cmp_exact(&dec("-999.9")), Ordering::Greater);
        assert_eq!(dec("-0.001").cmp_exact(&dec("999.9")), Ordering::Less);
    }

    /// Both-zero regardless of scale.
    #[test]
    fn cmp_exact_zero_any_scale() {
        let z0 = Decimal::from_parts(0, 0);
        let z5 = Decimal::from_parts(0, 5);
        let z18 = Decimal::from_parts(0, 18);
        assert_eq!(z0.cmp_exact(&z5), Ordering::Equal);
        assert_eq!(z5.cmp_exact(&z18), Ordering::Equal);
        assert_eq!(z18.cmp_exact(&z0), Ordering::Equal);
    }

    /// Large-mantissa regression: two large decimals at scale 0 vs scale 1 that the
    /// old 10^diff widening path would overflow on (mantissa near i128::MAX).
    ///
    /// The old code attempted: (i128::MAX / 10) * 10  which checks out but
    /// i128::MAX * 10 overflows — so we construct a pair where the lower-scale value's
    /// mantissa is large enough that multiplying by 10^diff would exceed i128::MAX.
    ///
    /// Specifically: mantissa = i128::MAX (scale 0) vs mantissa = i128::MAX (scale 1).
    /// Value A = i128::MAX × 10^0 = i128::MAX (≈ 1.70141…×10^38)
    /// Value B = i128::MAX × 10^(-1) ≈ 1.70141…×10^37
    /// So A > B.  The old code would try to scale A's mantissa up by 10 → overflow.
    #[test]
    fn cmp_exact_large_mantissa_no_overflow() {
        // A = i128::MAX at scale 0; B = i128::MAX at scale 1
        // A = 170141183460469231731687303715884105727
        // B = 17014118346046923173168730371588410572.7
        // True order: A > B
        let a = Decimal::from_parts(i128::MAX, 0);
        let b = Decimal::from_parts(i128::MAX, 1);
        assert_eq!(a.cmp_exact(&b), Ordering::Greater);
        assert_eq!(b.cmp_exact(&a), Ordering::Less);
    }

    /// Regression vector for the exact f64 collapse bug: two large unequal decimals
    /// at different scales that the old f64 path would round to the same f64 value
    /// and therefore return Equal incorrectly.
    ///
    /// f64 has ~15.9 significant decimal digits.  Construct two values that differ
    /// only in the 18th digit — well below f64 resolution — but whose true order
    /// is strict.
    ///
    /// A = 100000000000000000.1  (mantissa=1000000000000000001, scale=1)
    /// B = 100000000000000000.2  (mantissa=1000000000000000002, scale=1)
    /// Both have the same f64 representation (the fractional digit is lost), but
    /// A < B is exact.
    #[test]
    fn cmp_exact_large_f64_collapse_regression() {
        // 100000000000000000.1 and 100000000000000000.2 — same scale, near i64::MAX magnitude
        let a = Decimal::from_parts(1_000_000_000_000_000_001, 1);
        let b = Decimal::from_parts(1_000_000_000_000_000_002, 1);
        // Both collapse to the same f64 — the old path returns Equal incorrectly.
        assert_eq!(a.to_f64(), b.to_f64(), "f64 collapse precondition");
        // cmp_exact must return Less (A < B), not Equal.
        assert_eq!(a.cmp_exact(&b), Ordering::Less);
        assert_eq!(b.cmp_exact(&a), Ordering::Greater);
    }

    /// Same as above but across scales (scale 1 vs scale 2).
    #[test]
    fn cmp_exact_large_f64_collapse_cross_scale_regression() {
        // A = 100000000000000000.1  (scale 1)
        // B = 100000000000000000.11 (scale 2) = 10000000000000000011 mantissa
        // A < B (0.1 < 0.11).  Both f64-identical at this magnitude.
        let a = Decimal::from_parts(1_000_000_000_000_000_001, 1); // .1 at scale 1
        let b = Decimal::from_parts(10_000_000_000_000_000_011, 2); // .11 at scale 2
        assert_eq!(a.to_f64(), b.to_f64(), "f64 collapse precondition");
        assert_eq!(a.cmp_exact(&b), Ordering::Less);
        assert_eq!(b.cmp_exact(&a), Ordering::Greater);
    }

    #[test]
    fn double_specials_and_canonical() {
        assert_eq!(parse_double("INF").unwrap(), f64::INFINITY);
        assert_eq!(parse_double("-INF").unwrap(), f64::NEG_INFINITY);
        assert!(parse_double("NaN").unwrap().is_nan());
        assert!(parse_double("inf").is_err());
        assert!(parse_double("Infinity").is_err());
        assert_eq!(canonical_double(1.0), "1.0E0");
        assert_eq!(canonical_double(1.5), "1.5E0");
        assert_eq!(canonical_double(100.0), "1.0E2");
        assert_eq!(canonical_double(0.005), "5.0E-3");
        assert_eq!(canonical_double(f64::INFINITY), "INF");
        assert_eq!(canonical_double(f64::NEG_INFINITY), "-INF");
        assert_eq!(canonical_double(f64::NAN), "NaN");
    }

    #[test]
    fn numeric_promotion() {
        // "1"^^integer = "1.0"^^decimal
        assert!(numeric_eq(&int_val(1), &XsdValue::Decimal(dec("1.0"))));
        // integer vs double
        assert_eq!(
            numeric_cmp(&int_val(2), &XsdValue::Double(2.5)),
            Some(Ordering::Less)
        );
        // decimal vs float
        assert_eq!(
            numeric_cmp(&XsdValue::Decimal(dec("1.5")), &XsdValue::Float(1.25)),
            Some(Ordering::Greater)
        );
        // NaN is unordered and unequal.
        assert_eq!(numeric_cmp(&XsdValue::Double(f64::NAN), &int_val(1)), None);
        assert!(!numeric_eq(
            &XsdValue::Double(f64::NAN),
            &XsdValue::Double(f64::NAN)
        ));
        // +0 == -0.
        assert!(numeric_eq(&XsdValue::Double(0.0), &XsdValue::Double(-0.0)));

        // Cross-subtype integer equality: xsd:int 5 == xsd:long 5.
        let int5 = XsdValue::Integer {
            value: 5,
            datatype: D::Int,
        };
        let long5 = XsdValue::Integer {
            value: 5,
            datatype: D::Long,
        };
        assert!(numeric_eq(&int5, &long5));
        assert_eq!(numeric_cmp(&int5, &long5), Some(Ordering::Equal));
    }
}
