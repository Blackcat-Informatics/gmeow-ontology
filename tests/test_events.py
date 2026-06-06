"""The universal events facility (#41).

GMEOW had no universal event: genealogy modelled life events with the exact
double anti-pattern it refuses everywhere else — type-as-subclass (~30 LifeEvent
subclasses) AND role-as-subproperty (hasPrincipal / hasWitness / hasOfficiant ⊑
hasParticipant), plus a free-text eventDate. This module mints one gmeow:Event,
reifies participation as the gmeow:Participation relator, makes type and role open
VALUE vocabularies, gives time a real precision story, and absorbs the genealogy
hierarchy forward (Principle 6 — get it right, never keep the inferior form).

The centrepiece here is the anti-subclass / anti-subproperty REGRESSION GUARD: it
pins the refactor permanently, so the frozen taxonomy can never grow back. See
ontology/modules/events.ttl.
"""

from __future__ import annotations

from itertools import combinations
from pathlib import Path

from rdflib import OWL, RDF, RDFS, XSD, Graph, Namespace, URIRef

from gmeow_tools.graph import load_merged_graph
from gmeow_tools.validate import run_shacl

GMEOW = "https://blackcatinformatics.ca/gmeow/"
GM = Namespace(GMEOW)
GUFO = Namespace("http://purl.org/nemo/gufo#")
EX_EVENTS = Namespace("https://blackcatinformatics.ca/gmeow/examples/events/")
SHAPES_FIXTURES = Path(__file__).parent / "fixtures" / "shapes"
COVERAGE_FIXTURES = Path(__file__).parent / "fixtures" / "coverage"


def _graph() -> Graph:
    return load_merged_graph(include_imports=False)


def _fixture(name: str) -> Graph:
    return Graph().parse(SHAPES_FIXTURES / f"{name}.ttl", format="turtle")


# --------------------------------------------------------------------------- #
# gUFO grounding — Event is a perdurant type; Participation is a relator Kind.
# --------------------------------------------------------------------------- #


def test_event_is_grounded_in_gufo_event() -> None:
    g = _graph()
    assert (GM.Event, RDF.type, OWL.Class) in g
    assert (GM.Event, RDFS.subClassOf, GUFO.Event) in g
    # The former top occurrences re-parent onto the universal Event.
    assert (GM.Activity, RDFS.subClassOf, GM.Event) in g
    assert (GM.LifeEvent, RDFS.subClassOf, GM.Event) in g


def test_participation_is_a_gufo_relator() -> None:
    g = _graph()
    assert (GM.Participation, RDFS.subClassOf, GUFO.Relator) in g
    assert (GM.Participation, RDF.type, OWL.Class) in g


# --------------------------------------------------------------------------- #
# THE CENTREPIECE — anti-subclass / anti-subproperty regression guard. Type is a
# value; role is a value on a relator. This locks the refactor permanently.
# --------------------------------------------------------------------------- #

# The former genealogy event subclasses — must NOT exist as classes again.
_FORMER_EVENT_SUBCLASSES = (
    "Birth",
    "Death",
    "Burial",
    "Marriage",
    "Divorce",
    "Adoption",
    "Christening",
    "NameChange",
    "Census",
    "Immigration",
)
# The former participant-role subproperties — must NOT exist as properties again.
_FORMER_ROLE_SUBPROPERTIES = ("hasPrincipal", "hasWitness", "hasOfficiant")
# The former free-text temporal / place properties — replaced by structured terms.
_FORMER_FLAT_PROPERTIES = ("eventDate", "eventPlace")


def test_event_type_and_role_are_value_object_properties() -> None:
    """Type and role are pointed at by ObjectProperties into value vocabularies —
    never frozen into the class/property taxonomy."""
    g = _graph()
    assert (GM.eventType, RDF.type, OWL.ObjectProperty) in g
    assert (GM.eventType, RDFS.range, GM.EventType) in g
    assert (GM.participationRole, RDF.type, OWL.ObjectProperty) in g
    assert (GM.participationRole, RDFS.range, GM.ParticipantRole) in g
    # eventType is NON-functional (an occurrence may bear several types).
    assert (GM.eventType, RDF.type, OWL.FunctionalProperty) not in g


def test_former_event_types_are_individuals_not_classes() -> None:
    """The ~30 LifeEvent subclasses became gmeow:eventType VALUE individuals.

    This is the permanent lock: each former type IRI is gone as a class, and its
    replacement is an individual of gmeow:EventType — not a class, not a subclass.
    """
    g = _graph()
    for local in _FORMER_EVENT_SUBCLASSES:
        old = URIRef(GMEOW + local)
        assert (old, RDF.type, OWL.Class) not in g, f"{local} must not be a class"
        assert (old, RDFS.subClassOf, GM.LifeEvent) not in g
        assert (old, RDFS.subClassOf, GM.Event) not in g
        value = URIRef(GMEOW + "eventType" + local)
        assert (value, RDF.type, GM.EventType) in g, f"eventType{local} must be a value"
        assert (value, RDF.type, OWL.Class) not in g


def test_former_role_subproperties_are_gone_and_are_values_now() -> None:
    """hasPrincipal / hasWitness / hasOfficiant are no longer subproperties; the
    roles are gmeow:ParticipantRole value individuals borne on a Participation."""
    g = _graph()
    prop_types = (OWL.ObjectProperty, OWL.DatatypeProperty, OWL.AnnotationProperty)
    for local in _FORMER_ROLE_SUBPROPERTIES:
        old = URIRef(GMEOW + local)
        for pt in prop_types:
            assert (old, RDF.type, pt) not in g, f"{local} must not be a property"
        assert (old, RDFS.subPropertyOf, GM.hasParticipant) not in g
    # The role values that generalize them exist.
    for role in ("roleParticipantPrincipal", "roleWitness", "roleOfficiant"):
        assert (URIRef(GMEOW + role), RDF.type, GM.ParticipantRole) in g


def test_free_text_event_date_and_place_are_gone() -> None:
    """gmeow:eventDate (free-text rdfs:Literal) and gmeow:eventPlace are removed;
    structured gmeow:eventTime / eventInterval + gmeow:eventLocation replace them."""
    g = _graph()
    prop_types = (OWL.ObjectProperty, OWL.DatatypeProperty, OWL.AnnotationProperty)
    for local in _FORMER_FLAT_PROPERTIES:
        old = URIRef(GMEOW + local)
        for pt in prop_types:
            assert (old, RDF.type, pt) not in g, f"{local} must not exist"
    # The structured replacements are present.
    assert (GM.eventTime, RDF.type, OWL.DatatypeProperty) in g
    assert (GM.eventLocation, RDF.type, OWL.ObjectProperty) in g
    assert (GM.eventLocation, RDFS.range, GM.Location) in g


# --------------------------------------------------------------------------- #
# Orthogonality — eventType ⟂ participationRole ⟂ temporal ⟂ location. No
# inferential bridge, no shared value space (mirrors test_identity_orthogonality).
# --------------------------------------------------------------------------- #

# Axis representative property → its (exclusive) range class / datatype.
_AXES: dict[str, URIRef] = {
    "eventType": GM.EventType,
    "participationRole": GM.ParticipantRole,
    "eventTime": XSD.dateTime,
    "eventLocation": GM.Location,
}


def test_axes_range_over_distinct_spaces() -> None:
    g = _graph()
    ranges: set[URIRef] = set()
    for prop, rng in _AXES.items():
        node = URIRef(GMEOW + prop)
        declared = set(g.objects(node, RDFS.range))
        assert declared == {rng}, f"{prop} must range over only {rng}"
        ranges.add(rng)
    assert len(ranges) == len(_AXES)  # four distinct spaces


def test_no_axis_is_inferred_from_another() -> None:
    g = _graph()
    for a, b in combinations(_AXES, 2):
        na, nb = URIRef(GMEOW + a), URIRef(GMEOW + b)
        assert (na, RDFS.subPropertyOf, nb) not in g, f"{a} ⊑ {b} forbidden"
        assert (nb, RDFS.subPropertyOf, na) not in g, f"{b} ⊑ {a} forbidden"
        assert (na, OWL.equivalentProperty, nb) not in g
        assert (nb, OWL.equivalentProperty, na) not in g


# --------------------------------------------------------------------------- #
# DL-clean datatypes (Principle 3) — base triples use xsd:dateTime, never
# xsd:date (which is not in the OWL 2 datatype map).
# --------------------------------------------------------------------------- #


def test_temporal_datatypes_are_datetime_not_date() -> None:
    g = _graph()
    for prop in ("eventTime", "earliestStart", "latestEnd"):
        node = URIRef(GMEOW + prop)
        assert (node, RDFS.range, XSD.dateTime) in g, f"{prop} must range xsd:dateTime"
        assert (node, RDFS.range, XSD.date) not in g, f"{prop} must not range xsd:date"


# --------------------------------------------------------------------------- #
# Relator mediation axiom (#38) — open-world EL someValuesFrom; closed-world
# cardinality is SHACL's (ParticipationHasPlayersShape).
# --------------------------------------------------------------------------- #


def test_participation_mediation_axiom_present() -> None:
    g = _graph()
    mediated: set[URIRef] = set()
    for restriction in g.objects(GM.Participation, RDFS.subClassOf):
        on = g.value(restriction, OWL.onProperty)
        some = g.value(restriction, OWL.someValuesFrom)
        if isinstance(on, URIRef) and some is not None:
            mediated.add(on)
            if on == GM.participationEvent:
                assert some == GM.Event
            if on == GM.participationParticipant:
                assert some == GM.Entity
    assert {GM.participationEvent, GM.participationParticipant} <= mediated


# --------------------------------------------------------------------------- #
# Value-vocabulary membership — the seeds are individuals, the set is open.
# --------------------------------------------------------------------------- #


def test_participant_role_vocabulary_seeded() -> None:
    g = _graph()
    roles = set(g.subjects(RDF.type, GM.ParticipantRole))
    for role in (
        "roleParticipantPrincipal",
        "roleOrganizer",
        "roleAttendee",
        "rolePerformer",
        "roleOfficiant",
        "roleWitness",
        "roleVictim",
        "roleAgent",
        "roleBeneficiary",
    ):
        assert URIRef(GMEOW + role) in roles


def test_temporal_precision_vocabulary_seeded() -> None:
    g = _graph()
    members = set(g.subjects(RDF.type, GM.TemporalPrecision))
    assert members == {
        GM.precisionDay,
        GM.precisionMonth,
        GM.precisionYear,
        GM.precisionDecade,
        GM.precisionCirca,
    }


def test_subevent_mereology_is_transitive() -> None:
    g = _graph()
    assert (GM.subEventOf, RDF.type, OWL.TransitiveProperty) in g
    assert (GM.hasSubEvent, RDF.type, OWL.TransitiveProperty) in g
    # Non-simple (transitive) ⇒ never in a cardinality/functional axiom.
    assert (GM.subEventOf, RDF.type, OWL.FunctionalProperty) not in g


# --------------------------------------------------------------------------- #
# SHACL well-formedness of the Participation relator.
# --------------------------------------------------------------------------- #


def test_wellformed_participation_conforms() -> None:
    result = run_shacl(_fixture("participation-wellformed"))
    assert result.ok, "\n".join(result.errors)


def test_malformed_participation_is_flagged() -> None:
    result = run_shacl(_fixture("participation-malformed"))
    assert not result.ok
    joined = "\n".join(result.errors)
    assert "participationEvent" in joined or "participationParticipant" in joined


# --------------------------------------------------------------------------- #
# Standpoint coexistence — contested event-type / date claims COEXIST, none
# privileged, no preferred/primary term (consume #43). The events slice is a
# consumer of the standpoint facility, not a re-specifier.
# --------------------------------------------------------------------------- #


def test_contested_event_claims_coexist_and_validate() -> None:
    """Two contradictory standpoint-indexed eventType claims (genocide vs armed
    clash) load, SHACL-pass, and are BOTH retained — neither is the ground truth."""
    g = Graph().parse(COVERAGE_FIXTURES / "events.ttl", format="turtle")
    result = run_shacl(g)
    assert result.ok, "\n".join(result.errors)
    types = set(g.objects(EX_EVENTS.disputedEvent, GM.eventType))
    assert {EX_EVENTS.eventTypeGenocide, EX_EVENTS.eventTypeArmedClash} <= types
    # A contested date likewise coexists — two instants, neither privileged.
    dates = set(g.objects(EX_EVENTS.disputedEvent, GM.eventTime))
    assert len(dates) == 2


def test_no_preferred_or_primary_event_term() -> None:
    """Co-equality (Principle 9): the events module mints no primary*/preferred*
    selector for a contested type, role, date, or location."""
    g = _graph()
    prop_types = (OWL.ObjectProperty, OWL.DatatypeProperty, OWL.AnnotationProperty)
    for banned in (
        "primaryEventType",
        "preferredEventType",
        "primaryParticipant",
        "preferredParticipant",
        "primaryClaim",
        "preferredRank",
    ):
        node = URIRef(GMEOW + banned)
        for pt in prop_types:
            assert (node, RDF.type, pt) not in g, f"{banned} must not exist"
        assert (node, RDF.type, OWL.Class) not in g
