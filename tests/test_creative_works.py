"""WEMI creative-works spine (issue #208).

Structural TBox invariants have been migrated to the declarative slicetest DSL
at slices/core/creative-works/tests/structural.ttl. This file retains only:
  - run_shacl (ExampleConformance) tests
  - cross-slice assertions whose subjects live in other modules
  - dynamic whole-graph sweeps using transitive graph traversal

RETAINED (not migrated):
  - test_creative_work_is_category: gmeow:CreativeWork defined in documents/
  - test_wemi_tiers_subclass_information_object: uses transitive_objects()
  - test_document_subclasses_work: subjects in documents/
  - test_media_etc_subclasses_manifestation: subjects in documents/
  - test_contribution_degree_value_vocab: subjects in citations/
  - test_creation_event_types_exist: subjects in events/
  - All run_shacl tests (ExampleConformance)
  - All book/narrative model tests whose subjects are in documents/
"""

from __future__ import annotations

from gmeow_rdf.compat.rdflib import OWL, RDF, RDFS, Graph, Literal, Namespace, URIRef
from gmeow_rdf.compat.rdflib.namespace import XSD

from gmeow_tools.graph import load_merged_graph
from tests._graph_nt import run_shacl

GMEOW = Namespace("https://blackcatinformatics.ca/gmeow/")
GUFO = Namespace("http://purl.org/nemo/gufo#")
EX = Namespace("https://example.org/test/")


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
# SHACL well-formedness (ExampleConformance -- not migratable to DSL)
# =========================================================================== #


def _add_work(g: Graph, work: URIRef) -> None:
    """
    Create a minimal Work resource in the graph for SHACL and unit tests.

    Adds two triples for the given resource: types it as `GMEOW.Work` and assigns the
    rdfs:label "Test Work".

    Parameters:
        g (Graph): RDFLib graph to modify.
        work (URIRef): Subject URI for the Work resource to add.
    """
    g.add((work, RDF.type, GMEOW.Work))
    g.add((work, RDFS.label, Literal("Test Work")))


def _add_expression(g: Graph, expr: URIRef, work: URIRef) -> None:
    """
    Add an Expression node to the graph and attach a populated reference frame required
    for SHACL validation.

    Parameters:
        g (Graph): The RDF graph to modify.
        expr (URIRef): The URI to use for the Expression resource to create.
        work (URIRef): The Work URI that the Expression will realize.

    Description:
        Creates triples typing `expr` as a `GMEOW.Expression`, gives it a label, links
        it to `work` via `GMEOW.realizes`, and associates it with `EX.englishFrame`.
        Also populates `EX.englishFrame` and related nodes with the properties and types
        expected by the SHACL shape (realm, axis, dimension count, frame kind, host
        requirement, and determinacy).
    """
    g.add((expr, RDF.type, GMEOW.Expression))
    g.add((expr, RDFS.label, Literal("Test Expression")))
    g.add((expr, GMEOW.realizes, work))
    g.add((expr, GMEOW.hasReferenceFrame, EX.englishFrame))
    # Populate reference frame with required SHACL properties
    g.add((EX.englishFrame, RDF.type, GMEOW.ReferenceFrame))
    g.add((EX.englishFrame, RDFS.label, Literal("English")))
    g.add((EX.englishFrame, GMEOW.frameRealm, GMEOW.frameRealmNarrative))
    g.add((EX.englishFrame, GMEOW.hasAxis, EX.axisLang))
    g.add(
        (
            EX.englishFrame,
            GMEOW.dimensionCount,
            Literal(1, datatype=XSD.nonNegativeInteger),
        )
    )
    g.add((EX.englishFrame, GMEOW.frameKind, GMEOW.frameKindNarrative))
    g.add((EX.englishFrame, GMEOW.requiresHost, Literal(False)))
    g.add((EX.englishFrame, GMEOW.determinacyModel, GMEOW.determinacyCrisp))
    g.add((GMEOW.frameRealmNarrative, RDF.type, GMEOW.FrameRealm))
    g.add((EX.axisLang, RDF.type, GMEOW.Axis))
    g.add((GMEOW.frameKindNarrative, RDF.type, GMEOW.FrameKind))
    g.add((GMEOW.determinacyCrisp, RDF.type, GMEOW.Determinacy))


def _add_manifestation(g: Graph, manif: URIRef, expr: URIRef) -> None:
    """
    Add a Manifestation node to the graph and link it to an Expression.

    Parameters:
        g (Graph): RDFLib graph to modify.
        manif (URIRef): URI of the Manifestation to create or update.
        expr (URIRef): URI of the Expression that the Manifestation embodies.
    """
    g.add((manif, RDF.type, GMEOW.Manifestation))
    g.add((manif, RDFS.label, Literal("Test Manifestation")))
    g.add((manif, GMEOW.embodies, expr))


def _add_item(g: Graph, item: URIRef, manif: URIRef) -> None:
    """
    Populate the graph with a minimal Item individual linked to a Manifestation for
    SHACL tests.

    Parameters:
        g (Graph): RDF graph to modify.
        item (URIRef): URI of the Item individual to add.
        manif (URIRef): URI of the Manifestation that the item exemplifies.
    """
    g.add((item, RDF.type, GMEOW.Item))
    g.add((item, RDFS.label, Literal("Test Item")))
    g.add((item, GMEOW.exemplifies, manif))


def test_spine_shacl_passes() -> None:
    """A fully-populated WEMI spine passes SHACL."""
    g = Graph()
    _add_work(g, EX.work)
    _add_expression(g, EX.expression, EX.work)
    _add_manifestation(g, EX.manifestation, EX.expression)
    _add_item(g, EX.item, EX.manifestation)

    result = run_shacl(g)
    assert result.ok, "\n".join(result.errors)


def test_expression_without_work_fails_shacl() -> None:
    """An Expression that realizes no Work violates SHACL."""
    g = Graph()
    g.add((EX.expression, RDF.type, GMEOW.Expression))
    g.add((EX.expression, RDFS.label, Literal("Orphan Expression")))
    g.add((EX.expression, GMEOW.hasReferenceFrame, EX.englishFrame))
    g.add((EX.englishFrame, RDF.type, GMEOW.ReferenceFrame))
    g.add((EX.englishFrame, RDFS.label, Literal("English")))
    g.add((EX.englishFrame, GMEOW.frameRealm, GMEOW.frameRealmNarrative))
    g.add((EX.englishFrame, GMEOW.hasAxis, EX.axisLang))
    g.add(
        (
            EX.englishFrame,
            GMEOW.dimensionCount,
            Literal(1, datatype=XSD.nonNegativeInteger),
        )
    )
    g.add((EX.englishFrame, GMEOW.frameKind, GMEOW.frameKindNarrative))
    g.add((EX.englishFrame, GMEOW.requiresHost, Literal(False)))
    g.add((EX.englishFrame, GMEOW.determinacyModel, GMEOW.determinacyCrisp))
    g.add((GMEOW.frameRealmNarrative, RDF.type, GMEOW.FrameRealm))
    g.add((EX.axisLang, RDF.type, GMEOW.Axis))
    g.add((GMEOW.frameKindNarrative, RDF.type, GMEOW.FrameKind))
    g.add((GMEOW.determinacyCrisp, RDF.type, GMEOW.Determinacy))

    result = run_shacl(g)
    assert not result.ok
    assert any("Expression must realize" in e for e in result.errors)


def test_manifestation_without_expression_fails_shacl() -> None:
    """A Manifestation that embodies no Expression violates SHACL."""
    g = Graph()
    g.add((EX.manifestation, RDF.type, GMEOW.Manifestation))
    g.add((EX.manifestation, RDFS.label, Literal("Orphan Manifestation")))

    result = run_shacl(g)
    assert not result.ok
    assert any("Manifestation must embody" in e for e in result.errors)


def test_item_without_manifestation_fails_shacl() -> None:
    """
    Checks that an Item without an `exemplifies` relation fails SHACL validation.

    Asserts that SHACL validation returns a failing result and that at least one
    validation error message contains the substring "Item must exemplify".
    """
    g = Graph()
    g.add((EX.item, RDF.type, GMEOW.Item))
    g.add((EX.item, RDFS.label, Literal("Orphan Item")))

    result = run_shacl(g)
    assert not result.ok
    assert any("Item must exemplify" in e for e in result.errors)


def test_contribution_shacl_passes() -> None:
    """A well-formed Contribution relator passes SHACL."""
    g = Graph()
    g.add((EX.contribution, RDF.type, GMEOW.Contribution))
    g.add((EX.contribution, GMEOW.contributor, EX.alice))
    g.add((EX.contribution, GMEOW.contributionTarget, EX.work))
    g.add((EX.contribution, GMEOW.contributionRole, GMEOW.roleAuthor))
    g.add((EX.alice, RDF.type, GMEOW.Agent))
    g.add((EX.work, RDF.type, GMEOW.Work))
    g.add((EX.work, RDFS.label, Literal("Test Work")))

    result = run_shacl(g)
    assert result.ok, "\n".join(result.errors)


def test_contribution_missing_role_fails_shacl() -> None:
    """
    Verify that a Contribution missing a contributionRole fails SHACL validation.

    Creates a minimal graph with a Contribution that has a contributor and a
    contributionTarget but no contributionRole, runs SHACL, and asserts validation fails
    and at least one error message contains "Contribution must specify exactly one
    role".
    """
    g = Graph()
    g.add((EX.contribution, RDF.type, GMEOW.Contribution))
    g.add((EX.contribution, GMEOW.contributor, EX.alice))
    g.add((EX.contribution, GMEOW.contributionTarget, EX.work))
    g.add((EX.alice, RDF.type, GMEOW.Agent))
    g.add((EX.work, RDF.type, GMEOW.Work))
    g.add((EX.work, RDFS.label, Literal("Test Work")))

    result = run_shacl(g)
    assert not result.ok
    assert any("Contribution must specify exactly one role" in e for e in result.errors)


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


def test_content_segment_shacl_passes() -> None:
    """A well-formed ContentSegment passes SHACL."""
    g = Graph()
    g.add((EX.chapter1, RDF.type, GMEOW.ContentSegment))
    g.add((EX.chapter1, RDFS.label, Literal("Chapter 1")))
    g.add((EX.chapter1, GMEOW.segmentOf, EX.book))
    g.add((EX.book, RDF.type, GMEOW.LiteraryWork))
    g.add((EX.book, RDFS.label, Literal("Test Book")))

    result = run_shacl(g)
    assert result.ok, "\n".join(result.errors)


def test_content_segment_without_container_fails_shacl() -> None:
    """A ContentSegment with no segmentOf violates SHACL."""
    g = Graph()
    g.add((EX.chapter1, RDF.type, GMEOW.ContentSegment))
    g.add((EX.chapter1, RDFS.label, Literal("Orphan Chapter")))

    result = run_shacl(g)
    assert not result.ok
    assert any("ContentSegment must be part of" in e for e in result.errors)
