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
use crate::node::{Stage, StageInput, StageOutput, StageProduct};

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

/// Recursively collect every regular file under `dir` into `out` (fail-fast on a
/// `read_dir` entry error; a missing directory yields nothing).
fn walk_files(dir: &Path, out: &mut Vec<std::path::PathBuf>) -> Result<(), PipelineError> {
    if !dir.is_dir() {
        return Ok(());
    }
    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
        if path.is_dir() {
            walk_files(&path, out)?;
        } else {
            out.push(path);
        }
    }
    Ok(())
}

/// Every raw source file `gmeow_docs::DocsModel::discover` reads: slice modules,
/// per-slice `docs.md` guides, slice `examples/*.ttl`, `docs/four-boxes.md`,
/// per-slice `i18n/<lang>.po` gettext translation catalogs, per-slice
/// `shapes.ttl` SHACL constraint files, per-slice `tests/competency.ttl`
/// competency-question overlays, and root `shapes/*.ttl` aggregate node shapes.
/// These are NOT reflected in the composed `stage-gts-compose` product (guide
/// bodies ride the bundle only as blake3 digests), so any stage that derives an
/// artifact from the docs model must declare them as `input_files` for cache
/// soundness. Shared by `DocsRenderStage` (the documentation graph) and
/// `SnapshotStage` (the embedded rendered site, #897).
pub(crate) fn docs_source_files(root: &Path) -> Result<Vec<std::path::PathBuf>, PipelineError> {
    let mut files: Vec<std::path::PathBuf> = Vec::new();
    for module in crate::stages::source_load::module_files(root)? {
        let dir = module.parent().unwrap_or(root);
        files.push(module.clone());
        let docs = dir.join("docs.md");
        if docs.is_file() {
            files.push(docs);
        }
        let shapes = dir.join("shapes.ttl");
        if shapes.is_file() {
            files.push(shapes);
        }
        let competency = dir.join("tests").join("competency.ttl");
        if competency.is_file() {
            files.push(competency);
        }
        if let Ok(entries) = std::fs::read_dir(dir.join("examples")) {
            for entry in entries.flatten() {
                let p = entry.path();
                if p.extension().is_some_and(|x| x == "ttl") {
                    files.push(p);
                }
            }
        }
        if let Ok(entries) = std::fs::read_dir(dir.join("i18n")) {
            for entry in entries.flatten() {
                let p = entry.path();
                if p.extension().is_some_and(|x| x == "po") {
                    files.push(p);
                }
            }
        }
    }
    let four_boxes = root.join("docs").join("four-boxes.md");
    if four_boxes.is_file() {
        files.push(four_boxes);
    }
    walk_files(&root.join("i18n"), &mut files)?;
    walk_files(&root.join("shapes"), &mut files)?;
    files.sort();
    files.dedup();
    Ok(files)
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
    fn consumes(&self) -> &[String] {
        &self.consumes
    }
    fn impl_version(&self) -> &str {
        "docs_render.v1"
    }
    fn input_files(&self, root: &Path) -> Result<Vec<std::path::PathBuf>, PipelineError> {
        // The raw-source half of this DocsRender leaf — declared so a guide /
        // four-boxes / per-slice i18n catalog edit busts the cache (cache soundness,
        // #863). The snapshot stage embeds the rendered SITE from these same sources
        // (#897), so it shares this list via `docs_source_files`.
        docs_source_files(root)
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
    use crate::stages::source_load::rdf_bytes_to_dataset;

    fn repo_root() -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .canonicalize()
            .unwrap()
    }

    #[test]
    fn docs_source_files_includes_new_inputs() {
        let root = repo_root();
        let files = docs_source_files(&root).expect("docs_source_files");
        let has_shapes_ttl = files
            .iter()
            .any(|p| p.file_name().and_then(|n| n.to_str()) == Some("shapes.ttl"));
        assert!(
            has_shapes_ttl,
            "docs_source_files must include at least one per-slice shapes.ttl"
        );
        let has_competency = files.iter().any(|p| {
            p.file_name().and_then(|n| n.to_str()) == Some("competency.ttl")
                && p.parent()
                    .and_then(|parent| parent.file_name())
                    .and_then(|n| n.to_str())
                    == Some("tests")
        });
        assert!(
            has_competency,
            "docs_source_files must include at least one per-slice tests/competency.ttl"
        );
        let shapes_dir = root.join("shapes");
        let has_root_shapes = files.iter().any(|p| {
            p.extension().and_then(|s| s.to_str()) == Some("ttl")
                && p.parent()
                    .map(|parent| parent == shapes_dir)
                    .unwrap_or(false)
        });
        assert!(
            has_root_shapes,
            "docs_source_files must include root shapes/*.ttl files"
        );
    }

    #[test]
    fn docs_graph_is_nonempty_and_parses() {
        let root = repo_root();
        let nq = render_docs_graph(&root).expect("render docs graph");
        let dataset =
            rdf_bytes_to_dataset(nq.as_bytes(), "application/n-quads", "docs-graph").unwrap();
        let count = dataset.quad_count();
        // The documentation graph covers 50+ slices and their terms.
        assert!(
            count > 200,
            "docs named graph unexpectedly small: {count} quads"
        );
    }
}
