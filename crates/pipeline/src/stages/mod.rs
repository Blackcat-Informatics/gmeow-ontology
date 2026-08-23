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
// The by-reference TAR archive fold (mappings / cells / queries / tests / schemas /
// shapes / axioms / models-python). Its own stage rather than sink-inline work, so the
// archives exist as a product mid-DAG and a consumer can select corpora over them
// without closing a cycle on the terminal.
pub mod archive_blobs;
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
// The canonical distribution catalog (AC2/AC6): the meta-level named graph
// declaring which documentation distributions exist, their family, their consumer
// class, and their declared capability loss — folded at carrier time.
pub mod distribution_catalog;
pub mod docs_format_rendering;
pub mod docs_render;
pub mod evals;
pub mod export;
pub mod frame_shapes;
pub mod gate_verdict;
pub mod goal_directed;
// The governance-floors export leaf: the two slice-quality floor TSVs projected as
// lossy views of the ontology-resident gmeow:AxisFloorCommitment / gmeow:SliceTierFloor
// individuals (Principle 17 — the ontology is canonical, these TSVs are its projection).
pub mod governance_floors;
// The projection-ceilings export leaf: the two projection-vocabulary ratchet TSVs
// projected as lossy views of the ontology-resident gmeow:ProjectionCeilingCommitment /
// gmeow:ProjectionVocabulary individuals (Principle 17).
pub mod projection_ceilings;
// The GMN-1 round-trip gate: the executed byte witness behind
// `gmeow:gmnCorrNormalToGmn`'s `logic:mnemomorphic true` declaration, mirroring
// `superset`'s byte-reconstruction discipline over the grounding slices' GMN-0.
pub mod gmn1_gate;
// The rejection-sampled, proof-carrying GMN training-corpus emitter: a productive functor
// over the glyph signature that enumerates well-typed GMN terms, filters each through five
// verifiers, and folds the certified corpus (+ typed rejections) as graph/gmn-training-corpus.
pub mod gmn_training_corpus;
pub mod gts_compose;
pub mod gts_sink;
pub mod json_schema;
pub mod lang_docs_rendering;
pub mod lang_form;
pub mod lang_glossary;
pub mod lang_lowering;
pub mod lang_projection;
pub mod lang_translation;
pub mod lpg;
pub mod mappings;
pub mod math_producers;
pub mod matrix;
// The medium axis's producer: the seven declared zstd dictionaries trained over
// their declared corpora, measured into gmeow:CompressionDictionaryRealization
// records, and projected as graph/medium-registry. The terminal reads its product
// to pin the pack's in-band "dct" map and to seal one gmeow:MediumEnvelope per
// emitted frame.
pub mod medium_dictionaries;
pub mod meta_findings;
pub mod metadata;
// The native SPARQL substrate the introspection export leaves query through now
// lives in the read-side leaf `gmeow-bundle-view` (the MCP tool surface needs it
// without the build executor). Re-exported at its historical path so every
// `crate::stages::native_query::*` / `gmeow_pipeline::stages::native_query::*`
// caller is unchanged.
pub use gmeow_bundle_view::native_query;
pub mod okf;
pub mod profiles;
pub mod provenance_graph;
pub mod substrate_graph;
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
// The ONE shared enriched-CompiledSchema builder every SHACL-derived schema surface
// (json-schema, schemas) compiles through — dedups the compile+enrich+pretty-print
// sequence so both surfaces read byte-identical `$defs`.
pub(crate) mod schema_compile;
pub mod schemas;
// Shared identifier / text helpers lifted out of `schemas` so the LinkML/TS/GraphQL
// renderer and the Pydantic package emitter share ONE copy of each rule.
pub(crate) mod schema_ident;
// The FRESH shape-union loader: the registry union with the produced
// `generated/shapes/*.ttl` members sourced from THIS run's consumed products instead
// of disk (the stale-disk-fold class fix; ONE semantics shared by json-schema,
// pydantic, and validate).
pub mod shape_union_fresh;
// The SKOS concept-scheme export leaf (AC1/R3): a generated projection
// of the lifted NodeKind::Annotation axioms (GMEOW-authored RDFS/SKOS annotations).
pub mod skos_surface;
// The authoring-packet corpus producer: assembles a gmeow:AuthoringPacket per in-repo
// slice batch and folds the union into the carrier as graph/authoring-briefs.
pub mod slice_brief;
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
    registry.register(
        "goal_directed",
        Arc::new(goal_directed::GoalDirectedStage::new()),
    );
    registry.register("mappings", Arc::new(mappings::MappingsStage::new()));
    registry.register("slice-brief", Arc::new(slice_brief::SliceBriefStage::new()));
    registry.register(
        "math_producers",
        Arc::new(math_producers::MathProducersStage::new()),
    );
    registry.register(
        "gmn-training-corpus",
        Arc::new(gmn_training_corpus::GmnTrainingCorpusStage::new()),
    );
    registry.register("validate", Arc::new(validate::ValidateStage::new()));
    registry.register("docs_render", Arc::new(docs_render::DocsRenderStage::new()));
    registry.register("conformance", Arc::new(conformance::ConformanceStage));
    registry.register("snapshot", Arc::new(carrier::SnapshotStage::new()));
    registry.register(
        "archive-blobs",
        Arc::new(archive_blobs::ArchiveBlobsStage::new()),
    );
    registry.register(
        "medium-dictionaries",
        Arc::new(medium_dictionaries::MediumDictionariesStage::new()),
    );
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
    registry.register("skos_surface", Arc::new(skos_surface::SkosSurfaceStage));
    registry.register(
        "constraint_shapes",
        Arc::new(constraint_shapes::ConstraintShapesStage),
    );
    registry.register(
        "governance_floors",
        Arc::new(governance_floors::GovernanceFloorsStage),
    );
    registry.register(
        "projection_ceilings",
        Arc::new(projection_ceilings::ProjectionCeilingsStage),
    );
    registry.register("result_shapes", Arc::new(result_shapes::ResultShapesStage));
    registry.register(
        "result_shape_composition",
        Arc::new(result_shape_composition::ResultShapeCompositionStage),
    );
    registry.register("json_schema", Arc::new(json_schema::JsonSchemaStage::new()));
    registry.register("pydantic", Arc::new(pydantic::PydanticStage::new()));
    registry.register("matrix", Arc::new(matrix::MatrixStage));
    registry.register("glossary", Arc::new(lang_glossary::GlossaryTableStage));
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
