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
//! # The schemas sink-product tail
//!
//! The native `schemas` leaf consumes the `stage-gts-sink` product because the
//! generated schema surfaces are projections of the exact folded GTS bytes that
//! are shipped. `run_full` still runs the DAG in two phases so the sink product
//! exists before schemas render, but schemas read those bytes from the in-memory
//! upstream product; there is no Python subprocess and no disk-read dependency.

use std::collections::BTreeMap;
use std::path::Path;

use gmeow_diagnostics::{Finding, Severity};
use gmeow_logic::dag_profile::certify_acyclic;
use gmeow_logic::result::ReasoningResult;

use crate::error::PipelineError;
use crate::loader::{bind, PipelineSpec, StageSpec};
use crate::node::{StageProduct, ENGINE_RESOURCE, SINK_CAPABILITY, SOURCE_ORIGIN};
use crate::registry::default_registry;
use crate::scheduler::{run, RunContext};

/// The id of the stage that projects the exact sink GTS bytes into schema files.
const SCHEMAS_STAGE: &str = "stage-export-schemas";
/// The sole serialization exit; its product carries the `gmeow.gts` bytes.
const SINK_STAGE: &str = "stage-gts-sink";
/// The committed fold path the sink writes / schemas project.
const GTS_PATH: &str = "generated/dist/gmeow.gts";
/// The DAG-workflow contract identity the build plan executes under — the
/// `gmeow:pipeline-build` `logic:Plan` is certified under `logic:DagWorkflowResource`.
const BUILD_DAG_CONTRACT: &str = "contract:gmeow:pipeline-build:dag-workflow";
/// The world the build plan's certification verdict holds in.
const BUILD_DAG_WORLD: &str = "urn:gmeow:pipeline-build";

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
    /// The build plan's DAG-workflow certification, lowered to the typed
    /// [`ReasoningResult`] a consumer reads — the Rust-struct counterpart of the
    /// RDF `logic:ReasoningResult` `teleology::emit_dag_certification` emits, both
    /// computed from the SAME [`certify_acyclic`] verdict. For the always-acyclic
    /// `gmeow:pipeline-build` plan this is a `Completed` / `CompleteForFragment`
    /// (certified) result; a non-certified verdict at this point HARD-fails the run
    /// (no silent default) — see [`run_full`].
    pub certification: ReasoningResult,
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
        st_source("stage-source-load", "source_load", &[]),
        st("stage-statements", "statements", &[]),
        st("stage-compile-logic", "compile_logic", &[]),
        st("stage-mappings", "mappings", &["stage-compile-logic"]),
        st_reason(
            "stage-reason",
            "reason",
            &[
                "stage-compile-logic",
                "stage-mappings",
                "stage-source-load",
                "stage-statements",
            ],
        ),
        st(
            "stage-gts-compose",
            "gts_compose",
            &[
                "stage-mappings",
                "stage-reason",
                "stage-source-load",
                "stage-statements",
            ],
        ),
        st("stage-validate", "validate", &["stage-source-load"]),
        st("stage-conformance", "conformance", &[]),
        st("stage-docs-render", "docs_render", &["stage-gts-compose"]),
        st(
            "stage-snapshot",
            "snapshot",
            &[
                "stage-compile-logic",
                // Fold the external-corpus divergence Findings into graph/conformance.
                "stage-conformance",
                "stage-docs-render",
                // #700: fold THIS run's fresh JSON Schema/OpenAPI into the bundle.
                "stage-export-json-schema",
                "stage-gts-compose",
                // The FINAL projection-report loss ledger (logic ∪ correspondence rows).
                "stage-mappings",
                "stage-reason",
                "stage-statements",
                "stage-validate",
            ],
        ),
    ];

    // ── fold-reading export leaves (consume THIS run's snapshot) ──
    for (id, impl_key) in [
        ("stage-export-lpg", "lpg"),
        ("stage-export-yaml-ld", "yaml_ld"),
        ("stage-export-metadata", "metadata"),
        ("stage-export-export", "export"),
        ("stage-export-okf", "okf"),
        ("stage-export-parquet", "parquet"),
    ] {
        stages.push(st(id, impl_key, &["stage-snapshot"]));
    }

    // ── source-reading export leaves (independent; read slices/metadata/evals) ──
    for (id, impl_key) in [
        ("stage-export-catalog", "catalog"),
        ("stage-export-profiles", "profiles"),
        ("stage-export-frame-shapes", "frame_shapes"),
        ("stage-export-result-shapes", "result_shapes"),
        ("stage-export-json-schema", "json_schema"),
        ("stage-export-matrix", "matrix"),
        ("stage-export-apache", "apache"),
        ("stage-export-references", "references"),
        ("stage-export-evals", "evals"),
        ("stage-export-research-objects", "research-objects"),
        ("stage-export-bench", "bench"),
    ] {
        stages.push(st(id, impl_key, &[]));
    }

    // ── source-reading validation leaf: enforces the typed result-shape
    //    composition contract across competency files (emits no bundle artifact). ──
    stages.push(st(
        "stage-validate-result-shape-composition",
        "result_shape_composition",
        &[],
    ));

    // ── the single Sink: the terminal gts ARCHIVE writer (#1132 Stage C). It
    //    serializes the assembled carrier (read off `stage-snapshot`'s bundle — no
    //    re-assembly) and folds the by-reference blob archives gathered from the
    //    in-memory JSON-Schema / axiom / reasoning / SHACL-report products. ──
    stages.push(st_sink(
        SINK_STAGE,
        "gts_sink",
        &[
            "stage-compile-logic",
            "stage-export-json-schema",
            "stage-reason",
            "stage-snapshot",
            "stage-validate",
        ],
    ));

    // ── the schemas tail: a fold-reading export leaf over the carrier dataset
    //    (#1132) — reads `stage-snapshot`'s bundle directly, never the gts bytes. ──
    stages.push(st(SCHEMAS_STAGE, "schemas", &["stage-snapshot"]));

    PipelineSpec {
        id: "pipeline-build".to_string(),
        stages,
    }
}

/// A capability-free stage (Generated provenance, non-sink) — the default for every
/// transform / validate / docs-render / export leaf (the four behaviorally-inert
/// former kinds collapse to no declaration).
fn st(id: &str, impl_key: &str, consumes: &[&str]) -> StageSpec {
    StageSpec {
        id: id.to_string(),
        capabilities: Vec::new(),
        impl_key: impl_key.to_string(),
        consumes: consumes.iter().map(|s| s.to_string()).collect(),
        resources: Vec::new(),
        dataflow_entities: Vec::new(),
        formats: Vec::new(),
    }
}

/// The authored-source loader: holds [`SOURCE_ORIGIN`], so its emitted quads' provenance
/// origin is `Source`. Mirrors [`crate::stages::source_load::SourceLoadStage`]'s
/// capabilities() so the loader's bind-agreement holds.
fn st_source(id: &str, impl_key: &str, consumes: &[&str]) -> StageSpec {
    let mut s = st(id, impl_key, consumes);
    s.capabilities = vec![SOURCE_ORIGIN.to_string()];
    s
}

/// The sole serialization exit (the gts narrow waist): holds [`SINK_CAPABILITY`], the
/// one stage the loader requires to hold it. Mirrors
/// [`crate::stages::gts_sink::GtsSinkStage`]'s capabilities() for bind-agreement.
fn st_sink(id: &str, impl_key: &str, consumes: &[&str]) -> StageSpec {
    let mut s = st(id, impl_key, consumes);
    s.capabilities = vec![SINK_CAPABILITY.to_string()];
    s
}

/// The reasoning stage: it requires the exclusive reasoning engine (resource-conflict
/// serialization) AND reads only the `logic` / `relational-core` / `correspondence`
/// named graphs from `stage-compile-logic` (artifact-level typed dataflow). Mirrors
/// [`crate::stages::reason::ReasonStage`]'s resources() + consumed_entities() so the
/// dag_dogfood parity and the loader's bind-agreement both hold.
fn st_reason(id: &str, impl_key: &str, consumes: &[&str]) -> StageSpec {
    use crate::stages::compile_logic::{GRAPH_CORRESPONDENCE, GRAPH_LOGIC, GRAPH_RELATIONAL_CORE};
    let mut s = st(id, impl_key, consumes);
    s.resources = vec![ENGINE_RESOURCE.to_string()];
    s.dataflow_entities = vec![(
        "stage-compile-logic".to_string(),
        vec![
            GRAPH_CORRESPONDENCE.to_string(),
            GRAPH_LOGIC.to_string(),
            GRAPH_RELATIONAL_CORE.to_string(),
        ],
    )];
    s
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

    // Single-pass (#1132 Stage C): the schemas leaf is now a normal carrier-reading
    // export leaf (it consumes `stage-snapshot`, not the sink bytes), so the WHOLE DAG —
    // the terminal gts sink and the schemas tail included — runs in ONE scheduler pass.
    // No producer/serialization re-derivation, no SINK_STAGE-only sub-run.
    let graph = spec.validate()?;
    let registry = default_registry();
    let bound = bind(&spec, &graph, &registry)?;
    // A full single-pass build runs over the PERSISTENT per-stage cache
    // (`generated/.pipeline-cache/`, gitignored) for cross-invocation reuse: an edit to
    // one slice re-runs only the affected stages, not the whole DAG. This is safe
    // because every `stage_key` folds `cache::BUILD_FINGERPRINT` (a hash of the whole
    // workspace source + Cargo.lock + rustc), so ANY code/dependency/toolchain change —
    // including one with no `impl_version` bump — yields fresh keys and recomputes. The
    // cache is also self-verifying (blobs re-hashed on load; a mismatch hard-fails), so
    // it can never serve a stale or corrupt product. A clean checkout (CI) has no cache
    // dir and builds cold; subsequent local runs are warm.
    let mut ctx = RunContext::open(root, jobs)?;
    let result = run(&graph, &bound, &mut ctx)?;
    let products: BTreeMap<String, StageProduct> = result.products;

    let mut findings: Vec<Finding> = Vec::new();
    let mut drifted: Vec<String> = Vec::new();
    let mut produced = 0usize;
    let mut reproduced = 0usize;

    // ── Reconcile every produced artifact against committed / write it. ──
    for product in products.values() {
        for (path, bytes) in &product.artifacts() {
            // Internal in-memory dataflow artifacts (under the `pipeline/` logical
            // prefix: base-graph.nq, composed.nq, documentation.nq) are NOT
            // committed outputs — they exist only to pass between stages. Skip.
            if path.starts_with("pipeline/") {
                continue;
            }
            produced += 1;

            // The `gmeow.gts` bundle: in Regenerate mode WRITE the freshly-assembled
            // bundle to disk (the terminal's sole output — without this a stale
            // `merge=ours` bundle survives an `integrate-main` + regenerate, the exact
            // trap CLAUDE.md warns about). In Check mode it is compared by the FOLD
            // (per-named-graph quad set + reifier/annotation counts) elsewhere — CBOR
            // has encoding skew (#595) — so it is only counted here; the fold gate is
            // `tests/full_parity.rs`.
            if path == GTS_PATH {
                if mode == RunMode::Regenerate {
                    write_artifact(root, path, bytes)?;
                } else {
                    // Superset gate (PIPELINE_SPINE §7): every committed path under
                    // `generated/` must be byte-reconstructible from the emitted
                    // bundle — RDF as a named-graph fold, opaque as an inline blob.
                    // Reconstruct from THESE bytes (re-imported), so the gate proves
                    // the shipped bundle is a superset, not the in-memory carrier.
                    let report = crate::stages::superset::check_superset(root, bytes)?;
                    for path in report.missing {
                        drifted.push(path.clone());
                        findings.push(
                            Finding::new(
                                Severity::Error,
                                "pipeline.superset.missing",
                                format!("{path} has no carrier representative in gmeow.gts"),
                            )
                            .with_tool("gmeow-pipeline"),
                        );
                    }
                    for path in report.mismatch {
                        drifted.push(path.clone());
                        findings.push(
                            Finding::new(
                                Severity::Error,
                                "pipeline.superset.mismatch",
                                format!("{path} differs from its gmeow.gts reconstruction"),
                            )
                            .with_tool("gmeow-pipeline"),
                        );
                    }
                    for rep in report.orphan {
                        drifted.push(rep.clone());
                        findings.push(
                            Finding::new(
                                Severity::Error,
                                "pipeline.superset.orphan",
                                format!("{rep} is carried in gmeow.gts but maps to no committed generated/ path"),
                            )
                            .with_tool("gmeow-pipeline"),
                        );
                    }
                }
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

    // The DAG-workflow certification of the build plan (the W3 typed surface): the
    // SAME verdict the RDF `emit_dag_certification` emits, lowered to the typed
    // ReasoningResult a consumer reads. Hard-fails if the plan is not certified.
    let certification = certify_build_plan(&spec)?;

    Ok(RunReport {
        mode,
        produced,
        reproduced,
        findings,
        drifted,
        certification,
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
    // Try Turtle first, then N-Quads — the leaves emit one of these.
    // Native text ingress (#909) + native full RDFC-1.0 (#910): no oxigraph::io
    // parse, no oxrdf `Dataset::canonicalize`.
    for media_type in ["text/turtle", "application/n-quads"] {
        let Ok(ir) = gmeow_rdf::parse_dataset(bytes, media_type, None) else {
            continue;
        };
        let Ok(quads) = gmeow_rdf::oxigraph::flat_oxigraph_quads_from_dataset(&ir) else {
            continue;
        };
        if !quads.is_empty() {
            let canonical = gmeow_rdf::canonicalize_quads(quads).ok()?;
            let set: std::collections::BTreeSet<String> =
                canonical.iter().map(|q| format!("{q} .")).collect();
            return Some(set);
        }
    }
    None
}

/// Certify the build plan under the DAG-workflow contract and lower the verdict
/// into the typed [`ReasoningResult`] a build run surfaces.
///
/// The `gmeow:pipeline-build` plan is a `logic:Plan` declared under the DAG-workflow
/// contract; this certifies its producer → consumer dataflow closure with the SHARED
/// [`certify_acyclic`] certifier — the SAME one [`crate::graph::StageGraph::build`]
/// runs at load time and the RDF `teleology::emit_dag_certification` runs for a
/// reified plan — so the typed result and the RDF surface agree by construction.
///
/// The loader already rejected any cycle before a run reaches its result, so in
/// practice the verdict is `Certified`. The hard-fail here is the no-silent-default
/// backstop (Principle: no degraded fallback): a build that reached its result yet
/// is NOT certified is a defect, returned as a [`PipelineError::InvalidDag`] rather
/// than a degraded result.
fn certify_build_plan(spec: &PipelineSpec) -> Result<ReasoningResult, PipelineError> {
    // Producer → consumer orientation, matching the canonical logic:dataflowConsumes
    // (consumer → producer) the executor inverts — identical to the edge derivation
    // `StageGraph::build` and the `dag_profile_tests` use.
    let edges: Vec<(String, String)> = spec
        .stages
        .iter()
        .flat_map(|s| {
            s.consumes
                .iter()
                .map(move |dep| (dep.clone(), s.id.clone()))
        })
        .collect();
    let cert = certify_acyclic(edges.iter().map(|(a, b)| (a.as_str(), b.as_str())));
    if !cert.is_certified() {
        return Err(PipelineError::InvalidDag(format!(
            "the build plan {} reached its result but is NOT certified under the \
             DAG-workflow contract; offending cycle members: {}",
            spec.id,
            cert.witness().join(" → ")
        )));
    }
    Ok(cert.into_reasoning_result(BUILD_DAG_CONTRACT, BUILD_DAG_WORLD))
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

#[cfg(test)]
mod dag_profile_tests {
    use super::{certify_build_plan, full_spec};
    use gmeow_logic::dag_profile::{certify_acyclic, DagCertification};
    use gmeow_logic::result::{
        CompletenessStatus, EvaluationStatus, InformationState, PreservationClaim,
    };

    /// The dogfooded build plan (`gmeow:pipeline-build`, a `logic:Plan`) is
    /// certified under the DAG-workflow profile (`logic:DagWorkflowResource`): its
    /// producer → consumer dataflow closure is acyclic, so the shared certifier
    /// returns `Certified` and the typed result is complete-for-fragment. This is
    /// the executable witness of "gmeow:Pipeline certified under the profile".
    #[test]
    fn build_dag_certifies_under_the_dag_workflow_profile() {
        let spec = full_spec();
        // Edges in producer → consumer orientation, matching the canonical
        // logic:dataflowConsumes (consumer -> producer) the executor inverts.
        let edges: Vec<(String, String)> = spec
            .stages
            .iter()
            .flat_map(|s| {
                s.consumes
                    .iter()
                    .map(move |dep| (dep.clone(), s.id.clone()))
            })
            .collect();
        let cert = certify_acyclic(edges.iter().map(|(a, b)| (a.as_str(), b.as_str())));
        assert_eq!(
            cert,
            DagCertification::Certified,
            "the build DAG must certify acyclic under logic:DagWorkflowResource; witness: {:?}",
            cert.witness()
        );
        assert_eq!(
            cert.result_status(),
            (
                EvaluationStatus::Completed,
                CompletenessStatus::CompleteForFragment
            ),
            "an acyclic build plan reports complete-for-fragment under the DAG profile"
        );
    }

    /// The W3 hand-off: the build run's typed `ReasoningResult` certification
    /// surface. `certify_build_plan` is the EXACT wiring `run_full` folds into the
    /// returned `RunReport.certification` — it certifies the real `full_spec()`
    /// plan via the shared certifier and lowers the verdict to the typed result a
    /// consumer reads, so this asserts the run's certification field WITHOUT a full
    /// (off-budget) build. The real build is always acyclic, so the verdict is the
    /// certified `Completed` / `CompleteForFragment` result.
    #[test]
    fn build_run_surfaces_a_certified_typed_reasoning_result() {
        let spec = full_spec();
        let cert = certify_build_plan(&spec).expect("the build plan certifies (no cycle)");
        assert_eq!(
            cert.evaluation,
            EvaluationStatus::Completed,
            "the certified build plan's typed result is evaluation=completed"
        );
        assert_eq!(
            cert.completeness,
            CompletenessStatus::CompleteForFragment,
            "the certified build plan's typed result is complete-for-fragment"
        );
        assert_eq!(cert.information, InformationState::Supported);
        // A certified verdict carries an exact (loss-free) preservation claim and no
        // cycle witness — agreeing with the RDF emit_dag_certification mapping.
        assert_eq!(cert.preservation, PreservationClaim::exact());
        assert!(cert.preservation.unsupported_constructs.is_empty());
        assert!(cert.validate().is_ok());
    }
}
