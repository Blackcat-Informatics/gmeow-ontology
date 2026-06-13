"""Musical-time layer structural guards (issue #310).

Principles 4, 9, 10, 11, 12, 15, 16.
"""

from __future__ import annotations

from rdflib import OWL, RDF, RDFS, Graph, Literal, Namespace, URIRef
from rdflib.namespace import XSD

from gmeow_tools.graph import load_merged_graph
from gmeow_tools.validate import ValidationResult, run_shacl

GMEOW = Namespace("https://blackcatinformatics.ca/gmeow/")
GUFO = Namespace("http://purl.org/nemo/gufo#")
EX = Namespace("https://example.org/test-music-time/")


def _graph() -> Graph:
    return load_merged_graph(include_imports=False)


def _error_text(result: ValidationResult) -> str:
    return "\n".join(result.errors)


def test_musical_time_frame_is_reference_frame() -> None:
    graph = _graph()
    assert (
        URIRef(GMEOW + "MusicalTimeFrame"),
        RDFS.subClassOf,
        URIRef(GMEOW + "ReferenceFrame"),
    ) in graph


def test_tempo_map_is_time_mapping() -> None:
    graph = _graph()
    assert (
        URIRef(GMEOW + "TempoMap"),
        RDFS.subClassOf,
        URIRef(GMEOW + "TimeMapping"),
    ) in graph


def test_has_musical_time_frame_subproperty() -> None:
    graph = _graph()
    assert (
        URIRef(GMEOW + "hasMusicalTimeFrame"),
        RDFS.subPropertyOf,
        URIRef(GMEOW + "hasReferenceFrame"),
    ) in graph


def test_ontology_properties_multi_source_functionality() -> None:
    graph = _graph()
    constitutive = [
        "timeMappingKind",
        "tempoMapSegmentOf",
        "segmentSpan",
        "segmentTempoMapKind",
        "segmentMapRatioNumerator",
        "segmentMapRatioDenominator",
        "metricStructureOf",
        "metricGroupOrder",
        "meterCarrier",
        "assignedMeter",
        "assignmentSpan",
        "modulationFromFrame",
        "modulationToFrame",
        "grooveKind",
        "grooveGridUnit",
        "timeStartNumerator",
        "timeStartDenominator",
        "timeDurationNumerator",
        "timeDurationDenominator",
        "mapRatioNumerator",
        "mapRatioDenominator",
        "tempoRatioExpression",
        "groupLengthNumerator",
        "groupLengthDenominator",
        "pivotSourceValue",
        "pivotTargetValue",
    ]
    for prop in constitutive:
        assert (
            URIRef(GMEOW + prop),
            RDF.type,
            OWL.FunctionalProperty,
        ) in graph, f"{prop} should be functional"

    source_variable = ["tempoRatioApprox", "groupAccentWeight"]
    for prop in source_variable:
        assert (
            URIRef(GMEOW + prop),
            RDF.type,
            OWL.FunctionalProperty,
        ) not in graph, f"{prop} should NOT be functional"


def test_common_musical_time_frame_exists() -> None:
    graph = _graph()
    assert (
        URIRef(GMEOW + "musicalTimeFrameCommon"),
        RDF.type,
        URIRef(GMEOW + "MusicalTimeFrame"),
    ) in graph


def test_tempo_map_common_exists() -> None:
    graph = _graph()
    assert (
        URIRef(GMEOW + "tempoMapCommon"),
        RDF.type,
        URIRef(GMEOW + "TempoMap"),
    ) in graph


def test_meter_sequence_fixtures_exist() -> None:
    graph = _graph()
    for term in ("metricStructure58", "metricStructure78", "metricStructure44"):
        assert (
            URIRef(GMEOW + term),
            RDF.type,
            URIRef(GMEOW + "MetricStructure"),
        ) in graph


def test_polymeter_assignments_exist() -> None:
    graph = _graph()
    for term in ("meterAssignmentGuitar78", "meterAssignmentDrums44"):
        assert (
            URIRef(GMEOW + term),
            RDF.type,
            URIRef(GMEOW + "MeterAssignment"),
        ) in graph


def test_polymeter_pattern_exists() -> None:
    graph = _graph()
    assert (
        URIRef(GMEOW + "polymeterPattern"),
        RDF.type,
        URIRef(GMEOW + "Entity"),
    ) in graph


def test_tuplet_fixtures_exist() -> None:
    graph = _graph()
    for term in ("timeMappingTuplet32", "timeMappingTuplet54"):
        assert (
            URIRef(GMEOW + term),
            RDF.type,
            URIRef(GMEOW + "TimeMapping"),
        ) in graph


def test_sqrt2_canon_fixture_exists() -> None:
    graph = _graph()
    tm = URIRef(GMEOW + "timeMappingSqrt2Canon")
    assert (tm, RDF.type, URIRef(GMEOW + "TimeMapping")) in graph
    assert (
        tm,
        URIRef(GMEOW + "tempoRatioExpression"),
        Literal("sqrt(2)/2", datatype=XSD.string),
    ) in graph


def test_musical_time_span_valid_passes_shacl() -> None:
    g = Graph()
    g.bind("gmeow", GMEOW)
    g.bind("ex", EX)
    span = EX.spanValid
    g.add((span, RDF.type, GMEOW.MusicalTimeSpan))
    g.add((span, GMEOW.hasMusicalTimeFrame, GMEOW.musicalTimeFrameCommon))
    g.add((span, GMEOW.timeStartNumerator, Literal(0, datatype=XSD.integer)))
    g.add((span, GMEOW.timeStartDenominator, Literal(1, datatype=XSD.integer)))
    g.add((span, GMEOW.timeDurationNumerator, Literal(5, datatype=XSD.integer)))
    g.add((span, GMEOW.timeDurationDenominator, Literal(8, datatype=XSD.integer)))
    result = run_shacl(g)
    assert result.ok, _error_text(result)


def test_musical_time_span_missing_frame_fails_shacl() -> None:
    g = Graph()
    g.bind("gmeow", GMEOW)
    g.bind("ex", EX)
    span = EX.spanNoFrame
    g.add((span, RDF.type, GMEOW.MusicalTimeSpan))
    g.add((span, GMEOW.timeStartNumerator, Literal(0, datatype=XSD.integer)))
    g.add((span, GMEOW.timeStartDenominator, Literal(1, datatype=XSD.integer)))
    g.add((span, GMEOW.timeDurationNumerator, Literal(5, datatype=XSD.integer)))
    g.add((span, GMEOW.timeDurationDenominator, Literal(8, datatype=XSD.integer)))
    result = run_shacl(g)
    assert not result.ok
    assert result.errors
    assert "exactly one MusicalTimeFrame" in _error_text(result)


def test_musical_time_span_zero_denominator_fails_shacl() -> None:
    g = Graph()
    g.bind("gmeow", GMEOW)
    g.bind("ex", EX)
    span = EX.spanZeroDenom
    g.add((span, RDF.type, GMEOW.MusicalTimeSpan))
    g.add((span, GMEOW.hasMusicalTimeFrame, GMEOW.musicalTimeFrameCommon))
    g.add((span, GMEOW.timeStartNumerator, Literal(0, datatype=XSD.integer)))
    g.add((span, GMEOW.timeStartDenominator, Literal(0, datatype=XSD.integer)))
    g.add((span, GMEOW.timeDurationNumerator, Literal(5, datatype=XSD.integer)))
    g.add((span, GMEOW.timeDurationDenominator, Literal(8, datatype=XSD.integer)))
    result = run_shacl(g)
    assert not result.ok
    assert result.errors
    assert "positive start denominator" in _error_text(result)


def test_time_mapping_rational_passes_shacl() -> None:
    g = Graph()
    g.bind("gmeow", GMEOW)
    g.bind("ex", EX)
    tm = EX.tupletValid
    g.add((tm, RDF.type, GMEOW.TimeMapping))
    g.add((tm, GMEOW.timeMappingKind, GMEOW.timeMappingKindTuplet))
    g.add((tm, GMEOW.mapsFrame, GMEOW.musicalTimeFrameCommon))
    g.add((tm, GMEOW.mapsToFrame, GMEOW.musicalTimeFrameCommon))
    g.add((tm, GMEOW.mapRatioNumerator, Literal(3, datatype=XSD.integer)))
    g.add((tm, GMEOW.mapRatioDenominator, Literal(2, datatype=XSD.integer)))
    result = run_shacl(g)
    assert result.ok, _error_text(result)


def test_time_mapping_irrational_passes_shacl() -> None:
    g = Graph()
    g.bind("gmeow", GMEOW)
    g.bind("ex", EX)
    tm = EX.canonValid
    g.add((tm, RDF.type, GMEOW.TimeMapping))
    g.add((tm, GMEOW.timeMappingKind, GMEOW.timeMappingKindTempoCanon))
    g.add((tm, GMEOW.mapsFrame, GMEOW.musicalTimeFrameVoiceGuitar))
    g.add((tm, GMEOW.mapsToFrame, GMEOW.musicalTimeFrameVoiceDrums))
    g.add((tm, GMEOW.tempoRatioExpression, Literal("sqrt(2)/2", datatype=XSD.string)))
    g.add((tm, GMEOW.tempoRatioApprox, Literal("0.70710678", datatype=XSD.decimal)))
    result = run_shacl(g)
    assert result.ok, _error_text(result)


def test_time_mapping_both_encodings_fails_shacl() -> None:
    g = Graph()
    g.bind("gmeow", GMEOW)
    g.bind("ex", EX)
    tm = EX.tupletBoth
    g.add((tm, RDF.type, GMEOW.TimeMapping))
    g.add((tm, GMEOW.timeMappingKind, GMEOW.timeMappingKindTuplet))
    g.add((tm, GMEOW.mapsFrame, GMEOW.musicalTimeFrameCommon))
    g.add((tm, GMEOW.mapsToFrame, GMEOW.musicalTimeFrameCommon))
    g.add((tm, GMEOW.mapRatioNumerator, Literal(3, datatype=XSD.integer)))
    g.add((tm, GMEOW.mapRatioDenominator, Literal(2, datatype=XSD.integer)))
    g.add((tm, GMEOW.tempoRatioExpression, Literal("sqrt(2)/2", datatype=XSD.string)))
    g.add((tm, GMEOW.tempoRatioApprox, Literal("0.70710678", datatype=XSD.decimal)))
    result = run_shacl(g)
    assert not result.ok
    assert result.errors
    assert "exactly one encoding" in _error_text(result)


def test_tempo_map_segment_backed_passes_shacl() -> None:
    g = Graph()
    g.bind("gmeow", GMEOW)
    g.bind("ex", EX)
    tm = EX.tempoMapSegmented
    seg = EX.tempoMapSegmentOne
    g.add((tm, RDF.type, GMEOW.TempoMap))
    g.add((tm, GMEOW.timeMappingKind, GMEOW.timeMappingKindTempoMap))
    g.add((tm, GMEOW.mapsFrame, GMEOW.musicalTimeFrameCommon))
    g.add((tm, GMEOW.mapsToFrame, GMEOW.temporalFrameTAI))
    g.add((tm, GMEOW.hasTempoMapSegment, seg))
    g.add((seg, RDF.type, GMEOW.TempoMapSegment))
    g.add((seg, GMEOW.tempoMapSegmentOf, tm))
    g.add((seg, GMEOW.segmentSpan, GMEOW.musicalTimeSpanWholeSection))
    g.add((seg, GMEOW.segmentTempoMapKind, GMEOW.tempoMapKindConstant))
    g.add((seg, GMEOW.segmentMapRatioNumerator, Literal(1, datatype=XSD.integer)))
    g.add((seg, GMEOW.segmentMapRatioDenominator, Literal(2, datatype=XSD.integer)))
    result = run_shacl(g)
    assert result.ok, _error_text(result)


def test_non_tempo_map_segment_backed_fails_shacl() -> None:
    g = Graph()
    g.bind("gmeow", GMEOW)
    g.bind("ex", EX)
    tm = EX.tupletWithSegment
    seg = EX.tupletSegmentOne
    g.add((tm, RDF.type, GMEOW.TimeMapping))
    g.add((tm, GMEOW.timeMappingKind, GMEOW.timeMappingKindTuplet))
    g.add((tm, GMEOW.mapsFrame, GMEOW.musicalTimeFrameCommon))
    g.add((tm, GMEOW.mapsToFrame, GMEOW.musicalTimeFrameCommon))
    g.add((tm, GMEOW.hasTempoMapSegment, seg))
    g.add((seg, RDF.type, GMEOW.TempoMapSegment))
    g.add((seg, GMEOW.tempoMapSegmentOf, tm))
    g.add((seg, GMEOW.segmentSpan, GMEOW.musicalTimeSpanWholeSection))
    g.add((seg, GMEOW.segmentTempoMapKind, GMEOW.tempoMapKindConstant))
    g.add((seg, GMEOW.segmentMapRatioNumerator, Literal(1, datatype=XSD.integer)))
    g.add((seg, GMEOW.segmentMapRatioDenominator, Literal(2, datatype=XSD.integer)))
    result = run_shacl(g)
    assert not result.ok
    assert result.errors
    assert "exactly one encoding" in _error_text(result)


def test_meter_assignment_missing_carrier_fails_shacl() -> None:
    g = Graph()
    g.bind("gmeow", GMEOW)
    g.bind("ex", EX)
    ma = EX.meterBad
    g.add((ma, RDF.type, GMEOW.MeterAssignment))
    g.add((ma, GMEOW.assignedMeter, GMEOW.metricStructure44))
    g.add((ma, GMEOW.assignmentSpan, GMEOW.musicalTimeSpanWholeSection))
    result = run_shacl(g)
    assert not result.ok
    assert result.errors
    assert "exactly one carrier" in _error_text(result)


def test_metric_modulation_pivot_format_fails_shacl() -> None:
    g = Graph()
    g.bind("gmeow", GMEOW)
    g.bind("ex", EX)
    mm = EX.modulationBad
    g.add((mm, RDF.type, GMEOW.MetricModulation))
    g.add((mm, GMEOW.modulationFromFrame, GMEOW.musicalTimeFrameCommon))
    g.add((mm, GMEOW.modulationToFrame, GMEOW.musicalTimeFrameVoiceGuitar))
    g.add((mm, GMEOW.pivotSourceValue, Literal("3/8", datatype=XSD.string)))
    g.add((mm, GMEOW.pivotTargetValue, Literal("bad", datatype=XSD.string)))
    result = run_shacl(g)
    assert not result.ok
    assert result.errors
    assert "rational string" in _error_text(result)
