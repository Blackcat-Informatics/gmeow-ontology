"""Structural guards for the import-provenance / carrier-time slice.

These pin the "four clocks" design: valid time (validFrom/validUntil), assertion
time (assertedAt), carrier time (sourceModifiedAt on the Source), and transaction
time (ingestedAt on an ImportActivity) — each a distinct term in its own home,
never collapsed into one slot.

MIGRATED to slices/core/provenance/tests/structural.ttl (declarative cells):
  test_import_activity_is_an_activity  → ex:saImportActivityIsSubclassOfActivity
  test_activity_agent_link_is_event_safe → ex:saWasAssociatedWithShape

RETAINED here (cross-slice subjects):
  test_carrier_and_ingestion_props — gmeow:sourceModifiedAt and gmeow:contentDigest
    are defined in the sources slice, not provenance; cannot be scoped to
    gmeow:scopeModule for provenance.
  test_four_clocks_are_distinct_dated_annotations — gmeow:validFrom, gmeow:validUntil,
    gmeow:assertedAt, and gmeow:recordedNoLaterThan are all defined in the temporal
    and sources slices, not provenance.
"""

from __future__ import annotations

from gmeow_rdf.compat.rdflib import OWL, RDF, RDFS, Graph, URIRef

from gmeow_tools.graph import load_merged_graph

GMEOW = "https://blackcatinformatics.ca/gmeow/"


def _graph() -> Graph:
    return load_merged_graph(include_imports=False)


def test_carrier_and_ingestion_props() -> None:
    graph = _graph()
    # sourceModifiedAt: on the CreativeWork (carrier time), and NOT functional — copies
    # of the same content-addressed artifact may report different mtimes, which must
    # coexist rather than force a global inconsistency.
    src_modified = URIRef(GMEOW + "sourceModifiedAt")
    assert (src_modified, RDF.type, OWL.FunctionalProperty) not in graph
    assert (src_modified, RDFS.domain, URIRef(GMEOW + "CreativeWork")) in graph
    # ingestedAt: functional (transaction time).
    assert (URIRef(GMEOW + "ingestedAt"), RDF.type, OWL.FunctionalProperty) in graph
    # contentDigest is NOT functional (an artifact may carry several algorithms).
    assert (
        URIRef(GMEOW + "contentDigest"),
        RDF.type,
        OWL.FunctionalProperty,
    ) not in graph


def test_four_clocks_are_distinct_dated_annotations() -> None:
    # validFrom/validUntil (valid), assertedAt (observation), recordedNoLaterThan
    # (derived carrier bound) are all dateTime annotation properties — and four
    # distinct terms, so a consumer never conflates the clocks.
    graph = _graph()
    clocks = {"validFrom", "validUntil", "assertedAt", "recordedNoLaterThan"}
    for clock in clocks:
        node = URIRef(GMEOW + clock)
        assert (node, RDF.type, OWL.AnnotationProperty) in graph
        assert (
            node,
            RDFS.range,
            URIRef("http://www.w3.org/2001/XMLSchema#dateTime"),
        ) in graph
    assert len(clocks) == 4  # distinct terms, not one overloaded slot
