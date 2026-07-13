// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Fixed-subject golden for the per-term Python (Pydantic) + Rust example-syntax
//! tabs (issue 1408, gap G5).
//!
//! `python_syntax_tab` / `rust_syntax_tab` (`crates/docs/src/render.rs`) render
//! only when `model.schema_fragments.schema_by_term` carries an entry for the
//! term — the per-term JSON-Schema/OpenAPI fragment digest the production
//! pipeline folds from the live `stage-export-json-schema` product and attaches
//! via `DocsModel::attach_schema_fragments` in `stage-docs-render`. The shared
//! `common::cached_model()` fixture never runs that pipeline stage, so its
//! `schema_fragments` is always `None` and — before this test — neither tab had
//! ever been exercised by a committed gate: a regression collapsing either
//! provider to `None` (an empty/absent tab) would have passed silently.
//!
//! This test attaches a SYNTHETIC digest to a REAL class term drawn from the live
//! cached model — mirroring the same `attach_diagnostics`/`attach_term_loss`
//! precedent in `enrichment_golden.rs` — keyed by that term's own IRI (the exact
//! key both providers read). Everything else (the term's curie, owner slice, and
//! the properties that shape the synthesized worked instance) is genuine
//! live-model data, so the rendered Python/Rust bodies are the real production
//! output for a real modeled class, not a hand-built `DocSyntaxTab`.

use gmeow_docs::model::SchemaFragmentDigest;
use gmeow_docs::render::{Page, term_slug, to_markdown};
use gmeow_docs::{DocTerm, DocTermCategory, DocsModel};

mod common;

/// The first documented CLASS (by stable curie/iri sort) that is also the
/// `rdfs:domain` of at least one documented property — a deterministic,
/// richly-modeled subject whose synthesized quickstart is a multi-field
/// instance (`<subject> a C ; p1 ... ; p2 ... .`), not just a bare `a C .`
/// skeleton, so the worked instance the Python/Rust tabs validate is
/// non-trivial.
fn class_term_with_properties(model: &DocsModel) -> &DocTerm {
    let mut candidates: Vec<&DocTerm> = model
        .terms
        .iter()
        .filter(|t| {
            t.category == DocTermCategory::Class
                && model.terms.iter().any(|p| {
                    p.category == DocTermCategory::Property && p.domain.iter().any(|d| d == &t.iri)
                })
        })
        .collect();
    candidates.sort_by(|a, b| a.curie.cmp(&b.curie).then_with(|| a.iri.cmp(&b.iri)));
    candidates
        .first()
        .copied()
        .expect("the live model has at least one documented class that is a property's domain")
}

/// Extract the exact body of a `**{lang}**`-labeled fenced code block emitted by
/// `append_syntax_tabs` — i.e. the exact `DocSyntaxTab::body` string, byte for
/// byte. Hard-fails if the fence never opens or never closes, so a regression to
/// an absent tab (no fence at all) reds this helper rather than silently
/// snapshotting/asserting against an empty string.
fn fenced_body(md: &str, lang: &str) -> String {
    let open = format!("```{lang}\n");
    let start = md
        .find(&open)
        .unwrap_or_else(|| panic!("no ```{lang} fenced block found in rendered page:\n{md}"));
    let body_start = start + open.len();
    let close = "\n```\n";
    let end = md[body_start..]
        .find(close)
        .unwrap_or_else(|| panic!("```{lang}  fenced block never closes in rendered page"));
    md[body_start..body_start + end].to_string()
}

#[test]
fn python_and_rust_syntax_tabs_render_for_a_modeled_class() {
    let base = common::cached_model();
    let term = class_term_with_properties(&base).clone();

    let mut digest = SchemaFragmentDigest::default();
    digest.schema_by_term.insert(
        term.iri.clone(),
        serde_json::to_string_pretty(&serde_json::json!({
            "title": term.label.clone().unwrap_or_else(|| term.curie.clone()),
            "type": "object",
            "properties": {},
        }))
        .expect("serialize synthetic schema fragment"),
    );

    let mut model = base;
    model.attach_schema_fragments(digest);

    let slug = term_slug(&term);
    let md = to_markdown(&model, &Page::Term(slug));

    assert!(
        md.contains("## Example in multiple syntaxes"),
        "term page must render the multi-syntax example section"
    );

    let python = fenced_body(&md, "python");
    let rust = fenced_body(&md, "rust");

    // Falsifiable invariants beyond the snapshot: an empty/absent tab (a
    // regression to `None`) fails these even if the snapshot were blindly
    // re-accepted.
    assert!(
        python.contains("from gmeow_models."),
        "python tab must import the generated gmeow_models module: {python}"
    );
    assert!(
        python.contains(".model_validate("),
        "python tab must validate the worked instance against the Pydantic model: {python}"
    );
    assert!(
        !python.trim().is_empty(),
        "python tab body must not be empty"
    );

    assert!(
        rust.contains("purrdf::parse_turtle("),
        "rust tab must parse the same worked instance as Turtle: {rust}"
    );
    assert!(
        rust.contains("gmeow_validate::validate("),
        "rust tab must validate the parsed dataset with the native validator: {rust}"
    );
    assert!(!rust.trim().is_empty(), "rust tab body must not be empty");

    // Pin BOTH bodies verbatim: a drift in the generated snippet (module/class
    // naming, escaping, the worked-instance payload) trips this gate even when
    // the falsifiable substring checks above still pass.
    insta::assert_snapshot!("python_syntax_tab_body", python);
    insta::assert_snapshot!("rust_syntax_tab_body", rust);
}

/// The static per-term `card.md` (`crates/docs/src/render.rs::doc_term_card`) must
/// carry the SAME term→model link as the live `gmeow describe` / MCP `doc_card`
/// surface (`crates/docs/src/describe.rs::build_card`) and the folded-snapshot MCP
/// card (`crates/pipeline/src/stages/export.rs::term_to_card`) — one derivation
/// (`gmeow_docs::card::python_model_path` / `python_model_snippet`), never a third
/// copy. Before this test the static `card.md`/`card.json` builder never populated
/// `python_model`/`python_snippet` at all, so every shipped card file carried
/// `python_model: null` even for a modeled class (issue 1408 req 18).
///
/// Gated on the SAME `model.schema_fragments.schema_by_term` digest the Python/Rust
/// syntax tabs use (see `python_and_rust_syntax_tabs_render_for_a_modeled_class`
/// above) — a class only gets the link once its generated Pydantic model actually
/// exists.
#[test]
fn card_md_and_json_carry_the_python_model_link_for_a_modeled_class() {
    let base = common::cached_model();
    let term = class_term_with_properties(&base).clone();

    let mut digest = SchemaFragmentDigest::default();
    digest.schema_by_term.insert(
        term.iri.clone(),
        serde_json::to_string_pretty(&serde_json::json!({
            "title": term.label.clone().unwrap_or_else(|| term.curie.clone()),
            "type": "object",
            "properties": {},
        }))
        .expect("serialize synthetic schema fragment"),
    );

    let mut model = base;
    model.attach_schema_fragments(digest);

    // The expected dotted path + snippet, computed through the SAME shared
    // emitter routing the card builder must call — not re-derived by hand here.
    let expected_model = gmeow_docs::card::python_model_path(&term.owner_slice, &term.iri);
    let expected_snippet =
        gmeow_docs::card::python_model_snippet(&term.owner_slice, &term.iri, &term.curie);
    assert!(
        expected_model.starts_with("gmeow_models."),
        "sanity: the expected dotted path is a real gmeow_models.<slice>.<Class> path: {expected_model}"
    );

    // card.md — the human-oriented static card.
    let card_md = gmeow_docs::render::term_card_md(&model, &term);
    assert!(
        card_md.contains(&format!("**Python model:** `{expected_model}`")),
        "card.md must carry the Python model link for a modeled class:\n{card_md}"
    );
    assert!(
        card_md.contains(&expected_snippet),
        "card.md must carry the exact construct/validate snippet:\n{card_md}"
    );

    // card.json — the machine surface emitted alongside card.md; driven through
    // the SAME per-term builder `render_site_lang` uses to emit
    // `terms/{slug}/card.json` (byte-identical), without paying a full-site render.
    let json_bytes = gmeow_docs::render::term_card_json(&model, &term);
    let parsed: serde_json::Value =
        serde_json::from_slice(&json_bytes).expect("card.json parses as JSON");
    assert_eq!(
        parsed["python_model"], expected_model,
        "card.json python_model field must equal the shared emitter's dotted path"
    );
    assert_eq!(
        parsed["python_snippet"], expected_snippet,
        "card.json python_snippet field must equal the shared emitter's snippet"
    );
}
