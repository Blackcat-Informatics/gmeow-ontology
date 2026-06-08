"""Narrative reference frame and creative-work sourcing (issue #89).

Pins the structural core: NarrativeReferenceFrame is a ReferenceFrame (not a
Kind/Standpoint dual inheritance — gUFO MixIden); BookRelease and
SerialInstallment are CreativeWork subkinds; frameRealmNarrative and
frameKindNarrative are declared; sourceFor links works to frames.
"""

from __future__ import annotations

from rdflib import OWL, RDF, RDFS, Graph, Literal, Namespace, URIRef
from rdflib.namespace import XSD

from gmeow_tools.graph import load_merged_graph
from gmeow_tools.validate import run_shacl

GMEOW = "https://blackcatinformatics.ca/gmeow/"
GUFO = "http://purl.org/nemo/gufo#"
EX = Namespace("https://example.org/test/")


def _graph() -> Graph:
    return load_merged_graph(include_imports=False)


def test_narrative_reference_frame_is_reference_frame() -> None:
    graph = _graph()
    assert (
        URIRef(GMEOW + "NarrativeReferenceFrame"),
        RDFS.subClassOf,
        URIRef(GMEOW + "ReferenceFrame"),
    ) in graph


def test_narrative_reference_frame_is_not_standpoint_subclass() -> None:
    """gUFO MixIden forbids a sortal from specializing >1 Kind.
    NarrativeReferenceFrame specializes ReferenceFrame only and *functionally*
    serves as a standpoint (accordingTo range is open)."""
    graph = _graph()
    assert (
        URIRef(GMEOW + "NarrativeReferenceFrame"),
        RDFS.subClassOf,
        URIRef(GMEOW + "Standpoint"),
    ) not in graph


def test_book_release_and_serial_installment_are_creative_works() -> None:
    graph = _graph()
    for cls in ("BookRelease", "SerialInstallment"):
        assert (
            URIRef(GMEOW + cls),
            RDFS.subClassOf,
            URIRef(GMEOW + "CreativeWork"),
        ) in graph


def test_frame_realm_narrative_and_frame_kind_narrative_exist() -> None:
    graph = _graph()
    assert (
        URIRef(GMEOW + "frameRealmNarrative"),
        RDF.type,
        URIRef(GMEOW + "FrameRealm"),
    ) in graph
    assert (
        URIRef(GMEOW + "frameKindNarrative"),
        RDF.type,
        URIRef(GMEOW + "FrameKind"),
    ) in graph


def test_narrative_frame_relation_value_vocab() -> None:
    graph = _graph()
    rel_type = URIRef(GMEOW + "NarrativeFrameRelation")
    assert (rel_type, RDFS.subClassOf, URIRef(GUFO + "QualityValue")) in graph
    for ind in (
        "relationCanon",
        "relationAlternateContinuity",
        "relationExpandedUniverse",
        "relationFanon",
        "relationCrossover",
        "relationAdaptationOf",
    ):
        assert (URIRef(GMEOW + ind), RDF.type, rel_type) in graph


def test_source_for_property_exists() -> None:
    graph = _graph()
    prop = URIRef(GMEOW + "sourceFor")
    assert (prop, RDF.type, OWL.ObjectProperty) in graph
    assert (prop, RDFS.domain, URIRef(GMEOW + "CreativeWork")) in graph
    assert (prop, RDFS.range, URIRef(GMEOW + "NarrativeReferenceFrame")) in graph


def test_narrative_reference_frame_shacl_passes() -> None:
    """A fully-populated narrative frame passes SHACL."""
    g = Graph()
    g.add((EX.hpCanon, RDF.type, URIRef(GMEOW + "NarrativeReferenceFrame")))
    g.add(
        (
            EX.hpCanon,
            URIRef(GMEOW + "frameRealm"),
            URIRef(GMEOW + "frameRealmNarrative"),
        )
    )
    g.add((EX.hpCanon, URIRef(GMEOW + "hasAxis"), EX.axisPlot))
    g.add(
        (
            EX.hpCanon,
            URIRef(GMEOW + "dimensionCount"),
            Literal(1, datatype=XSD.nonNegativeInteger),
        )
    )
    g.add(
        (EX.hpCanon, URIRef(GMEOW + "frameKind"), URIRef(GMEOW + "frameKindNarrative"))
    )
    g.add((EX.hpCanon, URIRef(GMEOW + "requiresHost"), Literal(False)))
    g.add(
        (
            EX.hpCanon,
            URIRef(GMEOW + "determinacyModel"),
            URIRef(GMEOW + "determinacyCrisp"),
        )
    )

    g.add(
        (URIRef(GMEOW + "frameRealmNarrative"), RDF.type, URIRef(GMEOW + "FrameRealm"))
    )
    g.add((EX.axisPlot, RDF.type, URIRef(GMEOW + "Axis")))
    g.add((URIRef(GMEOW + "frameKindNarrative"), RDF.type, URIRef(GMEOW + "FrameKind")))
    g.add((URIRef(GMEOW + "determinacyCrisp"), RDF.type, URIRef(GMEOW + "Determinacy")))

    result = run_shacl(g)
    assert result.ok, "\n".join(result.errors)
