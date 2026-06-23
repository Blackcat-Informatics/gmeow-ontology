// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! `gmeow-pipeline` — the DAG-driven single-pass build executor (#861).
//!
//! The build is a directed acyclic graph of typed [`Stage`]s that exchange an
//! in-memory RDF dataset / bundle instead of re-parsing `gmeow.gts` per
//! generator. The graph itself is **dogfooded** as `gmeow:Pipeline` /
//! `gmeow:PipelineStage` individuals in `slices/core/pipeline/` and read back by
//! the [`loader`], so the build is a first-class ontological citizen.
//!
//! # Layout
//!
//! * [`node`] — the [`Stage`] trait, the [`StageKind`] taxonomy, the in-memory
//!   product / input / output handles.
//! * [`graph`] — acyclicity (`tarjan_scc`) + deterministic topological levelling.
//! * [`loader`] — parse the dogfooded DAG, validate it, bind stages to impls.
//! * [`registry`] — the `STAGE_REGISTRY` (`gmeow:stageImpl` → Rust [`Stage`]).
//! * [`cache`] — content-addressed, self-verifying per-stage cache (P2).
//! * [`scheduler`] — level-parallel execution + the `Reason` engine lock (P2).
//! * [`provenance`] — per-stage `OriginKind` / `UnitId` stamping (P2).
//! * [`stages`] — the concrete production stages (P3–P5).
//! * [`py`] — the PyO3 `run_pipeline` surface (P6, `python` feature).
//!
//! Invariants the [`loader`] proves before any stage runs (no-optionality): the
//! DAG is acyclic and complete, there is exactly one `Sink` (the gts narrow
//! waist), `gmeow:carriesEngineLock` equals the kind-derived value (single
//! source of truth), and every bound stage's `kind` / `consumes` agree with its
//! RDF declaration.

pub mod cache;
pub mod error;
pub mod graph;
pub mod loader;
pub mod node;
pub mod provenance;
pub mod registry;
pub mod run;
pub mod scheduler;
pub mod stages;

#[cfg(feature = "python")]
pub mod py;

pub use cache::PipelineCache;
pub use error::PipelineError;
pub use graph::StageGraph;
pub use loader::{bind, PipelineSpec, StageSpec};
pub use node::{Stage, StageInput, StageKind, StageOutput, StageProduct};
pub use registry::{default_registry, StageRegistry};
pub use run::{full_spec, run_full, RunMode, RunReport};
pub use scheduler::{run, RunContext, RunResult, ENGINE_LOCK};

#[cfg(feature = "python")]
pub use py::register;

#[cfg(test)]
mod tests;
