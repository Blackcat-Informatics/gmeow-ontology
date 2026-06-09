"""Structural guards for threadSubject and subjectPrefix. Issue #138."""

from __future__ import annotations

from pathlib import Path

from rdflib import OWL, RDF, RDFS, Graph, URIRef

from gmeow_tools.graph import load_merged_graph

GMEOW = "https://blackcatinformatics.ca/gmeow/"


def _graph() -> Graph:
    """Load the merged ontology graph without imports."""
    return load_merged_graph(include_imports=False)


def test_thread_subject_is_datatype_property_on_thread() -> None:
    """threadSubject must be a DatatypeProperty with domain Thread and range Literal."""
    graph = _graph()
    node = URIRef(GMEOW + "threadSubject")
    assert (node, RDF.type, OWL.DatatypeProperty) in graph
    assert (node, RDFS.domain, URIRef(GMEOW + "Thread")) in graph
    assert (node, RDFS.range, RDFS.Literal) in graph


def test_subject_prefix_is_datatype_property_on_email_message() -> None:
    """subjectPrefix must be a DatatypeProperty with domain EmailMessage and
    range Literal.

    It must NOT be functional because a single message may have multiple nested
    prefixes (e.g. "Re: Fwd: ...").
    """
    graph = _graph()
    node = URIRef(GMEOW + "subjectPrefix")
    assert (node, RDF.type, OWL.DatatypeProperty) in graph
    assert (node, RDFS.domain, URIRef(GMEOW + "EmailMessage")) in graph
    assert (node, RDFS.range, RDFS.Literal) in graph
    assert (node, RDF.type, OWL.FunctionalProperty) not in graph


def _fixture_path() -> str:
    """Return the path to the email coverage fixture."""
    return str(Path(__file__).parent / "fixtures" / "coverage" / "email.ttl")


def test_fixture_has_thread_subject_and_prefix() -> None:
    """The coverage fixture must include a Thread with threadSubject and a
    reply message with subjectPrefix.
    """
    graph = load_merged_graph(include_imports=False)
    graph.parse(_fixture_path(), format="turtle")

    thread1 = URIRef("https://example.org/mail/thread1")
    msg2 = URIRef("https://example.org/mail/msg2")

    assert (
        thread1,
        URIRef(GMEOW + "threadSubject"),
        None,
    ) in graph
    assert (
        msg2,
        URIRef(GMEOW + "subjectPrefix"),
        None,
    ) in graph
