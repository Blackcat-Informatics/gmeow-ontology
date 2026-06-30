// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The level-parallel scheduler + the [`RunContext`] (#861 P2).
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

use gmeow_rdf::provenance::DatasetProvenance;
use rayon::prelude::*;

use crate::bundle::set_bundle_provenance;
use crate::cache::{content_digest, stage_key, PipelineCache};
use crate::error::PipelineError;
use crate::graph::StageGraph;
use crate::node::{Stage, StageInput, StageProduct};
use crate::provenance::register_stage_unit;

/// The process-wide registry of per-resource mutexes. A stage that declares a
/// `gmeow:requiresResource` acquires that resource's permit before running, so two
/// stages competing for the same resource serialize. A permit, not data — results
/// are returned, never stored behind the lock. The engine resource
/// ([`crate::node::ENGINE_RESOURCE`]) mirrors the `CHASE_LOCK` in `gmeow-logic`
/// (the Nemo/Scryer engines are not concurrency-safe); any other shared resource a
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
    pub fn open(root: impl Into<PathBuf>, jobs: usize) -> Result<Self, PipelineError> {
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
    pub fn open_ephemeral(root: impl Into<PathBuf>, jobs: usize) -> Result<Self, PipelineError> {
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
/// `combined_digest` (a `String`), which tests compare directly (#1132 C4).
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
}

/// Run a validated, bound pipeline. `bound` is the stages in topological order
/// (from [`crate::loader::bind`]); `graph` provides the parallel levels.
pub fn run(
    graph: &StageGraph,
    bound: &[Arc<dyn Stage>],
    ctx: &mut RunContext,
) -> Result<RunResult, PipelineError> {
    let by_id: BTreeMap<&str, &Arc<dyn Stage>> = bound.iter().map(|s| (s.id(), s)).collect();

    // A local rayon pool honours the jobs budget without touching the global one.
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(ctx.jobs)
        .build()
        .map_err(|e| PipelineError::Stage {
            stage: "<scheduler>".to_string(),
            message: format!("failed to build rayon pool: {e}"),
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
                .map(|id| -> Result<StageRun, PipelineError> {
                    let stage = by_id.get(id.as_str()).ok_or_else(|| PipelineError::Stage {
                        stage: id.clone(),
                        message: "stage in graph was not bound".to_string(),
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
    }

    if profile {
        let floor: u128 = level_timings.iter().map(|l| l.elapsed_ms).sum();
        let total: u128 = stage_timings.iter().map(|t| t.elapsed_ms).sum();
        eprintln!(
            "[pipeline-timing] {} stages over {} levels; summed {total} ms; level-barrier floor {floor} ms",
            stage_timings.len(),
            level_timings.len(),
        );
        let mut slowest = stage_timings.clone();
        slowest.sort_by_key(|t| std::cmp::Reverse(t.elapsed_ms));
        for timing in slowest.iter().take(25) {
            eprintln!(
                "[pipeline-timing]   {ms:>7} ms  {id}{cached}",
                ms = timing.elapsed_ms,
                id = timing.stage_id,
                cached = if timing.cached { " (cached)" } else { "" }
            );
        }
        eprintln!("[pipeline-timing] per-level critical stage:");
        for timing in &level_timings {
            eprintln!(
                "[pipeline-timing]   level {idx:>2}: {ms:>7} ms  {id}",
                idx = timing.level,
                ms = timing.elapsed_ms,
                id = timing.critical_stage
            );
        }
    }

    let combined_digest = combined(&products);
    Ok(RunResult {
        products,
        combined_digest,
        stage_timings,
        level_timings,
    })
}

/// Execute one stage: assemble its upstream inputs, consult the cache, and run it
/// (holding any resource it requires exclusively) on a miss.
fn exec_stage(
    stage: &dyn Stage,
    root: &Path,
    products: &BTreeMap<String, StageProduct>,
    cache: &PipelineCache,
) -> Result<StageRun, PipelineError> {
    // Assemble exactly the upstream products this stage declared.
    let mut upstream: BTreeMap<String, StageProduct> = BTreeMap::new();
    for dep in stage.consumes() {
        let p = products.get(dep).ok_or_else(|| PipelineError::Stage {
            stage: stage.id().to_string(),
            message: format!("missing upstream product {dep}"),
        })?;
        upstream.insert(dep.clone(), p.clone());
    }

    // Cache key = build fingerprint ++ id ++ impl_version ++ sorted(upstream digests)
    // ++ the content digest of any RAW source files the stage declares via `input_files`
    // (export leaves that read non-fold sources — references.ttl, the eval corpus, the
    // slice manifests — declare them there so a source change busts the cache;
    // cache soundness for stages that legitimately consume nothing, #861/#863).
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
        return Ok(StageRun {
            id: stage.id().to_string(),
            key,
            product,
            cached: true,
            elapsed_ms: started.elapsed().as_millis(),
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
        _guards.push(lock.lock().map_err(|e| PipelineError::Stage {
            stage: stage.id().to_string(),
            message: format!("resource lock {resource} poisoned: {e}"),
        })?);
    }
    let out = stage.run(input)?;
    drop(_guards);

    Ok(StageRun {
        id: stage.id().to_string(),
        key,
        product: out.product,
        cached: false,
        elapsed_ms: started.elapsed().as_millis(),
    })
}

/// The content digest of a stage's declared raw `input_files`, or `None` when it
/// declares none (so the cache key is unchanged for the common case). The digest
/// folds each file's repo-relative logical path AND its bytes (sorted by path, so
/// it is order-independent); a declared file that cannot be read HARD-fails — a
/// missing required input is never silently treated as "unchanged" (no-optionality).
fn input_files_digest(stage: &dyn Stage, root: &Path) -> Result<Option<String>, PipelineError> {
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
        let content = std::fs::read(path).map_err(|e| PipelineError::Stage {
            stage: stage.id().to_string(),
            message: format!("declared input file {} could not be read: {e}", rel),
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
