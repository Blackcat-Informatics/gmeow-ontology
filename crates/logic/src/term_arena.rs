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

#[path = "term_arena.tests.rs"]
#[cfg(test)]
mod tests;
