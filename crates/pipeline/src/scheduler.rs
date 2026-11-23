// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The completion-driven scheduler + the [`RunContext`] (P2).
//!
//! Every stage becomes runnable as soon as its own declared producers publish,
//! without waiting for unrelated siblings in the same topological level. A stage that declares a
//! shared resource (`gmeow:requiresResource`, e.g. the reasoning stage's
//! [`crate::node::ENGINE_RESOURCE`]) holds it exclusively while it runs, so two
//! stages competing for the same resource serialize — the declarative
//! replacement for a hardcoded engine mutex. A context may opt focused stages into
//! the content-addressed [`PipelineCache`]; the final result is
//! keyed by stage id (a `BTreeMap`) and folded into one order-independent
//! `combined_digest`, so a run is byte-identical regardless of completion order
//! — the determinism the P2 tests pin.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::io::Read as _;
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock, Mutex, mpsc};
use std::time::Duration;

use crate::bundle::set_bundle_provenance;
use crate::cache::{
    PipelineCache, RawInputDigest, ReceiptOutputSelection, StageInputDigest, StageKeyContext,
    StageReceipt, content_digest, content_store_commitment, content_store_keys,
    default_graph_commitment, default_graph_keys, provenance_commitment, provenance_keys,
};
use crate::graph::StageGraph;
use crate::memory_record::{MemoryEvent, MemoryRecord, ProcessMemory};
use crate::node::{CachePolicy, Stage, StageInput, StageProduct, StageRunTiming, StageStability};
use crate::provenance::register_stage_unit;
use gmeow_cli_core::Reporter;
use purrdf::provenance::DatasetProvenance;

#[path = "scheduler_order.rs"]
mod ready_order;
pub(crate) use ready_order::ReadyOrder;

/// The process-wide registry of per-resource mutexes. A stage that declares a
/// `gmeow:requiresResource` acquires that resource's permit before running, so two
/// stages competing for the same resource serialize. A permit, not data — results
/// are returned, never stored behind the lock. The engine resource
/// ([`crate::node::ENGINE_RESOURCE`]) mirrors the `CHASE_LOCK` in `gmeow-logic`
/// (the process-wide reasoning state is exclusive); any other shared resource a
/// stage declares serializes the same way, with no new special case.
static RESOURCE_LOCKS: LazyLock<Mutex<HashMap<String, Arc<Mutex<()>>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// The permit mutex for `resource`, created on first request. The registry mutex is
/// held only to look up / insert the `Arc`, never across a stage's execution.
fn resource_lock(resource: &str) -> Arc<Mutex<()>> {
    let mut registry = RESOURCE_LOCKS
        .lock()
        .expect("resource-lock registry poisoned");
    Arc::clone(
        registry
            .entry(resource.to_string())
            .or_insert_with(|| Arc::new(Mutex::new(()))),
    )
}

/// Admits one return of freed pages at a time in this process. A permit, not data.
static RECLAIM_IN_PROGRESS: Mutex<()> = Mutex::new(());

/// Return wholly free allocator pages to the system and record what that gave back.
///
/// Dropping a stage's owners makes their allocations unreachable, but glibc may
/// retain their pages in process arenas. The scheduler calls this once after each
/// stage's publication, and the reasoning stage calls it at the handoffs inside its
/// body, naming the phase that just ended. The GNU hint scans all arenas for whole
/// free pages; fragmented free bytes may remain resident. `malloc_trim` is MT-Safe,
/// including when sibling stages still allocate, although it holds each arena's lock
/// in turn while it walks that arena, so it can delay a thread allocating there. It
/// cannot change a product, cache key, or stage order. Allocators without this
/// interface retain ordinary drop behavior, and the record then shows two equal
/// resident sizes.
///
/// The return is process-wide, so two at once would walk the same arenas for the same
/// pages. Callers therefore take turns, which keeps one return's two resident readings
/// clear of another return. The waiters are bounded by the callers that exist: one coordinator for
/// each pipeline run in the process and each reasoning body in flight, never one per
/// worker.
///
/// The returned record is an observation: resident size immediately before and after,
/// and the time the return took. Other threads go on allocating and freeing between
/// the two readings, so their difference is what the return gave back net of that
/// activity, and it can be small or negative beside a busy stage. See
/// [`crate::memory_record`].
pub(crate) fn reclaim_allocator_slack(stage_id: &str, phase: Option<&str>) -> MemoryRecord {
    let _turn = RECLAIM_IN_PROGRESS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let rss_before_kib = ProcessMemory::sample().rss_kib;
    let started = std::time::Instant::now();
    return_free_allocator_pages();
    crate::memory_record::reclaimed(stage_id, phase, rss_before_kib, started.elapsed())
}

#[cfg(all(target_os = "linux", target_env = "gnu"))]
fn return_free_allocator_pages() {
    // SAFETY: glibc documents `malloc_trim` as MT-Safe. The argument merely requests
    // release of whole free pages in all arenas; no live Rust allocation is exposed
    // or accessed. Returning zero is a valid result when no pages can be returned.
    unsafe {
        libc::malloc_trim(0);
    }
}

#[cfg(not(all(target_os = "linux", target_env = "gnu")))]
fn return_free_allocator_pages() {}

/// What the scheduler does with a stage's carrier once its LAST declared carrier
/// consumer has run and no artifact-lane view of it is in flight.
///
/// This is an explicit, first-class profile selection, not a degradation switch: the
/// run's `combined_digest` and every product's committed byte-artifact lane are
/// byte-identical under both arms (`crate::tests::carrier_retention_is_bounded_by_the_live_frontier`
/// pins that),
/// because a released product keeps its `stage_id` and `digest` verbatim and keeps
/// every committed artifact. The arms differ ONLY in whether material that no declared
/// consumer can still read stays resident for the life of the [`RunResult`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CarrierRetention {
    /// Release each stage's carrier when its exact remaining carrier-reader count
    /// reaches zero and no artifact-lane view of it is in flight (a running consumer
    /// whose edge declares no carrier read shares the carrier by reference): the
    /// frozen dataset, typed handles, blob records, provenance, and the internal
    /// `pipeline/` byte artifacts no consumer declares are then freed, bounding peak
    /// residency to the live frontier plus every run OUTPUT. THE profile for the
    /// whole-repository build
    /// ([`crate::run::run_full_scoped_with_progress`]), whose reconcile reads only
    /// committed artifacts.
    ///
    /// Under this profile a post-run reader of an INTERMEDIATE product's dataset,
    /// handles, or `pipeline/` artifacts HARD-fails on
    /// [`StageProduct::carrier_released`] — it is reaching past the declared dataflow,
    /// exactly as a post-drop `span_index()` read does.
    DropAfterLastConsumer,
    /// Retain every stage's full carrier for the life of the [`RunResult`].
    ///
    /// Required by the out-of-band whole-run consumers that read an INTERMEDIATE
    /// product's carrier after the DAG finishes — [`crate::docs_measure`] takes the
    /// terminal carrier off `stage-snapshot`'s bundle, which has declared consumers and
    /// would otherwise be released — and by tests that assert on a consumed stage's
    /// dataset.
    RetainAll,
}

/// The shared state of one pipeline run: the repo root, the parallelism budget,
/// the optional content-addressed cache, live progress sink, and the provenance
/// sidecar stages stamp into.
pub struct RunContext {
    /// The repository root the build operates over.
    pub root: PathBuf,
    /// Maximum concurrent stages across the ready frontier (rayon pool size).
    pub jobs: usize,
    /// The persistent, self-verifying per-stage cache.
    pub cache: PipelineCache,
    /// Whether stage cache reads and writes are enabled for this run.
    pub stage_cache_enabled: bool,
    /// Whether deterministic stage receipts are produced. It is disabled together
    /// with the legacy full-run inert boundary so this foundational change cannot add
    /// receipt work to that path before bounded RDF admission is enabled.
    pub stage_receipts_enabled: bool,
    /// Optional live stage-progress sink. Absent by default; `sync --verbose`
    /// supplies one explicitly.
    pub progress: Option<Arc<dyn Reporter>>,
    /// The provenance sidecar: one unit per stage (capability-derived origin).
    pub provenance: DatasetProvenance,
    /// What happens to a stage's carrier once its last declared consumer has run.
    ///
    /// [`CarrierRetention::RetainAll`] on every constructor — the conservative identity
    /// — so a caller that reads an intermediate carrier post-run keeps working until it
    /// explicitly opts in. Whole-repository callers select
    /// [`CarrierRetention::DropAfterLastConsumer`] and declare their post-run inputs.
    pub carrier_retention: CarrierRetention,
    /// Intermediate carriers an explicit out-of-band caller will read after the DAG.
    ///
    /// This is a narrow keep-set layered over [`CarrierRetention::DropAfterLastConsumer`],
    /// not a second scheduling graph: every id must already be an executed production
    /// stage in the selected run. It lets a whole-bundle proof retain `stage-snapshot`
    /// (or the terminal's exact carrier inputs) without retaining dozens of unrelated
    /// cumulative products.
    pub retained_carriers: BTreeSet<String>,
    /// Exact (stage, artifact) observations required by post-run reconciliation.
    /// The scheduler copies only these bytes before releasing their carriers. A
    /// missing stage or artifact is an error; selection never changes execution.
    pub retained_artifacts: BTreeSet<(String, String)>,
    /// Inputs borrowed from the run products by a reader outside the DAG. Internal
    /// byte artifacts stay in the producer's released product.
    post_run_inputs: BTreeMap<String, crate::reads::ReadSet>,
    /// What the run does when a stage's recorded reads differ from the reads its
    /// dataflow edges declare. Every context starts at
    /// [`ReadPolicy::Enforce`](crate::reads::ReadPolicy::Enforce); only the maintainer
    /// command that writes observed read sets to a file selects `Record`.
    pub read_policy: crate::reads::ReadPolicy,
    /// Deterministic preference for the dependency-ready, eligible frontier.
    pub(crate) ready_order: ReadyOrder,
    /// RAII owner of an ephemeral cache directory, when this context was built by
    /// [`Self::open_ephemeral`]. Holding the [`tempfile::TempDir`] here — rather than
    /// leaking a pid-salted path under the system temp dir — is what makes the cache
    /// die with the run that created it, on success, on error, and on panic. `None`
    /// for the persistent ([`Self::open`]) and inert ([`Self::open_uncached`])
    /// boundaries, which own no temporary directory.
    _ephemeral_cache_dir: Option<tempfile::TempDir>,
}

impl RunContext {
    /// Construct a run context rooted at `root` with `jobs` parallelism, opening the
    /// persistent cache under the shared `.cache/gmeow-sync/actions/` authority.
    /// Every action key embeds the producer fingerprint, so code/dependency/toolchain
    /// changes cannot serve an older executable's product; the common store owns the
    /// bounded receipt/blob quota across pipeline and documentation actions.
    pub fn open(root: impl Into<PathBuf>, jobs: usize) -> Result<Self, gmeow_errors::Diag> {
        let root = root.into();
        let cache = PipelineCache::open_default(&root)?;
        Ok(Self {
            root,
            jobs: jobs.max(1),
            cache,
            stage_cache_enabled: true,
            stage_receipts_enabled: true,
            progress: None,
            provenance: DatasetProvenance::new(),
            carrier_retention: CarrierRetention::RetainAll,
            retained_carriers: BTreeSet::new(),
            retained_artifacts: BTreeSet::new(),
            post_run_inputs: BTreeMap::new(),
            read_policy: crate::reads::ReadPolicy::Enforce,
            ready_order: ReadyOrder::default(),
            // Persistent boundary: the cache lives under the repo's `.cache/`, not a
            // temporary directory, so there is nothing to tear down.
            _ephemeral_cache_dir: None,
        })
    }

    /// Construct a run context whose cache lives in a FRESH, run-unique temp
    /// directory rather than the persistent `.cache/gmeow-sync/actions/`.
    ///
    /// Used by tests that want a clean, isolated cache per run (no cross-test or
    /// cross-invocation reuse). The full build ([`crate::run::run_full`]) uses the
    /// persistent boundary with DAG-declared admission; its cumulative aggregates are
    /// explicitly recompute-only.
    ///
    /// The directory is a [`tempfile::TempDir`] OWNED by the returned context, so it
    /// is removed when the run ends — including on error and on panic. It is
    /// deliberately not a pid-salted path the process merely happens to know about:
    /// a fresh name per invocation with no owner is an unbounded disk leak, which is
    /// exactly what this boundary used to be.
    pub fn open_ephemeral(
        root: impl Into<PathBuf>,
        jobs: usize,
    ) -> Result<Self, gmeow_errors::Diag> {
        let root = root.into();
        // `TempDir` supplies the uniqueness the old pid+nanosecond salt hand-rolled,
        // and — unlike that salt — it also supplies the teardown.
        let dir = tempfile::tempdir()?;
        let cache = PipelineCache::open(dir.path())?;
        Ok(Self {
            root,
            jobs: jobs.max(1),
            cache,
            stage_cache_enabled: true,
            stage_receipts_enabled: true,
            progress: None,
            provenance: DatasetProvenance::new(),
            carrier_retention: CarrierRetention::RetainAll,
            retained_carriers: BTreeSet::new(),
            retained_artifacts: BTreeSet::new(),
            post_run_inputs: BTreeMap::new(),
            read_policy: crate::reads::ReadPolicy::Enforce,
            ready_order: ReadyOrder::default(),
            _ephemeral_cache_dir: Some(dir),
        })
    }

    /// Construct a context with no per-stage cache I/O.
    ///
    /// This remains an explicit diagnostic/test boundary. Full repository
    /// synchronization uses [`Self::open`] so independently bounded contributions can
    /// be reused; cumulative aggregates stay out through their DAG declarations.
    pub fn open_uncached(root: impl Into<PathBuf>, jobs: usize) -> Self {
        Self {
            root: root.into(),
            jobs: jobs.max(1),
            cache: PipelineCache::inert(),
            stage_cache_enabled: false,
            stage_receipts_enabled: false,
            progress: None,
            provenance: DatasetProvenance::new(),
            carrier_retention: CarrierRetention::RetainAll,
            retained_carriers: BTreeSet::new(),
            retained_artifacts: BTreeSet::new(),
            post_run_inputs: BTreeMap::new(),
            read_policy: crate::reads::ReadPolicy::Enforce,
            ready_order: ReadyOrder::default(),
            // Inert boundary: no cache I/O at all, so no temporary directory.
            _ephemeral_cache_dir: None,
        }
    }

    /// Attach a live stage-progress reporter to this run.
    pub fn with_progress(mut self, progress: Arc<dyn Reporter>) -> Self {
        self.progress = Some(progress);
        self
    }

    /// Preserve selected intermediate carriers for a declared post-run reader while
    /// releasing every other dead carrier normally.
    pub fn retain_carriers<I, S>(&mut self, stage_ids: I)
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.retained_carriers
            .extend(stage_ids.into_iter().map(Into::into));
    }

    pub(crate) fn retain_post_run_inputs(
        &mut self,
        inputs: impl IntoIterator<Item = (String, crate::reads::ReadSet)>,
    ) {
        for (producer, reads) in inputs {
            if crate::reads::needs_carrier(&reads) {
                self.retained_carriers.insert(producer.clone());
            }
            self.post_run_inputs
                .entry(producer)
                .or_default()
                .extend(reads);
        }
    }
}

/// The result of a pipeline run: every stage's product plus the order-independent
/// digest folding them all (the determinism witness).
///
/// `StageProduct`'s carrier (`Arc<PipelineBundle>`) has no value equality, so this
/// no longer derives `PartialEq`/`Eq`; the determinism witness is the
/// `combined_digest` (a `String`), which tests compare directly (C4).
#[derive(Debug, Clone)]
pub struct RunResult {
    /// Width of the Rayon pool that executed the stages.
    pub(crate) rayon_workers: usize,
    /// Each stage's product, keyed by stage id (sorted).
    pub products: BTreeMap<String, StageProduct>,
    /// Selected observations surviving carrier release, keyed by (stage, path).
    pub retained_artifacts: BTreeMap<(String, String), Vec<u8>>,
    /// The hex SHA-256 over the sorted `(id, product-digest)` pairs.
    pub combined_digest: String,
    /// Per-stage wall-clock timings in deterministic topological order.
    pub stage_timings: Vec<StageTiming>,
    /// Internal phase timings emitted by freshly executed stages.
    pub stage_phase_timings: Vec<StagePhaseTiming>,
    /// Observed per-level critical-stage timings in topological level order. Levels
    /// remain a reporting dimension; they are not execution barriers.
    pub level_timings: Vec<LevelTiming>,
    /// Deterministic stage receipts in topological commit order.
    pub stage_receipts: Vec<StageReceipt>,
    /// Content root over `(stage id, receipt digest)` in that same order.
    pub receipt_root: String,
    /// The run-level diagnostic ledger: the FORWARD fold of every stage's emitted
    /// `DiagNode`s (each producer's report findings, projected once). It is built by
    /// replaying each stage's `diags` (fresh run) or its cache-restored
    /// `diagnostics:nodes` blob (cache hit) into one hash-consed ledger; `replay` +
    /// content-addressed fingerprints make a warm-cache run byte-identical to a cold
    /// one regardless of worker-completion order. `run_full` threads this into
    /// `RunReport.ledger` and attaches its own run-level reconcile findings to it.
    pub ledger: gmeow_errors::DiagLedger,
    /// Every dataflow edge a stage body read through in this run, with its declared and
    /// its recorded reads, in completion order. Cache hits contribute nothing.
    pub recorded_reads: Vec<Arc<crate::reads::EdgeAccess>>,
    /// The memory records of this run in the order they were made: one at each stage's
    /// dispatch, publication and carrier release, one for the return of freed allocator
    /// pages after each publication, and the measured phase boundaries that
    /// were crossed on a thread running a stage body. A boundary crossed on a pool thread
    /// that was running no stage body is not kept here; it reaches the log when the
    /// `pipeline_memory` target is on. Observations only:
    /// they enter no product, key, receipt or digest. See [`crate::memory_record`].
    pub memory: Vec<MemoryRecord>,
}

/// One stage's wall-clock timing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StageTiming {
    /// The stage's topological reporting level; execution is completion-driven.
    pub level: usize,
    /// The stage identifier.
    pub stage_id: String,
    /// Wall-clock spent in `exec_stage`: cache probe/hydration or compute.
    pub elapsed_ms: u128,
    /// Whether the product came from the persistent stage cache.
    pub cached: bool,
    /// Stable observational outcome label (`hit`, `miss:not-found`, or an explicit
    /// bypass reason). This is telemetry and never part of a receipt.
    pub cache_outcome: String,
    /// Serialized bytes read while hydrating a hit.
    pub cache_read_bytes: u64,
    /// Serialized bytes published for a cold persistent result.
    pub cache_write_bytes: u64,
    /// Change in this process's resident set across a verified cache hydration.
    ///
    /// This is observational and absent on an execution/miss. A `jobs=1` census gives
    /// the isolated per-stage value used for admission economics; in a parallel wave it
    /// remains useful context but may include concurrent sibling allocation.
    pub cache_hydration_rss_delta_kib: Option<i64>,
}

/// One internal phase timing qualified by its producing stage.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StagePhaseTiming {
    /// The stage that emitted the timing.
    pub stage_id: String,
    /// Stable phase name local to that stage.
    pub phase: String,
    /// Observed wall-clock duration in milliseconds.
    pub elapsed_ms: u128,
    /// Optional stable work metadata supplied by the stage.
    pub metadata: Option<String>,
}

/// The slowest observed stage assigned to one topological reporting level.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LevelTiming {
    /// The topological level index.
    pub level: usize,
    /// Wall-clock for the slowest stage in the level.
    pub elapsed_ms: u128,
    /// The slowest stage in this level.
    pub critical_stage: String,
}

/// One stage's execution outcome, carrying the cache key so a freshly-computed
/// product can be persisted immediately when the worker completes.
struct StageRun {
    id: String,
    key_context: StageKeyContext,
    product: StageProduct,
    cached: bool,
    /// Wall-clock spent in [`exec_stage`] for this stage (compute + cache probe).
    elapsed_ms: u128,
    /// This stage's FORWARD-projected diagnostic nodes: `out.diags` on a fresh run,
    /// or the cache-restored `diagnostics:nodes` blob on a hit. Replayed into the
    /// run-wide ledger in the sequential commit phase.
    diags: Vec<gmeow_errors::DiagNode>,
    /// Internal phase telemetry from a cache-miss execution.
    timings: Vec<StageRunTiming>,
    receipt: Option<StageReceipt>,
    output_selection: ReceiptOutputSelection,
    cache_outcome: String,
    cache_read_bytes: u64,
    cache_write_bytes: u64,
    cache_hydration_rss_delta_kib: Option<i64>,
    /// The dataflow edges the stage body read through, with what it read. Empty on a
    /// cache hit: no stage body ran, so there is nothing to compare.
    edges: Vec<Arc<crate::reads::EdgeAccess>>,
}

/// Run a validated, bound pipeline. `bound` is the stages in topological order
/// (from [`crate::loader::bind`]); `graph` provides the parallel levels.
pub fn run(
    graph: &StageGraph,
    bound: &[Arc<dyn Stage>],
    ctx: &mut RunContext,
) -> Result<RunResult, gmeow_errors::Diag> {
    run_without(graph, bound, ctx, &BTreeSet::new())
}

/// Compute the exact transitive producer closure required to build `targets`.
///
/// The returned set includes every target and every stage reachable through its
/// declared [`Stage::consumes`] edges. An unknown target hard-fails; an empty target
/// set is not a selected operation and therefore also hard-fails.
pub fn dependency_closure(
    bound: &[Arc<dyn Stage>],
    targets: &BTreeSet<String>,
) -> Result<BTreeSet<String>, gmeow_errors::Diag> {
    if targets.is_empty() {
        return Err(gmeow_errors::Diag::of_kind(crate::error::StageFailed {
            cause: None,
            stage: "<scheduler>".to_string(),
            message: "dependency closure requires at least one target stage".to_string(),
        }));
    }
    let by_id: BTreeMap<&str, &Arc<dyn Stage>> =
        bound.iter().map(|stage| (stage.id(), stage)).collect();
    let mut closure = BTreeSet::new();
    let mut pending: Vec<String> = targets.iter().cloned().collect();
    while let Some(stage_id) = pending.pop() {
        if !closure.insert(stage_id.clone()) {
            continue;
        }
        let stage = by_id.get(stage_id.as_str()).ok_or_else(|| {
            gmeow_errors::Diag::of_kind(crate::error::StageFailed {
                cause: None,
                stage: "<scheduler>".to_string(),
                message: format!("dependency closure names unknown target stage {stage_id}"),
            })
        })?;
        pending.extend(stage.consumes().iter().cloned());
    }
    Ok(closure)
}

/// Run exactly the dependency closure needed to produce `targets`.
///
/// This is the fixture/partial-DAG counterpart to [`run`]. It uses the same bound
/// stages, scheduler action keys, receipts, cache dispositions, resource permits, and
/// attach checks; only stages outside the mechanically derived ancestor closure are
/// omitted.
pub fn run_targets(
    graph: &StageGraph,
    bound: &[Arc<dyn Stage>],
    ctx: &mut RunContext,
    targets: &BTreeSet<String>,
) -> Result<RunResult, gmeow_errors::Diag> {
    let closure = dependency_closure(bound, targets)?;
    let skip: BTreeSet<String> = graph
        .order()
        .into_iter()
        .filter(|stage| !closure.contains(stage))
        .collect();
    run_without(graph, bound, ctx, &skip)
}

/// Run the pipeline with a DECLARED set of stages omitted, returning the products of
/// everything else.
///
/// It exists for exactly one caller and the reason is structural, not a convenience:
/// the off-gate medium sweep (`make maint-medium-sweep`) must MEASURE the frames the
/// terminal would write, and the terminal is precisely the stage that REFUSES to write
/// them when a dictionary does not pay for itself. A sweep that ran the whole graph
/// could therefore never produce the evidence a human needs in order to fix that — the
/// gate would eat its own diagnosis. Omitting the terminal is not a weaker run: every
/// stage the sweep reads a product from still runs, under the same cache and the same
/// fail-closed rules, and the omitted stage produces no product for anything else to
/// consume (it is the graph's sink).
///
/// `skip` is a set of stage ids. A stage that some REMAINING stage consumes is a hard
/// fail rather than a silently smaller run: it would leave that consumer to fail deep
/// inside its own logic with no statement of why.
///
/// # Errors
/// Any stage failure, or a `skip` entry some remaining stage consumes.
pub fn run_without(
    graph: &StageGraph,
    bound: &[Arc<dyn Stage>],
    ctx: &mut RunContext,
    skip: &BTreeSet<String>,
) -> Result<RunResult, gmeow_errors::Diag> {
    let by_id: BTreeMap<&str, &Arc<dyn Stage>> = bound.iter().map(|s| (s.id(), s)).collect();
    for retained in ctx
        .retained_carriers
        .iter()
        .chain(ctx.retained_artifacts.iter().map(|(stage, _)| stage))
        .chain(ctx.post_run_inputs.keys())
    {
        if !by_id.contains_key(retained.as_str()) || skip.contains(retained) {
            return Err(gmeow_errors::Diag::of_kind(crate::error::StageFailed {
                cause: None,
                stage: "<scheduler>".to_string(),
                message: format!(
                    "retained product `{retained}` does not name an executed production stage"
                ),
            }));
        }
    }
    for stage in bound {
        if skip.contains(stage.id()) {
            continue;
        }
        for consumed in stage.consumes() {
            if skip.contains(consumed) {
                return Err(gmeow_errors::Diag::of_kind(crate::error::StageFailed {
                    cause: None,
                    stage: stage.id().to_string(),
                    message: format!(
                        "cannot omit `{consumed}`: `{}` consumes it, so omitting it would leave a \
                         consumer to fail inside its own logic instead of here, where the reason \
                         is nameable",
                        stage.id()
                    ),
                }));
            }
        }
    }

    // A local rayon pool honours the jobs budget without touching the global one.
    // The coordinator publishes each completed action immediately and releases the
    // exact dependents it satisfies; topological levels remain telemetry only.
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(ctx.jobs)
        .build()
        .map_err(|e| {
            gmeow_errors::Diag::of_kind(crate::error::StageFailed {
                cause: None,
                stage: "<scheduler>".to_string(),
                message: format!("failed to build rayon pool: {e}"),
            })
        })?;

    let mut products: BTreeMap<String, StageProduct> = BTreeMap::new();
    let mut retained_artifacts = BTreeMap::new();
    let profile = std::env::var_os("GMEOW_PIPELINE_TIMING").is_some();
    let mut stage_timings: Vec<StageTiming> = Vec::new();
    let mut stage_phase_timings: Vec<StagePhaseTiming> = Vec::new();
    let mut level_timings: Vec<LevelTiming> = graph
        .levels
        .iter()
        .enumerate()
        .map(|(level, _)| LevelTiming {
            level,
            elapsed_ms: 0,
            critical_stage: String::new(),
        })
        .collect();
    let mut stage_receipts: Vec<StageReceipt> = Vec::new();
    let mut run_ledger = gmeow_errors::DiagLedger::new();

    let level_by_stage: BTreeMap<&str, usize> = graph
        .levels
        .iter()
        .enumerate()
        .flat_map(|(level, stages)| stages.iter().map(move |stage| (stage.as_str(), level)))
        .collect();
    let order = graph.order();
    let order_index: BTreeMap<&str, usize> = order
        .iter()
        .enumerate()
        .map(|(index, stage)| (stage.as_str(), index))
        .collect();
    let executed: BTreeSet<String> = order
        .iter()
        .filter(|stage| !skip.contains(stage.as_str()))
        .cloned()
        .collect();
    if executed.is_empty() {
        return Err(gmeow_errors::Diag::of_kind(crate::error::StageFailed {
            cause: None,
            stage: "<scheduler>".to_string(),
            message: "selected pipeline executes no stages".to_string(),
        }));
    }

    // Readiness is an exact count over the selected graph. `dependents` is sorted so
    // simultaneous publications produce a stable ready frontier independently of worker
    // completion order.
    let mut remaining_dependencies: BTreeMap<String, usize> = BTreeMap::new();
    let mut dependents: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut logically_consumed: BTreeSet<String> = ctx.post_run_inputs.keys().cloned().collect();
    let mut remaining_carrier_readers: BTreeMap<String, usize> = BTreeMap::new();
    for stage_id in &executed {
        let stage = by_id[stage_id.as_str()];
        let dependencies = stage
            .consumes()
            .iter()
            .filter(|producer| executed.contains(producer.as_str()))
            .count();
        remaining_dependencies.insert(stage_id.clone(), dependencies);
        for producer in stage
            .consumes()
            .iter()
            .filter(|producer| executed.contains(producer.as_str()))
        {
            dependents
                .entry(producer.clone())
                .or_default()
                .insert(stage_id.clone());
            logically_consumed.insert(producer.clone());
        }
        for producer in stage
            .carrier_consumes()
            .iter()
            .filter(|producer| executed.contains(producer.as_str()))
        {
            *remaining_carrier_readers
                .entry(producer.clone())
                .or_default() += 1;
        }
    }
    let declared_level_frontier = last_consumer_levels(graph, &by_id);
    ctx.ready_order.validate(&executed)?;
    let depth = if matches!(ctx.ready_order, ReadyOrder::Memory(_)) {
        ready_order::critical_depth(graph, &dependents)
    } else {
        BTreeMap::new()
    };
    let mut dataset_bytes = BTreeMap::new();
    // The internal artifacts each producer must keep through every release, because a
    // consumer declared it reads them.
    let mut kept_internal: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for stage_id in &executed {
        for (producer, artifact) in by_id[stage_id.as_str()].internal_artifact_reads() {
            kept_internal.entry(producer).or_default().insert(artifact);
        }
    }
    for (producer, reads) in &ctx.post_run_inputs {
        kept_internal
            .entry(producer.clone())
            .or_default()
            .extend(crate::reads::internal_artifacts(reads).map(str::to_owned));
    }
    // The same names, shared by every artifact-lane view of that producer.
    let kept_shared: BTreeMap<String, Arc<BTreeSet<String>>> = kept_internal
        .iter()
        .map(|(producer, kept)| (producer.clone(), Arc::new(kept.clone())))
        .collect();
    // The artifact-lane views of each producer's product that are in flight. A producer's
    // carrier is not swapped for its residue while one is, so no second copy of the lane
    // sits beside the one the view reads.
    let mut artifact_views_in_flight: BTreeMap<String, usize> = BTreeMap::new();
    let kept_for = |producer: &str| -> BTreeSet<&str> {
        kept_internal
            .get(producer)
            .map(|kept| kept.iter().map(String::as_str).collect())
            .unwrap_or_default()
    };
    let read_policy = ctx.read_policy;
    let release_payloads = read_policy == crate::reads::ReadPolicy::Enforce
        && ctx.carrier_retention == CarrierRetention::DropAfterLastConsumer;
    let mut remaining_payload_readers = BTreeMap::<String, PayloadReaders>::new();
    let mut producers_with_persistent_readers = BTreeSet::new();
    let mut recorded_edges: Vec<Arc<crate::reads::EdgeAccess>> = Vec::new();
    let mut memory: Vec<MemoryRecord> = Vec::new();
    // Every stage whose recorded reads differ from its declared dataflow. The run goes
    // on, so one pass names them all, and then fails.
    let mut read_differences: Vec<(String, Vec<String>)> = Vec::new();
    // Before any stage runs: every dependency edge of the selected graph declares what
    // it reads. An edge with no declared read would run the consumer after the producer
    // for no stated reason. Every such edge is reported together.
    if read_policy == crate::reads::ReadPolicy::Enforce {
        let mut undeclared: Vec<String> = Vec::new();
        for stage_id in &executed {
            let stage = by_id[stage_id.as_str()];
            let reads = stage.reads();
            for producer in stage.consumes() {
                if reads.get(producer).is_none_or(BTreeSet::is_empty) {
                    undeclared.push(format!(
                        "{stage_id} consumes {producer} and its dataflow edge declares no read"
                    ));
                }
            }
            if release_payloads {
                if stage.cache_policy() == CachePolicy::Persistent {
                    producers_with_persistent_readers
                        .extend(stage.carrier_consumes().iter().cloned());
                }
                for (producer, reads) in reads {
                    remaining_payload_readers
                        .entry(producer)
                        .or_default()
                        .declare(reads);
                }
            }
        }
        if !undeclared.is_empty() {
            return Err(gmeow_errors::Diag::of_kind(crate::error::InvalidDag {
                message: format!(
                    "the pipeline description does not say what {} dependency edge(s) read; \
                     run `gmeow-dev pipeline-reads --out <file>` to write the declarations \
                     the stage bodies need:\n  {}",
                    undeclared.len(),
                    undeclared.join("\n  ")
                ),
            }));
        }
    }
    let mut ready: BTreeSet<String> = remaining_dependencies
        .iter()
        .filter(|(_, remaining)| **remaining == 0)
        .map(|(stage, _)| stage.clone())
        .collect();

    let root: &Path = &ctx.root;
    let cache = &ctx.cache;
    let stage_cache_enabled = ctx.stage_cache_enabled;
    let stage_receipts_enabled = ctx.stage_receipts_enabled;
    let progress = ctx.progress.clone();
    let provenance = &mut ctx.provenance;
    let retained_carriers = &ctx.retained_carriers;
    let retained_artifact_selection = &ctx.retained_artifacts;
    let carrier_retention = ctx.carrier_retention;
    let ready_order = &ctx.ready_order;
    let rayon_workers = pool.current_num_threads();
    let stage_limit = ready_order.stage_limit(rayon_workers);

    let scheduling_result = pool.in_place_scope_fifo(|scope| -> Result<(), gmeow_errors::Diag> {
        let (tx, rx) = mpsc::channel::<(
            String,
            Vec<String>,
            Result<StageRun, gmeow_errors::Diag>,
            Vec<Arc<crate::reads::EdgeAccess>>,
            Vec<MemoryRecord>,
        )>();
        let mut active = 0usize;
        let mut completed = 0usize;
        let mut active_resources = BTreeSet::new();
        let mut first_error: Option<gmeow_errors::Diag> = None;

        while completed < executed.len() || active > 0 {
            // Fill the available stage slots from the dependency-ready frontier. A locally active
            // declared resource is excluded before dispatch, so a blocked resource user
            // never occupies a worker while unrelated ready work exists. The process-wide
            // locks in `exec_stage` remain authoritative across concurrent pipeline runs.
            while first_error.is_none() && active < stage_limit {
                let mut eligible = ready.iter().filter(|stage_id| {
                    by_id[stage_id.as_str()]
                        .resources()
                        .iter()
                        .all(|resource| !active_resources.contains(resource))
                });
                let next = match ready_order {
                    ReadyOrder::StageId | ReadyOrder::Calibrate => eligible.next(),
                    ReadyOrder::Memory(estimates) => {
                        let live = ready_order::LiveReads {
                            products: &products,
                            dataset_bytes: &dataset_bytes,
                            carrier_readers: &remaining_carrier_readers,
                            payload_readers: release_payloads.then_some(&remaining_payload_readers),
                            artifact_views: &artifact_views_in_flight,
                            retained: retained_carriers,
                        };
                        // `min_by_key` returns the first equal item in the sorted set.
                        eligible.min_by_key(|id| {
                            let terminal_only = dependents.get(*id).is_some_and(|consumers| {
                                consumers.len() == 1
                                    && consumers.iter().all(|consumer| {
                                        by_id[consumer.as_str()].capabilities().iter().any(
                                            |capability| capability == crate::node::SINK_CAPABILITY,
                                        )
                                    })
                            });
                            live.priority(
                                by_id[id.as_str()].as_ref(),
                                estimates.as_ref().and_then(|estimates| estimates.get(*id)),
                                terminal_only,
                                depth[*id],
                            )
                        })
                    }
                }
                .cloned();
                let Some(stage_id) = next else { break };
                ready.remove(&stage_id);
                let stage = Arc::clone(by_id[stage_id.as_str()]);
                let resources = stage.resources().to_vec();
                active_resources.extend(resources.iter().cloned());
                let lane_producers = artifact_lane_producers(stage.as_ref(), read_policy);
                for producer in &lane_producers {
                    *artifact_views_in_flight
                        .entry(producer.clone())
                        .or_default() += 1;
                }
                let declared_reads = stage.reads();
                let upstream = stage
                    .consumes()
                    .iter()
                    .map(|producer| {
                        let product = products.get(producer).cloned().ok_or_else(|| {
                            gmeow_errors::Diag::of_kind(crate::error::StageFailed {
                                cause: None,
                                stage: stage_id.clone(),
                                message: format!("missing published upstream product {producer}"),
                            })
                        })?;
                        // The consumer receives a view of the product that records each
                        // access made through the lane accessors, to be compared with
                        // what its dataflow edge declares when the stage returns.
                        let edge = crate::reads::EdgeAccess::new(
                            &stage_id,
                            producer,
                            declared_reads.get(producer).cloned().unwrap_or_default(),
                        );
                        // An edge that declares no carrier read is handed a view that
                        // shares the producer's carrier by reference and serves only its
                        // byte-artifact lane (the committed artifacts and the internal
                        // ones consumers declare); no byte is copied, and the view reads
                        // the same whether the producer is still whole or already
                        // released. Recording learns what a stage body reads, so it hands
                        // every product over whole; the whole-lane artifact accessors
                        // still return only committed artifacts and the internal ones the
                        // edge already names.
                        let mut view = if lane_producers.contains(producer) {
                            product.consumer_artifact_view(
                                edge,
                                kept_shared.get(producer).cloned().unwrap_or_default(),
                            )
                        } else {
                            product.consumer_view(edge)
                        };
                        if release_payloads {
                            view.restrict_rep_blob_reads()?;
                        }
                        Ok::<_, gmeow_errors::Diag>((producer.clone(), view))
                    })
                    .collect::<Result<BTreeMap<_, _>, _>>()?;
                if let Some(progress) = progress.as_deref() {
                    progress.stage_start(&stage_id);
                }
                memory.push(crate::memory_record::record(
                    &stage_id,
                    MemoryEvent::Dispatch,
                    None,
                    None,
                    None,
                ));
                let tx = tx.clone();
                let progress = progress.clone();
                active += 1;
                // The worker reports its edges whether the stage returns, fails or
                // panics: the reads recorded before a failure still say what the stage
                // touched, and the coordinator reports an undeclared one with the failure.
                let dispatched_edges: Vec<Arc<crate::reads::EdgeAccess>> = upstream
                    .values()
                    .filter_map(|product| product.edge_access().cloned())
                    .collect();
                scope.spawn_fifo(move |_| {
                    // A panic in a stage body must still deliver a result: the
                    // coordinator holds its own sender, so a worker that dies silently
                    // would leave it waiting on `recv` for ever. The phase boundaries
                    // the body crosses on this thread are collected around it, so they
                    // are delivered whether it returns, fails or panics.
                    let (result, phase_records) = crate::memory_record::collecting_phases(|| {
                        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                            exec_stage(stage.as_ref(), root, &upstream, cache, stage_cache_enabled)
                        }))
                        .unwrap_or_else(|payload| {
                            Err(gmeow_errors::Diag::of_kind(crate::error::StageFailed {
                                cause: None,
                                stage: stage_id.clone(),
                                message: format!(
                                    "stage body panicked: {}",
                                    panic_message(&*payload)
                                ),
                            }))
                        })
                    });
                    if let (Some(progress), Ok(run)) = (progress.as_deref(), &result) {
                        progress.stage_end(
                            stage.id(),
                            Duration::from_millis(
                                u64::try_from(run.elapsed_ms).unwrap_or(u64::MAX),
                            ),
                        );
                    }
                    // The views end here, before the coordinator learns the stage is done
                    // and may swap a producer's carrier for its residue.
                    drop(upstream);
                    // The coordinator outlives every scoped worker. A disconnected
                    // receiver therefore means the scope is already unwinding.
                    let _ = tx.send((stage_id, resources, result, dispatched_edges, phase_records));
                });
            }

            if active == 0 {
                if first_error.is_some() || completed == executed.len() {
                    break;
                }
                return Err(gmeow_errors::Diag::of_kind(crate::error::StageFailed {
                    cause: None,
                    stage: "<scheduler>".to_string(),
                    message: format!(
                        "completion-driven scheduling stalled with ready frontier {:?}",
                        ready
                    ),
                }));
            }

            let (stage_id, resources, result, dispatched_edges, phase_records) =
                rx.recv().map_err(|error| {
                    gmeow_errors::Diag::of_kind(crate::error::StageFailed {
                        cause: None,
                        stage: "<scheduler>".to_string(),
                        message: format!("scheduler worker channel closed: {error}"),
                    })
                })?;
            memory.extend(phase_records);
            active -= 1;
            for resource in resources {
                active_resources.remove(&resource);
            }
            // This stage's artifact-lane views ended with it, whether it returned or
            // failed.
            for producer in artifact_lane_producers(by_id[stage_id.as_str()].as_ref(), read_policy)
            {
                let in_flight = artifact_views_in_flight
                    .get_mut(&producer)
                    .expect("a dispatched artifact-lane view was counted");
                *in_flight = in_flight
                    .checked_sub(1)
                    .expect("a completed artifact-lane view was counted");
            }

            let mut run = match result {
                Ok(run) => run,
                Err(error) => {
                    // A failing stage's own error is kept, and so is every read it made
                    // that its edges do not declare: a read on a released product often
                    // IS the cause of the failure, and the declaration to add is the fix.
                    // Declared reads it never reached are not reported: it stopped early.
                    if read_policy == crate::reads::ReadPolicy::Enforce {
                        let undeclared = crate::reads::undeclared_reads(&dispatched_edges);
                        if !undeclared.is_empty() {
                            read_differences.push((stage_id.clone(), undeclared));
                        }
                    }
                    if first_error.is_none() {
                        first_error = Some(error);
                    }
                    continue;
                }
            };
            let stage = by_id[stage_id.as_str()];
            let level = level_by_stage[stage_id.as_str()];
            let carrier_inputs = stage.carrier_consumes().to_vec();

            // Publish each completed action immediately. Persistent cache publication
            // happens before downstream readiness, so a later failure retains every exact
            // successful action receipt instead of discarding the completed prefix.
            let commit_result = (|| -> Result<(), gmeow_errors::Diag> {
                // Declared against recorded reads, before anything of this stage is
                // cached: a stage body that read what its edges do not declare, or did
                // not read what they declare, produced a product the authored graph does
                // not describe. Its product is still correct for the stages after it, so
                // the run continues and reports every such stage at the end; the product
                // is not admitted to the persistent cache, whose key assumes the
                // declarations.
                let mut reads_differ = false;
                if !run.cached {
                    let mismatches = crate::reads::mismatches(&run.edges);
                    if !mismatches.is_empty() && read_policy == crate::reads::ReadPolicy::Enforce {
                        reads_differ = true;
                        read_differences.push((run.id.clone(), mismatches));
                    }
                    recorded_edges.append(&mut run.edges);
                }
                if stage_cache_enabled
                    && !reads_differ
                    && !run.cached
                    && stage.stability() == StageStability::StablePrefix
                    && stage.cache_policy() == CachePolicy::Persistent
                {
                    let receipt = cache.put(
                        &run.key_context,
                        stage.stability().iri(),
                        stage.cache_policy().iri(),
                        &run.output_selection,
                        &run.product,
                    )?;
                    run.cache_write_bytes = receipt.product_blob_bytes;
                    run.receipt = Some(receipt);
                }
                if stage_receipts_enabled && run.receipt.is_none() {
                    run.receipt = Some(PipelineCache::receipt_only(
                        &run.key_context,
                        stage.stability().iri(),
                        stage.cache_policy().iri(),
                        &run.output_selection,
                        &run.product,
                    )?);
                }
                if let Some(receipt) = run.receipt.clone() {
                    stage_receipts.push(receipt);
                }
                run_ledger.replay(std::mem::take(&mut run.diags));
                register_stage_unit(provenance, &run.id, stage.capabilities())?;
                let mut stage_prov = run.product.bundle.provenance().clone();
                register_stage_unit(&mut stage_prov, &run.id, stage.capabilities())?;
                set_bundle_provenance(&mut run.product.bundle, stage_prov);
                if producers_with_persistent_readers.contains(&run.id) {
                    run.product.preserve_input_content_keys()?;
                }

                let level_timing = &mut level_timings[level];
                if level_timing.critical_stage.is_empty()
                    || run.elapsed_ms > level_timing.elapsed_ms
                    || (run.elapsed_ms == level_timing.elapsed_ms
                        && run.id < level_timing.critical_stage)
                {
                    level_timing.elapsed_ms = run.elapsed_ms;
                    level_timing.critical_stage = run.id.clone();
                }
                stage_timings.push(StageTiming {
                    level,
                    stage_id: run.id.clone(),
                    elapsed_ms: run.elapsed_ms,
                    cached: run.cached,
                    cache_outcome: run.cache_outcome,
                    cache_read_bytes: run.cache_read_bytes,
                    cache_write_bytes: run.cache_write_bytes,
                    cache_hydration_rss_delta_kib: run.cache_hydration_rss_delta_kib,
                });
                stage_phase_timings.extend(run.timings.drain(..).map(|timing| StagePhaseTiming {
                    stage_id: run.id.clone(),
                    phase: timing.phase,
                    elapsed_ms: timing.elapsed_ms,
                    metadata: timing.metadata,
                }));
                for (selected_stage, path) in retained_artifact_selection {
                    if selected_stage != &run.id {
                        continue;
                    }
                    let bytes = run.product.artifact(path).ok_or_else(|| {
                        gmeow_errors::Diag::of_kind(crate::error::StageFailed {
                            cause: None,
                            stage: selected_stage.clone(),
                            message: format!("required post-run artifact `{path}` is missing"),
                        })
                    })?;
                    retained_artifacts
                        .insert((selected_stage.clone(), path.clone()), bytes.to_vec());
                }
                let footprint = run.product.footprint();
                if matches!(ready_order, ReadyOrder::Memory(_)) {
                    dataset_bytes.insert(run.id.clone(), footprint.dataset_payload_bytes);
                }
                memory.push(crate::memory_record::record(
                    &run.id,
                    MemoryEvent::Complete,
                    None,
                    Some(footprint),
                    None,
                ));
                products.insert(run.id.clone(), run.product);
                completed += 1;

                if carrier_retention == CarrierRetention::DropAfterLastConsumer {
                    for producer in &carrier_inputs {
                        let remaining = remaining_carrier_readers
                            .get_mut(producer)
                            .expect("carrier producer has a counted reader");
                        *remaining = remaining
                            .checked_sub(1)
                            .expect("completed carrier reader was counted");
                    }
                    if release_payloads {
                        for (producer, reads) in stage.reads() {
                            remaining_payload_readers
                                .get_mut(&producer)
                                .expect("a completed payload reader was counted")
                                .complete(reads);
                        }
                    }
                    // A producer's carrier is swapped for its residue once no carrier
                    // reader is left and no artifact-lane view of it is in flight. The
                    // three things that can make that true are this stage finishing its
                    // last carrier read of the producer, finishing the last view of it,
                    // and the producer itself having just been published.
                    let mut candidates: BTreeSet<String> = carrier_inputs.iter().cloned().collect();
                    candidates.extend(artifact_lane_producers(stage.as_ref(), read_policy));
                    candidates.insert(stage_id.clone());
                    for producer in candidates {
                        let dead = logically_consumed.contains(&producer)
                            && remaining_carrier_readers
                                .get(&producer)
                                .copied()
                                .unwrap_or(0)
                                == 0
                            && artifact_views_in_flight
                                .get(&producer)
                                .copied()
                                .unwrap_or(0)
                                == 0
                            && !retained_carriers.contains(&producer);
                        if dead
                            && let Some(product) = products.get(&producer)
                            && !product.carrier_released
                        {
                            let before = product.footprint();
                            let released = product
                                .clone()
                                .into_carrier_released_keeping(&kept_for(&producer))?;
                            let after = released.footprint();
                            let mut detached = before.released_to(&after);
                            (
                                detached.representation_records,
                                detached.representation_payload_bytes,
                            ) = crate::bundle::released_rep_blobs(
                                product.raw_bundle(),
                                released.raw_bundle(),
                            )?;
                            products.insert(producer.clone(), released);
                            // Made after the swap, so the process figures are those with
                            // the scheduler's reference to the carrier gone.
                            memory.push(crate::memory_record::record(
                                &producer,
                                MemoryEvent::Release,
                                None,
                                Some(after),
                                Some(detached),
                            ));
                        } else if release_payloads
                            && logically_consumed.contains(&producer)
                            && !retained_carriers.contains(&producer)
                            && let Some(product) = products.get_mut(&producer)
                        {
                            let readers = remaining_payload_readers.get(&producer);
                            product.release_unread_handles(|key| {
                                readers.is_some_and(|readers| readers.keeps_handle(key))
                            });
                            memory.extend(product.release_unread_rep_blobs(|rep| {
                                readers.is_some_and(|readers| readers.keeps_blob(rep))
                            })?);
                        }
                    }
                }

                // The stage is complete here: its worker has dropped the stage body's
                // state and its views of upstream products, its product is published,
                // and every carrier its completion made dead has been swapped for its
                // residue. All of that is freed memory the allocator may still hold, so
                // it is returned to the system now, before the next stage is dispatched.
                // Only this coordinator thread reaches this point, one stage at a time.
                memory.push(reclaim_allocator_slack(&stage_id, None));

                if let Some(consumers) = dependents.get(&stage_id) {
                    for consumer in consumers {
                        let remaining = remaining_dependencies
                            .get_mut(consumer)
                            .expect("dependent has a readiness counter");
                        *remaining = remaining
                            .checked_sub(1)
                            .expect("completed dependency was counted");
                        if *remaining == 0 {
                            ready.insert(consumer.clone());
                        }
                    }
                }
                Ok(())
            })();
            if let Err(error) = commit_result
                && first_error.is_none()
            {
                first_error = Some(error);
            }
        }

        if let Some(error) = first_error {
            Err(error)
        } else {
            Ok(())
        }
    });
    let scheduling_error = scheduling_result.err();
    if !read_differences.is_empty() {
        read_differences.sort();
        let total: usize = read_differences.iter().map(|(_, lines)| lines.len()).sum();
        let listing = read_differences
            .iter()
            .map(|(stage, lines)| format!("{stage}:\n    {}", lines.join("\n    ")))
            .collect::<Vec<_>>()
            .join("\n  ");
        // A run that also failed keeps its own error beside the read differences, so
        // neither hides the other.
        let failure = scheduling_error
            .as_ref()
            .map(|error| format!("\n  the run also failed: {error}"))
            .unwrap_or_default();
        return Err(gmeow_errors::Diag::of_kind(crate::error::StageFailed {
            cause: None,
            stage: read_differences[0].0.clone(),
            message: format!(
                "{} stage(s) read differently from their declared dataflow ({total} difference(s)):\n  {listing}{failure}",
                read_differences.len()
            ),
        }));
    }
    if let Some(error) = scheduling_error {
        return Err(error);
    }

    // Completion order is deliberately observational. Present receipts and timings in
    // the certified topological order so output identities and reports remain byte-stable.
    stage_receipts.sort_by_key(|receipt| order_index[receipt.context.stage_id.as_str()]);
    stage_timings.sort_by_key(|timing| order_index[timing.stage_id.as_str()]);
    stage_phase_timings.sort_by_key(|timing| order_index[timing.stage_id.as_str()]);
    // Memory records are an observation of what happened when, so they keep the order
    // they were made in.
    memory.sort_by_key(|made| made.seq);

    if profile {
        let floor: u128 = level_timings.iter().map(|l| l.elapsed_ms).sum();
        let total: u128 = stage_timings.iter().map(|t| t.elapsed_ms).sum();
        tracing::info!(
            target: "pipeline_timing",
            stages = stage_timings.len(),
            levels = level_timings.len(),
            declared_carrier_frontier_stages = declared_level_frontier.len(),
            summed_ms = total,
            level_max_sum_ms = floor,
            "pipeline timing summary",
        );
        let mut slowest = stage_timings.clone();
        slowest.sort_by_key(|t| std::cmp::Reverse(t.elapsed_ms));
        for timing in slowest.iter().take(25) {
            tracing::info!(
                target: "pipeline_timing",
                ms = timing.elapsed_ms,
                stage = %timing.stage_id,
                cached = timing.cached,
                "slowest stage",
            );
        }
        for timing in &level_timings {
            tracing::info!(
                target: "pipeline_timing",
                level = timing.level,
                ms = timing.elapsed_ms,
                critical_stage = %timing.critical_stage,
                "per-level critical stage",
            );
        }
    }

    let combined_digest = combined(&products);
    let receipt_root = combined_receipts(&stage_receipts);
    Ok(RunResult {
        rayon_workers,
        products,
        retained_artifacts,
        combined_digest,
        stage_timings,
        stage_phase_timings,
        level_timings,
        stage_receipts,
        receipt_root,
        ledger: run_ledger,
        recorded_reads: recorded_edges,
        memory,
    })
}

/// Compute the former level-bound carrier frontier for invariant tests and diagnostic
/// comparison with the completion-driven scheduler.
///
/// For each stage that ANY other stage logically consumes, this returns the topological
/// level of its LAST carrier consumer — or its own production level when all later
/// consumers are artifact-only. Production release uses exact remaining-reader counters
/// at completion time and also waits for any artifact-lane view still in flight, which
/// this level formulation does not model; it remains useful for
/// proving the same declared ownership relation over a levelled graph.
///
/// A stage with NO logical consumer has no entry: its product is a run OUTPUT, retained
/// for the life of the [`RunResult`]. The map is total over logically consumed stages,
/// including those with no carrier reader, so the retention bound is exact rather than
/// a hand-picked special case: after level `N`, the stages still holding a live carrier
/// are precisely `{ s : last_carrier_consumer_level(s) > N }` ∪
/// `{ s : s has no logical consumer }` — the property
/// `crate::tests::carrier_retention_is_bounded_by_the_live_frontier` pins.
///
/// Soundness rests on the loader-checked relation
/// `carrier_consumes() ⊆ consumes()`: only the former may read transport lanes, while
/// every later consumer absent from that subset reads committed artifacts only. A
/// consumer that is not a bound stage is ignored here — [`StageGraph::build`] already
/// hard-fails on a dangling dependency, so it cannot occur.
pub(crate) fn last_consumer_levels(
    graph: &StageGraph,
    by_id: &BTreeMap<&str, &Arc<dyn Stage>>,
) -> BTreeMap<String, usize> {
    let level_by_stage: BTreeMap<&str, usize> = graph
        .levels
        .iter()
        .enumerate()
        .flat_map(|(level_idx, level)| {
            level
                .iter()
                .map(move |stage_id| (stage_id.as_str(), level_idx))
        })
        .collect();
    let logically_consumed: BTreeSet<&str> = by_id
        .values()
        .flat_map(|stage| stage.consumes().iter().map(String::as_str))
        .collect();
    let mut last: BTreeMap<String, usize> = BTreeMap::new();
    for producer in logically_consumed {
        if let Some(level_idx) = level_by_stage.get(producer) {
            last.insert(producer.to_string(), *level_idx);
        }
    }
    for (level_idx, level) in graph.levels.iter().enumerate() {
        for consumer in level {
            let Some(stage) = by_id.get(consumer.as_str()) else {
                continue;
            };
            for producer in stage.carrier_consumes() {
                let entry = last.entry(producer.clone()).or_insert(level_idx);
                *entry = (*entry).max(level_idx);
            }
        }
    }
    last
}

/// Selected readers that still have access to a producer's keyed payloads.
#[derive(Default)]
struct PayloadReaders {
    whole: usize,
    handles: BTreeMap<String, usize>,
    blobs: BTreeMap<String, usize>,
}

impl PayloadReaders {
    fn declare(&mut self, reads: crate::reads::ReadSet) {
        for read in reads {
            match read {
                crate::reads::Read::Carrier => self.whole += 1,
                crate::reads::Read::Handle(key) => *self.handles.entry(key).or_default() += 1,
                crate::reads::Read::BlobRep(rep) => *self.blobs.entry(rep).or_default() += 1,
                _ => {}
            }
        }
    }

    fn complete(&mut self, reads: crate::reads::ReadSet) {
        for read in reads {
            let remaining = match read {
                crate::reads::Read::Carrier => &mut self.whole,
                crate::reads::Read::Handle(key) => self
                    .handles
                    .get_mut(&key)
                    .expect("declared typed handle reader was counted"),
                crate::reads::Read::BlobRep(rep) => self
                    .blobs
                    .get_mut(&rep)
                    .expect("declared representation reader was counted"),
                _ => continue,
            };
            *remaining = remaining
                .checked_sub(1)
                .expect("completed payload reader was counted");
        }
    }

    fn keeps_handle(&self, key: &str) -> bool {
        self.keeps_handle_after(key, None)
    }

    /// Is a whole-carrier or keyed reader still outstanding after one stage completes?
    fn keeps_handle_after(&self, key: &str, completed: Option<&crate::reads::ReadSet>) -> bool {
        use crate::reads::Read;

        let whole = completed.is_some_and(|reads| reads.contains(&Read::Carrier));
        let keyed = completed.is_some_and(|reads| {
            reads
                .iter()
                .any(|read| matches!(read, Read::Handle(read_key) if read_key == key))
        });
        self.whole > usize::from(whole)
            || self
                .handles
                .get(key)
                .is_some_and(|&readers| readers > usize::from(keyed))
    }

    fn keeps_blob(&self, rep: &str) -> bool {
        self.whole > 0 || self.blobs.get(rep).is_some_and(|&readers| readers > 0)
    }
}

/// The producers `stage` is handed an artifact-lane view of: the dependencies whose edge
/// declares no carrier read, under the enforcing read policy. Recording hands every
/// product over whole, so it has none.
fn artifact_lane_producers(
    stage: &dyn Stage,
    policy: crate::reads::ReadPolicy,
) -> BTreeSet<String> {
    if policy == crate::reads::ReadPolicy::Record {
        return BTreeSet::new();
    }
    let carrier: BTreeSet<&str> = stage
        .carrier_consumes()
        .iter()
        .map(String::as_str)
        .collect();
    stage
        .consumes()
        .iter()
        .filter(|producer| !carrier.contains(producer.as_str()))
        .cloned()
        .collect()
}

/// The text of a caught panic payload, for the failure a panicking stage reports.
fn panic_message(payload: &(dyn std::any::Any + Send)) -> String {
    payload
        .downcast_ref::<&str>()
        .map(|text| (*text).to_owned())
        .or_else(|| payload.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "a non-text panic payload".to_owned())
}

/// Execute one stage: assemble its upstream inputs, consult the cache, and run it
/// (holding any resource it requires exclusively) on a miss.
fn exec_stage(
    stage: &dyn Stage,
    root: &Path,
    products: &BTreeMap<String, StageProduct>,
    cache: &PipelineCache,
    stage_cache_enabled: bool,
) -> Result<StageRun, gmeow_errors::Diag> {
    // Assemble exactly the upstream products this stage declared.
    let mut upstream: BTreeMap<String, StageProduct> = BTreeMap::new();
    for dep in stage.consumes() {
        let p = products.get(dep).ok_or_else(|| {
            gmeow_errors::Diag::of_kind(crate::error::StageFailed {
                cause: None,
                stage: stage.id().to_string(),
                message: format!("missing upstream product {dep}"),
            })
        })?;
        upstream.insert(dep.clone(), p.clone());
    }

    // Cache key = the typed action context: build/toolchain/target/profile/features,
    // stage/implementation/codec identity, producer-qualified whole/entity inputs,
    // and each declared RAW source path+digest.
    // (export leaves that read non-fold sources — references.ttl, the eval corpus, the
    // slice manifests — declare them there so a source change busts the cache;
    // cache soundness for stages that legitimately consume nothing).
    //
    // ARTIFACT-LEVEL granularity: for a producer the stage declares typed dataflow
    // entities over (`consumed_entities`), fold ONLY those named graphs' canonical
    // digests rather than the producer's whole-bundle digest — so a change to a graph
    // the stage does not read no longer busts its key. A producer NOT declared narrows
    // to nothing: it stays a whole-product dependency (the sound default). Narrowing
    // can only ever REMOVE inputs from the key for graphs the stage provably ignores;
    // the loader's DataFlow agreement is what guarantees the declaration is honest.
    let key_context = action_key_context(stage, root, &upstream)?;
    let started = std::time::Instant::now();
    let rss_before_kib = ProcessMemory::sample().rss_kib;

    if stage_cache_enabled
        && stage.stability() == StageStability::StablePrefix
        && stage.cache_policy() == CachePolicy::Persistent
        && let Some(hit) = cache.get(&key_context)?
    {
        // A cache hit re-serves the identical product, so its `diagnostics:nodes` blob
        // (empty for a non-producer) recovers this stage's run-ledger contribution
        // WITHOUT re-running it — byte-identical to the fresh `out.diags`.
        // The attach-drift check MUST fire here too: a declaration edit need not bump
        // `impl_version`, so a stale cached product with drifted declarations would sail
        // through unless the compare runs on the returned product in BOTH branches.
        let output_selection = verify_attach_drift(stage, &upstream, &hit.product)?;
        PipelineCache::validate_hit_receipt(
            &key_context,
            stage.stability().iri(),
            stage.cache_policy().iri(),
            &output_selection,
            &hit,
        )?;
        let diags = hit.product.diag_nodes();
        return Ok(StageRun {
            id: stage.id().to_string(),
            key_context,
            product: hit.product,
            cached: true,
            elapsed_ms: started.elapsed().as_millis(),
            diags,
            timings: Vec::new(),
            receipt: Some(hit.receipt),
            output_selection,
            cache_outcome: "hit".to_string(),
            cache_read_bytes: hit.hydrated_bytes,
            cache_write_bytes: 0,
            cache_hydration_rss_delta_kib: rss_delta_kib(
                rss_before_kib,
                ProcessMemory::sample().rss_kib,
            ),
            edges: Vec::new(),
        });
    }

    let input = StageInput {
        root,
        upstream: &upstream,
    };
    // Acquire the stage's declared resources exclusively, in sorted IRI order
    // (deadlock-free: every stage takes a common subset of locks in the same
    // order), holding them across `run`. Two stages requiring the same resource
    // serialize — the resource-conflict replacement for the former engine lock.
    let mut resources: Vec<&str> = stage.resources().iter().map(String::as_str).collect();
    resources.sort_unstable();
    resources.dedup();
    let locks: Vec<Arc<Mutex<()>>> = resources.iter().map(|r| resource_lock(r)).collect();
    let mut _guards = Vec::with_capacity(locks.len());
    for (lock, resource) in locks.iter().zip(&resources) {
        _guards.push(lock.lock().map_err(|e| {
            gmeow_errors::Diag::of_kind(crate::error::StageFailed {
                cause: None,
                stage: stage.id().to_string(),
                message: format!("resource lock {resource} poisoned: {e}"),
            })
        })?);
    }
    // Carrier-scale serializers share one permit because each owns a whole-document
    // buffer. Within one run, the pages a sibling serializer freed are returned to the
    // system when the coordinator publishes that sibling, which is before it dispatches
    // the next holder of the permit.
    let out = stage.run(input)?;
    drop(_guards);

    // Verify the stage's ACTUAL attach delta against its declaration on the cache-miss
    // path too — the same compare as the cache-hit branch, so drift HARD-fails regardless
    // of whether the product came fresh or from cache.
    let output_selection = verify_attach_drift(stage, &upstream, &out.product)?;

    let cache_outcome = if !stage_cache_enabled {
        "bypass:disabled"
    } else if stage.stability() != StageStability::StablePrefix {
        "bypass:unstable"
    } else if stage.cache_policy() == CachePolicy::Recompute {
        "bypass:recompute"
    } else {
        "miss:not-found"
    };

    Ok(StageRun {
        id: stage.id().to_string(),
        key_context,
        product: out.product,
        cached: false,
        elapsed_ms: started.elapsed().as_millis(),
        diags: out.diags,
        timings: out.timings,
        receipt: None,
        output_selection,
        cache_outcome: cache_outcome.to_string(),
        cache_read_bytes: 0,
        cache_write_bytes: 0,
        cache_hydration_rss_delta_kib: None,
        edges: upstream
            .values()
            .filter_map(|product| product.edge_access().cloned())
            .collect(),
    })
}

fn rss_delta_kib(before: Option<u64>, after: Option<u64>) -> Option<i64> {
    let before = i128::from(before?);
    let after = i128::from(after?);
    i64::try_from(after - before).ok()
}

/// The set of named-graph IRIs a product's carrier bundle carries.
fn product_graphs(product: &StageProduct) -> std::collections::BTreeSet<String> {
    product
        .raw_bundle()
        .dataset()
        .owned_named_graphs()
        .filter_map(|t| match t {
            purrdf::RdfTerm::Iri(iri) => Some(iri),
            _ => None,
        })
        .collect()
}

fn product_handle_records(product: &StageProduct) -> BTreeMap<String, String> {
    product
        .handle_commitments()
        .iter()
        .map(|(graph, binding)| (graph.clone(), binding.digest.clone()))
        .collect()
}

/// HARD-fail if a stage's ACTUAL attach delta diverges from its DECLARED attach set,
/// in either direction. The delta is "what this stage attaches" = the named graphs /
/// blob-rep lanes present in its OUTPUT product bundle but NOT in its effective INPUT.
/// For named graphs, that input honors `consumed_entities()`: a typed producer contributes
/// only the graph entities this stage actually reads, while an untyped producer contributes
/// its whole product. The original published blob identities of carrier inputs determine
/// this delta independently of payload retention. Two producers may attach distinct
/// digests under the same representation label. The
/// cumulative output bundle is diffed against those inputs, so a graph the stage actually
/// consumes and carries forward is NOT counted as its attach, while a graph merely present
/// elsewhere in an upstream carrier is not allowed to hide a real attachment. Compared
/// against the stage's `attaches_graphs()` /
/// `attaches_blob_reps()` declaration (Rust/RDF-verified at load). Runs on both the cache-hit
/// and cache-miss paths (called by [`exec_stage`]) so a cached product with drifted
/// declarations cannot slip through. No optionality, no fallback.
pub(crate) fn verify_attach_drift(
    stage: &dyn Stage,
    upstream: &BTreeMap<String, StageProduct>,
    product: &StageProduct,
) -> Result<ReceiptOutputSelection, gmeow_errors::Diag> {
    use std::collections::BTreeSet;

    // Effective input graph set = the same typed-entity narrowing used by the cache key.
    // A producer absent from the declaration remains a whole-product dependency.
    // Blob record identities come from publication even when a consumer view holds
    // only some of the producer's payloads.
    let entities: BTreeMap<&str, &[String]> = stage
        .consumed_entities()
        .iter()
        .map(|(producer, ents)| (producer.as_str(), ents.as_slice()))
        .collect();
    let mut input_graphs: BTreeSet<String> = BTreeSet::new();
    let mut input_blob_records: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut input_artifacts: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut input_handles: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut input_default_graph = BTreeSet::new();
    let mut input_provenance = BTreeSet::new();
    let mut input_content_store = BTreeSet::new();
    let carrier_inputs: BTreeSet<&str> = stage
        .carrier_consumes()
        .iter()
        .map(String::as_str)
        .collect();
    for (producer, up) in upstream {
        let upstream_graphs = product_graphs(up);
        match entities.get(producer.as_str()) {
            Some(ents) if !ents.is_empty() => input_graphs.extend(
                upstream_graphs
                    .into_iter()
                    .filter(|graph| ents.iter().any(|entity| entity == graph)),
            ),
            _ => input_graphs.extend(upstream_graphs),
        }
        for (representation, digests) in up.published_blob_records() {
            input_blob_records
                .entry(representation.clone())
                .or_default()
                .extend(digests.iter().cloned());
        }
        for (name, digests) in up.artifact_records() {
            input_artifacts.entry(name).or_default().extend(digests);
        }
        for (graph, digest) in product_handle_records(up) {
            input_handles.entry(graph).or_default().insert(digest);
        }
        // Only declared carrier inputs can contribute transport lanes. An
        // artifact-only dependency's view carries no dataset, provenance or store of its
        // own, and the product may already have been released; inspecting those lanes
        // here would violate the same transport boundary this verifier is meant to
        // enforce.
        if stage.cache_policy() == CachePolicy::Persistent
            && carrier_inputs.contains(producer.as_str())
        {
            input_default_graph.extend(default_graph_keys(up));
            input_provenance.extend(provenance_keys(up)?);
            if let Some(keys) = up.published_store_keys() {
                input_content_store.extend(keys.iter().cloned());
            } else {
                input_content_store.extend(content_store_keys(up)?);
            }
        }
    }

    if stage.cache_policy() == CachePolicy::Persistent {
        for (lane, inherited) in [
            (
                "default graph",
                default_graph_keys(product)
                    .intersection(&input_default_graph)
                    .cloned()
                    .collect::<Vec<_>>(),
            ),
            (
                "provenance",
                provenance_keys(product)?
                    .intersection(&input_provenance)
                    .cloned()
                    .collect::<Vec<_>>(),
            ),
            (
                "content store",
                content_store_keys(product)?
                    .intersection(&input_content_store)
                    .cloned()
                    .collect::<Vec<_>>(),
            ),
        ] {
            if !inherited.is_empty() {
                return Err(gmeow_errors::Diag::of_kind(crate::error::StageFailed {
                    cause: None,
                    stage: stage.id().to_string(),
                    message: format!(
                        "persistent contribution inherits {lane} carrier state from its \
                         effective upstream ({inherited:?}); independently bounded units must \
                         emit only their delta or declare cacheRecomputeAggregate"
                    ),
                }));
            }
        }
    }

    let delta_graphs: BTreeSet<String> = product_graphs(product)
        .difference(&input_graphs)
        .cloned()
        .collect();
    let delta_blob_reps: BTreeSet<String> = product
        .published_blob_records()
        .filter_map(|(representation, output_digests)| {
            let already_present = input_blob_records
                .get(representation)
                .is_some_and(|input_digests| output_digests.is_subset(input_digests));
            (!already_present).then(|| representation.clone())
        })
        .collect();

    check_lane(
        stage.id(),
        "gmeow:attachesGraph",
        &delta_graphs,
        stage.attaches_graphs(),
    )?;
    check_lane(
        stage.id(),
        "gmeow:attachesBlobRep",
        &delta_blob_reps,
        stage.attaches_blob_reps(),
    )?;
    let delta_artifacts = product
        .artifact_records()
        .into_iter()
        .filter_map(|(name, output_digests)| {
            let already_present = input_artifacts
                .get(&name)
                .is_some_and(|input_digests| output_digests.is_subset(input_digests));
            (!already_present).then_some(name)
        })
        .collect();
    let delta_handles = product_handle_records(product)
        .into_iter()
        .filter_map(|(graph, digest)| {
            let already_present = input_handles
                .get(&graph)
                .is_some_and(|input_digests| input_digests.contains(&digest));
            (!already_present).then_some(graph)
        })
        .collect();
    Ok(ReceiptOutputSelection {
        graphs: delta_graphs.into_iter().collect(),
        blob_representations: delta_blob_reps.into_iter().collect(),
        logical_artifacts: delta_artifacts,
        handles: delta_handles,
        default_graph: default_graph_commitment(product)?,
        provenance: provenance_commitment(product)?,
        content_store: content_store_commitment(product)?,
    })
}

/// Compare one lane's actual attach delta against its declared set, HARD-failing on any
/// divergence in either direction ([`crate::error::AttachDrift`]).
fn check_lane(
    stage_id: &str,
    lane: &str,
    actual: &std::collections::BTreeSet<String>,
    declared: &[String],
) -> Result<(), gmeow_errors::Diag> {
    let declared: std::collections::BTreeSet<String> = declared.iter().cloned().collect();
    let attached_undeclared: Vec<String> = actual.difference(&declared).cloned().collect();
    let declared_unattached: Vec<String> = declared.difference(actual).cloned().collect();
    if !attached_undeclared.is_empty() || !declared_unattached.is_empty() {
        return Err(gmeow_errors::Diag::of_kind(crate::error::AttachDrift {
            stage: stage_id.to_string(),
            lane: lane.to_string(),
            attached_undeclared,
            declared_unattached,
        }));
    }
    Ok(())
}

/// The action-key identity available from either a live product or its authenticated
/// immutable receipt.
pub trait ActionIdentity {
    /// Producer that owns this product or receipt.
    fn producer_id(&self) -> &str;
    /// Complete native product commitment.
    fn product_digest(&self) -> &str;
    /// Selected graph commitment, including its typed handle when present.
    ///
    /// # Errors
    /// Propagates invalid native graph or handle commitments.
    fn entity_digest(&self, entity: &str) -> gmeow_errors::Result<Option<String>>;
}

impl ActionIdentity for StageProduct {
    fn producer_id(&self) -> &str {
        &self.stage_id
    }

    fn product_digest(&self) -> &str {
        &self.digest
    }

    fn entity_digest(&self, entity: &str) -> gmeow_errors::Result<Option<String>> {
        if !crate::handle_identity::contains_graph(self.raw_bundle(), entity) {
            return Ok(None);
        }
        Ok(Some(crate::handle_identity::entity_digest(
            &self
                .raw_bundle()
                .graph_digest(
                    entity,
                    &mut purrdf::CanonicalizationControl::new(purrdf::CANON_CORPUS_POLICY, None),
                )?
                .to_hex(),
            self.handle_commitments()
                .get(entity)
                .map(|binding| (binding.identity.as_str(), binding.digest.as_str())),
        )))
    }
}

impl ActionIdentity for StageReceipt {
    fn producer_id(&self) -> &str {
        &self.context.stage_id
    }

    fn product_digest(&self) -> &str {
        &self.product_digest
    }

    fn entity_digest(&self, entity: &str) -> gmeow_errors::Result<Option<String>> {
        Ok(self
            .graphs
            .iter()
            .find(|row| row.identity == entity)
            .map(|row| {
                crate::handle_identity::entity_digest(
                    &row.digest,
                    self.typed_handles.iter().filter_map(|binding| {
                        let (_, graph) = binding.identity.split_once('#')?;
                        (graph == entity)
                            .then_some((binding.identity.as_str(), binding.digest.as_str()))
                    }),
                )
            }))
    }
}

/// Build the scheduler's single typed action-key authority from either live products
/// or authenticated receipts. Producer and entity identity remain attached to every
/// digest; raw inputs remain separate path/digest rows.
fn action_key_context_from_identities<T: ActionIdentity>(
    stage: &dyn Stage,
    upstream: &BTreeMap<String, T>,
    raw_inputs: Vec<RawInputDigest>,
) -> Result<StageKeyContext, gmeow_errors::Diag> {
    let entities: BTreeMap<&str, &[String]> = stage
        .consumed_entities()
        .iter()
        .map(|(producer, entities)| (producer.as_str(), entities.as_slice()))
        .collect();
    let mut upstream_rows = Vec::new();
    for (producer, identity) in upstream {
        if identity.producer_id() != producer {
            return Err(gmeow_errors::Diag::of_kind(crate::error::StageFailed {
                cause: None,
                stage: stage.id().to_string(),
                message: format!(
                    "upstream map key {producer} carries identity for {}",
                    identity.producer_id()
                ),
            }));
        }
        match entities.get(producer.as_str()) {
            Some(selected) if !selected.is_empty() => {
                for entity in *selected {
                    let digest = identity.entity_digest(entity)?.ok_or_else(|| {
                        gmeow_errors::Diag::of_kind(crate::error::StageFailed {
                            cause: None,
                            stage: stage.id().to_string(),
                            message: format!(
                                "declared input entity <{entity}> is absent from producer \
                                 {producer}"
                            ),
                        })
                    })?;
                    upstream_rows.push(StageInputDigest {
                        producer: producer.clone(),
                        entity: Some(entity.clone()),
                        digest,
                    });
                }
            }
            _ => upstream_rows.push(StageInputDigest {
                producer: producer.clone(),
                entity: None,
                digest: identity.product_digest().to_string(),
            }),
        }
    }

    Ok(StageKeyContext::new(
        stage.id(),
        stage.impl_version(),
        upstream_rows,
        raw_inputs,
    ))
}

/// Build an action context from the exact live products a stage consumes.
pub fn action_key_context(
    stage: &dyn Stage,
    root: &Path,
    upstream: &BTreeMap<String, StageProduct>,
) -> Result<StageKeyContext, gmeow_errors::Diag> {
    let raw_inputs = input_file_digests(stage, root)?;
    action_key_context_from_identities(stage, upstream, raw_inputs)
}

/// Build the identical action context from authenticated upstream receipts, without
/// hydrating their datasets or handles.
pub fn action_key_context_from_receipts(
    stage: &dyn Stage,
    root: &Path,
    upstream: &BTreeMap<String, StageReceipt>,
) -> Result<StageKeyContext, gmeow_errors::Diag> {
    let raw_inputs = input_file_digests(stage, root)?;
    action_key_context_from_identities(stage, upstream, raw_inputs)
}

/// Build a receipt-derived action context while hashing each shared raw file at most
/// once in this producer admission. Many stage declarations intentionally overlap over
/// the same repository corpus; repeating those reads per stage turns a warm DAG probe
/// back into corpus-scale I/O.
pub(crate) fn action_key_context_from_receipts_cached(
    stage: &dyn Stage,
    root: &Path,
    upstream: &BTreeMap<String, StageReceipt>,
    digest_cache: &mut BTreeMap<PathBuf, String>,
) -> Result<StageKeyContext, gmeow_errors::Diag> {
    let raw_inputs = input_file_digests_cached(stage, root, digest_cache)?;
    action_key_context_from_identities(stage, upstream, raw_inputs)
}

/// Each declared raw input as a repo-relative path plus content digest. A missing
/// input hard-fails; it is never silently treated as unchanged.
pub(crate) fn input_file_digests(
    stage: &dyn Stage,
    root: &Path,
) -> Result<Vec<RawInputDigest>, gmeow_errors::Diag> {
    input_file_digests_cached(stage, root, &mut BTreeMap::new())
}

fn input_file_digests_cached(
    stage: &dyn Stage,
    root: &Path,
    digest_cache: &mut BTreeMap<PathBuf, String>,
) -> Result<Vec<RawInputDigest>, gmeow_errors::Diag> {
    let mut files = stage.input_files(root)?;
    files.sort();
    files.dedup();
    let mut rows = Vec::with_capacity(files.len());
    for path in &files {
        let rel = path
            .strip_prefix(root)
            .unwrap_or(path)
            .to_string_lossy()
            .into_owned();
        let digest = if let Some(digest) = digest_cache.get(path) {
            digest.clone()
        } else {
            let digest = digest_input_file(path).map_err(|e| {
                gmeow_errors::Diag::of_kind(crate::error::StageFailed {
                    cause: None,
                    stage: stage.id().to_string(),
                    message: format!("declared input file {} could not be read: {e}", rel),
                })
            })?;
            digest_cache.insert(path.clone(), digest.clone());
            digest
        };
        rows.push(RawInputDigest { path: rel, digest });
    }
    rows.sort();
    rows.dedup();
    Ok(rows)
}

/// Stream one raw input through the same single-field framing as
/// `content_digest(&[bytes])`. Action-key construction must not make peak RSS a function
/// of the largest declared input merely to learn its digest.
fn digest_input_file(path: &Path) -> std::io::Result<String> {
    use sha2::{Digest as _, Sha256};

    let mut input = std::io::BufReader::new(std::fs::File::open(path)?);
    let mut hash = Sha256::new();
    // Nested Rayon work may suspend this caller above another stage's hashing.
    let mut buffer = vec![0_u8; 1024 * 1024];
    loop {
        let read = input.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hash.update(&buffer[..read]);
    }
    hash.update(b"\x1f");
    Ok(format!("{:x}", hash.finalize()))
}

/// Fold the products into one order-independent digest over sorted
/// `(id, product-digest)` pairs.
fn combined(products: &BTreeMap<String, StageProduct>) -> String {
    let mut fields: Vec<&[u8]> = Vec::with_capacity(products.len() * 2);
    for (id, p) in products {
        fields.push(id.as_bytes());
        fields.push(p.digest.as_bytes());
    }
    content_digest(&fields)
}

fn combined_receipts(receipts: &[StageReceipt]) -> String {
    let rows: Vec<Vec<u8>> = receipts
        .iter()
        .map(|receipt| format!("{}\x1f{}", receipt.context.stage_id, receipt.digest()).into_bytes())
        .collect();
    let fields: Vec<&[u8]> = rows.iter().map(Vec::as_slice).collect();
    content_digest(&fields)
}

#[path = "scheduler.digest_tests.rs"]
#[cfg(test)]
mod digest_tests;
