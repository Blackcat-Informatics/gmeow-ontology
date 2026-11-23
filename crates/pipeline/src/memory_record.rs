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
//! * The scheduler makes one at each stage's dispatch, publication and carrier release,
//!   and one for the return of freed allocator pages that follows each publication.
//! * Reasoning records each return of freed pages inside its body. Snapshot records
//!   a return after native handle publication and another after product hashing.
//! * A stage body makes one at the start and one at the end of each phase it runs
//!   through [`measure_stage_phase`](crate::node::measure_stage_phase), and the snapshot
//!   serializer makes one at the end of each of its timed phases and one at the start
//!   of the phase that canonicalizes the stratum.
//!
//! A phase that is only logged makes no record of its own. That includes every phase
//! the `gmeow-logic` crate logs on the `pipeline_phase` target, the native receipt
//! encode among them; those fall inside the enclosing measured stage phase.
//!
//! # Cost
//!
//! Each record reads `/proc/self/status` once (a few kilobytes), asks the allocator for
//! its totals once and allocates its stage and phase names. A record of a return of
//! freed pages reads the status file a second time, before the return. A run makes at
//! most four scheduler records per executed stage plus two per measured phase
//! execution, so the records make at most `5 × stages + 2 × measured phase executions`
//! reads, plus two for each return within a stage body. A stage that is never
//! released makes three scheduler records. On a host without Linux procfs the read
//! fails and the two process figures are absent.
//!
//! The allocator totals come from glibc's `mallinfo2`, which takes each arena's lock in
//! turn and walks that arena's free lists, so its cost grows with the number of free
//! chunks and it can delay a thread allocating in the arena being read. It is one call
//! with no allocation, file or text of its own. On a target without glibc the totals
//! are absent.
//!
//! That bound covers the reads made for records only. The scheduler also reads the same
//! file once per stage, and a second time on a cache hit, for the cache-hydration
//! figure, and once for each individually detached typed handle. The snapshot reads
//! it once for each owned carrier whose size it logs (`log_carrier_size` in
//! `stages/carrier.rs`).

use std::cell::RefCell;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

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

/// Report an individual dropped handle reference at the scheduler boundary.
/// Shared handle backing can survive this event through another owner.
pub(crate) fn handle_release(stage: &str, handle: &str) {
    let process = ProcessMemory::sample();
    tracing::info!(
        target: TARGET,
        pid = std::process::id(),
        stage,
        event = "release-handle",
        handle,
        rss_kib = process.rss_kib,
        peak_rss_kib = process.peak_rss_kib,
        "typed handle release"
    );
}

/// What the process allocator held, in bytes, as its own totals report it.
///
/// The totals are those of every arena and are not an atomic snapshot: threads go on
/// allocating while the arenas are read one after another. Free bytes are the free
/// chunks the allocator keeps for reuse; they include pages a return of freed pages has
/// already handed back to the system, so they are not a resident figure.
///
/// These totals and the resident size measure different scopes: the resident size also
/// counts code, thread stacks and resident file and shared mappings, which the
/// allocator does not own. The resident size less [`Self::live_bytes`] is therefore an
/// indication of the resident memory the allocator holds unused, and is neither a bound
/// on it nor a measure of what a return of freed pages can recover. It reads high by the
/// resident memory the allocator does not own. It reads low because glibc counts chunks
/// freed into its per-thread caches as still in use, so they are inside the live bytes,
/// and because live pages that are swapped out or never touched are not resident; the
/// two readings are also not taken at one moment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AllocatorMemory {
    /// Bytes the allocator reports as in use, arena and mapped together: allocations that
    /// have not been freed, and chunks freed into a per-thread cache.
    pub live_bytes: u64,
    /// Bytes in free chunks the allocator keeps.
    pub free_bytes: u64,
}

/// One return of freed allocator pages to the system, as the record made after it
/// reports it. The resident size after the return is the record's own
/// [`MemoryRecord::process`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReclaimedMemory {
    /// Resident set size read immediately before the return (`VmRSS`).
    pub rss_before_kib: Option<u64>,
    /// How long the return took, in microseconds.
    pub elapsed_us: u64,
}

/// The size of one stage product's carrier, in structural counts.
///
/// These are counts of what the carrier holds, not resident bytes: a dataset's quads
/// and terms, the bytes of its term text and record tables, and the payload bytes of
/// its byte-artifact lane split into the committed artifacts and the internal
/// `pipeline/` ones.
///
/// Nothing here sizes a typed handle payload (the reasoning result, a compiled
/// program, diagnostics) or a dataset's lookup indexes: no payload states its own
/// size, PurRDF states an index capacity only for a query graph, which a stage carrier
/// is not, and a record does not walk or serialize a structure to find one.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ProductFootprint {
    /// Quads in the frozen dataset.
    pub quads: u64,
    /// Terms in the frozen dataset's term table.
    pub terms: u64,
    /// Bytes of the frozen dataset's term text, term table, quad, reifier and
    /// annotation tables and graph declarations, as the dataset's own accounting
    /// states them (`RdfDataset::rdf_payload_bytes`). It leaves out allocator
    /// overhead and the lookup indexes, so it is a lower bound on what the dataset
    /// holds, not its resident size.
    pub dataset_payload_bytes: u64,
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
            dataset_payload_bytes: u64::try_from(dataset.rdf_payload_bytes()).unwrap_or(u64::MAX),
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
    /// Freed allocator pages were returned to the system: after the stage's
    /// publication when the record names no phase, and after the named phase of the
    /// stage body when it names one.
    Reclaim,
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
            Self::Reclaim => "reclaim",
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
    /// The phase name, for the two phase events and for a return of freed pages made
    /// inside a stage body.
    pub phase: Option<String>,
    /// Process memory at the boundary.
    pub process: ProcessMemory,
    /// The allocator's totals at the boundary. Absent where the allocator offers none.
    pub allocator: Option<AllocatorMemory>,
    /// The stage's product: as published for [`MemoryEvent::Complete`], and as left
    /// after the release for [`MemoryEvent::Release`]. Absent for the other events.
    pub product: Option<ProductFootprint>,
    /// What the release removed, for [`MemoryEvent::Release`].
    pub released: Option<ReleasedFootprint>,
    /// The return of freed pages, for [`MemoryEvent::Reclaim`].
    pub reclaim: Option<ReclaimedMemory>,
}

/// Orders the records of one process across every thread that makes them.
static NEXT_SEQ: AtomicU64 = AtomicU64::new(0);

thread_local! {
    /// The phase records of the stage body running on this thread, while one is.
    static PHASE_SCOPE: RefCell<Option<Vec<MemoryRecord>>> = const { RefCell::new(None) };
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
    make(stage_id, event, phase, product, released, None)
}

/// Record a return of freed allocator pages that has just finished.
///
/// `rss_before_kib` is the resident size the caller read immediately before the return
/// and `elapsed` how long the return took; the record samples the resident size after
/// it. `phase` names the phase of the stage body the return followed, and is absent for
/// the return the scheduler makes after a publication.
pub(crate) fn reclaimed(
    stage_id: &str,
    phase: Option<&str>,
    rss_before_kib: Option<u64>,
    elapsed: Duration,
) -> MemoryRecord {
    let reclaim = ReclaimedMemory {
        rss_before_kib,
        elapsed_us: u64::try_from(elapsed.as_micros()).unwrap_or(u64::MAX),
    };
    make(
        stage_id,
        MemoryEvent::Reclaim,
        phase,
        None,
        None,
        Some(reclaim),
    )
}

fn make(
    stage_id: &str,
    event: MemoryEvent,
    phase: Option<&str>,
    product: Option<ProductFootprint>,
    released: Option<ReleasedFootprint>,
    reclaim: Option<ReclaimedMemory>,
) -> MemoryRecord {
    let made = MemoryRecord {
        pid: std::process::id(),
        seq: NEXT_SEQ.fetch_add(1, Ordering::Relaxed),
        stage_id: stage_id.to_owned(),
        event,
        phase: phase.map(str::to_owned),
        process: ProcessMemory::sample(),
        allocator: crate::allocator_observation::allocator_totals(),
        product,
        released,
        reclaim,
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
        allocator_live_bytes = made.allocator.map(|totals| totals.live_bytes),
        allocator_free_bytes = made.allocator.map(|totals| totals.free_bytes),
        reclaim_rss_before_kib = reclaim.and_then(|returned| returned.rss_before_kib),
        reclaim_elapsed_us = reclaim.map(|returned| returned.elapsed_us),
        quads = product.map(|footprint| footprint.quads),
        terms = product.map(|footprint| footprint.terms),
        dataset_payload_bytes = product.map(|footprint| footprint.dataset_payload_bytes),
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
/// stage's, or by none; the record names its own stage either way. A record made
/// with the target off and no collection in progress is dropped.
pub(crate) fn phase(stage_id: &str, phase: &str, event: MemoryEvent) {
    keep(record(stage_id, event, Some(phase), None, None));
}

/// Hand a record made inside a stage body to the collection the calling thread is
/// inside ([`collecting_phases`]), if any.
pub(crate) fn keep(made: MemoryRecord) {
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
