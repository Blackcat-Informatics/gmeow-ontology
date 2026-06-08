"""WEMI creative-works spine (issue #208).

Pins the structural core: Work/Expression/Manifestation/Item are gufo:Kinds
beneath the CreativeWork Category; documents.ttl classes are re-homed;
Contribution relator is well-formed; value vocabularies are individuals;
creation event types are seeded.
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
    """
    Load the project's merged RDF graph without following owl:imports.

    Returns:
        g (rdflib.Graph): The merged RDF graph with imports excluded
        (include_imports=False).
    """
    return load_merged_graph(include_imports=False)


# =========================================================================== #
# Class hierarchy
# =========================================================================== #


def test_wemi_tiers_are_kinds() -> None:
    """
    Assert that each WEMI tier class (Work, Expression, Manifestation, Item) is declared
    as a gufo:Kind in the merged graph.

    The test loads the merged graph and verifies that each listed class has an
    `rdf:type` of `gufo:Kind`.
    """
    graph = _graph()
    for cls in (GMEOW.Work, GMEOW.Expression, GMEOW.Manifestation, GMEOW.Item):
        assert (cls, RDF.type, GUFO.Kind) in graph


def test_creative_work_is_category() -> None:
    graph = _graph()
    assert (GMEOW.CreativeWork, RDF.type, GUFO.Category) in graph


def test_wemi_tiers_subclass_information_object() -> None:
    """
    Verify each WEMI tier class is a subclass (transitively) of InformationObject.

    Asserts that GMEOW.Work, GMEOW.Expression, GMEOW.Manifestation, and GMEOW.Item have
    GMEOW.InformationObject in their rdfs:subClassOf closure.
    """
    graph = _graph()
    for cls in (GMEOW.Work, GMEOW.Expression, GMEOW.Manifestation, GMEOW.Item):
        assert GMEOW.InformationObject in graph.transitive_objects(cls, RDFS.subClassOf)


def test_wemi_tiers_subclass_creative_work() -> None:
    """
    Verify that each WEMI tier class is declared as a direct subclass of
    GMEOW.CreativeWork.

    Checks that GMEOW.Work, GMEOW.Expression, GMEOW.Manifestation, and GMEOW.Item each
    have an (rdfs:subClassOf, GMEOW.CreativeWork) triple in the merged graph; the test
    fails if any are missing.
    """
    graph = _graph()
    for cls in (GMEOW.Work, GMEOW.Expression, GMEOW.Manifestation, GMEOW.Item):
        assert (cls, RDFS.subClassOf, GMEOW.CreativeWork) in graph


# =========================================================================== #
# Re-homed documents.ttl classes
# =========================================================================== #


def test_document_subclasses_work() -> None:
    graph = _graph()
    for cls in (GMEOW.Document, GMEOW.Article, GMEOW.Patent, GMEOW.Dataset):
        assert (cls, RDFS.subClassOf, GMEOW.Work) in graph


def test_media_etc_subclasses_manifestation() -> None:
    """
    Assert that specific media-related classes are direct subclasses of
    GMEOW.Manifestation.

    Checks that GMEOW.MediaObject, GMEOW.WebPage, GMEOW.BookRelease, and
    GMEOW.SerialInstallment each have an explicit rdfs:subClassOf relation to
    GMEOW.Manifestation.
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
# Spine relations
# =========================================================================== #


def test_spine_relations_exist() -> None:
    """
    Asserts that each spine relation is declared as an `owl:ObjectProperty` in the
    merged graph.

    Checks that the expected WEMI spine properties (e.g., `realizes`, `realizedThrough`,
    `embodies`, `embodiedIn`, `exemplifies`, `exemplifiedBy`, `hasCarrier`) are present
    with `rdf:type owl:ObjectProperty`.
    """
    graph = _graph()
    for prop in (
        GMEOW.realizes,
        GMEOW.realizedThrough,
        GMEOW.embodies,
        GMEOW.embodiedIn,
        GMEOW.exemplifies,
        GMEOW.exemplifiedBy,
        GMEOW.hasCarrier,
    ):
        assert (prop, RDF.type, OWL.ObjectProperty) in graph


def test_realizes_inverse() -> None:
    """
    Asserts that the merged graph declares GMEOW.realizedThrough as the owl:inverseOf of
    GMEOW.realizes.

    Raises:
        AssertionError: If the inverseOf triple (GMEOW.realizedThrough, OWL.inverseOf,
        GMEOW.realizes) is not present in the graph.
    """
    graph = _graph()
    assert (GMEOW.realizedThrough, OWL.inverseOf, GMEOW.realizes) in graph


def test_embodies_inverse() -> None:
    graph = _graph()
    assert (GMEOW.embodiedIn, OWL.inverseOf, GMEOW.embodies) in graph


def test_exemplifies_inverse() -> None:
    """
    Assert that the `exemplifiedBy` property is declared as the OWL inverse of
    `exemplifies` in the merged graph.

    This test loads the merged WEMI graph and checks for the triple
    `(GMEOW.exemplifiedBy, owl:inverseOf, GMEOW.exemplifies)`.
    """
    graph = _graph()
    assert (GMEOW.exemplifiedBy, OWL.inverseOf, GMEOW.exemplifies) in graph


# =========================================================================== #
# Contribution relator
# =========================================================================== #


def test_contribution_is_relator_kind() -> None:
    """
    Check that GMEOW.Contribution is declared as a gufo Kind and as a subclass of
    gufo:Relator.

    Asserts the graph contains (GMEOW.Contribution, rdf:type, GUFO.Kind) and
    (GMEOW.Contribution, rdfs:subClassOf, GUFO.Relator).
    """
    graph = _graph()
    assert (GMEOW.Contribution, RDF.type, GUFO.Kind) in graph
    assert (GMEOW.Contribution, RDFS.subClassOf, GUFO.Relator) in graph


def test_contribution_properties_exist() -> None:
    """
    Verify that contribution-related properties are declared as functional object
    properties.

    Asserts that GMEOW.contributor, GMEOW.contributionTarget, and GMEOW.contributionRole
    each have both rdf:type owl:ObjectProperty and rdf:type owl:FunctionalProperty.
    """
    graph = _graph()
    for prop in (
        GMEOW.contributor,
        GMEOW.contributionTarget,
        GMEOW.contributionRole,
    ):
        assert (prop, RDF.type, OWL.ObjectProperty) in graph
        assert (prop, RDF.type, OWL.FunctionalProperty) in graph


def test_has_contributor_exists() -> None:
    """
    Verify that GMEOW.hasContributor is declared as an OWL ObjectProperty in the merged
    graph.
    """
    graph = _graph()
    assert (GMEOW.hasContributor, RDF.type, OWL.ObjectProperty) in graph


def test_has_author_is_subproperty_of_has_contributor() -> None:
    """
    Verify that GMEOW.hasAuthor is declared as an rdfs:subPropertyOf
    GMEOW.hasContributor in the merged graph.

    This test loads the merged graph and checks for the (hasAuthor, rdfs:subPropertyOf,
    hasContributor) triple.
    """
    graph = _graph()
    assert (GMEOW.hasAuthor, RDFS.subPropertyOf, GMEOW.hasContributor) in graph


def test_flat_shortcuts_are_subproperties() -> None:
    """
    Verify that contributor shortcut properties (hasTranslator, hasIllustrator,
    hasNarrator, hasEditor) are declared as OWL object properties and are subproperties
    of `GMEOW.hasContributor`.
    """
    graph = _graph()
    for prop in (
        GMEOW.hasTranslator,
        GMEOW.hasIllustrator,
        GMEOW.hasNarrator,
        GMEOW.hasEditor,
    ):
        assert (prop, RDF.type, OWL.ObjectProperty) in graph
        assert (prop, RDFS.subPropertyOf, GMEOW.hasContributor) in graph


# =========================================================================== #
# Value vocabularies — individuals, never subclasses (Principle 9)
# =========================================================================== #


def test_creative_work_type_value_vocab() -> None:
    """
    Assert that CreativeWorkType is a subclass of gufo:QualityValue and that the common
    work-type vocabulary members are modeled as individuals of GMEOW.CreativeWorkType.

    Verifies that GMEOW.CreativeWorkType rdfs:subClassOf GUFO.QualityValue and that each
    expected work-type resource (e.g., workTypeLiterary, workTypeWritten,
    workTypeNarrative, workTypeMusical, workTypeComposedMusical, workTypeVisual,
    workTypePhotographic, workTypeAudiovisual, workTypeFilm, workTypeChoreographic,
    workTypeCartographic, workTypeSoftware, workTypeDataset) has rdf:type
    GMEOW.CreativeWorkType.
    """
    graph = _graph()
    assert (GMEOW.CreativeWorkType, RDFS.subClassOf, GUFO.QualityValue) in graph
    for ind in (
        GMEOW.workTypeLiterary,
        GMEOW.workTypeWritten,
        GMEOW.workTypeNarrative,
        GMEOW.workTypeMusical,
        GMEOW.workTypeComposedMusical,
        GMEOW.workTypeVisual,
        GMEOW.workTypePhotographic,
        GMEOW.workTypeAudiovisual,
        GMEOW.workTypeFilm,
        GMEOW.workTypeChoreographic,
        GMEOW.workTypeCartographic,
        GMEOW.workTypeSoftware,
        GMEOW.workTypeDataset,
    ):
        assert (ind, RDF.type, GMEOW.CreativeWorkType) in graph


def test_contribution_role_value_vocab() -> None:
    """
    Verify the contribution-role vocabulary is modeled as a value class and that its
    members are individuals.

    Asserts that `GMEOW.ContributionRole` is a subclass of `gufo:QualityValue` and that
    each expected contribution role (e.g., `roleAuthor`, `roleEditor`, `roleTranslator`,
    etc.) is typed as an instance of `GMEOW.ContributionRole`.
    """
    graph = _graph()
    assert (GMEOW.ContributionRole, RDFS.subClassOf, GUFO.QualityValue) in graph
    for ind in (
        GMEOW.roleAuthor,
        GMEOW.roleEditor,
        GMEOW.roleTranslator,
        GMEOW.roleIllustrator,
        GMEOW.roleNarrator,
        GMEOW.roleComposer,
        GMEOW.roleDirector,
        GMEOW.rolePhotographer,
        GMEOW.roleCoverArtist,
        GMEOW.roleLetterer,
        GMEOW.roleLLMAssistedEditor,
    ):
        assert (ind, RDF.type, GMEOW.ContributionRole) in graph


def test_manifestation_format_value_vocab() -> None:
    """
    Verify that GMEOW.ManifestationFormat is a subclass of GUFO.QualityValue and that
    each expected manifestation format is an individual of GMEOW.ManifestationFormat.

    Asserts the subclass relationship and that the listed format resources (hardcover,
    paperback, EPUB, PDF, audiobook, vinyl, digital file, web page) have rdf:type
    GMEOW.ManifestationFormat.
    """
    graph = _graph()
    assert (GMEOW.ManifestationFormat, RDFS.subClassOf, GUFO.QualityValue) in graph
    for ind in (
        GMEOW.formatHardcover,
        GMEOW.formatPaperback,
        GMEOW.formatEPUB,
        GMEOW.formatPDF,
        GMEOW.formatAudiobook,
        GMEOW.formatVinyl,
        GMEOW.formatDigitalFile,
        GMEOW.formatWebPage,
    ):
        assert (ind, RDF.type, GMEOW.ManifestationFormat) in graph


def test_carrier_medium_value_vocab() -> None:
    """
    Verify that GMEOW.CarrierMedium is declared as a subclass of gufo:QualityValue and
    that specific carrier medium resources are modeled as individuals of
    GMEOW.CarrierMedium.

    Checks that the following resources are typed as instances of GMEOW.CarrierMedium:
    GMEOW.mediumPrint, GMEOW.mediumEInkFile, GMEOW.mediumOpticalDisc, and
    GMEOW.mediumServerObject.
    """
    graph = _graph()
    assert (GMEOW.CarrierMedium, RDFS.subClassOf, GUFO.QualityValue) in graph
    for ind in (
        GMEOW.mediumPrint,
        GMEOW.mediumEInkFile,
        GMEOW.mediumOpticalDisc,
        GMEOW.mediumServerObject,
    ):
        assert (ind, RDF.type, GMEOW.CarrierMedium) in graph


# =========================================================================== #
# Creation event types
# =========================================================================== #


def test_creation_event_types_exist() -> None:
    """
    Verify that the expected creation event type individuals are present in the merged
    graph.

    Checks that the individuals GMEOW.eventTypeWorkConception,
    GMEOW.eventTypeExpressionCreation,
    and GMEOW.eventTypeManifestationProduction are each typed as GMEOW.EventType.
    """
    graph = _graph()
    for ind in (
        GMEOW.eventTypeWorkConception,
        GMEOW.eventTypeExpressionCreation,
        GMEOW.eventTypeManifestationProduction,
    ):
        assert (ind, RDF.type, GMEOW.EventType) in graph


# =========================================================================== #
# SHACL well-formedness
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
