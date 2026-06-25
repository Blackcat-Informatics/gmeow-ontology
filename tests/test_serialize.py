"""Tests for multi-format serialization and the JSON-LD context."""

from __future__ import annotations

from pathlib import Path

from gmeow_rdf.compat.rdflib import RDF, Graph, URIRef
from gmeow_rdf.compat.rdflib.compare import isomorphic
from gmeow_rdf.compat.rdflib.namespace import OWL

from gmeow_tools.config import NAMESPACE
from gmeow_tools.jsonld_context import build_context
from gmeow_tools.serialize import serialize_graph


def _sample_graph() -> Graph:
    graph = Graph()
    graph.add((URIRef(NAMESPACE + "Person"), RDF.type, OWL.Class))
    return graph


def test_serialize_all_formats(tmp_path: Path) -> None:
    written = serialize_graph(_sample_graph(), stem="gmeow", dist_dir=tmp_path)
    assert set(written) == {"ttl", "rdf", "nt", "jsonld", "yamlld"}
    for path in written.values():
        assert path.exists() and path.stat().st_size > 0


def test_serializations_round_trip(tmp_path: Path) -> None:
    original = _sample_graph()
    written = serialize_graph(original, stem="gmeow", dist_dir=tmp_path)
    readers = {"ttl": "turtle", "rdf": "xml", "nt": "nt", "jsonld": "json-ld"}
    for ext, reader in readers.items():
        reparsed = Graph().parse(written[ext], format=reader)
        assert isomorphic(original, reparsed), f"{ext} round-trip is not isomorphic"


def test_jsonld_context_structure() -> None:
    context = build_context()["@context"]
    assert context["@vocab"] == NAMESPACE
    assert context["gmeow"] == NAMESPACE
    assert "foaf" in context and "wd" in context
