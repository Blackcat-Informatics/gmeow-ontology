// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The shared [`TermValue`] surface renderer for output and explanation text.
//!
//! # Why this lives in the arena crate
//!
//! Rendering is separate from native term identity. The arena and the reasoning
//! runtime share one traversal ([`render_term`]); the two output styles differ
//! only in the literal arm. A presentation may elide datatype fields and must
//! never be used as a dictionary or content-identity key.
//!
//! # N3 serialization rules (mirror of rdflib `.n3()`)
//!
//! rdflib's `.n3()` produces:
//! - IRI: `<iri>`
//! - Language-tagged literal: `"lex"@lang`  (rdflib lower-cases the lang subtag)
//! - `xsd:string` literal: `"lex"` (datatype **elided**)
//! - `rdf:langString` literal: `"lex"@lang` (datatype **elided**, lang kept)
//! - Any other typed literal: `"lex"^^<datatype_iri>`
//!
//! Lexical-form escaping:
//! - `\` → `\\`
//! - `"` → `\"`
//! - `\n` (newline) → `\n`
//! - `\r` (CR) → `\r`
//! - `\t` (tab) → `\t`
//!
//! No numeric normalization — the lexical form is preserved verbatim.

use purrdf::{RdfTextDirection, TermValue};

/// The `xsd:string` datatype IRI — elided by both render styles.
pub const XSD_STRING: &str = "http://www.w3.org/2001/XMLSchema#string";
/// The `rdf:langString` datatype IRI — elided by both render styles.
pub const RDF_LANG_STRING: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#langString";

/// Escape a literal lexical form exactly as rdflib does in `.n3()`.
///
/// rdflib escapes: `\` → `\\`, `"` → `\"`, newline → `\n`, CR → `\r`, tab → `\t`.
/// No other escaping is applied.
fn escape_lexical(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 4);
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            other => out.push(other),
        }
    }
    out
}

/// Serialize a native literal (lexical form + datatype IRI + optional language) to
/// rdflib `.n3()` form.
///
/// Rules:
/// - `xsd:string` → `"lex"` (datatype elided)
/// - `rdf:langString` (lang-tagged) → `"lex"@lang` (datatype elided, lang kept)
/// - Any other datatype → `"lex"^^<datatype_iri>`
///
/// rdflib lowercases the BCP-47 language subtag.  The native IR lowercases the
/// language tag for the identity key, but a `TermValue` constructed directly may
/// carry an un-lowercased tag, so we lowercase here to stay in sync regardless.
fn literal_n3_parts(
    lexical_form: &str,
    datatype: &str,
    language: Option<&str>,
    direction: Option<RdfTextDirection>,
) -> String {
    let lex = escape_lexical(lexical_form);

    if let Some(lang) = language {
        // Language-tagged literal — rdflib elides the rdf:langString datatype.
        // rdflib lowercases the language tag; mirror that.
        let language = lang.to_lowercase();
        return match direction {
            Some(direction) => format!("\"{lex}\"@{language}--{}", direction.as_str()),
            None => format!("\"{lex}\"@{language}"),
        };
    }

    if datatype == XSD_STRING {
        // Plain xsd:string — rdflib elides the datatype.
        return format!("\"{}\"", lex);
    }
    if datatype == RDF_LANG_STRING {
        // rdf:langString without a language tag — treated like xsd:string by rdflib.
        // (In practice rdf:langString always has a lang tag; be defensive.)
        return format!("\"{}\"", lex);
    }

    // Typed literal with a non-elided datatype.
    format!("\"{}\"^^<{}>", lex, datatype)
}

#[derive(Clone, Copy)]
enum TermRenderStyle {
    N3,
    Display,
}

enum TermRenderTask<'term> {
    Term(&'term TermValue),
    Text(&'static str),
}

fn render_term(term: &TermValue, style: TermRenderStyle) -> String {
    let mut rendered = String::new();
    let mut tasks = vec![TermRenderTask::Term(term)];
    while let Some(task) = tasks.pop() {
        let term = match task {
            TermRenderTask::Term(term) => term,
            TermRenderTask::Text(text) => {
                rendered.push_str(text);
                continue;
            }
        };
        match term {
            TermValue::Iri(iri) => {
                rendered.push('<');
                rendered.push_str(iri);
                rendered.push('>');
            }
            TermValue::Blank { label, scope } => {
                rendered.push_str("_:");
                rendered.push_str(&scope.qualify_label(label));
            }
            TermValue::Literal {
                lexical_form,
                datatype,
                language,
                direction,
            } => match style {
                TermRenderStyle::N3 => rendered.push_str(&literal_n3_parts(
                    lexical_form,
                    datatype,
                    language.as_deref(),
                    *direction,
                )),
                TermRenderStyle::Display => {
                    let lex = escape_lexical(lexical_form);
                    if let Some(lang) = language {
                        rendered.push('"');
                        rendered.push_str(&lex);
                        rendered.push_str("\"@");
                        rendered.push_str(lang);
                        if let Some(direction) = direction {
                            rendered.push_str("--");
                            rendered.push_str(direction.as_str());
                        }
                    } else if datatype == XSD_STRING || datatype == RDF_LANG_STRING {
                        rendered.push('"');
                        rendered.push_str(&lex);
                        rendered.push('"');
                    } else {
                        rendered.push('"');
                        rendered.push_str(&lex);
                        rendered.push_str("\"^^<");
                        rendered.push_str(datatype);
                        rendered.push('>');
                    }
                }
            },
            TermValue::Triple { s, p, o } => {
                tasks.push(TermRenderTask::Text(" )>>"));
                tasks.push(TermRenderTask::Term(o));
                tasks.push(TermRenderTask::Text(" "));
                tasks.push(TermRenderTask::Term(p));
                tasks.push(TermRenderTask::Text(" "));
                tasks.push(TermRenderTask::Term(s));
                tasks.push(TermRenderTask::Text("<<( "));
            }
        }
    }
    rendered
}

/// Render a [`TermValue`] in the canonical Turtle term Display form.
///
/// This output surface preserves the historical `Term::to_string()` spelling.
/// It may elide native datatype fields and must not determine term identity.
///
/// Unlike [`term_n3_unchecked`] it does **not** lowercase the language tag (the Display
/// form preserves the stored tag verbatim) and renders a triple term in the RDF 1.2
/// non-asserting triple-term form `<<( s p o )>>`.
///
/// - `Iri` → `<iri>`
/// - `Blank` → `_:label`
/// - `Literal` xsd:string / rdf:langString → `"lex"` ; lang → `"lex"@lang` ;
///   typed → `"lex"^^<dt>`
/// - `Triple` → `<<( s p o )>>` (iteratively traversed, including nested triples)
pub fn term_display(term: &TermValue) -> String {
    render_term(term, TermRenderStyle::Display)
}

/// Render a [`TermValue`] in rdflib `.n3()` form — the *rendering half* of the provenance
/// recipes.
///
/// `_unchecked` names exactly one thing: this function does NOT validate that a nested
/// RDF 1.2 triple term carries an IRI predicate.  That validation is a semantic
/// precondition of the provenance recipes, which own it and raise their own typed
/// diagnostic; the renderer is total over a well-formed `TermValue` and never fails.
///
/// - `Iri(iri)` → `<iri>`
/// - `Blank` → `_:label`
/// - `Literal` → delegated to the N3 literal rules (see the module doctrine)
/// - `Triple` → RDF 1.2 non-asserting triple term `<<( s p o )>>`, recursively.
pub fn term_n3_unchecked(term: &TermValue) -> String {
    render_term(term, TermRenderStyle::N3)
}

#[path = "display.tests.rs"]
#[cfg(test)]
mod tests;
