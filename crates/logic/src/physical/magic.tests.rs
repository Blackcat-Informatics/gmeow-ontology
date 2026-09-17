// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use crate::physical::plan::Parsed;
use crate::query_ir::parse_query_program;
use crate::reference_resolver;
use crate::seam::WorldFactSnapshot;
use crate::store::WorldStore;

const W: &str = "http://logic.test/world/magic";
const PROFILE: &str = "https://blackcatinformatics.ca/logic/PositiveHornProfile";
const BASE: &str = "https://example.org/";

/// Drive the type-state plan pipeline for a stratifiable test program — the only path
/// to the `Executable` the backward `evaluate` executor accepts.
fn exe(rules: &[EvalRule]) -> crate::physical::plan::Executable {
    Parsed::uncached(rules)
        .stratify()
        .expect("stratifiable test program")
        .plan()
        .into_executable()
}

fn make_world(triples: &[(&str, &str, &str)]) -> (WorldStore, String) {
    let store = WorldStore::new();
    for (s, p, o) in triples {
        store.insert_quad(W, s, p, o);
    }
    (store, W.to_owned())
}

fn decided(outcome: NativeOutcome<AnswerSet>) -> AnswerSet {
    match outcome {
        NativeOutcome::Decided(a) => a,
        NativeOutcome::Unsupported(k) => panic!("expected Decided, got Unsupported({k:?})"),
    }
}

// ── Mode analysis over a dimension/quantity `is` generator ───────────────────
//
// The mode passes (`is_generator_reaches_head`, `negated_body_flounders`) match
// `QBuiltin::Is { target: QTerm::Var(_), .. }` STRUCTURALLY and UNIFORMLY — they
// never inspect the operator or the operand VALUE type. So a dimension-composition
// (`Mul` over two dimensions) or dimensioned-quantity `is` generator is recognized
// exactly as the i64 case is: its free target is range-restricting, and a partially
// bound one (a negated variable nothing binds) still flounders. These tests pin
// that value-type independence.

/// An `is` generator over the dimension algebra (`?D is ?A * ?B`), structurally
/// identical to the i64 form the mode passes already recognize.
fn dim_is_generator() -> QBuiltin {
    QBuiltin::Is {
        target: QTerm::Var("?D".to_owned()),
        lhs: QTerm::Var("?A".to_owned()),
        op: crate::query_ir::ArithOp::Mul,
        rhs: QTerm::Var("?B".to_owned()),
    }
}

/// The i64 sibling (`?D is ?A + ?B`) — a parity control proving the mode passes are
/// operator- and value-type-independent.
fn int_is_generator() -> QBuiltin {
    QBuiltin::Is {
        target: QTerm::Var("?D".to_owned()),
        lhs: QTerm::Var("?A".to_owned()),
        op: crate::query_ir::ArithOp::Add,
        rhs: QTerm::Var("?B".to_owned()),
    }
}

fn neg(subject: EvalTerm, predicate: &str, object: EvalTerm) -> EvalAtom {
    EvalAtom {
        subject,
        predicate: predicate.to_owned(),
        object,
        negated: true,
    }
}

#[test]
fn answer_projection_enforces_lowered_numeric_goal_constants() {
    let goal = QAtom {
        pred: "urn:answer:p".into(),
        args: vec![QTerm::Var("x".into()), QTerm::Num(42)],
    };
    let goal = atom_of(&goal).expect("numeric goal admission");
    let fact = |subject, number: &str| Fact {
        subject: TermValue::iri(subject),
        predicate: "urn:answer:p".into(),
        object: TermValue::typed_literal(number, crate::physical::XSD_INTEGER),
    };
    let answers = project_answers(&[fact("urn:match", "42"), fact("urn:miss", "43")], &goal);
    assert_eq!(
        answers,
        vec![BTreeMap::from([("x".into(), "<urn:match>".into())])]
    );
}

#[test]
fn answer_projection_repeated_variables_compare_native_terms_before_rendering() {
    let plain = TermValue::simple_literal("a");
    let distinct = TermValue::Literal {
        lexical_form: "a".into(),
        datatype: gmeow_term_arena::engine::RDF_LANG_STRING.into(),
        language: None,
        direction: None,
    };
    let goal = EvalAtom::positive(
        EvalTerm::Var("?x".into()),
        "urn:answer:p",
        EvalTerm::Var("?x".into()),
    );
    assert!(
        project_answer(
            &Fact {
                subject: plain.clone(),
                predicate: goal.predicate.clone(),
                object: distinct
            },
            &goal
        )
        .is_none()
    );
    assert_eq!(
        project_answer(
            &Fact {
                subject: plain.clone(),
                predicate: goal.predicate.clone(),
                object: plain
            },
            &goal
        ),
        Some(BTreeMap::from([("x".into(), "\"a\"".into())]))
    );
}

#[test]
fn dimension_is_generator_is_range_restricting_and_head_reaching() {
    // result(?X, ?D) :- dimA(?X, ?A), dimB(?X, ?B), ?D is ?A * ?B, \+ excluded(?X, ?D).
    // The composed dimension ?D range-restricts the negated atom (it is an `is`
    // target) and reaches the head — recognized identically to the i64 generator.
    let head = EvalAtom::positive(
        EvalTerm::var("?X"),
        &format!("{BASE}result"),
        EvalTerm::var("?D"),
    );
    let body = vec![
        EvalAtom::positive(
            EvalTerm::var("?X"),
            &format!("{BASE}dimA"),
            EvalTerm::var("?A"),
        ),
        EvalAtom::positive(
            EvalTerm::var("?X"),
            &format!("{BASE}dimB"),
            EvalTerm::var("?B"),
        ),
        neg(
            EvalTerm::var("?X"),
            &format!("{BASE}excluded"),
            EvalTerm::var("?D"),
        ),
    ];
    for generator in [dim_is_generator(), int_is_generator()] {
        let rule = EvalRule {
            numeric: Vec::new(),
            head: head.clone(),
            body: body.clone(),
            rule_iri: format!("{BASE}rule/compose"),
            distinct_pairs: vec![],
            builtins: vec![generator],
            reduction: None,
            constraint_tag: None,
        };
        // The `is` target binds ?D, so the negated atom is range-restricted.
        assert!(
            !negated_body_flounders(&rule.body, &rule.builtins),
            "an `is`-generated ?D range-restricts \\+ excluded(?X, ?D)"
        );
        // The generated ?D reaches the head (it is a head argument).
        assert!(
            is_generator_reaches_head(&rule),
            "?D is a head argument, so the value-generating `is` reaches the head"
        );
    }
}

#[test]
fn partially_bound_dimension_generator_still_flounders() {
    // result(?X, ?D) :- dimA(?X, ?A), ?D is ?A * ?B, \+ excluded(?D, ?W).
    // ?W is bound by NEITHER a positive atom NOR an `is` target, so the negated
    // atom flounders — the dimension generator's target ?D does not save it. The
    // i64 sibling flounders identically.
    let body = vec![
        EvalAtom::positive(
            EvalTerm::var("?X"),
            &format!("{BASE}dimA"),
            EvalTerm::var("?A"),
        ),
        neg(
            EvalTerm::var("?D"),
            &format!("{BASE}excluded"),
            EvalTerm::var("?W"),
        ),
    ];
    for generator in [dim_is_generator(), int_is_generator()] {
        assert!(
            negated_body_flounders(&body, &[generator]),
            "?W is bound by nothing, so \\+ excluded(?D, ?W) flounders"
        );
    }
}

/// A world with two SI-dimension EDB facts on `ex:a`: length (L¹) and time (T¹).
fn dimension_world() -> (WorldStore, String) {
    let dim_dt = "urn:gmeow:transport:dimension";
    let store = WorldStore::new();
    let length = TermValue::typed_literal("1/1,0/1,0/1,0/1,0/1,0/1,0/1", dim_dt);
    let time = TermValue::typed_literal("0/1,0/1,1/1,0/1,0/1,0/1,0/1", dim_dt);
    store
        .insert_quad_terms(
            W,
            TermValue::iri(format!("{BASE}a")),
            TermValue::iri(format!("{BASE}dimLen")),
            length,
        )
        .unwrap();
    store
        .insert_quad_terms(
            W,
            TermValue::iri(format!("{BASE}a")),
            TermValue::iri(format!("{BASE}dimTime")),
            time,
        )
        .unwrap();
    (store, W.to_owned())
}

#[test]
fn magic_dimension_composition_matches_reference_byte_identical() {
    // The bottom-up magic core and the top-down SLD oracle must agree BYTE-FOR-BYTE
    // on a dimension-composition rule: both route dimensioned operands through the
    // ONE shared evaluator, so the committed dimension transport surface is identical.
    let (store, world_nn) = dimension_world();
    let foreign = WorldFactSnapshot::from_world(&store, W, PROFILE).unwrap();
    let src = format!(
        ":- prefix(ex, '{BASE}').\n\
             ex:compose(X, D) :- ex:dimLen(X, A), ex:dimTime(X, B), D is A * B.\n\
             ?- ex:compose(X, D).\n"
    );
    let prog = parse_query_program(&src).unwrap();
    let budget = Budget::default();

    let native = decided(resolve_native(&foreign, &world_nn, &prog, &budget).unwrap());
    let reference = reference_resolver::resolve(&foreign, &world_nn, &prog, &budget).unwrap();

    assert_eq!(native.status, reference.status, "status parity");
    assert_eq!(
        native.bindings, reference.bindings,
        "magic == oracle on dimension composition: native {native:?} vs ref {reference:?}"
    );
    assert_eq!(native.bindings.len(), 1);
    assert_eq!(native.bindings[0]["X"], format!("<{BASE}a>"));
    assert_eq!(
        native.bindings[0]["D"],
        "\"1/1,0/1,1/1,0/1,0/1,0/1,0/1\"^^<urn:gmeow:transport:dimension>"
    );
}

// ── Test 1: non-recursive single-rule parity ─────────────────────────────────

#[test]
fn magic_non_recursive_matches_reference() {
    let (store, world_nn) = make_world(&[(
        &format!("{BASE}alice"),
        &format!("{BASE}parentOf"),
        &format!("{BASE}bob"),
    )]);
    let foreign = WorldFactSnapshot::from_world(&store, W, PROFILE).unwrap();

    let src = format!(
        ":- prefix(ex, '{BASE}').\n\
             ex:ancestorOf(X, Y) :- ex:parentOf(X, Y).\n\
             ?- ex:ancestorOf(ex:alice, Y).\n"
    );
    let prog = parse_query_program(&src).unwrap();
    let budget = Budget::default();

    let native = decided(resolve_native(&foreign, &world_nn, &prog, &budget).unwrap());
    let reference = reference_resolver::resolve(&foreign, &world_nn, &prog, &budget).unwrap();

    assert_eq!(native.status, reference.status, "status parity");
    assert_eq!(
        native.bindings, reference.bindings,
        "bottom-up magic-sets must equal top-down SLD: native {native:?} vs ref {reference:?}"
    );
    assert_eq!(native.bindings.len(), 1);
    assert_eq!(native.bindings[0]["Y"], format!("<{BASE}bob>"));
}

// ── Test 2: recursive transitive-closure parity (bottom-up == top-down) ──────

fn tc_program() -> QProgram {
    let src = format!(
        ":- prefix(ex, '{BASE}').\n\
             ex:ancestor(X, Y) :- ex:parentOf(X, Y).\n\
             ex:ancestor(X, Y) :- ex:parentOf(X, Z), ex:ancestor(Z, Y).\n\
             ?- ex:ancestor(ex:a, Y).\n"
    );
    parse_query_program(&src).unwrap()
}

fn tc_world() -> (WorldStore, String) {
    make_world(&[
        (
            &format!("{BASE}a"),
            &format!("{BASE}parentOf"),
            &format!("{BASE}b"),
        ),
        (
            &format!("{BASE}b"),
            &format!("{BASE}parentOf"),
            &format!("{BASE}c"),
        ),
        (
            &format!("{BASE}c"),
            &format!("{BASE}parentOf"),
            &format!("{BASE}d"),
        ),
    ])
}

#[test]
fn magic_recursive_transitive_closure_matches_reference() {
    let (store, world_nn) = tc_world();
    let foreign = WorldFactSnapshot::from_world(&store, W, PROFILE).unwrap();
    let prog = tc_program();
    let budget = Budget::default();

    let native = decided(resolve_native(&foreign, &world_nn, &prog, &budget).unwrap());
    let reference = reference_resolver::resolve(&foreign, &world_nn, &prog, &budget).unwrap();

    assert_eq!(native.status, reference.status, "status parity");
    assert_eq!(
        native.bindings, reference.bindings,
        "bottom-up magic-sets == top-down SLD on recursion: native {native:?} vs ref {reference:?}"
    );
    let ys: Vec<&str> = native.bindings.iter().map(|b| b["Y"].as_str()).collect();
    assert!(
        ys.contains(&format!("<{BASE}b>").as_str()),
        "missing b: {ys:?}"
    );
    assert!(
        ys.contains(&format!("<{BASE}c>").as_str()),
        "missing c: {ys:?}"
    );
    assert!(
        ys.contains(&format!("<{BASE}d>").as_str()),
        "missing d: {ys:?}"
    );
    assert_eq!(native.bindings.len(), 3);
}

// ── GAP B: the frontier-aware conclusive-`neither` consult ───────────────────
//
// A 0-step budget cuts the `ancestor` fixpoint before ANY derivation: the goal
// predicate's stratum never saturates. An empty witness is therefore genuinely
// UNDETERMINED (the search was cut mid-fixpoint), NOT the conclusive four-valued
// `neither`. The consult must NOT fire — the answer stays `Exhausted`. This is the
// usual recursive-goal case the doc calls out: the goal predicate is the ROOT of the
// demand transform, so it settles only when the whole run completes (then the status
// is already `Ok`), and can never be settled on an `Exhausted` run. The guard is
// present and correct, and this test proves it correctly does NOT over-collapse.

#[test]
fn magic_backward_exhausted_recursive_goal_is_not_over_collapsed() {
    let (store, world_nn) = tc_world();
    let foreign = WorldFactSnapshot::from_world(&store, W, PROFILE).unwrap();
    let prog = tc_program(); // ?- ancestor(a, Y)
    let budget = Budget {
        max_answers: None,
        max_steps: Some(0), // cut before any ancestor derivation
    };

    let native = decided(resolve_native(&foreign, &world_nn, &prog, &budget).unwrap());

    assert_eq!(
        native.status,
        BudgetStatus::Exhausted,
        "a 0-step cut mid-search is Exhausted (undetermined), not a conclusive Ok"
    );
    assert!(
        native.bindings.is_empty(),
        "no derivation committed ⇒ empty witness"
    );
    // The consult's precondition is the goal predicate being SETTLED. It is the root of
    // the demand transform and its stratum was cut, so it is NOT in the settled
    // frontier — the guard therefore correctly does not fire.
    let goal_pred = format!("{BASE}ancestor");
    assert!(
        !native.frontier.saturated_preds.contains(&goal_pred),
        "the cut goal predicate must NOT be reported settled: {:?}",
        native.frontier.saturated_preds
    );
}

// ── Test 3a: bb (both-bound) ground goal parity ──────────────────────────────

#[test]
fn magic_bb_ground_goal_matches_reference() {
    let (store, world_nn) = tc_world();
    let foreign = WorldFactSnapshot::from_world(&store, W, PROFILE).unwrap();
    // ?- ancestor(a, c)  → present (a→b→c); one "yes" (empty-binding) answer.
    let src = format!(
        ":- prefix(ex, '{BASE}').\n\
             ex:ancestor(X, Y) :- ex:parentOf(X, Y).\n\
             ex:ancestor(X, Y) :- ex:parentOf(X, Z), ex:ancestor(Z, Y).\n\
             ?- ex:ancestor(ex:a, ex:c).\n"
    );
    let prog = parse_query_program(&src).unwrap();
    let budget = Budget::default();

    let native = decided(resolve_native(&foreign, &world_nn, &prog, &budget).unwrap());
    let reference = reference_resolver::resolve(&foreign, &world_nn, &prog, &budget).unwrap();

    assert_eq!(native.status, reference.status);
    assert_eq!(
        native.bindings, reference.bindings,
        "bb parity: native {native:?} vs ref {reference:?}"
    );
    // Present → exactly one empty-binding "yes" answer.
    assert_eq!(native.bindings.len(), 1);
    assert!(
        native.bindings[0].is_empty(),
        "ground yes is an empty binding"
    );
}

#[test]
fn magic_bb_ground_goal_absent_matches_reference() {
    let (store, world_nn) = tc_world();
    let foreign = WorldFactSnapshot::from_world(&store, W, PROFILE).unwrap();
    // ?- ancestor(d, a)  → absent; zero answers.
    let src = format!(
        ":- prefix(ex, '{BASE}').\n\
             ex:ancestor(X, Y) :- ex:parentOf(X, Y).\n\
             ex:ancestor(X, Y) :- ex:parentOf(X, Z), ex:ancestor(Z, Y).\n\
             ?- ex:ancestor(ex:d, ex:a).\n"
    );
    let prog = parse_query_program(&src).unwrap();
    let budget = Budget::default();

    let native = decided(resolve_native(&foreign, &world_nn, &prog, &budget).unwrap());
    let reference = reference_resolver::resolve(&foreign, &world_nn, &prog, &budget).unwrap();
    assert_eq!(native.bindings, reference.bindings);
    assert!(
        native.bindings.is_empty(),
        "absent ground goal has no answers"
    );
}

// ── Test 3b: fb (object-bound) goal parity ───────────────────────────────────

#[test]
fn magic_fb_object_bound_goal_matches_reference() {
    let (store, world_nn) = tc_world();
    let foreign = WorldFactSnapshot::from_world(&store, W, PROFILE).unwrap();
    // ?- ancestor(X, d)  → X ∈ {a, b, c}.
    let src = format!(
        ":- prefix(ex, '{BASE}').\n\
             ex:ancestor(X, Y) :- ex:parentOf(X, Y).\n\
             ex:ancestor(X, Y) :- ex:parentOf(X, Z), ex:ancestor(Z, Y).\n\
             ?- ex:ancestor(X, ex:d).\n"
    );
    let prog = parse_query_program(&src).unwrap();
    let budget = Budget::default();

    let native = decided(resolve_native(&foreign, &world_nn, &prog, &budget).unwrap());
    let reference = reference_resolver::resolve(&foreign, &world_nn, &prog, &budget).unwrap();

    assert_eq!(native.status, reference.status);
    assert_eq!(
        native.bindings, reference.bindings,
        "fb parity: native {native:?} vs ref {reference:?}"
    );
    let xs: Vec<&str> = native.bindings.iter().map(|b| b["X"].as_str()).collect();
    assert!(
        xs.contains(&format!("<{BASE}a>").as_str()),
        "missing a: {xs:?}"
    );
    assert!(
        xs.contains(&format!("<{BASE}b>").as_str()),
        "missing b: {xs:?}"
    );
    assert!(
        xs.contains(&format!("<{BASE}c>").as_str()),
        "missing c: {xs:?}"
    );
    assert_eq!(native.bindings.len(), 3);
}

// ── Test 3c: ff (fully-free) goal parity ─────────────────────────────────────

#[test]
fn magic_ff_free_goal_matches_reference() {
    let (store, world_nn) = tc_world();
    let foreign = WorldFactSnapshot::from_world(&store, W, PROFILE).unwrap();
    // ?- ancestor(X, Y)  → the full transitive closure of a→b→c→d.
    let src = format!(
        ":- prefix(ex, '{BASE}').\n\
             ex:ancestor(X, Y) :- ex:parentOf(X, Y).\n\
             ex:ancestor(X, Y) :- ex:parentOf(X, Z), ex:ancestor(Z, Y).\n\
             ?- ex:ancestor(X, Y).\n"
    );
    let prog = parse_query_program(&src).unwrap();
    let budget = Budget::default();

    let native = decided(resolve_native(&foreign, &world_nn, &prog, &budget).unwrap());
    let reference = reference_resolver::resolve(&foreign, &world_nn, &prog, &budget).unwrap();

    assert_eq!(native.status, reference.status);
    assert_eq!(
        native.bindings, reference.bindings,
        "ff parity: native {native:?} vs ref {reference:?}"
    );
    // Closure of a→b→c→d: 6 pairs.
    assert_eq!(native.bindings.len(), 6);
}

// ── Test 4: demand pruning evidence ──────────────────────────────────────────

#[test]
fn magic_bf_demand_does_not_evaluate_unrelated_starts() {
    // Two disjoint chains: a→b→c and p→q→r.  A bf goal ?- ancestor(a, Y) must
    // demand-restrict to the `a` chain; the magic transform seeds the demand only for
    // `a`, so the derived `ancestor` facts cover only {a→b, a→c}, never the p chain.
    let (store, world_nn) = make_world(&[
        (
            &format!("{BASE}a"),
            &format!("{BASE}parentOf"),
            &format!("{BASE}b"),
        ),
        (
            &format!("{BASE}b"),
            &format!("{BASE}parentOf"),
            &format!("{BASE}c"),
        ),
        (
            &format!("{BASE}p"),
            &format!("{BASE}parentOf"),
            &format!("{BASE}q"),
        ),
        (
            &format!("{BASE}q"),
            &format!("{BASE}parentOf"),
            &format!("{BASE}r"),
        ),
    ]);
    let foreign = WorldFactSnapshot::from_world(&store, W, PROFILE).unwrap();
    let src = format!(
        ":- prefix(ex, '{BASE}').\n\
             ex:ancestor(X, Y) :- ex:parentOf(X, Y).\n\
             ex:ancestor(X, Y) :- ex:parentOf(X, Z), ex:ancestor(Z, Y).\n\
             ?- ex:ancestor(ex:a, Y).\n"
    );
    let prog = parse_query_program(&src).unwrap();

    // Build the transformed program and evaluate it directly to inspect the demanded
    // ancestor facts (the demand restriction is what we are asserting).
    let mut rules: Vec<EvalRule> = Vec::new();
    for r in &prog.rules {
        let head = atom_of(&r.head).unwrap();
        let body: Vec<EvalAtom> = r
            .body
            .iter()
            .filter_map(|l| match l {
                QBodyLit::Atom(a) => Some(atom_of(a).unwrap()),
                _ => None,
            })
            .collect();
        rules.push(EvalRule {
            numeric: Vec::new(),
            head,
            body,
            rule_iri: format!("{}::rule", atom_of(&r.head).unwrap().predicate.as_str()),
            distinct_pairs: vec![],
            builtins: vec![],
            reduction: None,
            constraint_tag: None,
        });
    }
    let goal = &prog.goal.atoms[0];
    let goal_atom = atom_of(goal).unwrap();
    let transformed = magic_transform(&rules, &goal_atom, goal_adornment(goal));
    let mut edb = extract_edb(&foreign, &world_nn).unwrap();
    for seed in &transformed.seeds {
        let f = seed_to_fact(seed).unwrap();
        edb.insert(&f.predicate, &f.subject, &f.object);
    }
    let facts = match evaluate(edb, &exe(&transformed.rules), None).unwrap() {
        NativeOutcome::Decided(budgeted) => budgeted.rows,
        other => panic!("expected Decided, got {other:?}"),
    };

    let anc = format!("{BASE}ancestor");
    let derived_anc: BTreeSet<(String, String)> = facts
        .iter()
        .filter(|f| f.predicate.as_str() == anc)
        .map(|f| (term_display(&f.subject), term_display(&f.object)))
        .collect();
    // The bf demand seeds the goal `ancestor(a, _)`; the SIPS propagates the demand
    // forward only along edges reachable from `a` (`a` then its successor `b`), so the
    // derived `ancestor` facts are exactly the reachable closure rooted in the
    // a-chain: {(a,b),(b,c),(a,c)}.  Crucially the DISJOINT p→q→r chain is NEVER
    // demanded — no `p`/`q`-rooted ancestor fact is derived, even though p→q→r is in
    // the EDB.  A non-demand (full) evaluation would additionally derive
    // {(p,q),(q,r),(p,r)}; their absence is the demand-pruning evidence.
    let want: BTreeSet<(String, String)> = [("a", "b"), ("b", "c"), ("a", "c")]
        .into_iter()
        .map(|(s, o)| (format!("<{BASE}{s}>"), format!("<{BASE}{o}>")))
        .collect();
    assert_eq!(
        derived_anc, want,
        "bf demand must derive exactly the a-rooted reachable closure, pruning the \
             disjoint p-chain: {derived_anc:?}"
    );
    // Explicit pruning witnesses: none of the p-chain ancestor facts appear.
    for (s, o) in [("p", "q"), ("q", "r"), ("p", "r")] {
        assert!(
            !derived_anc.contains(&(format!("<{BASE}{s}>"), format!("<{BASE}{o}>"))),
            "demand must prune the unrelated p-chain fact ancestor({s},{o})"
        );
    }
}

// ── Test 5: cut / arithmetic / non-binary unsupported ────────────────────────

#[test]
fn magic_cut_is_unsupported() {
    let (store, world_nn) = make_world(&[(
        &format!("{BASE}a"),
        &format!("{BASE}parentOf"),
        &format!("{BASE}b"),
    )]);
    let foreign = WorldFactSnapshot::from_world(&store, W, PROFILE).unwrap();
    let src = format!(
        ":- prefix(ex, '{BASE}').\n\
             ex:ancestor(X, Y) :- ex:parentOf(X, Y), !, ex:ancestor(X, Y).\n\
             ?- ex:ancestor(ex:a, Y).\n"
    );
    let prog = parse_query_program(&src).unwrap();
    let outcome = resolve_native(&foreign, &world_nn, &prog, &Budget::default()).unwrap();
    assert!(
        matches!(outcome, NativeOutcome::Unsupported(UnsupportedKind::Cut)),
        "cut must be Unsupported(Cut): {outcome:?}"
    );
}

#[test]
fn magic_binary_arithmetic_is_decided_natively() {
    // The binary arithmetic list-length program is now DECIDED by the native
    // magic core (no longer an Arithmetic gap): the builtin `N is M + 1` is
    // evaluated as a post-join generator in the modified rules.  Over the
    // single-cell list l0→rest→nil, len(l0) = 1.
    let (store, world_nn) = make_world(&[(
        &format!("{BASE}l0"),
        "http://www.w3.org/1999/02/22-rdf-syntax-ns#rest",
        "http://www.w3.org/1999/02/22-rdf-syntax-ns#nil",
    )]);
    let foreign = WorldFactSnapshot::from_world(&store, W, PROFILE).unwrap();
    let src = format!(
        ":- prefix(ex, '{BASE}').\n\
             :- prefix(rdf, 'http://www.w3.org/1999/02/22-rdf-syntax-ns#').\n\
             ex:len(rdf:nil, 0).\n\
             ex:len(L, N) :- rdf:rest(L, R), ex:len(R, M), N is M + 1.\n\
             ?- ex:len(ex:l0, N).\n"
    );
    let prog = parse_query_program(&src).unwrap();
    let outcome = resolve_native(&foreign, &world_nn, &prog, &Budget::default()).unwrap();
    let NativeOutcome::Decided(answer) = outcome else {
        panic!("binary arithmetic must be Decided natively, not a gap: {outcome:?}");
    };
    assert_eq!(answer.bindings.len(), 1, "one length answer: {answer:?}");
    assert_eq!(
        answer.bindings[0]["N"],
        "\"1\"^^<http://www.w3.org/2001/XMLSchema#integer>"
    );
}

#[test]
fn magic_exact_rational_matches_reference() {
    // A rational-arithmetic rule (`H is 6 / 4` → 3/2) is decided natively by the
    // backward magic core, and the answer is BYTE-IDENTICAL to the top-down SLD
    // reference oracle — the scalar-ℚ family cannot diverge across engines
    // because both call the one shared moded evaluator.
    let (store, world_nn) = make_world(&[(
        &format!("{BASE}a"),
        &format!("{BASE}kind"),
        &format!("{BASE}item"),
    )]);
    let foreign = WorldFactSnapshot::from_world(&store, W, PROFILE).unwrap();
    let src = format!(
        ":- prefix(ex, '{BASE}').\n\
             ex:half(X, H) :- ex:kind(X, ex:item), H is 6 / 4.\n\
             ?- ex:half(X, H).\n"
    );
    let prog = parse_query_program(&src).unwrap();
    let budget = Budget::default();

    let native = decided(resolve_native(&foreign, &world_nn, &prog, &budget).unwrap());
    let reference = reference_resolver::resolve(&foreign, &world_nn, &prog, &budget).unwrap();

    assert_eq!(native.status, reference.status, "status parity");
    assert_eq!(
        native.bindings, reference.bindings,
        "native magic must equal top-down SLD for exact ℚ: native {native:?} vs ref {reference:?}"
    );
    assert_eq!(native.bindings.len(), 1);
    assert_eq!(
        native.bindings[0]["H"],
        "\"3/2\"^^<urn:gmeow:transport:rational>"
    );
}

// NOTE: a non-binary goal atom is NO LONGER a binary-path `Unsupported(NonBinaryAtom)`
// gap — the arity-eligibility dispatch in `resolve_native` routes it to the arity-generic
// n-ary evaluator instead (see `super::magic_generic`, where the `triple(s, p, o, w)`
// predicate-as-data resolution and its demand-provenance coverage live). Only a
// multi-atom conjunctive goal remains a declared gap on the binary leg.

// ── Value-generating-recursion termination guard ─────────────────────────────
//
// Over the finite triple EDB a pure-Datalog backward program always terminates; the
// ONLY divergence source is an arithmetic `is` value-generator inside an IDB cycle
// with no finite driver.  `potentially_nonterminating_arithmetic` flags EXACTLY that
// shape, and `resolve_native` returns a typed refusal when `max_steps` is None (no
// hang possible), evaluating normally when a step budget can cut the recursion.

/// A binary self-drive `count(X,S) :- count(X,Y), S is Y+1` (seeded from an EDB
/// `seed(a,a)` via a base rule) has NO finite driver in its recursive rule — its only
/// positive body atom is the cyclic head predicate `count`, and the `is` generates a
/// fresh successor forever.  With no step budget that is an unbounded hang, so the
/// native core refuses it as `NonTerminatingArithmetic`.
fn self_drive_program() -> String {
    format!(
        ":- prefix(ex, '{BASE}').\n\
             ex:count(X, 0) :- ex:seed(X, X).\n\
             ex:count(X, S) :- ex:count(X, Y), S is Y + 1.\n\
             ?- ex:count(ex:a, N).\n"
    )
}

#[test]
fn magic_value_generating_self_drive_is_unsupported_without_budget() {
    let (store, world_nn) = make_world(&[(
        &format!("{BASE}a"),
        &format!("{BASE}seed"),
        &format!("{BASE}a"),
    )]);
    let foreign = WorldFactSnapshot::from_world(&store, W, PROFILE).unwrap();
    let prog = parse_query_program(&self_drive_program()).unwrap();
    // No max_steps ⇒ the guard fires (an unbounded hang would otherwise occur).
    let outcome = resolve_native(&foreign, &world_nn, &prog, &Budget::default()).unwrap();
    assert!(
        matches!(
            outcome,
            NativeOutcome::Unsupported(UnsupportedKind::NonTerminatingArithmetic)
        ),
        "a value-generating self-drive with no finite driver and no budget must be \
             Unsupported(NonTerminatingArithmetic): {outcome:?}"
    );
}

#[test]
fn magic_value_generating_self_drive_is_budgeted_partial_prefix() {
    let (store, world_nn) = make_world(&[(
        &format!("{BASE}a"),
        &format!("{BASE}seed"),
        &format!("{BASE}a"),
    )]);
    let foreign = WorldFactSnapshot::from_world(&store, W, PROFILE).unwrap();
    let prog = parse_query_program(&self_drive_program()).unwrap();
    // WITH a step budget the guard is bypassed: the StepGovernor cuts the otherwise-
    // infinite recursion deterministically, yielding a SOUND partial prefix.
    let budget = Budget {
        max_steps: Some(3),
        ..Default::default()
    };
    let cut = decided(resolve_native(&foreign, &world_nn, &prog, &budget).unwrap());
    assert_eq!(
        cut.status,
        BudgetStatus::Exhausted,
        "a budgeted value-generator is cut mid-recursion ⇒ Exhausted: {cut:?}"
    );
    // Every answer is a genuine `count(a, k)` for a distinct integer k — sound
    // (present in the infinite least model), and the governor cut it to a finite set.
    let mut seen: BTreeSet<String> = BTreeSet::new();
    for b in &cut.bindings {
        let n = &b["N"];
        assert!(
            n.contains("XMLSchema#integer"),
            "each answer binds N to an integer successor: {n}"
        );
        assert!(seen.insert(n.clone()), "successors are distinct: {n}");
    }
    assert!(
        !cut.bindings.is_empty() && cut.bindings.len() <= 4,
        "a 3-step budget admits a small finite prefix, not the infinite model: {cut:?}"
    );
}

#[test]
fn magic_finite_driver_arithmetic_is_not_flagged() {
    // The list-length program is in an IDB cycle (len→len) WITH arithmetic, but its
    // recursive rule carries the non-cyclic EDB body atom `rdf:rest(L,R)` — a finite
    // driver.  The guard must NOT flag it (condition 3 is false), so it is decided
    // natively even with no step budget.  This is the direct guard-precision check
    // complementing `magic_binary_arithmetic_is_decided_natively`.
    let (store, world_nn) = make_world(&[(
        &format!("{BASE}l0"),
        "http://www.w3.org/1999/02/22-rdf-syntax-ns#rest",
        "http://www.w3.org/1999/02/22-rdf-syntax-ns#nil",
    )]);
    let foreign = WorldFactSnapshot::from_world(&store, W, PROFILE).unwrap();
    let src = format!(
        ":- prefix(ex, '{BASE}').\n\
             :- prefix(rdf, 'http://www.w3.org/1999/02/22-rdf-syntax-ns#').\n\
             ex:len(rdf:nil, 0).\n\
             ex:len(L, N) :- rdf:rest(L, R), ex:len(R, M), N is M + 1.\n\
             ?- ex:len(ex:l0, N).\n"
    );
    let prog = parse_query_program(&src).unwrap();
    // Budget::default() ⇒ max_steps None ⇒ the guard is ACTIVE. A finite-driver
    // program must survive it and decide.
    let outcome = resolve_native(&foreign, &world_nn, &prog, &Budget::default()).unwrap();
    assert!(
        matches!(outcome, NativeOutcome::Decided(_)),
        "the finite-driver len program must NOT be flagged non-terminating: {outcome:?}"
    );
}

// ── Budget: max_answers truncation parity ────────────────────────────────────

#[test]
fn magic_budget_max_answers_matches_reference() {
    let (store, world_nn) = tc_world();
    let foreign = WorldFactSnapshot::from_world(&store, W, PROFILE).unwrap();
    let prog = tc_program();
    let budget = Budget {
        max_answers: Some(1),
        ..Default::default()
    };
    let native = decided(resolve_native(&foreign, &world_nn, &prog, &budget).unwrap());
    let reference = reference_resolver::resolve(&foreign, &world_nn, &prog, &budget).unwrap();
    assert_eq!(native.bindings.len(), 1, "capped at 1 answer");
    assert_eq!(native.status, BudgetStatus::Partial, "cap → Partial");
    assert_eq!(
        native.status, reference.status,
        "status parity under budget"
    );
}

// ── Budget: max_steps (step/derivation governor) ─────────────────────────────

/// A step budget below the completion cost stamps `Exhausted` and returns a SOUND
/// SUBSET of the unbounded answers — never a wrong verdict, never an answer the full
/// model does not contain.
#[test]
fn magic_budget_max_steps_exhausts_with_sound_subset() {
    let (store, world_nn) = tc_world();
    let foreign = WorldFactSnapshot::from_world(&store, W, PROFILE).unwrap();
    let prog = tc_program();

    let unbounded =
        decided(resolve_native(&foreign, &world_nn, &prog, &Budget::default()).unwrap());
    assert_eq!(unbounded.status, BudgetStatus::Ok);
    let full: BTreeSet<String> = unbounded.bindings.iter().map(|b| b["Y"].clone()).collect();
    assert_eq!(full.len(), 3, "a→b→c→d yields ancestors {{b,c,d}}");

    let budget = Budget {
        max_steps: Some(1),
        ..Default::default()
    };
    let cut = decided(resolve_native(&foreign, &world_nn, &prog, &budget).unwrap());
    assert_eq!(
        cut.status,
        BudgetStatus::Exhausted,
        "a 1-step budget cannot reach the 3-answer fixpoint ⇒ Exhausted"
    );
    for b in &cut.bindings {
        assert!(
            full.contains(&b["Y"]),
            "every budget-cut answer must be sound (present in the full model): {b:?}"
        );
    }
    assert!(
        cut.bindings.len() < full.len(),
        "the cut answer set is a strict subset of the full model"
    );
}

/// The step budget is charged at the SINGLE committed-derivation counting point
/// (`StepGovernor::charge` in `eval_stratum_fixpoint`): `max_steps = n` charges EXACTLY
/// `n` committed derivations before stamping `Exhausted`, and the returned bindings are
/// a SOUND prefix — a strict subset of the unbudgeted answer set, every member of which
/// is genuinely in the least model.  Asserted across several `n`.
#[test]
fn magic_budget_single_counting_point_exact_charge_and_sound_prefix() {
    let (store, world_nn) = tc_world();
    let foreign = WorldFactSnapshot::from_world(&store, W, PROFILE).unwrap();
    let prog = tc_program();

    let full: BTreeSet<String> =
        decided(resolve_native(&foreign, &world_nn, &prog, &Budget::default()).unwrap())
            .bindings
            .iter()
            .map(|b| b["Y"].clone())
            .collect();
    assert_eq!(full.len(), 3);

    for n in 1..=3u64 {
        let budget = Budget {
            max_steps: Some(n),
            ..Default::default()
        };
        let cut = decided(resolve_native(&foreign, &world_nn, &prog, &budget).unwrap());
        // Every cut answer is sound (present in the full least model).
        let got: BTreeSet<String> = cut.bindings.iter().map(|b| b["Y"].clone()).collect();
        assert!(
            got.is_subset(&full),
            "n={n}: cut answers must be a sound subset of the full model: {got:?} ⊄ {full:?}"
        );
        // The tc closure needs more than 3 committed derivations (magic seeds +
        // ancestor facts), so every n in 1..=3 cuts mid-fixpoint.
        assert_eq!(
            cut.status,
            BudgetStatus::Exhausted,
            "n={n}: below the completion cost ⇒ Exhausted"
        );
        // The single counting point charged EXACTLY n derivations: on `Exhausted` the
        // governor stopped the instant `consumed == n` (spent-before-commit).
        assert_eq!(
            cut.frontier.consumed_steps, n,
            "n={n}: exactly n committed derivations charged at the single counting point"
        );
    }
}

/// Budget composition is a stable status matrix at the `resolve_native` surface,
/// unchanged by the arity-generic dispatch rewrites: a generous budget completes
/// (`Ok`), a tight `max_steps` cuts (`Exhausted`), and a reached `max_answers` cap is
/// `Partial` (taking precedence over any concurrent step cut).  This locks the budget
/// transfer through the profile-gated backward leg.
#[test]
fn magic_budget_composition_status_matrix() {
    let (store, world_nn) = tc_world();
    let foreign = WorldFactSnapshot::from_world(&store, W, PROFILE).unwrap();
    let prog = tc_program();

    // Ok: a generous step budget, no answer cap ⇒ the fixpoint completes.
    let ok = decided(
        resolve_native(
            &foreign,
            &world_nn,
            &prog,
            &Budget {
                max_steps: Some(1_000_000),
                max_answers: None,
            },
        )
        .unwrap(),
    );
    assert_eq!(ok.status, BudgetStatus::Ok, "generous budget ⇒ Ok");
    assert_eq!(ok.bindings.len(), 3);

    // Exhausted: a tight step budget cuts before the fixpoint settles.
    let exhausted = decided(
        resolve_native(
            &foreign,
            &world_nn,
            &prog,
            &Budget {
                max_steps: Some(1),
                max_answers: None,
            },
        )
        .unwrap(),
    );
    assert_eq!(
        exhausted.status,
        BudgetStatus::Exhausted,
        "tight max_steps ⇒ Exhausted"
    );

    // Partial: a reached answer cap overrides even a concurrent step cut.
    let partial = decided(
        resolve_native(
            &foreign,
            &world_nn,
            &prog,
            &Budget {
                max_steps: Some(1),
                max_answers: Some(1),
            },
        )
        .unwrap(),
    );
    assert_eq!(
        partial.status,
        BudgetStatus::Partial,
        "a reached max_answers cap ⇒ Partial (precedence over the step cut)"
    );
    assert_eq!(partial.bindings.len(), 1);
}

/// A step cut is DETERMINISTIC on the backward leg: the same intermediate budget
/// yields byte-identical bindings and status run-to-run (the fixpoint cut is the Nth
/// FactKey-sorted committed winner, and `project_answers`+`canonicalize` is a
/// deterministic function of the fact cut).
#[test]
fn magic_budget_max_steps_is_deterministic() {
    let (store, world_nn) = tc_world();
    let foreign = WorldFactSnapshot::from_world(&store, W, PROFILE).unwrap();
    let prog = tc_program();
    let budget = Budget {
        max_steps: Some(2),
        ..Default::default()
    };
    let run1 = decided(resolve_native(&foreign, &world_nn, &prog, &budget).unwrap());
    let run2 = decided(resolve_native(&foreign, &world_nn, &prog, &budget).unwrap());
    assert_eq!(run1.status, run2.status, "status is deterministic");
    assert_eq!(
        run1.bindings, run2.bindings,
        "the backward-leg budget cut is byte-identical run-to-run"
    );
}

/// When BOTH budgets fire, the answer cap takes precedence (`Partial`), matching the
/// reference oracle — a step `Exhausted` does not override a reached `max_answers`.
#[test]
fn magic_budget_max_steps_and_max_answers_partial_precedence() {
    let (store, world_nn) = tc_world();
    let foreign = WorldFactSnapshot::from_world(&store, W, PROFILE).unwrap();
    let prog = tc_program();
    // A generous step budget so the fixpoint completes, then a max_answers cap of 1.
    let budget = Budget {
        max_steps: Some(1_000_000),
        max_answers: Some(1),
    };
    let native = decided(resolve_native(&foreign, &world_nn, &prog, &budget).unwrap());
    assert_eq!(native.bindings.len(), 1, "capped at 1 answer");
    assert_eq!(
        native.status,
        BudgetStatus::Partial,
        "the answer cap takes precedence over any step budget"
    );
}

/// A pure-EDB goal is `Ok`-complete under ANY step budget, including `max_steps = 0`:
/// no rule fires (the goal predicate is EDB, i.e. the settled stratum 0), so the
/// answer needs no derivation.  This is the frontier win at the query surface — the
/// reference oracle would stamp `Exhausted` at 0 (it counts the EDB lookup as a step),
/// but native honestly reports a complete answer.
#[test]
fn magic_pure_edb_goal_is_ok_under_zero_step_budget() {
    let (store, world_nn) = tc_world();
    let foreign = WorldFactSnapshot::from_world(&store, W, PROFILE).unwrap();
    // Goal is the EDB predicate parentOf; the program carries NO rules.
    let src = format!(
        ":- prefix(ex, '{BASE}').\n\
             ?- ex:parentOf(ex:a, Y).\n"
    );
    let prog = parse_query_program(&src).unwrap();
    let budget = Budget {
        max_steps: Some(0),
        ..Default::default()
    };
    let native = decided(resolve_native(&foreign, &world_nn, &prog, &budget).unwrap());
    assert_eq!(
        native.status,
        BudgetStatus::Ok,
        "a pure-EDB goal derives nothing ⇒ complete under any budget"
    );
    assert_eq!(native.bindings.len(), 1, "parentOf(a, b)");
    assert_eq!(native.bindings[0]["Y"], format!("<{BASE}b>"));
}

// ── Native base-rule fallback (the extracted fallback decision) ───────────────
//
// `eval_with_base_fallback` is the fallback DECISION extracted from `resolve_native`.
// The public query-IR fragment carries no negation, so a demand transform is always
// stratifiable and this arm is not reachable through `resolve_native`'s query inputs;
// exercise it directly with a hand-built non-stratifiable transformed program and a
// distinct, stratifiable base program.

/// A binary body/head atom `pred(?s, ?o)`, negated iff `neg`.
fn fb_atom(subject: &str, pred: &str, object: &str, neg: bool) -> EvalAtom {
    EvalAtom {
        subject: EvalTerm::Var(subject.to_owned()),
        predicate: format!("{BASE}{pred}"),
        object: EvalTerm::Var(object.to_owned()),
        negated: neg,
    }
}

/// A rule `head :- body` with no builtins/guards.
fn fb_rule(head: EvalAtom, body: Vec<EvalAtom>) -> EvalRule {
    let rule_iri = format!("{}::rule", head.predicate);
    EvalRule {
        numeric: Vec::new(),
        head,
        body,
        rule_iri,
        distinct_pairs: vec![],
        builtins: vec![],
        reduction: None,
        constraint_tag: None,
    }
}

/// A structurally non-stratifiable pair `a :- ~b`, `b :- ~a` (a negative cycle):
/// `stratify` returns `None`, so `evaluate` yields `Unsupported(NonStratifiable)`.
fn non_stratifiable_rules() -> Vec<EvalRule> {
    vec![
        fb_rule(
            fb_atom("?X", "a", "?Y", false),
            vec![fb_atom("?X", "b", "?Y", true)],
        ),
        fb_rule(
            fb_atom("?X", "b", "?Y", false),
            vec![fb_atom("?X", "a", "?Y", true)],
        ),
    ]
}

#[test]
fn eval_with_base_fallback_fires_on_nonstratifiable_transform() {
    let transformed = non_stratifiable_rules();
    // A DIFFERENT, stratifiable base program: derived(?X, ?Y) :- src(?X, ?Y).
    let base = vec![fb_rule(
        fb_atom("?X", "derived", "?Y", false),
        vec![fb_atom("?X", "src", "?Y", false)],
    )];
    let x = format!("{BASE}x");
    let y = format!("{BASE}y");
    // The base EDB `src(x, y)` — extracted lazily ONLY when the fallback fires.
    let base_edb = || {
        let mut edb = RelationStore::new();
        edb.insert(
            &format!("{BASE}src"),
            &TermValue::iri(x.clone()),
            &TermValue::iri(y.clone()),
        );
        Ok(edb)
    };
    // The transformed EDB is irrelevant: the transform is non-stratifiable, so
    // `evaluate` short-circuits before touching it.
    let out = eval_with_base_fallback(
        "fallback-test",
        RelationStore::new(),
        transformed,
        base,
        None,
        base_edb,
    )
    .expect("fallback must not error");
    let FallbackOutcome::Decided {
        facts,
        status,
        demand_pruning_dropped,
        ..
    } = out
    else {
        panic!("expected the base fallback to decide, got a declared gap");
    };
    assert_eq!(
        status,
        BudgetStatus::Ok,
        "the base fixpoint runs to its natural end"
    );
    // The base fallback fired: the demand pruning was dropped, and the caller must be
    // told so it can downgrade the answer's preservation claim from `{exact}`.
    assert!(
        demand_pruning_dropped,
        "a base-fallback decision must flag that demand pruning was dropped"
    );
    let keys: BTreeSet<_> = facts.iter().map(Fact::key).collect();
    // The base rule derived exactly derived(x, y) — proof the arm executed the BASE
    // rules, not the (non-stratifiable) transformed ones.
    assert!(
        keys.contains(&(
            purrdf::TermValue::iri(&x),
            format!("{BASE}derived"),
            purrdf::TermValue::iri(&y)
        )),
        "base materialization must contain derived(x, y): {keys:?}"
    );
    // No transformed-only predicate appears (the transformed program never evaluated).
    assert!(
        keys.iter()
            .all(|(_, p, _)| p != &format!("{BASE}a") && p != &format!("{BASE}b")),
        "no transformed-program predicate may appear: {keys:?}"
    );
    // Exactly the base EDB fact plus the single derived fact.
    assert_eq!(
        keys.len(),
        2,
        "base fact set = {{src(x, y), derived(x, y)}}: {keys:?}"
    );
}

#[test]
fn eval_with_base_fallback_passes_through_when_base_also_nonstratifiable() {
    let transformed = non_stratifiable_rules();
    let base = non_stratifiable_rules();
    // The base EDB never matters here: the base program is also non-stratifiable, so
    // its `evaluate` short-circuits at stratification.
    let out = eval_with_base_fallback(
        "fallback-both-gap-test",
        RelationStore::new(),
        transformed,
        base,
        None,
        || Ok(RelationStore::new()),
    )
    .expect("fallback must not error");
    assert!(
        matches!(
            out,
            FallbackOutcome::Unsupported(UnsupportedKind::NonStratifiable)
        ),
        "both transformed and base non-stratifiable ⇒ the genuine gap passes through"
    );
}

// ── Stratified negation-as-failure (backward surface) ────────────────────────

/// The `unsupported` kind of a native outcome, or a panic if it decided.
fn unsupported_kind(outcome: NativeOutcome<AnswerSet>) -> UnsupportedKind {
    match outcome {
        NativeOutcome::Unsupported(k) => k,
        NativeOutcome::Decided(a) => panic!("expected Unsupported, got Decided({a:?})"),
    }
}

/// A small reachability world: edges a→b, b→c (so a reaches b and c, b reaches c), plus
/// a `node(v, v)` self-loop domain marker for a, b, c (a binary encoding of the vertex
/// set so the whole program stays on the binary backward path).
fn reachability_world() -> (WorldStore, String) {
    make_world(&[
        (
            &format!("{BASE}a"),
            &format!("{BASE}edge"),
            &format!("{BASE}b"),
        ),
        (
            &format!("{BASE}b"),
            &format!("{BASE}edge"),
            &format!("{BASE}c"),
        ),
        (
            &format!("{BASE}a"),
            &format!("{BASE}node"),
            &format!("{BASE}a"),
        ),
        (
            &format!("{BASE}b"),
            &format!("{BASE}node"),
            &format!("{BASE}b"),
        ),
        (
            &format!("{BASE}c"),
            &format!("{BASE}node"),
            &format!("{BASE}c"),
        ),
    ])
}

// (a) A stratified-negation program whose BASE is stratifiable: the native core decides
// it with the correct hand-computed answer set. `reachable` is the transitive closure of
// `edge`; `unreachable(X, Y)` holds for domain vertices with no path X ⇝ Y.
#[test]
fn magic_stratified_negation_reachability_decides_correctly() {
    let (store, world_nn) = reachability_world();
    let foreign = WorldFactSnapshot::from_world(&store, W, PROFILE).unwrap();
    let src = format!(
        ":- prefix(ex, '{BASE}').\n\
             ex:reachable(X, Y) :- ex:edge(X, Y).\n\
             ex:reachable(X, Y) :- ex:edge(X, Z), ex:reachable(Z, Y).\n\
             ex:unreachable(X, Y) :- ex:node(X, X), ex:node(Y, Y), \\+ ex:reachable(X, Y).\n\
             ?- ex:unreachable(ex:a, Y).\n"
    );
    let prog = parse_query_program(&src).unwrap();
    let native = decided(resolve_native(&foreign, &world_nn, &prog, &Budget::default()).unwrap());

    // `a` reaches {b, c}; the domain is {a, b, c}; so `a` is unreachable only to `a`
    // itself (no self-loop edge). The sole answer is Y = a.
    let ys: Vec<&str> = native.bindings.iter().map(|b| b["Y"].as_str()).collect();
    assert_eq!(
        ys,
        vec![format!("<{BASE}a>").as_str()],
        "unreachable(a, Y) must be exactly {{a}}: {native:?}"
    );
    assert_eq!(native.status, BudgetStatus::Ok);
    // The demand transform of THIS program stays stratifiable (`unreachable` negates
    // `reachable`, which does not reach back), so the answer is fully demand-pruned and
    // its preservation is `{exact}` — no base fallback, nothing dropped.
    assert!(
        native
            .preservation
            .polarities
            .contains(&gmeow_logic_compile::ir::PreservationKind::Exact),
        "a stratifiable-transform answer is exact: {:?}",
        native.preservation
    );
}

#[test]
fn annotated_stratified_naf_scores_positive_support_and_treats_absence_as_unit() {
    let (store, world_nn) = reachability_world();
    let foreign = WorldFactSnapshot::from_world(&store, W, PROFILE).unwrap();
    let src = format!(
        ":- prefix(ex, '{BASE}').\n\
             ex:reachable(X, Y) :- ex:edge(X, Y).\n\
             ex:reachable(X, Y) :- ex:edge(X, Z), ex:reachable(Z, Y).\n\
             ex:unreachable(X, Y) :- ex:node(X, X), ex:node(Y, Y), \\+ ex:reachable(X, Y).\n\
             ?- ex:unreachable(ex:a, Y).\n"
    );
    let program = parse_query_program(&src).unwrap();
    let contract = crate::annotation::AnnotationContract::exact();
    let request = AnnotationRequest::new(
        &crate::provenance::ZWeightSemiring,
        &contract,
        |fact: AnnotationFactRef<'_>| (fact.predicate == format!("{BASE}node")).then_some(2),
    );
    let answer = match resolve_native_annotated_under(
        "annotated-naf-one-pass",
        &foreign,
        &world_nn,
        &program,
        &Budget::default(),
        &request,
    )
    .unwrap()
    {
        NativeOutcome::Decided(answer) => answer,
        NativeOutcome::Unsupported(kind) => panic!("unexpected NAF refusal: {kind:?}"),
    };

    assert_eq!(
        answer.certification.query_class,
        crate::annotation::AnnotationQueryClass::StratifiedNaf
    );
    assert_eq!(answer.answers.len(), 1);
    assert_eq!(answer.answers[0].binding["Y"], format!("<{BASE}a>"));
    assert_eq!(
        answer.answers[0].annotation, 4,
        "two positive node premises: 2*2"
    );
    let direct = answer.answers[0]
        .derivations
        .iter()
        .find(|derivation| derivation.sources.len() == 2)
        .expect("NAF answer keeps positive support only");
    assert_eq!(direct.annotation, 4);
}

// (a, downgrade) A stratified-negation program whose BASE is stratifiable but whose
// DEMAND transform is NOT: a negated recursive IDB atom placed before its positive use
// puts a negative literal inside a magic (demand) rule, breaking the transform's
// stratification. `eval_with_base_fallback` recovers the SOUND answer from the base
// program (full materialization), and the answer's preservation is honestly downgraded
// from `{exact}` to `{complete-over}` to record that the demand pruning was dropped.
//
// `asym(X, Y)`: X reaches Y but Y does not reach X (asymmetric reachability). Over the
// chain a→b→c, `asym(a, Y)` = {b, c}. Correctness of this answer set is the primary
// assertion; the preservation downgrade is the honest re-stratify signal.
#[test]
fn magic_negation_transform_nonstratifiable_falls_back_correctly_and_downgrades() {
    let (store, world_nn) = reachability_world();
    let foreign = WorldFactSnapshot::from_world(&store, W, PROFILE).unwrap();
    let src = format!(
        ":- prefix(ex, '{BASE}').\n\
             ex:r(X, Y) :- ex:edge(X, Y).\n\
             ex:r(X, Y) :- ex:edge(X, Z), ex:r(Z, Y).\n\
             ex:asym(X, Y) :- ex:node(X, X), ex:node(Y, Y), \\+ ex:r(Y, X), ex:r(X, Y).\n\
             ?- ex:asym(ex:a, Y).\n"
    );
    let prog = parse_query_program(&src).unwrap();
    let native = decided(resolve_native(&foreign, &world_nn, &prog, &Budget::default()).unwrap());

    let mut ys: Vec<&str> = native.bindings.iter().map(|b| b["Y"].as_str()).collect();
    ys.sort_unstable();
    assert_eq!(
        ys,
        [format!("<{BASE}b>"), format!("<{BASE}c>")]
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>(),
        "asym(a, Y) must be exactly {{b, c}} (full-materialization ground truth): {native:?}"
    );
    assert_eq!(native.status, BudgetStatus::Ok);
    // The transform was non-stratifiable ⇒ base fallback ⇒ demand pruning dropped ⇒ the
    // preservation is downgraded from `{exact}` to the conservative `{complete-over}`.
    assert_eq!(
        native.preservation.polarities,
        std::iter::once(gmeow_logic_compile::ir::PreservationKind::CompleteOver).collect(),
        "a base-fallback answer downgrades to a single {{complete-over}} polarity: {:?}",
        native.preservation
    );
    assert!(
        !native
            .preservation
            .polarities
            .contains(&gmeow_logic_compile::ir::PreservationKind::Exact),
        "the downgraded claim must NOT still assert exact"
    );
}

// (b) A genuinely non-stratifiable program (a negative cycle p ⇄ q at the BASE level,
// over binary atoms with the negated vars range-restricted by `e`): both the demand
// transform AND the base are non-stratifiable, so the native core declares the gap and
// production dispatch surfaces the typed refusal.
#[test]
fn magic_negative_cycle_is_unsupported_nonstratifiable() {
    let (store, world_nn) = make_world(&[(
        &format!("{BASE}a"),
        &format!("{BASE}e"),
        &format!("{BASE}b"),
    )]);
    let foreign = WorldFactSnapshot::from_world(&store, W, PROFILE).unwrap();
    let src = format!(
        ":- prefix(ex, '{BASE}').\n\
             ex:p(X, Y) :- ex:e(X, Y), \\+ ex:q(X, Y).\n\
             ex:q(X, Y) :- ex:e(X, Y), \\+ ex:p(X, Y).\n\
             ?- ex:p(ex:a, Y).\n"
    );
    let prog = parse_query_program(&src).unwrap();
    assert_eq!(
        unsupported_kind(resolve_native(&foreign, &world_nn, &prog, &Budget::default()).unwrap()),
        UnsupportedKind::NonStratifiable,
        "a base negative cycle is a genuine non-stratifiable gap"
    );
}

// (c) A floundering program: the negated atom carries a variable (`Z`) that no positive
// body atom binds, so it is still free when NAF fires. NAF over an unbound goal is
// unsound — the native core refuses it as a declared gap rather than answer wrongly.
#[test]
fn magic_floundering_negation_is_unsupported() {
    let (store, world_nn) = make_world(&[(
        &format!("{BASE}a"),
        &format!("{BASE}e"),
        &format!("{BASE}b"),
    )]);
    let foreign = WorldFactSnapshot::from_world(&store, W, PROFILE).unwrap();
    let src = format!(
        ":- prefix(ex, '{BASE}').\n\
             ex:p(X, Y) :- ex:e(X, Y), \\+ ex:q(Y, Z).\n\
             ?- ex:p(ex:a, Y).\n"
    );
    let prog = parse_query_program(&src).unwrap();
    assert_eq!(
        unsupported_kind(resolve_native(&foreign, &world_nn, &prog, &Budget::default()).unwrap()),
        UnsupportedKind::Floundering,
        "an unbound variable under NAF flounders"
    );
}

// Soundness under a negated atom whose variable is bound only by a LATER positive atom:
// `\+ q(X, Y)` precedes the recursive `r(X, Y)` that binds `Y`. The negated guard must
// NOT leak into `r`'s magic (demand) rule as existential NAF (`\+ q(X, _)`), which would
// wrongly prune `r(a, ·)` whenever `a` has ANY `q` edge and drop valid answers. The
// correct answer for the chain a→b→c is `p(a, Y) = {c}` (a↛directly-c but a⇝c).
#[test]
fn magic_negation_var_bound_by_later_atom_stays_sound() {
    let (store, world_nn) = reachability_world();
    let foreign = WorldFactSnapshot::from_world(&store, W, PROFILE).unwrap();
    let src = format!(
        ":- prefix(ex, '{BASE}').\n\
             ex:r(X, Y) :- ex:edge(X, Y).\n\
             ex:r(X, Y) :- ex:edge(X, Z), ex:r(Z, Y).\n\
             ex:q(X, Y) :- ex:edge(X, Y).\n\
             ex:p(X, Y) :- ex:node(X, X), \\+ ex:q(X, Y), ex:r(X, Y).\n\
             ?- ex:p(ex:a, Y).\n"
    );
    let prog = parse_query_program(&src).unwrap();
    let native = decided(resolve_native(&foreign, &world_nn, &prog, &Budget::default()).unwrap());
    let ys: Vec<&str> = native.bindings.iter().map(|b| b["Y"].as_str()).collect();
    assert_eq!(
        ys,
        vec![format!("<{BASE}c>").as_str()],
        "p(a, Y) must be exactly {{c}} — the later-bound negated var must not under-demand \
             r: {native:?}"
    );
}

// The `not` keyword is an accepted synonym for `\+` on the query surface, decided
// identically by the native stratified core.
#[test]
fn magic_not_keyword_negation_decides_like_backslash_plus() {
    let (store, world_nn) = reachability_world();
    let foreign = WorldFactSnapshot::from_world(&store, W, PROFILE).unwrap();
    let src = format!(
        ":- prefix(ex, '{BASE}').\n\
             ex:reachable(X, Y) :- ex:edge(X, Y).\n\
             ex:reachable(X, Y) :- ex:edge(X, Z), ex:reachable(Z, Y).\n\
             ex:unreachable(X, Y) :- ex:node(X, X), ex:node(Y, Y), not ex:reachable(X, Y).\n\
             ?- ex:unreachable(ex:a, Y).\n"
    );
    let prog = parse_query_program(&src).unwrap();
    let native = decided(resolve_native(&foreign, &world_nn, &prog, &Budget::default()).unwrap());
    let ys: Vec<&str> = native.bindings.iter().map(|b| b["Y"].as_str()).collect();
    assert_eq!(ys, vec![format!("<{BASE}a>").as_str()]);
}

// An n-ary (non-binary) program that also carries negation is an explicit, honest gap:
// stratified NAF lives only on the binary backward path, so the generic n-ary path
// returns a typed refusal rather than silently dropping the negation.
#[test]
fn magic_nary_with_negation_is_unsupported() {
    let (store, world_nn) = make_world(&[(
        &format!("{BASE}a"),
        &format!("{BASE}e"),
        &format!("{BASE}b"),
    )]);
    let foreign = WorldFactSnapshot::from_world(&store, W, PROFILE).unwrap();
    // `t(X, Y, Z)` is arity 3 ⇒ the whole program routes to the generic n-ary path,
    // which declares negation unsupported.
    let src = format!(
        ":- prefix(ex, '{BASE}').\n\
             ex:p(X, Y) :- ex:t(X, Y, Z), \\+ ex:q(X, Y).\n\
             ?- ex:p(ex:a, Y).\n"
    );
    let prog = parse_query_program(&src).unwrap();
    assert!(
        matches!(
            resolve_native(&foreign, &world_nn, &prog, &Budget::default()).unwrap(),
            NativeOutcome::Unsupported(_)
        ),
        "n-ary + negation is a declared gap"
    );
}

// ── Subsumptive demand keying: A/B byte-identity vs the variant transform ─────
//
// Tekle & Liu (SIGMOD 2011): subsumptive tabling keeps only the most-general demanded
// adornment per predicate and serves the more-specific calls from it. The perf win must
// be answer-preserving: the subsumptive transform's goal answer set is BYTE-IDENTICAL to
// the per-adornment (variant) transform's. `magic_transform_variant` is the retained
// reference; these tests are the load-bearing correctness gate for the collapse.

/// Lower a positive/negated-atom program to binary `EvalRule`s (no builtins) — the shared
/// setup for the A/B transform-parity tests.
fn eval_rules_of(prog: &QProgram) -> Vec<EvalRule> {
    prog.rules
        .iter()
        .map(|r| {
            let head = atom_of(&r.head).unwrap();
            let body = r
                .body
                .iter()
                .filter_map(|l| match l {
                    QBodyLit::Atom(a) => Some(atom_of(a).unwrap()),
                    QBodyLit::Neg(a) => Some(EvalAtom {
                        negated: true,
                        ..atom_of(a).unwrap()
                    }),
                    _ => None,
                })
                .collect();
            let rule_iri = format!("{}::rule", head.predicate.as_str());
            EvalRule {
                numeric: Vec::new(),
                head,
                body,
                rule_iri,
                distinct_pairs: vec![],
                builtins: vec![],
                reduction: None,
                constraint_tag: None,
            }
        })
        .collect()
}

/// Resolve `prog` through the given magic transform, returning the canonicalized goal
/// binding set — the A/B comparison surface. Evaluates the transformed program directly
/// (the stratifiable-transform corpus), so it never needs the base fallback.
fn answers_via(
    transform: impl Fn(&[EvalRule], &EvalAtom, BindingPattern) -> MagicProgram,
    foreign: &dyn WorldFactSource,
    world: &str,
    prog: &QProgram,
) -> Vec<Binding> {
    let rules = eval_rules_of(prog);
    let goal = &prog.goal.atoms[0];
    let goal_atom = atom_of(goal).unwrap();
    let transformed = transform(&rules, &goal_atom, goal_adornment(goal));
    let mut edb = extract_edb(foreign, world).unwrap();
    for seed in &transformed.seeds {
        let f = seed_to_fact(seed).unwrap();
        edb.insert(&f.predicate, &f.subject, &f.object);
    }
    let facts = match evaluate(edb, &exe(&transformed.rules), None).unwrap() {
        NativeOutcome::Decided(b) => b.rows,
        other => panic!("expected Decided, got {other:?}"),
    };
    let bindings = project_answers(&facts, &goal_atom);
    let mut answer = AnswerSet {
        bindings,
        status: BudgetStatus::Ok,
        preservation: crate::result::PreservationClaim::exact(),
        frontier: crate::query_ir::CompletionFrontier::empty(),
    };
    answer.canonicalize();
    answer.bindings
}

/// Assert the subsumptive transform produces the byte-identical goal answer set to the
/// variant transform for `prog`.
fn assert_ab_identical(foreign: &dyn WorldFactSource, world: &str, prog: &QProgram, label: &str) {
    let variant = answers_via(magic_transform_variant, foreign, world, prog);
    let subsumptive = answers_via(magic_transform, foreign, world, prog);
    assert_eq!(
        variant, subsumptive,
        "A/B byte-identity failed on {label}: variant {variant:?} vs subsumptive {subsumptive:?}"
    );
}

/// The distinct magic-predicate IRIs (`.../magic/...`) appearing in a transformed program
/// — the count of magic predicates actually MINTED.
fn minted_magic_preds(mp: &MagicProgram) -> BTreeSet<String> {
    let mut preds = BTreeSet::new();
    for r in &mp.rules {
        for p in std::iter::once(&r.head).chain(r.body.iter()) {
            if p.predicate.contains("/magic/") {
                preds.insert(p.predicate.clone());
            }
        }
    }
    preds
}

/// The multi-adornment program: `p` is demanded at BOTH `bf` (from `q`'s first rule and
/// `p(c, Y)` in the second) and `bb` (from `p(X, c)` in the second rule). `bf ⊑ bb`, so
/// the subsumptive collapse keeps only `magic_p_bf` and serves the `bb` demand from it.
fn multi_adornment_program() -> QProgram {
    let src = format!(
        ":- prefix(ex, '{BASE}').\n\
             ex:p(X, Y) :- ex:e(X, Y).\n\
             ex:q(X, Y) :- ex:p(X, Y).\n\
             ex:q(X, Y) :- ex:p(X, ex:c), ex:p(ex:c, Y).\n\
             ?- ex:q(ex:a, W).\n"
    );
    parse_query_program(&src).unwrap()
}

/// The multi-adornment world: `e(a,b), e(a,c), e(c,d), e(b,z)`. The `e(b,z)` edge is the
/// LEAK TRAP: it is reachable only if the general `bf` demand's over-derived `p(a,b)` were
/// wrongly used in the `p(X, c)` (bb) join slot of `q`'s second rule.
fn multi_adornment_world() -> (WorldStore, String) {
    make_world(&[
        (
            &format!("{BASE}a"),
            &format!("{BASE}e"),
            &format!("{BASE}b"),
        ),
        (
            &format!("{BASE}a"),
            &format!("{BASE}e"),
            &format!("{BASE}c"),
        ),
        (
            &format!("{BASE}c"),
            &format!("{BASE}e"),
            &format!("{BASE}d"),
        ),
        (
            &format!("{BASE}b"),
            &format!("{BASE}e"),
            &format!("{BASE}z"),
        ),
    ])
}

// ── Test 1: byte-identity over the full existing corpus + the multi-adornment program ─

#[test]
fn magic_subsumptive_matches_variant_over_corpus() {
    // Single-adornment corpus (subsumptive ≡ variant trivially — each predicate is
    // demanded at ONE adornment, so nothing collapses) over the tc / reachability worlds.
    let (tc_store, tc_w) = tc_world();
    let tc = WorldFactSnapshot::from_world(&tc_store, W, PROFILE).unwrap();
    let bf = format!(
        ":- prefix(ex, '{BASE}').\n\
             ex:ancestor(X, Y) :- ex:parentOf(X, Y).\n\
             ex:ancestor(X, Y) :- ex:parentOf(X, Z), ex:ancestor(Z, Y).\n\
             ?- ex:ancestor(ex:a, Y).\n"
    );
    let bb = bf.replace("?- ex:ancestor(ex:a, Y).", "?- ex:ancestor(ex:a, ex:c).");
    let fb = bf.replace("?- ex:ancestor(ex:a, Y).", "?- ex:ancestor(X, ex:d).");
    let ff = bf.replace("?- ex:ancestor(ex:a, Y).", "?- ex:ancestor(X, Y).");
    let nonrec = format!(
        ":- prefix(ex, '{BASE}').\n\
             ex:ancestorOf(X, Y) :- ex:parentOf(X, Y).\n\
             ?- ex:ancestorOf(ex:a, Y).\n"
    );
    for (label, src) in [
        ("tc-bf", &bf),
        ("tc-bb", &bb),
        ("tc-fb", &fb),
        ("tc-ff", &ff),
        ("non-recursive", &nonrec),
    ] {
        let prog = parse_query_program(src).unwrap();
        assert_ab_identical(&tc, &tc_w, &prog, label);
    }

    // Stratified-negation corpus (transform stays stratifiable) over the reachability
    // world: `unreachable` and the later-bound-negated-var soundness shape.
    let (rw_store, rw_w) = reachability_world();
    let rw = WorldFactSnapshot::from_world(&rw_store, W, PROFILE).unwrap();
    let unreachable = format!(
        ":- prefix(ex, '{BASE}').\n\
             ex:reachable(X, Y) :- ex:edge(X, Y).\n\
             ex:reachable(X, Y) :- ex:edge(X, Z), ex:reachable(Z, Y).\n\
             ex:unreachable(X, Y) :- ex:node(X, X), ex:node(Y, Y), \\+ ex:reachable(X, Y).\n\
             ?- ex:unreachable(ex:a, Y).\n"
    );
    let later_bound = format!(
        ":- prefix(ex, '{BASE}').\n\
             ex:r(X, Y) :- ex:edge(X, Y).\n\
             ex:r(X, Y) :- ex:edge(X, Z), ex:r(Z, Y).\n\
             ex:q(X, Y) :- ex:edge(X, Y).\n\
             ex:p(X, Y) :- ex:node(X, X), \\+ ex:q(X, Y), ex:r(X, Y).\n\
             ?- ex:p(ex:a, Y).\n"
    );
    for (label, src) in [
        ("unreachable", &unreachable),
        ("later-bound-neg", &later_bound),
    ] {
        let prog = parse_query_program(src).unwrap();
        assert_ab_identical(&rw, &rw_w, &prog, label);
    }

    // The multi-adornment program — where the collapse actually FIRES — must stay
    // byte-identical too.
    let (ma_store, ma_w) = multi_adornment_world();
    let ma = WorldFactSnapshot::from_world(&ma_store, W, PROFILE).unwrap();
    assert_ab_identical(&ma, &ma_w, &multi_adornment_program(), "multi-adornment");
}

// ── Test 2: the collapse fires — strictly fewer magic predicates on multi-adornment ──

#[test]
fn magic_subsumptive_collapses_multi_adornment_demand() {
    let (store, world_nn) = multi_adornment_world();
    let foreign = WorldFactSnapshot::from_world(&store, W, PROFILE).unwrap();
    let prog = multi_adornment_program();

    // (a) Byte-identical goal answers: q(a, W) = {b, c, d}. The leak-trap `z` is absent.
    let variant = answers_via(magic_transform_variant, &foreign, &world_nn, &prog);
    let subsumptive = answers_via(magic_transform, &foreign, &world_nn, &prog);
    assert_eq!(
        variant, subsumptive,
        "collapse must preserve the answer set"
    );
    let mut ws: Vec<&str> = subsumptive.iter().map(|b| b["W"].as_str()).collect();
    ws.sort_unstable();
    assert_eq!(
        ws,
        [
            format!("<{BASE}b>"),
            format!("<{BASE}c>"),
            format!("<{BASE}d>")
        ]
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>(),
        "q(a, W) must be exactly {{b, c, d}}: {subsumptive:?}"
    );

    // (b) Strictly fewer magic predicates minted than the variant transform.
    let rules = eval_rules_of(&prog);
    let goal_atom = atom_of(&prog.goal.atoms[0]).unwrap();
    let adorn = goal_adornment(&prog.goal.atoms[0]);
    let variant_mp = magic_transform_variant(&rules, &goal_atom, adorn);
    let subsumptive_mp = magic_transform(&rules, &goal_atom, adorn);

    let variant_preds = minted_magic_preds(&variant_mp);
    let subsumptive_preds = minted_magic_preds(&subsumptive_mp);
    assert!(
        subsumptive_preds.len() < variant_preds.len(),
        "subsumptive must mint strictly fewer magic predicates: subsumptive {subsumptive_preds:?} vs variant {variant_preds:?}"
    );
    // The variant mints `magic/p_bb`; the subsumptive folds it into `magic/p_bf` and mints
    // NO `p_bb` table (the bb demand is served from the more-general bf table).
    assert!(
        variant_preds.iter().any(|p| p.ends_with("p_bb")),
        "variant mints the separate p_bb table: {variant_preds:?}"
    );
    assert!(
        !subsumptive_preds.iter().any(|p| p.ends_with("p_bb")),
        "subsumptive must NOT mint p_bb (served by the general p_bf): {subsumptive_preds:?}"
    );
    assert!(
        subsumptive_preds.iter().any(|p| p.ends_with("p_bf")),
        "subsumptive keeps the most-general p_bf table: {subsumptive_preds:?}"
    );
}

// ── Test 3: residual no-leak — the general demand over-derives, the answer stays exact ─

#[test]
fn magic_subsumptive_residual_no_leak() {
    let (store, world_nn) = multi_adornment_world();
    let foreign = WorldFactSnapshot::from_world(&store, W, PROFILE).unwrap();
    let prog = multi_adornment_program();
    let rules = eval_rules_of(&prog);
    let goal_atom = atom_of(&prog.goal.atoms[0]).unwrap();
    let adorn = goal_adornment(&prog.goal.atoms[0]);

    // Evaluate the SUBSUMPTIVE transformed program and inspect the derived `p` facts.
    let mp = magic_transform(&rules, &goal_atom, adorn);
    let mut edb = extract_edb(&foreign, &world_nn).unwrap();
    for seed in &mp.seeds {
        let f = seed_to_fact(seed).unwrap();
        edb.insert(&f.predicate, &f.subject, &f.object);
    }
    let facts = match evaluate(edb, &exe(&mp.rules), None).unwrap() {
        NativeOutcome::Decided(b) => b.rows,
        other => panic!("expected Decided, got {other:?}"),
    };
    let p = format!("{BASE}p");
    let derived_p: BTreeSet<(String, String)> = facts
        .iter()
        .filter(|f| f.predicate.as_str() == p)
        .map(|f| (term_display(&f.subject), term_display(&f.object)))
        .collect();

    // The general `bf` demand for `p(a, _)` OVER-DERIVES: it materializes BOTH `p(a, b)`
    // and `p(a, c)` (a superset of the `bb` request `p(a, c)`). This is the widened demand
    // the collapse produces — the residual on the extra bound position is NOT enforced at
    // the magic table.
    assert!(
        derived_p.contains(&(format!("<{BASE}a>"), format!("<{BASE}b>"))),
        "the general bf demand over-derives p(a, b): {derived_p:?}"
    );
    assert!(
        derived_p.contains(&(format!("<{BASE}a>"), format!("<{BASE}c>"))),
        "the general bf demand derives the bb-requested p(a, c): {derived_p:?}"
    );

    // Despite the over-derivation, the goal answer is EXACT: the `p(X, ex:c)` (bb) body
    // atom in q's second rule carries the constant `c`, so the over-derived `p(a, b)` is
    // filtered out of the join — it can NEVER reach `p(c, Y)` and drag in the `e(b, z)`
    // trap edge. The residual is discharged by the ORIGINAL body atom's own constant.
    let answers = answers_via(magic_transform, &foreign, &world_nn, &prog);
    let ws: BTreeSet<&str> = answers.iter().map(|b| b["W"].as_str()).collect();
    assert!(
        !ws.contains(format!("<{BASE}z>").as_str()),
        "the over-derived p(a, b) must NOT leak the z trap into the answer: {answers:?}"
    );
    assert_eq!(
        ws,
        [
            format!("<{BASE}b>"),
            format!("<{BASE}c>"),
            format!("<{BASE}d>")
        ]
        .iter()
        .map(String::as_str)
        .collect::<BTreeSet<_>>(),
        "the specific request yields exactly the correct instances, no leak: {answers:?}"
    );
}

// ── Leading-bound recursive-IDB demand-seed repro ─────────────────────────────
//
// A conjunctive rule body that LEADS with a recursive IDB atom carrying a bound
// argument (`reach(self, P)`) must resolve the join — never silently return empty+Ok.
// The magic transform identifies the `reach_bf(self,self)` demand for that leading atom
// as an unconditional control fact. It must be materialized in the EDB seed set so
// `reach` is demanded before the semantic program runs.

/// The base recursive `reach` program + a trailing goal/rule snippet.
fn leading_idb_src(tail: &str) -> String {
    format!(
        ":- prefix(ex, '{BASE}').\n\
             ex:reach(X, Y) :- ex:knows(X, Y).\n\
             ex:reach(X, Y) :- ex:knows(X, Z), ex:reach(Z, Y).\n\
             {tail}"
    )
}

/// The repro world: `self →knows a →knows b`; `b` carries an EDB name and mention. The
/// name object here is an IRI (`nameB`); [`leading_idb_world_literal_name`] below carries
/// the SAME shape with the name object as a string literal instead, which is the issue's
/// exact repro term (`nameMatch(b, "b")`) — both constant kinds are now exercised by the
/// leading-IDB regression suite.
fn leading_idb_world() -> (WorldStore, String) {
    make_world(&[
        (
            &format!("{BASE}self"),
            &format!("{BASE}knows"),
            &format!("{BASE}a"),
        ),
        (
            &format!("{BASE}a"),
            &format!("{BASE}knows"),
            &format!("{BASE}b"),
        ),
        (
            &format!("{BASE}b"),
            &format!("{BASE}nameMatch"),
            &format!("{BASE}nameB"),
        ),
        (
            &format!("{BASE}b"),
            &format!("{BASE}mentioned"),
            &format!("{BASE}engines"),
        ),
    ])
}

/// The SAME repro world as [`leading_idb_world`], except the `nameMatch` triple's
/// object is a genuine `xsd:string` literal `"b"` rather than an IRI — the issue's exact
/// repro term `ex:nameMatch(ex:b, "b")`. The goal-rule's `S` position is a free variable
/// (never a parsed literal in the query text), so the literal only needs to exist in the
/// EDB store: it is inserted via [`WorldStore::insert_quad_terms`] (term-preserving),
/// while the other three triples still go through the IRI-only [`WorldStore::insert_quad`].
fn leading_idb_world_literal_name() -> (WorldStore, String) {
    let store = WorldStore::new();
    store.insert_quad(
        W,
        &format!("{BASE}self"),
        &format!("{BASE}knows"),
        &format!("{BASE}a"),
    );
    store.insert_quad(
        W,
        &format!("{BASE}a"),
        &format!("{BASE}knows"),
        &format!("{BASE}b"),
    );
    store
        .insert_quad_terms(
            W,
            TermValue::iri(format!("{BASE}b")),
            TermValue::iri(format!("{BASE}nameMatch")),
            TermValue::simple_literal("b"),
        )
        .unwrap();
    store.insert_quad(
        W,
        &format!("{BASE}b"),
        &format!("{BASE}mentioned"),
        &format!("{BASE}engines"),
    );
    (store, W.to_owned())
}

#[test]
fn magic_leading_bound_recursive_idb_body_resolves() {
    let (store, world_nn) = leading_idb_world();
    let foreign = WorldFactSnapshot::from_world(&store, W, PROFILE).unwrap();
    let budget = Budget::default();

    // (label, goal/rule tail, expected binding count). The bare-reach and EDB-first rows
    // are the controls (correct even on the pre-fix engine); the two leading-IDB rows are
    // the repro (empty + Ok before the seed-set fix).
    let rows: [(&str, &str, usize); 4] = [
        ("bare-reach", "?- ex:reach(ex:self, P).\n", 2),
        (
            "edb-first-control",
            "ex:c(P, S) :- ex:nameMatch(P, S), ex:reach(ex:self, P), ex:mentioned(P, ex:engines).\n\
                 ?- ex:c(P, S).\n",
            1,
        ),
        (
            "leading-idb-name",
            "ex:c(P, S) :- ex:reach(ex:self, P), ex:nameMatch(P, S).\n\
                 ?- ex:c(P, S).\n",
            1,
        ),
        (
            "leading-idb-name-mention",
            "ex:c(P, S) :- ex:reach(ex:self, P), ex:nameMatch(P, S), ex:mentioned(P, ex:engines).\n\
                 ?- ex:c(P, S).\n",
            1,
        ),
    ];
    for (label, tail, want) in rows {
        let prog = parse_query_program(&leading_idb_src(tail)).unwrap();
        let ans =
            crate::dispatch::dispatch_query(&foreign, &world_nn, &prog, PROFILE, &budget).unwrap();
        assert_eq!(
            ans.status,
            BudgetStatus::Ok,
            "{label}: status must be Ok (never a silent empty drop): {ans:?}"
        );
        assert_eq!(
            ans.bindings.len(),
            want,
            "{label}: expected {want} bindings, got {ans:?}"
        );
    }
}

#[test]
fn magic_leading_bound_recursive_idb_literal_name_object_resolves() {
    // The issue's exact repro term: `nameMatch(b, "b")` with a STRING LITERAL object,
    // not an IRI. The leading-IDB seed-set fix must not be sensitive to the constant
    // kind carried by the trailing EDB atom — this drives the same demand-seed path as
    // `magic_leading_bound_recursive_idb_body_resolves`'s "leading-idb-name" row, but
    // over `leading_idb_world_literal_name()`.
    let (store, world_nn) = leading_idb_world_literal_name();
    let foreign = WorldFactSnapshot::from_world(&store, W, PROFILE).unwrap();
    let budget = Budget::default();
    let src = leading_idb_src(
        "ex:c(P, S) :- ex:reach(ex:self, P), ex:nameMatch(P, S).\n\
             ?- ex:c(P, S).\n",
    );
    let prog = parse_query_program(&src).unwrap();
    let ans =
        crate::dispatch::dispatch_query(&foreign, &world_nn, &prog, PROFILE, &budget).unwrap();
    assert_eq!(
        ans.status,
        BudgetStatus::Ok,
        "literal-object leading-IDB: status must be Ok (never a silent empty drop): {ans:?}"
    );
    assert_eq!(
        ans.bindings.len(),
        1,
        "literal-object leading-IDB: expected 1 binding, got {ans:?}"
    );
    assert_eq!(
        ans.bindings[0]["S"], "\"b\"",
        "S must bind to the string literal \"b\": {ans:?}"
    );
}

#[test]
fn magic_ff_goal_ground_fact_rule_resolves() {
    // Site B: a ground fact-rule `pf(a, b).` under an all-free goal `?- pf(X, Y)` lowers
    // the modified rule to an EMPTY body — an unconditional fact that belongs in the
    // demand seed set rather than the transformed semantic program.
    let (store, world_nn) = make_world(&[]);
    let foreign = WorldFactSnapshot::from_world(&store, W, PROFILE).unwrap();
    let budget = Budget::default();
    let src = format!(
        ":- prefix(ex, '{BASE}').\n\
             ex:pf(ex:a, ex:b).\n\
             ?- ex:pf(X, Y).\n"
    );
    let prog = parse_query_program(&src).unwrap();
    let ans =
        crate::dispatch::dispatch_query(&foreign, &world_nn, &prog, PROFILE, &budget).unwrap();
    assert_eq!(
        ans.status,
        BudgetStatus::Ok,
        "Site B status must be Ok: {ans:?}"
    );
    assert_eq!(
        ans.bindings.len(),
        1,
        "Site B ff-goal + ground fact-rule must return the asserted fact: {ans:?}"
    );
}

#[test]
fn magic_leading_bound_recursive_idb_incremental_capable() {
    // The incremental path previously DECLINED (returned None) a leading-bound
    // recursive-IDB program because the transform produced a bodyless demand rule. With
    // that demand lifted to a seed, the session is prepared and yields the correct
    // non-empty answer end-to-end.
    let (store, world_nn) = leading_idb_world();
    let foreign = WorldFactSnapshot::from_world(&store, W, PROFILE).unwrap();
    let budget = Budget::default();
    let src = leading_idb_src(
        "ex:c(P, S) :- ex:reach(ex:self, P), ex:nameMatch(P, S).\n\
             ?- ex:c(P, S).\n",
    );
    let prog = parse_query_program(&src).unwrap();
    let mut session =
        prepare_incremental_query(&foreign, &world_nn, &prog, "test-contract-1511", &budget)
            .unwrap()
            .expect(
                "a leading-bound recursive-IDB program must now prepare an incremental \
                     session (its demand is a lifted seed, not a bodyless rule)",
            );
    // Apply no changes: the base least model already carries the demanded join.
    let ans = session
        .apply_iri_changes(std::iter::empty::<(String, String, String, i64)>(), None)
        .unwrap();
    assert_eq!(ans.status, BudgetStatus::Ok, "incremental status: {ans:?}");
    assert_eq!(
        ans.bindings.len(),
        1,
        "incremental leading-IDB answer must be non-empty: {ans:?}"
    );
    assert_eq!(
        ans.bindings[0]["P"],
        format!("<{BASE}b>"),
        "P must bind to b: {ans:?}"
    );
    assert_eq!(
        ans.bindings[0]["S"],
        format!("<{BASE}nameB>"),
        "S must bind to nameB: {ans:?}"
    );
}

#[test]
fn magic_seeds_are_exactly_the_bodyless_rule_heads() {
    // Demand-completeness certificate (Beeri–Ramakrishnan): the materialized seed set is
    // EXACTLY the set of ground heads of the bodyless positive rules the transform would
    // emit. For the leading-IDB program the sole such demand is `magic_reach_bf(self,
    // self)` — the goal `c` is ff, so it contributes no goal seed.
    let src = leading_idb_src(
        "ex:c(P, S) :- ex:reach(ex:self, P), ex:nameMatch(P, S).\n\
             ?- ex:c(P, S).\n",
    );
    let prog = parse_query_program(&src).unwrap();
    let rules = eval_rules_of(&prog);
    let goal = &prog.goal.atoms[0];
    let goal_atom = atom_of(goal).unwrap();
    let transformed = magic_transform(&rules, &goal_atom, goal_adornment(goal));

    // No unconditional rule survives (the invariant the transform asserts), and every
    // seed is ground. Semantic NAF-only/builtin-only rules remain valid because they
    // carry body or builtin content.
    assert!(
        transformed
            .rules
            .iter()
            .all(|r| !r.body.is_empty() || !r.builtins.is_empty()),
        "no transformed rule may be unconditional: {:?}",
        transformed.rules
    );
    for s in &transformed.seeds {
        assert!(seed_to_fact(s).is_ok(), "every seed must be ground: {s:?}");
    }

    // Re-derive the expected demand independently: the only leading bound recursive-IDB
    // atom is `reach(self, _)` adorned bf, so the single lifted demand seed is the
    // self-loop `magic_reach_bf(self, self)`.
    let reach_pred = rules
        .iter()
        .map(|r| r.head.predicate.as_str())
        .find(|p| p.ends_with("reach"))
        .expect("the program defines reach")
        .to_owned();
    let self_iri = EvalTerm::ConstNamed(format!("{BASE}self"));
    let expected = EvalAtom {
        subject: self_iri.clone(),
        predicate: magic_pred_iri(&reach_pred, "bf"),
        object: self_iri,
        negated: false,
    };
    assert_eq!(
        transformed.seeds.len(),
        1,
        "exactly one lifted demand seed: {:?}",
        transformed.seeds
    );
    assert_eq!(
        transformed.seeds[0], expected,
        "the seed set must equal the bodyless-rule-head demand set {{magic_reach_bf(self, self)}}"
    );
}

// ── Empty-positive-body identity: NAF-only and builtin-only rules evaluate ──
//
// The empty conjunction contributes one empty substitution. NAF then filters that row
// against the frozen lower-stratum store, while sequential `is` builtins extend it.

#[test]
fn resolve_native_ground_naf_only_body_evaluates_absence_and_presence() {
    let (store, world_nn) = make_world(&[]);
    let foreign = WorldFactSnapshot::from_world(&store, W, PROFILE).unwrap();
    let src = format!(
        ":- prefix(ex, '{BASE}').\n\
             ex:p(ex:a, ex:b) :- \\+ ex:q(ex:a, ex:b).\n\
             ?- ex:p(X, Y).\n"
    );
    let prog = parse_query_program(&src).unwrap();
    let absent = decided(resolve_native(&foreign, &world_nn, &prog, &Budget::default()).unwrap());
    assert_eq!(
        absent.bindings.len(),
        1,
        "absent q must let p fire: {absent:?}"
    );
    assert_eq!(absent.bindings[0]["X"], format!("<{BASE}a>"));
    assert_eq!(absent.bindings[0]["Y"], format!("<{BASE}b>"));

    let q_subject = format!("{BASE}a");
    let q_predicate = format!("{BASE}q");
    let q_object = format!("{BASE}b");
    let (store, world_nn) = make_world(&[(&q_subject, &q_predicate, &q_object)]);
    let foreign = WorldFactSnapshot::from_world(&store, W, PROFILE).unwrap();
    let present = decided(resolve_native(&foreign, &world_nn, &prog, &Budget::default()).unwrap());
    assert!(
        present.bindings.is_empty(),
        "present q must block the ground NAF-only rule: {present:?}"
    );
}

#[test]
fn resolve_native_builtin_only_body_evaluates_adjacent_assignments() {
    let (store, world_nn) = make_world(&[]);
    let foreign = WorldFactSnapshot::from_world(&store, W, PROFILE).unwrap();
    let src = format!(
        ":- prefix(ex, '{BASE}').\n\
             ex:p(X, Y) :- X is 1, Y is 2.\n\
             ?- ex:p(A, B).\n"
    );
    let prog = parse_query_program(&src).unwrap();
    let answer = decided(resolve_native(&foreign, &world_nn, &prog, &Budget::default()).unwrap());
    assert_eq!(
        answer.bindings.len(),
        1,
        "builtin-only rule must fire once: {answer:?}"
    );
    assert_eq!(
        answer.bindings[0]["A"],
        "\"1\"^^<http://www.w3.org/2001/XMLSchema#integer>"
    );
    assert_eq!(
        answer.bindings[0]["B"],
        "\"2\"^^<http://www.w3.org/2001/XMLSchema#integer>"
    );
}

#[test]
fn prepare_incremental_query_declines_ground_naf() {
    // The incremental path already declines any body carrying a `Neg` literal
    // (`binary_eligible`'s per-literal match at the top of
    // `prepare_incremental_query`), UPSTREAM of `magic_transform` — so it is
    // panic-safe on this shape with no additional gate needed there.
    let (store, world_nn) = make_world(&[]);
    let foreign = WorldFactSnapshot::from_world(&store, W, PROFILE).unwrap();
    let src = format!(
        ":- prefix(ex, '{BASE}').\n\
             ex:p(ex:a, ex:b) :- \\+ ex:q(ex:a, ex:b).\n\
             ?- ex:p(X, Y).\n"
    );
    let prog = parse_query_program(&src).unwrap();
    let session = prepare_incremental_query(
        &foreign,
        &world_nn,
        &prog,
        "test-contract-1511-unpositive",
        &Budget::default(),
    )
    .unwrap();
    assert!(
        session.is_none(),
        "a ground-NAF-only body must decline incremental preparation, not panic: \
             {:?}",
        session.is_some()
    );
}

// ── Bilinear-form squared-distance builtin: three-path parity + reachability ──

/// Author a world carrying the exact-rational `math:` cells of the Gram matrix
/// G = diag(2, 1) and two coordinate vectors: state = (1/2, 0) and contentment =
/// (1/5, 1/2). These are REAL authored graph facts (not test-injected transport
/// literals), so an end-to-end evaluation proves the builtin reaches production
/// authored data rather than only a fabricated resolver.
fn metric_world() -> (WorldStore, String) {
    let store = WorldStore::new();
    let m = |l: &str| format!("https://blackcatinformatics.ca/math/{l}");
    let t = |l: &str| format!("urn:gmeow:test:metric:{l}");
    let iri_q = |s: String, p: String, o: String| {
        store
            .insert_quad_terms(W, TermValue::iri(s), TermValue::iri(p), TermValue::iri(o))
            .unwrap();
    };
    let lit_q = |s: String, p: String, lex: &str| {
        store
            .insert_quad_terms(
                W,
                TermValue::iri(s),
                TermValue::iri(p),
                TermValue::typed_literal(lex, crate::physical::XSD_INTEGER),
            )
            .unwrap();
    };
    // Gram G = diag(2, 1).
    iri_q(t("g"), m("hasEntry"), t("e00"));
    iri_q(t("g"), m("hasEntry"), t("e11"));
    lit_q(t("e00"), m("atRow"), "0");
    lit_q(t("e00"), m("atColumn"), "0");
    iri_q(t("e00"), m("entryValue"), t("rat2"));
    lit_q(t("e11"), m("atRow"), "1");
    lit_q(t("e11"), m("atColumn"), "1");
    iri_q(t("e11"), m("entryValue"), t("rat1"));
    lit_q(t("rat2"), m("numerator"), "2");
    lit_q(t("rat2"), m("denominator"), "1");
    lit_q(t("rat1"), m("numerator"), "1");
    lit_q(t("rat1"), m("denominator"), "1");
    // state = (1/2, 0).
    iri_q(t("state"), m("hasComponent"), t("sc0"));
    iri_q(t("state"), m("hasComponent"), t("sc1"));
    lit_q(t("sc0"), m("atIndex"), "0");
    iri_q(t("sc0"), m("componentValue"), t("half"));
    lit_q(t("sc1"), m("atIndex"), "1");
    iri_q(t("sc1"), m("componentValue"), t("zero"));
    lit_q(t("half"), m("numerator"), "1");
    lit_q(t("half"), m("denominator"), "2");
    lit_q(t("zero"), m("numerator"), "0");
    lit_q(t("zero"), m("denominator"), "1");
    // contentment = (1/5, 1/2).
    iri_q(t("contentment"), m("hasComponent"), t("cc0"));
    iri_q(t("contentment"), m("hasComponent"), t("cc1"));
    lit_q(t("cc0"), m("atIndex"), "0");
    iri_q(t("cc0"), m("componentValue"), t("fifth"));
    lit_q(t("cc1"), m("atIndex"), "1");
    iri_q(t("cc1"), m("componentValue"), t("half"));
    lit_q(t("fifth"), m("numerator"), "1");
    lit_q(t("fifth"), m("denominator"), "5");
    (store, W.to_owned())
}

/// Lower a parsed program to the forward-executable base [`EvalRule`]s — the same
/// lowering the magic backward leg's base fallback uses.
fn lower_base_rules(program: &QProgram) -> Vec<EvalRule> {
    let mut rules = Vec::new();
    for source_rule in &program.rules {
        let head = atom_of(&source_rule.head).expect("binary head");
        let mut body = Vec::new();
        let mut builtins = Vec::new();
        for literal in &source_rule.body {
            match literal {
                QBodyLit::Atom(atom) => body.push(atom_of(atom).expect("binary body atom")),
                QBodyLit::Neg(atom) => body.push(EvalAtom {
                    negated: true,
                    ..atom_of(atom).expect("binary negated atom")
                }),
                QBodyLit::Builtin(builtin) => builtins.push(builtin_of(builtin)),
                QBodyLit::Cut => unreachable!("no cut in this program"),
            }
        }
        let rule_iri = format!("{}::rule", head.predicate.as_str());
        rules.push(EvalRule {
            numeric: Vec::new(),
            head,
            body,
            rule_iri,
            distinct_pairs: Vec::new(),
            builtins,
            reduction: None,
            constraint_tag: None,
        });
    }
    rules
}

/// The SAME one-rule metric program evaluated through all THREE native engines —
/// forward `materialize_native`, backward demand/magic `resolve_native`, and the
/// declarative reference oracle — must bind `D` to the byte-identical exact squared
/// distance 43/100. This pins that the shared moded evaluator (and its per-engine
/// cell resolver) is consistent across every path, and that the builtin is reachable
/// from real authored `math:` graph data (not only a test-injected transport
/// literal), so it is not DARK.
#[test]
fn bilinear_sqdist_three_engine_parity_over_authored_cells() {
    let (store, world_nn) = metric_world();
    let foreign = WorldFactSnapshot::from_world(&store, W, PROFILE).unwrap();
    let src = "\
             :- prefix(m, 'https://blackcatinformatics.ca/math/').\n\
             :- prefix(t, 'urn:gmeow:test:metric:').\n\
             t:res(t:episode, D) :- D is bilinearSqDist(t:g, t:state, t:contentment).\n\
             ?- t:res(t:episode, D).\n";
    let prog = parse_query_program(src).unwrap();
    let budget = Budget::default();

    // Exact squared distance: Δ = (3/10, −1/2) → 2·(3/10)² + 1·(1/2)² = 43/100.
    let expected = "\"43/100\"^^<urn:gmeow:transport:rational>";

    // (a) Forward semi-naive materialization.
    let rules = lower_base_rules(&prog);
    let exe = Parsed::uncached(&rules)
        .stratify()
        .expect("stratifiable")
        .plan()
        .into_executable();
    let forward = match crate::physical::materialize_native(&store, &exe, None).unwrap() {
        NativeOutcome::Decided(budgeted) => budgeted,
        NativeOutcome::Unsupported(kind) => panic!("forward path unsupported: {kind:?}"),
    };
    let res_pred = "urn:gmeow:test:metric:res";
    let forward_d = forward
        .rows
        .iter()
        .find(|row| row.predicate == res_pred)
        .map(|row| term_display(&row.object))
        .expect("forward materialization derives the res fact");
    assert_eq!(forward_d, expected, "forward semi-naive squared distance");

    // (b) Backward demand/magic.
    let native = decided(resolve_native(&foreign, &world_nn, &prog, &budget).unwrap());
    // (c) Declarative reference oracle.
    let reference = reference_resolver::resolve(&foreign, &world_nn, &prog, &budget).unwrap();

    assert_eq!(native.bindings.len(), 1, "magic binds exactly one D");
    assert_eq!(reference.bindings.len(), 1, "reference binds exactly one D");
    assert_eq!(native.bindings[0]["D"], expected, "magic squared distance");
    assert_eq!(
        reference.bindings[0]["D"], expected,
        "reference squared distance"
    );
    // Byte-identical across all three engines.
    assert_eq!(
        native.bindings, reference.bindings,
        "magic == reference: {native:?} vs {reference:?}"
    );
    assert_eq!(
        forward_d, native.bindings[0]["D"],
        "forward == magic == reference on the exact squared distance"
    );
}
