// SPDX-License-Identifier: AGPL-3.0-only
//! Whole-ontology SHACL conformance twins for the affect evidence spine and the
//! dimensional-landscape reshape. Each counter-example isolates ONE hard-fail rule
//! from the affect design's "Hard-fail rules" and proves the shape actually FIRES;
//! the conforming cases prove a well-formed record validates clean. Validated
//! fixture-only against the whole shapes corpus (no merged ontology needed — every
//! shape targets by asserted rdf:type or a datatype/nodeKind constraint).

use crate::conformance_support::*;
use gmeow_test_batch_macros::batch_cases;

/// Prefix header shared by every inline fixture.
const P: &str = concat!(
    "@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n",
    "@prefix gmeow-goemotions: <https://blackcatinformatics.ca/gmeow-registry/goemotions/> .\n",
    "@prefix gmeow-labelset: <https://blackcatinformatics.ca/gmeow-registry/labelset/> .\n",
    "@prefix ex: <https://example.org/affect/> .\n",
);

fn ttl(body: &str) -> String {
    format!("{P}{body}")
}

// ── Well-formed records conform ───────────────────────────────────────────────

#[batch_cases]
#[case::wellformed_run_and_output(Case::inline(ttl(
    "ex:run a gmeow:ModelInferenceRun ; gmeow:modelIdentifier \"SamLowe/roberta-base-go_emotions\" ; gmeow:modelRevision \"a1b2c3d\" .\n\
     ex:out a gmeow:AffectClassifierOutput ; gmeow:producedBy ex:run ; gmeow:classifiedTarget ex:chunk ; gmeow:emittedLabel gmeow-goemotions:joy ; gmeow:classifierScore 0.84 ; gmeow:scoreSemantics gmeow:scoreSigmoid .\n"
)))]
#[case::wellformed_registered_label(Case::inline(ttl(
    "gmeow-goemotions:joy a gmeow:AffectClassifierLabel ; gmeow:memberOfLabelSet gmeow-labelset:GoEmotions .\n"
)))]
#[case::wellformed_calibrated_with_calibration(Case::inline(ttl(
    "ex:out a gmeow:AffectClassifierOutput ; gmeow:producedBy ex:run ; gmeow:classifiedTarget ex:chunk ; gmeow:emittedLabel gmeow-goemotions:joy ; gmeow:classifierScore 0.84 ; gmeow:scoreSemantics gmeow:scoreCalibratedProbability ; gmeow:scoreCalibration \"temperature-scaled T=1.4\" .\n"
)))]
// A derived intensity declaring all four machine-readable metric components as
// IRIs (object ranges) and carrying NO stored magnitude conforms.
#[case::wellformed_derived_intensity(Case::inline(ttl(
    "ex:i a gmeow:DerivedAffectIntensityObservation ; gmeow:intensityBasis ex:v ; gmeow:metricProfile gmeow:coreAffectMetricPAD ; gmeow:weightingPolicy gmeow:weightingEqualCoreAffect ; gmeow:normFunction gmeow:affectMetricTensorNorm .\n"
)))]
fn conforms(#[case] case: Case) {
    case.run();
}

// ── Hard-fail rule 1 / 7: run provenance completeness ─────────────────────────

#[batch_cases]
#[case::run_missing_revision(
    Case::inline(ttl("ex:run a gmeow:ModelInferenceRun ; gmeow:modelIdentifier \"m\" .\n"))
        .fails()
        .violations(&["pinned gmeow:modelRevision"])
)]
#[case::run_missing_identifier(
    Case::inline(ttl("ex:run a gmeow:ModelInferenceRun ; gmeow:modelRevision \"r\" .\n"))
        .fails()
        .violations(&["must declare exactly one gmeow:modelIdentifier"])
)]
// ── Hard-fail rule 1: output provenance completeness ──────────────────────────
#[case::output_missing_producedby(
    Case::inline(ttl("ex:out a gmeow:AffectClassifierOutput ; gmeow:classifiedTarget ex:c ; gmeow:emittedLabel gmeow-goemotions:joy ; gmeow:classifierScore 0.8 ; gmeow:scoreSemantics gmeow:scoreSigmoid .\n"))
        .fails()
        .violations(&["gmeow:producedBy"])
)]
#[case::output_missing_target(
    Case::inline(ttl("ex:out a gmeow:AffectClassifierOutput ; gmeow:producedBy ex:run ; gmeow:emittedLabel gmeow-goemotions:joy ; gmeow:classifierScore 0.8 ; gmeow:scoreSemantics gmeow:scoreSigmoid .\n"))
        .fails()
        .violations(&["gmeow:classifiedTarget"])
)]
#[case::output_missing_label(
    Case::inline(ttl("ex:out a gmeow:AffectClassifierOutput ; gmeow:producedBy ex:run ; gmeow:classifiedTarget ex:c ; gmeow:classifierScore 0.8 ; gmeow:scoreSemantics gmeow:scoreSigmoid .\n"))
        .fails()
        .violations(&["gmeow:emittedLabel"])
)]
#[case::output_missing_score(
    Case::inline(ttl("ex:out a gmeow:AffectClassifierOutput ; gmeow:producedBy ex:run ; gmeow:classifiedTarget ex:c ; gmeow:emittedLabel gmeow-goemotions:joy ; gmeow:scoreSemantics gmeow:scoreSigmoid .\n"))
        .fails()
        .violations(&["raw gmeow:classifierScore"])
)]
#[case::output_missing_semantics(
    Case::inline(ttl("ex:out a gmeow:AffectClassifierOutput ; gmeow:producedBy ex:run ; gmeow:classifiedTarget ex:c ; gmeow:emittedLabel gmeow-goemotions:joy ; gmeow:classifierScore 0.8 .\n"))
        .fails()
        .violations(&["must declare exactly one gmeow:scoreSemantics"])
)]
// ── Hard-fail rule 2: label not registered in a label set ─────────────────────
#[case::label_unregistered(
    Case::inline(ttl("ex:l a gmeow:AffectClassifierLabel .\n"))
        .fails()
        .violations(&["registered in a gmeow:AffectLabelSet"])
)]
// ── Hard-fail rule 3: neutral (any label) as an EmotionType ────────────────────
#[case::label_as_emotiontype(
    Case::inline(ttl("gmeow-goemotions:neutral a gmeow:AffectClassifierLabel , gmeow:EmotionType ; gmeow:memberOfLabelSet gmeow-labelset:GoEmotions .\n"))
        .fails()
        .violations(&["must not be modeled as a gmeow:EmotionType"])
)]
// ── Hard-fail rule 4: calibrated-probability score without calibration ─────────
#[case::calibrated_without_calibration(
    Case::inline(ttl("ex:out a gmeow:AffectClassifierOutput ; gmeow:producedBy ex:run ; gmeow:classifiedTarget ex:c ; gmeow:emittedLabel gmeow-goemotions:joy ; gmeow:classifierScore 0.8 ; gmeow:scoreSemantics gmeow:scoreCalibratedProbability .\n"))
        .fails()
        .violations(&["requires a gmeow:scoreCalibration"])
)]
// ── Hard-fail rule 5: output directly asserts inner affect (no claim boundary) ─
#[case::output_asserts_inner_affect(
    Case::inline(ttl("ex:out a gmeow:AffectClassifierOutput ; gmeow:producedBy ex:run ; gmeow:classifiedTarget ex:c ; gmeow:emittedLabel gmeow-goemotions:joy ; gmeow:classifierScore 0.8 ; gmeow:scoreSemantics gmeow:scoreSigmoid ; gmeow:emotionType gmeow:emotionJoy .\n"))
        .fails()
        .violations(&["must not directly assert inner affect"])
)]
// ── Hard-fail rule 8: derived intensity missing a declared metric component ────
#[case::intensity_missing_norm(
    Case::inline(ttl("ex:i a gmeow:DerivedAffectIntensityObservation ; gmeow:intensityBasis ex:v ; gmeow:metricProfile gmeow:coreAffectMetricPAD ; gmeow:weightingPolicy gmeow:weightingEqualCoreAffect .\n"))
        .fails()
        .violations(&["gmeow:normFunction"])
)]
// ── Greenfield reshape: the weighting policy must be a machine-readable IRI ────
#[case::intensity_weighting_string(
    Case::inline(ttl("ex:i a gmeow:DerivedAffectIntensityObservation ; gmeow:intensityBasis ex:v ; gmeow:metricProfile gmeow:coreAffectMetricPAD ; gmeow:weightingPolicy \"equal-weight\" ; gmeow:normFunction gmeow:affectMetricTensorNorm .\n"))
        .fails()
        .violations(&["machine-readable gmeow:WeightingPolicy IRI"])
)]
// ── Never-stored gate: a derived intensity must not carry a stored magnitude ───
#[case::intensity_stores_magnitude(
    Case::inline(ttl("ex:i a gmeow:DerivedAffectIntensityObservation ; gmeow:intensityBasis ex:v ; gmeow:metricProfile gmeow:coreAffectMetricPAD ; gmeow:weightingPolicy gmeow:weightingEqualCoreAffect ; gmeow:normFunction gmeow:affectMetricTensorNorm ; gmeow:appraisalValue 0.8 .\n"))
        .fails()
        .violations(&["must NOT carry a stored magnitude"])
)]
// ── Stage-3 reshape: a dimensional appraisal reading is unframed ───────────────
#[case::appraisal_unframed(
    Case::inline(ttl("ex:a a gmeow:Appraisal ; gmeow:vantage ex:critic ; gmeow:appraisalOf ex:work ; gmeow:appraisalDimension gmeow:dimensionValence ; gmeow:appraisalValue 0.9 .\n"))
        .fails()
        .violations(&["unframed"])
)]
// ── Stage-3 model-up rule: a composite with no declared decomposition ─────────
#[case::composite_no_constituent(
    Case::inline(ttl("ex:x a gmeow:AffectComposite ; gmeow:emotionBearer ex:agent ; gmeow:emotionType gmeow:emotionSchadenfreude .\n"))
        .fails()
        .violations(&["unanalyzed primitive"])
)]
// ── Stage-3 reshape: the named frame is not actually a frame (non-profile referent) ─
#[case::appraisal_scale_profile_not_a_profile(
    Case::inline(ttl("ex:a a gmeow:Appraisal ; gmeow:vantage ex:critic ; gmeow:appraisalOf ex:work ; gmeow:appraisalDimension gmeow:dimensionValence ; gmeow:appraisalValue 0.9 ; gmeow:appraisalScaleProfile ex:notAProfile .\n"))
        .fails()
        .violations(&["non-profile IRI"])
)]
// ── Stage-3 reshape: a scale profile with an inverted (degenerate) range ───────
#[case::profile_range_inverted(
    Case::inline(ttl("ex:p a gmeow:AffectScaleProfile ; gmeow:profileRangeMin 1.0 ; gmeow:profileRangeMax -1.0 .\n"))
        .fails()
        .violations(&["must strictly exceed"])
)]
// ── Stage-4 provenance is single-valued: a run with two pinned revisions reds ──
#[case::run_multiple_revisions(
    Case::inline(ttl("ex:run a gmeow:ModelInferenceRun ; gmeow:modelIdentifier \"m\" ; gmeow:modelRevision \"r1\" ; gmeow:modelRevision \"r2\" .\n"))
        .fails()
        .violations(&["single pinned revision"])
)]
fn hard_fails(#[case] case: Case) {
    case.run();
}
