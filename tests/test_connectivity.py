"""Structural guards for the connectivity building block (#80) — cross-slice retained.

The module-local TBox invariants (Route/RouteKind/Connection/routeKind/routeStart/
routeEnd/routeVia/hasRouteSegment/hasRoute/referenceFrameNetworkGraph) have been
migrated to slices/extensions/connectivity/tests/structural.ttl (#867).

Retained here (cross-slice subjects — NOT defined in connectivity/module.ttl):
  - test_connects_to_universal_spine        gmeow:connectsTo lives in core
  - test_spatially_connects_to_is_...       gmeow:spatiallyConnectsTo lives in core
  - test_genealogy_subproperties_of_...     hasSpouse/hasSibling/hasParent/hasChild
                                            live in the genealogy slice
"""

from __future__ import annotations

from gmeow_rdf.compat.rdflib import OWL, RDF, RDFS, Graph, URIRef

from gmeow_tools.graph import load_merged_graph

GMEOW = "https://blackcatinformatics.ca/gmeow/"


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


def test_genealogy_subproperties_of_connects_to() -> None:
    """hasSpouse, hasSibling, hasParent, hasChild are subproperties of connectsTo."""
    graph = _graph()
    universal = URIRef(GMEOW + "connectsTo")
    for prop in ("hasSpouse", "hasSibling", "hasParent", "hasChild"):
        p = URIRef(GMEOW + prop)
        assert (p, RDFS.subPropertyOf, universal) in graph
