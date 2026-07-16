// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Robinson unification with occurs-check over the hash-consed term DAG.
//!
//! # What this is
//!
//! First-order (Robinson) unification over [`NodeId`], with a union-find
//! substitution over [`MetaId`]. It is the first consumer of the persistent
//! [`TermDag`]'s two structural gifts:
//!
//! - **Hash-consing ⇒ `O(1)` short-circuit.** Alpha-equivalent terms are the SAME
//!   `NodeId` (locally-nameless de-Bruijn), so `a == b` after resolution decides
//!   unification trivially — no structural walk, no alpha-renaming.
//! - **Cached free-metavariable sets ⇒ fast occurs-check.** [`TermDag::free_meta`]
//!   is the exact, bottom-up-cached support of each node, so the occurs-check tests
//!   membership against a sorted set rather than re-walking the term. The check
//!   FOLLOWS the current substitution ([`occurs_through`]): the raw cache is the
//!   `O(1)` fast path, but a metavariable already bound to a term that reintroduces
//!   the binder would make the raw cache *insufficient*, so when a free metavariable
//!   of the candidate is itself bound the check descends structurally — never
//!   accepting a cyclic term (the soundness contract).
//!
//! # Substitution is triangular (union-find), not eager
//!
//! [`Subst`] binds `MetaId -> Option<NodeId>` in a dense vector indexed by
//! [`MetaId::index`]. A binding stores the RESOLVED representative, not the fully
//! expanded term, so binding is `O(1)`. [`Subst::resolve`] is the SINGLE identity
//! primitive — it walks metavariable bindings to their representative and is the one
//! place "what does this node resolve to" is answered. A future congruence/e-class
//! layer becomes an indirection over `resolve`, so every consumer routes through it.
//!
//! # Capture-avoidance is the de-Bruijn shift, and nothing else
//!
//! [`apply`] materializes a substitution by re-interning a node with each resolved
//! metavariable replaced by its binding. Because the DAG is locally-nameless there are
//! NO names to freshen: a metavariable's solution `t`, spliced UNDER `k` binders, only
//! needs its free de-Bruijn indices lifted by `k` — the [`shift`]. The shift IS the
//! entire capture-avoidance; a bound occurrence can never be captured by an intervening
//! binder because its distance is corrected structurally. Every node `apply`/`shift`
//! newly interns flows through the `TermDag::intern_*` constructors, so its cached
//! free-metavariable set is recomputed exactly — never stale (a stale set would be a
//! false "no occurs" and hence an accepted cyclic term, i.e. unsoundness).

use std::collections::{HashMap, HashSet};

use crate::physical::id::{MetaId, NodeId};
use crate::physical::term_dag::{NodeData, TermDag};

/// A union-find substitution over unification metavariables.
///
/// Binds `MetaId -> Option<NodeId>` in a dense vector indexed by [`MetaId::index`]; a
/// binding stores the RESOLVED representative, so a bind is `O(1)` and identity
/// resolution is a metavariable-chain walk ([`Self::resolve`]) rather than an eager
/// rewrite. The map only ever grows.
#[derive(Debug, Default, Clone)]
pub(crate) struct Subst {
    /// Slot `MetaId::index()` holds that metavariable's binding, or `None` if unbound.
    bindings: Vec<Option<NodeId>>,
    /// Slot `MetaId::index()` holds that metavariable's current SORT refinement (a sort
    /// [`NodeId`] in the caller's [`SortOrder`] lattice), or `None` if the metavariable is
    /// sortless (the unsorted path, where this table stays empty). Order-sorted unification
    /// ([`unify_sorted`]) reads it to gate a binding and refines the representative's slot on
    /// a metavariable/metavariable union. It is EXACT only for the union-find representative;
    /// a bound metavariable's slot is stale-but-unread (its `sort_of` follows `resolve` first).
    meta_sort: Vec<Option<NodeId>>,
}

impl Subst {
    /// A fresh, empty substitution (every metavariable unbound).
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// Grow `bindings` so slot `idx` is addressable.
    #[inline]
    fn ensure(&mut self, idx: usize) {
        if self.bindings.len() <= idx {
            self.bindings.resize(idx + 1, None);
        }
    }

    /// Bind `m := node` (an `O(1)` union-find link). `node` should already be a resolved
    /// representative and must have passed the occurs-check.
    #[inline]
    fn bind(&mut self, m: MetaId, node: NodeId) {
        let idx = m.index();
        self.ensure(idx);
        self.bindings[idx] = Some(node);
    }

    /// The direct binding of `m`, if any (one union-find link, not a full resolution).
    #[inline]
    fn get(&self, m: MetaId) -> Option<NodeId> {
        self.bindings.get(m.index()).copied().flatten()
    }

    /// Bind `m := node` for a RENAMING substitution — the caller's entry point for the
    /// clause-variable freshening the structured backward resolver
    /// ([`crate::physical::resolve_fol`]) applies per firing. `node` must be a resolved
    /// representative that passes the occurs-check against `m`; for renaming, `node` is a
    /// FRESH (unbound) metavariable node, so the occurs-check is trivially satisfied. This is
    /// a thin public wrapper over the internal union-find link so a renaming can be
    /// materialized through [`apply`] without exposing the whole binding machinery.
    pub(crate) fn bind_renaming(&mut self, m: MetaId, node: NodeId) {
        self.bind(m, node);
    }

    /// Declare (or overwrite) metavariable `m`'s SORT — the caller's entry point for minting a
    /// sorted metavariable. A metavariable minted by [`TermDag::fresh_meta`] is sortless until
    /// declared here; leaving it undeclared keeps it on the unsorted path.
    pub(crate) fn declare_meta_sort(&mut self, m: MetaId, sort: NodeId) {
        self.set_meta_sort(m, Some(sort));
    }

    /// Set metavariable `m`'s sort slot (growing the table as needed). `None` clears it.
    #[inline]
    fn set_meta_sort(&mut self, m: MetaId, sort: Option<NodeId>) {
        let idx = m.index();
        if self.meta_sort.len() <= idx {
            self.meta_sort.resize(idx + 1, None);
        }
        self.meta_sort[idx] = sort;
    }

    /// Metavariable `m`'s current sort refinement, or `None` if it is sortless.
    #[inline]
    pub(crate) fn meta_sort(&self, m: MetaId) -> Option<NodeId> {
        self.meta_sort.get(m.index()).copied().flatten()
    }

    /// Whether `m` is bound in this substitution.
    #[inline]
    fn is_bound(&self, m: MetaId) -> bool {
        self.get(m).is_some()
    }

    /// The number of metavariables that currently have a binding.
    #[cfg(test)]
    pub(crate) fn bound_count(&self) -> usize {
        self.bindings.iter().filter(|b| b.is_some()).count()
    }

    /// Resolve `node` to its representative by walking metavariable bindings — the ONE
    /// identity-resolution primitive.
    ///
    /// Follows a bound [`NodeData::Meta`] to its binding, repeating until it reaches a
    /// non-metavariable node or an UNBOUND metavariable (the representative). It does NOT
    /// descend into structure (an `App`/`Binder` is its own representative); full
    /// expansion is [`apply`]. Terminates because the occurs-check forbids the cycles that
    /// would otherwise arise.
    pub(crate) fn resolve(&self, dag: &TermDag, node: NodeId) -> NodeId {
        let mut cur = node;
        loop {
            match dag.data(cur) {
                NodeData::Meta(m) => match self.get(*m) {
                    Some(next) => cur = next,
                    None => return cur,
                },
                _ => return cur,
            }
        }
    }
}

/// The outcome of a unification attempt.
///
/// A NEGATIVE result ([`Unified::Clash`]/[`Unified::Occurs`]) is a NORMAL answer — the two
/// terms have no unifier — never an engine error. On [`Unified::Ok`] the caller's
/// `&mut Subst` holds the most-general unifier accumulated in place (the substitution is
/// mutated as unification proceeds, so it is read from the caller's binding rather than
/// re-cloned into the `Ok` variant on every recursive step).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Unified {
    /// The terms unify; the accumulated most-general unifier is in the caller's `Subst`.
    Ok,
    /// A rigid/rigid mismatch: the two representatives cannot be equal under any
    /// substitution (distinct operators, arities, bound occurrences, leaves, or kinds).
    Clash {
        /// The left representative at the point of mismatch.
        left: NodeId,
        /// The right representative at the point of mismatch.
        right: NodeId,
    },
    /// The occurs-check fired: metavariable `meta` occurs (through the current
    /// substitution) in the term `in_node` it would be bound to, so no finite unifier
    /// exists.
    Occurs {
        /// The metavariable that would be bound.
        meta: MetaId,
        /// The term it would be bound to, in which it already occurs.
        in_node: NodeId,
    },
}

/// A caller-supplied partial order over sort [`NodeId`]s — the subsort lattice the
/// order-sorted unifier consults.
///
/// The order is SINGLE-SOURCED: the caller derives the covering edges from the reasoned
/// `rdfs:subClassOf` closure of the authored `math:` subsort tower (`math:NaturalNumber ⊑
/// Integer ⊑ RationalNumber ⊑ RealNumber ⊑ ComplexNumber`) and passes them to
/// [`Self::from_subclass_edges`]; nothing about the lattice is hardcoded here. [`Self::leq`]
/// is the reflexive-transitive subsort test and [`Self::meet`] the greatest-lower-bound the
/// metavariable/metavariable union rule needs.
#[derive(Debug, Default, Clone)]
pub(crate) struct SortOrder {
    /// `up[a]` is the reflexive-transitive upward closure of `a`: every sort `x` with `a ⊑ x`
    /// (including `a` itself). Membership is the [`Self::leq`] primitive.
    up: HashMap<NodeId, HashSet<NodeId>>,
    /// Every sort node named by a covering edge — the search space for [`Self::meet`].
    universe: HashSet<NodeId>,
}

impl SortOrder {
    /// Build the order from a set of covering `(sub, super)` subsort edges, computing the
    /// reflexive-transitive closure so [`Self::leq`] is a single set-membership test.
    pub(crate) fn from_subclass_edges(edges: &[(NodeId, NodeId)]) -> Self {
        let mut universe: HashSet<NodeId> = HashSet::new();
        let mut direct: HashMap<NodeId, Vec<NodeId>> = HashMap::new();
        for &(sub, sup) in edges {
            universe.insert(sub);
            universe.insert(sup);
            direct.entry(sub).or_default().push(sup);
        }
        // Reflexive-transitive upward closure per node (DFS over the covering edges).
        let mut up: HashMap<NodeId, HashSet<NodeId>> = HashMap::new();
        for &node in &universe {
            let mut reach: HashSet<NodeId> = HashSet::new();
            reach.insert(node);
            let mut stack = vec![node];
            while let Some(cur) = stack.pop() {
                if let Some(sups) = direct.get(&cur) {
                    for &sup in sups {
                        if reach.insert(sup) {
                            stack.push(sup);
                        }
                    }
                }
            }
            up.insert(node, reach);
        }
        Self { up, universe }
    }

    /// The subsort test `a ⊑ b` (reflexive): `a` is `b` or reaches `b` through the closure.
    pub(crate) fn leq(&self, a: NodeId, b: NodeId) -> bool {
        a == b || self.up.get(&a).is_some_and(|s| s.contains(&b))
    }

    /// The greatest lower bound `a ⊓ b`, or `None` if no common lower bound exists or it is
    /// not unique.
    ///
    /// A lower bound is a sort `c` with `c ⊑ a` and `c ⊑ b`; the meet is the UNIQUE common
    /// lower bound that is `⊒` every other common lower bound. For a chain `ℕ⊑ℤ⊑ℚ⊑ℝ⊑ℂ`,
    /// `meet(ℤ,ℝ)=ℤ`. Two incomparable maximal common lower bounds (a genuine non-lattice
    /// meet) return `None` rather than picking one — no silent degradation.
    pub(crate) fn meet(&self, a: NodeId, b: NodeId) -> Option<NodeId> {
        // Candidate lower bounds: the lattice universe, plus `a`/`b` themselves so a lone sort
        // (never named by an edge) still meets itself reflexively. De-duplicated into a set so
        // a candidate that is both in the universe and equal to `a`/`b` is not double-counted
        // (which would spuriously trip the uniqueness guard below).
        let mut candidates: HashSet<NodeId> = self.universe.clone();
        candidates.insert(a);
        candidates.insert(b);
        let common: Vec<NodeId> = candidates
            .into_iter()
            .filter(|&c| self.leq(c, a) && self.leq(c, b))
            .collect();
        // The meet is the common lower bound that dominates every common lower bound; if two
        // qualify (an incomparable pair), there is no unique GLB.
        let mut glb: Option<NodeId> = None;
        for &m in &common {
            if common.iter().all(|&c| self.leq(c, m)) {
                if glb.is_some() {
                    return None;
                }
                glb = Some(m);
            }
        }
        glb
    }
}

/// The order-sorted unification context: the subsort [`SortOrder`] plus the caller-supplied
/// sort tagging of rigid terms.
///
/// `term_sorts` maps a sort-tagged constant/literal [`NodeData::Leaf`]/[`NodeData::Free`]
/// node to its sort (the caller builds it from reasoned `rdf:type`); `op_result_sort` maps a
/// function-symbol operator node to the RESULT sort of an application headed by it (the rank
/// map). The mutable metavariable-sort refinement table lives in [`Subst`] (updated in place
/// through `&mut Subst`), so the context itself is shared (`&`).
#[derive(Debug, Default, Clone)]
pub(crate) struct SortContext {
    /// The subsort partial order.
    order: SortOrder,
    /// Sort of each sort-tagged rigid leaf/free constant.
    term_sorts: HashMap<NodeId, NodeId>,
    /// Result sort of an application headed by each function-symbol operator node.
    op_result_sort: HashMap<NodeId, NodeId>,
}

impl SortContext {
    /// Bundle a subsort order with the rigid-term sort tagging and the function rank map.
    pub(crate) fn new(
        order: SortOrder,
        term_sorts: HashMap<NodeId, NodeId>,
        op_result_sort: HashMap<NodeId, NodeId>,
    ) -> Self {
        Self {
            order,
            term_sorts,
            op_result_sort,
        }
    }

    /// The sort of `node` under substitution `s`, or `None` if it carries no sort obligation.
    ///
    /// Resolves `node` through `s` first (a bound metavariable takes its representative's
    /// sort), then: a metavariable → its current refinement; a tagged leaf/free constant → its
    /// `term_sorts` entry; an application → the `op_result_sort` of its operator; a bound
    /// occurrence or binder → `None` (untyped here).
    pub(crate) fn sort_of(&self, dag: &TermDag, node: NodeId, s: &Subst) -> Option<NodeId> {
        let node = s.resolve(dag, node);
        match dag.data(node) {
            NodeData::Meta(m) => s.meta_sort(*m),
            NodeData::Leaf(_) | NodeData::Free(_) => self.term_sorts.get(&node).copied(),
            NodeData::App { op, .. } => self.op_result_sort.get(op).copied(),
            NodeData::Bound { .. } | NodeData::Binder { .. } => None,
        }
    }
}

/// Unify `a` and `b` under substitution `s`, accumulating the most-general unifier into
/// `s` in place.
///
/// The algorithm (each step resolves through `s` first, so it operates on representatives):
///
/// 1. `a`, `b` ← `resolve`d. If they are the same `NodeId`, they are alpha-equal by
///    hash-consing → [`Unified::Ok`] with no work (the `O(1)` short-circuit).
/// 2. [`NodeData::Meta`] vs any term: occurs-check ([`occurs_through`]); on pass, bind.
/// 3. [`NodeData::App`]: operators unify, arities must match (else [`Unified::Clash`]),
///    arguments unify pairwise.
/// 4. [`NodeData::Binder`]: operators unify, sort-arity must match (else clash), sorts
///    unify pairwise, then bodies. Because the sorts are children, an ill-SORTED binder
///    pairing structurally clashes — sort EQUALITY is enforced for free, with NO
///    alpha-renaming (locally-nameless de-Bruijn).
/// 5. [`NodeData::Bound`]/[`NodeData::Leaf`]/[`NodeData::Free`]: rigid — equal iff the
///    representatives are the same node (already handled by step 1), else clash.
///
/// # Locally-nameless scope discipline
///
/// Metavariables live in the ambient (top-level, depth-0) context. As unification descends
/// through binders (rule 4 unifies bodies directly), the tracked binder `depth` records how
/// many binders enclose the current position. A metavariable occurrence at `depth` denotes
/// its ambient solution LIFTED by `depth` ([`whnf`]), so a solution is stored at depth 0:
/// binding `m := t` at `depth` records `t` lowered by `depth` ([`shift_down`]). A `t` that
/// mentions one of those `depth` local binders CANNOT be lowered — the bound variable would
/// escape the metavariable's scope, which has no first-order unifier — so it is a
/// [`Unified::Clash`] (the sound rejection a naive Robinson step would miss).
///
/// # Transactional (all-or-nothing) bindings
///
/// A multi-argument `App`/`Binder` unifies its children left-to-right and returns on the
/// FIRST clash/occurs failure, so an early argument can already have bound a metavariable
/// into `s` before a later argument fails — e.g. unifying `p(X,a)` against `p(b,c)` binds
/// `X := b` on argument 0 before argument 1 clashes. A failed unification must leave `s`
/// EXACTLY as it found it (the documented contract every caller relies on), so this entry
/// point snapshots `s` before descending and restores the snapshot on any non-`Ok` outcome —
/// the checkpoint/restore a partial bind through `unify_at` cannot itself undo.
pub(crate) fn unify(dag: &mut TermDag, a: NodeId, b: NodeId, s: &mut Subst) -> Unified {
    let checkpoint = s.clone();
    let outcome = unify_at(dag, a, b, s, 0, None);
    if outcome != Unified::Ok {
        *s = checkpoint;
    }
    outcome
}

/// ORDER-SORTED [`unify`]: identical structural algorithm, but a metavariable binding also
/// obeys the subsort lattice in `ctx`.
///
/// The only rule that changes is the metavariable step (rule 2). On `Meta(m:Sₘ)` against a
/// term `t`:
///
/// - `t` is `Meta(n:Sₙ)`: bind, refining BOTH metavariables' sort to `meet(Sₘ,Sₙ)`; a `None`
///   meet (no common lower bound) is a [`Unified::Clash`]. An unconstrained side takes the
///   other's sort.
/// - `t` is a non-metavariable of sort `Sₜ`: bind `m := t` iff `Sₜ ⊑ Sₘ`, else clash. An
///   unconstrained `Sₘ`, or an untyped `t` (no sort obligation), binds unconditionally.
///
/// The occurs-check and every structural rule (App/Binder/Bound/Leaf/Free) are UNCHANGED, so
/// passing an empty/sortless context makes `unify_sorted` behave exactly like [`unify`].
///
/// Transactional exactly like [`unify`]: `s` is snapshotted and restored on any non-`Ok`
/// outcome, so a partial bind from an earlier argument never survives a later clash.
pub(crate) fn unify_sorted(
    dag: &mut TermDag,
    a: NodeId,
    b: NodeId,
    s: &mut Subst,
    ctx: &SortContext,
) -> Unified {
    let checkpoint = s.clone();
    let outcome = unify_at(dag, a, b, s, 0, Some(ctx));
    if outcome != Unified::Ok {
        *s = checkpoint;
    }
    outcome
}

/// [`unify`]/[`unify_sorted`] at binder `depth` — the number of binders enclosing the current
/// position. `ctx` is `Some` on the order-sorted path (sorted metavariable binding) and `None`
/// on the plain unsorted path; the two share this one structural core.
fn unify_at(
    dag: &mut TermDag,
    a: NodeId,
    b: NodeId,
    s: &mut Subst,
    depth: u32,
    ctx: Option<&SortContext>,
) -> Unified {
    // Weak-head-normalize each side THROUGH the substitution at this depth: a bound
    // metavariable's ambient solution is lifted by `depth` to the current scope.
    let a = whnf(dag, s, a, depth);
    let b = whnf(dag, s, b, depth);
    if a == b {
        return Unified::Ok;
    }
    // Clone the two representatives' data to release the borrow on `dag` before the
    // recursive `unify_at` calls need `&mut dag`.
    let da = dag.data(a).clone();
    let db = dag.data(b).clone();
    match (da, db) {
        // A metavariable against anything: occurs-check, sort-check (order-sorted path),
        // scope-lower, then bind.
        (NodeData::Meta(m), _) => bind_meta(dag, s, m, a, b, depth, ctx),
        (_, NodeData::Meta(m)) => bind_meta(dag, s, m, b, a, depth, ctx),
        // Application: operator, arity, then arguments pairwise (all at the same depth).
        (
            NodeData::App {
                op: o1,
                args: args1,
            },
            NodeData::App {
                op: o2,
                args: args2,
            },
        ) => {
            if args1.len() != args2.len() {
                return Unified::Clash { left: a, right: b };
            }
            match unify_at(dag, o1, o2, s, depth, ctx) {
                Unified::Ok => {}
                other => return other,
            }
            for (&x, &y) in args1.iter().zip(args2.iter()) {
                match unify_at(dag, x, y, s, depth, ctx) {
                    Unified::Ok => {}
                    other => return other,
                }
            }
            Unified::Ok
        }
        // Binder: operator, sort-arity, sorts pairwise (sort EQUALITY), then body one binder
        // deeper. No alpha-renaming — the bodies are compared as-is under their shared
        // de-Bruijn frame, so the body unifies at `depth + 1`.
        (
            NodeData::Binder {
                op: o1,
                sorts: s1,
                body: b1,
            },
            NodeData::Binder {
                op: o2,
                sorts: s2,
                body: b2,
            },
        ) => {
            if s1.len() != s2.len() {
                return Unified::Clash { left: a, right: b };
            }
            match unify_at(dag, o1, o2, s, depth, ctx) {
                Unified::Ok => {}
                other => return other,
            }
            for (&x, &y) in s1.iter().zip(s2.iter()) {
                match unify_at(dag, x, y, s, depth, ctx) {
                    Unified::Ok => {}
                    other => return other,
                }
            }
            unify_at(dag, b1, b2, s, depth + 1, ctx)
        }
        // Any remaining pairing of rigid representatives is a clash (a == b was handled by
        // the short-circuit, so two identical leaves/frees/bounds never reach here).
        _ => Unified::Clash { left: a, right: b },
    }
}

/// Weak-head-normalize `node` at binder `depth`: unfold top-level metavariable bindings,
/// lifting each unfolded ambient solution by `depth` to the current scope.
///
/// This layers the de-Bruijn lift over the single resolution primitive
/// ([`Subst::resolve`]): a rigid node (or an unbound metavariable) is its own
/// representative and needs no lift, while unfolding a bound metavariable — whose solution
/// is stored at the ambient depth 0 — [`shift`]s it up by `depth` so its free de-Bruijn
/// indices still refer to the same ambient binders from the current position.
fn whnf(dag: &mut TermDag, s: &Subst, node: NodeId, depth: u32) -> NodeId {
    let mut cur = node;
    loop {
        match dag.data(cur).clone() {
            NodeData::Meta(m) => match s.get(m) {
                Some(sol) => cur = shift(dag, sol, depth),
                None => return cur,
            },
            _ => return cur,
        }
    }
}

/// Occurs-check `m` against `t`, scope-lower `t` to the ambient depth, then bind `m := t`.
///
/// `t` is the already-[`whnf`]'d representative at `depth`; `meta_node` is the `Meta(m)`
/// node it is unified with. If `m` occurs in `t` THROUGH the substitution, no finite
/// unifier exists → [`Unified::Occurs`]. Otherwise `t` is lowered by `depth` to the ambient
/// scope where solutions live ([`shift_down`]); if a local bound variable escapes (a free
/// de-Bruijn index below `depth`), `t` cannot be a solution for the ambient `m`, so there is
/// no first-order unifier → [`Unified::Clash`]. The union-find link then stores the lowered,
/// ambient solution.
fn bind_meta(
    dag: &mut TermDag,
    s: &mut Subst,
    m: MetaId,
    meta_node: NodeId,
    t: NodeId,
    depth: u32,
    ctx: Option<&SortContext>,
) -> Unified {
    if occurs_through(s, dag, m, t) {
        return Unified::Occurs {
            meta: m,
            in_node: t,
        };
    }
    // Order-sorted admissibility (only on the sorted path). On success, `representative_sort`
    // carries the metavariable/metavariable refined sort to install on the representative once
    // the bind lands.
    let mut representative_sort: Option<(MetaId, Option<NodeId>)> = None;
    if let Some(ctx) = ctx {
        let s_m = s.meta_sort(m);
        match dag.data(t).clone() {
            // Two metavariables: the representative (`t == n`) carries `meet(Sₘ,Sₙ)`; a
            // `None` meet (no common lower bound) is a sort clash.
            NodeData::Meta(n) => {
                let s_n = s.meta_sort(n);
                let refined = match (s_m, s_n) {
                    (None, other) | (other, None) => other,
                    (Some(x), Some(y)) => match ctx.order.meet(x, y) {
                        Some(glb) => Some(glb),
                        None => {
                            return Unified::Clash {
                                left: meta_node,
                                right: t,
                            };
                        }
                    },
                };
                representative_sort = Some((n, refined));
            }
            // A rigid term: it may bind `m` only if its sort is a subsort of `m`'s. An
            // unconstrained `Sₘ`, or an untyped `t`, imposes no obligation.
            _ => {
                if let (Some(sm), Some(st)) = (s_m, ctx.sort_of(dag, t, s))
                    && !ctx.order.leq(st, sm)
                {
                    return Unified::Clash {
                        left: meta_node,
                        right: t,
                    };
                }
            }
        }
    }
    match shift_down(dag, t, depth) {
        Some(solution) => {
            s.bind(m, solution);
            // Install the refined sort on the surviving representative (the metavariable/
            // metavariable union case); `solution == t == Meta(n)` here, so `n` stays the
            // unbound representative whose sort the next binding will consult.
            if let Some((rep, sort)) = representative_sort {
                s.set_meta_sort(rep, sort);
            }
            Unified::Ok
        }
        // `t` references a binder local to the unification descent that ambient `m` cannot
        // see: the bound variable would escape, so there is no first-order unifier.
        None => Unified::Clash {
            left: meta_node,
            right: t,
        },
    }
}

/// Whether metavariable `m` occurs in `node` MODULO the current substitution `s`.
///
/// Sound occurs-check for a triangular (union-find) substitution: the raw free-metavariable
/// cache is only exact for the RESOLVED term, so a candidate whose free metavariables are
/// all unbound is decided in `O(1)` by the cache (the fast path), while a candidate that
/// mentions an already-bound metavariable is walked structurally, resolving each child, so
/// a cycle reintroduced through the substitution is detected. Without this, binding
/// `m := f(n)` while `n := g(m)` would forge the infinite term `m = f(g(m))` — the exact
/// unsoundness the occurs-check exists to forbid.
fn occurs_through(s: &Subst, dag: &TermDag, m: MetaId, node: NodeId) -> bool {
    let node = s.resolve(dag, node);
    let fm = dag.free_meta(node);
    // Fast path: if no free metavariable of `node` is bound, the cached set is exact for
    // the fully-resolved term, so membership of `m` is the whole answer.
    if fm.iter().all(|v| !s.is_bound(v)) {
        return fm.contains(m);
    }
    // Slow path: a free metavariable is bound and could reintroduce `m`, so descend,
    // resolving each child in turn.
    match dag.data(node) {
        NodeData::Meta(other) => *other == m,
        NodeData::Leaf(_) | NodeData::Free(_) | NodeData::Bound { .. } => false,
        NodeData::App { op, args } => {
            occurs_through(s, dag, m, *op) || args.iter().any(|&a| occurs_through(s, dag, m, a))
        }
        NodeData::Binder { op, sorts, body } => {
            occurs_through(s, dag, m, *op)
                || sorts.iter().any(|&x| occurs_through(s, dag, m, x))
                || occurs_through(s, dag, m, *body)
        }
    }
}

/// Materialize `s` over `node`: re-intern `node` with every resolved metavariable replaced
/// by its binding, capture-avoiding by construction.
///
/// A metavariable bound to `t` and spliced UNDER `k` binders has `t`'s free de-Bruijn
/// indices lifted by `k` via [`shift`] — the sole capture-avoidance step. A subterm with no
/// substituted metavariable re-interns to its own `NodeId` (hash-consing), so `apply` over
/// a ground term is the identity. Every interned node's free-metavariable cache is exact by
/// construction (it flows through `TermDag::intern_*`).
pub(crate) fn apply(dag: &mut TermDag, s: &Subst, node: NodeId) -> NodeId {
    let mut memo: HashMap<(NodeId, u32), NodeId> = HashMap::new();
    apply_rec(dag, s, node, 0, &mut memo)
}

/// `apply` under `depth` enclosing binders, memoized on `(node, depth)`.
fn apply_rec(
    dag: &mut TermDag,
    s: &Subst,
    node: NodeId,
    depth: u32,
    memo: &mut HashMap<(NodeId, u32), NodeId>,
) -> NodeId {
    if let Some(&hit) = memo.get(&(node, depth)) {
        return hit;
    }
    let result = match dag.data(node).clone() {
        NodeData::Meta(m) => match s.get(m) {
            // A metavariable's solution lives in its own (depth-0) scope; fully apply it
            // there, then lift its free de-Bruijn indices by the current binder depth so it
            // still refers to the same binders after being spliced in.
            Some(binding) => {
                let applied = apply_rec(dag, s, binding, 0, memo);
                shift(dag, applied, depth)
            }
            None => node,
        },
        // A rigid node with no substitution point re-interns to itself; a pre-existing
        // bound occurrence already refers correctly within the term and is NOT shifted here
        // (only a spliced metavariable solution crosses binder scopes).
        NodeData::Leaf(_) | NodeData::Free(_) | NodeData::Bound { .. } => node,
        NodeData::App { op, args } => {
            let op = apply_rec(dag, s, op, depth, memo);
            let args: Vec<NodeId> = args
                .iter()
                .map(|&a| apply_rec(dag, s, a, depth, memo))
                .collect();
            dag.intern_app(op, args)
        }
        NodeData::Binder { op, sorts, body } => {
            let op = apply_rec(dag, s, op, depth, memo);
            let sorts: Vec<NodeId> = sorts
                .iter()
                .map(|&x| apply_rec(dag, s, x, depth, memo))
                .collect();
            let body = apply_rec(dag, s, body, depth + 1, memo);
            dag.intern_binder(op, sorts, body)
        }
    };
    memo.insert((node, depth), result);
    result
}

/// Lift every FREE de-Bruijn index in `node` by `by` — the capture-avoidance primitive.
///
/// "Free" is relative to `node`'s own root: an occurrence at distance `d` is free when `d`
/// is at least the number of binders enclosing it within `node`, and only free occurrences
/// are lifted (a locally-bound occurrence keeps its distance). `by == 0` is the identity.
/// Memoized on `(node, cutoff)` within the call so shared subterms shift once.
pub(crate) fn shift(dag: &mut TermDag, node: NodeId, by: u32) -> NodeId {
    if by == 0 {
        return node;
    }
    let mut memo: HashMap<(NodeId, u32), NodeId> = HashMap::new();
    shift_rec(dag, node, by, 0, &mut memo)
}

/// `shift` with an explicit `cutoff` (the binder depth traversed so far), memoized on
/// `(node, cutoff)`.
fn shift_rec(
    dag: &mut TermDag,
    node: NodeId,
    by: u32,
    cutoff: u32,
    memo: &mut HashMap<(NodeId, u32), NodeId>,
) -> NodeId {
    if let Some(&hit) = memo.get(&(node, cutoff)) {
        return hit;
    }
    let result = match dag.data(node).clone() {
        NodeData::Bound { debruijn, slot } => {
            if debruijn >= cutoff {
                let lifted = debruijn.checked_add(by).expect(
                    "de-Bruijn distance overflow during shift: a free occurrence lifted past \
                     u32::MAX would rebind to the wrong binder (variable-capture bug)",
                );
                dag.intern_bound(lifted, slot)
            } else {
                node
            }
        }
        NodeData::Leaf(_) | NodeData::Free(_) | NodeData::Meta(_) => node,
        NodeData::App { op, args } => {
            let op = shift_rec(dag, op, by, cutoff, memo);
            let args: Vec<NodeId> = args
                .iter()
                .map(|&a| shift_rec(dag, a, by, cutoff, memo))
                .collect();
            dag.intern_app(op, args)
        }
        NodeData::Binder { op, sorts, body } => {
            let op = shift_rec(dag, op, by, cutoff, memo);
            let sorts: Vec<NodeId> = sorts
                .iter()
                .map(|&x| shift_rec(dag, x, by, cutoff, memo))
                .collect();
            // The body sees one more enclosing binder, so its cutoff rises by one.
            let body = shift_rec(dag, body, by, cutoff + 1, memo);
            dag.intern_binder(op, sorts, body)
        }
    };
    memo.insert((node, cutoff), result);
    result
}

/// Lower every FREE de-Bruijn index in `node` by `by`, or `None` if a local binder escapes.
///
/// The inverse of [`shift`], used to bring a metavariable's solution — captured at binder
/// `depth` during unification — back to the ambient depth-0 scope where solutions are
/// stored. A free occurrence at distance `d` becomes `d - by`; if `d - by` would fall below
/// the current cutoff (i.e. the occurrence refers to one of the `by` binders being removed),
/// the bound variable would escape and the lowering is undefined → `None`. `by == 0` is the
/// identity.
fn shift_down(dag: &mut TermDag, node: NodeId, by: u32) -> Option<NodeId> {
    if by == 0 {
        return Some(node);
    }
    let mut memo: HashMap<(NodeId, u32), NodeId> = HashMap::new();
    shift_down_rec(dag, node, by, 0, &mut memo)
}

/// [`shift_down`] with an explicit `cutoff`, memoized on `(node, cutoff)`. Only successful
/// (`Some`) subterms are memoized; an escape short-circuits `None` up the recursion.
fn shift_down_rec(
    dag: &mut TermDag,
    node: NodeId,
    by: u32,
    cutoff: u32,
    memo: &mut HashMap<(NodeId, u32), NodeId>,
) -> Option<NodeId> {
    if let Some(&hit) = memo.get(&(node, cutoff)) {
        return Some(hit);
    }
    let result = match dag.data(node).clone() {
        NodeData::Bound { debruijn, slot } => {
            if debruijn >= cutoff {
                // Free occurrence: it must remain at or above the cutoff after lowering,
                // else it referenced one of the removed binders and escapes.
                match debruijn.checked_sub(by) {
                    Some(lowered) if lowered >= cutoff => dag.intern_bound(lowered, slot),
                    _ => return None,
                }
            } else {
                node
            }
        }
        NodeData::Leaf(_) | NodeData::Free(_) | NodeData::Meta(_) => node,
        NodeData::App { op, args } => {
            let op = shift_down_rec(dag, op, by, cutoff, memo)?;
            let mut lowered = Vec::with_capacity(args.len());
            for &a in args.iter() {
                lowered.push(shift_down_rec(dag, a, by, cutoff, memo)?);
            }
            dag.intern_app(op, lowered)
        }
        NodeData::Binder { op, sorts, body } => {
            let op = shift_down_rec(dag, op, by, cutoff, memo)?;
            let mut lowered_sorts = Vec::with_capacity(sorts.len());
            for &x in sorts.iter() {
                lowered_sorts.push(shift_down_rec(dag, x, by, cutoff, memo)?);
            }
            let body = shift_down_rec(dag, body, by, cutoff + 1, memo)?;
            dag.intern_binder(op, lowered_sorts, body)
        }
    };
    memo.insert((node, cutoff), result);
    Some(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::collections::BTreeSet;

    use proptest::prelude::*;
    use purrdf::TermValue;

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

    fn iri(dag: &mut TermDag, s: &str) -> NodeId {
        dag.intern_leaf(TermValue::iri(s))
    }

    /// A from-scratch (cache-independent) recomputation of a node's free-metavariable set,
    /// used to prove the DAG's cache stays exact after `apply`/`shift`.
    fn recompute_fm(dag: &TermDag, node: NodeId, out: &mut BTreeSet<MetaId>) {
        match dag.data(node) {
            NodeData::Meta(m) => {
                out.insert(*m);
            }
            NodeData::Leaf(_) | NodeData::Free(_) | NodeData::Bound { .. } => {}
            NodeData::App { op, args } => {
                recompute_fm(dag, *op, out);
                for &a in args.iter() {
                    recompute_fm(dag, a, out);
                }
            }
            NodeData::Binder { op, sorts, body } => {
                recompute_fm(dag, *op, out);
                for &x in sorts.iter() {
                    recompute_fm(dag, x, out);
                }
                recompute_fm(dag, *body, out);
            }
        }
    }

    /// Assert the cache equals the from-scratch recomputation for `node` and every node
    /// reachable from it.
    fn assert_fm_exact(dag: &TermDag, node: NodeId, seen: &mut BTreeSet<NodeId>) {
        if !seen.insert(node) {
            return;
        }
        let mut expected = BTreeSet::new();
        recompute_fm(dag, node, &mut expected);
        let cached: BTreeSet<MetaId> = dag.free_meta(node).iter().collect();
        assert_eq!(
            cached, expected,
            "free_meta cache for {node:?} drifted from a from-scratch recomputation"
        );
        match dag.data(node).clone() {
            NodeData::Leaf(_) | NodeData::Free(_) | NodeData::Bound { .. } | NodeData::Meta(_) => {}
            NodeData::App { op, args } => {
                assert_fm_exact(dag, op, seen);
                for a in args.iter() {
                    assert_fm_exact(dag, *a, seen);
                }
            }
            NodeData::Binder { op, sorts, body } => {
                assert_fm_exact(dag, op, seen);
                for x in sorts.iter() {
                    assert_fm_exact(dag, *x, seen);
                }
                assert_fm_exact(dag, body, seen);
            }
        }
    }

    // ── Test 1: occurs-check ────────────────────────────────────────────────────────────

    #[test]
    fn occurs_check_rejects_cyclic_and_accepts_well_founded() {
        // X = f(X) has no finite unifier: the occurs-check must fire.
        let mut dag = TermDag::new();
        let f = iri(&mut dag, "https://example.org/f");
        let (x, x_node) = dag.fresh_meta();
        let f_of_x = dag.intern_app(f, vec![x_node]);
        let mut s = Subst::new();
        let outcome = unify(&mut dag, x_node, f_of_x, &mut s);
        assert!(
            matches!(outcome, Unified::Occurs { meta, .. } if meta == x),
            "X = f(X) must fail the occurs-check, got {outcome:?}"
        );
        assert_eq!(s.bound_count(), 0, "a failed unification binds nothing");

        // X = f(a) is well-founded: it succeeds and binds X := f(a).
        let a = iri(&mut dag, "https://example.org/a");
        let f_of_a = dag.intern_app(f, vec![a]);
        let mut s = Subst::new();
        let outcome = unify(&mut dag, x_node, f_of_a, &mut s);
        assert_eq!(outcome, Unified::Ok, "X = f(a) must unify");
        assert_eq!(s.resolve(&dag, x_node), f_of_a, "X resolves to f(a)");
        assert_eq!(
            apply(&mut dag, &s, x_node),
            f_of_a,
            "apply expands X to f(a)"
        );
    }

    #[test]
    fn occurs_check_is_sound_through_substitution() {
        // p(N, M) vs p(g(M), f(N)) demands N = g(M) and M = f(N), i.e. M = f(g(M)) — no
        // finite unifier. The occurs-check must fire through the substitution, never forge
        // the cyclic term.
        let mut dag = TermDag::new();
        let p = iri(&mut dag, "https://example.org/p");
        let f = iri(&mut dag, "https://example.org/f");
        let g = iri(&mut dag, "https://example.org/g");
        let (_n, n_node) = dag.fresh_meta();
        let (_m, m_node) = dag.fresh_meta();
        let g_of_m = dag.intern_app(g, vec![m_node]);
        let f_of_n = dag.intern_app(f, vec![n_node]);
        let left = dag.intern_app(p, vec![n_node, m_node]);
        let right = dag.intern_app(p, vec![g_of_m, f_of_n]);
        let mut s = Subst::new();
        let outcome = unify(&mut dag, left, right, &mut s);
        assert!(
            matches!(outcome, Unified::Occurs { .. }),
            "the cyclic demand must be rejected by the occurs-check, got {outcome:?}"
        );
    }

    // ── Test 1c: a clash AFTER an argument already bound leaves NO bindings ─────────────

    #[test]
    fn failed_unification_after_partial_bind_leaves_no_bindings() {
        // G11 regression: unifying p(X,a) against p(b,c) binds X := b while unifying
        // argument 0, THEN clashes on argument 1 (a vs c). The module's documented
        // contract is "a failed unification binds nothing" — `unify` must roll back the
        // argument-0 bind, not merely report the clash while leaving X bound.
        let mut dag = TermDag::new();
        let p = iri(&mut dag, "https://example.org/p");
        let a = iri(&mut dag, "https://example.org/a");
        let b = iri(&mut dag, "https://example.org/b");
        let c = iri(&mut dag, "https://example.org/c");
        let (_x, x_node) = dag.fresh_meta();

        let left = dag.intern_app(p, vec![x_node, a]);
        let right = dag.intern_app(p, vec![b, c]);

        let mut s = Subst::new();
        let outcome = unify(&mut dag, left, right, &mut s);
        assert!(
            matches!(outcome, Unified::Clash { .. }),
            "p(X,a) vs p(b,c) must clash on argument 1 (a vs c), got {outcome:?}"
        );
        assert_eq!(
            s.bound_count(),
            0,
            "a failed unification must bind NOTHING, even though X was bound to b \
             while unifying argument 0 before the argument-1 clash"
        );
        assert_eq!(
            s.resolve(&dag, x_node),
            x_node,
            "X must remain its own (unbound) representative after the rollback"
        );
    }

    // ── Test 2: substitution round-trip / no capture ────────────────────────────────────

    #[test]
    fn apply_is_identity_on_ground_terms() {
        let mut dag = TermDag::new();
        let f = iri(&mut dag, "https://example.org/f");
        let a = iri(&mut dag, "https://example.org/a");
        let ground = dag.intern_app(f, vec![a]);
        let s = Subst::new();
        assert_eq!(
            apply(&mut dag, &s, ground),
            ground,
            "apply over a ground term with an empty substitution is the identity"
        );
    }

    #[test]
    fn shift_lifts_only_free_occurrences() {
        // Inside `binder[sort]. f(bound{0,0}, bound{1,0})`, `bound{0,0}` is LOCAL to the
        // binder and `bound{1,0}` is FREE (it refers one binder further out). Shifting the
        // whole binder by 2 must lift only the free one: 1 → 3, leaving the local 0 alone.
        let mut dag = TermDag::new();
        let f = iri(&mut dag, "https://example.org/f");
        let sort = iri(&mut dag, "https://example.org/sort");
        let local = dag.intern_bound(0, 0);
        let free = dag.intern_bound(1, 0);
        let body = dag.intern_app(f, vec![local, free]);
        let binder = dag.intern_binder(f, vec![sort], body);

        let shifted = shift(&mut dag, binder, 2);

        let expected_local = dag.intern_bound(0, 0);
        let expected_free = dag.intern_bound(3, 0);
        let expected_body = dag.intern_app(f, vec![expected_local, expected_free]);
        let expected = dag.intern_binder(f, vec![sort], expected_body);
        assert_eq!(
            shifted, expected,
            "shift by 2 lifts only the free occurrence (1 → 3), not the local (0)"
        );
    }

    #[test]
    fn apply_under_binders_avoids_capture() {
        // X is bound to a FREE de-Bruijn occurrence `bound{0,0}` (it points at whatever
        // binder encloses X's home scope). Splice X under one binder: `binder[sort]. g(X)`.
        // If apply naively kept `bound{0,0}`, it would be CAPTURED by the intervening
        // binder; the shift must push it out to `bound{1,0}`.
        let mut dag = TermDag::new();
        let g = iri(&mut dag, "https://example.org/g");
        let sort = iri(&mut dag, "https://example.org/sort");
        let (x, x_node) = dag.fresh_meta();
        let free_occ = dag.intern_bound(0, 0);
        let mut s = Subst::new();
        s.bind(x, free_occ);

        let g_of_x = dag.intern_app(g, vec![x_node]);
        let term = dag.intern_binder(g, vec![sort], g_of_x);
        let applied = apply(&mut dag, &s, term);

        let pushed = dag.intern_bound(1, 0);
        let g_of_pushed = dag.intern_app(g, vec![pushed]);
        let expected = dag.intern_binder(g, vec![sort], g_of_pushed);
        assert_eq!(
            applied, expected,
            "the free occurrence in X must be shifted past the intervening binder (0 → 1), \
             never captured"
        );

        // Round-trip: at binder depth 0 (no intervening binder), X keeps its occurrence.
        let g_top = dag.intern_app(g, vec![x_node]);
        let applied_top = apply(&mut dag, &s, g_top);
        let expected_top = dag.intern_app(g, vec![free_occ]);
        assert_eq!(
            applied_top, expected_top,
            "at depth 0 the occurrence is spliced unshifted (open/close round-trip)"
        );
    }

    // ── Test 3: free_meta exactness after apply/shift (proptest) ────────────────────────

    /// A tiny term specification over a fixed metavariable pool, for the property tests.
    #[derive(Clone, Debug)]
    enum TSpec {
        /// A constant leaf `c{i}`.
        Const(u8),
        /// A metavariable, by index into the pool.
        Meta(u8),
        /// A bound occurrence at de-Bruijn `(debruijn, slot)`.
        Bound(u32, u16),
        /// A unary application `f(child)` (operator `f`).
        F1(Box<TSpec>),
        /// A binary application `g(left, right)` (operator `g`).
        G2(Box<TSpec>, Box<TSpec>),
        /// A single-sorted binder `q[sort]. body`.
        Q(Box<TSpec>),
    }

    /// Two metavariables suffice to exhibit sharing, cycles, and generality.
    const POOL: usize = 2;

    fn build(dag: &mut TermDag, pool: &[NodeId], spec: &TSpec) -> NodeId {
        match spec {
            TSpec::Const(i) => iri(dag, &format!("https://example.org/c{i}")),
            TSpec::Meta(i) => pool[*i as usize % POOL],
            TSpec::Bound(d, slot) => dag.intern_bound(*d, *slot),
            TSpec::F1(c) => {
                let f = iri(dag, "https://example.org/f");
                let c = build(dag, pool, c);
                dag.intern_app(f, vec![c])
            }
            TSpec::G2(l, r) => {
                let g = iri(dag, "https://example.org/g");
                let l = build(dag, pool, l);
                let r = build(dag, pool, r);
                dag.intern_app(g, vec![l, r])
            }
            TSpec::Q(body) => {
                let q = iri(dag, "https://example.org/q");
                let sort = iri(dag, "https://example.org/sort");
                let body = build(dag, pool, body);
                dag.intern_binder(q, vec![sort], body)
            }
        }
    }

    fn arb_spec() -> impl Strategy<Value = TSpec> {
        let leaf = prop_oneof![
            (0u8..3).prop_map(TSpec::Const),
            (0u8..POOL as u8).prop_map(TSpec::Meta),
            (0u32..3, 0u16..2).prop_map(|(d, s)| TSpec::Bound(d, s)),
        ];
        leaf.prop_recursive(4, 24, 2, |inner| {
            prop_oneof![
                inner.clone().prop_map(|c| TSpec::F1(Box::new(c))),
                (inner.clone(), inner.clone())
                    .prop_map(|(l, r)| TSpec::G2(Box::new(l), Box::new(r))),
                inner.prop_map(|b| TSpec::Q(Box::new(b))),
            ]
        })
    }

    /// Mint a fresh pool of `POOL` metavariables in `dag`.
    fn fresh_pool(dag: &mut TermDag) -> (Vec<MetaId>, Vec<NodeId>) {
        let mut metas = Vec::with_capacity(POOL);
        let mut nodes = Vec::with_capacity(POOL);
        for _ in 0..POOL {
            let (m, n) = dag.fresh_meta();
            metas.push(m);
            nodes.push(n);
        }
        (metas, nodes)
    }

    proptest! {
        #![proptest_config(config())]

        /// After ANY `apply`/`shift`, every newly-interned node's cached free-metavariable
        /// set equals a from-scratch recomputation — the cache is never stale.
        #[test]
        fn free_meta_exact_after_apply_and_shift(
            term in arb_spec(),
            binding0 in arb_spec(),
            by in 0u32..4,
        ) {
            let mut dag = TermDag::new();
            let (metas, nodes) = fresh_pool(&mut dag);
            let term_node = build(&mut dag, &nodes, &term);

            // A substitution binding the first pool metavariable to some term (occurs-check
            // permitting — an occurs failure just leaves it unbound, still a valid input).
            let bind_node = build(&mut dag, &nodes, &binding0);
            let mut s = Subst::new();
            if !occurs_through(&s, &dag, metas[0], bind_node) {
                s.bind(metas[0], bind_node);
            }

            let applied = apply(&mut dag, &s, term_node);
            let mut seen = BTreeSet::new();
            assert_fm_exact(&dag, applied, &mut seen);

            let shifted = shift(&mut dag, term_node, by);
            assert_fm_exact(&dag, shifted, &mut seen);
        }
    }

    // ── Test 4: soundness + MGU generality (proptest, brute-forced on tiny terms) ───────

    /// The small ground universe every metavariable is exhaustively instantiated to, for the
    /// brute-force unifier enumeration. Rich enough that any unifiable tiny-term pair has a
    /// ground unifier in the enumerated set (so the completeness biconditional holds).
    fn ground_universe(dag: &mut TermDag) -> Vec<NodeId> {
        let c0 = iri(dag, "https://example.org/c0");
        let c1 = iri(dag, "https://example.org/c1");
        let c2 = iri(dag, "https://example.org/c2");
        let f = iri(dag, "https://example.org/f");
        let g = iri(dag, "https://example.org/g");
        let f_c0 = dag.intern_app(f, vec![c0]);
        let f_c1 = dag.intern_app(f, vec![c1]);
        let g_c0_c1 = dag.intern_app(g, vec![c0, c1]);
        vec![c0, c1, c2, f_c0, f_c1, g_c0_c1]
    }

    /// Build the substitution assigning each pool metavariable a ground term per `assign`.
    fn ground_subst(metas: &[MetaId], universe: &[NodeId], assign: &[usize]) -> Subst {
        let mut s = Subst::new();
        for (i, &g) in assign.iter().enumerate() {
            s.bind(metas[i], universe[g]);
        }
        s
    }

    /// Enumerate every assignment of the `POOL` metavariables to `universe`.
    fn all_assignments(universe_len: usize) -> Vec<Vec<usize>> {
        let mut out = vec![Vec::new()];
        for _ in 0..POOL {
            let mut next = Vec::new();
            for prefix in &out {
                for g in 0..universe_len {
                    let mut extended = prefix.clone();
                    extended.push(g);
                    next.push(extended);
                }
            }
            out = next;
        }
        out
    }

    proptest! {
        #![proptest_config(config())]

        /// Soundness: if `unify` returns `Ok`, the substitution IS a unifier
        /// (`apply(s,a) == apply(s,b)`). Completeness: `unify` returns `Ok` exactly when a
        /// ground unifier exists in the enumerated universe. Generality: every ground
        /// unifier factors through the produced substitution (it can be reached by further
        /// instantiating the residual metavariables).
        #[test]
        fn unify_is_sound_complete_and_most_general(a in arb_spec(), b in arb_spec()) {
            let mut dag = TermDag::new();
            let universe = ground_universe(&mut dag);
            let (metas, nodes) = fresh_pool(&mut dag);
            let a_node = build(&mut dag, &nodes, &a);
            let b_node = build(&mut dag, &nodes, &b);

            let mut s = Subst::new();
            let outcome = unify(&mut dag, a_node, b_node, &mut s);

            // Independently enumerate every ground unifier over the universe.
            let assignments = all_assignments(universe.len());
            let mut ground_unifiers: Vec<Vec<usize>> = Vec::new();
            for assign in &assignments {
                let u = ground_subst(&metas, &universe, assign);
                let ua = apply(&mut dag, &u, a_node);
                let ub = apply(&mut dag, &u, b_node);
                if ua == ub {
                    ground_unifiers.push(assign.clone());
                }
            }

            match outcome {
                Unified::Ok => {
                    // Soundness.
                    let sa = apply(&mut dag, &s, a_node);
                    let sb = apply(&mut dag, &s, b_node);
                    prop_assert_eq!(sa, sb, "unify Ok but apply(s,a) != apply(s,b)");

                    // (No "ground unifier must exist" assertion: the MGU can be more general
                    // than any instance in the finite universe — e.g. X = f(c2) when f(c2) is
                    // outside it — so an empty ground set here is not a contradiction.)

                    // Generality: every ground unifier factors through s — instantiating the
                    // residual metavariables of apply(s, ·) reproduces it.
                    for assign in &ground_unifiers {
                        let u = ground_subst(&metas, &universe, assign);
                        // Solve for the residual r by unifying apply(s, meta) against
                        // u(meta) for each pool metavariable; it must succeed and reproduce u.
                        let mut r = Subst::new();
                        let mut factors = true;
                        for &mnode in &nodes {
                            let s_img = apply(&mut dag, &s, mnode);
                            let u_img = apply(&mut dag, &u, mnode);
                            if unify(&mut dag, s_img, u_img, &mut r) != Unified::Ok {
                                factors = false;
                                break;
                            }
                        }
                        prop_assert!(
                            factors,
                            "ground unifier {:?} does not factor through the MGU",
                            assign
                        );
                        for &mnode in &nodes {
                            let composed = {
                                let s_img = apply(&mut dag, &s, mnode);
                                apply(&mut dag, &r, s_img)
                            };
                            let direct = apply(&mut dag, &u, mnode);
                            prop_assert_eq!(
                                composed, direct,
                                "r ∘ s disagrees with the ground unifier on a metavariable"
                            );
                        }
                    }
                }
                Unified::Clash { .. } | Unified::Occurs { .. } => {
                    // Completeness: a non-Ok verdict means NO ground unifier may exist.
                    prop_assert!(
                        ground_unifiers.is_empty(),
                        "unify reported {:?} but a ground unifier exists: {:?}",
                        outcome, ground_unifiers
                    );
                }
            }
        }
    }

    // ── Test 5: binder structural unification ───────────────────────────────────────────

    #[test]
    fn alpha_equivalent_binders_unify_trivially() {
        // Two independently-built `∀[sort]. p(bound{0,0})` binders are the SAME NodeId
        // (hash-consing over locally-nameless de-Bruijn), so they unify with the empty
        // substitution — the O(1) short-circuit, no alpha-renaming.
        let mut dag = TermDag::new();
        let build_forall = |dag: &mut TermDag| {
            let forall = iri(dag, "https://example.org/forall");
            let sort = iri(dag, "https://example.org/sort");
            let p = iri(dag, "https://example.org/p");
            let bound = dag.intern_bound(0, 0);
            let body = dag.intern_app(p, vec![bound]);
            dag.intern_binder(forall, vec![sort], body)
        };
        let left = build_forall(&mut dag);
        let right = build_forall(&mut dag);
        assert_eq!(
            left, right,
            "alpha-equivalent binders hash-cons to one node"
        );
        let mut s = Subst::new();
        assert_eq!(unify(&mut dag, left, right, &mut s), Unified::Ok);
        assert_eq!(
            s.bound_count(),
            0,
            "already-equal binders unify with the empty substitution"
        );
    }

    #[test]
    fn binder_with_metavar_body_unifies_by_binding() {
        // `∀[sort]. X` unifies with `∀[sort]. p(a)` (an ambient constant `a`, NOT the bound
        // variable) by binding the ambient metavariable X to `p(a)`; applying reproduces the
        // concrete binder.
        let mut dag = TermDag::new();
        let forall = iri(&mut dag, "https://example.org/forall");
        let sort = iri(&mut dag, "https://example.org/sort");
        let p = iri(&mut dag, "https://example.org/p");
        let a = iri(&mut dag, "https://example.org/a");
        let (_x, x_node) = dag.fresh_meta();
        let binder_meta = dag.intern_binder(forall, vec![sort], x_node);

        let concrete_body = dag.intern_app(p, vec![a]);
        let binder_concrete = dag.intern_binder(forall, vec![sort], concrete_body);

        let mut s = Subst::new();
        assert_eq!(
            unify(&mut dag, binder_meta, binder_concrete, &mut s),
            Unified::Ok
        );
        assert_eq!(
            s.resolve(&dag, x_node),
            concrete_body,
            "X binds to the concrete (ambient) binder body"
        );
        assert_eq!(
            apply(&mut dag, &s, binder_meta),
            binder_concrete,
            "applying the unifier makes the two binders identical"
        );
    }

    #[test]
    fn binder_body_capturing_bound_var_clashes() {
        // `∀[sort]. X` against `∀[sort]. p(bound{0,0})` has NO first-order unifier: the
        // ambient metavariable X cannot be the bound variable `bound{0,0}` (it would escape
        // the binder), so the scope check clashes rather than forging an unsound binding.
        let mut dag = TermDag::new();
        let forall = iri(&mut dag, "https://example.org/forall");
        let sort = iri(&mut dag, "https://example.org/sort");
        let p = iri(&mut dag, "https://example.org/p");
        let (_x, x_node) = dag.fresh_meta();
        let binder_meta = dag.intern_binder(forall, vec![sort], x_node);

        let bound = dag.intern_bound(0, 0);
        let captured_body = dag.intern_app(p, vec![bound]);
        let binder_captured = dag.intern_binder(forall, vec![sort], captured_body);

        let mut s = Subst::new();
        assert!(
            matches!(
                unify(&mut dag, binder_meta, binder_captured, &mut s),
                Unified::Clash { .. }
            ),
            "an ambient metavariable cannot capture a local bound variable"
        );
        assert_eq!(s.bound_count(), 0, "a failed unification binds nothing");
    }

    #[test]
    fn binder_over_distinct_sorts_clashes() {
        // Sort fidelity: a binder over sort A and one over sort B are ill-sorted to unify,
        // and clash structurally (sort EQUALITY, enforced by the sorts being children).
        let mut dag = TermDag::new();
        let forall = iri(&mut dag, "https://example.org/forall");
        let sort_a = iri(&mut dag, "https://example.org/A");
        let sort_b = iri(&mut dag, "https://example.org/B");
        let body = dag.intern_bound(0, 0);
        let binder_a = dag.intern_binder(forall, vec![sort_a], body);
        let binder_b = dag.intern_binder(forall, vec![sort_b], body);
        let mut s = Subst::new();
        assert!(
            matches!(
                unify(&mut dag, binder_a, binder_b, &mut s),
                Unified::Clash { .. }
            ),
            "binders over distinct sorts must clash (sort equality)"
        );
    }

    // ── Test 6: order-sorted unification over the math subsort lattice ───────────────────

    /// The authored `math:` number tower `ℕ ⊑ ℤ ⊑ ℚ ⊑ ℝ ⊑ ℂ`, minted as sort leaves plus the
    /// covering `SortOrder` — the caller-derived lattice the order-sorted unifier consults.
    struct NumberTower {
        nat: NodeId,
        int: NodeId,
        rat: NodeId,
        real: NodeId,
        complex: NodeId,
        order: SortOrder,
    }

    fn number_tower(dag: &mut TermDag) -> NumberTower {
        let nat = iri(dag, "https://gmeow.dev/math/NaturalNumber");
        let int = iri(dag, "https://gmeow.dev/math/Integer");
        let rat = iri(dag, "https://gmeow.dev/math/RationalNumber");
        let real = iri(dag, "https://gmeow.dev/math/RealNumber");
        let complex = iri(dag, "https://gmeow.dev/math/ComplexNumber");
        let order =
            SortOrder::from_subclass_edges(&[(nat, int), (int, rat), (rat, real), (real, complex)]);
        NumberTower {
            nat,
            int,
            rat,
            real,
            complex,
            order,
        }
    }

    #[test]
    fn subsort_metavar_binds_a_narrower_term_but_not_a_wider_one() {
        // A metavar X:ℝ unifies with a term of sort ℤ (ℤ ⊑ ℝ) — the subsort binding the
        // unsorted equality rule would wrongly reject. The reverse, Y:ℤ against a term of
        // sort ℝ, clashes (ℝ ⋢ ℤ).
        let mut dag = TermDag::new();
        let tower = number_tower(&mut dag);
        let three = iri(&mut dag, "https://example.org/three"); // a constant of sort ℤ
        let pi = iri(&mut dag, "https://example.org/pi"); // a constant of sort ℝ
        let term_sorts = HashMap::from([(three, tower.int), (pi, tower.real)]);
        let ctx = SortContext::new(tower.order.clone(), term_sorts, HashMap::new());

        // X:ℝ vs three:ℤ → Ok, binds X := three.
        let (x, x_node) = dag.fresh_meta();
        let mut s = Subst::new();
        s.declare_meta_sort(x, tower.real);
        assert_eq!(
            unify_sorted(&mut dag, x_node, three, &mut s, &ctx),
            Unified::Ok,
            "ℤ ⊑ ℝ: a metavar of sort ℝ must accept a term of sort ℤ"
        );
        assert_eq!(s.resolve(&dag, x_node), three, "X binds to the ℤ term");

        // Y:ℤ vs pi:ℝ → Clash (ℝ ⋢ ℤ).
        let (y, y_node) = dag.fresh_meta();
        let mut s = Subst::new();
        s.declare_meta_sort(y, tower.int);
        assert!(
            matches!(
                unify_sorted(&mut dag, y_node, pi, &mut s, &ctx),
                Unified::Clash { .. }
            ),
            "ℝ ⋢ ℤ: a metavar of sort ℤ must reject a term of sort ℝ"
        );
        assert_eq!(s.bound_count(), 0, "a sort clash binds nothing");
    }

    #[test]
    fn sorted_metavar_metavar_union_takes_the_meet() {
        // X:ℝ unified with Y:ℤ succeeds, and both resolve to one metavariable whose refined
        // sort is meet(ℝ,ℤ) = ℤ (the narrower of the chain).
        let mut dag = TermDag::new();
        let tower = number_tower(&mut dag);
        let ctx = SortContext::new(tower.order.clone(), HashMap::new(), HashMap::new());

        let (x, x_node) = dag.fresh_meta();
        let (y, y_node) = dag.fresh_meta();
        let mut s = Subst::new();
        s.declare_meta_sort(x, tower.real);
        s.declare_meta_sort(y, tower.int);

        assert_eq!(
            unify_sorted(&mut dag, x_node, y_node, &mut s, &ctx),
            Unified::Ok,
            "two comparable sorted metavars must unify"
        );
        let rx = s.resolve(&dag, x_node);
        let ry = s.resolve(&dag, y_node);
        assert_eq!(rx, ry, "both metavars resolve to one representative");
        assert!(
            matches!(dag.data(rx), NodeData::Meta(_)),
            "the representative is still an (unbound) metavariable"
        );
        assert_eq!(
            ctx.sort_of(&dag, rx, &s),
            Some(tower.int),
            "the representative's sort is meet(ℝ,ℤ) = ℤ"
        );
    }

    #[test]
    fn incomparable_sorts_clash() {
        // A sort `Bool` sits outside the number tower (no covering edge to it). A metavar of a
        // number sort cannot bind a term of sort `Bool` — the sorts are incomparable.
        let mut dag = TermDag::new();
        let tower = number_tower(&mut dag);
        let bool_sort = iri(&mut dag, "https://example.org/Bool");
        let flag = iri(&mut dag, "https://example.org/flag"); // a constant of sort Bool
        let term_sorts = HashMap::from([(flag, bool_sort)]);
        let ctx = SortContext::new(tower.order.clone(), term_sorts, HashMap::new());

        let (m, m_node) = dag.fresh_meta();
        let mut s = Subst::new();
        s.declare_meta_sort(m, tower.real); // a number sort
        assert!(
            matches!(
                unify_sorted(&mut dag, m_node, flag, &mut s, &ctx),
                Unified::Clash { .. }
            ),
            "Bool ⋢ ℝ (incomparable): a number metavar must reject a Bool term"
        );
        assert_eq!(
            s.bound_count(),
            0,
            "an incomparable sort clash binds nothing"
        );
    }

    #[test]
    fn sort_order_closure_and_meet() {
        // from_subclass_edges over the number tower's covering edges gives the full
        // reflexive-transitive subsort order and the chain meets.
        let mut dag = TermDag::new();
        let tower = number_tower(&mut dag);
        let o = &tower.order;

        assert!(o.leq(tower.nat, tower.nat), "leq is reflexive");
        assert!(o.leq(tower.nat, tower.real), "ℕ ⊑ ℝ through the closure");
        assert!(o.leq(tower.nat, tower.complex), "ℕ ⊑ ℂ through the closure");
        assert!(
            !o.leq(tower.real, tower.nat),
            "ℝ ⋢ ℕ (order is not symmetric)"
        );
        assert_eq!(
            o.meet(tower.int, tower.real),
            Some(tower.int),
            "meet(ℤ,ℝ) = ℤ"
        );
        assert_eq!(
            o.meet(tower.nat, tower.complex),
            Some(tower.nat),
            "meet(ℕ,ℂ) = ℕ"
        );
        assert_eq!(
            o.meet(tower.real, tower.int),
            Some(tower.int),
            "meet is symmetric"
        );
    }

    #[test]
    fn empty_sort_context_matches_the_unsorted_path() {
        // An order-sorted unify with a sortless context (no declared metavar sorts, no term
        // tags) must produce exactly the unsorted result — the backward-compatibility contract.
        let mut dag = TermDag::new();
        let f = iri(&mut dag, "https://example.org/f");
        let a = iri(&mut dag, "https://example.org/a");
        let f_of_a = dag.intern_app(f, vec![a]);
        let ctx = SortContext::default();

        // Sortless metavar X against f(a): binds identically on both paths.
        let (_x, x_node) = dag.fresh_meta();
        let mut s_plain = Subst::new();
        let plain = unify(&mut dag, x_node, f_of_a, &mut s_plain);
        let (_x2, x2_node) = dag.fresh_meta();
        let mut s_sorted = Subst::new();
        let sorted = unify_sorted(&mut dag, x2_node, f_of_a, &mut s_sorted, &ctx);
        assert_eq!(
            plain, sorted,
            "empty context must match the unsorted verdict"
        );
        assert_eq!(sorted, Unified::Ok);
        assert_eq!(s_plain.resolve(&dag, x_node), f_of_a);
        assert_eq!(s_sorted.resolve(&dag, x2_node), f_of_a);

        // A structural clash is likewise identical on both paths.
        let g = iri(&mut dag, "https://example.org/g");
        let g_of_a = dag.intern_app(g, vec![a]);
        let mut s1 = Subst::new();
        let mut s2 = Subst::new();
        assert_eq!(
            unify(&mut dag, f_of_a, g_of_a, &mut s1),
            unify_sorted(&mut dag, f_of_a, g_of_a, &mut s2, &ctx),
            "a rigid clash is unaffected by an empty sort context"
        );
    }
}
