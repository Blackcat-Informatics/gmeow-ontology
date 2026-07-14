// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Content addressing over the packet's semantic body.
//!
//! Both digests are sha256 over a canonical, newline-delimited rendering of the
//! ordered value model — never over a serialization that carries a volatile field
//! (no timestamp, no release version). Identical inputs therefore yield an
//! identical digest, which is the precondition for byte-stable turtle.

use sha2::{Digest, Sha256};

use crate::model::{CoveredTerm, GroundingCell, GroundingMargins, ObjTerm};

/// Hex sha256 of `body`.
fn hex(body: &str) -> String {
    let mut h = Sha256::new();
    h.update(body.as_bytes());
    let out = h.finalize();
    let mut s = String::with_capacity(out.len() * 2);
    for b in out {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

/// The canonical rendering of an object term for a digest — blank identity is
/// deliberately collapsed to `[]` (its content lives in the neighbour set), so the
/// digest never rides on parse-local blank labels.
fn render_obj(obj: &ObjTerm) -> String {
    match obj {
        ObjTerm::Iri(iri) => format!("<{iri}>"),
        ObjTerm::Blank(_) => "[]".to_string(),
        ObjTerm::Literal {
            lexical,
            datatype,
            language,
        } => match language {
            Some(l) => format!("\"{lexical}\"@{l}"),
            None => format!("\"{lexical}\"^^<{datatype}>"),
        },
    }
}

/// The per-term content digest: hex sha256 over the term's canonical body. When the
/// dataset carries a `gmeow:definitionDigest`, it is folded in as an authority the
/// digest defers to; otherwise the term's own content is the sole address.
#[must_use]
pub fn term_content_digest(term: &TermDigestInput) -> String {
    let mut body = String::new();
    body.push_str("IRI\t");
    body.push_str(term.iri);
    body.push('\n');
    if let Some(d) = &term.definition_digest {
        body.push_str("DEFDIGEST\t");
        body.push_str(d);
        body.push('\n');
    }
    body.push_str("LABEL\t");
    body.push_str(term.label.as_deref().unwrap_or(""));
    body.push('\n');
    body.push_str("DEF\t");
    body.push_str(term.definition.as_deref().unwrap_or(""));
    body.push('\n');
    for a in term.coat {
        body.push_str("COAT\t");
        body.push_str(&a.predicate);
        body.push('\t');
        body.push_str(a.language.as_deref().unwrap_or(""));
        body.push('\t');
        body.push_str(&a.value);
        body.push('\n');
    }
    for t in term.axioms {
        body.push_str("AXIOM\t");
        body.push_str(&t.predicate);
        body.push('\t');
        body.push_str(&render_obj(&t.object));
        body.push('\n');
    }
    for t in term.neighbors {
        body.push_str("NEIGH\t");
        body.push_str(&t.predicate);
        body.push('\t');
        body.push_str(&render_obj(&t.object));
        body.push('\n');
    }
    for c in term.closure {
        body.push_str("CLOSURE\t");
        body.push_str(&c.iri);
        body.push('\t');
        body.push_str(c.label.as_deref().unwrap_or(""));
        body.push('\t');
        body.push_str(c.definition.as_deref().unwrap_or(""));
        body.push('\n');
    }
    hex(&body)
}

/// The borrowed inputs to [`term_content_digest`] (kept separate from
/// [`CoveredTerm`] so the digest can be computed before the `content_digest` field
/// is filled).
pub struct TermDigestInput<'a> {
    /// The term IRI.
    pub iri: &'a str,
    /// The optional `gmeow:definitionDigest` carried in the dataset.
    pub definition_digest: Option<String>,
    /// The term's label.
    pub label: &'a Option<String>,
    /// The term's definition.
    pub definition: &'a Option<String>,
    /// The term's coat.
    pub coat: &'a [crate::model::Annotation],
    /// The term's axioms.
    pub axioms: &'a [crate::model::Triple],
    /// The term's neighbours.
    pub neighbors: &'a [crate::model::Triple],
    /// The term's definitional closure.
    pub closure: &'a [crate::model::ClosureEntry],
}

/// The packet digest: hex sha256 over the packet identity, the per-attribute margins,
/// the ordered per-term content digests, the ordered exemplar references, and the
/// ordered MATERIALIZED (present, non-English, non-exemplar) grounding cells — the SAME
/// semantic body [`crate::turtle`] emits. It excludes the packet IRI itself (derived
/// from identity), the always-present English cells, every absent
/// (derivable-complement) cell, and every volatile field, so `byte == digest` holds for
/// the sparse projection.
#[must_use]
pub fn packet_digest(
    source_slice: &str,
    axis: Option<&str>,
    batch: u32,
    terms: &[CoveredTerm],
    exemplars: &[String],
    margins: &GroundingMargins,
    grounding: &[GroundingCell],
) -> String {
    let mut body = String::new();
    body.push_str("SLICE\t");
    body.push_str(source_slice);
    body.push('\n');
    body.push_str("AXIS\t");
    body.push_str(axis.unwrap_or(""));
    body.push('\n');
    body.push_str(&format!("BATCH\t{batch}\n"));
    body.push_str(&format!(
        "MARGINS\t{}\t{}\t{}\t{}\t{}\t{}\n",
        margins.fr_present,
        margins.fr_absent,
        margins.zh_present,
        margins.zh_absent,
        margins.external_mapped,
        margins.external_absent,
    ));
    for t in terms {
        body.push_str("TERM\t");
        body.push_str(&t.iri);
        body.push('\t');
        body.push_str(&t.content_digest);
        body.push('\n');
    }
    for e in exemplars {
        body.push_str("EXEMPLAR\t");
        body.push_str(e);
        body.push('\n');
    }
    for c in grounding.iter().filter(|c| c.is_materialized()) {
        body.push_str("CELL\t");
        body.push_str(&c.term);
        body.push('\t');
        body.push_str(c.attribute.tag());
        body.push('\t');
        body.push_str(c.predicate.as_deref().unwrap_or(""));
        body.push('\t');
        body.push_str(c.value.as_deref().unwrap_or(""));
        body.push('\t');
        body.push_str(c.external_entity.as_deref().unwrap_or(""));
        body.push('\t');
        body.push_str(c.external_label.as_deref().unwrap_or(""));
        body.push('\t');
        body.push_str(c.align_predicate.as_deref().unwrap_or(""));
        body.push('\t');
        body.push_str(&c.confidence.map(|v| format!("{v}")).unwrap_or_default());
        body.push('\t');
        body.push_str(if c.conflict { "1" } else { "0" });
        body.push('\t');
        body.push_str(c.conflict_with.as_deref().unwrap_or(""));
        body.push('\n');
    }
    hex(&body)
}
