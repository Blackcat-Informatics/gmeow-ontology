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
//! PURREMB scan baselines (`crates/rdf-core/benches/purremb_alloc.rs` and its timed
//! sibling) as a cross-reference — they are corroboration, not a threshold.

#[allow(
    dead_code,
    reason = "the shared PURREMB fixture support module also exposes builders the timing harness does not use"
)]
#[path = "../tests/purremb_support/mod.rs"]
mod support;

use std::collections::BTreeSet;
use std::hint::black_box;

use criterion::{Criterion, criterion_group, criterion_main};
use gmeow_logic::annotation::TupleAnnotationAlgebra;
use gmeow_logic::external_relation::{
    ExternalRelationProvider, NeverCancelled, RelationAnnotationDimension, RelationCall,
    RelationOrderDirection, RelationOrdering,
};
use gmeow_logic::purremb_relation::{
    PurrembBinding, PurrembRetrievalProvider, RetrievalPolicy, RetrievalScore, SpaceTaggedScore,
    VectorSpaceScopedAlgebra, purremb_descriptor, purremb_generation_iri,
};
use gmeow_logic_compile::result_shape::ColumnKind;
use purrdf::{DistanceMetric, SourceVerificationMode, TermValue, VectorSpaceId};

use support::Fixture;

const ROWS: usize = 1_024;
const DIM: u32 = 512;
const RELATION: &str = "https://example.org/relation/vector";
const GEN_BASE: &str = "https://example.org/index/purremb-perf";
const ORDER_CRITERION: &str = "https://blackcatinformatics.ca/logic/VectorDistanceOrder";

static NEVER_CANCELLED: NeverCancelled = NeverCancelled;

fn ex(local: &str) -> String {
    format!("https://example.org/{local}")
}

fn annotate(score: RetrievalScore) -> SpaceTaggedScore {
    SpaceTaggedScore::single(
        score.distance,
        VectorSpaceId::from_raw(score.vector_space),
        score.metric_code,
    )
}

fn provider_for(fixture: &Fixture) -> PurrembRetrievalProvider<'_, SpaceTaggedScore> {
    let binding = PurrembBinding::open(
        &fixture.artifact_bytes,
        &fixture.source_bytes,
        fixture.selection(RetrievalPolicy::ExactFullSpace),
        SourceVerificationMode::Exact,
    )
    .expect("verified PURREMB binding");
    let generation = purremb_generation_iri(
        GEN_BASE,
        binding.artifact_root_hex(),
        RetrievalPolicy::ExactFullSpace,
        SourceVerificationMode::Exact,
    );
    let algebra = VectorSpaceScopedAlgebra::new(BTreeSet::new(), BTreeSet::new());
    let descriptor = purremb_descriptor(
        "https://example.org/provider/purremb",
        generation,
        "https://example.org/model/purremb-embedding-v1",
        RELATION,
        vec![ColumnKind::Iri, ColumnKind::Iri],
        RelationAnnotationDimension::Distance,
        algebra.identity().to_owned(),
        RelationOrdering::new(ORDER_CRITERION, RelationOrderDirection::Ascending)
            .expect("ordering"),
    )
    .expect("valid PURREMB descriptor");
    PurrembRetrievalProvider::new(binding, descriptor, Box::new(annotate)).expect("provider")
}

fn build_call(query_local: &str, limit: usize) -> RelationCall {
    RelationCall {
        request_iri: "https://example.org/request/purremb-bench".to_owned(),
        query_contract_hash: "purremb-bench-contract".to_owned(),
        relation_iri: RELATION.to_owned(),
        bounds: vec![Some(TermValue::iri(ex(query_local))), None],
        limit,
        ordering: RelationOrdering::new(ORDER_CRITERION, RelationOrderDirection::Ascending)
            .expect("ordering"),
    }
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
