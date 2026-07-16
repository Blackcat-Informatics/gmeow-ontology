// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The JSON and human-readable-text projections of a packet. Both read the same
//! ordered value model, so both are deterministic.

use std::fmt::Write as _;

use crate::model::{AuthoringPacket, GroundingAttribute, ObjTerm};
use crate::ns;

fn obj_str(obj: &ObjTerm) -> String {
    match obj {
        ObjTerm::Iri(iri) => ns::curie(iri),
        ObjTerm::Blank(_) => "[ … ]".to_string(),
        ObjTerm::Literal {
            lexical, language, ..
        } => match language {
            Some(l) => format!("\"{lexical}\"@{l}"),
            None => format!("\"{lexical}\""),
        },
    }
}

impl AuthoringPacket {
    /// The packet as pretty-printed JSON.
    #[must_use]
    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self)
            .unwrap_or_else(|e| panic!("authoring packet failed to serialize to JSON: {e}"))
    }

    /// The packet as a human-readable authoring brief.
    #[must_use]
    pub fn render_text(&self) -> String {
        let mut out = String::new();
        let axis = self.axis.as_deref().unwrap_or("(whole slice)");
        let _ = writeln!(out, "AUTHORING PACKET  {}", self.packet_iri);
        let _ = writeln!(out, "  slice   : {}", self.source_slice);
        let _ = writeln!(out, "  axis    : {axis}");
        let _ = writeln!(out, "  batch   : {}", self.batch);
        let _ = writeln!(out, "  terms   : {}", self.term_count);
        let _ = writeln!(out, "  digest  : {}", self.digest);
        let _ = writeln!(
            out,
            "  exemplars: {} (shortfall {})",
            self.exemplars.len(),
            self.exemplar_shortfall
        );
        if !self.exemplars.is_empty() {
            for e in &self.exemplars {
                let _ = writeln!(out, "      - {}", ns::curie(e));
            }
        }
        out.push('\n');

        for term in &self.terms {
            let _ = writeln!(out, "── {} ──", ns::curie(&term.iri));
            if let Some(label) = &term.label {
                let _ = writeln!(out, "  label     : {label}");
            }
            if let Some(def) = &term.definition {
                let _ = writeln!(out, "  definition: {def}");
            }
            if !term.axioms.is_empty() {
                let _ = writeln!(out, "  axioms:");
                for t in &term.axioms {
                    let _ = writeln!(
                        out,
                        "      {} {}",
                        ns::curie(&t.predicate),
                        obj_str(&t.object)
                    );
                }
            }
            if !term.neighbors.is_empty() {
                let _ = writeln!(out, "  neighbourhood (depth-1 CBD):");
                for t in &term.neighbors {
                    let _ = writeln!(
                        out,
                        "      {} {}",
                        ns::curie(&t.predicate),
                        obj_str(&t.object)
                    );
                }
            }
            if !term.closure.is_empty() {
                let _ = writeln!(out, "  definitional closure:");
                for c in &term.closure {
                    let label = c.label.as_deref().unwrap_or("");
                    let _ = writeln!(out, "      {} — {label}", ns::curie(&c.iri));
                    if let Some(def) = &c.definition {
                        let _ = writeln!(out, "          {def}");
                    }
                }
            }
            out.push('\n');
        }

        let _ = writeln!(out, "GROUNDING COVERAGE");
        for cell in &self.grounding {
            let mark = if cell.present { "✓" } else { "·" };
            let attr = match cell.attribute {
                GroundingAttribute::En => "en",
                GroundingAttribute::Fr => "fr",
                GroundingAttribute::Zh => "zh",
                GroundingAttribute::ExternalMapped => "external",
                GroundingAttribute::Exemplar => "exemplar",
            };
            let mut line = format!("  {mark} {:<9} {}", attr, ns::curie(&cell.term));
            if let Some(p) = &cell.predicate {
                let _ = write!(line, " [{p}]");
            }
            if let Some(v) = &cell.value {
                let _ = write!(line, " = {v}");
            }
            if let Some(e) = &cell.external_entity {
                let _ = write!(line, " → {}", ns::curie(e));
                if let Some(a) = &cell.align_predicate {
                    let _ = write!(line, " ({a})");
                }
                if let Some(c) = cell.confidence {
                    let _ = write!(line, " conf {c}");
                }
            }
            if cell.conflict
                && let Some(w) = &cell.conflict_with
            {
                let _ = write!(line, " ⚠ conflicts with {}", ns::curie(w));
            }
            let _ = writeln!(out, "{line}");
        }
        out
    }
}
