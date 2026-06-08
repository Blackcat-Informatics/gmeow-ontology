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

GMEOW = Namespace("https://blackcatinformatics.ca/gmeow/")
GUFO = Namespace("http://purl.org/nemo/gufo#")
EX = Namespace("https://example.org/test/")


def _graph() -> Graph:
    return load_merged_graph(include_imports=False)


def test_narrative_reference_frame_is_reference_frame() -> None:
    graph = _graph()
    assert (
        GMEOW.NarrativeReferenceFrame,
        RDFS.subClassOf,
        GMEOW.ReferenceFrame,
    ) in graph


def test_narrative_reference_frame_is_not_standpoint_subclass() -> None:
    """gUFO MixIden forbids a sortal from specializing >1 Kind.
    NarrativeReferenceFrame specializes ReferenceFrame only and *functionally*
    serves as a standpoint (accordingTo range is open)."""
    graph = _graph()
    assert GMEOW.Standpoint not in graph.transitive_objects(
        GMEOW.NarrativeReferenceFrame, RDFS.subClassOf
    )


def test_book_release_and_serial_installment_are_creative_works() -> None:
    graph = _graph()
    for cls in (GMEOW.BookRelease, GMEOW.SerialInstallment):
        assert GMEOW.CreativeWork in graph.transitive_objects(cls, RDFS.subClassOf)


def test_frame_realm_narrative_and_frame_kind_narrative_exist() -> None:
    """
    Check that the merged RDF graph declares the narrative frame realm and
    narrative frame kind individuals.

    Asserts that `GMEOW.frameRealmNarrative` is typed as `GMEOW.FrameRealm` and
    that `GMEOW.frameKindNarrative` is typed as `GMEOW.FrameKind`.
    """
    graph = _graph()
    assert (GMEOW.frameRealmNarrative, RDF.type, GMEOW.FrameRealm) in graph
    assert (GMEOW.frameKindNarrative, RDF.type, GMEOW.FrameKind) in graph


def test_narrative_frame_relation_value_vocab() -> None:
    graph = _graph()
    rel_type = GMEOW.NarrativeFrameRelation
    assert (rel_type, RDFS.subClassOf, GUFO.QualityValue) in graph
    for ind in (
        GMEOW.relationCanon,
        GMEOW.relationAlternateContinuity,
        GMEOW.relationExpandedUniverse,
        GMEOW.relationFanon,
        GMEOW.relationCrossover,
        GMEOW.relationAdaptationOf,
    ):
        assert (ind, RDF.type, rel_type) in graph


def test_source_for_property_exists() -> None:
    graph = _graph()
    prop = GMEOW.sourceFor
    assert (prop, RDF.type, OWL.ObjectProperty) in graph
    assert (prop, RDFS.domain, GMEOW.CreativeWork) in graph
    assert (prop, RDFS.range, GMEOW.NarrativeReferenceFrame) in graph


def test_narrative_reference_frame_shacl_passes() -> None:
    """A fully-populated narrative frame passes SHACL."""
    g = Graph()
    g.add((EX.hpCanon, RDF.type, GMEOW.NarrativeReferenceFrame))
    g.add((EX.hpCanon, GMEOW.frameRealm, GMEOW.frameRealmNarrative))
    g.add((EX.hpCanon, GMEOW.hasAxis, EX.axisPlot))
    g.add(
        (
            EX.hpCanon,
            GMEOW.dimensionCount,
            Literal(1, datatype=XSD.nonNegativeInteger),
        )
    )
    g.add((EX.hpCanon, GMEOW.frameKind, GMEOW.frameKindNarrative))
    g.add((EX.hpCanon, GMEOW.requiresHost, Literal(False)))
    g.add((EX.hpCanon, GMEOW.determinacyModel, GMEOW.determinacyCrisp))

    g.add((GMEOW.frameRealmNarrative, RDF.type, GMEOW.FrameRealm))
    g.add((EX.axisPlot, RDF.type, GMEOW.Axis))
    g.add((GMEOW.frameKindNarrative, RDF.type, GMEOW.FrameKind))
    g.add((GMEOW.determinacyCrisp, RDF.type, GMEOW.Determinacy))

    result = run_shacl(g)
    assert result.ok, "\n".join(result.errors)


def test_narrative_frame_link_is_relatore_subclass() -> None:
    graph = _graph()
    assert (GMEOW.NarrativeFrameLink, RDFS.subClassOf, GUFO.Relator) in graph


def test_narrative_frame_link_properties_exist() -> None:
    graph = _graph()
    for prop in (
        GMEOW.narrativeFrameLinkSource,
        GMEOW.narrativeFrameLinkTarget,
        GMEOW.narrativeFrameLinkRelation,
    ):
        assert (prop, RDF.type, OWL.ObjectProperty) in graph
    assert (
        GMEOW.narrativeFrameLinkSource,
        RDFS.domain,
        GMEOW.NarrativeFrameLink,
    ) in graph
    assert (
        GMEOW.narrativeFrameLinkSource,
        RDFS.range,
        GMEOW.NarrativeReferenceFrame,
    ) in graph
    assert (
        GMEOW.narrativeFrameLinkTarget,
        RDFS.domain,
        GMEOW.NarrativeFrameLink,
    ) in graph
    assert (
        GMEOW.narrativeFrameLinkTarget,
        RDFS.range,
        GMEOW.NarrativeReferenceFrame,
    ) in graph
    assert (
        GMEOW.narrativeFrameLinkRelation,
        RDFS.domain,
        GMEOW.NarrativeFrameLink,
    ) in graph
    assert (
        GMEOW.narrativeFrameLinkRelation,
        RDFS.range,
        GMEOW.NarrativeFrameRelation,
    ) in graph


def _add_narrative_frame(g: Graph, frame: URIRef, axis: URIRef) -> None:
    """Helper to populate a minimal narrative reference frame for SHACL."""
    g.add((frame, RDF.type, GMEOW.NarrativeReferenceFrame))
    g.add((frame, GMEOW.frameRealm, GMEOW.frameRealmNarrative))
    g.add((frame, GMEOW.hasAxis, axis))
    g.add(
        (
            frame,
            GMEOW.dimensionCount,
            Literal(1, datatype=XSD.nonNegativeInteger),
        )
    )
    g.add((frame, GMEOW.frameKind, GMEOW.frameKindNarrative))
    g.add((frame, GMEOW.requiresHost, Literal(False)))
    g.add((frame, GMEOW.determinacyModel, GMEOW.determinacyCrisp))


def test_narrative_frame_link_shacl_passes() -> None:
    """A reified narrative frame link (MCU is adaptation of Earth-616) passes SHACL."""
    g = Graph()
    _add_narrative_frame(g, EX.mcuCanon, EX.axisPlotMcu)
    _add_narrative_frame(g, EX.earth616Canon, EX.axisPlot616)

    g.add((EX.mcu616Link, RDF.type, GMEOW.NarrativeFrameLink))
    g.add((EX.mcu616Link, GMEOW.narrativeFrameLinkSource, EX.mcuCanon))
    g.add((EX.mcu616Link, GMEOW.narrativeFrameLinkTarget, EX.earth616Canon))
    g.add((EX.mcu616Link, GMEOW.narrativeFrameLinkRelation, GMEOW.relationAdaptationOf))

    g.add((GMEOW.frameRealmNarrative, RDF.type, GMEOW.FrameRealm))
    g.add((EX.axisPlotMcu, RDF.type, GMEOW.Axis))
    g.add((EX.axisPlot616, RDF.type, GMEOW.Axis))
    g.add((GMEOW.frameKindNarrative, RDF.type, GMEOW.FrameKind))
    g.add((GMEOW.determinacyCrisp, RDF.type, GMEOW.Determinacy))
    g.add((GMEOW.relationAdaptationOf, RDF.type, GMEOW.NarrativeFrameRelation))

    result = run_shacl(g)
    assert result.ok, "\n".join(result.errors)
