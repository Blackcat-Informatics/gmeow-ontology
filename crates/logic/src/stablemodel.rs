// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Native stable-model / answer-set evaluator (issue #651, Phase A).
//!
//! Nemo rejects non-stratifiable programs, so the stable models are enumerated
//! here directly, on top of the reduct least model in [`crate::rule_ir`].  Per
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
//! Phase-A note: [`stable_models`] / [`cautious_materialize`] are the entry points
//! `py.rs` will call in Phase B of #651; until that routing lands they are consumed
//! only by this module's tests, hence the crate-internal `dead_code` allowance.
#![allow(dead_code)]

use std::collections::BTreeSet;

use crate::rule_ir::{
    echo_asserted, least_model_of_reduct, world_edb_facts, DerivedRow, EvalRule, Fact, FactStore,
};

/// One stable model: its atoms in canonical (key-sorted) order.
#[derive(Debug, Clone)]
pub(crate) struct StableModel {
    /// The model's atoms, sorted by key.
    pub(crate) atoms: Vec<Fact>,
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
) -> Result<Vec<(String, Vec<StableModel>)>, String> {
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
) -> Result<Vec<StableModel>, String> {
    let edb_facts = world_edb_facts(store, world)?;
    let mut edb = FactStore::new();
    for f in &edb_facts {
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
        return Err(format!(
            "stablemodel: candidate universe too large ({n} atoms > {MAX_CANDIDATE_ATOMS}) \
             for exhaustive enumeration in gmeow-logic v1 (2^{n} reduct evaluations)"
        ));
    }

    let mut models: Vec<StableModel> = Vec::new();
    for mask in 0u64..(1u64 << n) {
        // Build candidate model M = EDB ∪ subset(mask).
        let mut m = FactStore::new();
        for f in &edb_facts {
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
) -> Result<Vec<DerivedRow>, String> {
    let mut worlds = store.worlds();
    worlds.sort();

    let mut out: Vec<DerivedRow> = Vec::new();
    for world in &worlds {
        let edb_facts = world_edb_facts(store, world)?;
        out.extend(echo_asserted(world, &edb_facts)?);

        let mut edb = FactStore::new();
        for f in &edb_facts {
            edb.insert(f.clone());
        }
        let edb_keys: BTreeSet<_> = edb.key_set().into_iter().collect();

        let models = stable_models_in_world(store, world, rules)?;

        // Cautious set = intersection of all models' atom keys, minus the EDB.
        // No models (inconsistent program) → empty cautious set (only asserted).
        let cautious_keys: BTreeSet<(String, String, String)> = match models.first() {
            None => BTreeSet::new(),
            Some(first) => {
                let mut acc: BTreeSet<_> = first.atoms.iter().map(Fact::key).collect();
                for m in &models[1..] {
                    let mk: BTreeSet<_> = m.atoms.iter().map(Fact::key).collect();
                    acc = acc.intersection(&mk).cloned().collect();
                }
                acc.into_iter().filter(|k| !edb_keys.contains(k)).collect()
            }
        };

        if cautious_keys.is_empty() {
            continue; // only the asserted rows for this world
        }

        // Non-empty cautious set: take provenance from the FIRST model's reduct
        // derivations, requiring every positive antecedent of a cautious atom to be
        // cautious itself (hard error otherwise — no corpus case reaches here).
        let first = models.first().expect("non-empty checked above");
        let mut first_model = FactStore::new();
        for f in &first.atoms {
            first_model.insert(f.clone());
        }

        // A cautious derivation may cite only antecedents that are themselves in the
        // cautious materialization — the asserted EDB or another cautious head.
        // Otherwise its source quad is absent from the emitted set and the provenance
        // would dangle.  Precompute the admissible source reifiers so the loop below
        // can hard-fail (rather than silently emit) on a non-cautious antecedent.
        let mut allowed_reifiers: BTreeSet<String> = BTreeSet::new();
        for f in &edb_facts {
            allowed_reifiers.insert(f.reifier()?);
        }
        for f in &first.atoms {
            if cautious_keys.contains(&f.key()) {
                allowed_reifiers.insert(f.reifier()?);
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
            // Hard error (no-optionality): a cautious atom whose firing rests on a
            // non-cautious positive antecedent cannot be soundly stamped.  This branch
            // is never reached by the Phase-A corpus (its cautious set is empty); it is
            // the documented v1 limitation made real, NOT a silent skip.
            for src in &row.source_quad_ids {
                if !allowed_reifiers.contains(src) {
                    return Err(format!(
                        "stablemodel: cautious atom <{}> <{}> {} cites non-cautious \
                         antecedent {src} — unsound provenance (gmeow-logic v1 does not \
                         materialize cautious consequences with non-cautious support)",
                        crate::provenance::term_display(&row.subject),
                        row.predicate.as_str(),
                        crate::provenance::term_display(&row.object)
                    ));
                }
            }
            out.push(DerivedRow {
                graph: world.clone(),
                ..row
            });
        }
    }

    crate::rule_ir::sort_rows(&mut out);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provenance::ASSERT_RULE_IRI;
    use crate::rule_ir::parse_eval_rules;
    use crate::store::WorldStore;

    const SM: &str = "https://example.org/profiles/stable-model/";

    fn sm_rules() -> Vec<EvalRule> {
        let rls = format!(
            "#[name(\"{SM}ruleInSet\")]\n\
             <{SM}inSet>(?X, ?X, ?W) :-\n\
                 <{SM}candidate>(?X, ?X, ?W),\n\
                 ~<{SM}outSet>(?X, ?X, ?W) .\n\
             #[name(\"{SM}ruleOutSet\")]\n\
             <{SM}outSet>(?X, ?X, ?W) :-\n\
                 <{SM}candidate>(?X, ?X, ?W),\n\
                 ~<{SM}inSet>(?X, ?X, ?W) .\n"
        );
        parse_eval_rules(&rls).expect("parse SM rules")
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
}
