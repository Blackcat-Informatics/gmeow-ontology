// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Admission for the existing single-context native DL entailment operation.
//! This checks context selection, not complete CompiledTheory law coverage.

use gmeow_logic_compile::frontend::{SourceEdgeRole, StructuralSourceGraph};
use purrdf::{QuadIds, RdfDataset, TermRef};

use super::{EntailmentGap, GapShape};

/// The profile implemented by `dl_entails`: an unspecified, world-local default
/// assertion context. It never selects a union of named or annotated contexts.
const PROFILE: &str = "native-dl-default-context-v1";
const CONFIDENCE: &str = "https://blackcatinformatics.ca/logic/confidence";

/// Borrow the original dataset only after admitting its context to this profile.
/// In particular, the reduction cannot be called with an unchecked raw dataset.
pub(super) struct AdmittedDefaultGraph<'a> {
    dataset: &'a RdfDataset,
}

impl<'a> AdmittedDefaultGraph<'a> {
    pub(super) fn new(dataset: &'a RdfDataset, side: &str) -> Result<Self, EntailmentGap> {
        if let Some(graph) = dataset.named_graphs().next() {
            return Err(coverage_gap(format!(
                "{side} declares named context {:?}; the selected operation has no named-context \
                 selection or cross-context bridge, so it cannot erase this graph identity",
                dataset.to_owned_term(graph),
            )));
        }
        for (carrier, quad) in dataset
            .quads()
            .map(|quad| ("statement", quad))
            .chain(dataset.annotation_quads().map(|quad| ("annotation", quad)))
        {
            let TermRef::Iri(predicate) = dataset.resolve(quad.p) else {
                unreachable!("a frozen RDF dataset has IRI predicates")
            };
            let role = StructuralSourceGraph::predicate_role(predicate);
            let selects_context = match role {
                Some(SourceEdgeRole::Standpoint | SourceEdgeRole::Module) => true,
                // Confidence remains evidence, just as in native program admission;
                // it neither selects a world nor licenses another truth algebra.
                Some(SourceEdgeRole::Context) => predicate != CONFIDENCE,
                _ => false,
            };
            if selects_context {
                return Err(coverage_gap(format!(
                    "{side} {carrier} has an unexecuted context coordinate: {:?} <{predicate}> {:?} \
                     in the default graph; its owner and scope cannot become unconditional",
                    dataset.to_owned_term(quad.s),
                    dataset.to_owned_term(quad.o),
                )));
            }
        }
        Ok(Self { dataset })
    }

    pub(super) fn dataset(&self) -> &'a RdfDataset {
        self.dataset
    }
}

/// Every asserted native row, retaining graph and scoped term identities. A
/// reifier binding is a statement about a quotation, never its quoted assertion.
pub(super) fn assertions(dataset: &RdfDataset) -> impl Iterator<Item = QuadIds> + '_ {
    dataset
        .quads()
        .chain(dataset.reifier_quads())
        .chain(dataset.annotation_quads())
}

fn coverage_gap(detail: String) -> EntailmentGap {
    EntailmentGap {
        shape: GapShape::NativeCoverage,
        detail: format!("{PROFILE}: {detail}"),
    }
}

#[cfg(test)]
mod tests;
