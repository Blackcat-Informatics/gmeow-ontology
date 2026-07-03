"""Structural guards for the calendar invitation email→event bridge.

Issue #139: Email messages with text/calendar attachments describe events.
"""

from __future__ import annotations

from pathlib import Path

import pytest
from purrdf.compat.rdflib import RDF, Graph, Literal, URIRef
from purrdf.compat.rdflib.namespace import XSD

from gmeow_tools.graph import load_merged_graph

GMEOW = "https://blackcatinformatics.ca/gmeow/"


@pytest.fixture(scope="module")
def ontology_graph() -> Graph:
    """Load the merged ontology graph without imports once per module."""
    return load_merged_graph(include_imports=False)


def _fixture_path() -> str:
    """Return the path to the email coverage fixture."""
    return str(Path(__file__).parent / "fixtures" / "coverage" / "email.ttl")


def test_fixture_calendar_invitation_links_to_event() -> None:
    graph = load_merged_graph(include_imports=False)
    graph.parse(_fixture_path(), format="turtle")
    msg_invite = URIRef("https://example.org/mail/msgCalendarInvite")
    event = URIRef("https://example.org/mail/meetingEvent")
    att = URIRef("https://example.org/mail/calendarAtt")
    method = URIRef(GMEOW + "calendarMethodRequest")
    kind = URIRef(GMEOW + "messageKindCalendarInvitation")
    assert (msg_invite, URIRef(GMEOW + "describesEvent"), event) in graph
    assert (msg_invite, URIRef(GMEOW + "calendarAttachment"), att) in graph
    assert (msg_invite, URIRef(GMEOW + "hasCalendarMethod"), method) in graph
    assert (
        msg_invite,
        URIRef(GMEOW + "calendarUid"),
        Literal("meeting-123@example.org"),
    ) in graph
    assert (msg_invite, URIRef(GMEOW + "hasMessageKind"), kind) in graph
    assert (att, URIRef(GMEOW + "mediaType"), Literal("text/calendar")) in graph
    assert (att, URIRef(GMEOW + "filename"), Literal("meeting.ics")) in graph
    assert (event, RDF.type, URIRef(GMEOW + "Event")) in graph
    assert (
        event,
        URIRef(GMEOW + "eventTime"),
        Literal("2026-06-08T14:00:00Z", datatype=XSD.dateTime),
    ) in graph
