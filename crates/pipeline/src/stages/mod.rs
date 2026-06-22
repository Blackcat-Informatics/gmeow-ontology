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

pub mod gts_compose;
pub mod mappings;
pub mod reason;
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
}
