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

    /// Exact comparison of two decimals (total order — decimals are never NaN).
    #[must_use]
    pub fn cmp_exact(&self, other: &Decimal) -> Ordering {
        if self.scale == other.scale {
            return self.mantissa.cmp(&other.mantissa);
        }
        // Align to the larger scale; on i128 overflow fall back to f64 (only
        // reachable for values far beyond any realistic literal).
        let (hi, lo) = if self.scale > other.scale {
            (self, other)
        } else {
            (other, self)
        };
        let diff = u32::from(hi.scale - lo.scale);
        match 10i128
            .checked_pow(diff)
            .and_then(|f| lo.mantissa.checked_mul(f))
        {
            Some(lo_scaled) => {
                let ord = lo_scaled.cmp(&hi.mantissa);
                if self.scale > other.scale {
                    ord.reverse()
                } else {
                    ord
                }
            }
            None => self
                .to_f64()
                .partial_cmp(&other.to_f64())
                .unwrap_or(Ordering::Equal),
        }
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
#[must_use]
pub fn numeric_cmp(a: &XsdValue, b: &XsdValue) -> Option<Ordering> {
    use XsdValue::{Decimal as Dec, Double, Float, Integer};
    match (a, b) {
        // Same exact integer / decimal cases keep full precision.
        (Integer(x), Integer(y)) => Some(x.cmp(y)),
        (Dec(x), Dec(y)) => Some(x.cmp_exact(y)),
        (Integer(x), Dec(y)) => Some(Decimal::from_parts(*x, 0).cmp_exact(y)),
        (Dec(x), Integer(y)) => Some(x.cmp_exact(&Decimal::from_parts(*y, 0))),
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
        XsdValue::Integer(i) => *i as f64,
        XsdValue::Decimal(d) => d.to_f64(),
        XsdValue::Float(f) => f64::from(*f),
        XsdValue::Double(d) => *d,
        _ => return None,
    })
}

/// The numeric value as `f32`, or `None` if `v` is not a numeric value.
fn num_f32(v: &XsdValue) -> Option<f32> {
    Some(match v {
        XsdValue::Integer(i) => *i as f32,
        XsdValue::Decimal(d) => d.to_f64() as f32,
        XsdValue::Float(f) => *f,
        XsdValue::Double(d) => *d as f32,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    fn dec(s: &str) -> Decimal {
        parse_decimal(s).unwrap()
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
        assert!(numeric_eq(
            &XsdValue::Integer(1),
            &XsdValue::Decimal(dec("1.0"))
        ));
        // integer vs double
        assert_eq!(
            numeric_cmp(&XsdValue::Integer(2), &XsdValue::Double(2.5)),
            Some(Ordering::Less)
        );
        // decimal vs float
        assert_eq!(
            numeric_cmp(&XsdValue::Decimal(dec("1.5")), &XsdValue::Float(1.25)),
            Some(Ordering::Greater)
        );
        // NaN is unordered and unequal.
        assert_eq!(
            numeric_cmp(&XsdValue::Double(f64::NAN), &XsdValue::Integer(1)),
            None
        );
        assert!(!numeric_eq(
            &XsdValue::Double(f64::NAN),
            &XsdValue::Double(f64::NAN)
        ));
        // +0 == -0.
        assert!(numeric_eq(&XsdValue::Double(0.0), &XsdValue::Double(-0.0)));
    }
}
