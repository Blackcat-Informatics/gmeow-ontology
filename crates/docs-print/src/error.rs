// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The single hard-fail diagnostic for the print renderer: a Typst compile or
//! PDF-export error. Typst diagnostics are severities-with-spans; we flatten the
//! error-level messages into one deterministic [`Diag`] message so a failed
//! compile stops the build with full context rather than being papered over.

use gmeow_errors::{Diag, define_diag_kind};
use typst::diag::SourceDiagnostic;
use typst::ecow::EcoVec;

define_diag_kind! {
    /// A Typst compilation or PDF-export stage failed. Carries the stage name and
    /// the flattened diagnostic messages.
    pub struct TypstRenderFailed { stage: String, messages: String }
    code = "docs-print.typst-render-failed";
    grade = ::gmeow_errors::Grade::new(
        ::gmeow_errors::Severity::Error,
        ::gmeow_errors::FindingCategory::ModelingDisciplineViolation,
        ::gmeow_errors::Standpoint::Binding,
    );
    message = "{} failed: {}", stage, messages;
}

/// Build the hard-fail [`Diag`] from a set of Typst diagnostics produced at
/// `stage`. Messages are joined in their emitted order for a deterministic
/// rendering.
pub fn from_typst(stage: &str, diags: &EcoVec<SourceDiagnostic>) -> Diag {
    let messages = if diags.is_empty() {
        "unknown Typst error (no diagnostics)".to_string()
    } else {
        diags
            .iter()
            .map(|d| d.message.as_str().to_string())
            .collect::<Vec<_>>()
            .join("; ")
    };
    Diag::of_kind(TypstRenderFailed {
        stage: stage.to_string(),
        messages,
    })
}
