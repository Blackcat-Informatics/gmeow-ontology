// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Read-only preservation of the authored dimension gate contracts.

use super::{Contract, report};
use gmeow_errors::Severity;

const INHOMOGENEITY_CODE: &str = "verify.dimensional-inhomogeneity";

fn inhomogeneity_findings(
    report: &gmeow_errors::model::Report,
) -> Vec<&gmeow_errors::model::Finding> {
    report
        .findings
        .iter()
        .filter(|f| f.severity == Severity::Error && f.code == INHOMOGENEITY_CODE)
        .collect()
}

fn integral_composition_mismatch_fires_on_verify() {
    // AC-2: dim(result) ≠ dim(integrand) ⊕ dim(measure) → reasoner-materialized
    // marker, surfaced as verify.dimensional-inhomogeneity.
    let report =
        report("slices/grounding/math/tests/counter-examples/force-dimension-inhomogeneous.ttl");
    assert!(
        !inhomogeneity_findings(report).is_empty(),
        "integral-composition mismatch must raise a {INHOMOGENEITY_CODE} finding via verify(); \
         got: {:?}",
        report
            .findings
            .iter()
            .map(|f| (f.code.as_str(), f.message.as_str()))
            .collect::<Vec<_>>()
    );
}

fn inhomogeneity_finding_names_the_offending_witnesses() {
    let report =
        report("slices/grounding/math/tests/counter-examples/force-dimension-inhomogeneous.ttl");
    let findings = inhomogeneity_findings(report);
    assert!(!findings.is_empty(), "expected an inhomogeneity finding");
    // The detail must name the offending subject, matching the diagnostic specificity
    // of the retired sweep — never a bare, witness-less marker. `netForce` is the
    // integral whose declared result dimension diverges. We assert on the concrete
    // `netForce` token ONLY: a substring like "Dim" would be tautological, since the
    // failure-class local name `DimensionalInhomogeneity` itself contains it, so it
    // would pass whenever any finding exists and prove nothing about message parity.
    let has_witness = findings
        .iter()
        .any(|f| f.detail.as_deref().is_some_and(|d| d.contains("netForce")));
    assert!(
        has_witness,
        "the derived finding must name the offending integral / dimensions; details were: {:?}",
        findings
            .iter()
            .map(|f| f.detail.as_deref())
            .collect::<Vec<_>>()
    );
}

fn clean_scene_materializes_no_marker() {
    let report = report("slices/grounding/math/examples/gmn-dimension-roundtrip.ttl");
    assert!(
        inhomogeneity_findings(report).is_empty(),
        "the dimensionally-consistent round-trip scene must raise NO {INHOMOGENEITY_CODE}; got: {:?}",
        inhomogeneity_findings(report)
            .iter()
            .map(|f| f.message.as_str())
            .collect::<Vec<_>>()
    );
}

fn zero_denominator_is_malformed_not_inhomogeneous() {
    let report =
        report("slices/grounding/math/tests/counter-examples/dimension-zero-denominator.ttl");
    assert!(
        inhomogeneity_findings(report).is_empty(),
        "a zero-denominator (malformed) dimension must NOT be reported as {INHOMOGENEITY_CODE} \
         (it is math:MalformedDimension); got: {:?}",
        inhomogeneity_findings(report)
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

pub(super) fn contracts() -> Vec<Contract> {
    vec![
        (
            "integral_composition_mismatch_fires_on_verify",
            integral_composition_mismatch_fires_on_verify,
        ),
        (
            "inhomogeneity_finding_names_the_offending_witnesses",
            inhomogeneity_finding_names_the_offending_witnesses,
        ),
        (
            "clean_scene_materializes_no_marker",
            clean_scene_materializes_no_marker,
        ),
        (
            "zero_denominator_is_malformed_not_inhomogeneous",
            zero_denominator_is_malformed_not_inhomogeneous,
        ),
    ]
}
