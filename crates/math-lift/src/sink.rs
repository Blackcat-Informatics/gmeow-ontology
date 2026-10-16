// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The RDF emitter every lift writes through.
//!
//! A thin accumulator over `purrdf::RdfDatasetBuilder`. Two properties are load-bearing:
//!
//! - **Canonical ordering is the codec's job.** The builder freezes and the Turtle
//!   serializer sorts, so a lift may emit triples in whatever order its walk produces and
//!   still get byte-identical output. Building Turtle by string concatenation (as the
//!   older in-bundle producers do) would make determinism a promise this crate has to keep
//!   by hand on every future edit.
//! - **Literals go through the RDF 1.2 value space**, so a datatype or language tag is a
//!   real term rather than a hand-escaped suffix.
//!
//! Mirrors `crates/affect-ingest`'s `Sink` and the projection `TripleSink` at
//! `crates/logic-compile/src/projections/rdf.rs`. That the shape appears a third time is
//! the established norm here, not drift: each lives in a crate that must not depend on
//! the others.
//!
//! # No `x-gmeow-*` language tags
//!
//! Lifted graphs are CONSUMER output — they leave through the shipped `gmeow` CLI.
//! `crates/gmeow-cli/tests/self_sufficiency.rs` asserts no `x-gmeow-*` private-use tag
//! leaks onto that surface, so this sink deliberately exposes no language-tagged
//! constructor. A lift emits IRIs, plain literals, and typed literals.

use purrdf::{RdfDatasetBuilder, RdfLiteral, SerializeGraph, serialize_dataset};

use crate::ns::{RDF_TYPE, XSD_BOOLEAN, XSD_DECIMAL, XSD_INTEGER};

/// A deterministic RDF accumulator.
#[derive(Default)]
pub struct Sink {
    builder: RdfDatasetBuilder,
}

impl Sink {
    /// A fresh, empty sink.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// `<s> <p> <o> .`
    ///
    /// # Errors
    /// Returns the original storage failure.
    pub fn iri(&mut self, s: &str, p: &str, o: &str) -> gmeow_errors::Result<()> {
        let s = self.builder.intern_iri(s)?;
        let p = self.builder.intern_iri(p)?;
        let o = self.builder.intern_iri(o)?;
        self.builder.push_quad(s, p, o, None)?;
        Ok(())
    }

    /// `<s> rdf:type <class> .`
    ///
    /// # Errors
    /// Returns the original storage failure.
    pub fn typed(&mut self, s: &str, class: &str) -> gmeow_errors::Result<()> {
        self.iri(s, RDF_TYPE, class)?;
        Ok(())
    }

    fn lit(&mut self, s: &str, p: &str, lit: RdfLiteral) -> gmeow_errors::Result<()> {
        let s = self.builder.intern_iri(s)?;
        let p = self.builder.intern_iri(p)?;
        let o = self.builder.intern_literal(lit)?;
        self.builder.push_quad(s, p, o, None)?;
        Ok(())
    }

    /// A plain (untyped, untagged) string literal.
    ///
    /// # Errors
    /// Returns the original storage failure.
    pub fn string(&mut self, s: &str, p: &str, value: &str) -> gmeow_errors::Result<()> {
        self.lit(s, p, RdfLiteral::simple(value))?;
        Ok(())
    }

    /// An `xsd:boolean` literal.
    ///
    /// # Errors
    /// Returns the original storage failure.
    pub fn boolean(&mut self, s: &str, p: &str, value: bool) -> gmeow_errors::Result<()> {
        self.lit(
            s,
            p,
            RdfLiteral::typed(
                if value { "true" } else { "false" }.to_owned(),
                XSD_BOOLEAN.to_owned(),
            ),
        )?;
        Ok(())
    }

    /// An `xsd:integer` literal.
    ///
    /// # Errors
    /// Returns the original storage failure.
    pub fn integer(&mut self, s: &str, p: &str, value: i64) -> gmeow_errors::Result<()> {
        self.lit(
            s,
            p,
            RdfLiteral::typed(value.to_string(), XSD_INTEGER.to_owned()),
        )?;
        Ok(())
    }

    /// An `xsd:decimal` literal.
    ///
    /// Never emits scientific notation — `1e-7` is not a valid `xsd:decimal` lexical
    /// form, so a bare `{f64}` Display would silently produce an ill-typed literal.
    ///
    /// # Errors
    /// Returns the original storage failure.
    pub fn decimal(&mut self, s: &str, p: &str, value: f64) -> gmeow_errors::Result<()> {
        self.lit(
            s,
            p,
            RdfLiteral::typed(format_decimal(value), XSD_DECIMAL.to_owned()),
        )?;
        Ok(())
    }

    /// Freeze and serialize as canonical Turtle.
    ///
    /// # Errors
    /// Returns the original storage, codec or UTF-8 failure.
    pub fn serialize(self) -> gmeow_errors::Result<String> {
        let dataset = self.builder.freeze()?;
        let bytes = serialize_dataset(
            dataset.as_ref(),
            "text/turtle",
            SerializeGraph::DefaultGraph,
        )?;
        Ok(String::from_utf8(bytes)?)
    }
}

/// `f64` → a valid `xsd:decimal` lexical form (never exponent notation, always a point).
fn format_decimal(value: f64) -> String {
    let s = format!("{value}");
    debug_assert!(
        !s.contains(['e', 'E']),
        "f64 Display must never emit scientific notation (invalid xsd:decimal): {s}"
    );
    if s.contains('.') { s } else { format!("{s}.0") }
}

#[path = "sink.tests.rs"]
#[cfg(test)]
mod tests;
