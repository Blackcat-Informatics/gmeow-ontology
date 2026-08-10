// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Fixture-level Wikidata auditing (native port of `gmeow_tools.wikidata_audit`).
//!
//! PyO3-free. Scans authored Turtle files (fixtures, ontology modules) for Wikidata
//! IRIs and reports invalid QIDs/PIDs and namespace misuse via the native
//! [`crate::mapping_eval::check_syntax_iri`] checker, plus `schema:sameAs`-to-Wikidata
//! profile-link notices. The universal `owl:sameAs` ban deliberately lives elsewhere
//! ([`crate::store::sameas_violations`], Principle 5); this auditor does NOT
//! duplicate it.
//!
//! Each file is parsed independently ([`crate::store::parse_file_dataset`]) so a parse
//! error is per-file and attributable: it produces ONE `error` finding with empty
//! subject/predicate/object, exactly mirroring the retired Python harness.

use std::path::{Path, PathBuf};

use purrdf::{DatasetView, GraphMatch, TermRef};

use crate::mapping_eval;
use crate::store;

/// Wikidata entity namespace (`wd:`).
const WD_NS: &str = "http://www.wikidata.org/entity/";
/// Wikidata direct-property namespace (`wdt:`).
const WDT_NS: &str = "http://www.wikidata.org/prop/direct/";
/// The HTTPS form of the Wikidata entity namespace (should be the `wd:` CURIE).
const WD_HTTPS: &str = "https://www.wikidata.org/entity/";
/// The HTTPS form of the Wikidata direct-property namespace (should be the `wdt:` CURIE).
const WDT_HTTPS: &str = "https://www.wikidata.org/prop/direct/";
/// `schema:sameAs` predicate IRI.
const SCHEMA_SAMEAS: &str = "https://schema.org/sameAs";

/// One finding from the fixture auditor.
///
/// `file` is the full path string (the renderer projects it to a basename); the rest
/// mirror the retired Python `AuditFinding` dataclass field-for-field.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuditFinding {
    /// The source file (full path string).
    pub file: String,
    /// Subject IRI (or empty on a parse-error finding).
    pub subject: String,
    /// Predicate IRI (or empty on a parse-error finding).
    pub predicate: String,
    /// Object IRI (or empty on a parse-error finding).
    pub object: String,
    /// `"warning"` or `"error"`.
    pub severity: String,
    /// Human-readable message.
    pub message: String,
}

/// Result of auditing a set of Turtle files.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AuditReport {
    /// Every finding, in file then document order.
    pub findings: Vec<AuditFinding>,
}

impl AuditReport {
    /// Whether the audit found no errors (warnings alone are non-fatal).
    pub fn ok(&self) -> bool {
        !self.findings.iter().any(|f| f.severity == "error")
    }

    /// Count of error-level findings.
    pub fn errors(&self) -> usize {
        self.findings
            .iter()
            .filter(|f| f.severity == "error")
            .count()
    }

    /// Count of warning-level findings.
    pub fn warnings(&self) -> usize {
        self.findings
            .iter()
            .filter(|f| f.severity == "warning")
            .count()
    }
}

/// Audit a single Turtle file for Wikidata misuse.
///
/// On a parse error, returns exactly one `error` finding with empty
/// subject/predicate/object and message `format!("failed to parse Turtle: {e}")`.
/// Otherwise iterates every quad; for IRI objects in the Wikidata entity / direct /
/// HTTPS-entity / HTTPS-direct namespaces it runs [`mapping_eval::check_syntax_iri`]
/// (mapping `bad-syntax` → `error`, everything else → `warning`), and for
/// `schema:sameAs` to a Wikidata entity (either scheme) it adds a profile-link warning.
/// Non-IRI objects are skipped (mirrors `isinstance(o, URIRef)`).
pub fn audit_file(path: &Path) -> Vec<AuditFinding> {
    let file = path.display().to_string();
    let dataset = match store::parse_file_dataset(path) {
        Ok(ds) => ds,
        Err(e) => {
            return vec![AuditFinding {
                file,
                subject: String::new(),
                predicate: String::new(),
                object: String::new(),
                severity: "error".to_owned(),
                message: format!("failed to parse Turtle: {e}"),
            }];
        }
    };

    let mut findings: Vec<AuditFinding> = Vec::new();
    for quad in dataset.quads_for_pattern(None, None, None, GraphMatch::Any) {
        // Only IRI objects (mirrors the Python `isinstance(o, URIRef)` guard).
        let TermRef::Iri(obj) = dataset.resolve(quad.o) else {
            continue;
        };
        let TermRef::Iri(pred) = dataset.resolve(quad.p) else {
            continue;
        };
        let subject = store::subject_display(dataset.resolve(quad.s));

        // Invalid or misused Wikidata IRIs.
        if obj.starts_with(WD_NS)
            || obj.starts_with(WDT_NS)
            || obj.starts_with(WD_HTTPS)
            || obj.starts_with(WDT_HTTPS)
        {
            for misuse in mapping_eval::check_syntax_iri(obj, true) {
                let severity = if misuse.kind.as_str() == "bad-syntax" {
                    "error"
                } else {
                    "warning"
                };
                findings.push(AuditFinding {
                    file: file.clone(),
                    subject: subject.clone(),
                    predicate: pred.to_owned(),
                    object: obj.to_owned(),
                    severity: severity.to_owned(),
                    message: misuse.message,
                });
            }
        }

        // schema:sameAs with a Wikidata entity, either scheme (acceptable but worth noting).
        if pred == SCHEMA_SAMEAS && (obj.starts_with(WD_NS) || obj.starts_with(WD_HTTPS)) {
            findings.push(AuditFinding {
                file: file.clone(),
                subject: subject.clone(),
                predicate: pred.to_owned(),
                object: obj.to_owned(),
                severity: "warning".to_owned(),
                message: "schema:sameAs to Wikidata entity — \
                          ensure this is a profile link, not ontology alignment"
                    .to_owned(),
            });
        }
    }

    findings
}

/// Audit a list of Turtle files in order.
pub fn audit_files(paths: &[PathBuf]) -> AuditReport {
    let mut findings: Vec<AuditFinding> = Vec::new();
    for path in paths {
        findings.extend(audit_file(path));
    }
    AuditReport { findings }
}

/// Render audit findings as human-readable text (byte-for-byte the retired Python
/// `render_audit`, including the `[yellow]`/`[red]` markup tokens the CLI routes on).
pub fn render_audit(report: &AuditReport) -> String {
    let mut lines: Vec<String> = Vec::new();
    lines.push("Wikidata Fixture Audit".to_owned());
    lines.push("=".repeat(40));
    lines.push(String::new());

    if report.findings.is_empty() {
        lines.push("No issues found.".to_owned());
        return lines.join("\n");
    }

    for finding in &report.findings {
        let emoji = if finding.severity == "warning" {
            "[yellow]warning[/yellow]"
        } else {
            "[red]error[/red]"
        };
        // basename only — matches the Python `finding.file.name`.
        let name = Path::new(&finding.file)
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| finding.file.clone());
        lines.push(format!(
            "{emoji} {name} — {} {} {}",
            finding.subject, finding.predicate, finding.object
        ));
        lines.push(format!("    {}", finding.message));
    }
    lines.push(String::new());
    lines.push(format!(
        "Totals: {} error(s), {} warning(s)",
        report.errors(),
        report.warnings()
    ));
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Write `contents` to `name` inside a fresh RAII temp directory.
    ///
    /// The returned [`tempfile::TempDir`] owns the directory: it is removed on
    /// drop, including on panic and early return. Bind it to a named `_tmp`
    /// (never a bare `_`, which would drop it immediately) so it outlives the
    /// path. The file *name* is preserved because the auditor dispatches on the
    /// `.ttl` extension.
    fn write_tmp(name: &str, contents: &str) -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().expect("create temp dir");
        let path = dir.path().join(name);
        std::fs::write(&path, contents).unwrap();
        (dir, path)
    }

    // Case 1: a well-formed `wd:Q42` object → 0 findings.
    #[test]
    fn audit_file_valid() {
        let (_tmp, path) = write_tmp(
            "gmeow_validate_wda_valid.ttl",
            "@prefix ex: <http://example.org/> .\n\
             @prefix wd: <http://www.wikidata.org/entity/> .\n\
             ex:item ex:ref wd:Q42 .\n",
        );
        let findings = audit_file(&path);
        assert_eq!(findings.len(), 0, "valid wd:Q42 must produce no findings");
    }

    // Case 2: a malformed `wd:Q0` object → exactly 1 error finding.
    #[test]
    fn audit_file_bad_syntax() {
        let (_tmp, path) = write_tmp(
            "gmeow_validate_wda_bad.ttl",
            "@prefix ex: <http://example.org/> .\n\
             @prefix wd: <http://www.wikidata.org/entity/> .\n\
             ex:item ex:ref wd:Q0 .\n",
        );
        let findings = audit_file(&path);
        assert_eq!(findings.len(), 1, "wd:Q0 must produce exactly one finding");
        assert_eq!(findings[0].severity, "error");
        assert!(
            findings[0].message.to_lowercase().contains("malformed"),
            "message must mention 'malformed'; got: {}",
            findings[0].message
        );
    }

    // Case 3: an HTTPS Wikidata entity URL → exactly 1 warning finding.
    #[test]
    fn audit_file_https_url() {
        let (_tmp, path) = write_tmp(
            "gmeow_validate_wda_https.ttl",
            "@prefix ex: <http://example.org/> .\n\
             ex:item ex:ref <https://www.wikidata.org/entity/Q42> .\n",
        );
        let findings = audit_file(&path);
        assert_eq!(
            findings.len(),
            1,
            "https URL must produce exactly one finding"
        );
        assert_eq!(findings[0].severity, "warning");
        assert!(
            findings[0].message.contains("should be written as wd:Q42"),
            "message must suggest the CURIE; got: {}",
            findings[0].message
        );
    }

    // Case 4: `owl:sameAs` is deliberately NOT this tool's job.
    #[test]
    fn audit_file_owl_sameas_not_reported_here() {
        let (_tmp, path) = write_tmp(
            "gmeow_validate_wda_sameas.ttl",
            "@prefix ex: <http://example.org/> .\n\
             @prefix wd: <http://www.wikidata.org/entity/> .\n\
             @prefix owl: <http://www.w3.org/2002/07/owl#> .\n\
             ex:item owl:sameAs wd:Q42 .\n",
        );
        let findings = audit_file(&path);
        let sameas: Vec<_> = findings
            .iter()
            .filter(|f| f.predicate == "http://www.w3.org/2002/07/owl#sameAs")
            .collect();
        assert!(
            sameas.is_empty(),
            "owl:sameAs must not be reported by the wikidata auditor"
        );
    }

    // Case 5: an HTTPS direct-property URL is now audited (previously dropped) and
    // suggests the wdt: CURIE.
    #[test]
    fn audit_file_https_direct_property() {
        let (_tmp, path) = write_tmp(
            "gmeow_validate_wda_https_wdt.ttl",
            "@prefix ex: <http://example.org/> .\n\
             ex:item ex:ref <https://www.wikidata.org/prop/direct/P31> .\n",
        );
        let findings = audit_file(&path);
        assert_eq!(
            findings.len(),
            1,
            "https wdt: URL must produce exactly one finding"
        );
        assert_eq!(findings[0].severity, "warning");
        assert!(
            findings[0].message.contains("should be written as wdt:P31"),
            "message must suggest the wdt: CURIE; got: {}",
            findings[0].message
        );
    }

    // Case 6: `schema:sameAs` to an HTTPS-form Wikidata entity fires the profile-link
    // warning (an HTTPS entity is still a Wikidata entity).
    #[test]
    fn audit_file_schema_sameas_https_entity() {
        let (_tmp, path) = write_tmp(
            "gmeow_validate_wda_sameas_https.ttl",
            "@prefix ex: <http://example.org/> .\n\
             @prefix schema: <https://schema.org/> .\n\
             ex:item schema:sameAs <https://www.wikidata.org/entity/Q42> .\n",
        );
        let findings = audit_file(&path);
        let sameas: Vec<_> = findings
            .iter()
            .filter(|f| f.message.contains("schema:sameAs to Wikidata entity"))
            .collect();
        assert_eq!(
            sameas.len(),
            1,
            "schema:sameAs to an HTTPS Wikidata entity must warn; got: {findings:?}"
        );
    }

    // Case 7: an empty file list renders the "No issues found." banner.
    #[test]
    fn render_audit_empty() {
        let report = audit_files(&[]);
        let text = render_audit(&report);
        assert!(
            text.contains("No issues found"),
            "empty report must render the no-issues banner; got: {text}"
        );
    }
}
