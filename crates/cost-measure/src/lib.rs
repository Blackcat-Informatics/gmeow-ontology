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
//! non-deterministic), [`AllocSample::peak_live`] is a pure function of the
//! measured operation and *can* gate.
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
//! # Determinism guarantee (single-threaded measured region)
//!
//! The engine parallelizes with Rayon. A **process-global** allocation counter
//! would capture background worker-thread allocations non-deterministically —
//! the exact failure this crate exists to kill. The counters here are therefore
//! **thread-local**: [`measure`] accounts only the allocations performed *on the
//! calling thread*. The guarantee is: **if the measured closure runs entirely on
//! the calling thread (a single-threaded region), the returned
//! [`AllocSample`] is a deterministic function of the closure.** Allocations a
//! closure fans out onto other threads are, by construction, not counted — so
//! the measured region must be single-threaded for the sample to be meaningful.
//!
//! The alloc/dealloc hot path touches only [`Cell`]-backed, `const`-initialized
//! thread-locals (no destructor, no lazy-init guard) and never itself heap-
//! allocates, so it is re-entrancy-safe and cannot recurse into the allocator —
//! avoiding the classic "thread-local that allocates on first access inside the
//! alloc path" footgun. The real allocation is always delegated to
//! [`System`](std::alloc::System); this wrapper only accounts around it.

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

/// Per-thread allocation counters. Four independent [`Cell`]s so the alloc hot
/// path is a handful of plain integer loads/stores with no heap traffic.
struct Counters {
    /// Monotonic total bytes requested via `alloc` on this thread.
    total_bytes: Cell<u64>,
    /// Monotonic count of successful `alloc` calls on this thread.
    count: Cell<u64>,
    /// Current simultaneously-live bytes on this thread (`alloc` − `dealloc`).
    live: Cell<u64>,
    /// High-water mark of [`Self::live`] since the last [`measure`] reset.
    peak: Cell<u64>,
}

impl Counters {
    /// A zeroed counter block usable as a `const` thread-local initializer (so
    /// the thread-local registers no destructor and never lazily allocates on
    /// first access — the property that makes the alloc path re-entrancy-safe).
    const fn new() -> Self {
        Self {
            total_bytes: Cell::new(0),
            count: Cell::new(0),
            live: Cell::new(0),
            peak: Cell::new(0),
        }
    }
}

thread_local! {
    /// The calling thread's allocation counters. `const`-initialized: no
    /// destructor is registered and access never allocates, so reading/writing
    /// these from inside `alloc`/`dealloc` cannot recurse into the allocator.
    static COUNTERS: Counters = const { Counters::new() };
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
    /// Total bytes requested via `alloc` on the calling thread during the region.
    pub bytes: u64,
    /// Number of `alloc` calls on the calling thread during the region.
    pub count: u64,
    /// High-water of simultaneously-live bytes during the region (the
    /// deterministic memory metric — unlike peak-RSS, this *can* gate).
    pub peak_live: u64,
}

/// A [`GlobalAlloc`] that delegates every real allocation to
/// [`System`](std::alloc::System) and accounts each call into the calling
/// thread's [`COUNTERS`].
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

/// Record a successful allocation of `size` bytes on the calling thread: bump the
/// monotonic totals and raise the live/peak high-water. Touches only `Cell`s, so
/// it performs no heap allocation and cannot recurse into the allocator.
#[inline]
fn account_alloc(size: usize) {
    let size = size as u64;
    // `with` on a `const`-initialized, destructor-free thread-local never fails,
    // so a hard `with` (not `try_with`) is correct and cheapest here.
    COUNTERS.with(|c| {
        c.total_bytes.set(c.total_bytes.get().wrapping_add(size));
        c.count.set(c.count.get().wrapping_add(1));
        let live = c.live.get().wrapping_add(size);
        c.live.set(live);
        if live > c.peak.get() {
            c.peak.set(live);
        }
    });
}

/// Record a deallocation of `size` bytes on the calling thread: lower the live
/// gauge (never touching the monotonic totals or the peak high-water). Touches
/// only `Cell`s.
#[inline]
fn account_dealloc(size: usize) {
    let size = size as u64;
    COUNTERS.with(|c| {
        c.live.set(c.live.get().saturating_sub(size));
    });
}

// SAFETY: every real allocation/deallocation is delegated verbatim to `System`,
// which is a correct `GlobalAlloc`; the accounting is pure integer bookkeeping in
// thread-local `Cell`s that performs no allocation, so it upholds every
// `GlobalAlloc` invariant that `System` upholds. `realloc` is intentionally NOT
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

/// Run `f` on the calling thread, returning its value plus the deterministic
/// [`AllocSample`] of the allocations it performed **on this thread**.
///
/// The totals ([`AllocSample::bytes`], [`AllocSample::count`]) are read as a
/// delta across `f` (snapshot before, difference after), and the peak high-water
/// is reset to the current live level before `f` so [`AllocSample::peak_live`] is
/// the high-water of *net* simultaneously-live bytes reached *during* `f`.
///
/// Determinism holds only when `f` runs entirely on the calling thread; work `f`
/// fans out onto other threads (e.g. via Rayon) is not counted. See the
/// crate-level docs.
pub fn measure<T>(f: impl FnOnce() -> T) -> (T, AllocSample) {
    let (bytes0, count0, live0) = COUNTERS.with(|c| {
        let live0 = c.live.get();
        // Reset the peak high-water to the current live level so the reported
        // peak is the growth reached during `f`, not a stale earlier high-water.
        c.peak.set(live0);
        (c.total_bytes.get(), c.count.get(), live0)
    });

    let value = f();

    let sample = COUNTERS.with(|c| AllocSample {
        bytes: c.total_bytes.get().wrapping_sub(bytes0),
        count: c.count.get().wrapping_sub(count0),
        peak_live: c.peak.get().saturating_sub(live0),
    });

    (value, sample)
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
