// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The validation-only substrate role, borrowed before one native import.

use std::sync::Arc;

use purrdf::RdfStoreCapabilities;
use purrdf::ir::import::DatasetImporter;
use purrdf::{
    DatasetView, GraphMatch, QuadIds, QuadRef, RdfDataset, RdfDatasetBuilder, TermId, TermRef,
    TermValue,
};

use super::SUBSTRATE_IRI_PREFIX;
use crate::stages::provenance_graph::GRAPH_PROVENANCE;

/// This fixed role admits only substrate subjects from the provenance graph and
/// places their statements in the validation default graph. It is not a general
/// graph-flattening rule or a certificate for projecting other logical contexts.
struct SubstrateView<'a> {
    source: &'a RdfDataset,
    graph: Option<TermId>,
}

impl SubstrateView<'_> {
    fn selected<'a>(
        &'a self,
        rows: impl Iterator<Item = QuadIds> + 'a,
    ) -> impl Iterator<Item = QuadIds> + 'a {
        rows.filter(|quad| self.graph.is_some() && quad.g == self.graph
            && matches!(self.source.resolve(quad.s), TermRef::Iri(iri) if iri.starts_with(SUBSTRATE_IRI_PREFIX)))
            .map(|quad| QuadIds { g: None, ..quad })
    }
}

impl DatasetView for SubstrateView<'_> {
    type Id = TermId;
    type ProbePlan = <RdfDataset as DatasetView>::ProbePlan;

    fn quads(&self) -> impl Iterator<Item = QuadIds> + '_ {
        self.selected(self.graph.into_iter().flat_map(|graph| {
            self.source
                .quads_for_pattern(None, None, None, GraphMatch::Named(graph))
        }))
    }

    fn quad_refs(&self) -> impl Iterator<Item = QuadRef<'_>> + '_ {
        self.quads().map(|quad| QuadRef {
            s: self.source.resolve(quad.s),
            p: self.source.resolve(quad.p),
            o: self.source.resolve(quad.o),
            g: None,
        })
    }

    fn resolve(&self, id: TermId) -> TermRef<'_> {
        self.source.resolve(id)
    }

    fn quads_for_pattern(
        &self,
        s: Option<TermId>,
        p: Option<TermId>,
        o: Option<TermId>,
        g: GraphMatch,
    ) -> impl Iterator<Item = QuadIds> + '_ {
        self.selected(
            self.graph
                .filter(|_| g.matches(None))
                .into_iter()
                .flat_map(move |graph| {
                    self.source
                        .quads_for_pattern(s, p, o, GraphMatch::Named(graph))
                }),
        )
    }

    fn probe_plan(&self, s: bool, p: bool, o: bool, _g: GraphMatch) -> Self::ProbePlan {
        self.source.probe_plan(
            s,
            p,
            o,
            self.graph.map_or(GraphMatch::Default, GraphMatch::Named),
        )
    }

    fn quads_for_pattern_with_plan(
        &self,
        plan: &Self::ProbePlan,
        s: Option<TermId>,
        p: Option<TermId>,
        o: Option<TermId>,
        g: GraphMatch,
    ) -> impl Iterator<Item = QuadIds> + '_ {
        let plan = *plan;
        self.selected(
            self.graph
                .filter(|_| g.matches(None))
                .into_iter()
                .flat_map(move |graph| {
                    self.source.quads_for_pattern_with_plan(
                        &plan,
                        s,
                        p,
                        o,
                        GraphMatch::Named(graph),
                    )
                }),
        )
    }

    fn term_id_by_value(&self, value: &TermValue) -> Option<TermId> {
        self.source.term_id_by_value(value)
    }

    fn capabilities(&self) -> RdfStoreCapabilities {
        self.source.capabilities()
    }

    fn term_count(&self) -> usize {
        self.source.term_count()
    }

    fn reifier_quads(&self) -> impl Iterator<Item = QuadIds> + '_ {
        self.selected(self.source.reifier_quads())
    }

    fn annotation_quads(&self) -> impl Iterator<Item = QuadIds> + '_ {
        self.selected(self.source.annotation_quads())
    }

    fn named_graphs(&self) -> impl Iterator<Item = TermId> + '_ {
        std::iter::empty()
    }
}

pub(super) fn project(source: &RdfDataset) -> gmeow_errors::Result<Arc<RdfDataset>> {
    let view = SubstrateView {
        source,
        graph: source.term_id_by_iri(GRAPH_PROVENANCE),
    };
    let mut builder = RdfDatasetBuilder::new();
    DatasetImporter::new(&mut builder, &view).append();
    builder.freeze().map_err(|error| {
        gmeow_errors::Diag::of_kind(crate::error::Parse {
            message: format!("native substrate A-Box projection: {error}"),
        })
    })
}

#[path = "substrate.tests.rs"]
#[cfg(test)]
mod tests;
