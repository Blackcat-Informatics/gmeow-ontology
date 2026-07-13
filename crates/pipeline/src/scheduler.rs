// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The level-parallel scheduler + the [`RunContext`] (P2).
//!
//! Stages run by topological level (from [`crate::graph::StageGraph`]); within a
//! level, independent stages run in parallel (rayon). A stage that declares a
//! shared resource (`gmeow:requiresResource`, e.g. the reasoning stage's
//! [`crate::node::ENGINE_RESOURCE`]) holds it exclusively while it runs, so two
//! stages competing for the same resource serialize — the declarative
//! replacement for a hardcoded engine mutex. Each stage's product is
//! content-addressed and memoized in the [`PipelineCache`]; the final result is
//! keyed by stage id (a `BTreeMap`) and folded into one order-independent
//! `combined_digest`, so a run is byte-identical regardless of completion order
//! — the determinism the P2 tests pin.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock, Mutex};

use purrdf::provenance::DatasetProvenance;
use rayon::prelude::*;

use crate::bundle::set_bundle_provenance;
use crate::cache::{PipelineCache, content_digest, stage_key};
use crate::graph::StageGraph;
use crate::node::{Stage, StageInput, StageProduct};
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

/// The shared state of one pipeline run: the repo root, the parallelism budget,
/// the content-addressed cache, and the provenance sidecar stages stamp into.
pub struct RunContext {
    /// The repository root the build operates over.
    pub root: PathBuf,
    /// Maximum concurrent stages within a level (rayon pool size).
    pub jobs: usize,
    /// The persistent, self-verifying per-stage cache.
    pub cache: PipelineCache,
    /// The provenance sidecar: one unit per stage (capability-derived origin).
    pub provenance: DatasetProvenance,
}

impl RunContext {
    /// Construct a run context rooted at `root` with `jobs` parallelism, opening the
    /// persistent cache under `generated/.pipeline-cache/<build-fingerprint>/`.
    ///
    /// The cache is namespaced by [`crate::cache::BUILD_FINGERPRINT`] and any SIBLING
    /// fingerprint directory is garbage-collected on open. Because every cache key also
    /// embeds the fingerprint, a code/dependency/toolchain change orphans the whole
    /// prior cache; GC-ing it bounds disk to a single build's products instead of
    /// growing unbounded across edits.
    pub fn open(root: impl Into<PathBuf>, jobs: usize) -> Result<Self, gmeow_errors::Diag> {
        let root = root.into();
        let base = PipelineCache::default_dir(&root);
        let fp = &crate::cache::BUILD_FINGERPRINT[..16];
        // GC stale fingerprint namespaces (best-effort: a missing base dir is fine).
        if let Ok(entries) = std::fs::read_dir(&base) {
            for entry in entries.flatten() {
                if entry.file_name().to_string_lossy() != *fp {
                    let _ = std::fs::remove_dir_all(entry.path());
                }
            }
        }
        let cache = PipelineCache::open(base.join(fp))?;
        Ok(Self {
            root,
            jobs: jobs.max(1),
            cache,
            provenance: DatasetProvenance::new(),
        })
    }

    /// Construct a run context whose cache lives in a FRESH, process-unique temp
    /// directory rather than the persistent `generated/.pipeline-cache/`.
    ///
    /// Used by TESTS that want a clean, isolated cache per run (no cross-test or
    /// cross-invocation reuse). The full build ([`crate::run::run_full`]) instead uses
    /// the persistent [`Self::open`] cache: that is safe because every `stage_key`
    /// folds [`crate::cache::BUILD_FINGERPRINT`], so a changed Rust impl (here or in
    /// any workspace crate) yields a fresh key and recomputes — the stale-serve hazard
    /// that once forced an ephemeral full build no longer exists.
    pub fn open_ephemeral(
        root: impl Into<PathBuf>,
        jobs: usize,
    ) -> Result<Self, gmeow_errors::Diag> {
        let root = root.into();
        // A process- + nanosecond-unique cache dir under the system temp root, so
        // concurrent runs never collide and nothing leaks into the repo tree.
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let dir = std::env::temp_dir().join(format!(
            "gmeow-pipeline-cache-{}-{}",
            std::process::id(),
            nonce
        ));
        let cache = PipelineCache::open(dir)?;
        Ok(Self {
            root,
            jobs: jobs.max(1),
            cache,
            provenance: DatasetProvenance::new(),
        })
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
    /// Per-level critical-stage timings in topological level order.
    pub level_timings: Vec<LevelTiming>,
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
    key: String,
    product: StageProduct,
    cached: bool,
    /// Wall-clock spent in [`exec_stage`] for this stage (compute + cache probe).
    elapsed_ms: u128,
    /// This stage's FORWARD-projected diagnostic nodes: `out.diags` on a fresh run,
    /// or the cache-restored `diagnostics:nodes` blob on a hit. Replayed into the
    /// run-wide ledger in the sequential commit phase.
    diags: Vec<gmeow_errors::DiagNode>,
}

/// Run a validated, bound pipeline. `bound` is the stages in topological order
/// (from [`crate::loader::bind`]); `graph` provides the parallel levels.
pub fn run(
    graph: &StageGraph,
    bound: &[Arc<dyn Stage>],
    ctx: &mut RunContext,
) -> Result<RunResult, gmeow_errors::Diag> {
    let by_id: BTreeMap<&str, &Arc<dyn Stage>> = bound.iter().map(|s| (s.id(), s)).collect();

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
    // (level_index, slowest-stage ms in the level, slowest-stage id): the sum of the
    // per-level maxima is the critical-path floor the level-barrier scheduler imposes.
    let mut level_timings: Vec<LevelTiming> = Vec::new();
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

    for (level_idx, level) in graph.levels.iter().enumerate() {
        // Parallel phase: every stage in the level runs concurrently; stages that
        // declare a shared resource serialize internally on that resource's permit.
        // `products` and `cache` are read-only here — siblings in one level never
        // depend on each other, so no stage can hit another's same-level cache write.
        let root: &Path = &ctx.root;
        let cache = &ctx.cache;
        let runs: Vec<StageRun> = pool.install(|| {
            level
                .par_iter()
                .map(|id| -> Result<StageRun, gmeow_errors::Diag> {
                    let stage = by_id.get(id.as_str()).ok_or_else(|| {
                        gmeow_errors::Diag::of_kind(crate::error::StageFailed {
                            stage: id.clone(),
                            message: "stage in graph was not bound".to_string(),
                        })
                    })?;
                    exec_stage(stage.as_ref(), root, &products, cache)
                })
                .collect::<Result<Vec<_>, _>>()
        })?;

        // Sequential commit phase: persist cache entries, stamp provenance, and
        // publish products for the next level.
        let mut level_max: u128 = 0;
        let mut level_max_id = String::new();
        for mut r in runs {
            if !r.cached {
                ctx.cache.put(&r.key, &r.product)?;
            }
            // Fold this stage's forward diagnostic nodes into the run-wide ledger.
            run_ledger.replay(std::mem::take(&mut r.diags));
            let stage = by_id[r.id.as_str()];
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
            });
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
    Ok(RunResult {
        products,
        combined_digest,
        stage_timings,
        level_timings,
        ledger: run_ledger,
    })
}

/// Execute one stage: assemble its upstream inputs, consult the cache, and run it
/// (holding any resource it requires exclusively) on a miss.
fn exec_stage(
    stage: &dyn Stage,
    root: &Path,
    products: &BTreeMap<String, StageProduct>,
    cache: &PipelineCache,
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

    // Cache key = build fingerprint ++ id ++ impl_version ++ sorted(upstream digests)
    // ++ the content digest of any RAW source files the stage declares via `input_files`
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
    let entities: BTreeMap<&str, &[String]> = stage
        .consumed_entities()
        .iter()
        .map(|(producer, ents)| (producer.as_str(), ents.as_slice()))
        .collect();
    let mut up_digests: Vec<String> = Vec::new();
    for (dep, product) in &upstream {
        match entities.get(dep.as_str()) {
            Some(ents) if !ents.is_empty() => {
                // Fold each consumed named graph's canonical content digest.
                for graph in *ents {
                    up_digests.push(product.bundle().graph_digest(graph).to_hex());
                }
            }
            _ => up_digests.push(product.digest.clone()),
        }
    }
    up_digests.sort();
    let source_digest = input_files_digest(stage, root)?;
    let key = stage_key(
        stage.id(),
        stage.impl_version(),
        &up_digests,
        source_digest.as_deref(),
    );

    let started = std::time::Instant::now();

    if let Some(product) = cache.get(&key)? {
        // A cache hit re-serves the identical product, so its `diagnostics:nodes` blob
        // (empty for a non-producer) recovers this stage's run-ledger contribution
        // WITHOUT re-running it — byte-identical to the fresh `out.diags`.
        // The attach-drift check MUST fire here too: a declaration edit need not bump
        // `impl_version`, so a stale cached product with drifted declarations would sail
        // through unless the compare runs on the returned product in BOTH branches.
        verify_attach_drift(stage, &upstream, &product)?;
        let diags = product.diag_nodes();
        return Ok(StageRun {
            id: stage.id().to_string(),
            key,
            product,
            cached: true,
            elapsed_ms: started.elapsed().as_millis(),
            diags,
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
    let out = stage.run(input)?;
    drop(_guards);

    // Verify the stage's ACTUAL attach delta against its declaration on the cache-miss
    // path too — the same compare as the cache-hit branch, so drift HARD-fails regardless
    // of whether the product came fresh or from cache.
    verify_attach_drift(stage, &upstream, &out.product)?;

    Ok(StageRun {
        id: stage.id().to_string(),
        key,
        product: out.product,
        cached: false,
        elapsed_ms: started.elapsed().as_millis(),
        diags: out.diags,
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
fn verify_attach_drift(
    stage: &dyn Stage,
    upstream: &BTreeMap<String, StageProduct>,
    product: &StageProduct,
) -> Result<(), gmeow_errors::Diag> {
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
    Ok(())
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

/// The content digest of a stage's declared raw `input_files`, or `None` when it
/// declares none (so the cache key is unchanged for the common case). The digest
/// folds each file's repo-relative logical path AND its bytes (sorted by path, so
/// it is order-independent); a declared file that cannot be read HARD-fails — a
/// missing required input is never silently treated as "unchanged" (no-optionality).
fn input_files_digest(
    stage: &dyn Stage,
    root: &Path,
) -> Result<Option<String>, gmeow_errors::Diag> {
    let mut files = stage.input_files(root)?;
    if files.is_empty() {
        return Ok(None);
    }
    files.sort();
    files.dedup();
    let mut rels: Vec<Vec<u8>> = Vec::with_capacity(files.len());
    let mut bytes: Vec<Vec<u8>> = Vec::with_capacity(files.len());
    for path in &files {
        let rel = path
            .strip_prefix(root)
            .unwrap_or(path)
            .to_string_lossy()
            .into_owned();
        let content = std::fs::read(path).map_err(|e| {
            gmeow_errors::Diag::of_kind(crate::error::StageFailed {
                stage: stage.id().to_string(),
                message: format!("declared input file {} could not be read: {e}", rel),
            })
        })?;
        rels.push(rel.into_bytes());
        bytes.push(content);
    }
    let mut fields: Vec<&[u8]> = Vec::with_capacity(files.len() * 2);
    for (rel, content) in rels.iter().zip(bytes.iter()) {
        fields.push(rel.as_slice());
        fields.push(content.as_slice());
    }
    Ok(Some(content_digest(&fields)))
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
