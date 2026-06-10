"""Images super-ontology (issue #22).

Pins the full eight-layer image model: contextual depiction, region encoding,
scene graphs, technical metadata, provenance, rights reuse, and SEO/discovery
projections.
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


def test_depiction_usage_is_relator_kind() -> None:
    """DepictionUsage is a gufo:SubKind subclass of Observation and Relator."""
    graph = _graph()
    assert (GMEOW.DepictionUsage, RDF.type, GUFO.SubKind) in graph
    assert (GMEOW.DepictionUsage, RDFS.subClassOf, GMEOW.Observation) in graph
    assert (GMEOW.DepictionUsage, RDFS.subClassOf, GUFO.Relator) in graph


def test_depicts_is_subproperty_of_is_about() -> None:
    """depicts exists as an ObjectProperty, subproperty of isAbout, with correct
    domain and range."""
    graph = _graph()
    assert (GMEOW.depicts, RDF.type, OWL.ObjectProperty) in graph
    assert (GMEOW.depicts, RDFS.subPropertyOf, GMEOW.isAbout) in graph
    assert (GMEOW.depicts, RDFS.domain, GMEOW.MediaObject) in graph
    assert (GMEOW.depicts, RDFS.range, GMEOW.Entity) in graph


def test_depicted_in_is_inverse_of_depicts() -> None:
    """depictedIn is declared as the owl:inverseOf of depicts."""
    graph = _graph()
    assert (GMEOW.depictedIn, OWL.inverseOf, GMEOW.depicts) in graph


def test_image_region_is_kind() -> None:
    """ImageRegion is a gufo:Kind subclass of InformationObject."""
    graph = _graph()
    assert (GMEOW.ImageRegion, RDF.type, GUFO.Kind) in graph
    assert (GMEOW.ImageRegion, RDFS.subClassOf, GMEOW.InformationObject) in graph


def test_region_selector_is_kind() -> None:
    """RegionSelector is a gufo:Kind subclass of InformationObject."""
    graph = _graph()
    assert (GMEOW.RegionSelector, RDF.type, GUFO.Kind) in graph
    assert (GMEOW.RegionSelector, RDFS.subClassOf, GMEOW.InformationObject) in graph


def test_scene_graph_edge_is_relator_kind() -> None:
    """SceneGraphEdge is a gufo:Kind subclass of Relator."""
    graph = _graph()
    assert (GMEOW.SceneGraphEdge, RDF.type, GUFO.Kind) in graph
    assert (GMEOW.SceneGraphEdge, RDFS.subClassOf, GUFO.Relator) in graph


# =========================================================================== #
# Relator role properties
# =========================================================================== #


def test_depiction_usage_roles_exist() -> None:
    """DepictionUsage role properties: domains, ranges, functionality."""
    graph = _graph()
    for prop, rng, is_func in (
        (GMEOW.depictionSubject, GMEOW.Entity, True),
        (GMEOW.depictionImage, GMEOW.MediaObject, True),
        (GMEOW.depictionContext, GMEOW.DepictionContext, True),
        (GMEOW.depictionAudience, GMEOW.Entity, False),
        (GMEOW.depictionInterval, GMEOW.TimeInterval, False),
        (GMEOW.depictionAuthority, GMEOW.Agent, False),
    ):
        assert (prop, RDF.type, OWL.ObjectProperty) in graph
        assert (prop, RDFS.domain, GMEOW.DepictionUsage) in graph
        assert (prop, RDFS.range, rng) in graph
        if is_func:
            assert (prop, RDF.type, OWL.FunctionalProperty) in graph


def test_image_region_properties_exist() -> None:
    """ImageRegion and RegionSelector properties have correct domains/ranges."""
    graph = _graph()
    assert (GMEOW.hasRegion, RDF.type, OWL.ObjectProperty) in graph
    assert (GMEOW.hasRegion, RDFS.domain, GMEOW.MediaObject) in graph
    assert (GMEOW.hasRegion, RDFS.range, GMEOW.ImageRegion) in graph

    assert (GMEOW.regionOf, RDF.type, OWL.ObjectProperty) in graph
    assert (GMEOW.regionOf, RDF.type, OWL.FunctionalProperty) in graph
    assert (GMEOW.regionOf, RDFS.domain, GMEOW.ImageRegion) in graph
    assert (GMEOW.regionOf, RDFS.range, GMEOW.MediaObject) in graph

    assert (GMEOW.regionSelector, RDF.type, OWL.ObjectProperty) in graph
    assert (GMEOW.regionSelector, RDF.type, OWL.FunctionalProperty) in graph
    assert (GMEOW.regionSelector, RDFS.domain, GMEOW.ImageRegion) in graph
    assert (GMEOW.regionSelector, RDFS.range, GMEOW.RegionSelector) in graph

    assert (GMEOW.selectorType, RDF.type, OWL.ObjectProperty) in graph
    assert (GMEOW.selectorType, RDF.type, OWL.FunctionalProperty) in graph
    assert (GMEOW.selectorType, RDFS.domain, GMEOW.RegionSelector) in graph
    assert (GMEOW.selectorType, RDFS.range, GMEOW.SelectorType) in graph

    assert (GMEOW.selectorValue, RDF.type, OWL.DatatypeProperty) in graph
    assert (GMEOW.selectorValue, RDFS.domain, GMEOW.RegionSelector) in graph

    assert (GMEOW.regionLabel, RDF.type, OWL.DatatypeProperty) in graph
    assert (GMEOW.regionLabel, RDFS.domain, GMEOW.ImageRegion) in graph


def test_scene_graph_edge_properties_exist() -> None:
    """SceneGraphEdge properties have correct domains, ranges, and functionality."""
    graph = _graph()
    for prop, rng in (
        (GMEOW.sceneSubject, GMEOW.ImageRegion),
        (GMEOW.sceneObject, GMEOW.ImageRegion),
        (GMEOW.sceneRelation, GMEOW.SceneRelationType),
    ):
        assert (prop, RDF.type, OWL.ObjectProperty) in graph
        assert (prop, RDF.type, OWL.FunctionalProperty) in graph
        assert (prop, RDFS.domain, GMEOW.SceneGraphEdge) in graph
        assert (prop, RDFS.range, rng) in graph

    assert (GMEOW.sceneConfidence, RDF.type, OWL.DatatypeProperty) in graph
    assert (GMEOW.sceneConfidence, RDF.type, OWL.FunctionalProperty) in graph
    assert (GMEOW.sceneConfidence, RDFS.domain, GMEOW.SceneGraphEdge) in graph


# =========================================================================== #
# Technical metadata on MediaObject
# =========================================================================== #


def test_pixel_dimensions_on_media_object() -> None:
    """pixelWidth and pixelHeight are FunctionalProperty on MediaObject."""
    graph = _graph()
    for prop in (GMEOW.pixelWidth, GMEOW.pixelHeight):
        assert (prop, RDF.type, OWL.DatatypeProperty) in graph
        assert (prop, RDF.type, OWL.FunctionalProperty) in graph
        assert (prop, RDFS.domain, GMEOW.MediaObject) in graph
        assert (prop, RDFS.range, XSD.nonNegativeInteger) in graph


def test_image_orientation_on_media_object() -> None:
    """imageOrientation is a FunctionalProperty on MediaObject."""
    graph = _graph()
    assert (GMEOW.imageOrientation, RDF.type, OWL.DatatypeProperty) in graph
    assert (GMEOW.imageOrientation, RDF.type, OWL.FunctionalProperty) in graph
    assert (GMEOW.imageOrientation, RDFS.domain, GMEOW.MediaObject) in graph
    assert (GMEOW.imageOrientation, RDFS.range, XSD.decimal) in graph


def test_capture_metadata_on_media_object() -> None:
    """captureTime and captureDevice exist on MediaObject."""
    graph = _graph()
    assert (GMEOW.captureTime, RDF.type, OWL.DatatypeProperty) in graph
    assert (GMEOW.captureTime, RDFS.domain, GMEOW.MediaObject) in graph
    assert (GMEOW.captureTime, RDFS.range, XSD.dateTime) in graph

    assert (GMEOW.captureDevice, RDF.type, OWL.ObjectProperty) in graph
    assert (GMEOW.captureDevice, RDFS.domain, GMEOW.MediaObject) in graph
    assert (GMEOW.captureDevice, RDFS.range, GMEOW.PhysicalObject) in graph


# =========================================================================== #
# Value vocabularies — individuals, never subclasses (Principle 9)
# =========================================================================== #


def test_selector_type_value_vocab() -> None:
    """SelectorType is a gufo:QualityValue and its seeds are individuals."""
    graph = _graph()
    assert (GMEOW.SelectorType, RDFS.subClassOf, GUFO.QualityValue) in graph
    for ind in (
        GMEOW.selectorTypeSvgPath,
        GMEOW.selectorTypePixelRectangle,
        GMEOW.selectorTypeFractionalRectangle,
        GMEOW.selectorTypePolygonPath,
        GMEOW.selectorTypeRunLengthEncoded,
        GMEOW.selectorTypeCocoRleMask,
        GMEOW.selectorTypeDicomSegMask,
        GMEOW.selectorTypePixelMask,
        GMEOW.selectorTypeWebAnnotationFragment,
    ):
        assert (ind, RDF.type, GMEOW.SelectorType) in graph


def test_depiction_context_value_vocab() -> None:
    """DepictionContext is a gufo:QualityValue and its seeds are individuals."""
    graph = _graph()
    assert (GMEOW.DepictionContext, RDFS.subClassOf, GUFO.QualityValue) in graph
    for ind in (
        GMEOW.depictionContextWork,
        GMEOW.depictionContextFamily,
        GMEOW.depictionContextChildhood,
        GMEOW.depictionContextNow,
        GMEOW.depictionContextPortrait,
        GMEOW.depictionContextCandid,
        GMEOW.depictionContextFormal,
        GMEOW.depictionContextProfessional,
        GMEOW.depictionContextSocial,
        GMEOW.depictionContextSelfPortrait,
        GMEOW.depictionContextActionShot,
        GMEOW.depictionContextEvent,
    ):
        assert (ind, RDF.type, GMEOW.DepictionContext) in graph


def test_scene_relation_type_value_vocab() -> None:
    """SceneRelationType is a gufo:QualityValue and its seeds are individuals."""
    graph = _graph()
    assert (GMEOW.SceneRelationType, RDFS.subClassOf, GUFO.QualityValue) in graph
    for ind in (
        GMEOW.sceneRelationLeftOf,
        GMEOW.sceneRelationRightOf,
        GMEOW.sceneRelationAbove,
        GMEOW.sceneRelationBelow,
        GMEOW.sceneRelationInside,
        GMEOW.sceneRelationTouching,
        GMEOW.sceneRelationNear,
        GMEOW.sceneRelationFarFrom,
        GMEOW.sceneRelationSameAs,
        GMEOW.sceneRelationPartOf,
        GMEOW.sceneRelationHolding,
        GMEOW.sceneRelationWearing,
        GMEOW.sceneRelationRiding,
        GMEOW.sceneRelationEating,
        GMEOW.sceneRelationPlaying,
    ):
        assert (ind, RDF.type, GMEOW.SceneRelationType) in graph


# =========================================================================== #
# Image event types
# =========================================================================== #


def test_image_event_types_exist() -> None:
    """Image-specific event type individuals are present in the graph."""
    graph = _graph()
    for ind in (
        GMEOW.eventTypeImageCapture,
        GMEOW.eventTypeImageScanning,
        GMEOW.eventTypeImageProcessing,
        GMEOW.eventTypeImageAnnotation,
    ):
        assert (ind, RDF.type, GMEOW.EventType) in graph


# =========================================================================== #
# SHACL well-formedness
# =========================================================================== #


def _add_media_object(g: Graph, img: URIRef) -> None:
    """Create a minimal MediaObject resource in the graph."""
    g.add((img, RDF.type, GMEOW.MediaObject))
    g.add((img, RDFS.label, Literal("Test Image")))


def test_depiction_usage_shacl_passes() -> None:
    """A well-formed DepictionUsage passes SHACL."""
    g = Graph()
    g.add((EX.usage, RDF.type, GMEOW.DepictionUsage))
    g.add((EX.alice, RDF.type, GMEOW.Entity))
    g.add((EX.usage, GMEOW.depictionSubject, EX.alice))
    g.add((EX.usage, GMEOW.depictionImage, EX.img))
    g.add((EX.usage, GMEOW.depictionContext, GMEOW.depictionContextPortrait))
    g.add((GMEOW.depictionContextPortrait, RDF.type, GMEOW.DepictionContext))
    _add_media_object(g, EX.img)

    result = run_shacl(g)
    assert result.ok, "\n".join(result.errors)


def test_depiction_usage_missing_image_fails_shacl() -> None:
    """A DepictionUsage without a depictionImage violates SHACL."""
    g = Graph()
    g.add((EX.usage, RDF.type, GMEOW.DepictionUsage))
    g.add((EX.alice, RDF.type, GMEOW.Entity))
    g.add((EX.usage, GMEOW.depictionSubject, EX.alice))
    g.add((EX.usage, GMEOW.depictionContext, GMEOW.depictionContextPortrait))
    g.add((GMEOW.depictionContextPortrait, RDF.type, GMEOW.DepictionContext))

    result = run_shacl(g)
    assert not result.ok
    assert any("depictionImage" in e for e in result.errors)


def test_image_region_shacl_passes() -> None:
    """A well-formed ImageRegion passes SHACL."""
    g = Graph()
    g.add((EX.region, RDF.type, GMEOW.ImageRegion))
    g.add((EX.region, RDFS.label, Literal("Test Region")))
    g.add((EX.region, GMEOW.regionOf, EX.img))
    g.add((EX.region, GMEOW.regionSelector, EX.sel))
    g.add((EX.sel, RDF.type, GMEOW.RegionSelector))
    g.add((EX.sel, GMEOW.selectorType, GMEOW.selectorTypePixelRectangle))
    g.add((EX.sel, GMEOW.selectorValue, Literal("10,20,100,200")))
    g.add((GMEOW.selectorTypePixelRectangle, RDF.type, GMEOW.SelectorType))
    _add_media_object(g, EX.img)

    result = run_shacl(g)
    assert result.ok, "\n".join(result.errors)


def test_image_region_missing_selector_fails_shacl() -> None:
    """An ImageRegion without a regionSelector violates SHACL."""
    g = Graph()
    g.add((EX.region, RDF.type, GMEOW.ImageRegion))
    g.add((EX.region, RDFS.label, Literal("Orphan Region")))
    g.add((EX.region, GMEOW.regionOf, EX.img))
    _add_media_object(g, EX.img)

    result = run_shacl(g)
    assert not result.ok
    assert any("regionSelector" in e for e in result.errors)


def test_scene_graph_edge_shacl_passes() -> None:
    """A well-formed SceneGraphEdge passes SHACL."""
    g = Graph()
    g.add((EX.edge, RDF.type, GMEOW.SceneGraphEdge))
    g.add((EX.edge, GMEOW.sceneSubject, EX.region1))
    g.add((EX.edge, GMEOW.sceneObject, EX.region2))
    g.add((EX.edge, GMEOW.sceneRelation, GMEOW.sceneRelationLeftOf))
    g.add((EX.edge, GMEOW.sceneConfidence, Literal("0.95", datatype=XSD.decimal)))
    g.add((EX.region1, RDF.type, GMEOW.ImageRegion))
    g.add((EX.region1, GMEOW.regionOf, EX.img))
    g.add((EX.region1, GMEOW.regionSelector, EX.sel1))
    g.add((EX.sel1, RDF.type, GMEOW.RegionSelector))
    g.add((EX.sel1, GMEOW.selectorType, GMEOW.selectorTypePixelRectangle))
    g.add((EX.sel1, GMEOW.selectorValue, Literal("0,0,50,50")))
    g.add((EX.region2, RDF.type, GMEOW.ImageRegion))
    g.add((EX.region2, GMEOW.regionOf, EX.img))
    g.add((EX.region2, GMEOW.regionSelector, EX.sel2))
    g.add((EX.sel2, RDF.type, GMEOW.RegionSelector))
    g.add((EX.sel2, GMEOW.selectorType, GMEOW.selectorTypePixelRectangle))
    g.add((EX.sel2, GMEOW.selectorValue, Literal("60,0,50,50")))
    g.add((GMEOW.sceneRelationLeftOf, RDF.type, GMEOW.SceneRelationType))
    g.add((GMEOW.selectorTypePixelRectangle, RDF.type, GMEOW.SelectorType))
    _add_media_object(g, EX.img)

    result = run_shacl(g)
    assert result.ok, "\n".join(result.errors)


def test_scene_graph_edge_missing_relation_fails_shacl() -> None:
    """A SceneGraphEdge without a sceneRelation violates SHACL."""
    g = Graph()
    g.add((EX.edge, RDF.type, GMEOW.SceneGraphEdge))
    g.add((EX.edge, GMEOW.sceneSubject, EX.region1))
    g.add((EX.edge, GMEOW.sceneObject, EX.region2))
    g.add((EX.region1, RDF.type, GMEOW.ImageRegion))
    g.add((EX.region1, GMEOW.regionOf, EX.img))
    g.add((EX.region1, GMEOW.regionSelector, EX.sel1))
    g.add((EX.sel1, RDF.type, GMEOW.RegionSelector))
    g.add((EX.sel1, GMEOW.selectorType, GMEOW.selectorTypePixelRectangle))
    g.add((EX.sel1, GMEOW.selectorValue, Literal("0,0,50,50")))
    g.add((EX.region2, RDF.type, GMEOW.ImageRegion))
    g.add((EX.region2, GMEOW.regionOf, EX.img))
    g.add((EX.region2, GMEOW.regionSelector, EX.sel2))
    g.add((EX.sel2, RDF.type, GMEOW.RegionSelector))
    g.add((EX.sel2, GMEOW.selectorType, GMEOW.selectorTypePixelRectangle))
    g.add((EX.sel2, GMEOW.selectorValue, Literal("60,0,50,50")))
    _add_media_object(g, EX.img)

    result = run_shacl(g)
    assert not result.ok
    assert any("sceneRelation" in e for e in result.errors)


def test_region_selector_missing_value_fails_shacl() -> None:
    """A RegionSelector without selectorValue violates SHACL."""
    g = Graph()
    g.add((EX.sel, RDF.type, GMEOW.RegionSelector))
    g.add((EX.sel, GMEOW.selectorType, GMEOW.selectorTypePixelRectangle))
    g.add((GMEOW.selectorTypePixelRectangle, RDF.type, GMEOW.SelectorType))

    result = run_shacl(g)
    assert not result.ok
    assert any("selectorValue" in e for e in result.errors)


def test_scene_graph_edge_confidence_out_of_range_fails_shacl() -> None:
    """A SceneGraphEdge with sceneConfidence > 1.0 violates SHACL."""
    g = Graph()
    g.add((EX.edge, RDF.type, GMEOW.SceneGraphEdge))
    g.add((EX.edge, GMEOW.sceneSubject, EX.region1))
    g.add((EX.edge, GMEOW.sceneObject, EX.region2))
    g.add((EX.edge, GMEOW.sceneRelation, GMEOW.sceneRelationLeftOf))
    g.add((EX.edge, GMEOW.sceneConfidence, Literal("1.5", datatype=XSD.decimal)))
    g.add((EX.region1, RDF.type, GMEOW.ImageRegion))
    g.add((EX.region1, GMEOW.regionOf, EX.img))
    g.add((EX.region1, GMEOW.regionSelector, EX.sel1))
    g.add((EX.sel1, RDF.type, GMEOW.RegionSelector))
    g.add((EX.sel1, GMEOW.selectorType, GMEOW.selectorTypePixelRectangle))
    g.add((EX.sel1, GMEOW.selectorValue, Literal("0,0,50,50")))
    g.add((EX.region2, RDF.type, GMEOW.ImageRegion))
    g.add((EX.region2, GMEOW.regionOf, EX.img))
    g.add((EX.region2, GMEOW.regionSelector, EX.sel2))
    g.add((EX.sel2, RDF.type, GMEOW.RegionSelector))
    g.add((EX.sel2, GMEOW.selectorType, GMEOW.selectorTypePixelRectangle))
    g.add((EX.sel2, GMEOW.selectorValue, Literal("60,0,50,50")))
    g.add((GMEOW.sceneRelationLeftOf, RDF.type, GMEOW.SceneRelationType))
    g.add((GMEOW.selectorTypePixelRectangle, RDF.type, GMEOW.SelectorType))
    _add_media_object(g, EX.img)

    result = run_shacl(g)
    assert not result.ok
    assert any("sceneConfidence" in e for e in result.errors)


def test_depiction_usage_multiple_subjects_fails_shacl() -> None:
    """A DepictionUsage with more than one depictionSubject violates SHACL."""
    g = Graph()
    g.add((EX.usage, RDF.type, GMEOW.DepictionUsage))
    g.add((EX.alice, RDF.type, GMEOW.Entity))
    g.add((EX.bob, RDF.type, GMEOW.Entity))
    g.add((EX.usage, GMEOW.depictionSubject, EX.alice))
    g.add((EX.usage, GMEOW.depictionSubject, EX.bob))
    g.add((EX.usage, GMEOW.depictionImage, EX.img))
    g.add((EX.usage, GMEOW.depictionContext, GMEOW.depictionContextPortrait))
    g.add((GMEOW.depictionContextPortrait, RDF.type, GMEOW.DepictionContext))
    _add_media_object(g, EX.img)

    result = run_shacl(g)
    assert not result.ok
    assert any("depictionSubject" in e for e in result.errors)
