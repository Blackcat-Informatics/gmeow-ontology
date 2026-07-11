// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! DETERMINISTIC retired-instruction corroboration for the whole-ontology-union
//! conformance cost-partition (the callgrind half; the sibling
//! `conformance_union_cost_alloc` bench is the always-available allocation half).
//!
//! Under Valgrind Callgrind (via `iai-callgrind`) this counts **retired
//! instructions** — a run-to-run stable, machine-independent quantity — for the
//! three load-bearing regions of a single whole-ontology-union twin:
//!   - `build_merged_ontology` — the setup cost `S_onto` a disk cache could amortize;
//!   - `validate_ontology_only` — the fixture-independent whole-graph SHACL scan
//!     `V_ontology_only` a disk cache can NEVER remove (the decision hinge);
//!   - `validate_fixture_only` — the tiny-data anchor `V_fixture` (~0.05 s path).
//!
//! The setup for each scan bench is the `#[bench::…]` argument expression, which
//! iai-callgrind evaluates OUTSIDE the measured region, so only the scan is counted.
//!
//! Metric doctrine (mirrors `gmeow-logic`'s `engines_iai`): only the retired-
//! instruction (`Instructions`) column is meaningful; estimated cycles / cache
//! figures are microarchitecture-dependent and advisory only.
//!
//! MAINT-ONLY: requires the `valgrind` binary + the out-of-tree
//! `iai-callgrind-runner`; invoked exclusively through `make maint-bench-instructions`,
//! NOT wired into `make check`.

#[path = "cost_common/mod.rs"]
mod cost_common;

use std::hint::black_box;
use std::sync::Arc;

use iai_callgrind::{library_benchmark, library_benchmark_group, main};
use purrdf::RdfDataset;
use purrdf::shapes::report::ValidationReport;
use purrdf::shapes::shapes::Shapes;

/// Setup (unmeasured): the merged ontology + production shape union — the `V_full` /
/// `V_ontology_only` scan inputs.
fn setup_scan_ontology() -> (Arc<RdfDataset>, Shapes) {
    (
        cost_common::build_merged_ontology(),
        cost_common::load_production_shapes(),
    )
}

/// Setup (unmeasured): the tiny fixture-only graph + production shape union.
fn setup_scan_fixture() -> (Arc<RdfDataset>, Shapes) {
    (
        cost_common::fixture_only_dataset(),
        cost_common::load_production_shapes(),
    )
}

// Measure the one-time setup cost S_onto: building the merged ontology dataset from
// every slices/**/module.ttl. (No `#[bench]` case: the fn body IS the measured work.)
#[library_benchmark]
fn build_merged_ontology() -> Arc<RdfDataset> {
    black_box(cost_common::build_merged_ontology())
}

// Measure the fixture-independent whole-graph SHACL scan V_ontology_only. The
// argument expression builds (ontology, shapes) as SETUP, outside the measured
// region — so only `validate_dataset` over the whole ontology is counted.
#[library_benchmark]
#[bench::ontology_only(setup_scan_ontology())]
fn validate_ontology_only(inp: (Arc<RdfDataset>, Shapes)) -> ValidationReport {
    black_box(cost_common::validate(&inp.0, &inp.1))
}

// Measure the tiny-data anchor V_fixture: validate the fixture ALONE against the
// same production shape corpus. The ratio V_ontology_only / V_fixture is the
// machine-independent estimate of how far the whole-ontology scan exceeds the cheap
// on-gate path.
#[library_benchmark]
#[bench::fixture_only(setup_scan_fixture())]
fn validate_fixture_only(inp: (Arc<RdfDataset>, Shapes)) -> ValidationReport {
    black_box(cost_common::validate(&inp.0, &inp.1))
}

library_benchmark_group!(
    name = conformance_union_cost;
    benchmarks = build_merged_ontology, validate_ontology_only, validate_fixture_only
);

main!(library_benchmark_groups = conformance_union_cost);
