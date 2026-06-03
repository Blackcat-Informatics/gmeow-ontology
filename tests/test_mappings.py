"""Tests for SSSOM mapping loading, expansion, and linkset generation."""

from __future__ import annotations

import pytest
from rdflib import RDF, URIRef
from rdflib.namespace import VOID

from gmeow_tools.mappings import (
    MappingError,
    build_alignment_graph,
    build_linksets,
    collect_wikidata_ids,
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
    # 5 distinct schema (target,predicate) pairs + 1 Wikidata pair = 6.
    assert len(nodes) == 6
    for node in nodes:
        assert (node, VOID.linkPredicate, None) in linksets
        assert (node, VOID.objectsTarget, None) in linksets


def test_collect_wikidata_ids() -> None:
    ids = collect_wikidata_ids(load_mappings())
    assert "Q5" in ids
    assert "Q43229" in ids
