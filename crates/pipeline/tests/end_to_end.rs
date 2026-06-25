// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! End-to-end executor test (#861 P3/P6): the DAG-driven executor runs the wired
//! spine — source_load → (statements, mappings) → reason → gts_compose →
//! validate/docs_render → gts_sink — over the real repo, binding every stage
//! against the default registry and serializing `gmeow.gts`. This exercises the
//! whole machinery (DAG validate → bind → level-parallel schedule → ENGINE_LOCK on reason →
//! content-addressed cache → one Sink) on production data, not synthetics. (GTS
//! readability + scheduler determinism are pinned by the crate's unit tests.)

use std::path::{Path, PathBuf};

use gmeow_pipeline::{
    bind, default_registry, run, PipelineCache, PipelineSpec, RunContext, StageKind, StageSpec,
};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .canonicalize()
        .unwrap()
}

fn spec(id: &str, kind: StageKind, impl_key: &str, consumes: &[&str]) -> StageSpec {
    StageSpec {
        id: id.to_string(),
        kind,
        impl_key: impl_key.to_string(),
        consumes: consumes.iter().map(|s| s.to_string()).collect(),
        engine_lock: kind.carries_engine_lock(),
        formats: Vec::new(),
    }
}

/// The implemented spine DAG — each stage's `consumes` matches its Rust impl's
/// `consumes()` exactly (so `bind` agreement holds). The remaining export leaves
/// of the full `gmeow:pipeline-build` DAG are excluded; `stage-export-json-schema`
/// is included because the snapshot now folds its product (#700).
fn spine() -> PipelineSpec {
    PipelineSpec {
        id: "pipeline-spine".to_string(),
        stages: vec![
            spec(
                "stage-source-load",
                StageKind::SourceLoad,
                "source_load",
                &[],
            ),
            spec("stage-statements", StageKind::Transform, "statements", &[]),
            spec("stage-mappings", StageKind::Transform, "mappings", &[]),
            spec(
                "stage-reason",
                StageKind::Reason,
                "reason",
                &["stage-mappings", "stage-source-load", "stage-statements"],
            ),
            spec(
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
            spec(
                "stage-docs-render",
                StageKind::DocsRender,
                "docs_render",
                &["stage-gts-compose"],
            ),
            spec(
                "stage-validate",
                StageKind::Validate,
                "validate",
                &["stage-source-load"],
            ),
            // The SHACL→JSON-Schema source leaf the snapshot folds (#700); a
            // source-reading ExportLeaf that consumes nothing.
            spec(
                "stage-export-json-schema",
                StageKind::ExportLeaf,
                "json_schema",
                &[],
            ),
            spec(
                "stage-snapshot",
                StageKind::Transform,
                "snapshot",
                &[
                    "stage-docs-render",
                    "stage-export-json-schema",
                    "stage-gts-compose",
                    "stage-reason",
                    "stage-statements",
                    "stage-validate",
                ],
            ),
            spec(
                "stage-gts-sink",
                StageKind::Sink,
                "gts_sink",
                &["stage-snapshot"],
            ),
        ],
    }
}

#[test]
fn executor_runs_the_spine_end_to_end() {
    let root = repo_root();
    let spec = spine();

    // validate → bind: the loader's structural gates (acyclic, one Sink,
    // engine-lock derived) + Rust/RDF consumes+kind agreement against the registry.
    let graph = spec.validate().expect("spine DAG validates");
    let bound = bind(&spec, &graph, &default_registry()).expect("every spine stage binds");
    assert_eq!(bound.len(), 10, "all 10 spine stages bound");

    // Run over a temp cache so the test never writes into the repo tree.
    let cache_dir = tempfile::tempdir().unwrap();
    let mut ctx = RunContext::open(&root, 4).expect("ctx");
    ctx.cache = PipelineCache::open(cache_dir.path()).unwrap();

    let result = run(&graph, &bound, &mut ctx).expect("pipeline runs end-to-end");
    assert_eq!(result.products.len(), 10);

    // The single Sink produced gmeow.gts.
    let sink = result.products.get("stage-gts-sink").expect("sink product");
    let gts = sink
        .artifact("generated/dist/gmeow.gts")
        .expect("gmeow.gts artifact");
    assert!(
        gts.len() > 4096,
        "gmeow.gts implausibly small: {} bytes",
        gts.len()
    );
}
