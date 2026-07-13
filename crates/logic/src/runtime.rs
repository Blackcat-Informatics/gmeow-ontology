// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The supported runtime query surface of `gmeow-logic`.
//!
//! (The curated re-export cluster and the full stability contract land alongside
//! this module's runtime pin.)

use std::sync::OnceLock;

use gmeow_logic_compile::ir::{LOGIC_NAMESPACE, SemanticProfileId};

use crate::query_ir::Budget;
use crate::result::EngineId;

const GMEOW_NS: &str = "https://blackcatinformatics.ca/gmeow/";
const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";

/// Every semantic profile the runtime dispatch surface recognizes, in a fixed
/// order. The `profile_manifest_covers_every_semantic_profile` test pins this list
/// against [`SemanticProfileId`] so a new profile cannot silently fall out of the
/// capability manifest.
const RUNTIME_PROFILES: [SemanticProfileId; 6] = [
    SemanticProfileId::PositiveHorn,
    SemanticProfileId::StratifiedNaf,
    SemanticProfileId::WellFounded,
    SemanticProfileId::StableModel,
    SemanticProfileId::ProceduralProlog,
    SemanticProfileId::Probabilistic,
];

/// The backward-dispatch source files whose bytes define what an [`AnswerSet`] a
/// [`dispatch_query`] call decides. A change to any of them changes
/// [`EngineContract::current`]'s `backward_source_hash`, so a pinned consumer detects
/// it. The `physical/` entries are guarded against drift by
/// `physical_coverage_matches_source_tree` — a new/renamed `physical` file breaks that
/// test loudly rather than silently dropping out of the contract.
///
/// [`AnswerSet`]: crate::query_ir::AnswerSet
/// [`dispatch_query`]: crate::dispatch::dispatch_query
const BACKWARD_SOURCE: &[(&str, &str)] = &[
    ("dispatch.rs", include_str!("dispatch.rs")),
    ("profile_gate.rs", include_str!("profile_gate.rs")),
    ("query_ir.rs", include_str!("query_ir.rs")),
    ("seam.rs", include_str!("seam.rs")),
    ("physical/arena.rs", include_str!("physical/arena.rs")),
    (
        "physical/binding_pattern.rs",
        include_str!("physical/binding_pattern.rs"),
    ),
    ("physical/bitset.rs", include_str!("physical/bitset.rs")),
    (
        "physical/builtin_eval.rs",
        include_str!("physical/builtin_eval.rs"),
    ),
    ("physical/chase.rs", include_str!("physical/chase.rs")),
    ("physical/cursor.rs", include_str!("physical/cursor.rs")),
    ("physical/generic.rs", include_str!("physical/generic.rs")),
    ("physical/id.rs", include_str!("physical/id.rs")),
    (
        "physical/incremental.rs",
        include_str!("physical/incremental.rs"),
    ),
    (
        "physical/incremental_grounding.rs",
        include_str!("physical/incremental_grounding.rs"),
    ),
    ("physical/magic.rs", include_str!("physical/magic.rs")),
    (
        "physical/magic_generic.rs",
        include_str!("physical/magic_generic.rs"),
    ),
    ("physical/mod.rs", include_str!("physical/mod.rs")),
    ("physical/parity.rs", include_str!("physical/parity.rs")),
    ("physical/plan.rs", include_str!("physical/plan.rs")),
    (
        "physical/seminaive.rs",
        include_str!("physical/seminaive.rs"),
    ),
    ("physical/store.rs", include_str!("physical/store.rs")),
];

/// Frame `value` under `tag` into `hasher` with a domain tag and length prefixes, so
/// no component boundary can collide with another (`("ab","c")` and `("a","bc")` hash
/// distinctly). Mirrors the framed-BLAKE3 discipline in `dispatch::query_contract_hash`.
fn frame(hasher: &mut blake3::Hasher, tag: &[u8], value: &[u8]) {
    hasher.update(&(tag.len() as u64).to_le_bytes());
    hasher.update(tag);
    hasher.update(&(value.len() as u64).to_le_bytes());
    hasher.update(value);
}

/// The content digest over the whole backward-dispatch source surface.
fn backward_source_hash() -> String {
    let mut hasher = blake3::Hasher::new();
    frame(&mut hasher, b"domain", b"gmeow-logic-backward-source-v1");
    // BACKWARD_SOURCE is authored in a fixed order; frame each (name, content) pair so a
    // rename or a content change both move the digest.
    for (name, content) in BACKWARD_SOURCE {
        frame(&mut hasher, b"file", name.as_bytes());
        frame(&mut hasher, b"body", content.as_bytes());
    }
    hasher.finalize().to_hex().to_string()
}

/// A single supported profile paired with its decidability-class guarantee — the unit
/// of the runtime capability manifest, so a consumer negotiates capability instead of
/// discovering an unsupported profile as a runtime `Err`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProfileCapability {
    /// The full profile IRI (e.g. `logic:StratifiedNAFProfile`).
    pub profile: String,
    /// The decidability class the engine guarantees for that profile.
    pub decidability_class: String,
}

/// A self-describing, content-addressed identity of the `gmeow-logic` runtime engine
/// contract — the runtime pin a signed-ledger consumer records and refuses against.
///
/// It mirrors the repo's own [`crate::certificate::CoherenceOutcome`] idiom (a
/// content-addressed, `to_nquads`-projectable evidence object): one descriptor covers
/// the WHOLE engine — the forward EL/DL/RL chase ([`forward_contract_hash`]) and the
/// backward goal-resolution surface ([`backward_source_hash`]) — plus the engine
/// identity and the per-profile capability manifest. A consumer fetches
/// [`EngineContract::current`] at load, records [`descriptor_hash`] (or the
/// [`to_nquads`] projection) beside its ledger, and later calls [`assert_matches`] to
/// refuse an answer minted under a drifted engine.
///
/// [`forward_contract_hash`]: EngineContract::forward_contract_hash
/// [`backward_source_hash`]: EngineContract::backward_source_hash
/// [`descriptor_hash`]: EngineContract::descriptor_hash
/// [`to_nquads`]: EngineContract::to_nquads
/// [`assert_matches`]: EngineContract::assert_matches
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EngineContract {
    /// The engine name + version that decides answers (from [`EngineId::native`]).
    pub engine: EngineId,
    /// Digest over the backward goal-resolution source surface (`dispatch`, the profile
    /// gates, `query_ir`, the `seam` snapshot, and the whole `physical` engine).
    pub backward_source_hash: String,
    /// The forward reasoning-contract identity ([`crate::reason::native_contract_hash`]),
    /// folded in so ONE descriptor pins both engine directions.
    pub forward_contract_hash: String,
    /// The supported profiles and their decidability-class guarantees.
    pub profiles: Vec<ProfileCapability>,
    /// Framed-BLAKE3 content address over every field above — the value a consumer pins.
    pub descriptor_hash: String,
}

impl EngineContract {
    /// The engine contract this compiled binary embodies (memoized).
    pub fn current() -> Self {
        static CONTRACT: OnceLock<EngineContract> = OnceLock::new();
        CONTRACT.get_or_init(Self::compute).clone()
    }

    fn compute() -> Self {
        let engine = EngineId::native();
        let backward_source_hash = backward_source_hash();
        let forward_contract_hash = crate::reason::native_contract_hash();
        let profiles: Vec<ProfileCapability> = RUNTIME_PROFILES
            .iter()
            .map(|p| ProfileCapability {
                profile: p.iri(),
                decidability_class: crate::certify::decidability_class(p.as_str()).to_owned(),
            })
            .collect();

        let mut hasher = blake3::Hasher::new();
        frame(&mut hasher, b"domain", b"gmeow-logic-engine-contract-v1");
        frame(&mut hasher, b"engine-name", engine.name.as_bytes());
        frame(&mut hasher, b"engine-version", engine.version.as_bytes());
        frame(
            &mut hasher,
            b"backward-source",
            backward_source_hash.as_bytes(),
        );
        frame(
            &mut hasher,
            b"forward-contract",
            forward_contract_hash.as_bytes(),
        );
        for cap in &profiles {
            frame(&mut hasher, b"profile", cap.profile.as_bytes());
            frame(
                &mut hasher,
                b"decidability",
                cap.decidability_class.as_bytes(),
            );
        }
        let descriptor_hash = hasher.finalize().to_hex().to_string();

        Self {
            engine,
            backward_source_hash,
            forward_contract_hash,
            profiles,
            descriptor_hash,
        }
    }

    /// The per-query contract hash — the identity of the semantics/resource inputs a
    /// single [`dispatch_query`](crate::dispatch::dispatch_query) call runs under
    /// (`profile` + `budget`). Single-sourced from the dispatch engine's own helper, so
    /// the value a consumer reproduces on its side is byte-identical to the one the
    /// engine keyed the physical plan under — there is no second copy to drift.
    ///
    /// Distinct from [`descriptor_hash`](Self::descriptor_hash): the descriptor pins the
    /// engine *source*; this pins the *invocation*. A consumer recording "answer X minted
    /// under contract Y" needs both, since two queries under different `profile`/`budget`
    /// carry the same descriptor but different per-query contracts.
    pub fn query_contract_hash(profile: &str, budget: &Budget) -> String {
        crate::dispatch::query_contract_hash(profile, budget)
    }

    /// Hard-fail (typed `Err`) when `pinned_descriptor_hash` differs from this engine's
    /// [`descriptor_hash`](Self::descriptor_hash) — the supported way to refuse an answer
    /// minted under a drifted contract, so the consumer does not hand-roll the comparison.
    ///
    /// # Errors
    ///
    /// Returns `Err` naming both hashes when the pin does not match.
    pub fn assert_matches(&self, pinned_descriptor_hash: &str) -> gmeow_errors::Result<()> {
        if self.descriptor_hash == pinned_descriptor_hash {
            Ok(())
        } else {
            Err(gmeow_errors::Diag::of_kind(crate::error::Reason {
                detail: format!(
                    "runtime EngineContract drift: answer pinned to descriptor {pinned} but this \
                     engine is {current}; answers minted under the pinned contract must not be \
                     trusted against a different engine",
                    pinned = pinned_descriptor_hash,
                    current = self.descriptor_hash,
                ),
            }))
        }
    }

    /// Project the descriptor into N-Quads in `graph_iri`, so a consumer can fold the
    /// runtime contract into its own (signed) ledger AS DATA — the same lossy-projection
    /// discipline as [`crate::certificate::CoherenceOutcome::to_nquads`] (the authored
    /// source is this Rust struct; the RDF is one projection). Deterministic: the subject
    /// is content-addressed on [`descriptor_hash`](Self::descriptor_hash) and every
    /// property is fixed-order.
    pub fn to_nquads(&self, graph_iri: &str) -> String {
        let graph = format!("<{graph_iri}>");
        let subject = format!(
            "<{GMEOW_NS}logic/runtime-contract/{}>",
            self.descriptor_hash
        );
        let mut lines: Vec<String> = Vec::new();
        let mut triple = |s: &str, p: &str, o: &str| lines.push(format!("{s} <{p}> {o} {graph} ."));

        triple(
            &subject,
            RDF_TYPE,
            &format!("<{LOGIC_NAMESPACE}EngineContract>"),
        );
        triple(
            &subject,
            &format!("{LOGIC_NAMESPACE}engineContractDescriptorHash"),
            &lit(&self.descriptor_hash),
        );
        triple(
            &subject,
            &format!("{LOGIC_NAMESPACE}backwardSourceHash"),
            &lit(&self.backward_source_hash),
        );
        triple(
            &subject,
            &format!("{LOGIC_NAMESPACE}forwardContractHash"),
            &lit(&self.forward_contract_hash),
        );
        triple(
            &subject,
            &format!("{LOGIC_NAMESPACE}engine"),
            &lit(&format!("{} {}", self.engine.name, self.engine.version)),
        );
        for cap in &self.profiles {
            let profile_iri = format!("<{}>", cap.profile);
            triple(
                &subject,
                &format!("{LOGIC_NAMESPACE}supportedProfile"),
                &profile_iri,
            );
            triple(
                &profile_iri,
                &format!("{LOGIC_NAMESPACE}decidabilityClass"),
                &lit(&cap.decidability_class),
            );
        }

        let mut out = lines.join("\n");
        if !out.is_empty() {
            out.push('\n');
        }
        out
    }
}

/// Render `value` as an escaped N-Triples/N-Quads string literal.
fn lit(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');
    for ch in value.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn is_hex64(s: &str) -> bool {
        s.len() == 64 && s.chars().all(|c| c.is_ascii_hexdigit())
    }

    #[test]
    fn physical_coverage_matches_source_tree() {
        // A new or renamed file under src/physical/ MUST be added to BACKWARD_SOURCE, or
        // the runtime pin would silently stop covering it. This test makes that drift a
        // loud build failure instead.
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/physical");
        let mut actual: Vec<String> = std::fs::read_dir(&dir)
            .expect("src/physical must be readable")
            .filter_map(Result::ok)
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| n.ends_with(".rs"))
            .collect();
        actual.sort();

        let mut covered: Vec<String> = BACKWARD_SOURCE
            .iter()
            .filter_map(|(name, _)| name.strip_prefix("physical/").map(str::to_owned))
            .collect();
        covered.sort();

        assert_eq!(
            actual, covered,
            "src/physical/ drifted from the backward-source contract enumeration; \
             add the new file to BACKWARD_SOURCE"
        );
    }

    #[test]
    fn every_covered_source_file_is_non_empty() {
        for (name, content) in BACKWARD_SOURCE {
            assert!(
                !content.trim().is_empty(),
                "backward-source file {name} resolved empty — the include_str! path is wrong"
            );
        }
    }

    #[test]
    fn descriptor_hash_is_deterministic_hex() {
        let a = EngineContract::current();
        let b = EngineContract::current();
        assert_eq!(
            a.descriptor_hash, b.descriptor_hash,
            "descriptor must be stable"
        );
        assert!(
            is_hex64(&a.descriptor_hash),
            "descriptor must be blake3 hex"
        );
        assert!(is_hex64(&a.backward_source_hash));
    }

    #[test]
    fn backward_and_forward_hashes_are_distinct_surfaces() {
        let c = EngineContract::current();
        assert_ne!(
            c.backward_source_hash, c.forward_contract_hash,
            "backward-dispatch and forward-chase surfaces must not alias"
        );
    }

    #[test]
    fn profile_manifest_covers_every_semantic_profile() {
        let c = EngineContract::current();
        assert_eq!(
            c.profiles.len(),
            RUNTIME_PROFILES.len(),
            "manifest must list every profile"
        );
        // Each carries a resolved (non-"unknown") decidability class.
        for cap in &c.profiles {
            assert!(!cap.decidability_class.is_empty());
            assert_ne!(
                cap.decidability_class, "unknown",
                "profile {} has no decidability class",
                cap.profile
            );
        }
    }

    #[test]
    fn assert_matches_accepts_self_and_rejects_drift() {
        let c = EngineContract::current();
        assert!(c.assert_matches(&c.descriptor_hash).is_ok());
        assert!(
            c.assert_matches("deadbeef").is_err(),
            "a mismatched pin must be a typed hard failure, not silently accepted"
        );
    }

    #[test]
    fn query_contract_hash_is_single_sourced_and_varies_by_budget() {
        let profile = crate::profile_gate::PROBABILISTIC_PROFILE;
        let budget_a = Budget {
            max_answers: Some(1),
            max_steps: None,
        };
        let budget_b = Budget {
            max_answers: Some(2),
            max_steps: None,
        };
        // Deterministic for identical inputs.
        assert_eq!(
            EngineContract::query_contract_hash(profile, &budget_a),
            EngineContract::query_contract_hash(profile, &budget_a),
        );
        // Delegates to the dispatch engine's own helper (single source of truth).
        assert_eq!(
            EngineContract::query_contract_hash(profile, &budget_a),
            crate::dispatch::query_contract_hash(profile, &budget_a),
        );
        // The per-query contract genuinely depends on the invocation, not only on source.
        assert_ne!(
            EngineContract::query_contract_hash(profile, &budget_a),
            EngineContract::query_contract_hash(profile, &budget_b),
            "different budgets must yield different per-query contracts"
        );
    }

    #[test]
    fn to_nquads_projects_the_descriptor_into_the_graph() {
        let c = EngineContract::current();
        let graph = "https://example.org/consumer/ledger";
        let nquads = c.to_nquads(graph);
        assert!(!nquads.is_empty());
        // Deterministic.
        assert_eq!(nquads, c.to_nquads(graph));
        assert!(nquads.contains(&format!("<{LOGIC_NAMESPACE}EngineContract>")));
        assert!(nquads.contains(&c.descriptor_hash));
        for line in nquads.lines() {
            assert!(
                line.ends_with(&format!("<{graph}> .")),
                "every quad must land in the target graph: {line}"
            );
        }
    }
}
