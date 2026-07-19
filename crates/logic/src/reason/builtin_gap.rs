// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Ledgered moded-builtin gaps — the "never silently wrong" terminal for the native
//! numeric builtins.
//!
//! The moded evaluator ([`crate::physical::eval_builtin`]) declines a builtin either
//! because an operand is still unbound in its required mode
//! ([`BuiltinGapKind::Unbound`]) or because it hit a typed `math:` domain fault
//! ([`BuiltinGapKind::Error`] — ÷0, overflow, incommensurable dimensions). The
//! seminaive spine used to collapse both into a single payload-less `gap: &mut bool`,
//! so a refused program named neither the KIND, the operation, nor the operands: a
//! diagnostics producer with no ledger identity (the repo invariant this module
//! restores).
//!
//! Each [`BuiltinGap`] now threads the whole spine intact and terminates here, minted
//! into a [`gmeow_errors::DiagLedger`] the SAME way
//! [`crate::reason::ledger::divergence_diag_ledger`] mints its divergence rows: one
//! [`Diag`] per gap, at a blocking [`Grade`], with a message-independent distinctness
//! `focus` so the projected finding carries a distinct per-kind `finding_iri` /
//! `anchor_iri`. This is the single kind→identity mapping; no parallel ledger is
//! invented.

use gmeow_errors::{
    Diag, DiagLedger, FindingCategory, Grade, Severity, StageId, Standpoint, register_code,
};

use crate::physical::{BuiltinGap, BuiltinGapKind};

/// The [`gmeow_errors::StageId`] every builtin-gap witness is attached under — the
/// native moded-builtin refusal producer on the single diagnostics substrate.
const BUILTIN_GAP_STAGE: &str = "reason.builtin-gap";

/// The ASCII unit separator (`U+001F`) joining a gap's structural distinctness fields
/// into a message-independent fingerprint `focus`. It cannot occur in a `math:` class
/// IRI, an operator token, or a rendered operand, so the joined key is unambiguous —
/// the same discipline [`crate::reason::ledger`] uses.
const FOCUS_SEP: &str = "\u{1f}";

/// The `math:` class label carried in the `focus`/message for a pure mode gap
/// (`Unbound` names no domain-failure class).
const MODE_GAP_CLASS: &str = "mode-gap";

/// The blocking [`Grade`] every builtin gap is interned at — the SAME grade the
/// divergence ledger's failing kinds take: an [`Severity::Error`]
/// [`FindingCategory::ContradictionWitness`] at [`Standpoint::Binding`], so the
/// ledger's gate verdict is `Fatal` and the whole program is refused. A moded-builtin
/// gap is never a soft warning: an incomplete native answer is a wrong answer.
#[must_use]
fn builtin_gap_grade() -> Grade {
    Grade::new(
        Severity::Error,
        FindingCategory::ContradictionWitness,
        Standpoint::Binding,
    )
}

/// Render a gap's antecedent bindings `(var, surface)` into a stable, deterministic
/// operand list for the message and the distinctness key.
#[must_use]
fn render_bindings(gap: &BuiltinGap) -> String {
    let mut parts: Vec<String> = gap
        .bindings
        .iter()
        .map(|(name, surface)| format!("{name}={surface}"))
        .collect();
    parts.sort();
    parts.join(", ")
}

/// Mint the single ledgered [`Diag`] for one moded-builtin gap — the ONE shared
/// helper every terminal routes a produced gap through.
///
/// The witness carries:
///
/// * `code` = `reason.builtin-gap.{suffix}` (`unbound` for a mode gap, else the
///   [`BuiltinError`](crate::physical::BuiltinError) kind's kebab suffix), registered
///   via [`register_code`] — so a distinct kind hashes to a distinct `finding_iri`;
/// * `grade` = [`builtin_gap_grade`] (blocking), so the gap gates `Fatal`;
/// * `message` = names the `math:` conformance class (or the mode-gap marker), the
///   rendered operation, and the antecedent operands — so the kind is never anonymous;
/// * `focus` = a message-INDEPENDENT distinctness key over `(math-class, suffix, op,
///   bindings)`, so two gaps of different kinds never hash-cons-merge and each keeps a
///   distinct `anchor_iri`.
#[must_use]
pub(crate) fn builtin_gap_diag(gap: &BuiltinGap) -> Diag {
    let suffix = gap.kind.code_suffix_or_unbound();
    let code = register_code(&format!("reason.builtin-gap.{suffix}"));
    let class = gap.kind.math_class().unwrap_or(MODE_GAP_CLASS);
    let operands = render_bindings(gap);

    let message = match &gap.kind {
        BuiltinGapKind::Unbound => format!(
            "moded builtin `{}` declined: an operand is unbound in its required mode \
             (antecedents: {operands})",
            gap.op
        ),
        BuiltinGapKind::Error(_) => format!(
            "moded builtin `{}` raised {class}: a typed math conformance failure \
             (antecedents: {operands})",
            gap.op
        ),
    };

    let focus = [class, suffix, gap.op.as_str(), operands.as_str()].join(FOCUS_SEP);
    Diag::new(code, builtin_gap_grade(), message).with_focus(focus)
}

/// Intern a set of moded-builtin gaps into a fresh [`gmeow_errors::DiagLedger`] — one
/// [`builtin_gap_diag`] per gap under [`BUILTIN_GAP_STAGE`].
///
/// This is the terminal projection the demand/dispatch refusal path attaches to: the
/// ledger's [`findings`](DiagLedger::findings) carry a distinct per-kind
/// `finding_iri`/`anchor_iri`, and its [`verdict`](DiagLedger::verdict) is `Fatal`
/// whenever any gap is present (each gap is graded blocking).
#[must_use]
pub(crate) fn builtin_gap_ledger(gaps: &[BuiltinGap]) -> DiagLedger {
    let mut ledger = DiagLedger::new();
    let stage = StageId::new(BUILTIN_GAP_STAGE);
    for gap in gaps {
        ledger.attach(builtin_gap_diag(gap), stage.clone());
    }
    ledger
}

/// A one-line English refusal naming every distinct gap kind + operation — the
/// message the production dispatch terminal returns when it refuses a program on a
/// moded-builtin gap, so the refusal is never the anonymous "does not support
/// Arithmetic".
#[must_use]
pub(crate) fn builtin_gap_refusal_detail(gaps: &[BuiltinGap]) -> String {
    let mut lines: Vec<String> = gaps
        .iter()
        .map(|gap| {
            let class = gap.kind.math_class().unwrap_or(MODE_GAP_CLASS);
            format!("{class} on `{}`", gap.op)
        })
        .collect();
    lines.sort();
    lines.dedup();
    format!(
        "native backward engine refused the query on a moded-builtin gap: {}; \
         query refused rather than presenting an incomplete numeric answer",
        lines.join("; ")
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::physical::{
        BuiltinError, Executable, NativeOutcome, Parsed, RelationStore, UnsupportedKind, Value,
        emit_surface, evaluate,
    };
    use crate::query_ir::{ArithOp, CmpOp, QBuiltin, QTerm};
    use crate::rule_ir::{EvalAtom, EvalRule, EvalTerm, Fact};
    use gmeow_errors::Severity;
    use gmeow_math::Rational;
    use gmeow_math::dimension::DimVector;
    use purrdf::TermValue;

    const GUARD: &str = "https://ex/guard";
    const HEAD: &str = "https://ex/bad";

    /// The engine `?`-prefixed variable surface a body atom / builtin operand carries.
    fn qvar(name: &str) -> QTerm {
        QTerm::Var(format!("?{name}"))
    }

    /// Build an [`Executable`] from rules through the sole type-state chain.
    fn exe(rules: &[EvalRule]) -> Executable {
        Parsed::uncached(rules)
            .stratify()
            .expect("stratifiable")
            .plan()
            .into_executable()
    }

    /// A single-rule program `bad(?S, ?Z) :- guard(?S, ?V), <builtin>.` seeded with one
    /// EDB fact `guard(a, o)`. The gap short-circuits `apply_builtins` before the head is
    /// grounded, so `?Z` never needs a binding (the `Compare` filter binds nothing).
    fn rule_with(builtin: QBuiltin) -> EvalRule {
        EvalRule {
            head: EvalAtom::positive(EvalTerm::var("?S"), HEAD, EvalTerm::var("?Z")),
            body: vec![EvalAtom::positive(
                EvalTerm::var("?S"),
                GUARD,
                EvalTerm::var("?V"),
            )],
            rule_iri: "https://ex/bad::rule".to_owned(),
            distinct_pairs: Vec::new(),
            builtins: vec![builtin],
        }
    }

    fn seeded_edb() -> RelationStore {
        let mut edb = RelationStore::new();
        edb.insert(
            GUARD,
            &TermValue::iri("https://ex/a"),
            &TermValue::iri("https://ex/o"),
        );
        edb
    }

    /// Drive the real demand-path terminal (`seminaive::evaluate`) and return the carried
    /// gaps (the `Vec<BuiltinGap>` threaded through the whole spine).
    fn gaps_for(builtin: QBuiltin) -> Vec<BuiltinGap> {
        let rule = rule_with(builtin);
        match evaluate(seeded_edb(), &exe(&[rule]), None).expect("evaluate") {
            NativeOutcome::Unsupported(UnsupportedKind::Arithmetic(gaps)) => gaps,
            other => panic!("expected an Arithmetic gap, got {other:?}"),
        }
    }

    /// A `math:` quantity transport surface for `scalar` over a single base dimension
    /// (index 0 = length, 1 = mass) — two INCOMMENSURABLE quantities for a
    /// DimensionMismatch.
    fn quantity_surface(scalar: i64, base_index: usize) -> String {
        let mut dim = DimVector::zero();
        dim.add_exponent(base_index, Rational::from_i128(1).unwrap())
            .unwrap();
        emit_surface(&Value::Quantity(
            Rational::from_i128(i128::from(scalar)).unwrap(),
            Box::new(dim),
        ))
    }

    #[test]
    fn zero_divisor_overflow_and_dimension_mismatch_are_distinct_ledgered_findings() {
        // ZeroDivisor: `?Z is 1 // 0` (a generator with a zero integer divisor).
        let zero = gaps_for(QBuiltin::Is {
            target: qvar("Z"),
            lhs: QTerm::Num(1),
            op: ArithOp::Div,
            rhs: QTerm::Num(0),
        });
        // Overflow: `?Z is i64::MAX + 1`.
        let over = gaps_for(QBuiltin::Is {
            target: qvar("Z"),
            lhs: QTerm::Num(i64::MAX),
            op: ArithOp::Add,
            rhs: QTerm::Num(1),
        });
        // DimensionMismatch: compare a length quantity with a mass quantity (both ground
        // transport constants) — incommensurable, so the compare raises the typed fault.
        let dim = gaps_for(QBuiltin::Compare {
            lhs: QTerm::Const(quantity_surface(1, 0)),
            op: CmpOp::Gt,
            rhs: QTerm::Const(quantity_surface(1, 1)),
        });

        // Each real path produced exactly one gap carrying the RIGHT typed kind — the
        // kind survived the whole seminaive spine, not collapsed to a bare bool.
        assert_eq!(zero.len(), 1, "one gap: {zero:?}");
        assert!(matches!(
            zero[0].kind,
            BuiltinGapKind::Error(BuiltinError::ZeroDivisor)
        ));
        assert!(matches!(
            over[0].kind,
            BuiltinGapKind::Error(BuiltinError::Overflow)
        ));
        assert!(matches!(
            dim[0].kind,
            BuiltinGapKind::Error(BuiltinError::DimensionMismatch)
        ));
        // The antecedent bindings were captured (the guard bound ?S and ?V).
        assert!(
            !zero[0].bindings.is_empty(),
            "antecedent bindings present: {:?}",
            zero[0]
        );

        // Ledger all three through the ONE shared helper (the same the dispatch terminal
        // uses) and assert distinct per-kind identity.
        let mut all = Vec::new();
        all.extend(zero.clone());
        all.extend(over.clone());
        all.extend(dim.clone());
        let ledger = builtin_gap_ledger(&all);
        let findings = ledger.findings("reason");
        assert_eq!(
            findings.len(),
            3,
            "one finding per distinct gap: {findings:?}"
        );

        let by_code = |code: &str| {
            findings
                .iter()
                .find(|f| f.code == code)
                .unwrap_or_else(|| panic!("a {code} finding in {findings:?}"))
        };
        let zd = by_code("reason.builtin-gap.zero-divisor");
        let ov = by_code("reason.builtin-gap.overflow");
        let dm = by_code("reason.builtin-gap.dimension-mismatch");

        for f in [zd, ov, dm] {
            assert_eq!(f.severity, Severity::Error, "blocking: {f:?}");
            assert_eq!(
                f.category,
                Some(FindingCategory::ContradictionWitness),
                "contradiction witness: {f:?}"
            );
            assert!(
                f.finding_iri.as_deref().is_some_and(|s| !s.is_empty()),
                "non-empty finding_iri: {f:?}"
            );
            assert!(
                f.anchor_iri.as_deref().is_some_and(|s| !s.is_empty()),
                "non-empty anchor_iri: {f:?}"
            );
        }

        // The kind is PRESERVED, not collapsed: distinct kinds have distinct finding_iri
        // AND anchor_iri.
        assert_ne!(zd.finding_iri, ov.finding_iri);
        assert_ne!(zd.finding_iri, dm.finding_iri);
        assert_ne!(ov.finding_iri, dm.finding_iri);
        assert_ne!(zd.anchor_iri, ov.anchor_iri);
        assert_ne!(zd.anchor_iri, dm.anchor_iri);
        assert_ne!(ov.anchor_iri, dm.anchor_iri);

        // The math: class is named in the message, and the antecedent operands ride in.
        assert!(zd.message.contains("math/ZeroDivisor"), "{}", zd.message);
        assert!(ov.message.contains("math/Overflow"), "{}", ov.message);
        assert!(
            dm.message.contains("math/DimensionalInhomogeneity"),
            "{}",
            dm.message
        );
        assert!(zd.message.contains("antecedents:"), "{}", zd.message);

        // The ledger's aggregate verdict is Fatal (each gap is graded blocking).
        assert_eq!(ledger.verdict(), gmeow_errors::GateVerdict::Fatal);
    }

    #[test]
    fn successful_division_produces_no_gap_finding() {
        // Negative control: `?Z is 6 // 2` succeeds → the program is DECIDED, no gap.
        let rule = EvalRule {
            head: EvalAtom::positive(EvalTerm::var("?S"), "https://ex/ok", EvalTerm::var("?Z")),
            body: vec![EvalAtom::positive(
                EvalTerm::var("?S"),
                GUARD,
                EvalTerm::var("?V"),
            )],
            rule_iri: "https://ex/ok::rule".to_owned(),
            distinct_pairs: Vec::new(),
            builtins: vec![QBuiltin::Is {
                target: qvar("Z"),
                lhs: QTerm::Num(6),
                op: ArithOp::Div,
                rhs: QTerm::Num(2),
            }],
        };
        let facts: Vec<Fact> = match evaluate(seeded_edb(), &exe(&[rule]), None).expect("evaluate")
        {
            NativeOutcome::Decided(budgeted) => budgeted.rows,
            other => panic!("expected Decided, got {other:?}"),
        };
        // The head fired with ?Z bound to the computed quotient.
        assert!(
            facts.iter().any(|f| f.predicate == "https://ex/ok"),
            "the ok/2 head fired: {facts:?}"
        );
        // An empty gap set ledgers to zero findings and a Collected (non-fatal) verdict.
        let ledger = builtin_gap_ledger(&[]);
        assert!(ledger.findings("reason").is_empty());
        assert_eq!(ledger.verdict(), gmeow_errors::GateVerdict::Collected);
    }
}
