// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::diagnostics_report;
use gmeow_logic_compile::frontend::{Diagnostic, Severity};

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
    assert_eq!(warning.severity, gmeow_errors::Severity::Warning);
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
    assert_eq!(info.severity, gmeow_errors::Severity::Info);
    assert_eq!(info.code, "logic-compile.redundant-axiom");
    assert!(info.locations.is_empty());

    // Empty subject string ⇒ no location either.
    let error = &report.findings[2];
    assert_eq!(error.severity, gmeow_errors::Severity::Error);
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
