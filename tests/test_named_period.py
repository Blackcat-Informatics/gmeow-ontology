"""Tests for named periods (issue #67)."""

from __future__ import annotations

from rdflib import Graph, URIRef
from rdflib.namespace import OWL, RDF, RDFS

from gmeow_tools.graph import load_merged_graph

GMEOW = "https://blackcatinformatics.ca/gmeow/"


def _graph() -> Graph:
    return load_merged_graph(include_imports=True)


def test_named_period_subclasses_entity() -> None:
    g = _graph()
    assert (
        URIRef(GMEOW + "NamedPeriod"),
        RDFS.subClassOf,
        URIRef(GMEOW + "Entity"),
    ) in g


def test_period_type_exists() -> None:
    g = _graph()
    assert (URIRef(GMEOW + "PeriodType"), RDF.type, OWL.Class) in g


def test_seed_geologic_periods_exist() -> None:
    g = _graph()
    periods = (
        "periodPhanerozoic",
        "periodCenozoic",
        "periodQuaternary",
        "periodHolocene",
    )
    for period in periods:
        assert (URIRef(GMEOW + period), RDF.type, URIRef(GMEOW + "NamedPeriod")) in g


def test_period_part_of_is_transitive() -> None:
    g = _graph()
    assert (URIRef(GMEOW + "periodPartOf"), RDF.type, OWL.TransitiveProperty) in g


def test_period_part_of_links() -> None:
    g = _graph()
    assert (
        URIRef(GMEOW + "periodHolocene"),
        URIRef(GMEOW + "periodPartOf"),
        URIRef(GMEOW + "periodQuaternary"),
    ) in g
    assert (
        URIRef(GMEOW + "periodQuaternary"),
        URIRef(GMEOW + "periodPartOf"),
        URIRef(GMEOW + "periodCenozoic"),
    ) in g
    assert (
        URIRef(GMEOW + "periodCenozoic"),
        URIRef(GMEOW + "periodPartOf"),
        URIRef(GMEOW + "periodPhanerozoic"),
    ) in g


def test_period_contains_period_is_inverse() -> None:
    g = _graph()
    prop = URIRef(GMEOW + "periodContainsPeriod")
    assert (prop, OWL.inverseOf, URIRef(GMEOW + "periodPartOf")) in g
