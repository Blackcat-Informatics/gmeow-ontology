// SPDX-License-Identifier: AGPL-3.0-only

//! Conformance twins migrated from tests/test_music_pitch.py
//!
//! Migrates the 7 `run_shacl(Graph())`-conformance tests that build inline
//! triples and assert `result.ok` / `not result.ok` / error substrings.
//! All use `validate(&nt)` (fixture-only mode) because every Python test
//! calls `run_shacl(g)` where `g` is a locally-constructed `Graph()`.
//!
//! Retained in Python (not migrated):
//!   - `test_tuning_system_is_reference_frame`: `_graph()` / TBox membership.
//!   - `test_tuning_system_kind_is_quality_value`: `_graph()` / TBox membership.
//!   - `test_pitch_anchor_is_functional`: `_graph()` / TBox membership.
//!   - `test_has_tuning_frame_subproperty`: `_graph()` / TBox membership.
//!   - `test_tuning_kind_is_functional`: `_graph()` / TBox membership.
//!   - `test_tuning_frame_properties_are_not_functional`: `_graph()` + dynamic
//!     property sweep.
//!   - `test_tuning_system_seeds_coexist`: `_graph()` membership over named
//!     individuals.
//!   - `test_pitch_anchor_a440_and_a415_coexist`: `_graph()` membership.
//!   - `test_slendro_requires_host_but_12edo_does_not`: `_graph()` membership.
//!   - `test_no_direct_frequency_property_on_pitch_value`: `_graph()` + dynamic
//!     property sweep.

use crate::conformance_support::*;
use gmeow_test_batch_macros::batch_cases;

// ── Turtle prefix block ───────────────────────────────────────────────────────

const PREFIXES: &str = "\
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix ex:    <https://example.org/test-music-pitch/> .
@prefix rdf:   <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .
@prefix xsd:   <http://www.w3.org/2001/XMLSchema#> .
";

// ── Tests migrated from tests/test_music_pitch.py ────────────────────────────

#[batch_cases]
#[case::pitch_value_ratio_only_passes_shacl(
    Case::inline(format!(
        "{PREFIXES}\
ex:valueRatio a gmeow:PitchValue .
ex:valueRatio gmeow:hasTuningFrame gmeow:tuningSystemJustIntonation .
ex:valueRatio gmeow:ratioNumerator \"3\"^^xsd:integer .
ex:valueRatio gmeow:ratioDenominator \"2\"^^xsd:integer .
"
    ))
)]
#[case::pitch_value_cents_only_passes_shacl(
    Case::inline(format!(
        "{PREFIXES}\
ex:valueCents a gmeow:PitchValue .
ex:valueCents gmeow:hasTuningFrame gmeow:tuningSystem12EDO .
ex:valueCents gmeow:centsFromOrigin \"700.0\"^^xsd:decimal .
"
    ))
)]
#[case::pitch_value_missing_frame_fails_shacl(
    Case::inline(format!(
        "{PREFIXES}\
ex:valueNoFrame a gmeow:PitchValue .
ex:valueNoFrame gmeow:ratioNumerator \"3\"^^xsd:integer .
ex:valueNoFrame gmeow:ratioDenominator \"2\"^^xsd:integer .
"
    ))
    .fails()
    .violations(&["A PitchValue must be relative to exactly one TuningSystem (Principle 11)."])
)]
#[case::pitch_value_ratio_and_cents_fails_shacl(
    Case::inline(format!(
        "{PREFIXES}\
ex:valueBoth a gmeow:PitchValue .
ex:valueBoth gmeow:hasTuningFrame gmeow:tuningSystemJustIntonation .
ex:valueBoth gmeow:ratioNumerator \"3\"^^xsd:integer .
ex:valueBoth gmeow:ratioDenominator \"2\"^^xsd:integer .
ex:valueBoth gmeow:centsFromOrigin \"701.96\"^^xsd:decimal .
"
    ))
    .fails()
    .violations(&["A PitchValue must provide exactly one encoding: \
        either (ratioNumerator + ratioDenominator) or centsFromOrigin."])
)]
#[case::pitch_value_zero_denominator_fails_shacl(
    Case::inline(format!(
        "{PREFIXES}\
ex:valueZeroDenom a gmeow:PitchValue .
ex:valueZeroDenom gmeow:hasTuningFrame gmeow:tuningSystemJustIntonation .
ex:valueZeroDenom gmeow:ratioNumerator \"3\"^^xsd:integer .
ex:valueZeroDenom gmeow:ratioDenominator \"0\"^^xsd:integer .
"
    ))
    .fails()
    .violations(&["The ratio denominator must be a positive integer."])
)]
#[case::pitch_interval_xor_ratio_cents_missing_both_fails(
    Case::inline(format!(
        "{PREFIXES}\
ex:intervalNone a gmeow:PitchInterval .
ex:intervalNone gmeow:hasTuningFrame gmeow:tuningSystemJustIntonation .
"
    ))
    .fails()
    .violations(&["A PitchInterval must provide exactly one encoding: \
        either (ratioNumerator + ratioDenominator) or centsFromOrigin."])
)]
#[case::pitch_interval_xor_ratio_cents_both_fails(
    Case::inline(format!(
        "{PREFIXES}\
ex:intervalBoth a gmeow:PitchInterval .
ex:intervalBoth gmeow:hasTuningFrame gmeow:tuningSystemJustIntonation .
ex:intervalBoth gmeow:ratioNumerator \"3\"^^xsd:integer .
ex:intervalBoth gmeow:ratioDenominator \"2\"^^xsd:integer .
ex:intervalBoth gmeow:centsFromOrigin \"701.96\"^^xsd:decimal .
"
    ))
    .fails()
    .violations(&["A PitchInterval must provide exactly one encoding: \
        either (ratioNumerator + ratioDenominator) or centsFromOrigin."])
)]
#[case::pitch_interval_xor_ratio_cents_ratio_only_passes(
    Case::inline(format!(
        "{PREFIXES}\
ex:intervalRatio a gmeow:PitchInterval .
ex:intervalRatio gmeow:hasTuningFrame gmeow:tuningSystemJustIntonation .
ex:intervalRatio gmeow:ratioNumerator \"3\"^^xsd:integer .
ex:intervalRatio gmeow:ratioDenominator \"2\"^^xsd:integer .
"
    ))
)]
#[case::tuning_system_shape_requires_kind_and_realm(
    Case::inline(format!(
        "{PREFIXES}\
ex:tuningBad a gmeow:TuningSystem .
ex:tuningBad gmeow:frameRealm gmeow:frameRealmMusicalPitch .
ex:tuningBad gmeow:frameKind gmeow:frameKindScalar .
ex:tuningBad gmeow:requiresHost \"false\"^^xsd:boolean .
"
    ))
    // `TuningSystem`'s exactly-one-kind bound is now PROJECTED SHACL derived from the
    // EL-safe `logic:Restriction` axioms in `slices/extensions/music/module.ttl`
    // (`generated/shapes/validation-shapes.ttl`, `gmeow:TuningSystem-shape`), which —
    // like every restriction-derived cardinality shape — carries no `sh:message` (the
    // prose-message convention was retired with the shapes-to-logic migration; see
    // docs/MIGRATING-SHAPES-TO-LOGIC.md). `whole_shapes()` deliberately drops that file
    // from this fixture corpus, so exercising the projected bound requires the live
    // production shape union and the assertion is by path + constraint component.
    .shape_union()
    .fails()
    .fails_on_path(
        "https://blackcatinformatics.ca/gmeow/tuningKind",
        "MinCountConstraintComponent",
    )
)]
fn music_pitch(#[case] case: Case) {
    case.run();
}
