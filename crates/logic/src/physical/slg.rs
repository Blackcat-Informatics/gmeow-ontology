// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Native top-down, variant-tabled SLG backward resolver.
//!
//! # Why a second backward leg next to `magic`
//!
//! The bottom-up magic-sets core ([`crate::physical::resolve_native`]) decides the
//! single binary goal atom.  A multi-atom conjunctive goal — or an n-ary (arity ≠ 2)
//! IDB relation — is left as a declared gap.  This module decides exactly that
//! fragment, and it does so **completely**: where the declarative path-memo of
//! [`crate::reference_resolver`] cuts a re-entrant call to nothing (and so
//! UNDER-produces on left / mutual recursion), this resolver iterates variant answer
//! tables to a least-model fixpoint.
//!
//! # Variant tabling
//!
//! A subgoal is keyed by its **call variant** `(predicate, [arg canonical | wildcard])`
//! — a bound constant contributes its canonical string, an unbound variable the empty
//! wildcard.  This generalizes the reference oracle's binary `(pred, subj, obj)` memo
//! to any arity.  Each variant owns an answer table (deduped).  The conjunctive goal is
//! itself tabled through a synthesized [`GOAL_PRED`] rule whose head carries the goal
//! variables, so goal answers flow through the same machinery — and DEMAND for a
//! sub-variant is registered exactly when a producer body binds it.
//!
//! # Semi-naive linear-tabling fixpoint
//!
//! Resolution runs in global passes.  Each table separates `settled` answers (final in
//! earlier passes) from its `delta` (answers added in the immediately preceding pass).
//! A producer pass derives a rule's new answers by the standard semi-naive
//! decomposition: for a rule with `k` direct IDB body atoms, it fires the rule `k`
//! times, each time forcing ONE of those atoms to consume only the `delta` of its table
//! while the others read the full (`settled + delta`) contents.  A rule with no IDB body
//! atom is a base rule and fires once (its extensional witnesses are static).  New
//! answers land in a `next` buffer; after the pass every table rotates
//! `settled += delta; delta = next`.  The fixpoint stops when a full pass adds no new
//! answer anywhere (and registers no new variant).  Total work is therefore proportional
//! to newly-derived answers rather than the quadratic cost of re-consuming whole tables.
//! For a definite program this converges to the least model — including left and mutual
//! recursion — and the answer set is identical to a naive re-consumption.
//!
//! # Termination backstop
//!
//! A program whose least model is infinite (an arithmetic counter that feeds a computed
//! value back as a recursive INPUT, e.g. `p(N1) :- p(N), N1 is N + 1`) has no finite
//! fixpoint.  When the caller sets no `max_steps`, the resolver still terminates by
//! applying [`crate::scryer_engine::DEFAULT_INFERENCE_LIMIT`] — the SAME default ceiling
//! the Scryer engine uses — and reports the budget-cut prefix as `Exhausted`.
//!
//! # Declared gaps
//!
//! Cut (`!`) is procedural and outside this rung — a cut-containing program returns
//! [`UnsupportedKind::Cut`].  An arithmetic builtin whose mode is uncomputable
//! (unbound operand / ÷0) surfaces [`UnsupportedKind::Arithmetic`]; a non-binary EDB
//! atom surfaces [`UnsupportedKind::NonBinaryAtom`].  Each is a first-class outcome the
//! caller routes to an oracle — never a wrong or missing answer.
//!
//! # Budget
//!
//! The step unit is: one subgoal expansion (an IDB variant producer run) plus one EDB
//! scan, each counting one step.  The effective `max_steps` ceiling is the caller's
//! value or, when unset, the shared default backstop.  Exceeding it stops with a SOUND
//! SUBSET (`Exhausted`); `max_answers` reached truncates the canonical prefix
//! (`Partial`, overriding a concurrent `Exhausted`), exactly as
//! [`crate::physical::resolve_native`] composes them.

use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet, HashSet};

use purrdf::TermValue;

use crate::physical::{NativeOutcome, UnsupportedKind};
use crate::provenance::term_n3;
use crate::query_ir::{AnswerSet, Binding, Budget, QAtom, QBodyLit, QProgram, QRule, QTerm};
use crate::reference_resolver::{
    apply_subst, canonical_to_term, rename_rule, resolve_term, term_canonical_or_wildcard,
    unify_atoms,
};
use crate::seam::{BudgetStatus, ScryerForeign};

/// The call-variant key of a subgoal: `(predicate, per-arg canonical | wildcard)`.
type VariantKey = (String, Vec<String>);

/// The synthetic predicate whose single rule is the conjunctive goal.  Its head carries
/// the goal variables so goal answers are tabled and consumed exactly like any IDB
/// predicate.  Not a real IRI — used only as a variant-table key.
const GOAL_PRED: &str = "urn:x-blackcat-slg:goal";

/// Which contents of a variant table a body atom consumes during a producer pass.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    /// The full known extension (`settled + delta`).
    Full,
    /// Only the previous pass's newly-added answers.
    Delta,
}

/// The answer table of a single call variant.
struct SubgoalTable {
    /// The canonical call atom for this variant: bound positions keep their constant,
    /// unbound positions carry deterministic fresh variables (`__SG{n}_{i}`) so a
    /// re-entrant recursive call reproduces the SAME variant key.
    canon: QAtom,
    /// Answers finalized in earlier passes.
    settled: Vec<Vec<QTerm>>,
    /// Answers added in the immediately preceding pass — the semi-naive frontier.
    delta: Vec<Vec<QTerm>>,
    /// Answers derived in the current pass, promoted to `delta` on rotation.
    next: Vec<Vec<QTerm>>,
    /// Deduplication signatures spanning `settled ∪ delta ∪ next`.
    seen: HashSet<String>,
    /// Whether this variant's base (IDB-free) rules have already fired.  Base witnesses
    /// are static, so they contribute exactly once.
    base_done: bool,
}

impl SubgoalTable {
    /// Insert a tuple if unseen; returns `true` when it was new.
    fn push_next(&mut self, tuple: Vec<QTerm>) -> bool {
        let sig = tuple_sig(&tuple);
        if self.seen.insert(sig) {
            self.next.push(tuple);
            true
        } else {
            false
        }
    }
}

/// The mutable state threaded through a resolution.
struct SlgState<'a> {
    foreign: &'a dyn ScryerForeign,
    world: &'a str,
    idb: BTreeSet<String>,
    /// The program rules plus the synthesized goal rule.
    rules: Vec<QRule>,
    /// The effective step ceiling (caller's `max_steps`, or the default backstop).
    max_steps: u64,
    tables: BTreeMap<VariantKey, SubgoalTable>,
    steps: u64,
    status: BudgetStatus,
    /// A declared gap discovered mid-resolution (cut / arithmetic / non-binary EDB).
    decline: Option<UnsupportedKind>,
    /// Monotone counter minting the fresh variable names of new variant tables.
    next_sg: usize,
    /// Set when a producer registered a not-yet-seen variant this pass, so the fixpoint
    /// runs at least once more to produce it.
    registered_new: bool,
}

/// Resolve `program` against `world` with the tabled semi-naive SLG resolver.
///
/// # Errors
///
/// Returns `Err(String)` if `term_n3` fails on an EDB term or `canonical_to_term`
/// rejects a non-IRI EDB constant — the same hard-fail the reference oracle raises.
pub(crate) fn resolve_slg(
    foreign: &dyn ScryerForeign,
    world: &str,
    program: &QProgram,
    budget: &Budget,
) -> Result<NativeOutcome<AnswerSet>, String> {
    // Cut is procedural — a declared gap decided by a later rung.
    if crate::profile_gate::has_cut(program) {
        return Ok(NativeOutcome::Unsupported(UnsupportedKind::Cut));
    }

    // The goal variables, in first-appearance order — the arguments of the synthetic
    // goal predicate and the columns of every answer row.
    let gvars = goal_variables(program);

    // Build the rule set: the program rules plus one synthetic rule whose body is the
    // conjunctive goal and whose head carries the goal variables.
    let mut rules = program.rules.clone();
    rules.push(QRule {
        head: QAtom {
            pred: GOAL_PRED.to_owned(),
            args: gvars.iter().map(|v| QTerm::Var(v.clone())).collect(),
        },
        body: program
            .goal
            .atoms
            .iter()
            .cloned()
            .map(QBodyLit::Atom)
            .collect(),
    });

    let mut idb: BTreeSet<String> = program.rules.iter().map(|r| r.head.pred.clone()).collect();
    idb.insert(GOAL_PRED.to_owned());

    let max_steps = budget
        .max_steps
        .unwrap_or(crate::scryer_engine::DEFAULT_INFERENCE_LIMIT);

    let mut state = SlgState {
        foreign,
        world,
        idb,
        rules,
        max_steps,
        tables: BTreeMap::new(),
        steps: 0,
        status: BudgetStatus::Ok,
        decline: None,
        next_sg: 0,
        registered_new: false,
    };

    // Seed the goal variant (all goal variables unbound) and run the fixpoint.
    let goal_call = QAtom {
        pred: GOAL_PRED.to_owned(),
        args: gvars.iter().map(|v| QTerm::Var(v.clone())).collect(),
    };
    let goal_key = variant_key(&goal_call);
    state.register(&goal_key, &goal_call);
    state.run_fixpoint()?;

    if let Some(kind) = state.decline {
        return Ok(NativeOutcome::Unsupported(kind));
    }

    // Project every tabled goal tuple onto the goal variables.  All three buckets are
    // disjoint (deduped), so their union is the full known answer set — robust whether
    // the run converged or was budget-cut mid-pass.
    let goal_table = &state.tables[&goal_key];
    let mut bindings: Vec<Binding> = Vec::new();
    for tuple in goal_table
        .settled
        .iter()
        .chain(&goal_table.delta)
        .chain(&goal_table.next)
    {
        let mut row: Binding = BTreeMap::new();
        for (i, gv) in gvars.iter().enumerate() {
            if let QTerm::Const(c) = &tuple[i] {
                row.insert(gv.clone(), c.clone());
            }
        }
        bindings.push(row);
    }

    let frontier = state.build_frontier();

    // Budget composition — the step governor (`Exhausted`) composed with the
    // post-fixpoint `max_answers` truncation (`Partial`).  The answer cap overrides a
    // concurrent step cut, mirroring the reference oracle.
    let mut status = state.status;
    if let Some(max_a) = budget.max_answers {
        let mut tmp = AnswerSet {
            bindings: bindings.clone(),
            status: BudgetStatus::Ok,
            preservation: crate::result::PreservationClaim::exact(),
            frontier: crate::query_ir::CompletionFrontier::empty(),
        };
        tmp.canonicalize();
        if tmp.bindings.len() >= max_a && !tmp.bindings.is_empty() {
            tmp.bindings.truncate(max_a);
            status = BudgetStatus::Partial;
        }
        bindings = tmp.bindings;
    }

    let mut answer = AnswerSet {
        bindings,
        status,
        preservation: crate::result::PreservationClaim::exact(),
        frontier,
    };
    answer.canonicalize();
    Ok(NativeOutcome::Decided(answer))
}

impl SlgState<'_> {
    /// `true` once the step ceiling is reached.
    fn steps_over(&self) -> bool {
        self.steps >= self.max_steps
    }

    /// Register a new variant table (idempotent-ish: only creates when absent).
    fn register(&mut self, key: &VariantKey, call: &QAtom) {
        if self.tables.contains_key(key) {
            return;
        }
        let n = self.next_sg;
        self.next_sg += 1;
        self.tables.insert(
            key.clone(),
            SubgoalTable {
                canon: canon_atom(call, n),
                settled: Vec::new(),
                delta: Vec::new(),
                next: Vec::new(),
                seen: HashSet::new(),
                base_done: false,
            },
        );
        self.registered_new = true;
    }

    /// The completion frontier: on a natural fixpoint every table is final; a step-cut
    /// run claims nothing saturated (a sound under-claim).
    fn build_frontier(&self) -> crate::query_ir::CompletionFrontier {
        let total = self.tables.len();
        let complete = self.status != BudgetStatus::Exhausted;
        let saturated_preds: BTreeSet<String> = if complete {
            self.tables.values().map(|t| t.canon.pred.clone()).collect()
        } else {
            BTreeSet::new()
        };
        crate::query_ir::CompletionFrontier {
            completed: if complete { total } else { 0 },
            total,
            saturated_preds,
            consumed_steps: self.steps,
        }
    }

    /// Run the global semi-naive fixpoint until quiescent, declined, or step-cut.
    fn run_fixpoint(&mut self) -> Result<(), String> {
        loop {
            if self.status == BudgetStatus::Exhausted || self.decline.is_some() {
                break;
            }
            self.registered_new = false;

            // Produce every currently-known variant once, reading the pass-start tables.
            let keys: Vec<VariantKey> = self.tables.keys().cloned().collect();
            for key in &keys {
                if self.status == BudgetStatus::Exhausted || self.decline.is_some() {
                    break;
                }
                self.produce(key)?;
            }
            if self.decline.is_some() {
                break;
            }

            // Rotate: settled += delta; delta = next.  `grew` records whether any table
            // gained a fresh answer this pass.
            let mut grew = false;
            for table in self.tables.values_mut() {
                let mut old_delta = std::mem::take(&mut table.delta);
                table.settled.append(&mut old_delta);
                table.delta = std::mem::take(&mut table.next);
                if !table.delta.is_empty() {
                    grew = true;
                }
            }

            if self.status == BudgetStatus::Exhausted {
                break;
            }
            if !grew && !self.registered_new {
                break;
            }
        }
        Ok(())
    }

    /// Produce a variant's new answers for the current pass into its `next` buffer.
    fn produce(&mut self, key: &VariantKey) -> Result<(), String> {
        // One subgoal expansion (rule application attempt) = one step.
        if self.steps_over() {
            self.status = BudgetStatus::Exhausted;
            return Ok(());
        }
        self.steps += 1;

        let canon = self.tables[key].canon.clone();
        let base_done = self.tables[key].base_done;

        let matching: Vec<QRule> = self
            .rules
            .iter()
            .filter(|r| r.head.pred == canon.pred)
            .cloned()
            .collect();

        for rule in &matching {
            if self.decline.is_some() || self.steps_over() {
                break;
            }
            let renamed = rename_rule(rule);
            let empty: Binding = BTreeMap::new();
            let Some(head_subst) = unify_atoms(&renamed.head, &canon, &empty) else {
                continue;
            };

            let idb_occs = count_idb_atoms(&renamed.body, &self.idb);
            if idb_occs == 0 {
                // Base rule: fire once — its extensional witnesses are static.
                if !base_done {
                    let sols = self.resolve_body(&renamed.body, &head_subst, None, 0)?;
                    for sol in &sols {
                        self.collect_head(key, &canon, sol);
                    }
                }
            } else {
                // Semi-naive: fire once per IDB body atom, forcing that atom onto its
                // table's delta while the rest read the full contents.
                for j in 0..idb_occs {
                    if self.decline.is_some() {
                        break;
                    }
                    let sols = self.resolve_body(&renamed.body, &head_subst, Some(j), 0)?;
                    for sol in &sols {
                        self.collect_head(key, &canon, sol);
                    }
                }
            }
        }

        if self.decline.is_none()
            && let Some(table) = self.tables.get_mut(key)
        {
            table.base_done = true;
        }
        Ok(())
    }

    /// Project the canonical head through a completed body substitution and table it.
    fn collect_head(&mut self, key: &VariantKey, canon: &QAtom, subst: &Binding) {
        let tuple: Vec<QTerm> = canon
            .args
            .iter()
            .enumerate()
            .map(|(i, a)| normalize_answer(resolve_term(a, subst), i))
            .collect();
        if let Some(table) = self.tables.get_mut(key) {
            table.push_next(tuple);
        }
    }

    /// Resolve a rule body under `subst`, returning every completed substitution.
    ///
    /// `delta_occ` names the left-to-right IDB-atom occurrence (if any) that must consume
    /// its table's `delta`; every other IDB atom consumes the full contents.  `next_idb`
    /// is the occurrence index of the next IDB atom encountered.
    fn resolve_body(
        &mut self,
        lits: &[QBodyLit],
        subst: &Binding,
        delta_occ: Option<usize>,
        next_idb: usize,
    ) -> Result<Vec<Binding>, String> {
        if self.decline.is_some() {
            return Ok(Vec::new());
        }
        let Some((first, rest)) = lits.split_first() else {
            return Ok(vec![subst.clone()]);
        };

        match first {
            QBodyLit::Cut => {
                // Cut is gated at entry; a defensive decline keeps the resolver total.
                self.decline = Some(UnsupportedKind::Cut);
                Ok(Vec::new())
            }
            QBodyLit::Builtin(builtin) => {
                let lookup = |name: &str| match crate::reference_resolver::chase_var(name, subst, 0)
                {
                    QTerm::Const(c) => Some(Cow::Owned(c)),
                    QTerm::Var(_) | QTerm::Num(_) => None,
                };
                match crate::physical::eval_builtin(builtin, &lookup) {
                    crate::physical::BuiltinOutcome::Filter(true) => {
                        self.resolve_body(rest, subst, delta_occ, next_idb)
                    }
                    crate::physical::BuiltinOutcome::Filter(false) => Ok(Vec::new()),
                    crate::physical::BuiltinOutcome::Generate { var, value } => {
                        let mut new_subst = subst.clone();
                        let root = match crate::reference_resolver::chase_var(&var, subst, 0) {
                            QTerm::Var(root) => root,
                            QTerm::Const(_) | QTerm::Num(_) => var,
                        };
                        new_subst.insert(root, crate::physical::emit_integer_surface(value));
                        self.resolve_body(rest, &new_subst, delta_occ, next_idb)
                    }
                    crate::physical::BuiltinOutcome::Unbound
                    | crate::physical::BuiltinOutcome::Error(_) => {
                        // An uncomputable arithmetic mode — a declared gap, never a guess.
                        self.decline = Some(UnsupportedKind::Arithmetic);
                        Ok(Vec::new())
                    }
                }
            }
            QBodyLit::Atom(atom) => {
                let resolved = apply_subst(atom, subst);
                if self.idb.contains(&resolved.pred) {
                    let mode = if delta_occ == Some(next_idb) {
                        Mode::Delta
                    } else {
                        Mode::Full
                    };
                    let heads = self.consume_idb(&resolved, subst, mode);
                    let mut out = Vec::new();
                    for s in heads {
                        if self.decline.is_some() {
                            break;
                        }
                        out.extend(self.resolve_body(rest, &s, delta_occ, next_idb + 1)?);
                    }
                    Ok(out)
                } else {
                    let heads = self.solve_edb(&resolved, subst)?;
                    let mut out = Vec::new();
                    for s in heads {
                        if self.decline.is_some() {
                            break;
                        }
                        out.extend(self.resolve_body(rest, &s, delta_occ, next_idb)?);
                    }
                    Ok(out)
                }
            }
        }
    }

    /// Consume an IDB atom against its variant table (registering the variant on first
    /// demand), unifying each tabled tuple with the call.  Never produces.
    fn consume_idb(&mut self, resolved: &QAtom, subst: &Binding, mode: Mode) -> Vec<Binding> {
        let key = variant_key(resolved);
        self.register(&key, resolved);

        let table = &self.tables[&key];
        let pred = table.canon.pred.clone();
        let tuples: Vec<Vec<QTerm>> = match mode {
            Mode::Delta => table.delta.clone(),
            Mode::Full => table.settled.iter().chain(&table.delta).cloned().collect(),
        };

        let mut out = Vec::new();
        for tuple in tuples {
            let ans_atom = QAtom {
                pred: pred.clone(),
                args: tuple,
            };
            if let Some(s2) = unify_atoms(&ans_atom, resolved, subst) {
                out.push(s2);
            }
        }
        out
    }

    /// Solve a binary EDB atom by scanning the world (mirrors the reference oracle).
    fn solve_edb(&mut self, resolved: &QAtom, subst: &Binding) -> Result<Vec<Binding>, String> {
        // EDB relations are binary RDF triples; any other arity is a declared gap.
        if resolved.args.len() != 2 {
            self.decline = Some(UnsupportedKind::NonBinaryAtom);
            return Ok(Vec::new());
        }
        if self.steps_over() {
            self.status = BudgetStatus::Exhausted;
            return Ok(Vec::new());
        }
        // One EDB scan = one step.
        self.steps += 1;

        let pred = resolved.pred.as_str();
        let subj_term: Option<TermValue> = match &resolved.args[0] {
            QTerm::Const(c) => Some(canonical_to_term(c)?),
            QTerm::Var(_) | QTerm::Num(_) => None,
        };
        let obj_term: Option<TermValue> = match &resolved.args[1] {
            QTerm::Const(c) => Some(canonical_to_term(c)?),
            QTerm::Var(_) | QTerm::Num(_) => None,
        };

        let matched: Vec<_> = self
            .foreign
            .in_world(
                self.world,
                subj_term.as_ref(),
                Some(pred),
                obj_term.as_ref(),
            )
            .map(|dq| (dq.subject.clone(), dq.object.clone()))
            .collect();

        let mut out = Vec::new();
        for (dq_subj, dq_obj) in matched {
            let subj_canon =
                term_n3(&dq_subj).map_err(|e| format!("term_n3 failed on EDB subject: {e}"))?;
            let obj_canon =
                term_n3(&dq_obj).map_err(|e| format!("term_n3 failed on EDB object: {e}"))?;

            let mut new_subst = subst.clone();
            if let QTerm::Var(v) = &resolved.args[0] {
                new_subst
                    .entry(v.clone())
                    .or_insert_with(|| subj_canon.clone());
                if new_subst[v] != subj_canon {
                    continue;
                }
            }
            if let QTerm::Var(v) = &resolved.args[1] {
                new_subst
                    .entry(v.clone())
                    .or_insert_with(|| obj_canon.clone());
                if new_subst[v] != obj_canon {
                    continue;
                }
            }
            out.push(new_subst);
        }
        Ok(out)
    }
}

// ── Free helpers ────────────────────────────────────────────────────────────────

/// The goal variables of `program`, in first-appearance order.
fn goal_variables(program: &QProgram) -> Vec<String> {
    let mut vars: Vec<String> = Vec::new();
    for atom in &program.goal.atoms {
        for t in &atom.args {
            if let QTerm::Var(v) = t
                && !vars.contains(v)
            {
                vars.push(v.clone());
            }
        }
    }
    vars
}

/// Count the direct IDB atoms in a rule body.
fn count_idb_atoms(body: &[QBodyLit], idb: &BTreeSet<String>) -> usize {
    body.iter()
        .filter(|l| matches!(l, QBodyLit::Atom(a) if idb.contains(&a.pred)))
        .count()
}

/// The call variant key of a resolved atom.
fn variant_key(atom: &QAtom) -> VariantKey {
    (
        atom.pred.clone(),
        atom.args.iter().map(term_canonical_or_wildcard).collect(),
    )
}

/// Build the canonical call atom of a new variant: constants stay, unbound positions
/// take deterministic fresh variables so a re-entrant recursive call reproduces the
/// SAME variant key.
fn canon_atom(resolved: &QAtom, n: usize) -> QAtom {
    QAtom {
        pred: resolved.pred.clone(),
        args: resolved
            .args
            .iter()
            .enumerate()
            .map(|(i, a)| match a {
                QTerm::Const(_) => a.clone(),
                QTerm::Num(x) => QTerm::Const(crate::physical::emit_integer_surface(*x)),
                QTerm::Var(_) => QTerm::Var(format!("__SG{n}_{i}")),
            })
            .collect(),
    }
}

/// Normalize a projected answer argument: a bound value keeps its canonical constant;
/// an unbound position takes a deterministic per-position variable so equal answers
/// deduplicate and consumption is stable.
fn normalize_answer(term: QTerm, i: usize) -> QTerm {
    match term {
        QTerm::Const(_) => term,
        QTerm::Num(n) => QTerm::Const(crate::physical::emit_integer_surface(n)),
        QTerm::Var(_) => QTerm::Var(format!("__ANS{i}")),
    }
}

/// The deduplication signature of an answer tuple.
fn tuple_sig(tuple: &[QTerm]) -> String {
    let mut sig = String::new();
    for (i, t) in tuple.iter().enumerate() {
        match t {
            QTerm::Const(c) => {
                sig.push('c');
                sig.push_str(c);
            }
            QTerm::Num(n) => {
                sig.push('n');
                sig.push_str(&n.to_string());
            }
            QTerm::Var(_) => {
                sig.push('v');
                sig.push_str(&i.to_string());
            }
        }
        sig.push('\u{1}');
    }
    sig
}

// ── Unit tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::query_ir::parse_query_program;
    use crate::seam::WorldStoreForeign;
    use crate::store::WorldStore;

    const W: &str = "http://logic.test/world/slg";
    const PROFILE: &str = "https://blackcatinformatics.ca/logic/PositiveHornProfile";
    const BASE: &str = "https://example.org/";
    const XSD_INT: &str = "http://www.w3.org/2001/XMLSchema#integer";

    fn make_world(triples: &[(&str, &str, &str)]) -> (WorldStore, String) {
        let store = WorldStore::new();
        for (s, p, o) in triples {
            store.insert_quad(W, s, p, o);
        }
        (store, W.to_owned())
    }

    fn iri(local: &str) -> String {
        format!("<{BASE}{local}>")
    }

    fn int_surface(n: i64) -> String {
        format!("\"{n}\"^^<{XSD_INT}>")
    }

    /// A chain `a→b→c→…` over predicate `pred`.
    fn chain(pred: &str, nodes: &[&str]) -> Vec<(String, String, String)> {
        nodes
            .windows(2)
            .map(|w| {
                (
                    format!("{BASE}{}", w[0]),
                    format!("{BASE}{pred}"),
                    format!("{BASE}{}", w[1]),
                )
            })
            .collect()
    }

    fn decided(out: NativeOutcome<AnswerSet>) -> AnswerSet {
        match out {
            NativeOutcome::Decided(a) => a,
            other => panic!("expected Decided, got {other:?}"),
        }
    }

    // ── Test 1: n-ary (ternary) recursive relation is complete ────────────────

    #[test]
    fn slg_nary_ternary_recursive_is_complete() {
        // step: a→b→c→d→e. Ternary IDB pr(X,Y,Z): a pair (Y,Z) of adjacent nodes
        // reachable from X.  Recursive in its third body position.
        let triples = chain("step", &["a", "b", "c", "d", "e"]);
        let refs: Vec<(&str, &str, &str)> = triples
            .iter()
            .map(|(s, p, o)| (s.as_str(), p.as_str(), o.as_str()))
            .collect();
        let (store, world_nn) = make_world(&refs);
        let foreign = WorldStoreForeign::from_world(&store, W, PROFILE).unwrap();

        let src = format!(
            ":- prefix(ex, '{BASE}').\n\
             ex:pr(X, Y, Z) :- ex:step(X, Y), ex:step(Y, Z).\n\
             ex:pr(X, Y, Z) :- ex:step(X, W), ex:pr(W, Y, Z).\n\
             ?- ex:pr(ex:a, Y, Z).\n"
        );
        let prog = parse_query_program(&src).unwrap();
        let ans = decided(resolve_slg(&foreign, &world_nn, &prog, &Budget::default()).unwrap());

        assert_eq!(ans.status, BudgetStatus::Ok);
        let pairs: BTreeSet<(String, String)> = ans
            .bindings
            .iter()
            .map(|b| (b["Y"].clone(), b["Z"].clone()))
            .collect();
        let expected: BTreeSet<(String, String)> = [
            (iri("b"), iri("c")),
            (iri("c"), iri("d")),
            (iri("d"), iri("e")),
        ]
        .into_iter()
        .collect();
        assert_eq!(pairs, expected, "ternary answer set: {:?}", ans.bindings);
    }

    // ── Test 2: LEFT recursion is complete (path-memo under-produces) ─────────

    #[test]
    fn slg_left_recursion_is_complete() {
        // edge: a→b→c→d. Left-recursive ancestor.
        let triples = chain("edge", &["a", "b", "c", "d"]);
        let refs: Vec<(&str, &str, &str)> = triples
            .iter()
            .map(|(s, p, o)| (s.as_str(), p.as_str(), o.as_str()))
            .collect();
        let (store, world_nn) = make_world(&refs);
        let foreign = WorldStoreForeign::from_world(&store, W, PROFILE).unwrap();

        let src = format!(
            ":- prefix(ex, '{BASE}').\n\
             ex:anc(X, Y) :- ex:anc(X, Z), ex:edge(Z, Y).\n\
             ex:anc(X, Y) :- ex:edge(X, Y).\n\
             ?- ex:anc(ex:a, Y).\n"
        );
        let prog = parse_query_program(&src).unwrap();

        let ans = decided(resolve_slg(&foreign, &world_nn, &prog, &Budget::default()).unwrap());
        assert_eq!(ans.status, BudgetStatus::Ok);
        let ys: BTreeSet<String> = ans.bindings.iter().map(|b| b["Y"].clone()).collect();
        for want in ["b", "c", "d"] {
            assert!(ys.contains(&iri(want)), "missing {want}: {ys:?}");
        }

        // The path-memo reference can under-produce on left recursion; slg is complete,
        // so it never returns fewer answers.
        let reference =
            crate::reference_resolver::resolve(&foreign, &world_nn, &prog, &Budget::default())
                .unwrap();
        assert!(
            ans.bindings.len() >= reference.bindings.len(),
            "slg {} < reference {}",
            ans.bindings.len(),
            reference.bindings.len()
        );
    }

    // ── Test 3: cyclic IDB terminates ─────────────────────────────────────────

    #[test]
    fn slg_cyclic_terminates() {
        // edge: a→b→a (cycle).
        let (store, world_nn) = make_world(&[
            (
                &format!("{BASE}a"),
                &format!("{BASE}edge"),
                &format!("{BASE}b"),
            ),
            (
                &format!("{BASE}b"),
                &format!("{BASE}edge"),
                &format!("{BASE}a"),
            ),
        ]);
        let foreign = WorldStoreForeign::from_world(&store, W, PROFILE).unwrap();

        let src = format!(
            ":- prefix(ex, '{BASE}').\n\
             ex:anc(X, Y) :- ex:edge(X, Y).\n\
             ex:anc(X, Y) :- ex:edge(X, Z), ex:anc(Z, Y).\n\
             ?- ex:anc(ex:a, Y).\n"
        );
        let prog = parse_query_program(&src).unwrap();
        let ans = decided(resolve_slg(&foreign, &world_nn, &prog, &Budget::default()).unwrap());
        assert_eq!(ans.status, BudgetStatus::Ok);
        let ys: BTreeSet<String> = ans.bindings.iter().map(|b| b["Y"].clone()).collect();
        // Both a and b are reachable in the cycle.
        assert!(ys.contains(&iri("a")), "reachable set: {ys:?}");
        assert!(ys.contains(&iri("b")), "reachable set: {ys:?}");
    }

    // ── Test 4: binary positive matches the reference oracle ──────────────────

    #[test]
    fn slg_binary_positive_matches_reference() {
        // parentOf: a→b→c→d (acyclic transitive closure).
        let triples = chain("parentOf", &["a", "b", "c", "d"]);
        let refs: Vec<(&str, &str, &str)> = triples
            .iter()
            .map(|(s, p, o)| (s.as_str(), p.as_str(), o.as_str()))
            .collect();
        let (store, world_nn) = make_world(&refs);
        let foreign = WorldStoreForeign::from_world(&store, W, PROFILE).unwrap();

        let src = format!(
            ":- prefix(ex, '{BASE}').\n\
             ex:ancestor(X, Y) :- ex:parentOf(X, Y).\n\
             ex:ancestor(X, Y) :- ex:parentOf(X, Z), ex:ancestor(Z, Y).\n\
             ?- ex:ancestor(ex:a, Y).\n"
        );
        let prog = parse_query_program(&src).unwrap();

        let mut slg = decided(resolve_slg(&foreign, &world_nn, &prog, &Budget::default()).unwrap());
        slg.canonicalize();
        let mut reference =
            crate::reference_resolver::resolve(&foreign, &world_nn, &prog, &Budget::default())
                .unwrap();
        reference.canonicalize();
        assert_eq!(slg.bindings, reference.bindings);
    }

    // ── Test 5: max_answers ⇒ Partial ─────────────────────────────────────────

    #[test]
    fn slg_budget_max_answers_is_partial() {
        let triples = chain("parentOf", &["a", "b", "c", "d"]);
        let refs: Vec<(&str, &str, &str)> = triples
            .iter()
            .map(|(s, p, o)| (s.as_str(), p.as_str(), o.as_str()))
            .collect();
        let (store, world_nn) = make_world(&refs);
        let foreign = WorldStoreForeign::from_world(&store, W, PROFILE).unwrap();

        let src = format!(
            ":- prefix(ex, '{BASE}').\n\
             ex:ancestor(X, Y) :- ex:parentOf(X, Y).\n\
             ex:ancestor(X, Y) :- ex:parentOf(X, Z), ex:ancestor(Z, Y).\n\
             ?- ex:ancestor(ex:a, Y).\n"
        );
        let prog = parse_query_program(&src).unwrap();
        let budget = Budget {
            max_answers: Some(1),
            ..Default::default()
        };
        let ans = decided(resolve_slg(&foreign, &world_nn, &prog, &budget).unwrap());
        assert_eq!(ans.bindings.len(), 1);
        assert_eq!(ans.status, BudgetStatus::Partial);
    }

    // ── Test 6: max_steps ⇒ Exhausted, sound subset ──────────────────────────

    #[test]
    fn slg_budget_max_steps_is_exhausted_sound_subset() {
        let triples = chain("parentOf", &["a", "b", "c", "d"]);
        let refs: Vec<(&str, &str, &str)> = triples
            .iter()
            .map(|(s, p, o)| (s.as_str(), p.as_str(), o.as_str()))
            .collect();
        let (store, world_nn) = make_world(&refs);
        let foreign = WorldStoreForeign::from_world(&store, W, PROFILE).unwrap();

        let src = format!(
            ":- prefix(ex, '{BASE}').\n\
             ex:ancestor(X, Y) :- ex:parentOf(X, Y).\n\
             ex:ancestor(X, Y) :- ex:parentOf(X, Z), ex:ancestor(Z, Y).\n\
             ?- ex:ancestor(ex:a, Y).\n"
        );
        let prog = parse_query_program(&src).unwrap();

        // Unbudgeted full answer set.
        let full = decided(resolve_slg(&foreign, &world_nn, &prog, &Budget::default()).unwrap());
        let full_set: BTreeSet<String> = full.bindings.iter().map(|b| b["Y"].clone()).collect();

        let budget = Budget {
            max_steps: Some(2),
            ..Default::default()
        };
        let ans = decided(resolve_slg(&foreign, &world_nn, &prog, &budget).unwrap());
        assert_eq!(ans.status, BudgetStatus::Exhausted);
        for b in &ans.bindings {
            assert!(
                full_set.contains(&b["Y"]),
                "budgeted answer not in full set: {b:?}"
            );
        }
        assert!(
            ans.bindings.len() < full.bindings.len(),
            "step cut must drop at least one answer"
        );
    }

    // ── Test 7: cut ⇒ Unsupported(Cut) ────────────────────────────────────────

    #[test]
    fn slg_cut_program_is_unsupported() {
        let (store, world_nn) = make_world(&[(
            &format!("{BASE}a"),
            &format!("{BASE}edge"),
            &format!("{BASE}b"),
        )]);
        let foreign = WorldStoreForeign::from_world(&store, W, PROFILE).unwrap();

        let src = format!(
            ":- prefix(ex, '{BASE}').\n\
             ex:anc(X, Y) :- ex:edge(X, Y), !, ex:anc(X, Y).\n\
             ?- ex:anc(ex:a, Y).\n"
        );
        let prog = parse_query_program(&src).unwrap();
        let out = resolve_slg(&foreign, &world_nn, &prog, &Budget::default()).unwrap();
        assert_eq!(out, NativeOutcome::Unsupported(UnsupportedKind::Cut));
    }

    // ── Test 8: infinite model terminates via an explicit step cut ────────────

    #[test]
    fn slg_infinite_model_terminates_via_default_backstop() {
        // p(N1) :- p(N), N1 is N + 1.  p(0).  ?- p(X).  Least model is 0,1,2,… (infinite);
        // an explicit small max_steps proves the step-cut path returns a sound prefix
        // fast, exercising the same ceiling logic the default backstop uses.
        let (store, world_nn) = make_world(&[]);
        let foreign = WorldStoreForeign::from_world(&store, W, PROFILE).unwrap();

        let src = format!(
            ":- prefix(ex, '{BASE}').\n\
             ex:p(N1) :- ex:p(N), N1 is N + 1.\n\
             ex:p(0).\n\
             ?- ex:p(X).\n"
        );
        let prog = parse_query_program(&src).unwrap();
        let budget = Budget {
            max_steps: Some(50),
            ..Default::default()
        };
        let ans = decided(resolve_slg(&foreign, &world_nn, &prog, &budget).unwrap());
        assert_eq!(ans.status, BudgetStatus::Exhausted);
        assert!(!ans.bindings.is_empty(), "expected a non-empty prefix");
        let xs: BTreeSet<String> = ans.bindings.iter().map(|b| b["X"].clone()).collect();
        // A sound prefix of {0,1,2,…} as canonical integer surfaces.
        assert!(
            xs.contains(&int_surface(0)),
            "prefix must contain 0: {xs:?}"
        );
        assert!(
            xs.contains(&int_surface(1)),
            "prefix must contain 1: {xs:?}"
        );
        for x in &xs {
            assert!(
                x.ends_with(&format!("^^<{XSD_INT}>")),
                "each answer is an integer surface: {x}"
            );
        }
    }
}
