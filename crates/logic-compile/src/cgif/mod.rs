// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The CGIF (Conceptual Graph Interchange Format) text dialect.
//!
//! CGIF is the ISO/IEC 24707 conceptual-graph syntax for full first-order Common Logic
//! (Sowa's graph notation — concepts `[Type: referent]`, relations `(rel arc…)`, coreference
//! `*x` / `?x`, negated contexts `~[…]`, universals `[@every*x]`). This module is a
//! **bidirectional, `PreservationKind::Exact`** dialect, a sibling of [`crate::clif`]:
//! [`project_cgif`] lowers a [`LogicProgram`](crate::ir::LogicProgram)
//! to CGIF text and [`parse_cgif_str`] lifts it back, and the two
//! are inverses on the canonical IR (the production round-trip test pins this).
//!
//! ## The two-channel split that makes Exact genuine
//!
//! Identical in shape to CLIF's — the IR is round-tripped through two disjoint channels:
//!
//! 1. **Idiomatic conceptual-graph channel** — `program.rules` + `program.formulas`. These
//!    become readable conceptual graphs (`[If: … [Then: …]]` rules, `~[…]` negated contexts,
//!    `[@every*x]` universals, `[*x]` existentials) by bespoke code below. This channel is
//!    **WRITE-ONLY / validated-only on read**: the canonical IR carries an `obj_is_literal`
//!    bit and minted reifier-node identities that idiomatic conceptual-graph syntax cannot
//!    express, so reconstructing the byte-exact IR from a graph alone would be lossy.
//! 2. **RDF / predication channel** — everything else (axioms + scope, contracts, path shapes,
//!    correspondences, transaction programs). These are already flat RDF; the writer serializes
//!    them through the lossless canonical-RDF-1.2 projection and re-emits each triple as a CGIF
//!    relation predication `("P" "S" "O")`, and the reader reconstructs the dataset and re-uses
//!    the canonical RDF frontend. The bidirectional faithfulness of that leg is therefore
//!    exactly the canonical-RDF-1.2 target's (already `Exact`).
//!
//! The lexer / printer are private helpers kept inside this module; [`writer`] and [`reader`]
//! are the public dialect surface.
//!
//! ### CGIF term lexicalisation used by the predication channel
//!
//! An IRI (or blank node) rides as a double-quoted CGIF name `"iri"` (`"_:label"` for a blank),
//! a variable as a bound coreference label `?x`, and a literal as a `(lit "lex")` /
//! `(lit "lex" "dt")` / `(lit "lex" @lang)` reserved relation form. An IRI arc and a literal's
//! lexical are both double-quoted, but the grammatical position (a direct arc vs. inside a
//! `(lit …)` form) disambiguates them — exactly as CLIF disambiguates `'iri'` from `(lit "x")`.

use gmeow_errors::Diag;

pub mod reader;
pub mod writer;

pub use reader::parse_cgif_str;
pub use writer::project_cgif;

#[cfg(test)]
mod tests;

// --------------------------------------------------------------------------- //
// The sentinel comment that delimits the RDF-meta block from the graph channel.
// --------------------------------------------------------------------------- //

/// The sentinel comment that opens the RDF/predication meta block. The reader detects it
/// to route the predications that follow into the RDF channel rather than the graph channel.
/// Kept as a CGIF `/* … */` block comment so a generic CGIF consumer ignores it, while still
/// being machine-detectable on its own line.
pub(crate) const RDF_META_SENTINEL: &str = "/* @@gmeow-rdf-meta@@ */";

// --------------------------------------------------------------------------- //
// CGIF parse tree
// --------------------------------------------------------------------------- //

/// A parsed CGIF expression. The variants cover both channels: the predication channel uses
/// [`CExpr::Paren`] / [`CExpr::Str`] / [`CExpr::Bound`] / [`CExpr::Sym`] / [`CExpr::At`], and
/// the idiomatic graph channel additionally uses [`CExpr::Brack`] (concept / context nodes),
/// [`CExpr::Neg`] (negated contexts), [`CExpr::Def`] (defining coreference labels), and
/// [`CExpr::Colon`] (the concept type/referent separator). The reader reconstructs the IR only
/// from the predication channel; the graph channel is validated for well-formedness (balanced
/// structure) but never reconstructed, so the tree only needs to round-trip *structurally*.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum CExpr {
    /// A `( … )` relation / reserved form.
    Paren(Vec<CExpr>),
    /// A `[ … ]` concept / context node (idiomatic graph channel only).
    Brack(Vec<CExpr>),
    /// A `~ <expr>` negated context (idiomatic graph channel only).
    Neg(Box<CExpr>),
    /// The `:` concept type/referent separator (idiomatic graph channel only).
    Colon,
    /// A `"…"`-quoted CGIF name or string. The stored value is the unescaped inner string.
    Str(String),
    /// A `?`-prefixed bound coreference label (the leading `?` is stripped).
    Bound(String),
    /// A `*`-prefixed defining coreference label (the leading `*` is stripped).
    Def(String),
    /// An `@word` token — a quantifier (`@every`) or a `@lang` tag (leading `@` stripped).
    At(String),
    /// A bare symbol (`lit`, `seq`, `If`, `Then`, a type label, …).
    Sym(String),
}

// --------------------------------------------------------------------------- //
// Printer helpers
// --------------------------------------------------------------------------- //

/// Escape `"` and `\` inside a double-quoted CGIF name / string.
pub(crate) fn escape_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            c => out.push(c),
        }
    }
    out
}

// --------------------------------------------------------------------------- //
// Lexer + recursive-descent parser
// --------------------------------------------------------------------------- //

/// A lexical token.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Token {
    Open,
    Close,
    OpenBrack,
    CloseBrack,
    Tilde,
    Colon,
    Str(String),
    Bound(String),
    Def(String),
    At(String),
    Sym(String),
}

/// Tokenize CGIF source, stripping `/* … */` block comments. Returns the token stream.
fn lex(src: &str) -> gmeow_errors::Result<Vec<Token>> {
    let mut tokens = Vec::new();
    let chars: Vec<char> = src.chars().collect();
    let mut i = 0;
    let n = chars.len();
    while i < n {
        let c = chars[i];
        match c {
            // Whitespace.
            c if c.is_whitespace() => i += 1,
            // `/* … */` block comment.
            '/' if i + 1 < n && chars[i + 1] == '*' => {
                i += 2;
                while i + 1 < n && !(chars[i] == '*' && chars[i + 1] == '/') {
                    i += 1;
                }
                if i + 1 >= n {
                    return Err(Diag::of_kind(crate::error::Cgif {
                        detail: "unterminated /* … */ comment".to_owned(),
                    }));
                }
                i += 2; // consume the closing `*/`
            }
            // A `/` that does not open a `/* … */` comment is malformed CGIF: the writer only
            // ever emits `/` as a comment delimiter. Hard-fail rather than fall through to the
            // bare-symbol arm, where `lex_bare` would break immediately on `/` (its own break
            // char) without advancing `i` and spin forever.
            '/' => {
                return Err(Diag::of_kind(crate::error::Cgif {
                    detail: "unexpected '/' outside a /* … */ comment".to_owned(),
                }));
            }
            '(' => {
                tokens.push(Token::Open);
                i += 1;
            }
            ')' => {
                tokens.push(Token::Close);
                i += 1;
            }
            '[' => {
                tokens.push(Token::OpenBrack);
                i += 1;
            }
            ']' => {
                tokens.push(Token::CloseBrack);
                i += 1;
            }
            '~' => {
                tokens.push(Token::Tilde);
                i += 1;
            }
            ':' => {
                tokens.push(Token::Colon);
                i += 1;
            }
            // A `"…"`-quoted CGIF name / string.
            '"' => {
                let (val, next) = lex_quoted(&chars, i + 1)?;
                tokens.push(Token::Str(val));
                i = next;
            }
            // A `?bound` coreference label.
            '?' => {
                let (val, next) = lex_bare(&chars, i + 1);
                if val.is_empty() {
                    return Err(Diag::of_kind(crate::error::Cgif {
                        detail: "empty coreference label after '?'".to_owned(),
                    }));
                }
                tokens.push(Token::Bound(val));
                i = next;
            }
            // A `*def` defining coreference label.
            '*' => {
                let (val, next) = lex_bare(&chars, i + 1);
                if val.is_empty() {
                    return Err(Diag::of_kind(crate::error::Cgif {
                        detail: "empty coreference label after '*'".to_owned(),
                    }));
                }
                tokens.push(Token::Def(val));
                i = next;
            }
            // An `@word` quantifier / language tag.
            '@' => {
                let (val, next) = lex_bare(&chars, i + 1);
                if val.is_empty() {
                    return Err(Diag::of_kind(crate::error::Cgif {
                        detail: "empty token after '@'".to_owned(),
                    }));
                }
                tokens.push(Token::At(val));
                i = next;
            }
            // A bare symbol.
            _ => {
                let (val, next) = lex_bare(&chars, i);
                tokens.push(Token::Sym(val));
                i = next;
            }
        }
    }
    Ok(tokens)
}

/// Read a `"…"`-quoted token body (starting AFTER the opening quote), honoring `\\` and `\"`
/// escapes. Returns the unescaped value and the index past the closing quote.
fn lex_quoted(chars: &[char], start: usize) -> gmeow_errors::Result<(String, usize)> {
    let mut out = String::new();
    let mut i = start;
    let n = chars.len();
    while i < n {
        let c = chars[i];
        if c == '\\' {
            if i + 1 >= n {
                return Err(Diag::of_kind(crate::error::Cgif {
                    detail: "trailing backslash in quoted token".to_owned(),
                }));
            }
            // Only `\\` and `\"` are recognized escapes (matching the writer).
            out.push(chars[i + 1]);
            i += 2;
            continue;
        }
        if c == '"' {
            return Ok((out, i + 1));
        }
        out.push(c);
        i += 1;
    }
    Err(Diag::of_kind(crate::error::Cgif {
        detail: "unterminated quoted token (expected closing \")".to_owned(),
    }))
}

/// Read a bare token (symbol / coreference label / quantifier tag) starting at `start`,
/// stopping at whitespace, a bracket / paren / tilde / colon / quote delimiter, or a `/`
/// (comment lead-in). Returns the token and the index past it.
fn lex_bare(chars: &[char], start: usize) -> (String, usize) {
    let mut out = String::new();
    let mut i = start;
    let n = chars.len();
    while i < n {
        let c = chars[i];
        if c.is_whitespace()
            || matches!(
                c,
                '(' | ')' | '[' | ']' | '~' | ':' | '"' | '?' | '*' | '@' | '/'
            )
        {
            break;
        }
        out.push(c);
        i += 1;
    }
    (out, i)
}

/// Parse CGIF source into a flat list of top-level [`CExpr`] forms.
fn parse_forms(src: &str) -> gmeow_errors::Result<Vec<CExpr>> {
    let tokens = lex(src)?;
    let mut pos = 0;
    let mut forms = Vec::new();
    while pos < tokens.len() {
        let (expr, next) = parse_one(&tokens, pos)?;
        forms.push(expr);
        pos = next;
    }
    Ok(forms)
}

/// Parse one CGIF expression starting at token index `pos`. Returns the expression and the
/// next token index.
fn parse_one(tokens: &[Token], pos: usize) -> gmeow_errors::Result<(CExpr, usize)> {
    match tokens.get(pos) {
        None => Err(Diag::of_kind(crate::error::Cgif {
            detail: "unexpected end of input while parsing CGIF".to_owned(),
        })),
        Some(Token::Close) => Err(Diag::of_kind(crate::error::Cgif {
            detail: "unbalanced ')' — unexpected close paren".to_owned(),
        })),
        Some(Token::CloseBrack) => Err(Diag::of_kind(crate::error::Cgif {
            detail: "unbalanced ']' — unexpected close bracket".to_owned(),
        })),
        Some(Token::Colon) => Ok((CExpr::Colon, pos + 1)),
        Some(Token::Str(s)) => Ok((CExpr::Str(s.clone()), pos + 1)),
        Some(Token::Bound(s)) => Ok((CExpr::Bound(s.clone()), pos + 1)),
        Some(Token::Def(s)) => Ok((CExpr::Def(s.clone()), pos + 1)),
        Some(Token::At(s)) => Ok((CExpr::At(s.clone()), pos + 1)),
        Some(Token::Sym(s)) => Ok((CExpr::Sym(s.clone()), pos + 1)),
        Some(Token::Tilde) => {
            let (inner, next) = parse_one(tokens, pos + 1)?;
            Ok((CExpr::Neg(Box::new(inner)), next))
        }
        Some(Token::Open) => {
            let (items, next) = parse_seq(tokens, pos + 1, &Token::Close, "'('")?;
            Ok((CExpr::Paren(items), next))
        }
        Some(Token::OpenBrack) => {
            let (items, next) = parse_seq(tokens, pos + 1, &Token::CloseBrack, "'['")?;
            Ok((CExpr::Brack(items), next))
        }
    }
}

/// Parse a sequence of sub-expressions until `closer`, returning the items and the index past
/// the closing delimiter. `opener` names the opening delimiter for error messages.
fn parse_seq(
    tokens: &[Token],
    start: usize,
    closer: &Token,
    opener: &str,
) -> gmeow_errors::Result<(Vec<CExpr>, usize)> {
    let mut items = Vec::new();
    let mut i = start;
    loop {
        match tokens.get(i) {
            None => {
                return Err(Diag::of_kind(crate::error::Cgif {
                    detail: format!("unbalanced {opener} — missing close at end of input"),
                }));
            }
            Some(t) if t == closer => return Ok((items, i + 1)),
            Some(_) => {
                let (item, next) = parse_one(tokens, i)?;
                items.push(item);
                i = next;
            }
        }
    }
}

// --------------------------------------------------------------------------- //
// Top-form partitioning (writer-side sentinel detection for the reader)
// --------------------------------------------------------------------------- //

/// Partition the source into (graph source text, RDF-meta source text). Everything from the
/// [`RDF_META_SENTINEL`] line to end-of-input is the RDF-meta block; everything before it is
/// the idiomatic graph channel. The sentinel is a `/* … */` comment, so each half is
/// independently lexable.
pub(crate) fn split_on_sentinel(src: &str) -> (String, String) {
    // Match the sentinel only as a WHOLE line (the writer emits it on its own line), so a CGIF
    // name / string that happens to contain the sentinel text never mis-splits the document.
    let mut offset = 0;
    for line in src.split_inclusive('\n') {
        let trimmed = line.trim_end_matches('\n').trim_end_matches('\r');
        if trimmed == RDF_META_SENTINEL {
            return (src[..offset].to_owned(), src[offset..].to_owned());
        }
        offset += line.len();
    }
    (src.to_owned(), String::new())
}
