// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Quantitative coordinates retain RDF term identity independently of numeric value.
//! PurRDF owns decoding and comparison; construction caches the decoded value once.

use std::cmp::Ordering;

use purrdf::{
    RdfLiteral, TermRef, TermValue,
    xsd::{XsdDatatype, XsdValue},
};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// A finite RDF numeric literal with a statically selected range contract.
/// The private fields prevent mutation from invalidating the decoded value. Equality
/// compares complete RDF identity, including datatype and lexical signed zero.
#[derive(Debug, Clone)]
pub struct NumericLiteral<const UNIT_INTERVAL: bool> {
    literal: RdfLiteral,
    value: XsdValue,
}

/// A finite solver weight, without a unit-interval restriction.
pub type FiniteNumericLiteral = NumericLiteral<false>;
/// Confidence, evidence strength or probability in the closed interval `[0, 1]`.
/// Selecting this range does not choose a dependence or warrant model.
pub type UnitInterval = NumericLiteral<true>;

impl<const UNIT_INTERVAL: bool> NumericLiteral<UNIT_INTERVAL> {
    /// Admit the complete RDF literal, retaining its lexical form and datatype.
    ///
    /// # Errors
    /// Refuses malformed, nonnumeric, nonfinite or out-of-range values, including
    /// values outside the selected native datatype's representable precision.
    pub fn new(mut literal: RdfLiteral) -> gmeow_errors::Result<Self> {
        super::literal_serde::normalize(&mut literal).map_err(error)?;
        let value = purrdf::xsd::parse_by_iri(&literal.lexical_form, literal.datatype_iri())
            .map_err(|detail| error(detail.to_string()))?
            .ok_or_else(|| error("requires a supported RDF numeric datatype"))?;
        let finite = match &value {
            XsdValue::Integer { .. } | XsdValue::Decimal(_) => true,
            XsdValue::Float(value) => value.is_finite(),
            XsdValue::Double(value) => value.is_finite(),
            _ => false,
        };
        if !finite {
            return Err(error("requires a finite RDF numeric value"));
        }
        if UNIT_INTERVAL {
            let integer = |value| XsdValue::Integer {
                value,
                datatype: XsdDatatype::Integer,
            };
            let below = purrdf::xsd::numeric::numeric_cmp(&value, &integer(0));
            let above = purrdf::xsd::numeric::numeric_cmp(&value, &integer(1));
            if !matches!(below, Some(Ordering::Equal | Ordering::Greater))
                || !matches!(above, Some(Ordering::Equal | Ordering::Less))
            {
                return Err(error("requires a value in [0, 1]"));
            }
        }
        Ok(Self { literal, value })
    }

    /// Admit a borrowed source term without rendering or reparsing RDF syntax.
    ///
    /// # Errors
    /// Refuses non-literals and the same value failures as [`Self::new`].
    pub fn from_term(
        dataset: &purrdf::RdfDataset,
        term: TermRef<'_>,
    ) -> gmeow_errors::Result<Self> {
        let TermRef::Literal {
            lexical,
            datatype,
            language,
            direction,
        } = term
        else {
            return Err(error("requires an RDF numeric literal"));
        };
        let TermRef::Iri(datatype) = dataset.resolve(datatype) else {
            return Err(error("requires a named numeric datatype"));
        };
        Self::new(RdfLiteral {
            lexical_form: lexical.to_owned(),
            datatype: Some(datatype.to_owned()),
            language: language.map(str::to_owned),
            direction,
        })
    }

    /// The unchanged RDF coordinate for native transport and projection.
    pub fn literal(&self) -> &RdfLiteral {
        &self.literal
    }

    /// The already decoded scalar; numeric equality is distinct from term identity.
    pub fn value(&self) -> &XsdValue {
        &self.value
    }

    /// Length-framed full RDF identity for program and cache keys.
    pub fn content_key(&self) -> String {
        super::literal_serde::key(&self.literal)
    }
}

impl NumericLiteral<true> {
    /// Widen an already validated coordinate without decoding its RDF scalar again.
    pub fn into_finite(self) -> NumericLiteral<false> {
        NumericLiteral {
            literal: self.literal,
            value: self.value,
        }
    }

    /// Explicit lossy boundary for external SSSOM/EDOAL and presentation measures.
    /// This value must never be fed back into the canonical correspondence axes.
    pub fn projection_f64(&self) -> f64 {
        match &self.value {
            // A unit-interval integer can only be zero or one.
            XsdValue::Integer { value, .. } => {
                if *value == 0 {
                    0.0
                } else {
                    1.0
                }
            }
            XsdValue::Decimal(value) => value.to_f64(),
            XsdValue::Float(value) => f64::from(*value),
            XsdValue::Double(value) => *value,
            _ => unreachable!("numeric literal construction admits only finite numerics"),
        }
    }
}

impl<const UNIT_INTERVAL: bool> TryFrom<TermValue> for NumericLiteral<UNIT_INTERVAL> {
    type Error = gmeow_errors::Diag;

    fn try_from(term: TermValue) -> Result<Self, Self::Error> {
        let TermValue::Literal {
            lexical_form,
            datatype,
            language,
            direction,
        } = term
        else {
            return Err(error("requires an RDF numeric literal"));
        };
        Self::new(RdfLiteral {
            lexical_form,
            datatype: Some(datatype),
            language,
            direction,
        })
    }
}

impl<const UNIT_INTERVAL: bool> PartialEq for NumericLiteral<UNIT_INTERVAL> {
    fn eq(&self, other: &Self) -> bool {
        self.literal == other.literal
    }
}

impl<const UNIT_INTERVAL: bool> Eq for NumericLiteral<UNIT_INTERVAL> {}

impl<const UNIT_INTERVAL: bool> PartialOrd for NumericLiteral<UNIT_INTERVAL> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl<const UNIT_INTERVAL: bool> Ord for NumericLiteral<UNIT_INTERVAL> {
    fn cmp(&self, other: &Self) -> Ordering {
        // Canonical RDF set order, never a substitute for numeric comparison.
        super::literal_serde::structural_cmp(&self.literal, &other.literal)
    }
}

impl<const UNIT_INTERVAL: bool> Serialize for NumericLiteral<UNIT_INTERVAL> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        super::literal_serde::single::serialize(&self.literal, serializer)
    }
}

impl<'de, const UNIT_INTERVAL: bool> Deserialize<'de> for NumericLiteral<UNIT_INTERVAL> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::new(super::literal_serde::single::deserialize(deserializer)?)
            .map_err(serde::de::Error::custom)
    }
}

fn error(detail: impl Into<String>) -> gmeow_errors::Diag {
    gmeow_errors::Diag::of_kind(crate::error::Ir {
        detail: format!("quantitative coordinate: {}", detail.into()),
    })
}
