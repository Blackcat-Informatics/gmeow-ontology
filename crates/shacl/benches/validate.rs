// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Baseline benchmark for the SHACL Core validator (#630 acceleration, Phase 0).
//!
//! Sweeps the whole committed conformance corpus through
//! [`gmeow_shacl::engine::validate_graphs`] — parse data + shapes, resolve focus
//! nodes, run every constraint. This is the end-to-end number Phase 2 (regex /
//! subclass-closure / SPARQL caching) and Phase 4 (focus-node `rayon`) move.

use std::fs;
use std::path::PathBuf;

use criterion::{criterion_group, criterion_main, Criterion};
use gmeow_shacl::engine::validate_graphs;

/// Read every `corpus/<case>/{data.nt, shapes.ttl}` pair, sorted by case name.
fn corpus_cases() -> Vec<(String, String, String)> {
    let dir = PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/corpus"));
    let mut paths: Vec<PathBuf> = fs::read_dir(&dir)
        .expect("read corpus dir")
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.is_dir())
        .collect();
    paths.sort();
    paths
        .into_iter()
        .map(|p| {
            let name = p.file_name().unwrap().to_string_lossy().into_owned();
            let data = fs::read_to_string(p.join("data.nt"))
                .unwrap_or_else(|e| panic!("{name}: data.nt: {e}"));
            let shapes = fs::read_to_string(p.join("shapes.ttl"))
                .unwrap_or_else(|e| panic!("{name}: shapes.ttl: {e}"));
            (name, data, shapes)
        })
        .collect()
}

fn bench_validate(c: &mut Criterion) {
    let cases = corpus_cases();

    let mut group = c.benchmark_group("shacl_validate");
    group.bench_function("corpus_all", |b| {
        b.iter(|| {
            for (name, data, shapes) in &cases {
                // Panic (don't silently skip) on a validation failure: a swallowed
                // error would run instantly and report a false speedup (gemini review).
                let report = validate_graphs(data, shapes)
                    .unwrap_or_else(|e| panic!("validation failed for {name}: {e:?}"));
                std::hint::black_box(report);
            }
        });
    });
    group.finish();
}

criterion_group!(benches, bench_validate);
criterion_main!(benches);
