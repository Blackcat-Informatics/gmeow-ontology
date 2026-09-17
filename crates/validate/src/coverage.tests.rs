// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

const NS: &str = "https://blackcatinformatics.ca/gmeow/";

/// Write `contents` to `name` inside a fresh RAII temp directory.
///
/// The returned [`tempfile::TempDir`] owns the directory: it is removed on
/// drop, including on panic and early return. Bind it to a named `_tmp`
/// (never a bare `_`, which would drop it immediately) so it outlives the
/// path. The file *name* is preserved because the coverage walk dispatches
/// on the `.ttl` extension.
fn write_tmp(name: &str, contents: &str) -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().expect("create temp dir");
    let path = dir.path().join(name);
    std::fs::write(&path, contents).unwrap();
    (dir, path)
}

#[test]
fn classifies_covered_gap_and_ignored() {
    let (_tmp, path) = write_tmp(
        "gmeow_validate_coverage_basic.ttl",
        "@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n\
             @prefix skos: <http://www.w3.org/2004/02/skos/core#> .\n\
             @prefix foaf: <http://xmlns.com/foaf/0.1/> .\n\
             @prefix ex: <https://example.org/> .\n\
             @prefix owl: <http://www.w3.org/2002/07/owl#> .\n\
             gmeow:thing a gmeow:Email ;\n\
                 gmeow:addressValue \"x\" ;\n\
                 foaf:homepage ex:hp ;\n\
                 ex:unaligned ex:y ;\n\
                 owl:sameAs ex:z .\n\
             ex:other a foaf:Person, skos:Concept, owl:Class .\n",
    );
    let mut aligned = BTreeSet::new();
    aligned.insert("http://xmlns.com/foaf/0.1/Person".to_owned());
    aligned.insert("http://xmlns.com/foaf/0.1/homepage".to_owned());
    let sets = coverage_analyze(std::slice::from_ref(&path), &aligned, NS).unwrap();

    // Classes: gmeow:Email + foaf:Person covered; skos:Concept covered
    // (recommended); owl:Class ignored; no gap classes here.
    assert!(sets.covered_classes.contains(&format!("{NS}Email")));
    assert!(
        sets.covered_classes
            .contains("http://xmlns.com/foaf/0.1/Person")
    );
    assert!(
        sets.covered_classes
            .contains("http://www.w3.org/2004/02/skos/core#Concept")
    );
    assert!(
        !sets
            .covered_classes
            .iter()
            .any(|c| c.starts_with("http://www.w3.org/2002/07/owl#"))
    );
    assert!(sets.gap_classes.is_empty());

    // Predicates: gmeow:addressValue + foaf:homepage covered; ex:unaligned
    // gap; owl:sameAs + rdf:type ignored.
    assert!(
        sets.covered_predicates
            .contains(&format!("{NS}addressValue"))
    );
    assert!(
        sets.covered_predicates
            .contains("http://xmlns.com/foaf/0.1/homepage")
    );
    assert!(
        sets.gap_predicates
            .contains("https://example.org/unaligned")
    );
    assert!(
        !sets
            .covered_predicates
            .iter()
            .any(|p| p.starts_with("http://www.w3.org/2002/07/owl#"))
    );
    assert!(!sets.gap_predicates.iter().any(|p| p.contains("sameAs")));
}

#[test]
fn to_diagnostics_emits_info_gaps_and_stays_ok() {
    // One covered + one gap each; only the gaps surface as info findings.
    let mut report = CoverageReport::default();
    report
        .covered_classes
        .insert("http://xmlns.com/foaf/0.1/Person".to_owned());
    report
        .gap_classes
        .insert("https://example.org/UnalignedClass".to_owned());
    report
        .covered_predicates
        .insert(format!("{NS}addressValue"));
    report
        .gap_predicates
        .insert("https://example.org/unalignedPredicate".to_owned());

    let diag = coverage_to_diagnostics(&report);
    assert_eq!(diag.tool, "coverage");
    assert!(diag.ok(), "info-only report must stay ok");
    assert_eq!(diag.error_count(), 0);
    assert_eq!(diag.warning_count(), 0);
    assert_eq!(diag.findings.len(), 2);
    assert!(diag.findings.iter().all(|f| f.severity == Severity::Info));
    let codes: BTreeSet<&str> = diag.findings.iter().map(|f| f.code.as_str()).collect();
    let expected: BTreeSet<&str> = ["coverage.gap-class", "coverage.gap-predicate"]
        .into_iter()
        .collect();
    assert_eq!(codes, expected);
    // The gap IRI rides as the finding's logical location.
    let class_finding = diag
        .findings
        .iter()
        .find(|f| f.code == "coverage.gap-class")
        .unwrap();
    assert_eq!(
        class_finding.locations[0].logical.as_deref(),
        Some("https://example.org/UnalignedClass")
    );
}

#[test]
fn run_coverage_over_real_fixtures() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let fixtures = root.join("tests").join("fixtures").join("coverage");
    let mappings = root.join("generated").join("mappings");
    let report = run_coverage(&fixtures, &mappings, NS).unwrap();

    // Covered classes: GMEOW-aligned externals + a GMEOW-native class.
    for iri in [
        "http://xmlns.com/foaf/0.1/Person",
        "https://schema.org/Person",
        "https://schema.org/Organization",
        &format!("{NS}EmailMessage"),
    ] {
        assert!(
            report.covered_classes.contains(iri),
            "expected covered class {iri}; got {:?}",
            report.covered_classes
        );
    }

    // Covered predicates: a GMEOW-native predicate + SSSOM-aligned externals
    // (these pin the aligned_iris SSSOM walk end-to-end).
    for iri in [
        format!("{NS}addressValue"),
        "https://schema.org/description".to_owned(),
        "https://schema.org/url".to_owned(),
        "http://xmlns.com/foaf/0.1/homepage".to_owned(),
    ] {
        assert!(
            report.covered_predicates.contains(&iri),
            "expected covered predicate {iri}; got {:?}",
            report.covered_predicates
        );
    }

    // The slice is intentionally partial, so there are real gaps.
    assert!(!report.gap_classes.is_empty());
    let cc = report.class_coverage();
    assert!(cc > 0.0 && cc <= 1.0, "class_coverage out of range: {cc}");

    // Covered and gap sets are disjoint in both dimensions.
    assert!(
        report
            .covered_classes
            .intersection(&report.gap_classes)
            .next()
            .is_none()
    );
    assert!(
        report
            .covered_predicates
            .intersection(&report.gap_predicates)
            .next()
            .is_none()
    );
}
