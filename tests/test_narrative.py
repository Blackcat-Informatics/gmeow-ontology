"""Narrative reference frame and creative-work sourcing (issue #89).

Retained dynamic / SHACL checks -- asserted-TBox invariants whose subjects
live in the narrative module have been migrated to
slices/extensions/narrative/tests/structural.ttl (16 cells, #867).

RETAINED here (not migratable to scopeModule cells):
  test_narrative_reference_frame_is_not_standpoint_subclass
    -- transitive graph walk over the merged graph; narrowing to the narrative
    module alone would miss Standpoint imported from other slices.
  test_book_release_and_serial_installment_are_creative_works
    -- transitive walk; gmeow:BookRelease and gmeow:SerialInstallment are
    subjects in slices/core/documents/module.ttl (cross-slice).
  test_frame_realm_narrative_and_frame_kind_narrative_exist
    -- gmeow:frameRealmNarrative and gmeow:frameKindNarrative are subjects in
    slices/core/places/module.ttl (cross-slice).
  test_reading_order_subclasses_standpoint
    -- gmeow:ReadingOrder is a subject in slices/core/documents/module.ttl
    (cross-slice).
  test_narrative_reference_frame_shacl_passes   -- run_shacl (ExampleConformance)
  test_narrative_frame_link_shacl_passes        -- run_shacl (ExampleConformance)
  test_character_arc_shacl_passes               -- run_shacl (ExampleConformance)
  test_character_arc_missing_subject_fails_shacl -- run_shacl negative
"""

from __future__ import annotations

from gmeow_rdf.compat.rdflib import RDF, RDFS, Graph, Literal, Namespace, URIRef
from gmeow_rdf.compat.rdflib.namespace import XSD

from gmeow_tools.graph import load_merged_graph
from tests._graph_nt import run_shacl

GMEOW = Namespace("https://blackcatinformatics.ca/gmeow/")
EX = Namespace("https://example.org/test/")


def _graph() -> Graph:
    return load_merged_graph(include_imports=False)


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

    Asserts that `GMEOW.frameRealmNarrative` is typed as `GMEOW.FrameRealm`
    and that `GMEOW.frameKindNarrative` is typed as `GMEOW.FrameKind`.
    """
    graph = _graph()
    assert (GMEOW.frameRealmNarrative, RDF.type, GMEOW.FrameRealm) in graph
    assert (GMEOW.frameKindNarrative, RDF.type, GMEOW.FrameKind) in graph


def test_reading_order_subclasses_standpoint() -> None:
    graph = _graph()
    assert (GMEOW.ReadingOrder, RDFS.subClassOf, GMEOW.Standpoint) in graph


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


def test_narrative_frame_link_shacl_passes() -> None:
    """A reified frame link (MCU is adaptation of Earth-616) passes SHACL."""
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


# =========================================================================== #
# Book / narrative model additions (issue #156)
# =========================================================================== #


def test_character_arc_shacl_passes() -> None:
    """A well-formed CharacterArc passes SHACL."""
    g = Graph()
    _add_narrative_frame(g, EX.hpCanon, EX.axisPlot)

    g.add((EX.harryArc, RDF.type, GMEOW.CharacterArc))
    g.add((EX.harryArc, GMEOW.arcSubject, EX.harry))
    g.add((EX.harryArc, GMEOW.arcFrame, EX.hpCanon))
    g.add((EX.harryArc, GMEOW.arcType, GMEOW.arcTypeComingOfAge))
    g.add((EX.harry, RDF.type, GMEOW.Entity))
    g.add((GMEOW.arcTypeComingOfAge, RDF.type, GMEOW.ArcType))

    result = run_shacl(g)
    assert result.ok, "\n".join(result.errors)


def test_character_arc_missing_subject_fails_shacl() -> None:
    """A CharacterArc missing arcSubject violates SHACL."""
    g = Graph()
    _add_narrative_frame(g, EX.hpCanon, EX.axisPlot)

    g.add((EX.harryArc, RDF.type, GMEOW.CharacterArc))
    g.add((EX.harryArc, GMEOW.arcFrame, EX.hpCanon))
    g.add((EX.harryArc, GMEOW.arcType, GMEOW.arcTypeComingOfAge))
    g.add((GMEOW.arcTypeComingOfAge, RDF.type, GMEOW.ArcType))

    result = run_shacl(g)
    assert not result.ok
    assert any(
        "CharacterArc must have exactly one gmeow:arcSubject" in e
        for e in result.errors
    )
