// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Baseline benchmark for the native reasoning hot path (T9) — the headline
//! "reasoning (materialize / native EL-DL)" workload that the existing benches
//! (foundation chase, SHACL validate, RDF layout) did not cover.
//!
//! Three groups, each over a small + a larger synthetic input so a regression in
//! either the constant factor or the scaling is visible:
//! - `reason_all` — the native EL/DL/RL reasoner (authority): one structured
//!   chase yielding the subsumption closure + the consistency verdict, over a
//!   `RdfDataset` class hierarchy (the same IR shape the PyO3 `reason_native`
//!   seam drives).
//! - `el_closure` — the EL subsumption closure alone (the sub-path of `reason_all`
//!   without the DL consistency verdict; whether it is meaningfully cheaper at a
//!   given size is exactly what the bench measures).
//! - `run_native_forward` — the native forward chase over a transitive
//!   `subClassOf` chain (O(n^2) derived facts), reusing the materialization test
//!   ruleset.

use criterion::{Criterion, criterion_group, criterion_main};
use gmeow_logic::cost::run_native_forward;
use gmeow_logic::reason::{el_closure, reason_all};
use gmeow_logic_compile::ir::{ContextualScope, LogicAxiom, LogicProgram, LogicRule};
use purrdf::{RdfDataset, RdfDatasetBuilder, RdfQuad, RdfTerm};
use std::sync::Arc;

const W: &str = "http://gmeow.example/w";
const RDFS_SUB: &str = "http://www.w3.org/2000/01/rdf-schema#subClassOf";
const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";

/// A class-subsumption EDB: a `C0 ⊑ C1 ⊑ … ⊑ C{n-1}` chain plus `instances`
/// individuals typed to `C0`, so the EL/DL closure propagates the whole chain.
fn hierarchy_store(num_classes: usize, instances: usize) -> Arc<RdfDataset> {
    let cls = |i: usize| format!("http://gmeow.example/C{i}");
    let mut quads = Vec::new();
    for i in 0..num_classes.saturating_sub(1) {
        quads.push(
            RdfQuad::new(RdfTerm::iri(cls(i)), RDFS_SUB, RdfTerm::iri(cls(i + 1)))
                .in_graph(RdfTerm::iri(W)),
        );
    }
    let c0 = cls(0);
    for j in 0..instances {
        quads.push(
            RdfQuad::new(
                RdfTerm::iri(format!("http://gmeow.example/x{j}")),
                RDF_TYPE,
                RdfTerm::iri(c0.clone()),
            )
            .in_graph(RdfTerm::iri(W)),
        );
    }
    let mut builder = RdfDatasetBuilder::new();
    for quad in quads {
        builder.push_owned_quad(&quad);
    }
    builder.freeze().expect("valid benchmark dataset")
}

fn transitivity_program() -> LogicProgram {
    let atom = |subject, object| {
        LogicAxiom::new(
            subject,
            "https://blackcatinformatics.ca/logic/subClassOf",
            object,
            false,
            false,
            ContextualScope::default(),
        )
        .expect("transitivity benchmark atom")
    };
    let scope = ContextualScope {
        provenance: Some("urn:gmeow:benchmark-rule:transitivity".to_owned()),
        ..ContextualScope::default()
    };
    LogicProgram::new(
        Vec::new(),
        vec![LogicRule::new(
            atom("?X", "?Z"),
            vec![atom("?X", "?Y"), atom("?Y", "?Z")],
            Vec::new(),
            scope,
        )],
        Vec::new(),
        None,
    )
}

/// A `C0 → C1 → … → C{n}` `logic:subClassOf` chain as N-Quads (one world); the
/// transitive chase derives the full O(n^2) closure.
fn chain_nquads(n: usize) -> String {
    use std::fmt::Write as _;
    let mut s = String::with_capacity(n * 160);
    for i in 0..n {
        // write! into a String is infallible — no intermediate format! allocation.
        let _ = writeln!(
            s,
            "<http://example.org/C{i}> <https://blackcatinformatics.ca/logic/subClassOf> <http://example.org/C{}> <http://world/Alpha> .",
            i + 1
        );
    }
    s
}

fn bench_reason_all(c: &mut Criterion) {
    let mut group = c.benchmark_group("reason_all");
    // ADVISORY-ONLY (measurement doctrine): wall-clock is report-only and NEVER gates
    // `make check`, so the reduced sample_size is a runtime economy, not a gate risk.
    group.sample_size(20);
    for &(n, inst) in &[(8usize, 4usize), (30usize, 15usize)] {
        let store = hierarchy_store(n, inst);
        group.bench_function(format!("hierarchy_{n}classes_{inst}inst"), |b| {
            b.iter(|| std::hint::black_box(reason_all(store.as_ref()).expect("reason_all")));
        });
    }
    group.finish();
}

fn bench_el_closure(c: &mut Criterion) {
    let mut group = c.benchmark_group("el_closure");
    // ADVISORY-ONLY (measurement doctrine): wall-clock is report-only and NEVER gates
    // `make check`, so the reduced sample_size is a runtime economy, not a gate risk.
    group.sample_size(20);
    for &(n, inst) in &[(8usize, 4usize), (30usize, 15usize)] {
        let store = hierarchy_store(n, inst);
        group.bench_function(format!("hierarchy_{n}classes_{inst}inst"), |b| {
            b.iter(|| std::hint::black_box(el_closure(store.as_ref()).expect("el_closure")));
        });
    }
    group.finish();
}

fn bench_native_forward(c: &mut Criterion) {
    let mut group = c.benchmark_group("native_forward");
    // ADVISORY-ONLY (measurement doctrine): wall-clock is report-only and NEVER gates
    // `make check`, so the reduced sample_size is a runtime economy, not a gate risk.
    group.sample_size(20);
    let program = transitivity_program();
    for &n in &[8usize, 40usize] {
        let input = chain_nquads(n);
        let dataset = purrdf::parse_dataset(input.as_bytes(), "application/n-quads", None)
            .expect("benchmark N-Quads parse");
        group.bench_function(format!("transitive_chain_{n}"), |b| {
            b.iter(|| {
                std::hint::black_box(
                    run_native_forward(dataset.as_ref(), &program).expect("run_native_forward"),
                )
            });
        });
    }
    group.finish();
}

criterion_group!(
    benches,
    bench_reason_all,
    bench_el_closure,
    bench_native_forward
);
criterion_main!(benches);
