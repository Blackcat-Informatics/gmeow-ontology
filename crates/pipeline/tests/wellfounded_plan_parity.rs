// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The intra-engine dogfood parity gate (#1057, Upgrade #4): the well-founded
//! materializer's per-world phase sequence authored as a sub-`logic:Plan` in
//! `slices/core/logic/module.ttl` must walk left-first to EXACTLY the ordered
//! phase descriptor the Rust runtime exposes
//! (`gmeow_logic::WELL_FOUNDED_PHASES`). This is the dogfood seam of
//! [`dag_dogfood.rs`](./dag_dogfood.rs) one scale DOWN: that gate proves the
//! inter-stage build DAG and its Rust `full_spec` never diverge; this one proves
//! the authored intra-engine phase plan and the Rust `materialize()` phase order
//! never diverge.
//!
//! Principle 12 BOUNDARY (NON-NEGOTIABLE): the plan is authored SOURCE this test
//! checks the runtime against — the reasoner NEVER parses this RDF at scheduling
//! or runtime. The native loop in `crates/logic/src/wellfounded.rs` is the
//! runtime; the `logic:wellFoundedMaterializerPlan` is its declared twin.
//!
//! The middle phase is the alternating fixpoint — a `logic:Iteration` (a loop),
//! so the plan is OUTSIDE the certified acyclic DAG fragment by construction;
//! that is correct and expected, and this gate also asserts the iterated phase is
//! exactly the one wrapped in a `logic:Iteration`.

use std::path::{Path, PathBuf};

use gmeow_rdf::{parse_dataset, DatasetView, GraphMatch, RdfDataset, TermRef, TermValue};

const LOGIC: &str = "https://blackcatinformatics.ca/logic/";
const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .canonicalize()
        .unwrap()
}

fn iri(local: &str) -> String {
    format!("{LOGIC}{local}")
}

fn local_name(iri: &str) -> String {
    iri.rsplit(['/', '#']).next().unwrap_or(iri).to_string()
}

/// Every named-node object of `(subject, predicate, _)` in the default graph.
fn named_objects(ds: &RdfDataset, subject: &str, predicate: &str) -> Vec<String> {
    let (Some(s), Some(p)) = (
        ds.term_id_by_value(&TermValue::iri(subject)),
        ds.term_id_by_value(&TermValue::iri(predicate)),
    ) else {
        return Vec::new();
    };
    ds.quads_for_pattern(Some(s), Some(p), None, GraphMatch::Default)
        .filter_map(|q| match ds.resolve(q.o) {
            TermRef::Iri(iri) => Some(iri.to_owned()),
            _ => None,
        })
        .collect()
}

/// The single named-node object of `(subject, predicate, _)`, hard-failing if the
/// edge is missing or multi-valued (every combinator link in the plan is
/// single-valued by construction).
fn one_object(ds: &RdfDataset, subject: &str, predicate: &str) -> String {
    let found = named_objects(ds, subject, predicate);
    assert_eq!(
        found.len(),
        1,
        "expected exactly one <{subject}> <{predicate}> ? edge, found {}",
        found.len()
    );
    found.into_iter().next().unwrap()
}

/// True iff `node` is `a class`.
fn is_a(ds: &RdfDataset, node: &str, class: &str) -> bool {
    let (Some(s), Some(p), Some(o)) = (
        ds.term_id_by_value(&TermValue::iri(node)),
        ds.term_id_by_value(&TermValue::iri(RDF_TYPE)),
        ds.term_id_by_value(&TermValue::iri(class)),
    ) else {
        return false;
    };
    ds.quads_for_pattern(Some(s), Some(p), Some(o), GraphMatch::Default)
        .next()
        .is_some()
}

/// Walk a transaction-program combinator tree LEFT-FIRST from `node`, collecting
/// the local names of the `logic:ActionSchema` phases in execution order.
///
/// - `logic:SerialConjunction` recurses `logic:leftOperand` then
///   `logic:rightOperand`.
/// - `logic:Iteration` recurses `logic:iterationBody` (the loop's body program).
/// - a `logic:ActionSchema` is a leaf — its local name is emitted.
///
/// When an `ActionSchema` is reached through a `logic:Iteration`'s
/// `logic:iterationBody`, its local name is also recorded in `iterated` so the
/// caller can prove which phase is the looped one.
fn collect_phases(ds: &RdfDataset, node: &str, out: &mut Vec<String>, iterated: &mut Vec<String>) {
    if is_a(ds, node, &iri("ActionSchema")) {
        out.push(local_name(node));
        return;
    }
    if is_a(ds, node, &iri("SerialConjunction")) {
        let left = one_object(ds, node, &iri("leftOperand"));
        let right = one_object(ds, node, &iri("rightOperand"));
        collect_phases(ds, &left, out, iterated);
        collect_phases(ds, &right, out, iterated);
        return;
    }
    if is_a(ds, node, &iri("Iteration")) {
        let body = one_object(ds, node, &iri("iterationBody"));
        // The body of a loop is, in execution order, the iterated phase.
        if is_a(ds, &body, &iri("ActionSchema")) {
            iterated.push(local_name(&body));
        }
        collect_phases(ds, &body, out, iterated);
        return;
    }
    panic!("node <{node}> is not a recognised transaction-program combinator or ActionSchema");
}

#[test]
fn authored_phase_plan_matches_runtime_phase_order() {
    let root = repo_root();
    let ttl = std::fs::read_to_string(root.join("slices/core/logic/module.ttl"))
        .expect("read logic slice");

    let ds = parse_dataset(ttl.as_bytes(), "text/turtle", None)
        .expect("parse the dogfooded logic slice");

    // The authored plan and its program-tree root.
    let plan = iri("wellFoundedMaterializerPlan");
    assert!(
        is_a(&ds, &plan, &iri("Plan")),
        "logic:wellFoundedMaterializerPlan must be `a logic:Plan`"
    );
    let body_root = one_object(&ds, &plan, &iri("planBody"));

    // Walk the combinator tree left-first and collect the phase order.
    let mut phases: Vec<String> = Vec::new();
    let mut iterated: Vec<String> = Vec::new();
    collect_phases(&ds, &body_root, &mut phases, &mut iterated);

    // The authored phase order EQUALS the Rust runtime descriptor — the plan is
    // the faithful twin of `materialize()`, never reparsed at runtime (P12).
    assert_eq!(
        phases,
        gmeow_logic::WELL_FOUNDED_PHASES.to_vec(),
        "the authored logic:wellFoundedMaterializerPlan phase order must equal \
         gmeow_logic::WELL_FOUNDED_PHASES (the runtime twin)"
    );

    // Exactly one phase is iterated, and it is the one the runtime declares as the
    // alternating-fixpoint loop — modelled as a logic:Iteration's iterationBody.
    assert_eq!(
        iterated,
        vec![gmeow_logic::WELL_FOUNDED_ITERATED_PHASE.to_string()],
        "the phase wrapped in a logic:Iteration must be exactly \
         gmeow_logic::WELL_FOUNDED_ITERATED_PHASE"
    );
}
