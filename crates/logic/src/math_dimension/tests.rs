// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Reasoned-graph tests for the `math:` measure-and-dimension gate. Each drives
//! [`check_math_dimension_findings`] over a frozen dataset — the same read substrate
//! the reason-verify pass hands it — and asserts the typed `math:` failure class.

use super::*;
use gmeow_errors::Severity;

const PREFIXES: &str = "@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n\
     @prefix math: <https://blackcatinformatics.ca/math/> .\n\
     @prefix ex: <https://example.org/> .\n";

/// Two pure-time quantities (dimension T) and one length quantity (dimension L),
/// used across the homogeneity tests.
const QUANTITIES: &str = "ex:t1 a math:Quantity ; math:hasDimension math:timeDimension .\n\
     ex:t2 a math:Quantity ; math:hasDimension math:timeDimension .\n\
     ex:len a math:Quantity ; math:hasDimension math:lengthDimension .\n";

fn dataset(turtle: &str) -> std::sync::Arc<RdfDataset> {
    purrdf::parse_dataset(turtle.as_bytes(), "text/turtle", None).expect("valid Turtle")
}

fn findings(turtle: &str) -> Vec<Finding> {
    check_math_dimension_findings(dataset(turtle).as_ref())
}

fn count_class(findings: &[Finding], needle: &str) -> usize {
    findings
        .iter()
        .filter(|f| f.severity == Severity::Error && f.message.contains(needle))
        .count()
}

fn has_class(findings: &[Finding], needle: &str) -> bool {
    count_class(findings, needle) >= 1
}

// ── Homogeneity across every construct kind ─────────────────────────────────

#[test]
fn inhomogeneous_addition_is_flagged() {
    // A summand-of-an-addition construct mixing T and L.
    let f = findings(&format!(
        "{PREFIXES}{QUANTITIES}\
         ex:addition a math:DimensionalExpression ; math:homogeneousOperand ex:t1 , ex:len .\n"
    ));
    assert!(
        has_class(&f, "math:DimensionalInhomogeneity"),
        "an inhomogeneous addition must be flagged: {f:?}"
    );
}

#[test]
fn inhomogeneous_equation_is_flagged() {
    // A side-of-an-equation construct mixing T and L.
    let f = findings(&format!(
        "{PREFIXES}{QUANTITIES}\
         ex:equation a math:DimensionalExpression ; math:homogeneousOperand ex:t1 , ex:len .\n"
    ));
    assert!(
        has_class(&f, "math:DimensionalInhomogeneity"),
        "an inhomogeneous equation must be flagged: {f:?}"
    );
}

#[test]
fn inhomogeneous_comparison_is_flagged() {
    // A term-of-a-comparison construct mixing T and L.
    let f = findings(&format!(
        "{PREFIXES}{QUANTITIES}\
         ex:comparison a math:DimensionalExpression ; math:homogeneousOperand ex:t1 , ex:len .\n"
    ));
    assert!(
        has_class(&f, "math:DimensionalInhomogeneity"),
        "an inhomogeneous comparison must be flagged: {f:?}"
    );
}

#[test]
fn every_construct_kind_is_covered_in_one_bundle() {
    // All three construct kinds present at once → three distinct inhomogeneity findings,
    // proving the gate fires across every construct kind and not just once per bundle.
    let f = findings(&format!(
        "{PREFIXES}{QUANTITIES}\
         ex:addition a math:DimensionalExpression ; math:homogeneousOperand ex:t1 , ex:len .\n\
         ex:equation a math:DimensionalExpression ; math:homogeneousOperand ex:t1 , ex:len .\n\
         ex:comparison a math:DimensionalExpression ; math:homogeneousOperand ex:t1 , ex:len .\n"
    ));
    assert_eq!(
        count_class(&f, "math:DimensionalInhomogeneity"),
        3,
        "each of the three inhomogeneous constructs must yield its own finding: {f:?}"
    );
}

#[test]
fn homogeneous_expression_passes() {
    let f = findings(&format!(
        "{PREFIXES}{QUANTITIES}\
         ex:ok a math:DimensionalExpression ; math:homogeneousOperand ex:t1 , ex:t2 .\n"
    ));
    assert!(
        f.iter().all(|x| x.severity != Severity::Error),
        "a homogeneous expression must pass cleanly: {f:?}"
    );
}

#[test]
fn undimensioned_operand_is_flagged() {
    let f = findings(&format!(
        "{PREFIXES}{QUANTITIES}\
         ex:mystery a math:Quantity .\n\
         ex:bad a math:DimensionalExpression ; math:homogeneousOperand ex:t1 , ex:mystery .\n"
    ));
    assert!(
        has_class(&f, "math:DimensionalInhomogeneity")
            && f.iter()
                .any(|x| x.message.contains("undimensioned operand")),
        "an undimensioned operand must raise math:DimensionalInhomogeneity: {f:?}"
    );
}

// ── Integral dimensional composition ────────────────────────────────────────

/// Energy = ∫ (energy-density) d(volume): M·L⁻¹·T⁻² times L³ = M·L²·T⁻².
const ENERGY_INTEGRAL: &str = "\
     ex:energyDim a math:DerivedDimension ;\n\
       math:baseDimensionExponent ex:mE1 , ex:lE2 , ex:tEm2 .\n\
     ex:mE1 a math:DimensionExponent ; math:exponentOfDimension math:massDimension ;\n\
       math:exponentNumerator 1 ; math:exponentDenominator 1 .\n\
     ex:lE2 a math:DimensionExponent ; math:exponentOfDimension math:lengthDimension ;\n\
       math:exponentNumerator 2 ; math:exponentDenominator 1 .\n\
     ex:tEm2 a math:DimensionExponent ; math:exponentOfDimension math:timeDimension ;\n\
       math:exponentNumerator -2 ; math:exponentDenominator 1 .\n\
     ex:densityDim a math:DerivedDimension ;\n\
       math:baseDimensionExponent ex:mD1 , ex:lDm1 , ex:tDm2 .\n\
     ex:mD1 a math:DimensionExponent ; math:exponentOfDimension math:massDimension ;\n\
       math:exponentNumerator 1 ; math:exponentDenominator 1 .\n\
     ex:lDm1 a math:DimensionExponent ; math:exponentOfDimension math:lengthDimension ;\n\
       math:exponentNumerator -1 ; math:exponentDenominator 1 .\n\
     ex:tDm2 a math:DimensionExponent ; math:exponentOfDimension math:timeDimension ;\n\
       math:exponentNumerator -2 ; math:exponentDenominator 1 .\n\
     ex:volumeDim a math:DerivedDimension ; math:baseDimensionExponent ex:lV3 .\n\
     ex:lV3 a math:DimensionExponent ; math:exponentOfDimension math:lengthDimension ;\n\
       math:exponentNumerator 3 ; math:exponentDenominator 1 .\n\
     ex:density a math:MeasurableFunction ; math:hasDimension ex:densityDim .\n\
     ex:vol a math:Measure ; math:hasDimension ex:volumeDim .\n";

#[test]
fn integral_with_composed_dimensions_passes() {
    let f = findings(&format!(
        "{PREFIXES}{ENERGY_INTEGRAL}\
         ex:energy a math:Integral ; math:integrand ex:density ;\n\
           math:withRespectTo ex:vol ; math:hasDimension ex:energyDim .\n"
    ));
    assert!(
        !has_class(&f, "math:DimensionalInhomogeneity"),
        "a correctly-composed integral must pass: {f:?}"
    );
}

#[test]
fn integral_with_mismatched_result_dimension_is_flagged() {
    // Declare the result as time (T) instead of energy — the parameters do not compose.
    let f = findings(&format!(
        "{PREFIXES}{ENERGY_INTEGRAL}\
         ex:energy a math:Integral ; math:integrand ex:density ;\n\
           math:withRespectTo ex:vol ; math:hasDimension math:timeDimension .\n"
    ));
    assert!(
        has_class(&f, "math:DimensionalInhomogeneity"),
        "a mismatched integral composition must be flagged: {f:?}"
    );
}

// ── Malformed dimension ─────────────────────────────────────────────────────

#[test]
fn zero_denominator_exponent_is_malformed() {
    let f = findings(&format!(
        "{PREFIXES}\
         ex:badDim a math:DerivedDimension ; math:baseDimensionExponent ex:zc .\n\
         ex:zc a math:DimensionExponent ; math:exponentOfDimension math:timeDimension ;\n\
           math:exponentNumerator -1 ; math:exponentDenominator 0 .\n"
    ));
    assert!(
        f.iter()
            .any(|x| x.message.contains("math:MalformedDimension")
                && x.message.contains("exponentDenominator 0")),
        "a zero-denominator power must raise math:MalformedDimension: {f:?}"
    );
}

#[test]
fn dimension_vector_string_drift_is_malformed() {
    // Structured exponents render to "T-1"; the authored string says "L" — drift.
    let f = findings(&format!(
        "{PREFIXES}\
         ex:freqDim a math:DerivedDimension ; math:dimensionVector \"L\" ;\n\
           math:baseDimensionExponent ex:tm1 .\n\
         ex:tm1 a math:DimensionExponent ; math:exponentOfDimension math:timeDimension ;\n\
           math:exponentNumerator -1 ; math:exponentDenominator 1 .\n"
    ));
    assert!(
        f.iter()
            .any(|x| x.message.contains("math:MalformedDimension")
                && x.message.contains("dimensionVector")),
        "a drifted math:dimensionVector string must raise math:MalformedDimension: {f:?}"
    );
}

#[test]
fn dimension_vector_string_matching_render_passes() {
    let f = findings(&format!(
        "{PREFIXES}\
         ex:freqDim a math:DerivedDimension ; math:dimensionVector \"T-1\" ;\n\
           math:baseDimensionExponent ex:tm1 .\n\
         ex:tm1 a math:DimensionExponent ; math:exponentOfDimension math:timeDimension ;\n\
           math:exponentNumerator -1 ; math:exponentDenominator 1 .\n"
    ));
    assert!(
        !has_class(&f, "math:MalformedDimension"),
        "a matching math:dimensionVector string must pass: {f:?}"
    );
}

// ── Gram positive-definiteness ──────────────────────────────────────────────

/// A 2×2 Gram matrix with the given off-diagonal (0,1)/(1,0) value num/den and unit
/// diagonal, authored `math:positiveDefinite`.
fn gram_2x2(off_num: i128, off_den: i128) -> String {
    format!(
        "{PREFIXES}\
         ex:g a math:GramMatrix ; math:definiteness math:positiveDefinite ;\n\
           math:hasEntry ex:e00 , ex:e11 , ex:e01 , ex:e10 .\n\
         ex:e00 a math:MatrixEntry ; math:atRow 0 ; math:atColumn 0 ; math:entryValue ex:one .\n\
         ex:e11 a math:MatrixEntry ; math:atRow 1 ; math:atColumn 1 ; math:entryValue ex:one .\n\
         ex:e01 a math:MatrixEntry ; math:atRow 0 ; math:atColumn 1 ; math:entryValue ex:off .\n\
         ex:e10 a math:MatrixEntry ; math:atRow 1 ; math:atColumn 0 ; math:entryValue ex:off .\n\
         ex:one a math:RationalValue ; math:numerator 1 ; math:denominator 1 .\n\
         ex:off a math:RationalValue ; math:numerator {off_num} ; math:denominator {off_den} .\n"
    )
}

#[test]
fn non_positive_definite_gram_is_flagged() {
    // Off-diagonal 2 → G = [[1,2],[2,1]], det = -3 < 0 → not positive-definite.
    let f = findings(&gram_2x2(2, 1));
    assert!(
        has_class(&f, "math:NonPositiveDefiniteNorm"),
        "an authored-PD Gram that LDLᵀ refutes must raise math:NonPositiveDefiniteNorm: {f:?}"
    );
}

#[test]
fn positive_definite_gram_passes() {
    // Off-diagonal 1/4 → G = [[1,1/4],[1/4,1]], leading minors 1 and 15/16 > 0 → PD.
    let f = findings(&gram_2x2(1, 4));
    assert!(
        !has_class(&f, "math:NonPositiveDefiniteNorm"),
        "a genuinely positive-definite Gram must pass: {f:?}"
    );
}

/// A 2×2 Gram matrix authoring BOTH transpose off-diagonals explicitly, with
/// conflicting values `off01 = a01_num/a01_den` at (0,1) and `off10 = a10_num/a10_den`
/// at (1,0) — a genuinely non-symmetric authored matrix. Diagonal is unit and it is
/// authored `math:positiveDefinite`.
fn gram_2x2_asymmetric(a01_num: i128, a01_den: i128, a10_num: i128, a10_den: i128) -> String {
    format!(
        "{PREFIXES}\
         ex:g a math:GramMatrix ; math:definiteness math:positiveDefinite ;\n\
           math:hasEntry ex:e00 , ex:e11 , ex:e01 , ex:e10 .\n\
         ex:e00 a math:MatrixEntry ; math:atRow 0 ; math:atColumn 0 ; math:entryValue ex:one .\n\
         ex:e11 a math:MatrixEntry ; math:atRow 1 ; math:atColumn 1 ; math:entryValue ex:one .\n\
         ex:e01 a math:MatrixEntry ; math:atRow 0 ; math:atColumn 1 ; math:entryValue ex:off01 .\n\
         ex:e10 a math:MatrixEntry ; math:atRow 1 ; math:atColumn 0 ; math:entryValue ex:off10 .\n\
         ex:one a math:RationalValue ; math:numerator 1 ; math:denominator 1 .\n\
         ex:off01 a math:RationalValue ; math:numerator {a01_num} ; math:denominator {a01_den} .\n\
         ex:off10 a math:RationalValue ; math:numerator {a10_num} ; math:denominator {a10_den} .\n"
    )
}

#[test]
fn asymmetric_gram_is_flagged() {
    // (0,1) = 1/4 but its transpose mate (1,0) = 3/4 — a non-symmetric authored
    // Gram must raise math:AsymmetricGramMatrix.
    let f = findings(&gram_2x2_asymmetric(1, 4, 3, 4));
    assert!(
        has_class(&f, "math:AsymmetricGramMatrix"),
        "an authored Gram whose transpose mates differ must raise \
         math:AsymmetricGramMatrix: {f:?}"
    );
    // The symmetry gate runs BEFORE LDLᵀ, so an asymmetric matrix is not also
    // reported as non-positive-definite from a factor that assumes symmetry.
    assert!(
        !has_class(&f, "math:NonPositiveDefiniteNorm"),
        "an asymmetric Gram is skipped before the LDLᵀ certificate: {f:?}"
    );
}

/// A Gram matrix whose maximum authored index is astronomically large. `dim` is
/// derived from the max `math:atRow`/`math:atColumn`, so this would drive a
/// `dim`×`dim` dense allocation — an OOM/abort — were the index not bounded to
/// `[0, MAX_BASIS_DIM)` by `load_gram` before any matrix is sized.
fn gram_oversized_index(huge: i128) -> String {
    format!(
        "{PREFIXES}\
         ex:g a math:GramMatrix ; math:definiteness math:positiveDefinite ;\n\
           math:hasEntry ex:e00 , ex:eBig .\n\
         ex:e00  a math:MatrixEntry ; math:atRow 0 ; math:atColumn 0 ; math:entryValue ex:one .\n\
         ex:eBig a math:MatrixEntry ; math:atRow {huge} ; math:atColumn {huge} ; math:entryValue ex:one .\n\
         ex:one a math:RationalValue ; math:numerator 1 ; math:denominator 1 .\n"
    )
}

#[test]
fn oversized_gram_index_is_bounded_not_allocated() {
    // A diagonal entry at index 1_000_000_000 makes the symmetry check pass but would
    // demand a ~1e9 × 1e9 dense matrix. The order bound must convert this into a typed
    // math:MalformedDimension finding BEFORE allocation. That this test returns at all
    // (no OOM/abort) is the proof the guard runs ahead of the allocation.
    let f = findings(&gram_oversized_index(1_000_000_000));
    assert!(
        has_class(&f, "math:MalformedDimension"),
        "an out-of-range authored Gram index must raise math:MalformedDimension, not \
         allocate: {f:?}"
    );
    assert!(
        !has_class(&f, "math:NonPositiveDefiniteNorm"),
        "a bounded-out oversized Gram must not also reach the LDLᵀ certificate: {f:?}"
    );
}

#[test]
fn upper_triangle_only_gram_is_symmetric() {
    // The ordinary idiom: author only the (0,1) off-diagonal; the absent (1,0) mate is
    // mirrored by the symmetric fill, NOT an asymmetry.
    let upper_only = gram_2x2(1, 4).replace(
        " math:hasEntry ex:e00 , ex:e11 , ex:e01 , ex:e10 .",
        " math:hasEntry ex:e00 , ex:e11 , ex:e01 .",
    );
    let f = findings(&upper_only);
    assert!(
        !has_class(&f, "math:AsymmetricGramMatrix"),
        "authoring only the upper triangle is symmetric, not an asymmetry: {f:?}"
    );
}

#[test]
fn gram_not_claimed_positive_definite_is_out_of_scope() {
    // The same indefinite matrix, but with NO positive-definite claim, is a legitimate
    // indefinite form (e.g. a Lorentzian metric) and must not be flagged.
    let indefinite = gram_2x2(2, 1).replace(" math:definiteness math:positiveDefinite ;", "");
    let f = findings(&indefinite);
    assert!(
        !has_class(&f, "math:NonPositiveDefiniteNorm"),
        "an un-claimed indefinite Gram is out of scope for the PD gate: {f:?}"
    );
}

// ── The GMN dimension round-trip scene + its falsifying counter-example ──────
// The force = ∫ a dm homomorphism law dim(acceleration) ⊕ dim(mass) = dim(force) is
// authored as a shipped math example and made FALSIFIABLE by a counter-example with one
// perturbed exponent. Both are driven through the real gate here, over the fixture bytes
// on disk (include_str! keeps the test in lockstep with the authored scenes).

const DIMENSION_ROUNDTRIP_SCENE: &str =
    include_str!("../../../../slices/grounding/math/examples/gmn-dimension-roundtrip.ttl");
const DIMENSION_INHOMOGENEOUS_COUNTER: &str = include_str!(
    "../../../../slices/grounding/math/tests/counter-examples/force-dimension-inhomogeneous.ttl"
);

#[test]
fn gmn_dimension_roundtrip_scene_is_clean() {
    // The positive scene: acceleration (L¹T⁻²) integrated against mass (M¹) composes to
    // force (M¹L¹T⁻²), and the two force quantities sum homogeneously — the gate raises
    // nothing.
    let f = findings(DIMENSION_ROUNDTRIP_SCENE);
    assert!(
        !has_class(&f, "math:DimensionalInhomogeneity")
            && !has_class(&f, "math:MalformedDimension"),
        "the force = ∫ a dm round-trip scene must pass the ℚ⁷ dimension gate cleanly: {f:?}"
    );
}

#[test]
fn real_math_module_force_dimension_is_clean_under_the_reasoned_gate() {
    // Unlike the example scene, the slice `module.ttl` IS part of the object-level EDB the
    // reason-verify pass reasons over (`carrier::assemble_object_level_edb` = ontology +
    // slice module.ttl, examples excluded). The canonical `math:forceDimension`
    // (M¹L¹T⁻²) therefore rides the SAME reasoned graph
    // `check_math_dimension_findings` scans at `make reason-verify` — so the ℚ⁷ gate
    // exercises real authored content in production, not only the include_str scene.
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../slices/grounding/math/module.ttl");
    let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("read {path:?}: {e}"));
    let turtle = String::from_utf8(bytes).expect("math module.ttl is UTF-8");
    // Non-vacuity: the canonical derived dimension really is in the module we feed the gate.
    assert!(
        turtle.contains("math:forceDimension") && turtle.contains("math:DerivedDimension"),
        "math:forceDimension must be authored as a math:DerivedDimension in the real math module"
    );
    let f = findings(&turtle);
    assert!(
        f.iter().all(|x| x.severity != Severity::Error),
        "the real math module (incl. the canonical math:forceDimension) must pass the ℚ⁷ \
         dimension gate cleanly in the production read substrate: {f:?}"
    );
}

#[test]
fn gmn_dimension_perturbed_exponent_falsifies_the_law() {
    // The counter-example perturbs the force result's time exponent (−2 → −3), so the
    // integral composition dim(accel) ⊕ dim(mass) ≠ dim(result): EXACTLY
    // math:DimensionalInhomogeneity fires, and no other dimension failure class.
    let f = findings(DIMENSION_INHOMOGENEOUS_COUNTER);
    assert_eq!(
        count_class(&f, "math:DimensionalInhomogeneity"),
        1,
        "the perturbed force dimension must raise exactly one math:DimensionalInhomogeneity: {f:?}"
    );
    assert!(
        !has_class(&f, "math:MalformedDimension"),
        "the perturbed exponent is a well-formed cell, so no math:MalformedDimension: {f:?}"
    );
}
