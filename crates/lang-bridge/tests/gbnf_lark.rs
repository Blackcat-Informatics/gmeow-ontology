// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The teeth for the GBNF (llama.cpp constrained-decode) and Lark grammar formalisms as PURE
//! surface renderings of the ONE shared `RuleExpr` structural tree (`grammar.rs`).
//!
//! Two claims are held here:
//!
//! 1. **Real round-trip.** For each of GBNF and Lark, over representative grammars (including the
//!    precedence-stressing alternation / sequence / repetition cases), the surface leg parses
//!    back to the SAME canonical tree: `parse(serialize(view)).canonicalize() ==
//!    view.canonicalize()`. The parse is real — never faked — so a serializer or parser drifting
//!    from the shared model reds this.
//!
//! 2. **Honest blocking, no fabrication.** A grammar carrying a construct a formalism cannot
//!    represent (GBNF: the set-difference operator, or LEFT-RECURSION the constrained decoder
//!    cannot enter) yields an ENUMERATED blocking entry and NO artifact — never a fabricated
//!    best-effort rendering. Lark's blocking set is genuinely narrower (its Earley/LALR core
//!    handles left-recursion), which the same grammars make falsifiable.

use gmeow_lang_bridge::{
    Formalism, Grammar, GrammarRule, distinguished_rule, gbnf_blocking_constructs,
    lark_blocking_constructs, parse_grammar, serialize_grammar,
};

/// Parse an EBNF authoring text into the ONE canonical grammar (the shared tree the surface
/// views render). The authoring formalism is irrelevant to identity — only the tree matters.
fn base(ebnf: &str) -> Grammar {
    parse_grammar(ebnf.as_bytes(), Formalism::Ebnf)
        .unwrap_or_else(|d| panic!("author base grammar as EBNF: {d:?}\n{ebnf}"))
        .canonicalize()
}

/// The representative, cross-formalism-expressible grammars — every construct here is within
/// BOTH the GBNF and Lark surfaces (refs, string literals, character classes, alternation,
/// sequence, `*` / `+` / `?`, and precedence-forcing groups). No set-difference, no bare hex,
/// no bounded repetition, no left-recursion, so neither formalism blocks them.
fn representative_rules() -> Vec<Vec<GrammarRule>> {
    vec![
        // Precedence stress: a lower-binding child nested in a tighter context only round-trips
        // when the serializer inserts a grouping — `prec(Alt) < prec(Seq)`, `< prec(*)`, and
        // `prec(Seq) < prec(?)`. This is the exact stress the coherence gate uses.
        base("seqalt ::= (a | b) c\nstaralt ::= (a | b)*\noptseq ::= (a b)?\na ::= 'x'\nb ::= 'y'\nc ::= 'z'\n").rules,
        // Character class + repetition (GBNF `[…]`, Lark `/[…]/`).
        base("ident ::= [a-zA-Z_] [a-zA-Z0-9_]*\n").rules,
        // Alternation of literals under a `+`.
        base("digits ::= digit+\ndigit ::= '0' | '1' | '2'\n").rules,
        // Ref-heavy, RIGHT-recursive (GBNF-safe: the leftmost element is `item`, never `list`).
        base("list ::= item list?\nitem ::= 'x'\n").rules,
        // Nested repetition over a grouped sequence: `term (op term)*`.
        base("expr ::= term (op term)*\nterm ::= 'n'\nop ::= '+' | '-'\n").rules,
    ]
}

/// One surface leg of the naturality square: render the canonical rules under `formalism`,
/// reparse, and require the reparsed canonical grammar to equal the canonical view — the REAL
/// round-trip, not a declared one.
fn assert_round_trip(base_rules: &[GrammarRule], formalism: Formalism) {
    let view = Grammar {
        formalism,
        rules: base_rules.to_vec(),
    };
    let canon = view.canonicalize();
    let text = serialize_grammar(&canon);
    let reparsed = parse_grammar(text.as_bytes(), formalism)
        .unwrap_or_else(|d| panic!("reparse under {formalism:?} failed: {d:?}\n{text}"));
    assert_eq!(
        reparsed.canonicalize(),
        canon,
        "the {formalism:?} surface leg diverged from the shared canonical RuleExpr tree:\n{text}"
    );
}

#[test]
fn gbnf_and_lark_round_trip_via_shared_tree() {
    for rules in representative_rules() {
        // Guard the premise: a representative grammar blocks under NEITHER formalism, so the
        // round-trip below is genuinely exercising the surface, not a vacuous skip.
        let canon = Grammar {
            formalism: Formalism::Ebnf,
            rules: rules.clone(),
        }
        .canonicalize();
        assert!(
            gbnf_blocking_constructs(&canon).is_empty(),
            "representative grammar unexpectedly blocks under GBNF: {:?}",
            gbnf_blocking_constructs(&canon)
        );
        assert!(
            lark_blocking_constructs(&canon).is_empty(),
            "representative grammar unexpectedly blocks under Lark: {:?}",
            lark_blocking_constructs(&canon)
        );
        assert_round_trip(&rules, Formalism::Gbnf);
        assert_round_trip(&rules, Formalism::Lark);
    }
}

/// The GBNF (and Lark) render decision, mirroring the registry's ABNF gate: a blocked grammar
/// emits NO artifact — a partial rendering would be fabrication.
fn render_or_block(canon: &Grammar, formalism: Formalism) -> Result<String, Vec<String>> {
    let blocking = match formalism {
        Formalism::Gbnf => gbnf_blocking_constructs(canon),
        Formalism::Lark => lark_blocking_constructs(canon),
        other => panic!("render_or_block covers GBNF/Lark, not {other:?}"),
    };
    if blocking.is_empty() {
        Ok(serialize_grammar(&Grammar {
            formalism,
            rules: canon.rules.clone(),
        }))
    } else {
        Err(blocking)
    }
}

#[test]
fn gbnf_left_recursion_is_soundunder_not_fabricated() {
    // Directly left-recursive: `expr` is reachable from its own left edge.
    let left_rec = base("expr ::= expr '+' term | term\nterm ::= 'x'\n");

    // GBNF: an honest, enumerated blocking entry naming the left-recursive rule — and NO
    // artifact (SoundUnder, never a fabricated rendering a constrained decoder cannot expand).
    let gbnf_blocking = gbnf_blocking_constructs(&left_rec);
    assert!(
        gbnf_blocking
            .iter()
            .any(|b| b.contains("expr") && b.contains("left recursion")),
        "GBNF must name the left-recursive rule 'expr' as a blocker, got: {gbnf_blocking:?}"
    );
    match render_or_block(&left_rec, Formalism::Gbnf) {
        Err(blockers) => assert_eq!(
            blockers, gbnf_blocking,
            "the GBNF render gate must refuse with exactly the enumerated blockers"
        ),
        Ok(text) => panic!("GBNF must emit NO artifact for a left-recursive grammar; got:\n{text}"),
    }

    // Lark's blocking set is NARROWER: its Earley/LALR core handles left-recursion, so the SAME
    // grammar blocks under NOTHING and round-trips through the Lark surface.
    assert!(
        lark_blocking_constructs(&left_rec).is_empty(),
        "Lark handles left recursion — it must not block this grammar: {:?}",
        lark_blocking_constructs(&left_rec)
    );
    assert!(
        render_or_block(&left_rec, Formalism::Lark).is_ok(),
        "Lark must render the left-recursive grammar (no fabrication concern — it is expressible)"
    );
    assert_round_trip(&left_rec.rules, Formalism::Lark);

    // The set-difference operator (`A - B`) is a blocker for BOTH surfaces — neither has a
    // difference operator — and again yields no artifact.
    let diff_grammar = base("body ::= char - '\"'\nchar ::= [a-z]\n");
    for formalism in [Formalism::Gbnf, Formalism::Lark] {
        let blockers = match render_or_block(&diff_grammar, formalism) {
            Err(b) => b,
            Ok(text) => panic!("{formalism:?} must emit NO artifact for a difference-bearing grammar:\n{text}"),
        };
        assert!(
            blockers.iter().any(|b| b.contains("set-difference")),
            "{formalism:?} must name the set-difference operator as a blocker, got: {blockers:?}"
        );
    }

    // Falsifiable contrast: a clean grammar blocks under NEITHER surface and renders.
    let clean = base("greeting ::= 'hello' name\nname ::= [A-Za-z]+\n");
    assert!(gbnf_blocking_constructs(&clean).is_empty());
    assert!(lark_blocking_constructs(&clean).is_empty());
    assert!(render_or_block(&clean, Formalism::Gbnf).is_ok());
    assert!(render_or_block(&clean, Formalism::Lark).is_ok());
}

#[test]
fn distinguished_rule_is_the_unreferenced_entry_led_first() {
    // `document` is referenced by no other rule; `word` is. The entry is the unreferenced one.
    let g = base("document ::= word+\nword ::= [a-z]+\n");
    assert_eq!(distinguished_rule(&g), "document");

    // GBNF/Lark serialization leads with the distinguished entry (an ordering choice; identity
    // is still the sorted canonical tree).
    for formalism in [Formalism::Gbnf, Formalism::Lark] {
        let view = Grammar {
            formalism,
            rules: g.rules.clone(),
        };
        let text = serialize_grammar(&view);
        let first_line = text.lines().next().expect("at least one rule line");
        assert!(
            first_line.starts_with("document"),
            "{formalism:?} must lead with the distinguished entry rule, got: {first_line}"
        );
    }

    // A tie (no rule references another) breaks to the lexicographically-first name.
    let tie = base("beta ::= 'b'\nalpha ::= 'a'\n");
    assert_eq!(distinguished_rule(&tie), "alpha");
}
