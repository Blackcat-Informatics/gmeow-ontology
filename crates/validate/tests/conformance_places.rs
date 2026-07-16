// SPDX-License-Identifier: AGPL-3.0-only

//! Conformance twins migrated from tests/test_places.py (all 36 tests).
//!
//! TBox / bnode tests over the merged ontology (`GraphStore::ontology()`):
//! - `test_location_superset_core` → `location_superset_core`
//! - `test_no_unsafe_motion_property_chains` → `no_unsafe_motion_property_chains`
//! - `test_has_coordinate_matrix_includes_geocode` → `has_coordinate_matrix_includes_geocode`
//! - `test_has_coordinates_property_chain` → `has_coordinates_property_chain`
//! - `test_has_geometry_property_chain` → `has_geometry_property_chain`
//! - `test_place_type_parcel_exists` → `place_type_parcel_exists`
//!
//! SHACL fixture (`run_shacl`) + structural coexistence tests over the
//! `tests/fixtures/coverage/*.ttl` fixtures:
//! - `test_contested_sovereignty_coexists` → `contested_sovereignty_coexists`
//! - `test_contested_place_names_coexist` → `contested_place_names_coexist`
//! - `test_superseded_historical_name_suppressed` → `superseded_historical_name_suppressed`
//! - `test_contested_jurisdiction_tenures_coexist` → `contested_jurisdiction_tenures_coexist`
//! - `test_containment_tenure_records_border_change` → `containment_tenure_records_border_change`
//! - `test_geometry_has_type_and_geojson` → `geometry_has_type_and_geojson`
//! - `test_contested_regulatory_overlays_coexist` → `contested_regulatory_overlays_coexist`
//! - `test_regulatory_overlay_linked_to_rights_statement` → `regulatory_overlay_linked_to_rights_statement`
//! - `test_regulatory_overlay_3d_bounds` → `regulatory_overlay_3d_bounds`
//! - `test_contested_eez_coexistence` → `contested_eez_coexistence`
//! - `test_location_stream_to_trajectory_derivation` → `location_stream_to_trajectory_derivation`
//! - `test_stream_and_trajectory_coexist` → `stream_and_trajectory_coexist`
//! - `test_contested_capacity_claims_coexist` → `contested_capacity_claims_coexist`
//! - `test_superseded_capacity_suppressed` → `superseded_capacity_suppressed`
//! - `test_occupancy_with_unit_asserted` → `occupancy_with_unit_asserted`
//! - `test_storage_capacity_in_bytes` → `storage_capacity_in_bytes`
//! - `test_virtual_location_types_coexist` → `virtual_location_types_coexist`
//! - `test_network_addresses_in_different_frames_coexist` → `network_addresses_in_different_frames_coexist`
//! - `test_superseded_network_address_suppressed` → `superseded_network_address_suppressed`
//! - `test_contested_dns_names_coexist` → `contested_dns_names_coexist`
//! - `test_biological_standpoint_coordinate_claims_coexist` → `biological_standpoint_coordinate_claims_coexist`
//! - `test_geocode_shape_invalid_no_code` → `geocode_shape_invalid_no_code`
//! - `test_geocode_shape_invalid_two_codes` → `geocode_shape_invalid_two_codes`
//! - `test_coordinate_observations_coexist` → `coordinate_observations_coexist`
//! - `test_superseded_coordinate_observation_suppressed` → `superseded_coordinate_observation_suppressed`
//! - `test_land_tenure_instance_structure` → `land_tenure_instance_structure`
//! - `test_cadastral_reference_instance_structure` → `cadastral_reference_instance_structure`
//! - `test_contested_land_tenures_coexist` → `contested_land_tenures_coexist`
//! - `test_lapsed_tenure_suppressed_not_deleted` → `lapsed_tenure_suppressed_not_deleted`
//! - `test_cadastral_reference_multiple_types_coexist` → `cadastral_reference_multiple_types_coexist`
//!
//! Blank-node walks (bnode-aware `*_h` helpers): the `hasCoordinateMatrix`
//! `owl:unionOf` domain, the `locatedAt` / `hasCoordinates` / `hasGeometry`
//! `owl:propertyChainAxiom` `rdf:List`s, and the RCC-8 `owl:AllDisjointProperties`
//! `owl:members` sweep. The `run_shacl` fixtures use only named IRIs in the
//! subjects/objects the originals traversed, so their structural checks use the
//! IRI-only `objects`/`subjects`/`has`/`has_literal` helpers; the geocode
//! mutation tests are expressed as inline Turtle `Case`s that `fails()` SHACL.

mod conformance_support;
use conformance_support::*;
use purrdf::slice::rdf_query::{Object, Subject};
use std::collections::BTreeSet;

const GMEOW: &str = "https://blackcatinformatics.ca/gmeow/";
const MATH: &str = "https://blackcatinformatics.ca/math/";
const EX: &str = "https://blackcatinformatics.ca/gmeow/examples/places/";

const RDFS_LABEL: &str = "http://www.w3.org/2000/01/rdf-schema#label";
const RDFS_DOMAIN: &str = "http://www.w3.org/2000/01/rdf-schema#domain";
const RDFS_RANGE: &str = "http://www.w3.org/2000/01/rdf-schema#range";
const RDFS_SUB_PROPERTY_OF: &str = "http://www.w3.org/2000/01/rdf-schema#subPropertyOf";
const RDFS_LITERAL: &str = "http://www.w3.org/2000/01/rdf-schema#Literal";

const OWL_CLASS: &str = "http://www.w3.org/2002/07/owl#Class";
const OWL_OBJECT_PROPERTY: &str = "http://www.w3.org/2002/07/owl#ObjectProperty";
const OWL_DATATYPE_PROPERTY: &str = "http://www.w3.org/2002/07/owl#DatatypeProperty";
const OWL_FUNCTIONAL_PROPERTY: &str = "http://www.w3.org/2002/07/owl#FunctionalProperty";
const OWL_TRANSITIVE_PROPERTY: &str = "http://www.w3.org/2002/07/owl#TransitiveProperty";
const OWL_SYMMETRIC_PROPERTY: &str = "http://www.w3.org/2002/07/owl#SymmetricProperty";
const OWL_UNION_OF: &str = "http://www.w3.org/2002/07/owl#unionOf";
const OWL_PROPERTY_CHAIN_AXIOM: &str = "http://www.w3.org/2002/07/owl#propertyChainAxiom";
const OWL_ALL_DISJOINT_PROPERTIES: &str = "http://www.w3.org/2002/07/owl#AllDisjointProperties";
const OWL_MEMBERS: &str = "http://www.w3.org/2002/07/owl#members";

const XSD_NON_NEGATIVE_INTEGER: &str = "http://www.w3.org/2001/XMLSchema#nonNegativeInteger";
const XSD_BOOLEAN: &str = "http://www.w3.org/2001/XMLSchema#boolean";
const XSD_DOUBLE: &str = "http://www.w3.org/2001/XMLSchema#double";
const XSD_STRING: &str = "http://www.w3.org/2001/XMLSchema#string";
const XSD_DECIMAL: &str = "http://www.w3.org/2001/XMLSchema#decimal";

const QUDT_BYTE: &str = "http://qudt.org/vocab/unit/BYTE";

fn gmeow(local: &str) -> String {
    format!("{GMEOW}{local}")
}

fn math(local: &str) -> String {
    format!("{MATH}{local}")
}

fn ex(local: &str) -> String {
    format!("{EX}{local}")
}

/// Named list of IRIs the union/chain walks yield, filtered to `Object::Named`.
fn named_set(objects: &[Object]) -> BTreeSet<String> {
    objects
        .iter()
        .filter_map(|o| match o {
            Object::Named(iri) => Some(iri.clone()),
            _ => None,
        })
        .collect()
}

/// Load a `tests/fixtures/coverage/{name}.ttl` fixture into a store AND assert it
/// passes SHACL fixture-only validation (the native twin of `run_shacl(g)`).
fn load_ok(name: &str) -> GraphStore {
    let path = repo_root()
        .join("tests/fixtures/coverage")
        .join(format!("{name}.ttl"));
    let g = GraphStore::parse_ttl_file(&path);
    assert!(
        ok(&validate(&ttl_file_to_nt(&path))),
        "fixture {name} must pass SHACL"
    );
    g
}

// ── TBox / bnode invariants over the merged ontology ──────────────────────────

/// Twin of `test_location_superset_core`.
#[test]
fn location_superset_core() {
    let g = GraphStore::ontology();

    // 1. New classes.
    for cls in [
        "ReferenceFrame",
        "Axis",
        "SpatialCoordinates",
        "Pose",
        "Orientation",
        "FrameRealm",
        "FrameKind",
        "LocationState",
        "Trajectory",
    ] {
        assert!(
            g.has(Some(&gmeow(cls)), Some(RDF_TYPE), Some(OWL_CLASS)),
            "gmeow:{cls} must be an owl:Class"
        );
    }

    // 2. Value scaffold individuals.
    for ind in [
        "frameRealmTerrestrial",
        "frameRealmIndoor",
        "frameRealmVirtual",
        "frameRealmCelestial",
        "frameRealmMathematical",
        "frameRealmRobotic",
        "frameRealmMeasurement",
        "frameRealmCurrency",
        "frameRealmTemporal",
        "frameRealmColourspace",
        "frameRealmLinguistic",
    ] {
        assert!(
            g.has(
                Some(&gmeow(ind)),
                Some(RDF_TYPE),
                Some(&gmeow("FrameRealm"))
            ),
            "gmeow:{ind} must be a FrameRealm"
        );
    }
    for ind in [
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
    ] {
        assert!(
            g.has(Some(&gmeow(ind)), Some(RDF_TYPE), Some(&gmeow("FrameKind"))),
            "gmeow:{ind} must be a FrameKind"
        );
    }
    for ind in [
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
    ] {
        assert!(
            g.has(Some(&gmeow(ind)), Some(RDF_TYPE), Some(&gmeow("Axis"))),
            "gmeow:{ind} must be an Axis"
        );
    }
    for ind in [
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
    ] {
        assert!(
            g.has(
                Some(&gmeow(ind)),
                Some(RDF_TYPE),
                Some(&gmeow("ReferenceFrame"))
            ),
            "gmeow:{ind} must be a ReferenceFrame"
        );
    }

    // 3. New properties: domain / range / functional.
    let d = |p: &str, o: &str| assert!(g.has(Some(&gmeow(p)), Some(RDFS_DOMAIN), Some(o)));
    let r = |p: &str, o: &str| assert!(g.has(Some(&gmeow(p)), Some(RDFS_RANGE), Some(o)));
    let func = |p: &str| {
        assert!(g.has(
            Some(&gmeow(p)),
            Some(RDF_TYPE),
            Some(OWL_FUNCTIONAL_PROPERTY)
        ))
    };

    d("frameRealm", &gmeow("ReferenceFrame"));
    r("frameRealm", &gmeow("FrameRealm"));
    func("frameRealm");
    d("hasAxis", &gmeow("ReferenceFrame"));
    r("hasAxis", &gmeow("Axis"));
    d("dimensionCount", &gmeow("ReferenceFrame"));
    r("dimensionCount", XSD_NON_NEGATIVE_INTEGER);
    func("dimensionCount");
    d("frameKind", &gmeow("ReferenceFrame"));
    r("frameKind", &gmeow("FrameKind"));
    func("frameKind");
    d("requiresHost", &gmeow("ReferenceFrame"));
    r("requiresHost", XSD_BOOLEAN);
    func("requiresHost");
    d("parentFrame", &gmeow("ReferenceFrame"));
    r("parentFrame", &gmeow("ReferenceFrame"));
    func("parentFrame");
    d("transformsTo", &gmeow("ReferenceFrame"));
    r("transformsTo", &gmeow("ReferenceFrame"));
    d("frameSolver", &gmeow("ReferenceFrame"));
    r("frameSolver", RDFS_LITERAL);
    d("determinacyModel", &gmeow("ReferenceFrame"));
    r("determinacyModel", &gmeow("Determinacy"));
    func("determinacyModel");
    d("coordinateFrame", &gmeow("SpatialCoordinates"));
    r("coordinateFrame", &gmeow("ReferenceFrame"));
    func("coordinateFrame");
    assert!(g.has(
        Some(&gmeow("hasReferenceFrame")),
        Some(RDF_TYPE),
        Some(OWL_OBJECT_PROPERTY)
    ));
    assert!(g.has(
        Some(&gmeow("coordinateFrame")),
        Some(RDFS_SUB_PROPERTY_OF),
        Some(&gmeow("hasReferenceFrame"))
    ));

    // hasCoordinateMatrix domain is a blank owl:unionOf(Axis, Pose, SpatialCoords, Geocode).
    let domain_objs = g.objects_h(&Subject::Named(gmeow("hasCoordinateMatrix")), RDFS_DOMAIN);
    assert_eq!(domain_objs.len(), 1, "hasCoordinateMatrix has one domain");
    let domain =
        GraphStore::object_as_subject(&domain_objs[0]).expect("domain is a named or blank node");
    assert_eq!(
        g.value_h(&domain, RDF_TYPE),
        Some(Object::Named(OWL_CLASS.to_owned())),
        "hasCoordinateMatrix domain node must be an owl:Class"
    );
    let union_head_obj = g
        .value_h(&domain, OWL_UNION_OF)
        .expect("domain carries owl:unionOf");
    let union_head =
        GraphStore::object_as_subject(&union_head_obj).expect("unionOf head is a list node");
    let union_members = named_set(&g.rdf_list_h(&union_head));
    assert_eq!(
        union_members,
        BTreeSet::from([
            gmeow("Axis"),
            gmeow("Pose"),
            gmeow("SpatialCoordinates"),
            gmeow("Geocode"),
        ])
    );
    assert!(g.has(
        Some(&gmeow("hasCoordinateMatrix")),
        Some(RDFS_RANGE),
        Some(RDFS_LITERAL)
    ));

    // Pose / Orientation properties.
    d("hasPose", &gmeow("Entity"));
    r("hasPose", &gmeow("Pose"));
    d("hasPosePosition", &gmeow("Pose"));
    r("hasPosePosition", &gmeow("SpatialCoordinates"));
    d("hasPoseOrientation", &gmeow("Pose"));
    r("hasPoseOrientation", &gmeow("Orientation"));
    d("poseFrame", &gmeow("Pose"));
    r("poseFrame", &gmeow("ReferenceFrame"));
    assert!(g.has(
        Some(&gmeow("poseFrame")),
        Some(RDFS_SUB_PROPERTY_OF),
        Some(&gmeow("hasReferenceFrame"))
    ));
    for orient_prop in [
        "quaternionX",
        "quaternionY",
        "quaternionZ",
        "quaternionW",
        "yaw",
        "pitch",
        "roll",
        "heading",
        "bearing",
    ] {
        assert!(g.has(
            Some(&gmeow(orient_prop)),
            Some(RDF_TYPE),
            Some(OWL_DATATYPE_PROPERTY)
        ));
        assert!(g.has(
            Some(&gmeow(orient_prop)),
            Some(RDFS_DOMAIN),
            Some(&gmeow("Orientation"))
        ));
        assert!(g.has(
            Some(&gmeow(orient_prop)),
            Some(RDFS_RANGE),
            Some(XSD_DOUBLE)
        ));
    }
    d("eulerOrder", &gmeow("Orientation"));
    r("eulerOrder", XSD_STRING);

    // 4. Topology relations.
    assert!(g.has(
        Some(&gmeow("containedInLocation")),
        Some(RDF_TYPE),
        Some(OWL_TRANSITIVE_PROPERTY)
    ));
    assert!(g.has(
        Some(&gmeow("containedInPlace")),
        Some(RDFS_SUB_PROPERTY_OF),
        Some(&gmeow("containedInLocation"))
    ));
    assert!(g.has(
        Some(&gmeow("adjacentTo")),
        Some(RDF_TYPE),
        Some(OWL_SYMMETRIC_PROPERTY)
    ));
    assert!(g.has(
        Some(&gmeow("spatiallyConnectsTo")),
        Some(RDF_TYPE),
        Some(OWL_SYMMETRIC_PROPERTY)
    ));

    // 5. locatedAt property chain axiom — blank rdf:List check.
    let chain_obj = g
        .value_h(
            &Subject::Named(gmeow("locatedAt")),
            OWL_PROPERTY_CHAIN_AXIOM,
        )
        .expect("locatedAt carries a property chain axiom");
    let chain_head = GraphStore::object_as_subject(&chain_obj).expect("chain head is a list node");
    assert_eq!(
        g.rdf_list_h(&chain_head),
        vec![
            Object::Named(gmeow("locatedAt")),
            Object::Named(gmeow("containedInLocation")),
        ]
    );

    // 6. RCC-8 JEPD disjoint properties — dynamic subject sweep (may be blank).
    let disjoint_nodes = g.subjects_of_type_h(OWL_ALL_DISJOINT_PROPERTIES);
    assert!(
        !disjoint_nodes.is_empty(),
        "at least one AllDisjointProperties"
    );
    let expected: BTreeSet<String> = [
        "rcc8dc",
        "rcc8ec",
        "rcc8po",
        "rcc8tpp",
        "rcc8ntpp",
        "rcc8tppi",
        "rcc8ntppi",
        "rcc8eq",
    ]
    .into_iter()
    .map(gmeow)
    .collect();
    let found = disjoint_nodes.iter().any(|node| {
        g.value_h(node, OWL_MEMBERS)
            .and_then(|members_obj| GraphStore::object_as_subject(&members_obj))
            .is_some_and(|head| named_set(&g.rdf_list_h(&head)) == expected)
    });
    assert!(
        found,
        "an AllDisjointProperties must list the RCC-8 relations"
    );

    // 7. RCC-8 subproperties.
    assert!(g.has(
        Some(&gmeow("rcc8tpp")),
        Some(RDFS_SUB_PROPERTY_OF),
        Some(&gmeow("containedInLocation"))
    ));
    assert!(g.has(
        Some(&gmeow("rcc8ntpp")),
        Some(RDFS_SUB_PROPERTY_OF),
        Some(&gmeow("containedInLocation"))
    ));
}

/// Twin of `test_no_unsafe_motion_property_chains`.
#[test]
fn no_unsafe_motion_property_chains() {
    let g = GraphStore::ontology();
    for prop in [
        "stateOf",
        "stateDuringInterval",
        "stateAtInstant",
        "stateHasVelocity",
        "stateHasAngularVelocity",
        "stateReferenceFrame",
        "trajectoryOf",
        "hasTrajectorySample",
        "trajectoryReferenceFrame",
    ] {
        assert!(
            !g.has(Some(&gmeow(prop)), Some(OWL_PROPERTY_CHAIN_AXIOM), None),
            "{prop} must not carry a property chain axiom"
        );
    }
}

/// Twin of `test_has_coordinate_matrix_includes_geocode`.
#[test]
fn has_coordinate_matrix_includes_geocode() {
    let g = GraphStore::ontology();
    let domain_objs = g.objects_h(&Subject::Named(gmeow("hasCoordinateMatrix")), RDFS_DOMAIN);
    assert_eq!(domain_objs.len(), 1);
    let domain = GraphStore::object_as_subject(&domain_objs[0]).expect("domain node");
    assert_eq!(
        g.value_h(&domain, RDF_TYPE),
        Some(Object::Named(OWL_CLASS.to_owned()))
    );
    let union_head_obj = g.value_h(&domain, OWL_UNION_OF).expect("owl:unionOf head");
    let union_head = GraphStore::object_as_subject(&union_head_obj).expect("list node");
    assert!(named_set(&g.rdf_list_h(&union_head)).contains(&gmeow("Geocode")));
}

/// Twin of `test_has_coordinates_property_chain`.
#[test]
fn has_coordinates_property_chain() {
    let g = GraphStore::ontology();
    let chain_obj = g
        .value_h(
            &Subject::Named(gmeow("hasCoordinates")),
            OWL_PROPERTY_CHAIN_AXIOM,
        )
        .expect("hasCoordinates carries a property chain axiom");
    let head = GraphStore::object_as_subject(&chain_obj).expect("chain head");
    assert_eq!(
        g.rdf_list_h(&head),
        vec![
            Object::Named(gmeow("hasCoordinateObservation")),
            Object::Named(gmeow("coordinateResult")),
        ]
    );
}

/// Twin of `test_has_geometry_property_chain`.
#[test]
fn has_geometry_property_chain() {
    let g = GraphStore::ontology();
    let chain_obj = g
        .value_h(
            &Subject::Named(gmeow("hasGeometry")),
            OWL_PROPERTY_CHAIN_AXIOM,
        )
        .expect("hasGeometry carries a property chain axiom");
    let head = GraphStore::object_as_subject(&chain_obj).expect("chain head");
    assert_eq!(
        g.rdf_list_h(&head),
        vec![
            Object::Named(gmeow("hasCoordinateObservation")),
            Object::Named(gmeow("geometryResult")),
        ]
    );
}

/// Twin of `test_place_type_parcel_exists`.
#[test]
fn place_type_parcel_exists() {
    let g = GraphStore::ontology();
    assert!(g.has(
        Some(&gmeow("placeTypeParcel")),
        Some(RDF_TYPE),
        Some(&gmeow("PlaceType"))
    ));
    let labels = g.objects_h(&Subject::Named(gmeow("placeTypeSite")), RDFS_LABEL);
    assert!(!labels.is_empty(), "placeTypeSite must have a label");
    for label in &labels {
        if let Object::Literal { value, .. } = label {
            assert!(
                !value.to_lowercase().contains("parcel"),
                "placeTypeSite label must not mention parcel: {value}"
            );
        }
    }
}

// ── Standpoint coexistence — contested sovereignty / place names ──────────────

/// Twin of `test_contested_sovereignty_coexists`.
#[test]
fn contested_sovereignty_coexists() {
    let g = load_ok("places-contested");
    let containers = g.objects(&ex("disputedPlace"), &gmeow("containedInPlace"));
    assert!(containers.contains(&ex("polityA")));
    assert!(containers.contains(&ex("polityB")));
}

/// Twin of `test_contested_place_names_coexist`.
#[test]
fn contested_place_names_coexist() {
    let g = load_ok("places-contested");
    let names = g.objects(&ex("disputedPlace"), &gmeow("hasPlaceName"));
    assert!(names.contains(&ex("nameEndonym")));
    assert!(names.contains(&ex("nameExonym")));
}

/// Twin of `test_superseded_historical_name_suppressed`.
#[test]
fn superseded_historical_name_suppressed() {
    let g = load_ok("places-contested");
    assert!(g.has_literal(
        &ex("nameHistorical"),
        &gmeow("displayable"),
        "false",
        XSD_BOOLEAN
    ));
}

/// Twin of `test_contested_jurisdiction_tenures_coexist`.
#[test]
fn contested_jurisdiction_tenures_coexist() {
    let g = load_ok("places-contested");
    let tenures = g.subjects_of_type(&gmeow("JurisdictionTenure"));
    assert!(tenures.len() >= 2, "at least two JurisdictionTenures");
    let mut polities: BTreeSet<String> = BTreeSet::new();
    for t in &tenures {
        polities.extend(g.objects(t.as_str(), &gmeow("jurisdictionPolity")));
    }
    assert!(polities.len() >= 2, "at least two distinct polity claims");
}

/// Twin of `test_containment_tenure_records_border_change`.
#[test]
fn containment_tenure_records_border_change() {
    let g = load_ok("places-contested");
    let tenures = g.subjects_of_type(&gmeow("ContainmentTenure"));
    assert!(tenures.len() >= 2, "at least two ContainmentTenure records");
    let mut claims: BTreeSet<(String, String)> = BTreeSet::new();
    for t in &tenures {
        if g.has(
            Some(t.as_str()),
            Some(&gmeow("containmentChild")),
            Some(&ex("disputedPlace")),
        ) {
            let parent = g.objects(t.as_str(), &gmeow("containmentParent"));
            let interval = g.objects(t.as_str(), &gmeow("duringInterval"));
            if let (Some(p), Some(i)) = (parent.into_iter().next(), interval.into_iter().next()) {
                claims.insert((p, i));
            }
        }
    }
    assert!(claims.contains(&(ex("polityA"), ex("interval1920_1954"))));
    assert!(claims.contains(&(ex("polityB"), ex("interval1954_present"))));
}

/// Twin of `test_geometry_has_type_and_geojson`.
#[test]
fn geometry_has_type_and_geojson() {
    let g = load_ok("places-contested");
    let geoms = g.objects(&ex("disputedPlace"), &gmeow("hasGeometry"));
    assert!(!geoms.is_empty(), "at least one geometry on disputedPlace");
    for geom in &geoms {
        assert!(
            g.has(Some(geom.as_str()), Some(&gmeow("geometryType")), None),
            "geometry must have a geometryType"
        );
        assert!(
            g.has(Some(geom.as_str()), Some(&gmeow("asGeoJSON")), None),
            "geometry must have an asGeoJSON serialization"
        );
    }
}

// ── RegulatoryOverlay — legal / regulatory overlays ───────────────────────────

/// Twin of `test_contested_regulatory_overlays_coexist`.
#[test]
fn contested_regulatory_overlays_coexist() {
    let g = load_ok("places-regulatory");
    let overlays = g.subjects_of_type(&gmeow("RegulatoryOverlay"));
    assert!(overlays.len() >= 2, "at least two RegulatoryOverlays");
    let mut authorities: BTreeSet<String> = BTreeSet::new();
    for o in &overlays {
        authorities.extend(g.objects(o.as_str(), &gmeow("overlayAuthority")));
    }
    assert!(authorities.len() >= 2, "at least two distinct authorities");
}

/// Twin of `test_regulatory_overlay_linked_to_rights_statement`.
#[test]
fn regulatory_overlay_linked_to_rights_statement() {
    let g = load_ok("places-regulatory");
    let overlays = g.subjects_of_type(&gmeow("RegulatoryOverlay"));
    assert!(!overlays.is_empty(), "at least one RegulatoryOverlay");
    let mut regs: BTreeSet<String> = BTreeSet::new();
    for o in &overlays {
        regs.extend(g.objects(o.as_str(), &gmeow("overlayRegulation")));
    }
    assert!(
        !regs.is_empty(),
        "at least one overlay linked to a rights statement"
    );
    for reg in &regs {
        assert!(g.has(
            Some(reg.as_str()),
            Some(RDF_TYPE),
            Some(&gmeow("RightsStatement"))
        ));
    }
}

/// Twin of `test_regulatory_overlay_3d_bounds`.
#[test]
fn regulatory_overlay_3d_bounds() {
    let g = load_ok("places-regulatory");
    let overlays = g.subjects_of_type(&gmeow("RegulatoryOverlay"));
    let mut found_bounds = false;
    for o in &overlays {
        let lower = g.objects(o.as_str(), &gmeow("overlayLowerBound"));
        let upper = g.objects(o.as_str(), &gmeow("overlayUpperBound"));
        if let (Some(l), Some(u)) = (lower.into_iter().next(), upper.into_iter().next()) {
            found_bounds = true;
            for bound in [&l, &u] {
                assert!(
                    g.has(Some(bound.as_str()), Some(&math("quantityValue")), None),
                    "3D bound must have quantityValue"
                );
                assert!(
                    g.has(Some(bound.as_str()), Some(&gmeow("unit")), None),
                    "3D bound must have unit"
                );
                assert!(
                    g.has(
                        Some(bound.as_str()),
                        Some(&gmeow("hasReferenceFrame")),
                        None
                    ),
                    "3D bound must have hasReferenceFrame"
                );
            }
        }
    }
    assert!(found_bounds, "at least one overlay with both bounds");
}

/// Twin of `test_contested_eez_coexistence`.
#[test]
fn contested_eez_coexistence() {
    let g = load_ok("places-maritime");
    let overlays = g.subjects_of_type(&gmeow("RegulatoryOverlay"));
    assert!(overlays.len() >= 2, "at least two EEZ overlays");
    let mut authorities: BTreeSet<String> = BTreeSet::new();
    for o in &overlays {
        authorities.extend(g.objects(o.as_str(), &gmeow("overlayAuthority")));
    }
    assert!(authorities.len() >= 2, "at least two distinct authorities");
    let mut found_depth_frame = false;
    for o in &overlays {
        let Some(lower) = g
            .objects(o.as_str(), &gmeow("overlayLowerBound"))
            .into_iter()
            .next()
        else {
            continue;
        };
        if g.objects(&lower, &gmeow("hasReferenceFrame"))
            .contains(&gmeow("referenceFrameDepthBelowSeaLevel"))
        {
            found_depth_frame = true;
        }
    }
    assert!(
        found_depth_frame,
        "at least one depth bound using the maritime frame"
    );
}

// ── Motion — streaming ────────────────────────────────────────────────────────

/// Twin of `test_location_stream_to_trajectory_derivation`.
#[test]
fn location_stream_to_trajectory_derivation() {
    let g = load_ok("places-streaming");
    assert!(g.has(
        Some(&ex("trajectory1")),
        Some(&gmeow("wasDerivedFrom")),
        Some(&ex("stream1"))
    ));
    let samples = g.objects(&ex("stream1"), &gmeow("streamSample"));
    assert_eq!(samples.len(), 3, "expected 3 stream samples");
    let traj_samples = g.objects(&ex("trajectory1"), &gmeow("hasTrajectorySample"));
    assert_eq!(
        samples, traj_samples,
        "stream and trajectory samples must match"
    );
}

/// Twin of `test_stream_and_trajectory_coexist`.
#[test]
fn stream_and_trajectory_coexist() {
    let g = load_ok("places-streaming");
    assert!(
        g.subjects_of_type(&gmeow("Stream")).len() >= 2,
        "at least two co-existing streams"
    );
    assert!(
        g.subjects_of_type(&gmeow("Trajectory")).len() >= 2,
        "at least two co-existing trajectories"
    );
}

// ── Capacity / occupancy / utilization ────────────────────────────────────────

/// Twin of `test_contested_capacity_claims_coexist`.
#[test]
fn contested_capacity_claims_coexist() {
    let g = load_ok("places-capacity");
    let caps = g.subjects(&gmeow("capacityOf"), &ex("venue"));
    assert!(caps.contains(&ex("capFireCode")));
    assert!(caps.contains(&ex("capVenueClaim")));
}

/// Twin of `test_superseded_capacity_suppressed`.
#[test]
fn superseded_capacity_suppressed() {
    let g = load_ok("places-capacity");
    assert!(g.has_literal(&ex("capOld"), &gmeow("displayable"), "false", XSD_BOOLEAN));
}

/// Twin of `test_occupancy_with_unit_asserted`.
#[test]
fn occupancy_with_unit_asserted() {
    let g = load_ok("places-capacity");
    let occs = g.subjects(&gmeow("occupancyOf"), &ex("venue"));
    assert_eq!(occs.len(), 1, "venue must have exactly one occupancy");
    let occ = occs.into_iter().next().unwrap();
    let sq = g
        .objects(&occ, &gmeow("observationResult"))
        .into_iter()
        .next()
        .expect("occupancy must have an observationResult");
    assert!(
        g.has_literal(&sq, &math("quantityValue"), "412", XSD_DECIMAL),
        "occupancy quantityValue must be 412"
    );
}

/// Twin of `test_storage_capacity_in_bytes`.
#[test]
fn storage_capacity_in_bytes() {
    let g = load_ok("places-capacity");
    let caps = g.subjects(&gmeow("capacityOf"), &ex("storage"));
    assert_eq!(caps.len(), 1, "storage must have exactly one capacity");
    let cap = caps.into_iter().next().unwrap();
    let sq = g
        .objects(&cap, &gmeow("observationResult"))
        .into_iter()
        .next()
        .expect("capacity must have an observationResult");
    assert!(
        g.has_literal(&sq, &math("quantityValue"), "1099511627776", XSD_DECIMAL),
        "storage capacity must be 1 TiB in bytes"
    );
    assert!(
        g.has(Some(&sq), Some(&gmeow("unit")), Some(QUDT_BYTE)),
        "storage capacity unit must be qudt:BYTE"
    );
}

// ── Virtual location type + network address space ─────────────────────────────

/// Twin of `test_virtual_location_types_coexist`.
#[test]
fn virtual_location_types_coexist() {
    let g = load_ok("places-virtual");
    let types = g.objects(&ex("confRoom"), &gmeow("virtualLocationType"));
    assert!(types.contains(&gmeow("virtualLocationTypeVideoConference")));
    assert!(types.contains(&gmeow("virtualLocationTypeVirtualEventSpace")));
}

/// Twin of `test_network_addresses_in_different_frames_coexist`.
#[test]
fn network_addresses_in_different_frames_coexist() {
    let g = load_ok("places-virtual");
    let addrs = g.objects(&ex("website"), &gmeow("hasNetworkAddress"));
    assert!(addrs.len() >= 3, "expected at least 3 network addresses");
    let mut frames: BTreeSet<String> = BTreeSet::new();
    for a in &addrs {
        frames.extend(g.objects(a.as_str(), &gmeow("networkAddressFrame")));
    }
    assert!(frames.contains(&gmeow("referenceFrameIPv4")));
    assert!(frames.contains(&gmeow("referenceFrameDNS")));
    assert!(frames.contains(&gmeow("referenceFrameURL")));
}

/// Twin of `test_superseded_network_address_suppressed`.
#[test]
fn superseded_network_address_suppressed() {
    let g = load_ok("places-virtual");
    assert!(g.has_literal(
        &ex("addrOldURL"),
        &gmeow("displayable"),
        "false",
        XSD_BOOLEAN
    ));
}

/// Twin of `test_contested_dns_names_coexist`.
#[test]
fn contested_dns_names_coexist() {
    let g = load_ok("places-virtual");
    let addrs = g.objects(&ex("service"), &gmeow("hasNetworkAddress"));
    assert!(addrs.contains(&ex("addrDNSCorpA")));
    assert!(addrs.contains(&ex("addrDNSCorpB")));

    let mut values: BTreeSet<String> = BTreeSet::new();
    for a in &addrs {
        for obj in g.objects_h(&Subject::Named(a.clone()), &gmeow("networkAddressValue")) {
            if let Object::Literal { value, .. } = obj {
                values.insert(value);
            }
        }
    }
    assert!(values.contains("service-corp-a.example"));
    assert!(values.contains("service-corp-b.example"));

    // Standpoint annotations on reified axioms (P9).
    let axioms = g.subjects_of_type(OWL_AXIOM);
    assert!(axioms.len() >= 2, "at least two reified standpoint axioms");
    let has_addr = gmeow("hasNetworkAddress");
    let count_for = |target: &str, standpoint: &str| -> usize {
        axioms
            .iter()
            .filter(|ax| {
                g.has(
                    Some(ax.as_str()),
                    Some(OWL_ANNOTATED_SOURCE),
                    Some(&ex("service")),
                ) && g.has(
                    Some(ax.as_str()),
                    Some(OWL_ANNOTATED_PROPERTY),
                    Some(&has_addr),
                ) && g.has(Some(ax.as_str()), Some(OWL_ANNOTATED_TARGET), Some(target))
                    && g.has(
                        Some(ax.as_str()),
                        Some(&gmeow("accordingTo")),
                        Some(standpoint),
                    )
            })
            .count()
    };
    assert_eq!(
        count_for(&ex("addrDNSCorpA"), &ex("standpoint-corp-a")),
        1,
        "exactly one corp-a claim"
    );
    assert_eq!(
        count_for(&ex("addrDNSCorpB"), &ex("standpoint-corp-b")),
        1,
        "exactly one corp-b claim"
    );
}

// ── Biological sequence ───────────────────────────────────────────────────────

/// Twin of `test_biological_standpoint_coordinate_claims_coexist`.
#[test]
fn biological_standpoint_coordinate_claims_coexist() {
    let g = load_ok("places-biological");
    let coords = g.objects(&ex("geneBRCA1Alt"), &gmeow("hasSequenceCoordinates"));
    assert!(
        coords.len() >= 2,
        "at least two co-existing coordinate claims"
    );
}

// ── Geocoding frames ──────────────────────────────────────────────────────────

/// Twin of `test_geocode_shape_invalid_no_code` — a Geocode with no code value
/// (only a frame) fails SHACL (`sh:xone` exactly-one-code).
#[test]
fn geocode_shape_invalid_no_code() {
    Case::inline(
        "\
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix ex:    <https://blackcatinformatics.ca/gmeow/examples/places/> .

ex:placeWithPlusCode a gmeow:Place ; gmeow:hasGeocode ex:pc1 .
ex:pc1 a gmeow:Geocode ; gmeow:geocodeFrame gmeow:referenceFramePlusCode .
",
    )
    .fails()
    .run();
}

/// Twin of `test_geocode_shape_invalid_two_codes` — a Geocode with two code
/// values fails SHACL.
#[test]
fn geocode_shape_invalid_two_codes() {
    Case::inline(
        "\
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix ex:    <https://blackcatinformatics.ca/gmeow/examples/places/> .

ex:placeWithPlusCode a gmeow:Place ; gmeow:hasGeocode ex:pc1 .
ex:pc1 a gmeow:Geocode ;
    gmeow:geocodeFrame gmeow:referenceFramePlusCode ;
    gmeow:plusCode \"9F4W9C8C+W4\" ;
    gmeow:geohash \"u4pruydqqvj\" .
",
    )
    .fails()
    .run();
}

// ── SpatialMeasurement / CoordinateObservation ────────────────────────────────

/// Twin of `test_coordinate_observations_coexist`.
#[test]
fn coordinate_observations_coexist() {
    let g = load_ok("coordinate-observations");
    let observations = g.subjects(&gmeow("coordinateObservationOf"), &ex("surveyedPlace"));
    assert!(observations.contains(&ex("gpsObservation")));
    assert!(observations.contains(&ex("lidarObservation")));
}

/// Twin of `test_superseded_coordinate_observation_suppressed`.
#[test]
fn superseded_coordinate_observation_suppressed() {
    let g = load_ok("coordinate-observations");
    assert!(g.has_literal(
        &ex("oldObservation"),
        &gmeow("displayable"),
        "false",
        XSD_BOOLEAN
    ));
}

// ── Cadastral / land administration ─────────────────────────────────────

/// Twin of `test_land_tenure_instance_structure`.
#[test]
fn land_tenure_instance_structure() {
    let g = load_ok("places-cadastral");
    let tenures = g.subjects_of_type(&gmeow("LandTenure"));
    assert!(!tenures.is_empty(), "at least one LandTenure");
    for t in &tenures {
        for pred in ["tenurePlace", "tenureParty", "tenureType", "duringInterval"] {
            assert!(
                g.has(Some(t.as_str()), Some(&gmeow(pred)), None),
                "LandTenure must have {pred}"
            );
        }
    }
}

/// Twin of `test_cadastral_reference_instance_structure`.
#[test]
fn cadastral_reference_instance_structure() {
    let g = load_ok("places-cadastral");
    let refs = g.subjects_of_type(&gmeow("CadastralReference"));
    assert!(!refs.is_empty(), "at least one CadastralReference");
    for r in &refs {
        for pred in [
            "referenceValue",
            "referenceType",
            "referenceAuthority",
            "referenceJurisdiction",
        ] {
            assert!(
                g.has(Some(r.as_str()), Some(&gmeow(pred)), None),
                "CadastralReference must have {pred}"
            );
        }
    }
}

/// Twin of `test_contested_land_tenures_coexist`.
#[test]
fn contested_land_tenures_coexist() {
    let g = load_ok("places-cadastral");
    let tenures = g.subjects_of_type(&gmeow("LandTenure"));
    let contested: Vec<String> = tenures
        .iter()
        .filter(|t| {
            g.has(
                Some(t.as_str()),
                Some(&gmeow("tenureType")),
                Some(&gmeow("tenureTypeOwnership")),
            )
        })
        .cloned()
        .collect();
    assert!(contested.len() >= 2, "at least two ownership claims");
    let mut parties: BTreeSet<String> = BTreeSet::new();
    for t in &contested {
        parties.extend(g.objects(t.as_str(), &gmeow("tenureParty")));
    }
    assert!(parties.len() >= 2, "at least two distinct party claims");
}

/// Twin of `test_lapsed_tenure_suppressed_not_deleted`.
#[test]
fn lapsed_tenure_suppressed_not_deleted() {
    let g = load_ok("places-cadastral");
    assert!(g.has(
        Some(&ex("lapsedTenure")),
        Some(RDF_TYPE),
        Some(&gmeow("LandTenure"))
    ));
    assert!(g.has_literal(
        &ex("lapsedTenure"),
        &gmeow("displayable"),
        "false",
        XSD_BOOLEAN
    ));
}

/// Twin of `test_cadastral_reference_multiple_types_coexist`.
#[test]
fn cadastral_reference_multiple_types_coexist() {
    let g = load_ok("places-cadastral");
    let types = g.objects(&ex("refMulti"), &gmeow("referenceType"));
    assert!(types.len() >= 2, "at least two co-existing reference types");
    assert!(types.contains(&gmeow("referenceTypeParcelId")));
    assert!(types.contains(&gmeow("referenceTypeTitle")));
}
