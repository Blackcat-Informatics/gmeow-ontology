// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Competency-style assertions over the rendered docs pages (R1 of #859).
//!
//! Unlike the byte-exact `insta` goldens in `render_golden.rs`, these are
//! *content-predicate* assertions: they verify the renderer faithfully surfaces
//! the model's own data (a term's IRI/definition/domain/range; a slice's
//! tier/consumers; a recipe's goal/terms; a learning path's audience/recipes), so
//! they survive cosmetic churn while still catching a renderer that drops a
//! section. They mirror the INTENT of the deleted Python `test_reference_pages_*`
//! / `test_recipes_*` / `test_learning_paths_*` against the new Rust renderer.

use gmeow_docs::model::{DocSlice, DocTerm};
use gmeow_docs::render::{term_slug, to_html, to_markdown, Page};
use gmeow_docs::{DocTermCategory, DocsModel};
use std::path::PathBuf;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("crate is at <repo>/crates/docs")
        .to_path_buf()
}

fn model() -> DocsModel {
    DocsModel::discover(&repo_root()).expect("build docs model from live slices")
}

/// The leading run of letters/digits/spaces in `s`, trimmed. The renderer
/// md-escapes table/inline metacharacters (`-`→`\-`, `.`→`\.`, …), so a raw
/// title/definition won't substring-match; this escape-free prefix does.
fn escape_free_probe(s: &str) -> String {
    s.chars()
        .take_while(|c| c.is_alphanumeric() || *c == ' ')
        .collect::<String>()
        .trim()
        .to_string()
}

/// The first (by curie,iri) Property term carrying a definition, a parent, a
/// domain, and a range — so its page exercises every term section. Mirrors the
/// `render_golden.rs` representative-pick idiom for determinism.
fn fully_populated_term(model: &DocsModel) -> &DocTerm {
    let mut candidates: Vec<&DocTerm> = model
        .terms
        .iter()
        .filter(|t| {
            t.category == DocTermCategory::Property
                && t.definition.is_some()
                && !t.parents.is_empty()
                && !t.domain.is_empty()
                && !t.range.is_empty()
        })
        .collect();
    candidates.sort_by(|a, b| a.curie.cmp(&b.curie).then_with(|| a.iri.cmp(&b.iri)));
    candidates
        .first()
        .copied()
        .expect("at least one fully-populated property term exists")
}

/// First slice (by iri) carrying a tier and at least one consumer.
fn slice_with_tier_and_consumers(model: &DocsModel) -> &DocSlice {
    let mut candidates: Vec<&DocSlice> = model
        .slices
        .iter()
        .filter(|s| s.tier.is_some() && !s.consumers.is_empty())
        .collect();
    candidates.sort_by(|a, b| a.iri.cmp(&b.iri));
    candidates
        .first()
        .copied()
        .expect("at least one slice has a tier and consumers")
}

#[test]
fn term_page_surfaces_iri_definition_domain_and_range() {
    let model = model();
    let term = fully_populated_term(&model);
    let slug = term_slug(term);
    let md = to_markdown(&model, &Page::Term(slug.clone()));
    let html = to_html(&model, &Page::Term(slug));

    // The term's own IRI anchors the page (markdown + html).
    assert!(
        md.contains(&term.iri),
        "term md missing its IRI {}",
        term.iri
    );
    assert!(html.contains(&term.iri), "term html missing its IRI");

    // Every populated section renders (the term was chosen to have all of them).
    for heading in ["## Definition", "## Domain", "## Range"] {
        assert!(md.contains(heading), "term md missing section `{heading}`");
    }

    // The definition text surfaces (matched via its escape-free prefix).
    let def = term.definition.as_deref().expect("term has a definition");
    let probe = escape_free_probe(def);
    if probe.len() >= 4 {
        assert!(
            md.contains(&probe),
            "term md missing definition prose `{probe}`"
        );
    }
}

#[test]
fn slice_page_lists_tier_and_consumers() {
    let model = model();
    let slice = slice_with_tier_and_consumers(&model);
    // Slice pages are keyed by slug; the page dir is slices/<slug>. The model's
    // slice slug is the local name of its IRI — reuse render's Page::Slice via the
    // slice identifier the renderer uses (local name of the IRI).
    let slug = slice
        .iri
        .rsplit(['/', '#'])
        .next()
        .unwrap_or(&slice.iri)
        .to_string();
    let md = to_markdown(&model, &Page::Slice(slug));

    assert!(md.contains("| Tier |"), "slice page missing the Tier row");
    assert!(
        md.contains("| Consumers |"),
        "slice page missing the Consumers row"
    );
    // The declared consumer prose is surfaced (matched escape-free).
    let probe = escape_free_probe(&slice.consumers[0]);
    if probe.len() >= 4 {
        assert!(
            md.contains(&probe),
            "slice page missing its consumer `{probe}`"
        );
    }
}

#[test]
fn recipe_index_and_page_surface_goal_and_terms() {
    let model = model();
    assert!(
        !model.recipes.is_empty(),
        "the live docs model must carry dogfooded recipes (#853)"
    );
    let recipe = {
        let mut r: Vec<_> = model.recipes.iter().collect();
        r.sort_by(|a, b| a.slug.cmp(&b.slug));
        r[0]
    };

    let index = to_markdown(&model, &Page::RecipeIndex);
    assert!(
        index.contains("Recipes"),
        "recipe index missing its heading"
    );
    let title_probe = escape_free_probe(&recipe.title);
    assert!(
        index.contains(&title_probe),
        "recipe index missing recipe title `{title_probe}`"
    );

    let page = to_markdown(&model, &Page::Recipe(recipe.slug.clone()));
    assert!(
        page.contains("## Goal"),
        "recipe page missing the Goal section"
    );
    if let Some(curie) = recipe.term_curies.first() {
        assert!(
            page.contains(curie),
            "recipe page missing member term `{curie}`"
        );
    }
}

#[test]
fn learning_path_index_and_page_sequence_audience_goal_and_recipes() {
    let model = model();
    assert!(
        !model.learning_paths.is_empty(),
        "the live docs model must carry dogfooded learning paths (#853)"
    );
    let path = {
        let mut p: Vec<_> = model.learning_paths.iter().collect();
        p.sort_by(|a, b| a.slug.cmp(&b.slug));
        p[0]
    };

    let index = to_markdown(&model, &Page::LearningPathIndex);
    assert!(
        index.contains("| Learning path | Audience | Goal |"),
        "learning-path index missing its table header"
    );
    let title_probe = escape_free_probe(&path.title);
    assert!(
        index.contains(&title_probe),
        "learning-path index missing path title `{title_probe}`"
    );

    let page = to_markdown(&model, &Page::LearningPath(path.slug.clone()));
    assert!(
        page.contains("| Audience |"),
        "learning-path page missing Audience row"
    );
    assert!(
        page.contains("## Goal"),
        "learning-path page missing Goal section"
    );
    if !path.recipe_slugs.is_empty() {
        assert!(
            page.contains("## Recipes"),
            "learning-path page missing the Recipes section"
        );
    }
}
