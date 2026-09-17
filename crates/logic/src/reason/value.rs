// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Parsed literal values shared by native DL and fragment refutation.
//! RDF term identity is retained independently of the interpreted value. Parsing
//! happens at admission to an analysis, never in its pairwise comparison loop.

use purrdf::xsd::{XsdValue, rational::Rational};
use purrdf::{RdfTextDirection, TermValue};

const OWL_RATIONAL: &str = "http://www.w3.org/2002/07/owl#rational";
const OWL_REAL: &str = "http://www.w3.org/2002/07/owl#real";
const XSD_DECIMAL: &str = "http://www.w3.org/2001/XMLSchema#decimal";

#[derive(Clone, Debug)]
pub(crate) enum LiteralMeaning {
    Rational(Rational),
    Native(XsdValue),
    Language {
        text: String,
        language: String,
        direction: Option<RdfTextDirection>,
    },
    /// No datatype interpretation was admitted. Only identical RDF terms are
    /// known equal; different lexical forms are not evidence of unequal values.
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
                if let Some((lo, hi)) = datatype.integer_range() {
                    return Some(
                        value.denominator() == 1 && (lo..=hi).contains(&value.numerator()),
                    );
                }
                if datatype == purrdf::xsd::XsdDatatype::Decimal {
                    let mut denominator = value.denominator();
                    for factor in [2, 5] {
                        while denominator % factor == 0 {
                            denominator /= factor;
                        }
                    }
                    return Some(denominator == 1);
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
            LiteralMeaning::Native(XsdValue::String(value)) => Some(value.chars().count() as u128),
            LiteralMeaning::Native(XsdValue::Binary { bytes, .. }) => Some(bytes.len() as u128),
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
        } else {
            let lexical = if datatype == "http://www.w3.org/2001/XMLSchema#string" {
                lexical_form
            } else {
                lexical_form.trim()
            };
            if matches!(datatype, OWL_RATIONAL | OWL_REAL) {
                rational(lexical)
                    .map(LiteralMeaning::Rational)
                    .unwrap_or_else(opaque)
            } else {
                match purrdf::xsd::parse_by_iri(lexical, datatype) {
                    Ok(Some(value)) => match Rational::from_xsd(&value) {
                        Some(value) => LiteralMeaning::Rational(value),
                        None => LiteralMeaning::Native(value),
                    },
                    // Preserve the native exact-rational domain beyond the fixed
                    // decimal scale of XsdValue; integer subtype failures never
                    // pass through this wider decimal admission.
                    Err(_) if datatype == XSD_DECIMAL => rational(lexical)
                        .map(LiteralMeaning::Rational)
                        .unwrap_or_else(opaque),
                    _ => opaque(),
                }
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
    entries: std::collections::HashMap<TermValue, std::sync::Arc<LiteralValue>>,
    counts: std::collections::HashMap<TermValue, u128>,
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
    pub(crate) fn cardinality(&mut self, source: &TermValue) -> Option<u128> {
        if let Some(count) = self.counts.get(source) {
            return Some(*count);
        }
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
        let count = u128::try_from(value).ok()?;
        if self.counts.len() < 128
            && lexical_form.len().saturating_add(datatype.iri().len()) <= 8 * 1024
        {
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

/// Exact rational parsing shared by the value and datatype-obligation readers.
pub(crate) fn rational(lexical: &str) -> Option<Rational> {
    let lexical = lexical.trim();
    if lexical.contains('/') {
        return Rational::parse(lexical).ok();
    }
    let value = purrdf::xsd::parse_by_iri(lexical, XSD_DECIMAL)
        .ok()
        .flatten()
        .and_then(|value| Rational::from_xsd(&value));
    value.or_else(|| {
        let value = super::refute::parse_rational(lexical)?;
        Rational::new(value.numerator(), value.denominator()).ok()
    })
}
