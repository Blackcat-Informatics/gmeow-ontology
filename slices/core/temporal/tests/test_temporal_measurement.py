"""Tests for temporal measurement and dating methods."""

from __future__ import annotations

from gmeow_rdf.compat.rdflib import Graph, URIRef
from gmeow_rdf.compat.rdflib.namespace import OWL, RDF, RDFS, XSD

from gmeow_tools.graph import load_merged_graph

GMEOW = "https://blackcatinformatics.ca/gmeow/"
GUFO = "http://purl.org/nemo/gufo#"
LOGIC = "https://blackcatinformatics.ca/logic/"


def _graph() -> Graph:
    return load_merged_graph(include_imports=True)


def test_temporal_measurement_and_dating_method_exist() -> None:
    g = _graph()
    assert (URIRef(GMEOW + "TemporalMeasurement"), RDF.type, OWL.Class) in g
    assert (URIRef(GMEOW + "DatingMethod"), RDF.type, OWL.Class) in g


def test_seed_dating_methods_exist() -> None:
    g = _graph()
    methods = [
        "datingMethodRadiocarbon",
        "datingMethodDendrochronology",
        "datingMethodThermoluminescence",
        "datingMethodPotassiumArgon",
        "datingMethodUraniumLead",
    ]
    for method in methods:
        assert (URIRef(GMEOW + method), RDF.type, URIRef(GMEOW + "DatingMethod")) in g


def test_measurement_method_is_functional() -> None:
    g = _graph()
    assert (URIRef(GMEOW + "measurementMethod"), RDF.type, OWL.FunctionalProperty) in g


def test_measured_age_range_is_decimal() -> None:
    g = _graph()
    assert (URIRef(GMEOW + "measuredAge"), RDFS.range, XSD.decimal) in g


def test_measurement_determinacy_links_to_determinacy() -> None:
    g = _graph()
    prop = URIRef(GMEOW + "measurementDeterminacy")
    assert (prop, RDFS.range, URIRef(GMEOW + "Determinacy")) in g


def test_temporal_measurement_is_subclass_of_measurement() -> None:
    g = _graph()
    tm = URIRef(GMEOW + "TemporalMeasurement")
    assert (tm, RDFS.subClassOf, URIRef(GMEOW + "Measurement")) in g
    # Measurement is a subclass of Observation, so TM is transitively one.
    # rdflib does not infer transitivity, so we check the path.
    assert (
        URIRef(GMEOW + "Measurement"),
        RDFS.subClassOf,
        URIRef(GMEOW + "Observation"),
    ) in g


def test_temporal_measurement_is_logic_relator() -> None:
    g = _graph()
    assert (
        URIRef(GMEOW + "TemporalMeasurement"),
        RDFS.subClassOf,
        URIRef(LOGIC + "Relator"),
    ) in g


def test_dating_method_is_subclass_of_observation_method() -> None:
    g = _graph()
    assert (
        URIRef(GMEOW + "DatingMethod"),
        RDFS.subClassOf,
        URIRef(GMEOW + "ObservationMethod"),
    ) in g


def test_measured_date_is_not_bridged_to_observation_result() -> None:
    # measuredDate is NOT rdfs:subPropertyOf observationResult because
    # Instant (gufo:AbstractIndividual) is not a subclass of Entity (gufo:Endurant),
    # the range of observationResult.  The alignment is projection-layer only.
    g = _graph()
    assert (
        URIRef(GMEOW + "measuredDate"),
        RDFS.subPropertyOf,
        URIRef(GMEOW + "measuredDate"),
    ) not in g


def test_measurement_method_bridges_to_observation_method() -> None:
    g = _graph()
    assert (
        URIRef(GMEOW + "measurementMethod"),
        RDFS.subPropertyOf,
        URIRef(GMEOW + "observationMethod"),
    ) in g


def test_measurement_determinacy_bridges_to_has_determinacy() -> None:
    g = _graph()
    assert (
        URIRef(GMEOW + "measurementDeterminacy"),
        RDFS.subPropertyOf,
        URIRef(GMEOW + "hasDeterminacy"),
    ) in g


def test_property_chain_period_start_from_measurement() -> None:
    g = _graph()
    chain = g.value(URIRef(GMEOW + "periodStart"), OWL.propertyChainAxiom)
    assert chain is not None
    items = list(g.items(chain))
    assert len(items) == 2
    assert items[0] == URIRef(GMEOW + "hasTemporalMeasurement")
    assert items[1] == URIRef(GMEOW + "measuredDate")


def test_seed_measurements_carry_vantage_and_observed_feature() -> None:
    g = _graph()
    for measurement in (
        "measurementPhanerozoicStart",
        "measurementCenozoicStart",
        "measurementQuaternaryStart",
        "measurementHoloceneStart",
    ):
        m = URIRef(GMEOW + measurement)
        assert (m, RDF.type, URIRef(GMEOW + "TemporalMeasurement")) in g
        assert (m, URIRef(GMEOW + "vantage"), None) in g
        assert (m, URIRef(GMEOW + "observedFeature"), None) in g
        assert (m, URIRef(GMEOW + "observationMethod"), None) in g
