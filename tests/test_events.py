"""The universal events facility (#41).

GMEOW had no universal event: genealogy modelled life events with the exact
double anti-pattern it refuses everywhere else -- type-as-subclass (~30 LifeEvent
subclasses) AND role-as-subproperty (hasPrincipal / hasWitness / hasOfficiant
=< hasParticipant), plus a free-text eventDate. This module mints one gmeow:Event,
reifies participation as the gmeow:Participation relator, makes type and role open
VALUE vocabularies, gives time a real precision story, and absorbs the genealogy
hierarchy forward (Principle 6 -- get it right, never keep the inferior form).

The centrepiece here is the anti-subclass / anti-subproperty REGRESSION GUARD: it
pins the refactor permanently, so the frozen taxonomy can never grow back. See
slices/core/events/module.ttl.

MIGRATED to slices/core/events/tests/structural.ttl (#867):
  test_participation_is_a_gufo_relator
  test_event_type_and_role_are_value_object_properties
  test_former_role_subproperties_are_gone_and_are_values_now
  test_free_text_event_date_and_place_are_gone
  test_event_trajectory_exists_and_ranges_over_trajectory
  test_event_spacetime_exists_and_ranges_over_location_state
  test_event_spacetime_and_trajectory_are_orthogonal
  test_axes_range_over_distinct_spaces
  test_no_axis_is_inferred_from_another
  test_temporal_datatypes_are_datetime_not_date
  test_participant_role_vocabulary_seeded
  test_temporal_precision_vocabulary_seeded
  test_subevent_mereology_is_transitive
  test_no_preferred_or_primary_event_term
  test_allen_relations_exist_on_events
  test_allen_inverses_and_characters
  test_duration_and_recurrence_terms
  test_timeml_tense_and_aspect_value_vocabs
  test_observational_activity_is_not_observation_subclass
  test_generated_observation_exists_and_is_dl_regular
  test_event_observation_is_inverse_of_observation_event
  test_observational_event_types_are_value_individuals
  test_census_event_type_still_exists
  test_no_primary_preferred_observation_term

RETAINED here (not migratable to scopeModule cells):
  test_event_is_grounded_in_gufo_event -- cross-slice: asserts
    gmeow:Activity rdfs:subClassOf gmeow:Event; Activity is defined in the
    provenance slice, so a scopeModule cell would miss that triple.
  test_former_event_types_are_individuals_not_classes -- dynamic sweep:
    uses g.subjects(RDFS.subClassOf, GM.LifeEvent) over the whole merged graph
    to catch any GMEOW-prefixed subclass resurrection; narrowing to the events
    module would silently weaken the regression guard.
  test_participation_mediation_axiom_present -- bnode walk: inspects
    owl:Restriction blank nodes via g.objects() + g.items() to verify
    someValuesFrom axioms; bnode list structure is not expressible as a
    simple module-scoped ASK.
  test_wellformed_participation_conforms -- run_shacl() ExampleConformance.
  test_malformed_participation_is_flagged -- run_shacl() with error-text check.
  test_contested_event_claims_coexist_and_validate -- multi-file ABox fixture
    loaded dynamically + run_shacl() + object sweep.
  test_schema_role_projection_keys_by_role -- project_graph() projection check.
  test_schema_role_projection_suppresses_withdrawn_participation -- projection.
  test_schema_fuzzy_time_projects_earliest_bound -- projection bound check.
  test_ical_vevent_interval_has_start_end_and_location -- projection check.
  test_ical_vevent_point_has_start_only -- projection check.
  test_ical_vevent_fuzzy_spans_the_bounds -- projection bound check.
  test_ical_summary_is_the_event_type_label -- projection label check.
  test_owl_time_projection_emits_pure_interval_relations -- projection sweep.
  test_observational_activity_is_subclass_of_activity_and_event -- cross-slice:
    asserts gmeow:Activity rdfs:subClassOf gmeow:Event (Activity is in provenance).
  test_observational_activity_chain_on_was_associated_with -- bnode list walk:
    inspects owl:propertyChainAxiom blank-node list via g.objects() + g.items()
    to verify the exact member order of generatedObservation + vantage.
"""

from __future__ import annotations

from pathlib import Path

from gmeow_rdf.compat.rdflib import OWL, RDF, RDFS, Graph, Namespace, URIRef

from gmeow_tools.graph import load_merged_graph
from tests._graph_nt import run_shacl

GMEOW = "https://blackcatinformatics.ca/gmeow/"
GM = Namespace(GMEOW)
LOGIC = Namespace("https://blackcatinformatics.ca/logic/")
EX_EVENTS = Namespace("https://blackcatinformatics.ca/gmeow/examples/events/")
SHAPES_FIXTURES = Path(__file__).parent / "fixtures" / "shapes"
COVERAGE_FIXTURES = Path(__file__).parent / "fixtures" / "coverage"


def _graph() -> Graph:
    return load_merged_graph(include_imports=False)


def _fixture(name: str) -> Graph:
    return Graph().parse(SHAPES_FIXTURES / f"{name}.ttl", format="turtle")


# --------------------------------------------------------------------------- #
# gUFO grounding -- Event is a perdurant type; cross-slice Activity assertion.
# --------------------------------------------------------------------------- #


def test_event_is_grounded_in_gufo_event() -> None:
    g = _graph()
    assert (GM.Event, RDF.type, OWL.Class) in g
    assert (GM.Event, RDFS.subClassOf, LOGIC.Event) in g
    # The former top occurrences re-parent onto the universal Event.
    # Activity is defined in the provenance slice (cross-slice assertion --
    # not migratable to a scopeModule cell).
    assert (GM.Activity, RDFS.subClassOf, GM.Event) in g
    assert (GM.LifeEvent, RDFS.subClassOf, GM.Event) in g


# --------------------------------------------------------------------------- #
# THE CENTREPIECE -- anti-subclass regression guard (dynamic g.subjects() sweep).
# This locks the refactor permanently; a scopeModule cell would silently narrow
# the sweep to the events graph only and miss re-introductions in other slices.
# --------------------------------------------------------------------------- #

# The former genealogy event subclasses -- must NOT exist as classes again.
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


def test_former_event_types_are_individuals_not_classes() -> None:
    """The ~30 LifeEvent subclasses became gmeow:eventType VALUE individuals.

    This is the permanent lock: each former type IRI is gone as a class, and its
    replacement is an individual of gmeow:EventType -- not a class, not a subclass.
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
    # gone for good) -- catches ANY accidental re-introduction, not just the known
    # list.
    sub = {
        s for s in g.subjects(RDFS.subClassOf, GM.LifeEvent) if str(s).startswith(GMEOW)
    }
    assert sub == set(), f"gmeow:LifeEvent must have no subclasses; found {sub}"


# --------------------------------------------------------------------------- #
# Relator mediation axiom (#38) -- blank-node restriction walk.
# Not migratable: inspects owl:Restriction bnodes via g.objects() + g.items().
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
# Standpoint coexistence -- multi-file ABox fixture + run_shacl() + object sweep.
# --------------------------------------------------------------------------- #


def test_contested_event_claims_coexist_and_validate() -> None:
    """Two contradictory standpoint-indexed eventType claims (genocide vs armed
    clash) load, SHACL-pass, and are BOTH retained -- neither is the ground truth.
    """
    g = Graph().parse(COVERAGE_FIXTURES / "events-contested.ttl", format="turtle")
    result = run_shacl(g)
    assert result.ok, "\n".join(result.errors)
    types = set(g.objects(EX_EVENTS.disputedEvent, GM.eventType))
    assert {EX_EVENTS.eventTypeGenocide, EX_EVENTS.eventTypeArmedClash} <= types
    # A contested date likewise coexists -- two instants, neither privileged.
    dates = set(g.objects(EX_EVENTS.disputedEvent, GM.eventTime))
    assert len(dates) == 2


# --------------------------------------------------------------------------- #
# Projections -- the reified, multi-modal event downcasts to flat consumer forms.
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
    """The reified Participation downcasts to the role-keyed flat schema.org edges --
    each role lands on its own predicate, not all three together."""
    out = _events_projected("schema-org")
    assert EX_EVENTS.casey in set(out.objects(EX_EVENTS.reception, SCHEMA.organizer))
    assert EX_EVENTS.band in set(out.objects(EX_EVENTS.reception, SCHEMA.performer))
    assert EX_EVENTS.dana in set(out.objects(EX_EVENTS.reception, SCHEMA.attendee))
    # Roles don't bleed across predicates: the organizer is not also an attendee.
    assert EX_EVENTS.casey not in set(out.objects(EX_EVENTS.reception, SCHEMA.attendee))


def test_schema_role_projection_suppresses_withdrawn_participation() -> None:
    """A superseded participation (gmeow:displayable false) is NOT projected -- the
    flat downcast honours suppression-not-erasure (Principle 10)."""
    out = _events_projected("schema-org")
    # ex:erin's attendee participation is displayable false -> must be dropped.
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
    """A circa-dated event becomes a VEVENT spanning earliestStart->latestEnd."""
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


def test_owl_time_projection_emits_pure_interval_relations() -> None:
    """The owl-time profile downcasts each Allen relation to OWL-Time's interval*
    relation, 1:1 -- and no relation bleeds across (distinct CONSTRUCT variables).
    """
    out = _events_projected("owl-time")
    assert (EX_EVENTS.dawn, TIME.intervalBefore, EX_EVENTS.noon) in out
    assert (EX_EVENTS.conference, TIME.intervalContains, EX_EVENTS.keynote) in out
    # dawn->noon is ONLY intervalBefore, not all 13 relations (no var aliasing).
    dawn_noon = {p for _, p, o in out if _ == EX_EVENTS.dawn and o == EX_EVENTS.noon}
    assert dawn_noon == {TIME.intervalBefore}


# --------------------------------------------------------------------------- #
# ObservationalActivity and observation linkage (#128) -- cross-slice assertion.
# The Activity rdfs:subClassOf Event assertion is cross-slice (provenance module).
# --------------------------------------------------------------------------- #


def test_observational_activity_is_subclass_of_activity_and_event() -> None:
    g = _graph()
    assert (GM.ObservationalActivity, RDF.type, OWL.Class) in g
    assert (GM.ObservationalActivity, RDFS.subClassOf, GM.Activity) in g
    # Activity is already a subclass of Event, so transitively
    # ObservationalActivity <= Event. Activity is cross-slice (provenance module).
    assert (GM.Activity, RDFS.subClassOf, GM.Event) in g


# --------------------------------------------------------------------------- #
# Property chain: generatedObservation + vantage -- bnode list walk.
# Not migratable: inspects owl:propertyChainAxiom blank-node list via
# g.objects() + g.items() to verify the exact member order.
# --------------------------------------------------------------------------- #


def test_observational_activity_chain_on_was_associated_with() -> None:
    """The DL-regular property chain generatedObservation o vantage
    =< wasAssociatedWith is present with the exact ordered sequence."""
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
        "generatedObservation o vantage in that order"
    )
