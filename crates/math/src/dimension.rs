// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Exact-rational ℚ⁷ SI-dimension algebra.
//!
//! A physical dimension is a point in the free ℚ-vector space over the seven SI
//! base dimensions (length, mass, time, electric current, temperature, amount of
//! substance, luminous intensity). A product of quantities adds their exponent
//! vectors; a power scales them; two quantities are *commensurable* exactly when
//! their exponent vectors are equal. This module owns that algebra as first-class,
//! graph-agnostic values ([`DimVector`]) plus the pure homogeneity decision and the
//! canonical `math:dimensionVector` render, so every consumer — the validate gate
//! and the native `math:` reasoning builtins — computes THROUGH one exact-rational
//! source rather than each re-deriving its own ℚ⁷ engine.
//!
//! The exponents are the shared [`Rational`] (`i128`-backed, gcd-normalized), so
//! equality is exact even for fractional dimensions such as `T^(-1/2)` (`√Hz`) and
//! [`DimVector`] is a sound `Eq`/`Hash` key.

use gmeow_errors::{Diag, Result};

use crate::error::MalformedDimension;
use crate::{Rational, TripleIndex, all_iris, first_i128, has_type};

/// Namespace root for the `math:` measure-and-dimension vocabulary.
const MATH: &str = "https://blackcatinformatics.ca/math/";

/// The number of SI base dimensions — the fixed rank of every [`DimVector`].
pub const BASE_DIMENSION_COUNT: usize = 7;

/// The seven SI base-dimension IRIs, in canonical ℚ⁷ index order. A [`DimVector`]
/// is a length-7 array of exact-rational exponents over these generators.
pub const BASE_DIMENSION_IRIS: [&str; BASE_DIMENSION_COUNT] = [
    "https://blackcatinformatics.ca/math/lengthDimension",
    "https://blackcatinformatics.ca/math/massDimension",
    "https://blackcatinformatics.ca/math/timeDimension",
    "https://blackcatinformatics.ca/math/electricCurrentDimension",
    "https://blackcatinformatics.ca/math/temperatureDimension",
    "https://blackcatinformatics.ca/math/amountOfSubstanceDimension",
    "https://blackcatinformatics.ca/math/luminousIntensityDimension",
];

/// Canonical base-dimension symbols, in the same order as [`BASE_DIMENSION_IRIS`]
/// (`L M T I Θ N J`). Used to render a dimension vector to its human-readable
/// `math:dimensionVector` string.
pub const BASE_SYMBOLS: [&str; BASE_DIMENSION_COUNT] = ["L", "M", "T", "I", "\u{0398}", "N", "J"];

fn math(local: &str) -> String {
    format!("{MATH}{local}")
}

/// Position of a base-dimension IRI in the canonical ℚ⁷ order, if it is one of the
/// seven SI generators.
pub fn base_dimension_index(iri: &str) -> Option<usize> {
    BASE_DIMENSION_IRIS.iter().position(|b| *b == iri)
}

/// An exact-rational ℚ⁷ SI-dimension exponent vector: the coordinates of a physical
/// dimension in the free ℚ-vector space over the seven SI base dimensions.
///
/// A product of dimensions adds their exponent vectors ([`DimVector::add`]); a
/// ratio subtracts ([`DimVector::sub`]); two dimensions are commensurable exactly
/// when their vectors are equal ([`DimVector::commensurable`]). Every coordinate is
/// an exact [`Rational`], so equality is precise for fractional dimensions and the
/// derived `Eq`/`Hash` make [`DimVector`] a sound map/set key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DimVector([Rational; BASE_DIMENSION_COUNT]);

impl DimVector {
    /// The dimensionless zero vector (all exponents zero).
    pub fn zero() -> Self {
        Self([Rational::zero(); BASE_DIMENSION_COUNT])
    }

    /// The unit basis vector `e_index` (exponent 1 on one base dimension, 0
    /// elsewhere). An out-of-range index is a hard fail.
    pub fn unit(index: usize) -> Result<Self> {
        let mut v = Self::zero();
        *v.component_mut(index)? = Rational::one();
        Ok(v)
    }

    /// The unit basis vector for a base-dimension IRI, or `None` if `iri` is not one
    /// of the seven SI generators.
    pub fn base_unit(iri: &str) -> Option<Self> {
        let index = base_dimension_index(iri)?;
        // The index came from `base_dimension_index`, so it is in `0..7`.
        Self::unit(index).ok()
    }

    /// The exact-rational exponent on base dimension `index`. An out-of-range index
    /// is a hard fail.
    pub fn component(&self, index: usize) -> Result<Rational> {
        self.0
            .get(index)
            .copied()
            .ok_or_else(|| out_of_range(index))
    }

    fn component_mut(&mut self, index: usize) -> Result<&mut Rational> {
        self.0.get_mut(index).ok_or_else(|| out_of_range(index))
    }

    /// Add `exponent` into base dimension `index` (checked exact-rational sum). An
    /// out-of-range index or an arithmetic overflow is a hard fail. This is the
    /// accumulation primitive a derived dimension's Σ `power · e_base` uses.
    pub fn add_exponent(&mut self, index: usize, exponent: Rational) -> Result<()> {
        let slot = self.component_mut(index)?;
        *slot = slot.checked_add(exponent)?;
        Ok(())
    }

    /// Componentwise exact-rational sum — the group operation of the dimension
    /// vector space (a product of dimensions adds exponent vectors). Overflow is a
    /// hard fail.
    pub fn add(&self, other: &Self) -> Result<Self> {
        let mut out = Self::zero();
        for i in 0..BASE_DIMENSION_COUNT {
            out.0[i] = self.0[i].checked_add(other.0[i])?;
        }
        Ok(out)
    }

    /// Componentwise exact-rational difference (a ratio of dimensions subtracts
    /// exponent vectors). Overflow is a hard fail.
    pub fn sub(&self, other: &Self) -> Result<Self> {
        let mut out = Self::zero();
        for i in 0..BASE_DIMENSION_COUNT {
            out.0[i] = self.0[i].checked_sub(other.0[i])?;
        }
        Ok(out)
    }

    /// `true` iff `self` and `other` are the same dimension (equal exponent
    /// vectors) — the derived `math:commensurableWith` decision. Pure: returns a
    /// bool so each consumer formats its own diagnostic.
    pub fn commensurable(&self, other: &Self) -> bool {
        self == other
    }

    /// `true` iff `self` is dimensionless (the zero vector).
    pub fn is_dimensionless(&self) -> bool {
        *self == Self::zero()
    }

    /// Render to the canonical `math:dimensionVector` string, e.g. `"L·T-1"` for
    /// velocity or `"1"` for a dimensionless quantity. Exponent 1 is elided; a
    /// non-unit denominator prints as `num/den`. This is the single source an
    /// authored `math:dimensionVector` string must match — the string is a computed
    /// projection, not an independent hand-authored fact.
    pub fn render(&self) -> String {
        let mut parts: Vec<String> = Vec::new();
        for (i, r) in self.0.iter().enumerate() {
            let (num, den) = (r.numerator(), r.denominator());
            if num == 0 {
                continue;
            }
            let mut s = BASE_SYMBOLS[i].to_string();
            if !(num == 1 && den == 1) {
                if den == 1 {
                    s.push_str(&num.to_string());
                } else {
                    s.push_str(&format!("{num}/{den}"));
                }
            }
            parts.push(s);
        }
        if parts.is_empty() {
            "1".to_string()
        } else {
            parts.join("\u{00b7}")
        }
    }
}

fn out_of_range(index: usize) -> Diag {
    Diag::of_kind(MalformedDimension {
        detail: format!(
            "base-dimension index {index} out of range (only {BASE_DIMENSION_COUNT} SI generators)"
        ),
    })
}

/// The distinct dimension vectors of `dimensions`, in first-seen order. Pure: the
/// homogeneity decision over a set of operand dimensions returns the data (the
/// distinct set); each consumer formats its own diagnostic from it.
pub fn distinct(dimensions: &[DimVector]) -> Vec<DimVector> {
    let mut out: Vec<DimVector> = Vec::new();
    for d in dimensions {
        if !out.iter().any(|seen| seen.commensurable(d)) {
            out.push(*d);
        }
    }
    out
}

/// `true` iff every dimension in `dimensions` is mutually commensurable (at most one
/// distinct exponent vector). An empty set is vacuously homogeneous.
pub fn homogeneous(dimensions: &[DimVector]) -> bool {
    distinct(dimensions).len() <= 1
}

/// Read the exact-rational ℚ⁷ exponent vector of a dimension IRI out of a
/// [`TripleIndex`]. A base dimension is a unit basis vector; `math:dimensionless`
/// (or any `math:Dimensionless`) is the zero vector; a `math:DerivedDimension` sums
/// `power · e_base` over its `math:baseDimensionExponent` cells (each a
/// `math:DimensionExponent` with a `math:exponentOfDimension` base target and
/// `math:exponentNumerator`/`math:exponentDenominator` power).
///
/// Every structural fault is a HARD fail ([`MalformedDimension`]): a cell naming a
/// non-base target, a missing numerator/denominator, a zero-denominator (undefined)
/// power, an arithmetic overflow, or a node that is not a recognizable dimension.
pub fn load_dimension_vector(index: &TripleIndex, dim_iri: &str) -> Result<DimVector> {
    if let Some(v) = DimVector::base_unit(dim_iri) {
        return Ok(v);
    }
    if dim_iri == math("dimensionless") || has_type(index, dim_iri, &math("Dimensionless")) {
        return Ok(DimVector::zero());
    }
    if !has_type(index, dim_iri, &math("DerivedDimension")) {
        return Err(Diag::of_kind(MalformedDimension {
            detail: format!(
                "dimension {dim_iri} is not a base dimension, math:Dimensionless, or \
                 math:DerivedDimension — its ℚ⁷ exponent vector cannot be derived"
            ),
        }));
    }
    let mut v = DimVector::zero();
    for cell in all_iris(index, dim_iri, &math("baseDimensionExponent")) {
        let base = all_iris(index, &cell, &math("exponentOfDimension"))
            .into_iter()
            .next()
            .ok_or_else(|| {
                Diag::of_kind(MalformedDimension {
                    detail: format!(
                        "dimension-exponent cell {cell} of {dim_iri} is missing \
                         math:exponentOfDimension"
                    ),
                })
            })?;
        let base_index = base_dimension_index(&base).ok_or_else(|| {
            Diag::of_kind(MalformedDimension {
                detail: format!(
                    "dimension-exponent cell {cell} names non-base target {base} in \
                     math:exponentOfDimension"
                ),
            })
        })?;
        let num = first_i128(index, &cell, &math("exponentNumerator")).ok_or_else(|| {
            Diag::of_kind(MalformedDimension {
                detail: format!(
                    "dimension-exponent cell {cell} is missing an integer math:exponentNumerator"
                ),
            })
        })?;
        let den = first_i128(index, &cell, &math("exponentDenominator")).ok_or_else(|| {
            Diag::of_kind(MalformedDimension {
                detail: format!(
                    "dimension-exponent cell {cell} is missing an integer math:exponentDenominator"
                ),
            })
        })?;
        // `Rational::new` hard-fails a zero denominator; re-badge it as a malformed
        // dimension so the reader speaks in dimension terms.
        let power = Rational::new(num, den).map_err(|_| {
            Diag::of_kind(MalformedDimension {
                detail: format!(
                    "dimension-exponent cell {cell} declares an undefined power {num}/{den} \
                     (zero or i128::MIN denominator)"
                ),
            })
        })?;
        v.add_exponent(base_index, power)?;
    }
    Ok(v)
}

/// The single dimension IRI a node carries through `math:hasDimension` (lexically
/// least if several — the shape forbids more than one), or `None` if it carries
/// none.
pub fn node_dimension(index: &TripleIndex, node_iri: &str) -> Option<String> {
    let mut iris = all_iris(index, node_iri, &math("hasDimension"));
    iris.sort();
    iris.into_iter().next()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::index_turtle;

    fn r(num: i128, den: i128) -> Rational {
        Rational::new(num, den).expect("rational")
    }

    const PREFIXES: &str = "@prefix math: <https://blackcatinformatics.ca/math/> .\n\
         @prefix ex: <https://example.org/> .\n";

    #[test]
    fn base_indexing_is_canonical_order() {
        assert_eq!(
            base_dimension_index("https://blackcatinformatics.ca/math/lengthDimension"),
            Some(0)
        );
        assert_eq!(
            base_dimension_index("https://blackcatinformatics.ca/math/timeDimension"),
            Some(2)
        );
        assert_eq!(
            base_dimension_index("https://blackcatinformatics.ca/math/luminousIntensityDimension"),
            Some(6)
        );
        assert_eq!(base_dimension_index("https://example.org/nope"), None);
    }

    #[test]
    fn add_sub_render_round_trip() {
        // Length e_0 and time e_2; velocity = L · T^-1 = e_0 - e_2.
        let length = DimVector::unit(0).unwrap();
        let time = DimVector::unit(2).unwrap();
        let velocity = length.sub(&time).unwrap();
        assert_eq!(velocity.render(), "L\u{00b7}T-1");
        // Adding time back recovers pure length.
        assert_eq!(velocity.add(&time).unwrap(), length);
        assert_eq!(length.render(), "L");
        // The zero vector renders as the dimensionless "1".
        assert_eq!(DimVector::zero().render(), "1");
        assert!(DimVector::zero().is_dimensionless());

        // Fractional exponent: T^(-1/2) renders with the num/den form.
        let mut sqrt_hz = DimVector::zero();
        sqrt_hz.add_exponent(2, r(-1, 2)).unwrap();
        assert_eq!(sqrt_hz.render(), "T-1/2");
    }

    #[test]
    fn commensurability_and_distinct() {
        let a = DimVector::unit(2).unwrap(); // T
        let b = DimVector::unit(2).unwrap(); // T
        let mut c = DimVector::zero();
        c.add_exponent(2, r(-1, 1)).unwrap(); // T^-1
        assert!(a.commensurable(&b));
        assert!(!a.commensurable(&c));
        assert_eq!(distinct(&[a, b, c]).len(), 2);
        assert!(homogeneous(&[a, b]));
        assert!(!homogeneous(&[a, c]));
        assert!(homogeneous(&[])); // vacuously homogeneous
    }

    #[test]
    fn add_exponent_out_of_range_is_malformed() {
        let mut v = DimVector::zero();
        let err = v.add_exponent(7, Rational::one()).unwrap_err();
        assert_eq!(
            gmeow_errors::code::code_str(err.code()),
            MalformedDimension::CODE
        );
        assert!(DimVector::unit(9).is_err());
    }

    #[test]
    fn reader_base_dimensionless_and_derived() {
        let turtle = format!(
            "{PREFIXES}\
             ex:freqDim a math:DerivedDimension ; math:baseDimensionExponent ex:tm1 .\n\
             ex:tm1 a math:DimensionExponent ; math:exponentOfDimension math:timeDimension ;\n\
               math:exponentNumerator -1 ; math:exponentDenominator 1 .\n\
             ex:noneDim a math:Dimensionless .\n\
             ex:q a math:Quantity ; math:hasDimension ex:freqDim .\n"
        );
        let index = index_turtle(turtle.as_bytes()).expect("index");

        // Base dimension → unit vector.
        let time =
            load_dimension_vector(&index, "https://blackcatinformatics.ca/math/timeDimension")
                .expect("time");
        assert_eq!(time, DimVector::unit(2).unwrap());

        // Dimensionless → zero.
        assert!(
            load_dimension_vector(&index, "https://example.org/noneDim")
                .unwrap()
                .is_dimensionless()
        );

        // Derived T^-1 → renders "T-1".
        let freq = load_dimension_vector(&index, "https://example.org/freqDim").expect("freq");
        assert_eq!(freq.render(), "T-1");

        // node_dimension resolves math:hasDimension.
        assert_eq!(
            node_dimension(&index, "https://example.org/q").as_deref(),
            Some("https://example.org/freqDim")
        );
        assert_eq!(node_dimension(&index, "https://example.org/freqDim"), None);
    }

    #[test]
    fn reader_malformed_cases_hard_fail() {
        // Zero-denominator power.
        let zero_den = format!(
            "{PREFIXES}\
             ex:badDim a math:DerivedDimension ; math:baseDimensionExponent ex:zc .\n\
             ex:zc a math:DimensionExponent ; math:exponentOfDimension math:timeDimension ;\n\
               math:exponentNumerator -1 ; math:exponentDenominator 0 .\n"
        );
        let index = index_turtle(zero_den.as_bytes()).expect("index");
        let err = load_dimension_vector(&index, "https://example.org/badDim").unwrap_err();
        assert_eq!(
            gmeow_errors::code::code_str(err.code()),
            MalformedDimension::CODE
        );
        assert!(err.message().contains("undefined power"), "{err}");

        // Non-base exponent target.
        let non_base = format!(
            "{PREFIXES}\
             ex:badDim a math:DerivedDimension ; math:baseDimensionExponent ex:nb .\n\
             ex:nb a math:DimensionExponent ; math:exponentOfDimension ex:notABase ;\n\
               math:exponentNumerator 1 ; math:exponentDenominator 1 .\n"
        );
        let index = index_turtle(non_base.as_bytes()).expect("index");
        let err = load_dimension_vector(&index, "https://example.org/badDim").unwrap_err();
        assert_eq!(
            gmeow_errors::code::code_str(err.code()),
            MalformedDimension::CODE
        );
        assert!(err.message().contains("non-base target"), "{err}");

        // A node that is not a dimension at all.
        let not_dim = format!("{PREFIXES}ex:thing a math:Quantity .\n");
        let index = index_turtle(not_dim.as_bytes()).expect("index");
        let err = load_dimension_vector(&index, "https://example.org/thing").unwrap_err();
        assert_eq!(
            gmeow_errors::code::code_str(err.code()),
            MalformedDimension::CODE
        );

        // A missing exponent numerator.
        let missing_num = format!(
            "{PREFIXES}\
             ex:badDim a math:DerivedDimension ; math:baseDimensionExponent ex:mn .\n\
             ex:mn a math:DimensionExponent ; math:exponentOfDimension math:timeDimension ;\n\
               math:exponentDenominator 1 .\n"
        );
        let index = index_turtle(missing_num.as_bytes()).expect("index");
        let err = load_dimension_vector(&index, "https://example.org/badDim").unwrap_err();
        assert_eq!(
            gmeow_errors::code::code_str(err.code()),
            MalformedDimension::CODE
        );
        assert!(err.message().contains("exponentNumerator"), "{err}");
    }
}
