"""Competency test for the doxastic belief-revision pattern (#560)."""

from __future__ import annotations

from pathlib import Path

import pytest
from rdflib import RDF, Graph, Literal, Namespace
from rdflib.namespace import XSD

GMEOW = Namespace("https://blackcatinformatics.ca/gmeow/")
EX = Namespace("https://blackcatinformatics.ca/gmeow/examples/epistemics/")

_FIXTURE = (
    Path(__file__).parent / "fixtures" / "coverage" / "epistemics-belief-revision.ttl"
)


@pytest.fixture
def graph() -> Graph:
    return Graph().parse(_FIXTURE, format="turtle")


def test_old_doxastic_tenure_is_closed(graph: Graph) -> None:
    old_interval = EX.originalInterval
    assert (old_interval, GMEOW.endedAtTime, None) in graph
    ends = list(graph.objects(old_interval, GMEOW.endedAtTime))
    assert len(ends) == 1
    assert isinstance(ends[0], Literal)
    assert ends[0].datatype == XSD.dateTime


def test_old_doxastic_tenure_is_suppressed(graph: Graph) -> None:
    old_tenure = EX.originalTenure
    assert (old_tenure, GMEOW.displayable, Literal(False)) in graph


def test_old_doxastic_state_is_retained(graph: Graph) -> None:
    old_state = EX.originalBelief
    assert (old_state, RDF.type, GMEOW.DoxasticState) in graph
    assert (old_state, GMEOW.epistemicAgent, EX.operator) in graph
    assert (old_state, GMEOW.doxasticContent, EX.propPrinterBroken) in graph


def test_new_doxastic_state_is_present(graph: Graph) -> None:
    new_state = EX.revisedBelief
    assert (new_state, RDF.type, GMEOW.DoxasticState) in graph
    assert (new_state, GMEOW.epistemicAgent, EX.operator) in graph
    assert (new_state, GMEOW.doxasticContent, EX.propPrinterBroken) in graph


def test_new_doxastic_tenure_is_open(graph: Graph) -> None:
    new_interval = EX.revisedInterval
    assert (new_interval, GMEOW.startedAtTime, None) in graph
    assert (new_interval, GMEOW.endedAtTime, None) not in graph


def test_qualitative_modality_via_linked_standpoint_claim(graph: Graph) -> None:
    original_claim = EX.originalClaim
    revised_claim = EX.revisedClaim
    assert (EX.originalBelief, GMEOW.doxasticClaim, original_claim) in graph
    assert (EX.revisedBelief, GMEOW.doxasticClaim, revised_claim) in graph
    assert (original_claim, GMEOW.claimModality, GMEOW.unequivocal) in graph
    assert (revised_claim, GMEOW.claimModality, GMEOW.probable) in graph
