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

/// The backward-dispatch source files whose bytes define what an [`AnswerSet`] a
/// [`dispatch_query`] call decides. A change to any of them changes
/// [`EngineContract::current`]'s `backward_source_hash`, so a pinned consumer detects
/// it. Coverage is guarded against drift by `backward_source_partition_is_total`: every
/// top-level `src/*.rs` file and every `src/physical/*.rs` file must appear in EITHER
/// this list OR the test-only `NOT_BACKWARD_SOURCE` (with a truthful reason) — a new, renamed, or
/// newly-delegated-to file breaks that test loudly rather than silently dropping out of
/// the contract.
///
/// `annotation.rs` (the caller-visible algebra and admission contract), `facts.rs`
/// (term/predicate interning — `TermInterner::intern` keys answer-bound term identity),
/// `provenance.rs` (`term_display`, the interning key `facts.rs` hashes on), `term_codec.rs`
/// (the native term decoder used by `rule_ir.rs`), and `rule_ir.rs` (the shared
/// evaluable rule IR + join/unification primitives —
/// `ground`, `match_atom`, `join_body`, `extend_solutions` — that every `physical/*.rs`
/// evaluator below imports in production, non-test code) are included for the same
/// reason the `physical/` files are: their bytes are demonstrably imported by the
/// production (non-`#[cfg(test)]`) code of the physical engine, so a change to them can
/// change a decided [`AnswerSet`] exactly as a change to `physical/seminaive.rs` can.
///
/// # Verified decision-surface boundary
///
/// The backward goal-resolution DECISION surface is confined to these top-level
/// `src/*.rs` files, plus `src/physical/*.rs`, plus the relocated shared term arena
/// ([`EXTERNAL_BACKWARD_SOURCE`]). The `src/` subdirectory modules — notably
/// `reason/` (the forward EL/DL/RL chase, pinned separately via
/// `forward_contract_hash`) — and the remaining subdirectories are forward reasoning or
/// post-hoc bookkeeping; none sit on the `dispatch_query` decision path. This was
/// verified by tracing production (non-`#[cfg(test)]`) imports. The retired external
/// engine and its physical parity adapter are deliberately absent: the native physical
/// core is the sole production authority. The partition is therefore not recursed into
/// forward-only subsystems, which would falsely imply they are backward-relevant.
///
/// [`AnswerSet`]: crate::query_ir::AnswerSet
/// [`dispatch_query`]: crate::dispatch::dispatch_query
const BACKWARD_SOURCE: &[(&str, &str)] = &[
    ("annotation.rs", include_str!("annotation.rs")),
    ("dispatch.rs", include_str!("dispatch.rs")),
    ("external_relation.rs", include_str!("external_relation.rs")),
    ("facts.rs", include_str!("facts.rs")),
    ("profile_gate.rs", include_str!("profile_gate.rs")),
    ("provenance.rs", include_str!("provenance.rs")),
    ("query_ir.rs", include_str!("query_ir.rs")),
    ("rule_ir.rs", include_str!("rule_ir.rs")),
    ("seam.rs", include_str!("seam.rs")),
    ("term_codec.rs", include_str!("term_codec.rs")),
    (
        "physical/annotation.rs",
        include_str!("physical/annotation.rs"),
    ),
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
    // The structured full-FOL rung, and the unification / proof substrate it consumes in
    // production. Since `magic::resolve_native_under` routes a structured
    // (`QTerm::Struct`) program into `resolve_fol`, and `resolve_fol` imports the shared
    // term arena, `lower`, `unify`, and `proof`, a change to any of them can change a
    // decided structured `AnswerSet` — so they are part of the backward decision surface.
    // The arena's own source moved OUT of this crate; it is pinned, byte-for-byte and with
    // the same weight, by `EXTERNAL_BACKWARD_SOURCE` below.
    ("physical/lower.rs", include_str!("physical/lower.rs")),
    ("physical/proof.rs", include_str!("physical/proof.rs")),
    ("physical/unify.rs", include_str!("physical/unify.rs")),
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
    ("physical/plan.rs", include_str!("physical/plan.rs")),
    (
        "physical/resolve_fol.rs",
        include_str!("physical/resolve_fol.rs"),
    ),
    (
        "physical/seminaive.rs",
        include_str!("physical/seminaive.rs"),
    ),
    ("physical/store.rs", include_str!("physical/store.rs")),
];

/// The backward-dispatch decision surface that lives OUTSIDE this crate.
///
/// The one structured-term representation — the hash-consed, content-addressed term DAG
/// and its content-key fold — was relocated into the reasoner-free `gmeow-term-arena`
/// crate so a parser front-end can intern terms without linking this runtime. Relocating
/// it does NOT relocate its authority: `resolve_fol`, `unify`, `proof`, and `lower` all
/// operate that arena in production, so a byte change to any of its source can change a
/// decided structured [`AnswerSet`] exactly as a change to `physical/seminaive.rs` can.
///
/// It is a SEPARATE list only because [`BACKWARD_SOURCE`]'s partition guard enumerates
/// this crate's `src/` tree, and a file that no longer lives there would be reported as a
/// stale entry. `external_backward_source_partition_is_total` enumerates the arena crate's
/// `src/` the same way, so a new, renamed, or deleted arena module still breaks loudly
/// rather than silently dropping out of [`backward_source_hash`].
///
/// The digest folds this list AFTER [`BACKWARD_SOURCE`], under its own framing tag, so the
/// two surfaces cannot alias and the fold order is fixed.
///
/// [`AnswerSet`]: crate::query_ir::AnswerSet
const EXTERNAL_BACKWARD_SOURCE: &[(&str, &str)] = &[
    (
        "gmeow-term-arena/src/display.rs",
        include_str!("../../term-arena/src/display.rs"),
    ),
    (
        "gmeow-term-arena/src/id.rs",
        include_str!("../../term-arena/src/id.rs"),
    ),
    (
        "gmeow-term-arena/src/interner.rs",
        include_str!("../../term-arena/src/interner.rs"),
    ),
    (
        "gmeow-term-arena/src/lib.rs",
        include_str!("../../term-arena/src/lib.rs"),
    ),
    (
        "gmeow-term-arena/src/term_dag.rs",
        include_str!("../../term-arena/src/term_dag.rs"),
    ),
    (
        "gmeow-term-arena/src/term_key.rs",
        include_str!("../../term-arena/src/term_key.rs"),
    ),
];

/// Every top-level `src/*.rs` file that is deliberately NOT part of the
/// backward-dispatch decision surface [`BACKWARD_SOURCE`] enumerates, paired with a
/// truthful, specific reason. `backward_source_partition_is_total` asserts every actual
/// top-level source file appears in EXACTLY ONE of `BACKWARD_SOURCE` or this list — so a
/// new top-level module cannot silently escape `backward_source_hash` coverage.
///
/// Classification was done by reading each file's own module doc comment AND tracing
/// its actual (non-`#[cfg(test)]`) callers across `dispatch.rs`, `profile_gate.rs`,
/// `query_ir.rs`, `seam.rs`, `physical/*.rs`, `facts.rs`, `provenance.rs`, and
/// `rule_ir.rs` — a file is excluded here only when that trace turned up no production
/// caller from the decision surface (or only a `#[cfg(test)]`/doc-comment mention).
///
/// `#[cfg(test)]`: this list is pure test scaffolding for
/// `backward_source_partition_is_total` — unlike [`BACKWARD_SOURCE`], nothing in the
/// production `EngineContract` computation reads it.
#[cfg(test)]
const NOT_BACKWARD_SOURCE: &[(&str, &str)] = &[
    (
        "physical/interning.rs",
        "#[cfg(test)] generating proptest suite for math: expression interning and alpha-equivalence identity — it DRIVES physical/lower.rs's production entry points but is itself test scaffolding with no production caller; the lowering it exercises is pinned as a BACKWARD_SOURCE member in its own right",
    ),
    (
        "physical/term_dag_tests.rs",
        "#[cfg(test)] invariant suite for the relocated shared term arena (injectivity, alpha-collapse, metavariable identity, and the DAG <-> logic: IR congruence) — test scaffolding with no production caller; the arena's own source is pinned in EXTERNAL_BACKWARD_SOURCE",
    ),
    (
        "certificate.rs",
        "forward reasoning/coherence-certificate surface — pinned via forward_contract_hash, not backward dispatch",
    ),
    (
        "term_arena.rs",
        "re-export of the shared term arena's façade plus the math:-graph interning wrapper — the ARENA's own source is pinned in EXTERNAL_BACKWARD_SOURCE and the lowering it calls is pinned as physical/lower.rs; this file itself adds no decision logic and no file in dispatch/profile_gate/query_ir/seam/physical/rule_ir/facts/provenance imports it",
    ),
    (
        "certify.rs",
        "static profile/decidability certifier (Python-oracle mirror) — consumed by the benchmark harness (cost.rs), the EngineContract capability manifest, and forward materialization routing (materialize.rs); no reference from dispatch/profile_gate/query_ir/seam/physical/rule_ir/facts/provenance",
    ),
    (
        "conjecture.rs",
        "conjecture-and-refutation over relational_core — forward/generative reasoning surface, not backward dispatch",
    ),
    (
        "correspondence_exec.rs",
        "executed lens-law discharge for logic:Correspondence gates — forward coherence-certification surface, not backward dispatch",
    ),
    (
        "cost.rs",
        "deterministic engine-benchmark harness + cost-vector instrumentation, not the dispatch decision path",
    ),
    (
        "counterfactual.rs",
        "Stratum-C counterfactual world construction — the generative/forward reasoning stratum; query_ir.rs only doc-links it, dispatch_query does not call it",
    ),
    (
        "dag_profile.rs",
        "DAG-workflow acyclicity certifier, consumed by teleology.rs — static certification, not backward dispatch",
    ),
    (
        "dense.rs",
        "dependency-free dense-id graph primitives, consumed by certify.rs (test) and entrenchment.rs — acceleration for forward/certification code, not backward dispatch",
    ),
    (
        "derivation_graph.rs",
        "truth-maintenance derivation graph, consumed by foundation.rs — forward evaluator support, not backward dispatch",
    ),
    (
        "entail.rs",
        "native entailment-by-refutation (A ⊨ C iff premise ∪ ¬C inconsistent) composed over dl_consistency — forward consistency reduction, not backward query dispatch",
    ),
    (
        "entrenchment.rs",
        "epistemic-entrenchment ordering for AGM revision — Stratum-C forward/generative revision support, not backward dispatch",
    ),
    (
        "error.rs",
        "reasoning-core diagnostic kinds — result/diagnostic data types, not dispatch decision logic",
    ),
    (
        "explain.rs",
        "explanation-skeleton emitter over an already-materialized result — post-hoc projection, not decision logic",
    ),
    (
        "foundation.rs",
        "native OntoUML foundation-discipline evaluator — forward evaluator/classifier, not backward dispatch",
    ),
    (
        "goal_directed.rs",
        "the pub façade that runs shipped goal-directed demonstrators through the backward engine and projects proof-checked answers to RDF — a downstream consumer of the decision path (it calls resolve_fol), not part of what dispatch_query decides",
    ),
    (
        "proof_tree.rs",
        "the pub structured proof view (proof term -> step tree -> TSTP derivation) — like goal_directed.rs a downstream READER of an already-decided proof (it calls goal_directed's lowering, resolve_fol, and proof::check and then only projects); nothing in dispatch/profile_gate/query_ir/seam/physical/rule_ir/facts/provenance imports it, so it cannot change what dispatch_query decides",
    ),
    ("lib.rs", "crate-root module wiring, not the decision path"),
    (
        "logic_diagnostics.rs",
        "projection of compile-parse diagnostics into gmeow_errors::Report — diagnostic data projection, not decision logic",
    ),
    (
        "lower.rs",
        "canonical-AST to EvalRule lowering consumed by the forward materializer and pinned through forward_contract_hash; no backward dispatch caller",
    ),
    (
        "materialize.rs",
        "pure-Rust forward materialization core pinned through forward_contract_hash — no backward dispatch caller",
    ),
    (
        "math_dimension.rs",
        "the math: measure-and-dimension reasoned-graph gate over an already-reasoned graph — post-hoc verify-time enforcement, not backward-dispatch decision logic",
    ),
    (
        "math_expression.rs",
        "the math: expression-identity reasoned-graph gate over an already-reasoned graph — post-hoc verify-time enforcement (structuralKey drift/leak/rejection), not backward-dispatch decision logic",
    ),
    (
        "nary.rs",
        "n-ary predication to reified-binary lowering + forward-chase ingestion — facts are lowered before backward dispatch runs; not itself part of the decision",
    ),
    (
        "obligations.rs",
        "typed-formalization-governance checks over an already-reasoned graph — post-hoc verification, not decision logic",
    ),
    (
        "oracle.rs",
        "native forward-materialization boundary plus a test-only backward reference seam; production backward dispatch calls the physical core directly and does not call this module",
    ),
    (
        "probabilistic.rs",
        "ProbLog-style weighted-inference evaluator for logic:ProbabilisticProfile — a separate execution path with no reference from dispatch/profile_gate/query_ir/seam/physical/rule_ir/facts/provenance",
    ),
    (
        "purremb_relation.rs",
        "query-scoped external-relation provider over a verified PURREMB embedding artifact — consumes the ExternalRelationProvider contract from external_relation.rs and is invoked only through the &dyn ExternalRelationProvider trait object supplied per query by callers; no production file in dispatch/profile_gate/query_ir/seam/physical/rule_ir/facts/provenance imports it, so its bytes are an external EDB input, not part of what dispatch_query decides",
    ),
    (
        "reasoning_graphs.rs",
        "shared constants and membership predicate for the forward object-level named-graph boundary — consumed by pipeline assembly and coherence gates, not backward dispatch",
    ),
    (
        "relational_core.rs",
        "FOL-to-Horn relational-core lowering adapter consumed by forward reasoning/materialization and pinned through forward_contract_hash — not backward dispatch",
    ),
    (
        "result.rs",
        "the typed ReasoningResult data model — pure data; physical/magic_generic.rs's only production use is constructing the PreservationClaim::exact() trust marker, not decision logic",
    ),
    (
        "result_rdf.rs",
        "deterministic RDF projection of ReasoningResult — data projection, not decision logic",
    ),
    (
        "runtime.rs",
        "this façade module itself — re-export wiring + the EngineContract descriptor, not the decision path",
    ),
    (
        "stablemodel.rs",
        "native stable-model/answer-set evaluator consumed by materialize.rs and pinned through forward_contract_hash — forward incremental materialization, not backward dispatch",
    ),
    (
        "store.rs",
        "the world-indexed named-graph store — a thin wrapper over purrdf's MutableDataset (insert/pattern/select delegate directly to purrdf); seam.rs's WorldFactSnapshot::from_world is the only backward-relevant caller, and it consumes only the copied-out quad bytes, not store.rs's own logic",
    ),
    (
        "synth_corpus.rs",
        "synthetic relational-core Datalog generators for the benchmark harness, not decision logic",
    ),
    (
        "teleology.rs",
        "native canonical-process teleology evaluator — forward evaluator/classifier, not backward dispatch",
    ),
    (
        "termination_demonstrators.rs",
        "static chase-termination-ladder demonstrator RDF constants shipped into gmeow.gts — data, not backward dispatch decision logic",
    ),
    (
        "transaction.rs",
        "Transaction Logic combinator interpreter — forward transaction-program executor, not backward dispatch",
    ),
    (
        "transition.rs",
        "elementary Transaction-Logic world-snapshot updates — a state-transition mechanism, not query decision logic",
    ),
    (
        "verify.rs",
        "native reasoned-graph verify (closed-world SHACL-like QC) — forward verification surface, not backward dispatch",
    ),
    (
        "versioning.rs",
        "content-hash graph-versioning keys for world caching — staleness/cache-key computation, not decision logic",
    ),
    (
        "wellfounded.rs",
        "native well-founded-semantics evaluator consumed by materialize.rs and pinned through forward_contract_hash — forward incremental materialization, not backward dispatch",
    ),
    (
        "reference_resolver.rs",
        "declarative SLD/Datalog reference oracle — used only inside #[cfg(test)] cross-checks (dispatch.rs::tests, physical/magic.rs::tests), not the production dispatch decision path",
    ),
    (
        "conjecture_eval.rs",
        "the conjecture-and-refutation evaluation ORCHESTRATION (parse a candidate logic:Formula, re-home the KB into the scenario world, run conjecture_test, project the verdict) — a high-level entry over conjecture.rs/result_rdf.rs, not a backward-dispatch decision source dispatch_query consults",
    ),
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
    // The relocated term-arena source, under its OWN framing tag and at a fixed position
    // after the in-crate list: relocation must not weaken the pin, and the two tags keep
    // the surfaces from aliasing.
    for (name, content) in EXTERNAL_BACKWARD_SOURCE {
        frame(&mut hasher, b"external-file", name.as_bytes());
        frame(&mut hasher, b"external-body", content.as_bytes());
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

#[cfg(test)]
mod tests {
    use super::*;

    fn is_hex64(s: &str) -> bool {
        s.len() == 64 && s.chars().all(|c| c.is_ascii_hexdigit())
    }

    /// List the `.rs` file names directly under `dir` (non-recursive), hard-failing on
    /// any unreadable entry rather than silently dropping it — a partially-unreadable
    /// directory must not be able to pass this guard by having its bad entries filtered
    /// away (repo policy: a missing/unreadable required thing is a hard fail, never a
    /// silent gap).
    fn rs_file_names(dir: &std::path::Path) -> Vec<String> {
        std::fs::read_dir(dir)
            .unwrap_or_else(|e| panic!("{} must be readable: {e}", dir.display()))
            .map(|entry| entry.expect("failed to read a src directory entry"))
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .filter(|n| n.ends_with(".rs"))
            .collect()
    }

    #[test]
    fn backward_source_partition_is_total() {
        // Every top-level src/*.rs file and every src/physical/*.rs file must appear in
        // EXACTLY ONE of BACKWARD_SOURCE (it helps decide an AnswerSet) or
        // NOT_BACKWARD_SOURCE (a reasoned exclusion). A new, renamed, or
        // newly-delegated-to file that is added to neither list would otherwise silently
        // stop being covered by backward_source_hash — this test makes that drift a loud
        // build failure instead of a silent pin gap.
        let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));

        let mut actual_top_level = rs_file_names(&manifest_dir.join("src"));
        actual_top_level.sort();

        let mut actual_physical: Vec<String> = rs_file_names(&manifest_dir.join("src/physical"))
            .into_iter()
            .map(|name| format!("physical/{name}"))
            .collect();
        actual_physical.sort();

        let mut actual: Vec<String> = actual_top_level;
        actual.extend(actual_physical);
        actual.sort();
        actual.dedup();

        let mut covered: Vec<String> = BACKWARD_SOURCE
            .iter()
            .map(|(name, _)| (*name).to_owned())
            .chain(
                NOT_BACKWARD_SOURCE
                    .iter()
                    .map(|(name, _)| (*name).to_owned()),
            )
            .collect();
        covered.sort();

        // Every classified name must name a file that actually exists — neither list may
        // enumerate a stale/renamed entry.
        for name in &covered {
            assert!(
                actual.contains(name),
                "{name} is listed in BACKWARD_SOURCE or NOT_BACKWARD_SOURCE but no such \
                 file exists under src/ — remove the stale entry"
            );
        }

        // No name may appear in both lists (a file cannot both decide an AnswerSet and
        // be reasoned-excluded from deciding one).
        let mut backward_only: Vec<&str> = BACKWARD_SOURCE.iter().map(|(name, _)| *name).collect();
        backward_only.sort();
        let mut not_backward_only: Vec<&str> =
            NOT_BACKWARD_SOURCE.iter().map(|(name, _)| *name).collect();
        not_backward_only.sort();
        for name in &backward_only {
            assert!(
                !not_backward_only.contains(name),
                "{name} is listed in BOTH BACKWARD_SOURCE and NOT_BACKWARD_SOURCE — pick one"
            );
        }

        covered.dedup();
        assert_eq!(
            actual, covered,
            "src/*.rs and src/physical/*.rs drifted from the BACKWARD_SOURCE ∪ \
             NOT_BACKWARD_SOURCE partition; a new src file must be added to \
             BACKWARD_SOURCE (if it affects what dispatch_query decides) or to \
             NOT_BACKWARD_SOURCE with a reason"
        );

        // Every NOT_BACKWARD_SOURCE reason must be non-empty — an exclusion without a
        // reason is exactly the silent guessing this partition exists to forbid.
        for (name, reason) in NOT_BACKWARD_SOURCE {
            assert!(
                !reason.trim().is_empty(),
                "NOT_BACKWARD_SOURCE entry {name} has no reason"
            );
        }
    }

    #[test]
    fn every_covered_source_file_is_non_empty() {
        for (name, content) in BACKWARD_SOURCE.iter().chain(EXTERNAL_BACKWARD_SOURCE) {
            assert!(
                !content.trim().is_empty(),
                "backward-source file {name} resolved empty — the include_str! path is wrong"
            );
        }
    }

    #[test]
    fn external_backward_source_partition_is_total() {
        // The relocation of the term arena into its own crate must not open a hole in the
        // pin: EVERY `.rs` file under `crates/term-arena/src` must appear in
        // EXTERNAL_BACKWARD_SOURCE. A new, renamed, or deleted arena module would
        // otherwise silently stop being covered by backward_source_hash — the same drift
        // `backward_source_partition_is_total` forbids for this crate's own `src/`.
        //
        // There is no NOT_ counterpart list: the arena IS the term representation the
        // structured resolver decides over, so every byte of it is backward-relevant.
        let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let arena_src = manifest_dir
            .parent()
            .expect("crates/logic has a parent")
            .join("term-arena/src");

        let mut actual: Vec<String> = rs_file_names(&arena_src)
            .into_iter()
            .map(|name| format!("gmeow-term-arena/src/{name}"))
            .collect();
        actual.sort();

        let mut covered: Vec<String> = EXTERNAL_BACKWARD_SOURCE
            .iter()
            .map(|(name, _)| (*name).to_owned())
            .collect();
        covered.sort();

        assert_eq!(
            actual, covered,
            "crates/term-arena/src/*.rs drifted from EXTERNAL_BACKWARD_SOURCE; every arena \
             module is part of what the structured resolver decides and must be pinned"
        );
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
        let err = c
            .assert_matches("deadbeef")
            .expect_err("a mismatched pin must be a typed hard failure, not silently accepted");
        assert_eq!(
            gmeow_errors::code::code_str(err.code()),
            crate::error::ContractDrift::CODE,
            "engine-contract drift must surface as the dedicated logic.contract-drift kind"
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
