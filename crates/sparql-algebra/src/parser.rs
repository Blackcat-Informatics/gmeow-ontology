// SPDX-FileCopyrightText: 2026 Blackcat Informatics Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The public parse entry point: SPARQL text → [`Query`] algebra.
//!
//! The full pipeline (lex → recursive-descent parse → §18.2 algebra translation)
//! is built incrementally across purrdf S5 tasks; this module owns the stable
//! [`SparqlParser`] surface the consumers call (`new()` + `parse_query`).

use crate::algebra::Query;
use crate::error::{ParseError, Result};

/// A reusable SPARQL query parser.
///
/// Mirrors the `spargebra::SparqlParser` surface the existing consumers call so
/// the port is mechanical: `SparqlParser::new().parse_query(text)`.
#[derive(Clone, Debug, Default)]
pub struct SparqlParser {
    base_iri: Option<String>,
}

impl SparqlParser {
    /// Construct a parser with no implicit base IRI.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set an implicit base IRI used to resolve relative IRI references that
    /// appear before any in-query `BASE` declaration.
    pub fn with_base_iri(mut self, base_iri: impl Into<String>) -> Self {
        self.base_iri = Some(base_iri.into());
        self
    }

    /// Parse a SPARQL 1.1/1.2 query into the algebra.
    pub fn parse_query(&self, query: &str) -> Result<Query> {
        // Scaffold: the lexer/parser/translation land in subsequent S5 tasks.
        // Hard-fail (no silent degradation) until then.
        let _ = (query, &self.base_iri);
        Err(ParseError::unsupported("parser not yet implemented"))
    }
}
