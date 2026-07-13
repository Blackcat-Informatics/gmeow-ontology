// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Tests for the relational-core lowering waist.

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
    assert!(
        out.preservation
            .polarities
            .contains(&PreservationKind::Exact)
    );
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

/// A 2-body-atom formula given in NON-canonical source order must produce the same
/// canonical body ordering as an equivalent `LogicRule`, proving the formula lane's
/// sort is byte-identical to `LogicRule::new`'s `sort_by_cached_key(LogicAxiom::sort_key)`.
///
/// Source order: `q(x,z) ∧ p(x,y)` (q before p — non-canonical).
/// Canonical order: `p(x,y)` before `q(x,z)` (because "{LOGIC}p" < "{LOGIC}q" lexically
/// under the `subject\0pred\0obj\0False` key).
///
/// The test would FAIL if the sort in `lower_formula_clause` were removed, because the
/// formula lane would then yield `[q, p]` while the LogicRule path yields `[p, q]`.
#[test]
fn multi_atom_formula_body_order_matches_logic_rule_canonical_order() {
    // ∀x y z. (q(x,z) ∧ p(x,y)) → head(x,z)
    // Body atoms authored in NON-canonical order (q before p) to exercise the sort.
    let body_formula = Formula::And(vec![
        atom("qRel", vec![var("x"), var("z")]), // q first — non-canonical
        atom("pRel", vec![var("x"), var("y")]), // p second
    ]);
    let formula = Formula::Forall {
        vars: vec!["x".into(), "y".into(), "z".into()],
        body: Box::new(Formula::Implies(
            Box::new(body_formula),
            Box::new(atom("headRel", vec![var("x"), var("z")])),
        )),
    };

    // Build the equivalent LogicRule with the SAME atoms in any order; LogicRule::new
    // canonicalizes the body, so the expected ordering is the canonical one.
    let head_ax = LogicAxiom::new(
        "?x",
        format!("{LOGIC}headRel"),
        "?z",
        false,
        false,
        ContextualScope::default(),
    )
    .unwrap();
    let p_ax = LogicAxiom::new(
        "?x",
        format!("{LOGIC}pRel"),
        "?y",
        false,
        false,
        ContextualScope::default(),
    )
    .unwrap();
    let q_ax = LogicAxiom::new(
        "?x",
        format!("{LOGIC}qRel"),
        "?z",
        false,
        false,
        ContextualScope::default(),
    )
    .unwrap();
    // Pass body in source (non-canonical) order — LogicRule::new will sort it.
    let logic_rule = LogicRule::new(
        head_ax,
        vec![q_ax, p_ax], // non-canonical: q before p
        vec![],
        ContextualScope::default(),
    );
    let expected = lower_rule(&logic_rule).expect("lower logic rule");

    // Lower the formula through the full-FOL lane.
    let out = lower_formulas(&program_with(vec![formula]));
    assert_eq!(
        out.rules.len(),
        1,
        "the 2-body-atom formula yields one rule"
    );
    let got = &out.rules[0];

    // The body must have exactly 2 atoms and be in canonical order (p before q).
    assert_eq!(got.body.len(), 2, "both body atoms must survive");
    assert_eq!(
        expected.body.len(),
        2,
        "sanity: LogicRule lowering also has 2 body atoms"
    );

    assert_eq!(
        got.head, expected.head,
        "head must match the LogicRule lowering"
    );
    assert_eq!(
        got.body, expected.body,
        "body canonical order must match the LogicRule lowering (pRel before qRel)"
    );

    // Extra explicit check: pRel must appear before qRel in the body.
    // This would fail if the sort were absent (source order puts q first).
    let first_pred = &got.body[0].predicate;
    let second_pred = &got.body[1].predicate;
    assert!(
        first_pred.contains("pRel"),
        "canonical order: pRel must be the first body atom, got: {first_pred}"
    );
    assert!(
        second_pred.contains("qRel"),
        "canonical order: qRel must be the second body atom, got: {second_pred}"
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
    assert!(
        out.preservation
            .polarities
            .contains(&PreservationKind::SoundUnder)
    );
    assert!(
        !out.preservation
            .polarities
            .contains(&PreservationKind::Exact)
    );
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
    assert!(
        out.preservation
            .polarities
            .contains(&PreservationKind::SoundUnder)
    );
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
    assert!(
        out_y
            .preservation
            .polarities
            .contains(&PreservationKind::Exact)
    );
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

/// Verify that the lane (`lower_formulas_to_rc`) deduplicates by content key (first-wins,
/// stable order), and that `lower_formulas` inherits that dedup without an extra pass.
///
/// Two identical copies of `horn_rule_formula()` reach `lower_formulas_to_rc`; the lane
/// must collapse them to a single `RcRule` (dedup-at-lane). The adapter then maps that one
/// `RcRule` onward to one `EvalRule`, with exact preservation (the duplicate is not residue).
#[test]
fn duplicate_formulas_produce_one_rule_not_two() {
    let f = horn_rule_formula();
    // The lane now dedups, so two identical formulas yield exactly one RcRule.
    use gmeow_logic_compile::relational_core::lower_formulas_to_rc;
    let prog = program_with(vec![f.clone(), f.clone()]);
    let (rc_rules, rc_residue) = lower_formulas_to_rc(&prog);
    assert_eq!(
        rc_rules.len(),
        1,
        "dedup-at-lane: lower_formulas_to_rc must collapse two identical formulas to one RcRule"
    );
    assert!(rc_residue.is_empty(), "no residue from Horn formulas");

    // The adapter inherits the dedup from the lane and maps one RcRule to one EvalRule.
    let out = lower_formulas(&prog);
    assert_eq!(
        out.rules.len(),
        1,
        "lower_formulas must yield one EvalRule: duplicate already dropped by the lane"
    );
    assert_eq!(
        out.rules[0].body.len(),
        1,
        "the surviving rule has one body atom"
    );
    // Exact preservation: a duplicate is not residue.
    use gmeow_logic_compile::ir::PreservationKind;
    assert!(
        out.preservation.unsupported_constructs.is_empty(),
        "a duplicate identical clause is not residue"
    );
    assert!(
        out.preservation
            .polarities
            .contains(&PreservationKind::Exact),
        "dedup of identical clauses must not degrade preservation to sound-under"
    );
}

/// Flagship 1 (the `lang:` MEANING stratum): the declarative sentence "cats chase mice"
/// denotes a full-FOL `logic:Formula`, authored as an RDF AST in the shared conformance
/// fixture. This test ties the RDF fixture to the native reasoner: it parses the fixture's
/// `logic:` layer with the real front-end and asserts the reasoner CONSUMES the denoted
/// formula — `lower_formulas` clausifies it into an evaluable Horn rule with EXACT
/// preservation — rather than merely DL-typing an empty AST node.
///
/// The formula is `∀x∀y. (instanceOf(x, typeCat) ∧ instanceOf(y, typeMouse)) → chase(x, y)`,
/// authored in the binary-Horn fragment (the nouns predicated through the HiLog
/// `logic:instanceOf` reflection so every atom is binary). It lowers to a single rule
/// `chase(x, y) ← instanceOf(x, typeCat) ∧ instanceOf(y, typeMouse)` — from which, given a
/// cat and a mouse, the chase derives a chased mouse (the flagship entailment "some mouse is
/// chased"). Both stages are exact: the `lang:` → `logic:` denotation (the fixture's asserted
/// `logic:ExactPreservation`) and the `logic:` → relational-core evaluation asserted here.
#[test]
fn flagship_cats_chase_mice_lowers_to_evaluable_rules_with_exact_preservation() {
    use gmeow_logic_compile::frontend::parse_logic_str;

    // The one authored fixture — the same file the `lang:` slice conformance harness
    // validates — so the RDF AST and the native IR can never silently drift apart.
    let fixture = include_str!(
        "../../../../slices/grounding/lang/tests/conformance-fixtures/meaning-cats-chase-mice.ttl"
    );
    let (program, diagnostics) = parse_logic_str(fixture, None).expect("fixture parses");
    assert!(
        !diagnostics.iter().any(|d| d.code == "MALFORMED_FORMULA"),
        "the flagship formula AST must reconstruct cleanly, got: {diagnostics:?}"
    );

    // The front-end lifted the sentence's denoted formula as a top-level assertion.
    assert_eq!(
        program.formulas.len(),
        1,
        "exactly the one top-level flagship formula is lifted: {:?}",
        program.formulas
    );

    // The native reasoner CONSUMES it: the full-FOL AST is clausified to an evaluable rule.
    let out = lower_formulas(&program);
    assert_eq!(
        out.rules.len(),
        1,
        "the flagship formula lowers to exactly one evaluable Horn rule: {:?}",
        out.rules
    );

    // Evaluation shape: head is the binary chase relation, body is the two type memberships.
    let rule = &out.rules[0];
    assert!(
        rule.head.predicate.ends_with("typeChase"),
        "the derived head is the chase predication, got: {}",
        rule.head.predicate
    );
    assert_eq!(
        rule.body.len(),
        2,
        "two body atoms (the cat and mouse type memberships): {:?}",
        rule.body
    );
    assert!(
        rule.body
            .iter()
            .all(|a| a.predicate.ends_with("instanceOf")),
        "each body atom is a HiLog type membership: {:?}",
        rule.body
    );

    // Per-stage preservation: the `logic:` → relational-core evaluation is EXACT (no residue),
    // matching the fixture's asserted `logic:ExactPreservation` on the denotation composition.
    assert!(
        out.preservation.unsupported_constructs.is_empty(),
        "nothing is carried as residue: {:?}",
        out.preservation.unsupported_constructs
    );
    assert!(
        out.preservation
            .polarities
            .contains(&PreservationKind::Exact),
        "the flagship formula lowers with exact preservation: {:?}",
        out.preservation.polarities
    );
}

/// The slice's typed-IR example is the source authority for the fixed ternary atom. Parse that
/// exact RDF formula and drive the production relational-core adapter: the result must be one
/// shared existential tuple reifier whose ordered binary edges preserve relation and arguments.
///
/// This closes the evidence gap a hand-authored `betweenTuple` could not: changing the fixture's
/// relation, argument kind/order, or the compiler's reification recipe now fails at the real
/// producer boundary.
#[test]
fn typed_ir_fixture_ternary_formula_legalizes_through_the_real_adapter() {
    use gmeow_logic_compile::frontend::parse_logic_str;

    const EX: &str = "https://blackcatinformatics.ca/gmeow/examples/logic/";
    let fixture = include_str!("../../../../slices/grounding/logic/examples/typed-ir.ttl");
    let (program, diagnostics) = parse_logic_str(fixture, None).expect("typed-IR fixture parses");
    assert!(
        !diagnostics.iter().any(|d| d.code == "MALFORMED_FORMULA"),
        "every authored typed-IR formula must reconstruct cleanly: {diagnostics:?}"
    );

    let relation = Term::Iri(format!("{EX}between"));
    let between = program
        .formulas
        .iter()
        .find(|formula| {
            matches!(
                formula,
                Formula::Atom {
                    relation: candidate,
                    ..
                } if candidate == &relation
            )
        })
        .expect("the source fixture carries its between formula")
        .clone();
    let Formula::Atom {
        relation: parsed_relation,
        args,
    } = &between
    else {
        unreachable!("selected by Formula::Atom relation")
    };
    assert_eq!(parsed_relation, &relation, "the relation IRI is preserved");
    assert_eq!(
        args,
        &vec![
            Term::Iri(format!("{EX}Alice")),
            Term::Iri(format!("{EX}Bob")),
            Term::Iri(format!("{EX}Carol")),
        ],
        "termIndex reconstructs the exact Alice/Bob/Carol argument order"
    );

    let out = lower_formulas(&program_with(vec![between]));
    assert!(
        out.rules.is_empty(),
        "a ternary derivation belongs to the conjunctive-head chase lane"
    );
    assert_eq!(
        out.nary_head_rules.len(),
        1,
        "one source ternary atom yields one existential tuple rule"
    );
    assert!(
        out.preservation.unsupported_constructs.is_empty(),
        "the fixed-arity ternary formula is legal, not residue: {:?}",
        out.preservation.unsupported_constructs
    );
    assert!(
        out.preservation
            .polarities
            .contains(&PreservationKind::Exact),
        "fixed-arity reification is exact"
    );

    let lowered = &out.nary_head_rules[0];
    assert!(lowered.body.is_empty(), "a ground assertion has no body");
    assert_eq!(
        lowered.head.len(),
        4,
        "the tuple has one relation-typing edge plus three positional edges"
    );
    let reifier = lowered.head[0].subject.clone();
    assert!(
        matches!(reifier, EvalTerm::Var(ref name) if name.starts_with("?naryH")),
        "the shared tuple node is the content-addressed existential reifier: {reifier:?}"
    );
    assert!(
        lowered.head.iter().all(|atom| atom.subject == reifier),
        "all four edges must share one tuple reifier: {:?}",
        lowered.head
    );
    assert_eq!(lowered.head[0].predicate, format!("{LOGIC}instanceOf"));
    assert_eq!(
        lowered.head[0].object,
        EvalTerm::ConstNamed(format!("{EX}between"))
    );
    for (index, expected) in ["Alice", "Bob", "Carol"].iter().enumerate() {
        let atom = &lowered.head[index + 1];
        assert_eq!(atom.predicate, format!("{LOGIC}naryArg{index}"));
        assert_eq!(atom.object, EvalTerm::ConstNamed(format!("{EX}{expected}")));
    }
}
