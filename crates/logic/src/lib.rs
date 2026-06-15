// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: Apache-2.0

//! `gmeow-logic` — the Rust core of the gmeow reasoning engine.
//!
//! This crate is the Rust counterpart of the Python reference oracle; it models
//! worlds as oxigraph named graphs and provides world-indexed entailment queries
//! gated against the same language-neutral conformance corpus as `gmeow-gts`.
//!
//! Nemo-based rule evaluation and PyO3 bindings arrive in later tasks.

pub mod seam;
pub mod store;
