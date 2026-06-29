// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The stage dependency graph: acyclicity (delegated to the shared
//! `gmeow_logic::dag_profile` DAG-workflow certifier) plus deterministic
//! topological *levelling* (#861).
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

use gmeow_logic::dag_profile::{certify_acyclic, DagCertification};

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
        //    later panic at `in_deps[stage]`; reject it here so the public
        //    `build`/`validate` API hard-fails with a diagnostic instead of
        //    panicking on a malformed adjacency map. (Self-loops and cycles are
        //    the certifier's job, below.) ──
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
            }
        }

        // ── Acyclicity: delegate to the shared DAG-workflow certifier
        //    (`gmeow_logic::dag_profile`) so the build DAG and any logic:Plan run
        //    the SAME acyclicity authority. Edge orientation producer(dep) →
        //    consumer(stage); the certifier maps a cyclic verdict to the
        //    offending witness, which we render as the existing InvalidDag
        //    diagnostics so callers (and `dag_dogfood`) see identical messages. ──
        let edges: Vec<(&str, &str)> = consumes
            .iter()
            .flat_map(|(stage, deps)| deps.iter().map(move |dep| (dep.as_str(), stage.as_str())))
            .collect();
        match certify_acyclic(edges.iter().copied()) {
            DagCertification::Certified => {}
            DagCertification::SelfLoop(stage) => {
                return Err(PipelineError::InvalidDag(format!(
                    "stage {stage} consumes itself (self-loop)"
                )));
            }
            DagCertification::Cycle(members) => {
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
