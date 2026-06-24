"""Deception -- SHACL / dynamic tests retained from issue #213.

TBox structural assertions (EventType individuals, property shapes,
ClaimVeridicality, MaximViolationType, no-isFalse guards, etc.) have been
migrated to slices/core/deception/tests/structural.ttl as declarative
gmeow:StructuralAssertion cells run by the native Rust slicetest harness.

Retained here (not migratable to module-scoped declarative cells):
  - test_blame_deflection_example_uses_doxastic_standpoint_claims:
      dynamic ABox file-load check over an example file.
  - test_bullshit_modality_exists:
      gmeow:bullshit is defined in slices/core/standpoint/module.ttl
      (cross-slice), not in the deception module.
  - All run_shacl() ExampleConformance tests.
  - test_licensed_falsehood_not_a_lie:
      run_shacl() + cross-slice NarrativeReferenceFrame guard.
  - test_disinformation_boundary_query:
      reads an external competency .rq file and checks result labels.
"""

from __future__ import annotations

from pathlib import Path

from gmeow_rdf.compat.rdflib import OWL, RDF, Graph, Literal, Namespace, URIRef

from gmeow_tools.config import COMPETENCY_DIR
from gmeow_tools.graph import load_merged_graph
from tests._graph_nt import run_shacl

GMEOW = Namespace("https://blackcatinformatics.ca/gmeow/")
EX = Namespace("https://example.org/test/")


def _graph() -> Graph:
    return load_merged_graph(include_imports=False)


def _doxastic_claim(
    g: Graph,
    claim: URIRef,
    agent: URIRef,
    proposition: URIRef,
    method: URIRef = EX.method1,
) -> URIRef:
    """Add a DoxasticStandpointClaim backed by a DoxasticState.

    The claim is explicitly typed as both gmeow:DoxasticStandpointClaim and
    gmeow:StandpointClaim so the SHACL engine (which does not perform subclass
    reasoning over the data graph) recognises it for the
    gmeow:doxasticClaim range on the backing state.
    """
    state = URIRef(str(claim) + "State")
    g.add((claim, RDF.type, GMEOW.DoxasticStandpointClaim))
    g.add((claim, RDF.type, GMEOW.StandpointClaim))
    g.add((claim, GMEOW.observationMethod, method))
    g.add((claim, GMEOW.claimOfBelief, state))
    g.add((state, RDF.type, GMEOW.DoxasticState))
    g.add((state, GMEOW.epistemicAgent, agent))
    g.add((state, GMEOW.doxasticContent, proposition))
    g.add((state, GMEOW.doxasticClaim, claim))
    return state


def test_blame_deflection_example_uses_doxastic_standpoint_claims() -> None:
    """Issue #561 re-grounding: every held/projected standpoint in the
    blame-deflection example is typed gmeow:DoxasticStandpointClaim."""
    g = Graph()
    example = (
        Path(__file__).resolve().parents[1]
        / "slices/core/deception/examples/blame-deflection.ttl"
    )
    g.parse(example, format="turtle")

    held = {o for s, p, o in g if p == GMEOW.heldStandpoint}
    projected = {o for s, p, o in g if p == GMEOW.projectedStandpoint}
    assert held, "expected at least one held standpoint"
    assert projected, "expected at least one projected standpoint"
    for standpoint in held | projected:
        assert (
            standpoint,
            RDF.type,
            GMEOW.DoxasticStandpointClaim,
        ) in g, f"{standpoint} is not a DoxasticStandpointClaim"


def test_bullshit_modality_exists() -> None:
    graph = _graph()
    assert (GMEOW.bullshit, RDF.type, GMEOW.StandpointModality) in graph


def test_standpoint_divergence_coexists() -> None:
    """Principle 9: held and projected standpoints are coexisting claims,
    neither privileged. The graph must permit both on the same event."""
    g = Graph()
    g.add((EX.event1, RDF.type, GMEOW.Event))
    g.add((EX.event1, GMEOW.eventType, GMEOW.eventTypeDeception))
    g.add((EX.agent1, RDF.type, GMEOW.Agent))
    g.add((EX.propA, RDF.type, GMEOW.Proposition))
    g.add((EX.propB, RDF.type, GMEOW.Proposition))
    g.add((EX.event1, GMEOW.heldStandpoint, EX.claimA))
    g.add((EX.event1, GMEOW.projectedStandpoint, EX.claimB))
    _doxastic_claim(g, EX.claimA, EX.agent1, EX.propA)
    _doxastic_claim(g, EX.claimB, EX.agent1, EX.propB)
    g.add((EX.method1, RDF.type, GMEOW.ObservationMethod))

    result = run_shacl(g)
    assert result.ok, "\n".join(result.errors)


def test_deception_event_shacl_passes() -> None:
    """A fully-populated deception event passes SHACL."""
    g = Graph()
    g.add((EX.event1, RDF.type, GMEOW.Event))
    g.add((EX.event1, GMEOW.eventType, GMEOW.eventTypeDeception))
    g.add((EX.agent1, RDF.type, GMEOW.Agent))
    g.add((EX.propA, RDF.type, GMEOW.Proposition))
    g.add((EX.propB, RDF.type, GMEOW.Proposition))
    g.add((EX.event1, GMEOW.heldStandpoint, EX.claimA))
    g.add((EX.event1, GMEOW.projectedStandpoint, EX.claimB))
    _doxastic_claim(g, EX.claimA, EX.agent1, EX.propA)
    _doxastic_claim(g, EX.claimB, EX.agent1, EX.propB)
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
    g.add((EX.agent1, RDF.type, GMEOW.Agent))
    g.add((EX.propA, RDF.type, GMEOW.Proposition))
    g.add((EX.propB, RDF.type, GMEOW.Proposition))
    g.add((EX.event1, GMEOW.heldStandpoint, EX.claimA))
    g.add((EX.event1, GMEOW.projectedStandpoint, EX.claimB))
    _doxastic_claim(g, EX.claimA, EX.agent1, EX.propA)
    _doxastic_claim(g, EX.claimB, EX.agent1, EX.propB)
    g.add((EX.method1, RDF.type, GMEOW.ObservationMethod))
    g.add((EX.cue1, RDF.type, GMEOW.Observation))
    g.add((EX.cue1, GMEOW.vantage, EX.analyst))
    g.add((EX.cue1, GMEOW.observationResult, EX.result1))
    g.add((EX.cue1, GMEOW.observedFeature, EX.event1))
    g.add((EX.event1, GMEOW.deceptionCue, EX.cue1))

    result = run_shacl(g)
    assert result.ok, "\n".join(result.errors)


# ===========================================================================
# Issue #215 -- Speech-act deception types (SHACL conformance).
# ===========================================================================


def test_paltering_implicates_structure() -> None:
    """A paltering event can carry gmeow:implicates to a proposition.
    The implicates property domain is Event and range is Entity."""
    g = Graph()
    g.add((EX.event1, RDF.type, GMEOW.Event))
    g.add((EX.event1, GMEOW.eventType, GMEOW.eventTypePaltering))
    g.add((EX.event1, GMEOW.implicates, EX.propositionPprime))
    g.add((EX.agent1, RDF.type, GMEOW.Agent))
    g.add((EX.propA, RDF.type, GMEOW.Proposition))
    g.add((EX.propB, RDF.type, GMEOW.Proposition))
    g.add((EX.event1, GMEOW.heldStandpoint, EX.claimA))
    g.add((EX.event1, GMEOW.projectedStandpoint, EX.claimB))
    _doxastic_claim(g, EX.claimA, EX.agent1, EX.propA)
    _doxastic_claim(g, EX.claimB, EX.agent1, EX.propB)
    g.add((EX.method1, RDF.type, GMEOW.ObservationMethod))

    result = run_shacl(g)
    assert result.ok, "\n".join(result.errors)


def test_self_deception_same_agent() -> None:
    """Self-deception: the same agent can bear both deceiver and deceived roles
    on the same event via distinct Participation relators (Principle 9)."""
    g = Graph()
    g.add((EX.event1, RDF.type, GMEOW.Event))
    g.add((EX.event1, GMEOW.eventType, GMEOW.eventTypeSelfDeception))
    g.add((EX.agent1, RDF.type, GMEOW.Agent))
    g.add((EX.propA, RDF.type, GMEOW.Proposition))
    g.add((EX.propB, RDF.type, GMEOW.Proposition))
    g.add((EX.event1, GMEOW.heldStandpoint, EX.claimA))
    g.add((EX.event1, GMEOW.projectedStandpoint, EX.claimB))
    _doxastic_claim(g, EX.claimA, EX.agent1, EX.propA)
    _doxastic_claim(g, EX.claimB, EX.agent1, EX.propB)
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
    g.add((EX.agent1, RDF.type, GMEOW.Agent))
    g.add((EX.propA, RDF.type, GMEOW.Proposition))
    g.add((EX.propB, RDF.type, GMEOW.Proposition))
    g.add((EX.event1, GMEOW.heldStandpoint, EX.claimA))
    g.add((EX.event1, GMEOW.projectedStandpoint, EX.claimB))
    _doxastic_claim(g, EX.claimA, EX.agent1, EX.propA)
    _doxastic_claim(g, EX.claimB, EX.agent1, EX.propB)
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
    tests/test_competency.py::test_competency_deception_licensed_falsehood_query.
    """
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


# ===========================================================================
# Issue #216 -- Carrier deception types (SHACL conformance).
# ===========================================================================


def test_fabrication_refuted_provenance() -> None:
    """A fabrication event with refuted provenance passes SHACL."""
    g = Graph()
    g.add((EX.event1, RDF.type, GMEOW.Event))
    g.add((EX.event1, GMEOW.eventType, GMEOW.eventTypeFabrication))
    g.add((EX.agent1, RDF.type, GMEOW.Agent))
    g.add((EX.propA, RDF.type, GMEOW.Proposition))
    g.add((EX.propB, RDF.type, GMEOW.Proposition))
    g.add((EX.event1, GMEOW.heldStandpoint, EX.claimA))
    g.add((EX.event1, GMEOW.projectedStandpoint, EX.claimB))
    _doxastic_claim(g, EX.claimA, EX.agent1, EX.propA)
    _doxastic_claim(g, EX.claimB, EX.agent1, EX.propB)
    g.add((EX.method1, RDF.type, GMEOW.ObservationMethod))
    # The fabricated work has a failed verification result (evidence, not axiom).
    g.add((EX.work1, RDF.type, GMEOW.CreativeWork))
    g.add((EX.event1, GMEOW.implicates, EX.work1))
    g.add((EX.verification1, RDF.type, GMEOW.VerificationResult))
    g.add(
        (EX.verification1, GMEOW.hasVerificationStatus, GMEOW.verificationStatusFailed)
    )

    result = run_shacl(g)
    assert result.ok, "\n".join(result.errors)


def test_forgery_failed_signature_structure() -> None:
    """A forgery event with counterpartOf + failed CryptographicSignature
    passes SHACL."""
    g = Graph()
    g.add((EX.event1, RDF.type, GMEOW.Event))
    g.add((EX.event1, GMEOW.eventType, GMEOW.eventTypeForgery))
    g.add((EX.agent1, RDF.type, GMEOW.Agent))
    g.add((EX.propA, RDF.type, GMEOW.Proposition))
    g.add((EX.propB, RDF.type, GMEOW.Proposition))
    g.add((EX.event1, GMEOW.heldStandpoint, EX.claimA))
    g.add((EX.event1, GMEOW.projectedStandpoint, EX.claimB))
    _doxastic_claim(g, EX.claimA, EX.agent1, EX.propA)
    _doxastic_claim(g, EX.claimB, EX.agent1, EX.propB)
    g.add((EX.method1, RDF.type, GMEOW.ObservationMethod))
    # Forged work counterpartOf genuine work.
    g.add((EX.forgedWork, RDF.type, GMEOW.CreativeWork))
    g.add((EX.genuineWork, RDF.type, GMEOW.CreativeWork))
    g.add((EX.forgedWork, GMEOW.counterpartOf, EX.genuineWork))
    g.add((EX.event1, GMEOW.implicates, EX.forgedWork))
    # Failed cryptographic signature.
    g.add((EX.signature1, RDF.type, GMEOW.CryptographicSignature))
    g.add((EX.signature1, GMEOW.signatureOf, EX.forgedWork))
    g.add((EX.signature1, GMEOW.hasVerificationStatus, GMEOW.verificationStatusFailed))

    result = run_shacl(g)
    assert result.ok, "\n".join(result.errors)


def test_impersonation_facet_subject_mismatch() -> None:
    """An impersonation event where projected identity facet subject
    ≠ deceiver passes SHACL."""
    g = Graph()
    g.add((EX.event1, RDF.type, GMEOW.Event))
    g.add((EX.event1, GMEOW.eventType, GMEOW.eventTypeImpersonation))
    g.add((EX.agent1, RDF.type, GMEOW.Agent))
    g.add((EX.propA, RDF.type, GMEOW.Proposition))
    g.add((EX.propB, RDF.type, GMEOW.Proposition))
    g.add((EX.event1, GMEOW.heldStandpoint, EX.claimA))
    g.add((EX.event1, GMEOW.projectedStandpoint, EX.claimB))
    _doxastic_claim(g, EX.claimA, EX.agent1, EX.propA)
    _doxastic_claim(g, EX.claimB, EX.agent1, EX.propB)
    g.add((EX.method1, RDF.type, GMEOW.ObservationMethod))
    # Projected identity facet's subject is the victim, not the deceiver.
    g.add((EX.facet1, RDF.type, GMEOW.IdentityFacet))
    g.add((EX.facet1, GMEOW.facetSubject, EX.victim))
    g.add((EX.facet1, GMEOW.facetVantage, EX.victim))
    g.add((EX.facet1, GMEOW.observedFeature, EX.event1))
    # Failed email authentication result (spoofing instance).
    g.add((EX.authResult1, RDF.type, GMEOW.AuthenticationResult))
    g.add((EX.authResult1, GMEOW.authResult, Literal("fail")))

    result = run_shacl(g)
    assert result.ok, "\n".join(result.errors)


# ===========================================================================
# Issue #217 -- Disinformation campaign (SHACL conformance + query).
# ===========================================================================


def test_disinformation_propagation_chain() -> None:
    """A 3-hop disinformation chain passes SHACL.

    Hop 0 (origin): deceiver seeds false claim — held ≠ projected + intent.
    Hop 1 (dupe):   sincere believer reshares — held ≈ projected, untrue, no intent.
    Hop 2 (downstream): another sincere resharing — held ≈ projected, untrue, no intent.
    """
    g = Graph()

    # --- Hop 0: Disinformation origin ---
    g.add((EX.originEvent, RDF.type, GMEOW.Event))
    g.add((EX.originEvent, GMEOW.eventType, GMEOW.eventTypeDisinformation))
    g.add((EX.deceiver, RDF.type, GMEOW.Agent))
    g.add((EX.originHeldProp, RDF.type, GMEOW.Proposition))
    g.add((EX.originProjectedProp, RDF.type, GMEOW.Proposition))
    # held ≠ projected
    g.add((EX.originEvent, GMEOW.heldStandpoint, EX.originHeld))
    g.add((EX.originEvent, GMEOW.projectedStandpoint, EX.originProjected))
    _doxastic_claim(g, EX.originHeld, EX.deceiver, EX.originHeldProp)
    _doxastic_claim(g, EX.originProjected, EX.deceiver, EX.originProjectedProp)
    # deceptive intent claim present
    g.add((EX.originEvent, GMEOW.deceptiveIntentClaim, EX.originIntent))
    g.add((EX.originIntent, RDF.type, GMEOW.StandpointClaim))
    g.add((EX.originIntent, GMEOW.observationMethod, EX.method1))
    # origin deceiver role
    g.add((EX.partOriginDeceiver, RDF.type, GMEOW.Participation))
    g.add((EX.partOriginDeceiver, GMEOW.participationEvent, EX.originEvent))
    g.add((EX.partOriginDeceiver, GMEOW.participationParticipant, EX.deceiver))
    g.add((EX.partOriginDeceiver, GMEOW.participationRole, GMEOW.roleDeceiver))

    # --- Hop 1: Dupe resharing (misinformation at this node) ---
    g.add((EX.dupeEvent, RDF.type, GMEOW.Event))
    g.add((EX.dupeEvent, GMEOW.eventType, GMEOW.eventTypeDeception))
    g.add((EX.dupe, RDF.type, GMEOW.Agent))
    g.add((EX.dupeProp, RDF.type, GMEOW.Proposition))
    # held ≈ projected (same claim)
    g.add((EX.dupeEvent, GMEOW.heldStandpoint, EX.dupeBelief))
    g.add((EX.dupeEvent, GMEOW.projectedStandpoint, EX.dupeBelief))
    _doxastic_claim(g, EX.dupeBelief, EX.dupe, EX.dupeProp)
    g.add((EX.dupeBelief, GMEOW.claimVeridicality, GMEOW.veridicalityUntrue))
    # NO deceptiveIntentClaim — the dupe is sincere
    # dupe role
    g.add((EX.partDupe, RDF.type, GMEOW.Participation))
    g.add((EX.partDupe, GMEOW.participationEvent, EX.dupeEvent))
    g.add((EX.partDupe, GMEOW.participationParticipant, EX.dupe))
    g.add((EX.partDupe, GMEOW.participationRole, GMEOW.roleDupe))

    # --- Hop 2: Downstream resharing (misinformation at this node) ---
    g.add((EX.downstreamEvent, RDF.type, GMEOW.Event))
    g.add((EX.downstreamEvent, GMEOW.eventType, GMEOW.eventTypeDeception))
    g.add((EX.downstream, RDF.type, GMEOW.Agent))
    g.add((EX.downstreamProp, RDF.type, GMEOW.Proposition))
    g.add((EX.downstreamEvent, GMEOW.heldStandpoint, EX.downstreamBelief))
    g.add((EX.downstreamEvent, GMEOW.projectedStandpoint, EX.downstreamBelief))
    _doxastic_claim(g, EX.downstreamBelief, EX.downstream, EX.downstreamProp)
    g.add((EX.downstreamBelief, GMEOW.claimVeridicality, GMEOW.veridicalityUntrue))
    # NO deceptiveIntentClaim

    # Shared method
    g.add((EX.method1, RDF.type, GMEOW.ObservationMethod))

    result = run_shacl(g)
    assert result.ok, "\n".join(result.errors)


def test_disinformation_boundary_query() -> None:
    """The boundary query correctly labels origin as disinformation and
    dupe/downstream as misinformation."""
    from gmeow_rdf.compat.rdflib.query import ResultRow

    g = load_merged_graph(include_imports=False)

    # Populate the chain in the graph
    g.add((EX.originEvent, RDF.type, GMEOW.Event))
    g.add((EX.originEvent, GMEOW.eventType, GMEOW.eventTypeDisinformation))
    g.add((EX.deceiver, RDF.type, GMEOW.Agent))
    g.add((EX.originHeldProp, RDF.type, GMEOW.Proposition))
    g.add((EX.originProjectedProp, RDF.type, GMEOW.Proposition))
    g.add((EX.originEvent, GMEOW.heldStandpoint, EX.originHeld))
    g.add((EX.originEvent, GMEOW.projectedStandpoint, EX.originProjected))
    _doxastic_claim(g, EX.originHeld, EX.deceiver, EX.originHeldProp)
    _doxastic_claim(g, EX.originProjected, EX.deceiver, EX.originProjectedProp)
    g.add((EX.originEvent, GMEOW.deceptiveIntentClaim, EX.originIntent))
    g.add((EX.originIntent, RDF.type, GMEOW.StandpointClaim))
    g.add((EX.originIntent, GMEOW.observationMethod, EX.method1))

    g.add((EX.dupeEvent, RDF.type, GMEOW.Event))
    g.add((EX.dupeEvent, GMEOW.eventType, GMEOW.eventTypeDeception))
    g.add((EX.dupe, RDF.type, GMEOW.Agent))
    g.add((EX.dupeProp, RDF.type, GMEOW.Proposition))
    g.add((EX.dupeEvent, GMEOW.heldStandpoint, EX.dupeBelief))
    g.add((EX.dupeEvent, GMEOW.projectedStandpoint, EX.dupeBelief))
    _doxastic_claim(g, EX.dupeBelief, EX.dupe, EX.dupeProp)
    g.add((EX.dupeBelief, GMEOW.claimVeridicality, GMEOW.veridicalityUntrue))

    g.add((EX.downstreamEvent, RDF.type, GMEOW.Event))
    g.add((EX.downstreamEvent, GMEOW.eventType, GMEOW.eventTypeDeception))
    g.add((EX.downstream, RDF.type, GMEOW.Agent))
    g.add((EX.downstreamProp, RDF.type, GMEOW.Proposition))
    g.add((EX.downstreamEvent, GMEOW.heldStandpoint, EX.downstreamBelief))
    g.add((EX.downstreamEvent, GMEOW.projectedStandpoint, EX.downstreamBelief))
    _doxastic_claim(g, EX.downstreamBelief, EX.downstream, EX.downstreamProp)
    g.add((EX.downstreamBelief, GMEOW.claimVeridicality, GMEOW.veridicalityUntrue))

    g.add((EX.method1, RDF.type, GMEOW.ObservationMethod))

    query = (COMPETENCY_DIR / "deception-disinformation-boundary.rq").read_text(
        encoding="utf-8"
    )
    by_event: dict[str, str] = {}
    for row in g.query(query):
        assert isinstance(row, ResultRow)
        by_event[str(row[0])] = str(row[2])

    assert by_event.get(str(EX.originEvent)) == "disinformation"
    assert by_event.get(str(EX.dupeEvent)) == "misinformation"
    assert by_event.get(str(EX.downstreamEvent)) == "misinformation"
