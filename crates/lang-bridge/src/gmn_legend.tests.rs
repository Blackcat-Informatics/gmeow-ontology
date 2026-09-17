// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

#[test]
fn an_unpinned_glyph_is_a_named_hard_error() {
    let err = pinned_glyph_token_cost("⊗").expect_err("an unpinned glyph must not price");
    assert_eq!(err.code(), GmnUnpinnedGlyphCost::register());
    assert!(err.to_string().contains('⊗'), "{err}");
}

#[test]
fn every_pinned_cost_is_positive() {
    for (glyph, cost) in GLYPH_TOKEN_COSTS {
        assert!(*cost > 0, "glyph {glyph:?} is pinned at a zero token cost");
    }
}

#[test]
fn the_pinned_table_carries_no_duplicate_glyph() {
    let mut seen: Vec<&str> = GLYPH_TOKEN_COSTS.iter().map(|(g, _)| *g).collect();
    let before = seen.len();
    seen.sort_unstable();
    seen.dedup();
    assert_eq!(before, seen.len(), "GLYPH_TOKEN_COSTS repeats a glyph");
}

#[test]
fn an_unpinned_audit_token_is_a_named_hard_error() {
    let err = pinned_symbol_audit_token_cost("⊗")
        .expect_err("a token outside the audited inventory must not price");
    assert_eq!(err.code(), GmnUnpinnedGlyphCost::register());
    assert!(err.to_string().contains('⊗'), "{err}");
}

#[test]
fn every_pinned_audit_cost_is_positive() {
    for (token, cost) in GMN_SYMBOL_AUDIT_TOKEN_COSTS {
        assert!(*cost > 0, "token {token:?} is pinned at a zero token cost");
    }
}

#[test]
fn the_pinned_audit_table_carries_no_duplicate_token() {
    let mut seen: Vec<&str> = GMN_SYMBOL_AUDIT_TOKEN_COSTS
        .iter()
        .map(|(t, _)| *t)
        .collect();
    let before = seen.len();
    seen.sort_unstable();
    seen.dedup();
    assert_eq!(
        before,
        seen.len(),
        "GMN_SYMBOL_AUDIT_TOKEN_COSTS repeats a token"
    );
}

/// The audit table must be a genuine SUPERSET of the shipped codec legend: the two
/// answer different questions, but every glyph the codec may emit is also a glyph the
/// audit can be asked to weigh, and the two tables must never disagree about a cost.
#[test]
fn the_audit_table_is_a_consistent_superset_of_the_codec_legend() {
    for (glyph, cost) in GLYPH_TOKEN_COSTS {
        let audited = pinned_symbol_audit_token_cost(glyph).unwrap_or_else(|e| {
            panic!("GLYPH_TOKEN_COSTS prices {glyph:?} but the audit table does not: {e}")
        });
        assert_eq!(
            *cost, audited,
            "the two pinned tables disagree about {glyph:?}"
        );
    }
}
