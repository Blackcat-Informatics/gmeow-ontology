// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The level-parallel scheduler + the [`RunContext`] (P2).
//!
//! Stages run by topological level (from [`crate::graph::StageGraph`]); within a
//! level, independent stages run in parallel (rayon). A stage that declares a
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
use std::sync::{Arc, LazyLock, Mutex};
use std::time::Duration;

use gmeow_cli_core::Reporter;
use purrdf::provenance::DatasetProvenance;
use rayon::prelude::*;

use crate::bundle::set_bundle_provenance;
use crate::cache::{
    PipelineCache, RawInputDigest, ReceiptOutputSelection, StageInputDigest, StageKeyContext,
    StageReceipt, content_digest,
};
use crate::graph::StageGraph;
use crate::node::{
    CachePolicy, SERIALIZATION_BUFFER_RESOURCE, SINK_CAPABILITY, Stage, StageInput, StageProduct,
    StageRunTiming, StageStability,
};
use crate::provenance::register_stage_unit;

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

/// Return allocator slack from completed DAG waves before the one terminal constructs
/// its whole-carrier wire value.
///
/// Dropping a carrier makes its allocations unreachable, but glibc may retain their
/// pages in process arenas. A cold run can otherwise carry mapping/dictionary arena
/// slack into the terminal's necessarily large canonical payload construction and cross
/// the 16-GiB runner boundary even though the live Rust values fit. `malloc_trim` is
/// thread-safe; calls occur either while the declarative serialization permit excludes
/// every other carrier-scale serializer or at the terminal's topological level barrier.
/// Cheap siblings may still be active at a permit handoff, which is safe under that
/// process-wide MT-Safe contract. It is only a reclamation hint and cannot change a
/// product, cache key, or stage order. Allocators without this interface keep the
/// ordinary drop behavior.
#[cfg(all(target_os = "linux", target_env = "gnu"))]
fn reclaim_allocator_slack_before_serialization() {
    // SAFETY: glibc documents `malloc_trim` as MT-Safe. The argument merely requests
    // release of every wholly free top-level heap page; no Rust allocation is exposed or
    // accessed through the call.
    unsafe {
        libc::malloc_trim(0);
    }
}

#[cfg(not(all(target_os = "linux", target_env = "gnu")))]
fn reclaim_allocator_slack_before_serialization() {}

/// What the scheduler does with a stage's carrier once its LAST declared carrier
/// consumer has run.
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
    /// Release each stage's carrier at its drop-after-last-carrier-consumer point
    /// ([`last_consumer_levels`]): the frozen dataset, typed handles, blob records,
    /// provenance, and internal `pipeline/` byte artifacts are freed as soon as no
    /// stage can still read them, bounding peak residency to the live frontier plus
    /// every run OUTPUT. THE profile for the whole-repository build
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
    /// Maximum concurrent stages within a level (rayon pool size).
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
    /// explicitly opts in. [`crate::run::run_full_scoped_with_progress`] selects
    /// [`CarrierRetention::DropAfterLastConsumer`]; that is the only whole-repository
    /// path and the one whose peak residency has to be bounded.
    pub carrier_retention: CarrierRetention,
    /// Intermediate carriers an explicit out-of-band caller will read after the DAG.
    ///
    /// This is a narrow keep-set layered over [`CarrierRetention::DropAfterLastConsumer`],
    /// not a second scheduling graph: every id must already be an executed production
    /// stage in the selected run. It lets a whole-bundle proof retain `stage-snapshot`
    /// (or the terminal's exact carrier inputs) without retaining dozens of unrelated
    /// cumulative products.
    pub retained_carriers: BTreeSet<String>,
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
    /// persistent cache under `.cache/gmeow-sync/pipeline/<build-fingerprint>/`.
    ///
    /// The cache is namespaced by [`crate::cache::BUILD_FINGERPRINT`]. Opening it keeps
    /// the current and newest prior namespace and reaps only older IDLE siblings; a
    /// shared namespace lease protects concurrent work. Every action key also embeds
    /// the fingerprint, so code/dependency/toolchain changes cannot serve an older
    /// executable's product.
    pub fn open(root: impl Into<PathBuf>, jobs: usize) -> Result<Self, gmeow_errors::Diag> {
        let root = root.into();
        let base = PipelineCache::default_dir(&root);
        let fp = &crate::cache::BUILD_FINGERPRINT[..16];
        let cache = PipelineCache::open(base.join(fp))?;
        PipelineCache::prune_namespaces(&base, fp, 2)?;
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
            // Persistent boundary: the cache lives under the repo's `.cache/`, not a
            // temporary directory, so there is nothing to tear down.
            _ephemeral_cache_dir: None,
        })
    }

    /// Construct a run context whose cache lives in a FRESH, run-unique temp
    /// directory rather than the persistent `.cache/gmeow-sync/pipeline/`.
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
}

/// The result of a pipeline run: every stage's product plus the order-independent
/// digest folding them all (the determinism witness).
///
/// `StageProduct`'s carrier (`Arc<PipelineBundle>`) has no value equality, so this
/// no longer derives `PartialEq`/`Eq`; the determinism witness is the
/// `combined_digest` (a `String`), which tests compare directly (C4).
#[derive(Debug, Clone)]
pub struct RunResult {
    /// Each stage's product, keyed by stage id (sorted).
    pub products: BTreeMap<String, StageProduct>,
    /// The hex SHA-256 over the sorted `(id, product-digest)` pairs.
    pub combined_digest: String,
    /// Per-stage wall-clock timings in topological execution order.
    pub stage_timings: Vec<StageTiming>,
    /// Internal phase timings emitted by freshly executed stages.
    pub stage_phase_timings: Vec<StagePhaseTiming>,
    /// Per-level critical-stage timings in topological level order.
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
    /// one regardless of level/commit interleaving. `run_full` threads this into
    /// `RunReport.ledger` and attaches its own run-level reconcile findings to it.
    pub ledger: gmeow_errors::DiagLedger,
}

/// One stage's wall-clock timing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StageTiming {
    /// The topological level index the stage ran in.
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

/// The slowest stage in one scheduler level.
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
/// product can be persisted after the parallel phase of its level.
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
    for retained in &ctx.retained_carriers {
        if !by_id.contains_key(retained.as_str()) || skip.contains(retained) {
            return Err(gmeow_errors::Diag::of_kind(crate::error::StageFailed {
                stage: "<scheduler>".to_string(),
                message: format!(
                    "retained carrier `{retained}` does not name an executed production stage"
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
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(ctx.jobs)
        .build()
        .map_err(|e| {
            gmeow_errors::Diag::of_kind(crate::error::StageFailed {
                stage: "<scheduler>".to_string(),
                message: format!("failed to build rayon pool: {e}"),
            })
        })?;

    let mut products: BTreeMap<String, StageProduct> = BTreeMap::new();
    // Opt-in per-stage profiling (GMEOW_PIPELINE_TIMING=1): accumulate
    // (stage_id, elapsed_ms, cached) and dump the slowest stages at the end so the
    // critical path is visible without changing default behaviour.
    let profile = std::env::var_os("GMEOW_PIPELINE_TIMING").is_some();
    let mut stage_timings: Vec<StageTiming> = Vec::new();
    let mut stage_phase_timings: Vec<StagePhaseTiming> = Vec::new();
    // (level_index, slowest-stage ms in the level, slowest-stage id): the sum of the
    // per-level maxima is the critical-path floor the level-barrier scheduler imposes.
    let mut level_timings: Vec<LevelTiming> = Vec::new();
    let mut stage_receipts: Vec<StageReceipt> = Vec::new();
    // The run-wide FORWARD diagnostics ledger: each stage's `diags` (fresh or cache-
    // restored) are replayed here in the sequential commit phase. `replay` hash-conses
    // by content-addressed fingerprint, so the folded ledger is byte-identical
    // regardless of level order or fresh/cache interleaving.
    let mut run_ledger = gmeow_errors::DiagLedger::new();

    // Drop-after-last-consumer point for stage-source-load's source-span table: the MAX
    // topological level holding a stage that declares `consumes_span_table()`. The real
    // consumers (stage-validate / stage-compile-logic) all run at or before this level, so
    // stripping the span blob AFTER this level commits keeps the drop reachable but never
    // spurious — every legitimate reader has already run, and any later reader HARD-fails.
    let span_drop_level: Option<usize> = graph
        .levels
        .iter()
        .enumerate()
        .rev()
        .find(|(_, level)| {
            level.iter().any(|id| {
                by_id
                    .get(id.as_str())
                    .is_some_and(|s| s.consumes_span_table())
            })
        })
        .map(|(idx, _)| idx);

    // Drop-after-last-consumer point for every stage's WHOLE carrier — the general law the
    // span-table strip above is one early, finer-grained instance of. Computed once, from
    // the same levelling, by exactly the same argument.
    let carrier_drop_level = last_consumer_levels(graph, &by_id);

    for (level_idx, level) in graph.levels.iter().enumerate() {
        // The sink is unique by loader contract. Reclaim pages freed by every completed
        // wave before it builds the one whole-carrier wire payload; doing this at the
        // level boundary also guarantees no sibling allocation is in flight.
        if level.iter().any(|id| {
            by_id.get(id.as_str()).is_some_and(|stage| {
                stage
                    .capabilities()
                    .iter()
                    .any(|capability| capability == SINK_CAPABILITY)
            })
        }) {
            reclaim_allocator_slack_before_serialization();
        }
        // Parallel phase: every stage in the level runs concurrently; stages that
        // declare a shared resource serialize internally on that resource's permit.
        // `products` and `cache` are read-only here — siblings in one level never
        // depend on each other, so no stage can hit another's same-level cache write.
        let root: &Path = &ctx.root;
        let cache = &ctx.cache;
        let stage_cache_enabled = ctx.stage_cache_enabled;
        let progress = ctx.progress.as_deref();
        let runs: Vec<StageRun> = pool.install(|| {
            level
                .par_iter()
                .filter(|id| !skip.contains(id.as_str()))
                .map(|id| -> Result<StageRun, gmeow_errors::Diag> {
                    let stage = by_id.get(id.as_str()).ok_or_else(|| {
                        gmeow_errors::Diag::of_kind(crate::error::StageFailed {
                            stage: id.clone(),
                            message: "stage in graph was not bound".to_string(),
                        })
                    })?;
                    if let Some(progress) = progress {
                        progress.stage_start(stage.id());
                    }
                    let result =
                        exec_stage(stage.as_ref(), root, &products, cache, stage_cache_enabled);
                    if let (Some(progress), Ok(run)) = (progress, &result) {
                        progress.stage_end(
                            stage.id(),
                            Duration::from_millis(
                                u64::try_from(run.elapsed_ms).unwrap_or(u64::MAX),
                            ),
                        );
                    }
                    result
                })
                .collect::<Result<Vec<_>, _>>()
        })?;

        // Sequential commit phase: persist cache entries, stamp provenance, and
        // publish products for the next level.
        let mut level_max: u128 = 0;
        let mut level_max_id = String::new();
        for mut r in runs {
            let stage = by_id[r.id.as_str()];
            if ctx.stage_cache_enabled
                && !r.cached
                && stage.stability() == StageStability::StablePrefix
                && stage.cache_policy() == CachePolicy::Persistent
            {
                let receipt = ctx.cache.put(
                    &r.key_context,
                    stage.stability().iri(),
                    stage.cache_policy().iri(),
                    &r.output_selection,
                    &r.product,
                )?;
                r.cache_write_bytes = receipt.product_blob_bytes;
                r.receipt = Some(receipt);
            }
            if ctx.stage_receipts_enabled && r.receipt.is_none() {
                r.receipt = Some(PipelineCache::receipt_only(
                    &r.key_context,
                    stage.stability().iri(),
                    stage.cache_policy().iri(),
                    &r.output_selection,
                    &r.product,
                )?);
            }
            if let Some(receipt) = r.receipt.clone() {
                stage_receipts.push(receipt);
            }
            // Fold this stage's forward diagnostic nodes into the run-wide ledger.
            run_ledger.replay(std::mem::take(&mut r.diags));
            // Register this stage as a provenance unit in the run-wide sidecar
            // (capability-derived origin: sourceOrigin → Source, else → Generated).
            register_stage_unit(&mut ctx.provenance, &r.id, stage.capabilities());
            // Thread a per-stage provenance into the produced bundle so the carrier
            // CARRIES a provenance sidecar (C4 deliverable 3). The producing stage is
            // MERGED into whatever provenance the bundle already carries (e.g.
            // reconstituted from cache, or accumulated upstream) rather than replacing
            // it, so carried occurrences/units are never dropped; `register_unit`
            // dedups by name so re-stamping a cache-restored unit is idempotent.
            // Stamping AFTER the cache `put` keeps the persisted product's cache-key
            // digest stable, and `combined()` still folds sorted `(id, digest)` — the
            // digest is the value the product was cached under.
            let mut stage_prov = r.product.bundle.provenance().clone();
            register_stage_unit(&mut stage_prov, &r.id, stage.capabilities());
            set_bundle_provenance(&mut r.product.bundle, stage_prov);
            if r.elapsed_ms > level_max {
                level_max = r.elapsed_ms;
                level_max_id = r.id.clone();
            }
            stage_timings.push(StageTiming {
                level: level_idx,
                stage_id: r.id.clone(),
                elapsed_ms: r.elapsed_ms,
                cached: r.cached,
                cache_outcome: r.cache_outcome,
                cache_read_bytes: r.cache_read_bytes,
                cache_write_bytes: r.cache_write_bytes,
            });
            stage_phase_timings.extend(r.timings.drain(..).map(|timing| StagePhaseTiming {
                stage_id: r.id.clone(),
                phase: timing.phase,
                elapsed_ms: timing.elapsed_ms,
                metadata: timing.metadata,
            }));
            products.insert(r.id, r.product);
        }
        level_timings.push(LevelTiming {
            level: level_idx,
            elapsed_ms: level_max,
            critical_stage: level_max_id,
        });

        // Once the last span-table consumer's level has committed, STRIP the source-span
        // blob from the stage-source-load product: every later stage that calls
        // `span_index()` now HARD-fails, and the shippable bundle the sink assembles from
        // this product never carries the span table. Deterministic and idempotent — the
        // stripped digest is a pure function of the source-load product, so a warm-cache
        // run reproduces it. (source-load is at level 0, so it is committed well before any
        // consumer level; the guard is defensive.)
        if Some(level_idx) == span_drop_level
            && let Some(product) = products.get("stage-source-load")
        {
            let stripped = crate::bundle::strip_rep_blob(
                product.bundle(),
                crate::stages::carrier::REP_SPAN_TABLE,
            )?;
            let stage_id = product.stage_id.clone();
            products.insert(
                stage_id.clone(),
                StageProduct::from_bundle(stage_id, Arc::new(stripped)),
            );
        }

        // ── Drop-after-last-carrier-consumer, generalized to the WHOLE carrier ──
        // Every stage whose LAST declared carrier reader ran in this level can now shed
        // the transport lanes: `exec_stage` still hands later artifact-only consumers the
        // same declared product, but only its COMMITTED byte-artifact residue remains.
        // Dataset, typed handles, blob records, provenance, and internal `pipeline/`
        // artifacts are released; `digest` remains verbatim so `combined()` is
        // byte-identical to a retain-all run. A caller may preserve an exact intermediate
        // carrier through `retained_carriers` for a post-run proof without retaining the
        // rest of the DAG.
        if ctx.carrier_retention == CarrierRetention::DropAfterLastConsumer {
            for (stage_id, drop_level) in &carrier_drop_level {
                if *drop_level != level_idx || ctx.retained_carriers.contains(stage_id) {
                    continue;
                }
                let Some(product) = products.remove(stage_id.as_str()) else {
                    continue;
                };
                products.insert(stage_id.clone(), product.into_carrier_released()?);
            }
        }
    }

    if profile {
        let floor: u128 = level_timings.iter().map(|l| l.elapsed_ms).sum();
        let total: u128 = stage_timings.iter().map(|t| t.elapsed_ms).sum();
        tracing::info!(
            target: "pipeline_timing",
            stages = stage_timings.len(),
            levels = level_timings.len(),
            summed_ms = total,
            level_barrier_floor_ms = floor,
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
        products,
        combined_digest,
        stage_timings,
        stage_phase_timings,
        level_timings,
        stage_receipts,
        receipt_root,
        ledger: run_ledger,
    })
}

/// For each stage that ANY other stage logically consumes, the topological level of its
/// LAST carrier consumer — or its own production level when all later consumers are
/// artifact-only. This is the level after whose commit its carrier is provably dead.
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
                stage: stage.id().to_string(),
                message: format!("resource lock {resource} poisoned: {e}"),
            })
        })?);
    }
    // Carrier-scale serializers share this permit because each owns a whole-document
    // buffer. Once a sibling releases the permit its Rust temporaries are dead, but a
    // cold glibc arena may still retain those pages. Reclaim at the handoff before the
    // next serializer allocates, while the common permit proves no competing serializer
    // is live. This scales with the declared resource, never a fixed thread cap or stage
    // name list.
    if resources.contains(&SERIALIZATION_BUFFER_RESOURCE) {
        reclaim_allocator_slack_before_serialization();
    }
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
    })
}

/// The set of named-graph IRIs a product's carrier bundle carries.
fn product_graphs(product: &StageProduct) -> std::collections::BTreeSet<String> {
    product
        .bundle()
        .dataset()
        .owned_named_graphs()
        .filter_map(|t| match t {
            purrdf::RdfTerm::Iri(iri) => Some(iri),
            _ => None,
        })
        .collect()
}

/// The content identities carried under each blob-representation lane label in a
/// product's carrier bundle (`representation`-keyed by-reference blob records — NOT
/// the byte-artifact lane). A representation label is the lane; its content digest
/// distinguishes this product's record from a different producer's record on the same
/// shared lane (notably each diagnostics producer's `diagnostics:nodes` contribution).
fn product_blob_records(
    product: &StageProduct,
) -> BTreeMap<String, std::collections::BTreeSet<String>> {
    let mut records: BTreeMap<String, std::collections::BTreeSet<String>> = BTreeMap::new();
    for record in &product.bundle().lookaside().blobs {
        if let Some(representation) = &record.representation {
            records
                .entry(representation.clone())
                .or_default()
                .insert(record.digest.clone());
        }
    }
    records
}

fn product_artifact_records(product: &StageProduct) -> BTreeMap<String, BTreeSet<String>> {
    let mut records: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for resource in &product.bundle().lookaside().resources {
        if let (Some(name), Some(digest)) = (&resource.name, &resource.content_digest) {
            records
                .entry(name.clone())
                .or_default()
                .insert(digest.clone());
        }
    }
    records
}

fn product_handle_records(product: &StageProduct) -> BTreeMap<String, String> {
    product
        .bundle()
        .handles()
        .iter()
        .map(|(graph, entry)| (graph.clone(), entry.content_digest.to_hex()))
        .collect()
}

/// HARD-fail if a stage's ACTUAL attach delta diverges from its DECLARED attach set,
/// in either direction. The delta is "what this stage attaches" = the named graphs /
/// blob-rep lanes present in its OUTPUT product bundle but NOT in its effective INPUT.
/// For named graphs, that input honors `consumed_entities()`: a typed producer contributes
/// only the graph entities this stage actually reads, while an untyped producer contributes
/// its whole product. Blob records remain whole-product inputs because there is no typed
/// blob-consumption declaration, but their identity is `(representation, content digest)`:
/// two producers may each attach distinct content under the same shared lane label. The
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
    // A producer absent from the declaration remains a whole-product dependency. Blob
    // records have no typed consumption lane, so they remain the union over every consumed
    // upstream product. Record identity includes the payload digest: sharing a representation
    // label does not make two producers' distinct payloads equal.
    let entities: BTreeMap<&str, &[String]> = stage
        .consumed_entities()
        .iter()
        .map(|(producer, ents)| (producer.as_str(), ents.as_slice()))
        .collect();
    let mut input_graphs: BTreeSet<String> = BTreeSet::new();
    let mut input_blob_records: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut input_artifacts: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut input_handles: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
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
        for (representation, digests) in product_blob_records(up) {
            input_blob_records
                .entry(representation)
                .or_default()
                .extend(digests);
        }
        for (name, digests) in product_artifact_records(up) {
            input_artifacts.entry(name).or_default().extend(digests);
        }
        for (graph, digest) in product_handle_records(up) {
            input_handles.entry(graph).or_default().insert(digest);
        }
    }

    let delta_graphs: BTreeSet<String> = product_graphs(product)
        .difference(&input_graphs)
        .cloned()
        .collect();
    let delta_blob_reps: BTreeSet<String> = product_blob_records(product)
        .into_iter()
        .filter_map(|(representation, output_digests)| {
            let already_present = input_blob_records
                .get(&representation)
                .is_some_and(|input_digests| output_digests.is_subset(input_digests));
            (!already_present).then_some(representation)
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
    let delta_artifacts = product_artifact_records(product)
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
trait ActionIdentity {
    fn producer_id(&self) -> &str;
    fn product_digest(&self) -> &str;
    fn entity_digest(&self, entity: &str) -> Option<String>;
}

impl ActionIdentity for StageProduct {
    fn producer_id(&self) -> &str {
        &self.stage_id
    }

    fn product_digest(&self) -> &str {
        &self.digest
    }

    fn entity_digest(&self, entity: &str) -> Option<String> {
        product_graphs(self)
            .contains(entity)
            .then(|| self.bundle().graph_digest(entity).to_hex())
    }
}

impl ActionIdentity for StageReceipt {
    fn producer_id(&self) -> &str {
        &self.context.stage_id
    }

    fn product_digest(&self) -> &str {
        &self.product_digest
    }

    fn entity_digest(&self, entity: &str) -> Option<String> {
        self.graphs
            .iter()
            .find(|row| row.identity == entity)
            .map(|row| row.digest.clone())
    }
}

/// Build the scheduler's single typed action-key authority from either live products
/// or authenticated receipts. Producer and entity identity remain attached to every
/// digest; raw inputs remain separate path/digest rows.
fn action_key_context_from_identities<T: ActionIdentity>(
    stage: &dyn Stage,
    root: &Path,
    upstream: &BTreeMap<String, T>,
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
                    let digest = identity.entity_digest(entity).ok_or_else(|| {
                        gmeow_errors::Diag::of_kind(crate::error::StageFailed {
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

    let raw_inputs = input_file_digests(stage, root)?;
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
    action_key_context_from_identities(stage, root, upstream)
}

/// Build the identical action context from authenticated upstream receipts, without
/// hydrating their datasets or handles.
pub fn action_key_context_from_receipts(
    stage: &dyn Stage,
    root: &Path,
    upstream: &BTreeMap<String, StageReceipt>,
) -> Result<StageKeyContext, gmeow_errors::Diag> {
    action_key_context_from_identities(stage, root, upstream)
}

/// Each declared raw input as a repo-relative path plus content digest. A missing
/// input hard-fails; it is never silently treated as unchanged.
pub(crate) fn input_file_digests(
    stage: &dyn Stage,
    root: &Path,
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
        let digest = digest_input_file(path).map_err(|e| {
            gmeow_errors::Diag::of_kind(crate::error::StageFailed {
                stage: stage.id().to_string(),
                message: format!("declared input file {} could not be read: {e}", rel),
            })
        })?;
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
    let mut buffer = [0_u8; 1024 * 1024];
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

#[cfg(test)]
mod digest_tests {
    use std::io::Write as _;

    #[test]
    fn streamed_input_digest_matches_the_action_key_framing_across_chunks() {
        let mut file = tempfile::NamedTempFile::new().expect("temporary input");
        let bytes = vec![0xa5_u8; 2 * 1024 * 1024 + 17];
        file.write_all(&bytes).expect("write multi-chunk input");
        file.flush().expect("flush input");
        assert_eq!(
            super::digest_input_file(file.path()).expect("stream digest"),
            crate::cache::content_digest(&[&bytes]),
        );
    }
}
