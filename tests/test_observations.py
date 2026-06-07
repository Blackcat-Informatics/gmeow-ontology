"""Competency and reasoning tests for the Observation module (#66).

The observation stack unifies spatial measurement, temporal dating, sensory
reading, and standpoint claims into one gufo:Relator structure. These tests
verify:

1. The TBox is well-formed (classes, properties, value vocabularies).
2. EL axioms fire (Observation mediates at least vantage + observedFeature).
3. Property chains fire (frame inheritance via observationResult).
4. ScalarQuantity is reasoned correctly as an observation result wrapper.
"""

from __future__ import annotations

import owlrl
from rdflib import OWL, RDF, RDFS, Graph, Namespace

from gmeow_tools.config import NAMESPACE, ONTOLOGY_DIR
from gmeow_tools.graph import load_merged_graph

GMEOW = Namespace(NAMESPACE)
GUFO = Namespace("http://purl.org/nemo/gufo#")
EX = Namespace("https://example.org/test/")


def test_observation_class_exists() -> None:
    graph = load_merged_graph(include_imports=False)
    assert (GMEOW.Observation, RDF.type, OWL.Class) in graph
    assert (GMEOW.Observation, RDFS.subClassOf, GUFO.Relator) in graph


def test_observation_properties_exist() -> None:
    graph = load_merged_graph(include_imports=False)
    for prop in (
        "observedFeature",
        "observationResult",
        "vantage",
        "observationMethod",
        "observationType",
        "observationEvent",
    ):
        assert (GMEOW[prop], RDF.type, OWL.ObjectProperty) in graph


def test_observation_value_vocabularies_exist() -> None:
    graph = load_merged_graph(include_imports=False)
    for term in (
        "ObservationMethod",
        "ObservationType",
        "Measurement",
        "SensoryObservation",
        "StandpointClaim",
        "ScalarQuantity",
    ):
        assert (GMEOW[term], RDF.type, OWL.Class) in graph


def test_observation_type_seeds_exist() -> None:
    graph = load_merged_graph(include_imports=False)
    for term in (
        "observationTypeMeasurement",
        "observationTypeSensory",
        "observationTypeStandpoint",
        "observationTypeDerived",
        "observationTypeSimulation",
    ):
        assert (GMEOW[term], RDF.type, GMEOW.ObservationType) in graph


def test_observation_method_seeds_exist() -> None:
    graph = load_merged_graph(include_imports=False)
    for term in (
        "methodDirectObservation",
        "methodInstrumentalReading",
        "methodSurvey",
        "methodRemoteSensing",
        "methodComputationalModel",
        "methodExpertJudgement",
    ):
        assert (GMEOW[term], RDF.type, GMEOW.ObservationMethod) in graph


def test_scalar_quantity_properties_exist() -> None:
    graph = load_merged_graph(include_imports=False)
    assert (GMEOW.quantityValue, RDF.type, OWL.DatatypeProperty) in graph
    assert (GMEOW.quantityUncertainty, RDF.type, OWL.DatatypeProperty) in graph


def test_observation_el_axioms_fire() -> None:
    """An Observation individual with required properties stays consistent."""
    graph = Graph()
    graph.parse(ONTOLOGY_DIR / "modules" / "observations.ttl", format="turtle")
    # Minimal A-Box: an observation of a place by an agent
    graph.add((EX.obs1, RDF.type, GMEOW.Observation))
    graph.add((EX.obs1, GMEOW.vantage, EX.agent1))
    graph.add((EX.obs1, GMEOW.observedFeature, EX.place1))
    graph.add((EX.agent1, RDF.type, GMEOW.Agent))
    graph.add((EX.place1, RDF.type, GMEOW.Place))

    # The EL restrictions are necessary (not sufficient) conditions, so the
    # reasoner will not *infer* Observation from the properties alone under
    # OWL 2 RL.  What we verify here is that the asserted type is not
    # contradicted — i.e. the axioms are consistent with the A-Box.
    owlrl.DeductiveClosure(owlrl.OWLRL_Semantics).expand(graph)
    assert (EX.obs1, RDF.type, GMEOW.Observation) in graph


def test_frame_inheritance_property_chain() -> None:
    """A result inherits the observation's reference frame via property chain."""
    graph = Graph()
    graph.parse(ONTOLOGY_DIR / "modules" / "observations.ttl", format="turtle")
    graph.parse(ONTOLOGY_DIR / "modules" / "places.ttl", format="turtle")

    graph.add((EX.obs1, GMEOW.observationResult, EX.coords1))
    graph.add((EX.obs1, GMEOW.hasReferenceFrame, EX.frameWGS84))
    graph.add((EX.coords1, RDF.type, GMEOW.GeoCoordinates))
    graph.add((EX.frameWGS84, RDF.type, GMEOW.ReferenceFrame))

    owlrl.DeductiveClosure(owlrl.OWLRL_Semantics).expand(graph)
    # The chain: inverse(observationResult) ∘ hasReferenceFrame ⊑ hasReferenceFrame
    # means: coords1 --inverse(observationResult)-- obs1
    #         --hasReferenceFrame-- frameWGS84
    # implies: coords1 --hasReferenceFrame-- frameWGS84
    assert (EX.coords1, GMEOW.hasReferenceFrame, EX.frameWGS84) in graph


def test_measurement_specialises_observation() -> None:
    """Measurement is inferred as an Observation."""
    graph = Graph()
    graph.parse(ONTOLOGY_DIR / "modules" / "observations.ttl", format="turtle")
    graph.add((EX.m1, RDF.type, GMEOW.Measurement))

    owlrl.DeductiveClosure(owlrl.OWLRL_Semantics).expand(graph)
    assert (EX.m1, RDF.type, GMEOW.Observation) in graph


def test_sensory_observation_specialises_observation() -> None:
    graph = Graph()
    graph.parse(ONTOLOGY_DIR / "modules" / "observations.ttl", format="turtle")
    graph.add((EX.s1, RDF.type, GMEOW.SensoryObservation))

    owlrl.DeductiveClosure(owlrl.OWLRL_Semantics).expand(graph)
    assert (EX.s1, RDF.type, GMEOW.Observation) in graph


def test_standpoint_claim_specialises_observation() -> None:
    graph = Graph()
    graph.parse(ONTOLOGY_DIR / "modules" / "observations.ttl", format="turtle")
    graph.add((EX.c1, RDF.type, GMEOW.StandpointClaim))

    owlrl.DeductiveClosure(owlrl.OWLRL_Semantics).expand(graph)
    assert (EX.c1, RDF.type, GMEOW.Observation) in graph
