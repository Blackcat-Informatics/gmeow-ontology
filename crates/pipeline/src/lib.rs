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
//!   sizes and the three external-distribution design totals (1491).
//!
//! Invariants the [`loader`] proves before any stage runs (no-optionality): the
//! DAG is acyclic and complete, there is exactly one `Sink` (the gts narrow
//! waist — the stage holding `gmeow:sinkCapability`), and every bound stage's
//! `capabilities` / `consumes` / `resources` agree with its RDF declaration
//! (single source of truth).

pub mod bundle;
pub mod bundle_blobs;
pub mod cache;
pub mod cli_ops;
pub mod correspondence_law;
pub mod diagnostics_reader;
pub mod docs_distribution;
pub mod docs_loss_lattice;
pub mod docs_measure;
pub mod error;
pub mod fanout;
pub mod generator_registry;
pub(crate) mod gmeow_ns;
/// Test-support only: the flagship discharge harness discovers the real slice catalog
/// with the same vocab the mappings stage uses. Re-exported doc-hidden so it is reachable
/// from the integration test without publishing the `gmeow_ns` module as stable API.
#[doc(hidden)]
pub use gmeow_ns::gmeow_slice_vocab;
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
pub mod transcode;
pub mod transform;
pub mod up_projection_corpus;
pub mod up_projection_gates;
pub mod up_projection_report;

pub mod mcp;

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
