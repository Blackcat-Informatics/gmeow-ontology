// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Shared diagnostics-projection renderer: one canonical `gmeow_diagnostics::Report`
//! → the four committed diagnostics artifacts (JSON, SARIF, HTML, and `gmeow:Finding`
//! N-Quads). Both `stage-validate` (SHACL) and `stage-compile-logic` (the logic
//! compiler) route their reports through this single path, so the SARIF surface and
//! the diagnostics named graph are normalized identically no matter which stage
//! produced the findings — one renderer, not a per-stage copy.

use std::collections::BTreeMap;

use gmeow_diagnostics::Report;

use crate::error::PipelineError;

/// The four committed logical paths a diagnostics report renders to.
pub struct DiagnosticsPaths<'a> {
    /// JSON projection path.
    pub json: &'a str,
    /// SARIF projection path.
    pub sarif: &'a str,
    /// HTML projection path.
    pub html: &'a str,
    /// `gmeow:Finding` N-Quads projection path.
    pub rdf: &'a str,
}

fn text_artifact(mut text: String) -> Vec<u8> {
    if !text.ends_with('\n') {
        text.push('\n');
    }
    text.into_bytes()
}

/// Render the four committed diagnostics projections for `report`, keyed by the
/// supplied `paths`. `stage` names the producing stage for error attribution.
pub fn render_diagnostics_artifacts(
    stage: &str,
    report: &Report,
    paths: &DiagnosticsPaths<'_>,
) -> Result<BTreeMap<String, Vec<u8>>, PipelineError> {
    let stage_err = |what: &str, detail: String| PipelineError::Stage {
        stage: stage.to_owned(),
        message: format!("render {what} diagnostics: {detail}"),
    };
    let mut artifacts: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    artifacts.insert(
        paths.json.to_owned(),
        text_artifact(
            gmeow_diagnostics::render::to_json(report)
                .map_err(|e| stage_err("JSON", e.to_string()))?,
        ),
    );
    artifacts.insert(
        paths.sarif.to_owned(),
        text_artifact(
            gmeow_diagnostics::render::to_sarif(report)
                .map_err(|e| stage_err("SARIF", e.to_string()))?,
        ),
    );
    artifacts.insert(
        paths.html.to_owned(),
        text_artifact(gmeow_diagnostics::render::to_html(report)),
    );
    artifacts.insert(
        paths.rdf.to_owned(),
        text_artifact(gmeow_diagnostics::render::to_gmeow_rdf(report)),
    );
    Ok(artifacts)
}
