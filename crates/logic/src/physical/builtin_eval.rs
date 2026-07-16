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
//! Evaluation is over checked `i64`. Integer division `//` truncates toward zero
//! (ISO `//`, matching the captured SLD semantics), and `=:=` is numeric value
//! equality, never structural unification. A value that cannot be computed is a
//! first-class declared gap ([`BuiltinOutcome::Unbound`]) or domain/precision
//! error ([`BuiltinOutcome::Error`]) — never a wrong answer or a panic.

use crate::query_ir::{ArithOp, CmpOp, QBuiltin, QTerm};
use std::borrow::Cow;

/// The canonical `xsd:integer` datatype IRI — the type of every computed
/// arithmetic answer. The surface form produced by [`emit_integer_surface`] is
/// byte-identical to `provenance::literal_n3` for `xsd:integer` and to the form
/// the captured SLD reference renders, so a generated value reads back like a
/// materialized typed literal.
pub(crate) const XSD_INTEGER: &str = "http://www.w3.org/2001/XMLSchema#integer";

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

/// The resolved binding state of a single builtin operand.
enum Operand {
    /// The operand is bound to an integer value.
    Num(i64),
    /// The operand is a variable with no binding under the current substitution.
    Unbound,
    /// The operand is bound to a surface that is not an integer.
    NonNumeric,
}

/// A domain / precision error raised while computing a builtin — routed by the
/// caller to a declared gap, never surfaced as a wrong answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BuiltinError {
    /// Integer division / remainder by zero (the oracle raises `zero_divisor`).
    ZeroDivisor,
    /// A computation overflowed the machine integer (the oracle uses a bignum).
    Overflow,
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
        /// The computed integer value.
        value: i64,
    },
    /// An operand needed for evaluation is still unbound — a declared mode gap
    /// (the caller declines rather than guessing).
    Unbound,
    /// A domain / precision error (÷0 or i64 overflow) — a declared gap.
    Error(BuiltinError),
}

/// Resolve one operand `term` to its binding state under `lookup`.
fn resolve_operand<'a>(term: &QTerm, lookup: &impl Fn(&str) -> Option<Cow<'a, str>>) -> Operand {
    match term {
        QTerm::Num(n) => Operand::Num(*n),
        QTerm::Var(v) => match lookup(v) {
            None => Operand::Unbound,
            Some(surface) => match parse_integer_surface(&surface) {
                Some(n) => Operand::Num(n),
                None => Operand::NonNumeric,
            },
        },
        // A bare `Const` surface may still be a typed integer literal (e.g. a fact
        // object materialized as `"3"^^<…#integer>`); otherwise it is non-numeric.
        QTerm::Const(c) => match parse_integer_surface(c) {
            Some(n) => Operand::Num(n),
            None => Operand::NonNumeric,
        },
        // A structured (function-symbol) operand is never a number; it is routed to the
        // full-FOL resolver upstream, so on the flat builtin path it is a non-numeric filter
        // failure, never a gap.
        QTerm::Struct(_) => Operand::NonNumeric,
    }
}

/// Apply an arithmetic operator with checked `i64` semantics.
///
/// `//` truncates toward zero (Rust `i64` division), matching ISO Prolog `//`.
/// Division by zero and any overflow are declared errors, never a wrong answer.
fn apply_arith(lhs: i64, op: ArithOp, rhs: i64) -> Result<i64, BuiltinError> {
    match op {
        ArithOp::Add => lhs.checked_add(rhs).ok_or(BuiltinError::Overflow),
        ArithOp::Sub => lhs.checked_sub(rhs).ok_or(BuiltinError::Overflow),
        ArithOp::Mul => lhs.checked_mul(rhs).ok_or(BuiltinError::Overflow),
        ArithOp::Div => {
            if rhs == 0 {
                Err(BuiltinError::ZeroDivisor)
            } else {
                // `checked_div` also guards the i64::MIN / -1 overflow.
                lhs.checked_div(rhs).ok_or(BuiltinError::Overflow)
            }
        }
    }
}

/// Apply a comparison operator as numeric value comparison.
fn apply_compare(lhs: i64, op: CmpOp, rhs: i64) -> bool {
    match op {
        CmpOp::Gt => lhs > rhs,
        CmpOp::Lt => lhs < rhs,
        CmpOp::Ge => lhs >= rhs,
        CmpOp::Le => lhs <= rhs,
        CmpOp::Eq => lhs == rhs,
    }
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
            // Both operands must be bound integers to compute; anything else is a
            // declared mode gap (a non-numeric operand is a type
            // error, which we decline rather than guess).
            let (l, r) = match (resolve_operand(lhs, lookup), resolve_operand(rhs, lookup)) {
                (Operand::Num(l), Operand::Num(r)) => (l, r),
                _ => return BuiltinOutcome::Unbound,
            };
            let value = match apply_arith(l, *op, r) {
                Ok(v) => v,
                Err(e) => return BuiltinOutcome::Error(e),
            };
            // Target role: unbound variable → generate; bound numeric → filter on
            // numeric equality; bound non-numeric → filter false (`foo is 1+2`
            // fails, it is not a gap).
            match target {
                QTerm::Var(v) => match lookup(v) {
                    None => BuiltinOutcome::Generate {
                        var: v.clone(),
                        value,
                    },
                    Some(surface) => match parse_integer_surface(&surface) {
                        Some(t) => BuiltinOutcome::Filter(t == value),
                        None => BuiltinOutcome::Filter(false),
                    },
                },
                QTerm::Num(t) => BuiltinOutcome::Filter(*t == value),
                QTerm::Const(c) => match parse_integer_surface(c) {
                    Some(t) => BuiltinOutcome::Filter(t == value),
                    None => BuiltinOutcome::Filter(false),
                },
                // A structured target can never equal a computed integer: filter false.
                QTerm::Struct(_) => BuiltinOutcome::Filter(false),
            }
        }
        QBuiltin::Compare { lhs, op, rhs } => {
            match (resolve_operand(lhs, lookup), resolve_operand(rhs, lookup)) {
                (Operand::Num(l), Operand::Num(r)) => {
                    BuiltinOutcome::Filter(apply_compare(l, *op, r))
                }
                // Either operand unbound or non-numeric → cannot compare; gap.
                _ => BuiltinOutcome::Unbound,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a `lookup` from a small set of (var, surface) pairs.
    fn env(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<Cow<'static, str>> {
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
        assert_eq!(
            eval(&b, &lookup),
            BuiltinOutcome::Generate {
                var: "N".to_owned(),
                value: 3
            }
        );
    }

    #[test]
    fn is_generator_over_bare_num_operands() {
        // X is 6 // 4 → 1 (truncation), X free.
        let lookup = env(&[]);
        let b = is(var("X"), QTerm::Num(6), ArithOp::Div, QTerm::Num(4));
        assert_eq!(
            eval(&b, &lookup),
            BuiltinOutcome::Generate {
                var: "X".to_owned(),
                value: 1
            }
        );
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
            BuiltinOutcome::Generate {
                var: "X".to_owned(),
                value: -3
            }
        );
        assert_eq!(
            eval(
                &is(var("X"), QTerm::Num(7), ArithOp::Div, QTerm::Num(-2)),
                &lookup
            ),
            BuiltinOutcome::Generate {
                var: "X".to_owned(),
                value: -3
            }
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
            BuiltinOutcome::Generate {
                var: "X".to_owned(),
                value: -6
            }
        );
        assert_eq!(
            eval(
                &is(var("X"), QTerm::Num(-3), ArithOp::Mul, QTerm::Num(4)),
                &lookup
            ),
            BuiltinOutcome::Generate {
                var: "X".to_owned(),
                value: -12
            }
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
}
