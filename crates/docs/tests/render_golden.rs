// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Golden + invariant tests for the static-site renderers.
//!
//! The goldens lock a *representative subset* (not the ~2k-term tree): the
//! landing page, one category index, one fully-populated term page (md + html),
//! the slice index, and one slice page (md + html). Representatives are chosen
//! by stable IRI/curie sort so the selection is deterministic. Two further tests
//! lock the cross-cutting invariants — byte-stability across two `render_site`
//! calls, and the absence of dangling internal `.html` links.

// Rich colored line-diffs on assert_eq! failure (#871); shadows the std macro
// for this file. Identical behaviour on pass; insta snapshots are unaffected.
use pretty_assertions::assert_eq;
use std::collections::BTreeSet;

use gmeow_docs::render::{
    concern_slug, llms_docs_txt, render_site, search_index_json, term_slug, to_html, to_markdown,
    Page,
};
use gmeow_docs::svg;
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

/// A deterministic, fully-populated term: the first by (curie, iri) sort that is
/// a Property carrying a definition, at least one parent, and a domain + range.
/// Among those, prefer one that ALSO carries usage advice and a per-term
/// alignment, so the golden exercises every term-page section (Usage Advice +
/// Alignments included). Falls back through advice-only, then any.
fn fully_populated_term_slug(model: &DocsModel) -> String {
    let mut candidates: Vec<&gmeow_docs::DocTerm> = model
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

    let has_advice = |t: &gmeow_docs::DocTerm| {
        !t.scope_notes.is_empty()
            || !t.examples.is_empty()
            || !t.use_when.is_empty()
            || !t.avoid_when.is_empty()
            || !t.how_to_use.is_empty()
            || !t.use_for_consumer.is_empty()
            || !t.avoid_for_consumer.is_empty()
    };
    let has_align = |t: &gmeow_docs::DocTerm| model.linkages.iter().any(|l| l.subject == t.iri);

    let term = candidates
        .iter()
        .find(|t| has_advice(t) && has_align(t))
        .or_else(|| candidates.iter().find(|t| has_advice(t)))
        .or_else(|| candidates.first())
        .copied()
        .expect("at least one fully-populated property term exists");
    term_slug(term)
}

#[test]
fn landing_markdown_golden() {
    let model = model();
    insta::assert_snapshot!(to_markdown(&model, &Page::Landing));
}

#[test]
fn classes_index_markdown_golden() {
    // The classes index is large; lock only its header region (the deterministic,
    // low-churn part) rather than every one of the hundreds of rows.
    let model = model();
    let md = to_markdown(&model, &Page::Category(DocTermCategory::Class));
    let header: String = md.lines().take(6).collect::<Vec<_>>().join("\n");
    insta::assert_snapshot!(header);
}

#[test]
fn fully_populated_term_markdown_golden() {
    let model = model();
    let slug = fully_populated_term_slug(&model);
    insta::assert_snapshot!(to_markdown(&model, &Page::Term(slug)));
}

#[test]
fn fully_populated_term_html_golden() {
    let model = model();
    let slug = fully_populated_term_slug(&model);
    insta::assert_snapshot!(to_html(&model, &Page::Term(slug)));
}

#[test]
fn slice_index_markdown_golden() {
    // Lock the header + first few rows; the row set is large and slice-owned.
    let model = model();
    let md = to_markdown(&model, &Page::SliceIndex);
    let head: String = md.lines().take(8).collect::<Vec<_>>().join("\n");
    insta::assert_snapshot!(head);
}

#[test]
fn first_slice_markdown_golden() {
    // The first slice by IRI (model.slices is IRI-sorted) — a deterministic rep.
    let model = model();
    let slug = gmeow_docs::render::slice_slug(&model.slices[0]);
    insta::assert_snapshot!(to_markdown(&model, &Page::Slice(slug)));
}

#[test]
fn first_slice_html_golden() {
    let model = model();
    let slug = gmeow_docs::render::slice_slug(&model.slices[0]);
    insta::assert_snapshot!(to_html(&model, &Page::Slice(slug)));
}

#[test]
fn linkage_index_markdown_golden() {
    // The linkage index is large (54 mapping sets); lock the header region plus
    // the first mapping set's heading block (the deterministic, low-churn part).
    let model = model();
    let md = to_markdown(&model, &Page::LinkageIndex);
    let head: String = md.lines().take(14).collect::<Vec<_>>().join("\n");
    insta::assert_snapshot!(head);
}

#[test]
fn first_concern_markdown_golden() {
    // The first concern by IRI (model.concerns is IRI-sorted) — a deterministic,
    // small page exercising definition + member terms + slices.
    let model = model();
    let slug = concern_slug(&model.concerns[0]);
    insta::assert_snapshot!(to_markdown(&model, &Page::Concern(slug)));
}

#[test]
fn slice_dependency_svg_golden() {
    // The SVG is large (a node per slice). Lock its structural head (the SVG
    // open tag, title, marker defs, and the first node) rather than every node —
    // determinism is asserted separately by `svg_is_pure`.
    let model = model();
    let svg_doc = svg::slice_dependency_svg(&model);
    let head: String = svg_doc.lines().take(12).collect::<Vec<_>>().join("\n");
    insta::assert_snapshot!(head);
}

#[test]
fn concern_overview_svg_golden() {
    // The concern overview is small (7 concerns); lock it in full.
    let model = model();
    insta::assert_snapshot!(svg::concern_overview_svg(&model));
}

#[test]
fn svg_is_pure() {
    let model = model();
    assert_eq!(
        svg::slice_dependency_svg(&model),
        svg::slice_dependency_svg(&model)
    );
    assert_eq!(
        svg::concern_overview_svg(&model),
        svg::concern_overview_svg(&model)
    );
}

#[test]
fn search_index_json_golden() {
    // Do NOT snapshot the whole ~2.4k-record index: lock its record count plus
    // the first and last records (URL-sorted) so the format + ordering are pinned.
    let model = model();
    let json = search_index_json(&model);
    let records: Vec<serde_json::Value> =
        serde_json::from_str(&json).expect("search index is valid JSON array");
    let summary = serde_json::json!({
        "record_count": records.len(),
        "first": records.first(),
        "last": records.last(),
    });
    insta::assert_json_snapshot!(summary);
}

#[test]
fn llms_docs_txt_golden() {
    // Lock the header (title/version/counts) plus one representative term line,
    // not the whole ~2k-line dump.
    let model = model();
    let txt = llms_docs_txt(&model);
    let header: String = txt.lines().take(4).collect::<Vec<_>>().join("\n");
    // A deterministic sample line: the first non-empty, non-comment line.
    let sample = txt
        .lines()
        .find(|l| !l.is_empty() && !l.starts_with('#'))
        .unwrap_or("")
        .to_string();
    insta::assert_snapshot!(format!("{header}\n---\n{sample}"));
}

#[test]
fn render_site_is_byte_stable() {
    let model = model();
    let a = render_site(&model);
    let b = render_site(&model);
    assert_eq!(a, b, "render_site must be byte-identical across calls");
    // The CSS asset and the landing pages are always present.
    assert!(a.files.contains_key("assets/gmeow.css"));
    assert!(a.files.contains_key("index.md"));
    assert!(a.files.contains_key("index.html"));
    // The T2 surfaces: diagrams, static indexes, and the new section pages.
    assert!(a.files.contains_key("diagrams/slices.svg"));
    assert!(a.files.contains_key("diagrams/concerns.svg"));
    assert!(a.files.contains_key("search-index.json"));
    assert!(a.files.contains_key("llms-docs.txt"));
    assert!(a.files.contains_key("linkages/index.html"));
    assert!(a.files.contains_key("examples/index.html"));
    assert!(a.files.contains_key("concerns/index.html"));
    assert!(a.files.contains_key("external-ontologies/index.html"));
    assert!(a.files.contains_key("integrity-constraints/index.html"));
    // The T3b guides surfaces: recipe/learning-path indexes + the four-boxes page.
    assert!(a.files.contains_key("recipes/index.html"));
    assert!(a.files.contains_key("learning-paths/index.html"));
    assert!(a.files.contains_key("four-boxes/index.html"));
}

#[test]
fn recipe_index_markdown_golden() {
    // The guides surface is small and curated; lock the recipe index in full.
    let model = model();
    insta::assert_snapshot!(to_markdown(&model, &Page::RecipeIndex));
}

#[test]
fn first_learning_path_markdown_golden() {
    // The first learning path by slug (model.learning_paths is slug-sorted) — a
    // deterministic representative exercising recipes + terms + adoption targets.
    let model = model();
    let slug = model.learning_paths[0].slug.clone();
    insta::assert_snapshot!(to_markdown(&model, &Page::LearningPath(slug)));
}

#[test]
fn no_dangling_internal_html_links() {
    // Every internal href in every emitted `.html` file must resolve to a key in
    // the site tree. Internal links are the relative `href="..."` attributes that
    // do NOT start with a scheme (`http`, `mailto`) — those are external.
    let model = model();
    let site = render_site(&model);
    let keys: BTreeSet<&String> = site.files.keys().collect();

    for (path, bytes) in &site.files {
        if !path.ends_with(".html") {
            continue;
        }
        let html = std::str::from_utf8(bytes).expect("html is utf-8");
        let dir = path.rsplit_once('/').map(|(d, _)| d).unwrap_or("");
        for href in extract_hrefs(html) {
            if href.is_empty()
                || href.contains("://")
                || href.starts_with("mailto:")
                || href.starts_with('#')
            {
                continue;
            }
            let resolved = resolve(dir, &href);
            assert!(
                keys.contains(&resolved),
                "dangling internal link in {path}: href={href:?} -> {resolved:?}"
            );
        }
    }
}

/// Pull every `href="..."` value out of an HTML string (attribute values are
/// always double-quoted by our shell + pulldown-cmark output).
fn extract_hrefs(html: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = html;
    while let Some(idx) = rest.find("href=\"") {
        rest = &rest[idx + 6..];
        if let Some(end) = rest.find('"') {
            out.push(rest[..end].to_string());
            rest = &rest[end + 1..];
        } else {
            break;
        }
    }
    out
}

/// Resolve a relative href against a site directory into a normalized site key.
fn resolve(dir: &str, href: &str) -> String {
    let mut parts: Vec<&str> = if dir.is_empty() {
        Vec::new()
    } else {
        dir.split('/').collect()
    };
    for seg in href.split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            other => parts.push(other),
        }
    }
    parts.join("/")
}
