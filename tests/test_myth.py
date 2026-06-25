"""Deception · Myth as a logic-backed SocialObject (issue #214).

Tests the myth module: Myth is a SocialObject Kind; hasMythTelling, mythFrame,
and propagatesFrom properties; recurringRisk and affectedConsumerSurface operational
fields; SHACL shape; and the no-truth-verdict doctrine.
"""

from __future__ import annotations

from gmeow_rdf.compat.rdflib import OWL, RDF, RDFS, Graph, Namespace, URIRef
from gmeow_rdf.compat.rdflib.namespace import XSD

from gmeow_tools.graph import load_merged_graph

GMEOW = Namespace("https://blackcatinformatics.ca/gmeow/")
LOGIC = Namespace("https://blackcatinformatics.ca/logic/")
EX = Namespace("https://example.org/test/")


def _graph() -> Graph:
    return load_merged_graph(include_imports=False)


def test_social_object_is_category() -> None:
    graph = _graph()
    assert (GMEOW.SocialObject, RDF.type, OWL.Class) in graph
    assert (GMEOW.SocialObject, RDF.type, LOGIC.Category) in graph
    assert (GMEOW.SocialObject, RDFS.subClassOf, GMEOW.Entity) in graph
    assert (GMEOW.SocialObject, RDFS.subClassOf, LOGIC.Object) in graph


def test_myth_properties_exist() -> None:
    graph = _graph()
    for prop in (GMEOW.hasMythTelling, GMEOW.mythFrame, GMEOW.propagatesFrom):
        assert (prop, RDF.type, OWL.ObjectProperty) in graph


def test_has_myth_telling_domain_range() -> None:
    graph = _graph()
    prop = GMEOW.hasMythTelling
    assert (prop, RDFS.domain, GMEOW.Myth) in graph
    assert (prop, RDFS.range, GMEOW.CreativeWork) in graph


def test_myth_frame_is_functional() -> None:
    graph = _graph()
    prop = GMEOW.mythFrame
    assert (prop, RDF.type, OWL.FunctionalProperty) in graph
    assert (prop, RDFS.domain, GMEOW.Myth) in graph
    assert (prop, RDFS.range, GMEOW.NarrativeReferenceFrame) in graph


def test_propagates_from_is_derived_from_subproperty() -> None:
    graph = _graph()
    prop = GMEOW.propagatesFrom
    assert (prop, RDFS.subPropertyOf, GMEOW.wasDerivedFrom) in graph
    assert (prop, RDFS.domain, GMEOW.CreativeWork) in graph
    assert (prop, RDFS.range, GMEOW.CreativeWork) in graph


def test_recurring_risk_exists() -> None:
    graph = _graph()
    prop = GMEOW.recurringRisk
    assert (prop, RDF.type, OWL.DatatypeProperty) in graph
    assert (prop, RDFS.domain, GMEOW.Myth) in graph
    assert (prop, RDFS.range, XSD.boolean) in graph


def test_affected_consumer_surface_exists() -> None:
    graph = _graph()
    prop = GMEOW.affectedConsumerSurface
    assert (prop, RDF.type, OWL.ObjectProperty) in graph
    assert (prop, RDFS.domain, GMEOW.Myth) in graph
    assert (prop, RDFS.range, GMEOW.ProjectionContext) in graph


def test_myth_el_restriction_on_has_myth_telling() -> None:
    """Myth carries an EL someValuesFrom restriction on hasMythTelling."""
    graph = _graph()
    # The restriction is a bnode subClassOf of Myth.
    restrictions = [
        o
        for o in graph.objects(GMEOW.Myth, RDFS.subClassOf)
        if (o, RDF.type, OWL.Restriction) in graph
    ]
    assert any(
        (r, OWL.onProperty, GMEOW.hasMythTelling) in graph
        and (r, OWL.someValuesFrom, GMEOW.CreativeWork) in graph
        for r in restrictions
    ), "Myth must have an EL someValuesFrom restriction on hasMythTelling"


def test_no_truth_axiom_on_myth() -> None:
    """Negative guard: no truth-verdict property may target gmeow:Myth.

    We check rdfs:domain rather than complete absence so the test does not
    break if a future module introduces isTrue / isFalse / isDeceptive for
    a different concept (e.g. a StandpointClaim or VerificationResult).
    """
    graph = _graph()
    for forbidden in ("isTrue", "isFalse", "isDeceptive"):
        prop = URIRef(str(GMEOW) + forbidden)
        assert (prop, RDFS.domain, GMEOW.Myth) not in graph
