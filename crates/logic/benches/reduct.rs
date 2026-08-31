// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Criterion benchmark for the SoA/predicate-indexed `FactStore` inside the
//! Gelfond-Lifschitz reduct engine (`rule_ir::least_model_of_reduct`), measuring
//! the shipped reduct path rather than a benchmark-only implementation.
//!
//! `least_model_of_reduct` is `pub(crate)`, so the bench drives it through the public
//! entry point [`gmeow_logic::wellfounded::bench_wf_materialize`], which takes a
//! pre-built `WorldStore` reference and a canonical typed program and returns the
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
//! The `WorldStore` and typed program are built ONCE outside `b.iter`; `b.iter` runs
//! only the reduct fixpoint materialization — the code path the SoA index is designed
//! to accelerate.

use criterion::{Criterion, criterion_group, criterion_main};
use gmeow_logic::store::WorldStore;
use gmeow_logic::wellfounded::bench_wf_materialize;
use gmeow_logic_compile::ir::{ContextualScope, LogicAxiom, LogicProgram, LogicRule};

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

fn ancestor_program() -> LogicProgram {
    let atom = |subject, predicate, object| {
        LogicAxiom::new(
            subject,
            predicate,
            object,
            false,
            false,
            ContextualScope::default(),
        )
        .expect("ancestor benchmark atom")
    };
    let make_rule = |name: &str, head: LogicAxiom, body: Vec<LogicAxiom>| {
        let scope = ContextualScope {
            provenance: Some(format!("{NS}{name}")),
            ..ContextualScope::default()
        };
        LogicRule::new(head, body, Vec::new(), scope)
    };
    let parent = format!("{NS}parent");
    let ancestor = format!("{NS}ancestor");
    LogicProgram::new(
        Vec::new(),
        vec![
            make_rule(
                "ruleAncestorBase",
                atom("?X", &ancestor, "?Y"),
                vec![atom("?X", &parent, "?Y")],
            ),
            make_rule(
                "ruleAncestorRec",
                atom("?X", &ancestor, "?Z"),
                vec![atom("?X", &parent, "?Y"), atom("?Y", &ancestor, "?Z")],
            ),
        ],
        Vec::new(),
        None,
    )
}

fn bench_reduct(c: &mut Criterion) {
    // 30-node chain: 30 parent edges → 435 ancestor derivations (n*(n-1)/2).
    // The bench builds the WorldStore and typed program once outside `b.iter`; only
    // the reduct fixpoint materialization runs inside the hot loop.
    const CHAIN_LEN: usize = 30;

    let store = build_parent_chain_store(CHAIN_LEN);
    let program = ancestor_program();

    // Closed-form expected row count:
    //   echo_asserted:      CHAIN_LEN rows  (rule_iri = logic:assert, one per parent)
    //   ruleAncestorBase:   CHAIN_LEN rows  (one per parent edge)
    //   ruleAncestorRec:    CHAIN_LEN*(CHAIN_LEN-1)/2 rows (strictly transitive pairs)
    //   total = 2*CHAIN_LEN + CHAIN_LEN*(CHAIN_LEN-1)/2
    const EXPECTED_ROWS: usize = 2 * CHAIN_LEN + CHAIN_LEN * (CHAIN_LEN - 1) / 2;

    // Smoke-check once outside criterion to catch correctness regressions early.
    let actual = bench_wf_materialize(&store, &program).expect("bench_wf_materialize");
    assert_eq!(
        actual, EXPECTED_ROWS,
        "reduct ancestor closure: expected {EXPECTED_ROWS} rows, got {actual}"
    );

    let edge_count = CHAIN_LEN;
    let mut group = c.benchmark_group("reduct_wf_ancestor");
    // ADVISORY-ONLY (measurement doctrine): wall-clock is report-only and NEVER gates
    // `make check`, so the reduced sample_size is a runtime economy, not a gate risk.
    group.sample_size(10);
    group.bench_function(format!("chain_{edge_count}edges_ancestor_closure"), |b| {
        b.iter(|| {
            let n = bench_wf_materialize(&store, &program).expect("bench_wf_materialize");
            std::hint::black_box(n)
        });
    });
    group.finish();
}

criterion_group!(benches, bench_reduct);
criterion_main!(benches);
