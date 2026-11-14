// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Preserve source context that the classical formula IR does not yet represent,
//! and say which evidence the problem leaves out.

use std::collections::{BTreeMap, BTreeSet};

use super::super::{
    EdgeScope, PreparedLogicSource, SourceEdgeRole, SourceNode, StructuralSourceGraph,
};
use crate::ir::EvidenceCoordinate;

const IN_CONTEXT: &str = "https://blackcatinformatics.ca/logic/inContext";

/// The scope that source edges give each formula, including every ancestor.
#[derive(Default)]
pub(super) struct FormulaScopes {
    /// Context coordinates not yet represented in the emitted formula.
    context: BTreeMap<SourceNode, BTreeSet<String>>,
    /// Evidence coordinates the classical problem leaves out.
    evidence: BTreeMap<SourceNode, BTreeSet<EvidenceCoordinate>>,
}

impl FormulaScopes {
    /// The context coordinates on this formula or any descendant, by name.
    pub(super) fn context(&self, node: SourceNode) -> Option<&BTreeSet<String>> {
        self.context.get(&node)
    }

    /// The evidence coordinates on this formula or any descendant.
    pub(super) fn evidence(&self, node: SourceNode) -> BTreeSet<EvidenceCoordinate> {
        self.evidence.get(&node).cloned().unwrap_or_default()
    }
}

/// Index affected formula ancestors once, retaining graph-local native identities.
/// The frontend already translates `inContext`; every other context coordinate may
/// not disappear when atom routing or owned subformulas erase their source
/// wrappers. Evidence edges (`provenance`, `confidence`) select no context: they
/// are recorded per formula and left out of the problem.
pub(super) fn formula_scopes(source: &PreparedLogicSource) -> FormulaScopes {
    let mut parents = BTreeMap::<SourceNode, Vec<SourceNode>>::new();
    let mut scopes = FormulaScopes::default();
    for edge in source.source_graph().edges() {
        if edge.role == SourceEdgeRole::FormulaComponent {
            parents
                .entry(SourceNode {
                    term: edge.target,
                    graph: edge.source.graph,
                })
                .or_default()
                .push(edge.source);
            continue;
        }
        let predicate = edge.predicate.iri(source.dataset());
        match StructuralSourceGraph::edge_scope(edge.role, predicate) {
            EdgeScope::Context(_) if predicate == IN_CONTEXT => {}
            EdgeScope::Context(name) => {
                scopes.context.entry(edge.source).or_default().insert(name);
            }
            EdgeScope::Evidence(coordinate) => {
                scopes
                    .evidence
                    .entry(edge.source)
                    .or_default()
                    .insert(coordinate);
            }
            EdgeScope::None => {}
        }
    }
    propagate(&mut scopes.context, &parents);
    propagate(&mut scopes.evidence, &parents);
    scopes
}

/// Carry each formula's coordinates up to every formula that contains it.
fn propagate<T: Ord + Clone>(
    scopes: &mut BTreeMap<SourceNode, BTreeSet<T>>,
    parents: &BTreeMap<SourceNode, Vec<SourceNode>>,
) {
    let mut pending = scopes.keys().copied().collect::<Vec<_>>();
    while let Some(child) = pending.pop() {
        let Some(ancestors) = parents.get(&child) else {
            continue;
        };
        let carried = scopes.get(&child).cloned().unwrap_or_default();
        for &parent in ancestors {
            let entry = scopes.entry(parent).or_default();
            let before = entry.len();
            entry.extend(carried.iter().cloned());
            if entry.len() != before {
                pending.push(parent);
            }
        }
    }
}

impl super::AdmissionBuilder<'_> {
    /// The scope a source node's own edges give it: the context coordinates it
    /// selects, by name, and the evidence coordinates it carries.
    pub(super) fn node_scope(
        &self,
        node: SourceNode,
    ) -> (BTreeSet<String>, BTreeSet<EvidenceCoordinate>) {
        let source = self.theory.source();
        let mut context = BTreeSet::new();
        let mut evidence = BTreeSet::new();
        for edge in source.source_graph().outgoing(node) {
            match StructuralSourceGraph::edge_scope(edge.role, edge.predicate.iri(source.dataset()))
            {
                EdgeScope::Context(name) => {
                    context.insert(name);
                }
                EdgeScope::Evidence(coordinate) => {
                    evidence.insert(coordinate);
                }
                EdgeScope::None => {}
            }
        }
        (context, evidence)
    }
}
