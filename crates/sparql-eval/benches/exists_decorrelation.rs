// SPDX-FileCopyrightText: 2026 Blackcat Informatics Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! `EXISTS` decorrelation benchmark: naive per-row inner re-evaluation
//! vs the cached inner evaluation.
//!
//! The `FILTER NOT EXISTS` shape re-evaluates its inner pattern once per outer
//! row in the naive path; the decorrelated path evaluates it once and reuses the
//! result (the inner eval is independent of the outer row). This bench runs the
//! SAME query and dataset twice — `EvalOptions::exists_memo` off vs on — over a
//! synthetic ~1k-subject dataset, so the speedup is **measured, not asserted**.
//!
//! Report-only, `make bench` lane only — excluded from `make check`. Observed
//! locally on this 1k-row anti-join with a *trivial* single-triple inner: naive
//! ~1.20 ms vs decorrelated ~0.71 ms (~1.7×). The win scales with the inner
//! pattern's cost: naive is O(N · inner_cost) (N full inner evals), decorrelated
//! is O(inner_cost + N · join), so a heavy inner (multi-triple join, property
//! path) improves by a far larger factor.

use std::sync::Arc;

use criterion::{criterion_group, criterion_main, Criterion};

use gmeow_rdf_core::{RdfDataset, RdfDatasetBuilder};
use gmeow_sparql_algebra::SparqlParser;
use gmeow_sparql_eval::{evaluate_query, EvalCtx};

/// `:s{i} :knows :o{i}` for i in 0..n, plus `:o0 :member :club` so exactly one
/// subject survives the anti-join. N subjects → N outer rows for the EXISTS.
fn knows_dataset(n: usize) -> Arc<RdfDataset> {
    let mut b = RdfDatasetBuilder::new();
    let knows = b.intern_iri("http://ex/knows".to_owned());
    let member = b.intern_iri("http://ex/member".to_owned());
    let club = b.intern_iri("http://ex/club".to_owned());
    let mut first_obj = None;
    for i in 0..n {
        let s = b.intern_iri(format!("http://ex/s{i}"));
        let o = b.intern_iri(format!("http://ex/o{i}"));
        b.push_quad(s, knows, o, None);
        if first_obj.is_none() {
            first_obj = Some(o);
        }
    }
    if let Some(o) = first_obj {
        b.push_quad(o, member, club, None);
    }
    b.freeze().expect("freeze")
}

const QUERY: &str = "SELECT ?s ?o WHERE { ?s <http://ex/knows> ?o \
                     FILTER NOT EXISTS { ?o <http://ex/member> ?m } }";

fn run(ds: &RdfDataset, memo: bool) {
    let parsed = SparqlParser::new().parse_query(QUERY).expect("parse");
    let mut ctx = EvalCtx::new(ds);
    ctx.options.exists_memo = memo;
    let outcome = evaluate_query(&parsed, &mut ctx).expect("eval");
    criterion::black_box(outcome);
}

fn bench_exists_decorrelation(c: &mut Criterion) {
    let ds = knows_dataset(1_000);
    let mut group = c.benchmark_group("exists_not_exists_1k_rows");
    group.bench_function("naive_per_row_reeval", |bencher| {
        bencher.iter(|| run(&ds, false));
    });
    group.bench_function("decorrelated_cached_inner", |bencher| {
        bencher.iter(|| run(&ds, true));
    });
    group.finish();
}

criterion_group!(benches, bench_exists_decorrelation);
criterion_main!(benches);
