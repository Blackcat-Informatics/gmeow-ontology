// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Acceptance tests for the structured full-FOL resolver + SLG-WFS.

use std::collections::HashMap;

use purrdf::TermValue;

use super::*;
use crate::physical::id::{MetaId, NodeId, TermId};
use crate::physical::proof::check;
use crate::physical::unify::{SortContext, SortOrder};
use gmeow_term_arena::engine::TermDag;

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
        FolControl::Decided(o) => *o,
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

    let arena = dag.arena();
    let struct_t = move |n: NodeId| QTerm::Struct(StructNode::wrap(n, arena));
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

// ── G1: WFS soundness under budget cut — prefer Undefined, never fabricate True ──────

#[test]
fn g1_budget_cut_demotes_dependent_atom_to_undefined_not_true() {
    // `p :- not q.` + fact `q.` under Budget{max_steps: 1}: round 1 fires the ONLY
    // demanded call (`p`), discovering the ground rule `p :- not q` (q is merely
    // DEMANDED, not yet derived, since the unit fact `q.` was never itself a demanded
    // call this round). The budget cuts after that single step, so `q`'s founding fact
    // is never reached. A resolver that fabricates True for `p` here (reading the
    // not-yet-grounded `q` as false) would be UNSOUND; the correct verdict is
    // Undefined — incomplete, never wrong.
    let mut dag = TermDag::new();
    let p = atom(&mut dag, "p", vec![]);
    let q = atom(&mut dag, "q", vec![]);

    let program = FolProgram {
        clauses: vec![
            // fact: q.
            FolClause {
                head: q,
                body: vec![],
                rule_iri: rule_iri(&mut dag, 0),
            },
            // rule: p :- not q.
            FolClause {
                head: p,
                body: vec![FolLit::Neg(q)],
                rule_iri: rule_iri(&mut dag, 1),
            },
        ],
        goal: p,
        goal_vars: vec![],
        meta_sorts: HashMap::new(),
    };

    let budget = Budget {
        max_answers: None,
        max_steps: Some(1),
    };
    let outcome = decided(resolve_fol(&mut dag, &program, &empty_ctx(), &budget).unwrap());
    assert_eq!(
        outcome.status,
        BudgetStatus::Exhausted,
        "the single-step budget must cut the grounding"
    );
    assert_eq!(
        outcome.truth_of(&dag, p),
        Truth::Undefined,
        "a budget cut must PREFER Undefined over a fabricated True for an atom whose \
         negative dependency was never proven complete"
    );
    assert!(
        outcome.answers.is_empty(),
        "an Undefined goal yields no TRUE answer: {:?}",
        outcome.answers
    );
}

// ── G6: content-key rule identity — firing_rule_iri hashes the STRING, not TermId ────

#[test]
fn g6_firing_rule_iri_is_content_addressed_across_independent_dags() {
    // Build the "same" ground rule instance `p(a) :- q(a)` in two INDEPENDENT DAGs whose
    // interning HISTORY differs (a different amount of unrelated "noise" interned first
    // shifts every TermId/NodeId ordinal), and assert `firing_rule_iri` still mints the
    // BYTE-IDENTICAL IRI — it must fold the rule IRI's STRING, never a per-arena TermId
    // ordinal that leaks arena history.
    fn build(noise: usize, pred: &str) -> (TermDag, GroundRule) {
        let mut dag = TermDag::new();
        for i in 0..noise {
            iri(&mut dag, &format!("https://example.org/noise-{i}"));
        }
        let a = iri(&mut dag, "https://example.org/a");
        let head = atom(&mut dag, pred, vec![a]);
        let q_a = atom(&mut dag, "https://example.org/q", vec![a]);
        let rule_iri_tid = rule_iri(&mut dag, 0);
        let rule = GroundRule {
            head,
            pos: vec![q_a],
            neg: vec![],
            rule_iri: rule_iri_tid,
            unit: false,
        };
        (dag, rule)
    }

    let (mut dag1, rule1) = build(0, "p");
    let (mut dag2, rule2) = build(11, "p");
    // A negative control: genuinely different content (different head predicate) must
    // mint a DIFFERENT firing IRI, so the test cannot pass vacuously.
    let (mut dag3, rule3) = build(0, "p2");

    let iri1 = firing_rule_iri(&mut dag1, &rule1);
    let iri2 = firing_rule_iri(&mut dag2, &rule2);
    let iri3 = firing_rule_iri(&mut dag3, &rule3);

    let s1 = dag1.atom_display(iri1).to_owned();
    let s2 = dag2.atom_display(iri2).to_owned();
    let s3 = dag3.atom_display(iri3).to_owned();

    assert_eq!(
        s1, s2,
        "identical ground firings must fold to the SAME firing IRI regardless of the \
         arena's independent interning history (never a TermId ordinal)"
    );
    assert_ne!(
        s1, s3,
        "genuinely different content must still mint a DIFFERENT firing IRI"
    );
}

// ── G9: table-call identity folds in metavariable sorts ──────────────────────────────

#[test]
fn g9_canon_call_key_distinguishes_metavariable_sort() {
    // Two otherwise-variant-identical calls that differ ONLY in a metavariable's
    // declared sort must key to DISTINCT canonical call patterns — else an order-sorted
    // answer set collapses onto one shared (and semantically wrong) table entry.
    let mut dag = TermDag::new();
    let sort_a = iri(&mut dag, "https://example.org/SortA");
    let sort_b = iri(&mut dag, "https://example.org/SortB");
    let (m, x) = var(&mut dag);
    let call = atom(&mut dag, "p", vec![x]);

    let mut sorts_a: HashMap<MetaId, NodeId> = HashMap::new();
    sorts_a.insert(m, sort_a);
    let mut sorts_b: HashMap<MetaId, NodeId> = HashMap::new();
    sorts_b.insert(m, sort_b);
    let sorts_none: HashMap<MetaId, NodeId> = HashMap::new();

    let key_a = canon(&dag, call, &sorts_a);
    let key_b = canon(&dag, call, &sorts_b);
    let key_none = canon(&dag, call, &sorts_none);

    assert_ne!(
        key_a, key_b,
        "the SAME variant pattern with two DIFFERENT declared metavariable sorts must \
         key to two DISTINCT table entries"
    );
    assert_ne!(
        key_a, key_none,
        "a sorted call must not collide with the unsorted key"
    );
    assert_ne!(
        key_b, key_none,
        "a sorted call must not collide with the unsorted key"
    );

    // Sanity: an identical sort still shares one key (the fix must not over-distinguish).
    let mut sorts_a2: HashMap<MetaId, NodeId> = HashMap::new();
    sorts_a2.insert(m, sort_a);
    assert_eq!(
        key_a,
        canon(&dag, call, &sorts_a2),
        "the SAME declared sort must still share one canonical key"
    );
}

// ── G10: binder-valued answers survive dedup (no collapse to "<binder>") ─────────────

#[test]
fn g10_distinct_binder_valued_answers_both_survive() {
    // Two facts `wraps(B1).` / `wraps(B2).` whose argument is a STRUCTURALLY DISTINCT
    // binder term must both surface as distinct goal answers for `?- wraps(X)`. A
    // renderer that collapses every binder to the literal `"<binder>"` would make both
    // answers bind `X` to the SAME surface string, so the dedup-by-binding-surface step
    // in `project` would silently drop one of two genuinely distinct answers.
    let mut dag = TermDag::new();
    let sort = iri(&mut dag, "https://example.org/Sort");
    let forall = iri(&mut dag, "https://example.org/forall");
    let p_op = iri(&mut dag, "https://example.org/p");
    let q_op = iri(&mut dag, "https://example.org/q");

    let bound0 = dag.intern_bound(0, 0);
    let body_p = dag.intern_app(p_op, vec![bound0]);
    let bound0b = dag.intern_bound(0, 0);
    let body_q = dag.intern_app(q_op, vec![bound0b]);
    let binder1 = dag.intern_binder(forall, vec![sort], body_p);
    let binder2 = dag.intern_binder(forall, vec![sort], body_q);
    assert_ne!(
        binder1, binder2,
        "structurally distinct binders (test setup)"
    );

    let wraps1 = atom(&mut dag, "wraps", vec![binder1]);
    let wraps2 = atom(&mut dag, "wraps", vec![binder2]);

    let (_xm, x) = var(&mut dag);
    let goal = atom(&mut dag, "wraps", vec![x]);

    let program = FolProgram {
        clauses: vec![
            FolClause {
                head: wraps1,
                body: vec![],
                rule_iri: rule_iri(&mut dag, 0),
            },
            FolClause {
                head: wraps2,
                body: vec![],
                rule_iri: rule_iri(&mut dag, 1),
            },
        ],
        goal,
        goal_vars: vec![(x, "X".to_owned())],
        meta_sorts: HashMap::new(),
    };

    let outcome =
        decided(resolve_fol(&mut dag, &program, &empty_ctx(), &Budget::default()).unwrap());
    assert_eq!(outcome.status, BudgetStatus::Ok);
    assert_eq!(
        outcome.answers.len(),
        2,
        "both distinct binder-valued answers must survive, not collapse to one: {:?}",
        outcome
            .answers
            .iter()
            .map(|a| a.bindings["X"].clone())
            .collect::<Vec<_>>()
    );
    let mut vals: Vec<String> = outcome
        .answers
        .iter()
        .map(|a| a.bindings["X"].clone())
        .collect();
    vals.sort();
    assert_ne!(
        vals[0], vals[1],
        "the two surfaces must actually be distinct"
    );
    assert_all_proofs_check(&mut dag, &outcome);
}

// ── G11: comma-bearing answer surfaces collide under render, but both survive ────────

#[test]
fn g11_comma_bearing_answer_surfaces_collide_but_both_survive_by_content_key() {
    // Two facts `wraps(f(a, b)).` / `wraps(f("a,b")).` whose goal-variable binding renders to
    // the SAME comma-joined surface `"f(a,b)"` — the 2-ary application of two leaves and the
    // 1-ary application of a single IRI leaf whose lexical text CONTAINS a comma both render
    // identically, because `render` comma-joins `App` arguments. Their ARENA CONTENT KEYS
    // differ (distinct arity + content). `project` must dedup by the content key, NOT by the
    // rendered surface; keying on the surface would silently drop one of two genuinely distinct
    // answers. Complete answer accumulation must retain both distinct values even
    // though their rendered surfaces collide.
    let mut dag = TermDag::new();
    let a = iri(&mut dag, "a");
    let b = iri(&mut dag, "b");
    // A single IRI leaf whose lexical text legally contains a comma.
    let ab = iri(&mut dag, "a,b");
    let t1 = atom(&mut dag, "f", vec![a, b]); // renders "f(a,b)"
    let t2 = atom(&mut dag, "f", vec![ab]); // renders "f(a,b)" — SAME surface, DISTINCT content

    // Test preconditions: the two surfaces collide, but the content keys differ.
    assert_eq!(
        render(&dag, t1),
        render(&dag, t2),
        "precondition: the two terms render to the SAME comma-joined surface"
    );
    assert_eq!(
        render(&dag, t1),
        "f(a,b)",
        "the colliding surface is f(a,b)"
    );
    assert_ne!(
        dag.key(t1),
        dag.key(t2),
        "precondition: the two terms have DISTINCT arena content keys"
    );

    let wraps1 = atom(&mut dag, "wraps", vec![t1]);
    let wraps2 = atom(&mut dag, "wraps", vec![t2]);

    let (_xm, x) = var(&mut dag);
    let goal = atom(&mut dag, "wraps", vec![x]);

    let program = FolProgram {
        clauses: vec![
            FolClause {
                head: wraps1,
                body: vec![],
                rule_iri: rule_iri(&mut dag, 0),
            },
            FolClause {
                head: wraps2,
                body: vec![],
                rule_iri: rule_iri(&mut dag, 1),
            },
        ],
        goal,
        goal_vars: vec![(x, "X".to_owned())],
        meta_sorts: HashMap::new(),
    };

    let outcome =
        decided(resolve_fol(&mut dag, &program, &empty_ctx(), &Budget::default()).unwrap());
    assert_eq!(outcome.status, BudgetStatus::Ok);
    assert_eq!(
        outcome.answers.len(),
        2,
        "both answers whose surfaces collide under comma-joining must survive (content-keyed \
         dedup), not collapse to one: {:?}",
        outcome
            .answers
            .iter()
            .map(|a| a.bindings["X"].clone())
            .collect::<Vec<_>>()
    );
    // Both bind X to the IDENTICAL rendered surface — that is the whole point; only the content
    // key distinguishes them, and both survive because dedup keys on content, not the surface.
    let surfaces: Vec<String> = outcome
        .answers
        .iter()
        .map(|a| a.bindings["X"].clone())
        .collect();
    assert_eq!(
        surfaces,
        vec!["f(a,b)".to_owned(), "f(a,b)".to_owned()],
        "both distinct answers render to the same human-facing surface"
    );
    // The two answers are distinct arena atoms (the content that dedup preserved).
    assert_ne!(
        outcome.answers[0].atom, outcome.answers[1].atom,
        "the two surviving answers are distinct arena atoms"
    );
    assert_all_proofs_check(&mut dag, &outcome);
}

// ── G8: budget on the structured (resolve_native_fol) result path ───────────────────

#[test]
fn g8_structured_dispatch_enforces_max_answers_and_marks_partial() {
    use crate::query_ir::{QAtom, QGoal, QProgram, QRule, QTerm, StructNode};

    // Three facts `m(a, list).` / `m(b, list).` / `m(c, list).` over a structured
    // (Struct-bearing) second argument, so the program routes through the full-FOL
    // dispatch entry `resolve_native_fol`. Budget{max_answers: Some(2)} must truncate
    // the 3-answer set to a deterministic 2-answer PARTIAL result — the structured
    // result path must not silently ignore `max_answers`.
    let mut dag = TermDag::new();
    let nil = iri(&mut dag, "nil");
    let elem = iri(&mut dag, "elem");
    let list = atom(&mut dag, "cons", vec![elem, nil]);
    let a = iri(&mut dag, "a");
    let b = iri(&mut dag, "b");
    let c = iri(&mut dag, "c");

    let arena = dag.arena();
    let struct_t = move |n: NodeId| QTerm::Struct(StructNode::wrap(n, arena));
    let vt = |s: &str| QTerm::Var(s.to_owned());

    let fact = |val: NodeId| QRule {
        head: QAtom {
            pred: "m".to_owned(),
            args: vec![struct_t(val), struct_t(list)],
        },
        body: vec![],
    };

    let program = QProgram {
        rules: vec![fact(a), fact(b), fact(c)],
        goal: QGoal {
            atoms: vec![QAtom {
                pred: "m".to_owned(),
                args: vec![vt("X"), struct_t(list)],
            }],
        },
        counterfactual: None,
        prob_facts: vec![],
        prob_model: None,
        confidences: vec![],
    };

    assert!(program_is_structured(&program));

    let budget = Budget {
        max_answers: Some(2),
        max_steps: None,
    };
    let outcome = resolve_native_fol(&mut dag, &program, &budget).unwrap();
    match outcome {
        NativeOutcome::Decided(answer) => {
            assert_eq!(
                answer.status,
                BudgetStatus::Partial,
                "a reached answer cap must stamp Partial"
            );
            assert_eq!(
                answer.bindings.len(),
                2,
                "the answer set must be truncated to max_answers: {:?}",
                answer.bindings
            );
            let vals: Vec<&str> = answer.bindings.iter().map(|b| b["X"].as_str()).collect();
            assert_eq!(
                vals,
                vec!["a", "b"],
                "truncation must be deterministic (the canonicalized sorted prefix)"
            );
        }
        NativeOutcome::Unsupported(kind) => panic!("structured program must resolve, got {kind:?}"),
    }
}

// ── Gap B: clause body wider than 64 literals is a typed refusal, never a silent cap ──

#[test]
fn clause_body_wider_than_64_literals_is_unsupported() {
    // p :- q0, q1, …, q64.  (65 distinct positive atoms) ?- p.
    // `solve_body` represents the not-yet-selected body literals as a `u64` bitmask (one
    // bit per literal); a body this wide cannot fit, so `resolve_fol` must reject it
    // up front as a typed `ClauseBodyTooWide` gap rather than silently truncate/overflow
    // the mask.
    let mut dag = TermDag::new();
    let p = atom(&mut dag, "p", vec![]);
    let body: Vec<FolLit> = (0..65)
        .map(|i| FolLit::Pos(atom(&mut dag, &format!("q{i}"), vec![])))
        .collect();
    assert_eq!(body.len(), 65, "test setup: body wider than 64 literals");

    let program = FolProgram {
        clauses: vec![FolClause {
            head: p,
            body,
            rule_iri: rule_iri(&mut dag, 0),
        }],
        goal: p,
        goal_vars: vec![],
        meta_sorts: HashMap::new(),
    };

    match resolve_fol(&mut dag, &program, &empty_ctx(), &Budget::default()).unwrap() {
        FolControl::Unsupported(kind) => {
            assert_eq!(
                kind,
                UnsupportedKind::ClauseBodyTooWide,
                "a body wider than 64 literals must be the typed ClauseBodyTooWide refusal"
            );
        }
        FolControl::Decided(o) => {
            panic!(
                "a clause body wider than the u64 mask must NOT be silently evaluated: {:?}",
                o.answers
            )
        }
    }
}

// ── Safe literal selection (SIPS): body-literal order independence ──────────────────

#[test]
fn safe_literal_selection_win_move_is_order_independent() {
    // The SAME win/move game as `win_move_game_has_correct_well_founded_model`, but with
    // the negative literal authored BEFORE the positive literal that binds its
    // variable — `win(X) :- not win(Y), move(X, Y).` — must resolve to the IDENTICAL
    // three-valued model. Body-literal selection must be SAFE (the standard SLG safe
    // computation / SIPS rule: a negative literal is selected only once its variables
    // are bound by a preceding — in SELECTION order, not authored order — positive
    // literal), so this must NOT flounder merely because the authored conjunct order
    // placed the negative literal first.
    let mut dag = TermDag::new();
    let a = iri(&mut dag, "a");
    let b = iri(&mut dag, "b");
    let c = iri(&mut dag, "c");
    let d = iri(&mut dag, "d");

    let mk_move = |dag: &mut TermDag, x: NodeId, y: NodeId| atom(dag, "move", vec![x, y]);
    let move_ab = mk_move(&mut dag, a, b);
    let move_ba = mk_move(&mut dag, b, a);
    let move_cd = mk_move(&mut dag, c, d);

    // win(X) :- not win(Y), move(X, Y).   — REVERSED body order vs the sibling test.
    let (_xm, x) = var(&mut dag);
    let (_ym, y) = var(&mut dag);
    let win_x = atom(&mut dag, "win", vec![x]);
    let move_xy = atom(&mut dag, "move", vec![x, y]);
    let win_y = atom(&mut dag, "win", vec![y]);
    let win_rule = rule_iri(&mut dag, 3);

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
                body: vec![FolLit::Neg(win_y), FolLit::Pos(move_xy)],
                rule_iri: win_rule,
            },
        ],
        goal,
        goal_vars: vec![(w, "W".to_owned())],
        meta_sorts: HashMap::new(),
    };

    let outcome =
        decided(resolve_fol(&mut dag, &program, &empty_ctx(), &Budget::default()).unwrap());
    assert_eq!(
        outcome.status,
        BudgetStatus::Ok,
        "must NOT flounder merely because the authored body order was unlucky"
    );

    let win_a = atom(&mut dag, "win", vec![a]);
    let win_b = atom(&mut dag, "win", vec![b]);
    let win_c = atom(&mut dag, "win", vec![c]);
    let win_d = atom(&mut dag, "win", vec![d]);
    assert_eq!(
        outcome.truth_of(&dag, win_c),
        Truth::True,
        "win(c): move to lost d"
    );
    assert_eq!(
        outcome.truth_of(&dag, win_d),
        Truth::False,
        "win(d): no move"
    );
    assert_eq!(
        outcome.truth_of(&dag, win_a),
        Truth::Undefined,
        "even cycle"
    );
    assert_eq!(
        outcome.truth_of(&dag, win_b),
        Truth::Undefined,
        "even cycle"
    );

    let ws: Vec<String> = outcome
        .answers
        .iter()
        .map(|a| a.bindings["W"].clone())
        .collect();
    assert_eq!(ws, vec!["c".to_owned()], "only c is a won position: {ws:?}");
    assert_all_proofs_check(&mut dag, &outcome);
}
