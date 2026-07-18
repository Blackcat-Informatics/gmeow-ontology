//! Shared moded evaluator for arithmetic / comparison builtins.
//!
//! Arithmetic (`X is Y op Z`) and comparison (`L cmp R`) builtins are
//! **mode-constrained relations** evaluated in body order as part of a rule's
//! sideways information passing (SIPS). A comparison is the infinite relation
//! `{(x, y) | x cmp y}`, evaluable only when both operands are bound — always a
//! *filter*. An `is` is a functional relation whose role turns on the target's
//! binding: a *filter* when the target is already bound (check equality), a
//! *generator* when the target is free and the operands are bound (compute and
//! bind).
//!
//! This one evaluator is the single semantics called by every native engine —
//! the forward semi-naive core ([`crate::physical::seminaive`]), the backward
//! magic core ([`crate::physical::magic`]), and the declarative reference oracle
//! ([`crate::reference_resolver`]) — so they cannot diverge from one another. The
//! caller supplies a `lookup` from a variable name to its bound surface (or
//! `None` when unbound) and interprets the returned [`BuiltinOutcome`] in its own
//! control flow.
//!
//! Values flow through an exact-numeric **value tower** ([`Value`]): dimensionless
//! machine integers ([`Value::Int`]) and exact rationals ([`Value::Rat`]), the SI
//! dimension vector ([`Value::Dim`]) and dimensioned rationals ([`Value::Quantity`]).
//! Integer evaluation is over checked `i64`: truncating integer division `//`
//! ([`ArithOp::Div`]) rounds toward zero (ISO `//`, matching the captured SLD
//! semantics), and `=:=` is numeric value equality, never structural unification.
//! Exact rational division `/` ([`ArithOp::ExactDiv`]) and any operand that resolves
//! to a [`Value::Rat`] route to the shared checked-ℚ kernel ([`q_add`], [`q_sub`],
//! [`q_mul`], [`q_div`]) and commit a [`Value::Rat`]; two integers under `+ - * //`
//! stay on the unchanged i64 fast path and commit a [`Value::Int`]. Dimensioned
//! operands ([`Value::Dim`] / [`Value::Quantity`]) evaluate the free-ℚ-module
//! dimension algebra ([`compute_is_value`] / [`compute_compare`]): dimension
//! composition (`Mul`) and quotient (`Div`/`ExactDiv`) add / subtract the ℚ⁷
//! exponent vectors, dimensioned-quantity `Add`/`Sub` require an equal dimension
//! (an unequal one is [`BuiltinError::DimensionMismatch`], the intrinsic-homogeneity
//! gate), quantity `Mul`/`Div` compose the magnitudes and dimensions, and a
//! dimensionless [`Value::Int`]/[`Value::Rat`] promotes to a `[0;7]`-dimensioned
//! quantity when mixed with a [`Value::Quantity`]. Commensurability (`=:=` over two
//! dimensions) is exact ℚ⁷ vector equality; ordering over bare dimensions, bare-
//! dimension addition, and any cross-kind combination composition does not define
//! are declared gaps. A value that cannot be computed is a first-class declared gap
//! ([`BuiltinOutcome::Unbound`]) or domain/precision error
//! ([`BuiltinOutcome::Error`]) — never a wrong answer or a panic.

use crate::query_ir::{ArithOp, CmpOp, QBuiltin, QTerm};
use std::borrow::Cow;

/// The canonical `xsd:integer` datatype IRI — the type of every computed
/// arithmetic answer. The surface form produced by [`emit_integer_surface`] is
/// byte-identical to `provenance::literal_n3` for `xsd:integer` and to the form
/// the captured SLD reference renders, so a generated value reads back like a
/// materialized typed literal.
pub(crate) const XSD_INTEGER: &str = "http://www.w3.org/2001/XMLSchema#integer";

/// Engine-internal transport datatype tag for an exact rational [`Value::Rat`].
///
/// This is a *transport* tag for carrying a computed value between native
/// engines through the string-surface substitution channel — **not** a persisted
/// ontology datatype. Its lexical form is `num/den` with the sign in the
/// numerator and `den > 0`.
const XSD_RATIONAL_TRANSPORT: &str = "urn:gmeow:transport:rational";

/// Engine-internal transport datatype tag for an SI dimension vector
/// [`Value::Dim`]. Lexical form: the seven rational exponents in fixed SI order,
/// comma-separated (`n0/d0,n1/d1,…,n6/d6`).
const XSD_DIMENSION_TRANSPORT: &str = "urn:gmeow:transport:dimension";

/// Engine-internal transport datatype tag for a dimensioned rational
/// [`Value::Quantity`]. Lexical form: the scalar rational, a `;`, then the seven
/// dimension exponents (`num/den;n0/d0,…,n6/d6`).
const XSD_QUANTITY_TRANSPORT: &str = "urn:gmeow:transport:quantity";

/// Render an `i64` as the canonical typed-integer literal surface
/// `"N"^^<…#integer>` — the single shared helper every producer of a computed
/// numeric value calls, so byte-identity is by construction rather than asserted.
pub(crate) fn emit_integer_surface(n: i64) -> String {
    format!("\"{n}\"^^<{XSD_INTEGER}>")
}

/// Parse a bound surface back to an `i64`, accepting both the canonical
/// typed-integer literal `"N"^^<…#integer>` and a bare integer token `N`.
/// Returns `None` for any non-numeric surface (a domain value that is not an
/// integer).
fn parse_integer_surface(surface: &str) -> Option<i64> {
    if let Some(rest) = surface.strip_prefix('"') {
        // `"N"^^<datatype>` — take the lexical form up to the closing quote and
        // require the integer datatype tag.
        let (lex, tag) = rest.split_once('"')?;
        // Match the `^^<datatype>` tag without allocating: strip the delimiters
        // and compare the datatype IRI directly (no raw indexing, so no
        // out-of-bounds path).
        if tag.strip_prefix("^^<").and_then(|t| t.strip_suffix('>')) == Some(XSD_INTEGER) {
            return lex.parse::<i64>().ok();
        }
        return None;
    }
    surface.parse::<i64>().ok()
}

/// Greatest common divisor of two **non-negative** `i64`s by Euclid's algorithm.
///
/// Every caller passes magnitudes (a `checked_abs` result or an already-positive
/// denominator), so `a % b` never underflows and the result is non-negative.
const fn gcd_i64(mut a: i64, mut b: i64) -> i64 {
    while b != 0 {
        let t = a % b;
        a = b;
        b = t;
    }
    a
}

/// An exact rational number over checked `i64` numerator / denominator.
///
/// **Invariant (upheld by construction):** the value is always in lowest terms —
/// `den > 0` and `gcd(|num|, den) == 1`. Every value is produced through
/// [`q_normalize`] (via [`Rational::new`] and the arithmetic kernel), so no path
/// can store an unnormalized form. Consequently the derived `PartialEq` / `Eq` /
/// `Hash` are canonical: `Rational::new(1, 2)` and `Rational::new(2, 4)` are equal
/// **and** hash-equal because both normalize to `(1, 2)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct Rational {
    /// Signed numerator (carries the sign of the whole value).
    num: i64,
    /// Strictly positive denominator, coprime with `|num|`.
    den: i64,
}

impl Rational {
    /// The rational `0` in canonical `0/1` form.
    const ZERO: Rational = Rational { num: 0, den: 1 };

    /// Construct a normalized rational from a raw `num/den`, or a declared error.
    ///
    /// `den == 0` is [`BuiltinError::ZeroDenominator`]; any `i64::MIN` sign-flip
    /// or magnitude that cannot be represented is [`BuiltinError::Overflow`]
    /// (never a panic).
    pub(crate) fn new(num: i64, den: i64) -> Result<Rational, BuiltinError> {
        q_normalize(num, den)
    }
}

/// Normalize a raw `num/den` to lowest terms with `den > 0`.
///
/// The sign is carried into the numerator; the denominator is made strictly
/// positive; both are divided by `gcd(|num|, den)`. `den == 0` is a
/// [`BuiltinError::ZeroDenominator`]. Because normalization needs the numerator's
/// magnitude and (for a negative denominator) its negation, an `i64::MIN` input
/// on either side has no representable normal form and is reported as
/// [`BuiltinError::Overflow`] rather than wrapping or panicking.
fn q_normalize(num: i64, den: i64) -> Result<Rational, BuiltinError> {
    if den == 0 {
        return Err(BuiltinError::ZeroDenominator);
    }
    // Carry the sign in the numerator and force a positive denominator; negating
    // `i64::MIN` is unrepresentable → overflow.
    let (num, den) = if den < 0 {
        (
            num.checked_neg().ok_or(BuiltinError::Overflow)?,
            den.checked_neg().ok_or(BuiltinError::Overflow)?,
        )
    } else {
        (num, den)
    };
    // `den` is now > 0; the gcd needs the numerator's magnitude, so `i64::MIN`
    // (whose magnitude overflows `i64`) is an overflow.
    let g = gcd_i64(num.checked_abs().ok_or(BuiltinError::Overflow)?, den);
    // `g >= 1` because `den > 0`, and it divides both operands exactly.
    Ok(Rational {
        num: num / g,
        den: den / g,
    })
}

/// Exact rational addition (or subtraction when `subtract`), cross-cancelling the
/// denominators' gcd before multiplying to delay overflow.
fn q_addsub(lhs: &Rational, rhs: &Rational, subtract: bool) -> Result<Rational, BuiltinError> {
    // Both denominators are > 0. Reduce by their gcd so the common denominator is
    // their lcm rather than the full product.
    let g = gcd_i64(lhs.den, rhs.den);
    let lhs_scale = rhs.den / g; // lcm / lhs.den
    let rhs_scale = lhs.den / g; // lcm / rhs.den
    let left = lhs
        .num
        .checked_mul(lhs_scale)
        .ok_or(BuiltinError::Overflow)?;
    let right = rhs
        .num
        .checked_mul(rhs_scale)
        .ok_or(BuiltinError::Overflow)?;
    let num = if subtract {
        left.checked_sub(right)
    } else {
        left.checked_add(right)
    }
    .ok_or(BuiltinError::Overflow)?;
    let den = lhs
        .den
        .checked_mul(lhs_scale)
        .ok_or(BuiltinError::Overflow)?;
    q_normalize(num, den)
}

/// Exact rational addition. Part of the shared checked-ℚ kernel driving the
/// rational arithmetic dispatch ([`apply_arith_q`]).
fn q_add(lhs: &Rational, rhs: &Rational) -> Result<Rational, BuiltinError> {
    q_addsub(lhs, rhs, false)
}

/// Exact rational subtraction. Part of the shared checked-ℚ kernel driving the
/// rational arithmetic dispatch ([`apply_arith_q`]).
fn q_sub(lhs: &Rational, rhs: &Rational) -> Result<Rational, BuiltinError> {
    q_addsub(lhs, rhs, true)
}

/// Exact rational multiplication with cross-cancellation of `gcd(a, d)` and
/// `gcd(c, b)` for `a/b · c/d` before multiplying, to delay overflow. Part of the
/// shared checked-ℚ kernel driving the rational arithmetic dispatch.
fn q_mul(lhs: &Rational, rhs: &Rational) -> Result<Rational, BuiltinError> {
    // Cross-cancel: gcd(lhs.num, rhs.den) and gcd(rhs.num, lhs.den). Magnitudes
    // are needed for the gcd, so an `i64::MIN` numerator is an overflow.
    let g1 = gcd_i64(
        lhs.num.checked_abs().ok_or(BuiltinError::Overflow)?,
        rhs.den,
    );
    let g2 = gcd_i64(
        rhs.num.checked_abs().ok_or(BuiltinError::Overflow)?,
        lhs.den,
    );
    let num = (lhs.num / g1)
        .checked_mul(rhs.num / g2)
        .ok_or(BuiltinError::Overflow)?;
    let den = (lhs.den / g2)
        .checked_mul(rhs.den / g1)
        .ok_or(BuiltinError::Overflow)?;
    q_normalize(num, den)
}

/// Exact rational division. Division by the zero rational is
/// [`BuiltinError::ZeroDivisor`]. Part of the shared checked-ℚ kernel driving the
/// rational arithmetic dispatch ([`apply_arith_q`]).
fn q_div(lhs: &Rational, rhs: &Rational) -> Result<Rational, BuiltinError> {
    if rhs.num == 0 {
        return Err(BuiltinError::ZeroDivisor);
    }
    // Reciprocal of a normalized rational: `den / num`, re-normalized to restore
    // the positive-denominator invariant (and to guard an `i64::MIN` sign-flip).
    let reciprocal = q_normalize(rhs.den, rhs.num)?;
    q_mul(lhs, &reciprocal)
}

/// The exact-numeric value tower carried between native engines.
///
/// [`Value::Int`] and [`Value::Rat`] are the dimensionless fast / exact variants
/// (no `[Rational; 7]` allocation). [`Value::Dim`] is the ℚ⁷ SI exponent vector
/// (in fixed SI base-quantity order) and [`Value::Quantity`] is a dimensioned
/// rational (scalar magnitude paired with its dimension vector). Every stored
/// [`Rational`] is normalized, so the derived `Eq` / `Hash` are canonical.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) enum Value {
    /// A dimensionless machine integer (the integer fast path).
    Int(i64),
    /// A dimensionless exact rational.
    Rat(Rational),
    /// An SI dimension vector: seven rational exponents in fixed base order.
    Dim([Rational; 7]),
    /// A dimensioned rational: a scalar magnitude with its dimension vector.
    Quantity(Rational, [Rational; 7]),
}

/// The resolved binding state of a single builtin operand.
enum Operand {
    /// The operand is bound to a value in the tower.
    Bound(Value),
    /// The operand is a variable with no binding under the current substitution.
    Unbound,
    /// The operand is bound to a surface that is not a number.
    NonNumeric,
}

/// A domain / precision error raised while computing a builtin — routed by the
/// caller to a declared gap, never surfaced as a wrong answer. Each arm anchors a
/// `math:` failure class.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BuiltinError {
    /// Integer division / remainder by zero, or division by the zero rational
    /// (the oracle raises `zero_divisor`). Anchors `math:ZeroDivisor`.
    ZeroDivisor,
    /// A computation overflowed the machine integer (the oracle uses a bignum),
    /// including an unrepresentable `i64::MIN` magnitude / sign-flip during
    /// rational normalization. Anchors `math:Overflow`.
    Overflow,
    /// A rational was constructed with a zero denominator. Anchors
    /// `math:ZeroDenominator`.
    ZeroDenominator,
    /// Two quantities of incompatible SI dimension were combined additively, or two
    /// incommensurable quantities were compared. Anchors `math:DimensionalInhomogeneity`.
    DimensionMismatch,
    /// A dimension-vector transport was not well-formed (wrong arity or an
    /// unparsable exponent). Anchors `math:MalformedDimension`.
    MalformedDimension,
    /// A bilinear form / Gram matrix presented for evaluation was not symmetric.
    /// Anchors `math:AsymmetricGramMatrix`.
    #[allow(dead_code)] // producer: Task-3 bilinear-form (Gram matrix) evaluation.
    AsymmetricForm,
}

/// The outcome of moded builtin evaluation against a partial substitution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum BuiltinOutcome {
    /// A filter verdict: `true` keeps the current solution, `false` prunes it.
    Filter(bool),
    /// A generator result: bind `var` to the computed `value`.
    Generate {
        /// The name of the (previously free) target variable.
        var: String,
        /// The computed value, carried in the exact-numeric tower.
        value: Value,
    },
    /// An operand needed for evaluation is still unbound — a declared mode gap
    /// (the caller declines rather than guessing).
    Unbound,
    /// A domain / precision error (÷0 or i64 overflow) — a declared gap.
    Error(BuiltinError),
}

/// Split a `"lex"^^<datatype>` typed-literal surface into `(lexical, datatype)`.
///
/// Returns `None` for any surface that is not a typed literal. The transport
/// lexical forms produced by [`emit_surface`] contain no `"` characters, so the
/// closing-quote split is unambiguous.
fn split_typed_literal(surface: &str) -> Option<(&str, &str)> {
    let rest = surface.strip_prefix('"')?;
    let (lex, tag) = rest.split_once('"')?;
    let datatype = tag.strip_prefix("^^<")?.strip_suffix('>')?;
    Some((lex, datatype))
}

/// Render a normalized rational as its transport lexical form `num/den`.
fn emit_rational_lex(r: &Rational) -> String {
    format!("{}/{}", r.num, r.den)
}

/// Render a seven-dimension SI exponent vector as its transport lexical form
/// `n0/d0,n1/d1,…,n6/d6` (fixed SI base order).
fn emit_dimension_lex(dim: &[Rational; 7]) -> String {
    let mut out = String::new();
    for (i, r) in dim.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push_str(&emit_rational_lex(r));
    }
    out
}

/// Emit a [`Value`] to its bound surface for transport between native engines.
///
/// [`Value::Int`] uses the canonical `xsd:integer` surface (byte-identical to
/// [`emit_integer_surface`]); the rational / dimension / quantity variants use
/// their pinned engine-internal transport tags.
pub(crate) fn emit_surface(value: &Value) -> String {
    match value {
        Value::Int(n) => emit_integer_surface(*n),
        Value::Rat(r) => format!("\"{}\"^^<{XSD_RATIONAL_TRANSPORT}>", emit_rational_lex(r)),
        Value::Dim(dim) => {
            format!(
                "\"{}\"^^<{XSD_DIMENSION_TRANSPORT}>",
                emit_dimension_lex(dim)
            )
        }
        Value::Quantity(scalar, dim) => format!(
            "\"{};{}\"^^<{XSD_QUANTITY_TRANSPORT}>",
            emit_rational_lex(scalar),
            emit_dimension_lex(dim)
        ),
    }
}

/// Parse a rational transport lexical form `num/den` back to a [`Rational`].
///
/// Returns `None` for a surface that is not a well-formed rational transport (a
/// domain value that is not one of ours); a well-formed but numerically invalid
/// form (zero denominator / overflow) also declines rather than fabricating.
fn parse_rational_lex(lex: &str) -> Option<Rational> {
    let (num, den) = lex.split_once('/')?;
    let num = num.parse::<i64>().ok()?;
    let den = den.parse::<i64>().ok()?;
    Rational::new(num, den).ok()
}

/// Parse a dimension transport lexical form (seven comma-separated exponents).
///
/// A wrong arity or an unparsable exponent is [`BuiltinError::MalformedDimension`]
/// — the transport is engine-internal, so a form carrying our dimension tag that
/// does not decode is corruption, not an ordinary domain value.
fn parse_dimension_lex(lex: &str) -> Result<[Rational; 7], BuiltinError> {
    let mut exponents = lex.split(',');
    let mut dim = [Rational::ZERO; 7];
    for slot in dim.iter_mut() {
        let token = exponents.next().ok_or(BuiltinError::MalformedDimension)?;
        *slot = parse_rational_lex(token).ok_or(BuiltinError::MalformedDimension)?;
    }
    if exponents.next().is_some() {
        // More than seven exponents — a malformed dimension.
        return Err(BuiltinError::MalformedDimension);
    }
    Ok(dim)
}

/// Parse a quantity transport lexical form `num/den;n0/d0,…,n6/d6`.
fn parse_quantity_lex(lex: &str) -> Result<Value, BuiltinError> {
    let (scalar_lex, dim_lex) = lex
        .split_once(';')
        .ok_or(BuiltinError::MalformedDimension)?;
    let scalar = parse_rational_lex(scalar_lex).ok_or(BuiltinError::MalformedDimension)?;
    let dim = parse_dimension_lex(dim_lex)?;
    Ok(Value::Quantity(scalar, dim))
}

/// Parse a bound surface into a [`Value`], or `None` if it is not numeric.
///
/// An `xsd:integer` surface becomes [`Value::Int`] exactly as before; each
/// engine-internal transport tag decodes to its tower variant. A malformed
/// transport (carrying our tag but not decoding) declines to `None` — it is
/// routed to a non-numeric filter-failure rather than a wrong answer.
fn parse_value_surface(surface: &str) -> Option<Value> {
    if let Some(n) = parse_integer_surface(surface) {
        return Some(Value::Int(n));
    }
    let (lex, datatype) = split_typed_literal(surface)?;
    match datatype {
        XSD_RATIONAL_TRANSPORT => parse_rational_lex(lex).map(Value::Rat),
        XSD_DIMENSION_TRANSPORT => parse_dimension_lex(lex).ok().map(Value::Dim),
        XSD_QUANTITY_TRANSPORT => parse_quantity_lex(lex).ok(),
        _ => None,
    }
}

/// Resolve one operand `term` to its binding state under `lookup`.
fn resolve_operand<'a>(term: &QTerm, lookup: &impl Fn(&str) -> Option<Cow<'a, str>>) -> Operand {
    match term {
        QTerm::Num(n) => Operand::Bound(Value::Int(*n)),
        QTerm::Var(v) => match lookup(v) {
            None => Operand::Unbound,
            Some(surface) => match parse_value_surface(&surface) {
                Some(value) => Operand::Bound(value),
                None => Operand::NonNumeric,
            },
        },
        // A bare `Const` surface may still be a typed integer literal (e.g. a fact
        // object materialized as `"3"^^<…#integer>`) or an engine transport;
        // otherwise it is non-numeric.
        QTerm::Const(c) => match parse_value_surface(c) {
            Some(value) => Operand::Bound(value),
            None => Operand::NonNumeric,
        },
        // A structured (function-symbol) operand is never a number; it is routed to the
        // full-FOL resolver upstream, so on the flat builtin path it is a non-numeric filter
        // failure, never a gap.
        QTerm::Struct(_) => Operand::NonNumeric,
    }
}

/// Apply an integer-fast-path operator with checked `i64` semantics.
///
/// Returns `Some` for the ℤ-shared operators (`+ - *` and truncating `//`), and
/// `None` for [`ArithOp::ExactDiv`] (`/`), which is ℚ-only and never computed on the
/// i64 path — the caller routes a `None` op (like any rational operand) to the
/// exact-ℚ kernel. Truncating `//` rounds toward zero (Rust `i64` division, ISO
/// Prolog `//`); division by zero and any overflow are declared errors, never a
/// wrong answer.
fn apply_arith_int(lhs: i64, op: ArithOp, rhs: i64) -> Option<Result<i64, BuiltinError>> {
    match op {
        ArithOp::Add => Some(lhs.checked_add(rhs).ok_or(BuiltinError::Overflow)),
        ArithOp::Sub => Some(lhs.checked_sub(rhs).ok_or(BuiltinError::Overflow)),
        ArithOp::Mul => Some(lhs.checked_mul(rhs).ok_or(BuiltinError::Overflow)),
        ArithOp::Div => Some(if rhs == 0 {
            Err(BuiltinError::ZeroDivisor)
        } else {
            // `checked_div` also guards the i64::MIN / -1 overflow.
            lhs.checked_div(rhs).ok_or(BuiltinError::Overflow)
        }),
        // `/` is exact rational division: not an i64-fast-path operator.
        ArithOp::ExactDiv => None,
    }
}

/// View a scalar tower value as an exact [`Rational`], promoting an integer via
/// `n/1`.
///
/// The outer `Option` distinguishes a *scalar* operand (`Some`) from a *dimensioned*
/// one (`None` — [`Value::Dim`] / [`Value::Quantity`]). A `None` on the scalar-ℚ
/// path is a declared gap; on the dimension path it is the promotion boundary (a
/// scalar mixed with a [`Value::Quantity`] promotes to a `[0;7]`-dimensioned
/// quantity). The inner `Result` carries the `i64::MIN`-magnitude overflow that an
/// integer promotion can raise during rational normalization
/// ([`BuiltinError::Overflow`]), never a panic.
fn scalar_rational(value: &Value) -> Option<Result<Rational, BuiltinError>> {
    match value {
        Value::Int(n) => Some(Rational::new(*n, 1)),
        Value::Rat(r) => Some(Ok(*r)),
        Value::Dim(_) | Value::Quantity(_, _) => None,
    }
}

/// Apply an arithmetic operator over two exact rationals (the ℚ dispatch).
///
/// `+ - *` and exact `/` use the shared checked-ℚ kernel directly. Truncating `//`
/// ([`ArithOp::Div`]) on a rational operand rounds the exact quotient toward zero to
/// an integer (carried as a `t/1` rational), preserving `//`'s integer-division
/// meaning when it mixes with a rational. Every overflow / ÷0 is a declared error.
fn apply_arith_q(lhs: Rational, op: ArithOp, rhs: Rational) -> Result<Rational, BuiltinError> {
    match op {
        ArithOp::Add => q_add(&lhs, &rhs),
        ArithOp::Sub => q_sub(&lhs, &rhs),
        ArithOp::Mul => q_mul(&lhs, &rhs),
        ArithOp::ExactDiv => q_div(&lhs, &rhs),
        ArithOp::Div => {
            // Truncating integer division over ℚ: exact quotient, then round toward
            // zero. `den > 0`, so `num / den` truncates toward zero and cannot
            // overflow (no `i64::MIN / -1`).
            let quotient = q_div(&lhs, &rhs)?;
            Rational::new(quotient.num / quotient.den, 1)
        }
    }
}

/// Compute `lhs op rhs` in exact ℚ, promoting integer operands.
///
/// `None` means a dimensioned operand (routed to the dimension algebra by the caller,
/// never reached on the scalar path); `Some(Err)` is a domain / precision error;
/// `Some(Ok)` is the exact result.
fn compute_rational(
    lhs: &Value,
    rhs: &Value,
    op: ArithOp,
) -> Option<Result<Rational, BuiltinError>> {
    let l = match scalar_rational(lhs)? {
        Ok(l) => l,
        Err(e) => return Some(Err(e)),
    };
    let r = match scalar_rational(rhs)? {
        Ok(r) => r,
        Err(e) => return Some(Err(e)),
    };
    Some(apply_arith_q(l, op, r))
}

/// Apply a comparison operator as exact numeric value comparison over ℚ.
///
/// The cross-multiplication is done in `i128` so it cannot overflow (an `i64 · i64`
/// product fits in `i128`), keeping the comparison exact for every operand pair.
fn apply_compare_q(lhs: &Rational, op: CmpOp, rhs: &Rational) -> bool {
    // Both denominators are > 0, so scaling by them preserves the ordering.
    let left = i128::from(lhs.num) * i128::from(rhs.den);
    let right = i128::from(rhs.num) * i128::from(lhs.den);
    match op {
        CmpOp::Gt => left > right,
        CmpOp::Lt => left < right,
        CmpOp::Ge => left >= right,
        CmpOp::Le => left <= right,
        CmpOp::Eq => left == right,
    }
}

/// Apply a comparison operator as numeric value comparison over `i64`.
fn apply_compare(lhs: i64, op: CmpOp, rhs: i64) -> bool {
    match op {
        CmpOp::Gt => lhs > rhs,
        CmpOp::Lt => lhs < rhs,
        CmpOp::Ge => lhs >= rhs,
        CmpOp::Le => lhs <= rhs,
        CmpOp::Eq => lhs == rhs,
    }
}

// ── Dimension algebra (free ℚ-module ℚ⁷ over the seven SI base dimensions) ────────

/// The dimensionless dimension: the zero exponent vector, identity of the ℚ-module
/// under composition. A bare [`Value::Int`] / [`Value::Rat`] is a quantity of this
/// dimension when it mixes with a [`Value::Quantity`].
const ZERO_DIM: [Rational; 7] = [Rational::ZERO; 7];

/// The classified outcome of computing an `is` target value in the tower.
///
/// Separated from [`BuiltinOutcome`] so the dispatch is over *values*, not roles: the
/// caller maps [`Computed::Value`] into a generate/filter role, [`Computed::Error`]
/// into [`BuiltinOutcome::Error`], and [`Computed::Gap`] (an operation the algebra does
/// not define for the operand kinds) into [`BuiltinOutcome::Unbound`].
enum Computed {
    /// A computed tower value to bind or filter against.
    Value(Value),
    /// A domain / precision / dimensional error.
    Error(BuiltinError),
    /// The operation is undefined for these operand kinds — a declared gap.
    Gap,
}

/// The classified outcome of a comparison over the tower.
enum CompareResult {
    /// A filter verdict.
    Filter(bool),
    /// A dimensional / precision error (incommensurable compare, overflow).
    Error(BuiltinError),
    /// The comparison is undefined for these operand kinds — a declared gap.
    Gap,
}

/// Compose two SI exponent vectors componentwise: dimension *product* (`subtract ==
/// false`, exponent addition) or *quotient* (`subtract == true`, exponent
/// subtraction). Every componentwise `q_add`/`q_sub` is checked, so an exponent that
/// overflows `i64` is [`BuiltinError::Overflow`], never a wraparound.
fn dim_compose(
    a: &[Rational; 7],
    b: &[Rational; 7],
    subtract: bool,
) -> Result<[Rational; 7], BuiltinError> {
    let mut out = ZERO_DIM;
    for (slot, (l, r)) in out.iter_mut().zip(a.iter().zip(b.iter())) {
        *slot = if subtract { q_sub(l, r) } else { q_add(l, r) }?;
    }
    Ok(out)
}

/// Dimension-on-dimension arithmetic: `Mul` composes (exponent addition, the
/// `math:integralDimensionCompositionLaw` dimProduct), `Div`/`ExactDiv` quotients
/// (exponent subtraction). `Add`/`Sub` are undefined on bare dimensions — dimensions
/// form a module under composition, which is multiplicative; there is no additive
/// operation on the exponent vectors themselves — so they are a declared gap.
fn dim_dim(a: &[Rational; 7], b: &[Rational; 7], op: ArithOp) -> Computed {
    match op {
        ArithOp::Mul => dim_compose(a, b, false)
            .map_or_else(Computed::Error, |d| Computed::Value(Value::Dim(d))),
        ArithOp::ExactDiv | ArithOp::Div => {
            dim_compose(a, b, true).map_or_else(Computed::Error, |d| Computed::Value(Value::Dim(d)))
        }
        ArithOp::Add | ArithOp::Sub => Computed::Gap,
    }
}

/// Dimensioned-quantity arithmetic (quantity calculus).
///
/// `Add`/`Sub` are the homogeneity gate: they REQUIRE an equal dimension vector (an
/// unequal one is [`BuiltinError::DimensionMismatch`], anchoring
/// `math:DimensionalInhomogeneity`) and add / subtract the magnitudes over the shared
/// dimension. `Mul` multiplies the magnitudes and composes the dimensions; `Div` /
/// `ExactDiv` divide the magnitudes (exact ℚ — a dimensioned magnitude has no
/// truncating meaning) and quotient the dimensions. Every magnitude step is the shared
/// checked-ℚ kernel, so overflow / ÷0 is a declared error.
fn quantity_arith(
    m1: &Rational,
    d1: &[Rational; 7],
    m2: &Rational,
    d2: &[Rational; 7],
    op: ArithOp,
) -> Computed {
    match op {
        ArithOp::Add | ArithOp::Sub => {
            if d1 != d2 {
                return Computed::Error(BuiltinError::DimensionMismatch);
            }
            let mag = if matches!(op, ArithOp::Add) {
                q_add(m1, m2)
            } else {
                q_sub(m1, m2)
            };
            match mag {
                Ok(m) => Computed::Value(Value::Quantity(m, *d1)),
                Err(e) => Computed::Error(e),
            }
        }
        ArithOp::Mul => {
            let mag = match q_mul(m1, m2) {
                Ok(m) => m,
                Err(e) => return Computed::Error(e),
            };
            dim_compose(d1, d2, false).map_or_else(Computed::Error, |d| {
                Computed::Value(Value::Quantity(mag, d))
            })
        }
        ArithOp::ExactDiv | ArithOp::Div => {
            let mag = match q_div(m1, m2) {
                Ok(m) => m,
                Err(e) => return Computed::Error(e),
            };
            dim_compose(d1, d2, true).map_or_else(Computed::Error, |d| {
                Computed::Value(Value::Quantity(mag, d))
            })
        }
    }
}

/// Compute the `is` target value for a fully-bound operand pair, type-directed over
/// the whole tower. The `(Value, Value)` match is EXHAUSTIVE — every operand-kind
/// combination is an explicit arm, so no `Value`/`ArithOp` pair is silently dropped;
/// an operation the algebra does not define is an explicit [`Computed::Gap`].
///
/// Scalar pairs keep the unchanged fast/exact behaviour (two integers under `+ - * //`
/// on the i64 fast path, exact `/` or any rational routing to the ℚ kernel). A scalar
/// mixed with a [`Value::Quantity`] promotes to a dimensionless quantity. A
/// [`Value::Dim`] combined with anything other than another [`Value::Dim`] is a gap
/// (composition is between two dimensions or two quantities only).
fn compute_is_value(lv: &Value, rv: &Value, op: ArithOp) -> Computed {
    match (lv, rv) {
        (Value::Int(l), Value::Int(r)) => match apply_arith_int(*l, op, *r) {
            Some(Ok(v)) => Computed::Value(Value::Int(v)),
            Some(Err(e)) => Computed::Error(e),
            // `/` (exact) on two integers is ℚ-only: route to the exact kernel.
            None => scalar_is(lv, rv, op),
        },
        (Value::Int(_) | Value::Rat(_), Value::Int(_) | Value::Rat(_)) => scalar_is(lv, rv, op),
        (Value::Dim(a), Value::Dim(b)) => dim_dim(a, b, op),
        (Value::Quantity(m1, d1), Value::Quantity(m2, d2)) => quantity_arith(m1, d1, m2, d2, op),
        (Value::Quantity(m1, d1), Value::Int(_) | Value::Rat(_)) => match scalar_rational(rv) {
            Some(Ok(m2)) => quantity_arith(m1, d1, &m2, &ZERO_DIM, op),
            Some(Err(e)) => Computed::Error(e),
            None => Computed::Gap,
        },
        (Value::Int(_) | Value::Rat(_), Value::Quantity(m2, d2)) => match scalar_rational(lv) {
            Some(Ok(m1)) => quantity_arith(&m1, &ZERO_DIM, m2, d2, op),
            Some(Err(e)) => Computed::Error(e),
            None => Computed::Gap,
        },
        // A dimension combined with a scalar or a quantity is undefined.
        (Value::Dim(_), Value::Int(_) | Value::Rat(_) | Value::Quantity(_, _)) => Computed::Gap,
        (Value::Int(_) | Value::Rat(_) | Value::Quantity(_, _), Value::Dim(_)) => Computed::Gap,
    }
}

/// The exact-ℚ scalar arm of [`compute_is_value`]: routes a scalar operand pair
/// through [`compute_rational`] and commits a [`Value::Rat`].
fn scalar_is(lv: &Value, rv: &Value, op: ArithOp) -> Computed {
    match compute_rational(lv, rv, op) {
        Some(Ok(r)) => Computed::Value(Value::Rat(r)),
        Some(Err(e)) => Computed::Error(e),
        // Unreachable for scalar operands (a dimensioned operand never routes here).
        None => Computed::Gap,
    }
}

/// Compare two dimensioned quantities: they must be commensurable (equal dimension
/// vector) to compare magnitudes; an unequal dimension is
/// [`BuiltinError::DimensionMismatch`] (incommensurable quantities are not ordered).
fn quantity_compare(
    m1: &Rational,
    d1: &[Rational; 7],
    m2: &Rational,
    d2: &[Rational; 7],
    op: CmpOp,
) -> CompareResult {
    if d1 != d2 {
        return CompareResult::Error(BuiltinError::DimensionMismatch);
    }
    CompareResult::Filter(apply_compare_q(m1, op, m2))
}

/// The exact-ℚ scalar arm of [`compute_compare`].
fn scalar_compare(lv: &Value, rv: &Value, op: CmpOp) -> CompareResult {
    match (scalar_rational(lv), scalar_rational(rv)) {
        (Some(l), Some(r)) => {
            let l = match l {
                Ok(l) => l,
                Err(e) => return CompareResult::Error(e),
            };
            let r = match r {
                Ok(r) => r,
                Err(e) => return CompareResult::Error(e),
            };
            CompareResult::Filter(apply_compare_q(&l, op, &r))
        }
        // Unreachable for scalar operands.
        _ => CompareResult::Gap,
    }
}

/// Compare two tower values, type-directed and EXHAUSTIVE over operand kinds.
///
/// Two integers take the i64 fast path; a scalar pair promotes to a common ℚ.
/// Commensurability (`=:=` over two dimensions) is exact ℚ⁷ vector equality; any
/// ORDERING over bare dimensions is undefined (a gap). Two quantities (or a quantity
/// and a promoted dimensionless scalar) must be commensurable to compare magnitudes.
/// A dimension compared with a scalar or a quantity is a gap.
fn compute_compare(lv: &Value, rv: &Value, op: CmpOp) -> CompareResult {
    match (lv, rv) {
        (Value::Int(l), Value::Int(r)) => CompareResult::Filter(apply_compare(*l, op, *r)),
        (Value::Int(_) | Value::Rat(_), Value::Int(_) | Value::Rat(_)) => {
            scalar_compare(lv, rv, op)
        }
        (Value::Dim(a), Value::Dim(b)) => match op {
            CmpOp::Eq => CompareResult::Filter(a == b),
            // Dimensions carry no ordering — only commensurability (`=:=`).
            CmpOp::Gt | CmpOp::Lt | CmpOp::Ge | CmpOp::Le => CompareResult::Gap,
        },
        (Value::Quantity(m1, d1), Value::Quantity(m2, d2)) => quantity_compare(m1, d1, m2, d2, op),
        (Value::Quantity(m1, d1), Value::Int(_) | Value::Rat(_)) => match scalar_rational(rv) {
            Some(Ok(m2)) => quantity_compare(m1, d1, &m2, &ZERO_DIM, op),
            Some(Err(e)) => CompareResult::Error(e),
            None => CompareResult::Gap,
        },
        (Value::Int(_) | Value::Rat(_), Value::Quantity(m2, d2)) => match scalar_rational(lv) {
            Some(Ok(m1)) => quantity_compare(&m1, &ZERO_DIM, m2, d2, op),
            Some(Err(e)) => CompareResult::Error(e),
            None => CompareResult::Gap,
        },
        // A dimension compared with a scalar or a quantity is undefined.
        (Value::Dim(_), Value::Int(_) | Value::Rat(_) | Value::Quantity(_, _)) => {
            CompareResult::Gap
        }
        (Value::Int(_) | Value::Rat(_) | Value::Quantity(_, _), Value::Dim(_)) => {
            CompareResult::Gap
        }
    }
}

// ── math: dimension correspondence lowering (Principle-17) ───────────────────────

/// The `math:` grounding namespace (`https://blackcatinformatics.ca/math/`).
const MATH_NS: &str = "https://blackcatinformatics.ca/math/";

/// The seven SI base-dimension individual IRIs, in the fixed vector order shared with
/// the dimension transport (length, mass, time, electric current, temperature, amount
/// of substance, luminous intensity). The index of an IRI in this array IS its slot in
/// the `[Rational; 7]` exponent vector.
const BASE_DIMENSION_LOCALS: [&str; 7] = [
    "lengthDimension",
    "massDimension",
    "timeDimension",
    "electricCurrentDimension",
    "temperatureDimension",
    "amountOfSubstanceDimension",
    "luminousIntensityDimension",
];

/// Resolve a base-dimension IRI to its fixed exponent-vector index, or `None` when the
/// IRI is not one of the seven `math:BaseDimension` individuals (a non-base target).
fn base_dimension_index(iri: &str) -> Option<usize> {
    let local = iri.strip_prefix(MATH_NS)?;
    BASE_DIMENSION_LOCALS.iter().position(|l| *l == local)
}

/// One `math:DimensionExponent` cell in structured form: the `math:BaseDimension` IRI
/// it raises (`math:exponentOfDimension`) and its exact-rational power as the
/// `math:exponentNumerator` / `math:exponentDenominator` object surfaces.
pub(crate) struct DimensionExponentCell<'a> {
    /// The `math:exponentOfDimension` target — must be one of the seven base IRIs.
    pub(crate) base_dimension: &'a str,
    /// The `math:exponentNumerator` object surface (integer literal or bare token).
    pub(crate) exponent_numerator: &'a str,
    /// The `math:exponentDenominator` object surface (integer literal or bare token).
    pub(crate) exponent_denominator: &'a str,
}

/// Assemble a canonical [`Value::Dim`] from a `math:DerivedDimension`'s
/// `math:baseDimensionExponent` cells (Principle-17 lowering of the `math:` dimension
/// surface into the engine value tower). Unmentioned base dimensions default to the
/// zero exponent — a derived dimension names only the bases it involves.
///
/// This is the consumer of [`rational_from_components`]: each cell's exact rational
/// exponent is lowered through it. A cell naming a non-base target, a duplicate base
/// (an ambiguous exponent vector), a missing/non-integer numerator or denominator, or
/// a zero denominator (not a rational power) is [`BuiltinError::MalformedDimension`] —
/// the same class the transport decoder raises, so the structured-cells ↔ transport
/// round trip agrees. An `i64::MIN`-magnitude exponent is an honest
/// [`BuiltinError::Overflow`].
pub(crate) fn dim_from_exponent_cells(
    cells: &[DimensionExponentCell<'_>],
) -> Result<Value, BuiltinError> {
    let mut dim = ZERO_DIM;
    let mut seen = [false; 7];
    for cell in cells {
        let idx =
            base_dimension_index(cell.base_dimension).ok_or(BuiltinError::MalformedDimension)?;
        if seen[idx] {
            // A second cell for the same base dimension is an ambiguous exponent.
            return Err(BuiltinError::MalformedDimension);
        }
        seen[idx] = true;
        let exponent =
            match rational_from_components(cell.exponent_numerator, cell.exponent_denominator) {
                Some(Ok(Value::Rat(r))) => r,
                // A zero denominator is not a rational power — a malformed dimension.
                Some(Err(BuiltinError::ZeroDenominator)) => {
                    return Err(BuiltinError::MalformedDimension);
                }
                // An honest numeric limit (i64::MIN magnitude) stays an overflow.
                Some(Err(e)) => return Err(e),
                // A non-integer / missing exponent surface is malformed.
                Some(Ok(_)) | None => return Err(BuiltinError::MalformedDimension),
            };
        dim[idx] = exponent;
    }
    Ok(Value::Dim(dim))
}

/// Assemble a [`Value::Quantity`] from a `math:Quantity`'s magnitude
/// (`math:quantityValue`, a `math:RationalValue` given as numerator / denominator
/// surfaces) and an already-lowered dimension vector (Principle-17 lowering).
///
/// The outer `Option` is `None` when the magnitude surfaces are not integers (a
/// malformed `RationalValue` node — routed to a gap, never fabricated); the inner
/// `Result` carries a magnitude [`BuiltinError::ZeroDenominator`] / overflow. Consumes
/// [`rational_from_components`] for the magnitude.
pub(crate) fn quantity_from_components(
    magnitude_numerator: &str,
    magnitude_denominator: &str,
    dimension: [Rational; 7],
) -> Option<Result<Value, BuiltinError>> {
    match rational_from_components(magnitude_numerator, magnitude_denominator)? {
        Ok(Value::Rat(mag)) => Some(Ok(Value::Quantity(mag, dimension))),
        // `rational_from_components` only ever yields a `Value::Rat`.
        Ok(_) => None,
        Err(e) => Some(Err(e)),
    }
}

/// Lower a `math:RationalValue` resource's `math:numerator` / `math:denominator`
/// integer bindings to an exact [`Value::Rat`] operand (Principle-17 correspondence
/// lowering of the `math:` rational surface into the engine value tower).
///
/// `numerator` / `denominator` are the bound object surfaces of the two integer
/// properties (canonical `"N"^^<…#integer>` or a bare integer token). The outer
/// `Option` is `None` when either surface is not an integer (a malformed
/// `RationalValue` node — routed to a gap by the caller, never fabricated); the
/// inner `Result` carries [`BuiltinError::ZeroDenominator`] / [`BuiltinError::Overflow`]
/// when the pair has no normal form.
///
/// Round-trip witness: `resource(num, den)` ↔ transport — the produced
/// [`Value::Rat`] emits ([`emit_surface`]) to the rational transport and parses
/// ([`parse_value_surface`]) back to the identical value.
pub(crate) fn rational_from_components(
    numerator: &str,
    denominator: &str,
) -> Option<Result<Value, BuiltinError>> {
    let num = parse_integer_surface(numerator)?;
    let den = parse_integer_surface(denominator)?;
    Some(Rational::new(num, den).map(Value::Rat))
}

/// Evaluate `builtin` against the current substitution, resolving variables via
/// `lookup` (variable name → bound surface, or `None` when unbound).
pub(crate) fn eval<'a>(
    builtin: &QBuiltin,
    lookup: &impl Fn(&str) -> Option<Cow<'a, str>>,
) -> BuiltinOutcome {
    match builtin {
        QBuiltin::Is {
            target,
            lhs,
            op,
            rhs,
        } => {
            // Both operands must be bound numeric values to compute; anything else
            // (unbound or non-numeric) is a declared mode gap.
            let (lv, rv) = match (resolve_operand(lhs, lookup), resolve_operand(rhs, lookup)) {
                (Operand::Bound(l), Operand::Bound(r)) => (l, r),
                _ => return BuiltinOutcome::Unbound,
            };
            // Type-directed dispatch over the whole tower: two integers under a
            // ℤ-shared operator take the i64 fast path and commit `Value::Int`; exact
            // `/` or any rational operand routes to the exact-ℚ kernel and commits
            // `Value::Rat`; dimensioned operands evaluate the dimension algebra and
            // commit `Value::Dim` / `Value::Quantity`. An operation the algebra does
            // not define is a declared gap.
            let value: Value = match compute_is_value(&lv, &rv, *op) {
                Computed::Value(v) => v,
                Computed::Error(e) => return BuiltinOutcome::Error(e),
                Computed::Gap => return BuiltinOutcome::Unbound,
            };
            // Target role: unbound variable → generate; bound value → filter on
            // value equality; bound non-numeric → filter false (`foo is 1+2` fails,
            // it is not a gap).
            match target {
                QTerm::Var(v) => match lookup(v) {
                    None => BuiltinOutcome::Generate {
                        var: v.clone(),
                        value,
                    },
                    Some(surface) => match parse_value_surface(&surface) {
                        Some(t) => BuiltinOutcome::Filter(t == value),
                        None => BuiltinOutcome::Filter(false),
                    },
                },
                QTerm::Num(t) => BuiltinOutcome::Filter(Value::Int(*t) == value),
                QTerm::Const(c) => match parse_value_surface(c) {
                    Some(t) => BuiltinOutcome::Filter(t == value),
                    None => BuiltinOutcome::Filter(false),
                },
                // A structured target can never equal a computed value: filter false.
                QTerm::Struct(_) => BuiltinOutcome::Filter(false),
            }
        }
        QBuiltin::Compare { lhs, op, rhs } => {
            let (lv, rv) = match (resolve_operand(lhs, lookup), resolve_operand(rhs, lookup)) {
                (Operand::Bound(l), Operand::Bound(r)) => (l, r),
                // Either operand unbound or non-numeric → gap.
                _ => return BuiltinOutcome::Unbound,
            };
            // Two integers compare on the i64 fast path; a rational operand promotes
            // both to a common ℚ; dimensioned operands compare via the dimension
            // algebra (commensurability for `=:=` over dimensions, magnitude compare
            // over commensurable quantities). An undefined comparison is a gap.
            match compute_compare(&lv, &rv, *op) {
                CompareResult::Filter(b) => BuiltinOutcome::Filter(b),
                CompareResult::Error(e) => BuiltinOutcome::Error(e),
                CompareResult::Gap => BuiltinOutcome::Unbound,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a `lookup` from a small set of (var, surface) pairs.
    fn env(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<Cow<'static, str>> + use<> {
        let owned: Vec<(String, String)> = pairs
            .iter()
            .map(|(n, s)| ((*n).to_owned(), (*s).to_owned()))
            .collect();
        move |v: &str| {
            owned
                .iter()
                .find(|(name, _)| name == v)
                .map(|(_, surface)| Cow::Owned(surface.clone()))
        }
    }

    fn var(name: &str) -> QTerm {
        QTerm::Var(name.to_owned())
    }

    fn is(target: QTerm, lhs: QTerm, op: ArithOp, rhs: QTerm) -> QBuiltin {
        QBuiltin::Is {
            target,
            lhs,
            op,
            rhs,
        }
    }

    fn cmp(lhs: QTerm, op: CmpOp, rhs: QTerm) -> QBuiltin {
        QBuiltin::Compare { lhs, op, rhs }
    }

    /// A generator outcome binding `var` to `Value::Int(value)` — the integer
    /// commit form, unchanged across the carrier generalization.
    fn gen_int(var: &str, value: i64) -> BuiltinOutcome {
        BuiltinOutcome::Generate {
            var: var.to_owned(),
            value: Value::Int(value),
        }
    }

    // ── surface round-trip ──────────────────────────────────────────────────

    #[test]
    fn emit_and_parse_round_trip() {
        for n in [-7, -1, 0, 1, 42, i64::MAX, i64::MIN] {
            let s = emit_integer_surface(n);
            assert_eq!(parse_integer_surface(&s), Some(n), "round-trip {n}");
        }
        // Canonical form.
        assert_eq!(
            emit_integer_surface(3),
            "\"3\"^^<http://www.w3.org/2001/XMLSchema#integer>"
        );
    }

    #[test]
    fn parse_accepts_bare_integer_and_rejects_non_numeric() {
        assert_eq!(parse_integer_surface("5"), Some(5));
        assert_eq!(parse_integer_surface("-5"), Some(-5));
        assert_eq!(parse_integer_surface("<https://example.org/x>"), None);
        assert_eq!(parse_integer_surface("\"hello\""), None);
        // A decimal-typed literal is not an xsd:integer.
        assert_eq!(
            parse_integer_surface("\"1.5\"^^<http://www.w3.org/2001/XMLSchema#decimal>"),
            None
        );
    }

    // ── generator mode (unbound target) ─────────────────────────────────────

    #[test]
    fn is_generates_when_target_unbound() {
        // N is M + 1, M = 2, N free → Generate N = 3.
        let lookup = env(&[("M", "\"2\"^^<http://www.w3.org/2001/XMLSchema#integer>")]);
        let b = is(var("N"), var("M"), ArithOp::Add, QTerm::Num(1));
        assert_eq!(eval(&b, &lookup), gen_int("N", 3));
    }

    #[test]
    fn is_generator_over_bare_num_operands() {
        // X is 6 // 4 → 1 (truncation), X free.
        let lookup = env(&[]);
        let b = is(var("X"), QTerm::Num(6), ArithOp::Div, QTerm::Num(4));
        assert_eq!(eval(&b, &lookup), gen_int("X", 1));
    }

    // ── filter mode (bound target) ──────────────────────────────────────────

    #[test]
    fn is_filters_when_target_bound_numeric() {
        let pass = env(&[("N", "3"), ("M", "2")]);
        let b = is(var("N"), var("M"), ArithOp::Add, QTerm::Num(1));
        assert_eq!(eval(&b, &pass), BuiltinOutcome::Filter(true));

        let fail = env(&[("N", "9"), ("M", "2")]);
        assert_eq!(eval(&b, &fail), BuiltinOutcome::Filter(false));
    }

    #[test]
    fn is_bound_non_numeric_target_is_filter_false_not_gap() {
        // `foo is 1 + 2` is a filter-false, never a gap.
        let lookup = env(&[("T", "<https://example.org/foo>")]);
        let b = is(var("T"), QTerm::Num(1), ArithOp::Add, QTerm::Num(2));
        assert_eq!(eval(&b, &lookup), BuiltinOutcome::Filter(false));
    }

    #[test]
    fn is_literal_num_target_is_filter() {
        // `3 is 1 + 2` → true; `4 is 1 + 2` → false.
        let lookup = env(&[]);
        assert_eq!(
            eval(
                &is(QTerm::Num(3), QTerm::Num(1), ArithOp::Add, QTerm::Num(2)),
                &lookup
            ),
            BuiltinOutcome::Filter(true)
        );
        assert_eq!(
            eval(
                &is(QTerm::Num(4), QTerm::Num(1), ArithOp::Add, QTerm::Num(2)),
                &lookup
            ),
            BuiltinOutcome::Filter(false)
        );
    }

    // ── unbound operand → declared gap ──────────────────────────────────────

    #[test]
    fn is_unbound_operand_is_gap() {
        let lookup = env(&[]); // M unbound
        let b = is(var("N"), var("M"), ArithOp::Add, QTerm::Num(1));
        assert_eq!(eval(&b, &lookup), BuiltinOutcome::Unbound);
    }

    #[test]
    fn compare_unbound_operand_is_gap() {
        let lookup = env(&[("N", "5")]); // K unbound
        let b = cmp(var("N"), CmpOp::Gt, var("K"));
        assert_eq!(eval(&b, &lookup), BuiltinOutcome::Unbound);
    }

    // ── arithmetic semantics ────────────────────────────────────────────────

    #[test]
    fn div_truncates_toward_zero_with_negatives() {
        let lookup = env(&[]);
        // (-7) // 2 == -3 (truncation toward zero), NOT -4 (floor).
        assert_eq!(
            eval(
                &is(var("X"), QTerm::Num(-7), ArithOp::Div, QTerm::Num(2)),
                &lookup
            ),
            gen_int("X", -3)
        );
        assert_eq!(
            eval(
                &is(var("X"), QTerm::Num(7), ArithOp::Div, QTerm::Num(-2)),
                &lookup
            ),
            gen_int("X", -3)
        );
    }

    #[test]
    fn sub_and_mul_over_negatives() {
        let lookup = env(&[]);
        assert_eq!(
            eval(
                &is(var("X"), QTerm::Num(-7), ArithOp::Sub, QTerm::Num(-1)),
                &lookup
            ),
            gen_int("X", -6)
        );
        assert_eq!(
            eval(
                &is(var("X"), QTerm::Num(-3), ArithOp::Mul, QTerm::Num(4)),
                &lookup
            ),
            gen_int("X", -12)
        );
    }

    #[test]
    fn division_by_zero_is_error() {
        let lookup = env(&[]);
        let b = is(var("X"), QTerm::Num(1), ArithOp::Div, QTerm::Num(0));
        assert_eq!(
            eval(&b, &lookup),
            BuiltinOutcome::Error(BuiltinError::ZeroDivisor)
        );
    }

    #[test]
    fn overflow_is_error_not_wraparound() {
        let lookup = env(&[]);
        let b = is(var("X"), QTerm::Num(i64::MAX), ArithOp::Add, QTerm::Num(1));
        assert_eq!(
            eval(&b, &lookup),
            BuiltinOutcome::Error(BuiltinError::Overflow)
        );
        let b2 = is(var("X"), QTerm::Num(i64::MIN), ArithOp::Div, QTerm::Num(-1));
        assert_eq!(
            eval(&b2, &lookup),
            BuiltinOutcome::Error(BuiltinError::Overflow)
        );
    }

    // ── comparison semantics ────────────────────────────────────────────────

    #[test]
    fn every_comparison_operator() {
        let lookup = env(&[]);
        let cases = [
            (CmpOp::Gt, 3, 2, true),
            (CmpOp::Gt, 2, 2, false),
            (CmpOp::Lt, 2, 3, true),
            (CmpOp::Ge, 2, 2, true),
            (CmpOp::Ge, 1, 2, false),
            (CmpOp::Le, 2, 2, true),
            (CmpOp::Le, 3, 2, false),
            (CmpOp::Eq, 2, 2, true),
            (CmpOp::Eq, 2, 3, false),
        ];
        for (op, l, r, expected) in cases {
            assert_eq!(
                eval(&cmp(QTerm::Num(l), op, QTerm::Num(r)), &lookup),
                BuiltinOutcome::Filter(expected),
                "{l} {} {r}",
                op.token()
            );
        }
    }

    #[test]
    fn compare_is_numeric_value_equality() {
        // `=:=` compares values, not structural terms; a typed-literal surface and
        // a bare integer with the same value are equal.
        let lookup = env(&[
            ("A", "\"5\"^^<http://www.w3.org/2001/XMLSchema#integer>"),
            ("B", "5"),
        ]);
        assert_eq!(
            eval(&cmp(var("A"), CmpOp::Eq, var("B")), &lookup),
            BuiltinOutcome::Filter(true)
        );
    }

    // ── Rational: normalization, sign, canonical Eq/Hash ────────────────────

    fn rat(num: i64, den: i64) -> Rational {
        Rational::new(num, den).expect("well-formed rational")
    }

    fn hash_of(r: &Rational) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        r.hash(&mut h);
        h.finish()
    }

    #[test]
    fn rational_normalizes_to_lowest_terms() {
        // 2/4 reduces to 1/2, and both forms are equal AND hash-equal.
        let half = rat(1, 2);
        let two_quarters = rat(2, 4);
        assert_eq!(half.num, 1);
        assert_eq!(half.den, 2);
        assert_eq!(two_quarters.num, 1, "no unnormalized numerator is stored");
        assert_eq!(two_quarters.den, 2, "no unnormalized denominator is stored");
        assert_eq!(half, two_quarters);
        assert_eq!(hash_of(&half), hash_of(&two_quarters));
    }

    #[test]
    fn rational_carries_sign_in_numerator_with_positive_denominator() {
        // 1/-2 normalizes to (-1)/2 with a positive denominator.
        let r = rat(1, -2);
        assert_eq!(r.num, -1);
        assert_eq!(r.den, 2);
        // -3/-6 → 1/2 (sign cancels, reduced).
        let s = rat(-3, -6);
        assert_eq!(s.num, 1);
        assert_eq!(s.den, 2);
        // 0/5 → 0/1 canonical zero.
        let z = rat(0, 5);
        assert_eq!(z.num, 0);
        assert_eq!(z.den, 1);
        assert_eq!(z, Rational::ZERO);
    }

    #[test]
    fn rational_zero_denominator_is_error() {
        assert_eq!(Rational::new(1, 0), Err(BuiltinError::ZeroDenominator));
        assert_eq!(Rational::new(0, 0), Err(BuiltinError::ZeroDenominator));
    }

    #[test]
    fn rational_i64_min_sign_flip_is_overflow_not_panic() {
        // A negative denominator forces a sign-flip of i64::MIN → unrepresentable.
        assert_eq!(Rational::new(1, i64::MIN), Err(BuiltinError::Overflow));
        // A numerator magnitude of i64::MIN is likewise unrepresentable.
        assert_eq!(Rational::new(i64::MIN, 1), Err(BuiltinError::Overflow));
        assert_eq!(Rational::new(i64::MIN, 4), Err(BuiltinError::Overflow));
    }

    // ── checked-ℚ kernel ────────────────────────────────────────────────────

    #[test]
    fn kernel_add_sub_mul_div_exact() {
        // 1/2 + 1/3 = 5/6.
        assert_eq!(q_add(&rat(1, 2), &rat(1, 3)), Ok(rat(5, 6)));
        // 1/2 - 1/3 = 1/6.
        assert_eq!(q_sub(&rat(1, 2), &rat(1, 3)), Ok(rat(1, 6)));
        // 2/3 * 3/4 = 1/2 (cross-cancellation reduces the intermediates).
        assert_eq!(q_mul(&rat(2, 3), &rat(3, 4)), Ok(rat(1, 2)));
        // (2/3) / (4/9) = 3/2.
        assert_eq!(q_div(&rat(2, 3), &rat(4, 9)), Ok(rat(3, 2)));
        // Sum that cancels to an integer: 1/2 + 1/2 = 1/1.
        assert_eq!(q_add(&rat(1, 2), &rat(1, 2)), Ok(rat(1, 1)));
    }

    #[test]
    fn kernel_division_by_zero_rational_is_zero_divisor() {
        assert_eq!(
            q_div(&rat(1, 2), &Rational::ZERO),
            Err(BuiltinError::ZeroDivisor)
        );
    }

    #[test]
    fn kernel_overflow_is_error_not_panic() {
        // Multiplying two near-maximal rationals overflows the numerator.
        let big = rat(i64::MAX, 1);
        assert_eq!(q_mul(&big, &big), Err(BuiltinError::Overflow));
        // Adding two large-denominator rationals overflows the common denominator.
        let a = rat(1, i64::MAX);
        let b = rat(1, i64::MAX - 1);
        assert_eq!(q_add(&a, &b), Err(BuiltinError::Overflow));
    }

    // ── Value transport round-trip (every committable variant) ──────────────

    #[test]
    fn value_transport_round_trip_each_variant() {
        let dim = [
            rat(1, 1),
            rat(0, 1),
            rat(-2, 1),
            rat(3, 2),
            rat(0, 1),
            rat(0, 1),
            rat(0, 1),
        ];
        let cases = [
            Value::Int(42),
            Value::Int(-7),
            Value::Int(i64::MIN),
            Value::Rat(rat(3, 4)),
            Value::Rat(rat(-1, 2)),
            Value::Dim(dim),
            Value::Quantity(rat(5, 3), dim),
        ];
        for value in cases {
            let surface = emit_surface(&value);
            let parsed = parse_value_surface(&surface).expect("transport parses back");
            assert_eq!(parsed, value, "round-trip {surface}");
            // Byte-stable: re-emitting the parsed value reproduces the surface.
            assert_eq!(emit_surface(&parsed), surface);
        }
    }

    #[test]
    fn value_int_transport_is_the_integer_surface() {
        // Value::Int emits exactly the canonical integer surface and parses back.
        for n in [-7, 0, 1, 42, i64::MAX, i64::MIN] {
            let surface = emit_surface(&Value::Int(n));
            assert_eq!(surface, emit_integer_surface(n));
            assert_eq!(parse_value_surface(&surface), Some(Value::Int(n)));
        }
    }

    #[test]
    fn malformed_dimension_transport_declines_to_non_numeric() {
        // A dimension transport with the wrong arity does not decode to a value.
        let bad = format!("\"1/1,0/1\"^^<{XSD_DIMENSION_TRANSPORT}>");
        assert_eq!(parse_value_surface(&bad), None);
        // The producing decode reports the malformed-dimension class directly.
        assert_eq!(
            parse_dimension_lex("1/1,0/1"),
            Err(BuiltinError::MalformedDimension)
        );
    }

    // ── Scalar-ℚ dispatch: exact `/`, rational operands, mode matrix ─────────

    /// A generator outcome binding `var` to `Value::Rat(num/den)` — the exact-ℚ
    /// commit form.
    fn gen_rat(var: &str, num: i64, den: i64) -> BuiltinOutcome {
        BuiltinOutcome::Generate {
            var: var.to_owned(),
            value: Value::Rat(rat(num, den)),
        }
    }

    /// The bound transport surface of a rational, for seeding a `lookup`.
    fn rat_surface(num: i64, den: i64) -> String {
        emit_surface(&Value::Rat(rat(num, den)))
    }

    #[test]
    fn exact_div_generates_normalized_rational() {
        // Q is 6 / 4 → 3/2 (exact rational division, Q free), canonically reduced.
        let lookup = env(&[]);
        let b = is(var("Q"), QTerm::Num(6), ArithOp::ExactDiv, QTerm::Num(4));
        assert_eq!(eval(&b, &lookup), gen_rat("Q", 3, 2));
    }

    #[test]
    fn exact_div_emits_only_the_normalized_surface() {
        // 6/4 commits as 3/2 — no unnormalized `6/4` transport surface is ever emitted.
        let lookup = env(&[]);
        let b = is(var("Q"), QTerm::Num(6), ArithOp::ExactDiv, QTerm::Num(4));
        let BuiltinOutcome::Generate { value, .. } = eval(&b, &lookup) else {
            panic!("exact division generates");
        };
        assert_eq!(value, Value::Rat(rat(3, 2)));
        assert_eq!(
            emit_surface(&value),
            format!("\"3/2\"^^<{XSD_RATIONAL_TRANSPORT}>")
        );
    }

    #[test]
    fn integer_truncating_and_exact_division_are_distinct_operators() {
        // `//` truncates on two integers (Int 1); `/` is exact (Rat 3/2).
        let lookup = env(&[]);
        assert_eq!(
            eval(
                &is(var("X"), QTerm::Num(6), ArithOp::Div, QTerm::Num(4)),
                &lookup
            ),
            gen_int("X", 1)
        );
        assert_eq!(
            eval(
                &is(var("X"), QTerm::Num(6), ArithOp::ExactDiv, QTerm::Num(4)),
                &lookup
            ),
            gen_rat("X", 3, 2)
        );
    }

    #[test]
    fn rational_operand_promotes_integer_and_computes_exactly() {
        // 1/2 + 1 = 3/2: a rational lhs mixes with an integer rhs by promotion.
        let lookup = env(&[("H", &rat_surface(1, 2))]);
        let b = is(var("X"), var("H"), ArithOp::Add, QTerm::Num(1));
        assert_eq!(eval(&b, &lookup), gen_rat("X", 3, 2));
        // 2/3 * 3/4 = 1/2 over two rational operands (cross-cancels).
        let lookup2 = env(&[("A", &rat_surface(2, 3)), ("B", &rat_surface(3, 4))]);
        let b2 = is(var("X"), var("A"), ArithOp::Mul, var("B"));
        assert_eq!(eval(&b2, &lookup2), gen_rat("X", 1, 2));
    }

    #[test]
    fn rational_filter_passes_and_fails_over_bound_rational_target() {
        // Target bound to the matching rational → keep; to a different one → prune.
        let pass = env(&[("T", &rat_surface(3, 2))]);
        let b = is(var("T"), QTerm::Num(6), ArithOp::ExactDiv, QTerm::Num(4));
        assert_eq!(eval(&b, &pass), BuiltinOutcome::Filter(true));

        let fail = env(&[("T", &rat_surface(5, 2))]);
        assert_eq!(eval(&b, &fail), BuiltinOutcome::Filter(false));
    }

    #[test]
    fn rational_bound_non_numeric_target_is_filter_false_not_gap() {
        // `foo is 6 / 4` (foo a bound IRI) is a filter-false, never a gap.
        let lookup = env(&[("T", "<https://example.org/foo>")]);
        let b = is(var("T"), QTerm::Num(6), ArithOp::ExactDiv, QTerm::Num(4));
        assert_eq!(eval(&b, &lookup), BuiltinOutcome::Filter(false));
    }

    #[test]
    fn exact_div_unbound_operand_is_gap() {
        // M unbound with the exact-`/` operator is still a declared mode gap.
        let lookup = env(&[]);
        let b = is(var("X"), var("M"), ArithOp::ExactDiv, QTerm::Num(2));
        assert_eq!(eval(&b, &lookup), BuiltinOutcome::Unbound);
    }

    #[test]
    fn exact_div_by_zero_is_zero_divisor() {
        let lookup = env(&[]);
        let b = is(var("X"), QTerm::Num(1), ArithOp::ExactDiv, QTerm::Num(0));
        assert_eq!(
            eval(&b, &lookup),
            BuiltinOutcome::Error(BuiltinError::ZeroDivisor)
        );
        // A rational divisor of value zero is likewise a zero-divisor.
        let lookup2 = env(&[("Z", &rat_surface(0, 5))]);
        let b2 = is(var("X"), QTerm::Num(1), ArithOp::ExactDiv, var("Z"));
        assert_eq!(
            eval(&b2, &lookup2),
            BuiltinOutcome::Error(BuiltinError::ZeroDivisor)
        );
    }

    #[test]
    fn rational_overflow_is_error_not_wraparound() {
        // (i64::MAX/1) * (i64::MAX/1) overflows the numerator on the ℚ path.
        let lookup = env(&[("A", &rat_surface(i64::MAX, 1))]);
        let b = is(var("X"), var("A"), ArithOp::Mul, QTerm::Num(i64::MAX));
        assert_eq!(
            eval(&b, &lookup),
            BuiltinOutcome::Error(BuiltinError::Overflow)
        );
    }

    #[test]
    fn every_op_over_rationals_via_exact_dispatch() {
        // Each operator with at least one rational operand routes to the ℚ kernel.
        let lookup = env(&[("A", &rat_surface(1, 2)), ("B", &rat_surface(1, 3))]);
        let cases = [
            (ArithOp::Add, rat(5, 6)),
            (ArithOp::Sub, rat(1, 6)),
            (ArithOp::Mul, rat(1, 6)),
            (ArithOp::ExactDiv, rat(3, 2)),
        ];
        for (op, expected) in cases {
            let b = is(var("X"), var("A"), op, var("B"));
            assert_eq!(
                eval(&b, &lookup),
                BuiltinOutcome::Generate {
                    var: "X".to_owned(),
                    value: Value::Rat(expected)
                },
                "1/2 {} 1/3",
                op.token()
            );
        }
        // Truncating `//` with a rational operand rounds the exact quotient toward
        // zero: (7/2) // (1/1) = 3.
        let lookup2 = env(&[("A", &rat_surface(7, 2))]);
        let b = is(var("X"), var("A"), ArithOp::Div, QTerm::Num(1));
        assert_eq!(eval(&b, &lookup2), gen_rat("X", 3, 1));
    }

    #[test]
    fn every_comparison_over_rationals() {
        // 1/2 vs 1/3: 1/2 > 1/3, so the ordering operators resolve exactly.
        let lookup = env(&[("A", &rat_surface(1, 2)), ("B", &rat_surface(1, 3))]);
        let cases = [
            (CmpOp::Gt, true),
            (CmpOp::Lt, false),
            (CmpOp::Ge, true),
            (CmpOp::Le, false),
            (CmpOp::Eq, false),
        ];
        for (op, expected) in cases {
            assert_eq!(
                eval(&cmp(var("A"), op, var("B")), &lookup),
                BuiltinOutcome::Filter(expected),
                "1/2 {} 1/3",
                op.token()
            );
        }
    }

    #[test]
    fn rational_equality_is_canonical_across_unreduced_forms() {
        // 1/2 =:= 2/4 → true (2/4 normalizes to 1/2 on construction).
        let lookup = env(&[("A", &rat_surface(1, 2)), ("B", &rat_surface(2, 4))]);
        assert_eq!(
            eval(&cmp(var("A"), CmpOp::Eq, var("B")), &lookup),
            BuiltinOutcome::Filter(true)
        );
        // A rational and an integer of equal value compare equal (mixed promotion):
        // 6/2 as a rational transport equals the integer 3.
        let mixed = env(&[("R", &rat_surface(6, 2)), ("I", "3")]);
        assert_eq!(
            eval(&cmp(var("R"), CmpOp::Eq, var("I")), &mixed),
            BuiltinOutcome::Filter(true)
        );
    }

    #[test]
    fn decimal_operand_is_rejected_from_the_exact_path() {
        // An xsd:decimal / xsd:double operand resolves to non-numeric → a declared
        // operand gap; it is NEVER coerced into a rational.
        let decimal = "\"1.5\"^^<http://www.w3.org/2001/XMLSchema#decimal>";
        let double = "\"1.5\"^^<http://www.w3.org/2001/XMLSchema#double>";
        assert_eq!(parse_value_surface(decimal), None);
        assert_eq!(parse_value_surface(double), None);
        // In `is` with the exact operator: gap, not a fabricated rational.
        let lookup = env(&[("D", decimal)]);
        let b = is(var("X"), var("D"), ArithOp::ExactDiv, QTerm::Num(2));
        assert_eq!(eval(&b, &lookup), BuiltinOutcome::Unbound);
        // In a comparison: likewise a gap.
        let b2 = cmp(var("D"), CmpOp::Gt, QTerm::Num(1));
        assert_eq!(eval(&b2, &lookup), BuiltinOutcome::Unbound);
    }

    // ── math:RationalValue correspondence lowering ──────────────────────────

    #[test]
    fn rational_from_components_round_trips_resource_to_transport() {
        // A math:RationalValue node (numerator 6, denominator 4) lowers to Rat(3/2),
        // and the emitted transport parses back to the identical value.
        let num = emit_integer_surface(6);
        let den = emit_integer_surface(4);
        let value = rational_from_components(&num, &den)
            .expect("integer surfaces decode")
            .expect("well-formed rational");
        assert_eq!(value, Value::Rat(rat(3, 2)));
        let surface = emit_surface(&value);
        assert_eq!(parse_value_surface(&surface), Some(value));

        // A bare-integer numerator/denominator pair is accepted too.
        assert_eq!(
            rational_from_components("1", "4"),
            Some(Ok(Value::Rat(rat(1, 4))))
        );
        // A zero denominator is a declared domain error, never a fabricated value.
        assert_eq!(
            rational_from_components("1", "0"),
            Some(Err(BuiltinError::ZeroDenominator))
        );
        // A non-integer component (a decimal) declines to `None` (malformed node).
        assert_eq!(
            rational_from_components("\"1.5\"^^<http://www.w3.org/2001/XMLSchema#decimal>", "4"),
            None
        );
    }

    // ── Dimension algebra (ℚ⁷ module) & dimensioned-quantity calculus ────────

    /// Build a `[Rational; 7]` from `(index, num, den)` exponents; unmentioned bases
    /// are the zero exponent.
    fn dim_of(pairs: &[(usize, i64, i64)]) -> [Rational; 7] {
        let mut d = [Rational::ZERO; 7];
        for (i, n, den) in pairs {
            d[*i] = rat(*n, *den);
        }
        d
    }

    /// The base-dimension vector indices (fixed SI order).
    const LEN: usize = 0;
    const TIME: usize = 2;

    fn dim_surface(d: &[Rational; 7]) -> String {
        emit_surface(&Value::Dim(*d))
    }

    fn qty_surface(mag_num: i64, mag_den: i64, d: &[Rational; 7]) -> String {
        emit_surface(&Value::Quantity(rat(mag_num, mag_den), *d))
    }

    fn gen_val(var: &str, value: Value) -> BuiltinOutcome {
        BuiltinOutcome::Generate {
            var: var.to_owned(),
            value,
        }
    }

    #[test]
    fn dimension_composition_adds_exponents_quotient_subtracts() {
        // L (length) and T (time) as base-dimension vectors.
        let length = dim_of(&[(LEN, 1, 1)]);
        let time = dim_of(&[(TIME, 1, 1)]);
        let lookup = env(&[("L", &dim_surface(&length)), ("T", &dim_surface(&time))]);

        // D is L * T → componentwise exponent ADDITION (dimension product).
        let product = dim_of(&[(LEN, 1, 1), (TIME, 1, 1)]);
        assert_eq!(
            eval(&is(var("D"), var("L"), ArithOp::Mul, var("T")), &lookup),
            gen_val("D", Value::Dim(product))
        );

        // D is L / T → componentwise exponent SUBTRACTION (dimension quotient), for
        // both the exact `/` and the truncating `//` spellings (a dimension quotient
        // has no integer-division meaning; both subtract exponents).
        let quotient = dim_of(&[(LEN, 1, 1), (TIME, -1, 1)]);
        for op in [ArithOp::ExactDiv, ArithOp::Div] {
            assert_eq!(
                eval(&is(var("D"), var("L"), op, var("T")), &lookup),
                gen_val("D", Value::Dim(quotient)),
                "L {} T subtracts exponents",
                op.token()
            );
        }
    }

    #[test]
    fn dimension_commensurability_equal_and_unequal() {
        // Velocity L·T⁻¹ compared to itself (=:= true) and to acceleration L·T⁻² (false).
        let velocity = dim_of(&[(LEN, 1, 1), (TIME, -1, 1)]);
        let velocity2 = dim_of(&[(LEN, 1, 1), (TIME, -1, 1)]);
        let accel = dim_of(&[(LEN, 1, 1), (TIME, -2, 1)]);
        let equal = env(&[
            ("A", &dim_surface(&velocity)),
            ("B", &dim_surface(&velocity2)),
        ]);
        assert_eq!(
            eval(&cmp(var("A"), CmpOp::Eq, var("B")), &equal),
            BuiltinOutcome::Filter(true)
        );
        let unequal = env(&[("A", &dim_surface(&velocity)), ("B", &dim_surface(&accel))]);
        assert_eq!(
            eval(&cmp(var("A"), CmpOp::Eq, var("B")), &unequal),
            BuiltinOutcome::Filter(false)
        );
    }

    #[test]
    fn dimension_addition_and_ordering_are_declared_gaps() {
        // Dimensions do not add, and they carry no ordering — both are declared gaps
        // (Unbound), never a fabricated dimension or a bogus verdict.
        let length = dim_of(&[(LEN, 1, 1)]);
        let time = dim_of(&[(TIME, 1, 1)]);
        let lookup = env(&[("L", &dim_surface(&length)), ("T", &dim_surface(&time))]);
        for op in [ArithOp::Add, ArithOp::Sub] {
            assert_eq!(
                eval(&is(var("D"), var("L"), op, var("T")), &lookup),
                BuiltinOutcome::Unbound,
                "bare dimensions do not support {}",
                op.token()
            );
        }
        for op in [CmpOp::Gt, CmpOp::Lt, CmpOp::Ge, CmpOp::Le] {
            assert_eq!(
                eval(&cmp(var("L"), op, var("T")), &lookup),
                BuiltinOutcome::Unbound,
                "dimensions carry no ordering ({})",
                op.token()
            );
        }
    }

    #[test]
    fn quantity_addition_requires_equal_dimension() {
        // A + B with equal dimension (both lengths) → magnitude sum over the shared dim.
        let length = dim_of(&[(LEN, 1, 1)]);
        let equal = env(&[
            ("A", &qty_surface(5, 1, &length)),
            ("B", &qty_surface(3, 1, &length)),
        ]);
        assert_eq!(
            eval(&is(var("Q"), var("A"), ArithOp::Add, var("B")), &equal),
            gen_val("Q", Value::Quantity(rat(8, 1), length))
        );
        assert_eq!(
            eval(&is(var("Q"), var("A"), ArithOp::Sub, var("B")), &equal),
            gen_val("Q", Value::Quantity(rat(2, 1), length))
        );

        // A + B with UNEQUAL dimension (length + time) is the intrinsic-homogeneity
        // failure: DimensionMismatch, NEVER a silently wrong quantity.
        let time = dim_of(&[(TIME, 1, 1)]);
        let unequal = env(&[
            ("A", &qty_surface(5, 1, &length)),
            ("B", &qty_surface(3, 1, &time)),
        ]);
        assert_eq!(
            eval(&is(var("Q"), var("A"), ArithOp::Add, var("B")), &unequal),
            BuiltinOutcome::Error(BuiltinError::DimensionMismatch)
        );
    }

    #[test]
    fn quantity_multiplication_multiplies_magnitude_and_composes_dimension() {
        // (2 L) * (3 T) = 6 (L·T); (6 L·T) / (3 T) = 2 L (magnitude ÷, dimension ⊖).
        let length = dim_of(&[(LEN, 1, 1)]);
        let time = dim_of(&[(TIME, 1, 1)]);
        let lt = dim_of(&[(LEN, 1, 1), (TIME, 1, 1)]);
        let lookup = env(&[
            ("A", &qty_surface(2, 1, &length)),
            ("B", &qty_surface(3, 1, &time)),
        ]);
        assert_eq!(
            eval(&is(var("Q"), var("A"), ArithOp::Mul, var("B")), &lookup),
            gen_val("Q", Value::Quantity(rat(6, 1), lt))
        );
        let div = env(&[
            ("N", &qty_surface(6, 1, &lt)),
            ("B", &qty_surface(3, 1, &time)),
        ]);
        assert_eq!(
            eval(&is(var("Q"), var("N"), ArithOp::ExactDiv, var("B")), &div),
            gen_val("Q", Value::Quantity(rat(2, 1), length))
        );
    }

    #[test]
    fn dimensionless_scalar_mixes_with_a_quantity() {
        // A dimensionless scalar promotes to a [0;7] quantity: (3 L) * 2 = 6 L.
        let length = dim_of(&[(LEN, 1, 1)]);
        let lookup = env(&[("A", &qty_surface(3, 1, &length))]);
        assert_eq!(
            eval(
                &is(var("Q"), var("A"), ArithOp::Mul, QTerm::Num(2)),
                &lookup
            ),
            gen_val("Q", Value::Quantity(rat(6, 1), length))
        );
        // Scalar on the LEFT promotes identically: 2 * (3 L) = 6 L.
        assert_eq!(
            eval(
                &is(var("Q"), QTerm::Num(2), ArithOp::Mul, var("A")),
                &lookup
            ),
            gen_val("Q", Value::Quantity(rat(6, 1), length))
        );
        // Adding a dimensionless scalar to a LENGTH is dimensionally inhomogeneous.
        assert_eq!(
            eval(
                &is(var("Q"), var("A"), ArithOp::Add, QTerm::Num(2)),
                &lookup
            ),
            BuiltinOutcome::Error(BuiltinError::DimensionMismatch)
        );
        // But a dimensionless QUANTITY adds to a scalar: (3 · 1) + 2 = 5 (dimensionless).
        let dimensionless = env(&[("Z", &qty_surface(3, 1, &ZERO_DIM))]);
        assert_eq!(
            eval(
                &is(var("Q"), var("Z"), ArithOp::Add, QTerm::Num(2)),
                &dimensionless
            ),
            gen_val("Q", Value::Quantity(rat(5, 1), ZERO_DIM))
        );
    }

    #[test]
    fn commensurable_quantities_compare_and_incommensurable_error() {
        // Two lengths compare by magnitude; a length vs a time is incommensurable.
        let length = dim_of(&[(LEN, 1, 1)]);
        let time = dim_of(&[(TIME, 1, 1)]);
        let commensurable = env(&[
            ("A", &qty_surface(5, 1, &length)),
            ("B", &qty_surface(3, 1, &length)),
        ]);
        assert_eq!(
            eval(&cmp(var("A"), CmpOp::Gt, var("B")), &commensurable),
            BuiltinOutcome::Filter(true)
        );
        assert_eq!(
            eval(&cmp(var("A"), CmpOp::Lt, var("B")), &commensurable),
            BuiltinOutcome::Filter(false)
        );
        let incommensurable = env(&[
            ("A", &qty_surface(5, 1, &length)),
            ("B", &qty_surface(5, 1, &time)),
        ]);
        assert_eq!(
            eval(&cmp(var("A"), CmpOp::Gt, var("B")), &incommensurable),
            BuiltinOutcome::Error(BuiltinError::DimensionMismatch)
        );
    }

    #[test]
    fn dimensioned_generator_filters_on_a_bound_target() {
        // A bound target that matches the composed dimension keeps the branch; a
        // mismatch prunes it; a bound non-numeric target is a filter-false, not a gap.
        let length = dim_of(&[(LEN, 1, 1)]);
        let time = dim_of(&[(TIME, 1, 1)]);
        let lt = dim_of(&[(LEN, 1, 1), (TIME, 1, 1)]);
        let pass = env(&[
            ("L", &dim_surface(&length)),
            ("T", &dim_surface(&time)),
            ("D", &dim_surface(&lt)),
        ]);
        assert_eq!(
            eval(&is(var("D"), var("L"), ArithOp::Mul, var("T")), &pass),
            BuiltinOutcome::Filter(true)
        );
        let fail = env(&[
            ("L", &dim_surface(&length)),
            ("T", &dim_surface(&time)),
            ("D", &dim_surface(&length)),
        ]);
        assert_eq!(
            eval(&is(var("D"), var("L"), ArithOp::Mul, var("T")), &fail),
            BuiltinOutcome::Filter(false)
        );
        let non_numeric = env(&[
            ("L", &dim_surface(&length)),
            ("T", &dim_surface(&time)),
            ("D", "<https://example.org/foo>"),
        ]);
        assert_eq!(
            eval(
                &is(var("D"), var("L"), ArithOp::Mul, var("T")),
                &non_numeric
            ),
            BuiltinOutcome::Filter(false)
        );
    }

    // ── math: dimension/quantity correspondence lowering ────────────────────

    fn base_iri(local: &str) -> String {
        format!("{MATH_NS}{local}")
    }

    #[test]
    fn dim_from_exponent_cells_round_trips_and_rejects_malformed() {
        // Velocity L·T⁻¹ from two structured cells, matching the direct vector.
        let length = base_iri("lengthDimension");
        let time = base_iri("timeDimension");
        let cells = [
            DimensionExponentCell {
                base_dimension: &length,
                exponent_numerator: "1",
                exponent_denominator: "1",
            },
            DimensionExponentCell {
                base_dimension: &time,
                exponent_numerator: "-1",
                exponent_denominator: "1",
            },
        ];
        let dim = dim_from_exponent_cells(&cells).expect("well-formed cells");
        assert_eq!(dim, Value::Dim(dim_of(&[(LEN, 1, 1), (TIME, -1, 1)])));
        // Structured cells ↔ transport witness: the lowered dimension emits to the
        // transport and parses back to the identical value.
        let surface = emit_surface(&dim);
        assert_eq!(parse_value_surface(&surface), Some(dim));

        // A fractional exponent (T^(1/2)) is exact — consumes the rational component
        // lowering with a non-unit denominator.
        let half = [DimensionExponentCell {
            base_dimension: &time,
            exponent_numerator: "1",
            exponent_denominator: "2",
        }];
        assert_eq!(
            dim_from_exponent_cells(&half),
            Ok(Value::Dim(dim_of(&[(TIME, 1, 2)])))
        );

        // A cell naming a non-base target is a malformed dimension.
        let derived = base_iri("velocityDimension");
        let bad_target = [DimensionExponentCell {
            base_dimension: &derived,
            exponent_numerator: "1",
            exponent_denominator: "1",
        }];
        assert_eq!(
            dim_from_exponent_cells(&bad_target),
            Err(BuiltinError::MalformedDimension)
        );

        // A duplicate base dimension is an ambiguous exponent vector — malformed.
        let dup = [
            DimensionExponentCell {
                base_dimension: &length,
                exponent_numerator: "1",
                exponent_denominator: "1",
            },
            DimensionExponentCell {
                base_dimension: &length,
                exponent_numerator: "2",
                exponent_denominator: "1",
            },
        ];
        assert_eq!(
            dim_from_exponent_cells(&dup),
            Err(BuiltinError::MalformedDimension)
        );

        // A zero-denominator exponent is not a rational power — malformed.
        let zero_den = [DimensionExponentCell {
            base_dimension: &time,
            exponent_numerator: "1",
            exponent_denominator: "0",
        }];
        assert_eq!(
            dim_from_exponent_cells(&zero_den),
            Err(BuiltinError::MalformedDimension)
        );

        // A non-integer exponent surface is malformed.
        let bad_exp = [DimensionExponentCell {
            base_dimension: &time,
            exponent_numerator: "\"1.5\"^^<http://www.w3.org/2001/XMLSchema#decimal>",
            exponent_denominator: "1",
        }];
        assert_eq!(
            dim_from_exponent_cells(&bad_exp),
            Err(BuiltinError::MalformedDimension)
        );
    }

    #[test]
    fn quantity_from_components_builds_and_round_trips() {
        // A math:Quantity of magnitude 5/2 with dimension length lowers and round-trips.
        let length = dim_of(&[(LEN, 1, 1)]);
        let num = emit_integer_surface(5);
        let den = emit_integer_surface(2);
        let value = quantity_from_components(&num, &den, length)
            .expect("integer magnitude decodes")
            .expect("well-formed magnitude");
        assert_eq!(value, Value::Quantity(rat(5, 2), length));
        let surface = emit_surface(&value);
        assert_eq!(parse_value_surface(&surface), Some(value));

        // A zero-denominator magnitude is a declared domain error.
        assert_eq!(
            quantity_from_components("5", "0", length),
            Some(Err(BuiltinError::ZeroDenominator))
        );
        // A non-integer magnitude component declines to None (malformed node).
        assert_eq!(
            quantity_from_components(
                "\"1.5\"^^<http://www.w3.org/2001/XMLSchema#decimal>",
                "1",
                length
            ),
            None
        );
    }

    #[test]
    fn lowered_cells_compose_consistently_with_the_dimension_algebra() {
        // The structured-cell lowering agrees with `Mul` on the base dimensions:
        // lengthDimension ⊗ timeDimension⁻¹ (via cells) == L * (T // T // ...) — here
        // demonstrated by composing L and T⁻¹ and comparing to the lowered velocity.
        let length_iri = base_iri("lengthDimension");
        let time_iri = base_iri("timeDimension");
        let lowered = dim_from_exponent_cells(&[
            DimensionExponentCell {
                base_dimension: &length_iri,
                exponent_numerator: "1",
                exponent_denominator: "1",
            },
            DimensionExponentCell {
                base_dimension: &time_iri,
                exponent_numerator: "-1",
                exponent_denominator: "1",
            },
        ])
        .expect("well-formed cells");
        // Compose L * (dimensionless / T) via the evaluator and compare.
        let length = dim_of(&[(LEN, 1, 1)]);
        let time = dim_of(&[(TIME, 1, 1)]);
        let dimensionless = ZERO_DIM;
        let lookup = env(&[
            ("L", &dim_surface(&length)),
            ("T", &dim_surface(&time)),
            ("ONE", &dim_surface(&dimensionless)),
        ]);
        // Tinv is 1 / T (dimensionless ⊖ T = T⁻¹).
        let BuiltinOutcome::Generate { value: tinv, .. } = eval(
            &is(var("X"), var("ONE"), ArithOp::ExactDiv, var("T")),
            &lookup,
        ) else {
            panic!("T⁻¹ generates");
        };
        let lookup2 = env(&[("L", &dim_surface(&length)), ("Tinv", &emit_surface(&tinv))]);
        let BuiltinOutcome::Generate {
            value: composed, ..
        } = eval(&is(var("X"), var("L"), ArithOp::Mul, var("Tinv")), &lookup2)
        else {
            panic!("L * T⁻¹ generates");
        };
        assert_eq!(composed, lowered);
    }
}
