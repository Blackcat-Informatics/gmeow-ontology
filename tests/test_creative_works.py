"""WEMI creative-works spine (issue #208).

Structural TBox invariants have been migrated to the declarative slicetest DSL
at slices/core/creative-works/tests/structural.ttl.

SHACL well-formedness (ExampleConformance) tests have been migrated to Rust at
crates/validate/tests/conformance_creative_works.rs (#867).

RETAINED:
  - test_creative_work_is_category: gmeow:CreativeWork defined in documents/
  - test_wemi_tiers_subclass_information_object: uses transitive_objects()
  - test_document_subclasses_work: subjects in documents/
  - test_media_etc_subclasses_manifestation: subjects in documents/
  - test_contribution_degree_value_vocab: subjects in citations/
  - test_creation_event_types_exist: subjects in events/
  - All book/narrative model tests whose subjects are in documents/
"""

from __future__ import annotations

from gmeow_rdf.compat.rdflib import OWL, RDF, RDFS, Graph, Namespace

from gmeow_tools.graph import load_merged_graph

GMEOW = Namespace("https://blackcatinformatics.ca/gmeow/")
GUFO = Namespace("http://purl.org/nemo/gufo#")


def _graph() -> Graph:
    """
    Load the project's merged RDF graph without following owl:imports.

    Returns:
        g (rdflib.Graph): The merged RDF graph with imports excluded
        (include_imports=False).
    """
    return load_merged_graph(include_imports=False)


# =========================================================================== #
# Class hierarchy (cross-slice and dynamic -- NOT in structural.ttl)
# =========================================================================== #


def test_creative_work_is_category() -> None:
    graph = _graph()
    assert (GMEOW.CreativeWork, RDF.type, GUFO.Category) in graph


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


# =========================================================================== #
# Re-homed documents.ttl classes (cross-slice -- subjects in documents/)
# =========================================================================== #


def test_document_subclasses_work() -> None:
    """
    RETAINED: Document, Article, Patent, Dataset are defined in
    slices/core/documents/module.ttl -- cross-slice subjects.
    """
    graph = _graph()
    for cls in (GMEOW.Document, GMEOW.Article, GMEOW.Patent, GMEOW.Dataset):
        assert (cls, RDFS.subClassOf, GMEOW.Work) in graph


def test_media_etc_subclasses_manifestation() -> None:
    """
    Assert that specific media-related classes are direct subclasses of
    GMEOW.Manifestation.

    RETAINED: MediaObject, WebPage, BookRelease, SerialInstallment are
    defined in slices/core/documents/module.ttl -- cross-slice subjects.
    """
    graph = _graph()
    for cls in (
        GMEOW.MediaObject,
        GMEOW.WebPage,
        GMEOW.BookRelease,
        GMEOW.SerialInstallment,
    ):
        assert (cls, RDFS.subClassOf, GMEOW.Manifestation) in graph


# =========================================================================== #
# Value vocabularies -- cross-slice (subjects NOT in creative-works module)
# =========================================================================== #


def test_contribution_degree_value_vocab() -> None:
    """
    Verify the contribution-degree vocabulary is modeled as a value class and its
    members are individuals.

    RETAINED: ContributionDegree, degreeLead, degreeEqual, degreeSupporting are
    defined in slices/core/citations/module.ttl -- cross-slice subjects.
    """
    graph = _graph()
    assert (GMEOW.ContributionDegree, RDFS.subClassOf, GUFO.QualityValue) in graph
    for ind in (
        GMEOW.degreeLead,
        GMEOW.degreeEqual,
        GMEOW.degreeSupporting,
    ):
        assert (ind, RDF.type, GMEOW.ContributionDegree) in graph


# =========================================================================== #
# Creation event types (cross-slice -- subjects in events/)
# =========================================================================== #


def test_creation_event_types_exist() -> None:
    """
    Verify that the expected creation event type individuals are present in the merged
    graph.

    RETAINED: eventTypeWorkConception, eventTypeExpressionCreation,
    eventTypeManifestationProduction are defined in slices/core/events/module.ttl
    -- cross-slice subjects.
    """
    graph = _graph()
    for ind in (
        GMEOW.eventTypeWorkConception,
        GMEOW.eventTypeExpressionCreation,
        GMEOW.eventTypeManifestationProduction,
    ):
        assert (ind, RDF.type, GMEOW.EventType) in graph


# =========================================================================== #
# Book / narrative model additions (issue #156)
# Cross-slice: all subjects defined in slices/core/documents/module.ttl
# =========================================================================== #


def test_literary_work_subclasses_work() -> None:
    """RETAINED: LiteraryWork is defined in slices/core/documents/."""
    graph = _graph()
    assert (GMEOW.LiteraryWork, RDFS.subClassOf, GMEOW.Work) in graph


def test_serial_work_subclasses_work() -> None:
    """RETAINED: SerialWork is defined in slices/core/documents/."""
    graph = _graph()
    assert (GMEOW.SerialWork, RDFS.subClassOf, GMEOW.Work) in graph


def test_content_segment_subclasses_information_object() -> None:
    """RETAINED: ContentSegment is defined in slices/core/documents/."""
    graph = _graph()
    assert (
        GMEOW.ContentSegment,
        RDFS.subClassOf,
        GMEOW.InformationObject,
    ) in graph


def test_has_segment_is_subproperty_of_has_part() -> None:
    """RETAINED: hasSegment is defined in slices/core/documents/."""
    graph = _graph()
    assert (GMEOW.hasSegment, RDFS.subPropertyOf, GMEOW.hasPart) in graph


def test_segment_of_is_subproperty_of_part_of() -> None:
    """RETAINED: segmentOf is defined in slices/core/documents/."""
    graph = _graph()
    assert (GMEOW.segmentOf, RDFS.subPropertyOf, GMEOW.partOf) in graph


def test_segment_type_is_functional() -> None:
    """RETAINED: segmentType is defined in slices/core/documents/."""
    graph = _graph()
    assert (GMEOW.segmentType, RDF.type, OWL.FunctionalProperty) in graph


def test_segment_index_is_functional() -> None:
    """RETAINED: segmentIndex is defined in slices/core/documents/."""
    graph = _graph()
    assert (GMEOW.segmentIndex, RDF.type, OWL.FunctionalProperty) in graph


def test_content_segment_type_value_vocab() -> None:
    """RETAINED: ContentSegmentType and its seeds are in slices/core/documents/."""
    graph = _graph()
    assert (GMEOW.ContentSegmentType, RDFS.subClassOf, GUFO.QualityValue) in graph
    for ind in (
        GMEOW.segmentTypeChapter,
        GMEOW.segmentTypeSection,
        GMEOW.segmentTypeScene,
        GMEOW.segmentTypeParagraph,
        GMEOW.segmentTypeFrontMatter,
        GMEOW.segmentTypeBackMatter,
    ):
        assert (ind, RDF.type, GMEOW.ContentSegmentType) in graph
