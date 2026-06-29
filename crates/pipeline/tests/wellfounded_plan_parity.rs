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

use gmeow_pipeline::stages::source_load::turtle_bytes_into_store;
use oxigraph::model::{NamedNode, Term};
use oxigraph::store::Store;

const LOGIC: &str = "https://blackcatinformatics.ca/logic/";

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .canonicalize()
        .unwrap()
}

fn iri(local: &str) -> NamedNode {
    NamedNode::new(format!("{LOGIC}{local}")).unwrap()
}

fn local_name(iri: &str) -> String {
    iri.rsplit(['/', '#']).next().unwrap_or(iri).to_string()
}

/// The single named-node object of `(subject, predicate, _)`, hard-failing if the
/// edge is missing or multi-valued (every combinator link in the plan is
/// single-valued by construction).
fn one_object(store: &Store, subject: &NamedNode, predicate: &NamedNode) -> NamedNode {
    let mut found: Vec<NamedNode> = Vec::new();
    for quad in store.quads_for_pattern(
        Some(subject.as_ref().into()),
        Some(predicate.as_ref()),
        None,
        None,
    ) {
        let quad = quad.expect("read quad");
        if let Term::NamedNode(nn) = quad.object {
            found.push(nn);
        }
    }
    assert_eq!(
        found.len(),
        1,
        "expected exactly one <{}> <{}> ? edge, found {}",
        subject.as_str(),
        predicate.as_str(),
        found.len()
    );
    found.into_iter().next().unwrap()
}

/// True iff `node` is `a class`.
fn is_a(store: &Store, node: &NamedNode, class: &NamedNode) -> bool {
    let rdf_type = NamedNode::new("http://www.w3.org/1999/02/22-rdf-syntax-ns#type").unwrap();
    store
        .quads_for_pattern(
            Some(node.as_ref().into()),
            Some(rdf_type.as_ref()),
            Some(class.as_ref().into()),
            None,
        )
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
fn collect_phases(
    store: &Store,
    node: &NamedNode,
    out: &mut Vec<String>,
    iterated: &mut Vec<String>,
) {
    if is_a(store, node, &iri("ActionSchema")) {
        out.push(local_name(node.as_str()));
        return;
    }
    if is_a(store, node, &iri("SerialConjunction")) {
        let left = one_object(store, node, &iri("leftOperand"));
        let right = one_object(store, node, &iri("rightOperand"));
        collect_phases(store, &left, out, iterated);
        collect_phases(store, &right, out, iterated);
        return;
    }
    if is_a(store, node, &iri("Iteration")) {
        let body = one_object(store, node, &iri("iterationBody"));
        // The body of a loop is, in execution order, the iterated phase.
        if is_a(store, &body, &iri("ActionSchema")) {
            iterated.push(local_name(body.as_str()));
        }
        collect_phases(store, &body, out, iterated);
        return;
    }
    panic!(
        "node <{}> is not a recognised transaction-program combinator or ActionSchema",
        node.as_str()
    );
}

#[test]
fn authored_phase_plan_matches_runtime_phase_order() {
    let root = repo_root();
    let ttl = std::fs::read_to_string(root.join("slices/core/logic/module.ttl"))
        .expect("read logic slice");

    let store = Store::new().expect("create store");
    turtle_bytes_into_store(&store, ttl.as_bytes(), "wellfounded-plan-parity")
        .expect("parse the dogfooded logic slice");

    // The authored plan and its program-tree root.
    let plan = iri("wellFoundedMaterializerPlan");
    assert!(
        is_a(&store, &plan, &iri("Plan")),
        "logic:wellFoundedMaterializerPlan must be `a logic:Plan`"
    );
    let body_root = one_object(&store, &plan, &iri("planBody"));

    // Walk the combinator tree left-first and collect the phase order.
    let mut phases: Vec<String> = Vec::new();
    let mut iterated: Vec<String> = Vec::new();
    collect_phases(&store, &body_root, &mut phases, &mut iterated);

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
