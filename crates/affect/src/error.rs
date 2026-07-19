// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Affect-geometry diagnostic kinds.
//!
//! Reading an affect-intensity observation out of an RDF graph is a HARD failure
//! surface (no-optionality): a missing required property, an unrecognized declared
//! handle, an empty basis, a definiteness cross-check disagreement, or a metric
//! basis mismatch must surface loudly rather than degrade to a meaningless number.
//! Each defect is a [`DiagKind`](gmeow_errors::DiagKind) minted by
//! [`define_diag_kind!`](gmeow_errors::define_diag_kind). The shared `gmeow-math`
//! engine reports its own exact-rational failures as typed diagnostics under the
//! `math.*` code namespace, which propagate through this crate unchanged.

use gmeow_errors::{Code, FindingCategory, Grade, Severity, Standpoint, define_diag_kind};

define_diag_kind! {
    /// A node in the affect graph is missing a required property. `node` is the
    /// subject IRI; `property` is the absent predicate (or cell shape).
    pub struct MissingAffectProperty { node: String, property: String }
    code = "affect.graph.missing-property";
    grade = Grade::new(Severity::Error, FindingCategory::DataShapeViolation, Standpoint::Binding);
    message = "{} is missing required {}", node, property;
}

define_diag_kind! {
    /// A declared affect handle (norm function / weighting policy) is not one the
    /// engine recognizes. `detail` carries the offending value and the expected set.
    pub struct UnrecognizedAffectHandle { detail: String }
    code = "affect.graph.unrecognized-handle";
    grade = Grade::new(Severity::Error, FindingCategory::DataShapeViolation, Standpoint::Binding);
    message = "{}", detail;
}

define_diag_kind! {
    /// The derived affect geometry has an empty basis — no dimension to measure.
    pub struct EmptyAffectBasis {}
    code = "affect.graph.empty-basis";
    grade = Grade::new(Severity::Error, FindingCategory::DataShapeViolation, Standpoint::Binding);
    message = "affect geometry has an empty basis";
}

define_diag_kind! {
    /// No `gmeow:DerivedAffectIntensityObservation` was found in the graph.
    pub struct NoAffectObservations {}
    code = "affect.graph.no-observations";
    grade = Grade::new(Severity::Error, FindingCategory::DataShapeViolation, Standpoint::Binding);
    message = "no gmeow:DerivedAffectIntensityObservation found in graph";
}

define_diag_kind! {
    /// A Gram matrix declares no `math:definiteness` — the authored positive-definite
    /// witness the cross-check certifies against is absent.
    pub struct AuthoredDefinitenessAbsent { gram_iri: String }
    code = "affect.crosscheck.definiteness-absent";
    grade = Grade::new(Severity::Error, FindingCategory::DataShapeViolation, Standpoint::Binding);
    message = "Gram matrix {} declares no math:definiteness (authored PD absent)", gram_iri;
}

define_diag_kind! {
    /// A Gram matrix carries no cells, so it cannot size a matrix.
    pub struct GramHasNoEntries { gram_iri: String }
    code = "affect.crosscheck.gram-no-entries";
    grade = Grade::new(Severity::Error, FindingCategory::DataShapeViolation, Standpoint::Binding);
    message = "Gram matrix {} has no entries", gram_iri;
}

define_diag_kind! {
    /// The authored `math:definiteness` disagrees with the computed LDLᵀ verdict.
    /// `detail` names the Gram, the authored IRI, and what the factorization says.
    pub struct DefinitenessCrosscheckFailed { detail: String }
    code = "affect.crosscheck.definiteness-mismatch";
    grade = Grade::new(Severity::Error, FindingCategory::ModelingDisciplineViolation, Standpoint::Binding);
    message = "{}", detail;
}

define_diag_kind! {
    /// A distance/cosine request spans two observations that do not share the same
    /// metric basis (Gram matrix / axis map), which would otherwise be silently
    /// zero-padded into a meaningless number.
    pub struct MetricBasisMismatch { detail: String }
    code = "affect.distance.metric-basis-mismatch";
    grade = Grade::new(Severity::Error, FindingCategory::ModelingDisciplineViolation, Standpoint::Binding);
    message = "{}", detail;
}

define_diag_kind! {
    /// A nearest-prototype classification was asked to rank over an EMPTY prototype
    /// set — there is no candidate to select, so it hard-fails rather than returning
    /// a meaningless "no nearest".
    pub struct EmptyPrototypeSet {}
    code = "affect.classify.empty-prototype-set";
    grade = Grade::new(Severity::Error, FindingCategory::DataShapeViolation, Standpoint::Binding);
    message = "nearest-prototype classification requires at least one prototype observation";
}

define_diag_kind! {
    /// The explicitly chosen vantage Gram is not positive-definite. The bilinear-form
    /// builtin TRUSTS positive-definiteness (it is certified off-gate), so a non-PD
    /// vantage would yield negative "distances" and a garbage argmin — hard-fail the
    /// classification up front instead. `detail` names the Gram and the LDLᵀ verdict.
    pub struct NonPositiveDefiniteVantage { detail: String }
    code = "affect.classify.vantage-not-pd";
    grade = Grade::new(Severity::Error, FindingCategory::ModelingDisciplineViolation, Standpoint::Binding);
    message = "{}", detail;
}

define_diag_kind! {
    /// A coordinate vector (state or prototype) has more axes than the vantage form's
    /// order, or the two vectors differ in dimension — measuring it under the vantage
    /// metric would silently truncate a coordinate (a wrong answer). `detail` names
    /// the observation and the dimensions.
    pub struct CoordinateDimensionMismatch { detail: String }
    code = "affect.classify.dimension-mismatch";
    grade = Grade::new(Severity::Error, FindingCategory::DataShapeViolation, Standpoint::Binding);
    message = "{}", detail;
}

define_diag_kind! {
    /// Two prototype signatures are COINCIDENT under the vantage metric (zero
    /// G-distance apart) — an authoring error that makes the nearest-prototype margin
    /// bisector undefined. `detail` names the coincident pair.
    pub struct CoincidentPrototypes { detail: String }
    code = "affect.classify.coincident-prototypes";
    grade = Grade::new(Severity::Error, FindingCategory::ModelingDisciplineViolation, Standpoint::Binding);
    message = "{}", detail;
}

define_diag_kind! {
    /// A state or prototype vector has ZERO G-norm, so its cosine (direction) is
    /// undefined — cosine-lens classification hard-fails rather than inventing an
    /// angle for the origin. `detail` names the zero-norm observation.
    pub struct ZeroNormCosine { detail: String }
    code = "affect.classify.zero-norm-cosine";
    grade = Grade::new(Severity::Error, FindingCategory::DataShapeViolation, Standpoint::Binding);
    message = "{}", detail;
}

define_diag_kind! {
    /// An authored coordinate magnitude lies OUTSIDE the vantage profile's declared
    /// bipolar range, so unit-clamp normalization / the metric would read a value the
    /// scale does not define. `detail` names the cell, the value, and the range.
    pub struct ValueOutOfRange { detail: String }
    code = "affect.classify.value-out-of-range";
    grade = Grade::new(Severity::Error, FindingCategory::DataShapeViolation, Standpoint::Binding);
    message = "{}", detail;
}

define_diag_kind! {
    /// The #1428 bilinear-form distance builtin declined (a malformed vantage form or
    /// an exact-arithmetic overflow) — never a wrong answer. `detail` carries the fault.
    pub struct BilinearDistanceFailed { detail: String }
    code = "affect.classify.distance-failed";
    grade = Grade::new(Severity::Error, FindingCategory::ModelingDisciplineViolation, Standpoint::Binding);
    message = "{}", detail;
}

/// The complete affect diagnostic-code catalog, in registration order.
pub const AFFECT_DIAG_CODES: &[&str] = &[
    MissingAffectProperty::CODE,
    UnrecognizedAffectHandle::CODE,
    EmptyAffectBasis::CODE,
    NoAffectObservations::CODE,
    AuthoredDefinitenessAbsent::CODE,
    GramHasNoEntries::CODE,
    DefinitenessCrosscheckFailed::CODE,
    MetricBasisMismatch::CODE,
    EmptyPrototypeSet::CODE,
    NonPositiveDefiniteVantage::CODE,
    CoordinateDimensionMismatch::CODE,
    CoincidentPrototypes::CODE,
    ZeroNormCosine::CODE,
    ValueOutOfRange::CODE,
    BilinearDistanceFailed::CODE,
];

/// Eagerly intern every affect diagnostic code (idempotent).
pub fn register_all() -> Vec<Code> {
    vec![
        MissingAffectProperty::register(),
        UnrecognizedAffectHandle::register(),
        EmptyAffectBasis::register(),
        NoAffectObservations::register(),
        AuthoredDefinitenessAbsent::register(),
        GramHasNoEntries::register(),
        DefinitenessCrosscheckFailed::register(),
        MetricBasisMismatch::register(),
        EmptyPrototypeSet::register(),
        NonPositiveDefiniteVantage::register(),
        CoordinateDimensionMismatch::register(),
        CoincidentPrototypes::register(),
        ZeroNormCosine::register(),
        ValueOutOfRange::register(),
        BilinearDistanceFailed::register(),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use gmeow_errors::intern_code;
    use std::collections::HashSet;

    #[test]
    fn every_affect_code_interns_with_no_collision() {
        let handles = register_all();
        assert_eq!(
            handles.len(),
            AFFECT_DIAG_CODES.len(),
            "register_all() and AFFECT_DIAG_CODES must enumerate the same kinds"
        );
        for code in AFFECT_DIAG_CODES {
            assert!(
                intern_code(code).is_ok(),
                "affect code `{code}` did not intern after register_all()"
            );
        }
        let distinct_strings: HashSet<&&str> = AFFECT_DIAG_CODES.iter().collect();
        assert_eq!(
            distinct_strings.len(),
            AFFECT_DIAG_CODES.len(),
            "duplicate affect diagnostic code string detected"
        );
        let distinct_handles: HashSet<Code> = handles.iter().copied().collect();
        assert_eq!(distinct_handles.len(), handles.len());
    }
}
