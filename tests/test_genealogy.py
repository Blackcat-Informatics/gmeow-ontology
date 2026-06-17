"""Structural + standpoint guards for the genealogy module.

The genealogy slice consumes the #43 standpoint facility for contested facts:
disputed parentage, conflicting birth/death dates, competing civil vs parish records.
No genealogy-specific dispute mechanism is minted (Principle 4, P9).
"""

from __future__ import annotations

from pathlib import Path

from rdflib import Graph, Literal, Namespace, URIRef
from rdflib.namespace import OWL, RDF, RDFS

from gmeow_tools.graph import load_merged_graph
from tests._graph_nt import run_shacl

GMEOW = "https://blackcatinformatics.ca/gmeow/"
GM = Namespace(GMEOW)
GUFO = Namespace("http://purl.org/nemo/gufo#")
EX_GENEALOGY = Namespace("https://blackcatinformatics.ca/gmeow/examples/genealogy/")
COVERAGE_FIXTURES = Path(__file__).parent / "fixtures" / "coverage"


def _graph() -> Graph:
    return load_merged_graph(include_imports=False)


# --------------------------------------------------------------------------- #
# gUFO grounding
# --------------------------------------------------------------------------- #


def test_genealogy_is_gufo_grounded() -> None:
    g = _graph()
    assert (GM.Family, RDF.type, OWL.Class) in g
    assert (GM.Family, RDFS.subClassOf, GM.Group) in g
    assert (GM.KinRelationship, RDFS.subClassOf, GUFO.Relator) in g
    assert (GM.ParentChildRelationship, RDFS.subClassOf, GM.KinRelationship) in g


# --------------------------------------------------------------------------- #
# Regression guard — former event subclasses live in the events module
# --------------------------------------------------------------------------- #


def test_former_event_subclasses_are_not_reintroduced() -> None:
    """The ~30 LifeEvent subclasses became gmeow:eventType value individuals in
    the events module; genealogy must not re-introduce them as classes."""
    g = _graph()
    for local in ("Birth", "Death", "Marriage", "Adoption", "Christening"):
        cls = URIRef(GMEOW + local)
        assert (cls, RDF.type, OWL.Class) not in g, f"{local} must not be a class"


# --------------------------------------------------------------------------- #
# Standpoint coexistence — contested parentage / birth dates (#51)
# --------------------------------------------------------------------------- #


def test_contested_parentage_coexists() -> None:
    """Two contradictory standpoint-indexed hasParent claims load, SHACL-pass,
    and are BOTH retained — neither is the ground truth."""
    g = Graph().parse(COVERAGE_FIXTURES / "genealogy-contested.ttl", format="turtle")
    result = run_shacl(g)
    assert result.ok, "\n".join(result.errors)
    parents = set(g.objects(EX_GENEALOGY.child, GM.hasParent))
    assert {EX_GENEALOGY.civilFather, EX_GENEALOGY.parishFather} <= parents


def test_contested_birth_date_coexists() -> None:
    """Two standpoint-indexed eventTime claims on the same LifeEvent coexist."""
    g = Graph().parse(COVERAGE_FIXTURES / "genealogy-contested.ttl", format="turtle")
    result = run_shacl(g)
    assert result.ok, "\n".join(result.errors)
    dates = set(g.objects(EX_GENEALOGY.childBirth, GM.eventTime))
    assert len(dates) == 2


def test_withdrawn_parentage_suppressed_not_deleted() -> None:
    """A refuted / withdrawn parentage claim is retained with displayable false
    (Principle 10 — suppression, never erasure)."""
    g = Graph().parse(COVERAGE_FIXTURES / "genealogy-contested.ttl", format="turtle")
    result = run_shacl(g)
    assert result.ok, "\n".join(result.errors)
    assert (EX_GENEALOGY.withdrawnClaim, GM.displayable, Literal(False)) in g


def test_no_preferred_or_primary_genealogy_term() -> None:
    """Principle 9: no single slot to win — genealogy mints no preferred/primary
    selector for a contested parent, kinship, or event."""
    g = _graph()
    prop_types = (OWL.ObjectProperty, OWL.DatatypeProperty, OWL.AnnotationProperty)
    for banned in (
        "primaryParent",
        "preferredParent",
        "primaryKinship",
        "preferredKinship",
        "primaryBirth",
        "preferredRank",
    ):
        node = URIRef(GMEOW + banned)
        for pt in prop_types:
            assert (node, RDF.type, pt) not in g, f"{banned} must not exist"
        assert (node, RDF.type, OWL.Class) not in g
