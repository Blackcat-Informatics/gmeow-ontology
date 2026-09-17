// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use crate::conllu::{ConlluDoc, ConlluSentence, ConlluToken, TokenId, serialize};

/// Build a one-token document whose every string column carries its own column NAME, so
/// the serializer's tab-joined output spells out the column order it emits.
fn column_probe_line() -> Vec<String> {
    let token = ConlluToken {
        id: TokenId::Simple(1),
        form: "FORM".to_owned(),
        lemma: "LEMMA".to_owned(),
        upos: "UPOS".to_owned(),
        xpos: "XPOS".to_owned(),
        feats: "FEATS".to_owned(),
        head: "HEAD".to_owned(),
        deprel: "DEPREL".to_owned(),
        deps: "DEPS".to_owned(),
        misc: "MISC".to_owned(),
    };
    let doc = ConlluDoc {
        sentences: vec![ConlluSentence {
            comments: vec![],
            tokens: vec![token],
        }],
    };
    let bytes = serialize(&doc);
    let text = String::from_utf8(bytes).expect("CoNLL-U serialization is UTF-8");
    let first_line = text.lines().next().expect("one token line");
    first_line.split('\t').map(str::to_owned).collect()
}

/// The `@λ` column pin: `GMN_LANG_AST_COLUMNS` must equal the order the CoNLL-U serializer
/// emits columns in. Column 0 is the ID slot (the serializer emits the numeric ID there);
/// columns 1..10 are the named string columns, which must match position-for-position. If
/// anyone reorders `ConlluToken`'s fields, the serializer's output reorders and this fails.
#[test]
fn lang_ast_columns_match_conllu_serializer() {
    let emitted = column_probe_line();
    assert_eq!(emitted.len(), 10, "CoNLL-U emits exactly ten columns");
    assert_eq!(GMN_LANG_AST_COLUMNS[0], "ID", "column 0 is the ID slot");
    assert_eq!(
        emitted[0], "1",
        "the serializer emits the numeric ID in column 0"
    );
    assert_eq!(
        &emitted[1..],
        &GMN_LANG_AST_COLUMNS[1..],
        "the @λ column ruling must reuse the CoNLL-U column order verbatim"
    );
}

/// `⟦` (U+27E6) and `⟧` (U+27E7) each fragment under the pinned BPE — the observation the
/// issue's "token-benchmark before adopting" demands.
#[test]
fn denotation_brackets_fragment() {
    assert!(
        gmn_glyph_token_cost("⟦") > 1,
        "⟦ (U+27E6) fragments into more than one token"
    );
    assert!(
        gmn_glyph_token_cost("⟧") > 1,
        "⟧ (U+27E7) fragments into more than one token"
    );
}

/// The ⟦·⟧-denotation ruling, read off the real measurement: the bracket pair costs more
/// than the one-token `den` named key, so the denotation term is dispositioned to the key
/// and the brackets never enter the glyph table. This is the executable form of the ruling
/// the DSL authors as `den` with no ⟦/⟧ graphemes.
#[test]
fn bracket_pair_costs_more_than_named_key() {
    let brackets = gmn_glyph_token_cost("⟦") + gmn_glyph_token_cost("⟧");
    let named_key = gmn_glyph_token_cost("den");
    assert!(
        brackets > named_key,
        "⟦·⟧ (measured {brackets} tokens) must cost more than the named key `den` \
             (measured {named_key} tokens) — hence the named-key disposition"
    );
}

/// The measurement-driven disposition for the other two proposed symbols. The rule is
/// crisp and executable: a GMN operator/relation symbol earns a glyph slot iff it costs a
/// SINGLE token — anything more is dearer than a one-token named key and is dispositioned
/// to the key. `*` (U+002A) is one token → it earns a glyph; `⇝` (U+21DD) fragments to more
/// than one → the translation-leg term stays a named key, exactly as ⟦·⟧ does.
#[test]
fn ungrammatical_earns_glyph_translation_stays_named_key() {
    assert_eq!(
        gmn_glyph_token_cost("*"),
        1,
        "`*` (U+002A) is a single token — it earns a glyph slot"
    );
    assert!(
        gmn_glyph_token_cost("⇝") > 1,
        "`⇝` (U+21DD) costs more than one token — the translation-leg term stays a named key"
    );
}

/// Every token-cost disposition shipped by dict-v3 is reproduced from the same pinned
/// tokenizer the quality axis calls. An adopted glyph may tie or beat its fallback,
/// while a named key must strictly beat the Unicode display notation it replaces.
#[test]
fn grounding_symbol_dispositions_match_pinned_bpe_costs() {
    for (glyph, fallback) in [
        ("¬", "not"),
        ("π", "pi"),
        ("γ", "gamma"),
        ("+", "add"),
        ("×", "mul"),
        ("^", "pow"),
        ("*", "ungrammatical"),
    ] {
        let glyph_cost = gmn_glyph_token_cost(glyph);
        let fallback_cost = gmn_glyph_token_cost(fallback);
        assert!(
            glyph_cost <= fallback_cost,
            "adopted glyph {glyph:?} costs {glyph_cost}, above fallback {fallback:?} ({fallback_cost})"
        );
    }

    for (glyph, fallback) in [
        ("⟦·⟧", "den"),
        ("⇝", "xl"),
        ("÷", "div"),
        ("⊕", "ds"),
        ("⌟", "lcon"),
        ("∀", "fa"),
        ("∃", "ex"),
        ("∧", "and"),
        ("∨", "or"),
        ("↔", "iff"),
    ] {
        let glyph_cost = gmn_glyph_token_cost(glyph);
        let fallback_cost = gmn_glyph_token_cost(fallback);
        assert!(
            glyph_cost > fallback_cost,
            "named key {fallback:?} ({fallback_cost}) does not beat display glyph {glyph:?} ({glyph_cost})"
        );
    }
}

/// The worked IPA phone symbols carry measured, non-zero costs — the feed the authored
/// `math:Quantity` per-glyph cost is cross-checked against.
#[test]
fn ipa_symbols_have_measured_cost() {
    for sym in ["k", "æ", "t", "ˈ"] {
        assert!(
            gmn_glyph_token_cost(sym) >= 1,
            "IPA symbol {sym:?} has a measured, non-zero token cost"
        );
    }
}

/// The measurement is deterministic: the pinned, embedded vocabulary yields the same count
/// for the same string every time — the property the cost feed's reproducibility rests on.
#[test]
fn cost_is_deterministic() {
    for sym in ["⟦", "⟧", "den", "*", "⇝", "æ", "ˈ"] {
        let a = gmn_glyph_token_cost(sym);
        let b = gmn_glyph_token_cost(sym);
        assert_eq!(a, b, "token cost of {sym:?} is stable across calls");
    }
}
