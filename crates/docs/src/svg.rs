// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Deterministic, hand-emitted SVG diagrams for the documentation model.
//!
//! No JavaScript, no Docker, no network, no PyO3 — every function is a pure,
//! byte-reproducible function of [`DocsModel`]. Determinism is structural: nodes
//! are sorted by IRI, and ALL coordinates are derived from that sorted order
//! (never from randomness or hashing). Every label is XML-escaped.
//!
//! - [`slice_dependency_svg`] lays the slice dependency DAG out in a simple grid
//!   and draws the cross-slice edges.
//! - [`concern_overview_svg`] draws a horizontal bar chart of concerns by the
//!   number of terms that declare each.
//! - [`term_neighbourhood_svg`] draws a single term's 1-hop neighbourhood — the
//!   per-term analogue of [`slice_local_svg`].

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

/// A deterministic placeholder emitted in place of a rendered diagram while
/// diagram SVG generation is DEFERRED pending purrdf's high-quality SVG graph
/// library. The hand-rolled renderers above carry a latent cross-process
/// ordering non-determinism (two renders of the same model can differ byte-wise);
/// rather than chase it, the emit sites route through this constant-shape
/// placeholder so the site render is byte-stable. AUTHORIZED DEFERRAL (paudley) —
/// restore the `*_svg` calls at the emit sites when the purrdf SVG lib lands.
/// Pure function of its title (which the callers derive from sorted model data).
pub fn deferred_diagram_svg(title: &str) -> String {
    format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"480\" height=\"72\" role=\"img\" \
         aria-label=\"{t}\">\n  \
         <rect width=\"480\" height=\"72\" fill=\"#f5f6f8\" stroke=\"#d0d5dd\" />\n  \
         <text x=\"240\" y=\"40\" text-anchor=\"middle\" font-family=\"sans-serif\" \
         font-size=\"13\" fill=\"#6b7280\">{t} — diagram pending</text>\n</svg>\n",
        t = xml_escape(title)
    )
}

/// Render the slice dependency DAG as a deterministic grid-layout SVG.
///
/// Nodes are the slices referenced by `model.dependency_edges` (plus every slice
/// in `model.slices`), sorted by IRI and placed left-to-right, top-to-bottom in a
/// fixed-width grid. Edges are drawn as straight lines between node centers. A
/// model with no dependency edges still renders every slice as a node.
pub fn slice_dependency_svg(model: &DocsModel) -> String {
    // Node set: all slices, by IRI (already IRI-sorted in the model).
    let nodes: Vec<&str> = model.slices.iter().map(|s| s.iri.as_str()).collect();

    // Grid geometry.
    const COLS: usize = 4;
    const BOX_W: i64 = 220;
    const BOX_H: i64 = 44;
    const GAP_X: i64 = 60;
    const GAP_Y: i64 = 48;
    const MARGIN: i64 = 24;
    let cell_w = BOX_W + GAP_X;
    let cell_h = BOX_H + GAP_Y;

    let pos = |i: usize| -> (i64, i64) {
        let col = (i % COLS) as i64;
        let row = (i / COLS) as i64;
        let x = MARGIN + col * cell_w;
        let y = MARGIN + row * cell_h;
        (x, y)
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
    svg_open(&mut out, width, height, "Slice dependency graph");
    arrow_marker(&mut out);

    // Edges first (drawn under the boxes). Deterministic: model edges are sorted.
    out.push_str("  <g stroke=\"#7a8aa0\" stroke-width=\"1.5\" fill=\"none\">\n");
    for edge in &model.dependency_edges {
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
    }
    out.push_str("  </g>\n");

    // Nodes.
    for (i, iri) in nodes.iter().enumerate() {
        let (x, y) = pos(i);
        let label = xml_escape(local_name(iri));
        out.push_str(&format!(
            "  <g>\n    <rect x=\"{x}\" y=\"{y}\" width=\"{BOX_W}\" height=\"{BOX_H}\" rx=\"6\" \
             fill=\"#eef2f8\" stroke=\"#33425b\" stroke-width=\"1\" />\n    \
             <text x=\"{tx}\" y=\"{ty}\" text-anchor=\"middle\" font-family=\"sans-serif\" \
             font-size=\"13\" fill=\"#1b2436\">{label}</text>\n  </g>\n",
            tx = x + BOX_W / 2,
            ty = y + BOX_H / 2 + 4,
        ));
    }

    out.push_str("</svg>\n");
    out
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

/// Render a per-slice local dependency SVG: the slice and its direct neighbours.
pub fn slice_local_svg(model: &DocsModel, slice_iri: &str) -> String {
    // Collect direct dependencies (out) and dependents (in), sorted+deduped.
    let mut deps: Vec<&str> = model
        .dependency_edges
        .iter()
        .filter(|e| e.from == slice_iri)
        .map(|e| e.to.as_str())
        .collect();
    deps.sort_unstable();
    deps.dedup();
    let mut dependents: Vec<&str> = model
        .dependency_edges
        .iter()
        .filter(|e| e.to == slice_iri)
        .map(|e| e.from.as_str())
        .collect();
    dependents.sort_unstable();
    dependents.dedup();

    const BOX_W: i64 = 220;
    const BOX_H: i64 = 40;
    const GAP_Y: i64 = 18;
    const MARGIN: i64 = 24;
    const COL_X: [i64; 3] = [MARGIN, MARGIN + 300, MARGIN + 600];
    let cell = BOX_H + GAP_Y;

    let rows = dependents.len().max(deps.len()).max(1) as i64;
    let height = MARGIN * 2 + rows * cell;
    let width = COL_X[2] + BOX_W + MARGIN;

    let mut out = String::new();
    svg_open(&mut out, width, height, "Local slice dependencies");

    let node = |out: &mut String, x: i64, y: i64, iri: &str, fill: &str| {
        let label = xml_escape(local_name(iri));
        out.push_str(&format!(
            "  <g>\n    <rect x=\"{x}\" y=\"{y}\" width=\"{BOX_W}\" height=\"{BOX_H}\" rx=\"6\" \
             fill=\"{fill}\" stroke=\"#33425b\" stroke-width=\"1\" />\n    \
             <text x=\"{tx}\" y=\"{ty}\" text-anchor=\"middle\" font-family=\"sans-serif\" \
             font-size=\"13\" fill=\"#1b2436\">{label}</text>\n  </g>\n",
            tx = x + BOX_W / 2,
            ty = y + BOX_H / 2 + 4,
        ));
    };

    // Centre node.
    let centre_y = MARGIN + (rows - 1) * cell / 2;
    node(&mut out, COL_X[1], centre_y, slice_iri, "#dfe9ff");
    for (i, dep) in dependents.iter().enumerate() {
        node(&mut out, COL_X[0], MARGIN + i as i64 * cell, dep, "#eef2f8");
    }
    for (i, dep) in deps.iter().enumerate() {
        node(&mut out, COL_X[2], MARGIN + i as i64 * cell, dep, "#eef2f8");
    }

    out.push_str("</svg>\n");
    out
}

/// A term's 1-hop neighbourhood, split into two flanks and each sorted+deduped
/// with the term's own IRI removed.
///
/// - `up` — the broader / formalizing side: `parents` (super-classes /
///   super-properties) and `formalized_by` (logic terms that formalize this one).
/// - `out` — the associated / typed side: `related_terms`, `domain`, and `range`.
///
/// A pure function of the term's own stored fields (already pre-sorted IRI
/// vectors), so no model lookup or edge walk is needed — this is structurally
/// simpler than [`slice_local_svg`], which must reverse-filter the edge set.
pub fn term_neighbours(term: &DocTerm) -> (Vec<&str>, Vec<&str>) {
    let mut up: Vec<&str> = term
        .parents
        .iter()
        .chain(term.formalized_by.iter())
        .map(String::as_str)
        .filter(|n| *n != term.iri)
        .collect();
    up.sort_unstable();
    up.dedup();

    let mut out: Vec<&str> = term
        .related_terms
        .iter()
        .chain(term.domain.iter())
        .chain(term.range.iter())
        .map(String::as_str)
        .filter(|n| *n != term.iri)
        .collect();
    out.sort_unstable();
    out.dedup();

    (up, out)
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

/// Render a per-term neighbourhood SVG: the term (centre column) flanked by its
/// 1-hop relations — `up` on the left, `out` on the right.
///
/// Deterministic and structural in exactly the same way as [`slice_local_svg`]:
/// neighbours are sorted, every coordinate is derived from the sorted index, and
/// every label is XML-escaped. A pure function of the term.
pub fn term_neighbourhood_svg(term: &DocTerm) -> String {
    let (up, out) = term_neighbours(term);

    const BOX_W: i64 = 220;
    const BOX_H: i64 = 40;
    const GAP_Y: i64 = 18;
    const MARGIN: i64 = 24;
    const COL_X: [i64; 3] = [MARGIN, MARGIN + 300, MARGIN + 600];
    let cell = BOX_H + GAP_Y;

    let rows = up.len().max(out.len()).max(1) as i64;
    let height = MARGIN * 2 + rows * cell;
    let width = COL_X[2] + BOX_W + MARGIN;

    let mut svg = String::new();
    svg_open(&mut svg, width, height, "Term neighbourhood");

    let node = |out: &mut String, x: i64, y: i64, iri: &str, fill: &str| {
        let label = xml_escape(local_name(iri));
        out.push_str(&format!(
            "  <g>\n    <rect x=\"{x}\" y=\"{y}\" width=\"{BOX_W}\" height=\"{BOX_H}\" rx=\"6\" \
             fill=\"{fill}\" stroke=\"#33425b\" stroke-width=\"1\" />\n    \
             <text x=\"{tx}\" y=\"{ty}\" text-anchor=\"middle\" font-family=\"sans-serif\" \
             font-size=\"13\" fill=\"#1b2436\">{label}</text>\n  </g>\n",
            tx = x + BOX_W / 2,
            ty = y + BOX_H / 2 + 4,
        ));
    };

    // Centre node, vertically centred across the available rows.
    let centre_y = MARGIN + (rows - 1) * cell / 2;
    node(&mut svg, COL_X[1], centre_y, &term.iri, "#dfe9ff");
    for (i, n) in up.iter().enumerate() {
        node(&mut svg, COL_X[0], MARGIN + i as i64 * cell, n, "#eef2f8");
    }
    for (i, n) in out.iter().enumerate() {
        node(&mut svg, COL_X[2], MARGIN + i as i64 * cell, n, "#eef2f8");
    }

    svg.push_str("</svg>\n");
    svg
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
    use crate::coverage::{DIMENSIONS, TermCoverage, alignment_subjects, term_coverage};

    let aligned = alignment_subjects(model);
    let mut rows: Vec<(String, usize, [usize; TermCoverage::TOTAL])> = Vec::new();
    for slice in &model.slices {
        let terms: Vec<&DocTerm> = model
            .terms
            .iter()
            .filter(|t| t.owner_slice == slice.iri)
            .collect();
        if terms.is_empty() {
            continue;
        }
        let mut covered = [0usize; TermCoverage::TOTAL];
        for t in &terms {
            for (i, present) in term_coverage(t, &aligned).flags().iter().enumerate() {
                if *present {
                    covered[i] += 1;
                }
            }
        }
        rows.push((local_name(&slice.iri).to_string(), terms.len(), covered));
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
    fn term_neighbours_split_sort_and_drop_self() {
        let term = term_with_neighbours();
        let (up, out) = term_neighbours(&term);
        assert_eq!(up, vec!["https://x/y/Parent"]);
        assert_eq!(out, vec!["https://x/y/Related"]); // self (domain) dropped
        assert!(term_has_neighbourhood(&term));
        assert!(!term_has_neighbourhood(&DocTerm::default()));
    }

    #[test]
    fn term_neighbourhood_svg_is_pure_and_labels_nodes() {
        let term = term_with_neighbours();
        let svg = term_neighbourhood_svg(&term);
        // Centre and both flank neighbours are present, by local name.
        assert!(svg.contains(">Centre</text>"));
        assert!(svg.contains(">Parent</text>"));
        assert!(svg.contains(">Related</text>"));
        // Pure: identical bytes across two calls.
        assert_eq!(svg, term_neighbourhood_svg(&term));
    }
}
