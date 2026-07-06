// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The `validate` stage: DAG-native SHACL diagnostics over the loaded source graph.
//!
//! This stage runs the same Rust SHACL engine and the same shape-file union used
//! by `gmeow-dev validate` / the JSON-Schema emitter, but as a first-class
//! pipeline node. It emits deterministic diagnostics projections so the build
//! DAG has an inspectable SHACL product instead of treating validation as an
//! out-of-band Make target only.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;

use gmeow_errors::{Finding, Report, Severity};
use purrdf::provenance::DatasetProvenance;
use serde_json::json;

use crate::node::{Stage, StageInput, StageOutput, StageProduct};
use crate::stages::source_load::BASE_GRAPH_PATH;

/// Committed JSON projection of the DAG SHACL diagnostics report.
pub const SHACL_JSON_PATH: &str = "generated/diagnostics/shacl.json";
/// Committed SARIF projection of the DAG SHACL diagnostics report.
pub const SHACL_SARIF_PATH: &str = "generated/diagnostics/shacl.sarif";
/// Committed HTML projection of the DAG SHACL diagnostics report.
pub const SHACL_HTML_PATH: &str = "generated/diagnostics/shacl.html";
/// Committed `gmeow:Finding` N-Quads projection of the DAG SHACL diagnostics report.
pub const SHACL_RDF_PATH: &str = "generated/diagnostics/shacl.nq";

/// Convert the native SHACL engine report into the canonical diagnostics report.
fn diagnostics_report(report: &purrdf::shapes::report::ValidationReport) -> Report {
    let mut out = Report::new("shacl");
    out.metadata.insert("category".to_owned(), json!("shacl"));
    out.metadata
        .insert("stage".to_owned(), json!("stage-validate"));
    out.metadata
        .insert("shaclConforms".to_owned(), json!(report.conforms));
    out.metadata
        .insert("shaclResultCount".to_owned(), json!(report.results.len()));

    for result in &report.results {
        out.add_finding(gmeow_validate::findings::finding_from_shacl(result));
    }
    if out.findings.is_empty() && !report.conforms {
        out.add_finding(
            Finding::new(
                Severity::Error,
                "shacl.nonconforming",
                "SHACL validation failed: non-conforming with no results",
            )
            .with_tool("shacl"),
        );
    }
    out.metadata
        .insert("shaclGatePassed".to_owned(), json!(out.ok()));
    out.metadata
        .insert("shaclErrorCount".to_owned(), json!(out.error_count()));
    out.metadata
        .insert("shaclWarningCount".to_owned(), json!(out.warning_count()));
    out
}

/// Render the four committed SHACL diagnostics projections for a canonical report,
/// through the shared [`crate::stages::diag_render`] renderer (the one path both
/// this stage and `stage-compile-logic` route their reports through).
fn render_artifacts(report: &Report) -> Result<BTreeMap<String, Vec<u8>>, gmeow_errors::Diag> {
    crate::stages::diag_render::render_diagnostics_artifacts(
        "stage-validate",
        report,
        &crate::stages::diag_render::DiagnosticsPaths {
            json: SHACL_JSON_PATH,
            sarif: SHACL_SARIF_PATH,
            html: SHACL_HTML_PATH,
            rdf: SHACL_RDF_PATH,
        },
    )
}

/// Run SHACL over source-graph N-Quads bytes and return deterministic diagnostics.
pub fn validate_source_graph(
    root: &Path,
    source_nquads: &[u8],
) -> Result<Report, gmeow_errors::Diag> {
    // Parse the source graph into the native IR and validate it directly through the
    // native SHACL engine (`validate_dataset`), oxigraph-free.
    let dataset =
        purrdf::parse_dataset(source_nquads, "application/n-quads", None).map_err(|e| {
            gmeow_errors::Diag::of_kind(crate::error::Parse {
                message: format!("source graph parse: {e}"),
            })
        })?;
    let (_shape_store, shapes) = purrdf::shapes::shape_union::load_shapes(root)
        .map_err(|m| gmeow_errors::Diag::of_kind(crate::error::Parse { message: m }))?;
    let report = purrdf::shapes::engine::validate_dataset(&dataset, &shapes)
        .map_err(|m| gmeow_errors::Diag::of_kind(crate::error::Parse { message: m }))?;
    Ok(diagnostics_report(&report))
}

/// The `stage-validate` pipeline stage.
pub struct ValidateStage {
    consumes: Vec<String>,
}

impl ValidateStage {
    /// Construct the SHACL validation stage. It consumes the loaded authored
    /// source graph and reads the shape union directly from disk.
    pub fn new() -> Self {
        Self {
            consumes: vec!["stage-source-load".to_string()],
        }
    }
}

impl Default for ValidateStage {
    fn default() -> Self {
        Self::new()
    }
}

impl Stage for ValidateStage {
    fn id(&self) -> &str {
        "stage-validate"
    }
    fn consumes(&self) -> &[String] {
        &self.consumes
    }
    fn consumes_span_table(&self) -> bool {
        true
    }
    fn impl_version(&self) -> &str {
        // v2: lift stage-source-load's source spans onto each SHACL finding's focus-node
        // location (path + line/column) before rendering + the forward diagnostics fold.
        "validate.v2-shacl-diagnostics-source-spans"
    }
    fn input_files(&self, root: &Path) -> Result<Vec<std::path::PathBuf>, gmeow_errors::Diag> {
        purrdf::shapes::shape_union::shape_files(root)
            .map_err(|m| gmeow_errors::Diag::of_kind(crate::error::Parse { message: m }))
    }
    fn run(&self, input: StageInput<'_>) -> Result<StageOutput, gmeow_errors::Diag> {
        let source_graph = input
            .upstream
            .get("stage-source-load")
            .and_then(|p| p.artifact(BASE_GRAPH_PATH))
            .ok_or_else(|| {
                gmeow_errors::Diag::of_kind(crate::error::StageFailed {
                    stage: self.id().to_owned(),
                    message: format!("missing stage-source-load {BASE_GRAPH_PATH} artifact"),
                })
            })?;
        let mut report = validate_source_graph(input.root, source_graph)?;
        // Lift the authored source spans onto each SHACL finding whose focus node (a bare
        // IRI in the finding's logical location) matches a span-index entry — the path +
        // 1-based line/column travel onto the SHIPPED finding locations (and, via the
        // forward fold below, into the run-ledger DiagNodes). The span table is read off
        // the consumed stage-source-load product (its SINGLE source; a swappable ingestion
        // adapter produced it), never re-derived here.
        let spans = input
            .upstream
            .get("stage-source-load")
            .ok_or_else(|| {
                gmeow_errors::Diag::of_kind(crate::error::StageFailed {
                    stage: self.id().to_owned(),
                    message: "missing stage-source-load product for the source-span table"
                        .to_owned(),
                })
            })?
            .span_index()?;
        crate::ingest::enrich_findings_with_spans(&mut report, &spans);
        let artifacts = render_artifacts(&report)?;
        // Attach the SHACL diagnostics RDF as the carrier's `graph/diagnostics` named
        // graph so the presenter reads it as a pure keyed fold (PIPELINE_SPINE §4) and
        // unions it with the logic-compile diagnostics, never re-parsing the byte
        // artifact. The four committed byte projections are kept on the byte lane.
        let shacl_rdf = artifacts.get(SHACL_RDF_PATH).ok_or_else(|| {
            gmeow_errors::Diag::of_kind(crate::error::StageFailed {
                stage: self.id().to_owned(),
                message: format!("render_artifacts omitted {SHACL_RDF_PATH}"),
            })
        })?;
        let dataset = crate::stages::carrier::parse_into_graph(
            shacl_rdf,
            "application/n-quads",
            crate::stages::carrier::GRAPH_DIAGNOSTICS,
        )?;
        // FORWARD diagnostics fold: the producer's report findings are the SINGLE source
        // of both the shipped `graph/diagnostics` RDF (above) AND the run-level
        // DiagLedger. Project the findings once to pre-lowered DiagNodes, carry them on
        // the product's `diagnostics:nodes` blob (so a cache hit re-serves them), and
        // hand them up as `StageOutput.diags` for the scheduler to fold on a fresh run.
        let nodes = crate::stages::diag_render::finding_nodes(&report, self.id());
        let diag_blob = serde_json::to_vec(&nodes).map_err(|e| {
            gmeow_errors::Diag::of_kind(crate::error::StageFailed {
                stage: self.id().to_owned(),
                message: format!("encode diagnostics nodes blob: {e}"),
            })
        })?;
        let bundle = crate::bundle::bundle_from_artifacts_over_with_rep_blob(
            dataset,
            artifacts,
            DatasetProvenance::new(),
            crate::stages::carrier::REP_DIAG_NODES,
            "application/json",
            diag_blob,
        );
        Ok(StageOutput {
            product: StageProduct::from_bundle(self.id(), Arc::new(bundle)),
            diags: nodes,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(path: &Path, content: &str) {
        std::fs::create_dir_all(path.parent().expect("parent")).unwrap();
        std::fs::write(path, content).unwrap();
    }

    fn mock_repo(shapes: &str) -> tempfile::TempDir {
        let repo = tempfile::tempdir().unwrap();
        write(&repo.path().join("shapes/gmeow-shapes.ttl"), shapes);
        write(
            &repo.path().join("generated/shapes/frame-shapes.ttl"),
            "# generated\n",
        );
        std::fs::create_dir_all(repo.path().join("slices")).unwrap();
        repo
    }

    #[test]
    fn validate_stage_emits_sarif_for_shacl_violation() {
        let repo = mock_repo(
            r#"
@prefix ex: <https://example.test/> .
@prefix sh: <http://www.w3.org/ns/shacl#> .

ex:RequiredShape a sh:NodeShape ;
    sh:targetNode ex:thing ;
    sh:property [
        sh:path ex:required ;
        sh:minCount 1 ;
        sh:message "required value is missing" ;
    ] .
"#,
        );
        let report = validate_source_graph(repo.path(), b"").expect("validate");
        assert_eq!(report.error_count(), 1);
        assert_eq!(
            report.metadata["shaclGatePassed"],
            serde_json::Value::Bool(false)
        );

        let artifacts = render_artifacts(&report).expect("render");
        let sarif: serde_json::Value =
            serde_json::from_slice(&artifacts[SHACL_SARIF_PATH]).expect("SARIF artifact is JSON");
        assert_eq!(sarif["version"], "2.1.0");
        assert_eq!(
            sarif["runs"][0]["automationDetails"]["id"],
            serde_json::Value::String("shacl".to_string())
        );
        assert_eq!(
            sarif["runs"][0]["results"][0]["ruleId"],
            serde_json::Value::String("shacl.MinCountConstraintComponent".to_string())
        );
    }
}
