"""Standpoint guards for the employment module (retained pytest subset).

Tests for asserted-TBox invariants that are local to the employment module have
been migrated to slices/extensions/employment/tests/structural.ttl as declarative
gmeow:StructuralAssertion cells and are no longer here (#867).

RETAINED here (not migratable as scopeModule cells):
  test_employment_is_gufo_grounded — cross-slice triple
    (gmeow:Membership rdfs:subClassOf gufo:Relator) lives in the memberships slice.
  test_employment_event_types_are_values — eventTypeHiring etc. are defined in
    slices/core/events/module.ttl; cross-slice subjects.
  test_founded_on_links_relator_to_agreement — gmeow:foundedOn is defined in
    slices/core/agreements/module.ttl; cross-slice subject.
  test_contested_employment_coexists — run_shacl() against an external fixture.
  test_withdrawn_employment_suppressed_not_deleted — run_shacl() + ABox check.
  test_no_preferred_or_primary_employment_term — whole-graph dynamic sweep.
"""

from __future__ import annotations

from pathlib import Path

from gmeow_rdf.compat.rdflib import Graph, Literal, Namespace, URIRef
from gmeow_rdf.compat.rdflib.namespace import OWL, RDF, RDFS

from gmeow_tools.graph import load_merged_graph
from tests._graph_nt import run_shacl

GMEOW = "https://blackcatinformatics.ca/gmeow/"
GM = Namespace(GMEOW)
GUFO = Namespace("http://purl.org/nemo/gufo#")
EX_EMPS = Namespace("https://blackcatinformatics.ca/gmeow/examples/employment/")
COVERAGE_FIXTURES = Path(__file__).parent / "fixtures" / "coverage"


def _graph() -> Graph:
    return load_merged_graph(include_imports=False)


# --------------------------------------------------------------------------- #
# gUFO grounding
# --------------------------------------------------------------------------- #


def test_employment_is_gufo_grounded() -> None:
    g = _graph()
    assert (GM.Employment, RDF.type, OWL.Class) in g
    assert (GM.Employment, RDFS.subClassOf, GM.Membership) in g
    assert (GM.Membership, RDFS.subClassOf, GUFO.Relator) in g


def test_employment_event_types_are_values() -> None:
    """Principle 9: employment events are EventType values, never Event subclasses."""
    g = _graph()
    for evt in (
        GM.eventTypeHiring,
        GM.eventTypePromotion,
        GM.eventTypeTransfer,
        GM.eventTypeResignation,
        GM.eventTypeTermination,
    ):
        assert (evt, RDF.type, GM.EventType) in g
    for banned in ("Hiring", "Promotion", "Transfer", "Resignation", "Termination"):
        node = URIRef(GMEOW + banned)
        msg = f"{banned} must not be an Event subclass"
        assert (node, RDFS.subClassOf, GM.Event) not in g, msg


def test_founded_on_links_relator_to_agreement() -> None:
    g = _graph()
    assert (GM.foundedOn, RDF.type, OWL.ObjectProperty) in g
    assert (GM.foundedOn, RDFS.domain, GUFO.Relator) in g
    assert (GM.foundedOn, RDFS.range, GM.Agreement) in g


# --------------------------------------------------------------------------- #
# Standpoint coexistence — contested employment / role / termination (#51)
# --------------------------------------------------------------------------- #


def test_contested_employment_coexists() -> None:
    """Two contradictory standpoint-indexed employment claims load, SHACL-pass,
    and are BOTH retained — neither is the ground truth."""
    g = Graph().parse(COVERAGE_FIXTURES / "employment-contested.ttl", format="turtle")
    result = run_shacl(g)
    assert result.ok, "\n".join(result.errors)
    orgs = set(g.objects(EX_EMPS.worker, GM.memberOf))
    assert {EX_EMPS.orgA, EX_EMPS.orgB} <= orgs


def test_withdrawn_employment_suppressed_not_deleted() -> None:
    """A closed Employment with displayable false is retained (Principle 10)."""
    g = Graph().parse(COVERAGE_FIXTURES / "employment-contested.ttl", format="turtle")
    result = run_shacl(g)
    assert result.ok, "\n".join(result.errors)
    assert (EX_EMPS.withdrawnEmployment, GM.displayable, Literal(False)) in g


def test_no_preferred_or_primary_employment_term() -> None:
    """Principle 9: no single slot to win — employment mints no preferred/primary
    selector for a contested job, role, or tenure."""
    g = _graph()
    prop_types = (OWL.ObjectProperty, OWL.DatatypeProperty, OWL.AnnotationProperty)
    for banned in (
        "primaryEmployment",
        "preferredEmployment",
        "primaryJob",
        "preferredJob",
        "primaryRole",
        "preferredRole",
    ):
        node = URIRef(GMEOW + banned)
        for pt in prop_types:
            assert (node, RDF.type, pt) not in g, f"{banned} must not exist"
        assert (node, RDF.type, OWL.Class) not in g
