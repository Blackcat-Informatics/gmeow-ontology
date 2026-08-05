// SPDX-FileCopyrightText: 2026 Blackcat Informatics Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Reusable exact-rational geometry over a metric Gram matrix.
//!
//! This crate owns the domain-neutral numeric core that any grounding layer can
//! reuse: an exact-rational number ([`Rational`], `i128`-backed, gcd-normalized,
//! hard-fail on overflow), a finite-dimensional inner-product space presented by a
//! symmetric positive-definite Gram matrix ([`InnerProductSpace`]) with its
//! `⟨x,y⟩ = xᵀGy` inner product / norm / distance / cosine / projection / LDLᵀ
//! positive-definiteness certificate, and the pure `math:`-vocabulary graph loaders
//! ([`load_gram`], [`load_vector`]) that read exact rational cells out of an RDF
//! graph. All arithmetic is exact rational — the ONLY approximation is the final
//! square root, emitted as a fixed-precision decimal ([`SQRT_DECIMALS`]) via an
//! integer floor-sqrt, never `f64::sqrt` — so the output strings are deterministic
//! and byte-identical across runs.
//!
//! This is the shared layer that domain grounders (affect intensity, and the
//! native `math:` conformance gates) compute THROUGH, rather than each re-deriving
//! its own exact-rational inner-product engine.

use std::cmp::Ordering;
use std::collections::HashMap;

use gmeow_errors::{Diag, Result};
use purrdf::gts::model::{Graph, RDF_LANG_STRING, Term, TermKind, XSD_STRING};

pub mod clifford;
pub mod dimension;
pub mod producers;

mod error;
use error::{
    ArithmeticOverflow, BadCosine, DecimalParse, DegenerateScale, EmptySpace, GraphRead,
    MissingProperty, NegativeSqrt, NoCells, NonSquareGram, NotPositiveDefinite, RationalDomain,
    ZeroVector,
};
// `IndexOutOfRange` is part of the public surface: a downstream reasoned-graph gate
// (`gmeow_logic::math_dimension`) distinguishes an out-of-range authored matrix index
// from a shape-caught structural fault to surface it as a typed `math:MalformedDimension`
// finding rather than silently skipping.
pub use error::{IndexOutOfRange, MATH_DIAG_CODES, register_all};

const MATH: &str = "https://blackcatinformatics.ca/math/";
const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";

/// Fixed number of fractional decimal digits emitted for every square-root
/// (norm/distance/cosine) output. This is a hard, documented contract: the sqrt
/// is computed by an exact integer floor-sqrt at `10^(2*(k+1))` scale, then
/// rounded half-up at the `(k+1)`-th digit down to `k` digits.
pub const SQRT_DECIMALS: u32 = 6;

/// Maximum supported basis dimension. The bases in use (valence–arousal = 2, PAD
/// = 3, and richer appraisal bases) are tiny; this deliberately generous cap
/// bounds every row/column/axis index parsed from a graph BEFORE it can size a
/// Gram-matrix allocation. A malformed index therefore becomes a hard-fail `Err`
/// rather than a lossy `usize` truncation or an OOM-scale allocation.
pub const MAX_BASIS_DIM: usize = 256;

/// Convert a parsed `i128` matrix/axis index into a bounded `usize`.
///
/// `usize::try_from` rejects negative indices, and the explicit bound rejects
/// any index at or above [`MAX_BASIS_DIM`], so the dimension derived from these
/// indices can never exceed the supported cap.
pub fn bounded_index(value: i128, what: &str) -> Result<usize> {
    let idx = usize::try_from(value).map_err(|_| {
        Diag::of_kind(IndexOutOfRange {
            detail: format!("{what} index out of range: {value}"),
        })
    })?;
    if idx >= MAX_BASIS_DIM {
        return Err(Diag::of_kind(IndexOutOfRange {
            detail: format!(
                "{what} index {idx} exceeds the maximum supported basis dimension {MAX_BASIS_DIM}"
            ),
        }));
    }
    Ok(idx)
}

fn math(local: &str) -> String {
    format!("{MATH}{local}")
}

// ---------------------------------------------------------------------------
// Exact rational arithmetic (i128, gcd-normalized, hard-fail on overflow).
// ---------------------------------------------------------------------------

fn gcd_i128(a: i128, b: i128) -> i128 {
    let mut a = a.abs();
    let mut b = b.abs();
    while b != 0 {
        let r = a % b;
        a = b;
        b = r;
    }
    if a == 0 { 1 } else { a }
}

/// An exact rational number backed by `i128`, always kept gcd-normalized with a
/// strictly positive denominator. Every arithmetic operation is checked and
/// **hard-fails** (returns `Err`) on overflow rather than wrapping.
///
/// The normalized `(numerator, denominator)` pair is the canonical representative
/// of the value, so the derived [`Hash`] is consistent with [`Eq`]: two rationals
/// that compare equal (e.g. `1/2` and `2/4`, both normalized to `1/2`) hash equal.
/// `Ord`/`PartialOrd` cross-multiply and **panic** on `i128` overflow (a loud,
/// deterministic hard fail, since `cmp` cannot return `Result`); overflow-safe
/// callers on hostile inputs must order via [`Rational::checked_sub`] and inspect
/// the sign instead of `cmp`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Rational {
    numerator: i128,
    denominator: i128,
}

impl Rational {
    /// Construct a normalized rational. Denominator zero, or an `i128::MIN`
    /// component (whose `abs` would overflow), is a hard fail.
    pub fn new(numerator: i128, denominator: i128) -> Result<Self> {
        if denominator == 0 {
            return Err(Diag::of_kind(RationalDomain {
                detail: "rational denominator must not be zero".to_string(),
            }));
        }
        if numerator == i128::MIN || denominator == i128::MIN {
            return Err(Diag::of_kind(RationalDomain {
                detail: "rational components must not be i128::MIN".to_string(),
            }));
        }
        let sign = if denominator < 0 { -1 } else { 1 };
        let g = gcd_i128(numerator, denominator);
        Ok(Self {
            numerator: sign * numerator / g,
            denominator: sign * denominator / g,
        })
    }

    /// The rational `value / 1`.
    pub fn from_i128(value: i128) -> Result<Self> {
        Self::new(value, 1)
    }

    /// The exact rational zero.
    pub fn zero() -> Self {
        Self {
            numerator: 0,
            denominator: 1,
        }
    }

    /// The exact rational one.
    pub fn one() -> Self {
        Self {
            numerator: 1,
            denominator: 1,
        }
    }

    /// Numerator (of the gcd-normalized form; carries the sign).
    pub fn numerator(self) -> i128 {
        self.numerator
    }

    /// Denominator (of the gcd-normalized form; always `> 0`).
    pub fn denominator(self) -> i128 {
        self.denominator
    }

    /// `true` iff the value is exactly zero.
    pub fn is_zero(self) -> bool {
        self.numerator == 0
    }

    /// `true` iff the value is `<= 0`.
    pub fn is_non_positive(self) -> bool {
        self.numerator <= 0
    }

    fn checked(op: &str, value: Option<i128>) -> Result<i128> {
        value.ok_or_else(|| {
            Diag::of_kind(ArithmeticOverflow {
                detail: format!("i128 overflow in rational {op}"),
            })
        })
    }

    /// Exact checked addition; hard-fails on overflow.
    pub fn checked_add(self, other: Self) -> Result<Self> {
        let left = Self::checked("add", self.numerator.checked_mul(other.denominator))?;
        let right = Self::checked("add", other.numerator.checked_mul(self.denominator))?;
        let num = Self::checked("add", left.checked_add(right))?;
        let den = Self::checked("add", self.denominator.checked_mul(other.denominator))?;
        Self::new(num, den)
    }

    /// Exact checked subtraction; hard-fails on overflow.
    pub fn checked_sub(self, other: Self) -> Result<Self> {
        let left = Self::checked("sub", self.numerator.checked_mul(other.denominator))?;
        let right = Self::checked("sub", other.numerator.checked_mul(self.denominator))?;
        let num = Self::checked("sub", left.checked_sub(right))?;
        let den = Self::checked("sub", self.denominator.checked_mul(other.denominator))?;
        Self::new(num, den)
    }

    /// Exact checked multiplication; hard-fails on overflow.
    pub fn checked_mul(self, other: Self) -> Result<Self> {
        let num = Self::checked("mul", self.numerator.checked_mul(other.numerator))?;
        let den = Self::checked("mul", self.denominator.checked_mul(other.denominator))?;
        Self::new(num, den)
    }

    /// Exact checked division; hard-fails on overflow or division by zero.
    pub fn checked_div(self, other: Self) -> Result<Self> {
        if other.is_zero() {
            return Err(Diag::of_kind(RationalDomain {
                detail: "rational division by zero".to_string(),
            }));
        }
        self.checked_mul(Self {
            numerator: other.denominator,
            denominator: other.numerator,
        })
    }

    /// Canonical `n/d` (or `n` when the denominator is one) printable ratio.
    pub fn ratio_string(self) -> String {
        if self.denominator == 1 {
            self.numerator.to_string()
        } else {
            format!("{}/{}", self.numerator, self.denominator)
        }
    }

    /// Parse an `xsd:decimal`/`xsd:integer` lexical form into an EXACT rational
    /// (decimals are exact rationals: `"0.7"` → `7/10`). No exponent form.
    pub fn parse_decimal(text: &str) -> Result<Self> {
        let text = text.trim();
        if text.is_empty() {
            return Err(Diag::of_kind(DecimalParse {
                detail: "empty decimal literal".to_string(),
            }));
        }
        let (sign, body) = match text.strip_prefix('-') {
            Some(rest) => (-1_i128, rest),
            None => (1_i128, text.strip_prefix('+').unwrap_or(text)),
        };
        let (int_part, frac_part) = match body.split_once('.') {
            Some((int_part, frac_part)) => (int_part, frac_part),
            None => (body, ""),
        };
        let int_part = if int_part.is_empty() { "0" } else { int_part };
        if !int_part.bytes().all(|b| b.is_ascii_digit())
            || !frac_part.bytes().all(|b| b.is_ascii_digit())
        {
            return Err(Diag::of_kind(DecimalParse {
                detail: format!("not a decimal literal: {text:?}"),
            }));
        }
        let digits = format!("{int_part}{frac_part}");
        let numerator: i128 = digits.parse().map_err(|_| {
            Diag::of_kind(DecimalParse {
                detail: format!("decimal literal out of i128 range: {text:?}"),
            })
        })?;
        let mut denominator: i128 = 1;
        for _ in 0..frac_part.len() {
            denominator = Self::checked("parse", denominator.checked_mul(10))?;
        }
        Self::new(sign * numerator, denominator)
    }
}

impl Ord for Rational {
    fn cmp(&self, other: &Self) -> Ordering {
        // Cross-multiply in i128; both denominators are positive, so the sign of
        // the comparison is unchanged. The products are checked and a wrap is a
        // loud, deterministic hard fail — never a silent overflow (matches the
        // crate's checked-arithmetic invariant; `cmp` cannot return `Result`).
        let left = self
            .numerator
            .checked_mul(other.denominator)
            .expect("Rational::cmp: cross-multiplication overflow");
        let right = other
            .numerator
            .checked_mul(self.denominator)
            .expect("Rational::cmp: cross-multiplication overflow");
        left.cmp(&right)
    }
}

impl PartialOrd for Rational {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

// ---------------------------------------------------------------------------
// Deterministic integer floor-sqrt and fixed-precision decimal formatting.
// ---------------------------------------------------------------------------

/// Floor of the integer square root of `value` (`u128`), by digit-by-digit
/// bit refinement. Exact and deterministic.
fn isqrt_u128(value: u128) -> u128 {
    if value < 2 {
        return value;
    }
    let mut x = 1_u128 << ((128 - value.leading_zeros()).div_ceil(2));
    loop {
        let next = (x + value / x) / 2;
        if next >= x {
            return x;
        }
        x = next;
    }
}

fn pow10_i128(exp: u32) -> Result<i128> {
    let mut acc: i128 = 1;
    for _ in 0..exp {
        acc = acc.checked_mul(10).ok_or_else(|| {
            Diag::of_kind(ArithmeticOverflow {
                detail: "i128 overflow computing power of ten".to_string(),
            })
        })?;
    }
    Ok(acc)
}

/// Deterministic `√q` as a fixed-precision decimal string with [`SQRT_DECIMALS`]
/// fractional digits (round-half-up at the seventh digit). `q` must be `>= 0`.
pub fn sqrt_rational_decimal(q: Rational) -> Result<String> {
    if q.numerator < 0 {
        return Err(Diag::of_kind(NegativeSqrt {}));
    }
    let k = SQRT_DECIMALS;
    // scaled = floor(q * 10^(2*(k+1))); isqrt(scaled) = floor(√q * 10^(k+1)).
    let scale = pow10_i128(2 * (k + 1))?;
    let numerator = q.numerator.checked_mul(scale).ok_or_else(|| {
        Diag::of_kind(ArithmeticOverflow {
            detail: "i128 overflow scaling quadratic form for sqrt".to_string(),
        })
    })?;
    let scaled = numerator / q.denominator; // floor; both operands >= 0
    let root = isqrt_u128(scaled as u128); // floor(√q * 10^(k+1))
    let rounded = (root + 5) / 10; // round-half-up to k digits
    let unit = 10_u128.pow(k);
    let int_part = rounded / unit;
    let frac_part = rounded % unit;
    Ok(format!(
        "{int_part}.{frac_part:0width$}",
        width = k as usize
    ))
}

/// A rational as a trimmed decimal with up to [`SQRT_DECIMALS`] fractional
/// digits (round-half-up), trailing zeros stripped — the shared normalization
/// formatter.
fn format_decimal(value: Rational) -> Result<String> {
    let k = SQRT_DECIMALS;
    let sign = if value.numerator < 0 { "-" } else { "" };
    let num = value.numerator.unsigned_abs();
    let den = value.denominator.unsigned_abs();
    // Integer-part-first long division. `rem` stays strictly below `den`
    // throughout, so the value's magnitude never scales the numerator up front —
    // a small-VALUED rational with an enormous numerator/denominator formats
    // without the spurious overflow of a `num * 10^k` prescaling.
    let mut int_part = num / den;
    let mut rem = num % den; // rem < den, never overflows.
    // Produce k fractional digits plus one guard digit for round-half-up.
    let mut digits: Vec<u8> = Vec::with_capacity((k + 1) as usize);
    for _ in 0..=k {
        rem = rem.checked_mul(10).ok_or_else(|| {
            Diag::of_kind(ArithmeticOverflow {
                detail: "u128 overflow formatting decimal".to_string(),
            })
        })?;
        digits.push((rem / den) as u8);
        rem %= den;
    }
    let round_digit = digits.pop().expect("k + 1 >= 1 digits produced");
    if round_digit >= 5 {
        // Increment the k-digit fractional integer, carrying into int_part.
        let mut carry = 1u8;
        for d in digits.iter_mut().rev() {
            let sum = *d + carry;
            *d = sum % 10;
            carry = sum / 10;
            if carry == 0 {
                break;
            }
        }
        if carry != 0 {
            // Carried past 10^k: the fractional part becomes zero and one unit
            // rolls into the integer part.
            int_part += 1;
            digits.iter_mut().for_each(|d| *d = 0);
        }
    }
    // Strip trailing-zero fractional digits.
    while digits.last() == Some(&0) {
        digits.pop();
    }
    if digits.is_empty() {
        return Ok(format!("{sign}{int_part}"));
    }
    let mut frac = String::with_capacity(digits.len());
    for d in &digits {
        frac.push((b'0' + d) as char);
    }
    Ok(format!("{sign}{int_part}.{frac}"))
}

// ---------------------------------------------------------------------------
// Reusable exact-rational inner-product-space over a Gram matrix G.
// ---------------------------------------------------------------------------

/// A finite-dimensional real inner-product space presented by a symmetric,
/// positive-definite Gram matrix `G`. The inner product is `⟨x,y⟩ = xᵀGy`; all
/// operations are exact rational except the final square root at the output edge.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InnerProductSpace {
    gram: Vec<Vec<Rational>>,
}

impl InnerProductSpace {
    /// Wrap a square Gram matrix. Non-square is a hard fail. (Symmetry and
    /// positive-definiteness are the caller's contract; positive-definiteness is
    /// certified on demand by [`InnerProductSpace::ldlt_pivots`].)
    pub fn new(gram: Vec<Vec<Rational>>) -> Result<Self> {
        let n = gram.len();
        if gram.iter().any(|row| row.len() != n) {
            return Err(Diag::of_kind(NonSquareGram {}));
        }
        Ok(Self { gram })
    }

    /// The dimension `n` of the space.
    pub fn dim(&self) -> usize {
        self.gram.len()
    }

    fn padded(&self, x: &[Rational]) -> Vec<Rational> {
        let mut v = vec![Rational::zero(); self.dim()];
        for (i, value) in x.iter().take(self.dim()).enumerate() {
            v[i] = *value;
        }
        v
    }

    /// `G·x`.
    fn matvec(&self, x: &[Rational]) -> Result<Vec<Rational>> {
        let x = self.padded(x);
        let mut out = vec![Rational::zero(); self.dim()];
        for (i, row) in self.gram.iter().enumerate() {
            let mut acc = Rational::zero();
            for (j, g) in row.iter().enumerate() {
                acc = acc.checked_add(g.checked_mul(x[j])?)?;
            }
            out[i] = acc;
        }
        Ok(out)
    }

    /// The exact inner product `⟨x,y⟩ = xᵀGy`.
    pub fn inner(&self, x: &[Rational], y: &[Rational]) -> Result<Rational> {
        let gy = self.matvec(y)?;
        let x = self.padded(x);
        let mut acc = Rational::zero();
        for (xi, gyi) in x.iter().zip(gy.iter()) {
            acc = acc.checked_add(xi.checked_mul(*gyi)?)?;
        }
        Ok(acc)
    }

    /// The exact quadratic form `Q = xᵀGx = ⟨x,x⟩`.
    pub fn quadratic_form(&self, x: &[Rational]) -> Result<Rational> {
        self.inner(x, x)
    }

    /// The norm `‖x‖_G = √(xᵀGx)`, as a fixed-precision decimal string.
    pub fn norm(&self, x: &[Rational]) -> Result<String> {
        sqrt_rational_decimal(self.quadratic_form(x)?)
    }

    /// The distance `‖x − y‖_G`, as a fixed-precision decimal string.
    pub fn distance(&self, x: &[Rational], y: &[Rational]) -> Result<String> {
        let x = self.padded(x);
        let y = self.padded(y);
        let diff = x
            .iter()
            .zip(y.iter())
            .map(|(xi, yi)| xi.checked_sub(*yi))
            .collect::<Result<Vec<_>>>()?;
        self.norm(&diff)
    }

    /// The cosine of the angle `⟨x,y⟩ / (‖x‖·‖y‖)`, as a signed fixed-precision
    /// decimal string. A zero vector makes the angle undefined → hard fail.
    pub fn cosine(&self, x: &[Rational], y: &[Rational]) -> Result<String> {
        let qx = self.quadratic_form(x)?;
        let qy = self.quadratic_form(y)?;
        if qx.is_zero() || qy.is_zero() {
            return Err(Diag::of_kind(ZeroVector {
                detail: "cosine is undefined for a zero vector".to_string(),
            }));
        }
        let inner = self.inner(x, y)?;
        // cos = inner / √(qx·qy) = sign(inner) · √(inner² / (qx·qy)).
        let magnitude =
            sqrt_rational_decimal(inner.checked_mul(inner)?.checked_div(qx.checked_mul(qy)?)?)?;
        if inner.numerator < 0 {
            Ok(format!("-{magnitude}"))
        } else {
            Ok(magnitude)
        }
    }

    /// The angle between `x` and `y` in radians. This is an output-edge float
    /// approximation (like the sqrt): `arccos` has no exact rational form.
    pub fn angle(&self, x: &[Rational], y: &[Rational]) -> Result<f64> {
        let cosine: f64 = self
            .cosine(x, y)?
            .parse()
            .map_err(|_| Diag::of_kind(BadCosine {}))?;
        Ok(cosine.clamp(-1.0, 1.0).acos())
    }

    /// `true` iff `x ⟂ y` under `G` (`⟨x,y⟩` exactly zero).
    pub fn is_orthogonal(&self, x: &[Rational], y: &[Rational]) -> Result<bool> {
        Ok(self.inner(x, y)?.is_zero())
    }

    /// The exact metric projection of `x` onto `onto`:
    /// `(⟨x,onto⟩ / ⟨onto,onto⟩) · onto`. A zero `onto` is a hard fail.
    pub fn project(&self, x: &[Rational], onto: &[Rational]) -> Result<Vec<Rational>> {
        let denom = self.quadratic_form(onto)?;
        if denom.is_zero() {
            return Err(Diag::of_kind(ZeroVector {
                detail: "cannot project onto a zero vector".to_string(),
            }));
        }
        let scale = self.inner(x, onto)?.checked_div(denom)?;
        self.padded(onto)
            .iter()
            .map(|c| c.checked_mul(scale))
            .collect()
    }

    /// The exact-rational LDLᵀ (Cholesky-without-√) pivots `Dᵢ` of `G`. By
    /// Sylvester's criterion `G` is positive-definite iff every pivot is `> 0`.
    /// A non-positive pivot is a hard fail naming the offending index — the
    /// positive-definiteness certificate.
    pub fn ldlt_pivots(&self) -> Result<Vec<Rational>> {
        let n = self.dim();
        let mut l = vec![vec![Rational::zero(); n]; n];
        let mut d = vec![Rational::zero(); n];
        for j in 0..n {
            let mut dj = self.gram[j][j];
            for k in 0..j {
                dj = dj.checked_sub(l[j][k].checked_mul(l[j][k])?.checked_mul(d[k])?)?;
            }
            if dj.is_non_positive() {
                return Err(Diag::of_kind(NotPositiveDefinite {
                    detail: format!(
                        "Gram matrix is not positive-definite: pivot {j} = {} is not > 0",
                        dj.ratio_string()
                    ),
                }));
            }
            d[j] = dj;
            l[j][j] = Rational::one();
            for i in (j + 1)..n {
                let mut s = self.gram[i][j];
                for k in 0..j {
                    s = s.checked_sub(l[i][k].checked_mul(l[j][k])?.checked_mul(d[k])?)?;
                }
                l[i][j] = s.checked_div(dj)?;
            }
        }
        Ok(d)
    }

    /// The metric-aware dominant axis: the index `i` maximizing its `G`-weighted
    /// contribution `xᵢ·(Gx)ᵢ` (NOT the raw largest component). Ties resolve to
    /// the lowest index. Requires a non-empty space.
    pub fn dominant_axis(&self, x: &[Rational]) -> Result<usize> {
        if self.dim() == 0 {
            return Err(Diag::of_kind(EmptySpace {}));
        }
        let x = self.padded(x);
        let gx = self.matvec(&x)?;
        let mut best_axis = 0_usize;
        let mut best = x[0].checked_mul(gx[0])?;
        for i in 1..self.dim() {
            let contribution = x[i].checked_mul(gx[i])?;
            if contribution > best {
                best = contribution;
                best_axis = i;
            }
        }
        Ok(best_axis)
    }
}

// ---------------------------------------------------------------------------
// Unit-clamp normalization shared across grounding layers.
// ---------------------------------------------------------------------------

/// Unit-clamp normalization of a cell magnitude `value` on `[range_min,
/// range_max]` into `[0,1]`: `(value − range_min)/(range_max − range_min)`, exact
/// rational, then the trimmed-decimal formatter. E.g. on the PAD unit scale
/// `[-1,1]`: valence `0.7` → `"0.85"`, arousal `0.4` → `"0.7"`.
pub fn normalize_to_unit(
    value: &Rational,
    range_min: &Rational,
    range_max: &Rational,
) -> Result<String> {
    let span = range_max.checked_sub(*range_min)?;
    if span.is_zero() {
        return Err(Diag::of_kind(DegenerateScale {}));
    }
    format_decimal(value.checked_sub(*range_min)?.checked_div(span)?)
}

// ---------------------------------------------------------------------------
// Graph indexing (a hand-rolled TripleIndex over a purrdf default-graph).
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum Node {
    Iri(String),
    Bnode(String),
    /// A literal, carrying its FULL identity: lexical form, datatype (an RDF
    /// literal always carries one — the RDF §7.1 default, `rdf:langString` with a
    /// language tag else `xsd:string`, is expanded here when a term omits it
    /// explicitly, never left ambiguous), and its optional language tag. Dropping
    /// either would silently conflate e.g. `"42"^^xsd:integer`, `"42"^^xsd:string`,
    /// and `"42"@en` — three distinct literals.
    Literal {
        lexical: String,
        datatype: String,
        language: Option<String>,
    },
}

/// A subject → predicate → objects index over the default graph of a purrdf
/// [`Graph`], the read substrate the exact-rational loaders walk.
#[derive(Debug, Default)]
pub struct TripleIndex {
    by_subject: HashMap<String, HashMap<String, Vec<Node>>>,
}

fn node_id(term: &Term) -> Option<String> {
    match term.kind {
        TermKind::Iri => term.value.clone(),
        TermKind::Bnode => term.value.as_ref().map(|value| format!("_:{value}")),
        TermKind::Literal | TermKind::Triple => None,
    }
}

/// The datatype IRI of a literal `Term`: the explicit `math:` datatype term if
/// present, else the RDF §7.1 implicit default (`rdf:langString` when a language
/// tag is carried, else `xsd:string`) — a literal's datatype is NEVER absent,
/// mirroring `purrdf::TermRef::Literal`'s always-resolved `datatype` field.
fn literal_datatype(graph: &Graph, term: &Term) -> String {
    if let Some(dt_id) = term.datatype
        && let Some(dt_term) = graph.terms.get(dt_id)
        && let Some(value) = &dt_term.value
    {
        return value.clone();
    }
    if term.lang.is_some() {
        RDF_LANG_STRING.to_owned()
    } else {
        XSD_STRING.to_owned()
    }
}

fn object_node(term: &Term, graph: &Graph) -> Option<Node> {
    match term.kind {
        TermKind::Iri => term.value.clone().map(Node::Iri),
        TermKind::Bnode => term
            .value
            .as_ref()
            .map(|value| Node::Bnode(format!("_:{value}"))),
        TermKind::Literal => Some(Node::Literal {
            lexical: term.value.clone().unwrap_or_default(),
            datatype: literal_datatype(graph, term),
            language: term.lang.clone(),
        }),
        TermKind::Triple => None,
    }
}

/// Build a [`TripleIndex`] over the default graph of `graph`.
pub fn index_graph(graph: &Graph) -> TripleIndex {
    let mut index = TripleIndex::default();
    for quad in graph.quad_terms().filter(|quad| quad.graph_name.is_none()) {
        let (Some(subject), Some(predicate), Some(object)) = (
            node_id(quad.subject),
            node_id(quad.predicate),
            object_node(quad.object, graph),
        ) else {
            continue;
        };
        index
            .by_subject
            .entry(subject)
            .or_default()
            .entry(predicate)
            .or_default()
            .push(object);
    }
    index
}

/// Build a [`TripleIndex`] over a purrdf [`RdfDataset`](purrdf::RdfDataset), the
/// read substrate the exact-rational loaders walk when the source is an in-memory
/// dataset (e.g. the native reasoned graph) rather than a `.gts` bundle. Every
/// graph is read (`GraphMatch::Any`), so an invariant computed off the index holds
/// bundle-wide exactly as the whole-dataset validate sweep did. Literal objects are
/// indexed by their lexical form; IRI and blank-node terms by their value
/// (blank-node labels scope-qualified), and quoted-triple terms are skipped — the
/// same node projection [`index_graph`] applies.
pub fn index_dataset(dataset: &purrdf::RdfDataset) -> TripleIndex {
    use purrdf::{DatasetView, GraphMatch, TermRef};
    let mut index = TripleIndex::default();
    for quad in dataset.quads_for_pattern(None, None, None, GraphMatch::Any) {
        let subject = match dataset.resolve(quad.s) {
            TermRef::Iri(iri) => iri.to_owned(),
            TermRef::Blank { label, scope } => {
                format!("_:{}", scope.qualify_label(label))
            }
            TermRef::Literal { .. } | TermRef::Triple { .. } => continue,
        };
        let TermRef::Iri(predicate) = dataset.resolve(quad.p) else {
            continue;
        };
        let object = match dataset.resolve(quad.o) {
            TermRef::Iri(iri) => Node::Iri(iri.to_owned()),
            TermRef::Blank { label, scope } => {
                Node::Bnode(format!("_:{}", scope.qualify_label(label)))
            }
            TermRef::Literal {
                lexical,
                datatype,
                language,
                ..
            } => {
                let TermRef::Iri(datatype) = dataset.resolve(datatype) else {
                    unreachable!("a literal's datatype term resolves to an IRI by RDF construction")
                };
                Node::Literal {
                    lexical: lexical.to_owned(),
                    datatype: datatype.to_owned(),
                    language: language.map(str::to_owned),
                }
            }
            TermRef::Triple { .. } => continue,
        };
        // DISTINCT (s, p, o), never once per graph carrying it. `quads_for_pattern` walks
        // every graph, and a slice's triples reach the reasoning EDB in more than one, so a
        // plain push counted each authored triple as many times as graphs held it. Cardinality
        // obligations read this index — an RDF triple is identity-bearing regardless of which
        // graphs assert it, so "exactly one math:operator" must count operators, not
        // assertions of one. Left unguarded, `gmeow validate --deep` reported this slice's own
        // conforming examples as carrying two operators where they author one.
        let objects = index
            .by_subject
            .entry(subject)
            .or_default()
            .entry(predicate.to_owned())
            .or_default();
        if !objects.contains(&object) {
            objects.push(object);
        }
    }
    index
}

/// Parse a Turtle document into a [`TripleIndex`] over its default graph.
///
/// The Turtle is normalized through the GTS snapshot codec (parse → snapshot →
/// read) so the resulting [`Graph`] is exactly the one the loaders walk when they
/// read a shipped `.gts` bundle — the conformance consumers exercise the same
/// read substrate as production, not a divergent parser.
pub fn index_turtle(turtle: &[u8]) -> Result<TripleIndex> {
    use purrdf::gts_compose::SnapshotBuilder;
    use purrdf::{NativeRdfFormat, parse_dataset};

    let dataset =
        parse_dataset(turtle, NativeRdfFormat::Turtle.media_type(), None).map_err(|err| {
            Diag::of_kind(GraphRead {
                detail: format!("cannot parse Turtle: {err}"),
            })
        })?;
    let mut builder = SnapshotBuilder::default();
    builder.add_dataset(&dataset).map_err(|err| {
        Diag::of_kind(GraphRead {
            detail: format!("cannot snapshot dataset: {err}"),
        })
    })?;
    let gts = gmeow_gts_profile::emit_gmeow_gts(&builder, Vec::new(), Vec::new(), None, None, None)
        .map_err(|err| {
            Diag::of_kind(GraphRead {
                detail: format!("cannot emit GTS: {err}"),
            })
        })?;
    let graph = purrdf::gts::reader::read(&gts, false, None);
    Ok(index_graph(&graph))
}

fn objects<'a>(index: &'a TripleIndex, subject: &str, predicate: &str) -> &'a [Node] {
    index
        .by_subject
        .get(subject)
        .and_then(|preds| preds.get(predicate))
        .map(Vec::as_slice)
        .unwrap_or(&[])
}

/// The first IRI/bnode object of `(subject, predicate, ?)`, if any.
pub fn first_iri(index: &TripleIndex, subject: &str, predicate: &str) -> Option<String> {
    objects(index, subject, predicate)
        .iter()
        .find_map(|node| match node {
            Node::Iri(value) | Node::Bnode(value) => Some(value.clone()),
            Node::Literal { .. } => None,
        })
}

/// All IRI/bnode objects of `(subject, predicate, ?)`, in index order.
pub fn all_iris(index: &TripleIndex, subject: &str, predicate: &str) -> Vec<String> {
    objects(index, subject, predicate)
        .iter()
        .filter_map(|node| match node {
            Node::Iri(value) | Node::Bnode(value) => Some(value.clone()),
            Node::Literal { .. } => None,
        })
        .collect()
}

/// The first literal object of `(subject, predicate, ?)`, if any — lexical form
/// only. Datatype/language fidelity is dropped here deliberately for callers that
/// never need it; a caller that needs full literal identity uses
/// [`first_literal_typed`] instead.
pub fn first_literal(index: &TripleIndex, subject: &str, predicate: &str) -> Option<String> {
    first_literal_typed(index, subject, predicate).map(|(lexical, _, _)| lexical.to_owned())
}

/// The first literal object of `(subject, predicate, ?)`, if any, as
/// `(lexical, datatype, language)` — full fidelity: the datatype (an RDF literal
/// always carries one) and the optional language tag are never discarded.
pub fn first_literal_typed<'a>(
    index: &'a TripleIndex,
    subject: &str,
    predicate: &str,
) -> Option<(&'a str, &'a str, Option<&'a str>)> {
    objects(index, subject, predicate)
        .iter()
        .find_map(|node| match node {
            Node::Literal {
                lexical,
                datatype,
                language,
            } => Some((lexical.as_str(), datatype.as_str(), language.as_deref())),
            Node::Iri(_) | Node::Bnode(_) => None,
        })
}

/// All literal objects of `(subject, predicate, ?)`, in index order, as
/// `(lexical, datatype, language)` — full fidelity (see [`first_literal_typed`]).
pub fn all_literals_typed<'a>(
    index: &'a TripleIndex,
    subject: &str,
    predicate: &str,
) -> Vec<(&'a str, &'a str, Option<&'a str>)> {
    objects(index, subject, predicate)
        .iter()
        .filter_map(|node| match node {
            Node::Literal {
                lexical,
                datatype,
                language,
            } => Some((lexical.as_str(), datatype.as_str(), language.as_deref())),
            Node::Iri(_) | Node::Bnode(_) => None,
        })
        .collect()
}

/// The first literal object parsed as an `i128`, if any.
pub fn first_i128(index: &TripleIndex, subject: &str, predicate: &str) -> Option<i128> {
    first_literal(index, subject, predicate)?
        .trim()
        .parse()
        .ok()
}

/// `true` iff `subject` carries `rdf:type` `class`.
pub fn has_type(index: &TripleIndex, subject: &str, class: &str) -> bool {
    objects(index, subject, RDF_TYPE)
        .iter()
        .any(|node| matches!(node, Node::Iri(value) if value == class))
}

/// The IRIs of every subject in the index (unsorted).
pub fn subjects(index: &TripleIndex) -> impl Iterator<Item = &String> {
    index.by_subject.keys()
}

// ---------------------------------------------------------------------------
// Pure math: exact-rational graph loaders.
// ---------------------------------------------------------------------------

fn rational_value(index: &TripleIndex, value_iri: &str) -> Result<Rational> {
    let num = first_i128(index, value_iri, &math("numerator")).ok_or_else(|| {
        Diag::of_kind(MissingProperty {
            detail: format!("rational value {value_iri} missing math:numerator"),
        })
    })?;
    let den = first_i128(index, value_iri, &math("denominator")).ok_or_else(|| {
        Diag::of_kind(MissingProperty {
            detail: format!("rational value {value_iri} missing math:denominator"),
        })
    })?;
    Rational::new(num, den)
}

/// Read the exact-rational cells of a `math:GramMatrix` from the graph: every
/// `math:hasEntry` is a `math:MatrixEntry` with `math:atRow`/`math:atColumn`
/// indices and a `math:entryValue` pointing at a `math:RationalValue`
/// (`math:numerator`/`math:denominator`). Returns `(row, col, value)` cells; the
/// caller fills the (declared symmetric) dense matrix. Indices are bounded by
/// [`bounded_index`], so a malformed index hard-fails before sizing a matrix.
pub fn load_gram(index: &TripleIndex, gram_iri: &str) -> Result<Vec<(usize, usize, Rational)>> {
    let entries = all_iris(index, gram_iri, &math("hasEntry"));
    if entries.is_empty() {
        return Err(Diag::of_kind(NoCells {
            detail: format!("Gram matrix {gram_iri} declares no math:hasEntry cells"),
        }));
    }
    let mut cells = Vec::new();
    for entry in entries {
        let row = first_i128(index, &entry, &math("atRow")).ok_or_else(|| {
            Diag::of_kind(MissingProperty {
                detail: format!("matrix entry {entry} missing math:atRow"),
            })
        })?;
        let col = first_i128(index, &entry, &math("atColumn")).ok_or_else(|| {
            Diag::of_kind(MissingProperty {
                detail: format!("matrix entry {entry} missing math:atColumn"),
            })
        })?;
        let row = bounded_index(row, "matrix row")?;
        let col = bounded_index(col, "matrix column")?;
        let value_iri = first_iri(index, &entry, &math("entryValue")).ok_or_else(|| {
            Diag::of_kind(MissingProperty {
                detail: format!("matrix entry {entry} missing math:entryValue"),
            })
        })?;
        cells.push((row, col, rational_value(index, &value_iri)?));
    }
    Ok(cells)
}

/// Read the exact-rational coordinates of a `math:Vector` from the graph: every
/// `math:hasComponent` is a `math:VectorComponent` with a `math:atIndex` and a
/// `math:componentValue` pointing at a `math:RationalValue`
/// (`math:numerator`/`math:denominator`). Returns a dense, zero-completed
/// coordinate vector sized to the maximum declared index + 1. Indices are bounded
/// by [`bounded_index`], so a malformed index hard-fails before sizing a vector.
pub fn load_vector(index: &TripleIndex, vector_iri: &str) -> Result<Vec<Rational>> {
    let components = all_iris(index, vector_iri, &math("hasComponent"));
    if components.is_empty() {
        return Err(Diag::of_kind(NoCells {
            detail: format!("vector {vector_iri} declares no math:hasComponent cells"),
        }));
    }
    let mut cells: Vec<(usize, Rational)> = Vec::new();
    for component in components {
        let idx = first_i128(index, &component, &math("atIndex")).ok_or_else(|| {
            Diag::of_kind(MissingProperty {
                detail: format!("vector component {component} missing math:atIndex"),
            })
        })?;
        let idx = bounded_index(idx, "vector index")?;
        let value_iri = first_iri(index, &component, &math("componentValue")).ok_or_else(|| {
            Diag::of_kind(MissingProperty {
                detail: format!("vector component {component} missing math:componentValue"),
            })
        })?;
        cells.push((idx, rational_value(index, &value_iri)?));
    }
    let dim = cells
        .iter()
        .map(|(i, _)| *i)
        .max()
        .map(|m| m + 1)
        .ok_or_else(|| {
            Diag::of_kind(NoCells {
                detail: format!("vector {vector_iri} has no components"),
            })
        })?;
    // Every index was bounded below MAX_BASIS_DIM, so the derived dimension is
    // bounded too; assert it before it sizes the vector.
    debug_assert!(dim <= MAX_BASIS_DIM, "derived dimension {dim} exceeds cap");
    let mut vector = vec![Rational::zero(); dim];
    for (idx, value) in cells {
        vector[idx] = value;
    }
    Ok(vector)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cmp::Ordering;

    fn r(num: i128, den: i128) -> Rational {
        Rational::new(num, den).expect("rational")
    }

    /// The canonical correlated metric G = [[1, 1/4], [1/4, 1]].
    fn correlated_gram() -> InnerProductSpace {
        InnerProductSpace::new(vec![vec![r(1, 1), r(1, 4)], vec![r(1, 4), r(1, 1)]]).expect("space")
    }

    // Hash is consistent with Eq: equal-valued rationals (normalized to the same
    // canonical pair) hash equal, so Rational is a sound HashMap/HashSet key.
    #[test]
    fn equal_rationals_hash_equal() {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        fn hash_of(value: Rational) -> u64 {
            let mut hasher = DefaultHasher::new();
            value.hash(&mut hasher);
            hasher.finish()
        }

        // 1/2 and 2/4 normalize to the same (1, 2); Eq and Hash must agree.
        let half = r(1, 2);
        let also_half = r(2, 4);
        assert_eq!(half, also_half);
        assert_eq!(hash_of(half), hash_of(also_half));

        // A negative denominator normalizes to a positive one; still hashes equal.
        let neg = r(1, -3);
        let pos = r(-1, 3);
        assert_eq!(neg, pos);
        assert_eq!(hash_of(neg), hash_of(pos));

        // Distinct values (overwhelmingly) hash apart — a weak inequality sanity check.
        assert_ne!(hash_of(r(1, 2)), hash_of(r(1, 3)));
    }

    #[test]
    fn parse_decimal_is_exact() {
        assert_eq!(Rational::parse_decimal("0.7").unwrap(), r(7, 10));
        assert_eq!(Rational::parse_decimal("-1.0").unwrap(), r(-1, 1));
        assert_eq!(Rational::parse_decimal("0.5").unwrap(), r(1, 2));
        assert_eq!(Rational::parse_decimal("2").unwrap(), r(2, 1));
    }

    // √(xᵀGx) over the NON-orthogonal G differs from raw L².
    #[test]
    fn metric_norm_distinct_from_raw_l2() {
        let space = correlated_gram();
        let x = [r(7, 10), r(2, 5)]; // valence 0.7, arousal 0.4
        // Q = xᵀGx = 0.7·(0.7 + 0.25·0.4) + 0.4·(0.25·0.7 + 0.4) = 79/100.
        let q = space.quadratic_form(&x).unwrap();
        assert_eq!(q, r(79, 100));
        assert_eq!(q.ratio_string(), "79/100");
        let intensity = space.norm(&x).unwrap();
        assert_eq!(intensity, "0.888819");
        // Raw L² over the SAME vector is √(0.49 + 0.16) = √0.65 ≈ 0.806226 — distinct.
        let raw_l2 = sqrt_rational_decimal(r(65, 100)).unwrap();
        assert_eq!(raw_l2, "0.806226");
        assert_ne!(intensity, raw_l2);
    }

    // LDLᵀ certifies PD (pivots 1, 15/16); an indefinite G names its pivot.
    #[test]
    fn ldlt_positive_definite_certificate() {
        let pivots = correlated_gram().ldlt_pivots().unwrap();
        assert_eq!(pivots, vec![r(1, 1), r(15, 16)]);

        let indefinite =
            InnerProductSpace::new(vec![vec![r(1, 1), r(2, 1)], vec![r(2, 1), r(1, 1)]]).unwrap();
        let err = indefinite.ldlt_pivots().unwrap_err();
        // Pivot 0 = 1 (> 0), pivot 1 = 1 − 4 = −3 (not > 0).
        assert!(err.message().contains("pivot 1"), "{err}");
        assert!(err.message().contains("-3"), "{err}");
    }

    // Metric-aware dominant axis differs from the raw-max component.
    #[test]
    fn dominant_axis_is_metric_aware_not_raw_max() {
        let space =
            InnerProductSpace::new(vec![vec![r(2, 1), r(0, 1)], vec![r(0, 1), r(1, 1)]]).unwrap();
        let x = [r(1, 2), r(3, 5)]; // valence 0.5, arousal 0.6
        // G-weighted: axis0 = 0.5·(2·0.5) = 0.5 > axis1 = 0.6·(1·0.6) = 0.36.
        assert_eq!(space.dominant_axis(&x).unwrap(), 0);
        // Raw-max component is arousal (axis 1): 0.6 > 0.5. Explicitly different.
        let raw_max = if x[1] > x[0] { 1 } else { 0 };
        assert_eq!(raw_max, 1);
        assert_ne!(space.dominant_axis(&x).unwrap(), raw_max);
    }

    // Distance and cosine match hand-computed exact values.
    #[test]
    fn distance_and_cosine_hand_checked() {
        // Identity metric so ⟨·,·⟩ is ordinary dot product.
        let space =
            InnerProductSpace::new(vec![vec![r(1, 1), r(0, 1)], vec![r(0, 1), r(1, 1)]]).unwrap();
        let x = [r(3, 1), r(4, 1)];
        let y = [r(4, 1), r(3, 1)];
        // x − y = (−1, 1); ‖·‖ = √2 = 1.414214 (rounded at 7th digit).
        assert_eq!(space.distance(&x, &y).unwrap(), "1.414214");
        // ⟨x,y⟩ = 24; ‖x‖‖y‖ = 25; cos = 24/25 = 0.96.
        assert_eq!(space.cosine(&x, &y).unwrap(), "0.960000");
        // Orthogonality and projection sanity.
        assert!(!space.is_orthogonal(&x, &y).unwrap());
        assert!(
            space
                .is_orthogonal(&[r(1, 1), r(0, 1)], &[r(0, 1), r(1, 1)])
                .unwrap()
        );
        // Project (3,4) onto (1,0) → (3,0).
        assert_eq!(
            space.project(&x, &[r(1, 1), r(0, 1)]).unwrap(),
            vec![r(3, 1), r(0, 1)]
        );
    }

    // Determinism, overflow hard-fail, and undefined-input hard fails.
    #[test]
    fn determinism_and_hard_fails() {
        let space = correlated_gram();
        let x = [r(7, 10), r(2, 5)];
        let first = space.norm(&x).unwrap();
        let second = space.norm(&x).unwrap();
        assert_eq!(first, second); // byte-identical, run twice

        // Overflow: (i128::MAX/2) · 4 must hard-fail, never wrap.
        let big = r(i128::MAX / 2, 1);
        assert!(
            big.checked_mul(r(4, 1))
                .unwrap_err()
                .message()
                .contains("overflow")
        );

        // Zero-vector cosine is undefined → Err.
        let zero = [r(0, 1), r(0, 1)];
        assert!(
            space
                .cosine(&x, &zero)
                .unwrap_err()
                .message()
                .contains("zero vector")
        );
    }

    // The Ord cross-multiply stays exact for the correlated-metric dominant-axis
    // case (valence 0.7 / arousal 0.4 over G = [[1,1/4],[1/4,1]]).
    #[test]
    fn dominant_axis_ord_correct_for_correlated_metric() {
        let space = correlated_gram();
        let x = [r(7, 10), r(2, 5)];
        assert_eq!(space.dominant_axis(&x).unwrap(), 0);
        // Direct Ord check of the two exact G-weighted contributions.
        assert!(r(56, 100) > r(23, 100));
        assert_eq!(r(56, 100).cmp(&r(23, 100)), Ordering::Greater);
        assert_eq!(r(23, 100).cmp(&r(56, 100)), Ordering::Less);
        assert_eq!(r(56, 100).cmp(&r(56, 100)), Ordering::Equal);
    }

    // Overflow in the Ord cross-multiply is a loud, deterministic hard fail
    // (checked_mul + expect), never a silent i128 wrap.
    #[test]
    #[should_panic(expected = "cross-multiplication overflow")]
    fn cmp_overflow_hard_fails() {
        let a = r(i128::MAX / 2, 1);
        let b = r(1, i128::MAX / 2 + 2);
        let _ = a.cmp(&b);
    }

    #[test]
    fn normalize_to_unit_matches_pad_scale() {
        let min = r(-1, 1);
        let max = r(1, 1);
        assert_eq!(normalize_to_unit(&r(7, 10), &min, &max).unwrap(), "0.85");
        assert_eq!(normalize_to_unit(&r(2, 5), &min, &max).unwrap(), "0.7");
    }

    /// Integer-part-first long division does not prematurely scale the numerator,
    /// so a small-VALUED rational carried by an enormous numerator/denominator
    /// (`num * 10^k` would blow past `u128::MAX`) still formats exactly.
    #[test]
    fn format_decimal_no_premature_overflow_on_small_valued_giant_ratios() {
        let big = 10i128.pow(33);
        // 10^33 / 10^33 = 1: old `num * 10^6 = 10^39` overflowed u128.
        let one = Rational {
            numerator: big,
            denominator: big,
        };
        assert_eq!(format_decimal(one).unwrap(), "1");
        // 10^33 / (2·10^33) = 0.5: same overflow, representable value.
        let half = Rational {
            numerator: big,
            denominator: 2 * big,
        };
        assert_eq!(format_decimal(half).unwrap(), "0.5");
        // Negative sign is preserved through the long-division path.
        let neg_half = Rational {
            numerator: -big,
            denominator: 2 * big,
        };
        assert_eq!(format_decimal(neg_half).unwrap(), "-0.5");
        // Existing exact values still format byte-identically.
        assert_eq!(format_decimal(r(17, 20)).unwrap(), "0.85");
        assert_eq!(format_decimal(r(2, 5)).unwrap(), "0.4");
        assert_eq!(format_decimal(r(12, 5)).unwrap(), "2.4");
    }

    // ── TripleIndex: a typed literal's datatype/language survives, and distinguishes
    // three otherwise-lexically-identical literals, through BOTH `index_graph` (via
    // the GTS-normalized `index_turtle`) and `index_dataset` ────────────────────────

    const TYPED_LITERAL_TTL: &str = "@prefix ex: <https://example.org/> .\n\
         @prefix xsd: <http://www.w3.org/2001/XMLSchema#> .\n\
         ex:s ex:intVal \"42\"^^xsd:integer .\n\
         ex:s ex:strVal \"42\"^^xsd:string .\n\
         ex:s ex:langVal \"42\"@en .\n";

    fn assert_typed_literal_round_trip(index: &TripleIndex) {
        let (lex_i, dt_i, lang_i) =
            first_literal_typed(index, "https://example.org/s", "https://example.org/intVal")
                .expect("xsd:integer literal present");
        assert_eq!(lex_i, "42");
        assert_eq!(dt_i, "http://www.w3.org/2001/XMLSchema#integer");
        assert_eq!(lang_i, None);

        let (lex_s, dt_s, lang_s) =
            first_literal_typed(index, "https://example.org/s", "https://example.org/strVal")
                .expect("xsd:string literal present");
        assert_eq!(lex_s, "42");
        assert_eq!(dt_s, "http://www.w3.org/2001/XMLSchema#string");
        assert_eq!(lang_s, None);

        let (lex_l, dt_l, lang_l) = first_literal_typed(
            index,
            "https://example.org/s",
            "https://example.org/langVal",
        )
        .expect("language-tagged literal present");
        assert_eq!(lex_l, "42");
        assert_eq!(
            dt_l,
            "http://www.w3.org/1999/02/22-rdf-syntax-ns#langString"
        );
        assert_eq!(lang_l, Some("en"));

        // Same lexical form, three DISTINCT literals: datatype/language is what
        // distinguishes them, never dropped.
        assert_ne!(dt_i, dt_s, "xsd:integer and xsd:string are distinct");
        assert_ne!(dt_s, dt_l, "xsd:string and rdf:langString are distinct");
        assert_ne!(lang_i, lang_l, "the language tag distinguishes langVal");

        // The lossy `first_literal` still returns just the lexical form.
        assert_eq!(
            first_literal(index, "https://example.org/s", "https://example.org/intVal").as_deref(),
            Some("42")
        );
    }

    #[test]
    fn typed_literal_round_trips_through_index_graph() {
        let index = index_turtle(TYPED_LITERAL_TTL.as_bytes()).expect("index_turtle");
        assert_typed_literal_round_trip(&index);
    }

    #[test]
    fn typed_literal_round_trips_through_index_dataset() {
        let dataset = purrdf::parse_dataset(TYPED_LITERAL_TTL.as_bytes(), "text/turtle", None)
            .expect("parse dataset");
        let index = index_dataset(&dataset);
        assert_typed_literal_round_trip(&index);
    }

    // ── TripleIndex: a blank node is `_:`-prefixed identically through both
    // `index_graph` and `index_dataset`, so a blank-node object round-trips back
    // into a followable subject key on either path ──────────────────────────────

    const BLANK_NODE_TTL: &str = "@prefix ex: <https://example.org/> .\n\
         ex:s ex:p _:b1 .\n\
         _:b1 ex:q ex:o .\n";

    fn assert_blank_node_round_trip(index: &TripleIndex) {
        let bnode_key = first_iri(index, "https://example.org/s", "https://example.org/p")
            .expect("blank-node object present");
        assert!(
            bnode_key.starts_with("_:"),
            "blank-node object key is `_:`-prefixed: {bnode_key}"
        );
        // The SAME key, used as a subject, resolves the blank node's own triple —
        // i.e. subject-position and object-position blank-node keys agree.
        let followed = first_iri(index, &bnode_key, "https://example.org/q")
            .expect("blank node is followable as a subject under its `_:`-prefixed key");
        assert_eq!(followed, "https://example.org/o");
    }

    #[test]
    fn blank_node_prefixed_through_index_graph() {
        let index = index_turtle(BLANK_NODE_TTL.as_bytes()).expect("index_turtle");
        assert_blank_node_round_trip(&index);
    }

    #[test]
    fn blank_node_prefixed_through_index_dataset() {
        let dataset = purrdf::parse_dataset(BLANK_NODE_TTL.as_bytes(), "text/turtle", None)
            .expect("parse dataset");
        let index = index_dataset(&dataset);
        assert_blank_node_round_trip(&index);
    }
}
