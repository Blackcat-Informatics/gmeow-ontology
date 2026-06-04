"""Tests for the canonical-term alignments (the superset mechanism)."""

from __future__ import annotations

from rdflib import Graph, URIRef
from rdflib.namespace import OWL

from gmeow_tools.config import ALIGNMENT_TARGETS, LinkPolicy
from gmeow_tools.mappings import build_alignment_graph, load_mappings

GMEOW = "https://blackcatinformatics.ca/gmeow/"


def _graph() -> Graph:
    return build_alignment_graph(load_mappings())


def test_person_unifies_across_vocabularies() -> None:
    graph = _graph()
    person = URIRef(GMEOW + "Person")
    equivalents = {str(o) for o in graph.objects(person, OWL.equivalentClass)}
    assert "http://xmlns.com/foaf/0.1/Person" in equivalents
    assert "https://schema.org/Person" in equivalents
    assert "http://www.w3.org/2000/10/swap/pim/gedcom#Individual" in equivalents


def test_software_project_aligned_to_doap() -> None:
    graph = _graph()
    assert (
        URIRef(GMEOW + "SoftwareProject"),
        OWL.equivalentClass,
        URIRef("http://usefulinc.com/ns/doap#Project"),
    ) in graph


def test_kinship_property_alignment() -> None:
    graph = _graph()
    assert (
        URIRef(GMEOW + "hasParent"),
        OWL.equivalentProperty,
        URIRef("http://purl.org/vocab/relationship/childOf"),
    ) in graph


def test_schema_is_reference_only() -> None:
    # schema.org (CC-BY-SA) may be linked but never imported.
    assert ALIGNMENT_TARGETS["schema"].policy is LinkPolicy.REFERENCE_ONLY


def test_all_mappings_expand() -> None:
    # No CURIE in any mapping row fails to expand (would raise MappingError).
    graph = _graph()
    assert len(graph) >= 80
