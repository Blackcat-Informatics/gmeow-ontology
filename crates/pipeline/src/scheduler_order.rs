// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Memory preferences over the dependency-ready, resource-eligible frontier.

use std::cmp::Reverse;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::sync::Arc;

use crate::bundle::PipelineHandle;
use crate::graph::StageGraph;
use crate::node::{Stage, StageProduct};

use super::HandleReaders;

#[derive(Default)]
pub(crate) enum ReadyOrder {
    #[default]
    StageId,
    Calibrate,
    Memory(Option<BTreeMap<String, StageEstimate>>),
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct StageEstimate {
    transient_bytes: u64,
    published_bytes: u64,
}

impl ReadyOrder {
    pub(crate) fn from_environment() -> gmeow_errors::Result<Self> {
        let profile = std::env::var_os("GMEOW_PIPELINE_MEMORY_ESTIMATES");
        match std::env::var_os("GMEOW_PIPELINE_READY_ORDER").as_deref() {
            None => {}
            Some(mode) if mode == "memory" => {}
            Some(mode) if mode == "stage-id" && profile.is_none() => return Ok(Self::StageId),
            Some(mode) if mode == "calibrate" && profile.is_none() => return Ok(Self::Calibrate),
            Some(mode) if mode == "stage-id" || mode == "calibrate" => {
                return Err(invalid_order(format!(
                    "{mode:?} order cannot use memory estimates"
                )));
            }
            Some(mode) => {
                return Err(invalid_order(format!("unknown ready order {mode:?}")));
            }
        }
        let estimates = profile
            .as_deref()
            .map(Path::new)
            .map(|path| {
                let file = std::fs::File::open(path).map_err(|error| {
                    invalid_order(format!("open memory estimates {}: {error}", path.display()))
                })?;
                serde_json::from_reader(std::io::BufReader::new(file)).map_err(|error| {
                    invalid_order(format!("read memory estimates {}: {error}", path.display()))
                })
            })
            .transpose()?;
        Ok(Self::Memory(estimates))
    }

    /// Calibration isolates one stage without reducing its Rayon pool.
    pub(crate) const fn stage_limit(&self, jobs: usize) -> usize {
        match self {
            Self::Calibrate => 1,
            Self::StageId | Self::Memory(_) => jobs,
        }
    }

    pub(super) fn validate(&self, selected: &BTreeSet<String>) -> gmeow_errors::Result<()> {
        if let Self::Memory(Some(estimates)) = self {
            let stages: BTreeSet<_> = estimates.keys().cloned().collect();
            if &stages != selected {
                return Err(invalid_order(format!(
                    "memory estimates must cover the selected DAG. Missing: {:?}. Unexpected: {:?}",
                    selected.difference(&stages).collect::<Vec<_>>(),
                    stages.difference(selected).collect::<Vec<_>>()
                )));
            }
        }
        Ok(())
    }
}

fn invalid_order(message: impl Into<String>) -> gmeow_errors::Diag {
    gmeow_errors::Diag::of_kind(crate::error::InvalidDag {
        message: message.into(),
    })
}

pub(super) fn critical_depth(
    graph: &StageGraph,
    dependents: &BTreeMap<String, BTreeSet<String>>,
) -> BTreeMap<String, usize> {
    let mut depths = BTreeMap::new();
    for level in graph.levels.iter().rev() {
        for id in level {
            let height = dependents
                .get(id)
                .into_iter()
                .flatten()
                .map(|consumer| depths[consumer])
                .max()
                .unwrap_or(0);
            depths.insert(id.clone(), height + 1);
        }
    }
    depths
}

pub(super) struct LiveReads<'a> {
    pub(super) products: &'a BTreeMap<String, StageProduct>,
    pub(super) dataset_bytes: &'a BTreeMap<String, u64>,
    pub(super) carrier_readers: &'a BTreeMap<String, usize>,
    pub(super) handle_readers: Option<&'a BTreeMap<String, HandleReaders>>,
    pub(super) artifact_views: &'a BTreeMap<String, usize>,
    pub(super) retained: &'a BTreeSet<String>,
}

impl LiveReads<'_> {
    fn released_payload_bytes(&self, stage: &dyn Stage) -> u64 {
        let dead = |id: &str| {
            stage
                .carrier_consumes()
                .iter()
                .any(|producer| producer == id)
                && self.carrier_readers.get(id) == Some(&1)
                && self.artifact_views.get(id).copied().unwrap_or(0) == 0
                && !self.retained.contains(id)
        };
        let completing_reads = self.handle_readers.map(|_| stage.reads());
        let mut credited = BTreeSet::new();
        let mut bytes = 0_u64;
        for id in stage.carrier_consumes() {
            let Some(product) = self.products.get(id) else {
                continue;
            };
            if self.retained.contains(id) {
                continue;
            }
            let carrier_dead = dead(id);
            let bundle = product.raw_bundle();
            if Arc::strong_count(bundle) == 1 && (carrier_dead || Arc::weak_count(bundle) == 0) {
                let readers = self.handle_readers.and_then(|readers| readers.get(id));
                let completed = completing_reads.as_ref().and_then(|reads| reads.get(id));
                for (key, entry) in bundle.handles() {
                    if !carrier_dead && readers.is_none_or(|r| r.contains_after(key, completed)) {
                        continue;
                    }
                    if let PipelineHandle::SourceCatalog(catalog) = &entry.payload {
                        let aggregate = catalog.materialized();
                        if Arc::strong_count(catalog) == 1 && Arc::strong_count(aggregate) == 1 {
                            // The receipt graph omits the independently owned source aggregate.
                            bytes = bytes.saturating_add(
                                u64::try_from(aggregate.rdf_payload_bytes()).unwrap_or(u64::MAX),
                            );
                        }
                    }
                }
            }
            if !carrier_dead {
                continue;
            }
            let dataset = bundle.dataset();
            if !credited.insert(std::ptr::from_ref(dataset)) {
                continue;
            }
            let still_live = self.products.iter().any(|(other, product)| {
                !product.carrier_released
                    && std::ptr::eq(dataset, product.raw_bundle().dataset())
                    && !dead(other)
            });
            if !still_live {
                bytes = bytes.saturating_add(self.dataset_bytes.get(id).copied().unwrap_or(0));
            }
        }
        bytes
    }

    pub(super) fn priority(
        &self,
        stage: &dyn Stage,
        estimate: Option<&StageEstimate>,
        terminal_only: bool,
        depth: usize,
    ) -> (u64, i128, bool, Reverse<usize>) {
        let transient = estimate.map_or(0, |estimate| estimate.transient_bytes);
        let published = estimate.map_or(0, |estimate| estimate.published_bytes);
        // Inputs coexist with the stage work and its publication. Credit a
        // release only when comparing the frontier after completion.
        let frontier_growth =
            i128::from(published) - i128::from(self.released_payload_bytes(stage));
        (
            transient.max(published),
            frontier_growth,
            terminal_only,
            Reverse(depth),
        )
    }
}
