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

#[path = "versioning.tests.rs"]
#[cfg(test)]
mod tests;
