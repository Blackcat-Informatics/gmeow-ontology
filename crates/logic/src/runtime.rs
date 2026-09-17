// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The supported, pin-able runtime query surface of `gmeow-logic`.
//!
//! This module is the ONE import path an external runtime consumer needs to fold a
//! dataset it owns, resolve goals against it, and pin the engine it trusted. It is a
//! curated projection of the RDF 1.2 view → selective fact source → native execution →
//! certified result chain, plus the compatibility store/snapshot path and the
//! self-describing [`EngineContract`](crate::runtime::EngineContract) runtime pin.
//!
//! # What "stable" means here
//!
//! Stability is delivered **consumer-side**, never as a backwards-compat freeze of the
//! core. The surface re-exported below is stable *within a pinned git tag*; across tags
//! it may change, and [`EngineContract`](crate::runtime::EngineContract) is how a consumer *detects* that change. The
//! `gmeow-logic` core (everything outside this module) is greenfield and free to churn;
//! a consumer is protected by its git-tag/vendor pin plus the content-addressed
//! contract, not by a repo promise to preserve these names. There is no crates.io semver
//! obligation over the core.
//!
//! # Pinning the engine
//!
//! At load, fetch [`EngineContract::current`](crate::runtime::EngineContract::current) and record its
//! [`descriptor_hash`](crate::runtime::EngineContract::descriptor_hash) — or the
//! [`to_nquads`](crate::runtime::EngineContract::to_nquads) projection, folded into a (signed) ledger AS
//! DATA. Before trusting a previously-minted answer, call
//! [`assert_matches`](crate::runtime::EngineContract::assert_matches): it hard-fails if the engine
//! drifted from the pinned descriptor, exactly as a signed-ledger consumer refuses an
//! entry under a wrong signature. Per invocation,
//! [`EngineContract::query_contract_hash`](crate::runtime::EngineContract::query_contract_hash) identifies the `profile`/`budget` an answer
//! was decided under (two queries under different budgets share the descriptor but not
//! the per-query contract). The annotated-query and selected-materialization helpers
//! reproduce their richer invocation identities from the same canonical inputs.
//!
//! # Thread-safety / single-writer contract
//!
//! [`RdfDataset`](crate::runtime::RdfDataset), [`PagedDataset`](crate::runtime::PagedDataset), and [`PackView`](crate::runtime::PackView) implement the frozen read contract;
//! share their supported handles across threads and create an operation-scoped fallible
//! paged query view when provider reads need budgets/cancellation evidence. The direct
//! dispatch/materialization functions borrow those views and own only rows admitted by
//! their pushed patterns. [`WorldStore`](crate::store::WorldStore) remains the mutable compatibility path: it wraps
//! a `RefCell` and is **`!Sync`**, so refresh it from one writer, then take a
//! [`WorldFactSnapshot`](crate::seam::WorldFactSnapshot) before parallel snapshot dispatch.
//!
//! # Refusal semantics (three distinct outcomes)
//!
//! [`dispatch_query`](crate::dispatch::dispatch_query) and [`dispatch_query_view`](crate::dispatch::dispatch_query_view) return:
//! * `Ok(AnswerSet { bindings, .. })` — the engine DECIDED. An empty `bindings` means
//!   "decided: no answers".
//! * `Err(..)` — the engine REFUSED: a profile gate rejected the program, or the native
//!   core reported an unsupported fragment. An unsupported fragment is a typed hard
//!   failure, **never** a silent empty answer — there is no fallback engine. A consumer
//!   must treat `Err` as "refused", distinct from `Ok(empty)`.
//!
//! A third semantic case is the caller's responsibility: querying a `world` IRI that is absent
//! from the snapshot yields `Ok(empty)` (nothing to resolve against), indistinguishable
//! from "decided: no answers". Precheck world existence with [`WorldStore::worlds`](crate::store::WorldStore::worlds) if
//! that distinction matters — world-scoping is the caller's job (as with
//! [`WorldStore::select`](crate::store::WorldStore::select)).
//!
//! The fallible boundaries add an operational outcome: provider, page/byte budget,
//! cancellation, deadline, or stale-generation failure. It is distinct from semantic
//! absence and takes precedence over any partial internal answer or materialization.
//!
//! # Direct resident, paged, and succinct-pack execution
//!
//! [`dispatch_query_view`](crate::dispatch::dispatch_query_view) and [`dispatch_query_fallible_view`](crate::dispatch::dispatch_query_fallible_view) bind
//! [`RdfViewFactSource`](crate::seam::RdfViewFactSource) directly to a caller's view. The compiled query pushes its named
//! world, predicate, and bound subject/object values plus cardinality estimates into the
//! view; unrelated pages are not copied or enumerated. The annotated variants preserve
//! tuple lineage through the same physical pass. [`materialize_program_view`](crate::materialize::materialize_program_view) and
//! [`materialize_program_fallible_view`](crate::materialize::materialize_program_fallible_view) provide the forward counterpart over explicit
//! named worlds: they admit only predicates consumed or produced by the canonical
//! program. [`materialize_program`](crate::materialize::materialize_program) remains the explicit whole-dataset/complete-input-echo
//! operation. A source plan that cannot name a predicate is refused rather than widened
//! to an unconstrained scan.
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

use gmeow_logic_compile::ir::LOGIC_NAMESPACE;

use crate::result::EngineId;

pub mod session;

/// The stable operational session surface: a content-addressed [`ReasoningSession`]
/// over the incremental maintenance engine, its 7-axis [`SessionIdentity`], the
/// authorized-commit-referencing [`SessionDelta`]/[`Suppression`] inputs, the total
/// 6-way [`OperationOutcome`], the hash-linked [`TransitionEntry`] journal, and the
/// content-addressed [`Checkpoint`]. Re-exported here so an external runtime consumer
/// needs only `use gmeow_logic::runtime::*` (the one supported import path).
pub use session::{
    Checkpoint, CommittedDelta, FragmentDisposition, IncompleteCause, IntegrityFault,
    OperationOutcome, OutcomeTag, PagedCompositionMetrics, ReasoningSession, RebuildReason,
    SessionDelta, SessionIdentity, Suppression, TransitionEntry, UnsupportedFragment,
    edb_data_generation,
};

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
    BudgetStatus, DerivationId, DerivedQuad, RdfViewFactSource, WorldFactPattern,
    WorldFactSnapshot, WorldFactSource, WorldSourceIdentity, WorldSourceMetrics,
};

/// The query IR: the parser, the program/goal value types, the answer set, and the
/// per-answer binding + completion-frontier shapes.
pub use crate::query_ir::{
    AnswerSet, Binding, Budget, CompletionFrontier, QProgram, parse_query_program,
};

/// Production entry points and completeness/evidence carriers for backward goal
/// resolution over snapshots, resident/pack views, and fallible paged views.
pub use crate::dispatch::{
    CompleteAnnotatedViewQuery, CompleteRelationViewQuery, CompleteViewQuery,
    FallibleAnnotatedViewQueryResult, FallibleRelationViewQueryError,
    FallibleRelationViewQueryResult, FallibleViewQueryError, FallibleViewQueryResult,
    QueryExecutionEvidence, QueryExecutionIdentity, RelationAnnotationRequest, RelationQueryError,
    ResidentViewEvidence, dispatch_query, dispatch_query_annotated_fallible_view,
    dispatch_query_annotated_view, dispatch_query_annotated_with_relations,
    dispatch_query_annotated_with_relations_fallible_view,
    dispatch_query_annotated_with_relations_view, dispatch_query_fallible_view,
    dispatch_query_view,
};

/// Immutable external-relation descriptors, providers, evidence, and typed failures.
pub use crate::external_relation::{
    ExternalRelationProvider, NeverCancelled, ProviderTupleSource, QueryRelationProviders,
    RelationAccessMetrics, RelationAnnotationDimension, RelationBatch, RelationCall,
    RelationCancellation, RelationContractError, RelationExecutionError,
    RelationExecutionFailureKind, RelationInvocationReceipt, RelationInvocationStatus,
    RelationOrderDirection, RelationOrdering, RelationProviderBudget, RelationProviderDescriptor,
    RelationProviderError, RelationProviderFailureKind, RelationProviderIncompletenessKind,
    RelationProviderRegistration, RelationQueryFailureReceipt, RelationQueryReceipt,
    RelationQueryResult, RelationTuple, TableRelationProvider,
};

/// Opaque tuple-annotation inputs and results used by annotated direct-view dispatch.
pub use crate::annotation::{
    AnnotatedAnswer, AnnotatedAnswerSet, AnnotatedFactKey, AnnotationCertification,
    AnnotationContract, AnnotationDerivation, AnnotationFactRef, AnnotationQueryClass,
    AnnotationRequest, TupleAnnotationAlgebra,
};

/// Forward materialization over whole resident datasets or selective RDF views.
pub use crate::materialize::{
    CompleteViewMaterialization, FallibleViewMaterializationError,
    FallibleViewMaterializationResult, Materialization, MaterializationLimits, MaterializeError,
    materialize_program, materialize_program_fallible_view, materialize_program_source,
    materialize_program_view,
};

/// The preservation claim an [`AnswerSet`] carries, and its polarity kind — the
/// faithfulness judgment a consumer reads off an answer.
pub use crate::result::PreservationClaim;
pub use gmeow_logic_compile::ir::{LogicProgram, PreservationKind, SemanticProfileId};
pub use gmeow_logic_compile::result_shape::ColumnKind;

/// Frozen-dataset construction types, re-exported so a consumer needs no direct
/// `purrdf` import churn to build the `Arc<RdfDataset>` it folds.
pub use purrdf::{
    FallibleDatasetView, PackView, PagedDataset, PagedQueryError, PagedQueryEvidence,
    PagedQueryLimits, RdfDataset, RdfDatasetBuilder, RdfQuad, RdfTerm, TermValue,
};

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

// One portable semantic source owner covers the complete native kernel and
// follows newly declared modules automatically. Executable/profile admission is
// a separate contract; this identity is the same on native and WebAssembly.
include!(concat!(env!("OUT_DIR"), "/native_semantic_sources.rs"));

/// Frame `value` under `tag` into `hasher` with a domain tag and length prefixes, so
/// no component boundary can collide with another (`("ab","c")` and `("a","bc")` hash
/// distinctly). Mirrors the framed-BLAKE3 discipline in `dispatch::query_contract_hash`.
pub(crate) fn frame(hasher: &mut blake3::Hasher, tag: &[u8], value: &[u8]) {
    hasher.update(&(tag.len() as u64).to_le_bytes());
    hasher.update(tag);
    hasher.update(&(value.len() as u64).to_le_bytes());
    hasher.update(value);
}

/// The content digest over the whole backward-dispatch source surface.
fn backward_source_hash() -> String {
    let mut hasher = blake3::Hasher::new();
    frame(&mut hasher, b"domain", b"gmeow-logic-backward-source-v2");
    frame(
        &mut hasher,
        b"native-semantic-kernel",
        NATIVE_SOURCE_CONTRACT.as_bytes(),
    );
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
/// the WHOLE engine — the forward EL/DL/RL chase plus typed-modal post-pass
/// ([`forward_contract_hash`]) and the
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

    /// Reproduce the invocation identity used by annotated direct-view dispatch.
    ///
    /// This frames the ordinary profile/resource contract together with the exact
    /// tuple-annotation admission/convergence contract.
    pub fn annotated_query_contract_hash(
        profile: &str,
        budget: &Budget,
        annotation: &AnnotationContract,
        algebra_identity: &str,
    ) -> String {
        crate::dispatch::annotated_query_contract_hash(
            profile,
            budget,
            annotation,
            algebra_identity,
        )
    }

    /// Reproduce the invocation identity used by provider-aware annotated dispatch.
    pub fn external_relation_query_contract_hash(
        profile: &str,
        budget: &Budget,
        annotation: &AnnotationContract,
        algebra_identity: &str,
        provider_manifest_hash: &str,
    ) -> String {
        crate::dispatch::external_relation_query_contract_hash(
            profile,
            budget,
            annotation,
            algebra_identity,
            provider_manifest_hash,
        )
    }

    /// Reproduce the invocation identity used by selected view materialization.
    ///
    /// The canonical program, explicit named-world set, step budget, and declared
    /// semantic profile are all content-framed; world input order is immaterial.
    pub fn materialization_contract_hash(
        program: &LogicProgram,
        worlds: &[String],
        limits: MaterializationLimits,
        declared_profile: Option<SemanticProfileId>,
    ) -> String {
        crate::materialize::selected_materialization_contract_hash(
            program,
            worlds,
            limits,
            declared_profile,
        )
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
            Err(gmeow_errors::Diag::of_kind(crate::error::ContractDrift {
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

#[path = "runtime.tests.rs"]
#[cfg(test)]
mod tests;
