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
//! caller supplies a lookup of borrowed native RDF values or reference-oracle
//! surfaces and interprets [`BuiltinOutcome`] in its own control flow. Native
//! bindings never pass through RDF text; both inputs share one value algebra.
//!
//! Values flow through an exact-numeric **value tower** ([`Value`]): dimensionless
//! machine integers ([`Value::Int`]) and exact rationals ([`Value::Rat`]), the SI
//! dimension vector ([`Value::Dim`]) and dimensioned rationals ([`Value::Quantity`]).
//! Integer evaluation is over checked `i64`: truncating integer division `//`
//! ([`ArithOp::Div`]) rounds toward zero (ISO `//`, matching the captured SLD
//! semantics), and `=:=` is numeric value equality, never structural unification.
//! Exact rational division `/` ([`ArithOp::ExactDiv`]) and any operand that resolves
//! to a [`Value::Rat`] route to the shared exact-ℚ core (`gmeow_math::Rational`'s
//! `checked_add`/`checked_sub`/`checked_mul`/`checked_div`) and commit a
//! [`Value::Rat`]; two integers under `+ - * //`
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
use gmeow_math::dimension::{BASE_DIMENSION_COUNT, DimVector, base_dimension_index};
use gmeow_math::{InnerProductSpace, Rational, bounded_index};
use std::borrow::Cow;

mod input;
use input::{Binding, resolve_iri_operand, resolve_operand};

/// Namespace root of the `math:` measure-and-form vocabulary — the owner of the
/// Gram/vector cell predicates the bilinear builtin reads (never re-authored here).
const MATH: &str = "https://blackcatinformatics.ca/math/";

/// The `math:` cell predicates walked to load a `math:GramMatrix` and its
/// `math:RationalValue` entries: `hasEntry` → (`atRow`, `atColumn`, `entryValue`) →
/// (`numerator`, `denominator`).
const MATH_HAS_ENTRY: &str = "https://blackcatinformatics.ca/math/hasEntry";
const MATH_AT_ROW: &str = "https://blackcatinformatics.ca/math/atRow";
const MATH_AT_COLUMN: &str = "https://blackcatinformatics.ca/math/atColumn";
const MATH_ENTRY_VALUE: &str = "https://blackcatinformatics.ca/math/entryValue";
/// The `math:` cell predicates walked to load a `math:` vector: `hasComponent` →
/// (`atIndex`, `componentValue`) → (`numerator`, `denominator`).
const MATH_HAS_COMPONENT: &str = "https://blackcatinformatics.ca/math/hasComponent";
const MATH_AT_INDEX: &str = "https://blackcatinformatics.ca/math/atIndex";
const MATH_COMPONENT_VALUE: &str = "https://blackcatinformatics.ca/math/componentValue";
const MATH_NUMERATOR: &str = "https://blackcatinformatics.ca/math/numerator";
const MATH_DENOMINATOR: &str = "https://blackcatinformatics.ca/math/denominator";

/// Every `math:` cell predicate a bilinear-form squared-distance evaluation reads.
///
/// The demand/magic backward leg builds its selective world probe from body atoms;
/// the Gram/vector cell predicates appear in no body atom, so the magic source plan
/// probes exactly these predicates when a `BilinearSqDist` builtin is present, so the
/// cell facts reach the columnar store the moded evaluator's resolver reads.
pub(crate) const MATH_CELL_PREDICATES: &[&str] = &[
    MATH_HAS_ENTRY,
    MATH_AT_ROW,
    MATH_AT_COLUMN,
    MATH_ENTRY_VALUE,
    MATH_HAS_COMPONENT,
    MATH_AT_INDEX,
    MATH_COMPONENT_VALUE,
    MATH_NUMERATOR,
    MATH_DENOMINATOR,
];

/// The `rdf:type` predicate — read generically through [`MathTriples::math_iri_objects`]
/// to decide a dimension node's class membership (no dedicated type-probe method is
/// needed on the trait).
const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
/// The `math:` cell predicates [`load_dimension_cells`] walks to derive a dimension
/// IRI's exact ℚ⁷ exponent vector: `math:baseDimensionExponent` → (`exponentOfDimension`,
/// `exponentNumerator`, `exponentDenominator`), plus the `math:Dimensionless` /
/// `math:DerivedDimension` class markers the algorithm branches on.
const MATH_DIMENSIONLESS_INDIVIDUAL: &str = "https://blackcatinformatics.ca/math/dimensionless";
const MATH_DIMENSIONLESS_CLASS: &str = "https://blackcatinformatics.ca/math/Dimensionless";
const MATH_DERIVED_DIMENSION_CLASS: &str = "https://blackcatinformatics.ca/math/DerivedDimension";
const MATH_BASE_DIMENSION_EXPONENT: &str =
    "https://blackcatinformatics.ca/math/baseDimensionExponent";
const MATH_EXPONENT_OF_DIMENSION: &str = "https://blackcatinformatics.ca/math/exponentOfDimension";
const MATH_EXPONENT_NUMERATOR: &str = "https://blackcatinformatics.ca/math/exponentNumerator";
const MATH_EXPONENT_DENOMINATOR: &str = "https://blackcatinformatics.ca/math/exponentDenominator";

/// Every `math:` predicate [`load_dimension_cells`] reads, beyond `rdf:type` — the
/// dimension-gate twin of [`MATH_CELL_PREDICATES`] for the magic/demand source plan.
pub(crate) const MATH_DIMENSION_PREDICATES: &[&str] = &[
    MATH_BASE_DIMENSION_EXPONENT,
    MATH_EXPONENT_OF_DIMENSION,
    MATH_EXPONENT_NUMERATOR,
    MATH_EXPONENT_DENOMINATOR,
];

/// Statement extensions a builtin reads outside its explicit operands. Adding
/// cells can change a value or its definedness, requiring completed predecessors.
/// Type lookups observe only the two dimension classes used by the decoder;
/// unrelated type markers cannot change its result.
pub(super) fn read_patterns(
    builtin: &crate::query_ir::QBuiltin,
) -> impl Iterator<Item = (&'static str, Option<&'static str>)> {
    use crate::query_ir::QBuiltin;
    let (predicates, reads_types): (&[&str], bool) = match builtin {
        QBuiltin::Is { .. } | QBuiltin::Compare { .. } => (&[], false),
        QBuiltin::BilinearSqDist { .. } => (MATH_CELL_PREDICATES, false),
        QBuiltin::DimEqual { .. } | QBuiltin::DimProduct { .. } => {
            (MATH_DIMENSION_PREDICATES, true)
        }
    };
    predicates
        .iter()
        .copied()
        .map(|predicate| (predicate, None))
        .chain(
            reads_types
                .then_some([
                    (RDF_TYPE, Some(MATH_DIMENSIONLESS_CLASS)),
                    (RDF_TYPE, Some(MATH_DERIVED_DIMENSION_CLASS)),
                ])
                .into_iter()
                .flatten(),
        )
}

/// Re-badge a `gmeow_math` overflow / domain [`gmeow_errors::Diag`] as the tower's
/// [`BuiltinError::Overflow`] class.
///
/// The shared exact-ℚ core hard-fails an `i128` overflow (and an `i128::MIN`
/// magnitude) with a `Diag`; every domain gap that must stay *distinct* — a zero
/// divisor, a zero denominator — is guarded at the builtin layer BEFORE the kernel
/// call, so any `Diag` that reaches this mapper is an honest numeric overflow.
fn overflow(_: gmeow_errors::Diag) -> BuiltinError {
    BuiltinError::Overflow
}

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

/// The exact-numeric value tower carried between native engines.
///
/// [`Value::Int`] and [`Value::Rat`] are the dimensionless fast / exact variants.
/// [`Value::Dim`] is the shared ℚ⁷ SI exponent vector ([`DimVector`], in fixed SI
/// base-quantity order) and [`Value::Quantity`] is a dimensioned rational (scalar
/// magnitude paired with its dimension vector). Every stored [`Rational`] and
/// [`DimVector`] is gcd-normalized / canonical, so the derived `Eq` / `Hash` are
/// canonical.
///
/// The shared ℚ⁷ [`DimVector`] is seven `i128` rationals (≈224 bytes), so the two
/// dimensioned variants carry it behind a `Box`: this keeps the common `Int`/`Rat`
/// path — and the [`BuiltinOutcome`] this value is returned inside on every builtin
/// eval — small and cheap to move, pushing the rare dimensioned payload behind one
/// pointer. `Box` derives `Eq`/`Hash` through its contents, so equality stays exact.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) enum Value {
    /// A dimensionless machine integer (the integer fast path).
    Int(i64),
    /// A dimensionless exact rational.
    Rat(Rational),
    /// An SI dimension vector: the shared ℚ⁷ exponent vector.
    Dim(Box<DimVector>),
    /// A dimensioned rational: a scalar magnitude with its dimension vector.
    Quantity(Rational, Box<DimVector>),
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
    /// A metric-form (bilinear squared distance) evaluation failed on the form
    /// itself: a Gram matrix or coordinate vector that is absent, has no cells, is
    /// malformed (missing / non-integer index or rational component), is non-square,
    /// or an overflow in the exact inner product. Anchors
    /// `math:NonPositiveDefiniteNorm` / a malformed-metric class. (Positive-
    /// definiteness itself is certified once, off-gate, by the `math:` reasoned-graph
    /// gate; the runtime builtin trusts that certificate and only reports a structural
    /// or arithmetic failure of the form.)
    MetricForm,
}

impl BuiltinError {
    /// The stable kebab code suffix identifying this domain-fault kind — the single
    /// source of the kind→identity mapping the ledger keys on
    /// (`reason.builtin-gap.{suffix}`). Total over every arm, so no fault is
    /// anonymous.
    #[must_use]
    pub(crate) fn code_suffix(&self) -> &'static str {
        match self {
            BuiltinError::ZeroDivisor => "zero-divisor",
            BuiltinError::Overflow => "overflow",
            BuiltinError::ZeroDenominator => "zero-denominator",
            BuiltinError::DimensionMismatch => "dimension-mismatch",
            BuiltinError::MalformedDimension => "malformed-dimension",
            BuiltinError::MetricForm => "metric-form",
        }
    }

    /// The bare `math:` conformance-failure class IRI this fault anchors — the same
    /// class each arm's doc comment names. Total, so every fault carries an
    /// ontology anchor (never a bare Rust side-channel).
    #[must_use]
    pub(crate) fn math_class(&self) -> &'static str {
        match self {
            BuiltinError::ZeroDivisor => "https://blackcatinformatics.ca/math/ZeroDivisor",
            BuiltinError::Overflow => "https://blackcatinformatics.ca/math/Overflow",
            BuiltinError::ZeroDenominator => "https://blackcatinformatics.ca/math/ZeroDenominator",
            BuiltinError::DimensionMismatch => {
                "https://blackcatinformatics.ca/math/DimensionalInhomogeneity"
            }
            BuiltinError::MalformedDimension => {
                "https://blackcatinformatics.ca/math/MalformedDimension"
            }
            BuiltinError::MetricForm => {
                "https://blackcatinformatics.ca/math/NonPositiveDefiniteNorm"
            }
        }
    }
}

/// Why a moded builtin declined to produce a value at a candidate solution — the
/// distinction the seminaive collapse site MUST preserve.
///
/// [`BuiltinGapKind::Unbound`] is a *mode* gap (an operand needed for evaluation is
/// still free — the evaluator declines rather than guess); [`BuiltinGapKind::Error`]
/// is a *domain* fault (÷0, overflow, incommensurable dimensions — a typed
/// `math:` conformance failure). Collapsing the two would lose the reason a program
/// was refused, so they stay disjoint all the way to the ledger.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum BuiltinGapKind {
    /// An operand was unbound in its required mode.
    Unbound,
    /// A domain / precision / dimensional fault (a typed `math:` failure class).
    Error(BuiltinError),
}

impl BuiltinGapKind {
    /// The stable kebab code suffix identifying this gap kind
    /// (`reason.builtin-gap.{suffix}`). `Unbound` is `"unbound"`; an `Error`
    /// delegates to [`BuiltinError::code_suffix`], so each domain fault keeps its
    /// distinct per-kind identity.
    #[must_use]
    pub(crate) fn code_suffix_or_unbound(&self) -> &'static str {
        match self {
            BuiltinGapKind::Unbound => "unbound",
            BuiltinGapKind::Error(e) => e.code_suffix(),
        }
    }

    /// The bare `math:` conformance-failure class IRI this gap anchors, or `None`
    /// for a pure mode gap (`Unbound` names no domain-failure class).
    #[must_use]
    pub(crate) fn math_class(&self) -> Option<&'static str> {
        match self {
            BuiltinGapKind::Unbound => None,
            BuiltinGapKind::Error(e) => Some(e.math_class()),
        }
    }
}

/// One ledgerable moded-builtin gap captured at the seminaive collapse site: the
/// KIND (mode gap vs. typed domain fault), the rendered builtin operation, and the
/// antecedent bindings under which it arose.
///
/// This is the structured payload the old payload-less `gap: &mut bool` channel
/// destroyed. It threads the whole seminaive spine so a refused program's terminal
/// can mint a ledgered finding naming the `math:` class, the op, and the operands —
/// never an anonymous "arithmetic gap".
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct BuiltinGap {
    /// Why the builtin declined (mode gap or typed domain fault).
    pub(crate) kind: BuiltinGapKind,
    /// The rendered builtin operation (e.g. `X is 1 // 0`, `A > B`,
    /// `D is bilinearSqDist(G, X, Y)`).
    pub(crate) op: String,
    /// The antecedent solution bindings `(var, surface)` in effect when the gap
    /// arose — the operands the finding's message and antecedent key carry.
    pub(crate) bindings: Vec<(String, String)>,
}

impl BuiltinGap {
    /// Capture a gap from a builtin outcome that declined, rendering the op and
    /// snapshotting the antecedent bindings. Returns `None` for a producing outcome
    /// (`Filter`/`Generate`), so the caller maps only the declining arms.
    #[must_use]
    pub(crate) fn from_outcome(
        builtin: &QBuiltin,
        outcome: &BuiltinOutcome,
        bindings: Vec<(String, String)>,
    ) -> Option<Self> {
        let kind = match outcome {
            BuiltinOutcome::Unbound => BuiltinGapKind::Unbound,
            BuiltinOutcome::Error(e) => BuiltinGapKind::Error(*e),
            BuiltinOutcome::Filter(_) | BuiltinOutcome::Generate { .. } => return None,
        };
        Some(BuiltinGap {
            kind,
            op: render_builtin_op(builtin),
            bindings,
        })
    }
}

/// Render a moded builtin to a stable human operation string for a gap message /
/// focus key: `target is lhs op rhs`, `lhs cmp rhs`, or
/// `target is bilinearSqDist(gram, x, y)`.
#[must_use]
pub(crate) fn render_builtin_op(builtin: &QBuiltin) -> String {
    match builtin {
        QBuiltin::Is {
            target,
            lhs,
            op,
            rhs,
        } => format!(
            "{} is {} {} {}",
            render_term(target),
            render_term(lhs),
            op.token(),
            render_term(rhs)
        ),
        QBuiltin::Compare { lhs, op, rhs } => {
            format!("{} {} {}", render_term(lhs), op.token(), render_term(rhs))
        }
        QBuiltin::BilinearSqDist { target, gram, x, y } => format!(
            "{} is bilinearSqDist({}, {}, {})",
            render_term(target),
            render_term(gram),
            render_term(x),
            render_term(y)
        ),
        QBuiltin::DimEqual { d1, d2 } => {
            format!("dimEqual({}, {})", render_term(d1), render_term(d2))
        }
        QBuiltin::DimProduct { d_f, d_m, d_r } => format!(
            "dimProduct({}, {}, {})",
            render_term(d_f),
            render_term(d_m),
            render_term(d_r)
        ),
    }
}

/// Render one operand term for [`render_builtin_op`] — a stable, allocation-light
/// surface for a diagnostic message (never re-parsed).
fn render_term(term: &QTerm) -> Cow<'static, str> {
    match term {
        QTerm::Num(n) => Cow::Owned(n.to_string()),
        QTerm::Var(v) => Cow::Owned(v.clone()),
        QTerm::Const(c) => Cow::Owned(c.clone()),
        QTerm::Struct(_) => Cow::Borrowed("<struct>"),
        QTerm::Triple { .. } => Cow::Borrowed("<triple>"),
    }
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
    format!("{}/{}", r.numerator(), r.denominator())
}

/// Render a seven-dimension SI exponent vector as its transport lexical form
/// `n0/d0,n1/d1,…,n6/d6` (fixed SI base order).
fn emit_dimension_lex(dim: &DimVector) -> String {
    let mut out = String::new();
    for i in 0..BASE_DIMENSION_COUNT {
        if i > 0 {
            out.push(',');
        }
        // `i < BASE_DIMENSION_COUNT`, so `component` is always in range; the zero
        // fallback is unreachable and merely keeps the render total (no panic).
        let r = dim.component(i).unwrap_or_else(|_| Rational::zero());
        out.push_str(&emit_rational_lex(&r));
    }
    out
}

/// Emit a [`Value`] to its bound surface for transport between native engines.
///
/// [`Value::Int`] uses the canonical `xsd:integer` surface (byte-identical to
/// [`emit_integer_surface`]); the rational / dimension / quantity variants use
/// their pinned engine-internal transport tags.
pub(crate) fn emit_surface(value: &Value) -> String {
    crate::provenance::term_display(&emit_term(value))
}

/// Carry a computed value into native rule bindings without an RDF text round trip.
/// Exact rational, dimension and quantity values retain their shared transport tags.
pub(crate) fn emit_term(value: &Value) -> purrdf::TermValue {
    let (lexical_form, datatype) = match value {
        Value::Int(n) => (n.to_string(), XSD_INTEGER),
        Value::Rat(r) => (emit_rational_lex(r), XSD_RATIONAL_TRANSPORT),
        Value::Dim(dim) => (emit_dimension_lex(dim), XSD_DIMENSION_TRANSPORT),
        Value::Quantity(scalar, dim) => (
            format!("{};{}", emit_rational_lex(scalar), emit_dimension_lex(dim)),
            XSD_QUANTITY_TRANSPORT,
        ),
    };
    purrdf::TermValue::Literal {
        lexical_form,
        datatype: datatype.to_owned(),
        language: None,
        direction: None,
    }
}

/// Parse a rational transport lexical form `num/den` back to a [`Rational`].
///
/// Returns `None` for a surface that is not a well-formed rational transport (a
/// domain value that is not one of ours); a well-formed but numerically invalid
/// form (zero denominator / overflow) also declines rather than fabricating.
fn parse_rational_lex(lex: &str) -> Option<Rational> {
    let (num, den) = lex.split_once('/')?;
    let num = num.parse::<i128>().ok()?;
    let den = den.parse::<i128>().ok()?;
    // `Rational::new` hard-fails a zero denominator (or an `i128::MIN` component),
    // so a numerically invalid transport declines to `None` — never a panic.
    Rational::new(num, den).ok()
}

/// Parse a dimension transport lexical form (seven comma-separated exponents).
///
/// A wrong arity or an unparsable exponent is [`BuiltinError::MalformedDimension`]
/// — the transport is engine-internal, so a form carrying our dimension tag that
/// does not decode is corruption, not an ordinary domain value.
fn parse_dimension_lex(lex: &str) -> Result<DimVector, BuiltinError> {
    let mut exponents = lex.split(',');
    let mut dim = DimVector::zero();
    for i in 0..BASE_DIMENSION_COUNT {
        let token = exponents.next().ok_or(BuiltinError::MalformedDimension)?;
        let exponent = parse_rational_lex(token).ok_or(BuiltinError::MalformedDimension)?;
        // `dim` starts at zero, so this sets slot `i` to `exponent`; `i` is in range
        // and `0 + exponent` cannot overflow.
        dim.add_exponent(i, exponent).map_err(overflow)?;
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
    Ok(Value::Quantity(scalar, Box::new(dim)))
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
    parse_typed_value(lex, datatype)
}

/// Decode a literal value using its existing components, without RDF rendering.
fn parse_typed_value(lex: &str, datatype: &str) -> Option<Value> {
    match datatype {
        XSD_INTEGER => lex.parse::<i64>().ok().map(Value::Int),
        XSD_RATIONAL_TRANSPORT => parse_rational_lex(lex).map(Value::Rat),
        XSD_DIMENSION_TRANSPORT => parse_dimension_lex(lex)
            .ok()
            .map(|dim| Value::Dim(Box::new(dim))),
        XSD_QUANTITY_TRANSPORT => parse_quantity_lex(lex).ok(),
        _ => None,
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
        Value::Int(n) => Some(Rational::from_i128(i128::from(*n)).map_err(overflow)),
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
        ArithOp::Add => lhs.checked_add(rhs).map_err(overflow),
        ArithOp::Sub => lhs.checked_sub(rhs).map_err(overflow),
        ArithOp::Mul => lhs.checked_mul(rhs).map_err(overflow),
        ArithOp::ExactDiv => {
            // Guard the zero divisor at the builtin layer so the typed gap stays
            // distinct from a plain overflow.
            if rhs.is_zero() {
                return Err(BuiltinError::ZeroDivisor);
            }
            lhs.checked_div(rhs).map_err(overflow)
        }
        ArithOp::Div => {
            // Truncating integer division over ℚ: exact quotient, then round toward
            // zero. A normalized rational never has an `i128::MIN` numerator and its
            // denominator is `> 0`, so `numerator / denominator` truncates toward
            // zero and cannot overflow.
            if rhs.is_zero() {
                return Err(BuiltinError::ZeroDivisor);
            }
            let quotient = lhs.checked_div(rhs).map_err(overflow)?;
            let truncated = quotient.numerator() / quotient.denominator();
            Rational::new(truncated, 1).map_err(overflow)
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
/// The sign of `lhs − rhs` decides every ordering. It is computed via the shared
/// core's checked subtraction, so an `i128` overflow in the difference is a first-
/// class [`BuiltinError::Overflow`] (a declared gap the caller threads), never the
/// panicking `Rational` `Ord` cross-multiply.
fn apply_compare_q(lhs: &Rational, op: CmpOp, rhs: &Rational) -> Result<bool, BuiltinError> {
    let diff = lhs.checked_sub(*rhs).map_err(overflow)?;
    let is_zero = diff.is_zero();
    // `is_non_positive` is `<= 0`; strictly-negative is that minus the zero case.
    let is_neg = diff.is_non_positive() && !is_zero;
    Ok(match op {
        CmpOp::Gt => !is_zero && !is_neg,
        CmpOp::Lt => is_neg,
        CmpOp::Ge => !is_neg,
        CmpOp::Le => is_neg || is_zero,
        CmpOp::Eq => is_zero,
    })
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

/// Dimension-on-dimension arithmetic: `Mul` composes (exponent addition, the
/// `math:integralDimensionCompositionLaw` dimProduct, [`DimVector::add`]),
/// `Div`/`ExactDiv` quotients (exponent subtraction, [`DimVector::sub`]). `Add`/`Sub`
/// are undefined on bare dimensions — dimensions form a module under composition,
/// which is multiplicative; there is no additive operation on the exponent vectors
/// themselves — so they are a declared gap. An exponent overflow in the shared ℚ⁷
/// algebra is [`BuiltinError::Overflow`], never a wraparound.
fn dim_dim(a: &DimVector, b: &DimVector, op: ArithOp) -> Computed {
    match op {
        ArithOp::Mul => match a.add(b) {
            Ok(d) => Computed::Value(Value::Dim(Box::new(d))),
            Err(e) => Computed::Error(overflow(e)),
        },
        ArithOp::ExactDiv | ArithOp::Div => match a.sub(b) {
            Ok(d) => Computed::Value(Value::Dim(Box::new(d))),
            Err(e) => Computed::Error(overflow(e)),
        },
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
    d1: &DimVector,
    m2: &Rational,
    d2: &DimVector,
    op: ArithOp,
) -> Computed {
    match op {
        ArithOp::Add | ArithOp::Sub => {
            if !d1.commensurable(d2) {
                return Computed::Error(BuiltinError::DimensionMismatch);
            }
            let mag = if matches!(op, ArithOp::Add) {
                m1.checked_add(*m2)
            } else {
                m1.checked_sub(*m2)
            };
            match mag {
                Ok(m) => Computed::Value(Value::Quantity(m, Box::new(*d1))),
                Err(e) => Computed::Error(overflow(e)),
            }
        }
        ArithOp::Mul => {
            let mag = match m1.checked_mul(*m2) {
                Ok(m) => m,
                Err(e) => return Computed::Error(overflow(e)),
            };
            match d1.add(d2) {
                Ok(d) => Computed::Value(Value::Quantity(mag, Box::new(d))),
                Err(e) => Computed::Error(overflow(e)),
            }
        }
        ArithOp::ExactDiv | ArithOp::Div => {
            // Guard the zero divisor so it stays a distinct typed gap (a dimensioned
            // magnitude has no truncating meaning — always exact ℚ division).
            if m2.is_zero() {
                return Computed::Error(BuiltinError::ZeroDivisor);
            }
            let mag = match m1.checked_div(*m2) {
                Ok(m) => m,
                Err(e) => return Computed::Error(overflow(e)),
            };
            match d1.sub(d2) {
                Ok(d) => Computed::Value(Value::Quantity(mag, Box::new(d))),
                Err(e) => Computed::Error(overflow(e)),
            }
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
            Some(Ok(m2)) => quantity_arith(m1, d1, &m2, &DimVector::zero(), op),
            Some(Err(e)) => Computed::Error(e),
            None => Computed::Gap,
        },
        (Value::Int(_) | Value::Rat(_), Value::Quantity(m2, d2)) => match scalar_rational(lv) {
            Some(Ok(m1)) => quantity_arith(&m1, &DimVector::zero(), m2, d2, op),
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
    d1: &DimVector,
    m2: &Rational,
    d2: &DimVector,
    op: CmpOp,
) -> CompareResult {
    if !d1.commensurable(d2) {
        return CompareResult::Error(BuiltinError::DimensionMismatch);
    }
    match apply_compare_q(m1, op, m2) {
        Ok(b) => CompareResult::Filter(b),
        Err(e) => CompareResult::Error(e),
    }
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
            match apply_compare_q(&l, op, &r) {
                Ok(b) => CompareResult::Filter(b),
                Err(e) => CompareResult::Error(e),
            }
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
            CmpOp::Eq => CompareResult::Filter(a.commensurable(b)),
            // Dimensions carry no ordering — only commensurability (`=:=`).
            CmpOp::Gt | CmpOp::Lt | CmpOp::Ge | CmpOp::Le => CompareResult::Gap,
        },
        (Value::Quantity(m1, d1), Value::Quantity(m2, d2)) => quantity_compare(m1, d1, m2, d2, op),
        (Value::Quantity(m1, d1), Value::Int(_) | Value::Rat(_)) => match scalar_rational(rv) {
            Some(Ok(m2)) => quantity_compare(m1, d1, &m2, &DimVector::zero(), op),
            Some(Err(e)) => CompareResult::Error(e),
            None => CompareResult::Gap,
        },
        (Value::Int(_) | Value::Rat(_), Value::Quantity(m2, d2)) => match scalar_rational(lv) {
            Some(Ok(m1)) => quantity_compare(&m1, &DimVector::zero(), m2, d2, op),
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

/// Numeric equality of an `is`-target against a computed value, across the whole
/// tower.
///
/// `Value` derives *structural* `PartialEq`, under which `Value::Int(3)` and
/// `Value::Rat(3/1)` are distinct even though they denote the same rational — so a
/// raw `target == value` filter silently rejects `3 is 6 / 2` (the exact-ℚ kernel
/// commits `Value::Rat(3/1)` for any `/` result and never collapses a unit
/// denominator back to `Int`). That is a wrong boolean answer, not a gap. Route the
/// target-equality decision through [`compute_compare`] with [`CmpOp::Eq`], which is
/// the same ℚ-correct cross-type equality the `=:=` operator uses: a match is exactly
/// `CompareResult::Filter(true)`; a type/dimension mismatch (e.g. a metre target
/// against a second result) is a definite non-match — the target simply is not equal
/// to the value — so it is `false`, never an error propagated out of a filter.
fn numeric_eq(target: &Value, value: &Value) -> bool {
    matches!(
        compute_compare(target, value, CmpOp::Eq),
        CompareResult::Filter(true)
    )
}

// ── Metric-form (bilinear squared distance) evaluation ───────────────────────────

/// Store-agnostic access to the exact-rational `math:` Gram/vector cells a metric-form
/// builtin reads. The moded evaluator carries NO graph/store handle, so each native
/// engine supplies its own resolver over its own substrate (the forward/backward
/// columnar `RelationStore`, the reference oracle's `WorldFactSource`), and the shared
/// evaluator stays substrate-neutral.
///
/// Both methods return the fully-built form: a `None` means the operand IRI names no
/// well-formed cell set (absent, no cells, or a malformed/out-of-range index or
/// component), which the evaluator routes to [`BuiltinError::MetricForm`] — never a
/// wrong answer.
pub(crate) trait CellResolver {
    /// The exact-rational cells `(row, col, value)` of the `math:GramMatrix` named
    /// `iri` (bare IRI, no angle brackets), or `None` when it names no well-formed
    /// matrix.
    fn gram(&self, iri: &str) -> Option<Vec<(usize, usize, Rational)>>;
    /// The dense, zero-completed exact-rational coordinate vector of the `math:`
    /// vector named `iri` (bare IRI), or `None` when it names no well-formed vector.
    fn vector(&self, iri: &str) -> Option<Vec<Rational>>;
    /// The exact ℚ⁷ SI dimension exponent vector of the dimension named `iri` (bare
    /// IRI), or `None` when it names no resolvable dimension (undimensioned,
    /// malformed, or not a dimension node at all) — the substrate the `math:`
    /// dimension-gate builtins ([`QBuiltin::DimEqual`](crate::query_ir::QBuiltin::DimEqual) /
    /// [`QBuiltin::DimProduct`](crate::query_ir::QBuiltin::DimProduct)) probe.
    ///
    /// Defaults to `None`: these builtins are LOWERING-ONLY, emitted solely by the
    /// `logic:Constraint` violation-rule lowering that runs through the forward
    /// semi-naive engine's own [`crate::physical::seminaive`] resolver — every other
    /// substrate (the declarative reference oracle, the in-memory bilinear-distance
    /// test doubles) never evaluates them, so it never needs to override this.
    fn dimension(&self, _iri: &str) -> Option<DimVector> {
        None
    }
}

/// The zero-capability resolver: names no cells. Scalar-only (`Is`/`Compare`) callers
/// and unit tests that never exercise a metric-form builtin pass this, so `eval`'s
/// resolver parameter is always present (never an `Option` a caller must special-case).
pub(crate) struct NoCellResolver;

impl CellResolver for NoCellResolver {
    fn gram(&self, _iri: &str) -> Option<Vec<(usize, usize, Rational)>> {
        None
    }
    fn vector(&self, _iri: &str) -> Option<Vec<Rational>> {
        None
    }
}

/// Store-agnostic read of the `math:` cell triples, the substrate a [`CellResolver`]
/// implementation walks. Each engine implements this over its own store; the shared
/// [`load_gram_cells`] / [`load_vector_dense`] loaders below then build the form from
/// it, so the cell-walk chain lives in ONE place regardless of substrate.
pub(crate) trait MathTriples {
    /// The bare IRIs (no angle brackets) that are objects of `(subject, predicate)`.
    fn math_iri_objects(&self, subject: &str, predicate: &str) -> Vec<String>;
    /// The first literal object of `(subject, predicate)` parsed as an `i128`, if any.
    fn math_literal_i128(&self, subject: &str, predicate: &str) -> Option<i128>;
}

/// Read the single IRI object of `(subject, predicate)`, requiring EXACTLY one.
///
/// A well-formed cell names one target for each of its functional object properties
/// (a dimension exponent's `math:exponentOfDimension` base, a Gram entry's
/// `math:entryValue`, a vector component's `math:componentValue`). Zero or MULTIPLE
/// targets is malformed, so this declines to `None` rather than silently taking the
/// first — which would decode a multi-valued cell to a possibly-wrong value and mask the
/// malformed-data signal the dimension gate exists to catch.
fn exactly_one_iri_object(src: &dyn MathTriples, subject: &str, predicate: &str) -> Option<String> {
    let mut targets = src.math_iri_objects(subject, predicate).into_iter();
    let only = targets.next()?;
    if targets.next().is_some() {
        return None;
    }
    Some(only)
}

/// Read one `math:RationalValue`'s `numerator`/`denominator` into a [`Rational`].
///
/// A missing property or an invalid (zero-denominator / overflowing) construction
/// declines to `None` — mapped by the caller to [`BuiltinError::MetricForm`].
fn load_rational_value(src: &dyn MathTriples, value_iri: &str) -> Option<Rational> {
    let num = src.math_literal_i128(value_iri, MATH_NUMERATOR)?;
    let den = src.math_literal_i128(value_iri, MATH_DENOMINATOR)?;
    Rational::new(num, den).ok()
}

/// Load the exact-rational cells of a `math:GramMatrix` — the store-native mirror of
/// `gmeow_math::load_gram`, over the substrate-neutral [`MathTriples`]. Every
/// `math:atRow`/`math:atColumn` is bounded to `[0, MAX_BASIS_DIM)` (via
/// [`gmeow_math::bounded_index`]), so an out-of-range index declines rather than sizing
/// a huge matrix. `None` on any absent/malformed cell.
pub(crate) fn load_gram_cells(
    src: &dyn MathTriples,
    gram_iri: &str,
) -> Option<Vec<(usize, usize, Rational)>> {
    let entries = src.math_iri_objects(gram_iri, MATH_HAS_ENTRY);
    if entries.is_empty() {
        return None;
    }
    let mut cells = Vec::with_capacity(entries.len());
    for entry in entries {
        let row = bounded_index(src.math_literal_i128(&entry, MATH_AT_ROW)?, "matrix row").ok()?;
        let col = bounded_index(
            src.math_literal_i128(&entry, MATH_AT_COLUMN)?,
            "matrix column",
        )
        .ok()?;
        let value_iri = exactly_one_iri_object(src, &entry, MATH_ENTRY_VALUE)?;
        cells.push((row, col, load_rational_value(src, &value_iri)?));
    }
    Some(cells)
}

/// Load a dense, zero-completed exact-rational coordinate vector — the store-native
/// mirror of `gmeow_math::load_vector`, over [`MathTriples`]. Sized to the maximum
/// declared `math:atIndex` + 1; each index bounded like the Gram loader. `None` on any
/// absent/malformed component.
pub(crate) fn load_vector_dense(src: &dyn MathTriples, vector_iri: &str) -> Option<Vec<Rational>> {
    let components = src.math_iri_objects(vector_iri, MATH_HAS_COMPONENT);
    if components.is_empty() {
        return None;
    }
    let mut cells: Vec<(usize, Rational)> = Vec::with_capacity(components.len());
    for component in components {
        let idx = bounded_index(
            src.math_literal_i128(&component, MATH_AT_INDEX)?,
            "vector index",
        )
        .ok()?;
        let value_iri = exactly_one_iri_object(src, &component, MATH_COMPONENT_VALUE)?;
        cells.push((idx, load_rational_value(src, &value_iri)?));
    }
    let dim = cells.iter().map(|(i, _)| *i).max().map(|m| m + 1)?;
    let mut vector = vec![Rational::zero(); dim];
    for (idx, value) in cells {
        vector[idx] = value;
    }
    Some(vector)
}

/// Load the exact ℚ⁷ SI dimension exponent vector of `dim_iri` — the store-native
/// mirror of [`gmeow_math::dimension::load_dimension_vector`], over the
/// substrate-neutral [`MathTriples`] (exactly as [`load_gram_cells`] /
/// [`load_vector_dense`] mirror `gmeow_math::load_gram`/`load_vector`): a base
/// dimension is a unit basis vector, `math:dimensionless` (or any `math:Dimensionless`
/// individual) is the zero vector, and a `math:DerivedDimension` sums `power · e_base`
/// over its `math:baseDimensionExponent` cells. Every structural fault — a node that is
/// not a recognizable dimension, a cell naming a non-base target, a missing
/// numerator/denominator, a zero-denominator power, or an exact-rational overflow —
/// declines to `None` rather than raising: the dimension-gate builtins route an
/// unresolved dimension to a declared mode gap, never a wrong answer (the malformed
/// case is separately caught by the retained native `math:MalformedDimension` scan).
pub(crate) fn load_dimension_cells(src: &dyn MathTriples, dim_iri: &str) -> Option<DimVector> {
    if let Some(v) = DimVector::base_unit(dim_iri) {
        return Some(v);
    }
    let types = src.math_iri_objects(dim_iri, RDF_TYPE);
    if dim_iri == MATH_DIMENSIONLESS_INDIVIDUAL
        || types.iter().any(|t| t == MATH_DIMENSIONLESS_CLASS)
    {
        return Some(DimVector::zero());
    }
    if !types.iter().any(|t| t == MATH_DERIVED_DIMENSION_CLASS) {
        return None;
    }
    let mut v = DimVector::zero();
    for cell in src.math_iri_objects(dim_iri, MATH_BASE_DIMENSION_EXPONENT) {
        let base = exactly_one_iri_object(src, &cell, MATH_EXPONENT_OF_DIMENSION)?;
        let base_index = base_dimension_index(&base)?;
        let num = src.math_literal_i128(&cell, MATH_EXPONENT_NUMERATOR)?;
        let den = src.math_literal_i128(&cell, MATH_EXPONENT_DENOMINATOR)?;
        let power = Rational::new(num, den).ok()?;
        v.add_exponent(base_index, power).ok()?;
    }
    Some(v)
}

/// Compute the exact bilinear-form squared distance `(x − y)ᵀ G (x − y)` in exact ℚ.
///
/// Builds the declared-symmetric dense Gram matrix from `cells` (mirroring the
/// `math_dimension` symmetric fill), forms the exact `x − y` difference, and evaluates
/// the quadratic form via the shared [`InnerProductSpace`]. Every failure — an absent
/// form, a length mismatch between `x` and `y`, a vector wider than the form, a
/// non-square Gram, or an exact-arithmetic overflow — is a typed
/// [`BuiltinError::MetricForm`] / [`BuiltinError::DimensionMismatch`], never a wrong
/// answer or panic. The result is the EXACT `Value::Rat` squared distance (√ stays a
/// downstream seam; squared order = distance order since √ is monotone).
fn compute_bilinear_sqdist(
    gram_iri: &str,
    x_iri: &str,
    y_iri: &str,
    resolver: &dyn CellResolver,
) -> Result<Value, BuiltinError> {
    let cells = resolver.gram(gram_iri).ok_or(BuiltinError::MetricForm)?;
    let x = resolver.vector(x_iri).ok_or(BuiltinError::MetricForm)?;
    let y = resolver.vector(y_iri).ok_or(BuiltinError::MetricForm)?;

    // The declared-symmetric dense fill: `dim` = max authored index + 1, each authored
    // cell mirrored across the diagonal (the `math:` reasoned gate certifies symmetry
    // and positive-definiteness once, off-gate; the runtime builtin trusts it).
    let dim = cells
        .iter()
        .flat_map(|(r, c, _)| [*r, *c])
        .max()
        .map(|m| m + 1)
        .ok_or(BuiltinError::MetricForm)?;
    let mut matrix = vec![vec![Rational::zero(); dim]; dim];
    for (row, col, value) in cells {
        matrix[row][col] = value;
        matrix[col][row] = value;
    }

    // `x` and `y` must denote coordinates of the SAME space to be subtracted, and
    // neither may exceed the form's order (the quadratic form would silently truncate
    // wider coordinates — a wrong answer). Both shorter-than-`dim` vectors are exact
    // zero-completed by the inner-product engine.
    if x.len() != y.len() || x.len() > dim {
        return Err(BuiltinError::DimensionMismatch);
    }
    let diff = x
        .iter()
        .zip(y.iter())
        .map(|(xi, yi)| xi.checked_sub(*yi))
        .collect::<Result<Vec<_>, _>>()
        .map_err(overflow)?;

    let space = InnerProductSpace::new(matrix).map_err(|_| BuiltinError::MetricForm)?;
    let sqdist = space
        .quadratic_form(&diff)
        .map_err(|_| BuiltinError::MetricForm)?;
    Ok(Value::Rat(sqdist))
}

/// Evaluate `builtin` against the current substitution, resolving variables via
/// `lookup` (variable name → bound surface, or `None` when unbound).
///
/// `resolver` supplies the exact-rational `math:` Gram/vector cells a metric-form
/// builtin ([`QBuiltin::BilinearSqDist`]) reads; scalar builtins (`Is`/`Compare`)
/// ignore it (pass [`NoCellResolver`]).
pub(crate) fn eval<'a>(
    builtin: &QBuiltin,
    lookup: &impl Fn(&str) -> Option<Cow<'a, str>>,
    resolver: &dyn CellResolver,
) -> BuiltinOutcome {
    eval_inputs(
        builtin,
        &|name| lookup(name).map(Binding::Surface),
        resolver,
    )
}

/// Evaluate against borrowed native RDF bindings. Successful execution never
/// renders or reparses a bound term. Datatype, language and direction determine
/// numeric admission; a numeric-looking IRI or text literal remains nonnumeric.
pub(crate) fn eval_native<'a>(
    builtin: &QBuiltin,
    lookup: &impl Fn(&str) -> Option<&'a purrdf::TermValue>,
    resolver: &dyn CellResolver,
) -> BuiltinOutcome {
    eval_inputs(builtin, &|name| lookup(name).map(Binding::Native), resolver)
}

/// Input representation affects decoding only. Binding modes, checked arithmetic,
/// dimension algebra, graph probes and failure semantics have one implementation.
fn eval_inputs<'a>(
    builtin: &'a QBuiltin,
    lookup: &impl Fn(&str) -> Option<Binding<'a>>,
    resolver: &dyn CellResolver,
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
            bind_computed(target, value, lookup)
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
        QBuiltin::BilinearSqDist { target, gram, x, y } => {
            // All three operand IRIs must be bound/ground to compute; an unbound or
            // non-IRI operand is a declared mode gap (decline, never guess).
            let (Some(gram_iri), Some(x_iri), Some(y_iri)) = (
                resolve_iri_operand(gram, lookup),
                resolve_iri_operand(x, lookup),
                resolve_iri_operand(y, lookup),
            ) else {
                return BuiltinOutcome::Unbound;
            };
            // The exact squared distance over exact ℚ; a missing/malformed form or a
            // length mismatch is a typed error, never a wrong answer.
            let value = match compute_bilinear_sqdist(&gram_iri, &x_iri, &y_iri, resolver) {
                Ok(value) => value,
                Err(e) => return BuiltinOutcome::Error(e),
            };
            bind_computed(target, value, lookup)
        }
        QBuiltin::DimEqual { d1, d2 } => {
            // Both dimension-IRI operands must resolve to a well-formed ℚ⁷ exponent
            // vector to decide the law; an unresolved (undimensioned/malformed)
            // operand is undefinedness, not a violation, so it declines to `Unbound`
            // — the constraint-tagged forward rule then simply does not fire.
            let (Some(d1_iri), Some(d2_iri)) = (
                resolve_iri_operand(d1, lookup),
                resolve_iri_operand(d2, lookup),
            ) else {
                return BuiltinOutcome::Unbound;
            };
            match (resolver.dimension(&d1_iri), resolver.dimension(&d2_iri)) {
                (Some(v1), Some(v2)) => BuiltinOutcome::Filter(v1.commensurable(&v2)),
                _ => BuiltinOutcome::Unbound,
            }
        }
        QBuiltin::DimProduct { d_f, d_m, d_r } => {
            let (Some(f_iri), Some(m_iri), Some(r_iri)) = (
                resolve_iri_operand(d_f, lookup),
                resolve_iri_operand(d_m, lookup),
                resolve_iri_operand(d_r, lookup),
            ) else {
                return BuiltinOutcome::Unbound;
            };
            let (Some(fv), Some(mv), Some(rv)) = (
                resolver.dimension(&f_iri),
                resolver.dimension(&m_iri),
                resolver.dimension(&r_iri),
            ) else {
                return BuiltinOutcome::Unbound;
            };
            // An exact-rational overflow composing dF ⊕ dM is undefinedness (per the
            // dimension-gate contract), never a raised domain error — decline to
            // `Unbound` exactly like an unresolved operand.
            match fv.add(&mv) {
                Ok(composed) => BuiltinOutcome::Filter(composed == rv),
                Err(_) => BuiltinOutcome::Unbound,
            }
        }
    }
}

/// An unbound target generates; a bound target filters by numeric equality.
/// Nonnumeric targets are false, while missing input operands remain mode gaps.
fn bind_computed<'a>(
    target: &QTerm,
    value: Value,
    lookup: &impl Fn(&str) -> Option<Binding<'a>>,
) -> BuiltinOutcome {
    match resolve_operand(target, lookup) {
        Operand::Bound(bound) => BuiltinOutcome::Filter(numeric_eq(&bound, &value)),
        Operand::Unbound => {
            let QTerm::Var(name) = target else {
                unreachable!("only a variable can be unbound")
            };
            BuiltinOutcome::Generate {
                var: name.clone(),
                value,
            }
        }
        Operand::NonNumeric => BuiltinOutcome::Filter(false),
    }
}

// ── Public production API: the native bilinear-form distance authority ──────────
//
// External crates (notably `gmeow-affect`'s nearest-prototype classifier) compute
// metric-space distances through this governed moded-builtin
// family — `eval` of a [`QBuiltin::BilinearSqDist`] — rather than a private exact-ℚ
// path, so the builtin dispatch is the SINGLE production distance authority (the
// maintainer routing mandate). The Gram is supplied EXPLICITLY per call, so the
// classification vantage metric is a first-class parameter, not baked into any one
// observation's declared profile.

/// A typed failure of the public bilinear-form distance API: a malformed/absent Gram
/// or coordinate vector, a dimension mismatch between the two vectors (or a vector
/// wider than the form), or an exact-rational arithmetic overflow. Never a wrong
/// answer or a panic.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BilinearFormError {
    /// The Gram or a coordinate vector was absent, empty, non-square, or malformed.
    MetricForm,
    /// The two coordinate vectors differ in length, or a vector exceeds the form's order.
    DimensionMismatch,
    /// Exact-rational arithmetic overflowed the machine integer.
    Overflow,
}

impl core::fmt::Display for BilinearFormError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(match self {
            BilinearFormError::MetricForm => "malformed or absent metric form (Gram/vector)",
            BilinearFormError::DimensionMismatch => "coordinate-vector dimension mismatch",
            BilinearFormError::Overflow => "exact-rational arithmetic overflow",
        })
    }
}

impl std::error::Error for BilinearFormError {}

impl From<BuiltinError> for BilinearFormError {
    fn from(e: BuiltinError) -> Self {
        match e {
            BuiltinError::DimensionMismatch => BilinearFormError::DimensionMismatch,
            BuiltinError::Overflow => BilinearFormError::Overflow,
            _ => BilinearFormError::MetricForm,
        }
    }
}

/// The exact-ℚ bilinear-form squared distance `(x − y)ᵀ G (x − y)`, computed THROUGH
/// the native moded-builtin dispatch (`eval` of a [`QBuiltin::BilinearSqDist`]) — the
/// single production distance authority for metric-space classification (the affect classifier).
///
/// `gram_cells` are the declared-symmetric `(row, col, value)` entries of the metric
/// Gram `G` (each authored cell is mirrored across the diagonal exactly as the builtin
/// does); `x` and `y` are dense exact-ℚ coordinate vectors in the SAME basis (a shorter
/// vector is exact zero-completed; a vector wider than the form is a
/// [`BilinearFormError::DimensionMismatch`], never a silent truncation). The result is
/// the EXACT squared distance — √ stays a downstream display seam, and squared order is
/// distance order since √ is monotone. Positive-definiteness of `G` is the CALLER's
/// certificate (the runtime builtin trusts it): a non-PD `G` can yield a negative form
/// value, so a caller that needs a metric MUST PD-certify `G` first.
pub fn bilinear_sqdist(
    gram_cells: &[(usize, usize, Rational)],
    x: &[Rational],
    y: &[Rational],
) -> Result<Rational, BilinearFormError> {
    struct MemCells<'a> {
        gram: &'a [(usize, usize, Rational)],
        x: &'a [Rational],
        y: &'a [Rational],
    }
    impl CellResolver for MemCells<'_> {
        fn gram(&self, iri: &str) -> Option<Vec<(usize, usize, Rational)>> {
            (iri == "urn:gmeow:bilinear:gram").then(|| self.gram.to_vec())
        }
        fn vector(&self, iri: &str) -> Option<Vec<Rational>> {
            match iri {
                "urn:gmeow:bilinear:x" => Some(self.x.to_vec()),
                "urn:gmeow:bilinear:y" => Some(self.y.to_vec()),
                _ => None,
            }
        }
    }

    let resolver = MemCells {
        gram: gram_cells,
        x,
        y,
    };
    let builtin = QBuiltin::BilinearSqDist {
        target: QTerm::Var("D".to_owned()),
        gram: QTerm::Const("<urn:gmeow:bilinear:gram>".to_owned()),
        x: QTerm::Const("<urn:gmeow:bilinear:x>".to_owned()),
        y: QTerm::Const("<urn:gmeow:bilinear:y>".to_owned()),
    };
    let lookup = |_: &str| -> Option<Cow<'static, str>> { None };
    match eval(&builtin, &lookup, &resolver) {
        BuiltinOutcome::Generate {
            value: Value::Rat(r),
            ..
        } => Ok(r),
        BuiltinOutcome::Error(e) => Err(e.into()),
        // A ground BilinearSqDist with an unbound target always generates a Rat or
        // errors; any other outcome means a malformed form.
        _ => Err(BilinearFormError::MetricForm),
    }
}

/// Exact-ℚ total ordering of two squared distances, routed through the native builtin family's
/// overflow-SAFE comparison (`apply_compare_q`) — never `Rational::cmp`, which panics
/// on i128 overflow. The single ordering primitive nearest-prototype ranking is built on.
pub fn compare_sqdist(
    a: &Rational,
    b: &Rational,
) -> Result<core::cmp::Ordering, BilinearFormError> {
    use core::cmp::Ordering;
    if apply_compare_q(a, CmpOp::Lt, b)? {
        Ok(Ordering::Less)
    } else if apply_compare_q(a, CmpOp::Gt, b)? {
        Ok(Ordering::Greater)
    } else {
        Ok(Ordering::Equal)
    }
}

#[path = "builtin_eval.tests.rs"]
#[cfg(test)]
mod tests;
