// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! End-to-end executor test (P3/P6): the DAG-driven executor runs the wired
//! spine through the snapshot carrier boundary — source_load → (statements,
//! mappings) → reason → gts_compose → validate/docs_render → snapshot → no-op
//! test sink — over the real repo, binding every stage against the default
//! registry. This exercises the scheduler and carrier assembly on production data
//! (DAG validate → bind → level-parallel schedule → engine-resource serialization
//! on reason → content-addressed cache → one Sink). It is an exhaustive
//! `maint-heavy` proof; the terminal sink has focused default-lane tests and
//! committed output remains protected by the generated-artifact drift gate.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use gmeow_pipeline::{
    PipelineCache, PipelineSpec, RunContext, SINK_CAPABILITY, SOURCE_ORIGIN, Stage, StageInput,
    StageOutput, StageProduct, StageRegistry, StageSpec, bind, default_registry, run,
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
/// [`SINK_CAPABILITY`], `stage-reason` requires the exclusive engine resource, and
/// `stage-reason` narrows its compile-logic dependency to the two object-level EDB
/// graphs, while `stage-validate` sees the complete three-graph compiled carrier.
fn spec(id: &str, impl_key: &str, consumes: &[&str]) -> StageSpec {
    // Every resource / typed-dataflow-entity declaration is DERIVED from the bound Rust
    // impl in the default registry (the same registry-derivation this test already does
    // for attaches_graphs / attaches_blob_reps below), so the spine can never drift from
    // the production impls' resources() / consumed_entities() — a stage that gains an
    // engine-resource requirement or a typed dataflow narrowing needs no edit here.
    // test_sink is absent from the default registry → the derived sets are empty, matching
    // its no-resource / no-typed-entity default; its SINK capability is set explicitly below.
    let registry = default_registry();
    let bound = registry.get(impl_key);
    StageSpec {
        id: id.to_string(),
        capabilities: match id {
            "stage-source-load" => vec![SOURCE_ORIGIN.to_string()],
            "stage-gts-sink" | "stage-test-sink" => vec![SINK_CAPABILITY.to_string()],
            _ => Vec::new(),
        },
        impl_key: impl_key.to_string(),
        consumes: consumes.iter().map(|s| s.to_string()).collect(),
        resources: bound
            .as_ref()
            .map(|s| s.resources().to_vec())
            .unwrap_or_default(),
        dataflow_entities: bound
            .as_ref()
            .map(|s| s.consumed_entities().to_vec())
            .unwrap_or_default(),
        formats: Vec::new(),
        // Mirror the bound Rust impl's attach declaration (the same registry-derivation
        // run.rs::full_spec() does) so bind's Rust/RDF attach agreement holds for the
        // real production stages this spine test binds. test_sink is absent from the
        // default registry → empty, matching its no-attach default.
        attaches_graphs: default_registry()
            .get(impl_key)
            .map(|s| {
                let mut g = s.attaches_graphs().to_vec();
                g.sort();
                g.dedup();
                g
            })
            .unwrap_or_default(),
        attaches_blob_reps: default_registry()
            .get(impl_key)
            .map(|s| {
                let mut b = s.attaches_blob_reps().to_vec();
                b.sort();
                b.dedup();
                b
            })
            .unwrap_or_default(),
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

    fn run(&self, input: StageInput<'_>) -> gmeow_errors::Result<StageOutput> {
        let snapshot = input.upstream.get("stage-snapshot").ok_or_else(|| {
            gmeow_errors::Diag::of_kind(gmeow_pipeline::error::StageFailed {
                stage: self.id().to_string(),
                message: "missing stage-snapshot input".to_string(),
            })
        })?;
        Ok(StageOutput::new(StageProduct::new(
            self.id(),
            snapshot.digest.clone(),
        )))
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
            spec(
                "stage-compile-logic",
                "compile_logic",
                &["stage-source-load"],
            ),
            // Leaf compute: the ten math producers, folded into the snapshot (mirrors
            // `run.rs::full_spec()` — kept in sync so `bind`'s Rust/RDF
            // consumes-agreement check holds for `stage-snapshot`). `stage-reason` does
            // NOT consume it: every graph it attaches is a computed producer graph, never
            // object-level axiom source.
            spec("stage-math-producers", "math_producers", &[]),
            spec("stage-mappings", "mappings", &["stage-compile-logic"]),
            spec(
                "stage-reason",
                "reason",
                &[
                    "stage-compile-logic",
                    "stage-source-load",
                    "stage-statements",
                ],
            ),
            // The production consumer of the native proof-carrying backward engine: it
            // attaches graph/goal-directed, which the snapshot folds into gmeow.gts.
            // Mirrors run.rs::st_goal_directed's consumes exactly: stage-reason is
            // required (not just stage-compile-logic) — the stage's own typed dataflow
            // entity narrows to stage-reason's graph/reasoning, and `spec.validate()`
            // hard-fails a typed-dataflow producer that is not also a plain consumes edge.
            spec(
                "stage-goal-directed",
                "goal_directed",
                &["stage-compile-logic", "stage-reason"],
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
            // The three generated-shape export leaves whose in-memory products (plus
            // compile-logic's validation-shape artifacts) supply the FRESH
            // generated/shapes/*.ttl union members validate/json-schema consume —
            // never a stale disk read (the stale-disk-fold class).
            spec("stage-export-frame-shapes", "frame_shapes", &[]),
            spec("stage-export-constraint-shapes", "constraint_shapes", &[]),
            spec("stage-export-result-shapes", "result_shapes", &[]),
            spec(
                "stage-validate",
                "validate",
                &[
                    "stage-compile-logic",
                    "stage-export-constraint-shapes",
                    "stage-export-frame-shapes",
                    "stage-export-result-shapes",
                    "stage-source-load",
                ],
            ),
            spec(
                "stage-docs-render",
                "docs_render",
                &[
                    "stage-compile-logic",
                    "stage-export-json-schema",
                    "stage-gts-compose",
                    "stage-mappings",
                    "stage-reason",
                    "stage-validate",
                ],
            ),
            // The SHACL→JSON-Schema leaf the snapshot folds; a fresh-union
            // ExportLeaf consuming the four generated-shape producers.
            spec(
                "stage-export-json-schema",
                "json_schema",
                &[
                    "stage-compile-logic",
                    "stage-export-constraint-shapes",
                    "stage-export-frame-shapes",
                    "stage-export-result-shapes",
                ],
            ),
            // The external-corpus divergence grader the snapshot folds into
            // graph/conformance; a source-reading Transform that consumes nothing.
            spec("stage-conformance", "conformance", &[]),
            // The snapshot reads the RDF fanout members (profiles / evals scores /
            // research-object graphs) off these producing leaves — rendered once, in the
            // leaf, never re-rendered in the presenter (the transform-once razor).
            spec("stage-export-profiles", "profiles", &[]),
            spec("stage-export-evals", "evals", &[]),
            // The ten math producer graphs the snapshot folds into gmeow.gts as their own
            // bundle-internal named graphs (mirrors `SnapshotStage::consumes()`).
            spec("stage-math-producers", "math_producers", &[]),
            spec(
                "stage-export-research-objects",
                "research-objects",
                &["stage-mappings"],
            ),
            // The generated constraint catalog / term-content manifest `.nq` producing
            // Transforms the snapshot folds as graph/fanout/catalog named graphs; each
            // reads the reasoned closure off `stage-reason`.
            spec(
                "stage-constraint-catalog",
                "constraint_catalog",
                &["stage-reason"],
            ),
            spec("stage-term-manifest", "term_manifest", &["stage-reason"]),
            spec(
                "stage-snapshot",
                "snapshot",
                &[
                    "stage-compile-logic",
                    "stage-conformance",
                    "stage-constraint-catalog",
                    "stage-docs-render",
                    "stage-export-evals",
                    "stage-export-json-schema",
                    "stage-export-profiles",
                    "stage-export-research-objects",
                    "stage-goal-directed",
                    "stage-gts-compose",
                    "stage-mappings",
                    "stage-math-producers",
                    "stage-reason",
                    "stage-source-load",
                    "stage-statements",
                    "stage-term-manifest",
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
    assert_eq!(bound.len(), 22, "all 22 snapshot-spine stages bound");
    assert!(
        bound.iter().any(|s| s.id() == "stage-goal-directed"),
        "goal-directed stage bound by id, not merely counted"
    );
    assert!(
        bound.iter().any(|s| s.id() == "stage-constraint-catalog"),
        "constraint-catalog stage bound by id, not merely counted"
    );
    assert!(
        bound.iter().any(|s| s.id() == "stage-term-manifest"),
        "term-manifest stage bound by id, not merely counted"
    );

    // Run over a temp cache so the test never writes into the repo tree.
    let cache_dir = tempfile::tempdir().unwrap();
    let mut ctx = RunContext::open(&root, 4).expect("ctx");
    ctx.cache = PipelineCache::open(cache_dir.path()).unwrap();

    let result = run(&graph, &bound, &mut ctx).expect("pipeline runs end-to-end");
    assert_eq!(result.products.len(), 22);
    assert!(
        result.products.contains_key("stage-constraint-catalog"),
        "constraint-catalog produced a product, not merely counted"
    );
    assert!(
        result.products.contains_key("stage-term-manifest"),
        "term-manifest produced a product, not merely counted"
    );

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

    // ── The load-bearing byte-move: graph/goal-directed reaches the terminal carrier ──
    // Project graph/goal-directed OUT of the assembled snapshot bundle (not off the
    // goal-directed stage's own product) and assert the native backward engine's checked
    // answer + its proof-derivation IRI actually rode through stage-snapshot's explicit
    // enumeration fold into gmeow.gts. Without the assemble_carrier push + the reciprocal
    // stage-snapshot dataflow edge, this graph would be EMPTY here even though the stage ran.
    let goal_directed = snapshot
        .dataset()
        .project_named_graph(gmeow_pipeline::stages::goal_directed::GRAPH_GOAL_DIRECTED);
    let gd_quads: Vec<_> = goal_directed.owned_quads().collect();
    assert!(
        !gd_quads.is_empty(),
        "graph/goal-directed reached the terminal carrier (non-empty)"
    );
    let has_answer_atom = gd_quads.iter().any(|q| {
        matches!(&q.object, purrdf::RdfTerm::Literal(l)
            if l.lexical_form == "add(s(s(zero)), s(zero), s(s(s(zero))))")
    });
    assert!(
        has_answer_atom,
        "the minimal Peano demonstrator's ground answer atom is folded into gmeow.gts"
    );
    let has_derivation = gd_quads.iter().any(|q| {
        q.predicate == "https://blackcatinformatics.ca/gmeow/goalDirectedDerivation"
            && matches!(&q.object, purrdf::RdfTerm::Iri(iri)
                if iri.starts_with("https://blackcatinformatics.ca/gmeow/derivation/"))
    });
    assert!(
        has_derivation,
        "a content-addressed proof-derivation IRI is folded into gmeow.gts"
    );

    // The three substantial demonstrators' distinctive results also ride the terminal carrier:
    // (a) a structured cons-list membership answer;
    let has_structured_answer = gd_quads.iter().any(|q| {
        matches!(&q.object, purrdf::RdfTerm::Literal(l)
            if l.lexical_form == "member(a, cons(a, cons(b, cons(c, nil))))")
    });
    assert!(
        has_structured_answer,
        "the structured member/cons demonstrator answer is folded into gmeow.gts"
    );
    // (b) the SLG-WFS three-valued negation surface, including at least one `undefined` verdict
    //     alongside a founded `true`/`false` — the observable evidence WFS is non-dark;
    let wfs_verdicts: Vec<&str> = gd_quads
        .iter()
        .filter(|q| q.predicate == "https://blackcatinformatics.ca/gmeow/goalDirectedVerdict")
        .filter_map(|q| match &q.object {
            purrdf::RdfTerm::Literal(l) => Some(l.lexical_form.as_str()),
            _ => None,
        })
        .collect();
    assert!(
        wfs_verdicts.contains(&"undefined"),
        "an undefined SLG-WFS verdict is folded into gmeow.gts: {wfs_verdicts:?}"
    );
    assert!(
        wfs_verdicts.contains(&"true") && wfs_verdicts.contains(&"false"),
        "founded true/false SLG-WFS verdicts are folded into gmeow.gts: {wfs_verdicts:?}"
    );
    // (c) the order-sorted (ℤ ⊑ ℝ) subsort-unified answer.
    let has_subsort_answer = gd_quads
        .iter()
        .any(|q| matches!(&q.object, purrdf::RdfTerm::Literal(l) if l.lexical_form == "p(one)"));
    assert!(
        has_subsort_answer,
        "the order-sorted subsort-unified answer p(one) is folded into gmeow.gts"
    );
}
