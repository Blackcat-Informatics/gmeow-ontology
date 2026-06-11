"""Structural guards for JMAP structural identifiers: blobId, bodyStructure,
and BodyValue. Issue #140.
"""

from __future__ import annotations

from pathlib import Path

from rdflib import OWL, RDF, RDFS, Graph, URIRef

from gmeow_tools.graph import load_merged_graph

GMEOW = "https://blackcatinformatics.ca/gmeow/"


def _graph() -> Graph:
    """Load the merged ontology graph without imports."""
    return load_merged_graph(include_imports=False)


def test_blob_id_is_datatype_property() -> None:
    graph = _graph()
    node = URIRef(GMEOW + "blobId")
    assert (node, RDF.type, OWL.DatatypeProperty) in graph
    assert (node, RDFS.range, RDFS.Literal) in graph
    # Domain-free so it can be used on EmailMessage and BodyPart.
    assert (node, RDFS.domain, URIRef(GMEOW + "EmailMessage")) not in graph
    assert (node, RDFS.domain, URIRef(GMEOW + "BodyPart")) not in graph


def test_body_structure_is_functional_datatype_property_on_email_message() -> None:
    graph = _graph()
    node = URIRef(GMEOW + "bodyStructure")
    assert (node, RDF.type, OWL.DatatypeProperty) in graph
    assert (node, RDF.type, OWL.FunctionalProperty) in graph
    assert (node, RDFS.domain, URIRef(GMEOW + "EmailMessage")) in graph
    assert (node, RDFS.range, RDFS.Literal) in graph


def test_body_value_class_exists() -> None:
    graph = _graph()
    node = URIRef(GMEOW + "BodyValue")
    assert (node, RDF.type, OWL.Class) in graph
    assert (
        node,
        RDFS.subClassOf,
        URIRef(GMEOW + "InformationObject"),
    ) in graph


def test_no_competing_body_structure_class_or_property() -> None:
    """The canonical MIME tree is the existing hasBodyPart/hasPart spine."""
    graph = _graph()
    assert (URIRef(GMEOW + "BodyStructure"), RDF.type, OWL.Class) not in graph
    assert (
        URIRef(GMEOW + "hasBodyStructure"),
        RDF.type,
        OWL.ObjectProperty,
    ) not in graph


def test_no_competing_body_value_property() -> None:
    """Decoded content is linked by wasDerivedFrom, not a dedicated property."""
    graph = _graph()
    assert (
        URIRef(GMEOW + "hasBodyValue"),
        RDF.type,
        OWL.ObjectProperty,
    ) not in graph


def test_fixture_includes_jmap_identifiers() -> None:
    fixture = Graph()
    fixture.parse(
        Path(__file__).parent / "fixtures" / "coverage" / "email.ttl",
        format="turtle",
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
