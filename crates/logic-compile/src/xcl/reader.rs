// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The XCL **reader**: XCL XML → [`LogicProgram`] + diagnostics.
//!
//! The inverse of [`project_xcl`](super::writer::project_xcl). See [`crate::xcl`] for the
//! architecture. The **lossless round-trip carrier** is the [`RDF_META_ELEMENT`] element: its
//! text is canonical N-Triples, reconstructed into a dataset and lifted through the canonical
//! RDF frontend ([`parse_logic_dataset`]), so the
//! reconstructed IR — axioms, rules, formulas, contracts, correspondences — is exactly the Exact
//! `canonical-rdf12` round-trip's. The idiomatic XCL2 sentences are a human-readable VIEW: the
//! document is parsed by a real XML parser (`roxmltree`, never a hand-rolled scanner) so
//! well-formedness is enforced, but the IR is never reconstructed from the sentences.

use gmeow_errors::Diag;

use crate::frontend::{Diagnostic, LogicParseError, Severity, parse_logic_dataset};
use crate::ir::LogicProgram;

use super::{RDF_META_ELEMENT, ROOT_ELEMENT};

use purrdf::parse_dataset;

/// The idiomatic-sentence container element the writer emits.
const SENTENCES_ELEMENT: &str = "sentences";

/// Parse XCL source text into a [`LogicProgram`] + diagnostics.
///
/// Fail-soft: a foreign/unexpected sentence element is recorded as an `XCL_MALFORMED_SENTENCE`
/// warning (and does not affect the reconstructed IR). A document that is not well-formed XML,
/// or that carries idiomatic sentences with no `<gmeow-rdf-meta>` carrier to reconstruct from,
/// raises [`LogicParseError`] — a construct is never silently dropped.
pub fn parse_xcl_str(
    xcl: &str,
    source_iri: Option<String>,
) -> Result<(LogicProgram, Vec<Diagnostic>), LogicParseError> {
    if xcl.trim().is_empty() {
        return Err(LogicParseError(
            "XCL source is empty — nothing to parse.".to_owned(),
        ));
    }

    // A real XML parse enforces well-formedness. A parse failure means we can reconstruct
    // nothing (the meta carrier lives in the same document), so fail closed.
    let doc = roxmltree::Document::parse(xcl)
        .map_err(|e| LogicParseError(format!("XCL is not well-formed XML: {e}")))?;

    let root = doc.root_element();
    if !root.has_tag_name(ROOT_ELEMENT) {
        return Err(LogicParseError(format!(
            "XCL root element must be <{ROOT_ELEMENT}>, found <{}>",
            root.tag_name().name()
        )));
    }

    let mut diagnostics: Vec<Diagnostic> = Vec::new();

    // ── RDF / predication channel (the round-trip authority) ────────────────────
    let meta_node = root
        .descendants()
        .find(|n| n.is_element() && n.has_tag_name(RDF_META_ELEMENT));

    // ── Sentence channel (human-readable view) — VALIDATE ONLY ──────────────────
    // Count the idiomatic sentence children so a document that carries sentences but lost its
    // meta carrier fails CLOSED (the sentence view alone is lossy for the byte-exact IR).
    let sentence_children: Vec<_> = root
        .children()
        .filter(|n| n.is_element() && n.has_tag_name(SENTENCES_ELEMENT))
        .flat_map(|s| s.children().filter(roxmltree::Node::is_element))
        .collect();

    let meta_text: String = match meta_node {
        Some(node) => node
            .descendants()
            .filter(roxmltree::Node::is_text)
            .filter_map(|n| n.text())
            .collect(),
        None => {
            if !sentence_children.is_empty() {
                return Err(LogicParseError(format!(
                    "XCL has idiomatic <{SENTENCES_ELEMENT}> but no <{RDF_META_ELEMENT}> carrier \
                     element; reconstruction from the sentence view alone is lossy and unsupported."
                )));
            }
            String::new()
        }
    };

    // Cross-check the sentence view for recognizable shapes (fail-soft diagnostics only).
    for child in &sentence_children {
        if let Err(msg) = validate_sentence(child) {
            diagnostics.push(Diagnostic {
                severity: Severity::Warning,
                code: "XCL_MALFORMED_SENTENCE".to_owned(),
                message: msg.message().to_owned(),
                subject: None,
            });
        }
    }

    // Reconstruct the WHOLE program from the meta carrier's N-Triples.
    if meta_text.trim().is_empty() {
        if !sentence_children.is_empty() {
            return Err(LogicParseError(format!(
                "XCL has idiomatic <{SENTENCES_ELEMENT}> but an empty <{RDF_META_ELEMENT}> \
                 carrier element; reconstruction from the sentence view alone is lossy and \
                 unsupported."
            )));
        }
        // No meta payload: a legitimately empty program (there were no sentences either).
        return Ok((
            LogicProgram::new(Vec::new(), Vec::new(), Vec::new(), source_iri),
            diagnostics,
        ));
    }

    let ds = parse_dataset(meta_text.as_bytes(), "application/n-triples", None).map_err(|e| {
        LogicParseError(format!("XCL meta carrier: N-Triples re-parse failed: {e}"))
    })?;
    let (program, meta_diags) = parse_logic_dataset(ds.as_ref(), source_iri)?;
    diagnostics.extend(meta_diags);

    Ok((program, diagnostics))
}

/// The XCL2 sentence element tags the writer emits at the top level of `<sentences>`.
const KNOWN_SENTENCE_TAGS: [&str; 8] = [
    "atom", "rule", "not", "and", "or", "implies", "iff", "forall",
];

/// Validate that a top-level sentence-channel element is a recognizable XCL2 sentence shape.
/// `Ok(())` = well-formed; `Err(msg)` = an `XCL_MALFORMED_SENTENCE` diagnostic. The sentences are
/// a view only — the meta carrier is the round-trip authority.
fn validate_sentence(node: &roxmltree::Node<'_, '_>) -> gmeow_errors::Result<()> {
    let tag = node.tag_name().name();
    // `exists` is a valid top-level sentence too; accept it alongside the writer's set.
    if KNOWN_SENTENCE_TAGS.contains(&tag) || tag == "exists" {
        Ok(())
    } else {
        Err(Diag::of_kind(crate::error::Xcl {
            detail: format!(
                "unexpected XCL sentence element <{tag}> (expected an atom / rule / connective / \
                 quantifier)"
            ),
        }))
    }
}
