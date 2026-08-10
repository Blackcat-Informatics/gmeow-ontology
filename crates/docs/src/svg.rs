// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Deterministic SVG diagrams for the documentation model.
//!
//! No JavaScript, no Docker, no network, no PyO3 — every function is a pure,
//! byte-reproducible function of [`DocsModel`]. The node-link *graph* diagrams
//! ([`slice_dependency_svg`], [`slice_local_svg`], [`term_neighbourhood_svg`]) are
//! projected into gmeow's own shipped RDF-graph renderer (`purrdf::viz`) over real
//! ontology predicate IRIs — deterministic across processes by construction
//! (IRI-sorted input, BTree/FNV ordering, no hashing). The *chart* diagrams, which
//! are not node-link graphs, stay hand-emitted: [`concern_overview_svg`] is a
//! horizontal bar chart of concerns by term count, [`coverage_heatmap_svg`] a
//! per-slice coverage matrix, and [`pipeline_dag_svg`] a capability-coloured build
//! DAG with flow-entity edge labels. Determinism in those is structural: every
//! coordinate derives from the sorted index, and every label is XML-escaped.

use std::collections::BTreeSet;

use purrdf::TermValue;
use purrdf::viz::{
    VizGraphInput, VizInputQuad, VizRenderOptions, VizSpec, VizSvgOptions, render_graph_input_svg,
};

use crate::model::{DocTerm, DocsModel};

/// The local name of an IRI: the tail after the last `/` or `#`.
fn local_name(iri: &str) -> &str {
    let cut = iri.rfind(['/', '#']).map(|i| i + 1).unwrap_or(0);
    &iri[cut..]
}

/// XML-escape a string for safe inclusion in SVG text / attribute content.
fn xml_escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            c => out.push(c),
        }
    }
    out
}

// Predicate / class IRIs used to project the documentation model's graph diagrams
// into `purrdf::viz`. These are the real ontology IRIs the model itself reads
// (`crates/docs/src/model.rs`), so each diagram depicts the same relations the RDF
// projection ships (dogfooding; single source of truth). `related_terms` is a union
// of `skos:related` / `gmeow:pairsWith` / `rdfs:seeAlso` in the model, of which
// `skos:related` is the representative label.
const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
// BENIGN: labels an edge over the already-resolved `term.parents` (describe.rs
// already merged both subsumption spellings before this renders) — not a second
// authored-surface scan, so the `rdfs:` label alone is correct here.
const RDFS_SUBCLASS_OF: &str = "http://www.w3.org/2000/01/rdf-schema#subClassOf";
const RDFS_DOMAIN: &str = "http://www.w3.org/2000/01/rdf-schema#domain";
const RDFS_RANGE: &str = "http://www.w3.org/2000/01/rdf-schema#range";
const RDFS_RESOURCE: &str = "http://www.w3.org/2000/01/rdf-schema#Resource";
const SKOS_RELATED: &str = "http://www.w3.org/2004/02/skos/core#related";
const LOGIC_FORMALIZES: &str = "https://blackcatinformatics.ca/logic/formalizes";
const GMEOW_SLICE_DEPENDS_ON: &str = "https://blackcatinformatics.ca/gmeow/sliceDependsOn";
const GMEOW_SLICE_CLASS: &str = "https://blackcatinformatics.ca/gmeow/Slice";

/// Render a directed node-link graph to a deterministic SVG via `purrdf::viz` — the
/// shipped, dogfooded RDF-graph renderer.
///
/// `edges` are `(subject, predicate, object)` IRI triples; `nodes` are IRIs that
/// must appear even with no incident edge (each such orphan gets an `rdf:type
/// <class_iri>` marker quad, since purrdf only materialises terms that occur in a
/// statement). Determinism is guaranteed across processes: the input is derived from
/// IRI-sorted model data, orphan detection uses a [`BTreeSet`] (never a `HashSet`,
/// whose iteration order leaks the per-process hash seed), and purrdf itself orders
/// by BTree/FNV content hash. `embed_metadata` is off to keep the per-term/per-slice
/// SVGs lean (no embedded VizExport JSON); `include_styles` is on because the SVGs
/// are `<img>`-embedded, so the site's external CSS never reaches them. Hard-fails on
/// a [`purrdf::viz::VizError`] (message names the diagram) — never a placeholder.
fn graph_svg(
    edges: Vec<(String, String, String)>,
    nodes: &[&str],
    class_iri: &str,
    title: &str,
) -> String {
    let iri = |s: &str| TermValue::Iri(s.to_string());
    let mut quads: Vec<VizInputQuad> = edges
        .into_iter()
        .map(|(s, p, o)| VizInputQuad {
            subject: TermValue::Iri(s),
            predicate: p,
            object: TermValue::Iri(o),
            graph_name: None,
        })
        .collect();
    // Any node not incident to an edge would not be projected — emit an `rdf:type`
    // marker so it still renders (deterministic: `nodes` is IRI-sorted by callers).
    // `edges` was consumed above, so orphan detection borrows the already-built
    // `quads` instead; the marker quads are staged into a temporary `Vec` so the
    // `present` borrow ends before `quads` is mutated.
    let present: BTreeSet<&str> = quads
        .iter()
        .flat_map(|q| match (&q.subject, &q.object) {
            (TermValue::Iri(s), TermValue::Iri(o)) => [s.as_str(), o.as_str()],
            _ => unreachable!("graph_svg only builds IRI subject/object quads"),
        })
        .collect();
    let orphans: Vec<VizInputQuad> = nodes
        .iter()
        .filter(|node| !present.contains(*node))
        .map(|node| VizInputQuad {
            subject: iri(node),
            predicate: RDF_TYPE.to_string(),
            object: iri(class_iri),
            graph_name: None,
        })
        .collect();
    quads.extend(orphans);

    let input = VizGraphInput {
        quads,
        ..Default::default()
    };
    let spec = VizSpec {
        max_statements: usize::MAX,
        max_terms: usize::MAX,
        ..Default::default()
    };
    let options = VizRenderOptions {
        svg: VizSvgOptions {
            embed_metadata: false,
            include_styles: true,
            title: title.to_string(),
        },
        ..Default::default()
    };
    render_graph_input_svg(&input, &spec, &options)
        .unwrap_or_else(|e| panic!("purrdf viz render failed for {title}: {e}"))
        .svg
}

/// Render the slice dependency DAG as a deterministic `purrdf::viz` node-link graph.
///
/// Edges are `model.dependency_edges` projected as `from gmeow:sliceDependsOn to`
/// (self-loops dropped); nodes are every slice in `model.slices` (IRI-sorted), so a
/// slice with no dependency edges still renders as a node.
pub fn slice_dependency_svg(model: &DocsModel) -> String {
    let edges: Vec<(String, String, String)> = model
        .dependency_edges
        .iter()
        .filter(|e| e.from != e.to)
        .map(|e| {
            (
                e.from.clone(),
                GMEOW_SLICE_DEPENDS_ON.to_string(),
                e.to.clone(),
            )
        })
        .collect();
    let nodes: Vec<&str> = model.slices.iter().map(|s| s.iri.as_str()).collect();
    graph_svg(edges, &nodes, GMEOW_SLICE_CLASS, "Slice dependency graph")
}

/// Render the dogfooded build-pipeline DAG (`model.pipeline`) as a deterministic
/// grid-layout SVG, modeled exactly on [`slice_dependency_svg`].
///
/// Nodes are the `gmeow:PipelineStage` individuals (already IRI-sorted in the
/// model), placed in a fixed grid; every coordinate derives from the sorted index
/// (integers only — no floats / locale formatting), so the bytes are reproducible.
/// Nodes are colored by a small deterministic capability/resource map:
/// the single `gmeow:sinkCapability` holder (`stage-gts-sink`) is highlighted as
/// the narrow-waist exit, the `gmeow:sourceOrigin` loader and any
/// resource-holding stage each get a distinct fill, and everything else is a
/// plain transform node. Edges are drawn from `model.pipeline.edges`; an edge that
/// carries reified `gmeow:flowEntity` graphs is labelled with their local names
/// (a missing label is honest computed-absence, never a placeholder). A model
/// with no pipeline renders a valid empty diagram.
pub fn pipeline_dag_svg(model: &DocsModel) -> String {
    let Some(pipeline) = &model.pipeline else {
        let mut out = String::new();
        svg_open(&mut out, 320, 80, "Build pipeline DAG");
        out.push_str(
            "  <text x=\"160\" y=\"44\" text-anchor=\"middle\" font-family=\"sans-serif\" \
             font-size=\"13\" fill=\"#1b2436\">no pipeline authored</text>\n",
        );
        out.push_str("</svg>\n");
        return out;
    };

    let nodes: Vec<&str> = pipeline.stages.iter().map(|s| s.iri.as_str()).collect();

    // Grid geometry (mirrors `slice_dependency_svg`; wider cells for stage labels).
    const COLS: usize = 4;
    const BOX_W: i64 = 220;
    const BOX_H: i64 = 44;
    const GAP_X: i64 = 70;
    const GAP_Y: i64 = 56;
    const MARGIN: i64 = 24;
    let cell_w = BOX_W + GAP_X;
    let cell_h = BOX_H + GAP_Y;

    let pos = |i: usize| -> (i64, i64) {
        let col = (i % COLS) as i64;
        let row = (i / COLS) as i64;
        (MARGIN + col * cell_w, MARGIN + row * cell_h)
    };
    let center = |i: usize| -> (i64, i64) {
        let (x, y) = pos(i);
        (x + BOX_W / 2, y + BOX_H / 2)
    };
    let index_of = |iri: &str| -> Option<usize> { nodes.iter().position(|n| *n == iri) };

    let rows = nodes.len().div_ceil(COLS).max(1) as i64;
    let width = MARGIN * 2 + (COLS as i64) * cell_w - GAP_X;
    let height = MARGIN * 2 + rows * cell_h - GAP_Y;

    let mut out = String::new();
    svg_open(&mut out, width, height, "Build pipeline DAG");
    arrow_marker(&mut out);

    // Edges first (drawn under the boxes). `pipeline.edges` is already sorted.
    out.push_str("  <g stroke=\"#7a8aa0\" stroke-width=\"1.5\" fill=\"none\">\n");
    for edge in &pipeline.edges {
        let (Some(fi), Some(ti)) = (index_of(&edge.from), index_of(&edge.to)) else {
            continue;
        };
        if fi == ti {
            continue;
        }
        let (x1, y1) = center(fi);
        let (x2, y2) = center(ti);
        out.push_str(&format!(
            "    <line x1=\"{x1}\" y1=\"{y1}\" x2=\"{x2}\" y2=\"{y2}\" marker-end=\"url(#arrow)\" />\n"
        ));
        // Flow-entity label at the edge midpoint (only where a reified edge names
        // the flowing graphs — otherwise nothing, an honest computed-absence).
        if !edge.flow_entities.is_empty() {
            let label = edge
                .flow_entities
                .iter()
                .map(|g| local_name(g))
                .collect::<Vec<_>>()
                .join(" · ");
            out.push_str(&format!(
                "    <text x=\"{mx}\" y=\"{my}\" text-anchor=\"middle\" \
                 font-family=\"sans-serif\" font-size=\"10\" fill=\"#4a5568\">{}</text>\n",
                xml_escape(&label),
                mx = (x1 + x2) / 2,
                my = (y1 + y2) / 2 - 3,
            ));
        }
    }
    out.push_str("  </g>\n");

    // Nodes, colored by capability/resource.
    for (i, stage) in pipeline.stages.iter().enumerate() {
        let (x, y) = pos(i);
        let is_sink = stage
            .capabilities
            .iter()
            .any(|c| c == "gmeow:sinkCapability");
        let is_source = stage.capabilities.iter().any(|c| c == "gmeow:sourceOrigin");
        // (fill, stroke, stroke-width) — the narrow-waist sink is highlighted.
        let (fill, stroke, sw) = if is_sink {
            ("#ffe6b3", "#b8860b", 2)
        } else if is_source {
            ("#d6f5d6", "#2f855a", 1)
        } else if !stage.resources.is_empty() {
            ("#f5d6e6", "#a03060", 1)
        } else {
            ("#eef2f8", "#33425b", 1)
        };
        let label = xml_escape(local_name(&stage.iri));
        out.push_str(&format!(
            "  <g>\n    <rect x=\"{x}\" y=\"{y}\" width=\"{BOX_W}\" height=\"{BOX_H}\" rx=\"6\" \
             fill=\"{fill}\" stroke=\"{stroke}\" stroke-width=\"{sw}\" />\n    \
             <text x=\"{tx}\" y=\"{ty}\" text-anchor=\"middle\" font-family=\"sans-serif\" \
             font-size=\"12\" fill=\"#1b2436\">{label}</text>\n  </g>\n",
            tx = x + BOX_W / 2,
            ty = y + BOX_H / 2 + 4,
        ));
    }

    out.push_str("</svg>\n");
    out
}

/// Render a per-slice local dependency SVG: the slice and its direct neighbours, as
/// a deterministic `purrdf::viz` node-link graph.
///
/// Edges are the `gmeow:sliceDependsOn` edges incident to `slice_iri` in either
/// direction — outgoing (this slice's dependencies) and incoming (its dependents) —
/// so edge direction carries the dep/dependent distinction the old two-column layout
/// encoded positionally. The centre slice is a fixed node, so an isolated slice
/// still renders.
pub fn slice_local_svg(model: &DocsModel, slice_iri: &str) -> String {
    let mut edges: Vec<(String, String, String)> = model
        .dependency_edges
        .iter()
        .filter(|e| e.from != e.to && (e.from == slice_iri || e.to == slice_iri))
        .map(|e| {
            (
                e.from.clone(),
                GMEOW_SLICE_DEPENDS_ON.to_string(),
                e.to.clone(),
            )
        })
        .collect();
    edges.sort();
    edges.dedup();
    graph_svg(
        edges,
        &[slice_iri],
        GMEOW_SLICE_CLASS,
        &format!("Dependencies for slice {}", local_name(slice_iri)),
    )
}

/// Whether the term has at least one neighbour worth drawing.
///
/// Gating both the per-term SVG emission and the term-page embed on this single
/// predicate keeps the two in lockstep: a page never embeds a diagram path that
/// was not emitted (which would trip the no-dangling-link invariant).
pub fn term_has_neighbourhood(term: &DocTerm) -> bool {
    term.parents
        .iter()
        .chain(term.formalized_by.iter())
        .chain(term.related_terms.iter())
        .chain(term.domain.iter())
        .chain(term.range.iter())
        .any(|n| n != &term.iri)
}

/// Render a per-term 1-hop neighbourhood as a deterministic `purrdf::viz` node-link
/// graph, centred on the term.
///
/// Each relation projects an edge from the term with its **real** predicate IRI, so
/// the diagram distinguishes the relation kinds the old two-flank layout collapsed:
/// `parents` → `rdfs:subClassOf`, `formalized_by` → `logic:formalizes`,
/// `related_terms` → `skos:related`, `domain` → `rdfs:domain`, `range` →
/// `rdfs:range`. Self-references are dropped. Emitted only for terms that
/// [`term_has_neighbourhood`], so the centre term is always incident to an edge.
pub fn term_neighbourhood_svg(term: &DocTerm) -> String {
    let mut edges: Vec<(String, String, String)> = Vec::new();
    let mut project = |predicate: &str, targets: &[String]| {
        for target in targets {
            if target != &term.iri {
                edges.push((term.iri.clone(), predicate.to_string(), target.clone()));
            }
        }
    };
    project(RDFS_SUBCLASS_OF, &term.parents);
    project(LOGIC_FORMALIZES, &term.formalized_by);
    project(SKOS_RELATED, &term.related_terms);
    project(RDFS_DOMAIN, &term.domain);
    project(RDFS_RANGE, &term.range);
    edges.sort();
    edges.dedup();
    graph_svg(
        edges,
        &[term.iri.as_str()],
        RDFS_RESOURCE,
        &format!("Neighbourhood for term {}", local_name(&term.iri)),
    )
}

/// Render an overview bar chart of concerns by their term count.
///
/// Concerns are sorted by descending term count, then IRI (deterministic). Each
/// concern is a horizontal bar whose width scales to the largest term count.
pub fn concern_overview_svg(model: &DocsModel) -> String {
    let mut concerns: Vec<(&str, usize)> = model
        .concerns
        .iter()
        .map(|c| {
            (
                c.label.as_deref().unwrap_or_else(|| local_name(&c.iri)),
                c.terms.len(),
            )
        })
        .collect();
    // Stable sort: by descending count, then by label.
    concerns.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(b.0)));

    const MARGIN: i64 = 24;
    const ROW_H: i64 = 28;
    const LABEL_W: i64 = 240;
    const BAR_MAX: i64 = 360;
    let max_count = concerns.iter().map(|(_, c)| *c).max().unwrap_or(0).max(1) as i64;

    let rows = concerns.len().max(1) as i64;
    let width = MARGIN * 2 + LABEL_W + BAR_MAX + 60;
    let height = MARGIN * 2 + rows * ROW_H;

    let mut out = String::new();
    svg_open(&mut out, width, height, "Concerns by term count");

    for (i, (label, count)) in concerns.iter().enumerate() {
        let y = MARGIN + i as i64 * ROW_H;
        let bar_w = (BAR_MAX * *count as i64) / max_count;
        let bar_x = MARGIN + LABEL_W;
        let text_y = y + ROW_H / 2 + 4;
        out.push_str(&format!(
            "  <text x=\"{lx}\" y=\"{text_y}\" text-anchor=\"end\" font-family=\"sans-serif\" \
             font-size=\"12\" fill=\"#1b2436\">{label}</text>\n  \
             <rect x=\"{bar_x}\" y=\"{by}\" width=\"{bar_w}\" height=\"{bh}\" rx=\"3\" \
             fill=\"#5b78c0\" />\n  \
             <text x=\"{cx}\" y=\"{text_y}\" font-family=\"sans-serif\" font-size=\"12\" \
             fill=\"#1b2436\">{count}</text>\n",
            lx = bar_x - 8,
            label = xml_escape(label),
            by = y + 4,
            bh = ROW_H - 10,
            cx = bar_x + bar_w + 6,
        ));
    }

    out.push_str("</svg>\n");
    out
}

/// Render a per-slice documentation-coverage heatmap: one row per slice that has
/// terms, one cell per coverage dimension, each cell filled on the shared
/// red/amber/green coverage scale ([`crate::badge::coverage_fraction_color`]) by
/// the fraction of the slice's terms that carry that dimension.
///
/// Deterministic and structural: slices are taken in the model's IRI order and
/// every coordinate derives from that order; every label is XML-escaped.
pub fn coverage_heatmap_svg(model: &DocsModel) -> String {
    use crate::coverage::{DIMENSIONS, TermCoverage};

    // Pure projection: group the emitted per-term `gmeow:docCoversDimension`
    // incidence by owning slice — NEVER a second coverage recompute. Each cell is
    // the count of the slice's documented terms that COVER a dimension, read back
    // from `graph/documentation`.
    let graph = crate::rdf::documentation_graph(model);
    let mut per_slice: std::collections::BTreeMap<String, (usize, [usize; TermCoverage::TOTAL])> =
        std::collections::BTreeMap::new();
    for term in &graph.terms {
        let entry = per_slice
            .entry(term.owner_slice.clone())
            .or_insert((0, [0usize; TermCoverage::TOTAL]));
        entry.0 += 1;
        for (i, dim) in DIMENSIONS.iter().enumerate() {
            if term.covers.contains(dim.dimension.local_name()) {
                entry.1[i] += 1;
            }
        }
    }
    // Slices in the model's IRI order, keeping only those that own documented terms
    // (matching the prior behaviour; the projection carries only owned terms).
    let mut rows: Vec<(String, usize, [usize; TermCoverage::TOTAL])> = Vec::new();
    for slice in &model.slices {
        if let Some((n, covered)) = per_slice.get(&slice.iri) {
            rows.push((local_name(&slice.iri).to_string(), *n, *covered));
        }
    }

    const LABEL_W: i64 = 220;
    const CELL_W: i64 = 92;
    const CELL_H: i64 = 22;
    const MARGIN: i64 = 24;
    const HEADER_H: i64 = 24;
    let cols = TermCoverage::TOTAL as i64;
    let width = MARGIN * 2 + LABEL_W + cols * CELL_W;
    let height = MARGIN * 2 + HEADER_H + rows.len().max(1) as i64 * CELL_H;

    let mut out = String::new();
    svg_open(&mut out, width, height, "Documentation coverage by slice");

    // Header: the dimension labels above each column.
    for (c, dim) in DIMENSIONS.iter().enumerate() {
        let cx = MARGIN + LABEL_W + c as i64 * CELL_W + CELL_W / 2;
        out.push_str(&format!(
            "  <text x=\"{cx}\" y=\"{hy}\" text-anchor=\"middle\" font-family=\"sans-serif\" \
             font-size=\"11\" fill=\"#1b2436\">{}</text>\n",
            xml_escape(dim.label),
            hy = MARGIN + HEADER_H - 8,
        ));
    }

    // One row per slice: the slice name then a colored, percentage-labelled cell
    // per coverage dimension.
    for (r, (label, n, covered)) in rows.iter().enumerate() {
        let y = MARGIN + HEADER_H + r as i64 * CELL_H;
        out.push_str(&format!(
            "  <text x=\"{lx}\" y=\"{ty}\" text-anchor=\"end\" font-family=\"sans-serif\" \
             font-size=\"11\" fill=\"#1b2436\">{}</text>\n",
            xml_escape(label),
            lx = MARGIN + LABEL_W - 6,
            ty = y + CELL_H / 2 + 4,
        ));
        for (c, cov) in covered.iter().enumerate() {
            let x = MARGIN + LABEL_W + c as i64 * CELL_W;
            let fill = crate::badge::coverage_fraction_color(*cov, *n);
            let text = crate::badge::text_color_for(fill);
            let pct = cov * 100 / *n;
            out.push_str(&format!(
                "  <rect x=\"{x}\" y=\"{y}\" width=\"{CELL_W}\" height=\"{CELL_H}\" fill=\"{fill}\" \
                 stroke=\"#ffffff\" stroke-width=\"1\" />\n  \
                 <text x=\"{cx}\" y=\"{ty}\" text-anchor=\"middle\" font-family=\"sans-serif\" \
                 font-size=\"10\" fill=\"{text}\">{pct}%</text>\n",
                cx = x + CELL_W / 2,
                ty = y + CELL_H / 2 + 4,
            ));
        }
    }

    out.push_str("</svg>\n");
    out
}

/// Open an SVG document with a fixed viewport and an accessible title.
fn svg_open(out: &mut String, width: i64, height: i64, title: &str) {
    out.push_str(&format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{width}\" height=\"{height}\" \
         viewBox=\"0 0 {width} {height}\" role=\"img\" aria-label=\"{}\">\n",
        xml_escape(title)
    ));
    out.push_str(&format!("  <title>{}</title>\n", xml_escape(title)));
    out.push_str(&format!(
        "  <rect width=\"{width}\" height=\"{height}\" fill=\"#ffffff\" />\n"
    ));
}

/// Emit a reusable arrowhead marker definition.
fn arrow_marker(out: &mut String) {
    out.push_str(
        "  <defs>\n    <marker id=\"arrow\" viewBox=\"0 0 10 10\" refX=\"9\" refY=\"5\" \
         markerWidth=\"7\" markerHeight=\"7\" orient=\"auto-start-reverse\">\n      \
         <path d=\"M 0 0 L 10 5 L 0 10 z\" fill=\"#7a8aa0\" />\n    </marker>\n  </defs>\n",
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn xml_escape_handles_metacharacters() {
        assert_eq!(xml_escape("a<b>&\"'"), "a&lt;b&gt;&amp;&quot;&apos;");
    }

    #[test]
    fn local_name_takes_tail() {
        assert_eq!(local_name("https://x/y/Foo"), "Foo");
        assert_eq!(local_name("https://x#Bar"), "Bar");
    }

    fn term_with_neighbours() -> DocTerm {
        DocTerm {
            iri: "https://x/y/Centre".to_string(),
            parents: vec!["https://x/y/Parent".to_string()],
            related_terms: vec!["https://x/y/Related".to_string()],
            // self-references must be filtered out of every flank
            domain: vec!["https://x/y/Centre".to_string()],
            ..Default::default()
        }
    }

    #[test]
    fn term_has_neighbourhood_gates_on_any_non_self_relation() {
        let term = term_with_neighbours();
        assert!(term_has_neighbourhood(&term));
        // A term whose only relation is a self-reference has no neighbourhood.
        let self_only = DocTerm {
            iri: "https://x/y/Centre".to_string(),
            domain: vec!["https://x/y/Centre".to_string()],
            ..Default::default()
        };
        assert!(!term_has_neighbourhood(&self_only));
        assert!(!term_has_neighbourhood(&DocTerm::default()));
    }

    #[test]
    fn term_neighbourhood_svg_is_pure_and_labels_nodes() {
        let term = term_with_neighbours();
        let svg = term_neighbourhood_svg(&term);
        // Centre and both non-self neighbours are present, by local name (the self
        // reference in `domain` is dropped). purrdf renders labels as node text.
        assert!(svg.contains("Centre"));
        assert!(svg.contains("Parent"));
        assert!(svg.contains("Related"));
        // Pure: identical bytes across two calls (no per-process hash-seed drift).
        assert_eq!(svg, term_neighbourhood_svg(&term));
    }
}
