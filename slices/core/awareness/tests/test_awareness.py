# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: AGPL-3.0-only
"""Awareness slice — the state-of-the-experiencer axis and its invariants.

These structural assertions guard the load-bearing shape of the awareness slice,
the third orthogonal axis of mind (state of the experiencer) beside content
(imagination) and attitude (epistemics / imagination):

* ``gmeow:AwarenessMode`` and ``gmeow:AwarenessLevel`` are VALUE VOCABULARIES
  (``gufo:AbstractIndividualType`` ⊑ ``gufo:QualityValue``): their members are
  individuals, never subclasses (Principle 9, the ``ContentOrigin`` idiom). The 17
  modes (12 human + 5 machine) and the 6 levels are seeded as individuals.
* The machine modes are SIBLINGS of the human modes in ONE open vocabulary,
  bridged by analogy not equivalence (Principle 5) — they are typed as the vocab
  class, not subclassed.
* Each ``gmeow:AwarenessLevel`` carries an integer ``gmeow:levelRank``; the six
  ranks are exactly ``{0,1,2,3,4,5}`` (high arousal → low).
* ``gmeow:awarenessMode`` / ``gmeow:awarenessLevel`` keep an OPEN domain (the edge
  applies to a tenure OR directly to a process / agent) and are non-functional;
  ``gmeow:awarenessSubject`` carries the per-branch bearer edge (domain
  ``AwarenessTenure``, range ``gmeow:Agent``) — NOT ``gufo:inheresIn``.
* ``gmeow:AwarenessTenure`` is a ``gufo:SituationType`` ⊑
  ``gmeow:TimeScopedRelation`` (the temporal reification seam).
* There is NO truth or reality bit (no ``isReal`` / ``isTrue`` / ``isDream``); the
  module asserts NO ``gufo:inheresIn`` / ``gmeow:inheresIn`` triple, and consumes
  mentation / metacognition strictly by reference.
* The manifest's ``sliceDependsOn`` is exactly ``{kernel, temporal}``.
"""

from __future__ import annotations

from pathlib import Path

from rdflib import Graph, URIRef
from rdflib.namespace import OWL, RDF, RDFS, XSD

GMEOW = "https://blackcatinformatics.ca/gmeow/"
GUFO = "http://purl.org/nemo/gufo#"
SKOS_DEFINITION = URIRef("http://www.w3.org/2004/02/skos/core#definition")
SLICE_IRI = URIRef("https://blackcatinformatics.ca/gmeow/slices/awareness")
SLICE_DEPENDS_ON = URIRef(GMEOW + "sliceDependsOn")
_MODULE = Path(__file__).resolve().parents[1] / "module.ttl"
_MANIFEST = Path(__file__).resolve().parents[1] / "manifest.ttl"

_HUMAN_MODES = (
    "modeWaking",
    "modeDrowsy",
    "modeAsleep",
    "modeDreaming",
    "modeREM",
    "modeLucidDreaming",
    "modeMindWandering",
    "modeFocused",
    "modeFlow",
    "modeMeditative",
    "modeSedated",
    "modeComatose",
)
_MACHINE_MODES = (
    "modeOnlineInference",
    "modeOfflineReplay",
    "modeTraining",
    "modeSampling",
    "modeDormant",
)
_MODES = (*_HUMAN_MODES, *_MACHINE_MODES)
_LEVELS = (
    "levelHyperalert",
    "levelAlert",
    "levelRelaxed",
    "levelDrowsy",
    "levelObtunded",
    "levelUnresponsive",
)
_PROPERTIES = (
    "awarenessMode",
    "awarenessLevel",
    "awarenessScalar",
    "levelRank",
    "awarenessSubject",
)

# Every locally-declared term, by name (31 total): the 2 value classes, the 17
# mode individuals, the 6 level individuals, the AwarenessTenure class, and the 5
# properties (mode / level / scalar edges, the rank, and the subject bearer edge).
_DECLARED_TERMS = (
    "AwarenessMode",
    *_MODES,
    "AwarenessLevel",
    *_LEVELS,
    "AwarenessTenure",
    *_PROPERTIES,
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


def test_vocab_classes_are_abstract_individual_types() -> None:
    """AwarenessMode and AwarenessLevel are each an owl:Class, a
    gufo:AbstractIndividualType, and a subclass of gufo:QualityValue — the
    value-vocabulary genus shared with gmeow:ContentOrigin."""
    g = _graph()
    for cls in ("AwarenessMode", "AwarenessLevel"):
        term = _t(cls)
        assert (term, RDF.type, OWL.Class) in g
        assert (term, RDF.type, _gufo("AbstractIndividualType")) in g
        assert (term, RDFS.subClassOf, _gufo("QualityValue")) in g


def test_mode_individuals_are_seeded() -> None:
    """All 17 awareness modes (12 human + 5 machine) are individuals of
    gmeow:AwarenessMode — one open vocabulary holding the human and machine modes
    as siblings (Principle 5)."""
    g = _graph()
    assert len(_MODES) == 17
    for indiv in _MODES:
        assert (_t(indiv), RDF.type, _t("AwarenessMode")) in g


def test_level_individuals_are_seeded() -> None:
    """All 6 awareness levels are individuals of gmeow:AwarenessLevel."""
    g = _graph()
    assert len(_LEVELS) == 6
    for indiv in _LEVELS:
        assert (_t(indiv), RDF.type, _t("AwarenessLevel")) in g


def test_vocab_individuals_are_not_subclasses() -> None:
    """Value-vocab discipline (Principle 9, no overtyping): each mode and level is
    an INDIVIDUAL, never an owl:Class nor an rdfs:subClassOf its vocab class — the
    machine modes too are siblings, not a subclass hierarchy."""
    g = _graph()
    for indiv in _MODES:
        term = _t(indiv)
        assert (term, RDF.type, OWL.Class) not in g
        assert (term, RDFS.subClassOf, _t("AwarenessMode")) not in g
    for indiv in _LEVELS:
        term = _t(indiv)
        assert (term, RDF.type, OWL.Class) not in g
        assert (term, RDFS.subClassOf, _t("AwarenessLevel")) not in g


def test_level_ranks_are_zero_through_five() -> None:
    """Each level individual carries a gmeow:levelRank, and the six ranks are
    exactly {0,1,2,3,4,5} (high arousal → low)."""
    g = _graph()
    rank = _t("levelRank")
    ranks = set()
    for indiv in _LEVELS:
        values = list(g.objects(_t(indiv), rank))
        assert len(values) == 1, f"{indiv} should carry exactly one levelRank"
        ranks.add(int(values[0]))
    assert ranks == {0, 1, 2, 3, 4, 5}, f"unexpected rank set: {ranks}"


def test_property_types_and_ranges() -> None:
    """awarenessMode / awarenessLevel / awarenessSubject are owl:ObjectProperty;
    awarenessScalar is an owl:DatatypeProperty ranging over xsd:decimal; levelRank
    is an owl:DatatypeProperty ranging over xsd:integer."""
    g = _graph()
    for prop in ("awarenessMode", "awarenessLevel", "awarenessSubject"):
        assert (_t(prop), RDF.type, OWL.ObjectProperty) in g
    assert (_t("awarenessMode"), RDFS.range, _t("AwarenessMode")) in g
    assert (_t("awarenessLevel"), RDFS.range, _t("AwarenessLevel")) in g
    assert (_t("awarenessScalar"), RDF.type, OWL.DatatypeProperty) in g
    assert (_t("awarenessScalar"), RDFS.range, XSD.decimal) in g
    assert (_t("levelRank"), RDF.type, OWL.DatatypeProperty) in g
    assert (_t("levelRank"), RDFS.range, XSD.integer) in g
    assert (_t("levelRank"), RDFS.domain, _t("AwarenessLevel")) in g


def test_awareness_edges_open_domain() -> None:
    """OPEN-DOMAIN (Principle 13): awarenessMode and awarenessLevel assert NO
    rdfs:domain (the edge applies to an AwarenessTenure or directly to a process /
    agent) and are non-functional."""
    g = _graph()
    for prop in ("awarenessMode", "awarenessLevel", "awarenessScalar"):
        term = _t(prop)
        assert (term, RDFS.domain, None) not in g
        assert (term, RDF.type, OWL.FunctionalProperty) not in g


def test_awareness_subject_bearer_edge() -> None:
    """awarenessSubject is the per-branch bearer edge (Principle 4): it HAS an
    rdfs:domain of gmeow:AwarenessTenure and an rdfs:range of gmeow:Agent — the
    tenure carries its subject explicitly, never through gufo:inheresIn."""
    g = _graph()
    subj = _t("awarenessSubject")
    assert (subj, RDF.type, OWL.ObjectProperty) in g
    assert (subj, RDF.type, OWL.FunctionalProperty) in g
    assert (subj, RDFS.domain, _t("AwarenessTenure")) in g
    assert (subj, RDFS.range, _t("Agent")) in g


def test_awareness_tenure_is_a_time_scoped_situation() -> None:
    """AwarenessTenure is an owl:Class, a gufo:SituationType, and an
    rdfs:subClassOf gmeow:TimeScopedRelation — the temporal reification seam."""
    g = _graph()
    tenure = _t("AwarenessTenure")
    assert (tenure, RDF.type, OWL.Class) in g
    assert (tenure, RDF.type, _gufo("SituationType")) in g
    assert (tenure, RDFS.subClassOf, _t("TimeScopedRelation")) in g


def test_no_reality_or_truth_bit() -> None:
    """No reality or truth bit (Principle 9): awareness is a state of the
    experiencer, not a verdict on the content — none of isReal / isTrue / isFake /
    isDream / isImaginary appears in ANY triple position."""
    g = _graph()
    for name in ("isReal", "isTrue", "isFake", "isDream", "isImaginary", "isFalse"):
        term = _t(name)
        assert (term, None, None) not in g
        assert (None, term, None) not in g
        assert (None, None, term) not in g


def test_by_reference_no_inherence_triple() -> None:
    """By-reference discipline (Principle 4 / Principle 5): the subject is carried
    by gmeow:awarenessSubject, so the module asserts NO gufo:inheresIn and NO
    gmeow:inheresIn triple (the gUFO inherence alignment target is left
    untouched)."""
    g = _graph()
    for term in (_gufo("inheresIn"), _t("inheresIn")):
        assert (term, None, None) not in g
        assert (None, term, None) not in g
        assert (None, None, term) not in g


def test_manifest_depends_only_on_kernel_and_temporal() -> None:
    """Manifest dependency hygiene (no over-declaration): the asserted foreign IRIs
    are gmeow:Agent (kernel) and the time-scoped-relation reification seam
    (temporal), so sliceDependsOn is exactly {kernel, temporal} — mentation,
    metacognition, and imagination are consumed by reference, never declared."""
    g = Graph()
    g.parse(_MANIFEST, format="turtle")
    deps = set(g.objects(SLICE_IRI, SLICE_DEPENDS_ON))
    assert deps == {
        URIRef(GMEOW + "slices/kernel"),
        URIRef(GMEOW + "slices/temporal"),
    }, f"unexpected deps: {deps}"


def test_every_declared_term_is_annotated() -> None:
    """Annotation-completeness (Principle 8): each of the 31 locally-declared terms
    — the 2 value classes, the 17 mode individuals, the 6 level individuals, the
    AwarenessTenure class, and the 5 properties — carries an rdfs:label, a
    skos:definition, and rdfs:isDefinedBy the awareness slice IRI."""
    g = _graph()
    assert len(_DECLARED_TERMS) == 31
    for name in _DECLARED_TERMS:
        term = _t(name)
        assert (term, RDFS.label, None) in g, f"{name} missing rdfs:label"
        assert (term, SKOS_DEFINITION, None) in g, f"{name} missing skos:definition"
        assert (term, RDFS.isDefinedBy, SLICE_IRI) in g, (
            f"{name} missing rdfs:isDefinedBy slice IRI"
        )
