// SPDX-License-Identifier: AGPL-3.0-only
//! Conformance twins migrated from tests/test_myth.py (#867)
//!
//! Each test builds an inline Turtle graph containing the triples that the
//! Python test assembled via `g.add(...)`, converts to N-Triples, and
//! validates against the whole shapes corpus.
//!
//! `_add_narrative_frame(g, frame, axis)` expanded inline for every call site:
//!
//! ```text
//!   <frame>  a gmeow:NarrativeReferenceFrame ;
//!            gmeow:frameRealm gmeow:frameRealmNarrative ;
//!            gmeow:hasAxis <axis> ;
//!            gmeow:dimensionCount "1"^^xsd:nonNegativeInteger ;
//!            gmeow:frameKind gmeow:frameKindNarrative ;
//!            gmeow:requiresHost false ;
//!            gmeow:determinacyModel gmeow:determinacyCrisp .
//! ```
//!
//! Retained in Python (not migrated):
//!   - `test_myth_is_kind_and_social_object`: calls `_graph()` / TBox membership.
//!   - `test_social_object_is_category`: calls `_graph()` / TBox membership.
//!   - `test_myth_properties_exist`: calls `_graph()` / dynamic sweep over properties.
//!   - `test_has_myth_telling_domain_range`: calls `_graph()` / TBox membership.
//!   - `test_myth_frame_is_functional`: calls `_graph()` / TBox membership.
//!   - `test_propagates_from_is_derived_from_subproperty`: calls `_graph()` / TBox membership.
//!   - `test_recurring_risk_exists`: calls `_graph()` / TBox membership.
//!   - `test_affected_consumer_surface_exists`: calls `_graph()` / TBox membership.
//!   - `test_myth_el_restriction_on_has_myth_telling`: calls `_graph()` + bnode iteration.
//!   - `test_no_truth_axiom_on_myth`: calls `_graph()` / dynamic sweep.

mod conformance_support;
use conformance_support::*;
use rstest::rstest;

// ── Helpers for the inline Turtle snippets ────────────────────────────────────

/// Turtle prefix block shared by all myth tests.
const PREFIXES: &str = "\
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix ex:    <https://example.org/test/> .
@prefix rdf:   <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .
@prefix xsd:   <http://www.w3.org/2001/XMLSchema#> .
";

/// Inline expansion of `_add_narrative_frame(g, frame, axis)`.
///
/// Emits the triples the Python helper adds for a minimal NarrativeReferenceFrame.
fn narrative_frame_ttl(frame: &str, axis: &str) -> String {
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

// ── Tests migrated from tests/test_myth.py ────────────────────────────────────

#[rstest]
#[case::myth_shacl_passes(
    Case::inline(format!(
        "{PREFIXES}\
{frame}\
ex:urbanLegend a gmeow:Myth .
ex:urbanLegend gmeow:mythFrame ex:urbanLegendFrame .
ex:urbanLegend gmeow:hasMythTelling ex:articleTelling .
ex:urbanLegend gmeow:recurringRisk true .
ex:urbanLegend gmeow:affectedConsumerSurface gmeow:consumerPublicSite .
ex:articleTelling a gmeow:CreativeWork .
gmeow:frameRealmNarrative a gmeow:FrameRealm .
ex:axisPlot a gmeow:Axis .
gmeow:frameKindNarrative a gmeow:FrameKind .
gmeow:determinacyCrisp a gmeow:Determinacy .
gmeow:consumerPublicSite a gmeow:ProjectionContext .
",
        frame = narrative_frame_ttl("ex:urbanLegendFrame", "ex:axisPlot"),
    ))
)]
#[case::myth_missing_frame_fails_shacl(
    Case::inline(format!(
        "{PREFIXES}\
ex:urbanLegend a gmeow:Myth .
ex:urbanLegend gmeow:hasMythTelling ex:articleTelling .
ex:articleTelling a gmeow:CreativeWork .
"
    ))
        .fails()
        .violations(&["exactly one reference frame (gmeow:mythFrame)"])
)]
#[case::myth_propagation_shacl_passes(
    Case::inline(format!(
        "{PREFIXES}\
{frame}\
ex:urbanLegend a gmeow:Myth .
ex:urbanLegend gmeow:mythFrame ex:urbanLegendFrame .
ex:articleTelling a gmeow:CreativeWork .
ex:socialPostTelling a gmeow:CreativeWork .
ex:socialPostTelling gmeow:propagatesFrom ex:articleTelling .
ex:urbanLegend gmeow:hasMythTelling ex:articleTelling .
ex:urbanLegend gmeow:hasMythTelling ex:socialPostTelling .
gmeow:frameRealmNarrative a gmeow:FrameRealm .
ex:axisPlot a gmeow:Axis .
gmeow:frameKindNarrative a gmeow:FrameKind .
gmeow:determinacyCrisp a gmeow:Determinacy .
",
        frame = narrative_frame_ttl("ex:urbanLegendFrame", "ex:axisPlot"),
    ))
)]
fn myth(#[case] case: Case) {
    case.run();
}
