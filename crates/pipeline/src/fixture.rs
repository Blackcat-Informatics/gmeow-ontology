// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Exact, cross-process fixtures for tests that assert on expensive pipeline output.
//!
//! A fixture is not a second producer. It binds the production DAG, executes the exact
//! dependency closure for the selected production stage, derives the scheduler's action
//! key over those products, and elects one process to run that same stage implementation.
//! The resulting product is admitted through the production structural cache and receipt
//! checks. A cache miss always recomputes; missing inputs and semantic drift hard-fail.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use crate::cache::{
    FixtureCoordinator, FixtureOutcome, PipelineCache, StageKeyContext, StageReceipt,
};
use crate::loader::bind;
use crate::node::{CachePolicy, StageInput, StageProduct, StageStability};
use crate::registry::default_registry;
use crate::run::full_spec;
use crate::scheduler::{
    RunContext, action_key_context, action_key_context_from_receipts, dependency_closure,
    run_targets, verify_attach_drift,
};

/// One production-stage fixture and the exact direct upstream products it consumed.
///
/// Keeping the upstream map lets structural tests inspect the same fresh surfaces that
/// produced the cached output without invoking either producer again.
#[derive(Debug)]
pub struct StageFixture {
    /// The verified stage product, receipt, and build/hydration telemetry.
    pub outcome: FixtureOutcome,
    /// Exact direct upstream products named by `Stage::consumes()`.
    pub upstream: BTreeMap<String, StageProduct>,
}

/// Materialize one real stable/persistent stage action across concurrent test processes.
///
/// The selected action is derived from the authored production DAG. Its dependencies run
/// through the normal scheduler, while the selected node is guarded by a
/// blocking per-action election and stored under the same typed key and receipt laws.
pub fn stage_fixture(
    root: &Path,
    jobs: usize,
    stage_id: &str,
) -> Result<StageFixture, gmeow_errors::Diag> {
    let spec = full_spec();
    let graph = spec.validate()?;
    let bound = bind(&spec, &graph, &default_registry())?;
    let stage = bound
        .iter()
        .find(|stage| stage.id() == stage_id)
        .cloned()
        .ok_or_else(|| {
            gmeow_errors::Diag::of_kind(crate::error::StageFailed {
                stage: stage_id.to_string(),
                message: format!("production DAG does not bind {stage_id}"),
            })
        })?;
    if stage.stability() != StageStability::StablePrefix
        || stage.cache_policy() != CachePolicy::Persistent
    {
        return Err(gmeow_errors::Diag::of_kind(crate::error::StageFailed {
            stage: stage.id().to_string(),
            message: format!(
                "cross-process fixture requires stable persistent admission, got {} / {}",
                stage.stability().iri(),
                stage.cache_policy().iri()
            ),
        }));
    }

    let direct_dependencies: BTreeSet<String> = stage.consumes().iter().cloned().collect();
    let mut context = RunContext::open(root, jobs)?;
    let upstream: BTreeMap<String, StageProduct> = if direct_dependencies.is_empty() {
        BTreeMap::new()
    } else {
        let dependency_run = run_targets(&graph, &bound, &mut context, &direct_dependencies)?;
        stage
            .consumes()
            .iter()
            .map(|producer| {
                dependency_run
                    .products
                    .get(producer)
                    .cloned()
                    .map(|product| (producer.clone(), product))
                    .ok_or_else(|| {
                        gmeow_errors::Diag::of_kind(crate::error::StageFailed {
                            stage: stage.id().to_string(),
                            message: format!(
                                "dependency closure did not return direct producer {producer}"
                            ),
                        })
                    })
            })
            .collect::<Result<_, _>>()?
    };
    let action = action_key_context(stage.as_ref(), root, &upstream)?;
    let coordinator = FixtureCoordinator::open(root)?;
    let outcome = coordinator.get_or_build(
        &action,
        stage.stability().iri(),
        stage.cache_policy().iri(),
        |product| verify_attach_drift(stage.as_ref(), &upstream, product),
        || {
            stage
                .run(StageInput {
                    root,
                    upstream: &upstream,
                })
                .map(|output| output.product)
        },
    )?;
    Ok(StageFixture { outcome, upstream })
}

/// A mappings fixture and the exact direct upstream products it consumed.
pub type MappingsFixture = StageFixture;

/// Materialize the real `stage-mappings` action once across concurrent test processes.
pub fn mappings_fixture(root: &Path, jobs: usize) -> Result<MappingsFixture, gmeow_errors::Diag> {
    stage_fixture(root, jobs, "stage-mappings")
}

/// Resolve one production stage's current action and receipt using only authenticated
/// upstream receipts. Any cache miss returns `None`; callers then run the exact producer
/// through [`stage_fixture`] rather than accepting an incomplete result.
fn current_stage_receipt(
    root: &Path,
    bound: &[std::sync::Arc<dyn crate::node::Stage>],
    cache: &PipelineCache,
    stage_id: &str,
) -> Result<Option<(StageKeyContext, StageReceipt)>, gmeow_errors::Diag> {
    let closure = dependency_closure(bound, &BTreeSet::from([stage_id.to_string()]))?;
    let mut receipts: BTreeMap<String, StageReceipt> = BTreeMap::new();
    for stage in bound {
        if !closure.contains(stage.id()) {
            continue;
        }
        if stage.stability() != StageStability::StablePrefix
            || stage.cache_policy() != CachePolicy::Persistent
        {
            return Ok(None);
        }
        let upstream: BTreeMap<String, StageReceipt> = stage
            .consumes()
            .iter()
            .map(|producer| {
                receipts
                    .get(producer)
                    .cloned()
                    .map(|receipt| (producer.clone(), receipt))
                    .ok_or_else(|| {
                        gmeow_errors::Diag::of_kind(crate::error::StageFailed {
                            stage: stage.id().to_string(),
                            message: format!(
                                "receipt-only dependency walk lacks producer {producer}"
                            ),
                        })
                    })
            })
            .collect::<Result<_, _>>()?;
        let context = action_key_context_from_receipts(stage.as_ref(), root, &upstream)?;
        let Some(receipt) = cache.inspect_receipt(&context)? else {
            return Ok(None);
        };
        receipts.insert(stage.id().to_string(), receipt);
    }
    Ok(receipts.remove(stage_id).map(|receipt| {
        let context = receipt.context.clone();
        (context, receipt)
    }))
}

/// Materialize one production stage's committed artifact lane.
///
/// A warm action authenticates the same receipt/product blob as the scheduler but does
/// not restore the packed RDF dataset. A miss delegates to [`stage_fixture`], so missing
/// cache state always recomputes the complete selected operation.
pub fn stage_artifacts(
    root: &Path,
    jobs: usize,
    stage_id: &str,
) -> Result<BTreeMap<String, Vec<u8>>, gmeow_errors::Diag> {
    let spec = full_spec();
    let graph = spec.validate()?;
    let bound = bind(&spec, &graph, &default_registry())?;
    if !bound.iter().any(|stage| stage.id() == stage_id) {
        return Err(gmeow_errors::Diag::of_kind(crate::error::StageFailed {
            stage: stage_id.to_string(),
            message: format!("production DAG does not bind {stage_id}"),
        }));
    }
    // The producer fingerprint is already a typed action-key field. Opening a
    // fingerprint-suffixed directory here would create a reader-only namespace,
    // miss every receipt written by the scheduler/primer, and force full product
    // hydration on every artifact-only fixture request.
    let cache = PipelineCache::open_default(root)?;
    if let Some((context, resolved_receipt)) =
        current_stage_receipt(root, &bound, &cache, stage_id)?
        && let Some(hit) = cache.get_artifacts(&context)?
    {
        if hit.receipt != resolved_receipt {
            return Err(gmeow_errors::Diag::of_kind(crate::error::CacheMismatch {
                expected: resolved_receipt.digest(),
                actual: hit.receipt.digest(),
            }));
        }
        return Ok(hit.artifacts);
    }
    Ok(stage_fixture(root, jobs, stage_id)?
        .outcome
        .product
        .artifacts())
}

/// Materialize and return the mappings stage's committed artifact lane.
pub fn mapping_artifacts(
    root: &Path,
    jobs: usize,
) -> Result<BTreeMap<String, Vec<u8>>, gmeow_errors::Diag> {
    stage_artifacts(root, jobs, "stage-mappings")
}
