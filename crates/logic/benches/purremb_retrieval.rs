// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Advisory wall-clock baseline for the verified PURREMB retrieval scan.
//!
//! This Criterion harness is **advisory only** and is never run on-gate: the correctness
//! gates live in `tests/purremb_perf.rs`, where a counting allocator proves the memory
//! shape (`O(k + dimension)` scan working set, no whole-matrix copy, no per-call
//! re-verification). Here we merely time `PurrembRetrievalProvider::call` over a wide,
//! short, deterministic corpus for a few `k`, so a regression in the streaming scan's
//! latency is visible. These numbers are intended to be read alongside PurRDF's documented
//! PURREMB scan baselines (purrdf's `purremb_alloc` bench and its timed
//! sibling) as a cross-reference — they are corroboration, not a threshold.

#[allow(
    dead_code,
    reason = "the shared PURREMB fixture support module also exposes builders the timing harness does not use"
)]
#[path = "../tests/purremb_support/mod.rs"]
mod support;

use std::hint::black_box;

use criterion::{Criterion, criterion_group, criterion_main};
use gmeow_logic::external_relation::{ExternalRelationProvider, NeverCancelled, RelationCall};
use gmeow_logic::purremb_relation::{PurrembRetrievalProvider, SpaceTaggedScore};
use purrdf::DistanceMetric;

use support::Fixture;

const ROWS: usize = 1_024;
const DIM: u32 = 512;
const RELATION: &str = "https://example.org/relation/vector";
const GEN_BASE: &str = "https://example.org/index/purremb-perf";
const ORDER_CRITERION: &str = "https://blackcatinformatics.ca/logic/VectorDistanceOrder";

static NEVER_CANCELLED: NeverCancelled = NeverCancelled;

fn provider_for(fixture: &Fixture) -> PurrembRetrievalProvider<'_, SpaceTaggedScore> {
    support::purremb_provider(fixture, RELATION, GEN_BASE, ORDER_CRITERION)
}

fn build_call(query_local: &str, limit: usize) -> RelationCall {
    support::purremb_call(
        RELATION,
        ORDER_CRITERION,
        "https://example.org/request/purremb-bench",
        "purremb-bench-contract",
        &support::ex(query_local),
        limit,
    )
}

fn bench_retrieval(criterion: &mut Criterion) {
    let fixture =
        support::iri_corpus_f32_large("bench", DistanceMetric::SquaredEuclidean, ROWS, DIM);
    let provider = provider_for(&fixture);
    let query_local = support::large_corpus_local(0);

    let mut group = criterion.benchmark_group("purremb_retrieval");
    for &limit in &[4_usize, 16, 64] {
        let call = build_call(&query_local, limit);
        group.bench_function(format!("exact_full_space_k{limit}"), |bencher| {
            bencher.iter(|| {
                let batch = provider
                    .call(black_box(&call), &NEVER_CANCELLED)
                    .expect("complete batch");
                black_box(batch.rows.len())
            });
        });
    }
    group.finish();
}

criterion_group!(benches, bench_retrieval);
criterion_main!(benches);
