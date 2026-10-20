// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

/// A tiny authored translation pair retains both actual languages through the
/// production glossary emitter; this never builds the repository corpus.
#[test]
fn glossary_words_preserve_source_and_translation_languages() {
    let entries = [Entry {
        glossary_iri: "urn:synthetic:glossary".into(),
        slice_iri: "urn:synthetic:slice".into(),
        lang: "fr".into(),
        entry_iri: "urn:synthetic:entry".into(),
        term: "urn:synthetic:term".into(),
        predicate: "http://www.w3.org/2000/01/rdf-schema#label".into(),
        source: "a source word".into(),
        translation: "bonjour".into(),
        concept_iri: "urn:synthetic:concept".into(),
        sense_iri: "urn:synthetic:sense".into(),
        unit_iri: "urn:synthetic:unit".into(),
    }];
    let wire = emit_ntriples(&entries);
    let dataset = purrdf::parse_dataset(&wire, "application/n-triples", None).unwrap();
    let mut observed = std::collections::BTreeMap::new();
    for quad in dataset.quads() {
        if let (purrdf::TermRef::Iri(predicate), purrdf::TermRef::Literal { language, .. }) =
            (dataset.resolve(quad.p), dataset.resolve(quad.o))
            && matches!(predicate, p if p == iri(GMEOW_NS, "glossarySource") || p == iri(GMEOW_NS, "glossaryTranslation"))
        {
            observed.insert(predicate.to_owned(), language.map(str::to_owned));
        }
    }
    assert_eq!(observed.len(), 2);
    assert_eq!(
        observed[&iri(GMEOW_NS, "glossarySource")].as_deref(),
        Some("x-gmeow-english")
    );
    assert_eq!(
        observed[&iri(GMEOW_NS, "glossaryTranslation")].as_deref(),
        Some("fr")
    );
}

#[test]
fn glossary_table_cells_escape_structural_characters() {
    assert_eq!(cell("a | b"), "a \\| b");
    assert_eq!(cell("line1\nline2\tx"), "line1 line2 x");
    assert_eq!(
        localname("https://blackcatinformatics.ca/gmeow/EntityExistence"),
        "EntityExistence"
    );
}

#[test]
fn tbx_text_and_attributes_escape_xml() {
    assert_eq!(xml_text("a & b < c > d"), "a &amp; b &lt; c &gt; d");
    assert_eq!(xml_attr("x\"y"), "x&quot;y");
}

#[test]
fn glossary_stage_inputs_share_strict_catalog_discovery() {
    let root = tempfile::tempdir().unwrap();
    let slice = root.path().join("slices/core/example");
    let i18n = slice.join("i18n");
    std::fs::create_dir_all(&i18n).unwrap();
    let module = slice.join("module.ttl");
    let catalog = i18n.join("fr.po");
    std::fs::write(&module, "# synthetic source\n").unwrap();
    std::fs::write(&catalog, "# synthetic catalog\n").unwrap();
    let stage = GlossaryTableStage;
    let mut expected = vec![module, catalog.clone()];
    expected.sort();
    assert_eq!(stage.input_files(root.path()).unwrap(), expected);
    std::fs::remove_file(catalog).unwrap();
    std::fs::remove_dir(&i18n).unwrap();
    std::fs::write(&i18n, "not a directory").unwrap();
    let failure = stage.input_files(root.path()).unwrap_err();
    assert!(failure.downcast_ref::<std::io::Error>().is_some());
    assert_eq!(
        failure.inner().source_ctx.location.path.as_deref(),
        i18n.to_str()
    );
}
