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

fn inhomogeneity_findings(report: &gmeow_errors::model::Report) -> Vec<&gmeow_errors::model::Finding> {
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
        findings.iter().map(|f| f.detail.as_deref()).collect::<Vec<_>>()
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
        report.findings.iter().map(|f| f.code.as_str()).collect::<Vec<_>>()
    );
}
