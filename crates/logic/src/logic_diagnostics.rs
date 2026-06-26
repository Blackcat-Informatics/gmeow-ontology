// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Projection of logic-compile parse diagnostics into the canonical
//! `gmeow-diagnostics` `Report` (#856).
//!
//! This is the RUNTIME-SIDE seam (#732): the [`diagnostics_report`] projection
//! returns a `gmeow_diagnostics::Report`, and `gmeow-diagnostics` carries an
//! unconditional PyO3 dependency, so this function CANNOT live in the wasm-able
//! `gmeow-logic-compile` crate. It stays here in the runtime crate, consuming the
//! pure `Diagnostic` / `Severity` values the compiler front-end emits. The PyO3
//! `compile_logic` entrypoint (`crate::py`) and the LSP server are its callers.

use crate::compile::frontend::{Diagnostic, Severity};

/// Project parse [`Diagnostic`]s into the canonical `gmeow-diagnostics` `Report`
/// (issue #856).
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
pub fn diagnostics_report(diagnostics: &[Diagnostic]) -> gmeow_diagnostics::Report {
    use gmeow_diagnostics::{Finding, Location, Report, Severity as DSeverity};

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

#[cfg(test)]
mod tests {
    use super::diagnostics_report;
    use crate::compile::frontend::{Diagnostic, Severity};

    #[test]
    fn diagnostics_report_projects_findings_with_logic_compile_namespace() {
        let diagnostics = vec![
            Diagnostic {
                severity: Severity::Warning,
                code: "unknown-stereotype".to_owned(),
                message: "term has no recognised stereotype".to_owned(),
                subject: Some("https://blackcatinformatics.ca/gmeow/Foo".to_owned()),
            },
            Diagnostic {
                severity: Severity::Info,
                code: "redundant-axiom".to_owned(),
                message: "axiom is entailed".to_owned(),
                subject: None,
            },
            // An empty subject string carries no logical grouping key.
            Diagnostic {
                severity: Severity::Error,
                code: "malformed-axiom".to_owned(),
                message: "axiom is malformed".to_owned(),
                subject: Some(String::new()),
            },
        ];

        let report = diagnostics_report(&diagnostics);

        assert_eq!(report.tool, "logic-compile");
        assert_eq!(report.findings.len(), 3);

        // Severity is mapped enum→enum (no string round-trip); the code carries the
        // `logic-compile.` prefix; tool is set; subject → logical location.
        let warning = &report.findings[0];
        assert_eq!(warning.severity, gmeow_diagnostics::Severity::Warning);
        assert_eq!(warning.code, "logic-compile.unknown-stereotype");
        assert_eq!(warning.message, "term has no recognised stereotype");
        assert_eq!(warning.tool.as_deref(), Some("logic-compile"));
        assert_eq!(
            warning
                .primary_location()
                .and_then(|l| l.logical.as_deref()),
            Some("https://blackcatinformatics.ca/gmeow/Foo")
        );

        // No subject ⇒ no location.
        let info = &report.findings[1];
        assert_eq!(info.severity, gmeow_diagnostics::Severity::Info);
        assert_eq!(info.code, "logic-compile.redundant-axiom");
        assert!(info.locations.is_empty());

        // Empty subject string ⇒ no location either.
        let error = &report.findings[2];
        assert_eq!(error.severity, gmeow_diagnostics::Severity::Error);
        assert_eq!(error.code, "logic-compile.malformed-axiom");
        assert!(error.locations.is_empty());
    }

    #[test]
    fn diagnostics_report_for_no_diagnostics_is_an_empty_ok_report() {
        let report = diagnostics_report(&[]);
        assert_eq!(report.tool, "logic-compile");
        assert!(report.findings.is_empty());
        assert!(report.ok());
    }

    #[test]
    fn any_error_diagnostic_makes_the_report_not_ok() {
        // The compile firewall: a single Severity::Error finding flips the report
        // to not-ok (the property the front-end parse tests rely on).
        let diagnostics = vec![Diagnostic {
            severity: Severity::Error,
            code: "UNSUPPORTED_CONTRACT".to_owned(),
            message: "not soundly evaluable".to_owned(),
            subject: Some("ex:Contract".to_owned()),
        }];
        assert!(!diagnostics_report(&diagnostics).ok());
    }
}
