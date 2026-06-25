// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Baseline benchmark for the native foundation chase (#630 acceleration, Phase 0).
//!
//! Drives [`gmeow_logic::foundation::evaluate`] — the stratified, all-IRI
//! OntoUML chase that Phase 5 targets for a semi-naive rewrite — over the six
//! committed foundation conformance inputs (all-IRI by construction, so the
//! chase never errors on a literal/blank object). Each iteration rebuilds the
//! `WorldStore` so the measured cost includes N-Quads load + the full chase, the
//! same shape the PyO3 seam invokes.
//!
//! A large synthetic case (`large_synthetic`) is also provided to stress the
//! chase with hundreds of typed instances across many worlds — the workload
//! needed to measure any multi-world or within-stratum parallelization benefit.

use std::fs;

use criterion::{criterion_group, criterion_main, Criterion};
use gmeow_logic::foundation::{evaluate, AntiRigidityPolicy};
use gmeow_logic::store::WorldStore;

/// The foundation conformance cases under `conformance/logic/cases/foundation/`.
const CASES: [&str; 7] = [
    "cross-world-rigidity",
    "exactly-one-stereotype",
    "free-role",
    "holonic-emergence",
    "identity-overlap-mixiden",
    "mixrig-kind-under-role",
    "relcomp-under-mediated",
];

fn load_case(name: &str) -> String {
    let path = format!(
        "{}/../../conformance/logic/cases/foundation/{name}/input.nq",
        env!("CARGO_MANIFEST_DIR")
    );
    fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {path}: {e}"))
}

/// Generate a large synthetic N-Quads input with many worlds and many typed
/// instances, designed to stress the foundation chase beyond the microsecond
/// range into multiple milliseconds — the scale at which any parallelization
/// benefit or regression is visible.
///
/// Design:
/// - `num_worlds` independent named-graph worlds.
/// - Per world: `kinds_per_world` Kind classes, each with `roles_per_kind`
///   Role subclasses and `phases_per_kind` Phase subclasses, plus `instances`
///   individuals typed to each Kind and its roles/phases.
/// - All classes carry rdf:type <logic:Kind/Role/Phase>, rdfs:subClassOf
///   chains, and individuals carry rdf:type to their sortal.
///
/// With `num_worlds=40`, `kinds_per_world=8`, `roles_per_kind=3`,
/// `phases_per_kind=2`, `instances=4` this yields ~40×(8+24+16+8×4×6)=
/// approximately 10 000+ quads across 40 worlds, giving a several-ms chase.
fn generate_large_synthetic(
    num_worlds: usize,
    kinds_per_world: usize,
    roles_per_kind: usize,
    phases_per_kind: usize,
    instances_per_type: usize,
) -> String {
    let base = "https://example.org/synth";
    let logic = "https://blackcatinformatics.ca/logic";
    let rdf_type = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
    let rdfs_sub = "http://www.w3.org/2000/01/rdf-schema#subClassOf";

    let mut buf = String::with_capacity(1 << 20); // 1 MiB pre-alloc

    for w in 0..num_worlds {
        let world = format!("{base}/world{w}");
        let quad =
            |s: &str, p: &str, o: &str| -> String { format!("<{s}> <{p}> <{o}> <{world}> .\n") };

        for k in 0..kinds_per_world {
            let kind_iri = format!("{base}/w{w}/Kind{k}");

            // Kind stereotype
            buf.push_str(&quad(&kind_iri, rdf_type, &format!("{logic}/Kind")));

            // Role subclasses of this Kind
            for r in 0..roles_per_kind {
                let role_iri = format!("{base}/w{w}/Kind{k}Role{r}");
                buf.push_str(&quad(&role_iri, rdf_type, &format!("{logic}/Role")));
                buf.push_str(&quad(&role_iri, rdfs_sub, &kind_iri));

                // Instances typed to the Role
                for i in 0..instances_per_type {
                    let inst = format!("{base}/w{w}/inst_k{k}r{r}_{i}");
                    buf.push_str(&quad(&inst, rdf_type, &role_iri));
                }
            }

            // Phase subclasses of this Kind
            for p in 0..phases_per_kind {
                let phase_iri = format!("{base}/w{w}/Kind{k}Phase{p}");
                buf.push_str(&quad(&phase_iri, rdf_type, &format!("{logic}/Phase")));
                buf.push_str(&quad(&phase_iri, rdfs_sub, &kind_iri));

                // Instances typed to the Phase
                for i in 0..instances_per_type {
                    let inst = format!("{base}/w{w}/inst_k{k}p{p}_{i}");
                    buf.push_str(&quad(&inst, rdf_type, &phase_iri));
                }
            }

            // Instances typed directly to the Kind
            for i in 0..instances_per_type {
                let inst = format!("{base}/w{w}/inst_k{k}_{i}");
                buf.push_str(&quad(&inst, rdf_type, &kind_iri));
            }
        }
    }

    buf
}

fn bench_foundation(c: &mut Criterion) {
    let inputs: Vec<(&str, String)> = CASES.iter().map(|n| (*n, load_case(n))).collect();

    let mut group = c.benchmark_group("foundation_evaluate");
    for (name, nq) in &inputs {
        group.bench_function(*name, |b| {
            b.iter(|| {
                let store = WorldStore::new();
                store.load_nquads(nq).expect("load_nquads");
                let out =
                    evaluate(&store, AntiRigidityPolicy::WitnessObligation).expect("evaluate");
                std::hint::black_box(out)
            });
        });
    }
    group.finish();
}

fn bench_foundation_large(c: &mut Criterion) {
    // 40 worlds × (8 Kinds + 24 Roles + 16 Phases) + 40×8×(3+2+1)×4 instances
    // ≈ 40×(8+24+16) + 40×8×24 = 40×48 + 7680 = 1920 + 7680 = 9600 quads.
    // Chase over ~9600 facts in 40 independent worlds → several-ms runtime.
    let nq = generate_large_synthetic(40, 8, 3, 2, 4);
    let quad_count = nq.lines().count();

    let mut group = c.benchmark_group("foundation_evaluate_large");
    // Fewer samples: this bench is slow; 10 samples is enough for the comparison.
    group.sample_size(10);
    group.bench_function(format!("large_synthetic_{quad_count}quads_40worlds"), |b| {
        b.iter(|| {
            let store = WorldStore::new();
            store.load_nquads(&nq).expect("load_nquads");
            let out = evaluate(&store, AntiRigidityPolicy::WitnessObligation).expect("evaluate");
            std::hint::black_box(out)
        });
    });
    group.finish();
}

criterion_group!(benches, bench_foundation, bench_foundation_large);
criterion_main!(benches);
