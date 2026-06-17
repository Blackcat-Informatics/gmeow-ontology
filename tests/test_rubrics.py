# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: MIT
"""The rubrics facility (#353, EPIC #348), in the norms slice.

A rubric IS a norm for judging (Rubric ⊑ Norm — issuer, overrides, and
authority come free; the P16 DAG rule bars extension→extension dependencies,
so the facility lives in the norms slice). Rubric content is fully reified —
criteria with NAMED poles, scales, anchored exemplars; rubric application is
solver-layer, permanently (Principle 12). An LLM judge is just a vantage:
two models disagreeing on a score are two coexisting Assessments, no winner
(Principle 9). Exemplar ⊑ CitationAct carries the kernel aboutness phase-gate
(#349) and the entity-subject pinning for EPIC #358's character exemplars.
"""

from __future__ import annotations

from pathlib import Path

from rdflib import OWL, RDF, RDFS, Graph, Literal, Namespace
from rdflib.query import ResultRow

from gmeow_tools.config import COMPETENCY_DIR
from gmeow_tools.graph import load_merged_graph
from tests._graph_nt import run_shacl

GMEOW = "https://blackcatinformatics.ca/gmeow/"
GM = Namespace(GMEOW)
GUFO = Namespace("http://purl.org/nemo/gufo#")
EX = Namespace("https://example.org/shapes/")

FIXTURES = Path(__file__).parent / "fixtures" / "shapes"


def _graph() -> Graph:
    return load_merged_graph(include_imports=False)


def _fixture(name: str) -> Graph:
    g = Graph()
    g.parse(FIXTURES / f"{name}.ttl", format="turtle")
    return g


# --------------------------------------------------------------------------- #
# Structural invariants
# --------------------------------------------------------------------------- #


def test_rubric_is_a_norm_for_judging() -> None:
    """Rubric ⊑ Norm: issuer, overrides, AuthorityLevel, PrecedenceTenure
    arrive free; an evaluation standard is never anonymous."""
    g = _graph()
    assert (GM.Rubric, RDF.type, GUFO.Kind) in g
    assert (GM.Rubric, RDFS.subClassOf, GM.Norm) in g
    assert (GM.Rubric, RDFS.subClassOf, GM.SocialObject) in g
    assert (GM.hasCriterion, RDFS.subPropertyOf, GM.hasPart) in g


def test_criterion_carries_named_poles() -> None:
    g = _graph()
    assert (GM.Criterion, RDFS.subClassOf, GM.InformationObject) in g
    assert (GM.CriterionPole, RDFS.subClassOf, GM.InformationObject) in g
    for prop in (GM.rewardPole, GM.penaltyPole):
        assert (prop, RDF.type, OWL.FunctionalProperty) in g, prop
        assert (prop, RDFS.range, GM.CriterionPole) in g, prop


def test_exemplar_is_a_citation_act_with_polarity() -> None:
    g = _graph()
    assert (GM.Exemplar, RDF.type, GUFO.SubKind) in g
    assert (GM.Exemplar, RDFS.subClassOf, GM.CitationAct) in g
    assert (GM.exemplarPolarity, RDF.type, OWL.FunctionalProperty) in g
    members = set(g.subjects(RDF.type, GM.ExemplarPolarity))
    assert members == {
        GM.polarityPositive,
        GM.polarityNegative,
        GM.polarityCautionary,
    }


def test_exemplar_subject_is_open_and_optional() -> None:
    """The EPIC #358 coordination property: entity-pattern exemplars (823 in
    the foundation corpus). Range open; not functional; coexists with
    selectors."""
    g = _graph()
    assert (GM.exemplarSubject, RDF.type, OWL.ObjectProperty) in g
    assert g.value(GM.exemplarSubject, RDFS.range) is None
    assert (GM.exemplarSubject, RDF.type, OWL.FunctionalProperty) not in g


def test_assessment_is_a_vantage_indexed_observation() -> None:
    g = _graph()
    assert (GM.Assessment, RDFS.subClassOf, GM.Observation) in g
    assert (GM.assessmentTarget, RDFS.subPropertyOf, GM.observedFeature) in g
    # The claimModality pattern: criterion/rubric play the observationMethod
    # ROLE without the subproperty axiom (functional QualityValue range vs
    # Entity-valued Criterion/Rubric).
    for prop in (GM.assessmentCriterion, GM.assessmentRubric):
        assert (prop, RDFS.subPropertyOf, GM.observationMethod) not in g, prop
        assert (prop, RDF.type, OWL.FunctionalProperty) in g, prop
    # Localizable prose is NOT functional (PR #376 review): one meaning /
    # rationale per language tag, enforced by sh:uniqueLang.
    for prop in (GM.anchorMeaning, GM.exemplarRationale):
        assert (prop, RDF.type, OWL.FunctionalProperty) not in g, prop
    assert (GM.assessmentScoreValue, RDF.type, OWL.DatatypeProperty) in g


def test_no_preferred_assessment_machinery() -> None:
    """No preferredScore / canonicalAssessment selectors (Principle 9): two
    judges disagreeing are two coexisting cells."""
    g = _graph()
    banned = (
        "preferredscore",
        "canonicalassessment",
        "primaryassessment",
        "preferredassessment",
    )
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


def test_wellformed_rubrics_fixture_conforms() -> None:
    result = run_shacl(_fixture("rubrics-wellformed"))
    assert result.ok, "\n".join(result.errors)


def test_malformed_rubrics_fixture_is_flagged() -> None:
    result = run_shacl(_fixture("rubrics-malformed"))
    assert not result.ok
    errors = "\n".join(result.errors)
    assert "reward and penalty poles must be distinct" in errors
    assert "minimum must be strictly below its maximum" in errors
    assert "at least one gmeow:anchorMeaning" in errors
    assert "range minimum must not exceed" in errors
    assert "must name exactly one gmeow:rewardPole" in errors
    assert "binds at most one gmeow:usesScale" in errors
    assert "must pin exactly one decimal gmeow:anchorRangeMin" in errors
    assert "must lie within the scale" in errors
    assert "may not redirect to the criterion that anchors it" in errors
    assert "at least one of gmeow:viaSelector" in errors
    assert "exactly one gmeow:exemplarPolarity" in errors
    assert "a gmeow:assessmentCriterion, a gmeow:assessmentRubric, or both" in errors


def test_two_judges_disagree_without_contradiction() -> None:
    """The LLM-judge doctrine in fixture form: one chunk, two vantages, two
    scores — both cells stand."""
    g = _fixture("rubrics-wellformed")
    scores: dict[object, float] = {}
    for a in g.subjects(RDF.type, GM.Assessment):
        value = g.value(a, GM.assessmentScoreValue)
        assert isinstance(value, Literal)
        scores[g.value(a, GM.vantage)] = float(value.toPython())
    assert scores[EX.judgeA] == 0.9
    assert scores[EX.judgeB] == 0.4


# --------------------------------------------------------------------------- #
# Competency
# --------------------------------------------------------------------------- #


def test_competency_exemplar_polarity_query() -> None:
    query_path = COMPETENCY_DIR / "rubrics-exemplar-polarity.rq"
    query = query_path.read_text(encoding="utf-8")
    polarities: set[object] = set()
    for row in _graph().query(query):
        assert isinstance(row, ResultRow)
        polarities.add(row[0])
    assert polarities == {
        GM.polarityPositive,
        GM.polarityNegative,
        GM.polarityCautionary,
    }
