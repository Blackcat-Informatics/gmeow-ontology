// SPDX-License-Identifier: AGPL-3.0-only
// Conformance twins migrated from tests/test_narrative.py (#867)

//! Conformance twins migrated from tests/test_narrative.py (#867).
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

mod conformance_support;
use conformance_support::*;

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

/// `test_narrative_reference_frame_shacl_passes` — a fully-populated narrative
/// frame passes SHACL.
#[test]
fn narrative_reference_frame_shacl_passes() {
    let ttl = format!(
        "{PREFIXES}\
{frame}\
gmeow:frameRealmNarrative a gmeow:FrameRealm .
ex:axisPlot a gmeow:Axis .
gmeow:frameKindNarrative a gmeow:FrameKind .
gmeow:determinacyCrisp a gmeow:Determinacy .
",
        frame = add_narrative_frame_ttl("ex:hpCanon", "ex:axisPlot"),
    );
    let nt = ttl_str_to_nt(&ttl);
    let report = validate(&nt);
    assert!(
        ok(&report),
        "fully-populated narrative frame must pass SHACL; violations: {:?}",
        violations(&report)
    );
}

/// `test_narrative_frame_link_shacl_passes` — a reified frame link (MCU is
/// adaptation of Earth-616) passes SHACL.
#[test]
fn narrative_frame_link_shacl_passes() {
    let ttl = format!(
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
    );
    let nt = ttl_str_to_nt(&ttl);
    let report = validate(&nt);
    assert!(
        ok(&report),
        "reified narrative frame link (MCU adapts Earth-616) must pass SHACL; violations: {:?}",
        violations(&report)
    );
}

/// `test_character_arc_shacl_passes` — a well-formed CharacterArc passes SHACL.
#[test]
fn character_arc_shacl_passes() {
    let ttl = format!(
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
    );
    let nt = ttl_str_to_nt(&ttl);
    let report = validate(&nt);
    assert!(
        ok(&report),
        "well-formed CharacterArc must pass SHACL; violations: {:?}",
        violations(&report)
    );
}

/// `test_character_arc_missing_subject_fails_shacl` — a CharacterArc missing
/// `arcSubject` must violate SHACL with the expected message.
#[test]
fn character_arc_missing_subject_fails_shacl() {
    let ttl = format!(
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
    );
    let nt = ttl_str_to_nt(&ttl);
    let report = validate(&nt);
    assert!(
        !ok(&report),
        "CharacterArc missing arcSubject must fail SHACL"
    );
    let msgs = violations(&report);
    assert!(
        msgs.iter()
            .any(|m| m.contains("CharacterArc must have exactly one gmeow:arcSubject")),
        "expected 'CharacterArc must have exactly one gmeow:arcSubject' in violations; got: {:?}",
        msgs
    );
}
