// SPDX-License-Identifier: AGPL-3.0-only

//! Conformance twins migrated from tests/test_reference_frames.py
//!
//! Each test builds an inline Turtle graph containing the triples that the
//! Python test assembled via `g.add(...)`, converts to N-Triples, and validates
//! against the whole shapes corpus.
//!
//! All tests use `validate()` (fixture-only, no merged ontology) because the
//! Python tests used `run_shacl(g)` (not `_graph()`), and each test explicitly
//! includes type declarations for `FrameRealm`, `FrameKind`, `Axis`, `Determinacy`,
//! `MetricKind`, etc. to satisfy `sh:class` constraints.

use crate::conformance_support::*;
use gmeow_test_batch_macros::batch_cases;

/// Turtle prefix block shared by all reference-frame tests.
const PREFIXES: &str = "\
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix ex:    <https://example.org/test/> .
@prefix rdf:   <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .
@prefix xsd:   <http://www.w3.org/2001/XMLSchema#> .
";

#[batch_cases]
#[case::measurement_reference_frame_passes(
    Case::inline(format!(
        "{PREFIXES}\
ex:siFrame a gmeow:ReferenceFrame .
ex:siFrame gmeow:frameRealm gmeow:frameRealmMeasurement .
ex:siFrame gmeow:hasAxis ex:axisScalar .
ex:siFrame gmeow:dimensionCount \"1\"^^xsd:nonNegativeInteger .
ex:siFrame gmeow:frameKind gmeow:frameKindScalar .
ex:siFrame gmeow:requiresHost false .
ex:siFrame gmeow:determinacyModel gmeow:determinacyCrisp .

gmeow:frameRealmMeasurement a gmeow:FrameRealm .
ex:axisScalar a gmeow:Axis .
gmeow:frameKindScalar a gmeow:FrameKind .
gmeow:determinacyCrisp a gmeow:Determinacy .
"
    ))
)]
#[case::currency_reference_frame_passes(
    Case::inline(format!(
        "{PREFIXES}\
ex:usdFrame a gmeow:ReferenceFrame .
ex:usdFrame gmeow:frameRealm gmeow:frameRealmCurrency .
ex:usdFrame gmeow:hasAxis ex:axisScalar .
ex:usdFrame gmeow:dimensionCount \"1\"^^xsd:nonNegativeInteger .
ex:usdFrame gmeow:frameKind gmeow:frameKindScalar .
ex:usdFrame gmeow:requiresHost false .
ex:usdFrame gmeow:determinacyModel gmeow:determinacyCrisp .

gmeow:frameRealmCurrency a gmeow:FrameRealm .
ex:axisScalar a gmeow:Axis .
gmeow:frameKindScalar a gmeow:FrameKind .
gmeow:determinacyCrisp a gmeow:Determinacy .
"
    ))
)]
#[case::temporal_reference_frame_passes(
    Case::inline(format!(
        "{PREFIXES}\
ex:gregorianFrame a gmeow:ReferenceFrame .
ex:gregorianFrame gmeow:frameRealm gmeow:frameRealmTemporal .
ex:gregorianFrame gmeow:hasAxis ex:axisYear .
ex:gregorianFrame gmeow:hasAxis ex:axisMonth .
ex:gregorianFrame gmeow:hasAxis ex:axisDay .
ex:gregorianFrame gmeow:dimensionCount \"3\"^^xsd:nonNegativeInteger .
ex:gregorianFrame gmeow:frameKind gmeow:frameKindTemporal .
ex:gregorianFrame gmeow:requiresHost false .
ex:gregorianFrame gmeow:determinacyModel gmeow:determinacyCrisp .

gmeow:frameRealmTemporal a gmeow:FrameRealm .
ex:axisYear a gmeow:Axis .
ex:axisMonth a gmeow:Axis .
ex:axisDay a gmeow:Axis .
gmeow:frameKindTemporal a gmeow:FrameKind .
gmeow:determinacyCrisp a gmeow:Determinacy .
"
    ))
)]
#[case::colourspace_reference_frame_passes(
    Case::inline(format!(
        "{PREFIXES}\
ex:srgbFrame a gmeow:ReferenceFrame .
ex:srgbFrame gmeow:frameRealm gmeow:frameRealmColourspace .
ex:srgbFrame gmeow:hasAxis ex:axisRed .
ex:srgbFrame gmeow:hasAxis ex:axisGreen .
ex:srgbFrame gmeow:hasAxis ex:axisBlue .
ex:srgbFrame gmeow:dimensionCount \"3\"^^xsd:nonNegativeInteger .
ex:srgbFrame gmeow:frameKind gmeow:frameKindCartesian .
ex:srgbFrame gmeow:requiresHost false .
ex:srgbFrame gmeow:determinacyModel gmeow:determinacyCrisp .

gmeow:frameRealmColourspace a gmeow:FrameRealm .
ex:axisRed a gmeow:Axis .
ex:axisGreen a gmeow:Axis .
ex:axisBlue a gmeow:Axis .
gmeow:frameKindCartesian a gmeow:FrameKind .
gmeow:determinacyCrisp a gmeow:Determinacy .
"
    ))
)]
#[case::linguistic_reference_frame_passes(
    Case::inline(format!(
        "{PREFIXES}\
ex:englishFrame a gmeow:ReferenceFrame .
ex:englishFrame gmeow:frameRealm gmeow:frameRealmLinguistic .
ex:englishFrame gmeow:hasAxis ex:axisScalar .
ex:englishFrame gmeow:dimensionCount \"1\"^^xsd:nonNegativeInteger .
ex:englishFrame gmeow:frameKind gmeow:frameKindScalar .
ex:englishFrame gmeow:requiresHost false .
ex:englishFrame gmeow:determinacyModel gmeow:determinacyCrisp .

gmeow:frameRealmLinguistic a gmeow:FrameRealm .
ex:axisScalar a gmeow:Axis .
gmeow:frameKindScalar a gmeow:FrameKind .
gmeow:determinacyCrisp a gmeow:Determinacy .
"
    ))
)]
#[case::mathematical_reference_frames_pass_shacl(
    Case::inline(format!(
        "{PREFIXES}\
gmeow:frameRealmMathematical a gmeow:FrameRealm .
gmeow:determinacyCrisp a gmeow:Determinacy .

# Phase space 3-DOF
ex:phaseSpace3DOF a gmeow:ReferenceFrame .
ex:phaseSpace3DOF gmeow:frameRealm gmeow:frameRealmMathematical .
ex:phaseSpace3DOF gmeow:hasAxis ex:axisGeneralizedCoordinate .
ex:phaseSpace3DOF gmeow:hasAxis ex:axisGeneralizedMomentum .
ex:phaseSpace3DOF gmeow:dimensionCount \"2\"^^xsd:nonNegativeInteger .
ex:phaseSpace3DOF gmeow:frameKind gmeow:frameKindPhaseSpace .
ex:phaseSpace3DOF gmeow:requiresHost false .
ex:phaseSpace3DOF gmeow:determinacyModel gmeow:determinacyCrisp .
ex:phaseSpace3DOF gmeow:hasMetricKind gmeow:metricSymplectic .

# Hilbert space
ex:hilbertSpace a gmeow:ReferenceFrame .
ex:hilbertSpace gmeow:frameRealm gmeow:frameRealmMathematical .
ex:hilbertSpace gmeow:hasAxis ex:axisHilbertState .
ex:hilbertSpace gmeow:dimensionCount \"1\"^^xsd:nonNegativeInteger .
ex:hilbertSpace gmeow:frameKind gmeow:frameKindHilbert .
ex:hilbertSpace gmeow:requiresHost false .
ex:hilbertSpace gmeow:determinacyModel gmeow:determinacyCrisp .
ex:hilbertSpace gmeow:hasMetricKind gmeow:metricEuclidean .

# Latent vector space
ex:latentVectorSpace a gmeow:ReferenceFrame .
ex:latentVectorSpace gmeow:frameRealm gmeow:frameRealmMathematical .
ex:latentVectorSpace gmeow:hasAxis ex:axisLatentVector .
ex:latentVectorSpace gmeow:dimensionCount \"1\"^^xsd:nonNegativeInteger .
ex:latentVectorSpace gmeow:frameKind gmeow:frameKindLatentSpace .
ex:latentVectorSpace gmeow:requiresHost false .
ex:latentVectorSpace gmeow:determinacyModel gmeow:determinacyCrisp .
ex:latentVectorSpace gmeow:hasMetricKind gmeow:metricCosine .

# Robot arm C-space
ex:robotArm6DOF a gmeow:ReferenceFrame .
ex:robotArm6DOF gmeow:frameRealm gmeow:frameRealmMathematical .
ex:robotArm6DOF gmeow:hasAxis ex:axisJointAngle1 .
ex:robotArm6DOF gmeow:hasAxis ex:axisJointAngle2 .
ex:robotArm6DOF gmeow:hasAxis ex:axisJointAngle3 .
ex:robotArm6DOF gmeow:hasAxis ex:axisJointAngle4 .
ex:robotArm6DOF gmeow:hasAxis ex:axisJointAngle5 .
ex:robotArm6DOF gmeow:hasAxis ex:axisJointAngle6 .
ex:robotArm6DOF gmeow:dimensionCount \"6\"^^xsd:nonNegativeInteger .
ex:robotArm6DOF gmeow:frameKind gmeow:frameKindManifold .
ex:robotArm6DOF gmeow:requiresHost true .
ex:robotArm6DOF gmeow:determinacyModel gmeow:determinacyCrisp .
ex:robotArm6DOF gmeow:hasMetricKind gmeow:metricEuclidean .

# Type declarations for axis individuals
ex:axisGeneralizedCoordinate a gmeow:Axis .
ex:axisGeneralizedMomentum a gmeow:Axis .
ex:axisHilbertState a gmeow:Axis .
ex:axisLatentVector a gmeow:Axis .
ex:axisJointAngle1 a gmeow:Axis .
ex:axisJointAngle2 a gmeow:Axis .
ex:axisJointAngle3 a gmeow:Axis .
ex:axisJointAngle4 a gmeow:Axis .
ex:axisJointAngle5 a gmeow:Axis .
ex:axisJointAngle6 a gmeow:Axis .

# Type declarations for frame kind individuals
gmeow:frameKindPhaseSpace a gmeow:FrameKind .
gmeow:frameKindHilbert a gmeow:FrameKind .
gmeow:frameKindLatentSpace a gmeow:FrameKind .
gmeow:frameKindManifold a gmeow:FrameKind .

# Type declarations for metric kind individuals
gmeow:metricSymplectic a gmeow:MetricKind .
gmeow:metricEuclidean a gmeow:MetricKind .
gmeow:metricCosine a gmeow:MetricKind .
"
    ))
)]
#[case::narrative_reference_frame_passes(
    Case::inline(format!(
        "{PREFIXES}\
ex:hpCanon a gmeow:NarrativeReferenceFrame .
ex:hpCanon gmeow:frameRealm gmeow:frameRealmNarrative .
ex:hpCanon gmeow:hasAxis ex:axisPlot .
ex:hpCanon gmeow:dimensionCount \"1\"^^xsd:nonNegativeInteger .
ex:hpCanon gmeow:frameKind gmeow:frameKindNarrative .
ex:hpCanon gmeow:requiresHost false .
ex:hpCanon gmeow:determinacyModel gmeow:determinacyCrisp .

gmeow:frameRealmNarrative a gmeow:FrameRealm .
ex:axisPlot a gmeow:Axis .
gmeow:frameKindNarrative a gmeow:FrameKind .
gmeow:determinacyCrisp a gmeow:Determinacy .
"
    ))
)]
#[case::biological_reference_frame_passes(
    Case::inline(format!(
        "{PREFIXES}\
gmeow:frameRealmBiological a gmeow:FrameRealm .
gmeow:frameKindLinearSequence a gmeow:FrameKind .
gmeow:axisSequencePosition a gmeow:Axis .
gmeow:determinacyCrisp a gmeow:Determinacy .
gmeow:metricPositionalDistance a gmeow:MetricKind .
gmeow:strandForward a gmeow:StrandOrientation .
gmeow:sequenceFeatureTypeGene a gmeow:SequenceFeatureType .

# GRCh38 reference frame
ex:grch38 a gmeow:ReferenceFrame .
ex:grch38 gmeow:frameRealm gmeow:frameRealmBiological .
ex:grch38 gmeow:hasAxis gmeow:axisSequencePosition .
ex:grch38 gmeow:dimensionCount \"1\"^^xsd:nonNegativeInteger .
ex:grch38 gmeow:frameKind gmeow:frameKindLinearSequence .
ex:grch38 gmeow:requiresHost false .
ex:grch38 gmeow:determinacyModel gmeow:determinacyCrisp .
ex:grch38 gmeow:hasMetricKind gmeow:metricPositionalDistance .

# A gene on chromosome 1
ex:gene1 a gmeow:SequenceFeature .
ex:gene1 gmeow:sequenceFeatureType gmeow:sequenceFeatureTypeGene .

# Sequence coordinates
ex:coords1 a gmeow:SequenceCoordinates .
ex:coords1 gmeow:sequenceStart \"1000000\"^^xsd:positiveInteger .
ex:coords1 gmeow:sequenceEnd \"1100000\"^^xsd:positiveInteger .
ex:coords1 gmeow:sequenceStrand gmeow:strandForward .
ex:coords1 gmeow:inReferenceAssembly ex:grch38 .

ex:gene1 gmeow:hasSequenceCoordinates ex:coords1 .
"
    ))
)]
fn reference_frames(#[case] case: Case) {
    case.run();
}
