// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! `gmeow-logic` — the Rust core of the gmeow reasoning engine.
//!
//! This crate is the Rust counterpart of the Python reference oracle; it models
//! worlds as oxigraph named graphs and provides world-indexed entailment queries
//! gated against the same language-neutral conformance corpus as `gmeow-gts`.
//!
//! This crate is single-target native only.
//! Nemo-based rule evaluation and PyO3 bindings are unconditionally included.

pub mod compile;
pub mod counterfactual;
pub mod dispatch;
pub mod encode;
pub mod entrenchment;
pub mod explain;
pub mod foundation;
pub mod materialize;
pub mod probabilistic;
pub mod profile_gate;
pub mod provenance;
pub mod query_ir;
pub mod reason;
pub mod reference_resolver;
pub mod rule_ir;
pub mod scryer_engine;
pub mod seam;
pub mod slme;
pub mod stablemodel;
pub mod store;
pub mod verify;
pub mod versioning;
pub mod wellfounded;

// PyO3 Python bindings.
pub mod py;

// Nemo reasoner bridge.
pub mod nemo_engine;

// Static profile / decidability certifier.
pub mod certify;
