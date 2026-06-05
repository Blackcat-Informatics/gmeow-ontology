"""Structural + DL-safety guards for the locations building block.

Pins the value-vs-subclass decisions (Location kinds are subclasses; place kinds
and storage media are value vocabularies), the DL-safe WKT range, the
non-functional evidence-centric datatype properties, and the open-range
authority link.
"""

from __future__ import annotations

from rdflib import OWL, RDF, RDFS, Graph, URIRef
from rdflib.namespace import XSD

from gmeow_tools.graph import load_merged_graph

GMEOW = "https://blackcatinformatics.ca/gmeow/"
GUFO = "http://purl.org/nemo/gufo#"
GEO = "http://www.opengis.net/ont/geosparql#"


def _graph() -> Graph:
    return load_merged_graph(include_imports=False)


def test_location_umbrella_and_structural_subclasses() -> None:
    graph = _graph()
    assert (
        URIRef(GMEOW + "Location"),
        RDFS.subClassOf,
        URIRef(GMEOW + "Entity"),
    ) in graph
    for sub in ("Place", "VirtualLocation", "StorageLocation"):
        assert (
            URIRef(GMEOW + sub),
            RDFS.subClassOf,
            URIRef(GMEOW + "Location"),
        ) in graph


def test_place_kind_is_value_not_subclass() -> None:
    graph = _graph()
    assert (
        URIRef(GMEOW + "PlaceType"),
        RDFS.subClassOf,
        URIRef(GUFO + "QualityValue"),
    ) in graph
    place_type = URIRef(GMEOW + "placeType")
    assert (place_type, RDF.type, OWL.ObjectProperty) in graph
    assert (place_type, RDF.type, OWL.FunctionalProperty) not in graph
    for ind in (
        "placeTypeCountry",
        "placeTypeCity",
        "placeTypeRoom",
        "placeTypePremises",
    ):
        assert (URIRef(GMEOW + ind), RDF.type, URIRef(GMEOW + "PlaceType")) in graph
    # The rejected per-kind subclasses must NOT exist as classes.
    for rejected in ("Country", "City", "Building", "Room"):
        assert (URIRef(GMEOW + rejected), RDF.type, OWL.Class) not in graph


def test_storage_medium_is_value_not_subclass() -> None:
    graph = _graph()
    medium = URIRef(GMEOW + "storageMedium")
    assert (medium, RDF.type, OWL.ObjectProperty) in graph
    # Functional: the medium is constitutive of the storage location (like
    # keyScheme), unlike the descriptive placeType.
    assert (medium, RDF.type, OWL.FunctionalProperty) in graph
    for ind in ("storageMediumCloudService", "storageMediumPhysicalDisk"):
        assert (URIRef(GMEOW + ind), RDF.type, URIRef(GMEOW + "StorageMedium")) in graph
    for rejected in ("CloudStorage", "LocalStorage", "ObjectStore"):
        assert (URIRef(GMEOW + rejected), RDF.type, OWL.Class) not in graph


def test_aswkt_range_is_dl_safe() -> None:
    # The WKT range is rdfs:Literal, NOT the custom geo:wktLiteral datatype, so
    # the ontology stays in the OWL 2 DL datatype map.
    graph = _graph()
    as_wkt = URIRef(GMEOW + "asWKT")
    assert (as_wkt, RDFS.range, RDFS.Literal) in graph
    assert (as_wkt, RDFS.range, URIRef(GEO + "wktLiteral")) not in graph


def test_address_components_present_and_nonfunctional() -> None:
    graph = _graph()
    for prop in (
        "streetAddress",
        "extendedAddress",
        "postOfficeBox",
        "addressLocality",
        "addressRegion",
        "postalCode",
        "countryCode",
    ):
        node = URIRef(GMEOW + prop)
        assert (node, RDF.type, OWL.DatatypeProperty) in graph
        assert (node, RDF.type, OWL.FunctionalProperty) not in graph
    assert (
        URIRef(GMEOW + "addressPlace"),
        RDFS.range,
        URIRef(GMEOW + "Place"),
    ) in graph


def test_containedinplace_transitive_not_symmetric() -> None:
    graph = _graph()
    node = URIRef(GMEOW + "containedInPlace")
    assert (node, RDF.type, OWL.TransitiveProperty) in graph
    assert (node, RDF.type, OWL.SymmetricProperty) not in graph


def test_storedin_subproperty_of_locatedat() -> None:
    graph = _graph()
    assert (
        URIRef(GMEOW + "storedIn"),
        RDFS.subPropertyOf,
        URIRef(GMEOW + "locatedAt"),
    ) in graph


def test_authoritylink_has_open_range() -> None:
    # authorityLink links to any external authority IRI — no range axiom (open),
    # which is DL-safe.
    graph = _graph()
    assert not list(graph.objects(URIRef(GMEOW + "authorityLink"), RDFS.range))


def test_coordinate_props_nonfunctional() -> None:
    graph = _graph()
    for prop in ("latitude", "longitude", "elevation", "timezone"):
        assert (
            URIRef(GMEOW + prop),
            RDF.type,
            OWL.FunctionalProperty,
        ) not in graph
    # elevation is a decimal.
    assert (URIRef(GMEOW + "elevation"), RDFS.range, XSD.decimal) in graph
