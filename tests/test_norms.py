# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: MIT
"""The norms extension + rights graft (#351 / #352, EPIC #348).

There is no ought, only ought-according-to: every modality-bearing Norm names
its issuer (normIssuer ⊑ accordingTo, documented not axiomatised — the
vantage precedent). Precedence is data, not semantics: overrides is pairwise
and deliberately NOT transitive; AuthorityLevel carries the ordered-vocab
pattern; PrecedenceTenure is the StandpointTenure idiom. Conditions are
stored, never executed (Principle 12); verdicts are vantage-indexed
Observations. The rights graft asserts Rule ⊑ Norm extension-side with zero
core churn — the trio stays a rigid subkind partition under the open
modality axis.
"""

from __future__ import annotations

from pathlib import Path

from gmeow_rdf.compat.rdflib import OWL, RDF, RDFS, Graph, Namespace
from gmeow_rdf.compat.rdflib.query import ResultRow

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
# The Norm spine
# --------------------------------------------------------------------------- #


def test_norm_is_an_entity_level_category() -> None:
    """Norm is a gufo:Category at Entity level (the IntentionalMoment
    precedent): the graft puts Rule (⊑ Relator) beneath it while plain norms
    are object-like, and Object ⟂ Aspect forbids committing to either; a
    Kind would stack identities under the rights trio (MixIden gate)."""
    g = _graph()
    assert (GM.Norm, RDF.type, GUFO.Category) in g
    assert (GM.Norm, RDF.type, GUFO.Kind) not in g
    assert (GM.Norm, RDFS.subClassOf, GM.Entity) in g
    assert (GM.Norm, RDFS.subClassOf, GM.SocialObject) not in g
    assert (GM.NormativeSystem, RDFS.subClassOf, GM.SocialObject) in g


def test_deontic_modality_vocab_seeds() -> None:
    g = _graph()
    members = set(g.subjects(RDF.type, GM.DeonticModality))
    assert {
        GM.deonticObligation,
        GM.deonticProhibition,
        GM.deonticPermission,
        GM.deonticRecommendation,
    } <= members


def test_no_anonymous_ought_machinery() -> None:
    """normIssuer is range-open, NOT functional (co-issued norms coexist),
    and deliberately not an rdfs:subPropertyOf of accordingTo (annotation
    property — the vantage precedent: documented, not axiomatised)."""
    g = _graph()
    assert (GM.normIssuer, RDF.type, OWL.ObjectProperty) in g
    assert g.value(GM.normIssuer, RDFS.range) is None
    # Domain-free so systemIssuer ⊑ normIssuer cannot leak Norm typing onto
    # NormativeSystems (PR #369 review).
    assert g.value(GM.normIssuer, RDFS.domain) is None
    assert (GM.normIssuer, RDF.type, OWL.FunctionalProperty) not in g
    assert (GM.normIssuer, RDFS.subPropertyOf, GM.accordingTo) not in g
    assert (GM.systemIssuer, RDFS.subPropertyOf, GM.normIssuer) in g


def test_prescribed_conduct_is_range_open() -> None:
    """The tenurePosition precedent: an open range so the graft needs no
    dual-typing and the generated surface keeps a clean named domain."""
    g = _graph()
    assert g.value(GM.prescribedConduct, RDFS.range) is None
    assert (GM.prescribedConduct, RDFS.domain, GM.Norm) in g


# --------------------------------------------------------------------------- #
# Authority & precedence
# --------------------------------------------------------------------------- #


def test_overrides_is_pairwise_never_transitive() -> None:
    g = _graph()
    assert (GM.overrides, RDF.type, OWL.ObjectProperty) in g
    assert (GM.overrides, RDF.type, OWL.TransitiveProperty) not in g
    assert (GM.overrides, RDFS.domain, GM.Norm) in g
    assert (GM.overrides, RDFS.range, GM.Norm) in g


def test_authority_levels_are_ordered_on_levels_only() -> None:
    g = _graph()
    assert (GM.strongerThan, RDF.type, OWL.TransitiveProperty) in g
    assert (GM.strongerThan, RDFS.domain, GM.AuthorityLevel) in g
    chain = [
        (GM.authorityAbsolute, GM.authorityHigh),
        (GM.authorityHigh, GM.authorityMedium),
        (GM.authorityMedium, GM.authorityConditional),
    ]
    for higher, lower in chain:
        assert (higher, GM.strongerThan, lower) in g
    # The norm-level axis stays defeasible: hasAuthorityLevel non-functional.
    assert (GM.hasAuthorityLevel, RDF.type, OWL.FunctionalProperty) not in g


def test_precedence_tenure_is_the_standpoint_tenure_idiom() -> None:
    g = _graph()
    assert (GM.PrecedenceTenure, RDF.type, GUFO.SituationType) in g
    assert (GM.PrecedenceTenure, RDFS.subClassOf, GM.TimeScopedRelation) in g
    for prop in (GM.precedenceHigher, GM.precedenceLower, GM.precedenceScope):
        assert (prop, RDF.type, OWL.FunctionalProperty) in g, prop


# --------------------------------------------------------------------------- #
# Condition machinery — stored, never executed
# --------------------------------------------------------------------------- #


def test_condition_is_prose_canonical() -> None:
    g = _graph()
    assert (GM.Condition, RDFS.subClassOf, GM.InformationObject) in g
    assert (GM.ConditionGroup, RDFS.subClassOf, GM.Condition) in g
    assert (GM.conditionText, RDF.type, OWL.FunctionalProperty) in g
    # One condition, several formalizations — each an equivalence claim.
    assert (GM.formalizedAs, RDF.type, OWL.FunctionalProperty) not in g


def test_group_operator_vocab_is_closed_three() -> None:
    g = _graph()
    members = set(g.subjects(RDF.type, GM.GroupOperator))
    assert members == {GM.operatorAll, GM.operatorAny, GM.operatorNone}


def test_expression_language_vocab_seeds() -> None:
    g = _graph()
    members = set(g.subjects(RDF.type, GM.ExpressionLanguage))
    assert {
        GM.exprLangProse,
        GM.exprLangSparqlAsk,
        GM.exprLangCel,
        GM.exprLangRego,
        GM.exprLangCedar,
        GM.exprLangXacml,
        GM.exprLangShacl,
    } <= members


def test_evaluation_is_a_vantage_indexed_observation() -> None:
    g = _graph()
    assert (GM.ConditionEvaluation, RDFS.subClassOf, GM.Observation) in g
    assert (GM.evaluatedCondition, RDFS.subPropertyOf, GM.observedFeature) in g
    # The claimModality precedent: verdict is NOT a subproperty of
    # observationResult (QualityValue ⟂ Entity in the DL profile).
    assert (GM.evaluationVerdict, RDFS.subPropertyOf, GM.observationResult) not in g
    members = set(g.subjects(RDF.type, GM.EvaluationVerdict))
    assert members == {GM.verdictHeld, GM.verdictNotHeld, GM.verdictUndetermined}


def test_compliance_is_never_entailed() -> None:
    """violates/complies are plain assertable shortcuts with no inferential
    machinery: no property chains, no inverse entailment, no functional
    axioms — judgement properties, not logic (Principles 1, 9, 12)."""
    g = _graph()
    for prop in (GM.violates, GM.complies):
        assert (prop, RDF.type, OWL.ObjectProperty) in g
        assert (prop, RDF.type, OWL.FunctionalProperty) not in g
        assert g.value(prop, OWL.propertyChainAxiom) is None
    assert (GM.ComplianceAssessment, RDFS.subClassOf, GM.Observation) in g
    assert (GM.assessedEvent, RDFS.subPropertyOf, GM.observedFeature) in g


# --------------------------------------------------------------------------- #
# The rights graft (#352) — zero core churn
# --------------------------------------------------------------------------- #


def test_graft_rule_is_a_norm() -> None:
    g = _graph()
    assert (GM.Rule, RDFS.subClassOf, GM.Norm) in g
    assert (GM.ruleAssignee, RDFS.subPropertyOf, GM.normBearer) in g
    assert (GM.ruleAction, RDFS.subPropertyOf, GM.prescribedConduct) in g


def test_graft_axioms_live_extension_side_only() -> None:
    """Zero core churn: the core rights module contains no reference to any
    norms-extension IRI — the graft is asserted in the norms module."""
    core_rights = Graph()
    core_rights.parse(
        Path(__file__).parent.parent / "slices" / "core" / "rights" / "module.ttl",
        format="turtle",
    )
    norms_terms = [GM.Norm, GM.deonticModality, GM.normIssuer, GM.normBearer]
    for term in norms_terms:
        assert not list(core_rights.triples((term, None, None))), f"{term} as subject"
        assert not list(core_rights.triples((None, term, None))), f"{term} as predicate"
        assert not list(core_rights.triples((None, None, term))), f"{term} as object"


def test_graft_preserves_core_trio_classhood() -> None:
    """The survey's do-not-touch list holds: the trio remain disjoint OWL
    classes with their gUFO grounding (the 5 class-dependent conflicts never
    fire)."""
    g = _graph()
    for cls in (GM.Permission, GM.Prohibition, GM.Duty):
        assert (cls, RDF.type, OWL.Class) in g
        assert (cls, RDFS.subClassOf, GM.Rule) in g


# --------------------------------------------------------------------------- #
# Closed-world SHACL shapes
# --------------------------------------------------------------------------- #


def test_wellformed_norms_fixture_conforms() -> None:
    result = run_shacl(_fixture("norms-wellformed"))
    assert result.ok, "\n".join(result.errors)


def test_malformed_norms_fixture_is_flagged() -> None:
    result = run_shacl(_fixture("norms-malformed"))
    assert not result.ok
    errors = "\n".join(result.errors)
    assert "no ought, only ought-according-to" in errors
    assert "never overrides itself" in errors
    assert "at least two gmeow:groupMember" in errors
    assert "exactly one gmeow:groupOperator" in errors
    # ex:mutexParam binds both a value and an entity — the XOR must fire.
    assert "binds exactly one of gmeow:parameterValue" in errors
    assert "higher and lower norms must be distinct" in errors
    # ex:flatTenure also omits its scope — both tenure violations must fire.
    assert "must be scoped to exactly one gmeow:precedenceScope" in errors
    assert "must be gmeow:deonticPermission" in errors
    assert "exactly one gmeow:evaluationVerdict" in errors
    assert "at most one gmeow:deonticModality" in errors


# --------------------------------------------------------------------------- #
# Competency
# --------------------------------------------------------------------------- #


def test_competency_deontic_modalities_query() -> None:
    query = (COMPETENCY_DIR / "norms-deontic-modalities.rq").read_text(encoding="utf-8")
    modalities: set[object] = set()
    for row in _graph().query(query):
        assert isinstance(row, ResultRow)
        modalities.add(row[0])
    assert {
        GM.deonticObligation,
        GM.deonticProhibition,
        GM.deonticPermission,
        GM.deonticRecommendation,
    } <= modalities


def test_competency_authority_order_query() -> None:
    """absolute reaches conditional through strongerThan+ and nothing
    outranks absolute."""
    query = (COMPETENCY_DIR / "norms-authority-order.rq").read_text(encoding="utf-8")
    tops: set[object] = set()
    for row in _graph().query(query):
        assert isinstance(row, ResultRow)
        tops.add(row[0])
    assert tops == {GM.authorityAbsolute}
