// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Tests for the relational-core lowering waist (#719).

use super::*;
use crate::lower::lower_rule;
use gmeow_logic_compile::ir::{
    ContextualScope, Formula, LogicAxiom, LogicProgram, LogicRule, PreservationKind, Term,
};

const LOGIC: &str = "https://blackcatinformatics.ca/logic/";

fn var(name: &str) -> Term {
    Term::var(name).unwrap()
}

fn con(local: &str) -> Term {
    Term::iri(format!("{LOGIC}{local}")).unwrap()
}

fn atom(rel: &str, args: Vec<Term>) -> Formula {
    Formula::atom(Term::iri(format!("{LOGIC}{rel}")).unwrap(), args).unwrap()
}

fn program_with(formulas: Vec<Formula>) -> LogicProgram {
    LogicProgram::new(vec![], vec![], vec![], None).with_formulas(formulas)
}

/// `∀x. (p(x, a) → q(x, b))` — a Horn rule of binary atoms.
fn horn_rule_formula() -> Formula {
    Formula::Forall {
        vars: vec!["x".into()],
        body: Box::new(Formula::Implies(
            Box::new(atom("p", vec![var("x"), con("a")])),
            Box::new(atom("q", vec![var("x"), con("b")])),
        )),
    }
}

#[test]
fn horn_formula_lowers_exactly_to_one_rule() {
    let out = lower_formulas(&program_with(vec![horn_rule_formula()]));
    assert_eq!(out.rules.len(), 1, "the Horn formula yields one rule");
    let rule = &out.rules[0];
    assert_eq!(rule.body.len(), 1, "one body atom (the antecedent)");
    // Exact preservation: nothing was carried as residue.
    assert!(out.preservation.unsupported_constructs.is_empty());
    assert!(out
        .preservation
        .polarities
        .contains(&PreservationKind::Exact));
}

#[test]
fn horn_formula_matches_the_equivalent_logic_rule() {
    // The same rule authored directly as a LogicRule must lower to the same head + body
    // (the rule_iri is a provenance/naming artifact and is allowed to differ).
    let head = LogicAxiom::ground("?x", format!("{LOGIC}q"), format!("{LOGIC}b"), false).unwrap();
    let body =
        vec![LogicAxiom::ground("?x", format!("{LOGIC}p"), format!("{LOGIC}a"), false).unwrap()];
    let logic_rule = LogicRule::new(head, body, vec![], ContextualScope::default());
    let expected = lower_rule(&logic_rule).expect("lower logic rule");

    let out = lower_formulas(&program_with(vec![horn_rule_formula()]));
    let got = &out.rules[0];
    assert_eq!(
        got.head, expected.head,
        "head must match the LogicRule lowering"
    );
    assert_eq!(
        got.body, expected.body,
        "body must match the LogicRule lowering"
    );
}

#[test]
fn disjunctive_head_is_unsupported_never_exact() {
    // ∀x. (p(x,a) → (q(x,b) ∨ r(x,c))) — a disjunctive head is not Horn.
    let f = Formula::Forall {
        vars: vec!["x".into()],
        body: Box::new(Formula::Implies(
            Box::new(atom("p", vec![var("x"), con("a")])),
            Box::new(Formula::Or(vec![
                atom("q", vec![var("x"), con("b")]),
                atom("r", vec![var("x"), con("c")]),
            ])),
        )),
    };
    let out = lower_formulas(&program_with(vec![f]));
    assert!(out.rules.is_empty(), "a disjunctive head yields no rule");
    // The eval-path honesty gate: residue present ⇒ SoundUnder, NEVER Exact.
    assert!(!out.preservation.unsupported_constructs.is_empty());
    assert!(out
        .preservation
        .polarities
        .contains(&PreservationKind::SoundUnder));
    assert!(!out
        .preservation
        .polarities
        .contains(&PreservationKind::Exact));
}

#[test]
fn quantifier_alternation_is_unsupported() {
    // ∀x. ∃y. p(x, y) — ∃ under ∀ would need a Skolem function; flagged, not mis-lowered.
    let f = Formula::Forall {
        vars: vec!["x".into()],
        body: Box::new(Formula::Exists {
            vars: vec!["y".into()],
            body: Box::new(atom("p", vec![var("x"), var("y")])),
        }),
    };
    let out = lower_formulas(&program_with(vec![f]));
    assert!(out.rules.is_empty());
    assert!(!out.preservation.unsupported_constructs.is_empty());
    assert!(out
        .preservation
        .polarities
        .contains(&PreservationKind::SoundUnder));
}

#[test]
fn sequence_marker_atom_is_unsupported() {
    // rel(...xs) — a variadic predication has no binary relational-core form.
    let f = Formula::atom(
        Term::iri(format!("{LOGIC}rel")).unwrap(),
        vec![Term::sequence_marker("xs").unwrap()],
    )
    .unwrap();
    let out = lower_formulas(&program_with(vec![f]));
    assert!(out.rules.is_empty());
    assert!(!out.preservation.unsupported_constructs.is_empty());
}

#[test]
fn partial_conversion_lowers_the_horn_conjunct_and_flags_the_rest() {
    // A top-level conjunction: one Horn rule + one disjunctive (unsupported) clause.
    let horn = horn_rule_formula();
    let disjunctive = Formula::Forall {
        vars: vec!["x".into()],
        body: Box::new(Formula::Implies(
            Box::new(atom("p", vec![var("x"), con("a")])),
            Box::new(Formula::Or(vec![
                atom("q", vec![var("x"), con("b")]),
                atom("r", vec![var("x"), con("c")]),
            ])),
        )),
    };
    // ∀x. (horn-body ∧ disjunctive-body) — modelled as two separate formulas, exercising
    // both legal-lowering and residue in one program (legalization: legal ⊕ flagged).
    let out = lower_formulas(&program_with(vec![horn, disjunctive]));
    assert_eq!(out.rules.len(), 1, "the Horn conjunct lowers");
    assert!(
        !out.preservation.unsupported_constructs.is_empty(),
        "the disjunctive conjunct is flagged residue"
    );
}

#[test]
fn existential_constant_skolemization_is_deterministic() {
    // ∃y. p(c, y) and ∃z. p(c, z) are alpha-equivalent, so the Skolem witness — and thus
    // the lowered fact — must be byte-identical regardless of the authored bound name.
    let f_y = Formula::Exists {
        vars: vec!["y".into()],
        body: Box::new(atom("p", vec![con("c"), var("y")])),
    };
    let f_z = Formula::Exists {
        vars: vec!["z".into()],
        body: Box::new(atom("p", vec![con("c"), var("z")])),
    };
    let out_y = lower_formulas(&program_with(vec![f_y]));
    let out_z = lower_formulas(&program_with(vec![f_z]));
    assert_eq!(out_y.rules.len(), 1, "a top-level ∃ Skolemizes to a fact");
    assert_eq!(
        out_y.rules, out_z.rules,
        "alpha-equivalent existentials must produce identical Skolemized rules"
    );
    // And the lowering is Exact (the ∃-constant fragment is fully supported).
    assert!(out_y
        .preservation
        .polarities
        .contains(&PreservationKind::Exact));
}

#[test]
fn shadowed_existential_binds_the_innermost_witness() {
    // ∃x. ∃x. p(c, x) — the leading prefix mints two Skolem constants (outer `-0`,
    // inner `-1`); `peel_exists` collects names = ["x", "x"] in that order. The matrix
    // occurrence of `x` is bound by the INNERMOST `∃x`, so it must resolve to the `-1`
    // witness. A forward substitution search would wrongly bind it to the outer `-0`.
    let f = Formula::Exists {
        vars: vec!["x".into()],
        body: Box::new(Formula::Exists {
            vars: vec!["x".into()],
            body: Box::new(atom("p", vec![con("c"), var("x")])),
        }),
    };
    let out = lower_formulas(&program_with(vec![f]));
    assert_eq!(
        out.rules.len(),
        1,
        "the leading-∃ prefix Skolemizes to a fact"
    );
    let rendered = format!("{:?}", out.rules[0]);
    assert!(
        rendered.contains("skolem/"),
        "the witness is a Skolem constant: {rendered}"
    );
    assert!(
        rendered.contains("-1"),
        "the matrix var binds the INNERMOST (index-1) witness: {rendered}"
    );
    assert!(
        !rendered.contains("-0"),
        "the outer (index-0) witness must NOT bind the matrix var (forward-search bug): {rendered}"
    );
}

#[test]
fn lowering_is_stable_across_repeated_calls() {
    // Idempotence at the public boundary: lowering the same program twice agrees.
    let prog = program_with(vec![horn_rule_formula(), horn_rule_formula()]);
    let a = lower_formulas(&prog);
    let b = lower_formulas(&prog);
    assert_eq!(a.rules, b.rules);
    assert_eq!(
        a.preservation.unsupported_constructs,
        b.preservation.unsupported_constructs
    );
}
