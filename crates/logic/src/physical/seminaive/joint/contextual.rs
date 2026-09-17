// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Atomic publication of already-costed contextual judgments at the common
//! frozen-round boundary. Receipt ownership survives statement deduplication.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use super::{WorldRuntime, commit_round, seminaive_err};
use crate::contextual::native::{NativeContextualBatch, NativeContextualReceipt};
use crate::modal::native::NativeCrossWorldEvidence;
use crate::physical::{SkolemRegistry, StepGovernor, WitnessStatement};
use crate::provenance::{MinProofHeightSemiring, ProofHeight};
use crate::rule_ir::{Fact, Provenance, RuleRoundCandidate};

/// Validate every complete group before publishing any of its rows. The only
/// budget charge belongs to the evaluator; this metadata cannot be cut in half.
pub(super) fn publish(
    batch: NativeContextualBatch,
    worlds: &mut BTreeMap<String, WorldRuntime>,
    registry: &SkolemRegistry,
    governor: &mut StepGovernor,
) -> gmeow_errors::Result<usize> {
    let mut publications = BTreeMap::new();
    for receipt in batch.receipts {
        receipt.validate()?;
        let provenance = provenance(&receipt, worlds)?;
        if publications
            .insert(receipt.id, (receipt, provenance, BTreeSet::new()))
            .is_some()
        {
            return Err(seminaive_err(
                "contextual batch repeats an assessment receipt",
            ));
        }
    }
    for candidate in batch.candidates {
        let Some((receipt, _, heads)) = publications.get_mut(&candidate.receipt.id) else {
            return Err(seminaive_err(
                "contextual candidate has no selected assessment receipt",
            ));
        };
        if !Arc::ptr_eq(receipt, &candidate.receipt) {
            return Err(seminaive_err(
                "contextual candidate changed its shared assessment receipt",
            ));
        }
        let statement = WitnessStatement::from(&candidate.head);
        receipt.validate_head(&candidate.owner, &statement)?;
        if !heads.insert(statement) {
            return Err(seminaive_err(
                "contextual publication repeats a metadata statement",
            ));
        }
    }
    for (receipt, _, heads) in publications.values() {
        if heads.iter().ne(receipt.statements.iter()) {
            return Err(seminaive_err(
                "contextual batch omits required assessment metadata",
            ));
        }
    }
    if publications.is_empty() {
        return Ok(0);
    }
    let world = worlds
        .get_mut(crate::result_rdf::GRAPH_REASONING)
        .ok_or_else(|| seminaive_err("contextual output world was not admitted"))?;
    let before = world.rows.len();
    for (receipt, provenance, heads) in publications.into_values() {
        let entries = heads
            .into_iter()
            .map(|statement| {
                let head = Fact {
                    subject: statement.subject,
                    predicate: statement.predicate,
                    object: statement.object,
                };
                (
                    head.key(),
                    RuleRoundCandidate {
                        head,
                        prov: Some(provenance.clone()),
                    },
                )
            })
            .collect();
        if commit_round(entries, &mut world.fixpoint(), governor, &BTreeSet::new())?
            != super::FixpointStatus::Complete
        {
            return Err(seminaive_err(
                "already-costed contextual metadata consumed inference allowance",
            ));
        }
        world
            .family
            .observe_committed(&world.store, &world.rows, registry)?;
        world.native_snapshot()?.retain_contextual(&receipt)?;
        world.family.index_proofs();
    }
    Ok(world.rows.len() - before)
}

/// Foreign support heights are computed once per receipt, never once per
/// projected metadata row. All heads share the same authenticated judgment.
fn provenance(
    receipt: &Arc<NativeContextualReceipt>,
    worlds: &BTreeMap<String, WorldRuntime>,
) -> gmeow_errors::Result<Provenance> {
    let mut max_height = ProofHeight::ASSERTED;
    let mut total_height = 0u64;
    for ((owner, statement), support) in receipt.premises.iter().zip(&receipt.supports) {
        let world = worlds
            .get(owner)
            .ok_or_else(|| seminaive_err("contextual support world is absent"))?;
        let proof = world
            .family
            .recorded_proof(&support.proof)
            .ok_or_else(|| seminaive_err("contextual support proof was not recorded"))?;
        if support.world != *owner || proof.statement != *statement {
            return Err(seminaive_err(
                "contextual support changed its actual source statement",
            ));
        }
        let fact = Fact {
            subject: statement.subject.clone(),
            predicate: statement.predicate.clone(),
            object: statement.object.clone(),
        };
        let row = world
            .store
            .row_index(&fact.key())
            .ok_or_else(|| seminaive_err("contextual premise is not committed"))?;
        let height = *world
            .depth
            .get(row)
            .ok_or_else(|| seminaive_err("contextual premise lacks proof height"))?;
        max_height = max_height.max(height);
        total_height = total_height.saturating_add(u64::from(height.get()));
    }
    let sources = receipt.source_quad_ids();
    let mut sorted_sources = sources.clone();
    sorted_sources.sort();
    Ok(Provenance {
        sources,
        sorted_sources,
        deriv: receipt.derivation_id(),
        rule_iri: crate::contextual::RULE_IRI.to_owned(),
        proof_height: MinProofHeightSemiring.derive([max_height])?,
        sum_src_depth: total_height,
        source_facts: Vec::new(),
        cross_world: Some(NativeCrossWorldEvidence::Contextual(Arc::clone(receipt))),
    })
}
