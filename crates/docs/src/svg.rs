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

use crate::model::DocsModel;

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
}
