// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The supported, pin-able runtime query surface of `gmeow-logic`.
//!
//! This module is the ONE import path an external runtime consumer needs to fold a
//! dataset it owns, resolve goals against it, and pin the engine it trusted. It is a
//! curated projection of the internal store → snapshot → dispatch → result chain, plus
//! the self-describing [`EngineContract`] runtime pin.
//!
//! # What "stable" means here
//!
//! Stability is delivered **consumer-side**, never as a backwards-compat freeze of the
//! core. The surface re-exported below is stable *within a pinned git tag*; across tags
//! it may change, and [`EngineContract`] is how a consumer *detects* that change. The
//! `gmeow-logic` core (everything outside this module) is greenfield and free to churn;
//! a consumer is protected by its git-tag/vendor pin plus the content-addressed
//! contract, not by a repo promise to preserve these names. There is no crates.io semver
//! obligation over the core.
//!
//! # Pinning the engine
//!
//! At load, fetch [`EngineContract::current`] and record its
//! [`descriptor_hash`](EngineContract::descriptor_hash) — or the
//! [`to_nquads`](EngineContract::to_nquads) projection, folded into a (signed) ledger AS
//! DATA. Before trusting a previously-minted answer, call
//! [`assert_matches`](EngineContract::assert_matches): it hard-fails if the engine
//! drifted from the pinned descriptor, exactly as a signed-ledger consumer refuses an
//! entry under a wrong signature. Per invocation,
//! [`EngineContract::query_contract_hash`] identifies the `profile`/`budget` an answer
//! was decided under (two queries under different budgets share the descriptor but not
//! the per-query contract).
//!
//! # Thread-safety / single-writer contract
//!
//! [`RdfDataset`] is the frozen, `Send + Sync` IR — share an `Arc<RdfDataset>` across
//! threads freely. [`WorldStore`] wraps a `RefCell` and is **`!Sync`**: refresh it from a
//! single writer, then take a [`WorldFactSnapshot`] via
//! [`WorldFactSnapshot::from_world`] and parallelize over the (immutable) snapshot. A
//! snapshot reflects the store's quads at the instant it is taken — after an append you
//! must re-snapshot to see the new facts.
//!
//! # Refusal semantics (three distinct outcomes)
//!
//! [`dispatch_query`] returns:
//! * `Ok(AnswerSet { bindings, .. })` — the engine DECIDED. An empty `bindings` means
//!   "decided: no answers".
//! * `Err(..)` — the engine REFUSED: a profile gate rejected the program, or the native
//!   core reported an unsupported fragment. An unsupported fragment is a typed hard
//!   failure, **never** a silent empty answer — there is no fallback engine. A consumer
//!   must treat `Err` as "refused", distinct from `Ok(empty)`.
//!
//! A third case is the caller's responsibility: querying a `world` IRI that is absent
//! from the snapshot yields `Ok(empty)` (nothing to resolve against), indistinguishable
//! from "decided: no answers". Precheck world existence with [`WorldStore::worlds`] if
//! that distinction matters — world-scoping is the caller's job (as with
//! [`WorldStore::select`]).
//!
//! # Forward-compatibility with a paged dataset backend
//!
//! [`WorldStore::from_dataset`] takes the [`RdfDataset`] IR, so its signature is stable
//! when a paged dataset/storage backend lands behind that type. Note the current path
//! **fully materializes**: [`WorldStore`] copies quads into an owned dataset, and
//! [`WorldFactSnapshot::from_world`] copies again into an owned fact vector. A paged
//! backend therefore reduces the caller's dataset-build cost, but the store still
//! materializes its working set; end-to-end paging would need a future read-through
//! [`WorldFactSource`] implemented directly over a paged `Arc<RdfDataset>`. This is not
//! delivered here.
//!
//! # Worked example — external dataset → snapshot → dispatch, with a load-bearing append
//!
//! ```rust
//! use gmeow_logic::runtime::*;
//!
//! const W: &str = "http://logic.test/world/doc";
//! const EX: &str = "https://example.org/";
//! let iri = |local: &str| format!("{EX}{local}");
//! let parent_of = iri("parentOf");
//! let profile = "https://blackcatinformatics.ca/logic/PositiveHornProfile";
//!
//! // A consumer folds its OWN Arc<RdfDataset> (here: a → b, b → c), not a repo checkout.
//! let mut builder = RdfDatasetBuilder::new();
//! for (s, o) in [("a", "b"), ("b", "c")] {
//!     let quad = RdfQuad::new(RdfTerm::iri(iri(s)), &parent_of, RdfTerm::iri(iri(o)))
//!         .in_graph(RdfTerm::iri(W));
//!     builder.push_owned_quad(&quad);
//! }
//! let dataset = builder.freeze().expect("valid dataset");
//! let store = WorldStore::from_dataset(&dataset).expect("fold the caller's dataset");
//!
//! // The recursive-ancestor program; the goal asks for c's descendants.
//! let program = parse_query_program(
//!     ":- prefix(ex, 'https://example.org/').\n\
//!      ex:ancestor(X, Y) :- ex:parentOf(X, Y).\n\
//!      ex:ancestor(X, Y) :- ex:parentOf(X, Z), ex:ancestor(Z, Y).\n\
//!      ?- ex:ancestor(ex:c, Y).\n",
//! )
//! .expect("valid query program");
//! let budget = Budget::default();
//!
//! // Base fold: c has no descendants, so the engine DECIDES the empty answer.
//! let base = WorldFactSnapshot::from_world(&store, W, profile).expect("snapshot");
//! let base_answer = dispatch_query(&base, W, &program, profile, &budget).expect("dispatch");
//! assert!(base_answer.bindings.is_empty(), "c has no descendants in the base fold");
//!
//! // Incrementally APPEND one quad (c → d). This is the SOLE cause of a new answer.
//! store.insert_quad(W, &iri("c"), &parent_of, &iri("d"));
//! let refreshed = WorldFactSnapshot::from_world(&store, W, profile).expect("re-snapshot");
//! let answer = dispatch_query(&refreshed, W, &program, profile, &budget).expect("dispatch");
//! assert_eq!(answer.bindings.len(), 1, "the appended quad yields exactly one answer");
//! assert_eq!(answer.bindings[0]["Y"], format!("<{}>", iri("d")), "and it is d");
//!
//! // Pin the engine: record the descriptor (or its N-Quads) and refuse drift later.
//! let contract = EngineContract::current();
//! contract.assert_matches(&contract.descriptor_hash).expect("self-match holds");
//! assert!(!contract.to_nquads("https://example.org/consumer/ledger").is_empty());
//! // The per-query contract the answer above was decided under (reproducible, stable).
//! let qc = EngineContract::query_contract_hash(profile, &budget);
//! assert_eq!(qc, EngineContract::query_contract_hash(profile, &budget));
//!
//! // Wholesale replace: a fresh store from a re-folded dataset carries only its worlds.
//! let mut other = RdfDatasetBuilder::new();
//! other.push_owned_quad(
//!     &RdfQuad::new(RdfTerm::iri(iri("m")), &parent_of, RdfTerm::iri(iri("n")))
//!         .in_graph(RdfTerm::iri(W)),
//! );
//! let replaced = WorldStore::from_dataset(&other.freeze().expect("valid")).expect("re-fold");
//! assert_eq!(replaced.worlds(), vec![W.to_string()]);
//! ```

use std::sync::OnceLock;

use gmeow_logic_compile::ir::{LOGIC_NAMESPACE, SemanticProfileId};

use crate::result::EngineId;

// ── The curated stable surface ───────────────────────────────────────────────────
//
// One import path (`use gmeow_logic::runtime::*`) for the whole runtime call chain. Each
// name below is part of the stable-within-a-tag surface (see the module docs); the
// items live in internal modules that are free to churn.

/// The world-indexed store and its supported constructors/refresh/append methods.
pub use crate::store::WorldStore;

/// The read-only fact-source bridge `dispatch_query` consumes, and the snapshot that
/// crosses a [`WorldStore`] into it.
pub use crate::seam::{
    BudgetStatus, DerivationId, DerivedQuad, WorldFactSnapshot, WorldFactSource,
};

/// The query IR: the parser, the program/goal value types, the answer set, and the
/// per-answer binding + completion-frontier shapes.
pub use crate::query_ir::{
    AnswerSet, Binding, Budget, CompletionFrontier, QProgram, parse_query_program,
};

/// The single production entry point for backward goal resolution.
pub use crate::dispatch::dispatch_query;

/// The preservation claim an [`AnswerSet`] carries, and its polarity kind — the
/// faithfulness judgment a consumer reads off an answer.
pub use crate::result::PreservationClaim;
pub use gmeow_logic_compile::ir::PreservationKind;

/// Frozen-dataset construction types, re-exported so a consumer needs no direct
/// `purrdf` import churn to build the `Arc<RdfDataset>` it folds.
pub use purrdf::{RdfDataset, RdfDatasetBuilder, RdfQuad, RdfTerm, TermValue};

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
    /// single [`dispatch_query`] call runs under
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
