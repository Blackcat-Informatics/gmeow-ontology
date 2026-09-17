// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Read-only consumer tag-map and retagging contracts over the authenticated bundle.
//! Authored reference-catalog coverage is observed by the pipeline producer.

use std::collections::HashMap;

mod conformance_support;
use conformance_support::authenticated_bundle_dataset;
use gmeow_validate::language_tags::{
    load_inverse_tag_map_from_dataset, load_tag_map_from_dataset, retag_graph_to_internal,
};

/// Mirror of `test_language_tag_map_is_deterministic_and_covers_catalog`:
/// `load_tag_map` over the authenticated carrier is deterministic across two reads and
/// covers the three framework carrier tags. Since the lang: graft, the internal
/// `x-gmeow-*` tag rides `lang:carrierTag` on the three carrier varieties
/// (gmeowEnglish/gmeowFrench/gmeowMandarin) ONLY, and their BCP-47 tag is
/// GENERATED — the former per-language `gmeow:languageTag`
/// (japanese/arabic/hindi/python/…) is dropped, so only the carriers map.
#[test]
fn language_tag_map_is_deterministic_and_covers_catalog() {
    let dataset = authenticated_bundle_dataset();
    let map_a: HashMap<String, String> =
        load_tag_map_from_dataset(dataset).expect("first load_tag_map_from_dataset must succeed");
    let map_b: HashMap<String, String> =
        load_tag_map_from_dataset(dataset).expect("second load_tag_map_from_dataset must succeed");
    assert_eq!(map_a, map_b, "load_tag_map output must be deterministic");

    for (internal_tag, expected_bcp) in [
        ("x-gmeow-english", "en"),
        ("x-gmeow-french", "fr"),
        ("x-gmeow-mandarin", "zh"),
    ] {
        let bcp = map_a
            .get(internal_tag)
            .unwrap_or_else(|| panic!("missing tag mapping for {internal_tag}"));
        assert_eq!(bcp, expected_bcp, "wrong BCP-47 mapping for {internal_tag}");
    }
    // The dropped per-language internal tags no longer appear.
    assert!(
        !map_a.contains_key("x-gmeow-japanese"),
        "per-language internal tags are dropped by the lang: graft"
    );
}

/// Carrier-surface coverage: `load_inverse_tag_map` over the REAL carrier surface
/// (grounding carrier varieties + generated `bcp47Tag` projection) recovers the
/// three project translation targets — English, French, and Mandarin.
///
/// This is a DATA audit that the inline unit test in `language_tags.rs`
/// (`load_inverse_tag_map_recovers_natural_tags`) cannot substitute for: that test
/// uses a 2-language synthetic fixture and asserts the LOGIC is correct. This test
/// asserts that the real carrier surface actually carries the three required
/// mappings. An authoring/generation error (missing generated `bcp47Tag`, wrong
/// tag, removed carrier variety) would break this test but leave the unit test green.
#[test]
fn inverse_tag_map_recovers_natural_internal_tags() {
    let inv = load_inverse_tag_map_from_dataset(authenticated_bundle_dataset())
        .expect("load_inverse_tag_map_from_dataset must succeed on the carrier surface");

    assert_eq!(
        inv.get("en"),
        Some(&"x-gmeow-english".to_owned()),
        "catalog inverse map must recover en → x-gmeow-english"
    );
    assert_eq!(
        inv.get("fr"),
        Some(&"x-gmeow-french".to_owned()),
        "catalog inverse map must recover fr → x-gmeow-french"
    );
    assert_eq!(
        inv.get("zh"),
        Some(&"x-gmeow-mandarin".to_owned()),
        "catalog inverse map must recover zh → x-gmeow-mandarin"
    );
}

/// Carrier-surface round-trip: `retag_graph_to_internal` using the carrier
/// surface's inverse map converts `@en` and `@zh` literals to `@x-gmeow-english`
/// and `@x-gmeow-mandarin` respectively; verifies the real carrier DATA drives the
/// graph-rewrite path.
///
/// This complements the unit-level `retag_graph_to_internal_lifts_public_tags` test
/// (which uses a synthetic 2-entry map) by asserting that the carrier-surface-derived
/// inverse map actually produces the correct internal tags on a concrete N-Triples
/// graph — exercising the end-to-end carrier-surface → inverse-map → retag path.
#[test]
fn retag_graph_to_internal_catalog_round_trip() {
    let inv = load_inverse_tag_map_from_dataset(authenticated_bundle_dataset())
        .expect("load_inverse_tag_map_from_dataset must succeed on the carrier surface");

    // Build a small N-Triples graph with one @en and one @zh literal.
    let nt = "<https://e/s> <https://e/label> \"Hello\"@en .\n\
              <https://e/s> <https://e/label> \"中文\"@zh .\n";

    let out = retag_graph_to_internal(nt.as_bytes(), "ntriples", &inv)
        .expect("retag_graph_to_internal must succeed");
    let text = String::from_utf8(out).expect("output must be valid UTF-8");

    assert!(
        text.contains("\"Hello\"@x-gmeow-english"),
        "@en must be retagged to @x-gmeow-english using catalog inverse map: {text}"
    );
    assert!(
        text.contains("\"中文\"@x-gmeow-mandarin"),
        "@zh must be retagged to @x-gmeow-mandarin using catalog inverse map: {text}"
    );
    assert!(
        !text.contains("\"Hello\"@en"),
        "original @en literal must not survive in output: {text}"
    );
    assert!(
        !text.contains("\"中文\"@zh"),
        "original @zh literal must not survive in output: {text}"
    );
}
