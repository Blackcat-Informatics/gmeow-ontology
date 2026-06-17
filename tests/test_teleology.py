# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: MIT
"""The teleology core slice (#350, EPIC #348).

The UFO-C commitment-graded trichotomy: Desire (wanted) and Intention
(internally committed) are intrinsic modes inhering in one agent; Commitment
(socially committed) is a relator toward distinct beneficiaries. All aim at a
Goal — propositional content (a SocialObject) satisfied by situations, with
constitutive opposition via counterGoal. No global satisfaction verdicts, no
preferred goals (Principle 9); flat-first via hasGoal (Principle 4); revision
by suppression via IntentionTenure (Principle 10).
"""

from __future__ import annotations

from pathlib import Path

from rdflib import OWL, RDF, RDFS, Graph, Namespace
from rdflib.query import ResultRow

from gmeow_tools.config import COMPETENCY_DIR
from gmeow_tools.graph import load_merged_graph
from tests._graph_nt import run_shacl

GMEOW = "https://blackcatinformatics.ca/gmeow/"
GM = Namespace(GMEOW)
GUFO = Namespace("http://purl.org/nemo/gufo#")

FIXTURES = Path(__file__).parent / "fixtures" / "shapes"


def _graph() -> Graph:
    return load_merged_graph(include_imports=False)


def _fixture(name: str) -> Graph:
    g = Graph()
    g.parse(FIXTURES / f"{name}.ttl", format="turtle")
    return g


# --------------------------------------------------------------------------- #
# Structural invariants — the trichotomy's gUFO grounding
# --------------------------------------------------------------------------- #


def test_goal_is_a_social_object_kind() -> None:
    g = _graph()
    assert (GM.Goal, RDF.type, OWL.Class) in g
    assert (GM.Goal, RDF.type, GUFO.Kind) in g
    assert (GM.Goal, RDFS.subClassOf, GM.SocialObject) in g


def test_intrinsic_modes_are_grounded() -> None:
    g = _graph()
    assert (GM.IntentionalMode, RDF.type, GUFO.Category) in g
    # Reparented under gmeow:MentalMoment (#556); MentalMoment ⊑ gufo:IntrinsicMode
    # supplies the gUFO branch, so IntentionalMode stays grounded in IntrinsicMode
    # transitively rather than by a direct (now-removed) subClassOf assertion.
    assert (GM.IntentionalMode, RDFS.subClassOf, GM.MentalMoment) in g
    assert (GM.MentalMoment, RDFS.subClassOf, GUFO.IntrinsicMode) in g
    assert (GM.IntentionalMode, RDFS.subClassOf, GM.IntentionalMoment) in g
    for kind in (GM.Desire, GM.Intention):
        assert (kind, RDF.type, GUFO.Kind) in g
        assert (kind, RDFS.subClassOf, GM.IntentionalMode) in g


def test_commitment_is_a_relator_not_a_mode() -> None:
    """Social commitment mediates agents; it does not inhere in one."""
    g = _graph()
    assert (GM.Commitment, RDF.type, GUFO.Kind) in g
    assert (GM.Commitment, RDFS.subClassOf, GUFO.Relator) in g
    assert (GM.Commitment, RDFS.subClassOf, GM.IntentionalMoment) in g
    assert (GM.Commitment, RDFS.subClassOf, GM.IntentionalMode) not in g


def test_goal_properties_carry_named_generator_visible_domains() -> None:
    """intentionGoal and motivates use the named IntentionalMoment umbrella,
    never an anonymous union — anonymous domains vanish from the generated
    LinkML/GraphQL/TypeScript surface (PR #366 review)."""
    g = _graph()
    assert (GM.IntentionalMoment, RDF.type, GUFO.Category) in g
    assert (GM.IntentionalMoment, RDFS.subClassOf, GM.Entity) in g
    assert g.value(GM.intentionGoal, RDFS.domain) == GM.IntentionalMoment
    assert g.value(GM.motivates, RDFS.domain) == GM.IntentionalMoment


def test_desire_intention_disjoint() -> None:
    g = _graph()
    members: set[object] = set()
    for s in g.subjects(RDF.type, OWL.AllDisjointClasses):
        items = g.value(s, OWL.members)
        if items is not None:
            collection: set[object] = set(g.items(items))
            if GM.Desire in collection:
                members = collection
    assert members == {GM.Desire, GM.Intention}


def test_functional_constituents() -> None:
    g = _graph()
    for prop in (
        GM.intentBearer,
        GM.committedAgent,
        GM.intentionGoal,
        GM.tenureAgent,
        GM.tenureIntention,
    ):
        assert (prop, RDF.type, OWL.FunctionalProperty) in g, prop
    # Beneficiaries, satisfaction, motivation, and the flat shortcut are NOT
    # functional — coexistence is the point (Principle 9).
    for prop in (GM.commitmentBeneficiary, GM.satisfiedBy, GM.motivates, GM.hasGoal):
        assert (prop, RDF.type, OWL.FunctionalProperty) not in g, prop


def test_counter_goal_is_symmetric_goal_to_goal() -> None:
    g = _graph()
    assert (GM.counterGoal, RDF.type, OWL.SymmetricProperty) in g
    assert (GM.counterGoal, RDFS.domain, GM.Goal) in g
    assert (GM.counterGoal, RDFS.range, GM.Goal) in g


def test_intention_tenure_is_the_standpoint_tenure_idiom() -> None:
    g = _graph()
    assert (GM.IntentionTenure, RDF.type, GUFO.SituationType) in g
    assert (GM.IntentionTenure, RDFS.subClassOf, GM.TimeScopedRelation) in g


def test_no_preferred_or_primary_goal_terms() -> None:
    """No preferredGoal / primaryIntention selectors exist (Principle 9)."""
    g = _graph()
    banned = ("primarygoal", "preferredgoal", "primaryintention", "preferredintention")
    offenders = [
        str(s)
        for s in set(g.subjects())
        if str(s).startswith(GMEOW)
        and "/" not in str(s)[len(GMEOW) :]
        and str(s)[len(GMEOW) :].lower().startswith(banned)
    ]
    assert offenders == []


# --------------------------------------------------------------------------- #
# Closed-world SHACL shapes
# --------------------------------------------------------------------------- #


def test_wellformed_teleology_fixture_conforms() -> None:
    result = run_shacl(_fixture("teleology-wellformed"))
    assert result.ok, "\n".join(result.errors)


def test_malformed_teleology_fixture_is_flagged() -> None:
    result = run_shacl(_fixture("teleology-malformed"))
    assert not result.ok
    errors = "\n".join(result.errors)
    assert "exactly one gmeow:intentBearer" in errors
    assert "distinct from its committed agent" in errors
    assert "never its own counter-goal" in errors
    assert "exactly one gmeow:tenureAgent" in errors


# --------------------------------------------------------------------------- #
# Competency
# --------------------------------------------------------------------------- #


def test_competency_teleology_modes_query() -> None:
    query = (COMPETENCY_DIR / "teleology-modes.rq").read_text(encoding="utf-8")
    modes: set[object] = set()
    for row in _graph().query(query):
        assert isinstance(row, ResultRow)
        modes.add(row[0])
    assert {GM.Desire, GM.Intention, GM.Commitment} <= modes
