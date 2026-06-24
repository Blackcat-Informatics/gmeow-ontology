"""Images super-ontology (issue #22) -- retained pytest tests.

Asserted-TBox invariants for subjects local to the images module have been
migrated to slices/extensions/images/tests/structural.ttl (11 DSL cells).

RETAINED here (not migratable to the DSL):
  - Cross-slice TBox checks: test_depicts_is_subproperty_of_is_about,
    test_depicted_in_is_inverse_of_depicts (subjects in documents module),
    test_pixel_dimensions_on_media_object, test_image_orientation_on_media_object,
    test_capture_metadata_on_media_object, test_colourspace_property_exists
    (subjects in documents module), test_image_event_types_exist (subjects in
    events module).
  - SHACL well-formedness checks: all 11 run_shacl() calls below.
"""

from __future__ import annotations

from gmeow_rdf.compat.rdflib import OWL, RDF, RDFS, Graph, Literal, Namespace, URIRef
from gmeow_rdf.compat.rdflib.namespace import XSD

from gmeow_tools.graph import load_merged_graph
from tests._graph_nt import run_shacl

GMEOW = Namespace("https://blackcatinformatics.ca/gmeow/")
EX = Namespace("https://example.org/test/")


def _graph() -> Graph:
    """Load the project's merged RDF graph without following owl:imports."""
    return load_merged_graph(include_imports=False)


# =========================================================================== #
# Cross-slice TBox checks (subjects live outside images/module.ttl)
# =========================================================================== #


def test_depicts_is_subproperty_of_is_about() -> None:
    """depicts exists as an ObjectProperty, subproperty of isAbout, with
    correct domain and range. Subject in slices/core/documents/module.ttl."""
    graph = _graph()
    assert (GMEOW.depicts, RDF.type, OWL.ObjectProperty) in graph
    assert (GMEOW.depicts, RDFS.subPropertyOf, GMEOW.isAbout) in graph
    assert (GMEOW.depicts, RDFS.domain, GMEOW.MediaObject) in graph
    assert (GMEOW.depicts, RDFS.range, GMEOW.Entity) in graph


def test_depicted_in_is_inverse_of_depicts() -> None:
    """depictedIn is declared as the owl:inverseOf of depicts.
    Subject in slices/core/documents/module.ttl."""
    graph = _graph()
    assert (GMEOW.depictedIn, OWL.inverseOf, GMEOW.depicts) in graph


def test_pixel_dimensions_on_media_object() -> None:
    """pixelWidth and pixelHeight are FunctionalProperty on MediaObject.
    Subjects in slices/core/documents/module.ttl."""
    graph = _graph()
    for prop in (GMEOW.pixelWidth, GMEOW.pixelHeight):
        assert (prop, RDF.type, OWL.DatatypeProperty) in graph
        assert (prop, RDF.type, OWL.FunctionalProperty) in graph
        assert (prop, RDFS.domain, GMEOW.MediaObject) in graph
        assert (prop, RDFS.range, XSD.nonNegativeInteger) in graph


def test_image_orientation_on_media_object() -> None:
    """imageOrientation is a FunctionalProperty on MediaObject.
    Subject in slices/core/documents/module.ttl."""
    graph = _graph()
    assert (GMEOW.imageOrientation, RDF.type, OWL.DatatypeProperty) in graph
    assert (GMEOW.imageOrientation, RDF.type, OWL.FunctionalProperty) in graph
    assert (GMEOW.imageOrientation, RDFS.domain, GMEOW.MediaObject) in graph
    assert (GMEOW.imageOrientation, RDFS.range, XSD.decimal) in graph


def test_capture_metadata_on_media_object() -> None:
    """captureTime and captureDevice exist on MediaObject.
    Subjects in slices/core/documents/module.ttl."""
    graph = _graph()
    assert (GMEOW.captureTime, RDF.type, OWL.DatatypeProperty) in graph
    assert (GMEOW.captureTime, RDFS.domain, GMEOW.MediaObject) in graph
    assert (GMEOW.captureTime, RDFS.range, XSD.dateTime) in graph

    assert (GMEOW.captureDevice, RDF.type, OWL.ObjectProperty) in graph
    assert (GMEOW.captureDevice, RDFS.domain, GMEOW.MediaObject) in graph
    assert (GMEOW.captureDevice, RDFS.range, GMEOW.PhysicalObject) in graph


def test_image_event_types_exist() -> None:
    """Image-specific event type individuals are present in the graph.
    Subjects in slices/core/events/module.ttl."""
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


# =========================================================================== #
# Colourspace cross-slice check (subject in documents module)
# =========================================================================== #


def test_colourspace_property_exists() -> None:
    """colourspace is an ObjectProperty, subPropertyOf hasReferenceFrame,
    with domain MediaObject and range ReferenceFrame. NOT FunctionalProperty
    in the logical core (Principle 9). Subject in documents/module.ttl."""
    graph = _graph()
    assert (GMEOW.colourspace, RDF.type, OWL.ObjectProperty) in graph
    assert (GMEOW.colourspace, RDF.type, OWL.FunctionalProperty) not in graph
    assert (GMEOW.colourspace, RDFS.subPropertyOf, GMEOW.hasReferenceFrame) in graph
    assert (GMEOW.colourspace, RDFS.domain, GMEOW.MediaObject) in graph
    assert (GMEOW.colourspace, RDFS.range, GMEOW.ReferenceFrame) in graph


def test_media_object_colourspace_shacl_passes() -> None:
    """A MediaObject with a colourspace passes SHACL."""
    g = Graph()
    g.add((EX.img, RDF.type, GMEOW.MediaObject))
    g.add((EX.img, RDFS.label, Literal("Test Image")))
    g.add((EX.img, GMEOW.colourspace, EX.srgbFrame))
    g.add((EX.srgbFrame, RDF.type, GMEOW.ReferenceFrame))
    g.add((EX.srgbFrame, GMEOW.frameRealm, GMEOW.frameRealmColourspace))
    g.add((EX.srgbFrame, GMEOW.hasAxis, EX.axisRed))
    g.add((EX.srgbFrame, GMEOW.hasAxis, EX.axisGreen))
    g.add((EX.srgbFrame, GMEOW.hasAxis, EX.axisBlue))
    g.add(
        (
            EX.srgbFrame,
            GMEOW.dimensionCount,
            Literal(3, datatype=XSD.nonNegativeInteger),
        )
    )
    g.add((EX.srgbFrame, GMEOW.frameKind, GMEOW.frameKindCartesian))
    g.add((EX.srgbFrame, GMEOW.requiresHost, Literal(False)))
    g.add((EX.srgbFrame, GMEOW.determinacyModel, GMEOW.determinacyCrisp))
    g.add((GMEOW.frameRealmColourspace, RDF.type, GMEOW.FrameRealm))
    for axis in [EX.axisRed, EX.axisGreen, EX.axisBlue]:
        g.add((axis, RDF.type, GMEOW.Axis))
    g.add((GMEOW.frameKindCartesian, RDF.type, GMEOW.FrameKind))
    g.add((GMEOW.determinacyCrisp, RDF.type, GMEOW.Determinacy))

    result = run_shacl(g)
    assert result.ok, "\n".join(result.errors)


def test_media_object_missing_colourspace_warns_shacl() -> None:
    """A MediaObject without a colourspace triggers a SHACL warning."""
    g = Graph()
    g.add((EX.img, RDF.type, GMEOW.MediaObject))
    g.add((EX.img, RDFS.label, Literal("Test Image")))

    result = run_shacl(g)
    # Warnings do not cause result.ok to be False in our SHACL runner.
    assert result.ok, f"warning-only graph must pass; errors: {result.errors}"
    assert any("colourspace" in w.lower() for w in result.warnings), (
        f"Expected colourspace warning, got: {result.warnings}"
    )
