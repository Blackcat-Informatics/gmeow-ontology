# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: Apache-2.0
"""Inquiry slice — erotetic content and the flat inquiry spine invariants.

These structural assertions guard the load-bearing shape of the inquiry slice:
``gmeow:Question`` as the third content-mode sibling of ``gmeow:Proposition``
(assertoric) and ``gmeow:Goal`` (optative), individuated by its answerhood
condition and joined to them only by a shared ``gmeow:SocialObject`` genus —
never by an ``rdfs:subClassOf`` bridge, which would erase the content-mode
distinction (the siblinghood is documentation only).

The slice is deliberately spare:

* The four-verb inquiry spine (``asks`` / ``wondersWhether`` / ``inquiresInto``
  / ``seeksToKnow``) is FLAT (Principle 4) and OPEN-RANGE (Principle 13) — no
  ``rdfs:subPropertyOf`` among them, no ``rdfs:range``, none functional.
* ``answers`` and ``evokes`` keep an OPEN domain so the answer / evocation
  surface is never prematurely closed.
* There is NO truth or resolution bit: a question is RESOLVED by reusing the
  epistemics spine over an ``answers``-claim (Principle 6), never by an
  ``isResolved`` / ``isAnswered`` / ``isTrue`` term.
* ``gmeow:InquiryTenure`` is the reified ``gufo:Situation``-based half, an EL
  ``someValuesFrom`` mediating at least one question.
"""

from __future__ import annotations

from pathlib import Path

from rdflib import Graph, URIRef
from rdflib.namespace import OWL, RDF, RDFS

GMEOW = "https://blackcatinformatics.ca/gmeow/"
GUFO = "http://purl.org/nemo/gufo#"
SKOS_DEFINITION = URIRef("http://www.w3.org/2004/02/skos/core#definition")
SLICE_IRI = URIRef("https://blackcatinformatics.ca/gmeow/slices/inquiry")
_MODULE = Path(__file__).resolve().parents[1] / "module.ttl"

_SPINE = ("asks", "wondersWhether", "inquiresInto", "seeksToKnow")
_TYPE_INDIVIDUALS = ("typePolar", "typeAlternative", "typeWh", "typeWhy", "typeHow")

# Every locally-declared term, by name (18 total): the Question class, the
# QuestionType class, the 5 type individuals, the 3 relational properties
# (questionType / presupposes / answers), the 4 spine verbs, evokes, the
# InquiryTenure class, and the 2 tenure roles.
_DECLARED_TERMS = (
    "Question",
    "QuestionType",
    *_TYPE_INDIVIDUALS,
    "questionType",
    "presupposes",
    "answers",
    *_SPINE,
    "evokes",
    "InquiryTenure",
    "tenureInquirer",
    "tenureQuestion",
)


def _t(name: str) -> URIRef:
    """A gmeow-namespaced term URI."""
    return URIRef(GMEOW + name)


def _gufo(name: str) -> URIRef:
    """A gufo-namespaced term URI."""
    return URIRef(GUFO + name)


def _graph() -> Graph:
    g = Graph()
    g.parse(_MODULE, format="turtle")
    return g


def test_question_is_a_social_object_kind() -> None:
    """Question is an owl:Class, a gufo:Kind, and a gmeow:SocialObject — the
    third content-mode sibling sharing the SocialObject genus."""
    g = _graph()
    question = _t("Question")
    assert (question, RDF.type, OWL.Class) in g
    assert (question, RDF.type, _gufo("Kind")) in g
    assert (question, RDFS.subClassOf, _t("SocialObject")) in g


def test_content_mode_siblings_have_no_subsumption() -> None:
    """Sibling no-subsumption guard: Question / Proposition / Goal are siblings
    by content-mode (documentation only), never by an rdfs:subClassOf bridge in
    either direction — subsumption would erase the content-mode distinction."""
    g = _graph()
    question = _t("Question")
    proposition = _t("Proposition")
    goal = _t("Goal")
    assert (question, RDFS.subClassOf, proposition) not in g
    assert (question, RDFS.subClassOf, goal) not in g
    assert (proposition, RDFS.subClassOf, question) not in g
    assert (goal, RDFS.subClassOf, question) not in g


def test_spine_are_object_properties_with_agent_domain_open_range() -> None:
    """Each spine verb is an owl:ObjectProperty with rdfs:domain gmeow:Agent, an
    OPEN range (no rdfs:range asserted, Principle 13), and is non-functional —
    an agent holds many inquiry attitudes at once."""
    g = _graph()
    for prop in _SPINE:
        term = _t(prop)
        assert (term, RDF.type, OWL.ObjectProperty) in g
        assert (term, RDFS.domain, _t("Agent")) in g
        assert (term, RDFS.range, None) not in g
        assert (term, RDF.type, OWL.FunctionalProperty) not in g


def test_spine_is_flat() -> None:
    """The inquiry spine is FLAT (Principle 4): no spine verb declares an
    rdfs:subPropertyOf — four distinct attitudes, not a hierarchy (contrast the
    doxastic keystone knowsThat ⊑ believes)."""
    g = _graph()
    for prop in _SPINE:
        assert (_t(prop), RDFS.subPropertyOf, None) not in g


def test_question_type_is_an_abstract_individual_type() -> None:
    """QuestionType is an owl:Class, a gufo:AbstractIndividualType, and a
    subclass of gufo:QualityValue — the value-vocabulary genus."""
    g = _graph()
    qt = _t("QuestionType")
    assert (qt, RDF.type, OWL.Class) in g
    assert (qt, RDF.type, _gufo("AbstractIndividualType")) in g
    assert (qt, RDFS.subClassOf, _gufo("QualityValue")) in g


def test_question_type_individuals_are_seeded() -> None:
    """The five question-type values are individuals of gmeow:QuestionType (a
    closed value vocabulary, members never subclasses)."""
    g = _graph()
    for indiv in _TYPE_INDIVIDUALS:
        assert (_t(indiv), RDF.type, _t("QuestionType")) in g


def test_question_type_property() -> None:
    """questionType is an owl:ObjectProperty from gmeow:Question to
    gmeow:QuestionType, and is non-functional (typeWhy and typeHow may
    co-apply)."""
    g = _graph()
    qt = _t("questionType")
    assert (qt, RDF.type, OWL.ObjectProperty) in g
    assert (qt, RDFS.domain, _t("Question")) in g
    assert (qt, RDFS.range, _t("QuestionType")) in g
    assert (qt, RDF.type, OWL.FunctionalProperty) not in g


def test_presupposes_property() -> None:
    """presupposes is an owl:ObjectProperty from gmeow:Question to
    gmeow:Proposition — the presupposition rides as the subject of the relation,
    not as a global fact."""
    g = _graph()
    presupposes = _t("presupposes")
    assert (presupposes, RDF.type, OWL.ObjectProperty) in g
    assert (presupposes, RDFS.domain, _t("Question")) in g
    assert (presupposes, RDFS.range, _t("Proposition")) in g


def test_answers_has_open_domain() -> None:
    """answers is an owl:ObjectProperty ranging over gmeow:Question with an OPEN
    domain (no rdfs:domain asserted) so the answer surface is never prematurely
    closed, and is non-functional (answers are vantage-indexed, Principle 9)."""
    g = _graph()
    answers = _t("answers")
    assert (answers, RDF.type, OWL.ObjectProperty) in g
    assert (answers, RDFS.range, _t("Question")) in g
    assert (answers, RDFS.domain, None) not in g
    assert (answers, RDF.type, OWL.FunctionalProperty) not in g


def test_evokes_has_open_domain() -> None:
    """evokes is an owl:ObjectProperty ranging over gmeow:Question with an OPEN
    domain (a Proposition or a Question may evoke a question) — a solver-layer
    decoration with no asserted rdfs:domain."""
    g = _graph()
    evokes = _t("evokes")
    assert (evokes, RDF.type, OWL.ObjectProperty) in g
    assert (evokes, RDFS.range, _t("Question")) in g
    assert (evokes, RDFS.domain, None) not in g


def test_inquiry_tenure_is_a_mediating_situation() -> None:
    """InquiryTenure is an owl:Class, a gufo:SituationType, and a subclass of
    gmeow:TimeScopedRelation (not a bare relator), its tenure roles are
    functional object properties, and the EL someValuesFrom mediation
    restriction over gmeow:tenureQuestion is present."""
    g = _graph()
    tenure = _t("InquiryTenure")
    assert (tenure, RDF.type, OWL.Class) in g
    assert (tenure, RDF.type, _gufo("SituationType")) in g
    assert (tenure, RDFS.subClassOf, _t("TimeScopedRelation")) in g

    inquirer = _t("tenureInquirer")
    assert (inquirer, RDF.type, OWL.ObjectProperty) in g
    assert (inquirer, RDFS.domain, tenure) in g
    assert (inquirer, RDFS.range, _t("Agent")) in g

    question_role = _t("tenureQuestion")
    assert (question_role, RDF.type, OWL.ObjectProperty) in g
    assert (question_role, RDFS.domain, tenure) in g
    assert (question_role, RDFS.range, _t("Question")) in g

    # The EL relator-mediation restriction: some blank node b is a
    # someValuesFrom Question restriction on tenureQuestion, asserted as a
    # superclass of InquiryTenure.
    found = False
    for b in g.objects(tenure, RDFS.subClassOf):
        if (
            (b, RDF.type, OWL.Restriction) in g
            and (b, OWL.onProperty, _t("tenureQuestion")) in g
            and (b, OWL.someValuesFrom, _t("Question")) in g
        ):
            found = True
            break
    assert found, "missing EL someValuesFrom mediation restriction on tenureQuestion"


def test_no_truth_or_resolved_bit() -> None:
    """No resolution or truth bit: a question is resolved by reusing the
    epistemics spine over an answers-claim (Principle 6), so none of
    isResolved / isAnswered / answerValue / isTrue / isFalse appears in ANY
    triple position."""
    g = _graph()
    for name in ("isResolved", "isAnswered", "answerValue", "isTrue", "isFalse"):
        term = _t(name)
        assert (term, None, None) not in g
        assert (None, term, None) not in g
        assert (None, None, term) not in g


def test_every_declared_term_is_annotated() -> None:
    """Annotation-completeness (Principle 8): each of the 18 locally-declared
    terms — including the 5 type individuals and the 2 tenure roles — carries an
    rdfs:label, a skos:definition, and rdfs:isDefinedBy the inquiry slice IRI."""
    g = _graph()
    assert len(_DECLARED_TERMS) == 18
    for name in _DECLARED_TERMS:
        term = _t(name)
        assert (term, RDFS.label, None) in g, f"{name} missing rdfs:label"
        assert (term, SKOS_DEFINITION, None) in g, f"{name} missing skos:definition"
        assert (term, RDFS.isDefinedBy, SLICE_IRI) in g, (
            f"{name} missing rdfs:isDefinedBy slice IRI"
        )
