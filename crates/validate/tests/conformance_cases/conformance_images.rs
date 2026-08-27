// SPDX-License-Identifier: AGPL-3.0-only

//! Conformance twins migrated from tests/test_images.py
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

use crate::conformance_support::*;
use gmeow_test_batch_macros::batch_cases;

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

// ── DepictionUsage tests ──────────────────────────────────────────────────────

#[batch_cases]
#[case::depiction_usage_shacl_passes(Case::inline(format!(
    "{PREFIXES}\
ex:usage a gmeow:DepictionUsage .
ex:alice a gmeow:Entity .
ex:usage gmeow:depictionSubject ex:alice .
ex:usage gmeow:depictionImage ex:img .
ex:usage gmeow:depictionContext gmeow:depictionContextPortrait .
gmeow:depictionContextPortrait a gmeow:DepictionContext .
{media_object}",
    media_object = media_object_ttl("ex:img"),
)))]
#[case::depiction_usage_missing_image_fails_shacl(
    Case::inline(format!(
        "{PREFIXES}\
ex:usage a gmeow:DepictionUsage .
ex:alice a gmeow:Entity .
ex:usage gmeow:depictionSubject ex:alice .
ex:usage gmeow:depictionContext gmeow:depictionContextPortrait .
gmeow:depictionContextPortrait a gmeow:DepictionContext .
"
    ))
    .shape_union()
    .fails()
    .fails_on_path("https://blackcatinformatics.ca/gmeow/depictionImage", "MinCountConstraintComponent")
)]
#[case::image_region_shacl_passes(Case::inline(format!(
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
)))]
#[case::image_region_missing_selector_fails_shacl(
    Case::inline(format!(
        "{PREFIXES}\
ex:region a gmeow:ImageRegion .
ex:region rdfs:label \"Orphan Region\" .
ex:region gmeow:regionOf ex:img .
{media_object}",
        media_object = media_object_ttl("ex:img"),
    ))
    .shape_union()
    .fails()
    .fails_on_path("https://blackcatinformatics.ca/gmeow/regionSelector", "MinCountConstraintComponent")
)]
#[case::region_selector_missing_value_fails_shacl(
    Case::inline(format!(
        "{PREFIXES}\
ex:sel a gmeow:RegionSelector .
ex:sel gmeow:selectorType gmeow:selectorTypePixelRectangle .
gmeow:selectorTypePixelRectangle a gmeow:SelectorType .
"
    ))
    .shape_union()
    .fails()
    .fails_on_path("https://blackcatinformatics.ca/gmeow/selectorValue", "MinCountConstraintComponent")
)]
#[case::scene_graph_edge_shacl_passes(Case::inline(format!(
    "{PREFIXES}\
ex:edge a gmeow:SceneGraphEdge .
ex:edge gmeow:sceneSubject ex:region1 .
ex:edge gmeow:sceneObject ex:region2 .
ex:edge gmeow:sceneRelation gmeow:sceneRelationLeftOf .
ex:edge gmeow:sceneConfidence \"0.95\"^^xsd:decimal .
gmeow:sceneRelationLeftOf a gmeow:SceneRelationType .
{regions}",
    regions = two_regions_ttl(),
)))]
#[case::scene_graph_edge_missing_relation_fails_shacl(
    Case::inline(format!(
        "{PREFIXES}\
ex:edge a gmeow:SceneGraphEdge .
ex:edge gmeow:sceneSubject ex:region1 .
ex:edge gmeow:sceneObject ex:region2 .
{regions}",
        regions = two_regions_ttl(),
    ))
    .shape_union()
    .fails()
    .fails_on_path("https://blackcatinformatics.ca/gmeow/sceneRelation", "MinCountConstraintComponent")
)]
#[case::scene_graph_edge_confidence_out_of_range_fails_shacl(
    Case::inline(format!(
        "{PREFIXES}\
ex:edge a gmeow:SceneGraphEdge .
ex:edge gmeow:sceneSubject ex:region1 .
ex:edge gmeow:sceneObject ex:region2 .
ex:edge gmeow:sceneRelation gmeow:sceneRelationLeftOf .
ex:edge gmeow:sceneConfidence \"1.5\"^^xsd:decimal .
gmeow:sceneRelationLeftOf a gmeow:SceneRelationType .
{regions}",
        regions = two_regions_ttl(),
    ))
    .fails()
    .violations(&["sceneConfidence"])
)]
#[case::depiction_usage_multiple_subjects_fails_shacl(
    Case::inline(format!(
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
    ))
    .shape_union()
    .fails()
    .fails_on_path("https://blackcatinformatics.ca/gmeow/depictionSubject", "MaxCountConstraintComponent")
)]
#[case::media_object_colourspace_shacl_passes(Case::inline(format!(
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
)))]
// A MediaObject without a colourspace triggers a SHACL warning (not a violation);
// the colourspace match is case-insensitive (`warnings_ci`), mirroring the original.
#[case::media_object_missing_colourspace_warns_shacl(Case::inline(format!(
    "{PREFIXES}\
ex:img a gmeow:MediaObject .
ex:img rdfs:label \"Test Image\" .
"
)).warnings_ci(&["colourspace"]))]
fn images(#[case] case: Case) {
    case.run();
}
