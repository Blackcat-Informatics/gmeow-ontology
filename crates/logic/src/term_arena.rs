// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The shared term arena's façade, plus the `math:`-graph interning wrapper.
//!
//! # What lives here and why
//!
//! The arena itself is [`gmeow_term_arena`] — a reasoner-free substrate crate, so a parser
//! front-end can intern terms without linking this runtime. Its façade
//! ([`ContentKey`](gmeow_term_arena::ContentKey),
//! [`TermArena`](gmeow_term_arena::TermArena),
//! [`StructNode`](gmeow_term_arena::StructNode),
//! [`InterningStats`](gmeow_term_arena::InterningStats)) is re-exported below so a
//! consumer that already depends on `gmeow-logic` names one surface.
//!
//! `intern_math_root` is the one addition. It writes NO interning logic — it composes the
//! existing [`MathGraph`](crate::physical::lower) parse, the existing
//! [`lower_math_expression`](crate::physical::lower) lowering, and the arena's own content key,
//! and it is what the shipped `math:structuralKey` digest is computed through.
//!
//! ## Why the lowering seam is HERE and not in `gmeow-term-arena`
//!
//! `math:` expressions have no typed Rust AST: the expression tree **is** an RDF graph, so
//! lowering one means parsing Turtle and walking a `purrdf` dataset. That lowering
//! (`crate::physical::lower`) additionally consumes `gmeow_logic_compile::ir` (for the
//! `logic:` consumer it shares its binder-frame machinery with), `gmeow_lang_form` (for the
//! `lang:` consumer), and `gmeow_errors` (for its typed diagnostics). Moving it into
//! `gmeow-term-arena` would therefore drag the compiler IR, the form AST, and the
//! diagnostics substrate into a crate whose entire purpose is to carry none of them — and
//! splitting only the `math:` arm out of a three-consumer lowering would fork the shared
//! de-Bruijn/binder-frame code into two copies.
//!
//! So the seam lives in the crate that ALREADY has the `purrdf` + `MathGraph` edge, and
//! `gmeow-term-arena` stays minimal.

use gmeow_term_arena::engine::ArenaAccess;

pub use gmeow_term_arena::{
    Arena, ArenaSnapshot, ContentKey, ForeignNode, InterningStats, StructNode, TermArena,
};

use crate::physical::lower::{MathGraph, MathResult, lower_math_expression};

/// Intern the expression rooted at `root` of an ALREADY-PARSED [`MathGraph`], keeping the typed
/// [`MathLoweringError`](crate::physical::lower::MathLoweringError) algebra intact.
///
/// This is the seam the shipped structural-key computation
/// ([`crate::physical::lower::math_expression_structural_keys`]) runs on: the digest the ontology
/// publishes IS the [`ContentKey`] this arena mints, folded to fixed width — the same bytes by
/// construction rather than by two implementations agreeing.
///
/// Crate-visible, and there is no public Turtle-bytes wrapper beside it. One existed and had no
/// caller outside its own tests; a second entry point into the same lowering, reachable by nobody,
/// is exactly the duplicate surface the greenfield rule says to delete rather than document.
///
/// # Errors
///
/// The lowering's own typed rejection — every variant carries the `math:` failure class it
/// denotes, which is what makes an unliftable expression reportable rather than merely absent.
///
/// # Panics
///
/// If `arena` rejects a node its OWN backing DAG just minted. That is an internal invariant
/// violation, not an input condition, so it is never folded into the typed error algebra (where
/// it would mint a failure class no fixture could ever exhibit).
pub(crate) fn intern_math_root(
    arena: &mut TermArena,
    graph: &MathGraph,
    root: &str,
) -> MathResult<(StructNode, ContentKey)> {
    let node = lower_math_expression(arena.dag_mut(), graph, root)?;
    // `brand_node`/`key` can only fail for a node minted by a DIFFERENT arena; `node` was just
    // minted by `arena.dag_mut()`, so a failure here is a broken arena, not bad input.
    let handle = arena
        .brand_node(node)
        .expect("a node this arena's own DAG just minted is one of its live slots");
    let key = arena
        .key(handle)
        .expect("a node this arena's own DAG just minted is one of its live slots");
    Ok((handle, key))
}

#[cfg(test)]
mod tests {
    use super::*;

    const MATH: &str = "https://blackcatinformatics.ca/math/";

    /// `plus(1, 2)` as a `math:ApplicationExpression`, and the same expression authored a
    /// second time with different blank-node labels and reversed slot order.
    /// Parse `turtle` and intern the expression rooted at `root` — the two steps the deleted
    /// Turtle-bytes wrapper used to bundle, spelled out at the one place that needs them.
    fn intern_turtle(
        arena: &mut TermArena,
        turtle: &[u8],
        root: &str,
    ) -> gmeow_errors::Result<(StructNode, ContentKey)> {
        let graph = crate::physical::lower::MathGraph::from_turtle(turtle)?;
        Ok(intern_math_root(arena, &graph, root)?)
    }

    fn application_turtle(labels: (&str, &str, &str), reversed: bool) -> Vec<u8> {
        let (root, s0, s1) = labels;
        let slots = if reversed {
            format!("_:{s1}, _:{s0}")
        } else {
            format!("_:{s0}, _:{s1}")
        };
        format!(
            r#"@prefix math: <{MATH}> .
@prefix xsd: <http://www.w3.org/2001/XMLSchema#> .

_:{root} a math:ApplicationExpression ;
    math:operator <https://example.org/plus> ;
    math:argumentSlot {slots} .

_:{s0} a math:ArgumentSlot ;
    math:slotIndex 0 ;
    math:slotExpression _:{s0}v .
_:{s0}v a math:NumberLiteral ; math:literalValue "1"^^xsd:integer .

_:{s1} a math:ArgumentSlot ;
    math:slotIndex 1 ;
    math:slotExpression _:{s1}v .
_:{s1}v a math:NumberLiteral ; math:literalValue "2"^^xsd:integer .
"#
        )
        .into_bytes()
    }

    /// The wrapper interns through the shared arena: the same expression authored twice —
    /// different blank-node labels, different serialization order — collapses to ONE node
    /// and ONE content key, and the second lift mints nothing.
    #[test]
    fn intern_math_root_is_content_addressed_and_hash_consed() {
        let mut arena = TermArena::new();

        let first_mark = arena.snapshot();
        let (first, first_key) = intern_turtle(
            &mut arena,
            &application_turtle(("e", "a", "b"), false),
            "_:e",
        )
        .expect("well-formed math application");
        let first_delta = first_mark.delta_to(&arena);
        assert!(
            first_delta.distinct_nodes > 0,
            "the first lift must mint nodes"
        );

        let second_mark = arena.snapshot();
        let (second, second_key) = intern_turtle(
            &mut arena,
            &application_turtle(("z", "y", "x"), true),
            "_:z",
        )
        .expect("well-formed math application");
        let second_delta = second_mark.delta_to(&arena);

        assert_eq!(first, second, "the same expression is ONE node");
        assert_eq!(first_key, second_key, "…with ONE content key");
        assert_eq!(
            second_delta.distinct_nodes, 0,
            "re-lifting an already-interned expression mints NOTHING"
        );
        assert_eq!(
            second_delta.intern_calls, first_delta.intern_calls,
            "…while doing the same interning work"
        );
    }

    /// An unliftable expression graph is a typed hard failure, never a partial lift.
    #[test]
    fn intern_math_root_hard_fails_on_a_malformed_slot_sequence() {
        // slotIndex 0 and 2 — non-contiguous, so the argument order is undecidable.
        let turtle = format!(
            r#"@prefix math: <{MATH}> .
@prefix xsd: <http://www.w3.org/2001/XMLSchema#> .

_:e a math:ApplicationExpression ;
    math:operator <https://example.org/plus> ;
    math:argumentSlot _:a, _:b .
_:a a math:ArgumentSlot ; math:slotIndex 0 ; math:slotExpression _:av .
_:av a math:NumberLiteral ; math:literalValue "1"^^xsd:integer .
_:b a math:ArgumentSlot ; math:slotIndex 2 ; math:slotExpression _:bv .
_:bv a math:NumberLiteral ; math:literalValue "2"^^xsd:integer .
"#
        )
        .into_bytes();

        let mut arena = TermArena::new();
        intern_turtle(&mut arena, &turtle, "_:e")
            .expect_err("a non-contiguous slot sequence must hard-fail");
    }

    /// Unparsable bytes are a typed hard failure too — no empty-graph fallback.
    #[test]
    fn intern_math_root_hard_fails_on_unparsable_turtle() {
        let mut arena = TermArena::new();
        intern_turtle(&mut arena, b"this is not turtle {{{", "_:e")
            .expect_err("unparsable input must hard-fail");
    }
}
