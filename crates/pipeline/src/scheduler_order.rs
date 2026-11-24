// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Memory preferences over the dependency-ready, resource-eligible frontier.

use std::cmp::Reverse;
use std::collections::{BTreeMap, BTreeSet};
use std::io::Read;
use std::num::NonZeroU64;
use std::path::Path;
use std::sync::Arc;

use gmeow_errors::json::read_unsigned;
use purrdf_lex::json::record::{DecodeError, FromJson, Record};
use purrdf_lex::json::{Limits, Value, read_slice};

use crate::bundle::PipelineHandle;
use crate::graph::StageGraph;
use crate::node::{Stage, StageProduct};

use super::PayloadReaders;

const ACTIVE_WORK_BYTES_ENV: &str = "GMEOW_PIPELINE_ACTIVE_WORK_BYTES";
const MAX_ESTIMATE_PROFILE_BYTES: u64 = 1024 * 1024;

#[derive(Default)]
pub(crate) enum ReadyOrder {
    #[default]
    StageId,
    Calibrate,
    Memory(Option<MemorySchedule>),
}

pub(crate) struct MemorySchedule {
    pub(super) estimates: BTreeMap<String, StageEstimate>,
    ceiling: Option<NonZeroU64>,
}

pub(super) struct StageEstimate {
    transient_bytes: u64,
    published_bytes: u64,
}

impl FromJson for StageEstimate {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "stage memory estimate")?;
        let estimate = Self {
            transient_bytes: fields
                .required_with("transient_bytes", |value| read_unsigned(value, "u64"))?,
            published_bytes: fields
                .required_with("published_bytes", |value| read_unsigned(value, "u64"))?,
        };
        fields.deny_unknown()?;
        Ok(estimate)
    }
}

impl StageEstimate {
    pub(super) fn active_work_bytes(&self) -> u64 {
        self.transient_bytes.max(self.published_bytes)
    }
}

impl ReadyOrder {
    pub(crate) fn active_work_is_requested() -> bool {
        std::env::var_os(ACTIVE_WORK_BYTES_ENV).is_some()
    }

    pub(crate) fn from_environment() -> gmeow_errors::Result<Self> {
        let profile = std::env::var_os("GMEOW_PIPELINE_MEMORY_ESTIMATES");
        let active_work_ceiling = std::env::var_os(ACTIVE_WORK_BYTES_ENV)
            .map(|value| {
                value
                    .to_str()
                    .and_then(|text| text.parse::<NonZeroU64>().ok())
                    .ok_or_else(|| {
                        invalid_order(
                            "GMEOW_PIPELINE_ACTIVE_WORK_BYTES must be a positive byte count",
                        )
                    })
            })
            .transpose()?;
        match std::env::var_os("GMEOW_PIPELINE_READY_ORDER").as_deref() {
            None => {}
            Some(mode) if mode == "memory" => {}
            Some(mode)
                if mode == "stage-id" && profile.is_none() && active_work_ceiling.is_none() =>
            {
                return Ok(Self::StageId);
            }
            Some(mode)
                if mode == "calibrate" && profile.is_none() && active_work_ceiling.is_none() =>
            {
                return Ok(Self::Calibrate);
            }
            Some(mode) if mode == "stage-id" || mode == "calibrate" => {
                return Err(invalid_order(format!(
                    "{mode:?} order cannot use memory estimates or an active-work ceiling"
                )));
            }
            Some(mode) => {
                return Err(invalid_order(format!("unknown ready order {mode:?}")));
            }
        }
        let Some(path) = profile.as_deref().map(Path::new) else {
            if active_work_ceiling.is_some() {
                return Err(invalid_order(
                    "GMEOW_PIPELINE_ACTIVE_WORK_BYTES requires GMEOW_PIPELINE_MEMORY_ESTIMATES",
                ));
            }
            return Ok(Self::Memory(None));
        };
        let mut bytes = Vec::new();
        std::fs::File::open(path)
            .and_then(|file| {
                file.take(MAX_ESTIMATE_PROFILE_BYTES + 1)
                    .read_to_end(&mut bytes)
            })
            .map_err(|error| {
                invalid_order(format!("read memory estimates {}: {error}", path.display()))
            })?;
        if bytes.len() as u64 > MAX_ESTIMATE_PROFILE_BYTES {
            return Err(invalid_order(format!(
                "memory estimates {} exceed the {MAX_ESTIMATE_PROFILE_BYTES}-byte profile limit",
                path.display()
            )));
        }
        Self::from_estimate_json(&bytes, active_work_ceiling).map_err(|error| {
            invalid_order(format!(
                "decode memory estimates {}: {error}",
                path.display()
            ))
        })
    }

    pub(crate) fn from_estimate_json(
        bytes: &[u8],
        ceiling: Option<NonZeroU64>,
    ) -> Result<Self, DecodeError> {
        let value = read_slice(bytes, Limits::DEFAULT)?;
        if let Some(duplicate) = value
            .as_object()
            .and_then(|object| object.first_duplicate())
        {
            return Err(DecodeError::duplicate_field(duplicate));
        }
        let estimates = BTreeMap::from_json(&value)?;
        Ok(Self::Memory(Some(MemorySchedule { estimates, ceiling })))
    }

    /// Calibration isolates one stage without reducing its Rayon pool.
    pub(crate) const fn stage_limit(&self, jobs: usize) -> usize {
        match self {
            Self::Calibrate => 1,
            Self::StageId | Self::Memory(_) => jobs,
        }
    }

    pub(super) fn active_work_admission(&self) -> Option<(u64, &BTreeMap<String, StageEstimate>)> {
        let Self::Memory(Some(schedule)) = self else {
            return None;
        };
        schedule
            .ceiling
            .map(|ceiling| (ceiling.get(), &schedule.estimates))
    }

    pub(crate) fn validate(&self, selected: &BTreeSet<String>) -> gmeow_errors::Result<()> {
        let Self::Memory(Some(schedule)) = self else {
            return Ok(());
        };
        let stages: BTreeSet<_> = schedule.estimates.keys().cloned().collect();
        if &stages != selected {
            return Err(invalid_order(format!(
                "memory estimates must cover the selected DAG. Missing: {:?}. Unexpected: {:?}",
                selected.difference(&stages).collect::<Vec<_>>(),
                stages.difference(selected).collect::<Vec<_>>()
            )));
        }
        if let Some(ceiling) = schedule.ceiling
            && let Some((stage, estimate)) = schedule
                .estimates
                .iter()
                .find(|(_, estimate)| estimate.active_work_bytes() > ceiling.get())
        {
            return Err(invalid_order(format!(
                "{stage} requires {} estimated active-work bytes, above the ceiling of {ceiling} bytes",
                estimate.active_work_bytes(),
            )));
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
    pub(super) payload_readers: Option<&'a BTreeMap<String, PayloadReaders>>,
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
        let completing_reads = self.payload_readers.map(|_| stage.reads());
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
                let readers = self.payload_readers.and_then(|readers| readers.get(id));
                let completed = completing_reads.as_ref().and_then(|reads| reads.get(id));
                for (key, entry) in bundle.handles() {
                    if !carrier_dead && readers.is_none_or(|r| r.keeps_handle_after(key, completed))
                    {
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
        let published = estimate.map_or(0, |estimate| estimate.published_bytes);
        // Inputs coexist with the stage work and its publication. Credit a
        // release only when comparing the frontier after completion.
        let frontier_growth =
            i128::from(published) - i128::from(self.released_payload_bytes(stage));
        (
            estimate.map_or(0, StageEstimate::active_work_bytes),
            frontier_growth,
            terminal_only,
            Reverse(depth),
        )
    }
}
