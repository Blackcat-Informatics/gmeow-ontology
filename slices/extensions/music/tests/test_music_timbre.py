"""Timbre & sensory bridge guards (issue #317).

Principles 4, 5, 9, 12, 16.
"""

from __future__ import annotations

from gmeow_rdf.compat.rdflib import OWL, RDF, Graph, Namespace
from gmeow_rdf.compat.rdflib.namespace import SKOS

from gmeow_tools.config import NAMESPACE
from gmeow_tools.graph import load_merged_graph
from gmeow_tools.slices import module_path

GMEOW = Namespace(NAMESPACE)

MUSIC_EQ_FILE = module_path("music").parent / "mappings" / "equivalences.ttl"

_MERGED_GRAPH: Graph | None = None


def _graph() -> Graph:
    global _MERGED_GRAPH
    if _MERGED_GRAPH is None:
        _MERGED_GRAPH = load_merged_graph(include_imports=False)
    return _MERGED_GRAPH


def test_timbre_descriptor_seeds_exist() -> None:
    graph = _graph()
    for term in (
        "timbreDescriptorBright",
        "timbreDescriptorDark",
        "timbreDescriptorBreathy",
        "timbreDescriptorGritty",
        "timbreDescriptorHollow",
    ):
        assert (
            GMEOW[term],
            RDF.type,
            GMEOW["TimbreDescriptor"],
        ) in graph, f"{term} should be a TimbreDescriptor"


def test_timbre_observation_result_property_exists() -> None:
    graph = _graph()
    prop = GMEOW["timbreObservationResult"]
    assert (prop, RDF.type, OWL.ObjectProperty) in graph
    assert (prop, RDF.type, OWL.FunctionalProperty) in graph


def test_timbre_fixture_observations_exist() -> None:
    graph = _graph()
    tone_event = GMEOW["fixtureTimbreToneEvent"]
    for term in ("fixtureHumanTimbreObservation", "fixtureMIRTimbreObservation"):
        obs = GMEOW[term]
        assert (obs, RDF.type, GMEOW.Observation) in graph
        assert (obs, GMEOW.observedFeature, tone_event) in graph
        assert any(graph.objects(obs, GMEOW.timbreObservationResult))


def test_timbre_fixture_coequal_vantages() -> None:
    graph = _graph()
    human = GMEOW["fixtureHumanTimbreObservation"]
    machine = GMEOW["fixtureMIRTimbreObservation"]
    assert (human, GMEOW.vantage, GMEOW["fixtureHumanListener"]) in graph
    assert (machine, GMEOW.vantage, GMEOW["fixtureMIRAgent"]) in graph
    assert (
        human,
        GMEOW.timbreObservationResult,
        GMEOW["timbreDescriptorBright"],
    ) in graph
    assert (
        machine,
        GMEOW.timbreObservationResult,
        GMEOW["timbreDescriptorGritty"],
    ) in graph
    # Co-equal: both observations point at the same tone event.
    tone_event = GMEOW["fixtureTimbreToneEvent"]
    assert (human, GMEOW.observedFeature, tone_event) in graph
    assert (machine, GMEOW.observedFeature, tone_event) in graph


def test_afo_timbre_mapping_exists() -> None:
    graph = Graph()
    graph.parse(MUSIC_EQ_FILE, format="turtle")
    eq_iri = GMEOW["eqMu039"]
    assert (eq_iri, RDF.type, GMEOW.TermEquivalence) in graph
    assert (eq_iri, GMEOW.alignPredicate, SKOS.closeMatch) in graph
    objects = {str(o) for o in graph.objects(eq_iri, GMEOW.alignObject)}
    assert "https://w3id.org/afo/onto/1.1#AudioFeature" in objects
    subjects = {str(o) for o in graph.objects(eq_iri, GMEOW.alignSubject)}
    assert "https://blackcatinformatics.ca/gmeow/TimbreDescriptor" in subjects
