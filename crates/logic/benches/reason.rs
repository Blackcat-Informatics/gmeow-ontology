// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Baseline benchmark for the native reasoning hot path (T9, #790) — the headline
//! "reasoning (materialize / native EL-DL)" workload that the existing benches
//! (foundation chase, SHACL validate, RDF layout) did not cover.
//!
//! Three groups, each over a small + a larger synthetic input so a regression in
//! either the constant factor or the scaling is visible:
//! - `reason_all` — the native EL/DL/RL reasoner (#665/#686 authority): one Nemo
//!   chase yielding the subsumption closure + the consistency verdict, over a
//!   `VecRdfStore` class hierarchy (the same store shape the PyO3 `reason_native`
//!   seam drives).
//! - `el_closure` — just the EL subsumption closure (the cheaper sub-path).
//! - `materialize_core` — the Nemo forward-chase materialize over a transitive
//!   `subClassOf` chain (O(n^2) derived facts), reusing the materialize.rs test
//!   ruleset.

use criterion::{criterion_group, criterion_main, Criterion};
use gmeow_logic::materialize::materialize_core;
use gmeow_logic::reason::{el_closure, reason_all};
use gmeow_rdf::{RdfQuad, RdfTerm, VecRdfStore};

const W: &str = "http://gmeow.example/w";
const RDFS_SUB: &str = "http://www.w3.org/2000/01/rdf-schema#subClassOf";
const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";

/// A class-subsumption EDB: a `C0 ⊑ C1 ⊑ … ⊑ C{n-1}` chain plus `instances`
/// individuals typed to `C0`, so the EL/DL closure propagates the whole chain.
fn hierarchy_store(num_classes: usize, instances: usize) -> VecRdfStore {
    let cls = |i: usize| format!("http://gmeow.example/C{i}");
    let mut quads = Vec::new();
    for i in 0..num_classes.saturating_sub(1) {
        quads.push(
            RdfQuad::new(RdfTerm::iri(cls(i)), RDFS_SUB, RdfTerm::iri(cls(i + 1)))
                .in_graph(RdfTerm::iri(W)),
        );
    }
    for j in 0..instances {
        quads.push(
            RdfQuad::new(
                RdfTerm::iri(format!("http://gmeow.example/x{j}")),
                RDF_TYPE,
                RdfTerm::iri(cls(0)),
            )
            .in_graph(RdfTerm::iri(W)),
        );
    }
    VecRdfStore::with_quads(quads)
}

/// `logic:subClassOf` transitivity rule in Nemo IRI-predicate syntax (mirrors the
/// `materialize.rs` test ruleset).
const TRANSITIVITY_RULES: &str = concat!(
    "<https://blackcatinformatics.ca/logic/subClassOf>(?X, ?Z, ?C0) :-\n",
    "    <https://blackcatinformatics.ca/logic/subClassOf>(?X, ?Y, ?C0),\n",
    "    <https://blackcatinformatics.ca/logic/subClassOf>(?Y, ?Z, ?C1) .\n",
);

/// A `C0 → C1 → … → C{n}` `logic:subClassOf` chain as N-Quads (one world); the
/// transitive chase derives the full O(n^2) closure.
fn chain_nquads(n: usize) -> String {
    let mut s = String::with_capacity(n * 160);
    for i in 0..n {
        s.push_str(&format!(
            "<http://example.org/C{i}> <https://blackcatinformatics.ca/logic/subClassOf> <http://example.org/C{}> <http://world/Alpha> .\n",
            i + 1
        ));
    }
    s
}

fn bench_reason_all(c: &mut Criterion) {
    let mut group = c.benchmark_group("reason_all");
    group.sample_size(20);
    for &(n, inst) in &[(8usize, 4usize), (30usize, 15usize)] {
        let store = hierarchy_store(n, inst);
        group.bench_function(format!("hierarchy_{n}classes_{inst}inst"), |b| {
            b.iter(|| std::hint::black_box(reason_all(&store).expect("reason_all")));
        });
    }
    group.finish();
}

fn bench_el_closure(c: &mut Criterion) {
    let mut group = c.benchmark_group("el_closure");
    group.sample_size(20);
    for &(n, inst) in &[(8usize, 4usize), (30usize, 15usize)] {
        let store = hierarchy_store(n, inst);
        group.bench_function(format!("hierarchy_{n}classes_{inst}inst"), |b| {
            b.iter(|| std::hint::black_box(el_closure(&store).expect("el_closure")));
        });
    }
    group.finish();
}

fn bench_materialize_core(c: &mut Criterion) {
    let mut group = c.benchmark_group("materialize_core");
    group.sample_size(20);
    for &n in &[8usize, 40usize] {
        let input = chain_nquads(n);
        group.bench_function(format!("transitive_chain_{n}"), |b| {
            b.iter(|| {
                std::hint::black_box(
                    materialize_core(TRANSITIVITY_RULES, &input, None, None, None)
                        .expect("materialize_core"),
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
    bench_materialize_core
);
criterion_main!(benches);
