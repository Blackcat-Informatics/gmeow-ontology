// SPDX-FileCopyrightText: 2026 Blackcat Informatics Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The [`XsdValue`] value type and the [`XsdError`] parse-failure type.
//!
//! `XsdValue` is a **value-space** representation: parsing maps a lexical form into
//! the abstract value it denotes. It is deliberately NOT a term-identity key —
//! parsing discards the lexical form, so `"1"^^xsd:integer` and `"01"^^xsd:integer`
//! both become [`XsdValue::Integer`]`(1)` even though they are DISTINCT RDF terms
//! (`sameTerm` is false). RDF term identity (`sameTerm`) is the IR's
//! `(lexical, datatype, language)` tuple, NOT this type. Consequently `XsdValue`
//! intentionally implements neither `PartialEq`/`Eq`/`Hash` (which would falsely
//! read as term identity) nor `PartialOrd`/`Ord` (value ordering is the partial
//! `value_cmp` free fn). It implements only `Clone`/`Debug`, so a consumer can cache
//! `HashMap<TermId, XsdValue>` keyed by the IR's `TermId`.

use crate::datatype::XsdDatatype;
use crate::numeric::Decimal;

/// A parsed XSD value (value space). Variants are added per datatype family across
/// the foundation tasks; numeric first.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum XsdValue {
    /// `xsd:integer` — `i128`-bounded (exceeds `oxsdatatypes`' `i64`).
    Integer(i128),
    /// `xsd:decimal` — exact fixed-point (`i128` mantissa + scale).
    Decimal(Decimal),
    /// `xsd:float` — IEEE single-precision.
    Float(f32),
    /// `xsd:double` — IEEE double-precision.
    Double(f64),
    /// `xsd:boolean`.
    Boolean(bool),
    /// `xsd:string` — the value space is the lexical space (no normalization).
    String(String),
}

impl XsdValue {
    /// The XSD datatype this value belongs to.
    #[must_use]
    pub fn datatype(&self) -> XsdDatatype {
        match self {
            XsdValue::Integer(_) => XsdDatatype::Integer,
            XsdValue::Decimal(_) => XsdDatatype::Decimal,
            XsdValue::Float(_) => XsdDatatype::Float,
            XsdValue::Double(_) => XsdDatatype::Double,
            XsdValue::Boolean(_) => XsdDatatype::Boolean,
            XsdValue::String(_) => XsdDatatype::String,
        }
    }

    /// The canonical lexical form of this value (XSD canonical mapping).
    #[must_use]
    pub fn canonical_lexical(&self) -> String {
        match self {
            XsdValue::Integer(i) => i.to_string(),
            XsdValue::Decimal(d) => d.canonical_lexical(),
            XsdValue::Float(f) => crate::numeric::canonical_float(*f),
            XsdValue::Double(d) => crate::numeric::canonical_double(*d),
            XsdValue::Boolean(b) => if *b { "true" } else { "false" }.to_string(),
            XsdValue::String(s) => s.clone(),
        }
    }
}

/// Parse a lexical form into the XSD value space for a known [`XsdDatatype`].
///
/// Hard-fails on malformed input. This is the interning entry point: a consumer
/// parses once and caches the result keyed by the IR's `TermId` (the cache lives in
/// the consumer; this crate stays decoupled from `gmeow-rdf-core`).
pub fn parse(lexical: &str, datatype: XsdDatatype) -> Result<XsdValue, XsdError> {
    use XsdDatatype as D;
    match datatype {
        D::Integer => crate::numeric::parse_integer(lexical).map(XsdValue::Integer),
        D::Decimal => crate::numeric::parse_decimal(lexical).map(XsdValue::Decimal),
        D::Float => crate::numeric::parse_float(lexical).map(XsdValue::Float),
        D::Double => crate::numeric::parse_double(lexical).map(XsdValue::Double),
        D::Boolean => crate::simple::parse_boolean(lexical).map(XsdValue::Boolean),
        D::String => Ok(XsdValue::String(lexical.to_string())),
        D::Date
        | D::Time
        | D::DateTime
        | D::Duration
        | D::DayTimeDuration
        | D::YearMonthDuration => {
            // Temporal value space lands in the next task (#907 Task 4).
            Err(XsdError::InvalidLexical {
                datatype,
                lexical: lexical.to_string(),
                reason: "temporal datatypes not yet implemented",
            })
        }
    }
}

/// Parse a lexical form by datatype IRI.
///
/// Returns `Ok(None)` when `datatype_iri` is **not** an XSD value-space datatype —
/// the caller then treats the literal as a plain (opaque) term. `Err` means the IRI
/// *is* an XSD value-space datatype but the lexical form is invalid. This cleanly
/// separates "unknown datatype" from "malformed lexical".
pub fn parse_by_iri(lexical: &str, datatype_iri: &str) -> Result<Option<XsdValue>, XsdError> {
    match XsdDatatype::from_iri(datatype_iri) {
        Some(dt) => parse(lexical, dt).map(Some),
        None => Ok(None),
    }
}

/// A failure to map a lexical form into the XSD value space. Malformed input is a
/// hard error (never a silent default), per the project's no-optionality rule.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum XsdError {
    /// The lexical form is not valid for the target datatype.
    InvalidLexical {
        /// The datatype the lexical was being parsed as.
        datatype: XsdDatatype,
        /// The offending lexical form.
        lexical: String,
        /// A short, stable explanation.
        reason: &'static str,
    },
    /// The lexical form is well-formed but exceeds this crate's representable range
    /// (e.g. an integer beyond `i128`, or a decimal beyond `i128` mantissa). This is
    /// a deliberate hard-fail rather than saturation; bignum support is a deferred
    /// enhancement (it would only be needed by the future public purrdf).
    OutOfRange {
        /// The datatype the lexical was being parsed as.
        datatype: XsdDatatype,
        /// The offending lexical form.
        lexical: String,
    },
}

impl std::fmt::Display for XsdError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            XsdError::InvalidLexical {
                datatype,
                lexical,
                reason,
            } => write!(
                f,
                "invalid lexical form {lexical:?} for <{}>: {reason}",
                datatype.iri()
            ),
            XsdError::OutOfRange { datatype, lexical } => write!(
                f,
                "lexical form {lexical:?} is out of representable range for <{}>",
                datatype.iri()
            ),
        }
    }
}

impl std::error::Error for XsdError {}
