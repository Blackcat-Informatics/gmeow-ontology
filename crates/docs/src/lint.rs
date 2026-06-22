// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The documentation lint gate (PyO3-free).
//!
//! [`lint`] checks the rendered [`Site`] and the typed [`DocsModel`] for
//! integrity defects and emits a [`gmeow_diagnostics::Report`] (tool
//! `"gmeow-docs"`), which `make check`'s `doc-lint` step turns into
//! `gmeow:Finding`s. Findings are deterministic: every collection is sorted
//! before iteration and the report is normalized by its consumer.
//!
//! Checks:
//! - **ERROR `docs/dangling-link`** — an internal `.html`/`.md` href in an
//!   emitted HTML page that does not resolve to a `Site.files` key. A dangling
//!   link is always a render bug, so it MUST be zero on the current docs (the
//!   gate stays green); if one ever appears, fix the renderer, not the lint.
//! - **ERROR `docs/broken-anchor`** — an in-page `#fragment` link whose target
//!   `id="…"`/`name="…"` is absent from the same page.
//! - **WARNING `docs/missing-definition`** — a vocabulary term with an empty
//!   `skos:definition`/`rdfs:comment` (coverage gap; warning so the gate stays
//!   green on the current docs).
//! - **WARNING `docs/missing-label`** — a vocabulary term with no `rdfs:label`
//!   (annotation-contract triad, VOCABULARY SURFACE ONLY — example individuals
//!   are never linted; warning for now so the gate stays green).

use std::collections::BTreeSet;

use gmeow_diagnostics::{Finding, Location, Report, Severity};

use crate::model::DocsModel;
use crate::render::{term_slug, Site};

/// The diagnostics tool name for documentation findings.
const TOOL: &str = "gmeow-docs";

/// Run the documentation lint over the model + rendered site.
///
/// Returns a `gmeow-docs` [`Report`]; the caller decides exit policy
/// (`error_count > 0` ⇒ failure). On the current docs this MUST be zero errors.
pub fn lint(model: &DocsModel, site: &Site) -> Report {
    let mut report = Report::new(TOOL);

    lint_links(site, &mut report);
    lint_coverage(model, &mut report);

    report
}

/// ERROR `docs/dangling-link` + ERROR `docs/broken-anchor`: every internal href
/// in every emitted HTML page must resolve to a site key (for `.html`/`.md`
/// links) or to an `id`/`name` on the same page (for `#fragment` links).
fn lint_links(site: &Site, report: &mut Report) {
    let keys: BTreeSet<&String> = site.files.keys().collect();

    // Iterate the BTreeMap in its sorted key order for deterministic findings.
    for (path, bytes) in &site.files {
        if !path.ends_with(".html") {
            continue;
        }
        let Ok(html) = std::str::from_utf8(bytes) else {
            continue;
        };
        let dir = path.rsplit_once('/').map(|(d, _)| d).unwrap_or("");
        let ids = collect_ids(html);

        for href in extract_hrefs(html) {
            // Skip empties and external links (absolute schemes, mailto).
            if href.is_empty() || href.contains("://") || href.starts_with("mailto:") {
                continue;
            }
            if let Some(fragment) = href.strip_prefix('#') {
                // An in-page anchor: its target id must exist on this page.
                if !fragment.is_empty() && !ids.contains(fragment) {
                    report.add_finding(broken_anchor(path, &href));
                }
                continue;
            }
            // Only intra-site page links are resolvable against site keys; assets
            // (.svg/.css/.json/.txt) and `#frag` already handled. Resolve the
            // relative href against the page's directory.
            let target = strip_fragment(&href);
            let resolved = resolve(dir, target);
            if !keys.contains(&resolved) {
                report.add_finding(dangling_link(path, &href));
            }
        }
    }
}

/// WARNING coverage findings over the vocabulary surface only.
fn lint_coverage(model: &DocsModel, report: &mut Report) {
    // model.terms is already IRI-sorted → findings come out deterministically.
    for term in &model.terms {
        let loc = Location::new(
            Some(format!("terms/{}/index.html", term_slug(term))),
            None,
            None,
            Some(term.curie.clone()),
        );
        if term.definition.as_deref().unwrap_or("").trim().is_empty() {
            let mut finding = Finding::new(
                Severity::Warning,
                "docs/missing-definition",
                format!(
                    "term `{}` has no skos:definition/rdfs:comment (documentation coverage gap)",
                    term.curie
                ),
            )
            .with_tool(TOOL);
            finding.add_location(loc.clone());
            report.add_finding(finding);
        }
        if term.label.as_deref().unwrap_or("").trim().is_empty() {
            let mut finding = Finding::new(
                Severity::Warning,
                "docs/missing-label",
                format!(
                    "term `{}` has no rdfs:label (annotation-contract triad incomplete)",
                    term.curie
                ),
            )
            .with_tool(TOOL);
            finding.add_location(loc);
            report.add_finding(finding);
        }
    }
}

fn dangling_link(page: &str, href: &str) -> Finding {
    let mut finding = Finding::new(
        Severity::Error,
        "docs/dangling-link",
        format!("internal link `{href}` does not resolve to any documentation page"),
    )
    .with_tool(TOOL);
    finding.add_location(Location::new(Some(page.to_string()), None, None, None));
    finding
}

fn broken_anchor(page: &str, href: &str) -> Finding {
    let mut finding = Finding::new(
        Severity::Error,
        "docs/broken-anchor",
        format!("in-page anchor `{href}` has no matching id on the page"),
    )
    .with_tool(TOOL);
    finding.add_location(Location::new(Some(page.to_string()), None, None, None));
    finding
}

// ── HTML parsing helpers (mirror tests/render_golden.rs) ───────────────────────

/// Pull every `href="..."` value out of an HTML string. Our shell + pulldown-
/// cmark always double-quote attribute values.
fn extract_hrefs(html: &str) -> Vec<String> {
    extract_attr(html, "href=\"")
}

/// Collect every `id="…"`/`name="…"` value on a page, for anchor resolution.
fn collect_ids(html: &str) -> BTreeSet<String> {
    let mut ids: BTreeSet<String> = BTreeSet::new();
    for value in extract_attr(html, "id=\"") {
        ids.insert(value);
    }
    for value in extract_attr(html, "name=\"") {
        ids.insert(value);
    }
    ids
}

/// Pull every double-quoted attribute value following `marker` (e.g. `href="`).
fn extract_attr(html: &str, marker: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = html;
    while let Some(idx) = rest.find(marker) {
        rest = &rest[idx + marker.len()..];
        if let Some(end) = rest.find('"') {
            out.push(rest[..end].to_string());
            rest = &rest[end + 1..];
        } else {
            break;
        }
    }
    out
}

/// Drop a trailing `#fragment` from an href, leaving the page path.
fn strip_fragment(href: &str) -> &str {
    href.split_once('#').map(|(p, _)| p).unwrap_or(href)
}

/// Resolve a relative href (from a page in directory `dir`) into a site-relative
/// key, collapsing `..`/`.` segments. Mirrors `tests/render_golden.rs::resolve`.
fn resolve(dir: &str, href: &str) -> String {
    let mut parts: Vec<&str> = if dir.is_empty() {
        Vec::new()
    } else {
        dir.split('/').collect()
    };
    for segment in href.split('/') {
        match segment {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            other => parts.push(other),
        }
    }
    parts.join("/")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{DocTerm, DocTermCategory};
    use crate::render::render_site;
    use std::collections::BTreeMap;

    const GMEOW: &str = "https://blackcatinformatics.ca/gmeow/";

    fn model_with_terms(terms: Vec<DocTerm>) -> DocsModel {
        DocsModel {
            title: "T".to_string(),
            version: "2".to_string(),
            slices: Vec::new(),
            terms,
            dependency_edges: Vec::new(),
            mapping_sets: Vec::new(),
            linkages: Vec::new(),
            examples: Vec::new(),
            concerns: Vec::new(),
            external_terms: Vec::new(),
            recipes: Vec::new(),
            learning_paths: Vec::new(),
            four_boxes: None,

            available_languages: vec!["english".to_string()],

            translations: crate::i18n::Translations::default(),

            ui_catalog: crate::i18n::UiCatalog::default(),
        }
    }

    fn cat(
        local: &str,
        category: DocTermCategory,
        definition: Option<&str>,
        label: Option<&str>,
    ) -> DocTerm {
        DocTerm {
            iri: format!("{GMEOW}{local}"),
            curie: format!("gmeow:{local}"),
            label: label.map(str::to_string),
            definition: definition.map(str::to_string),
            category,
            owner_slice: format!("{GMEOW}slice/zoo"),
            parents: Vec::new(),
            domain: Vec::new(),
            range: Vec::new(),
        }
    }

    fn term(local: &str, definition: Option<&str>, label: Option<&str>) -> DocTerm {
        cat(local, DocTermCategory::Class, definition, label)
    }

    /// The static nav + getting-started page always link to both the classes and
    /// properties category indexes, which only render when their category is
    /// non-empty; a fully-populated ontology has both, so test models do too.
    fn populated(extra: Vec<DocTerm>) -> DocsModel {
        let mut terms = vec![
            cat(
                "Animal",
                DocTermCategory::Class,
                Some("An animal."),
                Some("Animal"),
            ),
            cat(
                "hasOwner",
                DocTermCategory::Property,
                Some("Ownership."),
                Some("has owner"),
            ),
        ];
        terms.extend(extra);
        model_with_terms(terms)
    }

    #[test]
    fn clean_site_has_zero_errors() {
        let model = populated(vec![term("Cat", Some("A cat."), Some("Cat"))]);
        let site = render_site(&model);
        let report = lint(&model, &site);
        assert_eq!(
            report.error_count(),
            0,
            "a rendered site must have no dangling links: {:?}",
            report.legacy_errors()
        );
    }

    #[test]
    fn dangling_link_is_an_error() {
        // A site with a single page that links to a non-existent page.
        let mut files: BTreeMap<String, Vec<u8>> = BTreeMap::new();
        files.insert(
            "index.html".to_string(),
            br#"<a href="missing/index.html">x</a>"#.to_vec(),
        );
        let site = Site { files };
        let model = model_with_terms(Vec::new());
        let report = lint(&model, &site);
        assert_eq!(report.error_count(), 1);
        assert!(report.legacy_errors()[0].contains("missing/index.html"));
    }

    #[test]
    fn broken_anchor_is_an_error() {
        let mut files: BTreeMap<String, Vec<u8>> = BTreeMap::new();
        files.insert(
            "index.html".to_string(),
            br##"<a href="#nope">x</a><h2 id="real">y</h2>"##.to_vec(),
        );
        let site = Site { files };
        let model = model_with_terms(Vec::new());
        let report = lint(&model, &site);
        assert_eq!(report.error_count(), 1);
        assert!(report.legacy_errors()[0].contains("#nope"));
    }

    #[test]
    fn missing_definition_and_label_are_warnings() {
        // `populated` seeds two fully-annotated terms; the bare term adds exactly
        // one missing-definition + one missing-label warning, no errors.
        let model = populated(vec![term("Bare", None, None)]);
        let site = render_site(&model);
        let report = lint(&model, &site);
        assert_eq!(report.error_count(), 0);
        assert_eq!(report.warning_count(), 2);
    }
}
