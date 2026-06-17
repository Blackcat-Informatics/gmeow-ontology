# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: AGPL-3.0-only
"""Epistemics slice — the keystone entailment and the no-truth-bit invariants.

These structural assertions guard the minimal core of the epistemics slice: the
flat, open-range doxastic spine and the single load-bearing axiom
``gmeow:knowsThat rdfs:subPropertyOf gmeow:believes``. The slice is deliberately
non-factive — no ``isTrue`` term, no truth datatype, no factivity axiom — so the
reasoner never promotes justified-true-belief to knowledge (Principle 12).
"""

from __future__ import annotations

from pathlib import Path

from rdflib import Graph, URIRef
from rdflib.collection import Collection
from rdflib.namespace import OWL, RDF, RDFS

GMEOW = "https://blackcatinformatics.ca/gmeow/"
SKOS_DEFINITION = URIRef("http://www.w3.org/2004/02/skos/core#definition")
_MODULE = Path(__file__).resolve().parents[1] / "module.ttl"
_SPINE = ("believes", "doubts", "suspendsJudgementOn", "accepts", "knowsThat")


def _t(name: str) -> URIRef:
    """A gmeow-namespaced term URI."""
    return URIRef(GMEOW + name)


def _graph() -> Graph:
    g = Graph()
    g.parse(_MODULE, format="turtle")
    return g


def test_knows_that_subproperty_of_believes() -> None:
    """The keystone: knowsThat is a subproperty of believes."""
    g = _graph()
    assert (_t("knowsThat"), RDFS.subPropertyOf, _t("believes")) in g


def test_spine_are_object_properties_with_agent_domain() -> None:
    g = _graph()
    for prop in _SPINE:
        term = _t(prop)
        assert (term, RDF.type, OWL.ObjectProperty) in g
        assert (term, RDFS.domain, _t("Agent")) in g


def test_spine_have_open_range() -> None:
    """The flat spine is open-range (Principle 13) — no rdfs:range asserted."""
    g = _graph()
    for prop in _SPINE:
        assert (_t(prop), RDFS.range, None) not in g


def test_proposition_is_a_social_object() -> None:
    g = _graph()
    assert (_t("Proposition"), RDFS.subClassOf, _t("SocialObject")) in g


def test_spine_properties_are_not_functional() -> None:
    """The doxastic spine is non-functional — an agent holds many attitudes and
    contested ones coexist (Principle 9); no spine property is declared
    owl:FunctionalProperty (the no-functional-declaration invariant, #559)."""
    g = _graph()
    for prop in _SPINE:
        assert (_t(prop), RDF.type, OWL.FunctionalProperty) not in g


def test_no_factivity_no_truth_bit() -> None:
    """No isTrue/truth term in ANY triple position, and knowsThat smuggles in no
    range/factivity."""
    g = _graph()
    is_true = _t("isTrue")
    assert (is_true, None, None) not in g
    assert (None, is_true, None) not in g
    assert (None, None, is_true) not in g
    assert (_t("knowsThat"), RDFS.range, None) not in g


def _union_members(g: Graph, expr: URIRef) -> set[URIRef]:
    """Return the URIs inside an owl:unionOf class expression, if any."""
    list_node = g.value(expr, OWL.unionOf)
    if list_node is None:
        return set()
    return set(Collection(g, list_node))


# ---------------------------------------------------------------------------
# Issue #561 — Epistemic justification terms (Tasks 1 & 2).
# ---------------------------------------------------------------------------


def test_doxastic_standpoint_claim_is_subclass_of_standpoint_claim() -> None:
    g = _graph()
    assert (_t("DoxasticStandpointClaim"), RDFS.subClassOf, _t("StandpointClaim")) in g


def test_claim_of_belief_is_functional_object_property() -> None:
    g = _graph()
    prop = _t("claimOfBelief")
    assert (prop, RDF.type, OWL.ObjectProperty) in g
    assert (prop, RDF.type, OWL.FunctionalProperty) in g
    assert (prop, RDFS.domain, _t("DoxasticStandpointClaim")) in g
    assert (prop, RDFS.range, _t("DoxasticState")) in g


def test_justified_by_has_union_domain_and_range() -> None:
    g = _graph()
    prop = _t("justifiedBy")
    assert (prop, RDF.type, OWL.ObjectProperty) in g

    domain = g.value(prop, RDFS.domain)
    assert domain is not None
    domain_members = _union_members(g, domain)
    assert _t("DoxasticState") in domain_members
    assert _t("StandpointClaim") in domain_members

    range_ = g.value(prop, RDFS.range)
    assert range_ is not None
    range_members = _union_members(g, range_)
    assert _t("EvidenceSpan") in range_members
    assert _t("Attestation") in range_members
    assert _t("DoxasticState") in range_members


def test_defeated_by_has_status_range() -> None:
    g = _graph()
    prop = _t("defeatedBy")
    assert (prop, RDF.type, OWL.ObjectProperty) in g
    assert (prop, RDFS.range, _t("JustificationStatus")) in g


def test_justification_status_individuals_exist() -> None:
    g = _graph()
    for name in ("justificationStatusGettier", "justificationStatusDefeated"):
        assert (_t(name), RDF.type, _t("JustificationStatus")) in g


def test_justification_terms_are_annotated() -> None:
    """Annotation-completeness for the new #561 terms (Principle 8)."""
    g = _graph()
    for name in (
        "DoxasticStandpointClaim",
        "claimOfBelief",
        "justifiedBy",
        "defeatedBy",
        "JustificationStatus",
    ):
        term = _t(name)
        assert (term, RDFS.label, None) in g
        assert (term, SKOS_DEFINITION, None) in g
        assert (term, RDFS.isDefinedBy, None) in g


def test_every_term_is_annotated() -> None:
    """Annotation-completeness for the slice's own terms (Principle 8)."""
    g = _graph()
    for name in ("Proposition", *_SPINE):
        term = _t(name)
        assert (term, RDFS.label, None) in g
        assert (term, SKOS_DEFINITION, None) in g
        assert (term, RDFS.isDefinedBy, None) in g
