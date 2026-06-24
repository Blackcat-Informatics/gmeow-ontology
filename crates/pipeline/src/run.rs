// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The full-build entry point (#861 P6 integration): [`run_full`] runs the WHOLE
//! dogfooded DAG single-pass and either WRITES every produced artifact to disk
//! (regenerate mode) or COMPARES each against the committed bytes and collects
//! drift [`Finding`]s (check mode).
//!
//! # The single-pass property
//!
//! The fold-reading export leaves (lpg, metadata, export, okf, parquet) consume
//! the in-memory `stage-snapshot` product — THIS run's freshly-composed
//! `gmeow.gts` — rather than re-reading the committed file from disk. The sole
//! [`crate::stages::gts_sink::GtsSinkStage`] re-emits those snapshot bytes as its
//! product; `run_full` writes them to `generated/dist/gmeow.gts`.
//!
//! # The schemas disk-write barrier
//!
//! The `schemas` leaf shells out to the lane-only Python LinkML toolkit, which
//! reads `generated/dist/gmeow.gts` FROM DISK (`GTS_SNAPSHOT_FILE`). It therefore
//! declares a dataflow dependency on `stage-gts-sink` and cannot run until the
//! fresh fold is on disk. `run_full` honours this by running the DAG in two
//! phases: phase 1 runs everything up to and including the sink; in regenerate
//! mode the fresh `gmeow.gts` is written to disk between phases; phase 2 then runs
//! the `schemas` tail (which reads the now-fresh on-disk fold). In check mode the
//! committed on-disk fold is already the reference, so no write is needed.

use std::collections::BTreeMap;
use std::path::Path;

use gmeow_diagnostics::{Finding, Severity};

use crate::error::PipelineError;
use crate::loader::{bind, PipelineSpec, StageSpec};
use crate::node::{StageKind, StageProduct};
use crate::registry::default_registry;
use crate::scheduler::{run, RunContext};

/// The id of the stage whose Python toolkit reads the on-disk fold — the only
/// stage that must run AFTER the sink writes `gmeow.gts`.
const SCHEMAS_STAGE: &str = "stage-export-schemas";
/// The sole serialization exit; its product carries the `gmeow.gts` bytes.
const SINK_STAGE: &str = "stage-gts-sink";
/// The committed fold path the sink writes / schemas reads.
const GTS_PATH: &str = "generated/dist/gmeow.gts";

/// Whether `run_full` writes artifacts to disk (regenerate) or compares them to
/// the committed bytes and reports drift (check).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunMode {
    /// Write every produced artifact to `root.join(path)` (regenerate).
    Regenerate,
    /// Compare every produced artifact to the committed bytes, collecting drift.
    Check,
}

/// The outcome of a [`run_full`]: how many artifacts were produced / reproduced
/// and any drift findings (check mode) or write errors.
#[derive(Debug, Clone)]
pub struct RunReport {
    /// The run mode.
    pub mode: RunMode,
    /// Total committed-artifact paths the run produced.
    pub produced: usize,
    /// Paths that reproduced byte-for-byte (check) / were written (regenerate).
    pub reproduced: usize,
    /// Drift / write findings (empty ⇒ full parity).
    pub findings: Vec<Finding>,
    /// The drifted logical paths (check mode), sorted.
    pub drifted: Vec<String>,
}

impl RunReport {
    /// Whether the run reproduced every committed artifact with zero drift.
    pub fn is_clean(&self) -> bool {
        self.findings.is_empty() && self.drifted.is_empty()
    }
}

/// Build the authoritative full `PipelineSpec` — the executable twin of the
/// dogfooded `gmeow:pipeline-build` DAG in `slices/core/pipeline/module.ttl`.
///
/// Every `impl_key` here is a `default_registry()` key and every `consumes` set
/// matches the bound Rust [`crate::node::Stage::consumes`] exactly (the loader's
/// `bind` proves this). The slice `module.ttl` mirrors this graph as data; this
/// Rust spec is the one the run executes.
pub fn full_spec() -> PipelineSpec {
    // ── the spine ──
    let mut stages = vec![
        st(
            "stage-source-load",
            StageKind::SourceLoad,
            "source_load",
            &[],
        ),
        st("stage-statements", StageKind::Transform, "statements", &[]),
        st("stage-mappings", StageKind::Transform, "mappings", &[]),
        st(
            "stage-reason",
            StageKind::Reason,
            "reason",
            &["stage-mappings", "stage-source-load", "stage-statements"],
        ),
        st(
            "stage-gts-compose",
            StageKind::Transform,
            "gts_compose",
            &[
                "stage-mappings",
                "stage-reason",
                "stage-source-load",
                "stage-statements",
            ],
        ),
        st(
            "stage-docs-render",
            StageKind::DocsRender,
            "docs_render",
            &["stage-gts-compose"],
        ),
        st(
            "stage-snapshot",
            StageKind::Transform,
            "snapshot",
            &[
                "stage-docs-render",
                // #700: fold THIS run's fresh JSON Schema/OpenAPI into the bundle.
                "stage-export-json-schema",
                "stage-gts-compose",
                "stage-reason",
                "stage-statements",
            ],
        ),
    ];

    // ── fold-reading export leaves (consume THIS run's snapshot) ──
    for (id, impl_key) in [
        ("stage-export-lpg", "lpg"),
        ("stage-export-logic", "logic"),
        ("stage-export-metadata", "metadata"),
        ("stage-export-export", "export"),
        ("stage-export-okf", "okf"),
        ("stage-export-parquet", "parquet"),
    ] {
        stages.push(st(id, StageKind::ExportLeaf, impl_key, &["stage-snapshot"]));
    }

    // ── source-reading export leaves (independent; read slices/metadata/evals) ──
    for (id, impl_key) in [
        ("stage-export-catalog", "catalog"),
        ("stage-export-profiles", "profiles"),
        ("stage-export-frame-shapes", "frame_shapes"),
        ("stage-export-json-schema", "json_schema"),
        ("stage-export-matrix", "matrix"),
        ("stage-export-apache", "apache"),
        ("stage-export-references", "references"),
        ("stage-export-evals", "evals"),
        ("stage-export-research-objects", "research-objects"),
    ] {
        stages.push(st(id, StageKind::ExportLeaf, impl_key, &[]));
    }

    // ── the single Sink: re-emits the snapshot bytes (the disk-write target) ──
    stages.push(st(
        SINK_STAGE,
        StageKind::Sink,
        "gts_sink",
        &["stage-snapshot"],
    ));

    // ── the schemas tail: depends on the on-disk fold the Sink writes ──
    stages.push(st(
        SCHEMAS_STAGE,
        StageKind::ExportLeaf,
        "schemas",
        &[SINK_STAGE],
    ));

    PipelineSpec {
        id: "pipeline-build".to_string(),
        stages,
    }
}

fn st(id: &str, kind: StageKind, impl_key: &str, consumes: &[&str]) -> StageSpec {
    StageSpec {
        id: id.to_string(),
        kind,
        impl_key: impl_key.to_string(),
        consumes: consumes.iter().map(|s| s.to_string()).collect(),
        engine_lock: kind.carries_engine_lock(),
        formats: Vec::new(),
    }
}

/// Run the FULL dogfooded build single-pass and either write every produced
/// artifact (regenerate) or compare it to the committed bytes (check).
///
/// `jobs` is the per-level parallelism budget. Returns a [`RunReport`]; in check
/// mode `report.is_clean()` is the cutover gate (zero drift across every
/// committed artifact). RDF artifacts compare by bytes (they are byte
/// deterministic); the `gmeow.gts` bundle is compared by the FOLD (see
/// `tests/full_parity.rs`) because CBOR has encoding skew.
pub fn run_full(root: &Path, jobs: usize, mode: RunMode) -> Result<RunReport, PipelineError> {
    let spec = full_spec();

    // Split the DAG at the schemas tail: phase 1 is everything up to (and
    // including) the sink; phase 2 is the schemas leaf, which reads the on-disk
    // fold the sink writes.
    let (pre, tail): (Vec<StageSpec>, Vec<StageSpec>) = spec
        .stages
        .iter()
        .cloned()
        .partition(|s| s.id != SCHEMAS_STAGE);
    let pre_spec = PipelineSpec {
        id: spec.id.clone(),
        stages: pre,
    };

    // ── Phase 1: validate + bind + run everything up to the sink. ──
    let pre_graph = pre_spec.validate()?;
    let registry = default_registry();
    let pre_bound = bind(&pre_spec, &pre_graph, &registry)?;
    // A full single-pass build runs over a FRESH ephemeral cache (never the
    // persistent `generated/.pipeline-cache/`): the persistent cache keys stages
    // by `impl_version`, so a stage whose Rust impl changed without a version bump
    // could be served a stale pre-change product — a false-parity / false-drift
    // source for the cutover gate. Per-level memoization within this run still
    // applies; only cross-invocation reuse is dropped.
    let mut ctx = RunContext::open_ephemeral(root, jobs)?;
    let pre_result = run(&pre_graph, &pre_bound, &mut ctx)?;

    let mut products: BTreeMap<String, StageProduct> = pre_result.products;

    // ── The disk-write barrier: in regenerate mode, write the fresh fold (and
    //    every other phase-1 artifact) to disk so the schemas Python reads the
    //    fresh `gmeow.gts`. In check mode the committed file is already the
    //    reference, so no write happens. ──
    let mut findings: Vec<Finding> = Vec::new();
    let mut drifted: Vec<String> = Vec::new();
    let mut produced = 0usize;
    let mut reproduced = 0usize;

    if mode == RunMode::Regenerate {
        for product in products.values() {
            for path in product.artifacts.keys() {
                // `pipeline/`-prefixed artifacts are the in-memory dataflow
                // (composed.nq / base-graph.nq / documentation.nq) the stages
                // exchange — never committed to disk (mirrors the Check path).
                if path.starts_with("pipeline/") {
                    continue;
                }
                write_artifact(root, path, &product.artifacts[path])?;
            }
        }
    }

    // ── Phase 2: run the schemas tail (it now reads the on-disk fold). It binds
    //    against the sink product produced in phase 1. ──
    if let Some(sink) = products.get(SINK_STAGE).cloned() {
        let tail_spec = PipelineSpec {
            id: spec.id.clone(),
            stages: tail,
        };
        // Bind only the schemas stage; run it directly over the sink product.
        let registry = default_registry();
        for s in &tail_spec.stages {
            let stage =
                registry
                    .get(&s.impl_key)
                    .ok_or_else(|| PipelineError::UnknownStageImpl {
                        stage: s.id.clone(),
                        impl_key: s.impl_key.clone(),
                    })?;
            let mut upstream: BTreeMap<String, StageProduct> = BTreeMap::new();
            upstream.insert(SINK_STAGE.to_string(), sink.clone());
            let out = stage.run(crate::node::StageInput {
                root,
                upstream: &upstream,
            })?;
            products.insert(s.id.clone(), out.product);
        }
    }

    // ── Reconcile every produced artifact against committed / write it. ──
    for product in products.values() {
        for (path, bytes) in &product.artifacts {
            // Internal in-memory dataflow artifacts (under the `pipeline/` logical
            // prefix: base-graph.nq, composed.nq, documentation.nq) are NOT
            // committed outputs — they exist only to pass between stages. Skip.
            if path.starts_with("pipeline/") {
                continue;
            }
            produced += 1;

            // The `gmeow.gts` bundle is compared by the FOLD (per-named-graph quad
            // set + reifier/annotation counts) elsewhere — CBOR has encoding skew
            // (#595). Count it produced; the fold gate is `tests/full_parity.rs`.
            if path == GTS_PATH {
                reproduced += 1;
                continue;
            }

            // `dist/*` artifacts are gitignored runtime outputs with NO committed
            // authority: a fresh checkout (CI `check-generated`) has no `dist/` tree,
            // so they can never be drift-compared. They are WRITTEN in Regenerate but
            // SKIPPED in Check (their reproducibility is covered by the second-run
            // determinism check in `tests/full_parity.rs`).
            if path.starts_with("dist/") {
                if mode == RunMode::Regenerate {
                    write_artifact(root, path, bytes)?;
                }
                reproduced += 1;
                continue;
            }

            if mode == RunMode::Regenerate {
                // Phase-1 products were written above; (re)write every artifact.
                write_artifact(root, path, bytes)?;
                reproduced += 1;
                continue;
            }

            // ── Check mode: compare to the committed bytes. ──
            let committed = match std::fs::read(root.join(path)) {
                Ok(c) => c,
                Err(e) => {
                    drifted.push(path.clone());
                    findings.push(
                        Finding::new(
                            Severity::Error,
                            "pipeline.missing",
                            format!("{path} could not be read for comparison: {e}"),
                        )
                        .with_tool("gmeow-pipeline"),
                    );
                    continue;
                }
            };

            if committed == *bytes {
                reproduced += 1;
                continue;
            }

            // RDF/Turtle/N-Triples/N-Quads leaves are validated against committed
            // by GRAPH ISOMORPHISM (their unit-test contract: serialization
            // formatting is immaterial because the committed files were minted by a
            // DIFFERENT serializer — the retired Python build's rdflib). Compare
            // them isomorphically; byte drift that is isomorphic is NOT a finding.
            if is_rdf_artifact(path) && graphs_isomorphic(&committed, bytes) {
                reproduced += 1;
                continue;
            }

            // A genuine drift.
            drifted.push(path.clone());
            findings.push(
                Finding::new(
                    Severity::Error,
                    "pipeline.drift",
                    format!("{path} differs from the committed artifact"),
                )
                .with_tool("gmeow-pipeline"),
            );
        }
    }

    drifted.sort();
    drifted.dedup();
    Ok(RunReport {
        mode,
        produced,
        reproduced,
        findings,
        drifted,
    })
}

/// Whether `path` is an RDF text artifact compared by graph isomorphism (its
/// committed bytes were serialized by the retired Python build, so byte parity
/// is not expected; the unit tests assert isomorphism, never bytes).
fn is_rdf_artifact(path: &str) -> bool {
    path.ends_with(".ttl") || path.ends_with(".nt") || path.ends_with(".nq")
}

/// Whether two RDF documents (Turtle / N-Triples / N-Quads, by `a`'s extension —
/// both committed and produced share a logical path) are isomorphic: the same set
/// of quads after RDFC-1.0 blank-node canonicalization. Returns false on any
/// parse error (treated as drift).
fn graphs_isomorphic(committed: &[u8], produced: &[u8]) -> bool {
    canonical_quad_set(committed)
        .zip(canonical_quad_set(produced))
        .map(|(c, p)| c == p)
        .unwrap_or(false)
}

/// Parse RDF (lenient, Turtle for `.ttl`, N-Quads otherwise) and return the
/// canonicalized quad set as sorted strings. `None` on a parse error.
fn canonical_quad_set(bytes: &[u8]) -> Option<std::collections::BTreeSet<String>> {
    use oxigraph::io::{RdfFormat, RdfParser};
    use oxigraph::model::dataset::{
        CanonicalizationAlgorithm, CanonicalizationHashAlgorithm, Dataset,
    };
    // Try Turtle first, then N-Quads — the leaves emit one of these.
    for format in [RdfFormat::Turtle, RdfFormat::NQuads] {
        let mut dataset = Dataset::new();
        let mut ok = true;
        for quad in RdfParser::from_format(format).lenient().for_slice(bytes) {
            match quad {
                Ok(q) => {
                    dataset.insert(q.as_ref());
                }
                Err(_) => {
                    ok = false;
                    break;
                }
            }
        }
        if ok && !dataset.is_empty() {
            dataset.canonicalize(CanonicalizationAlgorithm::Rdfc10 {
                hash_algorithm: CanonicalizationHashAlgorithm::Sha256,
            });
            let set: std::collections::BTreeSet<String> =
                dataset.iter().map(|q| format!("{q} .")).collect();
            return Some(set);
        }
    }
    None
}

/// Write `bytes` to `root.join(path)`, creating parent directories.
fn write_artifact(root: &Path, path: &str, bytes: &[u8]) -> Result<(), PipelineError> {
    let target = root.join(path);
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&target, bytes)?;
    Ok(())
}
