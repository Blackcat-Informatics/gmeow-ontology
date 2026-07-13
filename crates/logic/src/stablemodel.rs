// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Native stable-model / answer-set evaluator.
//!
//! Stable models are enumerated directly on top of the reduct least model in
//! [`crate::rule_ir`]. Per
//! world:
//!
//! 1. **Candidate universe.**  `H = lmr(reference = ∅).store` — the least model of
//!    the program with every NAF literal treated as *absent* (so every rule fires
//!    maximally positively).  `H` upper-bounds every stable model's atom set.  The
//!    candidate atoms are `H \ EDB` (the EDB is forced into every model).
//! 2. **Enumeration.**  For each subset `S ⊆ (H \ EDB)` in canonical bitmask order
//!    over atoms sorted by key, form `M = EDB ∪ S` and keep it iff
//!    `lmr(reference = M).store` has the same key set as `M` — the
//!    Gelfond-Lifschitz stability condition.
//! 3. **Canonical order.**  Each model's atoms are sorted by key; the model list is
//!    sorted by its sorted key vector.
//!
//! [`cautious_materialize`] emits the asserted EDB plus the *cautious* (skeptical)
//! consequences — the intersection of all stable models minus the EDB.  For the
//! Phase-A corpus case (`inSet`/`outSet` even-loop choice) the two stable models
//! `{candidate, inSet}` and `{candidate, outSet}` intersect to EMPTY (modulo EDB),
//! so only the asserted `candidate(x,x)` quad is emitted.
//!
//! [`IncrementalStableModelSession`] is the production multi-shot boundary used by
//! the production materialization router. The low-level model
//! enumerator and scratch materializer remain crate-internal comparators for parity
//! tests, hence the crate-internal `dead_code` allowance.
#![allow(dead_code)]

use std::collections::BTreeSet;
use std::sync::Arc;

use crate::rule_ir::{
    DerivedRow, EvalRule, Fact, FactStore, echo_asserted, least_model_of_reduct, world_edb_facts,
};
use crate::{
    physical::{GroundingUpdate, IncrementalGroundProgram, SignedFact},
    reason::perf_ledger::{NonmonotoneSolveRun, NonmonotoneSolver, nonmonotone_solve_run},
};

/// Wrap a reasoning-driver condition message as a typed diagnostic on the shared
/// substrate, preserving the authored text verbatim.
fn reason_err(detail: String) -> gmeow_errors::Diag {
    gmeow_errors::Diag::of_kind(crate::error::Reason { detail })
}

/// One stable model: its atoms in canonical (key-sorted) order.
#[derive(Debug, Clone)]
pub(crate) struct StableModel {
    /// The model's atoms, sorted by key.
    pub(crate) atoms: Vec<Fact>,
}

/// One multi-shot cautious-stable-model update.
#[derive(Debug, Clone)]
pub(crate) struct IncrementalStableModelShot {
    pub(crate) grounding: GroundingUpdate,
    pub(crate) solve: NonmonotoneSolveRun,
    pub(crate) rows: Arc<[DerivedRow]>,
}

/// Stateful cautious stable-model facade over an incrementally maintained ground
/// program.  Solving itself remains the existing from-scratch enumeration.
#[derive(Debug, Clone)]
pub(crate) struct IncrementalStableModelSession {
    world: String,
    ground: IncrementalGroundProgram,
    rows: Arc<[DerivedRow]>,
}

impl IncrementalStableModelSession {
    /// Build the initial ground program and solve it once from scratch.
    pub(crate) fn new(
        contract_hash: impl Into<String>,
        world: impl Into<String>,
        edb: impl IntoIterator<Item = Fact>,
        rules: &[EvalRule],
    ) -> gmeow_errors::Result<Self> {
        let world = world.into();
        let ground = IncrementalGroundProgram::new(contract_hash, edb, rules)?;
        let snapshot = ground.snapshot();
        let rows = Arc::from(cautious_ground_slice(
            &world,
            &snapshot.edb,
            &snapshot.rules,
        )?);
        Ok(Self {
            world,
            ground,
            rows,
        })
    }

    /// Current cached cautious rows.
    pub(crate) fn rows(&self) -> &[DerivedRow] {
        &self.rows
    }

    /// Active fully-ground rules in the current solver slice.
    pub(crate) fn active_ground_rule_count(&self) -> usize {
        self.ground.active_ground_rule_count()
    }

    /// Falsifiable maintenance oracle for tests / deterministic benchmark lanes.
    pub(crate) fn check_grounding_scratch_parity(&self) -> gmeow_errors::Result<()> {
        self.ground.check_scratch_parity()
    }

    /// Apply one signed EDB shot, reusing the cached answer only when the complete
    /// asserted-EDB + active-ground-rule slice is unchanged.
    pub(crate) fn apply(
        &mut self,
        changes: impl IntoIterator<Item = SignedFact>,
    ) -> gmeow_errors::Result<IncrementalStableModelShot> {
        let mut next_ground = self.ground.clone();
        let grounding = next_ground.apply(changes)?;
        let solve = nonmonotone_solve_run(
            NonmonotoneSolver::StableModel,
            grounding.slice_changed,
            grounding.edb_changes.len(),
            grounding.rule_changes.len(),
        );
        let next_rows = if solve.solver_reran() {
            let snapshot = next_ground.snapshot();
            Arc::from(cautious_ground_slice(
                &self.world,
                &snapshot.edb,
                &snapshot.rules,
            )?)
        } else {
            self.rows.clone()
        };
        self.ground = next_ground;
        self.rows = next_rows.clone();
        Ok(IncrementalStableModelShot {
            grounding,
            solve,
            rows: next_rows,
        })
    }
}

/// Enumerate the stable models of `rules` over every world in `store`.
///
/// Returns `(world_iri, models)` per world, worlds in sorted order, each world's
/// models in canonical order.
///
/// # Errors
///
/// Returns `Err` for an invalid input IRI, an unbound head/guard variable, or a
/// provenance-recipe failure surfaced by the reduct engine.
pub(crate) fn stable_models(
    store: &crate::store::WorldStore,
    rules: &[EvalRule],
) -> gmeow_errors::Result<Vec<(String, Vec<StableModel>)>> {
    let mut worlds = store.worlds();
    worlds.sort();

    let mut out: Vec<(String, Vec<StableModel>)> = Vec::with_capacity(worlds.len());
    for world in &worlds {
        let models = stable_models_in_world(store, world, rules)?;
        out.push((world.clone(), models));
    }
    Ok(out)
}

/// Enumerate the stable models for a single world.
fn stable_models_in_world(
    store: &crate::store::WorldStore,
    world: &str,
    rules: &[EvalRule],
) -> gmeow_errors::Result<Vec<StableModel>> {
    let edb_facts = world_edb_facts(store, world)?;
    stable_models_for_slice(&edb_facts, rules)
}

/// Deliberately from-scratch stable-model enumeration over one complete solver
/// slice. `rules` may be the source program or its maintained fully-ground form.
fn stable_models_for_slice(
    edb_facts: &[Fact],
    rules: &[EvalRule],
) -> gmeow_errors::Result<Vec<StableModel>> {
    let mut edb = FactStore::new();
    for f in edb_facts {
        edb.insert(f.clone());
    }
    let edb_keys: BTreeSet<_> = edb.key_set().into_iter().collect();

    // Candidate universe H = least model treating every NAF atom as absent.
    let empty = FactStore::new();
    let h = least_model_of_reduct(&edb, rules, &empty)?.store;

    // Candidate atoms = H \ EDB, sorted by key for canonical bitmask order.
    let mut candidates: Vec<Fact> = h
        .facts()
        .iter()
        .filter(|f| !edb_keys.contains(&f.key()))
        .cloned()
        .collect();
    candidates.sort_by_key(Fact::key);

    // Exhaustive guess-and-check is O(2^n) reduct evaluations — each subset of the
    // candidate universe is tested for Gelfond-Lifschitz stability. The hard ceiling
    // bounds that blow-up: 2^20 ≈ 1M reducts is the practical limit for gmeow-logic
    // v1 (the conformance corpus has 2 candidate atoms). Above it we hard-fail rather
    // than hang — a smarter grounder/ASP solver is the path to lifting this bound.
    const MAX_CANDIDATE_ATOMS: usize = 20;
    let n = candidates.len();
    if n > MAX_CANDIDATE_ATOMS {
        return Err(reason_err(format!(
            "stablemodel: candidate universe too large ({n} atoms > {MAX_CANDIDATE_ATOMS}) \
             for exhaustive enumeration in gmeow-logic v1 (2^{n} reduct evaluations)"
        )));
    }

    let mut models: Vec<StableModel> = Vec::new();
    for mask in 0u64..(1u64 << n) {
        // Build candidate model M = EDB ∪ subset(mask).
        let mut m = FactStore::new();
        for f in edb_facts {
            m.insert(f.clone());
        }
        for (i, cand) in candidates.iter().enumerate() {
            if mask & (1u64 << i) != 0 {
                m.insert(cand.clone());
            }
        }
        let m_keys = m.key_set();

        // Stability: the reduct's least model w.r.t. M must equal M.
        let reduct = least_model_of_reduct(&edb, rules, &m)?.store;
        if reduct.key_set() == m_keys {
            let mut atoms: Vec<Fact> = m.facts().to_vec();
            atoms.sort_by_key(Fact::key);
            models.push(StableModel { atoms });
        }
    }

    // Canonical model order: by the sorted vector of atom keys.
    models.sort_by_key(model_key_vec);
    Ok(models)
}

/// The canonical sort key of a model: the sorted vector of its atom keys.
fn model_key_vec(m: &StableModel) -> Vec<(String, String, String)> {
    m.atoms.iter().map(Fact::key).collect()
}

/// Materialize the *cautious* (skeptical) consequences of `rules`.
///
/// Emits the asserted-EDB rows plus the cautious derived rows — the intersection of
/// all stable models' atoms minus the EDB.  When the cautious set is empty (the
/// Phase-A corpus case) only the asserted rows are returned.
///
/// # Errors
///
/// Returns `Err` for the same conditions as [`stable_models`], or — when the
/// cautious set is non-empty — if a cautious atom's first-model derivation rests on
/// a non-cautious positive antecedent (a hard error in v1; no corpus case hits it).
pub(crate) fn cautious_materialize(
    store: &crate::store::WorldStore,
    rules: &[EvalRule],
) -> gmeow_errors::Result<Vec<DerivedRow>> {
    let mut worlds = store.worlds();
    worlds.sort();

    let mut out: Vec<DerivedRow> = Vec::new();
    for world in &worlds {
        let edb_facts = world_edb_facts(store, world)?;
        out.extend(cautious_ground_slice(world, &edb_facts, rules)?);
    }

    crate::rule_ir::sort_rows(&mut out);
    Ok(out)
}

/// Cautious materialization over one complete solver slice.
fn cautious_ground_slice(
    world: &str,
    edb_facts: &[Fact],
    rules: &[EvalRule],
) -> gmeow_errors::Result<Vec<DerivedRow>> {
    let mut out = echo_asserted(world, edb_facts)?;
    let mut edb = FactStore::new();
    for f in edb_facts {
        edb.insert(f.clone());
    }
    let edb_keys: BTreeSet<_> = edb.key_set().into_iter().collect();
    let models = stable_models_for_slice(edb_facts, rules)?;

    // Cautious set = intersection of all models' atom keys, minus the EDB.
    // No models (inconsistent program) → empty cautious set (only asserted).
    let cautious_keys: BTreeSet<(String, String, String)> = match models.first() {
        None => BTreeSet::new(),
        Some(first) => {
            let mut acc: BTreeSet<_> = first.atoms.iter().map(Fact::key).collect();
            for model in &models[1..] {
                let keys: BTreeSet<_> = model.atoms.iter().map(Fact::key).collect();
                acc = acc.intersection(&keys).cloned().collect();
            }
            acc.into_iter()
                .filter(|key| !edb_keys.contains(key))
                .collect()
        }
    };

    if cautious_keys.is_empty() {
        return Ok(out);
    }

    let first = models.first().expect("non-empty checked above");
    let mut first_model = FactStore::new();
    for fact in &first.atoms {
        first_model.insert(fact.clone());
    }

    let mut allowed_reifiers: BTreeSet<String> = BTreeSet::new();
    for fact in edb_facts {
        allowed_reifiers.insert(fact.reifier()?);
    }
    for fact in &first.atoms {
        if cautious_keys.contains(&fact.key()) {
            allowed_reifiers.insert(fact.reifier()?);
        }
    }

    let derivations = least_model_of_reduct(&edb, rules, &first_model)?.derivations;

    for row in derivations {
        let key = (
            crate::provenance::term_display(&row.subject),
            row.predicate.as_str().to_owned(),
            crate::provenance::term_display(&row.object),
        );
        if !cautious_keys.contains(&key) {
            continue;
        }
        for source in &row.source_quad_ids {
            if !allowed_reifiers.contains(source) {
                return Err(reason_err(format!(
                    "stablemodel: cautious atom <{}> <{}> {} cites non-cautious \
                     antecedent {source} — unsound provenance (gmeow-logic v1 does not \
                     materialize cautious consequences with non-cautious support)",
                    crate::provenance::term_display(&row.subject),
                    row.predicate.as_str(),
                    crate::provenance::term_display(&row.object)
                )));
            }
        }
        out.push(DerivedRow {
            graph: world.to_owned(),
            ..row
        });
    }
    crate::rule_ir::sort_rows(&mut out);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provenance::ASSERT_RULE_IRI;
    use crate::rule_ir::{EvalAtom, EvalTerm};
    use crate::store::WorldStore;
    use purrdf::TermValue;

    const SM: &str = "https://example.org/profiles/stable-model/";

    fn sm_rules() -> Vec<EvalRule> {
        let atom = |predicate: &str, negated| EvalAtom {
            subject: EvalTerm::var("?X"),
            predicate: format!("{SM}{predicate}"),
            object: EvalTerm::var("?X"),
            negated,
        };
        let rule = |head: &str, blocked: &str, name: &str| EvalRule {
            head: atom(head, false),
            body: vec![atom("candidate", false), atom(blocked, true)],
            rule_iri: format!("{SM}{name}"),
            distinct_pairs: Vec::new(),
            builtins: Vec::new(),
        };
        vec![
            rule("inSet", "outSet", "ruleInSet"),
            rule("outSet", "inSet", "ruleOutSet"),
        ]
    }

    fn sm_store() -> WorldStore {
        let store = WorldStore::new();
        store.insert_quad(
            &format!("{SM}world-choice"),
            &format!("{SM}x"),
            &format!("{SM}candidate"),
            &format!("{SM}x"),
        );
        store
    }

    fn sm_fact(name: &str) -> Fact {
        Fact {
            subject: TermValue::iri(format!("{SM}{name}")),
            predicate: format!("{SM}candidate"),
            object: TermValue::iri(format!("{SM}{name}")),
        }
    }

    fn row_key(row: &DerivedRow) -> (String, String, String, String, String) {
        (
            row.graph.clone(),
            crate::provenance::term_display(&row.subject),
            row.predicate.clone(),
            crate::provenance::term_display(&row.object),
            row.rule_iri.clone(),
        )
    }

    #[test]
    fn exactly_two_stable_models() {
        let rules = sm_rules();
        let store = sm_store();
        let per_world = stable_models(&store, &rules).expect("stable_models");
        assert_eq!(per_world.len(), 1, "one world");
        let (world, models) = &per_world[0];
        assert_eq!(world, &format!("{SM}world-choice"));
        assert_eq!(models.len(), 2, "exactly two stable models: {models:#?}");

        // Model 1 = {candidate, inSet}; Model 2 = {candidate, outSet} (canonical
        // order: inSet < outSet lexicographically).
        let predicates: Vec<Vec<String>> = models
            .iter()
            .map(|m| {
                m.atoms
                    .iter()
                    .map(|f| f.predicate.as_str().to_owned())
                    .collect()
            })
            .collect();
        assert!(
            predicates
                .iter()
                .any(|ps| ps.contains(&format!("{SM}inSet"))),
            "an inSet model exists: {predicates:?}"
        );
        assert!(
            predicates
                .iter()
                .any(|ps| ps.contains(&format!("{SM}outSet"))),
            "an outSet model exists: {predicates:?}"
        );
        // Neither model contains BOTH inSet and outSet.
        for ps in &predicates {
            let has_in = ps.contains(&format!("{SM}inSet"));
            let has_out = ps.contains(&format!("{SM}outSet"));
            assert!(!(has_in && has_out), "no model has both: {ps:?}");
        }
    }

    #[test]
    fn cautious_emits_only_asserted_candidate() {
        let rules = sm_rules();
        let store = sm_store();
        let rows = cautious_materialize(&store, &rules).expect("cautious");

        // Cautious intersection is empty → only the asserted candidate(x,x) quad.
        assert_eq!(rows.len(), 1, "exactly one (asserted) row: {rows:#?}");
        let row = &rows[0];
        assert_eq!(row.rule_iri, ASSERT_RULE_IRI);
        assert_eq!(row.predicate.as_str(), format!("{SM}candidate"));
        assert_eq!(
            crate::provenance::term_display(&row.subject),
            format!("<{SM}x>")
        );
        assert_eq!(
            crate::provenance::term_display(&row.object),
            format!("<{SM}x>")
        );
        // No derived (non-asserted) rows.
        assert!(
            !rows.iter().any(|r| r.rule_iri != ASSERT_RULE_IRI),
            "no derived rows in the cautious materialization"
        );
    }

    #[test]
    fn incremental_grounding_reruns_stable_solver_only_for_changed_slice() {
        let world = format!("{SM}world-choice");
        let rules = sm_rules();
        let mut session =
            IncrementalStableModelSession::new("contract", &world, [sm_fact("x")], &rules)
                .expect("initial incremental stable-model session");
        let direct = cautious_materialize(&sm_store(), &rules).expect("direct cautious solve");
        assert_eq!(
            session.rows().iter().map(row_key).collect::<Vec<_>>(),
            direct.iter().map(row_key).collect::<Vec<_>>(),
            "ground-program solve preserves direct cautious rows"
        );

        let initial_rows = session.rows.clone();
        let cancelled = session
            .apply([
                SignedFact {
                    fact: sm_fact("y"),
                    weight: 1,
                },
                SignedFact {
                    fact: sm_fact("y"),
                    weight: -1,
                },
            ])
            .expect("cancelled stable-model shot");
        assert!(!cancelled.grounding.slice_changed);
        assert!(!cancelled.solve.solver_reran());
        assert!(Arc::ptr_eq(&initial_rows, &cancelled.rows));
        assert!(Arc::ptr_eq(&cancelled.rows, &session.rows));

        let changed = session
            .apply([SignedFact {
                fact: sm_fact("y"),
                weight: 1,
            }])
            .expect("changed stable-model shot");
        assert!(changed.grounding.slice_changed);
        assert!(changed.solve.solver_reran());
        assert!(!Arc::ptr_eq(&cancelled.rows, &changed.rows));
        assert_eq!(
            changed.solve.solver.as_str(),
            "stable-model cautious enumeration"
        );
        assert_eq!(
            changed
                .rows
                .iter()
                .filter(|row| row.rule_iri == ASSERT_RULE_IRI)
                .count(),
            2,
            "both candidates are asserted while the choice atoms remain non-cautious"
        );
        session
            .check_grounding_scratch_parity()
            .expect("changed stable-model grounding matches scratch");
    }
}
