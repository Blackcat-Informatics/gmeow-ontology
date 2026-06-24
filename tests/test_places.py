"""Retained guards for the places slice — bnode/chain, ABox fixture, and
label-content tests that cannot be expressed as module-scoped SPARQL ASK
cells in slices/core/places/tests/structural.ttl.

All pure TBox invariants (subClassOf, domain/range, functional/transitive/
symmetric, value-vocab seeds, mustNot-existence guards) have been migrated to
structural.ttl and are exercised by the native Rust slicetest harness
(crates/slicetest).

RETAINED here:
  * test_location_superset_core -- bnode owl:unionOf list walks, dynamic
      owl:AllDisjointProperties subject sweep, locatedAt property-chain
      bnode list check; not expressible as module-scoped ASK.
  * test_no_unsafe_motion_property_chains -- dynamic graph.triples() sweep
      over a live subject list; module-scoped cell would silently narrow.
  * test_has_coordinate_matrix_includes_geocode -- bnode unionOf walk.
  * test_has_coordinates_property_chain -- bnode property-chain list check.
  * test_has_geometry_property_chain -- bnode property-chain list check.
  * test_place_type_parcel_exists -- checks graph.value() label string
      contents ("parcel" not in label); string-content not expressible as ASK.
  * ABox fixture + run_shacl() tests (ExampleConformance + coexistence).
  * Numeric Decimal equality checks on ABox fixture values.
"""

from __future__ import annotations

from decimal import Decimal
from pathlib import Path

from gmeow_rdf.compat.rdflib import OWL, RDF, RDFS, Graph, Literal, Namespace, URIRef
from gmeow_rdf.compat.rdflib.namespace import XSD

from gmeow_tools.graph import load_merged_graph
from tests._graph_nt import run_shacl

GMEOW = "https://blackcatinformatics.ca/gmeow/"
LOGIC = "https://blackcatinformatics.ca/logic/"
GEO = "http://www.opengis.net/ont/geosparql#"
EX_PLACES = Namespace("https://blackcatinformatics.ca/gmeow/examples/places/")
COVERAGE_FIXTURES = Path(__file__).parent / "fixtures" / "coverage"


def _graph() -> Graph:
    return load_merged_graph(include_imports=False)


def test_location_superset_core() -> None:
    """Verifies classes, value scaffolds, properties, and topology of
    Location Core. Retained: bnode owl:unionOf walks, AllDisjointProperties
    sweep, locatedAt propertyChainAxiom bnode list check."""
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
    assert (
        URIRef(GMEOW + "dimensionCount"),
        RDF.type,
        OWL.FunctionalProperty,
    ) in graph
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
    assert (
        URIRef(GMEOW + "requiresHost"),
        RDF.type,
        OWL.FunctionalProperty,
    ) in graph
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
    assert (
        URIRef(GMEOW + "parentFrame"),
        RDF.type,
        OWL.FunctionalProperty,
    ) in graph
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
    # hasCoordinateMatrix domain is an owl:unionOf(Axis, Pose, SpatialCoords).
    # Retained: bnode owl:unionOf list walk.
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
        URIRef(GMEOW + "Geocode"),
    }
    assert (URIRef(GMEOW + "hasCoordinateMatrix"), RDFS.range, RDFS.Literal) in graph

    # Pose / Orientation properties
    assert (
        URIRef(GMEOW + "hasPose"),
        RDFS.domain,
        URIRef(GMEOW + "Entity"),
    ) in graph
    assert (URIRef(GMEOW + "hasPose"), RDFS.range, URIRef(GMEOW + "Pose")) in graph
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
    assert (URIRef(GMEOW + "eulerOrder"), RDFS.range, XSD.string) in graph

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

    # 5. locatedAt property chain axiom — bnode list check (retained).
    chain_head = graph.value(URIRef(GMEOW + "locatedAt"), OWL.propertyChainAxiom)
    assert chain_head is not None
    chain_elements = list(graph.items(chain_head))
    assert chain_elements == [
        URIRef(GMEOW + "locatedAt"),
        URIRef(GMEOW + "containedInLocation"),
    ]

    # 6. RCC-8 JEPD disjoint properties — dynamic subject sweep (retained).
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


def test_no_unsafe_motion_property_chains() -> None:
    """Principle 12: interpolation and coordinate transforms stay in solver.
    Retained: dynamic graph.triples() sweep over a live subject list."""
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


def test_has_coordinate_matrix_includes_geocode() -> None:
    """Retained: bnode owl:unionOf list walk to verify Geocode membership."""
    graph = _graph()
    hcm_domain = graph.value(URIRef(GMEOW + "hasCoordinateMatrix"), RDFS.domain)
    assert hcm_domain is not None
    assert (hcm_domain, RDF.type, OWL.Class) in graph
    union_of = graph.value(hcm_domain, OWL.unionOf)
    assert union_of is not None
    union_members = set(graph.items(union_of))
    assert URIRef(GMEOW + "Geocode") in union_members


def test_has_coordinates_property_chain() -> None:
    """Retained: bnode property-chain list check (exact member order)."""
    graph = _graph()
    chain_head = graph.value(URIRef(GMEOW + "hasCoordinates"), OWL.propertyChainAxiom)
    assert chain_head is not None
    chain_elements = list(graph.items(chain_head))
    assert chain_elements == [
        URIRef(GMEOW + "hasCoordinateObservation"),
        URIRef(GMEOW + "coordinateResult"),
    ]


def test_has_geometry_property_chain() -> None:
    """Retained: bnode property-chain list check (exact member order)."""
    graph = _graph()
    chain_head = graph.value(URIRef(GMEOW + "hasGeometry"), OWL.propertyChainAxiom)
    assert chain_head is not None
    chain_elements = list(graph.items(chain_head))
    assert chain_elements == [
        URIRef(GMEOW + "hasCoordinateObservation"),
        URIRef(GMEOW + "geometryResult"),
    ]


def test_place_type_parcel_exists() -> None:
    """Retained: label string-content check ('parcel' not in label)."""
    graph = _graph()
    assert (
        URIRef(GMEOW + "placeTypeParcel"),
        RDF.type,
        URIRef(GMEOW + "PlaceType"),
    ) in graph
    # placeTypeSite label was narrowed from "site / campus / parcel" to
    # "site / campus" — string-content check not expressible as ASK.
    site_label = graph.value(URIRef(GMEOW + "placeTypeSite"), RDFS.label)
    assert site_label is not None
    assert "parcel" not in str(site_label).lower()


# --------------------------------------------------------------------------- #
# Standpoint coexistence — contested sovereignty / place names (#51)
# --------------------------------------------------------------------------- #


def test_contested_sovereignty_coexists() -> None:
    """Two contradictory standpoint-indexed containedInPlace claims load,
    SHACL-pass, and are BOTH retained — neither is the ground truth."""
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
    """A superseded place name is retained with displayable false (P10)."""
    g = Graph().parse(COVERAGE_FIXTURES / "places-contested.ttl", format="turtle")
    result = run_shacl(g)
    assert result.ok, "\n".join(result.errors)
    assert (
        EX_PLACES.nameHistorical,
        URIRef(GMEOW + "displayable"),
        Literal(False),
    ) in g


# --------------------------------------------------------------------------- #
# Terrestrial realm deepening (#82)
# --------------------------------------------------------------------------- #


def test_contested_jurisdiction_tenures_coexist() -> None:
    """Two contradictory JurisdictionTenures on the same place load,
    SHACL-pass, and are BOTH retained (Principle 9)."""
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


def test_geometry_has_type_and_geojson() -> None:
    """A geometry may carry both a GeometryType value and a GeoJSON
    serialization."""
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
# RegulatoryOverlay — legal / regulatory overlays (#103)
# --------------------------------------------------------------------------- #


def test_contested_regulatory_overlays_coexist() -> None:
    """Two contradictory RegulatoryOverlays on the same place load,
    SHACL-pass, and are BOTH retained (Principle 9)."""
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
    """A RegulatoryOverlay may link to a RightsStatement for the deontic
    rules that govern activity within the overlay."""
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


def test_contested_eez_coexistence() -> None:
    """Two contradictory EEZ RegulatoryOverlays on the same maritime place
    load, SHACL-pass, and are BOTH retained (Principle 9)."""
    g = Graph().parse(COVERAGE_FIXTURES / "places-maritime.ttl", format="turtle")
    result = run_shacl(g)
    assert result.ok, "\n".join(result.errors)
    overlays = set(g.subjects(RDF.type, URIRef(GMEOW + "RegulatoryOverlay")))
    assert len(overlays) >= 2, "Expected at least two co-existing EEZ overlays"
    authorities = set()
    for overlay in overlays:
        auth = g.value(overlay, URIRef(GMEOW + "overlayAuthority"))
        if auth:
            authorities.add(auth)
    assert len(authorities) >= 2, "Expected at least two distinct authority claims"
    found_depth_frame = False
    for overlay in overlays:
        lower = g.value(overlay, URIRef(GMEOW + "overlayLowerBound"))
        if lower:
            frame = g.value(lower, URIRef(GMEOW + "hasReferenceFrame"))
            if frame and frame == URIRef(GMEOW + "referenceFrameDepthBelowSeaLevel"):
                found_depth_frame = True
    assert found_depth_frame, "Expected at least one depth bound using maritime frame"


# --------------------------------------------------------------------------- #
# Motion — Streaming (#96)
# --------------------------------------------------------------------------- #


def test_location_stream_to_trajectory_derivation() -> None:
    """A Trajectory derived from a Stream of LocationStates loads and passes
    SHACL."""
    ex_str = Namespace("https://blackcatinformatics.ca/gmeow/examples/places/")
    g = Graph().parse(COVERAGE_FIXTURES / "places-streaming.ttl", format="turtle")
    result = run_shacl(g)
    assert result.ok, "\n".join(result.errors)
    assert (
        ex_str.trajectory1,
        URIRef(GMEOW + "wasDerivedFrom"),
        ex_str.stream1,
    ) in g
    samples = set(g.objects(ex_str.stream1, URIRef(GMEOW + "streamSample")))
    assert len(samples) == 3, f"Expected 3 stream samples, got {len(samples)}"
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


# --------------------------------------------------------------------------- #
# Capacity / Occupancy / Utilization (#100)
# --------------------------------------------------------------------------- #


def test_contested_capacity_claims_coexist() -> None:
    """Two contradictory Capacity measurements on the same location load,
    SHACL-pass, and are BOTH retained (Principle 9)."""
    ex_cap = Namespace("https://blackcatinformatics.ca/gmeow/examples/places/")
    g = Graph().parse(COVERAGE_FIXTURES / "places-capacity.ttl", format="turtle")
    result = run_shacl(g)
    assert result.ok, "\n".join(result.errors)
    caps = set(g.subjects(URIRef(GMEOW + "capacityOf"), ex_cap.venue))
    assert {ex_cap.capFireCode, ex_cap.capVenueClaim} <= caps


def test_superseded_capacity_suppressed() -> None:
    """A superseded capacity is retained with displayable false (P10)."""
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
    """An Occupancy measurement carries a scalar quantity with a QUDT unit.
    Retained: Decimal numeric equality check on ABox fixture value."""
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
    """A StorageLocation can have a capacity in bytes (QUDT BYTE unit).
    Retained: Decimal numeric equality check on ABox fixture value."""
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
# Virtual Location Type + Network Address Space (#84)
# --------------------------------------------------------------------------- #


def test_virtual_location_types_coexist() -> None:
    """A VirtualLocation may have multiple virtualLocationType values (P9)."""
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
    """Two standpoint-indexed DNS names for the same virtual location
    coexist (P9)."""
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
        and (
            ax,
            URIRef(GMEOW + "accordingTo"),
            ex_vl["standpoint-corp-a"],
        )
        in g
    ]
    assert len(corp_a_claims) == 1
    corp_b_claims = [
        ax
        for ax in axioms
        if (ax, OWL.annotatedSource, ex_vl.service) in g
        and (ax, OWL.annotatedProperty, has_addr) in g
        and (ax, OWL.annotatedTarget, ex_vl.addrDNSCorpB) in g
        and (
            ax,
            URIRef(GMEOW + "accordingTo"),
            ex_vl["standpoint-corp-b"],
        )
        in g
    ]
    assert len(corp_b_claims) == 1


# --------------------------------------------------------------------------- #
# Biological sequence (#90)
# --------------------------------------------------------------------------- #


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


# --------------------------------------------------------------------------- #
# Geocoding frames (#91)
# --------------------------------------------------------------------------- #


def test_geocode_shape_invalid_no_code() -> None:
    """A Geocode without any code value fails SHACL."""
    g = Graph().parse(COVERAGE_FIXTURES / "places-geocode.ttl", format="turtle")
    for pred in (
        "plusCode",
        "what3words",
        "geohash",
        "mgrs",
        "unLocode",
        "mileMarker",
        "geocodeValue",
        "hasCoordinateMatrix",
    ):
        g.remove((EX_PLACES.pc1, URIRef(GMEOW + pred), None))
    result = run_shacl(g)
    assert not result.ok


def test_geocode_shape_invalid_two_codes() -> None:
    """A Geocode with two code values fails SHACL."""
    g = Graph().parse(COVERAGE_FIXTURES / "places-geocode.ttl", format="turtle")
    g.add((EX_PLACES.pc1, URIRef(GMEOW + "geohash"), Literal("u4pruydqqvj")))
    result = run_shacl(g)
    assert not result.ok


# --------------------------------------------------------------------------- #
# SpatialMeasurement / CoordinateObservation (#125)
# --------------------------------------------------------------------------- #


def test_coordinate_observations_coexist() -> None:
    """Multiple CoordinateObservations on the same place load, SHACL-pass,
    and are BOTH retained (Principle 9)."""
    g = Graph().parse(
        COVERAGE_FIXTURES / "coordinate-observations.ttl", format="turtle"
    )
    result = run_shacl(g)
    assert result.ok, "\n".join(result.errors)
    observations = set(
        g.subjects(
            URIRef(GMEOW + "coordinateObservationOf"),
            EX_PLACES.surveyedPlace,
        )
    )
    assert {EX_PLACES.gpsObservation, EX_PLACES.lidarObservation} <= observations


def test_superseded_coordinate_observation_suppressed() -> None:
    """A superseded coordinate observation is retained with displayable
    false."""
    g = Graph().parse(
        COVERAGE_FIXTURES / "coordinate-observations.ttl", format="turtle"
    )
    result = run_shacl(g)
    assert result.ok, "\n".join(result.errors)
    assert (
        EX_PLACES.oldObservation,
        URIRef(GMEOW + "displayable"),
        Literal(False),
    ) in g


# --------------------------------------------------------------------------- #
# Cadastral / land administration (#92)
# --------------------------------------------------------------------------- #


def test_land_tenure_instance_structure() -> None:
    """A LandTenure instance binds place, party, type, and interval."""
    g = Graph().parse(COVERAGE_FIXTURES / "places-cadastral.ttl", format="turtle")
    result = run_shacl(g)
    assert result.ok, "\n".join(result.errors)
    tenures = list(g.subjects(RDF.type, URIRef(GMEOW + "LandTenure")))
    assert tenures, "Expected at least one LandTenure"
    for tenure in tenures:
        place = g.value(tenure, URIRef(GMEOW + "tenurePlace"))
        assert place is not None, "LandTenure must have a tenurePlace"
        party = g.value(tenure, URIRef(GMEOW + "tenureParty"))
        assert party is not None, "LandTenure must have a tenureParty"
        ttype = g.value(tenure, URIRef(GMEOW + "tenureType"))
        assert ttype is not None, "LandTenure must have a tenureType"
        interval = g.value(tenure, URIRef(GMEOW + "duringInterval"))
        assert interval is not None, "LandTenure must have a duringInterval"


def test_cadastral_reference_instance_structure() -> None:
    """A CadastralReference instance binds value, type, authority, and
    jurisdiction."""
    g = Graph().parse(COVERAGE_FIXTURES / "places-cadastral.ttl", format="turtle")
    result = run_shacl(g)
    assert result.ok, "\n".join(result.errors)
    refs = list(g.subjects(RDF.type, URIRef(GMEOW + "CadastralReference")))
    assert refs, "Expected at least one CadastralReference"
    for ref in refs:
        value = g.value(ref, URIRef(GMEOW + "referenceValue"))
        assert value is not None, "CadastralReference must have a referenceValue"
        rtype = g.value(ref, URIRef(GMEOW + "referenceType"))
        assert rtype is not None, "CadastralReference must have a referenceType"
        auth = g.value(ref, URIRef(GMEOW + "referenceAuthority"))
        assert auth is not None, "CadastralReference must have a referenceAuthority"
        juris = g.value(ref, URIRef(GMEOW + "referenceJurisdiction"))
        assert juris is not None, "CadastralReference must have a referenceJurisdiction"


def test_contested_land_tenures_coexist() -> None:
    """Two contradictory LandTenures on the same parcel load, SHACL-pass,
    and are BOTH retained (Principle 9)."""
    g = Graph().parse(COVERAGE_FIXTURES / "places-cadastral.ttl", format="turtle")
    result = run_shacl(g)
    assert result.ok, "\n".join(result.errors)
    tenures = set(g.subjects(RDF.type, URIRef(GMEOW + "LandTenure")))
    contested = [
        t
        for t in tenures
        if g.value(t, URIRef(GMEOW + "tenureType"))
        == URIRef(GMEOW + "tenureTypeOwnership")
    ]
    assert len(contested) >= 2, "Expected at least two co-existing ownership claims"
    parties = set()
    for tenure in contested:
        party = g.value(tenure, URIRef(GMEOW + "tenureParty"))
        if party:
            parties.add(party)
    assert len(parties) >= 2, "Expected at least two distinct party claims"


def test_lapsed_tenure_suppressed_not_deleted() -> None:
    """A lapsed easement tenure is retained with displayable false (P10)."""
    g = Graph().parse(COVERAGE_FIXTURES / "places-cadastral.ttl", format="turtle")
    result = run_shacl(g)
    assert result.ok, "\n".join(result.errors)
    lapsed = EX_PLACES.lapsedTenure
    assert (lapsed, RDF.type, URIRef(GMEOW + "LandTenure")) in g
    displayable = g.value(lapsed, URIRef(GMEOW + "displayable"))
    assert displayable == Literal(False), (
        "Lapsed tenure must be suppressed (displayable false)"
    )


def test_cadastral_reference_multiple_types_coexist() -> None:
    """A CadastralReference may carry multiple co-equal type claims (P9)."""
    g = Graph().parse(COVERAGE_FIXTURES / "places-cadastral.ttl", format="turtle")
    result = run_shacl(g)
    assert result.ok, "\n".join(result.errors)
    multi_ref = EX_PLACES.refMulti
    types = set(g.objects(multi_ref, URIRef(GMEOW + "referenceType")))
    assert len(types) >= 2, "Expected at least two co-existing reference type claims"
    assert URIRef(GMEOW + "referenceTypeParcelId") in types
    assert URIRef(GMEOW + "referenceTypeTitle") in types


# Retained (cross-slice): postalAddress* terms are home-asserted in core/contacts and
# gmeow:hasPlaceName/PlaceName in core/names -- cross-slice, see #867.


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
    assert (
        URIRef(GMEOW + "PostalAddress"),
        RDFS.subClassOf,
        URIRef(GMEOW + "ContactPoint"),
    ) in graph


def test_postal_address_frame_property() -> None:
    """postalAddressFrame is a functional sub-property of hasReferenceFrame."""
    graph = _graph()
    prop = URIRef(GMEOW + "postalAddressFrame")
    assert (prop, RDF.type, OWL.ObjectProperty) in graph
    assert (prop, RDF.type, OWL.FunctionalProperty) in graph
    assert (prop, RDFS.subPropertyOf, URIRef(GMEOW + "hasReferenceFrame")) in graph
    assert (prop, RDFS.domain, URIRef(GMEOW + "PostalAddress")) in graph
    assert (prop, RDFS.range, URIRef(GMEOW + "ReferenceFrame")) in graph
