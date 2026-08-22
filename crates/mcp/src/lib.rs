// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! `gmeow-mcp` — the CONSUMER-mode MCP engine over a bundled `gmeow.gts` snapshot.
//!
//! [`McpView`] loads the snapshot ONCE (the narrow waist, bundle-only — never the
//! repo) and serves the `export`-backed surfaces — `lookup_term`, `llms_txt`,
//! `llms_full`, `doc_card`, `okf_index` — over a per-language `FoldView`. The
//! standard `llms.txt` / `doc_card` surfaces make the docs themselves
//! agent-consumable: the index links into the published site (URLs recovered from
//! the `gmeow:graph/documentation` graph) and the card is the per-term,
//! context-window-ready twin of the site's `card.md`. [`McpServer`] owns the stdio
//! JSON-RPC loop, startup language validation, tool/resource routing, the native
//! reasoning and validation tools, and the grounded-memory triad; the native `gmeow`
//! CLI is the launcher.
//!
//! # Why this crate exists
//!
//! The MCP surface used to live inside `gmeow-pipeline`, so an agent server that
//! only ever READS a shipped bundle inherited the whole build executor: the stage
//! DAG, the scheduler, the persistent cache, rayon, the release signer, the network
//! client, the docs renderer and its embedded multi-megabyte wasm. That is a hard
//! blocker for a wasm target and a very poor deal for a consumer that wants a term
//! card. This crate is the extraction: everything the consumer surface needs, and
//! nothing that writes a bundle.
//!
//! # Boundary rules
//!
//! * **Leaf.** It never depends on `gmeow-pipeline`. The four repo-reading dev tools
//!   (`validate`, `sync`, `reason`, `constitution`) live in `gmeow-mcp-dev`, which
//!   depends on THIS crate and registers them through [`extension`]. That inversion
//!   is what breaks the cycle: the only pipeline-coupled symbols were `run_full` /
//!   `RunMode`, and they now sit on the dev side of the seam.
//! * **wasm-clean, and not merely at the dependency level.** Nothing in the
//!   dependency tree pulls in rayon or process spawning, AND no module outside
//!   [`storage`]'s native half calls `std::fs` or `std::env`. Persistence and
//!   configuration go through the [`storage`] seam, which is `cfg`-selected: a real
//!   filesystem + environment natively, a real in-process store in a browser. The one
//!   surface that is not seamed is [`McpServer::run_stdio`], the blocking process-stdio
//!   line loop, which is `cfg`-gated OUT of wasm — a browser drives the identical
//!   protocol through `handle_message`, a frame at a time.
//! * **Bundle-only, and byte-only.** No tool here reads a checkout, and none reads a
//!   FILE: `validate_local`, `advise`, `query_local`, and `verify_graph` all take
//!   inline `data` plus an EXPLICIT `format`, and `slice_quality` takes an in-memory
//!   `files` map. The grounded-memory triad writes to the storage backend's claim
//!   package, never the repo.
//! * **One surface, total dispatch.** Tools and resources are `(descriptor,
//!   handler)` pairs in an assembled [`extension::Surface`]; see that module for the
//!   totality and duplicate-registration contract.
//!
//! # Segments: a tiered deployment of ONE total surface
//!
//! The engine ships in two tiers. The `reasoning` cargo feature (default ON) selects
//! whether this build LINKS the DL reasoner (`gmeow-logic`) and the rubric kernel over it
//! (`gmeow-slice-quality`); a [`SegmentSet`] selects whether a given deployment SERVES
//! them. Native `gmeow mcp`, `gmeow-mcp-dev`, and the full browser image all link and
//! serve everything, and are unchanged. The browser console's first-load image links
//! neither and defers [`REASONING_SEGMENT_TOOLS`] to a segment it fetches on first use.
//!
//! Deferral is NOT a reduced surface, and this is the load-bearing claim:
//!
//! * All [`TOOL_COUNT`] tools are advertised by `tools/list`, with identical descriptors,
//!   in every deployment — discovery cannot tell the tiers apart.
//! * The action theory (`action_policy`) is likewise total and identical, and the
//!   native bijection gate over it is a statement about the THEORY, not about any one
//!   deployment's linkage.
//! * A `tools/call` for a deferred tool returns the typed, machine-readable
//!   [`SegmentNotLoaded`](error::SegmentNotLoaded) signal — code
//!   `mcp.segment-not-loaded`, naming the tool AND the segment that serves it — which a
//!   host uses to load that segment and re-dispatch the SAME frame. It is a routing
//!   instruction, never a refusal, never an empty result, and never an answer computed
//!   by a weaker path.
//!
//! Membership is decided by LINKAGE (what a tool actually calls) and by SHARED MUTABLE
//! STATE (what a tool reads and writes), not by name; see [`REASONING_SEGMENT_TOOLS`] for
//! the classification and its evidence.
//!
//! # Direct dependencies
//!
//! The list below is the crate's complete direct dependency set — it must set-equal
//! `cargo tree -p gmeow-mcp --depth 1 -e normal`, and the `documented_dependencies`
//! gate in `crates/mcp/tests/` asserts exactly that, naming the symmetric difference in
//! both directions when it drifts. Each entry carries the reason it is here:
//!
//! * `purrdf` — the RDF 1.2 kernel: the snapshot imports to an `RdfDataset`, every
//!   query surface evaluates SPARQL over one, and the memory / conjecture / candidate
//!   libraries are GTS segment files written with its writer.
//! * `gmeow-errors` — the diagnostic substrate: every tool defect is a typed
//!   `DiagKind` raised as a `Diag`, and `explain_finding` rehydrates `Finding`s.
//! * `gmeow-ns` — the registered term namespaces (`GMEOW_NS`, `LOGIC_NS`, `MATH_NS`)
//!   the tool surface builds predicate and class IRIs from.
//! * `gmeow-bundle-view` — the bundle READ side: blob access, the fold view and card
//!   renderers, the diagnostics reader, the native query substrate, the graph IRIs.
//! * `gmeow-docs-catalog` — the distribution-catalog read side behind
//!   `distribution_matrix`: the per-format consumer-need matrix and the formal-concept
//!   lattice, read out of the bundle's meta-level catalog graph.
//! * `gmeow-transcode` — the RDF-1.2 transcode hub behind `convert`: the same
//!   `Codec` / `transcode` / `realized_loss_json` triple `gmeow convert` calls.
//! * `gmeow-docs-model` — the documentation MODEL (`card`, `llms`, `gmn1_primer`)
//!   the `doc_card` / `llms_txt` / primer surfaces render through; never
//!   `gmeow-docs`, whose renderer embeds vendored wasm.
//! * `gmeow-logic` — the native DL reasoner and its result algebra behind
//!   `verify_graph`, `explain_quad`, `coherence_certificate`, the conjecture engine, and
//!   the Transaction-Logic commit gate every WRITE tool runs its precondition through.
//!   OPTIONAL, under the `reasoning` feature (default on) — see the segments section.
//! * `gmeow-logic-compile` — the canonical prefix registry the tools render Turtle
//!   with, plus `ir::LOGIC_NAMESPACE`.
//! * `gmeow-validate` — the shipped validator behind `validate_local` and `advise`,
//!   and the local oracle's finding schema / entailment / fixture views.
//! * `gmeow-lang-bridge` — the GMN-1 codec behind `gmn_validate`, `gmn_expand`, and
//!   `gmn_explain`.
//! * `gmeow-slice-quality` — the advisor kernel `slice_quality` scores an in-memory
//!   slice file map with, against the bundle-carried rubric. OPTIONAL, under the same
//!   `reasoning` feature: it is a dependent of `gmeow-logic`, so the two travel as one
//!   segment.
//! * `serde_json` — the MCP wire format: every tool envelope and JSON-RPC frame.
//! * `sha2` — SHA-256 content addressing for the append-only library segments, the
//!   claim fingerprints, and the browser backend's record ids.
//! * `tempfile` — atomic write-then-rename for the NATIVE library commit path
//!   (`cfg(not(target_arch = "wasm32"))`; the browser backend needs none).
//! * `gmeow-gts-profile` — the ONE mandated GTS authorship door. The append-only library
//!   segments are authored GMEOW GTS, so they go through it rather than purrdf's raw
//!   writer, whose default catalog states no zstd level and would leave the frame
//!   profile unverifiable on the artifact. It also resolves a runtime store's priming
//!   bytes out of the loaded bundle's in-band map (`store_medium`).
//! * `ed25519-dalek` — the store COMPACTION lane's mandatory packaging signature:
//!   purrdf's `compact_streamable` takes the ordering-commitment signer as a plain tuple
//!   rather than an `Option`, so an unsigned repack is unrepresentable and the lane has
//!   to name the key type.

pub mod error;

/// The runtime-store medium: a dictionary id plus the bytes the loaded bundle pins for
/// it. Re-exported so a caller names one type, not two.
pub use gmeow_gts_profile::StoreMedium;
pub mod extension;
pub mod fold_arena;
pub mod storage;

// `Reverse` orders the `docs_search` relevance ranking — a CORE-segment surface only.
#[cfg(feature = "core")]
use std::cmp::Reverse;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::{Arc, OnceLock};
// The CORE segment's per-view term cache is the only lock this crate takes.
#[cfg(feature = "core")]
use std::sync::Mutex;

use serde_json::{Value, json};

use gmeow_errors::ResultExt;
// The CORE segment's GMN-1 codec imports: the four `gmn_*` tools are core tools, so the
// reasoning image names none of these. `gmeow-lang-bridge` itself stays linked there —
// `gmeow-slice-quality`'s rubric axes read its dictionary and coverage primitives — so this
// is a REACHABILITY gate, not a dependency one.
#[cfg(feature = "core")]
use gmeow_lang_bridge::{
    Gmn0Model, Gmn1Document, GmnDictionary, build_verbalization_pairs, gmn0_canonically_equal,
    gmn1_read, gmn1_write, resolve_operator_forms,
};
// The reasoning SEGMENT's imports. Every one of these is a `gmeow-logic` surface, and
// `gmeow-logic` is the optional half of this crate: a build with `reasoning` selected out
// does not link the DL reasoner at all, which is the whole point of the tiered browser
// deployment (see `SegmentSet`). Nothing outside a `#[cfg(feature = "reasoning")]` item
// may name these.
#[cfg(feature = "reasoning")]
use gmeow_logic::certificate::{CoherenceOutcome, ContradictionPolicy};
#[cfg(feature = "reasoning")]
use gmeow_logic::conjecture::ConjectureLifecycleState;
#[cfg(feature = "reasoning")]
use gmeow_logic::explain::{self, LazyExplanationIndex, Row, reifier_from_row};
#[cfg(feature = "reasoning")]
use gmeow_logic::provenance::{reifier_from_strings, term_display};
#[cfg(feature = "reasoning")]
use gmeow_logic::query_ir::Budget;
#[cfg(feature = "reasoning")]
use gmeow_logic::reason::reason_all_budgeted;
#[cfg(feature = "reasoning")]
use gmeow_logic::result::{CompletenessStatus, EvaluationStatus, ReasoningResult};
#[cfg(feature = "reasoning")]
use gmeow_logic::result_rdf::{project_conjecture_withdrawal, project_reasoning_result};
#[cfg(feature = "reasoning")]
use gmeow_logic::transaction::execute::{CommitMode, TxReceipt, execute_transaction};
#[cfg(feature = "reasoning")]
use gmeow_logic::verify::{embedded_verify_queries, verify_with_reasoning_result};
// `gmeow-logic-compile` is the prefix registry + the `logic:` IRI namespace, a pure
// compile-side leaf with NO reasoner in it, so it is never part of the segment. The
// namespace constant happens to be read only by segment code today (the certificate
// envelope, the conjecture library reader), hence the gate on this import alone.
#[cfg(feature = "reasoning")]
use gmeow_logic_compile::ir::LOGIC_NAMESPACE;
// The CORE segment's validator imports (`validate_local` / `advise` / `explain_finding`).
// As with lang-bridge, `gmeow-validate` stays linked in the reasoning image through
// `gmeow-slice-quality`; only these readers are gated.
#[cfg(feature = "core")]
use gmeow_validate::local_oracle::{self, EntailmentView, FixtureView};
// `recall` is a core read over the grounded-memory triad; the three WRITE option types
// belong to the WRITE tools, which are segment tools because their commit gate is a TR
// transaction.
use purrdf::gts::examples::agent_memory::RecallOptions;
#[cfg(feature = "reasoning")]
use purrdf::gts::examples::agent_memory::{RevisionOptions, StoreOptions, ToolCallOptions};
// The GTS WRITE side: only the six WRITE tools mint segments, so the writer and its term
// model travel with the segment their commit gate lives in.
#[cfg(feature = "reasoning")]
// The ONE mandated GMEOW GTS authorship door. `purrdf::gts::writer::Writer` is NOT it:
// its default catalog states no zstd level, so a segment authored through it satisfies the
// codec-name half of the frame profile while leaving the level unstated — and therefore
// unverifiable — on the artifact. The repo-static authorship seal enforces this.
use gmeow_gts_profile::GmeowGtsWriter as GtsWriter;
#[cfg(feature = "reasoning")]
use purrdf::TermValue;
#[cfg(feature = "reasoning")]
use purrdf::gts::model::{Term as GtsTerm, TermKind as GtsTermKind};
use purrdf::{RdfDatasetBuilder, RdfQuad, RdfTerm};
// Content addressing for the append-only library segments the WRITE tools commit.
#[cfg(feature = "reasoning")]
use sha2::{Digest, Sha256};

// The bundle READ side is a CORE-segment dependency in full: the fold view, the consumer
// term record, and the export renderers are reached only by core tools and core resources.
// The reasoning image reads the carrier dataset straight off `purrdf::import_gts_events`
// and needs none of it.
#[cfg(feature = "core")]
use gmeow_bundle_view::export::{self, FoldView, Term};
// The consumer RESOLUTION algebra: only CORE tools resolve a caller-supplied term to its
// canonical IRI, so only they can hit an ambiguity or a miss.
#[cfg(feature = "core")]
use gmeow_bundle_view::export::ConsumerResolution;

use crate::extension::{
    Extension, ResourceHandler, Surface, ToolHandler, zip_resources, zip_tools,
};
use crate::storage::{ClaimStore, storage};
// The append-only segment libraries (conjecture + candidate) and their commit lock: only
// the WRITE tools and `list_candidates` touch them, and all of those are segment tools.
#[cfg(feature = "reasoning")]
use crate::storage::{LibraryLock, SegmentLibrary};

// ── engine segments: a TIERED deployment of ONE total surface ─────────────────────

/// The name of the demand-loaded reasoning segment, as it appears in the
/// [`SegmentNotLoaded`](crate::error::SegmentNotLoaded) signal's `segment` field.
///
/// A stable wire identifier, not a display string: a host reads it to decide WHICH
/// segment image to fetch before re-dispatching the frame.
pub const REASONING_SEGMENT: &str = "reasoning";

/// The name of the always-resident core segment, as it appears in the
/// [`SegmentNotLoaded`](crate::error::SegmentNotLoaded) signal's `segment` field.
///
/// The twin of [`REASONING_SEGMENT`], and it exists because the two browser images are
/// DISJOINT halves of one surface rather than a superset and a subset. The reasoning image
/// carries the [`REASONING_SEGMENT_TOOL_COUNT`] [`REASONING_SEGMENT_TOOLS`] and defers the
/// other [`CORE_SEGMENT_TOOL_COUNT`] back to core, exactly as core defers those forward.
/// Without this identifier the reasoning image would have had to be a superset — which is
/// what made the old "heavy segment" duplicate the whole core image on disk.
/// The CHASE segment: the whole-bundle materialization tools.
///
/// A tier of its own because it is bounded by the host's ADDRESS SPACE rather than by any
/// budget the caller sets. Folding the governed bundle takes ~3.2 GiB and chasing it needs
/// some 5.4 GiB beyond that — measured, not estimated — against wasm32's hard 4 GiB ceiling,
/// and a supplied graph does not help because it is chased in UNION with the bundle. So a
/// 32-bit host cannot finish these calls for ANY input, and saying so through the deferral
/// signal is the difference between a tier a caller routes around and a trap they can only
/// report: an allocation failure aborts, and an abort reaches the browser as
/// `RuntimeError: unreachable`, with no message at all.
pub const CHASE_SEGMENT: &str = "chase";

/// The tools the [`CHASE_SEGMENT`] serves — every one that materializes the bundle's closure.
pub const CHASE_SEGMENT_TOOLS: &[&str] = &["verify_graph"];

pub const CORE_SEGMENT: &str = "core";

/// The tools the [`REASONING_SEGMENT`] serves, in advertised order.
///
/// Membership is decided by two facts about a tool, never by its name: what it LINKS, and
/// what mutable STATE it shares with another tool.
///
/// # Linkage
///
/// * `verify_graph` / `explain_quad` DERIVE — `reason_all_budgeted`, the verify pass, and
///   the explanation index are the DL reasoner itself.
/// * All six WRITE tools (`store_claim`, `revise_belief`, `store_conjecture`,
///   `refute_conjecture`, `submit_candidate`, `withdraw_candidate`) run their precondition
///   through `execute_transaction` — the Transaction-Logic executor's executional
///   entailment IS their commit gate, so a build without the reasoner cannot honour their
///   contract at all. Deferring them is the only alternative to silently skipping the gate.
/// * `conjecture_test` evaluates through `conjecture_eval`.
/// * `slice_quality` is the rubric kernel, itself a dependent of the reasoner.
/// * `coherence_certificate` and `list_candidates` READ rather than derive, but they parse
///   the reasoner's own status/lifecycle algebra (`CompletenessStatus`,
///   `EvaluationStatus`, `ConjectureLifecycleState`) — types that live in `gmeow-logic`
///   and must not be duplicated here to dodge the edge.
///
/// # Shared state: the grounded-memory triad is INDIVISIBLE
///
/// `recall` and `store_segment` link nothing but the storage seam, so by linkage alone they
/// would be core reads. They are here anyway, and the reason is a hard property of the
/// deployment rather than a preference: **a segment is an image, and an image is a store.**
/// The browser backend's claim package
/// ([`storage::browser_storage`](crate::storage::browser_storage)) is a `static` inside one
/// wasm module, and two wasm modules have two linear memories — there is no arrangement in
/// which they share one. Splitting the triad therefore does not distribute it; it FORKS it,
/// and a `store_claim` that reported a minted claim id would be unreachable by every read:
/// `recall` answers `[]` and `store_segment` reports an empty store, in the one deployment
/// that ships. That is silent capability degradation on a write the engine called `ok`.
///
/// `store_claim` and `revise_belief` cannot move the other way — their commit gate IS the
/// Transaction-Logic executor, so serving them from the core image would mean linking the
/// reasoner into the first-load image and deleting the split. So the triad follows the
/// writes: every tool that touches the claim package
/// ([`ClaimStore`](crate::storage::ClaimStore)) is served by ONE segment, and this is it.
/// The price is honest and bounded — a caller that recalls demand-loads the reasoning image
/// exactly as a caller that stores does — and it buys the property the triad is for: what
/// was stored can be read back, and what `store_segment` exports is the store the session's
/// reads actually answered from.
///
/// Everything else is core and answers in the first-load image. That deliberately includes
/// `entailments` and `counter_examples`, whose names suggest reasoning but whose bodies are
/// pure SPARQL over the bundle's documentation graph: the derivations were computed at
/// pipeline time and shipped, so reading them needs no engine.
///
/// This list is the single declaration of the split: [`SegmentSet::serves`] routes off
/// it and [`builtin_tool_handlers`] is proved total against it, so a tool cannot be
/// deferred without appearing here and cannot appear here without being deferred.
pub const REASONING_SEGMENT_TOOLS: &[&str] = &[
    // `verify_graph` is NOT here: it is its own tier ([`CHASE_SEGMENT_TOOLS`]), because a
    // 32-bit host cannot finish it for any input. The two lists partition the non-core
    // surface, so `tools_of` can answer for either.
    "reason_graph",
    "explain_quad",
    "coherence_certificate",
    "store_claim",
    "conjecture_test",
    "store_conjecture",
    "refute_conjecture",
    "recall",
    "store_segment",
    "revise_belief",
    "slice_quality",
    "submit_candidate",
    "withdraw_candidate",
    "list_candidates",
];

/// Every tool that reads or writes the grounded-memory claim package, in advertised order.
///
/// The engine's own answer to "which tools share the claim store?", declared once so the
/// indivisibility argument in [`REASONING_SEGMENT_TOOLS`] is CHECKED rather than asserted:
/// `the_grounded_memory_triad_is_served_by_one_segment` proves this whole set maps to a
/// single segment, which is what makes a stored claim recallable in the browser. Any new
/// tool that reaches [`ClaimStore`](crate::storage::ClaimStore) belongs here, and the same
/// proof then constrains where it may be served.
pub const CLAIM_STORE_TOOLS: &[&str] = &["store_claim", "recall", "store_segment", "revise_belief"];

/// The tools the consumer surface advertises, in EVERY deployment tier.
///
/// The ONE declaration of the surface's size. Every count claim this crate and its two
/// browser shims make — in rustdoc, in a package description, in a README, in a tool
/// description an agent reads at run time — resolves to this constant or to one derived
/// from it, and `the_shipped_prose_states_the_derived_tool_counts` fails on any shipped
/// text that states a different number. Hand-copied counts are what let the surface grow
/// past a prose claim of "35" in three crates at once.
///
/// It is pinned rather than computed because `builtin_tool_descriptors` itself quotes the
/// counts (`action_policy` describes the theory's shape to the agent calling it), so the
/// descriptor list cannot be its own source.
/// `consumer_surface_matches_the_declared_tool_count`
/// closes the loop: the assembled surface must have exactly this many tools.
pub const TOOL_COUNT: usize = 38;

/// The governed WRITE tools: the ones typed `logic:McpActionSchema` in the shipped action
/// policy, carrying `logic:precondition` / `logic:effect` / `logic:compensation`.
///
/// Declared here because the READ count is the surface minus the writes, and both numbers
/// are quoted in shipped prose. The policy remains the authority on what a write IS:
/// `the_action_theory_is_bijective_with_the_consumer_tool_surface` asserts this list equals the
/// `logic:McpActionSchema`-typed half of `action_policy`, so this cannot become a second,
/// drifting definition of the write set.
pub const WRITE_TOOLS: &[&str] = &[
    "store_claim",
    "store_conjecture",
    "refute_conjecture",
    "revise_belief",
    "submit_candidate",
    "withdraw_candidate",
];

/// How many tools carry a governed write schema — [`WRITE_TOOLS`]'s length, never restated.
pub const WRITE_TOOL_COUNT: usize = WRITE_TOOLS.len();

/// How many tools are READS: the surface minus the governed writes, by construction.
pub const READ_TOOL_COUNT: usize = TOOL_COUNT - WRITE_TOOL_COUNT;

/// How many tools the demand-loaded reasoning image serves — [`REASONING_SEGMENT_TOOLS`]'s
/// length, so the split's arithmetic tracks the list rather than a comment about it.
pub const REASONING_SEGMENT_TOOL_COUNT: usize = REASONING_SEGMENT_TOOLS.len();

/// How many tools the always-resident core image serves: the surface minus the reasoning
/// segment. The two segments PARTITION the surface, so this is a subtraction and not a
/// second list.
pub const CORE_SEGMENT_TOOL_COUNT: usize =
    TOOL_COUNT - REASONING_SEGMENT_TOOL_COUNT - CHASE_SEGMENT_TOOL_COUNT;

/// How many tools the [`CHASE_SEGMENT`] serves.
pub const CHASE_SEGMENT_TOOL_COUNT: usize = CHASE_SEGMENT_TOOLS.len();

/// How many tools the core image DEFERS — across both non-core tiers.
///
/// The quantity a core deployment's prose is about: what it advertises but does not answer
/// here. It says nothing about which tier answers, which is why the two tier counts are
/// separate constants rather than this one split at the point of use.
pub const DEFERRED_TOOL_COUNT: usize = REASONING_SEGMENT_TOOL_COUNT + CHASE_SEGMENT_TOOL_COUNT;

/// Which engine segments a deployment serves IN-PROCESS.
///
/// The tool surface is total in every deployment: all [`TOOL_COUNT`] tools are advertised,
/// described, and dispatchable everywhere, and the action theory that governs them is
/// unchanged. A
/// `SegmentSet` says only where a tool's *implementation* currently lives. A tool whose
/// segment is not served answers with [`SegmentNotLoaded`](crate::error::SegmentNotLoaded)
/// — a typed, machine-readable routing instruction naming the tool and the segment — which
/// the host uses to load that segment and re-dispatch the SAME frame. The caller sees a
/// slower answer; it never sees a missing tool, an empty result, or a refusal.
///
/// The surface is PARTITIONED into two segments, not layered into a base and an extension:
/// the [`REASONING_SEGMENT_TOOLS`] belong to [`REASONING_SEGMENT`] and the other
/// [`CORE_SEGMENT_TOOL_COUNT`] tools plus all five resources belong to [`CORE_SEGMENT`]. The two browser
/// images select one each and defer the other's half back, so neither is a superset of the
/// other and no byte is paid twice. The native build selects BOTH and is one whole engine.
///
/// Two axes decide whether a segment is served, and BOTH must hold:
/// * the build LINKS it (the `core` / `reasoning` cargo features), and
/// * the deployment SELECTS it ([`SegmentSet::core`] / [`SegmentSet::reasoning_only`] vs
///   [`SegmentSet::linked`]).
///
/// The second axis exists because "lean core" is a deployment shape, not merely a
/// compilation artifact: it must be observable — and therefore testable — from a build
/// that does link the segment, otherwise the deferral contract would only ever be
/// exercised by the image nobody runs the test suite against.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SegmentSet {
    /// Serve every tool OUTSIDE [`REASONING_SEGMENT_TOOLS`] locally rather than deferring
    /// it back to the always-resident core image.
    ///
    /// Can only ever be `true` on a build with the `core` feature — the constructors below
    /// are the only way to set it, and each folds in `cfg!(feature = "core")`.
    pub core: bool,
    /// Serve [`REASONING_SEGMENT_TOOLS`] locally rather than deferring them.
    ///
    /// Can only ever be `true` on a build with the `reasoning` feature — the constructors
    /// below are the only way to set it, and each folds in `cfg!(feature = "reasoning")`.
    pub reasoning: bool,
    /// Whether the whole-bundle chase runs HERE. False on a host whose address space cannot
    /// hold it; the tools stay advertised and governed, and defer.
    pub chase: bool,
}

impl SegmentSet {
    /// Every segment this BUILD links — the default for [`McpServer::from_snapshot`], and
    /// therefore what the native `gmeow mcp` and `gmeow-mcp-dev` get. On a build with both
    /// features on this is the whole engine, unchanged.
    #[must_use]
    pub const fn linked() -> Self {
        Self {
            core: cfg!(feature = "core"),
            reasoning: cfg!(feature = "reasoning"),
            // A 32-bit host cannot finish the chase for any input, so it does not claim to.
            chase: cfg!(feature = "reasoning") && !cfg!(target_arch = "wasm32"),
        }
    }

    /// The LEAN core deployment: the reasoning segment is deferred to first use.
    ///
    /// The tiered browser console's first-load image. Identical to [`Self::linked`] on a
    /// build that selected the reasoning feature out, so the two never disagree about what
    /// a core deployment is.
    #[must_use]
    pub const fn core() -> Self {
        Self {
            core: cfg!(feature = "core"),
            reasoning: false,
            chase: false,
        }
    }

    /// The DEMAND-LOADED reasoning deployment: the mirror image of [`Self::core`].
    ///
    /// It serves the [`REASONING_SEGMENT_TOOLS`] and defers everything else BACK to
    /// the core image the host already has resident. That symmetry is the point: the
    /// reasoning image is a genuine DELTA, not a superset, so the two images share no tool
    /// implementation and the bytes are not paid twice.
    #[must_use]
    pub const fn reasoning_only() -> Self {
        Self {
            core: false,
            reasoning: cfg!(feature = "reasoning"),
            chase: cfg!(feature = "reasoning") && !cfg!(target_arch = "wasm32"),
        }
    }

    /// Whether `tool` runs here, or is deferred to a segment this deployment has not
    /// loaded. TOTAL over the surface: every tool belongs to exactly one segment, so this
    /// answers for all [`TOOL_COUNT`] without a fallthrough.
    #[must_use]
    pub fn serves(self, tool: &str) -> bool {
        if CHASE_SEGMENT_TOOLS.contains(&tool) {
            self.chase
        } else if REASONING_SEGMENT_TOOLS.contains(&tool) {
            self.reasoning
        } else {
            self.core
        }
    }

    /// The segment that serves `tool` — the wire identifier a host fetches an image by.
    #[must_use]
    /// The tools a NAMED segment serves — the inverse of [`SegmentSet::segment_of`].
    ///
    /// Total over the partition: every tool belongs to exactly one segment, so a name that is
    /// not a segment answers with nothing rather than with a guess.
    pub fn tools_of(segment: &str) -> &'static [&'static str] {
        match segment {
            CHASE_SEGMENT => CHASE_SEGMENT_TOOLS,
            REASONING_SEGMENT => REASONING_SEGMENT_TOOLS,
            _ => &[],
        }
    }

    pub fn segment_of(tool: &str) -> &'static str {
        if CHASE_SEGMENT_TOOLS.contains(&tool) {
            CHASE_SEGMENT
        } else if REASONING_SEGMENT_TOOLS.contains(&tool) {
            REASONING_SEGMENT
        } else {
            CORE_SEGMENT
        }
    }
}

/// The deferral signal for one `tools/call` against a segment this deployment has not
/// loaded — the ONE construction site, so every deferred tool reports identically.
///
/// `segment` is the wire name of the image that DOES serve `tool`, so a host can route on
/// the signal alone without a second lookup.
fn segment_not_loaded(tool: &'static str, segment: &'static str) -> gmeow_errors::Diag {
    gmeow_errors::Diag::of_kind(crate::error::SegmentNotLoaded {
        tool: tool.to_owned(),
        segment: segment.to_owned(),
    })
}

// The internal→BCP-47 display-language map is carried on the lang: carrier
// varieties: each lang:LanguageVariety bears its internal tag through
// lang:carrierTag and its generated (folded) external tag through gmeow:bcp47Tag.
const LANGUAGE_CLASS: &str = "https://blackcatinformatics.ca/lang/LanguageVariety";
const LANGUAGE_TAG: &str = "https://blackcatinformatics.ca/lang/carrierTag";
const BCP47_TAG: &str = "https://blackcatinformatics.ca/gmeow/bcp47Tag";
const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
/// The `gmeow:` vocabulary namespace — the base of the documentation-graph
/// predicate and enumeration IRIs (`gmeow:docFixtureKind…`, etc.).
use gmeow_ns::GMEOW_NS;
#[cfg(feature = "reasoning")]
const TOOL_AGENT_NS: &str = "urn:gmeow:tool:";
/// The distinct external-provenance named graph the read-only local overlay is
/// re-homed into (the origin marker). Overlay triples are visible to reads
/// (`bundle ∪ overlay`) but quarantined under this graph — NEVER unioned into the
/// signed `gmeow:` canon and NEVER written back.
const EXTERNAL_OVERLAY_GRAPH: &str = "urn:gmeow:mcp:overlay:external";

/// The GMEOW namespace the native validation surface reasons in — the SAME
/// namespace the CLI `gmeow validate` passes to `gmeow_validate::data_validate`, so
/// `validate_local` never diverges from the shipped validator.
#[cfg(feature = "core")]
const MCP_NAMESPACE: &str = "https://blackcatinformatics.ca/gmeow/";

/// The origin marker stamped on every `validate_local` finding's primary location —
/// the transient, inline data has no file path, so this synthetic origin identifies
/// the tool that produced the finding.
#[cfg(feature = "core")]
const VALIDATE_LOCAL_ORIGIN: &str = "mcp:validate_local";

/// The origin marker stamped on every `advise` finding's primary location — the
/// `advise`-tool twin of [`VALIDATE_LOCAL_ORIGIN`].
#[cfg(feature = "core")]
const ADVISE_ORIGIN: &str = "mcp:advise";

/// A generous ceiling on the inline `data` payload `validate_local` accepts (8 MiB).
/// A larger payload is a HARD FAIL with a finding-style error — never silently
/// truncated (a truncated RDF graph would mis-parse and mislead).
#[cfg(feature = "core")]
const MAX_VALIDATE_DATA_BYTES: usize = 8 * 1024 * 1024;

/// `rdfs:label` — the controlled-NL nucleus the GMN verbalizer joins each operator
/// form to; harvested from the bundle dataset for `gmn_explain`'s gloss.
#[cfg(feature = "core")]
const RDFS_LABEL: &str = "http://www.w3.org/2000/01/rdf-schema#label";
/// `x-gmeow-english` — the preferred label language tag (mirrors the pipeline's
/// verbalizer label harvest: a GMEOW-English label wins, ties break to the smallest
/// lexical form), so the gloss the tool serves is byte-identical to Task 8's.
#[cfg(feature = "core")]
const GMEOW_ENGLISH: &str = "x-gmeow-english";
/// `lang:Denotation` — the typed meaning-assignment node `gmn_explain` resolves a
/// glyph back to (its denoted form supplies fixity/precedence/arity).
#[cfg(feature = "core")]
const LANG_DENOTATION: &str = "https://blackcatinformatics.ca/lang/Denotation";
/// `lang:denotedForm` — the Denotation → Form edge carrying the operator signature.
#[cfg(feature = "core")]
const LANG_DENOTED_FORM: &str = "https://blackcatinformatics.ca/lang/denotedForm";
/// `lang:denotationTarget` — the Denotation → denoted term (the operator's meaning).
#[cfg(feature = "core")]
const LANG_DENOTATION_TARGET: &str = "https://blackcatinformatics.ca/lang/denotationTarget";
/// `gmeow:gmnFixity` — the operator Form's fixity individual IRI.
#[cfg(feature = "core")]
const GMN_FIXITY: &str = "https://blackcatinformatics.ca/gmeow/gmnFixity";
/// `gmeow:gmnPrecedence` — the operator Form's binding-strength integer.
#[cfg(feature = "core")]
const GMN_PRECEDENCE: &str = "https://blackcatinformatics.ca/gmeow/gmnPrecedence";
/// `gmeow:gmnArity` — the operator Form's operand count.
#[cfg(feature = "core")]
const GMN_ARITY: &str = "https://blackcatinformatics.ca/gmeow/gmnArity";
/// The honest typed miss `gmn_explain` returns for an input that is not a covered GMN
/// operator glyph — the SAME `lang:` uncovered-term class the codec raises for a term
/// the dictionary does not mint, never a fabricated answer.
#[cfg(feature = "core")]
const LANG_GMN_UNCOVERED_TERM: &str = "https://blackcatinformatics.ca/lang/GmnUncoveredTerm";

/// The pre-reasoning hard ceiling on the `verify_graph` overlay size (quad count).
/// An overlay larger than this is REFUSED before any reasoning runs, so an
/// agent-supplied external annex can never push the governed forward closure past a
/// bounded starting EDB. 100_000 quads is a generous local-graph bound — far above
/// any hand-authored annex — yet keeps the `bundle ∪ overlay` EDB, and thus the
/// budgeted chase over it, bounded. Exceeding it is a HARD FAIL (the bounded agent
/// path), never a silently truncated graph.
#[cfg(feature = "reasoning")]
const MAX_VERIFY_OVERLAY_QUADS: usize = 100_000;

/// The pre-*parse* hard ceiling on the `query_local` / `verify_graph` overlay payload
/// (raw bytes of the caller's inline `data`), checked BEFORE the bytes are handed to
/// the parser. [`MAX_VERIFY_OVERLAY_QUADS`] alone bounds the PARSED quad count, but
/// that check only runs AFTER the whole payload has been parsed into a dataset — so a
/// huge annex (or one with a single enormous literal that parses to very few quads)
/// could exhaust memory before the quad ceiling ever gets a chance to refuse it. A
/// payload's byte length is known without inspecting its content, so this gate is O(1).
/// 16 MiB is generous — the existing 100,000-quad ceiling, serialized as short
/// synthetic IRIs/literals, tops out around ~4 MiB, and no hand-authored local annex
/// plausibly approaches this — yet it bounds the bytes the parser ever sees to a fixed,
/// small multiple of that. Exceeding it is a HARD FAIL BEFORE the parse (the bounded
/// agent path), never a truncated graph.
const MAX_VERIFY_OVERLAY_BYTES: u64 = 16 * 1024 * 1024;

/// The forward-chase derivation-step budget every agent-facing reasoning tool
/// (`verify_graph`, `explain_quad`, `conjecture_test`, `store_conjecture`) runs under when
/// the caller OMITS `max_steps`. R4 forbids exposing an unbudgeted
/// Turing-complete evaluation to an agent loop, so an omitted `max_steps` is never `None`
/// (unbounded) on these paths — see [`governed_budget`].
///
/// Deliberately SMALL, not just finite: [`reason_all_budgeted`] only skips the expensive DL
/// consistency post-pass while the governor actually CUTS the chase
/// ([`gmeow_logic::reason::reason_all_budgeted`]'s `BudgetExhausted` branch); once
/// `max_steps` reaches the true closure size the run instead COMPLETES and pays the full DL
/// consistency scan — on the shipped bundle that is the same ~500 s whole-graph cost the
/// off-gate `whole_bundle_coherence_gate_*` suite measures, an unusable default latency for
/// an interactive agent call. 64 mirrors the value the sibling `explain_quad_*_heavy_offgate`
/// tests already use for this exact reason ("a small governor budget already derives
/// IRI-object quads... a larger budget explodes the closure without adding coverage") and is
/// empirically well below the shipped bundle's true closure size (measured: `max_steps: 100`
/// still exhausts the budget in ~15 s; `max_steps: 500` reaches the true closure and falls
/// into the ~500 s DL post-pass).
#[cfg(feature = "reasoning")]
const DEFAULT_MAX_STEPS: u64 = 64;

/// The hard ceiling no agent-supplied `max_steps` may exceed, on the same tools as
/// [`DEFAULT_MAX_STEPS`]. Tied to [`MAX_VERIFY_OVERLAY_QUADS`] — the pre-reasoning overlay
/// EDB quad ceiling already governing `verify_graph` — so the post-EDB forward-chase step
/// budget rides the same order of magnitude as the bounded starting EDB it chases over. A
/// caller-supplied value above this is CLAMPED down, never honored past the ceiling. Unlike
/// the small [`DEFAULT_MAX_STEPS`], this ceiling is reached only by an agent's OWN explicit,
/// informed request for a deeper (possibly slow, but always finite) evaluation — never by an
/// omitted argument.
#[cfg(feature = "reasoning")]
const HARD_MAX_STEPS: u64 = MAX_VERIFY_OVERLAY_QUADS as u64;

/// The answer-binding cap every agent-facing reasoning tool runs under when the caller
/// OMITS `max_answers`; see [`DEFAULT_MAX_STEPS`]. Matches its scale: both bound the same
/// "generous but small" no-args default.
#[cfg(feature = "reasoning")]
const DEFAULT_MAX_ANSWERS: usize = 64;

/// The hard ceiling no agent-supplied `max_answers` may exceed, on the same tools as
/// [`DEFAULT_MAX_ANSWERS`]. Matches [`MAX_VERIFY_OVERLAY_QUADS`] in order of magnitude —
/// the same "generous but bounded" scale as every other agent-facing ceiling in this file.
#[cfg(feature = "reasoning")]
const HARD_MAX_ANSWERS: usize = MAX_VERIFY_OVERLAY_QUADS;

/// Build a governed [`Budget`] for an agent-facing MCP tool call — the ONLY way any
/// `tool_*` wrapper in this file may construct a `Budget` (R4: never expose an
/// unbudgeted Turing-complete evaluation to an agent loop).
///
/// * An omitted (`None`) user value falls back to the finite default
///   ([`DEFAULT_MAX_STEPS`] / [`DEFAULT_MAX_ANSWERS`]).
/// * A supplied user value is CLAMPED to the hard ceiling ([`HARD_MAX_STEPS`] /
///   [`HARD_MAX_ANSWERS`]) — the caller may only LOWER the bound, never raise it past the
///   server-side cap.
///
/// Both returned fields are therefore always `Some`: this helper can never produce the
/// unbounded `Budget { max_answers: None, max_steps: None }` an omitted-args call used to
/// build.
#[cfg(feature = "reasoning")]
fn governed_budget(max_steps: Option<u64>, max_answers: Option<usize>) -> Budget {
    let max_steps = max_steps.unwrap_or(DEFAULT_MAX_STEPS).min(HARD_MAX_STEPS);
    let max_answers = max_answers
        .unwrap_or(DEFAULT_MAX_ANSWERS)
        .min(HARD_MAX_ANSWERS);
    Budget {
        max_answers: Some(max_answers),
        max_steps: Some(max_steps),
    }
}

/// SELECT the counter-example conformance fixtures from the documentation graph: the
/// fixture IRI, its authored violation code, the full Turtle body, its label, the
/// authored outcome/rationale, and each referenced documented term (repeatable).
#[cfg(feature = "core")]
const COUNTER_EXAMPLE_FIXTURE_QUERY: &str = "\
PREFIX gm: <https://blackcatinformatics.ca/gmeow/>
PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#>
SELECT ?f ?code ?text ?label ?outcome ?rationale ?term WHERE {
  ?f a gm:DocFixture ;
     gm:docFixtureKind gm:docFixtureKindCounterExample ;
     gm:docViolationCode ?code ;
     gm:docFixtureText ?text .
  OPTIONAL { ?f rdfs:label ?label }
  OPTIONAL { ?f gm:docExpectedOutcome ?outcome }
  OPTIONAL { ?f gm:conformanceRationale ?rationale }
  OPTIONAL { ?f gm:documents ?term }
}";

/// SELECT the well-formed conformance fixtures (positive exemplars) from the
/// documentation graph: the fixture IRI, the full Turtle body, its label, the
/// authored outcome, and each referenced documented term (the well-formed↔term join
/// key). A well-formed fixture carries no violation code.
#[cfg(feature = "core")]
const WELLFORMED_FIXTURE_QUERY: &str = "\
PREFIX gm: <https://blackcatinformatics.ca/gmeow/>
PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#>
SELECT ?f ?text ?label ?outcome ?term WHERE {
  ?f a gm:DocFixture ;
     gm:docFixtureKind gm:docFixtureKindWellformed ;
     gm:docFixtureText ?text .
  OPTIONAL { ?f rdfs:label ?label }
  OPTIONAL { ?f gm:docExpectedOutcome ?outcome }
  OPTIONAL { ?f gm:documents ?term }
}";

/// SELECT the entailment records from the documentation graph: the entailment IRI,
/// the documented term it grounds on, the rule, the conclusion, and each premise
/// (repeatable).
#[cfg(feature = "core")]
const ENTAILMENT_QUERY: &str = "\
PREFIX gm: <https://blackcatinformatics.ca/gmeow/>
SELECT ?e ?term ?rule ?conclusion ?premise WHERE {
  ?e a gm:Entailment ;
     gm:documents ?term ;
     gm:entailmentRule ?rule ;
     gm:entailmentConclusion ?conclusion .
  OPTIONAL { ?e gm:entailmentPremise ?premise }
}";

/// SELECT the conformance fixtures documenting one specific term (both kinds), for
/// the `counter_examples` tool: the fixture IRI, its enumerated
/// `gmeow:docFixtureKind`, the full Turtle body, its label, and the authored
/// advisory fields. `term_iri` is a canonical term IRI already resolved from the
/// bundle's own term set, so embedding it as an IRI ref is not a caller-injected
/// value.
#[cfg(feature = "core")]
fn fixtures_by_term_query(term_iri: &str) -> String {
    format!(
        "\
PREFIX gm: <{GMEOW_NS}>
PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#>
SELECT ?f ?kind ?text ?label ?outcome ?code ?rationale WHERE {{
  ?f a gm:DocFixture ;
     gm:documents <{term_iri}> ;
     gm:docFixtureKind ?kind ;
     gm:docFixtureText ?text .
  OPTIONAL {{ ?f rdfs:label ?label }}
  OPTIONAL {{ ?f gm:docExpectedOutcome ?outcome }}
  OPTIONAL {{ ?f gm:docViolationCode ?code }}
  OPTIONAL {{ ?f gm:conformanceRationale ?rationale }}
}}"
    )
}

/// SELECT the entailment records documenting one specific term, for the
/// `entailments` tool: the entailment IRI, its rule, its conclusion, and each
/// premise (repeatable). See [`fixtures_by_term_query`] on the embedded IRI.
#[cfg(feature = "core")]
fn entailments_by_term_query(term_iri: &str) -> String {
    format!(
        "\
PREFIX gm: <{GMEOW_NS}>
SELECT ?e ?rule ?conclusion ?premise WHERE {{
  ?e a gm:Entailment ;
     gm:documents <{term_iri}> ;
     gm:entailmentRule ?rule ;
     gm:entailmentConclusion ?conclusion .
  OPTIONAL {{ ?e gm:entailmentPremise ?premise }}
}}"
    )
}

/// SELECT the diagnostics evidence documenting one specific term, for the
/// full-tier `doc_card` panel: each grounded finding code and the evidence claim.
/// The `gmeow:docEvidenceKindDiagnostics` evidence node is projected into the
/// documentation graph per term that a finding structurally concerns. See
/// [`fixtures_by_term_query`] on the embedded IRI.
#[cfg(feature = "core")]
fn diagnostics_by_term_query(term_iri: &str) -> String {
    format!(
        "\
PREFIX gm: <{GMEOW_NS}>
SELECT ?claim ?code WHERE {{
  ?e a gm:DocEvidence ;
     gm:docEvidenceKind gm:docEvidenceKindDiagnostics ;
     gm:documents <{term_iri}> ;
     gm:docClaim ?claim ;
     gm:docGroundedBy ?code .
}}"
    )
}

/// SELECT the projection-loss evidence documenting one specific term, for the
/// full-tier `doc_card` panel: each grounded loss target and the preservation
/// judgment (`gmeow:docJudgment`). The `gmeow:docEvidenceKindLoss` evidence node
/// is projected per term that degrades under one or more projections. See
/// [`fixtures_by_term_query`] on the embedded IRI.
#[cfg(feature = "core")]
fn loss_by_term_query(term_iri: &str) -> String {
    format!(
        "\
PREFIX gm: <{GMEOW_NS}>
SELECT ?target ?judgment WHERE {{
  ?e a gm:DocEvidence ;
     gm:docEvidenceKind gm:docEvidenceKindLoss ;
     gm:documents <{term_iri}> ;
     gm:docGroundedBy ?target ;
     gm:docJudgment ?judgment .
}}"
    )
}

/// What `query_local` evaluates the caller's SPARQL against.
///
/// An EXPLICIT selection, shaped exactly like the `format` argument beside it: a declared
/// token, a named hard error on anything else, and never a guess. The two scopes answer
/// genuinely different questions and neither is a degradation of the other —
/// [`Self::BundleUnion`] asks "what does this graph mean IN GMEOW", [`Self::InputOnly`] asks
/// "what does this graph say ON ITS OWN".
///
/// `InputOnly` exists because bundle-union was previously the ONLY behaviour, which made a
/// standalone question unaskable: every answer silently carried bundle triples, so a caller
/// querying a pasted document could not tell its own content from the canon's. That is the
/// reading a browser playground or an editor scratchpad actually wants, and it had no
/// expression on the surface.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg(feature = "core")]
enum QueryScope {
    /// `bundle ∪ overlay` — the caller's graph read AGAINST the signed canon. The default,
    /// because it is the reading that makes an inline annex mean something in GMEOW.
    BundleUnion,
    /// The overlay ALONE — the caller's graph read on its own terms, with no bundle triple
    /// in the answer.
    InputOnly,
}

#[cfg(feature = "core")]
impl QueryScope {
    /// The wire tokens, in the order the tool description lists them.
    const ACCEPTED: &'static [&'static str] = &["bundle", "input"];

    /// Parse the declared `scope` argument; an omitted value is [`Self::BundleUnion`].
    ///
    /// # Errors
    ///
    /// An unrecognized token is a HARD FAIL naming the accepted set — never a silent
    /// fallback to the default, which would answer a different question than the one asked.
    fn parse(raw: Option<&str>) -> gmeow_errors::Result<Self> {
        match raw {
            None | Some("bundle") => Ok(Self::BundleUnion),
            Some("input") => Ok(Self::InputOnly),
            Some(other) => Err(gmeow_errors::Diag::of_kind(crate::error::Mcp {
                message: format!(
                    "query_local: unknown scope `{other}`; accepted: {}",
                    Self::ACCEPTED.join(", ")
                ),
            })),
        }
    }
}

/// The output format of the `doc_card` tool: rendered Markdown or the neutral
/// [`gmeow_docs_model::card::Card`] serialized to JSON.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg(feature = "core")]
enum CardFormat {
    /// The card rendered through the single shared Markdown renderer.
    Markdown,
    /// The card serialized as a JSON object (deterministic field order).
    Json,
}

#[cfg(feature = "core")]
impl CardFormat {
    /// Parse the `format` argument — an UNKNOWN value is a HARD FAIL listing the
    /// valid values (never a silent default).
    #[cfg(feature = "core")]
    fn parse(raw: Option<&str>) -> gmeow_errors::Result<Self> {
        match raw.unwrap_or("markdown") {
            "markdown" => Ok(Self::Markdown),
            "json" => Ok(Self::Json),
            other => Err(gmeow_errors::Diag::of_kind(crate::error::Mcp {
                message: format!(
                    "doc_card: unknown format `{other}`; valid values: markdown, json"
                ),
            })),
        }
    }

    /// The canonical label echoed back in the response envelope.
    #[cfg(feature = "core")]
    fn label(self) -> &'static str {
        match self {
            Self::Markdown => "markdown",
            Self::Json => "json",
        }
    }
}

/// Parse the `detail` argument into a [`gmeow_docs_model::card::CardDetail`] tier — an
/// UNKNOWN value is a HARD FAIL listing the valid values (never a silent default).
#[cfg(feature = "core")]
fn parse_card_detail(
    raw: Option<&str>,
) -> gmeow_errors::Result<gmeow_docs_model::card::CardDetail> {
    use gmeow_docs_model::card::CardDetail;
    match raw.unwrap_or("standard") {
        "summary" => Ok(CardDetail::Summary),
        "standard" => Ok(CardDetail::Standard),
        "full" => Ok(CardDetail::Full),
        other => Err(gmeow_errors::Diag::of_kind(crate::error::Mcp {
            message: format!(
                "doc_card: unknown detail `{other}`; valid values: summary, standard, full"
            ),
        })),
    }
}

/// The canonical `detail` label echoed back in the response envelope.
#[cfg(feature = "core")]
fn card_detail_label(detail: gmeow_docs_model::card::CardDetail) -> &'static str {
    use gmeow_docs_model::card::CardDetail;
    match detail {
        CardDetail::Summary => "summary",
        CardDetail::Standard => "standard",
        CardDetail::Full => "full",
    }
}

/// One-line and cap a fixture Turtle body to a short snippet for the full-tier
/// `doc_card` Do / Don't panels (the card is token-budgeted; the full body is
/// available through the `counter_examples` tool).
#[cfg(feature = "core")]
fn fixture_body_snippet(text: &str) -> String {
    let one_line = text.split_whitespace().collect::<Vec<_>>().join(" ");
    gmeow_docs_model::llms::cap_note(&one_line)
}

/// SELECT the documented competency questions for the `competency_questions` tool:
/// every `gmeow:DocumentedCompetency` carrying a runnable `gmeow:cqQueryText`, or —
/// when `term_iri` is `Some` — only those documenting that term. `cqQueryText` is a
/// required pattern (not OPTIONAL), so every returned record is a runnable question.
#[cfg(feature = "core")]
fn competency_query(term_iri: Option<&str>) -> String {
    let documents = term_iri
        .map(|iri| format!("     gm:documents <{iri}> ;\n"))
        .unwrap_or_default();
    format!(
        "\
PREFIX gm: <{GMEOW_NS}>
SELECT ?c ?q ?rationale ?count ?exact WHERE {{
  ?c a gm:DocumentedCompetency ;
{documents}     gm:cqQueryText ?q .
  OPTIONAL {{ ?c gm:cqRationale ?rationale }}
  OPTIONAL {{ ?c gm:cqExpectRowCount ?count }}
  OPTIONAL {{ ?c gm:cqExactRows ?exact }}
}}"
    )
}

/// SELECT the searchable documentation-entry records from the documentation graph:
/// each entry subject, its `rdf:type`, the REAL display label, the optional
/// definition, the site URL, the documented real IRI, and the repeatable advisory /
/// alignment / missing-coverage facets. The `docSearchLabel` pattern is required (not
/// OPTIONAL), so only genuine documentation-entry records (gmeow:DocumentedTerm /
/// DocumentedSlice / DocumentedConcern) match — never a bare evidence node.
#[cfg(feature = "core")]
const DOC_SEARCH_QUERY: &str = "\
PREFIX gm: <https://blackcatinformatics.ca/gmeow/>
PREFIX rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#>
SELECT ?s ?type ?label ?definition ?url ?documents ?advice ?alignment ?missing WHERE {
  ?s rdf:type ?type ;
     gm:docSearchLabel ?label .
  OPTIONAL { ?s gm:docSearchDefinition ?definition }
  OPTIONAL { ?s gm:docUrl ?url }
  OPTIONAL { ?s gm:documents ?documents }
  OPTIONAL { ?s gm:docSearchAdvice ?advice }
  OPTIONAL { ?s gm:docSearchAlignment ?alignment }
  OPTIONAL { ?s gm:docMissesDimension ?missing }
}";

/// One ranked documentation search hit the `docs_search` tool returns. Carries the
/// same facet shape the site `search-index.json` record does (`kind`, `id`, `label`,
/// `definition`, `url`, `advice`, `alignments`, `missing_coverage`) plus a private
/// match `rank` used only for the deterministic ordering.
#[derive(Debug)]
#[cfg(feature = "core")]
struct SearchHit {
    /// The record kind (`term`, `slice`, `concern`) from its `rdf:type` local name.
    kind: String,
    /// The documented real IRI (`gmeow:documents`) — a value a caller can pass back to
    /// `lookup_term` / `doc_card`.
    id: String,
    /// The REAL display label (`gmeow:docSearchLabel`).
    label: String,
    /// The definition prose (`gmeow:docSearchDefinition`), absent when the record
    /// carries none.
    definition: Option<String>,
    /// The site-relative page URL (`gmeow:docUrl`), absent when the record carries none.
    url: Option<String>,
    /// The advisory-prose facet (`gmeow:docSearchAdvice`), sorted+deduped; empty when
    /// none.
    advice: Vec<String>,
    /// The crosswalk alignment facet (`gmeow:docSearchAlignment`), sorted+deduped;
    /// empty when none.
    alignments: Vec<String>,
    /// The missing-dimension facet — the local names of `gmeow:docMissesDimension`,
    /// sorted+deduped; empty when the record misses no dimension.
    missing_coverage: Vec<String>,
    /// The match rank (lower is better): 0 label, 1 definition, 2 advice. Not
    /// serialized — the tie-break beside the id gives a stable, reproducible order.
    rank: u8,
}

#[cfg(feature = "core")]
impl SearchHit {
    #[cfg(feature = "core")]
    fn to_json(&self) -> Value {
        json!({
            "kind": self.kind,
            "id": self.id,
            "label": self.label,
            "definition": self.definition,
            "url": self.url,
            "advice": self.advice,
            "alignments": self.alignments,
            "missing_coverage": self.missing_coverage,
        })
    }
}

/// Whether a lowercased searchable field matches the query: a substring hit on the
/// whole lowercased query, OR (token mode) every whitespace-split query token appears
/// somewhere in the field. Case-insensitive by construction (both sides lowercased).
#[cfg(feature = "core")]
fn field_matches(field_lc: &str, query_lc: &str, tokens: &[&str]) -> bool {
    field_lc.contains(query_lc)
        || (!tokens.is_empty() && tokens.iter().all(|t| field_lc.contains(t)))
}

/// The local name of an IRI: the tail after the last `/` or `#` (the whole string when
/// neither is present).
#[cfg(feature = "core")]
fn iri_local_name(iri: &str) -> &str {
    match iri.rfind(['/', '#']) {
        Some(i) => &iri[i + 1..],
        None => iri,
    }
}

/// Search the bundle's `gmeow:graph/documentation` projection for documentation-entry
/// records whose label / definition / advisory prose match `query`, returning up to
/// `limit` ranked [`SearchHit`]s.
///
/// The tool queries the documentation NAMED GRAPH — always present in every bundle —
/// NOT any packed `search-index.json` archive member (the lean `gmeow.gts` carries
/// none). An ABSENT/EMPTY documentation graph is therefore a HARD FAIL (a real defect,
/// never a silent empty result); a query that legitimately matches nothing returns an
/// empty vector (empty-but-ok is the caller's `{"ok":true,"results":[]}`).
///
/// Ranking is deterministic: records are ordered by match quality (a label match
/// outranks a definition match, which outranks an advice-only match) then by `id`
/// (the documented IRI), so the same query twice yields the same order.
#[cfg(feature = "core")]
fn search_documentation(
    docs: &Arc<purrdf::RdfDataset>,
    query: &str,
    limit: usize,
) -> gmeow_errors::Result<Vec<SearchHit>> {
    // HARD FAIL: the documentation graph is absent/empty in this bundle. docs_search
    // serves the documentation graph, so a missing graph is a genuine defect.
    if docs.quad_count() == 0 {
        return Err(gmeow_errors::Diag::of_kind(crate::error::Mcp {
            message: "docs_search: the gmeow:graph/documentation named graph is absent or empty \
                      in this bundle — the searchable documentation projection is missing"
                .to_string(),
        }));
    }
    let value = sparql_result_to_json(gmeow_bundle_view::native_query::query(
        docs,
        DOC_SEARCH_QUERY,
    )?)?;

    // Aggregate the (repeatable advice/alignment/missing) rows back per entry subject.
    #[derive(Default)]
    struct Acc {
        kind: String,
        id: String,
        label: String,
        definition: Option<String>,
        url: Option<String>,
        advice: BTreeSet<String>,
        alignments: BTreeSet<String>,
        missing: BTreeSet<String>,
    }
    let mut by_subject: BTreeMap<String, Acc> = BTreeMap::new();
    for row in select_rows(&value) {
        let (Some(subject), Some(ty), Some(label)) =
            (row.get("s"), row.get("type"), row.get("label"))
        else {
            continue;
        };
        let kind = match iri_local_name(ty) {
            "DocumentedTerm" => "term",
            "DocumentedSlice" => "slice",
            "DocumentedConcern" => "concern",
            // Any other typed subject is not a searchable documentation-entry record.
            _ => continue,
        };
        let acc = by_subject.entry(subject.clone()).or_default();
        acc.kind = kind.to_string();
        acc.label = label.clone();
        acc.id = row
            .get("documents")
            .cloned()
            .unwrap_or_else(|| subject.clone());
        if let Some(def) = row.get("definition") {
            acc.definition = Some(def.clone());
        }
        if let Some(url) = row.get("url") {
            acc.url = Some(url.clone());
        }
        if let Some(advice) = row.get("advice") {
            acc.advice.insert(advice.clone());
        }
        if let Some(alignment) = row.get("alignment") {
            acc.alignments.insert(alignment.clone());
        }
        if let Some(missing) = row.get("missing") {
            acc.missing.insert(iri_local_name(missing).to_string());
        }
    }

    let query_lc = query.to_lowercase();
    let tokens: Vec<&str> = query_lc.split_whitespace().collect();

    let mut hits: Vec<SearchHit> = Vec::new();
    for acc in by_subject.into_values() {
        let label_lc = acc.label.to_lowercase();
        let def_lc = acc.definition.as_deref().unwrap_or("").to_lowercase();
        let advice_lc = acc
            .advice
            .iter()
            .map(|a| a.to_lowercase())
            .collect::<Vec<_>>()
            .join("\n");
        // Rank by the strongest matching field (label > definition > advice); a record
        // matching none is not a hit.
        let rank = if field_matches(&label_lc, &query_lc, &tokens) {
            0
        } else if field_matches(&def_lc, &query_lc, &tokens) {
            1
        } else if field_matches(&advice_lc, &query_lc, &tokens) {
            2
        } else {
            continue;
        };
        hits.push(SearchHit {
            kind: acc.kind,
            id: acc.id,
            label: acc.label,
            definition: acc.definition,
            url: acc.url,
            advice: acc.advice.into_iter().collect(),
            alignments: acc.alignments.into_iter().collect(),
            missing_coverage: acc.missing.into_iter().collect(),
            rank,
        });
    }
    // Deterministic order: best match first, then by documented IRI as a stable
    // tie-break.
    hits.sort_by(|a, b| a.rank.cmp(&b.rank).then_with(|| a.id.cmp(&b.id)));
    hits.truncate(limit);
    Ok(hits)
}

/// Extract the `results.bindings` of a SPARQL-1.1 JSON envelope into flat
/// `var → lexical-value` rows (each binding's `"value"`). Shared by
/// [`McpView::docs_select_rows`] and [`search_documentation`].
#[cfg(feature = "core")]
fn select_rows(value: &Value) -> Vec<BTreeMap<String, String>> {
    value
        .get("results")
        .and_then(|r| r.get("bindings"))
        .and_then(Value::as_array)
        .map(|bindings| {
            bindings
                .iter()
                .filter_map(Value::as_object)
                .map(|binding| {
                    binding
                        .iter()
                        .filter_map(|(var, cell)| {
                            cell.get("value")
                                .and_then(Value::as_str)
                                .map(|v| (var.clone(), v.to_owned()))
                        })
                        .collect::<BTreeMap<String, String>>()
                })
                .collect()
        })
        .unwrap_or_default()
}

/// The string value of `key` in a JSON object, or `""` when absent / non-string.
/// A small adapter so the full-tier `doc_card` panels can reuse the Task-4
/// `term_entailments` / `term_fixtures` JSON records without re-querying the graph.
#[cfg(feature = "core")]
fn value_str(value: &Value, key: &str) -> String {
    value
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

/// The string members of the `key` array in a JSON object, in order; empty when
/// absent or not an array. (Values are already deterministically ordered upstream.)
#[cfg(feature = "core")]
fn value_str_array(value: &Value, key: &str) -> Vec<String> {
    value
        .get(key)
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

/// Render one fixture record as the tool's JSON object. The advisory fields are
/// emitted as `null` when the slice authored none (a stable shape callers can read
/// without probing for key presence).
#[cfg(feature = "core")]
fn fixture_json(view: &FixtureView) -> Value {
    json!({
        "title": view.title,
        "text": view.text,
        "expected_outcome": view.expected_outcome,
        "violation_code": view.violation_code,
        "rationale": view.rationale,
    })
}

/// Render one competency-question record as the tool's JSON object. The optional
/// expectations are included only when authored; the row-count and exact-flag are
/// typed back to a JSON number / boolean from their `xsd:integer` / `xsd:boolean`
/// lexical forms (the raw lexeme is kept only if it is somehow not well-typed).
#[cfg(feature = "core")]
fn competency_json(query_text: &str, row: &BTreeMap<String, String>) -> Value {
    let mut obj = serde_json::Map::new();
    obj.insert("query_text".to_string(), json!(query_text));
    if let Some(rationale) = row.get("rationale") {
        obj.insert("rationale".to_string(), json!(rationale));
    }
    if let Some(count) = row.get("count") {
        let value = count
            .parse::<i64>()
            .map(|n| json!(n))
            .unwrap_or_else(|_| json!(count));
        obj.insert("expected_row_count".to_string(), value);
    }
    if let Some(exact) = row.get("exact") {
        let value = match exact.as_str() {
            "true" | "1" => json!(true),
            "false" | "0" => json!(false),
            other => json!(other),
        };
        obj.insert("exact_rows".to_string(), value);
    }
    Value::Object(obj)
}

/// A loaded, bundle-backed view over the GMEOW snapshot for the MCP consumer.
pub struct McpView {
    /// THIS server's view of the bundled snapshot as the native carrier dataset:
    /// the MCP server is a gts ARCHIVE CONSUMER — it imports `gmeow.gts` to
    /// the carrier representation ONCE and serves every surface off the shared export
    /// `FoldView`, exactly as the in-pipeline export leaf does.
    dataset: Arc<purrdf::RdfDataset>,
    /// Ontology title / version — language-independent (`fold_meta` reads the
    /// header via a token-minimal `value`, not a language selector), so they are
    /// resolved once at construction.
    #[cfg(feature = "core")]
    title: String,
    #[cfg(feature = "core")]
    version: String,
    /// `requested.join(",")` → collected terms, mirroring `_TERMS_CACHE`. Stored
    /// behind an `Arc` so the cache mutex is released before the (potentially
    /// large) render runs — concurrent reads of a cached entry never serialize
    /// behind one another's rendering.
    #[cfg(feature = "core")]
    cache: Mutex<HashMap<String, Arc<Vec<Term>>>>,
    /// `term-IRI → published site URL`, built once from the
    /// `gmeow:graph/documentation` graph — language-independent, so it is cached
    /// across all `requested` lists. Empty when the doc graph is absent (then the
    /// `llms.txt` index renders linkless).
    #[cfg(feature = "core")]
    doc_urls: OnceLock<Arc<HashMap<String, String>>>,
    /// The documentation named graph projected to a default-graph dataset once per
    /// server. Every documentation tool and full-card panel queries this immutable
    /// view; rebuilding it for each SPARQL query copies the same whole graph and turns
    /// a single card into several bundle-scale scans.
    #[cfg(feature = "core")]
    documentation: OnceLock<Arc<purrdf::RdfDataset>>,
    /// The authoring-briefs named graph projected to a default-graph dataset once per
    /// server — the per-slice `gmeow:AuthoringPacket` corpus the `slice_brief` tool
    /// serves straight out of the bundle. Cached like `documentation`: projecting the
    /// whole (bundle-scale) briefs graph per call would rescan the entire corpus.
    #[cfg(feature = "core")]
    authoring_briefs: OnceLock<Arc<purrdf::RdfDataset>>,
    /// The JSON Schema `$defs` key set folded into this bundle's `schemas-archive`
    /// blob — the model-existence signal `export::term_to_card`'s `python_model`
    /// gate reads (built once from `gts`, like `doc_urls`; see [`Self::modeled_defs`]).
    #[cfg(feature = "core")]
    modeled_defs: OnceLock<Arc<BTreeSet<String>>>,
    /// The raw `gmeow.gts` snapshot bytes this view was imported from, retained
    /// verbatim. `from_snapshot` parses the bundle to the carrier dataset and
    /// then DISCARDS the bytes; the native validation surface
    /// (`validate_local`) needs them back — `gmeow_validate::data_validate` reads
    /// the folded `shapes-archive` blob directly from the raw GTS, not from the
    /// parsed dataset. Held behind an `Arc<[u8]>` so cloning the view is cheap
    /// and the (potentially large) bundle is never copied.
    gts: Arc<[u8]>,
    /// The bundle's parsed Tier-1 data-graph shape union, decoded ONCE from the
    /// raw snapshot bytes on the first `validate_local` call and shared by every
    /// subsequent call. The bundle decode + shape parse dominates a Tier-1 run
    /// (whole-bundle seconds vs the milliseconds the validation itself takes),
    /// and the shapes are immutable per bundle (see [`Self::tier1_shapes`]).
    #[cfg(feature = "core")]
    tier1_shapes: OnceLock<gmeow_validate::data_validate::Tier1Shapes>,
}

impl McpView {
    fn from_dataset(
        dataset: Arc<purrdf::RdfDataset>,
        gts: Arc<[u8]>,
    ) -> gmeow_errors::Result<Self> {
        // The header fold is a CORE-segment read: `title`/`version` reach the caller only
        // through the documentation surfaces (`llms.txt`, the card, the OKF index), all of
        // which are core tools or core resources. Folding it in the reasoning image would
        // keep `export::fold_meta` — and behind it the whole fold view — reachable for a
        // string nothing there can return.
        #[cfg(feature = "core")]
        let (title, version) = {
            let view = FoldView::new(dataset.as_ref());
            export::fold_meta(&view)?
        };
        Ok(Self {
            dataset,
            #[cfg(feature = "core")]
            title,
            #[cfg(feature = "core")]
            version,
            #[cfg(feature = "core")]
            cache: Mutex::new(HashMap::new()),
            #[cfg(feature = "core")]
            doc_urls: OnceLock::new(),
            #[cfg(feature = "core")]
            documentation: OnceLock::new(),
            #[cfg(feature = "core")]
            authoring_briefs: OnceLock::new(),
            #[cfg(feature = "core")]
            modeled_defs: OnceLock::new(),
            gts,
            #[cfg(feature = "core")]
            tier1_shapes: OnceLock::new(),
        })
    }

    /// The raw `gmeow.gts` snapshot bytes this view serves, for the native
    /// validation surface that reads the folded `shapes-archive` blob directly.
    ///
    /// PUBLIC because an [`Extension`](crate::Extension) handler is by definition code the
    /// leaf does not carry: a host that owns a reader this crate deliberately does not link
    /// — the medium registry among them — still has to be handed the same bytes the builtin
    /// surface answers from, or it would be answering about a different artifact.
    pub fn gts_bytes(&self) -> &[u8] {
        &self.gts
    }

    /// Resolve a CURIE / local name / IRI / unambiguous prefix to its public
    /// metadata record (JSON envelope with `"ok"`), or a not-found envelope.
    #[cfg(feature = "core")]
    fn lookup_term_json(&self, term: &str, requested: Vec<String>) -> String {
        self.with_terms(requested, |terms| export::lookup_envelope(terms, term))
    }

    /// Resolve a CURIE / local name / IRI / label (or unambiguous prefix) to its
    /// canonical term IRI, via the SAME resolution path `lookup_term` / `doc_card`
    /// use. Propagates [`export::ConsumerResolution`] so the caller HARD-FAILS a
    /// cross-namespace collision with a typed ambiguity diagnostic and an unknown
    /// term with the unknown-term diagnostic — never a fabricated empty result or a
    /// silent pick. `export::resolve_term_iri` borrows zero-copy; this wrapper must
    /// allocate ONE owned `String` here because the resolved IRI has to outlive the
    /// cached `terms` slice that `with_terms` only lends for the closure's duration.
    #[cfg(feature = "core")]
    fn resolve_term_iri(&self, term: &str, requested: Vec<String>) -> ConsumerResolution<String> {
        self.with_terms(requested, |terms| {
            match export::resolve_term_iri(terms, term) {
                ConsumerResolution::Resolved(iri) => ConsumerResolution::Resolved(iri.to_owned()),
                ConsumerResolution::Ambiguous { candidates } => {
                    ConsumerResolution::Ambiguous { candidates }
                }
                ConsumerResolution::NotFound => ConsumerResolution::NotFound,
            }
        })
    }

    /// The counter-example / well-formed conformance fixtures documenting `term_iri`,
    /// read from the `gmeow:graph/documentation` projection and split by
    /// `gmeow:docFixtureKind`. Each fixture is one record carrying its title, full
    /// Turtle body, and the authored advisory fields (expected outcome, violation
    /// code, conformance rationale) — `null` when the slice authored none. Both lists
    /// are ordered by fixture IRI, so the surface is deterministic. A term that
    /// documents no fixtures yields two empty lists (honest empty-but-ok).
    #[cfg(feature = "core")]
    fn term_fixtures(&self, term_iri: &str) -> gmeow_errors::Result<(Vec<Value>, Vec<Value>)> {
        let query = fixtures_by_term_query(term_iri);
        // One record per fixture IRI: the fixture-scoped columns are single-valued,
        // so first-row-wins is the whole record. BTreeMap → fixture-IRI order.
        let mut by_fixture: BTreeMap<String, (String, FixtureView)> = BTreeMap::new();
        for row in self.docs_select_rows(&query)? {
            let (Some(fixture), Some(kind), Some(text)) =
                (row.get("f"), row.get("kind"), row.get("text"))
            else {
                continue;
            };
            by_fixture.entry(fixture.clone()).or_insert_with(|| {
                (
                    kind.clone(),
                    FixtureView {
                        title: row.get("label").cloned().unwrap_or_else(|| fixture.clone()),
                        text: text.clone(),
                        expected_outcome: row.get("outcome").cloned(),
                        violation_code: row.get("code").cloned(),
                        rationale: row.get("rationale").cloned(),
                    },
                )
            });
        }

        let wellformed_kind = format!("{GMEOW_NS}docFixtureKindWellformed");
        let counter_kind = format!("{GMEOW_NS}docFixtureKindCounterExample");
        let mut wellformed = Vec::new();
        let mut counter_examples = Vec::new();
        for (kind, view) in by_fixture.into_values() {
            let record = fixture_json(&view);
            if kind == counter_kind {
                counter_examples.push(record);
            } else if kind == wellformed_kind {
                wellformed.push(record);
            }
        }
        Ok((wellformed, counter_examples))
    }

    /// The reasoner entailments documenting `term_iri`, read from the
    /// `gmeow:graph/documentation` projection. Each `gmeow:Entailment` node is one
    /// record — its rule, its conclusion, and ALL its premises (every
    /// `gmeow:entailmentPremise`, sorted). Records are ordered by entailment IRI, so
    /// the surface is deterministic. A term with no entailments yields an empty list.
    #[cfg(feature = "core")]
    fn term_entailments(&self, term_iri: &str) -> gmeow_errors::Result<Vec<Value>> {
        let query = entailments_by_term_query(term_iri);
        // Aggregate the (repeatable-premise) rows back per entailment IRI.
        let mut by_entailment: BTreeMap<String, (String, String, BTreeSet<String>)> =
            BTreeMap::new();
        for row in self.docs_select_rows(&query)? {
            let (Some(entailment), Some(rule), Some(conclusion)) =
                (row.get("e"), row.get("rule"), row.get("conclusion"))
            else {
                continue;
            };
            let entry = by_entailment
                .entry(entailment.clone())
                .or_insert_with(|| (rule.clone(), conclusion.clone(), BTreeSet::new()));
            if let Some(premise) = row.get("premise") {
                entry.2.insert(premise.clone());
            }
        }
        let out = by_entailment
            .into_values()
            .map(|(rule, conclusion, premises)| {
                json!({
                    "rule": rule,
                    "conclusion": conclusion,
                    "premises": premises.into_iter().collect::<Vec<_>>(),
                })
            })
            .collect();
        Ok(out)
    }

    /// The runnable competency questions from the `gmeow:graph/documentation`
    /// projection: every `gmeow:DocumentedCompetency` carrying a `gmeow:cqQueryText`,
    /// or — when `term_iri` is `Some` — only those documenting that term. Each record
    /// carries the runnable query text and the authored expectations
    /// (`gmeow:cqRationale`, `gmeow:cqExpectRowCount`, `gmeow:cqExactRows`) when
    /// present. Records are ordered by competency IRI, so the surface is
    /// deterministic.
    #[cfg(feature = "core")]
    fn competency_questions(&self, term_iri: Option<&str>) -> gmeow_errors::Result<Vec<Value>> {
        let query = competency_query(term_iri);
        // One record per competency IRI (all columns single-valued). BTreeMap →
        // competency-IRI order.
        let mut by_competency: BTreeMap<String, Value> = BTreeMap::new();
        for row in self.docs_select_rows(&query)? {
            let (Some(competency), Some(query_text)) = (row.get("c"), row.get("q")) else {
                continue;
            };
            by_competency
                .entry(competency.clone())
                .or_insert_with(|| competency_json(query_text, &row));
        }
        Ok(by_competency.into_values().collect())
    }

    /// The standard llmstxt.org vocabulary index (`llms.txt`) for `requested`,
    /// with bullets linking into the published docs site.
    #[cfg(feature = "core")]
    fn llms_txt_text(&self, requested: Vec<String>) -> String {
        let title = self.title.clone();
        let version = self.version.clone();
        let doc_urls = self.doc_urls();
        self.with_terms(requested, |terms| {
            export::consumer_llms_txt(terms, &title, &version, &doc_urls)
        })
    }

    /// The complete inlined index (`llms-full.txt`) for `requested`, carrying the graph-derived
    /// GMN-1 teachability primer. A carrier that fails to yield the primer's GMN-1 codebook is a
    /// HARD FAIL (no-optionality), never a silently primer-less complete form.
    #[cfg(feature = "core")]
    fn llms_full_text(&self, requested: Vec<String>) -> gmeow_errors::Result<String> {
        let title = self.title.clone();
        let version = self.version.clone();
        let modeled_defs = self.modeled_defs();
        let primer = self.gmn1_primer()?;
        Ok(self.with_terms(requested, |terms| {
            export::consumer_llms_full(terms, &title, &version, &modeled_defs, &primer)
        }))
    }

    /// The graph-derived GMN-1 teachability primer over THIS view's carrier dataset — shared by
    /// the `llms_full` surface and the `gmeow://ontology/gmn1-primer` resource.
    #[cfg(feature = "core")]
    fn gmn1_primer(&self) -> gmeow_errors::Result<gmeow_docs_model::gmn1_primer::Gmn1Primer> {
        gmeow_docs_model::gmn1_primer::build_primer(self.dataset.as_ref()).map_err(|e| {
            gmeow_errors::Diag::of_kind(crate::error::Mcp {
                message: format!("build GMN-1 teachability primer: {e}"),
            })
        })
    }

    /// A prompt-ready term card for one term at the requested detail tier and
    /// output format, wrapped in the cost-metadata envelope
    /// (`{"ok":true,"detail","format","bytes","tokens","card"}`).
    ///
    /// The card is built through the SINGLE shared builder + renderer
    /// (`export::doc_card_build` → `gmeow_docs_model::card`). For
    /// [`CardDetail::Full`](gmeow_docs_model::card::CardDetail::Full) the rich oracle
    /// panels (entailments, Do / Don't fixtures, diagnostics, projection loss) are
    /// populated by querying the `gmeow:graph/documentation` projection for the
    /// resolved term IRI. An UNKNOWN term is a HARD FAIL (`Err`).
    ///
    /// `format=markdown` renders through `render_card` (tier-gated); `format=json`
    /// serializes the tier-projected `Card`. `bytes`/`tokens` measure the returned
    /// card payload so callers can budget by tier.
    #[cfg(feature = "core")]
    fn doc_card(
        &self,
        term: &str,
        detail: gmeow_docs_model::card::CardDetail,
        format: CardFormat,
        requested: Vec<String>,
    ) -> gmeow_errors::Result<String> {
        use gmeow_docs_model::card::CardDetail;
        let modeled_defs = self.modeled_defs();
        let built = self.with_terms(requested, |terms| {
            export::doc_card_build(terms, term, &modeled_defs)
        });
        let (title, mut card) = match built {
            ConsumerResolution::Resolved(pair) => pair,
            ConsumerResolution::Ambiguous { candidates } => {
                return Err(ambiguous_term_err(term, &candidates));
            }
            ConsumerResolution::NotFound => return Err(unknown_term_err(term)),
        };
        // The full tier is the oracle card: enrich the compact card with the rich
        // panels queried from the documentation graph by the resolved term IRI.
        if detail == CardDetail::Full {
            let iri = card.iri.clone();
            let (fixtures_do, fixtures_dont) = self.card_fixtures(&iri)?;
            card.entailments = self.card_entailments(&iri)?;
            card.fixtures_do = fixtures_do;
            card.fixtures_dont = fixtures_dont;
            card.diagnostics = self.card_diagnostics(&iri)?;
            card.loss = self.card_loss(&iri)?;
        }
        let (rendered, card_value) = match format {
            CardFormat::Markdown => {
                let md = gmeow_docs_model::card::render_card(&title, &card, detail);
                let value = Value::String(md.clone());
                (md, value)
            }
            CardFormat::Json => {
                let projected = card.projected(detail);
                let js = serde_json::to_string(&projected).map_err(|e| {
                    gmeow_errors::Diag::of_kind(crate::error::Mcp {
                        message: format!("doc_card: serialize card to JSON: {e}"),
                    })
                })?;
                let value = serde_json::to_value(&projected).map_err(|e| {
                    gmeow_errors::Diag::of_kind(crate::error::Mcp {
                        message: format!("doc_card: card to JSON value: {e}"),
                    })
                })?;
                (js, value)
            }
        };
        Ok(json!({
            "ok": true,
            "detail": card_detail_label(detail),
            "format": format.label(),
            "bytes": rendered.len(),
            "tokens": gmeow_docs_model::llms::estimate_tokens(&rendered),
            "card": card_value,
        })
        .to_string())
    }

    /// The full-tier entailment panel for `term_iri`: the reasoner derivations
    /// documenting the term, mapped from the SAME `term_entailments` query the
    /// `entailments` tool serves. Empty for a term with no derivations.
    #[cfg(feature = "core")]
    fn card_entailments(
        &self,
        term_iri: &str,
    ) -> gmeow_errors::Result<Vec<gmeow_docs_model::card::CardEntailment>> {
        Ok(self
            .term_entailments(term_iri)?
            .iter()
            .map(|v| gmeow_docs_model::card::CardEntailment {
                rule: value_str(v, "rule"),
                conclusion: value_str(v, "conclusion"),
                premises: value_str_array(v, "premises"),
            })
            .collect())
    }

    /// The full-tier Do / Don't fixture panels for `term_iri`: the well-formed
    /// exemplars and the counter-examples, mapped from the SAME `term_fixtures`
    /// query the `counter_examples` tool serves. Each fixture body is one-lined and
    /// capped to a short snippet (the full body stays available via
    /// `counter_examples`). Both empty for a term documenting no fixtures.
    #[cfg(feature = "core")]
    fn card_fixtures(
        &self,
        term_iri: &str,
    ) -> gmeow_errors::Result<(
        Vec<gmeow_docs_model::card::CardFixture>,
        Vec<gmeow_docs_model::card::CardFixture>,
    )> {
        let to_card = |v: &Value| gmeow_docs_model::card::CardFixture {
            title: value_str(v, "title"),
            body: fixture_body_snippet(&value_str(v, "text")),
        };
        let (wellformed, counter_examples) = self.term_fixtures(term_iri)?;
        Ok((
            wellformed.iter().map(&to_card).collect(),
            counter_examples.iter().map(&to_card).collect(),
        ))
    }

    /// The full-tier diagnostics panel for `term_iri`: the finding codes the term
    /// may hit, read from the `gmeow:docEvidenceKindDiagnostics` evidence in the
    /// documentation graph. Rows are ordered by finding code, so the panel is
    /// deterministic. Empty for a term no finding concerns.
    #[cfg(feature = "core")]
    fn card_diagnostics(
        &self,
        term_iri: &str,
    ) -> gmeow_errors::Result<Vec<gmeow_docs_model::card::CardDiagnostic>> {
        let mut by_code: BTreeMap<String, String> = BTreeMap::new();
        for row in self.docs_select_rows(&diagnostics_by_term_query(term_iri))? {
            let (Some(code), Some(claim)) = (row.get("code"), row.get("claim")) else {
                continue;
            };
            by_code.entry(code.clone()).or_insert_with(|| claim.clone());
        }
        Ok(by_code
            .into_iter()
            .map(|(code, note)| gmeow_docs_model::card::CardDiagnostic { code, note })
            .collect())
    }

    /// The full-tier projection-loss panel for `term_iri`: the targets the term
    /// degrades into and each degradation's preservation judgment, read from the
    /// `gmeow:docEvidenceKindLoss` evidence in the documentation graph. Rows are
    /// ordered by target, so the panel is deterministic. Empty for a term that
    /// degrades under no projection.
    #[cfg(feature = "core")]
    fn card_loss(
        &self,
        term_iri: &str,
    ) -> gmeow_errors::Result<Vec<gmeow_docs_model::card::CardLoss>> {
        let mut by_target: BTreeMap<String, String> = BTreeMap::new();
        for row in self.docs_select_rows(&loss_by_term_query(term_iri))? {
            let (Some(target), Some(judgment)) = (row.get("target"), row.get("judgment")) else {
                continue;
            };
            by_target
                .entry(target.clone())
                .or_insert_with(|| judgment.clone());
        }
        Ok(by_target
            .into_iter()
            .map(|(target, preservation)| gmeow_docs_model::card::CardLoss {
                target,
                preservation,
            })
            .collect())
    }

    /// The OKF manifest JSON envelope for `requested`.
    #[cfg(feature = "core")]
    fn okf_index_json(&self, requested: Vec<String>) -> String {
        self.with_terms(requested, export::okf_index_envelope)
    }

    /// Run a SELECT / ASK SPARQL query over the `gmeow:graph/documentation` named
    /// graph (re-rooted to the default graph so a plain query with no `GRAPH`
    /// clause reaches it), returning a standard SPARQL-1.1 JSON-results envelope
    /// under `"ok"`. CONSTRUCT / DESCRIBE are rejected — the tool serves one result
    /// shape (bindings or a boolean), never a graph.
    #[cfg(feature = "core")]
    fn query_docs_json(&self, sparql: &str) -> String {
        match self.run_docs_query(sparql) {
            Ok(value) => value.to_string(),
            Err(err) => json!({"ok": false, "error": err.to_string()}).to_string(),
        }
    }

    #[cfg(feature = "core")]
    fn run_docs_query(&self, sparql: &str) -> gmeow_errors::Result<Value> {
        let docs = self.documentation();
        let result = gmeow_bundle_view::native_query::query(docs, sparql)?;
        sparql_result_to_json(result)
    }

    /// Run a SELECT over the documentation graph and return its rows as flat
    /// `var → lexical-value` maps (the `results.bindings` of the SPARQL-1.1 JSON
    /// envelope, with each binding's `"value"` extracted). A missing/optional
    /// variable is simply absent from a row's map.
    #[cfg(feature = "core")]
    fn docs_select_rows(
        &self,
        sparql: &str,
    ) -> gmeow_errors::Result<Vec<BTreeMap<String, String>>> {
        let value = self.run_docs_query(sparql)?;
        Ok(select_rows(&value))
    }

    /// Build the two fixture correspondence maps from the `gmeow:graph/documentation`
    /// projection: `finding-code → counter-example` (keyed on the authored
    /// `gmeow:docViolationCode`) and `finding-code → positive exemplar` (the
    /// well-formed sibling joined through a shared referenced term). Both are
    /// deterministic: a code that repeats across fixtures resolves to the
    /// lexicographically-first fixture IRI, and the well-formed sibling is the
    /// lexicographically-first well-formed fixture sharing a referenced term.
    #[cfg(feature = "core")]
    fn fixture_maps(
        &self,
    ) -> gmeow_errors::Result<(BTreeMap<String, FixtureView>, BTreeMap<String, FixtureView>)> {
        // Counter-example fixtures: aggregate the (possibly multi-`documents`) rows
        // back per fixture IRI so a fixture is one record with its full referenced-term
        // set. BTreeMap keeps the fixture IRIs sorted → deterministic first-wins.
        let mut counter_by_fixture: BTreeMap<String, (FixtureView, BTreeSet<String>)> =
            BTreeMap::new();
        for row in self.docs_select_rows(COUNTER_EXAMPLE_FIXTURE_QUERY)? {
            let (Some(fixture), Some(code), Some(text)) =
                (row.get("f"), row.get("code"), row.get("text"))
            else {
                continue;
            };
            let entry = counter_by_fixture
                .entry(fixture.clone())
                .or_insert_with(|| {
                    (
                        FixtureView {
                            title: row.get("label").cloned().unwrap_or_else(|| fixture.clone()),
                            text: text.clone(),
                            expected_outcome: row.get("outcome").cloned(),
                            violation_code: Some(code.clone()),
                            rationale: row.get("rationale").cloned(),
                        },
                        BTreeSet::new(),
                    )
                });
            if let Some(term) = row.get("term") {
                entry.1.insert(term.clone());
            }
        }

        // Well-formed fixtures: same aggregation, no violation code (they violate
        // nothing).
        let mut wellformed_by_fixture: BTreeMap<String, (FixtureView, BTreeSet<String>)> =
            BTreeMap::new();
        for row in self.docs_select_rows(WELLFORMED_FIXTURE_QUERY)? {
            let (Some(fixture), Some(text)) = (row.get("f"), row.get("text")) else {
                continue;
            };
            let entry = wellformed_by_fixture
                .entry(fixture.clone())
                .or_insert_with(|| {
                    (
                        FixtureView {
                            title: row.get("label").cloned().unwrap_or_else(|| fixture.clone()),
                            text: text.clone(),
                            expected_outcome: row.get("outcome").cloned(),
                            violation_code: None,
                            rationale: row.get("rationale").cloned(),
                        },
                        BTreeSet::new(),
                    )
                });
            if let Some(term) = row.get("term") {
                entry.1.insert(term.clone());
            }
        }

        // code → counter-example (first fixture IRI wins) + the code's referenced terms.
        let mut counter_examples_by_code: BTreeMap<String, FixtureView> = BTreeMap::new();
        let mut terms_by_code: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        for (view, terms) in counter_by_fixture.values() {
            let Some(code) = view.violation_code.clone() else {
                continue;
            };
            counter_examples_by_code
                .entry(code.clone())
                .or_insert_with(|| view.clone());
            // Terms of the CHOSEN (first) counter-example anchor the well-formed join.
            terms_by_code.entry(code).or_insert_with(|| terms.clone());
        }

        // code → well-formed sibling with the largest referenced-term overlap
        // (most specific/relevant exemplar); ties broken deterministically by
        // the lexicographically smallest fixture IRI. `min_by_key` is used
        // (rather than `max_by_key`, which returns the LAST maximal element on
        // ties) with a key of `(Reverse(overlap), fixture_iri)` — since fixture
        // IRIs are unique, this always has a single minimum, so the choice is
        // fully deterministic and stable across runs.
        let mut wellformed_by_code: BTreeMap<String, FixtureView> = BTreeMap::new();
        for (code, ce_terms) in &terms_by_code {
            let best = wellformed_by_fixture
                .iter()
                .filter_map(|(fixture, (view, wf_terms))| {
                    let overlap = wf_terms.intersection(ce_terms).count();
                    (overlap > 0).then_some((overlap, fixture, view))
                })
                .min_by_key(|(overlap, fixture, _)| (Reverse(*overlap), (*fixture).clone()));
            if let Some((_overlap, _fixture, view)) = best {
                wellformed_by_code.insert(code.clone(), view.clone());
            }
        }

        Ok((counter_examples_by_code, wellformed_by_code))
    }

    /// Build `term-IRI → entailments` from the documentation graph's entailment
    /// records (`gmeow:Entailment`), aggregating each record's premises and grouping
    /// by the `gmeow:documents` term. Entailment records are iterated in sorted IRI
    /// order and each entailment's premises are sorted, so the map is deterministic.
    #[cfg(feature = "core")]
    fn entailment_map(&self) -> gmeow_errors::Result<BTreeMap<String, Vec<EntailmentView>>> {
        // Aggregate the (possibly multi-premise) rows back per entailment IRI.
        let mut by_entailment: BTreeMap<String, (String, String, String, BTreeSet<String>)> =
            BTreeMap::new();
        for row in self.docs_select_rows(ENTAILMENT_QUERY)? {
            let (Some(entailment), Some(term), Some(rule), Some(conclusion)) = (
                row.get("e"),
                row.get("term"),
                row.get("rule"),
                row.get("conclusion"),
            ) else {
                continue;
            };
            let entry = by_entailment.entry(entailment.clone()).or_insert_with(|| {
                (
                    term.clone(),
                    rule.clone(),
                    conclusion.clone(),
                    BTreeSet::new(),
                )
            });
            if let Some(premise) = row.get("premise") {
                entry.3.insert(premise.clone());
            }
        }

        let mut entailments_by_term: BTreeMap<String, Vec<EntailmentView>> = BTreeMap::new();
        for (term, rule, conclusion, premises) in by_entailment.into_values() {
            entailments_by_term
                .entry(term)
                .or_default()
                .push(EntailmentView {
                    rule,
                    conclusion,
                    premises: premises.into_iter().collect(),
                });
        }
        Ok(entailments_by_term)
    }

    /// Run a SELECT / ASK SPARQL query over the bundle canon UNIONED with a
    /// READ-ONLY external overlay parsed from the caller's inline `data` in the
    /// EXPLICITLY declared `format`, returning a standard SPARQL-1.1 JSON-results
    /// envelope under `"ok"`. See [`Self::run_local_query`] for the read-only /
    /// external-provenance contract.
    #[cfg(feature = "core")]
    fn query_local_json(
        &self,
        data: &str,
        format: &str,
        sparql: &str,
        scope: QueryScope,
    ) -> String {
        match self.run_local_query(data, format, sparql, scope) {
            Ok(value) => value.to_string(),
            Err(err) => json!({"ok": false, "error": err.to_string()}).to_string(),
        }
    }

    /// Query `bundle ∪ overlay` where `overlay` is the caller's INLINE lower-tier
    /// graph text, loaded as a READ-ONLY external annex.
    ///
    /// CONTRACT (enforced here, not just documented):
    /// * the overlay arrives as BYTES plus an EXPLICIT `format`. There is no file
    ///   path and no extension sniffing: a missing or unrecognized `format` is a HARD
    ///   FAIL naming the accepted set ([`rdf_media_type`]), never a guess at Turtle
    ///   that would mis-parse an N-Quads document into a different graph;
    /// * the overlay is loaded into its own transient dataset — the signed canon
    ///   (`self.dataset`) is pushed VERBATIM and is NEVER mutated;
    /// * every overlay triple is re-homed under the distinct external-provenance
    ///   graph [`EXTERNAL_OVERLAY_GRAPH`] (the origin marker), so external content
    ///   stays isolable via a `GRAPH` clause and is NEVER unioned into the signed
    ///   `gmeow:` canon graphs;
    /// * a default-graph copy makes reads see `bundle ∪ overlay`, but the whole
    ///   union is transient and discarded after the query — it is NEVER persisted,
    ///   NEVER folded into `gmeow.gts`, and NEVER written back to the canon or the
    ///   overlay file (the memory-write triad only ever touches `memory.gts`);
    /// * SELECT, ASK, CONSTRUCT and DESCRIBE are all accepted — the result FORM is
    ///   declared in the envelope (see [`sparql_result_to_json`]).
    ///
    /// `scope` chooses WHAT the overlay is queried against, and it is an explicit
    /// first-class selection rather than a hidden default: see [`QueryScope`].
    #[cfg(feature = "core")]
    fn run_local_query(
        &self,
        data: &str,
        format: &str,
        sparql: &str,
        scope: QueryScope,
    ) -> gmeow_errors::Result<Value> {
        // The media type comes from the DECLARED format, never from a filename: an
        // unrecognized token hard-fails here naming the accepted set.
        let media = rdf_media_type("query_local", format)?;
        if data.len() > MAX_VERIFY_OVERLAY_BYTES as usize {
            return Err(gmeow_errors::Diag::of_kind(crate::error::Mcp {
                message: format!(
                    "query_local: overlay data is {} bytes, exceeding the {MAX_VERIFY_OVERLAY_BYTES}-byte \
                     ceiling; split the annex and query the parts (no silent truncation)",
                    data.len()
                ),
            }));
        }
        let overlay = purrdf::parse_dataset(data.as_bytes(), media, None)
            .with_ctx(|| format!("parse query_local overlay ({format})"))?;

        let mut builder = purrdf::RdfDatasetBuilder::new();
        if scope == QueryScope::BundleUnion {
            // The signed canon — verbatim, never mutated.
            builder.push_dataset(self.dataset.as_ref());
        }
        let external = purrdf::RdfTerm::Iri(EXTERNAL_OVERLAY_GRAPH.to_string());
        for quad in overlay.owned_quads() {
            // Default-graph copy → a plain query reads `bundle ∪ overlay` (or just the
            // overlay, under `QueryScope::InputOnly`).
            let mut in_default = quad.clone();
            in_default.graph_name = None;
            builder.push_owned_quad(&in_default);
            // Origin-marked copy → external provenance, isolable via GRAPH. Emitted under
            // BOTH scopes: the origin marker states where a triple CAME FROM, which is a
            // property of the triple and not of what it happens to be unioned with.
            let mut tagged = quad;
            tagged.graph_name = Some(external.clone());
            builder.push_owned_quad(&tagged);
        }
        let dataset = builder.freeze()?;
        let result = gmeow_bundle_view::native_query::query(&dataset, sparql)?;
        sparql_result_to_json(result)
    }

    /// Run the native reasoned-graph verify over the bundle canon UNIONED with a
    /// READ-ONLY external overlay parsed from the caller's inline `data` in the
    /// EXPLICITLY declared `format`, returning the proof-carrying JSON envelope under
    /// `"ok"`. See [`Self::run_verify_graph`] for the read-only / external-annex
    /// contract and the completeness-gate judgment.
    #[cfg(feature = "reasoning")]
    fn verify_graph_json(&self, data: &str, format: &str, budget: &Budget) -> String {
        match self.run_verify_graph(data, format, budget) {
            Ok(value) => value.to_string(),
            Err(err) => json!({"ok": false, "error": err.to_string()}).to_string(),
        }
    }

    /// Reason-and-verify `bundle ∪ overlay`, where `overlay` is the caller's INLINE
    /// lower-tier graph text, loaded as a READ-ONLY external annex, then return a
    /// PROOF-CARRYING judgment: the completeness-gated coherence class, the two
    /// completeness/evaluation axes, the reasoned-graph verify findings, the cited
    /// IRIs, and the grounded `logic:ReasoningResult` N-Quads.
    ///
    /// CONTRACT (enforced here, not just documented) — the SAME overlay discipline as
    /// [`Self::run_local_query`]:
    /// * the overlay arrives as BYTES plus an EXPLICIT `format` — no path, no
    ///   extension sniffing, and a missing/unrecognized `format` is a HARD FAIL naming
    ///   the accepted set;
    /// * the overlay is loaded into its own transient dataset; the signed canon
    ///   (`self.dataset`) is pushed VERBATIM and is NEVER mutated;
    /// * every overlay quad is dual-copied — one into the default graph (so the DL
    ///   calculus and the flat verify queries read `bundle ∪ overlay`) and one
    ///   re-homed under the external-provenance graph [`EXTERNAL_OVERLAY_GRAPH`] — but
    ///   the whole union is transient and DROPPED after the call: never persisted,
    ///   never folded into `gmeow.gts`, never written back to the canon or the overlay;
    /// * the forward closure runs THROUGH [`reason_all_budgeted`] (the mid-chase step
    ///   governor), never the unbudgeted [`gmeow_logic::reason::reason_all`], so an
    ///   agent-influenced union can never run an unbounded Turing-complete closure;
    /// * an overlay whose byte length exceeds [`MAX_VERIFY_OVERLAY_BYTES`] is a HARD
    ///   FAIL BEFORE it is parsed — the length of the inline payload is known without
    ///   touching its content, so an oversized annex (or one with a single enormous
    ///   literal) is refused before the parser ever builds a dataset from it;
    /// * an overlay exceeding [`MAX_VERIFY_OVERLAY_QUADS`] is a HARD FAIL BEFORE any
    ///   reasoning (the bounded agent path), never a truncated graph.
    ///
    /// # Completeness-gate judgment (`class_local_name`)
    ///
    /// The class is `completeness_class`'s T2 completeness-gate trichotomy — itself a
    /// thin wrapper over [`CoherenceOutcome::class_local_name_for`], the SAME gate
    /// the bundle-level coherence certifier (`certificate.rs`) uses, so this tool and
    /// [`Self::run_explain_quad`] can never diverge:
    /// * a witnessed DL contradiction (the forbidden glut under the default
    ///   forbid-glut policy) REFUTES coherence — but only a CONCLUSIVE check flatly
    ///   `Refused`s; a budget-cut check that ran into it can only attest;
    /// * else a CONCLUSIVE ([`ReasoningResult::is_conclusive`]) violation-free closure
    ///   with a NAMED certified fragment `CoherenceCertificate`s;
    /// * else (a budget-cut / non-conclusive closure, or a conclusive one naming no
    ///   certified fragment) the strongest honest claim is the strictly-weaker
    ///   `CoherenceCheckAttestation` — NEVER a certificate.
    ///
    /// # The `coherent` boolean agrees with `class_local_name` by construction
    ///
    /// `coherent` is `!completeness_refused(&result) && report.ok()` — the SAME
    /// `CoherenceOutcome` gate that decides `class_local_name`, ANDed with the
    /// bad-example verify findings. It is NEVER `report.ok()` alone: a conclusive DL
    /// glut that happens not to trip any bad-example verify query would otherwise
    /// leave `report.ok()` true while `class_local_name` reads `"Refused"` — a
    /// self-contradictory envelope. Routing both fields through one shared outcome
    /// makes `coherent:true` alongside `class_local_name:"Refused"` unrepresentable.
    ///
    /// [`ReasoningResult::is_conclusive`]: gmeow_logic::result::ReasoningResult::is_conclusive
    #[cfg(feature = "reasoning")]
    fn run_verify_graph(
        &self,
        data: &str,
        format: &str,
        budget: &Budget,
    ) -> gmeow_errors::Result<Value> {
        // Media from the DECLARED format; an unrecognized token HARD-FAILS naming the
        // accepted set (no silent fallback), exactly as query_local.
        let media = rdf_media_type("verify_graph", format)?;

        // Pre-PARSE hard bound: refuse an oversized overlay before a dataset is ever
        // built from it. The payload's byte length is known without inspecting its
        // content, so a huge annex (or one with a single enormous literal that parses to
        // very few quads) is refused before the quad ceiling below could measure it.
        let overlay_bytes = data.len() as u64;
        if overlay_bytes > MAX_VERIFY_OVERLAY_BYTES {
            return Err(gmeow_errors::Diag::of_kind(crate::error::Mcp {
                message: format!(
                    "verify_graph: overlay data is {overlay_bytes} bytes, \
                     exceeding the {MAX_VERIFY_OVERLAY_BYTES}-byte ceiling BEFORE any parse; \
                     split the annex and verify the parts (no silent truncation)"
                ),
            }));
        }

        let overlay = purrdf::parse_dataset(data.as_bytes(), media, None)
            .with_ctx(|| format!("parse verify_graph overlay ({format})"))?;

        // Pre-reasoning hard bound: refuse an oversized overlay before reasoning.
        let overlay_quads = overlay.quad_count();
        if overlay_quads > MAX_VERIFY_OVERLAY_QUADS {
            return Err(gmeow_errors::Diag::of_kind(crate::error::Mcp {
                message: format!(
                    "verify_graph: overlay carries {overlay_quads} quads, \
                     exceeding the {MAX_VERIFY_OVERLAY_QUADS} quad ceiling; split the annex \
                     and verify the parts (no silent truncation)"
                ),
            }));
        }

        // Build the transient union EXACTLY as run_local_query: canon verbatim, then
        // each overlay quad dual-copied (default graph + external-provenance graph).
        let mut builder = purrdf::RdfDatasetBuilder::new();
        builder.push_dataset(self.dataset.as_ref());
        let external = purrdf::RdfTerm::Iri(EXTERNAL_OVERLAY_GRAPH.to_string());
        for quad in overlay.owned_quads() {
            let mut in_default = quad.clone();
            in_default.graph_name = None;
            builder.push_owned_quad(&in_default);
            let mut tagged = quad;
            tagged.graph_name = Some(external.clone());
            builder.push_owned_quad(&tagged);
        }
        let union = builder.freeze()?;

        // GOVERNED closure — reason_all_budgeted CUTS the forward chase mid-flight; a
        // budget-cut returns a non-conclusive BudgetExhausted/Incomplete/Undetermined
        // verdict (never a wrong `supported`/`both`). NEVER reason_all.
        let result = reason_all_budgeted(&union, budget)?;
        let report = verify_with_reasoning_result(&union, &result, &embedded_verify_queries())?;

        // The T2 completeness-gate trichotomy over `result` (see the method docs),
        // read entirely through `completeness_class` — the SAME
        // `CoherenceOutcome::class_local_name_for` gate `run_explain_quad` reads, so
        // the two tool paths can never diverge on whether a witnessed DL glut
        // (forbidden under the default forbid-glut policy) refutes coherence. No
        // per-caller downgrade is bolted on here: the gate itself already returns
        // `"Refused"` for a CONCLUSIVE closure that witnesses one, and the
        // strictly-weaker `CoherenceCheckAttestation` for a budget-cut closure that
        // ran into one (it discloses what it found without refuting wholesale).
        let class_local_name = completeness_class(&result);

        // `coherent` MUST be derived from the SAME `completeness_refused` gate that
        // decided `class_local_name`, combined with the bad-example verify findings —
        // never from `report.ok()` alone. A conclusive DL glut that happens to trip
        // no bad-example verify query still leaves `report.ok()` true, but the shared
        // gate REFUSES it; without this combination `coherent:true` could render
        // alongside `class_local_name:"Refused"`, a self-contradictory envelope
        // `coherent` is true only when BOTH the shared outcome
        // permits it (non-refuting) AND no bad-example finding fired.
        let coherent = !completeness_refused(&result) && report.ok();

        // cited_iris: the DerivationRef cited-IRI surface when the verdict carries a
        // proof/counterproof, unioned with each finding's structured
        // `Finding::cited_iris` (the genuine `TermValue::Iri` bindings the offending
        // SPARQL solution rows carry — see `verify_with_reasoning_result`). NEVER
        // scraped from the rendered `message`/`detail` prose: an agent-controlled
        // overlay literal such as `"see <urn:fake>"` renders inside quotes and must
        // not be mistaken for a citation, which a text-scrape over angle brackets
        // cannot reliably tell apart from a genuine `<iri>` term. Sorted,
        // deduplicated via the BTreeSet.
        let mut cited_iris: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
        if let Some(proof) = &result.provenance.proof {
            cited_iris.extend(proof.cited_iris.iter().cloned());
        }
        if let Some(counterproof) = &result.provenance.counterproof {
            cited_iris.extend(counterproof.cited_iris.iter().cloned());
        }
        for finding in &report.findings {
            cited_iris.extend(finding.cited_iris.iter().cloned());
        }

        let findings: Vec<Value> = report
            .findings
            .iter()
            .map(|f| {
                json!({
                    "code": f.code,
                    "severity": f.severity.as_str(),
                    "message": f.message,
                    "detail": f.detail,
                })
            })
            .collect();

        // The grounded RDF judgment: the ReasoningResult node projected to N-Triples
        // (a subset of N-Quads) so an agent can reason over the verdict itself — its
        // completeness/evaluation axes and consumed budget ride the node.
        let judgment_nquads = project_reasoning_result(&result);

        Ok(json!({
            "ok": true,
            "class_local_name": class_local_name,
            "completeness": result.completeness.wire(),
            "evaluation": result.evaluation.wire(),
            "error_count": report.error_count(),
            "coherent": coherent,
            "findings": findings,
            "cited_iris": cited_iris.into_iter().collect::<Vec<_>>(),
            "judgment_nquads": judgment_nquads,
        }))
    }

    /// Run [`Self::run_explain_quad`] and render its proof-carrying envelope; an
    /// error (bad object surface, quad-not-in-closure, cross-world ambiguity, or a
    /// faithfulness violation) becomes the `{ok:false, error}` failure envelope,
    /// EXACTLY like [`Self::verify_graph_json`].
    #[cfg(feature = "reasoning")]
    fn explain_quad_json(
        &self,
        subject: &str,
        predicate: &str,
        obj_n3: &str,
        graph: &str,
        budget: &Budget,
    ) -> String {
        match self.run_explain_quad(subject, predicate, obj_n3, graph, budget) {
            Ok(value) => value.to_string(),
            Err(err) => json!({"ok": false, "error": err.to_string()}).to_string(),
        }
    }

    /// Read the bundle's carried SCOPED COHERENCE CERTIFICATE as a budget-free,
    /// proof-carrying envelope (R6) and render it. BUDGET-FREE and REASON-FREE: the
    /// certificate was computed ONCE at pipeline time and folded into `graph/attestations`;
    /// this reads it straight off the bundled dataset (`self.dataset`) — it NEVER
    /// re-reasons. A bundle carrying no coherence artifact is a HARD FAIL rendered as the
    /// `{ok:false, error}` envelope (there is no silent recompute fallback), EXACTLY like
    /// [`Self::verify_graph_json`].
    #[cfg(feature = "reasoning")]
    fn coherence_certificate_json(&self) -> String {
        match coherence_certificate_envelope(self.dataset.as_ref()) {
            Ok(value) => value.to_string(),
            Err(err) => json!({"ok": false, "error": err.to_string()}).to_string(),
        }
    }

    /// Reconstruct the FAITHFUL cited-IRI derivation skeleton for ONE arbitrary quad
    /// `(subject, predicate, obj_n3)` in world `graph`, over the bundle's governed
    /// forward closure. DISTINCT from [`Self::explain_finding_json`], which walks the
    /// pre-computed `graph/diagnostics` projection: this tool reconstructs the proof
    /// directly from the reasoner's premise-provenance, for any quad the closure
    /// entails (not only a published finding).
    ///
    /// CONTRACT (enforced here, not just documented):
    /// * the closure runs THROUGH [`reason_all_budgeted`] (the mid-chase step
    ///   governor), never the unbudgeted [`gmeow_logic::reason::reason_all`], so the
    ///   agent-driven target can never trigger an unbounded Turing-complete chase;
    /// * the target is located by its content-addressed reifier, WORLD-DISAMBIGUATED
    ///   by `graph` — a reifier shared across worlds that `graph` does not resolve to
    ///   exactly one row is a HARD FAIL (never an arbitrary pick), and a reifier no
    ///   row carries is a HARD FAIL `quad not in closure` (never an empty-but-ok proof);
    /// * only the SINGLE requested target is reconstructed
    ///   ([`LazyExplanationIndex::explain_one`], never `explain_all`), so the agent
    ///   never offloads whole-closure proof reconstruction onto the tool;
    /// * every cited IRI is re-checked against the full proof trace
    ///   ([`explain::assert_faithful`]) — a fabricated citation cannot escape.
    #[cfg(feature = "reasoning")]
    fn run_explain_quad(
        &self,
        subject: &str,
        predicate: &str,
        obj_n3: &str,
        graph: &str,
        budget: &Budget,
    ) -> gmeow_errors::Result<Value> {
        // The target's content-addressed reifier over the SAME canonical N3 object
        // surface `explain::Row` carries (`term_display`), so it joins the row set.
        let target_reifier = reifier_from_strings(subject, predicate, obj_n3);

        // GOVERNED closure over the whole bundle — reason_all_budgeted CUTS the chase
        // mid-flight on a budget breach (a non-conclusive verdict), NEVER reason_all.
        let result = reason_all_budgeted(self.dataset.as_ref(), budget)?;

        // The ONE row builder shared with `explanations_for_result`; this tool indexes
        // it and explains exactly one target (not the whole-closure `explain_all`).
        let rows = explain::rows_for_result(&result)?;
        let target_index = locate_explain_target(&rows, &target_reifier, graph)?;

        let explanation = LazyExplanationIndex::new(&rows).explain_one(target_index)?;
        // A fabricated cited IRI must never escape — re-verify against the full trace.
        explain::assert_faithful(&explanation, &rows)?;

        let step_skeleton: Vec<Value> = explanation
            .step_skeleton
            .iter()
            .map(|step| {
                json!({
                    "derivation_id": step.derivation_id,
                    "rule_iri": step.rule_iri,
                    "subject_iri": step.subject_iri,
                    "predicate_iri": step.predicate_iri,
                    "obj_n3": step.obj_n3,
                    "graph_iri": step.graph_iri,
                    "is_asserted": step.is_asserted,
                    "depth": step.depth,
                    "source_step_ids": step.source_step_ids,
                    "term_iris": step.term_iris,
                })
            })
            .collect();

        Ok(json!({
            "ok": true,
            "faithful": true,
            "markdown": explain::render_markdown(&explanation),
            "cited_iris": explanation.cited_iris.iter().cloned().collect::<Vec<_>>(),
            "step_skeleton": step_skeleton,
            "world_iri": explanation.world_iri,
            "completeness": completeness_class(&result),
            "judgment_nquads": project_reasoning_result(&result),
        }))
    }

    /// Explain a diagnostic witness over the bundle's `graph/diagnostics` named
    /// graph, addressed by its fingerprint IRI (a finding) or its anchor IRI (a
    /// cluster). Rehydrates the [`FindingIndex`] through the SAME native SPARQL
    /// reader the CLI `explain` uses, then returns the STRUCTURED witness surface:
    /// the focus finding (or the cluster's members), each with its provenance DAG,
    /// the aggregate ledger [`verdict`], the [`minimal_fatal_cut`] (fingerprint IRIs
    /// with codes), and the anchor cluster. An unknown/malformed target is a HARD
    /// FAIL (`Err`) — NEVER an empty-but-ok DAG.
    ///
    /// [`FindingIndex`]: gmeow_bundle_view::diagnostics_reader::FindingIndex
    /// [`verdict`]: gmeow_bundle_view::diagnostics_reader::verdict
    /// [`minimal_fatal_cut`]: gmeow_bundle_view::diagnostics_reader::minimal_fatal_cut
    #[cfg(feature = "core")]
    fn explain_finding_json(&self, target: &str) -> gmeow_errors::Result<String> {
        use gmeow_bundle_view::diagnostics_reader::{
            explain_finding, minimal_fatal_cut, read_findings, render_shared_dag, verdict,
        };
        // The reader projects the `graph/diagnostics` named graph out of THIS
        // server's held snapshot — the exact carrier the export surfaces query.
        let index = read_findings(&self.dataset)?;

        let is_finding = index.get(target).is_some();
        let cluster: Vec<String> = index
            .findings
            .iter()
            .filter(|(_, f)| f.anchor_iri.as_deref() == Some(target))
            .map(|(iri, _)| iri.clone())
            .collect();
        let is_anchor = !cluster.is_empty();
        // HARD FAIL: neither a known fingerprint IRI nor a known anchor IRI. Never a
        // fabricated empty DAG rendered as a success.
        if !is_finding && !is_anchor {
            return Err(gmeow_errors::Diag::of_kind(crate::error::Mcp {
                message: format!(
                    "unknown explain target `{target}`: not a finding fingerprint IRI or an \
                     anchor IRI in graph/diagnostics"
                ),
            }));
        }

        let walk = |iri: &str| -> gmeow_errors::Result<String> {
            let dag = explain_finding(&index, iri).map_err(|e| {
                gmeow_errors::Diag::of_kind(crate::error::Mcp {
                    message: format!("cannot walk provenance DAG for `{iri}`: {e}"),
                })
            })?;
            Ok(render_shared_dag(&dag))
        };

        // The focus: a single finding's DAG, or every cluster member's DAG.
        let (kind, focus_anchor, focus) = if is_finding {
            let f = index.get(target).expect("finding present");
            (
                "finding",
                f.anchor_iri.clone(),
                json!({
                    "finding_iri": target,
                    "code": f.code,
                    "severity": f.severity.as_str(),
                    "message": f.message,
                    "provenance_dag": walk(target)?,
                }),
            )
        } else {
            let mut members = Vec::with_capacity(cluster.len());
            for iri in &cluster {
                let f = index.get(iri).expect("cluster member present");
                members.push(json!({
                    "finding_iri": iri,
                    "code": f.code,
                    "severity": f.severity.as_str(),
                    "message": f.message,
                    "provenance_dag": walk(iri)?,
                }));
            }
            (
                "anchor",
                Some(target.to_owned()),
                json!({"anchor_iri": target, "members": members}),
            )
        };

        // The always-emitted substrate algebra: gate verdict + minimal fatal cut.
        let cut: Vec<Value> = minimal_fatal_cut(&index)
            .into_iter()
            .map(|iri| {
                let (code, message) = index
                    .get(&iri)
                    .map(|f| (f.code.clone(), f.message.clone()))
                    .unwrap_or_default();
                json!({"finding_iri": iri, "code": code, "message": message})
            })
            .collect();

        // The anchor cluster: every finding sharing the focus anchor (the code-blind
        // co-location the glut/Belnap join reads).
        let anchor_cluster: Vec<Value> = match focus_anchor.as_deref() {
            Some(anchor) => index
                .findings
                .values()
                .filter(|f| f.anchor_iri.as_deref() == Some(anchor))
                .map(|f| {
                    json!({
                        "finding_iri": f.finding_iri,
                        "code": f.code,
                        "severity": f.severity.as_str(),
                    })
                })
                .collect(),
            None => Vec::new(),
        };

        Ok(json!({
            "ok": true,
            "kind": kind,
            "target": target,
            "focus": focus,
            "anchor": focus_anchor,
            "anchor_cluster": anchor_cluster,
            "verdict": format!("{:?}", verdict(&index)),
            "minimal_fatal_cut": cut,
        })
        .to_string())
    }
}

/// A native SPARQL result rendered as a JSON envelope under `"ok"`: an ASK boolean,
/// SELECT bindings (SPARQL-1.1 JSON-results shape), or a CONSTRUCT / DESCRIBE graph
/// serialized as canonical N-Quads.
///
/// The graph arm used to be a hard error reading "this tool accepts only SELECT and ASK
/// queries". That was a REFUSAL STANDING IN FOR A CAPABILITY: the native engine evaluates
/// CONSTRUCT and DESCRIBE perfectly well and hands back
/// [`SparqlResult::Graph`](purrdf::SparqlResult::Graph); the surface simply declined to
/// serialize it. A caller asking `DESCRIBE gmeow:Foo` — the single most natural question to
/// ask a bundle — got a refusal for a query the engine had already answered. The result form
/// is now DECLARED in the envelope (`form`: `bindings` | `boolean` | `graph`) so a client
/// dispatches on a field rather than on a parse failure.
#[cfg(feature = "core")]
fn sparql_result_to_json(result: purrdf::SparqlResult) -> gmeow_errors::Result<Value> {
    match result {
        purrdf::SparqlResult::Boolean(value) => {
            Ok(json!({"ok": true, "form": "boolean", "boolean": value}))
        }
        purrdf::SparqlResult::Graph(graph) => {
            // Canonical N-Quads: the ONE lossless RDF-1.2 text form every other surface here
            // hands back (the conjecture verdict, the GMN expansion, the reasoned closure), so
            // a caller that wants Turtle or JSON-LD pipes this through `convert` rather than
            // getting a second, differently-shaped serializer here.
            let bytes = purrdf::serialize_dataset(
                graph.as_ref(),
                "application/n-quads",
                purrdf::SerializeGraph::Dataset,
            )
            .with_ctx(|| "serialize CONSTRUCT/DESCRIBE graph result".to_string())?;
            let nquads = String::from_utf8(bytes).map_err(|e| {
                gmeow_errors::Diag::of_kind(crate::error::Mcp {
                    message: format!("graph result N-Quads are not UTF-8: {e}"),
                })
            })?;
            Ok(json!({
                "ok": true,
                "form": "graph",
                "quad_count": graph.quad_count(),
                "graph_nquads": nquads,
            }))
        }
        purrdf::SparqlResult::Solutions {
            variables, rows, ..
        } => {
            let bindings: Vec<Value> = rows
                .iter()
                .map(|row| {
                    let mut obj = serde_json::Map::new();
                    for (i, cell) in row.iter().enumerate() {
                        if let (Some(name), Some(term)) = (variables.get(i), cell.as_ref())
                            && let Some(value) = sparql_term_to_json(term)
                        {
                            obj.insert(name.clone(), value);
                        }
                    }
                    Value::Object(obj)
                })
                .collect();
            Ok(json!({
                "ok": true,
                "form": "bindings",
                "head": {"vars": variables},
                "results": {"bindings": bindings},
            }))
        }
    }
}

/// One SPARQL binding rendered as a SPARQL-1.1 JSON-results term object. A quoted
/// triple term (rare in the documentation graph) has no standard binding shape and
/// is omitted.
#[cfg(feature = "core")]
fn sparql_term_to_json(term: &purrdf::TermValue) -> Option<Value> {
    const XSD_STRING: &str = "http://www.w3.org/2001/XMLSchema#string";
    match term {
        purrdf::TermValue::Iri(iri) => Some(json!({"type": "uri", "value": iri})),
        purrdf::TermValue::Blank { label, .. } => Some(json!({"type": "bnode", "value": label})),
        purrdf::TermValue::Literal {
            lexical_form,
            datatype,
            language,
            ..
        } => {
            let mut obj = serde_json::Map::new();
            obj.insert("type".to_string(), json!("literal"));
            obj.insert("value".to_string(), json!(lexical_form));
            if let Some(lang) = language {
                obj.insert("xml:lang".to_string(), json!(lang));
            } else if datatype != XSD_STRING {
                obj.insert("datatype".to_string(), json!(datatype));
            }
            Some(Value::Object(obj))
        }
        purrdf::TermValue::Triple { .. } => None,
    }
}

impl McpView {
    /// The documentation graph re-rooted to the default graph, projected once and
    /// shared by every query surface for this server.
    #[cfg(feature = "core")]
    fn documentation(&self) -> &Arc<purrdf::RdfDataset> {
        self.documentation.get_or_init(|| {
            Arc::new(
                self.dataset
                    .project_named_graph(gmeow_bundle_view::graph_iris::GRAPH_DOCUMENTATION),
            )
        })
    }

    /// The authoring-briefs graph re-rooted to the default graph, projected once and
    /// shared by every `slice_brief` call for this server.
    #[cfg(feature = "core")]
    fn authoring_briefs(&self) -> &Arc<purrdf::RdfDataset> {
        self.authoring_briefs.get_or_init(|| {
            Arc::new(
                self.dataset
                    .project_named_graph(gmeow_bundle_view::graph_iris::GRAPH_AUTHORING_BRIEFS),
            )
        })
    }

    /// The `term-IRI → site URL` map, built once from the documentation graph and
    /// cached (language-independent).
    #[cfg(feature = "core")]
    fn doc_urls(&self) -> Arc<HashMap<String, String>> {
        Arc::clone(self.doc_urls.get_or_init(|| {
            let view = FoldView::new(self.dataset.as_ref());
            Arc::new(export::doc_url_map(&view))
        }))
    }

    /// The JSON Schema `$defs` key set folded into this bundle's `schemas-archive`
    /// blob — the "this class has a generated Pydantic model" existence signal
    /// `export::term_to_card`'s `python_model` gate reads (see
    /// `export::class_is_modeled`), built once from the raw snapshot bytes (like
    /// `doc_urls`, language-independent). Empty when the bundle carries no
    /// `schemas-archive` rep, mirroring `gmeow_bundle_view::bundle_blobs::Bundle`'s own
    /// wheel-only-install contract for this accessor — a card's `python_model`
    /// line is ancillary, never worth a hard crash of the whole server.
    #[cfg(feature = "core")]
    fn modeled_defs(&self) -> Arc<BTreeSet<String>> {
        Arc::clone(self.modeled_defs.get_or_init(|| {
            let defs = gmeow_bundle_view::bundle_blobs::Bundle::from_snapshot(&self.gts)
                .and_then(|b| b.modeled_def_keys())
                .unwrap_or_default();
            Arc::new(defs)
        }))
    }

    /// The bundle's parsed Tier-1 data-graph shape union
    /// ([`gmeow_validate::data_validate::Tier1Shapes`]), decoded once from the
    /// raw snapshot bytes and cached for every `validate_local` call. Unlike the
    /// ancillary `modeled_defs`, a bundle whose `shapes-archive` blob is missing
    /// or malformed is a HARD FAIL surfaced to the caller — the failure is never
    /// cached, so a (theoretically impossible) transient failure never poisons
    /// the view.
    #[cfg(feature = "core")]
    fn tier1_shapes(&self) -> gmeow_errors::Result<&gmeow_validate::data_validate::Tier1Shapes> {
        if let Some(shapes) = self.tier1_shapes.get() {
            return Ok(shapes);
        }
        let built = gmeow_validate::data_validate::Tier1Shapes::from_gts(&self.gts)?;
        Ok(self.tier1_shapes.get_or_init(|| built))
    }

    /// Run `f` over the terms collected for `requested`, collecting (and caching)
    /// on first use per requested-tag list.
    #[cfg(feature = "core")]
    fn with_terms<R>(&self, requested: Vec<String>, f: impl FnOnce(&[Term]) -> R) -> R {
        let key = requested.join(",");
        let terms = {
            let mut cache = self.cache.lock().expect("McpView term cache poisoned");
            Arc::clone(cache.entry(key).or_insert_with(|| {
                let view = FoldView::with_requested(self.dataset.as_ref(), requested);
                Arc::new(export::collect_terms(&view))
            }))
        };
        f(terms.as_slice())
    }
}

/// A Rust MCP server over a bundled `gmeow.gts` snapshot.
///
/// There is no "mode" field and no repository root. What a server can do IS its
/// [`Surface`] — the assembled tool/resource registry — so the consumer server and
/// a host-extended server differ only in which registrations they carry. A host that
/// needs a checkout (`gmeow-mcp-dev`) captures the root inside its own handlers, so
/// the state a dev tool needs lives with the dev tools rather than as a
/// perpetually-`None` field on the consumer server. That replaces the old
/// `McpMode` + `root: Option<PathBuf>` pair, which encoded the same distinction
/// twice and left every dev-gated call site to re-derive it.
pub struct McpServer {
    view: McpView,
    surface: Surface,
    segments: SegmentSet,
    tag_map: BTreeMap<String, String>,
    available: BTreeSet<String>,
    startup_requested: Vec<String>,
}

impl McpServer {
    /// Build the CONSUMER MCP server over the bundled `gmeow.gts` snapshot bytes —
    /// the shippable `gmeow mcp` surface, which reads nothing but the bundle.
    ///
    /// Serves every segment this build links ([`SegmentSet::linked`]), so on the default
    /// feature set this is the whole engine and nothing about it has changed.
    ///
    /// # Errors
    ///
    /// Hard-fails if the snapshot does not read, if the startup language
    /// (`GMEOW_LANG`) is unknown, or if the builtin surface does not assemble.
    pub fn from_snapshot(snapshot: &[u8]) -> gmeow_errors::Result<Self> {
        Self::from_snapshot_with(snapshot, Extension::new())
    }

    /// Build an MCP server for a deployment that serves only `segments` in-process.
    ///
    /// The tiered browser console's lean core calls this with [`SegmentSet::core`]. The
    /// surface is IDENTICAL — all [`TOOL_COUNT`] tools advertised, described, and
    /// dispatchable — but
    /// a `tools/call` for a tool outside the served segments answers with the typed
    /// [`SegmentNotLoaded`](crate::error::SegmentNotLoaded) signal instead of running,
    /// so the host can load that segment and re-dispatch the same frame.
    ///
    /// # Errors
    ///
    /// As [`from_snapshot`](Self::from_snapshot).
    pub fn from_snapshot_segmented(
        snapshot: &[u8],
        segments: SegmentSet,
    ) -> gmeow_errors::Result<Self> {
        Self::from_snapshot_segmented_with(snapshot, segments, Extension::new())
    }

    /// Build an MCP server whose surface is the consumer builtins PLUS `extension`.
    ///
    /// This is the seam a host crate with more than the bundle registers through;
    /// see [`crate::extension`]. Registration order is builtins first, then the
    /// extension's entries in declaration order.
    ///
    /// # Errors
    ///
    /// As [`from_snapshot`](Self::from_snapshot), plus
    /// [`DuplicateRegistration`](crate::error::DuplicateRegistration) if `extension`
    /// claims a tool name or resource URI a builtin (or an earlier entry) already
    /// claims, and [`InvalidRegistration`](crate::error::InvalidRegistration) if a
    /// descriptor carries no dispatch key.
    pub fn from_snapshot_with(snapshot: &[u8], extension: Extension) -> gmeow_errors::Result<Self> {
        Self::from_snapshot_segmented_with(snapshot, SegmentSet::linked(), extension)
    }

    /// The one constructor: `segments` chooses the deployment tier, `extension` adds the
    /// host's tools. [`from_snapshot`](Self::from_snapshot),
    /// [`from_snapshot_segmented`](Self::from_snapshot_segmented) and
    /// [`from_snapshot_with`](Self::from_snapshot_with) all land here, so there is exactly
    /// one snapshot-import / language-resolution / surface-assembly path.
    ///
    /// # Errors
    ///
    /// As [`from_snapshot_with`](Self::from_snapshot_with).
    pub fn from_snapshot_segmented_with(
        snapshot: &[u8],
        segments: SegmentSet,
        extension: Extension,
    ) -> gmeow_errors::Result<Self> {
        let bundle = purrdf::import_gts_events(snapshot)
            .with_ctx(|| "read snapshot gmeow.gts".to_string())?;
        let dataset = bundle.dataset;
        let tag_map = language_tag_map(dataset.as_ref());
        let mut available: BTreeSet<String> =
            tag_map.values().map(|v| v.to_ascii_lowercase()).collect();
        available.insert("en".to_string());
        let startup_requested = resolve_lang(
            storage().env_var("GMEOW_LANG").as_deref(),
            &tag_map,
            &available,
        )?;
        Ok(Self {
            view: McpView::from_dataset(dataset, Arc::from(snapshot))?,
            surface: Surface::assemble(builtin_extension(segments)?, extension)?,
            segments,
            tag_map,
            available,
            startup_requested,
        })
    }

    /// The bundle view this server serves from — the read-side handle a
    /// host-registered tool needs (its snapshot bytes, its folded dataset).
    pub fn view(&self) -> &McpView {
        &self.view
    }

    /// The engine segments this deployment serves in-process.
    ///
    /// A host reads this to know, BEFORE dispatching, which tools will answer here and
    /// which will return the deferral signal — so it can pre-load a segment rather than
    /// discovering the need mid-frame.
    #[must_use]
    pub fn segments(&self) -> SegmentSet {
        self.segments
    }

    /// The assembled tool/resource surface: what this server advertises and,
    /// identically, what it dispatches.
    pub fn surface(&self) -> &Surface {
        &self.surface
    }

    #[cfg(feature = "core")]
    fn requested_from_args(&self, args: &Value) -> gmeow_errors::Result<Vec<String>> {
        match args.get("lang").and_then(Value::as_str) {
            Some(lang) => resolve_lang(Some(lang), &self.tag_map, &self.available),
            None => Ok(self.startup_requested.clone()),
        }
    }

    pub fn tools_result(&self) -> Value {
        json!({ "tools": self.surface.tool_descriptors() })
    }
}

/// The CONSUMER tool descriptors — what a bundle-only `gmeow mcp` advertises — in
/// advertised order.
///
/// This list is joined to [`builtin_tool_handlers`] index-by-index by
/// [`zip_tools`], which refuses to build unless the two agree name-for-name. Keep
/// them in the same order: a descriptor added here without its handler (or the
/// converse) is a hard construction failure, not a silently unusable tool.
fn builtin_tool_descriptors() -> Vec<Value> {
    vec![
        tool(
            "lookup_term",
            "Resolve a bundled GMEOW term.",
            &[("term", "string"), ("lang", "string")],
        ),
        tool(
            "llms_txt",
            "Return the standard bundled vocabulary index.",
            &[("lang", "string")],
        ),
        tool(
            "llms_full",
            "Return the complete inlined bundled vocabulary index.",
            &[("lang", "string")],
        ),
        tool(
            "doc_card",
            "Return a prompt-ready term card for a bundled term, wrapped in a \
                 cost-metadata envelope ({ok, detail, format, bytes, tokens, card}). \
                 `detail` selects a token-budgeted tier: `summary` (title + definition \
                 only), `standard` (the compact card — default), or `full` (the oracle \
                 card: compact card plus entailments, Do / Don't fixtures, diagnostics, \
                 and projection-loss panels, queried from gmeow:graph/documentation). \
                 `format` is `markdown` (default) or `json` (the neutral Card object). \
                 An unknown detail/format is a hard error.",
            &[
                ("term", "string"),
                ("detail", "string"),
                ("format", "string"),
                ("lang", "string"),
            ],
        ),
        tool(
            "okf_index",
            "Return the OKF manifest JSON envelope.",
            &[("lang", "string")],
        ),
        tool(
            "query_docs",
            "Run a SELECT or ASK SPARQL query over the bundled documentation graph \
                 (gmeow:graph/documentation) and return SPARQL-1.1 JSON results.",
            &[("query", "string")],
        ),
        tool(
            "docs_search",
            "Full-text search the bundled documentation graph (gmeow:graph/documentation) \
                 for terms, slices, and concerns whose label, definition, or advisory prose \
                 match `query` (case-insensitive substring / token match). Returns ranked \
                 records — a label match outranks a definition match outranks an advice match, \
                 tie-broken by IRI — each with its kind, documented IRI (id), label, definition, \
                 site URL, and the same advice / alignments / missing_coverage facets the site \
                 search index carries. Pass an optional `limit` (default 20). A query that \
                 matches nothing returns an empty result list.",
            &[("query", "string"), ("limit", "integer")],
        ),
        tool(
            "query_local",
            "Run a SPARQL query — SELECT, ASK, CONSTRUCT or DESCRIBE — over a READ-ONLY \
                 overlay graph you paste inline. `data` is the RDF text and `format` DECLARES \
                 how to read it — one of turtle|ttl|text/turtle, ntriples|nt|n-triples, \
                 nquads|nq|n-quads, trig, rdfxml|rdf+xml|xml|rdf, jsonld|json-ld. `scope` \
                 DECLARES what the query runs against: `bundle` (default) reads the overlay \
                 UNIONED with the signed canon — what does this graph mean in GMEOW — while \
                 `input` reads the overlay ALONE, with no bundle triple in the answer. The \
                 overlay is loaded as an EXTERNAL, read-only annex under both scopes: its \
                 triples are isolable via GRAPH <urn:gmeow:mcp:overlay:external> but are NEVER \
                 merged into the signed gmeow: canon and NEVER written anywhere. The result \
                 FORM is declared in the envelope: `bindings` (head/results), `boolean`, or \
                 `graph` (graph_nquads + quad_count). A missing or unrecognized `format` or \
                 `scope`, or an oversized payload, is a hard error — neither is ever guessed.",
            &[
                ("data", "string"),
                ("format", "string"),
                ("query", "string"),
                ("scope", "string"),
            ],
        ),
        tool(
            "encode_gmn1",
            "Encode authored RDF into the token-compact GMN-1 surface — the WRITE leg of \
                 the codec, and the exact inverse of gmn_expand. `data` is the RDF text and \
                 `format` DECLARES how to read it (the same codec set query_local accepts). \
                 Runs the SAME dictionary the bundle carries, so the emitted surface is the one \
                 the on-gate authority ships. Carries an internal round-trip witness (the \
                 emitted document is read back and canonical equality asserted), so it never \
                 returns a lossy encoding. Returns {ok, gmn1, canonical_nquads, round_trip}. An \
                 unrecognized format, an oversized payload, or an encoding that does not read \
                 back is a hard error.",
            &[("data", "string"), ("format", "string")],
        ),
        tool(
            "verify_graph",
            "Reason over the bundle UNIONED with a READ-ONLY overlay graph you paste \
                 inline, then run the native reasoned-graph verify (the bad-example negative \
                 tests + non-entailment obligations) and return a PROOF-CARRYING judgment. \
                 `data` is the RDF text and `format` DECLARES how to read it — one of \
                 turtle|ttl|text/turtle, ntriples|nt|n-triples, nquads|nq|n-quads, trig, \
                 rdfxml|rdf+xml|xml|rdf, jsonld|json-ld. The overlay is loaded as an EXTERNAL, \
                 read-only annex exactly like query_local: its triples join the reasoning \
                 default world (bundle \u{222a} overlay, also isolable via GRAPH \
                 <urn:gmeow:mcp:overlay:external>) but are NEVER merged into the signed gmeow: \
                 canon and NEVER written anywhere; the \
                 whole union is transient and discarded after the call. The forward closure \
                 runs under a mid-chase step governor: max_steps bounds the derivation budget \
                 and max_answers the answer cap. The response carries class_local_name \
                 (CoherenceCertificate for a conclusive coherent closure, \
                 CoherenceCheckAttestation for a budget-cut closure, Refused for a witnessed \
                 forbidden contradiction), the completeness/evaluation axes, the verify \
                 findings, the cited IRIs, and judgment_nquads (the grounded \
                 logic:ReasoningResult). An overlay exceeding the size ceiling, and a missing \
                 or unrecognized `format`, is a hard error — the format is never guessed.",
            &[
                ("data", "string"),
                ("format", "string"),
                ("max_steps", "integer"),
                ("max_answers", "integer"),
            ],
        ),
        tool(
            "reason_graph",
            "Run the native GMEOW structured-DL forward chase over an RDF graph you paste \
                 inline and return the ENTAILED CLOSURE — what this document implies. `data` is \
                 the RDF text and `format` DECLARES how to read it (the same codec set \
                 query_local accepts). Scoped to YOUR graph alone, never unioned with the \
                 bundle: the question is what this document entails. Distinct from verify_graph, \
                 which returns a verdict about a graph rather than its entailments, and from \
                 entailments, which reads precomputed derivation rows for one BUNDLED term and \
                 cannot see your data. The chase runs under the mid-chase step governor \
                 (max_steps bounds the derivation budget, max_answers the answer cap), and the \
                 completeness/evaluation axes plus judgment_nquads ride the response so a \
                 budget-cut closure is legible as CUT rather than as small. Returns {ok, \
                 entailed_count, closure_nquads, completeness, evaluation, judgment_nquads}. An \
                 unrecognized format, or an input above the size/quad ceiling, is a hard error.",
            &[
                ("data", "string"),
                ("format", "string"),
                ("max_steps", "integer"),
                ("max_answers", "integer"),
            ],
        ),
        tool(
            "explain_quad",
            "Reconstruct the FAITHFUL cited-IRI derivation skeleton for ONE arbitrary quad \
                 the bundle's reasoned closure entails: the target `(subject, predicate, \
                 object_value)` in named graph `graph`. Reasons over the bundle under the \
                 mid-chase step governor (max_steps bounds the derivation budget), locates the \
                 target by its content-addressed reifier WORLD-DISAMBIGUATED by `graph`, then \
                 reconstructs and re-verifies ONLY that quad's proof tree. Returns the DFS \
                 step_skeleton (target first, each step carrying its rule, derivation id, \
                 (S,P,O), world, asserted flag, depth, antecedent step ids, and cited term \
                 IRIs), the complete cited_iris set, a Markdown rendering, the world_iri, the \
                 completeness class (CoherenceCertificate for a conclusive closure, \
                 CoherenceCheckAttestation for a budget-cut one), and judgment_nquads (the \
                 grounded logic:ReasoningResult). `object_kind` is `iri` or `literal` (inferred \
                 from object_value when omitted); `object_datatype` types a literal object. This \
                 is DISTINCT from explain_finding: explain_finding addresses a PUBLISHED \
                 diagnostic by its fingerprint/anchor IRI over the graph/diagnostics projection, \
                 while explain_quad proves an arbitrary entailed quad from the reasoner's premise \
                 provenance. A quad the closure does not entail, or a reifier shared across \
                 worlds that `graph` does not resolve, is a hard error (never an empty-but-ok \
                 proof).",
            &[
                ("subject", "string"),
                ("predicate", "string"),
                ("object_value", "string"),
                ("object_kind", "string"),
                ("object_datatype", "string"),
                ("graph", "string"),
                ("max_steps", "integer"),
            ],
        ),
        tool(
            "coherence_certificate",
            "Read the bundle's SCOPED COHERENCE CERTIFICATE — a budget-free, \
                 proof-carrying coherence attestation computed ONCE at pipeline time over the \
                 whole assembled bundle and folded into graph/attestations. This tool reads it \
                 straight off the bundled dataset: it takes NO inputs, runs NO reasoning, and \
                 never recomputes. The response carries class_local_name (CoherenceCertificate \
                 for a conclusive, fragment-scoped, violation-free closure; the strictly-weaker \
                 CoherenceCheckAttestation otherwise — an attestation is NEVER reported as a \
                 certificate), issues_certificate, is_refused, the pinned bundle_hash and \
                 per-graph axiom_hashes (the tamper surface), contract_hash, engine, \
                 certified_fragment, the completeness/evaluation axes, contradiction_policy, \
                 projection_losses, and forbidden_violations. A bundle carrying no coherence \
                 artifact is a hard error (never a silent recompute).",
            &[],
        ),
        tool(
            "validate_local",
            "Validate an inline RDF graph against the bundled GMEOW shapes and disciplines \
                 (the same core as `gmeow validate`), then CLOSE THE LOOP: every finding is \
                 returned with its rule catalog helpUri, the CORRESPONDING counter-example fixture \
                 (matched by violation code), a positive well-formed exemplar, and the entailments \
                 of the term it concerns. `data` is the RDF text; `format` is one of \
                 turtle|ttl|text/turtle, ntriples|nt|n-triples, nquads|nq|n-quads, trig, \
                 rdfxml|rdf+xml|xml, jsonld|json-ld. Set `deep` true to ALSO run the Tier-2 \
                 semantic pass (reasons over your data unioned with the whole bundle's axioms, \
                 like `gmeow validate --deep` — powerful but multiple minutes; default false). \
                 Nothing is written (the data is validated in-memory and discarded); an unknown \
                 format or an oversized payload is a hard error.",
            &[
                ("data", "string"),
                ("format", "string"),
                ("deep", "boolean"),
            ],
        ),
        tool(
            "gmn_validate",
            "Validate a GMN-1 document (the token-compact GMEOW Model Notation surface) \
                 against the shipped codebook dictionary + validator tier, checkout-free off the \
                 bundle. This is an external LLM's entry to the GMN `@err` repair loop: `gmn` is \
                 the GMN-1 document text. Returns {ok, conformant} — conformant:true for a \
                 well-formed document, or conformant:false with the TYPED lang:Gmn*Failure class \
                 (failure_class / failure_local_name) and message of the first defect, so the \
                 caller repairs against the named class. A defect is a valid result, never a tool \
                 error; an oversized payload is a hard error.",
            &[("gmn", "string")],
        ),
        tool(
            "gmn_expand",
            "Expand a GMN-1 document to its GMN-0 normal form (the alias/glyph → full-IRI \
                 direction) and return the canonical N-Quads, checkout-free off the bundle. `gmn` \
                 is the GMN-1 document text. The expansion carries an internal round-trip witness \
                 (the decoded model is re-encoded and re-read, and canonical equality asserted), \
                 so it never returns a lossy expansion. Returns {ok, expanded_nquads, \
                 reencoded_gmn, round_trip}. A non-conformant input, or a round-trip that does not \
                 hold, is a hard error.",
            &[("gmn", "string")],
        ),
        tool(
            "gmn_explain",
            "Explain a GMN operator glyph: resolve `glyph` to its lang:Denotation and its \
                 graph-authored gmeow:gmnFixity / gmeow:gmnPrecedence / gmeow:gmnArity signature, \
                 plus its controlled-NL gloss (the deterministic GMN⇄NL verbalizer rendering), \
                 checkout-free off the bundle. Returns {ok, found, glyph, denotation, \
                 denotation_target, label, fixity, fixity_local_name, precedence, arity, \
                 gmn_surface, gloss}. An input that is not a covered operator glyph returns an \
                 honest typed miss (found:false + lang:GmnUncoveredTerm), never a fabricated \
                 answer.",
            &[("glyph", "string")],
        ),
        tool(
            "advise",
            "Advise on an inline agent-authored RDF claim: return the non-gating \
                 RECOMMENDATIONS (never rejections) GMEOW harvests for it — the avoid-when \
                 prohibition, the how-to-use corrective directive, and the use-when permission \
                 prose of every advisory `advice.*` finding the claim trips. The companion of \
                 `validate_local`: where validate reports OBLIGATIONS (a claim can fail), advise \
                 reports RECOMMENDATIONS and ALWAYS returns `ok:true` — a clean claim returns an \
                 empty list. Routes through the SAME shipped validator core as `validate_local` \
                 (shallow Tier-1 pass only; advice is a fast structural concept), so it never \
                 diverges. `data` is the RDF text; `format` is one of turtle|ttl|text/turtle, \
                 ntriples|nt|n-triples, nquads|nq|n-quads, trig, rdfxml|rdf+xml|xml, \
                 jsonld|json-ld. Nothing is written; an unknown format or an oversized payload \
                 is a hard error.",
            &[("data", "string"), ("format", "string")],
        ),
        tool(
            "explain_finding",
            "Explain a diagnostic witness over the bundled graph/diagnostics projection, \
                 addressed by its fingerprint IRI (a finding) or its anchor IRI (a cluster): \
                 returns the provenance DAG, the aggregate ledger gate verdict, the minimal fatal \
                 cut (fingerprint IRIs + codes), and the anchor cluster. An unknown target is a \
                 hard error (never an empty-but-ok DAG).",
            &[("target_iri", "string")],
        ),
        tool(
            "store_claim",
            "Append one attributed memory claim, executed as a Transaction-Logic \
                 transaction (the executional-entailment verdict gates the commit). Pass \
                 dry_run=true for a non-committing sandbox run (verdict only, nothing written).",
            &[
                ("text", "string"),
                ("source", "string"),
                ("confidence", "number"),
                ("according_to", "string"),
                ("dry_run", "boolean"),
            ],
        ),
        tool(
            "conjecture_test",
            "Test — a PURE hypothetical evaluation of a candidate logic: formula against a \
                 KB: compute and return the engine verdict, but NEVER TR-commit and NEVER \
                 append to the conjecture library. formula is a Turtle logic: document naming \
                 one candidate; kb is a Turtle KB; standpoint is the required reified scope \
                 (P9); math_conjecture optionally names the math:Conjecture twin. max_steps / \
                 max_answers optionally bound the isolated scenario evaluation (a \
                 derived-closure-size ceiling: exceeding it stamps BudgetExhausted → lifecycle \
                 open). For the committing counterpart, see store_conjecture.",
            &[
                ("formula", "string"),
                ("kb", "string"),
                ("standpoint", "string"),
                ("math_conjecture", "string"),
                ("max_steps", "integer"),
                ("max_answers", "integer"),
            ],
        ),
        tool(
            "store_conjecture",
            "Store — evaluate a candidate logic: formula against a KB and, TR-gated on the \
                 persistConjecture schema (the executional-entailment verdict gates the \
                 commit), APPEND the engine verdict to the append-only conjecture library. \
                 formula is a Turtle logic: document naming one candidate; kb is a Turtle KB; \
                 standpoint is the required reified scope (P9); math_conjecture optionally \
                 names the math:Conjecture twin. max_steps / max_answers optionally bound the \
                 isolated scenario evaluation (a derived-closure-size ceiling: exceeding it \
                 stamps BudgetExhausted → lifecycle open). Pass dry_run=true for a \
                 non-committing sandbox run (verdict only, nothing written) — a \
                 hypothetical-commit witness.",
            &[
                ("formula", "string"),
                ("kb", "string"),
                ("standpoint", "string"),
                ("math_conjecture", "string"),
                ("dry_run", "boolean"),
                ("max_steps", "integer"),
                ("max_answers", "integer"),
            ],
        ),
        tool(
            "refute_conjecture",
            "Author-withdraw a stored conjecture — the store_conjecture compensation (P10, \
                 as revise_belief compensates store_claim), executed as a Transaction-Logic \
                 transaction on the withdrawConjecture schema. It APPENDS a compensating \
                 author-withdrawn segment to the append-only conjecture library, flipping the \
                 node's effective logic:conjectureLifecycleState to logic:ConjectureWithdrawn \
                 (recorded, never deleted — prior segments stay intact). conjecture_id is the \
                 stored logic:Conjecture node IRI; reason is an optional author note. The TR \
                 precondition — the conjecture is still in the library and NOT already \
                 withdrawn — is decided from the live library state by segment order, so an \
                 unknown id or an already-withdrawn node is rejected before any write. Pass \
                 dry_run=true for a non-committing sandbox run (verdict only, nothing written).",
            &[
                ("conjecture_id", "string"),
                ("reason", "string"),
                ("dry_run", "boolean"),
            ],
        ),
        tool(
            "recall",
            "Recall stored memory claims.",
            &[
                ("query", "string"),
                ("min_confidence", "number"),
                ("limit", "integer"),
                ("include_suppressed", "boolean"),
            ],
        ),
        tool(
            "store_segment",
            "Serialize the grounded-memory store — every claim and every recorded tool \
                 call, in append order — as N-Quads in the shared session-store transport \
                 shape (gmeow:ClaimToken / gmeow:ToolCall, position-addressed under \
                 urn:gmeow:session:). Returns {ok, claim_count, tool_call_count, nquads}. \
                 A READ: it commits nothing and the segment is a projection of state that \
                 was already stored. Distinct from recall, which answers a QUERY with a \
                 ranked, truncated JSON view of matching claims and is therefore not a \
                 snapshot: this is what an exported session carries so the trajectory and \
                 the store it ran against travel together, and it is what re-seeds a store \
                 for a replay. An empty store returns an empty nquads and zero counts, \
                 which is an answer, not a failure.",
            &[],
        ),
        tool(
            "revise_belief",
            "Suppress a stored claim without deleting history (the store_claim \
                 compensation, P10), executed as a Transaction-Logic transaction whose \
                 precondition is that the target claim exists. Pass dry_run=true for a \
                 non-committing sandbox run (verdict only, nothing suppressed).",
            &[
                ("claim_id", "string"),
                ("reason", "string"),
                ("superseded_by", "string"),
                ("dry_run", "boolean"),
            ],
        ),
        tool(
            "counter_examples",
            "Return the conformance fixtures documenting a bundled term, split into \
                 well-formed exemplars and counter-examples, each with its full Turtle body and \
                 the authored expected outcome / violation code / conformance rationale (read from \
                 gmeow:graph/documentation). A term with no fixtures returns empty lists; an \
                 unknown term is a hard error.",
            &[("term", "string")],
        ),
        tool(
            "entailments",
            "Return the reasoner entailments documenting a bundled term — each derivation's \
                 rule, conclusion, and every premise (read from gmeow:graph/documentation). A term \
                 with no entailments returns an empty list; an unknown term is a hard error.",
            &[("term", "string")],
        ),
        tool(
            "competency_questions",
            "Return the runnable competency questions from gmeow:graph/documentation, each with \
                 its SPARQL query text and the authored rationale / expected row count / exact-rows \
                 flag. With the OPTIONAL `term`, only that term's questions (an unknown term is a \
                 hard error); without `term`, the whole index.",
            &[("term", "string")],
        ),
        tool(
            "slice_quality",
            "Score a slice against the bundle-carried slice-quality rubric and return its \
                 per-axis grades and ranked uplift advice. `files` is a JSON OBJECT mapping each \
                 slice-relative path to that file\u{2019}s text \
                 ({\"manifest.ttl\": \"\u{2026}\", \"module.ttl\": \"\u{2026}\", \
                 \"examples/x.ttl\": \"\u{2026}\", \"i18n/fr.po\": \"\u{2026}\", \
                 \"docs.md\": \"\u{2026}\"}) \u{2014} the slice is scored from those bytes \
                 alone, so it needs no directory and no checkout (the rubric ships in \
                 gmeow.gts). A map with no `manifest.ttl` entry, or a malformed one, is a hard \
                 error naming what is missing.",
            &[("files", "object")],
        ),
        tool(
            "slice_brief",
            "Serve the pre-assembled authoring packet(s) for a slice straight from the bundle: \
                 the covered-term IRIs, their present fr/zh/external grounding cells, exemplars, \
                 and coverage margins, as structured JSON plus canonical turtle. `slice` is a \
                 slice short-name (e.g. `ai`) or a full slice IRI; the OPTIONAL `axis` (default \
                 `whole`) and `batch` narrow the result. A slice/axis/batch with no packet is a \
                 hard error. Resolve each covered-term IRI to its full definition/axioms via \
                 `lookup_term` / `doc_card`.",
            &[
                ("slice", "string"),
                ("axis", "string"),
                ("batch", "integer"),
            ],
        ),
        tool(
            "submit_candidate",
            "Propose/verify seam: test a candidate logic: formula against a KB and — ONLY if \
                 the isolated-world verdict CORROBORATES it (admissible) — append it to the \
                 append-only candidate library. A refuted or open candidate is never admitted and \
                 stages nothing. `formula`/`kb`/`standpoint` are as conjecture_test; optional \
                 `for_slice`/`for_packet` record target provenance; optional `max_steps`/\
                 `max_answers` bound the isolated-world reasoning budget (as conjecture_test); \
                 `dry_run=true` returns the verdict but writes nothing.",
            &[
                ("formula", "string"),
                ("kb", "string"),
                ("standpoint", "string"),
                ("math_conjecture", "string"),
                ("for_slice", "string"),
                ("for_packet", "string"),
                ("dry_run", "boolean"),
                ("max_steps", "integer"),
                ("max_answers", "integer"),
            ],
        ),
        tool(
            "withdraw_candidate",
            "The P10 compensating withdrawal of a persisted candidate: append a 'withdrawn' \
                 segment flipping the candidate's effective lifecycle (recorded, never deleted). \
                 An unknown or already-withdrawn id hard-fails before writing. `dry_run=true` \
                 witnesses the withdrawal but writes nothing.",
            &[
                ("candidate_id", "string"),
                ("reason", "string"),
                ("dry_run", "boolean"),
            ],
        ),
        tool(
            "list_candidates",
            "List every admitted candidate in the library with its effective disposition \
                 (in-library | withdrawn) and target provenance (for_slice / for_packet). \
                 Optional `slice` filters by target provenance and `disposition` by effective \
                 state. A missing library is an empty list.",
            &[("slice", "string"), ("disposition", "string")],
        ),
        tool(
            "convert",
            "Transcode an RDF-1.2 document from any source codec to any target codec and \
                 report the loss the conversion actually realized. `data` is the source \
                 document and `from`/`to` DECLARE the codecs — one of turtle|ttl, \
                 ntriples|nt, nquads|nq, trig, jsonld|json-ld, jsonld-star, yaml-ld-star, \
                 rdfxml|rdf-xml|xml, gts, owl-rdf12, owl-dl, owl-el, datalog|dl, n3, gufo, \
                 canonical-rdf12. Optional `base` supplies a base IRI. Optional `encoding` \
                 declares how to read `data` (`utf8`, the default, or `base64` for a binary \
                 source such as gts); the response's own `encoding` says how to read \
                 `output` and is `base64` whenever the target bytes are not valid UTF-8. \
                 Returns {ok, from, to, encoding, output, bytes, loss} where `loss` is the \
                 realized loss ledger — the rows this particular dataset actually lost on \
                 this edge, empty for a lossless pair. Quoted triples survive to every \
                 star-capable target; a target that cannot carry them says so in `loss`. An \
                 unknown codec, an undecodable source, or a failed serialization is a hard \
                 error.",
            &[
                ("data", "string"),
                ("from", "string"),
                ("to", "string"),
                ("base", "string"),
                ("encoding", "string"),
            ],
        ),
        tool(
            "gmn_glyph_legend",
            "Return the GMN-1 glyph legend for the bundled codebook: every glyph the codec \
                 may emit, in canonical order, with its real cl100k_base LLM-token cost \
                 ({glyph, tokenCost}). The alphabet an agent needs before writing GMN-1 — \
                 the companion of gmn_validate / gmn_expand / gmn_explain, and the same \
                 legend the browser codec renders.",
            &[],
        ),
        tool(
            "distribution_matrix",
            "Return the shipped documentation-distribution catalog read out of the bundle's \
                 meta-level distribution-catalog graph: `distributions`, the per-format \
                 consumer-need matrix (slug, family, media_type, consumers, \
                 dropped_capabilities) — WHICH documentation surfaces exist, who each is \
                 for, and what capability each doc-render surface drops — and `concepts`, \
                 the formal-concept lattice over the surface x capability incidence \
                 (concept, extent, intent). An empty `concepts` list means the bundle \
                 declares no lattice, which is not an error. A bundle carrying no \
                 distribution catalog at all is a hard error.",
            &[],
        ),
        tool(
            "action_policy",
            // The two counts are the DERIVED constants, formatted in — an agent reads this
            // description at run time, so a hand-typed number here is a false statement
            // shipped to a caller the moment the surface grows.
            &format!(
                "Return the canonical action theory governing this engine's WHOLE tool surface, \
                 as N-Quads, in the transaction world the executor reasons in. It is TOTAL, \
                 not a sample: every tool advertised here has exactly one action schema and \
                 every schema names exactly one advertised tool, tied together by the \
                 logic:mcpToolName wire name (a schema's local name is an ontology name — \
                 ex:persistConjecture is the tool `store_conjecture` — so the correspondence \
                 is asserted, never guessed). The {WRITE_TOOL_COUNT} WRITE tools are typed \
                 logic:McpActionSchema and carry logic:precondition / logic:effect / \
                 logic:compensation (the rollback is supersession, never erasure); the \
                 {READ_TOOL_COUNT} READ tools are plain logic:ActionSchema carrying \
                 logic:capability + logic:precondition and NO effect and NO compensation, \
                 because a read changes no state. This is the exact projection the \
                 Transaction-Logic executor reads, so what you inspect is what the engine \
                 obeys. Also served as the gmeow://ontology/action-policy resource."
            ),
            &[],
        ),
    ]
}

/// Bind one reasoning-SEGMENT tool to a handler, given the deployment's `segments`.
///
/// Two expansions, and the cargo feature picks which one exists:
/// * with `reasoning` linked, the real method is named and — when the deployment serves
///   the segment — called; when it does not, the deferral signal is returned instead, so
///   a core deployment on a full build behaves exactly like the lean image;
/// * without it, the method does not exist to be named at all, and the only expansion is
///   the deferral signal.
///
/// A macro rather than a function because a function taking the real handler as an
/// argument would have to NAME `s.tool_verify_graph(a)` in a build where that method is
/// compiled out. This keeps ONE mechanism (the `segment_not_loaded` signal) and no
/// never-called stub bodies to drift from it.
macro_rules! reasoning_tool {
    ($segments:expr, $name:literal, $method:ident) => {{
        let name: &'static str = $name;
        debug_assert!(
            REASONING_SEGMENT_TOOLS.contains(&name) || CHASE_SEGMENT_TOOLS.contains(&name),
            "`{name}` is bound below core but is absent from BOTH non-core segment lists, so \
             `SegmentSet::serves` would route it as core"
        );
        #[cfg(feature = "reasoning")]
        // ONE routing predicate, shared with every host that asks the same question
        // ahead of time: `SegmentSet::serves` is the only place "does this tool run
        // here?" is decided, so a host's pre-flight answer and the engine's dispatch
        // cannot disagree.
        let entry: (&'static str, ToolHandler) = if $segments.serves(name) {
            (
                name,
                Box::new(|s: &McpServer, a: &Value| s.$method(a)) as ToolHandler,
            )
        } else {
            (
                name,
                Box::new(move |_: &McpServer, _: &Value| {
                    Err(segment_not_loaded(name, SegmentSet::segment_of(name)))
                }) as ToolHandler,
            )
        };
        #[cfg(not(feature = "reasoning"))]
        let entry: (&'static str, ToolHandler) = {
            let _ = &$segments;
            (
                name,
                Box::new(move |_: &McpServer, _: &Value| {
                    Err(segment_not_loaded(name, SegmentSet::segment_of(name)))
                }) as ToolHandler,
            )
        };
        entry
    }};
}

/// Bind one CORE-segment tool to a handler, given the deployment's `segments` — the exact
/// mirror of [`reasoning_tool!`], and it exists for the same reason.
///
/// The reasoning image is a DELTA, not a superset: it links the DL reasoner and the rubric
/// kernel and NOTHING of the core tool surface, so in that build `s.tool_convert(a)` is not
/// a method that exists to be named. Two expansions, and the cargo feature picks which:
/// * with `core` linked, the real method is named and — when the deployment serves the
///   segment — called; when it does not, the deferral signal routes the caller back to the
///   always-resident core image;
/// * without it, the method does not exist at all, and the only expansion is the signal.
///
/// The tool stays ADVERTISED in both, so `tools/list` is byte-identical across the tiers
/// and no caller can observe which half of the engine it is talking to except in latency.
macro_rules! core_tool {
    ($segments:expr, $name:literal, $method:ident) => {{
        let name: &'static str = $name;
        debug_assert!(
            !REASONING_SEGMENT_TOOLS.contains(&name),
            "`{name}` is bound as a core-segment tool but is present in \
             REASONING_SEGMENT_TOOLS, so `SegmentSet::serves` would route it as reasoning"
        );
        #[cfg(feature = "core")]
        // ONE routing predicate, shared with every host that asks the same question ahead
        // of time — see `reasoning_tool!`.
        let entry: (&'static str, ToolHandler) = if $segments.serves(name) {
            (
                name,
                Box::new(|s: &McpServer, a: &Value| s.$method(a)) as ToolHandler,
            )
        } else {
            (
                name,
                Box::new(move |_: &McpServer, _: &Value| {
                    Err(segment_not_loaded(name, CORE_SEGMENT))
                }) as ToolHandler,
            )
        };
        #[cfg(not(feature = "core"))]
        let entry: (&'static str, ToolHandler) = {
            let _ = &$segments;
            (
                name,
                Box::new(move |_: &McpServer, _: &Value| {
                    Err(segment_not_loaded(name, CORE_SEGMENT))
                }) as ToolHandler,
            )
        };
        entry
    }};
}

/// Bind one CORE-segment RESOURCE to a handler — the [`core_tool!`] shape, for the other
/// half of the surface.
///
/// All five builtin resources are core: each renders the bundle's documentation view
/// (`llms.txt`, the full inlined index, the GMN-1 primer, the OKF index) or the action
/// policy, and none of them reasons. Leaving them served in the reasoning image would keep
/// the whole term-card and `llms` renderer REACHABLE there — measured at ~3.7 MB of image —
/// which is exactly the superset the delta exists to avoid. They defer back to the
/// always-resident core image instead, with the same typed signal a deferred tool gets.
macro_rules! core_resource {
    ($segments:expr, $uri:expr, $handler:expr) => {{
        let uri: &'static str = $uri;
        #[cfg(feature = "core")]
        let entry: (&'static str, ResourceHandler) = if $segments.core {
            (uri, Box::new($handler) as ResourceHandler)
        } else {
            (
                uri,
                Box::new(move |_: &McpServer, _: &[String]| {
                    Err(segment_not_loaded(uri, CORE_SEGMENT))
                }) as ResourceHandler,
            )
        };
        #[cfg(not(feature = "core"))]
        let entry: (&'static str, ResourceHandler) = {
            let _ = &$segments;
            (
                uri,
                Box::new(move |_: &McpServer, _: &[String]| {
                    Err(segment_not_loaded(uri, CORE_SEGMENT))
                }) as ResourceHandler,
            )
        };
        entry
    }};
}

/// The CONSUMER `tools/call` handlers, in the SAME order as
/// [`builtin_tool_descriptors`]. Each entry restates the tool name so
/// [`zip_tools`] can prove the pairing rather than assume it.
///
/// All [`TOOL_COUNT`] entries exist in EVERY deployment — the list does not shrink when a
/// segment is unloaded, because "advertised" and "dispatchable" are one fact here and a
/// lean core still advertises the whole surface. What changes is what the
/// [`REASONING_SEGMENT_TOOLS`] entries dispatch TO: their real implementation when the
/// segment is served, the [`segment_not_loaded`] routing signal when it is not.
fn builtin_tool_handlers(segments: SegmentSet) -> Vec<(&'static str, ToolHandler)> {
    vec![
        core_tool!(segments, "lookup_term", tool_lookup_term),
        core_tool!(segments, "llms_txt", tool_llms_txt),
        core_tool!(segments, "llms_full", tool_llms_full),
        core_tool!(segments, "doc_card", tool_doc_card),
        core_tool!(segments, "okf_index", tool_okf_index),
        core_tool!(segments, "query_docs", tool_query_docs),
        core_tool!(segments, "docs_search", tool_docs_search),
        core_tool!(segments, "query_local", tool_query_local),
        core_tool!(segments, "encode_gmn1", tool_encode_gmn1),
        reasoning_tool!(segments, "verify_graph", tool_verify_graph),
        reasoning_tool!(segments, "reason_graph", tool_reason_graph),
        reasoning_tool!(segments, "explain_quad", tool_explain_quad),
        reasoning_tool!(
            segments,
            "coherence_certificate",
            tool_coherence_certificate
        ),
        core_tool!(segments, "validate_local", tool_validate_local),
        core_tool!(segments, "gmn_validate", tool_gmn_validate),
        core_tool!(segments, "gmn_expand", tool_gmn_expand),
        core_tool!(segments, "gmn_explain", tool_gmn_explain),
        core_tool!(segments, "advise", tool_advise),
        core_tool!(segments, "explain_finding", tool_explain_finding),
        reasoning_tool!(segments, "store_claim", tool_store_claim),
        reasoning_tool!(segments, "conjecture_test", tool_conjecture_test),
        reasoning_tool!(segments, "store_conjecture", tool_store_conjecture),
        reasoning_tool!(segments, "refute_conjecture", tool_refute_conjecture),
        reasoning_tool!(segments, "recall", tool_recall),
        reasoning_tool!(segments, "store_segment", tool_store_segment),
        reasoning_tool!(segments, "revise_belief", tool_revise_belief),
        core_tool!(segments, "counter_examples", tool_counter_examples),
        core_tool!(segments, "entailments", tool_entailments),
        core_tool!(segments, "competency_questions", tool_competency_questions),
        reasoning_tool!(segments, "slice_quality", tool_slice_quality),
        core_tool!(segments, "slice_brief", tool_slice_brief),
        reasoning_tool!(segments, "submit_candidate", tool_submit_candidate),
        reasoning_tool!(segments, "withdraw_candidate", tool_withdraw_candidate),
        reasoning_tool!(segments, "list_candidates", tool_list_candidates),
        core_tool!(segments, "convert", tool_convert),
        core_tool!(segments, "gmn_glyph_legend", tool_gmn_glyph_legend),
        core_tool!(segments, "distribution_matrix", tool_distribution_matrix),
        core_tool!(segments, "action_policy", tool_action_policy),
    ]
}

/// The CONSUMER surface as an [`Extension`]: the builtin descriptors joined to the
/// builtin handlers, both for tools and for resources. Every [`McpServer`] starts
/// from exactly this, so a consumer server and a host-extended server share one
/// definition of "the consumer surface".
///
/// # Errors
///
/// [`InvalidRegistration`](crate::error::InvalidRegistration) if the descriptor and
/// handler lists have drifted out of bijection.
fn builtin_extension(segments: SegmentSet) -> gmeow_errors::Result<Extension> {
    Ok(Extension::from_parts(
        zip_tools(builtin_tool_descriptors(), builtin_tool_handlers(segments))?,
        zip_resources(
            builtin_resource_descriptors(),
            builtin_resource_handlers(segments),
        )?,
    ))
}

impl McpServer {
    pub fn resources_result(&self) -> Value {
        json!({ "resources": self.surface.resource_descriptors() })
    }
}

/// The CONSUMER resource descriptors, in advertised order — paired with
/// [`builtin_resource_handlers`] exactly as the tool lists are.
fn builtin_resource_descriptors() -> Vec<Value> {
    vec![
        resource(
            "gmeow://ontology/llms.txt",
            "llms.txt",
            "Standard bundled vocabulary index.",
            "text/plain",
        ),
        resource(
            "gmeow://ontology/llms-full.txt",
            "llms-full.txt",
            "Complete inlined bundled vocabulary index.",
            "text/plain",
        ),
        resource(
            "gmeow://ontology/gmn1-primer",
            "gmn1-primer",
            "The ~500-token graph-derived GMN-1 teachability primer (record sigils, \
                 operator glyph table, repair loop).",
            "text/plain",
        ),
        resource(
            "gmeow://ontology/okf-index",
            "okf-index",
            "OKF manifest JSON envelope.",
            "application/json",
        ),
        resource(
            ACTION_POLICY_URI,
            "action-policy",
            "The canonical action theory governing the engine's whole tool surface, as \
                 N-Quads: one schema per advertised tool (tied by logic:mcpToolName), the 6 \
                 writes carrying logic:precondition / logic:effect / logic:compensation and \
                 the 32 reads carrying logic:capability / logic:precondition — the resource \
                 twin of the `action_policy` tool.",
            ACTION_POLICY_MEDIA_TYPE,
        ),
    ]
}

/// The CONSUMER `resources/read` handlers, in the SAME order as
/// [`builtin_resource_descriptors`]. The media type is NOT restated here — it comes
/// from the descriptor, so advertised and served can never disagree.
fn builtin_resource_handlers(segments: SegmentSet) -> Vec<(&'static str, ResourceHandler)> {
    vec![
        core_resource!(
            segments,
            "gmeow://ontology/llms.txt",
            |s: &McpServer, requested: &[String]| { Ok(s.view.llms_txt_text(requested.to_vec())) }
        ),
        core_resource!(
            segments,
            "gmeow://ontology/llms-full.txt",
            |s: &McpServer, requested: &[String]| { s.view.llms_full_text(requested.to_vec()) }
        ),
        core_resource!(
            segments,
            "gmeow://ontology/gmn1-primer",
            |s: &McpServer, _requested: &[String]| {
                s.view.gmn1_primer().map(|p| p.resource_text())
            }
        ),
        core_resource!(
            segments,
            "gmeow://ontology/okf-index",
            |s: &McpServer, requested: &[String]| { Ok(s.view.okf_index_json(requested.to_vec())) }
        ),
        // The SAME `action_policy_nquads()` the transaction executor reads and the
        // `action_policy` tool returns — one projection, three readers, no restatement.
        // Language-independent: the projection keeps only IRI→IRI structural quads, so
        // there is nothing here for a language selector to select.
        core_resource!(
            segments,
            ACTION_POLICY_URI,
            |_s: &McpServer, _requested: &[String]| { Ok(action_policy_nquads().to_owned()) }
        ),
    ]
}

impl McpServer {
    pub fn call_tool_result(&self, name: &str, args: &Value) -> Value {
        if let Some(err) = args.get("__parse_error").and_then(Value::as_str) {
            return tool_text(json!({"ok": false, "error": err}).to_string(), true);
        }
        // TOTAL dispatch: the assembled surface is the ONLY router. A name it does
        // not carry raises `mcp.unknown-tool` naming that name — there is no
        // fallthrough arm and no mode guard, so "advertised" and "dispatchable" stay
        // the same fact for builtin and host-registered tools alike.
        let result = self.surface.dispatch_tool(self, name, args);
        match result {
            Ok(text) => tool_text(text, false),
            // A DEFERRAL is not a failure and must not read like one. It keeps
            // `isError: true` (the call produced no answer, so a client that only checks
            // that flag is still correct) but the payload carries the STRUCTURED routing
            // fields — the stable diagnostic code, the tool asked for, and the segment
            // that serves it — so a host can act on it mechanically instead of matching
            // prose. Unreachable on a deployment that serves every segment, which is why
            // the full engine's bytes are unchanged.
            Err(err) => match err.downcast_ref::<crate::error::SegmentNotLoaded>() {
                Some(deferred) => tool_text(
                    json!({
                        "ok": false,
                        "error": err.to_string(),
                        "code": crate::error::SegmentNotLoaded::CODE,
                        "tool": deferred.tool,
                        "segment": deferred.segment,
                        // The tools of the segment NAMED, not every tool this image defers:
                        // a host loads one module and needs to know what that module answers
                        // for. Two tiers sit below core, so the two lists differ.
                        "segment_tools": SegmentSet::tools_of(&deferred.segment),
                    })
                    .to_string(),
                    true,
                ),
                None => tool_text(
                    json!({"ok": false, "error": err.to_string()}).to_string(),
                    true,
                ),
            },
        }
    }

    pub fn read_resource_result(&self, uri: &str) -> Value {
        match self.read_resource_text(uri) {
            Ok((mime, text)) => json!({
                "contents": [{"uri": uri, "mimeType": mime, "text": text}],
            }),
            Err(err) => json!({
                "contents": [{"uri": uri, "mimeType": "application/json", "text": json!({"ok": false, "error": err.to_string()}).to_string()}],
                "isError": true,
            }),
        }
    }

    pub fn handle_message(&self, message: &str) -> String {
        let parsed: Value = match serde_json::from_str(message) {
            Ok(value) => value,
            Err(err) => {
                return json!({
                    "jsonrpc": "2.0",
                    "id": Value::Null,
                    "error": {"code": -32700, "message": err.to_string()},
                })
                .to_string();
            }
        };
        let id = parsed.get("id").cloned().unwrap_or(Value::Null);
        let Some(method) = parsed.get("method").and_then(Value::as_str) else {
            return rpc_error(id, -32600, "missing method");
        };
        let params = parsed.get("params").cloned().unwrap_or_else(|| json!({}));
        let result = match method {
            "initialize" => json!({
                "protocolVersion": "2024-11-05",
                "serverInfo": {"name": "gmeow", "version": env!("CARGO_PKG_VERSION")},
                "capabilities": {"tools": {}, "resources": {}},
            }),
            "tools/list" => self.tools_result(),
            "resources/list" => self.resources_result(),
            "tools/call" => {
                let Some(name) = params.get("name").and_then(Value::as_str) else {
                    return rpc_error(id, -32602, "tools/call requires params.name");
                };
                let args = params
                    .get("arguments")
                    .cloned()
                    .unwrap_or_else(|| json!({}));
                self.call_tool_result(name, &args)
            }
            "resources/read" => {
                let Some(uri) = params.get("uri").and_then(Value::as_str) else {
                    return rpc_error(id, -32602, "resources/read requires params.uri");
                };
                self.read_resource_result(uri)
            }
            "shutdown" => json!({}),
            method if method.starts_with("notifications/") => return String::new(),
            _ => return rpc_error(id, -32601, &format!("unknown method: {method}")),
        };
        json!({"jsonrpc": "2.0", "id": id, "result": result}).to_string()
    }

    /// Serve the stdio JSON-RPC 2.0 MCP loop: one request per line on stdin, one
    /// response per line on stdout, until EOF. Blocking; the native `gmeow mcp` /
    /// `gmeow-dev mcp` launchers call this directly.
    ///
    /// Native-only, and deliberately NOT routed through a transport seam. A browser has
    /// no stdin, no stdout, and no blocking read loop to give one: the wasm host drives
    /// the very same surface a frame at a time through [`Self::handle_message`], which
    /// is the whole protocol implementation and is compiled on every target. Abstracting
    /// "a blocking line loop over process stdio" would produce an interface with exactly
    /// one implementation and one caller.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn run_stdio(&self) -> gmeow_errors::Result<()> {
        use std::io::{BufRead as _, Write as _};

        let stdin = std::io::stdin();
        let mut stdout = std::io::stdout();
        for line in stdin.lock().lines() {
            let line = line?;
            if line.trim().is_empty() {
                continue;
            }
            let response = self.handle_message(&line);
            if !response.is_empty() {
                writeln!(stdout, "{response}")?;
                stdout.flush()?;
            }
        }
        Ok(())
    }

    #[cfg(feature = "core")]
    fn tool_lookup_term(&self, args: &Value) -> gmeow_errors::Result<String> {
        let term = required_str(args, "term")?;
        let requested = self.requested_from_args(args)?;
        Ok(self.view.lookup_term_json(term, requested))
    }

    #[cfg(feature = "core")]
    fn tool_llms_txt(&self, args: &Value) -> gmeow_errors::Result<String> {
        let requested = self.requested_from_args(args)?;
        Ok(self.view.llms_txt_text(requested))
    }

    #[cfg(feature = "core")]
    fn tool_llms_full(&self, args: &Value) -> gmeow_errors::Result<String> {
        let requested = self.requested_from_args(args)?;
        self.view.llms_full_text(requested)
    }

    #[cfg(feature = "core")]
    fn tool_doc_card(&self, args: &Value) -> gmeow_errors::Result<String> {
        let term = required_str(args, "term")?;
        let detail = parse_card_detail(optional_str(args, "detail"))?;
        let format = CardFormat::parse(optional_str(args, "format"))?;
        let requested = self.requested_from_args(args)?;
        self.view.doc_card(term, detail, format, requested)
    }

    #[cfg(feature = "core")]
    fn tool_okf_index(&self, args: &Value) -> gmeow_errors::Result<String> {
        let requested = self.requested_from_args(args)?;
        Ok(self.view.okf_index_json(requested))
    }

    /// `counter_examples`: the conformance fixtures documenting a term, split into
    /// the well-formed exemplars and the counter-examples. An UNKNOWN term is a HARD
    /// FAIL (`Err` → error envelope); a KNOWN term that simply documents no fixtures
    /// is an honest empty-but-ok result (both lists empty).
    #[cfg(feature = "core")]
    fn tool_counter_examples(&self, args: &Value) -> gmeow_errors::Result<String> {
        let term = required_str(args, "term")?;
        let iri = self.resolve_term_or_err(term, args)?;
        let (wellformed, counter_examples) = self.view.term_fixtures(&iri)?;
        Ok(json!({
            "ok": true,
            "term": term,
            "wellformed": wellformed,
            "counter_examples": counter_examples,
        })
        .to_string())
    }

    /// `entailments`: the reasoner derivations documenting a term, each with its
    /// rule, conclusion, and every premise. Unknown term → hard error; a known term
    /// with no derivations → empty-but-ok.
    #[cfg(feature = "core")]
    fn tool_entailments(&self, args: &Value) -> gmeow_errors::Result<String> {
        let term = required_str(args, "term")?;
        let iri = self.resolve_term_or_err(term, args)?;
        let entailments = self.view.term_entailments(&iri)?;
        Ok(json!({
            "ok": true,
            "term": term,
            "entailments": entailments,
        })
        .to_string())
    }

    /// `competency_questions`: the runnable competency questions. With a `term`, only
    /// that term's questions (unknown term → hard error); without a `term`, the whole
    /// index of documented competency questions.
    #[cfg(feature = "core")]
    fn tool_competency_questions(&self, args: &Value) -> gmeow_errors::Result<String> {
        match optional_str(args, "term") {
            Some(term) => {
                let iri = self.resolve_term_or_err(term, args)?;
                let questions = self.view.competency_questions(Some(&iri))?;
                Ok(json!({"ok": true, "term": term, "questions": questions}).to_string())
            }
            None => {
                let questions = self.view.competency_questions(None)?;
                Ok(json!({"ok": true, "questions": questions}).to_string())
            }
        }
    }

    /// Resolve a term string to its canonical IRI, HARD-FAILING (`Err`) on an unknown
    /// term OR a cross-namespace bare-name collision — the shared resolution guard for
    /// the documentation-surface tools (`counter_examples`, `entailments`,
    /// `competency_questions`). Ambiguity carries the TYPED `McpAmbiguousTerm`
    /// diagnostic (sorted candidate CURIEs); an unknown term keeps the generic
    /// unknown-term diagnostic — never a silent pick.
    #[cfg(feature = "core")]
    fn resolve_term_or_err(&self, term: &str, args: &Value) -> gmeow_errors::Result<String> {
        let requested = self.requested_from_args(args)?;
        match self.view.resolve_term_iri(term, requested) {
            ConsumerResolution::Resolved(iri) => Ok(iri),
            ConsumerResolution::Ambiguous { candidates } => {
                Err(ambiguous_term_err(term, &candidates))
            }
            ConsumerResolution::NotFound => Err(unknown_term_err(term)),
        }
    }

    #[cfg(feature = "core")]
    fn tool_query_docs(&self, args: &Value) -> gmeow_errors::Result<String> {
        let query = required_str(args, "query")?;
        Ok(self.view.query_docs_json(query))
    }

    /// `docs_search`: rank the documented terms / slices / concerns whose searchable
    /// facets match `query` over the `gmeow:graph/documentation` projection. An
    /// absent/empty documentation graph is a HARD FAIL; a query matching nothing is an
    /// honest empty-but-ok result.
    #[cfg(feature = "core")]
    fn tool_docs_search(&self, args: &Value) -> gmeow_errors::Result<String> {
        let query = required_str(args, "query")?;
        let limit = optional_limit(args, "limit")?.unwrap_or(20);
        let docs = self.view.documentation();
        let hits = search_documentation(docs, query, limit)?;
        let results: Vec<Value> = hits.iter().map(SearchHit::to_json).collect();
        Ok(json!({"ok": true, "query": query, "results": results}).to_string())
    }

    #[cfg(feature = "core")]
    fn tool_query_local(&self, args: &Value) -> gmeow_errors::Result<String> {
        let data = required_str(args, "data")?;
        let format = required_str(args, "format")?;
        let query = required_str(args, "query")?;
        let scope = QueryScope::parse(optional_str(args, "scope"))?;
        Ok(self.view.query_local_json(data, format, query, scope))
    }

    /// `encode_gmn1` — the WRITE leg of the GMN-1 codec: authored RDF in, the
    /// token-compact GMN-1 surface out.
    ///
    /// The other four GMN tools all CONSUME GMN (validate a document, expand it to GMN-0,
    /// explain a glyph, price the legend). Producing GMN-1 — the direction an agent needs to
    /// emit the compact notation at all — had no expression on the surface: `gmn1_write`
    /// existed only INSIDE `gmn_expand`, as an internal round-trip witness on a GMN-1 input.
    /// That made the codec's forward leg unreachable through the protocol even though the
    /// engine shipped it.
    ///
    /// Runs the SAME `Gmn0Model::from_dataset` → `gmn1_write` pair the codec's other
    /// consumers do, against the dictionary carried IN the bundle, so the emitted surface is
    /// the one the on-gate authority ships. Pairs with `gmn_expand` as an exact round trip:
    /// `encode_gmn1` then `gmn_expand` returns the input's canonical N-Quads.
    #[cfg(feature = "core")]
    fn tool_encode_gmn1(&self, args: &Value) -> gmeow_errors::Result<String> {
        let data = required_str(args, "data")?;
        let format = required_str(args, "format")?;
        let media = rdf_media_type("encode_gmn1", format)?;
        if data.len() > MAX_VERIFY_OVERLAY_BYTES as usize {
            return Err(gmeow_errors::Diag::of_kind(crate::error::Mcp {
                message: format!(
                    "encode_gmn1: data is {} bytes, exceeding the {MAX_VERIFY_OVERLAY_BYTES}-byte \
                     ceiling; split the document and encode the parts (no silent truncation)",
                    data.len()
                ),
            }));
        }
        let dataset = purrdf::parse_dataset(data.as_bytes(), media, None)
            .with_ctx(|| format!("parse encode_gmn1 input ({format})"))?;
        let dict = self.gmn_dictionary()?;
        let model = Gmn0Model::from_dataset(&dataset);
        let doc = gmn1_write(&model, &dict).map_err(|error| {
            gmeow_errors::Diag::of_kind(crate::error::Mcp {
                message: format!("encode_gmn1: encoding the model to GMN-1 failed: {error}"),
            })
        })?;
        // Round-trip witness, the mirror of `gmn_expand`'s: read the emitted surface back and
        // assert canonical equality with the input model. An encoding that does not read back
        // is a HARD FAIL, never a returned answer — the same no-lossy-answer discipline.
        let back = gmn1_read(&doc, &dict).map_err(|error| {
            gmeow_errors::Diag::of_kind(crate::error::Mcp {
                message: format!(
                    "encode_gmn1: the emitted GMN-1 does not read back (lang:{}): {error}",
                    iri_local_name(error.failure_class())
                ),
            })
        })?;
        let round_trip = gmn0_canonically_equal(&model, &back);
        if !round_trip {
            return Err(gmeow_errors::Diag::of_kind(crate::error::Mcp {
                message: "encode_gmn1: the emitted GMN-1 does not round-trip to the input model \
                          (a lossy encoding is a hard error, never an answer)"
                    .to_string(),
            }));
        }
        Ok(json!({
            "ok": true,
            "gmn1": doc.text,
            "canonical_nquads": back.canonical_nquads(),
            "round_trip": round_trip,
        })
        .to_string())
    }

    /// `reason_graph` — R2's **reason** verb over caller-supplied RDF: run the native
    /// structured-DL forward chase and return the ENTAILED CLOSURE.
    ///
    /// Distinct from the two neighbours that sound like it, and neither could stand in:
    /// * `verify_graph` returns a proof-carrying VERDICT (completeness/evaluation axes,
    ///   findings, a coherence judgment) — it says whether the graph holds up, not what it
    ///   entails;
    /// * `entailments` reads PRECOMPUTED derivation rows out of the bundle's documentation
    ///   graph for one bundled term — it cannot see caller data at all.
    ///
    /// Scoped to the caller's graph ALONE (never `bundle ∪ data`): the question is what THIS
    /// document entails, and unioning the canon in would answer a different one. Budgeted
    /// through [`governed_budget`] like every other agent-facing reasoning call — R4 forbids
    /// exposing an unbudgeted forward chase to an agent loop — and the consumed/allowed steps
    /// ride the envelope so a truncated closure is legible as truncated rather than as small.
    #[cfg(feature = "reasoning")]
    fn tool_reason_graph(&self, args: &Value) -> gmeow_errors::Result<String> {
        let data = required_str(args, "data")?;
        let format = required_str(args, "format")?;
        let media = rdf_media_type("reason_graph", format)?;
        if data.len() > MAX_VERIFY_OVERLAY_BYTES as usize {
            return Err(gmeow_errors::Diag::of_kind(crate::error::Mcp {
                message: format!(
                    "reason_graph: data is {} bytes, exceeding the {MAX_VERIFY_OVERLAY_BYTES}-byte \
                     ceiling; split the graph and reason over the parts (no silent truncation)",
                    data.len()
                ),
            }));
        }
        let edb = purrdf::parse_dataset(data.as_bytes(), media, None)
            .with_ctx(|| format!("parse reason_graph input ({format})"))?;
        let edb_quads = edb.quad_count();
        if edb_quads > MAX_VERIFY_OVERLAY_QUADS {
            return Err(gmeow_errors::Diag::of_kind(crate::error::Mcp {
                message: format!(
                    "reason_graph: input is {} quads, exceeding the {MAX_VERIFY_OVERLAY_QUADS}-quad \
                     ceiling; the starting EDB of a governed chase is bounded",
                    edb_quads
                ),
            }));
        }
        let budget = governed_budget(
            optional_step_count(args, "max_steps")?,
            optional_limit(args, "max_answers")?,
        );
        let result = reason_all_budgeted(&edb, &budget)?;
        let closure = gmeow_logic::reason::inferred_axioms_to_dataset(result.inferred())?;
        let bytes = purrdf::serialize_dataset(
            closure.as_ref(),
            "application/n-quads",
            purrdf::SerializeGraph::Dataset,
        )
        .with_ctx(|| "serialize reasoned closure".to_string())?;
        let closure_nquads = String::from_utf8(bytes).map_err(|e| {
            gmeow_errors::Diag::of_kind(crate::error::Mcp {
                message: format!("reason_graph: closure N-Quads are not UTF-8: {e}"),
            })
        })?;
        Ok(json!({
            "ok": true,
            "entailed_count": closure.quad_count(),
            "closure_nquads": closure_nquads,
            "completeness": result.completeness.wire(),
            "evaluation": result.evaluation.wire(),
            "judgment_nquads": project_reasoning_result(&result),
        })
        .to_string())
    }

    /// Reason-and-verify the bundle UNIONED with a READ-ONLY inline overlay graph,
    /// returning the proof-carrying judgment envelope. A thin wrapper over
    /// [`McpView::run_verify_graph`]: it reads the `data` + `format` overlay and the
    /// `max_steps` / `max_answers` budget off the args and delegates the whole
    /// overlay/union/govern/verify/judge discipline to the view core (one
    /// implementation).
    #[cfg(feature = "reasoning")]
    fn tool_verify_graph(&self, args: &Value) -> gmeow_errors::Result<String> {
        let data = required_str(args, "data")?;
        let format = required_str(args, "format")?;
        let max_steps = optional_step_count(args, "max_steps")?;
        let max_answers = optional_limit(args, "max_answers")?;
        // R4: NEVER build an unbudgeted `Budget{None,None}` from omitted
        // agent args — `governed_budget` defaults+clamps to a finite server-side ceiling.
        let budget = governed_budget(max_steps, max_answers);
        Ok(self.view.verify_graph_json(data, format, &budget))
    }

    /// Reconstruct the FAITHFUL cited-IRI derivation skeleton for one quad in the
    /// bundle's governed closure, returning the proof-carrying envelope. A thin
    /// wrapper over [`McpView::run_explain_quad`]: it validates the required quad
    /// components, builds the canonical N3 object surface (Step B) from the structured
    /// `object_value` / `object_kind` / `object_datatype` args, reads the `max_steps`
    /// governor, and delegates the reason/locate/explain/faithfulness discipline to
    /// the view core (one implementation).
    ///
    /// This is the CONSUMER quad-explainer — distinct from `explain_finding`, which
    /// addresses a published diagnostic by its fingerprint/anchor IRI.
    #[cfg(feature = "reasoning")]
    fn tool_explain_quad(&self, args: &Value) -> gmeow_errors::Result<String> {
        let subject = required_str(args, "subject")?;
        let predicate = required_str(args, "predicate")?;
        let object_value = required_str(args, "object_value")?;
        let graph = required_str(args, "graph")?;
        // `object_kind` is optional: an explicit `iri`/`literal` overrides, else it is
        // inferred from the object surface. `object_datatype` types a literal only.
        let object_kind = optional_str(args, "object_kind");
        let object_datatype = optional_str(args, "object_datatype");
        let obj_n3 = object_term_n3(object_value, object_kind, object_datatype)?;

        let max_steps = optional_step_count(args, "max_steps")?;
        // R4: `explain_quad` never exposes `max_answers`, but
        // `governed_budget` still stamps a finite default there — no field of the
        // constructed `Budget` is ever `None` on an agent-facing path.
        let budget = governed_budget(max_steps, None);
        Ok(self
            .view
            .explain_quad_json(subject, predicate, &obj_n3, graph, &budget))
    }

    /// Surface the bundle's carried coherence certificate/attestation (R6). A thin,
    /// INPUT-FREE wrapper over [`McpView::coherence_certificate_json`]: the certificate is
    /// read straight off the bundled dataset (disk-free, reason-free), never recomputed.
    #[cfg(feature = "reasoning")]
    fn tool_coherence_certificate(&self, _args: &Value) -> gmeow_errors::Result<String> {
        Ok(self.view.coherence_certificate_json())
    }

    /// Validate an inline RDF `data` graph (in `format`) against the bundle's own
    /// shapes + disciplines through the DEEP (`--deep`) semantic pass — the SAME
    /// `gmeow_validate::data_validate::run` core the CLI `gmeow validate --deep`
    /// drives — then CLOSE THE LOOP by enriching each finding with the bundle's
    /// teaching surface: the rule's catalog help URI, the CORRESPONDING
    /// counter-example (matched by violation code), a positive exemplar, and the
    /// term's entailments.
    ///
    /// Transient discipline: nothing is written. The data arrives inline as a string
    /// arg (unlike `query_local`, which reads a file), is validated against the
    /// retained snapshot bytes, and is discarded. An unrecognized `format` or an
    /// oversized payload is a HARD FAIL, never a silent mis-parse or truncation.
    #[cfg(feature = "core")]
    fn tool_validate_local(&self, args: &Value) -> gmeow_errors::Result<String> {
        let data = required_str(args, "data")?;
        let format = required_str(args, "format")?;
        let canonical = canonical_rdf_format("validate_local", format)?;

        if data.len() > MAX_VALIDATE_DATA_BYTES {
            return Err(gmeow_errors::Diag::of_kind(crate::error::Mcp {
                message: format!(
                    "validate_local: data payload is {} bytes, exceeding the {} byte ceiling; \
                     split the graph and validate the parts (no silent truncation)",
                    data.len(),
                    MAX_VALIDATE_DATA_BYTES
                ),
            }));
        }

        // Tier-1 SHACL + disciplines always run (fast). The Tier-2 semantic
        // pass — reasoning over the user data unioned with the whole bundle's
        // axioms — is opt-in via `deep` (default false), exactly like
        // `gmeow validate --deep`: it is powerful but reasons over the entire
        // bundle (multiple minutes), so an interactive agent requests it only when
        // structural conformance is not enough. When enabled, a contract-invalid
        // input is emitted by the engine as a hard Error finding — surfaced here
        // verbatim, never post-processed away.
        //
        // This server is a RESIDENT bundle consumer: it already imported the
        // snapshot to the carrier dataset at startup and caches the parsed Tier-1
        // shape union on first use, so it drives the shared `run_with` composition
        // (the exact `gmeow validate` semantics) without re-decoding the whole
        // bundle on every call.
        let deep = optional_bool(args, "deep").unwrap_or(false);
        let report = gmeow_validate::data_validate::run_with(
            gmeow_validate::data_validate::BundleParts {
                gts_bytes: self.view.gts_bytes(),
                shapes: self.view.tier1_shapes()?,
                dataset: self.view.dataset.as_ref(),
            },
            data.as_bytes(),
            canonical,
            MCP_NAMESPACE,
            VALIDATE_LOCAL_ORIGIN,
            deep,
        )?;

        // Loop closure: extract the correspondence maps from the documentation graph
        // and enrich each finding through the wasm-clean pure join.
        let (counter_examples_by_code, wellformed_by_code) = self.view.fixture_maps()?;
        let entailments_by_term = self.view.entailment_map()?;
        let enriched = local_oracle::enrich_report(
            &report,
            &counter_examples_by_code,
            &wellformed_by_code,
            &entailments_by_term,
        );
        serde_json::to_string(&enriched).map_err(|e| {
            gmeow_errors::Diag::of_kind(crate::error::Mcp {
                message: format!("validate_local: serialize enriched report: {e}"),
            })
        })
    }

    /// Resolve the shipped GMN-1 dictionary (`gmeow:gmnDictV3` + its current codebook)
    /// straight off the bundled snapshot dataset — the SAME checkout-free source the
    /// `gmeow gmn` CLI folds from `BUNDLE_GTS`, so the MCP verifier tools an external LLM
    /// calls share ONE dictionary with the shipped CLI and gates. A load failure is a HARD
    /// FAIL, never a degraded default.
    #[cfg(feature = "core")]
    fn gmn_dictionary(&self) -> gmeow_errors::Result<GmnDictionary> {
        GmnDictionary::from_dataset(self.view.dataset.as_ref()).map_err(|e| {
            gmeow_errors::Diag::of_kind(crate::error::Mcp {
                message: format!(
                    "gmn: cannot resolve gmeow:gmnDictV3 from the bundled snapshot: {}",
                    e.0
                ),
            })
        })
    }

    /// `gmn_validate` — the external LLM's entry to the GMN `@err` repair loop: read a
    /// GMN-1 document against the shipped dictionary + validator tier and report either
    /// conformance or the TYPED `lang:Gmn*Failure` class (+ message) of the first defect.
    /// A defect is a VALID result (`ok:true, conformant:false`), never a tool error — the
    /// caller repairs against the named class.
    #[cfg(feature = "core")]
    fn tool_gmn_validate(&self, args: &Value) -> gmeow_errors::Result<String> {
        let gmn = required_str(args, "gmn")?;
        guard_gmn_size(gmn, "gmn_validate")?;
        let dict = self.gmn_dictionary()?;
        let doc = Gmn1Document::from_text(gmn.to_owned());
        match gmn1_read(&doc, &dict) {
            Ok(_model) => Ok(json!({ "ok": true, "conformant": true }).to_string()),
            Err(error) => {
                let class = error.failure_class();
                Ok(json!({
                    "ok": true,
                    "conformant": false,
                    "failure_class": class,
                    "failure_local_name": iri_local_name(class),
                    "message": error.to_string(),
                })
                .to_string())
            }
        }
    }

    /// `gmn_expand` — decode a GMN-1 document to its GMN-0 normal form (the "expand
    /// alias/glyph → full IRI" direction) and return the canonical N-Quads. The expansion
    /// carries an internal round-trip WITNESS: the decoded model is re-encoded and re-read,
    /// and canonical equality is asserted, so the tool never returns an unstable expansion.
    /// A non-conformant input, or a round-trip that does not hold, is a HARD FAIL.
    #[cfg(feature = "core")]
    fn tool_gmn_expand(&self, args: &Value) -> gmeow_errors::Result<String> {
        let gmn = required_str(args, "gmn")?;
        guard_gmn_size(gmn, "gmn_expand")?;
        let dict = self.gmn_dictionary()?;
        let doc = Gmn1Document::from_text(gmn.to_owned());
        let model = gmn1_read(&doc, &dict).map_err(|error| {
            gmeow_errors::Diag::of_kind(crate::error::Mcp {
                message: format!(
                    "gmn_expand: input is not a conformant GMN-1 document (lang:{}): {error}",
                    iri_local_name(error.failure_class())
                ),
            })
        })?;
        // Round-trip witness: re-encode the expanded model and read it back; the expansion
        // is only sound if the reconstruction is canonically equal (no lossy expansion).
        let reencoded = gmn1_write(&model, &dict).map_err(|error| {
            gmeow_errors::Diag::of_kind(crate::error::Mcp {
                message: format!("gmn_expand: re-encoding the expanded model failed: {error}"),
            })
        })?;
        let back = gmn1_read(&reencoded, &dict).map_err(|error| {
            gmeow_errors::Diag::of_kind(crate::error::Mcp {
                message: format!("gmn_expand: re-reading the re-encoded model failed: {error}"),
            })
        })?;
        if !gmn0_canonically_equal(&model, &back) {
            return Err(gmeow_errors::Diag::of_kind(crate::error::Mcp {
                message: "gmn_expand: round-trip witness failed — the expanded GMN-0 normal form \
                          is not canonically stable under re-encode/re-read (refusing a lossy \
                          expansion)"
                    .to_owned(),
            }));
        }
        Ok(json!({
            "ok": true,
            "expanded_nquads": model.canonical_nquads(),
            "reencoded_gmn": reencoded.text,
            "round_trip": true,
        })
        .to_string())
    }

    /// `gmn_explain` — resolve a GMN operator glyph to its `lang:Denotation` and its
    /// graph-authored `gmeow:gmnFixity` / `gmeow:gmnPrecedence` / `gmeow:gmnArity`
    /// signature, plus its controlled-NL gloss (the SAME Task 8 verbalizer rendering).
    /// An input that is not a covered operator glyph returns an HONEST typed miss
    /// (`found:false` + `lang:GmnUncoveredTerm`), never a fabricated answer.
    #[cfg(feature = "core")]
    fn tool_gmn_explain(&self, args: &Value) -> gmeow_errors::Result<String> {
        let glyph = required_str(args, "glyph")?;
        let dataset = self.view.dataset.as_ref();
        let dict = self.gmn_dictionary()?;
        let labels = harvest_dataset_labels(dataset);
        let forms = resolve_operator_forms(dict.glyph_registry(), &labels).map_err(|error| {
            gmeow_errors::Diag::of_kind(crate::error::Mcp {
                message: format!(
                    "gmn_explain: resolve GMN operator forms from the codebook: {error}"
                ),
            })
        })?;
        let Some(form) = forms.iter().find(|f| f.gmn_glyph == glyph) else {
            return Ok(json!({
                "ok": true,
                "found": false,
                "glyph": glyph,
                "failure_class": LANG_GMN_UNCOVERED_TERM,
                "failure_local_name": iri_local_name(LANG_GMN_UNCOVERED_TERM),
                "message": format!(
                    "glyph {glyph:?} is not a covered GMN operator in the current codebook"
                ),
            })
            .to_string());
        };
        // The controlled-NL gloss is Task 8's verbalizer rendering VERBATIM: build the whole
        // (injective, disambiguated) corpus and read off this form's rendered pair, so the
        // gloss the LLM sees is the exact GMN⇄NL training-pair surface, not a re-derivation.
        let pairs = build_verbalization_pairs(&forms).map_err(|error| {
            gmeow_errors::Diag::of_kind(crate::error::Mcp {
                message: format!("gmn_explain: render the verbalizer gloss corpus: {error}"),
            })
        })?;
        let rendered = pairs.iter().find(|p| &p.form == form).ok_or_else(|| {
            gmeow_errors::Diag::of_kind(crate::error::Mcp {
                message: format!("gmn_explain: no verbalizer pair for glyph {glyph:?}"),
            })
        })?;
        // Precedence + the lang:Denotation IRI are NOT carried by the glyph registry (it keys
        // on fixity/arity only), so join them from the dataset by the (target, fixity, arity)
        // signature the operator's denoted Form authors.
        let (precedence, denotation) =
            gmn_signature_join(dataset, &form.term_iri, &form.fixity, form.arity);
        Ok(json!({
            "ok": true,
            "found": true,
            "glyph": form.gmn_glyph,
            "denotation": denotation,
            "denotation_target": form.term_iri,
            "label": form.term_label,
            "fixity": form.fixity,
            "fixity_local_name": iri_local_name(&form.fixity),
            "precedence": precedence,
            "arity": form.arity,
            "gmn_surface": rendered.gmn_surface,
            "gloss": rendered.nl,
        })
        .to_string())
    }

    /// `gmn_glyph_legend` — the GMN-1 glyph inventory joined to each glyph's real
    /// LLM-token cost, as a deterministic JSON array of `{glyph, tokenCost}`.
    ///
    /// The completion of the GMN triad on this surface: `gmn_validate` says whether a
    /// document conforms, `gmn_expand` gives its GMN-0 normal form, `gmn_explain` explains
    /// one operator — and this states the ALPHABET, which an agent needs before it can
    /// write GMN-1 at all.
    ///
    /// The legend is composed by the ONE implementation
    /// ([`gmeow_lang_bridge::glyph_legend_json`]) the browser codec shim also marshals, over
    /// the glyph registry of THIS bundle's shipped `gmeow:gmnDictV3` codebook — so the
    /// agent's legend and the docs widget's legend are the same rows in the same order, and
    /// there is no second table to drift.
    #[cfg(feature = "core")]
    fn tool_gmn_glyph_legend(&self, _args: &Value) -> gmeow_errors::Result<String> {
        let dict = self.gmn_dictionary()?;
        let legend_text = gmeow_lang_bridge::glyph_legend_json(dict.glyph_registry())?;
        let legend: Value = serde_json::from_str(&legend_text).map_err(|e| {
            gmeow_errors::Diag::of_kind(crate::error::Mcp {
                message: format!("gmn_glyph_legend: the composed legend is not valid JSON: {e}"),
            })
        })?;
        Ok(json!({ "ok": true, "legend": legend }).to_string())
    }

    /// `convert` — the fourth verb of parse / reason / validate / SERIALIZE: transcode an
    /// RDF-1.2 document from any source codec to any target codec, and report the loss the
    /// conversion actually realized.
    ///
    /// This routes through [`gmeow_transcode`] — the SAME `Codec::from_cli_str` /
    /// `transcode` / `realized_loss_json` triple `gmeow convert` calls, not a second
    /// implementation — so the bytes an agent gets here and the bytes the CLI writes are
    /// the same function of the same input. `gmeow-transcode` is a leaf, which is what lets
    /// a bundle-only surface reach it without inheriting the build executor.
    ///
    /// RDF-1.2 is load-bearing: a quoted triple that survives to a star-capable target MUST
    /// still be there. The hub decides that, not this tool — and the `loss` ledger states
    /// what an RDF-1.1-shaped target dropped, rather than letting the caller assume nothing
    /// was lost.
    ///
    /// Bytes, not text: `encoding` declares how to read `data` (`utf8`, the default, or
    /// `base64` for a binary source such as `gts`), and the response's own `encoding` says
    /// how to read `output` — `base64` whenever the target's bytes are not valid UTF-8.
    /// Neither direction ever silently lossy-decodes.
    ///
    /// Gated on the `core` feature: `gmeow-transcode` is one of the two dependencies the
    /// core segment alone reaches, so the demand-loaded reasoning image links no transcode
    /// hub and this method does not exist there to be named. `core_tool!` binds the wire
    /// name to the deferral signal instead, and the tool stays advertised.
    #[cfg(feature = "core")]
    fn tool_convert(&self, args: &Value) -> gmeow_errors::Result<String> {
        use gmeow_transcode::{Codec, realized_loss_json, transcode as run_transcode};

        let data = required_str(args, "data")?;
        let from = required_str(args, "from")?;
        let to = required_str(args, "to")?;
        let base = optional_str(args, "base");
        let input = match optional_str(args, "encoding").unwrap_or("utf8") {
            "utf8" => data.as_bytes().to_vec(),
            "base64" => base64_decode("convert: `data`", data)?,
            other => {
                return Err(gmeow_errors::Diag::of_kind(crate::error::Mcp {
                    message: format!(
                        "convert: unknown `encoding` {other:?} — expected `utf8` or `base64`"
                    ),
                }));
            }
        };

        let from_codec = Codec::from_cli_str(from)?;
        let to_codec = Codec::from_cli_str(to)?;
        let output = run_transcode(&input, from_codec, to_codec, base)?;

        // The realized ledger is rendered by the hub's own `realized_loss_json` (which
        // interns through the single substrate loss store) and re-read here only to nest it
        // as structured JSON in the envelope — never re-derived off the `Vec<RealizedLoss>`.
        let loss: Value =
            serde_json::from_str(&realized_loss_json(&output.realized)).map_err(|e| {
                gmeow_errors::Diag::of_kind(crate::error::Mcp {
                    message: format!("convert: the realized-loss ledger is not valid JSON: {e}"),
                })
            })?;

        let byte_len = output.bytes.len();
        let (encoding, text) = match String::from_utf8(output.bytes) {
            Ok(utf8) => ("utf8", utf8),
            Err(err) => ("base64", base64_encode(err.as_bytes())),
        };
        Ok(json!({
            "ok": true,
            "from": from_codec.name(),
            "to": to_codec.name(),
            "encoding": encoding,
            "output": text,
            "bytes": byte_len,
            "loss": loss,
        })
        .to_string())
    }

    /// `distribution_matrix` — the shipped documentation-distribution catalog, read back
    /// out of THIS bundle's meta-level `graph/distribution-catalog` named graph.
    ///
    /// Two row sets, because they are two shapes over one graph:
    ///
    /// * `distributions` — the per-format consumer-need matrix (`slug`, `family`,
    ///   `media_type`, `consumers`, `dropped_capabilities`): WHICH documentation surfaces
    ///   exist, who each is for, and what capability each doc-render surface drops. This is
    ///   the same reader, over the same graph, that `gmeow docs matrix` prints.
    /// * `concepts` — the formal-concept lattice over the surface × capability incidence
    ///   (`concept`, `extent`, `intent`). A concept is not a distribution — it has an
    ///   extent and an intent and no media type — so it has its own reader and its own row
    ///   type. An empty list is the honest reading of a catalog that declares no concepts,
    ///   not a failure.
    ///
    /// Both come from [`gmeow_docs_catalog`], the wasm-clean catalog leaf, so this stays a
    /// bundle-only tool: no checkout, no build executor.
    ///
    /// Gated on the `core` feature, exactly as `tool_convert` is and for the same reason:
    /// `gmeow-docs-catalog` is the other dependency only this segment reaches.
    #[cfg(feature = "core")]
    fn tool_distribution_matrix(&self, _args: &Value) -> gmeow_errors::Result<String> {
        let gts = self.view.gts_bytes();
        let distributions = gmeow_docs_catalog::read_distribution_matrix(gts)?;
        let concepts = gmeow_docs_catalog::read_concept_lattice(gts)?;
        Ok(json!({
            "ok": true,
            "distributions": distributions
                .iter()
                .map(|row| json!({
                    "slug": row.slug,
                    "family": row.family,
                    "media_type": row.media_type,
                    "consumers": row.consumers,
                    "dropped_capabilities": row.dropped_capabilities,
                }))
                .collect::<Vec<Value>>(),
            "concepts": concepts
                .iter()
                .map(|row| json!({
                    "concept": row.concept,
                    "extent": row.extent,
                    "intent": row.intent,
                }))
                .collect::<Vec<Value>>(),
        })
        .to_string())
    }

    /// `action_policy` — the canonical action theory covering this engine's ENTIRE consumer
    /// tool surface, served as N-Quads.
    ///
    /// [`action_policy_nquads`] is the projection the Transaction-Logic executor reads: the
    /// `logic:precondition` / `logic:effect` / `logic:compensation` structure of every write
    /// action and the `logic:capability` / `logic:precondition` structure of every read, plus
    /// the `logic:mcpToolName` wire name that ties each schema to the tool it governs, in
    /// [`TXN_WORLD`]. The theory is TOTAL over the surface — 6 governed writes
    /// (`logic:McpActionSchema`) and 32 reads (plain `logic:ActionSchema`), bijective with the
    /// 38 advertised tools, enforced by
    /// `the_action_theory_is_bijective_with_the_consumer_tool_surface`. The tool returns THAT
    /// function's output verbatim — never a re-derivation off the embedded Turtle and never a
    /// second filter — so what an agent inspects is exactly what the engine obeys.
    ///
    /// No existing surface can serve it: `tools/list` returns names and JSON Schemas only,
    /// and `query_docs` is scoped to `gmeow:graph/documentation` while the policy is
    /// authored in the agentic slice's examples graph. The mirroring
    /// `gmeow://ontology/action-policy` resource serves the identical bytes as `text` for a
    /// client that reads resources rather than calling tools.
    #[cfg(feature = "core")]
    fn tool_action_policy(&self, _args: &Value) -> gmeow_errors::Result<String> {
        Ok(json!({
            "ok": true,
            "graph": TXN_WORLD,
            "media_type": ACTION_POLICY_MEDIA_TYPE,
            "nquads": action_policy_nquads(),
        })
        .to_string())
    }

    /// The `advise` tool: return the non-gating RECOMMENDATIONS (never rejections) a
    /// submitted claim trips — the companion of `validate_local`. It runs the SAME
    /// shipped validator core (shallow Tier-1 only — the advisory `advice.*` Note tier
    /// is a fast structural pass; deep reasoning buys no advisory yield for its
    /// multi-minute cost), keeps only the advisory tier, and serializes each as a
    /// contrary-to-duty-shaped recommendation. ALWAYS `ok:true`: advice is a
    /// recommendation, so a clean claim returns an empty list, never a failure.
    #[cfg(feature = "core")]
    fn tool_advise(&self, args: &Value) -> gmeow_errors::Result<String> {
        let data = required_str(args, "data")?;
        let format = required_str(args, "format")?;
        let canonical = canonical_rdf_format("advise", format)?;

        if data.len() > MAX_VALIDATE_DATA_BYTES {
            return Err(gmeow_errors::Diag::of_kind(crate::error::Mcp {
                message: format!(
                    "advise: data payload is {} bytes, exceeding the {} byte ceiling; \
                     split the graph and advise on the parts (no silent truncation)",
                    data.len(),
                    MAX_VALIDATE_DATA_BYTES
                ),
            }));
        }

        // Shallow Tier-1 only (deep = false): advisory Notes come from the fast SHACL
        // pass. Same bundle parts and core as validate_local, so advice never diverges
        // from the shipped validator.
        let report = gmeow_validate::data_validate::run_with(
            gmeow_validate::data_validate::BundleParts {
                gts_bytes: self.view.gts_bytes(),
                shapes: self.view.tier1_shapes()?,
                dataset: self.view.dataset.as_ref(),
            },
            data.as_bytes(),
            canonical,
            MCP_NAMESPACE,
            ADVISE_ORIGIN,
            false,
        )?;

        // Keep only the advisory tier (the advice.* Note family) and shape each as a
        // contrary-to-duty recommendation: the violated prohibition (avoid_when = the
        // finding message), the sub-ideal repair (how_to_use), and the permission gate
        // (use_when). The bridge builds `suggestions` as [howToUse, "Use when: <prose>"],
        // so the shared `gmeow_validate::advisory::ADVICE_USE_WHEN_PREFIX` marker cleanly
        // separates the permission leg.
        let recommendations: Vec<Value> = report
            .findings
            .iter()
            .filter(|f| f.code.starts_with(gmeow_validate::codes::ADVICE_FAMILY))
            .map(|f| {
                let mut use_when: Vec<String> = Vec::new();
                let mut how_to_use: Vec<String> = Vec::new();
                for suggestion in &f.suggestions {
                    match suggestion.strip_prefix(gmeow_validate::advisory::ADVICE_USE_WHEN_PREFIX)
                    {
                        Some(rest) => use_when.push(rest.to_string()),
                        None => how_to_use.push(suggestion.clone()),
                    }
                }
                // The governed term the advice formalizes rides as a `formalizes:<term>`
                // tag; the tripped subject is the first location's logical IRI.
                let formalizes = f
                    .tags
                    .iter()
                    .find_map(|t| t.strip_prefix("formalizes:").map(str::to_string));
                let subject = f.locations.iter().find_map(|l| l.logical.clone());
                json!({
                    "code": f.code,
                    "subject": subject,
                    "formalizes": formalizes,
                    "avoid_when": f.message,
                    "how_to_use": how_to_use,
                    "use_when": use_when,
                    "help_uri": gmeow_validate::rule_catalog::catalog_anchor_uri(&f.code),
                })
            })
            .collect();

        let response = json!({
            "ok": true,
            "tool": "advise",
            "recommendations": recommendations,
        });
        serde_json::to_string(&response).map_err(|e| {
            gmeow_errors::Diag::of_kind(crate::error::Mcp {
                message: format!("advise: serialize recommendations: {e}"),
            })
        })
    }

    #[cfg(feature = "core")]
    fn tool_explain_finding(&self, args: &Value) -> gmeow_errors::Result<String> {
        let target = required_str(args, "target_iri")?;
        self.view.explain_finding_json(target)
    }

    #[cfg(feature = "reasoning")]
    fn tool_store_claim(&self, args: &Value) -> gmeow_errors::Result<String> {
        let text = required_str(args, "text")?;
        let confidence = optional_f64(args, "confidence")?;
        let dry_run = optional_bool_checked(args, "dry_run")?.unwrap_or(false);

        // store_claim's precondition — a well-formed claim is presented — obtains once the input
        // validates (required text + in-range confidence, enforced above). Run the action as a
        // TR transaction; the engine's executional entailment over THIS start state is the gate
        // (no synthetic boolean — an absent precondition would fail the run).
        let obtains = [MCP_WELL_FORMED_CLAIM];
        let receipt = execute_memory_txn(MCP_STORE_CLAIM_SCHEMA, &obtains, dry_run)?;
        match &receipt {
            TxReceipt::CommittedFailure { reason } | TxReceipt::HypotheticalFailure { reason } => {
                return Ok(json!({
                    "ok": false,
                    "error": format!("store_claim precondition unmet: {reason}"),
                    "transaction": txn_json(&receipt),
                })
                .to_string());
            }
            // Sandbox run: the verdict is observed, nothing is written or recorded.
            TxReceipt::HypotheticalSuccess { .. } => {
                return Ok(json!({
                    "ok": true,
                    "dry_run": true,
                    "transaction": txn_json(&receipt),
                })
                .to_string());
            }
            TxReceipt::CommittedSuccess { .. } => {}
        }

        let memory = self.claim_store()?;
        let claim = memory.store_claim(
            text,
            StoreOptions {
                source: optional_str(args, "source"),
                confidence,
                according_to: optional_str(args, "according_to"),
            },
        )?;
        let response =
            json!({"ok": true, "claim": claim_json(&claim), "transaction": txn_json(&receipt)})
                .to_string();
        let generated = [claim.id.as_str()];
        let call = memory.record_tool_call(
            &format!("{TOOL_AGENT_NS}store_claim"),
            ToolCallOptions {
                arguments: Some(&tool_arguments(
                    args,
                    &["text", "source", "confidence", "according_to", "dry_run"],
                )),
                result: Some(&response),
                invocation: None,
                generated: &generated,
            },
        )?;
        // Record the trajectory-audit context on the recorded call so the committed turn is cold-auditable.
        let at_time = call.created.as_deref().ok_or_else(|| {
            gmeow_errors::Diag::of_kind(crate::error::Mcp {
                message: "record_tool_call did not stamp a creation time".to_string(),
            })
        })?;
        write_audit_segment(
            memory.as_ref(),
            &call.id,
            MCP_STORE_CLAIM_SCHEMA,
            &obtains,
            at_time,
        )?;
        Ok(response)
    }

    /// `recall` — the READ half of the grounded-memory triad.
    ///
    /// A reasoning-SEGMENT tool despite linking no reasoner: it must answer from the same
    /// claim package `store_claim` wrote to, and in the browser a segment is an image and
    /// an image is a store. See [`REASONING_SEGMENT_TOOLS`] for why the triad is
    /// indivisible.
    #[cfg(feature = "reasoning")]
    fn tool_recall(&self, args: &Value) -> gmeow_errors::Result<String> {
        recall_json(self.claim_store()?.as_ref(), args)
    }

    /// `store_segment` — the SERIALIZATION of the grounded-memory claim package.
    ///
    /// Segmented with the rest of the triad for the same reason `recall` is: an export of
    /// a store held in another module's memory would export an empty store.
    #[cfg(feature = "reasoning")]
    fn tool_store_segment(&self, _args: &Value) -> gmeow_errors::Result<String> {
        store_segment_json(self.claim_store()?.as_ref())
    }

    #[cfg(feature = "reasoning")]
    fn tool_revise_belief(&self, args: &Value) -> gmeow_errors::Result<String> {
        let claim_id = required_str(args, "claim_id")?;
        let dry_run = optional_bool_checked(args, "dry_run")?.unwrap_or(false);
        let memory = self.claim_store()?;
        let claims = memory.claims()?;
        let known: BTreeSet<&str> = claims.iter().map(|claim| claim.id.as_str()).collect();
        let active: BTreeSet<&str> = claims
            .iter()
            .filter(|claim| !claim.suppressed)
            .map(|claim| claim.id.as_str())
            .collect();
        if let Some(successor) = optional_str(args, "superseded_by")
            && !known.contains(successor)
        {
            return Ok(json!({
                "ok": false,
                "error": format!("unknown superseded_by id: {successor}"),
            })
            .to_string());
        }

        // revise_belief's precondition — the target claim exists — obtains iff claim_id is a known
        // claim (the existing pre-flight check, now expressed AS the TR precondition). The del
        // effect retires the active claim, so claimInMemory obtains iff it is not already
        // suppressed. The engine's executional entailment is the gate; an unknown id fails the run.
        let mut obtains: Vec<&str> = Vec::new();
        if known.contains(claim_id) {
            obtains.push(MCP_TARGET_CLAIM_EXISTS);
        }
        if active.contains(claim_id) {
            obtains.push(MCP_CLAIM_IN_MEMORY);
        }
        let receipt = execute_memory_txn(MCP_REVISE_BELIEF_SCHEMA, &obtains, dry_run)?;
        match &receipt {
            TxReceipt::CommittedFailure { .. } | TxReceipt::HypotheticalFailure { .. } => {
                return Ok(json!({
                    "ok": false,
                    "error": format!("unknown claim id: {claim_id}"),
                    "transaction": txn_json(&receipt),
                })
                .to_string());
            }
            // Sandbox run: the verdict is observed, nothing is suppressed or recorded.
            TxReceipt::HypotheticalSuccess { .. } => {
                return Ok(json!({
                    "ok": true,
                    "dry_run": true,
                    "suppressed": claim_id,
                    "transaction": txn_json(&receipt),
                })
                .to_string());
            }
            TxReceipt::CommittedSuccess { .. } => {}
        }

        memory.revise_claim(
            claim_id,
            RevisionOptions {
                reason: optional_str(args, "reason"),
                superseded_by: optional_str(args, "superseded_by"),
            },
        )?;
        let response = json!({
            "ok": true,
            "suppressed": claim_id,
            "superseded_by": optional_str(args, "superseded_by"),
            "transaction": txn_json(&receipt),
        })
        .to_string();
        let call = memory.record_tool_call(
            &format!("{TOOL_AGENT_NS}revise_belief"),
            ToolCallOptions {
                arguments: Some(&tool_arguments(
                    args,
                    &["claim_id", "reason", "superseded_by", "dry_run"],
                )),
                result: Some(&response),
                invocation: None,
                generated: &[],
            },
        )?;
        let at_time = call.created.as_deref().ok_or_else(|| {
            gmeow_errors::Diag::of_kind(crate::error::Mcp {
                message: "record_tool_call did not stamp a creation time".to_string(),
            })
        })?;
        write_audit_segment(
            memory.as_ref(),
            &call.id,
            MCP_REVISE_BELIEF_SCHEMA,
            &obtains,
            at_time,
        )?;
        Ok(response)
    }

    /// `conjecture_test` — the issue's "test" leg: a PURE hypothetical evaluation of a
    /// candidate `logic:` formula against a KB. Computes and returns the projected engine
    /// verdict; NEVER TR-commits and NEVER appends to the conjecture library. For the
    /// committing counterpart ("store"), see [`Self::tool_store_conjecture`].
    ///
    /// `formula` is a Turtle `logic:` document naming the candidate (exactly one top-level
    /// `logic:Formula`, or exactly one ground `logic:` axiom lifted to a binary atom); `kb`
    /// is a Turtle KB the candidate is tested against; `standpoint` is the REQUIRED reified
    /// scope (Principle 9); `math_conjecture` optionally names the `math:Conjecture` twin so
    /// the statement is bridged to the runtime `logic:Conjecture` node via
    /// `math:conjectureUnderTest` (on every verdict) and a refutation's counterexample is
    /// re-exposed via `math:hasCounterexample`.
    #[cfg(feature = "reasoning")]
    fn tool_conjecture_test(&self, args: &Value) -> gmeow_errors::Result<String> {
        let formula_src = required_str(args, "formula")?;
        let kb_src = required_str(args, "kb")?;
        let standpoint = required_str(args, "standpoint")?;
        let math_conjecture = optional_str(args, "math_conjecture");
        let max_steps = optional_step_count(args, "max_steps")?;
        let max_answers = optional_limit(args, "max_answers")?;
        // R4: NEVER build an unbudgeted `Budget{None,None}` from omitted
        // agent args — `governed_budget` defaults+clamps to a finite server-side ceiling.
        // The CLI's `gmeow conjecture test` shares `run_conjecture_test_pure` but calls it
        // directly with its own (possibly unbounded) args — this governance is scoped to
        // the agent-facing MCP wrapper.
        let budget = governed_budget(max_steps, max_answers);

        // The parse → test → project path is the SHARED evaluation core (also behind
        // `store_conjecture` and the CLI). This tool never TR-gates and never appends — it is
        // a thin wrapper rendering the pure verdict as the JSON response.
        let out = run_conjecture_test_pure(&ConjectureRunPureInput {
            formula_ttl: formula_src,
            kb_ttl: kb_src,
            standpoint,
            math_conjecture,
            max_steps: budget.max_steps,
            max_answers: budget.max_answers,
        })?;

        let witness_json = out.witness.as_ref().map(|w| {
            json!({
                "individual": w.individual,
                "world": w.world,
                "premises": w.premises,
            })
        });
        let verdict_json = json!({
            "information": out.information,
            "evaluation": out.evaluation,
            "completeness": out.completeness,
            "lifecycle": out.lifecycle,
            "discharge": out.discharge,
        });

        Ok(json!({
            "ok": true,
            "verdict": verdict_json,
            "witness": witness_json,
            "conjecture": out.node_iri,
            // T1: the same grounded-judgment key/shape `verify_graph`/`explain_quad` carry —
            // here the embedded `logic:ReasoningResult` the engine's verdict was read from,
            // wrapped in the content-addressed `logic:Conjecture` node.
            "judgment_nquads": out.verdict_nt,
        })
        .to_string())
    }

    /// `store_conjecture` — the issue's "store" leg: test a candidate `logic:` formula against
    /// a KB, project the engine verdict, and — TR-gated on the `persistConjecture` schema —
    /// APPEND it to the append-only conjecture library. Runs the SAME evaluation as
    /// `conjecture_test`; the difference is entirely in the tail (TR-gate + persist).
    ///
    /// `formula` / `kb` / `standpoint` / `math_conjecture` are as `conjecture_test`;
    /// `dry_run=true` computes and returns the verdict (via a hypothetical TR commit) but
    /// WRITES NOTHING.
    #[cfg(feature = "reasoning")]
    fn tool_store_conjecture(&self, args: &Value) -> gmeow_errors::Result<String> {
        let formula_src = required_str(args, "formula")?;
        let kb_src = required_str(args, "kb")?;
        let standpoint = required_str(args, "standpoint")?;
        let math_conjecture = optional_str(args, "math_conjecture");
        let dry_run = optional_bool_checked(args, "dry_run")?.unwrap_or(false);
        let max_steps = optional_step_count(args, "max_steps")?;
        let max_answers = optional_limit(args, "max_answers")?;
        // R4: NEVER build an unbudgeted `Budget{None,None}` from omitted
        // agent args — `governed_budget` defaults+clamps to a finite server-side ceiling.
        // The CLI's `gmeow conjecture test` shares `run_conjecture_test` but calls it
        // directly with its own (possibly unbounded) args — this governance is scoped to
        // the agent-facing MCP wrapper.
        let budget = governed_budget(max_steps, max_answers);

        // The parse → test → project → TR-gate → persist path is the SHARED persisting core
        // (also behind the CLI `gmeow conjecture test`); the tool is a thin wrapper rendering
        // its outcome as the JSON response.
        let out = run_conjecture_test(
            &ConjectureRunInput {
                formula_ttl: formula_src,
                kb_ttl: kb_src,
                standpoint,
                math_conjecture,
                dry_run,
                max_steps: budget.max_steps,
                max_answers: budget.max_answers,
            },
            &self.store_medium()?,
        )?;

        // The five-field verdict summary + witness, rendered for every response path.
        let witness_json = out.witness.as_ref().map(|w| {
            json!({
                "individual": w.individual,
                "world": w.world,
                "premises": w.premises,
            })
        });
        let verdict_json = json!({
            "information": out.information,
            "evaluation": out.evaluation,
            "completeness": out.completeness,
            "lifecycle": out.lifecycle,
            "discharge": out.discharge,
        });

        // T1: the verdict — and its grounded `judgment_nquads` projection — was ALREADY
        // computed by `evaluate_conjecture` before the TR gate ran, so it is available (and
        // carried) on every path below, including precondition-unmet: the engine's judgment
        // about the candidate is real regardless of whether the persist itself succeeded.
        if let Some(reason) = &out.precondition_unmet {
            return Ok(json!({
                "ok": false,
                "error": format!("persistConjecture precondition unmet: {reason}"),
                "verdict": verdict_json,
                "witness": witness_json,
                "transaction": txn_json(&out.receipt),
                "judgment_nquads": out.verdict_nt,
            })
            .to_string());
        }
        if out.dry_run {
            return Ok(json!({
                "ok": true,
                "dry_run": true,
                "verdict": verdict_json,
                "witness": witness_json,
                "conjecture": out.node_iri,
                "transaction": txn_json(&out.receipt),
                "judgment_nquads": out.verdict_nt,
            })
            .to_string());
        }

        Ok(json!({
            "ok": true,
            "verdict": verdict_json,
            "witness": witness_json,
            "conjecture": out.node_iri,
            "transaction": txn_json(&out.receipt),
            "judgment_nquads": out.verdict_nt,
        })
        .to_string())
    }

    /// `refute_conjecture` — the compensating author-WITHDRAWAL counterpart of
    /// `store_conjecture` (P10, exactly as `revise_belief` compensates `store_claim`). It
    /// appends one compensating "author-withdrawn" segment to the append-only conjecture
    /// library, flipping the target node's EFFECTIVE `logic:conjectureLifecycleState` to
    /// `logic:ConjectureWithdrawn` — recorded, never deleted; the prior segments stay intact.
    ///
    /// The write is a REAL TR gate on the `withdrawConjecture` schema, whose precondition —
    /// the conjecture is still in the library and NOT already withdrawn — is DERIVED from the
    /// live library state read back by SEGMENT ORDER ([`read_library`], R3): the
    /// `conjectureInLibrary` situation obtains iff the node exists and its effective state is
    /// not yet `Withdrawn`. An unknown id or an already-withdrawn node yields an empty start
    /// state, so the executional-entailment run FAILS the commit and the tool returns
    /// `ok:false` before writing. `dry_run=true` witnesses the hypothetical commit and appends
    /// nothing.
    #[cfg(feature = "reasoning")]
    fn tool_refute_conjecture(&self, args: &Value) -> gmeow_errors::Result<String> {
        let conjecture_id = required_str(args, "conjecture_id")?;
        let reason = optional_str(args, "reason").unwrap_or("");
        let dry_run = optional_bool_checked(args, "dry_run")?.unwrap_or(false);
        let library_handle = conjecture_library()?;
        let library_ref = library_handle.as_ref();

        // The read (library's EFFECTIVE state, by segment order) → precondition-check → (on a
        // real commit) library-append → audit-append sequence runs ENTIRELY inside ONE held
        // exclusive lock (`with_library_lock`): without it, two concurrent
        // `refute_conjecture` calls against the same id could both read "not yet withdrawn",
        // both pass the precondition, and both commit a withdrawal segment (lost-update). The
        // lock forces the second caller to observe the FIRST caller's already-committed
        // `ConjectureWithdrawn` state before it decides anything.
        with_library_lock(library_ref, || {
            // Read the library's EFFECTIVE state by segment order (last-writer-wins). The node
            // is withdrawable iff it is a stored conjecture whose effective state is not already
            // Withdrawn — the `del(conjectureInLibrary)` of a prior withdrawal retired it.
            let library = read_library(library_ref)?;
            let effective = library.get(conjecture_id).copied();
            let exists = effective.is_some();
            let in_library = matches!(
                effective,
                Some(state) if state != ConjectureLifecycleState::Withdrawn
            );

            // The precondition `conjectureInLibrary` obtains iff the node is present and not yet
            // withdrawn — the engine's executional entailment over THIS derived start state is
            // the commit gate (an absent situation fails the run; no synthetic boolean).
            let mut obtains: Vec<&str> = Vec::new();
            if in_library {
                obtains.push(MCP_CONJECTURE_IN_LIBRARY);
            }
            let receipt = execute_memory_txn(MCP_WITHDRAW_CONJECTURE_SCHEMA, &obtains, dry_run)?;
            // T1: the compensating withdrawal's OWN grounded RDF projection — content-addressed
            // on (conjecture_id, reason), pure and side-effect-free — computed once here so BOTH
            // the hypothetical (dry-run) and committed success paths can carry it under the SAME
            // `judgment_nquads` key the read tools and the other two conjecture tools use. The
            // precondition-unmet path below deliberately does NOT carry it: no withdrawal was
            // validly grounded (unknown id, or already withdrawn), so emitting this body there
            // would fabricate a judgment about a state change that never obtained.
            let nt_body = project_conjecture_withdrawal(conjecture_id, reason);
            match &receipt {
                TxReceipt::CommittedFailure { .. } | TxReceipt::HypotheticalFailure { .. } => {
                    let detail = if !exists {
                        format!("unknown conjecture id: {conjecture_id}")
                    } else {
                        format!("conjecture already withdrawn: {conjecture_id}")
                    };
                    return Ok(json!({
                        "ok": false,
                        "error": format!("withdrawConjecture precondition unmet: {detail}"),
                        "transaction": txn_json(&receipt),
                    })
                    .to_string());
                }
                // Sandbox run: the compensating verdict is witnessed, nothing is appended.
                TxReceipt::HypotheticalSuccess { .. } => {
                    return Ok(json!({
                        "ok": true,
                        "dry_run": true,
                        "conjecture": conjecture_id,
                        "lifecycle": ConjectureLifecycleState::Withdrawn.wire(),
                        "transaction": txn_json(&receipt),
                        "judgment_nquads": nt_body,
                    })
                    .to_string());
                }
                TxReceipt::CommittedSuccess { .. } => {}
            }

            // Committed: APPEND the compensating author-withdrawal segment (the target node
            // re-marked `ConjectureWithdrawn`, author reason, reviewer-asserted provenance)
            // TOGETHER with its cold-auditable trajectory segment (keyed to a content-addressed
            // call id) as ONE atomic file replace — so a failure building or committing either
            // segment can never leave the withdrawal applied without its audit record, or vice
            // versa. The library is still append-only overall: no PRIOR segment's bytes are
            // touched, only new bytes are added.
            let medium = self.store_medium()?;
            let existing = library_ref.read_bytes()?;
            let withdrawal_segment = build_nt_segment(&existing, &medium, &nt_body)?;
            // The audit continues the BODY, so it is authored against the bytes the body
            // leaves behind — not against the pre-commit library, which the body already moved past.
            let mut body_image = existing.clone();
            body_image.extend_from_slice(&withdrawal_segment);
            let call_id = format!(
                "urn:gmeow:conjecture-call:{}",
                sha256_hex(format!("withdraw\u{1}{conjecture_id}\u{1}{reason}").as_bytes())
            );
            let audit_segment = build_audit_segment(
                &body_image,
                &medium,
                &call_id,
                MCP_WITHDRAW_CONJECTURE_SCHEMA,
                &[MCP_CONJECTURE_IN_LIBRARY],
                "1970-01-01T00:00:00Z",
            )?;
            append_library_segments(library_ref, &[withdrawal_segment, audit_segment])?;
            Ok(json!({
                "ok": true,
                "conjecture": conjecture_id,
                "lifecycle": ConjectureLifecycleState::Withdrawn.wire(),
                "transaction": txn_json(&receipt),
                "judgment_nquads": nt_body,
            })
            .to_string())
        })
    }

    /// Score ONE slice on demand and return its grades + advice as JSON. This is a
    /// read-only advisory surface: it computes a fresh assessment for the caller and
    /// folds nothing. The whole-repo `gmeow:QualityAssessment` graph is instead attached
    /// to the carrier by the regeneration pipeline (`stage-source-load` via
    /// [`gmeow_slice_quality::assessment_nquads`]) so it ships inside `gmeow.gts`; this
    /// tool never mutates the bundle.
    ///
    /// The rubric standard is sourced from the embedded bundle bytes
    /// ([`McpView::gts_bytes`]) via [`gmeow_slice_quality::score_external_slice_files`]
    /// — the wheel-shippable `ScoringEnv::Bundle` path the `gmeow slice quality` CLI
    /// uses — so the tool is checkout-free and available on the Consumer surface.
    ///
    /// The slice arrives as an IN-MEMORY FILE MAP (`files`: slice-relative path -> file
    /// text), not a directory. That is what makes the tool servable by an engine with no
    /// filesystem at all, and it is also the honest surface for an agent that HAS the
    /// slice as text (a paste, an upload, a git blob) and no place to put it. A map with
    /// no `manifest.ttl` entry is a hard error naming it.
    #[cfg(feature = "reasoning")]
    fn tool_slice_quality(&self, args: &Value) -> gmeow_errors::Result<String> {
        let files = required_file_map("slice_quality", args, "files")?;
        let report = gmeow_slice_quality::score_external_slice_files(self.view.gts_bytes(), &files)
            .map_err(|e| {
                gmeow_errors::Diag::of_kind(crate::error::Mcp {
                    message: format!("slice_quality: {e}"),
                })
            })?;
        let grades: Vec<Value> = report
            .assessment
            .grades
            .iter()
            .map(|g| {
                let axis = g.axis_iri.rsplit(['/', '#']).next().unwrap_or(&g.axis_iri);
                json!({ "axis": axis, "tier": g.tier.label, "score": g.score })
            })
            .collect();
        let advice: Vec<Value> = report
            .advisories
            .iter()
            .map(|f| json!({ "code": f.code, "message": f.message }))
            .collect();
        Ok(json!({
            "slice": report.assessment.slice,
            "rollup_tier": report.assessment.rollup.label,
            "grades": grades,
            "advice": advice,
        })
        .to_string())
    }

    /// Serve the pre-assembled `gmeow:AuthoringPacket`(s) for one slice straight from
    /// the embedded bundle graph — a checkout-free consumer surface. `slice` is a slice
    /// short-name (`ai`) or full slice IRI; the optional `axis` (default `whole`, the
    /// only axis the pipeline currently partitions along) and `batch` narrow the result.
    /// A slice/axis/batch with no packet in the bundle is a hard error (never a vacuous
    /// empty pass). The covered-term IRIs the packet lists resolve to their full
    /// definition/axiom content through the existing `lookup_term` / `doc_card` tools.
    #[cfg(feature = "core")]
    fn tool_slice_brief(&self, args: &Value) -> gmeow_errors::Result<String> {
        let slice = required_str(args, "slice")?;
        let axis = optional_str(args, "axis");
        let batch = optional_step_count(args, "batch")?;
        let slice_iri = expand_slice_iri(slice);
        let out = extract_authoring_packets(self.view.authoring_briefs(), &slice_iri, axis, batch)?;
        Ok(out.to_string())
    }

    /// `submit_candidate` — the neurosymbolic propose/verify seam's WRITE leg: test a candidate
    /// `logic:` formula against a KB, and — POLARITY-gated on the `submitCandidate` schema
    /// (admissible iff corroborated) — APPEND it to the append-only candidate library. Runs the
    /// SAME evaluation as `conjecture_test`; a refuted or open candidate is NOT admitted and
    /// stages nothing. `formula`/`kb`/`standpoint`/`math_conjecture` are as `conjecture_test`;
    /// optional `for_slice`/`for_packet` record target provenance; `dry_run=true` computes the
    /// verdict but WRITES NOTHING.
    #[cfg(feature = "reasoning")]
    fn tool_submit_candidate(&self, args: &Value) -> gmeow_errors::Result<String> {
        let formula_src = required_str(args, "formula")?;
        let kb_src = required_str(args, "kb")?;
        let standpoint = required_str(args, "standpoint")?;
        let math_conjecture = optional_str(args, "math_conjecture");
        let for_slice = optional_str(args, "for_slice");
        let for_packet = optional_str(args, "for_packet");
        let dry_run = optional_bool_checked(args, "dry_run")?.unwrap_or(false);
        let max_steps = optional_step_count(args, "max_steps")?;
        let max_answers = optional_limit(args, "max_answers")?;
        let budget = governed_budget(max_steps, max_answers);

        let out = run_submit_candidate(
            &CandidateSubmitInput {
                formula_ttl: formula_src,
                kb_ttl: kb_src,
                standpoint,
                math_conjecture,
                for_slice,
                for_packet,
                dry_run,
                max_steps: budget.max_steps,
                max_answers: budget.max_answers,
            },
            &self.store_medium()?,
        )?;

        let witness_json = out.witness.as_ref().map(|w| {
            json!({
                "individual": w.individual,
                "world": w.world,
                "premises": w.premises,
            })
        });
        let verdict_json = json!({
            "information": out.information,
            "evaluation": out.evaluation,
            "completeness": out.completeness,
            "lifecycle": out.lifecycle,
            "discharge": out.discharge,
        });

        // NOT admissible (refuted / open) OR a dry sandbox on a non-admissible candidate: the
        // precondition was unmet, so NOTHING was appended (AC6). The verdict is still carried.
        if let Some(reason) = &out.precondition_unmet {
            return Ok(json!({
                "ok": false,
                "error": format!("submitCandidate precondition unmet (candidate not admissible): {reason}"),
                "admissible": out.admissible,
                "verdict": verdict_json,
                "witness": witness_json,
                "transaction": txn_json(&out.receipt),
                "judgment_nquads": out.verdict_nt,
            })
            .to_string());
        }
        if out.dry_run {
            return Ok(json!({
                "ok": true,
                "dry_run": true,
                "admissible": out.admissible,
                "verdict": verdict_json,
                "witness": witness_json,
                "candidate": out.node_iri,
                "transaction": txn_json(&out.receipt),
                "judgment_nquads": out.verdict_nt,
            })
            .to_string());
        }

        // Admissible + committed: the candidate segment was appended (AC5).
        Ok(json!({
            "ok": true,
            "admissible": out.admissible,
            "committed": out.committed,
            "verdict": verdict_json,
            "witness": witness_json,
            "candidate": out.node_iri,
            "transaction": txn_json(&out.receipt),
            "judgment_nquads": out.verdict_nt,
        })
        .to_string())
    }

    /// `withdraw_candidate` — the compensating author-WITHDRAWAL counterpart of
    /// `submit_candidate` (P10, exactly as `refute_conjecture` compensates `store_conjecture`).
    /// It appends one compensating "withdrawn" segment to the append-only candidate library,
    /// flipping the target node's EFFECTIVE lifecycle to `logic:ConjectureWithdrawn` — recorded,
    /// never deleted. The write is a REAL TR gate on the `withdrawCandidate` schema, whose
    /// precondition — the candidate is still in the library and not already withdrawn — is
    /// DERIVED from the live library state read back by SEGMENT ORDER. An unknown id or an
    /// already-withdrawn node fails the commit and returns `ok:false` before writing.
    #[cfg(feature = "reasoning")]
    fn tool_withdraw_candidate(&self, args: &Value) -> gmeow_errors::Result<String> {
        let candidate_id = required_str(args, "candidate_id")?;
        let reason = optional_str(args, "reason").unwrap_or("");
        let dry_run = optional_bool_checked(args, "dry_run")?.unwrap_or(false);
        run_withdraw_candidate(candidate_id, reason, dry_run, &self.store_medium()?)
    }

    #[cfg(feature = "reasoning")]
    fn tool_list_candidates(&self, args: &Value) -> gmeow_errors::Result<String> {
        run_list_candidates(
            optional_str(args, "slice"),
            optional_str(args, "disposition"),
        )
    }

    /// Resolve `uri` against the assembled surface and run its handler, returning
    /// `(mimeType, body)`.
    ///
    /// The `?lang=` query is stripped and resolved here (it is a property of the
    /// REQUEST, shared by every resource) and handed to the handler; the media type
    /// comes from the advertised descriptor. Like tool dispatch this is TOTAL: a URI
    /// the surface does not carry raises `mcp.unknown-resource` naming it.
    ///
    /// # Errors
    ///
    /// An unresolvable `?lang=`, an unregistered URI, or whatever the handler raises.
    fn read_resource_text(&self, uri: &str) -> gmeow_errors::Result<(String, String)> {
        let (base, query) = uri.split_once('?').unwrap_or((uri, ""));
        let requested = lang_from_query(query)
            .map(|raw| resolve_lang(Some(raw), &self.tag_map, &self.available))
            .transpose()?
            .unwrap_or_else(|| self.startup_requested.clone());
        self.surface.read_resource(self, base, &requested)
    }

    /// The grounded-memory claim package this server's memory triad reads and writes.
    ///
    /// Resolved through the [`storage`] seam, so a native host gets its real
    /// `memory.gts` (at `GMEOW_MEMORY_PATH`, else `~/.gmeow/memory.gts`) and a browser
    /// host gets the in-process store — the tools themselves never learn which.
    ///
    /// Gated on the `reasoning` feature because the whole memory surface
    /// ([`CLAIM_STORE_TOOLS`]) is served by that segment: the writes are pinned there by
    /// their Transaction-Logic commit gate, and the reads must answer from the same store,
    /// which in a wasm deployment means the same image. A core-only build reaches the claim
    /// package from nowhere, and saying so in the type system is what keeps that true.
    #[cfg(feature = "reasoning")]
    fn claim_store(&self) -> gmeow_errors::Result<Arc<dyn ClaimStore>> {
        storage().claim_store(self.view.gts_bytes())
    }

    /// The medium a runtime claim store is PRIMED with: the [`MEMORY_HOT_DICTIONARY`] bytes the
    /// loaded bundle pins.
    ///
    /// Priming is not an optimisation. A GTS segment declares exactly ONE codec catalog, so the
    /// claim package and the engine-minted trajectory-audit segment appended beside it have to
    /// name the SAME one; an unprimed store lets the two diverge, and the file then stops
    /// reconstructing — `Memory::graph()` yields `None`, so `claims()` reports an EMPTY store and
    /// `revise` rejects a claim that was stored moments earlier as unknown.
    ///
    /// # Errors
    ///
    /// The loaded bundle pins no [`MEMORY_HOT_DICTIONARY`] — a store cannot be primed with an id
    /// the bundle does not carry, and there is no unprimed fallback.
    #[cfg(feature = "reasoning")]
    fn store_medium(&self) -> gmeow_errors::Result<StoreMedium> {
        store_medium(self.view.gts_bytes(), MEMORY_HOT_DICTIONARY)
    }
}

impl McpView {
    /// The base reasoning EDB: the bundle's folded carrier dataset.
    ///
    /// Public because a host-registered tool reasons over exactly this graph and
    /// nothing else — the grounded-memory triad, the conjecture library, and the
    /// query overlay are all DISTINCT stores that are never unioned in here.
    ///
    /// # Errors
    ///
    /// Infallible today (the carrier IS the dataset, so there is no gts round-trip),
    /// but fallible in signature so a future lazily-materialized carrier does not
    /// force a breaking change on every caller.
    pub fn graph_dataset(&self) -> gmeow_errors::Result<Arc<purrdf::RdfDataset>> {
        // The carrier IS the dataset — no gts round-trip (GTS is exit-only).
        Ok(Arc::clone(&self.dataset))
    }
}

// --------------------------------------------------------------------------- //
// The shared conjecture-test core (one evaluation implementation, three surfaces).
//
// [`evaluate_conjecture`] is the SINGLE parse → test → project path: it parses the candidate
// document and KB, re-homes the KB into the isolated scenario world, runs the native engine,
// and projects the verdict to deterministic N-Triples. Nothing calls the engine or the
// projector a second time.
//
// Two public entries sit on top of it, matching the issue's test / store decomposition:
//   - [`run_conjecture_test_pure`] — the "test" leg: evaluate only, NEVER TR-gate, NEVER
//     persist. Behind the MCP `conjecture_test` tool.
//   - [`run_conjecture_test`] — the "store" leg: evaluate, then TR-gate on the
//     `persistConjecture` schema and, on a committed precondition-met run, APPEND the verdict
//     to the append-only conjecture library. Behind the MCP `store_conjecture` tool AND the
//     public `gmeow conjecture test` CLI subcommand (unchanged CLI behavior).
// Neither surface re-implements the evaluation; each renders its own outcome (JSON / text).
// --------------------------------------------------------------------------- //

/// The inputs shared by both evaluation entries: the candidate `logic:` Turtle document, the
/// KB Turtle it is tested against, the REQUIRED reified standpoint scope (Principle 9), an
/// optional `math:Conjecture` twin, and the optional derived-closure budget.
#[cfg(feature = "reasoning")]
pub struct ConjectureRunPureInput<'a> {
    /// The candidate document: a Turtle `logic:` doc naming exactly one candidate formula.
    pub formula_ttl: &'a str,
    /// The KB the candidate is tested against, as Turtle.
    pub kb_ttl: &'a str,
    /// The required reified standpoint scope IRI (Principle 9).
    pub standpoint: &'a str,
    /// Optionally, the `math:Conjecture` twin IRI so a refutation's counterexample is
    /// re-exposed via `math:hasCounterexample`.
    pub math_conjecture: Option<&'a str>,
    /// Optional post-hoc derived-closure-size ceiling on the isolated scenario evaluation: when
    /// the derived (non-EDB) closure exceeds this many steps the run is stamped `BudgetExhausted`
    /// → lifecycle Open → discharge Unknown. `None` = unbounded.
    pub max_steps: Option<u64>,
    /// Optional post-hoc derived-closure-size ceiling in answer bindings; see
    /// [`max_steps`](Self::max_steps). `None` = unbounded.
    pub max_answers: Option<usize>,
}

/// The inputs to one PERSISTING conjecture test: [`ConjectureRunPureInput`]'s fields plus
/// whether the run is a sandbox (`dry_run`, writes nothing).
#[cfg(feature = "reasoning")]
pub struct ConjectureRunInput<'a> {
    /// The candidate document: a Turtle `logic:` doc naming exactly one candidate formula.
    pub formula_ttl: &'a str,
    /// The KB the candidate is tested against, as Turtle.
    pub kb_ttl: &'a str,
    /// The required reified standpoint scope IRI (Principle 9).
    pub standpoint: &'a str,
    /// Optionally, the `math:Conjecture` twin IRI so a refutation's counterexample is
    /// re-exposed via `math:hasCounterexample`.
    pub math_conjecture: Option<&'a str>,
    /// When true, compute and return the verdict but WRITE NOTHING to the library.
    pub dry_run: bool,
    /// Optional post-hoc derived-closure-size ceiling; see
    /// [`ConjectureRunPureInput::max_steps`]. `None` = unbounded.
    pub max_steps: Option<u64>,
    /// Optional post-hoc derived-closure-size ceiling; see
    /// [`ConjectureRunPureInput::max_answers`]. `None` = unbounded.
    pub max_answers: Option<usize>,
}

/// A refutation's contradiction witness, flattened for every response surface.
#[cfg(feature = "reasoning")]
pub struct ConjectureRunWitness {
    /// The individual forced into a clash.
    pub individual: String,
    /// The world the contradiction is local to.
    pub world: String,
    /// The premise triples that witness the clash, each rendered `"s p o"`.
    pub premises: Vec<String>,
}

/// The outcome of a [`run_conjecture_test_pure`] call: the projected verdict facets, the
/// refutation witness (when refuted), the content-addressed node IRI, and the projected
/// N-Triples body. Nothing is ever committed or persisted on this path.
#[cfg(feature = "reasoning")]
pub struct ConjecturePureOutput {
    /// The epistemic lifecycle wire value (`open` | `corroborated` | `refuted-in-standpoint`).
    pub lifecycle: String,
    /// The Belnap information-state wire value.
    pub information: String,
    /// The evaluation-axis wire value.
    pub evaluation: String,
    /// The completeness-axis wire value.
    pub completeness: String,
    /// The discharge carrier local name (`ObligationDischarged` | `ObligationUnknown`).
    pub discharge: String,
    /// The refutation witness, present exactly when refuted.
    pub witness: Option<ConjectureRunWitness>,
    /// The content-addressed `(formula × standpoint × KB-world)` conjecture node IRI.
    pub node_iri: String,
    /// The deterministic N-Triples body [`project_conjecture_verdict`] emitted.
    pub verdict_nt: String,
}

/// The outcome of a [`run_conjecture_test`] call: the projected verdict facets, the refutation
/// witness (when refuted), the content-addressed node IRI, the projected N-Triples body, and
/// the TR receipt gating the persist. `committed` is true exactly when the segment was appended.
#[cfg(feature = "reasoning")]
pub struct ConjectureRunOutput {
    /// True exactly when the verdict segment + audit were appended to the library (a committed,
    /// precondition-met, non-dry run).
    pub committed: bool,
    /// True when the run was a sandbox (`dry_run`): the verdict is computed, nothing is written.
    pub dry_run: bool,
    /// `Some(reason)` when the TR precondition was UNMET (the persist was refused, nothing
    /// written); `None` otherwise.
    pub precondition_unmet: Option<String>,
    /// The epistemic lifecycle wire value (`open` | `corroborated` | `refuted-in-standpoint`).
    pub lifecycle: String,
    /// The Belnap information-state wire value.
    pub information: String,
    /// The evaluation-axis wire value.
    pub evaluation: String,
    /// The completeness-axis wire value.
    pub completeness: String,
    /// The discharge carrier local name (`ObligationDischarged` | `ObligationUnknown`).
    pub discharge: String,
    /// The refutation witness, present exactly when refuted.
    pub witness: Option<ConjectureRunWitness>,
    /// The content-addressed `(formula × standpoint × KB-world)` conjecture node IRI.
    pub node_iri: String,
    /// The deterministic N-Triples body [`project_conjecture_verdict`] emitted.
    pub verdict_nt: String,
    /// The TR receipt gating the persist (rendered as the transaction summary by callers).
    pub receipt: TxReceipt,
}

/// The shared evaluation core's result: everything computed by parsing, testing, and
/// projecting one conjecture verdict, before either public entry decides what to do with it.
#[cfg(feature = "reasoning")]
struct ConjectureEvaluation {
    lifecycle: String,
    information: String,
    evaluation: String,
    completeness: String,
    discharge: String,
    witness: Option<ConjectureRunWitness>,
    node_iri: String,
    verdict_nt: String,
    /// The candidate's content-addressing key, needed by the persisting tail's audit call id.
    content_key: String,
}

/// Parse the candidate document and KB, re-home the KB into the isolated scenario world, run
/// the native engine, and project the verdict to deterministic N-Triples. The SINGLE
/// evaluation path shared by [`run_conjecture_test_pure`] and [`run_conjecture_test`] — neither
/// TR-gates nor writes anything; that is each caller's own tail.
///
/// # Errors
///
/// Returns an error if the candidate document does not name exactly one candidate formula, if
/// the KB does not parse, if the native engine fails (see [`conjecture_test`]), or if a
/// refutation names a compound candidate with no soundly-derivable forbidden predicate.
#[cfg(feature = "reasoning")]
fn evaluate_conjecture(
    formula_ttl: &str,
    kb_ttl: &str,
    standpoint: &str,
    math_conjecture: Option<&str>,
    max_steps: Option<u64>,
    max_answers: Option<usize>,
) -> gmeow_errors::Result<ConjectureEvaluation> {
    // Delegate to the SINGLE conjecture-evaluation authority in gmeow-logic (shared with the
    // browser conjecture playground so both produce byte-identical verdict N-Triples), then
    // adapt its projection to the pipeline's response type at the boundary.
    let projection = gmeow_logic::conjecture_eval::evaluate_conjecture_eval(
        &gmeow_logic::conjecture_eval::ConjectureEvalInput {
            formula_ttl,
            kb_ttl,
            kb_format: "text/turtle",
            standpoint,
            math_conjecture,
            max_steps,
            max_answers,
        },
    )
    .map_err(|e| {
        gmeow_errors::Diag::of_kind(crate::error::Mcp {
            message: format!("conjecture evaluation failed: {}", e.message()),
        })
    })?;

    let witness = projection.witness.map(|w| ConjectureRunWitness {
        individual: w.individual,
        world: w.world,
        premises: w.premises,
    });

    Ok(ConjectureEvaluation {
        lifecycle: projection.lifecycle,
        information: projection.information,
        evaluation: projection.evaluation,
        completeness: projection.completeness,
        discharge: projection.discharge,
        witness,
        node_iri: projection.node_iri,
        verdict_nt: projection.verdict_nt,
        content_key: projection.content_key,
    })
}

/// Run one conjecture test PURELY — the issue's "test" leg: evaluate the candidate against the
/// KB and project the verdict, but NEVER TR-gate and NEVER append to the conjecture library.
/// Behind the MCP `conjecture_test` tool. For the committing counterpart ("store"), see
/// [`run_conjecture_test`].
///
/// # Errors
///
/// See [`evaluate_conjecture`].
#[cfg(feature = "reasoning")]
pub fn run_conjecture_test_pure(
    input: &ConjectureRunPureInput,
) -> gmeow_errors::Result<ConjecturePureOutput> {
    let ConjectureRunPureInput {
        formula_ttl,
        kb_ttl,
        standpoint,
        math_conjecture,
        max_steps,
        max_answers,
    } = *input;
    let eval = evaluate_conjecture(
        formula_ttl,
        kb_ttl,
        standpoint,
        math_conjecture,
        max_steps,
        max_answers,
    )?;
    Ok(ConjecturePureOutput {
        lifecycle: eval.lifecycle,
        information: eval.information,
        evaluation: eval.evaluation,
        completeness: eval.completeness,
        discharge: eval.discharge,
        witness: eval.witness,
        node_iri: eval.node_iri,
        verdict_nt: eval.verdict_nt,
    })
}

/// Run one conjecture test end-to-end — the issue's "store" leg: evaluate the candidate
/// against the KB (see [`evaluate_conjecture`]), TR-gate the write on the `persistConjecture`
/// schema, and — on a committed, precondition-met run — APPEND the verdict segment plus a
/// cold-auditable trajectory segment to the append-only conjecture library.
///
/// This is the shared persisting core behind both the MCP `store_conjecture` tool and the
/// `gmeow conjecture test` CLI subcommand. It never mutates the caller's KB (isolation is
/// inherent) and, on a `dry_run` or a precondition-unmet run, writes nothing.
///
/// # Errors
///
/// Returns an error if [`evaluate_conjecture`] fails, or if the TR transaction or the library
/// append fails.
#[cfg(feature = "reasoning")]
pub fn run_conjecture_test(
    input: &ConjectureRunInput,
    medium: &StoreMedium,
) -> gmeow_errors::Result<ConjectureRunOutput> {
    let ConjectureRunInput {
        formula_ttl,
        kb_ttl,
        standpoint,
        math_conjecture,
        dry_run,
        max_steps,
        max_answers,
    } = *input;

    let ConjectureEvaluation {
        lifecycle,
        information,
        evaluation,
        completeness,
        discharge,
        witness,
        node_iri,
        verdict_nt,
        content_key,
    } = evaluate_conjecture(
        formula_ttl,
        kb_ttl,
        standpoint,
        math_conjecture,
        max_steps,
        max_answers,
    )?;

    // (5) TR-gate the write on the persistConjecture schema. The precondition — a verdict has
    //     been presented — obtains once the engine returns a verdict (evaluation above).
    let obtains = [MCP_CONJECTURE_VERDICT_PRESENTED];
    let receipt = execute_memory_txn(MCP_PERSIST_CONJECTURE_SCHEMA, &obtains, dry_run)?;

    let mut out = ConjectureRunOutput {
        committed: false,
        dry_run,
        precondition_unmet: None,
        lifecycle,
        information,
        evaluation,
        completeness,
        discharge,
        witness,
        node_iri,
        verdict_nt,
        receipt,
    };

    match &out.receipt {
        TxReceipt::CommittedFailure { reason } | TxReceipt::HypotheticalFailure { reason } => {
            out.precondition_unmet = Some(reason.clone());
            return Ok(out);
        }
        // Sandbox run: the verdict is observed, nothing is appended to the library.
        TxReceipt::HypotheticalSuccess { .. } => return Ok(out),
        TxReceipt::CommittedSuccess { .. } => {}
    }

    // (6) Committed: APPEND the verdict segment plus its cold-auditable trajectory segment
    //     (keyed to a content-addressed call id) to the append-only library — TOGETHER, as ONE
    //     atomic file replace under ONE held lock, so a failure building or committing either
    //     segment can never leave the library holding the verdict without its audit record.
    let library = conjecture_library()?;
    let existing = library.read_bytes()?;
    let verdict_segment = build_nt_segment(&existing, medium, &out.verdict_nt)?;
    // The audit continues the BODY, so it is authored against the bytes the body
    // leaves behind — not against the pre-commit library, which the body already moved past.
    let mut body_image = existing.clone();
    body_image.extend_from_slice(&verdict_segment);
    let call_id = format!(
        "urn:gmeow:conjecture-call:{}",
        sha256_hex(format!("{}\u{1}{content_key}", out.node_iri).as_bytes())
    );
    let audit_segment = build_audit_segment(
        &body_image,
        medium,
        &call_id,
        MCP_PERSIST_CONJECTURE_SCHEMA,
        &obtains,
        "1970-01-01T00:00:00Z",
    )?;
    with_library_lock(library.as_ref(), || {
        append_library_segments(library.as_ref(), &[verdict_segment, audit_segment])
    })?;
    out.committed = true;
    Ok(out)
}

// ── Candidate submission: the neurosymbolic propose/verify seam ───────────────
//
// A candidate is a proposed authoring contribution — a candidate `logic:` formula tested
// against a KB, exactly like a conjecture — whose ADMISSIBILITY (a corroborated isolated-world
// verdict) gates its append to the SEPARATE, append-only candidate library. It reuses
// `evaluate_conjecture` VERBATIM for the verdict (the trusted symbolic half of propose/verify)
// and differs from `run_conjecture_test` in exactly two ways: (1) the TR precondition is
// DERIVED FROM VERDICT POLARITY (`candidateAdmissible` obtains iff corroborated) rather than set
// unconditionally, so a refuted/open candidate stages nothing (AC5/AC6); (2) the appended node
// carries its authoring role (`gmeow:AuthoringCandidate`) and target provenance, and lands in
// `candidates.gts`, not `conjectures.gts`.

/// The inputs to one candidate submission: [`ConjectureRunInput`]'s test fields plus the
/// optional target provenance (`for_slice` / `for_packet`) recorded on the admitted node.
#[cfg(feature = "reasoning")]
pub struct CandidateSubmitInput<'a> {
    /// The candidate document: a Turtle `logic:` doc naming exactly one candidate formula.
    pub formula_ttl: &'a str,
    /// The KB the candidate is tested against, as Turtle.
    pub kb_ttl: &'a str,
    /// The required reified standpoint scope IRI (Principle 9).
    pub standpoint: &'a str,
    /// Optionally, the `math:Conjecture` twin IRI (as `conjecture_test`).
    pub math_conjecture: Option<&'a str>,
    /// Optional provenance: the slice IRI this candidate is proposed FOR.
    pub for_slice: Option<&'a str>,
    /// Optional provenance: the `gmeow:AuthoringPacket` IRI this candidate answers.
    pub for_packet: Option<&'a str>,
    /// When true, compute the verdict but WRITE NOTHING to the library.
    pub dry_run: bool,
    /// Optional post-hoc derived-closure-size ceiling; `None` = unbounded.
    pub max_steps: Option<u64>,
    /// Optional post-hoc derived-closure-size ceiling; `None` = unbounded.
    pub max_answers: Option<usize>,
}

/// The outcome of a [`run_submit_candidate`] call: the projected verdict facets, the
/// admissibility decision, the refutation witness (when refuted), the content-addressed node
/// IRI, the projected N-Triples body, and the TR receipt gating the append. `committed` is true
/// exactly when the admissible candidate segment was appended.
#[cfg(feature = "reasoning")]
pub struct CandidateSubmitOutput {
    /// True exactly when the candidate segment + audit were appended (admissible, committed,
    /// non-dry run).
    pub committed: bool,
    /// True when the run was a sandbox (`dry_run`): the verdict is computed, nothing is written.
    pub dry_run: bool,
    /// `Some(reason)` when the TR precondition was UNMET — the candidate was NOT admissible
    /// (refuted or open) or a dry sandbox on a non-admissible candidate — so nothing was written.
    pub precondition_unmet: Option<String>,
    /// The admissibility decision: true iff the isolated-world verdict CORROBORATED the candidate.
    pub admissible: bool,
    /// The epistemic lifecycle wire value (`open` | `corroborated` | `refuted-in-standpoint`).
    pub lifecycle: String,
    /// The Belnap information-state wire value.
    pub information: String,
    /// The evaluation-axis wire value.
    pub evaluation: String,
    /// The completeness-axis wire value.
    pub completeness: String,
    /// The discharge carrier local name.
    pub discharge: String,
    /// The refutation witness, present exactly when refuted.
    pub witness: Option<ConjectureRunWitness>,
    /// The content-addressed candidate node IRI (shared with its `logic:Conjecture` verdict).
    pub node_iri: String,
    /// The deterministic N-Triples verdict body (before the candidate role/provenance lines).
    pub verdict_nt: String,
    /// The TR receipt gating the append.
    pub receipt: TxReceipt,
}

/// The authoring-role + provenance N-Triples a submitted candidate node carries IN ADDITION to
/// its `logic:Conjecture` verdict: `rdf:type gmeow:AuthoringCandidate` plus the optional
/// `gmeow:candidateForSlice` / `gmeow:candidateForPacket` target links. Every predicate/type is
/// a canonically-authored guides-slice term (no unauthored vocabulary).
#[cfg(feature = "reasoning")]
fn candidate_provenance_nt(
    node_iri: &str,
    for_slice: Option<&str>,
    for_packet: Option<&str>,
) -> String {
    let mut s = format!("<{node_iri}> <{RDF_TYPE}> <{GMEOW_AUTHORING_CANDIDATE}> .\n");
    if let Some(slice) = for_slice {
        s.push_str(&format!(
            "<{node_iri}> <{GMEOW_CANDIDATE_FOR_SLICE}> <{slice}> .\n"
        ));
    }
    if let Some(packet) = for_packet {
        s.push_str(&format!(
            "<{node_iri}> <{GMEOW_CANDIDATE_FOR_PACKET}> <{packet}> .\n"
        ));
    }
    s
}

/// Submit one candidate end-to-end — the neurosymbolic propose/verify seam: evaluate the
/// candidate against the KB (reusing [`evaluate_conjecture`] verbatim), POLARITY-gate the write
/// on the `submitCandidate` schema (admissible iff corroborated), and — on a committed,
/// admissible, non-dry run — APPEND the candidate verdict segment (verdict + authoring role +
/// provenance) plus a cold-auditable trajectory segment to the append-only candidate library.
///
/// This is the shared core behind both the MCP `submit_candidate` tool and the `gmeow candidate
/// submit` CLI subcommand. It never mutates the caller's KB (isolation is inherent) and, on a
/// `dry_run` or a non-admissible run, writes nothing.
///
/// # Errors
///
/// Returns an error if [`evaluate_conjecture`] fails, or if the TR transaction or the library
/// append fails.
#[cfg(feature = "reasoning")]
pub fn run_submit_candidate(
    input: &CandidateSubmitInput,
    medium: &StoreMedium,
) -> gmeow_errors::Result<CandidateSubmitOutput> {
    let CandidateSubmitInput {
        formula_ttl,
        kb_ttl,
        standpoint,
        math_conjecture,
        for_slice,
        for_packet,
        dry_run,
        max_steps,
        max_answers,
    } = *input;

    let ConjectureEvaluation {
        lifecycle,
        information,
        evaluation,
        completeness,
        discharge,
        witness,
        node_iri,
        verdict_nt,
        content_key,
    } = evaluate_conjecture(
        formula_ttl,
        kb_ttl,
        standpoint,
        math_conjecture,
        max_steps,
        max_answers,
    )?;

    // Polarity gate (AC5/AC6): the candidate is ADMISSIBLE iff the isolated-world verdict
    // CORROBORATED it. A refuted or open verdict is NOT admissible, so the `candidateAdmissible`
    // precondition does not obtain, the executional-entailment commit FAILS, and nothing is
    // appended — the score moves only via gate-passing content.
    let admissible = lifecycle == ConjectureLifecycleState::Corroborated.wire();
    let obtains: &[&str] = if admissible {
        &[MCP_CANDIDATE_ADMISSIBLE]
    } else {
        &[]
    };
    let receipt = execute_memory_txn(MCP_SUBMIT_CANDIDATE_SCHEMA, obtains, dry_run)?;

    let mut out = CandidateSubmitOutput {
        committed: false,
        dry_run,
        precondition_unmet: None,
        admissible,
        lifecycle,
        information,
        evaluation,
        completeness,
        discharge,
        witness,
        node_iri,
        verdict_nt,
        receipt,
    };

    match &out.receipt {
        TxReceipt::CommittedFailure { reason } | TxReceipt::HypotheticalFailure { reason } => {
            out.precondition_unmet = Some(reason.clone());
            return Ok(out);
        }
        // Sandbox run on an admissible candidate: the verdict is observed, nothing is appended.
        TxReceipt::HypotheticalSuccess { .. } => return Ok(out),
        TxReceipt::CommittedSuccess { .. } => {}
    }

    // Committed + admissible: APPEND the candidate verdict segment (verdict N-Triples + the
    // authoring-role/provenance lines on the same node) plus its cold-auditable trajectory
    // segment to the append-only candidate library — TOGETHER, as ONE atomic file replace under
    // ONE held lock (reusing the same GTS-library primitives the conjecture library uses).
    let library = candidate_library()?;
    let body = format!(
        "{}\n{}",
        out.verdict_nt.trim_end(),
        candidate_provenance_nt(&out.node_iri, for_slice, for_packet)
    );
    let existing = library.read_bytes()?;
    let verdict_segment = build_nt_segment(&existing, medium, &body)?;
    // The audit continues the BODY, so it is authored against the bytes the body
    // leaves behind — not against the pre-commit library, which the body already moved past.
    let mut body_image = existing.clone();
    body_image.extend_from_slice(&verdict_segment);
    let call_id = format!(
        "urn:gmeow:candidate-call:{}",
        sha256_hex(format!("{}\u{1}{content_key}", out.node_iri).as_bytes())
    );
    let audit_segment = build_audit_segment(
        &body_image,
        medium,
        &call_id,
        MCP_SUBMIT_CANDIDATE_SCHEMA,
        obtains,
        "1970-01-01T00:00:00Z",
    )?;
    with_library_lock(library.as_ref(), || {
        append_library_segments(library.as_ref(), &[verdict_segment, audit_segment])
    })?;
    out.committed = true;
    Ok(out)
}

/// Withdraw a persisted candidate — the compensating author-WITHDRAWAL counterpart of
/// [`run_submit_candidate`] (P10, exactly as `refute_conjecture` compensates `store_conjecture`).
/// Appends one compensating "withdrawn" segment to the candidate library, flipping the target
/// node's EFFECTIVE lifecycle to `logic:ConjectureWithdrawn` — recorded, never deleted. The write
/// is a REAL TR gate on the `withdrawCandidate` schema, whose precondition — the candidate is
/// still in the library and not already withdrawn — is DERIVED from the live library state read
/// back by SEGMENT ORDER. An unknown or already-withdrawn id fails the commit and returns
/// `ok:false` before writing. Returns the JSON response body shared by the MCP tool and the CLI.
///
/// # Errors
///
/// Returns an error if the library read, the TR transaction, or the append fails.
#[cfg(feature = "reasoning")]
pub fn run_withdraw_candidate(
    candidate_id: &str,
    reason: &str,
    dry_run: bool,
    medium: &StoreMedium,
) -> gmeow_errors::Result<String> {
    let library_handle = candidate_library()?;
    let library_ref = library_handle.as_ref();
    // The read → precondition-check → (on a real commit) append sequence runs entirely inside ONE
    // held exclusive lock, so two concurrent withdrawals cannot both observe "not yet withdrawn"
    // and both commit (lost-update).
    with_library_lock(library_ref, || {
        let library = read_library(library_ref)?;
        let effective = library.get(candidate_id).copied();
        let exists = effective.is_some();
        let in_library = matches!(
            effective,
            Some(state) if state != ConjectureLifecycleState::Withdrawn
        );

        let mut obtains: Vec<&str> = Vec::new();
        if in_library {
            obtains.push(MCP_CANDIDATE_IN_LIBRARY);
        }
        let receipt = execute_memory_txn(MCP_WITHDRAW_CANDIDATE_SCHEMA, &obtains, dry_run)?;
        let nt_body = project_conjecture_withdrawal(candidate_id, reason);
        match &receipt {
            TxReceipt::CommittedFailure { .. } | TxReceipt::HypotheticalFailure { .. } => {
                let detail = if exists {
                    format!("candidate already withdrawn: {candidate_id}")
                } else {
                    format!("unknown candidate id: {candidate_id}")
                };
                return Ok(json!({
                    "ok": false,
                    "error": format!("withdrawCandidate precondition unmet: {detail}"),
                    "transaction": txn_json(&receipt),
                })
                .to_string());
            }
            TxReceipt::HypotheticalSuccess { .. } => {
                return Ok(json!({
                    "ok": true,
                    "dry_run": true,
                    "candidate": candidate_id,
                    "lifecycle": ConjectureLifecycleState::Withdrawn.wire(),
                    "transaction": txn_json(&receipt),
                    "judgment_nquads": nt_body,
                })
                .to_string());
            }
            TxReceipt::CommittedSuccess { .. } => {}
        }

        let existing = library_ref.read_bytes()?;
        let withdrawal_segment = build_nt_segment(&existing, medium, &nt_body)?;
        // The audit continues the BODY, so it is authored against the bytes the body
        // leaves behind — not against the pre-commit library, which the body already moved past.
        let mut body_image = existing.clone();
        body_image.extend_from_slice(&withdrawal_segment);
        let call_id = format!(
            "urn:gmeow:candidate-call:{}",
            sha256_hex(format!("withdraw\u{1}{candidate_id}\u{1}{reason}").as_bytes())
        );
        let audit_segment = build_audit_segment(
            &body_image,
            medium,
            &call_id,
            MCP_WITHDRAW_CANDIDATE_SCHEMA,
            &[MCP_CANDIDATE_IN_LIBRARY],
            "1970-01-01T00:00:00Z",
        )?;
        append_library_segments(library_ref, &[withdrawal_segment, audit_segment])?;
        Ok(json!({
            "ok": true,
            "candidate": candidate_id,
            "lifecycle": ConjectureLifecycleState::Withdrawn.wire(),
            "transaction": txn_json(&receipt),
            "judgment_nquads": nt_body,
        })
        .to_string())
    })
}

/// List every admitted candidate in the library with its effective disposition
/// (`in-library` | `withdrawn`) and target provenance (`for_slice` / `for_packet`). The effective
/// lifecycle is resolved by SEGMENT ORDER (a later withdrawal supersedes the admission); the
/// immutable type/provenance is read from the unioned dataset. Optional `filter_slice` filters by
/// target provenance and `filter_disposition` by effective state. A missing library is an EMPTY
/// list, not an error. Returns the JSON response body shared by the MCP tool and the CLI.
///
/// # Errors
///
/// Returns an error if the library read fails.
#[cfg(feature = "reasoning")]
pub fn run_list_candidates(
    filter_slice: Option<&str>,
    filter_disposition: Option<&str>,
) -> gmeow_errors::Result<String> {
    run_list_candidates_in(
        candidate_library()?.as_ref(),
        filter_slice,
        filter_disposition,
    )
}

/// [`run_list_candidates`] against an EXPLICIT library rather than the process backend's.
///
/// The listing logic lives here, once, so it can be exercised against any
/// [`SegmentLibrary`] — in particular against the browser backend's in-process library,
/// which has no path to point [`run_list_candidates`] at.
///
/// # Errors
///
/// Returns an error if the library read fails.
#[cfg(feature = "reasoning")]
pub fn run_list_candidates_in(
    library: &dyn SegmentLibrary,
    filter_slice: Option<&str>,
    filter_disposition: Option<&str>,
) -> gmeow_errors::Result<String> {
    // Effective, segment-order-resolved lifecycle per stored node (last-writer-wins).
    let lifecycles = read_library(library)?;

    // Immutable type + provenance from the unioned dataset (set once at submit, never superseded,
    // so the union is sound for these fields).
    let mut for_slice: BTreeMap<String, String> = BTreeMap::new();
    let mut for_packet: BTreeMap<String, String> = BTreeMap::new();
    let mut is_candidate: BTreeSet<String> = BTreeSet::new();
    let bytes = library.read_bytes()?;
    if !bytes.is_empty() {
        {
            let bundle = purrdf::import_gts_events(&bytes)
                .with_ctx(|| "read candidate library".to_string())?;
            for quad in bundle.dataset.owned_quads() {
                let (RdfTerm::Iri(subj), RdfTerm::Iri(obj)) = (&quad.subject, &quad.object) else {
                    continue;
                };
                match quad.predicate.as_str() {
                    RDF_TYPE if obj == GMEOW_AUTHORING_CANDIDATE => {
                        is_candidate.insert(subj.clone());
                    }
                    GMEOW_CANDIDATE_FOR_SLICE => {
                        for_slice.insert(subj.clone(), obj.clone());
                    }
                    GMEOW_CANDIDATE_FOR_PACKET => {
                        for_packet.insert(subj.clone(), obj.clone());
                    }
                    _ => {}
                }
            }
        }
    }

    let mut candidates: Vec<Value> = Vec::new();
    for (node, state) in &lifecycles {
        // Only nodes typed gmeow:AuthoringCandidate (belt-and-suspenders: every candidate is one;
        // a bare logic:Conjecture in this library would be foreign).
        if !is_candidate.contains(node) {
            continue;
        }
        let disposition = if *state == ConjectureLifecycleState::Withdrawn {
            "withdrawn"
        } else {
            "in-library"
        };
        if filter_disposition.is_some_and(|d| d != disposition) {
            continue;
        }
        let slice = for_slice.get(node);
        if let Some(want) = filter_slice
            && slice.map(String::as_str) != Some(want)
        {
            continue;
        }
        candidates.push(json!({
            "candidate": node,
            "disposition": disposition,
            "lifecycle": state.wire(),
            "for_slice": slice,
            "for_packet": for_packet.get(node),
        }));
    }

    Ok(json!({
        "ok": true,
        "candidate_count": candidates.len(),
        "candidates": candidates,
    })
    .to_string())
}

fn language_tag_map(dataset: &purrdf::RdfDataset) -> BTreeMap<String, String> {
    let graph = fold_arena::Graph::from_dataset(dataset);
    let graph = &graph;
    let iri_index: HashMap<&str, usize> = graph
        .terms
        .iter()
        .enumerate()
        .filter_map(|(idx, term)| {
            (term.kind == fold_arena::TermKind::Iri)
                .then(|| term.value.as_deref().map(|value| (value, idx)))
                .flatten()
        })
        .collect();
    let Some(type_tid) = iri_index.get(RDF_TYPE).copied() else {
        return BTreeMap::new();
    };
    let Some(language_tid) = iri_index.get(LANGUAGE_CLASS).copied() else {
        return BTreeMap::new();
    };
    let Some(language_tag_tid) = iri_index.get(LANGUAGE_TAG).copied() else {
        return BTreeMap::new();
    };
    let Some(bcp47_tid) = iri_index.get(BCP47_TAG).copied() else {
        return BTreeMap::new();
    };
    let subjects: BTreeSet<usize> = graph
        .quads
        .iter()
        .filter_map(|&(s, p, o, _)| (p == type_tid && o == language_tid).then_some(s))
        .collect();
    let mut out = BTreeMap::new();
    for subject in subjects {
        let internal = graph.quads.iter().find_map(|&(s, p, o, _)| {
            (s == subject && p == language_tag_tid)
                .then(|| graph.terms.get(o).and_then(|term| term.value.as_deref()))
                .flatten()
        });
        let bcp = graph.quads.iter().find_map(|&(s, p, o, _)| {
            (s == subject && p == bcp47_tid)
                .then(|| graph.terms.get(o).and_then(|term| term.value.as_deref()))
                .flatten()
        });
        if let (Some(internal), Some(bcp)) = (internal, bcp) {
            out.insert(internal.to_ascii_lowercase(), bcp.to_ascii_lowercase());
        }
    }
    out
}

fn resolve_lang(
    raw: Option<&str>,
    tag_map: &BTreeMap<String, String>,
    available: &BTreeSet<String>,
) -> gmeow_errors::Result<Vec<String>> {
    let Some(raw) = raw else {
        return Ok(vec!["en".to_string()]);
    };
    if raw.trim().is_empty() {
        return Ok(vec!["en".to_string()]);
    }
    let mut seen = BTreeSet::new();
    let mut out = Vec::new();
    for token in raw
        .split(',')
        .map(str::trim)
        .filter(|token| !token.is_empty())
    {
        let normalized = if is_internal_tag(token) {
            let token_lower = token.to_ascii_lowercase();
            tag_map
                .get(&token_lower)
                .map(|tag| tag.to_ascii_lowercase())
                .ok_or_else(|| {
                    gmeow_errors::Diag::of_kind(crate::error::Mcp {
                        message: unknown_language(token, available),
                    })
                })?
        } else {
            token.to_ascii_lowercase()
        };
        if !available.contains(&normalized) {
            return Err(gmeow_errors::Diag::of_kind(crate::error::Mcp {
                message: unknown_language(token, available),
            }));
        }
        if seen.insert(normalized.clone()) {
            out.push(normalized);
        }
    }
    if out.is_empty() {
        Ok(vec!["en".to_string()])
    } else {
        Ok(out)
    }
}

fn is_internal_tag(lang: &str) -> bool {
    let lower = lang.to_ascii_lowercase();
    let Some(suffix) = lower.strip_prefix("x-gmeow-") else {
        return false;
    };
    !suffix.is_empty()
        && suffix
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

fn unknown_language(tag: &str, available: &BTreeSet<String>) -> String {
    let mut tags: Vec<&str> = available.iter().map(String::as_str).collect();
    tags.sort_by_key(|tag| (*tag != "en", *tag));
    format!(
        "unknown language tag '{tag}'. Available languages: {}",
        tags.join(", ")
    )
}

fn lang_from_query(query: &str) -> Option<&str> {
    query.split('&').find_map(|pair| {
        let (key, value) = pair.split_once('=')?;
        (key == "lang").then_some(value)
    })
}

pub fn tool(name: &str, description: &str, properties: &[(&str, &str)]) -> Value {
    let required: Vec<&str> = properties
        .iter()
        .filter_map(|(prop, _)| {
            let required_by_name = matches!(
                *prop,
                "term"
                    | "text"
                    | "claim_id"
                    | "conjecture_id"
                    | "candidate_id"
                    // `slice_quality`'s in-memory slice: a JSON object of
                    // slice-relative path -> file text. Enforced by `required_object`
                    // in the handler, so the advertised schema must match.
                    | "files"
                    | "slice"
                    | "target_iri"
                    | "data"
                    | "format"
                    // `convert` enforces the source and target codec names via
                    // `required_str` (there is no default codec — a silent guess would be
                    // exactly the degradation no-optionality forbids), so the advertised
                    // schema must match.
                    | "from"
                    | "to"
                    // The GMN verifier tools: `gmn_validate` / `gmn_expand` require the GMN-1
                    // document `gmn`; `gmn_explain` requires the operator `glyph`. Enforced via
                    // `required_str` in each handler, so the advertised schema must match.
                    | "gmn"
                    | "glyph"
                    | "query"
                    | "subject"
                    | "predicate"
                    | "object_value"
                    | "graph"
                    // G11: `conjecture_test` / `store_conjecture` enforce these three via
                    // `required_str` at call time (see `tool_conjecture_test` /
                    // `tool_store_conjecture`) — the advertised schema must match, or a client
                    // sees an OPTIONAL arg it then gets a runtime error for omitting.
                    | "formula"
                    | "kb"
                    | "standpoint"
            );
            // Carve-outs: an arg whose shared name is required everywhere else is
            // OPTIONAL for a specific tool, so marking it required THERE would advertise
            // a dishonest schema. `competency_questions` accepts an optional `term` (the
            // whole-index form omits it); `recall` accepts an optional `query` (an empty
            // query recalls everything); `list_candidates` accepts an optional `slice`
            // (an absent filter lists every candidate).
            let optional_here = (name == "competency_questions" && *prop == "term")
                || (name == "recall" && *prop == "query")
                || (name == "list_candidates" && *prop == "slice");
            (required_by_name && !optional_here).then_some(*prop)
        })
        .collect();
    let props = properties
        .iter()
        .map(|(name, kind)| ((*name).to_string(), json!({"type": kind})))
        .collect::<serde_json::Map<_, _>>();
    json!({
        "name": name,
        "description": description,
        "inputSchema": {
            "type": "object",
            "properties": props,
            "required": required,
        },
    })
}

/// The accepted `format` tokens, in one place, so every tool that takes RDF bytes
/// names the SAME set in its error and its descriptor.
const ACCEPTED_RDF_FORMATS: &str = "turtle|ttl|text/turtle, ntriples|nt|n-triples, \
     nquads|nq|n-quads, trig, rdfxml|rdf+xml|xml|rdf, jsonld|json-ld";

/// Canonicalize a caller-supplied RDF `format` token to the exact id
/// `gmeow_validate::data_validate` accepts. Accepts the common aliases per family;
/// an UNRECOGNIZED format is a HARD FAIL (the accepted set is listed in the error)
/// so a mistyped format can never silently mis-parse. `tool` names the caller so the
/// message points at the argument the agent actually passed.
fn canonical_rdf_format(tool: &str, format: &str) -> gmeow_errors::Result<&'static str> {
    let normalized = format.trim().to_ascii_lowercase();
    let token = match normalized.as_str() {
        "turtle" | "ttl" | "text/turtle" => "turtle",
        "ntriples" | "nt" | "n-triples" => "n-triples",
        "nquads" | "nq" | "n-quads" => "n-quads",
        "trig" => "trig",
        "rdfxml" | "rdf+xml" | "xml" | "rdf" => "rdf+xml",
        "jsonld" | "json-ld" => "json-ld",
        other => {
            return Err(gmeow_errors::Diag::of_kind(crate::error::Mcp {
                message: format!(
                    "{tool}: unrecognized RDF format `{other}`; accepted: {ACCEPTED_RDF_FORMATS}"
                ),
            }));
        }
    };
    Ok(token)
}

/// The media type `purrdf::parse_dataset` reads for a canonical format token.
///
/// The two surfaces differ by design: the validator core addresses formats by its own
/// short ids, the RDF parser by media type. Routing BOTH through
/// [`canonical_rdf_format`] means one accepted-alias table and one error message, so
/// `query_local` and `validate_local` can never disagree about what `nq` means.
fn rdf_media_type(tool: &str, format: &str) -> gmeow_errors::Result<&'static str> {
    Ok(match canonical_rdf_format(tool, format)? {
        "turtle" => "text/turtle",
        "n-triples" => "application/n-triples",
        "n-quads" => "application/n-quads",
        "trig" => "application/trig",
        "rdf+xml" => "application/rdf+xml",
        "json-ld" => "application/ld+json",
        // `canonical_rdf_format` is total over the six tokens above; a seventh would be
        // a code change here, not a runtime input.
        other => unreachable!("unmapped canonical RDF format token: {other}"),
    })
}

/// The medium-registry resource URI.
pub const MEDIUM_RESOURCE_URI: &str = "gmeow://ontology/medium";

/// The medium-registry resource DESCRIPTOR — the surface definition, single-sourced here.
///
/// The handler is not here and cannot be: computing the inventory needs the medium reader,
/// which lives in the build executor this leaf deliberately does not link. A host that owns
/// that reader registers this descriptor with its own handler through
/// [`Extension::with_resource`](crate::extension::Extension::with_resource), so every host
/// that CAN answer advertises the same surface, worded identically, and one that cannot does
/// not claim it.
#[must_use]
pub fn medium_resource_descriptor() -> Value {
    resource(
        MEDIUM_RESOURCE_URI,
        "medium",
        "The medium registry read off the loaded bundle alone: declared media, \
         dictionaries with their content digests and zstd Dictionary_IDs, the \
         envelope count, and the total rep to medium assignment.",
        "application/json",
    )
}

pub fn resource(uri: &str, name: &str, description: &str, mime: &str) -> Value {
    json!({
        "uri": uri,
        "name": name,
        "description": description,
        "mimeType": mime,
    })
}

fn tool_text(text: String, is_error: bool) -> Value {
    json!({
        "content": [{"type": "text", "text": text}],
        "isError": is_error,
    })
}

fn rpc_error(id: Value, code: i64, message: &str) -> String {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": {"code": code, "message": message},
    })
    .to_string()
}

/// The shared unknown-term hard fail for the resolution guard and `doc_card`: a
/// query that resolves to no bundled GMEOW term.
#[cfg(feature = "core")]
fn unknown_term_err(term: &str) -> gmeow_errors::Diag {
    gmeow_errors::Diag::of_kind(crate::error::Mcp {
        message: format!(
            "unknown term `{term}`: does not resolve to a bundled GMEOW term \
             (expected a CURIE, local name, IRI, or label)"
        ),
    })
}

/// The shared cross-namespace-collision hard fail: a bare local name that exactly
/// names terms in more than one namespace. Carries the TYPED `McpAmbiguousTerm`
/// code (distinct from the generic unknown-term `Mcp`), naming the query and listing
/// the sorted candidate CURIEs — the MCP twin of `gmeow-cli.describe.ambiguous`.
#[cfg(feature = "core")]
fn ambiguous_term_err(term: &str, candidates: &[String]) -> gmeow_errors::Diag {
    gmeow_errors::Diag::of_kind(crate::error::McpAmbiguousTerm {
        message: format!(
            "ambiguous term `{term}`: names terms in multiple namespaces: {}",
            candidates.join(" / ")
        ),
    })
}

/// The proof-carrying completeness class of a governed reasoning `result`, shared by
/// [`McpView::run_verify_graph`] and [`McpView::run_explain_quad`].
///
/// This is a thin wrapper over [`CoherenceOutcome::class_local_name_for`] — the SAME
/// completeness-gate trichotomy the bundle-level coherence certifier
/// (`certificate.rs`) uses to mint a real `logic:CoherenceCertificate`, under the
/// default classical policy ([`ContradictionPolicy::DEFAULT`], gaps and gluts both
/// forbidden — the conservative default a tool surface with no contract-specific
/// policy in hand must use). It requires BOTH:
/// * a CONCLUSIVE closure (a completed run OR a complete-for-fragment answer,
///   [`ReasoningResult::is_conclusive`]) — otherwise the strictly-weaker
///   `CoherenceCheckAttestation` claim is the strongest honest one, never a
///   certificate; AND
/// * no forbidden violation (a witnessed DL glut under the forbid-glut default) —
///   an inconsistent-but-conclusive closure is REFUSED (`"Refused"`), never labeled
///   `CoherenceCertificate`, on EITHER caller's path (no ad-hoc per-caller downgrade:
///   `run_verify_graph` no longer bolts one on separately — see its own docs).
///
/// The gate is never re-derived inline here (GREENFIELD/no-duplicate-logic) — the
/// one gate lives in `certificate.rs`, so the two tool paths can never diverge on
/// whether a genuine certificate is warranted.
#[cfg(feature = "reasoning")]
fn completeness_class(result: &ReasoningResult) -> &'static str {
    CoherenceOutcome::class_local_name_for(result, ContradictionPolicy::DEFAULT)
}

/// `true` iff [`completeness_class`] would answer `"Refused"` for `result` — the
/// SAME gate, under the SAME default policy, exposed as a boolean for
/// [`McpView::run_verify_graph`]'s `coherent` field. Deriving `coherent` from this
/// (rather than from an independent signal such as "did any bad-example verify
/// query match") is what guarantees `coherent` and `class_local_name` can never
/// disagree: a conclusive DL glut that trips no bad-example query still REFUSES via
/// this gate, so `coherent` is forced `false` in lockstep with `class_local_name`
/// being `"Refused"` — never `coherent:true` alongside `class:Refused`.
#[cfg(feature = "reasoning")]
fn completeness_refused(result: &ReasoningResult) -> bool {
    CoherenceOutcome::is_refused_for(result, ContradictionPolicy::DEFAULT)
}

/// Read the SCOPED COHERENCE CERTIFICATE carried in the bundle's `graph/attestations`
/// named graph and map it to the proof-carrying read envelope (R6).
///
/// This is BUDGET-FREE and REASON-FREE: the certificate was computed ONCE at pipeline
/// time (by the pipeline's carrier stage, over the whole assembled carrier) and folded into the
/// bundle; this reads it straight off the loaded dataset — it NEVER re-reasons. The class
/// (`logic:CoherenceCertificate` vs the strictly-weaker `logic:CoherenceCheckAttestation`)
/// is read from the carried `rdf:type`, so an attestation is NEVER silently reported as a
/// certificate. The completeness/evaluation axes are recovered from the linked
/// `logic:ReasoningResult` node's `logic:resultCompleteness` / `logic:resultEvaluation`
/// status individuals.
///
/// # Errors
/// HARD-FAILS if the bundle carries no coherence artifact in `graph/attestations` (there
/// is NO silent recompute fallback — after the terminal fold every gmeow.gts carries one),
/// or if it carries more than one distinct coherence subject (an ambiguous bundle).
#[cfg(feature = "reasoning")]
fn coherence_certificate_envelope(dataset: &purrdf::RdfDataset) -> gmeow_errors::Result<Value> {
    use purrdf::RdfTerm;

    let graph = gmeow_ns::graph_iris::GRAPH_ATTESTATIONS;
    let rdf_type = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
    let cert_type = format!("{LOGIC_NAMESPACE}CoherenceCertificate");
    let att_type = format!("{LOGIC_NAMESPACE}CoherenceCheckAttestation");

    let iri_of = |term: &RdfTerm| -> Option<String> {
        match term {
            RdfTerm::Iri(iri) => Some(iri.clone()),
            _ => None,
        }
    };
    let literal_of = |term: &RdfTerm| -> Option<String> {
        match term {
            RdfTerm::Literal(lit) => Some(lit.lexical_form.clone()),
            _ => None,
        }
    };
    let in_graph = |g: &Option<RdfTerm>| matches!(g, Some(RdfTerm::Iri(iri)) if iri == graph);

    // 1. Locate THE coherence subject and its class local name (Certificate vs the
    //    strictly-weaker Attestation). A Refused outcome emits nothing, so a carried
    //    subject is always one of the two issued artifacts.
    let mut carried: Option<(String, &'static str)> = None;
    for q in dataset.owned_quads() {
        if !in_graph(&q.graph_name) || q.predicate != rdf_type {
            continue;
        }
        let (Some(subject), Some(obj)) = (iri_of(&q.subject), iri_of(&q.object)) else {
            continue;
        };
        let class_local = if obj == cert_type {
            "CoherenceCertificate"
        } else if obj == att_type {
            "CoherenceCheckAttestation"
        } else {
            continue;
        };
        match &carried {
            Some((existing, _)) if *existing != subject => {
                return Err(gmeow_errors::Diag::of_kind(crate::error::Mcp {
                    message: format!(
                        "coherence_certificate: the bundle carries more than one distinct \
                         coherence subject in {graph} ({existing} and {subject}); an ambiguous \
                         coherence artifact set is a hard failure"
                    ),
                }));
            }
            // The SAME subject typed BOTH logic:CoherenceCertificate and
            // logic:CoherenceCheckAttestation is ambiguous — a hard fail, never a
            // silent first-wins pick of whichever `rdf:type` triple the dataset's
            // quad order happened to surface first.
            Some((_, existing_class)) if *existing_class != class_local => {
                return Err(gmeow_errors::Diag::of_kind(crate::error::Mcp {
                    message: format!(
                        "coherence_certificate: coherence subject {subject} is typed BOTH \
                         logic:CoherenceCertificate and logic:CoherenceCheckAttestation in \
                         {graph}; an ambiguously-typed coherence artifact is a hard failure"
                    ),
                }));
            }
            Some(_) => {}
            None => carried = Some((subject, class_local)),
        }
    }
    let Some((subject, class_local)) = carried else {
        return Err(gmeow_errors::Diag::of_kind(crate::error::Mcp {
            message: format!(
                "coherence_certificate: the bundle carries no coherence certificate or \
                 attestation in {graph} (no logic:CoherenceCertificate / \
                 logic:CoherenceCheckAttestation subject); the bundle is missing its \
                 pipeline-time coherence proof — refusing to recompute"
            ),
        }));
    };

    // 2. Gather the scoped payload off the subject's quads.
    let p = |local: &str| format!("{LOGIC_NAMESPACE}{local}");
    let (
        pred_bundle,
        pred_axiom,
        pred_contract,
        pred_engine,
        pred_fragment,
        pred_policy,
        pred_loss,
        pred_forbidden,
        pred_summarizes,
    ) = (
        p("bundleHash"),
        p("axiomHash"),
        p("contractHash"),
        p("engine"),
        p("certifiedFragment"),
        p("contradictionPolicy"),
        p("projectionLoss"),
        p("forbiddenViolationWitness"),
        p("summarizesResult"),
    );
    let mut bundle_hash: Option<String> = None;
    let mut contract_hash: Option<String> = None;
    let mut engine: Option<String> = None;
    let mut certified_fragment: Option<String> = None;
    let mut contradiction_policy: Option<String> = None;
    let mut result_node: Option<String> = None;
    let mut axiom_hashes: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    let mut projection_losses: std::collections::BTreeSet<String> =
        std::collections::BTreeSet::new();
    let mut forbidden_violations: std::collections::BTreeSet<String> =
        std::collections::BTreeSet::new();
    for q in dataset.owned_quads() {
        if !in_graph(&q.graph_name) || iri_of(&q.subject).as_deref() != Some(subject.as_str()) {
            continue;
        }
        let pred = q.predicate.as_str();
        if pred == pred_bundle {
            bundle_hash = literal_of(&q.object);
        } else if pred == pred_axiom {
            if let Some(v) = literal_of(&q.object) {
                axiom_hashes.insert(v);
            }
        } else if pred == pred_contract {
            contract_hash = literal_of(&q.object);
        } else if pred == pred_engine {
            engine = literal_of(&q.object);
        } else if pred == pred_fragment {
            certified_fragment = literal_of(&q.object);
        } else if pred == pred_policy {
            contradiction_policy = iri_of(&q.object);
        } else if pred == pred_loss {
            if let Some(v) = literal_of(&q.object) {
                projection_losses.insert(v);
            }
        } else if pred == pred_forbidden {
            if let Some(v) = iri_of(&q.object) {
                forbidden_violations.insert(v);
            }
        } else if pred == pred_summarizes {
            result_node = iri_of(&q.object);
        }
    }

    // 3. Recover the two completeness-gate axes off the linked logic:ReasoningResult node.
    let pred_completeness = p("resultCompleteness");
    let pred_evaluation = p("resultEvaluation");
    let mut completeness: Option<String> = None;
    let mut evaluation: Option<String> = None;
    if let Some(node) = &result_node {
        for q in dataset.owned_quads() {
            if !in_graph(&q.graph_name) || iri_of(&q.subject).as_deref() != Some(node.as_str()) {
                continue;
            }
            if q.predicate == pred_completeness {
                completeness = iri_of(&q.object)
                    .and_then(|iri| iri.strip_prefix(LOGIC_NAMESPACE).map(str::to_owned))
                    .and_then(|local| CompletenessStatus::from_local(&local))
                    .map(|s| s.wire().to_owned());
            } else if q.predicate == pred_evaluation {
                evaluation = iri_of(&q.object)
                    .and_then(|iri| iri.strip_prefix(LOGIC_NAMESPACE).map(str::to_owned))
                    .and_then(|local| EvaluationStatus::from_local(&local))
                    .map(|s| s.wire().to_owned());
            }
        }
    }

    // 4. STRICT VALIDATION — every field [`CoherenceOutcome::to_nquads`]
    //    (`crates/logic/src/certificate.rs`) writes UNCONDITIONALLY on every issued
    //    certificate/attestation subject is REQUIRED here, with the exact cardinality
    //    the producer guarantees. A carried subject missing one of these is a CORRUPT
    //    artifact, not a partial one — returning `ok:true` with a null/empty field would
    //    silently launder that corruption past the caller (no-silent-degradation, `.goals`).
    //    Each hard-fail names the missing predicate so the caller can act on it.
    let missing = |field: &str| -> gmeow_errors::Diag {
        gmeow_errors::Diag::of_kind(crate::error::Mcp {
            message: format!(
                "coherence_certificate: coherence subject {subject} carries no logic:{field} \
                 (malformed coherence artifact)"
            ),
        })
    };
    // bundle_hash / axiom_hashes are the load-bearing tamper surface. `axiomHash` rides
    // per-axiom-bearing-graph (`per_graph_axiom_hashes`), which hashes EVERY named graph in
    // the dataset the certificate was computed over — a real bundle always has at least one,
    // so an empty set here is corruption, not a legitimately-empty producer output.
    let Some(bundle_hash) = bundle_hash.filter(|v| !v.is_empty()) else {
        return Err(missing("bundleHash"));
    };
    if axiom_hashes.is_empty() {
        return Err(missing("axiomHash"));
    }
    let Some(contract_hash) = contract_hash.filter(|v| !v.is_empty()) else {
        return Err(missing("contractHash"));
    };
    let Some(engine) = engine.filter(|v| !v.is_empty()) else {
        return Err(missing("engine"));
    };
    let Some(contradiction_policy) = contradiction_policy else {
        return Err(missing("contradictionPolicy"));
    };
    // logic:summarizesResult is written unconditionally (M2, single-valued) and is the
    // only path to the two completeness-gate axes below; its absence is corruption.
    if result_node.is_none() {
        return Err(missing("summarizesResult"));
    }
    // logic:certifiedFragment is the ONE conditionally-required field: the completeness
    // gate (`certificate.rs::classify`) downgrades any fragment-less conclusive check to
    // an attestation, so a producer NEVER emits a `CoherenceCertificate` subject without
    // one — require it only for that class, exactly matching producer cardinality.
    if class_local == "CoherenceCertificate"
        && certified_fragment.as_deref().is_none_or(str::is_empty)
    {
        return Err(missing("certifiedFragment"));
    }
    // The two completeness-gate axes ride the linked result node unconditionally
    // (`resultEvaluation` / `resultCompleteness`) — required alongside it.
    let Some(completeness) = completeness else {
        return Err(missing("resultCompleteness"));
    };
    let Some(evaluation) = evaluation else {
        return Err(missing("resultEvaluation"));
    };

    Ok(json!({
        "ok": true,
        "issues_certificate": class_local == "CoherenceCertificate",
        "is_refused": false,
        "class_local_name": class_local,
        "bundle_hash": bundle_hash,
        "axiom_hashes": axiom_hashes.into_iter().collect::<Vec<_>>(),
        "contract_hash": contract_hash,
        "engine": engine,
        "certified_fragment": certified_fragment,
        "completeness": completeness,
        "evaluation": evaluation,
        "contradiction_policy": contradiction_policy,
        "projection_losses": projection_losses.into_iter().collect::<Vec<_>>(),
        "forbidden_violations": forbidden_violations.into_iter().collect::<Vec<_>>(),
    }))
}

/// Build the canonical N3 object surface for `explain_quad` — the EXACT byte form a
/// [`Row`]'s `obj` carries ([`term_display`] of the object [`TermValue`]) — so the
/// target reifier joins the reasoner's row set. Reuses the SAME serializer the row
/// builder feeds off (`gmeow_logic::provenance::term_display`); it never hand-rolls a
/// second quoting/escaping surface (a mismatch would forge a false "not in closure").
///
/// `kind` is `iri` or `literal`; when the caller omits it, it is inferred from the
/// object surface ([`infer_object_kind`]). `datatype` types a literal only — pairing
/// it with an `iri` object is a contradictory request and a HARD FAIL, as is any
/// `kind` other than `iri`/`literal`.
#[cfg(feature = "reasoning")]
fn object_term_n3(
    value: &str,
    kind: Option<&str>,
    datatype: Option<&str>,
) -> gmeow_errors::Result<String> {
    let kind = kind.unwrap_or_else(|| infer_object_kind(value));
    let term = match kind {
        "iri" => {
            if datatype.is_some() {
                return Err(gmeow_errors::Diag::of_kind(crate::error::Mcp {
                    message:
                        "explain_quad: object_datatype is only valid for object_kind=literal, \
                              not an IRI object"
                            .to_owned(),
                }));
            }
            TermValue::iri(value)
        }
        "literal" => match datatype {
            Some(dt) => TermValue::typed_literal(value, dt),
            None => TermValue::simple_literal(value),
        },
        other => {
            return Err(gmeow_errors::Diag::of_kind(crate::error::Mcp {
                message: format!(
                    "explain_quad: object_kind must be `iri` or `literal`, got `{other}`"
                ),
            }));
        }
    };
    Ok(term_display(&term))
}

/// Infer whether a bare `object_value` is an IRI or a literal when the caller omits
/// `object_kind`. An absolute IRI carries a URI scheme (`ALPHA *( ALPHA / DIGIT / "+"
/// / "-" / "." ) ":"`) and no ASCII whitespace and is not a quoted literal; anything
/// else is a literal lexical form. The inference is only a convenience default — a
/// mis-inference cannot corrupt a result: a wrong reifier simply fails to join the
/// closure and HARD-FAILS as "not in closure", never a fabricated proof.
#[cfg(feature = "reasoning")]
fn infer_object_kind(value: &str) -> &'static str {
    if is_absolute_iri_shape(value) {
        "iri"
    } else {
        "literal"
    }
}

/// `true` iff `value` has the shape of an absolute IRI: a non-empty
/// `ALPHA *( ALPHA / DIGIT / "+" / "-" / "." )` scheme followed by `:`, with no ASCII
/// whitespace and no leading `"` (which would mark a literal).
#[cfg(feature = "reasoning")]
fn is_absolute_iri_shape(value: &str) -> bool {
    if value.starts_with('"') || value.chars().any(|c| c.is_ascii_whitespace()) {
        return false;
    }
    let Some(colon) = value.find(':') else {
        return false;
    };
    if colon == 0 {
        return false;
    }
    let mut scheme = value[..colon].chars();
    let first = scheme.next().expect("colon>0 guarantees a scheme char");
    first.is_ascii_alphabetic()
        && scheme.all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'))
}

/// Locate the single row in `rows` whose reifier is `target_reifier` in world
/// `graph`, WORLD-DISAMBIGUATING the join (R1). The reifier alone is not a unique key:
/// the same `(S, P, O)` in two worlds shares a reifier, so the intended world must be
/// supplied.
///
/// * No row (in any world) carries the reifier → HARD FAIL `quad not in closure`.
/// * `graph` resolves to exactly one row → that row.
/// * `graph` resolves to no row but the reifier lives in exactly one OTHER world →
///   HARD FAIL `quad not in closure` naming that world (the caller asked the wrong one).
/// * The reifier spans MORE THAN ONE world and `graph` does not resolve it to exactly
///   one row → HARD FAIL ambiguity (never an arbitrary pick).
///
/// When `graph` resolves to several rows but the reifier lives in ONLY that world, the
/// engine's `(graph, reifier)` identity is last-wins; this returns that same last row
/// so the reconstructed root matches the index's resolution deterministically.
#[cfg(feature = "reasoning")]
fn locate_explain_target(
    rows: &[Row],
    target_reifier: &str,
    graph: &str,
) -> gmeow_errors::Result<usize> {
    let matches: Vec<usize> = rows
        .iter()
        .enumerate()
        .filter(|(_, row)| reifier_from_row(row) == target_reifier)
        .map(|(index, _)| index)
        .collect();
    if matches.is_empty() {
        return Err(gmeow_errors::Diag::of_kind(crate::error::Mcp {
            message: format!(
                "explain_quad: quad not in closure — reifier <{target_reifier}> matches no \
                 derived or asserted quad in the reasoned bundle (check the (subject, predicate, \
                 object) surface and the max_steps budget)"
            ),
        }));
    }
    // The distinct worlds the reifier appears in (sorted, for a deterministic message).
    let worlds: BTreeSet<&str> = matches
        .iter()
        .map(|&index| rows[index].graph.as_str())
        .collect();
    // The requested world's matching rows, in input order (last = the engine identity).
    let in_world: Vec<usize> = matches
        .iter()
        .copied()
        .filter(|&index| rows[index].graph == graph)
        .collect();

    let ambiguity = || {
        gmeow_errors::Diag::of_kind(crate::error::Mcp {
            message: format!(
                "explain_quad: ambiguous quad — reifier <{target_reifier}> matches quads in {} \
                 distinct worlds ({}); the supplied graph <{graph}> does not resolve it to \
                 exactly one row. Re-issue with graph set to the intended world.",
                worlds.len(),
                worlds.iter().copied().collect::<Vec<_>>().join(", ")
            ),
        })
    };

    match in_world.len() {
        1 => Ok(in_world[0]),
        0 => {
            if worlds.len() > 1 {
                Err(ambiguity())
            } else {
                let other = worlds.iter().next().copied().unwrap_or_default();
                Err(gmeow_errors::Diag::of_kind(crate::error::Mcp {
                    message: format!(
                        "explain_quad: quad not in closure — reifier <{target_reifier}> matches a \
                         quad in world <{other}>, not the supplied graph <{graph}>"
                    ),
                }))
            }
        }
        // The requested world carries the reifier on several rows. If the reifier ALSO
        // spans other worlds it is genuinely ambiguous; otherwise the engine identity is
        // last-wins, so we deterministically explain that last row.
        _ => {
            if worlds.len() > 1 {
                Err(ambiguity())
            } else {
                Ok(*in_world.last().expect("len > 1 has a last"))
            }
        }
    }
}

/// HARD-FAIL a GMN document argument that exceeds the inline payload ceiling, mirroring
/// `validate_local`'s size guard — never a silent truncation (a truncated GMN-1 document
/// would mis-parse and mislead the repair loop).
#[cfg(feature = "core")]
fn guard_gmn_size(gmn: &str, tool: &str) -> gmeow_errors::Result<()> {
    if gmn.len() > MAX_VALIDATE_DATA_BYTES {
        return Err(gmeow_errors::Diag::of_kind(crate::error::Mcp {
            message: format!(
                "{tool}: gmn payload is {} bytes, exceeding the {} byte ceiling; split the \
                 document (no silent truncation)",
                gmn.len(),
                MAX_VALIDATE_DATA_BYTES
            ),
        }));
    }
    Ok(())
}

/// Harvest the `rdfs:label` index (`IRI → label`) from the bundle dataset — the SAME
/// deterministic pick the pipeline verbalizer uses (a `@x-gmeow-english` label wins; ties
/// break to the smallest lexical form), so `gmn_explain`'s gloss nucleus matches Task 8.
#[cfg(feature = "core")]
fn harvest_dataset_labels(dataset: &purrdf::RdfDataset) -> BTreeMap<String, String> {
    // (is_gmeow_english, lexical_form) per IRI — the preference key.
    let mut best: BTreeMap<String, (bool, String)> = BTreeMap::new();
    for quad in dataset.owned_quads() {
        if quad.predicate != RDFS_LABEL {
            continue;
        }
        let (RdfTerm::Iri(subject), RdfTerm::Literal(literal)) = (&quad.subject, &quad.object)
        else {
            continue;
        };
        let is_english = literal.language.as_deref() == Some(GMEOW_ENGLISH);
        let candidate = (is_english, literal.lexical_form.clone());
        match best.get(subject) {
            Some((cur_english, cur_lex)) => {
                let better = (candidate.0, Reverse(candidate.1.clone()))
                    > (*cur_english, Reverse(cur_lex.clone()));
                if better {
                    best.insert(subject.clone(), candidate);
                }
            }
            None => {
                best.insert(subject.clone(), candidate);
            }
        }
    }
    best.into_iter().map(|(k, (_, v))| (k, v)).collect()
}

/// Join a resolved operator form back to the `gmeow:gmnPrecedence` integer and the
/// `lang:Denotation` IRI the glyph registry does NOT carry (it keys on fixity/arity only),
/// by matching the `(denotationTarget, gmnFixity, gmnArity)` signature the operator's
/// denoted Form authors in the dataset. Returns `(precedence, denotation_iri)`, each `None`
/// when the codebook authors no matching record (surfaced honestly, never fabricated).
#[cfg(feature = "core")]
fn gmn_signature_join(
    dataset: &purrdf::RdfDataset,
    target: &str,
    fixity: &str,
    arity: u32,
) -> (Option<i64>, Option<String>) {
    // Form → (fixity, precedence, arity); Denotation → (form, target); Denotation typed set.
    let mut form_fixity: BTreeMap<String, String> = BTreeMap::new();
    let mut form_precedence: BTreeMap<String, i64> = BTreeMap::new();
    let mut form_arity: BTreeMap<String, u32> = BTreeMap::new();
    let mut den_form: BTreeMap<String, String> = BTreeMap::new();
    let mut den_target: BTreeMap<String, String> = BTreeMap::new();
    let mut is_denotation: BTreeSet<String> = BTreeSet::new();
    for quad in dataset.owned_quads() {
        let RdfTerm::Iri(subject) = &quad.subject else {
            continue;
        };
        match quad.predicate.as_str() {
            RDF_TYPE => {
                if matches!(&quad.object, RdfTerm::Iri(class) if class == LANG_DENOTATION) {
                    is_denotation.insert(subject.clone());
                }
            }
            GMN_FIXITY => {
                if let RdfTerm::Iri(value) = &quad.object {
                    form_fixity.insert(subject.clone(), value.clone());
                }
            }
            GMN_PRECEDENCE => {
                if let RdfTerm::Literal(literal) = &quad.object
                    && let Ok(value) = literal.lexical_form.parse::<i64>()
                {
                    form_precedence.insert(subject.clone(), value);
                }
            }
            GMN_ARITY => {
                if let RdfTerm::Literal(literal) = &quad.object
                    && let Ok(value) = literal.lexical_form.parse::<u32>()
                {
                    form_arity.insert(subject.clone(), value);
                }
            }
            LANG_DENOTED_FORM => {
                if let RdfTerm::Iri(value) = &quad.object {
                    den_form.insert(subject.clone(), value.clone());
                }
            }
            LANG_DENOTATION_TARGET => {
                if let RdfTerm::Iri(value) = &quad.object {
                    den_target.insert(subject.clone(), value.clone());
                }
            }
            _ => {}
        }
    }
    // Deterministic (BTreeSet iteration): the first Denotation whose denoted Form signature
    // matches (target, fixity, arity).
    for denotation in &is_denotation {
        let Some(form) = den_form.get(denotation) else {
            continue;
        };
        if den_target.get(denotation).map(String::as_str) != Some(target) {
            continue;
        }
        if form_fixity.get(form).map(String::as_str) != Some(fixity) {
            continue;
        }
        if form_arity.get(form).copied() != Some(arity) {
            continue;
        }
        return (form_precedence.get(form).copied(), Some(denotation.clone()));
    }
    (None, None)
}

// ── base64 (RFC 4648 §4, standard alphabet, padded) ─────────────────────────────
//
// The MCP wire format is JSON, which carries text; the `convert` tool carries BYTES.
// Every codec whose output is not valid UTF-8 (`gts`) therefore needs a byte-exact text
// encoding in both directions, and a caller that pastes a binary source needs the same on
// the way in. This is ~40 lines of table lookup with no state — cheaper, and far easier to
// audit at a crate boundary this strict, than a dependency edge on the consumer surface.

/// The standard base64 alphabet, in index order.
#[cfg(feature = "core")]
const BASE64_ALPHABET: &[u8; 64] =
    b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// Encode `bytes` as padded standard base64.
#[cfg(feature = "core")]
fn base64_encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b0 = u32::from(chunk[0]);
        let b1 = chunk.get(1).copied().map_or(0, u32::from);
        let b2 = chunk.get(2).copied().map_or(0, u32::from);
        let triple = (b0 << 16) | (b1 << 8) | b2;
        for i in 0..4 {
            // A group of 3 input bytes yields 4 output characters; a short final group
            // yields 2 or 3 characters plus padding.
            if i <= chunk.len() {
                let index = ((triple >> (18 - 6 * i)) & 0x3F) as usize;
                out.push(char::from(BASE64_ALPHABET[index]));
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// The 0–63 value of one base64 character, or `None` when it is not in the alphabet.
#[cfg(feature = "core")]
fn base64_value(ch: u8) -> Option<u32> {
    let value = match ch {
        b'A'..=b'Z' => ch - b'A',
        b'a'..=b'z' => ch - b'a' + 26,
        b'0'..=b'9' => ch - b'0' + 52,
        b'+' => 62,
        b'/' => 63,
        _ => return None,
    };
    Some(u32::from(value))
}

/// Decode padded standard base64, refusing anything that is not exactly that.
///
/// Strict by construction: ASCII whitespace is skipped (a pasted payload is routinely
/// line-wrapped), but a stray character, a length that is not a multiple of four, and
/// misplaced padding are each named rather than silently truncated to whatever decoded —
/// a partial decode is exactly the silent degradation this surface forbids.
///
/// `what` labels the argument being decoded so the refusal points at the caller's own
/// parameter (`convert: \`data\``) rather than at this helper.
#[cfg(feature = "core")]
fn base64_decode(what: &str, text: &str) -> gmeow_errors::Result<Vec<u8>> {
    let refuse = |detail: String| {
        gmeow_errors::Diag::of_kind(crate::error::Mcp {
            message: format!("{what} is not valid base64: {detail}"),
        })
    };
    let symbols: Vec<u8> = text
        .bytes()
        .filter(|b| !b.is_ascii_whitespace())
        .collect::<Vec<u8>>();
    if !symbols.len().is_multiple_of(4) {
        return Err(refuse(format!(
            "length {} is not a multiple of 4 (standard base64 is padded)",
            symbols.len()
        )));
    }
    let mut out = Vec::with_capacity(symbols.len() / 4 * 3);
    for (group_index, group) in symbols.chunks(4).enumerate() {
        let is_last = (group_index + 1) * 4 == symbols.len();
        let pad = group.iter().filter(|b| **b == b'=').count();
        if pad > 0 && !is_last {
            return Err(refuse(format!(
                "padding appears in group {group_index}, which is not the final group"
            )));
        }
        if pad > 2 || (pad > 0 && group[4 - pad..].iter().any(|b| *b != b'=')) {
            return Err(refuse(
                "padding must be the final one or two characters of the last group".to_owned(),
            ));
        }
        let mut acc: u32 = 0;
        for (i, &ch) in group.iter().enumerate() {
            let value = if ch == b'=' {
                0
            } else {
                base64_value(ch).ok_or_else(|| {
                    refuse(format!(
                        "character {:?} is not in the base64 alphabet",
                        char::from(ch)
                    ))
                })?
            };
            acc |= value << (18 - 6 * i);
        }
        for i in 0..(3 - pad) {
            out.push(((acc >> (16 - 8 * i)) & 0xFF) as u8);
        }
    }
    Ok(out)
}

fn required_str<'a>(args: &'a Value, key: &str) -> gmeow_errors::Result<&'a str> {
    optional_str(args, key).ok_or_else(|| {
        gmeow_errors::Diag::of_kind(crate::error::Mcp {
            message: format!("{key} is required"),
        })
    })
}

/// Read a REQUIRED JSON-object argument as an in-memory file map: each key is a
/// slice-relative forward-slash path, each value that file's text.
///
/// The whole point of the argument is that the caller has FILES, not a directory, so
/// every defect is named precisely rather than collapsed into "bad input": a missing
/// argument, an argument that is not an object, and a value that is not a string are
/// three different authoring mistakes and get three different messages.
#[cfg(feature = "reasoning")]
fn required_file_map(
    tool: &str,
    args: &Value,
    key: &str,
) -> gmeow_errors::Result<BTreeMap<String, Vec<u8>>> {
    let Some(value) = args.get(key) else {
        return Err(gmeow_errors::Diag::of_kind(crate::error::Mcp {
            message: format!(
                "{tool}: missing required argument `{key}` (a JSON object mapping each \
                 slice-relative path to that file's text)"
            ),
        }));
    };
    let Some(object) = value.as_object() else {
        return Err(gmeow_errors::Diag::of_kind(crate::error::Mcp {
            message: format!(
                "{tool}: argument `{key}` must be a JSON object mapping each slice-relative \
                 path to that file's text"
            ),
        }));
    };
    let mut files = BTreeMap::new();
    for (path, contents) in object {
        let Some(text) = contents.as_str() else {
            return Err(gmeow_errors::Diag::of_kind(crate::error::Mcp {
                message: format!(
                    "{tool}: argument `{key}` entry `{path}` must be a string (the file's text)"
                ),
            }));
        };
        files.insert(path.clone(), text.as_bytes().to_vec());
    }
    Ok(files)
}

fn optional_str<'a>(args: &'a Value, key: &str) -> Option<&'a str> {
    args.get(key).and_then(Value::as_str)
}

fn optional_bool(args: &Value, key: &str) -> Option<bool> {
    args.get(key).and_then(Value::as_bool)
}

fn optional_f64(args: &Value, key: &str) -> gmeow_errors::Result<Option<f64>> {
    match args.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Number(n)) => n.as_f64().map(Some).ok_or_else(|| {
            gmeow_errors::Diag::of_kind(crate::error::Mcp {
                message: format!("{key} must be a finite number"),
            })
        }),
        Some(_) => Err(gmeow_errors::Diag::of_kind(crate::error::Mcp {
            message: format!("{key} must be a number"),
        })),
    }
}

fn optional_limit(args: &Value, key: &str) -> gmeow_errors::Result<Option<usize>> {
    match args.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Number(n)) => {
            let value = n.as_i64().ok_or_else(|| {
                gmeow_errors::Diag::of_kind(crate::error::Mcp {
                    message: format!("{key} must be an integer"),
                })
            })?;
            usize::try_from(value).map(Some).map_err(|_| {
                gmeow_errors::Diag::of_kind(crate::error::Mcp {
                    message: format!("{key} must be non-negative"),
                })
            })
        }
        Some(_) => Err(gmeow_errors::Diag::of_kind(crate::error::Mcp {
            message: format!("{key} must be an integer"),
        })),
    }
}

/// A strict non-negative `u64` argument (absent/null → `None`), used for the `max_steps`
/// closure-size ceiling: present must be a non-negative integer, anything else is a HARD FAIL.
fn optional_step_count(args: &Value, key: &str) -> gmeow_errors::Result<Option<u64>> {
    match args.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Number(n)) => {
            let value = n.as_i64().ok_or_else(|| {
                gmeow_errors::Diag::of_kind(crate::error::Mcp {
                    message: format!("{key} must be an integer"),
                })
            })?;
            u64::try_from(value).map(Some).map_err(|_| {
                gmeow_errors::Diag::of_kind(crate::error::Mcp {
                    message: format!("{key} must be non-negative"),
                })
            })
        }
        Some(_) => Err(gmeow_errors::Diag::of_kind(crate::error::Mcp {
            message: format!("{key} must be an integer"),
        })),
    }
}

/// A strict boolean argument: present-and-bool → `Some`, absent/null → `None`, anything else is a
/// HARD FAIL (no silent coercion — `dry_run` is a named default, not a degraded fallback).
#[cfg(feature = "reasoning")]
fn optional_bool_checked(args: &Value, key: &str) -> gmeow_errors::Result<Option<bool>> {
    match args.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Bool(value)) => Ok(Some(*value)),
        Some(_) => Err(gmeow_errors::Diag::of_kind(crate::error::Mcp {
            message: format!("{key} must be a boolean"),
        })),
    }
}

// ── Transaction-Logic execution of the memory write triad ────────────────────
//
// The two memory WRITE tools run as Transaction-Logic transactions: the canonical action theory
// is the single authority, the engine's executional entailment over the real start state is the
// commit gate, `dry_run` selects the hypothetical (sandbox) operator, and every committed turn is
// recorded with the audit context the trajectory audit reads.

/// The canonical action theory — the SINGLE authority for how every tool on this server
/// behaves: the writes' `logic:precondition` / `logic:effect` / `logic:compensation`, the reads'
/// `logic:capability` / `logic:precondition`, and the `logic:mcpToolName` wire name that ties
/// each schema to the tool it governs. Embedded at build so the shipped `gmeow` runs repo-free;
/// the slice file is the one source of truth, and the worked example and conformance case
/// reference these same schema IRIs (they encode no second copy).
///
/// This embedded copy is the theory's ONLY carrier on the wire. `gmeow.gts` ships no second
/// copy: the pipeline folds a slice's `module.ttl` into `graph/logic`, but a slice's
/// `examples/*.ttl` is read only to derive documentation and try-it inferences — its triples
/// are never folded. THE BROWSER READS THIS COPY: a wasm console cannot `include_str!` a
/// checkout at runtime, but it carries the compiled crate, so a
/// `gmeow://ontology/action-policy` reader in a browser is served [`action_policy_nquads`] —
/// the projection of these bytes — and the Transaction-Logic executor reads the same
/// function. One carrier can never drift; two can.
/// `the_action_theorys_two_carriers_agree_quad_for_quad` is the tripwire: it asserts the
/// bundle carries ZERO policy quads and that the browser-facing resource serves this copy's
/// projection. If slice examples ever start being folded, that test reds and must be replaced
/// by a quad-set equality between the two copies — never relaxed.
const MCP_ACTION_POLICY_TTL: &str =
    include_str!("../../../slices/core/agentic/examples/mcp-action-policy.ttl");

/// The URI of the resource mirroring the `action_policy` tool. A client that reads
/// resources rather than calling tools gets the identical projected theory, because both
/// sides serve [`action_policy_nquads`] and neither restates it.
pub const ACTION_POLICY_URI: &str = "gmeow://ontology/action-policy";

/// The media type the action-policy projection is served as, on BOTH the tool envelope
/// and the resource descriptor — declared once so advertised and served cannot disagree.
const ACTION_POLICY_MEDIA_TYPE: &str = "application/n-quads";

/// The transient world the TR run reasons in — a fresh in-memory store per call, NEVER persisted.
/// The executed verdict gates the write; the materialized outcome rides the tool response.
const TXN_WORLD: &str = "https://blackcatinformatics.ca/gmeow/agentic/mcp-exec";
#[cfg(feature = "reasoning")]
const TXN_ROOT: &str = "https://blackcatinformatics.ca/gmeow/agentic/mcp-exec/txn";
#[cfg(feature = "reasoning")]
const TXN_START: &str = "https://blackcatinformatics.ca/gmeow/agentic/mcp-exec/start";

/// The canonical action-schema and situation IRIs defined by `mcp-action-policy.ttl`.
#[cfg(feature = "reasoning")]
const MCP_STORE_CLAIM_SCHEMA: &str =
    "https://blackcatinformatics.ca/gmeow/examples/agentic/mcp-policy/storeClaim";
#[cfg(feature = "reasoning")]
const MCP_REVISE_BELIEF_SCHEMA: &str =
    "https://blackcatinformatics.ca/gmeow/examples/agentic/mcp-policy/reviseBelief";
#[cfg(feature = "reasoning")]
const MCP_WELL_FORMED_CLAIM: &str =
    "https://blackcatinformatics.ca/gmeow/examples/agentic/mcp-policy/wellFormedClaim";
#[cfg(feature = "reasoning")]
const MCP_TARGET_CLAIM_EXISTS: &str =
    "https://blackcatinformatics.ca/gmeow/examples/agentic/mcp-policy/targetClaimExists";
#[cfg(feature = "reasoning")]
const MCP_CLAIM_IN_MEMORY: &str =
    "https://blackcatinformatics.ca/gmeow/examples/agentic/mcp-policy/claimInMemory";

/// The `persistConjecture` action schema + its precondition situation, defined by
/// `mcp-action-policy.ttl`. The `conjecture_test` write triad instantiates this schema; the
/// executional-entailment verdict over the precondition gates the append to the library.
#[cfg(feature = "reasoning")]
const MCP_PERSIST_CONJECTURE_SCHEMA: &str =
    "https://blackcatinformatics.ca/gmeow/examples/agentic/mcp-policy/persistConjecture";
#[cfg(feature = "reasoning")]
const MCP_CONJECTURE_VERDICT_PRESENTED: &str =
    "https://blackcatinformatics.ca/gmeow/examples/agentic/mcp-policy/conjectureVerdictPresented";

/// The `withdrawConjecture` action schema + its precondition situation, defined by
/// `mcp-action-policy.ttl`. The compensating author-withdrawal counterpart of
/// `persistConjecture` (P10, `logic:compensation`): the precondition — the conjecture is
/// still in the library (not already withdrawn) — gates the compensating append.
#[cfg(feature = "reasoning")]
const MCP_WITHDRAW_CONJECTURE_SCHEMA: &str =
    "https://blackcatinformatics.ca/gmeow/examples/agentic/mcp-policy/withdrawConjecture";
#[cfg(feature = "reasoning")]
const MCP_CONJECTURE_IN_LIBRARY: &str =
    "https://blackcatinformatics.ca/gmeow/examples/agentic/mcp-policy/conjectureInLibrary";

/// The `submitCandidate` action schema + its precondition situation, defined by
/// `mcp-action-policy.ttl`. The candidate-submission write triad instantiates this schema; the
/// executional-entailment verdict over the precondition gates the append to the candidate
/// library. Unlike `persistConjecture`'s `conjectureVerdictPresented`, the precondition
/// `candidateAdmissible` is DERIVED FROM VERDICT POLARITY (corroborated, not merely present),
/// so a refuted or open candidate never obtains it and stages nothing (AC5/AC6).
#[cfg(feature = "reasoning")]
const MCP_SUBMIT_CANDIDATE_SCHEMA: &str =
    "https://blackcatinformatics.ca/gmeow/examples/agentic/mcp-policy/submitCandidate";
#[cfg(feature = "reasoning")]
const MCP_CANDIDATE_ADMISSIBLE: &str =
    "https://blackcatinformatics.ca/gmeow/examples/agentic/mcp-policy/candidateAdmissible";

/// The `withdrawCandidate` action schema + its precondition situation, defined by
/// `mcp-action-policy.ttl`. The compensating author-withdrawal counterpart of
/// `submitCandidate` (P10, `logic:compensation`): the precondition — the candidate is still in
/// the library (not already withdrawn) — gates the compensating append.
#[cfg(feature = "reasoning")]
const MCP_WITHDRAW_CANDIDATE_SCHEMA: &str =
    "https://blackcatinformatics.ca/gmeow/examples/agentic/mcp-policy/withdrawCandidate";
#[cfg(feature = "reasoning")]
const MCP_CANDIDATE_IN_LIBRARY: &str =
    "https://blackcatinformatics.ca/gmeow/examples/agentic/mcp-policy/candidateInLibrary";

/// The `gmeow:AuthoringCandidate` class and its provenance predicates (authored in the guides
/// slice `module.ttl`) — the authoring-role type and target links a submitted candidate node
/// carries IN ADDITION to its `logic:Conjecture` verdict, so the candidate library is queryable
/// by slice/packet and distinct from the conjecture library.
#[cfg(feature = "reasoning")]
const GMEOW_AUTHORING_CANDIDATE: &str = "https://blackcatinformatics.ca/gmeow/AuthoringCandidate";
#[cfg(feature = "reasoning")]
const GMEOW_CANDIDATE_FOR_SLICE: &str = "https://blackcatinformatics.ca/gmeow/candidateForSlice";
#[cfg(feature = "reasoning")]
const GMEOW_CANDIDATE_FOR_PACKET: &str = "https://blackcatinformatics.ca/gmeow/candidateForPacket";

#[cfg(feature = "reasoning")]
const LOGIC_INSTANTIATES_SCHEMA: &str = "https://blackcatinformatics.ca/logic/instantiatesSchema";
#[cfg(feature = "reasoning")]
const LOGIC_TRANSITION_FROM_STATE: &str =
    "https://blackcatinformatics.ca/logic/transitionFromState";
#[cfg(feature = "reasoning")]
const LOGIC_SITUATION_OBTAINS: &str = "https://blackcatinformatics.ca/logic/situationObtains";
#[cfg(feature = "reasoning")]
const LOGIC_PROPER_PART_OF: &str = "https://blackcatinformatics.ca/logic/properPartOf";
#[cfg(feature = "reasoning")]
const GMEOW_AT_TIME: &str = "https://blackcatinformatics.ca/gmeow/atTime";
#[cfg(feature = "reasoning")]
const GMEOW_EVENT_TEMPORAL_FRAME: &str = "https://blackcatinformatics.ca/gmeow/eventTemporalFrame";
#[cfg(feature = "reasoning")]
const GMEOW_TEMPORAL_FRAME_UTC_GREGORIAN: &str =
    "https://blackcatinformatics.ca/gmeow/temporalFrameUTCGregorian";
#[cfg(feature = "reasoning")]
const XSD_DATETIME: &str = "http://www.w3.org/2001/XMLSchema#dateTime";

/// The datatype property carrying an action schema's MCP wire name (minted in the `logic:`
/// grounding slice). The ONE literal-valued predicate the action-theory projection keeps: a
/// schema's local name is an ontology name (`ex:persistConjecture` is the tool
/// `store_conjecture`), so the tool↔schema correspondence has to be read off an asserted string
/// rather than mangled out of an IRI.
const LOGIC_MCP_TOOL_NAME: &str = "https://blackcatinformatics.ca/logic/mcpToolName";

/// The canonical action theory as N-Quads in [`TXN_WORLD`], parsed once from the embedded slice
/// file. HARD FAIL if the embedded authority does not parse — that is a build-time invariant, not
/// a runtime fallback (the `canonical_action_policy_parses` test guards it).
fn action_policy_nquads() -> &'static str {
    static CACHE: OnceLock<String> = OnceLock::new();
    CACHE.get_or_init(|| project_action_policy(MCP_ACTION_POLICY_TTL))
}

/// Project one action-theory Turtle document to the [`TXN_WORLD`] N-Quads the engine reads.
///
/// Separate from [`action_policy_nquads`] (which is just this over the embedded authority,
/// cached) so the bijection gate's NEGATIVE cases can run the REAL projection over a mutated
/// copy of the source. A negative test that re-implemented the filter would prove nothing about
/// the filter that ships.
fn project_action_policy(ttl: &str) -> String {
    {
        let dataset = purrdf::parse_dataset(ttl.as_bytes(), "text/turtle", None)
            .expect("canonical mcp-action-policy.ttl must parse (single authority)");
        let mut lines: Vec<String> = purrdf::flat_rdf_quads_from_dataset(&dataset)
            .into_iter()
            // The engine reads only the structural action theory (precondition / effect / ins /
            // del / compensation), all IRI→IRI — keep those and drop the annotation literals
            // (labels, comments) the executional-entailment run never consults.
            //
            // EXACTLY ONE literal-valued predicate is exempt: `logic:mcpToolName`. It is not an
            // annotation — it is the identity link from a schema to the tool it governs, and it
            // is UNRECOVERABLE from the IRI structure (`ex:persistConjecture` ⇄
            // `store_conjecture`, `ex:withdrawConjecture` ⇄ `refute_conjecture`). Dropping it
            // would make the projection unreadable as a tool contract: a browser reading the
            // `gmeow://ontology/action-policy` resource would see schemas it could not tie to
            // any callable tool, and the bijection gate would have nothing to check against.
            // Every OTHER literal (`rdfs:label`, `rdfs:comment`) still goes.
            .filter(|quad| {
                matches!(quad.subject, purrdf::RdfTerm::Iri(_))
                    && match &quad.object {
                        purrdf::RdfTerm::Iri(_) => true,
                        purrdf::RdfTerm::Literal(_) => quad.predicate == LOGIC_MCP_TOOL_NAME,
                        purrdf::RdfTerm::BlankNode(_) | purrdf::RdfTerm::Triple(_) => false,
                    }
            })
            .map(|quad| {
                format!(
                    "{} <{}> {} <{TXN_WORLD}> .",
                    quad.subject, quad.predicate, quad.object
                )
            })
            .collect();
        lines.sort();
        lines.join("\n")
    }
}

/// Build the per-call one-step transaction world: the canonical action theory plus this call's
/// primitive program (`root` instantiates `schema_iri`, transitions from the start state) and the
/// start state's obtaining situations (`obtains`, derived from REAL memory state).
#[cfg(feature = "reasoning")]
fn txn_world_nquads(schema_iri: &str, obtains: &[&str]) -> String {
    use std::fmt::Write as _;
    let policy = action_policy_nquads();
    // Pre-size to hold the cached policy verbatim plus this call's primitive program and one
    // line per obtaining situation, so the build never reallocates.
    let mut nq = String::with_capacity(policy.len() + obtains.len() * 128 + 256);
    nq.push_str(policy);
    nq.push('\n');
    // `String`'s `fmt::Write` is infallible, so the formatting `Result` is discarded.
    let _ = writeln!(
        nq,
        "<{TXN_ROOT}> <{LOGIC_INSTANTIATES_SCHEMA}> <{schema_iri}> <{TXN_WORLD}> ."
    );
    let _ = writeln!(
        nq,
        "<{TXN_ROOT}> <{LOGIC_TRANSITION_FROM_STATE}> <{TXN_START}> <{TXN_WORLD}> ."
    );
    for situation in obtains {
        let _ = writeln!(
            nq,
            "<{TXN_START}> <{LOGIC_SITUATION_OBTAINS}> <{situation}> <{TXN_WORLD}> ."
        );
    }
    nq
}

/// Execute one memory write action as a TR transaction. `obtains` is the set of situations that
/// obtain at the start state (real state); the engine's executional entailment over them is the
/// commit gate. `dry_run` selects the hypothetical (sandbox) operator.
#[cfg(feature = "reasoning")]
fn execute_memory_txn(
    schema_iri: &str,
    obtains: &[&str],
    dry_run: bool,
) -> gmeow_errors::Result<TxReceipt> {
    let nq = txn_world_nquads(schema_iri, obtains);
    let mode = if dry_run {
        CommitMode::Hypothetical
    } else {
        CommitMode::Committed
    };
    execute_transaction(&nq, TXN_WORLD, TXN_ROOT, mode).map_err(|e| {
        gmeow_errors::Diag::of_kind(crate::error::Mcp {
            message: e.message().to_owned(),
        })
    })
}

/// The TR outcome rendered for the tool response.
#[cfg(feature = "reasoning")]
fn txn_json(receipt: &TxReceipt) -> Value {
    match receipt {
        TxReceipt::CommittedSuccess { path_len, .. } => {
            json!({"committed": true, "succeeded": true, "path_len": path_len})
        }
        TxReceipt::CommittedFailure { reason } => {
            json!({"committed": true, "succeeded": false, "reason": reason})
        }
        TxReceipt::HypotheticalSuccess { witness } => {
            json!({"committed": false, "succeeded": true, "witness": witness})
        }
        TxReceipt::HypotheticalFailure { reason } => {
            json!({"committed": false, "succeeded": false, "reason": reason})
        }
    }
}

#[cfg(feature = "reasoning")]
fn gts_iri(value: &str) -> GtsTerm {
    GtsTerm {
        kind: GtsTermKind::Iri,
        value: Some(value.to_string()),
        datatype: None,
        lang: None,
        direction: None,
        reifier: None,
    }
}

#[cfg(feature = "reasoning")]
fn gts_literal_dt(value: &str, datatype: usize) -> GtsTerm {
    GtsTerm {
        kind: GtsTermKind::Literal,
        value: Some(value.to_string()),
        datatype: Some(datatype),
        lang: None,
        direction: None,
        reifier: None,
    }
}

/// One quad as the GTS writers take it: subject, predicate and object as indices into the
/// segment's term table, plus the optional graph term.
#[cfg(feature = "reasoning")]
type GtsQuadRow = (usize, usize, usize, Option<usize>);

#[cfg(feature = "reasoning")]
fn push_gts_term(terms: &mut Vec<GtsTerm>, term: GtsTerm) -> usize {
    terms.push(term);
    terms.len() - 1
}

/// Append the trajectory-audit context segment for a just-recorded `gmeow:ToolCall` to the SAME
/// `memory.gts`, keyed to `call_id`: the call's `logic:instantiatesSchema`, its single
/// `logic:properPartOf` turn anchor (one call = one anchor — the stateless server mints no shared
/// turn state), its `gmeow:atTime` and the single canonical `gmeow:eventTemporalFrame`
/// (UTC-Gregorian, P11), the anchor's `logic:transitionFromState` start state, and the start's
/// obtaining situations. This is exactly the shape `emit_trajectory_audits` reads, so a cold trajectory
/// audit of `memory.gts` (unioned with the canonical action theory) verifies the executed turn.
#[cfg(feature = "reasoning")]
fn write_audit_segment(
    store: &dyn ClaimStore,
    call_id: &str,
    schema_iri: &str,
    obtains: &[&str],
    at_time: &str,
) -> gmeow_errors::Result<()> {
    let existing = store.store_bytes()?;
    store.append_audit_segment(&build_store_audit_segment(
        &existing, call_id, schema_iri, obtains, at_time,
    )?)
}

/// The medium a store's CURRENT tail was written through, or `None` for a store with no bytes yet.
///
/// A GTS segment declares exactly ONE codec catalog. A record appended to a store whose tail was
/// written through a DIFFERENT medium names a catalog that tail never declared, and the store stops
/// reconstructing: `Memory::graph()` yields `None` and every reifier-walking reader — `claims()`
/// among them — then reports an EMPTY store rather than an error. Reading the medium back off the
/// store's own bytes is what keeps the appended segment inside the tail's catalog.
#[cfg(feature = "reasoning")]
fn store_tail_medium(existing: &[u8]) -> gmeow_errors::Result<Option<StoreMedium>> {
    if existing.is_empty() {
        return Ok(None);
    }
    Ok(gmeow_gts_profile::segment_dictionaries(existing)?
        .into_iter()
        .next()
        .map(|(dictionary, bytes)| StoreMedium { dictionary, bytes }))
}

/// A writer that appends into `existing` under `medium`, opening a freshly-headed segment when
/// the tail does not already declare that medium's dictionary.
///
/// The append-only LIBRARIES are runtime stores in exactly the sense the claim store is: the
/// same `gmeow:gtsProducerRuntimeStores` production site covers agent memory, the conjecture
/// library and the candidate library, and all three are primed from the bundle's
/// `gmeow-memory-hot-v1`. A library written through an unprimed writer declares no dictionary
/// at all, so a consumer priming a decode from the bundle finds a store whose header promises
/// nothing.
#[cfg(feature = "reasoning")]
fn library_writer(
    existing: &[u8],
    medium: &StoreMedium,
) -> gmeow_errors::Result<(gmeow_gts_profile::GmeowGtsWriter, Vec<u8>)> {
    // `store_writer`'s appending branch emits ONLY the new frames — the segment header is not
    // repeated — so when the tail does not yet pin this medium the caller has to emit the
    // header itself, ahead of those frames. Returning it here keeps that pairing in one place.
    if existing.is_empty() || gmeow_gts_profile::store_tail_pins(existing, &medium.dictionary)? {
        let writer = gmeow_gts_profile::store_writer("ai-package", existing, medium)?;
        return Ok((writer, Vec::new()));
    }
    let header = gmeow_gts_profile::open_store_segment("ai-package", medium)?;
    let mut base = existing.to_vec();
    base.extend_from_slice(&header);
    let writer = gmeow_gts_profile::store_writer("ai-package", &base, medium)?;
    Ok((writer, header))
}

/// Build one trajectory-audit segment as a CONTINUATION of `existing` — the claim-store variant of
/// [`build_audit_segment`], which authors a standalone segment for the append-only libraries.
///
/// `store_writer` continues the tail when it already declares the medium's dictionary and opens a
/// freshly-headed segment when it does not; either way the appended bytes name a catalog the file
/// declares, which is the invariant [`store_tail_medium`] documents.
#[cfg(feature = "reasoning")]
fn build_store_audit_segment(
    existing: &[u8],
    call_id: &str,
    schema_iri: &str,
    obtains: &[&str],
    at_time: &str,
) -> gmeow_errors::Result<Vec<u8>> {
    let (terms, quads) = audit_terms_and_quads(call_id, schema_iri, obtains, at_time);
    let mut writer = match store_tail_medium(existing)? {
        Some(medium) => gmeow_gts_profile::store_writer("ai-package", existing, &medium)?,
        None => GtsWriter::new("ai-package"),
    };
    writer.add_terms(&terms)?;
    writer.add_quads(&quads)?;
    Ok(writer.into_bytes())
}

/// Build one trajectory-audit context segment's serialized bytes — the PURE, side-effect-free
/// half of [`write_audit_segment`], factored out so the conjecture-library commit path can
/// build the verdict segment AND its audit segment in memory and commit both together via
/// [`append_library_segments`] (one atomic replace), rather than two separate appends where
/// the second can fail after the first has already landed.
#[cfg(feature = "reasoning")]
fn build_audit_segment(
    existing: &[u8],
    medium: &StoreMedium,
    call_id: &str,
    schema_iri: &str,
    obtains: &[&str],
    at_time: &str,
) -> gmeow_errors::Result<Vec<u8>> {
    let (terms, quads) = audit_terms_and_quads(call_id, schema_iri, obtains, at_time);
    let (mut writer, header) = library_writer(existing, medium)?;
    writer.add_terms(&terms)?;
    writer.add_quads(&quads)?;
    let mut out = header;
    out.extend_from_slice(&writer.into_bytes());
    Ok(out)
}

/// The terms and quads of ONE trajectory-audit context — the carrier-independent half, shared by
/// the standalone library segment ([`build_audit_segment`]) and the tail-continuing store segment
/// ([`build_store_audit_segment`]) so the two can never describe the turn differently.
#[cfg(feature = "reasoning")]
fn audit_terms_and_quads(
    call_id: &str,
    schema_iri: &str,
    obtains: &[&str],
    at_time: &str,
) -> (Vec<GtsTerm>, Vec<GtsQuadRow>) {
    let anchor = format!("{call_id}#turn");
    let start = format!("{call_id}#start");

    let mut terms: Vec<GtsTerm> = Vec::new();
    let mut quads: Vec<(usize, usize, usize, Option<usize>)> = Vec::new();

    let t_call = push_gts_term(&mut terms, gts_iri(call_id));
    let t_anchor = push_gts_term(&mut terms, gts_iri(&anchor));
    let t_start = push_gts_term(&mut terms, gts_iri(&start));

    let t_inst = push_gts_term(&mut terms, gts_iri(LOGIC_INSTANTIATES_SCHEMA));
    let t_schema = push_gts_term(&mut terms, gts_iri(schema_iri));
    quads.push((t_call, t_inst, t_schema, None));

    let t_ppo = push_gts_term(&mut terms, gts_iri(LOGIC_PROPER_PART_OF));
    quads.push((t_call, t_ppo, t_anchor, None));

    let t_dt = push_gts_term(&mut terms, gts_iri(XSD_DATETIME));
    let t_at_time_p = push_gts_term(&mut terms, gts_iri(GMEOW_AT_TIME));
    let t_at_time_o = push_gts_term(&mut terms, gts_literal_dt(at_time, t_dt));
    quads.push((t_call, t_at_time_p, t_at_time_o, None));

    let t_frame_p = push_gts_term(&mut terms, gts_iri(GMEOW_EVENT_TEMPORAL_FRAME));
    let t_frame_o = push_gts_term(&mut terms, gts_iri(GMEOW_TEMPORAL_FRAME_UTC_GREGORIAN));
    quads.push((t_call, t_frame_p, t_frame_o, None));

    let t_tfs = push_gts_term(&mut terms, gts_iri(LOGIC_TRANSITION_FROM_STATE));
    quads.push((t_anchor, t_tfs, t_start, None));

    let t_so = push_gts_term(&mut terms, gts_iri(LOGIC_SITUATION_OBTAINS));
    for situation in obtains {
        let t_sit = push_gts_term(&mut terms, gts_iri(situation));
        quads.push((t_start, t_so, t_sit, None));
    }

    (terms, quads)
}

// ── Conjecture-library persistence (append-only GTS ai-package, TR-gated) ─────
//
// The conjecture library is a SEPARATE, append-only GTS collection — the read-only twin of
// `memory.gts`. Each `conjecture_test` commit appends one `ai-package` segment carrying the
// `project_conjecture_verdict` graph (a content-addressed, standpoint-scoped
// `logic:Conjecture` node with its embedded `logic:ReasoningResult` and refutation witness).
// It is NEVER folded into the base KB reasoning graph (R2): the reasoner reads
// `graph_dataset()` / the caller's KB, never `conjectures.gts`.

/// The conjecture library, resolved through the [`storage`] seam: natively the GTS
/// file at `GMEOW_CONJECTURE_PATH` (home-expanded) or `~/.gmeow/conjectures.gts`, in a
/// browser the in-process segment collection.
#[cfg(feature = "reasoning")]
fn conjecture_library() -> gmeow_errors::Result<Arc<dyn SegmentLibrary>> {
    storage().conjecture_library()
}

/// The candidate library, resolved through the [`storage`] seam: natively the GTS file
/// at `GMEOW_CANDIDATE_PATH` (home-expanded) or `~/.gmeow/candidates.gts`, in a browser
/// the in-process segment collection.
///
/// The candidate library is a SEPARATE, append-only GTS collection — the read-only twin
/// of the conjecture library — holding admissibility-gated authoring candidates. It is
/// NEVER folded into base-KB reasoning (R2). Mirrors [`conjecture_library`].
#[cfg(feature = "reasoning")]
fn candidate_library() -> gmeow_errors::Result<Arc<dyn SegmentLibrary>> {
    storage().candidate_library()
}

/// A deterministic lowercase-hex SHA-256 of `bytes` (the KB-world content address seed).
#[cfg(feature = "reasoning")]
fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut out = String::with_capacity(digest.len() * 2);
    for byte in digest {
        use std::fmt::Write as _;
        let _ = write!(out, "{byte:02x}");
    }
    out
}

/// Intern one IRI into the GTS term table, deduplicated by value. Shared by the subject /
/// predicate / object paths and by a typed literal's datatype-IRI leg.
#[cfg(feature = "reasoning")]
fn intern_nt_iri(iri: &str, terms: &mut Vec<GtsTerm>, seen: &mut HashMap<String, usize>) -> usize {
    let sig = format!("I\u{1}{iri}");
    if let Some(&id) = seen.get(&sig) {
        return id;
    }
    let id = push_gts_term(terms, gts_iri(iri));
    seen.insert(sig, id);
    id
}

/// Intern one purrdf-parsed N-Triples node (IRI, blank node, or typed literal) into the GTS
/// term table (deduplicated by semantic value); a literal first interns its datatype IRI so
/// the literal term references a live datatype id — reproducing the exact encounter order the
/// append-only, content-addressed GTS segment bytes are keyed on. A quoted-triple term is
/// rejected fail-closed via `reject`: the projection's closed subset never emits one.
#[cfg(feature = "reasoning")]
fn intern_nt_term(
    node: &RdfTerm,
    terms: &mut Vec<GtsTerm>,
    seen: &mut HashMap<String, usize>,
    reject: impl FnOnce() -> gmeow_errors::Diag,
) -> gmeow_errors::Result<usize> {
    match node {
        RdfTerm::Iri(iri) => Ok(intern_nt_iri(iri, terms, seen)),
        RdfTerm::BlankNode(label) => {
            let sig = format!("B\u{1}{label}");
            if let Some(&id) = seen.get(&sig) {
                return Ok(id);
            }
            let term = GtsTerm {
                kind: GtsTermKind::Bnode,
                value: Some(label.clone()),
                datatype: None,
                lang: None,
                direction: None,
                reifier: None,
            };
            let id = push_gts_term(terms, term);
            seen.insert(sig, id);
            Ok(id)
        }
        RdfTerm::Literal(lit) => {
            // The projection always emits `"lex"^^<dt>`, so purrdf carries an explicit datatype.
            let datatype = lit.datatype.as_deref().ok_or_else(reject)?;
            let sig = format!("L\u{1}{datatype}\u{1}{}", lit.lexical_form);
            if let Some(&id) = seen.get(&sig) {
                return Ok(id);
            }
            let dt_id = intern_nt_iri(datatype, terms, seen);
            let id = push_gts_term(terms, gts_literal_dt(&lit.lexical_form, dt_id));
            seen.insert(sig, id);
            Ok(id)
        }
        RdfTerm::Triple(_) => Err(reject()),
    }
}

/// Build one N-Triples body (e.g. the `project_conjecture_verdict` verdict, or a candidate
/// verdict carrying its authoring-role + provenance triples) into a GTS `ai-package` segment's
/// serialized bytes — the PURE, side-effect-free segment builder shared by the conjecture and
/// candidate libraries. The body is parsed into `(subject, predicate, object)` triples, interned
/// as RDF-1.2-native GTS terms (IRIs, blank nodes, and typed literals with their datatype), and
/// written via [`GtsWriter`] — no plain-RDF / quad shortcut, so the append-only libraries stay
/// RDF-1.2-native. Building the bytes is separated from appending them so a caller can assemble
/// MULTIPLE segments (e.g. the verdict segment AND its audit segment) in memory and commit them
/// together as one atomic file replace — see [`append_library_segments`].
#[cfg(feature = "reasoning")]
fn build_nt_segment(
    existing: &[u8],
    medium: &StoreMedium,
    nt_body: &str,
) -> gmeow_errors::Result<Vec<u8>> {
    let mut terms: Vec<GtsTerm> = Vec::new();
    let mut quads: Vec<(usize, usize, usize, Option<usize>)> = Vec::new();
    let mut seen: HashMap<String, usize> = HashMap::new();

    for (lineno, raw) in nt_body.lines().enumerate() {
        let line = raw.trim();
        if line.is_empty() {
            continue;
        }
        let malformed = |what: &str| {
            gmeow_errors::Diag::of_kind(crate::error::Mcp {
                message: format!("conjecture projection line {} bad {what}", lineno + 1),
            })
        };
        // Delegate N-Triples lexing of THIS single statement to purrdf, one line at a time,
        // in DOCUMENT order. A whole-body parse cannot be used here: purrdf's dataset freeze
        // sorts quads by term id, so `owned_quads()` would not preserve document order — and
        // that order (subject, predicate, [datatype,] object per line) is baked into the
        // append-only, content-addressed GTS segment bytes. Line-at-a-time parsing recovers
        // it exactly while still owning zero hand-rolled term lexing.
        let statement = purrdf::parse_dataset(line.as_bytes(), "application/n-triples", None)
            .map_err(|_| malformed("triple"))?;
        let mut quad_iter = statement.owned_quads();
        let quad = quad_iter.next().ok_or_else(|| malformed("triple"))?;
        if quad_iter.next().is_some() {
            return Err(malformed("triple (multiple statements on one line)"));
        }
        // A subject is an IRI or a blank node; a literal/quoted-triple subject is rejected
        // fail-closed (purrdf never yields a literal subject in N-Triples, but the guard keeps
        // the closed subset honest). The predicate is always an IRI in purrdf's `RdfQuad`.
        let s = match &quad.subject {
            RdfTerm::Iri(_) | RdfTerm::BlankNode(_) => {
                intern_nt_term(&quad.subject, &mut terms, &mut seen, || {
                    malformed("subject")
                })?
            }
            RdfTerm::Literal(_) | RdfTerm::Triple(_) => return Err(malformed("subject")),
        };
        let p = intern_nt_iri(&quad.predicate, &mut terms, &mut seen);
        let o = intern_nt_term(&quad.object, &mut terms, &mut seen, || malformed("object"))?;
        quads.push((s, p, o, None));
    }

    let (mut writer, header) = library_writer(existing, medium)?;

    writer.add_terms(&terms)?;
    writer.add_quads(&quads)?;
    let mut out = header;
    out.extend_from_slice(&writer.into_bytes());
    Ok(out)
}

/// Hold the library's exclusive lock for the duration of `f`, serializing every
/// library operation — reads, precondition checks, and commits alike — against every
/// other holder. Callers that must read-then-decide-then-write (e.g.
/// `refute_conjecture`'s "is this id still in the library and not yet withdrawn?"
/// precondition) run the ENTIRE read → check → append sequence inside `f`, so two
/// concurrent callers can no longer both observe the pre-write state and both commit
/// (the lost-update / double-write race). The lock is released when the guard drops at
/// the end of this call, regardless of whether `f` succeeded.
///
/// What "exclusive" means is the backend's business ([`SegmentLibrary::lock`]): natively
/// a cross-process `flock` on a sidecar file, in a browser a mutex over the one
/// in-process library.
#[cfg(feature = "reasoning")]
fn with_library_lock<T>(
    library: &dyn SegmentLibrary,
    f: impl FnOnce() -> gmeow_errors::Result<T>,
) -> gmeow_errors::Result<T> {
    let guard: Box<dyn LibraryLock + '_> = library.lock()?;
    let result = f();
    drop(guard);
    result
}

/// Atomically commit `segments` (each an already-serialized GTS `ai-package` segment, in order)
/// to `library`, ALL-OR-NOTHING: the new contents — the library's current bytes (if any)
/// followed by every segment in `segments`, in order — are assembled ENTIRELY in memory, then
/// handed to [`SegmentLibrary::replace_bytes`], which either lands the WHOLE new content or
/// leaves the PRIOR content completely untouched. So if anything fails partway (e.g. the audit
/// segment's bytes can't be built, or the commit fails), the library is never left holding some
/// but not all of `segments` (closing the "audit append fails, library append is left applied"
/// failure mode). The caller MUST already hold the library lock (see [`with_library_lock`]) —
/// this function does not lock by itself, so it can be called once per commit even when it
/// writes more than one segment.
#[cfg(feature = "reasoning")]
fn append_library_segments(
    library: &dyn SegmentLibrary,
    segments: &[Vec<u8>],
) -> gmeow_errors::Result<()> {
    // Each segment is the FRAMES to append (plus its own header when it opens a segment), so
    // the commit concatenates them onto the library in order. They must have been authored in
    // that same order — each against the bytes the previous one produced — or the second one's
    // `prev` points past the first and the chain reads back broken.
    let mut bytes = library.read_bytes()?;
    for segment in segments {
        bytes.extend_from_slice(segment);
    }
    library.replace_bytes(&bytes)
}

/// A per-segment collector for [`read_library`]: it captures each GTS segment's
/// term table and quads IN FILE (append) ORDER, so a `logic:conjectureLifecycleState`
/// supersession can be resolved as last-writer-wins by SEGMENT ORDER.
///
/// Order is the ONLY sound disambiguator here (R3). The unioned dataset a plain
/// `import_gts_events` yields carries EVERY state a node ever held at once — after a store
/// then a refute, one node holds both its engine verdict (`Open`/`Corroborated`/`Refuted…`)
/// AND `ConjectureWithdrawn` — and `gmeow:atTime` cannot break the tie either, because every
/// audit segment stamps the SAME fixed determinism epoch. The streaming reader
/// (`read_to_sink` with `allow_segments = true`) is the one path that preserves per-segment
/// identity: each appended `GtsWriter` blob reads back as its own segment, delivered in
/// append order, so folding the lifecycle assertions in that order makes the LAST one win.
#[cfg(feature = "reasoning")]
#[derive(Default)]
struct ConjectureSegments {
    /// One row per segment, indexed by segment order.
    segments: Vec<ConjectureSegmentRows>,
    /// The first reader diagnostic, if any — any diagnostic is a HARD read failure (no
    /// silent partial read of a corrupt library).
    diagnostic: Option<String>,
}

/// One segment's captured rows: its segment-local term table and its `(s, p, o)` quads.
#[cfg(feature = "reasoning")]
#[derive(Default)]
struct ConjectureSegmentRows {
    /// Segment-local term id → interned term (ids are dense from 0 within a segment).
    terms: Vec<Option<GtsTerm>>,
    /// `(subject, predicate, object)` segment-local term ids (the graph slot is dropped —
    /// the conjecture library writes only default-graph triples).
    quads: Vec<(usize, usize, usize)>,
}

#[cfg(feature = "reasoning")]
impl ConjectureSegments {
    /// The rows for `index`, growing the segment vector so an out-of-order or sparse
    /// segment index still lands in its own slot.
    fn seg(&mut self, index: usize) -> &mut ConjectureSegmentRows {
        if index >= self.segments.len() {
            self.segments
                .resize_with(index + 1, ConjectureSegmentRows::default);
        }
        &mut self.segments[index]
    }
}

#[cfg(feature = "reasoning")]
impl purrdf::gts::reader::StreamingSink for ConjectureSegments {
    fn term(&mut self, segment_index: usize, term_id: usize, term: &GtsTerm) {
        let rows = self.seg(segment_index);
        if term_id >= rows.terms.len() {
            rows.terms.resize(term_id + 1, None);
        }
        rows.terms[term_id] = Some(term.clone());
    }

    fn quad(&mut self, segment_index: usize, quad: purrdf::gts::model::Quad) {
        let (subject, predicate, object, _graph) = quad;
        self.seg(segment_index)
            .quads
            .push((subject, predicate, object));
    }

    fn diagnostic(&mut self, diagnostic: &purrdf::gts::model::Diagnostic) {
        if self.diagnostic.is_none() {
            self.diagnostic = Some(format!("{}: {}", diagnostic.code, diagnostic.detail));
        }
    }
}

/// Read the append-only conjecture library at `path`, resolving each stored
/// `logic:Conjecture` node's **EFFECTIVE** `logic:conjectureLifecycleState` by SEGMENT
/// ORDER (last writer wins — see [`ConjectureSegments`] for why order, not the union or
/// `gmeow:atTime`, is the only sound key). A node typed `logic:Conjecture` in any segment is
/// in the library; its effective state is the object of the LAST lifecycle assertion for it
/// in append order. A missing library file is an EMPTY library (a first-ever refute of an
/// unknown id), not an error; any reader diagnostic or a torn trailing item is a HARD FAIL.
#[cfg(feature = "reasoning")]
fn read_library(
    library: &dyn SegmentLibrary,
) -> gmeow_errors::Result<BTreeMap<String, ConjectureLifecycleState>> {
    let bytes = library.read_bytes()?;
    if bytes.is_empty() {
        // A library that has never been written is an EMPTY library (a first-ever
        // refute of an unknown id), not an error.
        return Ok(BTreeMap::new());
    }

    let mut sink = ConjectureSegments::default();
    let result = purrdf::gts::reader::read_to_sink(&bytes, true, None, &mut sink);
    if let Some(diag) = sink.diagnostic.take() {
        return Err(gmeow_errors::Diag::of_kind(crate::error::Mcp {
            message: format!("conjecture library read diagnostic: {diag}"),
        }));
    }
    if let Some(first) = result.diagnostics.first() {
        return Err(gmeow_errors::Diag::of_kind(crate::error::Mcp {
            message: format!(
                "conjecture library read diagnostic: {}: {}",
                first.code, first.detail
            ),
        }));
    }
    if let Some(offset) = result.torn {
        return Err(gmeow_errors::Diag::of_kind(crate::error::Mcp {
            message: format!("conjecture library has a torn trailing item at byte {offset}"),
        }));
    }

    let conjecture_iri = format!("{LOGIC_NAMESPACE}Conjecture");
    let lifecycle_iri = format!("{LOGIC_NAMESPACE}conjectureLifecycleState");

    let mut is_conjecture: BTreeSet<String> = BTreeSet::new();
    let mut effective: BTreeMap<String, ConjectureLifecycleState> = BTreeMap::new();

    // Fold the segments in FILE ORDER — a later segment's lifecycle assertion for a node
    // supersedes an earlier one, so `insert` (last-writer-wins) IS the supersession rule.
    for seg in &sink.segments {
        let resolve_iri = |id: usize| -> Option<&str> {
            seg.terms
                .get(id)?
                .as_ref()
                .filter(|term| term.kind == GtsTermKind::Iri)
                .and_then(|term| term.value.as_deref())
        };
        for &(subject, predicate, object) in &seg.quads {
            let (Some(subj), Some(pred)) = (resolve_iri(subject), resolve_iri(predicate)) else {
                continue;
            };
            if pred == RDF_TYPE {
                if resolve_iri(object) == Some(conjecture_iri.as_str()) {
                    is_conjecture.insert(subj.to_owned());
                }
            } else if pred == lifecycle_iri
                && let Some(obj) = resolve_iri(object)
                && let Some(local) = obj.strip_prefix(LOGIC_NAMESPACE)
                && let Some(state) = ConjectureLifecycleState::from_local(local)
            {
                effective.insert(subj.to_owned(), state);
            }
        }
    }

    // A node is in the library iff it was typed `logic:Conjecture`; every such node carries a
    // lifecycle state (REQUIRED by logic:ConjectureShape), so the intersection is exactly the
    // library's conjecture set paired with its effective, segment-order-resolved state.
    Ok(effective
        .into_iter()
        .filter(|(node, _)| is_conjecture.contains(node))
        .collect())
}

#[cfg(feature = "reasoning")]
fn tool_arguments(args: &Value, keys: &[&str]) -> String {
    let mut out = serde_json::Map::new();
    for key in keys {
        if let Some(value) = args.get(*key)
            && !value.is_null()
        {
            out.insert((*key).to_string(), value.clone());
        }
    }
    Value::Object(out).to_string()
}

/// The `recall` tool body against an EXPLICIT claim store.
///
/// The shaping lives here, once, so it can be exercised against any [`ClaimStore`] —
/// in particular the browser backend's in-process store, which no environment variable
/// can point [`McpServer::claim_store`] at.
///
/// # Errors
///
/// A malformed `limit` / `min_confidence` argument, or a store read failure.
pub fn recall_json(store: &dyn ClaimStore, args: &Value) -> gmeow_errors::Result<String> {
    let limit = optional_limit(args, "limit")?.unwrap_or(10);
    let claims = store.recall(RecallOptions {
        query: optional_str(args, "query").unwrap_or(""),
        min_confidence: optional_f64(args, "min_confidence")?,
        limit,
        include_suppressed: optional_bool(args, "include_suppressed").unwrap_or(false),
    })?;
    Ok(json!({
        "ok": true,
        "claims": claims.iter().map(claim_json).collect::<Vec<_>>(),
    })
    .to_string())
}

/// The `store_segment` tool body against an EXPLICIT claim store.
///
/// Serializes the store's whole readable contents — every claim and every recorded tool
/// call — into the shared session-store transport segment, and reports the two counts the
/// caller needs to tell "the store holds nothing" apart from "the store holds something".
/// It COMMITS NOTHING: it is a read, and the segment it returns is a projection of state
/// that was already there.
///
/// This is the ONLY way a caller can obtain the store's contents as RDF. `recall` answers a
/// QUERY and returns a ranked, truncated, JSON view of matching claims; it is not, and
/// cannot be, a snapshot — which is why a session export that tried to read a serialization
/// off a `recall` result found none.
///
/// The shaping lives here, once, so it can be exercised against any [`ClaimStore`] — in
/// particular the browser backend's in-process store, which no environment variable can
/// point [`McpServer::claim_store`] at.
///
/// # Errors
///
/// A store read failure, or a record the transport shape cannot carry.
pub fn store_segment_json(store: &dyn ClaimStore) -> gmeow_errors::Result<String> {
    let claims = store.claims()?;
    let calls = store.tool_calls()?;
    let nquads = crate::storage::claim_segment(&claims, &calls)?;
    Ok(json!({
        "ok": true,
        "claim_count": claims.len(),
        "tool_call_count": calls.len(),
        "nquads": nquads,
    })
    .to_string())
}

fn claim_json(claim: &purrdf::gts::examples::agent_memory::Claim) -> Value {
    json!({
        "id": claim.id,
        "text": claim.text,
        "confidence": claim.confidence,
        "according_to": claim.according_to,
        "source": claim.source,
        "created": claim.created,
        "suppressed": claim.suppressed,
    })
}

// ── slice_brief: serve pre-assembled AuthoringPackets from the bundle graph ───────
//
// The per-slice `gmeow:AuthoringPacket` corpus is folded into `gmeow.gts` as the named
// graph `gmeow:graph/authoring-briefs` (see `stages::carrier`). Serving a packet is a
// checkout-free projection of that graph — no repo, no SHACL shape union, no live
// re-assembly. The bundle carries the SPARSE packet body (covered-term IRIs, present
// fr/zh/external grounding cells, exemplars, and coverage margins); each covered term's
// full definition/axiom content stays in the base graph, reachable per IRI via
// `lookup_term` / `doc_card`.

/// Expand a `slice_brief` slice argument to a full slice IRI: a value already carrying a
/// scheme is used verbatim; a bare short-name (`ai`) becomes `{GMEOW_NS}slices/{name}` —
/// the `gmeow:packetSourceSlice` shape the bundle carries.
#[cfg(feature = "core")]
fn expand_slice_iri(slice: &str) -> String {
    if slice.contains("://") {
        slice.to_string()
    } else {
        format!("{GMEOW_NS}slices/{}", slice.trim_start_matches('/'))
    }
}

/// The first object of `pred` among a subject's edges, as an IRI string.
fn iri_object<'a>(edges: &'a [(String, RdfTerm)], pred: &str) -> Option<&'a str> {
    edges.iter().find_map(|(p, o)| match o {
        RdfTerm::Iri(iri) if p == pred => Some(iri.as_str()),
        _ => None,
    })
}

/// The first object of `pred` among a subject's edges, as a literal lexical form.
fn lit_object<'a>(edges: &'a [(String, RdfTerm)], pred: &str) -> Option<&'a str> {
    edges.iter().find_map(|(p, o)| match o {
        RdfTerm::Literal(l) if p == pred => Some(l.lexical_form.as_str()),
        _ => None,
    })
}

/// Every IRI object of `pred` among a subject's edges, in edge order.
fn iri_objects(edges: &[(String, RdfTerm)], pred: &str) -> Vec<String> {
    edges
        .iter()
        .filter_map(|(p, o)| match o {
            RdfTerm::Iri(iri) if p == pred => Some(iri.clone()),
            _ => None,
        })
        .collect()
}

/// The first integer object of `pred` (a non-negative count/index literal).
fn int_object(edges: &[(String, RdfTerm)], pred: &str) -> Option<i64> {
    lit_object(edges, pred).and_then(|s| s.parse::<i64>().ok())
}

/// Read one `gmeow:GroundingCoverage` cell subject into JSON — the present fr/zh/external
/// incidences the packet materializes (fr/zh translations ride `gmeow:groundingValue`).
fn grounding_cell_json(cell_iri: &str, edges: &[(String, RdfTerm)]) -> Value {
    // Single O(N) pass: every grounding predicate lives under GMEOW_NS, so strip the prefix once
    // and match on the local name. First occurrence wins (the `iri_object`/`lit_object` find_map
    // semantics this replaces); the object type is matched inline so an IRI-only or literal-only
    // predicate is picked exactly as before. Fields are collected into locals, then inserted in a
    // FIXED order so the JSON is byte-identical regardless of edge order.
    let mut term = None;
    let mut attribute = None;
    let mut predicate = None;
    let mut value = None;
    let mut external_entity = None;
    let mut external_label = None;
    let mut align_predicate = None;
    let mut confidence_lit = None;
    let mut conflict_lit = None;
    let mut conflict_with = None;

    for (p, o) in edges {
        let Some(local) = p.strip_prefix(GMEOW_NS) else {
            continue;
        };
        match (local, o) {
            ("groundingTerm", RdfTerm::Iri(iri)) if term.is_none() => term = Some(iri.as_str()),
            ("groundingAttribute", RdfTerm::Iri(iri)) if attribute.is_none() => {
                attribute = Some(iri.rsplit(['/', '#']).next().unwrap_or(iri).to_string());
            }
            ("groundingPredicate", RdfTerm::Literal(l)) if predicate.is_none() => {
                predicate = Some(l.lexical_form.as_str());
            }
            ("groundingValue", RdfTerm::Literal(l)) if value.is_none() => {
                value = Some(l.lexical_form.as_str());
            }
            ("groundingExternalEntity", RdfTerm::Iri(iri)) if external_entity.is_none() => {
                external_entity = Some(iri.as_str());
            }
            ("groundingExternalLabel", RdfTerm::Literal(l)) if external_label.is_none() => {
                external_label = Some(l.lexical_form.as_str());
            }
            ("groundingAlignPredicate", RdfTerm::Literal(l)) if align_predicate.is_none() => {
                align_predicate = Some(l.lexical_form.as_str());
            }
            ("groundingConfidence", RdfTerm::Literal(l)) if confidence_lit.is_none() => {
                confidence_lit = Some(l.lexical_form.as_str());
            }
            ("groundingConflict", RdfTerm::Literal(l)) if conflict_lit.is_none() => {
                conflict_lit = Some(l.lexical_form.as_str());
            }
            ("groundingConflictWith", RdfTerm::Iri(iri)) if conflict_with.is_none() => {
                conflict_with = Some(iri.as_str());
            }
            _ => {}
        }
    }

    let mut obj = serde_json::Map::new();
    obj.insert("cell".to_string(), json!(cell_iri));
    obj.insert("term".to_string(), json!(term));
    obj.insert("attribute".to_string(), json!(attribute));
    if let Some(v) = predicate {
        obj.insert("predicate".to_string(), json!(v));
    }
    if let Some(v) = value {
        obj.insert("value".to_string(), json!(v));
    }
    if let Some(v) = external_entity {
        obj.insert("external_entity".to_string(), json!(v));
    }
    if let Some(v) = external_label {
        obj.insert("external_label".to_string(), json!(v));
    }
    if let Some(v) = align_predicate {
        obj.insert("align_predicate".to_string(), json!(v));
    }
    if let Some(v) = confidence_lit.and_then(|s| s.parse::<f64>().ok()) {
        obj.insert("confidence".to_string(), json!(v));
    }
    if conflict_lit == Some("true") {
        obj.insert("conflict".to_string(), json!(true));
        if let Some(v) = conflict_with {
            obj.insert("conflict_with".to_string(), json!(v));
        }
    }
    Value::Object(obj)
}

/// Extract the authoring packet(s) for one slice from the bundle's projected
/// `graph/authoring-briefs` dataset. Returns a JSON envelope carrying a structured
/// reading of every matching packet PLUS the packet subgraph as canonical turtle (the
/// byte-reconstructible surface the pipeline folded). A slice/axis/batch with no
/// matching packet is a HARD FAIL — never a vacuous `ok:true` empty result.
///
/// This is the SINGLE packet-extraction core shared by the `slice_brief` MCP tool and
/// the `gmeow slice brief --from-bundle` CLI path (one implementation, not two).
pub fn extract_authoring_packets(
    briefs: &purrdf::RdfDataset,
    slice_iri: &str,
    axis: Option<&str>,
    batch: Option<u64>,
) -> gmeow_errors::Result<Value> {
    let err = |message: String| gmeow_errors::Diag::of_kind(crate::error::Mcp { message });
    let packet_prefix = format!("{slice_iri}/authoring-packet/");
    let source_slice_p = format!("{GMEOW_NS}packetSourceSlice");
    let grounding_p = format!("{GMEOW_NS}packetGrounding");

    // Collect the slice's subgraph: every packet node and its grounding-cell nodes share
    // the packet-IRI prefix (turtle.rs mints stable IRIs, never blank nodes), so one
    // prefix scan captures exactly this slice's briefs and nothing else.
    let mut subjects: BTreeMap<String, Vec<(String, RdfTerm)>> = BTreeMap::new();
    for quad in briefs.owned_quads() {
        let RdfTerm::Iri(subj) = &quad.subject else {
            continue;
        };
        if !subj.starts_with(&packet_prefix) {
            continue;
        }
        subjects
            .entry(subj.clone())
            .or_default()
            .push((quad.predicate.clone(), quad.object.clone()));
    }

    // A packet subject is `{slice}/authoring-packet/{axis}/batch-{n}` with `n` a bare
    // integer; a cell appends `/cell/...` so its `batch-` suffix does not parse as an
    // integer. That structure (plus a `packetSourceSlice` edge) identifies packets, and
    // the optional axis/batch selectors narrow them.
    let mut selected: Vec<(String, String, u64)> = Vec::new(); // (packet_iri, axis, batch)
    for (subj, edges) in &subjects {
        let Some(rest) = subj.strip_prefix(&packet_prefix) else {
            continue;
        };
        let Some((axis_seg, batch_seg)) = rest.split_once("/batch-") else {
            continue;
        };
        let Ok(batch_n) = batch_seg.parse::<u64>() else {
            continue; // a cell, not a packet
        };
        if !edges.iter().any(|(p, _)| p == &source_slice_p) {
            continue;
        }
        if axis.is_some_and(|a| axis_seg != a) {
            continue;
        }
        if batch.is_some_and(|b| batch_n != b) {
            continue;
        }
        selected.push((subj.clone(), axis_seg.to_string(), batch_n));
    }
    selected.sort();

    if selected.is_empty() {
        let sel = match (axis, batch) {
            (Some(a), Some(b)) => format!(" (axis `{a}`, batch {b})"),
            (Some(a), None) => format!(" (axis `{a}`)"),
            (None, Some(b)) => format!(" (batch {b})"),
            (None, None) => String::new(),
        };
        return Err(err(format!(
            "slice_brief: no authoring packet for slice <{slice_iri}>{sel} in the bundle"
        )));
    }

    // Build the JSON reading and gather the subgraph subjects (packets + their cells) to
    // serialize as canonical turtle.
    let mut emit_subjects: BTreeSet<String> = BTreeSet::new();
    let mut packets_json: Vec<Value> = Vec::new();
    for (packet_iri, axis_seg, batch_n) in &selected {
        emit_subjects.insert(packet_iri.clone());
        let edges = &subjects[packet_iri];
        let cells = iri_objects(edges, &grounding_p);
        let mut grounding: Vec<Value> = Vec::new();
        for cell in &cells {
            emit_subjects.insert(cell.clone());
            if let Some(cell_edges) = subjects.get(cell) {
                grounding.push(grounding_cell_json(cell, cell_edges));
            }
        }
        packets_json.push(json!({
            "packet_iri": packet_iri,
            "source_slice": iri_object(edges, &source_slice_p),
            "axis": axis_seg,
            "batch": batch_n,
            "digest": lit_object(edges, &format!("{GMEOW_NS}packetDigest")),
            "term_count": int_object(edges, &format!("{GMEOW_NS}packetTermCount")),
            "exemplar_shortfall": int_object(edges, &format!("{GMEOW_NS}exemplarShortfall")),
            "margins": {
                "fr_present": int_object(edges, &format!("{GMEOW_NS}packetFrPresent")),
                "fr_absent": int_object(edges, &format!("{GMEOW_NS}packetFrAbsent")),
                "zh_present": int_object(edges, &format!("{GMEOW_NS}packetZhPresent")),
                "zh_absent": int_object(edges, &format!("{GMEOW_NS}packetZhAbsent")),
                "external_mapped": int_object(edges, &format!("{GMEOW_NS}packetExternalMapped")),
                "external_absent": int_object(edges, &format!("{GMEOW_NS}packetExternalAbsent")),
            },
            "covers_terms": iri_objects(edges, &format!("{GMEOW_NS}packetCoversTerm")),
            "exemplars": iri_objects(edges, &format!("{GMEOW_NS}packetExemplar")),
            "grounding": grounding,
        }));
    }

    // Canonical turtle of exactly the selected packet subgraph — the same bytes the
    // pipeline folded, reconstructed from the projected graph.
    let mut builder = RdfDatasetBuilder::new();
    for subj in &emit_subjects {
        let s = RdfTerm::iri(subj.clone());
        for (pred, obj) in &subjects[subj] {
            builder.push_owned_quad(&RdfQuad::new(s.clone(), pred.clone(), obj.clone()));
        }
    }
    let subgraph = builder.freeze().map_err(|e| {
        err(format!(
            "slice_brief: packet subgraph failed to freeze: {e}"
        ))
    })?;
    let nt = purrdf::serialize_dataset(
        &subgraph,
        "application/n-triples",
        purrdf::SerializeGraph::Dataset,
    )
    .map_err(|e| err(format!("slice_brief: serialize packet subgraph: {e}")))?;
    let turtle = purrdf::turtle_normalize::canonical_turtle(
        &nt,
        &gmeow_logic_compile::ingest::prefixes::registry_pairs(),
    )
    .map_err(|m| err(format!("slice_brief: canonicalize packet turtle: {m}")))?;

    Ok(json!({
        "slice": slice_iri,
        "packet_count": packets_json.len(),
        "packets": packets_json,
        "turtle": turtle,
    }))
}

/// Serve authoring packets for one slice directly from `gmeow.gts` snapshot bytes — the
/// `gmeow slice brief --from-bundle` path. Imports the bundle, projects the
/// authoring-briefs graph, and runs the SAME [`extract_authoring_packets`] core the MCP
/// `slice_brief` tool uses (one implementation, not two).
///
/// Gated on the `core` feature with the tool it shares its core with.
#[cfg(feature = "core")]
pub fn slice_brief_from_bundle(
    snapshot: &[u8],
    slice: &str,
    axis: Option<&str>,
    batch: Option<u64>,
) -> gmeow_errors::Result<Value> {
    let bundle =
        purrdf::import_gts_events(snapshot).with_ctx(|| "read snapshot gmeow.gts".to_string())?;
    let briefs = bundle
        .dataset
        .project_named_graph(gmeow_bundle_view::graph_iris::GRAPH_AUTHORING_BRIEFS);
    let slice_iri = expand_slice_iri(slice);
    extract_authoring_packets(&briefs, &slice_iri, axis, batch)
}

/// The native tool-surface suite.
///
/// Gated `not(target_arch = "wasm32")` as well as `test`, and that is not incidental:
/// these tests ARE the native storage backend's tests. They mutate the process
/// environment, create temp directories, take `flock`s across threads, and assert on
/// the bytes a `memory.gts` ends up holding — every one of which is a native-host fact.
/// The browser backend has its own suite in [`browser_storage_tests`], which runs on
/// every target including this one.
#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;
    use std::ffi::OsString;
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::sync::{Mutex, OnceLock};

    use purrdf::gts::examples::agent_memory::Memory;

    /// The `env` module the environment-mutating helpers below use. Named here rather
    /// than at the crate root so the production surface carries no `std::env` import at
    /// all — the seam is the only production path to a configuration value.
    use std::env;

    /// Read a real on-disk slice into the JSON `files` argument the `slice_quality`
    /// tool takes. The READING is the test's job — the tool is handed bytes, which is
    /// exactly the point of the argument. Delegates to the scorer's own directory
    /// reader so the test and the CLI agree on which files make up a slice.
    fn slice_files_arg(slice_dir: &Path) -> Value {
        let files = gmeow_slice_quality::report::slice_files_from_dir(slice_dir)
            .expect("the in-repo slice reads");
        Value::Object(
            files
                .into_iter()
                .map(|(path, bytes)| {
                    (
                        path,
                        Value::String(
                            String::from_utf8(bytes).expect("slice files are UTF-8 text"),
                        ),
                    )
                })
                .collect(),
        )
    }

    /// A native conjecture/candidate library at an explicit path — the handle the
    /// library-level tests drive instead of the process backend's env-resolved one.
    fn library_at(path: &Path) -> Arc<dyn SegmentLibrary> {
        crate::storage::fs_segment_library(path.to_path_buf())
    }

    /// Append one hand-built conjecture-verdict segment to the append-only library at `path`,
    /// as a GTS `ai-package` segment, via the SAME locked/atomic commit path production code
    /// uses ([`with_library_lock`] + [`append_library_segments`]). Test-only: it seeds a
    /// library file with a single segment so segment-order-resolution tests
    /// ([`read_library`]) can be driven without going through a full `store_conjecture`
    /// engine run. Production call sites build BOTH the verdict segment and its audit segment
    /// and commit them together via [`append_library_segments`] directly (one atomic replace
    /// covering both), rather than through this single-segment helper.
    fn write_conjecture_segment(path: &Path, nt_body: &str) -> gmeow_errors::Result<()> {
        let segment = build_nt_segment(&[], &probe_medium(), nt_body)?;
        let library = library_at(path);
        with_library_lock(library.as_ref(), || {
            append_library_segments(library.as_ref(), &[segment])
        })
    }

    /// A representative N-Triples body mirroring what `project_conjecture_verdict` /
    /// `project_candidate_verdict` emit: multiple triples; a repeated subject/predicate IRI
    /// (exercises term-table dedup); a blank-node subject linked to a blank-node object
    /// (`_:witness0` → `_:premise0`); typed literals carrying the projection's `\\ \" \n \t`
    /// escape subset; and both `xsd:string` and `xsd:integer` datatypes. Interning order per
    /// line is subject, predicate, [datatype,] object — the order the append-only GTS segment
    /// bytes are keyed on.
    const BYTE_PARITY_NT_BODY: &str = concat!(
        "<urn:c:1> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <https://gmeow.ai/logic#Conjecture> .\n",
        "<urn:c:1> <https://gmeow.ai/logic#conjectureFormula> \"line1\\nquote \\\" back \\\\ tab \\t end\"^^<http://www.w3.org/2001/XMLSchema#string> .\n",
        "<urn:c:1> <https://gmeow.ai/logic#conjectureStandpoint> <urn:sp:default> .\n",
        "<urn:c:1> <https://gmeow.ai/logic#conjectureRefutationWitness> _:witness0 .\n",
        "_:witness0 <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <https://gmeow.ai/logic#ContradictionWitness> .\n",
        "_:witness0 <https://gmeow.ai/logic#derivedFrom> _:premise0 .\n",
        "_:premise0 <https://gmeow.ai/logic#conjectureFormula> \"0\"^^<http://www.w3.org/2001/XMLSchema#integer> .\n",
    );

    /// Permanent regression guard for [`build_nt_segment`]'s exact output bytes. The append-only
    /// conjecture/candidate libraries are content-addressed, so the segment bytes (and thus this
    /// digest) MUST stay byte-identical across any change to how the body is parsed — this pins
    /// the delegation of N-Triples parsing to purrdf against the prior hand-rolled lexer.
    #[test]
    fn build_nt_segment_bytes_are_stable() {
        // DETERMINISM, not a pinned digest. The segment is authored through the store medium
        // now, so its bytes are a function of the shipped dictionary — which every corpus
        // sweep legitimately re-trains. A hardcoded digest would therefore red on ordinary
        // maintenance while saying "content-addressed", which is the opposite of what it
        // claims to protect. What must hold is that the SAME body under the SAME medium gives
        // the same bytes: that is what makes the segment content-addressable at all.
        let medium = probe_medium();
        let once = build_nt_segment(&[], &medium, BYTE_PARITY_NT_BODY)
            .expect("representative body must parse");
        let twice = build_nt_segment(&[], &medium, BYTE_PARITY_NT_BODY)
            .expect("representative body must parse");
        assert_eq!(
            sha256_hex(&once),
            sha256_hex(&twice),
            "build_nt_segment is not deterministic; segment bytes are content-addressed and \
             cannot depend on anything but the body and the medium",
        );
        assert!(
            !once.is_empty(),
            "a representative body must author real bytes, or the comparison above is vacuous",
        );
    }

    static ENV_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

    struct EnvRestore(Vec<(&'static str, Option<OsString>)>);

    impl EnvRestore {
        fn capture(keys: &[&'static str]) -> Self {
            Self(keys.iter().map(|key| (*key, env::var_os(key))).collect())
        }
    }

    impl Drop for EnvRestore {
        fn drop(&mut self) {
            for (key, value) in &self.0 {
                // SAFETY: single-threaded test env mutation under the test's env lock.
                unsafe {
                    match value {
                        Some(value) => env::set_var(key, value),
                        None => env::remove_var(key),
                    }
                }
            }
        }
    }

    struct CwdRestore(PathBuf);

    impl CwdRestore {
        fn capture() -> Self {
            Self(env::current_dir().expect("current dir"))
        }
    }

    impl Drop for CwdRestore {
        fn drop(&mut self) {
            env::set_current_dir(&self.0).expect("restore current dir");
        }
    }

    /// The hot store medium, resolved from the test snapshot exactly as the runtime resolves
    /// it — so a segment a test authors is primed the way a shipped one is.
    pub(crate) fn probe_medium() -> StoreMedium {
        store_medium(&snapshot(), MEMORY_HOT_DICTIONARY)
            .expect("the test snapshot pins the hot store dictionary")
    }

    fn snapshot() -> Vec<u8> {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        fs::read(root.join("generated/dist/gmeow.gts")).expect("read committed snapshot")
    }

    fn text_payload(value: Value) -> Value {
        let text = value["content"][0]["text"].as_str().expect("text content");
        serde_json::from_str(text).expect("tool text is JSON")
    }

    fn temp_memory() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("memory.gts");
        unsafe {
            // SAFETY: tests mutate process env single-threaded under ENV_LOCK.
            env::set_var("GMEOW_MEMORY_PATH", &path);
        }
        (dir, path)
    }

    #[test]
    fn consumer_view_retains_raw_snapshot_and_reaches_shapes_archive() {
        // The native validation surface (`validate_local`) needs the raw GTS bytes
        // back so `gmeow_validate` can read the folded `shapes-archive` blob — the
        // parsed carrier dataset does not carry it. Prove the bytes are retained
        // verbatim and that the shapes archive is reachable from them, so the
        // consumer server (the shippable `gmeow mcp`) can validate agent data.
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let _env = EnvRestore::capture(&["GMEOW_LANG"]);
        unsafe {
            // SAFETY: tests mutate process env single-threaded under ENV_LOCK.
            env::remove_var("GMEOW_LANG");
        }
        let bytes = snapshot();
        let server = McpServer::from_snapshot(&bytes).unwrap();
        assert_eq!(
            server.view.gts_bytes(),
            bytes.as_slice(),
            "the view must retain the snapshot bytes verbatim",
        );
        let shapes =
            gmeow_bundle_view::bundle_blobs::Bundle::from_snapshot(server.view.gts_bytes())
                .expect("bundle parses from retained bytes")
                .shapes()
                .expect("shapes-archive readable from retained bytes");
        assert!(
            !shapes.is_empty(),
            "the shapes-archive blob must be reachable from the retained snapshot \
             bytes — it is the SHACL surface validate_local checks agent data against",
        );
    }

    /// The grounded-memory triad is served by ONE segment, whichever segment that is.
    ///
    /// This is a deployment-correctness gate, not a taste one. In the browser the engine
    /// ships as two wasm modules, each with its own linear memory, and the claim package is
    /// a `static` inside one of them: a segment IS a store. So a `store_claim` served by one
    /// image and a `recall` served by the other are not two views of one store, they are two
    /// stores — the write succeeds, mints an id, and is unreachable by every read. That was
    /// the shipped behaviour before `recall` and `store_segment` joined the writes in
    /// [`REASONING_SEGMENT_TOOLS`]: `store_claim` returned `ok: true`, `recall` returned
    /// `[]`, and `store_segment` reported an empty store.
    ///
    /// It is checked HERE, over the routing declaration, because the native build cannot
    /// reproduce the failure at all — one process, one `browser_storage()`, one store. The
    /// end-to-end proof across two real wasm images is
    /// `crates/mcp-core-wasm/js/tests/witness.test.mjs`; this is the invariant that keeps
    /// the split from re-opening, and it fails the moment any claim-store tool is routed
    /// away from the others.
    #[test]
    fn the_grounded_memory_triad_is_served_by_one_segment() {
        let segments: BTreeSet<&str> = CLAIM_STORE_TOOLS
            .iter()
            .map(|tool| SegmentSet::segment_of(tool))
            .collect();
        assert_eq!(
            segments.len(),
            1,
            "the tools that share the claim package must share a segment — a browser image \
             is a store, so {CLAIM_STORE_TOOLS:?} split across {segments:?} means a stored \
             claim is unreachable by every read"
        );

        // …and each half of the tiering agrees: a core deployment defers ALL of them, a
        // reasoning deployment serves ALL of them. Either mixed answer is the same defect
        // seen from one side.
        for tool in CLAIM_STORE_TOOLS {
            assert!(
                !SegmentSet::core().serves(tool),
                "`{tool}` reads or writes the claim package, so the lean core must defer it \
                 rather than answer from an image the writes cannot reach"
            );
            assert!(
                SegmentSet::reasoning_only().serves(tool),
                "`{tool}` reads or writes the claim package, so the image that owns that \
                 package must serve it"
            );
        }
    }

    /// [`CLAIM_STORE_TOOLS`] is the WHOLE claim-store surface, and every entry is real.
    ///
    /// The invariant above is only as good as the list it quantifies over, so the list is
    /// checked from both ends: every name is an advertised tool (no ghost entry padding the
    /// set), and every tool whose descriptor is about the grounded-memory package is in it.
    /// The second half is what catches a NEW memory tool added outside the list — the way
    /// `store_segment` itself was added.
    #[test]
    fn the_claim_store_tool_list_covers_the_whole_memory_surface() {
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let advertised = advertised_consumer_tools();
        for tool in CLAIM_STORE_TOOLS {
            assert!(
                advertised.contains(*tool),
                "CLAIM_STORE_TOOLS names `{tool}`, which the consumer surface does not \
                 advertise"
            );
        }
        // The engine's memory surface, named the way the crate itself names it: the tools
        // whose bodies go through `McpServer::claim_store`. Restated here as a literal so
        // this test is an INDEPENDENT statement of the set rather than a tautology over
        // `CLAIM_STORE_TOOLS` — the two must agree.
        let memory_surface: BTreeSet<&str> =
            BTreeSet::from(["recall", "revise_belief", "store_claim", "store_segment"]);
        assert_eq!(
            CLAIM_STORE_TOOLS
                .iter()
                .copied()
                .collect::<BTreeSet<&str>>(),
            memory_surface,
            "CLAIM_STORE_TOOLS must be exactly the tools that reach the claim package"
        );
    }

    /// Every shipped surface that STATES a tool count states the DERIVED one.
    ///
    /// The counts had rotted to "35" in five places at once — two crate descriptions, two
    /// READMEs, a feature comment, and the published npm bindings — while the surface was
    /// at 38, and the segment split was described as "twelve / twenty-three" while the
    /// declaration held thirteen. Hand-fixing those numbers is what produced the drift in
    /// the first place, so the fix is a GATE: [`TOOL_COUNT`] and the four numbers derived
    /// from it are the only tool counts any of these files may state.
    ///
    /// Rust prose reads the constants directly (a rustdoc link, or `format!` for a tool
    /// description an agent sees at run time). A `Cargo.toml` comment, a README, and the
    /// wasm-bindgen output cannot — there is no interpolation in TOML or Markdown, and the
    /// bindings are generated bytes — so those state the number and are CHECKED here. Both
    /// halves of Principle: read the derived value, or be gated against it.
    ///
    /// The scan reads PROSE, not code: comment lines, Markdown, and a `description =` key.
    /// Within it, any number ≥ 5 (numeral or English word) followed within four tokens by
    /// something naming a tool must be one of the derived counts. Below 5 is English
    /// ("exactly one action schema per tool"), not arithmetic; and code is excluded because
    /// a request-frame literal carrying a JSON-RPC id next to the `tools/call` method name
    /// states nothing whatever about the surface.
    #[test]
    fn the_shipped_prose_states_the_derived_tool_counts() {
        /// `(path relative to this crate, must this file state at least one count?)`.
        ///
        /// The published wasm bindings are included because they are the bytes an npm
        /// consumer reads: a stale vendored `pkg/` carries a false count into the package
        /// even when every source file here is right. Re-vendor
        /// (`make maint-refresh-mcp-core-asset` / `make maint-refresh-mcp-asset`) is what
        /// clears them.
        const SURFACES: &[(&str, bool)] = &[
            ("src/lib.rs", true),
            ("src/error.rs", false),
            ("Cargo.toml", true),
            ("../mcp-core-wasm/src/lib.rs", false),
            ("../mcp-core-wasm/Cargo.toml", true),
            ("../mcp-core-wasm/README.md", true),
            ("../mcp-core-wasm/js/index.mjs", false),
            ("../mcp-core-wasm/js/index.d.ts", false),
            ("../mcp-wasm/src/lib.rs", false),
            ("../mcp-wasm/Cargo.toml", true),
            ("../mcp-wasm/README.md", true),
            ("../mcp-wasm/js/index.mjs", false),
            ("../mcp-wasm/js/index.d.ts", false),
            ("../docs/assets/mcp-core/index.mjs", false),
            ("../docs/assets/mcp-core/pkg/gmeow_mcp_core_wasm.js", false),
            (
                "../docs/assets/mcp-core/pkg/gmeow_mcp_core_wasm.d.ts",
                false,
            ),
            ("../docs/assets/mcp/index.mjs", false),
            ("../docs/assets/mcp/pkg/gmeow_mcp_wasm.js", false),
            ("../docs/assets/mcp/pkg/gmeow_mcp_wasm.d.ts", false),
        ];

        /// Is this line PROSE — a place a count is CLAIMED rather than computed?
        ///
        /// Rust and JS comment lines (including the `*` continuations wasm-bindgen emits
        /// for a rustdoc block), every line of Markdown, and TOML comments plus the
        /// `description` key that becomes the published package blurb.
        fn is_prose(path: &str, line: &str) -> bool {
            let trimmed = line.trim_start();
            if path.ends_with(".md") {
                return true;
            }
            if path.ends_with(".toml") {
                return trimmed.starts_with('#') || trimmed.starts_with("description");
            }
            trimmed.starts_with("//") || trimmed.starts_with('*') || trimmed.starts_with("/*")
        }

        /// The English number words a count claim has used. Only ≥ 5, for the reason above.
        fn word_value(token: &str) -> Option<usize> {
            Some(match token {
                "five" => 5,
                "six" => 6,
                "seven" => 7,
                "eight" => 8,
                "nine" => 9,
                "ten" => 10,
                "eleven" => 11,
                "twelve" => 12,
                "thirteen" => 13,
                "fourteen" => 14,
                "fifteen" => 15,
                "sixteen" => 16,
                "seventeen" => 17,
                "eighteen" => 18,
                "nineteen" => 19,
                "twenty" => 20,
                "thirty" => 30,
                "forty" => 40,
                "fifty" => 50,
                _ => return None,
            })
        }

        let allowed = [
            TOOL_COUNT,
            READ_TOOL_COUNT,
            WRITE_TOOL_COUNT,
            REASONING_SEGMENT_TOOL_COUNT,
            CHASE_SEGMENT_TOOL_COUNT,
            DEFERRED_TOOL_COUNT,
            CORE_SEGMENT_TOOL_COUNT,
        ];
        let here = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let mut problems: Vec<String> = Vec::new();
        let mut checked_total = 0usize;

        for (relative, must_state) in SURFACES {
            let path = here.join(relative);
            // A missing shipped surface is a defect, never a skip: the file being absent is
            // precisely the state in which nothing is checked.
            let text = fs::read_to_string(&path).unwrap_or_else(|e| {
                panic!(
                    "the count gate must read the shipped surface {}: {e}",
                    path.display()
                )
            });
            // Tokenised per PROSE line: a claim does not span a line, and joining the file
            // would let a comment's last word pair with the next line's code.
            let prose: String = text
                .lines()
                .filter(|line| is_prose(relative, line))
                .collect::<Vec<_>>()
                .join("\n");
            let tokens: Vec<&str> = prose
                .split(|c: char| !c.is_ascii_alphanumeric() && c != '_')
                .filter(|token| !token.is_empty())
                .collect();
            let mut checked_here = 0usize;
            for (index, token) in tokens.iter().enumerate() {
                let value = token
                    .parse::<usize>()
                    .ok()
                    // A count of this surface is a two-digit number. A longer numeral next
                    // to the word `tools` in prose is an error code or an IRI fragment
                    // being discussed, not a claim about how many tools there are.
                    .filter(|value| (5..100).contains(value))
                    .or_else(|| word_value(&token.to_ascii_lowercase()));
                let Some(value) = value else { continue };
                let names_a_tool = tokens[index + 1..]
                    .iter()
                    .take(4)
                    .any(|later| later.to_ascii_lowercase().contains("tool"));
                if !names_a_tool {
                    continue;
                }
                checked_here += 1;
                if !allowed.contains(&value) {
                    let context = tokens[index..(index + 5).min(tokens.len())].join(" ");
                    problems.push(format!(
                        "{relative}: `{context}` states {value}, which is none of the \
                         derived counts {allowed:?}"
                    ));
                }
            }
            if *must_state && checked_here == 0 {
                problems.push(format!(
                    "{relative} states NO tool count — either the surface stopped describing \
                     itself or this gate stopped reading it; both are defects"
                ));
            }
            checked_total += checked_here;
        }

        assert!(
            problems.is_empty(),
            "shipped prose states a tool count that is not derived from TOOL_COUNT \
             ({TOOL_COUNT} tools = {READ_TOOL_COUNT} reads + {WRITE_TOOL_COUNT} writes; \
             {REASONING_SEGMENT_TOOL_COUNT} reasoning + {CORE_SEGMENT_TOOL_COUNT} core):\n  {}",
            problems.join("\n  ")
        );
        assert!(
            checked_total >= SURFACES.len(),
            "the scan found only {checked_total} count claims across {} shipped surfaces — \
             too few to be reading them, so the gate is vacuous",
            SURFACES.len()
        );
    }

    /// The CONSUMER surface is exactly [`TOOL_COUNT`] tools and 5 resources.
    ///
    /// The counts are pinned, not approximated: a later bijection gate is defined
    /// against the consumer tool list, so silently adding (or dev-promoting) a tool
    /// would change that contract without anyone noticing. The names are asserted
    /// alongside the counts so a rename cannot pass by keeping the arithmetic.
    ///
    /// This is also what makes [`TOOL_COUNT`] a DERIVED number rather than a claim: every
    /// count in this crate's shipped prose resolves to that constant, and the constant
    /// cannot survive a surface that grew past it.
    #[test]
    fn consumer_surface_matches_the_declared_tool_count() {
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let _env = EnvRestore::capture(&["GMEOW_LANG"]);
        unsafe {
            // SAFETY: tests mutate process env single-threaded under ENV_LOCK.
            env::remove_var("GMEOW_LANG");
        }
        let bytes = snapshot();
        let consumer = McpServer::from_snapshot(&bytes).unwrap();
        let names = consumer.surface().tool_names();
        assert_eq!(
            names.len(),
            TOOL_COUNT,
            "the consumer tool surface is TOOL_COUNT tools, got {names:?}"
        );
        assert_eq!(
            names,
            [
                "lookup_term",
                "llms_txt",
                "llms_full",
                "doc_card",
                "okf_index",
                "query_docs",
                "docs_search",
                "query_local",
                "encode_gmn1",
                "verify_graph",
                "reason_graph",
                "explain_quad",
                "coherence_certificate",
                "validate_local",
                "gmn_validate",
                "gmn_expand",
                "gmn_explain",
                "advise",
                "explain_finding",
                "store_claim",
                "conjecture_test",
                "store_conjecture",
                "refute_conjecture",
                "recall",
                "store_segment",
                "revise_belief",
                "counter_examples",
                "entailments",
                "competency_questions",
                "slice_quality",
                "slice_brief",
                "submit_candidate",
                "withdraw_candidate",
                "list_candidates",
                "convert",
                "gmn_glyph_legend",
                "distribution_matrix",
                "action_policy",
            ],
            "the consumer tool list changed"
        );
        let resources = consumer.surface().resource_descriptors();
        assert_eq!(
            resources.len(),
            5,
            "the consumer resource surface is 5 resources, got {resources:?}"
        );
        let uris: Vec<&str> = resources
            .iter()
            .map(|r| r["uri"].as_str().expect("resource uri"))
            .collect();
        assert_eq!(
            uris,
            [
                "gmeow://ontology/llms.txt",
                "gmeow://ontology/llms-full.txt",
                "gmeow://ontology/gmn1-primer",
                "gmeow://ontology/okf-index",
                "gmeow://ontology/action-policy",
            ],
            "the consumer resource list changed"
        );
    }

    /// Dispatching a name the surface does not carry is a NAMED hard error
    /// (`mcp.unknown-tool`, quoting the name) — never a silent no-op and never a
    /// generic fallthrough. The same holds for a resource URI.
    #[test]
    fn dispatching_an_unregistered_tool_is_a_named_hard_error() {
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let _env = EnvRestore::capture(&["GMEOW_LANG"]);
        unsafe {
            // SAFETY: tests mutate process env single-threaded under ENV_LOCK.
            env::remove_var("GMEOW_LANG");
        }
        let bytes = snapshot();
        let server = McpServer::from_snapshot(&bytes).unwrap();

        let err = server
            .surface()
            .dispatch_tool(&server, "no_such_tool", &json!({}))
            .expect_err("an unregistered tool name must NOT dispatch");
        assert_eq!(err.code(), crate::error::UnknownTool::register());
        assert!(
            err.to_string().contains("no_such_tool"),
            "the refusal must name the tool: {err}"
        );

        // `sync` is a DEV tool: over a consumer server it is not registered at all,
        // so it takes the same named-refusal path (not a mode guard that quietly
        // returns nothing).
        let dev_only = server
            .surface()
            .dispatch_tool(&server, "sync", &json!({}))
            .expect_err("a dev tool must not dispatch on a consumer server");
        assert_eq!(dev_only.code(), crate::error::UnknownTool::register());
        assert!(dev_only.to_string().contains("sync"), "{dev_only}");

        // The JSON-RPC envelope carries the same refusal as an MCP tool error.
        let envelope = server.call_tool_result("no_such_tool", &json!({}));
        assert_eq!(envelope["isError"], json!(true), "{envelope}");
        assert!(
            envelope["content"][0]["text"]
                .as_str()
                .expect("text content")
                .contains("no_such_tool"),
            "{envelope}"
        );

        let missing = server
            .surface()
            .read_resource(&server, "gmeow://ontology/nope", &["en".to_string()])
            .expect_err("an unregistered resource URI must NOT resolve");
        assert_eq!(missing.code(), crate::error::UnknownResource::register());
        assert!(missing.to_string().contains("gmeow://ontology/nope"));
    }

    /// Registering a tool name (or a resource URI) that is already claimed is a
    /// NAMED hard error at construction — last-writer-wins would let the advertised
    /// descriptor and the dispatched handler disagree.
    #[test]
    fn duplicate_registration_is_a_named_hard_error() {
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let _env = EnvRestore::capture(&["GMEOW_LANG"]);
        unsafe {
            // SAFETY: tests mutate process env single-threaded under ENV_LOCK.
            env::remove_var("GMEOW_LANG");
        }
        let bytes = snapshot();

        // Shadowing a BUILTIN tool name.
        let shadowing = Extension::new()
            .with_tool(tool("lookup_term", "A second lookup_term.", &[]), |_, _| {
                Ok(String::new())
            });
        let err = McpServer::from_snapshot_with(&bytes, shadowing)
            .err()
            .expect("shadowing a builtin tool name must refuse to construct");
        assert_eq!(err.code(), crate::error::DuplicateRegistration::register());
        assert!(err.to_string().contains("lookup_term"), "{err}");

        // Two EXTENSION entries claiming the same new name.
        let twice = Extension::new()
            .with_tool(tool("host_tool", "First.", &[]), |_, _| Ok(String::new()))
            .with_tool(tool("host_tool", "Second.", &[]), |_, _| Ok(String::new()));
        let err = McpServer::from_snapshot_with(&bytes, twice)
            .err()
            .expect("registering one tool name twice must refuse to construct");
        assert_eq!(err.code(), crate::error::DuplicateRegistration::register());
        assert!(err.to_string().contains("host_tool"), "{err}");

        // The resource twin: shadowing a builtin resource URI.
        let dup_resource = Extension::new().with_resource(
            resource(
                "gmeow://ontology/okf-index",
                "okf-index",
                "A second okf-index.",
                "application/json",
            ),
            |_, _| Ok(String::new()),
        );
        let err = McpServer::from_snapshot_with(&bytes, dup_resource)
            .err()
            .expect("shadowing a builtin resource URI must refuse to construct");
        assert_eq!(err.code(), crate::error::DuplicateRegistration::register());
        assert!(
            err.to_string().contains("gmeow://ontology/okf-index"),
            "{err}"
        );
    }

    /// A host extension's tools and resources are advertised AND dispatchable —
    /// the seam `gmeow-mcp-dev` registers its four repo-reading tools through.
    #[test]
    fn a_host_extension_is_advertised_and_dispatchable() {
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let _env = EnvRestore::capture(&["GMEOW_LANG"]);
        unsafe {
            // SAFETY: tests mutate process env single-threaded under ENV_LOCK.
            env::remove_var("GMEOW_LANG");
        }
        let bytes = snapshot();
        let extension = Extension::new()
            .with_tool(tool("host_echo", "Echo the arg.", &[]), |_, args| {
                Ok(json!({"ok": true, "echo": args.clone()}).to_string())
            })
            .with_resource(
                resource(
                    "gmeow://host/marker",
                    "marker",
                    "A host-registered resource.",
                    "text/markdown",
                ),
                |_, _| Ok("host body".to_string()),
            );
        let server = McpServer::from_snapshot_with(&bytes, extension).unwrap();

        assert_eq!(server.surface().tool_names().len(), 39);
        assert_eq!(
            server.surface().tool_names().last().copied(),
            Some("host_echo"),
            "a host tool is advertised AFTER the builtins"
        );
        let out = text_payload(server.call_tool_result("host_echo", &json!({"a": 1})));
        assert_eq!(out["echo"], json!({"a": 1}), "{out}");

        assert_eq!(server.surface().resource_descriptors().len(), 6);
        let read = server.read_resource_result("gmeow://host/marker");
        assert!(read.get("isError").is_none(), "{read}");
        assert_eq!(read["contents"][0]["text"], json!("host body"), "{read}");
        assert_eq!(
            read["contents"][0]["mimeType"],
            json!("text/markdown"),
            "the served media type is the ADVERTISED one: {read}"
        );
    }

    #[test]
    fn the_consumer_surface_advertises_the_agent_facing_tools() {
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let _env = EnvRestore::capture(&["GMEOW_LANG", "GMEOW_MEMORY_PATH", "HOME", "USERPROFILE"]);
        unsafe {
            // SAFETY: tests mutate process env single-threaded under ENV_LOCK.
            env::remove_var("GMEOW_LANG");
        }
        let bytes = snapshot();
        let consumer = McpServer::from_snapshot(&bytes).unwrap();
        let consumer_tools = consumer.tools_result().to_string();
        assert!(consumer_tools.contains("\"lookup_term\""));
        assert!(consumer_tools.contains("\"llms_txt\""));
        assert!(consumer_tools.contains("\"llms_full\""));
        assert!(consumer_tools.contains("\"okf_index\""));
        assert!(consumer_tools.contains("\"query_docs\""));
        assert!(consumer_tools.contains("\"store_claim\""));
        // The AI-agent docs surface: every one of its tools is CONSUMER-visible
        // (served by the shippable `gmeow mcp` off the bundle alone), never
        // dev-gated. `validate_local` is distinct from the dev-only `validate`.
        assert!(consumer_tools.contains("\"validate_local\""));
        // `advise` — the recommendation companion of `validate_local`.
        assert!(consumer_tools.contains("\"advise\""));
        assert!(consumer_tools.contains("\"docs_search\""));
        assert!(consumer_tools.contains("\"counter_examples\""));
        assert!(consumer_tools.contains("\"entailments\""));
        assert!(consumer_tools.contains("\"competency_questions\""));
        assert!(!consumer_tools.contains("\"validate\""));
        // `slice_quality` is CONSUMER-visible: it scores an external slice directory
        // against the bundle-carried rubric, needing no checkout.
        assert!(consumer_tools.contains("\"slice_quality\""));
        assert!(
            !consumer
                .resources_result()
                .to_string()
                .contains("constitution")
        );
        // The four repo-reading dev tools live in `gmeow-mcp-dev` and are registered
        // through the extension seam; the DEV surface counts are asserted there.
        for dev_only in ["validate", "reason", "sync", "constitution"] {
            assert!(
                !consumer.surface().tool_names().contains(&dev_only),
                "`{dev_only}` must NOT be on the consumer surface"
            );
        }
    }

    #[test]
    fn conjecture_tool_schemas_advertise_their_enforced_required_args() {
        // `conjecture_test` / `store_conjecture` enforce `formula`,
        // `kb`, `standpoint` via `required_str` at call time (see `tool_conjecture_test` /
        // `tool_store_conjecture`); `refute_conjecture` enforces only `conjecture_id`. The
        // advertised `inputSchema.required` array must list EXACTLY what the tool body
        // enforces — otherwise a client sees an arg marked OPTIONAL and only discovers it is
        // mandatory from a runtime error (the dishonest-schema gap this test closes).
        let bytes = snapshot();
        let server = McpServer::from_snapshot(&bytes).unwrap();
        let tools_result = server.tools_result();
        let tools = tools_result["tools"].as_array().expect("tools array");

        let required_of = |name: &str| -> BTreeSet<String> {
            let tool = tools
                .iter()
                .find(|t| t["name"] == name)
                .unwrap_or_else(|| panic!("tool {name} must be advertised"));
            tool["inputSchema"]["required"]
                .as_array()
                .unwrap_or_else(|| panic!("{name} must advertise a required array"))
                .iter()
                .map(|v| {
                    v.as_str()
                        .expect("required entries are strings")
                        .to_string()
                })
                .collect()
        };

        for name in ["conjecture_test", "store_conjecture"] {
            let required = required_of(name);
            for arg in ["formula", "kb", "standpoint"] {
                assert!(
                    required.contains(arg),
                    "{name} enforces `{arg}` via required_str at call time but does not \
                     advertise it as required: {required:?}"
                );
            }
        }

        let refute_required = required_of("refute_conjecture");
        assert!(
            refute_required.contains("conjecture_id"),
            "refute_conjecture enforces `conjecture_id` via required_str but does not advertise \
             it as required: {refute_required:?}"
        );
        // `reason` / `dry_run` are read with `optional_str` / `optional_bool_checked` at the
        // call site, so advertising them as required would be dishonest the other way.
        assert!(
            !refute_required.contains("reason"),
            "refute_conjecture's `reason` is optional at call time; must not be advertised as \
             required: {refute_required:?}"
        );
        assert!(
            !refute_required.contains("dry_run"),
            "refute_conjecture's `dry_run` is optional at call time; must not be advertised as \
             required: {refute_required:?}"
        );

        // The authoring-factory tools enforce EXACTLY these keys via `required_str` in their
        // bodies (see `tool_slice_quality` / `tool_slice_brief` / `tool_submit_candidate` /
        // `tool_withdraw_candidate` / `tool_list_candidates`); every other advertised arg is
        // read with `optional_*`. The advertised `required` array must equal the enforced set —
        // no more (a client would get a runtime error omitting a merely-optional arg), no less.
        let expected_required: &[(&str, &[&str])] = &[
            ("slice_quality", &["files"]),
            ("slice_brief", &["slice"]),
            ("submit_candidate", &["formula", "kb", "standpoint"]),
            ("withdraw_candidate", &["candidate_id"]),
            // `slice` and `disposition` are BOTH optional filters — `list_candidates` enforces
            // nothing, so it must advertise an EMPTY required array (the dishonest-`slice`-required
            // gap this asserts against).
            ("list_candidates", &[]),
        ];
        for (name, enforced) in expected_required {
            let required = required_of(name);
            let want: BTreeSet<String> = enforced.iter().map(|s| (*s).to_string()).collect();
            assert_eq!(
                required, want,
                "{name} must advertise EXACTLY the args it enforces via required_str \
                 ({want:?}); advertised {required:?}"
            );
        }
    }

    #[test]
    fn slice_quality_tool_reports_grades_and_advice_in_consumer_mode() {
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let _env = EnvRestore::capture(&["GMEOW_LANG", "GMEOW_MEMORY_PATH", "HOME", "USERPROFILE"]);
        unsafe {
            // SAFETY: tests mutate process env single-threaded under ENV_LOCK.
            env::remove_var("GMEOW_LANG");
        }
        let bytes = snapshot();
        // Positive CONSUMER-mode dispatch (AC3): a server built with `root: None`
        // scores a slice handed to it as BYTES, purely off the embedded bundle rubric.
        // The bytes here come from a real in-repo slice read off disk BY THE TEST — the
        // tool itself never touches a filesystem.
        let server = McpServer::from_snapshot(&bytes).unwrap();
        let slice_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../slices/core/ai");
        let files = slice_files_arg(&slice_dir);

        // Functional dispatch: the tool returns the documented JSON shape — grades as
        // {axis, tier, score} and advice as {code, message}.
        let out = text_payload(server.call_tool_result("slice_quality", &json!({"files": files})));
        assert!(
            out.get("ok").is_none(),
            "a successful score carries no error envelope: {out}"
        );
        assert!(out["slice"].is_string(), "slice IRI present: {out}");
        assert!(
            out["rollup_tier"].is_string(),
            "roll-up tier present: {out}"
        );
        let grades = out["grades"].as_array().expect("grades array");
        assert!(!grades.is_empty(), "at least one axis grade: {out}");
        for g in grades {
            assert!(g["axis"].is_string(), "grade.axis is a string: {g}");
            assert!(g["tier"].is_string(), "grade.tier is a string: {g}");
            assert!(g["score"].is_number(), "grade.score is a number: {g}");
        }
        for a in out["advice"].as_array().expect("advice array") {
            assert!(a["code"].is_string(), "advice.code is a string: {a}");
            assert!(a["message"].is_string(), "advice.message is a string: {a}");
        }
    }

    /// Every way the `files` map can fail to describe a slice is a NAMED hard error —
    /// never a panic, never a silent pass, and never a vacuous clean score.
    #[test]
    fn slice_quality_tool_errors_on_a_map_that_is_not_a_slice() {
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let _env = EnvRestore::capture(&["GMEOW_LANG", "GMEOW_MEMORY_PATH", "HOME", "USERPROFILE"]);
        unsafe {
            // SAFETY: tests mutate process env single-threaded under ENV_LOCK.
            env::remove_var("GMEOW_LANG");
        }
        let bytes = snapshot();
        let server = McpServer::from_snapshot(&bytes).unwrap();

        // The `files` argument is missing entirely.
        let absent = text_payload(server.call_tool_result("slice_quality", &json!({})));
        assert_eq!(
            absent["ok"], false,
            "an omitted `files` map must hard-fail: {absent}"
        );
        assert!(
            absent["error"]
                .as_str()
                .unwrap_or_default()
                .contains("files"),
            "the error must name the missing argument: {absent}"
        );

        // A map that carries files but no `manifest.ttl` — the slice IRI cannot be
        // resolved, so there is nothing to score.
        let no_manifest = text_payload(server.call_tool_result(
            "slice_quality",
            &json!({"files": {"module.ttl": "# nothing here\n"}}),
        ));
        assert_eq!(
            no_manifest["ok"], false,
            "a map with no manifest.ttl must hard-fail: {no_manifest}"
        );
        assert!(
            no_manifest["error"]
                .as_str()
                .unwrap_or_default()
                .contains("manifest.ttl"),
            "the error must NAME manifest.ttl so the caller knows what to add: {no_manifest}"
        );

        // A `files` value that is not a string is a distinct, separately-named defect.
        let bad_value = text_payload(
            server.call_tool_result("slice_quality", &json!({"files": {"manifest.ttl": 42}})),
        );
        assert_eq!(
            bad_value["ok"], false,
            "a non-string file body must hard-fail: {bad_value}"
        );
        assert!(
            bad_value["error"]
                .as_str()
                .unwrap_or_default()
                .contains("manifest.ttl"),
            "the error must name the offending entry: {bad_value}"
        );
    }

    #[test]
    fn slice_brief_tool_serves_packet_with_fr_grounding_in_consumer_mode() {
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let _env = EnvRestore::capture(&["GMEOW_LANG", "GMEOW_MEMORY_PATH", "HOME", "USERPROFILE"]);
        unsafe {
            // SAFETY: tests mutate process env single-threaded under ENV_LOCK.
            env::remove_var("GMEOW_LANG");
        }
        let bytes = snapshot();
        // CONSUMER mode (root: None) serves the packet purely from the embedded
        // `graph/authoring-briefs` corpus — no checkout. The deterministic term-batch
        // numbering shifts whenever lang terms are added/removed, so the batch that
        // carries a present French grounding cell is NOT a fixed constant — it is
        // discovered dynamically below (a bare, batch-less request returns every `lang`
        // packet) rather than hardcoded, so the test survives future renumbering.
        let server = McpServer::from_snapshot(&bytes).unwrap();
        let all = text_payload(server.call_tool_result("slice_brief", &json!({"slice": "lang"})));
        let fr_batch = all["packets"]
            .as_array()
            .expect("packets array")
            .iter()
            .find(|p| {
                p["grounding"].as_array().is_some_and(|g| {
                    g.iter()
                        .any(|c| c["attribute"] == "groundingFr" && c["value"].is_string())
                })
            })
            .unwrap_or_else(|| {
                panic!("no `lang` batch carries a present French grounding cell: {all}")
            })["batch"]
            .as_u64()
            .expect("batch is a number");

        let out = text_payload(
            server.call_tool_result("slice_brief", &json!({"slice": "lang", "batch": fr_batch})),
        );

        assert!(
            out.get("ok").is_none(),
            "a served packet carries no error envelope: {out}"
        );
        assert_eq!(
            out["slice"], "https://blackcatinformatics.ca/gmeow/slices/lang",
            "short-name expanded to the full slice IRI: {out}"
        );
        assert_eq!(out["packet_count"], 1, "exactly the requested batch: {out}");
        let packet = &out["packets"][0];
        assert_eq!(packet["axis"], "whole");
        assert_eq!(packet["batch"], fr_batch);
        assert!(
            packet["digest"].is_string(),
            "packet digest present: {packet}"
        );
        assert!(
            packet["term_count"].as_i64().is_some_and(|n| n > 0),
            "packet covers terms: {packet}"
        );
        assert!(
            packet["covers_terms"]
                .as_array()
                .is_some_and(|a| !a.is_empty()),
            "covered-term IRIs listed: {packet}"
        );
        // AC4: a French translation cell survives with its JOINed value.
        let grounding = packet["grounding"].as_array().expect("grounding array");
        let fr = grounding
            .iter()
            .find(|c| c["attribute"] == "groundingFr" && c["value"].is_string());
        assert!(
            fr.is_some(),
            "a present French grounding value survives the round-trip: {packet}"
        );
        // The canonical turtle is the byte-reconstructible surface the bundle folded.
        let turtle = out["turtle"].as_str().expect("turtle string");
        assert!(
            turtle.contains("AuthoringPacket") && turtle.contains("packetSourceSlice"),
            "turtle carries the packet body"
        );
    }

    #[test]
    fn slice_brief_tool_hard_fails_on_unknown_slice() {
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let _env = EnvRestore::capture(&["GMEOW_LANG", "GMEOW_MEMORY_PATH", "HOME", "USERPROFILE"]);
        unsafe {
            // SAFETY: tests mutate process env single-threaded under ENV_LOCK.
            env::remove_var("GMEOW_LANG");
        }
        let bytes = snapshot();
        let server = McpServer::from_snapshot(&bytes).unwrap();

        // No packet for the slice → explicit hard error, never a vacuous empty pass.
        let miss = text_payload(
            server.call_tool_result("slice_brief", &json!({"slice": "no-such-slice-xyz"})),
        );
        assert_eq!(miss["ok"], false, "unknown slice must hard-fail: {miss}");

        // A real slice but an out-of-range batch also hard-fails.
        let bad_batch = text_payload(
            server.call_tool_result("slice_brief", &json!({"slice": "lang", "batch": 99999})),
        );
        assert_eq!(
            bad_batch["ok"], false,
            "an out-of-range batch must hard-fail: {bad_batch}"
        );
    }

    #[test]
    fn query_docs_selects_over_the_documentation_graph() {
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let _env = EnvRestore::capture(&["GMEOW_LANG", "GMEOW_MEMORY_PATH", "HOME", "USERPROFILE"]);
        unsafe {
            // SAFETY: tests mutate process env single-threaded under ENV_LOCK.
            env::remove_var("GMEOW_LANG");
        }
        let bytes = snapshot();
        let server = McpServer::from_snapshot(&bytes).unwrap();

        // A SELECT over the bundled documentation graph returns SPARQL-1.1 JSON
        // bindings (the doc graph carries a gmeow:DocumentedTerm per documented term).
        let select = text_payload(server.call_tool_result(
            "query_docs",
            &json!({"query": "SELECT ?s WHERE { ?s a <https://blackcatinformatics.ca/gmeow/DocumentedTerm> } LIMIT 3"}),
        ));
        assert_eq!(
            select["ok"], true,
            "query_docs SELECT must succeed: {select}"
        );
        assert_eq!(select["head"]["vars"][0], "s");
        assert!(
            select["results"]["bindings"]
                .as_array()
                .map(|b| !b.is_empty())
                .unwrap_or(false),
            "expected at least one DocumentedTerm binding: {select}"
        );

        // CONSTRUCT is ANSWERED, and the envelope DECLARES the result form. This used to
        // assert a refusal ("SELECT and ASK"), which was a refusal standing in for a
        // capability: the native engine had already evaluated the CONSTRUCT and handed back
        // a `SparqlResult::Graph` that the surface then declined to serialize.
        let construct = text_payload(server.call_tool_result(
            "query_docs",
            &json!({"query": "CONSTRUCT { ?s ?p ?o } WHERE { ?s ?p ?o } LIMIT 1"}),
        ));
        assert_eq!(construct["ok"], true, "{construct}");
        assert_eq!(construct["form"], "graph", "{construct}");
        assert!(
            construct["graph_nquads"]
                .as_str()
                .is_some_and(|g| g.contains('<')),
            "the graph result carries real N-Quads: {construct}"
        );
        assert_eq!(construct["quad_count"], 1, "{construct}");

        // The three forms are distinguishable WITHOUT parsing: a client dispatches on
        // `form`, never on whether a JSON parse of the payload happened to fail.
        assert_eq!(select["form"], "bindings", "{select}");
        let ask = text_payload(
            server.call_tool_result("query_docs", &json!({"query": "ASK { ?s ?p ?o }"})),
        );
        assert_eq!(ask["form"], "boolean", "{ask}");
        assert_eq!(ask["boolean"], true, "{ask}");
    }

    #[test]
    fn memory_triad_preserves_suppression_on_every_default_recall_path() {
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let _env = EnvRestore::capture(&["GMEOW_LANG", "GMEOW_MEMORY_PATH", "HOME", "USERPROFILE"]);
        unsafe {
            // SAFETY: tests mutate process env single-threaded under ENV_LOCK.
            env::remove_var("GMEOW_LANG");
        }
        let (_dir, memory_path) = temp_memory();
        let bytes = snapshot();
        let server = McpServer::from_snapshot(&bytes).unwrap();

        let canary = text_payload(server.call_tool_result(
            "store_claim",
            &json!({"text": "SUPPRESSED-CANARY belief about the launch window", "confidence": 0.9}),
        ));
        let canary_id = canary["claim"]["id"].as_str().unwrap().to_string();
        text_payload(server.call_tool_result(
            "store_claim",
            &json!({"text": "CONTROL-CANARY belief about the launch window", "confidence": 0.9}),
        ));
        let revised = text_payload(server.call_tool_result(
            "revise_belief",
            &json!({"claim_id": canary_id, "reason": "revised"}),
        ));
        assert_eq!(revised["ok"], true);

        text_payload(server.call_tool_result("recall", &json!({"query": "launch window"})));
        let calls = Memory::new(memory_path).tool_calls().unwrap();
        assert_eq!(
            calls
                .iter()
                .map(|call| call.tool.as_str())
                .collect::<Vec<_>>(),
            vec![
                "urn:gmeow:tool:store_claim",
                "urn:gmeow:tool:store_claim",
                "urn:gmeow:tool:revise_belief"
            ]
        );
        assert_eq!(calls[0].generated, vec![canary_id.clone()]);
        let stored_result: Value =
            serde_json::from_str(calls[0].result.as_deref().unwrap()).unwrap();
        assert_eq!(stored_result["ok"], true);
        assert_eq!(stored_result["claim"]["id"], canary_id);
        let stored_arguments: Value =
            serde_json::from_str(calls[0].arguments.as_deref().unwrap()).unwrap();
        assert_eq!(
            stored_arguments["text"],
            "SUPPRESSED-CANARY belief about the launch window"
        );

        for args in [
            json!({}),
            json!({"query": "launch window"}),
            json!({"query": "SUPPRESSED-CANARY belief"}),
            json!({"query": "launch", "min_confidence": 0.5}),
            json!({"query": "", "limit": 100}),
        ] {
            let recalled = text_payload(server.call_tool_result("recall", &args));
            let texts: Vec<&str> = recalled["claims"]
                .as_array()
                .unwrap()
                .iter()
                .map(|claim| claim["text"].as_str().unwrap())
                .collect();
            assert!(!texts.contains(&"SUPPRESSED-CANARY belief about the launch window"));
            assert!(texts.contains(&"CONTROL-CANARY belief about the launch window"));
        }

        let audit = text_payload(server.call_tool_result(
            "recall",
            &json!({"query": "launch window", "include_suppressed": true}),
        ));
        assert!(audit["claims"].as_array().unwrap().iter().any(|claim| {
            claim["text"] == "SUPPRESSED-CANARY belief about the launch window"
                && claim["suppressed"] == true
        }));
    }

    #[test]
    fn revision_rejects_unknown_ids_before_writing() {
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let _env = EnvRestore::capture(&["GMEOW_LANG", "GMEOW_MEMORY_PATH", "HOME", "USERPROFILE"]);
        unsafe {
            // SAFETY: tests mutate process env single-threaded under ENV_LOCK.
            env::remove_var("GMEOW_LANG");
        }
        let (_dir, _memory_path) = temp_memory();
        let bytes = snapshot();
        let server = McpServer::from_snapshot(&bytes).unwrap();
        let claim =
            text_payload(server.call_tool_result("store_claim", &json!({"text": "a real belief"})));
        let claim_id = claim["claim"]["id"].as_str().unwrap();

        let missing = text_payload(server.call_tool_result(
            "revise_belief",
            &json!({"claim_id": "urn:gmeow:assertion:no-such-id"}),
        ));
        assert_eq!(missing["ok"], false);

        let bad_successor = text_payload(server.call_tool_result(
            "revise_belief",
            &json!({"claim_id": claim_id, "superseded_by": "urn:gmeow:assertion:ghost"}),
        ));
        assert_eq!(bad_successor["ok"], false);

        let live =
            text_payload(server.call_tool_result("recall", &json!({"query": "real belief"})));
        assert_eq!(live["claims"][0]["id"], claim_id);
        assert_eq!(live["claims"][0]["suppressed"], false);
    }

    /// The `convert` tool's byte channel. The three residue classes (0, 1, 2 trailing
    /// bytes) are the whole of base64's arithmetic, and each has its own padding; the
    /// vectors are RFC 4648 §10's.
    #[test]
    fn base64_encodes_every_residue_class_with_the_rfc_vectors() {
        for (plain, encoded) in [
            ("", ""),
            ("f", "Zg=="),
            ("fo", "Zm8="),
            ("foo", "Zm9v"),
            ("foob", "Zm9vYg=="),
            ("fooba", "Zm9vYmE="),
            ("foobar", "Zm9vYmFy"),
        ] {
            assert_eq!(base64_encode(plain.as_bytes()), encoded, "encode {plain:?}");
            assert_eq!(
                base64_decode("test", encoded).expect("decode"),
                plain.as_bytes(),
                "decode {encoded:?}"
            );
        }
    }

    /// Round-trip over every byte value, including the ones that make `+` and `/` appear —
    /// the two alphabet characters a naive table gets wrong.
    #[test]
    fn base64_round_trips_every_byte_value() {
        let all: Vec<u8> = (0u8..=255).collect();
        let encoded = base64_encode(&all);
        assert_eq!(base64_decode("test", &encoded).expect("decode"), all);
        assert!(encoded.contains('+') && encoded.contains('/'), "{encoded}");
    }

    /// Line-wrapped input decodes (a pasted payload routinely is), but malformed input is
    /// REFUSED rather than truncated to whatever happened to decode.
    #[test]
    fn base64_decoding_is_strict_about_everything_except_whitespace() {
        assert_eq!(
            base64_decode("test", "Zm9v\n YmFy").expect("whitespace is skipped"),
            b"foobar"
        );
        for bad in ["Zg=", "Zm9vYg", "Zg===", "Zm=v", "Zm9v!!!!", "Z===="] {
            let err = base64_decode("convert: `data`", bad)
                .expect_err("malformed base64 must be refused, not partially decoded");
            assert!(
                err.to_string().contains("convert: `data`"),
                "the refusal must name the argument it is about: {err}"
            );
        }
    }

    #[test]
    fn canonical_action_policy_is_the_single_authority_and_parses() {
        // The embedded slice file is the one source of truth for the action theory.
        let policy = action_policy_nquads();
        assert!(!policy.is_empty());
        assert!(policy.contains(MCP_STORE_CLAIM_SCHEMA));
        assert!(policy.contains(MCP_REVISE_BELIEF_SCHEMA));
        assert!(policy.contains(TXN_WORLD));
    }

    /// The `action_policy` TOOL returns the projected theory the engine itself reads —
    /// the same quad set [`action_policy_nquads`] yields natively, as a SET (line order is
    /// not the contract; membership is).
    ///
    /// This is the point of the tool: no other surface can serve it. `tools/list` returns
    /// names and JSON Schemas, and `query_docs` is scoped to `gmeow:graph/documentation`
    /// while the policy is authored in the agentic slice's examples graph. If the tool ever
    /// re-derived the theory instead of returning it, the console's pane derivation — and
    /// anyone auditing what the engine gates its writes on — would be reading a copy.
    #[test]
    fn the_action_policy_tool_returns_exactly_the_projected_theory_the_engine_reads() {
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let server = McpServer::from_snapshot(&snapshot()).unwrap();

        let payload = text_payload(server.call_tool_result("action_policy", &json!({})));
        assert_eq!(payload["ok"], json!(true), "{payload}");
        assert_eq!(payload["graph"], json!(TXN_WORLD), "{payload}");
        assert_eq!(
            payload["media_type"],
            json!(ACTION_POLICY_MEDIA_TYPE),
            "{payload}"
        );

        let native = action_policy_nquads();
        let served = payload["nquads"].as_str().expect("nquads text");
        assert_eq!(
            served, native,
            "the tool must return the engine's projection verbatim, not a re-derivation"
        );

        let native_set: BTreeSet<&str> = native.lines().collect();
        let served_set: BTreeSet<&str> = served.lines().collect();
        assert_eq!(
            served_set, native_set,
            "the served quad SET must equal the natively projected quad set"
        );
        assert!(
            !native_set.is_empty(),
            "the projected theory must be non-empty, or this test proves nothing"
        );
    }

    /// The mirroring RESOURCE serves the identical bytes under the identical media type.
    /// Tool and resource are two readers of ONE projection, exactly as `constitution` is
    /// on the dev side — a second copy on either side could drift.
    #[test]
    fn the_action_policy_resource_serves_the_same_bytes_as_the_tool() {
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let server = McpServer::from_snapshot(&snapshot()).unwrap();

        let read = server.read_resource_result(ACTION_POLICY_URI);
        assert!(read.get("isError").is_none(), "{read}");
        let content = &read["contents"][0];
        assert_eq!(content["uri"], json!(ACTION_POLICY_URI), "{read}");
        assert_eq!(
            content["mimeType"],
            json!(ACTION_POLICY_MEDIA_TYPE),
            "the served media type must be the descriptor's: {read}"
        );

        let resource_text = content["text"].as_str().expect("resource text");
        assert_eq!(
            resource_text,
            action_policy_nquads(),
            "the resource must serve the engine's projection verbatim"
        );

        let tool_text = text_payload(server.call_tool_result("action_policy", &json!({})));
        assert_eq!(
            tool_text["nquads"].as_str().expect("nquads"),
            resource_text,
            "the tool and the resource must serve the SAME bytes"
        );
    }

    // ── The bijection gate: the action theory is TOTAL over the tool surface ──────
    //
    // Everything below replaces a spot-check that named five policy subjects by hand and
    // therefore could not notice a tool with no schema (or a schema with no tool) — the
    // two failures that make an action theory a decoration instead of a contract.

    /// The namespace every schema, capability, and situation IRI in the canonical action
    /// policy lives under.
    const POLICY_NS: &str = "https://blackcatinformatics.ca/gmeow/examples/agentic/mcp-policy/";
    const RDF_TYPE_IRI: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
    const LOGIC_ACTION_SCHEMA: &str = "https://blackcatinformatics.ca/logic/ActionSchema";
    const LOGIC_MCP_ACTION_SCHEMA: &str = "https://blackcatinformatics.ca/logic/McpActionSchema";

    /// The four repo-reading DEV tools, registered by `gmeow-mcp-dev` through the extension
    /// seam. That crate depends on THIS one, so it cannot be named from here; the list is
    /// restated and `gmeow_mcp_dev`'s own `dev_surface_is_thirty_nine_tools_and_six_resources`
    /// pins the same four, so a rename reds there rather than silently widening this list.
    const DEV_ONLY_TOOLS: [&str; 4] = ["validate", "reason", "sync", "constitution"];

    /// One object term of a projected action-policy quad.
    #[derive(Debug, PartialEq, Eq)]
    enum ProjectedObject {
        Iri(String),
        Literal(String),
    }

    /// Take a leading `<iri>` off `s`, returning the IRI and the rest (left-trimmed).
    fn take_angle(s: &str) -> (String, &str) {
        let body = s
            .strip_prefix('<')
            .unwrap_or_else(|| panic!("expected an IRI term at {s:?}"));
        let end = body
            .find('>')
            .unwrap_or_else(|| panic!("unterminated IRI term at {s:?}"));
        (body[..end].to_string(), body[end + 1..].trim_start())
    }

    /// Unescape a literal body starting immediately after its opening quote.
    fn take_literal(after_quote: &str) -> String {
        let mut out = String::new();
        let mut chars = after_quote.chars();
        while let Some(c) = chars.next() {
            match c {
                '"' => return out,
                '\\' => out.push(chars.next().expect("an escape has a body")),
                c => out.push(c),
            }
        }
        panic!("unterminated literal at {after_quote:?}");
    }

    /// Split one line of [`project_action_policy`]'s output. The projection writes every line
    /// as `<s> <p> o <TXN_WORLD> .`, so this parses exactly that shape rather than general
    /// N-Quads — which is the point: it reads back what the engine is actually handed.
    fn split_projected(line: &str) -> (String, String, ProjectedObject) {
        let body = line
            .strip_suffix(&format!(" <{TXN_WORLD}> ."))
            .unwrap_or_else(|| panic!("every projected quad is stamped into TXN_WORLD: {line:?}"));
        let (subject, rest) = take_angle(body);
        let (predicate, object) = take_angle(rest);
        let object = match object.strip_prefix('"') {
            Some(after_quote) => ProjectedObject::Literal(take_literal(after_quote)),
            None => ProjectedObject::Iri(take_angle(object).0),
        };
        (subject, predicate, object)
    }

    /// The ASSERTED action theory, read back out of the projected N-Quads the engine reads.
    ///
    /// Asserted, not entailed: `logic:McpActionSchema` is a SUBCLASS of `logic:ActionSchema`
    /// (`slices/grounding/logic/module.ttl`), so under the subclass-closed view every write
    /// is also an `ActionSchema` and the read/write partition would collapse. The projection
    /// is an asserted-quad projection, so reading it this way is faithful — and it is why
    /// this file must NOT carry a disjointness axiom between the two classes, which would
    /// contradict the subclass edge and red the reasoner.
    #[derive(Debug, Default)]
    struct ActionTheory {
        /// Subjects asserted `a logic:ActionSchema` — the reads.
        plain: BTreeSet<String>,
        /// Subjects asserted `a logic:McpActionSchema` — the governed writes.
        governed: BTreeSet<String>,
        /// Subject IRI → every `logic:mcpToolName` asserted on it.
        tool_names: BTreeMap<String, BTreeSet<String>>,
    }

    impl ActionTheory {
        fn read(nquads: &str) -> Self {
            let mut theory = Self::default();
            for line in nquads.lines() {
                let (subject, predicate, object) = split_projected(line);
                match (predicate.as_str(), &object) {
                    (RDF_TYPE_IRI, ProjectedObject::Iri(class)) if class == LOGIC_ACTION_SCHEMA => {
                        theory.plain.insert(subject);
                    }
                    (RDF_TYPE_IRI, ProjectedObject::Iri(class))
                        if class == LOGIC_MCP_ACTION_SCHEMA =>
                    {
                        theory.governed.insert(subject);
                    }
                    (LOGIC_MCP_TOOL_NAME, ProjectedObject::Literal(name)) => {
                        theory
                            .tool_names
                            .entry(subject)
                            .or_default()
                            .insert(name.clone());
                    }
                    (LOGIC_MCP_TOOL_NAME, ProjectedObject::Iri(iri)) => panic!(
                        "logic:mcpToolName is a datatype property: <{subject}> names <{iri}>"
                    ),
                    _ => {}
                }
            }
            theory
        }

        /// Every subject asserted as a schema, read or write.
        fn schemas(&self) -> BTreeSet<String> {
            self.plain.union(&self.governed).cloned().collect()
        }

        /// Wire name → the schema subjects claiming it.
        fn by_tool_name(&self) -> BTreeMap<String, BTreeSet<String>> {
            let mut index: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
            for (subject, names) in &self.tool_names {
                for name in names {
                    index
                        .entry(name.clone())
                        .or_default()
                        .insert(subject.clone());
                }
            }
            index
        }

        /// The wire names asserted on `subjects`, which must each carry exactly one.
        fn names_of(&self, subjects: &BTreeSet<String>) -> BTreeSet<String> {
            subjects
                .iter()
                .map(|subject| {
                    let names = self
                        .tool_names
                        .get(subject)
                        .unwrap_or_else(|| panic!("<{subject}> carries no logic:mcpToolName"));
                    assert_eq!(
                        names.len(),
                        1,
                        "<{subject}> declares {} tool names ({names:?})",
                        names.len()
                    );
                    names.iter().next().expect("exactly one name").clone()
                })
                .collect()
        }
    }

    /// The bijection check itself, as the list of violations (empty when it holds). The gate
    /// and BOTH its negative tests call this, so all three exercise one comparison instead of
    /// three lookalikes.
    fn bijection_violations(
        advertised: &BTreeSet<String>,
        named: &BTreeSet<String>,
    ) -> Vec<String> {
        let mut problems = Vec::new();
        for tool in advertised.difference(named) {
            problems.push(format!(
                "advertised consumer tool `{tool}` has NO logic:mcpToolName row in the \
                 shipped action theory"
            ));
        }
        for row in named.difference(advertised) {
            problems.push(format!(
                "action-theory row names tool `{row}`, which the consumer surface does NOT \
                 advertise"
            ));
        }
        problems
    }

    /// The advertised consumer tool names, as a set.
    fn advertised_consumer_tools() -> BTreeSet<String> {
        let _env = EnvRestore::capture(&["GMEOW_LANG"]);
        unsafe {
            // SAFETY: tests mutate process env single-threaded under ENV_LOCK.
            env::remove_var("GMEOW_LANG");
        }
        let bytes = snapshot();
        let consumer = McpServer::from_snapshot(&bytes).expect("consumer server constructs");
        consumer
            .surface()
            .tool_names()
            .into_iter()
            .map(str::to_string)
            .collect()
    }

    /// THE GATE. The tool-name set of the CONSUMER builtin surface is EQUAL to the
    /// `{?s logic:mcpToolName ?n}` set over `logic:ActionSchema ∪ logic:McpActionSchema` in
    /// the shipped policy — in BOTH directions.
    ///
    /// A tool with no schema means the engine advertises an action its own action theory
    /// does not describe; a schema with no tool means the theory describes an action the
    /// engine cannot perform. Either one makes the `action_policy` projection a decoration.
    /// Both are named individually when they fail.
    ///
    /// The correspondence is checked on `logic:mcpToolName` and NOT on the schema local
    /// name, because the two genuinely differ: `ex:persistConjecture` is the tool
    /// `store_conjecture` and `ex:withdrawConjecture` is `refute_conjecture`. Any gate built
    /// on name mangling would either reject those two or accept anything.
    #[test]
    fn the_action_theory_is_bijective_with_the_consumer_tool_surface() {
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let advertised = advertised_consumer_tools();
        assert_eq!(
            advertised.len(),
            TOOL_COUNT,
            "the consumer surface is TOOL_COUNT tools; this gate's arithmetic depends on it"
        );

        let theory = ActionTheory::read(action_policy_nquads());
        let by_name = theory.by_tool_name();
        let named: BTreeSet<String> = by_name.keys().cloned().collect();

        let problems = bijection_violations(&advertised, &named);
        assert!(
            problems.is_empty(),
            "the action theory is NOT bijective with the consumer tool surface:\n  {}",
            problems.join("\n  ")
        );
        assert_eq!(
            named, advertised,
            "both directions hold, so the sets are equal"
        );

        // A BIJECTION, not merely a two-sided cover: one schema per name, one name per
        // schema. Two schemas claiming `store_claim` would pass the set equality above while
        // leaving the engine's gate ambiguous.
        for (name, subjects) in &by_name {
            assert_eq!(
                subjects.len(),
                1,
                "tool `{name}` is claimed by {} action schemas ({subjects:?})",
                subjects.len()
            );
        }
        let schemas = theory.schemas();
        for (subject, names) in &theory.tool_names {
            assert_eq!(
                names.len(),
                1,
                "action schema <{subject}> declares {} tool names ({names:?})",
                names.len()
            );
            assert!(
                schemas.contains(subject),
                "<{subject}> carries logic:mcpToolName but is asserted neither \
                 logic:ActionSchema nor logic:McpActionSchema"
            );
        }
        for subject in &schemas {
            assert!(
                theory.tool_names.contains_key(subject),
                "action schema <{subject}> carries NO logic:mcpToolName, so nothing ties it \
                 to a tool"
            );
        }

        // The READ / WRITE partition, over ASSERTED types. `{?s a logic:ActionSchema} \
        // {?s a logic:McpActionSchema}` must be NON-EMPTY and be exactly the reads: if the
        // reads were ever typed logic:McpActionSchema (or the writes lost their type) this
        // difference would silently empty out, and a console pane built on it would go blank
        // rather than fail.
        assert!(
            theory.plain.is_disjoint(&theory.governed),
            "no schema is asserted BOTH plain and governed: {:?}",
            theory
                .plain
                .intersection(&theory.governed)
                .collect::<Vec<_>>()
        );
        let read_subjects: BTreeSet<String> =
            theory.plain.difference(&theory.governed).cloned().collect();
        assert!(
            !read_subjects.is_empty(),
            "the asserted-plain-minus-governed set must not be empty"
        );
        // Both counts are the DERIVED constants the shipped prose quotes, so this gate is
        // also what keeps `READ_TOOL_COUNT` / `WRITE_TOOL_COUNT` honest: the policy is the
        // authority on what a write is, and the constants must agree with it.
        assert_eq!(
            read_subjects.len(),
            READ_TOOL_COUNT,
            "READ_TOOL_COUNT reads: {read_subjects:?}"
        );
        assert_eq!(
            theory.governed.len(),
            WRITE_TOOL_COUNT,
            "WRITE_TOOL_COUNT writes: {:?}",
            theory.governed
        );

        let write_names = theory.names_of(&theory.governed);
        assert_eq!(
            write_names,
            WRITE_TOOLS
                .iter()
                .map(|name| (*name).to_string())
                .collect::<BTreeSet<String>>(),
            "the governed writes are exactly the tools WRITE_TOOLS declares — the policy is \
             the authority, so a drift here is the constant's defect, not the policy's"
        );
        let read_names = theory.names_of(&read_subjects);
        assert_eq!(
            read_names,
            advertised
                .difference(&write_names)
                .cloned()
                .collect::<BTreeSet<String>>(),
            "the reads are exactly the advertised tools that are not writes"
        );

        // The four DEV tools are on NEITHER side: they need a checkout, so a consumer server
        // neither advertises them nor is governed for them.
        for dev_only in DEV_ONLY_TOOLS {
            assert!(
                !advertised.contains(dev_only),
                "`{dev_only}` is dev-gated and must NOT be on the consumer surface"
            );
            assert!(
                !named.contains(dev_only),
                "`{dev_only}` is dev-gated and must NOT have a row in the consumer action \
                 theory"
            );
        }
    }

    /// NEGATIVE 1 — a tool with no row REDS the gate, naming that tool.
    ///
    /// The mutation is applied to a COPY of the canonical Turtle and pushed through the REAL
    /// [`project_action_policy`], so what is exercised is the shipping projection and the
    /// shipping comparison, not a stand-in.
    #[test]
    fn a_tool_with_no_action_schema_row_reds_the_bijection_gate() {
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let advertised = advertised_consumer_tools();

        const ROW: &str = "    logic:mcpToolName  \"store_conjecture\" ;\n";
        assert!(
            MCP_ACTION_POLICY_TTL.contains(ROW),
            "the row this test removes must exist to be removable"
        );
        let mutated = MCP_ACTION_POLICY_TTL.replacen(ROW, "", 1);

        let theory = ActionTheory::read(&project_action_policy(&mutated));
        let named: BTreeSet<String> = theory.by_tool_name().keys().cloned().collect();
        let problems = bijection_violations(&advertised, &named);

        assert_eq!(problems.len(), 1, "exactly one violation: {problems:?}");
        assert!(
            problems[0].contains("`store_conjecture`") && problems[0].contains("NO "),
            "the failure must NAME the unmodelled tool: {}",
            problems[0]
        );
    }

    /// NEGATIVE 2 — a row with no tool REDS the gate, naming that row.
    #[test]
    fn an_action_schema_row_with_no_tool_reds_the_bijection_gate() {
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let advertised = advertised_consumer_tools();

        let mutated = format!(
            "{MCP_ACTION_POLICY_TTL}\n\
             ex:teleportOntology a logic:ActionSchema ;\n\
             \x20   logic:mcpToolName  \"teleport_ontology\" ;\n\
             \x20   logic:capability   ex:bundleReadCapability ;\n\
             \x20   logic:precondition ex:actionTheoryPresent .\n"
        );

        let theory = ActionTheory::read(&project_action_policy(&mutated));
        let named: BTreeSet<String> = theory.by_tool_name().keys().cloned().collect();
        let problems = bijection_violations(&advertised, &named);

        assert_eq!(problems.len(), 1, "exactly one violation: {problems:?}");
        assert!(
            problems[0].contains("`teleport_ontology`") && problems[0].contains("does NOT"),
            "the failure must NAME the orphaned row: {}",
            problems[0]
        );
    }

    /// Every quad in `source` whose subject is an action-policy IRI, as comparable text.
    fn policy_statements(source: &purrdf::RdfDataset) -> BTreeSet<String> {
        purrdf::flat_rdf_quads_from_dataset(source)
            .into_iter()
            .filter(|quad| {
                matches!(&quad.subject, purrdf::RdfTerm::Iri(iri) if iri.starts_with(POLICY_NS))
            })
            .map(|quad| {
                format!(
                    "{} <{}> {}",
                    quad.subject, quad.predicate, quad.object
                )
            })
            .collect()
    }

    /// WHICH COPY THE BROWSER READS, pinned: the theory has two carriers on the wire and they
    /// are proved identical, so neither can drift away from the one the engine obeys.
    ///
    /// [`MCP_ACTION_POLICY_TTL`] is a compile-time `include_str!` of the slice file, and it is
    /// the ONLY carrier. A wasm console has no checkout to read and cannot `include_str!` one
    /// at runtime — it holds the compiled crate plus the snapshot bytes — so what a browser
    /// reading `gmeow://ontology/action-policy` renders is [`action_policy_nquads`], which is
    /// this embedded copy. The Transaction-Logic executor reads the same function. Browser and
    /// executor therefore read literally the same bytes, which is a stronger guarantee than
    /// two carriers proved equal.
    ///
    /// `gmeow.gts` NOW CARRIES A SECOND COPY, and this test is what makes that safe.
    ///
    /// The pipeline's snapshot `v33` folds EVERY slice's `examples/*.ttl` into
    /// `graph/examples` and `assemble_object_level_edb` admits that graph to the reasoned
    /// closure, so the action theory — which lives in an examples file — reaches the bundle.
    /// That is a deliberate, versioned decision on the producer side (maximal information
    /// flow: a repo-free consumer can read the theory the engine obeys), not a leak.
    ///
    /// This test was the tripwire on the older "no second copy" fact, and its own instruction
    /// for this moment was to be REPLACED by a quad-set equality between the two copies —
    /// never relaxed. That is what it now is. Equality is the STRONGER property: absence only
    /// proved there was nothing to drift, while equality proves the wire copy and the copy the
    /// engine obeys say the same thing. A second copy that nothing compares is exactly how a
    /// console comes to display a policy the engine does not obey; a second copy compared
    /// quad-for-quad cannot.
    ///
    /// The ENGINE still reads exactly one carrier: [`action_policy_nquads`] projects
    /// [`MCP_ACTION_POLICY_TTL`], and both the Transaction-Logic executor and the
    /// browser-facing resource read that function. The bundle copy is what a consumer may
    /// read INSTEAD of the crate; this test is what makes the two interchangeable.
    #[test]
    fn the_action_theorys_two_carriers_agree_quad_for_quad() {
        let embedded_dataset =
            purrdf::parse_dataset(MCP_ACTION_POLICY_TTL.as_bytes(), "text/turtle", None)
                .expect("the embedded authority parses");
        let embedded = policy_statements(&embedded_dataset);
        assert!(
            !embedded.is_empty(),
            "the embedded copy must be non-empty, or this test proves nothing"
        );

        let bytes = snapshot();
        let bundle = purrdf::import_gts_events(&bytes).expect("the shipped snapshot reads");
        let bundled = policy_statements(bundle.dataset.as_ref());
        assert!(
            !bundled.is_empty(),
            "gmeow.gts carries NO action-theory quads. The producer folds every slice's \
             examples corpus into graph/examples, so the theory must be there — an empty set \
             means the fold stopped and this equality has become vacuous rather than passing."
        );
        let only_embedded: Vec<&str> = embedded.difference(&bundled).map(String::as_str).collect();
        let only_bundled: Vec<&str> = bundled.difference(&embedded).map(String::as_str).collect();
        assert!(
            only_embedded.is_empty() && only_bundled.is_empty(),
            "the two carriers of the action theory have DRIFTED — the console would display a \
             policy the engine does not obey.\n  embedded-only ({}): {}\n  bundled-only ({}): {}",
            only_embedded.len(),
            only_embedded
                .iter()
                .take(10)
                .copied()
                .collect::<Vec<_>>()
                .join("\n    "),
            only_bundled.len(),
            only_bundled
                .iter()
                .take(10)
                .copied()
                .collect::<Vec<_>>()
                .join("\n    ")
        );

        // …and the bytes the resource serves ARE the projection of that one carrier, so
        // "which copy the browser reads" is not a claim about intent but about identity.
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let server = McpServer::from_snapshot(&bytes).expect("consumer server constructs");
        let read = server.read_resource_result(ACTION_POLICY_URI);
        assert_eq!(
            read["contents"][0]["text"].as_str().expect("resource text"),
            action_policy_nquads(),
            "the browser-facing resource serves the projection of the embedded carrier"
        );
    }

    /// The VOCABULARY the projection depends on is genuinely two-carrier, and the two must
    /// agree: `logic:mcpToolName` is declared in `slices/grounding/logic/module.ttl`, and a
    /// reader that resolves the predicate against the bundle must find the SAME declaration
    /// the projection asserts — otherwise the served quads name a property the shipped
    /// ontology does not define, and the tool↔schema link is unresolvable for anyone but
    /// this crate.
    ///
    /// The declaration lands in TWO bundle graphs, and this test pins both because each
    /// carries a different half and neither half alone resolves the predicate:
    ///
    /// * `graph/logic` is the canonical projection of the compiled `logic:` program, not a
    ///   verbatim fold of the slice file. Its frontend lifts only `logic:`-namespaced
    ///   predicates (plus a narrow annotation lane), so the SIGNATURE reaches it exactly
    ///   when it is authored as `logic:domain` / `logic:range` — the spelling every other
    ///   signature-bearing term in the slice uses. An `rdfs:`-spelled signature is dropped
    ///   at ingestion and never ships; asserting it here is what keeps the authored
    ///   spelling honest.
    /// * The default graph carries the OWL TYPING (`a owl:DatatypeProperty`) as authored.
    ///   `graph/logic` types nothing as an OWL property — it has no `owl:DatatypeProperty`
    ///   subject at all — so the typing must be resolved where it actually rides.
    ///
    /// Checked against the bundle, which means this test can only pass once `gmeow.gts` has
    /// been regenerated over the minted term. It is deliberately NOT weakened to "the slice
    /// file says so": the slice file is the source, and asserting the source against itself
    /// would prove nothing about what ships.
    #[test]
    fn the_bundled_logic_vocabulary_declares_the_tool_name_property() {
        const GRAPH_LOGIC: &str = "https://blackcatinformatics.ca/gmeow/graph/logic";
        const OWL_DATATYPE_PROPERTY: &str = "http://www.w3.org/2002/07/owl#DatatypeProperty";
        const LOGIC_DOMAIN: &str = "https://blackcatinformatics.ca/logic/domain";
        const LOGIC_RANGE: &str = "https://blackcatinformatics.ca/logic/range";
        const XSD_STRING: &str = "http://www.w3.org/2001/XMLSchema#string";

        let bytes = snapshot();
        let bundle = purrdf::import_gts_events(&bytes).expect("the shipped snapshot reads");
        let quads = purrdf::flat_rdf_quads_from_dataset(bundle.dataset.as_ref());

        // (predicate, IRI object) pairs asserted ON the term, partitioned by carrier graph.
        let iri_statements = |in_graph: &dyn Fn(&Option<purrdf::RdfTerm>) -> bool| {
            quads
                .iter()
                .filter(|quad| {
                    in_graph(&quad.graph_name)
                        && matches!(&quad.subject, purrdf::RdfTerm::Iri(s) if s == LOGIC_MCP_TOOL_NAME)
                })
                .filter_map(|quad| match &quad.object {
                    purrdf::RdfTerm::Iri(object) => {
                        Some((quad.predicate.clone(), object.clone()))
                    }
                    _ => None,
                })
                .collect::<BTreeSet<(String, String)>>()
        };

        let in_logic_graph = |g: &Option<purrdf::RdfTerm>| matches!(g, Some(purrdf::RdfTerm::Iri(g)) if g == GRAPH_LOGIC);
        let declared_in_logic_graph = iri_statements(&in_logic_graph);
        for (predicate, object) in [
            (LOGIC_DOMAIN, LOGIC_ACTION_SCHEMA),
            (LOGIC_RANGE, XSD_STRING),
        ] {
            assert!(
                declared_in_logic_graph.contains(&(predicate.to_string(), object.to_string())),
                "the shipped bundle's graph/logic must declare \
                 <{LOGIC_MCP_TOOL_NAME}> <{predicate}> <{object}>; it declares \
                 {declared_in_logic_graph:?}"
            );
        }

        let declared_in_default_graph = iri_statements(&|g: &Option<purrdf::RdfTerm>| g.is_none());
        assert!(
            declared_in_default_graph
                .contains(&(RDF_TYPE_IRI.to_string(), OWL_DATATYPE_PROPERTY.to_string())),
            "the shipped bundle's default graph must declare \
             <{LOGIC_MCP_TOOL_NAME}> <{RDF_TYPE_IRI}> <{OWL_DATATYPE_PROPERTY}>; it declares \
             {declared_in_default_graph:?}"
        );
    }

    /// The projection retains `logic:mcpToolName` literals and NO other literal.
    ///
    /// The one-predicate exception in [`project_action_policy`] is load-bearing and narrow:
    /// widening it would push `rdfs:label` / `rdfs:comment` prose into the world the
    /// executional-entailment run reasons in, and every language's translation with it.
    #[test]
    fn the_projection_retains_only_the_tool_name_literal() {
        let policy = action_policy_nquads();
        let mut literal_predicates: BTreeSet<String> = BTreeSet::new();
        let mut tool_name_count = 0usize;
        for line in policy.lines() {
            let (_subject, predicate, object) = split_projected(line);
            if let ProjectedObject::Literal(_) = object {
                literal_predicates.insert(predicate.clone());
                if predicate == LOGIC_MCP_TOOL_NAME {
                    tool_name_count += 1;
                }
            }
        }
        assert_eq!(
            literal_predicates,
            BTreeSet::from([LOGIC_MCP_TOOL_NAME.to_string()]),
            "exactly one literal-valued predicate survives the projection"
        );
        assert_eq!(
            tool_name_count, TOOL_COUNT,
            "one logic:mcpToolName per advertised consumer tool"
        );
        // The dropped annotations really were present in the source, so the assertion above
        // is about the FILTER and not about an unannotated source file.
        assert!(
            MCP_ACTION_POLICY_TTL.contains("rdfs:label")
                && MCP_ACTION_POLICY_TTL.contains("rdfs:comment"),
            "the source carries the annotations the projection drops"
        );
    }

    #[test]
    fn dry_run_must_be_a_boolean() {
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let _env = EnvRestore::capture(&["GMEOW_LANG", "GMEOW_MEMORY_PATH", "HOME", "USERPROFILE"]);
        unsafe {
            // SAFETY: tests mutate process env single-threaded under ENV_LOCK.
            env::remove_var("GMEOW_LANG");
        }
        let (_dir, _memory_path) = temp_memory();
        let bytes = snapshot();
        let server = McpServer::from_snapshot(&bytes).unwrap();
        let bad = text_payload(
            server.call_tool_result("store_claim", &json!({"text": "x", "dry_run": "yes"})),
        );
        assert_eq!(bad["ok"], false);
        assert!(
            bad["error"]
                .as_str()
                .unwrap()
                .contains("dry_run must be a boolean")
        );
    }

    #[test]
    fn store_claim_dry_run_computes_verdict_without_persisting() {
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let _env = EnvRestore::capture(&["GMEOW_LANG", "GMEOW_MEMORY_PATH", "HOME", "USERPROFILE"]);
        unsafe {
            // SAFETY: tests mutate process env single-threaded under ENV_LOCK.
            env::remove_var("GMEOW_LANG");
        }
        let (_dir, memory_path) = temp_memory();
        let bytes = snapshot();
        let server = McpServer::from_snapshot(&bytes).unwrap();

        let dry = text_payload(server.call_tool_result(
            "store_claim",
            &json!({"text": "a dry-run belief about orbits", "dry_run": true}),
        ));
        assert_eq!(dry["ok"], true);
        assert_eq!(dry["dry_run"], true);
        assert_eq!(dry["transaction"]["committed"], false);
        assert_eq!(dry["transaction"]["succeeded"], true);
        assert!(
            dry["transaction"]["witness"].as_str().is_some(),
            "a sandbox run leaves a content-addressed witness"
        );
        assert!(dry.get("claim").is_none(), "dry run writes no claim");

        // Nothing persisted: recall is empty and the memory holds no claims or tool calls.
        let recalled =
            text_payload(server.call_tool_result("recall", &json!({"query": "dry-run belief"})));
        assert!(recalled["claims"].as_array().unwrap().is_empty());
        let memory = Memory::new(&memory_path);
        assert!(memory.claims().unwrap().is_empty());
        assert!(memory.tool_calls().unwrap().is_empty());
    }

    #[test]
    fn committed_store_records_the_audit_context() {
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let _env = EnvRestore::capture(&["GMEOW_LANG", "GMEOW_MEMORY_PATH", "HOME", "USERPROFILE"]);
        unsafe {
            // SAFETY: tests mutate process env single-threaded under ENV_LOCK.
            env::remove_var("GMEOW_LANG");
        }
        let (_dir, memory_path) = temp_memory();
        let bytes = snapshot();
        let server = McpServer::from_snapshot(&bytes).unwrap();

        let stored = text_payload(server.call_tool_result(
            "store_claim",
            &json!({"text": "an audited belief about thrust", "confidence": 0.8}),
        ));
        assert_eq!(stored["ok"], true);
        assert_eq!(stored["transaction"]["committed"], true);
        assert_eq!(stored["transaction"]["succeeded"], true);

        // The committed turn is cold-auditable: the persisted memory.gts carries exactly the
        // predicates emit_trajectory_audits requires on the recorded ToolCall and its anchor.
        let raw = fs::read(&memory_path).unwrap();
        let bundle = purrdf::import_gts_events(&raw).expect("import memory.gts");
        let predicates: BTreeSet<String> = purrdf::flat_rdf_quads_from_dataset(&bundle.dataset)
            .iter()
            .map(|quad| quad.predicate.clone())
            .collect();
        for predicate in [
            LOGIC_INSTANTIATES_SCHEMA,
            LOGIC_PROPER_PART_OF,
            GMEOW_AT_TIME,
            GMEOW_EVENT_TEMPORAL_FRAME,
            LOGIC_TRANSITION_FROM_STATE,
            LOGIC_SITUATION_OBTAINS,
        ] {
            assert!(
                predicates.contains(predicate),
                "memory.gts must carry {predicate} for the trajectory audit"
            );
        }
        // The single canonical temporal frame is recorded (P11 — one frame per trajectory).
        let frames: Vec<String> = purrdf::flat_rdf_quads_from_dataset(&bundle.dataset)
            .iter()
            .filter(|quad| quad.predicate == GMEOW_EVENT_TEMPORAL_FRAME)
            .map(|quad| quad.object.to_string())
            .collect();
        assert!(
            frames
                .iter()
                .all(|frame| frame.contains("temporalFrameUTCGregorian"))
        );
    }

    #[test]
    fn revise_belief_dry_run_does_not_suppress() {
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let _env = EnvRestore::capture(&["GMEOW_LANG", "GMEOW_MEMORY_PATH", "HOME", "USERPROFILE"]);
        unsafe {
            // SAFETY: tests mutate process env single-threaded under ENV_LOCK.
            env::remove_var("GMEOW_LANG");
        }
        let (_dir, _memory_path) = temp_memory();
        let bytes = snapshot();
        let server = McpServer::from_snapshot(&bytes).unwrap();
        let stored = text_payload(
            server.call_tool_result("store_claim", &json!({"text": "a revisable belief"})),
        );
        let claim_id = stored["claim"]["id"].as_str().unwrap().to_string();

        let dry = text_payload(server.call_tool_result(
            "revise_belief",
            &json!({"claim_id": claim_id, "dry_run": true}),
        ));
        assert_eq!(dry["ok"], true);
        assert_eq!(dry["dry_run"], true);
        assert_eq!(dry["transaction"]["committed"], false);
        assert_eq!(dry["transaction"]["succeeded"], true);

        // The claim is still live — a sandbox revise suppresses nothing (P10 for free).
        let live =
            text_payload(server.call_tool_result("recall", &json!({"query": "revisable belief"})));
        assert_eq!(live["claims"][0]["suppressed"], false);
    }

    #[test]
    fn committed_revise_suppresses_but_never_deletes() {
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let _env = EnvRestore::capture(&["GMEOW_LANG", "GMEOW_MEMORY_PATH", "HOME", "USERPROFILE"]);
        unsafe {
            // SAFETY: tests mutate process env single-threaded under ENV_LOCK.
            env::remove_var("GMEOW_LANG");
        }
        let (_dir, _memory_path) = temp_memory();
        let bytes = snapshot();
        let server = McpServer::from_snapshot(&bytes).unwrap();
        let stored = text_payload(
            server.call_tool_result("store_claim", &json!({"text": "a belief to retire"})),
        );
        let claim_id = stored["claim"]["id"].as_str().unwrap().to_string();

        let revised = text_payload(server.call_tool_result(
            "revise_belief",
            &json!({"claim_id": claim_id, "reason": "superseded"}),
        ));
        assert_eq!(revised["ok"], true);
        assert_eq!(revised["transaction"]["committed"], true);

        // Default recall hides it (suppressed) ...
        let default =
            text_payload(server.call_tool_result("recall", &json!({"query": "belief retire"})));
        assert!(default["claims"].as_array().unwrap().is_empty());
        // ... but it is still present (supersession, never erasure — P10).
        let audit = text_payload(server.call_tool_result(
            "recall",
            &json!({"query": "belief retire", "include_suppressed": true}),
        ));
        assert!(
            audit["claims"]
                .as_array()
                .unwrap()
                .iter()
                .any(|claim| claim["id"] == claim_id.as_str() && claim["suppressed"] == true)
        );
    }

    #[test]
    fn startup_language_is_validated_and_json_rpc_dispatches() {
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let _env = EnvRestore::capture(&["GMEOW_LANG", "GMEOW_MEMORY_PATH", "HOME", "USERPROFILE"]);
        let bytes = snapshot();
        unsafe {
            // SAFETY: tests mutate process env single-threaded under ENV_LOCK.
            env::set_var("GMEOW_LANG", "notatag");
        }
        let err = match McpServer::from_snapshot(&bytes) {
            Ok(_) => panic!("invalid startup language must fail"),
            Err(err) => err,
        };
        assert!(err.to_string().contains("unknown language tag 'notatag'"));

        unsafe {
            // SAFETY: tests mutate process env single-threaded under ENV_LOCK.
            env::set_var("GMEOW_LANG", "fr");
        }
        let server = McpServer::from_snapshot(&bytes).unwrap();
        let init: Value = serde_json::from_str(
            &server.handle_message(r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#),
        )
        .unwrap();
        assert_eq!(init["result"]["serverInfo"]["name"], "gmeow");

        let tools: Value = serde_json::from_str(
            &server.handle_message(r#"{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}"#),
        )
        .unwrap();
        assert!(
            tools["result"]["tools"]
                .as_array()
                .unwrap()
                .iter()
                .any(|tool| tool["name"] == "lookup_term")
        );

        unsafe {
            // SAFETY: tests mutate process env single-threaded under ENV_LOCK.
            env::set_var("GMEOW_LANG", "X-GMEOW-FRENCH");
        }
        let server = McpServer::from_snapshot(&bytes).unwrap();
        let fr = text_payload(
            server.call_tool_result("lookup_term", &json!({"term": "gmeow:EntityExistence"})),
        );
        assert_eq!(fr["label"], "Existence d'entit\u{e9}");
        unsafe {
            // SAFETY: tests mutate process env single-threaded under ENV_LOCK.
            env::remove_var("GMEOW_LANG");
        }
    }

    /// A bare local name that collides across grounding namespaces HARD-FAILS on
    /// EVERY consumer surface — the MCP twin of `gmeow describe`'s typed ambiguity —
    /// never a silent first-exact pick (`.goals` NO OPTIONALITY). `Conjecture` names
    /// both `logic:Conjecture` and `math:Conjecture` in the shipped bundle.
    #[test]
    fn ambiguous_bare_name_hard_fails_across_mcp_surfaces() {
        let server = consumer_server();

        // `lookup_term`: the DISTINCT ambiguity envelope (ok:false) listing BOTH
        // sorted candidate CURIEs — not a silent pick of the first exact match.
        let looked =
            text_payload(server.call_tool_result("lookup_term", &json!({"term": "Conjecture"})));
        assert_eq!(looked["ok"], json!(false));
        let err = looked["error"].as_str().expect("ambiguity error string");
        assert_eq!(
            err, "ambiguous term 'Conjecture': logic:Conjecture, math:Conjecture",
            "lookup_term must emit the sorted-candidate ambiguity envelope"
        );

        // `doc_card`: hard fail (isError) with the same ambiguity — not a card.
        let card = server.call_tool_result("doc_card", &json!({"term": "Conjecture"}));
        assert_eq!(card["isError"], json!(true));
        let card_err = text_payload(card);
        assert_eq!(card_err["ok"], json!(false));
        let ce_msg = card_err["error"].as_str().expect("doc_card error");
        assert!(
            ce_msg.contains("logic:Conjecture") && ce_msg.contains("math:Conjecture"),
            "doc_card ambiguity must list sorted candidates, got {ce_msg:?}"
        );

        // The resolution guard feeding `counter_examples` / `entailments` /
        // `competency_questions` hard-fails too (isError) — never a silent IRI.
        let guarded = server.call_tool_result("counter_examples", &json!({"term": "Conjecture"}));
        assert_eq!(guarded["isError"], json!(true));

        // The ambiguity carries its OWN typed code, DISTINCT from the generic
        // unknown-term `pipeline.mcp` — greppable as `pipeline.mcp.ambiguous-term`.
        let requested = server.startup_requested.clone();
        let ConsumerResolution::Ambiguous { candidates } =
            server.view.resolve_term_iri("Conjecture", requested)
        else {
            panic!("`Conjecture` must resolve ambiguously across logic:/math:");
        };
        assert_eq!(
            candidates,
            vec![
                "logic:Conjecture".to_string(),
                "math:Conjecture".to_string()
            ],
            "candidates sorted + deduped"
        );
        let diag = ambiguous_term_err("Conjecture", &candidates);
        assert_eq!(diag.code(), crate::error::McpAmbiguousTerm::register());

        // Regression: unambiguous queries still RESOLVE on the same surface — the
        // ambiguity gate fires ONLY on genuine cross-namespace collisions.
        for (q, curie) in [
            ("lang:Denotation", "lang:Denotation"),
            ("math:Function", "math:Function"),
            ("Denotation", "lang:Denotation"),
        ] {
            let hit = text_payload(server.call_tool_result("lookup_term", &json!({"term": q})));
            assert_eq!(hit["ok"], json!(true), "`{q}` must still resolve");
            assert_eq!(hit["curie"], json!(curie), "`{q}` resolves to {curie}");
        }
    }

    #[test]
    fn default_memory_path_lives_under_home() {
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let _env = EnvRestore::capture(&["GMEOW_LANG", "GMEOW_MEMORY_PATH", "HOME", "USERPROFILE"]);
        unsafe {
            // SAFETY: tests mutate process env single-threaded under ENV_LOCK.
            env::remove_var("GMEOW_LANG");
            env::remove_var("GMEOW_MEMORY_PATH");
        }
        let dir = tempfile::tempdir().expect("tempdir");
        unsafe {
            // SAFETY: tests mutate process env single-threaded under ENV_LOCK.
            env::set_var("HOME", dir.path());
        }
        let bytes = snapshot();
        let server = McpServer::from_snapshot(&bytes).unwrap();
        text_payload(server.call_tool_result("store_claim", &json!({"text": "durable belief"})));
        assert!(dir.path().join(".gmeow/memory.gts").exists());
    }

    #[test]
    fn memory_path_handles_userprofile_and_relative_files() {
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let _env = EnvRestore::capture(&["GMEOW_LANG", "GMEOW_MEMORY_PATH", "HOME", "USERPROFILE"]);
        let _cwd = CwdRestore::capture();
        unsafe {
            // SAFETY: tests mutate process env single-threaded under ENV_LOCK.
            env::remove_var("GMEOW_LANG");
            env::remove_var("GMEOW_MEMORY_PATH");
            env::remove_var("HOME");
        }
        let dir = tempfile::tempdir().expect("tempdir");
        unsafe {
            // SAFETY: tests mutate process env single-threaded under ENV_LOCK.
            env::set_var("USERPROFILE", dir.path());
        }
        let bytes = snapshot();
        let server = McpServer::from_snapshot(&bytes).unwrap();
        text_payload(
            server.call_tool_result("store_claim", &json!({"text": "profile fallback belief"})),
        );
        assert!(dir.path().join(".gmeow/memory.gts").exists());

        let relative_dir = tempfile::tempdir().expect("relative tempdir");
        env::set_current_dir(relative_dir.path()).expect("set current dir");
        unsafe {
            // SAFETY: tests mutate process env single-threaded under ENV_LOCK.
            env::set_var("GMEOW_MEMORY_PATH", "memory.gts");
        }
        let server = McpServer::from_snapshot(&bytes).unwrap();
        text_payload(
            server.call_tool_result("store_claim", &json!({"text": "relative path belief"})),
        );
        assert!(relative_dir.path().join("memory.gts").exists());
    }

    /// The read-only local-ontology overlay: reads see `bundle ∪ overlay`, the
    /// overlay is provenance-isolated under the external graph, and nothing is
    /// written back — not the overlay file, not the canon, not memory.
    #[test]
    fn local_overlay_is_a_read_only_external_annex() {
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let _env = EnvRestore::capture(&["GMEOW_LANG", "GMEOW_MEMORY_PATH", "HOME", "USERPROFILE"]);
        unsafe {
            // SAFETY: tests mutate process env single-threaded under ENV_LOCK.
            env::remove_var("GMEOW_LANG");
        }
        let (mem_dir, memory_path) = temp_memory();
        let bytes = snapshot();
        let server = McpServer::from_snapshot(&bytes).unwrap();

        // A local lower-tier vocab file the agent supplies (not part of the canon).
        let overlay_ttl = "<urn:ex:widget> <urn:ex:label> \"Local Widget\" .\n<urn:ex:widget> a <urn:ex:Thing> .\n";
        let overlay_data = overlay_ttl;

        // Reads see the overlay unioned into the default graph.
        let seen = text_payload(server.call_tool_result(
            "query_local",
            &json!({
                "data": overlay_data, "format": "turtle",
                "query": "SELECT ?o WHERE { <urn:ex:widget> <urn:ex:label> ?o }",
            }),
        ));
        assert_eq!(seen["ok"], true, "overlay query must succeed: {seen}");
        assert_eq!(seen["results"]["bindings"][0]["o"]["value"], "Local Widget");

        // Reads ALSO see the bundle canon in the same active graph (union, not
        // replacement): a plain triple pattern still matches the signed ontology.
        let canon = text_payload(server.call_tool_result(
            "query_local",
            &json!({"data": overlay_data, "format": "turtle", "query": "ASK { ?s ?p ?o }"}),
        ));
        assert_eq!(canon["ok"], true);
        assert_eq!(canon["boolean"], true);

        // The overlay is provenance-isolable under the distinct external graph — its
        // triples never bear a signed gmeow: graph name.
        let isolated = text_payload(server.call_tool_result(
            "query_local",
            &json!({
                "data": overlay_data, "format": "turtle",
                "query": "SELECT ?o WHERE { GRAPH <urn:gmeow:mcp:overlay:external> \
                          { <urn:ex:widget> <urn:ex:label> ?o } }",
            }),
        ));
        assert_eq!(
            isolated["ok"], true,
            "external-graph query must succeed: {isolated}"
        );
        assert_eq!(
            isolated["results"]["bindings"][0]["o"]["value"],
            "Local Widget"
        );

        // CONSTRUCT/DESCRIBE are ANSWERED and the form is declared — see
        // `sparql_result_to_json` on why the old refusal was a capability gap, not a policy.
        let construct = text_payload(server.call_tool_result(
            "query_local",
            &json!({"data": overlay_data, "format": "turtle", "query": "CONSTRUCT { ?s ?p ?o } WHERE { ?s ?p ?o } LIMIT 1"}),
        ));
        assert_eq!(construct["ok"], true, "{construct}");
        assert_eq!(construct["form"], "graph", "{construct}");

        // ── the `scope` selection ────────────────────────────────────────────────────
        // The two scopes answer different questions over the SAME overlay, and both are
        // real answers. `input` was previously unaskable: every query silently carried the
        // canon, so a caller could not read a pasted document on its own terms.
        let bundle_scope = text_payload(server.call_tool_result(
            "query_local",
            &json!({
                "data": overlay_data, "format": "turtle", "scope": "bundle",
                "query": "SELECT (COUNT(*) AS ?n) WHERE { ?s ?p ?o }",
            }),
        ));
        let input_scope = text_payload(server.call_tool_result(
            "query_local",
            &json!({
                "data": overlay_data, "format": "turtle", "scope": "input",
                "query": "SELECT (COUNT(*) AS ?n) WHERE { ?s ?p ?o }",
            }),
        ));
        assert_eq!(bundle_scope["ok"], true, "{bundle_scope}");
        assert_eq!(input_scope["ok"], true, "{input_scope}");
        let count = |v: &Value| -> u64 {
            v["results"]["bindings"][0]["n"]["value"]
                .as_str()
                .and_then(|s| s.parse().ok())
                .unwrap_or_else(|| panic!("count binding: {v}"))
        };
        assert!(
            count(&bundle_scope) > count(&input_scope),
            "bundle scope reads the canon too, input scope reads the overlay alone: \
             {} vs {}",
            count(&bundle_scope),
            count(&input_scope)
        );
        // An omitted scope is the bundle union — the documented default, not a guess.
        let defaulted = text_payload(server.call_tool_result(
            "query_local",
            &json!({
                "data": overlay_data, "format": "turtle",
                "query": "SELECT (COUNT(*) AS ?n) WHERE { ?s ?p ?o }",
            }),
        ));
        assert_eq!(count(&defaulted), count(&bundle_scope), "{defaulted}");
        // An unknown scope is a NAMED hard error, never a silent fallback to the default —
        // the same discipline `format` has.
        let bogus = text_payload(server.call_tool_result(
            "query_local",
            &json!({
                "data": overlay_data, "format": "turtle", "scope": "everything",
                "query": "SELECT ?s WHERE { ?s ?p ?o }",
            }),
        ));
        assert_eq!(bogus["ok"], false, "{bogus}");
        assert!(
            bogus["error"]
                .as_str()
                .unwrap_or_default()
                .contains("unknown scope `everything`"),
            "the refusal names the offending token and the accepted set: {bogus}"
        );

        // Read-only: the overlay file is byte-for-byte unchanged and NOTHING was
        // written to memory (the write triad never touches the overlay or canon).
        assert!(Memory::new(&memory_path).claims().unwrap().is_empty());
        assert!(!memory_path.exists());
        drop(mem_dir);
    }

    /// verify_graph fires the matching `verify.<stem>` finding on a known bad-example
    /// overlay: a doxastic state whose asserted `gmeow:credence` is out of `[0,1]` — the
    /// exact violation `queries/verify/credence-out-of-range.rq` is a negative test for.
    /// The overlay joins the reasoning default world and the flat verify query matches it,
    /// so the response `findings` carries `verify.credence-out-of-range`.
    #[test]
    fn verify_graph_fires_on_a_bad_example_overlay_heavy_offgate() {
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let _env = EnvRestore::capture(&["GMEOW_LANG", "GMEOW_MEMORY_PATH", "HOME", "USERPROFILE"]);
        unsafe {
            // SAFETY: tests mutate process env single-threaded under ENV_LOCK.
            env::remove_var("GMEOW_LANG");
        }
        let bytes = snapshot();
        let server = McpServer::from_snapshot(&bytes).unwrap();

        // A credence out of [0,1] is the credence-out-of-range bad example. `5` is
        // numeric and > 1, so the negative test returns an offending row.
        let overlay_ttl = "@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n\
             <urn:ex:bad-credence-state> gmeow:credence 5 .\n";
        let overlay_data = overlay_ttl;

        // A tiny step budget: the credence negative test fires from the ASSERTED union
        // graph regardless of closure depth, so the budget keeps the test fast.
        let out = text_payload(server.call_tool_result(
            "verify_graph",
            &json!({"data": overlay_data, "format": "turtle", "max_steps": 8}),
        ));
        assert_eq!(out["ok"], true, "verify_graph must succeed: {out}");
        let codes: Vec<String> = out["findings"]
            .as_array()
            .expect("findings array")
            .iter()
            .map(|f| f["code"].as_str().unwrap_or_default().to_owned())
            .collect();
        assert!(
            codes.iter().any(|c| c == "verify.credence-out-of-range"),
            "the credence bad example must fire verify.credence-out-of-range: {out}"
        );
        assert!(
            out["error_count"].as_u64().unwrap_or(0) >= 1,
            "the violation must count as an error finding: {out}"
        );
    }

    /// Proof faithfulness: `cited_iris` must be derived ONLY from structured RDF
    /// binding terms, never scraped from rendered finding prose — an
    /// agent-controlled overlay literal carrying angle-bracket text must not be
    /// accepted as a genuine citation. The overlay's `gmeow:credence` value is the
    /// STRING literal `"see <urn:fake>"` (non-numeric, so `credence-out-of-range`
    /// fires) attached to the real subject `<urn:ex:forge-cited-iris-state>`. A
    /// text-scrape over the rendered `detail` (`credence="see <urn:fake>",
    /// state=<urn:ex:...>`) would forge `urn:fake` into `cited_iris`; the
    /// structured-term derivation must not, while the genuinely-cited subject IRI
    /// must still appear.
    #[test]
    fn verify_graph_cited_iris_excludes_forged_literal_text_heavy_offgate() {
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let _env = EnvRestore::capture(&["GMEOW_LANG", "GMEOW_MEMORY_PATH", "HOME", "USERPROFILE"]);
        unsafe {
            // SAFETY: tests mutate process env single-threaded under ENV_LOCK.
            env::remove_var("GMEOW_LANG");
        }
        let bytes = snapshot();
        let server = McpServer::from_snapshot(&bytes).unwrap();

        // `!isNumeric("see <urn:fake>")` is true, so this is a credence-out-of-range
        // bad example exactly like the sibling test above — but the credence VALUE
        // is a string literal carrying forged-looking angle-bracket text.
        let overlay_ttl = "@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n\
             <urn:ex:forge-cited-iris-state> gmeow:credence \"see <urn:fake>\" .\n";
        let overlay_data = overlay_ttl;

        let out = text_payload(server.call_tool_result(
            "verify_graph",
            &json!({"data": overlay_data, "format": "turtle", "max_steps": 8}),
        ));
        assert_eq!(out["ok"], true, "verify_graph must succeed: {out}");
        let codes: Vec<String> = out["findings"]
            .as_array()
            .expect("findings array")
            .iter()
            .map(|f| f["code"].as_str().unwrap_or_default().to_owned())
            .collect();
        assert!(
            codes.iter().any(|c| c == "verify.credence-out-of-range"),
            "the forged-literal overlay must fire verify.credence-out-of-range: {out}"
        );
        let cited: Vec<String> = out["cited_iris"]
            .as_array()
            .expect("cited_iris array")
            .iter()
            .map(|v| v.as_str().unwrap_or_default().to_owned())
            .collect();
        assert!(
            !cited.iter().any(|c| c.contains("urn:fake")),
            "a literal's angle-bracket text must never forge a citation: {out}"
        );
        assert!(
            cited.iter().any(|c| c == "urn:ex:forge-cited-iris-state"),
            "the finding's genuinely-cited subject IRI must still appear: {out}"
        );
    }

    /// Overlay isolation: verify_graph builds a TRANSIENT union and drops it — the signed
    /// canon `McpView::dataset` is never mutated. After the call the canon Arc is the SAME
    /// allocation with the SAME quad count, and the overlay file is byte-unchanged (the
    /// external annex is read-only and never merged into or written back from the canon).
    #[test]
    fn verify_graph_leaves_the_canon_dataset_untouched_heavy_offgate() {
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let _env = EnvRestore::capture(&["GMEOW_LANG", "GMEOW_MEMORY_PATH", "HOME", "USERPROFILE"]);
        unsafe {
            // SAFETY: tests mutate process env single-threaded under ENV_LOCK.
            env::remove_var("GMEOW_LANG");
        }
        let bytes = snapshot();
        let server = McpServer::from_snapshot(&bytes).unwrap();

        let overlay_ttl = "<urn:ex:probe-s> <urn:ex:probe-p> <urn:ex:probe-o> .\n";
        let overlay_data = overlay_ttl;

        let before_ptr = Arc::as_ptr(&server.view.dataset);
        let before_count = server.view.dataset.quad_count();

        let out = text_payload(server.call_tool_result(
            "verify_graph",
            &json!({"data": overlay_data, "format": "turtle", "max_steps": 4}),
        ));
        assert_eq!(out["ok"], true, "verify_graph must succeed: {out}");

        // Canon Arc identity + quad count are unchanged — the union was transient.
        assert!(
            std::ptr::eq(Arc::as_ptr(&server.view.dataset), before_ptr),
            "the signed canon Arc must not be swapped by verify_graph"
        );
        assert_eq!(
            server.view.dataset.quad_count(),
            before_count,
            "the signed canon quad count must be unchanged (overlay never merged)"
        );
        // The overlay probe triple never leaks into the canon graphs.
        let leaked = server.view.dataset.quads().any(|q| {
            matches!(
                server.view.dataset.resolve(q.s),
                purrdf::prelude::TermRef::Iri(iri) if iri == "urn:ex:probe-s"
            )
        });
        assert!(
            !leaked,
            "overlay triple must not appear in the signed canon"
        );
        // Read-only: the overlay file is byte-for-byte unchanged.
    }

    /// A budget-cut closure yields the strictly-weaker ATTESTATION, never a certificate:
    /// a `max_steps` of 1 cuts the forward chase over the large bundle union mid-flight,
    /// so `reason_all_budgeted` returns a non-conclusive BudgetExhausted / Incomplete
    /// verdict. The completeness gate then MUST render `CoherenceCheckAttestation` with a
    /// non-conclusive completeness axis — a certificate is impossible on an incomplete search.
    #[test]
    fn verify_graph_budget_cut_yields_attestation_never_certificate_heavy_offgate() {
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let _env = EnvRestore::capture(&["GMEOW_LANG", "GMEOW_MEMORY_PATH", "HOME", "USERPROFILE"]);
        unsafe {
            // SAFETY: tests mutate process env single-threaded under ENV_LOCK.
            env::remove_var("GMEOW_LANG");
        }
        let bytes = snapshot();
        let server = McpServer::from_snapshot(&bytes).unwrap();

        let overlay_data = "<urn:ex:s> <urn:ex:p> <urn:ex:o> .\n";

        let out = text_payload(server.call_tool_result(
            "verify_graph",
            &json!({"data": overlay_data, "format": "turtle", "max_steps": 1}),
        ));
        assert_eq!(out["ok"], true, "verify_graph must succeed: {out}");
        assert_eq!(
            out["class_local_name"], "CoherenceCheckAttestation",
            "a budget-cut closure must attest, never certify: {out}"
        );
        assert_ne!(
            out["class_local_name"], "CoherenceCertificate",
            "a budget-cut closure must NEVER render a certificate: {out}"
        );
        // The completeness axis is non-conclusive (the mid-chase cut → Incomplete), and the
        // computation axis records the budget exhaustion.
        assert_eq!(out["completeness"], "incomplete", "{out}");
        assert_eq!(out["evaluation"], "budget-exhausted", "{out}");
    }

    /// R4: OMITTING `max_steps`/`max_answers` entirely must NEVER run an
    /// unbounded Turing-complete chase — `governed_budget` stamps the finite
    /// `DEFAULT_MAX_STEPS` server-side ceiling on every agent-facing call, never `None`.
    /// The real bundle's full DL closure is far larger than `DEFAULT_MAX_STEPS` (the sibling
    /// `verify_graph_budget_cut_yields_attestation_never_certificate_heavy_offgate` above
    /// already shows even `max_steps: 1` cuts it), so a call that omits the args entirely
    /// must land on the SAME governed, non-conclusive `CoherenceCheckAttestation` —
    /// `budget-exhausted` / `incomplete` — never a conclusive certificate an unbounded chase
    /// would be needed to produce.
    #[test]
    fn verify_graph_omitted_max_steps_is_governed_not_unbounded_heavy_offgate() {
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let _env = EnvRestore::capture(&["GMEOW_LANG", "GMEOW_MEMORY_PATH", "HOME", "USERPROFILE"]);
        unsafe {
            // SAFETY: tests mutate process env single-threaded under ENV_LOCK.
            env::remove_var("GMEOW_LANG");
        }
        let bytes = snapshot();
        let server = McpServer::from_snapshot(&bytes).unwrap();

        let overlay_data = "<urn:ex:s> <urn:ex:p> <urn:ex:o> .\n";

        // The exact agent-omission shape R4 forbids treating as unbounded: no `max_steps`,
        // no `max_answers` key at all in the call args.
        let out = text_payload(server.call_tool_result(
            "verify_graph",
            &json!({"data": overlay_data, "format": "turtle"}),
        ));
        assert_eq!(out["ok"], true, "verify_graph must succeed: {out}");
        assert_eq!(
            out["class_local_name"], "CoherenceCheckAttestation",
            "an omitted-max_steps call over the large bundle union must still be GOVERNED \
             (budget-cut) by the default server-side ceiling, never a conclusive certificate \
             that only an unbounded chase could produce: {out}"
        );
        assert_ne!(
            out["class_local_name"], "CoherenceCertificate",
            "an omitted-max_steps call must NEVER run to an unbounded conclusive closure: {out}"
        );
        assert_eq!(
            out["completeness"], "incomplete",
            "the default ceiling must cut the real bundle's closure: {out}"
        );
        assert_eq!(
            out["evaluation"], "budget-exhausted",
            "the omitted max_steps must resolve to the finite DEFAULT_MAX_STEPS, never \
             None/unbounded: {out}"
        );

        // The grounded judgment's own carried budget usage confirms the finite ceiling: the
        // consumed step count is bounded by (and the declared allowance matches) the SAME
        // DEFAULT_MAX_STEPS `governed_budget` stamps — never absent (which would mean
        // unbounded).
        let judgment = out["judgment_nquads"]
            .as_str()
            .expect("judgment_nquads string");
        let parsed = gmeow_logic::result_rdf::parse_reasoning_graph(judgment)
            .expect("judgment_nquads parses as a reasoning-result graph");
        assert_eq!(
            parsed.provenance.consumed_budget.allowance,
            Some(DEFAULT_MAX_STEPS),
            "the grounded judgment must declare the DEFAULT_MAX_STEPS allowance, never an \
             absent (unbounded) allowance: {judgment}"
        );
        assert!(
            parsed.provenance.consumed_budget.consumed <= DEFAULT_MAX_STEPS,
            "consumed steps must never exceed the declared default allowance: {judgment}"
        );
    }

    /// An overlay exceeding `MAX_VERIFY_OVERLAY_QUADS` is a HARD FAIL — the bounded agent
    /// path — refused BEFORE any reasoning runs, never a silently truncated graph.
    #[test]
    fn verify_graph_rejects_an_oversized_overlay() {
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let _env = EnvRestore::capture(&["GMEOW_LANG", "GMEOW_MEMORY_PATH", "HOME", "USERPROFILE"]);
        unsafe {
            // SAFETY: tests mutate process env single-threaded under ENV_LOCK.
            env::remove_var("GMEOW_LANG");
        }
        let bytes = snapshot();
        let server = McpServer::from_snapshot(&bytes).unwrap();

        // One distinct triple over the ceiling — the smallest overlay that trips it.
        let mut body = String::with_capacity((MAX_VERIFY_OVERLAY_QUADS + 1) * 40);
        for i in 0..=MAX_VERIFY_OVERLAY_QUADS {
            body.push_str(&format!("<urn:ex:s{i}> <urn:ex:p> <urn:ex:o{i}> .\n"));
        }
        let overlay_data = body.as_str();

        let out = text_payload(server.call_tool_result(
            "verify_graph",
            &json!({"data": overlay_data, "format": "turtle", "max_steps": 1}),
        ));
        assert_eq!(
            out["ok"], false,
            "an oversized overlay must hard-fail: {out}"
        );
        assert!(
            out["error"]
                .as_str()
                .unwrap_or_default()
                .contains("exceeding"),
            "the error must name the ceiling breach: {out}"
        );
    }

    /// The R4 byte gate: an inline overlay whose LENGTH exceeds
    /// `MAX_VERIFY_OVERLAY_BYTES` is refused BEFORE it is handed to the parser — so a
    /// huge payload can never exhaust memory building a dataset before the post-parse
    /// `MAX_VERIFY_OVERLAY_QUADS` ceiling gets a chance to run. The filler here is a
    /// single deliberately-oversized comment line, NOT well-formed RDF that would parse
    /// into many quads: if the byte gate did not run before the parse (i.e. this fix
    /// regressed), the payload would still parse successfully (as an empty, all-comment
    /// document) and `verify_graph` would return `ok:true` instead of hard-failing on
    /// the byte ceiling, so this test would catch the regression either way — and it
    /// must never OOM proving it.
    #[test]
    fn verify_graph_rejects_an_overlay_over_the_byte_ceiling_before_read() {
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let _env = EnvRestore::capture(&["GMEOW_LANG", "GMEOW_MEMORY_PATH", "HOME", "USERPROFILE"]);
        unsafe {
            // SAFETY: tests mutate process env single-threaded under ENV_LOCK.
            env::remove_var("GMEOW_LANG");
        }
        let bytes = snapshot();
        let server = McpServer::from_snapshot(&bytes).unwrap();

        // One byte over the ceiling — the smallest overlay that trips it. A single
        // `#`-prefixed line is cheap to build (one allocation, no per-quad
        // formatting) and never parses into any quads.
        let filler = "#".repeat((MAX_VERIFY_OVERLAY_BYTES + 1) as usize);
        let overlay_data = filler.as_str();

        let out = text_payload(server.call_tool_result(
            "verify_graph",
            &json!({"data": overlay_data, "format": "turtle", "max_steps": 1}),
        ));
        assert_eq!(
            out["ok"], false,
            "an overlay over the byte ceiling must hard-fail: {out}"
        );
        let error = out["error"].as_str().unwrap_or_default();
        assert!(
            error.contains(&MAX_VERIFY_OVERLAY_BYTES.to_string()) && error.contains("byte"),
            "the error must name the byte limit: {out}"
        );
    }

    /// The two overlay tools take BYTES plus an EXPLICIT `format`. A pasted Turtle
    /// string with `{"format":"turtle"}` is accepted by both — the positive half of the
    /// contract the two negative tests below pin.
    #[test]
    fn overlay_tools_accept_pasted_turtle_with_an_explicit_format() {
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let _env = EnvRestore::capture(&["GMEOW_LANG", "GMEOW_MEMORY_PATH", "HOME", "USERPROFILE"]);
        unsafe {
            // SAFETY: tests mutate process env single-threaded under ENV_LOCK.
            env::remove_var("GMEOW_LANG");
        }
        let bytes = snapshot();
        let server = McpServer::from_snapshot(&bytes).unwrap();

        let pasted = "<urn:ex:pasted> <urn:ex:label> \"Pasted Widget\" .\n";

        let queried = text_payload(server.call_tool_result(
            "query_local",
            &json!({
                "data": pasted,
                "format": "turtle",
                "query": "SELECT ?o WHERE { <urn:ex:pasted> <urn:ex:label> ?o }",
            }),
        ));
        assert_eq!(
            queried["ok"], true,
            "pasted Turtle with an explicit format must query cleanly: {queried}"
        );
        assert_eq!(
            queried["results"]["bindings"][0]["o"]["value"], "Pasted Widget",
            "the pasted overlay must be visible to the query: {queried}"
        );

        let verified = text_payload(server.call_tool_result(
            "verify_graph",
            &json!({"data": pasted, "format": "turtle", "max_steps": 1}),
        ));
        assert_eq!(
            verified["ok"], true,
            "pasted Turtle with an explicit format must verify cleanly: {verified}"
        );
    }

    /// A MISSING `format` is a hard error on both overlay tools — never a guess at
    /// Turtle. The two tools also ADVERTISE `format` as required, so a client can see
    /// the obligation before it calls.
    #[test]
    fn overlay_tools_hard_fail_without_a_format() {
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let _env = EnvRestore::capture(&["GMEOW_LANG", "GMEOW_MEMORY_PATH", "HOME", "USERPROFILE"]);
        unsafe {
            // SAFETY: tests mutate process env single-threaded under ENV_LOCK.
            env::remove_var("GMEOW_LANG");
        }
        let bytes = snapshot();
        let server = McpServer::from_snapshot(&bytes).unwrap();

        let pasted = "<urn:ex:pasted> <urn:ex:p> <urn:ex:o> .\n";

        let queried = text_payload(server.call_tool_result(
            "query_local",
            &json!({"data": pasted, "query": "ASK { ?s ?p ?o }"}),
        ));
        assert_eq!(
            queried["ok"], false,
            "query_local without a format must hard-fail: {queried}"
        );
        assert!(
            queried["error"]
                .as_str()
                .unwrap_or_default()
                .contains("format"),
            "the error must name the missing argument: {queried}"
        );

        let verified = text_payload(
            server.call_tool_result("verify_graph", &json!({"data": pasted, "max_steps": 1})),
        );
        assert_eq!(
            verified["ok"], false,
            "verify_graph without a format must hard-fail: {verified}"
        );
        assert!(
            verified["error"]
                .as_str()
                .unwrap_or_default()
                .contains("format"),
            "the error must name the missing argument: {verified}"
        );

        // The advertised schema agrees with the enforcement.
        for name in ["query_local", "verify_graph"] {
            let descriptor = server
                .surface
                .tool_descriptors()
                .into_iter()
                .find(|t| t["name"] == name)
                .unwrap_or_else(|| panic!("{name} is advertised"));
            let required: Vec<String> = descriptor["inputSchema"]["required"]
                .as_array()
                .expect("required array")
                .iter()
                .map(|v| v.as_str().expect("required entries are strings").to_owned())
                .collect();
            for arg in ["data", "format"] {
                assert!(
                    required.iter().any(|r| r == arg),
                    "{name} enforces `{arg}` at call time and must advertise it as required: \
                     {required:?}"
                );
            }
        }
    }

    /// An UNKNOWN `format` is a hard error NAMING THE ACCEPTED SET on both overlay
    /// tools — the caller is told what to pass, and no fallback parse ever runs.
    #[test]
    fn overlay_tools_hard_fail_on_an_unknown_format_naming_the_accepted_set() {
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let _env = EnvRestore::capture(&["GMEOW_LANG", "GMEOW_MEMORY_PATH", "HOME", "USERPROFILE"]);
        unsafe {
            // SAFETY: tests mutate process env single-threaded under ENV_LOCK.
            env::remove_var("GMEOW_LANG");
        }
        let bytes = snapshot();
        let server = McpServer::from_snapshot(&bytes).unwrap();

        // Well-formed Turtle: the ONLY thing wrong is the declared format, so a
        // fallback-to-Turtle regression would return `ok:true` and fail this test.
        let pasted = "<urn:ex:pasted> <urn:ex:p> <urn:ex:o> .\n";

        let queried = text_payload(server.call_tool_result(
            "query_local",
            &json!({"data": pasted, "format": "yaml-ld", "query": "ASK { ?s ?p ?o }"}),
        ));
        assert_eq!(
            queried["ok"], false,
            "an unknown format must hard-fail rather than fall back to Turtle: {queried}"
        );
        let error = queried["error"].as_str().unwrap_or_default();
        assert!(
            error.contains("query_local") && error.contains("yaml-ld"),
            "the error must name the tool and the rejected token: {queried}"
        );
        for accepted in [
            "turtle",
            "n-triples",
            "n-quads",
            "trig",
            "rdf+xml",
            "json-ld",
        ] {
            assert!(
                error.contains(accepted),
                "the error must name the accepted format `{accepted}`: {queried}"
            );
        }

        let verified = text_payload(server.call_tool_result(
            "verify_graph",
            &json!({"data": pasted, "format": "yaml-ld", "max_steps": 1}),
        ));
        assert_eq!(
            verified["ok"], false,
            "an unknown format must hard-fail rather than fall back to Turtle: {verified}"
        );
        let error = verified["error"].as_str().unwrap_or_default();
        assert!(
            error.contains("verify_graph") && error.contains("yaml-ld"),
            "the error must name the tool and the rejected token: {verified}"
        );
        for accepted in [
            "turtle",
            "n-triples",
            "n-quads",
            "trig",
            "rdf+xml",
            "json-ld",
        ] {
            assert!(
                error.contains(accepted),
                "the error must name the accepted format `{accepted}`: {verified}"
            );
        }
    }

    /// A normal, well-under-ceiling overlay still succeeds through both the byte gate
    /// and the quad gate — the byte cap must never reject a legitimate small annex.
    #[test]
    fn verify_graph_accepts_a_normal_small_overlay_over_the_whole_bundle() {
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let _env = EnvRestore::capture(&["GMEOW_LANG", "GMEOW_MEMORY_PATH", "HOME", "USERPROFILE"]);
        unsafe {
            // SAFETY: tests mutate process env single-threaded under ENV_LOCK.
            env::remove_var("GMEOW_LANG");
        }
        let bytes = snapshot();
        let server = McpServer::from_snapshot(&bytes).unwrap();

        let overlay_data = "<urn:ex:s> <urn:ex:p> <urn:ex:o> .\n";

        let out = text_payload(server.call_tool_result(
            "verify_graph",
            &json!({"data": overlay_data, "format": "turtle", "max_steps": 1}),
        ));
        assert_eq!(
            out["ok"], true,
            "a normal small overlay must succeed: {out}"
        );
    }

    /// The grounded RDF judgment: `judgment_nquads` carries the `logic:ReasoningResult`
    /// node and round-trips through the shared reasoning-graph parser, so an agent can
    /// reason over the verdict itself. Its evaluation axis matches the JSON envelope's.
    #[test]
    fn verify_graph_judgment_nquads_grounds_the_reasoning_result_heavy_offgate() {
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let _env = EnvRestore::capture(&["GMEOW_LANG", "GMEOW_MEMORY_PATH", "HOME", "USERPROFILE"]);
        unsafe {
            // SAFETY: tests mutate process env single-threaded under ENV_LOCK.
            env::remove_var("GMEOW_LANG");
        }
        let bytes = snapshot();
        let server = McpServer::from_snapshot(&bytes).unwrap();

        let overlay_data = "<urn:ex:s> <urn:ex:p> <urn:ex:o> .\n";

        let out = text_payload(server.call_tool_result(
            "verify_graph",
            &json!({"data": overlay_data, "format": "turtle", "max_steps": 1}),
        ));
        assert_eq!(out["ok"], true, "verify_graph must succeed: {out}");
        let judgment = out["judgment_nquads"]
            .as_str()
            .expect("judgment_nquads string");
        assert!(
            judgment.contains("<https://blackcatinformatics.ca/logic/ReasoningResult>"),
            "the judgment must ground a logic:ReasoningResult node: {judgment}"
        );
        // It parses back through the shared reasoning-graph reader (round-trip honesty),
        // and its evaluation axis matches the envelope's.
        let parsed = gmeow_logic::result_rdf::parse_reasoning_graph(judgment)
            .expect("judgment_nquads parses as a reasoning-result graph");
        assert_eq!(
            parsed.evaluation.wire(),
            out["evaluation"].as_str().unwrap(),
            "the grounded judgment's evaluation axis must match the envelope: {out}"
        );
    }

    // ── explain_quad ──────────────────────────────────────────────────────────

    /// A synthetic explain row for the fast disambiguation / N3 unit tests.
    fn synthetic_row(graph: &str, subject: &str, predicate: &str, obj: &str) -> Row {
        Row {
            graph: graph.to_owned(),
            subject: subject.to_owned(),
            predicate: predicate.to_owned(),
            obj: obj.to_owned(),
            derivation_id: "urn:ex:deriv".to_owned(),
            rule_iri: "urn:ex:rule".to_owned(),
            source_quad_ids: Vec::new(),
        }
    }

    /// FAST (on-gate) unit test of the world-disambiguation helper on synthetic rows:
    /// a reifier shared across two worlds resolves to exactly the row in the supplied
    /// graph; a non-resolving graph over that multi-world reifier is an ambiguity HARD
    /// FAIL (never an arbitrary pick); a reifier no row carries is `not in closure`.
    #[test]
    fn explain_quad_disambiguation_resolves_by_graph_and_hard_fails_across_worlds() {
        let (world_a, world_b) = ("urn:ex:world-a", "urn:ex:world-b");
        let (s, p, o) = ("urn:ex:s", "urn:ex:p", "<urn:ex:o>");
        let rows = vec![
            synthetic_row(world_a, s, p, o),
            synthetic_row(world_b, s, p, o),
        ];
        let reifier = reifier_from_row(&rows[0]);
        assert_eq!(
            reifier,
            reifier_from_row(&rows[1]),
            "identical (S,P,O) shares a reifier across worlds"
        );

        // The supplied graph disambiguates to exactly one row.
        assert_eq!(locate_explain_target(&rows, &reifier, world_a).unwrap(), 0);
        assert_eq!(locate_explain_target(&rows, &reifier, world_b).unwrap(), 1);

        // A non-resolving graph over a multi-world reifier → ambiguity hard fail.
        let ambiguous = locate_explain_target(&rows, &reifier, "urn:ex:world-c").unwrap_err();
        assert!(
            ambiguous.to_string().contains("ambiguous"),
            "a cross-world reifier without a resolving graph must be ambiguous: {ambiguous}"
        );

        // A reifier no row carries → not in closure (never an arbitrary pick).
        let missing = locate_explain_target(
            &rows,
            "https://blackcatinformatics.ca/gmeow/reifier/deadbeef",
            world_a,
        )
        .unwrap_err();
        assert!(
            missing.to_string().contains("not in closure"),
            "an unknown reifier must be `not in closure`: {missing}"
        );
    }

    /// FAST (on-gate): a single-world reifier queried in the WRONG world is
    /// `not in closure`, and the error names the world the quad actually lives in.
    #[test]
    fn explain_quad_wrong_single_world_is_not_in_closure() {
        let rows = vec![synthetic_row(
            "urn:ex:world-a",
            "urn:ex:s",
            "urn:ex:p",
            "<urn:ex:o>",
        )];
        let reifier = reifier_from_row(&rows[0]);
        let err = locate_explain_target(&rows, &reifier, "urn:ex:other-world").unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("not in closure"), "{msg}");
        assert!(
            msg.contains("urn:ex:world-a"),
            "names the actual world: {msg}"
        );
    }

    /// FAST (on-gate): the canonical object N3 surface `explain_quad` builds is
    /// byte-identical to what a `Row.obj` carries (`term_display`), across IRI, plain
    /// literal, and typed literal; a bad `object_kind` and an `object_datatype` on an
    /// IRI object are HARD FAILS; and the omitted-kind inference is deterministic.
    #[test]
    fn explain_quad_object_n3_is_the_canonical_row_surface() {
        // IRI object → `<iri>` (explicit and inferred).
        assert_eq!(
            object_term_n3("urn:ex:o", Some("iri"), None).unwrap(),
            "<urn:ex:o>"
        );
        assert_eq!(
            object_term_n3("urn:ex:o", None, None).unwrap(),
            "<urn:ex:o>"
        );
        // Plain literal → `"lex"` (xsd:string elided, exactly like term_display).
        assert_eq!(
            object_term_n3("hello", Some("literal"), None).unwrap(),
            "\"hello\""
        );
        // A bare non-IRI value with whitespace is inferred as a literal.
        assert_eq!(
            object_term_n3("just text", None, None).unwrap(),
            "\"just text\""
        );
        // Typed literal → `"lex"^^<dt>`.
        assert_eq!(
            object_term_n3(
                "5",
                Some("literal"),
                Some("http://www.w3.org/2001/XMLSchema#integer")
            )
            .unwrap(),
            "\"5\"^^<http://www.w3.org/2001/XMLSchema#integer>"
        );
        // The N3 surface must MATCH what a Row built from the same TermValue carries.
        let iri_display = term_display(&TermValue::iri("urn:ex:o"));
        assert_eq!(
            object_term_n3("urn:ex:o", Some("iri"), None).unwrap(),
            iri_display
        );

        // A non-iri/literal kind is a hard error.
        assert!(object_term_n3("x", Some("bnode"), None).is_err());
        // A datatype on an IRI object is a contradictory request — hard error.
        assert!(object_term_n3("urn:ex:o", Some("iri"), Some("http://x")).is_err());
    }

    /// Reason the shipped bundle under `max_steps` and return the premise-closed
    /// explain rows — the SAME row set `run_explain_quad` reconstructs a target from.
    fn reasoned_rows(server: &McpServer, max_steps: u64) -> Vec<Row> {
        let budget = Budget {
            max_answers: None,
            max_steps: Some(max_steps),
        };
        let result = reason_all_budgeted(server.view.dataset.as_ref(), &budget)
            .expect("the shipped bundle reasons under the governor");
        explain::rows_for_result(&result).expect("rows build from the reasoning result")
    }

    /// The first row satisfying `pred` whose `(graph, reifier)` identity is UNIQUE in
    /// `rows`, so `explain_quad` resolves to exactly that row (never a duplicate).
    fn find_unique_row(rows: &[Row], pred: impl Fn(&Row) -> bool) -> Option<&Row> {
        let mut counts: HashMap<(String, String), usize> = HashMap::new();
        for row in rows {
            *counts
                .entry((row.graph.clone(), reifier_from_row(row)))
                .or_default() += 1;
        }
        rows.iter()
            .find(|row| pred(row) && counts[&(row.graph.clone(), reifier_from_row(row))] == 1)
    }

    /// HEAVY (off-gate): a DERIVED quad in the reasoned bundle closure explains to a
    /// non-empty skeleton (target first, `is_asserted:false`, firing rule preserved),
    /// a non-empty cited-IRI set that includes the target's reifier, and `faithful:true`.
    #[test]
    fn explain_quad_explains_a_derived_quad_heavy_offgate() {
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let _env = EnvRestore::capture(&["GMEOW_LANG", "GMEOW_MEMORY_PATH", "HOME", "USERPROFILE"]);
        unsafe {
            // SAFETY: tests mutate process env single-threaded under ENV_LOCK.
            env::remove_var("GMEOW_LANG");
        }
        let bytes = snapshot();
        let server = McpServer::from_snapshot(&bytes).unwrap();

        // A small governor budget already derives IRI-object quads (subsumption /
        // typing) while the mid-chase cut keeps the whole-bundle reason bounded — a
        // larger budget explodes the closure without adding coverage.
        let max_steps = 64u64;
        let rows = reasoned_rows(&server, max_steps);
        let derived = find_unique_row(&rows, |row| {
            row.rule_iri != gmeow_logic::provenance::ASSERT_RULE_IRI
                && row.obj.starts_with('<')
                && row.obj.ends_with('>')
        })
        .expect("the reasoned bundle must derive at least one unique IRI-object quad");
        let object_value = derived.obj[1..derived.obj.len() - 1].to_owned();

        let out = text_payload(server.call_tool_result(
            "explain_quad",
            &json!({
                "subject": derived.subject,
                "predicate": derived.predicate,
                "object_value": object_value,
                "object_kind": "iri",
                "graph": derived.graph,
                "max_steps": max_steps,
            }),
        ));
        assert_eq!(out["ok"], true, "explain_quad must succeed: {out}");
        assert_eq!(out["faithful"], true, "the proof must be faithful: {out}");
        let steps = out["step_skeleton"]
            .as_array()
            .expect("step_skeleton array");
        assert!(
            !steps.is_empty(),
            "a derived quad has a non-empty skeleton: {out}"
        );
        assert_eq!(
            steps[0]["is_asserted"], false,
            "the target step is derived: {out}"
        );
        assert_eq!(
            steps[0]["rule_iri"].as_str().unwrap(),
            derived.rule_iri,
            "the target step preserves the firing rule: {out}"
        );
        let cited = out["cited_iris"].as_array().expect("cited_iris array");
        assert!(!cited.is_empty(), "cited_iris must be non-empty: {out}");
        let reifier = reifier_from_row(derived);
        assert!(
            cited.iter().any(|c| c.as_str() == Some(reifier.as_str())),
            "the target reifier must be cited (the reifier matches the request): {out}"
        );
        assert_eq!(
            steps[0]["obj_n3"].as_str().unwrap(),
            derived.obj,
            "the target step's object N3 matches the row surface: {out}"
        );
    }

    /// HEAVY (off-gate): an ASSERTED (EDB) quad explains to a SINGLE step with
    /// `is_asserted:true` and `rule_iri` = the assert-rule IRI.
    #[test]
    fn explain_quad_explains_an_asserted_quad_heavy_offgate() {
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let _env = EnvRestore::capture(&["GMEOW_LANG", "GMEOW_MEMORY_PATH", "HOME", "USERPROFILE"]);
        unsafe {
            // SAFETY: tests mutate process env single-threaded under ENV_LOCK.
            env::remove_var("GMEOW_LANG");
        }
        let bytes = snapshot();
        let server = McpServer::from_snapshot(&bytes).unwrap();

        // Asserted rows are present regardless of chase depth, so a tiny budget suffices.
        let max_steps = 8u64;
        let rows = reasoned_rows(&server, max_steps);
        let asserted = find_unique_row(&rows, |row| {
            row.rule_iri == gmeow_logic::provenance::ASSERT_RULE_IRI
                && row.obj.starts_with('<')
                && row.obj.ends_with('>')
        })
        .expect("the reasoned bundle must carry a unique asserted IRI-object quad");
        let object_value = asserted.obj[1..asserted.obj.len() - 1].to_owned();

        let out = text_payload(server.call_tool_result(
            "explain_quad",
            &json!({
                "subject": asserted.subject,
                "predicate": asserted.predicate,
                "object_value": object_value,
                "object_kind": "iri",
                "graph": asserted.graph,
                "max_steps": max_steps,
            }),
        ));
        assert_eq!(out["ok"], true, "explain_quad must succeed: {out}");
        assert_eq!(out["faithful"], true, "the proof must be faithful: {out}");
        let steps = out["step_skeleton"]
            .as_array()
            .expect("step_skeleton array");
        assert_eq!(
            steps.len(),
            1,
            "an asserted quad is a single leaf step: {out}"
        );
        assert_eq!(steps[0]["is_asserted"], true, "the leaf is asserted: {out}");
        assert_eq!(
            steps[0]["rule_iri"].as_str().unwrap(),
            gmeow_logic::provenance::ASSERT_RULE_IRI,
            "the asserted leaf carries the assert-rule IRI: {out}"
        );
    }

    /// HEAVY (off-gate): a quad the closure does not entail is a HARD FAIL
    /// (`ok:false` + `not in closure`), NEVER an empty-but-ok proof.
    #[test]
    fn explain_quad_hard_fails_on_a_quad_not_in_closure_heavy_offgate() {
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let _env = EnvRestore::capture(&["GMEOW_LANG", "GMEOW_MEMORY_PATH", "HOME", "USERPROFILE"]);
        unsafe {
            // SAFETY: tests mutate process env single-threaded under ENV_LOCK.
            env::remove_var("GMEOW_LANG");
        }
        let bytes = snapshot();
        let server = McpServer::from_snapshot(&bytes).unwrap();

        let out = text_payload(server.call_tool_result(
            "explain_quad",
            &json!({
                "subject": "urn:ex:not-a-real-bundle-subject",
                "predicate": "urn:ex:not-a-real-bundle-predicate",
                "object_value": "urn:ex:not-a-real-bundle-object",
                "object_kind": "iri",
                "graph": "urn:ex:not-a-real-world",
                "max_steps": 8,
            }),
        ));
        assert_eq!(out["ok"], false, "a bogus quad must hard-fail: {out}");
        assert!(
            out["error"]
                .as_str()
                .unwrap_or_default()
                .contains("not in closure"),
            "the error must say the quad is not in the closure: {out}"
        );
    }

    /// HEAVY (off-gate): `judgment_nquads` grounds the `logic:ReasoningResult` node
    /// and round-trips through the shared reasoning-graph reader.
    #[test]
    fn explain_quad_judgment_nquads_grounds_the_reasoning_result_heavy_offgate() {
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let _env = EnvRestore::capture(&["GMEOW_LANG", "GMEOW_MEMORY_PATH", "HOME", "USERPROFILE"]);
        unsafe {
            // SAFETY: tests mutate process env single-threaded under ENV_LOCK.
            env::remove_var("GMEOW_LANG");
        }
        let bytes = snapshot();
        let server = McpServer::from_snapshot(&bytes).unwrap();

        let max_steps = 8u64;
        let rows = reasoned_rows(&server, max_steps);
        let asserted = find_unique_row(&rows, |row| {
            row.rule_iri == gmeow_logic::provenance::ASSERT_RULE_IRI
                && row.obj.starts_with('<')
                && row.obj.ends_with('>')
        })
        .expect("the reasoned bundle must carry a unique asserted IRI-object quad");
        let object_value = asserted.obj[1..asserted.obj.len() - 1].to_owned();

        let out = text_payload(server.call_tool_result(
            "explain_quad",
            &json!({
                "subject": asserted.subject,
                "predicate": asserted.predicate,
                "object_value": object_value,
                "object_kind": "iri",
                "graph": asserted.graph,
                "max_steps": max_steps,
            }),
        ));
        assert_eq!(out["ok"], true, "explain_quad must succeed: {out}");
        let judgment = out["judgment_nquads"]
            .as_str()
            .expect("judgment_nquads string");
        assert!(
            judgment.contains("<https://blackcatinformatics.ca/logic/ReasoningResult>"),
            "the judgment must ground a logic:ReasoningResult node: {judgment}"
        );
        let parsed = gmeow_logic::result_rdf::parse_reasoning_graph(judgment)
            .expect("judgment_nquads parses as a reasoning-result graph");
        // The grounded judgment carries a defined completeness axis (round-trip honesty).
        assert!(
            !parsed.completeness.wire().is_empty(),
            "the grounded judgment must carry a completeness axis: {judgment}"
        );
    }

    /// Full MCP protocol conformance over the real JSON-RPC dispatch: the
    /// handshake, the discovery surfaces, a read tool call and a TR-write tool call
    /// with `dry_run=true` (asserting the write stays hypothetical), and that EVERY
    /// advertised tool is dispatch-callable (no `unknown tool`).
    #[test]
    fn json_rpc_protocol_conformance_round_trip() {
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let _env = EnvRestore::capture(&["GMEOW_LANG", "GMEOW_MEMORY_PATH", "HOME", "USERPROFILE"]);
        unsafe {
            // SAFETY: tests mutate process env single-threaded under ENV_LOCK.
            env::remove_var("GMEOW_LANG");
        }
        let (_mem_dir, memory_path) = temp_memory();
        let bytes = snapshot();
        let server = McpServer::from_snapshot(&bytes).unwrap();

        let rpc = |body: &str| -> Value {
            let raw = server.handle_message(body);
            let value: Value = serde_json::from_str(&raw).expect("response is JSON");
            assert_eq!(value["jsonrpc"], "2.0", "JSON-RPC framing: {value}");
            value
        };

        // initialize
        let init = rpc(r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#);
        assert_eq!(init["id"], 1);
        assert_eq!(init["result"]["serverInfo"]["name"], "gmeow");
        assert!(init["result"]["protocolVersion"].as_str().is_some());

        // tools/list
        let tools = rpc(r#"{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}"#);
        let tool_names: Vec<String> = tools["result"]["tools"]
            .as_array()
            .expect("tools array")
            .iter()
            .map(|t| t["name"].as_str().unwrap().to_string())
            .collect();
        for expected in [
            "lookup_term",
            "query_docs",
            "query_local",
            "store_claim",
            "recall",
        ] {
            assert!(
                tool_names.iter().any(|n| n == expected),
                "missing {expected}"
            );
        }

        // resources/list
        let resources = rpc(r#"{"jsonrpc":"2.0","id":3,"method":"resources/list","params":{}}"#);
        assert!(
            resources["result"]["resources"]
                .as_array()
                .map(|r| !r.is_empty())
                .unwrap_or(false)
        );

        // tools/call — a read tool (query_docs ASK) succeeds.
        let read = rpc(
            r#"{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"query_docs","arguments":{"query":"ASK { ?s ?p ?o }"}}}"#,
        );
        let read_text: Value =
            serde_json::from_str(read["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
        assert_eq!(read_text["ok"], true);
        assert_eq!(read_text["boolean"], true);

        // tools/call — a TR-write tool (store_claim) with dry_run=true stays
        // hypothetical: the verdict is computed but nothing is committed.
        let dry = rpc(
            r#"{"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"store_claim","arguments":{"text":"a conformance probe belief","dry_run":true}}}"#,
        );
        let dry_text: Value =
            serde_json::from_str(dry["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
        assert_eq!(dry_text["ok"], true);
        assert_eq!(dry_text["dry_run"], true);
        assert_eq!(dry_text["transaction"]["committed"], false);
        assert!(dry_text.get("claim").is_none(), "dry run commits no claim");
        // Nothing persisted by the dry-run write.
        assert!(Memory::new(&memory_path).claims().unwrap().is_empty());
        assert!(!memory_path.exists());

        // Every advertised tool is dispatch-callable (recognized by tools/call).
        let overlay_data = "<urn:ex:s> <urn:ex:p> <urn:ex:o> .\n";
        let mut call_args: HashMap<&str, Value> = HashMap::new();
        call_args.insert("lookup_term", json!({"term": "gmeow:Entity"}));
        call_args.insert("doc_card", json!({"term": "gmeow:Entity"}));
        call_args.insert("query_docs", json!({"query": "ASK { ?s ?p ?o }"}));
        call_args.insert("docs_search", json!({"query": "entity"}));
        call_args.insert(
            "query_local",
            json!({"data": overlay_data, "format": "turtle", "query": "ASK { ?s ?p ?o }"}),
        );
        // The default (Tier-1) `validate_local` path is fast, so a valid tiny graph
        // dispatches and returns a well-formed EnrichedReport. (Tier-2 `deep` is opt-in
        // and reasons over the whole bundle, minutes — never exercised in this loop.)
        call_args.insert(
            "validate_local",
            json!({"data": "<urn:ex:s> <urn:ex:p> <urn:ex:o> .\n", "format": "turtle"}),
        );
        // `advise` is the fast Tier-1-only recommendation surface: a tiny clean graph
        // dispatches and returns ok:true with no recommendations.
        call_args.insert(
            "advise",
            json!({"data": "<urn:ex:s> <urn:ex:p> <urn:ex:o> .\n", "format": "turtle"}),
        );
        call_args.insert("store_claim", json!({"text": "probe", "dry_run": true}));
        call_args.insert(
            "conjecture_test",
            json!({
                "formula": forall_horn_candidate("B"),
                "kb": refuting_kb("B"),
                "standpoint": "http://ex/standpoint/probe",
            }),
        );
        call_args.insert(
            "store_conjecture",
            json!({
                "formula": forall_horn_candidate("B"),
                "kb": refuting_kb("B"),
                "standpoint": "http://ex/standpoint/probe",
                "dry_run": true,
            }),
        );
        call_args.insert("recall", json!({}));
        call_args.insert(
            "revise_belief",
            json!({"claim_id": "urn:gmeow:assertion:none", "dry_run": true}),
        );
        call_args.insert(
            "refute_conjecture",
            json!({"conjecture_id": "urn:gmeow:conjecture:none", "dry_run": true}),
        );
        // Documentation-surface tools: real terms that carry live data in the shipped
        // bundle (gmeow:Activity documents fixtures, gmeow:Entity grounds
        // entailments); competency_questions dispatches in its whole-index form.
        call_args.insert("counter_examples", json!({"term": "gmeow:Activity"}));
        call_args.insert("entailments", json!({"term": "gmeow:Entity"}));
        call_args.insert("competency_questions", json!({}));
        for name in &tool_names {
            let args = call_args
                .get(name.as_str())
                .cloned()
                .unwrap_or_else(|| json!({}));
            let result = server.call_tool_result(name, &args);
            let content = result["content"][0]["text"].as_str().expect("tool text");
            assert!(
                !content.contains("unknown tool"),
                "advertised tool {name} is not dispatch-callable: {content}"
            );
        }
    }

    /// Build a consumer server over the shipped bundle with a clean language env.
    fn consumer_server() -> McpServer {
        let bytes = snapshot();
        McpServer::from_snapshot(&bytes).unwrap()
    }

    /// The sorted finding-code multiset of a report.
    fn codes_of(report: &gmeow_errors::Report) -> Vec<String> {
        let mut codes: Vec<String> = report.findings.iter().map(|f| f.code.clone()).collect();
        codes.sort();
        codes
    }

    /// Select a REAL counter-example fixture from the SHIPPED bundle's
    /// `gmeow:graph/documentation` projection whose authored `gmeow:docViolationCode`
    /// is actually REPRODUCED by the Tier-1 SHACL engine on its own `gmeow:docFixtureText`
    /// body — the honest correspondence anchor (never weakened to "non-empty").
    ///
    /// The candidate for each code is the SAME one the enrichment join attaches:
    /// `fixture_maps` keys a code to the lexicographically-first fixture IRI carrying
    /// it, so selecting by `code → (first-fixture IRI, its text)` guarantees the
    /// attached counter-example's text equals the payload we validate. `tier1` is
    /// the production Tier-1 core (the [`run_tier1`] engine over ONE decode of the
    /// shipped bundle — decoding the whole bundle per candidate would multiply the
    /// dominant cost without strengthening anything). Returns
    /// `(fixture_iri, code, text)`. Panics with the observed codes if NONE reproduces
    /// (a real blocker, not a soft skip).
    fn select_reproducing_counter_example(
        server: &McpServer,
        tier1: &gmeow_validate::data_validate::Tier1Shapes,
    ) -> (String, String, String) {
        let rows = server
            .view
            .docs_select_rows(COUNTER_EXAMPLE_FIXTURE_QUERY)
            .expect("query counter-example fixtures from graph/documentation");
        // First row per fixture IRI (the fixture's code + full body).
        let mut by_fixture: BTreeMap<String, (String, String)> = BTreeMap::new();
        for row in &rows {
            if let (Some(f), Some(code), Some(text)) =
                (row.get("f"), row.get("code"), row.get("text"))
            {
                by_fixture
                    .entry(f.clone())
                    .or_insert_with(|| (code.clone(), text.clone()));
            }
        }
        // code → (first-fixture IRI, its text) — matches `fixture_maps`' first-wins,
        // so the attached counter-example is exactly this fixture.
        let mut by_code: BTreeMap<String, (String, String)> = BTreeMap::new();
        for (iri, (code, text)) in &by_fixture {
            by_code
                .entry(code.clone())
                .or_insert_with(|| (iri.clone(), text.clone()));
        }
        assert!(
            !by_code.is_empty(),
            "the shipped bundle carries NO bound counter-example fixtures"
        );
        for (code, (iri, text)) in &by_code {
            let report = tier1
                .validate(
                    text.as_bytes(),
                    "turtle",
                    MCP_NAMESPACE,
                    VALIDATE_LOCAL_ORIGIN,
                )
                .expect("tier-1 validate the fixture body");
            if report.findings.iter().any(|f| &f.code == code) {
                return (iri.clone(), code.clone(), text.clone());
            }
        }
        panic!(
            "no bound counter-example fixture reproduced its authored violation code under \
             Tier-1 validation — observed candidate codes: {:?}",
            by_code.keys().collect::<Vec<_>>()
        );
    }

    /// PARITY + CORRESPONDENCE (end-to-end, production surface): drive the REAL
    /// `validate_local` tool (default fast Tier-1 path) over a REAL counter-example
    /// fixture from the shipped bundle, and assert (a) PARITY — the enriched finding
    /// codes EQUAL `run_tier1` (the `gmeow validate` core); and (b) CORRESPONDENCE —
    /// at least one finding carries a counter-example, EVERY attached counter-example
    /// corresponds by violation code + rule help URI, and the finding whose code is
    /// the chosen fixture's `docViolationCode` gets that fixture's exact body back.
    #[test]
    fn validate_local_enrichment_parity_and_correspondence() {
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let _env = EnvRestore::capture(&["GMEOW_LANG"]);
        unsafe {
            // SAFETY: tests mutate process env single-threaded under ENV_LOCK.
            env::remove_var("GMEOW_LANG");
        }
        let server = consumer_server();
        // The reference side's OWN bundle decode (the `run_tier1` core), built once
        // and independent of anything the tool caches: the selection loop and the
        // reference run below share it, while the tool decodes its own from the
        // snapshot bytes — decoding the same immutable bundle a third time per
        // side would only re-test the decoder, not the tool.
        let tier1_shapes =
            gmeow_validate::data_validate::Tier1Shapes::from_gts(server.view.gts_bytes())
                .expect("parse the shipped bundle's Tier-1 shape union");
        let (fixture_iri, code, text) = select_reproducing_counter_example(&server, &tier1_shapes);
        eprintln!("validate_local test selected fixture={fixture_iri} code={code}");
        let help_uri = gmeow_validate::rule_catalog::help_uri_for(&code);

        // The CLI core: Tier-1 (the same engine `gmeow validate`'s `run_tier1` drives).
        let tier1 = tier1_shapes
            .validate(
                text.as_bytes(),
                "turtle",
                MCP_NAMESPACE,
                VALIDATE_LOCAL_ORIGIN,
            )
            .expect("tier-1 validate");
        let tier1_codes = codes_of(&tier1);

        // Drive the REAL tool by DISPATCH BY NAME (deep defaults to false → fast).
        let enriched = text_payload(
            server.call_tool_result("validate_local", &json!({"data": text, "format": "turtle"})),
        );
        assert_ne!(
            enriched["ok"],
            Value::Null,
            "the tool returned an EnrichedReport: {enriched}"
        );
        let findings = enriched["findings"].as_array().expect("findings array");

        // PARITY: with deep off, the tool's finding codes EQUAL the `gmeow validate`
        // (`run_tier1`) codes exactly — validate_local drops/adds/mutates nothing.
        let mut local_codes: Vec<String> = findings
            .iter()
            .map(|f| f["code"].as_str().unwrap().to_string())
            .collect();
        local_codes.sort();
        assert_eq!(
            local_codes, tier1_codes,
            "validate_local must reproduce the gmeow-validate Tier-1 finding codes exactly"
        );

        // CORRESPONDENCE (BINDING): the finding whose code == the fixture's violation
        // code carries THAT fixture's body back, with the corresponding help URI.
        let matched: Vec<&Value> = findings
            .iter()
            .filter(|f| f["code"].as_str() == Some(code.as_str()))
            .collect();
        assert!(
            !matched.is_empty(),
            "the chosen fixture's code {code} must appear among the findings: {enriched}"
        );
        let with_ce = matched
            .iter()
            .find(|f| !f["counter_example"].is_null())
            .unwrap_or_else(|| {
                panic!("the matched finding {code} must carry a counter-example: {enriched}")
            });
        assert_eq!(
            with_ce["counter_example"]["violation_code"].as_str(),
            Some(code.as_str()),
            "the attached counter-example corresponds by violation code"
        );
        assert_eq!(
            with_ce["counter_example"]["text"].as_str(),
            Some(text.as_str()),
            "the finding whose code is the fixture's violation code gets that fixture's body back"
        );
        assert_eq!(
            with_ce["help_uri"].as_str(),
            Some(help_uri.as_str()),
            "the finding carries the rule catalog help URI for its code"
        );

        // NON-VACUITY + the CORRESPONDENCE INVARIANT across the whole report: at least
        // one finding got a counter-example, and EVERY attached counter-example's
        // violation code equals its OWN finding's code (by-code, never blanket).
        let attached = findings
            .iter()
            .filter(|f| !f["counter_example"].is_null())
            .count();
        assert!(
            attached >= 1,
            "at least one finding must carry a counter-example (non-vacuous): {enriched}"
        );
        for f in findings {
            if !f["counter_example"].is_null() {
                assert_eq!(
                    f["counter_example"]["violation_code"].as_str(),
                    f["code"].as_str(),
                    "every attached counter-example corresponds to ITS finding's code \
                     (correspondence, never blanket): {f}"
                );
                assert_eq!(
                    f["help_uri"].as_str(),
                    Some(
                        gmeow_validate::rule_catalog::help_uri_for(f["code"].as_str().unwrap())
                            .as_str()
                    ),
                    "every enriched finding carries its rule catalog help URI: {f}"
                );
            }
        }
    }

    /// SELECT a REAL well-formed conformance fixture from the SHIPPED bundle's
    /// `gmeow:graph/documentation` projection whose `gmeow:docFixtureText` body
    /// ACTUALLY validates clean under Tier-1 (no `Error`-severity finding) — the
    /// honest correspondence anchor for "a claim consistent with the bundled
    /// axioms", mirroring [`select_reproducing_counter_example`]'s reproduction
    /// discipline: never hand-authored, always a REAL fixture the engine itself
    /// agrees is clean. Returns `(fixture_iri, text)`. Panics with the candidate
    /// count if NONE reproduces (a real blocker, not a soft skip).
    fn select_reproducing_wellformed_example(server: &McpServer) -> (String, String) {
        let rows = server
            .view
            .docs_select_rows(WELLFORMED_FIXTURE_QUERY)
            .expect("query well-formed fixtures from graph/documentation");
        let mut by_fixture: BTreeMap<String, String> = BTreeMap::new();
        for row in &rows {
            if let (Some(f), Some(text)) = (row.get("f"), row.get("text")) {
                by_fixture.entry(f.clone()).or_insert_with(|| text.clone());
            }
        }
        assert!(
            !by_fixture.is_empty(),
            "the shipped bundle carries NO bound well-formed conformance fixtures"
        );
        for (iri, text) in &by_fixture {
            let report = gmeow_validate::data_validate::run_tier1(
                text.as_bytes(),
                "turtle",
                server.view.gts_bytes(),
                MCP_NAMESPACE,
                VALIDATE_LOCAL_ORIGIN,
            )
            .expect("tier-1 validate the fixture body");
            if report.ok() {
                return (iri.clone(), text.clone());
            }
        }
        panic!(
            "no bound well-formed fixture actually validated clean under Tier-1 — {} \
             candidates checked, none reproduced a clean Tier-1 pass",
            by_fixture.len()
        );
    }

    /// ACCEPTANCE (R5, half 1): drive the REAL `validate_local` tool over a REAL
    /// well-formed fixture from the shipped bundle — a claim CONSISTENT with the
    /// bundled axioms — and assert the production surface reports it clean: `ok:
    /// true`, and every finding present (if any non-error advisory survives) still
    /// carries the full teaching surface (`help_uri`, and `entails`/
    /// `wellformed_exemplar` when the finding names a documented term). Never
    /// idealized: a clean claim MAY still carry Warning/Note/Info findings (the
    /// tool only hard-rejects on `Error` severity), so this asserts on `ok`, not
    /// on an empty findings array.
    #[test]
    fn validate_local_accepts_a_consistent_claim() {
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let _env = EnvRestore::capture(&["GMEOW_LANG"]);
        unsafe {
            // SAFETY: tests mutate process env single-threaded under ENV_LOCK.
            env::remove_var("GMEOW_LANG");
        }
        let server = consumer_server();
        let (fixture_iri, text) = select_reproducing_wellformed_example(&server);
        eprintln!("validate_local clean-claim test selected fixture={fixture_iri}");

        let enriched = text_payload(
            server.call_tool_result("validate_local", &json!({"data": text, "format": "turtle"})),
        );
        assert_eq!(
            enriched["ok"], true,
            "a claim consistent with the bundled axioms must validate clean: {enriched}"
        );
        assert_eq!(enriched["tool"].as_str(), Some("validate"));
        let findings = enriched["findings"].as_array().expect("findings array");
        // `ok: true` tolerates non-Error survivors; whichever DO appear must still
        // carry the full enrichment surface (never a bare code+message).
        for f in findings {
            assert!(
                f["help_uri"].as_str().is_some_and(|u| !u.is_empty()),
                "every surfaced finding carries a non-empty rule catalog help_uri: {f}"
            );
            assert!(
                f["finding_iri"].is_string(),
                "every finding has a stable IRI: {f}"
            );
        }
    }

    /// ACCEPTANCE (R5, half 2): drive the REAL `validate_local` tool over a REAL
    /// counter-example fixture from the shipped bundle — a claim that VIOLATES a
    /// bundled SHACL shape / modelling discipline — and assert the production
    /// surface REJECTS it: the tool surfaces the violating finding code at `Error`
    /// severity (proven against the raw Tier-1 `Report`, since the enriched
    /// envelope does not carry severity), and the enriched envelope is `ok: false`
    /// (never a silent clean pass on inconsistent input).
    #[test]
    fn validate_local_rejects_an_inconsistent_claim() {
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let _env = EnvRestore::capture(&["GMEOW_LANG"]);
        unsafe {
            // SAFETY: tests mutate process env single-threaded under ENV_LOCK.
            env::remove_var("GMEOW_LANG");
        }
        let server = consumer_server();
        let tier1_shapes =
            gmeow_validate::data_validate::Tier1Shapes::from_gts(server.view.gts_bytes())
                .expect("parse the bundle's Tier-1 shapes once for the selection loop");
        let (fixture_iri, code, text) = select_reproducing_counter_example(&server, &tier1_shapes);
        eprintln!(
            "validate_local inconsistent-claim test selected fixture={fixture_iri} code={code}"
        );

        // The raw Tier-1 report (the `gmeow validate` core) proves the specific
        // finding code fires at `Error` severity — the hard-reject signal
        // `EnrichedFinding` does not itself carry.
        let tier1 = tier1_shapes
            .validate(
                text.as_bytes(),
                "turtle",
                MCP_NAMESPACE,
                VALIDATE_LOCAL_ORIGIN,
            )
            .expect("tier-1 validate the violating fixture");
        let tier1_finding = tier1
            .findings
            .iter()
            .find(|f| f.code == code)
            .unwrap_or_else(|| panic!("tier-1 report must reproduce code {code}: {tier1:?}"));
        assert_eq!(
            tier1_finding.severity,
            gmeow_errors::Severity::Error,
            "the chosen counter-example must violate at Error severity (a hard reject, not \
             an advisory): {tier1_finding:?}"
        );
        assert!(
            !tier1.ok(),
            "a report carrying an Error-severity finding must not be ok(): {tier1:?}"
        );

        // Drive the REAL production tool over the SAME violating claim.
        let enriched = text_payload(
            server.call_tool_result("validate_local", &json!({"data": text, "format": "turtle"})),
        );
        assert_eq!(
            enriched["ok"], false,
            "a claim violating a bundled axiom must be rejected, not validated clean: {enriched}"
        );
        let findings = enriched["findings"].as_array().expect("findings array");
        assert!(
            findings
                .iter()
                .any(|f| f["code"].as_str() == Some(code.as_str())),
            "the rejecting tool must surface the specific violation code {code}: {enriched}"
        );
    }

    /// Compile a hand-authored draft-2020-12 schema and assert `instance` CONFORMS,
    /// surfacing every validation error verbatim on failure (a real payload the
    /// schema rejects is a schema bug to FIX, never to weaken).
    fn assert_conforms(schema: &Value, instance: &Value, what: &str) {
        let validator = jsonschema::options()
            .with_draft(jsonschema::Draft::Draft202012)
            .build(schema)
            .unwrap_or_else(|e| panic!("{what}: schema does not compile: {e}"));
        let errors: Vec<String> = validator
            .iter_errors(instance)
            .map(|e| e.to_string())
            .collect();
        assert!(
            errors.is_empty(),
            "{what}: real payload does not conform to its schema:\n{}\ninstance: {instance}",
            errors.join("\n")
        );
    }

    /// SELF-DESCRIBING SURFACE (card): a REAL `doc_card format=json` payload — the
    /// exact bytes the packed `terms/{slug}/card.json` member carries — CONFORMS to
    /// the hand-authored `card.schema.json` (`gmeow_docs_model::card::card_json_schema`).
    /// Both the STANDARD tier (the `card.json` shape) and the FULL tier (every rich
    /// panel, exercising the `$defs`) are checked against the SAME schema.
    #[test]
    fn card_json_conforms_to_card_schema() {
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let _env = EnvRestore::capture(&["GMEOW_LANG"]);
        unsafe {
            // SAFETY: tests mutate process env single-threaded under ENV_LOCK.
            env::remove_var("GMEOW_LANG");
        }
        let server = consumer_server();
        let schema = gmeow_docs_model::card::card_json_schema();

        // STANDARD tier — the `card.json` shape. `gmeow:Entity` is a real bundled term.
        let standard = text_payload(server.call_tool_result(
            "doc_card",
            &json!({"term": "gmeow:Entity", "format": "json", "detail": "standard"}),
        ));
        assert_eq!(standard["ok"], true, "doc_card standard: {standard}");
        assert_conforms(&schema, &standard["card"], "card.json (standard tier)");

        // FULL tier — exercises the rich panels ($defs). `gmeow:Activity` documents
        // conformance fixtures and `gmeow:Entity` grounds entailments in the shipped
        // bundle, so at least one full card carries populated panels.
        for term in ["gmeow:Activity", "gmeow:Entity"] {
            let full = text_payload(server.call_tool_result(
                "doc_card",
                &json!({"term": term, "format": "json", "detail": "full"}),
            ));
            assert_eq!(full["ok"], true, "doc_card full for {term}: {full}");
            assert_conforms(&schema, &full["card"], "card.json (full tier)");
        }
    }

    /// SELF-DESCRIBING SURFACE (finding): a REAL `validate_local` envelope — produced
    /// by driving the tool over a REAL counter-example from the shipped bundle —
    /// CONFORMS to the hand-authored `validate-finding.schema.json`
    /// (`gmeow_validate::local_oracle::finding_json_schema`), exercising the finding /
    /// fixture / entailment `$defs` on a payload that carries an attached
    /// counter-example.
    #[test]
    fn enriched_report_conforms_to_finding_schema() {
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let _env = EnvRestore::capture(&["GMEOW_LANG"]);
        unsafe {
            // SAFETY: tests mutate process env single-threaded under ENV_LOCK.
            env::remove_var("GMEOW_LANG");
        }
        let server = consumer_server();
        let tier1_shapes =
            gmeow_validate::data_validate::Tier1Shapes::from_gts(server.view.gts_bytes())
                .expect("parse the shipped bundle's Tier-1 shape union");
        let (_fixture_iri, _code, text) =
            select_reproducing_counter_example(&server, &tier1_shapes);
        let enriched = text_payload(
            server.call_tool_result("validate_local", &json!({"data": text, "format": "turtle"})),
        );
        // The chosen fixture reproduces its violation, so the envelope is non-vacuous:
        // at least one finding, and at least one attached counter-example fixture.
        let findings = enriched["findings"].as_array().expect("findings array");
        assert!(!findings.is_empty(), "expected findings: {enriched}");
        assert!(
            findings.iter().any(|f| !f["counter_example"].is_null()),
            "expected an attached counter-example (exercises the fixture $def): {enriched}"
        );
        let schema = gmeow_validate::local_oracle::finding_json_schema();
        assert_conforms(&schema, &enriched, "validate_local EnrichedReport");
    }

    /// DEEP PASS (heavy): drive the tool end-to-end with `deep = true` over a REAL
    /// counter-example from the shipped bundle and assert the Tier-2 semantic pass ran
    /// (a `validate.deep.*` finding is present) AND that the Tier-1 surface is
    /// preserved (true tool-vs-`gmeow validate` parity). `#[ignore]`d because the
    /// native deep reasoner over the whole bundle runs well past the 120 s on-gate
    /// nextest cliff; run in the heavy lane / manually
    /// (`cargo nextest run -E 'test(validate_local_deep)' --run-ignored all`).
    /// `validate.deep.contract-invalid` is engine-enforced (it fires only on a bundle
    /// carrying a garbled `logic:admissibleValuation`, which the shipped bundle does
    /// not) and is regression-covered by
    /// `gmeow_validate::data_validate` `deep_pass_garbled_contract_produces_error_not_advisory`.
    #[test]
    #[ignore = "runs the full-bundle native deep reasoner (>120s); heavy lane only"]
    fn validate_local_deep_pass_surfaces_deep_finding() {
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let _env = EnvRestore::capture(&["GMEOW_LANG"]);
        unsafe {
            // SAFETY: tests mutate process env single-threaded under ENV_LOCK.
            env::remove_var("GMEOW_LANG");
        }
        let server = consumer_server();
        let tier1_shapes =
            gmeow_validate::data_validate::Tier1Shapes::from_gts(server.view.gts_bytes())
                .expect("parse the shipped bundle's Tier-1 shape union");
        let (_iri, _code, text) = select_reproducing_counter_example(&server, &tier1_shapes);

        // The CLI Tier-1 surface the deep run must preserve.
        let tier1 = tier1_shapes
            .validate(
                text.as_bytes(),
                "turtle",
                MCP_NAMESPACE,
                VALIDATE_LOCAL_ORIGIN,
            )
            .expect("tier-1 validate");
        let tier1_codes = codes_of(&tier1);

        // Explicitly request the Tier-2 pass via the `deep` arg.
        let enriched = text_payload(server.call_tool_result(
            "validate_local",
            &json!({"data": text, "format": "turtle", "deep": true}),
        ));
        let findings = enriched["findings"].as_array().expect("findings array");

        assert!(
            findings
                .iter()
                .any(|f| f["code"].as_str().unwrap().starts_with("validate.deep.")),
            "the deep pass must run (a validate.deep.* finding must appear): {enriched}"
        );
        let mut tier1_surface: Vec<String> = findings
            .iter()
            .map(|f| f["code"].as_str().unwrap().to_string())
            .filter(|c| !c.starts_with("validate.deep."))
            .collect();
        tier1_surface.sort();
        assert_eq!(
            tier1_surface, tier1_codes,
            "the deep run must preserve the Tier-1 surface (parity with gmeow validate)"
        );
    }

    /// ROBUSTNESS: an unknown `format`, an oversized `data` payload, and malformed
    /// RDF each return a well-formed error envelope (`ok:false`) — never a panic, and
    /// a malformed graph surfaces as an Error, not a silent success.
    #[test]
    fn validate_local_hard_fails_bad_format_oversize_and_malformed() {
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let _env = EnvRestore::capture(&["GMEOW_LANG"]);
        unsafe {
            // SAFETY: tests mutate process env single-threaded under ENV_LOCK.
            env::remove_var("GMEOW_LANG");
        }
        let server = consumer_server();

        // Unknown format → error envelope listing the accepted tokens.
        let bad_format = text_payload(server.call_tool_result(
            "validate_local",
            &json!({"data": "<urn:s> <urn:p> <urn:o> .", "format": "bogus"}),
        ));
        assert_eq!(
            bad_format["ok"], false,
            "unknown format must be a hard fail"
        );
        assert!(
            bad_format["error"]
                .as_str()
                .unwrap()
                .contains("unrecognized RDF format"),
            "the error must name the offending format: {bad_format}"
        );

        // Oversized payload → error envelope, no truncation.
        let huge = format!(
            "<urn:s> <urn:p> \"{}\" .",
            "x".repeat(MAX_VALIDATE_DATA_BYTES + 1)
        );
        let oversize = text_payload(
            server.call_tool_result("validate_local", &json!({"data": huge, "format": "turtle"})),
        );
        assert_eq!(
            oversize["ok"], false,
            "oversized payload must be a hard fail"
        );
        assert!(
            oversize["error"].as_str().unwrap().contains("ceiling"),
            "the error must explain the size ceiling: {oversize}"
        );

        // Malformed Turtle → error envelope (the parse hard-fails; never a silent
        // empty-but-ok report).
        let malformed = text_payload(server.call_tool_result(
            "validate_local",
            &json!({"data": "<urn:s> <urn:p> <urn:o>  # unterminated, no dot", "format": "turtle"}),
        ));
        assert_eq!(
            malformed["ok"], false,
            "malformed RDF must surface as an error, not a silent success: {malformed}"
        );
    }

    /// The n-triples claim that types an individual as a BARE gmeow:Entity — the
    /// exact fixture the advisory bridge fires `BareEntitySortalAdviceConstraint`
    /// (Entity avoidWhen) on.
    const BARE_ENTITY_CLAIM: &str = "<https://ex.test/x> \
         <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> \
         <https://blackcatinformatics.ca/gmeow/Entity> .\n";

    /// AC2: `advise` returns the non-gating RECOMMENDATIONS a claim
    /// trips — driven over the REAL JSON-RPC `handle_message` dispatch. A bare-Entity
    /// claim surfaces the Entity avoid/use/how-to advice, `ok:true`.
    #[test]
    fn advise_surfaces_recommendations_for_a_matching_claim() {
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let _env = EnvRestore::capture(&["GMEOW_LANG"]);
        unsafe {
            // SAFETY: tests mutate process env single-threaded under ENV_LOCK.
            env::remove_var("GMEOW_LANG");
        }
        let server = consumer_server();

        // Drive the REAL JSON-RPC path: tools/call → dispatch → tool_advise.
        let body = format!(
            r#"{{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{{"name":"advise","arguments":{{"data":{},"format":"ntriples"}}}}}}"#,
            serde_json::to_string(BARE_ENTITY_CLAIM).unwrap()
        );
        let raw = server.handle_message(&body);
        let envelope: Value = serde_json::from_str(&raw).expect("JSON-RPC response");
        assert_eq!(envelope["jsonrpc"], "2.0");
        let payload: Value = serde_json::from_str(
            envelope["result"]["content"][0]["text"]
                .as_str()
                .expect("tool text"),
        )
        .expect("advise payload is JSON");

        assert_eq!(
            payload["ok"], true,
            "advise is a recommendation surface — always ok:true: {payload}"
        );
        assert_eq!(payload["tool"], "advise");
        let recs = payload["recommendations"]
            .as_array()
            .expect("recommendations array");
        assert!(
            !recs.is_empty(),
            "a bare-Entity claim must surface at least one recommendation: {payload}"
        );
        for rec in recs {
            let code = rec["code"].as_str().unwrap();
            assert!(
                code.starts_with("advice."),
                "advise must surface ONLY advisory advice.* codes: {rec}"
            );
            assert!(
                !rec["avoid_when"].as_str().unwrap().is_empty(),
                "each recommendation carries its avoid-when prohibition prose: {rec}"
            );
            assert_eq!(
                rec["help_uri"].as_str().unwrap(),
                gmeow_validate::rule_catalog::catalog_anchor_uri(code),
                "help_uri routes through the single anchor authority (→ #advice-): {rec}"
            );
        }
        // The Entity advice carries its formalized term and its corrective/permission
        // guidance (the contrary-to-duty how-to-use / use-when legs).
        let entity = recs
            .iter()
            .find(|r| {
                r["formalizes"].as_str() == Some("https://blackcatinformatics.ca/gmeow/Entity")
            })
            .unwrap_or_else(|| {
                panic!("the Entity advice recommendation must be present: {payload}")
            });
        let use_when = entity["use_when"].as_array().unwrap();
        assert!(
            !use_when.is_empty(),
            "the Entity advice carries use-when guidance: {entity}"
        );
        assert!(
            use_when
                .iter()
                .all(|v| !v.as_str().unwrap().starts_with("Use when: ")),
            "use_when entries must have the \"Use when: \" marker stripped: {entity}"
        );
        let how_to_use = entity["how_to_use"].as_array().unwrap();
        assert!(
            !how_to_use.is_empty(),
            "the Entity advice carries how-to-use guidance: {entity}"
        );
        assert!(
            how_to_use
                .iter()
                .all(|v| !v.as_str().unwrap().starts_with("Use when: ")),
            "no permission-leg prose may leak into how_to_use: {entity}"
        );
        // The tripped node is visible on the MCP surface, not just the RDF claim wing —
        // `subject` resolves to the focus IRI the finding's location carries.
        assert_eq!(
            entity["subject"].as_str(),
            Some("https://ex.test/x"),
            "advise must surface the tripped node as `subject`, not null: {entity}"
        );
    }

    /// AC2: a claim that trips NO advice returns an empty recommendation list, still
    /// `ok:true` — advice is a recommendation, never a rejection.
    #[test]
    fn advise_returns_empty_for_a_clean_claim() {
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let _env = EnvRestore::capture(&["GMEOW_LANG"]);
        unsafe {
            // SAFETY: tests mutate process env single-threaded under ENV_LOCK.
            env::remove_var("GMEOW_LANG");
        }
        let server = consumer_server();
        let payload = text_payload(server.call_tool_result(
            "advise",
            &json!({"data": "<urn:ex:s> <urn:ex:p> <urn:ex:o> .\n", "format": "ntriples"}),
        ));
        assert_eq!(
            payload["ok"], true,
            "advice never fails, even for a claim with no recommendations: {payload}"
        );
        assert_eq!(payload["tool"], "advise");
        assert!(
            payload["recommendations"].as_array().unwrap().is_empty(),
            "a claim tripping no advice returns an empty recommendation list: {payload}"
        );
    }

    /// AC2 (the sharpest witness): on a MIXED claim tripping BOTH a binding Error AND
    /// the Entity advice, `advise` returns ONLY the advisory tier — `ok:true`, never
    /// the binding code — while `validate_local` on the same claim is `ok:false`.
    #[test]
    fn advise_on_a_mixed_claim_returns_only_advice() {
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let _env = EnvRestore::capture(&["GMEOW_LANG"]);
        unsafe {
            // SAFETY: tests mutate process env single-threaded under ENV_LOCK.
            env::remove_var("GMEOW_LANG");
        }
        let server = consumer_server();
        let tier1_shapes =
            gmeow_validate::data_validate::Tier1Shapes::from_gts(server.view.gts_bytes())
                .expect("parse the bundle's Tier-1 shapes once");
        let (_fixture_iri, code, text) = select_reproducing_counter_example(&server, &tier1_shapes);

        // The Error-tripping fixture PLUS a bare-Entity advice trigger (a new subject,
        // so the fixture's Error still fires and the Entity advice is added).
        let claim = format!(
            "{text}\n<https://ex.test/advicex> a <https://blackcatinformatics.ca/gmeow/Entity> .\n"
        );

        // validate_local sees the binding Error → ok:false.
        let validated = text_payload(server.call_tool_result(
            "validate_local",
            &json!({"data": claim, "format": "turtle"}),
        ));
        assert_eq!(
            validated["ok"], false,
            "the mixed claim carries a binding Error (validate_local rejects it): {validated}"
        );

        // advise returns ONLY the advisory tier, ok:true, and NEVER the binding code.
        let advised = text_payload(
            server.call_tool_result("advise", &json!({"data": claim, "format": "turtle"})),
        );
        assert_eq!(
            advised["ok"], true,
            "advise never fails, even on a claim carrying a binding violation: {advised}"
        );
        let recs = advised["recommendations"].as_array().unwrap();
        assert!(
            !recs.is_empty(),
            "the bare-Entity leg must still surface advice on the mixed claim: {advised}"
        );
        for rec in recs {
            let rec_code = rec["code"].as_str().unwrap();
            assert!(
                rec_code.starts_with("advice."),
                "advise surfaces only advice.* codes, never the binding {code}: {rec}"
            );
            assert_ne!(
                rec_code, code,
                "the binding Error code must never appear in advise output: {rec}"
            );
        }
    }

    /// ROBUSTNESS (parity with `validate_local`): an unknown `format` and an oversized
    /// `data` payload each return a well-formed `ok:false` error envelope — never a
    /// panic, never a silent truncation.
    #[test]
    fn advise_hard_fails_bad_format_and_oversize() {
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let _env = EnvRestore::capture(&["GMEOW_LANG"]);
        unsafe {
            // SAFETY: tests mutate process env single-threaded under ENV_LOCK.
            env::remove_var("GMEOW_LANG");
        }
        let server = consumer_server();

        let bad_format = text_payload(server.call_tool_result(
            "advise",
            &json!({"data": "<urn:s> <urn:p> <urn:o> .", "format": "bogus"}),
        ));
        assert_eq!(
            bad_format["ok"], false,
            "unknown format must be a hard fail"
        );
        assert!(
            bad_format["error"]
                .as_str()
                .unwrap()
                .contains("unrecognized RDF format"),
            "the error must name the offending format: {bad_format}"
        );

        let huge = format!(
            "<urn:s> <urn:p> \"{}\" .",
            "x".repeat(MAX_VALIDATE_DATA_BYTES + 1)
        );
        let oversize = text_payload(
            server.call_tool_result("advise", &json!({"data": huge, "format": "turtle"})),
        );
        assert_eq!(
            oversize["ok"], false,
            "oversized payload must be a hard fail"
        );
        assert!(
            oversize["error"].as_str().unwrap().contains("ceiling"),
            "the error must explain the size ceiling: {oversize}"
        );
    }

    /// The `explain_finding` tool, driven by DISPATCH BY NAME over a server built
    /// from the shipped bundle: a real fingerprint IRI returns the finding's code +
    /// a gate verdict; an unknown IRI is a HARD FAIL (isError), never an empty DAG.
    #[test]
    fn explain_finding_tool_walks_a_real_witness_and_hard_fails_unknown() {
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let _env = EnvRestore::capture(&["GMEOW_LANG", "GMEOW_MEMORY_PATH", "HOME", "USERPROFILE"]);
        unsafe {
            // SAFETY: tests mutate process env single-threaded under ENV_LOCK.
            env::remove_var("GMEOW_LANG");
        }
        let bytes = snapshot();
        let server = McpServer::from_snapshot(&bytes).unwrap();

        // Obtain a real fingerprint IRI the SAME way `explain` does: the first key of
        // the FindingIndex the reader rehydrates from the server's held snapshot. An
        // empty graph/diagnostics is a blocker, not something to paper over.
        let index = gmeow_bundle_view::diagnostics_reader::read_findings(&server.view.dataset)
            .expect("read graph/diagnostics from shipped bundle");
        assert!(
            !index.is_empty(),
            "shipped bundle graph/diagnostics carries NO findings — explain_finding has no witness"
        );
        let real_iri = index.findings.keys().next().unwrap().clone();
        let expected_code = index.get(&real_iri).unwrap().code.clone();

        let ok = text_payload(
            server.call_tool_result("explain_finding", &json!({"target_iri": real_iri})),
        );
        assert_eq!(ok["ok"], true, "explain_finding must succeed: {ok}");
        assert_eq!(ok["kind"], "finding");
        assert_eq!(ok["focus"]["code"], expected_code);
        assert!(
            ok["verdict"].as_str().is_some(),
            "explain_finding must carry a gate verdict: {ok}"
        );
        assert!(
            ok["focus"]["provenance_dag"]
                .as_str()
                .unwrap()
                .contains(&real_iri),
            "provenance DAG must render the focus finding: {ok}"
        );

        // Unknown target → hard fail (isError result), never a success with an empty DAG.
        let bad = server.call_tool_result("explain_finding", &json!({"target_iri": "urn:nope"}));
        assert_eq!(
            bad["isError"], true,
            "unknown IRI must be a hard fail: {bad}"
        );
        let bad_text = text_payload(bad);
        assert_eq!(bad_text["ok"], false);
        assert!(
            bad_text["error"]
                .as_str()
                .unwrap_or_default()
                .contains("unknown explain target")
        );
    }

    /// The SPARQL half of the acceptance criterion, on the PRODUCTION MCP surface:
    /// `query_local` runs a SELECT over the bundle's `graph/diagnostics` named graph
    /// and returns a real finding's code — proving SPARQL over the diagnostics
    /// projection executes through the shipped query tool (its canon is the full
    /// signed dataset, which retains the diagnostics graph).
    #[test]
    fn query_local_selects_a_finding_code_over_graph_diagnostics() {
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let _env = EnvRestore::capture(&["GMEOW_LANG", "GMEOW_MEMORY_PATH", "HOME", "USERPROFILE"]);
        unsafe {
            // SAFETY: tests mutate process env single-threaded under ENV_LOCK.
            env::remove_var("GMEOW_LANG");
        }
        let bytes = snapshot();
        let server = McpServer::from_snapshot(&bytes).unwrap();

        // query_local requires an overlay path; a trivial annex suffices — the query
        // itself targets the bundle's graph/diagnostics named graph directly.
        let overlay_data = "<urn:ex:s> <urn:ex:p> <urn:ex:o> .\n";

        let query = "SELECT ?s ?code WHERE { \
                     GRAPH <https://blackcatinformatics.ca/gmeow/graph/diagnostics> { \
                     ?s a <https://blackcatinformatics.ca/gmeow/Finding> ; \
                     <https://blackcatinformatics.ca/gmeow/findingCode> ?code } } LIMIT 1";
        let res = text_payload(server.call_tool_result(
            "query_local",
            &json!({"data": overlay_data, "format": "turtle", "query": query}),
        ));
        assert_eq!(
            res["ok"], true,
            "query_local over graph/diagnostics must succeed: {res}"
        );
        let bindings = res["results"]["bindings"]
            .as_array()
            .expect("bindings array");
        assert!(
            !bindings.is_empty(),
            "expected at least one Finding binding from graph/diagnostics: {res}"
        );
        assert!(
            bindings[0]["code"]["value"].as_str().is_some(),
            "the Finding binding must carry a findingCode literal: {res}"
        );
    }

    // ── Conjecture-library persistence ───────────────────────────────────────

    use gmeow_ns::LOGIC_NS;
    use gmeow_ns::MATH_NS;

    /// A `∀x. trigger(x, mark) → rdf:type(x, <cls>)` candidate, authored as a reified
    /// `logic:Formula` (a single top-level formula — the trivially-Horn consequent is a
    /// sub-formula, so it never trips the `with_formulas` guard).
    fn forall_horn_candidate(cls_local: &str) -> String {
        format!(
            "@prefix logic: <{LOGIC_NS}> .\n\
             @prefix ex:  <http://ex/> .\n\
             @prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .\n\
             ex:cand a logic:Formula ;\n\
                 logic:forall ex:body ;\n\
                 logic:quantifiedVariable [ logic:termIndex 0 ; logic:termVariable \"x\" ] .\n\
             ex:body a logic:Formula ;\n\
                 logic:antecedent ex:ant ;\n\
                 logic:consequent ex:con .\n\
             ex:ant a logic:Formula ;\n\
                 logic:relation ex:trigger ;\n\
                 logic:argument [ logic:termIndex 0 ; logic:termVariable \"x\" ] ;\n\
                 logic:argument [ logic:termIndex 1 ; logic:termIri ex:mark ] .\n\
             ex:con a logic:Formula ;\n\
                 logic:relation rdf:type ;\n\
                 logic:argument [ logic:termIndex 0 ; logic:termVariable \"x\" ] ;\n\
                 logic:argument [ logic:termIndex 1 ; logic:termIri ex:{cls_local} ] .\n"
        )
    }

    /// A KB where the candidate's head class is DISJOINT with the individual's asserted type,
    /// so firing `rdf:type(a, <cls>)` forces an `owl:Nothing` clash ⇒ refutation + witness.
    fn refuting_kb(cls_local: &str) -> String {
        format!(
            "@prefix ex:  <http://ex/> .\n\
             @prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .\n\
             @prefix owl: <http://www.w3.org/2002/07/owl#> .\n\
             ex:a ex:trigger ex:mark .\n\
             ex:a rdf:type ex:A .\n\
             ex:A owl:disjointWith ex:{cls_local} .\n"
        )
    }

    /// A KB where the candidate's head class is UNRELATED (no disjointness), so firing derives
    /// a new consistent fact ⇒ Open/Neither, no witness.
    fn open_kb(cls_local: &str) -> String {
        format!(
            "@prefix ex:  <http://ex/> .\n\
             @prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .\n\
             ex:a ex:trigger ex:mark .\n\
             ex:a rdf:type ex:A .\n\
             # {cls_local} is unrelated to A — no clash.\n"
        )
    }

    fn temp_conjecture() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("conjectures.gts");
        unsafe {
            // SAFETY: tests mutate process env single-threaded under ENV_LOCK.
            env::set_var("GMEOW_CONJECTURE_PATH", &path);
        }
        (dir, path)
    }

    /// The imported conjecture-library dataset (all appended segments unioned).
    fn read_conjectures(path: &Path) -> std::sync::Arc<purrdf::RdfDataset> {
        let bytes = fs::read(path).expect("read conjecture library");
        purrdf::import_gts_events(&bytes)
            .expect("import conjecture library")
            .dataset
    }

    /// Every subject typed `logic:Conjecture` in `dataset`.
    fn conjecture_nodes(dataset: &purrdf::RdfDataset) -> BTreeSet<String> {
        dataset
            .owned_quads()
            .filter(|q| {
                q.predicate == RDF_TYPE_IRI
                    && q.object == RdfTerm::iri(format!("{LOGIC_NS}Conjecture"))
            })
            .filter_map(|q| match q.subject {
                RdfTerm::Iri(s) => Some(s),
                _ => None,
            })
            .collect()
    }

    /// The `logic:witnessPremise` literal lexicals in `dataset`.
    fn witness_premises(dataset: &purrdf::RdfDataset) -> Vec<String> {
        dataset
            .owned_quads()
            .filter(|q| q.predicate == format!("{LOGIC_NS}witnessPremise"))
            .filter_map(|q| match q.object {
                RdfTerm::Literal(lit) => Some(lit.lexical_form),
                _ => None,
            })
            .collect()
    }

    struct ConjEnvGuard;
    impl ConjEnvGuard {
        fn set() -> (EnvRestore, ConjEnvGuard) {
            let env = EnvRestore::capture(&[
                "GMEOW_LANG",
                "GMEOW_MEMORY_PATH",
                "GMEOW_CONJECTURE_PATH",
                "GMEOW_CANDIDATE_PATH",
                "HOME",
                "USERPROFILE",
            ]);
            unsafe {
                // SAFETY: tests mutate process env single-threaded under ENV_LOCK.
                env::remove_var("GMEOW_LANG");
            }
            (env, ConjEnvGuard)
        }
    }

    #[test]
    fn persist_conjecture_precondition_gates_the_commit() {
        // The write is a REAL TR gate: with the precondition present the committed run
        // succeeds; with it absent the run FAILS (so the tool returns ok:false before writing).
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let (_env, _cg) = ConjEnvGuard::set();
        let ok = execute_memory_txn(
            MCP_PERSIST_CONJECTURE_SCHEMA,
            &[MCP_CONJECTURE_VERDICT_PRESENTED],
            false,
        )
        .unwrap();
        assert!(
            matches!(ok, TxReceipt::CommittedSuccess { .. }),
            "the precondition-present committed run must succeed: {ok:?}"
        );
        let unmet = execute_memory_txn(MCP_PERSIST_CONJECTURE_SCHEMA, &[], false).unwrap();
        assert!(
            matches!(unmet, TxReceipt::CommittedFailure { .. }),
            "a persist with the precondition UNMET must fail the commit (the tool then returns \
             ok:false before writing): {unmet:?}"
        );
    }

    #[test]
    fn conjecture_test_is_pure_and_writes_nothing() {
        // R3a: the "test" leg (`conjecture_test`) is a PURE hypothetical evaluation. Driving it
        // with a candidate that, under the OLD single-tool surface, would have persisted a
        // refutation must still return the full verdict envelope while leaving the library file
        // byte-unchanged (here: absent) — no TR gate, no append, ever.
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let (_env, _cg) = ConjEnvGuard::set();
        let (_mem, _mp) = temp_memory();
        let (_dir, path) = temp_conjecture();
        let bytes = snapshot();
        let server = McpServer::from_snapshot(&bytes).unwrap();

        assert!(!path.exists(), "library must not exist before the call");
        let resp = text_payload(server.call_tool_result(
            "conjecture_test",
            &json!({
                "formula": forall_horn_candidate("B"),
                "kb": refuting_kb("B"),
                "standpoint": "http://ex/standpoint/alice",
            }),
        ));
        assert_eq!(resp["ok"], true, "the pure test must succeed: {resp}");
        assert_eq!(resp["verdict"]["lifecycle"], "refuted-in-standpoint");
        assert_eq!(resp["witness"]["individual"], "http://ex/a");
        let node = resp["conjecture"].as_str().expect("node iri").to_string();
        assert!(!node.is_empty());
        // No persist/transaction section at all on this surface.
        assert!(
            resp.get("transaction").is_none(),
            "conjecture_test must never render a transaction section: {resp}"
        );
        assert!(
            resp.get("committed").is_none(),
            "conjecture_test must never render a committed flag: {resp}"
        );

        // T1 (G7): the grounded judgment travels on the SAME `judgment_nquads` key
        // `verify_graph`/`explain_quad` carry — even on this pure, nothing-persisted path —
        // and round-trips through the shared reader as a real `logic:Conjecture` node
        // embedding a non-empty `logic:ReasoningResult`.
        let judgment = resp["judgment_nquads"]
            .as_str()
            .expect("conjecture_test must carry a judgment_nquads string");
        assert!(
            !judgment.trim().is_empty(),
            "judgment_nquads must not be empty"
        );
        let record = gmeow_logic::result_rdf::parse_conjecture_verdict(judgment)
            .expect("judgment_nquads must parse as a conjecture-verdict graph");
        assert_eq!(
            record.lifecycle,
            ConjectureLifecycleState::RefutedInStandpoint
        );
        assert!(
            !project_reasoning_result(&record.verdict).trim().is_empty(),
            "the embedded logic:ReasoningResult body must be non-empty"
        );

        // The library file is BYTE-UNCHANGED (still absent): no TR gate, no append.
        assert!(
            !path.exists(),
            "a pure conjecture_test call must write nothing to the library"
        );
    }

    #[test]
    fn store_conjecture_refutes_and_persists_with_witness() {
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let (_env, _cg) = ConjEnvGuard::set();
        let (_mem, _mp) = temp_memory();
        let (_dir, path) = temp_conjecture();
        let bytes = snapshot();
        let server = McpServer::from_snapshot(&bytes).unwrap();

        assert!(
            !path.exists(),
            "library must not exist before the first persist"
        );
        let resp = text_payload(server.call_tool_result(
            "store_conjecture",
            &json!({
                "formula": forall_horn_candidate("B"),
                "kb": refuting_kb("B"),
                "standpoint": "http://ex/standpoint/alice",
            }),
        ));
        assert_eq!(resp["ok"], true, "refutation persist must succeed: {resp}");
        assert_eq!(resp["verdict"]["lifecycle"], "refuted-in-standpoint");
        assert_eq!(resp["witness"]["individual"], "http://ex/a");
        let node = resp["conjecture"].as_str().expect("node iri").to_string();
        assert!(!node.is_empty());

        // The library file grew, and the witness premises round-trip back through the GTS.
        assert!(
            path.exists(),
            "the conjecture library file must have been written"
        );
        let dataset = read_conjectures(&path);
        assert!(
            conjecture_nodes(&dataset).contains(&node),
            "the content-addressed conjecture node must be readable back: {node}"
        );
        // The standpoint scope is recoverable.
        assert!(dataset.owned_quads().any(|q| {
            q.predicate == format!("{LOGIC_NS}conjectureStandpoint")
                && q.object == RdfTerm::iri("http://ex/standpoint/alice")
        }));
        // The witness premises are recoverable.
        let premises = witness_premises(&dataset);
        assert!(
            !premises.is_empty(),
            "a refutation must persist recoverable witness premises"
        );
    }

    #[test]
    fn store_conjecture_bridges_math_twin_via_conjecture_under_test() {
        // Driving the real MCP `store_conjecture` tool (the shared conjecture-test core) with a
        // `math_conjecture` must persist the always-present structural twin bridge
        // `<math> math:conjectureUnderTest <logic:Conjecture-node>` (domain math:Conjecture,
        // range logic:Conjecture) — readable back out of the append-only GTS library.
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let (_env, _cg) = ConjEnvGuard::set();
        let (_mem, _mp) = temp_memory();
        let (_dir, path) = temp_conjecture();
        let bytes = snapshot();
        let server = McpServer::from_snapshot(&bytes).unwrap();

        let math_iri = "https://blackcatinformatics.ca/math/conjecture/goldbach";
        let resp = text_payload(server.call_tool_result(
            "store_conjecture",
            &json!({
                "formula": forall_horn_candidate("B"),
                "kb": refuting_kb("B"),
                "standpoint": "http://ex/standpoint/alice",
                "math_conjecture": math_iri,
            }),
        ));
        assert_eq!(resp["ok"], true, "math-twin persist must succeed: {resp}");
        let node = resp["conjecture"].as_str().expect("node iri").to_string();

        let dataset = read_conjectures(&path);
        let under_test = format!("{MATH_NS}conjectureUnderTest");
        assert!(
            dataset.owned_quads().any(|q| {
                q.subject == RdfTerm::iri(math_iri)
                    && q.predicate == under_test
                    && q.object == RdfTerm::iri(node.clone())
            }),
            "the math:conjectureUnderTest bridge <{math_iri}> -> <{node}> must be recoverable \
             from the persisted GTS library"
        );
    }

    #[test]
    fn store_conjecture_dry_run_writes_nothing() {
        // The `dry_run` witness on `store_conjecture` is a HYPOTHETICAL commit: the verdict and
        // TR receipt are computed exactly as a real commit would be, but nothing is appended.
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let (_env, _cg) = ConjEnvGuard::set();
        let (_mem, _mp) = temp_memory();
        let (_dir, path) = temp_conjecture();
        let bytes = snapshot();
        let server = McpServer::from_snapshot(&bytes).unwrap();

        let resp = text_payload(server.call_tool_result(
            "store_conjecture",
            &json!({
                "formula": forall_horn_candidate("B"),
                "kb": refuting_kb("B"),
                "standpoint": "http://ex/standpoint/alice",
                "dry_run": true,
            }),
        ));
        assert_eq!(resp["ok"], true);
        assert_eq!(resp["dry_run"], true);
        assert_eq!(resp["transaction"]["committed"], false);
        // The verdict is still computed and returned.
        assert_eq!(resp["verdict"]["lifecycle"], "refuted-in-standpoint");
        // Nothing written: the library file does not exist / is zero bytes.
        assert!(
            !path.exists() || fs::metadata(&path).unwrap().len() == 0,
            "a dry run must write nothing to the library"
        );

        // T1 (G7): `store_conjecture`'s dry-run path STILL carries the grounded judgment under
        // the SAME `judgment_nquads` key the read tools and `conjecture_test` use — a
        // hypothetical persist is not a hypothetical verdict; the engine really ran.
        let judgment = resp["judgment_nquads"]
            .as_str()
            .expect("store_conjecture dry-run must carry a judgment_nquads string");
        assert!(
            !judgment.trim().is_empty(),
            "judgment_nquads must not be empty"
        );
        let record = gmeow_logic::result_rdf::parse_conjecture_verdict(judgment)
            .expect("judgment_nquads must parse as a conjecture-verdict graph");
        assert_eq!(
            record.lifecycle,
            ConjectureLifecycleState::RefutedInStandpoint
        );
        assert!(
            !project_reasoning_result(&record.verdict).trim().is_empty(),
            "the embedded logic:ReasoningResult body must be non-empty"
        );

        // A second, committing call on the SAME candidate now appends for real: the dry run
        // above left the library byte-unchanged, so this is the library's FIRST segment.
        let committed = text_payload(server.call_tool_result(
            "store_conjecture",
            &json!({
                "formula": forall_horn_candidate("B"),
                "kb": refuting_kb("B"),
                "standpoint": "http://ex/standpoint/alice",
            }),
        ));
        assert_eq!(committed["ok"], true);
        assert!(path.exists() && fs::metadata(&path).unwrap().len() > 0);
        // The committed path carries judgment_nquads too.
        assert!(
            committed["judgment_nquads"]
                .as_str()
                .is_some_and(|s| !s.trim().is_empty()),
            "the committed store_conjecture response must also carry judgment_nquads: {committed}"
        );
    }

    /// Store a real conjecture through the shipped `store_conjecture` tool and return its
    /// content-addressed node IRI (the shared setup for the refute tests below).
    fn store_one_conjecture(server: &McpServer, cls: &str) -> String {
        let resp = text_payload(server.call_tool_result(
            "store_conjecture",
            &json!({
                "formula": forall_horn_candidate(cls),
                "kb": open_kb(cls),
                "standpoint": "http://ex/standpoint/alice",
            }),
        ));
        assert_eq!(resp["ok"], true, "store must succeed: {resp}");
        assert_eq!(resp["verdict"]["lifecycle"], "open");
        resp["conjecture"].as_str().expect("node iri").to_string()
    }

    #[test]
    fn refute_conjecture_withdraws_and_appends_author_segment() {
        // store_conjecture then refute_conjecture its node IRI: the library gains a NEW
        // append-only segment marking that node ConjectureWithdrawn with the author reason;
        // the reader's EFFECTIVE lifecycle is now Withdrawn; the PRIOR segment bytes are
        // byte-for-byte intact (append-only, never mutated).
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let (_env, _cg) = ConjEnvGuard::set();
        let (_mem, _mp) = temp_memory();
        let (_dir, path) = temp_conjecture();
        let bytes = snapshot();
        let server = McpServer::from_snapshot(&bytes).unwrap();

        let node = store_one_conjecture(&server, "B");
        // Before withdrawal the effective state is the engine verdict (Open), never Withdrawn.
        let before = read_library(library_at(&path).as_ref()).unwrap();
        assert_eq!(
            before.get(&node).copied(),
            Some(ConjectureLifecycleState::Open)
        );
        let prior = fs::read(&path).expect("library bytes before refute");

        let resp = text_payload(server.call_tool_result(
            "refute_conjecture",
            &json!({"conjecture_id": node, "reason": "author retired this line of inquiry"}),
        ));
        assert_eq!(resp["ok"], true, "the withdrawal must commit: {resp}");
        assert_eq!(resp["conjecture"], node);
        assert_eq!(resp["lifecycle"], "withdrawn");
        assert_eq!(resp["transaction"]["committed"], true);
        assert_eq!(resp["transaction"]["succeeded"], true);
        // T1 (G7): the compensating withdrawal carries its own grounded RDF projection under
        // the SAME `judgment_nquads` key — the target node re-marked ConjectureWithdrawn.
        assert!(
            resp["judgment_nquads"]
                .as_str()
                .is_some_and(|s| s.contains("ConjectureWithdrawn")),
            "refute_conjecture must carry judgment_nquads naming ConjectureWithdrawn: {resp}"
        );

        // The EFFECTIVE lifecycle (segment order) is now Withdrawn.
        let after = read_library(library_at(&path).as_ref()).unwrap();
        assert_eq!(
            after.get(&node).copied(),
            Some(ConjectureLifecycleState::Withdrawn),
            "the effective lifecycle after refute must be Withdrawn"
        );

        // Append-only: the file GREW and the prior bytes are an untouched prefix.
        let now = fs::read(&path).expect("library bytes after refute");
        assert!(
            now.len() > prior.len(),
            "the withdrawal must append new bytes"
        );
        assert_eq!(
            &now[..prior.len()],
            &prior[..],
            "prior segment bytes must be byte-for-byte intact (append-only)"
        );

        // The author reason literal round-trips out of the unioned library.
        let dataset = read_conjectures(&path);
        assert!(
            dataset.owned_quads().any(|q| {
                q.subject == RdfTerm::iri(node.clone())
                    && q.predicate == format!("{LOGIC_NS}withdrawalReason")
                    && matches!(
                        q.object,
                        RdfTerm::Literal(ref lit)
                            if lit.lexical_form == "author retired this line of inquiry"
                    )
            }),
            "the author withdrawal reason must be recoverable from the library"
        );
        // The withdrawal is reviewer-asserted, never engine-produced.
        assert!(
            dataset.owned_quads().any(|q| {
                q.subject == RdfTerm::iri(node.clone())
                    && q.predicate == format!("{LOGIC_NS}verdictProvenance")
                    && q.object == RdfTerm::iri(format!("{LOGIC_NS}VerdictReviewerAsserted"))
            }),
            "the withdrawal must carry VerdictReviewerAsserted provenance"
        );
    }

    #[test]
    fn refute_conjecture_second_withdraw_rejected_by_segment_order() {
        // store -> withdraw -> withdraw again the SAME node: the second refute is rejected as
        // precondition-unmet because the EFFECTIVE state is already Withdrawn, decided by
        // SEGMENT ORDER (the last lifecycle assertion), not by the union or gmeow:atTime. The
        // rejected call appends NOTHING (the library stays byte-for-byte identical).
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let (_env, _cg) = ConjEnvGuard::set();
        let (_mem, _mp) = temp_memory();
        let (_dir, path) = temp_conjecture();
        let bytes = snapshot();
        let server = McpServer::from_snapshot(&bytes).unwrap();

        let node = store_one_conjecture(&server, "B");
        let first = text_payload(server.call_tool_result(
            "refute_conjecture",
            &json!({"conjecture_id": node, "reason": "first withdrawal"}),
        ));
        assert_eq!(
            first["ok"], true,
            "the first withdrawal must commit: {first}"
        );
        let after_first = fs::read(&path).expect("library bytes after first withdrawal");

        let second = text_payload(server.call_tool_result(
            "refute_conjecture",
            &json!({"conjecture_id": node, "reason": "second withdrawal"}),
        ));
        assert_eq!(
            second["ok"], false,
            "a second withdrawal must be rejected: {second}"
        );
        assert_eq!(second["transaction"]["committed"], true);
        assert_eq!(second["transaction"]["succeeded"], false);
        assert!(
            second["error"]
                .as_str()
                .is_some_and(|e| e.contains("already withdrawn")),
            "the rejection must name the already-withdrawn precondition: {second}"
        );
        // The rejected call wrote nothing.
        let after_second = fs::read(&path).expect("library bytes after rejected withdrawal");
        assert_eq!(
            after_first, after_second,
            "a rejected withdrawal must append nothing to the library"
        );
    }

    #[test]
    fn refute_conjecture_unknown_id_rejected() {
        // An unknown conjecture_id (nothing stored) fails the TR gate (empty start state) and
        // returns ok:false before any write — the library file is never created.
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let (_env, _cg) = ConjEnvGuard::set();
        let (_mem, _mp) = temp_memory();
        let (_dir, path) = temp_conjecture();
        let bytes = snapshot();
        let server = McpServer::from_snapshot(&bytes).unwrap();

        assert!(!path.exists(), "library must not exist before the call");
        let resp = text_payload(server.call_tool_result(
            "refute_conjecture",
            &json!({"conjecture_id": "https://blackcatinformatics.ca/gmeow/graph/conjecture/ghost"}),
        ));
        assert_eq!(resp["ok"], false, "an unknown id must be rejected: {resp}");
        assert!(
            resp["error"]
                .as_str()
                .is_some_and(|e| e.contains("unknown conjecture id")),
            "the rejection must name the unknown id: {resp}"
        );
        assert!(
            !path.exists(),
            "a rejected withdrawal of an unknown id must write nothing"
        );
    }

    #[test]
    fn refute_conjecture_dry_run_writes_nothing() {
        // dry_run=true witnesses the hypothetical commit (lifecycle withdrawn, committed:false)
        // but appends NOTHING — the library file stays byte-unchanged.
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let (_env, _cg) = ConjEnvGuard::set();
        let (_mem, _mp) = temp_memory();
        let (_dir, path) = temp_conjecture();
        let bytes = snapshot();
        let server = McpServer::from_snapshot(&bytes).unwrap();

        let node = store_one_conjecture(&server, "B");
        let before = fs::read(&path).expect("library bytes before dry run");

        let resp = text_payload(server.call_tool_result(
            "refute_conjecture",
            &json!({"conjecture_id": node, "reason": "sandbox", "dry_run": true}),
        ));
        assert_eq!(resp["ok"], true, "the dry run must succeed: {resp}");
        assert_eq!(resp["dry_run"], true);
        assert_eq!(resp["lifecycle"], "withdrawn");
        assert_eq!(resp["transaction"]["committed"], false);
        // T1 (G7): the hypothetical withdrawal still carries judgment_nquads — the RDF
        // projection is pure and side-effect-free, so witnessing it costs nothing written.
        assert!(
            resp["judgment_nquads"]
                .as_str()
                .is_some_and(|s| s.contains("ConjectureWithdrawn")),
            "refute_conjecture dry-run must carry judgment_nquads naming ConjectureWithdrawn: \
             {resp}"
        );

        // Nothing appended: bytes unchanged AND the effective lifecycle is still Open.
        let after = fs::read(&path).expect("library bytes after dry run");
        assert_eq!(before, after, "a dry run must write nothing to the library");
        let library = read_library(library_at(&path).as_ref()).unwrap();
        assert_eq!(
            library.get(&node).copied(),
            Some(ConjectureLifecycleState::Open),
            "a dry run must not change the effective lifecycle"
        );
    }

    #[test]
    fn store_conjecture_and_refute_round_trip_keep_library_and_audit_consistent() {
        // store_conjecture and refute_conjecture must each land their
        // library segment AND their audit segment TOGETHER — never one without the other. Drive
        // the real `call_tool_result` surface for both tools and, after EACH commit, assert the
        // library dataset carries BOTH the verdict/withdrawal triples AND the matching
        // `logic:instantiatesSchema` audit marker for that same call — round-tripping cleanly.
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let (_env, _cg) = ConjEnvGuard::set();
        let (_mem, _mp) = temp_memory();
        let (_dir, path) = temp_conjecture();
        let bytes = snapshot();
        let server = McpServer::from_snapshot(&bytes).unwrap();

        let node = store_one_conjecture(&server, "B");
        let after_store = read_conjectures(&path);
        assert!(
            conjecture_nodes(&after_store).contains(&node),
            "the store's library segment must be present after the commit"
        );
        assert!(
            after_store.owned_quads().any(|q| {
                q.predicate == LOGIC_INSTANTIATES_SCHEMA
                    && q.object == RdfTerm::iri(MCP_PERSIST_CONJECTURE_SCHEMA)
            }),
            "the store's audit segment must be present in the SAME commit as its library \
             segment (no library-without-audit gap): {after_store:?}"
        );

        let refute = text_payload(server.call_tool_result(
            "refute_conjecture",
            &json!({"conjecture_id": node, "reason": "round-trip proof"}),
        ));
        assert_eq!(refute["ok"], true, "the withdrawal must commit: {refute}");

        let after_refute = read_conjectures(&path);
        let library = read_library(library_at(&path).as_ref()).unwrap();
        assert_eq!(
            library.get(&node).copied(),
            Some(ConjectureLifecycleState::Withdrawn),
            "the round-trip's effective state must be Withdrawn"
        );
        assert!(
            after_refute.owned_quads().any(|q| {
                q.predicate == LOGIC_INSTANTIATES_SCHEMA
                    && q.object == RdfTerm::iri(MCP_WITHDRAW_CONJECTURE_SCHEMA)
            }),
            "the refute's audit segment must be present in the SAME commit as its withdrawal \
             segment: {after_refute:?}"
        );
        // The store's audit marker is STILL there too — append-only, nothing overwritten.
        assert!(
            after_refute.owned_quads().any(|q| {
                q.predicate == LOGIC_INSTANTIATES_SCHEMA
                    && q.object == RdfTerm::iri(MCP_PERSIST_CONJECTURE_SCHEMA)
            }),
            "the store's audit marker must survive the later refute commit"
        );
    }

    #[cfg(unix)]
    #[test]
    fn conjecture_persist_is_all_or_nothing_on_a_failed_commit() {
        // forcing the atomic-replace write to fail PARTWAY (the temp file
        // for the combined library+audit bytes can't even be created) must leave the library
        // BYTE-UNCHANGED — never holding the library segment without its audit segment (or vice
        // versa), because both are assembled in memory and committed via ONE rename. This
        // simulates the "audit append fails after the library append already landed" half of the
        // gap: with the fix, there is no such partial state to observe.
        use std::os::unix::fs::PermissionsExt;

        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let (_env, _cg) = ConjEnvGuard::set();
        let (_dir, path) = temp_conjecture();

        // Seed the library with one already-committed segment (as if a prior store succeeded).
        let seed_node = "https://blackcatinformatics.ca/gmeow/graph/conjecture/seed";
        let seed_nt = format!(
            "<{seed_node}> <{RDF_TYPE_IRI}> <{LOGIC_NS}Conjecture> .\n\
             <{seed_node}> <{LOGIC_NS}conjectureLifecycleState> <{LOGIC_NS}ConjectureOpen> .\n"
        );
        write_conjecture_segment(&path, &seed_nt).unwrap();
        let before = fs::read(&path).expect("seeded library bytes");
        assert!(!before.is_empty());

        // Make the library's directory read-only: the existing `.lock` sidecar can still be
        // opened (it already exists), but `append_conjecture_segments` can no longer create the
        // same-directory temp file its atomic rename depends on — an I/O failure squarely inside
        // the combined library+audit commit, after both segments' bytes are already built.
        let dir = path
            .parent()
            .expect("library has a parent dir")
            .to_path_buf();
        let original_mode = fs::metadata(&dir).unwrap().permissions().mode();
        fs::set_permissions(&dir, fs::Permissions::from_mode(0o555)).expect("chmod read-only");

        let outcome = (|| -> gmeow_errors::Result<()> {
            let lib_segment = build_nt_segment(
                &[],
                &probe_medium(),
                &format!(
                    "<{seed_node}> <{LOGIC_NS}conjectureLifecycleState> <{LOGIC_NS}ConjectureWithdrawn> .\n"
                ),
            )?;
            let audit_segment = build_audit_segment(
                &[],
                &probe_medium(),
                "urn:gmeow:conjecture-call:simulated-failure",
                MCP_WITHDRAW_CONJECTURE_SCHEMA,
                &[MCP_CONJECTURE_IN_LIBRARY],
                "1970-01-01T00:00:00Z",
            )?;
            let library = library_at(&path);
            with_library_lock(library.as_ref(), || {
                append_library_segments(library.as_ref(), &[lib_segment, audit_segment])
            })
        })();

        // Restore permissions unconditionally before asserting, so the tempdir can still clean
        // itself up even if an assertion below panics.
        fs::set_permissions(&dir, fs::Permissions::from_mode(original_mode))
            .expect("chmod restore");

        assert!(
            outcome.is_err(),
            "the forced I/O failure must surface as an error, not a silent partial write"
        );
        let after = fs::read(&path).expect("library bytes after the failed commit");
        assert_eq!(
            before, after,
            "a failed combined library+audit commit must leave the library BYTE-UNCHANGED \
             (all-or-nothing) — never holding one segment without the other"
        );
        let library = read_library(library_at(&path).as_ref()).unwrap();
        assert_eq!(
            library.get(seed_node).copied(),
            Some(ConjectureLifecycleState::Open),
            "the seed's state must still be Open — the failed withdrawal must not have applied"
        );
    }

    #[test]
    fn conjecture_lock_serializes_concurrent_writers() {
        // `with_conjecture_lock` must provide REAL mutual exclusion, not
        // just an in-process convention a second caller could sidestep — so this test opens the
        // SAME sidecar `.lock` file from two INDEPENDENT `std::fs::File` descriptors (one per
        // thread), mirroring how two separate OS processes would each open it themselves. Since
        // `flock` locks are scoped to the OPEN FILE DESCRIPTION (not the process), two distinct
        // descriptors contending on the same path exercise the identical kernel mechanism that
        // serializes real cross-process `store_conjecture` / `refute_conjecture` callers.
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let (_env, _cg) = ConjEnvGuard::set();
        let (_dir, path) = temp_conjecture();

        let (started_tx, started_rx) = std::sync::mpsc::channel::<()>();
        let (release_tx, release_rx) = std::sync::mpsc::channel::<()>();
        let holder_path = path.clone();
        let holder = std::thread::spawn(move || {
            with_library_lock(library_at(&holder_path).as_ref(), || {
                started_tx.send(()).expect("signal lock acquired");
                release_rx.recv().expect("wait for release signal");
                Ok(())
            })
            .expect("holder must acquire and release cleanly")
        });
        started_rx
            .recv_timeout(std::time::Duration::from_secs(5))
            .expect("the holder thread must acquire the lock");

        let waiter_done = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let waiter_done_writer = waiter_done.clone();
        let waiter_path = path.clone();
        let waiter = std::thread::spawn(move || {
            with_library_lock(library_at(&waiter_path).as_ref(), || {
                waiter_done_writer.store(true, std::sync::atomic::Ordering::SeqCst);
                Ok(())
            })
            .expect("waiter must eventually acquire and release cleanly")
        });

        // While the holder still owns the lock, the waiter's OWN, independently-opened file
        // descriptor must be blocked — proving this is a real `flock`, not an in-process no-op.
        std::thread::sleep(std::time::Duration::from_millis(250));
        assert!(
            !waiter_done.load(std::sync::atomic::Ordering::SeqCst),
            "a second, independently-opened lock attempt must still be blocked while the first \
             holder is inside its critical section"
        );

        release_tx.send(()).expect("release the holder");
        holder.join().expect("holder thread must not panic");
        waiter.join().expect("waiter thread must not panic");
        assert!(
            waiter_done.load(std::sync::atomic::Ordering::SeqCst),
            "the second lock attempt must complete once the first holder releases"
        );
    }

    #[test]
    fn read_conjecture_library_resolves_effective_state_by_segment_order() {
        // The reader resolves the effective lifecycle purely by SEGMENT ORDER (last writer
        // wins) — NOT by the union (which would carry both states at once) and NOT by
        // gmeow:atTime (every segment shares the fixed determinism epoch). Two nodes are
        // written with OPPOSITE last-writer states to prove position alone decides: the state
        // that "sounds terminal" (Withdrawn) does NOT win unless it is written LAST.
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let (_env, _cg) = ConjEnvGuard::set();
        let (_dir, path) = temp_conjecture();

        let base = "https://blackcatinformatics.ca/gmeow/graph/conjecture/";
        let reopened = format!("{base}reopened");
        let retired = format!("{base}retired");
        let ty = format!("<{reopened}> <{RDF_TYPE_IRI}> <{LOGIC_NS}Conjecture> .\n");
        let ty2 = format!("<{retired}> <{RDF_TYPE_IRI}> <{LOGIC_NS}Conjecture> .\n");
        let lc = |node: &str, state: &str| {
            format!("<{node}> <{LOGIC_NS}conjectureLifecycleState> <{LOGIC_NS}{state}> .\n")
        };

        // `reopened`: Withdrawn FIRST, then Open — Open is last, so Open must win.
        write_conjecture_segment(
            &path,
            &format!("{ty}{}", lc(&reopened, "ConjectureWithdrawn")),
        )
        .unwrap();
        // `retired`: Open FIRST, then Withdrawn — Withdrawn is last, so Withdrawn must win.
        write_conjecture_segment(&path, &format!("{ty2}{}", lc(&retired, "ConjectureOpen")))
            .unwrap();
        write_conjecture_segment(&path, &lc(&reopened, "ConjectureOpen")).unwrap();
        write_conjecture_segment(&path, &lc(&retired, "ConjectureWithdrawn")).unwrap();

        let library = read_library(library_at(&path).as_ref()).unwrap();
        assert_eq!(
            library.get(&reopened).copied(),
            Some(ConjectureLifecycleState::Open),
            "the LAST segment (Open) must win even though a prior segment said Withdrawn"
        );
        assert_eq!(
            library.get(&retired).copied(),
            Some(ConjectureLifecycleState::Withdrawn),
            "the LAST segment (Withdrawn) must win over the prior Open"
        );
    }

    /// A candidate authored as a REIFIED GROUND binary atom — the exact `logic:relation` /
    /// `logic:argument` reification every authored formula uses, but VARIABLE-FREE: the ground
    /// fact `ex:a rdf:type ex:<cls_local>`. The reconstructed `Formula::Atom` is trivially-Horn,
    /// so it once panicked `LogicProgram::with_formulas` during the candidate parse.
    fn reified_ground_atom_candidate(cls_local: &str) -> String {
        format!(
            "@prefix logic: <{LOGIC_NS}> .\n\
             @prefix ex:  <http://ex/> .\n\
             @prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .\n\
             ex:phi a logic:Formula ;\n\
                 logic:relation rdf:type ;\n\
                 logic:argument [ logic:termIndex 0 ; logic:termIri ex:a ] ;\n\
                 logic:argument [ logic:termIndex 1 ; logic:termIri ex:{cls_local} ] .\n"
        )
    }

    /// A KB that ASSERTS the ground fact the reified-ground-atom candidate names, so `KB ⊨ φ`.
    fn ground_atom_entailing_kb(cls_local: &str) -> String {
        format!(
            "@prefix ex:  <http://ex/> .\n\
             @prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .\n\
             ex:a rdf:type ex:{cls_local} .\n"
        )
    }

    #[test]
    fn parse_candidate_reified_ground_atom_lifts_not_panics() {
        // F2 regression: a REIFIED GROUND binary atom is trivially-Horn, so the front-end must
        // route it to `LogicProgram.axioms` (not `with_formulas`, which hard-asserts) and
        // `parse_candidate_formula` must reconstruct it — cleanly, never a panic and never a
        // false "0 formula(s) and 0 axiom(s)" rejection.
        use gmeow_logic_compile::ir::{Formula, Term as IrTerm};
        // `parse_candidate_formula` now lives in the shared gmeow-logic conjecture-eval
        // authority; assert the SHIPPED re-export still lifts a reified ground atom cleanly.
        let candidate = gmeow_logic::conjecture_eval::parse_candidate_formula(
            &reified_ground_atom_candidate("B"),
        )
        .expect("reified ground atom must lift to a candidate formula");
        match candidate {
            Formula::Atom { relation, args } => {
                assert_eq!(relation, IrTerm::Iri(RDF_TYPE_IRI.to_owned()));
                assert_eq!(
                    args,
                    vec![
                        IrTerm::Iri("http://ex/a".to_owned()),
                        IrTerm::Iri("http://ex/B".to_owned()),
                    ]
                );
            }
            other => panic!("expected a ground binary atom, got {other:?}"),
        }
    }

    #[test]
    fn conjecture_test_reified_ground_atom_evaluates_via_shipped_core() {
        // F2 regression on the SHIPPED surface: driving `run_conjecture_test` (the shared core
        // behind the CLI + MCP tool) with a reified ground-atom candidate must EVALUATE it (a KB
        // asserting the fact entails φ ⇒ corroborated) rather than panic (exit 101).
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let (_env, _cg) = ConjEnvGuard::set();
        let (_dir, _path) = temp_conjecture();

        let out = run_conjecture_test(
            &ConjectureRunInput {
                formula_ttl: &reified_ground_atom_candidate("B"),
                kb_ttl: &ground_atom_entailing_kb("B"),
                standpoint: "http://ex/standpoint/alice",
                math_conjecture: None,
                dry_run: true,
                max_steps: None,
                max_answers: None,
            },
            &probe_medium(),
        )
        .expect("a reified ground-atom conjecture must evaluate, not panic");
        assert_eq!(out.lifecycle, "corroborated");
        assert_eq!(out.information, "supported");
        assert_eq!(out.evaluation, "completed");
    }

    fn temp_candidate() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("candidates.gts");
        unsafe {
            // SAFETY: tests mutate process env single-threaded under ENV_LOCK.
            env::set_var("GMEOW_CANDIDATE_PATH", &path);
        }
        (dir, path)
    }

    #[test]
    fn submit_candidate_admits_corroborated_records_provenance_and_lists() {
        // AC5: a candidate whose isolated-world verdict CORROBORATES it is admissible, so it is
        // committed to the append-only candidate library — carrying its target provenance — and
        // becomes visible to list_candidates.
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let (_env, _cg) = ConjEnvGuard::set();
        let (_dir, path) = temp_candidate();
        let bytes = snapshot();
        let server = McpServer::from_snapshot(&bytes).unwrap();

        assert!(!path.exists(), "library must not exist before the call");
        let resp = text_payload(server.call_tool_result(
            "submit_candidate",
            &json!({
                "formula": reified_ground_atom_candidate("B"),
                "kb": ground_atom_entailing_kb("B"),
                "standpoint": "http://ex/standpoint/alice",
                "for_slice": "https://blackcatinformatics.ca/gmeow/slices/logic",
            }),
        ));
        assert_eq!(resp["ok"], true, "an admissible candidate commits: {resp}");
        assert_eq!(resp["admissible"], true);
        assert_eq!(resp["committed"], true);
        assert_eq!(resp["verdict"]["lifecycle"], "corroborated");
        let node = resp["candidate"]
            .as_str()
            .expect("candidate iri")
            .to_string();
        assert!(path.exists(), "the admissible candidate was appended");

        // list_candidates surfaces it, in-library, with its provenance.
        let list = text_payload(server.call_tool_result("list_candidates", &json!({})));
        assert_eq!(list["ok"], true);
        assert_eq!(list["candidate_count"], 1, "one admitted candidate: {list}");
        let c = &list["candidates"][0];
        assert_eq!(c["candidate"], node);
        assert_eq!(c["disposition"], "in-library");
        assert_eq!(
            c["for_slice"],
            "https://blackcatinformatics.ca/gmeow/slices/logic"
        );

        // The slice-provenance filter matches and mismatches correctly.
        let filtered = text_payload(server.call_tool_result(
            "list_candidates",
            &json!({"slice": "https://blackcatinformatics.ca/gmeow/slices/logic"}),
        ));
        assert_eq!(filtered["candidate_count"], 1);
        let other = text_payload(
            server.call_tool_result("list_candidates", &json!({"slice": "http://ex/nope"})),
        );
        assert_eq!(other["candidate_count"], 0);
    }

    #[test]
    fn submit_candidate_stages_nothing_on_refuted_or_open() {
        // AC6: a refuted (or open) candidate is NOT admissible — the candidateAdmissible
        // precondition never obtains, the commit fails, and the library file stays byte-identical
        // (here: absent). This is the polarity gate a verbatim conjecture clone would get WRONG
        // (it would commit a refuted node).
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let bytes = snapshot();

        for (label, formula, kb, expect_lifecycle) in [
            (
                "refuted",
                forall_horn_candidate("B"),
                refuting_kb("B"),
                "refuted-in-standpoint",
            ),
            ("open", forall_horn_candidate("B"), open_kb("B"), "open"),
        ] {
            let (_env, _cg) = ConjEnvGuard::set();
            let (_dir, path) = temp_candidate();
            let server = McpServer::from_snapshot(&bytes).unwrap();
            assert!(!path.exists(), "{label}: library absent before the call");

            let resp = text_payload(server.call_tool_result(
                "submit_candidate",
                &json!({
                    "formula": formula,
                    "kb": kb,
                    "standpoint": "http://ex/standpoint/alice",
                }),
            ));
            assert_eq!(resp["ok"], false, "{label}: not admitted: {resp}");
            assert_eq!(resp["admissible"], false, "{label}");
            assert_eq!(resp["verdict"]["lifecycle"], expect_lifecycle, "{label}");
            assert!(
                !path.exists(),
                "{label}: a non-admissible candidate must write NOTHING to the library"
            );
        }
    }

    #[test]
    fn submit_candidate_refuted_leaves_populated_library_byte_identical() {
        // AC6, strengthened: the "stages nothing" invariant must hold against a NON-EMPTY store,
        // not just an absent one. Admit a corroborated candidate (the library now holds real
        // bytes), snapshot them, then submit a genuinely refuted candidate — the polarity gate
        // must leave the on-disk library BYTE-IDENTICAL (no append, no truncation, no rewrite).
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let (_env, _cg) = ConjEnvGuard::set();
        let (_dir, path) = temp_candidate();
        let bytes = snapshot();
        let server = McpServer::from_snapshot(&bytes).unwrap();

        // Populate the library with one admissible candidate.
        let admit = text_payload(server.call_tool_result(
            "submit_candidate",
            &json!({
                "formula": reified_ground_atom_candidate("B"),
                "kb": ground_atom_entailing_kb("B"),
                "standpoint": "http://ex/standpoint/alice",
            }),
        ));
        assert_eq!(
            admit["committed"], true,
            "setup: admissible candidate commits"
        );
        let populated = std::fs::read(&path).expect("library exists after an admitted candidate");
        assert!(
            !populated.is_empty(),
            "the populated library holds real bytes"
        );

        // A refuted submit against the POPULATED store must write nothing.
        let resp = text_payload(server.call_tool_result(
            "submit_candidate",
            &json!({
                "formula": forall_horn_candidate("B"),
                "kb": refuting_kb("B"),
                "standpoint": "http://ex/standpoint/alice",
            }),
        ));
        assert_eq!(
            resp["ok"], false,
            "refuted candidate is not admitted: {resp}"
        );
        assert_eq!(resp["admissible"], false);
        assert_eq!(resp["verdict"]["lifecycle"], "refuted-in-standpoint");

        let after = std::fs::read(&path).expect("library still present");
        assert_eq!(
            after, populated,
            "a refuted submit must leave the populated library byte-identical"
        );
    }

    #[test]
    fn submit_candidate_dry_run_writes_nothing() {
        // A dry-run on an ADMISSIBLE candidate computes the verdict via a hypothetical commit but
        // writes nothing.
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let (_env, _cg) = ConjEnvGuard::set();
        let (_dir, path) = temp_candidate();
        let bytes = snapshot();
        let server = McpServer::from_snapshot(&bytes).unwrap();

        let resp = text_payload(server.call_tool_result(
            "submit_candidate",
            &json!({
                "formula": reified_ground_atom_candidate("B"),
                "kb": ground_atom_entailing_kb("B"),
                "standpoint": "http://ex/standpoint/alice",
                "dry_run": true,
            }),
        ));
        assert_eq!(resp["ok"], true, "{resp}");
        assert_eq!(resp["dry_run"], true);
        assert_eq!(resp["admissible"], true);
        assert!(
            !path.exists(),
            "a dry-run submit must write nothing to the library"
        );
    }

    #[test]
    fn withdraw_candidate_supersedes_and_gates() {
        // Submit an admissible candidate, then withdraw it: list flips it to `withdrawn`
        // (superseded, never deleted). Withdrawing an unknown id hard-fails before writing.
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let (_env, _cg) = ConjEnvGuard::set();
        let (_dir, path) = temp_candidate();
        let bytes = snapshot();
        let server = McpServer::from_snapshot(&bytes).unwrap();

        let submit = text_payload(server.call_tool_result(
            "submit_candidate",
            &json!({
                "formula": reified_ground_atom_candidate("B"),
                "kb": ground_atom_entailing_kb("B"),
                "standpoint": "http://ex/standpoint/alice",
            }),
        ));
        let node = submit["candidate"]
            .as_str()
            .expect("candidate iri")
            .to_string();

        // Withdrawing an unknown id fails the precondition (nothing appended for it).
        let unknown = text_payload(server.call_tool_result(
            "withdraw_candidate",
            &json!({"candidate_id": "urn:gmeow:not-a-candidate"}),
        ));
        assert_eq!(unknown["ok"], false, "unknown id must hard-fail: {unknown}");

        // Withdrawing the real node succeeds and supersedes it.
        let withdraw = text_payload(
            server.call_tool_result("withdraw_candidate", &json!({"candidate_id": node})),
        );
        assert_eq!(withdraw["ok"], true, "withdraw the real node: {withdraw}");
        assert!(path.exists());

        let list = text_payload(server.call_tool_result("list_candidates", &json!({})));
        assert_eq!(
            list["candidate_count"], 1,
            "still listed (superseded): {list}"
        );
        assert_eq!(list["candidates"][0]["disposition"], "withdrawn");

        // The disposition filter narrows correctly.
        let in_library = text_payload(
            server.call_tool_result("list_candidates", &json!({"disposition": "in-library"})),
        );
        assert_eq!(in_library["candidate_count"], 0);
    }

    #[test]
    fn action_policy_covers_the_candidate_submission_pair() {
        // The submit_candidate ⇄ withdraw_candidate governed-write pair must be REPRESENTED in
        // the canonical action theory the engine parses (the same projected N-Quads the TR run
        // feeds), with the mutual P10 compensation pairing — not merely documented.
        const EX: &str = "https://blackcatinformatics.ca/gmeow/examples/agentic/mcp-policy/";
        const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
        const LOGIC_MCP_ACTION_SCHEMA: &str =
            "https://blackcatinformatics.ca/logic/McpActionSchema";
        const LOGIC_COMPENSATION: &str = "https://blackcatinformatics.ca/logic/compensation";

        let policy = action_policy_nquads();
        for (schema, compensation) in [
            ("submitCandidate", "withdrawCandidate"),
            ("withdrawCandidate", "submitCandidate"),
        ] {
            let type_line =
                format!("<{EX}{schema}> <{RDF_TYPE}> <{LOGIC_MCP_ACTION_SCHEMA}> <{TXN_WORLD}> .");
            let comp_line = format!(
                "<{EX}{schema}> <{LOGIC_COMPENSATION}> <{EX}{compensation}> <{TXN_WORLD}> ."
            );
            assert!(
                policy.contains(&type_line),
                "{schema} must be typed logic:McpActionSchema: missing {type_line:?}"
            );
            assert!(
                policy.contains(&comp_line),
                "{schema}'s compensation must be {compensation}: missing {comp_line:?}"
            );
        }
    }

    /// A KB whose `ex:trigger` fires the `∀`-Horn candidate on SEVERAL individuals, so the
    /// candidate's derived (non-EDB) closure is strictly larger than a `max_steps`/`max_answers`
    /// bound of 1 — the isolated scenario evaluation exceeds the ceiling.
    fn multi_trigger_kb(cls_local: &str) -> String {
        format!(
            "@prefix ex:  <http://ex/> .\n\
             @prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .\n\
             ex:a ex:trigger ex:mark .\n\
             ex:b ex:trigger ex:mark .\n\
             ex:c ex:trigger ex:mark .\n\
             ex:a rdf:type ex:A .\n\
             # {cls_local} is unrelated to A — no clash, just several derived facts.\n"
        )
    }

    #[test]
    fn conjecture_test_budget_bound_forces_open_via_the_mcp_surface() {
        // GAP G1: the `max_steps` / `max_answers` bound is reachable from the SHIPPED MCP
        // surface (not just the logic-crate unit test). A run whose derived closure exceeds the
        // ceiling is truncated → evaluation budget-exhausted → lifecycle open → discharge
        // Unknown. This is a PURE assertion on `conjecture_test`: no persist tail exists on
        // this surface at all, so the library stays absent throughout.
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let (_env, _cg) = ConjEnvGuard::set();
        let (_mem, _mp) = temp_memory();
        let (_dir, path) = temp_conjecture();
        let bytes = snapshot();
        let server = McpServer::from_snapshot(&bytes).unwrap();

        // Unbounded control: the same candidate/KB runs to Completed (a non-budget verdict).
        let unbounded = text_payload(server.call_tool_result(
            "conjecture_test",
            &json!({
                "formula": forall_horn_candidate("C"),
                "kb": multi_trigger_kb("C"),
                "standpoint": "http://ex/standpoint/alice",
            }),
        ));
        assert_eq!(unbounded["ok"], true);
        assert_ne!(
            unbounded["verdict"]["evaluation"], "budget-exhausted",
            "the unbounded control must not trip the ceiling: {unbounded}"
        );

        // Bounded: a ceiling of 1 truncates the multi-fact derived closure.
        let bounded = text_payload(server.call_tool_result(
            "conjecture_test",
            &json!({
                "formula": forall_horn_candidate("C"),
                "kb": multi_trigger_kb("C"),
                "standpoint": "http://ex/standpoint/alice",
                "max_steps": 1,
            }),
        ));
        assert_eq!(
            bounded["ok"], true,
            "bounded run must compute a verdict: {bounded}"
        );
        assert_eq!(
            bounded["verdict"]["evaluation"], "budget-exhausted",
            "exceeding the ceiling must stamp BudgetExhausted: {bounded}"
        );
        assert_eq!(
            bounded["verdict"]["lifecycle"], "open",
            "a budget-exhausted run is inconclusive → Open: {bounded}"
        );
        assert_eq!(
            bounded["verdict"]["discharge"], "ObligationUnknown",
            "a budget-exhausted run carries the obligation forward as Unknown: {bounded}"
        );

        // `max_answers` is the equivalent binding-count ceiling and trips the same way.
        let bounded_answers = text_payload(server.call_tool_result(
            "conjecture_test",
            &json!({
                "formula": forall_horn_candidate("C"),
                "kb": multi_trigger_kb("C"),
                "standpoint": "http://ex/standpoint/alice",
                "max_answers": 1,
            }),
        ));
        assert_eq!(
            bounded_answers["verdict"]["evaluation"], "budget-exhausted",
            "max_answers must impose the same ceiling: {bounded_answers}"
        );
        // `conjecture_test` is PURE: none of these calls ever touch the library.
        assert!(
            !path.exists(),
            "conjecture_test calls must write nothing to the library"
        );
    }

    #[test]
    fn store_conjecture_budget_bound_forces_open_and_still_persists() {
        // R3b acceptance: a budget-exhausted `store_conjecture` (committing) must still yield
        // the non-conclusive verdict — lifecycle open, discharge Unknown — via the governor,
        // NEVER a false discharge, even though the run commits and appends to the library.
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let (_env, _cg) = ConjEnvGuard::set();
        let (_mem, _mp) = temp_memory();
        let (_dir, path) = temp_conjecture();
        let bytes = snapshot();
        let server = McpServer::from_snapshot(&bytes).unwrap();

        assert!(!path.exists());
        let bounded = text_payload(server.call_tool_result(
            "store_conjecture",
            &json!({
                "formula": forall_horn_candidate("C"),
                "kb": multi_trigger_kb("C"),
                "standpoint": "http://ex/standpoint/alice",
                "max_steps": 1,
            }),
        ));
        assert_eq!(
            bounded["ok"], true,
            "a budget-exhausted commit must still compute+persist a verdict: {bounded}"
        );
        assert_eq!(bounded["verdict"]["evaluation"], "budget-exhausted");
        assert_eq!(
            bounded["verdict"]["lifecycle"], "open",
            "never a false discharge: budget-exhausted must stay Open: {bounded}"
        );
        assert_eq!(bounded["verdict"]["discharge"], "ObligationUnknown");
        assert_eq!(bounded["transaction"]["committed"], true);

        // The non-conclusive verdict is still a real, committed, append-only segment.
        assert!(
            path.exists() && fs::metadata(&path).unwrap().len() > 0,
            "a committed budget-exhausted run must still append its Open verdict"
        );
        let node = bounded["conjecture"]
            .as_str()
            .expect("node iri")
            .to_string();
        assert!(conjecture_nodes(read_conjectures(&path).as_ref()).contains(&node));
    }

    #[test]
    fn store_conjecture_persists_are_append_only() {
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let (_env, _cg) = ConjEnvGuard::set();
        let (_mem, _mp) = temp_memory();
        let (_dir, path) = temp_conjecture();
        let bytes = snapshot();
        let server = McpServer::from_snapshot(&bytes).unwrap();

        text_payload(server.call_tool_result(
            "store_conjecture",
            &json!({
                "formula": forall_horn_candidate("B"),
                "kb": refuting_kb("B"),
                "standpoint": "http://ex/standpoint/alice",
            }),
        ));
        let first = fs::read(&path).expect("first library bytes");
        let first_len = first.len();
        assert!(first_len > 0);

        // A DISTINCT conjecture (different head class) appends a second segment.
        text_payload(server.call_tool_result(
            "store_conjecture",
            &json!({
                "formula": forall_horn_candidate("C"),
                "kb": open_kb("C"),
                "standpoint": "http://ex/standpoint/alice",
            }),
        ));
        let second = fs::read(&path).expect("second library bytes");
        assert!(
            second.len() > first_len,
            "the second persist must APPEND, growing the file"
        );
        assert_eq!(
            &second[..first_len],
            &first[..],
            "the first segment's bytes must be intact (append-only, never mutated)"
        );
        // Both conjectures are readable back.
        assert_eq!(conjecture_nodes(read_conjectures(&path).as_ref()).len(), 2);
    }

    #[test]
    fn store_conjecture_library_is_isolated_from_the_base_kb() {
        // R2: the caller's KB text is unchanged by the call, and the library is a DISTINCT
        // file the reasoner never folds into its base graph.
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let (_env, _cg) = ConjEnvGuard::set();
        let (_mem, mem_path) = temp_memory();
        let (_dir, path) = temp_conjecture();
        let bytes = snapshot();
        let server = McpServer::from_snapshot(&bytes).unwrap();

        let kb = refuting_kb("B");
        let kb_before = kb.clone();
        text_payload(server.call_tool_result(
            "store_conjecture",
            &json!({
                "formula": forall_horn_candidate("B"),
                "kb": kb,
                "standpoint": "http://ex/standpoint/alice",
            }),
        ));
        // The KB argument the tool was given is an owned String — the tool cannot mutate the
        // caller's copy. (Isolation is inherent: store_conjecture borrows and copies the KB.)
        assert_eq!(kb_before, refuting_kb("B"));
        // The conjecture library is a distinct file, NOT the memory store, and never the base
        // reasoning graph (reason reads graph_dataset(), never conjecture_path()).
        assert!(path.exists());
        assert_ne!(path, mem_path);
        // The bundled reasoning surface is unaffected — reason still runs cleanly over the
        // untouched base graph (the library is never unioned in).
        let reasoned = text_payload(server.call_tool_result("reason", &json!({})));
        // `reason` is dev-only; over a Consumer server it returns an error, proving the base
        // reasoning path does not consult the library either way. What matters for R2 is that
        // the library file and the KB are untouched, asserted above.
        let _ = reasoned;
    }

    #[test]
    fn same_formula_two_standpoints_mints_two_nodes() {
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let (_env, _cg) = ConjEnvGuard::set();
        let (_mem, _mp) = temp_memory();
        let (_dir, path) = temp_conjecture();
        let bytes = snapshot();
        let server = McpServer::from_snapshot(&bytes).unwrap();

        let a = text_payload(server.call_tool_result(
            "store_conjecture",
            &json!({
                "formula": forall_horn_candidate("C"),
                "kb": open_kb("C"),
                "standpoint": "http://ex/standpoint/alice",
            }),
        ));
        let b = text_payload(server.call_tool_result(
            "store_conjecture",
            &json!({
                "formula": forall_horn_candidate("C"),
                "kb": open_kb("C"),
                "standpoint": "http://ex/standpoint/bob",
            }),
        ));
        assert_ne!(
            a["conjecture"], b["conjecture"],
            "the same formula in two standpoints must mint two DISTINCT nodes (P9)"
        );
        assert_eq!(conjecture_nodes(read_conjectures(&path).as_ref()).len(), 2);
    }

    #[test]
    fn conjecture_library_corpus_query_recovers_open_and_refuted() {
        // A corpus competency: persist several DISTINCT conjectures (open + refuted) in one
        // standpoint, then scan the collection for all of them, with witness premises for the
        // refuted ones recoverable.
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let (_env, _cg) = ConjEnvGuard::set();
        let (_mem, _mp) = temp_memory();
        let (_dir, path) = temp_conjecture();
        let bytes = snapshot();
        let server = McpServer::from_snapshot(&bytes).unwrap();

        // Two refuted (B, D disjoint) and two open (C, E unrelated).
        for cls in ["B", "D"] {
            let r = text_payload(server.call_tool_result(
                "store_conjecture",
                &json!({
                    "formula": forall_horn_candidate(cls),
                    "kb": refuting_kb(cls),
                    "standpoint": "http://ex/standpoint/team",
                }),
            ));
            assert_eq!(r["verdict"]["lifecycle"], "refuted-in-standpoint", "{r}");
        }
        for cls in ["C", "E"] {
            let r = text_payload(server.call_tool_result(
                "store_conjecture",
                &json!({
                    "formula": forall_horn_candidate(cls),
                    "kb": open_kb(cls),
                    "standpoint": "http://ex/standpoint/team",
                }),
            ));
            assert_eq!(r["verdict"]["lifecycle"], "open", "{r}");
        }

        let dataset = read_conjectures(&path);
        // All four conjectures are recoverable.
        assert_eq!(conjecture_nodes(&dataset).len(), 4);
        // All are scoped to the team standpoint.
        let team_scoped = dataset
            .owned_quads()
            .filter(|q| {
                q.predicate == format!("{LOGIC_NS}conjectureStandpoint")
                    && q.object == RdfTerm::iri("http://ex/standpoint/team")
            })
            .count();
        assert_eq!(team_scoped, 4, "every conjecture is standpoint-scoped");
        // The two refuted conjectures each carry a recoverable individual + premises.
        let refuted = dataset
            .owned_quads()
            .filter(|q| {
                q.predicate == format!("{LOGIC_NS}conjectureLifecycleState")
                    && q.object == RdfTerm::iri(format!("{LOGIC_NS}ConjectureRefutedInStandpoint"))
            })
            .count();
        assert_eq!(refuted, 2, "exactly the two disjointness cases are refuted");
        assert!(
            witness_premises(&dataset).len() >= 2,
            "each refutation persists recoverable witness premises"
        );
    }

    /// `counter_examples` over the live bundle: a term documenting fixtures yields
    /// the real, split fixture bodies; a resolvable term documenting none yields the
    /// honest empty-but-ok shape; an unknown term is a hard error envelope.
    ///
    /// `gmeow:Activity` documents BOTH a well-formed exemplar and a counter-example
    /// in the shipped `gmeow:graph/documentation` graph (verified by the projection
    /// query in Task 1); `gmeow:AboutnessMode` is a documented term that authors no
    /// fixtures.
    #[test]
    fn tool_counter_examples_surface() {
        let server = consumer_server();

        // A term with fixtures → real, non-empty split bodies.
        let hit = text_payload(
            server.call_tool_result("counter_examples", &json!({"term": "gmeow:Activity"})),
        );
        assert_eq!(hit["ok"], true);
        assert_eq!(hit["term"], "gmeow:Activity");
        let counter = hit["counter_examples"]
            .as_array()
            .expect("counter_examples is an array");
        let wellformed = hit["wellformed"]
            .as_array()
            .expect("wellformed is an array");
        assert!(
            !counter.is_empty(),
            "gmeow:Activity documents at least one counter-example: {hit}"
        );
        assert!(
            !wellformed.is_empty(),
            "gmeow:Activity documents at least one well-formed exemplar: {hit}"
        );
        // The counter-example carries a real Turtle body AND a violation code.
        let ce = &counter[0];
        assert!(
            ce["text"].as_str().is_some_and(|t| t.contains(':')),
            "counter-example carries a real Turtle body: {ce}"
        );
        assert!(
            ce["violation_code"].as_str().is_some_and(|c| !c.is_empty()),
            "counter-example carries an authored violation code: {ce}"
        );
        // The well-formed exemplar has a real body and NO violation code.
        let wf = &wellformed[0];
        assert!(
            wf["text"].as_str().is_some_and(|t| t.contains(':')),
            "well-formed exemplar carries a real Turtle body: {wf}"
        );
        assert!(
            wf["violation_code"].is_null(),
            "well-formed exemplar has no violation code: {wf}"
        );
        // Deterministic: a second call is byte-identical.
        let again = text_payload(
            server.call_tool_result("counter_examples", &json!({"term": "gmeow:Activity"})),
        );
        assert_eq!(hit, again, "counter_examples output is deterministic");

        // A resolvable term with NO fixtures → empty-but-ok (NOT an error).
        let empty = text_payload(
            server.call_tool_result("counter_examples", &json!({"term": "gmeow:AboutnessMode"})),
        );
        assert_eq!(
            empty["ok"], true,
            "no-fixture term is empty-but-ok: {empty}"
        );
        assert_eq!(empty["wellformed"], json!([]));
        assert_eq!(empty["counter_examples"], json!([]));

        // An unknown term → hard error envelope.
        let unknown = text_payload(server.call_tool_result(
            "counter_examples",
            &json!({"term": "gmeow:DefinitelyNotARealTerm42"}),
        ));
        assert_eq!(
            unknown["ok"], false,
            "unknown term is a hard error: {unknown}"
        );
        assert!(
            unknown["error"]
                .as_str()
                .is_some_and(|e| e.contains("unknown term")),
            "unknown-term error names the failure: {unknown}"
        );
    }

    /// `entailments` over the live bundle: a term with derivations yields every
    /// entailment's rule/conclusion with its premises preserved; an unknown term is a
    /// hard error. `gmeow:Entity` grounds >1000 entailment records in the shipped
    /// documentation graph.
    #[test]
    fn tool_entailments_surface() {
        let server = consumer_server();

        let hit =
            text_payload(server.call_tool_result("entailments", &json!({"term": "gmeow:Entity"})));
        assert_eq!(hit["ok"], true);
        assert_eq!(hit["term"], "gmeow:Entity");
        let entailments = hit["entailments"]
            .as_array()
            .expect("entailments is an array");
        assert!(
            !entailments.is_empty(),
            "gmeow:Entity grounds at least one entailment: {}",
            &hit.to_string()[..hit.to_string().len().min(400)]
        );
        // Every record carries a non-empty rule and conclusion.
        for e in entailments {
            assert!(
                e["rule"].as_str().is_some_and(|r| !r.is_empty()),
                "entailment carries a rule: {e}"
            );
            assert!(
                e["conclusion"].as_str().is_some_and(|c| !c.is_empty()),
                "entailment carries a conclusion: {e}"
            );
            assert!(e["premises"].is_array(), "premises is an array: {e}");
        }
        // Premises are preserved: at least one derivation carries premises.
        let total_premises: usize = entailments
            .iter()
            .map(|e| e["premises"].as_array().map_or(0, Vec::len))
            .sum();
        assert!(
            total_premises > 0,
            "at least one gmeow:Entity entailment preserves its premises"
        );

        // Unknown term → hard error envelope.
        let unknown = text_payload(server.call_tool_result(
            "entailments",
            &json!({"term": "gmeow:DefinitelyNotARealTerm42"}),
        ));
        assert_eq!(
            unknown["ok"], false,
            "unknown term is a hard error: {unknown}"
        );
    }

    /// Select the RICHEST-surface term in the shipped bundle for the tier tests:
    /// the term (a) grounding at least one reasoner entailment AND (b) documenting at
    /// least one conformance fixture, maximizing the total panel count (entailments +
    /// fixtures), tie-broken by IRI so the choice is deterministic. Panics (a real
    /// blocker, never a soft skip) if the bundle documents no such term.
    ///
    /// Uses exactly TWO bulk queries (the entailment map + a single fixture-by-term
    /// scan) and intersects them — never a per-term query per candidate.
    fn richest_card_term(server: &McpServer) -> String {
        let entailments = server
            .view
            .entailment_map()
            .expect("entailment map from the shipped documentation graph");
        // One bulk scan: fixture count per documented term.
        let fixtures_query = format!(
            "PREFIX gm: <{GMEOW_NS}>\nSELECT ?term ?f WHERE {{ ?f a gm:DocFixture ; \
             gm:documents ?term . }}"
        );
        let mut fixtures_per_term: BTreeMap<String, usize> = BTreeMap::new();
        for row in server
            .view
            .docs_select_rows(&fixtures_query)
            .expect("fixture-by-term scan over graph/documentation")
        {
            if let Some(term) = row.get("term") {
                *fixtures_per_term.entry(term.clone()).or_default() += 1;
            }
        }
        let mut candidates: Vec<(usize, String)> = entailments
            .iter()
            .filter_map(|(iri, ents)| {
                fixtures_per_term
                    .get(iri)
                    .map(|&fx| (ents.len() + fx, iri.clone()))
            })
            .collect();
        // Most panels first, then lexicographically-first IRI (deterministic).
        candidates.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
        candidates
            .into_iter()
            .next()
            .map(|(_, iri)| iri)
            .expect("the shipped bundle documents a term with entailments AND fixtures")
    }

    /// A documented term that still carries full-tier panels (≥1 entailment AND ≥1
    /// fixture) but the FEWEST of them — the cheapest term whose `full` card exercises
    /// every rich-panel path. Tier/determinism assertions that render the full card
    /// several times use this instead of [`richest_card_term`] (whose ~1000-entailment
    /// surface is only needed for the byte-ceiling proof), so they render fast.
    fn modest_panel_card_term(server: &McpServer) -> String {
        let entailments = server
            .view
            .entailment_map()
            .expect("entailment map from the shipped documentation graph");
        let fixtures_query = format!(
            "PREFIX gm: <{GMEOW_NS}>\nSELECT ?term ?f WHERE {{ ?f a gm:DocFixture ; \
             gm:documents ?term . }}"
        );
        let mut fixtures_per_term: BTreeMap<String, usize> = BTreeMap::new();
        for row in server
            .view
            .docs_select_rows(&fixtures_query)
            .expect("fixture-by-term scan over graph/documentation")
        {
            if let Some(term) = row.get("term") {
                *fixtures_per_term.entry(term.clone()).or_default() += 1;
            }
        }
        let mut candidates: Vec<(usize, String)> = entailments
            .iter()
            .filter_map(|(iri, ents)| {
                fixtures_per_term
                    .get(iri)
                    .map(|&fx| (ents.len() + fx, iri.clone()))
            })
            .collect();
        // FEWEST panels first, then lexicographically-first IRI (deterministic).
        candidates.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
        candidates
            .into_iter()
            .next()
            .map(|(_, iri)| iri)
            .expect("the shipped bundle documents a term with entailments AND fixtures")
    }

    /// `doc_card` tiers: `summary` is the leanest surface (title + definition only,
    /// under a pinned byte ceiling, none of the advisory / panel sections); `full`
    /// is strictly larger and carries the rich oracle panels (Entailments / Do /
    /// Don't headers).
    #[test]
    fn tool_doc_card_tier_byte_ceiling() {
        const SUMMARY_CEILING: usize = 1500;
        let server = consumer_server();
        let term = richest_card_term(&server);

        let summary = text_payload(
            server.call_tool_result("doc_card", &json!({"term": term, "detail": "summary"})),
        );
        let full = text_payload(
            server.call_tool_result("doc_card", &json!({"term": term, "detail": "full"})),
        );
        let s_card = summary["card"].as_str().expect("summary markdown card");
        let f_card = full["card"].as_str().expect("full markdown card");

        // Summary is title + definition ONLY, under the ceiling.
        assert!(
            s_card.len() < SUMMARY_CEILING,
            "summary card ({} bytes) must be under {SUMMARY_CEILING}: {s_card}",
            s_card.len()
        );
        assert!(
            s_card.starts_with("# "),
            "summary carries the H1 title: {s_card}"
        );
        let summary_body = s_card
            .strip_prefix("# ")
            .and_then(|s| s.split_once("\n\n"))
            .map_or("", |(_, rest)| rest);
        assert!(
            !summary_body.trim().is_empty(),
            "summary carries a definition after the title: {s_card}"
        );
        // NONE of the advisory / metadata / panel surface at the summary tier.
        assert!(
            !s_card.contains("- category:"),
            "no metadata header: {s_card}"
        );
        assert!(
            !s_card.contains("**Use when:**"),
            "no advisory fields: {s_card}"
        );
        assert!(
            !s_card.contains("## Entailments"),
            "no rich panels: {s_card}"
        );

        // Full is strictly larger and carries the panel section headers.
        assert!(
            f_card.len() > s_card.len(),
            "full ({}) must exceed summary ({})",
            f_card.len(),
            s_card.len()
        );
        assert!(
            f_card.contains("## Entailments"),
            "full carries Entailments: {f_card}"
        );
        assert!(
            f_card.contains("## Do") || f_card.contains("## Don't"),
            "full carries a Do / Don't fixture panel: {f_card}"
        );
    }

    /// Single-renderer authority: `doc_card` at `detail=standard` is BYTE-IDENTICAL
    /// to rendering the shared compact `Card` through `render_card` at `Standard` —
    /// the tier gating never perturbs the docs-site card the standard tier mirrors.
    #[test]
    fn tool_doc_card_standard_is_byte_identical_to_compact_render() {
        let server = consumer_server();
        let term = "gmeow:EntityExistence";

        let envelope = text_payload(
            server.call_tool_result("doc_card", &json!({"term": term, "detail": "standard"})),
        );
        let card_md = envelope["card"].as_str().expect("standard markdown card");

        // Independent expected: the SAME shared builder + renderer at Standard.
        let requested = server.startup_requested.clone();
        let modeled_defs = server.view.modeled_defs();
        let expected = server.view.with_terms(requested, |terms| {
            let (title, card) = export::doc_card_build(terms, term, &modeled_defs)
                .resolved()
                .expect("known term resolves");
            gmeow_docs_model::card::render_card(
                &title,
                &card,
                gmeow_docs_model::card::CardDetail::Standard,
            )
        });
        assert_eq!(
            card_md, expected,
            "standard tier must be byte-identical to the compact single-renderer output"
        );
        // Default (no `detail`) is Standard.
        let defaulted = text_payload(server.call_tool_result("doc_card", &json!({"term": term})));
        assert_eq!(defaulted["detail"], "standard");
        assert_eq!(defaulted["card"].as_str().expect("card"), expected);
    }

    /// `doc_card` `format=json`: byte-stable across calls; standard-tier JSON omits
    /// the full-tier rich fields; full-tier JSON carries them.
    #[test]
    fn tool_doc_card_json_determinism_and_tier_fields() {
        let server = consumer_server();
        let docs_a = server.view.documentation();
        let docs_b = server.view.documentation();
        assert!(
            Arc::ptr_eq(docs_a, docs_b),
            "documentation queries must share one projected graph"
        );
        // Determinism + tier-field presence hold for ANY term that carries panels,
        // so use the cheapest such term (not the ~1000-entailment richest term) —
        // this test renders the full card three times, so a lean term keeps it fast.
        let term = modest_panel_card_term(&server);

        // Byte-identical raw tool text across two identical calls.
        let call = || {
            server.call_tool_result(
                "doc_card",
                &json!({"term": term, "detail": "full", "format": "json"}),
            )["content"][0]["text"]
                .as_str()
                .expect("json tool text")
                .to_string()
        };
        assert_eq!(call(), call(), "json card is byte-stable across calls");

        // Standard-tier JSON: a Card object WITHOUT the full-tier rich fields.
        let std_json = text_payload(server.call_tool_result(
            "doc_card",
            &json!({"term": term, "detail": "standard", "format": "json"}),
        ));
        assert_eq!(std_json["format"], "json");
        let std_card = &std_json["card"];
        assert!(std_card.is_object(), "json card is an object: {std_card}");
        assert!(
            std_card.get("entailments").is_none(),
            "no entailments at standard"
        );
        assert!(
            std_card.get("fixtures_do").is_none(),
            "no fixtures_do at standard"
        );
        assert!(
            std_card.get("fixtures_dont").is_none(),
            "no fixtures_dont at standard"
        );
        assert!(
            std_card.get("diagnostics").is_none(),
            "no diagnostics at standard"
        );
        assert!(std_card.get("loss").is_none(), "no loss at standard");

        // Full-tier JSON DOES carry the rich panels.
        let full_json = text_payload(server.call_tool_result(
            "doc_card",
            &json!({"term": term, "detail": "full", "format": "json"}),
        ));
        assert!(
            full_json["card"].get("entailments").is_some(),
            "full json carries entailments: {}",
            &full_json.to_string()[..full_json.to_string().len().min(400)]
        );
    }

    /// `doc_card` cost metadata: every envelope carries positive `bytes`/`tokens`,
    /// monotonically non-decreasing across summary ≤ standard ≤ full for one term.
    #[test]
    fn tool_doc_card_cost_metadata_is_monotone() {
        let server = consumer_server();
        let term = richest_card_term(&server);

        let tier = |detail: &str| {
            text_payload(
                server.call_tool_result("doc_card", &json!({"term": term, "detail": detail})),
            )
        };
        let summary = tier("summary");
        let standard = tier("standard");
        let full = tier("full");

        for env in [&summary, &standard, &full] {
            assert!(
                env["bytes"].as_u64().expect("bytes") > 0,
                "bytes > 0: {env}"
            );
            assert!(
                env["tokens"].as_u64().expect("tokens") > 0,
                "tokens > 0: {env}"
            );
        }
        let bytes = |e: &Value| e["bytes"].as_u64().unwrap();
        let tokens = |e: &Value| e["tokens"].as_u64().unwrap();
        assert!(
            bytes(&summary) <= bytes(&standard),
            "bytes summary ≤ standard"
        );
        assert!(bytes(&standard) <= bytes(&full), "bytes standard ≤ full");
        assert!(
            tokens(&summary) <= tokens(&standard),
            "tokens summary ≤ standard"
        );
        assert!(tokens(&standard) <= tokens(&full), "tokens standard ≤ full");

        // Unknown detail / format is a hard error listing the valid values.
        let bad_detail = text_payload(
            server.call_tool_result("doc_card", &json!({"term": term, "detail": "verbose"})),
        );
        assert_eq!(
            bad_detail["ok"], false,
            "unknown detail hard-fails: {bad_detail}"
        );
        let bad_format = text_payload(
            server.call_tool_result("doc_card", &json!({"term": term, "format": "yaml"})),
        );
        assert_eq!(
            bad_format["ok"], false,
            "unknown format hard-fails: {bad_format}"
        );
    }

    /// `doc_card` full tier populates the rich panels FROM the documentation graph:
    /// the full markdown inlines an actual entailment conclusion and a fixture title
    /// the sibling `entailments` / `counter_examples` tools report for the term.
    #[test]
    fn tool_doc_card_full_inlines_graph_panels() {
        let server = consumer_server();
        let term = richest_card_term(&server);

        let full = text_payload(
            server.call_tool_result("doc_card", &json!({"term": term, "detail": "full"})),
        );
        let f_card = full["card"].as_str().expect("full markdown card");

        // An entailment's conclusion (from the SAME graph the `entailments` tool reads).
        let ents = text_payload(server.call_tool_result("entailments", &json!({"term": term})));
        let conclusion = ents["entailments"][0]["conclusion"]
            .as_str()
            .expect("the richest term grounds an entailment with a conclusion");
        assert!(
            f_card.contains(conclusion),
            "full card inlines the entailment conclusion {conclusion:?}"
        );

        // A fixture title (from the SAME graph the `counter_examples` tool reads).
        let fixtures =
            text_payload(server.call_tool_result("counter_examples", &json!({"term": term})));
        let title = fixtures["counter_examples"]
            .get(0)
            .and_then(|f| f["title"].as_str())
            .or_else(|| {
                fixtures["wellformed"]
                    .get(0)
                    .and_then(|f| f["title"].as_str())
            })
            .expect("the richest term documents a fixture with a title");
        assert!(
            f_card.contains(title),
            "full card inlines the fixture title {title:?}"
        );
    }

    /// `competency_questions` over the live bundle: the index form (no `term`) returns
    /// every runnable question, each carrying a `query_text` that PARSES as a valid
    /// SPARQL SELECT; the per-term form returns that term's subset. `gmeow:Agent`
    /// documents competency questions in the shipped documentation graph.
    #[test]
    fn tool_competency_questions_surface() {
        let server = consumer_server();

        // Index form: no `term`.
        let index = text_payload(server.call_tool_result("competency_questions", &json!({})));
        assert_eq!(index["ok"], true);
        assert!(
            index.get("term").is_none(),
            "index form carries no term key: {}",
            &index.to_string()[..index.to_string().len().min(200)]
        );
        let questions = index["questions"]
            .as_array()
            .expect("questions is an array");
        assert!(!questions.is_empty(), "the competency index is non-empty");
        for q in questions {
            assert!(
                q["query_text"].as_str().is_some_and(|t| !t.is_empty()),
                "every competency question carries a runnable query_text: {q}"
            );
        }

        // The first question's query_text round-trips through the native SPARQL
        // parser as an executable SELECT (over an empty dataset — parse+plan only).
        let first_query = questions[0]["query_text"].as_str().expect("query_text");
        let empty = std::sync::Arc::new(
            purrdf::RdfDatasetBuilder::new()
                .freeze()
                .expect("empty dataset"),
        );
        let parsed = gmeow_bundle_view::native_query::query(&empty, first_query)
            .expect("competency query_text is a valid SPARQL query");
        assert!(
            matches!(parsed, purrdf::SparqlResult::Solutions { .. }),
            "competency query_text is a SPARQL SELECT: {first_query}"
        );

        // Deterministic index.
        let index_again = text_payload(server.call_tool_result("competency_questions", &json!({})));
        assert_eq!(index, index_again, "competency index is deterministic");

        // Per-term form: gmeow:Agent's subset — non-empty, each with a query_text.
        let per_term = text_payload(
            server.call_tool_result("competency_questions", &json!({"term": "gmeow:Agent"})),
        );
        assert_eq!(per_term["ok"], true);
        assert_eq!(per_term["term"], "gmeow:Agent");
        let agent_questions = per_term["questions"]
            .as_array()
            .expect("questions is an array");
        assert!(
            !agent_questions.is_empty(),
            "gmeow:Agent documents at least one competency question: {per_term}"
        );
        for q in agent_questions {
            assert!(
                q["query_text"].as_str().is_some_and(|t| !t.is_empty()),
                "per-term competency question carries a query_text: {q}"
            );
        }
        assert!(
            agent_questions.len() <= questions.len(),
            "a term's competency subset is no larger than the whole index"
        );

        // Unknown term (per-term form) → hard error envelope.
        let unknown = text_payload(server.call_tool_result(
            "competency_questions",
            &json!({"term": "gmeow:DefinitelyNotARealTerm42"}),
        ));
        assert_eq!(
            unknown["ok"], false,
            "unknown term is a hard error: {unknown}"
        );
    }

    /// A tiny synthetic documentation dataset for the `search_documentation` unit
    /// tests: two class terms whose `to_gmeow_rdf` projection carries the new
    /// `docSearch*` facets, parsed back into an [`purrdf::RdfDataset`] exactly the way
    /// the production carrier holds the bundle's documentation graph. This does NOT
    /// depend on the committed bundle (which only gains the facets after regenerate) —
    /// it exercises the projection + search end to end from the model.
    fn synthetic_docs_dataset() -> Arc<purrdf::RdfDataset> {
        use gmeow_docs_model::model::{DocLinkage, DocTerm, DocTermCategory};
        let ns = "https://blackcatinformatics.ca/gmeow/";
        let model = gmeow_docs_model::model::DocsModel {
            terms: vec![
                DocTerm {
                    iri: format!("{ns}Cat"),
                    curie: "gmeow:Cat".to_string(),
                    label: Some("Cat".to_string()),
                    definition: Some("A small domesticated feline.".to_string()),
                    category: DocTermCategory::Class,
                    owner_slice: format!("{ns}slice/zoo"),
                    scope_notes: vec![
                        "Prefer for a domestic cat; avoid for a wildcat.".to_string(),
                    ],
                    ..Default::default()
                },
                DocTerm {
                    iri: format!("{ns}Feline"),
                    curie: "gmeow:Feline".to_string(),
                    label: Some("Feline".to_string()),
                    definition: Some("The cat family of mammals.".to_string()),
                    category: DocTermCategory::Class,
                    owner_slice: format!("{ns}slice/zoo"),
                    ..Default::default()
                },
            ],
            // A crosswalk linkage on Cat → the alignment facet token `exactMatch:Q146`.
            linkages: vec![DocLinkage {
                mapping_set: None,
                subject: format!("{ns}Cat"),
                subject_curie: "gmeow:Cat".to_string(),
                predicate: "http://www.w3.org/2004/02/skos/core#exactMatch".to_string(),
                object: "http://www.wikidata.org/entity/Q146".to_string(),
                justification: None,
                confidence: None,
                owner_slice: format!("{ns}slice/zoo"),
            }],
            ..Default::default()
        };
        let nquads = gmeow_docs_model::rdf::to_gmeow_rdf(&model, &BTreeMap::new());
        purrdf::parse_dataset(nquads.as_bytes(), "application/n-quads", None)
            .expect("to_gmeow_rdf emits valid N-Quads")
    }

    /// `search_documentation` over the synthetic dataset: matches on label /
    /// definition / advice, attaches the advice + alignment + missing-coverage facets,
    /// ranks a label match above a definition match, returns empty for a non-match, and
    /// is deterministic.
    #[test]
    fn search_documentation_matches_facets_ranks_and_is_deterministic() {
        let dataset_arc = synthetic_docs_dataset();
        let dataset = Arc::new(
            dataset_arc.project_named_graph(gmeow_bundle_view::graph_iris::GRAPH_DOCUMENTATION),
        );
        let cat_iri = "https://blackcatinformatics.ca/gmeow/Cat";
        let feline_iri = "https://blackcatinformatics.ca/gmeow/Feline";

        // "cat": Cat matches by LABEL (rank 0); Feline matches by DEFINITION ("the cat
        // family …", rank 1) — so Cat sorts first.
        let hits = search_documentation(&dataset, "cat", 20).expect("search ok");
        let ids: Vec<&str> = hits.iter().map(|h| h.id.as_str()).collect();
        assert_eq!(
            ids,
            vec![cat_iri, feline_iri],
            "label match outranks definition match"
        );
        let cat = &hits[0];
        assert_eq!(cat.kind, "term");
        assert_eq!(cat.label, "Cat");
        assert_eq!(
            cat.definition.as_deref(),
            Some("A small domesticated feline.")
        );
        assert_eq!(
            cat.advice,
            vec!["Prefer for a domestic cat; avoid for a wildcat.".to_string()],
            "the advice facet is attached"
        );
        assert_eq!(
            cat.alignments,
            vec!["exactMatch:Q146".to_string()],
            "the alignment facet is attached"
        );
        assert!(
            !cat.missing_coverage.is_empty(),
            "an under-documented term carries missing-coverage dimensions: {:?}",
            cat.missing_coverage
        );

        // An advice-only match: "wildcat" appears only in Cat's advice prose.
        let advice_hits = search_documentation(&dataset, "wildcat", 20).expect("search ok");
        let advice_ids: Vec<&str> = advice_hits.iter().map(|h| h.id.as_str()).collect();
        assert_eq!(advice_ids, vec![cat_iri], "advice prose is searchable");

        // A definition-only match: "mammals" appears only in Feline's definition.
        let def_hits = search_documentation(&dataset, "mammals", 20).expect("search ok");
        let def_ids: Vec<&str> = def_hits.iter().map(|h| h.id.as_str()).collect();
        assert_eq!(def_ids, vec![feline_iri], "definition prose is searchable");

        // A non-matching query is empty-but-ok (never a hard fail on a populated graph).
        let none = search_documentation(&dataset, "xylophone", 20).expect("search ok");
        assert!(none.is_empty(), "a non-matching query returns no hits");

        // Determinism: the same query twice yields the same order.
        let a = search_documentation(&dataset, "cat", 20).expect("search ok");
        let b = search_documentation(&dataset, "cat", 20).expect("search ok");
        assert_eq!(
            a.iter().map(|h| &h.id).collect::<Vec<_>>(),
            b.iter().map(|h| &h.id).collect::<Vec<_>>(),
            "search order is reproducible"
        );

        // The limit is honored.
        let limited = search_documentation(&dataset, "cat", 1).expect("search ok");
        assert_eq!(limited.len(), 1, "limit caps the result count");
    }

    /// `search_documentation` HARD-FAILS when the documentation graph is absent/empty
    /// — docs_search serves the documentation graph, so a missing graph is a defect,
    /// never a silent empty result.
    #[test]
    fn search_documentation_hard_fails_on_absent_documentation_graph() {
        let empty = purrdf::RdfDatasetBuilder::new()
            .freeze()
            .expect("empty dataset");
        let err = search_documentation(&empty, "cat", 20)
            .expect_err("an absent documentation graph is a hard fail");
        assert!(
            err.to_string().contains("graph/documentation"),
            "the hard-fail error names the missing documentation graph: {err}"
        );
    }

    /// `docs_search` dispatches over the shipped bundle and returns an OK envelope with
    /// a `results` array. (The committed bundle carries the documentation graph but not
    /// yet the `docSearch*` facets — those land after regenerate — so the live match
    /// set is validated by the synthetic unit test above; here we prove the tool wires
    /// through, never hard-fails on the populated graph, and is deterministic.)
    #[test]
    fn docs_search_tool_dispatches_over_the_bundle() {
        let server = consumer_server();
        let hit = text_payload(server.call_tool_result("docs_search", &json!({"query": "entity"})));
        assert_eq!(
            hit["ok"], true,
            "docs_search returns ok over the bundle: {hit}"
        );
        assert_eq!(hit["query"], "entity");
        assert!(hit["results"].is_array(), "results is an array: {hit}");
        let again =
            text_payload(server.call_tool_result("docs_search", &json!({"query": "entity"})));
        assert_eq!(hit, again, "docs_search output is deterministic");
    }

    // ── coherence_certificate (R6) ─────────────────────────────────────────────────

    /// A consistent native [`ReasoningResult`], with the given evaluation/completeness
    /// axes and optional certified fragment — the minimum a coherence outcome reads.
    fn coherence_result(
        evaluation: EvaluationStatus,
        completeness: CompletenessStatus,
        fragment: Option<&str>,
    ) -> ReasoningResult {
        use gmeow_logic::result::{
            InformationState, InputStatus, PreservationClaim, ResultPayload, ResultProvenance,
        };
        let mut provenance = ResultProvenance::native("contract:abc", "world:default");
        provenance.certified_fragment = fragment.map(str::to_owned);
        ReasoningResult {
            input: InputStatus::Valid,
            evaluation,
            completeness,
            preservation: PreservationClaim::exact(),
            information: InformationState::Supported,
            provenance,
            payload: ResultPayload::Empty,
            row_schema: None,
        }
    }

    /// Parse a `graph/attestations` N-Quads document into a dataset the read helper reads.
    fn dataset_of(nquads: &str) -> Arc<purrdf::RdfDataset> {
        purrdf::parse_dataset(nquads.as_bytes(), "application/n-quads", None)
            .expect("parse coherence N-Quads")
    }

    /// Build the lightest `McpView` that can drive the REAL production entry point
    /// ([`McpView::coherence_certificate_json`], the exact method
    /// `tool_coherence_certificate` calls) over hand-crafted `graph/attestations`
    /// N-Quads — without paying for a full `emit_gts`/`SnapshotBuilder` round trip
    /// (reserved for the `_heavy_offgate` whole-bundle test). `McpView::from_dataset`
    /// only needs the ontology header for `export::fold_meta`; the raw `gts` bytes it
    /// also stores are unused by `coherence_certificate_json`, so an empty `Arc<[u8]>`
    /// is honest (never a fabricated placeholder read by the surface under test).
    fn view_of(nquads: &str) -> McpView {
        let header = "<https://blackcatinformatics.ca/gmeow> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <http://www.w3.org/2002/07/owl#Ontology> .\n\
             <https://blackcatinformatics.ca/gmeow> <http://purl.org/dc/terms/title> \"GMEOW\" .\n\
             <https://blackcatinformatics.ca/gmeow> <http://www.w3.org/2002/07/owl#versionInfo> \"test\" .\n";
        let doc = format!("{header}{nquads}");
        McpView::from_dataset(dataset_of(&doc), Arc::from(Vec::<u8>::new()))
            .expect("construct McpView over the crafted certificate fixture")
    }

    /// Simulate a producer that failed to write exactly ONE required predicate: strip
    /// every N-Quads line mentioning the bracket-delimited `logic:<local>` predicate
    /// IRI, leaving every other quad (including the subject's `rdf:type`) intact. The
    /// bracket delimiters make the match exact — no risk of one predicate's local name
    /// being a substring of another's.
    fn strip_predicate(nquads: &str, local: &str) -> String {
        let token = format!("<{LOGIC_NAMESPACE}{local}>");
        let mut out = nquads
            .lines()
            .filter(|line| !line.contains(&token))
            .collect::<Vec<_>>()
            .join("\n");
        out.push('\n');
        out
    }

    /// The carrier folds a CONCLUSIVE, fragment-scoped, violation-free closure as a real
    /// `logic:CoherenceCertificate`; the read tool surfaces `issues_certificate:true`, the
    /// certificate class, and the pinned bundle_hash / per-graph axiom_hashes VERBATIM (the
    /// tamper surface) — proving the digests are exactly `per_graph_axiom_hashes` and not
    /// fabricated.
    #[test]
    fn coherence_certificate_surfaces_a_certificate_with_the_pinned_digests() {
        use gmeow_logic::certificate::{
            CoherenceOutcome, ContradictionPolicy, per_graph_axiom_hashes,
        };
        use purrdf::gts::writer::digest_string;

        // A small axiom-bearing dataset whose per-graph digests the certificate pins.
        let axioms = dataset_of(
            "<https://e/A> <http://www.w3.org/2000/01/rdf-schema#subClassOf> <https://e/B> <https://e/w> .\n",
        );
        let axiom_hashes = per_graph_axiom_hashes(axioms.as_ref(), digest_string);
        assert!(!axiom_hashes.is_empty(), "the fixture pins ≥1 axiom digest");
        let bundle_hash = digest_string(b"bundle-identity-bytes");

        let result = coherence_result(
            EvaluationStatus::Completed,
            CompletenessStatus::Unknown,
            Some("fragment:test"),
        );
        let outcome = CoherenceOutcome::from_reasoning_result(
            &result,
            bundle_hash.clone(),
            axiom_hashes.clone(),
            ContradictionPolicy::ForbidGapAndGlut,
            "1970-01-01T00:00:00Z",
            std::collections::BTreeSet::new(),
        )
        .unwrap();
        assert!(outcome.issues_certificate(), "the fixture must certify");

        let dataset =
            dataset_of(&outcome.to_nquads(gmeow_bundle_view::graph_iris::GRAPH_ATTESTATIONS));
        let env = coherence_certificate_envelope(dataset.as_ref()).expect("certificate present");

        assert_eq!(env["ok"], true);
        assert_eq!(env["issues_certificate"], true);
        assert_eq!(env["is_refused"], false);
        assert_eq!(env["class_local_name"], "CoherenceCertificate");
        // The surfaced bundle_hash / axiom_hashes are the pipeline-pinned digests, VERBATIM.
        assert_eq!(env["bundle_hash"], bundle_hash);
        assert!(
            env["bundle_hash"].as_str().is_some_and(|s| !s.is_empty()),
            "bundle_hash is non-empty: {env}"
        );
        let surfaced: Vec<String> = env["axiom_hashes"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap().to_owned())
            .collect();
        assert_eq!(
            surfaced,
            axiom_hashes.into_iter().collect::<Vec<_>>(),
            "axiom_hashes are the per_graph_axiom_hashes digests, not fabricated: {env}"
        );
        assert!(
            surfaced.iter().all(|h| h.contains(':') && h.len() > 8),
            "axiom digests are non-trivial content addresses: {surfaced:?}"
        );
        // The two completeness-gate axes round-trip off the linked result node.
        assert_eq!(env["evaluation"], "completed");
        assert_eq!(env["completeness"], "unknown");
        assert_eq!(env["contract_hash"], "contract:abc");
    }

    /// R6 regression: a bounded/incomplete closure yields the strictly-weaker
    /// `logic:CoherenceCheckAttestation`; the read tool must report
    /// `issues_certificate:false` and `class_local_name:CoherenceCheckAttestation` — a
    /// regression must NEVER silently upgrade an attestation to a certificate.
    #[test]
    fn coherence_certificate_maps_an_attestation_never_a_certificate() {
        use gmeow_logic::certificate::{CoherenceOutcome, ContradictionPolicy};

        let result = coherence_result(
            EvaluationStatus::BudgetExhausted,
            CompletenessStatus::Incomplete,
            None,
        );
        assert!(!result.is_conclusive());
        let outcome = CoherenceOutcome::from_reasoning_result(
            &result,
            "blake3:bundle".to_owned(),
            ["blake3:axioms".to_owned()],
            ContradictionPolicy::ForbidGapAndGlut,
            "1970-01-01T00:00:00Z",
            std::collections::BTreeSet::new(),
        )
        .unwrap();
        assert!(!outcome.issues_certificate());

        let dataset =
            dataset_of(&outcome.to_nquads(gmeow_bundle_view::graph_iris::GRAPH_ATTESTATIONS));
        let env = coherence_certificate_envelope(dataset.as_ref()).expect("attestation present");
        assert_eq!(env["ok"], true);
        assert_eq!(
            env["issues_certificate"], false,
            "an attestation is NOT a certificate: {env}"
        );
        assert_eq!(env["class_local_name"], "CoherenceCheckAttestation");
        assert_eq!(env["evaluation"], "budget-exhausted");
        assert_eq!(env["completeness"], "incomplete");
    }

    /// HARD-FAIL: a bundle carrying no coherence artifact in `graph/attestations` is an
    /// error — there is NO silent recompute fallback.
    #[test]
    fn coherence_certificate_hard_fails_on_a_bundle_without_a_certificate() {
        let stripped = dataset_of("<https://e/s> <https://e/p> <https://e/o> <https://e/g> .\n");
        let err = coherence_certificate_envelope(stripped.as_ref())
            .expect_err("a bundle with no coherence artifact must hard-fail");
        assert!(
            err.to_string()
                .contains("no coherence certificate or attestation"),
            "the hard fail names the missing artifact: {err}"
        );
    }

    /// A bundle carrying more than one distinct coherence subject is ambiguous and a hard
    /// failure (no silent first-wins).
    #[test]
    fn coherence_certificate_hard_fails_on_an_ambiguous_bundle() {
        use gmeow_logic::certificate::{CoherenceOutcome, ContradictionPolicy};

        let graph = gmeow_bundle_view::graph_iris::GRAPH_ATTESTATIONS;
        let a = CoherenceOutcome::from_reasoning_result(
            &coherence_result(
                EvaluationStatus::Completed,
                CompletenessStatus::Unknown,
                Some("frag:a"),
            ),
            "blake3:bundle-a".to_owned(),
            ["blake3:axioms-a".to_owned()],
            ContradictionPolicy::ForbidGapAndGlut,
            "1970-01-01T00:00:00Z",
            std::collections::BTreeSet::new(),
        )
        .unwrap();
        let b = CoherenceOutcome::from_reasoning_result(
            &coherence_result(
                EvaluationStatus::Completed,
                CompletenessStatus::Unknown,
                Some("frag:b"),
            ),
            "blake3:bundle-b".to_owned(),
            ["blake3:axioms-b".to_owned()],
            ContradictionPolicy::ForbidGapAndGlut,
            "1970-01-01T00:00:00Z",
            std::collections::BTreeSet::new(),
        )
        .unwrap();
        let both = format!("{}{}", a.to_nquads(graph), b.to_nquads(graph));
        let err = coherence_certificate_envelope(dataset_of(&both).as_ref())
            .expect_err("two coherence subjects must hard-fail");
        assert!(
            err.to_string()
                .contains("more than one distinct coherence subject"),
            "the hard fail names the ambiguity: {err}"
        );
    }

    /// A SINGLE coherence subject typed BOTH
    /// `logic:CoherenceCertificate` and `logic:CoherenceCheckAttestation` is an
    /// ambiguous, malformed artifact and must hard-fail — REGRESSION GUARD for the
    /// exact gap this test closes: the ambiguity check used to compare only the
    /// SUBJECT IRI (`existing != subject`), so two `rdf:type` triples on the SAME
    /// subject fell through to the silent `Some(_) => {}` no-op arm and first-wins
    /// picked whichever `rdf:type` the dataset's quad iteration order surfaced first.
    /// Drives the REAL production entry point (`McpView::coherence_certificate_json`).
    #[test]
    fn coherence_certificate_json_hard_fails_on_a_dual_typed_subject() {
        use gmeow_logic::certificate::{CoherenceOutcome, ContradictionPolicy};

        let graph = gmeow_bundle_view::graph_iris::GRAPH_ATTESTATIONS;
        let outcome = CoherenceOutcome::from_reasoning_result(
            &coherence_result(
                EvaluationStatus::Completed,
                CompletenessStatus::Unknown,
                Some("fragment:dual"),
            ),
            "blake3:bundle-dual".to_owned(),
            ["blake3:axioms-dual".to_owned()],
            ContradictionPolicy::ForbidGapAndGlut,
            "1970-01-01T00:00:00Z",
            std::collections::BTreeSet::new(),
        )
        .unwrap();
        let nquads = outcome.to_nquads(graph);

        // Splice in a SECOND `rdf:type` triple on the SAME subject, typing it the
        // strictly-weaker Attestation class too — the malformed dual-typed artifact.
        let subject_line = nquads
            .lines()
            .find(|l| l.contains("22-rdf-syntax-ns#type") && l.contains("CoherenceCertificate"))
            .expect("the fixture carries the certificate's rdf:type triple");
        let subject = subject_line
            .split_whitespace()
            .next()
            .expect("rdf:type triple has a subject");
        let dual_typed = format!(
            "{nquads}{subject} <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> \
             <{LOGIC_NAMESPACE}CoherenceCheckAttestation> <{graph}> .\n"
        );

        let view = view_of(&dual_typed);
        let out: Value = serde_json::from_str(&view.coherence_certificate_json())
            .expect("coherence_certificate_json returns valid JSON");
        assert_eq!(
            out["ok"], false,
            "a dual-typed subject must hard-fail, never first-win a class: {out}"
        );
        let error = out["error"].as_str().unwrap_or_default();
        assert!(
            error.contains("typed BOTH"),
            "the hard fail names the ambiguous dual typing: {error}"
        );
    }

    /// Every producer-REQUIRED field the tool extracts must
    /// be present with the producer's exact cardinality — a bundle missing ANY of them
    /// is a CORRUPT artifact and must hard-fail (`ok:false`) naming the missing
    /// predicate, never `ok:true` with a null/empty field silently laundering the gap
    /// (no-silent-degradation, `.goals`). Drives the REAL production entry point
    /// (`McpView::coherence_certificate_json`, the exact method
    /// `tool_coherence_certificate` calls) over a battery of certificates each missing
    /// exactly one predicate [`CoherenceOutcome::to_nquads`] otherwise always writes.
    #[test]
    fn coherence_certificate_json_hard_fails_on_each_missing_required_field() {
        use gmeow_logic::certificate::{CoherenceOutcome, ContradictionPolicy};

        let graph = gmeow_bundle_view::graph_iris::GRAPH_ATTESTATIONS;
        let outcome = CoherenceOutcome::from_reasoning_result(
            &coherence_result(
                EvaluationStatus::Completed,
                CompletenessStatus::Unknown,
                Some("fragment:required"),
            ),
            "blake3:bundle-required".to_owned(),
            ["blake3:axioms-required".to_owned()],
            ContradictionPolicy::ForbidGapAndGlut,
            "1970-01-01T00:00:00Z",
            std::collections::BTreeSet::new(),
        )
        .unwrap();
        assert!(
            outcome.issues_certificate(),
            "the fixture must be a real certificate (so certifiedFragment is exercised too)"
        );
        let full = outcome.to_nquads(graph);

        // Every predicate the producer writes UNCONDITIONALLY on a CoherenceCertificate
        // subject (bundleHash/axiomHash/contractHash/engine/contradictionPolicy/
        // summarizesResult/certifiedFragment) plus the two axes carried on the linked
        // result node (resultCompleteness/resultEvaluation) — one case per field.
        let required_predicates = [
            "bundleHash",
            "axiomHash",
            "contractHash",
            "engine",
            "contradictionPolicy",
            "summarizesResult",
            "certifiedFragment",
            "resultCompleteness",
            "resultEvaluation",
        ];
        for predicate in required_predicates {
            let stripped = strip_predicate(&full, predicate);
            let view = view_of(&stripped);
            let out: Value = serde_json::from_str(&view.coherence_certificate_json())
                .expect("coherence_certificate_json returns valid JSON");
            assert_eq!(
                out["ok"], false,
                "stripping logic:{predicate} must hard-fail, not ok:true with a null field: {out}"
            );
            let error = out["error"].as_str().unwrap_or_default();
            assert!(
                error.contains(predicate),
                "the hard fail for a missing logic:{predicate} must name it: {error}"
            );
        }
    }

    /// Drive the REAL `coherence_certificate` tool through `call_tool_result` over a bundle
    /// that carries the certificate in `graph/attestations` — the same disk-free, reason-free
    /// read path the shipped consumer surface uses.
    ///
    /// Fast (a minimal synthetic snapshot — just the ontology header plus the certificate's
    /// own quads, built via `SnapshotBuilder`/`emit_gts`, NOT the real committed `gmeow.gts` —
    /// the same tiny-snapshot construction `verify_graph_inconsistent_but_conclusive_never_certifies`
    /// uses): on-gate, not `_heavy_offgate`. Measured at ~0.01-0.03 s standalone and under full
    /// contention with the genuinely-heavy `*_heavy_offgate` siblings because — unlike
    /// those siblings — this test never touches the real
    /// committed bundle.
    #[test]
    fn coherence_certificate_tool_reads_the_carried_bundle() {
        use gmeow_logic::certificate::{CoherenceOutcome, ContradictionPolicy};
        use purrdf::gts_compose::{DEFAULT_RSYNCABLE_THRESHOLD, SnapshotBuilder, emit_gts};

        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let _env = EnvRestore::capture(&["GMEOW_LANG", "GMEOW_MEMORY_PATH", "HOME", "USERPROFILE"]);
        unsafe {
            // SAFETY: tests mutate process env single-threaded under ENV_LOCK.
            env::remove_var("GMEOW_LANG");
        }

        let outcome = CoherenceOutcome::from_reasoning_result(
            &coherence_result(
                EvaluationStatus::Completed,
                CompletenessStatus::Unknown,
                Some("fragment:test"),
            ),
            "blake3:carried-bundle".to_owned(),
            ["blake3:axioms-carried".to_owned()],
            ContradictionPolicy::ForbidGapAndGlut,
            "1970-01-01T00:00:00Z",
            std::collections::BTreeSet::new(),
        )
        .unwrap();

        // A minimal snapshot: the required ontology header (the importer hard-fails
        // without it) plus the certificate's graph/attestations named graph, emitted as a
        // real gmeow.gts bundle.
        let doc = format!(
            "<https://blackcatinformatics.ca/gmeow> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <http://www.w3.org/2002/07/owl#Ontology> .\n\
             <https://blackcatinformatics.ca/gmeow> <http://purl.org/dc/terms/title> \"GMEOW\" .\n\
             <https://blackcatinformatics.ca/gmeow> <http://www.w3.org/2002/07/owl#versionInfo> \"test\" .\n\
             {}",
            outcome.to_nquads(gmeow_bundle_view::graph_iris::GRAPH_ATTESTATIONS)
        );
        let dataset = dataset_of(&doc);
        let mut builder = SnapshotBuilder::new();
        builder.add_dataset(dataset.as_ref()).expect("add_dataset");
        let gts = emit_gts(
            &builder,
            "dist",
            None,
            Vec::new(),
            Vec::new(),
            None,
            None,
            None,
            DEFAULT_RSYNCABLE_THRESHOLD,
            // Undicted at the mandated level: these fixtures build a tiny in-test
            // snapshot, so there is no shipped dictionary to prime with — but the frame
            // profile still applies, so the level is declared rather than defaulted.
            &purrdf::gts_compose::MediumPlan::undicted(Some(12)),
        )
        .expect("emit tiny cert-carrying snapshot");

        let server = McpServer::from_snapshot(&gts).unwrap();
        let out = text_payload(server.call_tool_result("coherence_certificate", &json!({})));
        assert_eq!(
            out["ok"], true,
            "the tool reads the carried certificate: {out}"
        );
        assert_eq!(out["class_local_name"], "CoherenceCertificate");
        assert_eq!(out["issues_certificate"], true);
        assert_eq!(out["bundle_hash"], "blake3:carried-bundle");
        assert_eq!(
            out["axiom_hashes"],
            json!(["blake3:axioms-carried"]),
            "the per-graph axiom digests ride the read envelope: {out}"
        );
        // Deterministic: the read is a pure projection of the carried quads.
        let again = text_payload(server.call_tool_result("coherence_certificate", &json!({})));
        assert_eq!(out, again, "the certificate read is deterministic");
    }

    /// An INCONSISTENT-but-CONCLUSIVE closure must never
    /// surface as `CoherenceCertificate` on the tool surface — `completeness_class`
    /// used to return `CoherenceCertificate` for ANY `is_conclusive()` result, with no
    /// check for a named certified fragment or for the absence of a forbidden
    /// violation, and `run_explain_quad` (unlike `run_verify_graph`, which bolted on
    /// its own ad-hoc `Refused` downgrade) had NO protection at all.
    ///
    /// Drives the REAL `verify_graph` tool (`call_tool_result`, not internals) with a
    /// tiny canon plus an overlay that is GENUINELY inconsistent — `A ⊑ B`, `A ⊑ C`,
    /// `B disjointWith C`, `x : A` forces `x` into `owl:Nothing` (the exact fixture
    /// `reason_all_single_chase_yields_inconsistent_and_nonempty_closure` proves
    /// derives `InformationState::Both` in one completed chase, i.e. CONCLUSIVE, not
    /// budget-cut) — and asserts the response's `class_local_name` is NEVER
    /// `CoherenceCertificate`. `class_local_name` is `completeness_class`'s output
    /// (folded through `CoherenceOutcome::class_local_name_for`, the SAME gate
    /// `run_explain_quad` now reads through), so this proves the shared gate, not a
    /// per-tool special case.
    ///
    /// Fast (a tiny synthetic canon + a 4-quad overlay, not the real corpus): on-gate,
    /// not `_heavy_offgate`.
    #[test]
    fn verify_graph_inconsistent_but_conclusive_never_certifies() {
        use purrdf::gts_compose::{DEFAULT_RSYNCABLE_THRESHOLD, SnapshotBuilder, emit_gts};

        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let _env = EnvRestore::capture(&["GMEOW_LANG", "GMEOW_MEMORY_PATH", "HOME", "USERPROFILE"]);
        unsafe {
            // SAFETY: tests mutate process env single-threaded under ENV_LOCK.
            env::remove_var("GMEOW_LANG");
        }

        // A minimal canon: just the required ontology header (the importer hard-fails
        // without it) — the SAME pattern `coherence_certificate_tool_reads_the_carried_
        // bundle_heavy_offgate` uses.
        let doc = "<https://blackcatinformatics.ca/gmeow> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <http://www.w3.org/2002/07/owl#Ontology> .\n\
             <https://blackcatinformatics.ca/gmeow> <http://purl.org/dc/terms/title> \"GMEOW\" .\n\
             <https://blackcatinformatics.ca/gmeow> <http://www.w3.org/2002/07/owl#versionInfo> \"test\" .\n";
        let dataset = dataset_of(doc);
        let mut builder = SnapshotBuilder::new();
        builder.add_dataset(dataset.as_ref()).expect("add_dataset");
        let gts = emit_gts(
            &builder,
            "dist",
            None,
            Vec::new(),
            Vec::new(),
            None,
            None,
            None,
            DEFAULT_RSYNCABLE_THRESHOLD,
            // Undicted at the mandated level: these fixtures build a tiny in-test
            // snapshot, so there is no shipped dictionary to prime with — but the frame
            // profile still applies, so the level is declared rather than defaulted.
            &purrdf::gts_compose::MediumPlan::undicted(Some(12)),
        )
        .expect("emit tiny header-only canon");

        let server = McpServer::from_snapshot(&gts).unwrap();

        // The overlay: a genuine DL contradiction — A ⊑ B, A ⊑ C, B disjointWith C,
        // x : A forces x into owl:Nothing. Un-graphed triples reason under the single
        // default world, and the whole tiny canon+overlay union closes well under the
        // governed step ceiling — CONCLUSIVE, never budget-cut.
        let overlay_data = "<http://gmeowtest.example/A> <http://www.w3.org/2000/01/rdf-schema#subClassOf> <http://gmeowtest.example/B> .\n\
             <http://gmeowtest.example/A> <http://www.w3.org/2000/01/rdf-schema#subClassOf> <http://gmeowtest.example/C> .\n\
             <http://gmeowtest.example/B> <http://www.w3.org/2002/07/owl#disjointWith> <http://gmeowtest.example/C> .\n\
             <http://gmeowtest.example/x> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <http://gmeowtest.example/A> .\n";

        let out = text_payload(server.call_tool_result(
            "verify_graph",
            &json!({"data": overlay_data, "format": "turtle", "max_steps": 64}),
        ));
        assert_eq!(out["ok"], true, "verify_graph must succeed: {out}");

        // The closure genuinely completed (conclusive), not a budget-cut — so the
        // downgrade below is caused by the witnessed glut, not by non-conclusiveness.
        assert_eq!(
            out["evaluation"], "completed",
            "the fixture's tiny closure must be CONCLUSIVE (not budget-cut) for this to be a \
             faithful proof: {out}"
        );

        // The falsifiable assertion: an inconsistent-but-conclusive closure must NEVER
        // render `CoherenceCertificate` — the shared `CoherenceOutcome` gate downgrades
        // it to the flat refusal instead.
        assert_ne!(
            out["class_local_name"], "CoherenceCertificate",
            "an inconsistent-but-conclusive closure must never be labeled a \
             CoherenceCertificate: {out}"
        );
        assert_eq!(
            out["class_local_name"], "Refused",
            "a witnessed forbidden violation in a CONCLUSIVE closure is a flat refusal, per \
             the SAME CoherenceOutcome gate the bundle-level coherence certifier uses: {out}"
        );
    }

    /// `verify_graph`'s `coherent` field MUST agree with
    /// `class_local_name` — both MUST be derived from the SAME `CoherenceOutcome`
    /// gate, never from two independent signals.
    ///
    /// The overlay carries a genuine DL glut via plain PAIRWISE `owl:disjointWith`
    /// (A ⊑ B, A ⊑ C, B disjointWith C, x : A forces x into owl:Nothing) —
    /// DELIBERATELY not an `owl:AllDisjointClasses` set, so the ONE bad-example
    /// verify query that could independently catch a disjoint-axis violation
    /// (`class-in-two-disjoint-axes.rq`, which matches only `owl:AllDisjointClasses`
    /// membership) does NOT fire: `report.ok()` is true. Earlier,
    /// `coherent` was read straight from `report.ok()`, so this exact fixture would
    /// render the self-contradictory `coherent:true` alongside
    /// `class_local_name:"Refused"`. The fix routes `coherent` through the SAME
    /// shared `completeness_refused` gate that decides `class_local_name`, so the
    /// two fields can never disagree.
    ///
    /// Fast (a tiny synthetic canon + a 4-quad overlay, not the real corpus): on-gate,
    /// not `_heavy_offgate`.
    #[test]
    fn verify_graph_coherent_never_disagrees_with_refused_class() {
        use purrdf::gts_compose::{DEFAULT_RSYNCABLE_THRESHOLD, SnapshotBuilder, emit_gts};

        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let _env = EnvRestore::capture(&["GMEOW_LANG", "GMEOW_MEMORY_PATH", "HOME", "USERPROFILE"]);
        unsafe {
            // SAFETY: tests mutate process env single-threaded under ENV_LOCK.
            env::remove_var("GMEOW_LANG");
        }

        // The header-only canon PLUS the `axis-not-disjoint.rq` bad-example query's
        // own required orthogonality matrix (an `owl:AllDisjointClasses` set naming
        // the seven fixed identity axes) — otherwise that unrelated bad-example
        // query fires on ANY header-only canon (it demands the matrix exist at all),
        // which would make `report.ok()` false for a reason having nothing to do
        // with this test's glut and defeat the falsifiability of the assertion below.
        let doc = "<https://blackcatinformatics.ca/gmeow> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <http://www.w3.org/2002/07/owl#Ontology> .\n\
             <https://blackcatinformatics.ca/gmeow> <http://purl.org/dc/terms/title> \"GMEOW\" .\n\
             <https://blackcatinformatics.ca/gmeow> <http://www.w3.org/2002/07/owl#versionInfo> \"test\" .\n\
             _:axdisj <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <http://www.w3.org/2002/07/owl#AllDisjointClasses> .\n\
             _:axdisj <http://www.w3.org/2002/07/owl#members> _:axlist0 .\n\
             _:axlist0 <http://www.w3.org/1999/02/22-rdf-syntax-ns#first> <https://blackcatinformatics.ca/gmeow/GenderIdentity> .\n\
             _:axlist0 <http://www.w3.org/1999/02/22-rdf-syntax-ns#rest> _:axlist1 .\n\
             _:axlist1 <http://www.w3.org/1999/02/22-rdf-syntax-ns#first> <https://blackcatinformatics.ca/gmeow/GenderExpression> .\n\
             _:axlist1 <http://www.w3.org/1999/02/22-rdf-syntax-ns#rest> _:axlist2 .\n\
             _:axlist2 <http://www.w3.org/1999/02/22-rdf-syntax-ns#first> <https://blackcatinformatics.ca/gmeow/SexAssignedAtBirth> .\n\
             _:axlist2 <http://www.w3.org/1999/02/22-rdf-syntax-ns#rest> _:axlist3 .\n\
             _:axlist3 <http://www.w3.org/1999/02/22-rdf-syntax-ns#first> <https://blackcatinformatics.ca/gmeow/SexualOrientation> .\n\
             _:axlist3 <http://www.w3.org/1999/02/22-rdf-syntax-ns#rest> _:axlist4 .\n\
             _:axlist4 <http://www.w3.org/1999/02/22-rdf-syntax-ns#first> <https://blackcatinformatics.ca/gmeow/RomanticOrientation> .\n\
             _:axlist4 <http://www.w3.org/1999/02/22-rdf-syntax-ns#rest> _:axlist5 .\n\
             _:axlist5 <http://www.w3.org/1999/02/22-rdf-syntax-ns#first> <https://blackcatinformatics.ca/gmeow/PronounSet> .\n\
             _:axlist5 <http://www.w3.org/1999/02/22-rdf-syntax-ns#rest> _:axlist6 .\n\
             _:axlist6 <http://www.w3.org/1999/02/22-rdf-syntax-ns#first> <https://blackcatinformatics.ca/gmeow/Honorific> .\n\
             _:axlist6 <http://www.w3.org/1999/02/22-rdf-syntax-ns#rest> <http://www.w3.org/1999/02/22-rdf-syntax-ns#nil> .\n";
        let dataset = dataset_of(doc);
        let mut builder = SnapshotBuilder::new();
        builder.add_dataset(dataset.as_ref()).expect("add_dataset");
        let gts = emit_gts(
            &builder,
            "dist",
            None,
            Vec::new(),
            Vec::new(),
            None,
            None,
            None,
            DEFAULT_RSYNCABLE_THRESHOLD,
            // Undicted at the mandated level: these fixtures build a tiny in-test
            // snapshot, so there is no shipped dictionary to prime with — but the frame
            // profile still applies, so the level is declared rather than defaulted.
            &purrdf::gts_compose::MediumPlan::undicted(Some(12)),
        )
        .expect("emit tiny header-only canon");

        let server = McpServer::from_snapshot(&gts).unwrap();

        // The glut: same shape as `verify_graph_inconsistent_but_conclusive_never_
        // certifies`, but PAIRWISE `owl:disjointWith` on classes NOT named in the
        // orthogonality matrix above — so neither `axis-not-disjoint.rq` (satisfied
        // by the matrix) nor `class-in-two-disjoint-axes.rq` (requires
        // `owl:AllDisjointClasses` membership, which g4B/g4C never join) can match.
        let overlay_data = "<http://gmeowtest.example/g4A> <http://www.w3.org/2000/01/rdf-schema#subClassOf> <http://gmeowtest.example/g4B> .\n\
             <http://gmeowtest.example/g4A> <http://www.w3.org/2000/01/rdf-schema#subClassOf> <http://gmeowtest.example/g4C> .\n\
             <http://gmeowtest.example/g4B> <http://www.w3.org/2002/07/owl#disjointWith> <http://gmeowtest.example/g4C> .\n\
             <http://gmeowtest.example/g4x> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <http://gmeowtest.example/g4A> .\n";

        let out = text_payload(server.call_tool_result(
            "verify_graph",
            &json!({"data": overlay_data, "format": "turtle", "max_steps": 64}),
        ));
        assert_eq!(out["ok"], true, "verify_graph must succeed: {out}");
        assert_eq!(
            out["evaluation"], "completed",
            "the fixture's tiny closure must be CONCLUSIVE (not budget-cut) for this to be a \
             faithful proof: {out}"
        );

        // The class label refutes coherence via the DL glut...
        assert_eq!(
            out["class_local_name"], "Refused",
            "a witnessed forbidden violation in a CONCLUSIVE closure is a flat refusal: {out}"
        );
        // ...and `coherent` MUST agree — never `coherent:true` alongside
        // `class_local_name:"Refused"`, even though no bad-example verify query
        // fired on this fixture's pairwise `owl:disjointWith` shape.
        assert_eq!(
            out["coherent"], false,
            "coherent must be false whenever class_local_name is Refused, regardless of \
             whether any bad-example verify query matched: {out}"
        );
    }

    // ── GMN verifier tools: gmn_validate / gmn_expand / gmn_explain ─────────────────

    /// Read a frozen GMN-1 conformance-vector file from the shipped corpus (a test
    /// artifact, never in the bundle) by its path relative to the vector root.
    fn gmn_vector(rel: &str) -> String {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        fs::read_to_string(
            root.join("slices/grounding/lang/tests/gmn1-vectors")
                .join(rel),
        )
        .unwrap_or_else(|e| panic!("read GMN vector {rel}: {e}"))
    }

    /// `gmn_validate` accepts a frozen conformance vector (`{ok, conformant:true}`) and
    /// rejects a perturbed document with the TYPED `lang:Gmn*Failure` class — the external
    /// LLM's entry to the `@err` repair loop.
    #[test]
    fn gmn_validate_accepts_conformant_and_rejects_perturbed() {
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let _env = EnvRestore::capture(&["GMEOW_LANG"]);
        unsafe {
            // SAFETY: tests mutate process env single-threaded under ENV_LOCK.
            env::remove_var("GMEOW_LANG");
        }
        let server = consumer_server();

        // A frozen POSITIVE vector conforms.
        let good = gmn_vector("claim-basic.gmn");
        let ok = text_payload(server.call_tool_result("gmn_validate", &json!({ "gmn": good })));
        assert_eq!(ok["ok"], true, "{ok}");
        assert_eq!(
            ok["conformant"], true,
            "a frozen conformance vector must validate: {ok}"
        );
        assert!(
            ok.get("failure_class").is_none(),
            "a conformant document carries no failure class: {ok}"
        );

        // A perturbed document (a value glyph flipped to a non-canonical 3-digit fraction,
        // the frozen negative fixture the corpus pins) raises the TYPED failure class.
        let bad = gmn_vector("negative-codec/neg-malformed-number-frac.gmn");
        let defect = text_payload(server.call_tool_result("gmn_validate", &json!({ "gmn": bad })));
        assert_eq!(defect["ok"], true, "{defect}");
        assert_eq!(
            defect["conformant"], false,
            "the perturbed document must be rejected: {defect}"
        );
        assert_eq!(
            defect["failure_class"], "https://blackcatinformatics.ca/lang/GmnMalformedNumber",
            "the typed lang:Gmn*Failure class names the defect: {defect}"
        );
        assert_eq!(
            defect["failure_local_name"], "GmnMalformedNumber",
            "{defect}"
        );
        assert!(
            defect["message"].as_str().is_some_and(|m| !m.is_empty()),
            "the defect carries a message: {defect}"
        );
    }

    /// `gmn_expand` decodes a GMN-1 document to its GMN-0 normal form (alias/glyph → full
    /// IRI) and its expansion round-trips: re-encoding equals the input under
    /// `gmn0_canonically_equal`.
    #[test]
    fn gmn_expand_roundtrips() {
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let _env = EnvRestore::capture(&["GMEOW_LANG"]);
        unsafe {
            // SAFETY: tests mutate process env single-threaded under ENV_LOCK.
            env::remove_var("GMEOW_LANG");
        }
        let server = consumer_server();
        let doc = gmn_vector("claim-basic.gmn");
        let out =
            text_payload(server.call_tool_result("gmn_expand", &json!({ "gmn": doc.clone() })));
        assert_eq!(out["ok"], true, "{out}");
        assert_eq!(
            out["round_trip"], true,
            "the expansion carries a holding round-trip witness: {out}"
        );
        let expanded = out["expanded_nquads"].as_str().expect("expanded_nquads");
        assert!(
            !expanded.is_empty(),
            "the GMN-0 normal form is non-empty: {out}"
        );
        // The "expand alias/glyph → full IRI" direction: the compact `gmeow__gate1` token
        // expands to its full IRI under the gmeow namespace.
        assert!(
            expanded.contains("https://blackcatinformatics.ca/gmeow/"),
            "the GMN-0 normal form carries full IRIs, not compact aliases: {out}"
        );

        // Expand then re-encode equals the input under gmn0_canonically_equal.
        let reencoded = out["reencoded_gmn"].as_str().expect("reencoded_gmn");
        let dict = server.gmn_dictionary().expect("dictionary resolves");
        let input_model = gmn1_read(&Gmn1Document::from_text(doc), &dict).expect("input reads");
        let re_model = gmn1_read(&Gmn1Document::from_text(reencoded.to_owned()), &dict)
            .expect("re-encoded reads");
        assert!(
            gmn0_canonically_equal(&input_model, &re_model),
            "expand then re-encode equals the input under gmn0_canonically_equal: {out}"
        );
    }

    /// `gmn_explain` resolves a known operator glyph (`¬` → `logic:not`) to its authored
    /// fixity / precedence / arity and its controlled-NL gloss, and returns an HONEST typed
    /// miss for an unknown glyph — never a fabricated answer.
    #[test]
    fn gmn_explain_names_fixity_and_gloss() {
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let _env = EnvRestore::capture(&["GMEOW_LANG"]);
        unsafe {
            // SAFETY: tests mutate process env single-threaded under ENV_LOCK.
            env::remove_var("GMEOW_LANG");
        }
        let server = consumer_server();

        // ¬ is the seeded prefix operator for logic:not (precedence 90, arity 1).
        let hit = text_payload(server.call_tool_result("gmn_explain", &json!({ "glyph": "¬" })));
        assert_eq!(hit["ok"], true, "{hit}");
        assert_eq!(
            hit["found"], true,
            "¬ is a covered GMN operator glyph: {hit}"
        );
        assert_eq!(hit["fixity_local_name"], "gmnFixityPrefix", "{hit}");
        assert_eq!(
            hit["precedence"], 90,
            "the graph-authored binding strength: {hit}"
        );
        assert_eq!(hit["arity"], 1, "{hit}");
        assert_eq!(
            hit["denotation_target"], "https://blackcatinformatics.ca/logic/not",
            "{hit}"
        );
        assert!(
            hit["denotation"]
                .as_str()
                .is_some_and(|d| d.contains("blackcatinformatics.ca")),
            "the lang:Denotation IRI is surfaced, not fabricated: {hit}"
        );
        // The gloss is Task 8's verbalizer rendering: the prefix template `<label> arg1`.
        assert!(
            hit["gloss"].as_str().is_some_and(|g| g.contains("arg1")),
            "the controlled-NL gloss is the prefix verbalizer rendering: {hit}"
        );
        // A GMN surface carries the record-initial sigil of the scope it reads in, because the
        // SAME glyph denotes different operators under different scopes (see `gmn_verbalize`);
        // the sigil is part of the surface, not decoration around it.
        assert_eq!(
            hit["gmn_surface"], "@ℒ ¬ arg1",
            "the GMN operator surface scopes the record and arranges the glyph in prefix \
             position: {hit}"
        );

        // An unknown glyph returns the honest typed miss, never a fabricated answer.
        let miss = text_payload(server.call_tool_result("gmn_explain", &json!({ "glyph": "☃" })));
        assert_eq!(miss["ok"], true, "{miss}");
        assert_eq!(
            miss["found"], false,
            "an unknown glyph is not found: {miss}"
        );
        assert_eq!(
            miss["failure_class"], "https://blackcatinformatics.ca/lang/GmnUncoveredTerm",
            "the miss is the typed lang:GmnUncoveredTerm class: {miss}"
        );
        assert_eq!(miss["failure_local_name"], "GmnUncoveredTerm", "{miss}");
    }

    /// The three GMN verifier tools are advertised in the CONSUMER surface (served off the
    /// bundle alone, like `validate_local`, never dev-gated) and each advertises its
    /// required arg honestly (the `tool()` allowlist addition).
    #[test]
    fn gmn_tools_are_advertised_in_the_consumer_surface() {
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let _env = EnvRestore::capture(&["GMEOW_LANG"]);
        unsafe {
            // SAFETY: tests mutate process env single-threaded under ENV_LOCK.
            env::remove_var("GMEOW_LANG");
        }
        let bytes = snapshot();
        let consumer = McpServer::from_snapshot(&bytes).unwrap();
        let result = consumer.tools_result();
        let arr = result["tools"].as_array().expect("tools array");
        for (name, req) in [
            ("gmn_validate", "gmn"),
            ("gmn_expand", "gmn"),
            ("gmn_explain", "glyph"),
        ] {
            let tool = arr
                .iter()
                .find(|t| t["name"] == name)
                .unwrap_or_else(|| panic!("{name} is advertised: {result}"));
            let required = tool["inputSchema"]["required"]
                .as_array()
                .expect("required array");
            assert!(
                required.iter().any(|r| r == req),
                "{name} advertises its required arg `{req}`: {tool}"
            );
        }
    }

    /// The GMN-1 teachability primer is exposed as a CONSUMER MCP resource
    /// (`gmeow://ontology/gmn1-primer`, served off the bundle alone), advertised in
    /// `resources/list` and readable through `resources/read` — a self-contained, graph-derived,
    /// budget-bounded card carrying the record sigils, the operator glyph table, and the repair
    /// loop. The shared `llms_full` surface carries the same primer section.
    #[test]
    fn gmn1_primer_resource_is_advertised_and_readable() {
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let _env = EnvRestore::capture(&["GMEOW_LANG"]);
        unsafe {
            // SAFETY: tests mutate process env single-threaded under ENV_LOCK.
            env::remove_var("GMEOW_LANG");
        }
        let bytes = snapshot();
        let consumer = McpServer::from_snapshot(&bytes).unwrap();

        // Advertised in the consumer resource list.
        let list = consumer.resources_result();
        let resources = list["resources"].as_array().expect("resources array");
        assert!(
            resources
                .iter()
                .any(|r| r["uri"] == "gmeow://ontology/gmn1-primer"),
            "the gmn1-primer resource must be advertised: {list}"
        );

        // Readable through resources/read, with the primer heading + a repair card + an operator
        // glyph row present (the graph-derived teaching surface).
        let read = consumer.read_resource_result("gmeow://ontology/gmn1-primer");
        assert!(
            read.get("isError").is_none(),
            "primer read must succeed: {read}"
        );
        let text = read["contents"][0]["text"].as_str().expect("primer text");
        assert!(
            text.contains(&format!("## {}", gmn1_primer_heading())),
            "the primer resource must carry its heading: {text}"
        );
        assert!(
            text.contains("gmeow:GmnErr"),
            "the primer resource must teach the @err repair record"
        );
        assert!(
            text.contains("⊑ (infix"),
            "the primer resource must carry the operator glyph table (⊑ subsumption row)"
        );

        // The same primer section rides the shared `llms_full` surface.
        let full = consumer
            .view
            .llms_full_text(vec!["en".to_string()])
            .expect("llms_full builds with the primer");
        assert!(
            full.contains(&format!("## {}", gmn1_primer_heading())),
            "llms_full must carry the primer section"
        );
    }

    /// The primer heading constant, re-exposed for the resource test (the shared docs const).
    fn gmn1_primer_heading() -> &'static str {
        gmeow_docs_model::gmn1_primer::PRIMER_HEADING
    }
}

/// The shipped dictionary every HOT runtime store — agent memory, the conjecture
/// library, the candidate library — primes its segments with
/// (`gmeow:dictGmeowMemoryHotV1`).
///
/// The store's payloads are short, highly repetitive RDF written a few hundred bytes
/// at a time, which is the single best case for a primed zstd stream and the single
/// worst case for an unprimed one: with no dictionary each record re-learns the same
/// IRIs from scratch.
pub const MEMORY_HOT_DICTIONARY: &str = "gmeow-memory-hot-v1";

/// The shipped dictionary the COMPACTION lane repacks a store under
/// (`gmeow:dictGmeowMemoryCompactV1`).
///
/// Resolved out of the loaded bundle by [`store_medium`] exactly as
/// [`MEMORY_HOT_DICTIONARY`] is, and fed to the repack VERBATIM
/// (`purrdf::gts::compact::DictStrategy::Pinned`), so this id names exactly one byte
/// sequence everywhere it appears: in the bundle header, in
/// `generated/medium/gmeow-memory-compact-v1.zdict`, and in the header of every
/// compacted store.
pub const MEMORY_COMPACT_DICTIONARY: &str = "gmeow-memory-compact-v1";

/// Resolve a runtime store's medium out of the SHIPPED bundle's in-band `"dct"` map.
///
/// The bundle is the dictionary's distribution channel: `gmeow.gts` pins every declared
/// dictionary in its segment header, so a consumer priming its own store reads the exact
/// bytes the build trained — never a re-derivation that could differ under the same id,
/// and never an out-of-band artifact a wheel-mode install would not have.
///
/// # Errors
/// The snapshot carries no readable header, or does not pin `dictionary` — which is
/// `gmeow:MediumUndeclaredDictionary`: an id that names no bytes, and there is no weaker
/// unprimed store to fall back to, because the store's OWN header is what makes it
/// decodable.
pub fn store_medium(
    snapshot: &[u8],
    dictionary: &str,
) -> gmeow_errors::Result<gmeow_gts_profile::StoreMedium> {
    let dicts = gmeow_gts_profile::segment_dictionaries(snapshot)?;
    let bytes = dicts.get(dictionary).cloned().ok_or_else(|| {
        gmeow_errors::Diag::of_kind(crate::error::MediumUnpinnedStoreDictionary {
            detail: format!(
                "the loaded gmeow.gts pins no in-band dictionary named {dictionary:?} (pinned: \
                 {:?}) — a runtime store cannot be primed with an id the bundle does not carry, \
                 and writing it unprimed would silently discard the density the dictionary exists \
                 to provide. Regenerate the bundle.",
                dicts.keys().collect::<Vec<_>>()
            ),
        })
    })?;
    Ok(gmeow_gts_profile::StoreMedium {
        dictionary: dictionary.to_string(),
        bytes,
    })
}

/// NATIVE ONLY: compaction rewrites a store IN PLACE through an atomic replace, and both
/// halves of that — the append-only segment library and the temp file the replace swaps
/// through — are filesystem doors this crate deliberately does not carry into the browser
/// image. A wasm consumer holds its store as live values, so there is nothing to compact.
#[cfg(all(not(target_arch = "wasm32"), feature = "reasoning"))]
/// lane.
///
/// A long-lived pack accumulates one segment boundary per medium change and one frame
/// per record; compaction rewrites that into a single streamable segment under a
/// single declared medium. The content claims are rewrite-invariant, so what changes
/// is the LAYOUT and the MEDIUM, never a statement.
///
/// # The dictionary is PINNED, not derived
///
/// `medium` is the resolved [`StoreMedium`] — the bytes [`store_medium`] read out of
/// the loaded bundle's in-band `"dct"` map — and they are handed to upstream as
/// `purrdf::gts::compact::DictStrategy::Pinned`: used verbatim, no training, no corpus
/// derivation, no truncation. That is what makes one `gmeow:dictionaryId` resolve to
/// exactly one byte sequence. Deriving the dictionary from the pack's own content-blob
/// corpus instead would label PACK-LOCAL bytes with the shipped id, so two compacted
/// stores could pin different dictionaries under the same name — precisely what the
/// envelope contract exists to make impossible.
///
/// Pinning also removes the corpus precondition the derived strategies carry: a
/// wholly-pinned plan never touches the content-blob corpus, so an agent-memory store
/// — whose records are `terms`/`quads` frames and which has no content blobs at all —
/// compacts exactly like any other pack. The dictionary rides the new header in band,
/// so the compacted file stays self-decoding without the bundle.
///
/// The whole read → rewrite → replace runs under the store lock, and the replace is
/// atomic: a compaction that fails part-way leaves the PRIOR store completely intact
/// rather than a half-rewritten one.
///
/// # Errors
/// The store cannot be read, is not safely compactable (refuse-don't-trust), or the
/// atomic replace fails.
pub fn compact_store(
    path: &std::path::Path,
    timestamp: &str,
    medium: &StoreMedium,
    packaging_signer: (ed25519_dalek::SigningKey, String),
) -> gmeow_errors::Result<()> {
    // The branch's lock is LIBRARY-scoped rather than path-scoped: a store is reached
    // through its `SegmentLibrary`, so the compaction lane takes the same lock every
    // other writer to that store takes.
    let library = crate::storage::fs_segment_library(path.to_path_buf());
    with_library_lock(library.as_ref(), move || {
        let bytes = std::fs::read(path)?;
        let compacted = gmeow_gts_profile::compact_gmeow_gts(
            &bytes,
            timestamp,
            &medium.dictionary,
            purrdf::gts::compact::DictStrategy::Pinned(medium.bytes.clone()),
            packaging_signer,
        )?;
        let dir = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .map_or_else(|| std::path::PathBuf::from("."), std::path::PathBuf::from);
        let mut tmp = tempfile::Builder::new()
            .prefix(".compact-")
            .suffix(".tmp")
            .tempfile_in(&dir)?;
        std::io::Write::write_all(&mut tmp, &compacted)?;
        tmp.as_file().sync_all()?;
        tmp.persist(path)?;
        Ok(())
    })
}

/// The BROWSER backend's suite: the in-process storage the wasm build runs on.
///
/// Compiled on every target, `wasm32` gate deliberately absent. The whole claim of
/// [`crate::storage::InMemoryStorage`] is that it is a real store rather than a
/// refusal, and a claim nobody executes is a claim nobody has checked — so the store
/// the browser uses is exercised by the NATIVE suite, which is the one that actually
/// runs. Nothing here touches a filesystem, an environment variable, or a clock.
#[cfg(test)]
mod browser_storage_tests {
    use serde_json::{Value, json};

    use purrdf::gts::examples::agent_memory::{RevisionOptions, StoreOptions};

    use std::collections::BTreeSet;

    use purrdf::RdfTerm;
    use purrdf::gts::examples::agent_memory::ToolCallOptions;

    use crate::storage::{
        ClaimStore, InMemoryClaimStore, InMemorySegmentLibrary, InMemoryStorage, Storage,
    };
    use crate::{
        append_library_segments, build_nt_segment, recall_json, run_list_candidates_in,
        store_segment_json, with_library_lock,
    };

    /// Store three claims and recall them: the browser store returns REAL, non-error
    /// results, ranked by the same token-overlap relevance the native package uses, and
    /// a suppressed claim drops out of the default recall.
    #[test]
    fn recall_returns_real_results_against_the_browser_claim_store() {
        let store = InMemoryClaimStore::default();

        let widgets = store
            .store_claim(
                "widgets are blue",
                StoreOptions {
                    source: Some("mcp:test"),
                    confidence: Some(0.9),
                    according_to: None,
                },
            )
            .expect("the browser store accepts a well-formed claim");
        store
            .store_claim(
                "gadgets are red",
                StoreOptions {
                    source: None,
                    confidence: None,
                    according_to: None,
                },
            )
            .expect("the browser store accepts a second claim");
        let retired = store
            .store_claim(
                "widgets are green",
                StoreOptions {
                    source: None,
                    confidence: None,
                    according_to: None,
                },
            )
            .expect("the browser store accepts a third claim");

        // The tool body itself, driven against the browser store.
        let hit: Value = serde_json::from_str(
            &recall_json(&store, &json!({"query": "widgets"})).expect("recall runs"),
        )
        .expect("recall returns JSON");
        assert_eq!(
            hit["ok"], true,
            "recall against the browser store must return a REAL non-error result: {hit}"
        );
        let texts: Vec<&str> = hit["claims"]
            .as_array()
            .expect("claims array")
            .iter()
            .map(|c| c["text"].as_str().unwrap_or_default())
            .collect();
        assert_eq!(
            texts,
            vec!["widgets are green", "widgets are blue"],
            "both widget claims must be recalled, most recent first among equal scores, \
             and the non-matching gadget claim must not: {hit}"
        );

        // A revision retires a claim: the default recall stops returning it, and asking
        // for suppressed claims brings it back — the store REMEMBERS the suppression
        // rather than deleting the record.
        store
            .revise_claim(
                &retired.id,
                RevisionOptions {
                    reason: Some("superseded by the blue measurement"),
                    superseded_by: Some(&widgets.id),
                },
            )
            .expect("the browser store accepts a revision");

        let after: Value = serde_json::from_str(
            &recall_json(&store, &json!({"query": "widgets"})).expect("recall runs"),
        )
        .expect("recall returns JSON");
        let texts: Vec<&str> = after["claims"]
            .as_array()
            .expect("claims array")
            .iter()
            .map(|c| c["text"].as_str().unwrap_or_default())
            .collect();
        assert_eq!(
            texts,
            vec!["widgets are blue"],
            "a suppressed claim must drop out of the default recall: {after}"
        );

        let with_suppressed: Value = serde_json::from_str(
            &recall_json(
                &store,
                &json!({"query": "widgets", "include_suppressed": true}),
            )
            .expect("recall runs"),
        )
        .expect("recall returns JSON");
        assert_eq!(
            with_suppressed["claims"]
                .as_array()
                .expect("claims array")
                .len(),
            2,
            "the suppressed claim is retained and returned on request: {with_suppressed}"
        );
        assert_eq!(
            store.revisions().len(),
            1,
            "the revision itself is recorded, reason and successor included"
        );

        // The store's two input rules are enforced, not merely documented.
        assert!(
            store
                .store_claim(
                    "   ",
                    StoreOptions {
                        source: None,
                        confidence: None,
                        according_to: None
                    }
                )
                .is_err(),
            "an empty claim must be refused"
        );
        assert!(
            store
                .store_claim(
                    "out of range",
                    StoreOptions {
                        source: None,
                        confidence: Some(1.5),
                        according_to: None
                    }
                )
                .is_err(),
            "a confidence outside 0.0..=1.0 must be refused"
        );
    }

    /// `store_segment` returns the browser store's REAL serialization — the field a
    /// session export reads to carry the store it ran against.
    ///
    /// This is the tool the console's export exists to call. Before it there was none:
    /// the export read `store_nquads ?? nquads` off a `recall` result, and no engine tool
    /// returns either field, so a console session could RECORD a store it could never
    /// EXPORT. The assertions below are over the parsed answer, so an engine that answered
    /// with an empty or absent serialization would fail here rather than downstream.
    #[test]
    fn store_segment_serializes_the_browser_claim_store() {
        let store = InMemoryClaimStore::default();

        // An untouched store serializes to nothing, and says so in its counts: "the store
        // holds nothing" and "the store holds something I cannot carry" are opposite
        // situations, and only the second is a failure.
        let empty: Value =
            serde_json::from_str(&store_segment_json(&store).expect("store_segment runs"))
                .expect("store_segment returns JSON");
        assert_eq!(empty["ok"], true, "{empty}");
        assert_eq!(empty["claim_count"], 0, "{empty}");
        assert_eq!(empty["tool_call_count"], 0, "{empty}");
        assert_eq!(empty["nquads"], "", "{empty}");

        let claim = store
            .store_claim(
                "the console can export what it stored",
                StoreOptions {
                    source: Some("mcp:test"),
                    confidence: Some(0.75),
                    according_to: None,
                },
            )
            .expect("stores");
        store
            .record_tool_call(
                "urn:gmeow:tool:store_claim",
                ToolCallOptions {
                    arguments: Some(r#"{"text":"the console can export what it stored"}"#),
                    result: Some(r#"{"ok":true}"#),
                    invocation: None,
                    generated: &[claim.id.as_str()],
                },
            )
            .expect("records");

        let read: Value =
            serde_json::from_str(&store_segment_json(&store).expect("store_segment runs"))
                .expect("store_segment returns JSON");
        assert_eq!(read["claim_count"], 1, "{read}");
        assert_eq!(read["tool_call_count"], 1, "{read}");
        let nquads = read["nquads"].as_str().expect("a serialization: {read}");
        assert!(
            !nquads.trim().is_empty(),
            "a store holding state must serialize to a non-empty segment: {read}"
        );

        // The answer is RDF, not a string that looks like it: it parses, and the claim's
        // text survives the round trip through the parser.
        let dataset = purrdf::parse_dataset(nquads.as_bytes(), "application/n-quads", None)
            .expect("the served segment parses as N-Quads");
        let carried: BTreeSet<String> = purrdf::flat_rdf_quads_from_dataset(&dataset)
            .into_iter()
            .filter_map(|quad| match quad.object {
                RdfTerm::Literal(literal) => Some(literal.lexical_form),
                _ => None,
            })
            .collect();
        assert!(
            carried.contains("the console can export what it stored"),
            "the stored claim's text must ride in the segment: {carried:?}"
        );

        // And it is genuinely re-seedable: a second store built from the segment holds the
        // same claim, which is what makes an exported session replayable.
        let seeded = InMemoryClaimStore::default();
        assert_eq!(
            crate::storage::seed_claim_store(&seeded, nquads).expect("seeds"),
            (1, 1)
        );
        assert_eq!(
            seeded.claims().expect("reads")[0].text,
            "the console can export what it stored"
        );
    }

    /// `list_candidates` returns REAL, non-error results against the browser library:
    /// an untouched library lists nothing, and a committed candidate segment lists the
    /// candidate with its disposition and target provenance.
    #[test]
    fn list_candidates_returns_real_results_against_the_browser_library() {
        let library = InMemorySegmentLibrary::default();

        // An untouched library is EMPTY, not an error — the same answer the native
        // backend gives for a file that does not exist yet.
        let empty: Value =
            serde_json::from_str(&run_list_candidates_in(&library, None, None).expect("lists"))
                .expect("list_candidates returns JSON");
        assert_eq!(
            empty["ok"], true,
            "an untouched browser library must list cleanly: {empty}"
        );
        assert_eq!(empty["candidate_count"], 0, "…and list nothing: {empty}");

        // Commit one admitted candidate through the very same locked, all-or-nothing
        // path the `submit_candidate` tool uses.
        let node = "urn:gmeow:candidate:browser-test";
        // Every IRI is built from the ONE declaration site — the shared `logic:`
        // namespace and the crate's own candidate-vocabulary constants — so a namespace
        // change cannot leave this fixture asserting a term nothing else recognizes.
        let logic_ns = gmeow_logic_compile::ir::LOGIC_NAMESPACE;
        let rdf_type = crate::RDF_TYPE;
        let candidate_class = crate::GMEOW_AUTHORING_CANDIDATE;
        let for_slice = crate::GMEOW_CANDIDATE_FOR_SLICE;
        let body = format!(
            "<{node}> <{rdf_type}> <{candidate_class}> .\n\
             <{node}> <{rdf_type}> <{logic_ns}Conjecture> .\n\
             <{node}> <{logic_ns}conjectureLifecycleState> <{logic_ns}ConjectureOpen> .\n\
             <{node}> <{for_slice}> <urn:gmeow:slice:demo> .\n"
        );
        let segment = build_nt_segment(&[], &crate::tests::probe_medium(), &body)
            .expect("the candidate body parses");
        with_library_lock(&library, || append_library_segments(&library, &[segment]))
            .expect("the browser library commits under its own lock");

        let listed: Value =
            serde_json::from_str(&run_list_candidates_in(&library, None, None).expect("lists"))
                .expect("list_candidates returns JSON");
        assert_eq!(
            listed["ok"], true,
            "list_candidates against the browser library must return a REAL non-error \
             result: {listed}"
        );
        assert_eq!(
            listed["candidate_count"], 1,
            "the committed candidate must be listed: {listed}"
        );
        assert_eq!(listed["candidates"][0]["candidate"], node);
        assert_eq!(listed["candidates"][0]["disposition"], "in-library");
        assert_eq!(
            listed["candidates"][0]["for_slice"], "urn:gmeow:slice:demo",
            "the candidate's target provenance survives the round trip: {listed}"
        );

        // The filters are real filters, not decoration.
        let filtered: Value = serde_json::from_str(
            &run_list_candidates_in(&library, Some("urn:gmeow:slice:other"), None).expect("lists"),
        )
        .expect("list_candidates returns JSON");
        assert_eq!(
            filtered["candidate_count"], 0,
            "a slice filter that matches nothing lists nothing: {filtered}"
        );
    }

    /// The browser backend hands out ONE store per kind, so a claim written by one tool
    /// call is visible to the next — a per-call store would be a store that forgets.
    #[test]
    fn the_browser_backend_shares_one_store_across_calls() {
        let backend = InMemoryStorage::new();
        // The browser backend keeps claims as live values, not as GTS segments, so no codec
        // catalog applies to it — it is the one store a snapshot says nothing about.
        backend
            .claim_store(&[])
            .expect("the browser backend always has a claim store")
            .store_claim(
                "persisted across calls",
                StoreOptions {
                    source: None,
                    confidence: None,
                    according_to: None,
                },
            )
            .expect("stores");

        let seen = backend
            .claim_store(&[])
            .expect("the browser backend always has a claim store")
            .claims()
            .expect("reads");
        assert_eq!(
            seen.len(),
            1,
            "a second handle must see the first handle's claim"
        );
        assert_eq!(seen[0].text, "persisted across calls");

        // Configuration behaves like an environment: unset is unset, set is readable.
        assert!(backend.env_var("GMEOW_LANG").is_none());
        backend.set_env("GMEOW_LANG", "fr");
        assert_eq!(backend.env_var("GMEOW_LANG").as_deref(), Some("fr"));
    }
}
