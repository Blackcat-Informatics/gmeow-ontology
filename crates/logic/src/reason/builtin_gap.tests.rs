// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

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
        numeric: Vec::new(),
        head: EvalAtom::positive(EvalTerm::var("?S"), HEAD, EvalTerm::var("?Z")),
        body: vec![EvalAtom::positive(
            EvalTerm::var("?S"),
            GUARD,
            EvalTerm::var("?V"),
        )],
        rule_iri: "https://ex/bad::rule".to_owned(),
        distinct_pairs: Vec::new(),
        builtins: vec![builtin],
        reduction: None,
        constraint_tag: None,
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
        numeric: Vec::new(),
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
        reduction: None,
        constraint_tag: None,
    };
    let facts: Vec<Fact> = match evaluate(seeded_edb(), &exe(&[rule]), None).expect("evaluate") {
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
