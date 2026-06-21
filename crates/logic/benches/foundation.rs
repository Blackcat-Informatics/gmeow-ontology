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

use std::fs;

use criterion::{criterion_group, criterion_main, Criterion};
use gmeow_logic::foundation::{evaluate, AntiRigidityPolicy};
use gmeow_logic::store::WorldStore;

/// The six foundation conformance cases under `conformance/logic/cases/foundation/`.
const CASES: [&str; 6] = [
    "cross-world-rigidity",
    "exactly-one-stereotype",
    "free-role",
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

criterion_group!(benches, bench_foundation);
criterion_main!(benches);
