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

from gmeow_rdf.compat.rdflib import OWL, RDF, RDFS, XSD, Graph, Namespace, URIRef

from gmeow_tools.graph import load_merged_graph
from tests._graph_nt import run_shacl

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
    # Structural lock: LifeEvent must have ZERO GMEOW subclasses (the taxonomy is
    # gone for good) — catches ANY accidental re-introduction, not just the known list.
    sub = {
        s for s in g.subjects(RDFS.subClassOf, GM.LifeEvent) if str(s).startswith(GMEOW)
    }
    assert sub == set(), f"gmeow:LifeEvent must have no subclasses; found {sub}"


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
# Spacetime / Trajectory (#106) — moving events and 4D spacetime slices reuse
# LocationState and Trajectory from the locations module (#94).
# --------------------------------------------------------------------------- #


def test_event_trajectory_exists_and_ranges_over_trajectory() -> None:
    g = _graph()
    assert (GM.eventTrajectory, RDF.type, OWL.ObjectProperty) in g
    assert (GM.eventTrajectory, RDFS.domain, GM.Event) in g
    assert (GM.eventTrajectory, RDFS.range, GM.Trajectory) in g
    assert (GM.eventTrajectory, RDF.type, OWL.FunctionalProperty) not in g


def test_event_spacetime_exists_and_ranges_over_location_state() -> None:
    g = _graph()
    assert (GM.eventSpacetime, RDF.type, OWL.ObjectProperty) in g
    assert (GM.eventSpacetime, RDFS.domain, GM.Event) in g
    assert (GM.eventSpacetime, RDFS.range, GM.LocationState) in g
    assert (GM.eventSpacetime, RDF.type, OWL.FunctionalProperty) not in g


def test_event_spacetime_and_trajectory_are_orthogonal() -> None:
    """No inferential bridge between eventSpacetime, eventTrajectory, eventLocation,
    or eventInterval — each is an independent assertion axis."""
    g = _graph()
    for a, b in combinations(
        ("eventSpacetime", "eventTrajectory", "eventLocation", "eventInterval"), 2
    ):
        na, nb = URIRef(GMEOW + a), URIRef(GMEOW + b)
        assert (na, RDFS.subPropertyOf, nb) not in g, f"{a} ⊑ {b} forbidden"
        assert (nb, RDFS.subPropertyOf, na) not in g, f"{b} ⊑ {a} forbidden"
        assert (na, OWL.equivalentProperty, nb) not in g, f"{a} ≡ {b} forbidden"
        assert (nb, OWL.equivalentProperty, na) not in g, f"{b} ≡ {a} forbidden"


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
    # An OPEN value vocabulary — assert the seeds are PRESENT (superset), never that
    # the set is closed, so a future seed (or a data-minted value) doesn't break it.
    g = _graph()
    members = set(g.subjects(RDF.type, GM.TemporalPrecision))
    assert {
        GM.precisionDay,
        GM.precisionMonth,
        GM.precisionYear,
        GM.precisionDecade,
        GM.precisionCirca,
    } <= members


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
    g = Graph().parse(COVERAGE_FIXTURES / "events-contested.ttl", format="turtle")
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


# --------------------------------------------------------------------------- #
# Projections — the reified, multi-modal event downcasts to flat consumer forms.
# schema.org event roles + the iCalendar VEVENT profile (maximally projected).
# --------------------------------------------------------------------------- #

SCHEMA = Namespace("https://schema.org/")
ICAL = Namespace("http://www.w3.org/2002/12/cal/icaltzd#")
TIME = Namespace("http://www.w3.org/2006/time#")


def _events_projected(profile: str) -> Graph:
    """The events worked-example fixture projected to a target profile."""
    from gmeow_tools.projections import project_graph

    src = load_merged_graph(include_imports=False)
    src.parse(COVERAGE_FIXTURES / "events.ttl", format="turtle")
    return project_graph(profile, src)


def test_schema_role_projection_keys_by_role() -> None:
    """The reified Participation downcasts to the role-keyed flat schema.org edges —
    each role lands on its own predicate, not all three together."""
    out = _events_projected("schema-org")
    assert EX_EVENTS.casey in set(out.objects(EX_EVENTS.reception, SCHEMA.organizer))
    assert EX_EVENTS.band in set(out.objects(EX_EVENTS.reception, SCHEMA.performer))
    assert EX_EVENTS.dana in set(out.objects(EX_EVENTS.reception, SCHEMA.attendee))
    # Roles don't bleed across predicates: the organizer is not also an attendee.
    assert EX_EVENTS.casey not in set(out.objects(EX_EVENTS.reception, SCHEMA.attendee))


def test_schema_role_projection_suppresses_withdrawn_participation() -> None:
    """A superseded participation (gmeow:displayable false) is NOT projected — the
    flat downcast honours suppression-not-erasure (Principle 10)."""
    out = _events_projected("schema-org")
    # ex:erin's attendee participation is displayable false → must be dropped.
    assert EX_EVENTS.erin not in set(out.objects(EX_EVENTS.reception, SCHEMA.attendee))


def test_schema_fuzzy_time_projects_earliest_bound() -> None:
    out = _events_projected("schema-org")
    starts = set(out.objects(EX_EVENTS.siege, SCHEMA.startDate))
    assert any(str(s).startswith("1453-04-01") for s in starts)


def test_ical_vevent_interval_has_start_end_and_location() -> None:
    """A crisp-interval event projects to a VEVENT with DTSTART/DTEND + LOCATION."""
    out = _events_projected("ical")
    assert (EX_EVENTS.wedding, RDF.type, ICAL.Vevent) in out
    assert set(out.objects(EX_EVENTS.wedding, ICAL.dtstart))
    assert set(out.objects(EX_EVENTS.wedding, ICAL.dtend))
    assert EX_EVENTS.chapel in set(out.objects(EX_EVENTS.wedding, ICAL.location))


def test_ical_vevent_point_has_start_only() -> None:
    out = _events_projected("ical")
    assert (EX_EVENTS.reception, RDF.type, ICAL.Vevent) in out
    assert set(out.objects(EX_EVENTS.reception, ICAL.dtstart))
    assert not set(out.objects(EX_EVENTS.reception, ICAL.dtend))


def test_ical_vevent_fuzzy_spans_the_bounds() -> None:
    """A circa-dated event becomes a VEVENT spanning earliestStart→latestEnd."""
    out = _events_projected("ical")
    starts = {str(o) for o in out.objects(EX_EVENTS.siege, ICAL.dtstart)}
    ends = {str(o) for o in out.objects(EX_EVENTS.siege, ICAL.dtend)}
    assert any(s.startswith("1453-04-01") for s in starts)
    assert any(e.startswith("1453-05-31") for e in ends)


def test_ical_summary_is_the_event_type_label() -> None:
    """The open eventType vocabulary collapses to a human-readable SUMMARY label."""
    out = _events_projected("ical")
    summaries = {str(o) for o in out.objects(EX_EVENTS.wedding, ICAL.summary)}
    assert "marriage" in summaries


# --------------------------------------------------------------------------- #
# Temporal relations — Allen's interval algebra over events (OWL-Time / TEO /
# ISO-TimeML interop). DL-clean: transitive/symmetric, never in cardinality.
# --------------------------------------------------------------------------- #

# Allen relation → (inverse local name or None if symmetric, is-transitive).
_ALLEN: dict[str, tuple[str | None, bool]] = {
    "before": ("after", True),
    "after": ("before", True),
    "meets": ("metBy", False),
    "metBy": ("meets", False),
    "overlaps": ("overlappedBy", False),
    "overlappedBy": ("overlaps", False),
    "starts": ("startedBy", False),
    "startedBy": ("starts", False),
    "during": ("contains", True),
    "contains": ("during", True),
    "finishes": ("finishedBy", False),
    "finishedBy": ("finishes", False),
    "coincidesWith": (None, True),  # symmetric
}


def test_allen_relations_exist_on_events() -> None:
    g = _graph()
    for rel in _ALLEN:
        node = URIRef(GMEOW + rel)
        assert (node, RDF.type, OWL.ObjectProperty) in g, f"{rel} must exist"
        assert (node, RDFS.domain, GM.Event) in g, f"{rel} domain Event"
        assert (node, RDFS.range, GM.Event) in g, f"{rel} range Event"


def test_allen_inverses_and_characters() -> None:
    """before/after + during/contains are transitive; coincidesWith is symmetric +
    transitive; the rest are inverse-only — and none is functional (DL regularity)."""
    g = _graph()
    for rel, (inverse, transitive) in _ALLEN.items():
        node = URIRef(GMEOW + rel)
        if transitive:
            assert (node, RDF.type, OWL.TransitiveProperty) in g, f"{rel} transitive"
        # Never functional/inverse-functional (non-simple ⇒ OWL 2 DL regularity).
        assert (node, RDF.type, OWL.FunctionalProperty) not in g
        if inverse is None:
            assert (node, RDF.type, OWL.SymmetricProperty) in g, f"{rel} symmetric"
        else:
            inv = URIRef(GMEOW + inverse)
            assert (node, OWL.inverseOf, inv) in g or (inv, OWL.inverseOf, node) in g


def test_duration_and_recurrence_terms() -> None:
    g = _graph()
    assert (GM.Duration, RDF.type, OWL.Class) in g
    assert (GM.durationValue, RDFS.range, XSD.duration) in g  # DL-clean xsd:duration
    assert (GM.RecurrenceRule, RDFS.subClassOf, GM.InformationObject) in g
    assert (GM.hasRecurrenceRule, RDFS.domain, GM.EventSeries) in g


def test_timeml_tense_and_aspect_value_vocabs() -> None:
    g = _graph()
    tenses = set(g.subjects(RDF.type, GM.GrammaticalTense))
    assert {GM.tensePast, GM.tensePresent, GM.tenseFuture, GM.tenseNone} <= tenses
    aspects = set(g.subjects(RDF.type, GM.GrammaticalAspect))
    assert {GM.aspectPerfective, GM.aspectProgressive, GM.aspectNone} <= aspects
    # Tense/aspect are an annotation axis — never bridged to the temporal placement.
    assert (GM.eventTense, RDFS.subPropertyOf, GM.eventTime) not in g
    assert (GM.eventAspect, RDFS.subPropertyOf, GM.eventTime) not in g


def test_owl_time_projection_emits_pure_interval_relations() -> None:
    """The owl-time profile downcasts each Allen relation to OWL-Time's interval*
    relation, 1:1 — and no relation bleeds across (distinct CONSTRUCT variables)."""
    out = _events_projected("owl-time")
    assert (EX_EVENTS.dawn, TIME.intervalBefore, EX_EVENTS.noon) in out
    assert (EX_EVENTS.conference, TIME.intervalContains, EX_EVENTS.keynote) in out
    # dawn→noon is ONLY intervalBefore, not all 13 relations (no var aliasing).
    dawn_noon = {p for _, p, o in out if _ == EX_EVENTS.dawn and o == EX_EVENTS.noon}
    assert dawn_noon == {TIME.intervalBefore}


# --------------------------------------------------------------------------- #
# ObservationalActivity and observation linkage (#128).
# --------------------------------------------------------------------------- #


def test_observational_activity_is_subclass_of_activity_and_event() -> None:
    g = _graph()
    assert (GM.ObservationalActivity, RDF.type, OWL.Class) in g
    assert (GM.ObservationalActivity, RDFS.subClassOf, GM.Activity) in g
    # Activity is already a subclass of Event, so transitively
    # ObservationalActivity ⊑ Event.
    assert (GM.Activity, RDFS.subClassOf, GM.Event) in g


def test_observational_activity_is_not_observation_subclass() -> None:
    """ObservationalActivity is an Event/Activity, NOT an Observation subclass
    (the name sounds similar but the taxonomic placement is the event stack)."""
    g = _graph()
    assert (GM.ObservationalActivity, RDFS.subClassOf, GM.Observation) not in g


def test_generated_observation_exists_and_is_dl_regular() -> None:
    g = _graph()
    assert (GM.generatedObservation, RDF.type, OWL.ObjectProperty) in g
    assert (GM.generatedObservation, RDFS.domain, GM.ObservationalActivity) in g
    assert (GM.generatedObservation, RDFS.range, GM.Observation) in g
    # DL regularity: must be a simple property (no transitivity, symmetry,
    # functionality).
    assert (GM.generatedObservation, RDF.type, OWL.TransitiveProperty) not in g
    assert (GM.generatedObservation, RDF.type, OWL.SymmetricProperty) not in g
    assert (GM.generatedObservation, RDF.type, OWL.FunctionalProperty) not in g


def test_event_observation_is_inverse_of_observation_event() -> None:
    g = _graph()
    assert (GM.eventObservation, RDF.type, OWL.ObjectProperty) in g
    assert (GM.eventObservation, RDFS.domain, GM.Event) in g
    assert (GM.eventObservation, RDFS.range, GM.Observation) in g
    assert (GM.eventObservation, OWL.inverseOf, GM.observationEvent) in g or (
        GM.observationEvent,
        OWL.inverseOf,
        GM.eventObservation,
    ) in g
    assert (GM.eventObservation, RDF.type, OWL.FunctionalProperty) not in g


def test_observational_event_types_are_value_individuals() -> None:
    """The observational event types added in #128 are EventType VALUE individuals,
    never classes — the same anti-subclass guard as the former genealogy types."""
    g = _graph()
    for local in (
        "eventTypeCensusActivity",
        "eventTypeSurvey",
        "eventTypeExcavation",
        "eventTypeAudit",
        "eventTypeClinicalTrial",
    ):
        node = URIRef(GMEOW + local)
        assert (node, RDF.type, GM.EventType) in g, (
            f"{local} must be an EventType value"
        )
        assert (node, RDF.type, OWL.Class) not in g, f"{local} must not be a class"


def test_census_event_type_still_exists() -> None:
    """eventTypeCensus existed before #128 and must still be present."""
    g = _graph()
    assert (GM.eventTypeCensus, RDF.type, GM.EventType) in g


def test_observational_activity_chain_on_was_associated_with() -> None:
    """The DL-regular property chain generatedObservation ∘ vantage ⊑ wasAssociatedWith
    is present in the ontology with the exact ordered sequence."""
    g = _graph()
    chains = list(g.objects(GM.wasAssociatedWith, OWL.propertyChainAxiom))
    assert chains, "wasAssociatedWith must have at least one property chain axiom"
    found = False
    for chain_node in chains:
        members = list(g.items(chain_node))
        if (
            len(members) == 2
            and members[0] == GM.generatedObservation
            and members[1] == GM.vantage
        ):
            found = True
            break
    assert found, (
        "wasAssociatedWith must have a chain containing "
        "generatedObservation ∘ vantage in that order"
    )


def test_no_primary_preferred_observation_term() -> None:
    """Principle 9: no primary/preferred selector for observations or activities,
    neither as properties nor as classes."""
    g = _graph()
    prop_types = (OWL.ObjectProperty, OWL.DatatypeProperty, OWL.AnnotationProperty)
    for banned in (
        "primaryObservation",
        "preferredObservation",
        "primaryActivity",
        "preferredActivity",
    ):
        node = URIRef(GMEOW + banned)
        for pt in prop_types:
            assert (node, RDF.type, pt) not in g, f"{banned} must not exist"
        assert (node, RDF.type, OWL.Class) not in g, f"{banned} must not exist"
