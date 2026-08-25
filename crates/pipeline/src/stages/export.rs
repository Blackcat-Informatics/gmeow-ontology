// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The `export` export leaf's STAGE half: the `stage-export-export` [`Stage`] impl
//! and the two carrier reads it needs.
//!
//! The renderers themselves — the fold view, the term surface, the CSVW / Markdown /
//! JSONL / llms.txt / N-Quads / TriG / SKOS / OBO-Graphs / ShEx writers, and the
//! consumer resolution surface — are pipeline-free read-side views and live in
//! [`gmeow_bundle_view::export`], glob-re-exported below so every existing
//! `crate::stages::export::*` / `gmeow_pipeline::stages::export::*` path is
//! unchanged. Only the pieces that touch the pipeline's own types stay here: the
//! carrier read ([`read_fold_upstream`]), the upstream JSON-Schema `$defs` read
//! ([`modeled_defs_from_upstream`]), and [`ExportStage`].

use std::collections::BTreeSet;

use purrdf::RdfDataset;

pub use gmeow_bundle_view::export::*;

use crate::node::{
    CachePolicy, SERIALIZATION_BUFFER_RESOURCE, Stage, StageInput, StageOutput, StageProduct,
};

/// Borrow THIS run's carrier dataset. The runtime path every fold-reading
/// export leaf (export / okf) uses: the `stage-snapshot` product carries the
/// terminal carrier `RdfDataset` directly, so the leaves read ONE shared dataset off
/// the bundle instead of re-parsing the `gmeow.gts` bytes (GTS is exit-only).
pub(crate) fn read_fold_upstream(
    upstream: &std::collections::BTreeMap<String, StageProduct>,
) -> Result<std::sync::Arc<RdfDataset>, gmeow_errors::Diag> {
    crate::stages::carrier::snapshot_dataset(upstream)
}

/// THIS run's JSON Schema `$defs` key set, read directly off the in-memory
/// `stage-export-json-schema` product (never a stale disk read of the previously
/// committed `generated/schemas/gmeow.schema.json`) — the model-existence signal
/// `class_is_modeled` gates `python_model` on. Hard-fails if the declared
/// upstream product or its `gmeow.schema.json` artifact is missing
/// (no-optionality): [`ExportStage`] declares this dependency explicitly, so its
/// absence is a genuine wiring defect, never an honest absence.
pub(crate) fn modeled_defs_from_upstream(
    upstream: &std::collections::BTreeMap<String, StageProduct>,
) -> Result<BTreeSet<String>, gmeow_errors::Diag> {
    let bytes = upstream
        .get("stage-export-json-schema")
        .and_then(|p| p.artifact(crate::stages::json_schema::JSON_SCHEMA_PATH))
        .ok_or_else(|| {
            gmeow_errors::Diag::of_kind(crate::error::StageFailed {
                stage: "stage-export-export".to_string(),
                message: "missing stage-export-json-schema gmeow.schema.json artifact for the \
                          model-existence gate"
                    .to_string(),
            })
        })?;
    let parsed: serde_json::Value = serde_json::from_slice(bytes).map_err(|e| {
        gmeow_errors::Diag::of_kind(crate::error::StageFailed {
            stage: "stage-export-export".to_string(),
            message: format!("parse gmeow.schema.json for the model-existence gate: {e}"),
        })
    })?;
    Ok(parsed
        .get("$defs")
        .and_then(|v| v.as_object())
        .map(|d| d.keys().cloned().collect())
        .unwrap_or_default())
}

/// The `stage-export-export` export-leaf stage.
pub struct ExportStage {
    consumes: Vec<String>,
    resources: Vec<String>,
}

impl ExportStage {
    /// Construct the stage; it consumes THIS run's snapshot fold plus the
    /// `stage-export-json-schema` product, whose freshly-emitted `$defs` drive the
    /// `llms-full.txt` cards' `python_model` gate (see `class_is_modeled`) —
    /// without this edge the stage would only ever see the PREVIOUS run's
    /// committed schema (or none on a first run).
    ///
    /// It requires [`SERIALIZATION_BUFFER_RESOURCE`]: `render_all_with_languages`
    /// materializes the whole terminal carrier as N-Quads AND as TriG (1.3 GB + 1.2 GB
    /// of text on the shipped corpus) and holds both, so its measured peak allocation
    /// is 9.06 GiB — the heaviest stage in the DAG. Mirrored by
    /// `gmeow:stage-export-export gmeow:requiresResource
    /// gmeow:serializationBufferResource` in `slices/core/pipeline/module.ttl`; the
    /// loader HARD-fails on disagreement.
    pub fn new() -> Self {
        Self {
            consumes: vec![
                "stage-export-json-schema".to_string(),
                "stage-snapshot".to_string(),
            ],
            resources: vec![SERIALIZATION_BUFFER_RESOURCE.to_string()],
        }
    }
}

impl Default for ExportStage {
    fn default() -> Self {
        Self::new()
    }
}

impl Stage for ExportStage {
    fn id(&self) -> &str {
        "stage-export-export"
    }
    fn consumes(&self) -> &[String] {
        &self.consumes
    }
    fn resources(&self) -> &[String] {
        &self.resources
    }
    fn cache_policy(&self) -> CachePolicy {
        // Measured contribution: 2.543 GB serialized / ~34.5 s rebuild, while the
        // renderer itself peaks at 9.06 GiB. Persisting it duplicates the two complete
        // whole-document buffers and is not a bounded-cache win.
        CachePolicy::Recompute
    }
    fn impl_version(&self) -> &str {
        "export.v1"
    }
    fn run(&self, input: StageInput<'_>) -> Result<StageOutput, gmeow_errors::Diag> {
        let graph = read_fold_upstream(input.upstream)?;
        let modeled_defs = modeled_defs_from_upstream(input.upstream)?;
        Ok(StageOutput::new(StageProduct::from_artifacts(
            self.id(),
            render_all(graph.as_ref(), &modeled_defs)?,
        )))
    }
}
