// SPDX-FileCopyrightText: 2026 Blackcat Informatics Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Rust-owned affect-intensity geometry.
//!
//! Overall affect intensity is the norm `√(xᵀGx)` over a **non-orthogonal,
//! positive-definite** metric Gram matrix `G` — never a raw L² norm — computed
//! **outside** the reasoned core (Principle 12) and fully deterministically.
//!
//! The crate exposes a reusable exact-rational inner-product-space over `G`
//! ([`InnerProductSpace`]) and the graph-reading front doors
//! ([`affective_geometry`], [`geometry_from_gts_bytes`], [`distance_and_cosine`])
//! consumed by the `gmeow affect` CLI and the EmotionML emitter.
//!
//! # Determinism contract
//!
//! All arithmetic is exact rational ([`Rational`], `i128`-backed, gcd-normalized,
//! hard-fail on overflow). The ONLY approximation is the final square root,
//! emitted as a fixed-precision decimal with [`SQRT_DECIMALS`] (`= 6`) fractional
//! digits, round-half-up at the seventh digit, via an integer floor-sqrt — never
//! `f64::sqrt`. Given the same inputs the output strings are byte-identical.

use std::cmp::Ordering;
use std::collections::{BTreeMap, HashMap};

use purrdf::gts::model::{Graph, Term, TermKind};

const GM: &str = "https://blackcatinformatics.ca/gmeow/";
const MATH: &str = "https://blackcatinformatics.ca/math/";
const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";

/// Fixed number of fractional decimal digits emitted for every square-root
/// (norm/distance/cosine) output. This is a hard, documented contract: the sqrt
/// is computed by an exact integer floor-sqrt at `10^(2*(k+1))` scale, then
/// rounded half-up at the `(k+1)`-th digit down to `k` digits.
pub const SQRT_DECIMALS: u32 = 6;

/// The one recognized affect norm function IRI (the metric-tensor norm).
const NORM_FUNCTION_IRI: &str = "https://blackcatinformatics.ca/gmeow/affectMetricTensorNorm";

/// The known, seeded `gmeow:WeightingPolicy` individuals (an OPEN vocabulary,
/// but an intensity record MUST name one of the grounded policies).
const KNOWN_WEIGHTING_POLICIES: &[&str] = &[
    "https://blackcatinformatics.ca/gmeow/weightingEqualCoreAffect",
    "https://blackcatinformatics.ca/gmeow/weightingValenceDominant",
];

fn gm(local: &str) -> String {
    format!("{GM}{local}")
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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rational {
    numerator: i128,
    denominator: i128,
}

impl Rational {
    /// Construct a normalized rational. Denominator zero, or an `i128::MIN`
    /// component (whose `abs` would overflow), is a hard fail.
    pub fn new(numerator: i128, denominator: i128) -> Result<Self, String> {
        if denominator == 0 {
            return Err("rational denominator must not be zero".to_string());
        }
        if numerator == i128::MIN || denominator == i128::MIN {
            return Err("rational components must not be i128::MIN".to_string());
        }
        let sign = if denominator < 0 { -1 } else { 1 };
        let g = gcd_i128(numerator, denominator);
        Ok(Self {
            numerator: sign * numerator / g,
            denominator: sign * denominator / g,
        })
    }

    /// The rational `value / 1`.
    pub fn from_i128(value: i128) -> Result<Self, String> {
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

    fn checked(op: &str, value: Option<i128>) -> Result<i128, String> {
        value.ok_or_else(|| format!("i128 overflow in rational {op}"))
    }

    /// Exact checked addition; hard-fails on overflow.
    pub fn checked_add(self, other: Self) -> Result<Self, String> {
        let left = Self::checked("add", self.numerator.checked_mul(other.denominator))?;
        let right = Self::checked("add", other.numerator.checked_mul(self.denominator))?;
        let num = Self::checked("add", left.checked_add(right))?;
        let den = Self::checked("add", self.denominator.checked_mul(other.denominator))?;
        Self::new(num, den)
    }

    /// Exact checked subtraction; hard-fails on overflow.
    pub fn checked_sub(self, other: Self) -> Result<Self, String> {
        let left = Self::checked("sub", self.numerator.checked_mul(other.denominator))?;
        let right = Self::checked("sub", other.numerator.checked_mul(self.denominator))?;
        let num = Self::checked("sub", left.checked_sub(right))?;
        let den = Self::checked("sub", self.denominator.checked_mul(other.denominator))?;
        Self::new(num, den)
    }

    /// Exact checked multiplication; hard-fails on overflow.
    pub fn checked_mul(self, other: Self) -> Result<Self, String> {
        let num = Self::checked("mul", self.numerator.checked_mul(other.numerator))?;
        let den = Self::checked("mul", self.denominator.checked_mul(other.denominator))?;
        Self::new(num, den)
    }

    /// Exact checked division; hard-fails on overflow or division by zero.
    pub fn checked_div(self, other: Self) -> Result<Self, String> {
        if other.is_zero() {
            return Err("rational division by zero".to_string());
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
    pub fn parse_decimal(text: &str) -> Result<Self, String> {
        let text = text.trim();
        if text.is_empty() {
            return Err("empty decimal literal".to_string());
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
            return Err(format!("not a decimal literal: {text:?}"));
        }
        let digits = format!("{int_part}{frac_part}");
        let numerator: i128 = digits
            .parse()
            .map_err(|_| format!("decimal literal out of i128 range: {text:?}"))?;
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

fn pow10_i128(exp: u32) -> Result<i128, String> {
    let mut acc: i128 = 1;
    for _ in 0..exp {
        acc = acc
            .checked_mul(10)
            .ok_or_else(|| "i128 overflow computing power of ten".to_string())?;
    }
    Ok(acc)
}

/// Deterministic `√q` as a fixed-precision decimal string with [`SQRT_DECIMALS`]
/// fractional digits (round-half-up at the seventh digit). `q` must be `>= 0`.
fn sqrt_rational_decimal(q: Rational) -> Result<String, String> {
    if q.numerator < 0 {
        return Err("cannot take the square root of a negative quadratic form".to_string());
    }
    let k = SQRT_DECIMALS;
    // scaled = floor(q * 10^(2*(k+1))); isqrt(scaled) = floor(√q * 10^(k+1)).
    let scale = pow10_i128(2 * (k + 1))?;
    let numerator = q
        .numerator
        .checked_mul(scale)
        .ok_or_else(|| "i128 overflow scaling quadratic form for sqrt".to_string())?;
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
/// formatter (mirrors `music::format_decimal`).
fn format_decimal(value: Rational) -> Result<String, String> {
    let k = SQRT_DECIMALS;
    let unit = pow10_i128(k)?;
    let sign = if value.numerator < 0 { "-" } else { "" };
    let num = value.numerator.unsigned_abs();
    let den = value.denominator.unsigned_abs();
    let unit_u = unit as u128;
    // round(|num|/den * 10^k) = (|num|*10^k + den/2) / den
    let scaled_num = num
        .checked_mul(unit_u)
        .ok_or_else(|| "u128 overflow formatting decimal".to_string())?;
    let rounded = (scaled_num + den / 2) / den;
    let int_part = rounded / unit_u;
    let frac_part = rounded % unit_u;
    if frac_part == 0 {
        return Ok(format!("{sign}{int_part}"));
    }
    let mut frac = format!("{frac_part:0width$}", width = k as usize);
    while frac.ends_with('0') {
        frac.pop();
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
    pub fn new(gram: Vec<Vec<Rational>>) -> Result<Self, String> {
        let n = gram.len();
        if gram.iter().any(|row| row.len() != n) {
            return Err("Gram matrix must be square".to_string());
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
    fn matvec(&self, x: &[Rational]) -> Result<Vec<Rational>, String> {
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
    pub fn inner(&self, x: &[Rational], y: &[Rational]) -> Result<Rational, String> {
        let gy = self.matvec(y)?;
        let x = self.padded(x);
        let mut acc = Rational::zero();
        for (xi, gyi) in x.iter().zip(gy.iter()) {
            acc = acc.checked_add(xi.checked_mul(*gyi)?)?;
        }
        Ok(acc)
    }

    /// The exact quadratic form `Q = xᵀGx = ⟨x,x⟩`.
    pub fn quadratic_form(&self, x: &[Rational]) -> Result<Rational, String> {
        self.inner(x, x)
    }

    /// The norm `‖x‖_G = √(xᵀGx)`, as a fixed-precision decimal string.
    pub fn norm(&self, x: &[Rational]) -> Result<String, String> {
        sqrt_rational_decimal(self.quadratic_form(x)?)
    }

    /// The distance `‖x − y‖_G`, as a fixed-precision decimal string.
    pub fn distance(&self, x: &[Rational], y: &[Rational]) -> Result<String, String> {
        let x = self.padded(x);
        let y = self.padded(y);
        let diff = x
            .iter()
            .zip(y.iter())
            .map(|(xi, yi)| xi.checked_sub(*yi))
            .collect::<Result<Vec<_>, _>>()?;
        self.norm(&diff)
    }

    /// The cosine of the angle `⟨x,y⟩ / (‖x‖·‖y‖)`, as a signed fixed-precision
    /// decimal string. A zero vector makes the angle undefined → hard fail.
    pub fn cosine(&self, x: &[Rational], y: &[Rational]) -> Result<String, String> {
        let qx = self.quadratic_form(x)?;
        let qy = self.quadratic_form(y)?;
        if qx.is_zero() || qy.is_zero() {
            return Err("cosine is undefined for a zero vector".to_string());
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
    pub fn angle(&self, x: &[Rational], y: &[Rational]) -> Result<f64, String> {
        let cosine: f64 = self
            .cosine(x, y)?
            .parse()
            .map_err(|_| "bad cosine".to_string())?;
        Ok(cosine.clamp(-1.0, 1.0).acos())
    }

    /// `true` iff `x ⟂ y` under `G` (`⟨x,y⟩` exactly zero).
    pub fn is_orthogonal(&self, x: &[Rational], y: &[Rational]) -> Result<bool, String> {
        Ok(self.inner(x, y)?.is_zero())
    }

    /// The exact metric projection of `x` onto `onto`:
    /// `(⟨x,onto⟩ / ⟨onto,onto⟩) · onto`. A zero `onto` is a hard fail.
    pub fn project(&self, x: &[Rational], onto: &[Rational]) -> Result<Vec<Rational>, String> {
        let denom = self.quadratic_form(onto)?;
        if denom.is_zero() {
            return Err("cannot project onto a zero vector".to_string());
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
    pub fn ldlt_pivots(&self) -> Result<Vec<Rational>, String> {
        let n = self.dim();
        let mut l = vec![vec![Rational::zero(); n]; n];
        let mut d = vec![Rational::zero(); n];
        for j in 0..n {
            let mut dj = self.gram[j][j];
            for k in 0..j {
                dj = dj.checked_sub(l[j][k].checked_mul(l[j][k])?.checked_mul(d[k])?)?;
            }
            if dj.is_non_positive() {
                return Err(format!(
                    "Gram matrix is not positive-definite: pivot {j} = {} is not > 0",
                    dj.ratio_string()
                ));
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
    pub fn dominant_axis(&self, x: &[Rational]) -> Result<usize, String> {
        if self.dim() == 0 {
            return Err("cannot pick a dominant axis of a zero-dimensional space".to_string());
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
// Public normalization shared with the EmotionML emitter (Task 5).
// ---------------------------------------------------------------------------

/// Unit-clamp normalization of a cell magnitude `value` on `[range_min,
/// range_max]` into `[0,1]`: `(value − range_min)/(range_max − range_min)`, exact
/// rational, then the trimmed-decimal formatter. E.g. on the PAD unit scale
/// `[-1,1]`: valence `0.7` → `"0.85"`, arousal `0.4` → `"0.7"`.
pub fn normalize_to_unit(
    value: &Rational,
    range_min: &Rational,
    range_max: &Rational,
) -> Result<String, String> {
    let span = range_max.checked_sub(*range_min)?;
    if span.is_zero() {
        return Err("scale profile range is degenerate (min == max)".to_string());
    }
    format_decimal(value.checked_sub(*range_min)?.checked_div(span)?)
}

// ---------------------------------------------------------------------------
// Result of reading + computing one derived-intensity observation.
// ---------------------------------------------------------------------------

/// A per-axis unit-clamp-normalized reading.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NormalizedAxis {
    /// The core-affect axis index (valence 0, arousal 1, dominance 2, unpredictability 3).
    pub axis: usize,
    /// The appraisal-dimension IRI.
    pub dimension: String,
    /// The unit-clamp-normalized value on `[0,1]` (trimmed decimal).
    pub value: String,
}

/// The computed affect-intensity geometry of one
/// `gmeow:DerivedAffectIntensityObservation`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Geometry {
    /// The observation IRI this geometry was computed for.
    pub observation: String,
    /// The exact quadratic form `Q = xᵀGx` as a printable ratio (e.g. `"79/100"`).
    pub quadratic_form: String,
    /// The intensity `√Q` as a fixed-precision decimal string (e.g. `"0.888819"`).
    pub intensity: String,
    /// The metric-aware dominant-axis appraisal-dimension IRI.
    pub dominant_axis: String,
    /// The per-axis unit-clamp-normalized values (ascending by axis).
    pub normalized: Vec<NormalizedAxis>,
    /// The LDLᵀ pivots of `G` (positive-definiteness certificate; all `> 0`),
    /// as printable ratios.
    pub pivots: Vec<String>,
}

// ---------------------------------------------------------------------------
// Graph indexing (mirrors the music crate's hand-rolled TripleIndex).
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum Node {
    Iri(String),
    Bnode(String),
    Literal(String),
}

#[derive(Debug, Default)]
struct TripleIndex {
    by_subject: HashMap<String, HashMap<String, Vec<Node>>>,
}

fn node_id(term: &Term) -> Option<String> {
    match term.kind {
        TermKind::Iri => term.value.clone(),
        TermKind::Bnode => term.value.as_ref().map(|value| format!("_:{value}")),
        TermKind::Literal | TermKind::Triple => None,
    }
}

fn object_node(term: &Term) -> Option<Node> {
    match term.kind {
        TermKind::Iri => term.value.clone().map(Node::Iri),
        TermKind::Bnode => term
            .value
            .as_ref()
            .map(|value| Node::Bnode(format!("_:{value}"))),
        TermKind::Literal => Some(Node::Literal(term.value.clone().unwrap_or_default())),
        TermKind::Triple => None,
    }
}

fn index_graph(graph: &Graph) -> TripleIndex {
    let mut index = TripleIndex::default();
    for quad in graph.quad_terms().filter(|quad| quad.graph_name.is_none()) {
        let (Some(subject), Some(predicate), Some(object)) = (
            node_id(quad.subject),
            node_id(quad.predicate),
            object_node(quad.object),
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

fn objects<'a>(index: &'a TripleIndex, subject: &str, predicate: &str) -> &'a [Node] {
    index
        .by_subject
        .get(subject)
        .and_then(|preds| preds.get(predicate))
        .map(Vec::as_slice)
        .unwrap_or(&[])
}

fn first_iri(index: &TripleIndex, subject: &str, predicate: &str) -> Option<String> {
    objects(index, subject, predicate)
        .iter()
        .find_map(|node| match node {
            Node::Iri(value) | Node::Bnode(value) => Some(value.clone()),
            Node::Literal(_) => None,
        })
}

fn all_iris(index: &TripleIndex, subject: &str, predicate: &str) -> Vec<String> {
    objects(index, subject, predicate)
        .iter()
        .filter_map(|node| match node {
            Node::Iri(value) | Node::Bnode(value) => Some(value.clone()),
            Node::Literal(_) => None,
        })
        .collect()
}

fn first_literal(index: &TripleIndex, subject: &str, predicate: &str) -> Option<String> {
    objects(index, subject, predicate)
        .iter()
        .find_map(|node| match node {
            Node::Literal(value) => Some(value.clone()),
            Node::Iri(_) | Node::Bnode(_) => None,
        })
}

fn first_i128(index: &TripleIndex, subject: &str, predicate: &str) -> Option<i128> {
    first_literal(index, subject, predicate)?
        .trim()
        .parse()
        .ok()
}

fn has_type(index: &TripleIndex, subject: &str, class: &str) -> bool {
    objects(index, subject, RDF_TYPE)
        .iter()
        .any(|node| matches!(node, Node::Iri(value) if value == class))
}

// ---------------------------------------------------------------------------
// Reading the affect data model from the graph.
// ---------------------------------------------------------------------------

/// The vector + metric inputs pulled from one observation, ready for geometry.
struct Inputs {
    space: InnerProductSpace,
    /// The zero-completed coordinate vector over the basis.
    vector: Vec<Rational>,
    /// Axis index → appraisal-dimension IRI.
    axis_to_dim: BTreeMap<usize, String>,
    /// Present cells: (axis, dimension IRI, raw appraisal value).
    cells: Vec<(usize, String, Rational)>,
    range_min: Rational,
    range_max: Rational,
}

fn load_gram(index: &TripleIndex, gram_iri: &str) -> Result<Vec<(usize, usize, Rational)>, String> {
    let entries = all_iris(index, gram_iri, &math("hasEntry"));
    if entries.is_empty() {
        return Err(format!(
            "Gram matrix {gram_iri} declares no math:hasEntry cells"
        ));
    }
    let mut cells = Vec::new();
    for entry in entries {
        let row = first_i128(index, &entry, &math("atRow"))
            .ok_or_else(|| format!("matrix entry {entry} missing math:atRow"))?;
        let col = first_i128(index, &entry, &math("atColumn"))
            .ok_or_else(|| format!("matrix entry {entry} missing math:atColumn"))?;
        if row < 0 || col < 0 {
            return Err(format!("matrix entry {entry} has a negative index"));
        }
        let value_iri = first_iri(index, &entry, &math("entryValue"))
            .ok_or_else(|| format!("matrix entry {entry} missing math:entryValue"))?;
        let num = first_i128(index, &value_iri, &math("numerator"))
            .ok_or_else(|| format!("rational value {value_iri} missing math:numerator"))?;
        let den = first_i128(index, &value_iri, &math("denominator"))
            .ok_or_else(|| format!("rational value {value_iri} missing math:denominator"))?;
        cells.push((row as usize, col as usize, Rational::new(num, den)?));
    }
    Ok(cells)
}

fn load_cells(
    index: &TripleIndex,
    vector_iri: &str,
) -> Result<Vec<(usize, String, Rational)>, String> {
    let component_iris = all_iris(index, vector_iri, &gm("vectorComponent"));
    if component_iris.is_empty() {
        return Err(format!(
            "affect vector {vector_iri} declares no gmeow:vectorComponent cells"
        ));
    }
    let mut cells = Vec::new();
    for cell in component_iris {
        let dimension = first_iri(index, &cell, &gm("appraisalDimension"))
            .ok_or_else(|| format!("appraisal {cell} missing gmeow:appraisalDimension"))?;
        let axis = first_i128(index, &dimension, &gm("coreAxisIndex"))
            .ok_or_else(|| format!("appraisal dimension {dimension} has no gmeow:coreAxisIndex"))?;
        if axis < 0 {
            return Err(format!(
                "dimension {dimension} has a negative coreAxisIndex"
            ));
        }
        let value = Rational::parse_decimal(
            &first_literal(index, &cell, &gm("appraisalValue"))
                .ok_or_else(|| format!("appraisal {cell} missing gmeow:appraisalValue"))?,
        )?;
        cells.push((axis as usize, dimension, value));
    }
    cells.sort_by_key(|(axis, _, _)| *axis);
    Ok(cells)
}

fn load_inputs(index: &TripleIndex, observation: &str) -> Result<Inputs, String> {
    let norm_fn = first_iri(index, observation, &gm("normFunction"))
        .ok_or_else(|| format!("observation {observation} missing gmeow:normFunction"))?;
    if norm_fn != NORM_FUNCTION_IRI {
        return Err(format!(
            "unrecognized gmeow:normFunction {norm_fn}: expected {NORM_FUNCTION_IRI}"
        ));
    }
    let policy = first_iri(index, observation, &gm("weightingPolicy"))
        .ok_or_else(|| format!("observation {observation} missing gmeow:weightingPolicy"))?;
    if !KNOWN_WEIGHTING_POLICIES.contains(&policy.as_str()) {
        return Err(format!("unrecognized gmeow:weightingPolicy {policy}"));
    }
    let basis = first_iri(index, observation, &gm("intensityBasis"))
        .ok_or_else(|| format!("observation {observation} missing gmeow:intensityBasis"))?;
    let profile = first_iri(index, observation, &gm("metricProfile"))
        .ok_or_else(|| format!("observation {observation} missing gmeow:metricProfile"))?;
    let gram_iri = first_iri(index, &profile, &gm("metricGram"))
        .ok_or_else(|| format!("scale profile {profile} missing gmeow:metricGram"))?;
    let range_min = Rational::parse_decimal(
        &first_literal(index, &profile, &gm("profileRangeMin"))
            .ok_or_else(|| format!("scale profile {profile} missing gmeow:profileRangeMin"))?,
    )?;
    let range_max = Rational::parse_decimal(
        &first_literal(index, &profile, &gm("profileRangeMax"))
            .ok_or_else(|| format!("scale profile {profile} missing gmeow:profileRangeMax"))?,
    )?;

    let gram_cells = load_gram(index, &gram_iri)?;
    let vector_cells = load_cells(index, &basis)?;

    let dim = gram_cells
        .iter()
        .flat_map(|(r, c, _)| [*r, *c])
        .chain(vector_cells.iter().map(|(axis, _, _)| *axis))
        .max()
        .map(|m| m + 1)
        .unwrap_or(0);
    if dim == 0 {
        return Err("affect geometry has an empty basis".to_string());
    }

    let mut gram = vec![vec![Rational::zero(); dim]; dim];
    for (row, col, value) in gram_cells {
        gram[row][col] = value;
        gram[col][row] = value; // declared symmetric fill
    }

    let mut vector = vec![Rational::zero(); dim];
    let mut axis_to_dim = BTreeMap::new();
    for (axis, dimension, value) in &vector_cells {
        vector[*axis] = *value;
        axis_to_dim.insert(*axis, dimension.clone());
    }

    Ok(Inputs {
        space: InnerProductSpace::new(gram)?,
        vector,
        axis_to_dim,
        cells: vector_cells,
        range_min,
        range_max,
    })
}

fn compute_geometry(index: &TripleIndex, observation: &str) -> Result<Geometry, String> {
    let inputs = load_inputs(index, observation)?;
    let quadratic = inputs.space.quadratic_form(&inputs.vector)?;
    let intensity = sqrt_rational_decimal(quadratic)?;
    let pivots = inputs.space.ldlt_pivots()?; // hard-fails on non-PD G
    let dominant = inputs.space.dominant_axis(&inputs.vector)?;
    let dominant_axis = inputs
        .axis_to_dim
        .get(&dominant)
        .cloned()
        .ok_or_else(|| format!("dominant axis {dominant} has no declared dimension"))?;
    let normalized = inputs
        .cells
        .iter()
        .map(|(axis, dimension, value)| {
            Ok(NormalizedAxis {
                axis: *axis,
                dimension: dimension.clone(),
                value: normalize_to_unit(value, &inputs.range_min, &inputs.range_max)?,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    Ok(Geometry {
        observation: observation.to_string(),
        quadratic_form: quadratic.ratio_string(),
        intensity,
        dominant_axis,
        normalized,
        pivots: pivots.into_iter().map(Rational::ratio_string).collect(),
    })
}

// ---------------------------------------------------------------------------
// Public graph front doors.
// ---------------------------------------------------------------------------

/// Compute the affect-intensity geometry of one
/// `gmeow:DerivedAffectIntensityObservation` in `graph`.
pub fn affective_geometry(graph: &Graph, observation_iri: &str) -> Result<Geometry, String> {
    compute_geometry(&index_graph(graph), observation_iri)
}

fn derived_observations(index: &TripleIndex) -> Vec<String> {
    let class = gm("DerivedAffectIntensityObservation");
    let mut observations = index
        .by_subject
        .keys()
        .filter(|subject| has_type(index, subject, &class))
        .cloned()
        .collect::<Vec<_>>();
    observations.sort();
    observations
}

/// Compute the geometry of every `gmeow:DerivedAffectIntensityObservation` in a
/// GTS bundle (or the single named one when `observation_iri` is `Some`), in
/// deterministic ascending-IRI order.
pub fn geometry_from_gts_bytes(
    bytes: &[u8],
    observation_iri: Option<&str>,
) -> Result<Vec<Geometry>, String> {
    let graph = purrdf::gts::reader::read(bytes, false, None);
    let index = index_graph(&graph);
    let observations = match observation_iri {
        Some(iri) => vec![iri.to_string()],
        None => derived_observations(&index),
    };
    if observations.is_empty() {
        return Err("no gmeow:DerivedAffectIntensityObservation found in graph".to_string());
    }
    observations
        .iter()
        .map(|iri| compute_geometry(&index, iri))
        .collect()
}

/// The metric distance `‖x_a − x_b‖_G` and cosine `⟨x_a,x_b⟩/(‖x_a‖‖x_b‖)`
/// between the basis vectors of two intensity observations, sharing the metric
/// of `obs_a`. Returned as `(distance, cosine)` fixed-precision decimals.
pub fn distance_and_cosine(
    graph: &Graph,
    obs_a_iri: &str,
    obs_b_iri: &str,
) -> Result<(String, String), String> {
    let index = index_graph(graph);
    let a = load_inputs(&index, obs_a_iri)?;
    let b = load_inputs(&index, obs_b_iri)?;
    // The metric and the axis→dimension basis of `obs_b` are discarded below —
    // both vectors are measured with `obs_a`'s `InnerProductSpace`. That is only
    // meaningful when the two observations share the same metric basis; a
    // mismatch would otherwise be silently zero-padded/truncated into a
    // well-formed but meaningless number. Hard-fail instead.
    if a.space != b.space || a.axis_to_dim != b.axis_to_dim {
        return Err(format!(
            "distance requires both observations to share the same metric basis; \
             obs_a {obs_a_iri} and obs_b {obs_b_iri} differ in Gram matrix / axis map"
        ));
    }
    let distance = a.space.distance(&a.vector, &b.vector)?;
    let cosine = a.space.cosine(&a.vector, &b.vector)?;
    Ok((distance, cosine))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fmt::Write as _;

    use purrdf::gts_compose::{DEFAULT_RSYNCABLE_THRESHOLD, SnapshotBuilder, emit_gts};
    use purrdf::{NativeRdfFormat, parse_dataset};

    fn r(num: i128, den: i128) -> Rational {
        Rational::new(num, den).expect("rational")
    }

    /// The canonical AC2 correlated metric G = [[1, 1/4], [1/4, 1]].
    fn correlated_gram() -> InnerProductSpace {
        InnerProductSpace::new(vec![vec![r(1, 1), r(1, 4)], vec![r(1, 4), r(1, 1)]]).expect("space")
    }

    fn turtle_to_gts(turtle: &str) -> Vec<u8> {
        let dataset = parse_dataset(
            turtle.as_bytes(),
            NativeRdfFormat::Turtle.media_type(),
            None,
        )
        .expect("parse turtle");
        let mut builder = SnapshotBuilder::default();
        builder.add_dataset(&dataset).expect("add dataset");
        emit_gts(
            &builder,
            "dist",
            None,
            Vec::new(),
            Vec::new(),
            None,
            None,
            None,
            DEFAULT_RSYNCABLE_THRESHOLD,
        )
        .expect("emit gts")
    }

    #[test]
    fn parse_decimal_is_exact() {
        assert_eq!(Rational::parse_decimal("0.7").unwrap(), r(7, 10));
        assert_eq!(Rational::parse_decimal("-1.0").unwrap(), r(-1, 1));
        assert_eq!(Rational::parse_decimal("0.5").unwrap(), r(1, 2));
        assert_eq!(Rational::parse_decimal("2").unwrap(), r(2, 1));
    }

    // AC2: √(xᵀGx) over the NON-orthogonal G differs from raw L².
    #[test]
    fn ac2_metric_norm_distinct_from_raw_l2() {
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

    // E2: LDLᵀ certifies PD (pivots 1, 15/16); an indefinite G names its pivot.
    #[test]
    fn e2_ldlt_positive_definite_certificate() {
        let pivots = correlated_gram().ldlt_pivots().unwrap();
        assert_eq!(pivots, vec![r(1, 1), r(15, 16)]);

        let indefinite =
            InnerProductSpace::new(vec![vec![r(1, 1), r(2, 1)], vec![r(2, 1), r(1, 1)]]).unwrap();
        let err = indefinite.ldlt_pivots().unwrap_err();
        // Pivot 0 = 1 (> 0), pivot 1 = 1 − 4 = −3 (not > 0).
        assert!(err.contains("pivot 1"), "{err}");
        assert!(err.contains("-3"), "{err}");
    }

    // E4: metric-aware dominant axis differs from the raw-max component.
    #[test]
    fn e4_dominant_axis_is_metric_aware_not_raw_max() {
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

    // E3/E5: distance and cosine match hand-computed exact values.
    #[test]
    fn e3_e5_distance_and_cosine_hand_checked() {
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

    // E6/E12: determinism, overflow hard-fail, and undefined-input hard fails.
    #[test]
    fn e6_e12_determinism_and_hard_fails() {
        let space = correlated_gram();
        let x = [r(7, 10), r(2, 5)];
        let first = space.norm(&x).unwrap();
        let second = space.norm(&x).unwrap();
        assert_eq!(first, second); // byte-identical, run twice

        // Overflow: (i128::MAX/2) · 4 must hard-fail, never wrap.
        let big = r(i128::MAX / 2, 1);
        assert!(big.checked_mul(r(4, 1)).unwrap_err().contains("overflow"));

        // Zero-vector cosine is undefined → Err.
        let zero = [r(0, 1), r(0, 1)];
        assert!(space.cosine(&x, &zero).unwrap_err().contains("zero vector"));
    }

    // Hard fails on unrecognized declared handles, via the graph path.
    #[test]
    fn unrecognized_norm_and_policy_hard_fail() {
        let bad_norm = observation_turtle(
            "gmeow:affectMetricTensorNorm2",
            "gmeow:weightingValenceDominant",
        );
        let err = geometry_from_gts_bytes(&turtle_to_gts(&bad_norm), None).unwrap_err();
        assert!(err.contains("normFunction"), "{err}");

        let bad_policy =
            observation_turtle("gmeow:affectMetricTensorNorm", "gmeow:weightingMadeUp");
        let err = geometry_from_gts_bytes(&turtle_to_gts(&bad_policy), None).unwrap_err();
        assert!(err.contains("weightingPolicy"), "{err}");
    }

    // A vector cell whose dimension lacks coreAxisIndex is a hard fail.
    #[test]
    fn missing_core_axis_index_hard_fails() {
        let mut turtle = observation_turtle(
            "gmeow:affectMetricTensorNorm",
            "gmeow:weightingValenceDominant",
        );
        // Drop the coreAxisIndex declarations.
        turtle = turtle
            .lines()
            .filter(|line| !line.contains("gmeow:coreAxisIndex"))
            .collect::<Vec<_>>()
            .join("\n");
        let err = geometry_from_gts_bytes(&turtle_to_gts(&turtle), None).unwrap_err();
        assert!(err.contains("coreAxisIndex"), "{err}");
    }

    // Graph-parse path is load-bearing: intensity + dominant axis from turtle.
    #[test]
    fn graph_parse_path_computes_intensity_and_dominant_axis() {
        let turtle = observation_turtle(
            "gmeow:affectMetricTensorNorm",
            "gmeow:weightingValenceDominant",
        );
        let bytes = turtle_to_gts(&turtle);
        let all = geometry_from_gts_bytes(&bytes, None).unwrap();
        assert_eq!(all.len(), 1);
        let geom = &all[0];
        assert_eq!(geom.intensity, "0.888819");
        assert_eq!(geom.quadratic_form, "79/100");
        assert_eq!(geom.dominant_axis, gm("dimensionValence"));
        assert_eq!(geom.pivots, vec!["1".to_string(), "15/16".to_string()]);
        // Unit-clamp normalization on PAD [-1, 1]: valence 0.7 → 0.85, arousal 0.4 → 0.7.
        assert_eq!(
            geom.normalized,
            vec![
                NormalizedAxis {
                    axis: 0,
                    dimension: gm("dimensionValence"),
                    value: "0.85".to_string(),
                },
                NormalizedAxis {
                    axis: 1,
                    dimension: gm("dimensionArousal"),
                    value: "0.7".to_string(),
                },
            ]
        );

        // Same call twice → byte-identical structure (determinism).
        let again = geometry_from_gts_bytes(&bytes, None).unwrap();
        assert_eq!(all, again);

        // Single-observation selection agrees with the sweep.
        let one = affective_geometry(
            &purrdf::gts::reader::read(&bytes, false, None),
            &geom.observation,
        )
        .unwrap();
        assert_eq!(&one, geom);
    }

    // The Ord cross-multiply stays exact for the canonical correlated-metric
    // dominant-axis case (valence 0.7 / arousal 0.4 over G = [[1,1/4],[1/4,1]]):
    // axis0 = 0.7·(0.7 + 0.25·0.4) = 0.56 > axis1 = 0.4·(0.25·0.7 + 0.4) = 0.23.
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
        // With a huge denominator, self.numerator · other.denominator overflows.
        let a = r(i128::MAX / 2, 1);
        let b = r(1, i128::MAX / 2 + 2);
        // a.numerator (≈ MAX/2) · b.denominator (≈ MAX/2) overflows i128.
        let _ = a.cmp(&b);
    }

    #[test]
    fn normalize_to_unit_matches_pad_scale() {
        let min = r(-1, 1);
        let max = r(1, 1);
        assert_eq!(normalize_to_unit(&r(7, 10), &min, &max).unwrap(), "0.85");
        assert_eq!(normalize_to_unit(&r(2, 5), &min, &max).unwrap(), "0.7");
    }

    /// A complete `gmeow:DerivedAffectIntensityObservation` over the correlated
    /// metric G = [[1, 1/4], [1/4, 1]], vector valence 0.7 / arousal 0.4.
    fn observation_turtle(norm_fn: &str, policy: &str) -> String {
        let mut out = String::new();
        let _ = writeln!(
            out,
            "@prefix gmeow: <{GM}> .\n@prefix math: <{MATH}> .\n@prefix xsd: <http://www.w3.org/2001/XMLSchema#> .\n@prefix ex: <https://blackcatinformatics.ca/gmeow/examples/affect/> ."
        );
        out.push_str(
            r#"
gmeow:dimensionValence a gmeow:CoreAffectDimension ; gmeow:coreAxisIndex "0"^^xsd:nonNegativeInteger .
gmeow:dimensionArousal a gmeow:CoreAffectDimension ; gmeow:coreAxisIndex "1"^^xsd:nonNegativeInteger .

ex:padUnitScale a gmeow:AffectScaleProfile ;
    gmeow:profileRangeMin "-1.0"^^xsd:decimal ;
    gmeow:profileRangeMax "1.0"^^xsd:decimal ;
    gmeow:metricGram ex:correlatedGram .

ex:correlatedGram a math:GramMatrix ;
    math:definiteness math:positiveDefinite ;
    math:hasEntry ex:g00 , ex:g01 , ex:g11 .

ex:g00 a math:MatrixEntry ; math:atRow "0"^^xsd:integer ; math:atColumn "0"^^xsd:integer ; math:entryValue ex:ratOne .
ex:g01 a math:MatrixEntry ; math:atRow "0"^^xsd:integer ; math:atColumn "1"^^xsd:integer ; math:entryValue ex:ratQuarter .
ex:g11 a math:MatrixEntry ; math:atRow "1"^^xsd:integer ; math:atColumn "1"^^xsd:integer ; math:entryValue ex:ratOne .

ex:ratOne a math:RationalValue ; math:numerator "1"^^xsd:integer ; math:denominator "1"^^xsd:integer .
ex:ratQuarter a math:RationalValue ; math:numerator "1"^^xsd:integer ; math:denominator "4"^^xsd:integer .

ex:vec a gmeow:AffectVectorObservation ;
    gmeow:vectorComponent ex:valenceCell , ex:arousalCell .

ex:valenceCell a gmeow:Appraisal ;
    gmeow:appraisalDimension gmeow:dimensionValence ;
    gmeow:appraisalValue "0.7"^^xsd:decimal .

ex:arousalCell a gmeow:Appraisal ;
    gmeow:appraisalDimension gmeow:dimensionArousal ;
    gmeow:appraisalValue "0.4"^^xsd:decimal .
"#,
        );
        let _ = writeln!(
            out,
            "ex:intensity a gmeow:DerivedAffectIntensityObservation ;\n    gmeow:intensityBasis ex:vec ;\n    gmeow:metricProfile ex:padUnitScale ;\n    gmeow:weightingPolicy {policy} ;\n    gmeow:normFunction {norm_fn} ;\n    gmeow:derivedByFunction gmeow:fnAffectiveIntensity ."
        );
        out
    }

    /// A `gmeow:DerivedAffectIntensityObservation` named `suffix`, over a 2×2
    /// metric with off-diagonal `off = off_num/off_den` and vector
    /// `(v0, v1)` (each written as `n/10`). All resource IRIs are suffixed so two
    /// such blocks compose into one graph with fully independent bases.
    fn distinct_observation_turtle(
        suffix: &str,
        off_num: i128,
        off_den: i128,
        v0_tenths: i128,
        v1_tenths: i128,
    ) -> String {
        let mut out = String::new();
        let _ = writeln!(
            out,
            r#"ex:padUnitScale{suffix} a gmeow:AffectScaleProfile ;
    gmeow:profileRangeMin "-1.0"^^xsd:decimal ;
    gmeow:profileRangeMax "1.0"^^xsd:decimal ;
    gmeow:metricGram ex:gram{suffix} .

ex:gram{suffix} a math:GramMatrix ;
    math:definiteness math:positiveDefinite ;
    math:hasEntry ex:g00{suffix} , ex:g01{suffix} , ex:g11{suffix} .

ex:g00{suffix} a math:MatrixEntry ; math:atRow "0"^^xsd:integer ; math:atColumn "0"^^xsd:integer ; math:entryValue ex:ratOne{suffix} .
ex:g01{suffix} a math:MatrixEntry ; math:atRow "0"^^xsd:integer ; math:atColumn "1"^^xsd:integer ; math:entryValue ex:ratOff{suffix} .
ex:g11{suffix} a math:MatrixEntry ; math:atRow "1"^^xsd:integer ; math:atColumn "1"^^xsd:integer ; math:entryValue ex:ratOne{suffix} .

ex:ratOne{suffix} a math:RationalValue ; math:numerator "1"^^xsd:integer ; math:denominator "1"^^xsd:integer .
ex:ratOff{suffix} a math:RationalValue ; math:numerator "{off_num}"^^xsd:integer ; math:denominator "{off_den}"^^xsd:integer .

ex:vec{suffix} a gmeow:AffectVectorObservation ;
    gmeow:vectorComponent ex:valenceCell{suffix} , ex:arousalCell{suffix} .

ex:valenceCell{suffix} a gmeow:Appraisal ;
    gmeow:appraisalDimension gmeow:dimensionValence ;
    gmeow:appraisalValue "0.{v0_tenths}"^^xsd:decimal .

ex:arousalCell{suffix} a gmeow:Appraisal ;
    gmeow:appraisalDimension gmeow:dimensionArousal ;
    gmeow:appraisalValue "0.{v1_tenths}"^^xsd:decimal .

ex:intensity{suffix} a gmeow:DerivedAffectIntensityObservation ;
    gmeow:intensityBasis ex:vec{suffix} ;
    gmeow:metricProfile ex:padUnitScale{suffix} ;
    gmeow:weightingPolicy gmeow:weightingValenceDominant ;
    gmeow:normFunction gmeow:affectMetricTensorNorm ;
    gmeow:derivedByFunction gmeow:fnAffectiveIntensity ."#
        );
        out
    }

    fn two_observation_graph(a: &str, b: &str) -> Graph {
        let mut turtle = String::new();
        let _ = writeln!(
            turtle,
            "@prefix gmeow: <{GM}> .\n@prefix math: <{MATH}> .\n@prefix xsd: <http://www.w3.org/2001/XMLSchema#> .\n@prefix ex: <https://blackcatinformatics.ca/gmeow/examples/affect/> ."
        );
        turtle.push_str(
            "gmeow:dimensionValence a gmeow:CoreAffectDimension ; gmeow:coreAxisIndex \"0\"^^xsd:nonNegativeInteger .\n",
        );
        turtle.push_str(
            "gmeow:dimensionArousal a gmeow:CoreAffectDimension ; gmeow:coreAxisIndex \"1\"^^xsd:nonNegativeInteger .\n",
        );
        turtle.push_str(a);
        turtle.push('\n');
        turtle.push_str(b);
        let bytes = turtle_to_gts(&turtle);
        purrdf::gts::reader::read(&bytes, false, None)
    }

    fn obs_iri(suffix: &str) -> String {
        format!("https://blackcatinformatics.ca/gmeow/examples/affect/intensity{suffix}")
    }

    // Matching metric basis (identical Gram + axis map) computes a real value —
    // agreeing bit-for-bit with the direct InnerProductSpace geometry.
    #[test]
    fn distance_and_cosine_matching_basis_ok() {
        let a = distinct_observation_turtle("A", 1, 4, 7, 4); // G off-diag 1/4, (0.7, 0.4)
        let b = distinct_observation_turtle("B", 1, 4, 4, 7); // same G, (0.4, 0.7)
        let graph = two_observation_graph(&a, &b);
        let (distance, cosine) =
            distance_and_cosine(&graph, &obs_iri("A"), &obs_iri("B")).expect("matching basis");
        // Pin to the direct-space computation over the shared correlated metric.
        let space = correlated_gram();
        let x = [r(7, 10), r(2, 5)];
        let y = [r(2, 5), r(7, 10)];
        assert_eq!(distance, space.distance(&x, &y).unwrap());
        assert_eq!(cosine, space.cosine(&x, &y).unwrap());
        // Deterministic: same call twice → identical strings.
        let again =
            distance_and_cosine(&graph, &obs_iri("A"), &obs_iri("B")).expect("matching basis");
        assert_eq!((distance, cosine), again);
    }

    // Different Gram matrices between the two observations is a hard fail — never
    // a silently zero-padded/truncated meaningless number.
    #[test]
    fn distance_and_cosine_mismatched_gram_hard_fails() {
        let a = distinct_observation_turtle("A", 1, 4, 7, 4); // G off-diag 1/4
        let b = distinct_observation_turtle("B", 0, 1, 4, 7); // G off-diag 0 → different metric
        let graph = two_observation_graph(&a, &b);
        let err = distance_and_cosine(&graph, &obs_iri("A"), &obs_iri("B"))
            .expect_err("mismatched Gram must hard fail");
        assert!(err.contains("metric basis"), "{err}");
        assert!(err.contains("Gram matrix / axis map"), "{err}");
    }
}
