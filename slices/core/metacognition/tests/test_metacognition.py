# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: Apache-2.0
"""Metacognition slice — second-order epistemics and calibration invariants.

These structural assertions guard the load-bearing shape of the metacognition
slice: ``gmeow:MetacognitiveState`` as the reflexive turn — a ``gufo:Kind`` mode
under ``gmeow:MentalMoment``, the co-equal sibling of ``gmeow:CognitiveState``
(cognition) and ``gmeow:IntentionalMode`` (teleology), never an
``rdfs:subClassOf`` of either (subsumption would erase the reflexive-vs-first-
order distinction, exactly as ``gmeow:Question`` is not a ``gmeow:Proposition``).

The slice is deliberately spare and solver-clean:

* ``gmeow:metaTarget`` is the reflexivity edge — domain ``MetacognitiveState``,
  OPEN range (Principle 13) — and carries NO property characteristics; in
  particular it is NOT ``owl:ReflexiveProperty`` (the reflexivity is conceptual,
  not logical, and reflexivity leaves EL).
* ``gmeow:calibration`` assigns a closed ``gmeow:CalibrationStatus`` value vocab
  (wellCalibrated / overconfident / underconfident — individuals, never
  subclasses).
* ``gmeow:calibrationError`` is the Brier-style magnitude and is an
  ``owl:AnnotationProperty``, NOT an ``owl:DatatypeProperty`` — the solver-layer
  guard (Principle 12): it is invisible to the reasoner and can never become a
  materialised axiom.
* ``gmeow:awareOfNotKnowing`` and ``gmeow:epistemicSelfTrust`` are flat
  (Principle 4), domain ``gmeow:Agent``, OPEN range, and reuse the inquiry /
  trust slices by reference rather than re-minting (no ``rdfs:subPropertyOf``).
* ``gmeow:eventTypeReflection`` is a ``gmeow:EventType`` value individual.
* There is NO truth / status bit (no boolean-ranged property, no
  isCalibrated / isReflected / isTrue term).
"""

from __future__ import annotations

from pathlib import Path

from gmeow_rdf.compat.rdflib import Graph, URIRef
from gmeow_rdf.compat.rdflib.namespace import OWL, RDF, RDFS, XSD

GMEOW = "https://blackcatinformatics.ca/gmeow/"
GUFO = "http://purl.org/nemo/gufo#"
SKOS_DEFINITION = URIRef("http://www.w3.org/2004/02/skos/core#definition")
SLICE_IRI = URIRef("https://blackcatinformatics.ca/gmeow/slices/metacognition")
_MODULE = Path(__file__).resolve().parents[1] / "module.ttl"

_CALIBRATION_STATUSES = ("wellCalibrated", "overconfident", "underconfident")

# Every locally-declared term, by name (11 total): the MetacognitiveState class,
# the reflexivity edge metaTarget, the CalibrationStatus class and its 3
# individuals, the calibration property, the calibrationError annotation, the
# two flat known-unknown / self-trust properties, and the reflection EventType
# individual.
_DECLARED_TERMS = (
    "MetacognitiveState",
    "metaTarget",
    "CalibrationStatus",
    *_CALIBRATION_STATUSES,
    "calibration",
    "calibrationError",
    "awareOfNotKnowing",
    "epistemicSelfTrust",
    "eventTypeReflection",
)

# The property characteristics gmeow:metaTarget must NOT carry — conceptual
# reflexivity is not OWL reflexivity, and none of these belong on the edge.
_FORBIDDEN_CHARACTERISTICS = (
    OWL.ReflexiveProperty,
    OWL.IrreflexiveProperty,
    OWL.TransitiveProperty,
    OWL.SymmetricProperty,
    OWL.FunctionalProperty,
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


def test_metacognitive_state_is_a_mental_moment_kind() -> None:
    """MetacognitiveState is an owl:Class, a gufo:Kind, and a subclass of
    gmeow:MentalMoment — the reflexive mode sibling (the CognitiveState
    pattern), carrying exactly one gUFO metaclass."""
    g = _graph()
    state = _t("MetacognitiveState")
    assert (state, RDF.type, OWL.Class) in g
    assert (state, RDF.type, _gufo("Kind")) in g
    assert (state, RDFS.subClassOf, _t("MentalMoment")) in g
    gufo_metaclasses = {
        o for o in g.objects(state, RDF.type) if str(o).startswith(GUFO)
    }
    assert gufo_metaclasses == {_gufo("Kind")}, (
        f"MetacognitiveState must carry exactly one gUFO metaclass, got "
        f"{sorted(str(x) for x in gufo_metaclasses)}"
    )


def test_metacognitive_state_is_a_sibling_not_a_sub_mode() -> None:
    """Sibling no-subsumption guard: MetacognitiveState is a co-equal child of
    gmeow:MentalMoment, never an rdfs:subClassOf gmeow:CognitiveState /
    gmeow:IntentionalMode — subsumption would erase the reflexive-vs-first-order
    distinction (Principle 9)."""
    g = _graph()
    state = _t("MetacognitiveState")
    assert (state, RDFS.subClassOf, _t("CognitiveState")) not in g
    assert (state, RDFS.subClassOf, _t("IntentionalMode")) not in g
    # It is the mode, not its own realisation as a claim.
    assert (state, RDFS.subClassOf, _t("StandpointClaim")) not in g


def test_meta_target_is_open_range_and_characteristic_free() -> None:
    """metaTarget is an owl:ObjectProperty with rdfs:domain MetacognitiveState,
    an OPEN range (no rdfs:range, Principle 13), non-subordinate (flat), and
    carries NO property characteristics — in particular it is NOT
    owl:ReflexiveProperty (conceptual reflexivity is not OWL reflexivity)."""
    g = _graph()
    target = _t("metaTarget")
    assert (target, RDF.type, OWL.ObjectProperty) in g
    assert (target, RDFS.domain, _t("MetacognitiveState")) in g
    assert (target, RDFS.range, None) not in g
    assert (target, RDFS.subPropertyOf, None) not in g
    for characteristic in _FORBIDDEN_CHARACTERISTICS:
        assert (target, RDF.type, characteristic) not in g, (
            f"metaTarget must not be {characteristic}"
        )


def test_calibration_status_is_an_abstract_individual_type() -> None:
    """CalibrationStatus is an owl:Class, a gufo:AbstractIndividualType, and a
    subclass of gufo:QualityValue — the value-vocabulary genus."""
    g = _graph()
    status = _t("CalibrationStatus")
    assert (status, RDF.type, OWL.Class) in g
    assert (status, RDF.type, _gufo("AbstractIndividualType")) in g
    assert (status, RDFS.subClassOf, _gufo("QualityValue")) in g


def test_calibration_statuses_are_seeded_individuals() -> None:
    """The three calibration statuses are individuals of gmeow:CalibrationStatus
    (a closed value vocabulary, members never subclasses)."""
    g = _graph()
    for indiv in _CALIBRATION_STATUSES:
        assert (_t(indiv), RDF.type, _t("CalibrationStatus")) in g
        # Members are individuals, not subclasses of the vocab class.
        assert (_t(indiv), RDFS.subClassOf, _t("CalibrationStatus")) not in g


def test_calibration_property() -> None:
    """calibration is an owl:ObjectProperty from gmeow:MetacognitiveState to the
    closed gmeow:CalibrationStatus vocabulary, and is non-functional (a
    self-assessment and an external assessment may coexist, Principle 9)."""
    g = _graph()
    calibration = _t("calibration")
    assert (calibration, RDF.type, OWL.ObjectProperty) in g
    assert (calibration, RDFS.domain, _t("MetacognitiveState")) in g
    assert (calibration, RDFS.range, _t("CalibrationStatus")) in g
    assert (calibration, RDF.type, OWL.FunctionalProperty) not in g


def test_calibration_error_is_a_solver_layer_annotation() -> None:
    """calibrationError is an owl:AnnotationProperty and NOT an
    owl:DatatypeProperty / owl:ObjectProperty — the solver-layer guard
    (Principle 12): a Brier-style metric invisible to the reasoner, which can
    never become a materialised axiom. It carries no domain/range."""
    g = _graph()
    error = _t("calibrationError")
    assert (error, RDF.type, OWL.AnnotationProperty) in g
    assert (error, RDF.type, OWL.DatatypeProperty) not in g
    assert (error, RDF.type, OWL.ObjectProperty) not in g
    assert (error, RDFS.domain, None) not in g
    assert (error, RDFS.range, None) not in g


def test_known_unknown_and_self_trust_are_flat_open_range_agent_props() -> None:
    """awareOfNotKnowing and epistemicSelfTrust are owl:ObjectProperty with
    rdfs:domain gmeow:Agent, an OPEN range (Principle 13), FLAT (no
    rdfs:subPropertyOf — they reuse the inquiry / trust slices by reference, not
    by subsumption), and non-functional."""
    g = _graph()
    for name in ("awareOfNotKnowing", "epistemicSelfTrust"):
        prop = _t(name)
        assert (prop, RDF.type, OWL.ObjectProperty) in g, (
            f"{name} not an object property"
        )
        assert (prop, RDFS.domain, _t("Agent")) in g, f"{name} missing Agent domain"
        assert (prop, RDFS.range, None) not in g, f"{name} must keep an open range"
        assert (prop, RDFS.subPropertyOf, None) not in g, f"{name} must stay flat"
        assert (prop, RDF.type, OWL.FunctionalProperty) not in g


def test_reflection_is_an_event_type_individual() -> None:
    """eventTypeReflection is an individual of gmeow:EventType (the
    eventTypeDeception pattern), never a class/subclass."""
    g = _graph()
    reflection = _t("eventTypeReflection")
    assert (reflection, RDF.type, _t("EventType")) in g
    assert (reflection, RDF.type, OWL.Class) not in g
    assert (reflection, RDFS.subClassOf, None) not in g


def test_no_status_or_truth_bit() -> None:
    """No status / truth bit: metacognitive state rides reified claims and the
    calibration value vocabulary (Principle 6). None of isCalibrated /
    isReflected / isTrue / isFalse appears in ANY triple position, and no
    locally-declared property has an xsd:boolean range."""
    g = _graph()
    for name in ("isCalibrated", "isReflected", "isTrue", "isFalse", "isMiscalibrated"):
        term = _t(name)
        assert (term, None, None) not in g
        assert (None, term, None) not in g
        assert (None, None, term) not in g
    # No locally-declared property carries a boolean range.
    for name in _DECLARED_TERMS:
        assert (_t(name), RDFS.range, XSD.boolean) not in g


def test_bridges_are_documented_not_axiomatised() -> None:
    """Cross-slice bridges (Principle 9) are prose-only routing: the
    known-unknown → inquiry and self-trust → trust links assert no
    rdfs:subPropertyOf into the reused slices' properties."""
    g = _graph()
    assert (_t("awareOfNotKnowing"), RDFS.subPropertyOf, _t("evokes")) not in g
    assert (_t("epistemicSelfTrust"), RDFS.subPropertyOf, _t("trustor")) not in g
    assert (_t("epistemicSelfTrust"), RDFS.subPropertyOf, _t("trustLevel")) not in g


def test_every_declared_term_is_annotated() -> None:
    """Annotation-completeness (Principle 8): each of the 11 locally-declared
    terms — including the 3 calibration individuals and the reflection
    EventType — carries an rdfs:label, a skos:definition, and rdfs:isDefinedBy
    the metacognition slice IRI."""
    g = _graph()
    assert len(_DECLARED_TERMS) == 11
    for name in _DECLARED_TERMS:
        term = _t(name)
        assert (term, RDFS.label, None) in g, f"{name} missing rdfs:label"
        assert (term, SKOS_DEFINITION, None) in g, f"{name} missing skos:definition"
        assert (term, RDFS.isDefinedBy, SLICE_IRI) in g, (
            f"{name} missing rdfs:isDefinedBy slice IRI"
        )
