// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Independently authored ontology and PO fragments; no real docs model,
//! repository discovery, rendering, authenticated fixture or corpus production.
use super::*;

#[test]
fn projection_literal_i18n_rendering_selects_actual_pages_without_discarding_catalog_translations()
{
    let root = tempfile::tempdir().unwrap();
    let owner = "https://blackcatinformatics.ca/gmeow/slices/authored";
    std::fs::write(
        root.path().join("manifest.ttl"),
        format!("<{owner}> a <https://blackcatinformatics.ca/gmeow/Slice> ."),
    )
    .unwrap();
    std::fs::write(
        root.path().join("module.ttl"),
        format!(
            r#"
@prefix g: <https://blackcatinformatics.ca/gmeow/> .
@prefix l: <https://blackcatinformatics.ca/logic/> .
@prefix r: <http://www.w3.org/2000/01/rdf-schema#> .
g:AuthoredDocumented a l:Class ; r:isDefinedBy <{owner}> .
g:AuthoredFormula a l:Formula ; r:isDefinedBy <{owner}> .
"#
        ),
    )
    .unwrap();
    let catalog =
        purrdf::slice::SliceCatalog::discover(root.path(), gmeow_ns::gmeow_slice_vocab()).unwrap();
    let ownership = purrdf::slice::OwnershipAnalyzer::new(&catalog)
        .analyze()
        .unwrap();
    let pages = gmeow_docs::model::documented_term_iris(&catalog, &ownership);
    let documented = "https://blackcatinformatics.ca/gmeow/AuthoredDocumented";
    let nondocumentable = "https://blackcatinformatics.ca/gmeow/AuthoredFormula";
    let dsl_term = "https://blackcatinformatics.ca/gmeow/AuthoredDslSelector";
    assert!(pages.contains(documented));
    assert!(!pages.contains(nondocumentable));
    assert!(!pages.contains(dsl_term));
    let mut po = String::from("msgid \"\"\nmsgstr \"\"\n\"Language: fr\\n\"\n\n");
    for (term, english, french) in [
        (documented, "authored class", "entité rédigée"),
        (nondocumentable, "authored formula", "expression rédigée"),
        (dsl_term, "authored selector", "sélecteur rédigé"),
    ] {
        po.push_str(&format!(
            "msgctxt \"{term}|rdfs:label\"\nmsgid \"{english}\"\nmsgstr \"{french}\"\n\n"
        ));
    }
    let sources = [OntologyCatalogSource {
        owner_iri: gmeow_docs::ontology_catalogs::MAPPING_DSL_OWNER.to_owned(),
        owner_path: root.path().join("authored-dsl"),
        path: root.path().join("authored-dsl/fr.po"),
        content: po.into_bytes(),
    }];
    let rendered = build_corpus_from_sources(&sources, &pages).unwrap();
    let graph = String::from_utf8(rendered.ntriples).unwrap();
    assert!(graph.contains(&format!("<{}>", docs_rendering_iri("fr", documented))));
    for absent in [nondocumentable, dsl_term] {
        assert!(!graph.contains(&format!("<{}>", docs_rendering_iri("fr", absent))));
    }
    let translated = crate::stages::lang_translation::build_corpus_from_sources(&sources).unwrap();
    let graph = String::from_utf8(translated.ntriples).unwrap();
    for retained in [documented, nondocumentable, dsl_term] {
        let unit = unit_iri(&format!("{retained}|rdfs:label"), "fr");
        assert!(graph.contains(&format!("<{unit}>")));
    }
}
