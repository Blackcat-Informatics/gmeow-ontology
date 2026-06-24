// SPDX-License-Identifier: AGPL-3.0-only

//! Conformance twins migrated from tests/test_images.py (#867)
//!
//! Each test builds an inline Turtle graph containing the triples that the
//! Python test assembled via `g.add(...)`, converts to N-Triples, and
//! validates against the whole shapes corpus.
//!
//! Mode: all tests use `validate(&nt)` (fixture-only / inline graph) — every
//! Python source used `run_shacl(g)` with an inline `Graph()`, not
//! `_graph()` / `load_merged_graph()`.
//!
//! Retained in Python (not migrated):
//!   - `test_depicts_is_subproperty_of_is_about`: calls `_graph()`, TBox membership.
//!   - `test_depicted_in_is_inverse_of_depicts`: calls `_graph()`, TBox membership.
//!   - `test_pixel_dimensions_on_media_object`: calls `_graph()`, TBox membership.
//!   - `test_image_orientation_on_media_object`: calls `_graph()`, TBox membership.
//!   - `test_capture_metadata_on_media_object`: calls `_graph()`, TBox membership.
//!   - `test_image_event_types_exist`: calls `_graph()`, subject in events module.
//!   - `test_colourspace_property_exists`: calls `_graph()`, cross-slice TBox check
//!     (subject in documents/module.ttl) — explicitly retained per docstring.

mod conformance_support;
use conformance_support::*;

/// Turtle prefix block shared by all images tests.
const PREFIXES: &str = "\
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix ex:    <https://example.org/test/> .
@prefix rdf:   <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .
@prefix rdfs:  <http://www.w3.org/2000/01/rdf-schema#> .
@prefix xsd:   <http://www.w3.org/2001/XMLSchema#> .
";

/// Inline expansion of `_add_media_object(g, img)` — creates a minimal
/// MediaObject resource.
fn media_object_ttl(img: &str) -> String {
    format!(
        "\
{img} a gmeow:MediaObject .
{img} rdfs:label \"Test Image\" .
"
    )
}

// ── DepictionUsage tests ──────────────────────────────────────────────────────

/// `test_depiction_usage_shacl_passes` — a well-formed DepictionUsage passes SHACL.
#[test]
fn depiction_usage_shacl_passes() {
    let ttl = format!(
        "{PREFIXES}\
ex:usage a gmeow:DepictionUsage .
ex:alice a gmeow:Entity .
ex:usage gmeow:depictionSubject ex:alice .
ex:usage gmeow:depictionImage ex:img .
ex:usage gmeow:depictionContext gmeow:depictionContextPortrait .
gmeow:depictionContextPortrait a gmeow:DepictionContext .
{media_object}",
        media_object = media_object_ttl("ex:img"),
    );
    let nt = ttl_str_to_nt(&ttl);
    let report = validate(&nt);
    assert!(
        ok(&report),
        "well-formed DepictionUsage must pass SHACL; violations: {:?}",
        violations(&report)
    );
}

/// `test_depiction_usage_missing_image_fails_shacl` — a DepictionUsage without
/// a depictionImage violates SHACL.
#[test]
fn depiction_usage_missing_image_fails_shacl() {
    let ttl = format!(
        "{PREFIXES}\
ex:usage a gmeow:DepictionUsage .
ex:alice a gmeow:Entity .
ex:usage gmeow:depictionSubject ex:alice .
ex:usage gmeow:depictionContext gmeow:depictionContextPortrait .
gmeow:depictionContextPortrait a gmeow:DepictionContext .
"
    );
    let nt = ttl_str_to_nt(&ttl);
    let report = validate(&nt);
    assert!(
        !ok(&report),
        "DepictionUsage without depictionImage must fail SHACL"
    );
    assert!(
        violations(&report)
            .iter()
            .any(|e| e.contains("depictionImage")),
        "expected depictionImage in violations; got: {:?}",
        violations(&report)
    );
}

// ── ImageRegion tests ─────────────────────────────────────────────────────────

/// `test_image_region_shacl_passes` — a well-formed ImageRegion passes SHACL.
#[test]
fn image_region_shacl_passes() {
    let ttl = format!(
        "{PREFIXES}\
ex:region a gmeow:ImageRegion .
ex:region rdfs:label \"Test Region\" .
ex:region gmeow:regionOf ex:img .
ex:region gmeow:regionSelector ex:sel .
ex:sel a gmeow:RegionSelector .
ex:sel gmeow:selectorType gmeow:selectorTypePixelRectangle .
ex:sel gmeow:selectorValue \"10,20,100,200\" .
gmeow:selectorTypePixelRectangle a gmeow:SelectorType .
{media_object}",
        media_object = media_object_ttl("ex:img"),
    );
    let nt = ttl_str_to_nt(&ttl);
    let report = validate(&nt);
    assert!(
        ok(&report),
        "well-formed ImageRegion must pass SHACL; violations: {:?}",
        violations(&report)
    );
}

/// `test_image_region_missing_selector_fails_shacl` — an ImageRegion without a
/// regionSelector violates SHACL.
#[test]
fn image_region_missing_selector_fails_shacl() {
    let ttl = format!(
        "{PREFIXES}\
ex:region a gmeow:ImageRegion .
ex:region rdfs:label \"Orphan Region\" .
ex:region gmeow:regionOf ex:img .
{media_object}",
        media_object = media_object_ttl("ex:img"),
    );
    let nt = ttl_str_to_nt(&ttl);
    let report = validate(&nt);
    assert!(
        !ok(&report),
        "ImageRegion without regionSelector must fail SHACL"
    );
    assert!(
        violations(&report)
            .iter()
            .any(|e| e.contains("regionSelector")),
        "expected regionSelector in violations; got: {:?}",
        violations(&report)
    );
}

// ── RegionSelector tests ──────────────────────────────────────────────────────

/// `test_region_selector_missing_value_fails_shacl` — a RegionSelector without
/// selectorValue violates SHACL.
#[test]
fn region_selector_missing_value_fails_shacl() {
    let ttl = format!(
        "{PREFIXES}\
ex:sel a gmeow:RegionSelector .
ex:sel gmeow:selectorType gmeow:selectorTypePixelRectangle .
gmeow:selectorTypePixelRectangle a gmeow:SelectorType .
"
    );
    let nt = ttl_str_to_nt(&ttl);
    let report = validate(&nt);
    assert!(
        !ok(&report),
        "RegionSelector without selectorValue must fail SHACL"
    );
    assert!(
        violations(&report)
            .iter()
            .any(|e| e.contains("selectorValue")),
        "expected selectorValue in violations; got: {:?}",
        violations(&report)
    );
}

// ── SceneGraphEdge tests ──────────────────────────────────────────────────────

/// Helper: two valid ImageRegions referencing the same image.
fn two_regions_ttl() -> String {
    format!(
        "\
ex:region1 a gmeow:ImageRegion .
ex:region1 gmeow:regionOf ex:img .
ex:region1 gmeow:regionSelector ex:sel1 .
ex:sel1 a gmeow:RegionSelector .
ex:sel1 gmeow:selectorType gmeow:selectorTypePixelRectangle .
ex:sel1 gmeow:selectorValue \"0,0,50,50\" .
ex:region2 a gmeow:ImageRegion .
ex:region2 gmeow:regionOf ex:img .
ex:region2 gmeow:regionSelector ex:sel2 .
ex:sel2 a gmeow:RegionSelector .
ex:sel2 gmeow:selectorType gmeow:selectorTypePixelRectangle .
ex:sel2 gmeow:selectorValue \"60,0,50,50\" .
gmeow:selectorTypePixelRectangle a gmeow:SelectorType .
{media_object}",
        media_object = media_object_ttl("ex:img"),
    )
}

/// `test_scene_graph_edge_shacl_passes` — a well-formed SceneGraphEdge passes SHACL.
#[test]
fn scene_graph_edge_shacl_passes() {
    let ttl = format!(
        "{PREFIXES}\
ex:edge a gmeow:SceneGraphEdge .
ex:edge gmeow:sceneSubject ex:region1 .
ex:edge gmeow:sceneObject ex:region2 .
ex:edge gmeow:sceneRelation gmeow:sceneRelationLeftOf .
ex:edge gmeow:sceneConfidence \"0.95\"^^xsd:decimal .
gmeow:sceneRelationLeftOf a gmeow:SceneRelationType .
{regions}",
        regions = two_regions_ttl(),
    );
    let nt = ttl_str_to_nt(&ttl);
    let report = validate(&nt);
    assert!(
        ok(&report),
        "well-formed SceneGraphEdge must pass SHACL; violations: {:?}",
        violations(&report)
    );
}

/// `test_scene_graph_edge_missing_relation_fails_shacl` — a SceneGraphEdge
/// without a sceneRelation violates SHACL.
#[test]
fn scene_graph_edge_missing_relation_fails_shacl() {
    let ttl = format!(
        "{PREFIXES}\
ex:edge a gmeow:SceneGraphEdge .
ex:edge gmeow:sceneSubject ex:region1 .
ex:edge gmeow:sceneObject ex:region2 .
{regions}",
        regions = two_regions_ttl(),
    );
    let nt = ttl_str_to_nt(&ttl);
    let report = validate(&nt);
    assert!(
        !ok(&report),
        "SceneGraphEdge without sceneRelation must fail SHACL"
    );
    assert!(
        violations(&report)
            .iter()
            .any(|e| e.contains("sceneRelation")),
        "expected sceneRelation in violations; got: {:?}",
        violations(&report)
    );
}

/// `test_scene_graph_edge_confidence_out_of_range_fails_shacl` — a
/// SceneGraphEdge with sceneConfidence > 1.0 violates SHACL.
#[test]
fn scene_graph_edge_confidence_out_of_range_fails_shacl() {
    let ttl = format!(
        "{PREFIXES}\
ex:edge a gmeow:SceneGraphEdge .
ex:edge gmeow:sceneSubject ex:region1 .
ex:edge gmeow:sceneObject ex:region2 .
ex:edge gmeow:sceneRelation gmeow:sceneRelationLeftOf .
ex:edge gmeow:sceneConfidence \"1.5\"^^xsd:decimal .
gmeow:sceneRelationLeftOf a gmeow:SceneRelationType .
{regions}",
        regions = two_regions_ttl(),
    );
    let nt = ttl_str_to_nt(&ttl);
    let report = validate(&nt);
    assert!(
        !ok(&report),
        "SceneGraphEdge with sceneConfidence > 1.0 must fail SHACL"
    );
    assert!(
        violations(&report)
            .iter()
            .any(|e| e.contains("sceneConfidence")),
        "expected sceneConfidence in violations; got: {:?}",
        violations(&report)
    );
}

// ── DepictionUsage cardinality test ──────────────────────────────────────────

/// `test_depiction_usage_multiple_subjects_fails_shacl` — a DepictionUsage
/// with more than one depictionSubject violates SHACL.
#[test]
fn depiction_usage_multiple_subjects_fails_shacl() {
    let ttl = format!(
        "{PREFIXES}\
ex:usage a gmeow:DepictionUsage .
ex:alice a gmeow:Entity .
ex:bob a gmeow:Entity .
ex:usage gmeow:depictionSubject ex:alice .
ex:usage gmeow:depictionSubject ex:bob .
ex:usage gmeow:depictionImage ex:img .
ex:usage gmeow:depictionContext gmeow:depictionContextPortrait .
gmeow:depictionContextPortrait a gmeow:DepictionContext .
{media_object}",
        media_object = media_object_ttl("ex:img"),
    );
    let nt = ttl_str_to_nt(&ttl);
    let report = validate(&nt);
    assert!(
        !ok(&report),
        "DepictionUsage with two depictionSubjects must fail SHACL"
    );
    assert!(
        violations(&report)
            .iter()
            .any(|e| e.contains("depictionSubject")),
        "expected depictionSubject in violations; got: {:?}",
        violations(&report)
    );
}

// ── Colourspace cross-slice SHACL tests ──────────────────────────────────────

/// `test_media_object_colourspace_shacl_passes` — a MediaObject with a
/// colourspace passes SHACL.
///
/// Note: `test_colourspace_property_exists` (the TBox membership check,
/// subject in documents/module.ttl) is retained in Python — this test covers
/// the SHACL well-formedness side only.
#[test]
fn media_object_colourspace_shacl_passes() {
    let ttl = format!(
        "{PREFIXES}\
ex:img a gmeow:MediaObject .
ex:img rdfs:label \"Test Image\" .
ex:img gmeow:colourspace ex:srgbFrame .
ex:srgbFrame a gmeow:ReferenceFrame .
ex:srgbFrame gmeow:frameRealm gmeow:frameRealmColourspace .
ex:srgbFrame gmeow:hasAxis ex:axisRed .
ex:srgbFrame gmeow:hasAxis ex:axisGreen .
ex:srgbFrame gmeow:hasAxis ex:axisBlue .
ex:srgbFrame gmeow:dimensionCount \"3\"^^xsd:nonNegativeInteger .
ex:srgbFrame gmeow:frameKind gmeow:frameKindCartesian .
ex:srgbFrame gmeow:requiresHost false .
ex:srgbFrame gmeow:determinacyModel gmeow:determinacyCrisp .
gmeow:frameRealmColourspace a gmeow:FrameRealm .
ex:axisRed a gmeow:Axis .
ex:axisGreen a gmeow:Axis .
ex:axisBlue a gmeow:Axis .
gmeow:frameKindCartesian a gmeow:FrameKind .
gmeow:determinacyCrisp a gmeow:Determinacy .
"
    );
    let nt = ttl_str_to_nt(&ttl);
    let report = validate(&nt);
    assert!(
        ok(&report),
        "MediaObject with colourspace must pass SHACL; violations: {:?}",
        violations(&report)
    );
}

/// `test_media_object_missing_colourspace_warns_shacl` — a MediaObject without
/// a colourspace triggers a SHACL warning (not a violation).
#[test]
fn media_object_missing_colourspace_warns_shacl() {
    let ttl = format!(
        "{PREFIXES}\
ex:img a gmeow:MediaObject .
ex:img rdfs:label \"Test Image\" .
"
    );
    let nt = ttl_str_to_nt(&ttl);
    let report = validate(&nt);
    // Warnings do not cause ok() to return false — only violations do.
    assert!(
        ok(&report),
        "warning-only graph must pass; violations: {:?}",
        violations(&report)
    );
    assert!(
        warnings(&report)
            .iter()
            .any(|w| w.to_lowercase().contains("colourspace")),
        "expected colourspace warning; got warnings: {:?}",
        warnings(&report)
    );
}
