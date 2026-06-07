"""Structural + DL-safety guards for the locations building block.

Pins the value-vs-subclass decisions (Location kinds are subclasses; place kinds
and storage media are value vocabularies), the DL-safe WKT range, the
non-functional evidence-centric datatype properties, and the open-range
authority link.
"""

from __future__ import annotations

from pathlib import Path

from rdflib import OWL, RDF, RDFS, Graph, Literal, Namespace, URIRef
from rdflib.namespace import XSD

from gmeow_tools.graph import load_merged_graph
from gmeow_tools.validate import run_shacl

GMEOW = "https://blackcatinformatics.ca/gmeow/"
GUFO = "http://purl.org/nemo/gufo#"
GEO = "http://www.opengis.net/ont/geosparql#"
EX_PLACES = Namespace("https://blackcatinformatics.ca/gmeow/examples/places/")
COVERAGE_FIXTURES = Path(__file__).parent / "fixtures" / "coverage"


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


def test_alternate_name_retired() -> None:
    """Greenfield (Principle 6, issue #105): the flat gmeow:alternateName literal was
    retired in favour of co-equal gmeow:PlaceName appellations borne via
    gmeow:hasPlaceName (names module). It must not exist as any property."""
    graph = _graph()
    alt = URIRef(GMEOW + "alternateName")
    assert (alt, RDF.type, OWL.DatatypeProperty) not in graph
    assert (alt, RDF.type, OWL.ObjectProperty) not in graph
    # The structured replacement bears a PlaceName on a Place.
    assert (
        URIRef(GMEOW + "hasPlaceName"),
        RDFS.range,
        URIRef(GMEOW + "PlaceName"),
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


def test_location_superset_core() -> None:
    """Verifies classes, value scaffolds, properties, and topology of Location Core."""
    graph = _graph()

    # 1. New classes
    for cls in (
        "ReferenceFrame",
        "Axis",
        "SpatialCoordinates",
        "Pose",
        "Orientation",
        "FrameRealm",
        "FrameKind",
        "LocationState",
        "Trajectory",
    ):
        ref = URIRef(GMEOW + cls)
        assert (ref, RDF.type, OWL.Class) in graph

    # 2. Value scaffold individuals
    for ind in (
        "frameRealmTerrestrial",
        "frameRealmIndoor",
        "frameRealmVirtual",
        "frameRealmCelestial",
        "frameRealmRobotic",
    ):
        assert (URIRef(GMEOW + ind), RDF.type, URIRef(GMEOW + "FrameRealm")) in graph
    for ind in (
        "frameKindGeodetic",
        "frameKindCartesian",
        "frameKindPolar",
        "frameKindGrid",
        "frameKindScalar",
        "frameKindTemporal",
        "frameKindCylindrical",
    ):
        assert (URIRef(GMEOW + ind), RDF.type, URIRef(GMEOW + "FrameKind")) in graph
    for ind in (
        "frameRealmMeasurement",
        "frameRealmCurrency",
        "frameRealmTemporal",
        "frameRealmColourspace",
        "frameRealmLinguistic",
    ):
        assert (URIRef(GMEOW + ind), RDF.type, URIRef(GMEOW + "FrameRealm")) in graph
    for ind in (
        "axisYear",
        "axisMonth",
        "axisDay",
        "axisHour",
        "axisMinute",
        "axisSecond",
        "axisRed",
        "axisGreen",
        "axisBlue",
        "axisCyan",
        "axisMagenta",
        "axisYellow",
        "axisKey",
        "axisScalar",
        "axisYaw",
        "axisPitch",
        "axisRoll",
        "axisQuaternionX",
        "axisQuaternionY",
        "axisQuaternionZ",
        "axisQuaternionW",
        "axisHeading",
        "axisBearing",
    ):
        assert (URIRef(GMEOW + ind), RDF.type, URIRef(GMEOW + "Axis")) in graph
    for ind in (
        "referenceFrameSI",
        "referenceFrameUSD",
        "referenceFrameGregorian",
        "referenceFrameUnixEpoch",
        "referenceFrameSRGB",
        "referenceFrameCMYK",
        "referenceFrameEnglish",
    ):
        assert (
            URIRef(GMEOW + ind),
            RDF.type,
            URIRef(GMEOW + "ReferenceFrame"),
        ) in graph

    # 3. New Properties domain & range
    assert (
        URIRef(GMEOW + "frameRealm"),
        RDFS.domain,
        URIRef(GMEOW + "ReferenceFrame"),
    ) in graph
    assert (
        URIRef(GMEOW + "frameRealm"),
        RDFS.range,
        URIRef(GMEOW + "FrameRealm"),
    ) in graph
    assert (URIRef(GMEOW + "frameRealm"), RDF.type, OWL.FunctionalProperty) in graph
    assert (
        URIRef(GMEOW + "hasAxis"),
        RDFS.domain,
        URIRef(GMEOW + "ReferenceFrame"),
    ) in graph
    assert (URIRef(GMEOW + "hasAxis"), RDFS.range, URIRef(GMEOW + "Axis")) in graph
    assert (
        URIRef(GMEOW + "dimensionCount"),
        RDFS.domain,
        URIRef(GMEOW + "ReferenceFrame"),
    ) in graph
    assert (
        URIRef(GMEOW + "dimensionCount"),
        RDFS.range,
        XSD.nonNegativeInteger,
    ) in graph
    assert (URIRef(GMEOW + "dimensionCount"), RDF.type, OWL.FunctionalProperty) in graph
    assert (
        URIRef(GMEOW + "frameKind"),
        RDFS.domain,
        URIRef(GMEOW + "ReferenceFrame"),
    ) in graph
    assert (
        URIRef(GMEOW + "frameKind"),
        RDFS.range,
        URIRef(GMEOW + "FrameKind"),
    ) in graph
    assert (URIRef(GMEOW + "frameKind"), RDF.type, OWL.FunctionalProperty) in graph
    assert (
        URIRef(GMEOW + "requiresHost"),
        RDFS.domain,
        URIRef(GMEOW + "ReferenceFrame"),
    ) in graph
    assert (URIRef(GMEOW + "requiresHost"), RDFS.range, XSD.boolean) in graph
    assert (URIRef(GMEOW + "requiresHost"), RDF.type, OWL.FunctionalProperty) in graph
    assert (
        URIRef(GMEOW + "parentFrame"),
        RDFS.domain,
        URIRef(GMEOW + "ReferenceFrame"),
    ) in graph
    assert (
        URIRef(GMEOW + "parentFrame"),
        RDFS.range,
        URIRef(GMEOW + "ReferenceFrame"),
    ) in graph
    assert (URIRef(GMEOW + "parentFrame"), RDF.type, OWL.FunctionalProperty) in graph
    assert (
        URIRef(GMEOW + "transformsTo"),
        RDFS.domain,
        URIRef(GMEOW + "ReferenceFrame"),
    ) in graph
    assert (
        URIRef(GMEOW + "transformsTo"),
        RDFS.range,
        URIRef(GMEOW + "ReferenceFrame"),
    ) in graph
    assert (
        URIRef(GMEOW + "frameSolver"),
        RDFS.domain,
        URIRef(GMEOW + "ReferenceFrame"),
    ) in graph
    assert (URIRef(GMEOW + "frameSolver"), RDFS.range, RDFS.Literal) in graph
    assert (
        URIRef(GMEOW + "determinacyModel"),
        RDFS.domain,
        URIRef(GMEOW + "ReferenceFrame"),
    ) in graph
    assert (
        URIRef(GMEOW + "determinacyModel"),
        RDFS.range,
        URIRef(GMEOW + "Determinacy"),
    ) in graph
    assert (
        URIRef(GMEOW + "determinacyModel"),
        RDF.type,
        OWL.FunctionalProperty,
    ) in graph
    assert (
        URIRef(GMEOW + "coordinateFrame"),
        RDFS.domain,
        URIRef(GMEOW + "SpatialCoordinates"),
    ) in graph
    assert (
        URIRef(GMEOW + "coordinateFrame"),
        RDFS.range,
        URIRef(GMEOW + "ReferenceFrame"),
    ) in graph
    assert (
        URIRef(GMEOW + "coordinateFrame"),
        RDF.type,
        OWL.FunctionalProperty,
    ) in graph
    assert (
        URIRef(GMEOW + "hasReferenceFrame"),
        RDF.type,
        OWL.ObjectProperty,
    ) in graph
    assert (
        URIRef(GMEOW + "coordinateFrame"),
        RDFS.subPropertyOf,
        URIRef(GMEOW + "hasReferenceFrame"),
    ) in graph
    # hasCoordinateMatrix domain is an owl:unionOf(Axis, Pose).
    hcm_domain = graph.value(URIRef(GMEOW + "hasCoordinateMatrix"), RDFS.domain)
    assert hcm_domain is not None
    assert (hcm_domain, RDF.type, OWL.Class) in graph
    union_of = graph.value(hcm_domain, OWL.unionOf)
    assert union_of is not None
    union_members = set(graph.items(union_of))
    assert union_members == {
        URIRef(GMEOW + "Axis"),
        URIRef(GMEOW + "Pose"),
    }
    assert (URIRef(GMEOW + "hasCoordinateMatrix"), RDFS.range, RDFS.Literal) in graph

    # Pose / Orientation properties
    assert (
        URIRef(GMEOW + "hasPose"),
        RDFS.domain,
        URIRef(GMEOW + "Entity"),
    ) in graph
    assert (
        URIRef(GMEOW + "hasPose"),
        RDFS.range,
        URIRef(GMEOW + "Pose"),
    ) in graph
    assert (
        URIRef(GMEOW + "hasPosePosition"),
        RDFS.domain,
        URIRef(GMEOW + "Pose"),
    ) in graph
    assert (
        URIRef(GMEOW + "hasPosePosition"),
        RDFS.range,
        URIRef(GMEOW + "SpatialCoordinates"),
    ) in graph
    assert (
        URIRef(GMEOW + "hasPoseOrientation"),
        RDFS.domain,
        URIRef(GMEOW + "Pose"),
    ) in graph
    assert (
        URIRef(GMEOW + "hasPoseOrientation"),
        RDFS.range,
        URIRef(GMEOW + "Orientation"),
    ) in graph
    assert (
        URIRef(GMEOW + "poseFrame"),
        RDFS.domain,
        URIRef(GMEOW + "Pose"),
    ) in graph
    assert (
        URIRef(GMEOW + "poseFrame"),
        RDFS.range,
        URIRef(GMEOW + "ReferenceFrame"),
    ) in graph
    assert (
        URIRef(GMEOW + "poseFrame"),
        RDFS.subPropertyOf,
        URIRef(GMEOW + "hasReferenceFrame"),
    ) in graph
    for orient_prop in (
        "quaternionX",
        "quaternionY",
        "quaternionZ",
        "quaternionW",
        "yaw",
        "pitch",
        "roll",
        "heading",
        "bearing",
    ):
        prop = URIRef(GMEOW + orient_prop)
        assert (prop, RDF.type, OWL.DatatypeProperty) in graph
        assert (prop, RDFS.domain, URIRef(GMEOW + "Orientation")) in graph
        assert (prop, RDFS.range, XSD.double) in graph
    assert (
        URIRef(GMEOW + "eulerOrder"),
        RDFS.domain,
        URIRef(GMEOW + "Orientation"),
    ) in graph
    assert (
        URIRef(GMEOW + "eulerOrder"),
        RDFS.range,
        XSD.string,
    ) in graph

    # 4. Topology relations
    assert (
        URIRef(GMEOW + "containedInLocation"),
        RDF.type,
        OWL.TransitiveProperty,
    ) in graph
    assert (
        URIRef(GMEOW + "containedInPlace"),
        RDFS.subPropertyOf,
        URIRef(GMEOW + "containedInLocation"),
    ) in graph
    assert (URIRef(GMEOW + "adjacentTo"), RDF.type, OWL.SymmetricProperty) in graph
    assert (
        URIRef(GMEOW + "spatiallyConnectsTo"),
        RDF.type,
        OWL.SymmetricProperty,
    ) in graph

    # 5. locatedAt property chain axiom
    chain_head = graph.value(URIRef(GMEOW + "locatedAt"), OWL.propertyChainAxiom)
    assert chain_head is not None
    chain_elements = list(graph.items(chain_head))
    assert chain_elements == [
        URIRef(GMEOW + "locatedAt"),
        URIRef(GMEOW + "containedInLocation"),
    ]

    # 6. RCC-8 JEPD disjoint properties
    all_disjoint_nodes = list(graph.subjects(RDF.type, OWL.AllDisjointProperties))
    assert all_disjoint_nodes
    expected_members = {
        URIRef(GMEOW + "rcc8dc"),
        URIRef(GMEOW + "rcc8ec"),
        URIRef(GMEOW + "rcc8po"),
        URIRef(GMEOW + "rcc8tpp"),
        URIRef(GMEOW + "rcc8ntpp"),
        URIRef(GMEOW + "rcc8tppi"),
        URIRef(GMEOW + "rcc8ntppi"),
        URIRef(GMEOW + "rcc8eq"),
    }
    assert any(
        (members_head := graph.value(node, OWL.members)) is not None
        and set(graph.items(members_head)) == expected_members
        for node in all_disjoint_nodes
    )

    # 7. RCC-8 subproperties
    assert (
        URIRef(GMEOW + "rcc8tpp"),
        RDFS.subPropertyOf,
        URIRef(GMEOW + "containedInLocation"),
    ) in graph
    assert (
        URIRef(GMEOW + "rcc8ntpp"),
        RDFS.subPropertyOf,
        URIRef(GMEOW + "containedInLocation"),
    ) in graph


def test_metric_kind_is_value_not_subclass() -> None:
    graph = _graph()
    assert (
        URIRef(GMEOW + "MetricKind"),
        RDFS.subClassOf,
        URIRef(GUFO + "QualityValue"),
    ) in graph
    for metric in (
        "metricGeodesic",
        "metricEuclidean",
        "metricCosine",
        "metricEditDistance",
        "metricGraphHops",
    ):
        assert (
            URIRef(GMEOW + metric),
            RDF.type,
            URIRef(GMEOW + "MetricKind"),
        ) in graph


def test_has_metric_kind_on_reference_frame() -> None:
    graph = _graph()
    assert (
        URIRef(GMEOW + "hasMetricKind"),
        RDF.type,
        OWL.ObjectProperty,
    ) in graph
    assert (
        URIRef(GMEOW + "hasMetricKind"),
        RDFS.domain,
        URIRef(GMEOW + "ReferenceFrame"),
    ) in graph
    assert (
        URIRef(GMEOW + "hasMetricKind"),
        RDFS.range,
        URIRef(GMEOW + "MetricKind"),
    ) in graph
    assert (
        URIRef(GMEOW + "hasMetricKind"),
        RDF.type,
        OWL.FunctionalProperty,
    ) in graph


def test_proximity_measurement_subclass_of_measurement() -> None:
    graph = _graph()
    assert (
        URIRef(GMEOW + "ProximityMeasurement"),
        RDFS.subClassOf,
        URIRef(GMEOW + "Measurement"),
    ) in graph


def test_proximity_property_domain_range() -> None:
    graph = _graph()
    assert (
        URIRef(GMEOW + "proximity"),
        RDF.type,
        OWL.ObjectProperty,
    ) in graph
    assert (
        URIRef(GMEOW + "proximity"),
        RDFS.domain,
        URIRef(GMEOW + "Entity"),
    ) in graph
    assert (
        URIRef(GMEOW + "proximity"),
        RDFS.range,
        URIRef(GMEOW + "ProximityMeasurement"),
    ) in graph


def test_proximity_to_property() -> None:
    graph = _graph()
    assert (
        URIRef(GMEOW + "proximityTo"),
        RDF.type,
        OWL.ObjectProperty,
    ) in graph
    assert (
        URIRef(GMEOW + "proximityTo"),
        RDF.type,
        OWL.FunctionalProperty,
    ) in graph
    assert (
        URIRef(GMEOW + "proximityTo"),
        RDFS.domain,
        URIRef(GMEOW + "ProximityMeasurement"),
    ) in graph
    assert (
        URIRef(GMEOW + "proximityTo"),
        RDFS.range,
        URIRef(GMEOW + "Entity"),
    ) in graph


def test_spatial_frames_declare_metric_kind() -> None:
    graph = _graph()
    assert (
        URIRef(GMEOW + "referenceFrameWGS84"),
        URIRef(GMEOW + "hasMetricKind"),
        URIRef(GMEOW + "metricGeodesic"),
    ) in graph
    assert (
        URIRef(GMEOW + "referenceFrameLocalGrid"),
        URIRef(GMEOW + "hasMetricKind"),
        URIRef(GMEOW + "metricEuclidean"),
    ) in graph
    assert (
        URIRef(GMEOW + "referenceFrameRobotBase"),
        URIRef(GMEOW + "hasMetricKind"),
        URIRef(GMEOW + "metricEuclidean"),
    ) in graph
    assert (
        URIRef(GMEOW + "referenceFrameCelestialEquatorial"),
        URIRef(GMEOW + "hasMetricKind"),
        URIRef(GMEOW + "metricGeodesic"),
    ) in graph


# --------------------------------------------------------------------------- #
# Standpoint coexistence — contested sovereignty / place names (#51)
# --------------------------------------------------------------------------- #


def test_contested_sovereignty_coexists() -> None:
    """Two contradictory standpoint-indexed containedInPlace claims load, SHACL-pass,
    and are BOTH retained — neither is the ground truth."""
    g = Graph().parse(COVERAGE_FIXTURES / "places-contested.ttl", format="turtle")
    result = run_shacl(g)
    assert result.ok, "\n".join(result.errors)
    containers = set(
        g.objects(EX_PLACES.disputedPlace, URIRef(GMEOW + "containedInPlace"))
    )
    assert {EX_PLACES.polityA, EX_PLACES.polityB} <= containers


def test_contested_place_names_coexist() -> None:
    """Two co-equal toponyms (endonym vs exonym) are both retained."""
    g = Graph().parse(COVERAGE_FIXTURES / "places-contested.ttl", format="turtle")
    result = run_shacl(g)
    assert result.ok, "\n".join(result.errors)
    names = set(g.objects(EX_PLACES.disputedPlace, URIRef(GMEOW + "hasPlaceName")))
    assert {EX_PLACES.nameEndonym, EX_PLACES.nameExonym} <= names


def test_superseded_historical_name_suppressed() -> None:
    """A superseded place name is retained with displayable false (Principle 10)."""
    g = Graph().parse(COVERAGE_FIXTURES / "places-contested.ttl", format="turtle")
    result = run_shacl(g)
    assert result.ok, "\n".join(result.errors)
    assert (
        EX_PLACES.nameHistorical,
        URIRef(GMEOW + "displayable"),
        Literal(False),
    ) in g


def test_no_preferred_or_primary_place_term() -> None:
    """Principle 9: no single slot to win — places mints no preferred/primary
    selector for a contested jurisdiction or toponym."""
    g = _graph()
    prop_types = (OWL.ObjectProperty, OWL.DatatypeProperty, OWL.AnnotationProperty)
    for banned in (
        "primaryName",
        "preferredName",
        "primaryJurisdiction",
        "preferredJurisdiction",
        "preferredRank",
    ):
        node = URIRef(GMEOW + banned)
        for pt in prop_types:
            assert (node, RDF.type, pt) not in g, f"{banned} must not exist"
        assert (node, RDF.type, OWL.Class) not in g
