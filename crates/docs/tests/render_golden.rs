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

// Rich colored line-diffs on assert_eq! failure; shadows the std macro
// for this file. Identical behaviour on pass; insta snapshots are unaffected.
use pretty_assertions::assert_eq;
use std::collections::BTreeSet;

use gmeow_docs::render::{Page, concern_slug, search_index_json, term_slug, to_html, to_markdown};
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

/// A deterministic term that exercises the relational surfaces: the term
/// (by stable curie/iri sort) carrying the MOST of {logic stereotype, SHACL
/// constraint, related term, competency back-ref, example cross-link, box role,
/// conformance fixture back-ref, pipeline-stage identity}. Locks a byte-golden
/// that actually renders the new term-page sections — the conformance Do/Don't
/// pairs and the enriched pipeline-stage surface (issue 1404) join here so the
/// richest term reaches those renderers too, not just the pre-1404 sections.
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
        // The conformance Do/Don't fixtures reference a term by CURIE (issue 1404);
        // a term with fixtures reaches the "Conformance examples" renderer.
        let has_fixture = model
            .fixtures
            .iter()
            .any(|f| f.terms_referenced.iter().any(|c| c == &t.curie));
        // A `gmeow:PipelineStage` term reaches the enriched stage-page surface
        // (consumes/consumed-by tables, flowing graphs); its IRI is a stage IRI.
        let is_pipeline_stage = model
            .pipeline
            .as_ref()
            .is_some_and(|p| p.stages.iter().any(|s| s.iri == t.iri));
        usize::from(!t.logic_stereotypes.is_empty())
            + usize::from(!t.related_terms.is_empty())
            + usize::from(t.box_role.is_some())
            + usize::from(has_constraint)
            + usize::from(has_competency)
            + usize::from(has_example)
            + usize::from(has_fixture)
            + usize::from(is_pipeline_stage)
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

/// The fixed IRI of a REAL `gmeow:PipelineStage` — the narrow-waist serialization
/// exit (`gmeow:stage-gts-sink`). Pinned by IRI, not the shifting richest-surface
/// heuristic, so the enriched pipeline-stage term page is drift-gated by a stable
/// subject: it carries the sink capability, consumes an upstream producer, and is
/// consumed by nothing — a deterministic exercise of every stage sub-section.
const STAGE_GTS_SINK_IRI: &str = "https://blackcatinformatics.ca/gmeow/stage-gts-sink";

/// Resolve the term slug for a stage IRI, hard-failing if the stage individual was
/// never lifted into a documented term (the exact regression this golden guards:
/// a `gmeow:PipelineStage` must be a `DocTerm` for its term page — and thus the
/// enriched stage section — to exist at all).
fn stage_term_slug(model: &DocsModel, stage_iri: &str) -> String {
    let term = model
        .terms
        .iter()
        .find(|t| t.iri == stage_iri)
        .unwrap_or_else(|| {
            panic!("pipeline stage `{stage_iri}` must be a documented term (has a term page)")
        });
    term_slug(term)
}

#[test]
fn pipeline_stage_term_markdown_golden() {
    // A fixed-subject golden over `gmeow:stage-gts-sink`: locks the enriched
    // "stage of the build pipeline" section — the `stageImpl`→Rust binding, the
    // consumes / consumed-by dataflow tables, the flowing named graphs, and the
    // capabilities/resources — so this surface (issue 1404) cannot silently
    // regress to the pre-wiring DARK state where no stage was a term page.
    let model = common::cached_model();
    let slug = stage_term_slug(&model, STAGE_GTS_SINK_IRI);
    insta::assert_snapshot!(to_markdown(&model, &Page::Term(slug)));
}

/// A REAL producer stage that has BOTH downstream consumers (a non-empty
/// consumed-by reverse-edge table) AND a reified `gmeow:BuildDataFlow` carrying
/// `gmeow:flowEntity` named graphs (a non-empty flowing-graphs table):
/// `gmeow:stage-compile-logic` is the `logic:flowFrom` of the
/// compile-logic → reason edge whose three flowing graphs are authored.
const STAGE_COMPILE_LOGIC_IRI: &str = "https://blackcatinformatics.ca/gmeow/stage-compile-logic";

#[test]
fn producer_stage_renders_reverse_edges_and_flowing_graphs() {
    // The sink golden (`pipeline_stage_term_markdown_golden`) is the terminal
    // stage, so its consumed-by and flowing-graphs tables are correctly ABSENT.
    // This gate exercises the OTHER two sub-sections on a producer stage: the
    // "Consumed by" reverse-edge table (built from `pipeline.edges` where
    // `from == this stage`) and the "Flowing graphs" table (the reified
    // `gmeow:flowEntity` named graphs), proving both render live on the
    // production surface.
    let model = common::cached_model();
    let slug = stage_term_slug(&model, STAGE_COMPILE_LOGIC_IRI);
    let page = to_markdown(&model, &Page::Term(slug));

    assert!(
        page.contains("## Build\\-pipeline stage"),
        "producer stage page must render the enriched stage section"
    );
    assert!(
        page.contains("### Consumed by"),
        "producer stage page must render the consumed-by reverse-edge table"
    );
    assert!(
        page.contains("### Flowing graphs"),
        "producer stage page must render the flowing-graphs table"
    );
    // The three authored flowing named graphs of the compile-logic → reason edge.
    for graph in [
        "https://blackcatinformatics.ca/gmeow/graph/correspondence",
        "https://blackcatinformatics.ca/gmeow/graph/logic",
        "https://blackcatinformatics.ca/gmeow/graph/relational-core",
    ] {
        assert!(
            page.contains(graph),
            "flowing-graphs table must list the authored named graph `{graph}`"
        );
    }
}

#[test]
fn every_pipeline_stage_is_a_documented_term() {
    // Structural gate: every `gmeow:PipelineStage` individual in the build DAG must
    // be lifted into a documented term, so every stage's term page renders the
    // enriched stage section. Guards the `category_for_type` wiring against a
    // future edit that drops the `GMEOW_PIPELINE_STAGE` arm.
    let model = common::cached_model();
    let pipeline = model
        .pipeline
        .as_ref()
        .expect("the whole-repo model carries the build pipeline");
    let term_iris: BTreeSet<&str> = model.terms.iter().map(|t| t.iri.as_str()).collect();
    let missing: Vec<&str> = pipeline
        .stages
        .iter()
        .map(|s| s.iri.as_str())
        .filter(|iri| !term_iris.contains(iri))
        .collect();
    assert!(
        missing.is_empty(),
        "every pipeline stage must be a documented term; missing: {missing:?}"
    );
}

/// The AUTHORED-changelog term this golden locks, pinned by CURIE: it carries an
/// explicit `gmeow:hasChangelogEntry` record in `slices/core/versions/module.ttl`.
const CHANGELOG_GOLDEN_CURIE: &str = "gmeow:ChangelogEntry";

/// The `gmeow:entryVersion` of that term's AUTHORED changelog record. Distinct from
/// the ontology's `owl:versionInfo` release (which is what a manifest-COMPUTED entry
/// would carry), so its presence witnesses that the authored record — not build
/// history — is what fills the golden's Changelog block.
const CHANGELOG_GOLDEN_AUTHORED_VERSION: &str = "1.0.2";

/// The term whose page locks the per-term changelog rendering: [`CHANGELOG_GOLDEN_CURIE`],
/// so the suppressed-when-empty Changelog + Profiles blocks are always exercised by a
/// golden.
///
/// The subject is PINNED BY NAME rather than picked as "the first by (curie, iri) sort
/// with a non-empty `changelog`", because that sort was not a function of the repo's
/// sources. [`gmeow_docs::DocTerm::changelog`] is the UNION of the AUTHORED
/// `gmeow:hasChangelogEntry` records and the entries `stage-term-manifest` COMPUTES
/// from a definition-digest divergence against the PRIOR
/// `generated/catalog/term-content-manifest.nq`. `generated/` is git-ignored, so that
/// computed set tracks the tree's build HISTORY: a bootstrap tree (fresh clone, CI, a
/// just-materialized worktree) has no prior manifest and computes nothing, leaving only
/// the authored entries, while a tree that has synced before computes a
/// "Definition changed" entry for every term whose definition moved since. Sorting over
/// the union therefore let a term with no authored changelog at all win the selection on
/// one machine and not another, and the golden alternated between subjects on identical
/// sources. Naming the subject removes that dependency outright — the strongest
/// determinism available here, and it costs no extra model build.
fn term_with_changelog_slug(model: &DocsModel) -> String {
    let term = model
        .terms
        .iter()
        .find(|t| t.curie == CHANGELOG_GOLDEN_CURIE)
        .unwrap_or_else(|| {
            panic!("the pinned changelog golden subject `{CHANGELOG_GOLDEN_CURIE}` is documented")
        });
    // The pin must keep EXERCISING the block it exists for: the authored record has to
    // still be there, in either the bootstrap or the synced condition.
    assert!(
        term.changelog
            .iter()
            .any(|e| e.version == CHANGELOG_GOLDEN_AUTHORED_VERSION),
        "`{CHANGELOG_GOLDEN_CURIE}` must still carry its authored \
         gmeow:hasChangelogEntry record for version {CHANGELOG_GOLDEN_AUTHORED_VERSION} \
         — the golden's Changelog block is keyed off it"
    );
    term_slug(term)
}

#[test]
fn term_with_changelog_markdown_golden() {
    // Exercises the lifecycle/citation blocks: an explicit stability badge,
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
fn constraint_catalog_markdown_golden() {
    // The "What GMEOW enforces" page lists every gmeow:ValidationRule grouped by
    // finding category. It is large (~50 rules); lock the header region plus the
    // first category's first rule block — the deterministic, low-churn part that
    // pins the per-rule shape (the `<a id="{slug}">` anchor, severity/rule-code
    // rows, and the helpUri deep link). The full render is exercised by the site
    // build; the anchor-resolves-to-helpUri invariant by the crate unit tests.
    let model = common::cached_model();
    let md = to_markdown(&model, &Page::ConstraintCatalog);
    let head: String = md.lines().take(24).collect::<Vec<_>>().join("\n");
    insta::assert_snapshot!(head);
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
    // The per-term neighbourhood SVG is small and bounded; lock it IN FULL. Unlike
    // the head-only goldens, this captures node/edge ordering — the one place a
    // per-process hash seed could reorder output — so the committed-vs-fresh-process
    // comparison itself discriminates cross-process determinism (reinforcing the
    // bundle fold/parity gate). `svg_is_pure` covers within-process purity.
    let model = common::cached_model();
    let term = neighbourhood_term(&model);
    assert!(svg::term_has_neighbourhood(term));
    insta::assert_snapshot!(svg::term_neighbourhood_svg(term));
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

/// The documentation-health completeness distribution + enhanced sections are a
/// PURE PROJECTION of the emitted graph: the distribution partitions every
/// documented-term RECORD in `graph/documentation` exactly once (counted from the
/// read-back the page consumes, NOT a `coverage.rs` recompute), and the dashboard's
/// enhanced surfaces are present. A byte golden would drift with slice content
/// (like the coverage-ratchet baseline); this invariant does not.
#[test]
fn documentation_health_distribution_partitions_the_graph_records() {
    use gmeow_docs::coverage::{DIMENSIONS, TermCoverage};
    use gmeow_docs::rdf::documentation_graph;

    let model = common::cached_model();
    let page = to_markdown(&model, &Page::Health);

    let graph = documentation_graph(&model);
    let total = graph.terms.len();
    // Per-term present-count = the per-term DIMENSIONS a record covers.
    let present = |term: &gmeow_docs::rdf::DocTermFacts| {
        DIMENSIONS
            .iter()
            .filter(|d| term.covers.contains(d.dimension.local_name()))
            .count()
    };

    // The completeness distribution partitions every documented-term RECORD once.
    let mut distributed = 0usize;
    for k in 0..=TermCoverage::TOTAL {
        let count = graph.terms.iter().filter(|t| present(t) == k).count();
        let row = format!("| {k} / {} | {count} |", TermCoverage::TOTAL);
        assert!(
            page.contains(&row),
            "health page missing distribution row `{row}`"
        );
        distributed += count;
    }
    assert_eq!(
        distributed, total,
        "distribution must cover every documented-term record once"
    );

    // The enhanced dashboard surfaces (the cached model carries no reasoning
    // verdict, so the Reasoning section is correctly absent here).
    for section in [
        "## Maturity by slice",
        "## Coverage by slice",
        "diagrams/coverage-heatmap.svg",
        "## Linkage",
        "**Alignment density:**",
        "**Orphan terms:**",
        "## Framework distribution",
        "## Badge legend",
    ] {
        assert!(
            page.contains(section),
            "health page missing enhanced section `{section}`"
        );
    }
    // The framework distribution reflects the gmeow: framework annotations,
    // rendered as `logic:`-prefixed CURIEs.
    assert!(
        page.contains("`logic:DeonticFramework`") || page.contains("`logic:TeleologicalFramework`"),
        "health page framework distribution must list at least one framework"
    );
    assert!(
        !page.contains("## Reasoning"),
        "no reasoning section without an attached verdict"
    );
}

/// The documentation-health page is a PURE PROJECTION of the emitted
/// `graph/documentation` incidence — not a re-derivation from `crate::coverage`.
/// This is proven END-TO-END: an INDEPENDENT raw scan of `to_gmeow_rdf`'s N-Quads
/// counts the per-term `gmeow:docCoversDimension` triples, and every per-dimension
/// "Covered" count the page prints EQUALS that triple count. The per-slice covered
/// totals the read-back the page consumes (`documentation_graph`) reports also equal
/// the raw triple counts grouped by `gmeow:docOwnerSlice`. If `md_health` ever went
/// back to side-computing coverage, these equalities would break.
#[test]
fn health_page_numbers_are_derivable_from_the_documentation_graph() {
    use gmeow_docs::coverage::DIMENSIONS;
    use gmeow_docs::rdf::documentation_graph;
    use gmeow_docs::to_gmeow_rdf;
    use std::collections::BTreeMap;

    const GMEOW: &str = "https://blackcatinformatics.ca/gmeow/";

    // Extract the three IRI tokens of a `<s> <p> <o> <graph> .` line, else None.
    fn take_iri(s: &str) -> Option<(&str, &str)> {
        let s = s.strip_prefix('<')?;
        let end = s.find('>')?;
        Some((&s[..end], s[end + 1..].trim_start()))
    }
    fn iri_triple(line: &str) -> Option<(&str, &str, &str)> {
        let (s, rest) = take_iri(line.trim())?;
        let (p, rest) = take_iri(rest)?;
        let (o, _) = take_iri(rest)?;
        Some((s, p, o))
    }
    let local = |iri: &str| iri.rsplit(['/', '#']).next().unwrap_or(iri).to_owned();

    let model = common::cached_model();
    let nquads = to_gmeow_rdf(&model, &BTreeMap::new());

    // INDEPENDENT raw scan: per documented-term subject, its covered dimension
    // local names (SET semantics — the RDF graph is a set of quads, so a subject
    // that distinct term slugs collide onto carries each incidence once) and its
    // owner slice.
    use std::collections::BTreeSet;
    let term_prefix = format!("{GMEOW}documentation/term/");
    let covers_p = format!("{GMEOW}docCoversDimension");
    let owner_p = format!("{GMEOW}docOwnerSlice");
    let mut term_covers: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut term_owner: BTreeMap<String, String> = BTreeMap::new();
    for line in nquads.lines() {
        let Some((s, p, o)) = iri_triple(line) else {
            continue;
        };
        if !s.starts_with(&term_prefix) {
            continue;
        }
        if p == covers_p {
            term_covers
                .entry(s.to_owned())
                .or_default()
                .insert(local(o));
        } else if p == owner_p {
            term_owner.insert(s.to_owned(), o.to_owned());
        }
    }
    let total = term_owner.len();
    assert!(total > 0, "the model must project documented terms");

    let page = to_markdown(&model, &Page::Health);

    // Every per-dimension "Covered" count on the page equals the raw triple count.
    for dim in DIMENSIONS.iter() {
        let want = dim.dimension.local_name();
        let covered = term_covers
            .values()
            .filter(|dims| dims.iter().any(|d| d == want))
            .count();
        let row = format!("| {} | {covered} | {total} |", dim.label);
        assert!(
            page.contains(&row),
            "health page dimension row `{row}` is not derivable from the graph triples"
        );
    }

    // The read-back the page + heatmap consume reproduces the raw per-slice covered
    // totals (grouped by owner slice) exactly — the projection cannot diverge from
    // the emitted triples.
    let mut raw_by_slice: BTreeMap<String, usize> = BTreeMap::new();
    for (subject, dims) in &term_covers {
        if let Some(owner) = term_owner.get(subject) {
            *raw_by_slice.entry(owner.clone()).or_default() += dims.len();
        }
    }
    let graph = documentation_graph(&model);
    let mut readback_by_slice: BTreeMap<String, usize> = BTreeMap::new();
    for term in &graph.terms {
        *readback_by_slice
            .entry(term.owner_slice.clone())
            .or_default() += term.covers.len();
    }
    assert_eq!(
        readback_by_slice, raw_by_slice,
        "the documentation_graph read-back must reproduce the emitted per-slice \
         docCoversDimension triple counts"
    );
}

/// Doc-entry term slugs are INJECTIVE over the real model: no two distinct term
/// IRIs map to the same `documentation/term/{slug}` subject. Before the resolved
/// slug map this FAILED — the lossy `slugify` case/punctuation fold merged 163
/// distinct terms (e.g. class `AcceptanceStatus` and property `acceptanceStatus`)
/// onto shared subjects, conflating their coverage incidence in the projected
/// graph. Also confirms only a MINORITY of slugs changed (the colliders), so the
/// ~2.4k non-colliders keep their historical URLs.
#[test]
fn doc_entry_term_slugs_are_injective_over_the_model() {
    use gmeow_docs::render::term_slug;

    let model = common::cached_model();
    let distinct_iris: BTreeSet<&str> = model.terms.iter().map(|t| t.iri.as_str()).collect();
    let distinct_slugs: BTreeSet<String> = model.terms.iter().map(term_slug).collect();

    assert_eq!(
        distinct_slugs.len(),
        distinct_iris.len(),
        "doc-entry slugs must be injective: {} distinct term IRIs but only {} distinct \
         slugs — distinct terms are conflated onto shared documentation/term/ subjects",
        distinct_iris.len(),
        distinct_slugs.len(),
    );

    // Only the colliding minority carries a disambiguated (resolved != empty) slug;
    // the resolved slug field is empty exactly when the base was already unique
    // (term_slug then falls back to base), so a non-empty resolved slug marks a
    // participant of a contended base group.
    let disambiguated = model.terms.iter().filter(|t| !t.slug.is_empty()).count();
    assert!(
        disambiguated > 0 && disambiguated < model.terms.len() / 4,
        "expected only the colliding minority to carry a resolved slug, got {disambiguated} \
         of {}",
        model.terms.len(),
    );
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

/// The five standing documentation pages the `llms.txt` surfaces must both
/// reference: (Reference-section title, `llms.txt` URL). The whole-repo model
/// always renders all five (it carries a pipeline), so both surfaces cover each.
const STANDING_PAGES: [(&str, &str); 5] = [
    ("Competency questions", "competency/index.html"),
    ("Conformance fixtures", "fixtures/index.html"),
    ("Notation grammars", "notation/index.html"),
    ("Glossary", "glossary/index.html"),
    ("Build pipeline", "pipeline/index.html"),
];

#[test]
fn llms_txt_references_the_standing_pages() {
    let model = common::cached_model();
    assert!(
        model.pipeline.is_some(),
        "the whole-repo model carries a build pipeline, so the pipeline page is rendered"
    );
    let txt = gmeow_docs::render::llms_txt(&model);
    for (title, url) in STANDING_PAGES {
        assert!(
            txt.contains(&format!("[{title}]({url})")),
            "llms.txt must link the standing page {title:?} at {url:?}"
        );
    }
}

#[test]
fn llms_full_txt_covers_the_standing_pages() {
    // The complete inlined form is linkless, so it names each standing page by
    // its title. Kept in sync with `llms_txt` (both cover all five).
    let model = common::cached_model();
    let txt = gmeow_docs::render::llms_full_txt(&model);
    for (title, _url) in STANDING_PAGES {
        assert!(
            txt.contains(title),
            "llms-full.txt must cover the standing page {title:?}"
        );
    }
}

#[test]
fn llms_surfaces_advertise_the_offline_snippet_corpus() {
    // Both `llms.txt` surfaces point at the offline snippet-corpus affordance so
    // an agent can find the ingestible per-term cards.
    let model = common::cached_model();
    let note = gmeow_docs::render::SNIPPETS_CORPUS_NOTE;
    assert!(
        note.contains("gmeow-dev sync --mode update --outputs docs"),
        "the shared corpus note names the snippets-export affordance"
    );
    assert!(
        gmeow_docs::render::llms_txt(&model).contains(note),
        "llms.txt must carry the offline snippet-corpus note"
    );
    assert!(
        gmeow_docs::render::llms_full_txt(&model).contains(note),
        "llms-full.txt must carry the offline snippet-corpus note"
    );
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
fn cached_site_has_the_required_distribution_surface() {
    let model = common::cached_model();
    let site = common::cached_site();
    // The CSS asset and the landing pages are always present.
    assert!(site.files.contains_key("assets/gmeow.css"));
    assert!(site.files.contains_key("index.md"));
    assert!(site.files.contains_key("index.html"));
    // The T2 surfaces: diagrams, static indexes, and the new section pages.
    assert!(site.files.contains_key("diagrams/slices.svg"));
    assert!(site.files.contains_key("diagrams/concerns.svg"));
    assert!(site.files.contains_key("search-index.json"));
    // The standard llmstxt.org surfaces (superseded `llms-docs.txt`).
    assert!(site.files.contains_key("llms.txt"));
    assert!(site.files.contains_key("llms-full.txt"));
    // The per-term card surface: at least the richest-surface term's
    // card.md must be present in the site tree (terms/{slug}/card.md).
    let card_slug = richest_surface_term_slug(&model);
    let card_path = format!("terms/{card_slug}/card.md");
    assert!(
        site.files.contains_key(card_path.as_str()),
        "expected per-term card at {card_path}"
    );
    assert!(site.files.contains_key("linkages/index.html"));
    assert!(site.files.contains_key("examples/index.html"));
    assert!(site.files.contains_key("concerns/index.html"));
    assert!(site.files.contains_key("external-ontologies/index.html"));
    assert!(site.files.contains_key("integrity-constraints/index.html"));
    // The logic-stereotypes index (resolves the formerly-dangling nav_logic).
    assert!(site.files.contains_key("logic/index.html"));
    // The T3b guides surfaces: recipe/learning-path indexes + the four-boxes page.
    assert!(site.files.contains_key("recipes/index.html"));
    assert!(site.files.contains_key("learning-paths/index.html"));
    assert!(site.files.contains_key("four-boxes/index.html"));
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
fn recipe_and_learning_path_term_curies_resolve_to_documented_terms() {
    // Every `gmeow:usesTerm` CURIE a recipe/learning path carries is expected to
    // already be validated upstream (it names a term the guide genuinely
    // exercises), so the composed quickstart section should never emit its
    // defensive `# UNRESOLVED` comment against the live repo — an unresolved
    // CURIE here is a real authoring bug (a typo or a term that moved/was
    // renamed) worth failing loudly on, not silently swallowing.
    let model = common::cached_model();
    let known: BTreeSet<&str> = model.terms.iter().map(|t| t.curie.as_str()).collect();

    let mut unresolved: Vec<String> = Vec::new();
    for recipe in &model.recipes {
        for curie in &recipe.term_curies {
            if !known.contains(curie.as_str()) {
                unresolved.push(format!("recipe `{}` -> `{curie}`", recipe.slug));
            }
        }
    }
    for path in &model.learning_paths {
        for curie in &path.term_curies {
            if !known.contains(curie.as_str()) {
                unresolved.push(format!("learning path `{}` -> `{curie}`", path.slug));
            }
        }
    }
    assert!(
        unresolved.is_empty(),
        "recipe/learning-path term_curies with no matching documented term: {unresolved:?}"
    );
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
            // A `#fragment` addresses a within-page anchor; the dangling check is
            // about the target FILE, so resolve only the path portion (anchor
            // existence is guaranteed separately by the SourceToPageMap hard-fail).
            let href = href.split('#').next().unwrap_or(&href);
            if href.is_empty()
                || href.contains("://")
                || href.starts_with("mailto:")
                || href.starts_with('#')
            {
                continue;
            }
            let resolved = resolve(dir, href);
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
