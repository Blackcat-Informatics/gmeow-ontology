# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: Apache-2.0
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


def test_no_factivity_no_truth_bit() -> None:
    """No isTrue/truth term anywhere, and knowsThat smuggles in no range/factivity."""
    g = _graph()
    assert (_t("isTrue"), None, None) not in g
    assert (_t("knowsThat"), RDFS.range, None) not in g


def test_every_term_is_annotated() -> None:
    """Annotation-completeness for the slice's own terms (Principle 8)."""
    g = _graph()
    for name in ("Proposition", *_SPINE):
        term = _t(name)
        assert (term, RDFS.label, None) in g
        assert (term, SKOS_DEFINITION, None) in g
        assert (term, RDFS.isDefinedBy, None) in g
