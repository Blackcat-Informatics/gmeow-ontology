// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Stratum-C counterfactual world construction.
//!
//! This is the **only generative, budgeted, possibly-incomplete** stratum of the
//! logic engine. When a query carries a [`crate::query_ir::QCounterfactual`]
//! declaration, `construct_and_resolve` performs the Phase-3 protocol from
//! `LOGIC-RUNTIME.md`:
//!
//! 1. **Minimal AGM revision.** Admit the antecedent `A` into a copy of the base
//!    world. `A` is a set of ground facts that overwrite functional slots
//!    `(subject, predicate)`: admitting `p(s, o)` retracts every base fact
//!    `p(s, o')` with `o' ≠ o`. When `A` is *internally over-determined* — two
//!    `assume(p(s, ·))` atoms claim different values for one slot — the
//!    **most-entrenched** value wins (read by [`crate::entrenchment`]); an
//!    incomparable maximum is a **genuine tie** and the whole construction returns
//!    `CfStatus::Unknown` rather than branching.
//! 2. **Transient, isolated construction.** Seed a *fresh* named graph `W_cf` with
//!    the revised facts. The base graph is never mutated, so paraconsistency is
//!    preserved and nothing leaks back into the base store.
//! 3. **Scoped resolution.** Resolve the consequent `φ` inside `W_cf` via the v4
//!    dispatcher (the program's Horn rules applied over the revised EDB).
//! 4. **Memoize or dispose.** Key the constructed world by the six-tuple in
//!    [`crate::versioning::counterfactual_world_key`]; an identical key reuses the
//!    cached answer instead of reconstructing.
//!
//! Nested counterfactuals are bounded by a **depth budget**: a request that would
//! recurse past the budget degrades to `CfStatus::Incomplete` rather than
//! constructing without bound.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use crate::dispatch::dispatch_query;
use crate::entrenchment::{Entrenchment, LeastEntrenched};
use crate::physical::IncrementalQuerySession;
use crate::query_ir::{Binding, Budget, QAtom, QProgram, QTerm};
use crate::result::ReasoningResult;
use crate::seam::{BudgetStatus, WorldFactSnapshot};
use crate::store::WorldStore;
use crate::versioning::{CounterfactualKeyInputs, counterfactual_world_key};

/// Wrap a counterfactual-construction condition message as a typed diagnostic on
/// the shared substrate, preserving the authored text verbatim.
fn counterfactual_err(detail: String) -> gmeow_errors::Diag {
    gmeow_errors::Diag::of_kind(crate::error::Counterfactual { detail })
}

/// Default hard cap on nested-counterfactual depth when a query does not declare
/// its own `depth_budget(N)`.
pub const DEFAULT_DEPTH_BUDGET: u64 = 4;

/// Hard cap on the number of closest worlds an opt-in Lewis multi-world profile
/// will enumerate. A closest-world set larger than this degrades to
/// [`CfStatus::Incomplete`] rather than branching without bound.
pub const DEFAULT_BRANCH_BUDGET: u64 = 16;

/// Native solver version stamped into the counterfactual cache key. Any native
/// engine change invalidates cached counterfactual worlds.
pub const SOLVER_VERSION: &str = concat!("gmeow-logic/", env!("CARGO_PKG_VERSION"), "+native");

/// Status of a counterfactual resolution. A superset of [`BudgetStatus`] that adds
/// the two Stratum-C-only outcomes.
///
/// This is the **engine-internal** computation/aggregation enum: the
/// per-world resolution and the `worst_status` Lewis fold track outcomes in this
/// ordered 5-way form. The *public* answer status is the typed
/// [`crate::result::ReasoningResult`] on [`CfAnswer`], folded from this via
/// [`cf_result`]; the conformance corpus's byte-pinned `status` string projects
/// back from the typed result via [`cf_status_string`] (a Principle-17 surface).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CfStatus {
    /// Construction and resolution completed within budget.
    Ok,
    /// The answer cap was hit during resolution.
    Partial,
    /// The inference budget was exhausted during resolution.
    Exhausted,
    /// The revision was genuinely ambiguous (an incomparable entrenchment tie);
    /// the engine declines to branch and reports `unknown`.
    Unknown,
    /// The nested-counterfactual depth budget was exhausted before construction.
    Incomplete,
}

impl CfStatus {
    /// Canonical lowercase serialization (the historical conformance answer string).
    /// Retained only for the [`cf_status_string`] round-trip cross-check (the public
    /// status now projects from the typed result).
    #[cfg(test)]
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            CfStatus::Ok => "ok",
            CfStatus::Partial => "partial",
            CfStatus::Exhausted => "exhausted",
            CfStatus::Unknown => "unknown",
            CfStatus::Incomplete => "incomplete",
        }
    }

    fn from_budget(b: BudgetStatus) -> Self {
        match b {
            BudgetStatus::Ok => CfStatus::Ok,
            BudgetStatus::Partial => CfStatus::Partial,
            BudgetStatus::Exhausted => CfStatus::Exhausted,
        }
    }
}

/// Fold the engine-internal [`CfStatus`] into the typed shared
/// [`crate::result::ReasoningResult`] — the canonical answer status. The
/// `BudgetLimit` discriminator keeps the fold lossless: `partial`/`exhausted`/
/// `incomplete` all map to budget exhaustion of *different* budgets and are
/// recovered exactly by [`cf_status_string`].
///
/// `payload` carries the goal-variable bindings for the resolution path or an
/// empty [`ResultPayload::Bindings`] for refusal/budget paths — never
/// [`ResultPayload::Empty`], so the typed model is fully lossless.
/// `projection_class` mirrors `preservation` (same idiom as result.rs:821/:883).
fn cf_result(
    status: CfStatus,
    world: &str,
    payload: crate::result::ResultPayload,
) -> ReasoningResult {
    use crate::result::{
        BudgetLimit, CompletenessStatus, EvaluationStatus, InformationState, InputStatus,
        PreservationClaim, ResultProvenance,
    };
    let bindings_present =
        matches!(&payload, crate::result::ResultPayload::Bindings(b) if !b.is_empty());
    let (evaluation, completeness, limit, information) = match status {
        // A completed run, complete for the certified fragment: the goal is
        // supported when answers were found, conclusively absent otherwise.
        CfStatus::Ok => (
            EvaluationStatus::Completed,
            CompletenessStatus::CompleteForFragment,
            None,
            if bindings_present {
                InformationState::Supported
            } else {
                InformationState::Neither
            },
        ),
        // Answer cap hit: the run finished but capped its output.
        CfStatus::Partial => (
            EvaluationStatus::Completed,
            CompletenessStatus::Incomplete,
            Some(BudgetLimit::Answers),
            InformationState::Undetermined,
        ),
        // Inference budget exhausted.
        CfStatus::Exhausted => (
            EvaluationStatus::BudgetExhausted,
            CompletenessStatus::Incomplete,
            Some(BudgetLimit::Inference),
            InformationState::Undetermined,
        ),
        // Nested-construction depth budget exhausted.
        CfStatus::Incomplete => (
            EvaluationStatus::BudgetExhausted,
            CompletenessStatus::Incomplete,
            Some(BudgetLimit::Depth),
            InformationState::Undetermined,
        ),
        // A genuine incomparable-entrenchment revision tie: completeness is not a
        // defined question, and the engine reaches no verdict.
        CfStatus::Unknown => (
            EvaluationStatus::Completed,
            CompletenessStatus::Unknown,
            None,
            InformationState::Undetermined,
        ),
    };
    let preservation = PreservationClaim::exact();
    let mut provenance = ResultProvenance::native(SOLVER_VERSION, world);
    provenance.consumed_budget.limit = limit;
    provenance.projection_class = preservation.clone();
    ReasoningResult::new(
        InputStatus::Valid,
        evaluation,
        completeness,
        preservation,
        information,
        provenance,
        payload,
    )
}

/// Project a counterfactual [`ReasoningResult`] back to the byte-pinned
/// conformance answer string (`ok`/`partial`/`exhausted`/`unknown`/`incomplete`).
/// The lossless inverse of [`cf_result`] (round-trip cross-checked in the tests),
/// so the cross-engine corpus is unchanged.
pub fn cf_status_string(result: &ReasoningResult) -> &'static str {
    use crate::result::{BudgetLimit, CompletenessStatus, EvaluationStatus};
    // A revision tie surfaces as completeness=unknown.
    if result.completeness == CompletenessStatus::Unknown {
        return "unknown";
    }
    match (result.evaluation, result.provenance.consumed_budget.limit) {
        (EvaluationStatus::Completed, None) => "ok",
        (EvaluationStatus::Completed, Some(BudgetLimit::Answers)) => "partial",
        (EvaluationStatus::BudgetExhausted, Some(BudgetLimit::Inference)) => "exhausted",
        (EvaluationStatus::BudgetExhausted, Some(BudgetLimit::Depth)) => "incomplete",
        // No other (evaluation, limit) pair is produced by cf_result.
        _ => "ok",
    }
}

/// The result of resolving a counterfactual query.
#[derive(Debug, Clone, PartialEq)]
pub struct CfAnswer {
    /// Goal-variable bindings (empty for `unknown`/`incomplete`/no-match).
    pub bindings: Vec<Binding>,
    /// The typed shared result status — the canonical answer status. The
    /// historical string projects from it via [`cf_status_string`].
    pub result: ReasoningResult,
    /// The constructed world IRI `W_cf` (bare IRI), for provenance/inspection.
    pub cf_world: String,
}

impl CfAnswer {
    /// The byte-pinned conformance status string for this answer.
    pub fn status_str(&self) -> &'static str {
        cf_status_string(&self.result)
    }
}

/// Content-addressed cache of constructed counterfactual worlds, with hit/miss
/// counters for observability. Keyed by [`counterfactual_world_key`]'s six-tuple,
/// so an identical `(base, antecedent, rules, entrenchment, profile, solver)`
/// reuses the prior answer.
#[derive(Debug, Default)]
pub struct CfCache {
    entries: HashMap<String, CfAnswer>,
    incremental_sessions: HashMap<String, IncrementalQuerySession>,
    incremental_ineligible: BTreeSet<String>,
    hits: u64,
    misses: u64,
    incremental_updates: u64,
}

impl CfCache {
    /// A fresh, empty cache.
    pub fn new() -> Self {
        Self::default()
    }
    /// Number of cache hits observed (test/inspection aid).
    pub fn hits(&self) -> u64 {
        self.hits
    }
    /// Number of cache misses observed (test/inspection aid).
    pub fn misses(&self) -> u64 {
        self.misses
    }
    /// Number of counterfactual worlds resolved by applying a signed revision to a
    /// cached fixed-program base session instead of rebuilding the least model.
    pub fn incremental_updates(&self) -> u64 {
        self.incremental_updates
    }
}

/// Return `true` iff `program` is a Stratum-C counterfactual query that must be
/// routed through [`construct_and_resolve`] rather than the plain v4 dispatcher.
pub fn is_counterfactual(program: &QProgram) -> bool {
    program.counterfactual.is_some()
}

/// Construct the counterfactual world declared by `program` and resolve its goal
/// inside it, using a fresh cache. Convenience wrapper over
/// [`construct_and_resolve_cached`].
///
/// `declared_row_schema`: when `Some`, the result's bindings are validated against
/// the caller's declared schema and the schema is attached via
/// [`crate::result::ReasoningResult::with_declared_row_schema`]. When `None`,
/// behaviour is unchanged.
pub fn construct_and_resolve(
    store: &WorldStore,
    program: &QProgram,
    profile: &str,
    budget: &Budget,
    depth: u64,
    declared_row_schema: Option<crate::result_shape::ResultShape>,
) -> gmeow_errors::Result<CfAnswer> {
    let mut cache = CfCache::new();
    construct_and_resolve_cached(
        store,
        program,
        profile,
        budget,
        depth,
        &mut cache,
        declared_row_schema,
    )
}

/// Construct + resolve with an explicit memoization `cache` (shared across nested
/// constructions so repeated identical worlds are built once).
///
/// `declared_row_schema`: when `Some`, the result's bindings are validated against
/// the caller's declared schema and the schema is attached via
/// [`crate::result::ReasoningResult::with_declared_row_schema`] as a post-step
/// (after any cache lookup, so caching is unaffected). When `None`, behaviour is
/// unchanged.
///
/// # Errors
///
/// Returns `Err(String)` on a malformed declaration, an invalid world IRI, or an
/// engine error, including a [`crate::result_shape::ContractViolation`] when the
/// declared schema does not match the result bindings. A genuine revision tie or a
/// depth-budget trip are **not** errors: they are reported as
/// [`CfStatus::Unknown`] / [`CfStatus::Incomplete`].
pub fn construct_and_resolve_cached(
    store: &WorldStore,
    program: &QProgram,
    profile: &str,
    budget: &Budget,
    depth: u64,
    cache: &mut CfCache,
    declared_row_schema: Option<crate::result_shape::ResultShape>,
) -> gmeow_errors::Result<CfAnswer> {
    let cf = program.counterfactual.as_ref().ok_or_else(|| {
        counterfactual_err(
            "construct_and_resolve called on a non-counterfactual program".to_owned(),
        )
    })?;

    let cf_world = strip_brackets(&cf.cf_world);
    let base_world = strip_brackets(&cf.base_world);

    // Depth budget: a request past the budget is incomplete, never unbounded.
    if depth == 0 {
        let result = cf_result(
            CfStatus::Incomplete,
            &cf_world,
            crate::result::ResultPayload::Bindings(vec![]),
        );
        return apply_schema(
            CfAnswer {
                bindings: vec![],
                result,
                cf_world,
            },
            declared_row_schema,
        );
    }

    // (1) Read the entrenchment ordering and the base EDB.
    let entrench = Entrenchment::read_from_world(store, &base_world)?;
    let base_facts = base_world_facts(store, &base_world);

    // (2) Resolve the antecedent into per-slot maximal admissible values.
    //     A unique maximum is the deterministic choice; an incomparable maximum
    //     leaves several values — a genuine tie that the two profiles treat
    //     differently (deterministic → unknown; Lewis → one closest world each).
    let choices = slot_choices(&cf.antecedent, &entrench)?;
    // Saturating product: an over-determined antecedent (many multi-valued slots)
    // must not panic (debug) or wrap (release) before the budget comparison below.
    // The exact magnitude past the budget is irrelevant — saturating to u64::MAX
    // preserves every `> 1` / `> DEFAULT_BRANCH_BUDGET` decision.
    let world_count: u64 = choices
        .iter()
        .map(|c| c.values.len() as u64)
        .try_fold(1u64, |acc, x| acc.checked_mul(x))
        .unwrap_or(u64::MAX);
    let lewis = crate::profile_gate::lewis_mode(profile);

    // (3) Compute the cache key over the exact inputs that determine the world(s).
    let key = counterfactual_world_key(&CounterfactualKeyInputs {
        base_world_hash: hash_facts(&base_facts),
        antecedent_hash: hash_choices(&choices),
        rule_set_hash: hash_rules(program),
        entrenchment_hash: entrench.hash(),
        profile: profile.to_owned(),
        solver_version: SOLVER_VERSION.to_owned(),
    });
    if let Some(cached) = cache.entries.get(&key) {
        cache.hits += 1;
        return apply_schema(cached.clone(), declared_row_schema);
    }
    cache.misses += 1;

    // (4) Profile-specific handling of an over-determined revision.
    let early = if lewis.is_none() && world_count > 1 {
        // Deterministic revision: a non-unique closest world is a genuine tie.
        // The engine declines to branch and reports unknown.
        Some(CfStatus::Unknown)
    } else if lewis.is_some() && world_count > DEFAULT_BRANCH_BUDGET {
        // Lewis multi-world: a closest-world set past the hard branch budget
        // degrades to incomplete rather than enumerating without bound.
        Some(CfStatus::Incomplete)
    } else {
        None
    };
    if let Some(status) = early {
        let result = cf_result(
            status,
            &cf_world,
            crate::result::ResultPayload::Bindings(vec![]),
        );
        let r = CfAnswer {
            bindings: vec![],
            result,
            cf_world,
        };
        cache.entries.insert(key, r.clone());
        return apply_schema(r, declared_row_schema);
    }

    // (5) Build each closest world (the cartesian product of per-slot choices) as
    //     a *fresh* isolated named graph and resolve φ inside it. A single
    //     deterministic world uses W_cf directly; Lewis branches get per-branch
    //     graph IRIs. The base graph is never touched, so nothing leaks.
    let worlds = cartesian(&choices);
    let incremental_base = incremental_base_session(cache, &base_facts, program, profile, budget)?;
    let mut per_world: Vec<(BTreeSet<Binding>, CfStatus)> = Vec::with_capacity(worlds.len());
    for (i, admitted) in worlds.iter().enumerate() {
        let world_iri = if worlds.len() == 1 {
            cf_world.clone()
        } else {
            format!("{cf_world}/lewis/{i}")
        };
        per_world.push(resolve_in_world(
            &base_facts,
            admitted,
            program,
            profile,
            budget,
            &world_iri,
            incremental_base.as_ref(),
        )?);
        if incremental_base.is_some() {
            cache.incremental_updates =
                cache.incremental_updates.checked_add(1).ok_or_else(|| {
                    counterfactual_err(
                        "counterfactual incremental-update counter overflow".to_owned(),
                    )
                })?;
        }
    }

    let result = match lewis {
        None => {
            let (bindings, status) = per_world
                .into_iter()
                .next()
                .expect("deterministic revision constructs exactly one world");
            let bindings: Vec<Binding> = bindings.into_iter().collect();
            let result = cf_result(
                status,
                &cf_world,
                crate::result::ResultPayload::Bindings(bindings.clone()),
            );
            CfAnswer {
                bindings,
                result,
                cf_world,
            }
        }
        Some(mode) => combine_lewis(mode, per_world, cf_world),
    };
    cache.entries.insert(key, result.clone());
    apply_schema(result, declared_row_schema)
}

/// Apply an optional caller-declared `schema` to the `CfAnswer` as a post-step.
/// The cache always stores the bare result; validation is the caller's declared
/// contract applied after retrieval, so caching is unaffected.
fn apply_schema(
    mut answer: CfAnswer,
    schema: Option<crate::result_shape::ResultShape>,
) -> gmeow_errors::Result<CfAnswer> {
    if let Some(s) = schema {
        answer.result = answer
            .result
            .with_declared_row_schema(s)
            .map_err(|v| counterfactual_err(v.to_string()))?;
    }
    Ok(answer)
}

// ── Antecedent resolution ────────────────────────────────────────────────────

/// One functional slot's admissible values after entrenchment arbitration.
#[derive(Debug, Clone, PartialEq, Eq)]
struct SlotChoice {
    subject: String,
    predicate: String,
    /// The maximal admissible values: exactly one when the slot has a unique
    /// most-entrenched value; the incomparable maxima (≥2) on a genuine tie.
    values: Vec<String>,
}

/// Resolve the antecedent atoms into per-slot maximal admissible values.
///
/// Atoms are grouped by `(subject, predicate)`. A slot with a single value admits
/// it. A slot with several distinct values is internally over-determined; the
/// **most-entrenched** value(s) are kept — one when comparable, the incomparable
/// maxima when not (a genuine tie surfaced to the caller as a multi-value slot).
fn slot_choices(
    antecedent: &[QAtom],
    entrench: &Entrenchment,
) -> gmeow_errors::Result<Vec<SlotChoice>> {
    let mut slots: BTreeMap<(String, String), BTreeSet<String>> = BTreeMap::new();
    for atom in antecedent {
        let s = const_iri(&atom.args[0]).ok_or_else(|| {
            counterfactual_err(format!(
                "antecedent subject must be a ground IRI: {:?}",
                atom.pred
            ))
        })?;
        let o = const_iri(&atom.args[1]).ok_or_else(|| {
            counterfactual_err(format!(
                "antecedent object must be a ground IRI: {:?}",
                atom.pred
            ))
        })?;
        slots.entry((s, atom.pred.clone())).or_default().insert(o);
    }

    let mut choices: Vec<SlotChoice> = Vec::new();
    for ((s, p), values) in slots {
        let vals: Vec<String> = values.into_iter().collect();
        let maximal = match vals.as_slice() {
            [single] => vec![single.clone()],
            _ => match entrench.most_entrenched(&vals) {
                LeastEntrenched::Unique(v) => vec![v],
                LeastEntrenched::Tie(t) => t,
                LeastEntrenched::Empty => {
                    return Err(counterfactual_err(
                        "internal: empty antecedent slot".to_owned(),
                    ));
                }
            },
        };
        choices.push(SlotChoice {
            subject: s,
            predicate: p,
            values: maximal,
        });
    }
    Ok(choices)
}

/// Enumerate the cartesian product of per-slot choices into one admitted fact-set
/// per closest world. With no over-determination this is a single set; an empty
/// antecedent yields one empty set (a world identical to the base).
fn cartesian(choices: &[SlotChoice]) -> Vec<Vec<(String, String, String)>> {
    let mut worlds: Vec<Vec<(String, String, String)>> = vec![vec![]];
    for sc in choices {
        let mut next: Vec<Vec<(String, String, String)>> = Vec::new();
        for prefix in &worlds {
            for v in &sc.values {
                let mut extended = prefix.clone();
                extended.push((sc.subject.clone(), sc.predicate.clone(), v.clone()));
                next.push(extended);
            }
        }
        worlds = next;
    }
    for w in &mut worlds {
        w.sort();
    }
    worlds
}

/// One canonical functional-slot revision shared by the incremental and scratch
/// execution branches. The base partition is computed once, so the two paths cannot
/// drift on which facts an admitted `(subject, predicate)` slot overwrites.
struct FunctionalRevision<'a> {
    retained_base: Vec<&'a (String, String, String)>,
    overwritten_base: Vec<&'a (String, String, String)>,
}

fn plan_functional_revision<'a>(
    base_facts: &'a [(String, String, String)],
    admitted: &[(String, String, String)],
) -> FunctionalRevision<'a> {
    let admitted_slots: BTreeSet<(&str, &str)> = admitted
        .iter()
        .map(|(subject, predicate, _)| (subject.as_str(), predicate.as_str()))
        .collect();
    let mut retained_base = Vec::new();
    let mut overwritten_base = Vec::new();
    for fact @ (subject, predicate, _) in base_facts {
        if admitted_slots.contains(&(subject.as_str(), predicate.as_str())) {
            overwritten_base.push(fact);
        } else {
            retained_base.push(fact);
        }
    }
    FunctionalRevision {
        retained_base,
        overwritten_base,
    }
}

/// Build one isolated world `world_iri` from `base_facts` with the `admitted`
/// slots overwritten, then resolve `program`'s goal inside it. Returns the
/// deduplicated binding set and the resolution status.
fn resolve_in_world(
    base_facts: &[(String, String, String)],
    admitted: &[(String, String, String)],
    program: &QProgram,
    profile: &str,
    budget: &Budget,
    world_iri: &str,
    incremental_base: Option<&IncrementalQuerySession>,
) -> gmeow_errors::Result<(BTreeSet<Binding>, CfStatus)> {
    let revision = plan_functional_revision(base_facts, admitted);
    if let Some(incremental_base) = incremental_base {
        let mut changes = Vec::new();
        for (subject, predicate, object) in revision.overwritten_base {
            changes.push((subject.clone(), predicate.clone(), object.clone(), -1));
        }
        changes.extend(admitted.iter().map(|(subject, predicate, object)| {
            (subject.clone(), predicate.clone(), object.clone(), 1)
        }));

        let mut session = incremental_base.clone();
        let answer = session.apply_iri_changes(changes, budget.max_answers)?;
        return Ok((
            answer.bindings.into_iter().collect(),
            CfStatus::from_budget(answer.status),
        ));
    }

    let cf_store = WorldStore::new();
    for (s, p, o) in revision.retained_base {
        cf_store.insert_quad(world_iri, s, p, o);
    }
    for (s, p, o) in admitted {
        cf_store.insert_quad(world_iri, s, p, o);
    }

    let foreign = WorldFactSnapshot::from_world(&cf_store, world_iri, profile)?;
    let answer = dispatch_query(&foreign, world_iri, program, profile, budget)?;
    Ok((
        answer.bindings.into_iter().collect(),
        CfStatus::from_budget(answer.status),
    ))
}

/// Fetch or build the fixed-program base session shared by counterfactual revisions.
///
/// The cache key excludes the antecedent on purpose: every antecedent is a signed
/// transaction over the same base/rules/goal state.  It includes the exact base facts,
/// rule set, goal, profile, and solver version, so reuse never crosses a contract seam.
fn incremental_base_session(
    cache: &mut CfCache,
    base_facts: &[(String, String, String)],
    program: &QProgram,
    profile: &str,
    budget: &Budget,
) -> gmeow_errors::Result<Option<IncrementalQuerySession>> {
    if budget.max_steps.is_some() {
        return Ok(None);
    }
    let key = incremental_base_key(base_facts, program, profile);
    if cache.incremental_ineligible.contains(&key) {
        return Ok(None);
    }
    if let Some(session) = cache.incremental_sessions.get(&key) {
        return Ok(Some(session.clone()));
    }

    const BASE: &str = "urn:gmeow:counterfactual-incremental-base";
    let base_store = WorldStore::new();
    for (subject, predicate, object) in base_facts {
        base_store.insert_quad(BASE, subject, predicate, object);
    }
    let foreign = WorldFactSnapshot::from_world(&base_store, BASE, profile)?;
    match crate::physical::prepare_incremental_query(&foreign, BASE, program, &key, budget)? {
        Some(session) => {
            cache.incremental_sessions.insert(key, session.clone());
            Ok(Some(session))
        }
        None => {
            cache.incremental_ineligible.insert(key);
            Ok(None)
        }
    }
}

fn incremental_base_key(
    base_facts: &[(String, String, String)],
    program: &QProgram,
    profile: &str,
) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"gmeow-counterfactual-incremental-base-v1\n");
    hasher.update(&hash_facts(base_facts));
    hasher.update(&hash_rules(program));
    hasher.update(&hash_goal(&program.goal));
    hasher.update(crate::profile_gate::canonical_profile_identity(profile).as_bytes());
    hasher.update(b"\n");
    hasher.update(SOLVER_VERSION.as_bytes());
    hasher.finalize().to_hex().to_string()
}

/// BLAKE3 of the typed goal structure. Every field is length-framed and every term
/// variant is domain-tagged, so cache identity is independent of `Debug` rendering
/// and cannot confuse a variable, constant, or number with the same surface text.
fn hash_goal(goal: &crate::query_ir::QGoal) -> [u8; 32] {
    fn frame(hasher: &mut blake3::Hasher, value: &[u8]) {
        hasher.update(&(value.len() as u64).to_le_bytes());
        hasher.update(value);
    }

    let mut hasher = blake3::Hasher::new();
    frame(&mut hasher, b"gmeow-counterfactual-goal-v1");
    hasher.update(&(goal.atoms.len() as u64).to_le_bytes());
    for atom in &goal.atoms {
        frame(&mut hasher, atom.pred.as_bytes());
        hasher.update(&(atom.args.len() as u64).to_le_bytes());
        for term in &atom.args {
            match term {
                QTerm::Const(value) => {
                    hasher.update(&[0]);
                    frame(&mut hasher, value.as_bytes());
                }
                QTerm::Var(value) => {
                    hasher.update(&[1]);
                    frame(&mut hasher, value.as_bytes());
                }
                QTerm::Num(value) => {
                    hasher.update(&[2]);
                    hasher.update(&value.to_le_bytes());
                }
                // A structured (compound) term — a counterfactual over structured goals
                // routes to the full-FOL resolver, so this arm is for exhaustiveness only.
                QTerm::Struct(sn) => {
                    hasher.update(&[3]);
                    hasher.update(&(sn.node().index() as u64).to_le_bytes());
                }
            }
        }
    }
    *hasher.finalize().as_bytes()
}

/// Combine per-closest-world resolutions under a Lewis quantifier:
/// **skeptical** keeps bindings true in *every* closest world (intersection);
/// **credulous** keeps bindings true in *some* world (union). The combined status
/// is the most-degraded per-world status (`ok` only if all worlds resolved `ok`).
fn combine_lewis(
    mode: crate::profile_gate::LewisMode,
    per_world: Vec<(BTreeSet<Binding>, CfStatus)>,
    cf_world: String,
) -> CfAnswer {
    use crate::profile_gate::LewisMode;
    let status = per_world
        .iter()
        .map(|(_, s)| *s)
        .fold(CfStatus::Ok, worst_status);

    let mut iter = per_world.into_iter().map(|(b, _)| b);
    let combined: BTreeSet<Binding> = match iter.next() {
        None => BTreeSet::new(),
        Some(first) => iter.fold(first, |acc, next| match mode {
            LewisMode::Skeptical => acc.intersection(&next).cloned().collect(),
            LewisMode::Credulous => acc.union(&next).cloned().collect(),
        }),
    };

    let bindings: Vec<Binding> = combined.into_iter().collect();
    let result = cf_result(
        status,
        &cf_world,
        crate::result::ResultPayload::Bindings(bindings.clone()),
    );
    CfAnswer {
        bindings,
        result,
        cf_world,
    }
}

/// Pick the more-degraded of two statuses for Lewis status folding.
fn worst_status(a: CfStatus, b: CfStatus) -> CfStatus {
    fn rank(s: CfStatus) -> u8 {
        match s {
            CfStatus::Ok => 0,
            CfStatus::Partial => 1,
            CfStatus::Exhausted => 2,
            CfStatus::Incomplete => 3,
            CfStatus::Unknown => 4,
        }
    }
    if rank(a) >= rank(b) { a } else { b }
}

// ── Helpers ──────────────────────────────────────────────────────────────────

/// Snapshot the base world's IRI-only triples as sorted `(s, p, o)` bare IRIs.
fn base_world_facts(store: &WorldStore, base_world: &str) -> Vec<(String, String, String)> {
    let mut facts: Vec<(String, String, String)> = store
        .quads_for_pattern_in_world(base_world, None, None, None)
        .into_iter()
        .filter_map(|q| {
            let s = q.s.as_iri()?.to_owned();
            let p = q.p.as_iri()?.to_owned();
            let o = q.o.as_iri()?.to_owned();
            Some((s, p, o))
        })
        .collect();
    facts.sort();
    facts.dedup();
    facts
}

/// BLAKE3 of a canonical, sorted serialization of `(s, p, o)` facts.
fn hash_facts(facts: &[(String, String, String)]) -> [u8; 32] {
    let mut sorted = facts.to_vec();
    sorted.sort();
    let mut buf = String::new();
    for (s, p, o) in &sorted {
        buf.push('<');
        buf.push_str(s);
        buf.push_str("> <");
        buf.push_str(p);
        buf.push_str("> <");
        buf.push_str(o);
        buf.push_str(">\n");
    }
    *blake3::hash(buf.as_bytes()).as_bytes()
}

/// BLAKE3 of the per-slot antecedent choices (subject, predicate, sorted values).
/// Captures the full closest-world fan-out so the cache key distinguishes a
/// deterministic single-value antecedent from an over-determined one.
fn hash_choices(choices: &[SlotChoice]) -> [u8; 32] {
    let mut buf = String::new();
    for c in choices {
        buf.push('<');
        buf.push_str(&c.subject);
        buf.push_str("> <");
        buf.push_str(&c.predicate);
        buf.push('>');
        let mut vals = c.values.clone();
        vals.sort();
        for v in &vals {
            buf.push_str(" <");
            buf.push_str(v);
            buf.push('>');
        }
        buf.push('\n');
    }
    *blake3::hash(buf.as_bytes()).as_bytes()
}

/// BLAKE3 of a canonical serialization of the program's Horn rules (the goal and
/// counterfactual directives are excluded — they are keyed separately).
fn hash_rules(program: &QProgram) -> [u8; 32] {
    let mut lines: Vec<String> = program
        .rules
        .iter()
        .map(|r| {
            let head = atom_str(&r.head);
            let body: Vec<String> = r
                .body
                .iter()
                .map(|lit| match lit {
                    crate::query_ir::QBodyLit::Atom(a) => atom_str(a),
                    crate::query_ir::QBodyLit::Neg(a) => format!("\\+ {}", atom_str(a)),
                    crate::query_ir::QBodyLit::Cut => "!".to_owned(),
                    crate::query_ir::QBodyLit::Builtin(b) => builtin_str(b),
                })
                .collect();
            format!("{head} :- {}", body.join(", "))
        })
        .collect();
    lines.sort();
    *blake3::hash(lines.join("\n").as_bytes()).as_bytes()
}

fn atom_str(a: &QAtom) -> String {
    let args: Vec<String> = a
        .args
        .iter()
        .map(|t| match t {
            QTerm::Const(c) => c.clone(),
            QTerm::Var(v) => format!("?{v}"),
            QTerm::Num(n) => n.to_string(),
            QTerm::Struct(sn) => format!("#struct{}", sn.node().index()),
        })
        .collect();
    format!("<{}>({})", a.pred, args.join(", "))
}

/// Canonical text for a `QBuiltin` used only in the rule-hash serialization.
fn builtin_str(b: &crate::query_ir::QBuiltin) -> String {
    use crate::query_ir::QBuiltin;
    fn term(t: &QTerm) -> String {
        match t {
            QTerm::Const(c) => c.clone(),
            QTerm::Var(v) => format!("?{v}"),
            QTerm::Num(n) => n.to_string(),
            QTerm::Struct(sn) => format!("#struct{}", sn.node().index()),
        }
    }
    match b {
        QBuiltin::Is {
            target,
            lhs,
            op,
            rhs,
        } => format!(
            "{} is {} {} {}",
            term(target),
            term(lhs),
            op.token(),
            term(rhs)
        ),
        QBuiltin::Compare { lhs, op, rhs } => {
            format!("{} {} {}", term(lhs), op.token(), term(rhs))
        }
        QBuiltin::BilinearSqDist { target, gram, x, y } => format!(
            "{} is bilinearSqDist({}, {}, {})",
            term(target),
            term(gram),
            term(x),
            term(y)
        ),
    }
}

/// Strip a single pair of angle brackets from a canonical `<iri>` constant.
fn strip_brackets(s: &str) -> String {
    s.strip_prefix('<')
        .and_then(|s| s.strip_suffix('>'))
        .map(|s| s.to_owned())
        .unwrap_or_else(|| s.to_owned())
}

/// Extract a bare IRI string from a ground constant `QTerm` (`<iri>` → `iri`).
fn const_iri(t: &QTerm) -> Option<String> {
    match t {
        QTerm::Const(c) => Some(strip_brackets(c)),
        QTerm::Var(_) | QTerm::Num(_) | QTerm::Struct(_) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entrenchment::OVERRIDES;
    use crate::query_ir::parse_query_program;

    const HORN: &str = "https://blackcatinformatics.ca/logic/PositiveHornProfile";
    const BASE: &str = "http://world/base";
    const CF: &str = "http://world/cf";

    fn plain_program() -> QProgram {
        parse_query_program(
            ":- prefix(ex, 'https://example.org/').\n\
             ?- ex:p(ex:s, Y).\n",
        )
        .unwrap()
    }

    #[test]
    fn incremental_goal_hash_is_typed_and_framed() {
        let goal = |pred: &str, term: QTerm| crate::query_ir::QGoal {
            atoms: vec![QAtom {
                pred: pred.to_owned(),
                args: vec![term],
            }],
        };

        assert_ne!(
            hash_goal(&goal("https://ex/p", QTerm::Const("1".to_owned()))),
            hash_goal(&goal("https://ex/p", QTerm::Var("1".to_owned()))),
        );
        assert_ne!(
            hash_goal(&goal("https://ex/p", QTerm::Const("1".to_owned()))),
            hash_goal(&goal("https://ex/p", QTerm::Num(1))),
        );
        assert_ne!(
            hash_goal(&goal("https://ex/a", QTerm::Const("bc".to_owned()))),
            hash_goal(&goal("https://ex/ab", QTerm::Const("c".to_owned()))),
            "length framing prevents adjacent-field boundary aliases",
        );
    }

    #[test]
    fn incremental_base_key_canonicalizes_profile_aliases() {
        let program = plain_program();
        let facts = vec![(
            "https://ex/s".to_owned(),
            "https://ex/p".to_owned(),
            "https://ex/o".to_owned(),
        )];
        assert_eq!(
            incremental_base_key(&facts, &program, HORN),
            incremental_base_key(&facts, &program, "logic:PositiveHornProfile"),
        );
        assert_eq!(
            incremental_base_key(&facts, &program, HORN),
            incremental_base_key(&facts, &program, "PositiveHornProfile"),
        );
    }

    #[test]
    fn is_counterfactual_detects_declaration() {
        assert!(!is_counterfactual(&plain_program()));
    }

    #[test]
    fn construct_and_resolve_rejects_plain_program() {
        let store = WorldStore::new();
        let err =
            construct_and_resolve(&store, &plain_program(), HORN, &Budget::default(), 4, None)
                .unwrap_err();
        assert!(err.message().contains("non-counterfactual"), "got: {err}");
    }

    // ── AC-1: a counterfactual query yields the expected consequent ───────────
    //
    // Base: status(server, up). Antecedent overwrites it to status(server, down).
    // Rule: alert(X, fired) :- status(X, down). Goal: alert(server, Z) -> {fired}.
    #[test]
    fn consequent_is_yielded_after_overwrite() {
        let store = WorldStore::new();
        store.insert_quad(
            BASE,
            "https://ex/server",
            "https://ex/status",
            "https://ex/up",
        );
        let prog = parse_query_program(
            ":- prefix(ex, 'https://ex/').\n\
             :- counterfactual('http://world/cf', 'http://world/base').\n\
             :- assume(ex:status(ex:server, ex:down)).\n\
             ex:alert(X, ex:fired) :- ex:status(X, ex:down).\n\
             ?- ex:alert(ex:server, Z).\n",
        )
        .unwrap();
        let ans = construct_and_resolve(&store, &prog, HORN, &Budget::default(), 4, None).unwrap();
        assert_eq!(ans.status_str(), "ok", "ans: {ans:?}");
        assert_eq!(ans.bindings.len(), 1, "exactly one consequent: {ans:?}");
        assert_eq!(ans.bindings[0]["Z"], "<https://ex/fired>");
    }

    #[test]
    fn functional_revision_is_identical_for_incremental_and_scratch_paths() {
        let base = vec![
            (
                "https://ex/server".to_owned(),
                "https://ex/status".to_owned(),
                "https://ex/up".to_owned(),
            ),
            (
                "https://ex/other".to_owned(),
                "https://ex/kept".to_owned(),
                "https://ex/value".to_owned(),
            ),
        ];
        let admitted = vec![(
            "https://ex/server".to_owned(),
            "https://ex/status".to_owned(),
            "https://ex/down".to_owned(),
        )];
        let program = parse_query_program(
            ":- prefix(ex, 'https://ex/').\n\
             ?- ex:status(ex:server, Z).\n",
        )
        .unwrap();
        let budget = Budget::default();
        let mut cache = CfCache::new();
        let incremental = incremental_base_session(&mut cache, &base, &program, HORN, &budget)
            .unwrap()
            .expect("positive query admits a fixed-program incremental session");

        let scratch = resolve_in_world(&base, &admitted, &program, HORN, &budget, CF, None)
            .expect("scratch revision");
        let maintained = resolve_in_world(
            &base,
            &admitted,
            &program,
            HORN,
            &budget,
            CF,
            Some(&incremental),
        )
        .expect("incremental revision");

        assert_eq!(maintained, scratch);
        assert_eq!(
            maintained.0,
            BTreeSet::from([BTreeMap::from([(
                "Z".to_owned(),
                "<https://ex/down>".to_owned(),
            )])])
        );
    }

    // ── Native production path: recursion resolves inside the constructed world ─
    //
    // Each closest world's goal is resolved via `dispatch_query` (native magic-sets
    // first), so a counterfactual whose consequent needs RECURSION exercises the
    // promoted native path end-to-end on the counterfactual production surface: the
    // assumed edge a→b joins the base chain b→c→d, so `reach(a, Y)` closes over
    // {b, c, d} inside the constructed world. The reference comparison for this
    // fragment lives in `physical::parity`.
    #[test]
    fn counterfactual_native_resolves_recursion_in_constructed_world() {
        let store = WorldStore::new();
        store.insert_quad(BASE, "https://ex/b", "https://ex/edge", "https://ex/c");
        store.insert_quad(BASE, "https://ex/c", "https://ex/edge", "https://ex/d");
        let prog = parse_query_program(
            ":- prefix(ex, 'https://ex/').\n\
             :- counterfactual('http://world/cf', 'http://world/base').\n\
             :- assume(ex:edge(ex:a, ex:b)).\n\
             ex:reach(X, Y) :- ex:edge(X, Y).\n\
             ex:reach(X, Y) :- ex:edge(X, Z), ex:reach(Z, Y).\n\
             ?- ex:reach(ex:a, Y).\n",
        )
        .unwrap();
        let mut cache = CfCache::new();
        let ans = construct_and_resolve_cached(
            &store,
            &prog,
            HORN,
            &Budget::default(),
            4,
            &mut cache,
            None,
        )
        .unwrap();
        assert_eq!(ans.status_str(), "ok", "ans: {ans:?}");
        let zs: BTreeSet<&str> = ans.bindings.iter().map(|b| b["Y"].as_str()).collect();
        assert_eq!(
            zs,
            BTreeSet::from(["<https://ex/b>", "<https://ex/c>", "<https://ex/d>"]),
            "native recursion inside the constructed counterfactual world: {ans:?}"
        );
        assert_eq!(
            cache.incremental_updates(),
            1,
            "the recursive counterfactual must apply one signed revision to the cached base"
        );
    }

    // ── AC-2: no leakage — the base store is never mutated ────────────────────
    #[test]
    fn no_leakage_base_store_unchanged() {
        let store = WorldStore::new();
        store.insert_quad(
            BASE,
            "https://ex/server",
            "https://ex/status",
            "https://ex/up",
        );
        let before = store.quads_in_world(BASE);
        let prog = parse_query_program(
            ":- prefix(ex, 'https://ex/').\n\
             :- counterfactual('http://world/cf', 'http://world/base').\n\
             :- assume(ex:status(ex:server, ex:down)).\n\
             ?- ex:status(ex:server, Z).\n",
        )
        .unwrap();
        let _ = construct_and_resolve(&store, &prog, HORN, &Budget::default(), 4, None).unwrap();
        // The base world still has exactly its original fact (status up), and the
        // constructed world W_cf never appears in the base store.
        let after = store.quads_in_world(BASE);
        assert_eq!(before, after, "base world must be unchanged");
        assert!(
            !store.worlds().contains(&CF.to_owned()),
            "W_cf must not leak into the base store: {:?}",
            store.worlds()
        );
        // And inside W_cf the antecedent value holds, not the base value.
        let prog2 = parse_query_program(
            ":- prefix(ex, 'https://ex/').\n\
             :- counterfactual('http://world/cf', 'http://world/base').\n\
             :- assume(ex:status(ex:server, ex:down)).\n\
             ?- ex:status(ex:server, Z).\n",
        )
        .unwrap();
        let ans = construct_and_resolve(&store, &prog2, HORN, &Budget::default(), 4, None).unwrap();
        assert_eq!(ans.bindings.len(), 1);
        assert_eq!(
            ans.bindings[0]["Z"], "<https://ex/down>",
            "overwrite applied in W_cf"
        );
    }

    // ── AC-3a: deterministic revision yields exactly one world ────────────────
    //
    // Over-determined antecedent {primary, backup} with primary ≻ backup -> primary wins.
    #[test]
    fn comparable_over_determination_is_deterministic() {
        let store = WorldStore::new();
        store.insert_quad(BASE, "https://ex/primary", OVERRIDES, "https://ex/backup");
        let prog = parse_query_program(
            ":- prefix(ex, 'https://ex/').\n\
             :- counterfactual('http://world/cf', 'http://world/base').\n\
             :- assume(ex:route(ex:traffic, ex:primary)).\n\
             :- assume(ex:route(ex:traffic, ex:backup)).\n\
             ?- ex:route(ex:traffic, Z).\n",
        )
        .unwrap();
        let ans = construct_and_resolve(&store, &prog, HORN, &Budget::default(), 4, None).unwrap();
        assert_eq!(ans.status_str(), "ok");
        assert_eq!(ans.bindings.len(), 1, "exactly one routed value: {ans:?}");
        assert_eq!(
            ans.bindings[0]["Z"], "<https://ex/primary>",
            "the more-entrenched value wins"
        );
    }

    // ── AC-3b: a genuine (incomparable) tie returns unknown ───────────────────
    #[test]
    fn incomparable_over_determination_is_unknown() {
        // No entrenchment edge between blue and green -> incomparable.
        let store = WorldStore::new();
        store.insert_quad(BASE, "https://ex/seed", "https://ex/p", "https://ex/o");
        let prog = parse_query_program(
            ":- prefix(ex, 'https://ex/').\n\
             :- counterfactual('http://world/cf', 'http://world/base').\n\
             :- assume(ex:flag(ex:x, ex:blue)).\n\
             :- assume(ex:flag(ex:x, ex:green)).\n\
             ?- ex:flag(ex:x, Z).\n",
        )
        .unwrap();
        let ans = construct_and_resolve(&store, &prog, HORN, &Budget::default(), 4, None).unwrap();
        assert_eq!(ans.status_str(), "unknown", "ambiguous tie must be unknown");
        assert!(ans.bindings.is_empty());
    }

    // ── depth budget trip ─────────────────────────────────────────────────────
    #[test]
    fn depth_budget_zero_is_incomplete() {
        let store = WorldStore::new();
        store.insert_quad(BASE, "https://ex/s", "https://ex/p", "https://ex/o");
        let prog = parse_query_program(
            ":- prefix(ex, 'https://ex/').\n\
             :- counterfactual('http://world/cf', 'http://world/base').\n\
             :- assume(ex:p2(ex:s, ex:o2)).\n\
             ?- ex:p(ex:s, Z).\n",
        )
        .unwrap();
        let ans = construct_and_resolve(&store, &prog, HORN, &Budget::default(), 0, None).unwrap();
        assert_eq!(ans.status_str(), "incomplete");
    }

    // ── memoization: identical key -> cache hit, identical answer ─────────────
    #[test]
    fn memoization_hit_on_identical_construction() {
        let store = WorldStore::new();
        store.insert_quad(
            BASE,
            "https://ex/server",
            "https://ex/status",
            "https://ex/up",
        );
        let prog = parse_query_program(
            ":- prefix(ex, 'https://ex/').\n\
             :- counterfactual('http://world/cf', 'http://world/base').\n\
             :- assume(ex:status(ex:server, ex:down)).\n\
             ?- ex:status(ex:server, Z).\n",
        )
        .unwrap();
        let mut cache = CfCache::new();
        let a = construct_and_resolve_cached(
            &store,
            &prog,
            HORN,
            &Budget::default(),
            4,
            &mut cache,
            None,
        )
        .unwrap();
        let b = construct_and_resolve_cached(
            &store,
            &prog,
            HORN,
            &Budget::default(),
            4,
            &mut cache,
            None,
        )
        .unwrap();
        assert_eq!(a, b, "identical construction must yield identical answers");
        assert_eq!(cache.misses(), 1, "first call is a miss");
        assert_eq!(cache.hits(), 1, "second identical call is a hit");
    }

    #[test]
    fn cf_status_serialization() {
        assert_eq!(CfStatus::Ok.as_str(), "ok");
        assert_eq!(CfStatus::Unknown.as_str(), "unknown");
        assert_eq!(CfStatus::Incomplete.as_str(), "incomplete");
    }

    #[test]
    fn cf_status_string_round_trips_every_cfstatus() {
        // The typed ReasoningResult is a lossless carrier: projecting it back
        // reproduces the byte-pinned conformance string exactly, so the
        // cross-engine corpus is unchanged.
        for s in [
            CfStatus::Ok,
            CfStatus::Partial,
            CfStatus::Exhausted,
            CfStatus::Unknown,
            CfStatus::Incomplete,
        ] {
            let r = cf_result(
                s,
                "http://gmeow.example/w",
                crate::result::ResultPayload::Bindings(vec![]),
            );
            assert_eq!(cf_status_string(&r), s.as_str(), "cf round-trip for {s:?}");
            assert!(
                r.validate().is_ok(),
                "cf_result must be a valid result: {s:?}"
            );
        }
    }

    #[test]
    fn cf_unknown_is_distinct_typed_state_from_prob_unknown() {
        use crate::result::{CompletenessStatus, EvaluationStatus, InformationState};
        // A cf revision tie: completed run, completeness=unknown, no verdict.
        let r = cf_result(
            CfStatus::Unknown,
            "http://gmeow.example/w",
            crate::result::ResultPayload::Bindings(vec![]),
        );
        assert_eq!(r.evaluation, EvaluationStatus::Completed);
        assert_eq!(r.completeness, CompletenessStatus::Unknown);
        assert_eq!(r.information, InformationState::Undetermined);
        // ...which differs from prob's no-model unknown (unsupported + not-evaluated),
        // even though both project to the same "unknown" corpus string.
        assert_eq!(cf_status_string(&r), "unknown");
    }

    // ── Lewis multi-world profile (opt-in, budget-capped) ─────────────────────

    const LEWIS_SKEPTICAL: &str = "https://blackcatinformatics.ca/logic/LewisSkepticalProfile";
    const LEWIS_CREDULOUS: &str = "https://blackcatinformatics.ca/logic/LewisCredulousProfile";

    fn two_world_program() -> QProgram {
        // {blue, green} are incomparable -> two closest worlds under Lewis.
        parse_query_program(
            ":- prefix(ex, 'https://ex/').\n\
             :- counterfactual('http://world/cf', 'http://world/base').\n\
             :- assume(ex:flag(ex:x, ex:blue)).\n\
             :- assume(ex:flag(ex:x, ex:green)).\n\
             ?- ex:flag(ex:x, Z).\n",
        )
        .unwrap()
    }

    fn seeded_base() -> WorldStore {
        let store = WorldStore::new();
        store.insert_quad(BASE, "https://ex/seed", "https://ex/p", "https://ex/o");
        store
    }

    #[test]
    fn lewis_skeptical_intersects_closest_worlds() {
        let store = seeded_base();
        let ans = construct_and_resolve(
            &store,
            &two_world_program(),
            LEWIS_SKEPTICAL,
            &Budget::default(),
            4,
            None,
        )
        .unwrap();
        assert_eq!(ans.status_str(), "ok");
        // Z=blue holds only in the blue-world, Z=green only in the green-world:
        // the intersection is empty.
        assert!(
            ans.bindings.is_empty(),
            "skeptical: no binding holds in every closest world: {ans:?}"
        );
    }

    #[test]
    fn lewis_credulous_unions_closest_worlds() {
        let store = seeded_base();
        let ans = construct_and_resolve(
            &store,
            &two_world_program(),
            LEWIS_CREDULOUS,
            &Budget::default(),
            4,
            None,
        )
        .unwrap();
        assert_eq!(ans.status_str(), "ok");
        // Union over both closest worlds: Z in {blue, green}.
        let zs: BTreeSet<&str> = ans.bindings.iter().map(|b| b["Z"].as_str()).collect();
        assert_eq!(
            zs,
            BTreeSet::from(["<https://ex/blue>", "<https://ex/green>"]),
            "credulous: union of both closest worlds: {ans:?}"
        );
    }

    #[test]
    fn lewis_branch_budget_trips_to_incomplete() {
        // 5 independent binary-incomparable slots -> 2^5 = 32 closest worlds,
        // past DEFAULT_BRANCH_BUDGET (16) -> Incomplete.
        let store = seeded_base();
        let prog = parse_query_program(
            ":- prefix(ex, 'https://ex/').\n\
             :- counterfactual('http://world/cf', 'http://world/base').\n\
             :- assume(ex:a(ex:s1, ex:v1)).\n\
             :- assume(ex:a(ex:s1, ex:w1)).\n\
             :- assume(ex:a(ex:s2, ex:v2)).\n\
             :- assume(ex:a(ex:s2, ex:w2)).\n\
             :- assume(ex:a(ex:s3, ex:v3)).\n\
             :- assume(ex:a(ex:s3, ex:w3)).\n\
             :- assume(ex:a(ex:s4, ex:v4)).\n\
             :- assume(ex:a(ex:s4, ex:w4)).\n\
             :- assume(ex:a(ex:s5, ex:v5)).\n\
             :- assume(ex:a(ex:s5, ex:w5)).\n\
             ?- ex:a(ex:s1, Z).\n",
        )
        .unwrap();
        let ans =
            construct_and_resolve(&store, &prog, LEWIS_SKEPTICAL, &Budget::default(), 4, None)
                .unwrap();
        assert_eq!(
            ans.status_str(),
            "incomplete",
            "32 worlds exceeds the branch budget"
        );
    }

    #[test]
    fn lewis_does_not_change_deterministic_single_world() {
        // A single-valued antecedent is one world even under a Lewis profile.
        let store = WorldStore::new();
        store.insert_quad(
            BASE,
            "https://ex/server",
            "https://ex/status",
            "https://ex/up",
        );
        let prog = parse_query_program(
            ":- prefix(ex, 'https://ex/').\n\
             :- counterfactual('http://world/cf', 'http://world/base').\n\
             :- assume(ex:status(ex:server, ex:down)).\n\
             ?- ex:status(ex:server, Z).\n",
        )
        .unwrap();
        let ans =
            construct_and_resolve(&store, &prog, LEWIS_CREDULOUS, &Budget::default(), 4, None)
                .unwrap();
        assert_eq!(ans.status_str(), "ok");
        assert_eq!(ans.bindings.len(), 1);
        assert_eq!(ans.bindings[0]["Z"], "<https://ex/down>");
    }

    // ── row_schema facet: declared schema is validated and attached ────────────

    /// A matching schema: the result binds IRI-valued `Z`; the schema declares
    /// `Required Iri` for `Z`. Schema is attached and `row_schema.is_some()`.
    #[test]
    fn declared_schema_matching_attaches_row_schema() {
        use gmeow_logic_compile::result_shape::{
            ColumnBinding, ColumnKind, ResultColumn, ResultShape, RowCardinality,
        };

        let store = WorldStore::new();
        store.insert_quad(
            BASE,
            "https://ex/server",
            "https://ex/status",
            "https://ex/up",
        );
        let prog = parse_query_program(
            ":- prefix(ex, 'https://ex/').\n\
             :- counterfactual('http://world/cf', 'http://world/base').\n\
             :- assume(ex:status(ex:server, ex:down)).\n\
             ?- ex:status(ex:server, Z).\n",
        )
        .unwrap();

        // Declare: one Required IRI column `Z`, any number of rows.
        let schema = ResultShape::new(
            vec![ResultColumn {
                var: "Z".to_owned(),
                kind: ColumnKind::Iri,
                binding: ColumnBinding::Required,
            }],
            RowCardinality::Contains,
        );

        let ans = construct_and_resolve(&store, &prog, HORN, &Budget::default(), 4, Some(schema))
            .unwrap();
        assert_eq!(ans.status_str(), "ok");
        assert_eq!(ans.bindings.len(), 1);
        assert!(
            ans.result.row_schema.is_some(),
            "row_schema must be attached when a declared schema matches"
        );
    }

    /// A mismatching schema: the result binds IRI-valued `Z`; the schema declares
    /// `Required BlankNode` for `Z`. Must return Err (ContractViolation propagated).
    #[test]
    fn declared_schema_mismatch_returns_err() {
        use gmeow_logic_compile::result_shape::{
            ColumnBinding, ColumnKind, ResultColumn, ResultShape, RowCardinality,
        };

        let store = WorldStore::new();
        store.insert_quad(
            BASE,
            "https://ex/server",
            "https://ex/status",
            "https://ex/up",
        );
        let prog = parse_query_program(
            ":- prefix(ex, 'https://ex/').\n\
             :- counterfactual('http://world/cf', 'http://world/base').\n\
             :- assume(ex:status(ex:server, ex:down)).\n\
             ?- ex:status(ex:server, Z).\n",
        )
        .unwrap();

        // Declare: `Z` must be a blank-node — but the binding is an IRI → mismatch.
        let schema = ResultShape::new(
            vec![ResultColumn {
                var: "Z".to_owned(),
                kind: ColumnKind::BlankNode,
                binding: ColumnBinding::Required,
            }],
            RowCardinality::Contains,
        );

        let err = construct_and_resolve(&store, &prog, HORN, &Budget::default(), 4, Some(schema))
            .unwrap_err();
        assert!(
            err.message().contains("result-shape violation"),
            "ContractViolation must be propagated as Err: {err}"
        );
    }
}
