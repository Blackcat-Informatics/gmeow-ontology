// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! `gmeow-term-arena` — **the one term representation**, relocated out of the reasoning
//! runtime.
//!
//! There is exactly one structured-term store in this workspace: a persistent,
//! hash-consed, binder-aware, content-addressed DAG whose identity is a
//! [`ContentKey`].  A lifted script, a `logic:` formula, a `math:` expression graph, and a
//! `lang:` form all intern INTO it, so alpha-equivalent inputs authored in any surface
//! collapse to one node and one key.  Slice-local key predicates are staging surfaces for
//! that unification, never permanent forks — which is why this crate exists rather than a
//! per-consumer AST crate.
//!
//! # Why it is its own crate
//!
//! The arena used to live inside `gmeow-logic`, the native DL reasoning runtime.  Any
//! consumer that merely wanted to *intern a term* therefore had to link the whole
//! reasoner — including its PyO3-tainted diagnostics path — and `gmeow-logic` does not
//! compile to wasm.  This crate links **no reasoning runtime**: its dependencies are the
//! wasm-clean `purrdf` RDF value space plus `hashbrown` / `foldhash` / `smallvec`.  A
//! parser front-end can intern terms without pulling the engine, and the shared store's
//! façade is reachable from all three grounding slices.
//!
//! # Two tiers, deliberately
//!
//! **The façade (this module)** is the curated surface every non-runtime consumer uses:
//!
//! | Item | Role |
//! |---|---|
//! | [`ContentKey`] | the persistent, serializable identity of an interned term |
//! | [`TermArena`] | the store: six interning constructors plus [`Arena::key`] |
//! | [`StructNode`] | an OPAQUE, arena-branded handle — no dense integer escapes |
//! | [`InterningStats`] | a **scoped snapshot delta**, never a global counter |
//!
//! Handles stay opaque on purpose: `NodeId`/`ArenaId` are dense per-arena runtime
//! integers, meaningless outside the arena that minted them and never serialized. The
//! façade hands out [`StructNode`], which carries its arena's brand so a foreign handle is
//! REJECTED rather than silently resolved against the wrong store.
//!
//! **The engine tier ([`engine`])** exposes the representation itself — [`engine::TermDag`],
//! [`engine::NodeData`], the branded [`engine::Id`] family, the atom dictionary.  It exists
//! for the ONE consumer that *operates* the arena (the reasoning runtime's unifier, proof
//! checker, and structured backward resolver) rather than *uses* it.  Rust has no
//! friend-crate visibility, so the boundary is drawn by module and stated here rather than
//! pretended away: reaching for [`engine`] from a front-end is using the wrong tier.
//!
//! # The seal
//!
//! [`Arena`] is a **sealed** trait ([`sealed::Sealed`] is private to this crate), so no
//! downstream crate can introduce a second arena implementation — "exactly one term
//! representation" is enforced by the type system, not by convention.  The same seal
//! covers [`ArenaAccess`] and [`StructNodeParts`], the two engine-tier access traits.
//!
//! # Worked example
//!
//! ```rust
//! use gmeow_term_arena::{Arena, TermArena};
//! use purrdf::TermValue;
//!
//! // `∀_:Sort. p(#0.0)` built twice from independently-constructed children.
//! let mut arena = TermArena::new();
//! let build = |arena: &mut TermArena| {
//!     let p = arena.intern_leaf(TermValue::iri("https://example.org/p"));
//!     let bound = arena.intern_bound(0, 0);
//!     let body = arena.intern_app(p, &[bound]).expect("own nodes");
//!     let sort = arena.intern_leaf(TermValue::iri("https://example.org/Sort"));
//!     let forall = arena.intern_leaf(TermValue::iri("https://example.org/forall"));
//!     arena.intern_binder(forall, &[sort], body).expect("own nodes")
//! };
//!
//! let before = arena.snapshot();
//! let first = build(&mut arena);
//! let second = build(&mut arena);
//! let delta = before.delta_to(&arena);
//!
//! // Hash-consing: the second build mints NOTHING new, yet the interning work is counted.
//! assert_eq!(first, second);
//! assert_eq!(arena.key(first).unwrap(), arena.key(second).unwrap());
//! assert_eq!(delta.distinct_nodes, 6, "six distinct nodes, built twice");
//! assert_eq!(delta.intern_calls, 12, "twelve interning calls");
//! ```

mod display;
mod id;
mod interner;
mod term_dag;
mod term_key;

use std::fmt;

use purrdf::TermValue;

use crate::id::NodeId;
use crate::term_dag::{ArenaId, TermDag};

/// The engine tier: the term representation itself.
///
/// This module is for the ONE consumer that *operates* the arena — the reasoning
/// runtime's unifier, proof checker, and structured backward resolver, which match on
/// [`NodeData`](engine::NodeData) shapes and rebuild nodes bottom-up.  Everything else
/// uses the crate-root façade, where dense integers never appear.
///
/// Reaching for this module from a parser front-end is using the wrong tier: the façade
/// already exposes every interning constructor and the content key.
pub mod engine {
    pub use crate::display::{RDF_LANG_STRING, XSD_STRING, term_display, term_n3_unchecked};
    pub use crate::id::{Id, Meta, MetaId, Node, NodeId, Term, TermId};
    pub use crate::interner::{TermInterner, surface_hash};
    pub use crate::term_dag::{ArenaId, MetaSet, NodeData, TermDag};

    pub use super::{ArenaAccess, StructNodeParts};
}

/// The private seal.
///
/// Implemented only for the types in this crate, so [`Arena`], [`ArenaAccess`], and
/// [`StructNodeParts`] are un-implementable downstream.  A second arena implementation is
/// therefore not merely discouraged — it does not compile.
mod sealed {
    pub trait Sealed {}
    impl Sealed for super::TermArena {}
    impl Sealed for super::StructNode {}
}

// ── ContentKey ────────────────────────────────────────────────────────────────

/// The persistent, content-addressed identity of an interned term.
///
/// A content key is a pure structural fold: two terms share a key exactly when they are
/// structurally identical up to bound-variable renaming (bound occurrences are
/// locally-nameless de-Bruijn refs, so alpha-equivalence is already quotiented away).
/// Unlike a [`StructNode`], a key is arena-independent and safe to serialize — it is the
/// identity that crosses process, file, and graph boundaries.
///
/// This is the ONE content-key type in the workspace: it lands on both sides of the
/// congruence seam (the DAG's fold and `gmeow_logic_compile::ir::Formula::content_key`),
/// so the guarantee that the two agree is stated in the types rather than by convention.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ContentKey(String);

impl ContentKey {
    /// Wrap the output of a content-key fold.
    ///
    /// The constructor is public because this workspace has exactly two folds that produce
    /// one — the DAG's netstring fold ([`crate::term_key`]) and the `logic:` IR's
    /// `Formula::content_key` — and they live in different crates.  It is NOT an invitation
    /// to mint keys from arbitrary text: a key that did not come from a fold addresses no
    /// term.
    pub fn new(key: String) -> Self {
        Self(key)
    }

    /// The key's bytes.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Consume the key, yielding its bytes.
    pub fn into_string(self) -> String {
        self.0
    }
}

impl fmt::Display for ContentKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl AsRef<str> for ContentKey {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

// ── StructNode ────────────────────────────────────────────────────────────────

/// An opaque handle to a compound-term node in a [`TermArena`].
///
/// It wraps a dense per-arena node handle behind PRIVATE fields, so the integer stays out
/// of the façade even though `StructNode` is `pub`.  The arena's brand travels WITH the
/// node, so a later membership test ([`Arena::contains`]) is an arena-IDENTITY check, not
/// a numeric index-range coincidence — a node from a foreign arena is rejected even when
/// its index happens to fall in the target arena's range.
///
/// Reading or minting the wrapped handles requires the engine-tier [`StructNodeParts`]
/// trait, which is sealed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StructNode(NodeId, ArenaId);

/// Engine-tier access to a [`StructNode`]'s branded parts.
///
/// Sealed: only this crate's [`StructNode`] implements it.  Importing this trait is the
/// explicit act by which a consumer leaves the façade and takes on the per-arena handle
/// contract.
pub trait StructNodeParts: sealed::Sealed + Sized {
    /// Wrap a node handle minted by the arena carrying `arena`'s brand.
    fn wrap(node: NodeId, arena: ArenaId) -> Self;
    /// The wrapped node handle.
    fn node(self) -> NodeId;
    /// The brand of the arena that minted [`Self::node`].
    fn arena(self) -> ArenaId;
}

impl StructNodeParts for StructNode {
    #[inline]
    fn wrap(node: NodeId, arena: ArenaId) -> Self {
        Self(node, arena)
    }

    #[inline]
    fn node(self) -> NodeId {
        self.0
    }

    #[inline]
    fn arena(self) -> ArenaId {
        self.1
    }
}

// ── ForeignNode ───────────────────────────────────────────────────────────────

/// A [`StructNode`] minted by a DIFFERENT arena was handed to this one.
///
/// A hard failure, never a silent resolution: the handle's numeric index may well be in
/// range here, and resolving it would silently address an unrelated term.  This crate
/// raises no diagnostics-substrate error type on purpose — it depends on no error crate,
/// so consumers map this into their own typed diagnostic.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ForeignNode;

impl fmt::Display for ForeignNode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(
            "the StructNode was minted by a different TermArena; a per-arena handle must \
             never cross arena boundaries",
        )
    }
}

impl std::error::Error for ForeignNode {}

// ── InterningStats / ArenaSnapshot ────────────────────────────────────────────

/// The interning work done between a [`ArenaSnapshot`] and a later arena state.
///
/// The discharge this measures is: **fact count grows with distinct structure, not with
/// textual repetition**.  Interning one normalized subexpression `N` times leaves
/// [`distinct_nodes`](Self::distinct_nodes) invariant in `N` while
/// [`intern_calls`](Self::intern_calls) grows with it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct InterningStats {
    /// How many times interning was ASKED for (hits included).
    pub intern_calls: u64,
    /// How many DISTINCT nodes were minted.
    pub distinct_nodes: u64,
}

/// A point-in-time mark on a [`TermArena`], for a **scoped** interning-stats delta.
///
/// This is deliberately not a global counter.  Two lifts in one process would pollute each
/// other's numbers, making any interning measurement order-dependent; the whole point of
/// the `distinct_nodes`-invariant-across-repetition-counts discharge is that it is a
/// property of ONE scoped lift.  Take a snapshot, do the work, call [`Self::delta_to`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ArenaSnapshot {
    intern_calls: u64,
    distinct_nodes: u64,
}

impl ArenaSnapshot {
    /// The interning work done on `arena` since this snapshot was taken.
    ///
    /// # Panics
    ///
    /// Panics if `arena` is behind the snapshot — a snapshot taken from a DIFFERENT arena
    /// measures nothing about this one, and silently returning a zero (or wrapped) delta
    /// would report a false invariance.  The arena is persistent and never resets, so a
    /// decrease can only mean the snapshot is foreign.
    pub fn delta_to<A: Arena>(&self, arena: &A) -> InterningStats {
        let intern_calls = arena.intern_calls().checked_sub(self.intern_calls);
        let distinct_nodes = arena.distinct_nodes().checked_sub(self.distinct_nodes);
        match (intern_calls, distinct_nodes) {
            (Some(intern_calls), Some(distinct_nodes)) => InterningStats {
                intern_calls,
                distinct_nodes,
            },
            _ => panic!(
                "ArenaSnapshot {{ intern_calls: {}, distinct_nodes: {} }} is ahead of the arena \
                 (intern_calls {}, distinct_nodes {}): the snapshot was taken from a DIFFERENT \
                 arena — a term arena is persistent and never rewinds",
                self.intern_calls,
                self.distinct_nodes,
                arena.intern_calls(),
                arena.distinct_nodes()
            ),
        }
    }
}

// ── TermArena ─────────────────────────────────────────────────────────────────

/// The one structured-term store.
///
/// A persistent hash-consed DAG: interning a structurally-identical term a second time
/// returns the SAME [`StructNode`] and the same [`ContentKey`], and bound variables are
/// locally-nameless, so alpha-equivalent terms are literally the same node.
///
/// The six interning constructors and [`Arena::key`] are the whole façade.  The two
/// constructors that take a [`StructNode`] — [`TermArena::intern_app`] and
/// [`TermArena::intern_binder`] — validate its brand and return [`ForeignNode`] rather
/// than resolve a foreign handle.
#[derive(Debug, Default)]
pub struct TermArena {
    dag: TermDag,
}

impl TermArena {
    /// A fresh, empty arena with a new process-unique brand.
    pub fn new() -> Self {
        Self::default()
    }

    /// Intern an atomic leaf — an IRI or a literal, with its datatype/language intact.
    pub fn intern_leaf(&mut self, tv: TermValue) -> StructNode {
        let node = self.dag.intern_leaf(tv);
        self.brand(node)
    }

    /// Intern a named free-variable occurrence (rigid: not unifiable).
    pub fn intern_free(&mut self, tv: TermValue) -> StructNode {
        let node = self.dag.intern_free(tv);
        self.brand(node)
    }

    /// Intern a bound-variable occurrence at de-Bruijn `debruijn` distance and `slot`
    /// ordinal (0 = the innermost enclosing binder).
    pub fn intern_bound(&mut self, debruijn: u32, slot: u16) -> StructNode {
        let node = self.dag.intern_bound(debruijn, slot);
        self.brand(node)
    }

    /// Mint a FRESH, identity-bearing unification metavariable and intern its node.
    ///
    /// Two calls yield two distinct nodes; the ordinal enters the content key, so
    /// metavariable identity is content-addressed like everything else. The dense
    /// `MetaId` stays in the engine tier — the façade hands back only the node.
    pub fn fresh_meta(&mut self) -> StructNode {
        let (_, node) = self.dag.fresh_meta();
        self.brand(node)
    }

    /// Intern a function application `op(args…)`.
    ///
    /// # Errors
    ///
    /// Returns [`ForeignNode`] if `op` or any argument was minted by a different arena.
    pub fn intern_app(
        &mut self,
        op: StructNode,
        args: &[StructNode],
    ) -> Result<StructNode, ForeignNode> {
        let op = self.resolve(op)?;
        let args = args
            .iter()
            .map(|&a| self.resolve(a))
            .collect::<Result<Vec<_>, _>>()?;
        let node = self.dag.intern_app(op, args);
        Ok(self.brand(node))
    }

    /// Intern a binder block `op[sorts…]. body`, where `body`'s de-Bruijn refs see this
    /// binder at distance 0.
    ///
    /// # Errors
    ///
    /// Returns [`ForeignNode`] if `op`, any sort, or `body` was minted by a different
    /// arena.
    pub fn intern_binder(
        &mut self,
        op: StructNode,
        sorts: &[StructNode],
        body: StructNode,
    ) -> Result<StructNode, ForeignNode> {
        let op = self.resolve(op)?;
        let sorts = sorts
            .iter()
            .map(|&s| self.resolve(s))
            .collect::<Result<Vec<_>, _>>()?;
        let body = self.resolve(body)?;
        let node = self.dag.intern_binder(op, sorts, body);
        Ok(self.brand(node))
    }

    /// Brand a freshly-minted node handle with THIS arena's identity.
    #[inline]
    fn brand(&self, node: NodeId) -> StructNode {
        StructNode::wrap(node, self.dag.arena())
    }

    /// The dense handle behind `node`, or [`ForeignNode`] if it belongs to another arena.
    #[inline]
    fn resolve(&self, node: StructNode) -> Result<NodeId, ForeignNode> {
        if self.dag.contains_node(node.node(), node.arena()) {
            Ok(node.node())
        } else {
            Err(ForeignNode)
        }
    }
}

/// The sealed arena capability.
///
/// Sealed so that "there is exactly one term representation" is a compile-time fact: no
/// downstream crate can supply a second [`Arena`].  It is a real bound, not a marker —
/// [`ArenaSnapshot::delta_to`] is generic over it.
pub trait Arena: sealed::Sealed {
    /// The [`ContentKey`] of `node` — its persistent, arena-independent identity.
    ///
    /// # Errors
    ///
    /// Returns [`ForeignNode`] if `node` was minted by a different arena.
    fn key(&self, node: StructNode) -> Result<ContentKey, ForeignNode>;

    /// Whether `node` was minted by THIS arena (a brand-identity test, not a bounds test).
    fn contains(&self, node: StructNode) -> bool;

    /// Mark the current state for a later [`ArenaSnapshot::delta_to`].
    fn snapshot(&self) -> ArenaSnapshot;

    /// How many times interning has been asked of this arena (hits included).
    fn intern_calls(&self) -> u64;

    /// How many DISTINCT nodes this arena holds.
    fn distinct_nodes(&self) -> u64;
}

impl Arena for TermArena {
    fn key(&self, node: StructNode) -> Result<ContentKey, ForeignNode> {
        let id = self.resolve(node)?;
        Ok(ContentKey::new(self.dag.key(id).to_owned()))
    }

    fn contains(&self, node: StructNode) -> bool {
        self.dag.contains_node(node.node(), node.arena())
    }

    fn snapshot(&self) -> ArenaSnapshot {
        ArenaSnapshot {
            intern_calls: self.intern_calls(),
            distinct_nodes: self.distinct_nodes(),
        }
    }

    fn intern_calls(&self) -> u64 {
        self.dag.intern_calls()
    }

    fn distinct_nodes(&self) -> u64 {
        self.dag.len() as u64
    }
}

/// Engine-tier access to a [`TermArena`]'s backing DAG.
///
/// Sealed.  This is the seam a consumer crossing into the engine tier uses — notably the
/// reasoning runtime's `math:`-graph lowering, which walks an RDF expression tree and
/// interns it node-by-node through [`engine::TermDag`].
pub trait ArenaAccess: sealed::Sealed {
    /// The backing DAG.
    fn dag(&self) -> &TermDag;
    /// The backing DAG, mutably.
    fn dag_mut(&mut self) -> &mut TermDag;
    /// Brand a node handle minted by this arena's DAG as an opaque [`StructNode`].
    ///
    /// # Errors
    ///
    /// Returns [`ForeignNode`] if `node` is not a live slot of this arena's DAG.
    fn brand_node(&self, node: NodeId) -> Result<StructNode, ForeignNode>;
}

impl ArenaAccess for TermArena {
    fn dag(&self) -> &TermDag {
        &self.dag
    }

    fn dag_mut(&mut self) -> &mut TermDag {
        &mut self.dag
    }

    fn brand_node(&self, node: NodeId) -> Result<StructNode, ForeignNode> {
        let branded = StructNode::wrap(node, self.dag.arena());
        if self.contains(branded) {
            Ok(branded)
        } else {
            Err(ForeignNode)
        }
    }
}
