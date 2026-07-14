// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The concrete production stages.
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

pub mod agreement;
pub mod apache;
pub(crate) mod attach;
pub mod bench;
pub mod catalog;
pub mod compile_logic;
pub mod conformance;
pub mod constraint_catalog;
// The oxigraph-free correspondence lowerings caller (SSSOM/FnO/EDOAL/SPARQL).
pub mod carrier;
pub mod correspondence_lower;
// The file-reading edge for the oxigraph-free correspondence soundness pass:
// the seven correspondence-stack semantic checks (incl. the sole native enforcer of
// Constitution Principle 5, the equivalence-collapse gate).
pub mod constraint_shapes;
pub mod correspondence_soundness;
pub mod diag_render;
pub mod docs_format_rendering;
pub mod docs_render;
pub mod evals;
pub mod export;
pub mod fold_arena;
pub mod frame_shapes;
pub mod gate_verdict;
// The governance-floors export leaf: the two slice-quality floor TSVs projected as
// lossy views of the ontology-resident gmeow:AxisFloorCommitment / gmeow:SliceTierFloor
// individuals (Principle 17 — the ontology is canonical, these TSVs are its projection).
pub mod governance_floors;
// The GMN-1 round-trip gate: the executed byte witness behind
// `gmeow:gmnCorrNormalToGmn`'s `logic:mnemomorphic true` declaration, mirroring
// `superset`'s byte-reconstruction discipline over the grounding slices' GMN-0.
pub mod gmn1_gate;
pub mod gts_compose;
pub mod gts_sink;
pub mod json_schema;
pub mod lang_docs_rendering;
pub mod lang_form;
pub mod lang_lowering;
pub mod lang_projection;
pub mod lang_translation;
pub mod lpg;
pub mod mappings;
pub mod math_producers;
pub mod matrix;
pub mod meta_findings;
pub mod metadata;
pub mod native_query;
pub mod okf;
pub mod parquet;
pub mod profiles;
pub mod provenance_graph;
// The SHACL-derived Pydantic v2 package emitter (`gmeow_models/<slice>.py`),
// co-derived from the SAME shape compilation as the JSON-Schema stage so the two
// surfaces agree (Task 8).
pub mod pydantic;
pub mod reason;
pub mod references;
pub mod release;
pub mod research_objects;
pub mod result_shape_composition;
pub mod result_shapes;
pub mod rule_severity;
pub mod schemas;
// Shared identifier / text helpers lifted out of `schemas` so the LinkML/TS/GraphQL
// renderer and the Pydantic package emitter share ONE copy of each rule.
pub(crate) mod schema_ident;
// The FRESH shape-union loader: the registry union with the produced
// `generated/shapes/*.ttl` members sourced from THIS run's consumed products instead
// of disk (the stale-disk-fold class fix; ONE semantics shared by json-schema,
// pydantic, and validate).
pub mod shape_union_fresh;
pub mod source_load;
pub mod statements;
// Shared value-vocabulary enum enrichment for the SHACL→JSON-Schema/Pydantic surfaces.
pub mod superset;
pub mod term_manifest;
pub mod validate;
pub(crate) mod value_vocab;
pub mod yaml_ld;

/// Register every production stage into `registry` under its `gmeow:stageImpl`
/// key. The single inventory the loader and `run_pipeline` (P6) share. Stages
/// land here as P3–P5 implement them.
pub fn register_default(registry: &mut StageRegistry) {
    registry.register("source_load", Arc::new(source_load::SourceLoadStage::new()));
    registry.register("statements", Arc::new(statements::StatementsStage));
    registry.register(
        "compile_logic",
        Arc::new(compile_logic::CompileLogicStage::new()),
    );
    registry.register("gts_compose", Arc::new(gts_compose::GtsComposeStage::new()));
    registry.register("reason", Arc::new(reason::ReasonStage::new()));
    registry.register("mappings", Arc::new(mappings::MappingsStage::new()));
    registry.register(
        "math_producers",
        Arc::new(math_producers::MathProducersStage::new()),
    );
    registry.register("validate", Arc::new(validate::ValidateStage::new()));
    registry.register("docs_render", Arc::new(docs_render::DocsRenderStage::new()));
    registry.register("conformance", Arc::new(conformance::ConformanceStage));
    registry.register("snapshot", Arc::new(carrier::SnapshotStage::new()));
    registry.register("gts_sink", Arc::new(gts_sink::GtsSinkStage::new()));
    registry.register("catalog", Arc::new(catalog::CatalogStage));
    registry.register(
        "constraint_catalog",
        Arc::new(constraint_catalog::ConstraintCatalogStage::new()),
    );
    registry.register(
        "term_manifest",
        Arc::new(term_manifest::TermManifestStage::new()),
    );
    registry.register("profiles", Arc::new(profiles::ProfilesStage));
    registry.register("frame_shapes", Arc::new(frame_shapes::FrameShapesStage));
    registry.register(
        "constraint_shapes",
        Arc::new(constraint_shapes::ConstraintShapesStage),
    );
    registry.register(
        "governance_floors",
        Arc::new(governance_floors::GovernanceFloorsStage),
    );
    registry.register("result_shapes", Arc::new(result_shapes::ResultShapesStage));
    registry.register(
        "result_shape_composition",
        Arc::new(result_shape_composition::ResultShapeCompositionStage),
    );
    registry.register("json_schema", Arc::new(json_schema::JsonSchemaStage::new()));
    registry.register("pydantic", Arc::new(pydantic::PydanticStage::new()));
    registry.register("matrix", Arc::new(matrix::MatrixStage));
    registry.register("metadata", Arc::new(metadata::MetadataStage::new()));
    registry.register("apache", Arc::new(apache::ApacheStage));
    registry.register("lpg", Arc::new(lpg::LpgStage::new()));
    registry.register("references", Arc::new(references::ReferencesStage));
    registry.register("evals", Arc::new(evals::EvalsStage));
    registry.register("schemas", Arc::new(schemas::SchemasStage::new()));
    registry.register(
        "research-objects",
        Arc::new(research_objects::ResearchObjectsStage::new()),
    );
    registry.register("parquet", Arc::new(parquet::ParquetStage::new()));
    registry.register("okf", Arc::new(okf::OkfStage::new()));
    registry.register("export", Arc::new(export::ExportStage::new()));
    registry.register("yaml_ld", Arc::new(yaml_ld::YamlLdStage::new()));
    registry.register("bench", Arc::new(bench::BenchLeaderboardStage));
    registry.register("cost-ledger", Arc::new(bench::CostLedgerStage));
    registry.register(
        "agreement",
        Arc::new(agreement::AgreementMatrixStage::new()),
    );
}
