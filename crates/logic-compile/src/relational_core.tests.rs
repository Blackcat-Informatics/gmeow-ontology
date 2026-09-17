// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use crate::ir::{ContextualScope, LogicAxiom, LogicRule};

#[test]
fn formula_and_compact_lowering_share_complete_native_body_order() {
    let atom = |subject, predicate, object| {
        Formula::atom(
            Term::iri(predicate).unwrap(),
            vec![Term::var(subject).unwrap(), object],
        )
        .unwrap()
    };
    let head = atom("long_variable", "urn:result", Term::var("a").unwrap());
    let body = vec![
        atom("long_variable", "urn:p", Term::var("a").unwrap()),
        atom(
            "a",
            "urn:q",
            Term::rdf_literal(purrdf::RdfLiteral::typed(
                "1",
                "http://www.w3.org/2001/XMLSchema#integer",
            ))
            .unwrap(),
        ),
        atom(
            "a",
            "urn:q",
            Term::rdf_literal(purrdf::RdfLiteral::language_tagged("1", "en")).unwrap(),
        ),
    ];
    let rule = LogicRule::new(
        head.as_horn_axiom().unwrap(),
        body.iter()
            .map(|atom| atom.as_horn_axiom().unwrap())
            .collect(),
        vec![],
        ContextualScope::default(),
    );
    let compact = lower_program(&LogicProgram::new(vec![], vec![rule], vec![], None));
    let formula = Formula::Forall {
        vars: vec!["long_variable".to_owned(), "a".to_owned()],
        body: Box::new(Formula::Implies(
            Box::new(Formula::And(body)),
            Box::new(head),
        )),
    };
    let expanded = lower_program_with_formulas(
        &LogicProgram::new(vec![], vec![], vec![], None).with_formulas(vec![formula]),
    );
    assert!(expanded.residue.is_empty(), "{:?}", expanded.residue);
    assert_eq!(compact.rules.len(), 1);
    assert_eq!(expanded.rules, compact.rules);
    assert_eq!(
        expanded.content_key().unwrap(),
        compact.content_key().unwrap()
    );
}

fn ax(s: &str, p: &str, o: &str, o_lit: bool, negated: bool) -> LogicAxiom {
    LogicAxiom::new(
        s,
        p,
        if o_lit {
            crate::ir::AtomicTerm::Literal(purrdf::RdfLiteral::simple(o))
        } else {
            crate::ir::AtomicTerm::resource(o)
        },
        negated,
        ContextualScope::default(),
    )
    .expect("axiom")
}

/// A clean Horn program: two ground type axioms + one transitive-style rule. The
/// whole rule set is in the binary-Horn fragment, so the lowering is `{exact}`.
pub(super) fn horn_program() -> LogicProgram {
    let animal = "https://blackcatinformatics.ca/gmeow/Animal";
    let cat = "https://blackcatinformatics.ca/gmeow/Cat";
    let kind = "https://blackcatinformatics.ca/logic/Kind";
    let sc = "http://www.w3.org/2000/01/rdf-schema#subClassOf";
    // ?x sc ?z :- ?x sc ?y, ?y sc ?z .
    let rule = LogicRule::new(
        ax("?x", sc, "?z", false, false),
        vec![
            ax("?x", sc, "?y", false, false),
            ax("?y", sc, "?z", false, false),
        ],
        vec![],
        ContextualScope::default(),
    );
    LogicProgram::new(
        vec![
            ax(animal, RDF_TYPE, kind, false, false),
            ax(cat, sc, animal, false, false),
        ],
        vec![rule],
        vec![],
        Some("https://blackcatinformatics.ca/logic/test".to_owned()),
    )
}

#[test]
fn horn_floor_lowers_exactly() {
    let lowered = lower_program(&horn_program());
    assert!(
        lowered.residue.is_empty(),
        "main's Horn rules lower with no residue: {:?}",
        lowered.residue
    );
    assert_eq!(
        lowered.preservation(),
        PreservationKind::Exact,
        "a fully-supported lowering is {{exact}}"
    );
    assert_eq!(lowered.facts.len(), 2, "both ground axioms lower to facts");
    assert_eq!(lowered.rules.len(), 1, "the Horn rule lowers");
    // The rule is a binary clause with a 2-atom body.
    assert_eq!(lowered.rules[0].body.len(), 2);
}

#[test]
fn unsupported_construct_becomes_flagged_residue_and_sound_under() {
    // The full-FOL lowering's seam: a non-Horn construct (e.g. a disjunctive head)
    // arrives as flagged residue. Assert it is CARRIED (not dropped) and the claim
    // drops to {sound-under}.
    let lowered = lower_with_residue(
        &horn_program(),
        ["disjunctive head: clause is not Horn (>1 positive literal)"],
    );
    assert_eq!(
        lowered.preservation(),
        PreservationKind::SoundUnder,
        "a carried residue makes the claim sound-under, not a false exact"
    );
    assert!(
        lowered
            .residue
            .iter()
            .any(|r| r.reason.contains("disjunctive head")),
        "the unsupported construct is carried and flagged, never dropped"
    );
    // The Horn fragment still lowered alongside the residue.
    assert_eq!(lowered.rules.len(), 1, "the legal fragment still lowers");
}

#[test]
fn projection_round_trips_through_the_graph() {
    // Dual carriage: the typed program → graph → typed program is identity (the
    // cache-hit re-derivation the handle relies on).
    let lowered = lower_program(&horn_program());
    let nt = project_relational_core(&lowered);
    let ds = purrdf::parse_dataset(nt.as_bytes(), "application/n-triples", None)
        .expect("parse projection");
    let re_derived = parse_relational_core(ds.as_ref()).expect("re-derive");
    assert_eq!(
        re_derived.content_key().unwrap(),
        lowered.content_key().unwrap(),
        "the graph round-trips to a content-key-equal program"
    );
    assert_eq!(re_derived, lowered, "the round-trip is value-identical");
}

#[test]
fn residue_round_trips_through_the_graph() {
    // A {sound-under} program (carrying residue) also round-trips: the residue
    // survives the projection and the preservation claim is re-derived correctly.
    let lowered = lower_with_residue(
        &horn_program(),
        [
            "disjunctive head: clause is not Horn (>1 positive literal)",
            "sequence marker (variadic) is not representable in the relational core",
        ],
    );
    let nt = project_relational_core(&lowered);
    let ds = purrdf::parse_dataset(nt.as_bytes(), "application/n-triples", None)
        .expect("parse projection");
    let re_derived = parse_relational_core(ds.as_ref()).expect("re-derive");
    assert_eq!(re_derived.preservation(), PreservationKind::SoundUnder);
    assert_eq!(re_derived.residue.len(), 2, "both residue rows survive");
    assert_eq!(re_derived, lowered);
}

#[test]
fn projection_is_byte_deterministic() {
    let lowered = lower_program(&horn_program());
    let a = project_relational_core(&lowered);
    let b = project_relational_core(&lower_program(&horn_program()));
    assert_eq!(
        a, b,
        "the projection is a byte-stable function of the program"
    );
    // Sorted N-Triples: every non-empty line ends with " ." and the whole is sorted.
    let lines: Vec<&str> = a.lines().collect();
    let mut sorted = lines.clone();
    sorted.sort_unstable();
    assert_eq!(lines, sorted, "projection lines are sorted (deterministic)");
}

/// A literal whose lexical form begins with '?' must be
/// classified as a Literal, not as a Var, regardless of variable-syntax heuristic.
#[test]
fn declared_literal_starting_with_question_mark_is_not_a_var() {
    let term = RcTerm::Literal(RdfLiteral::simple("?sparql-like-literal"));
    assert!(
        matches!(term, RcTerm::Literal(_)),
        "is_literal=true MUST win over variable-syntax heuristic; got {term:?}"
    );
    // Countercheck: without is_literal=true, '?' still produces Var.
    let var_term = RcTerm::resource("?x");
    assert!(
        matches!(var_term, RcTerm::Var(_)),
        "without is_literal flag, '?' prefix still produces Var"
    );
}

/// A rule with a repeated body atom (same content at two
/// positions) must round-trip without rcIndex collision on a shared node IRI.
#[test]
fn repeated_body_atom_round_trips_with_distinct_indices() {
    let sc = "http://www.w3.org/2000/01/rdf-schema#subClassOf";
    // ?x sc ?z :- ?x sc ?y, ?x sc ?y .   (same atom twice — pathological but legal)
    let rule = LogicRule::new(
        ax("?x", sc, "?z", false, false),
        vec![
            ax("?x", sc, "?y", false, false),
            ax("?x", sc, "?y", false, false), // duplicate
        ],
        vec![],
        ContextualScope::default(),
    );
    let program = LogicProgram::new(vec![], vec![rule], vec![], None);
    let lowered = lower_program(&program);
    // Project then re-derive: must reconstruct 2 body atoms, not 1.
    let nt = project_relational_core(&lowered);
    let ds = purrdf::parse_dataset(nt.as_bytes(), "application/n-triples", None)
        .expect("parse projection");
    let re_derived = parse_relational_core(ds.as_ref()).expect("re-derive");
    assert_eq!(
        re_derived.rules[0].body.len(),
        2,
        "both body occurrences must survive the round-trip (no rcIndex collision)"
    );
    assert_eq!(re_derived, lowered, "full round-trip value equality");
}

#[test]
fn malformed_graph_hard_fails() {
    // A fact node missing its rcPredicate edge is a corrupt projection — the
    // reverse parser HARD-fails rather than re-deriving a partial program.
    let prog = program_iri();
    let fact = format!("{LOGIC_NAMESPACE}relational-core/fact/deadbeef");
    let nt = format!(
        "<{prog}> <{}> <{}> .\n<{prog}> <{}> <{fact}> .\n<{fact}> <{}> <{}> .\n",
        RDF_TYPE,
        class_program(),
        p_has_fact(),
        RDF_TYPE,
        class_fact(),
    );
    let ds = purrdf::parse_dataset(nt.as_bytes(), "application/n-triples", None)
        .expect("parse malformed");
    let err = parse_relational_core(ds.as_ref()).expect_err("malformed graph must hard-fail");
    assert!(err.message().contains("missing rcSubject"), "got: {err}");
}

// ── Full-FOL formula lowering: the seam wiring the clausifier into the lane ──

fn firi(s: &str) -> String {
    format!("https://blackcatinformatics.ca/gmeow/{s}")
}
fn fatom(pred: &str, args: Vec<Term>) -> Formula {
    Formula::atom(Term::Iri(firi(pred)), args).expect("first-order atom")
}
fn fvar(n: &str) -> Term {
    Term::Var(n.to_owned())
}
fn transitivity_formula() -> Formula {
    // ∀x y z. (sc(x,y) ∧ sc(y,z)) → sc(x,z)
    let body = Formula::And(vec![
        fatom("scA", vec![fvar("x"), fvar("y")]),
        fatom("scA", vec![fvar("y"), fvar("z")]),
    ]);
    let head = fatom("scA", vec![fvar("x"), fvar("z")]);
    Formula::Forall {
        vars: vec!["x".into(), "y".into(), "z".into()],
        body: Box::new(Formula::Implies(Box::new(body), Box::new(head))),
    }
}

/// A Horn-expressible formula (a universally-closed implication with a conjunctive body)
/// lowers to exactly one Horn RcRule, leaves no residue, and keeps the lane {exact}.
#[test]
fn horn_expressible_formula_lowers_to_rcrule_exact() {
    let program =
        LogicProgram::new(vec![], vec![], vec![], None).with_formulas(vec![transitivity_formula()]);
    let lowered = lower_program_with_formulas(&program);
    assert!(
        lowered.residue.is_empty(),
        "a Horn-expressible formula leaves no residue: {:?}",
        lowered.residue
    );
    assert_eq!(lowered.preservation(), PreservationKind::Exact);
    assert_eq!(
        lowered.rules.len(),
        1,
        "the implication lowers to one Horn rule"
    );
    assert_eq!(lowered.rules[0].body.len(), 2, "both body atoms survive");
}

/// A disjunctive head is beyond Horn: carried + flagged, preservation drops to sound-under,
/// and the residue note names BOTH the reason and the Disjunctive FormulaShape tag.
#[test]
fn disjunctive_head_is_carried_and_named() {
    let f = Formula::Forall {
        vars: vec!["x".into()],
        body: Box::new(Formula::Or(vec![
            fatom("pA", vec![fvar("x"), Term::Iri(firi("a"))]),
            fatom("qA", vec![fvar("x"), Term::Iri(firi("b"))]),
        ])),
    };
    let program = LogicProgram::new(vec![], vec![], vec![], None).with_formulas(vec![f]);
    let lowered = lower_program_with_formulas(&program);
    assert_eq!(lowered.preservation(), PreservationKind::SoundUnder);
    assert!(lowered.rules.is_empty(), "nothing Horn-expressible here");
    assert_eq!(lowered.residue.len(), 1);
    let r = &lowered.residue[0].reason;
    assert!(r.contains("disjunctive head"), "names the reason: {r}");
    assert!(r.contains("Disjunctive"), "names the FormulaShape tag: {r}");
}

/// An unrestricted universal has no body binding its domain here. It cannot be
/// treated as a range-restricted TGD or mis-lowered to a global constant.
#[test]
fn existential_with_unbound_universal_is_carried_and_named() {
    let f = Formula::Forall {
        vars: vec!["x".into()],
        body: Box::new(Formula::Exists {
            vars: vec!["y".into()],
            body: Box::new(fatom("rA", vec![fvar("x"), fvar("y")])),
        }),
    };
    let program = LogicProgram::new(vec![], vec![], vec![], None).with_formulas(vec![f]);
    let lowered = lower_program_with_formulas(&program);
    assert_eq!(lowered.preservation(), PreservationKind::SoundUnder);
    assert!(lowered.rules.is_empty());
    assert_eq!(lowered.residue.len(), 1);
    assert!(
        lowered.residue[0].reason.contains("Quantified"),
        "names the Quantified tag: {}",
        lowered.residue[0].reason
    );
}

/// A fixed-arity ternary atom in a rule BODY is now evaluable: it reifies into a
/// conjunction of binary atoms over a fresh reifier variable, so the rule is
/// carried (not residue) and preservation is Exact.
#[test]
fn nary_body_atom_lowers_to_reified_rules() {
    // ∀x y z. relA(x,y,z) → pA(x,z)
    let body = fatom("relA", vec![fvar("x"), fvar("y"), fvar("z")]);
    let head = fatom("pA", vec![fvar("x"), fvar("z")]);
    let f = Formula::Forall {
        vars: vec!["x".into(), "y".into(), "z".into()],
        body: Box::new(Formula::Implies(Box::new(body), Box::new(head))),
    };
    let program = LogicProgram::new(vec![], vec![], vec![], None).with_formulas(vec![f]);
    let lowered = lower_program_with_formulas(&program);
    assert!(
        lowered.residue.is_empty(),
        "a fixed-arity n-ary body atom is evaluable, not residue: {:?}",
        lowered.residue
    );
    assert_eq!(lowered.preservation(), PreservationKind::Exact);
    assert_eq!(lowered.rules.len(), 1, "the implication lowers to one rule");
    // The ternary body atom reifies into instanceOf + naryArg0 + naryArg1 + naryArg2.
    assert_eq!(
        lowered.rules[0].body.len(),
        4,
        "ternary body atom → 4 reified binary atoms"
    );
    let preds: Vec<&str> = lowered.rules[0]
        .body
        .iter()
        .map(|a| a.predicate.as_str())
        .collect();
    assert!(preds.iter().any(|p| p.ends_with("instanceOf")), "{preds:?}");
    for i in 0..3 {
        assert!(
            preds.iter().any(|p| p.ends_with(&format!("naryArg{i}"))),
            "missing naryArg{i}: {preds:?}"
        );
    }
    // The reifier variable is shared across the whole reified conjunction.
    let reifier_subjects: BTreeSet<&RcTerm> =
        lowered.rules[0].body.iter().map(|a| &a.subject).collect();
    assert_eq!(
        reifier_subjects.len(),
        1,
        "all reified atoms share one reifier variable: {reifier_subjects:?}"
    );
}

/// Two DISTINCT same-relation ternary body atoms must NOT be unified: their reifier
/// variables are keyed on the raw syntactic atom (authored variable names intact),
/// so `rel(?a,?b,?c)` and `rel(?d,?e,?f)` get different reifiers. Keying on the
/// alpha-normalized content_key would unsoundly collapse them into one tuple.
#[test]
fn distinct_nary_body_atoms_do_not_collide() {
    // ∀ a b c d e f. relA(a,b,c) ∧ relA(d,e,f) → pA(a,d)
    let body = Formula::And(vec![
        fatom("relA", vec![fvar("a"), fvar("b"), fvar("c")]),
        fatom("relA", vec![fvar("d"), fvar("e"), fvar("f")]),
    ]);
    let head = fatom("pA", vec![fvar("a"), fvar("d")]);
    let f = Formula::Forall {
        vars: vec![
            "a".into(),
            "b".into(),
            "c".into(),
            "d".into(),
            "e".into(),
            "f".into(),
        ],
        body: Box::new(Formula::Implies(Box::new(body), Box::new(head))),
    };
    let program = LogicProgram::new(vec![], vec![], vec![], None).with_formulas(vec![f]);
    let lowered = lower_program_with_formulas(&program);
    assert!(lowered.residue.is_empty(), "{:?}", lowered.residue);
    assert_eq!(lowered.rules.len(), 1);
    // Two distinct ternary atoms → two distinct reifier variables (2 × 4 = 8 atoms).
    assert_eq!(
        lowered.rules[0].body.len(),
        8,
        "two distinct ternary atoms reify without collision"
    );
    let reifier_vars: BTreeSet<&RcTerm> = lowered.rules[0]
        .body
        .iter()
        .filter(|a| a.predicate.ends_with("instanceOf"))
        .map(|a| &a.subject)
        .collect();
    assert_eq!(
        reifier_vars.len(),
        2,
        "distinct atoms must NOT share a reifier variable: {reifier_vars:?}"
    );
}

/// A genuine sequence-marker atom (`rA(x, ...rest)`) is truly variadic — it stays
/// carried as residue and tagged Variadic; only *fixed*-arity atoms reify.
#[test]
fn sequence_marker_atom_is_carried_and_named() {
    let f = fatom(
        "rA",
        vec![fvar("x"), Term::SequenceMarker("rest".to_owned())],
    );
    let program = LogicProgram::new(vec![], vec![], vec![], None).with_formulas(vec![f]);
    let lowered = lower_program_with_formulas(&program);
    assert_eq!(lowered.preservation(), PreservationKind::SoundUnder);
    assert!(lowered.rules.is_empty());
    assert_eq!(lowered.residue.len(), 1);
    let r = &lowered.residue[0].reason;
    assert!(r.contains("sequence marker"), "names the reason: {r}");
    assert!(r.contains("Variadic"), "names the Variadic tag: {r}");
}

/// A fixed-arity ternary atom in the rule HEAD now DERIVES a reified tuple: it lowers to
/// a conjunctive-head existential rule (`instanceOf(R, Rel)` head + `naryArg{i}(R, aᵢ)`
/// conjuncts over a fresh existential reifier `R`), carried (not residue), preservation
/// Exact. `matMul` is ternary in the BODY (reified) and `mul` ternary in the HEAD.
#[test]
fn nary_head_atom_derives_a_reified_tuple() {
    // ∀A B AB dA dB dAB. matMul(A,B,AB) ∧ det(A,dA) ∧ det(B,dB) ∧ det(AB,dAB) → mul(dA,dB,dAB)
    let body = Formula::And(vec![
        fatom("matMul", vec![fvar("A"), fvar("B"), fvar("AB")]),
        fatom("det", vec![fvar("A"), fvar("dA")]),
        fatom("det", vec![fvar("B"), fvar("dB")]),
        fatom("det", vec![fvar("AB"), fvar("dAB")]),
    ]);
    let head = fatom("mul", vec![fvar("dA"), fvar("dB"), fvar("dAB")]);
    let f = Formula::Forall {
        vars: vec![
            "A".into(),
            "B".into(),
            "AB".into(),
            "dA".into(),
            "dB".into(),
            "dAB".into(),
        ],
        body: Box::new(Formula::Implies(Box::new(body), Box::new(head))),
    };
    let program = LogicProgram::new(vec![], vec![], vec![], None).with_formulas(vec![f]);
    let lowered = lower_program_with_formulas(&program);
    assert!(
        lowered.residue.is_empty(),
        "a range-restricted n-ary head is evaluable, not residue: {:?}",
        lowered.residue
    );
    assert_eq!(lowered.preservation(), PreservationKind::Exact);
    assert_eq!(lowered.rules.len(), 1, "the implication lowers to one rule");
    let rule = &lowered.rules[0];
    // Head is instanceOf(R, mul); the tail is naryArg0..2(R, dA/dB/dAB).
    assert!(
        rule.head.predicate.ends_with("instanceOf"),
        "head types the reifier: {}",
        rule.head.predicate
    );
    assert!(
        matches!(&rule.head.object, RcTerm::Iri(i) if i.ends_with("mul")),
        "head instanceOf object is the relation IRI: {:?}",
        rule.head.object
    );
    assert_eq!(
        rule.head_conjuncts.len(),
        3,
        "ternary head → 3 naryArg conjuncts: {:?}",
        rule.head_conjuncts
    );
    for (i, conjunct) in rule.head_conjuncts.iter().enumerate() {
        assert!(
            conjunct.predicate.ends_with(&format!("naryArg{i}")),
            "conjunct {i} predicate: {}",
            conjunct.predicate
        );
    }
    // The reifier `R` is one fresh existential var shared across head + every conjunct,
    // and it is NOT bound by the body (it is the invented tuple node).
    let reifier = &rule.head.subject;
    assert!(matches!(reifier, RcTerm::Var(v) if v.starts_with("?naryH")));
    assert!(
        rule.head_conjuncts.iter().all(|c| &c.subject == reifier),
        "all conjuncts share the reifier subject"
    );
    let body_bound = body_bound_vars(&rule.body);
    if let RcTerm::Var(v) = reifier {
        assert!(
            !body_bound.contains(v),
            "the existential reifier is not body-bound"
        );
    }
}

/// A head variable the body does not bind is a non-range-restricted existential (unsafe):
/// the clause is carried as residue tagged with the "not bound by the body" reason, never
/// lowered to an unsafe rule.
#[test]
fn nary_head_with_unbound_arg_is_residue() {
    // ∀A B AB dA dB. matMul(A,B,AB) ∧ det(A,dA) ∧ det(B,dB) → mul(dA, dB, dAB)
    // `dAB` appears only in the head — the body binds nothing to it.
    let body = Formula::And(vec![
        fatom("matMul", vec![fvar("A"), fvar("B"), fvar("AB")]),
        fatom("det", vec![fvar("A"), fvar("dA")]),
        fatom("det", vec![fvar("B"), fvar("dB")]),
    ]);
    let head = fatom("mul", vec![fvar("dA"), fvar("dB"), fvar("dAB")]);
    let f = Formula::Forall {
        vars: vec![
            "A".into(),
            "B".into(),
            "AB".into(),
            "dA".into(),
            "dB".into(),
            "dAB".into(),
        ],
        body: Box::new(Formula::Implies(Box::new(body), Box::new(head))),
    };
    let program = LogicProgram::new(vec![], vec![], vec![], None).with_formulas(vec![f]);
    let lowered = lower_program_with_formulas(&program);
    assert_eq!(lowered.preservation(), PreservationKind::SoundUnder);
    assert!(lowered.rules.is_empty(), "an unsafe head does not lower");
    assert_eq!(lowered.residue.len(), 1);
    assert!(
        lowered.residue[0].reason.contains("not bound by the body"),
        "the residue names the range-restriction failure: {}",
        lowered.residue[0].reason
    );
}

/// An n-ary head-derivation rule (carrying `head_conjuncts`) survives the lane's RDF
/// projection round-trip value-identically — the conjuncts re-derive in order.
#[test]
fn nary_head_rule_round_trips_through_the_graph() {
    let body = Formula::And(vec![
        fatom("matMul", vec![fvar("A"), fvar("B"), fvar("AB")]),
        fatom("det", vec![fvar("A"), fvar("dA")]),
        fatom("det", vec![fvar("B"), fvar("dB")]),
        fatom("det", vec![fvar("AB"), fvar("dAB")]),
    ]);
    let head = fatom("mul", vec![fvar("dA"), fvar("dB"), fvar("dAB")]);
    let f = Formula::Forall {
        vars: vec![
            "A".into(),
            "B".into(),
            "AB".into(),
            "dA".into(),
            "dB".into(),
            "dAB".into(),
        ],
        body: Box::new(Formula::Implies(Box::new(body), Box::new(head))),
    };
    let program = LogicProgram::new(vec![], vec![], vec![], None).with_formulas(vec![f]);
    let lowered = lower_program_with_formulas(&program);
    assert_eq!(lowered.rules.len(), 1);
    assert_eq!(lowered.rules[0].head_conjuncts.len(), 3);
    let nt = project_relational_core(&lowered);
    let ds = purrdf::parse_dataset(nt.as_bytes(), "application/n-triples", None)
        .expect("parse projection");
    let re_derived = parse_relational_core(ds.as_ref()).expect("re-derive");
    assert_eq!(
        re_derived, lowered,
        "an n-ary head-derivation rule round-trips value-identically"
    );
}

/// A relational-core projection whose reified head-conjunct positional `rcIndex` values are
/// corrupted into a duplicate (non-contiguous) set must HARD-FAIL on re-derivation: this
/// path re-reads an externally serializable projection, and a silently mis-positioned
/// conjunction would mint a wrong content-addressed reifier at chase time (no-optionality).
#[test]
fn nary_head_rule_with_duplicate_rc_index_is_rejected() {
    let body = Formula::And(vec![
        fatom("matMul", vec![fvar("A"), fvar("B"), fvar("AB")]),
        fatom("det", vec![fvar("A"), fvar("dA")]),
        fatom("det", vec![fvar("B"), fvar("dB")]),
        fatom("det", vec![fvar("AB"), fvar("dAB")]),
    ]);
    let head = fatom("mul", vec![fvar("dA"), fvar("dB"), fvar("dAB")]);
    let f = Formula::Forall {
        vars: vec![
            "A".into(),
            "B".into(),
            "AB".into(),
            "dA".into(),
            "dB".into(),
            "dAB".into(),
        ],
        body: Box::new(Formula::Implies(Box::new(body), Box::new(head))),
    };
    let program = LogicProgram::new(vec![], vec![], vec![], None).with_formulas(vec![f]);
    let lowered = lower_program_with_formulas(&program);
    let nt = project_relational_core(&lowered);
    // Rewrite the head conjunct at positional index 2 to a DUPLICATE of index 0. Only the
    // one `/headconjunct/` node carries rcIndex "2" (body atoms live under `/body/`).
    let corrupted: String = nt
        .lines()
        .map(|line| {
            if line.contains("/headconjunct/")
                && line.contains("rcIndex")
                && line.contains("\"2\"^^")
            {
                line.replace("\"2\"^^", "\"0\"^^")
            } else {
                line.to_owned()
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    assert_ne!(
        corrupted, nt,
        "the corruption must have hit a head-conjunct rcIndex line"
    );
    let ds = purrdf::parse_dataset(corrupted.as_bytes(), "application/n-triples", None)
        .expect("parse corrupted projection");
    let err = parse_relational_core(ds.as_ref()).expect_err("duplicate rcIndex must be rejected");
    assert!(
        err.message()
            .contains("non-contiguous or duplicate head-conjunct"),
        "the error names the malformed head-conjunct indices: {err}"
    );
}

/// The Horn-expressible formula fragment survives the lane's RDF projection round-trip,
/// so the carrier and the typed handle stay value-identical (dual carriage holds).
#[test]
fn formula_derived_rules_round_trip_through_the_graph() {
    let program =
        LogicProgram::new(vec![], vec![], vec![], None).with_formulas(vec![transitivity_formula()]);
    let lowered = lower_program_with_formulas(&program);
    let nt = project_relational_core(&lowered);
    let ds = purrdf::parse_dataset(nt.as_bytes(), "application/n-triples", None)
        .expect("parse projection");
    let re_derived = parse_relational_core(ds.as_ref()).expect("re-derive");
    assert_eq!(
        re_derived, lowered,
        "formula-derived rules round-trip value-identically"
    );
}
