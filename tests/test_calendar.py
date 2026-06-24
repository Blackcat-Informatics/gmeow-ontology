"""The calendar and scheduling slice (#62) — RETAINED pytest guards.

Builds on the events module (#41) and temporal module, adding recurrence,
availability, RSVP, reminders, tasks, and time zones. Reuses Event, Participation,
EventSeries/RecurrenceRule, TimeInterval, and the four clocks.

The bulk of the TBox structural invariants have been migrated to the declarative
slicetest DSL in slices/core/calendar/tests/structural.ttl (cells 1-50).

RETAINED here (not expressible as module-scoped SPARQL ASK cells):
  * test_calendar_temporal_datatypes_are_datetime_or_duration — complex
    blank-node union + cardinality check on gmeow:reminderTrigger
    (len(range_nodes)==1, owl:unionOf list walk).
  * test_calendar_axes_are_independent — itertools.combinations sweep over
    10 orthogonal properties (45 pairs x 4 assertions = 180 checks);
    converting to a finite blacklist would silently narrow coverage.
  * test_organizer_and_attendee_roles_exist — gmeow:roleOrganizer and
    gmeow:roleAttendee are defined in slices/core/event/module.ttl (cross-
    slice subjects); a scopeModule cell would silently miss them.
"""

from __future__ import annotations

from itertools import combinations

from gmeow_rdf.compat.rdflib import OWL, RDF, RDFS, XSD, Graph, Namespace, URIRef

from gmeow_tools.graph import load_merged_graph

GMEOW = "https://blackcatinformatics.ca/gmeow/"
GM = Namespace(GMEOW)


def _graph() -> Graph:
    return load_merged_graph(include_imports=False)


# --------------------------------------------------------------------------- #
# DL-clean datatypes (Principle 3)
# Retained: complex blank-node union + cardinality check on reminderTrigger.
# --------------------------------------------------------------------------- #


def test_calendar_temporal_datatypes_are_datetime_or_duration() -> None:
    g = _graph()
    for prop, rng in (
        (GM.exceptionOriginalDate, XSD.dateTime),
        (GM.taskDueDate, XSD.dateTime),
    ):
        assert (prop, RDFS.range, rng) in g, f"{prop} must range over {rng}"
    # reminderTrigger ranges over a union datatype (xsd:duration OR xsd:dateTime)
    range_nodes = list(g.objects(GM.reminderTrigger, RDFS.range))
    assert len(range_nodes) == 1, "reminderTrigger must have exactly one range"
    range_node = range_nodes[0]
    assert (range_node, RDF.type, RDFS.Datatype) in g
    union_head = g.value(range_node, OWL.unionOf)
    union_members = list(g.items(union_head)) if union_head else []
    assert XSD.duration in union_members, "reminderTrigger range must include duration"
    assert XSD.dateTime in union_members, "reminderTrigger range must include dateTime"
    # taskPriority is integer (0-9)
    assert (GM.taskPriority, RDFS.range, XSD.integer) in g


# --------------------------------------------------------------------------- #
# Orthogonality — schedule, invitation, availability, reminder, task axes are
# independent. No inferential bridge between them.
# Retained: itertools.combinations sweep (45 pairs x 4 assertions).
# --------------------------------------------------------------------------- #

_ORTHOGONAL_PROPS = (
    "scheduleTemplateEvent",
    "scheduleRecurrenceRule",
    "invitationEvent",
    "invitationStatus",
    "availabilitySlot",
    "availabilityStatus",
    "reminderTrigger",
    "reminderAction",
    "taskDueDate",
    "taskStatus",
)


def test_calendar_axes_are_independent() -> None:
    g = _graph()
    for a, b in combinations(_ORTHOGONAL_PROPS, 2):
        na, nb = URIRef(GMEOW + a), URIRef(GMEOW + b)
        assert (na, RDFS.subPropertyOf, nb) not in g, f"{a} ⊑ {b} forbidden"
        assert (nb, RDFS.subPropertyOf, na) not in g, f"{b} ⊑ {a} forbidden"
        assert (na, OWL.equivalentProperty, nb) not in g
        assert (nb, OWL.equivalentProperty, na) not in g


# --------------------------------------------------------------------------- #
# Organizer/attendee reuse — roleOrganizer and roleAttendee already exist in
# events.ttl as ParticipantRole values; no new terms needed.
# Retained: cross-slice subjects defined in slices/core/event/module.ttl.
# --------------------------------------------------------------------------- #


def test_organizer_and_attendee_roles_exist() -> None:
    g = _graph()
    assert (GM.roleOrganizer, RDF.type, GM.ParticipantRole) in g
    assert (GM.roleAttendee, RDF.type, GM.ParticipantRole) in g
