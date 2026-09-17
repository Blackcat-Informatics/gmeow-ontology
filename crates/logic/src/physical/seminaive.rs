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
//! checked by the permanent native parallel/budget parity fixture in [`crate::cost`].
//!
//! # Stratification (dynamic) + negation
//!
//! The signed dependency graph connects producers whose statement patterns can
//! intersect. NAF and structural lookups require completed predecessors; positive
//! reads may share a fixed point. A strict edge inside a cycle is non-stratifiable
//! → [`NativeOutcome::Unsupported`]`(`[`UnsupportedKind::NonStratifiable`]`)`.
//! Strata run in increasing order. A predicate is marked settled only after its
//! last possible writer completes, including writers of disjoint class markers.
//!
//! # Internal helper coverage
//!
//! The production native paths and focused parity tests exercise different subsets
//! of this module's kernels. The module-level `dead_code` allowance keeps those
//! verification helpers together rather than scattering per-item attributes.
#![allow(dead_code)]

#[cfg(test)]
mod builtin_tests;
pub(crate) mod joint;
pub(crate) mod property;
mod reduce;

use std::collections::BTreeSet;

use hashbrown::HashTable;
use rayon::prelude::*;

use crate::physical::builtin_eval::{
    BuiltinGap, BuiltinOutcome, CellResolver, MathTriples, emit_term, eval_native,
    load_dimension_cells, load_gram_cells, load_vector_dense,
};
use crate::physical::cursor::{LendingIterator, VALUE_OBJECT, VALUE_SUBJECT, ValueCursor};
use crate::physical::id::{RowId, TermId};
use crate::physical::plan::{
    AtomKernel, AtomOperator, CyclicPlan, Executable, IndexChoice, JoinGroup, RulePlan,
};
use crate::physical::store::{Bound, RelationStore};
use crate::provenance::{MinProofHeightSemiring, ProofHeight, mint_derivation_id, term_display};
use crate::query_ir::QBuiltin;
use crate::rule_ir::{
    DerivedRow, EvalAtom, EvalRule, EvalTerm, Fact, FactKey, FactStore, Provenance,
    RuleRoundCandidate, Solution, distinct_pairs_satisfied, echo_asserted, fact_key_hash,
    ground_head, ground_relational_head, sort_rows, world_edb_facts,
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
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum UnsupportedKind {
    /// A negative dependency-graph edge lies inside a cycle — no stratification exists.
    NonStratifiable,
    /// A `!`/cut control construct (no declarative bottom-up meaning).
    Cut,
    /// An arithmetic / builtin the native core could not evaluate in its binding mode,
    /// or that hit a typed `math:` domain fault (÷0, overflow, incommensurable
    /// dimensions). The payload is the ledgerable per-solution [`BuiltinGap`]s the
    /// evaluator captured — the KIND, the operation, and the antecedent operands — so a
    /// terminal mints a ledgered finding naming the `math:` class rather than an
    /// anonymous refusal. Empty for a STRUCTURAL refusal (a builtin encountered on a
    /// path that never evaluates one — the generic magic / FOL lowerings), which carries
    /// no evaluated gap.
    Arithmetic(Vec<BuiltinGap>),
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
    /// A clause body with more than 64 literals: the backward solver represents the
    /// set of not-yet-selected body literals as a `u64` bitmask (one bit per literal),
    /// so a wider body exceeds the mask. This is an explicit typed refusal (an authored
    /// program can always be normalized to bodies ≤ 64 literals), never a silent cap.
    ClauseBodyTooWide,
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

/// Whether a stratum completed, exhausted its budget, or met an undefined builtin.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FixpointStatus {
    /// The fixpoint reached `round.is_empty()` within budget.
    Complete,
    /// The `max_steps` budget was exhausted mid-fixpoint; the committed prefix is a
    /// sound (FactKey-ordered) partial least model.
    Exhausted,
    /// An ordinary builtin failed; no candidate in its round may be committed.
    BuiltinGap,
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

    /// Continue a measured native prefix under the original total allowance.
    pub(crate) fn from_consumed(max_steps: Option<u64>, consumed: u64) -> Self {
        assert!(
            max_steps.is_none_or(|limit| consumed <= limit),
            "prefix exceeds its governor"
        );
        Self {
            limit: max_steps,
            consumed,
        }
    }

    /// Temporarily tighten this same governor for one bounded native operation.
    /// A returned error/refusal restores the caller's ceiling too; committed work
    /// always remains charged exactly once.
    pub(crate) fn with_backstop<T>(
        &mut self,
        backstop: u64,
        operation: impl FnOnce(&mut Self) -> T,
    ) -> T {
        let original = self.limit;
        let ceiling = self.consumed.saturating_add(backstop);
        self.limit = Some(original.map_or(ceiling, |limit| limit.min(ceiling)));
        let result = operation(self);
        self.limit = original;
        result
    }

    /// Remaining commit allowance; absence means the caller selected no limit.
    pub(crate) fn remaining(&self) -> Option<u64> {
        self.limit.map(|limit| limit.saturating_sub(self.consumed))
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

    /// The per-call working-set cap a budgeted consumer may materialize, as a `usize`.
    ///
    /// `None` (unbudgeted) is [`usize::MAX`] — no cap, byte-identical to the pre-budget
    /// behavior — so this only bounds an explicitly budgeted run. A budgeted run can
    /// never COMMIT more than `limit` derivations, so it never needs to hold more than
    /// `limit` pending solutions/rows at once; capping an intermediate materialization
    /// at this value bounds a single round's memory to the same ceiling as the whole
    /// derivation, turning a super-polynomial per-round blow-up into a sound
    /// `Exhausted` withhold instead of an out-of-memory abort.
    pub(crate) fn solution_cap(&self) -> usize {
        self.limit
            .map_or(usize::MAX, |l| usize::try_from(l).unwrap_or(usize::MAX))
    }
}

// ── Index-selected semi-naive join ────────────────────────────────────────────────

/// Flat physical binding frame for the acyclic binary join.
///
/// Slots are assigned once by [`RulePlan`]. A row probe is therefore two direct indexed
/// reads over interned term IDs instead of copying display strings or repeatedly
/// searching `(variable_name, value)` pairs. The
/// native named [`Solution`] is reconstructed once after the positive join because the
/// post-join builtin/NAF/head helpers remain the shared semantic authority.
#[derive(Clone)]
struct SlotSolution {
    bindings: Vec<Option<TermId>>,
    source_facts: Vec<Fact>,
}

impl SlotSolution {
    fn empty(slot_count: usize) -> Self {
        Self {
            bindings: vec![None; slot_count],
            source_facts: Vec::new(),
        }
    }

    fn get(&self, slot: usize) -> Option<TermId> {
        self.bindings[slot]
    }

    fn into_named(self, variables: &[String], rel: &RelationStore) -> Solution {
        debug_assert_eq!(self.bindings.len(), variables.len());
        let bindings = variables
            .iter()
            .zip(self.bindings)
            .filter_map(|(name, value)| {
                value.map(|value| (name.clone(), rel.interner().resolve(value).clone()))
            })
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
    if rel.semantics.alternate_predicate(&atom.predicate).is_some() {
        return extend_slot_semantic::<SCAN>(operator, atom, rel, delta, solutions);
    }
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

/// The same indexed join under the selected grounded operator contract. Only
/// constant marker probes expand; variables bind the actual selected term.
fn extend_slot_semantic<const SCAN: u8>(
    operator: &AtomOperator,
    atom: &EvalAtom,
    rel: &RelationStore,
    delta: Delta,
    solutions: &[SlotSolution],
) -> Vec<SlotSolution> {
    let (subject_slot, subject_constant, object_slot, object_constant) = match operator.kernel() {
        AtomKernel::Vars {
            subject_slot,
            object_slot,
        } => (Some(*subject_slot), None, Some(*object_slot), None),
        AtomKernel::VarConst {
            subject_slot,
            object,
        } => (Some(*subject_slot), None, None, Some(object)),
        AtomKernel::ConstVar {
            subject,
            object_slot,
        } => (None, Some(subject), Some(*object_slot), None),
        AtomKernel::Consts { subject, object } => (None, Some(subject), None, Some(object)),
    };
    let mut next = Vec::new();
    for solution in solutions {
        let subject = subject_constant.or_else(|| {
            subject_slot
                .and_then(|slot| solution.get(slot))
                .map(|id| rel.interner().resolve(id))
        });
        let object = object_constant.or_else(|| {
            object_slot
                .and_then(|slot| solution.get(slot))
                .map(|id| rel.interner().resolve(id))
        });
        let mut rows = rel.select_pattern(
            &atom.predicate,
            subject,
            object,
            matches!(atom.object, EvalTerm::ConstNamed(_)),
            true,
        );
        while let Some((subject, object, row, predicate)) = rows.next() {
            if !keep_row::<SCAN>(delta, row)
                || (subject_slot.is_some() && subject_slot == object_slot && subject != object)
            {
                continue;
            }
            let mut merged = solution.clone();
            if let Some(slot) = subject_slot {
                merged.bindings[slot] = Some(subject);
            }
            if let Some(slot) = object_slot {
                merged.bindings[slot] = Some(object);
            }
            merged.source_facts.push(Fact {
                subject: rel.interner().resolve(subject).clone(),
                predicate: predicate.to_owned(),
                object: rel.interner().resolve(object).clone(),
            });
            next.push(merged);
        }
    }
    next
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
                let Some(subject) = solution.get(subject_slot) else {
                    continue;
                };
                Bound::Subject(subject)
            }
            INDEX_OBJECT => {
                let Some(object) = solution.get(object_slot) else {
                    continue;
                };
                Bound::Object(object)
            }
            INDEX_BOTH => {
                let (Some(subject), Some(object)) =
                    (solution.get(subject_slot), solution.get(object_slot))
                else {
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
                merged.bindings[subject_slot] = Some(subject_id);
            }
            if (INDEX == INDEX_ANY || INDEX == INDEX_SUBJECT) && object_slot != subject_slot {
                merged.bindings[object_slot] = Some(object_id);
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
    object: &purrdf::TermValue,
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
                let Some(subject_id) = solution.get(subject_slot) else {
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
                merged.bindings[subject_slot] = Some(subject_id);
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
    subject: &purrdf::TermValue,
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
                let Some(object_id) = solution.get(object_slot) else {
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
                merged.bindings[object_slot] = Some(object_id);
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
    subject: &purrdf::TermValue,
    object: &purrdf::TermValue,
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
/// tuple is wrapped as a [`Fact`] and handed to [`crate::rule_ir::match_atom`] exactly as
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
    let interner = rel.interner();
    let mut next: Vec<Solution> = Vec::new();
    for sol in solutions {
        let mut cursor = rel.select_atom(atom, sol);
        while let Some((s_id, o_id, row_id, predicate)) = cursor.next() {
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
                predicate: predicate.to_owned(),
                object: interner.resolve(o_id).clone(),
            };
            if let Some(mut merged) = rel.match_selected(atom, &f, sol) {
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
        let other = solution.get(*object_slot);
        Some(LeapfrogValueCursor::subject(
            rel.values_subject(atom.predicate.as_str(), other),
            scan,
            delta,
        ))
    } else if *object_slot == variable_slot {
        let other = solution.get(*subject_slot);
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
        let (subject_id, object_id) = (subject, object);
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
        let externally_bound = solution.get(variable_slot);
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
            solution.bindings[variable_slot] = Some(value);
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
    gap: &mut Vec<BuiltinGap>,
) -> gmeow_errors::Result<Vec<Solution>> {
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
        .map(|solution| solution.into_named(plan.variables(), rel))
        .collect();

    let numeric = plan.numeric(&rule.rule_iri)?;
    if !rule.builtins.is_empty() {
        let resolver = RelationCellResolver { store: accumulated };
        solutions = apply_builtins(
            &rule.builtins,
            solutions,
            gap,
            &resolver,
            rule.constraint_tag.is_some(),
        );
    }
    solutions = numeric.apply_all(&rule.rule_iri, solutions)?;
    if !plan.negated().is_empty() {
        solutions.retain(|solution| {
            !plan
                .negated()
                .iter()
                .any(|&index| negated_atom_satisfied(&rule.body[index], solution, accumulated))
        });
    }
    Ok(solutions)
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
    gap: &mut Vec<BuiltinGap>,
) -> gmeow_errors::Result<Vec<Solution>> {
    plan.numeric(&rule.rule_iri)?;
    if plan.has_cyclic_subplan()
        && !rule
            .body
            .iter()
            .any(|atom| rel.semantics.alternate_predicate(&atom.predicate).is_some())
    {
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
    gap: &mut Vec<BuiltinGap>,
) -> gmeow_errors::Result<Vec<Solution>> {
    // The positive (join) / negated (NAF) body-atom partition was precomputed ONCE at
    // plan time ([`RulePlan`]); the per-round `filter(..).collect()` allocation is gone.
    // Both slices are body-order indices into `rule.body`, so the produced solution
    // sequence is byte-identical to the previous per-round partition.
    let positive = plan.positive();
    let negated = plan.negated();

    let mut solutions: Vec<Solution> = if positive.is_empty() {
        // The empty conjunction is relational identity: one empty substitution. This
        // lets unconditional, NAF-only, and constraint-only rules fire once; duplicate
        // heads are suppressed by the store on the following round.
        vec![Solution {
            bindings: Vec::new(),
            source_facts: Vec::new(),
        }]
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
            .map(|solution| solution.into_named(plan.variables(), rel))
            .collect()
    };

    // Post-join constraint stage: evaluate the rule's arithmetic/comparison
    // builtins in body order.  A generator (`is` with a free target) binds its
    // target — available to `ground_head` and to the negated check below; a filter
    // prunes the solution.  This runs BEFORE the NAF retain so a negated atom over
    // a generator-bound variable sees the binding.
    let numeric = plan.numeric(&rule.rule_iri)?;
    if !rule.builtins.is_empty() {
        let resolver = RelationCellResolver { store: accumulated };
        solutions = apply_builtins(
            &rule.builtins,
            solutions,
            gap,
            &resolver,
            rule.constraint_tag.is_some(),
        );
    }

    solutions = numeric.apply_all(&rule.rule_iri, solutions)?;
    if !negated.is_empty() {
        solutions.retain(|sol| {
            !negated
                .iter()
                .any(|&i| negated_atom_satisfied(&rule.body[i], sol, accumulated))
        });
    }

    Ok(solutions)
}

/// A [`CellResolver`] reading the exact-rational `math:` Gram/vector cells out of the
/// accumulated columnar [`RelationStore`] the semi-naive fixpoint has built (the same
/// store the forward and demand/magic legs both accumulate into). IRIs are addressed in
/// the store's borrowed native IRI probe; the cell walk mirrors `gmeow_math`'s graph
/// loaders over the store's `(subject, predicate) → objects` index, so the shared
/// loaders build the form identically regardless of substrate.
struct RelationCellResolver<'a> {
    store: &'a RelationStore,
}

impl MathTriples for RelationCellResolver<'_> {
    fn math_iri_objects(&self, subject: &str, predicate: &str) -> Vec<String> {
        let Some(sid) = self.store.iri_id(subject) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        let mut cursor = self.store.select_semantic(predicate, Bound::Subject(sid));
        while let Some((_s, object, _row, _predicate)) = cursor.next() {
            if let purrdf::TermValue::Iri(iri) = self.store.interner().resolve(object) {
                out.push(iri.clone());
            }
        }
        out
    }

    fn math_literal_i128(&self, subject: &str, predicate: &str) -> Option<i128> {
        let sid = self.store.iri_id(subject)?;
        let mut cursor = self.store.select_semantic(predicate, Bound::Subject(sid));
        while let Some((_s, object, _row, _predicate)) = cursor.next() {
            if let purrdf::TermValue::Literal { lexical_form, .. } =
                self.store.interner().resolve(object)
                && let Ok(n) = lexical_form.trim().parse::<i128>()
            {
                return Some(n);
            }
        }
        None
    }
}

impl CellResolver for RelationCellResolver<'_> {
    fn gram(&self, iri: &str) -> Option<Vec<(usize, usize, gmeow_math::Rational)>> {
        load_gram_cells(self, iri)
    }
    fn vector(&self, iri: &str) -> Option<Vec<gmeow_math::Rational>> {
        load_vector_dense(self, iri)
    }
    fn dimension(&self, iri: &str) -> Option<gmeow_math::dimension::DimVector> {
        // The ONLY substrate the `math:` dimension-gate builtins (`DimEqual`/
        // `DimProduct`) ever probe: a constraint-tagged violation rule's forward
        // chase seeds its `RelationStore` from the FULL asserted quad set (via
        // `WorldStore::load_dataset`, never the literal-dropping typed-EDB fact
        // stream), so a dimension's `math:baseDimensionExponent` cells — numerator
        // and denominator literals included — are present here to walk.
        load_dimension_cells(self, iri)
    }
}

/// Evaluate a rule's arithmetic/comparison builtins against each candidate
/// solution, in body order, via the shared moded evaluator.
///
/// A generator extends the solution's bindings with the computed value in the
/// native computed literal. For an ORDINARY (untagged) rule a filter keeps
/// or prunes the solution as usual, and an operand that is still unbound, or a
/// domain/precision error (÷0, overflow), sets `gap` and drops the solution — the
/// caller then surfaces a typed refusal for the WHOLE program rather than present
/// an incomplete native answer, so a dropped solution is never a wrong answer.
///
/// `constraint_tagged` — `true` for a `logic:Constraint`-derived VIOLATION-EMITTING
/// rule ([`EvalRule::constraint_tag`](crate::rule_ir::EvalRule::constraint_tag))
/// — INVERTS the filter semantics: a builtin `Filter(true)` (the law's consequent
/// HOLDS) prunes the solution (no violation, no marker), while `Filter(false)` (the
/// law's consequent does NOT hold) KEEPS it so the caller's head grounding
/// materializes the constraint's failure-class marker. Undefinedness (`Unbound` — an
/// operand's dimension could not be resolved) is likewise NOT a violation for a
/// tagged rule: it silently prunes ONLY that one candidate solution — never the
/// whole batch, and never a ledgered gap — exactly mirroring the missing/malformed
/// dimension being delegated elsewhere (the retained native malformed-dimension
/// scan). `Error` never arises from a dimension-gate builtin by construction (an
/// unresolvable operand or a ⊕ overflow both decline to `Unbound`), but is handled
/// identically for safety.
fn apply_builtins(
    builtins: &[QBuiltin],
    mut sols: Vec<Solution>,
    gap: &mut Vec<BuiltinGap>,
    resolver: &dyn CellResolver,
    constraint_tagged: bool,
) -> Vec<Solution> {
    let mut failed = false;
    sols.retain_mut(|sol| {
        if failed {
            return false;
        }
        for b in builtins {
            // Borrow native values until evaluation finishes, then extend the
            // same solution for a generator. Text exists only in failure evidence.
            let outcome = eval_native(b, &|name| sol.get(name), resolver);
            match outcome {
                BuiltinOutcome::Filter(holds) => {
                    if holds == constraint_tagged {
                        // Ordinary rule, filter false → prune; OR constraint-tagged
                        // rule, the law's consequent HOLDS → the law is satisfied,
                        // no violation → prune.
                        return false;
                    }
                    // Ordinary rule, filter true → keep (fall through); OR
                    // constraint-tagged rule, the consequent does NOT hold → keep,
                    // so the head materializes the violation marker.
                }
                BuiltinOutcome::Generate { var, value } => {
                    sol.bindings.push((var, emit_term(&value)));
                }
                BuiltinOutcome::Unbound | BuiltinOutcome::Error(_) => {
                    if constraint_tagged {
                        // Undefinedness is NOT a violation: skip ONLY this candidate
                        // solution, never poison the rest of the batch, and never
                        // record a ledgered gap (the missing/malformed dimension is
                        // handled elsewhere).
                        return false;
                    }
                    // A single unbound operand / domain error refuses the WHOLE program,
                    // so the remaining solutions cannot change the outcome — capture the
                    // typed gap (KIND + operation + antecedent operands) and stop
                    // evaluating. `from_outcome` is total for these declining arms, so the
                    // gap is never anonymous.
                    if let Some(captured) = BuiltinGap::from_outcome(
                        b,
                        &outcome,
                        sol.bindings
                            .iter()
                            .map(|(name, value)| (name.clone(), term_display(value)))
                            .collect(),
                    ) {
                        gap.push(captured);
                    }
                    failed = true;
                    return false;
                }
            }
        }
        true
    });
    if failed {
        sols.clear();
    }
    sols
}

/// Whether a negated atom has a matching row in the completed lower-stratum store.
/// Constant marker interpretation and exact variable bindings use the same indexed
/// probes as positive joins. Partial bindings retain existential NAF semantics.
fn negated_atom_satisfied(atom: &EvalAtom, sol: &Solution, accumulated: &RelationStore) -> bool {
    accumulated.select_atom(atom, sol).any_remaining()
}

// ── Stratification ───────────────────────────────────────────────────────────────

/// Assign each producer a stratum using its typed read/write effects. Positive
/// recursion shares an SCC; a strict read within that SCC refuses the program.
/// The input boundary precedes NAF and structural builtin decisions.
pub(crate) fn stratify(rules: &[EvalRule]) -> Option<Vec<usize>> {
    let effects: Vec<_> = rules
        .iter()
        .map(super::effects::ProducerEffect::rule)
        .collect();
    super::effects::schedule(
        &effects,
        crate::native_semantics::SemanticVocabulary::Exact,
        &BTreeSet::new(),
    )
    .ok()
    .map(|schedule| schedule.strata)
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
/// provenance-recipe failure, or an ordinary builtin mode/domain failure. A failing
/// builtin round is never published, even when earlier candidates succeeded.
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
    // The round stops before commit on an ordinary builtin gap. A tagged
    // violation rule's undefined candidates are pruned under its distinct contract.
    let mut builtin_gap: Vec<BuiltinGap> = Vec::new();
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
            FixpointStatus::BuiltinGap => {
                return Err(seminaive_err(
                    crate::reason::builtin_gap::builtin_gap_refusal_detail(&builtin_gap),
                ));
            }
            FixpointStatus::Exhausted => {
                // The budget cut this stratum mid-fixpoint: it is NOT saturated, and no
                // later stratum runs.  The committed prefix stays (sound partial model).
                status = BudgetStatus::Exhausted;
                break;
            }
        }
    }

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
    builtin_gap: &'a mut Vec<BuiltinGap>,
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
    builtin_gap: Vec<BuiltinGap>,
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
            builtin_gap: Vec::new(),
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
    fn merge_from(&mut self, mut other: Self, mode: ProvenanceMode) -> gmeow_errors::Result<()> {
        self.builtin_gap.append(&mut other.builtin_gap);
        for (key, candidate) in other.entries {
            self.insert(key, candidate, mode)?;
        }
        Ok(())
    }
}

/// Build the common provenance winner from actual source rows in the shared store.
fn record_candidate(
    rule_iri: &str,
    head: Fact,
    source_facts: &[Fact],
    snapshot: RoundSnapshot<'_>,
) -> gmeow_errors::Result<RuleRoundCandidate> {
    // Provenance: reifiers of matched POSITIVE body facts in body order.
    let mut sources: Vec<String> = Vec::with_capacity(source_facts.len());
    let mut max_sd = ProofHeight::ASSERTED;
    let mut sum_sd: u64 = 0;
    for sf in source_facts {
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
    let deriv = mint_derivation_id(rule_iri, &source_refs);
    let mut sorted_sources = sources.clone();
    sorted_sources.sort();

    Ok(RuleRoundCandidate {
        head,
        prov: Some(Provenance {
            cross_world: None,
            sources,
            sorted_sources,
            source_facts: source_facts.to_vec(),
            deriv,
            rule_iri: rule_iri.to_owned(),
            proof_height,
            sum_src_depth: sum_sd,
        }),
    })
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
    // Every reduce read has completed before this stratum. Later deltas cannot
    // change its group membership, so only the initial all-input round runs it.
    if rule.reduction.is_some() && snapshot.delta.lo != 0 {
        return Ok(());
    }
    for sol in rule_solutions(
        rule,
        plan,
        snapshot.rel,
        snapshot.rel,
        snapshot.delta,
        &mut round.builtin_gap,
    )? {
        let head = match snapshot.mode {
            ProvenanceMode::Record => ground_head(&rule.head, &sol)?,
            ProvenanceMode::Skip => ground_relational_head(&rule.head, &sol)?,
        };
        let key = head.key();
        if snapshot.store.contains_key(&key) {
            continue; // a prior round/stratum already derived it; earlier wins
        }

        let candidate = match snapshot.mode {
            ProvenanceMode::Record => {
                record_candidate(&rule.rule_iri, head, &sol.source_facts, snapshot)?
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

/// Evaluate the complete rule body, its guards and its optional typed reduction.
/// Annotation and membership execution share this operator boundary.
pub(super) fn rule_solutions(
    rule: &EvalRule,
    plan: &RulePlan,
    rel: &RelationStore,
    accumulated: &RelationStore,
    delta: Delta,
    gaps: &mut Vec<BuiltinGap>,
) -> gmeow_errors::Result<Vec<Solution>> {
    let delta = if rule.reduction.is_some() {
        Delta::all(rel.row_count())
    } else {
        delta
    };
    let mut admitted = join_body_indexed(rule, plan, rel, accumulated, delta, gaps)?;
    if !rule.distinct_pairs.is_empty() {
        let mut failure = None;
        admitted.retain(|solution| {
            match distinct_pairs_satisfied(&rule.distinct_pairs, solution) {
                Ok(keep) => keep,
                Err(error) => {
                    failure = Some(error);
                    false
                }
            }
        });
        if let Some(error) = failure {
            return Err(error);
        }
    }
    match &rule.reduction {
        Some(reduction) => reduce::evaluate(&rule.rule_iri, reduction, admitted),
        None => Ok(admitted),
    }
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
    let mut delta = Delta::all(state.rel.row_count());
    loop {
        let mut round = evaluate_round_candidates(
            exe,
            stratum,
            RoundSnapshot {
                store: state.store,
                rel: state.rel,
                depth: state.depth,
                delta,
                mode,
            },
            round_execution,
            parallel_trace.as_deref_mut(),
        )?;
        state.builtin_gap.append(&mut round.builtin_gap);
        if !state.builtin_gap.is_empty() {
            return Ok(FixpointStatus::BuiltinGap);
        }
        if round.entries.is_empty() {
            return Ok(FixpointStatus::Complete);
        }
        let round_lo = state.rel.row_count();
        if commit_round(round.entries, state, governor, &BTreeSet::new())?
            == FixpointStatus::Exhausted
        {
            return Ok(FixpointStatus::Exhausted);
        }
        delta = Delta {
            lo: round_lo,
            hi: state.rel.row_count(),
        };
    }
}

/// The sole sorted commit for ordinary and joint fixed points. New facts enter
/// both indexed stores, proof heights and provenance under the same governor.
fn commit_round(
    entries: Vec<(FactKey, RuleRoundCandidate)>,
    state: &mut FixpointState<'_>,
    governor: &mut StepGovernor,
    retained_heads: &BTreeSet<FactKey>,
) -> gmeow_errors::Result<FixpointStatus> {
    let store = &mut *state.store;
    let rel = &mut *state.rel;
    let depth = &mut *state.depth;
    let derivations = &mut *state.derivations;
    // Commit winners in RESOLVED LEXICAL FactKey order — NOT any id/mint order — so
    // store/index insertion order AND the per-winner `governor.charge()` sequence
    // stay byte-deterministic.  RowId assignment is a purely ADDITIVE side effect of
    // the lockstep `rel.insert` inside this sorted loop; it never orders the commit
    // or the budget charge (mint order ≠ lexical order).  This is the columnar-store
    // determinism doctrine, matching `least_model_of_reduct`'s commit discipline.
    let mut winners = entries;
    winners.sort_by(|(a, _), (b, _)| a.cmp(b));
    let mut exhausted = false;
    for (key, winner) in winners {
        // Another producer's atomic metadata publication may have supplied this
        // frozen-round candidate. It is neither a new row nor another charge.
        if store.contains_key(&key) {
            continue;
        }
        // A retained head is free only when its source-bound cached proof was
        // independently eligible in this exact frozen round. Its proof still
        // competes normally with every fresh firing before this common writer.
        let charged = !retained_heads.contains(&key)
            && !matches!(
                winner
                    .prov
                    .as_ref()
                    .and_then(|provenance| provenance.cross_world.as_ref())
                    .map(crate::modal::native::NativeCrossWorldEvidence::publication_cost),
                Some(crate::modal::native::NativePublicationCost::ContextualMetadata { .. })
            );
        // Fresh inference rows consume one step at this deterministic boundary.
        // Once spent, omit further inference rows but retain complete contextual
        // metadata whose assessment was already charged by its evaluator.
        // Duplicate facts and assessment publication never charge a second time.
        if charged && governor.spent() {
            exhausted = true;
            continue;
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
                cross_world: prov.cross_world,
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
        if charged {
            governor.charge();
        }
    }
    Ok(if exhausted {
        FixpointStatus::Exhausted
    } else {
        FixpointStatus::Complete
    })
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
    let mut builtin_gap: Vec<BuiltinGap> = Vec::new();
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
            FixpointStatus::BuiltinGap => break,
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

    if !builtin_gap.is_empty() {
        return Ok(NativeOutcome::Unsupported(UnsupportedKind::Arithmetic(
            builtin_gap,
        )));
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
    fn feed(hasher: &mut blake3::Hasher, value: impl AsRef<[u8]>) {
        let value = value.as_ref();
        hasher.update(&(value.len() as u64).to_le_bytes());
        hasher.update(value);
    }

    let mut hasher = blake3::Hasher::new();
    hasher.update(b"gmeow-rule-parallel-derived-rows-v2\0");
    hasher.update(&(rows.len() as u64).to_le_bytes());
    for row in rows {
        feed(&mut hasher, &row.graph);
        feed(&mut hasher, row.subject.to_canonical_bytes());
        feed(&mut hasher, &row.predicate);
        feed(&mut hasher, row.object.to_canonical_bytes());
        feed(&mut hasher, &row.rule_iri);
        hasher.update(&(row.source_quad_ids.len() as u64).to_le_bytes());
        for source in &row.source_quad_ids {
            feed(&mut hasher, source);
        }
        feed(&mut hasher, &row.derivation_id);
        hasher.update(&row.proof_height.get().to_le_bytes());
        hasher.update(&(row.antecedents.len() as u64).to_le_bytes());
        for antecedent in &row.antecedents {
            feed(&mut hasher, antecedent.subject.to_canonical_bytes());
            feed(&mut hasher, &antecedent.predicate);
            feed(&mut hasher, antecedent.object.to_canonical_bytes());
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
