// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Isolated synthetic-family helpers; production uses the native joint engine.
use super::*;

/// Fixed ceiling only for the source-level synthetic assertion helper. Production
/// analysis consumes the native ledger's selected shared allowance.
pub(super) const SEARCH_BUDGET: u64 = 400_000;

/// Synthetic tests observe the same class algorithm without production execution.
pub(crate) fn decide(edb: &impl DatasetView) -> Option<RefutationCertificate> {
    decide_scan(&Scan::of(edb))
}

/// Aggregate the same prepared source analysis for legacy synthetic expectations.
pub(super) fn decide_scan(scan: &Scan) -> Option<RefutationCertificate> {
    if !scan.engages() {
        return None;
    }

    let mut clashes: BTreeSet<NothingClash> = BTreeSet::new();
    let mut conflicts = Vec::new();
    let mut source_boundaries = Vec::new();
    let mut counted: BTreeSet<String> = BTreeSet::new();
    let mut obstructions: BTreeSet<String> = BTreeSet::new();

    for world in scan.worlds.keys() {
        match scan.run_world(world) {
            WorldOutcome::Inconsistent(proof) => {
                let conflict = ContextualConflict {
                    world: world.clone(),
                    proof,
                };
                for c in conflict.local_clashes(RULE_CASESPLIT) {
                    counted.insert(c.individual.clone());
                    clashes.insert(c);
                }
                conflicts.push(conflict);
            }
            WorldOutcome::SourceBoundary(boundary) => source_boundaries.push(boundary),
            WorldOutcome::Consistent => {}
            WorldOutcome::OutOfFragment(reason) => {
                obstructions.insert(reason);
            }
            WorldOutcome::SearchBoundary(bound) => {
                obstructions.insert(bound.detail());
            }
        }
    }

    // A supported conflict remains true in its own selected world. Other worlds'
    // admission/completion boundaries remain separate evidence; this never makes
    // their input well formed or their reasoning complete.
    if !obstructions.is_empty() && (!conflicts.is_empty() || !source_boundaries.is_empty()) {
        source_boundaries.push(FragmentBoundary::Uncertified {
            family: FragmentFamily::CaseSplit,
            obstructions: std::mem::take(&mut obstructions),
        });
    }
    if !conflicts.is_empty() {
        return Some(certify_membership(
            FragmentFamily::CaseSplit,
            BTreeSet::new(),
            move || {
                (
                    Decision::Inconsistent,
                    Witness {
                        family: FragmentFamily::CaseSplit,
                        clashes,
                        evidence: WitnessEvidence {
                            contextual_conflicts: conflicts,
                            source_boundaries,
                            counted_individuals: counted,
                            violated_bound: None,
                            closed_branch: Some("all-branches-closed".to_owned()),
                        },
                    },
                )
            },
        ));
    }

    if !source_boundaries.is_empty() {
        let reason = if source_boundaries.len() == 1 {
            source_boundaries.pop().expect("one boundary")
        } else {
            FragmentBoundary::Combined(source_boundaries.into_iter().collect())
        };
        return Some(RefutationCertificate::OutOfFragment { reason });
    }

    // No clash anywhere — a `Consistent` verdict requires EVERY world to have
    // saturated clash-free inside the certified-complete fragment.
    Some(certify_membership(
        FragmentFamily::CaseSplit,
        obstructions,
        || {
            (
                Decision::Consistent,
                Witness {
                    family: FragmentFamily::CaseSplit,
                    clashes: BTreeSet::new(),
                    evidence: WitnessEvidence::default(),
                },
            )
        },
    ))
}

/// Synthetic completion assertion; never used by production coverage selection.
pub(crate) fn decides(edb: &impl DatasetView) -> bool {
    matches!(decide(edb), Some(RefutationCertificate::InFragment { witness, .. }) if witness.evidence.source_boundaries.is_empty())
}

impl Scan {
    pub(super) fn run_world(&self, world: &str) -> WorldOutcome {
        {
            let mut budget = SEARCH_BUDGET;
            self.run_world_budget(world, &mut budget).0
        }
    }
}
