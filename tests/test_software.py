"""Structural + behavioural guards for the software module (#231, Phase A).

The five-facet de-conflation: Project ≠ Product ≠ Codebase ≠ Repository ≠ History.
Each facet is separately classed, never bridged by subclassing or equivalence.
Contributions are reified relators (reuse creative-works.ttl Contribution).
AI agents are first-class contributors (Principle 9).
"""

from __future__ import annotations

from itertools import combinations
from pathlib import Path

from gmeow_rdf.compat.rdflib import OWL, RDF, RDFS, Graph, Literal, Namespace

from gmeow_tools.graph import load_merged_graph
from tests._graph_nt import run_shacl

GMEOW = "https://blackcatinformatics.ca/gmeow/"
GM = Namespace(GMEOW)
GUFO = Namespace("http://purl.org/nemo/gufo#")
XSD = Namespace("http://www.w3.org/2001/XMLSchema#")
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


# --------------------------------------------------------------------------- #
# Phase B — git provenance deepening (#232)
# --------------------------------------------------------------------------- #


def test_new_classes_are_owl_classes_with_correct_parents() -> None:
    g = _graph()
    assert (GM.Blob, RDF.type, OWL.Class) in g
    assert (GM.Blob, RDFS.subClassOf, GM.SourceNode) in g
    assert (GM.TreeEntry, RDF.type, OWL.Class) in g
    assert (GM.TreeEntry, RDFS.subClassOf, GM.InformationObject) in g
    assert (GM.Push, RDF.type, OWL.Class) in g
    assert (GM.Push, RDFS.subClassOf, GM.Activity) in g
    assert (GM.Merge, RDF.type, OWL.Class) in g
    assert (GM.Merge, RDFS.subClassOf, GM.Activity) in g
    assert (GM.CodeReview, RDF.type, OWL.Class) in g
    assert (GM.CodeReview, RDFS.subClassOf, GM.Event) in g
    assert (GM.Diff, RDF.type, OWL.Class) in g
    assert (GM.Diff, RDFS.subClassOf, GM.InformationObject) in g


def test_commit_ancestor_is_transitive_and_derived_from_subproperty() -> None:
    g = _graph()
    assert (GM.commitAncestor, RDF.type, OWL.TransitiveProperty) in g
    assert (GM.commitAncestor, RDFS.subPropertyOf, GM.wasDerivedFrom) in g


def test_parent_commit_is_subproperty_of_commit_ancestor() -> None:
    g = _graph()
    assert (GM.parentCommit, RDFS.subPropertyOf, GM.commitAncestor) in g


def test_commit_descendant_is_inverse_of_commit_ancestor() -> None:
    g = _graph()
    assert (GM.commitDescendant, OWL.inverseOf, GM.commitAncestor) in g


def test_tree_entry_properties_exist() -> None:
    g = _graph()
    assert (GM.treeEntryName, RDFS.domain, GM.TreeEntry) in g
    assert (GM.treeEntryMode, RDFS.domain, GM.TreeEntry) in g
    assert (GM.treeEntryObject, RDFS.domain, GM.TreeEntry) in g
    assert (GM.treeEntryObject, RDFS.range, GM.SourceNode) in g


def test_diff_properties_exist() -> None:
    g = _graph()
    assert (GM.diffFrom, RDFS.domain, GM.Diff) in g
    assert (GM.diffFrom, RDFS.range, GM.Commit) in g
    assert (GM.diffTo, RDFS.domain, GM.Diff) in g
    assert (GM.diffTo, RDFS.range, GM.Commit) in g


def test_push_merge_review_properties_exist() -> None:
    g = _graph()
    assert (GM.pushTarget, RDFS.domain, GM.Push) in g
    assert (GM.mergeBase, RDFS.domain, GM.Merge) in g
    assert (GM.mergeSource, RDFS.domain, GM.Merge) in g
    assert (GM.mergeTarget, RDFS.domain, GM.Merge) in g
    assert (GM.mergeTarget, RDF.type, OWL.FunctionalProperty) in g
    assert (GM.reviewOf, RDFS.domain, GM.CodeReview) in g
    assert (GM.reviewCommit, RDFS.domain, GM.CodeReview) in g


def test_materialization_depth_property_exists() -> None:
    g = _graph()
    assert (GM.materializationDepth, RDFS.domain, GM.Repository) in g
    assert (GM.materializationDepth, RDFS.range, XSD.nonNegativeInteger) in g


# --------------------------------------------------------------------------- #
# Fixture: MeowGraph Phase B — 3-commit DAG, blobs, tree entries, events
# --------------------------------------------------------------------------- #


def test_fixture_has_three_commit_dag() -> None:
    g = _fixture()
    # commitInitial is the root (no parentCommit)
    assert (EX.commitInitial, GM.parentCommit, None) not in g
    # commitFeature has commitInitial as parent
    assert (EX.commitFeature, GM.parentCommit, EX.commitInitial) in g
    # commitMerge has both parents
    assert (EX.commitMerge, GM.parentCommit, EX.commitInitial) in g
    assert (EX.commitMerge, GM.parentCommit, EX.commitFeature) in g


def test_fixture_has_commit_ancestor_closure() -> None:
    g = _fixture()
    # commitFeature commitAncestor commitInitial (direct)
    assert (EX.commitFeature, GM.commitAncestor, EX.commitInitial) in g
    # commitMerge commitAncestor both (direct assertions in fixture)
    assert (EX.commitMerge, GM.commitAncestor, EX.commitInitial) in g
    assert (EX.commitMerge, GM.commitAncestor, EX.commitFeature) in g


def test_fixture_has_blobs_and_tree_entries() -> None:
    g = _fixture()
    assert (EX.readmeBlob, RDF.type, GM.Blob) in g
    assert (EX.mainPyBlob, RDF.type, GM.Blob) in g
    assert (EX.readmeEntry, RDF.type, GM.TreeEntry) in g
    assert (EX.readmeEntry, GM.treeEntryName, None) in g
    assert (EX.readmeEntry, GM.treeEntryMode, None) in g
    assert (EX.readmeEntry, GM.treeEntryObject, EX.readmeBlob) in g
    assert (EX.mainPyEntry, RDF.type, GM.TreeEntry) in g
    assert (EX.mainPyEntry, GM.treeEntryObject, EX.mainPyBlob) in g


def test_fixture_has_push_event() -> None:
    g = _fixture()
    assert (EX.pushFeature, RDF.type, GM.Push) in g
    assert (EX.pushFeature, GM.pushTarget, EX.repo) in g
    assert (EX.pushFeature, GM.eventType, GM.eventTypePush) in g


def test_fixture_has_merge_event() -> None:
    g = _fixture()
    assert (EX.mergeFeature, RDF.type, GM.Merge) in g
    assert (EX.mergeFeature, GM.mergeBase, EX.commitInitial) in g
    assert (EX.mergeFeature, GM.mergeSource, EX.featureBranch) in g
    assert (EX.mergeFeature, GM.mergeTarget, EX.mainBranch) in g
    assert (EX.mergeFeature, GM.eventType, GM.eventTypeMerge) in g


def test_fixture_has_code_review_event() -> None:
    g = _fixture()
    assert (EX.reviewFeature, RDF.type, GM.CodeReview) in g
    assert (EX.reviewFeature, GM.reviewOf, EX.mrFeature) in g
    assert (EX.reviewFeature, GM.reviewCommit, EX.commitFeature) in g
    assert (EX.reviewFeature, GM.eventType, GM.eventTypeCodeReview) in g


def test_fixture_has_diff() -> None:
    g = _fixture()
    assert (EX.diffInitialFeature, RDF.type, GM.Diff) in g
    assert (EX.diffInitialFeature, GM.diffFrom, EX.commitInitial) in g
    assert (EX.diffInitialFeature, GM.diffTo, EX.commitFeature) in g


def test_fixture_repository_has_materialization_depth() -> None:
    g = _fixture()
    vals = list(g.objects(EX.repo, GM.materializationDepth))
    assert len(vals) == 1
    val = Literal(vals[0])
    assert val.datatype == XSD.nonNegativeInteger
    assert str(val) == "2"
