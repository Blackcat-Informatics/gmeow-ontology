"""Standpoint + fixture guards for the organization module.

The TBox structural assertions (gUFO grounding of Role/Membership/Post, the
OrganizationType/SiteType seed individuals, and all 14 properties minted in
issue #258) have been migrated to the slice-resident declarative DSL cell
file at slices/core/organization/tests/structural.ttl (#867).

Retained here (not expressible as module-scoped ASK cells):
  * Standpoint coexistence fixtures (run_shacl over ABox fixture files).
  * The whole-ontology Principle-9 banned-term sweep (uses the full merged
    graph; narrowing to scopeModule would miss cross-slice violations).
  * SHACL Warning text assertions (post/org mismatch).
  * SHACL Violation text assertions (legal-identity malformed fixture).
  * Graph-manipulation fixture tests (wellformed legal-identity).
  * Cross-slice EventType seeds (eventTypeMerger/Split/... live in
    slices/core/events/module.ttl, not here).
"""

from __future__ import annotations

from pathlib import Path

from gmeow_rdf.compat.rdflib import Graph, Literal, Namespace, URIRef
from gmeow_rdf.compat.rdflib.namespace import OWL, RDF

from gmeow_tools.graph import load_merged_graph
from tests._graph_nt import run_shacl

GMEOW = "https://blackcatinformatics.ca/gmeow/"
GM = Namespace(GMEOW)
EX_ORGS = Namespace("https://blackcatinformatics.ca/gmeow/examples/organizations/")
COVERAGE_FIXTURES = Path(__file__).parent / "fixtures" / "coverage"


def _graph() -> Graph:
    return load_merged_graph(include_imports=False)


# --------------------------------------------------------------------------- #
# Standpoint coexistence -- contested membership / succession (#51)
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


# --------------------------------------------------------------------------- #
# Post — seat independent of holder (issue #258)
# --------------------------------------------------------------------------- #


def test_post_seat_independent_of_holder() -> None:
    """A Post exists without any Membership filling it — the vacancy case."""
    g = Graph().parse(COVERAGE_FIXTURES / "organization-posts.ttl", format="turtle")
    result = run_shacl(g)
    assert result.ok, "\n".join(result.errors)
    assert (EX_ORGS.cfoPost, RDF.type, GM.Post) in g
    # No membership fills the CFO post.
    assert set(g.objects(None, GM.fillsPost)) & {EX_ORGS.cfoPost} == set()


def test_post_successive_holders() -> None:
    """Two Memberships may fill the same Post in succession."""
    g = Graph().parse(COVERAGE_FIXTURES / "organization-posts.ttl", format="turtle")
    result = run_shacl(g)
    assert result.ok, "\n".join(result.errors)
    holders = set(g.subjects(GM.fillsPost, EX_ORGS.ceoPost))
    assert holders == {EX_ORGS.aliceMembership, EX_ORGS.bobMembership}


def test_membership_fills_post_org_mismatch_warns() -> None:
    """A Membership filling a Post in a different org triggers a SHACL Warning."""
    g = Graph().parse(COVERAGE_FIXTURES / "organization-posts.ttl", format="turtle")
    result = run_shacl(g)
    # Warnings do not fail validation (result.ok stays true).
    assert result.ok, "\n".join(result.errors)
    report = "\n".join(result.errors + result.warnings)
    assert "fills a Post whose organization differs" in report


# --------------------------------------------------------------------------- #
# Site — organizational location (issue #258)
# --------------------------------------------------------------------------- #


def test_site_location() -> None:
    """An organization has sites with typed locations."""
    g = Graph().parse(COVERAGE_FIXTURES / "organization-sites.ttl", format="turtle")
    result = run_shacl(g)
    assert result.ok, "\n".join(result.errors)
    sites = set(g.objects(EX_ORGS.acme, GM.hasSite))
    assert sites == {EX_ORGS.hqBuilding, EX_ORGS.branchOffice}
    assert (EX_ORGS.hqBuilding, GM.siteType, GM.siteTypeHeadquarters) in g
    assert (EX_ORGS.branchOffice, GM.siteType, GM.siteTypeBranch) in g


# --------------------------------------------------------------------------- #
# Multi-organization change events (issue #258)
# --------------------------------------------------------------------------- #


def test_change_event_entailments() -> None:
    """Merger and split events link predecessor and successor organizations."""
    g = Graph().parse(
        COVERAGE_FIXTURES / "organization-change-events.ttl", format="turtle"
    )
    result = run_shacl(g)
    assert result.ok, "\n".join(result.errors)
    # Merger: 2 predecessors, 1 successor.
    preds = set(g.objects(EX_ORGS.mergerEvent, GM.predecessorOrganization))
    assert preds == {EX_ORGS.acquiredCo, EX_ORGS.acquirerCo}
    succs = set(g.objects(EX_ORGS.mergerEvent, GM.successorOrganization))
    assert succs == {EX_ORGS.mergedEntity}
    # Split: 1 predecessor, 2 successors.
    split_preds = set(g.objects(EX_ORGS.splitEvent, GM.predecessorOrganization))
    assert split_preds == {EX_ORGS.parentCo}
    split_succs = set(g.objects(EX_ORGS.splitEvent, GM.successorOrganization))
    assert split_succs == {EX_ORGS.spinOffA, EX_ORGS.spinOffB}


# --------------------------------------------------------------------------- #
# Legal identity (issue #258)
# --------------------------------------------------------------------------- #


def test_legal_identifier_requires_scheme() -> None:
    """An Identifier node without identifierScheme triggers a SHACL Violation."""
    g = Graph().parse(
        COVERAGE_FIXTURES / "organization-legal-identity.ttl", format="turtle"
    )
    result = run_shacl(g)
    assert not result.ok, "malformed legal-identity graph must fail validation"
    report = "\n".join(result.errors)
    assert "must declare a gmeow:identifierScheme" in report


def test_wellformed_legal_identifier_passes() -> None:
    """An organization with reified Identifier nodes (value + scheme) passes."""
    g = Graph().parse(
        COVERAGE_FIXTURES / "organization-legal-identity.ttl", format="turtle"
    )
    # Remove the malformed organization and its Identifier node.
    g.remove((EX_ORGS.badCo, None, None))
    g.remove((EX_ORGS.badCoId, None, None))
    result = run_shacl(g)
    assert result.ok, "\n".join(result.errors)
    # The reified structure: acme -> legalIdentifier -> Identifier node.
    id_nodes = set(g.objects(EX_ORGS.acme, GM.legalIdentifier))
    assert len(id_nodes) == 1
    id_node = id_nodes.pop()
    assert (id_node, GM.identifierValue, Literal("ROR-ABCDE")) in g
    assert (id_node, GM.identifierScheme, Literal("ror")) in g


# --------------------------------------------------------------------------- #
# Change event type values (issue #258) -- RETAINED: cross-slice
# eventTypeMerger/Split/SpinOff/Acquisition/Rename are defined in
# slices/core/events/module.ttl, not here; scopeModule would miss them.
# --------------------------------------------------------------------------- #


def test_change_event_type_values_exist() -> None:
    """The multi-org change event type vocabulary is seeded (cross-slice)."""
    g = _graph()
    expected = (
        "eventTypeMerger",
        "eventTypeSplit",
        "eventTypeSpinOff",
        "eventTypeAcquisition",
        "eventTypeRename",
    )
    for val in expected:
        node = URIRef(GMEOW + val)
        assert (node, RDF.type, GM.EventType) in g, f"{val} must exist"
