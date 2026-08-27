// SPDX-License-Identifier: AGPL-3.0-only
//! Conformance twins migrated from tests/test_creative_works.py
//!
//! Migrated tests build inline Turtle graphs, convert to N-Triples, and validate
//! against the whole shapes corpus via `validate()`.
//!
//! The Python helper functions `_add_work`, `_add_expression`, `_add_manifestation`,
//! and `_add_item` are inlined as Turtle string constants or helper functions below.
//!
//! Retained in Python (not migrated):
//!   - `test_creative_work_is_category`: loads merged graph, cross-slice subject in documents/
//!   - `test_wemi_tiers_subclass_information_object`: uses transitive_objects() graph traversal
//!   - `test_document_subclasses_work`: cross-slice subjects in documents/
//!   - `test_media_etc_subclasses_manifestation`: cross-slice subjects in documents/
//!   - `test_contribution_degree_value_vocab`: cross-slice subjects in citations/
//!   - `test_creation_event_types_exist`: cross-slice subjects in events/
//!   - `test_literary_work_subclasses_work`: cross-slice subject in documents/
//!   - `test_serial_work_subclasses_work`: cross-slice subject in documents/
//!   - `test_content_segment_subclasses_information_object`: cross-slice subject in documents/
//!   - `test_has_segment_is_subproperty_of_has_part`: cross-slice subject in documents/
//!   - `test_segment_of_is_subproperty_of_part_of`: cross-slice subject in documents/
//!   - `test_segment_type_is_functional`: cross-slice subject in documents/
//!   - `test_segment_index_is_functional`: cross-slice subject in documents/
//!   - `test_content_segment_type_value_vocab`: cross-slice subjects in documents/

use crate::conformance_support::*;
use gmeow_test_batch_macros::batch_cases;

const GMEOW: &str = "https://blackcatinformatics.ca/gmeow/";

fn gm(local: &str) -> String {
    format!("{GMEOW}{local}")
}

// ── Shared prefix block ───────────────────────────────────────────────────────

const PREFIXES: &str = "\
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix ex:    <https://example.org/test/> .
@prefix rdf:   <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .
@prefix rdfs:  <http://www.w3.org/2000/01/rdf-schema#> .
@prefix xsd:   <http://www.w3.org/2001/XMLSchema#> .
";

// ── Inline helper: populate a reference frame ─────────────────────────────────
//
// Mirrors `_add_expression` in Python: the reference frame sub-graph required
// by the SHACL shape for gmeow:Expression.

const REFERENCE_FRAME_TTL: &str = "\
ex:englishFrame a gmeow:ReferenceFrame .
ex:englishFrame rdfs:label \"English\" .
ex:englishFrame gmeow:frameRealm gmeow:frameRealmNarrative .
ex:englishFrame gmeow:hasAxis ex:axisLang .
ex:englishFrame gmeow:dimensionCount \"1\"^^xsd:nonNegativeInteger .
ex:englishFrame gmeow:frameKind gmeow:frameKindNarrative .
ex:englishFrame gmeow:requiresHost false .
ex:englishFrame gmeow:determinacyModel gmeow:determinacyCrisp .
gmeow:frameRealmNarrative a gmeow:FrameRealm .
ex:axisLang a gmeow:Axis .
gmeow:frameKindNarrative a gmeow:FrameKind .
gmeow:determinacyCrisp a gmeow:Determinacy .
";

// ── Tests migrated from tests/test_creative_works.py ─────────────────────────

#[batch_cases]
#[case::spine_shacl_passes(Case::inline(format!(
    "{PREFIXES}\
ex:work a gmeow:Work .
ex:work rdfs:label \"Test Work\" .
ex:expression a gmeow:Expression .
ex:expression rdfs:label \"Test Expression\" .
ex:expression gmeow:realizes ex:work .
ex:expression gmeow:hasReferenceFrame ex:englishFrame .
{REFERENCE_FRAME_TTL}\
ex:manifestation a gmeow:Manifestation .
ex:manifestation rdfs:label \"Test Manifestation\" .
ex:manifestation gmeow:embodies ex:expression .
ex:item a gmeow:Item .
ex:item rdfs:label \"Test Item\" .
ex:item gmeow:exemplifies ex:manifestation .
"
)))]
#[case::expression_without_work_fails_shacl(
    // The realizes existence migrated to the projected surface
    // (generated/shapes/validation-shapes.ttl, Expression-shape sh:minCount 1),
    // which the fixture corpus deliberately excludes — witness it on the LIVE
    // production shape union by path (projected shapes carry no sh:message).
    Case::inline(format!(
        "{PREFIXES}\
ex:expression a gmeow:Expression .
ex:expression rdfs:label \"Orphan Expression\" .
ex:expression gmeow:hasReferenceFrame ex:englishFrame .
{REFERENCE_FRAME_TTL}\
"
    ))
    .shape_union()
    .fails()
    .fails_on_path(
        "https://blackcatinformatics.ca/gmeow/realizes",
        "MinCountConstraintComponent"
    )
)]
#[case::manifestation_without_expression_fails_shacl(
    Case::inline(format!(
        "{PREFIXES}\
ex:manifestation a gmeow:Manifestation .
ex:manifestation rdfs:label \"Orphan Manifestation\" .
"
    ))
    .fails()
    .violations(&["Manifestation must embody"])
)]
#[case::item_without_manifestation_fails_shacl(
    // Same migration: the exemplifies existence rides the projected
    // Item-shape (sh:minCount 1) on the production union.
    Case::inline(format!(
        "{PREFIXES}\
ex:item a gmeow:Item .
ex:item rdfs:label \"Orphan Item\" .
"
    ))
    .shape_union()
    .fails()
    .fails_on_path(
        "https://blackcatinformatics.ca/gmeow/exemplifies",
        "MinCountConstraintComponent"
    )
)]
#[case::contribution_shacl_passes(Case::inline(format!(
    "{PREFIXES}\
ex:contribution a gmeow:Contribution .
ex:contribution gmeow:contributor ex:alice .
ex:contribution gmeow:contributionTarget ex:work .
ex:contribution gmeow:contributionRole gmeow:roleAuthor .
ex:alice a gmeow:Agent .
ex:work a gmeow:Work .
ex:work rdfs:label \"Test Work\" .
"
)))]
#[case::contribution_missing_role_fails_shacl(
    Case::inline(format!(
        "{PREFIXES}\
ex:contribution a gmeow:Contribution .
ex:contribution gmeow:contributor ex:alice .
ex:contribution gmeow:contributionTarget ex:work .
ex:alice a gmeow:Agent .
ex:work a gmeow:Work .
ex:work rdfs:label \"Test Work\" .
"
    ))
    .shape_union()
    .fails()
    // The role existence rides the projected Contribution-shape (sh:minCount 1)
    // on the production union; projected shapes carry no sh:message.
    .fails_on_path(
        "https://blackcatinformatics.ca/gmeow/contributionRole",
        "MinCountConstraintComponent"
    )
)]
#[case::content_segment_shacl_passes(Case::inline(format!(
    "{PREFIXES}\
ex:chapter1 a gmeow:ContentSegment .
ex:chapter1 rdfs:label \"Chapter 1\" .
ex:chapter1 gmeow:segmentOf ex:book .
ex:book a gmeow:LiteraryWork .
ex:book rdfs:label \"Test Book\" .
"
)))]
#[case::content_segment_without_container_fails_shacl(
    Case::inline(format!(
        "{PREFIXES}\
ex:chapter1 a gmeow:ContentSegment .
ex:chapter1 rdfs:label \"Orphan Chapter\" .
"
    ))
    .shape_union()
    .fails()
    // The segmentOf existence rides the projected ContentSegment-shape
    // (sh:minCount 1) on the production union.
    .fails_on_path(
        "https://blackcatinformatics.ca/gmeow/segmentOf",
        "MinCountConstraintComponent"
    )
)]
fn creative_works(#[case] case: Case) {
    case.run();
}

/// The full frame-relativity IRI of the Expression frame carrier — the `sh:path` the
/// generated `gmeow:ExpressionFrameRequirementShape` constrains (`sh:minCount 1`).
const HAS_REFERENCE_FRAME: &str = "https://blackcatinformatics.ca/gmeow/hasReferenceFrame";

/// W1 falsifying regression (CONSTITUTION P11 frame-relativity): a `gmeow:Expression`
/// asserted with NO `gmeow:hasReferenceFrame` is REJECTED — the generated
/// `gmeow:ExpressionFrameRequirementShape` (`sh:path gmeow:hasReferenceFrame`,
/// `sh:minCount 1`, `MinCountConstraintComponent`) flags the frameless Expression on
/// the LIVE production shape union (`.shape_union()`, the corpus `gmeow validate`
/// runs).
///
/// The frame requirement is a hard `sh:Violation`: `gmeow:Expression` carries no
/// `gmeow:ruleSeverity`, so its `gmeow:requiresFrame` shape generates at binding
/// severity. [`Case::fails_on_path`] (which filters to `sh:Violation`) therefore both
/// witnesses the frame `MinCountConstraintComponent` on the exact path AND guards the
/// promotion — if the class ever regains `gmeow:ruleSeverity "advisory"`, the shape
/// reverts to `sh:Warning`, no Violation fires on this path, and this test reds.
#[gmeow_test_batch_macros::batch_test]
fn frameless_expression_fails_frame_requirement() {
    Case::inline(format!(
        "{PREFIXES}\
ex:x a gmeow:Expression .
ex:x rdfs:label \"Frameless Expression\" .
"
    ))
    .shape_union()
    .fails()
    .fails_on_path(HAS_REFERENCE_FRAME, "MinCountConstraintComponent")
    .run();
}

/// Each WEMI tier class (Work, Expression, Manifestation, Item) has
/// gmeow:InformationObject in its transitive rdfs:subClassOf closure — a live
/// graph traversal over the merged ontology, not a single-module ASK.
#[gmeow_test_batch_macros::batch_test]
fn wemi_tiers_subclass_information_object() {
    let g = GraphStore::ontology();
    for cls in ["Work", "Expression", "Manifestation", "Item"] {
        let closure = g.subclass_closure(&gm(cls));
        assert!(
            closure.contains(&gm("InformationObject")),
            "gmeow:{cls} must be a (transitive) subclass of gmeow:InformationObject; closure: {closure:?}"
        );
    }
}
