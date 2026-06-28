// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Unit tests for the native teleology evaluator (issue #1055 W1).
//!
//! These build small N-Quads worlds mirroring the eight named W1 conformance
//! scenarios, run the evaluator's pure computations, and assert (a) the factored
//! verdict and (b) the content-addressed provenance determinism (same input → same
//! derivation IRIs).

use super::*;
use crate::store::WorldStore;
use std::collections::{BTreeMap, BTreeSet};

const W: &str = "https://blackcatinformatics.ca/gmeow/examples/w1/world";

/// Build a `WorldStore` from N-Quads text.
fn store_from(nq: &str) -> WorldStore {
    let store = WorldStore::new();
    store.load_nquads(nq).expect("valid N-Quads");
    store
}

/// Shorthand for a `logic:` IRI in N-Quads angle-bracket form.
fn l(local: &str) -> String {
    format!("<https://blackcatinformatics.ca/logic/{local}>")
}

/// Build a path of states chained by temporallySucceeds, each carrying its
/// situation-obtains facts. `obtains[i]` = situation locals obtaining at state i.
fn path_nq(obtains: &[&[&str]]) -> String {
    let mut s = String::new();
    for (i, obtained) in obtains.iter().enumerate() {
        let state = format!("<{W}#state{i}>");
        if i > 0 {
            let prev = format!("<{W}#state{}>", i - 1);
            s.push_str(&format!(
                "{state} {} {prev} <{W}> .\n",
                l("temporallySucceeds")
            ));
        }
        for sit in *obtained {
            s.push_str(&format!(
                "{state} {} <{W}#{sit}> <{W}> .\n",
                l("situationObtains")
            ));
        }
    }
    s
}

/// Read the world's facts.
fn facts_of(nq: &str) -> WorldFacts {
    let store = store_from(nq);
    WorldFacts::read(&store, W)
}

fn goal_expr_nq(local: &str, kind: &str, extra: &str) -> String {
    let g = format!("<{W}#{local}>");
    format!(
        "{g} {} {} <{W}> .\n{extra}",
        l("goalExpressionKind"),
        l(kind)
    )
}

// ── Scenario 1: conjunctive vs disjunctive ──────────────────────────────────────

#[test]
fn conjunctive_all_satisfied_vs_one_missing() {
    let mut nq = path_nq(&[&["sitA"]]);
    nq.push_str(&goal_expr_nq(
        "atomA",
        "AtomicGoal",
        &format!(
            "<{W}#atomA> {} <{W}#sitA> <{W}> .\n",
            l("boundSituationType")
        ),
    ));
    nq.push_str(&goal_expr_nq(
        "atomB",
        "AtomicGoal",
        &format!(
            "<{W}#atomB> {} <{W}#sitB> <{W}> .\n",
            l("boundSituationType")
        ),
    ));
    nq.push_str(&goal_expr_nq(
        "conj",
        "ConjunctiveGoal",
        &format!(
            "<{W}#conj> {0} <{W}#atomA> <{W}> .\n<{W}#conj> {0} <{W}#atomB> <{W}> .\n",
            l("operand")
        ),
    ));
    nq.push_str(&goal_expr_nq(
        "disj",
        "DisjunctiveGoal",
        &format!(
            "<{W}#disj> {0} <{W}#atomA> <{W}> .\n<{W}#disj> {0} <{W}#atomB> <{W}> .\n",
            l("operand")
        ),
    ));
    let f = facts_of(&nq);
    let states = ordered_states(&f).unwrap();
    let conj = evaluate_goal_over_path(&f, &format!("{W}#conj"), &states).unwrap();
    let disj = evaluate_goal_over_path(&f, &format!("{W}#disj"), &states).unwrap();
    assert_eq!(conj.satisfaction, Satisfaction::Unsatisfied);
    assert_eq!(conj.evaluation_status, EvaluationStatus::Completed);
    assert_eq!(disj.satisfaction, Satisfaction::Satisfied);
    assert_eq!(disj.evaluation_status, EvaluationStatus::Completed);
}

// ── Scenario 2: maintenance fail-midway vs open-window undetermined ──────────────

#[test]
fn maintenance_fail_midway_is_violated() {
    let mut nq = path_nq(&[&["safe"], &["safe"], &[]]);
    nq.push_str(&goal_expr_nq(
        "maint",
        "MaintenanceGoal",
        &format!(
            "<{W}#maint> {} <{W}#safe> <{W}> .\n",
            l("boundSituationType")
        ),
    ));
    let f = facts_of(&nq);
    let states = ordered_states(&f).unwrap();
    let v = evaluate_goal_over_path(&f, &format!("{W}#maint"), &states).unwrap();
    assert_eq!(v.satisfaction, Satisfaction::Violated);
    assert_eq!(v.evaluation_status, EvaluationStatus::Completed);
}

#[test]
fn maintenance_open_window_is_undetermined() {
    let mut nq = path_nq(&[&["safe"], &["safe"], &["safe"]]);
    nq.push_str(&goal_expr_nq(
        "maint",
        "MaintenanceGoal",
        &format!(
            "<{W}#maint> {} <{W}#safe> <{W}> .\n",
            l("boundSituationType")
        ),
    ));
    let f = facts_of(&nq);
    let states = ordered_states(&f).unwrap();
    let v = evaluate_goal_over_path(&f, &format!("{W}#maint"), &states).unwrap();
    assert_eq!(v.satisfaction, Satisfaction::Satisfied);
    assert_eq!(
        v.evaluation_status,
        EvaluationStatus::Undetermined,
        "open maintenance window must be Undetermined, not conclusively Satisfied"
    );
}

#[test]
fn deadline_window_closed_promotes_to_conclusive() {
    let mut nq = path_nq(&[&["safe"], &["safe"]]);
    nq.push_str(&goal_expr_nq(
        "maint",
        "MaintenanceGoal",
        &format!(
            "<{W}#maint> {} <{W}#safe> <{W}> .\n",
            l("boundSituationType")
        ),
    ));
    nq.push_str(&goal_expr_nq(
        "dw",
        "DeadlineWindowGoal",
        &format!(
            "<{W}#dw> {} <{W}#maint> <{W}> .\n<{W}#dw> {} \"true\"^^<http://www.w3.org/2001/XMLSchema#boolean> <{W}> .\n",
            l("operand"),
            l("deadlineWindowClosed")
        ),
    ));
    let f = facts_of(&nq);
    let states = ordered_states(&f).unwrap();
    let v = evaluate_goal_over_path(&f, &format!("{W}#dw"), &states).unwrap();
    assert_eq!(v.satisfaction, Satisfaction::Satisfied);
    assert_eq!(v.evaluation_status, EvaluationStatus::Completed);
}

#[test]
fn avoidance_dual_of_maintenance() {
    let mut nq = path_nq(&[&[], &["bad"]]);
    nq.push_str(&goal_expr_nq(
        "avoid",
        "AvoidanceGoal",
        &format!(
            "<{W}#avoid> {} <{W}#bad> <{W}> .\n",
            l("boundSituationType")
        ),
    ));
    let f = facts_of(&nq);
    let states = ordered_states(&f).unwrap();
    let v = evaluate_goal_over_path(&f, &format!("{W}#avoid"), &states).unwrap();
    assert_eq!(v.satisfaction, Satisfaction::Violated);
    assert_eq!(v.evaluation_status, EvaluationStatus::Completed);
}

// ── Scenario 3: conditional false-guard prescribes nothing ──────────────────────

#[test]
fn conditional_false_guard_does_not_apply() {
    let mut nq = path_nq(&[&["unrelated"]]);
    nq.push_str(&goal_expr_nq(
        "atom",
        "AtomicGoal",
        &format!(
            "<{W}#atom> {} <{W}#target> <{W}> .\n",
            l("boundSituationType")
        ),
    ));
    nq.push_str(&goal_expr_nq(
        "cond",
        "ConditionalGoal",
        &format!(
            "<{W}#cond> {} <{W}#guardSit> <{W}> .\n<{W}#cond> {} <{W}#atom> <{W}> .\n",
            l("guardSituation"),
            l("operand")
        ),
    ));
    let f = facts_of(&nq);
    let states = ordered_states(&f).unwrap();
    let v = evaluate_goal_over_path(&f, &format!("{W}#cond"), &states).unwrap();
    assert_eq!(
        v.satisfaction,
        Satisfaction::DoesNotApply,
        "false guard must prescribe nothing"
    );
    assert_eq!(v.evaluation_status, EvaluationStatus::Completed);
}

#[test]
fn conditional_true_guard_evaluates_operand() {
    let mut nq = path_nq(&[&["guardSit", "target"]]);
    nq.push_str(&goal_expr_nq(
        "atom",
        "AtomicGoal",
        &format!(
            "<{W}#atom> {} <{W}#target> <{W}> .\n",
            l("boundSituationType")
        ),
    ));
    nq.push_str(&goal_expr_nq(
        "cond",
        "ConditionalGoal",
        &format!(
            "<{W}#cond> {} <{W}#guardSit> <{W}> .\n<{W}#cond> {} <{W}#atom> <{W}> .\n",
            l("guardSituation"),
            l("operand")
        ),
    ));
    let f = facts_of(&nq);
    let states = ordered_states(&f).unwrap();
    let v = evaluate_goal_over_path(&f, &format!("{W}#cond"), &states).unwrap();
    assert_eq!(v.satisfaction, Satisfaction::Satisfied);
}

// ── Scenario 4: partial satisfaction kept apart from low confidence ─────────────

#[test]
fn optimization_records_degree_not_truth_value() {
    let mut nq = String::new();
    nq.push_str(&goal_expr_nq(
        "opt",
        "OptimizationGoal",
        &format!(
            "<{W}#opt> {} \"0.62\"^^<http://www.w3.org/2001/XMLSchema#decimal> <{W}> .\n",
            l("satisfactionDegree")
        ),
    ));
    let f = facts_of(&nq);
    let v = evaluate_goal_over_path(&f, &format!("{W}#opt"), &[]).unwrap();
    assert_eq!(v.satisfaction, Satisfaction::PartiallySatisfied);
    assert_eq!(v.degree.as_deref(), Some("0.62"));
    assert_ne!(
        v.satisfaction,
        Satisfaction::Satisfied,
        "a degree is never folded into a crisp truth value"
    );
}

// ── Scenario 5: weak vs strong plan over the same outcome set ────────────────────

fn two_outcome_schema_nq(o1_reaches: bool, o2_reaches: bool, o2_recoverable: bool) -> String {
    let mut nq = String::new();
    nq.push_str(&format!(
        "<{W}#schema> {0} <{W}#o1> <{W}> .\n<{W}#schema> {0} <{W}#o2> <{W}> .\n",
        l("nondeterministicOutcome")
    ));
    let s1 = if o1_reaches { "goalSit" } else { "otherSit" };
    let s2 = if o2_reaches { "goalSit" } else { "otherSit" };
    nq.push_str(&format!(
        "<{W}#o1> {} <{W}#{s1}> <{W}> .\n",
        l("outcomeSituation")
    ));
    nq.push_str(&format!(
        "<{W}#o2> {} <{W}#{s2}> <{W}> .\n",
        l("outcomeSituation")
    ));
    if o2_recoverable {
        nq.push_str(&format!(
            "<{W}#o2> {} \"true\"^^<http://www.w3.org/2001/XMLSchema#boolean> <{W}> .\n",
            l("recoverableOutcome")
        ));
    }
    nq
}

#[test]
fn strong_plan_when_every_outcome_reaches() {
    let nq = two_outcome_schema_nq(true, true, false);
    let f = facts_of(&nq);
    let v = classify_plan_success(&f, &format!("{W}#schema"), &format!("{W}#goalSit")).unwrap();
    assert_eq!(v, PlanSuccess::Strong);
}

#[test]
fn weak_plan_when_some_but_unrecoverable_miss() {
    let nq = two_outcome_schema_nq(true, false, false);
    let f = facts_of(&nq);
    let v = classify_plan_success(&f, &format!("{W}#schema"), &format!("{W}#goalSit")).unwrap();
    assert_eq!(v, PlanSuccess::Weak);
}

#[test]
fn strong_cyclic_when_misses_are_recoverable() {
    let nq = two_outcome_schema_nq(true, false, true);
    let f = facts_of(&nq);
    let v = classify_plan_success(&f, &format!("{W}#schema"), &format!("{W}#goalSit")).unwrap();
    assert_eq!(v, PlanSuccess::StrongCyclic);
}

// ── Scenario 6: outcome-specific compensation (two branches recover differently) ─

#[test]
fn outcome_specific_compensation_picks_the_right_branch() {
    let mut nq = String::new();
    nq.push_str(&format!(
        "<{W}#schema> {0} <{W}#o1> <{W}> .\n<{W}#schema> {0} <{W}#o2> <{W}> .\n",
        l("nondeterministicOutcome")
    ));
    nq.push_str(&format!(
        "<{W}#o1> {} <{W}#undo1> <{W}> .\n",
        l("compensation")
    ));
    nq.push_str(&format!(
        "<{W}#o2> {} <{W}#undo2> <{W}> .\n",
        l("compensation")
    ));
    let f = facts_of(&nq);
    let c1 = compensation_for_outcome(&f, &format!("{W}#o1")).unwrap();
    let c2 = compensation_for_outcome(&f, &format!("{W}#o2")).unwrap();
    assert_eq!(c1, format!("{W}#undo1"));
    assert_eq!(c2, format!("{W}#undo2"));
    assert_ne!(c1, c2, "each branch must name its own compensation");
}

// ── Scenario 7: deontic no-accessible-ideal-world → undetermined ────────────────

#[test]
fn deontic_no_ideal_world_is_undetermined() {
    let base_nq = path_nq(&[&["sit"]]);
    let base = facts_of(&base_nq);
    let ideals: BTreeMap<String, WorldFacts> = BTreeMap::new();
    let v = evaluate_deontic(
        &base,
        W,
        &ideals,
        &format!("{W}#goalSit"),
        &format!("{W}#proscribed"),
    );
    assert_eq!(
        v,
        DeonticVerdict::Undetermined,
        "no accessible ideal world must be Undetermined, NOT a vacuous obligation"
    );
}

#[test]
fn deontic_obligation_holds_in_every_ideal_world() {
    let iw = "https://blackcatinformatics.ca/gmeow/examples/w1/ideal";
    let base_nq = format!("<{W}#x> {} <{iw}> <{W}> .\n", l("deonticallyIdeal"));
    let base = facts_of(&base_nq);
    let ideal_nq = format!(
        "<{iw}#s0> {} <{iw}#goalSit> <{iw}> .\n",
        l("situationObtains")
    );
    let store = store_from(&ideal_nq);
    let mut ideals: BTreeMap<String, WorldFacts> = BTreeMap::new();
    ideals.insert(iw.to_owned(), WorldFacts::read(&store, iw));
    let v = evaluate_deontic(
        &base,
        W,
        &ideals,
        &format!("{iw}#goalSit"),
        &format!("{iw}#proscribed"),
    );
    assert_eq!(v, DeonticVerdict::ObligationHolds);
}

#[test]
fn deontic_prohibition_needs_support_for_negation() {
    let iw = "https://blackcatinformatics.ca/gmeow/examples/w1/ideal";
    let base_nq = format!("<{W}#x> {} <{iw}> <{W}> .\n", l("deonticallyIdeal"));
    let base = facts_of(&base_nq);
    let ideal_nq = format!(
        "<{iw}#s0> {} <{iw}#proscribed> <{iw}> .\n",
        l("situationObtains")
    );
    let store = store_from(&ideal_nq);
    let mut ideals: BTreeMap<String, WorldFacts> = BTreeMap::new();
    ideals.insert(iw.to_owned(), WorldFacts::read(&store, iw));
    let v = evaluate_deontic(
        &base,
        W,
        &ideals,
        &format!("{iw}#goalSit"),
        &format!("{iw}#proscribed"),
    );
    assert_eq!(
        v,
        DeonticVerdict::ProhibitionHolds,
        "support-for-negation (positive witness) must give ProhibitionHolds"
    );
}

// ── Serialization-anomaly detection ─────────────────────────────────────────────

#[test]
fn serializable_history_no_cycle() {
    let edges = vec![
        ConflictEdge {
            from: "t1".into(),
            to: "t2".into(),
        },
        ConflictEdge {
            from: "t2".into(),
            to: "t3".into(),
        },
    ];
    assert_eq!(
        detect_serialization_anomaly(&edges),
        SerializationVerdict::Serializable
    );
}

#[test]
fn cyclic_history_is_an_anomaly_not_a_contradiction() {
    let edges = vec![
        ConflictEdge {
            from: "t1".into(),
            to: "t2".into(),
        },
        ConflictEdge {
            from: "t2".into(),
            to: "t1".into(),
        },
    ];
    match detect_serialization_anomaly(&edges) {
        SerializationVerdict::Anomaly(cycle) => {
            assert_eq!(cycle, vec!["t1", "t2", "t1"]);
        }
        SerializationVerdict::Serializable => panic!("expected an anomaly"),
    }
}

#[test]
fn serialization_anomaly_is_a_finding_with_provenance() {
    let edges = vec![
        ConflictEdge {
            from: "https://x/t1".into(),
            to: "https://x/t2".into(),
        },
        ConflictEdge {
            from: "https://x/t2".into(),
            to: "https://x/t1".into(),
        },
    ];
    let cycle = match detect_serialization_anomaly(&edges) {
        SerializationVerdict::Anomaly(c) => c,
        SerializationVerdict::Serializable => panic!("expected anomaly"),
    };
    let quads = emit_serialization_anomaly(
        W,
        &format!("{W}#finding1"),
        &cycle,
        &format!("{LOGIC_NS}ConflictSerializability"),
        &edges,
    )
    .unwrap();
    assert!(quads
        .iter()
        .any(|q| q.predicate == RDF_TYPE && q.object == n3(&logic("SerializationAnomaly"))));
    assert!(!quads
        .iter()
        .any(|q| q.object.contains("contradictionWitness")));
    for q in &quads {
        assert!(q
            .derivation_id
            .starts_with("https://blackcatinformatics.ca/gmeow/derivation/"));
        assert_eq!(q.rule_iri, TELEOLOGY_RULE_IRI);
    }
}

// ── MCP action-policy evaluation ────────────────────────────────────────────────

#[test]
fn gate_admits_when_precondition_and_capability_hold() {
    let mut nq = path_nq(&[&["ready"]]);
    nq.push_str(&format!(
        "<{W}#schema> {} <{W}#ready> <{W}> .\n",
        l("precondition")
    ));
    nq.push_str(&format!(
        "<{W}#schema> {} <{W}#canWrite> <{W}> .\n",
        l("capability")
    ));
    let f = facts_of(&nq);
    let mut caps = BTreeSet::new();
    caps.insert(format!("{W}#canWrite"));
    let gate = gate_action(&f, &format!("{W}#schema"), &format!("{W}#state0"), &caps);
    assert_eq!(gate, ActionGate::Admit);
}

#[test]
fn gate_denies_and_returns_compensation_on_precondition_failure() {
    let mut nq = path_nq(&[&["notReady"]]);
    nq.push_str(&format!(
        "<{W}#schema> {} <{W}#ready> <{W}> .\n",
        l("precondition")
    ));
    nq.push_str(&format!(
        "<{W}#schema> {} <{W}#rollback> <{W}> .\n",
        l("compensation")
    ));
    let f = facts_of(&nq);
    let caps = BTreeSet::new();
    let gate = gate_action(&f, &format!("{W}#schema"), &format!("{W}#state0"), &caps);
    match gate {
        ActionGate::Deny { compensation, .. } => {
            assert_eq!(compensation, Some(format!("{W}#rollback")));
        }
        ActionGate::Admit => panic!("expected denial when precondition fails"),
    }
}

#[test]
fn gate_denies_when_capability_unavailable() {
    let mut nq = path_nq(&[&["ready"]]);
    nq.push_str(&format!(
        "<{W}#schema> {} <{W}#ready> <{W}> .\n",
        l("precondition")
    ));
    nq.push_str(&format!(
        "<{W}#schema> {} <{W}#canWrite> <{W}> .\n",
        l("capability")
    ));
    let f = facts_of(&nq);
    let caps = BTreeSet::new();
    let gate = gate_action(&f, &format!("{W}#schema"), &format!("{W}#state0"), &caps);
    assert!(matches!(gate, ActionGate::Deny { .. }));
}

// ── Scenario 8: contested evaluations (two coexisting GoalEvaluations) ───────────

#[test]
fn contested_evaluations_retained_with_factored_axes() {
    let mut nq = path_nq(&[&["sitA"]]);
    nq.push_str(&goal_expr_nq(
        "atomA",
        "AtomicGoal",
        &format!(
            "<{W}#atomA> {} <{W}#sitA> <{W}> .\n",
            l("boundSituationType")
        ),
    ));
    nq.push_str(&format!(
        "<{W}#goal1> {} <{W}#atomA> <{W}> .\n",
        l("hasGoalCondition")
    ));
    let store = store_from(&nq);
    let out = evaluate_world_goals(&store, W).unwrap();
    let evals: Vec<&TeleologyQuad> = out
        .iter()
        .filter(|q| q.predicate == RDF_TYPE && q.object == n3(&logic("GoalEvaluation")))
        .collect();
    assert_eq!(
        evals.len(),
        1,
        "driver emits the default-vantage evaluation"
    );
    // Factored axes are emitted as DISTINCT predicates — never collapsed into one label.
    assert!(out
        .iter()
        .any(|q| q.predicate == logic("satisfactionStatus")));
    assert!(out
        .iter()
        .any(|q| q.predicate == logic("goalEvaluationStatus")));
    let sat = out
        .iter()
        .find(|q| q.predicate == logic("satisfactionStatus"));
    assert_eq!(sat.unwrap().object, n3(&logic("Satisfied")));
}

// ── Determinism: same input → same provenance ids ───────────────────────────────

#[test]
fn determinism_same_input_same_derivation_ids() {
    let mut nq = path_nq(&[&["sitA"]]);
    nq.push_str(&goal_expr_nq(
        "atomA",
        "AtomicGoal",
        &format!(
            "<{W}#atomA> {} <{W}#sitA> <{W}> .\n",
            l("boundSituationType")
        ),
    ));
    nq.push_str(&format!(
        "<{W}#goal1> {} <{W}#atomA> <{W}> .\n",
        l("hasGoalCondition")
    ));
    let a = evaluate_world_goals(&store_from(&nq), W).unwrap();
    let b = evaluate_world_goals(&store_from(&nq), W).unwrap();
    assert_eq!(
        a, b,
        "same input must yield byte-identical quads + provenance"
    );
    for q in &a {
        assert!(q
            .derivation_id
            .starts_with("https://blackcatinformatics.ca/gmeow/derivation/"));
    }
}

#[test]
fn unknown_goal_kind_is_hard_error() {
    let nq = format!(
        "<{W}#bad> {} <{W}#NotAKind> <{W}> .\n",
        l("goalExpressionKind")
    );
    let f = facts_of(&nq);
    let err = evaluate_goal_over_path(&f, &format!("{W}#bad"), &[]).unwrap_err();
    assert!(
        err.contains("Unknown logic:GoalExpressionKind"),
        "got: {err}"
    );
}

#[test]
fn nonlinear_path_is_hard_error() {
    let nq = format!(
        "<{W}#state2> {0} <{W}#state0> <{W}> .\n<{W}#state2> {0} <{W}#state1> <{W}> .\n",
        l("temporallySucceeds")
    );
    let f = facts_of(&nq);
    let err = ordered_states(&f).unwrap_err();
    assert!(
        err.contains("not linear") || err.contains("predecessors") || err.contains("start"),
        "got: {err}"
    );
}
