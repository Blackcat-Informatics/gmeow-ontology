// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Multilingual render-selection tests (R3): fr/zh fallback.
//!
//! Uses controlled `Translations::from_entries` fixtures (the precedent),
//! NOT live `.po` content, so the assertions are deterministic and decoupled from
//! translation churn. Verifies that `render_site_lang` emits the translated
//! label/definition where a catalog entry exists and falls back to the English
//! carrier per-term where it does not, and that the page graph (and the
//! no-dangling-link invariant) is language-independent.

// Rich colored line-diffs on assert_eq! failure; shadows the std macro
// for this file. Identical behaviour on pass; insta snapshots are unaffected.
use pretty_assertions::assert_eq;

use gmeow_docs::lint::lint;
use gmeow_docs::model::{DocTerm, DocTermCategory};
use gmeow_docs::render::{Page, render_site_lang, term_slug};
use gmeow_docs::{DocsModel, Translations};

mod common;

// Marker strings are deliberately metacharacter-free (no `_`/`-`/`.`): the
// renderer md-escapes those, which would break a substring match.

const RDFS_LABEL: &str = "http://www.w3.org/2000/01/rdf-schema#label";
const SKOS_DEFINITION: &str = "http://www.w3.org/2004/02/skos/core#definition";

fn iri(local: &str) -> String {
    format!("https://blackcatinformatics.ca/gmeow/{local}")
}

fn labeled_term(local: &str, label: &str, def: &str) -> DocTerm {
    DocTerm {
        iri: iri(local),
        curie: format!("gmeow:{local}"),
        label: Some(label.to_string()),
        definition: Some(def.to_string()),
        category: DocTermCategory::Class,
        owner_slice: iri("slices/core/test"),
        parents: Vec::new(),
        domain: Vec::new(),
        range: Vec::new(),
        scope_notes: Vec::new(),
        examples: Vec::new(),
        use_when: Vec::new(),
        avoid_when: Vec::new(),
        how_to_use: Vec::new(),
        use_for_consumer: Vec::new(),
        avoid_for_consumer: Vec::new(),
        ..Default::default()
    }
}

/// Model with two terms; `Alpha` carries a French catalog (label+definition),
/// `Beta` carries none. Languages: english + fr + zh.
fn bilingual_model() -> DocsModel {
    let terms = vec![
        labeled_term("Alpha", "ALPHALABELEN", "ALPHADEFEN"),
        labeled_term("Beta", "BETALABELEN", "BETADEFEN"),
    ];
    let translations = Translations::from_entries(
        [
            (
                (iri("Alpha"), RDFS_LABEL.to_string(), "fr".to_string()),
                "ALPHALABELFR".to_string(),
            ),
            (
                (iri("Alpha"), SKOS_DEFINITION.to_string(), "fr".to_string()),
                "ALPHADEFFR".to_string(),
            ),
        ],
        ["fr".to_string(), "zh".to_string()],
    );

    DocsModel {
        title: "T".to_string(),
        version: "2".to_string(),
        slices: Vec::new(),
        terms,
        dependency_edges: Vec::new(),
        mapping_sets: Vec::new(),
        linkages: Vec::new(),
        examples: Vec::new(),
        fixtures: Vec::new(),
        shapes: Vec::new(),
        competencies: Vec::new(),
        grammars: Vec::new(),
        loss_targets: Vec::new(),
        worked_instances: Vec::new(),
        concerns: Vec::new(),
        external_terms: Vec::new(),
        recipes: Vec::new(),
        learning_paths: Vec::new(),
        constraint_rules: Vec::new(),
        advice_entries: Vec::new(),
        four_boxes: None,
        concept_doi: None,
        pipeline: None,
        available_languages: vec!["english".to_string(), "fr".to_string(), "zh".to_string()],
        translations,
        ui_catalog: Default::default(),
        reasoning: None,
        diagnostics: None,
        term_loss: None,
        schema_fragments: None,
        lang: String::new(),
    }
}

fn term_md<'a>(site: &'a gmeow_docs::render::Site, term: &DocTerm) -> &'a str {
    let path = Page::Term(term_slug(term)).md_path();
    let bytes = site
        .files
        .get(&path)
        .unwrap_or_else(|| panic!("term page {path} missing from site"));
    std::str::from_utf8(bytes).expect("utf8")
}

#[test]
fn french_render_translates_where_present_and_falls_back_per_term() {
    let model = bilingual_model();
    let alpha = &model.terms[0];
    let beta = &model.terms[1];

    let fr = render_site_lang(&model, "fr");

    // Alpha has a French catalog → its page shows the French definition/label.
    let alpha_fr = term_md(&fr, alpha);
    assert!(
        alpha_fr.contains("ALPHADEFFR"),
        "Alpha fr def not translated"
    );
    assert!(
        alpha_fr.contains("ALPHALABELFR"),
        "Alpha fr label not translated"
    );
    assert!(
        !alpha_fr.contains("ALPHADEFEN"),
        "Alpha fr page must not retain the English carrier definition"
    );

    // Beta has NO French catalog → it falls back to the English carrier.
    let beta_fr = term_md(&fr, beta);
    assert!(
        beta_fr.contains("BETADEFEN"),
        "Beta fr must fall back to English def"
    );
}

#[test]
fn chinese_render_falls_back_to_english_when_no_catalog() {
    // zh is an available language but no zh entries exist → every term falls back
    // to the English carrier (per-term fallback, not a hard failure).
    let model = bilingual_model();
    let zh = render_site_lang(&model, "zh");
    let alpha_zh = term_md(&zh, &model.terms[0]);
    assert!(
        alpha_zh.contains("ALPHADEFEN"),
        "Alpha zh must fall back to the English carrier definition"
    );
    assert!(
        !alpha_zh.contains("ALPHADEFFR"),
        "zh must not leak the French value"
    );
}

#[test]
fn translated_tree_preserves_the_no_dangling_link_invariant() {
    // Slugs/IRIs are language-independent, so the fr tree of the LIVE model (which
    // has every category page the static nav links to) must lint as cleanly as the
    // English one — zero dangling-link / broken-anchor errors. (A synthetic
    // minimal model would dangle on the static nav's category links.) The fr tree
    // is read from the shared per-language cache (rendered once by `prime`); the
    // lint pass walks it against the live model's IRIs.
    let model = common::cached_model();
    let fr = common::cached_site_lang("fr");
    let report = lint(&model, &fr);
    assert_eq!(
        report.error_count(),
        0,
        "fr tree must have no link errors: {:?}",
        report.legacy_errors()
    );
}

#[test]
fn language_render_is_deterministic() {
    let model = bilingual_model();
    assert_eq!(
        render_site_lang(&model, "fr"),
        render_site_lang(&model, "fr"),
        "fr render must be byte-stable across calls"
    );
}
