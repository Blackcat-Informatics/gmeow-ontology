// SPDX-License-Identifier: AGPL-3.0-only
// Conformance twins migrated from tests/test_narrative.py

//! Conformance twins migrated from tests/test_narrative.py.
//!
//! Each test builds an inline Turtle graph containing the triples that the
//! Python test assembled via `g.add(...)` / `_add_narrative_frame(...)`,
//! converts to N-Triples, and validates against the whole shapes corpus.
//!
//! `_add_narrative_frame(g, frame, axis)` expanded inline:
//!
//! ```text
//!   <frame> a gmeow:NarrativeReferenceFrame ;
//!           gmeow:frameRealm gmeow:frameRealmNarrative ;
//!           gmeow:hasAxis <axis> ;
//!           gmeow:dimensionCount "1"^^xsd:nonNegativeInteger ;
//!           gmeow:frameKind gmeow:frameKindNarrative ;
//!           gmeow:requiresHost false ;
//!           gmeow:determinacyModel gmeow:determinacyCrisp .
//! ```
//!
//! Retained in Python (not migrated):
//!   - `test_narrative_reference_frame_is_not_standpoint_subclass`: transitive
//!     graph walk over the merged graph; cross-slice `Standpoint` import makes it
//!     non-portable.
//!   - `test_book_release_and_serial_installment_are_creative_works`: transitive
//!     walk; subjects live in slices/core/documents/module.ttl (cross-slice).
//!   - `test_frame_realm_narrative_and_frame_kind_narrative_exist`: triple
//!     membership test on the merged graph; subjects in slices/core/places
//!     (cross-slice).
//!   - `test_reading_order_subclasses_standpoint`: triple membership test on the
//!     merged graph; `ReadingOrder` subject in slices/core/documents (cross-slice).

use crate::conformance_support::*;
use gmeow_test_batch_macros::batch_cases;

const GMEOW: &str = "https://blackcatinformatics.ca/gmeow/";
const RDFS_SUBCLASS_OF: &str = "http://www.w3.org/2000/01/rdf-schema#subClassOf";

fn gm(local: &str) -> String {
    format!("{GMEOW}{local}")
}

// ── Shared Turtle prefix block ────────────────────────────────────────────────

const PREFIXES: &str = "\
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix ex:    <https://example.org/test/> .
@prefix rdf:   <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .
@prefix xsd:   <http://www.w3.org/2001/XMLSchema#> .
";

// ── Inline expansion of `_add_narrative_frame(g, frame, axis)` ───────────────

/// Emit the 7 triples `_add_narrative_frame` adds for a given frame + axis.
fn add_narrative_frame_ttl(frame: &str, axis: &str) -> String {
    format!(
        "\
{frame} a gmeow:NarrativeReferenceFrame .
{frame} gmeow:frameRealm gmeow:frameRealmNarrative .
{frame} gmeow:hasAxis {axis} .
{frame} gmeow:dimensionCount \"1\"^^xsd:nonNegativeInteger .
{frame} gmeow:frameKind gmeow:frameKindNarrative .
{frame} gmeow:requiresHost false .
{frame} gmeow:determinacyModel gmeow:determinacyCrisp .
"
    )
}

// ── Tests migrated from tests/test_narrative.py ───────────────────────────────

#[batch_cases]
#[case::narrative_reference_frame_shacl_passes(
    Case::inline(format!(
        "{PREFIXES}\
{frame}\
gmeow:frameRealmNarrative a gmeow:FrameRealm .
ex:axisPlot a gmeow:Axis .
gmeow:frameKindNarrative a gmeow:FrameKind .
gmeow:determinacyCrisp a gmeow:Determinacy .
",
        frame = add_narrative_frame_ttl("ex:hpCanon", "ex:axisPlot"),
    ))
)]
#[case::narrative_frame_link_shacl_passes(
    Case::inline(format!(
        "{PREFIXES}\
{mcu}\
{earth616}\
ex:mcu616Link a gmeow:NarrativeFrameLink .
ex:mcu616Link gmeow:narrativeFrameLinkSource ex:mcuCanon .
ex:mcu616Link gmeow:narrativeFrameLinkTarget ex:earth616Canon .
ex:mcu616Link gmeow:narrativeFrameLinkRelation gmeow:relationAdaptationOf .
gmeow:frameRealmNarrative a gmeow:FrameRealm .
ex:axisPlotMcu a gmeow:Axis .
ex:axisPlot616 a gmeow:Axis .
gmeow:frameKindNarrative a gmeow:FrameKind .
gmeow:determinacyCrisp a gmeow:Determinacy .
gmeow:relationAdaptationOf a gmeow:NarrativeFrameRelation .
",
        mcu = add_narrative_frame_ttl("ex:mcuCanon", "ex:axisPlotMcu"),
        earth616 = add_narrative_frame_ttl("ex:earth616Canon", "ex:axisPlot616"),
    ))
)]
#[case::character_arc_shacl_passes(
    Case::inline(format!(
        "{PREFIXES}\
{frame}\
ex:harryArc a gmeow:CharacterArc .
ex:harryArc gmeow:arcSubject ex:harry .
ex:harryArc gmeow:arcFrame ex:hpCanon .
ex:harryArc gmeow:arcType gmeow:arcTypeComingOfAge .
ex:harry a gmeow:Entity .
gmeow:arcTypeComingOfAge a gmeow:ArcType .
gmeow:frameRealmNarrative a gmeow:FrameRealm .
ex:axisPlot a gmeow:Axis .
gmeow:frameKindNarrative a gmeow:FrameKind .
gmeow:determinacyCrisp a gmeow:Determinacy .
",
        frame = add_narrative_frame_ttl("ex:hpCanon", "ex:axisPlot"),
    ))
)]
#[case::character_arc_missing_subject_fails_shacl(
    Case::inline(format!(
        "{PREFIXES}\
{frame}\
ex:harryArc a gmeow:CharacterArc .
ex:harryArc gmeow:arcFrame ex:hpCanon .
ex:harryArc gmeow:arcType gmeow:arcTypeComingOfAge .
gmeow:arcTypeComingOfAge a gmeow:ArcType .
gmeow:frameRealmNarrative a gmeow:FrameRealm .
ex:axisPlot a gmeow:Axis .
gmeow:frameKindNarrative a gmeow:FrameKind .
gmeow:determinacyCrisp a gmeow:Determinacy .
",
        frame = add_narrative_frame_ttl("ex:hpCanon", "ex:axisPlot"),
    ))
    .fails()
    .violations(&["CharacterArc must have exactly one gmeow:arcSubject"])
)]
fn narrative(#[case] case: Case) {
    case.run();
}

// ── `_graph()`-based TBox + transitive-closure twins ─────────────────────────

/// gUFO MixIden: gmeow:NarrativeReferenceFrame specializes gmeow:ReferenceFrame
/// only and must NOT have gmeow:Standpoint in its transitive subclass closure
/// (Standpoint is imported cross-slice, so this needs the merged ontology).
#[gmeow_test_batch_macros::batch_test]
fn narrative_reference_frame_is_not_standpoint_subclass() {
    let g = GraphStore::ontology();
    let closure = g.subclass_closure(&gm("NarrativeReferenceFrame"));
    assert!(
        !closure.contains(&gm("Standpoint")),
        "gmeow:NarrativeReferenceFrame must not be a subclass of gmeow:Standpoint; closure: {closure:?}"
    );
}

/// gmeow:BookRelease and gmeow:SerialInstallment (defined cross-slice in the
/// documents module) each have gmeow:CreativeWork in their transitive subclass
/// closure.
#[gmeow_test_batch_macros::batch_test]
fn book_release_and_serial_installment_are_creative_works() {
    let g = GraphStore::ontology();
    for cls in ["BookRelease", "SerialInstallment"] {
        let closure = g.subclass_closure(&gm(cls));
        assert!(
            closure.contains(&gm("CreativeWork")),
            "gmeow:{cls} must be a (transitive) subclass of gmeow:CreativeWork; closure: {closure:?}"
        );
    }
}

/// The narrative frame realm and frame kind individuals (defined cross-slice in
/// the places module) are declared with their expected types.
#[gmeow_test_batch_macros::batch_test]
fn frame_realm_narrative_and_frame_kind_narrative_exist() {
    let g = GraphStore::ontology();
    assert!(
        g.has(
            Some(&gm("frameRealmNarrative")),
            Some(RDF_TYPE),
            Some(&gm("FrameRealm"))
        ),
        "gmeow:frameRealmNarrative must be a gmeow:FrameRealm"
    );
    assert!(
        g.has(
            Some(&gm("frameKindNarrative")),
            Some(RDF_TYPE),
            Some(&gm("FrameKind"))
        ),
        "gmeow:frameKindNarrative must be a gmeow:FrameKind"
    );
}

/// gmeow:ReadingOrder (defined cross-slice in the documents module) is a direct
/// rdfs:subClassOf gmeow:Standpoint.
#[gmeow_test_batch_macros::batch_test]
fn reading_order_subclasses_standpoint() {
    let g = GraphStore::ontology();
    assert!(
        g.has(
            Some(&gm("ReadingOrder")),
            Some(RDFS_SUBCLASS_OF),
            Some(&gm("Standpoint"))
        ),
        "gmeow:ReadingOrder must be rdfs:subClassOf gmeow:Standpoint"
    );
}
