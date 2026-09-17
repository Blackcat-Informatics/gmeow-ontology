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

#[path = "dag_profile.tests.rs"]
#[cfg(test)]
mod tests;
