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
  - SHACL well-formedness checks: migrated to
    crates/validate/tests/conformance_images.rs (#867).
"""

from __future__ import annotations

from gmeow_rdf.compat.rdflib import OWL, RDF, RDFS, Graph, Namespace
from gmeow_rdf.compat.rdflib.namespace import XSD

from gmeow_tools.graph import load_merged_graph

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
