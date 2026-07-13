// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Content-hash graph-versioning keys for world caching.
//!
//! # Design authority
//!
//! Verbatim from `slices/grounding/logic/design/LOGIC-RUNTIME.md` §"Graph versioning and staleness":
//!
//! > A materialized world graph is keyed by `(source_graph_hash, rule_set_hash, profile_id,
//! > solver_version, budget_params)`.
//! >
//! > A cached counterfactual world is valid **only** for the exact tuple
//! > `(base_world_hash, antecedent_hash, rule_set_hash, entrenchment_hash, profile,
//! > solver_version)`.
//! >
//! > Any change to a component invalidates the cache entry and forces reconstruction.
//!
//! # Staleness / cache-invalidation contract
//!
//! Changing **any single component** of either key tuple changes the computed cache key.
//! This is enforced by the hashing discipline documented below and verified by the unit
//! tests in this module.
//!
//! # Hashing discipline
//!
//! Components are fed into BLAKE3 in a **fixed, documented order** using **length-prefixed
//! framing**: every component is preceded by its 8-byte little-endian length. This prevents
//! component-boundary collisions — i.e. the two different splits `("ab", "c")` vs `("a", "bc")`
//! produce distinct byte streams and therefore distinct keys. The domain tag `"materialized-world-key\0"`,
//! `"counterfactual-world-key\0"`, or `"hypothetical-run-key\0"` is fed first as a length-prefixed entry
//! so the key spaces never collide even if all component values happen to coincide.

/// Length-prefix a byte slice and feed it into a BLAKE3 hasher.
///
/// Encoding: 8-byte LE length followed by the raw bytes. This prevents component-boundary
/// collisions that would arise from naive concatenation.
fn feed(hasher: &mut blake3::Hasher, data: &[u8]) {
    hasher.update(&(data.len() as u64).to_le_bytes());
    hasher.update(data);
}

/// Return the lowercase hex string of a BLAKE3 digest.
fn to_hex(digest: blake3::Hash) -> String {
    digest.to_hex().to_string()
}

// ── Budget parameters ─────────────────────────────────────────────────────────────────────────────

/// Typed budget parameters carried in a materialized-world cache key.
///
/// All fields are ordered. Two `BudgetParams` values are equal iff every field matches,
/// and any differing field produces a different serialization (and therefore a different key).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BudgetParams {
    /// Maximum number of fixpoint iterations, or `None` for no limit.
    pub max_iterations: Option<u64>,
    /// Maximum number of derived quads across the whole materialization, or `None` for no limit.
    pub max_derived_quads: Option<u64>,
    /// Maximum wall-clock milliseconds for a single solver run, or `None` for no limit.
    pub timeout_ms: Option<u64>,
}

impl BudgetParams {
    /// Serialize to a deterministic byte string for hashing.
    ///
    /// Uses a fixed format: three 1-byte presence flags followed by each present
    /// value as 8-byte LE u64. Any change to any field produces a different byte string.
    fn to_bytes(&self) -> Vec<u8> {
        let mut v = Vec::with_capacity(27);
        for opt in [self.max_iterations, self.max_derived_quads, self.timeout_ms] {
            match opt {
                None => {
                    v.push(0u8);
                }
                Some(n) => {
                    v.push(1u8);
                    v.extend_from_slice(&n.to_le_bytes());
                }
            }
        }
        v
    }
}

// ── Materialized world key ────────────────────────────────────────────────────────────────────────

/// Input components for a **materialized world** cache key.
///
/// Field names and semantics are verbatim from LOGIC-RUNTIME.md §"Graph versioning and staleness":
///
/// ```text
/// (source_graph_hash, rule_set_hash, profile_id, solver_version, budget_params)
/// ```
///
/// `source_graph_hash` and `rule_set_hash` are BLAKE3 digests (32 raw bytes) of the
/// source named graph and the rule set respectively.  `profile_id` and `solver_version`
/// are opaque strings (IRI and semver string respectively).  `budget_params` captures
/// the execution limits in force during materialization.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MaterializedKeyInputs {
    /// BLAKE3 digest of the source named graph (the RDF graph that was materialized).
    pub source_graph_hash: [u8; 32],
    /// BLAKE3 digest of the rule set applied during materialization.
    pub rule_set_hash: [u8; 32],
    /// IRI identifying the semantic / decidability profile in force.
    pub profile_id: String,
    /// Semver string of the solver version used.
    pub solver_version: String,
    /// Execution-budget parameters that were active during materialization.
    pub budget_params: BudgetParams,
}

/// Compute the deterministic BLAKE3 cache key for a **materialized world**.
///
/// # Hashing order (fixed and documented)
///
/// 1. Domain tag: `"materialized-world-key\0"` (prevents collision with counterfactual space)
/// 2. `source_graph_hash`
/// 3. `rule_set_hash`
/// 4. `profile_id`
/// 5. `solver_version`
/// 6. `budget_params` serialized bytes
///
/// Every component is length-prefixed (8-byte LE u64) before its data bytes so
/// that distinct component splits yield distinct byte streams.
///
/// # Returns
///
/// Lowercase hex string of the 32-byte BLAKE3 digest.
pub fn materialized_world_key(inputs: &MaterializedKeyInputs) -> String {
    let mut h = blake3::Hasher::new();
    feed(&mut h, b"materialized-world-key\0");
    feed(&mut h, &inputs.source_graph_hash);
    feed(&mut h, &inputs.rule_set_hash);
    feed(&mut h, inputs.profile_id.as_bytes());
    feed(&mut h, inputs.solver_version.as_bytes());
    feed(&mut h, &inputs.budget_params.to_bytes());
    to_hex(h.finalize())
}

// ── Counterfactual world key ──────────────────────────────────────────────────────────────────────

/// Input components for a **counterfactual world** cache key.
///
/// Field names and semantics are verbatim from LOGIC-RUNTIME.md §"Graph versioning and staleness":
///
/// ```text
/// (base_world_hash, antecedent_hash, rule_set_hash, entrenchment_hash, profile, solver_version)
/// ```
///
/// `base_world_hash`, `antecedent_hash`, `rule_set_hash`, and `entrenchment_hash` are BLAKE3
/// digests (32 raw bytes).  `profile` is an IRI string identifying the semantic profile.
/// `solver_version` is a semver string.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CounterfactualKeyInputs {
    /// BLAKE3 digest of the base (prior) world graph from which the counterfactual departs.
    pub base_world_hash: [u8; 32],
    /// BLAKE3 digest of the antecedent (the set of hypothetical facts being injected).
    pub antecedent_hash: [u8; 32],
    /// BLAKE3 digest of the rule set applied to the counterfactual world.
    pub rule_set_hash: [u8; 32],
    /// BLAKE3 digest of the entrenchment ordering (belief-revision priorities).
    pub entrenchment_hash: [u8; 32],
    /// IRI identifying the semantic / decidability profile in force.
    pub profile: String,
    /// Semver string of the solver version used.
    pub solver_version: String,
}

/// Compute the deterministic BLAKE3 cache key for a **counterfactual world**.
///
/// # Hashing order (fixed and documented)
///
/// 1. Domain tag: `"counterfactual-world-key\0"` (prevents collision with materialized space)
/// 2. `base_world_hash`
/// 3. `antecedent_hash`
/// 4. `rule_set_hash`
/// 5. `entrenchment_hash`
/// 6. `profile`
/// 7. `solver_version`
///
/// Every component is length-prefixed (8-byte LE u64) before its data bytes so
/// that distinct component splits yield distinct byte streams.
///
/// # Returns
///
/// Lowercase hex string of the 32-byte BLAKE3 digest.
pub fn counterfactual_world_key(inputs: &CounterfactualKeyInputs) -> String {
    let mut h = blake3::Hasher::new();
    feed(&mut h, b"counterfactual-world-key\0");
    feed(&mut h, &inputs.base_world_hash);
    feed(&mut h, &inputs.antecedent_hash);
    feed(&mut h, &inputs.rule_set_hash);
    feed(&mut h, &inputs.entrenchment_hash);
    feed(&mut h, inputs.profile.as_bytes());
    feed(&mut h, inputs.solver_version.as_bytes());
    to_hex(h.finalize())
}

// ── Hypothetical run key ──────────────────────────────────────────────────────────────────────────

/// Input components for a **hypothetical (sandbox) transaction-run** key.
///
/// This is the content-addressed identity of a Transaction-Logic program executed under
/// `logic:HypotheticalExecution` — run to test whether it *would* succeed, with its effects
/// discarded rather than committed. It is the witness recorded as `logic:executedHypotheticallyAs`
/// on the resulting `logic:TransactionOutcome`: the sole standing trace of a run whose effect
/// substrate is intentionally never emitted.
///
/// It reuses the **same content-addressed keying discipline** as [`counterfactual_world_key`] — the
/// paradigm-neutral substrate the hypothetical and modal-possibility operators share — under its own
/// domain tag so the two remain separate typed operators whose key spaces never collide. It does
/// **not** reuse the counterfactual store/dispatch machinery: the transaction interpreter is
/// deliberately effect-free, and coupling it to a store would conflate the two operators.
///
/// The components are the transaction-run analogue of a counterfactual's `(base_world, antecedent)`:
/// the start-state support set the run departs from, the program that was run, the world it is scoped
/// to, and the solver version (a behavioral bump invalidates the recorded witness).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HypotheticalRunKeyInputs {
    /// BLAKE3 digest of the start-state situation support the hypothetical run departs from.
    pub start_state_hash: [u8; 32],
    /// BLAKE3 digest of the transaction program (its combinator tree and action schemas).
    pub program_hash: [u8; 32],
    /// IRI of the world / named graph the run is scoped to (salts the run's identity).
    pub world: String,
    /// Semver string of the solver version used.
    pub solver_version: String,
}

/// Compute the deterministic BLAKE3 key for a **hypothetical (sandbox) transaction run**.
///
/// # Hashing order (fixed and documented)
///
/// 1. Domain tag: `"hypothetical-run-key\0"` (prevents collision with the materialized and
///    counterfactual spaces)
/// 2. `start_state_hash`
/// 3. `program_hash`
/// 4. `world`
/// 5. `solver_version`
///
/// Every component is length-prefixed (8-byte LE u64) before its data bytes so that distinct
/// component splits yield distinct byte streams.
///
/// # Returns
///
/// Lowercase hex string of the 32-byte BLAKE3 digest.
pub fn hypothetical_run_key(inputs: &HypotheticalRunKeyInputs) -> String {
    let mut h = blake3::Hasher::new();
    feed(&mut h, b"hypothetical-run-key\0");
    feed(&mut h, &inputs.start_state_hash);
    feed(&mut h, &inputs.program_hash);
    feed(&mut h, inputs.world.as_bytes());
    feed(&mut h, inputs.solver_version.as_bytes());
    to_hex(h.finalize())
}

// ── Unit tests — AC#3: cache-invalidation ────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
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
}
