// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Before/after baseline for exact weighted model counting (#823, Phase 5,
//! item 4 — the bitmask power-set change).
//!
//! [`gmeow_logic::probabilistic::evaluate`] enumerates `2^N` total choices over
//! the independent probabilistic facts. The pre-change `power_set_weights`
//! materialized all `2^N` subsets as `Vec<Fact>` up front; the change streams
//! each subset by its `u64` mask and materializes the fact list on demand,
//! dropping the peak `2^N · Vec<Fact>` allocation.
//!
//! The driver is a program with 17 independent `probability(...)` facts (so
//! `2^17 = 131_072` total choices — near the 2^N stress point and well within
//! `MAX_INDEPENDENT_FACTS = 20`) plus a couple of Horn rules that combine the
//! facts, so the closure/weighting loop does representative work per choice.
//!
//! The probabilistic marginals are EXACT-tested (`assert_eq!` + conformance
//! goldens), so the weight arithmetic must stay byte-identical: this bench only
//! measures the allocation/constant-factor change, never the numeric result.

use criterion::{criterion_group, criterion_main, Criterion};
use gmeow_logic::probabilistic::evaluate;
use gmeow_logic::query_ir::parse_query_program;
use gmeow_logic::store::WorldStore;

const PROFILE: &str = "https://blackcatinformatics.ca/logic/ProbabilisticProfile";
const WORLD: &str = "https://example.org/prob/world";
const BASE: &str = "https://example.org/prob/";

/// Build a probabilistic program with `n` independent `probability(...)` facts
/// plus two Horn rules that fan the facts into a single derived predicate, so
/// the per-choice closure has work to do (rather than a trivial empty model).
fn build_program_src(n: usize) -> String {
    let mut src = format!(
        ":- prefix(ex, '{BASE}').\n\
         :- probability_model(full_independence).\n"
    );
    // n independent Bernoulli facts. Distinct probabilities keep every weight
    // factor non-trivial (no 0/1 short-circuits in the product).
    for i in 0..n {
        // Probabilities in (0,1): 0.3, 0.35, 0.4, ... cycling, all strictly
        // between 0 and 1 so each subset has a genuine product weight.
        let p = 0.3 + ((i % 9) as f64) * 0.05;
        src.push_str(&format!(":- probability(ex:f{i}(ex:s, ex:on), {p}).\n"));
    }
    // wet :- f_i  for each i  → noisy-OR over all facts (every choice that
    // turns on any fact derives ex:wet, exercising the closure + binding path).
    for i in 0..n {
        src.push_str(&format!("ex:wet(S, ex:on) :- ex:f{i}(S, ex:on).\n"));
    }
    src.push_str("?- ex:wet(ex:s, X).\n");
    src
}

fn bench_evaluate(c: &mut Criterion) {
    // 17 independent facts → 2^17 = 131_072 total choices.
    let n = 17usize;
    let src = build_program_src(n);
    let prog = parse_query_program(&src).expect("parse probabilistic program");
    let store = WorldStore::new();

    let mut group = c.benchmark_group("probabilistic_evaluate");
    group.sample_size(10);
    group.bench_function(format!("noisy_or_{n}_indep_facts"), |b| {
        b.iter(|| {
            let ans = evaluate(&store, WORLD, &prog, PROFILE, None).expect("evaluate");
            std::hint::black_box(ans)
        });
    });
    group.finish();
}

criterion_group!(benches, bench_evaluate);
criterion_main!(benches);
