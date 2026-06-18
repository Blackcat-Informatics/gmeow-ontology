// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! `gmeow-diagnostics` — first-class diagnostics for GMEOW tooling.
//!
//! The data model and renderers are Rust-owned so every developer tool can
//! project the same findings to terminal text, JSON, SARIF, and HTML without
//! duplicating output logic. Python bindings are kept in [`py`]; the model and
//! render modules are PyO3-free.

pub mod model;
pub mod render;

// PyO3 bindings — the only module that imports pyo3.
pub mod py;

pub use model::{Finding, Location, Report, Rule, Severity};
