// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The DAG-workflow profile certifier (`logic:DagWorkflowResource`).
//!
//! A "DAG workflow" is the decidable, schedulable *shadow* of the canonical
//! process model: a statically-certified ACYCLIC fragment of `logic:Plan`. This
//! module is the single shared acyclicity certifier the canonical process model
//! and the build pipeline's stage graph (`crates/pipeline`,
//! [`crate::dag_profile`] consumer) both run, so there is one acyclicity
//! authority rather than two parallel copies.
//!
//! The certifier operates on the FLOW-GRAPH form (directed edges) — the
//! counterpart of the structured-program combinator tree (`logic:Choice`,
//! `logic:Iteration`, …). The guarantee it certifies is **acyclicity**: a
//! loop-free graph has a topological order, so the schedule terminates and the
//! reasoning result is `complete-for-fragment`. A cyclic plan stays valid
//! canonically (under a non-DAG contract) but resolves to `unsupported` under
//! the DAG profile, the offending edge disclosed as the `logic:dagCycleWitness`
//! — never silently truncated.

use std::collections::{BTreeMap, BTreeSet};

use petgraph::graph::{DiGraph, NodeIndex};

use crate::result::{CompletenessStatus, EvaluationStatus};

/// The certification verdict of the DAG-workflow profile
/// (`logic:DagWorkflowResource`) over a directed process flow-graph.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DagCertification {
    /// The graph is acyclic: the plan lies in the certified DAG fragment, so the
    /// result reports `complete-for-fragment` (`logic:CompleteForFragment`).
    Certified,
    /// A node depends on itself — the minimal cycle. Carries the offending node
    /// (the `logic:dagCycleWitness` surface).
    SelfLoop(String),
    /// A multi-node dependency cycle. Carries the offending cycle members, sorted
    /// for determinism (the `logic:dagCycleWitness` surface).
    Cycle(Vec<String>),
}

impl DagCertification {
    /// Whether the graph lies in the certified acyclic fragment.
    pub fn is_certified(&self) -> bool {
        matches!(self, DagCertification::Certified)
    }

    /// The offending cycle members for a non-certified verdict — the
    /// `logic:dagCycleWitness` set — or empty when certified. Never silently
    /// truncated: the offending structure is always named.
    pub fn witness(&self) -> Vec<String> {
        match self {
            DagCertification::Certified => Vec::new(),
            DagCertification::SelfLoop(node) => vec![node.clone()],
            DagCertification::Cycle(members) => members.clone(),
        }
    }

    /// Map the verdict onto the typed reasoning-result status axes
    /// ([`EvaluationStatus`], [`CompletenessStatus`]):
    ///
    /// - acyclic ⇒ `(Completed, CompleteForFragment)` — a conclusive,
    ///   complete-for-fragment evaluation;
    /// - cyclic ⇒ `(Unsupported, Unknown)` — the DAG profile has no defined
    ///   complete evaluation for a looping plan; completeness is not defined for
    ///   an unsupported verdict. The plan is still valid under a non-DAG
    ///   contract; the witness ([`Self::witness`]) names what broke acyclicity.
    pub fn result_status(&self) -> (EvaluationStatus, CompletenessStatus) {
        match self {
            DagCertification::Certified => (
                EvaluationStatus::Completed,
                CompletenessStatus::CompleteForFragment,
            ),
            DagCertification::SelfLoop(_) | DagCertification::Cycle(_) => {
                (EvaluationStatus::Unsupported, CompletenessStatus::Unknown)
            }
        }
    }
}

/// Certify that a directed flow-graph is acyclic — the DAG-workflow profile's
/// structural guarantee (`logic:DagWorkflowResource`).
///
/// `edges` are directed dependency edges in **producer → consumer** orientation;
/// node identity is the string id. Returns the offending structure — a
/// self-loop, else the lexicographically-smallest multi-node strongly-connected
/// component (members sorted) — or [`DagCertification::Certified`] when the graph
/// is a DAG.
///
/// Pure and deterministic: edges are de-duplicated and sorted before the graph is
/// built, and among several cycles the lexicographically-smallest is returned, so
/// the verdict (and its witness) is independent of edge iteration order. This is
/// the single certifier `crates/pipeline`'s `StageGraph` delegates to.
pub fn certify_acyclic<'a, I>(edges: I) -> DagCertification
where
    I: IntoIterator<Item = (&'a str, &'a str)>,
{
    // De-dup + sort edges for determinism. The edges borrow `&'a str` slices, so the
    // whole validation runs on borrowed data — no `String` is allocated on the happy
    // (acyclic) path; allocation happens only when a self-loop or cycle witness is built.
    let edge_set: BTreeSet<(&'a str, &'a str)> = edges.into_iter().collect();

    // A self-loop is the minimal cycle; `tarjan_scc` reports a self-looping node
    // as a SIZE-1 component, so the SCC pass below would miss it — detect it here
    // first. `edge_set` is sorted, so the smallest offending node is returned.
    for &(from, to) in &edge_set {
        if from == to {
            return DagCertification::SelfLoop(from.to_string());
        }
    }

    // Build a producer → consumer DiGraph with deterministic (sorted) node
    // insertion so the SCC decomposition is reproducible.
    let mut node_set: BTreeSet<&'a str> = BTreeSet::new();
    for &(from, to) in &edge_set {
        node_set.insert(from);
        node_set.insert(to);
    }
    let mut graph: DiGraph<&'a str, ()> = DiGraph::new();
    let mut index: BTreeMap<&'a str, NodeIndex> = BTreeMap::new();
    for &node in &node_set {
        let idx = graph.add_node(node);
        index.insert(node, idx);
    }
    for &(from, to) in &edge_set {
        graph.add_edge(index[from], index[to], ());
    }

    // Any SCC with more than one member is a cycle. Collect every such cycle
    // (each sorted) and return the lexicographically-smallest, so a graph with
    // several cycles still yields one deterministic witness.
    let mut cycles: Vec<Vec<&'a str>> = Vec::new();
    for component in petgraph::algo::tarjan_scc(&graph) {
        if component.len() > 1 {
            let mut members: Vec<&'a str> = component.into_iter().map(|n| graph[n]).collect();
            members.sort_unstable();
            cycles.push(members);
        }
    }
    cycles.sort_unstable();
    match cycles.into_iter().next() {
        Some(members) => DagCertification::Cycle(members.into_iter().map(str::to_string).collect()),
        None => DagCertification::Certified,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn acyclic_graph_certifies_complete_for_fragment() {
        // a -> b -> c, a -> c : a DAG (the shape of the build pipeline).
        let edges = [("a", "b"), ("b", "c"), ("a", "c")];
        let cert = certify_acyclic(edges.iter().copied());
        assert_eq!(cert, DagCertification::Certified);
        assert!(cert.is_certified());
        assert!(cert.witness().is_empty());
        assert_eq!(
            cert.result_status(),
            (
                EvaluationStatus::Completed,
                CompletenessStatus::CompleteForFragment
            )
        );
    }

    #[test]
    fn multi_node_cycle_is_unsupported_and_names_the_offending_edge() {
        // b -> c -> b is a cycle; a -> b is acyclic context.
        let edges = [("a", "b"), ("b", "c"), ("c", "b")];
        let cert = certify_acyclic(edges.iter().copied());
        assert_eq!(cert, DagCertification::Cycle(vec!["b".into(), "c".into()]));
        // The witness names the offending cycle members — never silent truncation.
        assert_eq!(cert.witness(), vec!["b".to_string(), "c".to_string()]);
        // Cyclic under the DAG profile ⇒ the issue-mandated `unsupported` verdict.
        assert_eq!(
            cert.result_status(),
            (EvaluationStatus::Unsupported, CompletenessStatus::Unknown)
        );
    }

    #[test]
    fn self_loop_is_the_minimal_cycle() {
        let edges = [("a", "b"), ("b", "b")];
        let cert = certify_acyclic(edges.iter().copied());
        assert_eq!(cert, DagCertification::SelfLoop("b".into()));
        assert_eq!(cert.witness(), vec!["b".to_string()]);
        assert_eq!(cert.result_status().0, EvaluationStatus::Unsupported);
    }

    #[test]
    fn cycle_witness_is_deterministic_regardless_of_edge_order() {
        let forward = certify_acyclic([("c", "b"), ("b", "c"), ("a", "b")].iter().copied());
        let shuffled = certify_acyclic([("a", "b"), ("b", "c"), ("c", "b")].iter().copied());
        assert_eq!(forward, shuffled);
        assert_eq!(
            forward,
            DagCertification::Cycle(vec!["b".into(), "c".into()])
        );
    }

    #[test]
    fn empty_graph_certifies() {
        let cert = certify_acyclic(std::iter::empty());
        assert_eq!(cert, DagCertification::Certified);
    }

    /// The W2 conformance case in executable form (mirrors the SHACL fixture
    /// tests/conformance-fixtures/dag-strong-cyclic-plan.ttl): a strong-cyclic
    /// plan with a retry loop is valid canonically, but the SAME plan under the
    /// DAG-workflow profile resolves to `unsupported` with the offending back-edge
    /// named (the recorded loss), while its acyclic projection — the plan with the
    /// back-edge dropped — certifies as complete-for-fragment.
    #[test]
    fn strong_cyclic_plan_vs_its_acyclic_projection() {
        // The strong-cyclic plan's control flow: probe -> recover -> probe (the
        // retry loop / back-edge) and probe -> commit (success).
        let cyclic = [
            ("stepProbe", "stepRecover"),
            ("stepRecover", "stepProbe"), // the recovery back-edge
            ("stepProbe", "stepCommit"),
        ];
        let verdict = certify_acyclic(cyclic.iter().copied());
        // Under the DAG profile the loop is unsupported, and the loss is RECORDED:
        // the witness names exactly the cycle members (the dropped back-edge).
        assert_eq!(
            verdict,
            DagCertification::Cycle(vec!["stepProbe".into(), "stepRecover".into()])
        );
        assert_eq!(verdict.result_status().0, EvaluationStatus::Unsupported);
        assert_eq!(
            verdict.witness(),
            vec!["stepProbe".to_string(), "stepRecover".to_string()],
            "the recorded loss names the loop the DAG projection drops"
        );

        // The acyclic PROJECTION drops the back-edge; the rest certifies cleanly.
        let acyclic = [("stepProbe", "stepRecover"), ("stepProbe", "stepCommit")];
        let projected = certify_acyclic(acyclic.iter().copied());
        assert_eq!(projected, DagCertification::Certified);
        assert_eq!(
            projected.result_status(),
            (
                EvaluationStatus::Completed,
                CompletenessStatus::CompleteForFragment
            )
        );
    }
}
