// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

#[test]
fn source_observation_names_the_uncovered_construct_with_its_ledger_class() {
    let dataset = parse_dataset(
        br#"
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
_:a__b gmeow:predicate gmeow:object .
"#,
        "text/turtle",
        None,
    )
    .unwrap();
    let source = observe(&dataset, &GmnDictionary::default());
    let path = "slices/grounding/math/module.ttl";
    let observations = Observations {
        sources: BTreeMap::from([(path.to_owned(), source)]),
    };
    let reports = observations.reports();
    assert!(!reports.roundtrip.is_clean());
    assert_eq!(reports.roundtrip.failures.len(), 1);
    assert_eq!(reports.roundtrip.failures[0].path, path);
    assert_eq!(
        reports.roundtrip.failures[0].failure_class(),
        Gmn1Error::CLASS_UNCOVERED_TERM
    );
    assert_eq!(reports.coverage.uncovered_quad_count, 1);
}

#[test]
fn required_grounding_domain_rejects_a_missing_module_or_examples() {
    let root = tempfile::tempdir().unwrap();
    assert!(input_files(root.path()).is_err());
    for slice in ["logic", "lang", "math"] {
        let dir = root.path().join(format!("slices/grounding/{slice}"));
        std::fs::create_dir_all(dir.join("examples")).unwrap();
        // Explicit inert input: this test only exercises source selection.
        std::fs::write(dir.join("module.ttl"), "").unwrap();
        std::fs::write(dir.join("examples/tiny.ttl"), "").unwrap();
    }
    assert_eq!(input_files(root.path()).unwrap().len(), 6);
    let module = root.path().join("slices/grounding/math/module.ttl");
    std::fs::remove_file(&module).unwrap();
    assert!(input_files(root.path()).is_err());
    std::fs::write(module, "").unwrap();
    std::fs::remove_file(root.path().join("slices/grounding/math/examples/tiny.ttl")).unwrap();
    assert!(input_files(root.path()).is_err());
}

#[test]
fn unchanged_source_reuses_its_complete_classification() {
    let dataset = parse_dataset(
        br#"
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
_:claim gmeow:label "porte"@fr .
"#,
        "text/turtle",
        None,
    )
    .unwrap();
    let source = observe(&dataset, &GmnDictionary::default());
    assert!(source.roundtrip.is_ok());
    assert_eq!(source.quads, source.retained_quads);
    assert_eq!(source.coverage, source.without_decimal);
}

#[test]
fn source_observation_preserves_decimal_removal_and_reference_content() {
    let dataset = parse_dataset(
        br#"
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix xsd: <http://www.w3.org/2001/XMLSchema#> .
_:claim gmeow:confidence "0.25"^^xsd:decimal ; gmeow:label "porte"@fr .
"#,
        "text/turtle",
        None,
    )
    .unwrap();
    let source = observe(&dataset, &GmnDictionary::default());
    assert!(source.roundtrip.is_ok(), "{:?}", source.roundtrip);
    assert_eq!(source.quads, 2);
    assert!(source.coverage.count(Gmn1ConstructCategory::LiteralDecimal) > 0);
    assert_eq!(source.retained_quads, 1);
    assert_eq!(
        source
            .without_decimal
            .count(Gmn1ConstructCategory::LiteralDecimal),
        0
    );
    assert!(source.without_decimal.uncovered.is_empty());
}

#[test]
fn source_observation_retains_domain_failure_through_observation_encoding() {
    let dataset = parse_dataset(br#"
<https://blackcatinformatics.ca/gmeow/a> <https://blackcatinformatics.ca/gmeow/p> <https://blackcatinformatics.ca/gmeow/b> <urn:standpoint> .
"#, "application/n-quads", None).unwrap();
    let source = observe(&dataset, &GmnDictionary::default());
    assert!(matches!(
        source.roundtrip,
        Err(Gmn1Error::NamedGraphOutOfDomain { .. })
    ));
    assert_eq!(source.coverage.uncovered.len(), 1);
    let observations = Observations {
        sources: BTreeMap::from([("synthetic.nq".to_owned(), source)]),
    };
    let original = &observations.sources["synthetic.nq"];
    let restored = from_bytes(&serde_json::to_vec(&observations).unwrap()).unwrap();
    let decoded = &restored.sources["synthetic.nq"];
    assert_eq!(decoded.roundtrip, original.roundtrip);
    assert_eq!(decoded.coverage, original.coverage);
    let reports = restored.reports();
    assert_eq!(reports.roundtrip.failures.len(), 1);
    assert_eq!(reports.roundtrip.failures[0].path, "synthetic.nq");
    assert_eq!(reports.coverage.uncovered_quad_count, 1);
}
