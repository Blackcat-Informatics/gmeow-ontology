"""Structural + standpoint guards for the organization module.

The organization slice consumes the #43 standpoint facility for contested facts:
disputed membership, rival succession claims, contested recognition.
No organization-specific dispute mechanism is minted (Principle 4, P9).
"""

from __future__ import annotations

from pathlib import Path

from rdflib import Graph, Literal, Namespace, URIRef
from rdflib.namespace import OWL, RDF, RDFS

from gmeow_tools.graph import load_merged_graph
from gmeow_tools.validate import run_shacl

GMEOW = "https://blackcatinformatics.ca/gmeow/"
GM = Namespace(GMEOW)
GUFO = Namespace("http://purl.org/nemo/gufo#")
EX_ORGS = Namespace("https://blackcatinformatics.ca/gmeow/examples/organizations/")
COVERAGE_FIXTURES = Path(__file__).parent / "fixtures" / "coverage"


def _graph() -> Graph:
    return load_merged_graph(include_imports=False)


# --------------------------------------------------------------------------- #
# gUFO grounding
# --------------------------------------------------------------------------- #


def test_organization_is_gufo_grounded() -> None:
    g = _graph()
    assert (GM.Role, RDF.type, OWL.Class) in g
    assert (GM.Role, RDFS.subClassOf, URIRef(GUFO + "FunctionalComplex")) in g
    assert (GM.Membership, RDFS.subClassOf, GUFO.Relator) in g


# --------------------------------------------------------------------------- #
# Standpoint coexistence — contested membership / succession (#51)
# --------------------------------------------------------------------------- #


def test_contested_membership_coexists() -> None:
    """Two contradictory standpoint-indexed memberOf claims load, SHACL-pass,
    and are BOTH retained — neither is the ground truth."""
    g = Graph().parse(COVERAGE_FIXTURES / "organization-contested.ttl", format="turtle")
    result = run_shacl(g)
    assert result.ok, "\n".join(result.errors)
    orgs = set(g.objects(EX_ORGS.member, GM.memberOf))
    assert {EX_ORGS.orgA, EX_ORGS.orgB} <= orgs


def test_contested_succession_coexists() -> None:
    """Two standpoint-indexed subOrganizationOf claims post-merger coexist."""
    g = Graph().parse(COVERAGE_FIXTURES / "organization-contested.ttl", format="turtle")
    result = run_shacl(g)
    assert result.ok, "\n".join(result.errors)
    parents = set(g.objects(EX_ORGS.subsidiary, GM.subOrganizationOf))
    assert {EX_ORGS.mergedCo, EX_ORGS.acquirerCo} <= parents


def test_withdrawn_recognition_suppressed_not_deleted() -> None:
    """A closed StandpointTenure with displayable false is retained (Principle 10)."""
    g = Graph().parse(COVERAGE_FIXTURES / "organization-contested.ttl", format="turtle")
    result = run_shacl(g)
    assert result.ok, "\n".join(result.errors)
    assert (EX_ORGS.withdrawnRecognition, GM.displayable, Literal(False)) in g


def test_no_preferred_or_primary_org_term() -> None:
    """Principle 9: no single slot to win — organizations mints no preferred/primary
    selector for a contested member, successor, or recognition."""
    g = _graph()
    prop_types = (OWL.ObjectProperty, OWL.DatatypeProperty, OWL.AnnotationProperty)
    for banned in (
        "primaryMember",
        "preferredMember",
        "primarySuccessor",
        "preferredSuccessor",
        "primaryRecognition",
        "preferredRank",
    ):
        node = URIRef(GMEOW + banned)
        for pt in prop_types:
            assert (node, RDF.type, pt) not in g, f"{banned} must not exist"
        assert (node, RDF.type, OWL.Class) not in g
