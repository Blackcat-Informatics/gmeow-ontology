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

#[cfg(test)]
mod test_support;

use std::collections::{HashMap, HashSet};

use crate::physical::id::{MetaId, NodeId};
use gmeow_term_arena::engine::{NodeData, TermDag};

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

#[path = "unify.tests.rs"]
#[cfg(test)]
mod tests;
