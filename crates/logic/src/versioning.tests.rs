// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

// ── helpers ──────────────────────────────────────────────────────────────────────────────────

fn hash_a() -> [u8; 32] {
    [0xAAu8; 32]
}

fn hash_b() -> [u8; 32] {
    [0xBBu8; 32]
}

fn baseline_mat() -> MaterializedKeyInputs {
    MaterializedKeyInputs {
        source_graph_hash: hash_a(),
        rule_set_hash: hash_b(),
        profile_id: "http://logic.gmeow.example/profile/MonotonicDatalog".to_owned(),
        solver_version: "0.1.0".to_owned(),
        budget_params: BudgetParams {
            max_iterations: Some(1000),
            max_derived_quads: Some(50_000),
            timeout_ms: Some(5000),
        },
    }
}

fn baseline_cf() -> CounterfactualKeyInputs {
    CounterfactualKeyInputs {
        base_world_hash: hash_a(),
        antecedent_hash: hash_b(),
        rule_set_hash: [0xCCu8; 32],
        entrenchment_hash: [0xDDu8; 32],
        profile: "http://logic.gmeow.example/profile/MonotonicDatalog".to_owned(),
        solver_version: "0.1.0".to_owned(),
    }
}

fn baseline_hypo() -> HypotheticalRunKeyInputs {
    HypotheticalRunKeyInputs {
        start_state_hash: hash_a(),
        program_hash: hash_b(),
        world: "https://blackcatinformatics.ca/gmeow/graph/imports".to_owned(),
        solver_version: "0.1.0".to_owned(),
    }
}

// ── Determinism ───────────────────────────────────────────────────────────────────────────────

#[test]
fn materialized_key_is_deterministic() {
    let k0 = materialized_world_key(&baseline_mat());
    let k1 = materialized_world_key(&baseline_mat());
    assert_eq!(k0, k1, "same inputs must produce identical keys");
}

#[test]
fn counterfactual_key_is_deterministic() {
    let k0 = counterfactual_world_key(&baseline_cf());
    let k1 = counterfactual_world_key(&baseline_cf());
    assert_eq!(k0, k1, "same inputs must produce identical keys");
}

#[test]
fn hypothetical_key_is_deterministic() {
    let k0 = hypothetical_run_key(&baseline_hypo());
    let k1 = hypothetical_run_key(&baseline_hypo());
    assert_eq!(k0, k1, "same inputs must produce identical keys");
}

// ── Materialized key: per-component invalidation ──────────────────────────────────────────────

#[test]
fn mat_key_changes_on_source_graph_hash() {
    let k0 = materialized_world_key(&baseline_mat());
    let mut inp = baseline_mat();
    inp.source_graph_hash = [0x01u8; 32];
    assert_ne!(
        materialized_world_key(&inp),
        k0,
        "source_graph_hash mutation must change key"
    );
}

#[test]
fn mat_key_changes_on_rule_set_hash() {
    let k0 = materialized_world_key(&baseline_mat());
    let mut inp = baseline_mat();
    inp.rule_set_hash = [0x02u8; 32];
    assert_ne!(
        materialized_world_key(&inp),
        k0,
        "rule_set_hash mutation must change key"
    );
}

#[test]
fn mat_key_changes_on_profile_id() {
    let k0 = materialized_world_key(&baseline_mat());
    let mut inp = baseline_mat();
    inp.profile_id = "http://logic.gmeow.example/profile/OtherProfile".to_owned();
    assert_ne!(
        materialized_world_key(&inp),
        k0,
        "profile_id mutation must change key"
    );
}

#[test]
fn mat_key_changes_on_solver_version() {
    let k0 = materialized_world_key(&baseline_mat());
    let mut inp = baseline_mat();
    inp.solver_version = "0.2.0".to_owned();
    assert_ne!(
        materialized_world_key(&inp),
        k0,
        "solver_version mutation must change key"
    );
}

#[test]
fn mat_key_changes_on_budget_max_iterations() {
    let k0 = materialized_world_key(&baseline_mat());
    let mut inp = baseline_mat();
    inp.budget_params.max_iterations = Some(9999);
    assert_ne!(
        materialized_world_key(&inp),
        k0,
        "budget_params.max_iterations mutation must change key"
    );
}

#[test]
fn mat_key_changes_on_budget_max_derived_quads() {
    let k0 = materialized_world_key(&baseline_mat());
    let mut inp = baseline_mat();
    inp.budget_params.max_derived_quads = Some(1);
    assert_ne!(
        materialized_world_key(&inp),
        k0,
        "budget_params.max_derived_quads mutation must change key"
    );
}

#[test]
fn mat_key_changes_on_budget_timeout_ms() {
    let k0 = materialized_world_key(&baseline_mat());
    let mut inp = baseline_mat();
    inp.budget_params.timeout_ms = Some(1);
    assert_ne!(
        materialized_world_key(&inp),
        k0,
        "budget_params.timeout_ms mutation must change key"
    );
}

#[test]
fn mat_key_changes_when_budget_limit_removed() {
    let k0 = materialized_world_key(&baseline_mat());
    let mut inp = baseline_mat();
    inp.budget_params.max_iterations = None;
    assert_ne!(
        materialized_world_key(&inp),
        k0,
        "removing a budget limit must change key"
    );
}

// ── Counterfactual key: per-component invalidation ────────────────────────────────────────────

#[test]
fn cf_key_changes_on_base_world_hash() {
    let k0 = counterfactual_world_key(&baseline_cf());
    let mut inp = baseline_cf();
    inp.base_world_hash = [0x01u8; 32];
    assert_ne!(
        counterfactual_world_key(&inp),
        k0,
        "base_world_hash mutation must change key"
    );
}

#[test]
fn cf_key_changes_on_antecedent_hash() {
    let k0 = counterfactual_world_key(&baseline_cf());
    let mut inp = baseline_cf();
    inp.antecedent_hash = [0x02u8; 32];
    assert_ne!(
        counterfactual_world_key(&inp),
        k0,
        "antecedent_hash mutation must change key"
    );
}

#[test]
fn cf_key_changes_on_rule_set_hash() {
    let k0 = counterfactual_world_key(&baseline_cf());
    let mut inp = baseline_cf();
    inp.rule_set_hash = [0x03u8; 32];
    assert_ne!(
        counterfactual_world_key(&inp),
        k0,
        "rule_set_hash mutation must change key"
    );
}

#[test]
fn cf_key_changes_on_entrenchment_hash() {
    let k0 = counterfactual_world_key(&baseline_cf());
    let mut inp = baseline_cf();
    inp.entrenchment_hash = [0x04u8; 32];
    assert_ne!(
        counterfactual_world_key(&inp),
        k0,
        "entrenchment_hash mutation must change key"
    );
}

#[test]
fn cf_key_changes_on_profile() {
    let k0 = counterfactual_world_key(&baseline_cf());
    let mut inp = baseline_cf();
    inp.profile = "http://logic.gmeow.example/profile/DifferentProfile".to_owned();
    assert_ne!(
        counterfactual_world_key(&inp),
        k0,
        "profile mutation must change key"
    );
}

#[test]
fn cf_key_changes_on_solver_version() {
    let k0 = counterfactual_world_key(&baseline_cf());
    let mut inp = baseline_cf();
    inp.solver_version = "1.0.0".to_owned();
    assert_ne!(
        counterfactual_world_key(&inp),
        k0,
        "solver_version mutation must change key"
    );
}

// ── Hypothetical key: per-component invalidation ──────────────────────────────────────────────

#[test]
fn hypo_key_changes_on_start_state_hash() {
    let k0 = hypothetical_run_key(&baseline_hypo());
    let mut inp = baseline_hypo();
    inp.start_state_hash = [0x01u8; 32];
    assert_ne!(
        hypothetical_run_key(&inp),
        k0,
        "start_state_hash mutation must change key"
    );
}

#[test]
fn hypo_key_changes_on_program_hash() {
    let k0 = hypothetical_run_key(&baseline_hypo());
    let mut inp = baseline_hypo();
    inp.program_hash = [0x02u8; 32];
    assert_ne!(
        hypothetical_run_key(&inp),
        k0,
        "program_hash mutation must change key"
    );
}

#[test]
fn hypo_key_changes_on_world() {
    let k0 = hypothetical_run_key(&baseline_hypo());
    let mut inp = baseline_hypo();
    inp.world = "https://blackcatinformatics.ca/gmeow/graph/other".to_owned();
    assert_ne!(
        hypothetical_run_key(&inp),
        k0,
        "world mutation must change key"
    );
}

#[test]
fn hypo_key_changes_on_solver_version() {
    let k0 = hypothetical_run_key(&baseline_hypo());
    let mut inp = baseline_hypo();
    inp.solver_version = "0.2.0".to_owned();
    assert_ne!(
        hypothetical_run_key(&inp),
        k0,
        "solver_version mutation must change key"
    );
}

// ── Domain separation: materialized vs counterfactual vs hypothetical ──────────────────────────

#[test]
fn mat_and_cf_keys_never_collide_on_equal_overlapping_components() {
    // Use deliberately matching values for components that appear in both key types.
    let shared_hash = [0x55u8; 32];
    let shared_profile = "http://logic.gmeow.example/profile/MonotonicDatalog".to_owned();
    let shared_solver = "0.1.0".to_owned();

    let mat = MaterializedKeyInputs {
        source_graph_hash: shared_hash,
        rule_set_hash: shared_hash,
        profile_id: shared_profile.clone(),
        solver_version: shared_solver.clone(),
        budget_params: BudgetParams {
            max_iterations: None,
            max_derived_quads: None,
            timeout_ms: None,
        },
    };
    let cf = CounterfactualKeyInputs {
        base_world_hash: shared_hash,
        antecedent_hash: shared_hash,
        rule_set_hash: shared_hash,
        entrenchment_hash: shared_hash,
        profile: shared_profile.clone(),
        solver_version: shared_solver.clone(),
    };
    let hypo = HypotheticalRunKeyInputs {
        start_state_hash: shared_hash,
        program_hash: shared_hash,
        world: shared_profile,
        solver_version: shared_solver,
    };
    let mk = materialized_world_key(&mat);
    let ck = counterfactual_world_key(&cf);
    let hk = hypothetical_run_key(&hypo);
    assert_ne!(
        mk, ck,
        "materialized and counterfactual keys must never collide"
    );
    assert_ne!(
        mk, hk,
        "materialized and hypothetical keys must never collide"
    );
    assert_ne!(
        ck, hk,
        "counterfactual and hypothetical keys must never collide"
    );
}

// ── Separator / length-prefix discipline ──────────────────────────────────────────────────────

/// Proves that two different component splits that would collide under naive string
/// concatenation produce DIFFERENT keys due to length-prefixed framing.
///
/// Without length-prefixing, feeding ("ab", "cd") and ("a", "bcd") into a BLAKE3 hasher
/// via plain `.update()` calls would produce the same hash (because the byte stream is
/// identical: `abcd`). With length-prefixed framing, the streams are:
///
/// ```text
/// ("ab","cd")  → [2,0,0,0,0,0,0,0] 'a' 'b'  [2,0,0,0,0,0,0,0] 'c' 'd'
/// ("a","bcd")  → [1,0,0,0,0,0,0,0] 'a'       [3,0,0,0,0,0,0,0] 'b' 'c' 'd'
/// ```
///
/// These are distinct byte streams and therefore produce distinct BLAKE3 digests.
#[test]
fn length_prefix_prevents_boundary_collision() {
    // Construct two MaterializedKeyInputs whose profile_id+solver_version pair have the
    // same concatenation but different splits:
    //   split A: profile_id = "XY", solver_version = "Z"   → concat = "XYZ"
    //   split B: profile_id = "X",  solver_version = "YZ"  → concat = "XYZ"
    let mut inp_a = baseline_mat();
    inp_a.profile_id = "XY".to_owned();
    inp_a.solver_version = "Z".to_owned();

    let mut inp_b = baseline_mat();
    inp_b.profile_id = "X".to_owned();
    inp_b.solver_version = "YZ".to_owned();

    assert_ne!(
        materialized_world_key(&inp_a),
        materialized_world_key(&inp_b),
        "length-prefix framing must prevent boundary-collision between (XY,Z) and (X,YZ)"
    );
}
