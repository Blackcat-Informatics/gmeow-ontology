// SPDX-FileCopyrightText: 2026 Blackcat Informatics Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! `gmeow-sparql-algebra` — the native **SPARQL 1.1/1.2 front-end** for the RDF
//! 1.2 query stack.
//!
//! A pure-Rust, wasm-clean leaf crate that parses SPARQL query text into a
//! gmeow-owned, **RDF 1.2-native** query algebra ([`Query`]/[`GraphPattern`]).
//! It is the drop-in replacement for the oxigraph-family SPARQL parser (purrdf S5,
//! EPIC #906) and the front-end the downstream evaluator S6 (`sparql-eval`,
//! #912) consumes. It builds only on the two CLOSED foundation leaves
//! [`gmeow_iri`] (#908) and [`gmeow_xsd`] (#907), and deliberately does **not**
//! depend on `gmeow-rdf-core`.
//!
//! # Scope (purrdf S5)
//!
//! Parse + algebra **only** — no evaluation. The in-scope SPARQL surface is
//! corpus-driven (the project's `queries/**/*.rq` plus the DSL-generated
//! projections) and covers both halves of SPARQL 1.1/1.2:
//!
//! - **Query**: the four query forms, basic graph patterns, `OPTIONAL`,
//!   `UNION`, `MINUS`, `GRAPH`, `FILTER`/`BIND`/`VALUES`, property paths,
//!   `GROUP BY`/aggregates, `EXISTS`/`NOT EXISTS`, solution modifiers, and the
//!   RDF 1.2 quoted triple terms (`<<( s p o )>>`) used by the codec round-trips.
//! - **Update** ([`Update`]/[`GraphUpdateOperation`]): `INSERT DATA` /
//!   `DELETE DATA`, the `DELETE`/`INSERT … WHERE` family (`WITH`/`USING`,
//!   `DELETE WHERE`), `LOAD`, and the graph-management operations
//!   `CLEAR`/`DROP`/`CREATE`/`ADD`/`MOVE`/`COPY`.
//!
//! Anything outside this surface is a hard [`ParseError::Unsupported`], never a
//! silently-degraded parse.
//!
//! # Hard-fail
//!
//! Per the repo `no-optionality` doctrine, every malformed or out-of-scope query
//! is a typed [`ParseError`]; there is no lenient mode and no partial algebra.
//!
//! # S6 reasoner-delegation seam
//!
//! The algebra is a faithful, standard, evaluable IR. Exploiting the native
//! OWL/EL-DL reasoner (entailment-regime-aware matching, path→subsumption-closure
//! delegation) is an *evaluation* concern that belongs in S6; this crate keeps
//! its own enums so S6 can grow annotations/variants without a breaking re-clone.
//! See the [`algebra`] module docs.

#![forbid(unsafe_code)]

pub mod algebra;
pub mod ast;
pub mod error;
pub mod lexer;
pub mod parser;

pub use algebra::{
    AggregateExpression, AggregateFunction, Expression, Function, GraphPattern, GraphTarget,
    GraphUpdateOperation, OrderExpression, PropertyPathExpression, Query, Update,
};
pub use ast::{
    BaseDirection, BlankNode, GroundTerm, GroundTriple, Literal, NamedNode, NamedNodePattern,
    QuadPattern, TermPattern, TriplePattern, Variable,
};
pub use error::{ParseError, Result};
pub use parser::SparqlParser;
