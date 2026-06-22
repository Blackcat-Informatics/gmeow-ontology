// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The concrete production stages (#861).
//!
//! Each stage implements [`crate::node::Stage`] and registers into the
//! `STAGE_REGISTRY` (see [`crate::registry`]). Stages are re-cut for in-memory
//! dataflow — a node is NOT a 1:1 port of an old Python generator.
//!
//! Landing order:
//!   * P3 — `source_load`, `statements`, `mappings`, `reason`, `gts_compose`.
//!   * P4 — one `ExportLeaf` per output format + the single `gts_sink`.
//!   * P5 — `docs_render` over `crates/docs`.
//!
//! P1 ships no concrete stages; this module is the home they register from.

use std::sync::Arc;

use crate::registry::StageRegistry;

pub mod apache;
pub mod catalog;
pub mod docs_render;
pub mod frame_shapes;
pub mod gts_compose;
pub mod gts_sink;
pub mod lpg;
pub mod mappings;
pub mod matrix;
pub mod metadata;
pub mod profiles;
pub mod reason;
pub mod references;
pub mod source_load;
pub mod statements;

/// Register every production stage into `registry` under its `gmeow:stageImpl`
/// key. The single inventory the loader and `run_pipeline` (P6) share. Stages
/// land here as P3–P5 implement them.
pub fn register_default(registry: &mut StageRegistry) {
    registry.register("source_load", Arc::new(source_load::SourceLoadStage));
    registry.register("statements", Arc::new(statements::StatementsStage));
    registry.register("gts_compose", Arc::new(gts_compose::GtsComposeStage::new()));
    registry.register("reason", Arc::new(reason::ReasonStage::new()));
    registry.register("mappings", Arc::new(mappings::MappingsStage));
    registry.register("docs_render", Arc::new(docs_render::DocsRenderStage::new()));
    registry.register("gts_sink", Arc::new(gts_sink::GtsSinkStage::new()));
    registry.register("catalog", Arc::new(catalog::CatalogStage));
    registry.register("profiles", Arc::new(profiles::ProfilesStage));
    registry.register("frame_shapes", Arc::new(frame_shapes::FrameShapesStage));
    registry.register("matrix", Arc::new(matrix::MatrixStage));
    registry.register("metadata", Arc::new(metadata::MetadataStage));
    registry.register("apache", Arc::new(apache::ApacheStage));
    registry.register("lpg", Arc::new(lpg::LpgStage));
    registry.register("references", Arc::new(references::ReferencesStage));
}
