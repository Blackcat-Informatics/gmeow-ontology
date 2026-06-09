"""Deception · Structural foundation (issue #213).

Tests the deception module: event type, divergence properties, veridicality
vocabulary, deception roles, bullshit modality, attestation type fact-check,
SHACL shapes, and the no-isFalse doctrine.
"""

from __future__ import annotations

from rdflib import OWL, RDF, RDFS, Graph, Namespace, URIRef

from gmeow_tools.graph import load_merged_graph
from gmeow_tools.validate import run_shacl

GMEOW = Namespace("https://blackcatinformatics.ca/gmeow/")
GUFO = Namespace("http://purl.org/nemo/gufo#")
EX = Namespace("https://example.org/test/")


def _graph() -> Graph:
    return load_merged_graph(include_imports=False)


def test_deception_event_type_exists() -> None:
    graph = _graph()
    assert (GMEOW.eventTypeDeception, RDF.type, GMEOW.EventType) in graph


def test_divergence_properties_exist() -> None:
    graph = _graph()
    for prop in (GMEOW.heldStandpoint, GMEOW.projectedStandpoint):
        assert (prop, RDF.type, OWL.ObjectProperty) in graph
        assert (prop, RDFS.domain, GMEOW.Event) in graph
        assert (prop, RDFS.range, GMEOW.StandpointClaim) in graph


def test_deceptive_intent_claim_property_exists() -> None:
    graph = _graph()
    prop = GMEOW.deceptiveIntentClaim
    assert (prop, RDF.type, OWL.ObjectProperty) in graph
    assert (prop, RDFS.domain, GMEOW.Event) in graph
    assert (prop, RDFS.range, GMEOW.StandpointClaim) in graph


def test_implicates_property_exists() -> None:
    graph = _graph()
    prop = GMEOW.implicates
    assert (prop, RDF.type, OWL.ObjectProperty) in graph
    assert (prop, RDFS.domain, GMEOW.Event) in graph


def test_deception_cue_property_exists() -> None:
    graph = _graph()
    prop = GMEOW.deceptionCue
    assert (prop, RDF.type, OWL.ObjectProperty) in graph
    assert (prop, RDFS.domain, GMEOW.Event) in graph
    assert (prop, RDFS.range, GMEOW.Observation) in graph


def test_deception_roles_exist() -> None:
    graph = _graph()
    for role in (
        GMEOW.roleDeceiver,
        GMEOW.roleDeceived,
        GMEOW.roleBeneficiaryOfDeception,
        GMEOW.roleDupe,
    ):
        assert (role, RDF.type, GMEOW.ParticipantRole) in graph


def test_veridicality_values_exist() -> None:
    graph = _graph()
    assert (GMEOW.ClaimVeridicality, RDF.type, OWL.Class) in graph
    assert (GMEOW.ClaimVeridicality, RDFS.subClassOf, GUFO.QualityValue) in graph
    for val in (GMEOW.veridicalityUntrue, GMEOW.veridicalityLicensedFalsehood):
        assert (val, RDF.type, GMEOW.ClaimVeridicality) in graph


def test_bullshit_modality_exists() -> None:
    graph = _graph()
    assert (GMEOW.bullshit, RDF.type, GMEOW.StandpointModality) in graph


def test_attestation_type_fact_check_exists() -> None:
    graph = _graph()
    assert (GMEOW.attestationTypeFactCheck, RDF.type, GMEOW.AttestationType) in graph


def test_no_is_false_axiom() -> None:
    """Negative guard: there must be no isFalse or isDeceptive property."""
    graph = _graph()
    for forbidden in ("isFalse", "isDeceptive"):
        prop = URIRef(str(GMEOW) + forbidden)
        assert (prop, RDF.type, OWL.ObjectProperty) not in graph
        assert (prop, RDF.type, OWL.DatatypeProperty) not in graph
        assert (prop, RDF.type, OWL.AnnotationProperty) not in graph


def test_standpoint_divergence_coexists() -> None:
    """Principle 9: held and projected standpoints are coexisting claims,
    neither privileged. The graph must permit both on the same event."""
    g = Graph()
    g.add((EX.event1, RDF.type, GMEOW.Event))
    g.add((EX.event1, GMEOW.eventType, GMEOW.eventTypeDeception))
    g.add((EX.event1, GMEOW.heldStandpoint, EX.claimA))
    g.add((EX.event1, GMEOW.projectedStandpoint, EX.claimB))
    g.add((EX.claimA, RDF.type, GMEOW.StandpointClaim))
    g.add((EX.claimB, RDF.type, GMEOW.StandpointClaim))
    g.add((EX.claimA, GMEOW.observationMethod, EX.method1))
    g.add((EX.claimB, GMEOW.observationMethod, EX.method1))
    g.add((EX.method1, RDF.type, GMEOW.ObservationMethod))

    result = run_shacl(g)
    assert result.ok, "\n".join(result.errors)


def test_deception_event_shacl_passes() -> None:
    """A fully-populated deception event passes SHACL."""
    g = Graph()
    g.add((EX.event1, RDF.type, GMEOW.Event))
    g.add((EX.event1, GMEOW.eventType, GMEOW.eventTypeDeception))
    g.add((EX.event1, GMEOW.heldStandpoint, EX.claimA))
    g.add((EX.event1, GMEOW.projectedStandpoint, EX.claimB))
    g.add((EX.claimA, RDF.type, GMEOW.StandpointClaim))
    g.add((EX.claimB, RDF.type, GMEOW.StandpointClaim))
    g.add((EX.claimA, GMEOW.observationMethod, EX.method1))
    g.add((EX.claimB, GMEOW.observationMethod, EX.method1))
    g.add((EX.method1, RDF.type, GMEOW.ObservationMethod))
    g.add((EX.cue1, RDF.type, GMEOW.Observation))
    g.add((EX.cue1, GMEOW.vantage, EX.analyst))
    g.add((EX.cue1, GMEOW.observationResult, EX.result1))
    g.add((EX.cue1, GMEOW.observedFeature, EX.event1))
    g.add((EX.event1, GMEOW.deceptionCue, EX.cue1))

    result = run_shacl(g)
    assert result.ok, "\n".join(result.errors)


def test_deception_cue_shacl_passes() -> None:
    """A deception cue observation passes SHACL."""
    g = Graph()
    g.add((EX.event1, RDF.type, GMEOW.Event))
    g.add((EX.event1, GMEOW.eventType, GMEOW.eventTypeDeception))
    g.add((EX.event1, GMEOW.heldStandpoint, EX.claimA))
    g.add((EX.event1, GMEOW.projectedStandpoint, EX.claimB))
    g.add((EX.cue1, RDF.type, GMEOW.Observation))
    g.add((EX.cue1, GMEOW.vantage, EX.analyst))
    g.add((EX.cue1, GMEOW.observationResult, EX.result1))
    g.add((EX.cue1, GMEOW.observedFeature, EX.event1))
    g.add((EX.event1, GMEOW.deceptionCue, EX.cue1))

    result = run_shacl(g)
    assert result.ok, "\n".join(result.errors)


def test_licensed_falsehood_is_not_deception() -> None:
    """A claim tagged as licensed falsehood is structurally distinct from
    deception — the safety property is explicit."""
    graph = _graph()
    assert (
        GMEOW.veridicalityLicensedFalsehood,
        RDF.type,
        GMEOW.ClaimVeridicality,
    ) in graph
    # Licensed falsehood and untrue are siblings, neither subsumes the other.
    assert (
        GMEOW.veridicalityLicensedFalsehood,
        RDFS.subClassOf,
        GMEOW.veridicalityUntrue,
    ) not in graph
    assert (
        GMEOW.veridicalityUntrue,
        RDFS.subClassOf,
        GMEOW.veridicalityLicensedFalsehood,
    ) not in graph
