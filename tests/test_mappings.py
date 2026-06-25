"""Tests for SSSOM mapping loading, expansion, and linkset generation."""

from __future__ import annotations

import pytest
from gmeow_rdf.compat.rdflib import RDF, URIRef
from gmeow_rdf.compat.rdflib.namespace import VOID

from gmeow_tools.mappings import (
    MappingError,
    build_alignment_graph,
    build_linksets,
    expand_curie,
    load_mappings,
)


def test_expand_curie() -> None:
    assert str(expand_curie("foaf:Person")) == "http://xmlns.com/foaf/0.1/Person"
    assert str(expand_curie("gmeow:Person")).endswith("/gmeow/Person")


def test_expand_curie_rejects_unknown_prefix() -> None:
    with pytest.raises(MappingError):
        expand_curie("nope:Thing")
    with pytest.raises(MappingError):
        expand_curie("notacurie")


def test_expand_curie_returns_absolute_iris_unchanged() -> None:
    assert expand_curie("urn:uuid:123e4567-e89b-12d3-a456-426614174000") == URIRef(
        "urn:uuid:123e4567-e89b-12d3-a456-426614174000"
    )
    assert expand_curie("file:///tmp/foo.ttl") == URIRef("file:///tmp/foo.ttl")
    assert expand_curie("http://example.org/Thing") == URIRef(
        "http://example.org/Thing"
    )
    assert expand_curie("https://example.org/Thing") == URIRef(
        "https://example.org/Thing"
    )


def test_load_seed_mappings() -> None:
    mappings = load_mappings()
    assert mappings, "seed SSSOM files should yield mappings"
    subjects = {m.subject_id for m in mappings}
    assert "gmeow:Person" in subjects


def test_alignment_graph_has_equivalence() -> None:
    graph = build_alignment_graph(load_mappings())
    gmeow_person = URIRef("https://blackcatinformatics.ca/gmeow/Person")
    foaf_person = URIRef("http://xmlns.com/foaf/0.1/Person")
    owl_eq = URIRef("http://www.w3.org/2002/07/owl#equivalentClass")
    assert (gmeow_person, owl_eq, foaf_person) in graph


def test_linksets_are_grouped() -> None:
    linksets = build_linksets(load_mappings())
    nodes = set(linksets.subjects(RDF.type, VOID.Linkset))
    # One linkset per (target namespace, predicate) pair; the entities slice
    # aligns to many surface vocabularies, so expect a substantial number.
    assert len(nodes) >= 10
    for node in nodes:
        assert (node, VOID.linkPredicate, None) in linksets
        assert (node, VOID.objectsTarget, None) in linksets
