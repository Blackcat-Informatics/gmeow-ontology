// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Baseline benchmark for the validation-path lints (#630 acceleration, Phase 0).
//!
//! Exercises the whole-store scans the explore pass flagged: `structural_lint`
//! (the language-tag / namespace sweep over every quad) and `collect_typed_terms`.
//! Driven by a synthetic but realistically-shaped GMEOW graph generated in-bench
//! (`owl:Class` terms carrying `@en` + `@x-gmeow-english` labels and definitions),
//! so the bench is self-contained, deterministic, and trivially scalable.

use std::collections::{BTreeSet, HashSet};

use criterion::{criterion_group, criterion_main, Criterion};
use gmeow_validate::lint::{collect_typed_terms, structural_lint, LintConfig};
use gmeow_validate::store::build_store_from_nt;

const NS: &str = "https://blackcatinformatics.ca/gmeow#";
const ONT: &str = "https://blackcatinformatics.ca/gmeow";
const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
const OWL_CLASS: &str = "http://www.w3.org/2002/07/owl#Class";
const RDFS_LABEL: &str = "http://www.w3.org/2000/01/rdf-schema#label";
const SKOS_DEF: &str = "http://www.w3.org/2004/02/skos/core#definition";

fn cfg() -> LintConfig {
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

/// `n` GMEOW-namespaced classes, each with an `@en` + `@x-gmeow-english` label
/// and an `@en` definition — the localizable-literal shape the lints police.
fn synthetic_nt(n: usize) -> String {
    let mut s = String::with_capacity(n * 256);
    for i in 0..n {
        let t = format!("{NS}Class{i}");
        s.push_str(&format!("<{t}> <{RDF_TYPE}> <{OWL_CLASS}> .\n"));
        s.push_str(&format!("<{t}> <{RDFS_LABEL}> \"Label {i}\"@en .\n"));
        s.push_str(&format!(
            "<{t}> <{RDFS_LABEL}> \"Etiquette {i}\"@x-gmeow-english .\n"
        ));
        s.push_str(&format!(
            "<{t}> <{SKOS_DEF}> \"Definition of class {i}.\"@en .\n"
        ));
    }
    s
}

fn bench_lint(c: &mut Criterion) {
    let nt = synthetic_nt(2000);
    let store = build_store_from_nt(&nt).expect("build store from synthetic NT");
    let config = cfg();

    let mut group = c.benchmark_group("validate_lint");
    group.bench_function("structural_lint_2k", |b| {
        b.iter(|| std::hint::black_box(structural_lint(&store, &config)));
    });
    group.bench_function("collect_typed_terms_2k", |b| {
        b.iter(|| std::hint::black_box(collect_typed_terms(&store, &config)));
    });
    group.finish();
}

criterion_group!(benches, bench_lint);
criterion_main!(benches);
