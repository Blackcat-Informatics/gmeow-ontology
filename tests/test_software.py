"""Structural + behavioural guards for the software module (#231, Phase A).

The five-facet de-conflation: Project ≠ Product ≠ Codebase ≠ Repository ≠ History.
Each facet is separately classed, never bridged by subclassing or equivalence.
Contributions are reified relators (reuse creative-works.ttl Contribution).
AI agents are first-class contributors (Principle 9).
"""

from __future__ import annotations

from itertools import combinations
from pathlib import Path

from rdflib import OWL, RDF, RDFS, Graph, Namespace

from gmeow_tools.graph import load_merged_graph
from gmeow_tools.validate import run_shacl

GMEOW = "https://blackcatinformatics.ca/gmeow/"
GM = Namespace(GMEOW)
GUFO = Namespace("http://purl.org/nemo/gufo#")
EX = Namespace("https://example.org/software/")
SOFTWARE_FIXTURE = Path(__file__).parent / "fixtures" / "software.ttl"

FACETS = [
    GM.Project,
    GM.SoftwareProduct,
    GM.SourceTree,
    GM.Repository,
    GM.Commit,
    GM.Release,
]


def _graph() -> Graph:
    return load_merged_graph(include_imports=False)


def _fixture() -> Graph:
    return Graph().parse(SOFTWARE_FIXTURE, format="turtle")


# --------------------------------------------------------------------------- #
# gUFO grounding
# --------------------------------------------------------------------------- #


def test_software_project_is_gufo_grounded() -> None:
    g = _graph()
    assert (GM.Project, RDF.type, OWL.Class) in g
    assert (GM.Project, RDFS.subClassOf, GM.Entity) in g
    assert (GM.SoftwareProject, RDFS.subClassOf, GM.Project) in g
    assert (GM.SoftwareProduct, RDFS.subClassOf, GM.Work) in g
    assert (GM.Commit, RDFS.subClassOf, GM.Activity) in g
    assert (GM.Release, RDFS.subClassOf, GM.Event) in g
    assert (GM.Repository, RDFS.subClassOf, GM.InformationObject) in g


def test_software_value_vocabs_are_quality_values() -> None:
    g = _graph()
    for cls in [GM.RepositoryType, GM.MaintenanceStatus, GM.GovernanceModel]:
        assert (cls, RDFS.subClassOf, GUFO.QualityValue) in g


# --------------------------------------------------------------------------- #
# Five-facet orthogonality guard (centrepiece)
# --------------------------------------------------------------------------- #


def test_no_subclass_bridge_between_facets() -> None:
    """The six facet classes are never bridged by rdfs:subClassOf or
    owl:equivalentClass (Principle 9 — no overtyping)."""
    g = _graph()
    for a, b in combinations(FACETS, 2):
        assert (a, RDFS.subClassOf, b) not in g, f"{a} must not be a subclass of {b}"
        assert (b, RDFS.subClassOf, a) not in g, f"{b} must not be a subclass of {a}"
        assert (a, OWL.equivalentClass, b) not in g, (
            f"{a} must not be equivalent to {b}"
        )
        assert (b, OWL.equivalentClass, a) not in g, (
            f"{b} must not be equivalent to {a}"
        )


def test_facet_orthogonality_shacl_rejects_two_facets() -> None:
    """The closed-world dual: an individual typed in two facet classes is
    rejected by SHACL without a reasoner."""
    bad = Graph()
    bad.add((EX.x, RDF.type, GM.Project))
    bad.add((EX.x, RDF.type, GM.SoftwareProduct))
    result = run_shacl(bad)
    assert not result.ok
    assert "may fill at most one software facet" in "\n".join(result.errors)


# --------------------------------------------------------------------------- #
# Provenance: parentCommit ⊑ wasDerivedFrom
# --------------------------------------------------------------------------- #


def test_parent_commit_is_derived_from_subproperty() -> None:
    g = _graph()
    assert (GM.parentCommit, RDFS.subPropertyOf, GM.wasDerivedFrom) in g


# --------------------------------------------------------------------------- #
# Fixture: MeowGraph
# --------------------------------------------------------------------------- #


def test_fixture_parses_and_shacl_passes() -> None:
    g = _fixture()
    result = run_shacl(g)
    assert result.ok, "\n".join(result.errors)


def test_fixture_has_all_five_facets() -> None:
    g = _fixture()
    assert (EX.meowgraph, RDF.type, GM.SoftwareProject) in g
    assert (EX.meowgraphProduct, RDF.type, GM.SoftwareProduct) in g
    assert (EX.treeInitial, RDF.type, GM.SourceTree) in g
    assert (EX.repo, RDF.type, GM.Repository) in g
    assert (EX.commitInitial, RDF.type, GM.Commit) in g
    assert (EX.v1_0_0, RDF.type, GM.Release) in g


def test_fixture_commit_has_content_digest() -> None:
    g = _fixture()
    assert (EX.commitInitial, GM.contentDigest, None) in g


def test_fixture_ai_contributor_is_first_class() -> None:
    """AI agents are SoftwareAgents with attributed Contribution relators
    (Principle 9 — co-equal facets, never ground truth)."""
    g = _fixture()
    assert (EX.copilot, RDF.type, GM.SoftwareAgent) in g
    contributions = set(g.subjects(GM.contributor, EX.copilot))
    assert contributions
    for contrib in contributions:
        assert (contrib, GM.contributionRole, GM.roleAIAssistant) in g


def test_fixture_contribution_reifies_role_and_degree() -> None:
    g = _fixture()
    assert (EX.contribAlice, GM.contributionRole, GM.roleSoftwareMaintainer) in g
    assert (EX.contribAlice, GM.contributionDegree, GM.degreeLead) in g
    assert (EX.contribAlice, GM.contributionTarget, EX.meowgraphProduct) in g


# --------------------------------------------------------------------------- #
# Software-specific seeds
# --------------------------------------------------------------------------- #


def test_software_contribution_roles_seeded() -> None:
    g = _graph()
    expected = {
        GM.roleSoftwareMaintainer,
        GM.roleSoftwareDeveloper,
        GM.roleCodeReviewer,
        GM.roleReleaser,
        GM.roleSecurityContact,
        GM.roleBotContributor,
        GM.roleAIAssistant,
    }
    actual = set(g.subjects(RDF.type, GM.ContributionRole))
    assert expected <= actual, (
        f"Missing software ContributionRole seeds: {expected - actual}"
    )


def test_software_event_types_seeded() -> None:
    g = _graph()
    expected = {
        GM.eventTypeCommit,
        GM.eventTypeRelease,
        GM.eventTypePush,
        GM.eventTypeMerge,
        GM.eventTypeCodeReview,
    }
    actual = set(g.subjects(RDF.type, GM.EventType))
    assert expected <= actual, f"Missing software EventType seeds: {expected - actual}"
