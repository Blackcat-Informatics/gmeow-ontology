"""Structural + DL-safety guards for the sexuality building block.

Pins the SexualOrientation / RomanticOrientation facets on the shared
gmeow:IdentityFacet base, the SPLIT-ATTRACTION independence (sexual and romantic
orientation are separate axes with no bridge), the value-vs-subclass decisions
(orientation values are OPEN value vocabularies of individuals), and the absence
of a flat-literal orientation shortcut.
"""

from __future__ import annotations

from rdflib import OWL, RDF, RDFS, Graph, URIRef
from rdflib.query import ResultRow

from gmeow_tools.config import COMPETENCY_DIR
from gmeow_tools.graph import load_merged_graph

GMEOW = "https://blackcatinformatics.ca/gmeow/"


def _graph() -> Graph:
    return load_merged_graph(include_imports=False)


def test_orientation_facets_subclass_identity_facet() -> None:
    graph = _graph()
    for facet in ("SexualOrientation", "RomanticOrientation"):
        assert (
            URIRef(GMEOW + facet),
            RDFS.subClassOf,
            URIRef(GMEOW + "IdentityFacet"),
        ) in graph


def test_split_attraction_axes_are_independent() -> None:
    """Sexual and romantic orientation are SEPARATE axes — no bridge either way."""
    graph = _graph()
    sexual = URIRef(GMEOW + "hasSexualOrientation")
    romantic = URIRef(GMEOW + "hasRomanticOrientation")
    assert (sexual, RDFS.subPropertyOf, romantic) not in graph
    assert (romantic, RDFS.subPropertyOf, sexual) not in graph
    assert (sexual, OWL.equivalentProperty, romantic) not in graph
    # Distinct facet ranges (each points to its own facet class).
    assert (sexual, RDFS.range, URIRef(GMEOW + "SexualOrientation")) in graph
    assert (romantic, RDFS.range, URIRef(GMEOW + "RomanticOrientation")) in graph


def test_orientation_values_are_individuals_not_subclasses() -> None:
    graph = _graph()
    for cls in ("SexualOrientationValue", "RomanticOrientationValue"):
        assert (URIRef(GMEOW + cls), RDFS.subClassOf, URIRef(GMEOW + "Entity")) in graph
    assert (
        URIRef(GMEOW + "orientAsexual"),
        RDF.type,
        URIRef(GMEOW + "SexualOrientationValue"),
    ) in graph
    assert (
        URIRef(GMEOW + "romanticAromantic"),
        RDF.type,
        URIRef(GMEOW + "RomanticOrientationValue"),
    ) in graph
    # No per-orientation subclasses.
    for rejected in ("AsexualPerson", "GayPerson", "BisexualPerson"):
        assert (URIRef(GMEOW + rejected), RDF.type, OWL.Class) not in graph


def test_orientation_value_properties_functional_facets_nonfunctional() -> None:
    graph = _graph()
    for prop in ("sexualOrientationValue", "romanticOrientationValue"):
        node = URIRef(GMEOW + prop)
        assert (node, RDF.type, OWL.FunctionalProperty) in graph
    for prop in ("hasSexualOrientation", "hasRomanticOrientation"):
        node = URIRef(GMEOW + prop)
        assert (node, RDF.type, OWL.FunctionalProperty) not in graph


def test_no_flat_orientation_shortcut() -> None:
    graph = _graph()
    for banned in ("orientation", "sexuality", "orientationLabel"):
        node = URIRef(GMEOW + banned)
        msg = f"{banned} is baggage"
        assert (node, RDF.type, OWL.DatatypeProperty) not in graph, msg


def test_competency_orientation_values_query() -> None:
    graph = _graph()
    query = (COMPETENCY_DIR / "orientation-values.rq").read_text(encoding="utf-8")
    values: set[str] = set()
    for row in graph.query(query):
        assert isinstance(row, ResultRow)
        values.add(str(row[0]))
    for ind in (
        "orientAsexual",
        "orientBisexual",
        "romanticAromantic",
        "romanticBiromantic",
    ):
        assert GMEOW + ind in values
    assert len(values) >= 16
