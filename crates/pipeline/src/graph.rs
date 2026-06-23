// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The stage dependency graph: acyclicity (via `petgraph::tarjan_scc`) plus
//! deterministic topological *levelling* (#861).
//!
//! An edge runs **producer → consumer**: if stage `A` declares `B` in its
//! `dataflowConsumes`, then `B` must run before `A`, so the edge is `B → A`.
//! Stages with no unscheduled dependencies form level 0; each subsequent level
//! holds the stages whose every dependency landed in an earlier level. Stages
//! within a level are independent and the scheduler may run them in parallel.
//!
//! Determinism mirrors `gmeow-slice::cache`: nodes and edges are inserted in
//! sorted order, and each level is sorted, so the levelling is identical
//! regardless of input order or completion order.

use std::collections::{BTreeMap, BTreeSet};

use petgraph::graph::{DiGraph, NodeIndex};

use crate::error::PipelineError;

/// The validated execution plan: stages bucketed into topological levels
/// (producers first), plus the flat topological order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StageGraph {
    /// Topological levels, producers first. Stages within a level are mutually
    /// independent (parallel-eligible) and sorted for determinism.
    pub levels: Vec<Vec<String>>,
}

impl StageGraph {
    /// The flat topological order (levels concatenated, producers first).
    pub fn order(&self) -> Vec<String> {
        self.levels.iter().flatten().cloned().collect()
    }

    /// Total number of stages in the plan.
    pub fn len(&self) -> usize {
        self.levels.iter().map(|l| l.len()).sum()
    }

    /// Whether the plan has no stages.
    pub fn is_empty(&self) -> bool {
        self.levels.iter().all(|l| l.is_empty())
    }

    /// Build a levelled, acyclic graph from `(stage_id, consumes)` adjacency.
    ///
    /// `nodes` is the full set of stage ids; `consumes` maps each stage to the
    /// ids it depends on. HARD-fails on a dangling dependency (a consumed id
    /// that is not a node) or any cycle (an SCC with more than one member, or a
    /// self-loop).
    pub fn build(
        nodes: &BTreeSet<String>,
        consumes: &BTreeMap<String, BTreeSet<String>>,
    ) -> Result<Self, PipelineError> {
        // ── Completeness: every CONSUMER key AND every consumed id must be a
        //    known node. A `consumes` entry whose KEY is not in `nodes` would
        //    later panic at `index[stage]` / `in_deps[stage]`; reject it here so
        //    the public `build`/`validate` API hard-fails with a diagnostic
        //    instead of panicking on a malformed adjacency map. ──
        for (stage, deps) in consumes {
            if !nodes.contains(stage) {
                return Err(PipelineError::InvalidDag(format!(
                    "stage {stage} declares dependencies but is not itself a declared stage"
                )));
            }
            for dep in deps {
                if !nodes.contains(dep) {
                    return Err(PipelineError::InvalidDag(format!(
                        "stage {stage} consumes {dep}, which is not a declared stage"
                    )));
                }
                if dep == stage {
                    return Err(PipelineError::InvalidDag(format!(
                        "stage {stage} consumes itself (self-loop)"
                    )));
                }
            }
        }

        // ── Build a producer → consumer DiGraph with deterministic insertion. ──
        let mut graph: DiGraph<String, ()> = DiGraph::new();
        let mut index: BTreeMap<&str, NodeIndex> = BTreeMap::new();
        for id in nodes {
            let idx = graph.add_node(id.clone());
            index.insert(id.as_str(), idx);
        }
        let mut seen: BTreeSet<(&str, &str)> = BTreeSet::new();
        for (stage, deps) in consumes {
            for dep in deps {
                // edge producer(dep) → consumer(stage)
                if !seen.insert((dep.as_str(), stage.as_str())) {
                    continue;
                }
                let (from, to) = (index[dep.as_str()], index[stage.as_str()]);
                graph.add_edge(from, to, ());
            }
        }

        // ── Acyclicity: any SCC with >1 member is a cycle. ──
        for component in petgraph::algo::tarjan_scc(&graph) {
            if component.len() > 1 {
                let mut members: Vec<String> =
                    component.into_iter().map(|n| graph[n].clone()).collect();
                members.sort();
                return Err(PipelineError::InvalidDag(format!(
                    "dependency cycle among stages: {}",
                    members.join(" → ")
                )));
            }
        }

        // ── Topological levelling (Kahn over the dependency in-degree). ──
        // in_deps[stage] = the set of producers it still waits on.
        let mut in_deps: BTreeMap<&str, BTreeSet<&str>> = nodes
            .iter()
            .map(|n| (n.as_str(), BTreeSet::new()))
            .collect();
        for (stage, deps) in consumes {
            let entry = in_deps.get_mut(stage.as_str()).expect("stage is a node");
            for dep in deps {
                entry.insert(dep.as_str());
            }
        }

        let mut levels: Vec<Vec<String>> = Vec::new();
        let mut scheduled: BTreeSet<&str> = BTreeSet::new();
        while scheduled.len() < nodes.len() {
            // This level = every unscheduled stage whose deps are all scheduled.
            let mut level: Vec<String> = in_deps
                .iter()
                .filter(|(id, _)| !scheduled.contains(*id))
                .filter(|(_, deps)| deps.iter().all(|d| scheduled.contains(d)))
                .map(|(id, _)| id.to_string())
                .collect();
            // A non-empty graph with no ready node would be a cycle, already
            // rejected above; treat an empty level as a defensive hard fail.
            if level.is_empty() {
                return Err(PipelineError::InvalidDag(
                    "topological levelling stalled (unreachable: cycle already rejected)"
                        .to_string(),
                ));
            }
            level.sort();
            for id in &level {
                scheduled.insert(nodes.get(id).expect("level id is a node").as_str());
            }
            levels.push(level);
        }

        Ok(StageGraph { levels })
    }
}
