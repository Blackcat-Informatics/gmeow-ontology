// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Stratified semi-naive bottom-up evaluator with index selection.
//!
//! This is the forward leg of the native execution core.  It evaluates an
//! [`EvalRule`] program over a [`crate::store::WorldStore`] world-by-world,
//! producing exactly the [`DerivedRow`] provenance the reference engine
//! ([`crate::rule_ir::least_model_of_reduct`]) emits — **byte-identical**.
//!
//! # Why a second evaluator next to `least_model_of_reduct`
//!
//! `least_model_of_reduct` is the Gelfond-Lifschitz reduct least model: NAF is
//! evaluated against a FIXED reference store, and the positive semi-naive join
//! scans the whole predicate bucket of a ternary [`FactStore`] and post-filters by
//! the semi-naive delta.  The native core replaces that full-scan with
//! **index selection** over the columnar [`RelationStore`]: each positive body atom
//! computes a [`Bound`] from the partial solution and scans ONLY the matching rows.
//!
//! It also computes the NAF reference *dynamically* by **stratification** instead
//! of an externally-supplied guess: lower strata are fully materialized and frozen
//! before a higher stratum runs, so a negated body atom is decided by membership in
//! the accumulated store — exactly the stratified-Datalog semantics.
//!
//! # Determinism (the parity guarantee)
//!
//! The round loop here is a structural copy of `least_model_of_reduct`'s loop:
//! same EDB-seeded delta, same per-round canonical-winner map keyed by head fact,
//! the SAME first-wins quality tiebreak
//! ([`RuleRoundCandidate`]'s `(max_src_depth, sum_src_depth, sorted_sources,
//! rule_iri)`]), same per-fact depth map (EDB depth 0; derived depth = 1 + max
//! source depth), and the same body-order `source_quad_ids`.  The ONLY substitution
//! is the join: `join_body`'s full-bucket scan becomes [`join_body_indexed`]'s
//! index-selected scan.  Because [`RelationStore::select`] preserves insertion
//! order and the [`RelationStore`] is filled in lockstep with the [`FactStore`], the
//! produced solution sequence — and hence `source_facts` order and the winner
//! tiebreak — is identical.  For a single-stratum POSITIVE program the derived rows
//! therefore equal `least_model_of_reduct(edb, rules, &empty)` exactly; this is
//! asserted by the [`physical_native_matches_reference_byte_identical`] oracle test.
//!
//! # Stratification (dynamic) + negation
//!
//! The predicate dependency graph carries an edge head_pred → body_pred per rule,
//! marked NEGATIVE when the body atom is negated.  Strata are assigned so a NEGATIVE
//! edge head→neg forces stratum(head) > stratum(neg) and a POSITIVE edge forces
//! stratum(head) >= stratum(body).  A negative edge inside a cycle is non-stratifiable
//! → [`NativeOutcome::Unsupported`]`(`[`UnsupportedKind::NonStratifiable`]`)`.  Strata
//! run in increasing order; within a stratum the predicates being derived never
//! appear negated, so NAF against the accumulated (frozen-below) store is correct.
//!
//! # Phase dead code
//!
//! Like [`crate::rule_ir`], this evaluator lands before the native-first routing
//! that consumes it, so the not-yet-wired surface allows `dead_code`
//! module-internally rather than scattering per-item attributes.
#![allow(dead_code)]

use std::borrow::Cow;
use std::collections::{BTreeSet, HashMap, HashSet};

use crate::physical::builtin_eval::{BuiltinOutcome, emit_integer_surface, eval as eval_builtin};
use crate::physical::store::{Bound, RelationStore};
use crate::provenance::mint_derivation_id;
use crate::query_ir::QBuiltin;
use crate::rule_ir::{
    DerivedRow, EvalAtom, EvalRule, Fact, FactKey, FactStore, RuleRoundCandidate, Solution,
    distinct_pairs_satisfied, echo_asserted, ground, ground_head, match_atom, sort_rows,
    world_edb_facts,
};
use crate::seam::BudgetStatus;

/// A native-execution combination the forward core cannot decide.
///
/// Carried by [`NativeOutcome::Unsupported`].  `NonStratifiable` is the only variant
/// the forward semi-naive leg can raise; the others name combinations the LATER
/// magic-sets / backward rungs surface, declared here so the outcome enum is stable
/// across the rungs that consume it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum UnsupportedKind {
    /// A negative dependency-graph edge lies inside a cycle — no stratification exists.
    NonStratifiable,
    /// A `!`/cut control construct (no declarative bottom-up meaning).
    Cut,
    /// An arithmetic / builtin the native core does not evaluate.
    Arithmetic,
    /// A non-binary atom (arity ≠ 2 after the world slot is dropped).
    NonBinaryAtom,
    /// A demand (magic-set) transformation that would break stratification.
    DemandBreaksStratification,
    /// An existential-rule program whose termination the acyclicity certifier could
    /// not establish (outside the certified-terminating chase fragment).  The router
    /// refuses it to the oracle, or runs it budgeted-partial — never a wrong or
    /// non-terminating native result.
    NonTerminatingExistential,
}

/// The result of a native-execution attempt: a decided value or a declared gap.
///
/// `Unsupported` is a FIRST-CLASS outcome, never a panic or a silent approximation —
/// the caller routes an unsupported combination to an oracle (no-optionality: the
/// native core states what it cannot do rather than papering over it).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum NativeOutcome<T> {
    /// The native core decided the request, yielding `T`.
    Decided(T),
    /// The request falls outside the native core's competence (named by the kind).
    Unsupported(UnsupportedKind),
}

// ── Step/derivation budget governor + completion frontier ─────────────────────────

/// The completion frontier of a stratified evaluation.
///
/// The least model is built stratum-by-stratum in a fixed order.  When a step budget
/// exhausts inside stratum *k*, every predicate at a stratum `< k` has its **final**
/// least-model extension (stratification guarantees a stratum-*k* rule only depends on,
/// and only negates, strata `< k`).  Those predicates — plus every EDB predicate — are
/// therefore genuinely decided even though the overall run is incomplete, and are
/// recorded in [`StrataProgress::saturated_preds`].  A goal predicate found there yields
/// a sound `neither` on an empty witness (a conclusive four-valued verdict), NOT the
/// `undetermined` of an unfinished search (`LOGIC-SEMANTICS.md` §five-field).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct StrataProgress {
    /// The number of strata fully saturated (the frontier).  Strata `0..completed`
    /// ran to their natural fixpoint; a stratum at index `completed`, if any, was cut
    /// mid-fixpoint by the step budget.
    pub(crate) completed: usize,
    /// The total number of strata in the program.
    pub(crate) total: usize,
    /// The predicates whose extension is final: the heads of the saturated strata plus
    /// every EDB predicate.  Under-claims rather than over-claims (a cut multi-world
    /// forward run reports only what is provably settled), never the reverse.
    pub(crate) saturated_preds: BTreeSet<String>,
}

/// A native evaluation outcome plus how far the step budget got.
///
/// `status` is the seam's canonical [`BudgetStatus`]: `Ok` when the fixpoint reached its
/// natural end within budget, `Exhausted` when a `max_steps` cut stopped it early
/// (`Partial` is a post-fixpoint `max_answers` concern owned by the backward leg).  An
/// `Exhausted` outcome is **incomplete, never wrong**: every committed derivation is
/// genuinely in the least model; the budget only bounds how many were produced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Budgeted<T> {
    /// The evaluation payload (derived rows, or the full fact set).
    pub(crate) rows: T,
    /// The budget status at the point evaluation stopped.
    pub(crate) status: BudgetStatus,
    /// The completion frontier (which strata / predicates are settled).
    pub(crate) progress: StrataProgress,
    /// The number of committed derivations (deterministic; a cost probe and the
    /// determinism check — identical inputs ⇒ identical count).
    pub(crate) consumed_steps: u64,
}

impl<T> Budgeted<T> {
    /// Lower the crate-internal governor state ([`StrataProgress`] + `consumed_steps`)
    /// into the public [`CompletionFrontier`] that crosses the crate boundary on
    /// [`crate::query_ir::AnswerSet`] / [`crate::materialize::Materialization`].
    pub(crate) fn frontier(&self) -> crate::query_ir::CompletionFrontier {
        crate::query_ir::CompletionFrontier {
            completed: self.progress.completed,
            total: self.progress.total,
            saturated_preds: self.progress.saturated_preds.clone(),
            consumed_steps: self.consumed_steps,
        }
    }
}

/// The set of predicates that appear as a rule HEAD (i.e. are IDB-derivable).
///
/// A predicate in this set is NOT settled merely by seeding its EDB facts: its full
/// least-model extension also includes whatever its stratum derives, so it becomes
/// settled only when that stratum reaches its natural fixpoint.  Only a *pure-EDB*
/// predicate — one that is never a rule head — is final from the seed alone.  This
/// distinction matters for a **self-recursive** predicate (both an EDB fact set and a
/// recursive head, e.g. `subClassOf(X,Z) :- subClassOf(X,Y), subClassOf(Y,Z)`): seeding
/// it as saturated would OVER-claim a settled extension while its closure is still being
/// (or was never) derived.  The frontier under-claims rather than over-claims.
fn head_predicates(rules_by_stratum: &[Vec<&EvalRule>]) -> BTreeSet<String> {
    rules_by_stratum
        .iter()
        .flatten()
        .map(|r| r.head.predicate.as_str().to_owned())
        .collect()
}

/// Whether a stratum's semi-naive fixpoint reached its natural end or was budget-cut.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FixpointStatus {
    /// The fixpoint reached `round.is_empty()` within budget.
    Complete,
    /// The `max_steps` budget was exhausted mid-fixpoint; the committed prefix is a
    /// sound (FactKey-ordered) partial least model.
    Exhausted,
}

/// Governs the step/derivation budget for the native fixpoint.
///
/// A native "step" is **one committed derivation** — a winner inserted in the
/// FactKey-sorted commit loop of [`eval_stratum_fixpoint`].  That is the only provably
/// reproducible counting point (the join-solution loop and the round map are not
/// order-stable across rules), so counting there keeps the `Exhausted`/`Ok` boundary
/// deterministic.  `limit == None` is unbounded: the counter never trips and the status
/// stays `Ok`, so an unbudgeted run is byte-identical to the pre-governor engine.
///
/// The unit is intentionally NOT the reference oracle's rule-expansion/EDB-lookup step
/// (`LOGIC-CONFORMANCE.md` leaves the budget unit open — "time, depth, or iteration
/// limit"); only the *outcome semantics* (incomplete-not-wrong, deterministic) must
/// match the docs, never a cross-engine step-count equivalence.
pub(crate) struct StepGovernor {
    /// The step ceiling; `None` is unbounded.
    limit: Option<u64>,
    /// Committed derivations so far.
    pub(crate) consumed: u64,
}

impl StepGovernor {
    pub(crate) fn new(max_steps: Option<u64>) -> Self {
        Self {
            limit: max_steps,
            consumed: 0,
        }
    }

    /// Whether the budget is spent — the next derivation may NOT be committed.
    ///
    /// Checked *before* committing each winner, so `limit == Some(0)` stops before the
    /// first derivation (zero derived rows, immediate `Exhausted`), and `limit ==
    /// Some(n)` admits exactly `n` committed derivations.
    pub(crate) fn spent(&self) -> bool {
        matches!(self.limit, Some(l) if self.consumed >= l)
    }

    /// Record one committed derivation.
    pub(crate) fn charge(&mut self) {
        self.consumed = self.consumed.saturating_add(1);
    }
}

// ── Index-selected semi-naive join ────────────────────────────────────────────────

/// Compute the [`Bound`] for `atom`'s `(subject, object)` columns under `sol`.
///
/// A position contributes iff it grounds (a bound var or a constant); an unbound
/// var contributes nothing.  Ground surfaces come from [`ground`] and are
/// translated to interned ids via [`RelationStore::term_id`] — the store's
/// dictionary is keyed on the same display surfaces, so the translation is exact.
/// `None` means a bound position's term has never entered `rel`: no row can match,
/// and the caller treats the selection as empty (exactly where a surface-keyed
/// index would have produced zero matches).
fn atom_bound(rel: &RelationStore, subj: Option<&str>, obj: Option<&str>) -> Option<Bound> {
    Some(match (subj, obj) {
        (Some(s), Some(o)) => Bound::Both(rel.term_id(s)?, rel.term_id(o)?),
        (Some(s), None) => Bound::Subject(rel.term_id(s)?),
        (None, Some(o)) => Bound::Object(rel.term_id(o)?),
        (None, None) => Bound::Any,
    })
}

/// Extend each partial solution by index-selecting `atom`'s matching rows under `scan`.
///
/// This is the index-selected analogue of `rule_ir::extend_solutions`: instead of
/// scanning the whole predicate bucket and post-filtering on the bound positions,
/// it computes a [`Bound`] from each partial solution and calls
/// [`RelationStore::select`], which returns ONLY the matching rows in insertion
/// order.  Each returned `(subject, object)` tuple is wrapped as a [`Fact`] and
/// handed to [`match_atom`] exactly as `extend_solutions` does, so the produced
/// solution sequence (and `source_facts` order) is identical to the full-scan
/// engine.  The semi-naive [`Scan`] filter is applied on the [`FactKey`] of each
/// wrapped fact, identical to `extend_solutions`'s `keep`.
fn extend_solutions_indexed(
    atom: &EvalAtom,
    rel: &RelationStore,
    delta: &HashSet<FactKey>,
    scan: Scan,
    solutions: &[Solution],
) -> Vec<Solution> {
    let pred = atom.predicate.as_str();
    let mut next: Vec<Solution> = Vec::new();
    for sol in solutions {
        // Compute the selection bound from the current partial solution, translating
        // ground surfaces to interned ids.  A bound term the store has never seen
        // matches nothing — skip to the next solution (the empty selection).
        let subj_surface = ground(&atom.subject, sol);
        let obj_surface = ground(&atom.object, sol);
        let Some(bound) = atom_bound(rel, subj_surface.as_deref(), obj_surface.as_deref()) else {
            continue;
        };
        for (subject, object) in rel.select(pred, bound) {
            let f = Fact {
                subject,
                predicate: atom.predicate.clone(),
                object,
            };
            // Semi-naive position decomposition: keep only the rows this scan mode
            // admits, on the SAME FactKey `extend_solutions` filters on.
            let keep = match scan {
                Scan::Delta => delta.contains(&f.key()),
                Scan::Full => true,
                Scan::OldOnly => !delta.contains(&f.key()),
            };
            if !keep {
                continue;
            }
            if let Some(mut merged) = match_atom(atom, &f, sol) {
                merged.source_facts.push(f);
                next.push(merged);
            }
        }
    }
    next
}

/// The semi-naive position-decomposition scan mode for one positive body atom.
///
/// Identical in meaning to `rule_ir::Scan` (which is private), reproduced here so the
/// index-selected join applies the same delta×full decomposition.
#[derive(Clone, Copy)]
enum Scan {
    /// Bind to rows whose key is in `delta` (the "new at p" position).
    Delta,
    /// Bind to any row (no delta constraint).
    Full,
    /// Bind only to rows whose key is NOT in `delta` (positions after p).
    OldOnly,
}

/// Join all body atoms against `rel`, evaluating NAF against the accumulated store.
///
/// The index-selected twin of `rule_ir::join_body`: the positive join is the SAME
/// semi-naive delta×full position decomposition (union over each delta position `p`
/// of `{ a_p ∈ delta, a_{<p} ∈ full, a_{>p} ∈ store \ delta }`), with each per-atom
/// scan performed by [`extend_solutions_indexed`].  NAF body atoms are filtered
/// after the positive join via membership in `accumulated` (the frozen-below store),
/// which is exactly the stratified-negation reference.
fn join_body_indexed(
    rule: &EvalRule,
    rel: &RelationStore,
    accumulated: &RelationStore,
    delta: &HashSet<FactKey>,
    gap: &mut bool,
) -> Vec<Solution> {
    let positive: Vec<&EvalAtom> = rule.body.iter().filter(|a| !a.negated).collect();
    let negated: Vec<&EvalAtom> = rule.body.iter().filter(|a| a.negated).collect();

    let empty = Solution {
        bindings: Vec::new(),
        source_facts: Vec::new(),
    };

    let mut solutions: Vec<Solution> = if positive.is_empty() {
        // Zero positive atoms never touch delta, so they never fire in a semi-naive
        // round — matches `join_body`'s empty-positive branch exactly.
        Vec::new()
    } else {
        let k = positive.len();
        let mut all: Vec<Solution> = Vec::new();
        for p in 0..k {
            let mut partial: Vec<Solution> = vec![empty.clone()];
            for (j, atom) in positive.iter().enumerate() {
                let scan = if j < p {
                    Scan::Full
                } else if j == p {
                    Scan::Delta
                } else {
                    Scan::OldOnly
                };
                partial = extend_solutions_indexed(atom, rel, delta, scan, &partial);
                if partial.is_empty() {
                    break;
                }
            }
            all.extend(partial);
        }
        all
    };

    // Post-join constraint stage: evaluate the rule's arithmetic/comparison
    // builtins in body order.  A generator (`is` with a free target) binds its
    // target — available to `ground_head` and to the negated check below; a filter
    // prunes the solution.  This runs BEFORE the NAF retain so a negated atom over
    // a generator-bound variable sees the binding.
    if !rule.builtins.is_empty() {
        solutions = apply_builtins(&rule.builtins, solutions, gap);
    }

    if !negated.is_empty() {
        solutions.retain(|sol| {
            !negated
                .iter()
                .any(|neg| negated_atom_satisfied(neg, sol, accumulated))
        });
    }

    solutions
}

/// Evaluate a rule's arithmetic/comparison builtins against each candidate
/// solution, in body order, via the shared moded evaluator.
///
/// A generator extends the solution's bindings with the computed value in the
/// canonical typed-integer surface; a filter keeps or prunes the solution. An
/// operand that is still unbound, or a domain/precision error (÷0, overflow),
/// sets `gap` and drops the solution — the caller then re-demotes the WHOLE
/// program to the oracle rather than present an incomplete native answer, so a
/// dropped solution is never a wrong answer.
fn apply_builtins(builtins: &[QBuiltin], sols: Vec<Solution>, gap: &mut bool) -> Vec<Solution> {
    let mut out: Vec<Solution> = Vec::with_capacity(sols.len());
    'next_sol: for mut sol in sols {
        for b in builtins {
            // Scope the immutable borrow of `sol` to the evaluation so the binding
            // can be extended after the outcome is known. The lookup borrows the
            // bound surface directly (no per-lookup allocation).
            let outcome = {
                let lookup = |name: &str| sol.get(name).map(Cow::Borrowed);
                eval_builtin(b, &lookup)
            };
            match outcome {
                BuiltinOutcome::Filter(true) => {}
                BuiltinOutcome::Filter(false) => continue 'next_sol,
                BuiltinOutcome::Generate { var, value } => {
                    sol.bindings.push((var, emit_integer_surface(value)));
                }
                BuiltinOutcome::Unbound | BuiltinOutcome::Error(_) => {
                    // A single unbound operand / domain error re-demotes the WHOLE
                    // program to the oracle, so the remaining solutions cannot
                    // change the outcome — stop evaluating them.
                    *gap = true;
                    return Vec::new();
                }
            }
        }
        out.push(sol);
    }
    out
}

/// Whether a negated atom is satisfied (blocks the rule) — its grounded form is
/// PRESENT in the accumulated (frozen-below) store.
///
/// Mirrors `rule_ir::negated_atom_satisfied`, but probes the columnar
/// [`RelationStore::contains`] rather than the ternary `FactStore`.  Within a
/// stratum the negated predicate is fully materialized in a strictly lower stratum,
/// so this membership is the stratified-negation truth value.  A partially-bound
/// negated atom never arises in the DL-safe gmeow fragment (every negated var is
/// bound by a positive body atom); an unbound one is treated as not-satisfied,
/// identical to the reference.
fn negated_atom_satisfied(atom: &EvalAtom, sol: &Solution, accumulated: &RelationStore) -> bool {
    let s = ground(&atom.subject, sol);
    let o = ground(&atom.object, sol);
    match (s, o) {
        (Some(s), Some(o)) => accumulated.contains(atom.predicate.as_str(), &s, &o),
        _ => false,
    }
}

// ── Stratification ───────────────────────────────────────────────────────────────

/// Assign each predicate a stratum, or report the program non-stratifiable.
///
/// Iterative longest-path relaxation over the predicate dependency graph: a POSITIVE
/// edge head→body requires stratum(head) ≥ stratum(body); a NEGATIVE edge requires
/// stratum(head) > stratum(body).  Repeatedly relaxing (raising a head's stratum to
/// satisfy a violated edge) converges iff the program is stratifiable.  If after
/// `n` full passes (n = predicate count) an edge is still violated, a negative edge
/// lies inside a cycle and no finite stratification exists → `None`.
///
/// Predicates appearing only as constants in EDB but never as a head still get a
/// stratum (0) so a negated reference to a base predicate is decided in stratum 0.
fn stratify(rules: &[EvalRule]) -> Option<HashMap<String, usize>> {
    // Collect every predicate (heads and body atoms).
    let mut preds: BTreeSet<String> = BTreeSet::new();
    for rule in rules {
        preds.insert(rule.head.predicate.as_str().to_owned());
        for atom in &rule.body {
            preds.insert(atom.predicate.as_str().to_owned());
        }
    }

    // Edges: (head_pred, body_pred, negative?).
    let mut edges: Vec<(String, String, bool)> = Vec::new();
    for rule in rules {
        let head = rule.head.predicate.as_str().to_owned();
        for atom in &rule.body {
            edges.push((
                head.clone(),
                atom.predicate.as_str().to_owned(),
                atom.negated,
            ));
        }
    }

    let mut stratum: HashMap<String, usize> = preds.iter().map(|p| (p.clone(), 0usize)).collect();

    // Bellman-Ford-style relaxation; `n` passes suffice for a stratifiable program,
    // one more pass detects a still-violated (cyclic-negative) edge.
    let n = preds.len();
    for _pass in 0..=n {
        let mut changed = false;
        for (head, body, negative) in &edges {
            let body_s = stratum[body];
            let need = if *negative { body_s + 1 } else { body_s };
            let head_s = stratum[head];
            if head_s < need {
                stratum.insert(head.clone(), need);
                changed = true;
            }
        }
        if !changed {
            return Some(stratum);
        }
    }
    // Still relaxing after n+1 passes ⇒ a negative edge sits in a cycle.
    None
}

// ── Forward entry ────────────────────────────────────────────────────────────────

/// Materialize `rules` over every world in `store` with the native stratified
/// semi-naive evaluator.
///
/// Mirrors [`crate::wellfounded::materialize`]: for each sorted world the asserted
/// EDB is echoed, then the stratified native fixpoint runs seeded from the world EDB,
/// each derived row is stamped with the world graph, and the whole output is sorted
/// by `(graph, subject, predicate, object)`.  If the program is not stratifiable the
/// result is `Ok(NativeOutcome::Unsupported(NonStratifiable))` — a declared gap, not
/// a panic.
///
/// # Errors
///
/// Returns `Err` for an invalid input IRI, an unbound head/guard variable, or a
/// provenance-recipe failure (propagated from the shared `rule_ir` helpers).
pub(crate) fn materialize_native(
    store: &crate::store::WorldStore,
    rules: &[EvalRule],
    max_steps: Option<u64>,
) -> Result<NativeOutcome<Budgeted<Vec<DerivedRow>>>, String> {
    // Stratification is a property of the rules alone; decide it once.
    let Some(stratum_of) = stratify(rules) else {
        return Ok(NativeOutcome::Unsupported(UnsupportedKind::NonStratifiable));
    };

    // Order the rules into strata.  A rule belongs to the stratum of its HEAD
    // predicate; within a stratum the original program order is preserved (rules
    // fire in parse order, matching the reference engine).
    let max_stratum = rules
        .iter()
        .map(|r| stratum_of[r.head.predicate.as_str()])
        .max()
        .unwrap_or(0);
    let mut rules_by_stratum: Vec<Vec<&EvalRule>> = vec![Vec::new(); max_stratum + 1];
    for rule in rules {
        let s = stratum_of[rule.head.predicate.as_str()];
        rules_by_stratum[s].push(rule);
    }

    let mut worlds = store.worlds();
    worlds.sort();

    // `max_steps` is a SINGLE GLOBAL budget across the sorted worlds (not reset per
    // world): the correct bundle-guard semantics and deterministic because world order
    // is fixed.  Worlds run until the shared counter is spent; later worlds then never
    // run (their strata stay unsaturated).
    let mut governor = StepGovernor::new(max_steps);
    let total = rules_by_stratum.len();
    let world_count = worlds.len();
    let mut out: Vec<DerivedRow> = Vec::new();
    let mut status = BudgetStatus::Ok;
    // Cross-world frontier.  A predicate is settled bundle-wide only when it is settled
    // in EVERY world that will contribute, so the frontier is the INTERSECTION of the
    // per-world settled sets.  If the global budget is spent before the last world runs,
    // the not-yet-run worlds could still extend any predicate, so the bundle frontier
    // under-claims to empty (never assert a predicate settled that an unrun world could
    // grow).  A cut on the LAST world leaves no unrun world, so that world's own frontier
    // (its settled lower strata) stands.
    let mut partial_completed = 0usize;
    let mut frontier: Option<BTreeSet<String>> = None;
    let mut every_world_complete = true;
    let mut untouched_worlds_remain = false;
    for (idx, world) in worlds.iter().enumerate() {
        let edb_facts = world_edb_facts(store, world)?;

        // Asserted-EDB echo (identical to wellfounded::materialize).
        out.extend(echo_asserted(world, &edb_facts)?);

        let budgeted = eval_world_stratified(&edb_facts, &rules_by_stratum, &mut governor)?;
        for mut row in budgeted.rows {
            row.graph = world.clone();
            out.push(row);
        }
        // Running intersection across the worlds that actually ran.
        frontier = Some(match frontier {
            None => budgeted.progress.saturated_preds,
            Some(f) => f
                .intersection(&budgeted.progress.saturated_preds)
                .cloned()
                .collect(),
        });
        if budgeted.status == BudgetStatus::Exhausted {
            status = BudgetStatus::Exhausted;
            every_world_complete = false;
            partial_completed = budgeted.progress.completed;
            untouched_worlds_remain = idx + 1 < world_count;
            break; // global budget spent — later worlds don't run
        }
    }

    let saturated_preds = if untouched_worlds_remain {
        // Later worlds never ran; nothing is provably settled bundle-wide.
        BTreeSet::new()
    } else {
        frontier.unwrap_or_default()
    };
    let progress = StrataProgress {
        completed: if every_world_complete {
            total
        } else {
            partial_completed
        },
        total,
        saturated_preds,
    };

    sort_rows(&mut out);
    Ok(NativeOutcome::Decided(Budgeted {
        rows: out,
        status,
        progress,
        consumed_steps: governor.consumed,
    }))
}

/// Run the stratified semi-naive fixpoint for ONE world's EDB, returning the derived
/// (non-EDB) rows with first-wins provenance.
///
/// Maintains, in lockstep, a [`FactStore`] (for keys/depth/provenance, exactly as
/// `least_model_of_reduct`) and a [`RelationStore`] (for the index-selected join).
/// Each stratum runs the semi-naive fixpoint seeded from the facts accumulated by
/// lower strata; the depth map and first-wins winner selection carry across strata,
/// so a single-stratum positive program reproduces `least_model_of_reduct` byte for
/// byte.  NAF body atoms read the accumulated [`RelationStore`], which holds all
/// strictly-lower strata fully materialized and frozen.
fn eval_world_stratified(
    edb_facts: &[Fact],
    rules_by_stratum: &[Vec<&EvalRule>],
    governor: &mut StepGovernor,
) -> Result<Budgeted<Vec<DerivedRow>>, String> {
    // Shared accumulated store (both forms), seeded from the EDB in sorted-key order
    // (world_edb_facts already sorted), so seeding matches the reference.
    let mut store = FactStore::new();
    let mut rel = RelationStore::new();
    let mut depth: HashMap<FactKey, u32> = HashMap::new();

    // A PURE-EDB predicate (never a rule head) is settled from the seed; a predicate that
    // is also a rule head is settled only when its stratum completes (below), so exclude
    // it here — otherwise a self-recursive predicate would over-claim while its closure is
    // still unbuilt.
    let head_preds = head_predicates(rules_by_stratum);
    let mut saturated_preds: BTreeSet<String> = edb_facts
        .iter()
        .map(|f| f.predicate.clone())
        .filter(|p| !head_preds.contains(p))
        .collect();

    for f in edb_facts {
        depth.insert(f.key(), 0);
        if store.insert(f.clone()) {
            rel.insert(&f.predicate, f.subject.clone(), f.object.clone());
        }
    }

    let mut derivations: Vec<DerivedRow> = Vec::new();

    let total = rules_by_stratum.len();
    let mut completed = 0usize;
    let mut status = BudgetStatus::Ok;
    // The forward `.rls` materialization carries no arithmetic builtins (the ontology
    // corpus has none), so this stays false; assert that invariant below.
    let mut builtin_gap = false;
    for stratum_rules in rules_by_stratum {
        if stratum_rules.is_empty() {
            completed += 1; // an empty stratum is trivially saturated
            continue;
        }
        match eval_stratum_fixpoint(
            stratum_rules,
            &mut store,
            &mut rel,
            &mut depth,
            &mut derivations,
            governor,
            &mut builtin_gap,
        )? {
            FixpointStatus::Complete => {
                // This stratum reached its natural fixpoint: its head predicates are now
                // final and join the settled frontier.
                for rule in stratum_rules {
                    saturated_preds.insert(rule.head.predicate.as_str().to_owned());
                }
                completed += 1;
            }
            FixpointStatus::Exhausted => {
                // The budget cut this stratum mid-fixpoint: it is NOT saturated, and no
                // later stratum runs.  The committed prefix stays (sound partial model).
                status = BudgetStatus::Exhausted;
                break;
            }
        }
    }

    debug_assert!(
        !builtin_gap,
        "forward materialization rules carry no arithmetic builtins"
    );

    Ok(Budgeted {
        rows: derivations,
        status,
        progress: StrataProgress {
            completed,
            total,
            saturated_preds,
        },
        consumed_steps: governor.consumed,
    })
}

/// Run the semi-naive fixpoint for the rules of ONE stratum into the shared stores.
///
/// This loop is a structural copy of `least_model_of_reduct`'s round loop — same
/// EDB/lower-stratum-seeded delta, same per-round canonical-winner map and quality
/// tiebreak, same depth bookkeeping, same body-order `source_quad_ids` — with the
/// join replaced by [`join_body_indexed`] (index selection) and NAF read from the
/// accumulated [`RelationStore`] (`rel`, the frozen-below store).
fn eval_stratum_fixpoint(
    rules: &[&EvalRule],
    store: &mut FactStore,
    rel: &mut RelationStore,
    depth: &mut HashMap<FactKey, u32>,
    derivations: &mut Vec<DerivedRow>,
    governor: &mut StepGovernor,
    builtin_gap: &mut bool,
) -> Result<FixpointStatus, String> {
    // Seed delta with ALL currently-known keys so this stratum's rules fire against
    // the seed in round 1 (mirrors `least_model_of_reduct`'s `delta = key_set()`).
    let mut delta: HashSet<FactKey> = store.key_set();

    loop {
        let mut round: HashMap<FactKey, RuleRoundCandidate> = HashMap::new();

        for rule in rules {
            for sol in join_body_indexed(rule, rel, rel, &delta, builtin_gap) {
                if !distinct_pairs_satisfied(&rule.distinct_pairs, &sol)? {
                    continue;
                }
                let head = ground_head(&rule.head, &sol)?;
                let key = head.key();
                if store.contains_key(&key) {
                    continue; // a prior round/stratum already derived it; earlier wins
                }

                // Provenance: reifiers of matched POSITIVE body facts in body order.
                let mut sources: Vec<String> = Vec::with_capacity(sol.source_facts.len());
                let mut max_sd: u32 = 0;
                let mut sum_sd: u64 = 0;
                for sf in &sol.source_facts {
                    sources.push(sf.reifier()?);
                    let d = *depth.get(&sf.key()).unwrap_or(&0);
                    max_sd = max_sd.max(d);
                    sum_sd = sum_sd.saturating_add(u64::from(d));
                }
                let src_refs: Vec<&str> = sources.iter().map(String::as_str).collect();
                let deriv = mint_derivation_id(&rule.rule_iri, &src_refs);
                let mut sorted_sources = sources.clone();
                sorted_sources.sort();

                let candidate = RuleRoundCandidate {
                    head,
                    key: key.clone(),
                    sources,
                    sorted_sources,
                    deriv,
                    rule_iri: rule.rule_iri.clone(),
                    max_src_depth: max_sd,
                    sum_src_depth: sum_sd,
                };
                round
                    .entry(key)
                    .and_modify(|existing| {
                        let cand_key = (
                            candidate.max_src_depth,
                            candidate.sum_src_depth,
                            &candidate.sorted_sources,
                            &candidate.rule_iri,
                        );
                        let exist_key = (
                            existing.max_src_depth,
                            existing.sum_src_depth,
                            &existing.sorted_sources,
                            &existing.rule_iri,
                        );
                        if cand_key < exist_key {
                            *existing = candidate.clone();
                        }
                    })
                    .or_insert(candidate);
            }
        }

        if round.is_empty() {
            break; // stratum fixpoint
        }

        let mut new_delta: HashSet<FactKey> = HashSet::with_capacity(round.len());
        // Commit winners in FactKey order, not raw `HashMap` order: store/index
        // insertion order must be deterministic (the columnar-store determinism
        // doctrine), matching `least_model_of_reduct`'s commit discipline.
        let mut winners: Vec<(FactKey, RuleRoundCandidate)> = round.into_iter().collect();
        winners.sort_by(|(a, _), (b, _)| a.cmp(b));
        for (_key, winner) in winners {
            // The step/derivation budget is charged HERE — one step per committed
            // derivation, at the deterministic FactKey-sorted boundary.  When the budget
            // is spent we stop BEFORE committing this winner, leaving a sound
            // (FactKey-ordered) partial prefix: `max_steps = n` admits exactly `n`
            // derivations, `max_steps = 0` admits none.  Every committed fact is
            // genuinely in the least model — the outcome is incomplete, never wrong.
            if governor.spent() {
                return Ok(FixpointStatus::Exhausted);
            }
            let winner_depth = winner.max_src_depth.saturating_add(1);
            depth.insert(winner.key.clone(), winner_depth);
            // Insert into both stores in lockstep so the columnar index order tracks
            // the ternary store's insertion order exactly.
            if store.insert(winner.head.clone()) {
                rel.insert(
                    &winner.head.predicate,
                    winner.head.subject.clone(),
                    winner.head.object.clone(),
                );
            }
            new_delta.insert(winner.key.clone());

            // A winner is always a NEW key: heads already present (including every
            // EDB fact, seeded into `store` before the fixpoint) are skipped above via
            // `store.contains_key`. So every winner is a genuine derivation.
            derivations.push(DerivedRow {
                graph: String::new(),
                subject: winner.head.subject,
                predicate: winner.head.predicate,
                object: winner.head.object,
                rule_iri: winner.rule_iri,
                source_quad_ids: winner.sources, // body-order, NEVER the sorted copy
                derivation_id: winner.deriv,
            });
            governor.charge();
        }

        delta = new_delta;
    }

    Ok(FixpointStatus::Complete)
}

// ── RelationStore-seeded bottom-up entry (the backward leg's evaluator) ───────────

/// Evaluate `rules` bottom-up over a [`RelationStore`] EDB, returning the FULL derived
/// fact set (EDB ∪ derived) of the stratified least model.
///
/// This is the [`RelationStore`]-seeded sibling of [`materialize_native`]: the backward
/// (`resolve_native`) leg magic-transforms a query into binary [`EvalRule`]s, extracts the
/// world EDB columnar-form via [`crate::physical::store::extract_edb`], seeds the magic
/// fact(s), and runs the SAME stratified semi-naive fixpoint the forward path uses.  The
/// caller then reads the goal predicate's tuples out of the returned facts.  Provenance
/// (`DerivedRow`) is not needed for answer projection, so this entry returns bare
/// [`Fact`]s (EDB seeded at depth 0, derived facts accreted by the fixpoint).
///
/// The EDB facts are seeded in a deterministic sorted-key order (matching
/// `world_edb_facts`' seed discipline) so the fixpoint is reproducible run-to-run.
///
/// # Errors
///
/// Returns `Err` for an unbound head/guard variable or a provenance-recipe failure
/// (propagated from the shared `rule_ir` helpers); `Ok(Unsupported(NonStratifiable))` if
/// the (transformed) rules carry a negative edge inside a cycle.
pub(crate) fn evaluate(
    edb: RelationStore,
    rules: &[EvalRule],
    max_steps: Option<u64>,
) -> Result<NativeOutcome<Budgeted<Vec<Fact>>>, String> {
    let Some(stratum_of) = stratify(rules) else {
        return Ok(NativeOutcome::Unsupported(UnsupportedKind::NonStratifiable));
    };

    let max_stratum = rules
        .iter()
        .map(|r| stratum_of[r.head.predicate.as_str()])
        .max()
        .unwrap_or(0);
    let mut rules_by_stratum: Vec<Vec<&EvalRule>> = vec![Vec::new(); max_stratum + 1];
    for rule in rules {
        let s = stratum_of[rule.head.predicate.as_str()];
        rules_by_stratum[s].push(rule);
    }

    // Lower the columnar EDB into the ternary `Fact` seed in sorted-key order so the
    // semi-naive seed order is deterministic (mirrors `world_edb_facts`).
    let mut edb_facts: Vec<Fact> = Vec::new();
    for pred in edb.predicates() {
        // `pred` is a predicate IRI surface already validated by the seam; carry it directly.
        let predicate = pred.to_owned();
        for (subject, object) in edb.select(pred, Bound::Any) {
            edb_facts.push(Fact {
                subject,
                predicate: predicate.clone(),
                object,
            });
        }
    }
    edb_facts.sort_by_key(Fact::key);

    // Run the stratified fixpoint, accumulating into a shared FactStore/RelationStore.
    let mut store = FactStore::new();
    let mut rel = RelationStore::new();
    let mut depth: HashMap<FactKey, u32> = HashMap::new();

    // A PURE-EDB predicate (never a rule head) is settled from the seed; a self-recursive
    // or otherwise IDB-derived predicate becomes settled only when its stratum completes
    // (below), so exclude the head predicates here to avoid over-claiming.
    let head_preds = head_predicates(&rules_by_stratum);
    let mut saturated_preds: BTreeSet<String> = edb_facts
        .iter()
        .map(|f| f.predicate.clone())
        .filter(|p| !head_preds.contains(p))
        .collect();

    for f in &edb_facts {
        depth.insert(f.key(), 0);
        if store.insert(f.clone()) {
            rel.insert(&f.predicate, f.subject.clone(), f.object.clone());
        }
    }

    // This leg returns the full fact set (`store.facts()`); the derivation rows are
    // unused here but the shared fixpoint records them for the forward leg.  The step
    // governor is honoured identically to the forward path (single EDB, so the frontier
    // is exact — no cross-world under-claim).
    let mut governor = StepGovernor::new(max_steps);
    let total = rules_by_stratum.len();
    let mut completed = 0usize;
    let mut status = BudgetStatus::Ok;
    let mut derivations: Vec<DerivedRow> = Vec::new();
    // Set iff a builtin could not be evaluated in its binding mode, or hit a
    // domain/precision error (÷0, overflow).  Such a program is a declared native
    // gap: the whole query re-demotes to the oracle rather than present an
    // incomplete answer set — never a wrong answer.
    let mut builtin_gap = false;
    for stratum_rules in &rules_by_stratum {
        if stratum_rules.is_empty() {
            completed += 1;
            continue;
        }
        match eval_stratum_fixpoint(
            stratum_rules,
            &mut store,
            &mut rel,
            &mut depth,
            &mut derivations,
            &mut governor,
            &mut builtin_gap,
        )? {
            FixpointStatus::Complete => {
                for rule in stratum_rules {
                    saturated_preds.insert(rule.head.predicate.as_str().to_owned());
                }
                completed += 1;
            }
            FixpointStatus::Exhausted => {
                status = BudgetStatus::Exhausted;
                break;
            }
        }
    }

    if builtin_gap {
        return Ok(NativeOutcome::Unsupported(UnsupportedKind::Arithmetic));
    }

    Ok(NativeOutcome::Decided(Budgeted {
        rows: store.facts().to_vec(),
        status,
        progress: StrataProgress {
            completed,
            total,
            saturated_preds,
        },
        consumed_steps: governor.consumed,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provenance::term_display;
    use crate::rule_ir::{EvalTerm, least_model_of_reduct, parse_eval_rules};
    use crate::store::WorldStore;
    use purrdf::TermValue;

    const NS: &str = "https://example.org/p3/";

    fn nn(local: &str) -> String {
        format!("{NS}{local}")
    }

    fn term(local: &str) -> TermValue {
        TermValue::iri(nn(local))
    }

    fn fact(s: &str, p: &str, o: &str) -> Fact {
        Fact {
            subject: term(s),
            predicate: nn(p),
            object: term(o),
        }
    }

    /// Transitive-closure rules in the 3-ary `pred(s, o, world)` encoding.
    ///
    /// `path(?X,?Z) :- edge(?X,?Z)` and `path(?X,?Z) :- edge(?X,?Y), path(?Y,?Z)`.
    fn tc_rules() -> Vec<EvalRule> {
        let rls = format!(
            "#[name(\"{NS}ruleBase\")]\n\
             <{NS}path>(?X, ?Z, ?W) :- <{NS}edge>(?X, ?Z, ?W) .\n\
             #[name(\"{NS}ruleStep\")]\n\
             <{NS}path>(?X, ?Z, ?W) :-\n\
                 <{NS}edge>(?X, ?Y, ?W),\n\
                 <{NS}path>(?Y, ?Z, ?W) .\n"
        );
        parse_eval_rules(&rls).expect("parse TC rules")
    }

    const WORLD: &str = "https://example.org/p3/world";

    /// A `WorldStore` with the edge chain a→b→c→d in one world.
    fn tc_store() -> WorldStore {
        let store = WorldStore::new();
        for (s, o) in [("a", "b"), ("b", "c"), ("c", "d")] {
            store.insert_quad(
                WORLD,
                &format!("{NS}{s}"),
                &format!("{NS}edge"),
                &format!("{NS}{o}"),
            );
        }
        store
    }

    /// Build a `FactStore` EDB equivalent to `tc_store`'s edges (sorted by key, as
    /// `world_edb_facts` would yield) for driving `least_model_of_reduct` directly.
    fn tc_edb_factstore() -> FactStore {
        let mut edb = FactStore::new();
        // Sort by FactKey to match world_edb_facts' deterministic seed order.
        let mut facts = vec![
            fact("a", "edge", "b"),
            fact("b", "edge", "c"),
            fact("c", "edge", "d"),
        ];
        facts.sort_by_key(Fact::key);
        for f in facts {
            edb.insert(f);
        }
        edb
    }

    fn derived_only(rows: &[DerivedRow]) -> Vec<&DerivedRow> {
        rows.iter()
            .filter(|r| r.rule_iri != crate::provenance::ASSERT_RULE_IRI)
            .collect()
    }

    /// A canonical comparison tuple for a derived row's full provenance.
    fn row_key(r: &DerivedRow) -> (String, String, String, String, Vec<String>, String) {
        (
            term_display(&r.subject),
            r.predicate.as_str().to_owned(),
            term_display(&r.object),
            r.rule_iri.clone(),
            r.source_quad_ids.clone(),
            r.derivation_id.clone(),
        )
    }

    /// THE DETERMINISM GATE.
    ///
    /// The native evaluator must produce byte-identical derived-row provenance to the
    /// reference `least_model_of_reduct(edb, rules, &empty)` for a single-stratum
    /// POSITIVE program (transitive closure).  Byte-identity holds because the native
    /// round loop is a structural copy of `least_model_of_reduct`'s loop with ONLY
    /// the join substituted by index selection over an insertion-ordered store filled
    /// in lockstep with the ternary store — so the solution sequence, `source_facts`
    /// order, and the first-wins tiebreak are identical.
    #[test]
    fn physical_native_matches_reference_byte_identical() {
        let rules = tc_rules();

        // Reference path: least model of the reduct against an EMPTY reference (a
        // positive program's reduct is itself; its least model is the least model).
        let edb = tc_edb_factstore();
        let reference = least_model_of_reduct(&edb, &rules, &FactStore::new()).expect("lmr");
        let mut ref_rows: Vec<(String, String, String, String, Vec<String>, String)> = reference
            .derivations
            .iter()
            .map(|r| {
                (
                    term_display(&r.subject),
                    r.predicate.as_str().to_owned(),
                    term_display(&r.object),
                    r.rule_iri.clone(),
                    r.source_quad_ids.clone(),
                    r.derivation_id.clone(),
                )
            })
            .collect();
        ref_rows.sort();

        // Native path via a WorldStore.
        let store = tc_store();
        let outcome = materialize_native(&store, &rules, None).expect("materialize_native");
        let NativeOutcome::Decided(Budgeted { rows, .. }) = outcome else {
            panic!("expected Decided for a stratifiable positive program");
        };
        let mut native_rows: Vec<_> = derived_only(&rows).iter().map(|r| row_key(r)).collect();
        native_rows.sort();

        assert_eq!(
            native_rows, ref_rows,
            "native derived provenance must be byte-identical to least_model_of_reduct"
        );
        assert!(
            !native_rows.is_empty(),
            "transitive closure must derive at least one path"
        );
    }

    /// Transitive closure correctness: every reachable pair is in the `path` relation.
    #[test]
    fn physical_transitive_closure_reaches_all_pairs() {
        let rules = tc_rules();
        let store = tc_store();
        let outcome = materialize_native(&store, &rules, None).expect("materialize_native");
        let NativeOutcome::Decided(Budgeted { rows, .. }) = outcome else {
            panic!("expected Decided");
        };

        // Collect derived path pairs (subject, object) local names.
        let path_pred = format!("{NS}path");
        let mut pairs: BTreeSet<(String, String)> = BTreeSet::new();
        for r in derived_only(&rows) {
            if r.predicate.as_str() == path_pred {
                pairs.insert((term_display(&r.subject), term_display(&r.object)));
            }
        }
        // Reachable closure of a→b→c→d: ab ac ad bc bd cd.
        let want: BTreeSet<(String, String)> = [
            ("a", "b"),
            ("a", "c"),
            ("a", "d"),
            ("b", "c"),
            ("b", "d"),
            ("c", "d"),
        ]
        .into_iter()
        .map(|(s, o)| (format!("<{NS}{s}>"), format!("<{NS}{o}>")))
        .collect();
        assert_eq!(
            pairs, want,
            "transitive closure must reach all reachable pairs"
        );
    }

    /// Stratified negation: `unreachable(?X) :- node(?X), ~reachable(?X)` over a
    /// `reachable` relation built in a lower stratum.
    #[test]
    fn physical_stratified_negation_matches_expected() {
        // node(a), node(b), node(c); reachableSeed(a); reachable closure over edge.
        // edge(a,b). reachable(?X) :- reachableSeed(?X). reachable(?Y) :- reachable(?X), edge(?X,?Y).
        // unreachable(?X) :- node(?X), ~reachable(?X).
        let rls = format!(
            "#[name(\"{NS}rReachSeed\")]\n\
             <{NS}reachable>(?X, ?X, ?W) :- <{NS}reachableSeed>(?X, ?X, ?W) .\n\
             #[name(\"{NS}rReachStep\")]\n\
             <{NS}reachable>(?Y, ?Y, ?W) :-\n\
                 <{NS}reachable>(?X, ?X, ?W),\n\
                 <{NS}edge>(?X, ?Y, ?W) .\n\
             #[name(\"{NS}rUnreach\")]\n\
             <{NS}unreachable>(?X, ?X, ?W) :-\n\
                 <{NS}node>(?X, ?X, ?W),\n\
                 ~<{NS}reachable>(?X, ?X, ?W) .\n"
        );
        let rules = parse_eval_rules(&rls).expect("parse stratified rules");

        let store = WorldStore::new();
        // node(a), node(b), node(c) — encoded as self-loop (s == o) to match the rules.
        for x in ["a", "b", "c"] {
            store.insert_quad(
                WORLD,
                &format!("{NS}{x}"),
                &format!("{NS}node"),
                &format!("{NS}{x}"),
            );
        }
        // reachableSeed(a).
        store.insert_quad(
            WORLD,
            &format!("{NS}a"),
            &format!("{NS}reachableSeed"),
            &format!("{NS}a"),
        );
        // edge(a,b): so b is reachable; c is NOT.
        store.insert_quad(
            WORLD,
            &format!("{NS}a"),
            &format!("{NS}edge"),
            &format!("{NS}b"),
        );

        let outcome = materialize_native(&store, &rules, None).expect("materialize_native");
        let NativeOutcome::Decided(Budgeted { rows, .. }) = outcome else {
            panic!("expected Decided for a stratifiable program");
        };

        // reachable = {a, b}; unreachable = {c}.
        let reach_pred = format!("{NS}reachable");
        let unreach_pred = format!("{NS}unreachable");
        let mut reachable: BTreeSet<String> = BTreeSet::new();
        let mut unreachable: BTreeSet<String> = BTreeSet::new();
        let mut unreach_has_provenance = false;
        for r in derived_only(&rows) {
            if r.predicate.as_str() == reach_pred {
                reachable.insert(term_display(&r.subject));
            } else if r.predicate.as_str() == unreach_pred {
                unreachable.insert(term_display(&r.subject));
                if !r.source_quad_ids.is_empty() && !r.derivation_id.is_empty() {
                    unreach_has_provenance = true;
                }
            }
        }
        let want_reach: BTreeSet<String> = ["a", "b"]
            .into_iter()
            .map(|x| format!("<{NS}{x}>"))
            .collect();
        let want_unreach: BTreeSet<String> =
            ["c"].into_iter().map(|x| format!("<{NS}{x}>")).collect();
        assert_eq!(reachable, want_reach, "reachable must be {{a,b}}");
        assert_eq!(unreachable, want_unreach, "unreachable must be {{c}}");
        assert!(
            unreach_has_provenance,
            "derived unreachable rows must carry provenance"
        );
    }

    /// A non-stratifiable program (negative edge in a cycle) is a declared gap.
    #[test]
    fn physical_non_stratifiable_is_unsupported() {
        // p(?X) :- ~q(?X). q(?X) :- ~p(?X).  (self-loop encoding for the unary preds.)
        let rls = format!(
            "#[name(\"{NS}rP\")]\n\
             <{NS}p>(?X, ?X, ?W) :- <{NS}dom>(?X, ?X, ?W), ~<{NS}q>(?X, ?X, ?W) .\n\
             #[name(\"{NS}rQ\")]\n\
             <{NS}q>(?X, ?X, ?W) :- <{NS}dom>(?X, ?X, ?W), ~<{NS}p>(?X, ?X, ?W) .\n"
        );
        let rules = parse_eval_rules(&rls).expect("parse cyclic-negation rules");

        let store = WorldStore::new();
        store.insert_quad(
            WORLD,
            &format!("{NS}a"),
            &format!("{NS}dom"),
            &format!("{NS}a"),
        );

        let outcome = materialize_native(&store, &rules, None).expect("materialize_native");
        assert!(
            matches!(
                outcome,
                NativeOutcome::Unsupported(UnsupportedKind::NonStratifiable)
            ),
            "p↔q via mutual negation must be reported non-stratifiable"
        );
    }

    /// Stratification of a positive-only program puts everything in stratum 0 and a
    /// cross-stratum negation lifts the negated head above its body.
    #[test]
    fn physical_stratify_assigns_negation_above_body() {
        let rls = format!(
            "#[name(\"{NS}rB\")]\n\
             <{NS}b>(?X, ?X, ?W) :- <{NS}a>(?X, ?X, ?W) .\n\
             #[name(\"{NS}rC\")]\n\
             <{NS}c>(?X, ?X, ?W) :- <{NS}dom>(?X, ?X, ?W), ~<{NS}b>(?X, ?X, ?W) .\n"
        );
        let rules = parse_eval_rules(&rls).expect("parse");
        let strata = stratify(&rules).expect("stratifiable");
        let s_b = strata[&format!("{NS}b")];
        let s_c = strata[&format!("{NS}c")];
        assert!(
            s_c > s_b,
            "c negates b, so stratum(c) ({s_c}) must exceed stratum(b) ({s_b})"
        );
    }

    // ── Step/derivation budget governor ────────────────────────────────────────────

    /// Run the forward engine, asserting a `Decided` outcome, and return the budget carrier.
    fn materialize_budgeted(
        store: &WorldStore,
        rules: &[EvalRule],
        max_steps: Option<u64>,
    ) -> Budgeted<Vec<DerivedRow>> {
        match materialize_native(store, rules, max_steps).expect("materialize_native") {
            NativeOutcome::Decided(b) => b,
            other => panic!("expected Decided, got {other:?}"),
        }
    }

    /// A two-stratum program: `reachable` (stratum 0) via seed + edge-step, then
    /// `unreachable` (stratum 1) via stratified negation of `reachable`.  Over
    /// `reachableSeed(a)` and `edge(a,b)` with nodes {a,b,c}: `reachable = {a,b}` (two
    /// derivations) and `unreachable = {c}` (one derivation) — three derivations total,
    /// with the `unreachable` derivation strictly after both `reachable` derivations.
    fn reach_program() -> (Vec<EvalRule>, WorldStore) {
        let rls = format!(
            "#[name(\"{NS}rReachSeed\")]\n\
             <{NS}reachable>(?X, ?X, ?W) :- <{NS}reachableSeed>(?X, ?X, ?W) .\n\
             #[name(\"{NS}rReachStep\")]\n\
             <{NS}reachable>(?Y, ?Y, ?W) :-\n\
                 <{NS}reachable>(?X, ?X, ?W),\n\
                 <{NS}edge>(?X, ?Y, ?W) .\n\
             #[name(\"{NS}rUnreach\")]\n\
             <{NS}unreachable>(?X, ?X, ?W) :-\n\
                 <{NS}node>(?X, ?X, ?W),\n\
                 ~<{NS}reachable>(?X, ?X, ?W) .\n"
        );
        let rules = parse_eval_rules(&rls).expect("parse reach program");
        let store = WorldStore::new();
        for x in ["a", "b", "c"] {
            store.insert_quad(WORLD, &nn(x), &nn("node"), &nn(x));
        }
        store.insert_quad(WORLD, &nn("a"), &nn("reachableSeed"), &nn("a"));
        store.insert_quad(WORLD, &nn("a"), &nn("edge"), &nn("b"));
        (rules, store)
    }

    /// A zero step budget derives NOTHING: `Exhausted`, no strata saturated, and zero
    /// DERIVED rows (the EDB echo still emits — the budget bounds derivations, not the
    /// asserted base facts).  This is the boundary the reference oracle stamps at 0.
    #[test]
    fn physical_materialize_zero_budget_derives_nothing() {
        let rules = tc_rules();
        let store = tc_store();
        let b = materialize_budgeted(&store, &rules, Some(0));
        assert_eq!(
            b.status,
            BudgetStatus::Exhausted,
            "0-step budget ⇒ Exhausted"
        );
        assert_eq!(
            b.consumed_steps, 0,
            "no derivation may be committed at budget 0"
        );
        assert_eq!(b.progress.completed, 0, "no stratum saturated at budget 0");
        assert!(
            derived_only(&b.rows).is_empty(),
            "0-step budget must derive zero rows (EDB echo aside)"
        );
        // The EDB echo is still present — the budget governs derivations, not base facts.
        assert!(
            b.rows.len() >= 3,
            "the three asserted edges must still be echoed"
        );
    }

    /// A budget larger than the completion cost is byte-identical to the unbounded run:
    /// `Ok`, every stratum saturated, and the SAME derived rows.  The governor never
    /// trips, so an over-budgeted run cannot diverge from the pre-governor engine.
    #[test]
    fn physical_materialize_huge_budget_matches_unbounded() {
        let rules = tc_rules();
        let store = tc_store();
        let unbounded = materialize_budgeted(&store, &rules, None);
        let huge = materialize_budgeted(&store, &rules, Some(1_000_000));
        assert_eq!(huge.status, BudgetStatus::Ok, "an ample budget completes");
        assert_eq!(
            huge.progress.completed, huge.progress.total,
            "every stratum saturated under an ample budget"
        );
        assert!(huge.consumed_steps > 0, "TC derives at least one path");
        assert_eq!(
            unbounded.consumed_steps, huge.consumed_steps,
            "consumed-step count is the deterministic completion cost"
        );
        let key = |rows: &[DerivedRow]| -> Vec<_> {
            let mut k: Vec<_> = derived_only(rows).iter().map(|r| row_key(r)).collect();
            k.sort();
            k
        };
        assert_eq!(
            key(&unbounded.rows),
            key(&huge.rows),
            "an over-budgeted run must be byte-identical to the unbounded run"
        );
    }

    /// A mid-run budget is DETERMINISTIC: run twice at the same intermediate budget and
    /// the derived prefix, status, and consumed-step count are identical (the cut is the
    /// Nth FactKey-sorted committed winner).  The prefix is also a strict, sound subset
    /// of the unbounded derivations.
    #[test]
    fn physical_materialize_mid_budget_is_deterministic() {
        let rules = tc_rules();
        let store = tc_store();
        let full = materialize_budgeted(&store, &rules, None);
        let full_derived = derived_only(&full.rows).len();
        assert!(full_derived > 2, "TC must derive more than 2 paths");

        let run1 = materialize_budgeted(&store, &rules, Some(2));
        let run2 = materialize_budgeted(&store, &rules, Some(2));
        assert_eq!(
            run1.status,
            BudgetStatus::Exhausted,
            "budget 2 < {full_derived} ⇒ Exhausted"
        );
        assert_eq!(run1.consumed_steps, 2, "exactly 2 derivations committed");
        let key = |rows: &[DerivedRow]| -> Vec<_> {
            let mut k: Vec<_> = derived_only(rows).iter().map(|r| row_key(r)).collect();
            k.sort();
            k
        };
        assert_eq!(
            key(&run1.rows),
            key(&run2.rows),
            "the mid-budget prefix must be byte-identical run-to-run"
        );
        assert_eq!(
            derived_only(&run1.rows).len(),
            2,
            "the prefix holds exactly `max_steps` derivations"
        );
        // Soundness: every prefix derivation is genuinely in the unbounded model.
        let full_keys: BTreeSet<_> = derived_only(&full.rows)
            .iter()
            .map(|r| row_key(r))
            .collect();
        for r in derived_only(&run1.rows) {
            assert!(
                full_keys.contains(&row_key(r)),
                "a budget-cut derivation must be sound (present in the full model)"
            );
        }
    }

    /// The completion frontier records which strata are settled: a budget that saturates
    /// the LOWER stratum (`reachable`) but cuts the higher one (`unreachable`) reports
    /// `reachable` (and the EDB predicates) as saturated and `unreachable` as NOT — the
    /// per-stratum settledness the frontier-aware 5-field mapping reads.
    #[test]
    fn physical_materialize_frontier_records_saturated_lower_stratum() {
        let (rules, store) = reach_program();

        // Full run: both strata saturate; `unreachable(c)` is derived.
        let full = materialize_budgeted(&store, &rules, None);
        assert_eq!(full.status, BudgetStatus::Ok);
        assert!(full.progress.saturated_preds.contains(&nn("reachable")));
        assert!(full.progress.saturated_preds.contains(&nn("unreachable")));

        // Budget 2 saturates `reachable` (two derivations) then cuts before the single
        // `unreachable` derivation.
        let cut = materialize_budgeted(&store, &rules, Some(2));
        assert_eq!(
            cut.status,
            BudgetStatus::Exhausted,
            "budget 2 cuts stratum 1"
        );
        assert_eq!(cut.consumed_steps, 2);
        assert_eq!(
            cut.progress.completed, 1,
            "only the lower stratum saturated"
        );
        assert!(
            cut.progress.saturated_preds.contains(&nn("reachable")),
            "reachable's stratum completed ⇒ it is settled"
        );
        assert!(
            cut.progress.saturated_preds.contains(&nn("node"))
                && cut.progress.saturated_preds.contains(&nn("edge")),
            "EDB predicates are settled from the seed"
        );
        assert!(
            !cut.progress.saturated_preds.contains(&nn("unreachable")),
            "unreachable's stratum was cut ⇒ it is NOT settled"
        );
        // The unreachable fact must NOT have been derived under the cut budget.
        let unreach = nn("unreachable");
        assert!(
            derived_only(&cut.rows)
                .iter()
                .all(|r| r.predicate.as_str() != unreach),
            "no unreachable row may be derived after the budget cut"
        );
    }

    // ── Arithmetic/comparison builtin evaluation in the seminaive engine ──────
    //
    // These drive the post-join constraint stage directly by constructing an
    // `EvalRule` carrying builtins (the shape the magic backward transform
    // produces), independent of the magic front-end.

    const XSD_INT: &str = "http://www.w3.org/2001/XMLSchema#integer";

    fn int_lit(n: i64) -> TermValue {
        TermValue::typed_literal(n.to_string(), XSD_INT)
    }

    fn var_atom(subject: &str, pred: &str, object: &str) -> EvalAtom {
        EvalAtom {
            subject: EvalTerm::Var(subject.to_owned()),
            predicate: nn(pred),
            object: EvalTerm::Var(object.to_owned()),
            negated: false,
        }
    }

    /// A single-fact `RelationStore` EDB `val(<a>, <n as xsd:integer>)`.
    fn val_edb(pairs: &[(&str, i64)]) -> RelationStore {
        let mut edb = RelationStore::new();
        for (s, n) in pairs {
            edb.insert(&nn("val"), term(s), int_lit(*n));
        }
        edb
    }

    fn is_builtin(
        target: &str,
        lhs: &str,
        op: crate::query_ir::ArithOp,
        rhs: QBuiltinRhs,
    ) -> QBuiltin {
        use crate::query_ir::QTerm;
        QBuiltin::Is {
            target: QTerm::Var(target.to_owned()),
            lhs: QTerm::Var(lhs.to_owned()),
            op,
            rhs: match rhs {
                QBuiltinRhs::Var(v) => QTerm::Var(v.to_owned()),
                QBuiltinRhs::Num(n) => QTerm::Num(n),
            },
        }
    }

    enum QBuiltinRhs {
        Var(&'static str),
        Num(i64),
    }

    /// The `evaluate` result facts as a set of `(subject, predicate, object)` displays.
    fn fact_keys(facts: &[Fact]) -> std::collections::BTreeSet<FactKey> {
        facts.iter().map(Fact::key).collect()
    }

    #[test]
    fn seminaive_generator_binds_head_from_arithmetic() {
        use crate::query_ir::ArithOp;
        // result(?X, ?D) :- val(?X, ?N), D is N + 10 .
        let rule = EvalRule {
            head: var_atom("?X", "result", "?D"),
            body: vec![var_atom("?X", "val", "?N")],
            rule_iri: nn("rule/result"),
            distinct_pairs: vec![],
            builtins: vec![is_builtin("?D", "?N", ArithOp::Add, QBuiltinRhs::Num(10))],
        };
        let out = evaluate(val_edb(&[("a", 2)]), &[rule], None).expect("evaluate");
        let NativeOutcome::Decided(budgeted) = out else {
            panic!("expected Decided, got a gap");
        };
        let keys = fact_keys(&budgeted.rows);
        // The generator bound ?D = 12, surfaced as the canonical typed integer.
        assert!(
            keys.contains(&(
                format!("<{}>", nn("a")),
                nn("result"),
                format!("\"12\"^^<{XSD_INT}>"),
            )),
            "expected result(a, 12): {keys:?}"
        );
    }

    #[test]
    fn seminaive_comparison_filters_solutions() {
        use crate::query_ir::{CmpOp, QTerm};
        // big(?X) as big(?X, ?X) :- val(?X, ?N), N > 5 .   (binary head encoding)
        let rule = EvalRule {
            head: var_atom("?X", "big", "?X"),
            body: vec![var_atom("?X", "val", "?N")],
            rule_iri: nn("rule/big"),
            distinct_pairs: vec![],
            builtins: vec![QBuiltin::Compare {
                lhs: QTerm::Var("?N".to_owned()),
                op: CmpOp::Gt,
                rhs: QTerm::Num(5),
            }],
        };
        let out = evaluate(val_edb(&[("a", 2), ("b", 9)]), &[rule], None).expect("evaluate");
        let NativeOutcome::Decided(budgeted) = out else {
            panic!("expected Decided, got a gap");
        };
        let keys = fact_keys(&budgeted.rows);
        // Only b (9 > 5) survives the filter; a (2) is pruned.
        assert!(
            keys.contains(&(
                format!("<{}>", nn("b")),
                nn("big"),
                format!("<{}>", nn("b"))
            )),
            "b passes the filter: {keys:?}"
        );
        assert!(
            !keys.contains(&(
                format!("<{}>", nn("a")),
                nn("big"),
                format!("<{}>", nn("a"))
            )),
            "a must be filtered out: {keys:?}"
        );
    }

    #[test]
    fn seminaive_overflow_is_declared_gap_not_wrong_answer() {
        use crate::query_ir::ArithOp;
        // over(?X, ?D) :- val(?X, ?N), D is N + i64::MAX .  → overflow → declared gap.
        let rule = EvalRule {
            head: var_atom("?X", "over", "?D"),
            body: vec![var_atom("?X", "val", "?N")],
            rule_iri: nn("rule/over"),
            distinct_pairs: vec![],
            builtins: vec![is_builtin(
                "?D",
                "?N",
                ArithOp::Add,
                QBuiltinRhs::Num(i64::MAX),
            )],
        };
        let out = evaluate(val_edb(&[("a", 1)]), &[rule], None).expect("evaluate");
        assert!(
            matches!(out, NativeOutcome::Unsupported(UnsupportedKind::Arithmetic)),
            "overflow must be a declared Arithmetic gap, never a wrong answer"
        );
    }
}
