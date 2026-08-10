// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The persistent hash-consed, binder-aware, content-addressed structured-term DAG.
//!
//! # What this is
//!
//! [`TermDag`] is the persistent term arena for *structured* terms — function-symbol
//! applications, binders, and proof-object trees.  It mirrors the atom dictionary's
//! borrowed-key discipline ([`crate::interner::TermInterner`]): a node is interned once
//! and addressed by a dense insertion-ordered [`NodeId`], its content key lives once in
//! a side arena, and a borrowed-`&str` probe resolves a candidate id against that arena
//! with no owned-key clone and no re-render.
//!
//! It is **persistent** — never reset within its lifetime — and is therefore distinct
//! from any per-round row/tuple bump arena a consumer may run alongside it.
//!
//! # Hash-consing ⇒ structural sharing ⇒ alpha-equivalence is `NodeId` equality
//!
//! Every node is content-keyed bottom-up ([`crate::term_key`]): a node's key is
//! folded from its children's already-cached keys, so interning a structurally-identical
//! term a second time returns the SAME `NodeId` (maximal sharing).  Bound variables are
//! **locally-nameless** — a [`NodeData::Bound`] carries a de-Bruijn distance to its binder
//! plus a slot ordinal, never a name — so two alpha-equivalent terms are *the same tree*
//! and hence the same `NodeId`.  Alpha-equivalence is thus decided by `Copy` integer
//! equality, and the free-metavariable set of each node is cached for an `O(1)`
//! occurs-check on the unification rungs.
//!
//! # Determinism doctrine (inherited)
//!
//! `NodeId`/`MetaId` are runtime handles: assigned in insertion / mint order, meaningless
//! outside the DAG that minted them, NEVER serialized and NEVER hashed for provenance.
//! The content key is the persistent identity; the dense integers never escape the
//! runtime (the [`crate::id`] doctrine) — the crate-root façade hands out an opaque,
//! arena-branded [`StructNode`](crate::StructNode) instead.

use std::sync::atomic::{AtomicU64, Ordering};

use hashbrown::HashTable;
use purrdf::TermValue;
use smallvec::SmallVec;

use crate::id::{MetaId, NodeId, TermId};
use crate::interner::{TermInterner, surface_hash};
use crate::term_key;

/// The process-global source of the next [`ArenaId`] brand. Starts at 1 so a defaulted
/// `ArenaId(0)` (were one ever constructed by mistake) can never alias a live arena.
static NEXT_ARENA_BRAND: AtomicU64 = AtomicU64::new(1);

/// A process-unique brand identifying the [`TermDag`] arena that minted a [`NodeId`].
///
/// A [`NodeId`] is a dense per-DAG slot handle whose numeric index is only meaningful within
/// the arena that minted it; two independent DAGs mint overlapping index ranges. The brand
/// closes that gap: a caller holding a [`NodeId`] of unknown provenance carries the brand of
/// the arena it came from (via [`crate::StructNode`]), and [`TermDag::contains_node`]
/// rejects any handle whose brand is not this arena's — so a foreign node can never be silently
/// resolved against the wrong arena. Like every runtime handle it is NEVER serialized and NEVER
/// hashed for provenance (the content key remains the persistent identity).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ArenaId(u64);

/// A structured-term DAG node.
///
/// **Locally-nameless**: a bound occurrence is a de-Bruijn [`NodeData::Bound`] ref, so
/// alpha-equivalence is structural `NodeData` equality (and, after interning, `NodeId`
/// equality).  Derives structural `Eq`/`Hash` so the DAG can key on the node itself where
/// convenient; canonical identity nonetheless always flows through the content key.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub enum NodeData {
    /// An interned atomic IRI/literal leaf, delegated to the
    /// [`TermInterner`](crate::interner::TermInterner).
    Leaf(TermId),
    /// A named free-variable occurrence (not unifiable), interned by name.
    Free(TermId),
    /// A unification metavariable (identity-bearing).
    Meta(MetaId),
    /// A bound-variable OCCURRENCE: a de-Bruijn `debruijn` distance out to its binder
    /// (0 = the innermost enclosing binder) plus the zero-based `slot` in that binder's
    /// declaration block.
    Bound {
        /// De-Bruijn distance to the binder (0 = innermost enclosing binder).
        debruijn: u32,
        /// Zero-based slot within that binder's declaration block.
        slot: u16,
    },
    /// A function application: `op` applied to positional, zero-based, contiguous `args`.
    App {
        /// The applied operator node.
        op: NodeId,
        /// The positional argument nodes.
        args: Box<[NodeId]>,
    },
    /// A binder DECLARATION block: `op` (the binder symbol), one `sorts` child per bound
    /// slot (its declared type/sort), and a `body` keyed one binder-depth deeper.
    Binder {
        /// The binder-symbol node.
        op: NodeId,
        /// One per-slot sort/type child (its length is the binder's arity).
        sorts: Box<[NodeId]>,
        /// The body, whose de-Bruijn refs see this binder at distance 0.
        body: NodeId,
    },
}

/// A small, deduplicated, **sorted** set of [`MetaId`] — the cached free-metavariable set
/// of a node, for an `O(1)` occurs-check on the unification rungs.
///
/// Sorted so membership is a binary search and union is a linear merge; inline for the
/// common small-support case, spilling to the heap only for metavariable-rich terms.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MetaSet(SmallVec<[MetaId; 4]>);

impl MetaSet {
    /// The empty set.
    #[inline]
    fn empty() -> Self {
        Self(SmallVec::new())
    }

    /// The singleton `{m}`.
    #[inline]
    fn singleton(m: MetaId) -> Self {
        let mut v = SmallVec::new();
        v.push(m);
        Self(v)
    }

    /// Merge `other` into `self`, preserving the sorted-deduplicated invariant.
    #[inline]
    fn union_with(&mut self, other: &MetaSet) {
        for &m in &other.0 {
            if let Err(pos) = self.0.binary_search(&m) {
                self.0.insert(pos, m);
            }
        }
    }

    /// Whether `m` is a free metavariable of the owning node — the occurs-check primitive.
    #[inline]
    pub fn contains(&self, m: MetaId) -> bool {
        self.0.binary_search(&m).is_ok()
    }

    /// The free metavariables, in ascending [`MetaId`] order.
    #[inline]
    pub fn iter(&self) -> impl Iterator<Item = MetaId> + '_ {
        self.0.iter().copied()
    }

    /// The number of distinct free metavariables.
    #[inline]
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Whether the node has no free metavariables (a fully-ground / metavariable-free term).
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

/// A persistent hash-consed structured-term DAG.
///
/// Mirrors [`crate::interner::TermInterner`]: dense insertion-ordered handles, the content
/// key stored once in a side arena, and a borrowed-key [`HashTable`] probe.
#[derive(Debug)]
pub struct TermDag {
    /// This arena's process-unique brand, minted at construction. Carried by no node — it is
    /// the identity a [`NodeId`] of unknown provenance is validated AGAINST in
    /// [`Self::contains_node`].
    arena: ArenaId,
    /// Atomic leaves reuse the atom dictionary verbatim: intern a [`TermValue`] → [`TermId`].
    atoms: TermInterner,
    /// Nodes in insertion order (slot = [`NodeId`] index).
    nodes: Vec<NodeData>,
    /// Cached content key per node, in lockstep with `nodes` — the side arena the
    /// `by_key` probe resolves against.
    keys: Vec<Box<str>>,
    /// Cached free-metavariable set per node, for an `O(1)` occurs-check.
    free_meta: Vec<MetaSet>,
    /// Content key → id, for `O(1)` intern/lookup.  Holds the [`NodeId`] ONLY: the key
    /// bytes live once in `keys`, so a borrowed-`&str` probe resolves a candidate id with
    /// a `keys[id.index()]` slice compare — the eq/hash closure never re-folds a key.
    by_key: HashTable<NodeId>,
    /// The number of metavariables minted so far — the next [`MetaId`] ordinal.
    meta_count: usize,
    /// The number of [`Self::intern`] CALLS this arena has served — hits included.
    ///
    /// This is the numerator of the interning-demonstrability measurement: re-interning an
    /// already-normalized subexpression grows `intern_calls` while `nodes.len()` stays put,
    /// which is exactly "fact count grows with distinct structure, not textual repetition".
    /// It lives on the arena (never a process-global counter) so two independent arenas —
    /// or two scoped snapshots of one arena — report INDEPENDENT deltas.
    intern_calls: u64,
}

impl Default for TermDag {
    /// A fresh, empty DAG minting a new process-unique [`ArenaId`] brand.
    fn default() -> Self {
        Self {
            arena: ArenaId(NEXT_ARENA_BRAND.fetch_add(1, Ordering::Relaxed)),
            atoms: TermInterner::default(),
            nodes: Vec::new(),
            keys: Vec::new(),
            free_meta: Vec::new(),
            by_key: HashTable::new(),
            meta_count: 0,
            intern_calls: 0,
        }
    }
}

impl TermDag {
    /// A fresh, empty DAG.
    pub fn new() -> Self {
        Self::default()
    }

    /// This arena's process-unique brand — the identity a caller records (via
    /// [`crate::StructNode`]) so a held [`NodeId`] can later be validated against the
    /// arena that minted it in [`Self::contains_node`].
    pub fn arena(&self) -> ArenaId {
        self.arena
    }

    // ── Accessors ───────────────────────────────────────────────────────────────

    /// The node data for `id`.
    ///
    /// # Panics
    ///
    /// Panics if `id` was not minted by this DAG — `NodeId`s are per-DAG handles.
    pub fn data(&self, id: NodeId) -> &NodeData {
        self.nodes.get(id.index()).unwrap_or_else(|| {
            panic!(
                "NodeId {id:?} was not minted by this DAG (len {}): NodeIds are per-DAG handles",
                self.nodes.len()
            )
        })
    }

    /// The cached content key for `id` (same per-DAG panic contract as [`Self::data`]).
    pub fn key(&self, id: NodeId) -> &str {
        self.keys.get(id.index()).map(|k| &**k).unwrap_or_else(|| {
            panic!(
                "NodeId {id:?} was not minted by this DAG (len {}): NodeIds are per-DAG handles",
                self.keys.len()
            )
        })
    }

    /// The cached free-metavariable set for `id` (same per-DAG panic contract).
    pub fn free_meta(&self, id: NodeId) -> &MetaSet {
        self.free_meta.get(id.index()).unwrap_or_else(|| {
            panic!(
                "NodeId {id:?} was not minted by this DAG (len {}): NodeIds are per-DAG handles",
                self.free_meta.len()
            )
        })
    }

    /// The cached display surface of an atomic leaf, for the content-key fold.
    pub fn atom_display(&self, id: TermId) -> &str {
        self.atoms.display_of(id)
    }

    /// The first-seen [`TermValue`] backing an atomic leaf handle.
    ///
    /// The inverse of [`Self::intern_atom`]: it recovers the resolved N3-serializable term
    /// surface a leaf stands for, so a ground structured term can be projected back to its
    /// content-addressed reifier IRI. Same per-DAG panic contract as [`Self::atom_display`].
    pub fn atom_value(&self, id: TermId) -> &TermValue {
        self.atoms.resolve(id)
    }

    /// The number of DISTINCT nodes interned — the denominator of the
    /// interning-demonstrability measurement.
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    /// Whether the DAG holds no nodes.
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// The number of [`Self::intern`] calls this arena has served (hits included).
    ///
    /// See the field doctrine on [`intern_calls`](Self::intern_calls): it is per-arena, so
    /// deltas taken over it are scoped, never process-global.
    pub fn intern_calls(&self) -> u64 {
        self.intern_calls
    }

    /// Whether the branded handle `(id, arena)` was minted by THIS DAG.
    ///
    /// A [`NodeId`] is meaningful only within the DAG that minted it, so a caller holding a
    /// [`NodeId`] of unknown provenance tests membership here rather than risking the
    /// per-DAG [`Self::data`] panic — OR, worse, silently resolving a FOREIGN node whose
    /// numeric index merely happens to fall in this arena's range.
    ///
    /// Two SEPARATE conditions, both mandatory: the `arena` brand must be THIS arena's (so a
    /// node minted by any other DAG is rejected outright), AND the slot index must be in range.
    /// The bounds check alone is unsound across arenas; the brand check makes membership an
    /// identity test, not an index-range coincidence.
    pub fn contains_node(&self, id: NodeId, arena: ArenaId) -> bool {
        let same_arena = arena == self.arena;
        let in_bounds = id.index() < self.nodes.len();
        same_arena && in_bounds
    }

    // ── Constructors ──────────────────────────────────────────────────────────────

    /// Intern an atomic term surface into the leaf dictionary, returning its [`TermId`]
    /// WITHOUT minting a leaf node.
    ///
    /// This exposes the atom handle a caller needs to key content on the term itself — e.g.
    /// a proof-object rule context keying a rule clause by its rule-IRI [`TermId`] and
    /// carrying that handle as a `Leaf` proof argument via [`Self::intern_leaf_atom`].
    pub fn intern_atom(&mut self, tv: &TermValue) -> TermId {
        self.atoms.intern(tv)
    }

    /// Intern a leaf node for an already-interned atom handle (the node-level counterpart of
    /// [`Self::intern_atom`]).
    pub fn intern_leaf_atom(&mut self, atom: TermId) -> NodeId {
        self.intern(NodeData::Leaf(atom))
    }

    /// Intern an atomic leaf (an IRI/literal), returning its node id.
    pub fn intern_leaf(&mut self, tv: TermValue) -> NodeId {
        let atom = self.intern_atom(&tv);
        self.intern_leaf_atom(atom)
    }

    /// Intern a named free-variable occurrence, returning its node id.
    pub fn intern_free(&mut self, tv: TermValue) -> NodeId {
        let atom = self.atoms.intern(&tv);
        self.intern(NodeData::Free(atom))
    }

    /// Mint a FRESH unification metavariable and intern its node, returning both handles.
    ///
    /// Each call mints a new [`MetaId`] (metavariables are identity-bearing), so two
    /// `fresh_meta` calls yield distinct nodes; re-interning the same `MetaId` shares one
    /// node.
    pub fn fresh_meta(&mut self) -> (MetaId, NodeId) {
        let m = MetaId::from_index(self.meta_count);
        self.meta_count += 1;
        let node = self.intern(NodeData::Meta(m));
        (m, node)
    }

    /// Intern a bound-variable occurrence at de-Bruijn `debruijn`/`slot`.
    pub fn intern_bound(&mut self, debruijn: u32, slot: u16) -> NodeId {
        self.intern(NodeData::Bound { debruijn, slot })
    }

    /// Intern a function application `op(args…)`.  `op` and every arg MUST be nodes of
    /// THIS DAG.
    pub fn intern_app(&mut self, op: NodeId, args: Vec<NodeId>) -> NodeId {
        self.intern(NodeData::App {
            op,
            args: args.into_boxed_slice(),
        })
    }

    /// Intern a binder block `op[sorts…]. body`.  `op`, every sort, and `body` MUST be
    /// nodes of THIS DAG; `body`'s de-Bruijn refs see this binder at distance 0.
    pub fn intern_binder(&mut self, op: NodeId, sorts: Vec<NodeId>, body: NodeId) -> NodeId {
        self.intern(NodeData::Binder {
            op,
            sorts: sorts.into_boxed_slice(),
            body,
        })
    }

    // ── The total interning core ─────────────────────────────────────────────────

    /// Intern `data`, returning its node id — a new insertion-ordered id if its content
    /// key is new, else the existing id (structural memoization / hash-consing).
    ///
    /// TOTAL over [`NodeData`]: every kind folds to a content key ([`crate::term_key`]);
    /// there is no string fallback and no failure mode.
    ///
    /// Public because the engine tier's consumers (unification, proof re-derivation, the
    /// structured resolver) rebuild nodes from a matched [`NodeData`] shape. The children
    /// it references MUST be nodes of THIS DAG — the same per-DAG handle contract every
    /// accessor above carries.
    pub fn intern(&mut self, data: NodeData) -> NodeId {
        // Count the CALL (hits included) before the probe: the scoped-delta measurement
        // needs "how many times was interning asked for", not "how many nodes were minted".
        self.intern_calls += 1;
        // Fold the content key from the children's already-cached keys (bottom-up).
        let key = term_key::content_key(self, &data);
        let hash = surface_hash(&key);
        // Borrowed-key probe: resolve each candidate id to its key slice in the side
        // arena — no re-fold, no owned-key clone.
        let keys = &self.keys;
        if let Some(&id) = self
            .by_key
            .find(hash, |&id| &*keys[id.index()] == key.as_str())
        {
            return id;
        }
        // Miss: compute the free-metavar set from the children's cached sets, then push
        // node + key + set in lockstep and record the id.
        let free_meta = self.compute_free_meta(&data);
        let id = NodeId::from_index(self.nodes.len());
        self.nodes.push(data);
        self.keys.push(key.into_boxed_str());
        self.free_meta.push(free_meta);
        let keys = &self.keys;
        self.by_key
            .insert_unique(hash, id, |&id| surface_hash(&keys[id.index()]));
        id
    }

    /// The free-metavariable set of `data`, unioned bottom-up from its children's cached
    /// sets.  Object binders do NOT bind metavariables, so nothing is removed at a
    /// [`NodeData::Binder`] — a metavariable stays free through binder scope (exactly what
    /// the occurs-check needs).
    fn compute_free_meta(&self, data: &NodeData) -> MetaSet {
        match data {
            NodeData::Leaf(_) | NodeData::Free(_) | NodeData::Bound { .. } => MetaSet::empty(),
            NodeData::Meta(m) => MetaSet::singleton(*m),
            NodeData::App { op, args } => {
                let mut set = self.free_meta(*op).clone();
                for arg in args.iter() {
                    set.union_with(self.free_meta(*arg));
                }
                set
            }
            NodeData::Binder { op, sorts, body } => {
                let mut set = self.free_meta(*op).clone();
                for sort in sorts.iter() {
                    set.union_with(self.free_meta(*sort));
                }
                set.union_with(self.free_meta(*body));
                set
            }
        }
    }
}
