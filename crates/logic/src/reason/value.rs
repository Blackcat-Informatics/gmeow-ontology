// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Parsed literal values shared by native DL and fragment refutation.
//! RDF term identity is retained independently of the interpreted value. Parsing
//! happens at admission to an analysis, never in its pairwise comparison loop.

use purrdf::xsd::{XsdValue, exact::Rational};
use purrdf::{RdfTextDirection, TermValue};
use purrdf_hash::fixed::FixedState;

const OWL_RATIONAL: &str = "http://www.w3.org/2002/07/owl#rational";
const OWL_REAL: &str = "http://www.w3.org/2002/07/owl#real";

#[derive(Clone, Debug)]
pub(crate) enum LiteralMeaning {
    Rational(Rational),
    Native(XsdValue),
    Language {
        text: String,
        language: String,
        direction: Option<RdfTextDirection>,
    },
    /// No general native value representation was admitted. Identical RDF terms
    /// are known equal. Other lexical differences are not evidence of unequal values.
    Opaque(Box<TermValue>),
}

#[derive(Clone, Debug)]
pub(crate) struct LiteralValue {
    pub(crate) meaning: LiteralMeaning,
}

impl LiteralValue {
    /// Membership in the named datatype map over the already interpreted native
    /// value. PurRDF owns its XSD spaces; exact rational and language/direction
    /// residue stays in the shared GMEOW value algebra. Unknown is never false.
    pub(crate) fn named_datatype(&self, datatype: &str) -> Option<bool> {
        let meaning = &self.meaning;
        if matches!(meaning, LiteralMeaning::Opaque(_)) {
            return None;
        }
        if datatype == "http://www.w3.org/2000/01/rdf-schema#Literal" {
            return Some(true);
        }
        if matches!(datatype, OWL_REAL | OWL_RATIONAL) {
            return Some(matches!(meaning, LiteralMeaning::Rational(_)));
        }
        if matches!(
            datatype,
            "http://www.w3.org/1999/02/22-rdf-syntax-ns#langString"
                | "http://www.w3.org/1999/02/22-rdf-syntax-ns#dirLangString"
        ) {
            return Some(
                matches!(meaning, LiteralMeaning::Language { direction, .. } if direction.is_some() == datatype.ends_with("#dirLangString")),
            );
        }
        let datatype = purrdf::xsd::XsdDatatype::from_iri(datatype)?;
        match meaning {
            LiteralMeaning::Rational(value) => {
                if datatype.integer_range().is_some() {
                    if !value.is_integer() {
                        return Some(false);
                    }
                    let integer = XsdValue::from_exact_integer(
                        value.numerator().clone(),
                        purrdf::xsd::XsdDatatype::Integer,
                    );
                    use purrdf::xsd::range::{DataRange, Known, contains};
                    return match contains(&DataRange::Datatype(datatype), &integer) {
                        Known::Yes => Some(true),
                        Known::No => Some(false),
                        Known::Unknown => None,
                    };
                }
                if datatype == purrdf::xsd::XsdDatatype::Decimal {
                    return Some(value.is_terminating());
                }
                Some(false)
            }
            LiteralMeaning::Language { .. } => Some(false),
            LiteralMeaning::Native(value) => {
                use purrdf::xsd::range::{DataRange, Known, contains};
                match contains(&DataRange::Datatype(datatype), value) {
                    Known::Yes => Some(true),
                    Known::No => Some(false),
                    Known::Unknown => None,
                }
            }
            LiteralMeaning::Opaque(_) => None,
        }
    }

    /// Facet order within one admitted value space. PurRDF supplies XSD partial
    /// orders; its SPARQL cross-space numeric promotion is deliberately excluded.
    pub(crate) fn facet_order(&self, other: &Self) -> Option<std::cmp::Ordering> {
        use XsdValue::{Date, DateTime, Double, Duration, Float, Gregorian, Time};
        match (&self.meaning, &other.meaning) {
            (LiteralMeaning::Rational(left), LiteralMeaning::Rational(right)) => {
                Some(left.cmp(right))
            }
            (LiteralMeaning::Native(left), LiteralMeaning::Native(right))
                if matches!(
                    (left, right),
                    (Float(_), Float(_))
                        | (Double(_), Double(_))
                        | (DateTime(_), DateTime(_))
                        | (Date(_), Date(_))
                        | (Time(_), Time(_))
                        | (Duration(_), Duration(_))
                        | (Gregorian(_), Gregorian(_))
                ) =>
            {
                purrdf::xsd::value_cmp(left, right)
            }
            _ => None,
        }
    }

    /// XSD length uses Unicode scalar values for strings and octets for binary
    /// values. Other value spaces do not silently acquire a length facet.
    pub(crate) fn facet_length(&self) -> Option<u128> {
        match &self.meaning {
            LiteralMeaning::Native(value) => value.facet_length().map(|length| length as u128),
            _ => None,
        }
    }

    /// Interpret the native term directly, without constructing or parsing an RDF
    /// surface. The source term remains separate from its datatype interpretation.
    fn native(value: &TermValue) -> Self {
        let TermValue::Literal {
            lexical_form,
            datatype,
            language,
            direction,
        } = value
        else {
            unreachable!("the native literal cache admits literals only")
        };
        Self::from_parts(
            lexical_form,
            datatype,
            language.as_deref(),
            *direction,
            || LiteralMeaning::Opaque(Box::new(value.clone())),
        )
    }

    fn from_parts(
        lexical_form: &str,
        datatype: &str,
        language: Option<&str>,
        direction: Option<RdfTextDirection>,
        opaque: impl Fn() -> LiteralMeaning,
    ) -> Self {
        let meaning = if let Some(language) = language {
            LiteralMeaning::Language {
                text: lexical_form.to_owned(),
                language: language.to_ascii_lowercase(),
                direction,
            }
        } else if datatype == OWL_RATIONAL {
            lexical_form
                .parse::<Rational>()
                .map(LiteralMeaning::Rational)
                .unwrap_or_else(|_| opaque())
        } else {
            match purrdf::xsd::parse_by_iri(lexical_form, datatype) {
                Ok(Some(value)) => match Rational::from_xsd(&value) {
                    Some(value) => LiteralMeaning::Rational(value),
                    None => LiteralMeaning::Native(value),
                },
                _ => opaque(),
            }
        };
        Self { meaning }
    }

    /// OWL value equality, without SPARQL numeric promotion. Unknown datatype
    /// mappings remain unknown. The IEEE signed zeros remain distinct OWL values.
    pub(crate) fn same_value(&self, other: &Self) -> Option<bool> {
        match (&self.meaning, &other.meaning) {
            (LiteralMeaning::Opaque(a), LiteralMeaning::Opaque(b)) if a == b => Some(true),
            (LiteralMeaning::Opaque(_), _) | (_, LiteralMeaning::Opaque(_)) => None,
            (LiteralMeaning::Rational(a), LiteralMeaning::Rational(b)) => Some(a == b),
            (
                LiteralMeaning::Language {
                    text: a,
                    language: al,
                    direction: ad,
                },
                LiteralMeaning::Language {
                    text: b,
                    language: bl,
                    direction: bd,
                },
            ) => Some((a, al, ad) == (b, bl, bd)),
            (
                LiteralMeaning::Native(XsdValue::Float(a)),
                LiteralMeaning::Native(XsdValue::Float(b)),
            ) => Some((a.is_nan() && b.is_nan()) || a.to_bits() == b.to_bits()),
            (
                LiteralMeaning::Native(XsdValue::Double(a)),
                LiteralMeaning::Native(XsdValue::Double(b)),
            ) => Some((a.is_nan() && b.is_nan()) || a.to_bits() == b.to_bits()),
            (LiteralMeaning::Native(a), LiteralMeaning::Native(b)) => {
                Some(purrdf::xsd::same_value(a, b))
            }
            _ => Some(false),
        }
    }
}

/// Parsed values shared by every native schema operator in one world execution.
/// At most 128 literals with at most 8 KiB of source payload each are retained.
/// Larger values and cache misses still receive the exact same interpretation;
/// A separate bounded table retains at most 128 parsed cardinality fields with
/// the same source-payload limit. No facts, worlds or complete datasets are cached.
#[derive(Default)]
pub(crate) struct NativeValues {
    entries: std::collections::HashMap<TermValue, std::sync::Arc<LiteralValue>, FixedState>,
    counts: std::collections::HashMap<TermValue, u128, FixedState>,
}

impl NativeValues {
    /// The intrinsic datatype map has no source-defined constructor dependency.
    pub(crate) fn is_named_datatype(datatype: &str) -> bool {
        purrdf::xsd::XsdDatatype::from_iri(datatype).is_some()
            || matches!(
                datatype,
                OWL_REAL
                    | OWL_RATIONAL
                    | "http://www.w3.org/2000/01/rdf-schema#Literal"
                    | "http://www.w3.org/1999/02/22-rdf-syntax-ns#langString"
                    | "http://www.w3.org/1999/02/22-rdf-syntax-ns#dirLangString"
            )
    }
    /// Native cardinality fields share the projection's explicit lexical-count
    /// convention for strings. Other literals must inhabit an integer datatype;
    /// fractional values, language tags and invalid subtype values are refused.
    pub(crate) fn parse_cardinality(source: &TermValue) -> Option<u128> {
        let TermValue::Literal {
            lexical_form,
            datatype,
            language: None,
            direction: None,
        } = source
        else {
            return None;
        };
        let datatype = if datatype == "http://www.w3.org/2001/XMLSchema#string" {
            purrdf::xsd::XsdDatatype::NonNegativeInteger
        } else {
            purrdf::xsd::XsdDatatype::from_iri(datatype)?
        };
        datatype.integer_range()?;
        let XsdValue::Integer { value, .. } = purrdf::xsd::parse(lexical_form, datatype).ok()?
        else {
            return None;
        };
        if value.is_zero() {
            Some(0)
        } else if value.is_negative() {
            None
        } else {
            // Native parsing already validated the datatype and integer value.
            // Convert the count's own u128 representation without an i128 limit
            // or an intermediate arbitrary-precision magnitude allocation.
            lexical_form.trim().parse().ok()
        }
    }

    pub(crate) fn cardinality(&mut self, source: &TermValue) -> Option<u128> {
        if let Some(count) = self.counts.get(source) {
            return Some(*count);
        }
        let count = Self::parse_cardinality(source)?;
        let cacheable = matches!(
            source,
            TermValue::Literal {
                lexical_form,
                datatype,
                ..
            } if lexical_form.len().saturating_add(datatype.len()) <= 8 * 1024
        );
        if self.counts.len() < 128 && cacheable {
            self.counts.insert(source.clone(), count);
        }
        Some(count)
    }

    /// Reuse one native literal interpretation. The caller must supply a literal.
    pub(crate) fn literal(&mut self, source: &TermValue) -> std::sync::Arc<LiteralValue> {
        if let Some(value) = self.entries.get(source) {
            return std::sync::Arc::clone(value);
        }
        let value = std::sync::Arc::new(LiteralValue::native(source));
        let TermValue::Literal {
            lexical_form,
            datatype,
            language,
            ..
        } = source
        else {
            unreachable!("literal interpretation requires a literal")
        };
        let payload = lexical_form
            .len()
            .saturating_add(datatype.len())
            .saturating_add(language.as_ref().map_or(0, String::len));
        if self.entries.len() < 128 && payload <= 8 * 1024 {
            self.entries
                .insert(source.clone(), std::sync::Arc::clone(&value));
        }
        value
    }

    /// Native value equality, retaining literal language/direction and unknown
    /// datatype mappings. Resources and quoted terms retain exact term identity.
    pub(crate) fn same_value(&mut self, left: &TermValue, right: &TermValue) -> Option<bool> {
        if left == right {
            return Some(true);
        }
        match (left, right) {
            (TermValue::Literal { .. }, TermValue::Literal { .. }) => {
                self.literal(left).same_value(&self.literal(right))
            }
            _ => Some(false),
        }
    }
}

#[path = "value.native_cache_tests.rs"]
#[cfg(test)]
mod native_cache_tests;

#[path = "value.tests.rs"]
#[cfg(test)]
mod tests;
