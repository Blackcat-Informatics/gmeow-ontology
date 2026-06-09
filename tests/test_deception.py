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
    assert (prop, RDFS.range, GMEOW.Entity) in graph


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


# ===========================================================================
# Issue #215 — Speech-act deception types.
# ===========================================================================


def test_event_type_lie_exists() -> None:
    graph = _graph()
    assert (GMEOW.eventTypeLie, RDF.type, GMEOW.EventType) in graph


def test_event_type_paltering_exists() -> None:
    graph = _graph()
    assert (GMEOW.eventTypePaltering, RDF.type, GMEOW.EventType) in graph


def test_event_type_omission_exists() -> None:
    graph = _graph()
    assert (GMEOW.eventTypeOmission, RDF.type, GMEOW.EventType) in graph


def test_event_type_distortion_exists() -> None:
    graph = _graph()
    assert (GMEOW.eventTypeDistortion, RDF.type, GMEOW.EventType) in graph


def test_event_type_bullshit_exists() -> None:
    graph = _graph()
    assert (GMEOW.eventTypeBullshit, RDF.type, GMEOW.EventType) in graph


def test_event_type_self_deception_exists() -> None:
    graph = _graph()
    assert (GMEOW.eventTypeSelfDeception, RDF.type, GMEOW.EventType) in graph


def test_role_spin_doctor_exists() -> None:
    graph = _graph()
    assert (GMEOW.roleSpinDoctor, RDF.type, GMEOW.ParticipantRole) in graph


def test_maxim_violation_values_exist() -> None:
    graph = _graph()
    assert (GMEOW.MaximViolationType, RDF.type, OWL.Class) in graph
    assert (GMEOW.MaximViolationType, RDFS.subClassOf, GUFO.QualityValue) in graph
    for val in (
        GMEOW.maximViolationQuality,
        GMEOW.maximViolationQuantity,
        GMEOW.maximViolationRelation,
        GMEOW.maximViolationManner,
    ):
        assert (val, RDF.type, GMEOW.MaximViolationType) in graph


def test_paltering_implicates_structure() -> None:
    """A paltering event can carry gmeow:implicates to a proposition.
    The implicates property domain is Event and range is Entity."""
    g = Graph()
    g.add((EX.event1, RDF.type, GMEOW.Event))
    g.add((EX.event1, GMEOW.eventType, GMEOW.eventTypePaltering))
    g.add((EX.event1, GMEOW.implicates, EX.propositionPprime))
    g.add((EX.event1, GMEOW.heldStandpoint, EX.claimA))
    g.add((EX.event1, GMEOW.projectedStandpoint, EX.claimB))
    g.add((EX.claimA, RDF.type, GMEOW.StandpointClaim))
    g.add((EX.claimB, RDF.type, GMEOW.StandpointClaim))
    g.add((EX.claimA, GMEOW.observationMethod, EX.method1))
    g.add((EX.claimB, GMEOW.observationMethod, EX.method1))
    g.add((EX.method1, RDF.type, GMEOW.ObservationMethod))

    result = run_shacl(g)
    assert result.ok, "\n".join(result.errors)


def test_self_deception_same_agent() -> None:
    """Self-deception: the same agent can bear both deceiver and deceived roles
    on the same event via distinct Participation relators (Principle 9)."""
    g = Graph()
    g.add((EX.event1, RDF.type, GMEOW.Event))
    g.add((EX.event1, GMEOW.eventType, GMEOW.eventTypeSelfDeception))
    g.add((EX.event1, GMEOW.heldStandpoint, EX.claimA))
    g.add((EX.event1, GMEOW.projectedStandpoint, EX.claimB))
    g.add((EX.claimA, RDF.type, GMEOW.StandpointClaim))
    g.add((EX.claimB, RDF.type, GMEOW.StandpointClaim))
    g.add((EX.claimA, GMEOW.observationMethod, EX.method1))
    g.add((EX.claimB, GMEOW.observationMethod, EX.method1))
    g.add((EX.method1, RDF.type, GMEOW.ObservationMethod))
    # Same agent in both roles via distinct participations.
    g.add((EX.partDeceiver, RDF.type, GMEOW.Participation))
    g.add((EX.partDeceiver, GMEOW.participationEvent, EX.event1))
    g.add((EX.partDeceiver, GMEOW.participationParticipant, EX.agent1))
    g.add((EX.partDeceiver, GMEOW.participationRole, GMEOW.roleDeceiver))
    g.add((EX.partDeceived, RDF.type, GMEOW.Participation))
    g.add((EX.partDeceived, GMEOW.participationEvent, EX.event1))
    g.add((EX.partDeceived, GMEOW.participationParticipant, EX.agent1))
    g.add((EX.partDeceived, GMEOW.participationRole, GMEOW.roleDeceived))

    result = run_shacl(g)
    assert result.ok, "\n".join(result.errors)


def test_distortion_shacl_passes() -> None:
    """A distortion event with spin-doctor role passes SHACL."""
    g = Graph()
    g.add((EX.event1, RDF.type, GMEOW.Event))
    g.add((EX.event1, GMEOW.eventType, GMEOW.eventTypeDistortion))
    g.add((EX.event1, GMEOW.heldStandpoint, EX.claimA))
    g.add((EX.event1, GMEOW.projectedStandpoint, EX.claimB))
    g.add((EX.claimA, RDF.type, GMEOW.StandpointClaim))
    g.add((EX.claimB, RDF.type, GMEOW.StandpointClaim))
    g.add((EX.claimA, GMEOW.observationMethod, EX.method1))
    g.add((EX.claimB, GMEOW.observationMethod, EX.method1))
    g.add((EX.method1, RDF.type, GMEOW.ObservationMethod))
    g.add((EX.partSpin, RDF.type, GMEOW.Participation))
    g.add((EX.partSpin, GMEOW.participationEvent, EX.event1))
    g.add((EX.partSpin, GMEOW.participationParticipant, EX.spinDoctor))
    g.add((EX.partSpin, GMEOW.participationRole, GMEOW.roleSpinDoctor))

    result = run_shacl(g)
    assert result.ok, "\n".join(result.errors)


def test_licensed_falsehood_not_a_lie() -> None:
    """Negative guard: a fiction claim under a NarrativeReferenceFrame must NOT
    be typed as a lie event — the licensed-falsehood safety property.

    This test verifies (a) the vocabulary terms exist in the ontology, and
    (b) the inline fiction structure passes SHACL validation. The full safety
    property (fiction claim is NOT returned by the lie query) is exercised in
    tests/test_competency.py::test_competency_deception_licensed_falsehood_query."""
    g = Graph()
    g.add((EX.fictionClaim, RDF.type, GMEOW.StandpointClaim))
    g.add(
        (
            EX.fictionClaim,
            GMEOW.claimVeridicality,
            GMEOW.veridicalityLicensedFalsehood,
        )
    )
    g.add((EX.fictionClaim, GMEOW.accordingTo, EX.narrativeFrame))
    g.add((EX.fictionClaim, GMEOW.observationMethod, EX.method1))
    g.add((EX.method1, RDF.type, GMEOW.ObservationMethod))
    g.add((EX.narrativeFrame, RDF.type, GMEOW.NarrativeReferenceFrame))
    g.add((EX.narrativeFrame, GMEOW.frameRealm, GMEOW.frameRealmNarrative))
    g.add((EX.narrativeFrame, GMEOW.frameKind, GMEOW.frameKindNarrative))
    result = run_shacl(g)
    assert result.ok, "\n".join(result.errors)
    graph = _graph()
    assert (
        GMEOW.veridicalityLicensedFalsehood,
        RDF.type,
        GMEOW.ClaimVeridicality,
    ) in graph
    assert (
        GMEOW.NarrativeReferenceFrame,
        RDF.type,
        OWL.Class,
    ) in graph
