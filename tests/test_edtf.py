"""Tests for EDTF datatype property and instant (issue #67)."""

from __future__ import annotations

from gmeow_rdf.compat.rdflib import Graph, URIRef
from gmeow_rdf.compat.rdflib.namespace import OWL, RDF, RDFS, XSD

from gmeow_tools.graph import load_merged_graph

GMEOW = "https://blackcatinformatics.ca/gmeow/"


def _graph() -> Graph:
    return load_merged_graph(include_imports=True)


def test_edtf_value_is_datatype_property() -> None:
    g = _graph()
    assert (URIRef(GMEOW + "edtfValue"), RDF.type, OWL.DatatypeProperty) in g


def test_edtf_value_range_is_literal() -> None:
    g = _graph()
    assert (URIRef(GMEOW + "edtfValue"), RDFS.range, RDFS.Literal) in g


def test_instant_exists_and_has_instant_value() -> None:
    g = _graph()
    assert (URIRef(GMEOW + "Instant"), RDF.type, OWL.Class) in g
    assert (URIRef(GMEOW + "instantValue"), RDF.type, OWL.DatatypeProperty) in g
    assert (URIRef(GMEOW + "instantValue"), RDFS.range, XSD.dateTime) in g
    assert (URIRef(GMEOW + "instantValue"), RDFS.domain, URIRef(GMEOW + "Instant")) in g
