"""Structural + DL-safety guards for the spatial aggregation module (#101).

Pins the value-vs-subclass decisions (AggregationFunction is a value vocabulary;
SpatialAggregation is a Measurement specialisation; SpatialBin is a Place
specialisation), the non-functional hasBin property, and the co-equal
observation-result pattern inherited from the universal observation stack.
"""

from __future__ import annotations

from gmeow_rdf.compat.rdflib import OWL, RDF, RDFS, Graph, URIRef
from gmeow_rdf.compat.rdflib.namespace import XSD

from gmeow_tools.graph import load_merged_graph

GMEOW = "https://blackcatinformatics.ca/gmeow/"
GUFO = "http://purl.org/nemo/gufo#"
QB = "http://purl.org/linked-data/cube#"
GEO = "http://www.opengis.net/ont/geosparql#"


def _graph() -> Graph:
    return load_merged_graph(include_imports=False)


def test_spatial_aggregation_is_measurement() -> None:
    graph = _graph()
    assert (
        URIRef(GMEOW + "SpatialAggregation"),
        RDFS.subClassOf,
        URIRef(GMEOW + "Measurement"),
    ) in graph


def test_spatial_bin_is_place() -> None:
    graph = _graph()
    assert (
        URIRef(GMEOW + "SpatialBin"),
        RDFS.subClassOf,
        URIRef(GMEOW + "Place"),
    ) in graph


def test_aggregation_function_is_value_not_subclass() -> None:
    graph = _graph()
    assert (
        URIRef(GMEOW + "AggregationFunction"),
        RDFS.subClassOf,
        URIRef(GUFO + "QualityValue"),
    ) in graph
    for ind in (
        "aggCount",
        "aggSum",
        "aggAverage",
        "aggDensity",
        "aggCentroid",
        "aggMinimum",
        "aggMaximum",
    ):
        assert (
            URIRef(GMEOW + ind),
            RDF.type,
            URIRef(GMEOW + "AggregationFunction"),
        ) in graph
    # No per-function subclasses must exist.
    for rejected in ("CountAggregation", "DensityAggregation", "CentroidAggregation"):
        assert (URIRef(GMEOW + rejected), RDF.type, OWL.Class) not in graph


def test_aggregation_function_property_is_functional() -> None:
    graph = _graph()
    prop = URIRef(GMEOW + "aggregationFunction")
    assert (prop, RDF.type, OWL.ObjectProperty) in graph
    assert (prop, RDF.type, OWL.FunctionalProperty) in graph


def test_has_bin_is_non_functional() -> None:
    graph = _graph()
    prop = URIRef(GMEOW + "hasBin")
    assert (prop, RDF.type, OWL.ObjectProperty) in graph
    assert (prop, RDF.type, OWL.FunctionalProperty) not in graph


def test_contains_place_exists_and_is_inverse() -> None:
    graph = _graph()
    prop = URIRef(GMEOW + "containsPlace")
    assert (prop, RDF.type, OWL.ObjectProperty) in graph
    assert (
        prop,
        OWL.inverseOf,
        URIRef(GMEOW + "containedInPlace"),
    ) in graph


def test_minimum_population_is_datatype() -> None:
    graph = _graph()
    prop = URIRef(GMEOW + "minimumPopulation")
    assert (prop, RDF.type, OWL.DatatypeProperty) in graph
    assert (prop, RDF.type, OWL.FunctionalProperty) in graph
    assert (prop, RDFS.domain, URIRef(GMEOW + "SpatialAggregation")) in graph
    assert (prop, RDFS.range, XSD.nonNegativeInteger) in graph


def test_no_unsafe_complex_property_chains() -> None:
    # Principle 12: aggregation computation stays in the solver layer.
    graph = _graph()
    # No property chains asserted on aggregation-specific properties.
    for prop in ("aggregationFunction", "hasBin"):
        p = URIRef(GMEOW + prop)
        for _, _, _o in graph.triples((p, OWL.propertyChainAxiom, None)):
            raise AssertionError(f"{prop} must not carry a property chain axiom")
