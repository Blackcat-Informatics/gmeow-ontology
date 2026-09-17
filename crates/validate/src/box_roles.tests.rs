// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

const ONTOLOGY_IRI: &str = "https://blackcatinformatics.ca/gmeow";
const NS: &str = "https://blackcatinformatics.ca/gmeow/";

/// Write `contents` to `name` inside a fresh RAII temp directory.
///
/// The returned [`tempfile::TempDir`] owns the directory: it is removed on
/// drop, including on panic and early return. Bind it to a named `_tmp`
/// (never a bare `_`, which would drop it immediately) so it outlives the
/// path. The file *name* is preserved because the audit dispatches on the
/// `.ttl` extension and on file stems.
fn write_tmp(name: &str, contents: &str) -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().expect("create temp dir");
    let path = dir.path().join(name);
    std::fs::write(&path, contents).unwrap();
    (dir, path)
}

#[test]
fn box_role_audit_passes_for_explicit_typed_role() {
    let (_tmp, path) = write_tmp(
        "gmeow_validate_box_roles_pass.ttl",
        "@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n\
             @prefix ex: <https://example.org/> .\n\
             @prefix owl: <http://www.w3.org/2002/07/owl#> .\n\
             ex:tbox a gmeow:GraphBoxRole .\n\
             gmeow:Documented\n\
                 a owl:Class ;\n\
                 gmeow:graphBoxRole ex:tbox .\n",
    );
    let report = audit_box_roles(std::slice::from_ref(&path), ONTOLOGY_IRI, NS).unwrap();
    assert!(report.ok());
    assert_eq!(report.role_counts.get("https://example.org/tbox"), Some(&1));
}

#[test]
fn box_role_audit_reports_missing_and_invalid_roles() {
    let (_tmp, path) = write_tmp(
        "gmeow_validate_box_roles_missing_invalid.ttl",
        "@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n\
             @prefix ex: <https://example.org/> .\n\
             @prefix owl: <http://www.w3.org/2002/07/owl#> .\n\
             gmeow:MissingRole a owl:Class .\n\
             gmeow:InvalidRole\n\
                 a owl:Class ;\n\
                 gmeow:graphBoxRole ex:notTypedAsRole .\n",
    );
    let report = audit_box_roles(std::slice::from_ref(&path), ONTOLOGY_IRI, NS).unwrap();
    assert!(!report.ok());
    assert_eq!(
        report
            .missing
            .iter()
            .map(|f| f.term.clone())
            .collect::<Vec<_>>(),
        vec!["https://blackcatinformatics.ca/gmeow/MissingRole".to_owned()]
    );
    assert_eq!(
        report
            .invalid
            .iter()
            .map(|f| f.term.clone())
            .collect::<Vec<_>>(),
        vec!["https://blackcatinformatics.ca/gmeow/InvalidRole".to_owned()]
    );
    let text = render_text(&report, ONTOLOGY_IRI, NS);
    assert!(text.contains("Missing roles (1)"), "text was: {text}");
    assert!(text.contains("Invalid roles (1)"), "text was: {text}");
}

#[test]
fn to_diagnostics_report_maps_missing_and_invalid() {
    let (_tmp, path) = write_tmp(
        "gmeow_validate_box_roles_diag.ttl",
        "@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n\
             @prefix ex: <https://example.org/> .\n\
             @prefix owl: <http://www.w3.org/2002/07/owl#> .\n\
             gmeow:MissingRole a owl:Class .\n\
             gmeow:InvalidRole\n\
                 a owl:Class ;\n\
                 gmeow:graphBoxRole ex:notTypedAsRole .\n",
    );
    let audit = audit_box_roles(std::slice::from_ref(&path), ONTOLOGY_IRI, NS).unwrap();
    let report = to_diagnostics_report(&audit, ONTOLOGY_IRI, NS);

    assert_eq!(report.tool, "box-roles");
    assert_eq!(report.error_count(), 2);
    assert_eq!(report.warning_count(), 0);
    let codes: BTreeSet<String> = report.findings.iter().map(|f| f.code.clone()).collect();
    let expected: BTreeSet<String> = [
        "box-roles.missing".to_owned(),
        "box-roles.invalid".to_owned(),
    ]
    .into_iter()
    .collect();
    assert_eq!(codes, expected);
}

#[test]
fn to_diagnostics_report_clean_audit_is_ok() {
    let audit = audit_box_roles(&[], ONTOLOGY_IRI, NS).unwrap();
    let report = to_diagnostics_report(&audit, ONTOLOGY_IRI, NS);
    assert!(report.ok());
    assert_eq!(report.findings.len(), 0);
}

#[test]
fn box_role_audit_with_empty_paths_audits_nothing() {
    let report = audit_box_roles(&[], ONTOLOGY_IRI, NS).unwrap();
    assert!(report.ok());
    assert_eq!(report.term_count, 0);
    assert!(report.role_counts.is_empty());
    assert!(report.missing.is_empty());
    assert!(report.invalid.is_empty());
    let text = render_text(&report, ONTOLOGY_IRI, NS);
    assert!(text.contains("Typed GMEOW terms: 0"), "text was: {text}");
    assert!(
        text.contains("All typed GMEOW terms have explicit typed graph-box roles."),
        "text was: {text}"
    );
}
