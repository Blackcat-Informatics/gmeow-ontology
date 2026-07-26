// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! `gmeow-pipeline` — the DAG-driven single-pass build executor.
//!
//! The build is a directed acyclic graph of typed [`Stage`]s that exchange an
//! in-memory RDF dataset / bundle instead of re-parsing `gmeow.gts` per
//! generator. The graph itself is **dogfooded** as `gmeow:Pipeline` /
//! `gmeow:PipelineStage` individuals in `slices/core/pipeline/` and read back by
//! the [`loader`], so the build is a first-class ontological citizen.
//!
//! # Layout
//!
//! * [`node`] — the [`Stage`] trait, the capability IRIs, the in-memory
//!   product / input / output handles.
//! * [`graph`] — acyclicity (`tarjan_scc`) + deterministic topological levelling.
//! * [`loader`] — parse the dogfooded DAG, validate it, bind stages to impls.
//! * [`registry`] — the `STAGE_REGISTRY` (`gmeow:stageImpl` → Rust [`Stage`]).
//! * [`cache`] — content-addressed, self-verifying per-stage cache (P2).
//! * [`scheduler`] — level-parallel execution + per-resource serialization (P2).
//! * [`provenance`] — per-stage `OriginKind` / `UnitId` stamping (P2).
//! * [`stages`] — the concrete production stages (P3–P5).
//! * [`docs_measure`] — measured, deterministic per-format documentation byte
//!   sizes and the three external-distribution design totals.
//!
//! Invariants the [`loader`] proves before any stage runs (no-optionality): the
//! DAG is acyclic and complete, there is exactly one `Sink` (the gts narrow
//! waist — the stage holding `gmeow:sinkCapability`), and every bound stage's
//! `capabilities` / `consumes` / `resources` agree with its RDF declaration
//! (single source of truth).

pub mod bundle;
// The bundle READ side lives in the leaf crate `gmeow-bundle-view` so a consumer
// that only reads a materialized `gmeow.gts` (the `gmeow` CLI, `gmeow-dev`, the MCP
// tool surface) never inherits the build executor. Re-exported under the original
// paths so every `gmeow_pipeline::bundle_blobs::*` /
// `gmeow_pipeline::diagnostics_reader::*` caller is unchanged.
pub use gmeow_bundle_view::bundle_blobs;
pub use gmeow_bundle_view::diagnostics_reader;
pub mod cache;
pub mod cli_ops;
pub mod correspondence_law;
pub mod docs_distribution;
pub mod docs_loss_lattice;
pub mod docs_measure;
pub mod error;
pub mod fanout;
pub mod generator_registry;
pub mod graph;
pub(crate) mod gts_profile;
pub mod ingest;
pub mod loader;
pub mod mapping_purity;
pub mod node;
pub mod projections;
pub mod provenance;
pub mod put_executor;
pub mod registry;
pub mod run;
pub mod scheduler;
pub mod scoreboards;
pub mod stages;
// The transcode hub now lives in the leaf crate `gmeow-transcode` so the MCP
// `convert` tool can reach it without pulling gmeow-pipeline into a wasm build.
// Re-exported under the original path so `gmeow_pipeline::transcode::*` callers
// (crates/gmeow-cli/src/commands.rs) are unchanged.
pub use gmeow_transcode as transcode;
pub mod transform;
pub mod up_projection_corpus;
pub mod up_projection_gates;
pub mod up_projection_report;

pub use bundle::{PipelineHandle, bundle_artifact, bundle_artifacts};
pub use cache::PipelineCache;
pub use fanout::{FanoutReport, fanout};
pub use generator_registry::{
    GENERATORS, GeneratorInfo, GeneratorMetadata, all_output_paths, generator_by_name,
    generator_metadata, generator_names, generator_order, retained_product_paths,
};
pub use graph::StageGraph;
pub use gts_profile::validate_mandated_frames;
pub use loader::{PipelineSpec, StageSpec, bind};
pub use node::{
    CachePolicy, ENGINE_RESOURCE, SINK_CAPABILITY, SOURCE_ORIGIN, Stage, StageInput, StageOutput,
    StageProduct,
};
pub use registry::{StageRegistry, default_registry};
pub use run::{
    RunMode, RunOutputScope, RunReport, full_spec, run_full, run_full_scoped,
    run_full_scoped_with_progress,
};
pub use scheduler::{RunContext, RunResult, run};

#[cfg(test)]
mod tests;
