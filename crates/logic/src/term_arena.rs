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
//! [`MathGraphInterning::intern_math_graph`](crate::term_arena::MathGraphInterning::intern_math_graph)
//! is the one addition. It is a thin wrapper —
//! it writes NO interning logic — over the existing
//! [`MathGraph::from_turtle`](crate::physical::lower) parse, the existing
//! [`lower_math_expression`](crate::physical::lower) lowering, and the arena's own content
//! key.
//!
//! ## Why the wrapper is HERE and not in `gmeow-term-arena`
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
//! So the wrapper lives in the crate that ALREADY has the `purrdf` + `MathGraph` edge, and
//! `gmeow-term-arena` stays minimal. It is spelled as an extension trait so the call site
//! still reads `arena.intern_math_graph(turtle, root)` — a method on the arena, exactly as
//! it would if it could live there.

use gmeow_errors::Result;
use gmeow_term_arena::engine::ArenaAccess;

pub use gmeow_term_arena::{
    Arena, ArenaSnapshot, ContentKey, ForeignNode, InterningStats, StructNode, TermArena,
};

use crate::physical::lower::{MathGraph, lower_math_expression};

/// Intern a `math:` expression graph into the shared arena.
///
/// Sealed by [`gmeow_term_arena::Arena`]: [`TermArena`] is the only implementer, so this
/// extension cannot be attached to a second, invented arena.
pub trait MathGraphInterning: Arena {
    /// Parse `turtle` as a `math:` application/binding expression graph, lower the
    /// expression rooted at `root` into this arena, and return its opaque handle together
    /// with its content key.
    ///
    /// Because the arena is content-addressed and locally-nameless, a `math:` expression
    /// and an alpha-equivalent `logic:` formula intern to the SAME node and the SAME
    /// [`ContentKey`] — that cross-surface collapse is the point.
    ///
    /// # Errors
    ///
    /// Returns a typed diagnostic if the Turtle does not parse, if the expression graph is
    /// malformed (a non-contiguous `math:slotIndex` sequence, an occurrence bound to
    /// nothing, a `math:NumberLiteral` with no `math:literalValue`, an unrecognized node
    /// type), or if a de-Bruijn distance/slot would overflow the physical node's field
    /// width. There is no degraded path: an unliftable expression is a hard failure, never
    /// a silently-dropped subterm.
    fn intern_math_graph(&mut self, turtle: &[u8], root: &str) -> Result<(StructNode, ContentKey)>;
}

impl MathGraphInterning for TermArena {
    fn intern_math_graph(&mut self, turtle: &[u8], root: &str) -> Result<(StructNode, ContentKey)> {
        let graph = MathGraph::from_turtle(turtle)?;
        let node = lower_math_expression(self.dag_mut(), &graph, root)?;
        let handle = self.brand_node(node).map_err(|err| {
            gmeow_errors::Diag::of_kind(gmeow_logic_compile::error::Ir {
                detail: format!(
                    "math expression lowering produced a node this arena rejects: {err}"
                ),
            })
        })?;
        let key = self.key(handle).map_err(|err| {
            gmeow_errors::Diag::of_kind(gmeow_logic_compile::error::Ir {
                detail: format!(
                    "math expression lowering produced a node this arena rejects: {err}"
                ),
            })
        })?;
        Ok((handle, key))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MATH: &str = "https://blackcatinformatics.ca/math/";

    /// `plus(1, 2)` as a `math:ApplicationExpression`, and the same expression authored a
    /// second time with different blank-node labels and reversed slot order.
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
    fn intern_math_graph_is_content_addressed_and_hash_consed() {
        let mut arena = TermArena::new();

        let first_mark = arena.snapshot();
        let (first, first_key) = arena
            .intern_math_graph(&application_turtle(("e", "a", "b"), false), "_:e")
            .expect("well-formed math application");
        let first_delta = first_mark.delta_to(&arena);
        assert!(
            first_delta.distinct_nodes > 0,
            "the first lift must mint nodes"
        );

        let second_mark = arena.snapshot();
        let (second, second_key) = arena
            .intern_math_graph(&application_turtle(("z", "y", "x"), true), "_:z")
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
    fn intern_math_graph_hard_fails_on_a_malformed_slot_sequence() {
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
        arena
            .intern_math_graph(&turtle, "_:e")
            .expect_err("a non-contiguous slot sequence must hard-fail");
    }

    /// Unparsable bytes are a typed hard failure too — no empty-graph fallback.
    #[test]
    fn intern_math_graph_hard_fails_on_unparsable_turtle() {
        let mut arena = TermArena::new();
        arena
            .intern_math_graph(b"this is not turtle {{{", "_:e")
            .expect_err("unparsable input must hard-fail");
    }
}
