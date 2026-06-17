# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: Apache-2.0
"""Learning slice — learning-as-process invariants.

These structural assertions guard the load-bearing shape of the learning slice:
``gmeow:LearningEvent`` as the OCCURRENT that transitions an agent's knowledge
state, reparented under ``gmeow:MentalProcess`` (the hook the mentation slice
reserves) — the dynamic complement to the endurant knowledge STATES modelled by
the cognition and expertise slices.

The slice is deliberately spare:

* The VARIETY of a learning event is a ``gmeow:LearningEventType`` value carried
  by ``gmeow:learningType`` (non-functional), never a subclass of
  ``gmeow:LearningEvent`` (Principle 9) — the dedicated kind-vocab mirroring the
  inference slice's ``gmeow:InferenceMode``. The CLASS names the learning-ness,
  so NO class-trivial ``gmeow:mentalProcessType`` value is minted (the
  inference-slice precedent that dropped ``eventTypeInference``, Principle 4).
* ``gmeow:subjectTaught`` / ``gmeow:learnedFrom`` / ``gmeow:fromLevel`` /
  ``gmeow:toLevel`` / ``gmeow:produces`` keep an OPEN range (Principle 13): the
  targets are retired (``gmeow:Source``) or not yet built (``gmeow:Concept``).
* ``gmeow:Teaching`` is the reified ``gufo:Relator`` mediating teacher /
  learner / subjectTaught, an EL ``someValuesFrom`` over teacher and learner;
  the closed-world teacher ≠ learner rule is SHACL's (``gmeow:TeachingShape``).
* Forgetting is suppression, referenced not duplicated (Principle 6): there is
  NO ``gmeow:forgets`` / ``gmeow:forgotten`` / ``gmeow:isLearned`` term, and the
  slice does not redeclare the cognition slice's ``gmeow:remembers``.
"""

from __future__ import annotations

from pathlib import Path

from rdflib import Graph, URIRef
from rdflib.namespace import OWL, RDF, RDFS

GMEOW = "https://blackcatinformatics.ca/gmeow/"
GUFO = "http://purl.org/nemo/gufo#"
SKOS_DEFINITION = URIRef("http://www.w3.org/2004/02/skos/core#definition")
SLICE_IRI = URIRef("https://blackcatinformatics.ca/gmeow/slices/learning")
_MODULE = Path(__file__).resolve().parents[1] / "module.ttl"

_TYPE_INDIVIDUALS = (
    "learningConceptFormation",
    "learningSkillAcquisition",
    "learningBeingTaught",
    "learningConsolidation",
    "learningTransfer",
    "learningUnlearning",
)
_OPEN_RANGE_PROPS = ("subjectTaught", "learnedFrom", "fromLevel", "toLevel", "produces")

# Every locally-declared term, by name (17 total): the LearningEvent class, the
# LearningEventType class, the 6 type individuals, the learningType property, the
# 4 provenance/trajectory/product properties (learnedFrom / fromLevel / toLevel /
# produces), the Teaching class, and its 3 role properties.
_DECLARED_TERMS = (
    "LearningEvent",
    "LearningEventType",
    *_TYPE_INDIVIDUALS,
    "learningType",
    "learnedFrom",
    "fromLevel",
    "toLevel",
    "produces",
    "Teaching",
    "teacher",
    "learner",
    "subjectTaught",
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


def test_learning_event_reparents_mental_process() -> None:
    """LearningEvent is an owl:Class, a gufo:EventType, and rdfs:subClassOf
    gmeow:MentalProcess — the occurrent reparenting hook the mentation slice
    reserves, exactly as the sibling gmeow:InferenceProcess."""
    g = _graph()
    event = _t("LearningEvent")
    assert (event, RDF.type, OWL.Class) in g
    assert (event, RDF.type, _gufo("EventType")) in g
    assert (event, RDFS.subClassOf, _t("MentalProcess")) in g


def test_no_class_trivial_process_type() -> None:
    """The class names the learning-ness, so NO class-trivial gmeow:processLearning
    value is minted (Principle 4 — one canonical source, the inference-slice
    precedent), and the slice asserts no gmeow:mentalProcessType on the class."""
    g = _graph()
    process_learning = _t("processLearning")
    assert (process_learning, None, None) not in g
    assert (None, None, process_learning) not in g
    assert (_t("LearningEvent"), _t("mentalProcessType"), None) not in g


def test_learning_event_type_is_an_abstract_individual_type() -> None:
    """LearningEventType is an owl:Class, a gufo:AbstractIndividualType, and a
    subclass of gufo:QualityValue — the value-vocabulary genus."""
    g = _graph()
    let = _t("LearningEventType")
    assert (let, RDF.type, OWL.Class) in g
    assert (let, RDF.type, _gufo("AbstractIndividualType")) in g
    assert (let, RDFS.subClassOf, _gufo("QualityValue")) in g


def test_learning_event_type_individuals_are_seeded() -> None:
    """The six learning-event-type values are individuals of
    gmeow:LearningEventType (an open value vocabulary, members never
    subclasses)."""
    g = _graph()
    for indiv in _TYPE_INDIVIDUALS:
        assert (_t(indiv), RDF.type, _t("LearningEventType")) in g
        # A value individual is never a subclass of the event class.
        assert (_t(indiv), RDFS.subClassOf, _t("LearningEvent")) not in g


def test_learning_type_property() -> None:
    """learningType is an owl:ObjectProperty from gmeow:LearningEvent to
    gmeow:LearningEventType, and is non-functional (several varieties may
    co-apply to one event)."""
    g = _graph()
    lt = _t("learningType")
    assert (lt, RDF.type, OWL.ObjectProperty) in g
    assert (lt, RDFS.domain, _t("LearningEvent")) in g
    assert (lt, RDFS.range, _t("LearningEventType")) in g
    assert (lt, RDF.type, OWL.FunctionalProperty) not in g


def test_provenance_trajectory_product_are_open_range() -> None:
    """subjectTaught / learnedFrom / fromLevel / toLevel / produces keep an OPEN
    range (no rdfs:range asserted, Principle 13) and are non-functional — the
    target classes are retired (gmeow:Source) or not yet built (gmeow:Concept).
    learnedFrom / fromLevel / toLevel / produces have domain gmeow:LearningEvent;
    subjectTaught has domain gmeow:Teaching."""
    g = _graph()
    for prop in _OPEN_RANGE_PROPS:
        term = _t(prop)
        assert (term, RDF.type, OWL.ObjectProperty) in g
        assert (term, RDFS.range, None) not in g, f"{prop} must keep an open range"
        assert (term, RDF.type, OWL.FunctionalProperty) not in g
    for prop in ("learnedFrom", "fromLevel", "toLevel", "produces"):
        assert (_t(prop), RDFS.domain, _t("LearningEvent")) in g
    assert (_t("subjectTaught"), RDFS.domain, _t("Teaching")) in g


def test_teaching_is_a_mediating_relator() -> None:
    """Teaching is an owl:Class, a gufo:Kind, and a subclass of gufo:Relator;
    teacher is a functional object property (range Agent), learner is
    non-functional (range Agent), subjectTaught is open-range, and the EL
    someValuesFrom mediation restrictions over teacher and learner are present."""
    g = _graph()
    teaching = _t("Teaching")
    assert (teaching, RDF.type, OWL.Class) in g
    assert (teaching, RDF.type, _gufo("Kind")) in g
    assert (teaching, RDFS.subClassOf, _gufo("Relator")) in g

    teacher = _t("teacher")
    assert (teacher, RDF.type, OWL.ObjectProperty) in g
    assert (teacher, RDFS.domain, teaching) in g
    assert (teacher, RDFS.range, _t("Agent")) in g
    assert (teacher, RDF.type, OWL.FunctionalProperty) in g

    learner = _t("learner")
    assert (learner, RDF.type, OWL.ObjectProperty) in g
    assert (learner, RDFS.domain, teaching) in g
    assert (learner, RDFS.range, _t("Agent")) in g
    assert (learner, RDF.type, OWL.FunctionalProperty) not in g

    # The EL relator-mediation restrictions: someValuesFrom Agent on teacher AND
    # on learner, each asserted as a superclass of Teaching.
    for role in ("teacher", "learner"):
        found = False
        for b in g.objects(teaching, RDFS.subClassOf):
            if (
                (b, RDF.type, OWL.Restriction) in g
                and (b, OWL.onProperty, _t(role)) in g
                and (b, OWL.someValuesFrom, _t("Agent")) in g
            ):
                found = True
                break
        assert found, f"missing EL someValuesFrom mediation restriction on {role}"


def test_no_forgetting_or_truth_bit_and_remembers_not_redeclared() -> None:
    """Forgetting is suppression, referenced not duplicated (Principle 6): none of
    forgets / forgotten / isLearned / unlearns appears in ANY triple position, and
    the slice does not redeclare the cognition slice's gmeow:remembers."""
    g = _graph()
    for name in ("forgets", "forgotten", "isLearned", "unlearns", "isTaught"):
        term = _t(name)
        assert (term, None, None) not in g
        assert (None, term, None) not in g
        assert (None, None, term) not in g
    # gmeow:remembers belongs to cognition; the learning module never declares it.
    assert (_t("remembers"), None, None) not in g


def test_every_declared_term_is_annotated() -> None:
    """Annotation-completeness (Principle 8): each of the 17 locally-declared
    terms — including the 6 type individuals and the 3 Teaching roles — carries an
    rdfs:label, a skos:definition, and rdfs:isDefinedBy the learning slice IRI."""
    g = _graph()
    assert len(_DECLARED_TERMS) == 17
    for name in _DECLARED_TERMS:
        term = _t(name)
        assert (term, RDFS.label, None) in g, f"{name} missing rdfs:label"
        assert (term, SKOS_DEFINITION, None) in g, f"{name} missing skos:definition"
        assert (term, RDFS.isDefinedBy, SLICE_IRI) in g, (
            f"{name} missing rdfs:isDefinedBy slice IRI"
        )
