// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Transaction-local proof retention for the joint native executor. A retained
//! row remains derived: its positive premises, producer contract and scoped
//! witness introductions must survive admission. Completed reads are invalidated
//! through the full world-aware producer graph before any row is reused.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use purrdf::TermValue;

use super::{JointProgram, RoundCandidateBuffer, SkolemRegistry, WorldRuntime};
use crate::physical::effects::{WorldEffectSchedule, WorldProducerEffect};
use crate::rule_ir::{DerivedRow, Fact, FactKey};

/// One retained execution, owned by an explicit reasoning session. Forks share
/// the immutable base; there is no process-global corpus or result cache.
#[derive(Clone, Default)]
pub(crate) struct RetainedJoint {
    state: Option<Arc<Snapshot>>,
    reused: usize,
}

struct Snapshot {
    template: [u8; 32],
    graphs: BTreeMap<String, Option<TermValue>>,
    facts: BTreeMap<String, Vec<Fact>>,
    rows: Vec<DerivedRow>,
    registry: SkolemRegistry,
}

pub(super) struct Reuse {
    by_stratum: BTreeMap<usize, Vec<DerivedRow>>,
    registry: SkolemRegistry,
    introductions: BTreeMap<usize, Vec<crate::physical::WitnessDerivation>>,
    count: usize,
}

impl RetainedJoint {
    pub(crate) fn reused_rows(&self) -> usize {
        self.reused
    }

    pub(super) fn admit(
        &self,
        template: [u8; 32],
        graphs: &BTreeMap<String, Option<TermValue>>,
        runtimes: &BTreeMap<String, WorldRuntime>,
        effects: &[WorldProducerEffect],
        schedule: &WorldEffectSchedule,
        program: &JointProgram,
    ) -> Reuse {
        let empty = || Reuse {
            by_stratum: BTreeMap::new(),
            registry: SkolemRegistry::new(),
            introductions: BTreeMap::new(),
            count: 0,
        };
        let Some(prior) = &self.state else {
            return empty();
        };
        // A changed theory has no transferable producer proof. Admission still
        // executes the selected new theory; no cached row becomes an assertion.
        if prior.template != template || &prior.graphs != graphs {
            return empty();
        }
        let source: BTreeMap<String, BTreeSet<FactKey>> = runtimes
            .iter()
            .map(|(world, runtime)| (world.clone(), runtime.store.key_set().into_iter().collect()))
            .collect();
        let mut changes = Vec::new();
        for (world, runtime) in runtimes {
            let previous: BTreeSet<_> = prior
                .facts
                .get(world)
                .into_iter()
                .flatten()
                .map(Fact::key)
                .collect();
            changes.extend(
                runtime
                    .store
                    .facts()
                    .iter()
                    .filter(|fact| !previous.contains(&fact.key()))
                    .cloned()
                    .map(|fact| (world.clone(), fact)),
            );
            changes.extend(
                prior
                    .facts
                    .get(world)
                    .into_iter()
                    .flatten()
                    .filter(|fact| !source[world].contains(&fact.key()))
                    .cloned()
                    .map(|fact| (world.clone(), fact)),
            );
        }
        let invalid = crate::physical::effects::invalidated_completed_readers(
            effects,
            &changes,
            program.semantics,
        );
        let mut ranks = BTreeMap::new();
        for (index, effect) in effects.iter().enumerate() {
            let key = (effect.owner.clone(), effect.effect.name().to_owned());
            ranks
                .entry(key)
                .and_modify(|rank: &mut usize| *rank = (*rank).max(schedule.strata[index]))
                .or_insert(schedule.strata[index]);
        }
        // An invalid completed-read producer cannot lend either its selected
        // row or a losing-but-otherwise-valid witness introduction to this fork.
        ranks.retain(|key, _| !invalid.contains(key));
        let mut candidates: Vec<_> = prior
            .rows
            .iter()
            .filter(|row| {
                row.cross_world.is_none()
                    && ranks.contains_key(&(row.graph.clone(), row.rule_iri.clone()))
            })
            .collect();
        candidates.sort_by_key(|row| {
            (
                row.proof_height,
                row.graph.as_str(),
                row.derivation_id.as_str(),
            )
        });
        let mut rejected = BTreeSet::new();
        loop {
            let mut available: BTreeMap<String, BTreeMap<FactKey, usize>> = source
                .iter()
                .map(|(world, facts)| {
                    (
                        world.clone(),
                        facts.iter().cloned().map(|fact| (fact, 0)).collect(),
                    )
                })
                .collect();
            let mut pending: Vec<_> = candidates
                .iter()
                .copied()
                .filter(|row| {
                    !rejected.contains(&(
                        row.graph.clone(),
                        (
                            row.subject.clone(),
                            row.predicate.clone(),
                            row.object.clone(),
                        ),
                    ))
                })
                .collect();
            let mut selected = Vec::new();
            loop {
                let before = pending.len();
                pending.retain(|row| {
                    let Some(known) = available.get_mut(&row.graph) else {
                        return false;
                    };
                    let head = (
                        row.subject.clone(),
                        row.predicate.clone(),
                        row.object.clone(),
                    );
                    if known.contains_key(&head) {
                        return false;
                    }
                    if !row
                        .antecedents
                        .iter()
                        .all(|fact| known.contains_key(&fact.key()))
                    {
                        return true;
                    }
                    known.insert(head, ranks[&(row.graph.clone(), row.rule_iri.clone())]);
                    selected.push((**row).clone());
                    false
                });
                if before == pending.len() {
                    break;
                }
            }
            let derived = selected
                .iter()
                .map(|row| {
                    (
                        row.graph.clone(),
                        (
                            row.subject.clone(),
                            row.predicate.clone(),
                            row.object.clone(),
                        ),
                    )
                })
                .collect();
            let introductions = prior
                .registry
                .retained_introductions(&ranks, &available, &derived);
            let mut introduction_ranks = BTreeMap::new();
            for (rank, receipts) in &introductions {
                for receipt in receipts {
                    introduction_ranks
                        .entry(receipt.witness.as_str())
                        .and_modify(|prior: &mut usize| *prior = (*prior).min(*rank))
                        .or_insert(*rank);
                }
            }
            let before = rejected.len();
            for row in &selected {
                let rank = ranks[&(row.graph.clone(), row.rule_iri.clone())];
                let head = Fact {
                    subject: row.subject.clone(),
                    predicate: row.predicate.clone(),
                    object: row.object.clone(),
                };
                if std::iter::once(&head).chain(&row.antecedents).any(|fact| {
                    prior
                        .registry
                        .introduced_values(fact)
                        .iter()
                        .any(|witness| {
                            introduction_ranks
                                .get(witness)
                                .is_none_or(|ready| *ready > rank)
                        })
                }) {
                    // Recompute this row normally. A proof that becomes valid in
                    // a later stratum must never leak through an earlier budget cut.
                    rejected.insert((row.graph.clone(), head.key()));
                }
            }
            if rejected.len() != before {
                continue;
            }
            let mut by_stratum = BTreeMap::<usize, Vec<DerivedRow>>::new();
            for row in selected {
                by_stratum
                    .entry(ranks[&(row.graph.clone(), row.rule_iri.clone())])
                    .or_default()
                    .push(row);
            }
            return Reuse {
                by_stratum,
                registry: prior.registry.recipes_only(),
                introductions,
                count: 0,
            };
        }
    }

    pub(super) fn capture(
        &mut self,
        template: [u8; 32],
        graphs: BTreeMap<String, Option<TermValue>>,
        facts: BTreeMap<String, Vec<Fact>>,
        rows: Vec<DerivedRow>,
        registry: SkolemRegistry,
        reused: usize,
    ) {
        self.state = Some(Arc::new(Snapshot {
            template,
            graphs,
            facts,
            rows,
            registry,
        }));
        self.reused = reused;
    }
}

impl Reuse {
    pub(super) fn install_registry(&self, registry: &mut SkolemRegistry) {
        *registry = self.registry.clone();
    }

    pub(super) fn count(&self) -> usize {
        self.count
    }

    /// Offer only proofs whose exact premises belonged to this frozen round.
    /// Retained and fresh proofs share the ordinary winner comparison; no cached
    /// head can hide a shorter proof by entering the store before that merge.
    pub(super) fn gather(
        &mut self,
        stratum: usize,
        runtimes: &BTreeMap<String, WorldRuntime>,
        rounds: &mut BTreeMap<String, RoundCandidateBuffer>,
    ) -> gmeow_errors::Result<BTreeMap<String, BTreeSet<FactKey>>> {
        let mut eligible = BTreeMap::<String, BTreeSet<FactKey>>::new();
        let Some(pending) = self.by_stratum.get_mut(&stratum) else {
            return Ok(eligible);
        };
        let mut remaining = Vec::with_capacity(pending.len());
        for row in pending.drain(..) {
            let runtime = runtimes
                .get(&row.graph)
                .ok_or_else(|| super::seminaive_err("retained proof lost its admitted world"))?;
            let fact = Fact {
                subject: row.subject.clone(),
                predicate: row.predicate.clone(),
                object: row.object.clone(),
            };
            let key = fact.key();
            if runtime.store.contains_key(&key) {
                continue;
            }
            if !row
                .antecedents
                .iter()
                .all(|fact| runtime.store.contains_key(&fact.key()))
            {
                remaining.push(row);
                continue;
            }
            let candidate = super::record_candidate(
                &row.rule_iri,
                fact,
                &row.antecedents,
                super::RoundSnapshot {
                    store: &runtime.store,
                    rel: &runtime.rel,
                    depth: &runtime.depth,
                    delta: runtime.delta,
                    mode: super::ProvenanceMode::Record,
                },
            )?;
            rounds
                .get_mut(&row.graph)
                .ok_or_else(|| super::seminaive_err("retained proof lost its round owner"))?
                .insert(key.clone(), candidate, super::ProvenanceMode::Record)?;
            eligible.entry(row.graph.clone()).or_default().insert(key);
            // Publication remains owned by the common writer. A later gather
            // removes this proof only after observing its actually committed head.
            remaining.push(row);
        }
        *pending = remaining;
        Ok(eligible)
    }

    /// Count committed heads for which this round supplied a validated carry
    /// candidate, even when a better fresh proof won their common row merge.
    pub(super) fn record_committed(&mut self, count: usize) {
        self.count += count;
    }

    /// Receipt publication follows actual head commitment, not candidate arrival.
    /// A multi-head or alternative introduction can become ready across rounds;
    /// retain every unready head without exposing any later producer stratum.
    pub(super) fn install_ready_introductions(
        &mut self,
        stratum: usize,
        runtimes: &BTreeMap<String, WorldRuntime>,
        registry: &mut SkolemRegistry,
    ) -> gmeow_errors::Result<()> {
        let reached = self
            .introductions
            .range(..=stratum)
            .map(|(rank, _)| *rank)
            .collect::<Vec<_>>();
        for rank in reached {
            let mut pending = Vec::new();
            let mut ready = Vec::new();
            for mut introduction in self.introductions.remove(&rank).unwrap_or_default() {
                let runtime = runtimes.get(&introduction.scope.world).ok_or_else(|| {
                    super::seminaive_err("retained introduction lost its admitted world")
                })?;
                let (committed, uncommitted): (Vec<_>, Vec<_>) =
                    introduction.heads.into_iter().partition(|head| {
                        std::iter::once(&head.statement)
                            .chain(&head.premises)
                            .all(|statement| {
                                runtime.store.contains_key(&(
                                    statement.subject.clone(),
                                    statement.predicate.clone(),
                                    statement.object.clone(),
                                ))
                            })
                    });
                introduction.heads = committed;
                if !introduction.heads.is_empty() {
                    ready.push(introduction.clone());
                }
                if !uncommitted.is_empty() {
                    introduction.heads = uncommitted;
                    pending.push(introduction);
                }
            }
            registry.install_introductions(&ready, |world, fact| {
                runtimes
                    .get(world)
                    .is_some_and(|runtime| runtime.store.contains_key(fact))
            })?;
            if !pending.is_empty() {
                self.introductions.insert(rank, pending);
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
