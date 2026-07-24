// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The deterministic Typst-source renderer.
//!
//! [`render_typ`] turns a [`DocsModel`] (plus the axiom listings, the
//! bibliography bytes, and the shared per-format capability table) into a single
//! Typst source string. The output is a pure function of its inputs — no clock,
//! no environment, no map-iteration nondeterminism — so it is byte-reproducible
//! and `insta`-goldenable, and the PDF compiled from it is byte-stable.
//!
//! ## The one escape authority
//!
//! Every piece of interpolated model text (titles, term labels, definitions,
//! examples, axiom bodies, table cells) is routed through the single
//! [`escape_typ`] helper. Rather than backslash-escaping each Typst markup
//! metacharacter (`#`, `$`, `@`, `_`, `*`, `` ` ``, `<`, `\`, …) individually,
//! `escape_typ` emits the value as the BODY of a Typst *string literal*: inside a
//! `"…"` string every markup metacharacter is inert, so the rendered document can
//! never be perturbed — or broken — by hostile term text. Call sites splice the
//! escaped body into `#"…"` (a bare string expression renders as literal text)
//! or `== #"…"` (a heading whose content is that string). There is no ad-hoc
//! escaping anywhere else.

use std::collections::{BTreeMap, BTreeSet};

use gmeow_docs::formats::{Capability, DocFormat, FormatCapabilities};
use gmeow_docs::model::{DocSlice, DocTerm, DocTermCategory, DocsModel};
use gmeow_docs::source_map::SourceToPageMap;

use crate::doc_render::render_document;

/// The logical source path of the slice-page markdown (`docs.md`) — the guide whose
/// prose is grafted onto the slice, rendered before the slice's child documents and
/// term material.
const SLICE_GUIDE_SOURCE: &str = "docs.md";

/// The FAIR-metadata gate this document's FAIR statement cites. Held as a literal
/// so a text test can assert its presence in the rendered source.
pub const FAIR_GATE: &str = "meta:gate-fair-metadata";

// ── The single escape authority ────────────────────────────────────────────

/// Escape an arbitrary model string for safe interpolation into Typst source.
///
/// Returns the BODY of a Typst double-quoted string literal — i.e. with `\` and
/// `"` backslash-escaped and every control character rendered as a `\u{…}`
/// escape (newlines collapse to a single space so one-lined fields stay on one
/// line). Splicing the result into `#"{body}"` yields a Typst string expression
/// whose value is exactly the input text; because it is a STRING (not markup),
/// every Typst markup metacharacter it may contain (`#`, `$`, `@`, `_`, `*`,
/// `` ` ``, `<`, `>`, `[`, `]`, `=`, …) is inert and cannot break compilation.
///
/// This is the ONE place term text is made Typst-safe; no call site escapes
/// ad hoc.
pub fn escape_typ(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' | '\r' | '\t' => out.push(' '),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{{{:x}}}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

/// A Typst string EXPRESSION (`"…"`) for `s`, rendering as literal text in
/// markup. The workhorse behind every interpolation.
fn tstr(s: &str) -> String {
    format!("\"{}\"", escape_typ(s))
}

/// A Typst markup interpolation that displays `s` as literal text (`#"…"`).
fn disp(s: &str) -> String {
    format!("#{}", tstr(s))
}

// ── Top-level renderer ─────────────────────────────────────────────────────

/// Render the deterministic Typst source for the print documentation of `model`.
///
/// * `axioms` — a name → raw-bytes map of axiom listings (e.g. per-profile logic
///   text); each entry is rendered verbatim inside a raw code block. A
///   [`BTreeMap`] so iteration is deterministic.
/// * `bib` — the bibliography database (hayagriva/BibTeX bytes). An EMPTY buffer
///   omits the bibliography section entirely (a valid document, never a panic).
/// * `losses` — the shared per-format capability table; the loss appendix reads
///   the [`DocFormat::Pdf`] row from it, so the appendix matches the graph ledger
///   by construction.
pub fn render_typ(
    model: &DocsModel,
    axioms: &BTreeMap<String, Vec<u8>>,
    bib: &[u8],
    losses: &[FormatCapabilities],
) -> String {
    let mut out = String::new();

    preamble(&mut out, model);
    title_page(&mut out, model);
    out.push_str("#outline()\n#pagebreak()\n\n");

    slice_chapters(&mut out, model);
    axiom_chapter(&mut out, axioms);

    section_metrics(&mut out, model);
    section_methodology(&mut out, model);
    section_fair(&mut out);
    section_comparisons(&mut out, model);
    section_pipeline_dag(&mut out, model);
    section_loss_appendix(&mut out, losses);

    bibliography(&mut out, bib);

    out
}

// ── Preamble + title ───────────────────────────────────────────────────────

fn preamble(out: &mut String, model: &DocsModel) {
    // `set document(...)` records the title/author in the PDF metadata; both are
    // deterministic string literals. `set page`/`set text` fix the layout so the
    // rendered bytes do not depend on defaults drifting across patch releases.
    out.push_str(&format!(
        "#set document(title: {}, author: (\"GMEOW\",))\n",
        tstr(&model.title)
    ));
    out.push_str("#set page(paper: \"a4\", numbering: \"1\")\n");
    out.push_str("#set text(font: \"Libertinus Serif\", size: 10pt, lang: \"en\")\n");
    out.push_str("#set heading(numbering: \"1.1\")\n\n");
}

fn title_page(out: &mut String, model: &DocsModel) {
    out.push_str("#align(center)[\n");
    out.push_str(&format!(
        "  #text(size: 24pt, weight: \"bold\")[{}]\n",
        disp(&model.title)
    ));
    out.push_str("  #v(1em)\n");
    out.push_str(&format!(
        "  #text(size: 14pt)[Version {}]\n",
        disp(&model.version)
    ));
    out.push_str("]\n#pagebreak()\n\n");
}

// ── Slice chapters + term entries ──────────────────────────────────────────

fn slice_chapters(out: &mut String, model: &DocsModel) {
    // The single link authority, rebuilt from the model (a pure function of its
    // already-validated document set — the SAME map the HTML site and the mdbook
    // consult). It resolves every intra-corpus document link the inlined guides and
    // child documents carry, and mints their collision-free Typst labels.
    let map = SourceToPageMap::build(model)
        .expect("SourceToPageMap: model documents were already validated at discovery");

    for slice in &model.slices {
        let terms: Vec<&DocTerm> = model
            .terms
            .iter()
            .filter(|t| t.owner_slice == slice.iri)
            .collect();
        // A slice with no documented terms still gets a chapter, so the outline
        // is a total projection of the slice catalog (honest empty state).
        out.push_str(&format!("= Slice: {}\n\n", disp(&slice_title(slice))));

        // 1) The GUIDE: the slice's `docs.md` prose, inlined directly (headings
        //    demoted one level so they sit under the `= Slice:` chapter).
        if let Some(guide) = slice
            .documents
            .iter()
            .find(|d| d.source_path == SLICE_GUIDE_SOURCE)
            && let Some(page) = map.page_of(&slice.iri, &guide.source_path)
        {
            let page = page.to_string();
            render_document(
                out,
                &guide.source_text,
                &slice.iri,
                &guide.source_path,
                &page,
                1,
                &map,
            );
        }

        // 2) The CHILD documents (every non-`docs.md` markdown), in the map's
        //    path-sorted order — the SAME order and set the HTML/mdbook emit.
        for entry in map.slice_children(&slice.iri) {
            if let Some(doc) = slice
                .documents
                .iter()
                .find(|d| d.source_path == entry.source_path)
            {
                render_document(
                    out,
                    &doc.source_text,
                    &slice.iri,
                    &doc.source_path,
                    &entry.page,
                    1,
                    &map,
                );
            }
        }

        // 3) The generated TERM material, last.
        if terms.is_empty() {
            out.push_str("_No documented terms in this slice._\n\n");
            continue;
        }
        for term in terms {
            term_entry(out, term);
        }
    }
}

/// The display title of a slice: its `dcterms:title`, else `rdfs:label`, else the
/// last path segment of its IRI.
fn slice_title(slice: &DocSlice) -> String {
    slice
        .title
        .clone()
        .or_else(|| slice.label.clone())
        .unwrap_or_else(|| {
            slice
                .iri
                .trim_end_matches('/')
                .rsplit('/')
                .next()
                .unwrap_or(&slice.iri)
                .to_string()
        })
}

fn term_entry(out: &mut String, term: &DocTerm) {
    out.push_str(&format!("== {}\n\n", disp(&term.curie)));
    if let Some(label) = &term.label
        && !label.is_empty()
    {
        out.push_str(&format!(
            "*{}* #h(0.5em) {}\n\n",
            disp(label),
            category_label(term.category)
        ));
    } else {
        out.push_str(&format!("{}\n\n", category_label(term.category)));
    }

    if let Some(def) = &term.definition
        && !def.is_empty()
    {
        out.push_str(&format!("{}\n\n", disp(def)));
    }

    field(out, "Parents", &term.parents);
    field(out, "Domain", &term.domain);
    field(out, "Range", &term.range);
    field(out, "Use when", &term.use_when);
    field(out, "Avoid when", &term.avoid_when);
    field(out, "How to use", &term.how_to_use);
    field(out, "Scope notes", &term.scope_notes);
    field(out, "Examples", &term.examples);
    field(out, "Logic", &term.logic_stereotypes);
    field(out, "Related", &term.related_terms);
}

/// The singular human category label of a term.
fn category_label(cat: DocTermCategory) -> &'static str {
    match cat {
        DocTermCategory::Class => "Class",
        DocTermCategory::Property => "Property",
        DocTermCategory::Individual => "Individual",
        DocTermCategory::Datatype => "Datatype",
        DocTermCategory::Other => "Term",
    }
}

/// Emit one advisory field line `*Label:* v1; v2; …` when non-empty. Values are
/// display strings resolved upstream (local names / prose); each is escaped.
fn field(out: &mut String, label: &str, values: &[String]) {
    if values.is_empty() {
        return;
    }
    // Escape each value individually, then join the escaped bodies with a plain
    // "; " separator (no Typst metacharacters, so it needs no escaping itself)
    // into a SINGLE string literal — one `#"…"` expression rather than a chain
    // of string-concatenation.
    let joined = values
        .iter()
        .map(|v| escape_typ(v))
        .collect::<Vec<_>>()
        .join("; ");
    out.push_str(&format!("*{label}:* #\"{joined}\"\n\n"));
}

// ── Axiom chapter ──────────────────────────────────────────────────────────

fn axiom_chapter(out: &mut String, axioms: &BTreeMap<String, Vec<u8>>) {
    if axioms.is_empty() {
        return;
    }
    out.push_str("= Axiom listings\n\n");
    for (name, bytes) in axioms {
        out.push_str(&format!("== {}\n\n", disp(name)));
        let text = String::from_utf8_lossy(bytes);
        // `raw(block: true, "…")` takes the body as a STRING, so backticks or any
        // other char in the axiom text cannot break the code block.
        out.push_str(&format!("#raw(block: true, {})\n\n", tstr(&text)));
    }
}

// ── Academic sections (each behind a stable machine-readable marker) ─────────
//
// Every section opens with a `// <<section:slug>>` Typst line comment AND a
// heading, so a plain-text search for the marker in the emitted `.typ` is stable
// regardless of layout.

fn section_metrics(out: &mut String, model: &DocsModel) {
    out.push_str("// <<section:metrics>>\n");
    out.push_str("= Ontology metrics\n\n");

    let term_count = model.terms.len();
    let (classes, properties, individuals, datatypes) = category_counts(model);
    let expressivity = dl_expressivity(model);
    let consistency = consistency_verdict(model);
    let (cq_count, covered, coverage_pct) = competency_coverage(model);

    out.push_str("#table(\n  columns: 2,\n  align: (left, left),\n");
    metric_row(out, "Terms", &term_count.to_string());
    metric_row(out, "Classes", &classes.to_string());
    metric_row(out, "Properties", &properties.to_string());
    metric_row(out, "Individuals", &individuals.to_string());
    metric_row(out, "Datatypes", &datatypes.to_string());
    metric_row(
        out,
        "DL expressivity (observed feature profile)",
        &expressivity,
    );
    metric_row(out, "Consistency", &consistency);
    metric_row(out, "Competency questions", &cq_count.to_string());
    metric_row(
        out,
        "Competency coverage",
        &format!("{covered}/{term_count} terms ({coverage_pct})"),
    );
    out.push_str(")\n\n");
}

fn metric_row(out: &mut String, k: &str, v: &str) {
    // Inside `#table(...)` the cells are CODE arguments, so they are bare string
    // expressions (no leading `#`).
    out.push_str(&format!("  {}, {},\n", tstr(k), tstr(v)));
}

/// Per-category term counts `(classes, properties, individuals, datatypes)`.
fn category_counts(model: &DocsModel) -> (usize, usize, usize, usize) {
    let mut c = (0usize, 0usize, 0usize, 0usize);
    for t in &model.terms {
        match t.category {
            DocTermCategory::Class => c.0 += 1,
            DocTermCategory::Property => c.1 += 1,
            DocTermCategory::Individual => c.2 += 1,
            DocTermCategory::Datatype => c.3 += 1,
            DocTermCategory::Other => {}
        }
    }
    c
}

/// A deterministic, honest DL expressivity descriptor derived from OBSERVABLE
/// model features (a lower bound, never an overclaim): the base attributive
/// language `AL`, plus `H` when a class/property hierarchy is present, plus `(D)`
/// when datatype terms or datatype-valued ranges appear.
fn dl_expressivity(model: &DocsModel) -> String {
    let has_hierarchy = model.terms.iter().any(|t| !t.parents.is_empty());
    let has_datatypes = model.terms.iter().any(|t| {
        t.category == DocTermCategory::Datatype
            || t.range
                .iter()
                .any(|r| r.contains("XMLSchema") || r.contains("/xsd") || r.contains("#xsd"))
    });
    let mut s = String::from("AL");
    if has_hierarchy {
        s.push('H');
    }
    if has_datatypes {
        s.push_str("(D)");
    }
    s
}

/// The native-reasoner consistency verdict, rendered honestly per state.
fn consistency_verdict(model: &DocsModel) -> String {
    match &model.reasoning {
        None => "not evaluated".to_string(),
        Some(v) if v.is_consistent && v.unsatisfiable.is_empty() => {
            "consistent (no unsatisfiable classes)".to_string()
        }
        Some(v) if v.is_consistent => {
            format!(
                "consistent; {} unsatisfiable class(es)",
                v.unsatisfiable.len()
            )
        }
        Some(v) => format!(
            "inconsistent; {} unsatisfiable class(es)",
            v.unsatisfiable.len()
        ),
    }
}

/// Competency coverage: `(question_count, terms_covered, coverage_percent)` where
/// coverage is the BOUNDED fraction of documented terms that at least one
/// competency question exercises.
fn competency_coverage(model: &DocsModel) -> (usize, usize, String) {
    let term_iris: BTreeSet<&str> = model.terms.iter().map(|t| t.iri.as_str()).collect();
    let exercised: BTreeSet<&str> = model
        .competencies
        .iter()
        .flat_map(|c| c.exercises.iter().map(String::as_str))
        .collect();
    let covered = exercised.iter().filter(|i| term_iris.contains(**i)).count();
    let pct = if model.terms.is_empty() {
        "0.0%".to_string()
    } else {
        format!("{:.1}%", 100.0 * covered as f64 / model.terms.len() as f64)
    };
    (model.competencies.len(), covered, pct)
}

fn section_methodology(out: &mut String, model: &DocsModel) {
    out.push_str("// <<section:methodology>>\n");
    out.push_str("= Methodology and reproducibility\n\n");
    out.push_str(
        "This documentation is a projection of a single canonical bundle. The \
         ontology's constitutional principles govern every artifact: the `logic:` \
         core is the one canonical reasoning language and every other serialization \
         is a generated, lossy projection of it, each carrying a preservation \
         judgment in the loss ledger.\n\n",
    );
    out.push_str(
        "The build is deterministic end to end. Every collection is emitted in a \
         stable sorted order, no wall-clock time enters any artifact, and each \
         input is content-addressed with BLAKE3 so a byte-identical input always \
         yields a byte-identical output. This PDF itself carries no creation \
         timestamp and a fixed document identifier, so recompiling the same source \
         reproduces the same bytes.\n\n",
    );
    out.push_str(
        "Attestations bind each released bundle to its content address, so a \
         consumer can verify that a downloaded artifact is exactly the one the \
         pipeline produced.\n\n",
    );

    // Provenance chain (Task 12 / issue 1404): this PDF is one projection of the
    // documentation render, itself the product of the build DAG. Render the
    // coarse-grain producing-stage chain walked backward over
    // `gmeow:dataflowConsumes` from `stage-docs-render`, so the PDF carries the
    // same provenance the site footer does. Absent only for a bare model whose
    // catalog has no pipeline (honest absence).
    if let Some(pipeline) = &model.pipeline {
        let chain = provenance_spine(pipeline, "stage-docs-render");
        if !chain.is_empty() {
            let rendered = chain.join(" ← ");
            out.push_str(&format!(
                "This document is a projection of the documentation render, itself a \
                 product of the dogfooded build pipeline. Its provenance chain, walked \
                 backward over the authored `gmeow:dataflowConsumes` dataflow, is: this \
                 document ← {rendered}.\n\n",
            ));
        }
    }
}

/// The coarse-grain provenance chain for the PDF's methodology + DAG figure: the
/// producing-stage path walked BACKWARD over `gmeow:dataflowConsumes` from the
/// stage whose local name is `start_local` (default `stage-docs-render`),
/// following the lexicographically-smallest consumed producer at each step until a
/// source-reading stage is reached. Cycle-safe; returns stage local names in
/// consumer→producer order. Mirrors `gmeow_docs`'s site-footer walk so the PDF and
/// the HTML site report the SAME provenance chain.
fn provenance_spine(pipeline: &gmeow_docs::model::DocPipeline, start_local: &str) -> Vec<String> {
    use std::collections::{BTreeMap, BTreeSet};
    let by_iri: BTreeMap<&str, &gmeow_docs::model::DocStage> = pipeline
        .stages
        .iter()
        .map(|s| (s.iri.as_str(), s))
        .collect();
    let Some(mut current) = pipeline
        .stages
        .iter()
        .find(|s| stage_local_name(&s.iri) == start_local)
    else {
        return Vec::new();
    };
    let mut chain = vec![stage_local_name(&current.iri).to_string()];
    let mut visited: BTreeSet<&str> = BTreeSet::new();
    visited.insert(current.iri.as_str());
    // Owned `next_iri` so the borrow of `current.consumes` ends before `current`
    // is reassigned in the body.
    while let Some(next_iri) = current
        .consumes
        .iter()
        .filter(|p| !visited.contains(p.as_str()))
        .min()
        .cloned()
    {
        let Some(next) = by_iri.get(next_iri.as_str()) else {
            break;
        };
        chain.push(stage_local_name(&next.iri).to_string());
        visited.insert(next.iri.as_str());
        current = *next;
    }
    chain
}

/// The local name of a stage IRI: the tail after the last `/` or `#`.
fn stage_local_name(iri: &str) -> &str {
    let cut = iri.rfind(['/', '#']).map(|i| i + 1).unwrap_or(0);
    &iri[cut..]
}

fn section_fair(out: &mut String) {
    out.push_str("// <<section:fair>>\n");
    out.push_str("= FAIR statement\n\n");
    out.push_str(&format!(
        "The ontology and its documentation are Findable, Accessible, \
         Interoperable, and Reusable. Findability and machine-checkable metadata \
         completeness are enforced by the `{FAIR_GATE}` gate, which fails the build \
         if the required descriptive metadata is absent. Accessibility follows from \
         the openly licensed, self-contained bundle; interoperability from the \
         standard RDF, OWL, SHACL, and SSSOM projections; and reusability from the \
         per-term provenance, versioning, and the loss ledger that documents every \
         projection's declared losses.\n\n",
    ));
}

// ── Framework-comparison tables ─────────────────────────────────────────────

fn section_comparisons(out: &mut String, model: &DocsModel) {
    comparison_table(out, "comparison-gufo", "gUFO", "gufo", model);
    comparison_table(out, "comparison-bfo", "BFO", "bfo", model);
    comparison_table(out, "comparison-dolce", "DOLCE", "dolce", model);
}

/// A framework-comparison table. Rows are derived from the model's linkages whose
/// external object IRI mentions `needle`; if the model carries no such
/// correspondence, a defined non-empty baseline row is emitted so the table is
/// never empty.
fn comparison_table(out: &mut String, marker: &str, label: &str, needle: &str, model: &DocsModel) {
    out.push_str(&format!("// <<section:{marker}>>\n"));
    out.push_str(&format!("= {label} comparison\n\n"));

    let mut rows: Vec<(String, String, String)> = Vec::new();
    for link in &model.linkages {
        if link.object.to_ascii_lowercase().contains(needle) {
            rows.push((
                link.subject_curie.clone(),
                local_predicate(&link.predicate),
                link.object.clone(),
            ));
        }
    }
    rows.sort();
    rows.dedup();
    if rows.is_empty() {
        // A defined baseline: GMEOW's top-level relationship to the framework,
        // stated as prose rows so the table is always present and non-empty.
        rows.push((
            "gmeow:".to_string(),
            "relatesTo".to_string(),
            format!("{label} (no direct term correspondences in this bundle)"),
        ));
    }

    // Cells are CODE arguments of `#table(...)`, so bare string expressions.
    out.push_str("#table(\n  columns: 3,\n  align: (left, left, left),\n");
    out.push_str(&format!(
        "  table.header({}, {}, {}),\n",
        tstr("GMEOW term"),
        tstr("Predicate"),
        tstr(&format!("{label} term"))
    ));
    for (s, p, o) in rows {
        out.push_str(&format!("  {}, {}, {},\n", tstr(&s), tstr(&p), tstr(&o)));
    }
    out.push_str(")\n\n");
}

/// The local name of a predicate IRI/CURIE (the segment after the last `/`, `#`,
/// or `:`), for compact table cells.
fn local_predicate(pred: &str) -> String {
    pred.rsplit(['/', '#', ':'])
        .next()
        .filter(|s| !s.is_empty())
        .unwrap_or(pred)
        .to_string()
}

// ── Pipeline-DAG figure ─────────────────────────────────────────────────────

/// The canonical build-pipeline stage sequence rendered as a deterministic boxed
/// figure. These are text/box stages, NOT the rich SVG diagrams the loss table
/// declares dropped (search / four-boxes / dependency SVGs); a simple labelled
/// flow is not one of those interactive diagram surfaces.
fn section_pipeline_dag(out: &mut String, model: &DocsModel) {
    out.push_str("// <<section:pipeline-dag>>\n");
    out.push_str("= Pipeline DAG\n\n");

    // The genuine authored build DAG (`model.pipeline`, from
    // `slices/core/pipeline/module.ttl`). The figure is the deterministic
    // provenance SPINE (source-reading root → … → the narrow-waist sink) computed
    // from the real dataflow, NOT a hand-written sketch. A model without a
    // pipeline falls back to the conceptual spine so the figure is never empty.
    let stages: Vec<String> = match &model.pipeline {
        Some(pipeline) => {
            // The provenance chain runs consumer→producer from the sink; reverse it
            // so the figure reads left-to-right in build (producer→consumer) order.
            let mut spine = provenance_spine(pipeline, "stage-gts-sink");
            spine.reverse();
            if spine.is_empty() {
                Vec::new()
            } else {
                out.push_str(&format!(
                    "The dogfooded build graph is authored as data: {} typed \
                     `gmeow:PipelineStage` nodes wired by the `gmeow:dataflowConsumes` \
                     dataflow, with exactly one narrow-waist `gmeow:sinkCapability` \
                     serialization exit. The figure shows the provenance spine from a \
                     source-reading root to that sink.\n\n",
                    pipeline.stages.len(),
                ));
                spine
            }
        }
        None => Vec::new(),
    };
    let stages: Vec<String> = if stages.is_empty() {
        ["slices", "compose", "reason", "project", "gmeow.gts"]
            .iter()
            .map(|s| (*s).to_string())
            .collect()
    } else {
        stages
    };

    out.push_str("#figure(\n  caption: [The regeneration pipeline spine.],\n");
    out.push_str("  grid(\n    columns: (auto,) * ");
    out.push_str(&(stages.len() * 2 - 1).to_string());
    out.push_str(",\n    align: horizon,\n    gutter: 0.4em,\n");
    for (i, stage) in stages.iter().enumerate() {
        if i > 0 {
            out.push_str("    [→],\n");
        }
        out.push_str(&format!(
            "    box(stroke: 0.5pt, inset: 6pt, radius: 3pt)[{}],\n",
            disp(stage)
        ));
    }
    out.push_str("  ),\n)\n\n");
}

// ── Loss-ledger appendix (sourced from the shared table) ────────────────────

fn section_loss_appendix(out: &mut String, losses: &[FormatCapabilities]) {
    out.push_str("// <<section:loss-appendix>>\n");
    out.push_str("= Loss ledger appendix\n\n");
    out.push_str(
        "This PDF is a lossy projection. The capabilities it declares lost are read \
         directly from the shared per-format capability table, so this appendix \
         matches the ontology's loss ledger by construction.\n\n",
    );

    let pdf = losses.iter().find(|c| c.format == DocFormat::Pdf);
    match pdf {
        None => {
            out.push_str("_No capability partition was supplied for the PDF format._\n\n");
        }
        Some(caps) => {
            out.push_str("== Declared losses\n\n");
            if caps.dropped.is_empty() {
                out.push_str("_This format declares no capability losses._\n\n");
            } else {
                for cap in &caps.dropped {
                    out.push_str(&format!(
                        "- {} ({})\n",
                        disp(capability_label(*cap)),
                        disp(cap.slug())
                    ));
                }
                out.push('\n');
            }
            out.push_str("== Represented capabilities\n\n");
            if caps.representable.is_empty() {
                out.push_str("_This format represents none of the tracked capabilities._\n\n");
            } else {
                for cap in &caps.representable {
                    out.push_str(&format!(
                        "- {} ({})\n",
                        disp(capability_label(*cap)),
                        disp(cap.slug())
                    ));
                }
                out.push('\n');
            }
        }
    }
}

/// A human label for a capability (the appendix's readable column).
fn capability_label(cap: Capability) -> &'static str {
    match cap {
        Capability::SearchIndex => "Full-text search index",
        Capability::LiveSparql => "Live SPARQL queries",
        Capability::Interactivity => "Interactive surfaces",
        Capability::LiveReasoning => "In-browser reasoning + GMN transcode",
        Capability::Diagrams => "Rendered diagrams",
        Capability::CrossLinkFidelity => "Cross-link fidelity",
    }
}

// ── Bibliography ────────────────────────────────────────────────────────────

/// Emit the bibliography over `references.bib` — but ONLY when `bib` is
/// non-empty. An empty database omits the section entirely, so an empty bib
/// yields a valid document rather than a Typst error over a zero-entry file.
fn bibliography(out: &mut String, bib: &[u8]) {
    if bib.is_empty() {
        return;
    }
    out.push_str("// <<section:bibliography>>\n");
    out.push_str("= Bibliography\n\n");
    out.push_str(&format!(
        "#bibliography(\"{}\", full: true, title: none)\n",
        crate::world::BIB_PATH
    ));
}
