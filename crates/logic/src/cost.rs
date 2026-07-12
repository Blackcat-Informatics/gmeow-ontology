// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Deterministic engine-benchmark seams + the decomposable cost vector.
//!
//! This module is the public boundary a benchmark harness drives each reasoning
//! engine through, and the carrier of the deterministic cost signal the
//! `LOGIC-PERFORMANCE.md §Measurement doctrine` mandates: *"Cost is an algebra,
//! not a scalar … carried as a decomposable cost vector keyed by (rule, predicate,
//! stratum), reusing the stratification the certifier already computes; the
//! committed-derivation count, allocation bytes/count, and peak-live bytes are its
//! scalar projections."*
//!
//! - [`CostVector`] is the scalar-projection carrier of that cost semiring: a
//!   sorted [`std::collections::BTreeMap`] from [`CostKey`] `(rule, predicate,
//!   stratum)` to the committed-derived-row count, plus the allocation-scalar slots
//!   a later measurement pass fills. Its serialization is integer-valued and
//!   byte-deterministic (identical inputs ⇒ identical bytes), so it is the integer
//!   baseline the drift gate compares.
//! - [`run_native_forward`] drives the native stratified core and returns the full
//!   decomposable [`NativeForwardRun`] (rows + `consumed_steps` + cost vector +
//!   engine identity).
//! - [`run_nemo_forward`] drives the demoted Nemo bootstrap oracle and returns only
//!   its deterministically-ordered [`ForwardRows`] — Nemo exposes no governor step
//!   counts, so no cost vector is fabricated for it (the no-optionality doctrine:
//!   an absent measurement is absent, never a zeroed lie).
//!
//! Both seams build the typed EDB through the SAME
//! [`crate::reason::build_edb_facts`] the production reasoning path uses, so every
//! engine sees a byte-identical fact set and any measured difference is the engine's
//! alone.

use std::collections::{BTreeMap, BTreeSet};

use purrdf::{RdfDataset, TermValue};

use crate::oracle::{ForwardBudget, ForwardOracle, TypedChaseResult, TypedRow};
use crate::result::EngineId;

/// Wrap a cost-seam condition message as a typed diagnostic on the shared
/// substrate, preserving the authored text verbatim.
fn cost_err(detail: String) -> gmeow_errors::Diag {
    gmeow_errors::Diag::of_kind(crate::error::Engine { detail })
}

/// One coordinate of the decomposable cost vector: `(rule, predicate, stratum)`.
///
/// `rule` is the firing rule IRI (from the row's provenance — never the EDB-echo
/// assert sentinel, since only derived rows are keyed), `predicate` is the bare
/// predicate IRI of the derived row, and `stratum` is that predicate's stratum
/// index from the certifier ([`crate::certify::predicate_strata`]). Ordered
/// `(rule, predicate, stratum)` so the [`BTreeMap`] emission is deterministic.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct CostKey {
    /// The firing rule IRI (`#[name(...)]`) that committed the derivation.
    pub rule: String,
    /// The bare predicate IRI of the derived row.
    pub predicate: String,
    /// The predicate's stratum index (certifier stratification).
    pub stratum: u32,
}

/// The decomposable cost vector — the scalar-projection carrier of the cost
/// semiring (`LOGIC-PERFORMANCE.md §Measurement doctrine`).
///
/// The primary axis is the committed-derived-row count per [`CostKey`], held in a
/// sorted [`BTreeMap`] so every projection and serialization is deterministic. The
/// allocation scalars ([`Self::alloc_bytes`], [`Self::alloc_count`],
/// [`Self::peak_live_bytes`]) are the remaining scalar projections; they default to
/// `0` here and are populated by the deterministic allocation-measurement pass — no
/// allocation number is invented at this seam.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CostVector {
    /// Committed-derived-row count per `(rule, predicate, stratum)`, sorted.
    counts: BTreeMap<CostKey, u64>,
    /// Total bytes allocated during the run (a scalar projection; `0` until measured).
    alloc_bytes: u64,
    /// Total allocation count during the run (a scalar projection; `0` until measured).
    alloc_count: u64,
    /// Peak simultaneously-live bytes during the run (a scalar projection; `0` until measured).
    peak_live_bytes: u64,
}

impl CostVector {
    /// Aggregate a typed forward result into the cost vector, keying every DERIVED
    /// (non-EDB) row by `(firing rule, predicate, stratum)` and counting it.
    ///
    /// `strata` is the certifier's per-predicate stratum map
    /// ([`crate::certify::predicate_strata`]) over the SAME rule text that produced
    /// `chase`. EDB-echo rows are skipped (they carry no firing rule). A derived row
    /// whose provenance carries no firing rule, or whose predicate the certifier
    /// never stratified, is a genuine engine/certifier inconsistency and a hard
    /// error — never a silently-dropped or defaulted derivation.
    ///
    /// # Errors
    ///
    /// Returns `Err` if a derived row lacks a firing rule name, or if a derived
    /// row's predicate is absent from `strata`.
    pub(crate) fn from_chase(
        chase: &TypedChaseResult,
        strata: &BTreeMap<String, u32>,
    ) -> gmeow_errors::Result<Self> {
        let mut counts: BTreeMap<CostKey, u64> = BTreeMap::new();
        for (row, prov) in &chase.rows {
            if prov.is_edb {
                continue;
            }
            let Some(rule) = prov.rule_name.clone() else {
                return Err(cost_err(format!(
                    "derived row for predicate {:?} carries no firing rule — a derived \
                     fact must cite the rule that committed it, never an empty attribution",
                    row.predicate
                )));
            };
            let predicate = row.predicate.clone();
            let Some(&stratum) = strata.get(&predicate) else {
                return Err(cost_err(format!(
                    "derived predicate {predicate:?} has no stratum in the certifier's \
                     stratification — a rule-head predicate must be stratified; this is an \
                     engine/certifier inconsistency, not a defaultable cost"
                )));
            };
            *counts
                .entry(CostKey {
                    rule,
                    predicate,
                    stratum,
                })
                .or_insert(0) += 1;
        }
        Ok(CostVector {
            counts,
            alloc_bytes: 0,
            alloc_count: 0,
            peak_live_bytes: 0,
        })
    }

    fn from_derived_rows(
        rows: &[crate::rule_ir::DerivedRow],
        strata: &BTreeMap<String, u32>,
    ) -> gmeow_errors::Result<Self> {
        let mut counts = BTreeMap::new();
        for row in rows {
            if row.rule_iri == crate::provenance::ASSERT_RULE_IRI {
                continue;
            }
            let Some(&stratum) = strata.get(&row.predicate) else {
                return Err(cost_err(format!(
                    "repeat-evaluation derived predicate {:?} has no certified stratum",
                    row.predicate
                )));
            };
            *counts
                .entry(CostKey {
                    rule: row.rule_iri.clone(),
                    predicate: row.predicate.clone(),
                    stratum,
                })
                .or_insert(0) += 1;
        }
        Ok(Self {
            counts,
            alloc_bytes: 0,
            alloc_count: 0,
            peak_live_bytes: 0,
        })
    }

    /// Attribute the newly-present derived rows in one signed incremental
    /// transaction to their concrete `(rule, predicate, stratum)` witnesses.
    /// Asserted insertions carry no derivation witness and are deliberately omitted;
    /// negative closure changes likewise carry no fabricated reverse attribution.
    fn from_incremental_delta(
        delta: &crate::physical::IncrementalDelta,
        strata: &BTreeMap<String, u32>,
        asserted_changes: &BTreeSet<crate::rule_ir::FactKey>,
    ) -> gmeow_errors::Result<Self> {
        let mut counts: BTreeMap<CostKey, u64> = BTreeMap::new();
        for change in &delta.changes {
            if change.weight <= 0 {
                continue;
            }
            let key = change.fact.key();
            let Some(witness) = delta.derivations.get(&key) else {
                if asserted_changes.contains(&key) {
                    continue;
                }
                return Err(cost_err(format!(
                    "incremental positive derived row {key:?} carries no firing witness"
                )));
            };
            let predicate = change.fact.predicate.clone();
            let Some(&stratum) = strata.get(&predicate) else {
                return Err(cost_err(format!(
                    "incrementally-derived predicate {predicate:?} has no stratum in the \
                     certifier's stratification"
                )));
            };
            let count = u64::try_from(change.weight).map_err(|_| {
                cost_err(format!(
                    "positive incremental distinct weight {} cannot be represented as u64",
                    change.weight
                ))
            })?;
            let slot = counts
                .entry(CostKey {
                    rule: witness.rule_iri.clone(),
                    predicate,
                    stratum,
                })
                .or_insert(0);
            *slot = slot.checked_add(count).ok_or_else(|| {
                cost_err("incremental cost-vector coordinate overflow".to_owned())
            })?;
        }
        Ok(Self {
            counts,
            alloc_bytes: 0,
            alloc_count: 0,
            peak_live_bytes: 0,
        })
    }

    /// The total committed-derivation count — the sum over every coordinate (the
    /// scalar projection onto the counting semiring's `1`).
    #[must_use]
    pub fn total_derivations(&self) -> u64 {
        self.counts.values().sum()
    }

    /// The number of distinct `(rule, predicate, stratum)` coordinates carrying a count.
    #[must_use]
    pub fn attributed_coordinates(&self) -> usize {
        self.counts.len()
    }

    /// The sorted `(rule, predicate, stratum, count)` tuples — the integer-valued,
    /// byte-deterministic serialization the committed baseline compares.
    ///
    /// [`BTreeMap`] iteration is in `CostKey` order, so the tuple sequence is a pure
    /// function of `(engine version, corpus)` — identical inputs ⇒ identical bytes.
    #[must_use]
    pub fn to_sorted_tuples(&self) -> Vec<(String, String, u32, u64)> {
        self.counts
            .iter()
            .map(|(key, &count)| (key.rule.clone(), key.predicate.clone(), key.stratum, count))
            .collect()
    }

    /// Total bytes allocated during the run (a scalar projection; `0` until measured).
    #[must_use]
    pub fn alloc_bytes(&self) -> u64 {
        self.alloc_bytes
    }

    /// Total allocation count during the run (a scalar projection; `0` until measured).
    #[must_use]
    pub fn alloc_count(&self) -> u64 {
        self.alloc_count
    }

    /// Peak simultaneously-live bytes during the run (a scalar projection; `0` until measured).
    #[must_use]
    pub fn peak_live_bytes(&self) -> u64 {
        self.peak_live_bytes
    }

    /// Attach the deterministically-measured allocation scalars (the remaining
    /// scalar projections of the cost semiring). The later allocation-measurement
    /// pass calls this once; the derivation counts are never touched.
    ///
    /// The argument order `(alloc_bytes, alloc_count, peak_live_bytes)` matches the
    /// `(bytes, count, peak_live)` field shape of the harness-scoped allocation
    /// sample, so a harness plugs a measured sample straight in field-for-field —
    /// intentionally by value (three `u64`s), never by depending on the measurement
    /// crate here (its `#[global_allocator]` must never reach this crate or the CLI).
    pub fn set_allocation(&mut self, alloc_bytes: u64, alloc_count: u64, peak_live_bytes: u64) {
        self.alloc_bytes = alloc_bytes;
        self.alloc_count = alloc_count;
        self.peak_live_bytes = peak_live_bytes;
    }
}

/// A single materialized row exposed across the public benchmark seam: the bare
/// relation name plus its decoded native-term arguments (the neutral shape both the
/// native and Nemo engines emit — a public projection of the crate-internal
/// [`TypedRow`]).
#[derive(Debug, Clone, PartialEq)]
pub struct ForwardRow {
    /// The relation name (a full predicate IRI, un-bracketed, or a bare program-local symbol).
    pub predicate: String,
    /// One decoded native term per column in the row.
    pub args: Vec<TermValue>,
}

/// A deterministically-ordered materialized row set — the engine-neutral forward
/// output both seams return.
///
/// The rows are sorted by `(predicate, term-display of each argument)`, a total
/// order independent of the engine's internal emission order, so two engines (or two
/// runs of one engine) over the same EDB yield byte-comparable row sets.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ForwardRows {
    /// The materialized rows in canonical `(predicate, args)` order.
    pub rows: Vec<ForwardRow>,
}

impl ForwardRows {
    /// Project a typed chase result's rows into the sorted public row set.
    fn from_chase(chase: &TypedChaseResult) -> Self {
        Self::from_typed_rows(chase.rows.iter().map(|(row, _prov)| row))
    }

    /// Project any iterator of crate-internal [`TypedRow`]s into the sorted public set.
    fn from_typed_rows<'a>(rows: impl Iterator<Item = &'a TypedRow>) -> Self {
        let mut out: Vec<ForwardRow> = rows
            .map(|row| ForwardRow {
                predicate: row.predicate.clone(),
                args: row.args.clone(),
            })
            .collect();
        // Canonical, engine-independent order: predicate first, then the term-display
        // surface of each argument (deterministic and total — the display is a pure
        // function of the term).
        out.sort_by(|a, b| {
            let ka = row_sort_key(a);
            let kb = row_sort_key(b);
            ka.cmp(&kb)
        });
        ForwardRows { rows: out }
    }

    /// Whether the row set is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    /// The number of rows.
    #[must_use]
    pub fn len(&self) -> usize {
        self.rows.len()
    }
}

/// The canonical sort key of a row: `(predicate, [term-display, …])` — a pure
/// function of the row, so the ordering is fully deterministic.
fn row_sort_key(row: &ForwardRow) -> (String, Vec<String>) {
    (
        row.predicate.clone(),
        row.args
            .iter()
            .map(crate::provenance::term_display)
            .collect(),
    )
}

/// The full decomposable result of a native forward run: the deterministically
/// ordered row set, the governor's committed-derivation count, the cost vector, and
/// the engine-version identity the run was produced under.
#[derive(Debug, Clone)]
pub struct NativeForwardRun {
    /// The materialized rows (asserted EDB ∪ derived), in canonical order.
    pub rows: ForwardRows,
    /// The governor's committed-derivation count (`0` on the ungoverned arity-generic path).
    pub consumed_steps: u64,
    /// The decomposable cost vector keyed by `(rule, predicate, stratum)`.
    pub cost: CostVector,
    /// The engine-version identity this run was produced under.
    pub engine: EngineId,
}

/// One cold/warm observation from [`RepeatForwardSession`].
///
/// Every field is deterministic: no wall-clock enters this surface. Allocation count
/// and peak-live are attached by the dedicated benchmark allocator outside this crate.
pub struct RepeatForwardObservation {
    /// Whether the bounded cache supplied the executable.
    pub cache_hit: bool,
    /// Physical plans built during this evaluation (`1` cold, `0` warm).
    pub plan_builds: u64,
    /// Static rule/atom/guard/builtin nodes inspected by planning (`N` cold, `0` warm).
    pub planning_units: u64,
    /// Whether this evaluation consumed the exact same immutable `Arc<Executable>` as
    /// the session's first evaluation.
    pub same_executable_as_first: bool,
    /// Canonical rule-program hash from the executable identity.
    pub rule_hash: [u8; 32],
    /// Physical solver/planner ABI version from the executable identity.
    pub solver_version: &'static str,
    /// Governor committed-derivation count.
    pub consumed_steps: u64,
    /// Decomposable `(rule, predicate, stratum)` derivation vector.
    pub cost: CostVector,
    /// Byte-stable BLAKE3 digest of the complete sorted materialized row set.
    pub closure_hash: [u8; 32],
    /// Byte-stable BLAKE3 digest of only `(world, subject, predicate, object)`, used
    /// to prove Record and Skip commit the identical fact closure.
    pub fact_closure_hash: [u8; 32],
    /// Number of bounded proof-height annotations retained by Record mode (one per
    /// asserted or derived row in this complete closure).
    pub annotation_count: u64,
    /// Maximum selected minimal-proof height in the closure.
    pub max_proof_height: u32,
}

/// Facts-only observation over the same EDB/rules/executable as a Record run.
///
/// Skip retains no proof annotations. This public benchmark projection records only
/// the exact fact closure and committed-step count needed for the Record/Skip law.
pub struct SkipForwardObservation {
    /// Governor committed-derivation count.
    pub consumed_steps: u64,
    /// Byte-stable fact-only closure digest.
    pub fact_closure_hash: [u8; 32],
    /// Asserted plus derived fact count.
    pub fact_count: u64,
}

/// Record-mode half of the fair bounded-provenance overhead probe.
///
/// Unlike [`RepeatForwardObservation`], this projection performs no plan-cache cost
/// attribution or provenance-sensitive closure hash after evaluation. Its post-work
/// exactly mirrors [`SkipForwardObservation`]: fact-only hash, steps, and row count,
/// plus the two bounded annotation scalars Record alone owns.
pub struct RecordForwardObservation {
    /// Governor committed-derivation count.
    pub consumed_steps: u64,
    /// Byte-stable fact-only closure digest.
    pub fact_closure_hash: [u8; 32],
    /// Number of retained proof-height annotations.
    pub annotation_count: u64,
    /// Maximum selected minimal-proof height.
    pub max_proof_height: u32,
}

/// Fixed-EDB/rule session proving a second evaluation reuses one immutable physical
/// plan while executing the same forward core from scratch.
///
/// Parsing, EDB loading, and certified-stratum construction happen in [`Self::prepare`]
/// outside the measured region. Each [`Self::evaluate`] still performs a complete
/// materialization; only stratification and physical planning are cacheable.
pub struct RepeatForwardSession {
    store: crate::store::WorldStore,
    rules: Vec<crate::rule_ir::EvalRule>,
    strata: BTreeMap<String, u32>,
    contract_hash: String,
    cache: crate::physical::PlanCache,
    first_executable: Option<std::sync::Arc<crate::physical::Executable>>,
}

impl RepeatForwardSession {
    /// Prepare a binary named-ternary forward session.
    ///
    /// # Errors
    ///
    /// Returns an error for malformed/non-ternary rules, EDB loading failure, or a
    /// certification/lowering failure.
    pub fn prepare(
        edb: &RdfDataset,
        rules: &str,
        contract_hash: impl Into<String>,
    ) -> gmeow_errors::Result<Self> {
        let program = crate::nemo_engine::NemoParsedRules::parse_unvalidated(rules)?.into_program();
        if !crate::oracle::program_is_pure_ternary(&program) {
            return Err(cost_err(
                "repeat-forward plan probe requires a pure named-ternary rule program".to_owned(),
            ));
        }
        let eval_rules = crate::rule_ir::lower_program_eval_rules(&program)?;
        let strata = crate::certify::predicate_strata(rules)?;
        let store = crate::store::WorldStore::new();
        store.load_dataset(edb)?;
        Ok(Self {
            store,
            rules: eval_rules,
            strata,
            contract_hash: contract_hash.into(),
            cache: crate::physical::PlanCache::new(2),
            first_executable: None,
        })
    }

    /// Execute one complete materialization through the session-local plan cache.
    ///
    /// # Errors
    ///
    /// Returns an error if the program is non-stratifiable, execution reports a
    /// declared native gap, or a derived row cannot be attributed to its stratum.
    pub fn evaluate(&mut self) -> gmeow_errors::Result<RepeatForwardObservation> {
        let lookup = self
            .cache
            .get_or_compile(self.contract_hash.clone(), self.rules.clone());
        let executable = lookup.executable.ok_or_else(|| {
            cost_err("repeat-forward plan probe program is non-stratifiable".to_owned())
        })?;
        let same_executable_as_first = self
            .first_executable
            .as_ref()
            .is_none_or(|first| std::sync::Arc::ptr_eq(first, &executable));
        if self.first_executable.is_none() {
            self.first_executable = Some(executable.clone());
        }
        let identity = executable.identity();
        let rule_hash = *identity.rule_hash();
        let solver_version = identity.solver_version();

        let budgeted =
            match crate::physical::materialize_native(&self.store, executable.as_ref(), None)? {
                crate::physical::NativeOutcome::Decided(budgeted) => budgeted,
                crate::physical::NativeOutcome::Unsupported(kind) => {
                    return Err(cost_err(format!(
                        "repeat-forward plan probe hit declared native gap {kind:?}"
                    )));
                }
            };
        let closure_hash = derived_rows_hash(&budgeted.rows);
        let fact_closure_hash = derived_fact_rows_hash(&budgeted.rows);
        let annotation_count = budgeted.rows.len() as u64;
        let max_proof_height = budgeted
            .rows
            .iter()
            .map(|row| row.proof_height.get())
            .max()
            .unwrap_or(0);
        let cost = CostVector::from_derived_rows(&budgeted.rows, &self.strata)?;
        Ok(RepeatForwardObservation {
            cache_hit: lookup.cache_hit,
            plan_builds: lookup.plan_builds,
            planning_units: lookup.planning_units,
            same_executable_as_first,
            rule_hash,
            solver_version,
            consumed_steps: budgeted.consumed_steps,
            cost,
            closure_hash,
            fact_closure_hash,
            annotation_count,
            max_proof_height,
        })
    }

    /// Execute the same complete binary plan in facts-only Skip mode.
    ///
    /// The session cache must already contain the executable (normally after the
    /// paired cold/warm Record observations). The method deliberately records no
    /// provenance and returns no cost vector that would require rule attribution.
    ///
    /// # Errors
    ///
    /// Returns an error for a non-stratifiable program or a declared execution gap.
    pub fn evaluate_skip(&mut self) -> gmeow_errors::Result<SkipForwardObservation> {
        let lookup = self
            .cache
            .get_or_compile(self.contract_hash.clone(), self.rules.clone());
        let executable = lookup.executable.ok_or_else(|| {
            cost_err("repeat-forward Skip probe program is non-stratifiable".to_owned())
        })?;

        let mut worlds = self.store.worlds();
        worlds.sort();
        let mut rows = Vec::new();
        let mut consumed_steps = 0u64;
        for world in worlds {
            let edb = crate::rule_ir::world_edb_facts(&self.store, &world)?;
            let mut relation = crate::physical::RelationStore::new();
            for fact in edb {
                relation.insert(&fact.predicate, &fact.subject, &fact.object);
            }
            let budgeted = match crate::physical::evaluate(relation, executable.as_ref(), None)? {
                crate::physical::NativeOutcome::Decided(budgeted) => budgeted,
                crate::physical::NativeOutcome::Unsupported(kind) => {
                    return Err(cost_err(format!(
                        "repeat-forward Skip probe hit declared native gap {kind:?}"
                    )));
                }
            };
            consumed_steps = consumed_steps
                .checked_add(budgeted.consumed_steps)
                .ok_or_else(|| cost_err("Skip committed-step count overflow".to_owned()))?;
            rows.extend(budgeted.rows.into_iter().map(|fact| (world.clone(), fact)));
        }
        rows.sort_by(|(world_a, fact_a), (world_b, fact_b)| {
            world_a
                .cmp(world_b)
                .then_with(|| fact_a.key().cmp(&fact_b.key()))
        });
        Ok(SkipForwardObservation {
            consumed_steps,
            fact_closure_hash: skipped_fact_rows_hash(&rows),
            fact_count: rows.len() as u64,
        })
    }

    /// Execute one complete warm-plan Record evaluation for the fair provenance
    /// overhead pair.
    ///
    /// # Errors
    ///
    /// Returns an error for a non-stratifiable program or a declared execution gap.
    pub fn evaluate_record_provenance(&mut self) -> gmeow_errors::Result<RecordForwardObservation> {
        let lookup = self
            .cache
            .get_or_compile(self.contract_hash.clone(), self.rules.clone());
        let executable = lookup.executable.ok_or_else(|| {
            cost_err("repeat-forward Record probe program is non-stratifiable".to_owned())
        })?;
        let budgeted =
            match crate::physical::materialize_native(&self.store, executable.as_ref(), None)? {
                crate::physical::NativeOutcome::Decided(budgeted) => budgeted,
                crate::physical::NativeOutcome::Unsupported(kind) => {
                    return Err(cost_err(format!(
                        "repeat-forward Record probe hit declared native gap {kind:?}"
                    )));
                }
            };
        Ok(RecordForwardObservation {
            consumed_steps: budgeted.consumed_steps,
            fact_closure_hash: derived_fact_rows_hash(&budgeted.rows),
            annotation_count: budgeted.rows.len() as u64,
            max_proof_height: budgeted
                .rows
                .iter()
                .map(|row| row.proof_height.get())
                .max()
                .unwrap_or(0),
        })
    }
}

fn derived_rows_hash(rows: &[crate::rule_ir::DerivedRow]) -> [u8; 32] {
    fn frame(hasher: &mut blake3::Hasher, value: &str) {
        hasher.update(&(value.len() as u64).to_le_bytes());
        hasher.update(value.as_bytes());
    }

    let mut hasher = blake3::Hasher::new();
    frame(&mut hasher, "gmeow-repeat-forward-closure-v1");
    hasher.update(&(rows.len() as u64).to_le_bytes());
    for row in rows {
        frame(&mut hasher, &row.graph);
        frame(&mut hasher, &crate::provenance::term_display(&row.subject));
        frame(&mut hasher, &row.predicate);
        frame(&mut hasher, &crate::provenance::term_display(&row.object));
        frame(&mut hasher, &row.rule_iri);
        hasher.update(&row.proof_height.get().to_le_bytes());
    }
    *hasher.finalize().as_bytes()
}

fn derived_fact_rows_hash(rows: &[crate::rule_ir::DerivedRow]) -> [u8; 32] {
    fn frame(hasher: &mut blake3::Hasher, value: &str) {
        hasher.update(&(value.len() as u64).to_le_bytes());
        hasher.update(value.as_bytes());
    }

    let mut hasher = blake3::Hasher::new();
    frame(&mut hasher, "gmeow-record-skip-fact-closure-v1");
    hasher.update(&(rows.len() as u64).to_le_bytes());
    for row in rows {
        frame(&mut hasher, &row.graph);
        frame(&mut hasher, &crate::provenance::term_display(&row.subject));
        frame(&mut hasher, &row.predicate);
        frame(&mut hasher, &crate::provenance::term_display(&row.object));
    }
    *hasher.finalize().as_bytes()
}

fn skipped_fact_rows_hash(rows: &[(String, crate::rule_ir::Fact)]) -> [u8; 32] {
    fn frame(hasher: &mut blake3::Hasher, value: &str) {
        hasher.update(&(value.len() as u64).to_le_bytes());
        hasher.update(value.as_bytes());
    }

    let mut hasher = blake3::Hasher::new();
    frame(&mut hasher, "gmeow-record-skip-fact-closure-v1");
    hasher.update(&(rows.len() as u64).to_le_bytes());
    for (world, fact) in rows {
        frame(&mut hasher, world);
        frame(&mut hasher, &crate::provenance::term_display(&fact.subject));
        frame(&mut hasher, &fact.predicate);
        frame(&mut hasher, &crate::provenance::term_display(&fact.object));
    }
    *hasher.finalize().as_bytes()
}

/// One signed row emitted by the public incremental benchmark seam.
#[derive(Debug, Clone, PartialEq)]
pub struct SignedForwardRow {
    /// The world-scoped row whose set membership changed.
    pub row: ForwardRow,
    /// `+1` for insertion into the closure, `-1` for retraction.
    pub weight: i64,
}

/// The deterministic result of one incremental forward transaction.
#[derive(Debug, Clone)]
pub struct NativeIncrementalRun {
    /// The sound closure after the transaction (or at an inline budget cut).
    pub rows: ForwardRows,
    /// Signed closure changes in lexical fact order.
    pub changes: Vec<SignedForwardRow>,
    /// Number of derived rows currently present beyond the asserted EDB set.
    pub derived_count: u64,
    /// Genuinely-new derived rows charged by the inline governor.
    pub consumed_steps: u64,
    /// Signed-delta rows admitted at mechanically differentiated join positions.
    pub joined_rows: u64,
    /// Adjusted nested fixed-point iterations run by this transaction.
    pub inner_iterations: usize,
    /// The positive derived-change cost vector. Retractions never receive a
    /// fabricated reverse-rule attribution.
    pub cost: CostVector,
    /// Whether an inline step bound completed or cut the transaction.
    pub status: crate::seam::BudgetStatus,
    /// Native engine identity.
    pub engine: EngineId,
}

/// A fixed-program, single-world incremental forward session for deterministic
/// benchmark and parity lanes.
///
/// Construction performs the initial materialization. Callers therefore create the
/// session outside a measured region and measure only [`Self::insert`] or
/// [`Self::retract`], matching the stable-world/small-delta loop this path optimizes.
#[derive(Debug, Clone)]
pub struct IncrementalForwardSession {
    world: String,
    inner: crate::physical::IncrementalSession,
    edb: BTreeSet<crate::rule_ir::FactKey>,
    strata: BTreeMap<String, u32>,
}

impl IncrementalForwardSession {
    /// Prepare a fixed-rule incremental session from a single named-graph world.
    ///
    /// # Errors
    ///
    /// Returns an error when the EDB is not exactly one named world, the rule text
    /// is outside finite positive binary Datalog, or the rules cannot be stratified.
    pub fn prepare(edb: &RdfDataset, rules: &str) -> gmeow_errors::Result<Self> {
        let (world, facts) = incremental_dataset_facts(edb, None)?;
        let eval_rules = crate::rule_ir::parse_eval_rules(rules)?;
        let strata = crate::certify::predicate_strata(rules)?;
        let edb_keys = facts.iter().map(crate::rule_ir::Fact::key).collect();
        let contract_hash = format!(
            "gmeow-native-incremental-forward-v1:{}",
            blake3::hash(world.as_bytes()).to_hex()
        );
        let inner = crate::physical::IncrementalSession::new(contract_hash, facts, &eval_rules)?;
        Ok(Self {
            world,
            inner,
            edb: edb_keys,
            strata,
        })
    }

    /// Apply an insert-only dataset under the inline governor.
    ///
    /// `max_steps = None` is unbounded but still counts committed derived rows. An
    /// exhausted transaction leaves the cached session unchanged and returns its
    /// sound partial closure.
    ///
    /// # Errors
    ///
    /// Returns an error for a different/default world, duplicate insertion, or an
    /// invalid/overflowing signed transaction.
    pub fn insert(
        &mut self,
        changes: &RdfDataset,
        max_steps: Option<u64>,
    ) -> gmeow_errors::Result<NativeIncrementalRun> {
        let (_world, facts) = incremental_dataset_facts(changes, Some(&self.world))?;
        let asserted_changes: BTreeSet<crate::rule_ir::FactKey> =
            facts.iter().map(crate::rule_ir::Fact::key).collect();
        let next_edb_count = self.edb.len().checked_add(facts.len()).ok_or_else(|| {
            cost_err("incremental EDB row-count overflow during insertion".to_owned())
        })?;
        let signed = facts
            .iter()
            .cloned()
            .map(|fact| crate::physical::SignedFact { fact, weight: 1 });
        let budgeted = self.inner.apply_insert_budgeted(signed, max_steps)?;
        let run = incremental_run(
            &self.world,
            &self.strata,
            next_edb_count,
            budgeted,
            &asserted_changes,
        )?;
        if run.status == crate::seam::BudgetStatus::Ok {
            self.edb.extend(facts.iter().map(crate::rule_ir::Fact::key));
        }
        Ok(run)
    }

    /// Apply an unbounded retract-only dataset.
    ///
    /// Bounded deletion deliberately remains outside this seam until a sound partial
    /// delete frontier is defined; unbounded retraction still uses the signed nested
    /// circuit and returns the exact new least model.
    ///
    /// # Errors
    ///
    /// Returns an error for a different/default world, absent retraction, or an
    /// invalid/overflowing signed transaction.
    pub fn retract(&mut self, changes: &RdfDataset) -> gmeow_errors::Result<NativeIncrementalRun> {
        let (_world, facts) = incremental_dataset_facts(changes, Some(&self.world))?;
        let asserted_changes: BTreeSet<crate::rule_ir::FactKey> =
            facts.iter().map(crate::rule_ir::Fact::key).collect();
        let next_edb_count = self.edb.len().checked_sub(facts.len()).ok_or_else(|| {
            cost_err("incremental EDB row-count underflow during retraction".to_owned())
        })?;
        let signed = facts
            .iter()
            .cloned()
            .map(|fact| crate::physical::SignedFact { fact, weight: -1 });
        let delta = self.inner.apply(signed)?;
        let closure = self.inner.closure();
        let run = incremental_run(
            &self.world,
            &self.strata,
            next_edb_count,
            crate::physical::BudgetedIncrementalDelta {
                delta,
                closure,
                status: crate::seam::BudgetStatus::Ok,
                consumed_steps: 0,
            },
            &asserted_changes,
        )?;
        for fact in facts {
            self.edb.remove(&fact.key());
        }
        Ok(run)
    }
}

fn incremental_run(
    world: &str,
    strata: &BTreeMap<String, u32>,
    edb_count: usize,
    budgeted: crate::physical::BudgetedIncrementalDelta,
    asserted_changes: &BTreeSet<crate::rule_ir::FactKey>,
) -> gmeow_errors::Result<NativeIncrementalRun> {
    let crate::physical::BudgetedIncrementalDelta {
        delta,
        closure,
        status,
        consumed_steps,
    } = budgeted;
    let derived_count = closure.len().checked_sub(edb_count).ok_or_else(|| {
        cost_err(format!(
            "incremental closure has {} rows but the asserted EDB has {edb_count}",
            closure.len()
        ))
    })? as u64;
    let cost = CostVector::from_incremental_delta(&delta, strata, asserted_changes)?;
    let rows = forward_rows_from_facts(&closure, world);
    let changes = delta
        .changes
        .into_iter()
        .map(|change| SignedForwardRow {
            row: forward_row_from_fact(change.fact, world),
            weight: change.weight,
        })
        .collect();
    Ok(NativeIncrementalRun {
        rows,
        changes,
        derived_count,
        consumed_steps,
        joined_rows: delta.joined_rows,
        inner_iterations: delta.inner_iterations,
        cost,
        status,
        engine: EngineId::native(),
    })
}

fn forward_rows_from_facts(facts: &[crate::rule_ir::Fact], world: &str) -> ForwardRows {
    let mut rows: Vec<ForwardRow> = facts
        .iter()
        .cloned()
        .map(|fact| forward_row_from_fact(fact, world))
        .collect();
    rows.sort_by_key(row_sort_key);
    ForwardRows { rows }
}

fn forward_row_from_fact(fact: crate::rule_ir::Fact, world: &str) -> ForwardRow {
    ForwardRow {
        predicate: fact.predicate,
        args: vec![fact.subject, fact.object, TermValue::simple_literal(world)],
    }
}

/// Coerce a dataset through the production typed-EDB bridge and then drop the
/// validated single world column into the binary incremental fact shape.
fn incremental_dataset_facts(
    dataset: &RdfDataset,
    expected_world: Option<&str>,
) -> gmeow_errors::Result<(String, Vec<crate::rule_ir::Fact>)> {
    let typed = crate::reason::build_edb_facts(dataset)?;
    if typed.is_empty() {
        return Err(cost_err(
            "incremental dataset must contain at least one named-world IRI-object fact".to_owned(),
        ));
    }
    let interner = typed.interner();
    let mut world: Option<String> = None;
    let mut facts = Vec::new();
    for fact in typed.facts() {
        if fact.args.len() != 3 {
            return Err(cost_err(format!(
                "incremental binary fact {:?} has arity {}, expected 3",
                fact.predicate,
                fact.args.len()
            )));
        }
        let row_world = match interner.resolve(fact.args[2]) {
            TermValue::Literal { lexical_form, .. } => lexical_form.clone(),
            other => {
                return Err(cost_err(format!(
                    "incremental world column must be a simple literal, got {other:?}"
                )));
            }
        };
        if let Some(expected) = expected_world
            && row_world != expected
        {
            return Err(cost_err(format!(
                "incremental transaction world {row_world:?} differs from session world {expected:?}"
            )));
        }
        if let Some(first) = &world {
            if first != &row_world {
                return Err(cost_err(format!(
                    "incremental dataset spans multiple worlds ({first:?}, {row_world:?}); \
                     prepare one fixed session per world"
                )));
            }
        } else {
            world = Some(row_world);
        }
        facts.push(crate::rule_ir::Fact {
            subject: interner.resolve(fact.args[0]).clone(),
            predicate: fact.predicate.clone(),
            object: interner.resolve(fact.args[1]).clone(),
        });
    }
    facts.sort_by_key(crate::rule_ir::Fact::key);
    Ok((world.expect("non-empty typed EDB has a world"), facts))
}

/// Drive the NATIVE stratified forward core over `edb` under `rules`, returning the
/// decomposable [`NativeForwardRun`].
///
/// The typed EDB is built through [`crate::reason::build_edb_facts`] (the exact
/// production construction), then the native evaluation runs through
/// [`crate::oracle::native_forward_with_frontier`] — the same body the production
/// [`crate::oracle::NativeForwardOracle`] runs, additionally surfacing the
/// governor's completion frontier. `consumed_steps` is read from that frontier; the
/// [`CostVector`] is aggregated from the result's derived rows + provenance and the
/// certifier's per-predicate stratification ([`crate::certify::predicate_strata`]).
///
/// # Errors
///
/// Returns `Err` if the EDB cannot be built, if the native chase declines the rule
/// set (a declared gap), if the certifier cannot stratify the rules, or if a derived
/// row cannot be attributed a `(rule, predicate, stratum)` coordinate.
pub fn run_native_forward(edb: &RdfDataset, rules: &str) -> gmeow_errors::Result<NativeForwardRun> {
    let facts = crate::reason::build_edb_facts(edb)?;
    let (chase, frontier) = crate::oracle::native_forward_with_frontier(&facts, rules)?;
    let strata = crate::certify::predicate_strata(rules)?;
    let cost = CostVector::from_chase(&chase, &strata)?;
    Ok(NativeForwardRun {
        rows: ForwardRows::from_chase(&chase),
        consumed_steps: frontier.consumed_steps,
        cost,
        engine: EngineId::native(),
    })
}

/// Drive the DEMOTED Nemo bootstrap oracle over `edb` under `rules`, returning ONLY
/// its deterministically-ordered [`ForwardRows`].
///
/// Nemo is off the primary reasoning path and exposes no governor step counts, so
/// this seam fabricates no cost vector for it (the no-optionality doctrine). It
/// reuses the SAME [`crate::reason::build_edb_facts`] EDB construction as
/// [`run_native_forward`], so both engines see a byte-identical fact set — the
/// precondition for a faithful native↔Nemo comparison.
///
/// # Errors
///
/// Returns `Err` if the EDB cannot be built or if the Nemo chase fails to
/// parse/validate/evaluate/decode.
pub fn run_nemo_forward(edb: &RdfDataset, rules: &str) -> gmeow_errors::Result<ForwardRows> {
    let facts = crate::reason::build_edb_facts(edb)?;
    let oracle = crate::oracle::nemo_forward_oracle();
    let chase = oracle.materialize(&facts, rules, &ForwardBudget::UNBOUNDED)?;
    Ok(ForwardRows::from_chase(&chase))
}

/// Drive the DEMOTED Nemo bootstrap oracle over `edb` under a VALUE-INVENTING
/// (existential-rule) `rules` set via the FACTS-ONLY chase, returning its
/// deterministically-ordered [`ForwardRows`].
///
/// This is the existential sibling of [`run_nemo_forward`]. The provenance-carrying
/// [`crate::oracle::nemo_forward_oracle`] that [`run_nemo_forward`] uses hard-errors
/// on a chase that invents fresh labeled nulls ("no trace tree for derived fact") —
/// an architectural fact of the trace reconstruction, NOT an optional degradation —
/// so an existential program is materialized through the facts-only
/// [`crate::oracle::NemoFactsOracle`] instead, exactly as the `materialize_routed`
/// existential fallback (`demote_existential_to_facts_only`) does. The facts-only
/// closure carries no provenance (it attributes nothing), which is why no cost vector
/// is fabricated for it — the row set is the sound, comparable signal (the invented
/// nulls are labeled per-engine, so a benchmark compares the DERIVED COUNT across
/// engines, never the null identifiers).
///
/// It reuses the SAME [`crate::reason::build_edb_facts`] EDB construction as the
/// native chase seam, so both engines see a byte-identical fact set.
///
/// # Errors
///
/// Returns `Err` if the EDB cannot be built or if the Nemo facts-only chase fails to
/// parse/validate/evaluate/decode.
pub fn run_nemo_forward_facts_only(
    edb: &RdfDataset,
    rules: &str,
) -> gmeow_errors::Result<ForwardRows> {
    let facts = crate::reason::build_edb_facts(edb)?;
    let chase =
        crate::oracle::NemoFactsOracle.materialize(&facts, rules, &ForwardBudget::UNBOUNDED)?;
    Ok(ForwardRows::from_chase(&chase))
}

#[cfg(test)]
mod tests {
    use super::*;
    use purrdf::{RdfDatasetBuilder, RdfQuad, RdfTerm};

    const EDGE: &str = "http://example.org/edge";
    const PATH: &str = "http://example.org/path";
    const REACH: &str = "http://example.org/reach";
    const W: &str = "http://example.org/w";
    const A: &str = "http://example.org/a";
    const B: &str = "http://example.org/b";
    const C: &str = "http://example.org/c";

    /// A tiny 2-stratum pure-ternary Datalog program (binary-eligible, so the
    /// governed semi-naive core runs and `consumed_steps` is populated):
    ///
    /// * stratum 1 — `path` is the transitive closure of `edge`;
    /// * stratum 2 — `reach` echoes every `path` edge.
    ///
    /// Named rules (`#[name(...)]`) so derived rows carry a real firing-rule IRI.
    fn two_stratum_rules() -> String {
        format!(
            "#[name(\"http://example.org/rules/edge-is-path\")]\n\
             <{PATH}>(?s, ?o, ?w) :- <{EDGE}>(?s, ?o, ?w) .\n\
             #[name(\"http://example.org/rules/path-trans\")]\n\
             <{PATH}>(?s, ?o, ?w) :- <{PATH}>(?s, ?m, ?w), <{EDGE}>(?m, ?o, ?w) .\n\
             #[name(\"http://example.org/rules/path-is-reach\")]\n\
             <{REACH}>(?s, ?o, ?w) :- <{PATH}>(?s, ?o, ?w) .\n"
        )
    }

    /// EDB: `edge(a, b)`, `edge(b, c)` in world `W`.
    fn two_edge_edb() -> std::sync::Arc<RdfDataset> {
        let mut builder = RdfDatasetBuilder::new();
        for (s, o) in [(A, B), (B, C)] {
            builder.push_owned_quad(
                &RdfQuad::new(RdfTerm::iri(s), EDGE, RdfTerm::iri(o)).in_graph(RdfTerm::iri(W)),
            );
        }
        builder.freeze().expect("valid test dataset")
    }

    /// The native seam is deterministic (byte-identical cost vector across two runs)
    /// AND its attribution is non-vacuous (≥1 real-rule coordinate with a nonzero
    /// count), and the Nemo seam feasibly returns a non-empty row set over the SAME
    /// program — the seams a benchmark harness drives each engine through.
    #[test]
    fn native_forward_cost_vector_is_deterministic_and_attributed() {
        let edb = two_edge_edb();
        let rules = two_stratum_rules();

        let run_a = run_native_forward(edb.as_ref(), &rules).expect("native forward run a");
        let run_b = run_native_forward(edb.as_ref(), &rules).expect("native forward run b");

        // Determinism: the two cost vectors are byte-identical, and so is their
        // integer serialization.
        assert_eq!(
            run_a.cost, run_b.cost,
            "the native cost vector must be deterministic across identical runs"
        );
        assert_eq!(run_a.cost.to_sorted_tuples(), run_b.cost.to_sorted_tuples());
        assert_eq!(
            run_a.rows, run_b.rows,
            "the native row set must be deterministic"
        );
        assert_eq!(run_a.consumed_steps, run_b.consumed_steps);

        // Non-vacuous attribution: ≥1 coordinate keyed to a REAL rule IRI with a
        // nonzero count.
        let tuples = run_a.cost.to_sorted_tuples();
        assert!(
            tuples.iter().any(|(rule, _pred, _stratum, count)| {
                rule.starts_with("http://example.org/rules/") && *count > 0
            }),
            "the cost vector must attribute ≥1 derivation to a real rule: {tuples:?}"
        );
        assert!(
            run_a.cost.total_derivations() > 0,
            "the program derives facts, so total_derivations must be nonzero"
        );

        // The governed semi-naive core committed derivations, so the step probe is nonzero.
        assert!(
            run_a.consumed_steps > 0,
            "the binary-eligible program runs the governed core; consumed_steps must be nonzero"
        );

        // Engine identity is stamped.
        assert_eq!(run_a.engine, EngineId::native());

        // Two distinct strata are attributed: `path` (stratum 1) and `reach`
        // (stratum 2) — proving the stratum coordinate is threaded, not flattened.
        let strata: std::collections::BTreeSet<u32> = tuples
            .iter()
            .map(|(_r, _p, stratum, _c)| *stratum)
            .collect();
        assert!(
            strata.contains(&1) && strata.contains(&2),
            "both stratum 1 (path) and stratum 2 (reach) must be attributed: {tuples:?}"
        );

        // Feasibility: the Nemo seam returns a non-empty row set over the same program.
        let nemo = run_nemo_forward(edb.as_ref(), &rules).expect("nemo forward run");
        assert!(
            !nemo.is_empty(),
            "the Nemo seam must materialize a non-empty row set for the same program"
        );
    }

    #[test]
    fn repeat_forward_session_reuses_plan_with_identical_cost_and_closure() {
        let edb = two_edge_edb();
        let rules = two_stratum_rules();
        let mut session =
            RepeatForwardSession::prepare(edb.as_ref(), &rules, "repeat-forward-test")
                .expect("prepare repeat-forward session");

        let cold = session.evaluate().expect("cold evaluation");
        let warm = session.evaluate().expect("warm evaluation");
        assert!(!cold.cache_hit);
        assert_eq!(cold.plan_builds, 1);
        assert!(cold.planning_units > 0);
        assert!(warm.cache_hit);
        assert_eq!((warm.plan_builds, warm.planning_units), (0, 0));
        assert!(warm.same_executable_as_first);
        assert_eq!(cold.rule_hash, warm.rule_hash);
        assert_eq!(cold.solver_version, warm.solver_version);
        assert_eq!(cold.closure_hash, warm.closure_hash);
        assert_eq!(cold.fact_closure_hash, warm.fact_closure_hash);
        assert_eq!(cold.consumed_steps, warm.consumed_steps);
        assert_eq!(cold.cost, warm.cost);
        assert_eq!(warm.annotation_count, 8, "one height per closure fact");
        assert_eq!(
            warm.max_proof_height, 3,
            "reach(a,c) is one level above the two-edge path proof"
        );
        assert!(
            cold.cost.attributed_coordinates() >= 2,
            "repeat evidence retains per-(rule,predicate,stratum) attribution"
        );

        let record = session
            .evaluate_record_provenance()
            .expect("bounded-provenance evaluation");
        let skip = session.evaluate_skip().expect("facts-only evaluation");
        assert_eq!(record.annotation_count, warm.annotation_count);
        assert_eq!(record.max_proof_height, warm.max_proof_height);
        assert_eq!(skip.fact_count, record.annotation_count);
        assert_eq!(skip.fact_closure_hash, record.fact_closure_hash);
        assert_eq!(skip.consumed_steps, record.consumed_steps);
    }

    /// The facts-only Nemo seam materializes a value-inventing (existential-rule)
    /// program the provenance-carrying seam cannot: [`run_nemo_forward`] hard-errors on
    /// its invented labeled nulls ("no trace tree for derived fact"), while
    /// [`run_nemo_forward_facts_only`] returns a non-empty, deterministic closure. This
    /// is the seam the benchmark harness drives the existential fragment's Nemo side
    /// through.
    #[test]
    fn nemo_facts_only_materializes_an_existential_program() {
        // `e1(?x, !y, ?w) :- edge(?x, ?o, ?w) .` — a single existential (value-inventing)
        // TGD over the ternary forward surface (`!y` is a fresh labeled null per binding).
        let rules = format!(
            "#[name(\"http://example.org/rules/e1\")]\n\
             <{PATH}>(?x, !y, ?w) :- <{EDGE}>(?x, ?o, ?w) .\n"
        );
        let edb = two_edge_edb();

        // The provenance-carrying forward seam refuses the invented nulls (an
        // architectural fact, not an optional degradation).
        assert!(
            run_nemo_forward(edb.as_ref(), &rules).is_err(),
            "the provenance-carrying Nemo forward seam must refuse invented nulls"
        );

        // The facts-only seam materializes them: a non-empty, deterministic closure.
        let a = run_nemo_forward_facts_only(edb.as_ref(), &rules).expect("facts-only run a");
        let b = run_nemo_forward_facts_only(edb.as_ref(), &rules).expect("facts-only run b");
        assert!(
            !a.is_empty(),
            "the facts-only seam must materialize a non-empty existential closure"
        );
        assert_eq!(
            a, b,
            "the facts-only row set must be deterministic across runs"
        );

        // Each input edge invents one `path` head, so a derived `path` row is present.
        assert!(
            a.rows.iter().any(|r| r.predicate == PATH),
            "the facts-only closure must carry the invented existential head `path`"
        );
    }
}
