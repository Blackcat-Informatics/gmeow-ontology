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
pub mod bench;
pub mod catalog;
pub mod compile_logic;
pub mod conformance;
pub mod diag_render;
pub mod docs_render;
pub mod evals;
pub mod export;
pub mod frame_shapes;
pub mod gts_compose;
pub mod gts_sink;
pub mod json_schema;
pub mod logic;
pub mod lpg;
pub mod mappings;
pub mod matrix;
pub mod metadata;
pub mod okf;
pub mod parquet;
pub mod profiles;
pub mod provenance_graph;
pub mod reason;
pub mod references;
pub mod release;
pub mod research_objects;
pub mod result_shape_composition;
pub mod result_shapes;
pub mod rule_severity;
pub mod schemas;
pub mod snapshot;
pub mod source_load;
pub mod statements;
pub mod validate;
pub mod yaml_ld;

/// Register every production stage into `registry` under its `gmeow:stageImpl`
/// key. The single inventory the loader and `run_pipeline` (P6) share. Stages
/// land here as P3–P5 implement them.
pub fn register_default(registry: &mut StageRegistry) {
    registry.register("source_load", Arc::new(source_load::SourceLoadStage));
    registry.register("statements", Arc::new(statements::StatementsStage));
    registry.register(
        "compile_logic",
        Arc::new(compile_logic::CompileLogicStage::new()),
    );
    registry.register("gts_compose", Arc::new(gts_compose::GtsComposeStage::new()));
    registry.register("reason", Arc::new(reason::ReasonStage::new()));
    registry.register("mappings", Arc::new(mappings::MappingsStage));
    registry.register("validate", Arc::new(validate::ValidateStage::new()));
    registry.register("docs_render", Arc::new(docs_render::DocsRenderStage::new()));
    registry.register("conformance", Arc::new(conformance::ConformanceStage));
    registry.register("snapshot", Arc::new(snapshot::SnapshotStage::new()));
    registry.register("gts_sink", Arc::new(gts_sink::GtsSinkStage::new()));
    registry.register("catalog", Arc::new(catalog::CatalogStage));
    registry.register("profiles", Arc::new(profiles::ProfilesStage));
    registry.register("frame_shapes", Arc::new(frame_shapes::FrameShapesStage));
    registry.register("result_shapes", Arc::new(result_shapes::ResultShapesStage));
    registry.register(
        "result_shape_composition",
        Arc::new(result_shape_composition::ResultShapeCompositionStage),
    );
    registry.register("json_schema", Arc::new(json_schema::JsonSchemaStage));
    registry.register("matrix", Arc::new(matrix::MatrixStage));
    registry.register("metadata", Arc::new(metadata::MetadataStage::new()));
    registry.register("apache", Arc::new(apache::ApacheStage));
    registry.register("lpg", Arc::new(lpg::LpgStage::new()));
    registry.register("logic", Arc::new(logic::LogicStage::new()));
    registry.register("references", Arc::new(references::ReferencesStage));
    registry.register("evals", Arc::new(evals::EvalsStage));
    registry.register("schemas", Arc::new(schemas::SchemasStage::new()));
    registry.register(
        "research-objects",
        Arc::new(research_objects::ResearchObjectsStage),
    );
    registry.register("parquet", Arc::new(parquet::ParquetStage::new()));
    registry.register("okf", Arc::new(okf::OkfStage::new()));
    registry.register("export", Arc::new(export::ExportStage::new()));
    registry.register("yaml_ld", Arc::new(yaml_ld::YamlLdStage::new()));
    registry.register("bench", Arc::new(bench::BenchLeaderboardStage));
}
