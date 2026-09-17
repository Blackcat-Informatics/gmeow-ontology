// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

/// Scalar-only test shim: the arithmetic/comparison unit tests never exercise a
/// metric-form builtin, so they route through the zero-capability resolver. This
/// two-argument `eval` shadows [`super::eval`] inside the test module only, so the
/// existing scalar tests read unchanged; the metric-form tests call
/// [`super::eval`] with an explicit resolver.
fn eval<'a>(builtin: &QBuiltin, lookup: &impl Fn(&str) -> Option<Cow<'a, str>>) -> BuiltinOutcome {
    super::eval(builtin, lookup, &NoCellResolver)
}

/// A canonical `<iri>`-surface constant operand from a bare IRI.
fn iri_const(iri: &str) -> QTerm {
    QTerm::Const(format!("<{iri}>"))
}

/// A `BilinearSqDist` builtin over the given operand terms.
fn bilinear(target: QTerm, gram: QTerm, x: QTerm, y: QTerm) -> QBuiltin {
    QBuiltin::BilinearSqDist { target, gram, x, y }
}

/// A metric-form [`CellResolver`] test double: canned Gram cells and named
/// coordinate vectors, keyed by bare IRI. Returns `None` for any unknown operand,
/// exactly as a store-backed resolver does for an absent form.
struct FakeCells {
    gram_iri: String,
    gram: Vec<(usize, usize, Rational)>,
    vectors: Vec<(String, Vec<Rational>)>,
}

impl CellResolver for FakeCells {
    fn gram(&self, iri: &str) -> Option<Vec<(usize, usize, Rational)>> {
        (iri == self.gram_iri).then(|| self.gram.clone())
    }
    fn vector(&self, iri: &str) -> Option<Vec<Rational>> {
        self.vectors
            .iter()
            .find(|(name, _)| name == iri)
            .map(|(_, v)| v.clone())
    }
}

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

/// A minimal in-memory [`MathTriples`] for the dimension-cell loader tests: flat
/// `(subject, predicate, iri-object)` and `(subject, predicate, i128-literal)`
/// lists, queried exactly as a store-backed resolver would be.
struct FakeTriples {
    iri: Vec<(String, String, String)>,
    lit: Vec<(String, String, i128)>,
}

impl FakeTriples {
    fn new() -> Self {
        Self {
            iri: Vec::new(),
            lit: Vec::new(),
        }
    }
    fn iri(mut self, s: &str, p: &str, o: &str) -> Self {
        self.iri.push((s.to_owned(), p.to_owned(), o.to_owned()));
        self
    }
    fn lit(mut self, s: &str, p: &str, o: i128) -> Self {
        self.lit.push((s.to_owned(), p.to_owned(), o));
        self
    }
}

impl MathTriples for FakeTriples {
    fn math_iri_objects(&self, subject: &str, predicate: &str) -> Vec<String> {
        self.iri
            .iter()
            .filter(|(s, p, _)| s == subject && p == predicate)
            .map(|(_, _, o)| o.clone())
            .collect()
    }
    fn math_literal_i128(&self, subject: &str, predicate: &str) -> Option<i128> {
        self.lit
            .iter()
            .find(|(s, p, _)| s == subject && p == predicate)
            .map(|(_, _, o)| *o)
    }
}

#[test]
fn dimension_cell_with_duplicate_exponent_of_dimension_declines_not_mis_decodes() {
    const DIM: &str = "https://blackcatinformatics.ca/math/customDim";
    const CELL: &str = "https://blackcatinformatics.ca/math/cell1";
    const LENGTH: &str = "https://blackcatinformatics.ca/math/lengthDimension";
    const MASS: &str = "https://blackcatinformatics.ca/math/massDimension";

    // Control: a well-formed single-target cell resolves (customDim = L¹).
    let ok = FakeTriples::new()
        .iri(DIM, RDF_TYPE, MATH_DERIVED_DIMENSION_CLASS)
        .iri(DIM, MATH_BASE_DIMENSION_EXPONENT, CELL)
        .iri(CELL, MATH_EXPONENT_OF_DIMENSION, LENGTH)
        .lit(CELL, MATH_EXPONENT_NUMERATOR, 1)
        .lit(CELL, MATH_EXPONENT_DENOMINATOR, 1);
    assert!(
        load_dimension_cells(&ok, DIM).is_some(),
        "a single-target exponent cell must resolve to a dimension vector"
    );

    // The SAME cell with a SECOND `math:exponentOfDimension` target is malformed:
    // the loader must DECLINE (None), never silently take the first target and decode
    // the dimension to a possibly-wrong ℚ⁷ vector — which would mask the
    // `math:MalformedDimension` signal the dimension gate depends on.
    let dup = FakeTriples::new()
        .iri(DIM, RDF_TYPE, MATH_DERIVED_DIMENSION_CLASS)
        .iri(DIM, MATH_BASE_DIMENSION_EXPONENT, CELL)
        .iri(CELL, MATH_EXPONENT_OF_DIMENSION, LENGTH)
        .iri(CELL, MATH_EXPONENT_OF_DIMENSION, MASS)
        .lit(CELL, MATH_EXPONENT_NUMERATOR, 1)
        .lit(CELL, MATH_EXPONENT_DENOMINATOR, 1);
    assert!(
        load_dimension_cells(&dup, DIM).is_none(),
        "a cell with two math:exponentOfDimension targets must decline, not mis-decode"
    );
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

#[test]
fn is_filter_cross_type_int_vs_exact_rational_is_equal() {
    // Regression: exact `/` commits `Value::Rat(3/1)`; a structural `==` against a
    // `Value::Int(3)` target would (wrongly) reject it. `3 is 6 / 2` MUST filter true
    // — an integer target equals the mathematically-equal exact-ℚ result.
    let lookup = env(&[]);
    assert_eq!(
        eval(
            &is(
                QTerm::Num(3),
                QTerm::Num(6),
                ArithOp::ExactDiv,
                QTerm::Num(2)
            ),
            &lookup
        ),
        BuiltinOutcome::Filter(true),
        "3 is 6 / 2 must be true across the Int/Rat type boundary"
    );
    // Negative control: a genuinely-different integer target still filters false.
    assert_eq!(
        eval(
            &is(
                QTerm::Num(4),
                QTerm::Num(6),
                ArithOp::ExactDiv,
                QTerm::Num(2)
            ),
            &lookup
        ),
        BuiltinOutcome::Filter(false),
        "4 is 6 / 2 must be false"
    );
    // A non-integral exact result is not equal to any integer target.
    assert_eq!(
        eval(
            &is(
                QTerm::Num(1),
                QTerm::Num(1),
                ArithOp::ExactDiv,
                QTerm::Num(2)
            ),
            &lookup
        ),
        BuiltinOutcome::Filter(false),
        "1 is 1 / 2 must be false (1 ≠ 1/2)"
    );
}

#[test]
fn is_filter_bound_var_surface_matches_exact_rational() {
    // `X is 6 / 2` with X bound to the integer surface "3" — the bound-variable
    // target path (parse_value_surface) must also see cross-type equality.
    let lookup = env(&[("X", "3")]);
    let b = is(var("X"), QTerm::Num(6), ArithOp::ExactDiv, QTerm::Num(2));
    assert_eq!(eval(&b, &lookup), BuiltinOutcome::Filter(true));
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

// ── Rational helper (over the shared gmeow_math exact-ℚ core) ────────────

/// Build a shared-core [`Rational`] from `i64` literals (widened to the
/// `i128` core). The gmeow_math crate owns the normalization / overflow /
/// zero-denominator unit coverage; here it is only a test convenience.
fn rat(num: i64, den: i64) -> Rational {
    Rational::new(i128::from(num), i128::from(den)).expect("well-formed rational")
}

// ── Value transport round-trip (every committable variant) ──────────────

#[test]
fn value_transport_round_trip_each_variant() {
    // L^1 · T^-2 · (4th base)^(3/2): exercises negative and fractional exponents.
    let dim = dim_of(&[(0, 1, 1), (2, -2, 1), (3, 3, 2)]);
    let cases = [
        Value::Int(42),
        Value::Int(-7),
        Value::Int(i64::MIN),
        Value::Rat(rat(3, 4)),
        Value::Rat(rat(-1, 2)),
        Value::Dim(Box::new(dim)),
        Value::Quantity(rat(5, 3), Box::new(dim)),
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
    // (i128::MAX/1) * (i128::MAX/1) overflows the i128 numerator on the ℚ path —
    // a hard-fail Overflow, never a silent wraparound. (An i64·i64 product now
    // fits in the widened i128 core, so overflow requires near-i128::MAX operands
    // carried through the rational transport.)
    let big = format!("\"{}/1\"^^<{XSD_RATIONAL_TRANSPORT}>", i128::MAX);
    let lookup = env(&[("A", &big), ("B", &big)]);
    let b = is(var("X"), var("A"), ArithOp::Mul, var("B"));
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

// ── Dimension algebra (ℚ⁷ module) & dimensioned-quantity calculus ────────

/// Build a shared-core [`DimVector`] from `(index, num, den)` exponents;
/// unmentioned bases are the zero exponent.
fn dim_of(pairs: &[(usize, i64, i64)]) -> DimVector {
    let mut d = DimVector::zero();
    for (i, n, den) in pairs {
        d.add_exponent(*i, rat(*n, *den))
            .expect("in-range base-dimension index");
    }
    d
}

/// The base-dimension vector indices (fixed SI order).
const LEN: usize = 0;
const TIME: usize = 2;

fn dim_surface(d: &DimVector) -> String {
    emit_surface(&Value::Dim(Box::new(*d)))
}

fn qty_surface(mag_num: i64, mag_den: i64, d: &DimVector) -> String {
    emit_surface(&Value::Quantity(rat(mag_num, mag_den), Box::new(*d)))
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
        gen_val("D", Value::Dim(Box::new(product)))
    );

    // D is L / T → componentwise exponent SUBTRACTION (dimension quotient), for
    // both the exact `/` and the truncating `//` spellings (a dimension quotient
    // has no integer-division meaning; both subtract exponents).
    let quotient = dim_of(&[(LEN, 1, 1), (TIME, -1, 1)]);
    for op in [ArithOp::ExactDiv, ArithOp::Div] {
        assert_eq!(
            eval(&is(var("D"), var("L"), op, var("T")), &lookup),
            gen_val("D", Value::Dim(Box::new(quotient))),
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
        gen_val("Q", Value::Quantity(rat(8, 1), Box::new(length)))
    );
    assert_eq!(
        eval(&is(var("Q"), var("A"), ArithOp::Sub, var("B")), &equal),
        gen_val("Q", Value::Quantity(rat(2, 1), Box::new(length)))
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
        gen_val("Q", Value::Quantity(rat(6, 1), Box::new(lt)))
    );
    let div = env(&[
        ("N", &qty_surface(6, 1, &lt)),
        ("B", &qty_surface(3, 1, &time)),
    ]);
    assert_eq!(
        eval(&is(var("Q"), var("N"), ArithOp::ExactDiv, var("B")), &div),
        gen_val("Q", Value::Quantity(rat(2, 1), Box::new(length)))
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
        gen_val("Q", Value::Quantity(rat(6, 1), Box::new(length)))
    );
    // Scalar on the LEFT promotes identically: 2 * (3 L) = 6 L.
    assert_eq!(
        eval(
            &is(var("Q"), QTerm::Num(2), ArithOp::Mul, var("A")),
            &lookup
        ),
        gen_val("Q", Value::Quantity(rat(6, 1), Box::new(length)))
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
    let dimensionless = env(&[("Z", &qty_surface(3, 1, &DimVector::zero()))]);
    assert_eq!(
        eval(
            &is(var("Q"), var("Z"), ArithOp::Add, QTerm::Num(2)),
            &dimensionless
        ),
        gen_val("Q", Value::Quantity(rat(5, 1), Box::new(DimVector::zero())))
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

// ── Bilinear-form squared distance (the metric-form moded builtin) ───────────

/// The valence-dominant worked example (G = diag(2, 1)): the exact squared
/// distance from the state (1/2, 0) to two named prototypes, and the metric-nearest
/// verdict — the teaching point carried by the canonical
/// `slices/core/affect/examples/classify-canonical-prototype.ttl`: the closer
/// prototype in the valence-dominant metric (38/100) is NOT the raw-L² nearest
/// (43/100), so metric-nearest ≠ Euclidean-nearest (38/100 < 43/100).
#[test]
fn bilinear_sqdist_reproduces_nearest_prototype_worked_example() {
    let g = "urn:gmeow:test:gram";
    let state = "urn:gmeow:test:state";
    let contentment = "urn:gmeow:test:contentment";
    let elation = "urn:gmeow:test:elation";
    let resolver = FakeCells {
        gram_iri: g.to_owned(),
        gram: vec![(0, 0, rat(2, 1)), (1, 1, rat(1, 1))],
        vectors: vec![
            (state.to_owned(), vec![rat(1, 2), rat(0, 1)]),
            (contentment.to_owned(), vec![rat(1, 5), rat(1, 2)]),
            (elation.to_owned(), vec![rat(3, 5), rat(3, 5)]),
        ],
    };
    let lookup = env(&[]);

    // Δ = (3/10, −1/2) → 2·(3/10)² + 1·(1/2)² = 18/100 + 25/100 = 43/100.
    let to_contentment = bilinear(
        var("D"),
        iri_const(g),
        iri_const(state),
        iri_const(contentment),
    );
    assert_eq!(
        super::eval(&to_contentment, &lookup, &resolver),
        gen_rat("D", 43, 100),
        "state → contentment squared distance is exactly 43/100"
    );

    // Δ = (−1/10, −3/5) → 2·(1/10)² + 1·(3/5)² = 2/100 + 36/100 = 38/100.
    let to_elation = bilinear(var("D"), iri_const(g), iri_const(state), iri_const(elation));
    assert_eq!(
        super::eval(&to_elation, &lookup, &resolver),
        gen_rat("D", 38, 100),
        "state → elation squared distance is exactly 38/100"
    );

    // Nearest-prototype decides on the EXACT squared distance: 38/100 < 43/100.
    let dist = |b: &QBuiltin| match super::eval(b, &lookup, &resolver) {
        BuiltinOutcome::Generate {
            value: Value::Rat(r),
            ..
        } => r,
        other => panic!("expected a rational generate, got {other:?}"),
    };
    let d_elation = dist(&to_elation);
    let d_contentment = dist(&to_contentment);
    assert_eq!(
        apply_compare_q(&d_elation, CmpOp::Lt, &d_contentment),
        Ok(true),
        "elation is the metric-nearest prototype (38/100 < 43/100)"
    );
}

/// The PUBLIC production API reproduces the worked-example distances through the
/// same governed dispatch, and its overflow-safe ordering ranks elation nearest.
#[test]
fn public_bilinear_sqdist_reproduces_worked_example() {
    // G = diag(2, 1); state (1/2, 0); contentment (1/5, 1/2); elation (3/5, 3/5).
    let gram = vec![(0, 0, rat(2, 1)), (1, 1, rat(1, 1))];
    let state = vec![rat(1, 2), rat(0, 1)];
    let contentment = vec![rat(1, 5), rat(1, 2)];
    let elation = vec![rat(3, 5), rat(3, 5)];

    let d_c = super::bilinear_sqdist(&gram, &state, &contentment);
    let d_e = super::bilinear_sqdist(&gram, &state, &elation);
    assert_eq!(
        d_c,
        Ok(rat(43, 100)),
        "state → contentment is exactly 43/100"
    );
    assert_eq!(d_e, Ok(rat(38, 100)), "state → elation is exactly 38/100");

    // Ordering rides the governed overflow-safe compare, not Rational::cmp.
    assert_eq!(
        super::compare_sqdist(&d_e.unwrap(), &d_c.unwrap()),
        Ok(core::cmp::Ordering::Less),
        "elation is the metric-nearest prototype (38/100 < 43/100)"
    );
}

/// Malformed input is a TYPED error, never a panic or a wrong answer.
#[test]
fn public_bilinear_sqdist_dimension_mismatch_is_typed_error() {
    let gram = vec![(0, 0, rat(1, 1)), (1, 1, rat(1, 1))];
    let x = vec![rat(1, 1), rat(0, 1)];
    let y = vec![rat(1, 1)]; // shorter than x → mismatch, not a silent zero-complete
    assert_eq!(
        super::bilinear_sqdist(&gram, &x, &y),
        Err(super::BilinearFormError::DimensionMismatch)
    );
    // An absent form (no gram cells) is the metric-form fault.
    assert_eq!(
        super::bilinear_sqdist(&[], &x, &x),
        Err(super::BilinearFormError::MetricForm)
    );
}

/// A bound target filters on ℚ-correct value equality (mirrors `Is`).
#[test]
fn bilinear_sqdist_bound_target_filters() {
    let g = "urn:gmeow:test:gram";
    let x = "urn:gmeow:test:x";
    let y = "urn:gmeow:test:y";
    let resolver = FakeCells {
        gram_iri: g.to_owned(),
        gram: vec![(0, 0, rat(2, 1)), (1, 1, rat(1, 1))],
        vectors: vec![
            (x.to_owned(), vec![rat(1, 2), rat(0, 1)]),
            (y.to_owned(), vec![rat(1, 5), rat(1, 2)]),
        ],
    };
    // Target bound to the matching 43/100 → keep; a different value → prune.
    let pass = env(&[("D", &rat_surface(43, 100))]);
    let b = bilinear(var("D"), iri_const(g), iri_const(x), iri_const(y));
    assert_eq!(
        super::eval(&b, &pass, &resolver),
        BuiltinOutcome::Filter(true)
    );
    let fail = env(&[("D", &rat_surface(1, 2))]);
    assert_eq!(
        super::eval(&b, &fail, &resolver),
        BuiltinOutcome::Filter(false)
    );
}

/// Mismatched coordinate-vector lengths are a typed [`BuiltinError::DimensionMismatch`],
/// never a silently truncated (wrong) squared distance.
#[test]
fn bilinear_sqdist_mismatched_vector_lengths_is_error() {
    let g = "urn:gmeow:test:gram";
    let x = "urn:gmeow:test:x";
    let y = "urn:gmeow:test:y";
    let resolver = FakeCells {
        gram_iri: g.to_owned(),
        gram: vec![(0, 0, rat(2, 1)), (1, 1, rat(1, 1))],
        vectors: vec![
            (x.to_owned(), vec![rat(1, 2)]),
            (y.to_owned(), vec![rat(1, 5), rat(1, 2)]),
        ],
    };
    let b = bilinear(var("D"), iri_const(g), iri_const(x), iri_const(y));
    assert_eq!(
        super::eval(&b, &env(&[]), &resolver),
        BuiltinOutcome::Error(BuiltinError::DimensionMismatch)
    );
}

/// A well-formed 1×1 form is exact (control that dense fill + the kernel agree on a
/// degenerate order): (1/2 − 1/5)² · 1 = (3/10)² = 9/100.
#[test]
fn bilinear_sqdist_one_by_one_form_is_exact() {
    let g = "urn:gmeow:test:gram";
    let x = "urn:gmeow:test:x";
    let y = "urn:gmeow:test:y";
    let resolver = FakeCells {
        gram_iri: g.to_owned(),
        gram: vec![(0, 0, rat(1, 1))],
        vectors: vec![
            (x.to_owned(), vec![rat(1, 2)]),
            (y.to_owned(), vec![rat(1, 5)]),
        ],
    };
    let b = bilinear(var("D"), iri_const(g), iri_const(x), iri_const(y));
    assert_eq!(super::eval(&b, &env(&[]), &resolver), gen_rat("D", 9, 100));
}

/// An absent Gram / vector (no cells) is a typed [`BuiltinError::MetricForm`], not a
/// gap or a wrong answer — with [`NoCellResolver`] every operand is absent.
#[test]
fn bilinear_sqdist_absent_form_is_metric_form_error() {
    let b = bilinear(
        var("D"),
        iri_const("urn:gmeow:test:g"),
        iri_const("urn:gmeow:test:x"),
        iri_const("urn:gmeow:test:y"),
    );
    assert_eq!(
        super::eval(&b, &env(&[]), &NoCellResolver),
        BuiltinOutcome::Error(BuiltinError::MetricForm)
    );
}

/// An unbound operand variable is a declared mode gap (Unbound), never a guess.
#[test]
fn bilinear_sqdist_unbound_operand_is_gap() {
    let g = "urn:gmeow:test:gram";
    let resolver = FakeCells {
        gram_iri: g.to_owned(),
        gram: vec![(0, 0, rat(1, 1))],
        vectors: vec![("urn:gmeow:test:x".to_owned(), vec![rat(1, 1)])],
    };
    // The `y` operand is an unbound variable → gap.
    let b = bilinear(
        var("D"),
        iri_const(g),
        iri_const("urn:gmeow:test:x"),
        var("Y"),
    );
    assert_eq!(
        super::eval(&b, &env(&[]), &resolver),
        BuiltinOutcome::Unbound
    );
}
