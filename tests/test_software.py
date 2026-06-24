"""Behavioural guards for the software module (#231 Phase A, #232 Phase B).

TBox structural assertions have been migrated to the declarative test-DSL at
slices/extensions/software/tests/structural.ttl (21 cells). Only SHACL
conformance, ABox fixture checks, and dynamic whole-graph sweeps are retained
here.

Migrated to structural.ttl (not tested below):
  test_software_project_is_gufo_grounded
  test_software_value_vocabs_are_quality_values
  test_parent_commit_is_derived_from_subproperty
  test_new_classes_are_owl_classes_with_correct_parents
  test_commit_ancestor_is_transitive_and_derived_from_subproperty
  test_parent_commit_is_subproperty_of_commit_ancestor
  test_commit_descendant_is_inverse_of_commit_ancestor
  test_tree_entry_properties_exist
  test_diff_properties_exist
  test_push_merge_review_properties_exist
  test_materialization_depth_property_exists
"""

from __future__ import annotations

from itertools import combinations
from pathlib import Path

from gmeow_rdf.compat.rdflib import OWL, RDF, RDFS, Graph, Literal, Namespace

from gmeow_tools.graph import load_merged_graph

GMEOW = "https://blackcatinformatics.ca/gmeow/"
GM = Namespace(GMEOW)
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
# Five-facet orthogonality guard (centrepiece)
# --------------------------------------------------------------------------- #


def test_no_subclass_bridge_between_facets() -> None:
    """The six facet classes are never bridged by rdfs:subClassOf or
    owl:equivalentClass (Principle 9 -- no overtyping)."""
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


# test_facet_orthogonality_shacl_rejects_two_facets — migrated to
# crates/validate/tests/conformance_software.rs (#867)

# --------------------------------------------------------------------------- #
# Fixture: MeowGraph
# --------------------------------------------------------------------------- #

# test_fixture_parses_and_shacl_passes — migrated to
# crates/validate/tests/conformance_software.rs (#867)


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
    (Principle 9 -- co-equal facets, never ground truth)."""
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
# Software-specific seeds (dynamic subset sweeps -- not closed-set)
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
# Fixture: MeowGraph Phase B -- 3-commit DAG, blobs, tree entries, events
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
    assert val.datatype == XSD.integer
    assert str(val) == "2"
