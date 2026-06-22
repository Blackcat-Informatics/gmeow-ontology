"""The universal lifecycle facility (#81).

GMEOW had no universal existence-over-time model: birth/death were person-scoped
life events, founding/dissolution were organization-scoped schema.org fields, and
places had no lifecycle at all. This module mints universal flat properties
(hasCreationEvent, hasDestructionEvent, existenceInterval, supersededBy/supersedes)
and a reified EntityExistence TimeScopedRelation for contested or evidence-bearing
claims. It absorbs the person/org-specific forms forward (Principle 6).

The centerpiece is the anti-subclass regression guard: creation, destruction,
supersession and dissolution are EventType VALUE individuals, never classes.
"""

from __future__ import annotations

from pathlib import Path

from gmeow_rdf.compat.rdflib import OWL, RDF, RDFS, Graph, Namespace, URIRef

from gmeow_tools.graph import load_merged_graph
from tests._graph_nt import run_shacl

GMEOW = "https://blackcatinformatics.ca/gmeow/"
GM = Namespace(GMEOW)
GUFO = Namespace("http://purl.org/nemo/gufo#")
EX_LIFE = Namespace("https://blackcatinformatics.ca/gmeow/examples/lifecycle/")
SHAPES_FIXTURES = Path(__file__).parent / "fixtures" / "shapes"
COVERAGE_FIXTURES = Path(__file__).parent / "fixtures" / "coverage"


def _graph() -> Graph:
    return load_merged_graph(include_imports=False)


def _fixture(name: str) -> Graph:
    return Graph().parse(SHAPES_FIXTURES / f"{name}.ttl", format="turtle")


# --------------------------------------------------------------------------- #
# gUFO grounding — EntityExistence is a Situation; flat properties are object
# properties.
# --------------------------------------------------------------------------- #


def test_entity_existence_is_a_gufo_situation() -> None:
    g = _graph()
    assert (GM.EntityExistence, RDF.type, OWL.Class) in g
    assert (GM.EntityExistence, RDF.type, GUFO.SituationType) in g
    assert (GM.EntityExistence, RDFS.subClassOf, GM.TimeScopedRelation) in g


def test_creation_and_destruction_are_object_properties() -> None:
    g = _graph()
    for prop in ("hasCreationEvent", "hasDestructionEvent", "existenceInterval"):
        node = URIRef(GMEOW + prop)
        assert (node, RDF.type, OWL.ObjectProperty) in g, (
            f"{prop} must be an ObjectProperty"
        )
        assert (node, RDFS.domain, GM.Entity) in g, f"{prop} domain must be Entity"


def test_supersession_properties_are_object_properties() -> None:
    g = _graph()
    assert (GM.supersededBy, RDF.type, OWL.ObjectProperty) in g
    assert (GM.supersedes, RDF.type, OWL.ObjectProperty) in g
    assert (GM.supersedes, OWL.inverseOf, GM.supersededBy) in g or (
        GM.supersededBy,
        OWL.inverseOf,
        GM.supersedes,
    ) in g
    assert (GM.supersededBy, RDFS.domain, GM.Entity) in g
    assert (GM.supersededBy, RDFS.range, GM.Entity) in g


# --------------------------------------------------------------------------- #
# THE CENTREPIECE — anti-subclass / anti-overtyping regression guard. Lifecycle
# event kinds are VALUE individuals (EventType), never classes.
# --------------------------------------------------------------------------- #

_LIFECYCLE_EVENT_TYPES = (
    "eventTypeCreation",
    "eventTypeDestruction",
    "eventTypeSupersession",
    "eventTypeDissolution",
)


def test_lifecycle_event_types_are_individuals_not_classes() -> None:
    """Creation, destruction, supersession and dissolution are gmeow:EventType
    VALUE individuals, never classes — the permanent anti-overtyping lock."""
    g = _graph()
    for local in _LIFECYCLE_EVENT_TYPES:
        node = URIRef(GMEOW + local)
        assert (node, RDF.type, GM.EventType) in g, (
            f"{local} must be an EventType value"
        )
        assert (node, RDF.type, OWL.Class) not in g, f"{local} must not be a class"


def test_no_lifecycle_event_subclasses_exist() -> None:
    """No CreationEvent / DestructionEvent / SupersessionEvent classes are
    introduced — the universal gmeow:Event + eventType value vocabulary is the
    single canonical pattern."""
    g = _graph()
    banned = (
        "CreationEvent",
        "DestructionEvent",
        "SupersessionEvent",
        "DissolutionEvent",
    )
    for local in banned:
        node = URIRef(GMEOW + local)
        assert (node, RDF.type, OWL.Class) not in g, (
            f"{local} must not exist as a class"
        )


# --------------------------------------------------------------------------- #
# Flat-first pattern — both flat shortcuts and reified EntityExistence coexist.
# --------------------------------------------------------------------------- #


def test_flat_lifecycle_properties_exist() -> None:
    g = _graph()
    for prop in ("hasCreationEvent", "hasDestructionEvent", "existenceInterval"):
        node = URIRef(GMEOW + prop)
        assert (node, RDF.type, OWL.ObjectProperty) in g


def test_reified_entity_existence_exists() -> None:
    g = _graph()
    assert (GM.EntityExistence, RDF.type, OWL.Class) in g
    assert (GM.existenceEntity, RDF.type, OWL.ObjectProperty) in g
    assert (GM.existenceCreationEvent, RDF.type, OWL.ObjectProperty) in g
    assert (GM.existenceDestructionEvent, RDF.type, OWL.ObjectProperty) in g


# --------------------------------------------------------------------------- #
# No preferred/primary term (Principle 9) — the lifecycle module mints no
# primary*/preferred* selector.
# --------------------------------------------------------------------------- #


def test_no_preferred_or_primary_lifecycle_term() -> None:
    g = _graph()
    prop_types = (OWL.ObjectProperty, OWL.DatatypeProperty, OWL.AnnotationProperty)
    for banned in (
        "primaryCreationEvent",
        "preferredCreationEvent",
        "primaryDestructionEvent",
        "preferredDestructionEvent",
        "primaryExistenceInterval",
        "preferredExistenceInterval",
        "preferredRank",
    ):
        node = URIRef(GMEOW + banned)
        for pt in prop_types:
            assert (node, RDF.type, pt) not in g, f"{banned} must not exist"
        assert (node, RDF.type, OWL.Class) not in g


# --------------------------------------------------------------------------- #
# Relator mediation axiom — open-world EL someValuesFrom on EntityExistence.
# --------------------------------------------------------------------------- #


def test_entity_existence_mediation_axiom_present() -> None:
    g = _graph()
    mediated: set[URIRef] = set()
    for restriction in g.objects(GM.EntityExistence, RDFS.subClassOf):
        on = g.value(restriction, OWL.onProperty)
        some = g.value(restriction, OWL.someValuesFrom)
        if isinstance(on, URIRef) and some is not None:
            mediated.add(on)
            if on == GM.existenceEntity:
                assert some == GM.Entity
    assert GM.existenceEntity in mediated


# --------------------------------------------------------------------------- #
# SHACL well-formedness of the EntityExistence situation.
# --------------------------------------------------------------------------- #


def test_wellformed_entity_existence_conforms() -> None:
    result = run_shacl(_fixture("entity-existence-wellformed"))
    assert result.ok, "\n".join(result.errors)


def test_malformed_entity_existence_is_flagged() -> None:
    result = run_shacl(_fixture("entity-existence-malformed"))
    assert not result.ok
    joined = "\n".join(result.errors)
    assert "existenceEntity" in joined and "duringInterval" in joined


# --------------------------------------------------------------------------- #
# Standpoint coexistence — contested existence claims COEXIST, none privileged.
# --------------------------------------------------------------------------- #


def test_contested_existence_claims_coexist_and_validate() -> None:
    """Two contradictory standpoint-indexed existence intervals for the same
    entity load, SHACL-pass, and are BOTH retained — neither is the ground truth."""
    g = Graph().parse(COVERAGE_FIXTURES / "lifecycle-contested.ttl", format="turtle")
    result = run_shacl(g)
    assert result.ok, "\n".join(result.errors)
    # Two existence records for the same entity, each with a different interval.
    existences = set(g.subjects(RDF.type, GM.EntityExistence))
    assert len(existences) == 2


# --------------------------------------------------------------------------- #
# Coverage fixture — supersession, ceased place, dissolved org.
# --------------------------------------------------------------------------- #


def test_coverage_fixture_loads_and_validates() -> None:
    g = Graph().parse(COVERAGE_FIXTURES / "lifecycle.ttl", format="turtle")
    result = run_shacl(g)
    assert result.ok, "\n".join(result.errors)
    # The medieval village has a destruction event.
    assert (
        EX_LIFE.medievalVillage,
        GM.hasDestructionEvent,
        EX_LIFE.villageDestroyed,
    ) in g
    # The old company is superseded by the new company.
    assert (EX_LIFE.oldCompany, GM.supersededBy, EX_LIFE.newCompany) in g
    assert (EX_LIFE.newCompany, GM.supersedes, EX_LIFE.oldCompany) in g
