"""Tests for temporal measurement and dating methods (issue #67)."""

from __future__ import annotations

from rdflib import Graph, URIRef
from rdflib.namespace import OWL, RDF, RDFS, XSD

from gmeow_tools.graph import load_merged_graph

GMEOW = "https://blackcatinformatics.ca/gmeow/"


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
