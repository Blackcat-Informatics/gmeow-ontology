"""Structural + DL-safety guards for the locations building block.

Pins the value-vs-subclass decisions (Location kinds are subclasses; place kinds
and storage media are value vocabularies), the DL-safe WKT range, the
non-functional evidence-centric datatype properties, and the open-range
authority link.
"""

from __future__ import annotations

from decimal import Decimal
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
        "placeTypeSite",
    ):
        assert (URIRef(GMEOW + ind), RDF.type, URIRef(GMEOW + "PlaceType")) in graph
    # The rejected per-kind subclasses must NOT exist as classes.
    for rejected in ("Country", "City", "Building", "Room", "Site"):
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
        "frameRealmMathematical",
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
        "frameKindHilbert",
        "frameKindManifold",
        "frameKindPhaseSpace",
        "frameKindLatentSpace",
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
        "axisGeneralizedCoordinate",
        "axisGeneralizedMomentum",
        "axisMomentumX",
        "axisMomentumY",
        "axisMomentumZ",
        "axisHilbertState",
        "axisLatentVector",
        "axisJointAngle1",
        "axisJointAngle2",
        "axisJointAngle3",
        "axisJointAngle4",
        "axisJointAngle5",
        "axisJointAngle6",
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
        "referenceFramePhaseSpace3DOF",
        "referenceFrameHilbertSpace",
        "referenceFrameLatentVectorSpace",
        "referenceFrameRobotArm6DOF",
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
    # hasCoordinateMatrix domain is an owl:unionOf(Axis, Pose, SpatialCoordinates).
    hcm_domain = graph.value(URIRef(GMEOW + "hasCoordinateMatrix"), RDFS.domain)
    assert hcm_domain is not None
    assert (hcm_domain, RDF.type, OWL.Class) in graph
    union_of = graph.value(hcm_domain, OWL.unionOf)
    assert union_of is not None
    union_members = set(graph.items(union_of))
    assert union_members == {
        URIRef(GMEOW + "Axis"),
        URIRef(GMEOW + "Pose"),
        URIRef(GMEOW + "SpatialCoordinates"),
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
        "metricSymplectic",
        "metricPositionalDistance",
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


def test_has_centroid_domain_range() -> None:
    graph = _graph()
    prop = URIRef(GMEOW + "hasCentroid")
    assert (prop, RDF.type, OWL.ObjectProperty) in graph
    assert (prop, RDFS.domain, URIRef(GMEOW + "Place")) in graph
    assert (prop, RDFS.range, URIRef(GMEOW + "Geometry")) in graph


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
        "primaryOverlay",
        "preferredOverlay",
        "primaryRegulatoryOverlay",
        "preferredRegulatoryOverlay",
    ):
        node = URIRef(GMEOW + banned)
        for pt in prop_types:
            assert (node, RDF.type, pt) not in g, f"{banned} must not exist"
        assert (node, RDF.type, OWL.Class) not in g


# --------------------------------------------------------------------------- #
# Terrestrial realm deepening — JurisdictionTenure, ContainmentTenure,
# GeometryType, asGeoJSON, determinacy & lifecycle wiring (#82)
# --------------------------------------------------------------------------- #


def test_jurisdiction_tenure_grounding() -> None:
    graph = _graph()
    assert (
        URIRef(GMEOW + "JurisdictionTenure"),
        RDFS.subClassOf,
        URIRef(GMEOW + "TimeScopedRelation"),
    ) in graph
    assert (
        URIRef(GMEOW + "jurisdictionPlace"),
        RDFS.domain,
        URIRef(GMEOW + "JurisdictionTenure"),
    ) in graph
    assert (
        URIRef(GMEOW + "jurisdictionPlace"),
        RDFS.range,
        URIRef(GMEOW + "Place"),
    ) in graph
    assert (
        URIRef(GMEOW + "jurisdictionPlace"),
        RDF.type,
        OWL.FunctionalProperty,
    ) in graph
    assert (
        URIRef(GMEOW + "jurisdictionPolity"),
        RDFS.domain,
        URIRef(GMEOW + "JurisdictionTenure"),
    ) in graph
    assert (
        URIRef(GMEOW + "jurisdictionPolity"),
        RDFS.range,
        URIRef(GMEOW + "Agent"),
    ) in graph
    assert (
        URIRef(GMEOW + "jurisdictionPolity"),
        RDF.type,
        OWL.FunctionalProperty,
    ) in graph


def test_containment_tenure_grounding() -> None:
    graph = _graph()
    assert (
        URIRef(GMEOW + "ContainmentTenure"),
        RDFS.subClassOf,
        URIRef(GMEOW + "TimeScopedRelation"),
    ) in graph
    assert (
        URIRef(GMEOW + "containmentChild"),
        RDFS.domain,
        URIRef(GMEOW + "ContainmentTenure"),
    ) in graph
    assert (
        URIRef(GMEOW + "containmentChild"),
        RDFS.range,
        URIRef(GMEOW + "Place"),
    ) in graph
    assert (
        URIRef(GMEOW + "containmentChild"),
        RDF.type,
        OWL.FunctionalProperty,
    ) in graph
    assert (
        URIRef(GMEOW + "containmentParent"),
        RDFS.domain,
        URIRef(GMEOW + "ContainmentTenure"),
    ) in graph
    assert (
        URIRef(GMEOW + "containmentParent"),
        RDFS.range,
        URIRef(GMEOW + "Place"),
    ) in graph
    assert (
        URIRef(GMEOW + "containmentParent"),
        RDF.type,
        OWL.FunctionalProperty,
    ) in graph


def test_geometry_type_is_value_not_subclass() -> None:
    graph = _graph()
    assert (
        URIRef(GMEOW + "GeometryType"),
        RDFS.subClassOf,
        URIRef(GUFO + "QualityValue"),
    ) in graph
    for gt in (
        "geometryTypePoint",
        "geometryTypeLineString",
        "geometryTypePolygon",
        "geometryTypeMultiPoint",
        "geometryTypeMultiLineString",
        "geometryTypeMultiPolygon",
    ):
        assert (
            URIRef(GMEOW + gt),
            RDF.type,
            URIRef(GMEOW + "GeometryType"),
        ) in graph
    for rejected in ("Point", "LineString", "Polygon", "MultiPoint"):
        assert (URIRef(GMEOW + rejected), RDF.type, OWL.Class) not in graph


def test_asgeojson_range_is_dl_safe() -> None:
    graph = _graph()
    as_gj = URIRef(GMEOW + "asGeoJSON")
    assert (as_gj, RDF.type, OWL.DatatypeProperty) in graph
    assert (as_gj, RDFS.range, RDFS.Literal) in graph
    assert (as_gj, RDFS.range, URIRef(GEO + "geoJSONLiteral")) not in graph


def test_place_determinacy_subproperty() -> None:
    graph = _graph()
    assert (
        URIRef(GMEOW + "placeDeterminacy"),
        RDFS.subPropertyOf,
        URIRef(GMEOW + "hasDeterminacy"),
    ) in graph
    assert (
        URIRef(GMEOW + "placeDeterminacy"),
        RDFS.domain,
        URIRef(GMEOW + "Place"),
    ) in graph
    assert (
        URIRef(GMEOW + "geometryDeterminacy"),
        RDFS.subPropertyOf,
        URIRef(GMEOW + "hasDeterminacy"),
    ) in graph
    assert (
        URIRef(GMEOW + "geometryDeterminacy"),
        RDFS.domain,
        URIRef(GMEOW + "Geometry"),
    ) in graph


def test_place_supersession_subproperties() -> None:
    graph = _graph()
    assert (
        URIRef(GMEOW + "placeSupersededBy"),
        RDFS.subPropertyOf,
        URIRef(GMEOW + "supersededBy"),
    ) in graph
    assert (
        URIRef(GMEOW + "placeSupersedes"),
        RDFS.subPropertyOf,
        URIRef(GMEOW + "supersedes"),
    ) in graph


def test_contested_jurisdiction_tenures_coexist() -> None:
    """Two contradictory JurisdictionTenures on the same place load, SHACL-pass,
    and are BOTH retained — neither is the ground truth (Principle 9)."""
    g = Graph().parse(COVERAGE_FIXTURES / "places-contested.ttl", format="turtle")
    result = run_shacl(g)
    assert result.ok, "\n".join(result.errors)
    tenures = set(g.subjects(RDF.type, URIRef(GMEOW + "JurisdictionTenure")))
    assert len(tenures) >= 2, "Expected at least two co-existing JurisdictionTenures"
    polities = set()
    for tenure in tenures:
        polity = g.value(tenure, URIRef(GMEOW + "jurisdictionPolity"))
        if polity:
            polities.add(polity)
    assert len(polities) >= 2, "Expected at least two distinct polity claims"


def test_containment_tenure_records_border_change() -> None:
    """A ContainmentTenure records a place's parent change over time."""
    g = Graph().parse(COVERAGE_FIXTURES / "places-contested.ttl", format="turtle")
    result = run_shacl(g)
    assert result.ok, "\n".join(result.errors)
    tenures = list(g.subjects(RDF.type, URIRef(GMEOW + "ContainmentTenure")))
    assert len(tenures) >= 2, "Expected at least two ContainmentTenure records"

    parent_pred = URIRef(GMEOW + "containmentParent")
    child_pred = URIRef(GMEOW + "containmentChild")
    interval_pred = URIRef(GMEOW + "duringInterval")

    claims = {
        (
            g.value(t, parent_pred),
            g.value(t, interval_pred),
        )
        for t in tenures
        if (t, child_pred, EX_PLACES.disputedPlace) in g
    }

    assert (EX_PLACES.polityA, EX_PLACES.interval1920_1954) in claims
    assert (EX_PLACES.polityB, EX_PLACES.interval1954_present) in claims


# --------------------------------------------------------------------------- #
# RegulatoryOverlay — legal / regulatory overlays beyond sovereignty (#103)
# --------------------------------------------------------------------------- #


def test_regulatory_overlay_grounding() -> None:
    graph = _graph()
    assert (
        URIRef(GMEOW + "RegulatoryOverlay"),
        RDFS.subClassOf,
        URIRef(GMEOW + "TimeScopedRelation"),
    ) in graph
    assert (
        URIRef(GMEOW + "overlayPlace"),
        RDFS.domain,
        URIRef(GMEOW + "RegulatoryOverlay"),
    ) in graph
    assert (
        URIRef(GMEOW + "overlayPlace"),
        RDFS.range,
        URIRef(GMEOW + "Place"),
    ) in graph
    assert (
        URIRef(GMEOW + "overlayPlace"),
        RDF.type,
        OWL.FunctionalProperty,
    ) in graph
    assert (
        URIRef(GMEOW + "overlayAuthority"),
        RDFS.domain,
        URIRef(GMEOW + "RegulatoryOverlay"),
    ) in graph
    assert (
        URIRef(GMEOW + "overlayAuthority"),
        RDFS.range,
        URIRef(GMEOW + "Agent"),
    ) in graph
    assert (
        URIRef(GMEOW + "overlayAuthority"),
        RDF.type,
        OWL.FunctionalProperty,
    ) in graph
    assert (
        URIRef(GMEOW + "overlayType"),
        RDFS.domain,
        URIRef(GMEOW + "RegulatoryOverlay"),
    ) in graph
    assert (
        URIRef(GMEOW + "overlayRegulation"),
        RDFS.domain,
        URIRef(GMEOW + "RegulatoryOverlay"),
    ) in graph
    assert (
        URIRef(GMEOW + "overlayRegulation"),
        RDFS.range,
        URIRef(GMEOW + "RightsStatement"),
    ) in graph
    assert (
        URIRef(GMEOW + "RegulatoryOverlayType"),
        RDF.type,
        OWL.Class,
    ) in graph
    assert (
        URIRef(GMEOW + "overlayType"),
        RDFS.range,
        URIRef(GMEOW + "RegulatoryOverlayType"),
    ) in graph
    assert (
        URIRef(GMEOW + "overlayDeterminacy"),
        RDFS.subPropertyOf,
        URIRef(GMEOW + "hasDeterminacy"),
    ) in graph
    assert (
        URIRef(GMEOW + "overlayDeterminacy"),
        RDFS.domain,
        URIRef(GMEOW + "RegulatoryOverlay"),
    ) in graph
    assert (
        URIRef(GMEOW + "overlayDeterminacy"),
        RDFS.range,
        URIRef(GMEOW + "Determinacy"),
    ) in graph
    assert (
        URIRef(GMEOW + "overlayLowerBound"),
        RDFS.domain,
        URIRef(GMEOW + "RegulatoryOverlay"),
    ) in graph
    assert (
        URIRef(GMEOW + "overlayLowerBound"),
        RDFS.range,
        URIRef(GMEOW + "ScalarQuantity"),
    ) in graph
    assert (
        URIRef(GMEOW + "overlayUpperBound"),
        RDFS.domain,
        URIRef(GMEOW + "RegulatoryOverlay"),
    ) in graph
    assert (
        URIRef(GMEOW + "overlayUpperBound"),
        RDFS.range,
        URIRef(GMEOW + "ScalarQuantity"),
    ) in graph


def test_contested_regulatory_overlays_coexist() -> None:
    """Two contradictory RegulatoryOverlays on the same place load, SHACL-pass,
    and are BOTH retained — neither is the ground truth (Principle 9)."""
    g = Graph().parse(COVERAGE_FIXTURES / "places-regulatory.ttl", format="turtle")
    result = run_shacl(g)
    assert result.ok, "\n".join(result.errors)
    overlays = set(g.subjects(RDF.type, URIRef(GMEOW + "RegulatoryOverlay")))
    assert len(overlays) >= 2, "Expected at least two co-existing RegulatoryOverlays"
    authorities = set()
    for overlay in overlays:
        auth = g.value(overlay, URIRef(GMEOW + "overlayAuthority"))
        if auth:
            authorities.add(auth)
    assert len(authorities) >= 2, "Expected at least two distinct authority claims"


def test_regulatory_overlay_linked_to_rights_statement() -> None:
    """A RegulatoryOverlay may link to a RightsStatement for the deontic rules
    that govern activity within the overlay."""
    g = Graph().parse(COVERAGE_FIXTURES / "places-regulatory.ttl", format="turtle")
    result = run_shacl(g)
    assert result.ok, "\n".join(result.errors)
    overlays = list(g.subjects(RDF.type, URIRef(GMEOW + "RegulatoryOverlay")))
    assert overlays, "Expected at least one RegulatoryOverlay"
    regs = set()
    for overlay in overlays:
        reg = g.value(overlay, URIRef(GMEOW + "overlayRegulation"))
        if reg:
            regs.add(reg)
    assert regs, "Expected at least one overlay linked to a RightsStatement"
    for reg in regs:
        assert (reg, RDF.type, URIRef(GMEOW + "RightsStatement")) in g


def test_regulatory_overlay_3d_bounds() -> None:
    """A restricted-airspace overlay carries altitude bounds as ScalarQuantity
    with QUDT units and a reference frame (Principle 11)."""
    g = Graph().parse(COVERAGE_FIXTURES / "places-regulatory.ttl", format="turtle")
    result = run_shacl(g)
    assert result.ok, "\n".join(result.errors)
    overlays = list(g.subjects(RDF.type, URIRef(GMEOW + "RegulatoryOverlay")))
    found_bounds = False
    for overlay in overlays:
        lower = g.value(overlay, URIRef(GMEOW + "overlayLowerBound"))
        upper = g.value(overlay, URIRef(GMEOW + "overlayUpperBound"))
        if lower and upper:
            found_bounds = True
            for bound in (lower, upper):
                sq = g.value(bound, URIRef(GMEOW + "quantityValue"))
                assert sq is not None, "3D bound ScalarQuantity must have quantityValue"
                unit = g.value(bound, URIRef(GMEOW + "hasUnit"))
                assert unit is not None, "3D bound ScalarQuantity must have hasUnit"
                frame = g.value(bound, URIRef(GMEOW + "hasReferenceFrame"))
                assert frame is not None, (
                    "3D bound ScalarQuantity must have hasReferenceFrame"
                )
    assert found_bounds, (
        "Expected at least one overlay with both lower and upper bounds"
    )


def test_geometry_has_type_and_geojson() -> None:
    """A geometry may carry both a GeometryType value and a GeoJSON serialization."""
    g = Graph().parse(COVERAGE_FIXTURES / "places-contested.ttl", format="turtle")
    result = run_shacl(g)
    assert result.ok, "\n".join(result.errors)
    geoms = set(g.objects(EX_PLACES.disputedPlace, URIRef(GMEOW + "hasGeometry")))
    assert geoms, "Expected at least one geometry on disputedPlace"
    for geom in geoms:
        gt = g.value(geom, URIRef(GMEOW + "geometryType"))
        assert gt is not None, "Geometry must have a geometryType"
        gj = g.value(geom, URIRef(GMEOW + "asGeoJSON"))
        assert gj is not None, "Geometry must have an asGeoJSON serialization"


# --------------------------------------------------------------------------- #
# Motion — LocationState / Trajectory (#94)
# --------------------------------------------------------------------------- #


def test_location_state_is_entity() -> None:
    graph = _graph()
    assert (
        URIRef(GMEOW + "LocationState"),
        RDFS.subClassOf,
        URIRef(GMEOW + "Entity"),
    ) in graph


def test_trajectory_is_entity() -> None:
    graph = _graph()
    assert (
        URIRef(GMEOW + "Trajectory"),
        RDFS.subClassOf,
        URIRef(GMEOW + "Entity"),
    ) in graph


def test_location_state_properties_exist() -> None:
    graph = _graph()
    assert (
        URIRef(GMEOW + "stateOf"),
        RDFS.domain,
        URIRef(GMEOW + "LocationState"),
    ) in graph
    assert (
        URIRef(GMEOW + "stateDuringInterval"),
        RDFS.domain,
        URIRef(GMEOW + "LocationState"),
    ) in graph
    assert (
        URIRef(GMEOW + "stateAtInstant"),
        RDFS.domain,
        URIRef(GMEOW + "LocationState"),
    ) in graph
    assert (
        URIRef(GMEOW + "stateHasVelocity"),
        RDFS.domain,
        URIRef(GMEOW + "LocationState"),
    ) in graph
    assert (
        URIRef(GMEOW + "stateHasAngularVelocity"),
        RDFS.domain,
        URIRef(GMEOW + "LocationState"),
    ) in graph
    assert (
        URIRef(GMEOW + "stateReferenceFrame"),
        RDFS.domain,
        URIRef(GMEOW + "LocationState"),
    ) in graph


def test_trajectory_properties_exist() -> None:
    graph = _graph()
    assert (
        URIRef(GMEOW + "trajectoryOf"),
        RDFS.domain,
        URIRef(GMEOW + "Trajectory"),
    ) in graph
    assert (
        URIRef(GMEOW + "hasTrajectorySample"),
        RDFS.domain,
        URIRef(GMEOW + "Trajectory"),
    ) in graph
    assert (
        URIRef(GMEOW + "trajectoryReferenceFrame"),
        RDFS.domain,
        URIRef(GMEOW + "Trajectory"),
    ) in graph


def test_velocity_range_is_scalar_quantity() -> None:
    graph = _graph()
    assert (
        URIRef(GMEOW + "stateHasVelocity"),
        RDFS.range,
        URIRef(GMEOW + "ScalarQuantity"),
    ) in graph
    assert (
        URIRef(GMEOW + "stateHasAngularVelocity"),
        RDFS.range,
        URIRef(GMEOW + "ScalarQuantity"),
    ) in graph


def test_trajectory_sample_is_non_functional() -> None:
    graph = _graph()
    prop = URIRef(GMEOW + "hasTrajectorySample")
    assert (prop, RDF.type, OWL.ObjectProperty) in graph
    assert (prop, RDF.type, OWL.FunctionalProperty) not in graph


def test_state_reference_frame_is_functional() -> None:
    graph = _graph()
    prop = URIRef(GMEOW + "stateReferenceFrame")
    assert (prop, RDF.type, OWL.FunctionalProperty) in graph


def test_trajectory_reference_frame_is_functional() -> None:
    graph = _graph()
    prop = URIRef(GMEOW + "trajectoryReferenceFrame")
    assert (prop, RDF.type, OWL.FunctionalProperty) in graph


def test_state_of_is_functional() -> None:
    graph = _graph()
    prop = URIRef(GMEOW + "stateOf")
    assert (prop, RDF.type, OWL.FunctionalProperty) in graph


def test_trajectory_of_is_functional() -> None:
    graph = _graph()
    prop = URIRef(GMEOW + "trajectoryOf")
    assert (prop, RDF.type, OWL.FunctionalProperty) in graph


def test_no_unsafe_motion_property_chains() -> None:
    """Principle 12: interpolation and coordinate transforms stay in solver."""
    graph = _graph()
    for prop in (
        "stateOf",
        "stateDuringInterval",
        "stateAtInstant",
        "stateHasVelocity",
        "stateHasAngularVelocity",
        "stateReferenceFrame",
        "trajectoryOf",
        "hasTrajectorySample",
        "trajectoryReferenceFrame",
    ):
        p = URIRef(GMEOW + prop)
        for _, _, _o in graph.triples((p, OWL.propertyChainAxiom, None)):
            raise AssertionError(f"{prop} must not carry a property chain axiom")


# --------------------------------------------------------------------------- #
# Streaming — LocationState / Trajectory / Stream (#96)
# --------------------------------------------------------------------------- #


def test_location_stream_to_trajectory_derivation() -> None:
    """A Trajectory derived from a Stream of LocationStates loads and passes SHACL."""
    from rdflib import Namespace

    ex_str = Namespace("https://blackcatinformatics.ca/gmeow/examples/places/")
    g = Graph().parse(COVERAGE_FIXTURES / "places-streaming.ttl", format="turtle")
    result = run_shacl(g)
    assert result.ok, "\n".join(result.errors)
    # The trajectory is derived from the stream via wasDerivedFrom.
    assert (
        ex_str.trajectory1,
        URIRef(GMEOW + "wasDerivedFrom"),
        ex_str.stream1,
    ) in g
    # The stream has three location-state samples.
    samples = set(g.objects(ex_str.stream1, URIRef(GMEOW + "streamSample")))
    assert len(samples) == 3, f"Expected 3 stream samples, got {len(samples)}"
    # The trajectory has the same samples.
    traj_samples = set(
        g.objects(ex_str.trajectory1, URIRef(GMEOW + "hasTrajectorySample"))
    )
    assert samples == traj_samples, "Stream samples and trajectory samples must match"


def test_stream_and_trajectory_coexist() -> None:
    """Multiple streams/trajectories on the same entity coexist (Principle 9)."""
    g = Graph().parse(COVERAGE_FIXTURES / "places-streaming.ttl", format="turtle")
    result = run_shacl(g)
    assert result.ok, "\n".join(result.errors)
    streams = set(g.subjects(RDF.type, URIRef(GMEOW + "Stream")))
    assert len(streams) >= 2, "Expected at least two co-existing streams"
    trajectories = set(g.subjects(RDF.type, URIRef(GMEOW + "Trajectory")))
    assert len(trajectories) >= 2, "Expected at least two co-existing trajectories"


def test_no_preferred_stream_term() -> None:
    """Principle 9: no single slot to win — streaming mints no preferred/primary
    selector for a contested or competing stream."""
    g = _graph()
    prop_types = (OWL.ObjectProperty, OWL.DatatypeProperty, OWL.AnnotationProperty)
    for banned in (
        "primaryStream",
        "preferredStream",
        "primaryLocationStream",
        "preferredLocationStream",
    ):
        node = URIRef(GMEOW + banned)
        for pt in prop_types:
            assert (node, RDF.type, pt) not in g, f"{banned} must not exist"
        assert (node, RDF.type, OWL.Class) not in g


# --------------------------------------------------------------------------- #
# Capacity / Occupancy / Utilization — issue #100
# --------------------------------------------------------------------------- #


def test_capacity_occupancy_utilization_subclass_of_measurement() -> None:
    graph = _graph()
    for cls in ("Capacity", "Occupancy", "Utilization"):
        ref = URIRef(GMEOW + cls)
        assert (ref, RDF.type, OWL.Class) in graph
        assert (ref, RDFS.subClassOf, URIRef(GMEOW + "Measurement")) in graph


def test_capacity_of_property() -> None:
    graph = _graph()
    assert (
        URIRef(GMEOW + "capacityOf"),
        RDF.type,
        OWL.FunctionalProperty,
    ) in graph
    assert (
        URIRef(GMEOW + "capacityOf"),
        RDFS.domain,
        URIRef(GMEOW + "Capacity"),
    ) in graph
    assert (
        URIRef(GMEOW + "capacityOf"),
        RDFS.range,
        URIRef(GMEOW + "Location"),
    ) in graph
    assert (
        URIRef(GMEOW + "capacityOf"),
        RDFS.subPropertyOf,
        URIRef(GMEOW + "observedFeature"),
    ) in graph


def test_occupancy_of_property() -> None:
    graph = _graph()
    assert (
        URIRef(GMEOW + "occupancyOf"),
        RDF.type,
        OWL.FunctionalProperty,
    ) in graph
    assert (
        URIRef(GMEOW + "occupancyOf"),
        RDFS.domain,
        URIRef(GMEOW + "Occupancy"),
    ) in graph
    assert (
        URIRef(GMEOW + "occupancyOf"),
        RDFS.range,
        URIRef(GMEOW + "Location"),
    ) in graph
    assert (
        URIRef(GMEOW + "occupancyOf"),
        RDFS.subPropertyOf,
        URIRef(GMEOW + "observedFeature"),
    ) in graph


def test_utilization_of_property() -> None:
    graph = _graph()
    assert (
        URIRef(GMEOW + "utilizationOf"),
        RDF.type,
        OWL.FunctionalProperty,
    ) in graph
    assert (
        URIRef(GMEOW + "utilizationOf"),
        RDFS.domain,
        URIRef(GMEOW + "Utilization"),
    ) in graph
    assert (
        URIRef(GMEOW + "utilizationOf"),
        RDFS.range,
        URIRef(GMEOW + "Location"),
    ) in graph
    assert (
        URIRef(GMEOW + "utilizationOf"),
        RDFS.subPropertyOf,
        URIRef(GMEOW + "observedFeature"),
    ) in graph


def test_has_capacity_domain_range() -> None:
    graph = _graph()
    assert (
        URIRef(GMEOW + "hasCapacity"),
        RDFS.domain,
        URIRef(GMEOW + "Location"),
    ) in graph
    assert (
        URIRef(GMEOW + "hasCapacity"),
        RDFS.range,
        URIRef(GMEOW + "Capacity"),
    ) in graph


def test_has_occupancy_domain_range() -> None:
    graph = _graph()
    assert (
        URIRef(GMEOW + "hasOccupancy"),
        RDFS.domain,
        URIRef(GMEOW + "Location"),
    ) in graph
    assert (
        URIRef(GMEOW + "hasOccupancy"),
        RDFS.range,
        URIRef(GMEOW + "Occupancy"),
    ) in graph


def test_has_utilization_domain_range() -> None:
    graph = _graph()
    assert (
        URIRef(GMEOW + "hasUtilization"),
        RDFS.domain,
        URIRef(GMEOW + "Location"),
    ) in graph
    assert (
        URIRef(GMEOW + "hasUtilization"),
        RDFS.range,
        URIRef(GMEOW + "Utilization"),
    ) in graph


def test_no_preferred_or_primary_capacity_term() -> None:
    """Principle 9: no single slot to win — capacity/occupancy mints no
    preferred/primary selector for a contested measurement."""
    g = _graph()
    prop_types = (OWL.ObjectProperty, OWL.DatatypeProperty, OWL.AnnotationProperty)
    for banned in (
        "primaryCapacity",
        "preferredCapacity",
        "primaryOccupancy",
        "preferredOccupancy",
        "primaryUtilization",
        "preferredUtilization",
    ):
        node = URIRef(GMEOW + banned)
        for pt in prop_types:
            assert (node, RDF.type, pt) not in g, f"{banned} must not exist"
        assert (node, RDF.type, OWL.Class) not in g


def test_contested_capacity_claims_coexist() -> None:
    """Two contradictory Capacity measurements on the same location load,
    SHACL-pass, and are BOTH retained — neither is the ground truth (P9)."""
    from rdflib import Namespace

    ex_cap = Namespace("https://blackcatinformatics.ca/gmeow/examples/places/")
    g = Graph().parse(COVERAGE_FIXTURES / "places-capacity.ttl", format="turtle")
    result = run_shacl(g)
    assert result.ok, "\n".join(result.errors)
    caps = set(g.subjects(URIRef(GMEOW + "capacityOf"), ex_cap.venue))
    assert {ex_cap.capFireCode, ex_cap.capVenueClaim} <= caps


def test_superseded_capacity_suppressed() -> None:
    """A superseded capacity is retained with displayable false (P10)."""
    from rdflib import Namespace

    ex_cap = Namespace("https://blackcatinformatics.ca/gmeow/examples/places/")
    g = Graph().parse(COVERAGE_FIXTURES / "places-capacity.ttl", format="turtle")
    result = run_shacl(g)
    assert result.ok, "\n".join(result.errors)
    assert (
        ex_cap.capOld,
        URIRef(GMEOW + "displayable"),
        Literal(False),
    ) in g


def test_occupancy_with_unit_asserted() -> None:
    """An Occupancy measurement carries a scalar quantity with a QUDT unit."""
    from rdflib import Namespace

    ex_cap = Namespace("https://blackcatinformatics.ca/gmeow/examples/places/")
    g = Graph().parse(COVERAGE_FIXTURES / "places-capacity.ttl", format="turtle")
    result = run_shacl(g)
    assert result.ok, "\n".join(result.errors)
    occs = list(g.subjects(URIRef(GMEOW + "occupancyOf"), ex_cap.venue))
    assert len(occs) == 1, "Venue must have exactly one occupancy"
    occ = occs[0]
    sq = g.value(occ, URIRef(GMEOW + "observationResult"))
    assert sq is not None, "Occupancy must have an observationResult"
    val = g.value(sq, URIRef(GMEOW + "quantityValue"))
    assert val is not None, "ScalarQuantity must have a quantityValue"
    assert Decimal(str(val)) == Decimal("412")


def test_storage_capacity_in_bytes() -> None:
    """A StorageLocation can have a capacity in bytes (QUDT BYTE unit)."""
    from rdflib import Namespace

    ex_cap = Namespace("https://blackcatinformatics.ca/gmeow/examples/places/")
    g = Graph().parse(COVERAGE_FIXTURES / "places-capacity.ttl", format="turtle")
    result = run_shacl(g)
    assert result.ok, "\n".join(result.errors)
    caps = list(g.subjects(URIRef(GMEOW + "capacityOf"), ex_cap.storage))
    assert len(caps) == 1, "Storage must have exactly one capacity"
    cap = caps[0]
    sq = g.value(cap, URIRef(GMEOW + "observationResult"))
    assert sq is not None, "Capacity must have an observationResult"
    val = g.value(sq, URIRef(GMEOW + "quantityValue"))
    assert val is not None, "ScalarQuantity must have a quantityValue"
    assert Decimal(str(val)) == Decimal("1099511627776")
    unit = g.value(sq, URIRef(GMEOW + "hasUnit"))
    assert unit == URIRef("http://qudt.org/vocab/unit/BYTE")


# --------------------------------------------------------------------------- #
# Virtual Location Type + Network Address Space — issue #84
# --------------------------------------------------------------------------- #


def test_virtual_location_type_is_value_not_subclass() -> None:
    graph = _graph()
    assert (
        URIRef(GMEOW + "VirtualLocationType"),
        RDFS.subClassOf,
        URIRef(GUFO + "QualityValue"),
    ) in graph
    vlt = URIRef(GMEOW + "virtualLocationType")
    assert (vlt, RDF.type, OWL.ObjectProperty) in graph
    assert (vlt, RDF.type, OWL.FunctionalProperty) not in graph
    for ind in (
        "virtualLocationTypeVideoConference",
        "virtualLocationTypeChatSpace",
        "virtualLocationTypeWebsite",
        "virtualLocationTypeSocialMediaPage",
    ):
        assert (
            URIRef(GMEOW + ind),
            RDF.type,
            URIRef(GMEOW + "VirtualLocationType"),
        ) in graph
    for rejected in ("VideoConference", "ChatSpace", "Website", "SocialMediaPage"):
        assert (URIRef(GMEOW + rejected), RDF.type, OWL.Class) not in graph


def test_network_address_grounding() -> None:
    graph = _graph()
    assert (
        URIRef(GMEOW + "NetworkAddress"),
        RDF.type,
        OWL.Class,
    ) in graph
    assert (
        URIRef(GMEOW + "hasNetworkAddress"),
        RDFS.domain,
        URIRef(GMEOW + "VirtualLocation"),
    ) in graph
    assert (
        URIRef(GMEOW + "hasNetworkAddress"),
        RDFS.range,
        URIRef(GMEOW + "NetworkAddress"),
    ) in graph
    assert (
        URIRef(GMEOW + "networkAddressType"),
        RDFS.domain,
        URIRef(GMEOW + "NetworkAddress"),
    ) in graph
    assert (
        URIRef(GMEOW + "networkAddressType"),
        RDFS.range,
        URIRef(GMEOW + "NetworkAddressType"),
    ) in graph
    assert (
        URIRef(GMEOW + "networkAddressValue"),
        RDFS.domain,
        URIRef(GMEOW + "NetworkAddress"),
    ) in graph
    assert (
        URIRef(GMEOW + "networkAddressFrame"),
        RDFS.domain,
        URIRef(GMEOW + "NetworkAddress"),
    ) in graph
    assert (
        URIRef(GMEOW + "networkAddressFrame"),
        RDFS.range,
        URIRef(GMEOW + "ReferenceFrame"),
    ) in graph
    assert (
        URIRef(GMEOW + "networkAddressFrame"),
        RDF.type,
        OWL.FunctionalProperty,
    ) in graph
    assert (
        URIRef(GMEOW + "networkAddressFrame"),
        RDFS.subPropertyOf,
        URIRef(GMEOW + "hasReferenceFrame"),
    ) in graph


def test_network_address_type_is_value_not_subclass() -> None:
    graph = _graph()
    assert (
        URIRef(GMEOW + "NetworkAddressType"),
        RDFS.subClassOf,
        URIRef(GUFO + "QualityValue"),
    ) in graph
    for ind in (
        "networkAddressTypeIPv4",
        "networkAddressTypeIPv6",
        "networkAddressTypeMAC",
        "networkAddressTypeDNS",
        "networkAddressTypeURL",
        "networkAddressTypePort",
        "networkAddressTypeBGP",
    ):
        assert (
            URIRef(GMEOW + ind),
            RDF.type,
            URIRef(GMEOW + "NetworkAddressType"),
        ) in graph
    for rejected in ("IPv4Address", "IPv6Address", "MACAddress", "DNSName"):
        assert (URIRef(GMEOW + rejected), RDF.type, OWL.Class) not in graph


def test_frame_kind_topological_exists() -> None:
    graph = _graph()
    assert (
        URIRef(GMEOW + "frameKindTopological"),
        RDF.type,
        URIRef(GMEOW + "FrameKind"),
    ) in graph


def test_network_reference_frames_have_parent_frame() -> None:
    graph = _graph()
    for frame in (
        "referenceFrameIPv4",
        "referenceFrameIPv6",
        "referenceFrameMAC",
        "referenceFrameDNS",
        "referenceFrameURL",
        "referenceFramePort",
        "referenceFrameBGP",
    ):
        assert (
            URIRef(GMEOW + frame),
            URIRef(GMEOW + "parentFrame"),
            URIRef(GMEOW + "referenceFrameInternet"),
        ) in graph


def test_internet_root_frame_is_topological() -> None:
    graph = _graph()
    assert (
        URIRef(GMEOW + "referenceFrameInternet"),
        URIRef(GMEOW + "frameKind"),
        URIRef(GMEOW + "frameKindTopological"),
    ) in graph
    assert (
        URIRef(GMEOW + "referenceFrameInternet"),
        URIRef(GMEOW + "hasMetricKind"),
        URIRef(GMEOW + "metricGraphHops"),
    ) in graph


def test_virtual_location_types_coexist() -> None:
    """A VirtualLocation may have multiple virtualLocationType values (P9)."""
    from rdflib import Namespace

    ex_vl = Namespace("https://blackcatinformatics.ca/gmeow/examples/places/")
    g = Graph().parse(COVERAGE_FIXTURES / "places-virtual.ttl", format="turtle")
    result = run_shacl(g)
    assert result.ok, "\n".join(result.errors)
    types = set(g.objects(ex_vl.confRoom, URIRef(GMEOW + "virtualLocationType")))
    assert {
        URIRef(GMEOW + "virtualLocationTypeVideoConference"),
        URIRef(GMEOW + "virtualLocationTypeVirtualEventSpace"),
    } <= types


def test_network_addresses_in_different_frames_coexist() -> None:
    """A VirtualLocation may have NetworkAddresses in different frames (P9)."""
    from rdflib import Namespace

    ex_vl = Namespace("https://blackcatinformatics.ca/gmeow/examples/places/")
    g = Graph().parse(COVERAGE_FIXTURES / "places-virtual.ttl", format="turtle")
    result = run_shacl(g)
    assert result.ok, "\n".join(result.errors)
    addrs = set(g.objects(ex_vl.website, URIRef(GMEOW + "hasNetworkAddress")))
    assert len(addrs) >= 3, "Expected at least 3 network addresses"
    frames = {g.value(a, URIRef(GMEOW + "networkAddressFrame")) for a in addrs}
    assert {
        URIRef(GMEOW + "referenceFrameIPv4"),
        URIRef(GMEOW + "referenceFrameDNS"),
        URIRef(GMEOW + "referenceFrameURL"),
    } <= frames


def test_superseded_network_address_suppressed() -> None:
    """A superseded network address is retained with displayable false (P10)."""
    from rdflib import Namespace

    ex_vl = Namespace("https://blackcatinformatics.ca/gmeow/examples/places/")
    g = Graph().parse(COVERAGE_FIXTURES / "places-virtual.ttl", format="turtle")
    result = run_shacl(g)
    assert result.ok, "\n".join(result.errors)
    assert (
        ex_vl.addrOldURL,
        URIRef(GMEOW + "displayable"),
        Literal(False),
    ) in g


def test_contested_dns_names_coexist() -> None:
    """Two standpoint-indexed DNS names for the same virtual location coexist (P9)."""
    from rdflib import Namespace

    ex_vl = Namespace("https://blackcatinformatics.ca/gmeow/examples/places/")
    g = Graph().parse(COVERAGE_FIXTURES / "places-virtual.ttl", format="turtle")
    result = run_shacl(g)
    assert result.ok, "\n".join(result.errors)
    addrs = set(g.objects(ex_vl.service, URIRef(GMEOW + "hasNetworkAddress")))
    assert {ex_vl.addrDNSCorpA, ex_vl.addrDNSCorpB} <= addrs
    values = {g.value(a, URIRef(GMEOW + "networkAddressValue")) for a in addrs}
    assert {
        Literal("service-corp-a.example"),
        Literal("service-corp-b.example"),
    } <= values

    # Verify standpoint annotations on reified axioms (P9).
    axioms = list(g.subjects(RDF.type, OWL.Axiom))
    assert len(axioms) >= 2
    has_addr = URIRef(GMEOW + "hasNetworkAddress")
    corp_a_claims = [
        ax
        for ax in axioms
        if (ax, OWL.annotatedSource, ex_vl.service) in g
        and (ax, OWL.annotatedProperty, has_addr) in g
        and (ax, OWL.annotatedTarget, ex_vl.addrDNSCorpA) in g
        and (ax, URIRef(GMEOW + "accordingTo"), ex_vl["standpoint-corp-a"]) in g
    ]
    assert len(corp_a_claims) == 1
    corp_b_claims = [
        ax
        for ax in axioms
        if (ax, OWL.annotatedSource, ex_vl.service) in g
        and (ax, OWL.annotatedProperty, has_addr) in g
        and (ax, OWL.annotatedTarget, ex_vl.addrDNSCorpB) in g
        and (ax, URIRef(GMEOW + "accordingTo"), ex_vl["standpoint-corp-b"]) in g
    ]
    assert len(corp_b_claims) == 1


def test_no_preferred_or_primary_virtual_location_term() -> None:
    """Principle 9: no single slot to win — virtual locations mint no preferred/primary
    selector for a contested type or network address."""
    g = _graph()
    prop_types = (OWL.ObjectProperty, OWL.DatatypeProperty, OWL.AnnotationProperty)
    for banned in (
        "primaryVirtualLocationType",
        "preferredVirtualLocationType",
        "primaryNetworkAddress",
        "preferredNetworkAddress",
        "primaryNetworkAddressType",
        "preferredNetworkAddressType",
    ):
        node = URIRef(GMEOW + banned)
        for pt in prop_types:
            assert (node, RDF.type, pt) not in g, f"{banned} must not exist"
        assert (node, RDF.type, OWL.Class) not in g


# =========================================================================== #
# Issue #85 — Celestial realm structural guards
# =========================================================================== #


def test_celestial_location_subclass_of_location() -> None:
    graph = _graph()
    assert (
        URIRef(GMEOW + "CelestialLocation"),
        RDFS.subClassOf,
        URIRef(GMEOW + "Location"),
    ) in graph


def test_celestial_coordinates_subclass_of_entity() -> None:
    graph = _graph()
    assert (
        URIRef(GMEOW + "CelestialCoordinates"),
        RDFS.subClassOf,
        URIRef(GMEOW + "Entity"),
    ) in graph


def test_celestial_object_type_is_value_not_subclass() -> None:
    graph = _graph()
    assert (
        URIRef(GMEOW + "CelestialObjectType"),
        RDFS.subClassOf,
        URIRef(GUFO + "QualityValue"),
    ) in graph
    celestial_type = URIRef(GMEOW + "celestialObjectType")
    assert (celestial_type, RDF.type, OWL.ObjectProperty) in graph
    assert (celestial_type, RDF.type, OWL.FunctionalProperty) not in graph
    for ind in (
        "celestialObjectTypeStar",
        "celestialObjectTypeGalaxy",
        "celestialObjectTypePlanet",
        "celestialObjectTypeNebula",
        "celestialObjectTypeAsteroid",
        "celestialObjectTypeComet",
        "celestialObjectTypeCluster",
        "celestialObjectTypeSpacecraft",
    ):
        assert (
            URIRef(GMEOW + ind),
            RDF.type,
            URIRef(GMEOW + "CelestialObjectType"),
        ) in graph
    for rejected in ("Star", "Galaxy", "Nebula", "Planet"):
        assert (URIRef(GMEOW + rejected), RDF.type, OWL.Class) not in graph


def test_reference_position_and_timescale_are_values() -> None:
    graph = _graph()
    assert (
        URIRef(GMEOW + "CelestialReferenceOrigin"),
        RDFS.subClassOf,
        URIRef(GUFO + "QualityValue"),
    ) in graph
    for ind in (
        "refOriginTopocentric",
        "refOriginGeocentric",
        "refOriginBarycentric",
        "refOriginHeliocentric",
    ):
        assert (
            URIRef(GMEOW + ind),
            RDF.type,
            URIRef(GMEOW + "CelestialReferenceOrigin"),
        ) in graph


def test_icrs_fk5_galactic_frames_exist() -> None:
    graph = _graph()
    for frame in ("referenceFrameICRS", "referenceFrameFK5", "referenceFrameGalactic"):
        frame_uri = URIRef(GMEOW + frame)
        assert (frame_uri, RDF.type, URIRef(GMEOW + "ReferenceFrame")) in graph
        assert (
            frame_uri,
            URIRef(GMEOW + "frameRealm"),
            URIRef(GMEOW + "frameRealmCelestial"),
        ) in graph
        assert (
            frame_uri,
            URIRef(GMEOW + "hasMetricKind"),
            URIRef(GMEOW + "metricGeodesic"),
        ) in graph


def test_celestial_coords_have_ra_dec_epoch() -> None:
    graph = _graph()
    for prop in ("rightAscension", "declination", "celestialEpoch"):
        node = URIRef(GMEOW + prop)
        assert (node, RDF.type, OWL.DatatypeProperty) in graph
        assert (node, RDFS.domain, URIRef(GMEOW + "CelestialCoordinates")) in graph


def test_celestial_location_has_coords_property() -> None:
    graph = _graph()
    node = URIRef(GMEOW + "hasCelestialCoordinates")
    assert (node, RDF.type, OWL.ObjectProperty) in graph
    assert (node, RDFS.domain, URIRef(GMEOW + "CelestialLocation")) in graph
    assert (node, RDFS.range, URIRef(GMEOW + "CelestialCoordinates")) in graph


def test_reference_frame_has_refpos_and_timescale() -> None:
    graph = _graph()
    assert (
        URIRef(GMEOW + "hasReferencePosition"),
        RDF.type,
        OWL.FunctionalProperty,
    ) in graph
    assert (
        URIRef(GMEOW + "hasTimeScale"),
        RDF.type,
        OWL.FunctionalProperty,
    ) in graph
    # Verify actual frame bindings: ICRS uses BARYCENTER + TDB
    icrs = URIRef(GMEOW + "referenceFrameICRS")
    assert (
        icrs,
        URIRef(GMEOW + "hasReferencePosition"),
        URIRef(GMEOW + "refOriginBarycentric"),
    ) in graph
    assert (
        icrs,
        URIRef(GMEOW + "hasTimeScale"),
        URIRef(GMEOW + "timeScaleTDB"),
    ) in graph


# =========================================================================== #
# Issue #87 — Psychological / cognitive realm structural guards
# =========================================================================== #


def test_is_hosted_by_property_exists() -> None:
    """Issue #87: isHostedBy links a ReferenceFrame to its hosting Entity."""
    graph = _graph()
    prop = URIRef(GMEOW + "isHostedBy")
    assert (prop, RDF.type, OWL.ObjectProperty) in graph
    assert (prop, RDFS.domain, URIRef(GMEOW + "ReferenceFrame")) in graph
    assert (prop, RDFS.range, URIRef(GMEOW + "Entity")) in graph


def test_psychological_frame_realm_exists() -> None:
    """Issue #87: frameRealmPsychological is present for mental reference frames."""
    graph = _graph()
    assert (
        URIRef(GMEOW + "frameRealmPsychological"),
        RDF.type,
        URIRef(GMEOW + "FrameRealm"),
    ) in graph


def test_psychological_axes_exist() -> None:
    """Issue #87: psychological / cognitive axes are present."""
    graph = _graph()
    for axis in (
        "axisValence",
        "axisArousal",
        "axisConceptualSimilarity",
        "axisEgocentricForward",
        "axisEgocentricLateral",
        "axisAllocentricX",
        "axisAllocentricY",
        "axisImaginedSpaceX",
        "axisImaginedSpaceY",
        "axisImaginedSpaceZ",
    ):
        assert (URIRef(GMEOW + axis), RDF.type, URIRef(GMEOW + "Axis")) in graph


# --------------------------------------------------------------------------- #
# Biological-sequence realm — FALDO / Sequence Ontology / GFF3 (#90)
# --------------------------------------------------------------------------- #


def test_biological_sequence_location_subclass_of_location() -> None:
    graph = _graph()
    assert (
        URIRef(GMEOW + "BiologicalSequenceLocation"),
        RDFS.subClassOf,
        URIRef(GMEOW + "Location"),
    ) in graph


def test_sequence_feature_type_is_value_not_subclass() -> None:
    graph = _graph()
    assert (
        URIRef(GMEOW + "SequenceFeatureType"),
        RDFS.subClassOf,
        URIRef(GUFO + "QualityValue"),
    ) in graph
    for sft in (
        "sequenceFeatureTypeGene",
        "sequenceFeatureTypeExon",
        "sequenceFeatureTypeIntron",
        "sequenceFeatureTypeCDS",
        "sequenceFeatureTypeSNP",
        "sequenceFeatureTypeChromosome",
    ):
        assert (
            URIRef(GMEOW + sft),
            RDF.type,
            URIRef(GMEOW + "SequenceFeatureType"),
        ) in graph
    for rejected in ("Gene", "Exon", "Intron", "CDS", "SNP"):
        assert (URIRef(GMEOW + rejected), RDF.type, OWL.Class) not in graph


def test_strand_orientation_values() -> None:
    graph = _graph()
    assert (
        URIRef(GMEOW + "StrandOrientation"),
        RDFS.subClassOf,
        URIRef(GUFO + "QualityValue"),
    ) in graph
    for strand in ("strandForward", "strandReverse", "strandBoth"):
        assert (
            URIRef(GMEOW + strand),
            RDF.type,
            URIRef(GMEOW + "StrandOrientation"),
        ) in graph


def test_sequence_coordinates_properties() -> None:
    graph = _graph()
    # sequenceStart — positiveInteger, functional, domain SequenceCoordinates
    start = URIRef(GMEOW + "sequenceStart")
    assert (start, RDF.type, OWL.DatatypeProperty) in graph
    assert (start, RDF.type, OWL.FunctionalProperty) in graph
    assert (start, RDFS.domain, URIRef(GMEOW + "SequenceCoordinates")) in graph
    assert (start, RDFS.range, XSD.positiveInteger) in graph

    # sequenceEnd — positiveInteger, functional, domain SequenceCoordinates
    end = URIRef(GMEOW + "sequenceEnd")
    assert (end, RDF.type, OWL.DatatypeProperty) in graph
    assert (end, RDF.type, OWL.FunctionalProperty) in graph
    assert (end, RDFS.domain, URIRef(GMEOW + "SequenceCoordinates")) in graph
    assert (end, RDFS.range, XSD.positiveInteger) in graph

    # sequenceStrand — StrandOrientation, functional, domain SequenceCoordinates
    strand = URIRef(GMEOW + "sequenceStrand")
    assert (strand, RDF.type, OWL.ObjectProperty) in graph
    assert (strand, RDF.type, OWL.FunctionalProperty) in graph
    assert (strand, RDFS.domain, URIRef(GMEOW + "SequenceCoordinates")) in graph
    assert (strand, RDFS.range, URIRef(GMEOW + "StrandOrientation")) in graph

    # inReferenceAssembly — ReferenceFrame, functional, subPropertyOf hasReferenceFrame
    ref = URIRef(GMEOW + "inReferenceAssembly")
    assert (ref, RDF.type, OWL.ObjectProperty) in graph
    assert (ref, RDF.type, OWL.FunctionalProperty) in graph
    assert (ref, RDFS.domain, URIRef(GMEOW + "SequenceCoordinates")) in graph
    assert (ref, RDFS.range, URIRef(GMEOW + "ReferenceFrame")) in graph
    assert (
        ref,
        RDFS.subPropertyOf,
        URIRef(GMEOW + "hasReferenceFrame"),
    ) in graph

    # hasSequenceCoordinates — domain SequenceFeature, range SequenceCoordinates
    hsc = URIRef(GMEOW + "hasSequenceCoordinates")
    assert (hsc, RDF.type, OWL.ObjectProperty) in graph
    assert (hsc, RDFS.domain, URIRef(GMEOW + "SequenceFeature")) in graph
    assert (hsc, RDFS.range, URIRef(GMEOW + "SequenceCoordinates")) in graph
    assert (hsc, RDF.type, OWL.FunctionalProperty) not in graph

    # sequenceFeatureType — non-functional, co-equal classifications
    sft = URIRef(GMEOW + "sequenceFeatureType")
    assert (sft, RDF.type, OWL.ObjectProperty) in graph
    assert (sft, RDFS.domain, URIRef(GMEOW + "SequenceFeature")) in graph
    assert (sft, RDFS.range, URIRef(GMEOW + "SequenceFeatureType")) in graph
    assert (sft, RDF.type, OWL.FunctionalProperty) not in graph

    # hasSequenceFeature — domain BiologicalSequenceLocation, range SequenceFeature
    hsf = URIRef(GMEOW + "hasSequenceFeature")
    assert (hsf, RDF.type, OWL.ObjectProperty) in graph
    assert (hsf, RDFS.domain, URIRef(GMEOW + "BiologicalSequenceLocation")) in graph
    assert (hsf, RDFS.range, URIRef(GMEOW + "SequenceFeature")) in graph


def test_grch38_reference_frame_seeded() -> None:
    graph = _graph()
    grch38 = URIRef(GMEOW + "referenceFrameGRCh38")
    assert (grch38, RDF.type, URIRef(GMEOW + "ReferenceFrame")) in graph
    assert (
        grch38,
        URIRef(GMEOW + "frameRealm"),
        URIRef(GMEOW + "frameRealmBiological"),
    ) in graph
    assert (
        grch38,
        URIRef(GMEOW + "frameKind"),
        URIRef(GMEOW + "frameKindLinearSequence"),
    ) in graph
    assert (
        grch38,
        URIRef(GMEOW + "hasAxis"),
        URIRef(GMEOW + "axisSequencePosition"),
    ) in graph
    assert (
        grch38,
        URIRef(GMEOW + "dimensionCount"),
        Literal(1, datatype=XSD.nonNegativeInteger),
    ) in graph
    assert (
        grch38,
        URIRef(GMEOW + "determinacyModel"),
        URIRef(GMEOW + "determinacyCrisp"),
    ) in graph
    assert (
        grch38,
        URIRef(GMEOW + "hasMetricKind"),
        URIRef(GMEOW + "metricPositionalDistance"),
    ) in graph


def test_frame_realm_biological_seeded() -> None:
    graph = _graph()
    assert (
        URIRef(GMEOW + "frameRealmBiological"),
        RDF.type,
        URIRef(GMEOW + "FrameRealm"),
    ) in graph
    assert (
        URIRef(GMEOW + "frameKindLinearSequence"),
        RDF.type,
        URIRef(GMEOW + "FrameKind"),
    ) in graph
    assert (
        URIRef(GMEOW + "axisSequencePosition"),
        RDF.type,
        URIRef(GMEOW + "Axis"),
    ) in graph


def test_sequence_feature_has_coordinates_and_type() -> None:
    """A SequenceFeature can bear coordinates and a feature type."""
    graph = _graph()
    assert (URIRef(GMEOW + "SequenceFeature"), RDF.type, OWL.Class) in graph
    assert (
        URIRef(GMEOW + "hasSequenceCoordinates"),
        RDFS.domain,
        URIRef(GMEOW + "SequenceFeature"),
    ) in graph
    assert (
        URIRef(GMEOW + "sequenceFeatureType"),
        RDFS.domain,
        URIRef(GMEOW + "SequenceFeature"),
    ) in graph


def test_biological_coverage_passes_shacl() -> None:
    """A biological-sequence coverage fixture with GRCh38 features loads and
    passes SHACL validation."""
    g = Graph().parse(COVERAGE_FIXTURES / "places-biological.ttl", format="turtle")
    result = run_shacl(g)
    assert result.ok, "\n".join(result.errors)


def test_biological_standpoint_coordinate_claims_coexist() -> None:
    """Two standpoint-indexed SequenceCoordinates on the same gene load,
    SHACL-pass, and are BOTH retained (Principle 9)."""
    g = Graph().parse(COVERAGE_FIXTURES / "places-biological.ttl", format="turtle")
    result = run_shacl(g)
    assert result.ok, "\n".join(result.errors)
    coords = set(
        g.objects(
            URIRef("https://blackcatinformatics.ca/gmeow/examples/places/geneBRCA1Alt"),
            URIRef(GMEOW + "hasSequenceCoordinates"),
        )
    )
    assert len(coords) >= 2, "Expected at least two co-existing coordinate claims"
