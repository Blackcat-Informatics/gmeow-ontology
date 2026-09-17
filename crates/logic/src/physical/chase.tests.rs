// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use crate::provenance::LOGIC_NAMESPACE;
use purrdf::TermValue;

const W: &str = "https://blackcatinformatics.ca/gmeow/world/default";
const P: &str = "http://ex/p";
const TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
const C: &str = "http://ex/C";
const D: &str = "http://ex/D";

fn iri(s: &str) -> TermValue {
    TermValue::iri(s)
}

fn fact(s: &str, p: &str, o: &str) -> Fact {
    Fact {
        subject: iri(s),
        predicate: p.to_owned(),
        object: iri(o),
    }
}

fn var(name: &str) -> EvalTerm {
    EvalTerm::Var(name.to_owned())
}

fn atom(s: EvalTerm, p: &str, o: EvalTerm) -> EvalAtom {
    EvalAtom {
        subject: s,
        predicate: p.to_owned(),
        object: o,
        negated: false,
    }
}

/// `type(x, C) → ∃y. p(x, y) ∧ type(y, D)` — the EL `∃p.D` obligation as a TGD.
fn some_values_from_rule() -> ExistentialRule {
    ExistentialRule {
        numeric: Vec::new(),
        rule_iri: "http://ex/rule/svf".to_owned(),
        body: vec![atom(var("?x"), TYPE, EvalTerm::ConstNamed(C.to_owned()))],
        head: vec![
            atom(var("?x"), P, var("?y")),
            atom(var("?y"), TYPE, EvalTerm::ConstNamed(D.to_owned())),
        ],
        distinct: vec![],
        witness_frontier: None,
        witness_policy: WitnessPolicy::FrontierSkolem,
    }
}

fn decided(outcome: NativeOutcome<Budgeted<Vec<DerivedRow>>>) -> Budgeted<Vec<DerivedRow>> {
    match outcome {
        NativeOutcome::Decided(b) => b,
        NativeOutcome::Unsupported(k) => panic!("expected Decided, got Unsupported({k:?})"),
    }
}

/// Count derived rows for a predicate.
fn count(rows: &[DerivedRow], predicate: &str) -> usize {
    rows.iter().filter(|r| r.predicate == predicate).count()
}

#[test]
fn duplicate_heads_keep_the_first_standalone_proof_and_raw_budget() {
    let mut first = some_values_from_rule();
    first.rule_iri = "urn:z-first".into();
    let head = atom(var("?x"), P, EvalTerm::named("urn:result"));
    first.head = vec![head.clone(), head.clone()];
    let mut second = first.clone();
    second.rule_iri = "urn:a-second".into();
    second.body = vec![atom(var("?x"), TYPE, EvalTerm::named(D))];
    second.head = vec![head];
    let edb = [fact("urn:subject", TYPE, C), fact("urn:subject", TYPE, D)];
    for budget in [None, Some(1)] {
        let result =
            decided(chase_world(W, &edb, &[first.clone(), second.clone()], budget).unwrap());
        assert_eq!(
            result.status,
            if budget.is_none() {
                BudgetStatus::Ok
            } else {
                BudgetStatus::Exhausted
            }
        );
        assert_eq!(result.consumed_steps, 1);
        assert_eq!(count(&result.rows, P), 1);
        let row = result.rows.iter().find(|row| row.predicate == P).unwrap();
        assert_eq!(row.rule_iri, first.rule_iri);
        assert_eq!(row.antecedents.len(), 1);
        assert_eq!(row.antecedents[0].key(), edb[0].key());
        assert_eq!(row.source_quad_ids, [edb[0].reifier().unwrap()]);
        assert_eq!(
            row.derivation_id,
            mint_derivation_id(&first.rule_iri, &[&row.source_quad_ids[0]])
        );
    }
}

/// A well-formed reified head extracts its args in positional order regardless of the
/// head-atom authoring order.
#[test]
fn reified_nary_head_accepts_a_contiguous_shape() {
    let rel = "http://ex/rel/op";
    let a0 = format!("{LOGIC_NAMESPACE}naryArg0");
    let a1 = format!("{LOGIC_NAMESPACE}naryArg1");
    let rule = ExistentialRule {
        numeric: Vec::new(),
        rule_iri: "http://ex/rule/nary".to_owned(),
        body: vec![atom(var("?x"), P, var("?a")), atom(var("?x"), P, var("?b"))],
        // naryArg1 authored BEFORE naryArg0 — the extractor must sort to positional order.
        head: vec![
            atom(
                var("?r"),
                &instance_of_iri(),
                EvalTerm::ConstNamed(rel.to_owned()),
            ),
            atom(var("?r"), &a1, var("?b")),
            atom(var("?r"), &a0, var("?a")),
        ],
        distinct: vec![],
        witness_frontier: None,
        witness_policy: WitnessPolicy::FrontierSkolem,
    };
    let (reifier, got_rel, args) = reified_nary_head(&rule).unwrap().unwrap();
    assert_eq!(reifier, "?r");
    assert_eq!(got_rel, rel);
    assert_eq!(args, vec![var("?a"), var("?b")]);
}

/// A gapped positional index (naryArg0 + naryArg2, no naryArg1) is a HARD ERROR — the
/// ordered arg vector feeds `mint_nary_reifier`, so a gap would mint a wrong reifier.
#[test]
fn reified_nary_head_rejects_a_gapped_positional_index() {
    let rel = "http://ex/rel/op";
    let a0 = format!("{LOGIC_NAMESPACE}naryArg0");
    let a2 = format!("{LOGIC_NAMESPACE}naryArg2");
    let rule = ExistentialRule {
        numeric: Vec::new(),
        rule_iri: "http://ex/rule/nary".to_owned(),
        body: vec![atom(var("?x"), P, var("?a")), atom(var("?x"), P, var("?c"))],
        head: vec![
            atom(
                var("?r"),
                &instance_of_iri(),
                EvalTerm::ConstNamed(rel.to_owned()),
            ),
            atom(var("?r"), &a0, var("?a")),
            atom(var("?r"), &a2, var("?c")),
        ],
        distinct: vec![],
        witness_frontier: None,
        witness_policy: WitnessPolicy::FrontierSkolem,
    };
    let err = reified_nary_head(&rule).unwrap_err();
    assert!(
        err.message().contains("non-contiguous or duplicate"),
        "{err}"
    );
}

/// A duplicate positional index (two naryArg0) is a HARD ERROR for the same reason.
#[test]
fn reified_nary_head_rejects_a_duplicate_positional_index() {
    let rel = "http://ex/rel/op";
    let a0 = format!("{LOGIC_NAMESPACE}naryArg0");
    let rule = ExistentialRule {
        numeric: Vec::new(),
        rule_iri: "http://ex/rule/nary".to_owned(),
        body: vec![atom(var("?x"), P, var("?a")), atom(var("?x"), P, var("?b"))],
        head: vec![
            atom(
                var("?r"),
                &instance_of_iri(),
                EvalTerm::ConstNamed(rel.to_owned()),
            ),
            atom(var("?r"), &a0, var("?a")),
            atom(var("?r"), &a0, var("?b")),
        ],
        distinct: vec![],
        witness_frontier: None,
        witness_policy: WitnessPolicy::FrontierSkolem,
    };
    let err = reified_nary_head(&rule).unwrap_err();
    assert!(
        err.message().contains("non-contiguous or duplicate"),
        "{err}"
    );
}

#[test]
fn chase_invents_a_witness_for_some_values_from() {
    // Two individuals of type C ⇒ two distinct p-edges to two distinct D witnesses
    // (restricted chase = one fresh witness per frontier binding).
    let edb = vec![fact("http://ex/a", TYPE, C), fact("http://ex/b", TYPE, C)];
    let b = decided(chase_world(W, &edb, &[some_values_from_rule()], None).unwrap());
    assert_eq!(b.status, BudgetStatus::Ok);
    assert_eq!(count(&b.rows, P), 2, "one p-edge per C individual");
    assert_eq!(count(&b.rows, TYPE), 4, "2 asserted C + 2 invented D");
    // The two witnesses are distinct nulls.
    let objs: BTreeSet<_> = b
        .rows
        .iter()
        .filter(|r| r.predicate == P)
        .map(|r| term_display(&r.object))
        .collect();
    assert_eq!(objs.len(), 2);
}

#[test]
fn chase_restricted_satisfaction_skips_when_witness_exists() {
    // `a` already has a p-edge to `w` typed D ⇒ the obligation is satisfied and no
    // fresh witness is invented; `b` still gets one.
    let edb = vec![
        fact("http://ex/a", TYPE, C),
        fact("http://ex/a", P, "http://ex/w"),
        fact("http://ex/w", TYPE, D),
        fact("http://ex/b", TYPE, C),
    ];
    let b = decided(chase_world(W, &edb, &[some_values_from_rule()], None).unwrap());
    assert_eq!(
        count(&b.rows, P),
        2,
        "a's existing edge + b's invented edge"
    );
    // `a` invents nothing: its only p-edge is the pre-existing one to w.
    let a_targets: Vec<_> = b
        .rows
        .iter()
        .filter(|r| r.predicate == P && term_display(&r.subject) == "<http://ex/a>")
        .collect();
    assert_eq!(a_targets.len(), 1);
    assert_eq!(term_display(&a_targets[0].object), "<http://ex/w>");
}

#[test]
fn conjunctive_blocker_stops_at_a_witness_and_unfinished_search_never_invents() {
    let rule = ExistentialRule {
        numeric: Vec::new(),
        rule_iri: "http://ex/rule/six-distinct".into(),
        body: vec![atom(var("?x"), TYPE, EvalTerm::named(C))],
        head: (0..6)
            .map(|i| atom(var("?x"), P, var(&format!("?v{i}"))))
            .collect(),
        distinct: (0..6)
            .flat_map(|a| (a + 1..6).map(move |b| (format!("?v{a}"), format!("?v{b}"))))
            .collect(),
        witness_frontier: None,
        witness_policy: WitnessPolicy::FrontierSkolem,
    };
    let mut edb = vec![fact("http://ex/a", TYPE, C)];
    edb.extend((0..24).map(|i| fact("http://ex/a", P, &format!("http://ex/value/{i}"))));
    let (result, registry) =
        chase_world_explained(W, &edb, std::slice::from_ref(&rule), Some(32)).unwrap();
    let result = decided(result);
    assert_eq!(result.status, BudgetStatus::Ok);
    assert_eq!(result.rows.len(), edb.len());
    assert_eq!(result.consumed_steps, 0);
    assert_eq!(
        registry.len(),
        0,
        "an existing full blocker must prevent invention"
    );

    let (result, registry) = chase_world_explained(W, &edb, &[rule], Some(1)).unwrap();
    let result = decided(result);
    assert_eq!(result.status, BudgetStatus::Exhausted);
    assert_eq!(result.rows.len(), edb.len());
    assert_eq!(result.consumed_steps, 0);
    assert_eq!(
        registry.len(),
        0,
        "unfinished blocking cannot authorize invention"
    );
}

#[test]
fn chase_terminates_on_a_bounded_program() {
    // An acyclic EL restriction over three C individuals: the chase reaches its
    // natural fixpoint (status Ok) with a bounded, exact derived-row count.
    let edb = vec![
        fact("http://ex/a", TYPE, C),
        fact("http://ex/b", TYPE, C),
        fact("http://ex/c", TYPE, C),
    ];
    let b = decided(chase_world(W, &edb, &[some_values_from_rule()], None).unwrap());
    assert_eq!(b.status, BudgetStatus::Ok);
    // 3 echoed C + 3 invented p-edges + 3 invented D-types = 9 rows, and no more on
    // a second identical run (determinism).
    assert_eq!(b.rows.len(), 9);
    let again = decided(chase_world(W, &edb, &[some_values_from_rule()], None).unwrap());
    assert_eq!(b.rows.len(), again.rows.len());
    assert_eq!(b.consumed_steps, again.consumed_steps);
}

#[test]
fn chase_budget_exhaustion_is_incomplete_not_wrong() {
    // A cyclic `D ⊑ ∃p.D` would not terminate unbudgeted; with a step budget the
    // chase stops early, reporting Exhausted with a sound committed prefix.
    let cyclic = ExistentialRule {
        numeric: Vec::new(),
        rule_iri: "http://ex/rule/cyclic".to_owned(),
        body: vec![atom(var("?x"), TYPE, EvalTerm::ConstNamed(D.to_owned()))],
        head: vec![
            atom(var("?x"), P, var("?y")),
            atom(var("?y"), TYPE, EvalTerm::ConstNamed(D.to_owned())),
        ],
        distinct: vec![],
        witness_frontier: None,
        witness_policy: WitnessPolicy::FrontierSkolem,
    };
    let edb = vec![fact("http://ex/a", TYPE, D)];
    let b = decided(chase_world(W, &edb, &[cyclic], Some(3)).unwrap());
    assert_eq!(b.status, BudgetStatus::Exhausted);
    assert_eq!(
        b.consumed_steps, 3,
        "exactly the budget of committed derivations"
    );
}

#[test]
fn chase_at_least_two_requires_two_distinct_witnesses() {
    // `≥2 p.D`: a single existing typed p-edge does NOT satisfy the obligation; the
    // chase must invent a second, distinct witness.
    let ge2 = ExistentialRule {
        numeric: Vec::new(),
        rule_iri: "http://ex/rule/ge2".to_owned(),
        body: vec![atom(var("?x"), TYPE, EvalTerm::ConstNamed(C.to_owned()))],
        head: vec![
            atom(var("?x"), P, var("?y1")),
            atom(var("?y1"), TYPE, EvalTerm::ConstNamed(D.to_owned())),
            atom(var("?x"), P, var("?y2")),
            atom(var("?y2"), TYPE, EvalTerm::ConstNamed(D.to_owned())),
        ],
        distinct: vec![("?y1".to_owned(), "?y2".to_owned())],
        witness_frontier: None,
        witness_policy: WitnessPolicy::FrontierSkolem,
    };
    // `a` has ONE existing typed witness — short of the two required.
    let edb = vec![
        fact("http://ex/a", TYPE, C),
        fact("http://ex/a", P, "http://ex/w"),
        fact("http://ex/w", TYPE, D),
    ];
    let b = decided(chase_world(W, &edb, &[ge2], None).unwrap());
    // a must end with ≥2 distinct D-typed p-targets.
    let targets: BTreeSet<_> = b
        .rows
        .iter()
        .filter(|r| r.predicate == P && term_display(&r.subject) == "<http://ex/a>")
        .map(|r| term_display(&r.object))
        .collect();
    assert!(
        targets.len() >= 2,
        "≥2 distinct witnesses, got {}",
        targets.len()
    );
}

#[test]
fn chase_materialize_echoes_later_worlds_asserted_facts_after_budget_exhaustion() {
    // A step budget governs DERIVED steps, not input. When it is spent in an earlier
    // world, later worlds' ASSERTED (EDB) facts must still be echoed — never dropped
    // with the derivations.
    let w1 = "http://ex/world/1";
    let w2 = "http://ex/world/2";
    let store = crate::store::WorldStore::new();
    // Two obligations in world 1 exhaust a 1-step budget before world 2 is reached.
    store.insert_quad(w1, "http://ex/a1", TYPE, C);
    store.insert_quad(w1, "http://ex/a2", TYPE, C);
    store.insert_quad(w2, "http://ex/b", TYPE, C);
    let (_admission, outcome) =
        chase_materialize(&store, &[some_values_from_rule()], Some(1)).unwrap();
    let b = decided(outcome);
    assert_eq!(
        b.status,
        BudgetStatus::Exhausted,
        "the 1-step budget must exhaust before world 2"
    );
    assert!(
        b.rows.iter().any(|r| r.graph == w2
            && r.predicate == TYPE
            && term_display(&r.subject) == "<http://ex/b>"),
        "world 2's asserted EDB must survive world 1's budget exhaustion; rows: {:#?}",
        b.rows
    );
}

// ── ChaseAdmission termination certificate ───────────────────────────────────

const E: &str = "http://ex/E";
const Q: &str = "http://ex/q";

/// `type(x, from) → ∃y. rel(x, y) ∧ type(y, to)`.
fn restriction_rule(iri: &str, from: &str, rel: &str, to: &str) -> ExistentialRule {
    ExistentialRule {
        numeric: Vec::new(),
        rule_iri: iri.to_owned(),
        body: vec![atom(var("?x"), TYPE, EvalTerm::ConstNamed(from.to_owned()))],
        head: vec![
            atom(var("?x"), rel, var("?y")),
            atom(var("?y"), TYPE, EvalTerm::ConstNamed(to.to_owned())),
        ],
        distinct: vec![],
        witness_frontier: None,
        witness_policy: WitnessPolicy::FrontierSkolem,
    }
}

#[test]
fn certify_acyclic_el_restriction_is_weakly_acyclic_and_non_vacuous() {
    // `C ⊑ ∃p.D` terminates (the D-witness never re-triggers the C-bodied rule).
    // The certifier must (a) certify it AND (b) actually SEE an existential edge —
    // the load-bearing non-vacuity check: if the ∃ head var were invisible the
    // certifier would trivially (vacuously) certify with ZERO special edges.
    let admission = ChaseAdmission::certify(&[some_values_from_rule()]);
    match &admission {
        ChaseAdmission::WeaklyAcyclic { evidence } => {
            assert!(admission.admits_native());
            assert!(
                !evidence.contains("0 existential edge"),
                "certifier must see ≥1 existential edge (non-vacuous): {evidence}"
            );
        }
        other => {
            panic!("acyclic C⊑∃p.D must certify as weakly-acyclic, got: {other:?}")
        }
    }
}

#[test]
fn certify_cyclic_restriction_is_uncertified() {
    // `D ⊑ ∃p.D`: the witness is itself D-typed, re-triggering the rule forever.
    // No rung of the ladder may certify it — it must fall through to Uncertified.
    let cyclic = restriction_rule("http://ex/rule/cyclic", D, P, D);
    let admission = ChaseAdmission::certify(&[cyclic]);
    match admission {
        ChaseAdmission::Uncertified { violations } => {
            assert!(!violations.is_empty());
            assert!(violations[0].contains("lies in a cycle"));
        }
        certified => {
            panic!("cyclic D⊑∃p.D must NOT certify by any class, got: {certified:?}")
        }
    }
}

#[test]
fn certify_acyclic_chain_certifies() {
    // `C ⊑ ∃p.D` and `D ⊑ ∃q.E`: a finite chain C→D→E, terminating.
    let r1 = restriction_rule("http://ex/rule/c", C, P, D);
    let r2 = restriction_rule("http://ex/rule/d", D, Q, E);
    assert!(ChaseAdmission::certify(&[r1, r2]).admits_native());
}

#[test]
fn certify_two_rule_cycle_is_uncertified() {
    // `C ⊑ ∃p.D` and `D ⊑ ∃q.C`: C→D→C invents forever across two rules.
    let r1 = restriction_rule("http://ex/rule/c", C, P, D);
    let r2 = restriction_rule("http://ex/rule/d", D, Q, C);
    assert!(!ChaseAdmission::certify(&[r1, r2]).admits_native());
}

// ── Joint acyclicity (strictly broader than weak) ────────────────────────────

/// `type(x,C) ∧ type(x,D) → ∃y. p(x,y)` and `p(x,y) → type(y,C)`.
///
/// **Jointly acyclic but NOT weakly acyclic.** Weak acyclicity sees the position
/// cycle `(type,S,C) → (p,O,*) → (type,S,C)` (the p-object null flows to `type,C`,
/// which is a body position of the first rule) and refuses.  Joint acyclicity tracks
/// that the null becomes `C` but never `D`, so it can never re-bind the `C∧D`-guarded
/// frontier `x` of the first rule — no existential depends on itself.  The chase
/// terminates: the C-only witness does not satisfy the `C∧D` guard, so no further
/// invention fires.
fn jointly_acyclic_not_weakly_acyclic() -> Vec<ExistentialRule> {
    let guarded = ExistentialRule {
        numeric: Vec::new(),
        rule_iri: "http://ex/rule/ja-guard".to_owned(),
        body: vec![
            atom(var("?x"), TYPE, EvalTerm::ConstNamed(C.to_owned())),
            atom(var("?x"), TYPE, EvalTerm::ConstNamed(D.to_owned())),
        ],
        head: vec![atom(var("?x"), P, var("?y"))],
        distinct: vec![],
        witness_frontier: None,
        witness_policy: WitnessPolicy::FrontierSkolem,
    };
    let feedback = ExistentialRule {
        numeric: Vec::new(),
        rule_iri: "http://ex/rule/ja-feedback".to_owned(),
        body: vec![atom(var("?x"), P, var("?y"))],
        head: vec![atom(var("?y"), TYPE, EvalTerm::ConstNamed(C.to_owned()))],
        distinct: vec![],
        witness_frontier: None,
        witness_policy: WitnessPolicy::FrontierSkolem,
    };
    vec![guarded, feedback]
}

#[test]
fn certify_jointly_acyclic_non_vacuous_beyond_weak() {
    // The rung is a REAL increment: weak acyclicity refuses this program, joint
    // acyclicity certifies it.
    let prog = jointly_acyclic_not_weakly_acyclic();
    assert!(
        ChaseAdmission::certify_weakly_acyclic(&prog).is_err(),
        "weak acyclicity must REFUSE the guard-split program (position-cycle)"
    );
    match ChaseAdmission::certify(&prog) {
        ChaseAdmission::JointlyAcyclic { .. } => {}
        other => panic!("ladder must certify as JointlyAcyclic, got {other:?}"),
    }
    assert!(ChaseAdmission::certify(&prog).admits_native());
}

#[test]
fn certify_jointly_acyclic_evidence_is_non_vacuous() {
    // The certifier actually SAW the existential (≥1 existential variable): a vacuous
    // certificate would report zero and is a bug.
    match ChaseAdmission::certify(&jointly_acyclic_not_weakly_acyclic()) {
        ChaseAdmission::JointlyAcyclic { evidence } => assert!(
            !evidence.contains("0 existential variable"),
            "joint-acyclicity certificate must be non-vacuous (saw ≥1 ∃): {evidence}"
        ),
        other => panic!("expected JointlyAcyclic, got {other:?}"),
    }
}

#[test]
fn jointly_acyclic_program_runs_natively_unbudgeted_on_route_chase() {
    // The production-surface proof: a program weak acyclicity REFUSES today is now
    // admitted AND runs to a natural fixpoint UNBUDGETED on the real router.  A false
    // certification of a non-terminating program would loop/exhaust here.
    let prog = jointly_acyclic_not_weakly_acyclic();
    let edb = vec![fact("http://ex/a", TYPE, C), fact("http://ex/a", TYPE, D)];
    let (admission, outcome) = route_chase(W, &edb, &prog, None).unwrap();
    assert!(
        matches!(admission, ChaseAdmission::JointlyAcyclic { .. }),
        "route_chase must admit the program as jointly-acyclic, got {admission:?}"
    );
    // Runs to a fixpoint with NO budget (Decided, not Unsupported/Exhausted).
    let _ = decided(outcome);
    // Previously-refused leg: the WA-only certifier refuses P, so pre-change
    // route_chase(P, None) would have been Unsupported(NonTerminatingExistential).
    assert!(
        ChaseAdmission::certify_weakly_acyclic(&prog).is_err(),
        "the same program is refused by weak acyclicity alone (previously refused)"
    );
}

#[test]
fn certify_cyclic_defeats_joint_acyclicity() {
    // A genuine two-rule invention cycle (`C ⊑ ∃p.D`, `D ⊑ ∃q.C`) is non-terminating:
    // joint acyclicity must NOT certify it, and the ladder falls through to refusal.
    let cyclic = vec![
        restriction_rule("http://ex/rule/c", C, P, D),
        restriction_rule("http://ex/rule/d", D, Q, C),
    ];
    assert!(
        ChaseAdmission::certify_joint_acyclic(&cyclic).is_none(),
        "joint acyclicity must refuse a genuine invention cycle"
    );
    assert!(!ChaseAdmission::certify(&cyclic).admits_native());
}

#[test]
fn reduced_witness_frontier_does_not_erase_copied_value_feedback() {
    let mut rules = jointly_acyclic_not_weakly_acyclic();
    // The original witness only gets class C. This producer also gives it D,
    // completing the guard that re-invents; its own witness is rule-scoped.
    rules.push(ExistentialRule {
        numeric: Vec::new(),
        rule_iri: "urn:shared-witness-relay".to_owned(),
        body: vec![atom(var("?x"), P, var("?y"))],
        head: vec![
            atom(var("?y"), TYPE, EvalTerm::named(D)),
            atom(var("?y"), Q, var("?shared")),
        ],
        distinct: Vec::new(),
        witness_frontier: Some(Vec::new()),
        witness_policy: WitnessPolicy::FrontierSkolem,
    });
    assert!(ChaseAdmission::certify_weakly_acyclic(&rules).is_err());
    assert!(ChaseAdmission::certify_joint_acyclic(&rules).is_none());
    assert!(!ChaseAdmission::certify(&rules).admits_native());
    let input = [fact("http://ex/a", TYPE, C), fact("http://ex/a", TYPE, D)];
    let (_, outcome) = route_chase(W, &input, &rules, Some(16)).unwrap();
    let NativeOutcome::Decided(result) = outcome else {
        panic!("an explicit budget admits the incomplete run");
    };
    assert_eq!(result.status, BudgetStatus::Exhausted);
}

// ── Super-weak acyclicity (Skolem place graph, incomparable sibling of JA) ────

/// `type(x,C) → ∃y. p(x,y)` and `p(x,x) → type(x,C)`.
///
/// **Super-weakly acyclic but NOT weakly acyclic.** Weak acyclicity sees the position
/// cycle `(type,S,C) → (p,O,*) → (type,S,C)` (the p-object null flows to `type(·,C)`,
/// a body position of the invention rule) and refuses.  Super-weak acyclicity refuses
/// that flow: the null minted at `p(x, f(x))` cannot unify into the **diagonal** body
/// atom `p(x, x)` because the occurs-check `f(x) = x` fails, so no fact ever satisfies
/// the diagonal rule on the null.  The chase terminates: `p(a, f)` is never a diagonal,
/// so the second rule never re-types a witness.
fn super_weakly_acyclic_diagonal() -> Vec<ExistentialRule> {
    let invent = ExistentialRule {
        numeric: Vec::new(),
        rule_iri: "http://ex/rule/swa-invent".to_owned(),
        body: vec![atom(var("?x"), TYPE, EvalTerm::ConstNamed(C.to_owned()))],
        head: vec![atom(var("?x"), P, var("?y"))],
        distinct: vec![],
        witness_frontier: None,
        witness_policy: WitnessPolicy::FrontierSkolem,
    };
    let diagonal = ExistentialRule {
        numeric: Vec::new(),
        rule_iri: "http://ex/rule/swa-diagonal".to_owned(),
        body: vec![atom(var("?x"), P, var("?x"))],
        head: vec![atom(var("?x"), TYPE, EvalTerm::ConstNamed(C.to_owned()))],
        distinct: vec![],
        witness_frontier: None,
        witness_policy: WitnessPolicy::FrontierSkolem,
    };
    vec![invent, diagonal]
}

#[test]
fn certify_super_weakly_acyclic_non_vacuous_beyond_weak() {
    // Real increment over weak acyclicity (the issue's non-vacuity bar): WA refuses
    // the diagonal program, SWA certifies it via the occurs-check on `f(x) = x`.
    let prog = super_weakly_acyclic_diagonal();
    assert!(
        ChaseAdmission::certify_weakly_acyclic(&prog).is_err(),
        "weak acyclicity must REFUSE the diagonal program (position-cycle)"
    );
    match ChaseAdmission::certify_super_weak_acyclic(&prog) {
        Some(ChaseAdmission::SuperWeaklyAcyclic { .. }) => {}
        other => {
            panic!("super-weak acyclicity must certify the diagonal program, got {other:?}")
        }
    }
}

/// `type(x,C) → ∃y. p(x,y) ∧ p(y,x)` and `p(x,x) → type(x,C)`.
///
/// **Reported by the ladder as SuperWeaklyAcyclic** (WA and JA both refuse it, SWA
/// certifies it): the null is placed DIRECTLY at both `p` slots (`p(x,f)` and
/// `p(f,x)`) — no datalog laundering — so the occurs-check blocks both head atoms from
/// unifying with the diagonal body `p(x,x)`, breaking the cycle WA's position graph and
/// JA's existential-dependency graph both report.  Terminating: `p(a,f)`/`p(f,a)` are
/// never the diagonal, so the second rule never re-types a witness.
fn super_weakly_acyclic_symmetric_head() -> Vec<ExistentialRule> {
    let invent = ExistentialRule {
        numeric: Vec::new(),
        rule_iri: "http://ex/rule/swa-sym".to_owned(),
        body: vec![atom(var("?x"), TYPE, EvalTerm::ConstNamed(C.to_owned()))],
        head: vec![atom(var("?x"), P, var("?y")), atom(var("?y"), P, var("?x"))],
        distinct: vec![],
        witness_frontier: None,
        witness_policy: WitnessPolicy::FrontierSkolem,
    };
    let diagonal = ExistentialRule {
        numeric: Vec::new(),
        rule_iri: "http://ex/rule/swa-sym-diagonal".to_owned(),
        body: vec![atom(var("?x"), P, var("?x"))],
        head: vec![atom(var("?x"), TYPE, EvalTerm::ConstNamed(C.to_owned()))],
        distinct: vec![],
        witness_frontier: None,
        witness_policy: WitnessPolicy::FrontierSkolem,
    };
    vec![invent, diagonal]
}

#[test]
fn certify_super_weakly_acyclic_is_reported_by_the_ladder() {
    // WA and JA both refuse, SWA certifies — so the escalation ladder REPORTS
    // SuperWeaklyAcyclic (the rung is reachable, not merely a sound standalone check).
    let prog = super_weakly_acyclic_symmetric_head();
    assert!(
        ChaseAdmission::certify_weakly_acyclic(&prog).is_err(),
        "WA must refuse"
    );
    assert!(
        ChaseAdmission::certify_joint_acyclic(&prog).is_none(),
        "JA must refuse the symmetric-head program"
    );
    match ChaseAdmission::certify(&prog) {
        ChaseAdmission::SuperWeaklyAcyclic { .. } => {}
        other => panic!("ladder must report SuperWeaklyAcyclic, got {other:?}"),
    }
}

#[test]
fn certify_super_weak_evidence_is_non_vacuous() {
    // The certifier actually saw ≥1 invented null (existential output place).
    match ChaseAdmission::certify_super_weak_acyclic(&super_weakly_acyclic_diagonal()) {
        Some(ChaseAdmission::SuperWeaklyAcyclic { evidence }) => assert!(
            !evidence.contains("0 existential output place"),
            "super-weak-acyclicity certificate must be non-vacuous: {evidence}"
        ),
        other => panic!("expected SuperWeaklyAcyclic, got {other:?}"),
    }
}

#[test]
fn super_weakly_acyclic_program_runs_natively_unbudgeted_on_route_chase() {
    // Production-surface proof: a program weak acyclicity refuses runs to a natural
    // fixpoint UNBUDGETED on the real router.  (The ladder reports this particular
    // program as JointlyAcyclic — JA also accepts it and runs first — so we assert
    // `admits_native`, and separately pin the SWA certifier's beyond-WA property.)
    let prog = super_weakly_acyclic_diagonal();
    let edb = vec![fact("http://ex/a", TYPE, C)];
    let (admission, outcome) = route_chase(W, &edb, &prog, None).unwrap();
    assert!(
        admission.admits_native(),
        "route_chase must admit the SWA-certified program natively, got {admission:?}"
    );
    let _ = decided(outcome);
    assert!(
        ChaseAdmission::certify_super_weak_acyclic(&prog).is_some(),
        "the super-weak certifier certifies the program directly"
    );
    assert!(
        ChaseAdmission::certify_weakly_acyclic(&prog).is_err(),
        "the same program is refused by weak acyclicity alone (previously refused)"
    );
}

#[test]
fn certify_cyclic_defeats_super_weak_acyclicity() {
    // Genuine invention cycles: the null unifies back into its own rule's body (no
    // occurs-check block), so super-weak acyclicity refuses both.
    let self_cycle = vec![restriction_rule("http://ex/rule/cyclic", D, P, D)];
    assert!(
        ChaseAdmission::certify_super_weak_acyclic(&self_cycle).is_none(),
        "super-weak acyclicity must refuse the self-cycle D ⊑ ∃p.D"
    );
    let two_rule = vec![
        restriction_rule("http://ex/rule/c", C, P, D),
        restriction_rule("http://ex/rule/d", D, Q, C),
    ];
    assert!(
        ChaseAdmission::certify_super_weak_acyclic(&two_rule).is_none(),
        "super-weak acyclicity must refuse the two-rule invention cycle"
    );
    assert!(!ChaseAdmission::certify(&two_rule).admits_native());
}

// ── Model-summarizing acyclicity (self-hosted, the engine's own fixpoint) ─────

/// `p(x,x) → ∃y. p(x,y)` and `p(x,y) → p(y,x)`.
///
/// Terminating, but **every structural class refuses it**: weak acyclicity sees a
/// self special edge, joint acyclicity sees a self existential-dependency (the null's
/// positions cover the diagonal frontier), and super-weak acyclicity's cross-rule
/// unification is defeated by the swap rule (`p(y,x)` unifies with the diagonal
/// `p(x,x)` at the variable level).  Model-summarizing acyclicity certifies it: run on
/// the critical instance, the summarizing null `p(*, n)` never forms the diagonal
/// `p(n, n)`, so no `dep(n, n)` is derived — the engine's own fixpoint proves its own
/// termination.
fn model_summarizing_beyond_structural() -> Vec<ExistentialRule> {
    let invent = ExistentialRule {
        numeric: Vec::new(),
        rule_iri: "http://ex/rule/msa-invent".to_owned(),
        body: vec![atom(var("?x"), P, var("?x"))],
        head: vec![atom(var("?x"), P, var("?y"))],
        distinct: vec![],
        witness_frontier: None,
        witness_policy: WitnessPolicy::FrontierSkolem,
    };
    let swap = ExistentialRule {
        numeric: Vec::new(),
        rule_iri: "http://ex/rule/msa-swap".to_owned(),
        body: vec![atom(var("?x"), P, var("?y"))],
        head: vec![atom(var("?y"), P, var("?x"))],
        distinct: vec![],
        witness_frontier: None,
        witness_policy: WitnessPolicy::FrontierSkolem,
    };
    vec![invent, swap]
}

#[test]
fn certify_model_summarizing_non_vacuous_beyond_structural() {
    // Real increment: WA, JA, and SWA all refuse, MSA certifies — and the ladder
    // reports it as ModelSummarizingAcyclic (all cheaper rungs fell through).
    let prog = model_summarizing_beyond_structural();
    assert!(
        ChaseAdmission::certify_weakly_acyclic(&prog).is_err(),
        "weak acyclicity must refuse the swap-diagonal program"
    );
    assert!(
        ChaseAdmission::certify_joint_acyclic(&prog).is_none(),
        "joint acyclicity must refuse the swap-diagonal program"
    );
    assert!(
        ChaseAdmission::certify_super_weak_acyclic(&prog).is_none(),
        "super-weak acyclicity must refuse the swap-diagonal program"
    );
    match ChaseAdmission::certify_model_summarizing(&prog) {
        Some(ChaseAdmission::ModelSummarizingAcyclic { .. }) => {}
        other => panic!("MSA must certify the swap-diagonal program, got {other:?}"),
    }
    match ChaseAdmission::certify(&prog) {
        ChaseAdmission::ModelSummarizingAcyclic { .. } => {}
        other => panic!("the ladder must report ModelSummarizingAcyclic, got {other:?}"),
    }
}

#[test]
fn certify_msa_runs_the_engine_fixpoint() {
    // The self-hosting actually executed the engine's own fixpoint over a non-empty
    // critical instance (not a syntactic shortcut).
    match ChaseAdmission::certify_model_summarizing(&model_summarizing_beyond_structural()) {
        Some(ChaseAdmission::ModelSummarizingAcyclic { evidence }) => assert!(
            !evidence.contains("0 critical-instance fact"),
            "MSA must run the engine fixpoint over a non-empty critical instance: {evidence}"
        ),
        other => panic!("expected ModelSummarizingAcyclic, got {other:?}"),
    }
}

#[test]
fn certify_msa_refuses_oversized_critical_instance_without_materializing() {
    // A disjoint union of independent acyclic invent rules `p(cI,cI) → ∃y. q(cI,y)`:
    // no null ever reaches itself, so MSA holds — UNTIL the constant domain grows past
    // the critical-instance cap. The cap must flip the verdict to `None` (conservative
    // refuse → the ladder falls through to `Uncertified`) purely on projected size,
    // WITHOUT materializing `predicates × domain²` facts (no OOM / no hang).
    let build = |n: usize| -> Vec<ExistentialRule> {
        (0..n)
            .map(|i| {
                let c = EvalTerm::ConstNamed(format!("http://ex/c/{i}"));
                ExistentialRule {
                    numeric: Vec::new(),
                    rule_iri: format!("http://ex/rule/invent/{i}"),
                    body: vec![atom(c.clone(), P, c.clone())],
                    head: vec![atom(c, Q, var("?y"))],
                    distinct: vec![],
                    witness_frontier: None,
                    witness_policy: WitnessPolicy::FrontierSkolem,
                }
            })
            .collect()
    };
    // Small domain: MSA certifies (the fixpoint actually runs, no self-dependency).
    match ChaseAdmission::certify_model_summarizing(&build(3)) {
        Some(ChaseAdmission::ModelSummarizingAcyclic { .. }) => {}
        other => panic!("a small acyclic invent program must certify as MSA, got {other:?}"),
    }
    // Oversized domain (> 1024 constants ⇒ predicates × domain² > 1 << 20): the cap
    // refuses before materialization. This returns near-instantly; a pre-cap build
    // would allocate millions of facts.
    assert!(
        ChaseAdmission::certify_model_summarizing(&build(1100)).is_none(),
        "an oversized critical instance must conservatively refuse (None), not OOM/hang"
    );
}

#[test]
fn model_summarizing_program_runs_natively_unbudgeted_on_route_chase() {
    // Production-surface proof: a program every structural class refuses is admitted
    // by the MSA rung and runs to a natural fixpoint UNBUDGETED on the real router.
    // The diagonal `p(a,a)` fires the invent rule so a null is genuinely invented (an
    // `p(a,b)` seed would terminate without ever exercising existential invention).
    let prog = model_summarizing_beyond_structural();
    let edb = vec![fact("http://ex/a", P, "http://ex/a")];
    let (admission, outcome) = route_chase(W, &edb, &prog, None).unwrap();
    assert!(
        matches!(admission, ChaseAdmission::ModelSummarizingAcyclic { .. }),
        "route_chase must admit the program as model-summarizing-acyclic, got {admission:?}"
    );
    let _ = decided(outcome);
}

#[test]
fn certify_cyclic_defeats_msa() {
    // A genuine self-cycle `D ⊑ ∃p.D`: on the critical instance the summarizing null
    // is re-typed D and re-triggers its own rule, so `dep(n, n)` is derived → MSA
    // refuses, and the ladder falls through to Uncertified.
    let cyclic = vec![restriction_rule("http://ex/rule/cyclic", D, P, D)];
    assert!(
        ChaseAdmission::certify_model_summarizing(&cyclic).is_none(),
        "MSA must refuse the self-cycle D ⊑ ∃p.D"
    );
    assert!(!ChaseAdmission::certify(&cyclic).admits_native());
}

#[test]
fn sparse_obstruction_absence_cannot_authorize_msa() {
    // Sparse seeds p(*,*), q(*,c), r(*,a) derive p(*,n), r(n,*) but
    // never r(n,a). They have no null cycle. The FULL critical instance
    // also contains p(a,a): it derives r(n,a), then q(n,c), then lets
    // n occupy the frontier and re-mint itself. Sparse success must not
    // bypass that full-model rejection.
    let q = "http://ex/q";
    let r = "http://ex/r";
    let a = EvalTerm::ConstNamed("http://ex/a".to_owned());
    let c = EvalTerm::ConstNamed("http://ex/c".to_owned());
    let invent = ExistentialRule {
        numeric: Vec::new(),
        rule_iri: "http://ex/rule/probe-invent".to_owned(),
        body: vec![atom(var("?x"), P, var("?y")), atom(var("?y"), q, c.clone())],
        head: vec![atom(var("?y"), P, var("?z")), atom(var("?z"), r, var("?x"))],
        distinct: vec![],
        witness_frontier: None,
        witness_policy: WitnessPolicy::FrontierSkolem,
    };
    let feedback = ExistentialRule {
        numeric: Vec::new(),
        rule_iri: "http://ex/rule/probe-feedback".to_owned(),
        body: vec![atom(var("?x"), r, a)],
        head: vec![atom(var("?x"), q, c)],
        distinct: vec![],
        witness_frontier: None,
        witness_policy: WitnessPolicy::FrontierSkolem,
    };
    assert!(ChaseAdmission::certify_model_summarizing(&[invent, feedback]).is_none());
}

// ── Nemo-free soundness self-oracle: every certified program terminates ────────

#[test]
fn certifier_soundness_differential_reaches_fixpoint() {
    // The soundness differential replacing the retired Nemo oracle: for every program
    // the ladder ADMITS (spanning all four classes), (b) the production router runs it
    // natively unbudgeted, and (c) the budgeted native chase reaches a NATURAL
    // fixpoint — a false certification of a non-terminating program would exhaust the
    // budget instead. Self-hosted, deterministic, on-gate.
    const BIG: u64 = 1_000;
    let certified: Vec<(&str, Vec<ExistentialRule>, Vec<Fact>)> = vec![
        (
            "weakly-acyclic",
            vec![some_values_from_rule()],
            vec![fact("http://ex/a", TYPE, C)],
        ),
        (
            "jointly-acyclic",
            jointly_acyclic_not_weakly_acyclic(),
            vec![fact("http://ex/a", TYPE, C), fact("http://ex/a", TYPE, D)],
        ),
        (
            // The genuinely-SWA-classified witness: `certify` reports
            // `super_weakly_acyclic_diagonal()` as JointlyAcyclic (JA accepts and runs
            // first), so the SWA row must use the symmetric-head fixture the ladder
            // actually reports as SuperWeaklyAcyclic, or this slot only re-tests JA.
            "super-weakly-acyclic",
            super_weakly_acyclic_symmetric_head(),
            vec![fact("http://ex/a", TYPE, C)],
        ),
        (
            // EDB seeds the diagonal `p(a,a)` so the invent rule `p(x,x) → ∃y. p(x,y)`
            // actually FIRES — with `p(a,b)` invention never triggers and the
            // fixpoint-soundness probe is vacuous (a false MSA certification could not
            // exhaust the budget if no null is ever invented).
            "model-summarizing-acyclic",
            model_summarizing_beyond_structural(),
            vec![fact("http://ex/a", P, "http://ex/a")],
        ),
    ];
    for (label, prog, edb) in &certified {
        let admission = ChaseAdmission::certify(prog);
        assert!(
            admission.admits_native(),
            "{label}: a certified program must admit natively, got {admission:?}"
        );
        // Run the BUDGETED oracle FIRST: a false certification of a non-terminating
        // program exhausts the budget and fails this assertion, rather than hanging the
        // unbudgeted route below forever (which never returns to fail the test).
        let budgeted = decided(chase_world(W, edb, prog, Some(BIG)).unwrap());
        assert_eq!(
            budgeted.status,
            BudgetStatus::Ok,
            "{label}: a certified program must reach a NATURAL fixpoint (a false \
                 certification would exhaust the budget)"
        );
        // Only once the budgeted oracle has proven termination do we exercise the
        // production router unbudgeted — the executable proof that it runs natively.
        let (_, unbudgeted) = route_chase(W, edb, prog, None).unwrap();
        let _ = decided(unbudgeted);
    }
}

#[test]
fn certifier_refuses_non_terminating_programs() {
    // The sound fallback: genuinely non-terminating programs stay Uncertified, and the
    // unbudgeted router refuses them (Unsupported) rather than looping.
    let refused: Vec<(&str, Vec<ExistentialRule>, Vec<Fact>)> = vec![
        (
            "self-cycle",
            vec![restriction_rule("http://ex/rule/cyclic", D, P, D)],
            vec![fact("http://ex/a", TYPE, D)],
        ),
        (
            "two-rule-cycle",
            vec![
                restriction_rule("http://ex/rule/c", C, P, D),
                restriction_rule("http://ex/rule/d", D, Q, C),
            ],
            vec![fact("http://ex/a", TYPE, C)],
        ),
    ];
    for (label, prog, edb) in &refused {
        assert!(
            !ChaseAdmission::certify(prog).admits_native(),
            "{label}: a non-terminating program must stay Uncertified"
        );
        let (_, outcome) = route_chase(W, edb, prog, None).unwrap();
        assert!(
            matches!(
                outcome,
                NativeOutcome::Unsupported(UnsupportedKind::NonTerminatingExistential)
            ),
            "{label}: the unbudgeted router must refuse rather than loop"
        );
    }
}

#[test]
fn certify_lattice_ranks_are_strictly_ordered() {
    // The explicit escalation order (never a derived `Ord`): the ranks strictly
    // increase Uncertified < WA < JA < SWA < MSA.
    let ev = |s: &str| s.to_owned();
    let ranks = [
        ChaseAdmission::Uncertified { violations: vec![] }.rank(),
        ChaseAdmission::WeaklyAcyclic { evidence: ev("wa") }.rank(),
        ChaseAdmission::JointlyAcyclic { evidence: ev("ja") }.rank(),
        ChaseAdmission::SuperWeaklyAcyclic {
            evidence: ev("swa"),
        }
        .rank(),
        ChaseAdmission::ModelSummarizingAcyclic {
            evidence: ev("msa"),
        }
        .rank(),
    ];
    for w in ranks.windows(2) {
        assert!(
            w[0] < w[1],
            "certificate ranks must strictly increase: {ranks:?}"
        );
    }
}

#[test]
fn certify_non_existential_program_is_trivially_weakly_acyclic() {
    // A plain Datalog rule (no ∃ head var) has no special edges → certified.
    let datalog = ExistentialRule {
        numeric: Vec::new(),
        rule_iri: "http://ex/rule/datalog".to_owned(),
        body: vec![atom(var("?x"), P, var("?y"))],
        head: vec![atom(var("?y"), P, var("?x"))],
        distinct: vec![],
        witness_frontier: None,
        witness_policy: WitnessPolicy::FrontierSkolem,
    };
    let admission = ChaseAdmission::certify(&[datalog]);
    assert!(admission.admits_native());
    assert!(matches!(
        admission,
        ChaseAdmission::WeaklyAcyclic { evidence } if evidence.contains("0 existential edge")
    ));
}

#[test]
fn certify_lattice_combine_takes_the_weaker() {
    // The whole program is admitted only if every part is: combine → the weaker.
    let good = ChaseAdmission::certify(&[some_values_from_rule()]);
    let bad = ChaseAdmission::certify(&[restriction_rule("http://ex/r", D, P, D)]);
    assert!(!good.clone().combine(bad.clone()).admits_native());
    assert!(!bad.combine(good).admits_native());
}

#[test]
fn certify_lattice_combine_merges_uncertified_violations() {
    // Two uncertified parts meet to Uncertified keeping EVERY violation — merged,
    // sorted, deduped — so no termination-failure diagnostic is dropped by the meet.
    let a = ChaseAdmission::Uncertified {
        violations: vec!["edge y -> z in cycle".to_owned(), "shared".to_owned()],
    };
    let b = ChaseAdmission::Uncertified {
        violations: vec!["edge p -> q in cycle".to_owned(), "shared".to_owned()],
    };
    match a.combine(b) {
        ChaseAdmission::Uncertified { violations } => assert_eq!(
            violations,
            vec![
                "edge p -> q in cycle".to_owned(),
                "edge y -> z in cycle".to_owned(),
                "shared".to_owned(),
            ],
            "combine keeps every violation, sorted and deduped (no lost diagnostic)"
        ),
        other => panic!("two uncertified parts combine to Uncertified, got {other:?}"),
    }
}

#[test]
fn certify_lattice_combine_is_the_strength_poset_meet() {
    // combine is the certificate-STRENGTH lattice meet (glb) over the poset
    // WA ⊏ {JA ∥ SWA} ⊏ MSA — NOT a linearization by escalation rank. The incomparable
    // siblings JA and SWA meet to their glb, WeaklyAcyclic.
    let ev = |s: &str| s.to_owned();
    let wa = || ChaseAdmission::WeaklyAcyclic { evidence: ev("wa") };
    let ja = || ChaseAdmission::JointlyAcyclic { evidence: ev("ja") };
    let swa = || ChaseAdmission::SuperWeaklyAcyclic {
        evidence: ev("swa"),
    };
    let msa = || ChaseAdmission::ModelSummarizingAcyclic {
        evidence: ev("msa"),
    };

    for cert in [wa(), ja(), swa(), msa()] {
        assert!(
            cert.admits_native(),
            "every certified class admits: {cert:?}"
        );
    }

    // Comparable pairs meet to the lower (more conservative) class, commutatively.
    let comparable = [
        (wa(), ja(), wa()),
        (wa(), swa(), wa()),
        (wa(), msa(), wa()),
        (ja(), msa(), ja()),
        (swa(), msa(), swa()),
    ];
    for (a, b, glb) in comparable {
        assert_eq!(a.clone().combine(b.clone()), glb, "meet({a:?}, {b:?})");
        assert_eq!(b.combine(a), glb, "meet is commutative");
    }

    // The INCOMPARABLE siblings JA ∥ SWA meet to their glb, WeaklyAcyclic — never to
    // JA (the escalation-cheaper sibling), which a rank linearization would give.
    match ja().combine(swa()) {
        ChaseAdmission::WeaklyAcyclic { evidence } => assert!(
            evidence.contains("incomparable") && evidence.contains("greatest lower bound"),
            "JA ∧ SWA glb evidence must record the incomparable meet: {evidence}"
        ),
        other => panic!("JA ∧ SWA must meet to WeaklyAcyclic (glb), got {other:?}"),
    }
    assert!(
        matches!(swa().combine(ja()), ChaseAdmission::WeaklyAcyclic { .. }),
        "the incomparable meet is commutative"
    );
    assert_ne!(
        ja().combine(swa()),
        ja(),
        "combine must NOT linearize JA ∥ SWA to JointlyAcyclic"
    );

    // Any certified class meets Uncertified down to Uncertified.
    let uncertified = ChaseAdmission::Uncertified {
        violations: vec![ev("v")],
    };
    for cert in [wa(), ja(), swa(), msa()] {
        assert!(!cert.clone().combine(uncertified.clone()).admits_native());
        assert!(!uncertified.clone().combine(cert).admits_native());
    }
}

// ── route_chase: certify → chase / refuse / budget ───────────────────────────

#[test]
fn route_certified_program_runs_natively() {
    let edb = vec![fact("http://ex/a", TYPE, C)];
    let (admission, outcome) = route_chase(W, &edb, &[some_values_from_rule()], None).unwrap();
    assert!(admission.admits_native());
    let b = decided(outcome);
    assert_eq!(b.status, BudgetStatus::Ok);
    assert_eq!(count(&b.rows, P), 1);
}

#[test]
fn route_uncertified_without_budget_refuses_to_the_oracle() {
    // Cyclic D⊑∃p.D, no budget ⇒ a first-class declared gap, never
    // a native loop.
    let cyclic = restriction_rule("http://ex/rule/cyclic", D, P, D);
    let edb = vec![fact("http://ex/a", TYPE, D)];
    let (admission, outcome) = route_chase(W, &edb, &[cyclic], None).unwrap();
    assert!(!admission.admits_native());
    assert!(matches!(
        outcome,
        NativeOutcome::Unsupported(UnsupportedKind::NonTerminatingExistential)
    ));
}

#[test]
fn route_uncertified_with_budget_runs_partial() {
    // Cyclic program WITH a budget ⇒ budgeted-partial native run (incomplete, never
    // wrong), deterministically selected by budget config.
    let cyclic = restriction_rule("http://ex/rule/cyclic", D, P, D);
    let edb = vec![fact("http://ex/a", TYPE, D)];
    let (admission, outcome) = route_chase(W, &edb, &[cyclic], Some(2)).unwrap();
    assert!(!admission.admits_native());
    let b = decided(outcome);
    assert_eq!(b.status, BudgetStatus::Exhausted);
    assert_eq!(b.consumed_steps, 2);
}

// ── H3: capability-gap counting, invented-individual explain, certificate Finding ──

#[test]
fn refused_existential_program_counts_a_reason_ledger_dlgap() {
    // A cyclic `D ⊑ ∃p.D` is uncertified; its refusal is a COUNTED reason::ledger
    // DlGap carrying the weak-acyclicity violation evidence — never silently dropped.
    let cyclic = restriction_rule("http://ex/rule/cyclic", D, P, D);
    let admission = ChaseAdmission::certify(&[cyclic]);
    assert!(
        !admission.admits_native(),
        "cyclic program must be uncertified"
    );

    let rows = admission.capability_gap_rows();
    assert_eq!(rows.len(), 1, "one DlGap row per violation");
    assert_eq!(rows[0].kind, crate::reason::ledger::DivergenceKind::DlGap);
    assert_eq!(
        rows[0].category,
        crate::reason::ledger::EXISTENTIAL_CHASE_CATEGORY,
        "scoped out of the DL/EL crosscheck corpus by category"
    );
    assert!(
        rows[0].detail.contains("lies in a cycle"),
        "the violation evidence rides in detail: {:?}",
        rows[0].detail
    );

    // Routed into the counted divergence ledger it IS tallied and fails enforce…
    let ledger = crate::reason::ledger::build_ledger(Vec::new(), rows, Vec::new());
    assert_eq!(ledger.dl_gap, 1, "counted as a DL gap in reason::ledger");
    assert!(!crate::reason::ledger::enforce(&ledger).passed);

    // …but a CERTIFIED program contributes no gap rows.
    assert!(
        ChaseAdmission::certify(&[some_values_from_rule()])
            .capability_gap_rows()
            .is_empty(),
        "a weakly-acyclic program is not a capability-gap"
    );
}

#[test]
fn explain_recovers_the_recipe_of_a_chase_invented_witness() {
    // Run the chase on `C ⊑ ∃p.D` for one C-individual, then EXPLAIN the invented null:
    // its recipe must name the firing rule and the frontier binding (the C-individual).
    let edb = vec![fact("http://ex/a", TYPE, C)];
    let (outcome, registry) =
        chase_world_explained(W, &edb, &[some_values_from_rule()], None).unwrap();
    let b = decided(outcome);

    // The one p-edge's object is the invented witness.
    let witness = b
        .rows
        .iter()
        .find(|r| r.predicate == P)
        .map(|r| term_display(&r.object))
        .expect("the chase must invent a p-target witness");
    let witness_iri = witness
        .strip_prefix('<')
        .and_then(|s| s.strip_suffix('>'))
        .expect("witness is an IRI display form");

    assert_eq!(registry.len(), 1, "exactly one witness invented");
    let derivation = registry
        .explain(witness_iri)
        .expect("the invented witness must be explainable from the registry");
    assert_eq!(derivation.witness, witness_iri);
    assert_eq!(derivation.rule_iri, "http://ex/rule/svf");
    assert_eq!(derivation.ordinal, 0);
    assert_eq!(derivation.frontier, vec![TermValue::iri("http://ex/a")]);
    assert_eq!(derivation.scope.world, W);
    assert_eq!(derivation.heads.len(), 2);
    derivation.validate().unwrap();

    // A never-invented term is not explainable.
    assert!(registry.explain("http://ex/a").is_none());
}

#[test]
fn certificate_finding_carries_evidence_or_violations() {
    // WeaklyAcyclic ⇒ an informational Finding carrying the proof evidence.
    let good = ChaseAdmission::certify(&[some_values_from_rule()]);
    let good_finding = good.to_finding();
    assert_eq!(good_finding.severity, Severity::Info);
    assert_eq!(good_finding.code, "chase.certificate.weakly-acyclic");
    assert_eq!(good_finding.tool.as_deref(), Some("chase"));
    assert!(
        good_finding.message.contains("weakly acyclic"),
        "the WeaklyAcyclic finding carries its evidence: {}",
        good_finding.message
    );

    // Uncertified ⇒ an error Finding carrying the weak-acyclicity violations.
    let bad = ChaseAdmission::certify(&[restriction_rule("http://ex/rule/cyclic", D, P, D)]);
    let bad_finding = bad.to_finding();
    assert_eq!(bad_finding.severity, Severity::Error);
    assert_eq!(bad_finding.code, "chase.certificate.uncertified");
    assert!(
        bad_finding.message.contains("lies in a cycle"),
        "the Uncertified finding carries its violations: {}",
        bad_finding.message
    );
}
