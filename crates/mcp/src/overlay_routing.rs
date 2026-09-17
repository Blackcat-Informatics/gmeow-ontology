// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Complete RDF placement for the two read-only external-overlay operations.

use std::sync::Arc;

use purrdf::{BlankScope, RdfDataset, RdfDatasetBuilder, RdfTerm};

/// Select the caller's explicit canon scope, then place every overlay RDF record
/// in both the default graph and the external origin graph. The overlay's source
/// graph roles are deliberately flattened; their IRIs used as provenance values
/// remain ordinary terms. Empty input still declares the selected origin graph.
///
/// One fresh scope binds both overlay copies, including quoted and collection
/// terms, independently of the canon. Ordinary row locations survive. Neither
/// input is mutated, and no intermediate routed dataset is materialized.
///
/// # Errors
/// Rejects exhausted blank scopes or an invalid assembled native dataset.
pub(super) fn transient_union(
    canon: Option<&RdfDataset>,
    overlay: &RdfDataset,
) -> gmeow_errors::Result<Arc<RdfDataset>> {
    let mut builder = RdfDatasetBuilder::new();
    if let Some(canon) = canon {
        builder.push_dataset(canon);
    }
    let scope = builder
        .blank_identities()
        .map(|(_, scope)| scope.0)
        .max()
        .unwrap_or(0)
        .checked_add(1)
        .map(BlankScope)
        .ok_or_else(|| {
            gmeow_errors::Diag::of_kind(crate::error::Mcp {
                message: "no blank scope remains for the external overlay".to_owned(),
            })
        })?;
    let external = RdfTerm::iri(super::EXTERNAL_OVERLAY_GRAPH);
    let external_id = builder.intern_owned_term(&external);
    builder.declare_named_graph(external_id);
    for mut quad in overlay.owned_quads() {
        quad.graph_name = None;
        builder.push_owned_quad_scoped(&quad, scope);
        quad.graph_name = Some(external.clone());
        builder.push_owned_quad_scoped(&quad, scope);
    }
    for mut reifier in overlay.owned_reifiers() {
        reifier.graph = None;
        builder.push_owned_reifier_scoped(&reifier, scope);
        reifier.graph = Some(external.clone());
        builder.push_owned_reifier_scoped(&reifier, scope);
    }
    for mut annotation in overlay.owned_annotations() {
        annotation.graph = None;
        builder.push_owned_annotation_scoped(&annotation, scope);
        annotation.graph = Some(external.clone());
        builder.push_owned_annotation_scoped(&annotation, scope);
    }
    Ok(builder.freeze()?)
}

/// Bound the complete starting RDF relation before duplicating it or reasoning.
///
/// # Errors
/// Refuses an overlay whose ordinary and statement-layer rows exceed `limit`.
#[cfg(feature = "reasoning")]
pub(super) fn verify_record_limit(overlay: &RdfDataset, limit: usize) -> gmeow_errors::Result<()> {
    let records = overlay.rdf_row_count();
    if records > limit {
        return Err(gmeow_errors::Diag::of_kind(crate::error::Mcp {
            message: format!(
                "verify_graph: overlay carries {records} RDF records (quads, reifiers and annotations), \
                 exceeding the {limit} quad ceiling; split the annex \
                 and verify the parts (no silent truncation)"
            ),
        }));
    }
    Ok(())
}

#[cfg(test)]
mod tests;
