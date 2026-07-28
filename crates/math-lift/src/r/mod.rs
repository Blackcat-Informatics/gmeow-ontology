// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The R → `math:` bridge: the flagship ingestion front-end.
//!
//! `MATHEMATICS-BRIDGES.md` states the bar plainly:
//!
//! > **Flagship — R, for any script.** Answerable when the bridge lifts an arbitrary R
//! > script's statistical content into `math:` and its computation into `logic:`, retains
//! > the source as `math:parseSource`, records any loss, and **hard-fails** with a typed
//! > diagnostic on anything it cannot lift — never emitting a degraded or string-valued
//! > placeholder.
//!
//! The bridge is split into the two tiers the crate doc mandates:
//!
//! | tier | module | contract |
//! |---|---|---|
//! | parse | [`lexer`] + [`parser`] | bytes → a typed AST, no RDF, no ontology in the loop |
//! | lift | [`mod@lift`] | AST → `math:` triples, no parsing |
//!
//! Keeping them apart is what lets the grammar be tested against real R without an
//! ontology, and what keeps the lift honest about which structure it is actually carrying
//! across. There is no third tier and no fallback path: a script this bridge cannot
//! fully structure produces NO triples.
//!
//! # Entry point
//!
//! [`lift()`] takes the source bytes and the IRI base to mint under. The shipped
//! CLI hands it bytes read from a user's path; an in-bundle producer would hand it bytes
//! embedded at compile time. One implementation, two byte sources.

pub mod lexer;
pub mod lift;
pub mod parser;

pub use lexer::{Lexer, Op, Tok, Token, lex};
pub use lift::lift;
pub use parser::{
    Arg, AssignKind, BinaryOp, Formula, FormulaTerm, Param, RExpr, RScript, RStmt, RStmtKind,
    TermKind, UnaryOp, desugar_pipe, parse,
};
