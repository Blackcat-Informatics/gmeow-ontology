// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Preserve source context that the classical formula IR does not yet represent.

use std::collections::{BTreeMap, BTreeSet};

use super::super::{PreparedLogicSource, SourceEdgeRole, SourceNode};

/// Index affected formula ancestors once, retaining graph-local native identities.
/// The frontend already translates `inContext`; other scope coordinates may not
/// disappear when atom routing or owned subformulas erase their source wrappers.
pub(super) fn untranslated_formulas(source: &PreparedLogicSource) -> BTreeSet<SourceNode> {
    let mut parents = BTreeMap::<SourceNode, Vec<SourceNode>>::new();
    let mut affected = BTreeSet::new();
    for edge in source.source_graph().edges() {
        match edge.role {
            SourceEdgeRole::FormulaComponent => parents
                .entry(SourceNode {
                    term: edge.target,
                    graph: edge.source.graph,
                })
                .or_default()
                .push(edge.source),
            SourceEdgeRole::Standpoint | SourceEdgeRole::Provenance | SourceEdgeRole::Module => {
                affected.insert(edge.source);
            }
            SourceEdgeRole::Context
                if edge.predicate.iri(source.dataset())
                    != "https://blackcatinformatics.ca/logic/inContext" =>
            {
                affected.insert(edge.source);
            }
            _ => {}
        }
    }
    let mut pending = affected.iter().copied().collect::<Vec<_>>();
    while let Some(child) = pending.pop() {
        if let Some(ancestors) = parents.get(&child) {
            for &parent in ancestors {
                if affected.insert(parent) {
                    pending.push(parent);
                }
            }
        }
    }
    affected
}
