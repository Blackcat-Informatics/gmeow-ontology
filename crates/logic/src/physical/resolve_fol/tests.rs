// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Acceptance tests for the structured full-FOL resolver + SLG-WFS.

use std::collections::HashMap;

use purrdf::TermValue;

use super::*;
use crate::physical::id::{MetaId, NodeId, TermId};
use crate::physical::proof::check;
use crate::physical::term_dag::TermDag;
use crate::physical::unify::{SortContext, SortOrder};

// ── Builders ────────────────────────────────────────────────────────────────────

fn iri(dag: &mut TermDag, s: &str) -> NodeId {
    dag.intern_leaf(TermValue::iri(s.to_owned()))
}

/// An `n`-ary atom / application `pred(args…)`.
fn atom(dag: &mut TermDag, pred: &str, args: Vec<NodeId>) -> NodeId {
    let op = iri(dag, pred);
    dag.intern_app(op, args)
}

/// The Peano successor `s(x)`.
fn succ(dag: &mut TermDag, x: NodeId) -> NodeId {
    atom(dag, "s", vec![x])
}

fn rule_iri(dag: &mut TermDag, n: usize) -> TermId {
    dag.intern_atom(&TermValue::iri(format!(
        "https://blackcatinformatics.ca/logic/test/rule/{n}"
    )))
}

fn var(dag: &mut TermDag) -> (MetaId, NodeId) {
    dag.fresh_meta()
}

fn empty_ctx() -> SortContext {
    SortContext::default()
}

/// Assert every produced answer carries a proof `check()` validates to its goal atom.
fn assert_all_proofs_check(dag: &mut TermDag, outcome: &FolOutcome) {
    for ans in &outcome.answers {
        let checked = check(dag, ans.proof, &outcome.rule_ctx)
            .unwrap_or_else(|e| panic!("answer proof failed to check: {e:?}"));
        assert_eq!(
            checked, ans.atom,
            "the checked proof must re-derive exactly the answer atom"
        );
    }
}

fn decided(control: FolControl) -> FolOutcome {
    match control {
        FolControl::Decided(o) => o,
        FolControl::Unsupported(kind) => panic!("expected Decided, got Unsupported({kind:?})"),
    }
}

// ── Test 1: structured backward query correctness (Peano add) + proof carrying ──────

#[test]
fn peano_add_returns_correct_answer_with_checkable_proofs() {
    // add(0, Y, Y).
    // add(s(X), Y, s(Z)) :- add(X, Y, Z).
    // ?- add(s(s(0)), s(0), R).   ⇒   R = s(s(s(0))).
    let mut dag = TermDag::new();
    let zero = iri(&mut dag, "zero");

    // Fact clause: add(zero, Y, Y).
    let (_ym, y) = var(&mut dag);
    let fact_head = atom(&mut dag, "add", vec![zero, y, y]);
    let fact_rule = rule_iri(&mut dag, 0);

    // Rule clause: add(s(X), Y, s(Z)) :- add(X, Y, Z).
    let (_xm, x) = var(&mut dag);
    let (_ym2, y2) = var(&mut dag);
    let (_zm, z) = var(&mut dag);
    let sx = succ(&mut dag, x);
    let sz = succ(&mut dag, z);
    let rule_head = atom(&mut dag, "add", vec![sx, y2, sz]);
    let rule_body = atom(&mut dag, "add", vec![x, y2, z]);
    let step_rule = rule_iri(&mut dag, 1);

    // Goal: add(s(s(0)), s(0), R).
    let ssz = {
        let sz0 = succ(&mut dag, zero);
        succ(&mut dag, sz0)
    };
    let s0 = succ(&mut dag, zero);
    let (_rm, r) = var(&mut dag);
    let goal = atom(&mut dag, "add", vec![ssz, s0, r]);

    let program = FolProgram {
        clauses: vec![
            FolClause {
                head: fact_head,
                body: vec![],
                rule_iri: fact_rule,
            },
            FolClause {
                head: rule_head,
                body: vec![FolLit::Pos(rule_body)],
                rule_iri: step_rule,
            },
        ],
        goal,
        goal_vars: vec![(r, "R".to_owned())],
        meta_sorts: HashMap::new(),
    };

    let outcome =
        decided(resolve_fol(&mut dag, &program, &empty_ctx(), &Budget::default()).unwrap());
    assert_eq!(outcome.status, BudgetStatus::Ok);
    assert_eq!(
        outcome.answers.len(),
        1,
        "exactly one add answer: {:?}",
        outcome.answers
    );
    assert_eq!(
        outcome.answers[0].bindings["R"], "s(s(s(zero)))",
        "2 + 1 = 3 in Peano successors"
    );
    assert_all_proofs_check(&mut dag, &outcome);
}

// ── Test 1b: member over cons/nil (a second structured program) ─────────────────────

#[test]
fn member_over_cons_lists_enumerates_elements_with_proofs() {
    // member(X, cons(X, T)).
    // member(X, cons(H, T)) :- member(X, T).
    // ?- member(M, cons(a, cons(b, cons(c, nil)))).   ⇒   M ∈ {a, b, c}.
    let mut dag = TermDag::new();
    let nil = iri(&mut dag, "nil");
    let a = iri(&mut dag, "a");
    let b = iri(&mut dag, "b");
    let c = iri(&mut dag, "c");

    let cons = |dag: &mut TermDag, h: NodeId, t: NodeId| atom(dag, "cons", vec![h, t]);

    // member(X, cons(X, T)).
    let (_xm, x) = var(&mut dag);
    let (_tm, t) = var(&mut dag);
    let cons_x_t = cons(&mut dag, x, t);
    let base_head = atom(&mut dag, "member", vec![x, cons_x_t]);
    let base_rule = rule_iri(&mut dag, 0);

    // member(X, cons(H, T)) :- member(X, T).
    let (_xm2, x2) = var(&mut dag);
    let (_hm, h2) = var(&mut dag);
    let (_tm2, t2) = var(&mut dag);
    let cons_h_t = cons(&mut dag, h2, t2);
    let step_head = atom(&mut dag, "member", vec![x2, cons_h_t]);
    let step_body = atom(&mut dag, "member", vec![x2, t2]);
    let step_rule = rule_iri(&mut dag, 1);

    // Goal list cons(a, cons(b, cons(c, nil))).
    let list = {
        let cn = cons(&mut dag, c, nil);
        let bcn = cons(&mut dag, b, cn);
        cons(&mut dag, a, bcn)
    };
    let (_mm, m) = var(&mut dag);
    let goal = atom(&mut dag, "member", vec![m, list]);

    let program = FolProgram {
        clauses: vec![
            FolClause {
                head: base_head,
                body: vec![],
                rule_iri: base_rule,
            },
            FolClause {
                head: step_head,
                body: vec![FolLit::Pos(step_body)],
                rule_iri: step_rule,
            },
        ],
        goal,
        goal_vars: vec![(m, "M".to_owned())],
        meta_sorts: HashMap::new(),
    };

    let outcome =
        decided(resolve_fol(&mut dag, &program, &empty_ctx(), &Budget::default()).unwrap());
    assert_eq!(outcome.status, BudgetStatus::Ok);
    let mut got: Vec<String> = outcome
        .answers
        .iter()
        .map(|a| a.bindings["M"].clone())
        .collect();
    got.sort();
    assert_eq!(got, vec!["a".to_owned(), "b".to_owned(), "c".to_owned()]);
    assert_all_proofs_check(&mut dag, &outcome);
}

// ── Test 2: SLG-WFS three-valued negation ───────────────────────────────────────────

#[test]
fn win_move_game_has_correct_well_founded_model() {
    // move(a, b). move(b, a). move(c, d).
    // win(X) :- move(X, Y), not win(Y).
    // Expected WFS: win(c) True, win(d) False, win(a)/win(b) Undefined.
    let mut dag = TermDag::new();
    let a = iri(&mut dag, "a");
    let b = iri(&mut dag, "b");
    let c = iri(&mut dag, "c");
    let d = iri(&mut dag, "d");

    let mk_move = |dag: &mut TermDag, x: NodeId, y: NodeId| atom(dag, "move", vec![x, y]);
    let move_ab = mk_move(&mut dag, a, b);
    let move_ba = mk_move(&mut dag, b, a);
    let move_cd = mk_move(&mut dag, c, d);

    // win(X) :- move(X, Y), not win(Y).
    let (_xm, x) = var(&mut dag);
    let (_ym, y) = var(&mut dag);
    let win_x = atom(&mut dag, "win", vec![x]);
    let move_xy = atom(&mut dag, "move", vec![x, y]);
    let win_y = atom(&mut dag, "win", vec![y]);
    let win_rule = rule_iri(&mut dag, 3);

    // Goal ?- win(W).
    let (_wm, w) = var(&mut dag);
    let goal = atom(&mut dag, "win", vec![w]);

    let program = FolProgram {
        clauses: vec![
            FolClause {
                head: move_ab,
                body: vec![],
                rule_iri: rule_iri(&mut dag, 0),
            },
            FolClause {
                head: move_ba,
                body: vec![],
                rule_iri: rule_iri(&mut dag, 1),
            },
            FolClause {
                head: move_cd,
                body: vec![],
                rule_iri: rule_iri(&mut dag, 2),
            },
            FolClause {
                head: win_x,
                body: vec![FolLit::Pos(move_xy), FolLit::Neg(win_y)],
                rule_iri: win_rule,
            },
        ],
        goal,
        goal_vars: vec![(w, "W".to_owned())],
        meta_sorts: HashMap::new(),
    };

    let outcome =
        decided(resolve_fol(&mut dag, &program, &empty_ctx(), &Budget::default()).unwrap());
    assert_eq!(outcome.status, BudgetStatus::Ok);

    // Explicit three-valued verdicts.
    let win_a = atom(&mut dag, "win", vec![a]);
    let win_b = atom(&mut dag, "win", vec![b]);
    let win_c = atom(&mut dag, "win", vec![c]);
    let win_d = atom(&mut dag, "win", vec![d]);
    assert_eq!(
        outcome.truth_of(&dag, win_c),
        Truth::True,
        "win(c): move to lost d ⇒ won"
    );
    assert_eq!(
        outcome.truth_of(&dag, win_d),
        Truth::False,
        "win(d): no move ⇒ lost"
    );
    assert_eq!(
        outcome.truth_of(&dag, win_a),
        Truth::Undefined,
        "win(a): even cycle ⇒ undefined"
    );
    assert_eq!(
        outcome.truth_of(&dag, win_b),
        Truth::Undefined,
        "win(b): even cycle ⇒ undefined"
    );

    // The only TRUE goal answer is win(c).
    let ws: Vec<String> = outcome
        .answers
        .iter()
        .map(|a| a.bindings["W"].clone())
        .collect();
    assert_eq!(ws, vec!["c".to_owned()], "only c is a won position: {ws:?}");
    assert_all_proofs_check(&mut dag, &outcome);
}

#[test]
fn direct_negative_loop_is_undefined_not_true_or_false() {
    // p :- not p.   ⇒   p is Undefined under the well-founded model.
    let mut dag = TermDag::new();
    let p = atom(&mut dag, "p", vec![]);
    let program = FolProgram {
        clauses: vec![FolClause {
            head: p,
            body: vec![FolLit::Neg(p)],
            rule_iri: rule_iri(&mut dag, 0),
        }],
        goal: p,
        goal_vars: vec![],
        meta_sorts: HashMap::new(),
    };
    let outcome =
        decided(resolve_fol(&mut dag, &program, &empty_ctx(), &Budget::default()).unwrap());
    assert_eq!(
        outcome.truth_of(&dag, p),
        Truth::Undefined,
        "p :- not p is the canonical undefined atom, never fabricated true/false"
    );
    assert!(
        outcome.answers.is_empty(),
        "an undefined goal yields no true answer"
    );
}

// ── Test 3: budget incompleteness (sound partial) ───────────────────────────────────

#[test]
fn open_peano_nat_under_small_budget_is_sound_partial() {
    // nat(0).  nat(s(X)) :- nat(X).   ?- nat(N).   (open, infinite Herbrand base)
    let mut dag = TermDag::new();
    let zero = iri(&mut dag, "zero");
    let nat_zero = atom(&mut dag, "nat", vec![zero]);

    let (_xm, x) = var(&mut dag);
    let sx = succ(&mut dag, x);
    let nat_sx = atom(&mut dag, "nat", vec![sx]);
    let nat_x = atom(&mut dag, "nat", vec![x]);

    let (_nm, n) = var(&mut dag);
    let goal = atom(&mut dag, "nat", vec![n]);

    let program = FolProgram {
        clauses: vec![
            FolClause {
                head: nat_zero,
                body: vec![],
                rule_iri: rule_iri(&mut dag, 0),
            },
            FolClause {
                head: nat_sx,
                body: vec![FolLit::Pos(nat_x)],
                rule_iri: rule_iri(&mut dag, 1),
            },
        ],
        goal,
        goal_vars: vec![(n, "N".to_owned())],
        meta_sorts: HashMap::new(),
    };

    let budget = Budget {
        max_answers: None,
        max_steps: Some(3),
    };
    let outcome = decided(resolve_fol(&mut dag, &program, &empty_ctx(), &budget).unwrap());
    assert_eq!(
        outcome.status,
        BudgetStatus::Exhausted,
        "an open term-growth query must exhaust"
    );
    assert!(
        !outcome.answers.is_empty(),
        "a sound partial set still has some answers"
    );

    // Every returned binding is a genuine Peano numeral (sound: a subset of the true model).
    let genuine = ["zero", "s(zero)", "s(s(zero))", "s(s(s(zero)))"];
    for ans in &outcome.answers {
        assert!(
            genuine.contains(&ans.bindings["N"].as_str()),
            "returned N={} must be a genuine nat instance",
            ans.bindings["N"]
        );
    }
    assert_all_proofs_check(&mut dag, &outcome);
}

// ── Test 4: determinism (byte-identical partial set across runs) ─────────────────────

#[test]
fn budget_partial_answer_set_is_deterministic_across_runs() {
    fn run_once() -> (Vec<Vec<(String, String)>>, BudgetStatus) {
        let mut dag = TermDag::new();
        let zero = iri(&mut dag, "zero");
        let nat_zero = atom(&mut dag, "nat", vec![zero]);
        let (_xm, x) = var(&mut dag);
        let sx = succ(&mut dag, x);
        let nat_sx = atom(&mut dag, "nat", vec![sx]);
        let nat_x = atom(&mut dag, "nat", vec![x]);
        let (_nm, n) = var(&mut dag);
        let goal = atom(&mut dag, "nat", vec![n]);
        let program = FolProgram {
            clauses: vec![
                FolClause {
                    head: nat_zero,
                    body: vec![],
                    rule_iri: rule_iri(&mut dag, 0),
                },
                FolClause {
                    head: nat_sx,
                    body: vec![FolLit::Pos(nat_x)],
                    rule_iri: rule_iri(&mut dag, 1),
                },
            ],
            goal,
            goal_vars: vec![(n, "N".to_owned())],
            meta_sorts: HashMap::new(),
        };
        let budget = Budget {
            max_answers: None,
            max_steps: Some(5),
        };
        let outcome = decided(resolve_fol(&mut dag, &program, &empty_ctx(), &budget).unwrap());
        let rows: Vec<Vec<(String, String)>> = outcome
            .answers
            .iter()
            .map(|a| {
                a.bindings
                    .iter()
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .collect()
            })
            .collect();
        (rows, outcome.status)
    }

    let first = run_once();
    for _ in 0..4 {
        assert_eq!(
            run_once(),
            first,
            "the sound partial set must be byte-identical across runs"
        );
    }
    assert_eq!(first.1, BudgetStatus::Exhausted);
    assert!(!first.0.is_empty());
}

// ── Test 5: genuinely unsupported goal shape → typed hard-fail ───────────────────────

#[test]
fn floundering_naf_goal_is_typed_unsupported_not_a_fabricated_answer() {
    // p(X) :- not q(X).   ?- p(X).   X under negation is not range-restricted ⇒ flounders.
    let mut dag = TermDag::new();
    let (_xm, x) = var(&mut dag);
    let p_x = atom(&mut dag, "p", vec![x]);
    let q_x = atom(&mut dag, "q", vec![x]);
    let (_x2, x2) = var(&mut dag);
    let goal = atom(&mut dag, "p", vec![x2]);

    let program = FolProgram {
        clauses: vec![FolClause {
            head: p_x,
            body: vec![FolLit::Neg(q_x)],
            rule_iri: rule_iri(&mut dag, 0),
        }],
        goal,
        goal_vars: vec![(x2, "X".to_owned())],
        meta_sorts: HashMap::new(),
    };

    match resolve_fol(&mut dag, &program, &empty_ctx(), &Budget::default()).unwrap() {
        FolControl::Unsupported(kind) => {
            assert_eq!(
                kind,
                UnsupportedKind::Floundering,
                "an unbound negated goal flounders"
            );
        }
        FolControl::Decided(o) => {
            panic!(
                "a floundering goal must NOT fabricate an answer: {:?}",
                o.answers
            )
        }
    }
}

// ── Test 6: order-sorted resolution (ℤ ⊑ ℝ) ─────────────────────────────────────────

#[test]
fn order_sorted_query_resolves_subsort_and_rejects_incomparable() {
    // Fact p(one) where one : Integer.  Goal p(X) with X : RealNumber (ℤ ⊑ ℝ) resolves;
    // a control with X : an incomparable sort yields no answer.
    fn run(sort_of_x: fn(&mut TermDag) -> NodeId) -> usize {
        let mut dag = TermDag::new();
        let integer = iri(&mut dag, "Integer");
        let real = iri(&mut dag, "RealNumber");
        let one = iri(&mut dag, "one");

        // Fact p(one).
        let fact = atom(&mut dag, "p", vec![one]);
        // Goal p(X).
        let (xm, x) = var(&mut dag);
        let goal = atom(&mut dag, "p", vec![x]);

        // Sort lattice ℤ ⊑ ℝ and the sort of the constant `one` = Integer.
        let order = SortOrder::from_subclass_edges(&[(integer, real)]);
        let mut term_sorts = HashMap::new();
        term_sorts.insert(one, integer);
        let ctx = SortContext::new(order, term_sorts, HashMap::new());

        // The metavariable X's declared sort (Real for the positive case; an incomparable
        // sort for the control).
        let x_sort = sort_of_x(&mut dag);
        let _ = real;
        let mut meta_sorts = HashMap::new();
        meta_sorts.insert(xm, x_sort);

        let program = FolProgram {
            clauses: vec![FolClause {
                head: fact,
                body: vec![],
                rule_iri: rule_iri(&mut dag, 0),
            }],
            goal,
            goal_vars: vec![(x, "X".to_owned())],
            meta_sorts,
        };
        let outcome = decided(resolve_fol(&mut dag, &program, &ctx, &Budget::default()).unwrap());
        if !outcome.answers.is_empty() {
            assert_all_proofs_check(&mut dag, &outcome);
        }
        outcome.answers.len()
    }

    // Positive: X : RealNumber accepts the Integer-sorted constant (ℤ ⊑ ℝ).
    let accepted = run(|dag| iri(dag, "RealNumber"));
    assert_eq!(
        accepted, 1,
        "an Integer constant binds a RealNumber variable (ℤ ⊑ ℝ)"
    );

    // Control: X : an incomparable sort rejects the Integer constant.
    let rejected = run(|dag| iri(dag, "Color"));
    assert_eq!(
        rejected, 0,
        "an Integer constant does NOT bind an incomparable-sort variable"
    );
}

// ── Test 7: QProgram → resolve_native_fol dispatch entry (QTerm::Struct wiring) ───────

#[test]
fn structured_qprogram_routes_through_resolve_native_fol() {
    use crate::query_ir::{QAtom, QGoal, QProgram, QRule, QTerm, StructNode};

    // Build the Peano add program on a shared DAG, referencing compound terms via
    // QTerm::Struct, then resolve it through the dispatch-facing entry.
    let mut dag = TermDag::new();
    let zero = iri(&mut dag, "zero");

    let struct_t = |n: NodeId| QTerm::Struct(StructNode::new(n));
    let vt = |s: &str| QTerm::Var(s.to_owned());

    // Fact add(zero, Y, Y).
    let fact = QRule {
        head: QAtom {
            pred: "add".to_owned(),
            args: vec![struct_t(zero), vt("Y"), vt("Y")],
        },
        body: vec![],
    };
    // Rule add(s(X), Y, s(Z)) :- add(X, Y, Z).  s(X)/s(Z) are Struct over fresh metas... but
    // for a QProgram the variables X/Z live inside the compound term, so we build them as
    // Struct nodes whose leaves are the SAME metavariables the flat Var positions use. To
    // keep the surface simple we express the recursive rule directly over DAG nodes: the
    // dispatch entry lowers flat Var/Const/Struct uniformly, so a Struct carrying a bound
    // metavariable shares identity with a Var of the same name only if we thread the node.
    // Here we exercise the *goal* structured path (the integration point) with a ground
    // query, which is the minimal end-to-end wire.
    let two = {
        let s0 = succ(&mut dag, zero);
        succ(&mut dag, s0)
    };
    let one = succ(&mut dag, zero);
    // ?- add(s(s(zero)), s(zero), R).
    let goal = QGoal {
        atoms: vec![QAtom {
            pred: "add".to_owned(),
            args: vec![struct_t(two), struct_t(one), vt("R")],
        }],
    };

    // Recursive rule authored directly on the DAG and expressed as Struct args sharing the
    // clause metavariables.
    let (_xm, x) = var(&mut dag);
    let (_zm, z) = var(&mut dag);
    let sx = succ(&mut dag, x);
    let sz = succ(&mut dag, z);
    let step = QRule {
        head: QAtom {
            pred: "add".to_owned(),
            args: vec![struct_t(sx), vt("Y"), struct_t(sz)],
        },
        body: vec![crate::query_ir::QBodyLit::Atom(QAtom {
            pred: "add".to_owned(),
            args: vec![struct_t(x), vt("Y"), struct_t(z)],
        })],
    };

    let program = QProgram {
        rules: vec![fact, step],
        goal,
        counterfactual: None,
        prob_facts: vec![],
        prob_model: None,
        confidences: vec![],
    };

    assert!(
        program_is_structured(&program),
        "a Struct-bearing program is structured"
    );

    let outcome = resolve_native_fol(&mut dag, &program, &Budget::default()).unwrap();
    match outcome {
        NativeOutcome::Decided(answer) => {
            assert_eq!(answer.status, BudgetStatus::Ok);
            assert_eq!(
                answer.bindings.len(),
                1,
                "one add answer: {:?}",
                answer.bindings
            );
            assert_eq!(answer.bindings[0]["R"], "s(s(s(zero)))");
        }
        NativeOutcome::Unsupported(kind) => panic!("structured program must resolve, got {kind:?}"),
    }
}
