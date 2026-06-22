"""Structural guards for the calendar invitation email→event bridge.

Issue #139: Email messages with text/calendar attachments describe events.
"""

from __future__ import annotations

from pathlib import Path

import pytest
from gmeow_rdf.compat.rdflib import OWL, RDF, RDFS, Graph, Literal, URIRef
from gmeow_rdf.compat.rdflib.namespace import XSD

from gmeow_tools.graph import load_merged_graph

GMEOW = "https://blackcatinformatics.ca/gmeow/"


@pytest.fixture(scope="module")
def ontology_graph() -> Graph:
    """Load the merged ontology graph without imports once per module."""
    return load_merged_graph(include_imports=False)


def _fixture_path() -> str:
    """Return the path to the email coverage fixture."""
    return str(Path(__file__).parent / "fixtures" / "coverage" / "email.ttl")


def test_describes_event_is_object_property(ontology_graph: Graph) -> None:
    node = URIRef(GMEOW + "describesEvent")
    assert (node, RDF.type, OWL.ObjectProperty) in ontology_graph
    assert (node, RDFS.domain, URIRef(GMEOW + "EmailMessage")) in ontology_graph
    assert (node, RDFS.range, URIRef(GMEOW + "Event")) in ontology_graph
    # Must NOT be functional — an email may describe multiple events
    assert (node, RDF.type, OWL.FunctionalProperty) not in ontology_graph


def test_event_described_by_is_inverse(ontology_graph: Graph) -> None:
    node = URIRef(GMEOW + "eventDescribedBy")
    assert (node, RDF.type, OWL.ObjectProperty) in ontology_graph
    assert (node, RDFS.domain, URIRef(GMEOW + "Event")) in ontology_graph
    assert (node, RDFS.range, URIRef(GMEOW + "EmailMessage")) in ontology_graph
    assert (node, OWL.inverseOf, URIRef(GMEOW + "describesEvent")) in ontology_graph


def test_calendar_attachment_is_subproperty_of_has_attachment(
    ontology_graph: Graph,
) -> None:
    node = URIRef(GMEOW + "calendarAttachment")
    assert (node, RDF.type, OWL.ObjectProperty) in ontology_graph
    assert (
        node,
        RDFS.subPropertyOf,
        URIRef(GMEOW + "hasAttachment"),
    ) in ontology_graph
    assert (node, RDFS.domain, URIRef(GMEOW + "EmailMessage")) in ontology_graph
    assert (node, RDFS.range, URIRef(GMEOW + "Attachment")) in ontology_graph


def test_calendar_uid_is_datatype_property_on_email_message(
    ontology_graph: Graph,
) -> None:
    node = URIRef(GMEOW + "calendarUid")
    assert (node, RDF.type, OWL.DatatypeProperty) in ontology_graph
    assert (node, RDFS.domain, URIRef(GMEOW + "EmailMessage")) in ontology_graph
    assert (node, RDFS.range, RDFS.Literal) in ontology_graph
    # Must NOT be functional — competing or multiple UIDs may coexist
    assert (node, RDF.type, OWL.FunctionalProperty) not in ontology_graph


def test_calendar_method_class_exists(ontology_graph: Graph) -> None:
    node = URIRef(GMEOW + "CalendarMethod")
    assert (node, RDF.type, OWL.Class) in ontology_graph
    assert (
        node,
        RDFS.subClassOf,
        URIRef("http://purl.org/nemo/gufo#QualityValue"),
    ) in ontology_graph


def test_calendar_method_individuals_exist(ontology_graph: Graph) -> None:
    for method in (
        "calendarMethodRequest",
        "calendarMethodReply",
        "calendarMethodCancel",
        "calendarMethodPublish",
        "calendarMethodCounter",
        "calendarMethodDeclineCounter",
        "calendarMethodAdd",
        "calendarMethodRefresh",
    ):
        node = URIRef(GMEOW + method)
        assert (
            node,
            RDF.type,
            URIRef(GMEOW + "CalendarMethod"),
        ) in ontology_graph, f"{method} missing"


def test_has_calendar_method_property(ontology_graph: Graph) -> None:
    node = URIRef(GMEOW + "hasCalendarMethod")
    assert (node, RDF.type, OWL.ObjectProperty) in ontology_graph
    assert (node, RDFS.domain, URIRef(GMEOW + "EmailMessage")) in ontology_graph
    assert (node, RDFS.range, URIRef(GMEOW + "CalendarMethod")) in ontology_graph
    # Must NOT be functional — a message may carry multiple methods
    assert (node, RDF.type, OWL.FunctionalProperty) not in ontology_graph


def test_message_kind_calendar_invitation_exists(ontology_graph: Graph) -> None:
    node = URIRef(GMEOW + "messageKindCalendarInvitation")
    assert (node, RDF.type, URIRef(GMEOW + "MessageKind")) in ontology_graph


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

    # The attachment has the correct media type and filename
    assert (
        att,
        URIRef(GMEOW + "mediaType"),
        Literal("text/calendar"),
    ) in graph
    assert (
        att,
        URIRef(GMEOW + "filename"),
        Literal("meeting.ics"),
    ) in graph

    # The event reuses the events module spine
    assert (event, RDF.type, URIRef(GMEOW + "Event")) in graph
    assert (
        event,
        URIRef(GMEOW + "eventTime"),
        Literal("2026-06-08T14:00:00Z", datatype=XSD.dateTime),
    ) in graph
