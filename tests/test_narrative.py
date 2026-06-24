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

Migrated to crates/validate/tests/conformance_narrative.rs (#867):
  test_narrative_reference_frame_shacl_passes   -- run_shacl (ExampleConformance)
  test_narrative_frame_link_shacl_passes        -- run_shacl (ExampleConformance)
  test_character_arc_shacl_passes               -- run_shacl (ExampleConformance)
  test_character_arc_missing_subject_fails_shacl -- run_shacl negative
"""

from __future__ import annotations

from gmeow_rdf.compat.rdflib import RDF, RDFS, Graph, Namespace

from gmeow_tools.graph import load_merged_graph

GMEOW = Namespace("https://blackcatinformatics.ca/gmeow/")


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


# =========================================================================== #
# Book / narrative model additions (issue #156)
# =========================================================================== #
