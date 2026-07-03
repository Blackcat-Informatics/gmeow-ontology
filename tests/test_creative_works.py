"""WEMI creative-works spine (issue #208).

Structural TBox invariants have been migrated to the declarative slicetest DSL
in the slices that define their subject terms:
  - slices/core/creative-works/tests/structural.ttl (WEMI core)
  - slices/core/documents/tests/structural.ttl (Document, Article, Patent,
    Dataset, media classes, LiteraryWork, SerialWork, ContentSegment, and the
    segment properties)
  - slices/core/events/tests/structural.ttl (creation event type individuals)

SHACL well-formedness (ExampleConformance) tests have been migrated to Rust at
crates/validate/tests/conformance_creative_works.rs (#867).

RETAINED:
  - test_wemi_tiers_subclass_information_object: uses transitive_objects()
"""

from __future__ import annotations

from purrdf.compat.rdflib import RDFS, Graph, Namespace

from gmeow_tools.graph import load_merged_graph

GMEOW = Namespace("https://blackcatinformatics.ca/gmeow/")


def _graph() -> Graph:
    """
    Load the project's merged RDF graph without following owl:imports.

    Returns:
        g (rdflib.Graph): The merged RDF graph with imports excluded
        (include_imports=False).
    """
    return load_merged_graph(include_imports=False)


def test_wemi_tiers_subclass_information_object() -> None:
    """
    Verify each WEMI tier class is a subclass (transitively) of InformationObject.

    Asserts that GMEOW.Work, GMEOW.Expression, GMEOW.Manifestation, and GMEOW.Item
    have GMEOW.InformationObject in their rdfs:subClassOf closure.

    RETAINED: uses transitive_objects() -- a live graph traversal not
    expressible as a module-scoped SPARQL ASK.
    """
    graph = _graph()
    for cls in (GMEOW.Work, GMEOW.Expression, GMEOW.Manifestation, GMEOW.Item):
        assert GMEOW.InformationObject in graph.transitive_objects(cls, RDFS.subClassOf)
