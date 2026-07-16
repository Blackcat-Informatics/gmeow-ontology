// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Exact-rational geometry diagnostic kinds.
//!
//! Every numeric or graph-loading failure in this grounding crate is a HARD fail
//! (no-optionality): a zero/`i128::MIN` rational, a checked-arithmetic overflow, a
//! malformed decimal literal, an out-of-range basis index, a non-square or
//! non-positive-definite Gram matrix, a zero-vector angle/projection, a degenerate
//! normalization scale, an RDF-read failure, or a missing/empty `math:` cell. Each
//! is a [`DiagKind`](gmeow_errors::DiagKind) minted by
//! [`define_diag_kind!`](gmeow_errors::define_diag_kind) under the `math.*` code
//! namespace, so the exact-rational engine reports on the shared substrate rather
//! than a bare string.

use gmeow_errors::{Code, FindingCategory, Grade, Severity, Standpoint, define_diag_kind};

define_diag_kind! {
    /// A rational value is out of domain: a zero denominator, an `i128::MIN`
    /// component (whose `abs` would overflow), or a division by zero.
    pub struct RationalDomain { detail: String }
    code = "math.rational.domain";
    grade = Grade::new(Severity::Error, FindingCategory::DataShapeViolation, Standpoint::Binding);
    message = "{}", detail;
}

define_diag_kind! {
    /// A grade projection lies outside the algebra's closed interval `0..=p+q`.
    pub struct CliffordGradeOutOfRange { detail: String }
    code = "math.clifford.grade-out-of-range";
    grade = Grade::new(Severity::Error, FindingCategory::DataShapeViolation, Standpoint::Binding);
    message = "{}", detail;
}

define_diag_kind! {
    /// A checked exact-rational operation overflowed the `i128`/`u128` backing
    /// integer and hard-failed rather than wrapping.
    pub struct ArithmeticOverflow { detail: String }
    code = "math.rational.overflow";
    grade = Grade::new(Severity::Error, FindingCategory::DataShapeViolation, Standpoint::Binding);
    message = "{}", detail;
}

define_diag_kind! {
    /// A lexical form is not a well-formed `xsd:decimal`/`xsd:integer`: empty, a
    /// non-digit body, or a magnitude outside the `i128` range.
    pub struct DecimalParse { detail: String }
    code = "math.decimal.parse";
    grade = Grade::new(Severity::Error, FindingCategory::DataShapeViolation, Standpoint::Binding);
    message = "{}", detail;
}

define_diag_kind! {
    /// A matrix/axis index parsed from a graph is negative or at/above the maximum
    /// supported basis dimension, so it cannot size an allocation.
    pub struct IndexOutOfRange { detail: String }
    code = "math.index.out-of-range";
    grade = Grade::new(Severity::Error, FindingCategory::DataShapeViolation, Standpoint::Binding);
    message = "{}", detail;
}

define_diag_kind! {
    /// A Gram matrix is not square, so it cannot present an inner product.
    pub struct NonSquareGram {}
    code = "math.gram.non-square";
    grade = Grade::new(Severity::Error, FindingCategory::DataShapeViolation, Standpoint::Binding);
    message = "Gram matrix must be square";
}

define_diag_kind! {
    /// The LDLᵀ factorization found a non-positive pivot: the Gram matrix is not
    /// positive-definite. `detail` names the offending pivot index and value.
    pub struct NotPositiveDefinite { detail: String }
    code = "math.gram.not-positive-definite";
    grade = Grade::new(Severity::Error, FindingCategory::ModelingDisciplineViolation, Standpoint::Binding);
    message = "{}", detail;
}

define_diag_kind! {
    /// An angle/cosine/projection request touches a zero vector, which makes the
    /// operation undefined.
    pub struct ZeroVector { detail: String }
    code = "math.vector.zero";
    grade = Grade::new(Severity::Error, FindingCategory::DataShapeViolation, Standpoint::Binding);
    message = "{}", detail;
}

define_diag_kind! {
    /// A dominant-axis request was made against a zero-dimensional space, which has
    /// no axis to pick.
    pub struct EmptySpace {}
    code = "math.space.zero-dimensional";
    grade = Grade::new(Severity::Error, FindingCategory::DataShapeViolation, Standpoint::Binding);
    message = "cannot pick a dominant axis of a zero-dimensional space";
}

define_diag_kind! {
    /// A square root was requested of a negative quadratic form, which has no real
    /// value.
    pub struct NegativeSqrt {}
    code = "math.sqrt.negative";
    grade = Grade::new(Severity::Error, FindingCategory::DataShapeViolation, Standpoint::Binding);
    message = "cannot take the square root of a negative quadratic form";
}

define_diag_kind! {
    /// A unit-clamp normalization scale is degenerate (`min == max`), so the span
    /// is zero and the mapping is undefined.
    pub struct DegenerateScale {}
    code = "math.scale.degenerate";
    grade = Grade::new(Severity::Error, FindingCategory::DataShapeViolation, Standpoint::Binding);
    message = "scale profile range is degenerate (min == max)";
}

define_diag_kind! {
    /// The angle formatter could not re-parse the cosine string it emitted — an
    /// internal invariant break.
    pub struct BadCosine {}
    code = "math.angle.bad-cosine";
    grade = Grade::new(Severity::Error, FindingCategory::ModelingDisciplineViolation, Standpoint::Binding);
    message = "bad cosine";
}

define_diag_kind! {
    /// The shared `purrdf` RDF read (Turtle parse, snapshot build, GTS emit) failed
    /// while normalizing a `math:` graph. Its message is preserved verbatim.
    pub struct GraphRead { detail: String }
    code = "math.graph.read";
    grade = Grade::new(Severity::Error, FindingCategory::ModelingDisciplineViolation, Standpoint::Binding);
    message = "{}", detail;
}

define_diag_kind! {
    /// A `math:` node is missing a required property (a rational's
    /// numerator/denominator, a matrix entry's row/column/value, or a vector
    /// component's index/value).
    pub struct MissingProperty { detail: String }
    code = "math.graph.missing-property";
    grade = Grade::new(Severity::Error, FindingCategory::DataShapeViolation, Standpoint::Binding);
    message = "{}", detail;
}

define_diag_kind! {
    /// A `math:GramMatrix`/`math:Vector` declares no cells, so there is nothing to
    /// read.
    pub struct NoCells { detail: String }
    code = "math.graph.no-cells";
    grade = Grade::new(Severity::Error, FindingCategory::DataShapeViolation, Standpoint::Binding);
    message = "{}", detail;
}

define_diag_kind! {
    /// A Clifford signature requests more than the supported 64 generators, so
    /// its basis blades cannot be represented by the kernel's exact `u64` masks.
    pub struct InvalidCliffordSignature { detail: String }
    code = "math.clifford.invalid-signature";
    grade = Grade::new(Severity::Error, FindingCategory::DataShapeViolation, Standpoint::Binding);
    message = "{}", detail;
}

define_diag_kind! {
    /// A Clifford basis-blade mask or generator index lies outside the algebra's
    /// declared signature.
    pub struct CliffordBladeOutOfRange { detail: String }
    code = "math.clifford.blade-out-of-range";
    grade = Grade::new(Severity::Error, FindingCategory::DataShapeViolation, Standpoint::Binding);
    message = "{}", detail;
}

/// The complete math diagnostic-code catalog, in registration order.
pub const MATH_DIAG_CODES: &[&str] = &[
    RationalDomain::CODE,
    ArithmeticOverflow::CODE,
    DecimalParse::CODE,
    IndexOutOfRange::CODE,
    NonSquareGram::CODE,
    NotPositiveDefinite::CODE,
    ZeroVector::CODE,
    EmptySpace::CODE,
    NegativeSqrt::CODE,
    DegenerateScale::CODE,
    BadCosine::CODE,
    GraphRead::CODE,
    MissingProperty::CODE,
    NoCells::CODE,
    InvalidCliffordSignature::CODE,
    CliffordBladeOutOfRange::CODE,
    CliffordGradeOutOfRange::CODE,
];

/// Eagerly intern every math diagnostic code (idempotent).
pub fn register_all() -> Vec<Code> {
    vec![
        RationalDomain::register(),
        ArithmeticOverflow::register(),
        DecimalParse::register(),
        IndexOutOfRange::register(),
        NonSquareGram::register(),
        NotPositiveDefinite::register(),
        ZeroVector::register(),
        EmptySpace::register(),
        NegativeSqrt::register(),
        DegenerateScale::register(),
        BadCosine::register(),
        GraphRead::register(),
        MissingProperty::register(),
        NoCells::register(),
        InvalidCliffordSignature::register(),
        CliffordBladeOutOfRange::register(),
        CliffordGradeOutOfRange::register(),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use gmeow_errors::intern_code;
    use std::collections::HashSet;

    #[test]
    fn every_math_code_interns_with_no_collision() {
        let handles = register_all();
        assert_eq!(
            handles.len(),
            MATH_DIAG_CODES.len(),
            "register_all() and MATH_DIAG_CODES must enumerate the same kinds"
        );
        for code in MATH_DIAG_CODES {
            assert!(
                intern_code(code).is_ok(),
                "math code `{code}` did not intern after register_all()"
            );
        }
        let distinct_strings: HashSet<&&str> = MATH_DIAG_CODES.iter().collect();
        assert_eq!(
            distinct_strings.len(),
            MATH_DIAG_CODES.len(),
            "duplicate math diagnostic code string detected"
        );
        let distinct_handles: HashSet<Code> = handles.iter().copied().collect();
        assert_eq!(distinct_handles.len(), handles.len());
    }
}
