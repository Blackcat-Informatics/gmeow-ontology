// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! End-to-end benchmark for the per-file `ValidationRun::run` path (#822).
//!
//! Exercises the parse-once refactor: each source Turtle file is read and
//! parsed exactly once (instead of ~3× under the old per-phase `parse_file`
//! calls). The benchmark is hermetic — it writes a handful of synthetic Turtle
//! files to `std::env::temp_dir()` and tears them down after setup, so no
//! real ontology files are required.
//!
//! Measured: end-to-end `ValidationRun::run` over the per-file path with 8
//! synthetic Turtle files (a realistic small-to-medium slice count).

use std::collections::{BTreeSet, HashSet};
use std::path::PathBuf;

use criterion::{criterion_group, criterion_main, Criterion};
use gmeow_validate::lint::LintConfig;
use gmeow_validate::validate_all::{ValidateOptions, ValidationRun};

const NS: &str = "https://blackcatinformatics.ca/gmeow#";
const ONT: &str = "https://blackcatinformatics.ca/gmeow";
const RDFS_LABEL: &str = "http://www.w3.org/2000/01/rdf-schema#label";
const SKOS_DEF: &str = "http://www.w3.org/2004/02/skos/core#definition";

/// Minimal SHACL shapes Turtle that always conforms (no constraints).
const EMPTY_SHAPES_TTL: &str = "@prefix sh: <http://www.w3.org/ns/shacl#> .\n";

fn lint_config() -> LintConfig {
    LintConfig {
        namespace: NS.to_owned(),
        ontology_iri: ONT.to_owned(),
        selector_tokens: ["primary", "preferred", "default", "main"]
            .iter()
            .map(|s| s.to_string())
            .collect::<BTreeSet<_>>(),
        core_slice_iris: HashSet::new(),
        annotation_predicates: [RDFS_LABEL, SKOS_DEF]
            .iter()
            .map(|s| s.to_string())
            .collect::<HashSet<_>>(),
    }
}

/// Generate a synthetic Turtle file with `n` triples (all in the GMEOW
/// namespace so the namespace check in structural lint is exercised).
fn synthetic_ttl(n: usize) -> String {
    let mut s = String::with_capacity(n * 128);
    s.push_str(&format!(
        "@prefix gmeow: <{NS}> .\n\
         @prefix owl: <http://www.w3.org/2002/07/owl#> .\n\
         @prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .\n\
         @prefix skos: <http://www.w3.org/2004/02/skos/core#> .\n"
    ));
    for i in 0..n {
        s.push_str(&format!(
            "gmeow:Class{i} a owl:Class ;\n\
             \trdfs:label \"Class {i}\"@en ;\n\
             \trdfs:label \"Class {i}\"@x-gmeow-english ;\n\
             \tskos:definition \"Definition of class {i}.\"@en .\n"
        ));
    }
    s
}

/// Write `count` synthetic Turtle files to temp dir and return their paths.
///
/// Each file contains `triples_per_file` triples. Files are named with the
/// bench process id plus index/count so concurrent bench processes (e.g.
/// parallel worktrees) never collide in the shared temp dir.
fn write_bench_files(count: usize, triples_per_file: usize) -> Vec<PathBuf> {
    let ttl = synthetic_ttl(triples_per_file);
    let pid = std::process::id();
    (0..count)
        .map(|i| {
            let path = std::env::temp_dir()
                .join(format!("gmeow_bench_validate_all_{pid}_{i}_{count}.ttl"));
            std::fs::write(&path, &ttl).expect("write bench Turtle file");
            path
        })
        .collect()
}

fn remove_bench_files(paths: &[PathBuf]) {
    for p in paths {
        std::fs::remove_file(p).ok();
    }
}

fn bench_validate_all(c: &mut Criterion) {
    // 8 files × 50 triples each — representative of a small-to-medium slice
    // set (the real GMEOW core currently has ~20 source files).
    let paths = write_bench_files(8, 50);
    let source_paths: Vec<String> = paths
        .iter()
        .map(|p| p.to_string_lossy().into_owned())
        .collect();
    let config = lint_config();
    let options = ValidateOptions::default();

    let mut group = c.benchmark_group("validate_all");
    group.bench_function("run_per_file_8x50", |b| {
        b.iter(|| {
            std::hint::black_box(
                ValidationRun::run(&source_paths, EMPTY_SHAPES_TTL, "", "", &config, &options)
                    .expect("ValidationRun::run must succeed"),
            )
        });
    });
    group.finish();

    remove_bench_files(&paths);
}

criterion_group!(benches, bench_validate_all);
criterion_main!(benches);
