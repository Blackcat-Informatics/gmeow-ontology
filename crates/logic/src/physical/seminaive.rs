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

use std::collections::{BTreeSet, HashMap, HashSet};

use crate::physical::store::{Bound, RelationStore};
use crate::provenance::mint_derivation_id;
use crate::rule_ir::{
    distinct_pairs_satisfied, echo_asserted, ground, ground_head, match_atom, sort_rows,
    world_edb_facts, DerivedRow, EvalAtom, EvalRule, Fact, FactKey, FactStore, RuleRoundCandidate,
    Solution,
};

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

// ── Index-selected semi-naive join ────────────────────────────────────────────────

/// Compute the [`Bound`] for `atom`'s `(subject, object)` columns under `sol`.
///
/// A position contributes a surface iff it grounds (a bound var or a constant);
/// an unbound var contributes nothing.  Surfaces come from [`ground`] so they match
/// the [`RelationStore`]'s N3 index keys exactly (`<iri>` for an IRI, the literal's
/// N3 for a literal).  The borrowed `&str` slices point into the owned strings the
/// caller pins for the lifetime of the `select` call.
fn atom_bound<'a>(subj: &'a Option<String>, obj: &'a Option<String>) -> Bound<'a> {
    match (subj.as_deref(), obj.as_deref()) {
        (Some(s), Some(o)) => Bound::Both(s, o),
        (Some(s), None) => Bound::Subject(s),
        (None, Some(o)) => Bound::Object(o),
        (None, None) => Bound::Any,
    }
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
        // Compute the selection bound from the current partial solution.  Pin the
        // ground surfaces so the `Bound`'s `&str`s outlive the `select` call.
        let subj_surface = ground(&atom.subject, sol);
        let obj_surface = ground(&atom.object, sol);
        let bound = atom_bound(&subj_surface, &obj_surface);
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

    if !negated.is_empty() {
        solutions.retain(|sol| {
            !negated
                .iter()
                .any(|neg| negated_atom_satisfied(neg, sol, accumulated))
        });
    }

    solutions
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
) -> Result<NativeOutcome<Vec<DerivedRow>>, String> {
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

    let mut out: Vec<DerivedRow> = Vec::new();
    for world in &worlds {
        let edb_facts = world_edb_facts(store, world)?;

        // Asserted-EDB echo (identical to wellfounded::materialize).
        out.extend(echo_asserted(world, &edb_facts)?);

        let derived = eval_world_stratified(&edb_facts, &rules_by_stratum)?;
        for mut row in derived {
            row.graph = world.clone();
            out.push(row);
        }
    }

    sort_rows(&mut out);
    Ok(NativeOutcome::Decided(out))
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
) -> Result<Vec<DerivedRow>, String> {
    // Shared accumulated store (both forms), seeded from the EDB in sorted-key order
    // (world_edb_facts already sorted), so seeding matches the reference.
    let mut store = FactStore::new();
    let mut rel = RelationStore::new();
    let mut depth: HashMap<FactKey, u32> = HashMap::new();

    for f in edb_facts {
        depth.insert(f.key(), 0);
        if store.insert(f.clone()) {
            rel.insert(&f.predicate, f.subject.clone(), f.object.clone());
        }
    }

    let mut derivations: Vec<DerivedRow> = Vec::new();

    for stratum_rules in rules_by_stratum {
        if stratum_rules.is_empty() {
            continue;
        }
        eval_stratum_fixpoint(
            stratum_rules,
            &mut store,
            &mut rel,
            &mut depth,
            &mut derivations,
        )?;
    }

    Ok(derivations)
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
) -> Result<(), String> {
    // Seed delta with ALL currently-known keys so this stratum's rules fire against
    // the seed in round 1 (mirrors `least_model_of_reduct`'s `delta = key_set()`).
    let mut delta: HashSet<FactKey> = store.key_set();

    loop {
        let mut round: HashMap<FactKey, RuleRoundCandidate> = HashMap::new();

        for rule in rules {
            for sol in join_body_indexed(rule, rel, rel, &delta) {
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
        for (_key, winner) in round {
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
        }

        delta = new_delta;
    }

    Ok(())
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
) -> Result<NativeOutcome<Vec<Fact>>, String> {
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
        let predicate = oxigraph::model::NamedNode::new(pred)
            .map_err(|e| format!("physical::evaluate: invalid EDB predicate IRI {pred:?}: {e}"))?;
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

    for f in &edb_facts {
        depth.insert(f.key(), 0);
        if store.insert(f.clone()) {
            rel.insert(&f.predicate, f.subject.clone(), f.object.clone());
        }
    }

    // This leg returns the full fact set (`store.facts()`); the derivation rows are
    // unused here but the shared fixpoint records them for the forward leg.
    let mut derivations: Vec<DerivedRow> = Vec::new();
    for stratum_rules in &rules_by_stratum {
        if stratum_rules.is_empty() {
            continue;
        }
        eval_stratum_fixpoint(
            stratum_rules,
            &mut store,
            &mut rel,
            &mut depth,
            &mut derivations,
        )?;
    }

    Ok(NativeOutcome::Decided(store.facts().to_vec()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rule_ir::{least_model_of_reduct, parse_eval_rules};
    use crate::store::WorldStore;
    use oxigraph::model::{NamedNode, Term};

    const NS: &str = "https://example.org/p3/";

    fn nn(local: &str) -> NamedNode {
        NamedNode::new(format!("{NS}{local}")).expect("valid IRI")
    }

    fn term(local: &str) -> Term {
        Term::NamedNode(nn(local))
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
            r.subject.to_string(),
            r.predicate.as_str().to_owned(),
            r.object.to_string(),
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
                    r.subject.to_string(),
                    r.predicate.as_str().to_owned(),
                    r.object.to_string(),
                    r.rule_iri.clone(),
                    r.source_quad_ids.clone(),
                    r.derivation_id.clone(),
                )
            })
            .collect();
        ref_rows.sort();

        // Native path via a WorldStore.
        let store = tc_store();
        let outcome = materialize_native(&store, &rules).expect("materialize_native");
        let NativeOutcome::Decided(rows) = outcome else {
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
        let outcome = materialize_native(&store, &rules).expect("materialize_native");
        let NativeOutcome::Decided(rows) = outcome else {
            panic!("expected Decided");
        };

        // Collect derived path pairs (subject, object) local names.
        let path_pred = format!("{NS}path");
        let mut pairs: BTreeSet<(String, String)> = BTreeSet::new();
        for r in derived_only(&rows) {
            if r.predicate.as_str() == path_pred {
                pairs.insert((r.subject.to_string(), r.object.to_string()));
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

        let outcome = materialize_native(&store, &rules).expect("materialize_native");
        let NativeOutcome::Decided(rows) = outcome else {
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
                reachable.insert(r.subject.to_string());
            } else if r.predicate.as_str() == unreach_pred {
                unreachable.insert(r.subject.to_string());
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

        let outcome = materialize_native(&store, &rules).expect("materialize_native");
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
}
