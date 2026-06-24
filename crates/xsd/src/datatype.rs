// SPDX-FileCopyrightText: 2026 Blackcat Informatics Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The XSD datatype vocabulary this crate's value space covers.
//!
//! The IRI string constants are **value-identical** to the ones used elsewhere in
//! the workspace (e.g. `XSD_STRING` in `gmeow-rdf-core`'s `ir/term.rs`). They are
//! copied here deliberately: `gmeow-xsd` is a leaf crate and does not (yet) share a
//! symbol with `gmeow-rdf-core` (whose copies are `pub(crate)` and which does not
//! depend on this crate). The [`tests`] module pins the exact strings so the copies
//! cannot silently drift; de-duplicating into a single source is a later slice.

/// The XML Schema datatype namespace.
pub const XSD_NS: &str = "http://www.w3.org/2001/XMLSchema#";

/// `xsd:integer` — arbitrary-magnitude (this crate: `i128`-bounded) signed integer.
pub const XSD_INTEGER: &str = "http://www.w3.org/2001/XMLSchema#integer";
/// `xsd:decimal` — exact decimal (this crate: `i128` mantissa, fixed scale).
pub const XSD_DECIMAL: &str = "http://www.w3.org/2001/XMLSchema#decimal";
/// `xsd:float` — IEEE single-precision.
pub const XSD_FLOAT: &str = "http://www.w3.org/2001/XMLSchema#float";
/// `xsd:double` — IEEE double-precision.
pub const XSD_DOUBLE: &str = "http://www.w3.org/2001/XMLSchema#double";
/// `xsd:boolean`.
pub const XSD_BOOLEAN: &str = "http://www.w3.org/2001/XMLSchema#boolean";
/// `xsd:string`.
pub const XSD_STRING: &str = "http://www.w3.org/2001/XMLSchema#string";
/// `xsd:date`.
pub const XSD_DATE: &str = "http://www.w3.org/2001/XMLSchema#date";
/// `xsd:time`.
pub const XSD_TIME: &str = "http://www.w3.org/2001/XMLSchema#time";
/// `xsd:dateTime`.
pub const XSD_DATE_TIME: &str = "http://www.w3.org/2001/XMLSchema#dateTime";
/// `xsd:duration` — the general duration (months + seconds; partial order).
pub const XSD_DURATION: &str = "http://www.w3.org/2001/XMLSchema#duration";
/// `xsd:dayTimeDuration` — totally-ordered duration subtype (seconds only).
pub const XSD_DAY_TIME_DURATION: &str = "http://www.w3.org/2001/XMLSchema#dayTimeDuration";
/// `xsd:yearMonthDuration` — totally-ordered duration subtype (months only).
pub const XSD_YEAR_MONTH_DURATION: &str = "http://www.w3.org/2001/XMLSchema#yearMonthDuration";

/// The XSD datatypes whose **value space** `gmeow-xsd` models.
///
/// This is a closed set by design: XSD does not grow at runtime, so dispatch over
/// this enum is closed-but-correct (no runtime registry). A datatype IRI outside
/// this set is "not an XSD value-space type" — the caller treats such a literal as
/// a plain term (see `parse_by_iri` returning `Ok(None)`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum XsdDatatype {
    /// `xsd:integer`.
    Integer,
    /// `xsd:decimal`.
    Decimal,
    /// `xsd:float`.
    Float,
    /// `xsd:double`.
    Double,
    /// `xsd:boolean`.
    Boolean,
    /// `xsd:string`.
    String,
    /// `xsd:date`.
    Date,
    /// `xsd:time`.
    Time,
    /// `xsd:dateTime`.
    DateTime,
    /// `xsd:duration`.
    Duration,
    /// `xsd:dayTimeDuration`.
    DayTimeDuration,
    /// `xsd:yearMonthDuration`.
    YearMonthDuration,
}

impl XsdDatatype {
    /// Resolve a datatype IRI to its [`XsdDatatype`], or `None` when the IRI is not
    /// one of the XSD value-space datatypes this crate models.
    #[must_use]
    pub fn from_iri(iri: &str) -> Option<Self> {
        Some(match iri {
            XSD_INTEGER => Self::Integer,
            XSD_DECIMAL => Self::Decimal,
            XSD_FLOAT => Self::Float,
            XSD_DOUBLE => Self::Double,
            XSD_BOOLEAN => Self::Boolean,
            XSD_STRING => Self::String,
            XSD_DATE => Self::Date,
            XSD_TIME => Self::Time,
            XSD_DATE_TIME => Self::DateTime,
            XSD_DURATION => Self::Duration,
            XSD_DAY_TIME_DURATION => Self::DayTimeDuration,
            XSD_YEAR_MONTH_DURATION => Self::YearMonthDuration,
            _ => return None,
        })
    }

    /// The canonical datatype IRI for this value-space datatype.
    #[must_use]
    pub const fn iri(self) -> &'static str {
        match self {
            Self::Integer => XSD_INTEGER,
            Self::Decimal => XSD_DECIMAL,
            Self::Float => XSD_FLOAT,
            Self::Double => XSD_DOUBLE,
            Self::Boolean => XSD_BOOLEAN,
            Self::String => XSD_STRING,
            Self::Date => XSD_DATE,
            Self::Time => XSD_TIME,
            Self::DateTime => XSD_DATE_TIME,
            Self::Duration => XSD_DURATION,
            Self::DayTimeDuration => XSD_DAY_TIME_DURATION,
            Self::YearMonthDuration => XSD_YEAR_MONTH_DURATION,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn iri_round_trips_for_every_datatype() {
        for dt in [
            XsdDatatype::Integer,
            XsdDatatype::Decimal,
            XsdDatatype::Float,
            XsdDatatype::Double,
            XsdDatatype::Boolean,
            XsdDatatype::String,
            XsdDatatype::Date,
            XsdDatatype::Time,
            XsdDatatype::DateTime,
            XsdDatatype::Duration,
            XsdDatatype::DayTimeDuration,
            XsdDatatype::YearMonthDuration,
        ] {
            assert_eq!(XsdDatatype::from_iri(dt.iri()), Some(dt));
            assert!(dt.iri().starts_with(XSD_NS));
        }
    }

    #[test]
    fn non_xsd_iri_is_none() {
        assert_eq!(
            XsdDatatype::from_iri("http://www.w3.org/1999/02/22-rdf-syntax-ns#langString"),
            None
        );
        assert_eq!(XsdDatatype::from_iri("https://example.org/custom"), None);
    }

    /// Pins the exact IRI strings byte-for-byte (the value-equality guard described
    /// in the module docs — these must match `gmeow-rdf-core`'s `pub(crate)` copies).
    #[test]
    fn iri_constants_are_byte_exact() {
        assert_eq!(XSD_STRING, "http://www.w3.org/2001/XMLSchema#string");
        assert_eq!(XSD_INTEGER, "http://www.w3.org/2001/XMLSchema#integer");
        assert_eq!(XSD_DECIMAL, "http://www.w3.org/2001/XMLSchema#decimal");
        assert_eq!(XSD_BOOLEAN, "http://www.w3.org/2001/XMLSchema#boolean");
        assert_eq!(XSD_DOUBLE, "http://www.w3.org/2001/XMLSchema#double");
        assert_eq!(XSD_DATE_TIME, "http://www.w3.org/2001/XMLSchema#dateTime");
    }
}
