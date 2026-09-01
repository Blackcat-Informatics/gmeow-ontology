// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! DETERMINISTIC external-corroboration backend for the native forward engine.
//!
//! This bench drives [`gmeow_logic::cost::run_native_forward`] over a few tiny
//! [`gmeow_logic::synth_corpus`] workloads under Valgrind Callgrind (via
//! `iai-callgrind`). Unlike the wall-clock criterion benches, Callgrind counts
//! **retired instructions**: a run-to-run stable, machine-independent quantity that
//! corroborates the on-gate `steps + alloc + peak-live` cost gate from a fully
//! independent measurement path.
//!
//! Metric doctrine: **only the retired-instruction (`Instructions`) column
//! is a gating-eligible metric.** iai-callgrind additionally reports estimated
//! cycles and L1/L2/LL cache figures — those are microarchitecture-dependent and
//! therefore ADVISORY only; never gate on them.
//!
//! MAINT-ONLY: this bench cannot run without the `valgrind` binary and the
//! out-of-tree `iai-callgrind-runner`, so it is invoked exclusively through
//! `make maint-bench-instructions` (which hard-fails with a remediation message
//! when either tool is absent) and is NOT wired into `make check`.
//!
//! `n` is kept tiny on every case: the goal is a fast, low-RAM, deterministic
//! instruction count, not a scaling curve (the criterion benches cover scaling).

use gmeow_logic::cost::{NativeForwardRun, run_native_forward};
use gmeow_logic::synth_corpus::{
    SynthWorkload, reachability, same_generation, strongly_connected, transitive_closure,
};
use iai_callgrind::{library_benchmark, library_benchmark_group, main};
use std::hint::black_box;

// Measure the retired-instruction cost of one native forward run over a synthetic
// workload. Each case's argument expression is the generator call that builds the
// workload (typed program + frozen EDB); iai-callgrind evaluates and black-boxes that
// expression as SETUP, OUTSIDE the measured region — so only the engine's chase is
// counted, never workload construction. (This comment sits ABOVE
// `#[library_benchmark]`: the macro forbids any attribute — including a `///` doc
// attribute — between itself and its `#[bench::…]` cases.)
#[library_benchmark]
// NOTE: each case id must NOT equal a generator name — the macro emits a
// per-case item under that id, which would shadow the imported generator — so the
// ids carry the scale suffix (`_n6`, …) and stay distinct from the fn names.
#[bench::transitive_closure_n6(transitive_closure(6))]
#[bench::strongly_connected_n4(strongly_connected(4))]
#[bench::same_generation_n3(same_generation(3))]
#[bench::reachability_n6(reachability(6))]
fn native_forward(workload: SynthWorkload) -> NativeForwardRun {
    black_box(
        run_native_forward(&workload.edb, &workload.program).expect("native forward run succeeds"),
    )
}

library_benchmark_group!(
    name = engines;
    benchmarks = native_forward
);

main!(library_benchmark_groups = engines);
