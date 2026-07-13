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
//! the SAME quality tiebreak
//! ([`RuleRoundCandidate`]'s total order `(proof_height, sum_src_depth,
//! sorted_sources, rule_iri, sources)`), same per-fact depth map (EDB depth 0; derived
//! depth = 1 + max source depth), and the same body-order `source_quad_ids`.  The ONLY
//! substitution is the join: `join_body`'s full-bucket scan becomes
//! [`join_body_indexed`]'s index-selected scan.  The winner tiebreak is a **total
//! order over observable provenance**, so byte-identity does NOT depend on the order in
//! which [`RelationStore::select`] enumerates rows: two derivations that would produce
//! different output bytes differ in the tiebreak key and the same winner is chosen
//! regardless of enumeration order.  For a single-stratum POSITIVE program the derived
//! rows therefore equal `least_model_of_reduct(edb, rules, &empty)` exactly; this is
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
//! # Internal helper coverage
//!
//! The production native paths and focused parity tests exercise different subsets
//! of this module's kernels. The module-level `dead_code` allowance keeps those
//! verification helpers together rather than scattering per-item attributes.
#![allow(dead_code)]

use std::borrow::Cow;
use std::collections::{BTreeSet, HashMap};

use hashbrown::HashTable;
use rayon::prelude::*;

use crate::physical::builtin_eval::{BuiltinOutcome, emit_integer_surface, eval as eval_builtin};
use crate::physical::cursor::{LendingIterator, VALUE_OBJECT, VALUE_SUBJECT, ValueCursor};
use crate::physical::id::{RowId, TermId};
use crate::physical::plan::{
    AtomKernel, AtomOperator, CyclicPlan, Executable, IndexChoice, JoinGroup, RulePlan,
};
use crate::physical::store::{Bound, RelationStore};
use crate::provenance::{MinProofHeightSemiring, ProofHeight, mint_derivation_id};
use crate::query_ir::QBuiltin;
use crate::rule_ir::{
    DerivedRow, EvalAtom, EvalRule, Fact, FactKey, FactStore, Provenance, RuleRoundCandidate,
    Solution, distinct_pairs_satisfied, echo_asserted, fact_key_hash, ground, ground_head,
    match_atom, sort_rows, world_edb_facts,
};
use crate::seam::BudgetStatus;

fn seminaive_err(detail: impl Into<String>) -> gmeow_errors::Diag {
    gmeow_errors::Diag::of_kind(crate::error::Physical {
        detail: detail.into(),
    })
}

/// A native-execution combination the forward core cannot decide.
///
/// Carried by [`NativeOutcome::Unsupported`]. `NonStratifiable` is the only variant
/// the forward semi-naive leg can raise; the others name combinations surfaced by
/// the native magic-sets, backward, and existential rungs.
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
    /// A negation-as-failure body atom whose variables are not range-restricted by a
    /// positive body atom — i.e. a variable is still unbound when the NAF goal is
    /// evaluated.  NAF over an unbound goal is unsound (it would test a single partial
    /// grounding rather than the intended universally-quantified absence), so the native
    /// core refuses it as a declared gap rather than return a wrong or empty `Decided`.
    /// The caller surfaces that typed refusal; no comparison evaluator is a fallback.
    Floundering,
    /// An existential-rule program whose termination the acyclicity certifier could
    /// not establish (outside the certified-terminating chase fragment). The native
    /// caller refuses it or runs it budgeted-partial — never a wrong or non-terminating
    /// result.
    NonTerminatingExistential,
    /// A backward program whose only path to divergence is arithmetic self-drive: an
    /// IDB predicate in a dependency cycle carries a value-generating `is` builtin whose
    /// result feeds the cyclic head, and the recursive rule has NO finite (EDB or
    /// strictly-lower-stratum) driver bounding the recursion.  Over the finite triple
    /// EDB every other backward Datalog program terminates; only such a value-generator
    /// can invent an unbounded stream of fresh Herbrand terms.  With no `max_steps`
    /// budget that is an unbounded hang, so the native core returns a typed refusal
    /// (incomplete-never-wrong); with a step budget the [`StepGovernor`] cuts it, so it
    /// is evaluated normally.
    NonTerminatingArithmetic,
    /// A rule with no POSITIVE body atom that is not a materializable ground fact — it
    /// carries only NAF and/or builtin literals — cannot drive bottom-up derivation: the
    /// semi-naive engine never fires a zero-positive-body rule (`join_body_binary`
    /// returns empty when the positive set is empty). It is therefore a declared gap
    /// routed to the oracle, rather than a rule the magic transform would emit and then
    /// trip its no-bodyless-positive-rule invariant.
    UnpositiveBody,
}

/// The result of a native-execution attempt: a decided value or a declared gap.
///
/// `Unsupported` is a FIRST-CLASS outcome, never a panic or a silent approximation.
/// The caller may apply an explicitly defined sound native transformation fallback or
/// surface the typed refusal; no external engine demotion route remains.
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

/// Whether a stratum's semi-naive fixpoint reached its natural end or was budget-cut.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FixpointStatus {
    /// The fixpoint reached `round.is_empty()` within budget.
    Complete,
    /// The `max_steps` budget was exhausted mid-fixpoint; the committed prefix is a
    /// sound (FactKey-ordered) partial least model.
    Exhausted,
}

/// Whether the stratified fixpoint records per-derivation provenance.
///
/// `Record` mints reifiers + a content-addressed derivation id per firing and pushes a
/// [`DerivedRow`] for every committed derivation — the forward `materialize_native` leg
/// (the proof-graph evaluation). `Skip` commits the identical fact set, insertion order,
/// and step budget but records nothing — the backward `evaluate` leg, which projects only
/// [`Fact`]s and discards `DerivedRow`s (the facts-only evaluation). Because every candidate
/// under one `FactKey` shares an identical head, the committed facts and their order are
/// independent of provenance recording; `Skip` therefore agrees with `Record` on facts,
/// order, and step count wherever `Record` succeeds, and is total on the input superset where
/// reifier minting is partial (an RDF-star source that `Record` hard-fails on).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProvenanceMode {
    /// Mint and retain full per-derivation provenance.
    Record,
    /// Commit facts only; record no provenance (no reifier minting, no derivation ids,
    /// no `DerivedRow`s, no depth bookkeeping).
    Skip,
}

/// How one semi-naive round schedules its immutable per-rule candidate work.
///
/// Production selects [`Parallel`](Self::Parallel). [`Sequential`](Self::Sequential)
/// remains an internal parity oracle: both policies feed the SAME lexical winner merge
/// and sorted commit, so scheduling can never affect bytes or budget observations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RoundExecution {
    /// Evaluate every rule directly into one round buffer in program order.
    Sequential,
    /// Evaluate rules into independent buffers, then merge them in program order.
    Parallel,
}

impl RoundExecution {
    /// Whether this round has enough independent work and workers to use Rayon.
    ///
    /// Single-rule strata and one-worker deterministic measurement pools stay on the
    /// allocation-minimal direct path; there is no parallelism to recover in either case.
    fn should_parallelize(self, rule_count: usize) -> bool {
        self == Self::Parallel && rule_count > 1 && rayon::current_num_threads() > 1
    }
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

/// Flat physical binding frame for the acyclic binary join.
///
/// Slots are assigned once by [`RulePlan`]. A row probe is therefore two direct indexed
/// reads instead of repeated linear searches over `(variable_name, value)` pairs. The
/// legacy named [`Solution`] is reconstructed once after the positive join because the
/// post-join builtin/NAF/head helpers remain the shared semantic authority.
#[derive(Clone)]
struct SlotSolution {
    bindings: Vec<Option<String>>,
    source_facts: Vec<Fact>,
}

impl SlotSolution {
    fn empty(slot_count: usize) -> Self {
        Self {
            bindings: vec![None; slot_count],
            source_facts: Vec::new(),
        }
    }

    fn get(&self, slot: usize) -> Option<&str> {
        self.bindings[slot].as_deref()
    }

    fn into_named(self, variables: &[String]) -> Solution {
        debug_assert_eq!(self.bindings.len(), variables.len());
        let bindings = variables
            .iter()
            .zip(self.bindings)
            .filter_map(|(name, value)| value.map(|value| (name.clone(), value)))
            .collect();
        Solution {
            bindings,
            source_facts: self.source_facts,
        }
    }
}

fn selected_fact(atom: &EvalAtom, rel: &RelationStore, subject: TermId, object: TermId) -> Fact {
    Fact {
        subject: rel.interner().resolve(subject).clone(),
        predicate: atom.predicate.clone(),
        object: rel.interner().resolve(object).clone(),
    }
}

/// One enum dispatch per atom invocation selects a statically-shaped kernel. The
/// const-generic scan selection remains outside the tuple loop as well.
fn extend_slot_solutions_indexed(
    operator: &AtomOperator,
    atom: &EvalAtom,
    rel: &RelationStore,
    delta: Delta,
    scan: Scan,
    solutions: &[SlotSolution],
) -> Vec<SlotSolution> {
    match scan {
        Scan::Delta => extend_slot_operator::<SCAN_DELTA>(operator, atom, rel, delta, solutions),
        Scan::Full => extend_slot_operator::<SCAN_FULL>(operator, atom, rel, delta, solutions),
        Scan::OldOnly => {
            extend_slot_operator::<SCAN_OLD_ONLY>(operator, atom, rel, delta, solutions)
        }
    }
}

fn extend_slot_operator<const SCAN: u8>(
    operator: &AtomOperator,
    atom: &EvalAtom,
    rel: &RelationStore,
    delta: Delta,
    solutions: &[SlotSolution],
) -> Vec<SlotSolution> {
    match (operator.kernel(), operator.index()) {
        (
            AtomKernel::Vars {
                subject_slot,
                object_slot,
            },
            IndexChoice::Any,
        ) => extend_slot_vars::<SCAN, INDEX_ANY>(
            atom,
            *subject_slot,
            *object_slot,
            rel,
            delta,
            solutions,
        ),
        (
            AtomKernel::Vars {
                subject_slot,
                object_slot,
            },
            IndexChoice::Subject,
        ) => extend_slot_vars::<SCAN, INDEX_SUBJECT>(
            atom,
            *subject_slot,
            *object_slot,
            rel,
            delta,
            solutions,
        ),
        (
            AtomKernel::Vars {
                subject_slot,
                object_slot,
            },
            IndexChoice::Object,
        ) => extend_slot_vars::<SCAN, INDEX_OBJECT>(
            atom,
            *subject_slot,
            *object_slot,
            rel,
            delta,
            solutions,
        ),
        (
            AtomKernel::Vars {
                subject_slot,
                object_slot,
            },
            IndexChoice::Both,
        ) => extend_slot_vars::<SCAN, INDEX_BOTH>(
            atom,
            *subject_slot,
            *object_slot,
            rel,
            delta,
            solutions,
        ),
        (
            AtomKernel::VarConst {
                subject_slot,
                object,
            },
            IndexChoice::Object,
        ) => extend_slot_var_const::<SCAN, INDEX_OBJECT>(
            atom,
            *subject_slot,
            object,
            rel,
            delta,
            solutions,
        ),
        (
            AtomKernel::VarConst {
                subject_slot,
                object,
            },
            IndexChoice::Both,
        ) => extend_slot_var_const::<SCAN, INDEX_BOTH>(
            atom,
            *subject_slot,
            object,
            rel,
            delta,
            solutions,
        ),
        (
            AtomKernel::ConstVar {
                subject,
                object_slot,
            },
            IndexChoice::Subject,
        ) => extend_slot_const_var::<SCAN, INDEX_SUBJECT>(
            atom,
            subject,
            *object_slot,
            rel,
            delta,
            solutions,
        ),
        (
            AtomKernel::ConstVar {
                subject,
                object_slot,
            },
            IndexChoice::Both,
        ) => extend_slot_const_var::<SCAN, INDEX_BOTH>(
            atom,
            subject,
            *object_slot,
            rel,
            delta,
            solutions,
        ),
        (AtomKernel::Consts { subject, object }, IndexChoice::Both) => {
            extend_slot_consts::<SCAN>(atom, subject, object, rel, delta, solutions)
        }
        _ => unreachable!("planner emits a term-shape-compatible index choice"),
    }
}

const INDEX_ANY: u8 = 0;
const INDEX_SUBJECT: u8 = 1;
const INDEX_OBJECT: u8 = 2;
const INDEX_BOTH: u8 = 3;

fn extend_slot_vars<const SCAN: u8, const INDEX: u8>(
    atom: &EvalAtom,
    subject_slot: usize,
    object_slot: usize,
    rel: &RelationStore,
    delta: Delta,
    solutions: &[SlotSolution],
) -> Vec<SlotSolution> {
    let mut next = Vec::new();
    for solution in solutions {
        let bound = match INDEX {
            INDEX_ANY => Bound::Any,
            INDEX_SUBJECT => {
                let Some(subject) = solution
                    .get(subject_slot)
                    .and_then(|value| rel.term_id(value))
                else {
                    continue;
                };
                Bound::Subject(subject)
            }
            INDEX_OBJECT => {
                let Some(object) = solution
                    .get(object_slot)
                    .and_then(|value| rel.term_id(value))
                else {
                    continue;
                };
                Bound::Object(object)
            }
            INDEX_BOTH => {
                let (Some(subject), Some(object)) = (
                    solution
                        .get(subject_slot)
                        .and_then(|value| rel.term_id(value)),
                    solution
                        .get(object_slot)
                        .and_then(|value| rel.term_id(value)),
                ) else {
                    continue;
                };
                Bound::Both(subject, object)
            }
            _ => unreachable!("INDEX is a planned index code"),
        };
        let mut cursor = rel.select(atom.predicate.as_str(), bound);
        while let Some((subject_id, object_id, row_id)) = cursor.next() {
            if !keep_row::<SCAN>(delta, row_id)
                || (subject_slot == object_slot && subject_id != object_id)
            {
                continue;
            }
            let mut merged = solution.clone();
            if INDEX == INDEX_ANY || INDEX == INDEX_OBJECT {
                merged.bindings[subject_slot] =
                    Some(rel.interner().display_of(subject_id).to_owned());
            }
            if (INDEX == INDEX_ANY || INDEX == INDEX_SUBJECT) && object_slot != subject_slot {
                merged.bindings[object_slot] =
                    Some(rel.interner().display_of(object_id).to_owned());
            }
            merged
                .source_facts
                .push(selected_fact(atom, rel, subject_id, object_id));
            next.push(merged);
        }
    }
    next
}

fn extend_slot_var_const<const SCAN: u8, const INDEX: u8>(
    atom: &EvalAtom,
    subject_slot: usize,
    object: &str,
    rel: &RelationStore,
    delta: Delta,
    solutions: &[SlotSolution],
) -> Vec<SlotSolution> {
    let Some(object_id) = rel.term_id(object) else {
        return Vec::new();
    };
    let mut next = Vec::new();
    for solution in solutions {
        let bound = match INDEX {
            INDEX_OBJECT => Bound::Object(object_id),
            INDEX_BOTH => {
                let Some(subject_id) = solution
                    .get(subject_slot)
                    .and_then(|value| rel.term_id(value))
                else {
                    continue;
                };
                Bound::Both(subject_id, object_id)
            }
            _ => unreachable!("VarConst uses Object or Both index"),
        };
        let mut cursor = rel.select(atom.predicate.as_str(), bound);
        while let Some((subject_id, selected_object, row_id)) = cursor.next() {
            if !keep_row::<SCAN>(delta, row_id) {
                continue;
            }
            let mut merged = solution.clone();
            if INDEX == INDEX_OBJECT {
                merged.bindings[subject_slot] =
                    Some(rel.interner().display_of(subject_id).to_owned());
            }
            merged
                .source_facts
                .push(selected_fact(atom, rel, subject_id, selected_object));
            next.push(merged);
        }
    }
    next
}

fn extend_slot_const_var<const SCAN: u8, const INDEX: u8>(
    atom: &EvalAtom,
    subject: &str,
    object_slot: usize,
    rel: &RelationStore,
    delta: Delta,
    solutions: &[SlotSolution],
) -> Vec<SlotSolution> {
    let Some(subject_id) = rel.term_id(subject) else {
        return Vec::new();
    };
    let mut next = Vec::new();
    for solution in solutions {
        let bound = match INDEX {
            INDEX_SUBJECT => Bound::Subject(subject_id),
            INDEX_BOTH => {
                let Some(object_id) = solution
                    .get(object_slot)
                    .and_then(|value| rel.term_id(value))
                else {
                    continue;
                };
                Bound::Both(subject_id, object_id)
            }
            _ => unreachable!("ConstVar uses Subject or Both index"),
        };
        let mut cursor = rel.select(atom.predicate.as_str(), bound);
        while let Some((selected_subject, object_id, row_id)) = cursor.next() {
            if !keep_row::<SCAN>(delta, row_id) {
                continue;
            }
            let mut merged = solution.clone();
            if INDEX == INDEX_SUBJECT {
                merged.bindings[object_slot] =
                    Some(rel.interner().display_of(object_id).to_owned());
            }
            merged
                .source_facts
                .push(selected_fact(atom, rel, selected_subject, object_id));
            next.push(merged);
        }
    }
    next
}

fn extend_slot_consts<const SCAN: u8>(
    atom: &EvalAtom,
    subject: &str,
    object: &str,
    rel: &RelationStore,
    delta: Delta,
    solutions: &[SlotSolution],
) -> Vec<SlotSolution> {
    let (Some(subject_id), Some(object_id)) = (rel.term_id(subject), rel.term_id(object)) else {
        return Vec::new();
    };
    let mut next = Vec::new();
    for solution in solutions {
        let mut cursor = rel.select(atom.predicate.as_str(), Bound::Both(subject_id, object_id));
        while let Some((selected_subject, selected_object, row_id)) = cursor.next() {
            if !keep_row::<SCAN>(delta, row_id) {
                continue;
            }
            let mut merged = solution.clone();
            merged
                .source_facts
                .push(selected_fact(atom, rel, selected_subject, selected_object));
            next.push(merged);
        }
    }
    next
}

/// Extend each partial solution by index-selecting `atom`'s matching rows under `scan`.
///
/// This is the ONE-TIME [`Scan`]-mode dispatch: once per operator
/// (per atom-scan invocation, NOT per row) it lifts the semi-naive scan mode to the
/// `const SCAN: u8` compile-time parameter of [`extend_solutions_kernel`] via this
/// single enum `match`, so the per-row delta filter is resolved at monomorphization
/// instead of re-branched per tuple.  Dispatch is a plain enum `match` into the
/// concrete monomorphized kernel — never a trait object.
fn extend_solutions_indexed(
    atom: &EvalAtom,
    rel: &RelationStore,
    delta: Delta,
    scan: Scan,
    solutions: &[Solution],
) -> Vec<Solution> {
    match scan {
        Scan::Delta => extend_solutions_kernel::<SCAN_DELTA>(atom, rel, delta, solutions),
        Scan::Full => extend_solutions_kernel::<SCAN_FULL>(atom, rel, delta, solutions),
        Scan::OldOnly => extend_solutions_kernel::<SCAN_OLD_ONLY>(atom, rel, delta, solutions),
    }
}

/// The monomorphized index-selected join kernel for a fixed compile-time scan mode.
///
/// The index-selected analogue of `rule_ir::extend_solutions`: instead of scanning the
/// whole predicate bucket and post-filtering on the bound positions, it computes a
/// [`Bound`] from each partial solution and calls [`RelationStore::select`], which
/// returns ONLY the matching rows in insertion order.  Each returned `(subject, object)`
/// tuple is wrapped as a [`Fact`] and handed to [`match_atom`] exactly as
/// `extend_solutions` does, so the produced solution sequence (and `source_facts` order)
/// is identical to the full-scan engine.
///
/// `SCAN` is a compile-time constant ([`SCAN_DELTA`] / [`SCAN_FULL`] / [`SCAN_OLD_ONLY`]),
/// so the per-row semi-naive delta filter ([`keep_row`]) monomorphizes to a single
/// constant / one-word bitset probe with NO runtime branch on the scan mode — the
/// `match scan { … }` that formerly sat INSIDE this per-tuple loop is gone (greenfield).
fn extend_solutions_kernel<const SCAN: u8>(
    atom: &EvalAtom,
    rel: &RelationStore,
    delta: Delta,
    solutions: &[Solution],
) -> Vec<Solution> {
    let pred = atom.predicate.as_str();
    let interner = rel.interner();
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
        // Drive the arrangement's galloping lending cursor directly — NO per-stage
        // `Vec<(TermId, TermId, RowId)>` is materialized for this atom's selection.
        // Each `next()` yields one borrowed id row in row-id
        // (insertion) order, byte-identical to the former eager `select` vector.
        let mut cursor = rel.select(pred, bound);
        while let Some((s_id, o_id, row_id)) = cursor.next() {
            // Semi-naive position decomposition on the selected row's dense RowId — the
            // same delta×full split `extend_solutions` applies, but membership is one
            // `u64`-word test on the delta bitset.  `SCAN` is a
            // monomorphization constant, so `keep_row` is branch-free on the scan mode,
            // with NO three-`String` `Fact::key()` allocation and NO hashing per row.
            if !keep_row::<SCAN>(delta, row_id) {
                continue;
            }
            // Resolve the id row to its `TermValue` surfaces ONLY now — at the single
            // point the `Fact` (and its downstream reifier / provenance) needs them.
            let f = Fact {
                subject: interner.resolve(s_id).clone(),
                predicate: atom.predicate.clone(),
                object: interner.resolve(o_id).clone(),
            };
            if let Some(mut merged) = match_atom(atom, &f, sol) {
                merged.source_facts.push(f);
                next.push(merged);
            }
        }
    }
    next
}

/// The compile-time scan-mode codes for [`extend_solutions_kernel`]'s `const SCAN`
/// parameter — the const-generic translation of [`Scan`]'s three variants (Rust const
/// generics range over primitive `u8`, not enum variants directly).
const SCAN_DELTA: u8 = 0;
const SCAN_FULL: u8 = 1;
const SCAN_OLD_ONLY: u8 = 2;

/// The semi-naive delta as a contiguous RowId range `[lo, hi)` — the rows committed in
/// the PREVIOUS round (or, on the round-1 seed, every accumulated row `[0, row_count)`).
///
/// RowIds are minted densely in FactKey-sorted commit order, so a round's committed rows
/// are ALWAYS a contiguous span; delta membership is therefore a single range compare —
/// byte-identical to the former per-round `DenseBitset` holding exactly those ids, but
/// with NO per-round bitset allocation and NO arena round-trip.  "The round batch IS the
/// delta" is literally this RowId span.
#[derive(Clone, Copy)]
pub(super) struct Delta {
    /// Inclusive lower RowId index of the round's committed span.
    lo: usize,
    /// Exclusive upper RowId index of the round's committed span.
    hi: usize,
}

impl Delta {
    /// The empty delta — used by `Full` scans, which ignore membership entirely.
    const EMPTY: Self = Self { lo: 0, hi: 0 };

    /// The round-1 seed: every accumulated row `[0, row_count)` is "new" this round
    /// (mirrors `least_model_of_reduct`'s `delta = key_set()`).
    #[inline]
    pub(super) fn all(row_count: usize) -> Self {
        Self {
            lo: 0,
            hi: row_count,
        }
    }

    /// Whether `row` falls in the delta's committed span — one range compare, no hashing.
    #[inline]
    fn contains(self, row: RowId) -> bool {
        let i = row.index();
        self.lo <= i && i < self.hi
    }
}

/// The monomorphized per-row semi-naive keep test for a fixed scan mode.
///
/// `SCAN` is a compile-time constant, so this `match` folds at monomorphization to a
/// single arm — `true` (Full), `delta.contains(row_id)` (Delta), or its negation
/// (OldOnly) — with the other arms (and the `unreachable!`) dead-code eliminated.  The
/// membership is a contiguous-RowId range compare (`[lo, hi)`), byte-identical to the
/// former per-round `DenseBitset` word test but with no per-round allocation; the branch
/// is resolved at compile time, once per operator, not per tuple.
#[inline(always)]
fn keep_row<const SCAN: u8>(delta: Delta, row_id: RowId) -> bool {
    match SCAN {
        SCAN_FULL => true,
        SCAN_DELTA => delta.contains(row_id),
        SCAN_OLD_ONLY => !delta.contains(row_id),
        // `extend_solutions_indexed` instantiates only the three `Scan` codes above;
        // no other `SCAN` value is constructible, so this arm is statically unreachable.
        _ => unreachable!("SCAN is one of SCAN_DELTA / SCAN_FULL / SCAN_OLD_ONLY"),
    }
}

/// The semi-naive position-decomposition scan mode for one positive body atom.
///
/// Identical in meaning to `rule_ir::Scan` (which is private), reproduced here so the
/// index-selected join applies the same delta×full decomposition.  It is the fixed
/// per-(round, atom-position) shape [`join_body_indexed`] decides once, then hands to
/// [`extend_solutions_indexed`] which lifts it to the `const SCAN` monomorphization
/// parameter.
#[derive(Clone, Copy)]
enum Scan {
    /// Bind to rows whose key is in `delta` (the "new at p" position).
    Delta,
    /// Bind to any row (no delta constraint).
    Full,
    /// Bind only to rows whose key is NOT in `delta` (positions after p).
    OldOnly,
}

/// Runtime scan selection at operator construction; each variant contains a
/// const-generic filtered cursor, so the per-row delta predicate remains
/// monomorphized exactly like the binary kernel.
enum LeapfrogValueCursor<'a> {
    DeltaSubject(FilteredValueCursor<'a, SCAN_DELTA, VALUE_SUBJECT>),
    FullSubject(FilteredValueCursor<'a, SCAN_FULL, VALUE_SUBJECT>),
    OldOnlySubject(FilteredValueCursor<'a, SCAN_OLD_ONLY, VALUE_SUBJECT>),
    DeltaObject(FilteredValueCursor<'a, SCAN_DELTA, VALUE_OBJECT>),
    FullObject(FilteredValueCursor<'a, SCAN_FULL, VALUE_OBJECT>),
    OldOnlyObject(FilteredValueCursor<'a, SCAN_OLD_ONLY, VALUE_OBJECT>),
}

impl<'a> LeapfrogValueCursor<'a> {
    fn subject(rows: ValueCursor<'a, VALUE_SUBJECT>, scan: Scan, delta: Delta) -> Self {
        match scan {
            Scan::Delta => Self::DeltaSubject(FilteredValueCursor::new(rows, delta)),
            Scan::Full => Self::FullSubject(FilteredValueCursor::new(rows, delta)),
            Scan::OldOnly => Self::OldOnlySubject(FilteredValueCursor::new(rows, delta)),
        }
    }

    fn object(rows: ValueCursor<'a, VALUE_OBJECT>, scan: Scan, delta: Delta) -> Self {
        match scan {
            Scan::Delta => Self::DeltaObject(FilteredValueCursor::new(rows, delta)),
            Scan::Full => Self::FullObject(FilteredValueCursor::new(rows, delta)),
            Scan::OldOnly => Self::OldOnlyObject(FilteredValueCursor::new(rows, delta)),
        }
    }

    fn current(&self) -> Option<TermId> {
        match self {
            Self::DeltaSubject(cursor) => cursor.current,
            Self::FullSubject(cursor) => cursor.current,
            Self::OldOnlySubject(cursor) => cursor.current,
            Self::DeltaObject(cursor) => cursor.current,
            Self::FullObject(cursor) => cursor.current,
            Self::OldOnlyObject(cursor) => cursor.current,
        }
    }

    fn seek(&mut self, target: TermId) -> Option<TermId> {
        match self {
            Self::DeltaSubject(cursor) => cursor.seek(target),
            Self::FullSubject(cursor) => cursor.seek(target),
            Self::OldOnlySubject(cursor) => cursor.seek(target),
            Self::DeltaObject(cursor) => cursor.seek(target),
            Self::FullObject(cursor) => cursor.seek(target),
            Self::OldOnlyObject(cursor) => cursor.seek(target),
        }
    }

    fn advance(&mut self) -> Option<TermId> {
        match self {
            Self::DeltaSubject(cursor) => cursor.advance(),
            Self::FullSubject(cursor) => cursor.advance(),
            Self::OldOnlySubject(cursor) => cursor.advance(),
            Self::DeltaObject(cursor) => cursor.advance(),
            Self::FullObject(cursor) => cursor.advance(),
            Self::OldOnlyObject(cursor) => cursor.advance(),
        }
    }
}

/// One relation's distinct, sorted trie-level values under a fixed semi-naive scan.
struct FilteredValueCursor<'a, const SCAN: u8, const COLUMN: u8> {
    rows: ValueCursor<'a, COLUMN>,
    delta: Delta,
    current: Option<TermId>,
}

impl<'a, const SCAN: u8, const COLUMN: u8> FilteredValueCursor<'a, SCAN, COLUMN> {
    fn new(rows: ValueCursor<'a, COLUMN>, delta: Delta) -> Self {
        let mut cursor = Self {
            rows,
            delta,
            current: None,
        };
        cursor.fill(None);
        cursor
    }

    /// Fill `current` with the next distinct scan-admitted value, skipping `prior`.
    fn fill(&mut self, prior: Option<TermId>) -> Option<TermId> {
        self.current = None;
        while let Some((value, row)) = self.rows.next() {
            if keep_row::<SCAN>(self.delta, row) && Some(value) != prior {
                self.current = Some(value);
                break;
            }
        }
        self.current
    }

    fn seek(&mut self, target: TermId) -> Option<TermId> {
        if self.current.is_some_and(|value| value >= target) {
            return self.current;
        }
        self.rows.seek(target);
        self.fill(None)
    }

    fn advance(&mut self) -> Option<TermId> {
        let prior = self.current;
        self.fill(prior)
    }
}

/// A standard leapfrog intersection across sorted distinct value cursors.
struct LeapfrogIntersection<'a> {
    cursors: Vec<LeapfrogValueCursor<'a>>,
}

impl<'a> LeapfrogIntersection<'a> {
    fn new(cursors: Vec<LeapfrogValueCursor<'a>>) -> Self {
        Self { cursors }
    }

    /// Return the next value present in every cursor. The first cursor advances past
    /// the returned value before control returns, so repeated calls enumerate the
    /// intersection without duplicates.
    fn next(&mut self) -> Option<TermId> {
        let mut target = self
            .cursors
            .iter()
            .filter_map(|cursor| cursor.current())
            .max()?;
        loop {
            let mut aligned = true;
            for cursor in &mut self.cursors {
                let value = cursor.seek(target)?;
                if value > target {
                    target = value;
                    aligned = false;
                }
            }
            if aligned {
                self.cursors[0].advance();
                return Some(target);
            }
        }
    }

    /// Whether the exact externally-bound value occurs in every relation cursor.
    fn contains(&mut self, wanted: TermId) -> bool {
        for cursor in &mut self.cursors {
            if cursor.seek(wanted) != Some(wanted) {
                return false;
            }
        }
        true
    }
}

#[inline]
fn scan_for(positive_position: usize, delta_position: usize) -> Scan {
    if positive_position < delta_position {
        Scan::Full
    } else if positive_position == delta_position {
        Scan::Delta
    } else {
        Scan::OldOnly
    }
}

#[inline]
fn keep_row_for_scan(scan: Scan, delta: Delta, row: RowId) -> bool {
    match scan {
        Scan::Delta => keep_row::<SCAN_DELTA>(delta, row),
        Scan::Full => keep_row::<SCAN_FULL>(delta, row),
        Scan::OldOnly => keep_row::<SCAN_OLD_ONLY>(delta, row),
    }
}

/// Build one cycle atom's trie cursor for `variable`, constrained by any binding of
/// its other variable. Cycle certification guarantees two distinct variable terms.
fn cycle_atom_cursor<'a>(
    atom: &EvalAtom,
    operator: &AtomOperator,
    variable_slot: usize,
    solution: &SlotSolution,
    rel: &'a RelationStore,
    scan: Scan,
    delta: Delta,
) -> Option<LeapfrogValueCursor<'a>> {
    let AtomKernel::Vars {
        subject_slot,
        object_slot,
    } = operator.kernel()
    else {
        return None;
    };
    if *subject_slot == variable_slot {
        let other = match solution.get(*object_slot) {
            Some(surface) => Some(rel.term_id(surface)?),
            None => None,
        };
        Some(LeapfrogValueCursor::subject(
            rel.values_subject(atom.predicate.as_str(), other),
            scan,
            delta,
        ))
    } else if *object_slot == variable_slot {
        let other = match solution.get(*subject_slot) {
            Some(surface) => Some(rel.term_id(surface)?),
            None => None,
        };
        Some(LeapfrogValueCursor::object(
            rel.values_object(atom.predicate.as_str(), other),
            scan,
            delta,
        ))
    } else {
        None
    }
}

/// Capture the unique fully-ground row for every cycle atom, in the cycle plan's
/// authored atom order. Returns false if a scan-mode constraint excludes any row.
fn append_cycle_sources(
    rule: &EvalRule,
    plan: &RulePlan,
    cycle: &CyclicPlan,
    delta_position: usize,
    rel: &RelationStore,
    delta: Delta,
    solution: &mut SlotSolution,
) -> bool {
    let original_len = solution.source_facts.len();
    for &planned in cycle.atoms() {
        let atom = &rule.body[planned.body_index()];
        let operator = plan.operator_at(planned.positive_position());
        let AtomKernel::Vars {
            subject_slot,
            object_slot,
        } = operator.kernel()
        else {
            unreachable!("cycle certification admits only distinct variable-variable atoms")
        };
        let (Some(subject), Some(object)) =
            (solution.get(*subject_slot), solution.get(*object_slot))
        else {
            solution.source_facts.truncate(original_len);
            return false;
        };
        let (Some(subject_id), Some(object_id)) = (rel.term_id(subject), rel.term_id(object))
        else {
            solution.source_facts.truncate(original_len);
            return false;
        };
        let scan = scan_for(planned.positive_position(), delta_position);
        let mut rows = rel.select(atom.predicate.as_str(), Bound::Both(subject_id, object_id));
        let mut matched = None;
        while let Some((subject_id, object_id, row)) = rows.next() {
            if keep_row_for_scan(scan, delta, row) {
                matched = Some(Fact {
                    subject: rel.interner().resolve(subject_id).clone(),
                    predicate: atom.predicate.clone(),
                    object: rel.interner().resolve(object_id).clone(),
                });
                break;
            }
        }
        let Some(fact) = matched else {
            solution.source_facts.truncate(original_len);
            return false;
        };
        solution.source_facts.push(fact);
    }
    true
}

/// Immutable state shared by every recursive variable level of one LFTJ component.
struct LeapfrogRun<'a> {
    rule: &'a EvalRule,
    plan: &'a RulePlan,
    cycle: &'a CyclicPlan,
    delta_position: usize,
    rel: &'a RelationStore,
    delta: Delta,
}

impl LeapfrogRun<'_> {
    /// Recursive LFTJ variable descent for one certified cycle component.
    fn recurse(
        &self,
        variable_position: usize,
        solution: &mut SlotSolution,
        out: &mut Vec<SlotSolution>,
    ) {
        if variable_position == self.cycle.variable_slots().len() {
            if append_cycle_sources(
                self.rule,
                self.plan,
                self.cycle,
                self.delta_position,
                self.rel,
                self.delta,
                solution,
            ) {
                out.push(solution.clone());
                solution
                    .source_facts
                    .truncate(solution.source_facts.len() - self.cycle.atoms().len());
            }
            return;
        }

        let variable_slot = self.cycle.variable_slots()[variable_position];
        let externally_bound = match solution.get(variable_slot) {
            Some(surface) => match self.rel.term_id(surface) {
                Some(value) => Some(value),
                None => return,
            },
            None => None,
        };
        let mut cursors = Vec::new();
        for &planned in self.cycle.atoms() {
            let atom = &self.rule.body[planned.body_index()];
            let operator = self.plan.operator_at(planned.positive_position());
            let AtomKernel::Vars {
                subject_slot,
                object_slot,
            } = operator.kernel()
            else {
                unreachable!("cycle certification admits only variable-variable atoms")
            };
            let contains_variable = *subject_slot == variable_slot || *object_slot == variable_slot;
            if !contains_variable {
                continue;
            }
            let scan = scan_for(planned.positive_position(), self.delta_position);
            let Some(cursor) = cycle_atom_cursor(
                atom,
                operator,
                variable_slot,
                solution,
                self.rel,
                scan,
                self.delta,
            ) else {
                return;
            };
            cursors.push(cursor);
        }
        if cursors.is_empty() {
            return;
        }
        let mut intersection = LeapfrogIntersection::new(cursors);

        if let Some(value) = externally_bound {
            if intersection.contains(value) {
                self.recurse(variable_position + 1, solution, out);
            }
            return;
        }

        while let Some(value) = intersection.next() {
            debug_assert!(solution.bindings[variable_slot].is_none());
            solution.bindings[variable_slot] =
                Some(self.rel.interner().display_of(value).to_owned());
            self.recurse(variable_position + 1, solution, out);
            solution.bindings[variable_slot] = None;
        }
    }
}

/// Extend every partial solution through one certified cyclic component without
/// materializing any binary intermediate relation.
fn extend_solutions_leapfrog(
    rule: &EvalRule,
    plan: &RulePlan,
    cycle: &CyclicPlan,
    delta_position: usize,
    rel: &RelationStore,
    delta: Delta,
    solutions: &[SlotSolution],
) -> Vec<SlotSolution> {
    let mut out = Vec::new();
    let run = LeapfrogRun {
        rule,
        plan,
        cycle,
        delta_position,
        rel,
        delta,
    };
    for solution in solutions {
        let mut working = solution.clone();
        run.recurse(0, &mut working, &mut out);
    }
    out
}

/// Hybrid positive join for a rule with at least one certified cyclic subplan.
fn join_body_leapfrog(
    rule: &EvalRule,
    plan: &RulePlan,
    rel: &RelationStore,
    accumulated: &RelationStore,
    delta: Delta,
    gap: &mut bool,
) -> Vec<Solution> {
    let mut slot_solutions = Vec::new();
    for delta_position in 0..plan.positive().len() {
        let mut partial = vec![SlotSolution::empty(plan.variables().len())];
        for group in plan.join_groups() {
            partial = match group {
                JoinGroup::Binary(planned) => extend_slot_solutions_indexed(
                    plan.operator_at(planned.positive_position()),
                    &rule.body[planned.body_index()],
                    rel,
                    delta,
                    scan_for(planned.positive_position(), delta_position),
                    &partial,
                ),
                JoinGroup::Leapfrog(cycle) => extend_solutions_leapfrog(
                    rule,
                    plan,
                    cycle,
                    delta_position,
                    rel,
                    delta,
                    &partial,
                ),
            };
            if partial.is_empty() {
                break;
            }
        }
        for solution in &mut partial {
            for &(left, right) in plan.hybrid_source_order_swaps() {
                solution.source_facts.swap(left, right);
            }
        }
        slot_solutions.extend(partial);
    }

    let mut solutions: Vec<Solution> = slot_solutions
        .into_iter()
        .map(|solution| solution.into_named(plan.variables()))
        .collect();

    if !rule.builtins.is_empty() {
        solutions = apply_builtins(&rule.builtins, solutions, gap);
    }
    if !plan.negated().is_empty() {
        solutions.retain(|solution| {
            !plan
                .negated()
                .iter()
                .any(|&index| negated_atom_satisfied(&rule.body[index], solution, accumulated))
        });
    }
    solutions
}

/// Join all body atoms against `rel`, evaluating NAF against the accumulated store.
///
/// The index-selected twin of `rule_ir::join_body`: the positive join is the SAME
/// semi-naive delta×full position decomposition (union over each delta position `p`
/// of `{ a_p ∈ delta, a_{<p} ∈ full, a_{>p} ∈ store \ delta }`), with each per-atom
/// scan performed by the planned slot/index kernels (or the certified LFTJ group).
/// NAF body atoms are filtered
/// after the positive join via membership in `accumulated` (the frozen-below store),
/// which is exactly the stratified-negation reference.
pub(super) fn join_body_indexed(
    rule: &EvalRule,
    plan: &RulePlan,
    rel: &RelationStore,
    accumulated: &RelationStore,
    delta: Delta,
    gap: &mut bool,
) -> Vec<Solution> {
    if plan.has_cyclic_subplan() {
        return join_body_leapfrog(rule, plan, rel, accumulated, delta, gap);
    }
    join_body_binary(rule, plan, rel, accumulated, delta, gap)
}

/// The retained indexed-binary reference. It remains the production fast path for
/// every acyclic rule and the focused parity oracle for promoted cyclic rules.
fn join_body_binary(
    rule: &EvalRule,
    plan: &RulePlan,
    rel: &RelationStore,
    accumulated: &RelationStore,
    delta: Delta,
    gap: &mut bool,
) -> Vec<Solution> {
    // The positive (join) / negated (NAF) body-atom partition was precomputed ONCE at
    // plan time ([`RulePlan`]); the per-round `filter(..).collect()` allocation is gone.
    // Both slices are body-order indices into `rule.body`, so the produced solution
    // sequence is byte-identical to the previous per-round partition.
    let positive = plan.positive();
    let negated = plan.negated();

    let mut solutions: Vec<Solution> = if positive.is_empty() {
        // Zero positive atoms never touch delta, so they never fire in a semi-naive
        // round — matches `join_body`'s empty-positive branch exactly.
        Vec::new()
    } else {
        let k = positive.len();
        debug_assert_eq!(plan.operators().len(), k);
        let mut all: Vec<SlotSolution> = Vec::new();
        for p in 0..k {
            let mut partial = vec![SlotSolution::empty(plan.variables().len())];
            for (j, operator) in plan.operators().iter().enumerate() {
                let atom = &rule.body[operator.body_index()];
                let scan = if j < p {
                    Scan::Full
                } else if j == p {
                    Scan::Delta
                } else {
                    Scan::OldOnly
                };
                partial = extend_slot_solutions_indexed(operator, atom, rel, delta, scan, &partial);
                if partial.is_empty() {
                    break;
                }
            }
            all.extend(partial);
        }
        for solution in &mut all {
            for &(left, right) in plan.operator_source_order_swaps() {
                solution.source_facts.swap(left, right);
            }
        }
        all.into_iter()
            .map(|solution| solution.into_named(plan.variables()))
            .collect()
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
                .any(|&i| negated_atom_satisfied(&rule.body[i], sol, accumulated))
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

/// Whether a negated atom is satisfied (blocks the rule) — some grounded form is
/// PRESENT in the accumulated (frozen-below) store.
///
/// Probes the columnar [`RelationStore`] rather than the ternary `FactStore`.  Within
/// a stratum the negated predicate is fully materialized in a strictly lower stratum,
/// so this membership is the stratified-negation truth value.
///
/// Two binding modes, mirroring `foundation.rs::negated_atom_satisfied` +
/// `match_partial` exactly:
///
/// * **Fully ground** (both subject and object bound/constant): an O(1)
///   [`RelationStore::contains`] membership test.
/// * **Partially bound (existential NAF)**: at least one position is an unbound
///   variable — e.g. `NOT genericQuality(?Q, ?G)` with `?G` free, meaning "`?Q` has
///   NO `genericQuality`".  The atom is satisfied iff SOME fact matches the *ground*
///   positions; an unbound position is unconstrained (even if a variable repeats —
///   repeated unbound vars are NOT required to agree, matching foundation's
///   `match_partial` byte for byte).  A ground term that never entered the store
///   constrains to zero rows → not satisfied.
fn negated_atom_satisfied(atom: &EvalAtom, sol: &Solution, accumulated: &RelationStore) -> bool {
    let s = ground(&atom.subject, sol);
    let o = ground(&atom.object, sol);
    match (s, o) {
        (Some(s), Some(o)) => accumulated.contains(atom.predicate.as_str(), &s, &o),
        (s, o) => {
            // At least one position is unbound.  Build the selection bound from the
            // ground positions only; a ground term the store never interned yields
            // `None` (no row can match → not satisfied).
            let Some(bound) = atom_bound(accumulated, s.as_deref(), o.as_deref()) else {
                return false;
            };
            // Existential NAF asks only "does SOME row match?" — probe the cursor for a
            // single row (`any_remaining`) instead of materializing a whole `Vec` just
            // to call `is_empty()` on it.
            accumulated
                .select(atom.predicate.as_str(), bound)
                .any_remaining()
        }
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
pub(super) fn stratify(rules: &[EvalRule]) -> Option<HashMap<String, usize>> {
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
    exe: &Executable,
    max_steps: Option<u64>,
) -> gmeow_errors::Result<NativeOutcome<Budgeted<Vec<DerivedRow>>>> {
    materialize_native_with_round_execution(store, exe, max_steps, RoundExecution::Parallel)
}

/// Policy-selectable implementation behind [`materialize_native`].
///
/// The policy is private because callers may not weaken production execution; focused
/// tests use it to prove forced sequential and forced parallel rounds are identical.
fn materialize_native_with_round_execution(
    store: &crate::store::WorldStore,
    exe: &Executable,
    max_steps: Option<u64>,
    round_execution: RoundExecution,
) -> gmeow_errors::Result<NativeOutcome<Budgeted<Vec<DerivedRow>>>> {
    // Stratification and per-rule join planning are properties of the rules alone; the
    // caller computed them ONCE through the `Parsed → Stratified → Planned → Executable`
    // pipeline (a non-stratifiable program never reaches here — it is the pipeline's
    // `stratify()` → `None` declared gap).  This forward leg only executes the plan.
    let mut worlds = store.worlds();
    worlds.sort();

    let total = exe.stratum_count();

    // UNBOUNDED path (foundation's `materialize_native(store, &rules, None)`): with no
    // step budget the `StepGovernor` never cuts, so every world runs to full fixpoint,
    // `status` is always `Ok`, no world is left untouched, and the worlds are fully
    // independent (each reads only the shared `store` + `exe`, both `&`-
    // shared/read-only).  That independence is what makes per-world rayon parallelism
    // deterministic and byte-identical to the sequential fold. A SHARED step budget keeps
    // the OUTER sorted-world loop order-serial, but each world's immutable per-rule round
    // work still uses `round_execution`; only the lexical commit mutates shared state.
    if max_steps.is_none() {
        // `WorldStore` holds a `RefCell` and is therefore NOT `Sync`, so the store read
        // (`world_edb_facts`) is hoisted out of the parallel region and run sequentially
        // per sorted world FIRST.  The read is pure and order-independent, so this seed
        // pass changes no observable output; only the OWNED `(world, edb)` pairs cross
        // into the thread pool.  The per-world chase below reads only these owned facts
        // and the `&`-shared read-only `exe` (an `Executable` — its `&[EvalRule]` +
        // owned strata/plans are all `Sync`).
        let edb_by_world: Vec<(String, Vec<Fact>)> = worlds
            .iter()
            .map(|world| Ok((world.clone(), world_edb_facts(store, world)?)))
            .collect::<gmeow_errors::Result<Vec<_>>>()?;

        // Per-world independent chase.  `into_par_iter().map(..).collect::<Result<Vec<_>>>()`
        // preserves the sorted-world INPUT order in the output Vec, so folding the results
        // in that order reproduces the sequential push order exactly.
        let per_world: Vec<(Vec<DerivedRow>, BTreeSet<String>, u64)> = edb_by_world
            .into_par_iter()
            .map(
                |(world, edb_facts)| -> gmeow_errors::Result<(Vec<DerivedRow>, BTreeSet<String>, u64)> {
                    // Echo the asserted EDB FIRST (identical order to the sequential body).
                    let mut rows = echo_asserted(&world, &edb_facts)?;
                    // A PER-WORLD unbounded governor: it never cuts, so its final
                    // `.consumed` counts exactly this world's derivations.
                    let mut governor = StepGovernor::new(None);
                    let budgeted = eval_world_stratified(
                        &edb_facts,
                        exe,
                        &mut governor,
                        ProvenanceMode::Record,
                        round_execution,
                    )?;
                    // Derived rows AFTER the echo rows (same order as the sequential body).
                    for mut row in budgeted.rows {
                        row.graph = world.clone();
                        rows.push(row);
                    }
                    Ok((rows, budgeted.progress.saturated_preds, governor.consumed))
                },
            )
            .collect::<gmeow_errors::Result<Vec<_>>>()?;

        let mut out: Vec<DerivedRow> = Vec::new();
        let mut frontier: Option<BTreeSet<String>> = None;
        let mut consumed: u64 = 0;
        for (rows, saturated, world_consumed) in per_world {
            // Concatenate rows in sorted-world order — reproduces the sequential
            // `out.extend`/`out.push` interleaving exactly.
            out.extend(rows);
            // Set-intersection is order-independent — the same cross-world frontier the
            // sequential running intersection computes.
            frontier = Some(match frontier {
                None => saturated,
                Some(f) => f.intersection(&saturated).cloned().collect(),
            });
            // The sequential path threads ONE governor whose final `.consumed` equals the
            // SUM of per-world derivations, so summing here is byte-identical.
            consumed += world_consumed;
        }

        let progress = StrataProgress {
            completed: total,
            total,
            saturated_preds: frontier.unwrap_or_default(),
        };
        sort_rows(&mut out);
        return Ok(NativeOutcome::Decided(Budgeted {
            rows: out,
            status: BudgetStatus::Ok,
            progress,
            consumed_steps: consumed,
        }));
    }

    // BUDGETED path: `max_steps` is a SINGLE GLOBAL budget across the sorted worlds (not
    // reset per world): the correct bundle-guard semantics and deterministic because
    // world order is fixed.  Worlds run until the shared counter is spent; later worlds
    // then never run (their strata stay unsaturated).
    let mut governor = StepGovernor::new(max_steps);
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

        let budgeted = eval_world_stratified(
            &edb_facts,
            exe,
            &mut governor,
            ProvenanceMode::Record,
            round_execution,
        )?;
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
    exe: &Executable,
    governor: &mut StepGovernor,
    mode: ProvenanceMode,
    round_execution: RoundExecution,
) -> gmeow_errors::Result<Budgeted<Vec<DerivedRow>>> {
    eval_world_stratified_with_trace(edb_facts, exe, governor, mode, round_execution, None)
}

fn eval_world_stratified_with_trace(
    edb_facts: &[Fact],
    exe: &Executable,
    governor: &mut StepGovernor,
    mode: ProvenanceMode,
    round_execution: RoundExecution,
    mut parallel_trace: Option<&mut RuleParallelTrace>,
) -> gmeow_errors::Result<Budgeted<Vec<DerivedRow>>> {
    // Shared accumulated store (both forms), seeded from the EDB in sorted-key order
    // (world_edb_facts already sorted), so seeding matches the reference.
    let mut store = FactStore::new();
    let mut rel = RelationStore::new();
    // Per-fact derivation-depth column, indexed by `store`'s insertion-order row (pushed
    // in lockstep with `store.insert`).  Depth feeds ONLY the Record-mode tiebreak; the
    // Skip lane never writes it, so it stays empty there (asserted below in `evaluate`).
    let mut depth: Vec<ProofHeight> = Vec::new();

    // A PURE-EDB predicate (never a rule head) is settled from the seed; a predicate that
    // is also a rule head is settled only when its stratum completes (below), so exclude
    // it here — otherwise a self-recursive predicate would over-claim while its closure is
    // still unbuilt.  The head-predicate set is memoized on the `Executable`.
    let head_preds = exe.head_predicates();
    let mut saturated_preds: BTreeSet<String> = edb_facts
        .iter()
        .map(|f| f.predicate.clone())
        .filter(|p| !head_preds.contains(p))
        .collect();

    for f in edb_facts {
        // Insert into both stores in lockstep; under Record push the depth-0 seed slot
        // so `depth` tracks `store`'s rows exactly (Skip omits depth entirely).
        if let Some(idx) = store.insert(f.clone()) {
            rel.insert(&f.predicate, &f.subject, &f.object);
            if let ProvenanceMode::Record = mode {
                debug_assert_eq!(idx, depth.len(), "depth/store lockstep on the EDB seed");
                depth.push(ProofHeight::ASSERTED); // EDB facts have height 0
            }
        }
    }

    let mut derivations: Vec<DerivedRow> = Vec::new();

    let total = exe.stratum_count();
    let mut completed = 0usize;
    let mut status = BudgetStatus::Ok;
    // Ordinary forward materialization carries no arithmetic builtins (the ontology
    // corpus has none), so this stays false; assert that invariant below.
    let mut builtin_gap = false;
    for k in 0..total {
        if exe.stratum_is_empty(k) {
            completed += 1; // an empty stratum is trivially saturated
            continue;
        }
        match eval_stratum_fixpoint(
            exe,
            k,
            &mut FixpointState {
                store: &mut store,
                rel: &mut rel,
                depth: &mut depth,
                derivations: &mut derivations,
                builtin_gap: &mut builtin_gap,
            },
            governor,
            mode,
            round_execution,
            parallel_trace.as_deref_mut(),
        )? {
            FixpointStatus::Complete => {
                // This stratum reached its natural fixpoint: its head predicates are now
                // final and join the settled frontier.
                for pred in exe.stratum_head_predicates(k) {
                    saturated_preds.insert(pred.to_owned());
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

/// The mutable working set carried across every stratum of one world's fixpoint: the two
/// lockstep stores, the depth map, the derivation accumulator, and the arithmetic-gap flag.
/// Bundling them keeps [`eval_stratum_fixpoint`] under clippy's argument-count bar with a
/// cohesive named type rather than a suppression.
struct FixpointState<'a> {
    store: &'a mut FactStore,
    rel: &'a mut RelationStore,
    depth: &'a mut Vec<ProofHeight>,
    derivations: &'a mut Vec<DerivedRow>,
    builtin_gap: &'a mut bool,
}

/// The immutable snapshot every rule task reads during one semi-naive round.
///
/// No task may mutate these structures. The single sorted commit begins only after all
/// task buffers have been collected and deterministically merged.
#[derive(Clone, Copy)]
struct RoundSnapshot<'a> {
    store: &'a FactStore,
    rel: &'a RelationStore,
    depth: &'a [ProofHeight],
    delta: Delta,
    mode: ProvenanceMode,
}

/// One rule task's round-local winners and arithmetic-gap observation.
///
/// The borrowed-key index points into `entries`, so keys are owned exactly once. Parallel
/// tasks own independent instances; after task completion they are merged serially in
/// executable program order using the same total provenance winner relation.
struct RoundCandidateBuffer {
    entries: Vec<(FactKey, RuleRoundCandidate)>,
    index: HashTable<usize>,
    builtin_gap: bool,
}

/// Deterministic structural work observed on the rule-parallel path.
///
/// Candidate rows are counted after each rule-local winner dedup and before the
/// scheduling-erasing merge. For one round, serial buffered work is the sum of
/// every task's rows while the ideal rule-task critical path is the maximum. Summing
/// those quantities across the necessarily sequential semi-naive rounds produces a
/// scheduler-independent comparison; no wall clock or worker-arrival order enters it.
#[derive(Debug, Default, Clone, Copy)]
pub(super) struct RuleParallelTrace {
    pub(super) parallel_rounds: u64,
    pub(super) rule_tasks: u64,
    pub(super) serial_candidate_rows: u64,
    pub(super) critical_path_candidate_rows: u64,
    pub(super) max_buffered_candidate_rows: u64,
    pub(super) max_task_candidate_rows: u64,
}

impl RuleParallelTrace {
    fn record_round(&mut self, task_rows: &[usize]) {
        let serial = task_rows.iter().map(|&rows| rows as u64).sum::<u64>();
        let critical = task_rows
            .iter()
            .copied()
            .max()
            .map_or(0, |rows| rows as u64);
        self.parallel_rounds += 1;
        self.rule_tasks += task_rows.len() as u64;
        self.serial_candidate_rows += serial;
        self.critical_path_candidate_rows += critical;
        self.max_buffered_candidate_rows = self.max_buffered_candidate_rows.max(serial);
        self.max_task_candidate_rows = self.max_task_candidate_rows.max(critical);
    }
}

impl RoundCandidateBuffer {
    fn new() -> Self {
        Self {
            entries: Vec::new(),
            index: HashTable::new(),
            builtin_gap: false,
        }
    }

    /// Insert or quality-merge one candidate under its cached fact key.
    fn insert(
        &mut self,
        key: FactKey,
        candidate: RuleRoundCandidate,
        mode: ProvenanceMode,
    ) -> gmeow_errors::Result<()> {
        let hash = fact_key_hash(&key);
        match self.index.find(hash, |&i| self.entries[i].0 == key) {
            Some(&i) => {
                if mode == ProvenanceMode::Record && candidate.preferred_over(&self.entries[i].1)? {
                    self.entries[i].1 = candidate;
                }
            }
            None => {
                let index = self.entries.len();
                self.entries.push((key, candidate));
                let entries = &self.entries;
                self.index
                    .insert_unique(hash, index, |&i| fact_key_hash(&entries[i].0));
            }
        }
        Ok(())
    }

    /// Merge a completed rule-local buffer at the scheduling-erasing serial boundary.
    fn merge_from(&mut self, other: Self, mode: ProvenanceMode) -> gmeow_errors::Result<()> {
        self.builtin_gap |= other.builtin_gap;
        for (key, candidate) in other.entries {
            self.insert(key, candidate, mode)?;
        }
        Ok(())
    }
}

/// Evaluate one rule against the frozen round snapshot into `round`.
///
/// The sequential policy calls this directly on one shared round buffer (preserving the
/// allocation-minimal one-worker baseline). The parallel policy gives every invocation a
/// private buffer and merges those buffers after all joins finish.
fn evaluate_rule_into_round(
    rule: &EvalRule,
    plan: &RulePlan,
    snapshot: RoundSnapshot<'_>,
    round: &mut RoundCandidateBuffer,
) -> gmeow_errors::Result<()> {
    for sol in join_body_indexed(
        rule,
        plan,
        snapshot.rel,
        snapshot.rel,
        snapshot.delta,
        &mut round.builtin_gap,
    ) {
        if !distinct_pairs_satisfied(&rule.distinct_pairs, &sol)? {
            continue;
        }
        let head = ground_head(&rule.head, &sol)?;
        let key = head.key();
        if snapshot.store.contains_key(&key) {
            continue; // a prior round/stratum already derived it; earlier wins
        }

        let candidate = match snapshot.mode {
            ProvenanceMode::Record => {
                // Provenance: reifiers of matched POSITIVE body facts in body order.
                let mut sources: Vec<String> = Vec::with_capacity(sol.source_facts.len());
                let mut max_sd = ProofHeight::ASSERTED;
                let mut sum_sd: u64 = 0;
                for sf in &sol.source_facts {
                    sources.push(sf.reifier()?);
                    let source_key = sf.key();
                    let row = snapshot.store.row_index(&source_key).ok_or_else(|| {
                        seminaive_err(format!(
                            "provenance source {source_key:?} is absent from the physical fact store"
                        ))
                    })?;
                    drop(source_key);
                    let d = snapshot.depth.get(row).copied().ok_or_else(|| {
                        seminaive_err(format!(
                            "provenance source row {row} has no proof-height annotation"
                        ))
                    })?;
                    max_sd = max_sd.max(d);
                    sum_sd = sum_sd.saturating_add(u64::from(d.get()));
                }
                let proof_height = MinProofHeightSemiring.derive([max_sd])?;
                let source_refs: Vec<&str> = sources.iter().map(String::as_str).collect();
                let deriv = mint_derivation_id(&rule.rule_iri, &source_refs);
                let mut sorted_sources = sources.clone();
                sorted_sources.sort();

                RuleRoundCandidate {
                    head,
                    prov: Some(Provenance {
                        sources,
                        sorted_sources,
                        source_facts: sol.source_facts.clone(),
                        deriv,
                        rule_iri: rule.rule_iri.clone(),
                        proof_height,
                        sum_src_depth: sum_sd,
                    }),
                }
            }
            ProvenanceMode::Skip => {
                // Facts-only: every candidate under `key` has the same content-derived head,
                // so first-seen is sufficient and no provenance work is performed.
                RuleRoundCandidate { head, prov: None }
            }
        };
        round.insert(key, candidate, snapshot.mode)?;
    }
    Ok(())
}

/// Evaluate all rules in a stratum, optionally in parallel, and erase scheduling order.
fn evaluate_round_candidates(
    exe: &Executable,
    stratum: usize,
    snapshot: RoundSnapshot<'_>,
    execution: RoundExecution,
    trace: Option<&mut RuleParallelTrace>,
) -> gmeow_errors::Result<RoundCandidateBuffer> {
    let rule_indices = exe.stratum_rule_indices(stratum);
    if !execution.should_parallelize(rule_indices.len()) {
        let mut round = RoundCandidateBuffer::new();
        for &rule_index in rule_indices {
            let (rule, plan) = exe.rule_entry(rule_index);
            evaluate_rule_into_round(rule, plan, snapshot, &mut round)?;
        }
        return Ok(round);
    }

    // `par_iter` over a slice is indexed: `collect::<Vec<_>>()` preserves input program
    // order regardless of completion order. Keep each task result wrapped until the serial
    // loop so, if multiple rules fail, the observable diagnostic is also the first one in
    // program order rather than whichever worker happened to finish first.
    let rule_results: Vec<gmeow_errors::Result<RoundCandidateBuffer>> = rule_indices
        .par_iter()
        .map(|&rule_index| {
            let (rule, plan) = exe.rule_entry(rule_index);
            let mut round = RoundCandidateBuffer::new();
            evaluate_rule_into_round(rule, plan, snapshot, &mut round)?;
            Ok(round)
        })
        .collect();

    if let Some(trace) = trace {
        // Do not inspect or reorder diagnostics here. Evidence is recorded only when
        // every task succeeded; otherwise the program-order merge below returns the
        // same first error it always did.
        let task_rows = rule_results
            .iter()
            .map(|result| result.as_ref().ok().map(|buffer| buffer.entries.len()))
            .collect::<Option<Vec<_>>>();
        if let Some(task_rows) = task_rows {
            trace.record_round(&task_rows);
        }
    }

    let mut buffers = rule_results.into_iter();
    let Some(first) = buffers.next() else {
        return Ok(RoundCandidateBuffer::new());
    };
    let mut merged = first?;
    for buffer in buffers {
        merged.merge_from(buffer?, snapshot.mode)?;
    }
    Ok(merged)
}

/// Run the semi-naive fixpoint for the rules of ONE stratum into the shared stores.
///
/// This loop is a structural copy of `least_model_of_reduct`'s round loop — same
/// EDB/lower-stratum-seeded delta, same per-round canonical-winner map and quality
/// tiebreak, same depth bookkeeping, same body-order `source_quad_ids` — with the
/// join replaced by [`join_body_indexed`] (index selection) and NAF read from the
/// accumulated [`RelationStore`] (`rel`, the frozen-below store).
///
/// `mode` selects whether the loop mints and records provenance ([`ProvenanceMode::Record`],
/// the forward leg) or commits facts only ([`ProvenanceMode::Skip`], the backward leg) — the
/// committed fact set, insertion order, and step budget are identical either way.
///
/// # The type-state executor gate
///
/// This is the semi-naive executor entry point, and it is **unrepresentable without an
/// [`Executable`]**: the rules of stratum `stratum` are read from `exe`, whose only
/// constructor chain is `Parsed::uncached(..).stratify()?.plan().into_executable()` (see
/// [`super::plan`]).  There is no overload taking `&[EvalRule]`, a `Parsed`, a
/// `Stratified`, or a `Planned`; the compiler — not a doc comment — rejects any attempt
/// to execute a program that has not been stratified AND join-planned.
fn eval_stratum_fixpoint(
    exe: &Executable,
    stratum: usize,
    state: &mut FixpointState<'_>,
    governor: &mut StepGovernor,
    mode: ProvenanceMode,
    round_execution: RoundExecution,
    mut parallel_trace: Option<&mut RuleParallelTrace>,
) -> gmeow_errors::Result<FixpointStatus> {
    // Reborrow each accumulator into a single `&mut` local so the loop body below is a verbatim
    // copy of `least_model_of_reduct`'s — the `FixpointState` bundle exists only to keep the
    // signature under clippy's argument-count bar without an `#[allow]`, not to change the engine.
    let store = &mut *state.store;
    let rel = &mut *state.rel;
    let depth = &mut *state.depth;
    let derivations = &mut *state.derivations;
    let builtin_gap = &mut *state.builtin_gap;
    // Seed delta with EVERY accumulated row so this stratum's rules fire against the
    // seed in round 1 (mirrors `least_model_of_reduct`'s `delta = key_set()`).  The
    // `RelationStore` mints RowIds densely as `0..row_count` in commit order, so the
    // whole accumulated store is exactly the contiguous span `[0, row_count)` — a range,
    // no per-key materialization, no bitset, no hashing.
    let mut delta = Delta::all(rel.row_count());

    loop {
        // Every rule reads this immutable snapshot. Parallel tasks produce independent
        // borrowed-key winner buffers; their program-order merge erases scheduling before
        // the single lexical commit mutates either store or charges the governor.
        let round = evaluate_round_candidates(
            exe,
            stratum,
            RoundSnapshot {
                store,
                rel,
                depth,
                delta,
                mode,
            },
            round_execution,
            parallel_trace.as_deref_mut(),
        )?;
        *builtin_gap |= round.builtin_gap;
        let round_entries = round.entries;

        if round_entries.is_empty() {
            break; // stratum fixpoint
        }

        // The next round's delta is exactly the rows committed THIS round.  RowIds are
        // minted densely in the FactKey-sorted commit loop below, so those rows form the
        // contiguous span `[round_lo, rel.row_count())` — captured as a range with NO
        // arena staging and NO per-round bitset (the round batch IS the delta).
        let round_lo = rel.row_count();
        // Commit winners in RESOLVED LEXICAL FactKey order — NOT any id/mint order — so
        // store/index insertion order AND the per-winner `governor.charge()` sequence
        // stay byte-deterministic.  RowId assignment is a purely ADDITIVE side effect of
        // the lockstep `rel.insert` inside this sorted loop; it never orders the commit
        // or the budget charge (mint order ≠ lexical order).  This is the columnar-store
        // determinism doctrine, matching `least_model_of_reduct`'s commit discipline.
        let mut winners: Vec<(FactKey, RuleRoundCandidate)> = round_entries;
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
            // Insert into both stores in lockstep so the columnar index order tracks the
            // ternary store's insertion order exactly, capturing the store row index (for
            // the Record-mode depth push) and the store-global dense RowId the
            // `RelationStore` stamps on the new row.  This — and the FactKey-sorted commit
            // order, the delta, and the per-winner budget charge — are provenance-
            // independent, so the committed fact set is byte-identical across modes.  A
            // winner is always a genuinely-new fact (heads already present are skipped
            // above via `store.contains_key`), so the lockstep insert returns `Some(...)`.
            let store_idx = store.insert(winner.head.clone());
            if store_idx.is_some() {
                let inserted = rel.insert(
                    &winner.head.predicate,
                    &winner.head.subject,
                    &winner.head.object,
                );
                // A winner is new in the FactStore (gated by `store.contains_key` above),
                // and the columnar store dedups on the SAME predicate + interned surfaces,
                // so it is new there too — the insert stamps the next dense RowId, keeping
                // the committed span `[round_lo, rel.row_count())` contiguous.
                assert!(
                    inserted.is_some(),
                    "a fresh winner must insert a new columnar row (dense RowId span)"
                );
            }
            // Depth bookkeeping feeds ONLY the provenance tiebreak; the facts-only lane
            // carries `prov: None`, so it is not maintained there (keeping the `depth` Vec
            // empty under Skip — the `assert!` in `evaluate` locks that invariant in
            // release builds too).  Pushed in lockstep with the store row just added, so
            // `depth[i]` stays the depth of the store's row `i`.
            if let (Some(idx), Some(prov)) = (store_idx, winner.prov.as_ref()) {
                let winner_depth = prov.proof_height;
                assert_eq!(
                    idx,
                    depth.len(),
                    "depth/store index desync: `depth` and the `FactStore` rows must stay \
                     in lockstep under Record (each committed row pushes one depth slot)"
                );
                depth.push(winner_depth);
            }
            // A winner is always a NEW key: heads already present (including every
            // EDB fact, seeded into `store` before the fixpoint) are skipped above via
            // `store.contains_key`. So every winner is a genuine derivation.  Under Skip the
            // row is not built at all (no reifier strings, no derivation-id hash, no vec growth
            // — the native analogue of the trace memory the facts-only lane must not pay).
            if let Some(prov) = winner.prov {
                derivations.push(DerivedRow {
                    graph: String::new(),
                    subject: winner.head.subject,
                    predicate: winner.head.predicate,
                    object: winner.head.object,
                    rule_iri: prov.rule_iri,
                    source_quad_ids: prov.sources, // body-order, NEVER the sorted copy
                    derivation_id: prov.deriv,
                    proof_height: prov.proof_height,
                    antecedents: prov.source_facts,
                });
            }
            governor.charge();
        }

        // The next round's delta is the contiguous RowId span committed this round —
        // `[round_lo, rel.row_count())`.  No arena read-back, no per-round bitset: the
        // dense FactKey-sorted commit order makes this range exactly the set of rows the
        // former bitset held, so the next round's `Delta`/`OldOnly` scans are byte-identical.
        delta = Delta {
            lo: round_lo,
            hi: rel.row_count(),
        };
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
/// (propagated from the shared `rule_ir` helpers).  A non-stratifiable program never
/// reaches here — it is the pipeline's `stratify()` → `None` declared gap, decided by the
/// caller (`magic::eval_with_base_fallback`) before an [`Executable`] exists; the only
/// `Unsupported` this leg raises is [`UnsupportedKind::Arithmetic`] (a builtin gap).
pub(crate) fn evaluate(
    edb: RelationStore,
    exe: &Executable,
    max_steps: Option<u64>,
) -> gmeow_errors::Result<NativeOutcome<Budgeted<Vec<Fact>>>> {
    // Lower the columnar EDB through the single shared projection used by both the
    // scratch and incremental evaluators.  It returns lexical FactKey order.
    let edb_facts = edb.facts_sorted();

    // Run the stratified fixpoint, accumulating into a shared FactStore/RelationStore.
    let mut store = FactStore::new();
    let mut rel = RelationStore::new();
    // Depth column (row-indexed, like the forward leg).  This Skip-mode leg never records
    // provenance, so it is never written — it stays empty, asserted below.
    let mut depth: Vec<ProofHeight> = Vec::new();

    // A PURE-EDB predicate (never a rule head) is settled from the seed; a self-recursive
    // or otherwise IDB-derived predicate becomes settled only when its stratum completes
    // (below), so exclude the head predicates here to avoid over-claiming.  The
    // head-predicate set is memoized on the `Executable`.
    let head_preds = exe.head_predicates();
    let mut saturated_preds: BTreeSet<String> = edb_facts
        .iter()
        .map(|f| f.predicate.clone())
        .filter(|p| !head_preds.contains(p))
        .collect();

    // This leg runs the fixpoint in `Skip` mode: it returns the full fact set
    // (`store.facts()`) and never reads provenance, so the shared fixpoint mints no
    // reifiers, no derivation ids, and no `DerivedRow`s (the native analogue of the trace
    // memory a closure-only lane must not pay).  The seed therefore also skips the
    // provenance-only `depth` bookkeeping.
    for f in &edb_facts {
        if store.insert(f.clone()).is_some() {
            rel.insert(&f.predicate, &f.subject, &f.object);
        }
    }

    // The step governor is honoured identically to the forward path (single EDB, so the
    // frontier is exact — no cross-world under-claim).
    let mut governor = StepGovernor::new(max_steps);
    let total = exe.stratum_count();
    let mut completed = 0usize;
    let mut status = BudgetStatus::Ok;
    let mut derivations: Vec<DerivedRow> = Vec::new();
    // Set iff a builtin could not be evaluated in its binding mode, or hit a
    // domain/precision error (÷0, overflow).  Such a program is a declared native
    // gap: the whole query is refused rather than presenting an incomplete
    // answer set — never a wrong answer.
    let mut builtin_gap = false;
    for k in 0..total {
        if exe.stratum_is_empty(k) {
            completed += 1;
            continue;
        }
        match eval_stratum_fixpoint(
            exe,
            k,
            &mut FixpointState {
                store: &mut store,
                rel: &mut rel,
                depth: &mut depth,
                derivations: &mut derivations,
                builtin_gap: &mut builtin_gap,
            },
            &mut governor,
            ProvenanceMode::Skip,
            RoundExecution::Parallel,
            None,
        )? {
            FixpointStatus::Complete => {
                for pred in exe.stratum_head_predicates(k) {
                    saturated_preds.insert(pred.to_owned());
                }
                completed += 1;
            }
            FixpointStatus::Exhausted => {
                status = BudgetStatus::Exhausted;
                break;
            }
        }
    }

    // Skip mode records no provenance: the shared fixpoint mints no `DerivedRow`s and never
    // writes the provenance-only `depth` map.  This is the whole point of the closure-only lane
    // — the provenance memory it must NOT pay is precisely what OOMs the trace-recording engine —
    // so the invariant is a hard `assert!` that fires in RELEASE builds too, where OOM actually
    // bites.  A `debug_assert!` here would compile out of exactly the builds the toggle protects.
    // It is O(1) (two `is_empty()` checks, once per `evaluate` call), and any future edit that let
    // `depth` gate fact derivation, or accidentally recorded a row on this lane, hard-fails here
    // instead of silently diverging the facts-only equivalence — or silently reintroducing the OOM.
    assert!(
        derivations.is_empty() && depth.is_empty(),
        "Skip-mode evaluate must record no DerivedRows and no depth entries"
    );

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

/// Deterministic evidence from the four-worker rule-parallel production path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RuleParallelProbe {
    pub(crate) worker_count: usize,
    pub(crate) rule_count: usize,
    pub(crate) seed_rows: usize,
    pub(crate) derived_rows: usize,
    pub(crate) consumed_steps: u64,
    pub(crate) parallel_rounds: u64,
    pub(crate) rule_tasks: u64,
    pub(crate) serial_candidate_rows: u64,
    pub(crate) critical_path_candidate_rows: u64,
    pub(crate) max_buffered_candidate_rows: u64,
    pub(crate) max_task_candidate_rows: u64,
    pub(crate) budget_cases: usize,
    pub(crate) output_parity: bool,
    pub(crate) budget_parity: bool,
    pub(crate) parallel_path_entered: bool,
    pub(crate) critical_path_strictly_lower: bool,
    pub(crate) closure_hash: [u8; 32],
}

fn same_derived_rows(left: &[DerivedRow], right: &[DerivedRow]) -> bool {
    left.len() == right.len()
        && left.iter().zip(right).all(|(left, right)| {
            left.graph == right.graph
                && left.subject == right.subject
                && left.predicate == right.predicate
                && left.object == right.object
                && left.rule_iri == right.rule_iri
                && left.source_quad_ids == right.source_quad_ids
                && left.derivation_id == right.derivation_id
                && left.proof_height == right.proof_height
                && left
                    .antecedents
                    .iter()
                    .map(Fact::key)
                    .eq(right.antecedents.iter().map(Fact::key))
        })
}

fn same_budgeted_rows(left: &Budgeted<Vec<DerivedRow>>, right: &Budgeted<Vec<DerivedRow>>) -> bool {
    left.status == right.status
        && left.progress == right.progress
        && left.consumed_steps == right.consumed_steps
        && same_derived_rows(&left.rows, &right.rows)
}

fn derived_rows_hash(rows: &[DerivedRow]) -> [u8; 32] {
    fn feed(hasher: &mut blake3::Hasher, value: &str) {
        hasher.update(&(value.len() as u64).to_le_bytes());
        hasher.update(value.as_bytes());
    }

    let mut hasher = blake3::Hasher::new();
    hasher.update(b"gmeow-rule-parallel-derived-rows-v1\0");
    hasher.update(&(rows.len() as u64).to_le_bytes());
    for row in rows {
        feed(&mut hasher, &row.graph);
        feed(&mut hasher, &crate::provenance::term_display(&row.subject));
        feed(&mut hasher, &row.predicate);
        feed(&mut hasher, &crate::provenance::term_display(&row.object));
        feed(&mut hasher, &row.rule_iri);
        hasher.update(&(row.source_quad_ids.len() as u64).to_le_bytes());
        for source in &row.source_quad_ids {
            feed(&mut hasher, source);
        }
        feed(&mut hasher, &row.derivation_id);
        hasher.update(&row.proof_height.get().to_le_bytes());
        hasher.update(&(row.antecedents.len() as u64).to_le_bytes());
        for antecedent in &row.antecedents {
            let key = antecedent.key();
            feed(&mut hasher, &key.0);
            feed(&mut hasher, &key.1);
            feed(&mut hasher, &key.2);
        }
    }
    *hasher.finalize().as_bytes()
}

/// Run the permanent balanced rule-parallel fixture under a real four-worker pool.
///
/// The returned work comparison is structural, not timed: the serial work is the
/// sum of rule-local candidate buffers, while the parallel critical path is the sum
/// of each round's largest task. Full output/provenance and a budget sweep are also
/// compared against the forced-sequential policy.
pub(crate) fn rule_parallel_probe() -> gmeow_errors::Result<RuleParallelProbe> {
    const NS: &str = "https://example.org/parallel/";
    const WORLD: &str = "https://example.org/parallel/world";
    let iri = |local: &str| format!("{NS}{local}");
    let atom = |predicate: &str| {
        crate::rule_ir::EvalAtom::positive(
            crate::rule_ir::EvalTerm::var("?X"),
            &iri(predicate),
            crate::rule_ir::EvalTerm::var("?X"),
        )
    };
    let rule = |name: &str, head: &str, body: &str| {
        crate::rule_ir::EvalRule::positive(
            &iri(&format!("rule/{name}")),
            atom(head),
            vec![atom(body)],
        )
    };
    let rules = vec![
        rule("z-duplicate", "shared", "seed"),
        rule("a-duplicate", "shared", "seed"),
        rule("alpha", "alpha", "seed"),
        rule("omega", "omega", "seed"),
        rule("left", "left", "shared"),
        rule("right", "right", "shared"),
    ];
    let executable = super::plan::Parsed::uncached(&rules)
        .stratify()
        .ok_or_else(|| seminaive_err("rule-parallel evidence fixture is non-stratifiable"))?
        .plan()
        .into_executable();
    let store = crate::store::WorldStore::new();
    const SEED_ROWS: usize = 24;
    for index in 0..SEED_ROWS {
        let node = iri(&format!("node-{index:02}"));
        store.insert_quad(WORLD, &node, &iri("seed"), &node);
    }
    let edb = world_edb_facts(&store, WORLD)?;
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(4)
        .build()
        .map_err(|error| seminaive_err(format!("build four-worker evidence pool: {error}")))?;

    pool.install(move || {
        let run = |max_steps: Option<u64>,
                   execution: RoundExecution,
                   trace: Option<&mut RuleParallelTrace>| {
            let mut governor = StepGovernor::new(max_steps);
            eval_world_stratified_with_trace(
                &edb,
                &executable,
                &mut governor,
                ProvenanceMode::Record,
                execution,
                trace,
            )
        };

        let sequential = run(None, RoundExecution::Sequential, None)?;
        let mut trace = RuleParallelTrace::default();
        let parallel = run(None, RoundExecution::Parallel, Some(&mut trace))?;
        let output_parity = same_budgeted_rows(&parallel, &sequential);
        const BUDGETS: [u64; 8] = [0, 1, 23, 72, 73, 119, 120, 121];
        let mut budget_parity = true;
        for budget in BUDGETS {
            let sequential_cut = run(Some(budget), RoundExecution::Sequential, None)?;
            let parallel_cut = run(Some(budget), RoundExecution::Parallel, None)?;
            budget_parity &= same_budgeted_rows(&parallel_cut, &sequential_cut);
        }

        let worker_count = rayon::current_num_threads();
        let parallel_path_entered = worker_count == 4 && trace.parallel_rounds > 0;
        let critical_path_strictly_lower = trace.critical_path_candidate_rows > 0
            && trace.critical_path_candidate_rows < trace.serial_candidate_rows;
        Ok(RuleParallelProbe {
            worker_count,
            rule_count: rules.len(),
            seed_rows: SEED_ROWS,
            derived_rows: parallel.rows.len(),
            consumed_steps: parallel.consumed_steps,
            parallel_rounds: trace.parallel_rounds,
            rule_tasks: trace.rule_tasks,
            serial_candidate_rows: trace.serial_candidate_rows,
            critical_path_candidate_rows: trace.critical_path_candidate_rows,
            max_buffered_candidate_rows: trace.max_buffered_candidate_rows,
            max_task_candidate_rows: trace.max_task_candidate_rows,
            budget_cases: BUDGETS.len(),
            output_parity,
            budget_parity,
            parallel_path_entered,
            critical_path_strictly_lower,
            closure_hash: derived_rows_hash(&parallel.rows),
        })
    })
}
