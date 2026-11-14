// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The per-stage memory record: what the process and one stage product held at each
//! scheduler boundary and each measured phase boundary of a pipeline run.
//!
//! Every record is an observation. None enters a product, a cache key, a receipt or a
//! digest. The scheduler returns the records of a run that finishes in
//! [`RunResult::memory`](crate::scheduler::RunResult::memory). While the [`TARGET`]
//! target is enabled, each record is also written as it is made, as one `tracing`
//! event, so a run that is killed part-way leaves the records of every boundary it
//! reached. The target controls only that emission, never what a record holds.
//!
//! # Which boundaries make a record
//!
//! * The scheduler makes one at each stage's dispatch, publication and carrier release.
//! * A stage body makes one at the start and one at the end of each phase it runs
//!   through [`measure_stage_phase`](crate::node::measure_stage_phase), and the snapshot
//!   serializer makes one at the end of each of its timed phases.
//!
//! A phase that is only logged makes no record of its own. That includes every phase
//! the `gmeow-logic` crate logs on the `pipeline_phase` target, the native receipt
//! encode among them; those fall inside the enclosing measured stage phase.
//!
//! # Cost
//!
//! Each record reads `/proc/self/status` once (a few kilobytes) and allocates its stage
//! and phase names. A run makes at most three scheduler records per executed stage plus
//! two per measured phase execution, so the number of reads is
//! `3 × stages + 2 × measured phase executions`. A stage that is never released makes
//! two scheduler records. On a host without Linux procfs the read fails and the two
//! process figures are absent.

use std::cell::RefCell;
use std::sync::atomic::{AtomicU64, Ordering};

use purrdf::PipelineBundle;

use crate::bundle::PipelineHandle;

/// The `tracing` target every memory record is emitted on.
pub(crate) const TARGET: &str = gmeow_cli_core::STAGE_MEMORY_TARGET;

/// This process's resident memory as the kernel reports it, in KiB.
///
/// A value is absent on a host without Linux procfs and when the status text does not
/// carry the field. The peak is that of the whole process lifetime, not of one stage.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ProcessMemory {
    /// Current resident set size (`VmRSS`).
    pub rss_kib: Option<u64>,
    /// Peak resident set size since process start (`VmHWM`).
    pub peak_rss_kib: Option<u64>,
}

impl ProcessMemory {
    /// Read this process's resident and peak resident size from `/proc/self/status`.
    #[must_use]
    pub(crate) fn sample() -> Self {
        std::fs::read_to_string("/proc/self/status")
            .map(|status| Self::parse_status(&status))
            .unwrap_or_default()
    }

    /// Read the `VmRSS` and `VmHWM` fields of a `/proc/<pid>/status` text.
    fn parse_status(status: &str) -> Self {
        let value = |key: &str| {
            status.lines().find_map(|line| {
                line.strip_prefix(key)?
                    .split_whitespace()
                    .next()?
                    .parse::<u64>()
                    .ok()
            })
        };
        Self {
            rss_kib: value("VmRSS:"),
            peak_rss_kib: value("VmHWM:"),
        }
    }
}

/// The size of one stage product's carrier, in structural counts.
///
/// These are counts of what the carrier holds, not resident bytes: a dataset's quads
/// and terms, and the payload bytes of its byte-artifact lane split into the committed
/// artifacts and the internal `pipeline/` ones.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ProductFootprint {
    /// Quads in the frozen dataset.
    pub quads: u64,
    /// Terms in the frozen dataset's term table.
    pub terms: u64,
    /// Payload bytes of the committed byte artifacts.
    pub committed_lane_bytes: u64,
    /// Payload bytes of the internal `pipeline/` byte artifacts.
    pub internal_lane_bytes: u64,
    /// Whether the product's carrier had been released when it was measured.
    pub carrier_released: bool,
}

impl ProductFootprint {
    /// Measure `bundle` without recording a dataflow read.
    pub(crate) fn of(bundle: &PipelineBundle<PipelineHandle>, carrier_released: bool) -> Self {
        let dataset = bundle.dataset();
        let (committed_lane_bytes, internal_lane_bytes) =
            crate::bundle::artifact_lane_bytes(bundle);
        Self {
            quads: u64::try_from(dataset.quad_count()).unwrap_or(u64::MAX),
            terms: u64::try_from(dataset.term_count()).unwrap_or(u64::MAX),
            committed_lane_bytes,
            internal_lane_bytes,
            carrier_released,
        }
    }

    /// What a release removed: this footprint, taken before, less `after`.
    #[must_use]
    pub fn released_to(&self, after: &Self) -> ReleasedFootprint {
        ReleasedFootprint {
            quads: self.quads.saturating_sub(after.quads),
            terms: self.terms.saturating_sub(after.terms),
            committed_lane_bytes: self
                .committed_lane_bytes
                .saturating_sub(after.committed_lane_bytes),
            internal_lane_bytes: self
                .internal_lane_bytes
                .saturating_sub(after.internal_lane_bytes),
        }
    }
}

/// What one carrier release removed from a product, in the units of
/// [`ProductFootprint`].
///
/// The allocator may keep the freed pages, so the resident size sampled in the same
/// record need not fall by a matching amount.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ReleasedFootprint {
    /// Quads no longer held.
    pub quads: u64,
    /// Terms no longer held.
    pub terms: u64,
    /// Committed artifact bytes no longer held.
    pub committed_lane_bytes: u64,
    /// Internal artifact bytes no longer held.
    pub internal_lane_bytes: u64,
}

/// The boundary a [`MemoryRecord`] was made at.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemoryEvent {
    /// The scheduler handed the stage to a worker.
    Dispatch,
    /// The scheduler published the stage's product.
    Complete,
    /// The scheduler swapped the stage's carrier for its residue.
    Release,
    /// A named phase inside a stage body began.
    PhaseStart,
    /// A named phase inside a stage body ended.
    PhaseEnd,
}

impl MemoryEvent {
    /// The stable label written to the log and the timings JSON.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Dispatch => "dispatch",
            Self::Complete => "complete",
            Self::Release => "release",
            Self::PhaseStart => "phase-start",
            Self::PhaseEnd => "phase-end",
        }
    }
}

/// One observation of memory at a stage or phase boundary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemoryRecord {
    /// The process that made the record. One log file can hold the records of several.
    pub pid: u32,
    /// Position in the order the records of that process were made.
    pub seq: u64,
    /// The stage the boundary belongs to.
    pub stage_id: String,
    /// The boundary.
    pub event: MemoryEvent,
    /// The phase name, for the two phase events.
    pub phase: Option<String>,
    /// Process memory at the boundary.
    pub process: ProcessMemory,
    /// The stage's product: as published for [`MemoryEvent::Complete`], and as left
    /// after the release for [`MemoryEvent::Release`]. Absent for the other events.
    pub product: Option<ProductFootprint>,
    /// What the release removed, for [`MemoryEvent::Release`].
    pub released: Option<ReleasedFootprint>,
}

/// Orders the records of one process across every thread that makes them.
static NEXT_SEQ: AtomicU64 = AtomicU64::new(0);

thread_local! {
    /// The phase records of the stage body running on this thread, while one is.
    static PHASE_SCOPE: RefCell<Option<Vec<MemoryRecord>>> = const { RefCell::new(None) };
}

fn target_enabled() -> bool {
    tracing::enabled!(target: TARGET, tracing::Level::INFO)
}

/// Make one record, sampling process memory, write it to the log when [`TARGET`] is
/// enabled, and return it.
pub(crate) fn record(
    stage_id: &str,
    event: MemoryEvent,
    phase: Option<&str>,
    product: Option<ProductFootprint>,
    released: Option<ReleasedFootprint>,
) -> MemoryRecord {
    let made = MemoryRecord {
        pid: std::process::id(),
        seq: NEXT_SEQ.fetch_add(1, Ordering::Relaxed),
        stage_id: stage_id.to_owned(),
        event,
        phase: phase.map(str::to_owned),
        process: ProcessMemory::sample(),
        product,
        released,
    };
    tracing::info!(
        target: TARGET,
        pid = made.pid,
        seq = made.seq,
        stage = stage_id,
        event = event.as_str(),
        phase,
        rss_kib = made.process.rss_kib,
        peak_rss_kib = made.process.peak_rss_kib,
        quads = product.map(|footprint| footprint.quads),
        terms = product.map(|footprint| footprint.terms),
        committed_lane_bytes = product.map(|footprint| footprint.committed_lane_bytes),
        internal_lane_bytes = product.map(|footprint| footprint.internal_lane_bytes),
        carrier_released = product.map(|footprint| footprint.carrier_released),
        released_quads = released.map(|freed| freed.quads),
        released_terms = released.map(|freed| freed.terms),
        released_committed_lane_bytes = released.map(|freed| freed.committed_lane_bytes),
        released_internal_lane_bytes = released.map(|freed| freed.internal_lane_bytes),
        "stage memory"
    );
    made
}

/// Record a phase boundary of a stage body.
///
/// The record is written to the log when [`TARGET`] is enabled, and kept by the
/// collection the calling thread is inside ([`collecting_phases`]), if any. A stage's
/// parallel work runs on the scheduler's pool, so a phase crossed there is kept by
/// whichever stage's collection that worker thread is inside, which may be another
/// stage's, or by none; the record names its own stage either way. With the target off
/// and no collection in progress, nothing is read or made.
pub(crate) fn phase(stage_id: &str, phase: &str, event: MemoryEvent) {
    let collecting = PHASE_SCOPE.with_borrow(Option::is_some);
    if !collecting && !target_enabled() {
        return;
    }
    let made = record(stage_id, event, Some(phase), None, None);
    PHASE_SCOPE.with_borrow_mut(|scope| {
        if let Some(records) = scope.as_mut() {
            records.push(made);
        }
    });
}

/// Run `work` and return the phase records made on this thread while it ran.
///
/// A collection begun inside another on the same thread keeps its own records and
/// leaves the outer one as it found it.
pub(crate) fn collecting_phases<T>(work: impl FnOnce() -> T) -> (T, Vec<MemoryRecord>) {
    /// Puts the enclosing collection back, also when `work` unwinds.
    struct Restore(Option<Vec<MemoryRecord>>);
    impl Drop for Restore {
        fn drop(&mut self) {
            let outer = self.0.take();
            PHASE_SCOPE.with_borrow_mut(|scope| *scope = outer);
        }
    }
    let outer = PHASE_SCOPE.with_borrow_mut(|scope| scope.replace(Vec::new()));
    let restore = Restore(outer);
    let value = work();
    let records = PHASE_SCOPE
        .with_borrow_mut(Option::take)
        .unwrap_or_default();
    drop(restore);
    (value, records)
}

#[path = "memory_record.tests.rs"]
#[cfg(test)]
mod tests;
