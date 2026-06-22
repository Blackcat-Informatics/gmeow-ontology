// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Before/after baseline for the dense-id graph algorithms (#823, Phase 5).
//!
//! Two hot graph algorithms are lowered from `BTreeMap<String, …>` adjacency to
//! dense `u32` ids + `Vec<u64>` bitsets:
//!
//! - [`gmeow_logic::certify::tarjan_scc`] — benched directly (it is `pub`), over
//!   a large synthetic dependency graph of chains plus interleaved cycles.
//! - `entrenchment::closure` — private, reachable only via
//!   [`gmeow_logic::entrenchment::Entrenchment::read_from_world`], so it is
//!   benched through that public path over a large `gmeow:overrides` DAG (the
//!   transitive-closure step is the dominant cost there). The DAG is acyclic so
//!   `read_from_world` does not reject it as an entrenchment cycle.

use std::collections::BTreeMap;

use criterion::{criterion_group, criterion_main, Criterion};
use gmeow_logic::certify::tarjan_scc;
use gmeow_logic::entrenchment::{Entrenchment, OVERRIDES};
use gmeow_logic::store::WorldStore;

/// Build a synthetic dependency graph: `chains` disjoint chains of `chain_len`
/// nodes each (each node depends on the next), with a back-edge closing every
/// `cycle_period`-th chain into a cycle. This yields many trivial SCCs plus a
/// scattering of multi-node SCCs — the shape Tarjan walks in the certifier.
fn build_scc_graph(
    chains: usize,
    chain_len: usize,
    cycle_period: usize,
) -> BTreeMap<String, Vec<String>> {
    let mut g: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for c in 0..chains {
        for n in 0..chain_len {
            let node = format!("https://example.org/n/c{c:05}/v{n:05}");
            let mut succ = Vec::new();
            if n + 1 < chain_len {
                succ.push(format!("https://example.org/n/c{c:05}/v{:05}", n + 1));
            }
            // Close some chains into a cycle (last → first) to make real SCCs.
            if n + 1 == chain_len && c % cycle_period == 0 {
                succ.push(format!("https://example.org/n/c{c:05}/v{:05}", 0));
            }
            g.insert(node, succ);
        }
    }
    g
}

/// Build a large acyclic `gmeow:overrides` graph as N-Quads in one world: a wide
/// layered DAG (each node in layer `l` overrides several nodes in layer `l+1`),
/// so the transitive closure fans out substantially — exercising `closure`.
fn build_overrides_nquads(layers: usize, width: usize, fanout: usize) -> String {
    let world = "https://example.org/world/base";
    let mut buf = String::with_capacity(1 << 20);
    for l in 0..layers - 1 {
        for w in 0..width {
            let s = format!("https://example.org/o/L{l:04}_{w:04}");
            for f in 0..fanout {
                let t = (w + f) % width;
                let o = format!("https://example.org/o/L{:04}_{t:04}", l + 1);
                buf.push_str(&format!("<{s}> <{OVERRIDES}> <{o}> <{world}> .\n"));
            }
        }
    }
    buf
}

fn bench_tarjan(c: &mut Criterion) {
    // 2000 chains × 5 nodes = 10 000 nodes; every 7th chain is a 5-node cycle.
    let graph = build_scc_graph(2000, 5, 7);
    let node_count = graph.len();
    let mut group = c.benchmark_group("tarjan_scc");
    group.bench_function(format!("synthetic_{node_count}nodes"), |b| {
        b.iter(|| {
            let out = tarjan_scc(&graph);
            std::hint::black_box(out)
        });
    });
    group.finish();
}

fn bench_closure(c: &mut Criterion) {
    // 30 layers × 40 wide × fanout 4 → a dense layered DAG; transitive closure
    // from upper layers reaches the whole downstream cone.
    let nq = build_overrides_nquads(30, 40, 4);
    let edge_count = nq.lines().count();
    let world = "https://example.org/world/base";

    let mut group = c.benchmark_group("entrenchment_closure");
    group.sample_size(20);
    group.bench_function(format!("overrides_dag_{edge_count}edges"), |b| {
        b.iter(|| {
            let store = WorldStore::new();
            store.load_nquads(&nq).expect("load_nquads");
            let e = Entrenchment::read_from_world(&store, world).expect("read_from_world");
            std::hint::black_box(e)
        });
    });
    group.finish();
}

criterion_group!(benches, bench_tarjan, bench_closure);
criterion_main!(benches);
