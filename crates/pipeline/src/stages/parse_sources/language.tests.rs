// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

const TINY_LANGUAGE: &str = r#"
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix lang: <https://blackcatinformatics.ca/lang/> .
@prefix logic: <https://blackcatinformatics.ca/logic/> .
gmeow:gmnCodebookCurrent a gmeow:GmnCodebook ;
  gmeow:references gmeow:tinyDictionary, lang:tinyScript ;
  gmeow:gmnDictionaryVersion "3" ; gmeow:gmnGlyphTableVersion "2" .
gmeow:tinyDictionary a gmeow:GmnDictionary ; gmeow:gmnDictionaryVersion "3" .
lang:tinyScript a lang:Script ; lang:hasGrapheme lang:tinyGrapheme .
gmeow:gmnDialectVersions a gmeow:VersionSet ; gmeow:gmnAcceptWindow 1 .
gmeow:tinyLatest logic:versionInfo "1" .
gmeow:tinyMembership a gmeow:VersionMembership ;
  gmeow:versionMember gmeow:tinyLatest ; gmeow:versionSet gmeow:gmnDialectVersions ;
  gmeow:versionRole gmeow:roleLatest .
"#;

fn tiny_catalog(lang: &str) -> super::super::SourceCatalog {
    let parsed = crate::stages::source_load::ParsedAuthoredSources::synthetic_documents([
        (MODULES[0], "text/turtle", ""),
        (LANG, "text/turtle", lang),
        (MODULES[2], "text/turtle", ""),
    ]);
    super::super::SourceCatalog::from_sources(parsed).unwrap()
}

#[test]
fn selected_native_language_context_is_shared_and_requires_its_lineage() {
    let sources = tiny_catalog(TINY_LANGUAGE);
    let first = sources.language().unwrap();
    let again = sources.language().unwrap();
    assert!(std::ptr::eq(first, again));
    assert!(Arc::ptr_eq(&first.dictionary, &again.dictionary));
    assert!(Arc::ptr_eq(&first.codebook, &again.codebook));
    assert_eq!(
        first.dictionary.version(),
        first.codebook.dictionary_version
    );
    assert_eq!(first.dialect.latest_major_key(), "1");
    assert!(first.operator_forms.is_empty());

    let no_lineage = TINY_LANGUAGE
        .split("gmeow:gmnDialectVersions a")
        .next()
        .unwrap();
    let missing = tiny_catalog(no_lineage);
    assert!(
        missing
            .language()
            .err()
            .unwrap()
            .to_string()
            .contains("no GMN dialect lineage")
    );
    assert!(missing.language.get().is_none());
}

#[test]
fn selected_language_context_never_substitutes_an_absent_grounding_module() {
    let parsed = crate::stages::source_load::ParsedAuthoredSources::synthetic_documents([
        (MODULES[0], "text/turtle", ""),
        (LANG, "text/turtle", TINY_LANGUAGE),
    ]);
    let sources = super::super::SourceCatalog::from_sources(parsed).unwrap();
    assert!(
        sources
            .language()
            .err()
            .unwrap()
            .to_string()
            .contains(MODULES[2])
    );
}

#[test]
fn native_label_selection_preserves_rendering_policy_and_order_independence() {
    let first = purrdf::parse_dataset(
        br#"
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
<urn:term> rdfs:label "earlier"@en, "zeta"@x-gmeow-english .
"#,
        "text/turtle",
        None,
    )
    .unwrap();
    let second = purrdf::parse_dataset(
        br#"
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
<urn:term> rdfs:label "alpha"@x-gmeow-english .
<urn:other> rdfs:label "b"@en, "a"@fr .
"#,
        "text/turtle",
        None,
    )
    .unwrap();
    let forward = labels([first.as_ref(), second.as_ref()]);
    assert_eq!(forward, labels([second.as_ref(), first.as_ref()]));
    assert_eq!(forward["urn:term"], "alpha");
    assert_eq!(forward["urn:other"], "a");
}
