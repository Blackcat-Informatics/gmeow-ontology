"""Structural + DL-safety guards for the connectivity building block (#80).

Pins the universal connectsTo spine, the spatiallyConnectsTo subproperty,
the Route / Connection / RouteKind module, and the genealogy subproperty
declarations.
"""

from __future__ import annotations

from gmeow_rdf.compat.rdflib import OWL, RDF, RDFS, Graph, URIRef

from gmeow_tools.graph import load_merged_graph

GMEOW = "https://blackcatinformatics.ca/gmeow/"
GUFO = "http://purl.org/nemo/gufo#"


def _graph() -> Graph:
    return load_merged_graph(include_imports=False)


def test_connects_to_universal_spine() -> None:
    """connectsTo is in core, has no domain/range, is NOT symmetric or transitive."""
    graph = _graph()
    node = URIRef(GMEOW + "connectsTo")
    assert (node, RDF.type, OWL.ObjectProperty) in graph
    # No domain/range asserted on the universal spine.
    assert not list(graph.objects(node, RDFS.domain))
    assert not list(graph.objects(node, RDFS.range))
    # NOT symmetric — subproperties decide their own directionality.
    assert (node, RDF.type, OWL.SymmetricProperty) not in graph
    # NOT transitive — reachability is computed by the solver (P12).
    assert (node, RDF.type, OWL.TransitiveProperty) not in graph


def test_spatially_connects_to_is_symmetric_subproperty() -> None:
    """spatiallyConnectsTo ⊑ connectsTo, symmetric, domain/range Location."""
    graph = _graph()
    node = URIRef(GMEOW + "spatiallyConnectsTo")
    assert (node, RDF.type, OWL.ObjectProperty) in graph
    assert (node, RDF.type, OWL.SymmetricProperty) in graph
    assert (
        node,
        RDFS.subPropertyOf,
        URIRef(GMEOW + "connectsTo"),
    ) in graph
    assert (node, RDFS.domain, URIRef(GMEOW + "Location")) in graph
    assert (node, RDFS.range, URIRef(GMEOW + "Location")) in graph


def test_route_class_and_kinds() -> None:
    """Route, RouteKind value vocabulary, routeKind functional."""
    graph = _graph()
    # Route is a gufo:Kind subclass of Entity
    route = URIRef(GMEOW + "Route")
    assert (route, RDF.type, OWL.Class) in graph
    assert (route, RDFS.subClassOf, URIRef(GMEOW + "Entity")) in graph

    # RouteKind is a value vocabulary (individuals, not subclasses)
    route_kind = URIRef(GMEOW + "RouteKind")
    assert (route_kind, RDF.type, OWL.Class) in graph
    assert (route_kind, RDFS.subClassOf, URIRef(GUFO + "QualityValue")) in graph

    for ind in (
        "routeKindTransit",
        "routeKindWalking",
        "routeKindFlight",
        "routeKindNetwork",
        "routeKindCitation",
        "routeKindSocial",
        "routeKindDependency",
    ):
        assert (URIRef(GMEOW + ind), RDF.type, route_kind) in graph

    # routeKind is functional
    rk = URIRef(GMEOW + "routeKind")
    assert (rk, RDF.type, OWL.ObjectProperty) in graph
    assert (rk, RDF.type, OWL.FunctionalProperty) in graph
    assert (rk, RDFS.domain, route) in graph
    assert (rk, RDFS.range, route_kind) in graph


def test_connection_relator() -> None:
    """Connection: gufo Relator with functional source/target."""
    graph = _graph()
    conn = URIRef(GMEOW + "Connection")
    assert (conn, RDF.type, OWL.Class) in graph
    assert (conn, RDFS.subClassOf, URIRef(GUFO + "Relator")) in graph

    for prop in ("connectionSource", "connectionTarget"):
        p = URIRef(GMEOW + prop)
        assert (p, RDF.type, OWL.ObjectProperty) in graph
        assert (p, RDF.type, OWL.FunctionalProperty) in graph
        assert (p, RDFS.domain, conn) in graph
        # Range is owl:Thing (universal connectivity contract).
        assert (p, RDFS.range, OWL.Thing) in graph


def test_route_properties() -> None:
    """routeStart/End functional; routeVia not; hasRouteSegment ⊑ hasPart."""
    graph = _graph()
    route = URIRef(GMEOW + "Route")

    # Functional properties
    for prop in ("routeStart", "routeEnd"):
        p = URIRef(GMEOW + prop)
        assert (p, RDF.type, OWL.ObjectProperty) in graph
        assert (p, RDF.type, OWL.FunctionalProperty) in graph
        assert (p, RDFS.domain, route) in graph
        # Range is owl:Thing (universal connectivity contract).
        assert (p, RDFS.range, OWL.Thing) in graph

    # routeVia is NOT functional (multiple via points possible)
    via = URIRef(GMEOW + "routeVia")
    assert (via, RDF.type, OWL.ObjectProperty) in graph
    assert (via, RDF.type, OWL.FunctionalProperty) not in graph
    assert (via, RDFS.domain, route) in graph
    # Range is owl:Thing (universal connectivity contract).
    assert (via, RDFS.range, OWL.Thing) in graph

    # hasRouteSegment ⊑ hasPart, transitive
    seg = URIRef(GMEOW + "hasRouteSegment")
    assert (seg, RDF.type, OWL.ObjectProperty) in graph
    assert (seg, RDF.type, OWL.TransitiveProperty) in graph
    assert (
        seg,
        RDFS.subPropertyOf,
        URIRef(GMEOW + "hasPart"),
    ) in graph
    assert (seg, RDFS.domain, route) in graph
    assert (seg, RDFS.range, route) in graph

    # hasRoute links things to routes
    has_route = URIRef(GMEOW + "hasRoute")
    assert (has_route, RDF.type, OWL.ObjectProperty) in graph
    # Domain is owl:Thing (universal connectivity contract).
    assert (has_route, RDFS.domain, OWL.Thing) in graph
    assert (has_route, RDFS.range, route) in graph


def test_genealogy_subproperties_of_connects_to() -> None:
    """hasSpouse, hasSibling, hasParent, hasChild are subproperties of connectsTo."""
    graph = _graph()
    universal = URIRef(GMEOW + "connectsTo")
    for prop in ("hasSpouse", "hasSibling", "hasParent", "hasChild"):
        p = URIRef(GMEOW + prop)
        assert (p, RDFS.subPropertyOf, universal) in graph


def test_reference_frame_network_graph() -> None:
    """referenceFrameNetworkGraph declares metricGraphHops."""
    graph = _graph()
    frame = URIRef(GMEOW + "referenceFrameNetworkGraph")
    assert (frame, RDF.type, URIRef(GMEOW + "ReferenceFrame")) in graph
    assert (
        frame,
        URIRef(GMEOW + "hasMetricKind"),
        URIRef(GMEOW + "metricGraphHops"),
    ) in graph
