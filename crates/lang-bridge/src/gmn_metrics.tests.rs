// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

#[test]
fn selected_reductions_preserve_integer_rounding_and_source_denominators() {
    let measured = MeasuredTokenCorpus {
        sources: vec![
            (
                0,
                MetricTotals {
                    total_sources: 1,
                    valid_sources: 1,
                    roundtrip_sources: 1,
                    gmn_bytes: 4,
                    gmn_ascii_bytes: 1,
                    gmn_nonascii_bytes: 3,
                    gmn_chars: 2,
                    tokens_in_context: 1,
                    glyph_chars: 1,
                    turtle_bytes: 20,
                    turtle_chars: 13,
                    jsonld_bytes: 25,
                    covered_quads: 1,
                    total_quads: 1,
                },
            ),
            (
                2,
                MetricTotals {
                    total_sources: 1,
                    valid_sources: 1,
                    roundtrip_sources: 1,
                    gmn_bytes: 7,
                    gmn_ascii_bytes: 1,
                    gmn_nonascii_bytes: 6,
                    gmn_chars: 3,
                    tokens_in_context: 1,
                    glyph_chars: 2,
                    turtle_bytes: 30,
                    turtle_chars: 17,
                    jsonld_bytes: 35,
                    covered_quads: 1,
                    total_quads: 3,
                },
            ),
            (
                3,
                MetricTotals {
                    total_sources: 1,
                    covered_quads: 2,
                    total_quads: 6,
                    ..MetricTotals::default()
                },
            ),
        ],
    };
    let all = measured.aggregate();
    assert_eq!(all.total_sources, 3);
    assert_eq!(all.measured_sources, 2);
    assert_eq!(all.bytes_on_disk, 11);
    assert_eq!(
        all.gmn_worst_case_tokens, 10,
        "round after reducing ASCII bytes"
    );
    assert_eq!(
        all.turtle_best_case_tokens, 8,
        "round after reducing Turtle characters"
    );
    assert_eq!(
        all.dictionary_hit_rate, 0.4,
        "weight by quads rather than source count"
    );
    assert_eq!(all.ast_validity_rate, 2.0 / 3.0);
    assert_eq!(all.roundtrip_loss, 1.0 - 2.0 / 3.0);
    let selected = measured.selected(|index| index == 2);
    assert_eq!(selected.total_sources, 1);
    assert_eq!(selected.bytes_on_disk, 7);
    assert_eq!(selected.dictionary_hit_rate, 1.0 / 3.0);
    let absent = measured.selected(|index| index == 1);
    assert_eq!(
        absent.total_sources, 0,
        "original parse-domain positions are retained"
    );
    assert!(!absent.compression_gate_holds());
}

#[test]
fn estimate_tokens_is_chars_over_four_rounded_up() {
    assert_eq!(estimate_tokens(""), 0, "empty text costs no tokens");
    assert_eq!(estimate_tokens("abcd"), 1, "exactly four chars ⇒ one token");
    assert_eq!(estimate_tokens("abcde"), 2, "five chars round up to two");
    // Counted by CHARACTER, not UTF-8 byte: four 3-byte glyphs are four chars ⇒ one token,
    // never twelve. The byte-fallback penalty lives in the worst-case bound, not here.
    assert_eq!(
        estimate_tokens("→→→→"),
        1,
        "four multibyte glyphs are four chars"
    );
}

#[test]
fn ratio_is_bounded_with_a_vacuous_denominator_perfect() {
    assert_eq!(
        ratio(0, 0),
        1.0,
        "0/0 is trivially perfect (nothing to fail)"
    );
    assert_eq!(ratio(0, 5), 0.0);
    assert_eq!(ratio(1, 2), 0.5);
    assert_eq!(ratio(3, 3), 1.0);
}

#[test]
fn count_glyph_chars_is_greedy_longest_match() {
    // Longest-first ordering: "→→" wins over "→" where a position starts "→→".
    let glyphs = ["→→", "→"];
    // "a→→b→c": glyph-covered = the two chars in "→→" plus the one "→" = 3; a/b/c are not.
    assert_eq!(count_glyph_chars("a→→b→c", &glyphs), 3);
    // No glyph matches ⇒ zero coverage; an empty glyph string is ignored (never matches).
    assert_eq!(count_glyph_chars("abc", &[""]), 0);
    assert_eq!(count_glyph_chars("", &glyphs), 0);
}

/// A `TokenMetrics` carrying only the gate-relevant witnesses; every field
/// `compression_gate_holds` does not read is zero.
fn gate_witnesses(
    measured_sources: u64,
    gmn_worst_case_tokens: u64,
    turtle_best_case_tokens: u64,
) -> TokenMetrics {
    TokenMetrics {
        bytes_on_disk: 0,
        tokens_in_context: 0,
        ast_validity_rate: 0.0,
        roundtrip_loss: 0.0,
        compression_ratio: 0.0,
        glyph_density: 0.0,
        dictionary_hit_rate: 0.0,
        gmn_worst_case_tokens,
        gmn_realistic_tokens: 0,
        turtle_best_case_tokens,
        gmn_ascii_bytes: 0,
        gmn_nonascii_bytes: 0,
        turtle_bytes_on_disk: 0,
        jsonld_bytes_on_disk: 0,
        total_sources: 0,
        measured_sources,
    }
}

#[test]
fn compression_gate_requires_strict_worst_below_best_over_a_nonvacuous_corpus() {
    // Holds only when GMN's byte-fallback worst case is STRICTLY below Turtle's best case.
    assert!(
        gate_witnesses(1, 14_027, 23_695).compression_gate_holds(),
        "worst < best ⇒ the gate holds"
    );
    assert!(
        !gate_witnesses(1, 23_695, 23_695).compression_gate_holds(),
        "worst == best ⇒ does NOT hold (the comparison is strict)"
    );
    assert!(
        !gate_witnesses(1, 23_696, 23_695).compression_gate_holds(),
        "worst > best ⇒ does NOT hold"
    );
    // A vacuous corpus (no round-tripping source) never holds, even with worst < best:
    // there is no compression claim to make.
    assert!(
        !gate_witnesses(0, 1, 1_000_000).compression_gate_holds(),
        "vacuous corpus ⇒ no claim, gate does not hold"
    );
}
