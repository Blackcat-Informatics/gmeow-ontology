// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Unit tests for the native teleology evaluator.
//!
//! These build small N-Quads worlds mirroring the eight named conformance
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
    )
    .expect("valid deontic path");
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
    )
    .expect("valid deontic path");
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
    )
    .expect("valid deontic path");
    assert_eq!(
        v,
        DeonticVerdict::ProhibitionHolds,
        "support-for-negation (positive witness) must give ProhibitionHolds"
    );
}

// ── Scenario 7b: deontic forked ideal-world path → hard error ───────────────────

#[test]
fn deontic_forked_ideal_world_path_is_hard_error() {
    // Build an ideal world whose temporallySucceeds graph is forked: both s1 and s2
    // succeed s0, giving s0 two successors — ordered_states must return Err.
    let iw = "https://blackcatinformatics.ca/gmeow/examples/w1/ideal";
    let base_nq = format!("<{W}#x> {} <{iw}> <{W}> .\n", l("deonticallyIdeal"));
    let base = facts_of(&base_nq);
    // s1 → s0 and s2 → s0 both via temporallySucceeds: s0 has two successors.
    let ideal_nq = format!(
        "<{iw}#s1> {ts} <{iw}#s0> <{iw}> .\n\
         <{iw}#s2> {ts} <{iw}#s0> <{iw}> .\n",
        ts = l("temporallySucceeds"),
    );
    let store = store_from(&ideal_nq);
    let mut ideals: BTreeMap<String, WorldFacts> = BTreeMap::new();
    ideals.insert(iw.to_owned(), WorldFacts::read(&store, iw));
    let result = evaluate_deontic(
        &base,
        W,
        &ideals,
        &format!("{iw}#goalSit"),
        &format!("{iw}#proscribed"),
    );
    assert!(
        result.is_err(),
        "a forked ideal-world path must hard-fail, not degrade to Neither"
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

// ── The REAL memory-MCP triad's store_claim schema under gate_action ─────────────
//
// These exercise gate_action over EXACTLY the store_claim pattern dogfooded in
// slices/extensions/agentic/examples/mcp-action-policy.ttl — same logic: facets
// (capability / precondition / compensation) and the same triad wiring (store_claim's
// compensation is revise_belief). The example's ex: IRIs live in a CC-BY example
// namespace; here they ride the test world (W) under the SAME local names so the
// structure the policy reads is identical.

/// Build the store_claim McpActionSchema (precondition wellFormedClaim, capability
/// memoryWriteCapability, compensation reviseBelief — the rollback) with a one-state
/// path where `obtained` lists the situation locals that obtain at state0.
fn store_claim_triad_nq(obtained: &[&str]) -> String {
    let mut nq = path_nq(&[obtained]);
    nq.push_str(&format!(
        "<{W}#storeClaim> {} <{W}#wellFormedClaim> <{W}> .\n",
        l("precondition")
    ));
    nq.push_str(&format!(
        "<{W}#storeClaim> {} <{W}#memoryWriteCapability> <{W}> .\n",
        l("capability")
    ));
    nq.push_str(&format!(
        "<{W}#storeClaim> {} <{W}#reviseBelief> <{W}> .\n",
        l("compensation")
    ));
    nq
}

#[test]
fn store_claim_admitted_when_precondition_holds_and_capability_available() {
    // well-formed claim obtains AND memory-write capability is held → ADMIT.
    let f = facts_of(&store_claim_triad_nq(&["wellFormedClaim"]));
    let mut caps = BTreeSet::new();
    caps.insert(format!("{W}#memoryWriteCapability"));
    let gate = gate_action(
        &f,
        &format!("{W}#storeClaim"),
        &format!("{W}#state0"),
        &caps,
    );
    assert_eq!(gate, ActionGate::Admit);
}

#[test]
fn store_claim_denied_on_missing_precondition_with_revise_belief_rollback() {
    // precondition (well-formed claim) does NOT obtain → DENY, carrying revise_belief
    // as the compensation/rollback. Capability is available so the precondition is the
    // sole cause.
    let f = facts_of(&store_claim_triad_nq(&[])); // state0 has no situations obtaining
    let mut caps = BTreeSet::new();
    caps.insert(format!("{W}#memoryWriteCapability"));
    let gate = gate_action(
        &f,
        &format!("{W}#storeClaim"),
        &format!("{W}#state0"),
        &caps,
    );
    match gate {
        ActionGate::Deny {
            compensation,
            reason,
        } => {
            assert_eq!(
                compensation,
                Some(format!("{W}#reviseBelief")),
                "rollback for a denied store_claim must be revise_belief (P10 suppression)"
            );
            assert!(
                reason.contains("wellFormedClaim"),
                "denial reason must name the failing precondition, got {reason:?}"
            );
        }
        ActionGate::Admit => panic!("expected denial when the precondition is absent"),
    }
}

#[test]
fn store_claim_denied_on_unavailable_capability_with_revise_belief_rollback() {
    // precondition holds but the memory-write capability is NOT available → DENY,
    // again carrying revise_belief as the rollback.
    let f = facts_of(&store_claim_triad_nq(&["wellFormedClaim"]));
    let caps = BTreeSet::new(); // no capabilities held
    let gate = gate_action(
        &f,
        &format!("{W}#storeClaim"),
        &format!("{W}#state0"),
        &caps,
    );
    match gate {
        ActionGate::Deny {
            compensation,
            reason,
        } => {
            assert_eq!(compensation, Some(format!("{W}#reviseBelief")));
            assert!(
                reason.contains("memoryWriteCapability"),
                "denial reason must name the unavailable capability, got {reason:?}"
            );
        }
        ActionGate::Admit => panic!("expected denial when the capability is unavailable"),
    }
}

#[test]
fn store_claim_gate_is_deterministic() {
    // Same input → identical verdict (P12 pure function over given structure).
    let f = facts_of(&store_claim_triad_nq(&["wellFormedClaim"]));
    let mut caps = BTreeSet::new();
    caps.insert(format!("{W}#memoryWriteCapability"));
    let a = gate_action(
        &f,
        &format!("{W}#storeClaim"),
        &format!("{W}#state0"),
        &caps,
    );
    let b = gate_action(
        &f,
        &format!("{W}#storeClaim"),
        &format!("{W}#state0"),
        &caps,
    );
    assert_eq!(a, b);
    assert_eq!(a, ActionGate::Admit);
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

// ── satisfiedBy ⟷ GoalEvaluation dual-authority bridge ──────────────────────────

/// Shorthand for a `gmeow:` IRI in N-Quads angle-bracket form.
fn g(local: &str) -> String {
    format!("<https://blackcatinformatics.ca/gmeow/{local}>")
}

/// Build a reified GoalEvaluation in N-Quads under the given vantage with the given
/// satisfaction + goal-evaluation status.
fn eval_nq(
    eval: &str,
    goal: &str,
    situation: &str,
    vantage: &str,
    sat_status: &str,
    eval_status: &str,
) -> String {
    let e = format!("<{W}#{eval}>");
    format!(
        "{e} {ty} {ge} <{W}> .\n\
         {e} {eg} <{W}#{goal}> <{W}> .\n\
         {e} {ea} <{W}#{situation}> <{W}> .\n\
         {e} {ev} <{W}#{vantage}> <{W}> .\n\
         {e} {ss} {sat} <{W}> .\n\
         {e} {gs} {est} <{W}> .\n",
        ty = rdf_type_tok(),
        ge = l("GoalEvaluation"),
        eg = l("evaluatesGoal"),
        ea = l("evaluatedAgainst"),
        ev = l("evaluationEvaluator"),
        ss = l("satisfactionStatus"),
        sat = l(sat_status),
        gs = l("goalEvaluationStatus"),
        est = l(eval_status),
    )
}

/// The rdf:type predicate as an N-Quads token.
fn rdf_type_tok() -> String {
    "<http://www.w3.org/1999/02/22-rdf-syntax-ns#type>".to_string()
}

#[test]
fn forward_satisfied_completed_emits_one_vantage_indexed_edge() {
    let nq = eval_nq(
        "ev1",
        "goalOrbit",
        "sitStable",
        "vantageMC",
        "Satisfied",
        "GoalEvaluationCompleted",
    );
    let f = facts_of(&nq);
    let out = bridge_generate_satisfied_by(&f, W).unwrap();
    // Exactly one flat satisfiedBy edge.
    let flat: Vec<&TeleologyQuad> = out
        .iter()
        .filter(|q| q.predicate == gmeow(SATISFIED_BY))
        .collect();
    assert_eq!(
        flat.len(),
        1,
        "one satisfied+completed eval → one flat edge"
    );
    assert_eq!(flat[0].subject, format!("{W}#goalOrbit"));
    assert_eq!(flat[0].object, n3(&format!("{W}#sitStable")));
    // The edge is vantage-indexed: a reified statement carries gmeow:accordingTo vantage.
    let acc: Vec<&TeleologyQuad> = out
        .iter()
        .filter(|q| q.predicate == gmeow(ACCORDING_TO))
        .collect();
    assert_eq!(acc.len(), 1, "the edge must carry exactly one vantage");
    assert_eq!(acc[0].object, n3(&format!("{W}#vantageMC")));
    // Provenance is content-addressed under the teleology rule.
    for q in &out {
        assert_eq!(q.rule_iri, TELEOLOGY_RULE_IRI);
        assert!(q
            .derivation_id
            .starts_with("https://blackcatinformatics.ca/gmeow/derivation/"));
    }
}

#[test]
fn forward_undetermined_or_partial_emits_no_edge() {
    // Satisfied but UNDETERMINED → no edge (not conclusive).
    let nq_undet = eval_nq(
        "ev1",
        "goalOrbit",
        "sitStable",
        "vantageMC",
        "Satisfied",
        "GoalEvaluationUndetermined",
    );
    let out = bridge_generate_satisfied_by(&facts_of(&nq_undet), W).unwrap();
    assert!(
        out.iter().all(|q| q.predicate != gmeow(SATISFIED_BY)),
        "satisfied-but-undetermined must NOT generate a satisfiedBy edge"
    );
    // PartiallySatisfied + completed → no edge (not Satisfied).
    let nq_partial = eval_nq(
        "ev1",
        "goalOrbit",
        "sitStable",
        "vantageMC",
        "PartiallySatisfied",
        "GoalEvaluationCompleted",
    );
    let out = bridge_generate_satisfied_by(&facts_of(&nq_partial), W).unwrap();
    assert!(
        out.iter().all(|q| q.predicate != gmeow(SATISFIED_BY)),
        "partial satisfaction must NOT generate a satisfiedBy edge"
    );
}

#[test]
fn reverse_authored_edge_with_no_backing_mints_default_eval() {
    // Authored flat edge with an explicit vantage but no reified evaluation backing it.
    let stmt = satisfied_by_reifier(&format!("{W}#goalOrbit"), &format!("{W}#sitStable")).unwrap();
    let nq = format!(
        "<{W}#goalOrbit> {sb} <{W}#sitStable> <{W}> .\n\
         <{stmt}> {acc} <{W}#vantageMC> <{W}> .\n",
        sb = g("satisfiedBy"),
        acc = g("accordingTo"),
    );
    let f = facts_of(&nq);
    let out = bridge_expand_authored_satisfied_by(&f, W).unwrap();
    // Exactly one default GoalEvaluation, attributed to the asserting vantage.
    let typed: Vec<&TeleologyQuad> = out
        .iter()
        .filter(|q| q.predicate == RDF_TYPE && q.object == n3(&logic("GoalEvaluation")))
        .collect();
    assert_eq!(typed.len(), 1, "one default evaluation minted");
    let eval_iri = &typed[0].subject;
    assert!(out.iter().any(|q| q.subject == *eval_iri
        && q.predicate == logic("satisfactionStatus")
        && q.object == n3(&logic("Satisfied"))));
    assert!(out.iter().any(|q| q.subject == *eval_iri
        && q.predicate == logic("goalEvaluationStatus")
        && q.object == n3(&logic("GoalEvaluationCompleted"))));
    assert!(out.iter().any(|q| q.subject == *eval_iri
        && q.predicate == logic("evaluationEvaluator")
        && q.object == n3(&format!("{W}#vantageMC"))));
    assert!(out.iter().any(|q| q.subject == *eval_iri
        && q.predicate == logic("evaluatesGoal")
        && q.object == n3(&format!("{W}#goalOrbit"))));
    assert!(out.iter().any(|q| q.subject == *eval_iri
        && q.predicate == logic("evaluatedAgainst")
        && q.object == n3(&format!("{W}#sitStable"))));

    // Idempotent: re-running yields the SAME content-addressed eval node id + quads.
    let out2 = bridge_expand_authored_satisfied_by(&facts_of(&nq), W).unwrap();
    assert_eq!(out, out2, "reverse bridge must be idempotent/deterministic");
}

#[test]
fn reverse_authored_edge_no_vantage_uses_default_standpoint() {
    let nq = format!(
        "<{W}#goalOrbit> {sb} <{W}#sitStable> <{W}> .\n",
        sb = g("satisfiedBy"),
    );
    let out = bridge_expand_authored_satisfied_by(&facts_of(&nq), W).unwrap();
    assert!(
        out.iter()
            .any(|q| q.predicate == logic("evaluationEvaluator")
                && q.object == n3(&gmeow(UNSPECIFIED_STANDPOINT))),
        "an authored edge with no accordingTo defaults to gmeow:unspecifiedStandpoint"
    );
}

#[test]
fn reverse_authored_edge_already_backed_is_not_re_expanded() {
    // Authored edge AND a satisfied+completed evaluation already back it under the same
    // vantage — the reverse direction must not mint a duplicate default.
    let mut nq = format!(
        "<{W}#goalOrbit> {sb} <{W}#sitStable> <{W}> .\n",
        sb = g("satisfiedBy"),
    );
    let stmt = satisfied_by_reifier(&format!("{W}#goalOrbit"), &format!("{W}#sitStable")).unwrap();
    nq.push_str(&format!(
        "<{stmt}> {acc} <{W}#vantageMC> <{W}> .\n",
        acc = g("accordingTo"),
    ));
    nq.push_str(&eval_nq(
        "ev1",
        "goalOrbit",
        "sitStable",
        "vantageMC",
        "Satisfied",
        "GoalEvaluationCompleted",
    ));
    let out = bridge_expand_authored_satisfied_by(&facts_of(&nq), W).unwrap();
    assert!(
        out.is_empty(),
        "an edge already backed under its vantage must not be re-expanded: {out:?}"
    );
}

#[test]
fn reverse_authored_edge_two_vantages_expands_to_two_edges() {
    // A single flat gmeow:satisfiedBy statement whose reifier carries TWO gmeow:accordingTo
    // vantages must yield TWO independent AuthoredEdge entries — one per co-agreeing evaluator.
    let stmt = satisfied_by_reifier(&format!("{W}#goalOrbit"), &format!("{W}#sitStable")).unwrap();
    let nq = format!(
        "<{W}#goalOrbit> {sb} <{W}#sitStable> <{W}> .
         <{stmt}> {acc} <{W}#vantageAlpha> <{W}> .
         <{stmt}> {acc} <{W}#vantageBeta> <{W}> .
",
        sb = g("satisfiedBy"),
        acc = g("accordingTo"),
    );
    let out = bridge_expand_authored_satisfied_by(&facts_of(&nq), W).unwrap();
    // Two GoalEvaluation nodes must be minted, one per vantage.
    let typed: Vec<&TeleologyQuad> = out
        .iter()
        .filter(|q| q.predicate == RDF_TYPE && q.object == n3(&logic("GoalEvaluation")))
        .collect();
    assert_eq!(
        typed.len(),
        2,
        "two evaluations must be minted for two co-agreeing vantages"
    );
    // Collect the two eval IRIs.
    let mut eval_iris: Vec<String> = typed.iter().map(|q| q.subject.clone()).collect();
    eval_iris.sort();
    eval_iris.dedup();
    assert_eq!(
        eval_iris.len(),
        2,
        "the two minted evaluations must have distinct IRIs"
    );
    // Each eval node is attributed to its own vantage.
    for eval_iri in &eval_iris {
        let evaluators: Vec<String> = out
            .iter()
            .filter(|q| q.subject == *eval_iri && q.predicate == logic("evaluationEvaluator"))
            .map(|q| q.object.clone())
            .collect();
        assert_eq!(
            evaluators.len(),
            1,
            "each eval must have exactly one evaluationEvaluator"
        );
    }
    // The vantages are both present across the two evaluations.
    let all_evaluators: Vec<String> = out
        .iter()
        .filter(|q| q.predicate == logic("evaluationEvaluator"))
        .map(|q| q.object.clone())
        .collect();
    assert!(
        all_evaluators.contains(&n3(&format!("{W}#vantageAlpha"))),
        "vantageAlpha must be present"
    );
    assert!(
        all_evaluators.contains(&n3(&format!("{W}#vantageBeta"))),
        "vantageBeta must be present"
    );
    // Idempotent.
    let out2 = bridge_expand_authored_satisfied_by(&facts_of(&nq), W).unwrap();
    assert_eq!(out, out2, "multi-vantage reverse bridge must be idempotent");
}

#[test]
fn reverse_authored_edge_zero_vantage_uses_default_standpoint_single_edge() {
    // Zero gmeow:accordingTo on the reifier → exactly ONE edge with gmeow:unspecifiedStandpoint.
    let nq = format!(
        "<{W}#goalOrbit> {sb} <{W}#sitStable> <{W}> .
",
        sb = g("satisfiedBy"),
    );
    let out = bridge_expand_authored_satisfied_by(&facts_of(&nq), W).unwrap();
    let typed: Vec<&TeleologyQuad> = out
        .iter()
        .filter(|q| q.predicate == RDF_TYPE && q.object == n3(&logic("GoalEvaluation")))
        .collect();
    assert_eq!(
        typed.len(),
        1,
        "zero-vantage case must yield exactly one default evaluation"
    );
    assert!(
        out.iter()
            .any(|q| q.predicate == logic("evaluationEvaluator")
                && q.object == n3(&gmeow(UNSPECIFIED_STANDPOINT))),
        "the single default evaluation must be attributed to gmeow:unspecifiedStandpoint"
    );
}

#[test]
fn contested_evaluations_forward_generates_one_edge_and_retains_both() {
    // Two vantages on the same goal+situation: one Satisfied+completed, one Violated.
    let mut nq = eval_nq(
        "evSat",
        "goalOrbit",
        "sitStable",
        "vantageMC",
        "Satisfied",
        "GoalEvaluationCompleted",
    );
    nq.push_str(&eval_nq(
        "evViol",
        "goalOrbit",
        "sitStable",
        "vantageAudit",
        "Violated",
        "GoalEvaluationCompleted",
    ));
    let f = facts_of(&nq);
    let out = bridge_generate_satisfied_by(&f, W).unwrap();
    // EXACTLY one flat edge — only the satisfied vantage generates one; NO global verdict.
    let flat: Vec<&TeleologyQuad> = out
        .iter()
        .filter(|q| q.predicate == gmeow(SATISFIED_BY))
        .collect();
    assert_eq!(
        flat.len(),
        1,
        "only the satisfied vantage generates an edge"
    );
    // The edge is attributed to the SATISFIED vantage, not a merged/global one.
    let acc: Vec<&TeleologyQuad> = out
        .iter()
        .filter(|q| q.predicate == gmeow(ACCORDING_TO))
        .collect();
    assert_eq!(acc.len(), 1);
    assert_eq!(acc[0].object, n3(&format!("{W}#vantageMC")));
    // The dissenting (Violated) evaluation is in the INPUT and the bridge neither reads
    // it as a generator nor deletes it — the input facts still carry it untouched.
    assert!(
        f.triples.iter().any(|t| t.subject.ends_with("#evViol")
            && t.predicate == logic("satisfactionStatus")
            && t.object_iri.as_deref() == Some(logic("Violated").as_str())),
        "the dissenting evaluation must remain present and untouched"
    );
}

#[test]
fn round_trip_forward_of_reverse_is_stable() {
    // Author a flat edge → reverse expands a default eval → forward over the union
    // regenerates the SAME flat edge (flat and reified agree per-vantage).
    let nq = format!(
        "<{W}#goalOrbit> {sb} <{W}#sitStable> <{W}> .\n",
        sb = g("satisfiedBy"),
    );
    let f = facts_of(&nq);
    let reverse = bridge_expand_authored_satisfied_by(&f, W).unwrap();
    let extended = f.extended_with(&reverse);
    let forward = bridge_generate_satisfied_by(&extended, W).unwrap();
    // The regenerated flat edge matches the authored one, under the default vantage.
    let flat: Vec<&TeleologyQuad> = forward
        .iter()
        .filter(|q| q.predicate == gmeow(SATISFIED_BY))
        .collect();
    assert_eq!(flat.len(), 1);
    assert_eq!(flat[0].subject, format!("{W}#goalOrbit"));
    assert_eq!(flat[0].object, n3(&format!("{W}#sitStable")));
    let acc = forward
        .iter()
        .find(|q| q.predicate == gmeow(ACCORDING_TO))
        .unwrap();
    assert_eq!(acc.object, n3(&gmeow(UNSPECIFIED_STANDPOINT)));
    // Running the combined bridge again over the union is stable (idempotent fold).
    let combined = bridge(&extended, W).unwrap();
    let combined2 = bridge(&extended, W).unwrap();
    assert_eq!(combined, combined2, "combined bridge must be deterministic");
}

#[test]
fn bridge_determinism_same_input_identical_quads_and_ids() {
    let mut nq = eval_nq(
        "ev1",
        "goalOrbit",
        "sitStable",
        "vantageMC",
        "Satisfied",
        "GoalEvaluationCompleted",
    );
    nq.push_str(&format!(
        "<{W}#goalOther> {sb} <{W}#sitOther> <{W}> .\n",
        sb = g("satisfiedBy"),
    ));
    let a = bridge(&facts_of(&nq), W).unwrap();
    let b = bridge(&facts_of(&nq), W).unwrap();
    assert_eq!(a, b, "same input → byte-identical quads + provenance ids");
}

#[test]
fn driver_post_pass_emits_bridged_satisfied_by_edge() {
    // The driver evaluates an atomic goal whose situation obtains → Satisfied/Completed,
    // and the bridge post-pass projects the flat gmeow:satisfiedBy edge from it.
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
    let out = evaluate_world_goals(&store_from(&nq), W).unwrap();
    // The driver emitted a Satisfied+Completed evaluation; the bridge post-pass projects
    // the flat edge AND its vantage index.
    assert!(
        out.iter().any(|q| q.predicate == gmeow(SATISFIED_BY)),
        "driver post-pass must emit the flat gmeow:satisfiedBy edge: {out:?}"
    );
    assert!(
        out.iter().any(|q| q.predicate == gmeow(ACCORDING_TO)),
        "the bridged edge must carry its vantage (gmeow:accordingTo)"
    );
}

// ── Whole-store materialize_teleology driver ────────────────────────────────────

#[test]
fn materialize_teleology_runs_all_families_and_is_deterministic() {
    // A world carrying: a goal-condition (family 1+5), a plan (family 2), a deontic
    // context + an ideal world (family 3), and a cyclic concurrent history (family 4).
    let mut nq = path_nq(&[&["sitA"]]);
    // Family 1: an atomic goal whose situation obtains → Satisfied/Completed → bridged.
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
    // Family 2: a plan over a 2-outcome schema where every outcome reaches the goal → Strong.
    nq.push_str(&format!(
        "<{W}#plan1> {ty} {plan} <{W}> .\n\
         <{W}#plan1> {ps} <{W}#schema> <{W}> .\n\
         <{W}#plan1> {pgs} <{W}#goalSit> <{W}> .\n",
        ty = rdf_type_tok(),
        plan = l("Plan"),
        ps = l("planSchema"),
        pgs = l("planGoalSituation"),
    ));
    nq.push_str(&two_outcome_schema_nq(true, true, false));
    // Family 3: a deontic context with an accessible ideal world satisfying the goal.
    let iw = "https://blackcatinformatics.ca/gmeow/examples/w1/idealX";
    nq.push_str(&format!(
        "<{W}#ctx> {ty} {dc} <{W}> .\n\
         <{W}#ctx> {pg} <{W}#goalNorm> <{W}> .\n\
         <{W}#ctx> {pgs} <{iw}#mustHold> <{W}> .\n\
         <{W}#ctx> {di} <{iw}> <{W}> .\n",
        ty = rdf_type_tok(),
        dc = l("DeonticContext"),
        pg = l("prescribesGoal"),
        pgs = l("prescribedGoalSituation"),
        di = l("deonticallyIdeal"),
    ));
    let ideal_nq = format!(
        "<{iw}#s0> {} <{iw}#mustHold> <{iw}> .\n",
        l("situationObtains")
    );
    // Family 4: a cyclic conflict history → SerializationAnomaly finding.
    nq.push_str(&format!(
        "<{W}#hist> {ty} {ch} <{W}> .\n\
         <{W}#t1> {pr} <{W}#t2> <{W}> .\n\
         <{W}#t2> {pr} <{W}#t1> <{W}> .\n",
        ty = rdf_type_tok(),
        ch = l("ConcurrentHistory"),
        pr = l("precedes"),
    ));

    let store = WorldStore::new();
    store.load_nquads(&nq).expect("base world");
    store.load_nquads(&ideal_nq).expect("ideal world");

    let out = materialize_teleology(&store).unwrap().0;

    // Family 1+5: a GoalEvaluation + a flat satisfiedBy edge for the satisfied goal.
    assert!(out
        .iter()
        .any(|q| q.predicate == RDF_TYPE && q.object == n3(&logic("GoalEvaluation"))));
    assert!(out.iter().any(|q| q.predicate == gmeow(SATISFIED_BY)));
    // Family 2: planSuccessMode = StrongPlanSuccess.
    assert!(out.iter().any(|q| q.subject == format!("{W}#plan1")
        && q.predicate == logic("planSuccessMode")
        && q.object == n3(&logic("StrongPlanSuccess"))));
    // Family 3: a deontic GoalEvaluation attributing the obligation to goalNorm, Satisfied.
    assert!(
        out.iter()
            .any(|q| q.predicate == logic("evaluatesGoal")
                && q.object == n3(&format!("{W}#goalNorm")))
    );
    // Family 4: a SerializationAnomaly finding (NOT a contradiction witness).
    assert!(out
        .iter()
        .any(|q| q.predicate == RDF_TYPE && q.object == n3(&logic("SerializationAnomaly"))));

    // Determinism: re-running over a fresh store yields byte-identical quads + ids.
    let store2 = WorldStore::new();
    store2.load_nquads(&nq).expect("base world");
    store2.load_nquads(&ideal_nq).expect("ideal world");
    let out2 = materialize_teleology(&store2).unwrap().0;
    assert_eq!(out, out2, "materialize_teleology must be deterministic");
}

#[test]
fn materialize_teleology_deontic_no_ideal_world_is_undetermined() {
    // A deontic context naming an ideal world that is NOT in the store → no accessible
    // ideal world → the deontic GoalEvaluation is Undetermined, never a vacuous Satisfied.
    let nq = format!(
        "<{W}#ctx> {ty} {dc} <{W}> .\n\
         <{W}#ctx> {pg} <{W}#goalNorm> <{W}> .\n\
         <{W}#ctx> {pgs} <{W}#mustHold> <{W}> .\n\
         <{W}#ctx> {di} <{W}#absentIdeal> <{W}> .\n",
        ty = rdf_type_tok(),
        dc = l("DeonticContext"),
        pg = l("prescribesGoal"),
        pgs = l("prescribedGoalSituation"),
        di = l("deonticallyIdeal"),
    );
    let store = WorldStore::new();
    store.load_nquads(&nq).expect("base world");
    let out = materialize_teleology(&store).unwrap().0;
    // The deontic evaluation carries goalEvaluationStatus = Undetermined.
    let is_undetermined = out.iter().any(|q| {
        q.predicate == logic("goalEvaluationStatus")
            && q.object == n3(&logic("GoalEvaluationUndetermined"))
    });
    assert!(
        is_undetermined,
        "no accessible ideal world must be Undetermined: {out:?}"
    );
    // And it asserts NO satisfactionStatus = Satisfied (no fabricated truth value).
    assert!(
        !out.iter().any(|q| q.subject.contains("/eval/")
            && q.predicate == logic("satisfactionStatus")
            && q.object == n3(&logic("Satisfied"))),
        "an undetermined obligation must not assert Satisfied"
    );
}

#[test]
fn materialize_teleology_serializable_history_emits_no_anomaly() {
    // An acyclic history → conflict-serializable → NO SerializationAnomaly finding.
    let nq = format!(
        "<{W}#hist> {ty} {ch} <{W}> .\n\
         <{W}#t1> {pr} <{W}#t2> <{W}> .\n\
         <{W}#t2> {pr} <{W}#t3> <{W}> .\n",
        ty = rdf_type_tok(),
        ch = l("ConcurrentHistory"),
        pr = l("precedes"),
    );
    let store = WorldStore::new();
    store.load_nquads(&nq).expect("world");
    let out = materialize_teleology(&store).unwrap().0;
    assert!(
        out.iter()
            .all(|q| q.object != n3(&logic("SerializationAnomaly"))),
        "a serializable history must emit no anomaly: {out:?}"
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

// ── Facet: invariant (breach denies the action) ──────────────────────────────────

#[test]
fn invariant_holding_admits_action() {
    // precondition + capability + invariant all hold → admit.
    let mut nq = path_nq(&[&["ready", "balanced"]]);
    nq.push_str(&format!(
        "<{W}#schema> {} <{W}#ready> <{W}> .\n",
        l("precondition")
    ));
    nq.push_str(&format!(
        "<{W}#schema> {} <{W}#balanced> <{W}> .\n",
        l("invariant")
    ));
    let f = facts_of(&nq);
    let caps = BTreeSet::new();
    let gate = gate_action(&f, &format!("{W}#schema"), &format!("{W}#state0"), &caps);
    assert_eq!(gate, ActionGate::Admit);
}

#[test]
fn invariant_breach_denies_action_with_reason_and_compensation() {
    // precondition holds but the invariant (balanced) is NOT obtaining → hard deny,
    // never a silent pass; the denial names the breached invariant and carries rollback.
    let mut nq = path_nq(&[&["ready"]]); // balanced does NOT obtain
    nq.push_str(&format!(
        "<{W}#schema> {} <{W}#ready> <{W}> .\n",
        l("precondition")
    ));
    nq.push_str(&format!(
        "<{W}#schema> {} <{W}#balanced> <{W}> .\n",
        l("invariant")
    ));
    nq.push_str(&format!(
        "<{W}#schema> {} <{W}#rollback> <{W}> .\n",
        l("compensation")
    ));
    let f = facts_of(&nq);
    let caps = BTreeSet::new();
    let gate = gate_action(&f, &format!("{W}#schema"), &format!("{W}#state0"), &caps);
    match gate {
        ActionGate::Deny {
            compensation,
            reason,
        } => {
            assert!(
                reason.contains("invariant") && reason.contains("balanced"),
                "denial must name the breached invariant, got {reason:?}"
            );
            assert_eq!(compensation, Some(format!("{W}#rollback")));
        }
        ActionGate::Admit => panic!("a breached invariant must deny, not pass"),
    }
}

// ── Facet: actionResource (exhaustion / absence gates the action) ────────────────

#[test]
fn resource_supplied_and_not_exhausted_admits() {
    let mut nq = path_nq(&[&["ready"]]);
    nq.push_str(&format!(
        "<{W}#schema> {} <{W}#ready> <{W}> .\n",
        l("precondition")
    ));
    nq.push_str(&format!(
        "<{W}#schema> {} <{W}#engineLock> <{W}> .\n",
        l("actionResource")
    ));
    nq.push_str(&format!(
        "<{W}#state0> {} <{W}#engineLock> <{W}> .\n",
        l("resourceSupply")
    ));
    let f = facts_of(&nq);
    let caps = BTreeSet::new();
    let gate = gate_action(&f, &format!("{W}#schema"), &format!("{W}#state0"), &caps);
    assert_eq!(gate, ActionGate::Admit);
}

#[test]
fn resource_not_supplied_gates_action() {
    let mut nq = path_nq(&[&["ready"]]); // state supplies NO resource
    nq.push_str(&format!(
        "<{W}#schema> {} <{W}#ready> <{W}> .\n",
        l("precondition")
    ));
    nq.push_str(&format!(
        "<{W}#schema> {} <{W}#engineLock> <{W}> .\n",
        l("actionResource")
    ));
    let f = facts_of(&nq);
    let caps = BTreeSet::new();
    let gate = gate_action(&f, &format!("{W}#schema"), &format!("{W}#state0"), &caps);
    match gate {
        ActionGate::Deny { reason, .. } => assert!(
            reason.contains("resource") && reason.contains("engineLock"),
            "denial must name the unavailable resource, got {reason:?}"
        ),
        ActionGate::Admit => panic!("an unsupplied resource must gate the action"),
    }
}

#[test]
fn resource_exhausted_gates_action() {
    // The resource is supplied but flagged exhausted → still gated.
    let mut nq = path_nq(&[&["ready"]]);
    nq.push_str(&format!(
        "<{W}#schema> {} <{W}#ready> <{W}> .\n",
        l("precondition")
    ));
    nq.push_str(&format!(
        "<{W}#schema> {} <{W}#engineLock> <{W}> .\n",
        l("actionResource")
    ));
    nq.push_str(&format!(
        "<{W}#state0> {} <{W}#engineLock> <{W}> .\n",
        l("resourceSupply")
    ));
    nq.push_str(&format!(
        "<{W}#engineLock> {} \"true\"^^<http://www.w3.org/2001/XMLSchema#boolean> <{W}> .\n",
        l("resourceExhausted")
    ));
    let f = facts_of(&nq);
    let caps = BTreeSet::new();
    let gate = gate_action(&f, &format!("{W}#schema"), &format!("{W}#state0"), &caps);
    assert!(
        matches!(gate, ActionGate::Deny { .. }),
        "an exhausted resource must gate the action"
    );
}

// ── Facet: effect (ins/del supersession, append-only) ────────────────────────────

#[test]
fn effect_computes_successor_support_as_supersession() {
    // Predecessor state0 obtains {sitX, sitY}; the effect ins sitZ and del sitY.
    // Successor support = {sitX, sitZ}; sitY is RETIRED (recorded as superseded), never
    // erased from the predecessor.
    let mut nq = path_nq(&[&["sitX", "sitY"]]);
    nq.push_str(&format!("<{W}#schema> {} <{W}#eff> <{W}> .\n", l("effect")));
    nq.push_str(&format!("<{W}#eff> {} <{W}#sitZ> <{W}> .\n", l("ins")));
    nq.push_str(&format!("<{W}#eff> {} <{W}#sitY> <{W}> .\n", l("del")));
    let f = facts_of(&nq);
    let support = apply_effect(&f, &format!("{W}#schema"), &format!("{W}#state0")).unwrap();
    assert!(support.asserted.contains(&format!("{W}#sitX")));
    assert!(support.asserted.contains(&format!("{W}#sitZ")));
    assert!(
        !support.asserted.contains(&format!("{W}#sitY")),
        "del removes sitY from the active successor support"
    );
    assert!(
        support.retired.contains(&format!("{W}#sitY")),
        "the retired support must be recorded, never silently dropped"
    );
}

#[test]
fn effect_application_emits_append_only_supersession_quartet() {
    let mut nq = path_nq(&[&["sitX", "sitY"]]);
    nq.push_str(&format!("<{W}#schema> {} <{W}#eff> <{W}> .\n", l("effect")));
    nq.push_str(&format!("<{W}#eff> {} <{W}#sitZ> <{W}> .\n", l("ins")));
    nq.push_str(&format!("<{W}#eff> {} <{W}#sitY> <{W}> .\n", l("del")));
    // The transaction step instantiating the schema, from state0 to state1.
    nq.push_str(&format!(
        "<{W}#step> {ty} {ts} <{W}> .\n\
         <{W}#step> {is} <{W}#schema> <{W}> .\n\
         <{W}#step> {tf} <{W}#state0> <{W}> .\n\
         <{W}#step> {tt} <{W}#state1> <{W}> .\n",
        ty = rdf_type_tok(),
        ts = l("TransactionStep"),
        is = l("instantiatesSchema"),
        tf = l("transitionFromState"),
        tt = l("transitionToState"),
    ));
    let store = store_from(&nq);
    let out = materialize_teleology(&store).unwrap().0;
    // Successor state1 asserts sitX and sitZ.
    assert!(out.iter().any(|q| q.subject == format!("{W}#state1")
        && q.predicate == logic("situationObtains")
        && q.object == n3(&format!("{W}#sitZ"))));
    // sitY is superseded (append-only), carrying the full quartet.
    assert!(out
        .iter()
        .any(|q| q.subject == format!("{W}#sitY") && q.predicate == logic("supersededBy")));
    assert!(out.iter().any(|q| q.subject == format!("{W}#sitY")
        && q.predicate == logic("validUntilState")
        && q.object == n3(&format!("{W}#state1"))));
    assert!(out
        .iter()
        .any(|q| q.subject == format!("{W}#sitY") && q.predicate == logic("retiredByTransaction")));
    // The predecessor state0 STILL carries sitY (echoed) — del is not erasure.
    assert!(out.iter().any(|q| q.subject == format!("{W}#state0")
        && q.predicate == logic("situationObtains")
        && q.object == n3(&format!("{W}#sitY"))));
}

// ── Facet: observation (observation-conditioned policy) ──────────────────────────

#[test]
fn observation_conditioned_branch_is_selected_and_surfaced() {
    // An action schema reveals sitObserved; a policy branch guarded on sitObserved
    // invokes nextSchema. The driver selects the branch and surfaces the next schema.
    let mut nq = String::new();
    nq.push_str(&format!(
        "<{W}#senseSchema> {ty} {as_} <{W}> .\n\
         <{W}#senseSchema> {ob} <{W}#obs> <{W}> .\n\
         <{W}#obs> {rv} <{W}#sitObserved> <{W}> .\n",
        ty = rdf_type_tok(),
        as_ = l("ActionSchema"),
        ob = l("observation"),
        rv = l("reveals"),
    ));
    nq.push_str(&format!(
        "<{W}#policy> {pb} <{W}#branch> <{W}> .\n\
         <{W}#branch> {bo} <{W}#obs> <{W}> .\n\
         <{W}#branch> {bg} <{W}#sitObserved> <{W}> .\n\
         <{W}#branch> {ba} <{W}#nextSchema> <{W}> .\n",
        pb = l("planBranch"),
        bo = l("branchObservation"),
        bg = l("branchGuard"),
        ba = l("branchActionSchema"),
    ));
    let store = store_from(&nq);
    let out = materialize_teleology(&store).unwrap().0;
    // The observation's reveal is surfaced.
    assert!(out.iter().any(|q| q.subject == format!("{W}#obs")
        && q.predicate == logic("reveals")
        && q.object == n3(&format!("{W}#sitObserved"))));
    // The policy selects the matching branch and surfaces the next action schema.
    assert!(out.iter().any(|q| q.subject == format!("{W}#policy")
        && q.predicate == logic("selectedBranch")
        && q.object == n3(&format!("{W}#branch"))));
    assert!(out.iter().any(|q| q.subject == format!("{W}#policy")
        && q.predicate == logic("nextActionSchema")
        && q.object == n3(&format!("{W}#nextSchema"))));
}

#[test]
fn observation_branch_with_non_matching_guard_is_not_selected() {
    // The branch guard does NOT match what the observation reveals → no selection.
    let mut nq = String::new();
    nq.push_str(&format!(
        "<{W}#senseSchema> {ty} {as_} <{W}> .\n\
         <{W}#senseSchema> {ob} <{W}#obs> <{W}> .\n\
         <{W}#obs> {rv} <{W}#sitObserved> <{W}> .\n",
        ty = rdf_type_tok(),
        as_ = l("ActionSchema"),
        ob = l("observation"),
        rv = l("reveals"),
    ));
    nq.push_str(&format!(
        "<{W}#policy> {pb} <{W}#branch> <{W}> .\n\
         <{W}#branch> {bo} <{W}#obs> <{W}> .\n\
         <{W}#branch> {bg} <{W}#otherSit> <{W}> .\n\
         <{W}#branch> {ba} <{W}#nextSchema> <{W}> .\n",
        pb = l("planBranch"),
        bo = l("branchObservation"),
        bg = l("branchGuard"),
        ba = l("branchActionSchema"),
    ));
    let store = store_from(&nq);
    let out = materialize_teleology(&store).unwrap().0;
    assert!(
        out.iter().all(|q| q.predicate != logic("selectedBranch")),
        "a non-matching guard must not select the branch"
    );
}

#[test]
fn new_facet_families_are_deterministic() {
    // The four new facet families must be byte-deterministic (foundation contract).
    let mut nq = path_nq(&[&["sitX", "sitY"]]);
    nq.push_str(&format!("<{W}#schema> {} <{W}#eff> <{W}> .\n", l("effect")));
    nq.push_str(&format!("<{W}#eff> {} <{W}#sitZ> <{W}> .\n", l("ins")));
    nq.push_str(&format!("<{W}#eff> {} <{W}#sitY> <{W}> .\n", l("del")));
    nq.push_str(&format!(
        "<{W}#step> {ty} {ts} <{W}> .\n\
         <{W}#step> {is} <{W}#schema> <{W}> .\n\
         <{W}#step> {tf} <{W}#state0> <{W}> .\n\
         <{W}#step> {tt} <{W}#state1> <{W}> .\n",
        ty = rdf_type_tok(),
        ts = l("TransactionStep"),
        is = l("instantiatesSchema"),
        tf = l("transitionFromState"),
        tt = l("transitionToState"),
    ));
    let a = materialize_teleology(&store_from(&nq)).unwrap().0;
    let b = materialize_teleology(&store_from(&nq)).unwrap().0;
    assert_eq!(a, b, "new facet families must be deterministic");
}

#[test]
fn gate_probe_surfaces_invariant_breach_denial() {
    // A gate probe over a schema whose invariant is breached emits a GateDenied verdict.
    let mut nq = path_nq(&[&["ready"]]); // balanced does NOT obtain
    nq.push_str(&format!(
        "<{W}#schema> {} <{W}#ready> <{W}> .\n",
        l("precondition")
    ));
    nq.push_str(&format!(
        "<{W}#schema> {} <{W}#balanced> <{W}> .\n",
        l("invariant")
    ));
    nq.push_str(&format!(
        "<{W}#probe> {ty} {gp} <{W}> .\n\
         <{W}#probe> {ps} <{W}#schema> <{W}> .\n\
         <{W}#probe> {pst} <{W}#state0> <{W}> .\n",
        ty = rdf_type_tok(),
        gp = l("GateProbe"),
        ps = l("probesSchema"),
        pst = l("probesState"),
    ));
    let store = store_from(&nq);
    let out = materialize_teleology(&store).unwrap().0;
    assert!(out.iter().any(|q| q.subject == format!("{W}#probe")
        && q.predicate == logic("gateVerdict")
        && q.object == n3(&logic("GateDenied"))));
    assert!(
        out.iter()
            .any(|q| q.subject == format!("{W}#probe") && q.predicate == logic("gateDenialReason")),
        "a denied probe must surface the denial reason"
    );
}

// ── DAG-workflow certification (logic:DagWorkflowResource) ───────────────────────

/// N-Quads for `<W#plan>` run under the DAG-workflow contract, gathering one reified
/// control-flow edge per `(edge, from, to)` through `logic:planFlowEdge`.
fn dag_plan_nq(edges: &[(&str, &str, &str)]) -> String {
    let mut s = String::new();
    let plan = format!("<{W}#plan>");
    s.push_str(&format!(
        "{plan} {ty} {pl} <{W}> .\n",
        ty = rdf_type_tok(),
        pl = l("Plan")
    ));
    s.push_str(&format!(
        "{plan} {} <{W}#dagContract> <{W}> .\n",
        l("executedUnderContract")
    ));
    s.push_str(&format!(
        "<{W}#dagContract> {} {} <{W}> .\n",
        l("resourcePolicy"),
        l("DagWorkflowResource")
    ));
    for (edge, from, to) in edges {
        s.push_str(&format!(
            "{plan} {} <{W}#{edge}> <{W}> .\n",
            l("planFlowEdge")
        ));
        s.push_str(&format!(
            "<{W}#{edge}> {} <{W}#{from}> <{W}> .\n",
            l("flowFrom")
        ));
        s.push_str(&format!(
            "<{W}#{edge}> {} <{W}#{to}> <{W}> .\n",
            l("flowTo")
        ));
    }
    s
}

/// The IRI the plan's `logic:planCertification` points at, given the emitted quads.
fn dag_result_iri(out: &[TeleologyQuad]) -> String {
    let edge = out
        .iter()
        .find(|q| q.subject == format!("{W}#plan") && q.predicate == logic("planCertification"))
        .expect("plan links to its DAG certification result");
    edge.object
        .trim_start_matches('<')
        .trim_end_matches('>')
        .to_owned()
}

#[test]
fn dag_certification_acyclic_plan_is_complete_for_fragment() {
    // a -> b -> c : a DAG, the shape of the build pipeline.
    let nq = dag_plan_nq(&[("e1", "a", "b"), ("e2", "b", "c")]);
    let facts = facts_of(&nq);
    let (out, cyclic) = emit_dag_certification(&facts, W, &format!("{W}#plan")).unwrap();
    assert!(!cyclic, "an acyclic plan must not be unsupported");
    let result = dag_result_iri(&out);
    assert!(
        out.iter().any(|q| q.subject == result
            && q.predicate == logic("resultEvaluation")
            && q.object == n3(&logic("EvaluationCompleted"))),
        "an acyclic plan certifies EvaluationCompleted"
    );
    assert!(
        out.iter().any(|q| q.subject == result
            && q.predicate == logic("resultCompleteness")
            && q.object == n3(&logic("CompleteForFragment"))),
        "an acyclic plan is CompleteForFragment"
    );
    assert!(
        out.iter().all(|q| q.predicate != logic("dagCycleWitness")),
        "a certified plan carries no cycle witness"
    );
}

#[test]
fn dag_certification_cyclic_plan_is_unsupported_with_witness() {
    // probe -> recover -> probe (the retry back-edge) + probe -> commit (success).
    let nq = dag_plan_nq(&[
        ("e1", "probe", "recover"),
        ("e2", "recover", "probe"),
        ("e3", "probe", "commit"),
    ]);
    let facts = facts_of(&nq);
    let (out, cyclic) = emit_dag_certification(&facts, W, &format!("{W}#plan")).unwrap();
    assert!(cyclic, "a cyclic plan resolves to unsupported");
    let result = dag_result_iri(&out);
    assert!(
        out.iter().any(|q| q.subject == result
            && q.predicate == logic("resultEvaluation")
            && q.object == n3(&logic("EvaluationUnsupported"))),
        "a cyclic plan resolves to EvaluationUnsupported, never silently truncated"
    );
    // The witness names EXACTLY the cycle members (probe, recover) — the dropped back-edge.
    let witnesses: BTreeSet<String> = out
        .iter()
        .filter(|q| q.subject == result && q.predicate == logic("dagCycleWitness"))
        .map(|q| q.object.clone())
        .collect();
    assert_eq!(
        witnesses,
        BTreeSet::from([n3(&format!("{W}#probe")), n3(&format!("{W}#recover"))]),
        "the witness discloses the loop the DAG projection drops"
    );
}

#[test]
fn dag_certification_skips_plan_without_dag_contract() {
    // A plan with flow edges but NO DAG contract is not certified under the profile.
    let mut nq = String::new();
    let plan = format!("<{W}#plan>");
    nq.push_str(&format!(
        "{plan} {ty} {pl} <{W}> .\n",
        ty = rdf_type_tok(),
        pl = l("Plan")
    ));
    nq.push_str(&format!("{plan} {} <{W}#e1> <{W}> .\n", l("planFlowEdge")));
    nq.push_str(&format!("<{W}#e1> {} <{W}#a> <{W}> .\n", l("flowFrom")));
    nq.push_str(&format!("<{W}#e1> {} <{W}#b> <{W}> .\n", l("flowTo")));
    let facts = facts_of(&nq);
    let (out, cyclic) = emit_dag_certification(&facts, W, &format!("{W}#plan")).unwrap();
    assert!(
        out.is_empty() && !cyclic,
        "a plan with no DAG contract is not certified under the profile"
    );
}

#[test]
fn dag_certification_is_deterministic() {
    let nq = dag_plan_nq(&[("e1", "probe", "recover"), ("e2", "recover", "probe")]);
    let a = emit_dag_certification(&facts_of(&nq), W, &format!("{W}#plan")).unwrap();
    let b = emit_dag_certification(&facts_of(&nq), W, &format!("{W}#plan")).unwrap();
    assert_eq!(a, b, "DAG certification must be byte-deterministic");
}

// ── logic:evaluationTime vocabulary completeness ─────────────────────────────

/// `logic:evaluationTime` is the "time of judgment" axis of the GoalEvaluation
/// identifying tuple.  No code path currently carries a timestamp into evaluation
/// emission, so the property exists for vocabulary completeness only.  This test
/// proves two things:
///
/// 1. The property IRI is correctly named — `logic("evaluationTime")` returns the
///    canonical `https://blackcatinformatics.ca/logic/evaluationTime` IRI.
/// 2. Driver-emitted GoalEvaluation quads do NOT contain a spurious
///    `logic:evaluationTime` triple (absence is intentional: time-of-judgment is
///    unspecified in the default bridge path, not defaulted).
#[test]
fn evaluation_time_iri_and_absent_from_driver_emitted_eval() {
    // 1. IRI correctness.
    let expected_iri = "https://blackcatinformatics.ca/logic/evaluationTime";
    assert_eq!(
        logic("evaluationTime"),
        expected_iri,
        "logic:evaluationTime IRI must match the minted vocabulary surface"
    );

    // 2. No evaluationTime triple on a driver-emitted GoalEvaluation.
    // Build the simplest world that produces a GoalEvaluation: one atomic goal
    // satisfied by the only state in the path.
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
        "<{W}#goalA> {has_cond} <{W}#atomA> <{W}> .\n",
        has_cond = l("hasGoalCondition"),
    ));
    let store = store_from(&nq);
    let out = materialize_teleology(&store).unwrap().0;
    // At least one GoalEvaluation quad must have been emitted.
    assert!(
        out.iter().any(|q| q.predicate == logic("evaluatesGoal")),
        "expected at least one GoalEvaluation to be emitted"
    );
    // None of the emitted quads should carry evaluationTime.
    assert!(
        out.iter().all(|q| q.predicate != logic("evaluationTime")),
        "driver-emitted GoalEvaluation must NOT carry logic:evaluationTime \
         (time-of-judgment is unspecified, not defaulted)"
    );
}
