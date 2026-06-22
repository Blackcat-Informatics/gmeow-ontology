// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The `docs_render` stage (#861 P5): the typed documentation model as data.
//!
//! Pure WIRING of the Rust docs crate (#853) — no port. It discovers the
//! `gmeow_docs::DocsModel` from the slice catalog and projects it to the
//! self-hosting documentation named graph via `gmeow_docs::to_gmeow_rdf` — the
//! exact N-Quads the Python `DocSet.to_gmeow_rdf()` folds into `gmeow.gts`. The
//! rendered HTML/Markdown site blobs (`render_site`) are folded by `gts_sink`.

use std::collections::BTreeMap;
use std::path::Path;

use gmeow_docs::model::DocsModel;
use gmeow_docs::rdf::to_gmeow_rdf;

use crate::error::PipelineError;
use crate::node::{Stage, StageInput, StageKind, StageOutput, StageProduct};

/// Logical path of the documentation named graph (N-Quads, in-memory dataflow).
pub const DOCS_GRAPH_PATH: &str = "pipeline/documentation.nq";

/// Discover the docs model under `root` and project it to the documentation
/// named graph (N-Quads).
pub fn render_docs_graph(root: &Path) -> Result<String, PipelineError> {
    let model = DocsModel::discover(root).map_err(|e| PipelineError::Stage {
        stage: "stage-docs-render".to_string(),
        message: format!("docs model discovery failed: {e}"),
    })?;
    Ok(to_gmeow_rdf(&model))
}

// ── Stage impl ───────────────────────────────────────────────────────────────

/// The `docs_render` pipeline stage.
pub struct DocsRenderStage {
    consumes: Vec<String>,
}

impl DocsRenderStage {
    /// Construct the stage. It discovers the docs model from the slice catalog
    /// at the root; the slice DAG edge (consumes gts-compose) is reconciled at P6.
    pub fn new() -> Self {
        Self {
            consumes: vec!["stage-gts-compose".to_string()],
        }
    }
}

impl Default for DocsRenderStage {
    fn default() -> Self {
        Self::new()
    }
}

impl Stage for DocsRenderStage {
    fn id(&self) -> &str {
        "stage-docs-render"
    }
    fn kind(&self) -> StageKind {
        StageKind::DocsRender
    }
    fn consumes(&self) -> &[String] {
        &self.consumes
    }
    fn impl_version(&self) -> &str {
        "docs_render.v1"
    }
    fn run(&self, input: StageInput<'_>) -> Result<StageOutput, PipelineError> {
        let graph = render_docs_graph(input.root)?;
        let mut artifacts: BTreeMap<String, Vec<u8>> = BTreeMap::new();
        artifacts.insert(DOCS_GRAPH_PATH.to_string(), graph.into_bytes());
        Ok(StageOutput {
            product: StageProduct::from_artifacts(self.id(), artifacts),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use oxigraph::io::{RdfFormat, RdfParser};
    use oxigraph::store::Store;

    fn repo_root() -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .canonicalize()
            .unwrap()
    }

    #[test]
    fn docs_graph_is_nonempty_and_parses() {
        let root = repo_root();
        let nq = render_docs_graph(&root).expect("render docs graph");
        let store = Store::new().unwrap();
        let mut count = 0usize;
        for quad in RdfParser::from_format(RdfFormat::NQuads)
            .lenient()
            .for_reader(nq.as_bytes())
        {
            store.insert(&quad.expect("valid docs n-quad")).unwrap();
            count += 1;
        }
        // The documentation graph covers 50+ slices and their terms.
        assert!(
            count > 200,
            "docs named graph unexpectedly small: {count} quads"
        );
    }
}
