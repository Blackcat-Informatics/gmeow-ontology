"""The universal lifecycle facility (#81) — retained dynamic tests.

Structural TBox invariants for the lifecycle module are expressed as declarative
slicetest cells in slices/core/lifecycle/tests/structural.ttl (#867 migration).
The functions below are RETAINED because they are either:
  - dynamic whole-graph sweeps that cannot be narrowed to scopeModule without
    silent coverage loss (test_no_lifecycle_event_subclasses_exist,
    test_no_preferred_or_primary_lifecycle_term),
  - cross-slice checks whose subjects are defined outside this module
    (test_supersession_properties_are_object_properties → gmeow:supersedes in
    slices/core/coreference; test_lifecycle_event_types_are_individuals_not_classes
    → eventType* individuals in slices/core/events),
  - run_shacl() ExampleConformance checks (test_wellformed_entity_existence_conforms,
    test_malformed_entity_existence_is_flagged),
  - multi-file ABox fixture checks (contested-existence + coverage-fixture).

The asserted-TBox structural assertions were migrated to
slices/core/lifecycle/tests/structural.ttl (5 cells); see
dsl/tests/MIGRATION-LEDGER.md for the per-fn mapping.
                                              → ex:saSupersededByIsObjectProperty
"""

from __future__ import annotations

from pathlib import Path

from gmeow_rdf.compat.rdflib import OWL, RDF, RDFS, Graph, Namespace, URIRef

from gmeow_tools.graph import load_merged_graph
from tests._graph_nt import run_shacl

GMEOW = "https://blackcatinformatics.ca/gmeow/"
GM = Namespace(GMEOW)
EX_LIFE = Namespace("https://blackcatinformatics.ca/gmeow/examples/lifecycle/")
SHAPES_FIXTURES = Path(__file__).parent / "fixtures" / "shapes"
COVERAGE_FIXTURES = Path(__file__).parent / "fixtures" / "coverage"


def _graph() -> Graph:
    return load_merged_graph(include_imports=False)


def _fixture(name: str) -> Graph:
    return Graph().parse(SHAPES_FIXTURES / f"{name}.ttl", format="turtle")


# --------------------------------------------------------------------------- #
# gUFO grounding — supersession properties.
# NOTE: gmeow:EntityExistence (gufo:SituationType + rdfs:subClassOf
# TimeScopedRelation) and the flat object properties (hasCreationEvent,
# hasDestructionEvent, existenceInterval, domain Entity) are asserted as
# structural cells in structural.ttl (ex:saEntityExistenceIsSituationType,
# ex:saFlatLifecyclePropertiesAreObjectProperties).
# --------------------------------------------------------------------------- #


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
# No preferred/primary term (Principle 9) — the lifecycle module mints no
# primary*/preferred* selector.
# NOTE: Flat-first pattern properties (hasCreationEvent, hasDestructionEvent,
# existenceInterval as ObjectProperty) and reified EntityExistence slot set
# (existenceEntity, existenceCreationEvent, existenceDestructionEvent) are
# asserted as structural cells ex:saFlatLifecyclePropertiesAreObjectProperties
# and ex:saReifiedEntityExistenceProperties in structural.ttl.
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
# SHACL well-formedness of the EntityExistence situation.
# NOTE: The relator mediation axiom (someValuesFrom restriction on existenceEntity)
# is asserted as structural cell ex:saEntityExistenceMediationAxiom in structural.ttl.
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
