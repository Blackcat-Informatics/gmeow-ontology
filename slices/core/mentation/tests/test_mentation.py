# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: AGPL-3.0-only
"""Mentation slice — structural invariants for the occurrent mind.

Guards the minimal core of the mentation slice:
  * gmeow:MentalProcess ⊑ gmeow:Event (the occurrent umbrella)
  * gmeow:Experience ⊑ gmeow:MentalProcess (the phenomenal/qualia subset)
  * gmeow:experiencer is a functional ObjectProperty (one process, one experiencer)
  * gmeow:mentalProcessType is a NON-functional ObjectProperty (a process may have
    several type values simultaneously)
  * gmeow:MentalProcessType is a value-vocab class (gufo:AbstractIndividualType,
    ⊑ gufo:QualityValue — no subclasses, only individuals; Principle 9)
  * all eight gmeow:process* seed individuals are declared as MentalProcessType
  * gmeow:realizesMentalMoment is non-functional, domain MentalProcess,
    range MentalMoment
  * gmeow:producesMentalMoment is non-functional, domain MentalProcess,
    range MentalMoment
  * gmeow:updatesMentalTenure is non-functional, domain MentalProcess,
    range TimeScopedRelation
  * gmeow:realizes is NOT declared here (owned by creative-works; Principle 4)
  * annotation completeness for all 16 terms (Principle 8)
"""

from __future__ import annotations

from pathlib import Path

from rdflib import Graph, URIRef
from rdflib.namespace import OWL, RDF, RDFS

GMEOW = "https://blackcatinformatics.ca/gmeow/"
GUFO = "http://purl.org/nemo/gufo#"
SKOS_DEFINITION = URIRef("http://www.w3.org/2004/02/skos/core#definition")
_MODULE = Path(__file__).resolve().parents[1] / "module.ttl"

_SLICE_IRI = URIRef(GMEOW + "slices/mentation")

_CLASSES = ("MentalProcess", "Experience", "MentalProcessType")
_PROPERTIES = (
    "experiencer",
    "mentalProcessType",
    "realizesMentalMoment",
    "producesMentalMoment",
    "updatesMentalTenure",
)
_INDIVIDUALS = (
    "processPerception",
    "processAttention",
    "processReasoning",
    "processImagining",
    "processDeliberation",
    "processRecollection",
    "processMindWandering",
    "processDreaming",
)
_ALL_TERMS = list(_CLASSES) + list(_PROPERTIES) + list(_INDIVIDUALS)


def _t(name: str) -> URIRef:
    """A gmeow-namespaced term URI."""
    return URIRef(GMEOW + name)


def _g(name: str) -> URIRef:
    """A gufo-namespaced term URI."""
    return URIRef(GUFO + name)


def _graph() -> Graph:
    g = Graph()
    g.parse(_MODULE, format="turtle")
    return g


# --------------------------------------------------------------------------- #
# Subsumption
# --------------------------------------------------------------------------- #


def test_mentalprocess_subclass_of_event() -> None:
    """MentalProcess ⊑ Event — the occurrent umbrella."""
    g = _graph()
    assert (_t("MentalProcess"), RDFS.subClassOf, _t("Event")) in g


def test_experience_subclass_of_mentalprocess() -> None:
    """Experience ⊑ MentalProcess — the phenomenal subset."""
    g = _graph()
    assert (_t("Experience"), RDFS.subClassOf, _t("MentalProcess")) in g


# --------------------------------------------------------------------------- #
# experiencer property
# --------------------------------------------------------------------------- #


def test_experiencer_is_functional_object_property() -> None:
    """experiencer: FunctionalProperty, domain MentalProcess, range Agent."""
    g = _graph()
    exp = _t("experiencer")
    assert (exp, RDF.type, OWL.ObjectProperty) in g
    assert (exp, RDF.type, OWL.FunctionalProperty) in g
    assert (exp, RDFS.domain, _t("MentalProcess")) in g
    assert (exp, RDFS.range, _t("Agent")) in g


# --------------------------------------------------------------------------- #
# mentalProcessType property
# --------------------------------------------------------------------------- #


def test_mentalprocesstype_property() -> None:
    """mentalProcessType: non-functional ObjectProperty.

    domain MentalProcess, range MentalProcessType.
    """
    g = _graph()
    mpt = _t("mentalProcessType")
    assert (mpt, RDF.type, OWL.ObjectProperty) in g
    # intentionally non-functional:
    assert (mpt, RDF.type, OWL.FunctionalProperty) not in g
    assert (mpt, RDFS.domain, _t("MentalProcess")) in g
    assert (mpt, RDFS.range, _t("MentalProcessType")) in g


# --------------------------------------------------------------------------- #
# MentalProcessType value-vocab invariants
# --------------------------------------------------------------------------- #


def test_mentalprocesstype_is_value_vocab() -> None:
    """MentalProcessType: owl:Class + gufo:AbstractIndividualType.

    Must be ⊑ gufo:QualityValue (Principle 9 value-vocab).
    """
    g = _graph()
    mpt_class = _t("MentalProcessType")
    assert (mpt_class, RDF.type, OWL.Class) in g
    assert (mpt_class, RDF.type, _g("AbstractIndividualType")) in g
    assert (mpt_class, RDFS.subClassOf, _g("QualityValue")) in g


def test_all_eight_process_individuals() -> None:
    """All eight gmeow:process* seeds are declared as gmeow:MentalProcessType."""
    g = _graph()
    for name in _INDIVIDUALS:
        individual = _t(name)
        assert (individual, RDF.type, _t("MentalProcessType")) in g, (
            f"gmeow:{name} is not declared as gmeow:MentalProcessType"
        )


# --------------------------------------------------------------------------- #
# Three bridge properties — non-functional, precise ranges
# --------------------------------------------------------------------------- #


def test_realizesmentalmoment_property() -> None:
    """realizesMentalMoment: non-functional ObjectProperty.

    domain MentalProcess, range MentalMoment.
    """
    g = _graph()
    prop = _t("realizesMentalMoment")
    assert (prop, RDF.type, OWL.ObjectProperty) in g
    assert (prop, RDFS.domain, _t("MentalProcess")) in g
    assert (prop, RDFS.range, _t("MentalMoment")) in g
    # Non-functional: one process may manifest several moments
    assert (prop, RDF.type, OWL.FunctionalProperty) not in g


def test_producesmentalmoment_property() -> None:
    """producesMentalMoment: non-functional ObjectProperty.

    domain MentalProcess, range MentalMoment.
    """
    g = _graph()
    prop = _t("producesMentalMoment")
    assert (prop, RDF.type, OWL.ObjectProperty) in g
    assert (prop, RDFS.domain, _t("MentalProcess")) in g
    assert (prop, RDFS.range, _t("MentalMoment")) in g
    # Non-functional: one process may produce several moments
    assert (prop, RDF.type, OWL.FunctionalProperty) not in g


def test_updatesmentaltenure_property() -> None:
    """updatesMentalTenure: non-functional ObjectProperty.

    domain MentalProcess, range TimeScopedRelation.
    """
    g = _graph()
    prop = _t("updatesMentalTenure")
    assert (prop, RDF.type, OWL.ObjectProperty) in g
    assert (prop, RDFS.domain, _t("MentalProcess")) in g
    assert (prop, RDFS.range, _t("TimeScopedRelation")) in g
    # Non-functional: one process may update several tenures
    assert (prop, RDF.type, OWL.FunctionalProperty) not in g


# --------------------------------------------------------------------------- #
# gmeow:realizes collision guard
# --------------------------------------------------------------------------- #


def test_realizes_collision_guard() -> None:
    """gmeow:realizes must NOT appear here (creative-works owns it — Principle 4).

    All three bridge properties MUST be declared.
    """
    g = _graph()
    realizes = _t("realizes")
    # gmeow:realizes must not appear in ANY triple position in this module
    # (subject, predicate, or object) — creative-works owns it (Principle 4).
    msg = (
        "gmeow:realizes must not be in mentation (creative-works owns it, Principle 4)"
    )
    assert next(g.triples((realizes, None, None)), None) is None, msg
    assert next(g.triples((None, realizes, None)), None) is None, msg
    assert next(g.triples((None, None, realizes)), None) is None, msg
    # All three bridge properties must be declared
    for prop_name in (
        "realizesMentalMoment",
        "producesMentalMoment",
        "updatesMentalTenure",
    ):
        prop = _t(prop_name)
        assert (prop, RDF.type, None) in g, (
            f"gmeow:{prop_name} must be declared in mentation"
        )


# --------------------------------------------------------------------------- #
# Annotation completeness (Principle 8)
# --------------------------------------------------------------------------- #


def test_every_term_annotated() -> None:
    """All 16 terms: rdfs:label, skos:definition, rdfs:isDefinedBy → mentation slice."""
    g = _graph()
    for name in _ALL_TERMS:
        term = _t(name)
        assert (term, RDFS.label, None) in g, f"gmeow:{name} missing rdfs:label"
        assert (term, SKOS_DEFINITION, None) in g, (
            f"gmeow:{name} missing skos:definition"
        )
        assert (term, RDFS.isDefinedBy, None) in g, (
            f"gmeow:{name} missing rdfs:isDefinedBy"
        )
        # isDefinedBy must point at the mentation slice IRI
        defined_by = list(g.objects(term, RDFS.isDefinedBy))
        assert _SLICE_IRI in defined_by, (
            f"gmeow:{name} rdfs:isDefinedBy must be {_SLICE_IRI!s}, got {defined_by}"
        )
