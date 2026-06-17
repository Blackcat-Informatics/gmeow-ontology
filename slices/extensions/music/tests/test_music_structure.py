"""Musical-structure layer guards (structure graph).

Principles 4, 8, 9, 11, 12, 16.
"""

from __future__ import annotations

from rdflib import OWL, RDF, RDFS, Graph, Literal, Namespace, URIRef
from rdflib.namespace import XSD
from tests._graph_nt import run_shacl

from gmeow_tools.graph import load_merged_graph
from gmeow_tools.validate import ValidationResult

GMEOW = Namespace("https://blackcatinformatics.ca/gmeow/")
GUFO = Namespace("http://purl.org/nemo/gufo#")
EX = Namespace("https://example.org/test-music-structure/")


def _graph() -> Graph:
    return load_merged_graph(include_imports=False)


def _error_text(result: ValidationResult) -> str:
    return "\n".join(result.errors)


def test_musical_segment_subclass_of_content_segment() -> None:
    graph = _graph()
    assert (
        URIRef(GMEOW + "MusicalSegment"),
        RDFS.subClassOf,
        URIRef(GMEOW + "ContentSegment"),
    ) in graph


def test_tone_event_subclass_of_musical_segment() -> None:
    graph = _graph()
    assert (
        URIRef(GMEOW + "ToneEvent"),
        RDFS.subClassOf,
        URIRef(GMEOW + "MusicalSegment"),
    ) in graph


def test_segment_kind_values_exist() -> None:
    graph = _graph()
    for term in (
        "segmentKindToneEventContainer",
        "segmentKindMotif",
        "segmentKindRiff",
        "segmentKindCell",
        "segmentKindPhrase",
        "segmentKindSection",
        "segmentKindFragment",
        "segmentKindTalea",
        "segmentKindColor",
        "segmentKindDrone",
        "segmentKindLoop",
    ):
        assert (
            URIRef(GMEOW + term),
            RDF.type,
            URIRef(GMEOW + "SegmentKind"),
        ) in graph, f"{term} should be a SegmentKind"


def test_transformation_types_exist() -> None:
    graph = _graph()
    for term in (
        "transformTransposition",
        "transformInversion",
        "transformRetrograde",
        "transformAugmentation",
        "transformDiminution",
        "transformPhaseShift",
        "transformReaccentuation",
        "transformOctaveDisplacement",
        "transformTimbreReorchestration",
        "transformSpectralCompression",
        "transformOrnamentation",
        "transformQuotation",
        "transformReduction",
    ):
        assert (
            URIRef(GMEOW + term),
            RDF.type,
            URIRef(GMEOW + "TransformationType"),
        ) in graph, f"{term} should be a TransformationType"


def test_interpolation_kinds_exist() -> None:
    graph = _graph()
    for term in (
        "interpolationLinearCents",
        "interpolationExponential",
        "interpolationStochasticByReference",
    ):
        assert (
            URIRef(GMEOW + term),
            RDF.type,
            URIRef(GMEOW + "PitchTrajectoryInterpolationKind"),
        ) in graph


def test_dynamics_values_exist() -> None:
    graph = _graph()
    for term in (
        "dynamicsPpp",
        "dynamicsPp",
        "dynamicsP",
        "dynamicsMp",
        "dynamicsMf",
        "dynamicsF",
        "dynamicsFf",
        "dynamicsFff",
    ):
        assert (
            URIRef(GMEOW + term),
            RDF.type,
            URIRef(GMEOW + "DynamicsValue"),
        ) in graph


def test_articulation_kinds_exist() -> None:
    graph = _graph()
    for term in (
        "articulationStaccato",
        "articulationLegato",
        "articulationTenuto",
        "articulationAccent",
        "articulationMarcato",
        "articulationPizzicato",
        "articulationHarmonic",
    ):
        assert (
            URIRef(GMEOW + term),
            RDF.type,
            URIRef(GMEOW + "ArticulationKind"),
        ) in graph


def test_riff_transformation_chain_exists() -> None:
    graph = _graph()
    for term in (
        "fixtureStructureRiffA",
        "fixtureStructureRiffATransposed",
        "fixtureStructureRiffAReaccented",
    ):
        assert (
            URIRef(GMEOW + term),
            RDF.type,
            URIRef(GMEOW + "MusicalSegment"),
        ) in graph
    for term in ("fixtureStructureTransposition", "fixtureStructureReaccentuation"):
        assert (
            URIRef(GMEOW + term),
            RDF.type,
            URIRef(GMEOW + "SegmentTransformation"),
        ) in graph


def test_tone_event_fixture_exists() -> None:
    graph = _graph()
    tone_event = URIRef(GMEOW + "fixtureStructureToneEventC4")
    assert (tone_event, RDF.type, URIRef(GMEOW + "ToneEvent")) in graph
    assert (
        tone_event,
        GMEOW.toneEventPitchValue,
        URIRef(GMEOW + "pitchValueC4Fixture"),
    ) in graph


def test_pitch_trajectory_fixture_exists() -> None:
    graph = _graph()
    traj = URIRef(GMEOW + "fixtureStructureGlissando")
    assert (traj, RDF.type, URIRef(GMEOW + "PitchTrajectory")) in graph
    for term in ("fixtureStructureGlissPointC4", "fixtureStructureGlissPointG4"):
        assert (
            URIRef(GMEOW + term),
            RDF.type,
            URIRef(GMEOW + "PitchTrajectoryControlPoint"),
        ) in graph


def test_voice_fixture_exists() -> None:
    graph = _graph()
    voice = URIRef(GMEOW + "fixtureStructureVoiceBass")
    assert (voice, RDF.type, URIRef(GMEOW + "Voice")) in graph
    assert (
        voice,
        GMEOW.voiceTimeFrame,
        URIRef(GMEOW + "musicalTimeFrameCommon"),
    ) in graph
    assert (voice, GMEOW.voiceTuningFrame, URIRef(GMEOW + "tuningSystem12EDO")) in graph


def test_placeholder_voices_retyped() -> None:
    graph = _graph()
    for term in ("voiceGuitarPlaceholder", "voiceDrumsPlaceholder"):
        assert (
            URIRef(GMEOW + term),
            RDF.type,
            URIRef(GMEOW + "Voice"),
        ) in graph


def test_structure_functional_properties() -> None:
    graph = _graph()
    functional = [
        "segmentKind",
        "segmentSpan",
        "toneEventPitchValue",
        "toneEventPitchTrajectory",
        "toneEventDynamics",
        "toneEventArticulation",
        "toneEventTimbre",
        "toneEventIsUnpitched",
        "controlPointOfTrajectory",
        "controlPointPitch",
        "controlPointTimeFrame",
        "controlPointTimePositionNumerator",
        "controlPointTimePositionDenominator",
        "controlPointOrder",
        "interpolationKind",
        "voiceTimeFrame",
        "voiceTuningFrame",
        "voiceMetricStructure",
        "transformationSource",
        "transformationTarget",
        "transformationType",
        "transformationParameter",
    ]
    for prop in functional:
        assert (
            URIRef(GMEOW + prop),
            RDF.type,
            OWL.FunctionalProperty,
        ) in graph, f"{prop} should be functional"


def test_musical_segment_valid_passes_shacl() -> None:
    g = Graph()
    g.bind("gmeow", GMEOW)
    g.bind("ex", EX)
    seg = EX.segmentValid
    g.add((seg, RDF.type, GMEOW.MusicalSegment))
    g.add((seg, GMEOW.segmentKind, GMEOW.segmentKindRiff))
    g.add((seg, GMEOW.segmentSpan, GMEOW.musicalTimeSpanBarOne))
    result = run_shacl(g)
    assert result.ok, _error_text(result)


def test_musical_segment_missing_kind_fails_shacl() -> None:
    g = Graph()
    g.bind("gmeow", GMEOW)
    g.bind("ex", EX)
    seg = EX.segmentNoKind
    g.add((seg, RDF.type, GMEOW.MusicalSegment))
    g.add((seg, GMEOW.segmentSpan, GMEOW.musicalTimeSpanBarOne))
    result = run_shacl(g)
    assert not result.ok
    assert result.errors
    assert "exactly one segmentKind" in _error_text(result)


def test_tone_event_pitch_value_passes_shacl() -> None:
    g = Graph()
    g.bind("gmeow", GMEOW)
    g.bind("ex", EX)
    tone_event = EX.toneEventC4
    g.add((tone_event, RDF.type, GMEOW.ToneEvent))
    g.add((tone_event, GMEOW.segmentKind, GMEOW.segmentKindToneEventContainer))
    g.add((tone_event, GMEOW.segmentSpan, GMEOW.musicalTimeSpanBarOne))
    g.add((tone_event, GMEOW.toneEventPitchValue, GMEOW.pitchValueC4Fixture))
    g.add((tone_event, GMEOW.toneEventDynamics, GMEOW.dynamicsMf))
    result = run_shacl(g)
    assert result.ok, _error_text(result)


def test_tone_event_multiple_pitch_modes_fails_shacl() -> None:
    g = Graph()
    g.bind("gmeow", GMEOW)
    g.bind("ex", EX)
    tone_event = EX.toneEventBad
    g.add((tone_event, RDF.type, GMEOW.ToneEvent))
    g.add((tone_event, GMEOW.segmentKind, GMEOW.segmentKindToneEventContainer))
    g.add((tone_event, GMEOW.segmentSpan, GMEOW.musicalTimeSpanBarOne))
    g.add((tone_event, GMEOW.toneEventPitchValue, GMEOW.pitchValueC4Fixture))
    g.add((tone_event, GMEOW.toneEventIsUnpitched, Literal(True, datatype=XSD.boolean)))
    result = run_shacl(g)
    assert not result.ok
    assert result.errors
    assert "exactly one pitch-content mode" in _error_text(result)


def test_pitch_trajectory_valid_passes_shacl() -> None:
    g = Graph()
    g.bind("gmeow", GMEOW)
    g.bind("ex", EX)
    traj = EX.glissandoValid
    cp1 = EX.glissPointOne
    cp2 = EX.glissPointTwo
    g.add((traj, RDF.type, GMEOW.PitchTrajectory))
    g.add((traj, GMEOW.interpolationKind, GMEOW.interpolationLinearCents))
    g.add((traj, GMEOW.trajectoryControlPoint, cp1))
    g.add((traj, GMEOW.trajectoryControlPoint, cp2))
    for cp, order, _cents in ((cp1, 0, "0.0"), (cp2, 1, "700.0")):
        g.add((cp, RDF.type, GMEOW.PitchTrajectoryControlPoint))
        g.add((cp, GMEOW.controlPointOfTrajectory, traj))
        g.add(
            (
                cp,
                GMEOW.controlPointOrder,
                Literal(order, datatype=XSD.nonNegativeInteger),
            )
        )
        g.add((cp, GMEOW.controlPointTimeFrame, GMEOW.musicalTimeFrameCommon))
        g.add(
            (
                cp,
                GMEOW.controlPointTimePositionNumerator,
                Literal(order, datatype=XSD.integer),
            )
        )
        g.add(
            (
                cp,
                GMEOW.controlPointTimePositionDenominator,
                Literal(1, datatype=XSD.integer),
            )
        )
        g.add(
            (
                cp,
                GMEOW.controlPointPitch,
                GMEOW.pitchValueC4Fixture if order == 0 else GMEOW.pitchValueG4Fixture,
            )
        )
    result = run_shacl(g)
    assert result.ok, _error_text(result)


def test_pitch_trajectory_single_control_point_fails_shacl() -> None:
    g = Graph()
    g.bind("gmeow", GMEOW)
    g.bind("ex", EX)
    traj = EX.glissandoShort
    cp = EX.glissPointOne
    g.add((traj, RDF.type, GMEOW.PitchTrajectory))
    g.add((traj, GMEOW.interpolationKind, GMEOW.interpolationLinearCents))
    g.add((traj, GMEOW.trajectoryControlPoint, cp))
    g.add((cp, RDF.type, GMEOW.PitchTrajectoryControlPoint))
    g.add((cp, GMEOW.controlPointOfTrajectory, traj))
    g.add((cp, GMEOW.controlPointOrder, Literal(0, datatype=XSD.nonNegativeInteger)))
    g.add((cp, GMEOW.controlPointTimeFrame, GMEOW.musicalTimeFrameCommon))
    g.add(
        (cp, GMEOW.controlPointTimePositionNumerator, Literal(0, datatype=XSD.integer))
    )
    g.add(
        (
            cp,
            GMEOW.controlPointTimePositionDenominator,
            Literal(1, datatype=XSD.integer),
        )
    )
    g.add((cp, GMEOW.controlPointPitch, GMEOW.pitchValueC4Fixture))
    result = run_shacl(g)
    assert not result.ok
    assert result.errors
    assert "at least two control points" in _error_text(result)


def test_segment_transformation_valid_passes_shacl() -> None:
    g = Graph()
    g.bind("gmeow", GMEOW)
    g.bind("ex", EX)
    st = EX.transpositionValid
    src = EX.sourceRiff
    tgt = EX.targetRiff
    g.add((st, RDF.type, GMEOW.SegmentTransformation))
    g.add((st, GMEOW.transformationSource, src))
    g.add((st, GMEOW.transformationTarget, tgt))
    g.add((st, GMEOW.transformationType, GMEOW.transformTransposition))
    g.add((src, RDF.type, GMEOW.MusicalSegment))
    g.add((src, GMEOW.segmentKind, GMEOW.segmentKindRiff))
    g.add((tgt, RDF.type, GMEOW.MusicalSegment))
    g.add((tgt, GMEOW.segmentKind, GMEOW.segmentKindRiff))
    result = run_shacl(g)
    assert result.ok, _error_text(result)


def test_segment_transformation_missing_source_fails_shacl() -> None:
    g = Graph()
    g.bind("gmeow", GMEOW)
    g.bind("ex", EX)
    st = EX.transpositionBad
    tgt = EX.targetRiff
    g.add((st, RDF.type, GMEOW.SegmentTransformation))
    g.add((st, GMEOW.transformationTarget, tgt))
    g.add((st, GMEOW.transformationType, GMEOW.transformTransposition))
    g.add((tgt, RDF.type, GMEOW.MusicalSegment))
    g.add((tgt, GMEOW.segmentKind, GMEOW.segmentKindRiff))
    result = run_shacl(g)
    assert not result.ok
    assert result.errors
    assert "exactly one source MusicalSegment" in _error_text(result)


def test_segment_transformation_source_equals_target_fails_shacl() -> None:
    g = Graph()
    g.bind("gmeow", GMEOW)
    g.bind("ex", EX)
    st = EX.retrogradeBad
    seg = EX.palindromeRiff
    g.add((st, RDF.type, GMEOW.SegmentTransformation))
    g.add((st, GMEOW.transformationSource, seg))
    g.add((st, GMEOW.transformationTarget, seg))
    g.add((st, GMEOW.transformationType, GMEOW.transformRetrograde))
    g.add((seg, RDF.type, GMEOW.MusicalSegment))
    g.add((seg, GMEOW.segmentKind, GMEOW.segmentKindRiff))
    result = run_shacl(g)
    assert not result.ok
    assert result.errors
    assert "source and target must be distinct" in _error_text(result)


def test_voice_valid_passes_shacl() -> None:
    g = Graph()
    g.bind("gmeow", GMEOW)
    g.bind("ex", EX)
    voice = EX.voiceValid
    g.add((voice, RDF.type, GMEOW.Voice))
    g.add((voice, GMEOW.voiceTimeFrame, GMEOW.musicalTimeFrameCommon))
    g.add((voice, GMEOW.voiceTuningFrame, GMEOW.tuningSystem12EDO))
    result = run_shacl(g)
    assert result.ok, _error_text(result)
