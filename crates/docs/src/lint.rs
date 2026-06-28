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
//! - **WARNING `docs/missing-usage-advice`** — a vocabulary term carrying no
//!   usage advice at all: empty `gmeow:useWhen` AND `gmeow:avoidWhen` AND
//!   `gmeow:howToUse` (the consumer-routing fields `gmeow:useForConsumer` /
//!   `gmeow:avoidForConsumer` are a separate surface and do NOT count as advice).
//! - **WARNING `docs/missing-example`** — a vocabulary term with no `skos:example`
//!   worked-usage prose.
//! - **WARNING `docs/missing-scope-note`** — a vocabulary term with no
//!   `skos:scopeNote` usage-advice prose.
//! - **WARNING `docs/missing-alignment`** — a vocabulary term whose IRI is not the
//!   subject of any term equivalence (no external crosswalk; a super-ontology
//!   coverage opportunity). All four richness findings are report-only warnings
//!   (the gate stays green) — a ratchet whose baseline burns down as source prose
//!   and alignments land.

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
    // A term whose IRI appears as an alignment subject has at least one external
    // crosswalk; the rest trip docs/missing-alignment. Build the lookup once so
    // the per-term test is O(log n), not a full linkages scan.
    let aligned: BTreeSet<&str> = model.linkages.iter().map(|l| l.subject.as_str()).collect();

    // model.terms is already IRI-sorted → findings come out deterministically.
    for term in &model.terms {
        let loc = Location::new(
            Some(format!("terms/{}/index.html", term_slug(term))),
            None,
            None,
            Some(term.curie.clone()),
        );
        // Emit a report-only coverage WARNING anchored at this term.
        let mut emit = |code: &'static str, message: String| {
            let mut finding = Finding::new(Severity::Warning, code, message).with_tool(TOOL);
            finding.add_location(loc.clone());
            report.add_finding(finding);
        };

        if term.definition.as_deref().unwrap_or("").trim().is_empty() {
            emit(
                "docs/missing-definition",
                format!(
                    "term `{}` has no skos:definition/rdfs:comment (documentation coverage gap)",
                    term.curie
                ),
            );
        }
        if term.label.as_deref().unwrap_or("").trim().is_empty() {
            emit(
                "docs/missing-label",
                format!(
                    "term `{}` has no rdfs:label (annotation-contract triad incomplete)",
                    term.curie
                ),
            );
        }
        // Usage advice = the useWhen/avoidWhen/howToUse triad ONLY; the consumer-
        // routing fields are a separate surface and are not counted here.
        if term.use_when.is_empty() && term.avoid_when.is_empty() && term.how_to_use.is_empty() {
            emit(
                "docs/missing-usage-advice",
                format!(
                    "term `{}` has no usage advice (gmeow:useWhen/avoidWhen/howToUse) (documentation coverage gap)",
                    term.curie
                ),
            );
        }
        if term.examples.is_empty() {
            emit(
                "docs/missing-example",
                format!(
                    "term `{}` has no skos:example (documentation coverage gap)",
                    term.curie
                ),
            );
        }
        if term.scope_notes.is_empty() {
            emit(
                "docs/missing-scope-note",
                format!(
                    "term `{}` has no skos:scopeNote (documentation coverage gap)",
                    term.curie
                ),
            );
        }
        if !aligned.contains(term.iri.as_str()) {
            emit(
                "docs/missing-alignment",
                format!(
                    "term `{}` has no external alignment (term equivalence) — super-ontology coverage opportunity",
                    term.curie
                ),
            );
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
    use crate::model::{DocLinkage, DocTerm, DocTermCategory};
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
            shapes: Vec::new(),
            competencies: Vec::new(),
            concerns: Vec::new(),
            external_terms: Vec::new(),
            recipes: Vec::new(),
            learning_paths: Vec::new(),
            four_boxes: None,
            concept_doi: None,

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

    fn term(local: &str, definition: Option<&str>, label: Option<&str>) -> DocTerm {
        cat(local, DocTermCategory::Class, definition, label)
    }

    /// A term with NO coverage gaps: definition, label, scope note, example, the
    /// full usage-advice triad, and (via `populated`) a matching alignment.
    fn rich(local: &str, category: DocTermCategory, label: &str) -> DocTerm {
        DocTerm {
            scope_notes: vec![format!("Scope of {local}.")],
            examples: vec![format!("Worked use of {local}.")],
            use_when: vec![format!("Use {local} when …")],
            avoid_when: vec![format!("Avoid {local} when …")],
            how_to_use: vec![format!("Idiomatic {local} use.")],
            ..cat(local, category, Some("Fully covered."), Some(label))
        }
    }

    /// An external term equivalence whose subject is the GMEOW term `local`, so it
    /// satisfies the `docs/missing-alignment` check.
    fn linkage(local: &str) -> DocLinkage {
        DocLinkage {
            mapping_set: None,
            subject: format!("{GMEOW}{local}"),
            subject_curie: format!("gmeow:{local}"),
            predicate: "skos:closeMatch".to_string(),
            object: format!("http://example.org/{local}"),
            justification: None,
            confidence: None,
            owner_slice: format!("{GMEOW}slice/zoo"),
        }
    }

    /// The static nav + getting-started page always link to both the classes and
    /// properties category indexes, which only render when their category is
    /// non-empty; a fully-populated ontology has both, so test models do too. Both
    /// seed terms are fully covered (and aligned) so only `extra` terms can warn.
    fn populated(extra: Vec<DocTerm>) -> DocsModel {
        let mut terms = vec![
            rich("Animal", DocTermCategory::Class, "Animal"),
            rich("hasOwner", DocTermCategory::Property, "has owner"),
        ];
        terms.extend(extra);
        let mut model = model_with_terms(terms);
        model.linkages = vec![linkage("Animal"), linkage("hasOwner")];
        model
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
    fn coverage_gaps_are_warnings() {
        // `populated` seeds two fully-covered terms (definition, label, scope note,
        // example, usage-advice triad, alignment); the bare term adds EXACTLY one
        // of each of the six coverage warnings — and no errors.
        let model = populated(vec![term("Bare", None, None)]);
        let site = render_site(&model);
        let report = lint(&model, &site);
        assert_eq!(report.error_count(), 0);
        assert_eq!(report.warning_count(), 6, "{:?}", report.legacy_errors());
        let codes: BTreeSet<&str> = report.findings.iter().map(|f| f.code.as_str()).collect();
        for code in [
            "docs/missing-definition",
            "docs/missing-label",
            "docs/missing-usage-advice",
            "docs/missing-example",
            "docs/missing-scope-note",
            "docs/missing-alignment",
        ] {
            assert!(codes.contains(code), "expected `{code}`; got {codes:?}");
        }
    }

    #[test]
    fn fully_covered_terms_emit_no_coverage_warnings() {
        // The two `rich` + aligned seed terms carry every annotation, so a model of
        // only those must produce zero warnings (and zero errors).
        let model = populated(Vec::new());
        let site = render_site(&model);
        let report = lint(&model, &site);
        assert_eq!(report.error_count(), 0);
        assert_eq!(report.warning_count(), 0, "{:?}", report.legacy_errors());
    }
}
