// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The CLIF (Common Logic Interchange Format) text dialect.
//!
//! CLIF is the ISO/IEC 24707 s-expression (Lisp-like) syntax for full first-order
//! Common Logic. This module is a **bidirectional, `PreservationKind::Exact`** dialect:
//! [`project_clif`] lowers a [`LogicProgram`](crate::ir::LogicProgram)
//! to CLIF text and [`parse_clif_str`] lifts it back, and the two
//! are inverses on the canonical IR (the production round-trip test pins this).
//!
//! ## The two-channel split that makes Exact genuine
//!
//! The IR is round-tripped through two disjoint channels:
//!
//! 1. **Idiomatic FOL channel** — `program.rules` + `program.formulas`. These become
//!    readable Common Logic sentences (`(forall …)`, `(if …)`, `(and …)`, `(or …)`,
//!    `(not …)`, `(exists …)`) by bespoke bidirectional code below.
//! 2. **RDF / predication channel** — everything else (axioms + scope, contracts,
//!    path shapes, correspondences, transaction programs). These are already flat RDF;
//!    the writer serializes them through the lossless canonical-RDF-1.2 projection and
//!    re-emits each triple as a CL predication, and the reader reconstructs the dataset
//!    and re-uses the canonical RDF frontend. The bidirectional faithfulness of that
//!    leg is therefore exactly the canonical-RDF-1.2 target's (already `Exact`).
//!
//! The s-expression lexer / printer are private helpers kept inside this module (no
//! separate `sexpr.rs`); [`writer`] and [`reader`] are the public dialect surface.

use gmeow_errors::Diag;

pub mod reader;
pub mod writer;

pub use reader::parse_clif_str;
pub use writer::project_clif;

#[cfg(test)]
mod tests;

// --------------------------------------------------------------------------- //
// The sentinel comment that delimits the RDF-meta block from the FOL channel.
// --------------------------------------------------------------------------- //

/// The sentinel comment that opens the RDF/predication meta block. The reader detects it
/// to route the predications that follow into the RDF channel rather than the FOL channel.
/// Kept as a `;`-comment so a generic CLIF consumer ignores it, while still being
/// machine-detectable.
pub(crate) const RDF_META_SENTINEL: &str = ";; @@gmeow-rdf-meta@@";

// --------------------------------------------------------------------------- //
// S-expression tree
// --------------------------------------------------------------------------- //

/// A parsed s-expression: an atom (a single token, with its lexical kind preserved) or a
/// parenthesized list. The atom keeps its **kind** so the reader can tell a quoted CL name
/// (`'iri'`) from a bare symbol (`forall`, `if`, `and`) from a variable (`?x`) from a
/// string (`"lex"`) — they are syntactically distinct in CLIF and must not be conflated.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum SExpr {
    /// A single atomic token.
    Atom(Atom),
    /// A parenthesized list of sub-expressions.
    List(Vec<SExpr>),
}

/// The lexical kind of an [`SExpr::Atom`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Atom {
    /// A bare symbol (`forall`, `if`, `and`, `lit`, `seq`, `/=`, …).
    Symbol(String),
    /// A `?`-prefixed variable (the leading `?` is stripped from the stored name).
    Var(String),
    /// A `'…'`-quoted CL name (an IRI). The stored value is the unescaped inner string.
    Name(String),
    /// A `"…"`-quoted string literal. The stored value is the unescaped inner string.
    Str(String),
    /// A `@lang` language tag (the leading `@` is stripped from the stored tag).
    Lang(String),
}

// --------------------------------------------------------------------------- //
// Printer
// --------------------------------------------------------------------------- //

/// Escape `"` and `\` inside a double-quoted string literal.
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

/// Escape `'` and `\` inside a single-quoted CL name.
pub(crate) fn escape_name(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    for c in s.chars() {
        match c {
            '\'' => out.push_str("\\'"),
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
    Atom(Atom),
}

/// Tokenize CLIF source, stripping `;`-to-EOL comments. Returns the token stream.
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
            // Comment to end of line.
            ';' => {
                while i < n && chars[i] != '\n' {
                    i += 1;
                }
            }
            '(' => {
                tokens.push(Token::Open);
                i += 1;
            }
            ')' => {
                tokens.push(Token::Close);
                i += 1;
            }
            // A `'…'`-quoted CL name.
            '\'' => {
                let (val, next) = lex_quoted(&chars, i + 1, '\'')?;
                tokens.push(Token::Atom(Atom::Name(val)));
                i = next;
            }
            // A `"…"`-quoted string.
            '"' => {
                let (val, next) = lex_quoted(&chars, i + 1, '"')?;
                tokens.push(Token::Atom(Atom::Str(val)));
                i = next;
            }
            // A `?var`.
            '?' => {
                let (val, next) = lex_bare(&chars, i + 1);
                if val.is_empty() {
                    return Err(Diag::of_kind(crate::error::Clif {
                        detail: "empty variable name after '?'".to_owned(),
                    }));
                }
                tokens.push(Token::Atom(Atom::Var(val)));
                i = next;
            }
            // A `@lang`.
            '@' => {
                let (val, next) = lex_bare(&chars, i + 1);
                if val.is_empty() {
                    return Err(Diag::of_kind(crate::error::Clif {
                        detail: "empty language tag after '@'".to_owned(),
                    }));
                }
                tokens.push(Token::Atom(Atom::Lang(val)));
                i = next;
            }
            // A bare symbol.
            _ => {
                let (val, next) = lex_bare(&chars, i);
                tokens.push(Token::Atom(Atom::Symbol(val)));
                i = next;
            }
        }
    }
    Ok(tokens)
}

/// Read a `'…'` / `"…"`-quoted token body (starting AFTER the opening quote), honoring
/// `\\` and `\<quote>` escapes. Returns the unescaped value and the index past the
/// closing quote.
fn lex_quoted(chars: &[char], start: usize, quote: char) -> gmeow_errors::Result<(String, usize)> {
    let mut out = String::new();
    let mut i = start;
    let n = chars.len();
    while i < n {
        let c = chars[i];
        if c == '\\' {
            if i + 1 >= n {
                return Err(Diag::of_kind(crate::error::Clif {
                    detail: "trailing backslash in quoted token".to_owned(),
                }));
            }
            let next = chars[i + 1];
            // Only `\\` and `\<quote>` are recognized escapes (matching the writer).
            out.push(next);
            i += 2;
            continue;
        }
        if c == quote {
            return Ok((out, i + 1));
        }
        out.push(c);
        i += 1;
    }
    Err(Diag::of_kind(crate::error::Clif {
        detail: format!("unterminated quoted token (expected closing {quote})"),
    }))
}

/// Read a bare token (symbol / variable name / language tag) starting at `start`, stopping
/// at whitespace, a paren, or a quote/comment delimiter. Returns the token and the index
/// past it.
fn lex_bare(chars: &[char], start: usize) -> (String, usize) {
    let mut out = String::new();
    let mut i = start;
    let n = chars.len();
    while i < n {
        let c = chars[i];
        if c.is_whitespace() || matches!(c, '(' | ')' | '\'' | '"' | ';') {
            break;
        }
        out.push(c);
        i += 1;
    }
    (out, i)
}

/// Parse CLIF source into a flat list of top-level [`SExpr`] forms.
fn parse_sexprs(src: &str) -> gmeow_errors::Result<Vec<SExpr>> {
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

/// Parse one s-expression starting at token index `pos`. Returns the expression and the
/// next token index.
fn parse_one(tokens: &[Token], pos: usize) -> gmeow_errors::Result<(SExpr, usize)> {
    match tokens.get(pos) {
        None => Err(Diag::of_kind(crate::error::Clif {
            detail: "unexpected end of input while parsing s-expression".to_owned(),
        })),
        Some(Token::Close) => Err(Diag::of_kind(crate::error::Clif {
            detail: "unbalanced ')' — unexpected close paren".to_owned(),
        })),
        Some(Token::Atom(a)) => Ok((SExpr::Atom(a.clone()), pos + 1)),
        Some(Token::Open) => {
            let mut items = Vec::new();
            let mut i = pos + 1;
            loop {
                match tokens.get(i) {
                    None => {
                        return Err(Diag::of_kind(crate::error::Clif {
                            detail: "unbalanced '(' — missing ')' at end of input".to_owned(),
                        }));
                    }
                    Some(Token::Close) => return Ok((SExpr::List(items), i + 1)),
                    Some(_) => {
                        let (item, next) = parse_one(tokens, i)?;
                        items.push(item);
                        i = next;
                    }
                }
            }
        }
    }
}

// --------------------------------------------------------------------------- //
// Top-form partitioning (writer-side sentinel detection for the reader)
// --------------------------------------------------------------------------- //

/// Partition the source into (FOL source text, RDF-meta source text). Everything from the
/// [`RDF_META_SENTINEL`] line to end-of-input is the RDF-meta block; everything before it is
/// the FOL channel. The sentinel is a comment, so each half is independently lexable.
pub(crate) fn split_on_sentinel(src: &str) -> (String, String) {
    // Match the sentinel only as a WHOLE line (the writer emits it on its own line), so a CL
    // name or string literal that happens to contain the sentinel text never mis-splits the
    // document into corrupted FOL/meta halves.
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
