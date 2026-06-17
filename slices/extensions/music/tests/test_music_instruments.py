"""Instrument-type, configuration, and playing-technique guards.

Principles 4, 5, 8, 9, 11, 16.
"""

from __future__ import annotations

from rdflib import OWL, RDF, RDFS, SKOS, Graph, Namespace, URIRef
from tests._graph_nt import run_shacl

from gmeow_tools.graph import load_merged_graph
from gmeow_tools.validate import ValidationResult

GMEOW = Namespace("https://blackcatinformatics.ca/gmeow/")
GUFO = Namespace("http://purl.org/nemo/gufo#")
EX = Namespace("https://example.org/test-music-instruments/")

_MERGED_GRAPH: Graph | None = None


def _graph() -> Graph:
    global _MERGED_GRAPH
    if _MERGED_GRAPH is None:
        _MERGED_GRAPH = load_merged_graph(include_imports=False)
    return _MERGED_GRAPH


def _error_text(result: ValidationResult) -> str:
    return "\n".join(result.errors)


def test_instrument_classes_exist() -> None:
    """Instrument classes use owl:Class plus the intended gUFO stereotype."""
    graph = _graph()
    assert (
        URIRef(GMEOW + "InstrumentConfiguration"),
        RDF.type,
        OWL.Class,
    ) in graph
    assert (
        URIRef(GMEOW + "InstrumentConfiguration"),
        RDFS.subClassOf,
        GUFO.Relator,
    ) in graph, "InstrumentConfiguration must be a gufo:Relator"
    assert (
        URIRef(GMEOW + "InstrumentModification"),
        RDF.type,
        OWL.Class,
    ) in graph
    assert (
        URIRef(GMEOW + "InstrumentModification"),
        RDFS.subClassOf,
        GUFO.QualityValue,
    ) in graph, "InstrumentModification must be a value vocabulary"


def test_configuration_properties_exist() -> None:
    """Configuration properties are functional per relator where required."""
    graph = _graph()
    functional = [
        "configurationOf",
        "configurationInstrumentType",
        "configurationTuningFrame",
        "configurationInterval",
    ]
    for prop in functional:
        assert (
            URIRef(GMEOW + prop),
            RDF.type,
            OWL.FunctionalProperty,
        ) in graph, f"{prop} should be functional"

    # Modification is deliberately non-functional to allow compound modifications.
    assert (
        URIRef(GMEOW + "configurationModification"),
        RDF.type,
        OWL.FunctionalProperty,
    ) not in graph, "configurationModification must not be functional"


def test_participation_instrument_item_ranges_over_entity() -> None:
    """participationInstrumentItem ranges over Entity, not the CreativeWork Item."""
    graph = _graph()
    prop = URIRef(GMEOW + "participationInstrumentItem")
    assert (prop, RDFS.range, GMEOW.Entity) in graph
    assert (prop, RDFS.range, GMEOW.Item) not in graph


def test_instrument_type_seeds_exist() -> None:
    """All InstrumentType seeds are present and typed."""
    graph = _graph()
    for term in (
        "instrumentTypePiano",
        "instrumentTypeViolin",
        "instrumentTypeDoubleBass",
        "instrumentTypeDrumKit",
        "instrumentTypeElectricGuitar",
        "instrumentTypeVoice",
        "instrumentTypeSitar",
        "instrumentTypeTabla",
        "instrumentTypeModularSynth",
        "instrumentTypeTurntables",
        "instrumentTypeAdaptedGuitar",
        "instrumentTypeGamelan",
    ):
        assert (
            URIRef(GMEOW + term),
            RDF.type,
            URIRef(GMEOW + "InstrumentType"),
        ) in graph, f"{term} should be an InstrumentType"


def test_instrument_type_hs_numbers() -> None:
    """Selected InstrumentType seeds carry Hornbostel-Sachs numbers."""
    graph = _graph()
    expected = {
        "instrumentTypePiano": "314.122-4-8",
        "instrumentTypeElectricGuitar": "321.322-6",
    }
    for term, hs in expected.items():
        values = list(graph.objects(URIRef(GMEOW + term), GMEOW.hsNumber))
        assert any(str(v) == hs for v in values), f"{term} should have hsNumber {hs}"


def test_instrument_type_mimo_matches() -> None:
    """InstrumentType seeds with MIMO entries carry skos:exactMatch."""
    graph = _graph()
    mimo = Namespace("http://www.mimo-db.eu/InstrumentsKeywords/")
    matches = {
        "instrumentTypePiano": mimo["2299"],
        "instrumentTypeElectricGuitar": mimo["3236"],
        "instrumentTypeSitar": mimo["3456"],
        "instrumentTypeTabla": mimo["2899"],
        "instrumentTypeGamelan": mimo["2805"],
    }
    for term, expected in matches.items():
        assert (
            URIRef(GMEOW + term),
            SKOS.exactMatch,
            expected,
        ) in graph, f"{term} should exactMatch {expected}"


def test_instrument_modification_seeds_exist() -> None:
    """All InstrumentModification seeds are present."""
    graph = _graph()
    for term in (
        "instrumentModificationPrepared",
        "instrumentModificationScordatura",
        "instrumentModificationCapo",
        "instrumentModificationMute",
        "instrumentModificationElectrified",
        "instrumentModificationExtendedRange",
    ):
        assert (
            URIRef(GMEOW + term),
            RDF.type,
            URIRef(GMEOW + "InstrumentModification"),
        ) in graph, f"{term} should be an InstrumentModification"


def test_playing_technique_seeds_exist() -> None:
    """Expanded PlayingTechnique seeds are present."""
    graph = _graph()
    for term in (
        "playingTechniqueArco",
        "playingTechniquePizzicato",
        "playingTechniqueColLegno",
        "playingTechniquePreparedPiano",
        "playingTechniqueMultiphonics",
        "playingTechniqueTapping",
        "playingTechniqueSlap",
        "playingTechniqueGrowl",
        "playingTechniqueKonnakol",
        "playingTechniqueBentNote",
        "playingTechniqueHarmonics",
    ):
        assert (
            URIRef(GMEOW + term),
            RDF.type,
            URIRef(GMEOW + "PlayingTechnique"),
        ) in graph, f"{term} should be a PlayingTechnique"


def test_configuration_fixtures_exist() -> None:
    """The prepared-piano, drop-D, and item-level Les Paul configuration
    fixtures are present."""
    graph = _graph()
    prepared = URIRef(GMEOW + "fixturePreparedPianoConfiguration")
    dropd = URIRef(GMEOW + "fixtureDropDGuitarConfiguration")
    les_paul = URIRef(GMEOW + "fixture1959LesPaulConfiguration")
    assert (prepared, RDF.type, GMEOW.InstrumentConfiguration) in graph
    assert (dropd, RDF.type, GMEOW.InstrumentConfiguration) in graph
    assert (les_paul, RDF.type, GMEOW.InstrumentConfiguration) in graph
    assert (
        prepared,
        GMEOW.configurationModification,
        GMEOW.instrumentModificationPrepared,
    ) in graph
    assert (
        dropd,
        GMEOW.configurationModification,
        GMEOW.instrumentModificationScordatura,
    ) in graph
    assert (
        dropd,
        GMEOW.configurationInterval,
        GMEOW.pitchIntervalMajorSecondDown,
    ) in graph
    assert (
        les_paul,
        GMEOW.configurationOf,
        URIRef(GMEOW + "fixture1959LesPaul"),
    ) in graph


def test_instrument_configuration_valid_with_type_passes_shacl() -> None:
    """A type-level InstrumentConfiguration passes SHACL validation."""
    g = Graph()
    g.bind("gmeow", GMEOW)
    g.bind("ex", EX)
    config = EX.configValid
    g.add((config, RDF.type, GMEOW.InstrumentConfiguration))
    g.add((config, GMEOW.configurationInstrumentType, GMEOW.instrumentTypePiano))
    g.add(
        (config, GMEOW.configurationModification, GMEOW.instrumentModificationPrepared)
    )
    g.add((config, GMEOW.configurationTuningFrame, GMEOW.tuningSystem12EDO))
    result = run_shacl(g)
    assert result.ok, _error_text(result)


def test_instrument_configuration_valid_with_item_passes_shacl() -> None:
    """An item-level InstrumentConfiguration passes SHACL validation."""
    g = Graph()
    g.bind("gmeow", GMEOW)
    g.bind("ex", EX)
    config = EX.configValidItem
    g.add((config, RDF.type, GMEOW.InstrumentConfiguration))
    g.add((config, GMEOW.configurationOf, EX.myInstrument))
    g.add((config, GMEOW.configurationModification, GMEOW.instrumentModificationCapo))
    g.add((config, GMEOW.configurationTuningFrame, GMEOW.tuningSystem12EDO))
    g.add((config, GMEOW.configurationInterval, GMEOW.pitchIntervalMajorSecondDown))
    result = run_shacl(g)
    assert result.ok, _error_text(result)


def test_instrument_configuration_missing_target_fails_shacl() -> None:
    """An InstrumentConfiguration without configurationOf or
    configurationInstrumentType fails SHACL."""
    g = Graph()
    g.bind("gmeow", GMEOW)
    g.bind("ex", EX)
    config = EX.configBadTarget
    g.add((config, RDF.type, GMEOW.InstrumentConfiguration))
    g.add(
        (config, GMEOW.configurationModification, GMEOW.instrumentModificationPrepared)
    )
    result = run_shacl(g)
    assert not result.ok
    assert result.errors
    assert "exactly one of: a specific instrument item" in _error_text(result)


def test_instrument_configuration_two_intervals_fails_shacl() -> None:
    """Two intervals on one InstrumentConfiguration violates the functional
    interval shape."""
    g = Graph()
    g.bind("gmeow", GMEOW)
    g.bind("ex", EX)
    config = EX.configBadInterval
    g.add((config, RDF.type, GMEOW.InstrumentConfiguration))
    g.add(
        (config, GMEOW.configurationInstrumentType, GMEOW.instrumentTypeElectricGuitar)
    )
    g.add(
        (
            config,
            GMEOW.configurationModification,
            GMEOW.instrumentModificationScordatura,
        )
    )
    g.add((config, GMEOW.configurationInterval, GMEOW.pitchIntervalMajorSecondDown))
    g.add((config, GMEOW.configurationInterval, GMEOW.pitchIntervalPerfectFifth))
    result = run_shacl(g)
    assert not result.ok
    assert result.errors
    assert "At most one interval" in _error_text(result)


def test_instrument_configuration_compound_modification_passes_shacl() -> None:
    """Two modifications on one InstrumentConfiguration is allowed."""
    g = Graph()
    g.bind("gmeow", GMEOW)
    g.bind("ex", EX)
    config = EX.configCompound
    g.add((config, RDF.type, GMEOW.InstrumentConfiguration))
    g.add(
        (config, GMEOW.configurationInstrumentType, GMEOW.instrumentTypeElectricGuitar)
    )
    g.add((config, GMEOW.configurationModification, GMEOW.instrumentModificationMute))
    g.add(
        (
            config,
            GMEOW.configurationModification,
            GMEOW.instrumentModificationElectrified,
        )
    )
    g.add((config, GMEOW.configurationTuningFrame, GMEOW.tuningSystem12EDO))
    result = run_shacl(g)
    assert result.ok, _error_text(result)
