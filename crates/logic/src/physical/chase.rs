// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Native restricted (standard) existential-rule chase.
//!
//! The forward semi-naive core ([`crate::physical::seminaive`]) is a pure Datalog
//! engine: [`ground_head`] hard-errors on a head variable the body does not bind.
//! This module adds the missing capability — **value invention** for existential head
//! variables — as the Datalog± *restricted (standard) chase*, the forward fragment
//! Nemo carries as a demoted oracle today.
//!
//! # Restricted, not oblivious
//!
//! An [`ExistentialRule`] `∃ȳ. H(x̄, ȳ) ← B(x̄)` fires on a frontier binding of the body
//! ONLY when the head is not *already* satisfied: if the store already contains an
//! extension of the frontier to witnesses making every head atom true, the firing is
//! **skipped** (the restricted-chase satisfaction check).  This is what distinguishes
//! the restricted chase from the oblivious chase, and — together with weak acyclicity
//! of the rule set — is what makes it terminate.
//!
//! # The witness is a Skolem function of the frontier (matches Nemo)
//!
//! When a firing does invent, each existential variable is bound to a deterministic
//! [`crate::physical::store::SkolemTerm`] witness addressed on the bound frontier
//! VALUES (never lexical variable names) — a genuine Skolem function `f(x̄)`.  Two
//! distinct frontier bindings mint two distinct witnesses, exactly as Nemo's restricted
//! chase does, so the produced fact set is parity-comparable to Nemo up to a null-blind
//! (recipe-recursive) renaming.  Re-firing on the same frontier recovers the same
//! witness (the registry is idempotent), so a converging program reaches its fixpoint.
//!
//! # Termination is a certificate, not a hope
//!
//! This engine does NOT decide termination — it assumes the caller has certified the
//! program terminating (weak acyclicity) via `ChaseAdmission` and refuses/​budgets the
//! rest.  The [`StepGovernor`] budget is the backstop: an unbudgeted run of a
//! non-terminating program would loop, so the router only calls this unbudgeted on a
//! certified-terminating program, and budgeted otherwise (incomplete-never-wrong).
//!
//! # Phase dead code
//!
//! Like the sibling evaluators, the chase lands before the routing that consumes it, so
//! the not-yet-wired surface allows `dead_code` module-internally.
#![allow(dead_code)]

use std::collections::BTreeSet;

use crate::physical::seminaive::{Budgeted, NativeOutcome, StepGovernor, StrataProgress};
use crate::physical::store::{Bound, RelationStore, SkolemRegistry, SkolemTerm};
use crate::provenance::{mint_derivation_id, term_display};
use crate::rule_ir::{
    distinct_pairs_satisfied, echo_asserted, ground, ground_head, match_atom, sort_rows,
    DerivedRow, EvalAtom, EvalTerm, Fact, FactKey, Solution,
};
use crate::seam::BudgetStatus;

/// A single existential (tuple-generating) rule: a conjunctive body implies a
/// conjunctive head that may quantify fresh existential variables.
///
/// The head is a conjunction so a `∃y. p(x,y) ∧ D(y)` obligation is ONE rule sharing
/// the invented witness `y` across its atoms.  `distinct` carries the pairwise
/// inequalities of a `≥n p.D` obligation (its `n` witnesses must be distinct), read
/// both by the satisfaction check and — since distinct existential ordinals already
/// mint distinct witnesses — honored by construction on a firing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ExistentialRule {
    /// The content-addressed firing rule IRI.
    pub(crate) rule_iri: String,
    /// The body atoms (positive; the DL-safe fragment binds every frontier var here).
    pub(crate) body: Vec<EvalAtom>,
    /// The conjunctive head atoms.
    pub(crate) head: Vec<EvalAtom>,
    /// Pairwise inequality guards over head/existential variables.
    pub(crate) distinct: Vec<(String, String)>,
}

impl ExistentialRule {
    /// Every variable occurring in the body (subject/object positions).
    fn body_vars(&self) -> BTreeSet<String> {
        let mut vars = BTreeSet::new();
        for atom in &self.body {
            collect_var(&atom.subject, &mut vars);
            collect_var(&atom.object, &mut vars);
        }
        vars
    }

    /// Every variable occurring in the head.
    fn head_vars(&self) -> BTreeSet<String> {
        let mut vars = BTreeSet::new();
        for atom in &self.head {
            collect_var(&atom.subject, &mut vars);
            collect_var(&atom.object, &mut vars);
        }
        vars
    }

    /// The existential head variables: head vars the body does not bind, sorted.
    ///
    /// Sorted so the ordinal assigned to each (its index here) is deterministic —
    /// the ordinal disambiguates the `n` witnesses of a `≥n` head.
    pub(crate) fn existentials(&self) -> Vec<String> {
        let body = self.body_vars();
        self.head_vars()
            .into_iter()
            .filter(|v| !body.contains(v))
            .collect()
    }

    /// The frontier variables: head vars the body DOES bind, sorted.  These are the
    /// Skolem function's arguments — the witness depends on their bound values.
    fn frontier_vars(&self) -> Vec<String> {
        let body = self.body_vars();
        self.head_vars()
            .into_iter()
            .filter(|v| body.contains(v))
            .collect()
    }

    /// Whether this rule invents (has at least one existential head variable).
    pub(crate) fn is_existential(&self) -> bool {
        !self.existentials().is_empty()
    }
}

/// Push `term`'s variable name into `vars` if it is a variable.
fn collect_var(term: &EvalTerm, vars: &mut BTreeSet<String>) {
    if let EvalTerm::Var(name) = term {
        vars.insert(name.clone());
    }
}

/// Join `atoms` against `rel` starting from `seed`, returning every extension.
///
/// A full (non-delta) index-selected conjunctive join: each atom computes a [`Bound`]
/// from the partial solution and scans only the matching rows via
/// [`RelationStore::select`], merging via [`match_atom`] (so a repeated variable must
/// agree and a constant must equal the fact surface).  Used both for the body frontier
/// join and for the restricted-chase head-satisfaction probe.
fn join_atoms(atoms: &[EvalAtom], rel: &RelationStore, seed: &Solution) -> Vec<Solution> {
    let mut solutions = vec![seed.clone()];
    for atom in atoms {
        let mut next: Vec<Solution> = Vec::new();
        for sol in &solutions {
            let subj = ground(&atom.subject, sol);
            let obj = ground(&atom.object, sol);
            let Some(bound) = atom_bound(rel, subj.as_deref(), obj.as_deref()) else {
                continue; // a bound term the store has never seen matches nothing
            };
            for (subject, object) in rel.select(atom.predicate.as_str(), bound) {
                let f = Fact {
                    subject,
                    predicate: atom.predicate.clone(),
                    object,
                };
                if let Some(mut merged) = match_atom(atom, &f, sol) {
                    merged.source_facts.push(f);
                    next.push(merged);
                }
            }
        }
        solutions = next;
        if solutions.is_empty() {
            break;
        }
    }
    solutions
}

/// The selection [`Bound`] for a `(subject, object)` pair of ground surfaces.
///
/// `None` means a bound position's term has never entered `rel`, so no row can match.
fn atom_bound(rel: &RelationStore, subj: Option<&str>, obj: Option<&str>) -> Option<Bound> {
    Some(match (subj, obj) {
        (Some(s), Some(o)) => Bound::Both(rel.term_id(s)?, rel.term_id(o)?),
        (Some(s), None) => Bound::Subject(rel.term_id(s)?),
        (None, Some(o)) => Bound::Object(rel.term_id(o)?),
        (None, None) => Bound::Any,
    })
}

/// Whether the head is ALREADY satisfied under the frontier binding `sol`.
///
/// The restricted-chase blocking condition: does the store already contain an extension
/// of `sol` to the existential variables making every head atom true — with the
/// existential/​distinct inequalities honored?  If so the firing is skipped.  Realized
/// as a conjunctive query over the head atoms (existentials free), filtered by the
/// distinct guards; any surviving solution means satisfied.
fn head_satisfied(
    rule: &ExistentialRule,
    sol: &Solution,
    rel: &RelationStore,
) -> Result<bool, String> {
    for candidate in join_atoms(&rule.head, rel, sol) {
        if distinct_pairs_satisfied(&rule.distinct, &candidate)? {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Run the restricted chase for one world's EDB under `rules`.
///
/// Returns the derived rows (the asserted-EDB echo plus every chase-invented head fact),
/// budget status, and completion frontier — the same [`Budgeted`] surface the
/// semi-naive forward core returns, so the router treats a chase result identically.
///
/// # Errors
///
/// Propagates provenance/​grounding failures from the shared `rule_ir` helpers.
pub(crate) fn chase_world(
    world: &str,
    edb_facts: &[Fact],
    rules: &[ExistentialRule],
    max_steps: Option<u64>,
) -> Result<NativeOutcome<Budgeted<Vec<DerivedRow>>>, String> {
    // Seed the columnar store from the EDB; echo the asserted facts as derived rows so
    // the native fact set is directly comparable to an oracle's closure (which includes
    // the EDB).
    let mut store = RelationStore::new();
    for f in edb_facts {
        store.insert(&f.predicate, f.subject.clone(), f.object.clone());
    }
    let mut out = echo_asserted(world, edb_facts)?;

    let mut governor = StepGovernor::new(max_steps);
    let mut registry = SkolemRegistry::new();
    let mut committed: BTreeSet<FactKey> = edb_facts.iter().map(Fact::key).collect();
    let mut status = BudgetStatus::Ok;

    // Naive restricted-chase fixpoint: each round re-derives against the full store,
    // the restricted-satisfaction check skips already-witnessed obligations, and the
    // SkolemRegistry collapses repeat firings — so a weakly-acyclic program converges.
    // (Incrementality is out of scope: the perf ledger flags the chase non-incremental.)
    'fixpoint: loop {
        // Gather this round's new facts with their provenance, keyed for deterministic
        // FactKey-sorted commit (the columnar-store determinism doctrine).
        let mut round: Vec<(FactKey, Fact, Vec<String>)> = Vec::new();
        for rule in rules {
            let existentials = rule.existentials();
            let frontier_vars = rule.frontier_vars();
            for sol in join_atoms(&rule.body, &store, &empty_solution()) {
                // The rule's `distinct` guards range over the EXISTENTIAL head vars
                // (the `≥n` distinctness), which are unbound in the body solution.  They
                // are enforced two ways: `head_satisfied` applies them to store
                // candidates (so `≥n` blocks only on n distinct existing witnesses), and
                // distinct existential ordinals mint distinct witnesses on a firing (so
                // the invented facts satisfy them by construction).
                //
                // Restricted-chase satisfaction: skip if the head already holds.
                if head_satisfied(rule, &sol, &store)? {
                    continue;
                }
                // Invent one witness per existential var (distinct ordinals ⇒ distinct
                // witnesses), addressed on the bound frontier values.
                let frontier: Vec<_> = frontier_vars
                    .iter()
                    .map(|v| bound_value(&sol, v))
                    .collect::<Result<_, _>>()?;
                let mut extended = sol.clone();
                for (ordinal, evar) in existentials.iter().enumerate() {
                    let witness = registry.mint(SkolemTerm {
                        rule_iri: rule.rule_iri.clone(),
                        ordinal,
                        frontier: frontier.clone(),
                    });
                    extended
                        .bindings
                        .push((evar.clone(), term_display(&witness)));
                }
                // Ground every head atom; each becomes a candidate new fact.
                let sources = reifiers_of(&sol)?;
                for hatom in &rule.head {
                    let fact = ground_head(hatom, &extended)?;
                    round.push((fact.key(), fact, sources.clone()));
                }
            }
        }

        // Commit in FactKey-sorted order, deduped against what is already known.
        round.sort_by(|(a, _, _), (b, _, _)| a.cmp(b));
        let mut progressed = false;
        for (key, fact, sources) in round {
            if committed.contains(&key) {
                continue;
            }
            if governor.spent() {
                status = BudgetStatus::Exhausted;
                break 'fixpoint;
            }
            let src_refs: Vec<&str> = sources.iter().map(String::as_str).collect();
            let derivation_id = mint_derivation_id(&fact_rule_iri(&sources), &src_refs);
            store.insert(&fact.predicate, fact.subject.clone(), fact.object.clone());
            out.push(DerivedRow {
                graph: world.to_owned(),
                subject: fact.subject,
                predicate: fact.predicate,
                object: fact.object,
                rule_iri: CHASE_RULE_IRI.to_owned(),
                source_quad_ids: sources,
                derivation_id,
            });
            committed.insert(key);
            governor.charge();
            progressed = true;
        }
        if !progressed {
            break; // natural fixpoint — the chase terminated
        }
    }

    sort_rows(&mut out);
    let progress = StrataProgress {
        // The chase has no strata; report a single "stratum" completed iff it ran to
        // its natural fixpoint, and saturate no predicate (a value-inventing round can
        // always, in principle, extend any head predicate — under-claim, never over).
        completed: usize::from(status == BudgetStatus::Ok),
        total: 1,
        saturated_preds: BTreeSet::new(),
    };
    Ok(NativeOutcome::Decided(Budgeted {
        rows: out,
        status,
        progress,
        consumed_steps: governor.consumed,
    }))
}

/// The firing IRI stamped on a chase-derived row.
const CHASE_RULE_IRI: &str = "https://blackcatinformatics.ca/gmeow/logic/chase/exists";

/// The empty seed solution.
fn empty_solution() -> Solution {
    Solution {
        bindings: Vec::new(),
        source_facts: Vec::new(),
    }
}

/// The `TermValue` a frontier variable is bound to under `sol` (a hard error if
/// unbound — a frontier var is bound by the body by construction).
fn bound_value(sol: &Solution, var: &str) -> Result<purrdf::TermValue, String> {
    let surface = sol
        .get(var)
        .ok_or_else(|| format!("chase: frontier variable {var:?} unbound after body join"))?;
    crate::rule_ir::surface_to_value(surface)
}

/// The reifier IRIs of a solution's matched body facts, in body order.
fn reifiers_of(sol: &Solution) -> Result<Vec<String>, String> {
    sol.source_facts.iter().map(Fact::reifier).collect()
}

/// The firing rule IRI recorded for provenance — a fixed chase IRI (the chase is one
/// engine, not a per-rule reduct), kept separate from `CHASE_RULE_IRI` only so a future
/// per-rule attribution can refine it without touching the derivation-id recipe.
fn fact_rule_iri(_sources: &[String]) -> String {
    CHASE_RULE_IRI.to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use purrdf::TermValue;

    const W: &str = "https://blackcatinformatics.ca/gmeow/world/default";
    const P: &str = "http://ex/p";
    const TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
    const C: &str = "http://ex/C";
    const D: &str = "http://ex/D";

    fn iri(s: &str) -> TermValue {
        TermValue::iri(s)
    }

    fn fact(s: &str, p: &str, o: &str) -> Fact {
        Fact {
            subject: iri(s),
            predicate: p.to_owned(),
            object: iri(o),
        }
    }

    fn var(name: &str) -> EvalTerm {
        EvalTerm::Var(name.to_owned())
    }

    fn atom(s: EvalTerm, p: &str, o: EvalTerm) -> EvalAtom {
        EvalAtom {
            subject: s,
            predicate: p.to_owned(),
            object: o,
            negated: false,
        }
    }

    /// `type(x, C) → ∃y. p(x, y) ∧ type(y, D)` — the EL `∃p.D` obligation as a TGD.
    fn some_values_from_rule() -> ExistentialRule {
        ExistentialRule {
            rule_iri: "http://ex/rule/svf".to_owned(),
            body: vec![atom(var("?x"), TYPE, EvalTerm::ConstNamed(C.to_owned()))],
            head: vec![
                atom(var("?x"), P, var("?y")),
                atom(var("?y"), TYPE, EvalTerm::ConstNamed(D.to_owned())),
            ],
            distinct: vec![],
        }
    }

    fn decided(outcome: NativeOutcome<Budgeted<Vec<DerivedRow>>>) -> Budgeted<Vec<DerivedRow>> {
        match outcome {
            NativeOutcome::Decided(b) => b,
            NativeOutcome::Unsupported(k) => panic!("expected Decided, got Unsupported({k:?})"),
        }
    }

    /// Count derived rows for a predicate.
    fn count(rows: &[DerivedRow], predicate: &str) -> usize {
        rows.iter().filter(|r| r.predicate == predicate).count()
    }

    #[test]
    fn chase_invents_a_witness_for_some_values_from() {
        // Two individuals of type C ⇒ two distinct p-edges to two distinct D witnesses
        // (restricted chase = one fresh witness per frontier binding).
        let edb = vec![fact("http://ex/a", TYPE, C), fact("http://ex/b", TYPE, C)];
        let b = decided(chase_world(W, &edb, &[some_values_from_rule()], None).unwrap());
        assert_eq!(b.status, BudgetStatus::Ok);
        assert_eq!(count(&b.rows, P), 2, "one p-edge per C individual");
        assert_eq!(count(&b.rows, TYPE), 4, "2 asserted C + 2 invented D");
        // The two witnesses are distinct nulls.
        let objs: BTreeSet<_> = b
            .rows
            .iter()
            .filter(|r| r.predicate == P)
            .map(|r| term_display(&r.object))
            .collect();
        assert_eq!(objs.len(), 2);
    }

    #[test]
    fn chase_restricted_satisfaction_skips_when_witness_exists() {
        // `a` already has a p-edge to `w` typed D ⇒ the obligation is satisfied and no
        // fresh witness is invented; `b` still gets one.
        let edb = vec![
            fact("http://ex/a", TYPE, C),
            fact("http://ex/a", P, "http://ex/w"),
            fact("http://ex/w", TYPE, D),
            fact("http://ex/b", TYPE, C),
        ];
        let b = decided(chase_world(W, &edb, &[some_values_from_rule()], None).unwrap());
        assert_eq!(
            count(&b.rows, P),
            2,
            "a's existing edge + b's invented edge"
        );
        // `a` invents nothing: its only p-edge is the pre-existing one to w.
        let a_targets: Vec<_> = b
            .rows
            .iter()
            .filter(|r| r.predicate == P && term_display(&r.subject) == "<http://ex/a>")
            .collect();
        assert_eq!(a_targets.len(), 1);
        assert_eq!(term_display(&a_targets[0].object), "<http://ex/w>");
    }

    #[test]
    fn chase_terminates_on_a_bounded_program() {
        // An acyclic EL restriction over three C individuals: the chase reaches its
        // natural fixpoint (status Ok) with a bounded, exact derived-row count.
        let edb = vec![
            fact("http://ex/a", TYPE, C),
            fact("http://ex/b", TYPE, C),
            fact("http://ex/c", TYPE, C),
        ];
        let b = decided(chase_world(W, &edb, &[some_values_from_rule()], None).unwrap());
        assert_eq!(b.status, BudgetStatus::Ok);
        // 3 echoed C + 3 invented p-edges + 3 invented D-types = 9 rows, and no more on
        // a second identical run (determinism).
        assert_eq!(b.rows.len(), 9);
        let again = decided(chase_world(W, &edb, &[some_values_from_rule()], None).unwrap());
        assert_eq!(b.rows.len(), again.rows.len());
        assert_eq!(b.consumed_steps, again.consumed_steps);
    }

    #[test]
    fn chase_budget_exhaustion_is_incomplete_not_wrong() {
        // A cyclic `D ⊑ ∃p.D` would not terminate unbudgeted; with a step budget the
        // chase stops early, reporting Exhausted with a sound committed prefix.
        let cyclic = ExistentialRule {
            rule_iri: "http://ex/rule/cyclic".to_owned(),
            body: vec![atom(var("?x"), TYPE, EvalTerm::ConstNamed(D.to_owned()))],
            head: vec![
                atom(var("?x"), P, var("?y")),
                atom(var("?y"), TYPE, EvalTerm::ConstNamed(D.to_owned())),
            ],
            distinct: vec![],
        };
        let edb = vec![fact("http://ex/a", TYPE, D)];
        let b = decided(chase_world(W, &edb, &[cyclic], Some(3)).unwrap());
        assert_eq!(b.status, BudgetStatus::Exhausted);
        assert_eq!(
            b.consumed_steps, 3,
            "exactly the budget of committed derivations"
        );
    }

    #[test]
    fn chase_at_least_two_requires_two_distinct_witnesses() {
        // `≥2 p.D`: a single existing typed p-edge does NOT satisfy the obligation; the
        // chase must invent a second, distinct witness.
        let ge2 = ExistentialRule {
            rule_iri: "http://ex/rule/ge2".to_owned(),
            body: vec![atom(var("?x"), TYPE, EvalTerm::ConstNamed(C.to_owned()))],
            head: vec![
                atom(var("?x"), P, var("?y1")),
                atom(var("?y1"), TYPE, EvalTerm::ConstNamed(D.to_owned())),
                atom(var("?x"), P, var("?y2")),
                atom(var("?y2"), TYPE, EvalTerm::ConstNamed(D.to_owned())),
            ],
            distinct: vec![("?y1".to_owned(), "?y2".to_owned())],
        };
        // `a` has ONE existing typed witness — short of the two required.
        let edb = vec![
            fact("http://ex/a", TYPE, C),
            fact("http://ex/a", P, "http://ex/w"),
            fact("http://ex/w", TYPE, D),
        ];
        let b = decided(chase_world(W, &edb, &[ge2], None).unwrap());
        // a must end with ≥2 distinct D-typed p-targets.
        let targets: BTreeSet<_> = b
            .rows
            .iter()
            .filter(|r| r.predicate == P && term_display(&r.subject) == "<http://ex/a>")
            .map(|r| term_display(&r.object))
            .collect();
        assert!(
            targets.len() >= 2,
            "≥2 distinct witnesses, got {}",
            targets.len()
        );
    }
}
