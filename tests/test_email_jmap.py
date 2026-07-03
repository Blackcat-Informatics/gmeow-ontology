"""Structural guards for JMAP structural identifiers: blobId, bodyStructure,
and BodyValue. Issue #140.
"""

from __future__ import annotations

from pathlib import Path

from purrdf.compat.rdflib import RDF, Graph, URIRef

from gmeow_tools.graph import load_merged_graph

GMEOW = "https://blackcatinformatics.ca/gmeow/"


def _graph() -> Graph:
    """Load the merged ontology graph without imports."""
    return load_merged_graph(include_imports=False)


def test_fixture_includes_jmap_identifiers() -> None:
    fixture = Graph()
    fixture.parse(
        Path(__file__).parent / "fixtures" / "coverage" / "email.ttl", format="turtle"
    )
    msg = URIRef("https://example.org/mail/msgMultipart")
    assert (msg, URIRef(GMEOW + "blobId"), None) in fixture
    assert (msg, URIRef(GMEOW + "bodyStructure"), None) in fixture
    assert (msg, URIRef(GMEOW + "hasBodyPart"), None) in fixture
    plain_part = URIRef("https://example.org/mail/plainPart")
    assert (plain_part, URIRef(GMEOW + "partId"), None) in fixture
    assert (plain_part, URIRef(GMEOW + "blobId"), None) in fixture
    body_value = URIRef("https://example.org/mail/plainBodyValue")
    assert (body_value, RDF.type, URIRef(GMEOW + "BodyValue")) in fixture
    assert (body_value, URIRef(GMEOW + "wasDerivedFrom"), plain_part) in fixture
