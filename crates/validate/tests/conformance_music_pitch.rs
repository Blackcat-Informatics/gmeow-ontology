// SPDX-License-Identifier: AGPL-3.0-only

//! Conformance twins migrated from tests/test_music_pitch.py (#867)
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

mod conformance_support;
use conformance_support::*;

// ── Turtle prefix block ───────────────────────────────────────────────────────

const PREFIXES: &str = "\
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix ex:    <https://example.org/test-music-pitch/> .
@prefix rdf:   <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .
@prefix xsd:   <http://www.w3.org/2001/XMLSchema#> .
";

// ── Tests migrated from tests/test_music_pitch.py ────────────────────────────

/// `test_pitch_value_ratio_only_passes_shacl` — a PitchValue with ratio
/// encoding and a tuning frame passes SHACL.
#[test]
fn pitch_value_ratio_only_passes_shacl() {
    let ttl = format!(
        "{PREFIXES}\
ex:valueRatio a gmeow:PitchValue .
ex:valueRatio gmeow:hasTuningFrame gmeow:tuningSystemJustIntonation .
ex:valueRatio gmeow:ratioNumerator \"3\"^^xsd:integer .
ex:valueRatio gmeow:ratioDenominator \"2\"^^xsd:integer .
"
    );
    let nt = ttl_str_to_nt(&ttl);
    let report = validate(&nt);
    assert!(
        ok(&report),
        "PitchValue with ratio encoding must pass SHACL; violations: {:?}",
        violations(&report)
    );
}

/// `test_pitch_value_cents_only_passes_shacl` — a PitchValue with cents
/// encoding and a tuning frame passes SHACL.
#[test]
fn pitch_value_cents_only_passes_shacl() {
    let ttl = format!(
        "{PREFIXES}\
ex:valueCents a gmeow:PitchValue .
ex:valueCents gmeow:hasTuningFrame gmeow:tuningSystem12EDO .
ex:valueCents gmeow:centsFromOrigin \"700.0\"^^xsd:decimal .
"
    );
    let nt = ttl_str_to_nt(&ttl);
    let report = validate(&nt);
    assert!(
        ok(&report),
        "PitchValue with cents encoding must pass SHACL; violations: {:?}",
        violations(&report)
    );
}

/// `test_pitch_value_missing_frame_fails_shacl` — a PitchValue with no
/// `hasTuningFrame` must fail SHACL with the expected message.
#[test]
fn pitch_value_missing_frame_fails_shacl() {
    let ttl = format!(
        "{PREFIXES}\
ex:valueNoFrame a gmeow:PitchValue .
ex:valueNoFrame gmeow:ratioNumerator \"3\"^^xsd:integer .
ex:valueNoFrame gmeow:ratioDenominator \"2\"^^xsd:integer .
"
    );
    let nt = ttl_str_to_nt(&ttl);
    let report = validate(&nt);
    assert!(
        !ok(&report),
        "PitchValue without tuning frame must fail SHACL"
    );
    let msgs = violations(&report);
    assert!(!msgs.is_empty(), "expected at least one violation");
    assert!(
        msgs.iter().any(|m| m
            .contains("A PitchValue must be relative to exactly one TuningSystem (Principle 11).")),
        "expected missing-frame violation message; got: {:?}",
        msgs
    );
}

/// `test_pitch_value_ratio_and_cents_fails_shacl` — a PitchValue with both
/// ratio and cents encoding must fail SHACL with the XOR message.
#[test]
fn pitch_value_ratio_and_cents_fails_shacl() {
    let ttl = format!(
        "{PREFIXES}\
ex:valueBoth a gmeow:PitchValue .
ex:valueBoth gmeow:hasTuningFrame gmeow:tuningSystemJustIntonation .
ex:valueBoth gmeow:ratioNumerator \"3\"^^xsd:integer .
ex:valueBoth gmeow:ratioDenominator \"2\"^^xsd:integer .
ex:valueBoth gmeow:centsFromOrigin \"701.96\"^^xsd:decimal .
"
    );
    let nt = ttl_str_to_nt(&ttl);
    let report = validate(&nt);
    assert!(
        !ok(&report),
        "PitchValue with both ratio and cents must fail SHACL"
    );
    let msgs = violations(&report);
    assert!(!msgs.is_empty(), "expected at least one violation");
    let expected = "A PitchValue must provide exactly one encoding: \
        either (ratioNumerator + ratioDenominator) or centsFromOrigin.";
    assert!(
        msgs.iter().any(|m| m.contains(expected)),
        "expected XOR encoding violation message; got: {:?}",
        msgs
    );
}

/// `test_pitch_value_zero_denominator_fails_shacl` — a PitchValue with a zero
/// ratio denominator must fail SHACL.
#[test]
fn pitch_value_zero_denominator_fails_shacl() {
    let ttl = format!(
        "{PREFIXES}\
ex:valueZeroDenom a gmeow:PitchValue .
ex:valueZeroDenom gmeow:hasTuningFrame gmeow:tuningSystemJustIntonation .
ex:valueZeroDenom gmeow:ratioNumerator \"3\"^^xsd:integer .
ex:valueZeroDenom gmeow:ratioDenominator \"0\"^^xsd:integer .
"
    );
    let nt = ttl_str_to_nt(&ttl);
    let report = validate(&nt);
    assert!(
        !ok(&report),
        "PitchValue with zero denominator must fail SHACL"
    );
    let msgs = violations(&report);
    assert!(!msgs.is_empty(), "expected at least one violation");
    assert!(
        msgs.iter()
            .any(|m| m.contains("The ratio denominator must be a positive integer.")),
        "expected positive-denominator violation message; got: {:?}",
        msgs
    );
}

/// `test_pitch_interval_xor_ratio_cents` — a PitchInterval must carry exactly
/// one encoding (ratio xor cents); tests missing-both, both-present, and
/// ratio-only cases.
#[test]
fn pitch_interval_xor_ratio_cents_missing_both_fails() {
    let ttl = format!(
        "{PREFIXES}\
ex:intervalNone a gmeow:PitchInterval .
ex:intervalNone gmeow:hasTuningFrame gmeow:tuningSystemJustIntonation .
"
    );
    let nt = ttl_str_to_nt(&ttl);
    let report = validate(&nt);
    assert!(
        !ok(&report),
        "PitchInterval with no encoding must fail SHACL"
    );
    let msgs = violations(&report);
    assert!(!msgs.is_empty(), "expected at least one violation");
    let expected = "A PitchInterval must provide exactly one encoding: \
        either (ratioNumerator + ratioDenominator) or centsFromOrigin.";
    assert!(
        msgs.iter().any(|m| m.contains(expected)),
        "expected missing-encoding violation; got: {:?}",
        msgs
    );
}

#[test]
fn pitch_interval_xor_ratio_cents_both_fails() {
    let ttl = format!(
        "{PREFIXES}\
ex:intervalBoth a gmeow:PitchInterval .
ex:intervalBoth gmeow:hasTuningFrame gmeow:tuningSystemJustIntonation .
ex:intervalBoth gmeow:ratioNumerator \"3\"^^xsd:integer .
ex:intervalBoth gmeow:ratioDenominator \"2\"^^xsd:integer .
ex:intervalBoth gmeow:centsFromOrigin \"701.96\"^^xsd:decimal .
"
    );
    let nt = ttl_str_to_nt(&ttl);
    let report = validate(&nt);
    assert!(
        !ok(&report),
        "PitchInterval with both ratio and cents must fail SHACL"
    );
    let msgs = violations(&report);
    assert!(!msgs.is_empty(), "expected at least one violation");
    let expected = "A PitchInterval must provide exactly one encoding: \
        either (ratioNumerator + ratioDenominator) or centsFromOrigin.";
    assert!(
        msgs.iter().any(|m| m.contains(expected)),
        "expected XOR encoding violation; got: {:?}",
        msgs
    );
}

#[test]
fn pitch_interval_xor_ratio_cents_ratio_only_passes() {
    let ttl = format!(
        "{PREFIXES}\
ex:intervalRatio a gmeow:PitchInterval .
ex:intervalRatio gmeow:hasTuningFrame gmeow:tuningSystemJustIntonation .
ex:intervalRatio gmeow:ratioNumerator \"3\"^^xsd:integer .
ex:intervalRatio gmeow:ratioDenominator \"2\"^^xsd:integer .
"
    );
    let nt = ttl_str_to_nt(&ttl);
    let report = validate(&nt);
    assert!(
        ok(&report),
        "PitchInterval with ratio-only encoding must pass SHACL; violations: {:?}",
        violations(&report)
    );
}

/// `test_tuning_system_shape_requires_kind_and_realm` — a TuningSystem missing
/// `tuningKind` must fail SHACL with the expected message.
#[test]
fn tuning_system_shape_requires_kind_and_realm() {
    let ttl = format!(
        "{PREFIXES}\
ex:tuningBad a gmeow:TuningSystem .
ex:tuningBad gmeow:frameRealm gmeow:frameRealmMusicalPitch .
ex:tuningBad gmeow:frameKind gmeow:frameKindScalar .
ex:tuningBad gmeow:requiresHost \"false\"^^xsd:boolean .
"
    );
    let nt = ttl_str_to_nt(&ttl);
    let report = validate(&nt);
    assert!(
        !ok(&report),
        "TuningSystem without tuningKind must fail SHACL"
    );
    let msgs = violations(&report);
    assert!(!msgs.is_empty(), "expected at least one violation");
    assert!(
        msgs.iter()
            .any(|m| m.contains("A TuningSystem must have exactly one tuningKind (Principle 9).")),
        "expected missing-tuningKind violation; got: {:?}",
        msgs
    );
}
