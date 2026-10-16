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
use gmeow_logic::reason::refute::{
    ClassDiagnosticOutcome, NativeFamilyCompletion, NativeProofOrigin, class_diagnostic,
};
use gmeow_logic::reason::{
    DomainProfile, LogicalGraph, SelectedDomains, SelectedLogicalWorld, el_closure,
    prepare_reasoning_input, reason_all,
};
use gmeow_logic_compile::ir::{ContextualScope, LogicAxiom, LogicProgram, LogicRule};
use purrdf::{RdfDataset, RdfDatasetBuilder, RdfLiteral, RdfQuad, RdfTerm};
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
        builder.push_owned_quad(&quad).unwrap();
    }
    builder.freeze().expect("valid benchmark dataset")
}

fn transitivity_program() -> LogicProgram {
    let atom = |subject, object| {
        LogicAxiom::new(
            subject,
            "https://blackcatinformatics.ca/logic/subClassOf",
            gmeow_logic_compile::ir::AtomicTerm::resource(object),
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
        let selection = blake3::hash(format!("{W}:{n}:{inst}").as_bytes());
        let domains = SelectedDomains::new([SelectedLogicalWorld::new(
            LogicalGraph::Named(purrdf::TermValue::iri(W)),
            DomainProfile::NonemptyObjectDomainV1,
            "gmeow.bench.native-hierarchy.v1".to_owned(),
            *selection.as_bytes(),
        )
        .expect("benchmark hierarchy domain")])
        .expect("one benchmark hierarchy world");
        group.bench_function(format!("hierarchy_{n}classes_{inst}inst"), |b| {
            b.iter(|| {
                // Preserve the measured ingress boundary: each sample includes
                // native source preparation as well as the selected closure.
                let input = prepare_reasoning_input(store.as_ref()).expect("benchmark input");
                std::hint::black_box(reason_all(input, &domains).expect("reason_all"))
            });
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

/// Every descriptor has an independent property; only one has an asserted instance.
fn restriction_store(count: usize) -> Arc<RdfDataset> {
    let mut builder = RdfDatasetBuilder::new();
    let mut add = |subject: &str, predicate: &str, object: RdfTerm| {
        builder
            .push_owned_quad(
                &RdfQuad::new(RdfTerm::iri(subject), predicate, object).in_graph(RdfTerm::iri(W)),
            )
            .unwrap();
    };
    const LOGIC: &str = "https://blackcatinformatics.ca/logic/";
    add(
        "urn:class-bench:Filler",
        RDF_TYPE,
        RdfTerm::iri(format!("{LOGIC}Class")),
    );
    // A fixed complement engages the class model independently of descriptor
    // count; an unselected family would measure only its no-op return path.
    add(
        "urn:class-bench:NotFiller",
        RDF_TYPE,
        RdfTerm::iri(format!("{LOGIC}Class")),
    );
    add(
        "urn:class-bench:NotFiller",
        &format!("{LOGIC}complementOf"),
        RdfTerm::iri("urn:class-bench:Filler"),
    );
    for index in 0..count {
        let restriction = format!("urn:class-bench:R{index}");
        let property = format!("urn:class-bench:p{index}");
        add(
            &restriction,
            RDF_TYPE,
            RdfTerm::iri(format!("{LOGIC}Restriction")),
        );
        add(
            &property,
            RDF_TYPE,
            RdfTerm::iri(format!("{LOGIC}ObjectProperty")),
        );
        add(
            &restriction,
            &format!("{LOGIC}onProperty"),
            RdfTerm::iri(property),
        );
        add(
            &restriction,
            &format!("{LOGIC}onClass"),
            RdfTerm::iri("urn:class-bench:Filler"),
        );
        add(
            &restriction,
            &format!("{LOGIC}minQualifiedCardinality"),
            RdfTerm::literal(RdfLiteral::typed(
                "1",
                "http://www.w3.org/2001/XMLSchema#nonNegativeInteger",
            )),
        );
    }
    add(
        "urn:class-bench:instance",
        RDF_TYPE,
        RdfTerm::iri("urn:class-bench:R0"),
    );
    builder.freeze().expect("synthetic restriction dataset")
}

fn bench_class_evidence(c: &mut Criterion) {
    let mut group = c.benchmark_group("class_restrictions");
    group.sample_size(10);
    for count in [1250, 2500, 5000] {
        let store = restriction_store(count);
        let prepared = prepare_reasoning_input(store.as_ref()).expect("class benchmark contexts");
        let domains =
            SelectedDomains::new(prepared.source_contexts().iter().map(|(world, graph)| {
                let selection =
                    blake3::hash(format!("{world}:qualified-restrictions:{count}").as_bytes());
                SelectedLogicalWorld::new(
                    LogicalGraph::from_graph(graph.clone()),
                    DomainProfile::NonemptyObjectDomainV1,
                    "gmeow.bench.class-restrictions.v1".to_owned(),
                    *selection.as_bytes(),
                )
                .expect("class benchmark domain")
            }))
            .expect("all declared class benchmark contexts");
        group.bench_function(format!("descriptors_{count}"), |b| {
            b.iter(|| {
                let input = prepare_reasoning_input(store.as_ref()).expect("class benchmark input");
                let outcome = class_diagnostic(input, &domains, None).expect("class diagnostic");
                let ClassDiagnosticOutcome::Executed { execution } = outcome else {
                    panic!("the synthetic restrictions must execute, not refuse source admission");
                };
                assert!(!execution.classes.is_empty());
                let class = execution
                    .classes
                    .iter()
                    .find(|class| class.world == W)
                    .expect("the descriptor world has a class outcome");
                assert_ne!(class.completion, NativeFamilyCompletion::NotEngaged);
                assert!(
                    execution
                        .proofs
                        .iter()
                        .any(|ledger| !ledger.proofs.is_empty())
                );
                assert_eq!(
                    execution
                        .proofs
                        .iter()
                        .flat_map(|ledger| &ledger.proofs)
                        .filter(|proof| {
                            proof.statement.predicate
                                == "https://blackcatinformatics.ca/logic/onProperty"
                                && matches!(&proof.origin, NativeProofOrigin::Asserted { .. })
                        })
                        .count(),
                    count,
                    "every synthetic descriptor must reach native source-support preparation"
                );
                std::hint::black_box(execution)
            });
        });
    }
    group.finish();
}

criterion_group!(
    benches,
    bench_reason_all,
    bench_el_closure,
    bench_native_forward,
    bench_class_evidence
);
criterion_main!(benches);
