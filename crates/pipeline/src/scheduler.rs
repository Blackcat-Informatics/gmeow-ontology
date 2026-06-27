// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The level-parallel scheduler + the [`RunContext`] (#861 P2).
//!
//! Stages run by topological level (from [`crate::graph::StageGraph`]); within a
//! level, independent stages run in parallel (rayon), except `Reason` stages,
//! which serialize under the process-wide [`ENGINE_LOCK`] because the underlying
//! Nemo/Scryer engines hold their own global locks. Each stage's product is
//! content-addressed and memoized in the [`PipelineCache`]; the final result is
//! keyed by stage id (a `BTreeMap`) and folded into one order-independent
//! `combined_digest`, so a run is byte-identical regardless of completion order
//! — the determinism the P2 tests pin.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock, Mutex};

use gmeow_rdf::provenance::DatasetProvenance;
use rayon::prelude::*;

use crate::cache::{content_digest, stage_key, PipelineCache};
use crate::error::PipelineError;
use crate::graph::StageGraph;
use crate::node::{Stage, StageInput, StageProduct};
use crate::provenance::register_stage_unit;

/// Serializes execution of every `Reason` stage. Mirrors the `CHASE_LOCK` in
/// `gmeow-logic` (the Nemo/Scryer engines are not concurrency-safe). A permit,
/// not data — results are returned, never stored behind the lock.
pub static ENGINE_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

/// The shared state of one pipeline run: the repo root, the parallelism budget,
/// the content-addressed cache, and the provenance sidecar stages stamp into.
pub struct RunContext {
    /// The repository root the build operates over.
    pub root: PathBuf,
    /// Maximum concurrent stages within a level (rayon pool size).
    pub jobs: usize,
    /// The persistent, self-verifying per-stage cache.
    pub cache: PipelineCache,
    /// The provenance sidecar: one unit per stage (kind-derived origin).
    pub provenance: DatasetProvenance,
}

impl RunContext {
    /// Construct a run context rooted at `root` with `jobs` parallelism, opening
    /// the cache under `generated/.pipeline-cache/`.
    pub fn open(root: impl Into<PathBuf>, jobs: usize) -> Result<Self, PipelineError> {
        let root = root.into();
        let cache = PipelineCache::open(PipelineCache::default_dir(&root))?;
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
    /// The full-build entry point ([`crate::run::run_full`]) uses this so the
    /// build is deterministic w.r.t. the CURRENT code: the persistent cache keys
    /// stages by `stage_id + impl_version + upstream_digests`, so a stage whose
    /// Rust implementation changed (e.g. across a rebase) without a bumped
    /// `impl_version` would otherwise be served a STALE pre-change product — a
    /// false-parity / false-drift source for the gate. Per-level memoization
    /// within the single run still applies (the temp cache is populated and read
    /// during the run); only cross-invocation reuse is dropped.
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
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunResult {
    /// Each stage's product, keyed by stage id (sorted).
    pub products: BTreeMap<String, StageProduct>,
    /// The hex SHA-256 over the sorted `(id, product-digest)` pairs.
    pub combined_digest: String,
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
    let mut stage_timings: Vec<(String, u128, bool)> = Vec::new();

    for level in &graph.levels {
        // Parallel phase: every stage in the level runs concurrently; Reason
        // stages serialize internally under the ENGINE_LOCK. `products` and
        // `cache` are read-only here — siblings in one level never depend on
        // each other, so no stage can hit another's same-level cache write.
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
        for r in runs {
            if !r.cached {
                ctx.cache.put(&r.key, &r.product)?;
            }
            let stage = by_id[r.id.as_str()];
            register_stage_unit(&mut ctx.provenance, &r.id, stage.kind());
            if profile {
                stage_timings.push((r.id.clone(), r.elapsed_ms, r.cached));
            }
            products.insert(r.id, r.product);
        }
    }

    if profile {
        stage_timings.sort_by_key(|t| std::cmp::Reverse(t.1));
        let total: u128 = stage_timings.iter().map(|t| t.1).sum();
        eprintln!(
            "[pipeline-timing] {} stages, summed {total} ms:",
            stage_timings.len()
        );
        for (id, ms, cached) in stage_timings.iter().take(25) {
            eprintln!(
                "[pipeline-timing]   {ms:>7} ms  {id}{}",
                if *cached { " (cached)" } else { "" }
            );
        }
    }

    let combined_digest = combined(&products);
    Ok(RunResult {
        products,
        combined_digest,
    })
}

/// Execute one stage: assemble its upstream inputs, consult the cache, and run it
/// (under the engine lock when its kind requires) on a miss.
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

    // Cache key = id ++ impl_version ++ sorted(upstream digests) ++ the content
    // digest of any RAW source files the stage declares via `input_files` (export
    // leaves that read non-fold sources — references.ttl, the eval corpus, the
    // slice manifests — declare them there so a source change busts the cache;
    // cache soundness for stages that legitimately consume nothing, #861/#863).
    let mut up_digests: Vec<String> = upstream.values().map(|p| p.digest.clone()).collect();
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
    let out = if stage.kind().carries_engine_lock() {
        let _guard = ENGINE_LOCK.lock().map_err(|e| PipelineError::Stage {
            stage: stage.id().to_string(),
            message: format!("ENGINE_LOCK poisoned: {e}"),
        })?;
        stage.run(input)?
    } else {
        stage.run(input)?
    };

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
