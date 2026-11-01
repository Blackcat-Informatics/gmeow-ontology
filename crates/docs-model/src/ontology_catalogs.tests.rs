// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Original tiny authored ontology/PO bytes only; no repository catalog, GTS,
//! authenticated fixture, rendering or corpus producer is invoked.
use super::*;

const VOCABULARY: &str = r#"
<https://blackcatinformatics.ca/gmeow/mapping-dsl> a <https://blackcatinformatics.ca/logic/Ontology> .
<urn:authored:selector> <http://www.w3.org/2000/01/rdf-schema#label> "selected predicate"@x-gmeow-english .
"#;
const PO: &str = "msgid \"\"\nmsgstr \"\"\n\"Language: fr\\n\"\n\nmsgctxt \"urn:authored:selector|rdfs:label\"\nmsgid \"selected predicate\"\nmsgstr \"prédicat sélectionné\"\n";

fn authored_root() -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(root.path().join("slices/control/example/i18n")).unwrap();
    std::fs::write(
        root.path().join("slices/control/example/manifest.ttl"),
        "<urn:authored:slice> a <https://blackcatinformatics.ca/gmeow/Slice> .",
    )
    .unwrap();
    std::fs::write(
        root.path().join("slices/control/example/module.ttl"),
        "# independent module\n",
    )
    .unwrap();
    std::fs::create_dir_all(root.path().join("dsl/mappings/i18n")).unwrap();
    std::fs::write(root.path().join(MAPPING_DSL_SOURCES[0]), VOCABULARY).unwrap();
    std::fs::write(
        root.path().join(MAPPING_DSL_SOURCES[1]),
        "# independent registry\n",
    )
    .unwrap();
    std::fs::write(root.path().join("dsl/mappings/i18n/fr.po"), PO).unwrap();
    root
}

#[test]
fn projection_literal_i18n_uses_actual_dsl_owner_across_native_index_export_and_reviewed_inputs() {
    let root = authored_root();
    let catalog = SliceCatalog::discover(
        &root.path().join("slices"),
        gmeow_ns::gmeow_slice_vocab(),
        &mut purrdf::CanonicalizationControl::new(purrdf::RDFC_V2_CORPUS_POLICY, None),
    )
    .unwrap();
    let sources = for_repository(root.path(), &catalog).unwrap();
    assert_eq!(sources.len(), 1);
    assert_eq!(sources[0].owner_iri, MAPPING_DSL_OWNER);
    assert_eq!(sources[0].content, PO.as_bytes());
    let discovered = discover(root.path()).unwrap();
    assert_eq!(discovered[0].owner_iri, sources[0].owner_iri);
    assert_eq!(discovered[0].path, sources[0].path);
    let translations = crate::i18n::Translations::from_ontology_catalogs(&catalog, &sources);
    assert_eq!(
        translations.lookup(
            "urn:authored:selector",
            "http://www.w3.org/2000/01/rdf-schema#label",
            "fr"
        ),
        Some("prédicat sélectionné")
    );
    let exported = crate::i18n_compile::iter_po_catalogs(root.path()).unwrap();
    assert_eq!(exported[0].slice, MAPPING_DSL_OWNER);
    assert_eq!(exported[0].slice_path, "dsl/mappings");
    let coverage = crate::reviewed_coverage::reviewed_coverage_map(root.path()).unwrap();
    assert_eq!(
        coverage["dsl/mappings/i18n/fr.po"],
        vec!["urn:authored:selector|http://www.w3.org/2000/01/rdf-schema#label"]
    );
    let inputs = crate::reviewed_coverage::input_files(root.path()).unwrap();
    for selected in [
        "dsl/mappings/i18n/fr.po",
        MAPPING_DSL_SOURCES[0],
        MAPPING_DSL_SOURCES[1],
    ] {
        assert!(inputs.contains(&root.path().join(selected)));
    }
    assert!(crate::i18n_compile::lint_po_files(root.path(), 100.0).ok());
}

#[test]
fn projection_literal_i18n_discovery_does_not_load_unrelated_slice_artifacts_or_foreign_catalogs() {
    let root = authored_root();
    std::fs::create_dir_all(root.path().join("slices/control/example/examples")).unwrap();
    std::fs::write(
        root.path()
            .join("slices/control/example/examples/broken.ttl"),
        "not valid RDF {{",
    )
    .unwrap();
    std::fs::create_dir_all(root.path().join("unregistered/i18n")).unwrap();
    std::fs::write(root.path().join("unregistered/i18n/fr.po"), "broken PO").unwrap();
    let sources = discover(root.path()).unwrap();
    assert_eq!(sources.len(), 1);
    assert_eq!(sources[0].owner_iri, MAPPING_DSL_OWNER);
}

#[test]
fn projection_literal_i18n_refuses_missing_or_ambiguous_selected_owner_and_required_dsl_sources() {
    let root = authored_root();
    let slice_po = root.path().join("slices/control/example/i18n/fr.po");
    std::fs::write(&slice_po, PO).unwrap();
    let manifest = root.path().join("slices/control/example/manifest.ttl");
    for source in [
        "# no owner",
        "<urn:a> a <https://blackcatinformatics.ca/gmeow/Slice> . <urn:b> a <https://blackcatinformatics.ca/gmeow/Slice> .",
        "<urn:a> a <https://blackcatinformatics.ca/gmeow/Slice> . [] a <https://blackcatinformatics.ca/gmeow/Slice> .",
    ] {
        std::fs::write(&manifest, source).unwrap();
        assert!(discover(root.path()).is_err());
    }
    std::fs::write(
        &manifest,
        "<urn:authored:slice> a <https://blackcatinformatics.ca/gmeow/Slice> .",
    )
    .unwrap();
    let vocabulary = root.path().join(MAPPING_DSL_SOURCES[0]);
    std::fs::write(
        &vocabulary,
        "<urn:impostor> a <https://blackcatinformatics.ca/logic/Ontology> .",
    )
    .unwrap();
    assert!(discover(root.path()).is_err());
    std::fs::write(
        &vocabulary,
        format!(
            "{VOCABULARY}\n<urn:extra-owner> a <https://blackcatinformatics.ca/logic/Ontology> ."
        ),
    )
    .unwrap();
    assert!(discover(root.path()).is_err());
    std::fs::write(&vocabulary, VOCABULARY).unwrap();
    std::fs::remove_file(root.path().join(MAPPING_DSL_SOURCES[1])).unwrap();
    assert!(discover(root.path()).is_err());
    std::fs::remove_file(vocabulary).unwrap();
    assert!(discover(root.path()).is_err());
}

#[test]
fn projection_literal_i18n_refuses_family_only_partial_dsl_owner_but_allows_fully_absent_owner() {
    let root = tempfile::tempdir().unwrap();
    assert!(dsl_sources(root.path()).unwrap().is_empty());
    std::fs::create_dir_all(root.path().join("dsl/mappings")).unwrap();
    std::fs::write(
        root.path().join(MAPPING_DSL_SOURCES[1]),
        "# selected registry\n",
    )
    .unwrap();
    let error = dsl_sources(root.path()).unwrap_err().to_string();
    assert!(error.contains("required vocabulary owner"));
    assert!(crate::i18n_compile::authored_turtle_files(root.path()).is_err());
    assert!(discover(root.path()).is_err());
}

#[test]
fn cq_rationale_i18n_test_dsl_owner_reaches_all_shared_catalog_consumers() {
    let root = authored_root();
    std::fs::create_dir_all(root.path().join("dsl/tests/i18n")).unwrap();
    std::fs::write(root.path().join(TEST_DSL_SOURCES[0]), r#"
<https://blackcatinformatics.ca/gmeow/test-dsl> a <https://blackcatinformatics.ca/logic/Ontology> .
<urn:authored:rationale> <http://www.w3.org/2000/01/rdf-schema#label> "authored rationale"@x-gmeow-english .
"#).unwrap();
    let po = "msgid \"\"\nmsgstr \"\"\n\"Language: fr\\n\"\n\nmsgctxt \"urn:authored:rationale|http://www.w3.org/2000/01/rdf-schema#label\"\nmsgid \"authored rationale\"\nmsgstr \"justification rédigée\"\n";
    std::fs::write(root.path().join("dsl/tests/i18n/fr.po"), po).unwrap();
    let catalog = SliceCatalog::discover(
        &root.path().join("slices"),
        gmeow_ns::gmeow_slice_vocab(),
        &mut purrdf::CanonicalizationControl::new(purrdf::RDFC_V2_CORPUS_POLICY, None),
    )
    .unwrap();
    let sources = for_repository(root.path(), &catalog).unwrap();
    assert_eq!(sources.len(), 2);
    let selected = sources
        .iter()
        .find(|source| source.owner_iri == TEST_DSL_OWNER)
        .unwrap();
    assert_eq!(selected.owner_path, root.path().join("dsl/tests"));
    assert_eq!(selected.content, po.as_bytes());
    assert_eq!(discover(root.path()).unwrap().len(), 2);
    let translations = crate::i18n::Translations::from_ontology_catalogs(&catalog, &sources);
    assert_eq!(
        translations.lookup(
            "urn:authored:rationale",
            "http://www.w3.org/2000/01/rdf-schema#label",
            "fr"
        ),
        Some("justification rédigée")
    );
    let exported = crate::i18n_compile::iter_po_catalogs(root.path()).unwrap();
    assert!(
        exported
            .iter()
            .any(|record| record.slice == TEST_DSL_OWNER && record.slice_path == "dsl/tests")
    );
    let coverage = crate::reviewed_coverage::reviewed_coverage_map(root.path()).unwrap();
    assert_eq!(
        coverage["dsl/tests/i18n/fr.po"],
        ["urn:authored:rationale|http://www.w3.org/2000/01/rdf-schema#label"]
    );
    let inputs = crate::reviewed_coverage::input_files(root.path()).unwrap();
    assert!(inputs.contains(&root.path().join(TEST_DSL_SOURCES[0])));
    assert!(inputs.contains(&root.path().join("dsl/tests/i18n/fr.po")));
    assert!(!inputs.contains(&root.path().join("dsl/tests/manifest.ttl")));
    assert!(
        crate::i18n_compile::authored_turtle_files(root.path())
            .unwrap()
            .contains(&root.path().join(TEST_DSL_SOURCES[0]))
    );
    assert!(crate::i18n_compile::lint_po_files(root.path(), 100.0).ok());
}

#[test]
fn cq_rationale_i18n_refuses_partial_wrong_or_ambiguous_test_dsl_owners() {
    let root = authored_root();
    std::fs::create_dir_all(root.path().join("dsl/tests/i18n")).unwrap();
    assert!(dsl_sources(root.path()).is_err());
    for source in [
        "<urn:wrong> a <https://blackcatinformatics.ca/logic/Ontology> .",
        "<https://blackcatinformatics.ca/gmeow/test-dsl> a <https://blackcatinformatics.ca/logic/Ontology> . [] a <https://blackcatinformatics.ca/logic/Ontology> .",
        "# absent owner",
    ] {
        std::fs::write(root.path().join(TEST_DSL_SOURCES[0]), source).unwrap();
        assert!(discover(root.path()).is_err());
    }
    std::fs::write(root.path().join(TEST_DSL_SOURCES[0]), "<https://blackcatinformatics.ca/gmeow/test-dsl> a <https://blackcatinformatics.ca/logic/Ontology> .").unwrap();
    std::fs::remove_dir(root.path().join("dsl/tests/i18n")).unwrap();
    assert!(discover(root.path()).is_err());
}
