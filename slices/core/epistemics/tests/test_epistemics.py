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
from rdflib.term import Node

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


def _union_members(g: Graph, expr: Node) -> set[URIRef]:
    """Return the URIs inside an owl:unionOf class expression, if any.

    Handles both direct union expressions and unions nested via
    owl:equivalentClass (used for schema-friendly named union classes).
    """
    list_node = g.value(expr, OWL.unionOf)
    if list_node is None:
        equivalent = g.value(expr, OWL.equivalentClass)
        if equivalent is not None:
            list_node = g.value(equivalent, OWL.unionOf)
        if list_node is None:
            return set()
    return {member for member in Collection(g, list_node) if isinstance(member, URIRef)}


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


def test_justified_by_has_named_domain_and_range() -> None:
    g = _graph()
    prop = _t("justifiedBy")
    assert (prop, RDF.type, OWL.ObjectProperty) in g

    # Domain/range are now schema-friendly named classes instead of blank unions.
    assert (prop, RDFS.domain, _t("JustificationSubject")) in g
    assert (prop, RDFS.range, _t("JustificationGround")) in g

    subject_union = _union_members(g, _t("JustificationSubject"))
    assert subject_union == {_t("DoxasticState"), _t("StandpointClaim")}

    ground_union = _union_members(g, _t("JustificationGround"))
    assert ground_union == {_t("EvidenceSpan"), _t("Attestation"), _t("DoxasticState")}


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
        "JustificationSubject",
        "JustificationGround",
    ):
        term = _t(name)
        assert (term, RDFS.label, None) in g
        assert (term, SKOS_DEFINITION, None) in g
        assert (term, RDFS.isDefinedBy, None) in g

    for status in g.subjects(RDF.type, _t("JustificationStatus")):
        assert (status, RDFS.label, None) in g
        assert (status, SKOS_DEFINITION, None) in g
        assert (status, RDFS.isDefinedBy, None) in g


def test_every_term_is_annotated() -> None:
    """Annotation-completeness for the slice's own terms (Principle 8)."""
    g = _graph()
    for name in ("Proposition", *_SPINE):
        term = _t(name)
        assert (term, RDFS.label, None) in g
        assert (term, SKOS_DEFINITION, None) in g
        assert (term, RDFS.isDefinedBy, None) in g


def test_credence_and_confidence_are_distinct() -> None:
    """credence lives on DoxasticState; confidence belongs to the statement layer.

    They are separate terms, neither subsumes the other, and only credence is
    declared with domain gmeow:DoxasticState in this slice (Principle 6).
    """
    g = _graph()
    credence = _t("credence")
    confidence = _t("confidence")
    assert credence != confidence
    assert (credence, RDFS.domain, _t("DoxasticState")) in g
    assert (confidence, RDFS.domain, _t("DoxasticState")) not in g
    assert (credence, RDFS.subPropertyOf, confidence) not in g
    assert (confidence, RDFS.subPropertyOf, credence) not in g


def test_justified_by_property_constraints() -> None:
    """gmeow:justifiedBy is an open-range, non-functional object property hook."""
    g = _graph()
    justified_by = _t("justifiedBy")
    assert (justified_by, RDF.type, OWL.ObjectProperty) in g
    assert (justified_by, RDFS.domain, _t("DoxasticState")) in g
    assert (justified_by, RDFS.range, None) not in g
    assert (justified_by, RDF.type, OWL.FunctionalProperty) not in g


def test_suppression_round_trip() -> None:
    """The flagship example retains superseded states and suppresses the tenure.

    A defeater closes the original tenure (endedAtTime) and marks it
    gmeow:displayable false, while the revised tenure stays open.  Both the
    original and revised DoxasticState individuals remain in the ledger
    (Principle 10); the original credence exceeds the revised one.
    """
    g = Graph()
    flagship = _MODULE.parent / "examples" / "flagship-epistemic-ledger.ttl"
    g.parse(flagship, format="turtle")

    tenures = list(g.subjects(RDF.type, _t("DoxasticTenure")))
    assert len(tenures) == 2

    original: URIRef | None = None
    revised: URIRef | None = None
    for tenure in tenures:
        interval = g.value(tenure, _t("duringInterval"))
        if interval is not None and (interval, _t("endedAtTime"), None) in g:
            original = tenure
        else:
            revised = tenure

    assert original is not None
    assert revised is not None
    assert original != revised

    from rdflib import Literal

    assert (original, _t("displayable"), Literal(False)) in g
    assert (revised, _t("endedAtTime"), None) not in g

    original_state = g.value(original, _t("tenureOfDoxasticState"))
    revised_state = g.value(revised, _t("tenureOfDoxasticState"))
    assert isinstance(original_state, URIRef)
    assert isinstance(revised_state, URIRef)
    assert original_state != revised_state
    assert (original_state, RDF.type, _t("DoxasticState")) in g
    assert (revised_state, RDF.type, _t("DoxasticState")) in g

    original_cred = g.value(original_state, _t("credence"))
    revised_cred = g.value(revised_state, _t("credence"))
    assert original_cred is not None
    assert revised_cred is not None
    assert float(original_cred) > float(revised_cred)


def test_epistemics_mapping_set_exists_and_has_expected_rows() -> None:
    """The generated SSSOM mapping set for epistemics contains expected subjects."""
    import csv

    mapping = (
        _MODULE.parents[3] / "generated" / "mappings" / "gmeow-epistemics.sssom.tsv"
    )
    assert mapping.exists(), f"Missing mapping file: {mapping}"

    with mapping.open("r", encoding="utf-8") as fh:
        lines = [line for line in fh if not line.startswith("#")]
        reader = csv.DictReader(lines, delimiter="\t")
        subjects = {row["subject_id"] for row in reader if row.get("subject_id")}

    expected = {
        "gmeow:DoxasticState",
        "gmeow:Proposition",
        "gmeow:believes",
        "gmeow:knowsThat",
        "gmeow:justifiedBy",
    }
    assert expected.issubset(subjects), (
        f"Missing subjects in mapping: {expected - subjects}"
    )


def test_flagship_example_parses() -> None:
    """The flagship epistemic ledger is valid Turtle."""
    g = Graph()
    flagship = _MODULE.parent / "examples" / "flagship-epistemic-ledger.ttl"
    g.parse(flagship, format="turtle")
    assert len(g) > 0
