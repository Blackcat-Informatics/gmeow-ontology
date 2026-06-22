// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Criterion benchmark for the SoA/predicate-indexed `FactStore` inside the
//! Gelfond-Lifschitz reduct engine (`rule_ir::least_model_of_reduct`) — Gap 3 of the
//! Phase-5 acceptance criteria (#823).
//!
//! `least_model_of_reduct` is `pub(crate)`, so the bench drives it through the public
//! entry point [`gmeow_logic::wellfounded::bench_wf_materialize`], which takes a
//! pre-built `WorldStore` reference and a Nemo `.rls` rule string and returns the
//! materialized row count.
//! This mirrors the public-path pattern used by `benches/graph.rs` for
//! `entrenchment::closure`.
//!
//! **Synthetic workload:** transitive closure of an `ancestor` relation over a chain of
//! 30 base `parent` edges (`parent(0→1), parent(1→2), …, parent(29→30)`). The
//! recursive rule `ancestor(?X,?Z,?W) :- parent(?X,?Y,?W), ancestor(?Y,?Z,?W)` forces
//! many reduct fixpoint rounds, driving the predicate-indexed delta×full join that the
//! SoA/predicate-index optimization targets.  No NAF literals appear, so the alternating
//! fixpoint converges in one outer iteration (well-founded = least Herbrand model for
//! stratifiable programs), making the timing dominated by the inner reduct join loop.
//!
//! The `WorldStore` is built ONCE outside `b.iter`; `b.iter` runs only rule
//! parsing + the reduct fixpoint materialize — the code path the SoA index is
//! designed to accelerate.

use criterion::{criterion_group, criterion_main, Criterion};
use gmeow_logic::store::WorldStore;
use gmeow_logic::wellfounded::bench_wf_materialize;

const NS: &str = "https://example.org/b/";
const WORLD: &str = "https://example.org/b/world";

/// Build a `WorldStore` with a `parent` chain of length `n`.
fn build_parent_chain_store(n: usize) -> WorldStore {
    let store = WorldStore::new();
    for i in 0..n {
        store.insert_quad(
            WORLD,
            &format!("{NS}node{i}"),
            &format!("{NS}parent"),
            &format!("{NS}node{}", i + 1),
        );
    }
    store
}

/// Build the `.rls` rule text for transitive ancestor closure.
///
/// The world slot (`?W`) is present so the arity-3 gmeow fragment matches; all
/// nodes and edges are in a single world.
/// (No inline comments inside the .rls text — Nemo's parser rejects them.)
fn ancestor_rules() -> String {
    format!(
        "#[name(\"{NS}ruleAncestorBase\")]\n\
         <{NS}ancestor>(?X, ?Y, ?W) :-\n\
             <{NS}parent>(?X, ?Y, ?W) .\n\
         #[name(\"{NS}ruleAncestorRec\")]\n\
         <{NS}ancestor>(?X, ?Z, ?W) :-\n\
             <{NS}parent>(?X, ?Y, ?W),\n\
             <{NS}ancestor>(?Y, ?Z, ?W) .\n"
    )
}

fn bench_reduct(c: &mut Criterion) {
    // 30-node chain: 30 parent edges → 435 ancestor derivations (n*(n-1)/2).
    // The bench builds the WorldStore once outside `b.iter`; only rule parsing +
    // the reduct fixpoint materialize run inside the hot loop — the code path the
    // SoA/predicate-index optimization targets.
    const CHAIN_LEN: usize = 30;

    let store = build_parent_chain_store(CHAIN_LEN);
    let rules = ancestor_rules();

    // Closed-form expected row count:
    //   echo_asserted:      CHAIN_LEN rows  (rule_iri = logic:assert, one per parent)
    //   ruleAncestorBase:   CHAIN_LEN rows  (one per parent edge)
    //   ruleAncestorRec:    CHAIN_LEN*(CHAIN_LEN-1)/2 rows (strictly transitive pairs)
    //   total = 2*CHAIN_LEN + CHAIN_LEN*(CHAIN_LEN-1)/2
    const EXPECTED_ROWS: usize = 2 * CHAIN_LEN + CHAIN_LEN * (CHAIN_LEN - 1) / 2;

    // Smoke-check once outside criterion to catch correctness regressions early.
    let actual = bench_wf_materialize(&store, &rules).expect("bench_wf_materialize");
    assert_eq!(
        actual, EXPECTED_ROWS,
        "reduct ancestor closure: expected {EXPECTED_ROWS} rows, got {actual}"
    );

    let edge_count = CHAIN_LEN;
    let mut group = c.benchmark_group("reduct_wf_ancestor");
    group.sample_size(10);
    group.bench_function(format!("chain_{edge_count}edges_ancestor_closure"), |b| {
        b.iter(|| {
            let n = bench_wf_materialize(&store, &rules).expect("bench_wf_materialize");
            std::hint::black_box(n)
        });
    });
    group.finish();
}

criterion_group!(benches, bench_reduct);
criterion_main!(benches);
