"""Tests for generalized reference frames (Issue #70)."""

from rdflib import Graph, Literal, Namespace
from rdflib.namespace import RDF, XSD

from gmeow_tools.validate import run_shacl

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
