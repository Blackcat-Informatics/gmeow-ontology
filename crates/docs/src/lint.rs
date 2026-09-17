// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The documentation lint gate (PyO3-free).
//!
//! [`lint`] checks the rendered [`Site`] and the typed [`DocsModel`] for
//! integrity defects and emits a [`gmeow_errors::Report`] (tool
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
//! - **WARNING `docs/missing-alignment`** — a vocabulary term that DECLARES an
//!   external correspondence (a non-empty `gmeow:adoptionTarget`, or it already
//!   participates in an alignment / mapping-set linkage) yet carries no term
//!   equivalence. GMEOW is a SUPERSET ontology, so this dimension (like
//!   `docs/missing-linkage-coverage`, `docs/missing-loss-ledger-row`, and
//!   `docs/missing-loss-judgment-sound`) is APPLICABILITY-CONDITIONED: a
//!   superset-native term that maps to nothing external is NOT applicable and does
//!   NOT warn — external linkage is an encouraged bonus, never a per-term
//!   obligation. All richness findings are report-only warnings (the gate stays
//!   green) — a ratchet whose baseline burns down as source prose and alignments
//!   land.

use std::collections::BTreeSet;

use gmeow_errors::{Finding, Location, Report, Severity};

use crate::coverage::{
    CoverageContext, DIMENSIONS, SLICE_DIMENSIONS, prose_quality_detail, term_coverage,
};
use crate::maturity::Dimension;
use crate::model::DocsModel;
use crate::render::{Site, slice_slug, term_slug};

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
///
/// The per-term coverage predicates live in [`crate::coverage`] — the single
/// source shared with the rendered docs site — so a `docs/missing-*` warning fires
/// exactly when the same dimension is shown absent on the term's page.
fn lint_coverage(model: &DocsModel, report: &mut Report) {
    let ctx = CoverageContext::new(model);

    // Per-term dimensions — one `docs/missing-<dim>` WARNING per absent dimension,
    // driven generically off the single coverage producer so the gate count and
    // the emitted `gmeow:docMissesDimension` incidence can never disagree. The
    // ratchet burns down as source prose, fixtures, alignments, and translations
    // land. model.terms is already IRI-sorted → findings come out deterministically.
    for term in &model.terms {
        let flags = term_coverage(term, &ctx).flags();
        let loc = Location::new(
            Some(format!("terms/{}/index.html", term_slug(term))),
            None,
            None,
            Some(term.curie.clone()),
        );
        for (dim, covered) in DIMENSIONS.iter().zip(flags) {
            if !covered {
                // `dimProseQuality` is a FOUR-way conjunction. Naming only the
                // dimension tells an author nothing about which of the four to fix,
                // so the per-conjunct detail rides the message. Every other dimension
                // is a single fact and needs no elaboration.
                let unmet = if dim.dimension == Dimension::ProseQuality {
                    format!(
                        " — unmet: {}",
                        prose_quality_detail(term, &ctx).unmet().join("; ")
                    )
                } else {
                    String::new()
                };
                let mut finding = Finding::new(
                    Severity::Warning,
                    dim.lint_code,
                    format!(
                        "term `{}` misses documentation dimension `{}` (documentation coverage gap){unmet}",
                        term.curie, dim.label
                    ),
                )
                .with_tool(TOOL);
                finding.add_location(loc.clone());
                report.add_finding(finding);
            }
        }
    }

    // Slice-scoped dimensions (thesis sentence, realized-state design-set table) —
    // one WARNING per slice that misses one, anchored at the slice page. model.slices
    // is IRI-sorted → deterministic. A missing realized-state marker becomes a scored,
    // gating defect rather than authorial vigilance.
    for slice in &model.slices {
        let loc = Location::new(
            Some(format!("slices/{}/index.html", slice_slug(slice))),
            None,
            None,
            None,
        );
        let slice_flags = [slice.realized_state_complete, slice.has_thesis_sentence];
        for (dim, present) in SLICE_DIMENSIONS.iter().zip(slice_flags) {
            if !present {
                let mut finding = Finding::new(
                    Severity::Warning,
                    dim.lint_code,
                    format!(
                        "slice `{}` misses documentation dimension `{}` (documentation coverage gap)",
                        slice.iri, dim.label
                    ),
                )
                .with_tool(TOOL);
                finding.add_location(loc.clone());
                report.add_finding(finding);
            }
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

#[path = "lint.tests.rs"]
#[cfg(test)]
mod tests;
