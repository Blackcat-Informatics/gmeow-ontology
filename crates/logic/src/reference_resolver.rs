// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Declarative SLD/Datalog reference oracle for `.logic` query programs.
//!
//! # Design
//!
//! This module implements a pure, declarative backward-chaining resolver for
//! [`QProgram`] queries over a [`ScryerForeign`] world.  It is deliberately
//! **non-procedural**: cut (`!`) is detected and rejected with a clear error
//! rather than being silently ignored or executed.
//!
//! ## IDB vs EDB
//!
//! A predicate is **IDB** (intensional) if it is the head predicate of at least
//! one rule in the program.  All other predicates are **EDB** (extensional) and
//! are looked up directly in the materialized world via `ScryerForeign::in_world`.
//!
//! ## Termination guarantee
//!
//! The resolver maintains a **seen-goal memo table** keyed on
//! `(predicate_iri, bound_subject_canonical, bound_object_canonical)`.  Before
//! recursing into an IDB goal the resolver checks whether an identical call is
//! already on the stack; if so, no new answers can be produced by further
//! recursion, so the call returns immediately with no new bindings.  This
//! guarantees termination for all cyclic/recursive IDB programs.
//!
//! ## Cut
//!
//! The oracle is declarative.  If resolution would use a rule whose body
//! contains `QBodyLit::Cut`, it returns
//! `Err("cut is procedural; not supported by the declarative reference oracle ...")`.
//! Cut-containing programs must use the Scryer engine instead.
//!
//! ## Budget
//!
//! - `max_answers`: stop (status `Partial`) once this many answer bindings are
//!   collected.
//! - `max_steps`: stop (status `Exhausted`) once this many resolution steps
//!   (rule expansions + EDB lookups) are counted.

use std::collections::{BTreeMap, BTreeSet};

use oxigraph::model::{NamedNode, Term};

use crate::provenance::term_n3;
use crate::query_ir::{AnswerSet, Binding, Budget, QAtom, QBodyLit, QGoal, QProgram, QTerm};
use crate::seam::{BudgetStatus, ScryerForeign};

// ── Public entry point ────────────────────────────────────────────────────────

/// Resolve `program` against `world` using the declarative SLD oracle.
///
/// # Arguments
///
/// - `foreign` — the blackboard access layer (implements [`ScryerForeign`]).
/// - `world`   — the named-graph IRI identifying the world to query.
/// - `program` — the parsed `.logic` program (rules + goal).
/// - `budget`  — execution limits; `Budget::default()` is unlimited.
///
/// # Returns
///
/// An [`AnswerSet`] with all goal-variable bindings in sorted order plus a
/// [`BudgetStatus`] indicating whether resolution completed or was cut short.
///
/// # Errors
///
/// Returns `Err(String)` if:
/// - A rule body that would be used contains `!` (cut is procedural).
/// - `term_n3` fails on an RDF-star triple-term (out of scope).
pub fn resolve(
    foreign: &dyn ScryerForeign,
    world: &NamedNode,
    program: &QProgram,
    budget: &Budget,
) -> Result<AnswerSet, String> {
    // Build the IDB set: predicate IRIs that appear as rule heads.
    let idb: BTreeSet<String> = program.rules.iter().map(|r| r.head.pred.clone()).collect();

    let mut state = ResolveState {
        foreign,
        world,
        program,
        idb,
        budget,
        answers: Vec::new(),
        steps: 0,
        status: BudgetStatus::Ok,
    };

    // Resolve the conjunctive goal with an empty initial substitution.
    let initial_subst: Binding = BTreeMap::new();
    // Seen-goal memo: (pred, subject_canonical, object_canonical)
    let mut seen: BTreeSet<(String, String, String)> = BTreeSet::new();

    state.resolve_conjunct(&program.goal.atoms, &initial_subst, &mut seen)?;

    let mut answer_set = AnswerSet {
        bindings: state.answers,
        status: state.status,
        preservation: crate::result::PreservationClaim::exact(),
    };
    answer_set.canonicalize();
    Ok(answer_set)
}

// ── Internal state ────────────────────────────────────────────────────────────

struct ResolveState<'a> {
    foreign: &'a dyn ScryerForeign,
    world: &'a NamedNode,
    program: &'a QProgram,
    idb: BTreeSet<String>,
    budget: &'a Budget,
    answers: Vec<Binding>,
    steps: u64,
    status: BudgetStatus,
}

impl<'a> ResolveState<'a> {
    /// Check budget; return `true` if resolution should stop.
    fn budget_exceeded(&self) -> bool {
        if let Some(max_a) = self.budget.max_answers {
            if self.answers.len() >= max_a {
                return true;
            }
        }
        if let Some(max_s) = self.budget.max_steps {
            if self.steps >= max_s {
                return true;
            }
        }
        false
    }

    /// Resolve a conjunction of atoms with the given substitution.
    ///
    /// Collects all answer bindings into `self.answers`.
    fn resolve_conjunct(
        &mut self,
        atoms: &[QAtom],
        subst: &Binding,
        seen: &mut BTreeSet<(String, String, String)>,
    ) -> Result<(), String> {
        if self.budget_exceeded() {
            self.status = if self.budget.max_answers.is_some()
                && self.answers.len() >= self.budget.max_answers.unwrap()
            {
                BudgetStatus::Partial
            } else {
                BudgetStatus::Exhausted
            };
            return Ok(());
        }

        if atoms.is_empty() {
            // All atoms satisfied — record the answer binding.
            // Extract only the variables that appear in the goal.
            let goal_vars = goal_vars(&self.program.goal);
            let answer: Binding = goal_vars
                .into_iter()
                .filter_map(|v| {
                    // Chase through the substitution (including aliases) to get the
                    // final bound value for each goal variable.
                    match chase_var(v.as_str(), subst, 0) {
                        QTerm::Const(c) => Some((v, c)),
                        // The oracle never binds a goal variable to a bare number
                        // (it rejects builtin programs before resolution); omit.
                        QTerm::Var(_) | QTerm::Num(_) => None,
                    }
                })
                .collect();
            self.answers.push(answer);
            // Check if we just hit max_answers; mark Partial so the caller sees it.
            if let Some(max_a) = self.budget.max_answers {
                if self.answers.len() >= max_a {
                    self.status = BudgetStatus::Partial;
                }
            }
            return Ok(());
        }

        let (first, rest) = atoms.split_first().unwrap();

        // Apply the current substitution to the first atom.
        let applied = apply_subst(first, subst);

        if self.idb.contains(&applied.pred) {
            // IDB predicate: expand matching rules.
            self.resolve_idb(&applied, rest, subst, seen)?;
        } else {
            // EDB predicate: look up in the world.
            self.resolve_edb(&applied, rest, subst, seen)?;
        }

        Ok(())
    }

    /// Resolve an IDB atom by expanding matching rules.
    fn resolve_idb(
        &mut self,
        atom: &QAtom,
        rest: &[QAtom],
        subst: &Binding,
        seen: &mut BTreeSet<(String, String, String)>,
    ) -> Result<(), String> {
        // Memo key: (pred, arg0_canonical, arg1_canonical) — Consts are already canonical;
        // unbound Vars are represented as the empty string (wildcard).
        let key = (
            atom.pred.clone(),
            term_canonical_or_wildcard(&atom.args[0]),
            term_canonical_or_wildcard(&atom.args[1]),
        );

        if seen.contains(&key) {
            // Already being resolved on this path — cut the recursion to prevent
            // infinite loops on cyclic IDB.
            return Ok(());
        }
        seen.insert(key.clone());

        self.steps += 1;
        if let Some(max_s) = self.budget.max_steps {
            if self.steps > max_s {
                self.status = BudgetStatus::Exhausted;
                seen.remove(&key);
                return Ok(());
            }
        }

        // Collect matching rules (borrow the program, not self).
        let matching: Vec<_> = self
            .program
            .rules
            .iter()
            .filter(|r| r.head.pred == atom.pred)
            .cloned()
            .collect();

        for rule in &matching {
            if self.budget_exceeded() {
                break;
            }

            // Detect cut in the rule body — reject immediately.
            if rule.body.iter().any(|b| matches!(b, QBodyLit::Cut)) {
                return Err(
                    "cut is procedural; not supported by the declarative reference oracle \
                     (use the Scryer engine)"
                        .to_owned(),
                );
            }

            // Detect an arithmetic/comparison builtin — reject immediately, the same
            // way cut is rejected: the declarative oracle has no arithmetic engine, so
            // Scryer is the sole evaluator for builtin programs (#1009 G2a).
            if rule.body.iter().any(|b| matches!(b, QBodyLit::Builtin(_))) {
                return Err(
                    "arithmetic/comparison builtins are not supported by the declarative \
                     reference oracle (use the Scryer engine)"
                        .to_owned(),
                );
            }

            // Try to unify the rule head with `atom`.
            // Rename rule variables to avoid collisions with the current substitution.
            let renamed_rule = rename_rule(rule);

            if let Some(new_subst) = unify_atoms(&renamed_rule.head, atom, subst) {
                // Build the new body goal: renamed rule body ++ rest.
                let body_atoms: Vec<QAtom> = renamed_rule
                    .body
                    .iter()
                    .filter_map(|b| match b {
                        QBodyLit::Atom(a) => Some(a.clone()),
                        QBodyLit::Cut | QBodyLit::Builtin(_) => None, // already rejected above
                    })
                    .collect();

                let mut combined = body_atoms;
                combined.extend_from_slice(rest);

                self.resolve_conjunct(&combined, &new_subst, seen)?;
            }
        }

        seen.remove(&key);
        Ok(())
    }

    /// Resolve an EDB atom by querying `foreign.in_world`.
    fn resolve_edb(
        &mut self,
        atom: &QAtom,
        rest: &[QAtom],
        subst: &Binding,
        seen: &mut BTreeSet<(String, String, String)>,
    ) -> Result<(), String> {
        self.steps += 1;
        if let Some(max_s) = self.budget.max_steps {
            if self.steps > max_s {
                self.status = BudgetStatus::Exhausted;
                return Ok(());
            }
        }

        // Build NamedNode for the predicate IRI.
        let pred_nn = NamedNode::new(&atom.pred)
            .map_err(|e| format!("EDB predicate is not a valid IRI {:?}: {e}", atom.pred))?;

        // Convert bound args to oxigraph Terms for the pattern filter.
        let subj_term: Option<Term> = match &atom.args[0] {
            QTerm::Const(c) => Some(canonical_to_term(c)?),
            // A bare number is never an EDB subject in oracle-resolved programs.
            QTerm::Var(_) | QTerm::Num(_) => None,
        };
        let obj_term: Option<Term> = match &atom.args[1] {
            QTerm::Const(c) => Some(canonical_to_term(c)?),
            QTerm::Var(_) | QTerm::Num(_) => None,
        };

        // Collect matching DerivedQuads into a Vec to release the iterator borrow.
        let matched: Vec<_> = self
            .foreign
            .in_world(
                self.world,
                subj_term.as_ref(),
                Some(&pred_nn),
                obj_term.as_ref(),
            )
            .map(|dq| (dq.subject.clone(), dq.object.clone()))
            .collect();

        for (dq_subj, dq_obj) in matched {
            if self.budget_exceeded() {
                break;
            }

            // Convert DerivedQuad subject/object to canonical strings.
            let subj_canon =
                term_n3(&dq_subj).map_err(|e| format!("term_n3 failed on EDB subject: {e}"))?;
            let obj_canon =
                term_n3(&dq_obj).map_err(|e| format!("term_n3 failed on EDB object: {e}"))?;

            // Extend substitution with any new variable bindings from this match.
            let mut new_subst = subst.clone();
            if let QTerm::Var(v) = &atom.args[0] {
                // Only bind if not already bound (EDB is called with subst already applied).
                new_subst
                    .entry(v.clone())
                    .or_insert_with(|| subj_canon.clone());
                if new_subst[v] != subj_canon {
                    continue; // existing binding conflicts
                }
            }
            if let QTerm::Var(v) = &atom.args[1] {
                new_subst
                    .entry(v.clone())
                    .or_insert_with(|| obj_canon.clone());
                if new_subst[v] != obj_canon {
                    continue;
                }
            }

            self.resolve_conjunct(rest, &new_subst, seen)?;
        }

        Ok(())
    }
}

// ── Substitution helpers ──────────────────────────────────────────────────────

/// Apply a substitution to an atom, returning a new atom with bound variables replaced.
///
/// Chases `__ALIAS__` sentinels so that aliased variables are fully resolved.
fn apply_subst(atom: &QAtom, subst: &Binding) -> QAtom {
    QAtom {
        pred: atom.pred.clone(),
        args: atom.args.iter().map(|t| resolve_term(t, subst)).collect(),
    }
}

/// Unify a rule head atom with a goal atom under the current substitution.
///
/// Returns a new (extended) substitution on success, `None` on failure.
fn unify_atoms(head: &QAtom, goal: &QAtom, subst: &Binding) -> Option<Binding> {
    if head.pred != goal.pred {
        return None;
    }
    if head.args.len() != goal.args.len() {
        return None;
    }

    let mut new_subst = subst.clone();
    for (h, g) in head.args.iter().zip(goal.args.iter()) {
        let h_val = resolve_term(h, &new_subst);
        let g_val = resolve_term(g, &new_subst);
        match (h_val, g_val) {
            (QTerm::Const(hc), QTerm::Const(gc)) => {
                if hc != gc {
                    return None;
                }
            }
            // Numeric operands are never unified by the oracle (it rejects builtin
            // programs up front). A `Num` only unifies with an identical `Num`; any
            // mismatch — including against a Const or Var — fails unification.
            (QTerm::Num(hn), QTerm::Num(gn)) => {
                if hn != gn {
                    return None;
                }
            }
            (QTerm::Num(_), _) | (_, QTerm::Num(_)) => {
                return None;
            }
            (QTerm::Var(hv), QTerm::Const(gc)) => {
                new_subst.insert(hv, gc);
            }
            (QTerm::Const(hc), QTerm::Var(gv)) => {
                new_subst.insert(gv, hc);
            }
            (QTerm::Var(hv), QTerm::Var(gv)) => {
                // Both unbound variables: we alias the head variable to the goal
                // variable. We represent this by storing a sentinel string so that
                // when the head variable is looked up later in `resolve_term`, we
                // can detect and chase the alias.
                //
                // Sentinel form: `__ALIAS__<goal_var_name>`. `resolve_term` chases
                // these aliases through the substitution so that when `gv` later
                // gets bound, `hv` sees the same binding.
                new_subst.insert(hv, format!("__ALIAS__{}", gv));
            }
        }
    }
    Some(new_subst)
}

/// Resolve a term under a substitution, chasing variable aliases transitively.
///
/// Aliases are stored as `__ALIAS__<var_name>` sentinels in `Binding`; this
/// function follows the alias chain until it reaches a concrete constant or an
/// unbound variable.
fn resolve_term(t: &QTerm, subst: &Binding) -> QTerm {
    match t {
        QTerm::Const(_) | QTerm::Num(_) => t.clone(),
        QTerm::Var(v) => chase_var(v, subst, 0),
    }
}

/// Chase a variable name through the substitution, following `__ALIAS__` sentinels.
///
/// `depth` guards against pathological alias cycles (max 128 hops).
fn chase_var(v: &str, subst: &Binding, depth: u32) -> QTerm {
    if depth > 128 {
        // Cycle guard: return as unbound.
        return QTerm::Var(v.to_owned());
    }
    match subst.get(v) {
        None => QTerm::Var(v.to_owned()),
        Some(val) => {
            if let Some(aliased) = val.strip_prefix("__ALIAS__") {
                // This variable is aliased to another variable; chase it.
                chase_var(aliased, subst, depth + 1)
            } else {
                // Concrete constant.
                QTerm::Const(val.clone())
            }
        }
    }
}

// ── Rule renaming ─────────────────────────────────────────────────────────────

/// Rename all variables in a rule with a unique suffix to avoid conflicts.
///
/// Uses a global counter per call (via a local counter passed by the caller).
/// In this implementation we use a thread-local counter.
fn rename_rule(rule: &crate::query_ir::QRule) -> crate::query_ir::QRule {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let id = COUNTER.fetch_add(1, Ordering::Relaxed);
    let suffix = format!("_r{}", id);

    fn rename_term(t: &QTerm, suffix: &str) -> QTerm {
        match t {
            QTerm::Var(v) => QTerm::Var(format!("{}{}", v, suffix)),
            QTerm::Const(_) | QTerm::Num(_) => t.clone(),
        }
    }

    fn rename_atom(a: &QAtom, suffix: &str) -> QAtom {
        QAtom {
            pred: a.pred.clone(),
            args: a.args.iter().map(|t| rename_term(t, suffix)).collect(),
        }
    }

    fn rename_builtin(b: &crate::query_ir::QBuiltin, suffix: &str) -> crate::query_ir::QBuiltin {
        use crate::query_ir::QBuiltin;
        match b {
            QBuiltin::Is {
                target,
                lhs,
                op,
                rhs,
            } => QBuiltin::Is {
                target: rename_term(target, suffix),
                lhs: rename_term(lhs, suffix),
                op: *op,
                rhs: rename_term(rhs, suffix),
            },
            QBuiltin::Compare { lhs, op, rhs } => QBuiltin::Compare {
                lhs: rename_term(lhs, suffix),
                op: *op,
                rhs: rename_term(rhs, suffix),
            },
        }
    }

    crate::query_ir::QRule {
        head: rename_atom(&rule.head, &suffix),
        body: rule
            .body
            .iter()
            .map(|b| match b {
                QBodyLit::Atom(a) => QBodyLit::Atom(rename_atom(a, &suffix)),
                QBodyLit::Cut => QBodyLit::Cut,
                // Builtins are rejected before resolution; carry intact for renaming
                // completeness (Num operands are rename-invariant).
                QBodyLit::Builtin(b) => QBodyLit::Builtin(rename_builtin(b, &suffix)),
            })
            .collect(),
    }
}

// ── Term conversion helpers ───────────────────────────────────────────────────

/// Return the canonical string for a `QTerm::Const`, or `""` for a `Var` (wildcard).
fn term_canonical_or_wildcard(t: &QTerm) -> String {
    match t {
        QTerm::Const(c) => c.clone(),
        QTerm::Var(_) => String::new(),
        // A bare number canonicalizes to its decimal text for memo-keying purposes.
        QTerm::Num(n) => n.to_string(),
    }
}

/// Convert a canonical constant string (`<iri>` or `"lit"...`) to an oxigraph `Term`.
fn canonical_to_term(canonical: &str) -> Result<Term, String> {
    if canonical.starts_with('<') && canonical.ends_with('>') {
        let iri = &canonical[1..canonical.len() - 1];
        let nn = NamedNode::new(iri)
            .map_err(|e| format!("canonical_to_term: invalid IRI {iri:?}: {e}"))?;
        Ok(Term::NamedNode(nn))
    } else {
        Err(format!(
            "canonical_to_term: unsupported canonical form {canonical:?} \
             (only <iri> IRI constants are supported in this oracle)"
        ))
    }
}

/// Collect all variable names that appear in the goal atoms.
fn goal_vars(goal: &QGoal) -> Vec<String> {
    let mut vars: Vec<String> = Vec::new();
    for atom in &goal.atoms {
        for t in &atom.args {
            if let QTerm::Var(v) = t {
                if !vars.contains(v) {
                    vars.push(v.clone());
                }
            }
        }
    }
    vars
}

// ── Unit tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::query_ir::parse_query_program;
    use crate::seam::WorldStoreForeign;
    use crate::store::WorldStore;

    const W: &str = "http://logic.test/world/resolver";
    const PROFILE: &str = "https://blackcatinformatics.ca/logic/PositiveHornProfile";

    // ── Helpers ───────────────────────────────────────────────────────────────

    fn make_world(triples: &[(&str, &str, &str)]) -> (WorldStore, NamedNode) {
        let store = WorldStore::new();
        for (s, p, o) in triples {
            store.insert_quad(W, s, p, o);
        }
        let world_nn = NamedNode::new(W).unwrap();
        (store, world_nn)
    }

    // ── Test 1: Non-recursive EDB lookup + single IDB rule ───────────────────

    #[test]
    fn non_recursive_single_rule() {
        // EDB: parentOf(alice, bob)
        // Rule: ancestorOf(X,Y) :- parentOf(X,Y).
        // Goal: ?- ancestorOf(alice, Y).
        // Expected: Y = <.../bob>

        let base = "https://example.org/";
        let (store, world_nn) = make_world(&[(
            &format!("{base}alice"),
            &format!("{base}parentOf"),
            &format!("{base}bob"),
        )]);

        let foreign = WorldStoreForeign::from_world(&store, W, PROFILE).unwrap();

        let src = format!(
            ":- prefix(ex, '{base}').\n\
             ex:ancestorOf(X, Y) :- ex:parentOf(X, Y).\n\
             ?- ex:ancestorOf(ex:alice, Y).\n"
        );
        let prog = parse_query_program(&src).unwrap();
        let budget = Budget::default();
        let ans = resolve(&foreign, &world_nn, &prog, &budget).unwrap();

        assert_eq!(ans.status, BudgetStatus::Ok);
        assert_eq!(ans.bindings.len(), 1);
        assert_eq!(ans.bindings[0]["Y"], format!("<{base}bob>"));
    }

    // ── Test 2a: Recursive transitive closure ─────────────────────────────────

    #[test]
    fn recursive_transitive_closure_chain() {
        // EDB: parentOf(a,b), parentOf(b,c), parentOf(c,d)
        // Rules: ancestor(X,Y):-parentOf(X,Y). ancestor(X,Y):-parentOf(X,Z),ancestor(Z,Y).
        // Goal: ?- ancestor(a, Y).
        // Expected: Y ∈ {b, c, d}

        let base = "https://example.org/";
        let (store, world_nn) = make_world(&[
            (
                &format!("{base}a"),
                &format!("{base}parentOf"),
                &format!("{base}b"),
            ),
            (
                &format!("{base}b"),
                &format!("{base}parentOf"),
                &format!("{base}c"),
            ),
            (
                &format!("{base}c"),
                &format!("{base}parentOf"),
                &format!("{base}d"),
            ),
        ]);

        let foreign = WorldStoreForeign::from_world(&store, W, PROFILE).unwrap();

        let src = format!(
            ":- prefix(ex, '{base}').\n\
             ex:ancestor(X, Y) :- ex:parentOf(X, Y).\n\
             ex:ancestor(X, Y) :- ex:parentOf(X, Z), ex:ancestor(Z, Y).\n\
             ?- ex:ancestor(ex:a, Y).\n"
        );
        let prog = parse_query_program(&src).unwrap();
        let budget = Budget::default();
        let ans = resolve(&foreign, &world_nn, &prog, &budget).unwrap();

        assert_eq!(ans.status, BudgetStatus::Ok);
        let ys: Vec<&str> = ans.bindings.iter().map(|b| b["Y"].as_str()).collect();
        assert!(
            ys.contains(&format!("<{base}b>").as_str()),
            "missing b: {ys:?}"
        );
        assert!(
            ys.contains(&format!("<{base}c>").as_str()),
            "missing c: {ys:?}"
        );
        assert!(
            ys.contains(&format!("<{base}d>").as_str()),
            "missing d: {ys:?}"
        );
        assert_eq!(ans.bindings.len(), 3, "expected exactly 3 answers: {ys:?}");
    }

    // ── Test 2b: Cyclic EDB — seen-memo prevents infinite loop ───────────────

    #[test]
    fn cyclic_edb_terminates() {
        // EDB: parentOf(a,b), parentOf(b,a)  ← cycle
        // Rules: ancestor(X,Y):-parentOf(X,Y). ancestor(X,Y):-parentOf(X,Z),ancestor(Z,Y).
        // Goal: ?- ancestor(a, Y).
        // The memo must prevent infinite looping; result is {b, a} (possibly duplicates
        // filtered by the memo).

        let base = "https://example.org/";
        let (store, world_nn) = make_world(&[
            (
                &format!("{base}a"),
                &format!("{base}parentOf"),
                &format!("{base}b"),
            ),
            (
                &format!("{base}b"),
                &format!("{base}parentOf"),
                &format!("{base}a"),
            ),
        ]);

        let foreign = WorldStoreForeign::from_world(&store, W, PROFILE).unwrap();

        let src = format!(
            ":- prefix(ex, '{base}').\n\
             ex:ancestor(X, Y) :- ex:parentOf(X, Y).\n\
             ex:ancestor(X, Y) :- ex:parentOf(X, Z), ex:ancestor(Z, Y).\n\
             ?- ex:ancestor(ex:a, Y).\n"
        );
        let prog = parse_query_program(&src).unwrap();
        let budget = Budget {
            max_steps: Some(500), // generous but finite
            ..Default::default()
        };
        // Must terminate — no timeout needed beyond the budget.
        let ans = resolve(&foreign, &world_nn, &prog, &budget);
        assert!(ans.is_ok(), "cyclic EDB must terminate: {ans:?}");
        let ans = ans.unwrap();
        // Must have found at least b (and possibly a itself) without panicking.
        let ys: Vec<&str> = ans.bindings.iter().map(|b| b["Y"].as_str()).collect();
        assert!(
            ys.contains(&format!("<{base}b>").as_str()),
            "must find b: {ys:?}"
        );
    }

    // ── Test 3: Budget — max_answers ──────────────────────────────────────────

    #[test]
    fn budget_max_answers_partial() {
        let base = "https://example.org/";
        let (store, world_nn) = make_world(&[
            (
                &format!("{base}a"),
                &format!("{base}parentOf"),
                &format!("{base}b"),
            ),
            (
                &format!("{base}b"),
                &format!("{base}parentOf"),
                &format!("{base}c"),
            ),
            (
                &format!("{base}c"),
                &format!("{base}parentOf"),
                &format!("{base}d"),
            ),
        ]);

        let foreign = WorldStoreForeign::from_world(&store, W, PROFILE).unwrap();

        let src = format!(
            ":- prefix(ex, '{base}').\n\
             ex:ancestor(X, Y) :- ex:parentOf(X, Y).\n\
             ex:ancestor(X, Y) :- ex:parentOf(X, Z), ex:ancestor(Z, Y).\n\
             ?- ex:ancestor(ex:a, Y).\n"
        );
        let prog = parse_query_program(&src).unwrap();
        let budget = Budget {
            max_answers: Some(1),
            ..Default::default()
        };
        let ans = resolve(&foreign, &world_nn, &prog, &budget).unwrap();

        assert_eq!(ans.bindings.len(), 1, "exactly 1 answer with budget=1");
        assert_eq!(ans.status, BudgetStatus::Partial);
    }

    // ── Test 4: Cut rejection ─────────────────────────────────────────────────

    #[test]
    fn cut_in_rule_returns_err() {
        let base = "https://example.org/";
        let (store, world_nn) = make_world(&[(
            &format!("{base}a"),
            &format!("{base}parentOf"),
            &format!("{base}b"),
        )]);

        let foreign = WorldStoreForeign::from_world(&store, W, PROFILE).unwrap();

        let src = format!(
            ":- prefix(ex, '{base}').\n\
             ex:ancestor(X, Y) :- ex:parentOf(X, Y), !, ex:ancestor(X, Y).\n\
             ?- ex:ancestor(ex:a, Y).\n"
        );
        let prog = parse_query_program(&src).unwrap();
        let budget = Budget::default();
        let result = resolve(&foreign, &world_nn, &prog, &budget);

        assert!(result.is_err(), "cut must return Err");
        let err = result.unwrap_err();
        assert!(
            err.contains("cut is procedural"),
            "error must mention 'cut is procedural': {err:?}"
        );
    }
}
