// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Signed incremental grounding for the non-monotone binary fragment.
//!
//! This module implements only the part of multi-shot WFS / stable-model
//! evaluation that is mechanically incrementalizable: **grounding**.  The
//! non-monotone solver remains a named from-scratch step in the performance
//! ledger.
//!
//! For a fixed source program, [`IncrementalGroundProgram`] keeps two related
//! objects:
//!
//! * the positive candidate universe `H`, maintained by the recursive Z-set
//!   circuit in [`super::incremental`], after removing NAF literals; and
//! * the exact set of fully-ground rule instances whose positive bodies match
//!   `H`, maintained as a signed support bag.
//!
//! A rule grounding is a non-recursive relational query over `H`.  Its delta is
//! therefore the same telescoping product used by the recursive circuit:
//! `new[..p] × delta[p] × old[p+1..]`.  Raw support multiplicities stay internal;
//! only a `0 ↔ positive` crossing changes the active ground program.  The solver
//! slice is the active ground-rule set **plus the asserted EDB**, so asserting a
//! fact that was already derivable still invalidates the non-monotone solver even
//! when `H` and every ground rule stay unchanged.
//!
//! The construction is a finite, safe binary-rule seam.  Every rule must have a
//! positive body atom, arithmetic builtins are rejected, and every variable in a
//! head, NAF literal, or inequality guard must be bound by the positive body.  A
//! blank-node binding cannot currently be represented as a constant [`EvalTerm`]
//! and is rejected rather than rewritten as an IRI.

use std::cell::Cell;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use crate::provenance::{ProvenanceSemiring, ZWeightSemiring};
use crate::rule_ir::{
    EvalAtom, EvalRule, EvalTerm, Fact, FactKey, Solution, distinct_pairs_satisfied, ground,
    match_atom, surface_to_value,
};

use super::incremental::{IncrementalSession, SignedFact};

fn grounding_err(detail: impl Into<String>) -> gmeow_errors::Diag {
    gmeow_errors::Diag::of_kind(crate::error::Reason {
        detail: detail.into(),
    })
}

/// One active ground-rule insertion (`+1`) or retraction (`-1`).
#[derive(Debug, Clone)]
pub(crate) struct GroundRuleChange {
    /// Fully-ground rule instance.
    pub(crate) rule: EvalRule,
    /// Set-boundary change; exactly `+1` or `-1`.
    pub(crate) weight: i64,
}

/// The semantic delta produced by one incremental-grounding transaction.
#[derive(Debug, Clone)]
pub(crate) struct GroundingUpdate {
    /// Consolidated asserted-EDB changes in lexical [`FactKey`] order.
    pub(crate) edb_changes: Vec<SignedFact>,
    /// Active ground-rule zero-crossings in canonical rendered-rule order.
    pub(crate) rule_changes: Vec<GroundRuleChange>,
    /// Number of candidate-universe fact zero-crossings maintained by the recursive
    /// positive circuit.
    pub(crate) universe_changes: usize,
    /// Deterministic number of signed rows admitted at differentiated join positions
    /// across the positive-universe circuit and ground-rule projection.
    pub(crate) joined_rows: u64,
    /// The [`joined_rows`](Self::joined_rows) contribution from recursive positive
    /// candidate-universe maintenance.
    pub(crate) universe_joined_rows: u64,
    /// The [`joined_rows`](Self::joined_rows) contribution from ground-rule joins.
    pub(crate) ground_rule_joined_rows: u64,
    /// Every candidate row inspected by the differentiated ground-rule joins,
    /// including unit-weight prefix/suffix relations. This has an exactly comparable
    /// full scratch projection.
    pub(crate) ground_rule_probe_rows: u64,
    /// Whether the complete solver slice (asserted EDB + active ground rules) changed.
    pub(crate) slice_changed: bool,
}

/// Canonical, scratch-comparable view of the current solver input.
#[derive(Debug, Clone)]
pub(crate) struct GroundProgramSnapshot {
    /// Asserted facts, lexical by [`FactKey`].
    pub(crate) edb: Vec<Fact>,
    /// Fully-ground active rules, canonical by their rendered key.
    pub(crate) rules: Vec<EvalRule>,
}

impl GroundProgramSnapshot {
    fn parity_key(&self) -> (Vec<FactKey>, Vec<String>) {
        (
            self.edb.iter().map(Fact::key).collect(),
            self.rules.iter().map(ground_rule_key).collect(),
        )
    }
}

#[derive(Debug, Clone)]
struct WeightedGroundRule {
    rule: EvalRule,
    weight: i64,
}

/// Canonical active-rule order: source-program order first, then the rendered
/// fully-ground substitution.  Preserving source order is load-bearing for the
/// reduct evaluator's deterministic first-wins provenance.
type GroundRuleKey = (usize, String);

#[derive(Clone)]
struct WeightedSolution {
    solution: Solution,
    weight: i64,
}

/// Stateful ground-program maintainer for one fixed binary rule program.
#[derive(Debug, Clone)]
pub(crate) struct IncrementalGroundProgram {
    contract_hash: String,
    source_rules: Arc<[EvalRule]>,
    positive: IncrementalSession,
    edb: BTreeMap<FactKey, Fact>,
    ground_rules: BTreeMap<GroundRuleKey, WeightedGroundRule>,
    scratch_ground_rule_probe_rows: Cell<Option<u64>>,
}

impl IncrementalGroundProgram {
    /// Build and fully ground a fixed program over one asserted EDB.
    ///
    /// # Errors
    ///
    /// Rejects an unsafe/non-finite source rule, a blank-node constant binding, or
    /// any error from the signed positive-Datalog session.
    pub(crate) fn new(
        contract_hash: impl Into<String>,
        edb: impl IntoIterator<Item = Fact>,
        rules: &[EvalRule],
    ) -> gmeow_errors::Result<Self> {
        let contract_hash = contract_hash.into();
        let positive_rules = positive_projection(rules)?;
        let edb = keyed_facts(edb);
        let positive = IncrementalSession::new(
            format!("{contract_hash}:nonmonotone-grounding"),
            edb.values().cloned(),
            &positive_rules,
        )?;
        let mut scratch_ground_rule_probe_rows = 0;
        let ground_rules =
            ground_from_scratch(rules, &positive, &mut scratch_ground_rule_probe_rows)?;
        Ok(Self {
            contract_hash,
            source_rules: Arc::from(rules.to_vec()),
            positive,
            edb,
            ground_rules,
            scratch_ground_rule_probe_rows: Cell::new(Some(scratch_ground_rule_probe_rows)),
        })
    }

    /// Current canonical solver slice.
    pub(crate) fn snapshot(&self) -> GroundProgramSnapshot {
        GroundProgramSnapshot {
            edb: self.edb.values().cloned().collect(),
            rules: self
                .ground_rules
                .values()
                .filter(|entry| entry.weight > 0)
                .map(|entry| entry.rule.clone())
                .collect(),
        }
    }

    /// Apply one atomic signed EDB transaction and maintain the ground program.
    ///
    /// Input duplicates are consolidated first.  A net insertion of an asserted
    /// fact already asserted, or a net retraction of an absent asserted fact, is a
    /// hard error.  The recursive candidate universe and the ground-rule bag are
    /// updated on clones and installed only after every checked ring operation and
    /// safety check succeeds.
    pub(crate) fn apply(
        &mut self,
        changes: impl IntoIterator<Item = SignedFact>,
    ) -> gmeow_errors::Result<GroundingUpdate> {
        let consolidated = consolidate_edb(changes)?;
        if consolidated.is_empty() {
            return Ok(GroundingUpdate {
                edb_changes: Vec::new(),
                rule_changes: Vec::new(),
                universe_changes: 0,
                joined_rows: 0,
                universe_joined_rows: 0,
                ground_rule_joined_rows: 0,
                ground_rule_probe_rows: 0,
                slice_changed: false,
            });
        }

        let mut next_edb = self.edb.clone();
        for (key, change) in &consolidated {
            let present = next_edb.contains_key(key);
            let next = ZWeightSemiring.add(i64::from(present), change.weight)?;
            match next {
                0 => {
                    next_edb.remove(key);
                }
                1 => {
                    next_edb.insert(key.clone(), change.fact.clone());
                }
                _ => {
                    return Err(grounding_err(format!(
                        "incremental grounding changes asserted fact {key:?} from membership {} by {} (result {next}); expected exactly 0 or 1",
                        i64::from(present),
                        change.weight
                    )));
                }
            }
        }

        let mut next_positive = self.positive.clone();
        let positive_delta = next_positive.apply(consolidated.values().cloned())?;
        let universe_delta: BTreeMap<FactKey, SignedFact> = positive_delta
            .changes
            .iter()
            .cloned()
            .map(|change| (change.fact.key(), change))
            .collect();

        let mut ground_rule_joined_rows = 0u64;
        let mut ground_rule_probe_rows = 0u64;
        let weighted_delta = differentiate_grounding(
            &self.source_rules,
            &self.positive,
            &next_positive,
            &universe_delta,
            &mut ground_rule_joined_rows,
            &mut ground_rule_probe_rows,
        )?;
        let joined_rows = positive_delta
            .joined_rows
            .checked_add(ground_rule_joined_rows)
            .ok_or_else(|| grounding_err("incremental grounding joined-row total overflow"))?;

        // Validate every adjusted support weight before mutating the live map.  This
        // keeps the whole transaction atomic even when a checked ring operation or
        // grounding invariant fails after the positive session has settled.
        let mut adjusted = Vec::with_capacity(weighted_delta.len());
        let mut rule_changes = Vec::new();
        for (key, delta) in weighted_delta {
            let old_weight = self.ground_rules.get(&key).map_or(0, |entry| entry.weight);
            let new_weight = ZWeightSemiring.add(old_weight, delta.weight)?;
            if new_weight < 0 {
                return Err(grounding_err(format!(
                    "incremental grounding produced negative support {new_weight} for ground rule {key:?}"
                )));
            }
            let was_active = old_weight > 0;
            let is_active = new_weight > 0;
            if was_active != is_active {
                let rule = if is_active {
                    delta.rule.clone()
                } else {
                    self.ground_rules
                        .get(&key)
                        .expect("positive old support must have a stored ground rule")
                        .rule
                        .clone()
                };
                rule_changes.push(GroundRuleChange {
                    rule,
                    weight: if is_active { 1 } else { -1 },
                });
            }
            adjusted.push((key, delta.rule, new_weight));
        }

        for (key, rule, new_weight) in adjusted {
            if new_weight == 0 {
                self.ground_rules.remove(&key);
            } else {
                self.ground_rules.insert(
                    key,
                    WeightedGroundRule {
                        rule,
                        weight: new_weight,
                    },
                );
            }
        }
        let edb_changes = consolidated.into_values().collect::<Vec<_>>();
        self.positive = next_positive;
        self.edb = next_edb;
        self.scratch_ground_rule_probe_rows.set(None);

        Ok(GroundingUpdate {
            slice_changed: !edb_changes.is_empty() || !rule_changes.is_empty(),
            edb_changes,
            rule_changes,
            universe_changes: positive_delta.changes.len(),
            joined_rows,
            universe_joined_rows: positive_delta.joined_rows,
            ground_rule_joined_rows,
            ground_rule_probe_rows,
        })
    }

    /// Candidate-row probes paid by a clean full grounding of this exact snapshot.
    pub(crate) fn scratch_ground_rule_probe_rows(&self) -> gmeow_errors::Result<u64> {
        if let Some(probes) = self.scratch_ground_rule_probe_rows.get() {
            return Ok(probes);
        }
        let mut probes = 0;
        let _ = ground_from_scratch(&self.source_rules, &self.positive, &mut probes)?;
        self.scratch_ground_rule_probe_rows.set(Some(probes));
        Ok(probes)
    }

    /// Number of active fully-ground rule instances in the current solver slice.
    pub(crate) fn active_ground_rule_count(&self) -> usize {
        self.ground_rules
            .values()
            .filter(|entry| entry.weight > 0)
            .count()
    }

    /// Rebuild the same fixed program from the current asserted EDB and hard-fail
    /// if the maintained solver slice diverges.  This is the falsifiable scratch
    /// oracle used by tests and deterministic benchmark evidence.
    pub(crate) fn check_scratch_parity(&self) -> gmeow_errors::Result<()> {
        let scratch = Self::new(
            self.contract_hash.clone(),
            self.edb.values().cloned(),
            &self.source_rules,
        )?;
        let maintained = self.snapshot().parity_key();
        let rebuilt = scratch.snapshot().parity_key();
        if maintained != rebuilt {
            return Err(grounding_err(format!(
                "incremental ground program diverged from scratch reconstruction: maintained {:?}, scratch {:?}",
                maintained, rebuilt
            )));
        }
        Ok(())
    }
}

fn keyed_facts(edb: impl IntoIterator<Item = Fact>) -> BTreeMap<FactKey, Fact> {
    let mut out = BTreeMap::new();
    for fact in edb {
        out.entry(fact.key()).or_insert(fact);
    }
    out
}

fn positive_projection(rules: &[EvalRule]) -> gmeow_errors::Result<Vec<EvalRule>> {
    let mut projected = Vec::with_capacity(rules.len());
    for rule in rules {
        if !rule.builtins.is_empty() {
            return Err(grounding_err(format!(
                "incremental grounding does not admit arithmetic/comparison builtins in rule <{}>",
                rule.rule_iri
            )));
        }
        let positive_body: Vec<EvalAtom> = rule
            .body
            .iter()
            .filter(|atom| !atom.negated)
            .cloned()
            .collect();
        if positive_body.is_empty() {
            return Err(grounding_err(format!(
                "incremental grounding requires at least one positive body atom in rule <{}>",
                rule.rule_iri
            )));
        }
        validate_safety(rule, &positive_body)?;
        let mut positive = rule.clone();
        positive.body = positive_body;
        projected.push(positive);
    }
    Ok(projected)
}

fn validate_safety(rule: &EvalRule, positive_body: &[EvalAtom]) -> gmeow_errors::Result<()> {
    let mut bound = BTreeSet::new();
    for atom in positive_body {
        collect_vars(&atom.subject, &mut bound);
        collect_vars(&atom.object, &mut bound);
    }
    let mut required = BTreeSet::new();
    collect_vars(&rule.head.subject, &mut required);
    collect_vars(&rule.head.object, &mut required);
    for atom in rule.body.iter().filter(|atom| atom.negated) {
        collect_vars(&atom.subject, &mut required);
        collect_vars(&atom.object, &mut required);
    }
    for (left, right) in &rule.distinct_pairs {
        required.insert(left.clone());
        required.insert(right.clone());
    }
    if let Some(unbound) = required.difference(&bound).next() {
        return Err(grounding_err(format!(
            "incremental grounding rule <{}> is unsafe: variable {unbound:?} is not bound by a positive body atom",
            rule.rule_iri
        )));
    }
    Ok(())
}

fn collect_vars(term: &EvalTerm, vars: &mut BTreeSet<String>) {
    if let EvalTerm::Var(name) = term {
        vars.insert(name.clone());
    }
}

fn ground_from_scratch(
    rules: &[EvalRule],
    universe: &IncrementalSession,
    probe_rows: &mut u64,
) -> gmeow_errors::Result<BTreeMap<GroundRuleKey, WeightedGroundRule>> {
    let mut output = BTreeMap::new();
    for (source_index, rule) in rules.iter().enumerate() {
        let positive: Vec<&EvalAtom> = rule.body.iter().filter(|atom| !atom.negated).collect();
        let mut solutions = vec![WeightedSolution {
            solution: Solution {
                bindings: Vec::new(),
                source_facts: Vec::new(),
            },
            weight: ZWeightSemiring.one(),
        }];
        for atom in positive {
            solutions = extend_from_closure(atom, universe, &solutions, probe_rows)?;
            if solutions.is_empty() {
                break;
            }
        }
        add_groundings(source_index, rule, solutions, &mut output)?;
    }
    Ok(output)
}

fn differentiate_grounding(
    rules: &[EvalRule],
    old: &IncrementalSession,
    new: &IncrementalSession,
    delta: &BTreeMap<FactKey, SignedFact>,
    joined_rows: &mut u64,
    probe_rows: &mut u64,
) -> gmeow_errors::Result<BTreeMap<GroundRuleKey, WeightedGroundRule>> {
    let mut output = BTreeMap::new();
    for (source_index, rule) in rules.iter().enumerate() {
        let positive: Vec<&EvalAtom> = rule.body.iter().filter(|atom| !atom.negated).collect();
        for delta_position in 0..positive.len() {
            let mut solutions = vec![WeightedSolution {
                solution: Solution {
                    bindings: Vec::new(),
                    source_facts: Vec::new(),
                },
                weight: ZWeightSemiring.one(),
            }];
            for (position, atom) in positive.iter().enumerate() {
                solutions = if position < delta_position {
                    extend_from_closure(atom, new, &solutions, probe_rows)?
                } else if position == delta_position {
                    extend_from_delta(atom, delta, &solutions, joined_rows, probe_rows)?
                } else {
                    extend_from_closure(atom, old, &solutions, probe_rows)?
                };
                if solutions.is_empty() {
                    break;
                }
            }
            add_groundings(source_index, rule, solutions, &mut output)?;
        }
    }
    Ok(output)
}

fn extend_from_closure(
    atom: &EvalAtom,
    session: &IncrementalSession,
    partial: &[WeightedSolution],
    probe_rows: &mut u64,
) -> gmeow_errors::Result<Vec<WeightedSolution>> {
    let mut out = Vec::new();
    for base in partial {
        for fact in session.closure_facts_for_predicate(&atom.predicate) {
            *probe_rows = probe_rows
                .checked_add(1)
                .ok_or_else(|| grounding_err("incremental grounding probe-row counter overflow"))?;
            if let Some(mut solution) = match_atom(atom, fact, &base.solution) {
                solution.source_facts.push(fact.clone());
                out.push(WeightedSolution {
                    solution,
                    weight: base.weight,
                });
            }
        }
    }
    Ok(out)
}

fn extend_from_delta(
    atom: &EvalAtom,
    delta: &BTreeMap<FactKey, SignedFact>,
    partial: &[WeightedSolution],
    joined_rows: &mut u64,
    probe_rows: &mut u64,
) -> gmeow_errors::Result<Vec<WeightedSolution>> {
    let mut out = Vec::new();
    for base in partial {
        for change in delta
            .values()
            .filter(|change| change.fact.predicate == atom.predicate)
        {
            *joined_rows = joined_rows.checked_add(1).ok_or_else(|| {
                grounding_err("incremental grounding joined-row counter overflow")
            })?;
            *probe_rows = probe_rows
                .checked_add(1)
                .ok_or_else(|| grounding_err("incremental grounding probe-row counter overflow"))?;
            if let Some(mut solution) = match_atom(atom, &change.fact, &base.solution) {
                solution.source_facts.push(change.fact.clone());
                let weight = ZWeightSemiring.multiply(base.weight, change.weight)?;
                if weight != 0 {
                    out.push(WeightedSolution { solution, weight });
                }
            }
        }
    }
    Ok(out)
}

fn add_groundings(
    source_index: usize,
    source: &EvalRule,
    solutions: Vec<WeightedSolution>,
    output: &mut BTreeMap<GroundRuleKey, WeightedGroundRule>,
) -> gmeow_errors::Result<()> {
    for weighted in solutions {
        if !distinct_pairs_satisfied(&source.distinct_pairs, &weighted.solution)? {
            continue;
        }
        let rule = ground_rule(source, &weighted.solution)?;
        let key = (source_index, ground_rule_key(&rule));
        match output.entry(key) {
            std::collections::btree_map::Entry::Vacant(slot) => {
                slot.insert(WeightedGroundRule {
                    rule,
                    weight: weighted.weight,
                });
            }
            std::collections::btree_map::Entry::Occupied(mut slot) => {
                let combined = ZWeightSemiring.add(slot.get().weight, weighted.weight)?;
                if combined == 0 {
                    slot.remove();
                } else {
                    slot.get_mut().weight = combined;
                }
            }
        }
    }
    Ok(())
}

fn ground_rule(source: &EvalRule, solution: &Solution) -> gmeow_errors::Result<EvalRule> {
    Ok(EvalRule {
        head: ground_atom(&source.head, solution)?,
        body: source
            .body
            .iter()
            .map(|atom| ground_atom(atom, solution))
            .collect::<gmeow_errors::Result<Vec<_>>>()?,
        rule_iri: source.rule_iri.clone(),
        distinct_pairs: Vec::new(),
        builtins: Vec::new(),
        constraint_tag: None,
    })
}

fn ground_atom(atom: &EvalAtom, solution: &Solution) -> gmeow_errors::Result<EvalAtom> {
    Ok(EvalAtom {
        subject: ground_term(&atom.subject, solution)?,
        predicate: atom.predicate.clone(),
        object: ground_term(&atom.object, solution)?,
        negated: atom.negated,
    })
}

fn ground_term(term: &EvalTerm, solution: &Solution) -> gmeow_errors::Result<EvalTerm> {
    let surface = ground(term, solution).ok_or_else(|| {
        grounding_err(format!(
            "incremental grounding left term {term:?} unbound after positive-body matching"
        ))
    })?;
    if let Some(iri) = surface
        .strip_prefix('<')
        .and_then(|value| value.strip_suffix('>'))
    {
        return Ok(EvalTerm::ConstNamed(iri.to_owned()));
    }
    if surface.starts_with("_:") {
        return Err(grounding_err(format!(
            "incremental grounding cannot encode blank-node constant {surface:?} in EvalTerm"
        )));
    }
    Ok(EvalTerm::ConstLit(surface_to_value(&surface)?))
}

fn ground_rule_key(rule: &EvalRule) -> String {
    super::plan::canonical_rule_hash(std::slice::from_ref(rule))
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn consolidate_edb(
    changes: impl IntoIterator<Item = SignedFact>,
) -> gmeow_errors::Result<BTreeMap<FactKey, SignedFact>> {
    let mut out: BTreeMap<FactKey, SignedFact> = BTreeMap::new();
    for change in changes {
        if change.weight == 0 {
            continue;
        }
        let key = change.fact.key();
        match out.entry(key) {
            std::collections::btree_map::Entry::Vacant(slot) => {
                slot.insert(change);
            }
            std::collections::btree_map::Entry::Occupied(mut slot) => {
                let combined = ZWeightSemiring.add(slot.get().weight, change.weight)?;
                if combined == 0 {
                    slot.remove();
                } else {
                    slot.get_mut().weight = combined;
                }
            }
        }
    }
    Ok(out)
}
