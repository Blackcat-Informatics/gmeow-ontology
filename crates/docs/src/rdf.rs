// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Self-hosting RDF projection of the documentation model (PyO3-free).
//!
//! [`to_gmeow_rdf`] dogfoods the doc model: it projects [`DocsModel`] into the
//! `gmeow:` vocabulary as deterministic N-Quads, all in the
//! `gmeow:graph/documentation` named graph, so the documentation surface is
//! itself SPARQL-queryable RDF folded into the offline `gmeow.gts` bundle beside
//! the ontology it describes (Principle 4). This mirrors the discipline of
//! `gmeow-errors`'s `to_gmeow_rdf`: N-Quads (no TriG/prefix handling),
//! `nq_escape`d literals, IRIs (never blank nodes) so the graph round-trips
//! through GTS fold without bnode relabeling, sorted iteration over the
//! already-sorted model collections, and a trailing newline.

use crate::coverage::{CoverageContext, DIMENSIONS, slice_covered_dims, term_coverage};
use crate::maturity::{
    Dimension, MaturityAnchor, anchor_table, coverage_fraction, earned_maturity,
};
use crate::model::DocsModel;
use crate::render::{
    concern_display, concern_slug, precompute_alignment_facets, provenance_chain, slice_display,
    slice_slug, term_advice_facet, term_slug,
};
use crate::source_map::SourceToPageMap;

/// The GMEOW namespace IRI prefix.
const GMEOW: &str = "https://blackcatinformatics.ca/gmeow/";
/// The named graph the documentation projection lives in.
const DOCUMENTATION_GRAPH: &str = "https://blackcatinformatics.ca/gmeow/graph/documentation";
const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
const XSD_BOOLEAN: &str = "http://www.w3.org/2001/XMLSchema#boolean";
const XSD_INTEGER: &str = "http://www.w3.org/2001/XMLSchema#integer";
const XSD_DECIMAL: &str = "http://www.w3.org/2001/XMLSchema#decimal";
const GMEOW_DEFINITION_DIGEST: &str = "https://blackcatinformatics.ca/gmeow/definitionDigest";
const GMEOW_ADDED_IN_VERSION: &str = "https://blackcatinformatics.ca/gmeow/addedInVersion";
const GMEOW_HAS_CHANGELOG_ENTRY: &str = "https://blackcatinformatics.ca/gmeow/hasChangelogEntry";
const GMEOW_ENTRY_VERSION: &str = "https://blackcatinformatics.ca/gmeow/entryVersion";
const GMEOW_ENTRY_NOTE: &str = "https://blackcatinformatics.ca/gmeow/entryNote";
const GMEOW_CHANGELOG_ENTRY: &str = "https://blackcatinformatics.ca/gmeow/ChangelogEntry";

/// Project the documentation model into the `gmeow:` RDF vocabulary as N-Quads,
/// all in the `gmeow:graph/documentation` named graph.
///
/// Vocabulary (every resource is an IRI under the documentation namespace):
/// - each term → `gmeow:documentation/term/{slug}` `a gmeow:DocumentedTerm`,
///   with `gmeow:documents <term-iri>`, `gmeow:docCategory "Class|…"`,
///   `gmeow:docHasDefinition "true|false"^^xsd:boolean`, `gmeow:docUrl
///   "terms/{slug}/index.html"`, and `gmeow:docOwnerSlice <slice-iri>`.
/// - each term's REAL IRI → its content-address provenance (from the term content
///   manifest): `gmeow:definitionDigest "blake3:…"`, `gmeow:addedInVersion "…"`,
///   and one `gmeow:hasChangelogEntry <changelog-iri>` per release — each entry a
///   deterministically-minted `gmeow:documentation/changelog/{term}/{version}` `a
///   gmeow:ChangelogEntry` with `gmeow:entryVersion` and optional `gmeow:entryNote`.
/// - each slice → `gmeow:documentation/slice/{slug}` `a gmeow:DocumentedSlice`,
///   `gmeow:documents <slice-iri>`, `gmeow:docUrl "slices/{slug}/index.html"`.
/// - each concern → `gmeow:documentation/concern/{slug}` `a
///   gmeow:DocumentedConcern`, `gmeow:documents <concern-iri>`, `gmeow:docUrl
///   "concerns/{slug}/index.html"`.
/// - each mapping set → `gmeow:documentation/mapping-set/{n}` `a
///   gmeow:DocumentedMappingSet`, `gmeow:documents <set-iri>`, `gmeow:docUrl
///   "linkages/index.html"`.
/// - each documented term that carries an enrichment fact → one uniform
///   `gmeow:documentation/evidence/{term-slug}/{kind}` `a gmeow:DocEvidence`
///   node PER evidence-kind (`competency`, `diagnostics`, `fixture`, `loss`,
///   `provenance`), each with `gmeow:docEvidenceKind
///   gmeow:docEvidenceKind{Kind}` (an enumerated individual IRI — chosen over a
///   string literal so the kind is a first-class, join-able resource the
///   documentation-slice TBox can subclass), `gmeow:documents <term-iri>`,
///   `gmeow:docClaim`, one or
///   more MANDATORY `gmeow:docGroundedBy` carrier-truthmaker edges,
///   `gmeow:docProducedByChain` (the build stage chain, when a pipeline is
///   attached), an optional `gmeow:docJudgment`, and typed count/digest
///   properties (`gmeow:docFixtureCount`, `gmeow:docCompetencyCount`,
///   `gmeow:docCompetencyQueryDigest`, `gmeow:docFindingCount`,
///   `gmeow:docLossRowCount`, `gmeow:docProvenanceDepth`). A node is emitted
///   ONLY when the term genuinely carries that evidence, and is grounded by
///   construction (an ungrounded evidence node is the doc-layer analogue of a
///   DARK finding — enforced by `tests/doc_evidence.rs`).
///
/// Beyond the per-term `gmeow:DocEvidence` incidence, three teaching-content
/// surfaces are promoted to first-class, term-keyed, SPARQL-queryable subjects so
/// a downstream MCP tool can serve them straight from the bundle:
/// - each conformance fixture → its minted identity IRI
///   `gmeow:documentation/fixture/{slice}-{path}` `a gmeow:DocFixture`, carrying
///   the FULL Turtle body (`gmeow:docFixtureText`), a `gmeow:documents` edge per
///   referenced documented term, an enumerated `gmeow:docFixtureKind`
///   (`gmeow:docFixtureKindWellformed` / `…CounterExample`), and — when the slice
///   authors a binding — `gmeow:docExpectedOutcome`, `gmeow:docViolationCode`, and
///   `gmeow:conformanceRationale`. The SAME IRI the fixture `gmeow:DocEvidence`
///   incidence node grounds by, so the count and the body join on one IRI.
/// - each competency question → `gmeow:documentation/competency/{slug}` `a
///   gmeow:DocumentedCompetency`, grounded by the real competency IRI, carrying
///   the runnable `gmeow:cqQueryText`, `gmeow:cqExpectRowCount` /
///   `gmeow:cqExactRows` / `gmeow:cqRationale` when present, a `gmeow:documents`
///   edge per exercised term, and one `gmeow:CompetencyExpectedRow` node per
///   enumerated expected row (its cells as `gmeow:cqCellVar` /
///   `gmeow:cqCellValueIri` / `gmeow:cqCellValueLiteral`).
/// - each per-term entailment (from the already-materialized
///   `reasoning-explanations`; reason-once) → one
///   `gmeow:documentation/entailment/{term}/{n}` `a gmeow:Entailment` node per
///   derivation, grounded by the term IRI, carrying `gmeow:entailmentRule`,
///   `gmeow:entailmentConclusion`, and one `gmeow:entailmentPremise` per premise
///   (the full derivation DAG, never a flattened single hop). The `entailments`
///   map is empty in the model-only render seam — no entailment nodes are then
///   emitted (an honest absence, never a fabricated "no entailments" claim).
///
/// Output is deterministic: the model collections are already sorted by IRI, and
/// every subject's triples are emitted in a fixed order.
pub fn to_gmeow_rdf(
    model: &DocsModel,
    entailments: &BTreeMap<String, Vec<crate::exec::Entailment>>,
) -> String {
    let graph = format!("<{DOCUMENTATION_GRAPH}>");
    let mut lines: Vec<String> = Vec::new();

    let triple = |s: &str, p: &str, o: &str, lines: &mut Vec<String>| {
        lines.push(format!("{s} <{p}> {o} {graph} ."));
    };
    let literal = |value: &str| format!("\"{}\"", nq_escape(value));

    // Every projected subject is generated A-Box instance data, not vocabulary
    // surface: tag it with a human label, a derived definition-equivalent, its
    // named-graph provenance anchor, and the assertional `gmeow:boxABox` role so
    // the folded bundle satisfies the assertional-tier validation contract while
    // staying genuinely self-describing (the validator requires the box role and
    // a provenance link on every materialized individual). Routed through the
    // single `gmeow_errors::abox::annotate_nquads` contract every generated
    // A-Box individual satisfies identically (the same primitive
    // `gmeow-errors`'s own `to_gmeow_rdf_in_graph` uses for `gmeow:Finding`
    // individuals), rather than a second hand-rolled copy of the four triples.
    let annotate = |subject: &str, label: &str, definition: &str, lines: &mut Vec<String>| {
        // Every caller here already formats `subject` bracketed (`<iri>`) for its
        // OTHER triples in the same block; the shared adapter takes the bare IRI
        // and re-brackets it itself, so strip the brackets back off here rather
        // than thread a second, unbracketed variable through every call site.
        let bare_subject = subject.trim_start_matches('<').trim_end_matches('>');
        gmeow_errors::abox::annotate_nquads(
            bare_subject,
            label,
            definition,
            DOCUMENTATION_GRAPH,
            lines,
        );
    };

    // The coarse-grain provenance shared by every documented term's
    // `gmeow:DocEvidence` provenance node: the producing `stage-docs-render` IRI
    // (the `gmeow:docGroundedBy` truthmaker) and the stage chain walked backward
    // over `gmeow:dataflowConsumes` (the same single relation the per-page
    // provenance footer renders). Present ONLY when the model carries the pipeline — the
    // production docs-graph stage does; a bare unit-test model without a pipeline
    // emits no provenance evidence, an honest absence rather than a fabricated
    // chain.
    // The deterministic coverage incidence, computed ONCE from the single coverage
    // producer ([`crate::coverage`]) and shared by the per-term and per-slice
    // projections below — so the emitted `gmeow:docCoversDimension` graph, the
    // `docs/missing-*` lint counts, and the health page can never disagree. No
    // reasoner: the FCA earned-maturity closure is plain deterministic computation.
    let cov_ctx = CoverageContext::new(model);

    // The per-term crosswalk facets, computed ONCE in a single pass over the
    // linkages — the SAME producer the site `search-index.json` uses
    // ([`crate::render::precompute_alignment_facets`]), so the RDF search facets and
    // the site facets are byte-identical (single source of truth). Keyed by the real
    // term IRI; a term with no linkages is simply absent (honest, no empty facet).
    let alignment_facets = precompute_alignment_facets(model);

    let provenance: Option<(String, String)> = model.pipeline.as_ref().and_then(|pipeline| {
        let stage = pipeline
            .stages
            .iter()
            .find(|s| local_name(&s.iri) == "stage-docs-render")?;
        let chain = provenance_chain(pipeline, "stage-docs-render");
        if chain.is_empty() {
            return None;
        }
        Some((stage.iri.clone(), chain.join(" <- ")))
    });

    // Terms (model.terms is IRI-sorted).
    for term in &model.terms {
        let slug = term_slug(term);
        let subject = format!("<{GMEOW}documentation/term/{slug}>");
        triple(
            &subject,
            RDF_TYPE,
            &format!("<{GMEOW}DocumentedTerm>"),
            &mut lines,
        );
        triple(
            &subject,
            &format!("{GMEOW}documents"),
            &format!("<{}>", term.iri),
            &mut lines,
        );
        triple(
            &subject,
            &format!("{GMEOW}docCategory"),
            &literal(category_name(term.category)),
            &mut lines,
        );
        triple(
            &subject,
            &format!("{GMEOW}docHasDefinition"),
            &boolean(term.definition.is_some()),
            &mut lines,
        );
        triple(
            &subject,
            &format!("{GMEOW}docUrl"),
            &literal(&format!("terms/{slug}/index.html")),
            &mut lines,
        );
        triple(
            &subject,
            &format!("{GMEOW}docOwnerSlice"),
            &format!("<{}>", term.owner_slice),
            &mut lines,
        );
        // Per-term native-reasoner status — projected ONLY when a verdict is
        // attached (the production docs-graph stage consumes stage-reason), so the
        // SPARQL surface never carries a fabricated satisfiability claim. A class
        // is satisfiable unless proven unsatisfiable; a non-class is not-evaluated.
        if let Some(verdict) = &model.reasoning {
            let status = if term.category == crate::model::DocTermCategory::Class {
                if verdict.unsatisfiable.contains(&term.iri) {
                    "unsatisfiable"
                } else {
                    "satisfiable"
                }
            } else {
                "not-evaluated"
            };
            triple(
                &subject,
                &format!("{GMEOW}docReasoningStatus"),
                &literal(status),
                &mut lines,
            );
        }
        annotate(
            &subject,
            &format!("Documentation entry: {}", term.curie),
            &format!(
                "Documentation projection for {} ({}).",
                term.iri,
                category_name(term.category)
            ),
            &mut lines,
        );

        // Searchable content facets — the REAL display label, definition, advisory
        // prose, and crosswalk tokens (NOT the meta `annotate` label), so an offline
        // consumer's `docs_search` matches on the same fields the site
        // `search-index.json` indexes. Reuses the SAME facet builders
        // (`term_advice_facet` / `precompute_alignment_facets`) so the two surfaces
        // can never derive advice/alignments differently. Emitted in a fixed order,
        // and never as an empty predicate (an absent definition/advice/alignment is an
        // honest absence, never a vacuous triple). The MISSING-coverage facet is
        // already carried by `gmeow:docMissesDimension` below — not duplicated here.
        triple(
            &subject,
            &format!("{GMEOW}docSearchLabel"),
            &literal(term.label.as_deref().unwrap_or(&term.curie)),
            &mut lines,
        );
        if let Some(definition) = &term.definition {
            triple(
                &subject,
                &format!("{GMEOW}docSearchDefinition"),
                &literal(definition),
                &mut lines,
            );
        }
        for advice in term_advice_facet(term) {
            triple(
                &subject,
                &format!("{GMEOW}docSearchAdvice"),
                &literal(&advice),
                &mut lines,
            );
        }
        if let Some(tokens) = alignment_facets.get(term.iri.as_str()) {
            for token in tokens {
                triple(
                    &subject,
                    &format!("{GMEOW}docSearchAlignment"),
                    &literal(token),
                    &mut lines,
                );
            }
        }

        // Per-term content-address provenance, projected on the REAL term IRI (not
        // the documentation-entry subject): the content digest, first-seen version,
        // and a reified changelog entry per release. Blank nodes are forbidden here
        // (the graph round-trips through GTS as IRIs), so each changelog entry is a
        // deterministically-minted IRI under the documentation namespace.
        let real = format!("<{}>", term.iri);
        if !term.content_digest.is_empty() {
            triple(
                &real,
                GMEOW_DEFINITION_DIGEST,
                &literal(&term.content_digest),
                &mut lines,
            );
        }
        if let Some(version) = &term.added_in_version {
            triple(&real, GMEOW_ADDED_IN_VERSION, &literal(version), &mut lines);
        }
        for entry in &term.changelog {
            let entry_iri = format!(
                "<{GMEOW}documentation/changelog/{}/{}>",
                term_slug(term),
                set_slug(&entry.version)
            );
            triple(&real, GMEOW_HAS_CHANGELOG_ENTRY, &entry_iri, &mut lines);
            triple(
                &entry_iri,
                RDF_TYPE,
                &format!("<{GMEOW_CHANGELOG_ENTRY}>"),
                &mut lines,
            );
            // A changelog entry is generated A-Box instance data like every other doc
            // subject: annotate it (label / definition / provenance anchor / gmeow:boxABox
            // role) so the folded bundle satisfies the assertional-tier validation contract
            // — otherwise every minted changelog entry trips the four structural annotation
            // lints. The note (when present) IS the entry's definition-equivalent.
            annotate(
                &entry_iri,
                &format!("Changelog entry: {} {}", term.curie, entry.version),
                &entry.note.clone().unwrap_or_else(|| {
                    format!(
                        "Changelog entry recording the {} release of {}.",
                        entry.version, term.iri
                    )
                }),
                &mut lines,
            );
            triple(
                &entry_iri,
                GMEOW_ENTRY_VERSION,
                &literal(&entry.version),
                &mut lines,
            );
            if let Some(note) = &entry.note {
                triple(&entry_iri, GMEOW_ENTRY_NOTE, &literal(note), &mut lines);
            }
        }

        // Uniform `gmeow:DocEvidence` layer: one
        // node per (term, evidence-kind) carrying the same shape for every kind —
        // a claim, its MANDATORY carrier grounding (`gmeow:docGroundedBy`), the
        // build provenance chain, and (where one exists) a preservation judgment —
        // so the seven evidence sources are views of a single per-term
        // justification DAG rather than parallel predicate families. Each node is
        // emitted ONLY when the term genuinely carries that evidence, and is
        // grounded BY CONSTRUCTION (an ungrounded evidence node is the doc-layer
        // analogue of a DARK finding). Kinds are pushed in a fixed alphabetical
        // order for deterministic output. Every node runs through `annotate` like
        // the surrounding per-term subjects so it satisfies the A-Box-tier
        // validation contract.
        for ev in term_evidence(model, term, provenance.as_ref()) {
            let ev_iri = format!("<{GMEOW}documentation/evidence/{slug}/{}>", ev.kind);
            triple(
                &ev_iri,
                RDF_TYPE,
                &format!("<{GMEOW}DocEvidence>"),
                &mut lines,
            );
            triple(
                &ev_iri,
                &format!("{GMEOW}docEvidenceKind"),
                &format!("<{GMEOW}docEvidenceKind{}>", ev.kind_suffix),
                &mut lines,
            );
            triple(
                &ev_iri,
                &format!("{GMEOW}documents"),
                &format!("<{}>", term.iri),
                &mut lines,
            );
            triple(
                &ev_iri,
                &format!("{GMEOW}docClaim"),
                &literal(&ev.claim),
                &mut lines,
            );
            // The grounding edge is mandatory and never empty — the fail-fast
            // grounding invariant (see `tests/doc_evidence.rs`).
            for grounded in &ev.grounded {
                triple(
                    &ev_iri,
                    &format!("{GMEOW}docGroundedBy"),
                    grounded,
                    &mut lines,
                );
            }
            if let Some((_, chain)) = &provenance {
                triple(
                    &ev_iri,
                    &format!("{GMEOW}docProducedByChain"),
                    &literal(chain),
                    &mut lines,
                );
            }
            if let Some(judgment) = &ev.judgment {
                triple(
                    &ev_iri,
                    &format!("{GMEOW}docJudgment"),
                    &literal(judgment),
                    &mut lines,
                );
            }
            for (predicate, value) in &ev.int_props {
                triple(
                    &ev_iri,
                    &format!("{GMEOW}{predicate}"),
                    &integer(*value),
                    &mut lines,
                );
            }
            for (predicate, value) in &ev.str_props {
                triple(
                    &ev_iri,
                    &format!("{GMEOW}{predicate}"),
                    &literal(value),
                    &mut lines,
                );
            }
            annotate(
                &ev_iri,
                &format!("Doc evidence ({}): {}", ev.kind, term.curie),
                &format!(
                    "{} documentation evidence for {} — {}.",
                    ev.kind, term.iri, ev.claim
                ),
                &mut lines,
            );
        }

        // Per-term documentation-coverage incidence: for each of the seventeen
        // per-term dimensions, one `gmeow:docCoversDimension` (COVERED) or
        // `gmeow:docMissesDimension` (applicable ∧ ¬present) edge into the dimension
        // value vocabulary — the incidence the FCA maturity closure runs over.
        // `flags()` already folds the applicability layer (`covered = !applicable ∨
        // present`), so a superset-native term for which a dimension does NOT apply
        // emits `docCoversDimension` for it (never a spurious miss); only an
        // applicable-but-absent dimension emits `docMissesDimension`. Emitted in
        // stable [`DIMENSIONS`] order (a fixed subset of `gmeow:dim*`), so the
        // projection stays deterministic; the two slice-scoped dimensions are
        // emitted on the slice record below, not here.
        let flags = term_coverage(term, &cov_ctx).flags();
        for (dim, covered) in DIMENSIONS.iter().zip(flags) {
            let predicate = if covered {
                "docCoversDimension"
            } else {
                "docMissesDimension"
            };
            triple(
                &subject,
                &format!("{GMEOW}{predicate}"),
                &format!("<{GMEOW}{}>", dim.dimension.local_name()),
                &mut lines,
            );
        }
    }

    // Slices (model.slices is IRI-sorted).
    for slice in &model.slices {
        let slug = slice_slug(slice);
        let subject = format!("<{GMEOW}documentation/slice/{slug}>");
        triple(
            &subject,
            RDF_TYPE,
            &format!("<{GMEOW}DocumentedSlice>"),
            &mut lines,
        );
        triple(
            &subject,
            &format!("{GMEOW}documents"),
            &format!("<{}>", slice.iri),
            &mut lines,
        );
        triple(
            &subject,
            &format!("{GMEOW}docUrl"),
            &literal(&format!("slices/{slug}/index.html")),
            &mut lines,
        );
        annotate(
            &subject,
            &format!("Documentation entry: slice {slug}"),
            &format!("Documentation projection for slice {}.", slice.iri),
            &mut lines,
        );
        // The slice's REAL display name as a searchable facet (the site
        // `search-index.json` indexes the same `slice_display`), so `docs_search`
        // can surface a slice record by its title/label. A slice carries no
        // definition or advisory prose, so only the label facet is emitted.
        triple(
            &subject,
            &format!("{GMEOW}docSearchLabel"),
            &literal(&slice_display(slice)),
            &mut lines,
        );

        // Per-slice documentation-coverage incidence + FCA-derived maturity. The
        // slice's covered-dimension set is its concept intent over ALL nineteen
        // dimensions (a per-term dimension is covered iff EVERY documented term the
        // slice owns COVERS it under `!applicable ∨ present`, so an all-superset-native
        // slice is not penalized for the four applicability-conditioned dimensions;
        // the two slice-scoped dimensions read the slice's docs.md facts) — computed
        // by the single coverage producer, no reasoner.
        let covered = slice_covered_dims(&slice.iri, model, &cov_ctx);
        for dim in Dimension::ALL {
            let predicate = if covered.contains(&dim) {
                "docCoversDimension"
            } else {
                "docMissesDimension"
            };
            triple(
                &subject,
                &format!("{GMEOW}{predicate}"),
                &format!("<{GMEOW}{}>", dim.local_name()),
                &mut lines,
            );
        }
        // The bounded coverage fraction against the FULL anchor's intent — the
        // reference floor (`asserted-or-Full`; no per-slice asserted maturity is
        // carried in the model yet, so FULL is the reference). Computed over the
        // applicability-aware `covered` set, so a dimension that does not apply to the
        // slice's terms counts as covered (never dragging the fraction down for a
        // superset-native slice). A value in [0,1], never an unbounded ratio, so it
        // can never be tuned to a target.
        let fraction = coverage_fraction(&covered, &MaturityAnchor::Full.intent());
        triple(
            &subject,
            &format!("{GMEOW}coverageFraction"),
            &decimal(fraction),
            &mut lines,
        );
        // The FCA-derived earned floor: the largest anchor whose intent ⊆ the
        // slice's covered set. Emitted ONLY when an anchor is earned (an honest
        // absence otherwise) — deterministic next-closure over the incidence, never
        // gated on the pipeline (coverage is always available).
        if let Some(anchor) = earned_maturity(&covered, &anchor_table()) {
            triple(
                &subject,
                &format!("{GMEOW}docEarnedMaturity"),
                &format!("<{GMEOW}{}>", anchor.local_name()),
                &mut lines,
            );
        }
    }

    // Canonical slice-Markdown documents (docs.md + every recursively-discovered
    // `text/markdown` source). Each document is projected as a first-class
    // `gmeow:DocumentedDocument` carrying its owning slice, normalized source path,
    // title, raw content digest, and its RESOLVED generated page location — the
    // last minted by the single `SourceToPageMap` link-rewrite authority, so the
    // "no dangling internal document link" invariant is a graph-level check rather
    // than a per-renderer string pass. The map is a pure function of the model's
    // document set, already validated at discovery (a UTF-8, path-collision, or
    // page-collision defect hard-fails there), so re-building it here is total.
    let page_map = SourceToPageMap::build(model)
        .expect("SourceToPageMap: model documents were already validated at discovery");
    for slice in &model.slices {
        for doc in &slice.documents {
            let Some(node_slug) = page_map.node_slug(&doc.slice_iri, &doc.source_path) else {
                continue;
            };
            let subject = format!("<{GMEOW}documentation/document/{node_slug}>");
            triple(
                &subject,
                RDF_TYPE,
                &format!("<{GMEOW}DocumentedDocument>"),
                &mut lines,
            );
            triple(
                &subject,
                &format!("{GMEOW}documents"),
                &format!("<{}>", doc.slice_iri),
                &mut lines,
            );
            triple(
                &subject,
                &format!("{GMEOW}docSourcePath"),
                &literal(&doc.source_path),
                &mut lines,
            );
            triple(
                &subject,
                &format!("{GMEOW}docTitle"),
                &literal(&doc.title),
                &mut lines,
            );
            triple(
                &subject,
                &format!("{GMEOW}docRawDigest"),
                &literal(&doc.raw_digest),
                &mut lines,
            );
            // The RESOLVED page location — the resolver projection. `page_of`
            // returns the trailing-slash page path the `SourceToPageMap` minted;
            // the top-level `docs.md` resolves to its slice page.
            if let Some(page) = page_map.page_of(&doc.slice_iri, &doc.source_path) {
                triple(
                    &subject,
                    &format!("{GMEOW}docUrl"),
                    &literal(page),
                    &mut lines,
                );
            }
            annotate(
                &subject,
                &doc.title,
                &format!(
                    "Documentation page for `{}` in slice {}.",
                    doc.source_path, doc.slice_iri
                ),
                &mut lines,
            );
        }
    }

    // Concerns (model.concerns is IRI-sorted).
    for concern in &model.concerns {
        let slug = concern_slug(concern);
        let subject = format!("<{GMEOW}documentation/concern/{slug}>");
        triple(
            &subject,
            RDF_TYPE,
            &format!("<{GMEOW}DocumentedConcern>"),
            &mut lines,
        );
        triple(
            &subject,
            &format!("{GMEOW}documents"),
            &format!("<{}>", concern.iri),
            &mut lines,
        );
        triple(
            &subject,
            &format!("{GMEOW}docUrl"),
            &literal(&format!("concerns/{slug}/index.html")),
            &mut lines,
        );
        annotate(
            &subject,
            &format!("Documentation entry: concern {slug}"),
            &format!("Documentation projection for concern {}.", concern.iri),
            &mut lines,
        );
        // The concern's REAL display name (and definition, when authored) as
        // searchable facets — the same fields the site `search-index.json` indexes
        // for a concern record.
        triple(
            &subject,
            &format!("{GMEOW}docSearchLabel"),
            &literal(&concern_display(concern)),
            &mut lines,
        );
        if let Some(definition) = &concern.definition {
            triple(
                &subject,
                &format!("{GMEOW}docSearchDefinition"),
                &literal(definition),
                &mut lines,
            );
        }
    }

    // Mapping sets (model.mapping_sets is IRI-sorted). All link to the single
    // linkages index page.
    for set in &model.mapping_sets {
        let slug = set_slug(&set.iri);
        let subject = format!("<{GMEOW}documentation/mapping-set/{slug}>");
        triple(
            &subject,
            RDF_TYPE,
            &format!("<{GMEOW}DocumentedMappingSet>"),
            &mut lines,
        );
        triple(
            &subject,
            &format!("{GMEOW}documents"),
            &format!("<{}>", set.iri),
            &mut lines,
        );
        triple(
            &subject,
            &format!("{GMEOW}docUrl"),
            &literal("linkages/index.html"),
            &mut lines,
        );
        annotate(
            &subject,
            &format!("Documentation entry: mapping set {slug}"),
            &format!("Documentation projection for mapping set {}.", set.iri),
            &mut lines,
        );
    }

    // ── Fixture bodies (first-class, term-keyed) ─────────────────────────────
    // The conformance Do/Don't fixture bodies, promoted from an opaque per-term
    // count on the `fixture` gmeow:DocEvidence node to first-class,
    // SPARQL-queryable subjects carrying the FULL Turtle body. The subject is the
    // SAME minted identity IRI the fixture evidence node grounds BY (see
    // `term_evidence`'s fixture branch), so the incidence count and the body join
    // on one IRI. Emitted in minted-IRI order for determinism.
    let mut fixture_subjects: Vec<(String, &crate::model::DocFixture)> = model
        .fixtures
        .iter()
        .map(|f| (fixture_identity(f), f))
        .collect();
    fixture_subjects.sort_by(|a, b| a.0.cmp(&b.0));
    // CURIE → term IRI, built ONCE so the per-fixture, per-reference resolution
    // below is an O(1) map lookup rather than an O(terms) linear scan (the fixture
    // loop is otherwise O(fixtures × refs × terms)). Lookup only — iteration order
    // is unaffected since we never iterate this map, only probe it.
    let term_iri_by_curie: std::collections::HashMap<&str, &str> = model
        .terms
        .iter()
        .map(|t| (t.curie.as_str(), t.iri.as_str()))
        .collect();
    for (subject, f) in &fixture_subjects {
        triple(
            subject,
            RDF_TYPE,
            &format!("<{GMEOW}DocFixture>"),
            &mut lines,
        );
        // One `gmeow:documents` edge per referenced DOCUMENTED term (the
        // reference list is CURIEs; resolve each to its real term IRI so the edge
        // lands on the term node, never a CURIE literal). A referenced term that
        // is not itself documented is honestly absent — no fabricated edge.
        for curie in &f.terms_referenced {
            if let Some(iri) = term_iri_by_curie.get(curie.as_str()) {
                triple(
                    subject,
                    &format!("{GMEOW}documents"),
                    &format!("<{}>", iri),
                    &mut lines,
                );
            }
        }
        // The well-formed / counter-example kind as an enumerated individual
        // (mirrors gmeow:docEvidenceKind{Kind}) so the kind is a join-able resource.
        let kind_suffix = match f.kind {
            crate::model::DocFixtureKind::Wellformed => "Wellformed",
            crate::model::DocFixtureKind::CounterExample => "CounterExample",
        };
        triple(
            subject,
            &format!("{GMEOW}docFixtureKind"),
            &format!("<{GMEOW}docFixtureKind{kind_suffix}>"),
            &mut lines,
        );
        triple(
            subject,
            &format!("{GMEOW}docFixtureText"),
            &literal(&f.text),
            &mut lines,
        );
        if let Some(outcome) = &f.expected_outcome {
            triple(
                subject,
                &format!("{GMEOW}docExpectedOutcome"),
                &literal(outcome),
                &mut lines,
            );
        }
        if let Some(code) = &f.violation_code {
            triple(
                subject,
                &format!("{GMEOW}docViolationCode"),
                &literal(code),
                &mut lines,
            );
        }
        if let Some(rationale) = &f.rationale {
            triple(
                subject,
                &format!("{GMEOW}conformanceRationale"),
                &literal(rationale),
                &mut lines,
            );
        }
        // Mandatory grounding: the owning slice IRI is the honest truthmaker (the
        // slice's tests/ tree is where the fixture body lives).
        triple(
            subject,
            &format!("{GMEOW}docGroundedBy"),
            &format!("<{}>", f.slice),
            &mut lines,
        );
        annotate(
            subject,
            &format!("Conformance fixture: {}", f.title),
            &format!(
                "Conformance fixture {} in slice {} ({}).",
                f.logical_path,
                f.slice,
                kind_suffix.to_ascii_lowercase()
            ),
            &mut lines,
        );
    }

    // ── Competency questions (query text + expected rows, first-class) ───────
    // Each competency question promoted from an opaque blake3 digest on the
    // `competency` gmeow:DocEvidence node to a first-class record carrying the
    // runnable query text and the enumerated expected rows, keyed by a minted
    // record IRI and grounded by the real competency IRI. `model.competencies` is
    // IRI-sorted, so the emission order is deterministic.
    for comp in &model.competencies {
        let comp_slug = iri_slug(&comp.iri);
        let subject = format!("<{GMEOW}documentation/competency/{comp_slug}>");
        triple(
            &subject,
            RDF_TYPE,
            &format!("<{GMEOW}DocumentedCompetency>"),
            &mut lines,
        );
        // Mandatory grounding: the real competency-question IRI the record is a
        // projection of.
        triple(
            &subject,
            &format!("{GMEOW}docGroundedBy"),
            &format!("<{}>", comp.iri),
            &mut lines,
        );
        // One `gmeow:documents` edge per exercised term (exercises is
        // sorted/deduped in the model).
        for term_iri in &comp.exercises {
            triple(
                &subject,
                &format!("{GMEOW}documents"),
                &format!("<{term_iri}>"),
                &mut lines,
            );
        }
        if let Some(query) = &comp.query_text {
            triple(
                &subject,
                &format!("{GMEOW}cqQueryText"),
                &literal(query),
                &mut lines,
            );
        }
        if let Some(count) = comp.expected_row_count {
            triple(
                &subject,
                &format!("{GMEOW}cqExpectRowCount"),
                &integer(count),
                &mut lines,
            );
        }
        if let Some(exact) = comp.exact_rows {
            triple(
                &subject,
                &format!("{GMEOW}cqExactRows"),
                &boolean(exact),
                &mut lines,
            );
        }
        if let Some(rationale) = &comp.rationale {
            triple(
                &subject,
                &format!("{GMEOW}cqRationale"),
                &literal(rationale),
                &mut lines,
            );
        }
        // The enumerated expected result rows, each a first-class node carrying
        // its per-variable cells — so the expected result set is queryable content,
        // not just a count. Rows arrive in the model's deterministic order.
        for (i, row) in comp.expected_rows.iter().enumerate() {
            let row_iri = format!("<{GMEOW}documentation/competency/{comp_slug}/row/{i}>");
            triple(
                &subject,
                &format!("{GMEOW}cqExpectedRow"),
                &row_iri,
                &mut lines,
            );
            triple(
                &row_iri,
                RDF_TYPE,
                &format!("<{GMEOW}CompetencyExpectedRow>"),
                &mut lines,
            );
            for cell in &row.cells {
                if let Some(var) = &cell.var {
                    triple(
                        &row_iri,
                        &format!("{GMEOW}cqCellVar"),
                        &literal(var),
                        &mut lines,
                    );
                }
                if let Some(value_iri) = &cell.value_iri {
                    triple(
                        &row_iri,
                        &format!("{GMEOW}cqCellValueIri"),
                        &format!("<{value_iri}>"),
                        &mut lines,
                    );
                }
                if let Some(value_literal) = &cell.value_literal {
                    triple(
                        &row_iri,
                        &format!("{GMEOW}cqCellValueLiteral"),
                        &literal(value_literal),
                        &mut lines,
                    );
                }
            }
            triple(
                &row_iri,
                &format!("{GMEOW}docGroundedBy"),
                &format!("<{}>", comp.iri),
                &mut lines,
            );
            annotate(
                &row_iri,
                &format!("Expected row {i}: {}", comp.iri),
                &format!(
                    "Expected result row {i} of competency question {}.",
                    comp.iri
                ),
                &mut lines,
            );
        }
        annotate(
            &subject,
            &format!("Competency question: {}", comp.iri),
            &format!(
                "Documentation projection of competency question {}.",
                comp.iri
            ),
            &mut lines,
        );
    }

    // ── Entailment DAG (per-term reasoner derivations) ───────────────────────
    // The already-materialized per-term entailments (parsed from stage-reason's
    // `reasoning-explanations` — reason-once, never a second reasoning pass here):
    // one node per derivation carrying its firing rule, the concluded triple, and
    // EVERY premise (the derivation DAG, not a flattened hop). The map is
    // BTreeMap-sorted by term IRI and each Vec is already sorted, so emission is
    // deterministic; an empty map (the model-only render seam) emits nothing.
    for (term_iri, ents) in entailments {
        let term_slug_ent = iri_slug(term_iri);
        for (i, ent) in ents.iter().enumerate() {
            let ent_iri = format!("<{GMEOW}documentation/entailment/{term_slug_ent}/{i}>");
            triple(
                &ent_iri,
                RDF_TYPE,
                &format!("<{GMEOW}Entailment>"),
                &mut lines,
            );
            triple(
                &ent_iri,
                &format!("{GMEOW}documents"),
                &format!("<{term_iri}>"),
                &mut lines,
            );
            triple(
                &ent_iri,
                &format!("{GMEOW}entailmentRule"),
                &literal(&ent.rule),
                &mut lines,
            );
            triple(
                &ent_iri,
                &format!("{GMEOW}entailmentConclusion"),
                &literal(&ent.conclusion),
                &mut lines,
            );
            for premise in &ent.premises {
                triple(
                    &ent_iri,
                    &format!("{GMEOW}entailmentPremise"),
                    &literal(premise),
                    &mut lines,
                );
            }
            // Mandatory grounding: the documented term the derivation is about.
            triple(
                &ent_iri,
                &format!("{GMEOW}docGroundedBy"),
                &format!("<{term_iri}>"),
                &mut lines,
            );
            annotate(
                &ent_iri,
                &format!("Entailment {i}: {term_iri}"),
                &format!(
                    "Reasoner entailment {i} about {term_iri}: {}.",
                    ent.conclusion
                ),
                &mut lines,
            );
        }
    }

    let mut out = lines.join("\n");
    if !out.is_empty() {
        out.push('\n');
    }
    out
}

/// The deterministically-minted identity IRI (angle-bracketed) of a conformance
/// fixture — slice-scoped so two slices' same-named fixtures never collide. This is
/// the SAME IRI the fixture `gmeow:DocEvidence` incidence node grounds by, so the
/// evidence count and the first-class fixture body join on one IRI.
fn fixture_identity(f: &crate::model::DocFixture) -> String {
    format!(
        "<{GMEOW}documentation/fixture/{}-{}>",
        set_slug(local_name(&f.slice)),
        set_slug(&f.logical_path)
    )
}

/// A filesystem-safe slug over a WHOLE IRI (not just its local name), lowercased and
/// reduced to `[a-z0-9-]`. Used to mint collision-free per-competency and
/// per-entailment record IRIs where two subjects can share a local name across
/// namespaces (so slugging the local name alone would fuse them).
fn iri_slug(iri: &str) -> String {
    let mut out = String::with_capacity(iri.len());
    let mut prev_dash = false;
    for ch in iri.chars() {
        let lc = ch.to_ascii_lowercase();
        if lc.is_ascii_alphanumeric() {
            out.push(lc);
            prev_dash = false;
        } else if !prev_dash {
            out.push('-');
            prev_dash = true;
        }
    }
    let trimmed = out.trim_matches('-');
    if trimmed.is_empty() {
        "unnamed".to_string()
    } else {
        trimmed.to_string()
    }
}

/// The stable `gmeow:docCategory` label for a term category.
fn category_name(category: crate::model::DocTermCategory) -> &'static str {
    use crate::model::DocTermCategory;
    match category {
        DocTermCategory::Class => "Class",
        DocTermCategory::Property => "Property",
        DocTermCategory::Individual => "Individual",
        DocTermCategory::Datatype => "Datatype",
        DocTermCategory::Other => "Other",
    }
}

/// An `xsd:boolean`-typed N-Quads literal object.
fn boolean(value: bool) -> String {
    format!("\"{value}\"^^<{XSD_BOOLEAN}>")
}

/// An `xsd:integer`-typed N-Quads literal object.
fn integer(value: i64) -> String {
    format!("\"{value}\"^^<{XSD_INTEGER}>")
}

/// An `xsd:decimal`-typed N-Quads literal object with a fixed 4-decimal lexical
/// form, so the bounded coverage fraction is byte-reproducible across platforms
/// (Rust's `{:.4}` float formatting is deterministic).
fn decimal(value: f64) -> String {
    format!("\"{value:.4}\"^^<{XSD_DECIMAL}>")
}

/// The local name of an IRI: the tail after the last `/` or `#`.
fn local_name(iri: &str) -> &str {
    let cut = iri.rfind(['/', '#']).map(|i| i + 1).unwrap_or(0);
    &iri[cut..]
}

// ── Read-back projection of graph/documentation ─────────────────────────────────
//
// The health page and the coverage heatmap are PURE projections of the emitted
// `graph/documentation` incidence — not a parallel recompute from [`crate::coverage`].
// Both parse the SAME N-Quads [`to_gmeow_rdf`] produces (via [`documentation_graph`])
// and render from THAT, so the published dashboard and the reasoned graph can never
// silently disagree: they are the same bytes read two ways.

use std::collections::{BTreeMap, BTreeSet};

/// The per-term facts read back from `graph/documentation`: which dimensions the
/// record `gmeow:docCoversDimension`, its owning slice, and the real term IRI.
#[derive(Debug, Clone)]
pub struct DocTermFacts {
    /// The documentation-entry subject IRI (`documentation/term/{slug}`).
    pub subject: String,
    /// The real term IRI the record `gmeow:documents`.
    pub documents: String,
    /// The owning slice IRI (`gmeow:docOwnerSlice`).
    pub owner_slice: String,
    /// The per-term `gmeow:DocCoverageDimension` local names the record COVERS.
    pub covers: BTreeSet<String>,
}

/// The per-slice facts read back from `graph/documentation`: the slice's
/// covered-dimension set, its bounded `gmeow:coverageFraction`, the projected
/// `gmeow:docEarnedMaturity` floor, and any asserted `gmeow:sliceDocMaturity`.
#[derive(Debug, Clone)]
pub struct DocSliceFacts {
    /// The documentation-entry subject IRI (`documentation/slice/{slug}`).
    pub subject: String,
    /// The real slice IRI the record `gmeow:documents`.
    pub documents: String,
    /// The `gmeow:DocCoverageDimension` local names the slice COVERS.
    pub covers: BTreeSet<String>,
    /// The bounded coverage fraction (`gmeow:coverageFraction` from 0 through 1).
    pub coverage_fraction: f64,
    /// The projected earned-maturity anchor local name (`gmeow:docEarnedMaturity`),
    /// absent when the slice earns no anchor.
    pub earned: Option<String>,
    /// The asserted maturity anchor local name (`gmeow:sliceDocMaturity`), absent
    /// when the slice claims none (the projection then reports the earned floor).
    pub asserted: Option<String>,
}

/// The read-back projection of the emitted `graph/documentation` incidence — the
/// facts the health page and the coverage heatmap render, parsed from the SAME
/// N-Quads [`to_gmeow_rdf`] emits. Terms and slices arrive in the emitter's
/// deterministic IRI-sorted order.
#[derive(Debug, Clone)]
pub struct DocGraph {
    /// Every `gmeow:DocumentedTerm` record, in subject-IRI order.
    pub terms: Vec<DocTermFacts>,
    /// Every `gmeow:DocumentedSlice` record, in subject-IRI order.
    pub slices: Vec<DocSliceFacts>,
}

/// Split `<iri> …` into `(iri, rest)` where `rest` is the remainder after the
/// closing `>` with leading whitespace trimmed. `None` when `s` is not an
/// angle-bracket IRI token.
fn take_iri(s: &str) -> Option<(&str, &str)> {
    let s = s.strip_prefix('<')?;
    let end = s.find('>')?;
    Some((&s[..end], s[end + 1..].trim_start()))
}

/// Parse the `graph/documentation` N-Quads [`to_gmeow_rdf`] emits into the typed
/// [`DocGraph`] read-back. Every line has the fixed shape `<s> <p> o <graph> .`;
/// only the predicates the health surface reads are decoded (all IRI-valued except
/// `gmeow:coverageFraction`), so a literal-object line for an unrelated predicate is
/// skipped without a full N-Quads parser.
pub fn documentation_graph(model: &DocsModel) -> DocGraph {
    // The read-back reads only per-term / per-slice coverage facts, which are a
    // pure function of the model — the entailment DAG is irrelevant here, so an
    // empty entailments map is honest (no fabricated derivations).
    let nquads = to_gmeow_rdf(model, &BTreeMap::new());
    let graph_suffix = format!(" <{DOCUMENTATION_GRAPH}> .");

    let documents_p = format!("{GMEOW}documents");
    let owner_p = format!("{GMEOW}docOwnerSlice");
    let covers_p = format!("{GMEOW}docCoversDimension");
    let fraction_p = format!("{GMEOW}coverageFraction");
    let earned_p = format!("{GMEOW}docEarnedMaturity");
    let asserted_p = format!("{GMEOW}sliceDocMaturity");
    let term_type = format!("{GMEOW}DocumentedTerm");
    let slice_type = format!("{GMEOW}DocumentedSlice");

    // Per-subject accumulators, keyed by the documentation-entry subject IRI.
    #[derive(Default)]
    struct Row {
        is_term: bool,
        is_slice: bool,
        documents: Option<String>,
        owner: Option<String>,
        covers: BTreeSet<String>,
        fraction: Option<f64>,
        earned: Option<String>,
        asserted: Option<String>,
    }
    let mut rows: BTreeMap<String, Row> = BTreeMap::new();

    for line in nquads.lines() {
        let Some(body) = line.trim().strip_suffix(&graph_suffix) else {
            continue;
        };
        let Some((subject, rest)) = take_iri(body) else {
            continue;
        };
        let Some((predicate, object)) = take_iri(rest) else {
            continue;
        };
        // The IRI-valued object (present for every predicate here except the
        // decimal `coverageFraction`); its bare local name for the value vocab.
        let obj_iri = take_iri(object).map(|(iri, _)| iri);

        let row = rows.entry(subject.to_owned()).or_default();
        match predicate {
            RDF_TYPE => match obj_iri {
                Some(t) if t == term_type => row.is_term = true,
                Some(t) if t == slice_type => row.is_slice = true,
                _ => {}
            },
            p if p == documents_p => {
                if let Some(iri) = obj_iri {
                    row.documents = Some(iri.to_owned());
                }
            }
            p if p == owner_p => {
                if let Some(iri) = obj_iri {
                    row.owner = Some(iri.to_owned());
                }
            }
            p if p == covers_p => {
                if let Some(iri) = obj_iri {
                    row.covers.insert(local_name(iri).to_owned());
                }
            }
            p if p == earned_p => {
                if let Some(iri) = obj_iri {
                    row.earned = Some(local_name(iri).to_owned());
                }
            }
            p if p == asserted_p => {
                if let Some(iri) = obj_iri {
                    row.asserted = Some(local_name(iri).to_owned());
                }
            }
            p if p == fraction_p => {
                // `"0.5000"^^<…decimal>` → the lexical value between the quotes.
                if let Some(value) = object
                    .strip_prefix('"')
                    .and_then(|s| s.split('"').next())
                    .and_then(|s| s.parse::<f64>().ok())
                {
                    row.fraction = Some(value);
                }
            }
            _ => {}
        }
    }

    let mut terms = Vec::new();
    let mut slices = Vec::new();
    for (subject, row) in rows {
        if row.is_term {
            terms.push(DocTermFacts {
                subject,
                documents: row.documents.unwrap_or_default(),
                owner_slice: row.owner.unwrap_or_default(),
                covers: row.covers,
            });
        } else if row.is_slice {
            slices.push(DocSliceFacts {
                subject,
                documents: row.documents.unwrap_or_default(),
                covers: row.covers,
                coverage_fraction: row.fraction.unwrap_or(0.0),
                earned: row.earned,
                asserted: row.asserted,
            });
        }
    }
    DocGraph { terms, slices }
}

/// One uniform `gmeow:DocEvidence` node the projection emits for a documented
/// term. Every kind carries the SAME shape — a claim, its mandatory
/// `gmeow:docGroundedBy` carrier truthmaker(s), typed count/string properties,
/// and (where one exists) a preservation/confidence judgment — so a future
/// evidence source plugs in as a new `docEvidenceKind`, not a new predicate
/// family.
struct Evidence {
    /// The URL-slug kind segment used in the node IRI (`fixture`, `competency`,
    /// `diagnostics`, `loss`, `provenance`).
    kind: &'static str,
    /// The `gmeow:docEvidenceKind{Suffix}` enumerated-individual suffix.
    kind_suffix: &'static str,
    /// The short human claim string (`gmeow:docClaim`).
    claim: String,
    /// Pre-formatted N-Quad `gmeow:docGroundedBy` objects (an IRI `<…>` or an
    /// escaped literal `"…"`). NEVER empty — every evidence node is grounded by
    /// construction, the fail-fast grounding invariant.
    grounded: Vec<String>,
    /// The optional preservation/confidence judgment (`gmeow:docJudgment`).
    judgment: Option<String>,
    /// Typed `xsd:integer` count properties (predicate local name → value).
    int_props: Vec<(&'static str, i64)>,
    /// Typed string properties (predicate local name → literal value).
    str_props: Vec<(&'static str, String)>,
}

/// Build the uniform evidence nodes for one documented term, in a fixed
/// alphabetical kind order (`competency`, `diagnostics`, `fixture`, `loss`,
/// `provenance`) so the projection stays deterministic. A kind is included ONLY
/// when the term genuinely carries that evidence — never a vacuous node — and
/// every returned node carries at least one grounding object.
fn term_evidence(
    model: &DocsModel,
    term: &crate::model::DocTerm,
    provenance: Option<&(String, String)>,
) -> Vec<Evidence> {
    let mut out: Vec<Evidence> = Vec::new();

    // competency — the term is exercised by one or more competency questions.
    // Grounded by each competency IRI; the joined query digest travels as a
    // typed property so the evidence node carries the query fingerprint the
    // issue's completeness contract lists.
    let comps: Vec<&crate::model::DocCompetency> = model
        .competencies
        .iter()
        .filter(|c| c.exercises.iter().any(|iri| iri == &term.iri))
        .collect();
    if !comps.is_empty() {
        let grounded = comps.iter().map(|c| format!("<{}>", c.iri)).collect();
        let mut str_props: Vec<(&'static str, String)> = Vec::new();
        let queries: String = comps
            .iter()
            .filter_map(|c| c.query_text.as_deref())
            .collect::<Vec<_>>()
            .join("\n");
        if !queries.is_empty() {
            str_props.push((
                "docCompetencyQueryDigest",
                format!("blake3:{}", blake3::hash(queries.as_bytes()).to_hex()),
            ));
        }
        out.push(Evidence {
            kind: "competency",
            kind_suffix: "Competency",
            claim: format!("exercised by {} competency question(s)", comps.len()),
            grounded,
            judgment: None,
            int_props: vec![("docCompetencyCount", comps.len() as i64)],
            str_props,
        });
    }

    // diagnostics — the term is a key in the diagnostics-to-term join. Grounded
    // by each finding code (the finding's stable identifier). On the real repo a
    // documented term joins when a finding structurally concerns it — e.g. a SHACL
    // violation's constrained `sh:path` property (`gmeow:hasReferenceFrame` carries
    // the ExpressionFrameRequirement MinCount violations); a term no finding concerns
    // is honestly absent (proven by the synthetic-digest and real-repo B1 gates).
    if let Some(findings) = model
        .diagnostics
        .as_ref()
        .and_then(|d| d.by_term.get(&term.iri))
        .filter(|f| !f.is_empty())
    {
        let grounded = findings.iter().map(|f| literal_object(&f.code)).collect();
        out.push(Evidence {
            kind: "diagnostics",
            kind_suffix: "Diagnostics",
            claim: format!("has {} diagnostic finding(s)", findings.len()),
            grounded,
            judgment: None,
            int_props: vec![("docFindingCount", findings.len() as i64)],
            str_props: Vec::new(),
        });
    }

    // fixture — the term is referenced by one or more conformance fixtures /
    // counter-examples. Grounded by a deterministically-minted fixture IRI per
    // fixture (slice-scoped so two slices' same-named fixtures never collide).
    let fixtures: Vec<&crate::model::DocFixture> = model
        .fixtures
        .iter()
        .filter(|f| f.terms_referenced.iter().any(|c| c == &term.curie))
        .collect();
    if !fixtures.is_empty() {
        let grounded = fixtures.iter().map(|f| fixture_identity(f)).collect();
        out.push(Evidence {
            kind: "fixture",
            kind_suffix: "Fixture",
            claim: format!("has {} conformance fixture(s)", fixtures.len()),
            grounded,
            judgment: None,
            int_props: vec![("docFixtureCount", fixtures.len() as i64)],
            str_props: Vec::new(),
        });
    }

    // loss — the term degrades under one or more projections: the dynamic
    // per-term ledger rows (`TermLossDigest`) and/or a static authored
    // projection-loss target (`DocLossTarget`) whose subject local name is this
    // term. Grounded by each ledger-row / loss-target identifier; the distinct
    // preservation kinds are the evidence judgment.
    let mut loss_grounded: Vec<String> = Vec::new();
    let mut preservation_kinds: Vec<String> = Vec::new();
    if let Some(rows) = model
        .term_loss
        .as_ref()
        .and_then(|d| d.by_term.get(&term.iri))
    {
        for row in rows {
            loss_grounded.push(literal_object(&row.target));
            preservation_kinds.push(row.preservation_kind.clone());
        }
    }
    let term_local = local_name(&term.iri);
    for lt in model
        .loss_targets
        .iter()
        .filter(|lt| lt.target == term_local)
    {
        loss_grounded.push(literal_object(&lt.target));
        preservation_kinds.push(lt.preservation_kind.clone());
    }
    if !loss_grounded.is_empty() {
        preservation_kinds.sort();
        preservation_kinds.dedup();
        out.push(Evidence {
            kind: "loss",
            kind_suffix: "Loss",
            claim: format!("has {} projection-loss row(s)", loss_grounded.len()),
            int_props: vec![("docLossRowCount", loss_grounded.len() as i64)],
            grounded: loss_grounded,
            judgment: Some(preservation_kinds.join(", ")),
            str_props: Vec::new(),
        });
    }

    // provenance — every documented term is produced by the docs render chain,
    // grounded by the `stage-docs-render` IRI, with the backward stage-walk
    // depth as a typed property. Present only when the model carries the
    // pipeline (see the caller); the shared chain string rides every evidence
    // node as `gmeow:docProducedByChain`, emitted by the caller.
    if let Some((stage_iri, chain)) = provenance {
        let depth = chain.split(" <- ").count() as i64;
        out.push(Evidence {
            kind: "provenance",
            kind_suffix: "Provenance",
            claim: format!("rendered by the docs build pipeline ({depth} stage(s))"),
            grounded: vec![format!("<{stage_iri}>")],
            judgment: None,
            int_props: vec![("docProvenanceDepth", depth)],
            str_props: Vec::new(),
        });
    }

    out
}

/// An escaped N-Quads string-literal object for a grounding identifier that is a
/// stable code, not an IRI (a diagnostic code, a ledger-row target).
fn literal_object(value: &str) -> String {
    format!("\"{}\"", nq_escape(value))
}

/// A filesystem-safe slug from a mapping-set IRI's local name (tail after the
/// last `/` or `#`, lowercased + reduced to `[a-z0-9-]`).
fn set_slug(iri: &str) -> String {
    let cut = iri.rfind(['/', '#']).map(|i| i + 1).unwrap_or(0);
    let name = &iri[cut..];
    let mut out = String::with_capacity(name.len());
    let mut prev_dash = false;
    for ch in name.chars() {
        let lc = ch.to_ascii_lowercase();
        if lc.is_ascii_alphanumeric() {
            out.push(lc);
            prev_dash = false;
        } else if !prev_dash {
            out.push('-');
            prev_dash = true;
        }
    }
    let trimmed = out.trim_matches('-');
    if trimmed.is_empty() {
        "unnamed".to_string()
    } else {
        trimmed.to_string()
    }
}

/// Escape a string literal for N-Triples/N-Quads (mirrors
/// `gmeow_errors::render::nq_escape`).
fn nq_escape(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    for ch in value.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            // Any remaining C0 control character must be escaped as \uXXXX, else
            // the literal is illegal raw in an N-Quads STRING_LITERAL_QUOTE and
            // rdflib/oxigraph reject the graph (mirrors diagnostics).
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04X}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{DocTerm, DocTermCategory};

    fn tiny_model() -> DocsModel {
        DocsModel {
            title: "T".to_string(),
            version: "2".to_string(),
            slices: Vec::new(),
            terms: vec![
                DocTerm {
                    iri: format!("{GMEOW}Cat"),
                    curie: "gmeow:Cat".to_string(),
                    label: Some("Cat".to_string()),
                    definition: Some("A cat.".to_string()),
                    category: DocTermCategory::Class,
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
                },
                DocTerm {
                    iri: format!("{GMEOW}hasOwner"),
                    curie: "gmeow:hasOwner".to_string(),
                    label: None,
                    definition: None,
                    category: DocTermCategory::Property,
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
                },
            ],
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
            four_boxes: None,
            concept_doi: None,
            pipeline: None,

            available_languages: vec!["english".to_string()],

            translations: crate::i18n::Translations::default(),

            ui_catalog: crate::i18n::UiCatalog::default(),
            reasoning: None,
            diagnostics: None,
            term_loss: None,
            schema_fragments: None,
            lang: String::new(),
        }
    }

    #[test]
    fn projection_is_well_formed_and_deterministic() {
        let model = tiny_model();
        let a = to_gmeow_rdf(&model, &BTreeMap::new());
        let b = to_gmeow_rdf(&model, &BTreeMap::new());
        assert_eq!(a, b, "projection must be deterministic");

        // Every line is a 4-term N-Quad in the documentation graph.
        for line in a.lines() {
            assert!(
                line.ends_with(&format!("<{DOCUMENTATION_GRAPH}> .")),
                "line not in documentation graph: {line}"
            );
        }
        assert!(a.contains("DocumentedTerm"));
        assert!(a.contains("docCategory"));
        // The definition-less property records false; the cat records true.
        assert!(a.contains(&format!("\"true\"^^<{XSD_BOOLEAN}>")));
        assert!(a.contains(&format!("\"false\"^^<{XSD_BOOLEAN}>")));
        assert!(a.ends_with('\n'));
    }

    #[test]
    fn empty_model_yields_empty_string() {
        let model = DocsModel {
            title: "T".to_string(),
            version: "2".to_string(),
            slices: Vec::new(),
            terms: Vec::new(),
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
            four_boxes: None,
            concept_doi: None,
            pipeline: None,

            available_languages: vec!["english".to_string()],

            translations: crate::i18n::Translations::default(),

            ui_catalog: crate::i18n::UiCatalog::default(),
            reasoning: None,
            diagnostics: None,
            term_loss: None,
            schema_fragments: None,
            lang: String::new(),
        };
        assert_eq!(to_gmeow_rdf(&model, &BTreeMap::new()), "");
    }

    #[test]
    fn entailment_dag_round_trips_rule_conclusion_and_all_premises() {
        let model = tiny_model();
        let mut entailments: BTreeMap<String, Vec<crate::exec::Entailment>> = BTreeMap::new();
        entailments.insert(
            format!("{GMEOW}Cat"),
            vec![crate::exec::Entailment {
                rule: "owl:subClassOf-transitive".to_string(),
                conclusion: "gmeow:Cat rdfs:subClassOf gmeow:Animal".to_string(),
                premises: vec![
                    "gmeow:Cat rdfs:subClassOf gmeow:Feline".to_string(),
                    "gmeow:Feline rdfs:subClassOf gmeow:Animal".to_string(),
                ],
            }],
        );
        let nq = to_gmeow_rdf(&model, &entailments);

        // Determinism holds with a non-empty map too.
        assert_eq!(nq, to_gmeow_rdf(&model, &entailments));

        // The entailment node is minted, typed, term-keyed, and grounded.
        assert!(nq.contains(&format!("<{GMEOW}Entailment>")));
        assert!(nq.contains(&format!("{GMEOW}entailmentRule> ")));
        assert!(nq.contains("owl:subClassOf-transitive"));
        assert!(nq.contains("gmeow:Cat rdfs:subClassOf gmeow:Animal"));
        // BOTH premises round-trip (the derivation DAG, not a flattened hop).
        assert!(nq.contains("gmeow:Cat rdfs:subClassOf gmeow:Feline"));
        assert!(nq.contains("gmeow:Feline rdfs:subClassOf gmeow:Animal"));
        let premise_lines = nq
            .lines()
            .filter(|l| l.contains(&format!("{GMEOW}entailmentPremise>")))
            .count();
        assert_eq!(premise_lines, 2, "both premises must be emitted");

        // The node grounds by the documented term IRI.
        assert!(nq.contains(&format!("{GMEOW}docGroundedBy> <{GMEOW}Cat>")));

        // Empty map ⇒ no entailment nodes (honest absence).
        let bare = to_gmeow_rdf(&model, &BTreeMap::new());
        assert!(!bare.contains(&format!("<{GMEOW}Entailment>")));
    }

    #[test]
    fn search_facets_carry_real_content_and_honest_absence() {
        use crate::model::DocLinkage;
        let mut model = tiny_model();
        // Cat gains advisory prose + a crosswalk linkage, so its documentation-entry
        // record projects the full search-facet set.
        model.terms[0].scope_notes = vec!["Prefer for a domestic cat.".to_string()];
        model.linkages = vec![DocLinkage {
            mapping_set: None,
            subject: format!("{GMEOW}Cat"),
            subject_curie: "gmeow:Cat".to_string(),
            predicate: "http://www.w3.org/2004/02/skos/core#exactMatch".to_string(),
            object: "http://www.wikidata.org/entity/Q146".to_string(),
            justification: None,
            confidence: None,
            owner_slice: format!("{GMEOW}slice/zoo"),
        }];
        let nq = to_gmeow_rdf(&model, &BTreeMap::new());

        // Cat: the REAL label / definition, the advice string, and the alignment token
        // — NOT the meta annotate() label.
        assert!(nq.contains(&format!("{GMEOW}docSearchLabel> \"Cat\"")));
        assert!(nq.contains(&format!("{GMEOW}docSearchDefinition> \"A cat.\"")));
        assert!(nq.contains(&format!(
            "{GMEOW}docSearchAdvice> \"Prefer for a domestic cat.\""
        )));
        assert!(nq.contains(&format!("{GMEOW}docSearchAlignment> \"exactMatch:Q146\"")));

        // hasOwner has no label/definition/advice/linkage: docSearchLabel falls back to
        // the CURIE, and NO definition/advice/alignment facet is fabricated.
        assert!(nq.contains(&format!("{GMEOW}docSearchLabel> \"gmeow:hasOwner\"")));
        // The definition-less property emits no docSearchDefinition line for itself.
        let has_owner_subject = format!("<{GMEOW}documentation/term/hasowner>");
        for line in nq.lines().filter(|l| l.starts_with(&has_owner_subject)) {
            assert!(
                !line.contains("docSearchDefinition"),
                "definition-less property must not emit docSearchDefinition: {line}"
            );
            assert!(
                !line.contains("docSearchAdvice"),
                "advice-less property must not emit docSearchAdvice: {line}"
            );
            assert!(
                !line.contains("docSearchAlignment"),
                "linkage-less property must not emit docSearchAlignment: {line}"
            );
        }
    }
}
