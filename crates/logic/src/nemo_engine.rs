// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: Apache-2.0

//! Nemo reasoner bridge — native targets only.
//!
//! This module provides the surface that links the Nemo crate into
//! `gmeow-logic`.  Real rule materialization arrives in issue #501; this
//! scaffold establishes the hard dependency and exposes the types that the
//! rest of the engine will drive.
//!
//! # Platform note
//!
//! Nemo's transitive dependencies (`reqwest`, `tower-lsp`) use OS networking
//! unavailable on `wasm32-unknown-unknown`.  The `#[cfg(not(target_arch =
//! "wasm32"))]` guard in `lib.rs` is platform-correct, not an optionality
//! toggle: there are zero degraded fallbacks and zero feature flags controlling
//! this.  The wasm surface is provided by `wasm.rs` via wasm-bindgen.

use nemo::api::{load_program, validate};
use nemo::rule_model::programs::program::Program;

/// A parsed, validated Nemo rule program ready to be handed to a tokio runtime
/// for execution via [`nemo::api::reason`].
///
/// `NemoParsedRules` is the synchronous half of the pipeline.  The async
/// chase (`nemo::api::reason`) arrives in issue #501; the scaffold here
/// validates that the Nemo crate is genuinely linked and that the parse +
/// validate path is exercised without a tokio runtime.
#[derive(Debug)]
pub struct NemoParsedRules {
    program: Program,
}

impl NemoParsedRules {
    /// Parse and validate a Nemo rule program from a source string.
    ///
    /// Uses [`nemo::api::load_program`], which is fully synchronous (no tokio
    /// required).  Actual reasoning ([`nemo::api::reason`]) is async and will
    /// be invoked by the `py.rs` layer via a thread-local tokio runtime in #501.
    ///
    /// # Errors
    ///
    /// Returns a string error if Nemo cannot parse or validate the program.
    pub fn parse(rules: &str) -> Result<Self, String> {
        let program = load_program(rules.to_owned(), "<gmeow-logic>".to_owned())
            .map_err(|report| format!("nemo parse error: {report:?}"))?;
        Ok(Self { program })
    }

    /// Validate a Nemo rule string and return any diagnostics as a string.
    ///
    /// This is a pure syntax/semantic check; no engine is instantiated.
    pub fn lint(rules: &str) -> String {
        let report = validate(rules.to_owned(), "<gmeow-logic>".to_owned());
        format!("{report:?}")
    }

    /// Return the inner [`Program`] for use by the async chase driver in #501.
    pub fn into_program(self) -> Program {
        self.program
    }
}
