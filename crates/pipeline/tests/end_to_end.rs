// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! End-to-end executor test (#861 P3/P6): the DAG-driven executor runs the wired
//! spine through the snapshot carrier boundary — source_load → (statements,
//! mappings) → reason → gts_compose → validate/docs_render → snapshot → no-op
//! test sink — over the real repo, binding every stage against the default
//! registry. This exercises the scheduler and carrier assembly on production data
//! (DAG validate → bind → level-parallel schedule → engine-resource serialization
//! on reason → content-addressed cache → one Sink) while keeping the always-on
//! test under the 25s per-test budget. The terminal sink has a focused unit test,
//! and full sink-vs-committed fold parity stays with the heavy fold-parity lane.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use gmeow_pipeline::{
    bind, default_registry, run, PipelineCache, PipelineError, PipelineSpec, RunContext, Stage,
    StageInput, StageOutput, StageProduct, StageRegistry, StageSpec, ENGINE_RESOURCE,
    SINK_CAPABILITY, SOURCE_ORIGIN,
};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .canonicalize()
        .unwrap()
}

/// Build a spine [`StageSpec`], deriving resources / capabilities / typed dataflow
/// from the stage `id` so each mirrors the real Rust impl and bind's agreement holds:
/// `stage-source-load` holds [`SOURCE_ORIGIN`], the sink stage holds
/// [`SINK_CAPABILITY`], and `stage-reason` requires the exclusive engine resource and
/// narrows its compile-logic dependency to the three object-level EDB graphs.
fn spec(id: &str, impl_key: &str, consumes: &[&str]) -> StageSpec {
    use gmeow_pipeline::stages::compile_logic::{
        GRAPH_CORRESPONDENCE, GRAPH_LOGIC, GRAPH_RELATIONAL_CORE,
    };
    let is_reason = id == "stage-reason";
    StageSpec {
        id: id.to_string(),
        capabilities: match id {
            "stage-source-load" => vec![SOURCE_ORIGIN.to_string()],
            "stage-gts-sink" | "stage-test-sink" => vec![SINK_CAPABILITY.to_string()],
            _ => Vec::new(),
        },
        impl_key: impl_key.to_string(),
        consumes: consumes.iter().map(|s| s.to_string()).collect(),
        // The reason stage requires the exclusive engine resource; mirror the Rust
        // ReasonStage::resources() so bind's resource-agreement holds.
        resources: if is_reason {
            vec![ENGINE_RESOURCE.to_string()]
        } else {
            Vec::new()
        },
        // The reason stage reads only the logic/relational-core/correspondence graphs
        // from compile-logic; mirror ReasonStage::consumed_entities() for bind.
        dataflow_entities: if is_reason {
            vec![(
                "stage-compile-logic".to_string(),
                vec![
                    GRAPH_CORRESPONDENCE.to_string(),
                    GRAPH_LOGIC.to_string(),
                    GRAPH_RELATIONAL_CORE.to_string(),
                ],
            )]
        } else {
            Vec::new()
        },
        formats: Vec::new(),
    }
}

/// A no-op terminal sink for the spine test: it holds [`SINK_CAPABILITY`] (so the
/// loader's single-narrow-waist invariant is satisfied) and simply re-emits the
/// snapshot product's digest. The production `gts_sink` (which serializes the bundle
/// to disk) is covered by its own unit test and the heavy fold-parity lane.
struct TestSink {
    consumes: Vec<String>,
    capabilities: Vec<String>,
}

impl TestSink {
    fn new() -> Self {
        Self {
            consumes: vec!["stage-snapshot".to_string()],
            capabilities: vec![SINK_CAPABILITY.to_string()],
        }
    }
}

impl Stage for TestSink {
    fn id(&self) -> &str {
        "stage-test-sink"
    }

    fn consumes(&self) -> &[String] {
        &self.consumes
    }

    fn capabilities(&self) -> &[String] {
        &self.capabilities
    }

    fn impl_version(&self) -> &str {
        "test-sink.v1"
    }

    fn run(&self, input: StageInput<'_>) -> Result<StageOutput, PipelineError> {
        let snapshot =
            input
                .upstream
                .get("stage-snapshot")
                .ok_or_else(|| PipelineError::Stage {
                    stage: self.id().to_string(),
                    message: "missing stage-snapshot input".to_string(),
                })?;
        Ok(StageOutput {
            product: StageProduct::new(self.id(), snapshot.digest.clone()),
        })
    }
}

fn registry() -> StageRegistry {
    let mut registry = default_registry();
    registry.register("test_sink", Arc::new(TestSink::new()));
    registry
}

/// The implemented spine DAG through `stage-snapshot` plus a test-local no-op
/// sink — each stage's `consumes` matches its Rust impl's `consumes()` exactly
/// (so `bind` agreement holds). The remaining export leaves and production
/// terminal sink of the full `gmeow:pipeline-build` DAG are covered by focused
/// tests / the heavy fold-parity lane.
fn spine() -> PipelineSpec {
    PipelineSpec {
        id: "pipeline-spine".to_string(),
        stages: vec![
            spec("stage-source-load", "source_load", &[]),
            spec("stage-statements", "statements", &[]),
            spec("stage-compile-logic", "compile_logic", &[]),
            spec("stage-mappings", "mappings", &["stage-compile-logic"]),
            spec(
                "stage-reason",
                "reason",
                &[
                    "stage-compile-logic",
                    "stage-mappings",
                    "stage-source-load",
                    "stage-statements",
                ],
            ),
            spec(
                "stage-gts-compose",
                "gts_compose",
                &[
                    "stage-mappings",
                    "stage-reason",
                    "stage-source-load",
                    "stage-statements",
                ],
            ),
            spec("stage-docs-render", "docs_render", &["stage-gts-compose"]),
            spec("stage-validate", "validate", &["stage-source-load"]),
            // The SHACL→JSON-Schema source leaf the snapshot folds (#700); a
            // source-reading ExportLeaf that consumes nothing.
            spec("stage-export-json-schema", "json_schema", &[]),
            // The external-corpus divergence grader the snapshot folds into
            // graph/conformance; a source-reading Transform that consumes nothing.
            spec("stage-conformance", "conformance", &[]),
            spec(
                "stage-snapshot",
                "snapshot",
                &[
                    "stage-compile-logic",
                    "stage-conformance",
                    "stage-docs-render",
                    "stage-export-json-schema",
                    "stage-gts-compose",
                    "stage-mappings",
                    "stage-reason",
                    "stage-statements",
                    "stage-validate",
                ],
            ),
            spec("stage-test-sink", "test_sink", &["stage-snapshot"]),
        ],
    }
}

#[test]
fn executor_runs_the_spine_end_to_end() {
    let root = repo_root();
    let spec = spine();

    // validate → bind: the loader's structural gates (acyclic, exactly one stage
    // holding sinkCapability) + Rust/RDF consumes+capabilities agreement against the
    // registry.
    let graph = spec.validate().expect("spine DAG validates");
    let bound = bind(&spec, &graph, &registry()).expect("every spine stage binds");
    assert_eq!(bound.len(), 12, "all 12 snapshot-spine stages bound");

    // Run over a temp cache so the test never writes into the repo tree.
    let cache_dir = tempfile::tempdir().unwrap();
    let mut ctx = RunContext::open(&root, 4).expect("ctx");
    ctx.cache = PipelineCache::open(cache_dir.path()).unwrap();

    let result = run(&graph, &bound, &mut ctx).expect("pipeline runs end-to-end");
    assert_eq!(result.products.len(), 12);

    // The snapshot stage produced the terminal carrier dataset.
    let snapshot = result
        .products
        .get("stage-snapshot")
        .expect("snapshot product");
    let quad_count = snapshot.dataset().owned_quads().count();
    assert!(
        quad_count > 4096,
        "snapshot carrier implausibly small: {quad_count} quads"
    );
}
