// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

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
