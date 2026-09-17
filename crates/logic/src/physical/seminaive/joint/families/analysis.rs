// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! One current analysis per family and execution world. Publication observes the
//! actual completion frontier separately from the already charged computation.

use super::*;
use crate::reason::dl::{NativeConstructAdmission, SourceCoverageWorld};

#[derive(Default)]
pub(super) struct Observations {
    families: BTreeMap<NativeRefutationFamily, Observation>,
}

struct Observation {
    reads: Vec<StatementPattern>,
    preparation: Vec<NativeRead>,
    admitted: Vec<crate::reason::dl::DlConstructFamily>,
    current: Option<Current>,
}

struct Current {
    rows: usize,
    preparation: Vec<bool>,
    admissions: Vec<NativeConstructAdmission>,
    outcomes: Vec<NativeFamilyOutcome>,
}

impl Observation {
    fn new(family: NativeRefutationFamily) -> Self {
        let mut reads = Vec::new();
        let mut preparation = BTreeSet::new();
        for arm in arms()
            .into_iter()
            .filter(|arm| arm.family == Producer::Obligation(family))
        {
            reads.extend(arm.effect.reads.into_iter().map(|(pattern, _)| pattern));
            preparation.extend(arm.preparation);
        }
        Self {
            reads,
            preparation: preparation.into_iter().collect(),
            admitted: admission_families(Producer::Obligation(family)),
            current: None,
        }
    }
}

impl Observations {
    pub(super) fn evaluate(
        &mut self,
        families: &[NativeRefutationFamily],
        input: &NativeFamilyInput<'_>,
        coverage: &SourceCoverageWorld,
        values: &mut crate::physical::SchemaValues,
        lists: &mut crate::physical::LogicalListCache,
        ledger: &mut NativeFamilyLedger,
    ) -> gmeow_errors::Result<()> {
        for family in families {
            let observation = self
                .families
                .entry(*family)
                .or_insert_with(|| Observation::new(*family));
            let preparation = observation
                .preparation
                .iter()
                .map(|read| input.completed(read))
                .collect::<Vec<_>>();
            let admissions = coverage
                .admissions
                .iter()
                .filter(|admission| observation.admitted.contains(&admission.family))
                .collect::<Vec<_>>();
            let reusable = observation.current.as_ref().is_some_and(|current| {
                current.rows <= input.store.row_count()
                    && current.preparation == preparation
                    && current.admissions.iter().eq(admissions.iter().copied())
                    && !input.store.facts()[current.rows..].iter().any(|fact| {
                        observation
                            .reads
                            .iter()
                            .any(|read| read.matches_fact(fact, input.rel.semantics))
                    })
            });
            if !reusable {
                // This view defers only the wildcard observation made by outcome
                // finish. Actual constructor/list/admission reads are unchanged.
                let raw = input.defer_world_completion();
                super::evaluate(&[*family], &raw, values, lists, ledger)?;
                if ledger.work.exhausted
                    && let Some(previous) = &observation.current
                {
                    retain_supported_conclusions(previous, &admissions, input, ledger)?;
                }
                observation.current = Some(Current {
                    rows: input.store.row_count(),
                    preparation,
                    admissions: admissions.into_iter().cloned().collect(),
                    outcomes: ledger
                        .outcomes
                        .iter()
                        .filter(|outcome| outcome.family == *family)
                        .cloned()
                        .collect(),
                });
            }
            let current = observation
                .current
                .as_mut()
                .expect("observed native family");
            current.rows = input.store.row_count();
            ledger.outcomes.retain(|outcome| outcome.family != *family);
            for raw in &current.outcomes {
                let mut outcome = raw.clone();
                observe_completion(&mut outcome, input)?;
                ledger.outcomes.push(outcome);
            }
        }
        ledger.validate()
    }
}

/// A cut can stop before revisiting an older positive clash. Its committed
/// support remains valid in this append-only world; absence and model status do
/// not. Keep only positive conclusions, under the new analysis's actual status.
fn retain_supported_conclusions(
    previous: &Current,
    admissions: &[&NativeConstructAdmission],
    input: &NativeFamilyInput<'_>,
    ledger: &mut NativeFamilyLedger,
) -> gmeow_errors::Result<()> {
    if previous
        .outcomes
        .iter()
        .all(|outcome| outcome.conclusions.is_empty())
    {
        return Ok(());
    }
    if previous.rows > input.store.row_count()
        || ledger.world != input.world()
        || ledger.graph.as_ref() != input.graph()
        || &ledger.input_contract != input.input_contract()
    {
        return Err(super::super::seminaive_err(
            "retained family conclusions changed their native execution scope",
        ));
    }
    // Positive constructor conclusions require already completed definitions.
    // New owners are allowed; changing an admitted owner violates the prepared
    // grammar frontier and cannot lend its old semantic interpretation.
    if previous.admissions.iter().any(|prior| {
        prior.completion == NativeFamilyCompletion::Complete
            && !admissions.iter().any(|current| *current == prior)
    }) {
        return Err(super::super::seminaive_err(
            "retained family conclusion lost its completed source admission",
        ));
    }
    let proofs = ledger
        .proofs
        .iter()
        .map(|proof| (proof.id, &proof.statement))
        .collect::<BTreeMap<_, _>>();
    for prior in &previous.outcomes {
        for conclusion in &prior.conclusions {
            if conclusion.support.is_empty()
                || conclusion
                    .support
                    .iter()
                    .chain(conclusion.committed.iter())
                    .any(|id| {
                        proofs.get(id).is_none_or(|statement| {
                            !input.store.contains_key(&(
                                statement.subject.clone(),
                                statement.predicate.clone(),
                                statement.object.clone(),
                            ))
                        })
                    })
            {
                return Err(super::super::seminaive_err(
                    "retained family conclusion lost its exact committed support",
                ));
            }
            let index = match ledger.outcomes.iter().position(|outcome| {
                outcome.family == prior.family && outcome.obligation == prior.obligation
            }) {
                Some(index) => index,
                None => {
                    let mut outcome =
                        NativeFamilyOutcome::new(prior.family, prior.obligation.clone());
                    outcome.completion = NativeFamilyCompletion::Exhausted;
                    ledger.outcomes.push(outcome);
                    ledger.outcomes.len() - 1
                }
            };
            let current = &mut ledger.outcomes[index].conclusions;
            if let Some(existing) = current.iter_mut().find(|existing| {
                existing.subject == conclusion.subject
                    && existing.rule == conclusion.rule
                    && existing.support == conclusion.support
            }) {
                if existing.committed.is_none() {
                    existing.committed = conclusion.committed;
                }
            } else {
                current.push(conclusion.clone());
            }
        }
    }
    Ok(())
}

fn observe_completion(
    outcome: &mut NativeFamilyOutcome,
    input: &NativeFamilyInput<'_>,
) -> gmeow_errors::Result<()> {
    let world = NativeRead {
        marker: None,
        predicate: None,
        kind: NativeReadKind::Completed,
    };
    let mut pending = match &outcome.completion {
        NativeFamilyCompletion::Complete | NativeFamilyCompletion::NotEngaged => Vec::new(),
        NativeFamilyCompletion::Awaiting { reads } => {
            // Such reads affected the computation itself. Their completion must
            // invalidate the observation, never silently certify skipped work.
            if reads.iter().any(|read| input.completed(read)) {
                return Err(super::super::seminaive_err(
                    "cached family preparation changed outside its declared reads",
                ));
            }
            reads.clone()
        }
        _ => return Ok(()),
    };
    if !input.completed(&world) {
        pending.push(world);
    }
    if !pending.is_empty() {
        pending.sort();
        pending.dedup();
        outcome.completion = NativeFamilyCompletion::Awaiting { reads: pending };
    }
    Ok(())
}

#[cfg(test)]
mod tests;
