// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! `gmeow-lsp` library: analysis core shared between the LSP server loop and the
//! `sarif` CLI subcommand.
//!
//! # Public surface
//!
//! * [`Lang`] — file-type discriminant for `.ttl` and `.logic` files.
//! * [`classify`] — infer [`Lang`] from a URI or file-system path suffix.
//! * [`analyze`] — parse and lint a text buffer, returning a [`Report`].
//! * [`report_to_diagnostics`] — project a [`Report`] to LSP [`Diagnostic`] objects.

use gmeow_diagnostics::model::{Finding, Location, Report, Severity};
use lsp_types::{Diagnostic, DiagnosticSeverity, NumberOrString, Position, Range};
use oxigraph::io::{RdfFormat, RdfParser, RdfSyntaxError};

// ─── Language discriminant ───────────────────────────────────────────────────

/// The language a source file is written in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lang {
    /// Turtle (`.ttl`).
    Ttl,
    /// GMEOW logic DSL (`.logic`).
    Logic,
}

/// Infer the [`Lang`] from a file URI or filesystem path suffix.
///
/// Returns `None` for unrecognised extensions so callers can skip files that
/// fall outside the server's scope.
pub fn classify(uri_or_path: &str) -> Option<Lang> {
    if uri_or_path.ends_with(".ttl") {
        Some(Lang::Ttl)
    } else if uri_or_path.ends_with(".logic") {
        Some(Lang::Logic)
    } else {
        None
    }
}

// ─── Analysis entry-point ────────────────────────────────────────────────────

/// Parse and lint `text` according to `lang`, attributing source locations to
/// `virtual_path` (a filesystem path or `file://` URI).
///
/// The returned [`Report`] is already normalised (findings are sorted).
pub fn analyze(lang: Lang, text: &str, virtual_path: &str) -> Report {
    match lang {
        Lang::Ttl => analyze_ttl(text, virtual_path),
        Lang::Logic => analyze_logic(text),
    }
}

// ─── Turtle analysis ─────────────────────────────────────────────────────────

fn analyze_ttl(text: &str, virtual_path: &str) -> Report {
    let mut report = Report::new("gmeow-lsp");
    let bytes = text.as_bytes();

    // Use the lenient parser so we collect ALL syntax errors in a single pass
    // rather than stopping at the first one.
    let results: Vec<_> = RdfParser::from_format(RdfFormat::Turtle)
        .lenient()
        .for_slice(bytes)
        .collect();

    for result in results {
        if let Err(e) = result {
            let (line, col, msg) = extract_rdf_error(&e);
            let mut finding = Finding::new(Severity::Error, "turtle.syntax", msg);
            let loc = Location::new(Some(virtual_path.to_string()), Some(line), Some(col), None);
            finding.add_location(loc);
            report.add_finding(finding);
        }
    }
    report.normalize();
    report
}

/// Extract a (1-based line, 1-based column, message) triple from an
/// [`RdfParseError`].  Uses the structured [`RdfSyntaxError::location`] method
/// when available; falls back to `(1, 1, display_string)` for I/O errors or
/// format-specific errors that do not expose position data.
fn extract_rdf_error(err: &RdfSyntaxError) -> (u32, u32, String) {
    // RdfSyntaxError::location() returns Option<Range<TextPosition>>
    // where line and column are 0-based (from oxrdfio).
    if let Some(range) = err.location() {
        let line = (range.start.line as u32).saturating_add(1);
        let col = (range.start.column as u32).saturating_add(1);
        (line, col, err.to_string())
    } else {
        (1, 1, err.to_string())
    }
}

// ─── Logic analysis ──────────────────────────────────────────────────────────

fn analyze_logic(text: &str) -> Report {
    use gmeow_logic::compile::frontend::{diagnostics_report, parse_logic_str};

    match parse_logic_str(text, None) {
        Ok((_program, diags)) => {
            let mut report = diagnostics_report(&diags);
            report.normalize();
            report
        }
        Err(e) => {
            let mut report = Report::new("gmeow-lsp");
            report.add_finding(Finding::new(Severity::Error, "logic.parse", e.to_string()));
            report.normalize();
            report
        }
    }
}

// ─── LSP projection ──────────────────────────────────────────────────────────

/// Project a [`Report`] to a flat `Vec<`[`Diagnostic`]`>` suitable for the
/// `textDocument/publishDiagnostics` notification.
///
/// Findings whose primary location points at a *different* file are included
/// with their path stripped so the editor does not try to render them at the
/// wrong URI.  Findings with no location get a zero-range at line 0.
pub fn report_to_diagnostics(report: &Report) -> Vec<Diagnostic> {
    report
        .findings
        .iter()
        .map(|finding| {
            let severity = match finding.severity {
                Severity::Error => DiagnosticSeverity::ERROR,
                Severity::Warning => DiagnosticSeverity::WARNING,
                Severity::Note => DiagnosticSeverity::INFORMATION,
                Severity::Info => DiagnosticSeverity::HINT,
            };

            // Build the LSP range.  LSP positions are 0-based; our Location
            // uses 1-based line/column so we subtract 1 with saturating_sub.
            let range = if let Some(loc) = finding.primary_location() {
                let line = loc.line.unwrap_or(1).saturating_sub(1);
                let character = loc.column.unwrap_or(1).saturating_sub(1);
                let start = Position { line, character };
                Range { start, end: start }
            } else {
                Range::default()
            };

            Diagnostic {
                range,
                severity: Some(severity),
                code: Some(NumberOrString::String(finding.code.clone())),
                source: Some(report.tool.clone()),
                message: finding.message.clone(),
                ..Default::default()
            }
        })
        .collect()
}

// ─── Unit tests ──────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_ttl() {
        assert_eq!(classify("foo.ttl"), Some(Lang::Ttl));
        assert_eq!(classify("file:///a/b/foo.ttl"), Some(Lang::Ttl));
    }

    #[test]
    fn classify_logic() {
        assert_eq!(classify("rules.logic"), Some(Lang::Logic));
    }

    #[test]
    fn classify_unknown_returns_none() {
        assert_eq!(classify("foo.rs"), None);
        assert_eq!(classify("foo"), None);
    }

    #[test]
    fn analyze_valid_turtle_produces_empty_report() {
        let ttl = "@prefix : <http://example.org/> .\n:a a :Thing .\n";
        let report = analyze(Lang::Ttl, ttl, "test.ttl");
        assert!(report.ok(), "expected no errors: {:?}", report.findings);
    }

    #[test]
    fn analyze_invalid_turtle_produces_error_finding() {
        // Unclosed IRI — definite syntax error.
        let ttl = "@prefix : <http://bad";
        let report = analyze(Lang::Ttl, ttl, "bad.ttl");
        assert!(!report.ok(), "expected errors");
        assert_eq!(report.findings[0].code, "turtle.syntax");
        // Primary location must point at the file we passed.
        let loc = report.findings[0].primary_location().expect("location");
        assert_eq!(loc.path.as_deref(), Some("bad.ttl"));
        // Line must be 1-based and >= 1.
        assert!(loc.line.unwrap_or(0) >= 1, "line should be >= 1");
    }

    #[test]
    fn report_to_diagnostics_maps_severity_and_position() {
        let ttl = "@prefix : <http://bad";
        let report = analyze(Lang::Ttl, ttl, "bad.ttl");
        let diags = report_to_diagnostics(&report);
        assert!(!diags.is_empty());
        let d = &diags[0];
        assert_eq!(d.severity, Some(DiagnosticSeverity::ERROR));
        // The range start.line is 0-based so it must be one less than the
        // 1-based line in the finding.
        let finding_line = report.findings[0]
            .primary_location()
            .and_then(|l| l.line)
            .unwrap_or(1);
        assert_eq!(d.range.start.line, finding_line - 1);
    }

    #[test]
    fn report_to_diagnostics_no_location_uses_default_range() {
        let mut report = Report::new("gmeow-lsp");
        report.add_finding(Finding::new(Severity::Warning, "test.warn", "no location"));
        let diags = report_to_diagnostics(&report);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].range, Range::default());
    }

    #[test]
    fn analyze_to_sarif_emits_valid_finding() {
        // The same analyze -> to_sarif path the `sarif` CLI subcommand drives:
        // a syntax error must surface as a SARIF result with the rule id.
        let report = analyze(Lang::Ttl, "@prefix : <http://bad", "bad.ttl");
        let sarif = gmeow_diagnostics::render::to_sarif(&report).expect("render SARIF");
        let parsed: serde_json::Value = serde_json::from_str(&sarif).expect("valid SARIF JSON");
        assert_eq!(parsed["version"], "2.1.0");
        let results = parsed["runs"][0]["results"]
            .as_array()
            .expect("results array");
        assert!(
            results.iter().any(|r| r["ruleId"] == "turtle.syntax"),
            "expected a turtle.syntax result in {sarif}"
        );
    }
}
