"""The calendar and scheduling slice (#62).

Builds on the events module (#41) and temporal module, adding recurrence,
availability, RSVP, reminders, tasks, and time zones. Reuses Event, Participation,
EventSeries/RecurrenceRule, TimeInterval, and the four clocks.

The centrepiece guards here are:
  * Anti-subclass / anti-subproperty: status vocabularies are individuals, never
    classes. No CancelledEvent subclass, no hasAttendee subproperty.
  * Suppression, never erasure (Principle 10): cancelled occurrences keep their
    URI with displayable false.
  * Open value vocabularies (Principle 9): invitationStatus, rsvpStatus,
    availabilityStatus, reminderAction, taskStatus, exceptionType are all
    gufo:QualityValue individuals.
"""

from __future__ import annotations

from itertools import combinations

from rdflib import OWL, RDF, RDFS, XSD, Graph, Namespace, URIRef

from gmeow_tools.graph import load_merged_graph

GMEOW = "https://blackcatinformatics.ca/gmeow/"
GM = Namespace(GMEOW)
GUFO = Namespace("http://purl.org/nemo/gufo#")


def _graph() -> Graph:
    return load_merged_graph(include_imports=False)


# --------------------------------------------------------------------------- #
# gUFO grounding
# --------------------------------------------------------------------------- #


def test_calendar_is_information_object() -> None:
    g = _graph()
    assert (GM.Calendar, RDF.type, OWL.Class) in g
    assert (GM.Calendar, RDFS.subClassOf, GM.InformationObject) in g


def test_event_schedule_is_relator() -> None:
    g = _graph()
    assert (GM.EventSchedule, RDF.type, OWL.Class) in g
    assert (GM.EventSchedule, RDFS.subClassOf, GUFO.Relator) in g


def test_schedule_exception_is_relator() -> None:
    g = _graph()
    assert (GM.ScheduleException, RDF.type, OWL.Class) in g
    assert (GM.ScheduleException, RDFS.subClassOf, GUFO.Relator) in g


def test_event_invitation_is_agreement_and_relator() -> None:
    g = _graph()
    assert (GM.EventInvitation, RDF.type, OWL.Class) in g
    assert (GM.EventInvitation, RDFS.subClassOf, GM.Agreement) in g
    assert (GM.EventInvitation, RDFS.subClassOf, GUFO.Relator) in g


def test_availability_is_time_scoped_relation() -> None:
    g = _graph()
    assert (GM.Availability, RDF.type, OWL.Class) in g
    assert (GM.Availability, RDFS.subClassOf, GM.TimeScopedRelation) in g


def test_reminder_is_entity() -> None:
    g = _graph()
    assert (GM.Reminder, RDF.type, OWL.Class) in g
    assert (GM.Reminder, RDFS.subClassOf, GM.Entity) in g


def test_task_is_event() -> None:
    g = _graph()
    assert (GM.Task, RDF.type, OWL.Class) in g
    assert (GM.Task, RDFS.subClassOf, GM.Event) in g


def test_time_zone_is_entity() -> None:
    g = _graph()
    assert (GM.TimeZone, RDF.type, OWL.Class) in g
    assert (GM.TimeZone, RDFS.subClassOf, GM.Entity) in g


# --------------------------------------------------------------------------- #
# Anti-subclass / anti-subproperty guard — statuses are values, never classes.
# --------------------------------------------------------------------------- #

_VALUE_VOCABULARIES: dict[str, str] = {
    "InvitationStatus": "invitationStatus",
    "RsvpStatus": "rsvpStatus",
    "AvailabilityStatus": "availabilityStatus",
    "ReminderAction": "reminderAction",
    "TaskStatus": "taskStatus",
    "ExceptionType": "exceptionType",
}

_STATUS_INDIVIDUALS: dict[str, tuple[str, ...]] = {
    "InvitationStatus": (
        "invitationStatusNeedsAction",
        "invitationStatusAccepted",
        "invitationStatusDeclined",
        "invitationStatusTentative",
    ),
    "RsvpStatus": (
        "rsvpStatusNeedsAction",
        "rsvpStatusAccepted",
        "rsvpStatusDeclined",
        "rsvpStatusTentative",
    ),
    "AvailabilityStatus": (
        "availabilityStatusFree",
        "availabilityStatusBusy",
        "availabilityStatusTentative",
        "availabilityStatusOutOfOffice",
    ),
    "ReminderAction": (
        "reminderActionDisplay",
        "reminderActionEmail",
        "reminderActionAudio",
    ),
    "TaskStatus": (
        "taskStatusNotStarted",
        "taskStatusInProgress",
        "taskStatusCompleted",
        "taskStatusCancelled",
    ),
    "ExceptionType": (
        "exceptionTypeCancellation",
        "exceptionTypeRescheduling",
    ),
}


def test_calendar_value_vocabularies_are_object_properties() -> None:
    """Each status axis is pointed at by an ObjectProperty into a value vocabulary —
    never frozen into the class/property taxonomy."""
    g = _graph()
    for vocab, prop_local in _VALUE_VOCABULARIES.items():
        prop = URIRef(GMEOW + prop_local)
        assert (prop, RDF.type, OWL.ObjectProperty) in g, (
            f"{prop_local} must be an ObjectProperty"
        )
        assert (prop, RDFS.range, URIRef(GMEOW + vocab)) in g


def test_calendar_statuses_are_individuals_not_classes() -> None:
    """Every status value is an individual of its value class, never a class itself."""
    g = _graph()
    for vocab, individuals in _STATUS_INDIVIDUALS.items():
        vocab_class = URIRef(GMEOW + vocab)
        for local in individuals:
            node = URIRef(GMEOW + local)
            assert (node, RDF.type, vocab_class) in g, (
                f"{local} must be a {vocab} value"
            )
            assert (node, RDF.type, OWL.Class) not in g, f"{local} must not be a class"


def test_no_cancelled_event_subclass_exists() -> None:
    """Cancellation is a ScheduleException + displayable false, never a subclass."""
    g = _graph()
    banned = ("CancelledEvent", "CanceledEvent", "RescheduledEvent")
    for local in banned:
        node = URIRef(GMEOW + local)
        assert (node, RDF.type, OWL.Class) not in g, (
            f"{local} must not exist as a class"
        )


# --------------------------------------------------------------------------- #
# Relator mediation axioms (open-world EL someValuesFrom)
# --------------------------------------------------------------------------- #


def test_event_schedule_mediation_axiom_present() -> None:
    g = _graph()
    mediated: set[URIRef] = set()
    for restriction in g.objects(GM.EventSchedule, RDFS.subClassOf):
        on = g.value(restriction, OWL.onProperty)
        some = g.value(restriction, OWL.someValuesFrom)
        if isinstance(on, URIRef) and some is not None:
            mediated.add(on)
            if on == GM.scheduleTemplateEvent:
                assert some == GM.Event
    assert GM.scheduleTemplateEvent in mediated


def test_schedule_exception_mediation_axiom_present() -> None:
    g = _graph()
    mediated: set[URIRef] = set()
    for restriction in g.objects(GM.ScheduleException, RDFS.subClassOf):
        on = g.value(restriction, OWL.onProperty)
        some = g.value(restriction, OWL.someValuesFrom)
        if isinstance(on, URIRef) and some is not None:
            mediated.add(on)
    assert GM.exceptionSchedule in mediated
    assert GM.exceptionOriginalDate in mediated


def test_event_invitation_mediation_axiom_present() -> None:
    g = _graph()
    mediated: set[URIRef] = set()
    for restriction in g.objects(GM.EventInvitation, RDFS.subClassOf):
        on = g.value(restriction, OWL.onProperty)
        some = g.value(restriction, OWL.someValuesFrom)
        if isinstance(on, URIRef) and some is not None:
            mediated.add(on)
    assert GM.invitationEvent in mediated
    assert GM.invitationInvitee in mediated


def test_availability_mediation_axiom_present() -> None:
    g = _graph()
    mediated: set[URIRef] = set()
    for restriction in g.objects(GM.Availability, RDFS.subClassOf):
        on = g.value(restriction, OWL.onProperty)
        some = g.value(restriction, OWL.someValuesFrom)
        if isinstance(on, URIRef) and some is not None:
            mediated.add(on)
    assert GM.availabilityAgent in mediated
    assert GM.availabilitySlot in mediated


def test_reminder_mediation_axiom_present() -> None:
    g = _graph()
    mediated: set[URIRef] = set()
    for restriction in g.objects(GM.Reminder, RDFS.subClassOf):
        on = g.value(restriction, OWL.onProperty)
        some = g.value(restriction, OWL.someValuesFrom)
        if isinstance(on, URIRef) and some is not None:
            mediated.add(on)
    assert GM.reminderTarget in mediated


# --------------------------------------------------------------------------- #
# Value vocabulary membership — seeds are individuals, the set is open.
# --------------------------------------------------------------------------- #


def test_invitation_status_vocabulary_seeded() -> None:
    g = _graph()
    statuses = set(g.subjects(RDF.type, GM.InvitationStatus))
    for local in _STATUS_INDIVIDUALS["InvitationStatus"]:
        assert URIRef(GMEOW + local) in statuses


def test_rsvp_status_vocabulary_seeded() -> None:
    g = _graph()
    statuses = set(g.subjects(RDF.type, GM.RsvpStatus))
    for local in _STATUS_INDIVIDUALS["RsvpStatus"]:
        assert URIRef(GMEOW + local) in statuses


def test_availability_status_vocabulary_seeded() -> None:
    g = _graph()
    statuses = set(g.subjects(RDF.type, GM.AvailabilityStatus))
    for local in _STATUS_INDIVIDUALS["AvailabilityStatus"]:
        assert URIRef(GMEOW + local) in statuses


def test_reminder_action_vocabulary_seeded() -> None:
    g = _graph()
    actions = set(g.subjects(RDF.type, GM.ReminderAction))
    for local in _STATUS_INDIVIDUALS["ReminderAction"]:
        assert URIRef(GMEOW + local) in actions


def test_task_status_vocabulary_seeded() -> None:
    g = _graph()
    statuses = set(g.subjects(RDF.type, GM.TaskStatus))
    for local in _STATUS_INDIVIDUALS["TaskStatus"]:
        assert URIRef(GMEOW + local) in statuses


def test_exception_type_vocabulary_seeded() -> None:
    g = _graph()
    types = set(g.subjects(RDF.type, GM.ExceptionType))
    for local in _STATUS_INDIVIDUALS["ExceptionType"]:
        assert URIRef(GMEOW + local) in types


# --------------------------------------------------------------------------- #
# DL-clean datatypes (Principle 3)
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
# No preferred / primary claim (Principle 9)
# --------------------------------------------------------------------------- #


def test_no_preferred_or_primary_calendar_term() -> None:
    g = _graph()
    prop_types = (OWL.ObjectProperty, OWL.DatatypeProperty, OWL.AnnotationProperty)
    for banned in (
        "primaryInvitation",
        "preferredInvitation",
        "primaryAvailability",
        "preferredAvailability",
        "primaryTask",
        "preferredTask",
        "primaryReminder",
        "preferredReminder",
    ):
        node = URIRef(GMEOW + banned)
        for pt in prop_types:
            assert (node, RDF.type, pt) not in g, f"{banned} must not exist"
        assert (node, RDF.type, OWL.Class) not in g


# --------------------------------------------------------------------------- #
# Structural properties — domain/range and functionality
# --------------------------------------------------------------------------- #


def test_event_schedule_properties_are_well_typed() -> None:
    g = _graph()
    assert (GM.scheduleTemplateEvent, RDFS.domain, GM.EventSchedule) in g
    assert (GM.scheduleTemplateEvent, RDFS.range, GM.Event) in g
    assert (GM.scheduleTemplateEvent, RDF.type, OWL.FunctionalProperty) in g
    assert (GM.scheduleRecurrenceRule, RDFS.domain, GM.EventSchedule) in g
    assert (GM.scheduleRecurrenceRule, RDFS.range, GM.RecurrenceRule) in g
    assert (GM.scheduleTimeZone, RDFS.domain, GM.EventSchedule) in g
    assert (GM.scheduleTimeZone, RDFS.range, GM.TimeZone) in g
    assert (GM.scheduleOccurrence, RDFS.domain, GM.EventSchedule) in g
    assert (GM.scheduleOccurrence, RDFS.range, GM.Event) in g


def test_invitation_properties_are_well_typed() -> None:
    g = _graph()
    assert (GM.invitationEvent, RDFS.domain, GM.EventInvitation) in g
    assert (GM.invitationEvent, RDFS.range, GM.Event) in g
    assert (GM.invitationEvent, RDF.type, OWL.FunctionalProperty) in g
    assert (GM.invitationInvitee, RDFS.domain, GM.EventInvitation) in g
    assert (GM.invitationInvitee, RDFS.range, GM.Agent) in g
    assert (GM.invitationStatus, RDFS.domain, GM.EventInvitation) in g
    assert (GM.invitationStatus, RDFS.range, GM.InvitationStatus) in g
    assert (GM.rsvpStatus, RDFS.domain, GM.EventInvitation) in g
    assert (GM.rsvpStatus, RDFS.range, GM.RsvpStatus) in g


def test_availability_properties_are_well_typed() -> None:
    g = _graph()
    assert (GM.availabilityAgent, RDFS.domain, GM.Availability) in g
    assert (GM.availabilityAgent, RDFS.range, GM.Agent) in g
    assert (GM.availabilitySlot, RDFS.domain, GM.Availability) in g
    assert (GM.availabilitySlot, RDFS.range, GM.TimeInterval) in g
    assert (GM.availabilitySlot, RDF.type, OWL.FunctionalProperty) in g
    assert (GM.availabilityStatus, RDFS.domain, GM.Availability) in g
    assert (GM.availabilityStatus, RDFS.range, GM.AvailabilityStatus) in g


def test_reminder_properties_are_well_typed() -> None:
    g = _graph()
    assert (GM.reminderTrigger, RDFS.domain, GM.Reminder) in g
    assert (GM.reminderAction, RDFS.domain, GM.Reminder) in g
    assert (GM.reminderAction, RDFS.range, GM.ReminderAction) in g
    assert (GM.reminderTarget, RDFS.domain, GM.Reminder) in g
    assert (GM.reminderTarget, RDFS.range, GM.Event) in g
    assert (GM.reminderTarget, RDF.type, OWL.FunctionalProperty) in g


def test_task_properties_are_well_typed() -> None:
    g = _graph()
    assert (GM.taskDueDate, RDFS.domain, GM.Task) in g
    assert (GM.taskDueDate, RDFS.range, XSD.dateTime) in g
    assert (GM.taskStatus, RDFS.domain, GM.Task) in g
    assert (GM.taskStatus, RDFS.range, GM.TaskStatus) in g
    assert (GM.taskPriority, RDFS.domain, GM.Task) in g
    assert (GM.taskPriority, RDFS.range, XSD.integer) in g
    assert (GM.taskRecurrenceUntilDone, RDFS.domain, GM.Task) in g
    assert (GM.taskRecurrenceUntilDone, RDFS.range, XSD.boolean) in g


def test_time_zone_properties_are_well_typed() -> None:
    g = _graph()
    assert (GM.timeZoneIanaId, RDFS.domain, GM.TimeZone) in g
    assert (GM.timeZoneIanaId, RDFS.range, XSD.string) in g
    assert (GM.timeZoneIanaId, RDF.type, OWL.FunctionalProperty) in g
    assert (GM.eventTimeZone, RDFS.domain, GM.Event) in g
    assert (GM.eventTimeZone, RDFS.range, GM.TimeZone) in g
    assert (GM.eventTimeZone, RDF.type, OWL.FunctionalProperty) in g


def test_calendar_collection_properties_are_well_typed() -> None:
    g = _graph()
    assert (GM.calendarTimeZone, RDFS.domain, GM.Calendar) in g
    assert (GM.calendarTimeZone, RDFS.range, GM.TimeZone) in g
    assert (GM.calendarTimeZone, RDF.type, OWL.FunctionalProperty) in g
    assert (GM.calendarMember, RDFS.domain, GM.Calendar) in g
    assert (GM.calendarMember, RDFS.range, GM.Event) in g


def test_schedule_exception_properties_are_well_typed() -> None:
    g = _graph()
    assert (GM.exceptionSchedule, RDFS.domain, GM.ScheduleException) in g
    assert (GM.exceptionSchedule, RDFS.range, GM.EventSchedule) in g
    assert (GM.exceptionOriginalDate, RDFS.domain, GM.ScheduleException) in g
    assert (GM.exceptionOriginalDate, RDFS.range, XSD.dateTime) in g
    assert (GM.exceptionReplacementEvent, RDFS.domain, GM.ScheduleException) in g
    assert (GM.exceptionReplacementEvent, RDFS.range, GM.Event) in g
    assert (GM.exceptionType, RDFS.domain, GM.ScheduleException) in g
    assert (GM.exceptionType, RDFS.range, GM.ExceptionType) in g


# --------------------------------------------------------------------------- #
# Organizer/attendee reuse — roleOrganizer and roleAttendee already exist in
# events.ttl as ParticipantRole values; no new terms needed.
# --------------------------------------------------------------------------- #


def test_organizer_and_attendee_roles_exist() -> None:
    g = _graph()
    assert (GM.roleOrganizer, RDF.type, GM.ParticipantRole) in g
    assert (GM.roleAttendee, RDF.type, GM.ParticipantRole) in g
