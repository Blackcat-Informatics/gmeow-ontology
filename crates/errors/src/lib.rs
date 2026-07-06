// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! `gmeow-errors` — first-class diagnostics for GMEOW tooling.
//!
//! The data model and renderers are Rust-owned so every developer tool can
//! project the same findings to terminal text, JSON, SARIF, and HTML without
//! duplicating output logic. Python bindings are kept in [`py`]; the model and
//! render modules are PyO3-free.

/// Render-test snapshot helper (U5): a thin wrapper over
/// [`insta::assert_snapshot!`] so every renderer golden goes through one
/// substrate-owned entry point. The wrapper forwards its tokens verbatim, so
/// the auto-derived snapshot name and rendered body are byte-identical to a
/// direct `insta::assert_snapshot!` call.
#[macro_export]
macro_rules! assert_diag_snapshot {
    ($($tokens:tt)*) => {
        ::insta::assert_snapshot!($($tokens)*)
    };
}

pub mod model;
pub mod render;

// PyO3 bindings — enabled only for the unified native extension.
#[cfg(feature = "python")]
pub mod py;

pub use model::{
    DiagnosticAttribution, Finding, FindingCategory, Location, Report, Rule, Severity,
};
// Re-export the module-registration entrypoint so the unified `gmeow_native`
// cdylib can populate the `gmeow_native.diagnostics` submodule.
#[cfg(feature = "python")]
pub use py::register;
