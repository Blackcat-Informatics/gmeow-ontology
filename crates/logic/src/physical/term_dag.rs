// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The persistent hash-consed, binder-aware, content-addressed structured-term DAG.
//!
//! # What this is
//!
//! [`TermDag`] is the persistent term arena for *structured* terms — function-symbol
//! applications, binders, and proof-object trees — the seam that
//! [`crate::physical::id::TermRef`] documents growing out of the atomic
//! [`crate::facts::TermInterner`].  It mirrors that interner's borrowed-key discipline:
//! a node is interned once and addressed by a dense insertion-ordered [`NodeId`], its
//! content key lives once in a side arena, and a borrowed-`&str` probe resolves a
//! candidate id against that arena with no owned-key clone and no re-render.
//!
//! It is **persistent** — never reset within its lifetime — and is therefore distinct
//! from [`crate::physical::arena::RowArena`], the per-round row/tuple bump arena that
//! truncates every round.  This module touches neither `RowArena` nor `TermRef`.
//!
//! # Hash-consing ⇒ structural sharing ⇒ alpha-equivalence is `NodeId` equality
//!
//! Every node is content-keyed bottom-up ([`crate::physical::term_key`]): a node's key is
//! folded from its children's already-cached keys, so interning a structurally-identical
//! term a second time returns the SAME `NodeId` (maximal sharing).  Bound variables are
//! **locally-nameless** — a [`NodeData::Bound`] carries a de-Bruijn distance to its binder
//! plus a slot ordinal, never a name — so two alpha-equivalent terms are *the same tree*
//! and hence the same `NodeId`.  Alpha-equivalence is thus decided by `Copy` integer
//! equality, and the free-metavariable set of each node is cached for an `O(1)`
//! occurs-check on the unification rungs to come.
//!
//! # Determinism doctrine (inherited)
//!
//! `NodeId`/`MetaId` are runtime handles: assigned in insertion / mint order, meaningless
//! outside the DAG that minted them, NEVER serialized and NEVER hashed for provenance.
//! The content key is the persistent identity; the dense integers never escape the
//! runtime (the [`crate::physical::id`] doctrine).

use std::hash::BuildHasher;

use hashbrown::HashTable;
use purrdf::TermValue;
use smallvec::SmallVec;

use crate::facts::TermInterner;
use crate::physical::id::{MetaId, NodeId, TermId};
use crate::physical::term_key;

/// A structured-term DAG node.
///
/// **Locally-nameless**: a bound occurrence is a de-Bruijn [`NodeData::Bound`] ref, so
/// alpha-equivalence is structural `NodeData` equality (and, after interning, `NodeId`
/// equality).  Derives structural `Eq`/`Hash` so the DAG can key on the node itself where
/// convenient; canonical identity nonetheless always flows through the content key.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub(crate) enum NodeData {
    /// An interned atomic IRI/literal leaf, delegated to the [`TermInterner`].
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
pub(crate) struct MetaSet(SmallVec<[MetaId; 4]>);

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
    pub(crate) fn contains(&self, m: MetaId) -> bool {
        self.0.binary_search(&m).is_ok()
    }

    /// The free metavariables, in ascending [`MetaId`] order.
    #[inline]
    pub(crate) fn iter(&self) -> impl Iterator<Item = MetaId> + '_ {
        self.0.iter().copied()
    }

    /// The number of distinct free metavariables.
    #[inline]
    pub(crate) fn len(&self) -> usize {
        self.0.len()
    }

    /// Whether the node has no free metavariables (a fully-ground / metavariable-free term).
    #[inline]
    pub(crate) fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

/// Fixed-seed hash of a content key, for the DAG's borrowed-key probe (mirrors
/// [`crate::facts`]'s interner: the seed is fixed, never persisted — determinism comes
/// from insertion order, never this hash).
#[inline]
fn key_hash(key: &str) -> u64 {
    foldhash::fast::FixedState::default().hash_one(key)
}

/// A persistent hash-consed structured-term DAG.
///
/// Mirrors [`crate::facts::TermInterner`]: dense insertion-ordered handles, the content
/// key stored once in a side arena, and a borrowed-key [`HashTable`] probe.
#[derive(Debug, Default)]
pub(crate) struct TermDag {
    /// Atomic leaves reuse the fact interner verbatim: intern a [`TermValue`] → [`TermId`].
    atoms: TermInterner,
    /// Nodes in insertion order (slot = [`NodeId`] index).
    nodes: Vec<NodeData>,
    /// Cached content key per node, in lockstep with `nodes` — the side arena the
    /// `by_key` probe resolves against (the `displays`-style arena of `TermInterner`).
    keys: Vec<Box<str>>,
    /// Cached free-metavariable set per node, for an `O(1)` occurs-check.
    free_meta: Vec<MetaSet>,
    /// Content key → id, for `O(1)` intern/lookup.  Holds the [`NodeId`] ONLY: the key
    /// bytes live once in `keys`, so a borrowed-`&str` probe resolves a candidate id with
    /// a `keys[id.index()]` slice compare — the eq/hash closure never re-folds a key.
    by_key: HashTable<NodeId>,
    /// The number of metavariables minted so far — the next [`MetaId`] ordinal.
    meta_count: usize,
}

impl TermDag {
    /// A fresh, empty DAG.
    pub(crate) fn new() -> Self {
        Self::default()
    }

    // ── Accessors ───────────────────────────────────────────────────────────────

    /// The node data for `id`.
    ///
    /// # Panics
    ///
    /// Panics if `id` was not minted by this DAG — `NodeId`s are per-DAG handles.
    pub(crate) fn data(&self, id: NodeId) -> &NodeData {
        self.nodes.get(id.index()).unwrap_or_else(|| {
            panic!(
                "NodeId {id:?} was not minted by this DAG (len {}): NodeIds are per-DAG handles",
                self.nodes.len()
            )
        })
    }

    /// The cached content key for `id` (same per-DAG panic contract as [`Self::data`]).
    pub(crate) fn key(&self, id: NodeId) -> &str {
        self.keys.get(id.index()).map(|k| &**k).unwrap_or_else(|| {
            panic!(
                "NodeId {id:?} was not minted by this DAG (len {}): NodeIds are per-DAG handles",
                self.keys.len()
            )
        })
    }

    /// The cached free-metavariable set for `id` (same per-DAG panic contract).
    pub(crate) fn free_meta(&self, id: NodeId) -> &MetaSet {
        self.free_meta.get(id.index()).unwrap_or_else(|| {
            panic!(
                "NodeId {id:?} was not minted by this DAG (len {}): NodeIds are per-DAG handles",
                self.free_meta.len()
            )
        })
    }

    /// The cached display surface of an atomic leaf, for the content-key fold.
    pub(crate) fn atom_display(&self, id: TermId) -> &str {
        self.atoms.display_of(id)
    }

    /// The number of distinct nodes interned.
    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.nodes.len()
    }

    // ── Constructors ──────────────────────────────────────────────────────────────

    /// Intern an atomic leaf (an IRI/literal), returning its node id.
    pub(crate) fn intern_leaf(&mut self, tv: TermValue) -> NodeId {
        let atom = self.atoms.intern(&tv);
        self.intern(NodeData::Leaf(atom))
    }

    /// Intern a named free-variable occurrence, returning its node id.
    pub(crate) fn intern_free(&mut self, tv: TermValue) -> NodeId {
        let atom = self.atoms.intern(&tv);
        self.intern(NodeData::Free(atom))
    }

    /// Mint a FRESH unification metavariable and intern its node, returning both handles.
    ///
    /// Each call mints a new [`MetaId`] (metavariables are identity-bearing), so two
    /// `fresh_meta` calls yield distinct nodes; re-interning the same `MetaId` shares one
    /// node.
    pub(crate) fn fresh_meta(&mut self) -> (MetaId, NodeId) {
        let m = MetaId::from_index(self.meta_count);
        self.meta_count += 1;
        let node = self.intern(NodeData::Meta(m));
        (m, node)
    }

    /// Intern a bound-variable occurrence at de-Bruijn `debruijn`/`slot`.
    pub(crate) fn intern_bound(&mut self, debruijn: u32, slot: u16) -> NodeId {
        self.intern(NodeData::Bound { debruijn, slot })
    }

    /// Intern a function application `op(args…)`.  `op` and every arg MUST be nodes of
    /// THIS DAG.
    pub(crate) fn intern_app(&mut self, op: NodeId, args: Vec<NodeId>) -> NodeId {
        self.intern(NodeData::App {
            op,
            args: args.into_boxed_slice(),
        })
    }

    /// Intern a binder block `op[sorts…]. body`.  `op`, every sort, and `body` MUST be
    /// nodes of THIS DAG; `body`'s de-Bruijn refs see this binder at distance 0.
    pub(crate) fn intern_binder(&mut self, op: NodeId, sorts: Vec<NodeId>, body: NodeId) -> NodeId {
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
    /// TOTAL over [`NodeData`]: every kind folds to a content key
    /// ([`crate::physical::term_key`]); there is no string fallback and no failure mode.
    fn intern(&mut self, data: NodeData) -> NodeId {
        // Fold the content key from the children's already-cached keys (bottom-up).
        let key = term_key::content_key(self, &data);
        let hash = key_hash(&key);
        // Borrowed-key probe: resolve each candidate id to its key slice in the side
        // arena — no re-fold, no owned-key clone (the `facts.rs` idiom).
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
            .insert_unique(hash, id, |&id| key_hash(&keys[id.index()]));
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

#[cfg(test)]
mod tests {
    use super::*;

    use gmeow_logic_compile::ir::{Formula, Term};
    use proptest::prelude::*;

    fn config() -> ProptestConfig {
        let cases = std::env::var("PROPTEST_CASES")
            .ok()
            .and_then(|v| v.parse::<u32>().ok())
            .unwrap_or(256);
        ProptestConfig {
            cases,
            failure_persistence: None,
            ..ProptestConfig::default()
        }
    }

    fn iri(s: &str) -> TermValue {
        TermValue::iri(s)
    }

    // ── (a) alpha-normalization / interning: same de-Bruijn structure interns once ────

    #[test]
    fn dag_alpha_equal_terms_intern_to_one_node_and_key() {
        // `∀_. p(bound{0,0})` built twice — via independently-constructed child nodes —
        // must intern to ONE NodeId with a byte-identical content key.
        let build = |dag: &mut TermDag| {
            let p = dag.intern_leaf(iri("https://example.org/p"));
            let bound = dag.intern_bound(0, 0);
            let body = dag.intern_app(p, vec![bound]);
            let sort = dag.intern_leaf(iri("https://example.org/Sort"));
            let forall = dag.intern_leaf(iri("https://example.org/forall"));
            dag.intern_binder(forall, vec![sort], body)
        };

        let mut dag = TermDag::new();
        let first = build(&mut dag);
        let len_after_first = dag.len();
        let second = build(&mut dag);

        assert_eq!(first, second, "alpha-equal terms must share one NodeId");
        assert_eq!(
            dag.len(),
            len_after_first,
            "re-interning the same structure must mint no new nodes"
        );
        assert_eq!(
            dag.key(first),
            dag.key(second),
            "alpha-equal terms must have byte-identical content keys"
        );
    }

    // ── (b) negative alpha: a binder differing only in a `sorts` child differs ─────────

    #[test]
    fn dag_binder_differing_in_sort_is_distinct() {
        let mut dag = TermDag::new();
        let forall = dag.intern_leaf(iri("https://example.org/forall"));
        let body = dag.intern_bound(0, 0);
        let sort_a = dag.intern_leaf(iri("https://example.org/A"));
        let sort_b = dag.intern_leaf(iri("https://example.org/B"));

        let binder_a = dag.intern_binder(forall, vec![sort_a], body);
        let binder_b = dag.intern_binder(forall, vec![sort_b], body);

        assert_ne!(
            binder_a, binder_b,
            "binders over distinct sorts must be distinct nodes"
        );
        assert_ne!(
            dag.key(binder_a),
            dag.key(binder_b),
            "binders over distinct sorts must have distinct content keys"
        );
    }

    // ── metavariable identity + cached free-metavar set ────────────────────────────────

    #[test]
    fn dag_metavars_are_identity_bearing_and_tracked() {
        let mut dag = TermDag::new();
        let (m0, n0) = dag.fresh_meta();
        let (m1, n1) = dag.fresh_meta();
        assert_ne!(m0, m1, "each fresh_meta mints a distinct metavariable");
        assert_ne!(n0, n1, "distinct metavariables are distinct nodes");
        assert_ne!(dag.key(n0), dag.key(n1), "distinct metavar keys");

        // Re-interning the SAME metavariable shares one node.
        let n0_again = dag.intern(NodeData::Meta(m0));
        assert_eq!(n0, n0_again, "the same MetaId shares one node");

        // Free-metavar sets: a leaf is empty; a metavar is its singleton; an application
        // is the union of its children's sets.
        let p = dag.intern_leaf(iri("https://example.org/p"));
        assert!(dag.free_meta(p).is_empty());
        assert!(dag.free_meta(n0).contains(m0));
        assert!(!dag.free_meta(n0).contains(m1));

        let app = dag.intern_app(p, vec![n0, n1]);
        let fm = dag.free_meta(app);
        assert_eq!(fm.len(), 2, "app over two distinct metavars has both free");
        assert!(fm.contains(m0) && fm.contains(m1));
        assert_eq!(fm.iter().collect::<Vec<_>>(), vec![m0, m1]);

        // A metavariable stays free through binder scope (occurs-check contract).
        let sort = dag.intern_leaf(iri("https://example.org/Sort"));
        let forall = dag.intern_leaf(iri("https://example.org/forall"));
        let binder = dag.intern_binder(forall, vec![sort], n0);
        assert!(
            dag.free_meta(binder).contains(m0),
            "object binders do not bind metavariables"
        );
    }

    // ── (c) anti-collision fuzz: distinct structure ⟺ distinct key ⟺ distinct NodeId ──

    /// A generator spec that maps INJECTIVELY onto [`NodeData`] trees: two specs are
    /// `PartialEq`-equal exactly when they build the same node.  The collision property
    /// then reduces to: spec-equality ⟺ key-equality ⟺ NodeId-equality.
    #[derive(Clone, Debug, PartialEq, Eq)]
    enum Spec {
        Leaf(String),
        Free(String),
        Bound(u32, u16),
        App(Box<Spec>, Vec<Spec>),
        Binder(Box<Spec>, Vec<Spec>, Box<Spec>),
    }

    fn build_spec(dag: &mut TermDag, spec: &Spec) -> NodeId {
        match spec {
            Spec::Leaf(s) => dag.intern_leaf(TermValue::simple_literal(s.clone())),
            Spec::Free(s) => dag.intern_free(TermValue::simple_literal(s.clone())),
            Spec::Bound(d, slot) => dag.intern_bound(*d, *slot),
            Spec::App(op, args) => {
                let op = build_spec(dag, op);
                let args = args.iter().map(|a| build_spec(dag, a)).collect();
                dag.intern_app(op, args)
            }
            Spec::Binder(op, sorts, body) => {
                let op = build_spec(dag, op);
                let sorts = sorts.iter().map(|s| build_spec(dag, s)).collect();
                let body = build_spec(dag, body);
                dag.intern_binder(op, sorts, body)
            }
        }
    }

    /// Adversarial leaf/free bytes: strings that embed the netstring separators (`:`),
    /// mimic kind tags (`I`/`V`/`APP`/`BIND`), embed a decimal-then-colon length prefix,
    /// or carry a NUL — exactly the bytes a bare-separator scheme would conflate.
    fn arb_leaf_bytes() -> impl Strategy<Value = String> {
        prop_oneof![
            Just(String::new()),
            Just("I".to_owned()),
            Just("V".to_owned()),
            Just("APP".to_owned()),
            Just("BIND".to_owned()),
            Just("3:foo".to_owned()),
            Just("1:0".to_owned()),
            Just(":".to_owned()),
            Just("5:".to_owned()),
            Just("free_".to_owned()),
            Just("a\u{0}b".to_owned()),
            Just("\u{0}".to_owned()),
            "[0-9:IVAPPBIND\u{0}a-z]{0,6}",
        ]
    }

    fn arb_spec() -> impl Strategy<Value = Spec> {
        let leaf = prop_oneof![
            arb_leaf_bytes().prop_map(Spec::Leaf),
            arb_leaf_bytes().prop_map(Spec::Free),
            (0u32..3, 0u16..3).prop_map(|(d, s)| Spec::Bound(d, s)),
        ];
        leaf.prop_recursive(4, 32, 3, |inner| {
            prop_oneof![
                (inner.clone(), prop::collection::vec(inner.clone(), 0..3))
                    .prop_map(|(op, args)| Spec::App(Box::new(op), args)),
                (
                    inner.clone(),
                    prop::collection::vec(inner.clone(), 0..3),
                    inner.clone()
                )
                    .prop_map(|(op, sorts, body)| Spec::Binder(
                        Box::new(op),
                        sorts,
                        Box::new(body)
                    )),
            ]
        })
    }

    proptest! {
        #![proptest_config(config())]

        /// The core injectivity property: over adversarial structures, spec-equality,
        /// content-key equality, and `NodeId` equality all coincide.  A `s1 != s2` pair
        /// that shared a key (`k1 == k2`) would be an encoding COLLISION; a `s1 == s2`
        /// pair that got two ids would be an interning bug.  Both are caught here.
        #[test]
        fn dag_key_is_injective_and_interns(s1 in arb_spec(), s2 in arb_spec()) {
            let mut dag = TermDag::new();
            let id1 = build_spec(&mut dag, &s1);
            let id2 = build_spec(&mut dag, &s2);
            let k1 = dag.key(id1).to_owned();
            let k2 = dag.key(id2).to_owned();

            prop_assert_eq!(
                s1 == s2,
                k1 == k2,
                "structural equality must coincide with content-key equality (no collision)"
            );
            prop_assert_eq!(
                s1 == s2,
                id1 == id2,
                "structural equality must coincide with hash-consed NodeId equality"
            );
        }
    }

    #[test]
    fn dag_meta_keys_are_injective() {
        // A focused check that identity-bearing metavariable ordinals never collide with
        // one another (the proptest corpus above deliberately omits Meta, whose id cannot
        // be pinned through `fresh_meta`).
        let mut dag = TermDag::new();
        let mut keys = std::collections::HashSet::new();
        let mut ids = std::collections::HashSet::new();
        for _ in 0..64 {
            let (_, node) = dag.fresh_meta();
            assert!(
                keys.insert(dag.key(node).to_owned()),
                "metavar keys are distinct"
            );
            assert!(ids.insert(node), "metavar nodes are distinct");
        }
    }

    // ── (d) DAG ↔ ir.rs congruence ─────────────────────────────────────────────────────

    // Reserved binder/connective operator IRIs, in a namespace disjoint from the object
    // relation IRIs used by the corpus (`example.org`), so a connective App can never be
    // conflated with an object-relation Atom.  The full three-consumer lowering (Task 3)
    // formalizes this reservation; this focused lowering only needs the corpus it tests.
    const OP_NS: &str = "https://blackcatinformatics.ca/logic/dag/op/";
    const SORT_IRI: &str = "https://blackcatinformatics.ca/logic/dag/sort/individual";

    /// Intern a reserved operator leaf.
    fn op_leaf(dag: &mut TermDag, local: &str) -> NodeId {
        dag.intern_leaf(iri(&format!("{OP_NS}{local}")))
    }

    /// Resolve a name against the binder-frame stack (innermost frame last) to a de-Bruijn
    /// `(distance, slot)`, or `None` if free.
    fn resolve_debruijn(env: &[Vec<String>], name: &str) -> Option<(u32, u16)> {
        for (back, frame) in env.iter().rev().enumerate() {
            if let Some(slot) = frame.iter().position(|v| v == name) {
                return Some((back as u32, slot as u16));
            }
        }
        None
    }

    /// Lower an `ir::Term` into the DAG under the current binder environment.
    fn lower_term(
        dag: &mut TermDag,
        term: &Term,
        env: &[Vec<String>],
    ) -> gmeow_errors::Result<NodeId> {
        Ok(match term {
            Term::Iri(s) => dag.intern_leaf(iri(s)),
            Term::Literal { lexical, datatype } => {
                let tv = match datatype {
                    None => TermValue::simple_literal(lexical.clone()),
                    Some(dt) => TermValue::typed_literal(lexical.clone(), dt.clone()),
                };
                dag.intern_leaf(tv)
            }
            Term::Var(name) => match resolve_debruijn(env, name) {
                Some((d, slot)) => dag.intern_bound(d, slot),
                None => dag.intern_free(TermValue::simple_literal(name.clone())),
            },
            Term::SequenceMarker(_) => {
                return Err(gmeow_errors::Diag::of_kind(
                    gmeow_logic_compile::error::Ir {
                        detail: "sequence markers are variadic and out of scope for the focused \
                                 Task-2 DAG lowering (generalized in Task 3)"
                            .to_owned(),
                    },
                ));
            }
        })
    }

    /// Flatten a commutative connective's same-tag operands, mirroring `ir.rs`'s
    /// `flatten_commutative`, so `And[And[a,b],c] ≡ And[a,b,c]`.
    fn flatten_commutative<'a>(is_and: bool, fs: &'a [Formula], out: &mut Vec<&'a Formula>) {
        for f in fs {
            match (is_and, f) {
                (true, Formula::And(inner)) => flatten_commutative(is_and, inner, out),
                (false, Formula::Or(inner)) => flatten_commutative(is_and, inner, out),
                _ => out.push(f),
            }
        }
    }

    /// Lower an `ir::Formula` into the DAG.
    ///
    /// This is a FOCUSED lowering for the congruence corpus (the full three-consumer
    /// lowering is Task 3).  It reproduces exactly the equivalences `ir::Formula::content_key`
    /// decides: bound-variable alpha-renaming (via locally-nameless de-Bruijn), commutative
    /// flatten+order-normalization of `And`/`Or`/`Iff`, and ordered `Implies`.
    fn lower_formula(
        dag: &mut TermDag,
        f: &Formula,
        env: &mut Vec<Vec<String>>,
    ) -> gmeow_errors::Result<NodeId> {
        Ok(match f {
            Formula::Atom { relation, args } => {
                let op = lower_term(dag, relation, env)?;
                let mut arg_nodes = Vec::with_capacity(args.len());
                for a in args {
                    arg_nodes.push(lower_term(dag, a, env)?);
                }
                dag.intern_app(op, arg_nodes)
            }
            Formula::Not(b) => {
                let op = op_leaf(dag, "not");
                let child = lower_formula(dag, b, env)?;
                dag.intern_app(op, vec![child])
            }
            Formula::And(fs) => {
                let op = op_leaf(dag, "and");
                lower_commutative(dag, true, op, fs, env)?
            }
            Formula::Or(fs) => {
                let op = op_leaf(dag, "or");
                lower_commutative(dag, false, op, fs, env)?
            }
            Formula::Implies(a, b) => {
                let op = op_leaf(dag, "implies");
                let la = lower_formula(dag, a, env)?;
                let lb = lower_formula(dag, b, env)?;
                dag.intern_app(op, vec![la, lb])
            }
            Formula::Iff(a, b) => {
                let op = op_leaf(dag, "iff");
                let mut pair = [lower_formula(dag, a, env)?, lower_formula(dag, b, env)?];
                pair.sort();
                dag.intern_app(op, pair.to_vec())
            }
            Formula::Forall { vars, body } => lower_binder(dag, "forall", vars, body, env)?,
            Formula::Exists { vars, body } => lower_binder(dag, "exists", vars, body, env)?,
        })
    }

    /// Lower a flattened, order-normalized commutative connective.  Interning the operands
    /// yields NodeIds that are order-independent, so sorting them canonicalizes operand
    /// order exactly as `ir.rs` sorts operand keys (duplicates preserved).
    fn lower_commutative(
        dag: &mut TermDag,
        is_and: bool,
        op: NodeId,
        fs: &[Formula],
        env: &mut Vec<Vec<String>>,
    ) -> gmeow_errors::Result<NodeId> {
        let mut operands: Vec<&Formula> = Vec::new();
        flatten_commutative(is_and, fs, &mut operands);
        let mut nodes = Vec::with_capacity(operands.len());
        for f in operands {
            nodes.push(lower_formula(dag, f, env)?);
        }
        nodes.sort();
        Ok(dag.intern_app(op, nodes))
    }

    /// Lower a quantifier binder.  Each bound variable becomes a slot with an (untyped)
    /// individual sort, so the binder's arity is captured; the body is lowered one
    /// binder-depth deeper via a pushed frame.
    fn lower_binder(
        dag: &mut TermDag,
        op_local: &str,
        vars: &[String],
        body: &Formula,
        env: &mut Vec<Vec<String>>,
    ) -> gmeow_errors::Result<NodeId> {
        let op = op_leaf(dag, op_local);
        let sort = dag.intern_leaf(iri(SORT_IRI));
        let sorts = vec![sort; vars.len()];
        env.push(vars.to_vec());
        let body_node = lower_formula(dag, body, env);
        env.pop();
        let body_node = body_node?;
        Ok(dag.intern_binder(op, sorts, body_node))
    }

    fn tvar(name: &str) -> Term {
        Term::var(name).expect("non-empty var name")
    }

    fn tiri(iri: &str) -> Term {
        Term::iri(iri).expect("non-empty iri")
    }

    fn atom(relation: &str, args: Vec<Term>) -> Formula {
        Formula::atom(tiri(relation), args).expect("iri relation")
    }

    #[test]
    fn dag_congruent_with_ir_content_key() {
        const P: &str = "https://example.org/p";
        const Q: &str = "https://example.org/q";
        const R: &str = "https://example.org/r";
        const A: &str = "https://example.org/a";
        const B: &str = "https://example.org/b";

        // A corpus spanning Atom / And / Or / Not / Implies / Iff / Forall / Exists / Var /
        // Iri / Literal, including alpha-variants and commutative-variants (which must
        // COLLAPSE) and sign/order/arity variants (which must stay DISTINCT).
        let lit = Term::literal("v", None).expect("literal");
        let corpus: Vec<(&str, Formula)> = vec![
            ("atom_pab", atom(P, vec![tiri(A), tiri(B)])),
            ("atom_pba", atom(P, vec![tiri(B), tiri(A)])),
            ("atom_lit", atom(P, vec![tiri(A), lit])),
            (
                "and_pq",
                Formula::And(vec![atom(P, vec![tiri(A)]), atom(Q, vec![tiri(A)])]),
            ),
            (
                "and_qp",
                Formula::And(vec![atom(Q, vec![tiri(A)]), atom(P, vec![tiri(A)])]),
            ),
            (
                "and_nested",
                Formula::And(vec![
                    Formula::And(vec![atom(P, vec![tiri(A)]), atom(Q, vec![tiri(A)])]),
                    atom(R, vec![tiri(A)]),
                ]),
            ),
            (
                "and_flat3",
                Formula::And(vec![
                    atom(P, vec![tiri(A)]),
                    atom(Q, vec![tiri(A)]),
                    atom(R, vec![tiri(A)]),
                ]),
            ),
            (
                "or_pq",
                Formula::Or(vec![atom(P, vec![tiri(A)]), atom(Q, vec![tiri(A)])]),
            ),
            ("not_pa", Formula::Not(Box::new(atom(P, vec![tiri(A)])))),
            (
                "impl_pq",
                Formula::Implies(
                    Box::new(atom(P, vec![tiri(A)])),
                    Box::new(atom(Q, vec![tiri(A)])),
                ),
            ),
            (
                "impl_qp",
                Formula::Implies(
                    Box::new(atom(Q, vec![tiri(A)])),
                    Box::new(atom(P, vec![tiri(A)])),
                ),
            ),
            (
                "iff_pq",
                Formula::Iff(
                    Box::new(atom(P, vec![tiri(A)])),
                    Box::new(atom(Q, vec![tiri(A)])),
                ),
            ),
            (
                "iff_qp",
                Formula::Iff(
                    Box::new(atom(Q, vec![tiri(A)])),
                    Box::new(atom(P, vec![tiri(A)])),
                ),
            ),
            (
                "forall_x_px",
                Formula::Forall {
                    vars: vec!["x".to_owned()],
                    body: Box::new(atom(P, vec![tvar("x")])),
                },
            ),
            (
                "forall_y_py",
                Formula::Forall {
                    vars: vec!["y".to_owned()],
                    body: Box::new(atom(P, vec![tvar("y")])),
                },
            ),
            (
                "exists_x_px",
                Formula::Exists {
                    vars: vec!["x".to_owned()],
                    body: Box::new(atom(P, vec![tvar("x")])),
                },
            ),
            (
                "forall_xy_rxy",
                Formula::Forall {
                    vars: vec!["x".to_owned(), "y".to_owned()],
                    body: Box::new(atom(R, vec![tvar("x"), tvar("y")])),
                },
            ),
            (
                "forall_uv_ruv",
                Formula::Forall {
                    vars: vec!["u".to_owned(), "v".to_owned()],
                    body: Box::new(atom(R, vec![tvar("u"), tvar("v")])),
                },
            ),
            (
                "forall_xy_ryx",
                Formula::Forall {
                    vars: vec!["x".to_owned(), "y".to_owned()],
                    body: Box::new(atom(R, vec![tvar("y"), tvar("x")])),
                },
            ),
            (
                "forall_x_forall_y_rxy",
                Formula::Forall {
                    vars: vec!["x".to_owned()],
                    body: Box::new(Formula::Forall {
                        vars: vec!["y".to_owned()],
                        body: Box::new(atom(R, vec![tvar("x"), tvar("y")])),
                    }),
                },
            ),
            (
                "forall_a_forall_b_rab",
                Formula::Forall {
                    vars: vec!["a".to_owned()],
                    body: Box::new(Formula::Forall {
                        vars: vec!["b".to_owned()],
                        body: Box::new(atom(R, vec![tvar("a"), tvar("b")])),
                    }),
                },
            ),
            (
                "forall_free_y_pxy",
                // `x` is free (never bound), `y` is bound — free-variable identity is by
                // NAME and must NOT be alpha-collapsed.
                Formula::Forall {
                    vars: vec!["y".to_owned()],
                    body: Box::new(atom(R, vec![tvar("x"), tvar("y")])),
                },
            ),
        ];

        // Lower the whole corpus into ONE shared DAG so NodeId equality is comparable.
        let mut dag = TermDag::new();
        let mut lowered: Vec<(&str, NodeId, String)> = Vec::with_capacity(corpus.len());
        for (label, formula) in &corpus {
            let mut env: Vec<Vec<String>> = Vec::new();
            let node = lower_formula(&mut dag, formula, &mut env)
                .unwrap_or_else(|e| panic!("lowering {label} failed: {e:?}"));
            lowered.push((label, node, formula.content_key()));
        }

        // The biconditional over every ordered pair: alpha/commutative-equal ⟺ same key
        // ⟺ same NodeId.
        for (la, na, ka) in &lowered {
            for (lb, nb, kb) in &lowered {
                let ir_eq = ka == kb;
                let dag_eq = na == nb;
                assert_eq!(
                    ir_eq, dag_eq,
                    "congruence violated for ({la}, {lb}): ir_key_eq={ir_eq} dag_node_eq={dag_eq}\n\
                     ka={ka}\nkb={kb}"
                );
            }
        }

        // Sanity: the intended collapses and separations are actually present (guards
        // against a vacuous corpus where every pair is trivially distinct).
        let node = |name: &str| lowered.iter().find(|(l, ..)| *l == name).unwrap().1;
        assert_eq!(node("and_pq"), node("and_qp"), "And is commutative");
        assert_eq!(node("and_nested"), node("and_flat3"), "And is associative");
        assert_eq!(node("iff_pq"), node("iff_qp"), "Iff is commutative");
        assert_eq!(node("forall_x_px"), node("forall_y_py"), "alpha-equal ∀");
        assert_eq!(
            node("forall_x_forall_y_rxy"),
            node("forall_a_forall_b_rab"),
            "alpha-equal nested ∀"
        );
        assert_eq!(
            node("forall_xy_rxy"),
            node("forall_uv_ruv"),
            "alpha-equal 2-var ∀"
        );
        assert_ne!(node("atom_pab"), node("atom_pba"), "arg order matters");
        assert_ne!(node("impl_pq"), node("impl_qp"), "Implies is ordered");
        assert_ne!(
            node("forall_x_px"),
            node("exists_x_px"),
            "∀ and ∃ are distinct"
        );
        assert_ne!(
            node("forall_xy_rxy"),
            node("forall_xy_ryx"),
            "bound-var order in body matters"
        );
    }
}
