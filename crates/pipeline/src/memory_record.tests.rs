// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

/// The unified parser feeds every resident-memory figure the pipeline reports (stage
/// records, cache-hydration deltas). It must read the kernel's two fields in KiB and
/// report a missing or malformed field as absent, never as zero.
#[test]
fn status_text_yields_resident_and_peak_or_absence() {
    let status = "Name:\tgmeow-dev\nVmPeak:\t  900000 kB\nVmHWM:\t  123456 kB\n\
                  VmRSS:\t   98765 kB\nThreads:\t8\n";
    assert_eq!(
        ProcessMemory::parse_status(status),
        ProcessMemory {
            rss_kib: Some(98_765),
            peak_rss_kib: Some(123_456),
        }
    );
    assert_eq!(
        ProcessMemory::parse_status("Name:\tkthreadd\nThreads:\t1\n"),
        ProcessMemory::default(),
        "a process without a resident set reports absence"
    );
    assert_eq!(
        ProcessMemory::parse_status("VmRSS:\tunknown kB\nVmHWM:\t77 kB\n"),
        ProcessMemory {
            rss_kib: None,
            peak_rss_kib: Some(77),
        },
        "a malformed field is absent and does not hide the other"
    );
}

/// Phase records reach the timings JSON through the collection installed around a
/// stage body. A stage body may run nested on a worker that is waiting on another
/// stage's parallel work, so an inner collection must return exactly its own
/// records, in order, and hand the outer one back intact; a phase boundary crossed
/// outside any collection is kept by none.
#[test]
fn phase_records_reach_the_enclosing_collection_and_only_it() {
    phase("outside", "ignored", MemoryEvent::PhaseStart);
    let ((), outer) = collecting_phases(|| {
        phase("outer", "first", MemoryEvent::PhaseStart);
        let ((), inner) = collecting_phases(|| {
            phase("inner", "only", MemoryEvent::PhaseStart);
            phase("inner", "only", MemoryEvent::PhaseEnd);
        });
        assert_eq!(
            inner
                .iter()
                .map(|made| (made.stage_id.as_str(), made.event))
                .collect::<Vec<_>>(),
            [
                ("inner", MemoryEvent::PhaseStart),
                ("inner", MemoryEvent::PhaseEnd)
            ]
        );
        phase("outer", "first", MemoryEvent::PhaseEnd);
    });
    assert_eq!(
        outer
            .iter()
            .map(|made| (made.stage_id.as_str(), made.phase.as_deref(), made.event))
            .collect::<Vec<_>>(),
        [
            ("outer", Some("first"), MemoryEvent::PhaseStart),
            ("outer", Some("first"), MemoryEvent::PhaseEnd)
        ]
    );
    assert!(
        outer[0].seq < outer[1].seq,
        "records keep the order they were made in"
    );
    let ((), after) = collecting_phases(|| {});
    assert!(
        after.is_empty(),
        "a finished collection leaves nothing behind"
    );
}

/// A hosted run is diagnosed from how much of the resident size is live data and how
/// much the allocator kept after a free, so every record carries the allocator's own
/// totals where glibc supplies them and they follow a real allocation. On a target
/// without them the totals are absent, never zero.
#[test]
fn record_carries_allocator_totals_only_where_the_allocator_reports_them() {
    let before = record("probe", MemoryEvent::Dispatch, None, None, None);
    let held = vec![1_u8; 8 << 20];
    let after = record("probe", MemoryEvent::Complete, None, None, None);
    assert!(
        after.reclaim.is_none(),
        "a boundary record returns no pages"
    );
    #[cfg(all(target_os = "linux", target_env = "gnu"))]
    {
        let (before, after) = (
            before.allocator.expect("glibc reports its totals"),
            after.allocator.expect("glibc reports its totals"),
        );
        assert!(
            after.live_bytes - before.live_bytes >= 8 << 20,
            "live bytes follow an allocation that is still held: {before:?} -> {after:?}"
        );
    }
    #[cfg(not(all(target_os = "linux", target_env = "gnu")))]
    assert_eq!((before.allocator, after.allocator), (None, None));
    drop(held);
}
