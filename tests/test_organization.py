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
from tests._graph_nt import run_shacl

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


# --------------------------------------------------------------------------- #
# Post — seat independent of holder (issue #258)
# --------------------------------------------------------------------------- #


def test_post_is_gufo_role_mixin() -> None:
    g = _graph()
    assert (GM.Post, RDF.type, OWL.Class) in g
    assert (GM.Post, RDF.type, URIRef(GUFO + "RoleMixin")) in g
    assert (GM.Post, RDFS.subClassOf, URIRef(GUFO + "FunctionalComplex")) in g


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
# Organization type values (issue #258)
# --------------------------------------------------------------------------- #


def test_organization_type_values_exist() -> None:
    """The organization type vocabulary is seeded with expected individuals."""
    g = _graph()
    expected = (
        "organizationTypeCompany",
        "organizationTypeNonprofit",
        "organizationTypeGovernmentBody",
        "organizationTypeEducationalInstitution",
        "organizationTypeAssociation",
        "organizationTypeCollaboration",
    )
    for val in expected:
        node = URIRef(GMEOW + val)
        assert (node, RDF.type, GM.OrganizationType) in g, f"{val} must exist"


def test_site_type_values_exist() -> None:
    """The site type vocabulary is seeded with expected individuals."""
    g = _graph()
    expected = ("siteTypeHeadquarters", "siteTypeBranch", "siteTypeRegistered")
    for val in expected:
        node = URIRef(GMEOW + val)
        assert (node, RDF.type, GM.SiteType) in g, f"{val} must exist"


# --------------------------------------------------------------------------- #
# Change event type values (issue #258)
# --------------------------------------------------------------------------- #


def test_change_event_type_values_exist() -> None:
    """The multi-org change event type vocabulary is seeded."""
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


# --------------------------------------------------------------------------- #
# New properties exist in the TBox (issue #258)
# --------------------------------------------------------------------------- #


def test_new_organization_properties_exist() -> None:
    """Every new property minted in #258 is present in the merged graph."""
    g = _graph()
    for prop_name, expected_type in (
        ("organizationType", OWL.ObjectProperty),
        ("postIn", OWL.ObjectProperty),
        ("fillsPost", OWL.ObjectProperty),
        ("hasSite", OWL.ObjectProperty),
        ("siteType", OWL.ObjectProperty),
        ("predecessorOrganization", OWL.ObjectProperty),
        ("successorOrganization", OWL.ObjectProperty),
        ("hasIdentifier", OWL.ObjectProperty),
        ("legalIdentifier", OWL.ObjectProperty),
        ("industryClassification", OWL.ObjectProperty),
        ("identifierValue", OWL.DatatypeProperty),
        ("identifierScheme", OWL.DatatypeProperty),
        ("jurisdiction", OWL.ObjectProperty),
        ("organizationPurpose", OWL.DatatypeProperty),
    ):
        node = URIRef(GMEOW + prop_name)
        assert (node, RDF.type, expected_type) in g, (
            f"{prop_name} must be a {expected_type}"
        )
