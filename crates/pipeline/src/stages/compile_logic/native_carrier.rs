// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Native placement of the compiler's already-produced RDF projections.
//!
//! The three canonical projections replace every source graph with their selected
//! carrier graph, including statement-layer rows and declaration-only sources.
//! Diagnostics retain their existing graph placement. Independent input scopes
//! remain distinct; the final dataset is the sole row materialization here.
//! A former graph IRI used as a provenance or reference value remains that RDF
//! value; replacement applies to graph ownership and declarations only.
//!
//! These producer inputs represent source locations in their RDF/report payloads,
//! not as physical row attachments. PurRDF's view importer does not carry those
//! attachments, so a changed producer violating this contract fails admission.

use std::sync::Arc;

use purrdf::{
    CompositeDatasetView, CompositeSource, GraphPlacement, RdfDataset, TermValue, ViewLimits,
};

use super::{CARRIER_GRAPHS, stage_err};
use crate::stages::carrier::GRAPH_DIAGNOSTICS;

/// Place the existing compiler projections and diagnostics in one native carrier.
/// Inputs must carry source locations in RDF/report values, without row attachments.
///
/// # Errors
/// Rejects attached row locations, exhausted composite retention/scope capacity,
/// or a failed final materialization before publishing a dataset.
pub(super) fn assemble(
    projections: [Arc<RdfDataset>; 3],
    diagnostics: Arc<RdfDataset>,
) -> gmeow_errors::Result<Arc<RdfDataset>> {
    compose(projections, diagnostics)?
        .materialize()
        .map_err(|error| stage_err(format!("materialize native compile carrier: {error}")))
}

fn compose(
    projections: [Arc<RdfDataset>; 3],
    diagnostics: Arc<RdfDataset>,
) -> gmeow_errors::Result<CompositeDatasetView> {
    let mut sources = Vec::with_capacity(4);
    for (source, graph) in projections.into_iter().zip(CARRIER_GRAPHS) {
        admit_locations(&source, graph)?;
        sources.push(
            CompositeSource::new(source)
                .with_graph_placement(GraphPlacement::Named(TermValue::Iri(graph.to_owned()))),
        );
    }
    admit_locations(&diagnostics, GRAPH_DIAGNOSTICS)?;
    sources.push(CompositeSource::new(diagnostics));
    CompositeDatasetView::from_sources(
        sources,
        ViewLimits {
            // Four retained inputs plus PurRDF's small destination-name dictionary.
            max_sources: 5,
            ..ViewLimits::default()
        },
    )
    .map_err(|error| stage_err(format!("admit native compile carrier: {error}")))
}

fn admit_locations(source: &RdfDataset, graph: &str) -> gmeow_errors::Result<()> {
    // The frozen capability flag is O(1); no row scan or owned term conversion.
    if source.capabilities().source_locations {
        return Err(stage_err(format!(
            "compile carrier input <{graph}> has physical row locations; \
             this producer contract requires locations in its RDF and native report payloads"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests;
