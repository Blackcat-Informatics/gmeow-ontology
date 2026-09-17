// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

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
    let time = load_dimension_vector(&index, "https://blackcatinformatics.ca/math/timeDimension")
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
