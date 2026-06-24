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
  - test_licensed_falsehood_not_a_lie:
      run_shacl() + cross-slice NarrativeReferenceFrame guard.
  - test_disinformation_boundary_query:
      reads an external competency .rq file and checks result labels.

Migrated to crates/validate/tests/conformance_deception.rs (#867 Batch 2):
  - test_standpoint_divergence_coexists
  - test_deception_event_shacl_passes
  - test_deception_cue_shacl_passes
  - test_paltering_implicates_structure
  - test_self_deception_same_agent
  - test_distortion_shacl_passes
  - test_fabrication_refuted_provenance
  - test_forgery_failed_signature_structure
  - test_impersonation_facet_subject_mismatch
  - test_disinformation_propagation_chain
"""

from __future__ import annotations

from pathlib import Path

from gmeow_rdf.compat.rdflib import OWL, RDF, Graph, Namespace, URIRef

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
