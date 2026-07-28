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

use crate::render::{Site, slice_slug, term_slug};
use gmeow_docs_model::coverage::{CoverageContext, DIMENSIONS, SLICE_DIMENSIONS, term_coverage};
use gmeow_docs_model::model::DocsModel;

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
/// The per-term coverage predicates live in [`gmeow_docs_model::coverage`] — the single
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
                let mut finding = Finding::new(
                    Severity::Warning,
                    dim.lint_code,
                    format!(
                        "term `{}` misses documentation dimension `{}` (documentation coverage gap)",
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::render_site;
    use gmeow_docs_model::model::{
        DocCompetency, DocExample, DocFixture, DocFixtureKind, DocLinkage, DocLossTarget, DocTerm,
        DocTermCategory,
    };
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
            fixtures: Vec::new(),
            shapes: Vec::new(),
            competencies: Vec::new(),
            grammars: Vec::new(),
            loss_targets: Vec::new(),
            worked_instances: Vec::new(),
            concerns: Vec::new(),
            external_terms: Vec::new(),
            seams: Vec::new(),
            recipes: Vec::new(),
            learning_paths: Vec::new(),
            constraint_rules: Vec::new(),
            advice_entries: Vec::new(),
            four_boxes: None,
            concept_doi: None,
            pipeline: None,

            available_languages: vec!["english".to_string()],

            translations: gmeow_docs_model::i18n::Translations::default(),

            ui_catalog: gmeow_docs_model::i18n::UiCatalog::default(),
            reasoning: None,
            diagnostics: None,
            term_loss: None,
            schema_fragments: None,
            lang: String::new(),
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
        // A bare, superset-NATIVE term (no external-correspondence intent, not a
        // lossy-projection source) trips a `docs/missing-*` WARNING for every
        // UNCONDITIONAL per-term dimension it lacks, but NONE of the four
        // applicability-conditioned dimensions (alignment, linkage coverage, loss
        // ledger row, loss judgment sound) — those apply only where a term declares
        // an external correspondence or is a lossy projection. No errors.
        let model = populated(vec![term("Bare", None, None)]);
        let site = render_site(&model);
        let report = lint(&model, &site);
        assert_eq!(report.error_count(), 0);
        let codes: BTreeSet<&str> = report.findings.iter().map(|f| f.code.as_str()).collect();
        for code in [
            "docs/missing-definition",
            "docs/missing-label",
            "docs/missing-usage-advice",
            "docs/missing-example",
            "docs/missing-scope-note",
            "docs/missing-fixture-pair",
            "docs/missing-competency-rationale",
            "docs/missing-worked-instance",
            "docs/missing-annotation-coat",
            "docs/missing-test-reach",
            "docs/missing-prose-quality",
        ] {
            assert!(codes.contains(code), "expected `{code}`; got {codes:?}");
        }
        // The novel `Bare` term is NOT applicable for the external-correspondence /
        // lossy dimensions, so it contributes no such miss. `missing-alignment`,
        // `missing-loss-ledger-row`, and `missing-loss-judgment-sound` are absent from
        // the whole report (the aligned seed terms cover alignment; nothing is a lossy
        // source). The vacuously-covered dimensions (no non-English langs, no
        // rationale) are also absent.
        assert!(!codes.contains("docs/missing-alignment"));
        assert!(!codes.contains("docs/missing-loss-ledger-row"));
        assert!(!codes.contains("docs/missing-loss-judgment-sound"));
        assert!(!codes.contains("docs/missing-translation-coverage"));
        assert!(!codes.contains("docs/missing-provenance-honesty"));
        // `missing-linkage-coverage` DOES fire — but only for the seed terms, which
        // DECLARE an external correspondence (they are alignment subjects) yet carry
        // no mapping-set-backed linkage: applicable ∧ ¬present, a real defect the
        // applicability layer still catches.
        assert!(codes.contains("docs/missing-linkage-coverage"));
    }

    /// A term wired to cover ALL sixteen per-term dimensions: full annotation coat,
    /// a fixture pair, a competency question with a clean rationale, a worked
    /// example, a projection-loss target, and a mapping-set-backed alignment.
    fn fully_covered_term(local: &str, category: DocTermCategory, label: &str) -> DocTerm {
        DocTerm {
            scope_notes: vec![format!("Scope of {local}.")],
            // A boundary definition (states what it is NOT) — for dimProseQuality.
            examples: vec![format!("gmeow:{local} a owl:Class .")],
            use_when: vec![format!("Use {local} when modelling.")],
            avoid_when: vec![format!("Avoid {local} for raw strings.")],
            how_to_use: vec![format!("Attach {local} idiomatically.")],
            box_role: Some("gmeow:boxTBox".to_string()),
            ..cat(
                local,
                category,
                Some("A living thing, not a mineral."),
                Some(label),
            )
        }
    }

    #[test]
    fn fully_covered_terms_emit_no_coverage_warnings() {
        // Two fully-wired terms (a class + a property so both category indexes
        // render without dangling links) covering every per-term dimension → zero
        // coverage warnings and zero errors. No slices ⇒ no slice-scoped warnings.
        let mut model = model_with_terms(vec![
            fully_covered_term("Animal", DocTermCategory::Class, "Animal"),
            fully_covered_term("hasOwner", DocTermCategory::Property, "has owner"),
        ]);
        for local in ["Animal", "hasOwner"] {
            let iri = format!("{GMEOW}{local}");
            let curie = format!("gmeow:{local}");
            model.fixtures.push(DocFixture {
                slice: format!("{GMEOW}slice/zoo"),
                logical_path: format!("tests/conformance-fixtures/{local}-ok.ttl"),
                title: "ok".to_string(),
                text: String::new(),
                kind: DocFixtureKind::Wellformed,
                terms_referenced: vec![curie.clone()],
                expected_outcome: None,
                violation_code: None,
                rationale: None,
                catalog_slug: None,
            });
            model.fixtures.push(DocFixture {
                slice: format!("{GMEOW}slice/zoo"),
                logical_path: format!("tests/counter-examples/{local}-bad.ttl"),
                title: "bad".to_string(),
                text: String::new(),
                kind: DocFixtureKind::CounterExample,
                terms_referenced: vec![curie.clone()],
                expected_outcome: None,
                violation_code: None,
                rationale: None,
                catalog_slug: None,
            });
            model.examples.push(DocExample {
                slice: format!("{GMEOW}slice/zoo"),
                logical_path: format!("examples/{local}.ttl"),
                title: local.to_string(),
                text: String::new(),
                terms_referenced: vec![curie.clone()],
            });
            model.competencies.push(DocCompetency {
                iri: format!("{GMEOW}cq/{local}"),
                rationale: Some("Every animal is a living thing.".to_string()),
                exercises: vec![iri.clone()],
                owner_slice: format!("{GMEOW}slice/zoo"),
                ..Default::default()
            });
            model.loss_targets.push(DocLossTarget {
                target: local.to_string(),
                label: None,
                preservation_kind: "SoundUnderApproximation".to_string(),
                complexity_class: "PTIME".to_string(),
                slice: format!("{GMEOW}slice/zoo"),
            });
            model.linkages.push(DocLinkage {
                mapping_set: Some(format!("{GMEOW}mappingSet/1")),
                subject: iri.clone(),
                subject_curie: curie.clone(),
                predicate: "skos:closeMatch".to_string(),
                object: format!("http://example.org/{local}"),
                justification: None,
                confidence: None,
                owner_slice: format!("{GMEOW}slice/zoo"),
            });
        }
        let site = render_site(&model);
        let report = lint(&model, &site);
        assert_eq!(report.error_count(), 0, "{:?}", report.legacy_errors());
        let coverage: Vec<&str> = report
            .findings
            .iter()
            .map(|f| f.code.as_str())
            .filter(|c| c.starts_with("docs/missing-"))
            .collect();
        assert!(
            coverage.is_empty(),
            "fully-covered terms must emit no coverage warnings; got {coverage:?}"
        );
    }
}
