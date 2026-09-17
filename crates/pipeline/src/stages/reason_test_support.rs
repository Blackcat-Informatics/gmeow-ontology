// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Test-only helpers for the owning production module.

use super::*;

/// Synthetic in-crate fixtures explicitly interpret their submitted contexts as
/// tiny theories. They use the same prepared execution/artifact path as production;
/// this test-only admission never selects a production carrier's graph roles.
#[cfg(test)]
pub(crate) fn reason_test_dataset(edb: &RdfDataset) -> Result<ReasonArtifacts, gmeow_errors::Diag> {
    let canon = canonicalize_edb(edb, "stage-reason")?;
    let input = prepare_reasoning_input(canon.as_ref())?;
    const AUTHORITY: &str = "gmeow.pipeline.synthetic-theories.v1";
    let roles: Vec<_> = input
        .source_contexts()
        .iter()
        .filter(|(world, graph)| {
            graph.is_some()
                || input.source_contexts().len() == 1
                || input
                    .source_assertion_count(world)
                    .is_some_and(|count| count > 0)
        })
        .map(|(_, graph)| LogicalGraph::from_graph(graph.clone()))
        .collect();
    let bytes = serde_json::to_vec(&(AUTHORITY, DomainProfile::NonemptyObjectDomainV1, &roles))
        .map_err(|error| stage_failure(format!("synthetic domain selection identity: {error}")))?;
    let selection = *blake3::hash(&bytes).as_bytes();
    let domains = SelectedDomains::new(
        roles
            .into_iter()
            .map(|graph| {
                SelectedLogicalWorld::new(
                    graph,
                    DomainProfile::NonemptyObjectDomainV1,
                    AUTHORITY.to_owned(),
                    selection,
                )
            })
            .collect::<gmeow_errors::Result<Vec<_>>>()?,
    )?;
    reason_prepared_input(edb, input, &domains)
}

#[cfg(test)]
pub(crate) fn reason_artifacts(
    composed_nquads: &[u8],
) -> Result<ReasonArtifacts, gmeow_errors::Diag> {
    let edb = purrdf::parse_dataset(composed_nquads, "application/n-quads", None)
        .map_err(|error| stage_failure(format!("synthetic reason input parse: {error}")))?;
    reason_test_dataset(edb.as_ref())
}

#[cfg(test)]
pub(crate) fn reason_product(composed_nquads: &[u8]) -> Result<StageProduct, gmeow_errors::Diag> {
    reason_product_from_artifacts(reason_artifacts(composed_nquads)?)
}
