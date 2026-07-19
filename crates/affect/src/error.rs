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
    /// A nearest-prototype classification was asked to argmin over an EMPTY
    /// prototype set — there is no candidate to select, so it hard-fails rather
    /// than returning a meaningless "no nearest".
    pub struct EmptyPrototypeSet {}
    code = "affect.nearest.empty-prototype-set";
    grade = Grade::new(Severity::Error, FindingCategory::DataShapeViolation, Standpoint::Binding);
    message = "nearest-prototype classification requires at least one prototype observation";
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
