"""Tests for generalized reference frames (Issue #70)."""

from rdflib import Graph, Literal, Namespace
from rdflib.namespace import RDF, XSD

from tests._graph_nt import run_shacl

EX = Namespace("https://example.org/test/")
GMEOW = Namespace("https://blackcatinformatics.ca/gmeow/")


def test_measurement_reference_frame_passes() -> None:
    """A measurement reference frame (SI) passes SHACL."""
    g = Graph()
    g.add((EX.siFrame, RDF.type, GMEOW.ReferenceFrame))
    g.add((EX.siFrame, GMEOW.frameRealm, GMEOW.frameRealmMeasurement))
    g.add((EX.siFrame, GMEOW.hasAxis, EX.axisScalar))
    g.add(
        (EX.siFrame, GMEOW.dimensionCount, Literal(1, datatype=XSD.nonNegativeInteger))
    )
    g.add((EX.siFrame, GMEOW.frameKind, GMEOW.frameKindScalar))
    g.add((EX.siFrame, GMEOW.requiresHost, Literal(False)))
    g.add((EX.siFrame, GMEOW.determinacyModel, GMEOW.determinacyCrisp))

    g.add((GMEOW.frameRealmMeasurement, RDF.type, GMEOW.FrameRealm))
    g.add((EX.axisScalar, RDF.type, GMEOW.Axis))
    g.add((GMEOW.frameKindScalar, RDF.type, GMEOW.FrameKind))
    g.add((GMEOW.determinacyCrisp, RDF.type, GMEOW.Determinacy))

    result = run_shacl(g)
    assert result.ok, "\n".join(result.errors)


def test_currency_reference_frame_passes() -> None:
    """A currency reference frame (USD) passes SHACL."""
    g = Graph()
    g.add((EX.usdFrame, RDF.type, GMEOW.ReferenceFrame))
    g.add((EX.usdFrame, GMEOW.frameRealm, GMEOW.frameRealmCurrency))
    g.add((EX.usdFrame, GMEOW.hasAxis, EX.axisScalar))
    g.add(
        (EX.usdFrame, GMEOW.dimensionCount, Literal(1, datatype=XSD.nonNegativeInteger))
    )
    g.add((EX.usdFrame, GMEOW.frameKind, GMEOW.frameKindScalar))
    g.add((EX.usdFrame, GMEOW.requiresHost, Literal(False)))
    g.add((EX.usdFrame, GMEOW.determinacyModel, GMEOW.determinacyCrisp))

    g.add((GMEOW.frameRealmCurrency, RDF.type, GMEOW.FrameRealm))
    g.add((EX.axisScalar, RDF.type, GMEOW.Axis))
    g.add((GMEOW.frameKindScalar, RDF.type, GMEOW.FrameKind))
    g.add((GMEOW.determinacyCrisp, RDF.type, GMEOW.Determinacy))

    result = run_shacl(g)
    assert result.ok, "\n".join(result.errors)


def test_temporal_reference_frame_passes() -> None:
    """A temporal reference frame (Gregorian) passes SHACL."""
    g = Graph()
    g.add((EX.gregorianFrame, RDF.type, GMEOW.ReferenceFrame))
    g.add((EX.gregorianFrame, GMEOW.frameRealm, GMEOW.frameRealmTemporal))
    g.add((EX.gregorianFrame, GMEOW.hasAxis, EX.axisYear))
    g.add((EX.gregorianFrame, GMEOW.hasAxis, EX.axisMonth))
    g.add((EX.gregorianFrame, GMEOW.hasAxis, EX.axisDay))
    g.add(
        (
            EX.gregorianFrame,
            GMEOW.dimensionCount,
            Literal(3, datatype=XSD.nonNegativeInteger),
        )
    )
    g.add((EX.gregorianFrame, GMEOW.frameKind, GMEOW.frameKindTemporal))
    g.add((EX.gregorianFrame, GMEOW.requiresHost, Literal(False)))
    g.add((EX.gregorianFrame, GMEOW.determinacyModel, GMEOW.determinacyCrisp))

    g.add((GMEOW.frameRealmTemporal, RDF.type, GMEOW.FrameRealm))
    for axis in [EX.axisYear, EX.axisMonth, EX.axisDay]:
        g.add((axis, RDF.type, GMEOW.Axis))
    g.add((GMEOW.frameKindTemporal, RDF.type, GMEOW.FrameKind))
    g.add((GMEOW.determinacyCrisp, RDF.type, GMEOW.Determinacy))

    result = run_shacl(g)
    assert result.ok, "\n".join(result.errors)


def test_colourspace_reference_frame_passes() -> None:
    """A colourspace reference frame (sRGB) passes SHACL."""
    g = Graph()
    g.add((EX.srgbFrame, RDF.type, GMEOW.ReferenceFrame))
    g.add((EX.srgbFrame, GMEOW.frameRealm, GMEOW.frameRealmColourspace))
    g.add((EX.srgbFrame, GMEOW.hasAxis, EX.axisRed))
    g.add((EX.srgbFrame, GMEOW.hasAxis, EX.axisGreen))
    g.add((EX.srgbFrame, GMEOW.hasAxis, EX.axisBlue))
    g.add(
        (
            EX.srgbFrame,
            GMEOW.dimensionCount,
            Literal(3, datatype=XSD.nonNegativeInteger),
        )
    )
    g.add((EX.srgbFrame, GMEOW.frameKind, GMEOW.frameKindCartesian))
    g.add((EX.srgbFrame, GMEOW.requiresHost, Literal(False)))
    g.add((EX.srgbFrame, GMEOW.determinacyModel, GMEOW.determinacyCrisp))

    g.add((GMEOW.frameRealmColourspace, RDF.type, GMEOW.FrameRealm))
    for axis in [EX.axisRed, EX.axisGreen, EX.axisBlue]:
        g.add((axis, RDF.type, GMEOW.Axis))
    g.add((GMEOW.frameKindCartesian, RDF.type, GMEOW.FrameKind))
    g.add((GMEOW.determinacyCrisp, RDF.type, GMEOW.Determinacy))

    result = run_shacl(g)
    assert result.ok, "\n".join(result.errors)


def test_linguistic_reference_frame_passes() -> None:
    """A linguistic reference frame (English) passes SHACL."""
    g = Graph()
    g.add((EX.englishFrame, RDF.type, GMEOW.ReferenceFrame))
    g.add((EX.englishFrame, GMEOW.frameRealm, GMEOW.frameRealmLinguistic))
    g.add((EX.englishFrame, GMEOW.hasAxis, EX.axisScalar))
    g.add(
        (
            EX.englishFrame,
            GMEOW.dimensionCount,
            Literal(1, datatype=XSD.nonNegativeInteger),
        )
    )
    g.add((EX.englishFrame, GMEOW.frameKind, GMEOW.frameKindScalar))
    g.add((EX.englishFrame, GMEOW.requiresHost, Literal(False)))
    g.add((EX.englishFrame, GMEOW.determinacyModel, GMEOW.determinacyCrisp))

    g.add((GMEOW.frameRealmLinguistic, RDF.type, GMEOW.FrameRealm))
    g.add((EX.axisScalar, RDF.type, GMEOW.Axis))
    g.add((GMEOW.frameKindScalar, RDF.type, GMEOW.FrameKind))
    g.add((GMEOW.determinacyCrisp, RDF.type, GMEOW.Determinacy))

    result = run_shacl(g)
    assert result.ok, "\n".join(result.errors)


def test_mathematical_reference_frames_pass_shacl() -> None:
    """Mathematical reference frames (phase space, Hilbert, latent,
    C-space) pass SHACL."""
    g = Graph()
    g.add((GMEOW.frameRealmMathematical, RDF.type, GMEOW.FrameRealm))
    g.add((GMEOW.determinacyCrisp, RDF.type, GMEOW.Determinacy))

    # Phase space 3-DOF
    g.add((EX.phaseSpace3DOF, RDF.type, GMEOW.ReferenceFrame))
    g.add((EX.phaseSpace3DOF, GMEOW.frameRealm, GMEOW.frameRealmMathematical))
    g.add((EX.phaseSpace3DOF, GMEOW.hasAxis, EX.axisGeneralizedCoordinate))
    g.add((EX.phaseSpace3DOF, GMEOW.hasAxis, EX.axisGeneralizedMomentum))
    g.add(
        (
            EX.phaseSpace3DOF,
            GMEOW.dimensionCount,
            Literal(2, datatype=XSD.nonNegativeInteger),
        )
    )
    g.add((EX.phaseSpace3DOF, GMEOW.frameKind, GMEOW.frameKindPhaseSpace))
    g.add((EX.phaseSpace3DOF, GMEOW.requiresHost, Literal(False)))
    g.add((EX.phaseSpace3DOF, GMEOW.determinacyModel, GMEOW.determinacyCrisp))
    g.add((EX.phaseSpace3DOF, GMEOW.hasMetricKind, GMEOW.metricSymplectic))

    # Hilbert space
    g.add((EX.hilbertSpace, RDF.type, GMEOW.ReferenceFrame))
    g.add((EX.hilbertSpace, GMEOW.frameRealm, GMEOW.frameRealmMathematical))
    g.add((EX.hilbertSpace, GMEOW.hasAxis, EX.axisHilbertState))
    g.add(
        (
            EX.hilbertSpace,
            GMEOW.dimensionCount,
            Literal(1, datatype=XSD.nonNegativeInteger),
        )
    )
    g.add((EX.hilbertSpace, GMEOW.frameKind, GMEOW.frameKindHilbert))
    g.add((EX.hilbertSpace, GMEOW.requiresHost, Literal(False)))
    g.add((EX.hilbertSpace, GMEOW.determinacyModel, GMEOW.determinacyCrisp))
    g.add((EX.hilbertSpace, GMEOW.hasMetricKind, GMEOW.metricEuclidean))

    # Latent vector space
    g.add((EX.latentVectorSpace, RDF.type, GMEOW.ReferenceFrame))
    g.add((EX.latentVectorSpace, GMEOW.frameRealm, GMEOW.frameRealmMathematical))
    g.add((EX.latentVectorSpace, GMEOW.hasAxis, EX.axisLatentVector))
    g.add(
        (
            EX.latentVectorSpace,
            GMEOW.dimensionCount,
            Literal(1, datatype=XSD.nonNegativeInteger),
        )
    )
    g.add((EX.latentVectorSpace, GMEOW.frameKind, GMEOW.frameKindLatentSpace))
    g.add((EX.latentVectorSpace, GMEOW.requiresHost, Literal(False)))
    g.add((EX.latentVectorSpace, GMEOW.determinacyModel, GMEOW.determinacyCrisp))
    g.add((EX.latentVectorSpace, GMEOW.hasMetricKind, GMEOW.metricCosine))

    # Robot arm C-space
    g.add((EX.robotArm6DOF, RDF.type, GMEOW.ReferenceFrame))
    g.add((EX.robotArm6DOF, GMEOW.frameRealm, GMEOW.frameRealmMathematical))
    g.add((EX.robotArm6DOF, GMEOW.hasAxis, EX.axisJointAngle1))
    g.add((EX.robotArm6DOF, GMEOW.hasAxis, EX.axisJointAngle2))
    g.add((EX.robotArm6DOF, GMEOW.hasAxis, EX.axisJointAngle3))
    g.add((EX.robotArm6DOF, GMEOW.hasAxis, EX.axisJointAngle4))
    g.add((EX.robotArm6DOF, GMEOW.hasAxis, EX.axisJointAngle5))
    g.add((EX.robotArm6DOF, GMEOW.hasAxis, EX.axisJointAngle6))
    g.add(
        (
            EX.robotArm6DOF,
            GMEOW.dimensionCount,
            Literal(6, datatype=XSD.nonNegativeInteger),
        )
    )
    g.add((EX.robotArm6DOF, GMEOW.frameKind, GMEOW.frameKindManifold))
    g.add((EX.robotArm6DOF, GMEOW.requiresHost, Literal(True)))
    g.add((EX.robotArm6DOF, GMEOW.determinacyModel, GMEOW.determinacyCrisp))
    g.add((EX.robotArm6DOF, GMEOW.hasMetricKind, GMEOW.metricEuclidean))

    # Type declarations for value individuals
    for axis in (
        EX.axisGeneralizedCoordinate,
        EX.axisGeneralizedMomentum,
        EX.axisHilbertState,
        EX.axisLatentVector,
        EX.axisJointAngle1,
        EX.axisJointAngle2,
        EX.axisJointAngle3,
        EX.axisJointAngle4,
        EX.axisJointAngle5,
        EX.axisJointAngle6,
    ):
        g.add((axis, RDF.type, GMEOW.Axis))
    for kind in (
        GMEOW.frameKindPhaseSpace,
        GMEOW.frameKindHilbert,
        GMEOW.frameKindLatentSpace,
        GMEOW.frameKindManifold,
    ):
        g.add((kind, RDF.type, GMEOW.FrameKind))
    for metric in (GMEOW.metricSymplectic, GMEOW.metricEuclidean, GMEOW.metricCosine):
        g.add((metric, RDF.type, GMEOW.MetricKind))

    result = run_shacl(g)
    assert result.ok, "\n".join(result.errors)


def test_narrative_reference_frame_passes() -> None:
    """A narrative reference frame (Harry Potter canon) passes SHACL."""
    g = Graph()
    g.add((EX.hpCanon, RDF.type, GMEOW.NarrativeReferenceFrame))
    g.add((EX.hpCanon, GMEOW.frameRealm, GMEOW.frameRealmNarrative))
    g.add((EX.hpCanon, GMEOW.hasAxis, EX.axisPlot))
    g.add(
        (
            EX.hpCanon,
            GMEOW.dimensionCount,
            Literal(1, datatype=XSD.nonNegativeInteger),
        )
    )
    g.add((EX.hpCanon, GMEOW.frameKind, GMEOW.frameKindNarrative))
    g.add((EX.hpCanon, GMEOW.requiresHost, Literal(False)))
    g.add((EX.hpCanon, GMEOW.determinacyModel, GMEOW.determinacyCrisp))

    g.add((GMEOW.frameRealmNarrative, RDF.type, GMEOW.FrameRealm))
    g.add((EX.axisPlot, RDF.type, GMEOW.Axis))
    g.add((GMEOW.frameKindNarrative, RDF.type, GMEOW.FrameKind))
    g.add((GMEOW.determinacyCrisp, RDF.type, GMEOW.Determinacy))

    result = run_shacl(g)
    assert result.ok, "\n".join(result.errors)


def test_biological_reference_frame_passes() -> None:
    """A biological reference frame (GRCh38) and a SequenceFeature with
    SequenceCoordinates pass SHACL."""
    g = Graph()
    g.add((GMEOW.frameRealmBiological, RDF.type, GMEOW.FrameRealm))
    g.add((GMEOW.frameKindLinearSequence, RDF.type, GMEOW.FrameKind))
    g.add((GMEOW.axisSequencePosition, RDF.type, GMEOW.Axis))
    g.add((GMEOW.determinacyCrisp, RDF.type, GMEOW.Determinacy))
    g.add((GMEOW.metricPositionalDistance, RDF.type, GMEOW.MetricKind))
    g.add((GMEOW.strandForward, RDF.type, GMEOW.StrandOrientation))
    g.add((GMEOW.sequenceFeatureTypeGene, RDF.type, GMEOW.SequenceFeatureType))

    # GRCh38 reference frame
    g.add((EX.grch38, RDF.type, GMEOW.ReferenceFrame))
    g.add((EX.grch38, GMEOW.frameRealm, GMEOW.frameRealmBiological))
    g.add((EX.grch38, GMEOW.hasAxis, GMEOW.axisSequencePosition))
    g.add(
        (
            EX.grch38,
            GMEOW.dimensionCount,
            Literal(1, datatype=XSD.nonNegativeInteger),
        )
    )
    g.add((EX.grch38, GMEOW.frameKind, GMEOW.frameKindLinearSequence))
    g.add((EX.grch38, GMEOW.requiresHost, Literal(False)))
    g.add((EX.grch38, GMEOW.determinacyModel, GMEOW.determinacyCrisp))
    g.add((EX.grch38, GMEOW.hasMetricKind, GMEOW.metricPositionalDistance))

    # A gene on chromosome 1
    g.add((EX.gene1, RDF.type, GMEOW.SequenceFeature))
    g.add((EX.gene1, GMEOW.sequenceFeatureType, GMEOW.sequenceFeatureTypeGene))

    # Sequence coordinates
    g.add((EX.coords1, RDF.type, GMEOW.SequenceCoordinates))
    g.add(
        (
            EX.coords1,
            GMEOW.sequenceStart,
            Literal(1000000, datatype=XSD.positiveInteger),
        )
    )
    g.add(
        (
            EX.coords1,
            GMEOW.sequenceEnd,
            Literal(1100000, datatype=XSD.positiveInteger),
        )
    )
    g.add((EX.coords1, GMEOW.sequenceStrand, GMEOW.strandForward))
    g.add((EX.coords1, GMEOW.inReferenceAssembly, EX.grch38))

    g.add((EX.gene1, GMEOW.hasSequenceCoordinates, EX.coords1))

    result = run_shacl(g)
    assert result.ok, "\n".join(result.errors)
