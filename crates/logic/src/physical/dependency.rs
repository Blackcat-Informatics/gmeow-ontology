// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Shared signed dependencies for native producers and rule consumers.
//!
//! Positive recursion belongs to one strongly connected component. A completed
//! read inside that component makes stratification impossible: the producer would
//! need its own final extension before it could finish. Otherwise the condensation
//! graph is a DAG, and a single topological pass computes the least valid strata.
//! No dependency is represented as a synthetic executable rule.

use std::collections::{BTreeMap, BTreeSet};

use petgraph::algo::kosaraju_scc;
use petgraph::graph::{DiGraph, NodeIndex};
use petgraph::visit::EdgeRef;

/// Whether a producer may share a positive fixed point with the relation it reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum ReadDependency {
    Positive,
    /// NAF, universal/absence decisions and structural builtin lookups require a
    /// completed predecessor, rather than a partially grown positive extension.
    Completed,
}

/// A strict read and the strongly connected producer component containing it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DependencyCycle {
    pub(crate) members: Vec<String>,
    pub(crate) head: String,
    pub(crate) read: String,
}

/// Dependency registration owns each symbol once. Graph edges point from
/// the producer being read to the producer that consumes it.
#[derive(Default)]
pub(crate) struct SignedDependencies {
    symbols: BTreeMap<String, NodeIndex>,
    graph: DiGraph<(), ReadDependency>,
}

#[derive(Debug, Clone, Copy)]
struct PredicatePlan {
    component: usize,
    stratum: usize,
}

/// Canonical SCC membership and minimal stratification of the registered effects.
#[derive(Debug)]
pub(crate) struct DependencyPlan {
    predicates: BTreeMap<String, PredicatePlan>,
}

impl DependencyPlan {
    /// Exact positive-recursion component for an admitted predicate.
    pub(crate) fn component(&self, predicate: &str) -> Option<usize> {
        self.predicates.get(predicate).map(|plan| plan.component)
    }

    /// A completed dependency always has a strictly smaller stratum than its head.
    pub(crate) fn stratum(&self, predicate: &str) -> Option<usize> {
        self.predicates.get(predicate).map(|plan| plan.stratum)
    }
}

impl SignedDependencies {
    /// Register a producer or input relation even when it has no reads.
    pub(crate) fn define(&mut self, predicate: &str) -> NodeIndex {
        if let Some(node) = self.symbols.get(predicate) {
            return *node;
        }
        let node = self.graph.add_node(());
        self.symbols.insert(predicate.to_owned(), node);
        node
    }

    /// Register a read directly, without constructing or lowering an executable atom.
    pub(crate) fn read(&mut self, head: &str, read: &str, dependency: ReadDependency) {
        let head = self.define(head);
        let read = self.define(read);
        self.graph.add_edge(read, head, dependency);
    }

    /// Condense positive recursion, reject every strict cycle, and rank the DAG.
    /// SCCs and their members have lexical identity independent of registration order.
    pub(crate) fn plan(self) -> Result<DependencyPlan, DependencyCycle> {
        let mut names = vec![""; self.graph.node_count()];
        for (name, node) in &self.symbols {
            names[node.index()] = name;
        }
        let mut components = kosaraju_scc(&self.graph);
        for component in &mut components {
            component.sort_by_key(|node| names[node.index()]);
        }
        components.sort_by_key(|component| names[component[0].index()]);
        let mut component_of = vec![0; names.len()];
        for (index, component) in components.iter().enumerate() {
            for node in component {
                component_of[node.index()] = index;
            }
        }
        if let Some(edge) = self
            .graph
            .edge_references()
            .filter(|edge| {
                *edge.weight() == ReadDependency::Completed
                    && component_of[edge.source().index()] == component_of[edge.target().index()]
            })
            .min_by_key(|edge| (names[edge.target().index()], names[edge.source().index()]))
        {
            return Err(DependencyCycle {
                members: components[component_of[edge.source().index()]]
                    .iter()
                    .map(|node| names[node.index()].to_owned())
                    .collect(),
                head: names[edge.target().index()].to_owned(),
                read: names[edge.source().index()].to_owned(),
            });
        }
        let mut outgoing = vec![Vec::new(); components.len()];
        let mut indegrees = vec![0usize; components.len()];
        for edge in self.graph.edge_references() {
            let source = component_of[edge.source().index()];
            let target = component_of[edge.target().index()];
            if source != target {
                outgoing[source].push((target, *edge.weight()));
                indegrees[target] += 1;
            }
        }
        let mut ready: BTreeSet<_> = indegrees
            .iter()
            .enumerate()
            .filter_map(|(index, degree)| (*degree == 0).then_some(index))
            .collect();
        let mut strata = vec![0usize; components.len()];
        let mut completed = 0;
        while let Some(source) = ready.pop_first() {
            completed += 1;
            for &(target, dependency) in &outgoing[source] {
                strata[target] = strata[target]
                    .max(strata[source] + usize::from(dependency == ReadDependency::Completed));
                indegrees[target] -= 1;
                if indegrees[target] == 0 {
                    ready.insert(target);
                }
            }
        }
        assert_eq!(
            completed,
            components.len(),
            "SCC condensation must be acyclic"
        );
        Ok(DependencyPlan {
            predicates: self
                .symbols
                .into_iter()
                .map(|(name, node)| {
                    let component = component_of[node.index()];
                    (
                        name,
                        PredicatePlan {
                            component,
                            stratum: strata[component],
                        },
                    )
                })
                .collect(),
        })
    }
}

#[cfg(test)]
mod tests;
