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

use gmeow_logic_compile::ir::LOGIC_NAMESPACE;
use petgraph::graph::{DiGraph, NodeIndex};

use crate::result::{
    CompletenessStatus, EvaluationStatus, InformationState, InputStatus, PreservationClaim,
    ReasoningResult, ResultPayload, ResultProvenance,
};

/// The `logic:` namespace the DAG-workflow resource IRI is minted under (the same
/// namespace `teleology::emit_dag_certification` writes the verdict's individuals in).
const DAG_PROFILE_NAMESPACE: &str = LOGIC_NAMESPACE;

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

    /// Lower this DAG-workflow verdict into the typed [`ReasoningResult`] a build
    /// run (or a reified `logic:Plan`) surfaces — the Rust-struct counterpart of
    /// the RDF `logic:ReasoningResult` `teleology::emit_dag_certification` emits.
    ///
    /// It REUSES the SAME [`Self::result_status`] / [`Self::witness`] mapping the
    /// RDF emitter runs, so the two surfaces agree by construction:
    ///
    /// - **Certified** ⇒ `(Completed, CompleteForFragment)` with an empty
    ///   `{exact}` preservation claim and [`InformationState::Supported`] — the
    ///   plan lies in the certified acyclic fragment; the certification verdict is
    ///   itself supported, conclusively (complete-for-fragment).
    /// - **SelfLoop / Cycle** ⇒ `(Unsupported, Unknown)` — the DAG profile has no
    ///   defined evaluation for a looping plan. The offending cycle members
    ///   ([`Self::witness`]) are disclosed as the preservation claim's
    ///   `unsupported_constructs` (the typed "constructs the profile could not
    ///   carry" set, with the `{unsupported}` polarity), never silently dropped,
    ///   and the information axis is [`InformationState::NotEvaluated`] (the engine
    ///   could not look — the `unsupported`-contract floor, mirroring
    ///   [`ReasoningResult::invalid`]).
    ///
    /// `world` is the named-graph IRI the verdict holds in; `contract_hash`
    /// identifies the DAG-workflow contract the plan executed under. The provenance
    /// bundle is otherwise minimal (no proof/counterproof — a certification verdict
    /// is a structural property, not a derived conclusion).
    pub fn into_reasoning_result(
        &self,
        contract_hash: impl Into<String>,
        world: impl Into<String>,
    ) -> ReasoningResult {
        let (evaluation, completeness) = self.result_status();
        let witness = self.witness();
        let mut provenance = ResultProvenance::native(contract_hash, world);
        let (preservation, information) = if self.is_certified() {
            (PreservationClaim::exact(), InformationState::Supported)
        } else {
            // The legalization floor: the looping plan was refused as unsupported
            // under the DAG profile and never evaluated. The offending cycle members
            // are the unsupported constructs the lowering could not carry.
            (
                PreservationClaim::unsupported_with(witness.iter().cloned()),
                InformationState::NotEvaluated,
            )
        };
        provenance.projection_class = preservation.clone();
        if self.is_certified() {
            // The certified fragment backing the complete-for-fragment claim is the
            // DAG-workflow resource itself (the profile the plan was certified under).
            provenance.certified_fragment =
                Some(format!("{}DagWorkflowResource", DAG_PROFILE_NAMESPACE));
        }
        ReasoningResult::new(
            InputStatus::Valid,
            evaluation,
            completeness,
            preservation,
            information,
            provenance,
            ResultPayload::Empty,
        )
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

    #[test]
    fn certified_verdict_lowers_to_a_complete_for_fragment_reasoning_result() {
        let cert = certify_acyclic([("a", "b"), ("b", "c")].iter().copied());
        let result = cert.into_reasoning_result("contract:dag-test", "urn:world:test");
        // The typed result agrees with the RDF emitter's status mapping.
        assert_eq!(result.evaluation, EvaluationStatus::Completed);
        assert_eq!(result.completeness, CompletenessStatus::CompleteForFragment);
        assert_eq!(result.information, InformationState::Supported);
        // A certified plan carries an exact (loss-free) preservation claim and names
        // the fragment backing the complete-for-fragment claim.
        assert_eq!(result.preservation, PreservationClaim::exact());
        assert!(result
            .provenance
            .certified_fragment
            .as_deref()
            .is_some_and(|f| f.ends_with("DagWorkflowResource")));
        assert!(result.validate().is_ok());
    }

    #[test]
    fn cyclic_verdict_lowers_to_an_unsupported_reasoning_result_carrying_the_witness() {
        // A SYNTHETIC cyclic verdict (no cyclic pipeline needed): the DAG profile
        // refuses the loop, and the witness members are disclosed on the typed result.
        let cert = DagCertification::Cycle(vec!["stepProbe".into(), "stepRecover".into()]);
        let result = cert.into_reasoning_result("contract:dag-test", "urn:world:test");
        // The issue-mandated `unsupported` verdict, mirroring the RDF emitter.
        assert_eq!(result.evaluation, EvaluationStatus::Unsupported);
        assert_eq!(result.completeness, CompletenessStatus::Unknown);
        // The engine could not look — the unsupported-contract floor.
        assert_eq!(result.information, InformationState::NotEvaluated);
        // The offending cycle members are carried as the unsupported constructs the
        // DAG profile could not lower — never silently truncated.
        assert_eq!(
            result.preservation.unsupported_constructs,
            ["stepProbe".to_string(), "stepRecover".to_string()]
                .into_iter()
                .collect()
        );
        assert_eq!(
            result.preservation,
            PreservationClaim::unsupported_with(cert.witness())
        );
        assert!(result.provenance.certified_fragment.is_none());
        assert!(result.validate().is_ok());
    }

    #[test]
    fn self_loop_verdict_also_lowers_to_unsupported_and_names_the_node() {
        let cert = DagCertification::SelfLoop("stepStuck".into());
        let result = cert.into_reasoning_result("contract:dag-test", "urn:world:test");
        assert_eq!(result.evaluation, EvaluationStatus::Unsupported);
        assert_eq!(
            result.preservation.unsupported_constructs,
            ["stepStuck".to_string()].into_iter().collect()
        );
        assert!(result.validate().is_ok());
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
