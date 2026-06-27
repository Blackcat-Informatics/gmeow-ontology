// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Golden + invariant tests for the static-site renderers.
//!
//! The goldens lock a *representative subset* (not the ~2k-term tree): the
//! landing page, one category index, one fully-populated term page (md + html),
//! the slice index, and one slice page (md + html). Representatives are chosen
//! by stable IRI/curie sort so the selection is deterministic. Two further tests
//! lock the cross-cutting invariants — byte-stability of a fresh `render_site`
//! against the cached once-per-run render, and the absence of dangling internal
//! `.html` links.

// Rich colored line-diffs on assert_eq! failure (#871); shadows the std macro
// for this file. Identical behaviour on pass; insta snapshots are unaffected.
use pretty_assertions::assert_eq;
use std::collections::BTreeSet;

use gmeow_docs::render::{
    concern_slug, render_site, search_index_json, term_slug, to_html, to_markdown, Page,
};
use gmeow_docs::svg;
use gmeow_docs::{DocTermCategory, DocsModel};

mod common;

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

/// A deterministic term that exercises the #1020 relational surfaces: the term
/// (by stable curie/iri sort) carrying the MOST of {logic stereotype, SHACL
/// constraint, related term, competency back-ref, example cross-link, box role}.
/// Locks a byte-golden that actually renders the new term-page sections.
fn richest_surface_term_slug(model: &DocsModel) -> String {
    let surface_count = |t: &gmeow_docs::DocTerm| -> usize {
        let has_constraint = model.shapes.iter().any(|s| s.target_term == t.iri);
        let has_competency = model
            .competencies
            .iter()
            .any(|c| c.exercises.iter().any(|e| e == &t.iri));
        let has_example = model
            .examples
            .iter()
            .any(|e| e.terms_referenced.iter().any(|c| c == &t.curie));
        usize::from(!t.logic_stereotypes.is_empty())
            + usize::from(!t.related_terms.is_empty())
            + usize::from(t.box_role.is_some())
            + usize::from(has_constraint)
            + usize::from(has_competency)
            + usize::from(has_example)
    };
    let mut terms: Vec<&gmeow_docs::DocTerm> = model.terms.iter().collect();
    terms.sort_by(|a, b| a.curie.cmp(&b.curie).then_with(|| a.iri.cmp(&b.iri)));
    let term = terms
        .iter()
        .max_by_key(|t| surface_count(t))
        .expect("model has terms");
    term_slug(term)
}

/// The fully-populated term as a `&DocTerm` — it carries parents + domain +
/// range, so it is guaranteed to have a neighbourhood to draw.
fn neighbourhood_term(model: &DocsModel) -> &gmeow_docs::DocTerm {
    let slug = fully_populated_term_slug(model);
    model
        .terms
        .iter()
        .find(|t| term_slug(t) == slug)
        .expect("the fully-populated term resolves")
}

#[test]
fn richest_surface_term_markdown_golden() {
    let model = common::cached_model();
    let slug = richest_surface_term_slug(&model);
    insta::assert_snapshot!(to_markdown(&model, &Page::Term(slug)));
}

/// A deterministic term that carries a per-term changelog (#1026): the first by
/// (curie, iri) sort with a non-empty `changelog`. Keyed off the EXPLICIT
/// `gmeow:hasChangelogEntry` data — not the richest-surface heuristic, which can
/// shift — so the suppressed-when-empty Changelog + Profiles blocks are always
/// exercised by a golden.
fn term_with_changelog_slug(model: &DocsModel) -> String {
    let mut candidates: Vec<&gmeow_docs::DocTerm> = model
        .terms
        .iter()
        .filter(|t| !t.changelog.is_empty())
        .collect();
    candidates.sort_by(|a, b| a.curie.cmp(&b.curie).then_with(|| a.iri.cmp(&b.iri)));
    let term = candidates
        .first()
        .expect("at least one term carries a changelog entry (#1026 seed data)");
    term_slug(term)
}

#[test]
fn term_with_changelog_markdown_golden() {
    // Exercises the #1026 lifecycle/citation blocks: an explicit stability badge,
    // an added-in version, a reified changelog entry, profile chips, and the
    // citation block (permalink + concept DOI).
    let model = common::cached_model();
    let slug = term_with_changelog_slug(&model);
    insta::assert_snapshot!(to_markdown(&model, &Page::Term(slug)));
}

#[test]
fn logic_index_markdown_golden() {
    // The logic index groups every stereotyped term; lock the header + the first
    // stereotype group block (the deterministic, low-churn part).
    let model = common::cached_model();
    let md = to_markdown(&model, &Page::Logic);
    let head: String = md.lines().take(10).collect::<Vec<_>>().join("\n");
    insta::assert_snapshot!(head);
}

#[test]
fn landing_markdown_golden() {
    let model = common::cached_model();
    insta::assert_snapshot!(to_markdown(&model, &Page::Landing));
}

#[test]
fn classes_index_markdown_golden() {
    // The classes index is large; lock only its header region (the deterministic,
    // low-churn part) rather than every one of the hundreds of rows.
    let model = common::cached_model();
    let md = to_markdown(&model, &Page::Category(DocTermCategory::Class));
    let header: String = md.lines().take(6).collect::<Vec<_>>().join("\n");
    insta::assert_snapshot!(header);
}

#[test]
fn fully_populated_term_markdown_golden() {
    let model = common::cached_model();
    let slug = fully_populated_term_slug(&model);
    insta::assert_snapshot!(to_markdown(&model, &Page::Term(slug)));
}

#[test]
fn fully_populated_term_html_golden() {
    let model = common::cached_model();
    let slug = fully_populated_term_slug(&model);
    insta::assert_snapshot!(to_html(&model, &Page::Term(slug)));
}

#[test]
fn slice_index_markdown_golden() {
    // Lock the header + first few rows; the row set is large and slice-owned.
    let model = common::cached_model();
    let md = to_markdown(&model, &Page::SliceIndex);
    let head: String = md.lines().take(8).collect::<Vec<_>>().join("\n");
    insta::assert_snapshot!(head);
}

#[test]
fn first_slice_markdown_golden() {
    // The first slice by IRI (model.slices is IRI-sorted) — a deterministic rep.
    let model = common::cached_model();
    let slug = gmeow_docs::render::slice_slug(&model.slices[0]);
    insta::assert_snapshot!(to_markdown(&model, &Page::Slice(slug)));
}

#[test]
fn first_slice_html_golden() {
    let model = common::cached_model();
    let slug = gmeow_docs::render::slice_slug(&model.slices[0]);
    insta::assert_snapshot!(to_html(&model, &Page::Slice(slug)));
}

#[test]
fn linkage_index_markdown_golden() {
    // The linkage index is large (54 mapping sets); lock the header region plus
    // the first mapping set's heading block (the deterministic, low-churn part).
    let model = common::cached_model();
    let md = to_markdown(&model, &Page::LinkageIndex);
    let head: String = md.lines().take(14).collect::<Vec<_>>().join("\n");
    insta::assert_snapshot!(head);
}

#[test]
fn first_concern_markdown_golden() {
    // The first concern by IRI (model.concerns is IRI-sorted) — a deterministic,
    // small page exercising definition + member terms + slices.
    let model = common::cached_model();
    let slug = concern_slug(&model.concerns[0]);
    insta::assert_snapshot!(to_markdown(&model, &Page::Concern(slug)));
}

#[test]
fn slice_dependency_svg_golden() {
    // The SVG is large (a node per slice). Lock its structural head (the SVG
    // open tag, title, marker defs, and the first node) rather than every node —
    // determinism is asserted separately by `svg_is_pure`.
    let model = common::cached_model();
    let svg_doc = svg::slice_dependency_svg(&model);
    let head: String = svg_doc.lines().take(12).collect::<Vec<_>>().join("\n");
    insta::assert_snapshot!(head);
}

#[test]
fn concern_overview_svg_golden() {
    // The concern overview is small (7 concerns); lock it in full.
    let model = common::cached_model();
    insta::assert_snapshot!(svg::concern_overview_svg(&model));
}

#[test]
fn term_neighbourhood_svg_golden() {
    // The per-term neighbourhood SVG is small; lock its structural head (open
    // tag, title, background, centre + first flank nodes). Determinism is
    // asserted separately by `svg_is_pure`.
    let model = common::cached_model();
    let term = neighbourhood_term(&model);
    assert!(svg::term_has_neighbourhood(term));
    let svg_doc = svg::term_neighbourhood_svg(term);
    let head: String = svg_doc.lines().take(12).collect::<Vec<_>>().join("\n");
    insta::assert_snapshot!(head);
}

#[test]
fn svg_is_pure() {
    let model = common::cached_model();
    assert_eq!(
        svg::slice_dependency_svg(&model),
        svg::slice_dependency_svg(&model)
    );
    assert_eq!(
        svg::concern_overview_svg(&model),
        svg::concern_overview_svg(&model)
    );
    let term = neighbourhood_term(&model);
    assert_eq!(
        svg::term_neighbourhood_svg(term),
        svg::term_neighbourhood_svg(term)
    );
}

#[test]
fn search_index_json_golden() {
    // Do NOT snapshot the whole ~2.4k-record index: lock its record count plus
    // the first and last records (URL-sorted) so the format + ordering are pinned.
    let model = common::cached_model();
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
fn llms_txt_header_golden() {
    // The standard llmstxt.org index is ~2k bullets; lock only its deterministic
    // head — H1 + canonical summary blockquote + prose + the Vocabulary section.
    let model = common::cached_model();
    let txt = gmeow_docs::render::llms_txt(&model);
    let head: String = txt.lines().take(16).collect::<Vec<_>>().join("\n");
    insta::assert_snapshot!(head);
}

#[test]
fn llms_full_txt_header_golden() {
    // Lock the complete form's header skeleton + the `## Terms` banner.
    let model = common::cached_model();
    let txt = gmeow_docs::render::llms_full_txt(&model);
    let head: String = txt.lines().take(8).collect::<Vec<_>>().join("\n");
    insta::assert_snapshot!(head);
}

#[test]
fn term_card_md_golden() {
    // The richest-surface term exercises every advisory field in the card.
    let model = common::cached_model();
    let slug = richest_surface_term_slug(&model);
    let term = model
        .terms
        .iter()
        .find(|t| term_slug(t) == slug)
        .expect("the richest-surface term resolves");
    insta::assert_snapshot!(gmeow_docs::render::term_card_md(&model, term));
}

#[test]
fn term_card_md_structural_gate() {
    // Hard-fail guards on card format invariants: H1 title, bold labels, and
    // the absence of the legacy italic-label convention (`*Label:*`).
    let model = common::cached_model();
    let slug = richest_surface_term_slug(&model);
    let term = model
        .terms
        .iter()
        .find(|t| term_slug(t) == slug)
        .expect("the richest-surface term resolves");
    let card = gmeow_docs::render::term_card_md(&model, term);

    // 1. The card must start with a `# ` H1 title line.
    assert!(
        card.starts_with("# "),
        "term card must start with a '# ' H1 title line; got: {:?}",
        card.lines().next().unwrap_or("")
    );

    // 2. The card must contain at least one `**` bold label (the canonical
    //    advisory-field convention).
    assert!(
        card.contains("**"),
        "term card must contain at least one '**' bold label (e.g. **Parents:**)"
    );

    // 3. The card must NOT use the legacy italic-label convention (`*Label:*`):
    //    single-asterisk italics directly after a newline.
    let has_italic_label = card.lines().any(|line| {
        // An italic label starts a line with `*` but NOT `**`.
        line.starts_with('*') && !line.starts_with("**")
    });
    assert!(
        !has_italic_label,
        "term card must use bold (**Label:**) not italic (*Label:*) labels"
    );
}

/// Extract the `url` of a `- [text](url): note` markdown-link bullet, if the line
/// is one (else `None`). URLs never contain `)`, so the first `)` closes them.
fn bullet_url(line: &str) -> Option<&str> {
    let after = line.strip_prefix("- [")?;
    let close = after.find("](")?;
    let rest = &after[close + 2..];
    let end = rest.find(')')?;
    Some(&rest[..end])
}

/// Shared conformance helper for both the linked index form (`llms.txt`) and the
/// complete inlined form (`llms-full.txt`).
///
/// Invariants checked unconditionally:
/// - Exactly one `# ` H1 line.
/// - At least one `> ` blockquote line.
/// - ≥`min_sections` non-empty `## ` section headings.
/// - Every `## ` section is followed by ≥1 bullet or `### ` sub-block before the
///   next `## ` or end of document (no empty sections).
///
/// When `require_links = true` (the published index surface):
/// - >100 `- [text](url)` linked bullets in total.
/// - Every such bullet URL resolves to a key in `site_files`.
fn assert_llmstxt_conformant(
    doc: &str,
    min_sections: usize,
    require_links: bool,
    site_files: Option<&std::collections::BTreeMap<String, Vec<u8>>>,
) {
    // ── H1 + blockquote ──────────────────────────────────────────────────────
    assert_eq!(
        doc.lines().filter(|l| l.starts_with("# ")).count(),
        1,
        "llmstxt doc must have exactly one H1"
    );
    assert!(
        doc.lines().any(|l| l.starts_with("> ")),
        "llmstxt doc must carry a summary blockquote"
    );

    // ── Section count + non-empty section guard ───────────────────────────────
    let mut sections = 0usize;
    let mut linked_bullets = 0usize;
    // Track whether the current section has seen at least one bullet or sub-block.
    let mut current_section_has_content = true; // true before first section (preamble is fine)
    let mut current_section_heading = String::new();

    for line in doc.lines() {
        if let Some(heading) = line.strip_prefix("## ") {
            // Close the previous section: if it had no content, fail.
            if sections > 0 {
                assert!(
                    current_section_has_content,
                    "section '## {current_section_heading}' must have ≥1 bullet or sub-block before the next section"
                );
            }
            sections += 1;
            assert!(
                !heading.trim().is_empty(),
                "section heading must not be empty"
            );
            current_section_heading = heading.trim().to_string();
            current_section_has_content = false;
        } else if line.starts_with("- ") || line.starts_with("### ") {
            current_section_has_content = true;
        }

        if let Some(url) = bullet_url(line) {
            linked_bullets += 1;
            if let Some(files) = site_files {
                assert!(
                    files.contains_key(url),
                    "llms.txt bullet URL must resolve to a site file: {url}"
                );
            }
        }
    }
    // Close the final section.
    if sections > 0 {
        assert!(
            current_section_has_content,
            "final section '## {current_section_heading}' must have ≥1 bullet or sub-block"
        );
    }

    assert!(
        sections >= min_sections,
        "expected at least {min_sections} sections, got {sections}"
    );

    if require_links {
        assert!(
            linked_bullets > 100,
            "expected the full term vocabulary linked, got {linked_bullets}"
        );
    }
}

#[test]
fn llms_txt_conforms_to_llmstxt_org() {
    // The load-bearing correctness gate (NOT insta): the structural llmstxt.org
    // invariants plus the guarantee that every bullet URL resolves to a real file
    // in the published site tree (the anchor-lint equivalent for the .txt surface,
    // which the HTML-only `no_dangling_internal_html_links` does not cover).
    let site = common::cached_site();
    let txt = std::str::from_utf8(&site.files["llms.txt"]).expect("llms.txt is utf-8");
    // The linked index has the full standard section set: Vocabulary, Classes,
    // Properties, Individuals, Slices, Concerns, Reference — at least 5.
    assert_llmstxt_conformant(txt, 5, true, Some(&site.files));
}

#[test]
fn llms_full_txt_conforms_structurally() {
    // Gate the complete inlined form (`llms-full.txt`) against the same
    // structural invariants as the linked index, minus the URL-resolution check
    // (the complete form is linkless). Also verify that the `## Terms` section
    // carries `### ` sub-blocks (one per term).
    let site = common::cached_site();
    let txt = std::str::from_utf8(&site.files["llms-full.txt"]).expect("llms-full.txt is utf-8");

    // The complete form has Terms + Concerns + Slices — at least 3 sections,
    // no link resolution needed.
    assert_llmstxt_conformant(txt, 3, false, None);

    // `## Terms` must be followed by `### ` sub-blocks (one per inlined term).
    let term_section_pos = txt
        .find("## Terms\n")
        .expect("llms-full.txt must contain a '## Terms' section");
    let after_terms = &txt[term_section_pos + "## Terms\n".len()..];
    assert!(
        after_terms.contains("### "),
        "the '## Terms' section must contain '### ' per-term sub-blocks"
    );
}

#[test]
fn render_site_is_byte_stable() {
    let model = common::cached_model();
    let a = render_site(&model);
    let b = common::cached_site();
    assert_eq!(
        a, b,
        "a fresh render_site must be byte-identical to the cached once-per-run render"
    );
    // The CSS asset and the landing pages are always present.
    assert!(a.files.contains_key("assets/gmeow.css"));
    assert!(a.files.contains_key("index.md"));
    assert!(a.files.contains_key("index.html"));
    // The T2 surfaces: diagrams, static indexes, and the new section pages.
    assert!(a.files.contains_key("diagrams/slices.svg"));
    assert!(a.files.contains_key("diagrams/concerns.svg"));
    assert!(a.files.contains_key("search-index.json"));
    // The #1027 standard llmstxt.org surfaces (superseded `llms-docs.txt`).
    assert!(a.files.contains_key("llms.txt"));
    assert!(a.files.contains_key("llms-full.txt"));
    // The #1027 per-term card surface: at least the richest-surface term's
    // card.md must be present in the site tree (terms/{slug}/card.md).
    let card_slug = richest_surface_term_slug(&model);
    let card_path = format!("terms/{card_slug}/card.md");
    assert!(
        a.files.contains_key(card_path.as_str()),
        "expected per-term card at {card_path}"
    );
    assert!(a.files.contains_key("linkages/index.html"));
    assert!(a.files.contains_key("examples/index.html"));
    assert!(a.files.contains_key("concerns/index.html"));
    assert!(a.files.contains_key("external-ontologies/index.html"));
    assert!(a.files.contains_key("integrity-constraints/index.html"));
    // The #1020 logic-stereotypes index (resolves the formerly-dangling nav_logic).
    assert!(a.files.contains_key("logic/index.html"));
    // The T3b guides surfaces: recipe/learning-path indexes + the four-boxes page.
    assert!(a.files.contains_key("recipes/index.html"));
    assert!(a.files.contains_key("learning-paths/index.html"));
    assert!(a.files.contains_key("four-boxes/index.html"));
}

#[test]
fn recipe_index_markdown_golden() {
    // The guides surface is small and curated; lock the recipe index in full.
    let model = common::cached_model();
    insta::assert_snapshot!(to_markdown(&model, &Page::RecipeIndex));
}

#[test]
fn first_learning_path_markdown_golden() {
    // The first learning path by slug (model.learning_paths is slug-sorted) — a
    // deterministic representative exercising recipes + terms + adoption targets.
    let model = common::cached_model();
    let slug = model.learning_paths[0].slug.clone();
    insta::assert_snapshot!(to_markdown(&model, &Page::LearningPath(slug)));
}

#[test]
fn no_dangling_internal_html_links() {
    // Every internal href in every emitted `.html` file must resolve to a key in
    // the site tree. Internal links are the relative `href="..."` attributes that
    // do NOT start with a scheme (`http`, `mailto`) — those are external.
    let site = common::cached_site();
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
