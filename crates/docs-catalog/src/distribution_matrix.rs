// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The consumer-side per-format distribution matrix (`gmeow docs matrix`, the MCP
//! `distribution_matrix` tool).
//!
//! [`read_distribution_matrix`] is the production dogfooding consumer of the distribution
//! catalog's own ontology content: it QUERIES the meta-level catalog graph shipped inside
//! a materialized `gmeow.gts` rather than re-authoring a static table anywhere. The
//! `gmeow:DocumentationDistribution` type filter is the row selector and is deliberately
//! narrow — the same graph also carries family, capability, loss, site-sub-asset, and
//! (later) formal-concept nodes, and none of those is a distribution.

use std::collections::BTreeSet;

use gmeow_errors::Diag;
use gmeow_ns::GMEOW_NS;

use crate::catalog_graph::{RDF_TYPE, catalog_triples};
use crate::error::DistributionCatalog;
use crate::identity::{iri, local_name};

fn err(message: impl Into<String>) -> Diag {
    Diag::of_kind(DistributionCatalog {
        message: message.into(),
    })
}

/// One resolved row of the per-format consumer-need matrix: a single
/// `gmeow:DocumentationDistribution` from the distribution catalog, as read back by
/// [`read_distribution_matrix`] — the production dogfooding consumer of the catalog's
/// ontology content (`gmeow docs matrix`), never a re-authored static table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DistributionRow {
    pub slug: String,
    pub family: String,
    pub media_type: String,
    /// Sorted, deduped consumer local names (`gmeow:eligibleForConsumer`).
    pub consumers: Vec<String>,
    /// Sorted, deduped dropped-capability slugs
    /// (`gmeow:declaredLoss`/`gmeow:accountsForParameter`) — empty for the
    /// serialization family, which declares no loss.
    pub dropped_capabilities: Vec<String>,
}

/// Resolve the per-format consumer-need matrix by QUERYING the meta-level
/// [`GRAPH_DISTRIBUTION_CATALOG`](crate::catalog_graph::GRAPH_DISTRIBUTION_CATALOG) named
/// graph shipped inside `gts_bytes`, dogfooding the distribution catalog content rather
/// than re-deriving it.
///
/// The named graph survives only through the STRUCTURAL reader — see
/// [`crate::catalog_graph`], which owns the read both catalog readers share.
///
/// No-optionality: an absent or empty catalog graph, or a catalog subject missing a
/// required facet, is a HARD FAIL — the shipped bundle MUST carry a complete distribution
/// catalog, never a silently partial matrix.
///
/// # Errors
///
/// [`DistributionCatalog`] when the snapshot will not fold, when it carries no
/// distribution-catalog named graph, when that graph declares no
/// `gmeow:DocumentationDistribution` subject, or when a declared distribution is missing
/// its format / family / media type / consumer facet (or a declared loss node is missing
/// its accounted parameter).
pub fn read_distribution_matrix(gts_bytes: &[u8]) -> Result<Vec<DistributionRow>, Diag> {
    let catalog = catalog_triples(gts_bytes, &err)?;

    let pred_dist_type = iri(GMEOW_NS, "DocumentationDistribution");
    let pred_format = iri(GMEOW_NS, "distributionFormat");
    let pred_family = iri(GMEOW_NS, "distributionFamily");
    let pred_media = iri(GMEOW_NS, "artifactMediaType");
    let pred_consumer = iri(GMEOW_NS, "eligibleForConsumer");
    let pred_loss = iri(GMEOW_NS, "declaredLoss");
    let pred_accounts = iri(GMEOW_NS, "accountsForParameter");

    let subjects: BTreeSet<&str> = catalog
        .iter()
        .filter(|(_, p, o)| *p == RDF_TYPE && *o == pred_dist_type)
        .map(|(s, _, _)| s.as_str())
        .collect();
    if subjects.is_empty() {
        return Err(err(format!(
            "the <{}> named graph carries no gmeow:DocumentationDistribution subject",
            crate::catalog_graph::GRAPH_DISTRIBUTION_CATALOG
        )));
    }

    let mut rows = Vec::with_capacity(subjects.len());
    for subject in subjects {
        let slug = catalog
            .iter()
            .find(|(s, p, _)| s == subject && *p == pred_format)
            .map(|(_, _, o)| o.clone())
            .ok_or_else(|| {
                err(format!(
                    "distribution {subject} is missing gmeow:distributionFormat"
                ))
            })?;
        let family = catalog
            .iter()
            .find(|(s, p, _)| s == subject && *p == pred_family)
            .map(|(_, _, o)| local_name(o))
            .ok_or_else(|| {
                err(format!(
                    "distribution {subject} is missing gmeow:distributionFamily"
                ))
            })?;
        let media_type = catalog
            .iter()
            .find(|(s, p, _)| s == subject && *p == pred_media)
            .map(|(_, _, o)| o.clone())
            .ok_or_else(|| {
                err(format!(
                    "distribution {subject} is missing gmeow:artifactMediaType"
                ))
            })?;

        let mut consumers: Vec<String> = catalog
            .iter()
            .filter(|(s, p, _)| s == subject && *p == pred_consumer)
            .map(|(_, _, o)| local_name(o))
            .collect();
        if consumers.is_empty() {
            return Err(err(format!(
                "distribution {subject} is missing gmeow:eligibleForConsumer"
            )));
        }
        consumers.sort();
        consumers.dedup();

        let loss_nodes: Vec<&str> = catalog
            .iter()
            .filter(|(s, p, _)| s == subject && *p == pred_loss)
            .map(|(_, _, o)| o.as_str())
            .collect();
        let mut dropped_capabilities: Vec<String> = Vec::with_capacity(loss_nodes.len());
        for loss_node in loss_nodes {
            let cap = catalog
                .iter()
                .find(|(s, p, _)| s == loss_node && *p == pred_accounts)
                .map(|(_, _, o)| local_name(o))
                .ok_or_else(|| {
                    err(format!(
                        "loss node {loss_node} is missing gmeow:accountsForParameter"
                    ))
                })?;
            dropped_capabilities.push(cap);
        }
        dropped_capabilities.sort();
        dropped_capabilities.dedup();

        rows.push(DistributionRow {
            slug,
            family,
            media_type,
            consumers,
            dropped_capabilities,
        });
    }
    rows.sort_by(|a, b| a.slug.cmp(&b.slug));
    Ok(rows)
}
