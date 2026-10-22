// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

const TINY_REGISTRY: &str = include_str!("../tests/fixtures/glyph-legend.ttl");

#[test]
fn json_glyph_legend_shared_value_and_string_pin_literal_rows_costs_and_order() {
    let dataset = purrdf::parse_dataset(TINY_REGISTRY.as_bytes(), "text/turtle", None).unwrap();
    let registry = GmnGlyphRegistry::from_dataset(&dataset).unwrap();
    let expected =
        r#"[{"glyph":"+","tokenCost":1},{"glyph":"¬","tokenCost":1},{"glyph":"⊑","tokenCost":3}]"#;
    assert_eq!(registry.glyph_tokens(), ["+", "¬", "⊑"]);
    assert_eq!(glyph_legend_json(&registry).unwrap(), expected);
    assert_eq!(
        purrdf_lex::json::write_compact(&glyph_legend_value(&registry).unwrap()),
        expected
    );
    assert_eq!(
        glyph_legend_json(&registry).unwrap(),
        glyph_legend_json(&registry).unwrap()
    );
}

#[test]
fn json_glyph_legend_empty_admitted_registry_has_no_fabricated_rows() {
    let empty = TINY_REGISTRY.split_once("# Executable bindings").unwrap().0;
    let dataset = purrdf::parse_dataset(empty.as_bytes(), "text/turtle", None).unwrap();
    let registry = GmnGlyphRegistry::from_dataset(&dataset).unwrap();
    assert!(registry.glyph_tokens().is_empty());
    assert_eq!(glyph_legend_json(&registry).unwrap(), "[]");
    assert_eq!(
        glyph_legend_value(&registry).unwrap(),
        Value::Array(Vec::new())
    );
}

#[test]
fn json_glyph_legend_missing_pin_is_original_named_refusal_on_both_exits() {
    let unpinned = TINY_REGISTRY.replace("U+2291", "U+2297");
    let dataset = purrdf::parse_dataset(unpinned.as_bytes(), "text/turtle", None).unwrap();
    let registry = GmnGlyphRegistry::from_dataset(&dataset).unwrap();
    assert!(registry.glyph_tokens().contains(&"⊗"));
    for error in [
        glyph_legend_value(&registry).unwrap_err(),
        glyph_legend_json(&registry).unwrap_err(),
    ] {
        assert_eq!(error.code(), GmnUnpinnedGlyphCost::register());
        assert!(error.is::<GmnUnpinnedGlyphCost>());
        assert!(error.to_string().contains('⊗'));
    }
}

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
