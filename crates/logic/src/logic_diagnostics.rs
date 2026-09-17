// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Projection of logic-compile parse diagnostics into the canonical
//! `gmeow-errors` `Report`.
//!
//! This is the RUNTIME-SIDE seam: the [`diagnostics_report`] projection
//! returns a `gmeow_errors::Report`, and `gmeow-errors` carries an
//! unconditional PyO3 dependency, so this function CANNOT live in the wasm-able
//! `gmeow-logic-compile` crate. It stays here in the runtime crate, consuming the
//! pure `Diagnostic` / `Severity` values the compiler front-end emits. The PyO3
//! `compile_logic` entrypoint (`crate::py`) and the LSP server are its callers.

use gmeow_logic_compile::frontend::{Diagnostic, Severity};

/// Projects logic-compile [`Diagnostic`]s into the canonical `gmeow-errors`
/// `Report`.
///
/// This is the RUST-FIRST seam: the `Finding`/`Report` construction the `logic:`
/// compile surface used to do in Python now happens here, in the Rust core, and
/// `gmeow_logic.compile_logic` hands Python a live, normalized `Report` instead of
/// a `list[dict]` of raw diagnostics.
///
/// The tool/code namespace is `logic-compile`: the report tool is `logic-compile`,
/// every finding carries `with_tool("logic-compile")`, and each code is prefixed
/// `logic-compile.<code>`. The diagnostic `subject` (an IRI / blank-node id) becomes
/// the finding's logical location; an absent **or empty** subject yields no location
/// (mirroring the prior `(subject or None)` Python behavior).
pub fn diagnostics_report(diagnostics: &[Diagnostic]) -> gmeow_errors::Report {
    use gmeow_errors::{Finding, Location, Report, Severity as DSeverity};

    let mut report = Report::new("logic-compile");
    for diag in diagnostics {
        let severity = match diag.severity {
            Severity::Error => DSeverity::Error,
            Severity::Warning => DSeverity::Warning,
            Severity::Info => DSeverity::Info,
        };
        let mut finding = Finding::new(
            severity,
            format!("logic-compile.{}", diag.code),
            diag.message.clone(),
        )
        .with_tool("logic-compile");
        if let Some(subject) = diag.subject.as_deref().filter(|s| !s.is_empty()) {
            finding.add_location(Location::new(None, None, None, Some(subject.to_owned())));
        }
        report.add_finding(finding);
    }
    report
}

#[path = "logic_diagnostics.tests.rs"]
#[cfg(test)]
mod tests;
