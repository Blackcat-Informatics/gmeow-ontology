// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Differential maintenance for finite positive binary Datalog.
//!
//! This is the stateful sibling of [`super::seminaive`].  The scratch evaluator
//! computes one least fixed point; this module keeps the fixed point's **inner
//! iteration history** and adjusts it when an outer transaction supplies a signed
//! EDB batch.  That nested-time shape is the recursive DBSP construction:
//!
//! * facts and changes are Z-sets (`i64` weights),
//! * `distinct` maps a positive raw weight to set membership,
//! * an n-way rule join is differentiated mechanically with the telescoping product
//!   `new[..p] × delta[p] × old[p+1..]`, and
//! * the adjusted inner stream is evaluated until the new least fixed point settles.
//!
//! Keeping the iteration history is load-bearing for deletion correctness.  Merely
//! iterating from the old closure computes a greatest fixed point after a retraction
//! and can retain a mutually-supporting cycle whose last asserted ground was removed.
//! The nested stream instead adjusts every prior iteration, so such a cycle disappears
//! at the same finite depth at which it originally became grounded.
//!
//! # Honest fragment boundary
//!
//! The circuit admits only finite, positive, binary Datalog: at least one positive
//! body atom per rule, no NAF, no arithmetic generators/filters, and no existential
//! heads.  Callers must keep the existential chase, well-founded/stable-model solving,
//! and modal/paraconsistent facets on their explicitly non-incremental paths.  The
//! constructor hard-fails on an uncovered rule instead of silently rebuilding and
//! presenting that rebuild as incremental.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use super::seminaive::StepGovernor;
use crate::provenance::{
    MinProofHeightSemiring, ProofHeight, ProvenanceRing, ProvenanceSemiring, ZWeightSemiring,
};
use crate::rule_ir::{
    EvalRule, EvalTerm, Fact, FactKey, FactStore, Solution, distinct_pairs_satisfied, ground_head,
    match_atom,
};
use crate::seam::BudgetStatus;

const INCREMENTAL_SOLVER_VERSION: &str = "gmeow-native-dbsp-v1";

fn incremental_err(detail: impl Into<String>) -> gmeow_errors::Diag {
    gmeow_errors::Diag::of_kind(crate::error::Reason {
        detail: detail.into(),
    })
}

/// The immutable identity under which an incremental state is valid.
///
/// The contract hash is supplied by the caller.  The rule hash is minted from the
/// canonical rule rendering and the solver version is pinned here.  Because a session
/// owns its rules, no update API can accidentally apply a delta under a different rule
/// set; consumers additionally compare this identity when retrieving cached sessions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct IncrementalIdentity {
    pub(crate) contract_hash: String,
    pub(crate) rule_hash: String,
    pub(crate) solver_version: &'static str,
}

impl IncrementalIdentity {
    fn new(contract_hash: impl Into<String>, rules: &[EvalRule]) -> Self {
        let rule_hash = super::plan::canonical_rule_hash(rules);
        Self {
            contract_hash: contract_hash.into(),
            rule_hash: rule_hash.iter().map(|byte| format!("{byte:02x}")).collect(),
            solver_version: INCREMENTAL_SOLVER_VERSION,
        }
    }
}

/// One row of a signed input or output batch.
///
/// `+1` means insertion and `-1` means retraction at the set boundary.  A transaction
/// may contain duplicate rows; they are consolidated by [`IncrementalSession::apply`]
/// before the 0/1 set-membership invariant is checked.
#[derive(Debug, Clone)]
pub(crate) struct SignedFact {
    pub(crate) fact: Fact,
    pub(crate) weight: i64,
}

/// The closure change produced by one committed transaction.
#[derive(Debug, Clone)]
pub(crate) struct IncrementalDelta {
    /// Signed closure rows in lexical [`FactKey`] order.
    pub(crate) changes: Vec<SignedFact>,
    /// Number of adjusted inner fixed-point iterations for this transaction.
    pub(crate) inner_iterations: usize,
    /// Deterministic count of signed-delta rows admitted at differentiated join
    /// positions. This is diagnostic evidence, not wall-clock timing.
    pub(crate) joined_rows: u64,
    /// One real rule firing for each newly-present derived fact.  Asserted changes
    /// have no entry.  This keeps loop consumers provenance-bearing without storing
    /// the full polynomial lineage in the incremental state.
    pub(crate) derivations: BTreeMap<FactKey, IncrementalDerivation>,
}

/// A concrete proof witness emitted for a newly-present derived fact.
///
/// `proof_height` is the fact's minimal-proof-height annotation over the
/// [`MinProofHeightSemiring`] — `1 + max(premise heights)` for the height-first
/// canonical firing that produced it, exactly the annotation the full forward
/// reasoner ([`crate::reason::reason_program`]) computes. It is reconstructed from a
/// min-height fixpoint over the settled snapshot (see
/// [`IncrementalSession::settled_heights`]), never delta-maintained through the Z-set
/// circuit (the tropical `(min, max)` semiring has no additive inverse and cannot be
/// pushed through a signed deletion).
#[derive(Debug, Clone)]
pub(crate) struct IncrementalDerivation {
    pub(crate) rule_iri: String,
    pub(crate) premises: Vec<Fact>,
    pub(crate) proof_height: ProofHeight,
}

/// The deterministic total-order key that selects the canonical proof witness for one
/// derived fact, mirroring `crate::rule_ir::RuleRoundCandidate::tiebreak_key` — smaller
/// wins. `proof_height` is the leading component, so the winner is always a
/// minimal-height derivation.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct WitnessTiebreak {
    proof_height: ProofHeight,
    source_height_sum: u64,
    sorted_reifiers: Vec<String>,
    rule_iri: String,
    source_reifiers: Vec<String>,
}

/// A governed insert-only transaction result.
#[derive(Debug, Clone)]
pub(crate) struct BudgetedIncrementalDelta {
    pub(crate) delta: IncrementalDelta,
    /// Sound closure at the deterministic cut (full closure on `Ok`).
    pub(crate) closure: Vec<Fact>,
    pub(crate) status: BudgetStatus,
    pub(crate) consumed_steps: u64,
}

/// A weighted grounded head before it is interned into the session's fact arena.
///
/// Only the signed `weight` (the compact support account) is carried here; the
/// per-fact proof witness AND its minimal proof height are reconstructed
/// authoritatively from the settled snapshot after the circuit settles (see
/// [`IncrementalSession::reconstruct_derivations`]), so no transient candidate
/// witness is threaded through the differential pass.
struct WeightedHead {
    fact: Fact,
    weight: i64,
}

type WeightedHeads = BTreeMap<FactKey, WeightedHead>;
type Snapshot = BTreeSet<usize>;
type Weights = BTreeMap<usize, i64>;

#[derive(Clone)]
struct WeightedSolution {
    solution: Solution,
    weight: i64,
}

/// Stateful nested-iteration circuit for one fixed rule program.
///
/// `snapshots[i]` is the set-valued recursive relation at inner iteration `i`;
/// `raw[i]` is the weighted, pre-`distinct` output that produced
/// `snapshots[i + 1]`.  The final two snapshots are equal, recording the fixed-point
/// witness.  Facts are interned once in `arena`; snapshots and weights carry dense row
/// indexes only.
#[derive(Debug, Clone)]
pub(crate) struct IncrementalSession {
    identity: IncrementalIdentity,
    /// Immutable across a fixed contract; every branch clone shares the rule plan.
    rules: Arc<[EvalRule]>,
    /// Append-only fact arena. Branch clones share it until a transaction interns a
    /// genuinely new row, then clone-on-write only this arena.
    arena: Arc<FactStore>,
    /// Cached fixed-point state is shared by cheap loop forks; a successful update
    /// installs new Arc roots instead of cloning the base histories up front.
    edb: Arc<Snapshot>,
    snapshots: Arc<Vec<Snapshot>>,
    raw: Arc<Vec<Weights>>,
}

impl IncrementalSession {
    /// Build and fully settle a session from an asserted EDB.
    ///
    /// # Errors
    ///
    /// Returns an error for an uncovered rule fragment, an unsafe/unbound rule, or
    /// integer-weight overflow.  Inputs are set-deduplicated in lexical key order.
    pub(crate) fn new(
        contract_hash: impl Into<String>,
        edb: impl IntoIterator<Item = Fact>,
        rules: &[EvalRule],
    ) -> gmeow_errors::Result<Self> {
        validate_fragment(rules)?;

        let identity = IncrementalIdentity::new(contract_hash, rules);
        let mut arena = FactStore::new();
        let mut keyed_edb: BTreeMap<FactKey, Fact> = BTreeMap::new();
        for fact in edb {
            keyed_edb.entry(fact.key()).or_insert(fact);
        }
        let mut edb_ids = BTreeSet::new();
        for (_key, fact) in keyed_edb {
            let id = arena
                .insert(fact)
                .expect("lexically deduplicated EDB fact must intern exactly once");
            edb_ids.insert(id);
        }

        let mut session = Self {
            identity,
            rules: Arc::from(rules.to_vec()),
            arena: Arc::new(arena),
            edb: Arc::new(edb_ids.clone()),
            snapshots: Arc::new(vec![edb_ids]),
            raw: Arc::new(Vec::new()),
        };
        session.settle_from_scratch()?;
        Ok(session)
    }

    pub(crate) fn identity(&self) -> &IncrementalIdentity {
        &self.identity
    }

    /// The current least-model facts in lexical key order.
    pub(crate) fn closure(&self) -> Vec<Fact> {
        let mut out: Vec<Fact> = self
            .fixed_snapshot()
            .iter()
            .map(|&id| self.arena.facts()[id].clone())
            .collect();
        out.sort_by_key(Fact::key);
        out
    }

    /// Borrow the current fixed-point rows for one predicate in arena insertion
    /// order, without cloning the closure into a second [`FactStore`].
    ///
    /// The incremental non-monotone grounder needs simultaneous old/new relation
    /// views for its telescoping product. Both sessions already share the immutable
    /// arena and carry their own fixed snapshot, so this filtered borrowed iterator
    /// is the zero-copy seam.
    pub(crate) fn closure_facts_for_predicate<'a>(
        &'a self,
        predicate: &'a str,
    ) -> impl Iterator<Item = &'a Fact> + 'a {
        let snapshot = self.fixed_snapshot();
        self.arena
            .facts_for_predicate(predicate)
            .iter()
            .copied()
            .filter(move |id| snapshot.contains(id))
            .map(|id| &self.arena.facts()[id])
    }

    /// Apply one atomic signed EDB transaction and return the signed closure change.
    ///
    /// The transaction is consolidated before application.  At the external set
    /// boundary, each net row must move membership exactly `0 -> 1` or `1 -> 0`;
    /// duplicate insertion and absent-row deletion are hard errors.  Internal raw rule
    /// weights may have arbitrary non-negative multiplicity.
    ///
    /// The fact arena is monotone and may intern a row while computing a transaction
    /// that later errors.  Such an unreferenced arena row is semantically inert: `edb`,
    /// every snapshot, and every raw batch are replaced only after the whole adjusted
    /// fixed point succeeds, so the logical transaction remains atomic.
    pub(crate) fn apply(
        &mut self,
        changes: impl IntoIterator<Item = SignedFact>,
    ) -> gmeow_errors::Result<IncrementalDelta> {
        let result = self.apply_internal(changes, None)?;
        debug_assert_eq!(result.status, BudgetStatus::Ok);
        Ok(result.delta)
    }

    /// Apply an insert-only transaction under the shared committed-derivation
    /// governor. Stable cached facts and asserted inserts are free; each genuinely new
    /// derived fact is charged once, in lexical `FactKey` order at its first inner
    /// iteration. Like the scratch engine, a whole join round may be computed before
    /// the governor cuts at commit, but no later recursive round runs.
    pub(crate) fn apply_insert_budgeted(
        &mut self,
        changes: impl IntoIterator<Item = SignedFact>,
        max_steps: Option<u64>,
    ) -> gmeow_errors::Result<BudgetedIncrementalDelta> {
        self.apply_internal(changes, Some(StepGovernor::new(max_steps)))
    }

    fn apply_internal(
        &mut self,
        changes: impl IntoIterator<Item = SignedFact>,
        mut governor: Option<StepGovernor>,
    ) -> gmeow_errors::Result<BudgetedIncrementalDelta> {
        let consolidated = consolidate_input(changes)?;
        if consolidated.is_empty() {
            return Ok(BudgetedIncrementalDelta {
                delta: IncrementalDelta {
                    changes: Vec::new(),
                    inner_iterations: 0,
                    joined_rows: 0,
                    derivations: BTreeMap::new(),
                },
                closure: self.closure(),
                status: BudgetStatus::Ok,
                consumed_steps: 0,
            });
        }

        if governor.is_some() && consolidated.values().any(|change| change.weight < 0) {
            return Err(incremental_err(
                "a step-governed incremental transaction must be insert-only; bounded \
                 retraction stays on the scratch governor until a sound atomic-delete \
                 frontier is defined",
            ));
        }

        // Validate the entire set transaction before changing any membership.  Facts
        // are then interned in lexical-key order, keeping dense ids deterministic.
        for (key, change) in &consolidated {
            let present = self
                .arena
                .row_index(key)
                .is_some_and(|id| self.edb.contains(&id));
            let next = ZWeightSemiring.add(i64::from(present), change.weight)?;
            if !(0..=1).contains(&next) {
                return Err(incremental_err(format!(
                    "incremental set transaction changes fact {key:?} from membership {} by {} \
                     (result {next}); expected exactly 0 or 1",
                    i64::from(present),
                    change.weight
                )));
            }
        }

        let old_fixed = self.fixed_snapshot().clone();
        let mut next_edb = self.edb.as_ref().clone();
        let mut edb_delta = Weights::new();
        for (key, change) in consolidated {
            let id = match self.arena.row_index(&key) {
                Some(id) => id,
                None => Arc::make_mut(&mut self.arena)
                    .insert(change.fact)
                    .expect("a missing fact key must intern exactly once"),
            };
            add_id_weight(&mut edb_delta, id, change.weight)?;
            match change.weight {
                1 => {
                    next_edb.insert(id);
                }
                -1 => {
                    next_edb.remove(&id);
                }
                other => {
                    return Err(incremental_err(format!(
                        "consolidated set change has weight {other}; expected +1 or -1"
                    )));
                }
            }
        }

        let mut partial_closure = old_fixed.union(&next_edb).copied().collect::<Snapshot>();
        let mut charged = Snapshot::new();
        let mut next_snapshots = vec![next_edb.clone()];
        let mut next_raw = Vec::new();
        let mut joined_rows = 0u64;
        let old_last_snapshot = self.snapshots.len() - 1;
        let old_last_raw = self.raw.len() - 1;

        for iteration in 0usize.. {
            let old_snapshot = self.snapshots[iteration.min(old_last_snapshot)].clone();
            let old_raw = self.raw[iteration.min(old_last_raw)].clone();
            let new_snapshot = next_snapshots[iteration].clone();
            let snapshot_delta = diff_snapshots(&old_snapshot, &new_snapshot)?;

            let weighted_delta = self.differentiate_operator(
                &old_snapshot,
                &new_snapshot,
                &snapshot_delta,
                &edb_delta,
                &mut joined_rows,
            )?;
            let raw_delta = self.intern_weighted(weighted_delta)?;
            let mut adjusted_raw = old_raw;
            for (id, weight) in raw_delta {
                add_id_weight(&mut adjusted_raw, id, weight)?;
            }
            if let Some((&id, &weight)) = adjusted_raw.iter().find(|(_, weight)| **weight < 0) {
                return Err(incremental_err(format!(
                    "differential circuit produced negative settled multiplicity {weight} for {:?}",
                    self.arena.facts()[id].key()
                )));
            }

            let next_snapshot = distinct(&adjusted_raw);

            if let Some(governor) = governor.as_mut() {
                let mut candidates: Vec<usize> = next_snapshot
                    .difference(&old_fixed)
                    .filter(|id| !next_edb.contains(id) && !charged.contains(id))
                    .copied()
                    .collect();
                candidates.sort_by_key(|id| self.arena.facts()[*id].key());
                for id in candidates {
                    if governor.spent() {
                        let derivations =
                            self.complete_derivations(&old_fixed, &partial_closure, &next_edb)?;
                        let delta = self.delta_between(
                            &old_fixed,
                            &partial_closure,
                            iteration + 1,
                            joined_rows,
                            &derivations,
                        )?;
                        return Ok(BudgetedIncrementalDelta {
                            delta,
                            closure: self.facts_in(&partial_closure),
                            status: BudgetStatus::Exhausted,
                            consumed_steps: governor.consumed,
                        });
                    }
                    charged.insert(id);
                    partial_closure.insert(id);
                    governor.charge();
                }
            }
            let fixed = next_snapshot == new_snapshot;
            next_raw.push(adjusted_raw);
            next_snapshots.push(next_snapshot);

            // Once the old history is at its fixed row, the new stream has no unseen
            // old columns left to adjust.  Equality of the last two new snapshots is
            // therefore the new least-fixed-point witness.
            if iteration >= old_last_raw && fixed {
                break;
            }
        }

        let new_fixed = next_snapshots
            .last()
            .expect("a settled history always has a final snapshot")
            .clone();
        let inner_iterations = next_raw.len();
        let derivations = self.complete_derivations(&old_fixed, &new_fixed, &next_edb)?;
        let delta = self.delta_between(
            &old_fixed,
            &new_fixed,
            inner_iterations,
            joined_rows,
            &derivations,
        )?;
        let closure = self.facts_in(&new_fixed);
        let consumed_steps = governor.as_ref().map_or(0, |governor| governor.consumed);
        self.edb = Arc::new(next_edb);
        self.snapshots = Arc::new(next_snapshots);
        self.raw = Arc::new(next_raw);

        Ok(BudgetedIncrementalDelta {
            delta,
            closure,
            status: BudgetStatus::Ok,
            consumed_steps,
        })
    }

    fn facts_in(&self, snapshot: &Snapshot) -> Vec<Fact> {
        let mut facts: Vec<Fact> = snapshot
            .iter()
            .map(|&id| self.arena.facts()[id].clone())
            .collect();
        facts.sort_by_key(Fact::key);
        facts
    }

    fn delta_between(
        &self,
        old: &Snapshot,
        new: &Snapshot,
        inner_iterations: usize,
        joined_rows: u64,
        derivations: &BTreeMap<usize, IncrementalDerivation>,
    ) -> gmeow_errors::Result<IncrementalDelta> {
        let mut output_ids: Vec<(usize, i64)> = diff_snapshots(old, new)?.into_iter().collect();
        output_ids.sort_by_key(|(id, _)| self.arena.facts()[*id].key());
        let changes = output_ids
            .into_iter()
            .map(|(id, weight)| SignedFact {
                fact: self.arena.facts()[id].clone(),
                weight,
            })
            .collect();
        let derivations = derivations
            .iter()
            .filter(|(id, _)| new.contains(id) && !old.contains(id))
            .map(|(&id, witness)| (self.arena.facts()[id].key(), witness.clone()))
            .collect();
        Ok(IncrementalDelta {
            changes,
            inner_iterations,
            joined_rows,
            derivations,
        })
    }

    fn fixed_snapshot(&self) -> &Snapshot {
        self.snapshots
            .last()
            .expect("a session always carries its fixed snapshot")
    }

    /// One canonical proof witness — firing rule, premises, AND minimal proof height —
    /// for every DERIVED fact newly present in `new` versus `old`. Every witness is
    /// reconstructed from a min-height fixpoint over the settled `new` snapshot, so the
    /// choice and the height are identical to a from-scratch recompute (the transient
    /// differential pass carries no proof-height annotation and is not consulted).
    fn complete_derivations(
        &self,
        old: &Snapshot,
        new: &Snapshot,
        edb: &Snapshot,
    ) -> gmeow_errors::Result<BTreeMap<usize, IncrementalDerivation>> {
        let targets: BTreeSet<usize> = new
            .difference(old)
            .filter(|id| !edb.contains(id))
            .copied()
            .collect();
        if targets.is_empty() {
            return Ok(BTreeMap::new());
        }
        let derivations = self.reconstruct_derivations(new, edb, &targets)?;
        if derivations.len() != targets.len() {
            let witnessed: BTreeSet<usize> = derivations.keys().copied().collect();
            let missing_keys: Vec<FactKey> = targets
                .difference(&witnessed)
                .map(|&id| self.arena.facts()[id].key())
                .collect();
            return Err(incremental_err(format!(
                "settled incremental facts have no surviving proof witnesses: {missing_keys:?}"
            )));
        }
        Ok(derivations)
    }

    /// Minimal proof height of every fact in `snapshot` over the tropical
    /// `(min, max)` [`MinProofHeightSemiring`].
    ///
    /// The min proof height is NOT delta-maintainable through the signed Z-set circuit
    /// (deleting a fact can RAISE a dependent's height and the tropical semiring has no
    /// additive inverse), so it is recomputed by a monotone relaxation fixpoint over the
    /// already-SETTLED snapshot: every EDB fact is seeded at [`ProofHeight::ASSERTED`],
    /// and each derived fact relaxes to the minimum `1 + max(premise heights)` over every
    /// rule firing whose premises already have a finite height. Heights only ever
    /// decrease and are bounded below, so the relaxation converges. This is the exact
    /// recurrence the full forward reasoner carries in lockstep, over the identical least
    /// model, so the two agree fact-for-fact under both insertion and deletion.
    fn settled_heights(
        &self,
        snapshot: &Snapshot,
        edb: &Snapshot,
    ) -> gmeow_errors::Result<BTreeMap<usize, ProofHeight>> {
        let mut heights: BTreeMap<usize, ProofHeight> = BTreeMap::new();
        for &id in edb.intersection(snapshot) {
            heights.insert(id, ProofHeight::ASSERTED);
        }
        loop {
            let mut changed = false;
            for rule in self.rules.iter() {
                let mut partial = vec![WeightedSolution {
                    solution: Solution {
                        bindings: Vec::new(),
                        source_facts: Vec::new(),
                    },
                    weight: 1,
                }];
                for atom in &rule.body {
                    partial = self.extend_from_snapshot(atom, snapshot, &partial, None)?;
                    if partial.is_empty() {
                        break;
                    }
                }
                for weighted in partial {
                    if !distinct_pairs_satisfied(&rule.distinct_pairs, &weighted.solution)? {
                        continue;
                    }
                    let head = ground_head(&rule.head, &weighted.solution)?;
                    let Some(id) = self.arena.row_index(&head.key()) else {
                        continue;
                    };
                    if !snapshot.contains(&id) {
                        continue;
                    }
                    let Some(max_premise) =
                        self.max_premise_height(&weighted.solution.source_facts, &heights)
                    else {
                        continue;
                    };
                    let candidate = MinProofHeightSemiring.derive([max_premise])?;
                    match heights.entry(id) {
                        std::collections::btree_map::Entry::Vacant(slot) => {
                            slot.insert(candidate);
                            changed = true;
                        }
                        std::collections::btree_map::Entry::Occupied(mut slot) => {
                            if candidate < *slot.get() {
                                slot.insert(candidate);
                                changed = true;
                            }
                        }
                    }
                }
            }
            if !changed {
                break;
            }
        }
        Ok(heights)
    }

    /// `max` over the settled heights of `premises`, or `None` if any premise has not
    /// yet acquired a finite height in the current relaxation round.
    fn max_premise_height(
        &self,
        premises: &[Fact],
        heights: &BTreeMap<usize, ProofHeight>,
    ) -> Option<ProofHeight> {
        let mut max_height = ProofHeight::ASSERTED;
        for premise in premises {
            let id = self.arena.row_index(&premise.key())?;
            let height = heights.get(&id).copied()?;
            max_height = max_height.max(height);
        }
        Some(max_height)
    }

    /// Re-descend the settled snapshot and select, for each target, the single canonical
    /// proof witness AND its minimal proof height.
    ///
    /// Witness selection mirrors the full reasoner's total-order tiebreak
    /// (`crate::rule_ir::RuleRoundCandidate::tiebreak_key`):
    /// `(proof_height, sum_src_depth, sorted_source_reifiers, rule_iri, source_reifiers)`,
    /// smaller wins. Because `proof_height` is the leading key, the selected witness is
    /// always a minimal-height derivation, so its stamped height equals the fact's
    /// [`Self::settled_heights`] value — identical, field-for-field, to the from-scratch
    /// oracle's choice.
    fn reconstruct_derivations(
        &self,
        snapshot: &Snapshot,
        edb: &Snapshot,
        targets: &BTreeSet<usize>,
    ) -> gmeow_errors::Result<BTreeMap<usize, IncrementalDerivation>> {
        let heights = self.settled_heights(snapshot, edb)?;
        let target_ids: BTreeMap<FactKey, usize> = targets
            .iter()
            .map(|&id| (self.arena.facts()[id].key(), id))
            .collect();
        // Per target: the winning tiebreak key alongside its witness.
        let mut best: BTreeMap<usize, (WitnessTiebreak, IncrementalDerivation)> = BTreeMap::new();

        for rule in self.rules.iter() {
            let mut partial = vec![WeightedSolution {
                solution: Solution {
                    bindings: Vec::new(),
                    source_facts: Vec::new(),
                },
                weight: 1,
            }];
            for atom in &rule.body {
                partial = self.extend_from_snapshot(atom, snapshot, &partial, None)?;
                if partial.is_empty() {
                    break;
                }
            }
            for weighted in partial {
                if !distinct_pairs_satisfied(&rule.distinct_pairs, &weighted.solution)? {
                    continue;
                }
                let head = ground_head(&rule.head, &weighted.solution)?;
                let Some(&id) = target_ids.get(&head.key()) else {
                    continue;
                };
                let sources = &weighted.solution.source_facts;
                let Some(max_premise) = self.max_premise_height(sources, &heights) else {
                    continue;
                };
                let proof_height = MinProofHeightSemiring.derive([max_premise])?;
                let mut source_reifiers = Vec::with_capacity(sources.len());
                let mut source_height_sum = 0_u64;
                for source in sources {
                    source_reifiers.push(source.reifier()?);
                    let id = self.arena.row_index(&source.key());
                    let height = id
                        .and_then(|id| heights.get(&id).copied())
                        .unwrap_or(ProofHeight::ASSERTED);
                    source_height_sum = source_height_sum.saturating_add(u64::from(height.get()));
                }
                let mut sorted_reifiers = source_reifiers.clone();
                sorted_reifiers.sort();
                let tiebreak = WitnessTiebreak {
                    proof_height,
                    source_height_sum,
                    sorted_reifiers,
                    rule_iri: rule.rule_iri.clone(),
                    source_reifiers,
                };
                let candidate = IncrementalDerivation {
                    rule_iri: rule.rule_iri.clone(),
                    premises: weighted.solution.source_facts,
                    proof_height,
                };
                match best.entry(id) {
                    std::collections::btree_map::Entry::Vacant(slot) => {
                        slot.insert((tiebreak, candidate));
                    }
                    std::collections::btree_map::Entry::Occupied(mut slot) => {
                        if tiebreak < slot.get().0 {
                            slot.insert((tiebreak, candidate));
                        }
                    }
                }
            }
        }
        Ok(best
            .into_iter()
            .map(|(id, (_, witness))| (id, witness))
            .collect())
    }

    /// One canonical proof witness for every DERIVED (non-EDB) fact currently in the
    /// closure — the "why is this fact here?" provenance over the FULL maintained
    /// closure, not just the last transaction's newly-derived facts.
    ///
    /// This is a clean reuse of [`Self::reconstruct_derivations`] (the same re-descent
    /// the incremental `apply` path uses to complete every transaction's witnesses): it
    /// re-descends the settled fixed point once over the set of derived facts and returns
    /// one canonical `(fact, witness)` per fact — carrying the minimal proof height — in
    /// lexical [`FactKey`] order. The settle circuit itself is untouched.
    ///
    /// # Errors
    ///
    /// Propagates a rule-evaluation failure from the re-descent.
    pub(crate) fn closure_derivations(
        &self,
    ) -> gmeow_errors::Result<Vec<(Fact, IncrementalDerivation)>> {
        let fixed = self.fixed_snapshot();
        let derived: BTreeSet<usize> = fixed.difference(self.edb.as_ref()).copied().collect();
        if derived.is_empty() {
            return Ok(Vec::new());
        }
        let by_id = self.reconstruct_derivations(fixed, self.edb.as_ref(), &derived)?;
        let mut out: Vec<(Fact, IncrementalDerivation)> = by_id
            .into_iter()
            .map(|(id, witness)| (self.arena.facts()[id].clone(), witness))
            .collect();
        out.sort_by_key(|(fact, _)| fact.key());
        Ok(out)
    }

    fn settle_from_scratch(&mut self) -> gmeow_errors::Result<()> {
        loop {
            let snapshot = self
                .snapshots
                .last()
                .expect("the EDB is the first snapshot")
                .clone();
            let weighted = self.evaluate_operator(&snapshot)?;
            let raw = self.intern_weighted(weighted)?;
            let next = distinct(&raw);
            let fixed = next == snapshot;
            Arc::make_mut(&mut self.raw).push(raw);
            Arc::make_mut(&mut self.snapshots).push(next);
            if fixed {
                return Ok(());
            }
        }
    }

    /// Evaluate `EDB + sum(rule(snapshot))` with full non-negative weights.
    fn evaluate_operator(&self, snapshot: &Snapshot) -> gmeow_errors::Result<WeightedHeads> {
        let mut output = WeightedHeads::new();
        for &id in self.edb.iter() {
            add_head(&mut output, self.arena.facts()[id].clone(), 1)?;
        }
        for rule in self.rules.iter() {
            let mut partial = vec![WeightedSolution {
                solution: Solution {
                    bindings: Vec::new(),
                    source_facts: Vec::new(),
                },
                weight: 1,
            }];
            for atom in &rule.body {
                partial = self.extend_from_snapshot(atom, snapshot, &partial, None)?;
                if partial.is_empty() {
                    break;
                }
            }
            emit_heads(rule, partial, &mut output)?;
        }
        Ok(output)
    }

    /// Differentiate the whole non-recursive rule operator at one inner iteration.
    fn differentiate_operator(
        &self,
        old: &Snapshot,
        new: &Snapshot,
        delta: &Weights,
        edb_delta: &Weights,
        joined_rows: &mut u64,
    ) -> gmeow_errors::Result<WeightedHeads> {
        let mut output = WeightedHeads::new();
        for (&id, &weight) in edb_delta {
            add_head(&mut output, self.arena.facts()[id].clone(), weight)?;
        }

        for rule in self.rules.iter() {
            // Product(new) - product(old), expanded exactly once per selected delta
            // position: new inputs before p, delta at p, old inputs after p.
            for delta_position in 0..rule.body.len() {
                let mut partial = vec![WeightedSolution {
                    solution: Solution {
                        bindings: Vec::new(),
                        source_facts: Vec::new(),
                    },
                    weight: 1,
                }];
                for (position, atom) in rule.body.iter().enumerate() {
                    let (snapshot, signed) = if position < delta_position {
                        (new, None)
                    } else if position == delta_position {
                        (new, Some(delta))
                    } else {
                        (old, None)
                    };
                    partial = self.extend_from_snapshot(
                        atom,
                        snapshot,
                        &partial,
                        signed.map(|weights| (weights, &mut *joined_rows)),
                    )?;
                    if partial.is_empty() {
                        break;
                    }
                }
                emit_heads(rule, partial, &mut output)?;
            }
        }
        Ok(output)
    }

    /// Extend weighted solutions by one atom.  `signed = Some(delta)` selects only
    /// rows in the signed delta; otherwise `snapshot` supplies unit-weight rows.
    fn extend_from_snapshot(
        &self,
        atom: &crate::rule_ir::EvalAtom,
        snapshot: &Snapshot,
        partial: &[WeightedSolution],
        mut signed: Option<(&Weights, &mut u64)>,
    ) -> gmeow_errors::Result<Vec<WeightedSolution>> {
        let mut out = Vec::new();
        for base in partial {
            for &id in self.arena.facts_for_predicate(&atom.predicate) {
                let row_weight = match &mut signed {
                    Some((weights, visited)) => match weights.get(&id) {
                        Some(&weight) => {
                            **visited = visited.checked_add(1).ok_or_else(|| {
                                incremental_err("incremental joined-row counter overflow")
                            })?;
                            weight
                        }
                        None => continue,
                    },
                    None => {
                        if !snapshot.contains(&id) {
                            continue;
                        }
                        1
                    }
                };
                let fact = &self.arena.facts()[id];
                if let Some(mut solution) = match_atom(atom, fact, &base.solution) {
                    solution.source_facts.push(fact.clone());
                    let weight = ZWeightSemiring.multiply(base.weight, row_weight)?;
                    if weight != 0 {
                        out.push(WeightedSolution { solution, weight });
                    }
                }
            }
        }
        Ok(out)
    }

    /// Intern lexically-ordered weighted heads and return dense-id weights.
    fn intern_weighted(&mut self, weighted: WeightedHeads) -> gmeow_errors::Result<Weights> {
        let mut weights = Weights::new();
        for (key, head) in weighted {
            if head.weight == 0 {
                continue;
            }
            let id = match self.arena.row_index(&key) {
                Some(id) => id,
                None => Arc::make_mut(&mut self.arena)
                    .insert(head.fact)
                    .expect("a missing weighted head must intern exactly once"),
            };
            add_id_weight(&mut weights, id, head.weight)?;
        }
        Ok(weights)
    }
}

/// The typed reason one rule falls outside finite positive binary Datalog — the
/// fragment the incremental circuit maintains.
///
/// Each variant is the exact condition [`validate_fragment`] refuses today; keeping it
/// typed (rather than only a [`gmeow_errors::Diag`] string) lets the operational
/// `ReasoningSession` façade classify the FIXED program ONCE at `open` and route every
/// later `apply` to a typed `UnsupportedFragment` outcome, without string-matching a
/// diagnostic. `validate_fragment` is the single behavioural source of truth: it calls
/// [`classify_incremental_fragment`] and maps each reason back to the same Diag string
/// it has always emitted, so there is no second copy of the checks to drift.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum UnsupportedFragmentReason {
    /// A rule with an empty body (an asserted fact masquerading as a rule).
    Bodyless,
    /// A negation-as-failure body atom.
    Negation,
    /// An arithmetic / comparison builtin.
    Builtins,
    /// A head variable not bound by any positive body atom.
    UnsafeHeadVar,
    /// An inequality (`distinct`) variable not bound by any positive body atom.
    UnsafeInequalityVar,
}

/// A typed fragment refusal: the offending rule IRI plus the [`UnsupportedFragmentReason`].
///
/// The IRI is retained so [`validate_fragment`] can reproduce its historical
/// per-rule Diag message byte-for-byte while the façade consumes only the typed
/// [`Self::reason`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FragmentRefusal {
    pub(crate) rule_iri: String,
    pub(crate) reason: UnsupportedFragmentReason,
    /// The specific offending variable name for the unsafe-variable reasons; `None`
    /// for the reasons that name no variable. Carried so [`Self::message`] reproduces
    /// the historical per-variable Diag text without re-scanning the rule.
    offending_var: Option<String>,
}

impl FragmentRefusal {
    /// The exact diagnostic message [`validate_fragment`] emitted before the typed
    /// classifier was factored out — the single-source-of-truth text, keyed by reason.
    fn message(&self) -> String {
        match &self.reason {
            UnsupportedFragmentReason::Bodyless => format!(
                "incremental circuit does not admit a bodyless rule <{}>; asserted facts belong in the EDB",
                self.rule_iri
            ),
            UnsupportedFragmentReason::Negation => format!(
                "incremental circuit does not admit negation-as-failure in rule <{}>",
                self.rule_iri
            ),
            UnsupportedFragmentReason::Builtins => format!(
                "incremental circuit does not admit arithmetic/comparison builtins in rule <{}>",
                self.rule_iri
            ),
            UnsupportedFragmentReason::UnsafeHeadVar => format!(
                "incremental circuit rule <{}> is unsafe: head variable {var} is not bound by a positive body atom",
                self.rule_iri,
                var = self.unsafe_head_var().unwrap_or_default(),
            ),
            UnsupportedFragmentReason::UnsafeInequalityVar => format!(
                "incremental circuit rule <{}> is unsafe: inequality variable {var} is not bound by a positive body atom",
                self.rule_iri,
                var = self.unsafe_inequality_var().unwrap_or_default(),
            ),
        }
    }

    /// Re-locate the specific unbound head variable name for message parity. The
    /// classifier records only the reason + rule IRI (the façade needs no variable
    /// name), so the exact offending variable is recovered here from the same rule.
    fn unsafe_head_var(&self) -> Option<String> {
        self.offending_var.clone()
    }

    fn unsafe_inequality_var(&self) -> Option<String> {
        self.offending_var.clone()
    }
}

/// Classify a rule set against the incremental circuit's admissible fragment
/// (finite positive binary Datalog), returning the FIRST typed refusal.
///
/// This performs exactly the checks [`validate_fragment`] performed inline, in the
/// same order, so the two can never disagree: `validate_fragment` is now a thin
/// diagnostic-projection over this function.
pub(crate) fn classify_incremental_fragment(rules: &[EvalRule]) -> Result<(), FragmentRefusal> {
    for rule in rules {
        if rule.body.is_empty() {
            return Err(FragmentRefusal {
                rule_iri: rule.rule_iri.clone(),
                reason: UnsupportedFragmentReason::Bodyless,
                offending_var: None,
            });
        }
        if rule.body.iter().any(|atom| atom.negated) {
            return Err(FragmentRefusal {
                rule_iri: rule.rule_iri.clone(),
                reason: UnsupportedFragmentReason::Negation,
                offending_var: None,
            });
        }
        if !rule.builtins.is_empty() {
            return Err(FragmentRefusal {
                rule_iri: rule.rule_iri.clone(),
                reason: UnsupportedFragmentReason::Builtins,
                offending_var: None,
            });
        }

        let mut positively_bound = BTreeSet::new();
        for atom in &rule.body {
            for term in [&atom.subject, &atom.object] {
                if let EvalTerm::Var(variable) = term {
                    positively_bound.insert(variable.as_str());
                }
            }
        }
        for term in [&rule.head.subject, &rule.head.object] {
            if let EvalTerm::Var(variable) = term
                && !positively_bound.contains(variable.as_str())
            {
                return Err(FragmentRefusal {
                    rule_iri: rule.rule_iri.clone(),
                    reason: UnsupportedFragmentReason::UnsafeHeadVar,
                    offending_var: Some(variable.clone()),
                });
            }
        }
        for (left, right) in &rule.distinct_pairs {
            for variable in [left, right] {
                if !positively_bound.contains(variable.as_str()) {
                    return Err(FragmentRefusal {
                        rule_iri: rule.rule_iri.clone(),
                        reason: UnsupportedFragmentReason::UnsafeInequalityVar,
                        offending_var: Some(variable.clone()),
                    });
                }
            }
        }
    }
    Ok(())
}

fn validate_fragment(rules: &[EvalRule]) -> gmeow_errors::Result<()> {
    match classify_incremental_fragment(rules) {
        Ok(()) => Ok(()),
        Err(refusal) => Err(incremental_err(refusal.message())),
    }
}

fn consolidate_input(
    changes: impl IntoIterator<Item = SignedFact>,
) -> gmeow_errors::Result<WeightedHeads> {
    let mut out = WeightedHeads::new();
    for change in changes {
        add_head(&mut out, change.fact, change.weight)?;
    }
    out.retain(|_, head| head.weight != 0);
    Ok(out)
}

fn emit_heads(
    rule: &EvalRule,
    solutions: Vec<WeightedSolution>,
    output: &mut WeightedHeads,
) -> gmeow_errors::Result<()> {
    for weighted in solutions {
        if !distinct_pairs_satisfied(&rule.distinct_pairs, &weighted.solution)? {
            continue;
        }
        let head = ground_head(&rule.head, &weighted.solution)?;
        add_head(output, head, weighted.weight)?;
    }
    Ok(())
}

fn add_head(output: &mut WeightedHeads, fact: Fact, weight: i64) -> gmeow_errors::Result<()> {
    if weight == 0 {
        return Ok(());
    }
    let key = fact.key();
    match output.entry(key) {
        std::collections::btree_map::Entry::Vacant(slot) => {
            slot.insert(WeightedHead { fact, weight });
        }
        std::collections::btree_map::Entry::Occupied(mut slot) => {
            let combined = ZWeightSemiring.add(slot.get().weight, weight)?;
            if combined == 0 {
                slot.remove();
            } else {
                slot.get_mut().weight = combined;
            }
        }
    }
    Ok(())
}

fn add_id_weight(output: &mut Weights, id: usize, weight: i64) -> gmeow_errors::Result<()> {
    if weight == 0 {
        return Ok(());
    }
    match output.entry(id) {
        std::collections::btree_map::Entry::Vacant(slot) => {
            slot.insert(weight);
        }
        std::collections::btree_map::Entry::Occupied(mut slot) => {
            let combined = ZWeightSemiring.add(*slot.get(), weight)?;
            if combined == 0 {
                slot.remove();
            } else {
                *slot.get_mut() = combined;
            }
        }
    }
    Ok(())
}

fn distinct(raw: &Weights) -> Snapshot {
    raw.iter()
        .filter_map(|(&id, &weight)| (weight > 0).then_some(id))
        .collect()
}

fn diff_snapshots(old: &Snapshot, new: &Snapshot) -> gmeow_errors::Result<Weights> {
    let mut delta = Weights::new();
    for &id in new.difference(old) {
        delta.insert(id, ZWeightSemiring.one());
    }
    let retraction = ZWeightSemiring.negate(ZWeightSemiring.one())?;
    for &id in old.difference(new) {
        delta.insert(id, retraction);
    }
    Ok(delta)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::query_ir::{ArithOp, QBuiltin, QTerm};
    use crate::rule_ir::{EvalAtom, EvalTerm, least_model_of_reduct};
    use purrdf::TermValue;

    const NS: &str = "https://example.org/incremental/";

    fn iri(local: &str) -> TermValue {
        TermValue::iri(format!("{NS}{local}"))
    }

    fn fact(predicate: &str, subject: &str, object: &str) -> Fact {
        Fact {
            subject: iri(subject),
            predicate: format!("{NS}{predicate}"),
            object: iri(object),
        }
    }

    fn var(name: &str) -> EvalTerm {
        EvalTerm::Var(format!("?{name}"))
    }

    fn constant(local: &str) -> EvalTerm {
        EvalTerm::ConstNamed(format!("{NS}{local}"))
    }

    fn atom(predicate: &str, subject: EvalTerm, object: EvalTerm) -> EvalAtom {
        EvalAtom {
            subject,
            predicate: format!("{NS}{predicate}"),
            object,
            negated: false,
        }
    }

    fn rule(name: &str, head: EvalAtom, body: Vec<EvalAtom>) -> EvalRule {
        EvalRule {
            head,
            body,
            rule_iri: format!("{NS}rule/{name}"),
            distinct_pairs: Vec::new(),
            builtins: Vec::new(),
            constraint_tag: None,
        }
    }

    fn closure_keys(facts: &[Fact]) -> BTreeSet<FactKey> {
        facts.iter().map(Fact::key).collect()
    }

    fn scratch(edb: &[Fact], rules: &[EvalRule]) -> BTreeSet<FactKey> {
        let mut edb_store = FactStore::new();
        let mut sorted = edb.to_vec();
        sorted.sort_by_key(Fact::key);
        for fact in sorted {
            edb_store.insert(fact);
        }
        let result = least_model_of_reduct(&edb_store, rules, &edb_store)
            .expect("positive scratch materialization");
        closure_keys(result.store.facts())
    }

    fn assert_scratch_parity(session: &IncrementalSession, edb: &[Fact], rules: &[EvalRule]) {
        assert_eq!(closure_keys(&session.closure()), scratch(edb, rules));
    }

    fn transitive_rules() -> Vec<EvalRule> {
        vec![
            rule(
                "base",
                atom("path", var("X"), var("Y")),
                vec![atom("edge", var("X"), var("Y"))],
            ),
            rule(
                "step",
                atom("path", var("X"), var("Z")),
                vec![
                    atom("path", var("X"), var("Y")),
                    atom("edge", var("Y"), var("Z")),
                ],
            ),
        ]
    }

    #[test]
    fn recursive_insert_and_retract_match_clean_rebuild() {
        let rules = transitive_rules();
        let mut edb = vec![
            fact("edge", "a", "b"),
            fact("edge", "b", "c"),
            fact("edge", "c", "d"),
        ];
        let mut session = IncrementalSession::new("contract", edb.clone(), &rules).unwrap();
        assert_scratch_parity(&session, &edb, &rules);

        let inserted = fact("edge", "d", "e");
        let delta = session
            .apply([SignedFact {
                fact: inserted.clone(),
                weight: 1,
            }])
            .unwrap();
        edb.push(inserted);
        assert_scratch_parity(&session, &edb, &rules);
        assert!(delta.changes.iter().any(|change| {
            change.weight == 1 && change.fact.key() == fact("path", "a", "e").key()
        }));
        assert!(delta.inner_iterations > 0);
        assert!(delta.joined_rows > 0);

        let removed = fact("edge", "b", "c");
        let delta = session
            .apply([SignedFact {
                fact: removed.clone(),
                weight: -1,
            }])
            .unwrap();
        edb.retain(|candidate| candidate.key() != removed.key());
        assert_scratch_parity(&session, &edb, &rules);
        assert!(delta.changes.iter().any(|change| {
            change.weight == -1 && change.fact.key() == fact("path", "a", "d").key()
        }));
    }

    #[test]
    fn loop_forks_share_cached_histories_until_the_branch_updates() {
        let rules = transitive_rules();
        let base = fact("edge", "a", "b");
        let session = IncrementalSession::new("contract", [base], &rules).unwrap();
        let mut branch = session.clone();

        assert!(Arc::ptr_eq(&session.rules, &branch.rules));
        assert!(Arc::ptr_eq(&session.arena, &branch.arena));
        assert!(Arc::ptr_eq(&session.edb, &branch.edb));
        assert!(Arc::ptr_eq(&session.snapshots, &branch.snapshots));
        assert!(Arc::ptr_eq(&session.raw, &branch.raw));

        branch
            .apply([SignedFact {
                fact: fact("edge", "b", "c"),
                weight: 1,
            }])
            .unwrap();

        assert!(Arc::ptr_eq(&session.rules, &branch.rules));
        assert!(!Arc::ptr_eq(&session.arena, &branch.arena));
        assert!(!Arc::ptr_eq(&session.edb, &branch.edb));
        assert!(!Arc::ptr_eq(&session.snapshots, &branch.snapshots));
        assert!(!Arc::ptr_eq(&session.raw, &branch.raw));
        assert_eq!(
            closure_keys(&session.closure()),
            scratch(&[fact("edge", "a", "b")], &rules)
        );
        assert_eq!(
            closure_keys(&branch.closure()),
            scratch(&[fact("edge", "a", "b"), fact("edge", "b", "c")], &rules)
        );
    }

    #[test]
    fn retracting_last_ground_removes_a_mutual_support_cycle() {
        let rules = vec![
            rule(
                "seed-q",
                atom("q", var("X"), var("Y")),
                vec![atom("seed", var("X"), var("Y"))],
            ),
            rule(
                "q-p",
                atom("p", var("X"), var("Y")),
                vec![atom("q", var("X"), var("Y"))],
            ),
            rule(
                "p-q",
                atom("q", var("X"), var("Y")),
                vec![atom("p", var("X"), var("Y"))],
            ),
        ];
        let seed = fact("seed", "a", "b");
        let mut session = IncrementalSession::new("contract", [seed.clone()], &rules).unwrap();
        assert!(closure_keys(&session.closure()).contains(&fact("p", "a", "b").key()));

        let delta = session
            .apply([SignedFact {
                fact: seed,
                weight: -1,
            }])
            .unwrap();
        assert_scratch_parity(&session, &[], &rules);
        let removed: BTreeSet<FactKey> = delta
            .changes
            .iter()
            .filter(|change| change.weight == -1)
            .map(|change| change.fact.key())
            .collect();
        assert!(removed.contains(&fact("p", "a", "b").key()));
        assert!(removed.contains(&fact("q", "a", "b").key()));
    }

    #[test]
    fn alternative_proof_survives_one_retraction() {
        let rules = vec![
            rule(
                "left",
                atom("answer", var("X"), var("Y")),
                vec![atom("left", var("X"), var("Y"))],
            ),
            rule(
                "right",
                atom("answer", var("X"), var("Y")),
                vec![atom("right", var("X"), var("Y"))],
            ),
        ];
        let left = fact("left", "a", "b");
        let right = fact("right", "a", "b");
        let mut session =
            IncrementalSession::new("contract", [left.clone(), right.clone()], &rules).unwrap();
        let delta = session
            .apply([SignedFact {
                fact: left,
                weight: -1,
            }])
            .unwrap();
        assert_scratch_parity(&session, &[right], &rules);
        assert!(
            !delta
                .changes
                .iter()
                .any(|change| { change.fact.key() == fact("answer", "a", "b").key() })
        );
    }

    #[test]
    fn newly_derived_fact_selects_a_witness_that_survives_signed_cancellation() {
        let rules = vec![
            rule(
                "a-left",
                atom("answer", var("X"), var("Y")),
                vec![
                    atom("left", var("X"), var("Y")),
                    atom("gate", var("X"), var("Y")),
                ],
            ),
            rule(
                "b-right",
                atom("answer", var("X"), var("Y")),
                vec![
                    atom("right", var("X"), var("Y")),
                    atom("gate", var("X"), var("Y")),
                ],
            ),
        ];
        let left = fact("left", "a", "b");
        let right = fact("right", "a", "b");
        let gate = fact("gate", "a", "b");
        let answer = fact("answer", "a", "b");
        let mut session = IncrementalSession::new("contract", [left.clone()], &rules).unwrap();

        // The left rule contributes +answer when gate arrives and -answer when its
        // old support retracts; the right rule contributes the surviving +answer.
        let delta = session
            .apply([
                SignedFact {
                    fact: gate.clone(),
                    weight: 1,
                },
                SignedFact {
                    fact: right.clone(),
                    weight: 1,
                },
                SignedFact {
                    fact: left.clone(),
                    weight: -1,
                },
            ])
            .unwrap();

        let witness = delta
            .derivations
            .get(&answer.key())
            .expect("new answer carries a surviving proof witness");
        let premise_keys: BTreeSet<_> = witness.premises.iter().map(Fact::key).collect();
        assert_eq!(witness.rule_iri, format!("{NS}rule/b-right"));
        assert!(premise_keys.contains(&right.key()));
        assert!(premise_keys.contains(&gate.key()));
        assert!(!premise_keys.contains(&left.key()));
        assert_scratch_parity(&session, &[right, gate], &rules);
    }

    #[test]
    fn closure_reconstruct_selects_the_minimal_height_witness() {
        let rules = transitive_rules();
        // a→b, b→c, and a direct a→c edge: path(a,c) has a height-1 (direct `base`) and a
        // height-2 (via b, `step`) derivation. The reconstructed canonical witness is the
        // minimal-height one, and every derived fact appears exactly once.
        let edb = vec![
            fact("edge", "a", "b"),
            fact("edge", "b", "c"),
            fact("edge", "a", "c"),
        ];
        let session = IncrementalSession::new("contract", edb, &rules).unwrap();
        let derivations = session.closure_derivations().unwrap();

        // path(a,b), path(b,c), path(a,c) — one canonical witness each.
        assert_eq!(derivations.len(), 3);

        let witness = |s: &str, o: &str| {
            derivations
                .iter()
                .find(|(f, _)| f.key() == fact("path", s, o).key())
                .map(|(_, w)| w.clone())
                .unwrap_or_else(|| panic!("path {s} {o} is derived"))
        };

        let ac = witness("a", "c");
        assert_eq!(ac.proof_height.get(), 1, "the minimal proof height wins");
        assert_eq!(ac.rule_iri, format!("{NS}rule/base"));
        assert_eq!(ac.premises.len(), 1);
        assert_eq!(ac.premises[0].key(), fact("edge", "a", "c").key());

        assert_eq!(witness("a", "b").proof_height.get(), 1);
        assert_eq!(witness("b", "c").proof_height.get(), 1);
    }

    #[test]
    fn reconstructed_height_rises_when_a_short_proof_is_retracted() {
        let rules = transitive_rules();
        // path(a,c) starts with a height-1 direct proof; retracting edge(a,c) leaves only
        // the height-2 path via b, so the maintained proof height must RISE from 1 to 2.
        let mut session = IncrementalSession::new(
            "contract",
            vec![
                fact("edge", "a", "b"),
                fact("edge", "b", "c"),
                fact("edge", "a", "c"),
            ],
            &rules,
        )
        .unwrap();
        let height_ac = |session: &IncrementalSession| {
            session
                .closure_derivations()
                .unwrap()
                .into_iter()
                .find(|(f, _)| f.key() == fact("path", "a", "c").key())
                .map(|(_, w)| w.proof_height.get())
                .expect("path a c is derived")
        };
        assert_eq!(height_ac(&session), 1);

        session
            .apply([SignedFact {
                fact: fact("edge", "a", "c"),
                weight: -1,
            }])
            .unwrap();
        assert_eq!(
            height_ac(&session),
            2,
            "the surviving proof is one hop longer"
        );
    }

    #[test]
    fn constants_repeated_variables_and_inequality_are_differential() {
        let mut guarded = rule(
            "guarded",
            atom("out", var("X"), constant("fixed")),
            vec![
                atom("pair", var("X"), var("Y")),
                atom("pair", var("Y"), var("Y")),
            ],
        );
        guarded
            .distinct_pairs
            .push(("?X".to_owned(), "?Y".to_owned()));
        let rules = vec![guarded];
        let loop_row = fact("pair", "b", "b");
        let edge = fact("pair", "a", "b");
        let mut session =
            IncrementalSession::new("contract", [loop_row.clone(), edge.clone()], &rules).unwrap();
        assert_scratch_parity(&session, &[loop_row.clone(), edge.clone()], &rules);
        assert!(closure_keys(&session.closure()).contains(&fact("out", "a", "fixed").key()));

        session
            .apply([SignedFact {
                fact: loop_row,
                weight: -1,
            }])
            .unwrap();
        assert_scratch_parity(&session, &[edge], &rules);
        assert!(!closure_keys(&session.closure()).contains(&fact("out", "a", "fixed").key()));
    }

    #[test]
    fn signed_input_consolidates_and_invalid_membership_hard_fails() {
        let rules = transitive_rules();
        let edge = fact("edge", "a", "b");
        let mut session = IncrementalSession::new("contract", [], &rules).unwrap();
        let noop = session
            .apply([
                SignedFact {
                    fact: edge.clone(),
                    weight: 1,
                },
                SignedFact {
                    fact: edge.clone(),
                    weight: -1,
                },
            ])
            .unwrap();
        assert!(noop.changes.is_empty());

        let error = session
            .apply([SignedFact {
                fact: edge,
                weight: -1,
            }])
            .expect_err("an absent-row retraction must hard-fail");
        assert!(error.message().contains("expected exactly 0 or 1"));
    }

    #[test]
    fn insert_budget_cuts_at_sorted_new_fact_commit_without_recharging_cache() {
        let rules = transitive_rules();
        let edb = vec![
            fact("edge", "a", "b"),
            fact("edge", "b", "c"),
            fact("edge", "c", "d"),
        ];
        let mut session = IncrementalSession::new("contract", edb.clone(), &rules).unwrap();
        let old = closure_keys(&session.closure());
        let inserted = fact("edge", "d", "e");
        let cut = session
            .apply_insert_budgeted(
                [SignedFact {
                    fact: inserted.clone(),
                    weight: 1,
                }],
                Some(1),
            )
            .unwrap();

        assert_eq!(cut.status, BudgetStatus::Exhausted);
        assert_eq!(cut.consumed_steps, 1);
        let cut_keys = closure_keys(&cut.closure);
        assert!(
            old.is_subset(&cut_keys),
            "stable cached closure is never recharged"
        );
        assert!(
            cut_keys.contains(&inserted.key()),
            "the asserted delta is free"
        );
        let mut full_edb = edb;
        full_edb.push(inserted);
        assert!(
            cut_keys.is_subset(&scratch(&full_edb, &rules)),
            "the cut closure is sound under the updated EDB"
        );
        assert_eq!(
            cut_keys.len(),
            old.len() + 2,
            "one asserted fact plus exactly one governed derivation"
        );

        // An exhausted attempt is atomic: the reusable base session remains unchanged.
        assert_eq!(closure_keys(&session.closure()), old);
    }

    #[test]
    fn governed_retraction_is_refused_instead_of_returning_stale_facts() {
        let rules = transitive_rules();
        let edge = fact("edge", "a", "b");
        let mut session = IncrementalSession::new("contract", [edge.clone()], &rules).unwrap();
        let error = session
            .apply_insert_budgeted(
                [SignedFact {
                    fact: edge,
                    weight: -1,
                }],
                Some(0),
            )
            .expect_err("a bounded retraction has no sound partial-prefix contract yet");
        assert!(error.message().contains("must be insert-only"));
    }

    #[test]
    fn uncovered_fragments_are_refused_at_construction() {
        let mut negated = rule(
            "negated",
            atom("p", var("X"), var("Y")),
            vec![atom("q", var("X"), var("Y"))],
        );
        negated.body[0].negated = true;
        assert!(
            IncrementalSession::new("contract", [], &[negated])
                .unwrap_err()
                .message()
                .contains("negation-as-failure")
        );

        let mut arithmetic = rule(
            "arithmetic",
            atom("p", var("X"), var("Y")),
            vec![atom("q", var("X"), var("Y"))],
        );
        arithmetic.builtins.push(QBuiltin::Is {
            target: QTerm::Var("?N".to_owned()),
            lhs: QTerm::Num(1),
            op: ArithOp::Add,
            rhs: QTerm::Num(1),
        });
        assert!(
            IncrementalSession::new("contract", [], &[arithmetic])
                .unwrap_err()
                .message()
                .contains("builtins")
        );

        let head_unbound = rule(
            "head-unbound",
            atom("p", var("X"), var("Z")),
            vec![atom("q", var("X"), var("Y"))],
        );
        let error = IncrementalSession::new("contract", [], &[head_unbound])
            .expect_err("a head-only variable must be refused at construction");
        assert!(error.message().contains("head variable ?Z"), "{error}");

        let mut inequality_unbound = rule(
            "inequality-unbound",
            atom("p", var("X"), var("Y")),
            vec![atom("q", var("X"), var("Y"))],
        );
        inequality_unbound
            .distinct_pairs
            .push(("?X".to_owned(), "?Z".to_owned()));
        let error = IncrementalSession::new("contract", [], &[inequality_unbound])
            .expect_err("an inequality-only variable must be refused at construction");
        assert!(
            error.message().contains("inequality variable ?Z"),
            "{error}"
        );
    }

    #[test]
    fn validate_fragment_subsumes_the_typed_classifier() {
        // The typed classifier and the diagnostic projection must never disagree:
        // whenever `classify_incremental_fragment` returns a typed refusal,
        // `validate_fragment` errors with the SAME message that refusal renders, and
        // whenever the classifier accepts, `validate_fragment` accepts.
        let ok_rules = transitive_rules();
        assert!(classify_incremental_fragment(&ok_rules).is_ok());
        assert!(validate_fragment(&ok_rules).is_ok());

        let mut negated = rule(
            "negated",
            atom("p", var("X"), var("Y")),
            vec![atom("q", var("X"), var("Y"))],
        );
        negated.body[0].negated = true;

        let mut arithmetic = rule(
            "arithmetic",
            atom("p", var("X"), var("Y")),
            vec![atom("q", var("X"), var("Y"))],
        );
        arithmetic.builtins.push(QBuiltin::Is {
            target: QTerm::Var("?N".to_owned()),
            lhs: QTerm::Num(1),
            op: ArithOp::Add,
            rhs: QTerm::Num(1),
        });

        let head_unbound = rule(
            "head-unbound",
            atom("p", var("X"), var("Z")),
            vec![atom("q", var("X"), var("Y"))],
        );

        let bodyless = EvalRule {
            head: atom("p", var("X"), var("Y")),
            body: Vec::new(),
            rule_iri: format!("{NS}rule/bodyless"),
            distinct_pairs: Vec::new(),
            builtins: Vec::new(),
            constraint_tag: None,
        };

        for rules in [
            vec![negated],
            vec![arithmetic],
            vec![head_unbound],
            vec![bodyless],
        ] {
            let refusal =
                classify_incremental_fragment(&rules).expect_err("must be a typed refusal");
            let diag = validate_fragment(&rules).expect_err("must project to the same Diag");
            assert_eq!(
                diag.message(),
                refusal.message(),
                "validate_fragment must reproduce the typed refusal's message verbatim"
            );
        }
    }

    #[test]
    fn identity_pins_contract_rules_and_solver() {
        let rules = transitive_rules();
        let a = IncrementalSession::new("contract-a", [], &rules).unwrap();
        let b = IncrementalSession::new("contract-b", [], &rules).unwrap();
        assert_ne!(a.identity(), b.identity());
        assert_eq!(a.identity().rule_hash, b.identity().rule_hash);
        assert_eq!(a.identity().solver_version, INCREMENTAL_SOLVER_VERSION);
    }

    /// Exhaust every directed graph on three nodes, then apply every valid single-edge
    /// insert/retract.  This covers cycles, self-loops, redundant paths, and alternative
    /// proofs without relying on random seeds; every adjusted closure must equal a clean
    /// semi-naive rebuild.
    #[test]
    fn exhaustive_three_node_transitive_updates_match_scratch() {
        let rules = transitive_rules();
        let nodes = ["a", "b", "c"];
        let universe: Vec<Fact> = nodes
            .iter()
            .flat_map(|from| nodes.iter().map(move |to| fact("edge", from, to)))
            .collect();

        for mask in 0usize..(1usize << universe.len()) {
            let edb: Vec<Fact> = universe
                .iter()
                .enumerate()
                .filter(|(bit, _)| mask & (1usize << bit) != 0)
                .map(|(_, edge)| edge.clone())
                .collect();
            let base = IncrementalSession::new("contract", edb.clone(), &rules).unwrap();
            assert_scratch_parity(&base, &edb, &rules);

            for (bit, edge) in universe.iter().enumerate() {
                let present = mask & (1usize << bit) != 0;
                let mut expected_edb = edb.clone();
                let mut adjusted = base.clone();
                if present {
                    adjusted
                        .apply([SignedFact {
                            fact: edge.clone(),
                            weight: -1,
                        }])
                        .unwrap();
                    expected_edb.retain(|candidate| candidate.key() != edge.key());
                } else {
                    adjusted
                        .apply([SignedFact {
                            fact: edge.clone(),
                            weight: 1,
                        }])
                        .unwrap();
                    expected_edb.push(edge.clone());
                }
                assert_scratch_parity(&adjusted, &expected_edb, &rules);
            }
        }
    }
}
