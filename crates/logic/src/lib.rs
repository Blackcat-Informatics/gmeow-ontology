// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only
#![feature(portable_simd)]

//! `gmeow-logic` — the Rust core of the gmeow reasoning engine.
//!
//! This crate is the Rust counterpart of the Python reference oracle; it models
//! worlds as oxigraph named graphs and provides world-indexed entailment queries
//! gated against the same language-neutral conformance corpus as `gmeow-gts`.
//!
//! This crate is single-target native only.
//! Nemo-based rule evaluation and PyO3 bindings are unconditionally included.

pub mod certificate;
pub mod counterfactual;
/// The DAG-workflow profile certifier (`logic:DagWorkflowResource`): the single
/// shared acyclicity check the canonical process model and the build pipeline run.
pub mod dag_profile;
/// Dense-id graph primitives (interner + bitset) for the hot graph algorithms.
pub(crate) mod dense;
pub mod derivation_graph;
pub mod dispatch;
pub mod encode;
pub mod entrenchment;
pub mod explain;
pub mod foundation;
// Runtime-side projection of compiler parse diagnostics into the PyO3-tainted
// gmeow-diagnostics Report (#732/#856) — kept out of the wasm-able compiler crate.
pub mod logic_diagnostics;
// Compiler-IR → runtime EvalRule bridge (#732): depends on crate::rule_ir (Nemo),
// so it stays in the runtime crate, not the wasm-able gmeow-logic-compile crate.
pub mod lower;
pub mod materialize;
pub mod obligations;
// Path-projection runtime tests (#732): they run the projected Datalog through
// crate::rule_ir (Nemo), so they live runtime-side as an in-crate test module.
#[cfg(test)]
mod path_projection_tests;
// Native physical execution core: columnar RelationStore + the semi-naive / magic-sets
// engine that the materialize and dispatch routers invoke native-first. Crate-internal.
mod physical;
pub mod probabilistic;
pub mod profile_gate;
pub mod provenance;
pub mod query_ir;
pub mod reason;
pub mod reference_resolver;
pub mod relational_core;
pub mod result;
pub mod result_rdf;
/// The typed `logic:ResultShape` lives in the Nemo-free `gmeow-logic-compile`
/// crate (alongside `LOGIC_NAMESPACE`/`PreservationKind`) so pure-data consumers
/// — notably the slice-test harness — can use it without pulling in Nemo;
/// re-exported here as `gmeow_logic::result_shape` for the result family.
pub use gmeow_logic_compile::result_shape;
pub mod rule_ir;
pub mod scryer_engine;
pub mod seam;
pub mod slme;
pub mod sparql_path_lower;
pub mod stablemodel;
pub mod store;
pub mod teleology;
pub mod transaction;
pub mod transition;
pub mod verify;
pub mod versioning;
pub mod wellfounded;
// The intra-engine phase descriptor of the well-founded materializer — the
// runtime twin the dogfood parity gate checks the authored
// `logic:wellFoundedMaterializerPlan` against (Principle 12).
pub use wellfounded::{WELL_FOUNDED_ITERATED_PHASE, WELL_FOUNDED_PHASES};

// PyO3 Python bindings.
pub mod py;

// Re-export the module-registration entrypoint so the unified `gmeow_native`
// cdylib can populate the `gmeow_native.logic` submodule (#630).
pub use py::register;

// Nemo reasoner bridge.
pub mod nemo_engine;

// Static profile / decidability certifier.
pub mod certify;
