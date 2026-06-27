// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The `compile_logic` transform: run the `logic:` compiler inside the build DAG.
//!
//! The pure parse → IR → projection compiler (`gmeow-logic-compile`) is the single
//! producer of every `logic:` information product. Before this stage it ran only
//! behind the `gmeow logic compile` CLI / the PyO3 entry point, so the loss ledger
//! (`projection-report.ttl`) and the compile diagnostics never reached the pipeline
//! rail — they terminated on disk and in conformance fixtures. This stage makes the
//! compiler a first-class DAG node: it parses the canonical logic source, runs every
//! projection back-end once, and emits — as committed artifacts the single-pass
//! regenerate/drift gate owns —
//!
//! * the eight projection serializations (the canonical RDF 1.2 IR, the OWL DL/EL,
//!   Datalog, N3, gUFO and Nemo projections, and the projection-report loss ledger), and
//! * the compile diagnostics rendered to the four canonical projections (JSON, SARIF,
//!   HTML, and `gmeow:Finding` N-Quads) — each below-`Exact` projection's structural
//!   drops surfaced as a `logic-compile.lossy-drop` note finding.
//!
//! Downstream, `stage-snapshot` folds the loss ledger into the bundle as its own named
//! graph and unions the compile findings into the diagnostics graph, so a repo-free
//! consumer reads every compiler product without re-running the compiler.
//!
//! ## Engine lock
//!
//! Compilation is pure (parse + projection); it never drives Nemo/Scryer, so this is a
//! [`StageKind::Transform`] that carries no engine lock.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use gmeow_diagnostics::{Finding, Severity};
use gmeow_logic_compile::frontend::parse_logic_str;
use gmeow_logic_compile::projections::compile_program;

use crate::error::PipelineError;
use crate::node::{Stage, StageInput, StageKind, StageOutput, StageProduct};
use crate::stages::diag_render::{render_diagnostics_artifacts, DiagnosticsPaths};

/// The single authoritative `logic:` vocabulary source the compiler reads.
pub const SOURCE_PATH: &str = "slices/core/logic/module.ttl";

/// Committed OWL 2 DL projection.
pub const OWL_DL_PATH: &str = "generated/owl/gmeow-dl.ttl";
/// Committed OWL 2 EL projection.
pub const OWL_EL_PATH: &str = "generated/owl/gmeow-el.ttl";
/// Committed Datalog projection.
pub const DATALOG_PATH: &str = "generated/datalog/gmeow.dl";
/// Committed N3 rules projection.
pub const N3_PATH: &str = "generated/n3/gmeow.n3";
/// Committed gUFO bridge projection.
pub const GUFO_PATH: &str = "generated/foundation/gufo.ttl";
/// Committed canonical RDF 1.2 IR serialization.
pub const CANONICAL_RDF12_PATH: &str = "generated/logic/gmeow.logic.rdf12.ttl";
/// Committed Nemo (`.rls`) projection.
pub const RULES_PATH: &str = "generated/logic/gmeow.rls";
/// Committed projection-report loss ledger (preservation kinds + lossy drops).
pub const PROJECTION_REPORT_PATH: &str = "generated/logic/projection-report.ttl";

/// Committed JSON projection of the compile diagnostics report.
pub const DIAG_JSON_PATH: &str = "generated/diagnostics/logic-compile.json";
/// Committed SARIF projection of the compile diagnostics report.
pub const DIAG_SARIF_PATH: &str = "generated/diagnostics/logic-compile.sarif";
/// Committed HTML projection of the compile diagnostics report.
pub const DIAG_HTML_PATH: &str = "generated/diagnostics/logic-compile.html";
/// Committed `gmeow:Finding` N-Quads projection of the compile diagnostics report.
pub const DIAG_RDF_PATH: &str = "generated/diagnostics/logic-compile.nq";

/// The diagnostics tool/code namespace for this surface.
const TOOL: &str = "logic-compile";

fn stage_err(message: impl Into<String>) -> PipelineError {
    PipelineError::Stage {
        stage: "stage-compile-logic".to_string(),
        message: message.into(),
    }
}

/// The `stage-compile-logic` pipeline stage.
pub struct CompileLogicStage;

impl CompileLogicStage {
    /// Construct the stage. It consumes no upstream product; it reads the canonical
    /// logic source directly from disk (declared via [`Stage::input_files`]).
    pub fn new() -> Self {
        Self
    }
}

impl Default for CompileLogicStage {
    fn default() -> Self {
        Self::new()
    }
}

impl Stage for CompileLogicStage {
    fn id(&self) -> &str {
        "stage-compile-logic"
    }
    fn kind(&self) -> StageKind {
        StageKind::Transform
    }
    fn consumes(&self) -> &[String] {
        &[]
    }
    fn impl_version(&self) -> &str {
        "compile-logic.v1"
    }
    fn input_files(&self, root: &Path) -> Result<Vec<PathBuf>, PipelineError> {
        // The compiler parses one canonical Turtle document; its content is the only
        // source input, so a change to it busts this stage's cache.
        Ok(vec![root.join(SOURCE_PATH)])
    }
    fn run(&self, input: StageInput<'_>) -> Result<StageOutput, PipelineError> {
        let source = std::fs::read_to_string(input.root.join(SOURCE_PATH))
            .map_err(|e| stage_err(format!("read {SOURCE_PATH}: {e}")))?;
        let (program, diagnostics) = parse_logic_str(&source, None)
            .map_err(|e| stage_err(format!("parse {SOURCE_PATH}: {}", e.0)))?;
        // The overclaim / rule-safety gate runs inside `compile_program`; a violation
        // is a hard error (fail-closed), never a silently dropped product.
        let arts = compile_program(&program).map_err(|e| stage_err(format!("compile: {e}")))?;

        let mut artifacts: BTreeMap<String, Vec<u8>> = BTreeMap::new();
        // The eight projection serializations, byte-for-byte as the compiler produced
        // them (RDF targets are reconciled by graph isomorphism, text targets by bytes).
        artifacts.insert(OWL_DL_PATH.to_string(), arts.owl_dl.into_bytes());
        artifacts.insert(OWL_EL_PATH.to_string(), arts.owl_el.into_bytes());
        artifacts.insert(DATALOG_PATH.to_string(), arts.datalog.into_bytes());
        artifacts.insert(N3_PATH.to_string(), arts.n3.into_bytes());
        artifacts.insert(GUFO_PATH.to_string(), arts.gufo.into_bytes());
        artifacts.insert(
            CANONICAL_RDF12_PATH.to_string(),
            arts.canonical_rdf12.into_bytes(),
        );
        artifacts.insert(RULES_PATH.to_string(), arts.nemo.into_bytes());
        artifacts.insert(PROJECTION_REPORT_PATH.to_string(), arts.report.into_bytes());

        // The compile diagnostics: the front-end parse findings (already coded
        // `logic-compile.<code>` by the shared bridge) plus one note finding per
        // structural lossy drop, so the loss ledger reaches the SARIF surface.
        let mut report = gmeow_logic::logic_diagnostics::diagnostics_report(&diagnostics);
        let lossy_drop_code = format!("{TOOL}.lossy-drop");
        for entry in &arts.preservation_ledger {
            for drop in &entry.lossy_drops {
                report.add_finding(
                    Finding::new(
                        Severity::Note,
                        lossy_drop_code.clone(),
                        format!("projection {} drops: {drop}", entry.target),
                    )
                    .with_tool(TOOL),
                );
            }
        }
        // Normalize for a deterministic committed artifact (mirrors the PyO3 surface).
        let report = report.normalized();
        artifacts.extend(render_diagnostics_artifacts(
            self.id(),
            &report,
            &DiagnosticsPaths {
                json: DIAG_JSON_PATH,
                sarif: DIAG_SARIF_PATH,
                html: DIAG_HTML_PATH,
                rdf: DIAG_RDF_PATH,
            },
        )?);

        Ok(StageOutput {
            product: StageProduct::from_artifacts(self.id(), artifacts),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repo_root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .canonicalize()
            .unwrap()
    }

    /// The stage emits all eight projection artifacts plus the four diagnostics
    /// projections, and the loss ledger surfaces as `logic-compile.lossy-drop`
    /// findings in the SARIF projection.
    #[test]
    fn compile_logic_stage_emits_every_product() {
        let root = repo_root();
        let upstream = BTreeMap::new();
        let out = CompileLogicStage::new()
            .run(StageInput {
                root: &root,
                upstream: &upstream,
            })
            .expect("compile-logic stage");
        let arts = &out.product.artifacts;
        for path in [
            OWL_DL_PATH,
            OWL_EL_PATH,
            DATALOG_PATH,
            N3_PATH,
            GUFO_PATH,
            CANONICAL_RDF12_PATH,
            RULES_PATH,
            PROJECTION_REPORT_PATH,
            DIAG_JSON_PATH,
            DIAG_SARIF_PATH,
            DIAG_HTML_PATH,
            DIAG_RDF_PATH,
        ] {
            assert!(arts.contains_key(path), "missing artifact {path}");
        }
        // The loss ledger reaches SARIF as note-level lossy-drop findings.
        let sarif: serde_json::Value =
            serde_json::from_slice(&arts[DIAG_SARIF_PATH]).expect("SARIF is JSON");
        let results = sarif["runs"][0]["results"]
            .as_array()
            .expect("SARIF results array");
        assert!(
            results
                .iter()
                .any(|r| r["ruleId"] == "logic-compile.lossy-drop"),
            "expected at least one logic-compile.lossy-drop finding in SARIF"
        );
        // The projection report is the loss ledger, not empty.
        let report = std::str::from_utf8(&arts[PROJECTION_REPORT_PATH]).unwrap();
        assert!(report.contains("ProjectionReport"), "report missing type");
    }
}
