// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Full-FOL backward resolution with SLG tabling over compound terms and
//! three-valued SLG-WFS well-founded negation.
//!
//! # What this is
//!
//! The structured backward resolver: it resolves a goal over a program whose atoms
//! carry *function-symbol* (compound) terms — `append`/`member` over `cons`/`nil`,
//! Peano `add`/`nat`, a `win/move` game — which the flat binary magic path
//! ([`crate::physical::magic`]) cannot express. It is the integration keystone of the
//! full-FOL resolution tower: it stands on the hash-consed [`TermDag`], the occurs-checked order-sorted
//! [`unify_sorted`] unifier, and the checkable [`crate::physical::proof`] objects, and every
//! answer it yields is proof-carrying and validated by [`check`].
//!
//! # Tabling over compound terms (SLG)
//!
//! Resolution is **goal-directed and tabled**. A subgoal *call* is keyed by the content of
//! its subsumption-general call pattern ([`canon`]): bound positions carry their ground
//! sub-terms and free positions are canonicalized metavariables, so two *variant* calls share
//! one answer table and a more-general demanded call subsumes (and serves) a more-specific one
//! — the same subsumptive-demand discipline the binary magic path applies through
//! [`crate::physical::magic`]'s `minimal_antichain`, lifted from binary adornment bitsets to
//! full term subsumption. Answers are [`NodeId`]s deduped by content identity, and the
//! join/matching primitive is [`unify_sorted`] (occurs-checked ⇒ finite terms, no
//! rational-tree unsoundness). Tabling gives goal-directed **grounding**: the fixpoint
//! discovers exactly the ground rule instances relevant to the goal.
//!
//! # Three-valued well-founded negation (SLG-WFS)
//!
//! Negation is well-founded and three-valued. During tabled grounding a negative literal
//! `not A` is **delayed**: its subgoal `A` is demanded (so `A`'s own rules are grounded) but
//! the derivation is not constrained by `A`'s truth. Once the relevant subgoal tables have
//! completed, the delayed literals are **simplified** by evaluating the well-founded model of
//! the discovered ground residual program via the van-Gelder alternating fixpoint (`Γ²`): an
//! atom is `True` (in the least fixpoint `W = lfp(Γ²)`), `False` (outside `Γ(W)`), or
//! `Undefined` (in `Γ(W)∖W`). An atom trapped in a negative loop — `p :- not p`, or
//! `win(X) :- move(X, Y), not win(Y)` over a cyclic move graph — evaluates to `Undefined`
//! under the well-founded model, NEVER a fabricated `True`/`False`. Stratified NAF (the
//! negated subgoal is founded) resolves to a definite `True`/`False`. This runs on the SAME
//! answer tables the positive resolution builds.
//!
//! # Incomplete, never wrong — and deterministic
//!
//! Function symbols make the Herbrand base infinite (semi-decidable): an open Peano query
//! `nat(X)` grounds forever. The [`Budget`]'s `max_steps` bounds answer expansion; on a cut
//! the resolver returns a **sound partial** answer set with [`BudgetStatus::Exhausted`] —
//! every returned binding is a genuine answer (a positive program truncated mid-grounding
//! yields a SUBSET of its true model). Answer expansion and step-charging proceed in a
//! **deterministic content-key order** (never hash-iteration order), so two runs under the
//! same [`Budget`] return the byte-identical sound partial set.
//!
//! # Proof-carrying answers
//!
//! Every produced answer carries a [`crate::physical::proof`] node — [`proof_assert`] for a
//! unit-clause (EDB) instance, [`proof_by_rule`] for a rule firing — built against the
//! program's [`RuleCtx`], so [`check`] re-derives (and thereby VALIDATES) it. A wrong answer
//! cannot carry a checkable proof.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use purrdf::TermValue;

use crate::physical::id::{MetaId, NodeId, TermId};
use crate::physical::proof::{GroundClause, RuleCtx, proof_assert, proof_by_rule};
use crate::physical::seminaive::{NativeOutcome, UnsupportedKind};
use crate::physical::term_dag::{NodeData, TermDag};
use crate::physical::unify::{SortContext, Subst, Unified, apply, unify_sorted};
use crate::query_ir::{AnswerSet, Binding, Budget, CompletionFrontier, QAtom, QProgram, QTerm};
use crate::seam::BudgetStatus;

// ── Program model ─────────────────────────────────────────────────────────────────

/// A body literal: a positive atom or a negation-as-failure atom.
#[derive(Debug, Clone, Copy)]
pub(crate) enum FolLit {
    /// A positive atom (an `App` node whose variables are [`NodeData::Meta`]).
    Pos(NodeId),
    /// A negation-as-failure atom (well-founded).
    Neg(NodeId),
}

/// A program clause `head :- body`; a *fact*/unit clause has an empty body.
///
/// The `head` and body atoms are `App` nodes in the shared [`TermDag`], with the clause's
/// variables interned as [`NodeData::Meta`] nodes (shared across head and body). `rule_iri`
/// is the content-addressed rule identity a [`proof_by_rule`] node cites and [`check`]
/// re-derives against.
#[derive(Debug, Clone)]
pub(crate) struct FolClause {
    /// The head atom.
    pub(crate) head: NodeId,
    /// The body literals (empty ⇒ a fact / unit clause).
    pub(crate) body: Vec<FolLit>,
    /// The content-addressed rule-IRI handle (for proof carrying / checking).
    pub(crate) rule_iri: TermId,
}

/// A structured backward program: clauses plus the single backward goal.
#[derive(Debug, Clone)]
pub(crate) struct FolProgram {
    /// The program clauses (rules and facts), in a deterministic authored order.
    pub(crate) clauses: Vec<FolClause>,
    /// The goal atom (an `App` node; its variables are [`NodeData::Meta`]).
    pub(crate) goal: NodeId,
    /// The goal's answer variables, paired `(metavariable node, surface name)`, so a
    /// projected answer maps each name to the resolved sub-term surface.
    pub(crate) goal_vars: Vec<(NodeId, String)>,
    /// Declared sort of any sorted metavariable (goal or clause), for the order-sorted
    /// unifier. Empty ⇒ the unsorted path (behaves exactly like plain unification).
    pub(crate) meta_sorts: HashMap<MetaId, NodeId>,
}

/// The three-valued verdict of a ground atom under the well-founded model.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Truth {
    /// In the well-founded true set `W`.
    True,
    /// Outside `Γ(W)` — well-founded false.
    False,
    /// In `Γ(W) ∖ W` — well-founded undefined (a negative loop).
    Undefined,
}

/// One projected answer: the goal variable bindings, the ground answer atom, and the
/// checkable proof node.
#[derive(Debug, Clone)]
pub(crate) struct FolBinding {
    /// The goal variable → resolved sub-term surface map.
    pub(crate) bindings: Binding,
    /// The ground answer atom node.
    pub(crate) atom: NodeId,
    /// The checkable proof node ([`check`] re-derives it).
    pub(crate) proof: NodeId,
}

/// The decided outcome of a structured resolution.
#[derive(Debug)]
pub(crate) struct FolOutcome {
    /// The projected goal answers (deterministically ordered), each proof-carrying.
    pub(crate) answers: Vec<FolBinding>,
    /// Whether resolution completed within budget.
    pub(crate) status: BudgetStatus,
    /// The rule/EDB context [`check`] re-derives an answer proof against.
    pub(crate) rule_ctx: RuleCtx,
    /// The well-founded true set (content keys) — for [`Self::truth_of`].
    true_set: BTreeSet<String>,
    /// The not-false set `Γ(W)` (content keys) — true ∪ undefined.
    not_false: BTreeSet<String>,
}

impl FolOutcome {
    /// The three-valued well-founded verdict of the ground atom `node`.
    ///
    /// An atom the grounding never reached is well-founded [`Truth::False`] (no rule founds
    /// it) — exactly the well-founded model's treatment of an atom with no support.
    pub(crate) fn truth_of(&self, dag: &TermDag, node: NodeId) -> Truth {
        let key = dag.key(node);
        if self.true_set.contains(key) {
            Truth::True
        } else if self.not_false.contains(key) {
            Truth::Undefined
        } else {
            Truth::False
        }
    }
}

/// The control outcome of the resolver: a decided model or a typed unsupported gap.
#[derive(Debug)]
pub(crate) enum FolControl {
    /// The resolver decided the goal. Boxed: [`FolOutcome`] now carries the well-founded
    /// true/not-false sets alongside the rule context, so it is comfortably larger than the
    /// typed-gap variant — boxing keeps `FolControl` itself pointer-sized rather than sized
    /// to its largest payload.
    Decided(Box<FolOutcome>),
    /// A typed gap (e.g. a floundering NAF goal). Surfaced by dispatch as a hard failure,
    /// never a fabricated answer.
    Unsupported(UnsupportedKind),
}

// ── Ground residual program ─────────────────────────────────────────────────────────

/// A discovered ground rule instance `head :- pos, not neg` (all atoms ground).
#[derive(Debug, Clone)]
struct GroundRule {
    /// The ground head atom.
    head: NodeId,
    /// Ground positive body atoms.
    pos: Vec<NodeId>,
    /// Ground negative body atoms.
    neg: Vec<NodeId>,
    /// The clause rule-IRI (proof identity).
    rule_iri: TermId,
    /// Whether this came from a unit clause (empty body) — proved by `assert`, not `by_rule`.
    unit: bool,
}

// ── The engine ────────────────────────────────────────────────────────────────────

/// The tabled grounding + WFS engine state (the [`TermDag`] is threaded separately so the
/// mutable-arena borrow never conflicts with the immutable state reads).
struct Engine {
    /// Answer tables: canonical call key → (answer content key → (answer atom, proof)).
    tables: HashMap<String, BTreeMap<String, (NodeId, NodeId)>>,
    /// The demanded calls: canonical key → a representative call node.
    calls: BTreeMap<String, NodeId>,
    /// Discovered ground rules, deduped by content key.
    ground_rules: BTreeMap<String, GroundRule>,
    /// Every ground atom discovered (content key → node), for the WFS universe.
    atoms: BTreeMap<String, NodeId>,
    /// Declared metavariable sorts (grows as clause variables are renamed).
    meta_sorts: HashMap<MetaId, NodeId>,
    /// Committed answer-expansion steps (the budget probe; deterministic).
    steps: u64,
    /// Whether a budget cut left the grounding incomplete.
    exhausted: bool,
    /// Whether a NAF literal floundered (an unbound negated variable).
    floundered: bool,
}

/// A single new derivation discovered in an expansion round.
struct Deriv {
    /// The canonical call key this answers.
    call_key: String,
    /// The clause index (for deterministic ordering).
    rule_idx: usize,
    /// The answer atom (an instance of the call).
    answer_atom: NodeId,
    /// The proof of the answer.
    answer_proof: NodeId,
    /// The ground rule this firing witnesses.
    ground_rule: GroundRule,
    /// The ground-rule content key (dedup identity).
    rule_key: String,
    /// The answer content key (table dedup identity).
    answer_key: String,
}

impl Engine {
    fn new(meta_sorts: HashMap<MetaId, NodeId>) -> Self {
        Self {
            tables: HashMap::new(),
            calls: BTreeMap::new(),
            ground_rules: BTreeMap::new(),
            atoms: BTreeMap::new(),
            meta_sorts,
            steps: 0,
            exhausted: false,
            floundered: false,
        }
    }

    /// A fresh substitution pre-loaded with every known metavariable sort, so order-sorted
    /// unification consults the goal/clause sort declarations.
    fn fresh_subst(&self) -> Subst {
        let mut s = Subst::new();
        // Deterministic: declare in ascending MetaId order.
        let mut sorts: Vec<(MetaId, NodeId)> =
            self.meta_sorts.iter().map(|(m, n)| (*m, *n)).collect();
        sorts.sort_by_key(|(m, _)| m.index());
        for (m, sort) in sorts {
            s.declare_meta_sort(m, sort);
        }
        s
    }

    /// Register a demanded call (its canonical pattern), returning the canonical key.
    fn register_call(&mut self, dag: &TermDag, node: NodeId) -> String {
        let key = canon(dag, node, &self.meta_sorts);
        self.calls.entry(key.clone()).or_insert(node);
        self.tables.entry(key.clone()).or_default();
        key
    }

    /// Record a ground atom in the WFS universe.
    fn record_atom(&mut self, dag: &TermDag, node: NodeId) {
        let key = dag.key(node).to_owned();
        self.atoms.entry(key).or_insert(node);
    }

    /// The stored answers whose atom unifies with the (partially instantiated) call `a`,
    /// as `(answer_atom, answer_proof)` pairs, in deterministic content-key order.
    fn answers_for(&self, dag: &TermDag, a: NodeId) -> Vec<(NodeId, NodeId)> {
        let key = canon(dag, a, &self.meta_sorts);
        match self.tables.get(&key) {
            Some(table) => table.values().copied().collect(),
            None => Vec::new(),
        }
    }
}

// ── Canonical call key + rendering ──────────────────────────────────────────────────

/// The canonical (variant) key of a call/term: metavariables renamed to a first-occurrence
/// ordinal, so two variant call patterns share one key and one answer table.
///
/// The ordinal alone is NOT a sound table identity for an order-sorted program: `p(X:ℤ)` and
/// `p(X:ℝ)` both rename their sole variable to `?v0`, so keying on the ordinal alone would
/// collapse them onto one table and serve one call the OTHER sort's answer set. So each
/// metavariable's DECLARED sort (from `meta_sorts`) is folded into its ordinal token — different
/// sorts ⇒ distinct keys ⇒ distinct tables, while an identical ordinal+sort still shares.
fn canon(dag: &TermDag, node: NodeId, meta_sorts: &HashMap<MetaId, NodeId>) -> String {
    let mut map: HashMap<MetaId, usize> = HashMap::new();
    let mut ctr = 0usize;
    let mut out = String::new();
    canon_rec(dag, node, meta_sorts, &mut map, &mut ctr, &mut out);
    out
}

fn canon_rec(
    dag: &TermDag,
    node: NodeId,
    meta_sorts: &HashMap<MetaId, NodeId>,
    map: &mut HashMap<MetaId, usize>,
    ctr: &mut usize,
    out: &mut String,
) {
    match dag.data(node) {
        NodeData::Meta(m) => {
            let id = *map.entry(*m).or_insert_with(|| {
                let v = *ctr;
                *ctr += 1;
                v
            });
            out.push_str("?v");
            out.push_str(&id.to_string());
            // Fold the declared sort so a sorted variant keys DISTINCTLY (the sort's content
            // key is ground and canonical). A sortless metavariable adds nothing, so the
            // unsorted path is byte-identical to before.
            if let Some(sort) = meta_sorts.get(m) {
                out.push(':');
                out.push_str(dag.key(*sort));
            }
        }
        NodeData::Leaf(_) | NodeData::Free(_) | NodeData::Bound { .. } => {
            // No metavariables reachable — the cached content key is already canonical.
            out.push('=');
            out.push_str(dag.key(node));
        }
        NodeData::App { op, args } => {
            let (op, args) = (*op, args.clone());
            out.push_str("A(");
            canon_rec(dag, op, meta_sorts, map, ctr, out);
            for a in args.iter() {
                out.push(',');
                canon_rec(dag, *a, meta_sorts, map, ctr, out);
            }
            out.push(')');
        }
        NodeData::Binder { op, sorts, body } => {
            let (op, sorts, body) = (*op, sorts.clone(), *body);
            out.push_str("B(");
            canon_rec(dag, op, meta_sorts, map, ctr, out);
            for s in sorts.iter() {
                out.push(',');
                canon_rec(dag, *s, meta_sorts, map, ctr, out);
            }
            out.push(';');
            canon_rec(dag, body, meta_sorts, map, ctr, out);
            out.push(')');
        }
    }
}

/// Render a ground term to a deterministic functional surface: an IRI/literal leaf to its
/// lexical/IRI text, and `op(arg, …)` for an application (a nullary application is bare
/// `op`). Used for the answer-binding surfaces.
///
/// # Binders (G10)
///
/// A [`NodeData::Binder`] renders its FULL de-Bruijn-faithful structure — `op[sorts…].body`
/// — rather than collapsing to an opaque literal. Bound occurrences inside `body` already
/// render as `_b{debruijn}.{slot}` (locally-nameless, alpha-invariant), so two
/// structurally-distinct binder terms render to two distinct surfaces and two alpha-equal
/// ones render identically.
///
/// # NOT an identity key (G11)
///
/// This surface is **human-facing** — the `goalDirectedAtom` binding literal — and is **NOT**
/// the answer identity/dedup key. The `op(arg,…)` application join and the `op[sorts…].body`
/// binder join are **comma-delimited**, so the rendering is **NON-INJECTIVE over comma-bearing
/// content**: an IRI or literal whose lexical text legally contains a comma (or a compound
/// sort) can make two structurally DISTINCT terms render to the SAME string — e.g. the 2-ary
/// `f(a, b)` and the 1-ary application of the single IRI leaf `"a,b"` both render `"f(a,b)"`.
/// [`project`] therefore dedups (and orders on tie) by the arena CONTENT KEY ([`TermDag::key`],
/// the engine's content address for answer tables and rule identity), NEVER by this rendered
/// string — so a genuinely distinct answer is never silently dropped (a completeness bug).
pub(crate) fn render(dag: &TermDag, node: NodeId) -> String {
    match dag.data(node) {
        NodeData::Leaf(tid) | NodeData::Free(tid) => render_atom(dag.atom_value(*tid)),
        NodeData::Bound { debruijn, slot } => format!("_b{debruijn}.{slot}"),
        NodeData::Meta(m) => format!("?{}", m.index()),
        NodeData::App { op, args } => {
            let (op, args) = (*op, args.clone());
            if args.is_empty() {
                return render(dag, op);
            }
            let inner: Vec<String> = args.iter().map(|a| render(dag, *a)).collect();
            format!("{}({})", render(dag, op), inner.join(","))
        }
        NodeData::Binder { op, sorts, body } => {
            let (op, sorts, body) = (*op, sorts.clone(), *body);
            let sort_strs: Vec<String> = sorts.iter().map(|s| render(dag, *s)).collect();
            format!(
                "{}[{}].{}",
                render(dag, op),
                sort_strs.join(","),
                render(dag, body)
            )
        }
    }
}

fn render_atom(tv: &TermValue) -> String {
    match tv {
        TermValue::Iri(iri) => iri.clone(),
        other => crate::provenance::term_display(other),
    }
}

/// Whether `node` is ground (no free metavariables).
fn is_ground(dag: &TermDag, node: NodeId) -> bool {
    dag.free_meta(node).is_empty()
}

/// The free metavariables of `node`, in ascending order.
fn metas_of(dag: &TermDag, node: NodeId) -> Vec<MetaId> {
    dag.free_meta(node).iter().collect()
}

// ── Clause renaming ─────────────────────────────────────────────────────────────────

/// A clause renamed with fresh metavariables (one firing), plus the sort declarations for
/// those fresh metavariables.
struct Renamed {
    head: NodeId,
    body: Vec<FolLit>,
    rule_iri: TermId,
}

/// Rename every clause variable to a FRESH metavariable, so a rule firing never shares
/// variables with the call or with a sibling firing. The fresh metavariables inherit the
/// origin variable's declared sort (recorded into `engine.meta_sorts`).
fn rename_clause(dag: &mut TermDag, engine: &mut Engine, clause: &FolClause) -> Renamed {
    // Collect the clause's metavariables (head + body).
    let mut vars: BTreeSet<usize> = BTreeSet::new();
    let mut ordered: Vec<MetaId> = Vec::new();
    let push =
        |dag: &TermDag, node: NodeId, vars: &mut BTreeSet<usize>, ordered: &mut Vec<MetaId>| {
            for m in metas_of(dag, node) {
                if vars.insert(m.index()) {
                    ordered.push(m);
                }
            }
        };
    push(dag, clause.head, &mut vars, &mut ordered);
    for lit in &clause.body {
        let atom = match lit {
            FolLit::Pos(a) | FolLit::Neg(a) => *a,
        };
        push(dag, atom, &mut vars, &mut ordered);
    }

    // Build the renaming substitution old-meta → fresh-meta node, carrying sorts forward.
    let mut sub = Subst::new();
    for m in &ordered {
        let (fm, fnode) = dag.fresh_meta();
        sub.bind_renaming(*m, fnode);
        if let Some(sort) = engine.meta_sorts.get(m) {
            let sort = *sort;
            engine.meta_sorts.insert(fm, sort);
        }
    }
    let head = apply(dag, &sub, clause.head);
    let body = clause
        .body
        .iter()
        .map(|lit| match lit {
            FolLit::Pos(a) => FolLit::Pos(apply(dag, &sub, *a)),
            FolLit::Neg(a) => FolLit::Neg(apply(dag, &sub, *a)),
        })
        .collect();
    Renamed {
        head,
        body,
        rule_iri: clause.rule_iri,
    }
}

// ── Body solving (SLD over the current tables) ──────────────────────────────────────

/// One partial body solution: the accumulated substitution, the positive premises' proofs
/// (in body order), and the ground negative atoms discovered.
struct BodySolution {
    subst: Subst,
    premises: Vec<NodeId>,
    negs: Vec<NodeId>,
}

/// Solve a clause body against the CURRENT answer tables, returning every completed
/// solution.
///
/// # Safe literal selection (SLG safe-computation rule / SIPS)
///
/// Body literals are NOT processed in bare authored order: at each step the solver selects,
/// among the not-yet-processed literals, the FIRST one (in original-index order, for
/// determinism) that is *safe* to resolve now — a positive literal is always safe (it
/// PRODUCES bindings by joining the demanded subgoal's stored answers), while a negative
/// literal is safe only once every one of its variables is already bound by an
/// already-selected positive literal (its instantiated atom is ground). This is the
/// standard mode-constrained selection function real SLG/tabling engines use: `not A`
/// consumes bindings, it never produces them, so selecting it before its variables are
/// bound is unsound NAF-over-an-open-goal, not a sound answer. Selecting a SAFE literal
/// first means the authored conjunct order of e.g. `win(X) :- move(X, Y), not win(Y).` and
/// its reversal `win(X) :- not win(Y), move(X, Y).` both resolve identically: `not win(Y)`
/// is deferred until `move(X, Y)` — wherever it sits in the body — has bound `Y`.
///
/// A positive literal joins the stored answers of its (demanded) subgoal; a negative
/// literal is *delayed* (its subgoal is demanded but the derivation is not constrained),
/// recording the ground negated atom. Floundering — [`Engine::floundered`] — is raised ONLY
/// when NO remaining literal is safe under ANY selection order (every remaining literal is a
/// negative atom with a variable no positive literal, present or absent, could ever bind),
/// never merely because the authored order happened to place a negative literal first.
///
/// `remaining` is a `u64` bitmask over body-literal indices (bit `i` set ⇒ `body[i]` is
/// not yet selected) rather than a heap-allocated index list — the caller
/// ([`resolve_fol`]) rejects any clause body wider than 64 literals up front
/// ([`UnsupportedKind::ClauseBodyTooWide`]), so every mask built here and in
/// [`expand_round`] is guaranteed to fit.
fn solve_body(
    dag: &mut TermDag,
    engine: &mut Engine,
    ctx: &SortContext,
    body: &[FolLit],
    remaining: u64,
    state: BodySolution,
    out: &mut Vec<BodySolution>,
) {
    if engine.floundered {
        return;
    }
    if remaining == 0 {
        out.push(state);
        return;
    }
    // Scan the mask in ascending (original-index-preserving) order and select the FIRST safe
    // literal. When the authored order is already safe this picks the lowest set bit every
    // time, so a program with no negation-before-binding hazard behaves byte-identically to
    // plain left-to-right solving.
    let mut chosen: Option<usize> = None;
    for (i, lit) in body.iter().enumerate() {
        if remaining & (1u64 << i) == 0 {
            continue;
        }
        let safe = match *lit {
            FolLit::Pos(_) => true,
            FolLit::Neg(atom) => {
                let a = apply(dag, &state.subst, atom);
                is_ground(dag, a)
            }
        };
        if safe {
            chosen = Some(i);
            break;
        }
    }
    let sel = match chosen {
        Some(i) => i,
        None => {
            // Every remaining literal is negative and unbound under EVERY selection order —
            // no positive literal remains that could ever ground it. Genuine floundering, a
            // declared gap, never a fabricated answer.
            engine.floundered = true;
            return;
        }
    };
    let rest: u64 = remaining & !(1u64 << sel);
    match body[sel] {
        FolLit::Pos(atom) => {
            let a = apply(dag, &state.subst, atom);
            engine.register_call(dag, a);
            let candidates = engine.answers_for(dag, a);
            for (ans_atom, ans_proof) in candidates {
                let mut s2 = state.subst.clone();
                if unify_sorted(dag, a, ans_atom, &mut s2, ctx) == Unified::Ok {
                    let mut premises = state.premises.clone();
                    premises.push(ans_proof);
                    let next = BodySolution {
                        subst: s2,
                        premises,
                        negs: state.negs.clone(),
                    };
                    solve_body(dag, engine, ctx, body, rest, next, out);
                }
            }
        }
        FolLit::Neg(atom) => {
            let a = apply(dag, &state.subst, atom);
            debug_assert!(
                is_ground(dag, a),
                "safe selection guarantees a negative literal is ground when chosen"
            );
            // Demand A so its own rules are grounded; DELAY the truth check to the WFS phase.
            engine.register_call(dag, a);
            engine.record_atom(dag, a);
            let mut negs = state.negs.clone();
            negs.push(a);
            let next = BodySolution {
                subst: state.subst.clone(),
                premises: state.premises.clone(),
                negs,
            };
            solve_body(dag, engine, ctx, body, rest, next, out);
        }
    }
}

// ── Grounding fixpoint (phase 1) ────────────────────────────────────────────────────

/// Compute the content-addressed reifier IRI handle for an asserted ground atom.
///
/// [`crate::physical::proof::check`] independently RECOMPUTES and validates this exact
/// value from the goal (G2: a caller-supplied reifier that does not match is rejected as
/// forged provenance), so this delegates to the single-sourced
/// [`crate::physical::proof::structured_reifier`] recipe rather than folding a second,
/// forkable copy of the same hash here.
fn reifier_of(dag: &mut TermDag, atom: NodeId) -> TermId {
    let iri = super::proof::structured_reifier(dag, atom);
    dag.intern_atom(&TermValue::iri(iri))
}

/// The ground-rule dedup key: head + sorted positive + sorted negative + rule identity.
fn ground_rule_key(
    dag: &TermDag,
    head: NodeId,
    pos: &[NodeId],
    neg: &[NodeId],
    rule: TermId,
) -> String {
    let mut pos_keys: Vec<&str> = pos.iter().map(|n| dag.key(*n)).collect();
    pos_keys.sort_unstable();
    let mut neg_keys: Vec<&str> = neg.iter().map(|n| dag.key(*n)).collect();
    neg_keys.sort_unstable();
    format!(
        "r{}|h={}|p={}|n={}",
        rule.index(),
        dag.key(head),
        pos_keys.join("&"),
        neg_keys.join("&")
    )
}

/// One expansion round: recompute every demanded call against every clause over the current
/// tables, returning ONLY the genuinely new derivations, sorted deterministically.
fn expand_round(
    dag: &mut TermDag,
    engine: &mut Engine,
    ctx: &SortContext,
    program: &FolProgram,
) -> Vec<Deriv> {
    let mut new: Vec<Deriv> = Vec::new();
    // Snapshot the demanded calls in canonical-key order (deterministic).
    let calls: Vec<(String, NodeId)> = engine.calls.iter().map(|(k, n)| (k.clone(), *n)).collect();
    for (call_key, call_node) in calls {
        for (rule_idx, clause) in program.clauses.iter().enumerate() {
            let renamed = rename_clause(dag, engine, clause);
            // Unify the (renamed) head with the demanded call.
            let mut s = engine.fresh_subst();
            if unify_sorted(dag, renamed.head, call_node, &mut s, ctx) != Unified::Ok {
                continue;
            }
            // Solve the body over the current tables.
            let mut solutions: Vec<BodySolution> = Vec::new();
            let seed = BodySolution {
                subst: s,
                premises: Vec::new(),
                negs: Vec::new(),
            };
            // A full mask over the body's literal indices. `n >= 64` is already unreachable
            // (`resolve_fol` rejects any body wider than 64 literals up front), but the
            // explicit branch keeps the `1u64 << n` shift self-evidently safe (a bare
            // `(1u64 << n) - 1` would debug-panic on overflow at `n == 64`).
            let n = renamed.body.len();
            let remaining: u64 = if n == 0 {
                0
            } else if n >= 64 {
                u64::MAX
            } else {
                (1u64 << n) - 1
            };
            solve_body(
                dag,
                engine,
                ctx,
                &renamed.body,
                remaining,
                seed,
                &mut solutions,
            );
            if engine.floundered {
                return Vec::new();
            }
            for sol in solutions {
                let head_g = apply(dag, &sol.subst, renamed.head);
                // A non-ground head cannot be an answer (mode/range violation for this call).
                if !is_ground(dag, head_g) {
                    continue;
                }
                let answer_atom = apply(dag, &sol.subst, call_node);
                if !is_ground(dag, answer_atom) {
                    continue;
                }
                let unit = renamed.body.is_empty();
                let pos: Vec<NodeId> = renamed
                    .body
                    .iter()
                    .filter_map(|lit| match lit {
                        FolLit::Pos(a) => Some(apply(dag, &sol.subst, *a)),
                        FolLit::Neg(_) => None,
                    })
                    .collect();
                let neg = sol.negs.clone();
                let rule_key = ground_rule_key(dag, head_g, &pos, &neg, renamed.rule_iri);
                let answer_key = dag.key(answer_atom).to_owned();

                let already_answer = engine
                    .tables
                    .get(&call_key)
                    .is_some_and(|t| t.contains_key(&answer_key));
                let already_rule = engine.ground_rules.contains_key(&rule_key);
                if already_answer && already_rule {
                    continue;
                }

                // Build the answer proof.
                let proof = if unit {
                    let reifier = reifier_of(dag, head_g);
                    proof_assert(dag, head_g, reifier)
                } else {
                    proof_by_rule(dag, head_g, renamed.rule_iri, &sol.premises)
                };
                let ground_rule = GroundRule {
                    head: head_g,
                    pos,
                    neg,
                    rule_iri: renamed.rule_iri,
                    unit,
                };
                new.push(Deriv {
                    call_key: call_key.clone(),
                    rule_idx,
                    answer_atom,
                    answer_proof: proof,
                    ground_rule,
                    rule_key,
                    answer_key,
                });
            }
        }
    }
    // Deterministic ordering: by (call key, clause index, answer content key, rule key).
    new.sort_by(|a, b| {
        a.call_key
            .cmp(&b.call_key)
            .then(a.rule_idx.cmp(&b.rule_idx))
            .then(a.answer_key.cmp(&b.answer_key))
            .then(a.rule_key.cmp(&b.rule_key))
    });
    new
}

/// Drive the grounding fixpoint to completion or a deterministic budget cut.
fn ground(
    dag: &mut TermDag,
    engine: &mut Engine,
    ctx: &SortContext,
    program: &FolProgram,
    budget: &Budget,
) {
    loop {
        let calls_before = engine.calls.len();
        let derivs = expand_round(dag, engine, ctx, program);
        if engine.floundered {
            return;
        }
        // Filter to genuinely new derivations (either a new answer or a new ground rule).
        let mut progressed = false;
        for d in derivs {
            let table = engine.tables.entry(d.call_key.clone()).or_default();
            let new_answer = !table.contains_key(&d.answer_key);
            let new_rule = !engine.ground_rules.contains_key(&d.rule_key);
            if !new_answer && !new_rule {
                continue;
            }
            progressed = true;

            if new_answer {
                table.insert(d.answer_key.clone(), (d.answer_atom, d.answer_proof));
            }
            if new_rule {
                engine.record_atom(dag, d.ground_rule.head);
                for p in &d.ground_rule.pos {
                    engine.record_atom(dag, *p);
                }
                for n in &d.ground_rule.neg {
                    engine.record_atom(dag, *n);
                }
                engine
                    .ground_rules
                    .insert(d.rule_key.clone(), d.ground_rule);
            }

            // Charge one answer-expansion step; a budget cut leaves a sound partial grounding.
            engine.steps += 1;
            if let Some(max) = budget.max_steps
                && engine.steps >= max
            {
                engine.exhausted = true;
                return;
            }
        }
        // Progress is either a committed answer/rule OR a newly-demanded subgoal (a recursive
        // rule's FIRST round only registers its body subgoal, deriving nothing yet — the
        // demand still advances the search, so the fixpoint must not stop until BOTH the
        // answer set and the demanded-call set are stable).
        let calls_grew = engine.calls.len() > calls_before;
        if !progressed && !calls_grew {
            return;
        }
    }
}

// ── Well-founded model (phase 2, van-Gelder alternating fixpoint) ────────────────────

/// The immediate-consequence operator `Γ(S)`: the least model of the definite program
/// obtained from the discovered ground rules by keeping a rule iff every negative body atom
/// is NOT in `S`, then dropping the (satisfied) negatives. Deterministic (sorted iteration).
fn gamma(dag: &TermDag, engine: &Engine, s: &BTreeSet<String>) -> BTreeSet<String> {
    let mut model: BTreeSet<String> = BTreeSet::new();
    loop {
        let mut added = false;
        for rule in engine.ground_rules.values() {
            let head_key = dag.key(rule.head);
            if model.contains(head_key) {
                continue;
            }
            let neg_ok = rule.neg.iter().all(|n| !s.contains(dag.key(*n)));
            if !neg_ok {
                continue;
            }
            let pos_ok = rule.pos.iter().all(|p| model.contains(dag.key(*p)));
            if pos_ok {
                model.insert(head_key.to_owned());
                added = true;
            }
        }
        if !added {
            break;
        }
    }
    model
}

/// The least model of the negation-FREE subprogram: iterate only the ground rules with an
/// EMPTY negative body. Monotone, so its least fixpoint is a genuine subset of the true
/// well-founded model — every atom in it is founded by a chain of positive facts/rules with no
/// negative dependency whatsoever. This is the ONLY set that is sound to assert definite-True on
/// a budget-truncated grounding, where a negative literal may be false only because its
/// supporting rules were not yet ground.
fn positive_least_model(dag: &TermDag, engine: &Engine) -> BTreeSet<String> {
    let mut model: BTreeSet<String> = BTreeSet::new();
    loop {
        let mut added = false;
        for rule in engine.ground_rules.values() {
            if !rule.neg.is_empty() {
                continue; // negation is not trusted on a cut — positive subprogram only.
            }
            let head_key = dag.key(rule.head);
            if model.contains(head_key) {
                continue;
            }
            if rule.pos.iter().all(|p| model.contains(dag.key(*p))) {
                model.insert(head_key.to_owned());
                added = true;
            }
        }
        if !added {
            break;
        }
    }
    model
}

/// The well-founded true set `W = lfp(Γ²)` and the not-false set `Γ(W)`.
///
/// # Soundness under a budget cut (incomplete grounding)
///
/// When the grounding was truncated ([`Engine::exhausted`]) the discovered ground residual is a
/// SUBSET of the true program, so negation-as-failure is unsound: a negative literal `not q` may
/// read as satisfied only because `q`'s founding rules were not yet ground. Evaluating the
/// alternating fixpoint over that partial program can therefore fabricate a definite `True` (the
/// `p :- not q.` + `q.` hazard). So on a cut we DEMOTE: `W` is the least model of the
/// negation-free subprogram (every member independently positively founded, a sound subset of
/// the true model), and the not-false set is `W` plus every reached atom — so no reached atom is
/// reported definitely False and no negation-bearing ground rule can found a definite answer.
/// Answers stay "incomplete, never wrong"; the [`BudgetStatus::Exhausted`] status discloses it.
fn well_founded(dag: &TermDag, engine: &Engine) -> (BTreeSet<String>, BTreeSet<String>) {
    if engine.exhausted {
        let w = positive_least_model(dag, engine);
        let mut not_false = w.clone();
        not_false.extend(engine.atoms.keys().cloned());
        return (w, not_false);
    }
    let mut w: BTreeSet<String> = BTreeSet::new();
    loop {
        let next = gamma(dag, engine, &gamma(dag, engine, &w));
        if next == w {
            break;
        }
        w = next;
    }
    let not_false = gamma(dag, engine, &w);
    (w, not_false)
}

// ── Proof-carrying least model over Γ(W) (phase 3) ──────────────────────────────────

/// Recompute the TRUE atoms' proofs by a least-model derivation restricted to the
/// well-founded true set `W`, building the [`RuleCtx`] the proofs check against in lockstep:
/// derive an atom only when it is in `W`, its negatives are all false (∉ `W`), and its
/// positive premises are already proven — so every premise proof is itself a proof of a TRUE
/// atom (a founded well-founded justification). Deterministic.
///
/// # Ground-instance proof identity
///
/// A [`proof_by_rule`] node is checked ([`check`]) by re-deriving its head from its premises'
/// unifier. A *general* program rule whose head carries a variable NOT bound by any positive
/// body atom — e.g. `member(X, cons(H, T)) :- member(X, T)`, where `H` is pinned by the CALL,
/// not the body — cannot be re-derived from premises alone. So each firing registers a
/// content-addressed GROUND-instance rule `(ground_head, ground_pos)`: [`check`] then unifies
/// the ground body atoms with the (identical) ground premises and re-derives the exact ground
/// head. This keeps every answer proof-carrying and independently checkable regardless of a
/// clause's mode/range shape; the firing IRI folds the program rule IRI with the ground
/// instance so distinct instances of one clause never collide.
fn build_proofs(
    dag: &mut TermDag,
    engine: &Engine,
    w: &BTreeSet<String>,
    not_false: &BTreeSet<String>,
) -> (HashMap<String, NodeId>, RuleCtx) {
    let mut proven: HashMap<String, NodeId> = HashMap::new();
    // The atoms that are NOT well-founded-false (Γ(W) = true ∪ undefined), as the arena node
    // set [`check`] tests a `by_rule` proof's negative premises against: a negative premise is
    // valid ONLY if its atom is absent here (genuinely FALSE, not merely non-true).
    let mut ctx = RuleCtx {
        not_false: not_false
            .iter()
            .filter_map(|k| engine.atoms.get(k).copied())
            .collect(),
        ..RuleCtx::default()
    };
    loop {
        let mut added = false;
        // Snapshot rules deterministically (sorted map already gives a stable order).
        for rule in engine.ground_rules.values() {
            let head_key = dag.key(rule.head).to_owned();
            if proven.contains_key(&head_key) {
                continue;
            }
            if !w.contains(&head_key) {
                continue;
            }
            // A founded justification requires every negative premise to be well-founded-FALSE
            // (∉ Γ(W)). An Undefined negative (in Γ(W)∖W) is NOT a valid `not` — rejecting it
            // here keeps every built proof soundly founded, and [`check`] re-verifies it.
            if !rule.neg.iter().all(|n| !not_false.contains(dag.key(*n))) {
                continue;
            }
            let mut premise_proofs = Vec::with_capacity(rule.pos.len());
            let mut ready = true;
            for p in &rule.pos {
                match proven.get(dag.key(*p)) {
                    Some(pf) => premise_proofs.push(*pf),
                    None => {
                        ready = false;
                        break;
                    }
                }
            }
            if !ready {
                continue;
            }
            let proof = if rule.unit {
                let reifier = reifier_of(dag, rule.head);
                ctx.asserted.insert(rule.head);
                proof_assert(dag, rule.head, reifier)
            } else {
                let firing_iri = firing_rule_iri(dag, rule);
                ctx.rules.insert(
                    firing_iri,
                    GroundClause {
                        head: rule.head,
                        pos: rule.pos.clone(),
                        neg: rule.neg.clone(),
                    },
                );
                proof_by_rule(dag, rule.head, firing_iri, &premise_proofs)
            };
            proven.insert(head_key, proof);
            added = true;
        }
        if !added {
            break;
        }
    }
    (proven, ctx)
}

/// The content-addressed IRI handle of a GROUND rule instance, folding the program rule IRI
/// with the ground head + positive body so distinct instances of one clause never share a
/// [`RuleCtx`] key.
fn firing_rule_iri(dag: &mut TermDag, rule: &GroundRule) -> TermId {
    let mut hasher = blake3::Hasher::new();
    // Seed from the rule IRI's STABLE lexical surface, never its `TermId` index (a per-DAG
    // mint-order handle that leaks arena history): identical ground firings must fold to the
    // SAME firing IRI across independent DAG interning histories. Length-framed so the IRI's
    // boundary with the head key is unambiguous.
    let rule_iri_str = dag.atom_display(rule.rule_iri);
    hasher.update(&(rule_iri_str.len() as u64).to_le_bytes());
    hasher.update(rule_iri_str.as_bytes());
    hasher.update(dag.key(rule.head).as_bytes());
    for p in &rule.pos {
        hasher.update(b"|");
        hasher.update(dag.key(*p).as_bytes());
    }
    let iri = format!(
        "https://blackcatinformatics.ca/logic/dag/firing/{}",
        hasher.finalize().to_hex()
    );
    dag.intern_atom(&TermValue::iri(iri))
}

// ── Projection (phase 4) ────────────────────────────────────────────────────────────

/// Project the goal's TRUE instances into proof-carrying bindings, deterministically ordered.
fn project(
    dag: &mut TermDag,
    engine: &Engine,
    ctx: &SortContext,
    program: &FolProgram,
    w: &BTreeSet<String>,
    proofs: &HashMap<String, NodeId>,
) -> gmeow_errors::Result<Vec<FolBinding>> {
    // Candidate true atoms in deterministic content-key order. Each row carries a CONTENT
    // identity key (built from the arena content address of every goal-variable binding) used
    // for dedup and tie-order — never the rendered binding surface, which comma-joins compound
    // terms and is therefore non-injective over comma-bearing content (see [`render`] § G11).
    let mut rows: Vec<(String, FolBinding)> = Vec::new();
    let mut seen: BTreeSet<String> = BTreeSet::new();
    let true_atoms: Vec<(String, NodeId)> = engine
        .atoms
        .iter()
        .filter(|(k, _)| w.contains(*k))
        .map(|(k, n)| (k.clone(), *n))
        .collect();
    for (atom_key, atom) in true_atoms {
        // Match the goal against the true atom.
        let mut s = engine.fresh_subst();
        if unify_sorted(dag, program.goal, atom, &mut s, ctx) != Unified::Ok {
            continue;
        }
        // The goal must fully instantiate to this atom (a proper answer instance).
        let goal_instance = apply(dag, &s, program.goal);
        if goal_instance != atom {
            continue;
        }
        let mut binding: Binding = BTreeMap::new();
        // The CONTENT identity of this answer: each goal variable paired with the arena content
        // key of its resolved sub-term. Length-framed so the concatenation is injective (a name
        // or key legally containing the delimiter cannot forge a collision). The rendered
        // surface goes into `binding` for the human-facing literal; `dag.key` is the identity.
        let mut identity = String::new();
        for (meta_node, name) in &program.goal_vars {
            let resolved = apply(dag, &s, *meta_node);
            let rendered = render(dag, resolved);
            let key = dag.key(resolved);
            identity.push_str(&(name.len() as u64).to_string());
            identity.push(':');
            identity.push_str(name);
            identity.push('=');
            identity.push_str(&(key.len() as u64).to_string());
            identity.push(':');
            identity.push_str(key);
            identity.push(';');
            binding.insert(name.clone(), rendered);
        }
        let proof = proofs.get(&atom_key).copied().ok_or_else(|| {
            gmeow_errors::Diag::of_kind(crate::error::Physical {
                detail: format!(
                    "a well-founded-TRUE goal answer {atom_key} has no founded proof — the \
                     proof-carrying least model must cover every TRUE atom"
                ),
            })
        })?;
        // Deterministic dedup by the CONTENT identity — two answers whose surfaces collide
        // under comma-joining but whose content differs BOTH survive (completeness).
        if seen.insert(identity.clone()) {
            rows.push((
                identity,
                FolBinding {
                    bindings: binding,
                    atom,
                    proof,
                },
            ));
        }
    }
    // Sort rows by their binding surface for a stable, deterministic answer order, breaking a
    // rendered-surface tie (a comma-join collision) by the content identity so colliding
    // answers still order deterministically and byte-stably.
    rows.sort_by(|a, b| {
        let ka: Vec<(&String, &String)> = a.1.bindings.iter().collect();
        let kb: Vec<(&String, &String)> = b.1.bindings.iter().collect();
        ka.cmp(&kb).then_with(|| a.0.cmp(&b.0))
    });
    Ok(rows.into_iter().map(|(_, b)| b).collect())
}

// ── The core entry ──────────────────────────────────────────────────────────────────

/// Resolve a structured [`FolProgram`]'s goal against the shared [`TermDag`] with SLG tabling
/// and three-valued well-founded negation.
///
/// # Errors
///
/// Returns `Err` only for an internal invariant breach (e.g. a TRUE atom without a founded
/// proof). A floundering NAF goal is the typed [`FolControl::Unsupported`] outcome, never an
/// error and never a fabricated answer.
pub(crate) fn resolve_fol(
    dag: &mut TermDag,
    program: &FolProgram,
    ctx: &SortContext,
    budget: &Budget,
) -> gmeow_errors::Result<FolControl> {
    // Upfront guard: `solve_body` represents the not-yet-selected body literals as a `u64`
    // bitmask (one bit per literal). Renaming ([`rename_clause`]) only renames variables — it
    // never adds or removes body literals — so checking authored arity here, BEFORE any
    // grounding, guarantees every mask built later (in `expand_round`/`solve_body`) fits. A
    // body wider than 64 literals is an explicit typed refusal, never a silent truncation.
    if program.clauses.iter().any(|c| c.body.len() > 64) {
        return Ok(FolControl::Unsupported(UnsupportedKind::ClauseBodyTooWide));
    }

    let mut engine = Engine::new(program.meta_sorts.clone());
    // Seed: demand the goal, and record any explicitly ground goal atom.
    engine.register_call(dag, program.goal);

    ground(dag, &mut engine, ctx, program, budget);
    if engine.floundered {
        return Ok(FolControl::Unsupported(UnsupportedKind::Floundering));
    }

    let (w, not_false) = well_founded(dag, &engine);
    let (proofs, rule_ctx) = build_proofs(dag, &engine, &w, &not_false);
    let answers = project(dag, &engine, ctx, program, &w, &proofs)?;

    let status = if engine.exhausted {
        BudgetStatus::Exhausted
    } else {
        BudgetStatus::Ok
    };
    Ok(FolControl::Decided(Box::new(FolOutcome {
        answers,
        status,
        rule_ctx,
        true_set: w,
        not_false,
    })))
}

// ── Dispatch-facing entry (QProgram → structured resolution → AnswerSet) ─────────────

/// Whether a parsed [`QProgram`] carries any structured ([`QTerm::Struct`]) argument in its
/// goal, a rule head, or a rule body — the flat/structured routing gate.
pub(crate) fn program_is_structured(program: &QProgram) -> bool {
    let atom_structured = |atom: &QAtom| atom.args.iter().any(|t| matches!(t, QTerm::Struct(_)));
    if program.goal.atoms.iter().any(atom_structured) {
        return true;
    }
    program.rules.iter().any(|r| {
        atom_structured(&r.head)
            || r.body.iter().any(|lit| match lit {
                crate::query_ir::QBodyLit::Atom(a) | crate::query_ir::QBodyLit::Neg(a) => {
                    atom_structured(a)
                }
                crate::query_ir::QBodyLit::Cut | crate::query_ir::QBodyLit::Builtin(_) => false,
            })
    })
}

/// Resolve a *structured* [`QProgram`] via the full-FOL resolver, projecting to the public
/// [`AnswerSet`]. `dag` must own every [`QTerm::Struct`] node the program references (the
/// caller shares the DAG the structured terms were interned into).
///
/// A structured program whose `Struct` nodes are NOT in `dag` (e.g. a fresh dispatch DAG) is
/// a typed [`NativeOutcome::Unsupported`] gap, never a panic.
pub(crate) fn resolve_native_fol(
    dag: &mut TermDag,
    program: &QProgram,
    budget: &Budget,
) -> gmeow_errors::Result<NativeOutcome<AnswerSet>> {
    // Guard: every `Struct` node the program cites must belong to `dag`.
    if !structured_nodes_in_dag(dag, program) {
        return Ok(NativeOutcome::Unsupported(UnsupportedKind::NonBinaryAtom));
    }
    let (fol, goal_vars) = match lower_qprogram(dag, program) {
        Ok(v) => v,
        Err(kind) => return Ok(NativeOutcome::Unsupported(kind)),
    };
    let full = FolProgram {
        clauses: fol.clauses,
        goal: fol.goal,
        goal_vars,
        // The declared metavariable sorts DISCOVERED by the lowering — the single source, so
        // order-sorting is live on this path instead of dead behind a hardcoded empty map. A
        // `QTerm` carries no sort surface today, so the lowering yields an empty map and this
        // path stays unsorted; any future sorted `QTerm` flows through here unchanged.
        meta_sorts: fol.meta_sorts,
    };
    let ctx = SortContext::default();
    match resolve_fol(dag, &full, &ctx, budget)? {
        FolControl::Unsupported(kind) => Ok(NativeOutcome::Unsupported(kind)),
        FolControl::Decided(outcome) => {
            let bindings: Vec<Binding> = outcome.answers.into_iter().map(|a| a.bindings).collect();
            let mut answer = AnswerSet {
                bindings,
                status: outcome.status,
                preservation: crate::result::PreservationClaim::exact(),
                frontier: CompletionFrontier::empty(),
            };
            // Canonicalize BEFORE the answer cap so the kept prefix is deterministic.
            answer.canonicalize();
            // Budget: compose the resolver's step governor (already stamped `Exhausted` on a
            // grounding cut) with the post-projection `max_answers` truncation. Precedence
            // mirrors the reference oracle / binary magic path: a REACHED answer cap stamps
            // `Partial`, overriding a concurrent step `Exhausted`.
            if let Some(max_a) = budget.max_answers
                && answer.bindings.len() >= max_a
                && !answer.bindings.is_empty()
            {
                answer.bindings.truncate(max_a);
                answer.status = BudgetStatus::Partial;
            }
            Ok(NativeOutcome::Decided(answer))
        }
    }
}

/// Whether every `QTerm::Struct` node in `program` is a node of `dag`.
fn structured_nodes_in_dag(dag: &TermDag, program: &QProgram) -> bool {
    let atom_ok = |atom: &QAtom| {
        atom.args.iter().all(|t| match t {
            // Arena-identity membership: the `Struct` node must belong to THIS dag by brand,
            // not merely by an in-range slot index (a foreign node is rejected).
            QTerm::Struct(sn) => dag.contains_node(sn.node(), sn.arena()),
            _ => true,
        })
    };
    if !program.goal.atoms.iter().all(atom_ok) {
        return false;
    }
    program.rules.iter().all(|r| {
        atom_ok(&r.head)
            && r.body.iter().all(|lit| match lit {
                crate::query_ir::QBodyLit::Atom(a) | crate::query_ir::QBodyLit::Neg(a) => {
                    atom_ok(a)
                }
                crate::query_ir::QBodyLit::Cut | crate::query_ir::QBodyLit::Builtin(_) => true,
            })
    })
}

/// A lowered structured program plus the discovered goal variable names and metavariable
/// sort declarations.
struct LoweredProgram {
    clauses: Vec<FolClause>,
    goal: NodeId,
    /// The declared sort of any sorted metavariable minted during lowering (order-sorted
    /// unification consults it). A `QTerm` carries no sort surface today, so this is empty; it
    /// is the single, live source the [`FolProgram`] threads rather than a hardcoded map.
    meta_sorts: HashMap<MetaId, NodeId>,
}

/// Lower a structured [`QProgram`] into DAG clauses. Each clause is lowered with its OWN
/// fresh metavariable per source variable name (shared across that clause's head and body),
/// and interned rule-IRI handle.
fn lower_qprogram(
    dag: &mut TermDag,
    program: &QProgram,
) -> Result<(LoweredProgram, Vec<(NodeId, String)>), UnsupportedKind> {
    let mut clauses = Vec::with_capacity(program.rules.len());
    for (idx, rule) in program.rules.iter().enumerate() {
        let mut vars: HashMap<String, (MetaId, NodeId)> = HashMap::new();
        let head = lower_atom(dag, &rule.head, &mut vars)?;
        let mut body = Vec::with_capacity(rule.body.len());
        for lit in &rule.body {
            match lit {
                crate::query_ir::QBodyLit::Atom(a) => {
                    body.push(FolLit::Pos(lower_atom(dag, a, &mut vars)?));
                }
                crate::query_ir::QBodyLit::Neg(a) => {
                    body.push(FolLit::Neg(lower_atom(dag, a, &mut vars)?));
                }
                crate::query_ir::QBodyLit::Cut => return Err(UnsupportedKind::Cut),
                crate::query_ir::QBodyLit::Builtin(_) => {
                    return Err(UnsupportedKind::Arithmetic(Vec::new()));
                }
            }
        }
        let rule_iri = dag.intern_atom(&TermValue::iri(format!(
            "https://blackcatinformatics.ca/logic/dag/rule/{idx}"
        )));
        clauses.push(FolClause {
            head,
            body,
            rule_iri,
        });
    }
    if program.goal.atoms.len() != 1 {
        return Err(UnsupportedKind::NonBinaryAtom);
    }
    let mut goal_vars_map: HashMap<String, (MetaId, NodeId)> = HashMap::new();
    let goal = lower_atom(dag, &program.goal.atoms[0], &mut goal_vars_map)?;
    let mut goal_vars: Vec<(NodeId, String)> = goal_vars_map
        .into_iter()
        .map(|(name, (_, node))| (node, name))
        .collect();
    goal_vars.sort_by(|a, b| a.1.cmp(&b.1));
    // A `QTerm` carries no sort surface, so the lowering declares no metavariable sorts; the
    // empty map is nonetheless the live, single source the caller threads.
    let meta_sorts: HashMap<MetaId, NodeId> = HashMap::new();
    Ok((
        LoweredProgram {
            clauses,
            goal,
            meta_sorts,
        },
        goal_vars,
    ))
}

/// Lower one [`QAtom`] into an `App` node, interning fresh metavariables per source variable
/// name (shared within the caller's `vars` scope).
fn lower_atom(
    dag: &mut TermDag,
    atom: &QAtom,
    vars: &mut HashMap<String, (MetaId, NodeId)>,
) -> Result<NodeId, UnsupportedKind> {
    let op = dag.intern_leaf(TermValue::iri(atom.pred.clone()));
    let mut args = Vec::with_capacity(atom.args.len());
    for t in &atom.args {
        args.push(lower_term(dag, t, vars)?);
    }
    Ok(dag.intern_app(op, args))
}

/// Lower one [`QTerm`] into a DAG node. `Struct` nodes are used as-is (already interned);
/// `Const`/`Num` become leaves; `Var` interns (or reuses) a fresh metavariable.
fn lower_term(
    dag: &mut TermDag,
    term: &QTerm,
    vars: &mut HashMap<String, (MetaId, NodeId)>,
) -> Result<NodeId, UnsupportedKind> {
    match term {
        QTerm::Struct(sn) => Ok(sn.node()),
        QTerm::Const(c) => {
            let iri = c
                .strip_prefix('<')
                .and_then(|s| s.strip_suffix('>'))
                .unwrap_or(c);
            Ok(dag.intern_leaf(TermValue::iri(iri.to_owned())))
        }
        // A ground quoted-triple lowers to a single interned leaf carrying the
        // reconstructed `TermValue::Triple` (it is a value, not a compound function term).
        QTerm::Triple { .. } => Ok(dag.intern_leaf(super::magic::qterm_to_value(term)?)),
        QTerm::Num(n) => Ok(dag.intern_leaf(TermValue::typed_literal(
            n.to_string(),
            crate::physical::XSD_INTEGER,
        ))),
        QTerm::Var(v) => {
            if let Some((_, node)) = vars.get(v) {
                return Ok(*node);
            }
            let (m, node) = dag.fresh_meta();
            vars.insert(v.clone(), (m, node));
            Ok(node)
        }
    }
}

#[cfg(test)]
mod tests;
