// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use crate::query_ir::QGoal;
use crate::seam::WorldFactSnapshot;
use crate::store::WorldStore;

const W: &str = "http://logic.test/world/magic-generic";
const PROFILE: &str = "https://blackcatinformatics.ca/logic/PositiveHornProfile";
const P1: &str = "http://ex/p1";
const P2: &str = "http://ex/p2";

struct SumProduct;

impl TupleAnnotationAlgebra for SumProduct {
    type Element = f64;

    fn identity(&self) -> &str {
        "https://blackcatinformatics.ca/logic/algebra/test-sum-product-v1"
    }

    fn canonical_element(&self, element: &Self::Element) -> String {
        format!("{:016x}", element.to_bits())
    }

    fn zero(&self) -> Self::Element {
        0.0
    }

    fn one(&self) -> Self::Element {
        1.0
    }

    fn add(
        &self,
        left: &Self::Element,
        right: &Self::Element,
    ) -> gmeow_errors::Result<Self::Element> {
        Ok(left + right)
    }

    fn multiply(
        &self,
        left: &Self::Element,
        right: &Self::Element,
    ) -> gmeow_errors::Result<Self::Element> {
        Ok(left * right)
    }
}

fn make_world(triples: &[(&str, &str, &str)]) -> (WorldStore, String) {
    let store = WorldStore::new();
    for (s, p, o) in triples {
        store.insert_quad(W, s, p, o);
    }
    (store, W.to_owned())
}

/// A `triple(?s, <pred>, ?o, ?w)` atom over the generic-triple encoding: the relation
/// is the BARE `triple` symbol (matching `build_generic_edb`'s `push_fact("triple",…)`),
/// the predicate position pinned to `<pred>` and the rest variables `s`/`o`/`w`.
fn triple_atom(pred: &str) -> QAtom {
    QAtom {
        pred: "triple".to_owned(),
        args: vec![
            QTerm::Var("s".to_owned()),
            QTerm::Const(format!("<{pred}>")),
            QTerm::Var("o".to_owned()),
            QTerm::Var("w".to_owned()),
        ],
    }
}

/// The sub-property propagation program: `triple(?s,<p2>,?o,?w) :- triple(?s,<p1>,?o,?w)`
/// with the arity-4 backward goal `triple(?s,<p2>,?o,?w)` — NOT binary-eligible.
fn subprop_program() -> QProgram {
    QProgram {
        rules: vec![crate::query_ir::QRule {
            head: triple_atom(P2),
            body: vec![QBodyLit::Atom(triple_atom(P1))],
        }],
        goal: QGoal {
            atoms: vec![triple_atom(P2)],
        },
        counterfactual: None,
        prob_facts: vec![],
        prob_model: None,
        confidences: vec![],
    }
}

fn decided(outcome: NativeOutcome<AnswerSet>) -> AnswerSet {
    match outcome {
        NativeOutcome::Decided(a) => a,
        NativeOutcome::Unsupported(k) => panic!("expected Decided, got Unsupported({k:?})"),
    }
}

// ── (b) N-ary decides: genuine predicate-as-data backward resolution ─────────

#[test]
fn generic_subproperty_propagation_decides_nary_goal() {
    // A single <p1> edge x→y; the sub-property rule must derive x <p2> y, and the
    // arity-4 backward goal must return that derived edge — resolution the binary
    // store cannot express (the predicate rides in a DATA position).
    let (store, world) = make_world(&[("http://ex/x", P1, "http://ex/y")]);
    let foreign = WorldFactSnapshot::from_world(&store, W, PROFILE).unwrap();
    let prog = subprop_program();

    // The dispatch signal: the goal atom is arity 4, so resolve_native routes here.
    assert_eq!(prog.goal.atoms[0].args.len(), 4, "arity != 2 ⇒ n-ary path");

    let answer = decided(
        super::super::magic::resolve_native(&foreign, &world, &prog, &Budget::default()).unwrap(),
    );
    assert_eq!(answer.status, BudgetStatus::Ok);
    assert_eq!(
        answer.bindings.len(),
        1,
        "exactly one derived <p2> edge: {answer:?}"
    );
    let b = &answer.bindings[0];
    assert_eq!(b["s"], "<http://ex/x>", "subject binding");
    assert_eq!(b["o"], "<http://ex/y>", "object binding");
    assert_eq!(b["w"], format!("<{W}>"), "world binding");
}

#[test]
fn generic_goal_with_no_matching_edge_decides_empty() {
    // Only a <p2>-unrelated edge under a DIFFERENT predicate: no <p1> edge to
    // propagate, so the demand-restricted fixpoint derives nothing.
    let (store, world) = make_world(&[("http://ex/x", "http://ex/other", "http://ex/y")]);
    let foreign = WorldFactSnapshot::from_world(&store, W, PROFILE).unwrap();
    let prog = subprop_program();
    let answer = decided(
        super::super::magic::resolve_native(&foreign, &world, &prog, &Budget::default()).unwrap(),
    );
    assert!(answer.bindings.is_empty(), "no <p1> edge ⇒ no answers");
}

// ── N-ary backward budget: the step governor threads the single counting point ─

/// A transitive `<trans>` relation in the `triple(?s,?p,?o,?w)` encoding: the recursive
/// rule `triple(?s,<trans>,?o,?w) :- triple(?s,<trans>,?m,?w), triple(?m,<trans>,?o,?w)`
/// with the arity-4 backward goal `triple(?s,<trans>,?o,?w)` — n-ary (routes here).
fn transitive_program() -> QProgram {
    let trans = "http://ex/trans";
    let edge = |s: &str, o: &str, w: &str| QAtom {
        pred: "triple".to_owned(),
        args: vec![
            QTerm::Var(s.to_owned()),
            QTerm::Const(format!("<{trans}>")),
            QTerm::Var(o.to_owned()),
            QTerm::Var(w.to_owned()),
        ],
    };
    QProgram {
        rules: vec![crate::query_ir::QRule {
            head: edge("s", "o", "w"),
            body: vec![
                QBodyLit::Atom(edge("s", "m", "w")),
                QBodyLit::Atom(edge("m", "o", "w")),
            ],
        }],
        goal: QGoal {
            atoms: vec![edge("s", "o", "w")],
        },
        counterfactual: None,
        prob_facts: vec![],
        prob_model: None,
        confidences: vec![],
    }
}

#[test]
fn generic_backward_max_steps_exhausts_with_sound_prefix() {
    // A 4-node <trans> chain a→b→c→d; the transitive closure adds a→c, b→d, a→d.
    let trans = "http://ex/trans";
    let (store, world) = make_world(&[
        ("http://ex/a", trans, "http://ex/b"),
        ("http://ex/b", trans, "http://ex/c"),
        ("http://ex/c", trans, "http://ex/d"),
    ]);
    let foreign = WorldFactSnapshot::from_world(&store, W, PROFILE).unwrap();
    let prog = transitive_program();

    // Unbudgeted: the full demand-restricted closure, complete (`Ok`).
    let full = decided(
        super::super::magic::resolve_native(&foreign, &world, &prog, &Budget::default()).unwrap(),
    );
    assert_eq!(full.status, BudgetStatus::Ok, "unbudgeted ⇒ Ok complete");
    // 3 EDB edges echoed + 3 derived transitive edges = 6 goal answers.
    assert_eq!(full.bindings.len(), 6, "full closure: {full:?}");
    let full_set: BTreeSet<(String, String)> = full
        .bindings
        .iter()
        .map(|b| (b["s"].clone(), b["o"].clone()))
        .collect();

    // Budgeted: a 1-step cut stamps `Exhausted` with a sound prefix (a strict subset).
    let budget = Budget {
        max_steps: Some(1),
        ..Default::default()
    };
    let cut =
        decided(super::super::magic::resolve_native(&foreign, &world, &prog, &budget).unwrap());
    assert_eq!(
        cut.status,
        BudgetStatus::Exhausted,
        "a 1-step budget cannot reach the closure ⇒ Exhausted: {cut:?}"
    );
    assert!(
        cut.bindings.len() < full.bindings.len(),
        "the cut answer set is a strict subset of the full closure: {cut:?}"
    );
    for b in &cut.bindings {
        assert!(
            full_set.contains(&(b["s"].clone(), b["o"].clone())),
            "every budget-cut answer is sound (present in the full closure): {b:?}"
        );
    }
}

// ── (c) Provenance: the derived answer carries its demand antecedent ─────────

#[test]
fn generic_derived_row_carries_demand_and_rule_provenance() {
    let (store, world) = make_world(&[("http://ex/x", P1, "http://ex/y")]);
    let foreign = WorldFactSnapshot::from_world(&store, W, PROFILE).unwrap();
    let prog = subprop_program();

    // Drive the transform + generic materialization directly to inspect provenance.
    let rules: Vec<GenericRule> = prog
        .rules
        .iter()
        .map(|r| {
            let head = generic_atom_of(&r.head).unwrap();
            let body: Vec<GenericAtom> = r
                .body
                .iter()
                .map(|l| match l {
                    QBodyLit::Atom(a) => generic_atom_of(a).unwrap(),
                    other => panic!("unexpected body literal {other:?}"),
                })
                .collect();
            let iri = format!("{}::rule", head.relation);
            generic_rule(head, body, iri)
        })
        .collect();
    let goal_atom = generic_atom_of(&prog.goal.atoms[0]).unwrap();
    let pattern = goal_pattern(&prog.goal.atoms[0]);
    let transformed = magic_transform_generic(&rules, &goal_atom, pattern).unwrap();

    // The goal seed is the ground magic fact carrying the single bound sub-tuple <p2>
    // (the property position) — arity 1, NOT a self-loop. It is the sole seed of this
    // EDB-first program (no bodyless demand/fact-rule is lifted).
    assert_eq!(transformed.seeds.len(), 1, "only the goal seed is lifted");
    let (seed_rel, seed_args) = transformed.seeds[0].clone();
    assert_eq!(seed_rel, magic_pred_iri("triple", "fbff"));
    assert_eq!(
        seed_args.len(),
        1,
        "one bound position ⇒ arity-1 bound sub-tuple"
    );
    assert_eq!(term_display(&seed_args[0]), format!("<{P2}>"));

    let source_patterns = generic_source_patterns(&rules, &goal_atom, &world, &BTreeSet::new());
    let mut facts = build_generic_edb(&foreign, &world, &source_patterns).unwrap();
    let ids: Vec<_> = seed_args.iter().map(|a| facts.intern(a)).collect();
    facts.push_fact(&seed_rel, ids);
    let (result, _status) = materialize_generic_budgeted(&facts, &transformed.rules, None).unwrap();

    // The derived triple(x, p2, y, w) row carries a firing rule name and its demand
    // antecedent (the magic guard) plus the antecedent <p1> edge.
    let derived = result
        .rows
        .iter()
        .find(|(row, prov)| {
            !prov.is_edb
                && row.predicate == "triple"
                && term_display(&row.args[1]) == format!("<{P2}>")
        })
        .expect("the derived <p2> edge must be present");
    let (_, prov) = derived;
    assert!(
        prov.rule_name.is_some(),
        "derived row names its firing rule"
    );
    let magic_rel = magic_pred_iri("triple", "fbff");
    assert!(
        prov.antecedents.iter().any(|a| a.predicate == magic_rel),
        "the derived answer carries its demand antecedent {magic_rel:?}: {:?}",
        prov.antecedents
    );
    assert!(
        prov.antecedents
            .iter()
            .any(|a| a.predicate == "triple" && term_display(&a.args[1]) == format!("<{P1}>")),
        "the derived answer carries its antecedent <p1> edge: {:?}",
        prov.antecedents
    );
}
#[test]
fn annotated_generic_dispatch_carries_score_and_positional_lineage_in_one_fixpoint() {
    let (store, world) = make_world(&[("http://ex/x", P1, "http://ex/y")]);
    let foreign = WorldFactSnapshot::from_world(&store, W, PROFILE).unwrap();
    let program = subprop_program();
    let contract = crate::annotation::AnnotationContract::exact();
    let request = AnnotationRequest::new(&SumProduct, &contract, |fact: AnnotationFactRef<'_>| {
        (fact.predicate == P1).then_some(2.5)
    });

    let answer = match resolve_native_generic_annotated(
        &foreign,
        &world,
        &program,
        &Budget::default(),
        &request,
    )
    .unwrap()
    {
        NativeOutcome::Decided(answer) => answer,
        NativeOutcome::Unsupported(kind) => panic!("unexpected n-ary refusal: {kind:?}"),
    };

    assert_eq!(
        answer.certification.query_class,
        AnnotationQueryClass::PositiveNaryAcyclic
    );
    assert_eq!(
        answer.certification.lineage_contract,
        AnnotationLineageContract::AllPhysicalDerivations
    );
    assert_eq!(answer.answers.len(), 1);
    assert_eq!(answer.answers[0].annotation, 2.5);
    let rule = answer.answers[0]
        .derivations
        .iter()
        .find(|derivation| !derivation.tuple_sources.is_empty())
        .expect("derived answer retains positional tuple lineage");
    assert_eq!(
        rule.tuple_sources.len(),
        1,
        "magic control tuples are unit/hidden"
    );
    assert_eq!(rule.tuple_sources[0].relation, GENERIC_TRIPLE_RELATION);
    assert_eq!(rule.tuple_sources[0].arguments[1], format!("<{P1}>"));
}
// ── Leading-bound recursive IDB (the n-ary soundness repro) ──────────────────

const EDGE: &str = "http://ex/edge";
const NAME: &str = "http://ex/name";
const SELF: &str = "http://ex/self";

/// An arity-3 `reach(?s, ?o, ?w)` IDB atom (a rule-head relation, so servable).
fn reach_atom(s: QTerm, o: QTerm, w: QTerm) -> QAtom {
    QAtom {
        pred: "reach".to_owned(),
        args: vec![s, o, w],
    }
}

/// The n-ary leading-bound recursive-IDB program: an arity-3 recursive `reach`
/// (transitive `<edge>` reachability in the `triple` encoding), with a goal rule
/// whose body LEADS with the recursive IDB atom `reach(<self>, ?o, ?w)` carrying a
/// bound (constant) first position — the exact shape that emits a bodyless demand
/// rule the n-ary semi-naive engine drops.
fn leading_bound_idb_program() -> QProgram {
    let v = |n: &str| QTerm::Var(n.to_owned());
    let triple = |s: QTerm, p: &str, o: QTerm, w: QTerm| QAtom {
        pred: "triple".to_owned(),
        args: vec![s, QTerm::Const(format!("<{p}>")), o, w],
    };
    QProgram {
        rules: vec![
            // base: reach(?s,?o,?w) :- triple(?s,<edge>,?o,?w).
            crate::query_ir::QRule {
                head: reach_atom(v("s"), v("o"), v("w")),
                body: vec![QBodyLit::Atom(triple(v("s"), EDGE, v("o"), v("w")))],
            },
            // recursive: reach(?s,?o,?w) :- reach(?s,?m,?w), triple(?m,<edge>,?o,?w).
            crate::query_ir::QRule {
                head: reach_atom(v("s"), v("o"), v("w")),
                body: vec![
                    QBodyLit::Atom(reach_atom(v("s"), v("m"), v("w"))),
                    QBodyLit::Atom(triple(v("m"), EDGE, v("o"), v("w"))),
                ],
            },
            // goal rule: answer(?p,?o,?w) :- reach(<self>,?o,?w), triple(?o,<name>,?p,?w).
            crate::query_ir::QRule {
                head: QAtom {
                    pred: "answer".to_owned(),
                    args: vec![v("p"), v("o"), v("w")],
                },
                body: vec![
                    QBodyLit::Atom(reach_atom(
                        QTerm::Const(format!("<{SELF}>")),
                        v("o"),
                        v("w"),
                    )),
                    QBodyLit::Atom(triple(v("o"), NAME, v("p"), v("w"))),
                ],
            },
        ],
        goal: QGoal {
            atoms: vec![QAtom {
                pred: "answer".to_owned(),
                args: vec![v("p"), v("o"), v("w")],
            }],
        },
        counterfactual: None,
        prob_facts: vec![],
        prob_model: None,
        confidences: vec![],
    }
}

#[test]
fn generic_leading_bound_recursive_idb_decides_full_answer_set() {
    // World: self →edge a →edge b; a and b carry a <name>. reach(self,·) = {a, b},
    // so the goal rule (leading with the bound recursive IDB atom) yields exactly the
    // two named reachable nodes. Pre-fix the leading `reach(<self>,…)` demand rule is
    // bodyless → dropped by the n-ary semi-naive engine → reach is never demanded →
    // empty answer with status Ok (the silent under-demand this task fixes).
    let (store, world) = make_world(&[
        (SELF, EDGE, "http://ex/a"),
        ("http://ex/a", EDGE, "http://ex/b"),
        ("http://ex/a", NAME, "http://ex/na"),
        ("http://ex/b", NAME, "http://ex/nb"),
    ]);
    let foreign = WorldFactSnapshot::from_world(&store, W, PROFILE).unwrap();
    let prog = leading_bound_idb_program();

    let answer =
        decided(resolve_native_generic(&foreign, &world, &prog, &Budget::default()).unwrap());
    assert_eq!(answer.status, BudgetStatus::Ok, "complete ⇒ Ok: {answer:?}");
    assert_eq!(
        answer.bindings.len(),
        2,
        "reach(self,·) = {{a, b}}, each named ⇒ 2 answers: {answer:?}"
    );
}

#[test]
fn generic_seeds_are_exactly_the_bodyless_rule_heads() {
    // Demand-completeness certificate (Beeri–Ramakrishnan), the n-ary mirror of
    // `magic::tests::magic_seeds_are_exactly_the_bodyless_rule_heads`: the materialized
    // seed set is EXACTLY the set of ground heads of the bodyless positive rules the
    // transform would emit. The program is the arity-2 analogue of the binary leading-
    // IDB repro (`reach(X,Y):-knows(X,Y)`, `reach(X,Y):-knows(X,Z),reach(Z,Y)`) behind a
    // wrapping goal-rule `c(P) :- reach(<self>,P)` whose body LEADS with the recursive
    // IDB atom bound-first. The top-level goal is `c(P)`, all-free, so it contributes no
    // goal seed of its own — the sole seed is minted when `c`'s rule body is walked and
    // the leading `reach(<self>,P)` atom is adorned bound-first (`bf`).
    let v = |n: &str| QTerm::Var(n.to_owned());
    let konst = |iri: &str| QTerm::Const(format!("<{iri}>"));
    let knows = |x: QTerm, y: QTerm| QAtom {
        pred: "knows".to_owned(),
        args: vec![x, y],
    };
    let reach = |x: QTerm, y: QTerm| QAtom {
        pred: "reach".to_owned(),
        args: vec![x, y],
    };

    let prog = QProgram {
        rules: vec![
            // base: reach(X,Y) :- knows(X,Y).
            crate::query_ir::QRule {
                head: reach(v("x"), v("y")),
                body: vec![QBodyLit::Atom(knows(v("x"), v("y")))],
            },
            // recursive: reach(X,Y) :- knows(X,Z), reach(Z,Y).
            crate::query_ir::QRule {
                head: reach(v("x"), v("y")),
                body: vec![
                    QBodyLit::Atom(knows(v("x"), v("z"))),
                    QBodyLit::Atom(reach(v("z"), v("y"))),
                ],
            },
            // goal rule, leading with the recursive IDB atom bound-first:
            // c(P) :- reach(<self>, P).
            crate::query_ir::QRule {
                head: QAtom {
                    pred: "c".to_owned(),
                    args: vec![v("p")],
                },
                body: vec![QBodyLit::Atom(reach(konst(SELF), v("p")))],
            },
        ],
        goal: QGoal {
            atoms: vec![QAtom {
                pred: "c".to_owned(),
                args: vec![v("p")],
            }],
        },
        counterfactual: None,
        prob_facts: vec![],
        prob_model: None,
        confidences: vec![],
    };

    let rules: Vec<GenericRule> = prog
        .rules
        .iter()
        .map(|r| {
            let head = generic_atom_of(&r.head).unwrap();
            let body: Vec<GenericAtom> = r
                .body
                .iter()
                .map(|l| match l {
                    QBodyLit::Atom(a) => generic_atom_of(a).unwrap(),
                    other => panic!("unexpected body literal {other:?}"),
                })
                .collect();
            let iri = format!("{}::rule", head.relation);
            generic_rule(head, body, iri)
        })
        .collect();
    let goal_atom = generic_atom_of(&prog.goal.atoms[0]).unwrap();
    let pattern = goal_pattern(&prog.goal.atoms[0]);
    let transformed = magic_transform_generic(&rules, &goal_atom, pattern).unwrap();

    // (a) Beeri–Ramakrishnan completeness: no surviving rule is bodyless (the invariant
    // `magic_transform_generic` itself asserts).
    assert!(
        transformed.rules.iter().all(|r| !r.body.is_empty()),
        "no transformed rule may be bodyless: {:?}",
        transformed.rules
    );

    // (b) Re-derive the expected demand INDEPENDENTLY of the transform. The sole leading
    // bound recursive-IDB atom is `reach(<self>, ?p)` inside `c`'s body, adorned
    // bound-first (subject bound, object free) — pattern `bf`. The generic magic guard
    // carries the REAL bound sub-tuple (arity = #bound positions; see the module-level
    // doc contrasting this with the binary store's self-loop pair hack), so for a
    // single bound position the expected seed args are the ARITY-1 tuple `[<self>]`, not
    // a 2-element self-loop pair. The relation is minted by the arity-agnostic
    // `magic_pred_iri` helper both backward legs share.
    let expected_relation = magic_pred_iri("reach", "bf");
    let expected_seed = (expected_relation, vec![TermValue::iri(SELF)]);
    assert_eq!(
        transformed.seeds.len(),
        1,
        "exactly one lifted demand seed: {:?}",
        transformed.seeds
    );
    assert_eq!(
        transformed.seeds[0], expected_seed,
        "the seed set must equal the bodyless-rule-head demand set {{magic_reach_bf(<self>)}}"
    );
}

#[test]
fn generic_ff_goal_ground_fact_rule_decides_the_fact() {
    // Site B: an n-ary ground fact-rule (empty body) `p(<a>,<b>,<c>).` under an
    // all-free goal `?- p(?x,?y,?z)`. Pre-fix the modified rule for `p` collapses to a
    // bodyless positive rule the engine never fires → the asserted fact is lost. The
    // fact-rule head is ground, so it must be materialized as a seed.
    let (store, world) = make_world(&[]);
    let foreign = WorldFactSnapshot::from_world(&store, W, PROFILE).unwrap();
    let c = |iri: &str| QTerm::Const(format!("<{iri}>"));
    let v = |n: &str| QTerm::Var(n.to_owned());
    let prog = QProgram {
        rules: vec![crate::query_ir::QRule {
            head: QAtom {
                pred: "p".to_owned(),
                args: vec![c("http://ex/a"), c("http://ex/b"), c("http://ex/c")],
            },
            body: vec![],
        }],
        goal: QGoal {
            atoms: vec![QAtom {
                pred: "p".to_owned(),
                args: vec![v("x"), v("y"), v("z")],
            }],
        },
        counterfactual: None,
        prob_facts: vec![],
        prob_model: None,
        confidences: vec![],
    };

    let answer =
        decided(resolve_native_generic(&foreign, &world, &prog, &Budget::default()).unwrap());
    assert_eq!(answer.status, BudgetStatus::Ok, "complete ⇒ Ok: {answer:?}");
    assert_eq!(
        answer.bindings.len(),
        1,
        "the asserted ground fact p(a,b,c) is the sole answer: {answer:?}"
    );
}
