"""Tests for Allen interval relations and JEPD disjointness (issue #67)."""

from __future__ import annotations

from rdflib import Graph, URIRef
from rdflib.namespace import OWL, RDF

from gmeow_tools.graph import load_merged_graph

GMEOW = "https://blackcatinformatics.ca/gmeow/"

_INTERVAL_ALLEN = [
    "intervalBefore",
    "intervalAfter",
    "intervalMeets",
    "intervalMetBy",
    "intervalOverlaps",
    "intervalOverlappedBy",
    "intervalStarts",
    "intervalStartedBy",
    "intervalDuring",
    "intervalContains",
    "intervalFinishes",
    "intervalFinishedBy",
    "intervalCoincidesWith",
]

_EVENT_ALLEN = [
    "before",
    "after",
    "meets",
    "metBy",
    "overlaps",
    "overlappedBy",
    "starts",
    "startedBy",
    "during",
    "contains",
    "finishes",
    "finishedBy",
    "coincidesWith",
]


def _graph() -> Graph:
    return load_merged_graph(include_imports=True)


def test_all_interval_level_allen_relations_exist() -> None:
    g = _graph()
    for rel in _INTERVAL_ALLEN:
        node = URIRef(GMEOW + rel)
        assert (node, RDF.type, OWL.ObjectProperty) in g, f"{rel} must exist"


def test_interval_before_and_after_are_transitive() -> None:
    g = _graph()
    assert (URIRef(GMEOW + "intervalBefore"), RDF.type, OWL.TransitiveProperty) in g
    assert (URIRef(GMEOW + "intervalAfter"), RDF.type, OWL.TransitiveProperty) in g


def test_interval_coincides_with_is_symmetric_and_transitive() -> None:
    g = _graph()
    prop = URIRef(GMEOW + "intervalCoincidesWith")
    assert (prop, RDF.type, OWL.SymmetricProperty) in g
    assert (prop, RDF.type, OWL.TransitiveProperty) in g


def test_no_owl_all_disjoint_properties_over_interval_relations() -> None:
    """OWL 2 DL forbids DisjointObjectProperties over non-simple (transitive)
    properties.  JEPD enforcement lives in SHACL / solver instead."""
    g = _graph()
    for subject in g.subjects(RDF.type, OWL.AllDisjointProperties):
        for list_head in g.objects(subject, OWL.members):
            members = {str(m) for m in g.items(list_head)}
            interval_members = {GMEOW + rel for rel in _INTERVAL_ALLEN}
            overlap = members & interval_members
            assert not overlap, (
                "owl:AllDisjointProperties must not cover interval relations: "
                f"{overlap}"
            )


def test_no_event_interval_property_disjointness_in_owl() -> None:
    """OWL 2 DL forbids DisjointObjectProperties when any property is non-simple
    (transitive).  Cross-family separation is enforced by SHACL / solver."""
    g = _graph()
    for event_rel, interval_rel in zip(_EVENT_ALLEN, _INTERVAL_ALLEN, strict=True):
        event_node = URIRef(GMEOW + event_rel)
        interval_node = URIRef(GMEOW + interval_rel)
        for s, o in [(event_node, interval_node), (interval_node, event_node)]:
            assert (
                s,
                OWL.propertyDisjointWith,
                o,
            ) not in g, f"{s} must NOT be property-disjoint with {o} in OWL"
