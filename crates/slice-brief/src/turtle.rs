// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Canonical-turtle projection of a packet — the CANONICAL, byte-reconstructible,
//! parity-critical surface.
//!
//! A formal context is naturally SPARSE, so this projection materializes ONLY the
//! present (object, attribute) incidences of the grounding cross-table — the fr/zh
//! translations and external mappings that actually exist. English is always present
//! (its margin IS `gmeow:packetTermCount`) and every absent incidence is the derivable
//! complement, both summarized by the packet's per-attribute counts
//! (`gmeow:packetFrPresent` / `packetFrAbsent`, `packetZhPresent` / `packetZhAbsent`,
//! `packetExternalMapped` / `packetExternalAbsent`) rather than materialized as a cell
//! apiece. Exemplars are carried by reference through `gmeow:packetExemplar`.
//!
//! The packet individual and its materialized grounding-coverage cells are written to a
//! turtle body with **stable, minted cell IRIs** (never blank nodes, whose labels are
//! not byte-stable) and then normalized through
//! `purrdf::turtle_normalize::canonical_turtle` — the SAME serializer the pipeline
//! superset gate folds with — so `file == fold` holds and the bytes are stable across
//! identical assemblies.

use crate::model::{AuthoringPacket, GroundingCell};
use crate::ns;

/// Escape a string for a double-quoted turtle literal.
fn esc(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            _ => out.push(c),
        }
    }
    out
}

/// A double-quoted turtle string literal.
fn lit(s: &str) -> String {
    format!("\"{}\"", esc(s))
}

/// Render a decimal so it always parses as `xsd:decimal` (a trailing `.0` when the
/// value has no fractional part), never as `xsd:integer`.
fn decimal(v: f64) -> String {
    let s = format!("{v}");
    if s.contains('.') { s } else { format!("{s}.0") }
}

fn cell_block(cell: &GroundingCell) -> String {
    let mut po: Vec<String> = vec![
        "a gmeow:GroundingCoverage".to_string(),
        format!("gmeow:groundingTerm <{}>", cell.term),
        format!("gmeow:groundingAttribute <{}>", cell.attribute.iri()),
    ];
    if let Some(p) = &cell.predicate {
        po.push(format!("gmeow:groundingPredicate {}", lit(p)));
    }
    if let Some(v) = &cell.value {
        po.push(format!("gmeow:groundingValue {}", lit(v)));
    }
    if let Some(e) = &cell.external_entity {
        po.push(format!("gmeow:groundingExternalEntity <{e}>"));
    }
    if let Some(l) = &cell.external_label {
        po.push(format!("gmeow:groundingExternalLabel {}", lit(l)));
    }
    if let Some(a) = &cell.align_predicate {
        po.push(format!("gmeow:groundingAlignPredicate {}", lit(a)));
    }
    if let Some(c) = cell.confidence {
        po.push(format!("gmeow:groundingConfidence {}", decimal(c)));
    }
    if cell.conflict {
        po.push("gmeow:groundingConflict true".to_string());
    }
    if let Some(w) = &cell.conflict_with {
        po.push(format!("gmeow:groundingConflictWith <{w}>"));
    }
    format!("<{}>\n  {} .\n\n", cell.cell_iri, po.join(" ;\n  "))
}

impl AuthoringPacket {
    /// The packet as canonical, byte-stable turtle.
    ///
    /// # Panics
    /// Panics if the internally-built turtle body fails to canonicalize — an
    /// unreachable invariant (the body is always well-formed turtle); a panic here
    /// signals a bug in the body construction, never a data condition.
    #[must_use]
    pub fn to_turtle(&self) -> String {
        let mut po: Vec<String> = vec![
            "a gmeow:AuthoringPacket".to_string(),
            format!("gmeow:packetSourceSlice <{}>", self.source_slice),
        ];
        if let Some(ax) = &self.axis {
            po.push(format!("gmeow:packetAxis {}", lit(ax)));
        }
        po.push(format!("gmeow:packetBatch {}", self.batch));
        po.push(format!("gmeow:packetDigest {}", lit(&self.digest)));
        po.push(format!("gmeow:packetTermCount {}", self.term_count));
        po.push(format!(
            "gmeow:exemplarShortfall {}",
            self.exemplar_shortfall
        ));
        // Per-attribute margins of the sparse cross-table: the present incidences below
        // plus these counts recover the full (dense) grounding profile, so absence is
        // an explicit recorded fact without a cell per absent incidence.
        let m = &self.margins;
        po.push(format!("gmeow:packetFrPresent {}", m.fr_present));
        po.push(format!("gmeow:packetFrAbsent {}", m.fr_absent));
        po.push(format!("gmeow:packetZhPresent {}", m.zh_present));
        po.push(format!("gmeow:packetZhAbsent {}", m.zh_absent));
        po.push(format!("gmeow:packetExternalMapped {}", m.external_mapped));
        po.push(format!("gmeow:packetExternalAbsent {}", m.external_absent));
        if !self.terms.is_empty() {
            let list = self
                .terms
                .iter()
                .map(|t| format!("<{}>", t.iri))
                .collect::<Vec<_>>()
                .join(" , ");
            po.push(format!("gmeow:packetCoversTerm {list}"));
        }
        if !self.exemplars.is_empty() {
            let list = self
                .exemplars
                .iter()
                .map(|e| format!("<{e}>"))
                .collect::<Vec<_>>()
                .join(" , ");
            po.push(format!("gmeow:packetExemplar {list}"));
        }
        let materialized: Vec<&GroundingCell> = self
            .grounding
            .iter()
            .filter(|c| c.is_materialized())
            .collect();
        if !materialized.is_empty() {
            let list = materialized
                .iter()
                .map(|c| format!("<{}>", c.cell_iri))
                .collect::<Vec<_>>()
                .join(" , ");
            po.push(format!("gmeow:packetGrounding {list}"));
        }

        let mut body = String::new();
        for (prefix, namespace) in ns::prefixes() {
            body.push_str(&format!("@prefix {prefix}: <{namespace}> .\n"));
        }
        body.push('\n');
        body.push_str(&format!(
            "<{}>\n  {} .\n\n",
            self.packet_iri,
            po.join(" ;\n  ")
        ));
        for cell in &materialized {
            body.push_str(&cell_block(cell));
        }

        purrdf::turtle_normalize::canonical_turtle(body.as_bytes(), &ns::prefixes())
            .unwrap_or_else(|e| panic!("authoring-packet turtle failed to canonicalize: {e}"))
    }
}
