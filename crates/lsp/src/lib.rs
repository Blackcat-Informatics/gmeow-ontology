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

use gmeow_errors::model::{Finding, Location, Report, Rule, Severity};
use lsp_types::{
    CodeDescription, Diagnostic, DiagnosticRelatedInformation, DiagnosticSeverity,
    Location as LspLocation, NumberOrString, Position, Range, Uri,
};
use purrdf::RdfDiagnostic;
use purrdf::{NativeRdfFormat, parse_dataset};

/// The embedded canonical GMEOW snapshot — the whole ontology + its transforms,
/// folded into one GTS bundle, baked into the analyzer so the `.ttl` linter needs no
/// repository, no generator inputs, and no network (the same `include_bytes!` the
/// consumer CLI uses). The `.ttl` analysis path reads the bundle's `shapes-archive`
/// through this constant to run the substrate SHACL validation — a HARD dependency:
/// the build fails if `generated/dist/gmeow.gts` is absent, never a degraded fallback.
///
/// The bundle is a git-ignored staged product materialized by `make check` (or
/// `make install`), never a committed input. `build.rs` resolves it to an
/// absolute path, guards against it being absent or empty, and exposes that
/// path via the `GMEOW_BUNDLE_PATH` build-time env var this `include_bytes!`
/// reads — so the build fails closed with a bootstrap pointer (naming
/// `make check`) rather than a bare "file not found" when the bundle hasn't
/// been materialized yet. `GMEOW_BUNDLE_PATH` may be set in the environment to
/// override the staged path for release/package flows; the same hard fail on
/// absence still applies.
pub const BUNDLE_GTS: &[u8] = include_bytes!(env!("GMEOW_BUNDLE_PATH"));

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
    let bytes = text.as_bytes();

    // Parse through the native (oxigraph-free) codec. The native parser is
    // fail-fast (one diagnostic per parse) rather than lenient-multi-error; that is
    // acceptable for an editor linter, which re-lints on every edit. A parse error
    // fast-fails with a single structured `turtle.syntax` finding (carrying the
    // 1-based line/column), exactly as before — the substrate pass runs only once the
    // document parses, so there is nothing to validate against the shapes otherwise.
    if let Err(diagnostic) = parse_dataset(bytes, NativeRdfFormat::Turtle.media_type(), None) {
        let mut report = Report::new("gmeow-lsp");
        let (line, col, msg) = extract_rdf_error(&diagnostic);
        let mut finding = Finding::new(Severity::Error, "turtle.syntax", msg);
        let loc = Location::new(Some(virtual_path.to_string()), Some(line), Some(col), None);
        finding.add_location(loc);
        report.add_finding(finding);
        report.normalize();
        return report;
    }

    // The document parses: READ THE SUBSTRATE. Route the parsed data graph through the
    // repo-free Tier-1 consumer SHACL validator against the bundle's data-graph shape
    // union, projecting each result THROUGH a `DiagLedger`, so a shape violation
    // surfaces with the SHACL result-path / offending-value secondary labels the LSP
    // renders as `DiagnosticRelatedInformation`. The embedded [`BUNDLE_GTS`] is the
    // shapes source — a hard dependency, never a degraded parse-only fallback.
    let mut report = analyze_ttl_substrate(bytes, virtual_path);
    report.normalize();
    report
}

/// Run the substrate SHACL validation of an already-parsed Turtle document `bytes`
/// against the bundled data-graph shapes, projected through the ledger so the returned
/// [`Report`]'s findings carry the SHACL secondary labels (`related_labels`).
///
/// A substrate failure is a HARD FAIL surfaced as a visible `lsp.substrate` error
/// finding (never a silent swallow): the shapes are embedded and always parse, and the
/// document has already parsed cleanly here, so this path is reached only on a genuine
/// internal fault — which must be reported, not hidden.
fn analyze_ttl_substrate(bytes: &[u8], virtual_path: &str) -> Report {
    match gmeow_validate::data_validate::shacl_report_via_ledger(
        bytes,
        NativeRdfFormat::Turtle.media_type(),
        BUNDLE_GTS,
        "gmeow-lsp",
    ) {
        Ok(report) => report,
        Err(diag) => {
            let mut report = Report::new("gmeow-lsp");
            let mut finding = Finding::new(Severity::Error, "lsp.substrate", diag.message());
            finding.add_location(Location::new(
                Some(virtual_path.to_string()),
                None,
                None,
                None,
            ));
            report.add_finding(finding);
            report
        }
    }
}

/// Extract a (1-based line, 1-based column, message) triple from a native
/// [`RdfDiagnostic`]. Uses the structured [`RdfLocation`](purrdf::RdfLocation)
/// line/column when the parser supplies them; falls back to `(1, 1)` otherwise.
fn extract_rdf_error(err: &RdfDiagnostic) -> (u32, u32, String) {
    let (line, col) = err
        .location
        .as_ref()
        .map(|loc| (loc.line.unwrap_or(1).max(1), loc.column.unwrap_or(1).max(1)))
        .unwrap_or((1, 1));
    (line, col, err.to_string())
}

// ─── Logic analysis ──────────────────────────────────────────────────────────

fn analyze_logic(text: &str) -> Report {
    use gmeow_logic::logic_diagnostics::diagnostics_report;
    use gmeow_logic_compile::frontend::parse_logic_str;

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
/// Parse a URI string into an LSP [`CodeDescription`].
///
/// Returns `None` if the URI string fails to parse rather than panicking.
fn make_code_description(uri: &str) -> Option<CodeDescription> {
    uri.parse::<Uri>().ok().map(|href| CodeDescription { href })
}

/// Resolve a secondary label's [`Location::path`] into an LSP [`Uri`].
///
/// A related label may anchor at the SAME document the diagnostic rides on, or at
/// another file (a "defined here" span in a companion module). The `path` a label
/// carries is the same virtual-path spelling the analyzer attributes primary
/// locations to (an absolute filesystem path for `file://` documents), so we
/// invert it back to a `file://` [`Uri`]:
///
/// * An absolute filesystem path → a `file://` URI (the common case; a same-document
///   label round-trips back to the document's own `doc_uri`).
/// * A path that is already a URI string (e.g. `untitled:foo`) → parsed as-is.
/// * No usable path, or an unresolvable one → the primary document's `doc_uri`, so
///   the labelled message is never dropped — it stays attached to the diagnostic's
///   own document.
fn resolve_label_uri(path: Option<&str>, doc_uri: &Uri) -> Uri {
    let Some(path) = path.map(str::trim).filter(|p| !p.is_empty()) else {
        return doc_uri.clone();
    };
    if let Ok(url) = url::Url::from_file_path(path)
        && let Ok(uri) = url.as_str().parse::<Uri>()
    {
        return uri;
    }
    if let Ok(uri) = path.parse::<Uri>() {
        return uri;
    }
    doc_uri.clone()
}

/// Project a witness node's TEXT-bearing secondary labels into LSP
/// [`DiagnosticRelatedInformation`]. Each labelled span keeps its MESSAGE beside a
/// resolved [`LspLocation`]; the 1-based line/column is converted to LSP's 0-based
/// coordinates with the SAME `saturating_sub` the primary range uses, producing a
/// zero-width range at the span's start. Returns `None` when the finding carries no
/// labelled spans (never `Some(vec![])`), and skips any empty-message label so no
/// empty related info is emitted.
fn related_information(
    finding: &Finding,
    doc_uri: &Uri,
) -> Option<Vec<DiagnosticRelatedInformation>> {
    let infos: Vec<DiagnosticRelatedInformation> = finding
        .related_labels
        .iter()
        .filter(|label| !label.message.is_empty())
        .map(|label| {
            let line = label.location.line.unwrap_or(1).saturating_sub(1);
            let character = label.location.column.unwrap_or(1).saturating_sub(1);
            let start = Position { line, character };
            DiagnosticRelatedInformation {
                location: LspLocation {
                    uri: resolve_label_uri(label.location.path.as_deref(), doc_uri),
                    range: Range { start, end: start },
                },
                message: label.message.clone(),
            }
        })
        .collect();
    (!infos.is_empty()).then_some(infos)
}

/// Project a [`Report`] to LSP [`Diagnostic`]s for `doc_uri`'s
/// `textDocument/publishDiagnostics` notification.
///
/// `doc_uri` is the document the diagnostics ride on; it anchors every secondary
/// [`DiagnosticRelatedInformation`] whose label carries no cross-file path (see
/// [`resolve_label_uri`]).
pub fn report_to_diagnostics(report: &Report, doc_uri: &Uri) -> Vec<Diagnostic> {
    // Build a rule lookup once so each finding can resolve its help URI in O(log n).
    let rules: std::collections::BTreeMap<&str, &Rule> =
        report.rules.iter().map(|r| (r.id.as_str(), r)).collect();

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

            // Resolve help URI from the rule registry and build CodeDescription.
            let code_description = rules
                .get(finding.code.as_str())
                .and_then(|rule| rule.help_uri.as_deref())
                .and_then(make_code_description);

            // Build the message: base message followed by any suggestions.
            let message = if finding.suggestions.is_empty() {
                finding.message.clone()
            } else {
                let mut msg = finding.message.clone();
                for s in &finding.suggestions {
                    msg.push_str("\n  \u{21b3} suggestion: ");
                    msg.push_str(s);
                }
                msg
            };

            Diagnostic {
                range,
                severity: Some(severity),
                code: Some(NumberOrString::String(finding.code.clone())),
                source: Some(report.tool.clone()),
                message,
                code_description,
                related_information: related_information(finding, doc_uri),
                ..Default::default()
            }
        })
        .collect()
}

// ─── Unit tests ──────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use gmeow_errors::model::RelatedLabel;

    /// A stable primary-document URI for the projection tests.
    fn doc_uri() -> Uri {
        "file:///home/user/bad.ttl".parse().expect("valid doc uri")
    }

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
        let diags = report_to_diagnostics(&report, &doc_uri());
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
        let diags = report_to_diagnostics(&report, &doc_uri());
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].range, Range::default());
    }

    #[test]
    fn advisory_diagnostic_carries_help_and_suggestions() {
        let mut report = Report::new("validate");

        // Register the rule with a help URI.
        let mut rule = Rule::new("advice.sample", Severity::Note);
        rule.help_uri = Some("https://blackcatinformatics.ca/gmeow/advice#sample".to_owned());
        report.add_rule(rule);

        // Add a finding with one suggestion.
        let mut finding = Finding::new(
            Severity::Note,
            "advice.sample",
            "consider a more specific sortal",
        );
        finding.suggestions.push("use gmeow:Kind".to_owned());
        report.add_finding(finding);

        let diags = report_to_diagnostics(&report, &doc_uri());
        assert_eq!(diags.len(), 1);
        let diag = &diags[0];

        // Note severity must map to INFORMATION.
        assert_eq!(diag.severity, Some(DiagnosticSeverity::INFORMATION));

        // code_description must carry the help URI.
        let code_desc = diag
            .code_description
            .as_ref()
            .expect("code_description should be Some");
        assert_eq!(
            code_desc.href.as_str(),
            "https://blackcatinformatics.ca/gmeow/advice#sample"
        );

        // Message must contain the base text and the suggestion.
        assert!(
            diag.message.contains("consider a more specific sortal"),
            "message missing base text: {:?}",
            diag.message
        );
        assert!(
            diag.message.contains("\u{21b3} suggestion: use gmeow:Kind"),
            "message missing suggestion: {:?}",
            diag.message
        );
    }

    #[test]
    fn related_labels_project_to_related_information() {
        // A finding carrying two secondary labelled spans: one anchored at another
        // file (cross-file "defined here") and one with no path (same document).
        let mut report = Report::new("gmeow-lsp");
        let mut finding = Finding::new(Severity::Error, "logic.conflict", "unsatisfiable class");
        finding.add_location(Location::new(
            Some("/home/user/bad.ttl".to_owned()),
            Some(4),
            Some(2),
            None,
        ));
        finding.add_related_label(RelatedLabel {
            location: Location::new(
                Some("/home/user/other.ttl".to_owned()),
                Some(3),
                Some(5),
                None,
            ),
            message: "conflicting axiom defined here".to_owned(),
        });
        finding.add_related_label(RelatedLabel {
            location: Location::new(None, Some(9), None, None),
            message: "witnessed in this document".to_owned(),
        });
        report.add_finding(finding);

        let diags = report_to_diagnostics(&report, &doc_uri());
        assert_eq!(diags.len(), 1);
        let infos = diags[0]
            .related_information
            .as_ref()
            .expect("related_information should be Some");
        assert_eq!(infos.len(), 2);

        // Cross-file label: resolved to a file:// URI at the 0-based span, message intact.
        let cross = infos
            .iter()
            .find(|i| i.message == "conflicting axiom defined here")
            .expect("cross-file related info present");
        assert_eq!(cross.location.uri.as_str(), "file:///home/user/other.ttl");
        assert_eq!(cross.location.range.start.line, 2);
        assert_eq!(cross.location.range.start.character, 4);

        // Path-less label falls back to the primary document URI (never dropped).
        let same = infos
            .iter()
            .find(|i| i.message == "witnessed in this document")
            .expect("same-document related info present");
        assert_eq!(same.location.uri.as_str(), doc_uri().as_str());
        assert_eq!(same.location.range.start.line, 8);
    }

    #[test]
    fn no_related_labels_yields_none() {
        // A finding with no labelled spans must set related_information to None,
        // not Some(vec![]).
        let mut report = Report::new("gmeow-lsp");
        report.add_finding(Finding::new(Severity::Warning, "test.warn", "plain"));
        let diags = report_to_diagnostics(&report, &doc_uri());
        assert_eq!(diags.len(), 1);
        assert!(diags[0].related_information.is_none());
    }

    #[test]
    fn analyze_to_sarif_emits_valid_finding() {
        // The same analyze -> to_sarif path the `sarif` CLI subcommand drives:
        // a syntax error must surface as a SARIF result with the rule id.
        let report = analyze(Lang::Ttl, "@prefix : <http://bad", "bad.ttl");
        let sarif = gmeow_errors::render::to_sarif(&report).expect("render SARIF");
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
