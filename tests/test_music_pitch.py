"""Pitch frame structural guards (issue #308).

Principles 4, 9, 11, 12, 15, 16.
"""

from __future__ import annotations

from gmeow_rdf.compat.rdflib import OWL, RDF, RDFS, Graph, Literal, Namespace, URIRef
from gmeow_rdf.compat.rdflib.namespace import XSD

from gmeow_tools.graph import load_merged_graph
from gmeow_tools.validate import ValidationResult
from tests._graph_nt import run_shacl

GMEOW = Namespace("https://blackcatinformatics.ca/gmeow/")
GUFO = Namespace("http://purl.org/nemo/gufo#")
EX = Namespace("https://example.org/test-music-pitch/")


def _graph() -> Graph:
    return load_merged_graph(include_imports=False)


def test_tuning_system_is_reference_frame() -> None:
    graph = _graph()
    assert (
        URIRef(GMEOW + "TuningSystem"),
        RDFS.subClassOf,
        URIRef(GMEOW + "ReferenceFrame"),
    ) in graph


def test_tuning_system_kind_is_quality_value() -> None:
    graph = _graph()
    assert (
        URIRef(GMEOW + "TuningSystemKind"),
        RDFS.subClassOf,
        URIRef(GUFO + "QualityValue"),
    ) in graph


def test_pitch_anchor_is_functional() -> None:
    graph = _graph()
    assert (
        URIRef(GMEOW + "pitchAnchorOf"),
        RDF.type,
        OWL.FunctionalProperty,
    ) in graph


def test_has_tuning_frame_subproperty() -> None:
    graph = _graph()
    assert (
        URIRef(GMEOW + "hasTuningFrame"),
        RDFS.subPropertyOf,
        URIRef(GMEOW + "hasReferenceFrame"),
    ) in graph


def test_tuning_kind_is_functional() -> None:
    graph = _graph()
    assert (
        URIRef(GMEOW + "tuningKind"),
        RDF.type,
        OWL.FunctionalProperty,
    ) in graph


def test_tuning_frame_properties_are_not_functional() -> None:
    """tuningAnchor, hasTuningFrame, and derivedFromSpectrum may have many values."""
    graph = _graph()
    for prop in ("tuningAnchor", "hasTuningFrame", "derivedFromSpectrum"):
        assert (
            URIRef(GMEOW + prop),
            RDF.type,
            OWL.FunctionalProperty,
        ) not in graph, f"{prop} must not be declared owl:FunctionalProperty"


def test_tuning_system_seeds_coexist() -> None:
    graph = _graph()
    for iri in (
        "tuningSystem12EDO",
        "tuningSystem19EDO",
        "tuningSystem24EDO",
        "tuningSystem31EDO",
        "tuningSystemJustIntonation",
        "tuningSystemPythagorean",
        "tuningSystemQuarterCommaMeantone",
        "tuningSystemPartch43",
        "tuningSystemBohlenPierce",
        "tuningSystemSlendro",
        "tuningSystemPelog",
    ):
        assert (
            URIRef(GMEOW + iri),
            RDF.type,
            URIRef(GMEOW + "TuningSystem"),
        ) in graph, f"missing {iri}"


def test_pitch_anchor_a440_and_a415_coexist() -> None:
    """Two anchors for the same tuning system coexist (Principle 9)."""
    graph = _graph()
    for iri in ("pitchAnchorA440", "pitchAnchorA415"):
        assert (
            URIRef(GMEOW + iri),
            RDF.type,
            URIRef(GMEOW + "PitchAnchor"),
        ) in graph
    assert (
        URIRef(GMEOW + "pitchAnchorA440"),
        URIRef(GMEOW + "pitchAnchorOf"),
        URIRef(GMEOW + "tuningSystem12EDO"),
    ) in graph
    assert (
        URIRef(GMEOW + "pitchAnchorA415"),
        URIRef(GMEOW + "pitchAnchorOf"),
        URIRef(GMEOW + "tuningSystem12EDO"),
    ) in graph


def test_slendro_requires_host_but_12edo_does_not() -> None:
    graph = _graph()
    assert (
        URIRef(GMEOW + "tuningSystemSlendro"),
        URIRef(GMEOW + "requiresHost"),
        Literal(True),
    ) in graph
    assert (
        URIRef(GMEOW + "tuningSystem12EDO"),
        URIRef(GMEOW + "requiresHost"),
        Literal(False),
    ) in graph


def _error_text(result: ValidationResult) -> str:
    """Flatten a ValidationResult.errors list for substring checks."""
    return "\n".join(result.errors)


def test_pitch_value_ratio_only_passes_shacl() -> None:
    g = Graph()
    g.bind("gmeow", GMEOW)
    g.bind("ex", EX)
    value = EX.valueRatio
    g.add((value, RDF.type, GMEOW.PitchValue))
    g.add((value, GMEOW.hasTuningFrame, GMEOW.tuningSystemJustIntonation))
    g.add((value, GMEOW.ratioNumerator, Literal(3, datatype=XSD.integer)))
    g.add((value, GMEOW.ratioDenominator, Literal(2, datatype=XSD.integer)))
    result = run_shacl(g)
    assert result.ok, _error_text(result)


def test_pitch_value_cents_only_passes_shacl() -> None:
    g = Graph()
    g.bind("gmeow", GMEOW)
    g.bind("ex", EX)
    value = EX.valueCents
    g.add((value, RDF.type, GMEOW.PitchValue))
    g.add((value, GMEOW.hasTuningFrame, GMEOW.tuningSystem12EDO))
    g.add((value, GMEOW.centsFromOrigin, Literal("700.0", datatype=XSD.decimal)))
    result = run_shacl(g)
    assert result.ok, _error_text(result)


def test_pitch_value_missing_frame_fails_shacl() -> None:
    g = Graph()
    g.bind("gmeow", GMEOW)
    g.bind("ex", EX)
    value = EX.valueNoFrame
    g.add((value, RDF.type, GMEOW.PitchValue))
    g.add((value, GMEOW.ratioNumerator, Literal(3, datatype=XSD.integer)))
    g.add((value, GMEOW.ratioDenominator, Literal(2, datatype=XSD.integer)))
    result = run_shacl(g)
    assert not result.ok
    assert result.errors
    assert (
        "A PitchValue must be relative to exactly one TuningSystem (Principle 11)."
        in _error_text(result)
    )


def test_pitch_value_ratio_and_cents_fails_shacl() -> None:
    g = Graph()
    g.bind("gmeow", GMEOW)
    g.bind("ex", EX)
    value = EX.valueBoth
    g.add((value, RDF.type, GMEOW.PitchValue))
    g.add((value, GMEOW.hasTuningFrame, GMEOW.tuningSystemJustIntonation))
    g.add((value, GMEOW.ratioNumerator, Literal(3, datatype=XSD.integer)))
    g.add((value, GMEOW.ratioDenominator, Literal(2, datatype=XSD.integer)))
    g.add((value, GMEOW.centsFromOrigin, Literal("701.96", datatype=XSD.decimal)))
    result = run_shacl(g)
    assert not result.ok
    assert result.errors
    expected = (
        "A PitchValue must provide exactly one encoding: "
        "either (ratioNumerator + ratioDenominator) or centsFromOrigin."
    )
    assert expected in _error_text(result)


def test_pitch_value_zero_denominator_fails_shacl() -> None:
    g = Graph()
    g.bind("gmeow", GMEOW)
    g.bind("ex", EX)
    value = EX.valueZeroDenom
    g.add((value, RDF.type, GMEOW.PitchValue))
    g.add((value, GMEOW.hasTuningFrame, GMEOW.tuningSystemJustIntonation))
    g.add((value, GMEOW.ratioNumerator, Literal(3, datatype=XSD.integer)))
    g.add((value, GMEOW.ratioDenominator, Literal(0, datatype=XSD.integer)))
    result = run_shacl(g)
    assert not result.ok
    assert result.errors
    assert "The ratio denominator must be a positive integer." in _error_text(result)


def test_pitch_interval_xor_ratio_cents() -> None:
    """An interval must carry ratio or cents, not both, not neither."""
    g = Graph()
    g.bind("gmeow", GMEOW)
    g.bind("ex", EX)

    # missing both
    interval = EX.intervalNone
    g.add((interval, RDF.type, GMEOW.PitchInterval))
    g.add((interval, GMEOW.hasTuningFrame, GMEOW.tuningSystemJustIntonation))
    result = run_shacl(g)
    assert not result.ok
    assert result.errors
    expected_interval = (
        "A PitchInterval must provide exactly one encoding: "
        "either (ratioNumerator + ratioDenominator) or centsFromOrigin."
    )
    assert expected_interval in _error_text(result)

    # both
    g2 = Graph()
    g2.bind("gmeow", GMEOW)
    g2.bind("ex", EX)
    interval2 = EX.intervalBoth
    g2.add((interval2, RDF.type, GMEOW.PitchInterval))
    g2.add((interval2, GMEOW.hasTuningFrame, GMEOW.tuningSystemJustIntonation))
    g2.add((interval2, GMEOW.ratioNumerator, Literal(3, datatype=XSD.integer)))
    g2.add((interval2, GMEOW.ratioDenominator, Literal(2, datatype=XSD.integer)))
    g2.add((interval2, GMEOW.centsFromOrigin, Literal("701.96", datatype=XSD.decimal)))
    result2 = run_shacl(g2)
    assert not result2.ok
    assert result2.errors
    assert expected_interval in _error_text(result2)

    # ratio only
    g3 = Graph()
    g3.bind("gmeow", GMEOW)
    g3.bind("ex", EX)
    interval3 = EX.intervalRatio
    g3.add((interval3, RDF.type, GMEOW.PitchInterval))
    g3.add((interval3, GMEOW.hasTuningFrame, GMEOW.tuningSystemJustIntonation))
    g3.add((interval3, GMEOW.ratioNumerator, Literal(3, datatype=XSD.integer)))
    g3.add((interval3, GMEOW.ratioDenominator, Literal(2, datatype=XSD.integer)))
    result3 = run_shacl(g3)
    assert result3.ok, _error_text(result3)


def test_tuning_system_shape_requires_kind_and_realm() -> None:
    g = Graph()
    g.bind("gmeow", GMEOW)
    g.bind("ex", EX)
    ts = EX.tuningBad
    g.add((ts, RDF.type, GMEOW.TuningSystem))
    g.add((ts, GMEOW.frameRealm, GMEOW.frameRealmMusicalPitch))
    g.add((ts, GMEOW.frameKind, GMEOW.frameKindScalar))
    g.add((ts, GMEOW.requiresHost, Literal(False)))
    # missing tuningKind
    result = run_shacl(g)
    assert not result.ok
    assert result.errors
    assert (
        "A TuningSystem must have exactly one tuningKind (Principle 9)."
        in _error_text(result)
    )


def test_no_direct_frequency_property_on_pitch_value() -> None:
    """Hz is reached only through fnPitchToFrequency + PitchAnchor (Principle 11)."""
    graph = _graph()
    for candidate in ("frequency", "pitchFrequency", "hz"):
        prop = URIRef(GMEOW + candidate)
        assert (prop, RDF.type, OWL.DatatypeProperty) not in graph
        assert (prop, RDF.type, OWL.ObjectProperty) not in graph
