"""Musical-performance events & participation guards (issue #313).

Principles 4, 5, 6, 9, 11, 12, 15, 16.
"""

from __future__ import annotations

from rdflib import OWL, RDF, RDFS, Graph, Namespace, URIRef

from gmeow_tools.graph import load_merged_graph
from gmeow_tools.validate import ValidationResult, run_shacl

GMEOW = Namespace("https://blackcatinformatics.ca/gmeow/")
GUFO = Namespace("http://purl.org/nemo/gufo#")
EX = Namespace("https://example.org/test-music-performance-events/")

_MERGED_GRAPH: Graph | None = None


def _graph() -> Graph:
    global _MERGED_GRAPH
    if _MERGED_GRAPH is None:
        _MERGED_GRAPH = load_merged_graph(include_imports=False)
    return _MERGED_GRAPH


def _error_text(result: ValidationResult) -> str:
    return "\n".join(result.errors)


def test_performance_events_classes_exist() -> None:
    """#313 classes are declared as owl:Class."""
    graph = _graph()
    for term in (
        "PerformanceParticipation",
        "InstrumentType",
        "InstrumentConfiguration",
        "PlayingTechnique",
    ):
        assert (
            URIRef(GMEOW + term),
            RDF.type,
            OWL.Class,
        ) in graph, f"{term} should be an owl:Class"


def test_performance_participation_is_participation_subkind() -> None:
    """PerformanceParticipation is a gufo:SubKind of core Participation."""
    graph = _graph()
    pp = URIRef(GMEOW + "PerformanceParticipation")
    participation = URIRef(GMEOW + "Participation")
    assert (pp, RDF.type, GUFO.SubKind) in graph
    assert (pp, RDFS.subClassOf, participation) in graph


def test_performance_event_properties_exist() -> None:
    """#313 properties have the intended OWL characteristics.

    participationInstrument is deliberately non-functional: the SHACL shape
    issues a warning for >1 instrument so that deployments can mint multiple
    participations without forcing identity entailments (Principle 12).
    """
    graph = _graph()
    functional = [
        "participationInstrumentItem",
        "participationConfiguration",
        "participationPart",
        "participationTechnique",
    ]
    for prop in functional:
        assert (
            URIRef(GMEOW + prop),
            RDF.type,
            OWL.FunctionalProperty,
        ) in graph, f"{prop} should be functional"

    assert (
        URIRef(GMEOW + "participationInstrument"),
        RDF.type,
        OWL.ObjectProperty,
    ) in graph
    assert (
        URIRef(GMEOW + "participationInstrument"),
        RDF.type,
        OWL.FunctionalProperty,
    ) not in graph, "participationInstrument must not be functional"

    assert (
        URIRef(GMEOW + "performanceOf"),
        RDF.type,
        OWL.ObjectProperty,
    ) in graph
    assert (
        URIRef(GMEOW + "performanceOf"),
        RDF.type,
        OWL.FunctionalProperty,
    ) not in graph, "performanceOf must be non-functional"


def test_event_type_seeds_exist() -> None:
    """Music event-type seeds are present in the events slice."""
    graph = _graph()
    event_types = (
        "eventTypeMusicalPerformance",
        "eventTypeConcert",
        "eventTypeRecordingSession",
        "eventTypeTake",
        "eventTypeOverdub",
        "eventTypeRehearsal",
        "eventTypeJamSession",
        "eventTypeSoundcheck",
        "eventTypeDJSet",
        "eventTypeTransmission",
    )
    for term in event_types:
        assert (
            URIRef(GMEOW + term),
            RDF.type,
            URIRef(GMEOW + "EventType"),
        ) in graph, f"{term} should be an EventType"


def test_participant_role_seeds_exist() -> None:
    """Music participant-role seeds are present in the events slice."""
    graph = _graph()
    roles = (
        "roleSoloist",
        "roleAccompanist",
        "roleEnsembleMember",
        "roleSessionMusician",
        "roleImproviser",
        "roleTransmitter",
        "roleLearner",
    )
    for term in roles:
        assert (
            URIRef(GMEOW + term),
            RDF.type,
            URIRef(GMEOW + "ParticipantRole"),
        ) in graph, f"{term} should be a ParticipantRole"


def test_dual_typed_music_roles() -> None:
    """Performer, conductor and producer are both Contribution and Participant roles."""
    graph = _graph()
    contribution_role = URIRef(GMEOW + "ContributionRole")
    participant_role = URIRef(GMEOW + "ParticipantRole")
    for term in ("rolePerformer", "roleConductor", "roleProducer"):
        iri = URIRef(GMEOW + term)
        assert (iri, RDF.type, contribution_role) in graph, (
            f"{term} should be a ContributionRole"
        )
        assert (iri, RDF.type, participant_role) in graph, (
            f"{term} should be a ParticipantRole"
        )


def test_instrument_type_seeds_exist() -> None:
    """InstrumentType value seeds are present."""
    graph = _graph()
    for term in (
        "instrumentTypePiano",
        "instrumentTypeViolin",
        "instrumentTypeDoubleBass",
        "instrumentTypeDrumKit",
        "instrumentTypeElectricGuitar",
    ):
        assert (
            URIRef(GMEOW + term),
            RDF.type,
            URIRef(GMEOW + "InstrumentType"),
        ) in graph, f"{term} should be an InstrumentType"


def test_playing_technique_seeds_exist() -> None:
    """PlayingTechnique value seeds are present."""
    graph = _graph()
    for term in (
        "playingTechniqueArco",
        "playingTechniquePizzicato",
        "playingTechniquePreparedPiano",
    ):
        assert (
            URIRef(GMEOW + term),
            RDF.type,
            URIRef(GMEOW + "PlayingTechnique"),
        ) in graph, f"{term} should be a PlayingTechnique"


def test_session_fixture_exists() -> None:
    """The session -> takes -> overdub -> composite fixture is present and typed."""
    graph = _graph()
    assert (
        URIRef(GMEOW + "fixtureSessionEvent"),
        RDF.type,
        URIRef(GMEOW + "Event"),
    ) in graph
    for term in (
        "fixtureSessionTake1Event",
        "fixtureSessionTake2Event",
        "fixtureSessionTake3Event",
        "fixtureSessionOverdubEvent",
    ):
        assert (
            URIRef(GMEOW + term),
            RDF.type,
            URIRef(GMEOW + "Event"),
        ) in graph, f"{term} should be an Event"
    for term in (
        "fixtureSessionTake1Recording",
        "fixtureSessionTake2Recording",
        "fixtureSessionTake3Recording",
        "fixtureSessionOverdubRecording",
        "fixtureSessionComposite",
    ):
        assert (
            URIRef(GMEOW + term),
            RDF.type,
            URIRef(GMEOW + "Recording"),
        ) in graph, f"{term} should be a Recording"


def test_who_played_what_on_take_3() -> None:
    """The fixture SPARQL query returns bassist + drummer on take 3."""
    graph = _graph()
    query = """
        PREFIX gmeow: <https://blackcatinformatics.ca/gmeow/>
        SELECT ?participant ?instrument ?technique
        WHERE {
            gmeow:fixtureSessionTake3Event
                gmeow:performanceOf gmeow:fixtureSessionWork .
            ?participation a gmeow:PerformanceParticipation ;
                gmeow:participationEvent gmeow:fixtureSessionTake3Event ;
                gmeow:participationParticipant ?participant ;
                gmeow:participationInstrument ?instrument .
            OPTIONAL {
                ?participation gmeow:participationTechnique ?technique .
            }
        }
        ORDER BY ?participant
    """
    results = list(graph.query(query))
    assert len(results) == 2, "Expected bassist and drummer on take 3"
    participants = {row[0] for row in results}
    assert GMEOW.fixtureSessionBassist in participants
    assert GMEOW.fixtureSessionDrummer in participants
    # Bassist uses pizzicato on double bass.
    bassist_row = next(row for row in results if row[0] == GMEOW.fixtureSessionBassist)
    assert bassist_row[1] == GMEOW.instrumentTypeDoubleBass
    assert bassist_row[2] == GMEOW.playingTechniquePizzicato


def test_performance_participation_valid_passes_shacl() -> None:
    """A complete PerformanceParticipation passes SHACL validation."""
    g = Graph()
    g.bind("gmeow", GMEOW)
    g.bind("ex", EX)
    pp = EX.performanceParticipationValid
    g.add((pp, RDF.type, GMEOW.PerformanceParticipation))
    g.add((pp, GMEOW.participationEvent, EX.concert))
    g.add((pp, GMEOW.participationParticipant, EX.agent))
    g.add((pp, GMEOW.participationRole, GMEOW.roleSoloist))
    g.add((pp, GMEOW.participationInstrument, GMEOW.instrumentTypePiano))
    g.add((pp, GMEOW.participationTechnique, GMEOW.playingTechniqueArco))
    result = run_shacl(g)
    assert result.ok, _error_text(result)


def test_performance_participation_missing_role_fails_shacl() -> None:
    """A PerformanceParticipation without a role violates SHACL."""
    g = Graph()
    g.bind("gmeow", GMEOW)
    g.bind("ex", EX)
    pp = EX.performanceParticipationBad
    g.add((pp, RDF.type, GMEOW.PerformanceParticipation))
    g.add((pp, GMEOW.participationEvent, EX.concert))
    g.add((pp, GMEOW.participationParticipant, EX.agent))
    # participationRole intentionally omitted
    result = run_shacl(g)
    assert not result.ok
    assert result.errors
    assert "exactly one ParticipantRole" in _error_text(result)


def test_performance_participation_two_instruments_warns_shacl() -> None:
    """Two instruments on one participation is allowed but produces a SHACL warning."""
    g = Graph()
    g.bind("gmeow", GMEOW)
    g.bind("ex", EX)
    pp = EX.performanceParticipationBad
    g.add((pp, RDF.type, GMEOW.PerformanceParticipation))
    g.add((pp, GMEOW.participationEvent, EX.concert))
    g.add((pp, GMEOW.participationParticipant, EX.agent))
    g.add((pp, GMEOW.participationRole, GMEOW.roleSoloist))
    g.add((pp, GMEOW.participationInstrument, GMEOW.instrumentTypePiano))
    g.add((pp, GMEOW.participationInstrument, GMEOW.instrumentTypeViolin))
    result = run_shacl(g)
    # The instrument cardinality is a Warning, not a Violation.
    assert result.ok, _error_text(result)
    assert any("At most one instrument" in e for e in result.warnings)
