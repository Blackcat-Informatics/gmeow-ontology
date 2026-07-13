// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The CGIF **reader**: CGIF text → [`LogicProgram`] + diagnostics.
//!
//! The inverse of [`project_cgif`](super::writer::project_cgif). See [`crate::cgif`] for the
//! architecture. The **lossless round-trip carrier** is the RDF/predication channel after the
//! sentinel: it is reconstructed into an N-Triples dataset and lifted through the canonical
//! RDF frontend ([`parse_logic_dataset`]), so the
//! reconstructed IR — axioms, rules, formulas, contracts, correspondences — is exactly the
//! Exact `canonical-rdf12` round-trip's. The idiomatic conceptual graphs before the sentinel
//! are a human-readable VIEW; the reader still VALIDATES them (a malformed graph raises a
//! `CGIF_MALFORMED_SENTENCE` diagnostic), but the IR is never reconstructed from them.

use gmeow_errors::Diag;

use crate::frontend::{Diagnostic, LogicParseError, Severity, parse_logic_dataset};
use crate::ir::LogicProgram;
use crate::nt::{nt_escape_iri, nt_escape_literal};

use super::{CExpr, parse_forms, split_on_sentinel};

use purrdf::parse_dataset;

/// Parse CGIF source text into a [`LogicProgram`] + diagnostics.
///
/// Fail-soft: a malformed graph-channel view is recorded as a `CGIF_MALFORMED_SENTENCE`
/// warning (and does not affect the reconstructed IR); a truly unparsable document
/// (unbalanced brackets / empty) raises [`LogicParseError`]. A construct is never silently
/// dropped.
pub fn parse_cgif_str(
    cgif: &str,
    source_iri: Option<String>,
) -> Result<(LogicProgram, Vec<Diagnostic>), LogicParseError> {
    if cgif.trim().is_empty() {
        return Err(LogicParseError(
            "CGIF source is empty — nothing to parse.".to_owned(),
        ));
    }

    let (graph_src, meta_src) = split_on_sentinel(cgif);

    let mut diagnostics: Vec<Diagnostic> = Vec::new();

    // ── RDF / predication channel (the round-trip authority) ────────────────────
    // Reconstruct the WHOLE program from the predications after the sentinel, by lifting
    // them through the canonical RDF frontend (reuse, not reinvention).
    let (program, meta_diags) = parse_meta_block(&meta_src, source_iri.clone())?;
    diagnostics.extend(meta_diags);

    // ── Graph channel (human-readable view) — VALIDATE ONLY ─────────────────────
    // The idiomatic conceptual graphs are cross-checked for well-formedness (so corruption
    // surfaces as a diagnostic), but the IR above is the authority — the graph parse never
    // feeds it.
    match parse_forms(&graph_src) {
        Ok(forms) => {
            // A document carrying graph forms but NO meta carrier block cannot be reconstructed
            // (an idiomatic conceptual graph alone is lossy for the byte-exact IR), so fail
            // CLOSED rather than silently returning an empty program.
            if meta_src.trim().is_empty() && !forms.is_empty() {
                return Err(LogicParseError(
                    "CGIF has idiomatic conceptual graphs but no `/* @@gmeow-rdf-meta@@ */` \
                     carrier block; reconstruction from the graph view alone is lossy and \
                     unsupported."
                        .to_owned(),
                ));
            }
            for form in &forms {
                if let Err(msg) = validate_graph_form(form) {
                    diagnostics.push(Diagnostic {
                        severity: Severity::Warning,
                        code: "CGIF_MALFORMED_SENTENCE".to_owned(),
                        message: msg.message().to_owned(),
                        subject: None,
                    });
                }
            }
        }
        Err(msg) => {
            // The graph channel did not even lex/parse (unbalanced brackets etc.). If there is
            // no meta carrier either, we cannot reconstruct anything — fail closed. Otherwise
            // the authority (meta) already reconstructed the IR; record a fail-soft diagnostic.
            if meta_src.trim().is_empty() && !graph_src.trim().is_empty() {
                return Err(LogicParseError(format!(
                    "CGIF graph channel is malformed and there is no \
                     `/* @@gmeow-rdf-meta@@ */` carrier block to reconstruct from: {}",
                    msg.message()
                )));
            }
            diagnostics.push(Diagnostic {
                severity: Severity::Warning,
                code: "CGIF_MALFORMED_SENTENCE".to_owned(),
                message: msg.message().to_owned(),
                subject: None,
            });
        }
    }

    Ok((program, diagnostics))
}

// --------------------------------------------------------------------------- //
// RDF / predication channel
// --------------------------------------------------------------------------- //

/// Parse the RDF-meta block (the text after the sentinel) into a [`LogicProgram`]. Each
/// `("P" "S" "O")` predication becomes one N-Triples line; the assembled document is lifted by
/// the canonical RDF frontend.
fn parse_meta_block(
    meta_src: &str,
    source_iri: Option<String>,
) -> Result<(LogicProgram, Vec<Diagnostic>), LogicParseError> {
    if meta_src.trim().is_empty() {
        // No meta channel: an empty program (the graph channel carries everything).
        return Ok((
            LogicProgram::new(Vec::new(), Vec::new(), Vec::new(), source_iri),
            Vec::new(),
        ));
    }

    let forms = parse_forms(meta_src).map_err(|e| LogicParseError(e.message().to_owned()))?;
    let mut nt_lines: Vec<String> = Vec::new();
    for form in &forms {
        let CExpr::Paren(items) = form else {
            return Err(LogicParseError(format!(
                "CGIF meta block: expected a (P S O) predication, found: {form:?}"
            )));
        };
        if items.len() != 3 {
            return Err(LogicParseError(format!(
                "CGIF meta block: predication must have exactly 3 terms (P S O), found {}",
                items.len()
            )));
        }
        let p = nt_term(&items[0])?;
        let s = nt_term(&items[1])?;
        let o = nt_term(&items[2])?;
        nt_lines.push(format!("{s} {p} {o} ."));
    }

    if nt_lines.is_empty() {
        return Ok((
            LogicProgram::new(Vec::new(), Vec::new(), Vec::new(), source_iri),
            Vec::new(),
        ));
    }

    let nt = nt_lines.join("\n");
    let ds = parse_dataset(nt.as_bytes(), "application/n-triples", None)
        .map_err(|e| LogicParseError(format!("CGIF meta block: N-Triples re-parse failed: {e}")))?;
    parse_logic_dataset(ds.as_ref(), source_iri)
}

/// Encode one CGIF meta-predication term as an N-Triples term (`<iri>`, `_:b`, or a literal).
fn nt_term(expr: &CExpr) -> Result<String, LogicParseError> {
    match expr {
        CExpr::Str(name) => {
            if let Some(blank) = name.strip_prefix("_:") {
                Ok(format!("_:{blank}"))
            } else {
                Ok(format!("<{}>", nt_escape_iri(name)))
            }
        }
        CExpr::Paren(items) => {
            // A `(lit "lex")` / `(lit "lex" "dt")` / `(lit "lex" @lang)` reserved form.
            let lit = parse_lit_form(items)?;
            Ok(lit.to_ntriples())
        }
        other => Err(LogicParseError(format!(
            "CGIF meta term must be a quoted name or a (lit …) form, found: {other:?}"
        ))),
    }
}

/// A parsed `(lit …)` form.
struct LitTerm {
    lexical: String,
    datatype: Option<String>,
    language: Option<String>,
}

impl LitTerm {
    fn to_ntriples(&self) -> String {
        let lex = nt_escape_literal(&self.lexical);
        match (&self.datatype, &self.language) {
            (_, Some(lang)) => format!("\"{lex}\"@{lang}"),
            (Some(dt), None) => format!("\"{lex}\"^^<{}>", nt_escape_iri(dt)),
            (None, None) => format!("\"{lex}\""),
        }
    }
}

/// Parse a `(lit "lex")` / `(lit "lex" "dt")` / `(lit "lex" @lang)` form.
fn parse_lit_form(items: &[CExpr]) -> Result<LitTerm, LogicParseError> {
    let head = items.first().and_then(sym_of);
    if head != Some("lit") {
        return Err(LogicParseError(format!(
            "expected a (lit …) form, found head {head:?}"
        )));
    }
    // A `(lit …)` form is `(lit "x")` or `(lit "x" "dt")` / `(lit "x" @lang)` — never more.
    if items.len() > 3 {
        return Err(LogicParseError(format!(
            "(lit …) form has {} operands; expected 2 or 3 (lexical + optional datatype/lang)",
            items.len()
        )));
    }
    let lexical = match items.get(1) {
        Some(CExpr::Str(s)) => s.clone(),
        other => {
            return Err(LogicParseError(format!(
                "(lit …) first argument must be a \"string\", found {other:?}"
            )));
        }
    };
    match items.get(2) {
        None => Ok(LitTerm {
            lexical,
            datatype: None,
            language: None,
        }),
        Some(CExpr::Str(dt)) => Ok(LitTerm {
            lexical,
            datatype: Some(dt.clone()),
            language: None,
        }),
        Some(CExpr::At(lang)) => Ok(LitTerm {
            lexical,
            datatype: None,
            language: Some(lang.clone()),
        }),
        other => Err(LogicParseError(format!(
            "(lit …) third argument must be a \"datatype\" or @lang, found {other:?}"
        ))),
    }
}

// --------------------------------------------------------------------------- //
// Graph channel validation (view only)
// --------------------------------------------------------------------------- //

/// Validate that a top-level graph-channel form is a recognizable conceptual-graph shape: a
/// relation `( … )`, a concept / context node `[ … ]`, a negated context `~[ … ]`, or a bare
/// coreference label. `Ok(())` = well-formed; `Err(msg)` = a `CGIF_MALFORMED_SENTENCE`
/// diagnostic. The graph is a view only — the RDF channel is the round-trip authority.
fn validate_graph_form(form: &CExpr) -> gmeow_errors::Result<()> {
    match form {
        // A relation `(name arc…)` must have a name-like head (a quoted CGIF name or a symbol).
        CExpr::Paren(items) => {
            let Some(head) = items.first() else {
                return Err(Diag::of_kind(crate::error::Cgif {
                    detail: "empty relation `()` in the CGIF graph channel".to_owned(),
                }));
            };
            if !matches!(head, CExpr::Str(_) | CExpr::Sym(_)) {
                return Err(Diag::of_kind(crate::error::Cgif {
                    detail: format!("CGIF relation must start with a name, found head {head:?}"),
                }));
            }
            Ok(())
        }
        // A concept / context node — brackets already parsed balanced; accept.
        CExpr::Brack(_) => Ok(()),
        // A negated context `~[…]`.
        CExpr::Neg(inner) => validate_graph_form(inner),
        // A bare coreference label or quantifier is an acceptable dangling view element.
        CExpr::Bound(_) | CExpr::Def(_) | CExpr::At(_) => Ok(()),
        other => Err(Diag::of_kind(crate::error::Cgif {
            detail: format!(
                "top-level CGIF graph form must be a relation, concept, or negated context, \
                 found {other:?}"
            ),
        })),
    }
}

/// The symbol string of a bare [`CExpr::Sym`], borrowed (no clone).
fn sym_of(expr: &CExpr) -> Option<&str> {
    match expr {
        CExpr::Sym(s) => Some(s),
        _ => None,
    }
}
