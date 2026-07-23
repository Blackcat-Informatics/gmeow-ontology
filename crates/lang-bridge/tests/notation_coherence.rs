// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The executable teeth of the GMN-1 cross-surface coherence law
//! (`gmeow:gmnNotationCoherenceLaw` / `gmeow:gmnCorrNotationCoherence`,
//! `slices/grounding/lang/module.ttl`).
//!
//! GMN-1 is ONE graph-derived notation whose glyph signature is presented through several
//! typed views (`gmeow:GmnNotationView`). This gate discharges the naturality square for the
//! tree-producing legs: it renders the graph-derived GMN glyph grammar from the carrier's
//! executable glyph registry and asserts that **every grammar formalism parses it back to the
//! ONE canonical `RuleExpr` tree**, and that the codec's single operator-precedence table
//! (`grammar.rs` `prec()`, re-exported as [`expr_precedence`]) is the strict ladder every
//! formalism's serializer shares — the reason the views can agree at all.
//!
//! Falsifiability: if a formalism rendered a divergent tree (its serializer or parser drifting
//! from the shared `RuleExpr` model), or the precedence ladder stopped being strictly monotone
//! (so a lower-binding node nested in a tighter context no longer round-trips), the
//! cross-formalism equality below reds. The `FORMALISMS` list is the single extension point:
//! when the GBNF and Lark serializers land (a later task adds `Formalism::Gbnf` /
//! `Formalism::Lark`), appending them here is the only change needed to hold those legs to the
//! same canonical tree.

use std::path::Path;

use gmeow_lang_bridge::{
    Formalism, GmnGlyphRegistry, Grammar, GrammarRule, RuleExpr, expr_precedence, parse_grammar,
    serialize_grammar,
};

/// The grammar formalisms whose surface legs must all parse back to ONE canonical tree — the
/// codec-precedence leg is asserted separately below. THIS is the single extension point:
/// when the GBNF / Lark serializers land, add `Formalism::Gbnf` / `Formalism::Lark` here and
/// every leg is held to the same canonical tree with no other change. The `gmn1-ecosystem`
/// ontology names all five tree-producing views now
/// (`gmeow:gmnViewCodec` / `Ebnf` / `Abnf` / `Gbnf` / `Lark`); the parse arms for GBNF/Lark
/// do not exist yet, so they are deliberately NOT referenced here.
const FORMALISMS: &[Formalism] = &[Formalism::Ebnf, Formalism::Abnf];

/// Read a file relative to the lang slice root.
fn lang_slice_file(rel: &str) -> Vec<u8> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../slices/grounding/lang")
        .join(rel);
    std::fs::read(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

/// Re-render one grammar's canonical rules under `formalism`, reparse, and return the canonical
/// rules — the leg of the naturality square for that surface. Compares RULES, not the whole
/// `Grammar`, so the comparison is formalism-independent (identity is the tree, never the
/// notation's spelling).
fn canonical_rules_via(base_rules: &[GrammarRule], formalism: Formalism) -> Vec<GrammarRule> {
    let view = Grammar {
        formalism,
        rules: base_rules.to_vec(),
    };
    let text = serialize_grammar(&view);
    let reparsed = parse_grammar(text.as_bytes(), formalism)
        .unwrap_or_else(|d| panic!("reparse under {formalism:?} failed: {d:?}\n{text}"));
    reparsed.canonicalize().rules
}

/// Every tree-producing view of the ONE glyph signature agrees on ONE canonical `RuleExpr`,
/// and the codec's operator-precedence table agrees with the grammar's precedence.
#[test]
fn all_notation_views_agree_on_one_canonical_tree() {
    // 1. The graph-derived glyph grammar: rendered SOLELY from the carrier's executable glyph
    //    registry (the same `render_glyph_token_production` the projection stage renders into
    //    `gmeow:gmnGrammar`), never a hand-listed inventory.
    let module = lang_slice_file("module.ttl");
    let dataset =
        purrdf::parse_dataset(&module, "text/turtle", None).expect("parse lang module.ttl");
    let registry = GmnGlyphRegistry::from_dataset(&dataset)
        .unwrap_or_else(|e| panic!("build the graph-derived glyph registry: {}", e.0));
    let production = registry.render_glyph_token_production();
    assert!(
        production.starts_with("glyphToken ::= '"),
        "the graph-derived glyph grammar must be a non-empty closed production, got: {production}"
    );

    // 2. The ONE canonical tree of the graph-derived glyph grammar (parsed via the authoring
    //    formalism, EBNF).
    let base = parse_grammar(production.as_bytes(), Formalism::Ebnf)
        .expect("parse the graph-derived glyph grammar as EBNF")
        .canonicalize();
    assert!(
        !base.rules.is_empty(),
        "the graph-derived glyph grammar must carry at least one production"
    );

    // 3. Naturality: every formalism leg parses back to that ONE canonical tree.
    for &formalism in FORMALISMS {
        let via = canonical_rules_via(&base.rules, formalism);
        assert_eq!(
            via, base.rules,
            "the {formalism:?} view of the graph-derived glyph grammar diverged from the \
             canonical RuleExpr tree — the cross-surface coherence law is broken"
        );
    }

    // 4. A precedence-stressing, cross-formalism-expressible grammar makes the shared
    //    precedence ladder genuinely load-bearing: `(a | b) c` and `(a | b)*` and `(a b)?`
    //    only round-trip through EVERY formalism when the serializer inserts a grouping around
    //    a lower-binding child — i.e. when `prec(Alt) < prec(Seq)` and `prec(Alt) < prec(*)`
    //    and `prec(Seq) < prec(?)`. A wrong ladder reparses `b | c d` as `Alt[b, Seq[c, d]]`
    //    and reds this. Every construct here is cross-formalism-expressible (no char class, no
    //    EBNF difference).
    let stress = parse_grammar(
        b"seqalt ::= (a | b) c\nstaralt ::= (a | b)*\noptseq ::= (a b)?\n",
        Formalism::Ebnf,
    )
    .expect("parse the precedence-stress grammar as EBNF")
    .canonicalize();
    for &formalism in FORMALISMS {
        let via = canonical_rules_via(&stress.rules, formalism);
        assert_eq!(
            via, stress.rules,
            "the {formalism:?} view disagreed on the precedence-stress grammar — the codec's \
             operator-precedence table does not agree with the grammar's precedence"
        );
    }

    // 5. The codec-precedence leg, read directly off the codec's operator-precedence table
    //    (`grammar.rs` `prec()`): the SINGLE ladder every formalism's serializer shares. It
    //    must be the strict monotone order the grouping discipline depends on —
    //    Alt < Seq < Diff < repetition < atoms — with all repetition nodes at one rung and all
    //    atoms at the tightest rung. This is what the round-trips above rely on; asserting it
    //    directly makes a ladder regression a localized, named failure.
    let alt = expr_precedence(&RuleExpr::Alt(Vec::new()));
    let seq = expr_precedence(&RuleExpr::Seq(Vec::new()));
    let diff = expr_precedence(&RuleExpr::Diff(
        Box::new(RuleExpr::Ref("a".into())),
        Box::new(RuleExpr::Ref("b".into())),
    ));
    let star = expr_precedence(&RuleExpr::Star(Box::new(RuleExpr::Ref("a".into()))));
    let plus = expr_precedence(&RuleExpr::Plus(Box::new(RuleExpr::Ref("a".into()))));
    let opt = expr_precedence(&RuleExpr::Opt(Box::new(RuleExpr::Ref("a".into()))));
    let repeat = expr_precedence(&RuleExpr::Repeat(
        Some(2),
        Some(4),
        Box::new(RuleExpr::Ref("a".into())),
    ));
    let reff = expr_precedence(&RuleExpr::Ref("a".into()));
    let term = expr_precedence(&RuleExpr::Terminal("a".into()));
    let class = expr_precedence(&RuleExpr::CharClass("a-z".into()));
    let group = expr_precedence(&RuleExpr::Group(Box::new(RuleExpr::Ref("a".into()))));

    assert!(
        alt < seq && seq < diff && diff < star && star < reff,
        "codec operator-precedence ladder must be strictly Alt < Seq < Diff < repetition < atom \
         (got Alt={alt} Seq={seq} Diff={diff} rep={star} atom={reff})"
    );
    assert!(
        star == plus && plus == opt && opt == repeat,
        "every repetition node must share one precedence rung (Star={star} Plus={plus} \
         Opt={opt} Repeat={repeat})"
    );
    assert!(
        reff == term && term == class && class == group,
        "every atom must share the tightest precedence rung (Ref={reff} Terminal={term} \
         CharClass={class} Group={group})"
    );
}
