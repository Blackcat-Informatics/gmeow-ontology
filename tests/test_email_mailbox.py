"""Structural guards for mailbox hierarchy and provider-derived state terms.

JMAP mailbox hierarchy (parentMailbox, childMailbox),
sort order, path, and derived message counts.
"""

from __future__ import annotations

from pathlib import Path

from purrdf.compat.rdflib import Graph, Literal, URIRef

from gmeow_tools.graph import load_merged_graph

GMEOW = "https://blackcatinformatics.ca/gmeow/"


def _graph() -> Graph:
    """Load the merged ontology graph without imports."""
    return load_merged_graph(include_imports=False)


def _fixture_path() -> str:
    return str(Path(__file__).parent / "fixtures" / "coverage" / "email.ttl")


def test_fixture_nested_hierarchy() -> None:
    """The coverage fixture shows a three-level mailbox hierarchy."""
    graph = load_merged_graph(include_imports=False)
    graph.parse(_fixture_path(), format="turtle")
    inbox = URIRef("https://example.org/mail/inbox")
    work = URIRef("https://example.org/mail/workFolder")
    projects = URIRef("https://example.org/mail/projectsFolder")
    assert (inbox, URIRef(GMEOW + "childMailbox"), work) in graph
    assert (work, URIRef(GMEOW + "parentMailbox"), inbox) in graph
    assert (work, URIRef(GMEOW + "childMailbox"), projects) in graph
    assert (projects, URIRef(GMEOW + "parentMailbox"), work) in graph


def test_fixture_mailbox_paths() -> None:
    """Derived path strings are present on nested mailboxes."""
    graph = load_merged_graph(include_imports=False)
    graph.parse(_fixture_path(), format="turtle")
    work = URIRef("https://example.org/mail/workFolder")
    projects = URIRef("https://example.org/mail/projectsFolder")
    assert (work, URIRef(GMEOW + "mailboxPath"), Literal("INBOX/Work")) in graph
    assert (
        projects,
        URIRef(GMEOW + "mailboxPath"),
        Literal("INBOX/Work/Projects"),
    ) in graph


def test_fixture_sort_orders() -> None:
    """Sort orders are present on nested mailboxes."""
    graph = load_merged_graph(include_imports=False)
    graph.parse(_fixture_path(), format="turtle")
    work = URIRef("https://example.org/mail/workFolder")
    projects = URIRef("https://example.org/mail/projectsFolder")
    assert (work, URIRef(GMEOW + "mailboxSortOrder"), Literal(1)) in graph
    assert (projects, URIRef(GMEOW + "mailboxSortOrder"), Literal(0)) in graph


def test_fixture_destroyed_mailbox_uses_lifecycle() -> None:
    """A destroyed mailbox uses hasDestructionEvent, not a boolean flag."""
    graph = load_merged_graph(include_imports=False)
    graph.parse(_fixture_path(), format="turtle")
    old_folder = URIRef("https://example.org/mail/oldFolder")
    destruction_event = URIRef("https://example.org/mail/oldFolderDestroyed")
    assert (
        old_folder,
        URIRef(GMEOW + "hasDestructionEvent"),
        destruction_event,
    ) in graph
    assert (old_folder, URIRef(GMEOW + "displayable"), Literal(False)) in graph
    assert not list(graph.objects(old_folder, URIRef(GMEOW + "isDestroyedMailbox")))


def test_fixture_messages_in_nested_mailbox() -> None:
    """Messages reside in the nested projectsFolder."""
    graph = load_merged_graph(include_imports=False)
    graph.parse(_fixture_path(), format="turtle")
    projects = URIRef("https://example.org/mail/projectsFolder")
    msg4 = URIRef("https://example.org/mail/msg4")
    msg5 = URIRef("https://example.org/mail/msg5")
    assert (msg4, URIRef(GMEOW + "residesIn"), projects) in graph
    assert (msg5, URIRef(GMEOW + "residesIn"), projects) in graph
