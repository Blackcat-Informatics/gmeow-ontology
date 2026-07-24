// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Production-surface acceptance harness for the reasoner-derived `math:`
//! dimensional-homogeneity gate.
//!
//! These tests drive the **production `verify()` entrypoint** — the same one
//! `make reason-verify` invokes — with a real input dataset, never a
//! hand-assembled EDB or a private engine call. They encode the *correct
//! decomposition* of the gate's failure classes, NOT byte-identity with the
//! retired Rust sweep:
//!
//! * differing-ℚ⁷-dimension operands / integral-composition mismatch →
//!   reasoner-materialized `math:DimensionalInhomogeneity`, surfaced as a
//!   `verify.dimensional-inhomogeneity` finding by the verify query loop;
//! * malformed / zero-denominator dimension → only `math:MalformedDimension`
//!   (never a spurious inhomogeneity — cross-class non-contamination);
//! * a clean scene materializes ZERO inhomogeneity markers.
//!
//! The *missing-dimension* case (an undimensioned quantity/operand) is a
//! cardinality obligation whose home is the `validate()` / derived-SHACL
//! surface (`math:UndimensionedQuantity`); its acceptance check lives with the
//! `gmeow-validate` lint pin, not here — `verify()` does not evaluate SHACL
//! `sh:minCount`. The compile-source check (the emitted rule derives from the
//! authored `logic:Formula` laws) lives with the compile-pipeline test that
//! owns the `math` program builder. Together those three surfaces cover the
//! full acceptance contract without any surface being asserted where it cannot
//! execute.

use gmeow_errors::Severity;
use gmeow_logic::verify::{embedded_verify_queries, verify};
use std::sync::Arc;

use purrdf::RdfDataset;

// ── Fixtures (shipped slice artifacts — driven in isolation, exactly as the
//    reason-verify pass would see them) ───────────────────────────────────────

/// The force = ∫ a dm scene with one perturbed exponent: the integral's declared
/// result dimension (M L T⁻³) ≠ integrand ⊕ measure (M L T⁻²). Must raise
/// dimensional inhomogeneity through the integral-composition law.
const INTEGRAL_MISMATCH: &str = include_str!(
    "../../../slices/grounding/math/tests/counter-examples/force-dimension-inhomogeneous.ttl"
);

/// The clean, dimensionally-consistent round-trip scene — zero markers.
const CLEAN_SCENE: &str =
    include_str!("../../../slices/grounding/math/examples/gmn-dimension-roundtrip.ttl");

/// A dimension whose exponent denominator is zero — malformed, NOT inhomogeneous.
const ZERO_DENOMINATOR: &str = include_str!(
    "../../../slices/grounding/math/tests/counter-examples/dimension-zero-denominator.ttl"
);

/// The reasoner-derived inhomogeneity gate's finding code: the verify query loop
/// renders a returned row of `dimensional-inhomogeneity.rq` as `verify.<stem>`.
const INHOMOGENEITY_CODE: &str = "verify.dimensional-inhomogeneity";

/// A differing-dimension expression: two homogeneous operands carrying distinct
/// dimensions (T vs L). Inline so the harness pins the addition/comparison shape
/// directly, not only the integral shape.
const DIFFERING_DIMENSIONS: &str = "\
@prefix math: <https://blackcatinformatics.ca/math/> .
@prefix ex:   <http://example.org/math/> .

ex:mix a math:DimensionalExpression ;
    math:homogeneousOperand ex:t1 , ex:len .
ex:t1  a math:Quantity ; math:hasDimension math:timeDimension .
ex:len a math:Quantity ; math:hasDimension math:lengthDimension .
";

fn parse(ttl: &str) -> Arc<RdfDataset> {
    purrdf::parse_dataset(ttl.as_bytes(), "text/turtle", None).expect("fixture is valid Turtle")
}

/// Drive the production reason-verify entrypoint over `ttl` with the full
/// embedded verify query set.
fn run_verify(ttl: &str) -> gmeow_errors::model::Report {
    let ds = parse(ttl);
    verify(ds.as_ref(), &embedded_verify_queries()).expect("verify() must not error on the fixture")
}

fn inhomogeneity_findings(
    report: &gmeow_errors::model::Report,
) -> Vec<&gmeow_errors::model::Finding> {
    report
        .findings
        .iter()
        .filter(|f| f.severity == Severity::Error && f.code == INHOMOGENEITY_CODE)
        .collect()
}

// ── AC-1 / AC-2: the inhomogeneity gate fires on the production surface ───────

#[test]
fn integral_composition_mismatch_fires_on_verify() {
    // AC-2: dim(result) ≠ dim(integrand) ⊕ dim(measure) → reasoner-materialized
    // marker, surfaced as verify.dimensional-inhomogeneity.
    let report = run_verify(INTEGRAL_MISMATCH);
    assert!(
        !inhomogeneity_findings(&report).is_empty(),
        "integral-composition mismatch must raise a {INHOMOGENEITY_CODE} finding via verify(); \
         got: {:?}",
        report
            .findings
            .iter()
            .map(|f| (f.code.as_str(), f.message.as_str()))
            .collect::<Vec<_>>()
    );
}

#[test]
fn differing_operand_dimensions_fire_on_verify() {
    // AC-1: two homogeneous operands with distinct ℚ⁷ dimensions.
    let report = run_verify(DIFFERING_DIMENSIONS);
    assert!(
        !inhomogeneity_findings(&report).is_empty(),
        "differing operand dimensions must raise a {INHOMOGENEITY_CODE} finding via verify(); \
         got: {:?}",
        report
            .findings
            .iter()
            .map(|f| (f.code.as_str(), f.message.as_str()))
            .collect::<Vec<_>>()
    );
}

// ── AC-7: the derived finding carries the offending witnesses (message parity) ─

#[test]
fn inhomogeneity_finding_names_the_offending_witnesses() {
    let report = run_verify(INTEGRAL_MISMATCH);
    let findings = inhomogeneity_findings(&report);
    assert!(!findings.is_empty(), "expected an inhomogeneity finding");
    // The detail must name the offending subject and/or its dimensions, matching
    // the diagnostic specificity of the retired sweep — never a bare, witness-less
    // marker. `netForce` is the integral whose declared result dimension diverges.
    let has_witness = findings.iter().any(|f| {
        f.detail
            .as_deref()
            .is_some_and(|d| d.contains("netForce") || d.contains("Dim"))
    });
    assert!(
        has_witness,
        "the derived finding must name the offending integral / dimensions; details were: {:?}",
        findings
            .iter()
            .map(|f| f.detail.as_deref())
            .collect::<Vec<_>>()
    );
}

// ── AC-4: a clean scene materializes ZERO inhomogeneity markers ───────────────

#[test]
fn clean_scene_materializes_no_marker() {
    let report = run_verify(CLEAN_SCENE);
    assert!(
        inhomogeneity_findings(&report).is_empty(),
        "the dimensionally-consistent round-trip scene must raise NO {INHOMOGENEITY_CODE}; got: {:?}",
        inhomogeneity_findings(&report)
            .iter()
            .map(|f| f.message.as_str())
            .collect::<Vec<_>>()
    );
}

// ── AC-5: cross-class non-contamination — a malformed dimension is malformed,
//    never a spurious inhomogeneity ─────────────────────────────────────────────

#[test]
fn zero_denominator_is_malformed_not_inhomogeneous() {
    let report = run_verify(ZERO_DENOMINATOR);
    assert!(
        inhomogeneity_findings(&report).is_empty(),
        "a zero-denominator (malformed) dimension must NOT be reported as {INHOMOGENEITY_CODE} \
         (it is math:MalformedDimension); got: {:?}",
        inhomogeneity_findings(&report)
            .iter()
            .map(|f| f.message.as_str())
            .collect::<Vec<_>>()
    );
    // And it MUST still be caught as malformed on the reason-verify surface (the
    // retained native check), so the malformed case is never a silent pass.
    let malformed = report
        .findings
        .iter()
        .any(|f| f.severity == Severity::Error && f.code.contains("malformed-dimension"));
    assert!(
        malformed,
        "a zero-denominator dimension must still raise a malformed-dimension finding; got: {:?}",
        report
            .findings
            .iter()
            .map(|f| f.code.as_str())
            .collect::<Vec<_>>()
    );
}

// ── AC-3: the law is satisfied → no marker (the positive mirror of AC-1/AC-2) ─

/// A homogeneous expression: both operands carry the SAME dimension
/// (`math:timeDimension`). The reasoner-derived `dimEqual` consequent holds, so
/// the constraint-tagged violation rule prunes — no marker.
const HOMOGENEOUS_SAME_DIMENSION: &str = "\
@prefix math: <https://blackcatinformatics.ca/math/> .
@prefix ex:   <http://example.org/math/> .

ex:ok a math:DimensionalExpression ;
    math:homogeneousOperand ex:t1 , ex:t2 .
ex:t1 a math:Quantity ; math:hasDimension math:timeDimension .
ex:t2 a math:Quantity ; math:hasDimension math:timeDimension .
";

#[test]
fn homogeneous_expression_passes_on_verify() {
    // The law is satisfied (dimEqual(timeDimension, timeDimension) holds) → the
    // reasoner materializes NO math:DimensionalInhomogeneity marker.
    let report = run_verify(HOMOGENEOUS_SAME_DIMENSION);
    assert!(
        inhomogeneity_findings(&report).is_empty(),
        "two homogeneous operands sharing one dimension must raise NO {INHOMOGENEITY_CODE}; \
         got: {:?}",
        inhomogeneity_findings(&report)
            .iter()
            .map(|f| f.message.as_str())
            .collect::<Vec<_>>()
    );
}

/// A correctly-composed integral, modeled on the `ENERGY_INTEGRAL` fixture of
/// `crates/logic/src/math_dimension/tests.rs`: density (M¹L⁻¹T⁻²) integrated
/// against volume (L³) composes to energy (M¹L²T⁻²), and the integral declares
/// exactly that result dimension.
const INTEGRAL_COMPOSED_CORRECTLY: &str = "\
@prefix math: <https://blackcatinformatics.ca/math/> .
@prefix ex:   <http://example.org/math/> .

ex:energyDim a math:DerivedDimension ;
    math:baseDimensionExponent ex:mE1 , ex:lE2 , ex:tEm2 .
ex:mE1 a math:DimensionExponent ; math:exponentOfDimension math:massDimension ;
    math:exponentNumerator 1 ; math:exponentDenominator 1 .
ex:lE2 a math:DimensionExponent ; math:exponentOfDimension math:lengthDimension ;
    math:exponentNumerator 2 ; math:exponentDenominator 1 .
ex:tEm2 a math:DimensionExponent ; math:exponentOfDimension math:timeDimension ;
    math:exponentNumerator -2 ; math:exponentDenominator 1 .

ex:densityDim a math:DerivedDimension ;
    math:baseDimensionExponent ex:mD1 , ex:lDm1 , ex:tDm2 .
ex:mD1 a math:DimensionExponent ; math:exponentOfDimension math:massDimension ;
    math:exponentNumerator 1 ; math:exponentDenominator 1 .
ex:lDm1 a math:DimensionExponent ; math:exponentOfDimension math:lengthDimension ;
    math:exponentNumerator -1 ; math:exponentDenominator 1 .
ex:tDm2 a math:DimensionExponent ; math:exponentOfDimension math:timeDimension ;
    math:exponentNumerator -2 ; math:exponentDenominator 1 .

ex:volumeDim a math:DerivedDimension ; math:baseDimensionExponent ex:lV3 .
ex:lV3 a math:DimensionExponent ; math:exponentOfDimension math:lengthDimension ;
    math:exponentNumerator 3 ; math:exponentDenominator 1 .

ex:density a math:MeasurableFunction ; math:hasDimension ex:densityDim .
ex:vol a math:Measure ; math:hasDimension ex:volumeDim .

ex:energy a math:Integral ;
    math:integrand ex:density ;
    math:withRespectTo ex:vol ;
    math:hasDimension ex:energyDim .
";

#[test]
fn integral_composed_correctly_passes_on_verify() {
    // dim(result) == dim(integrand) ⊕ dim(measure) → dimProduct holds → the
    // reasoner materializes NO math:DimensionalInhomogeneity marker.
    let report = run_verify(INTEGRAL_COMPOSED_CORRECTLY);
    assert!(
        inhomogeneity_findings(&report).is_empty(),
        "a correctly-composed integral must raise NO {INHOMOGENEITY_CODE}; got: {:?}",
        inhomogeneity_findings(&report)
            .iter()
            .map(|f| f.message.as_str())
            .collect::<Vec<_>>()
    );
}

/// An integral whose integrand and measure both carry a dimension with an
/// astronomically large exact-rational exponent on the SAME base dimension
/// (`math:timeDimension`, numerator `i128::MAX`) so their ℚ⁷ vector sum
/// (`dF ⊕ dM`) overflows `i128` inside the `math:dimensionProductRel` builtin.
const INTEGRAL_EXPONENT_OVERFLOW: &str = "\
@prefix math: <https://blackcatinformatics.ca/math/> .
@prefix ex:   <http://example.org/math/> .

ex:hugeDim1 a math:DerivedDimension ; math:baseDimensionExponent ex:hugeExp1 .
ex:hugeExp1 a math:DimensionExponent ; math:exponentOfDimension math:timeDimension ;
    math:exponentNumerator 170141183460469231731687303715884105727 ;
    math:exponentDenominator 1 .

ex:hugeDim2 a math:DerivedDimension ; math:baseDimensionExponent ex:hugeExp2 .
ex:hugeExp2 a math:DimensionExponent ; math:exponentOfDimension math:timeDimension ;
    math:exponentNumerator 170141183460469231731687303715884105727 ;
    math:exponentDenominator 1 .

ex:hugeIntegrand a math:MeasurableFunction ; math:hasDimension ex:hugeDim1 .
ex:hugeMeasure a math:Measure ; math:hasDimension ex:hugeDim2 .

ex:overflowIntegral a math:Integral ;
    math:integrand ex:hugeIntegrand ;
    math:withRespectTo ex:hugeMeasure ;
    math:hasDimension math:timeDimension .
";

#[test]
fn dimension_product_overflow_yields_no_spurious_marker() {
    // `math:exponentNumerator` on BOTH the integrand and the measure dimensions is
    // `i128::MAX` on the same base (time); their exact-rational ⊕ composition
    // overflows `i128` inside the `math:dimensionProductRel` builtin
    // (`DimVector::add` → `Rational::checked_add`). Per the dimension-gate
    // contract (`crates/logic/src/physical/builtin_eval.rs`'s `QBuiltin::DimProduct`
    // arm), an exact-rational overflow composing dF ⊕ dM is undefinedness, so the
    // builtin declines to `BuiltinOutcome::Unbound` — undefinedness is NOT a
    // violation for a constraint-tagged rule (it is skipped, never materialized as
    // a violation), matching the retired Rust sweep's own skip-on-overflow
    // behavior (`DimVector::add` returning `Err` was a deliberate `continue`, never
    // a spurious finding). So this must raise ZERO markers, not an error and not a
    // spurious math:DimensionalInhomogeneity.
    let report = run_verify(INTEGRAL_EXPONENT_OVERFLOW);
    assert!(
        inhomogeneity_findings(&report).is_empty(),
        "an integral-composition overflow must decline to Unbound, never fabricate a \
         {INHOMOGENEITY_CODE}; got: {:?}",
        inhomogeneity_findings(&report)
            .iter()
            .map(|f| f.message.as_str())
            .collect::<Vec<_>>()
    );
}
