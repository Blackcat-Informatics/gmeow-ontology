"""Competency test for the doxastic belief-revision pattern (#560)."""

from __future__ import annotations

from pathlib import Path

import pytest
from rdflib import OWL, RDF, RDFS, Graph, Literal, Namespace
from rdflib.namespace import XSD

GMEOW = Namespace("https://blackcatinformatics.ca/gmeow/")
EX = Namespace("https://blackcatinformatics.ca/gmeow/examples/epistemics/")

_FIXTURE = (
    Path(__file__).parent / "fixtures" / "coverage" / "epistemics-belief-revision.ttl"
)


@pytest.fixture
def graph() -> Graph:
    """Load and return the epistemics belief-revision fixture graph.

    Returns:
        Parsed RDF graph containing the belief-revision example data.
    """
    return Graph().parse(_FIXTURE, format="turtle")


def test_old_doxastic_tenure_is_closed(graph: Graph) -> None:
    """Verify the original doxastic tenure has a single xsd:dateTime end time.

    Args:
        graph: Fixture graph containing the belief-revision example.
    """
    old_interval = EX.originalInterval
    assert (old_interval, GMEOW.endedAtTime, None) in graph
    ends = list(graph.objects(old_interval, GMEOW.endedAtTime))
    assert len(ends) == 1
    assert isinstance(ends[0], Literal)
    assert ends[0].datatype == XSD.dateTime


def test_old_doxastic_tenure_is_suppressed(graph: Graph) -> None:
    """Verify the superseded original tenure is marked as not displayable.

    Args:
        graph: Fixture graph containing the belief-revision example.
    """
    old_tenure = EX.originalTenure
    assert (old_tenure, GMEOW.displayable, Literal(False)) in graph


def test_old_doxastic_state_is_retained(graph: Graph) -> None:
    """Verify the original belief remains typed and linked to its agent and content.

    Args:
        graph: Fixture graph containing the belief-revision example.
    """
    old_state = EX.originalBelief
    assert (old_state, RDF.type, GMEOW.DoxasticState) in graph
    assert (old_state, GMEOW.epistemicAgent, EX.operator) in graph
    assert (old_state, GMEOW.doxasticContent, EX.propPrinterBroken) in graph


def test_new_doxastic_state_is_present(graph: Graph) -> None:
    """Verify the revised belief is present as a DoxasticState for the operator.

    Args:
        graph: Fixture graph containing the belief-revision example.
    """
    new_state = EX.revisedBelief
    assert (new_state, RDF.type, GMEOW.DoxasticState) in graph
    assert (new_state, GMEOW.epistemicAgent, EX.operator) in graph
    assert (new_state, GMEOW.doxasticContent, EX.propPrinterBroken) in graph


def test_new_doxastic_tenure_is_open(graph: Graph) -> None:
    """Verify the revised tenure interval has started but has not yet ended.

    Args:
        graph: Fixture graph containing the belief-revision example.
    """
    new_interval = EX.revisedInterval
    starts = list(graph.objects(new_interval, GMEOW.startedAtTime))
    assert len(starts) == 1
    assert isinstance(starts[0], Literal)
    assert starts[0].datatype == XSD.dateTime
    assert (new_interval, GMEOW.endedAtTime, None) not in graph


def test_qualitative_modality_via_linked_standpoint_claim(graph: Graph) -> None:
    """Verify both beliefs link to standpoint claims with the expected modalities.

    Args:
        graph: Fixture graph containing the belief-revision example.
    """
    original_claim = EX.originalClaim
    revised_claim = EX.revisedClaim
    assert (EX.originalBelief, GMEOW.doxasticClaim, original_claim) in graph
    assert (EX.revisedBelief, GMEOW.doxasticClaim, revised_claim) in graph
    assert (original_claim, GMEOW.claimModality, GMEOW.unequivocal) in graph
    assert (revised_claim, GMEOW.claimModality, GMEOW.probable) in graph


def test_ontology_constraints_and_functionality() -> None:
    """Verify OWL domain/range constraints and functional-property tagging."""
    g = Graph().parse("slices/core/epistemics/module.ttl", format="turtle")

    # Domain / range constraints.
    assert (GMEOW.epistemicAgent, RDFS.domain, GMEOW.DoxasticState) in g
    assert (GMEOW.epistemicAgent, RDFS.range, GMEOW.Agent) in g
    assert (GMEOW.doxasticContent, RDFS.domain, GMEOW.DoxasticState) in g
    assert (GMEOW.doxasticContent, RDFS.range, GMEOW.Proposition) in g
    assert (GMEOW.doxasticClaim, RDFS.domain, GMEOW.DoxasticState) in g
    assert (GMEOW.doxasticClaim, RDFS.range, GMEOW.StandpointClaim) in g
    assert (GMEOW.credence, RDFS.domain, GMEOW.DoxasticState) in g
    assert (GMEOW.credence, RDFS.range, XSD.decimal) in g
    assert (GMEOW.tenureOfDoxasticState, RDFS.domain, GMEOW.DoxasticTenure) in g
    assert (GMEOW.tenureOfDoxasticState, RDFS.range, GMEOW.DoxasticState) in g

    # Functional properties.
    assert (GMEOW.epistemicAgent, RDF.type, OWL.FunctionalProperty) in g
    assert (GMEOW.doxasticContent, RDF.type, OWL.FunctionalProperty) in g
    assert (GMEOW.doxasticClaim, RDF.type, OWL.FunctionalProperty) in g
    assert (GMEOW.tenureOfDoxasticState, RDF.type, OWL.FunctionalProperty) in g

    # Non-functional properties.
    assert (GMEOW.credence, RDF.type, OWL.FunctionalProperty) not in g
