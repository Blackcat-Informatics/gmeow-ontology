"""Structural guards for threadSubject and subjectPrefix."""

from __future__ import annotations

from pathlib import Path

from purrdf.compat.rdflib import Graph, URIRef

from gmeow_tools.graph import load_merged_graph
import pytest
pytestmark = pytest.mark.maintainer

GMEOW = "https://blackcatinformatics.ca/gmeow/"


def _graph() -> Graph:
    """Load the merged ontology graph without imports."""
    return load_merged_graph(include_imports=False)


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
    assert (thread1, URIRef(GMEOW + "threadSubject"), None) in graph
    assert (msg2, URIRef(GMEOW + "subjectPrefix"), None) in graph
