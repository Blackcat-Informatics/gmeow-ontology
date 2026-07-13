// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Harness-scoped deterministic allocation-measurement substrate.
//!
//! This crate exists to give a benchmark harness and an `iai` bench a single,
//! shared, *deterministic* way to measure the native reasoning engine's
//! allocation cost — total bytes allocated, allocation count, and peak
//! simultaneously-live bytes — so an allocation-reducing optimization (fewer
//! clones, fewer owned-key allocations) shows up as a monotone drop in a number
//! a gate can compare. Unlike peak-RSS (which is polluted by the allocator's
//! arena high-water, page reuse, and background threads and is therefore
//! non-deterministic), all three scalars of an [`AllocSample`] are a pure
//! function of the measured operation (given a sequential measured region) and
//! **all three gate** — see the determinism guarantee below for why each metric
//! uses the accounting mechanism that keeps *it* deterministic.
//!
//! # Why this lives in its own crate (and must never reach the CLI)
//!
//! A [`#[global_allocator]`](std::alloc::GlobalAlloc) is **process-global**:
//! whichever binary installs one changes allocation for the *entire* process.
//! The shipped `gmeow` CLI (`crates/gmeow-cli`) must keep the platform's default
//! allocator and pay zero measurement overhead, so this crate is a workspace
//! member that only bench targets / the maintenance harness binary may depend
//! on — **never** `gmeow-cli`, directly or transitively.
//!
//! Merely depending on this crate installs nothing: it provides the allocator
//! *type* plus [`measure`]; the actual install is a one-line `static` in the
//! consuming **binary** (see below). That keeps the blast radius to exactly the
//! binaries that opt in.
//!
//! # Installing the allocator in a binary
//!
//! A binary (bench harness / `iai` bench / maint harness) installs it with a
//! single `static`, because [`CountingAllocator::new`] is a `const fn`:
//!
//! ```
//! use gmeow_cost_measure::CountingAllocator;
//!
//! #[global_allocator]
//! static ALLOC: CountingAllocator = CountingAllocator::new();
//! ```
//!
//! With that in place, wrap the measured operation in [`measure`]:
//!
//! ```
//! # use gmeow_cost_measure::measure;
//! let (sum, sample) = measure(|| (0u64..1000).sum::<u64>());
//! assert_eq!(sum, 499_500);
//! let _ = sample; // sample.bytes / sample.count / sample.peak_live
//! ```
//!
//! # Determinism guarantee (sequential measured region; split accounting)
//!
//! The engine parallelizes with Rayon: even under a single-thread global pool,
//! `par_iter`/parallel sections execute partly on the Rayon worker thread (a
//! *different* thread from the measuring caller) via work-stealing. That split
//! is non-deterministic run-to-run, so the two total-allocation scalars and the
//! net-live peak need **different** accounting mechanisms — each chosen so its
//! own metric is deterministic:
//!
//! * **`bytes` / `count` are PROCESS-GLOBAL running totals** (atomics summed
//!   across *all* threads). The *total* bytes/count a measured operation
//!   allocates is the logical allocation of the work and is **invariant to which
//!   thread (caller vs Rayon worker) performs each allocation** — a sum does not
//!   depend on the interleaving — so the global delta across [`measure`] is a
//!   deterministic function of the closure even though the per-thread split is
//!   not. This is sound *because the harness is sequential*: it pins the global
//!   Rayon pool to one thread and measures each case one at a time, so no
//!   unrelated concurrent allocation falls inside a measured region. **The
//!   measured region must be the only thing allocating in the process** for the
//!   global delta to attribute solely to the closure.
//! * **`peak_live` stays THREAD-LOCAL** (`alloc` − `dealloc` net high-water on
//!   the calling thread). Net-live is *order-dependent* — a global net-live
//!   high-water would depend on the non-deterministic caller/worker interleaving
//!   — so it is accounted per-thread, where it is a deterministic function of the
//!   caller's own allocation pattern and nets each transient scratch allocation
//!   (freed within the region) to zero.
//!
//! The alloc/dealloc hot path touches only lock-free atomics plus a
//! [`Cell`]-backed, `const`-initialized thread-local (no destructor, no lazy-init
//! guard) and never itself heap-allocates, so it is re-entrancy-safe and cannot
//! recurse into the allocator — avoiding the classic "thread-local that allocates
//! on first access inside the alloc path" footgun. The real allocation is always
//! delegated to [`System`]; this wrapper only accounts around
//! it.

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::sync::atomic::{AtomicU64, Ordering};

/// Process-global monotonic total bytes requested via `alloc`, summed across
/// **every** thread. Deterministic as a per-region delta because a sum is
/// invariant to which thread performed each allocation (see the crate docs).
static TOTAL_BYTES: AtomicU64 = AtomicU64::new(0);
/// Process-global monotonic count of successful `alloc` calls, summed across
/// every thread. Deterministic as a per-region delta for the same reason.
static ALLOC_COUNT: AtomicU64 = AtomicU64::new(0);

/// Per-thread net-live counters backing the order-dependent [`AllocSample::peak_live`].
/// Two [`Cell`]s so the live/peak update is a couple of plain integer loads/stores
/// with no heap traffic.
struct LiveCounters {
    /// Current simultaneously-live bytes on this thread (`alloc` − `dealloc`).
    live: Cell<u64>,
    /// High-water mark of [`Self::live`] since the last [`measure`] reset.
    peak: Cell<u64>,
}

impl LiveCounters {
    /// A zeroed counter block usable as a `const` thread-local initializer (so
    /// the thread-local registers no destructor and never lazily allocates on
    /// first access — the property that makes the alloc path re-entrancy-safe).
    const fn new() -> Self {
        Self {
            live: Cell::new(0),
            peak: Cell::new(0),
        }
    }
}

thread_local! {
    /// The calling thread's net-live counters. `const`-initialized: no destructor
    /// is registered and access never allocates, so reading/writing these from
    /// inside `alloc`/`dealloc` cannot recurse into the allocator.
    static LIVE: LiveCounters = const { LiveCounters::new() };
}

/// A deterministic snapshot of the allocation cost of a measured region, as
/// produced by [`measure`].
///
/// The three fields are the scalar projections an allocation-reducing
/// optimization is expected to move: fewer/cheaper allocations lower [`Self::bytes`]
/// and [`Self::count`], and a tighter working set lowers [`Self::peak_live`].
/// The field shape is `(bytes, count, peak_live)`, aligned with the
/// `CostVector::set_allocation(alloc_bytes, alloc_count, peak_live_bytes)` seam in
/// `gmeow-logic` so a harness can plug a sample straight in without coupling the
/// logic crate to this allocator.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AllocSample {
    /// Total bytes requested via `alloc` across **all threads** during the region
    /// (the process-global sum; invariant to the caller/worker split, so it gates).
    pub bytes: u64,
    /// Number of `alloc` calls across **all threads** during the region (the
    /// process-global sum; invariant to the caller/worker split, so it gates).
    pub count: u64,
    /// High-water of simultaneously-live bytes on the calling thread during the
    /// region (the order-dependent memory metric, accounted per-thread so it stays
    /// deterministic — unlike peak-RSS, this *can* gate).
    pub peak_live: u64,
}

/// A [`GlobalAlloc`] that delegates every real allocation to
/// [`System`] and accounts each call into the process-global
/// [`TOTAL_BYTES`]/[`ALLOC_COUNT`] totals plus the calling thread's net-live
/// [`LIVE`] counters.
///
/// Installing this as the `#[global_allocator]` of a binary is what turns
/// [`measure`] on for that binary; see the crate-level docs. It must **never**
/// be installed in the shipped `gmeow` CLI.
#[derive(Debug, Default, Clone, Copy)]
pub struct CountingAllocator;

impl CountingAllocator {
    /// Construct the allocator. `const` so it can initialize a `#[global_allocator]`
    /// `static` directly:
    ///
    /// ```
    /// # use gmeow_cost_measure::CountingAllocator;
    /// static A: CountingAllocator = CountingAllocator::new();
    /// # let _ = A;
    /// ```
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

/// Record a successful allocation of `size` bytes: bump the process-global
/// monotonic totals (summed across all threads) and raise the calling thread's
/// net-live/peak high-water. Touches only lock-free atomics and thread-local
/// `Cell`s, so it performs no heap allocation and cannot recurse into the allocator.
#[inline]
fn account_alloc(size: usize) {
    let size = size as u64;
    // `Relaxed` is sufficient: the harness reads the totals as a delta only after
    // the measured closure has fully returned (a happens-before established by the
    // sequential control flow / thread joins), never racing an in-flight `alloc`.
    TOTAL_BYTES.fetch_add(size, Ordering::Relaxed);
    ALLOC_COUNT.fetch_add(1, Ordering::Relaxed);
    // `with` on a `const`-initialized, destructor-free thread-local never fails,
    // so a hard `with` (not `try_with`) is correct and cheapest here.
    LIVE.with(|c| {
        let live = c.live.get().wrapping_add(size);
        c.live.set(live);
        if live > c.peak.get() {
            c.peak.set(live);
        }
    });
}

/// Record a deallocation of `size` bytes: lower the calling thread's net-live
/// gauge (never touching the monotonic totals or the peak high-water). Touches
/// only thread-local `Cell`s.
#[inline]
fn account_dealloc(size: usize) {
    let size = size as u64;
    LIVE.with(|c| {
        c.live.set(c.live.get().saturating_sub(size));
    });
}

// SAFETY: every real allocation/deallocation is delegated verbatim to `System`,
// which is a correct `GlobalAlloc`; the accounting is pure integer bookkeeping in
// lock-free atomics and thread-local `Cell`s that performs no allocation, so it
// upholds every `GlobalAlloc` invariant that `System` upholds. `realloc` is NOT
// overridden: the default `GlobalAlloc::realloc` routes through `self.alloc` +
// `self.dealloc`, so a reallocation is accounted as a counted alloc/dealloc pair
// (deterministic), rather than an unaccounted in-place `System::realloc`.
unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: forwarding an unchanged `layout` to the `System` allocator.
        let ptr = unsafe { System.alloc(layout) };
        if !ptr.is_null() {
            account_alloc(layout.size());
        }
        ptr
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        // SAFETY: forwarding an unchanged `layout` to the `System` allocator.
        let ptr = unsafe { System.alloc_zeroed(layout) };
        if !ptr.is_null() {
            account_alloc(layout.size());
        }
        ptr
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        account_dealloc(layout.size());
        // SAFETY: `ptr`/`layout` came from this allocator's `alloc`, which
        // delegated to `System`, so `System::dealloc` is the matching free.
        unsafe { System.dealloc(ptr, layout) };
    }
}

/// Run `f`, returning its value plus the deterministic [`AllocSample`] of the
/// allocations it performed.
///
/// The totals ([`AllocSample::bytes`], [`AllocSample::count`]) are read as a
/// delta across `f` over the **process-global** counters — capturing every
/// thread's allocations, including work `f` fans out onto a Rayon worker — and
/// the peak high-water is reset to the calling thread's current live level before
/// `f` so [`AllocSample::peak_live`] is the high-water of *net* simultaneously-live
/// bytes reached *during* `f` on this thread.
///
/// Determinism of the global totals holds only when the measured region is the
/// only thing allocating in the process (a sequential harness); a concurrent
/// unrelated allocation would land inside the delta. See the crate-level docs.
pub fn measure<T>(f: impl FnOnce() -> T) -> (T, AllocSample) {
    // Snapshot the calling thread's live level and reset its peak high-water so
    // the reported peak is the growth reached during `f`, not a stale earlier one.
    let live0 = LIVE.with(|c| {
        let live0 = c.live.get();
        c.peak.set(live0);
        live0
    });
    let bytes0 = TOTAL_BYTES.load(Ordering::Relaxed);
    let count0 = ALLOC_COUNT.load(Ordering::Relaxed);

    let value = f();

    let bytes = TOTAL_BYTES.load(Ordering::Relaxed).wrapping_sub(bytes0);
    let count = ALLOC_COUNT.load(Ordering::Relaxed).wrapping_sub(count0);
    let peak_live = LIVE.with(|c| c.peak.get().saturating_sub(live0));

    (
        value,
        AllocSample {
            bytes,
            count,
            peak_live,
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    // Install the counting allocator on THIS test binary so `measure` is live.
    // This is exactly how a bench/maint binary opts in; the shipped CLI never does.
    #[global_allocator]
    static ALLOC: CountingAllocator = CountingAllocator::new();

    /// A fixed, deterministic allocating workload: build and drop a
    /// `Vec<u64>` (pre-sized then pushed to a fixed count) plus two `String`s.
    /// Identical every call, so its allocation sample must be identical every call.
    fn deterministic_workload() -> u64 {
        let mut v: Vec<u64> = Vec::with_capacity(256);
        for i in 0..256u64 {
            v.push(i.wrapping_mul(2654435761));
        }
        let mut s = String::with_capacity(128);
        for i in 0..64u64 {
            s.push_str(&i.to_string());
        }
        let t = format!("{s}-{}-{}", v.len(), v[0]);
        // Fold everything so the optimizer cannot elide the allocations.
        let acc = v.iter().copied().fold(0u64, u64::wrapping_add);
        acc.wrapping_add(s.len() as u64)
            .wrapping_add(t.len() as u64)
    }

    /// The whole reason this crate exists: the SAME deterministic closure measured
    /// twice yields byte-identical [`AllocSample`]s, and the sample is non-vacuous.
    #[test]
    fn measure_is_deterministic_across_identical_runs() {
        // Warm any first-touch thread-local / formatting machinery so it does not
        // skew the first measured run relative to the second.
        let _ = deterministic_workload();

        let (_r1, a) = measure(deterministic_workload);
        let (_r2, b) = measure(deterministic_workload);

        assert_eq!(
            a, b,
            "identical deterministic workloads must yield identical allocation samples"
        );
        assert!(a.bytes > 0, "the workload allocates, so bytes must be > 0");
        assert!(a.count > 0, "the workload allocates, so count must be > 0");
        assert!(
            a.peak_live > 0,
            "the workload holds live allocations, so peak_live must be > 0"
        );
    }
}
