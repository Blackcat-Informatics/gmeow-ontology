"""Tests for the cross-cutting temporal facility introduced by the email slice.

The temporal module reifies time-scoped relations as gufo:Situation subclasses
(so residence and tenure hold over an interval), and offers validFrom/validUntil
as lighter-weight RDF-star annotations. These structural assertions guard the
pattern that later slices (calendar, projects) will reuse.
"""

from __future__ import annotations

from rdflib import Graph, URIRef
from rdflib.namespace import OWL, RDF, RDFS

from gmeow_tools.graph import load_merged_graph

GMEOW = "https://blackcatinformatics.ca/gmeow/"
GUFO = "http://purl.org/nemo/gufo#"


def _graph() -> Graph:
    # Imports included so the gufo:Situation grounding resolves.
    return load_merged_graph(include_imports=True)


def test_time_scoped_relation_is_a_gufo_situation() -> None:
    graph = _graph()
    assert (
        URIRef(GMEOW + "TimeScopedRelation"),
        RDFS.subClassOf,
        URIRef(GUFO + "Situation"),
    ) in graph


def test_reified_residence_and_tenure_are_time_scoped() -> None:
    graph = _graph()
    for term in ("MailboxResidence", "AddressTenure"):
        assert (
            URIRef(GMEOW + term),
            RDFS.subClassOf,
            URIRef(GMEOW + "TimeScopedRelation"),
        ) in graph


def test_interpersonal_relationship_is_a_gufo_relator() -> None:
    # A standing interpersonal tie is a relator (mediates + depends on its
    # players), NOT a Situation — the Relator-vs-Situation decision is load-bearing.
    graph = _graph()
    assert (
        URIRef(GMEOW + "InterpersonalRelationship"),
        RDFS.subClassOf,
        URIRef(GUFO + "Relator"),
    ) in graph


def test_validity_predicates_are_annotation_properties() -> None:
    graph = _graph()
    for term in ("validFrom", "validUntil"):
        assert (
            URIRef(GMEOW + term),
            RDF.type,
            OWL.AnnotationProperty,
        ) in graph


def test_instant_subclasses_gufo_abstract_individual() -> None:
    graph = _graph()
    assert (
        URIRef(GMEOW + "Instant"),
        RDFS.subClassOf,
        URIRef(GUFO + "AbstractIndividual"),
    ) in graph


def test_time_interval_has_start_and_end_instants() -> None:
    graph = _graph()
    for prop in ("hasStartInstant", "hasEndInstant"):
        node = URIRef(GMEOW + prop)
        assert (node, RDF.type, OWL.ObjectProperty) in graph
        assert (node, RDFS.domain, URIRef(GMEOW + "TimeInterval")) in graph
        assert (node, RDFS.range, URIRef(GMEOW + "Instant")) in graph


def test_time_interval_can_have_temporal_frame() -> None:
    graph = _graph()
    assert (
        URIRef(GMEOW + "hasTemporalFrame"),
        RDF.type,
        OWL.ObjectProperty,
    ) in graph
    assert (
        URIRef(GMEOW + "hasTemporalFrame"),
        RDFS.domain,
        URIRef(GMEOW + "TimeInterval"),
    ) in graph
    assert (
        URIRef(GMEOW + "hasTemporalFrame"),
        RDFS.range,
        URIRef(GMEOW + "TemporalFrame"),
    ) in graph


def test_temporal_measurement_is_gufo_relator() -> None:
    # — TemporalMeasurement re-parented under Observation/Measurement.
    graph = _graph()
    assert (
        URIRef(GMEOW + "TemporalMeasurement"),
        RDFS.subClassOf,
        URIRef(GUFO + "Relator"),
    ) in graph
