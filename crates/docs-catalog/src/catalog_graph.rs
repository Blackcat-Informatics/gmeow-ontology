// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The one way this crate gets at the meta-level distribution-catalog named graph.
//!
//! The catalog rides in [`GRAPH_DISTRIBUTION_CATALOG`], and that named graph survives
//! only through the STRUCTURAL gts reader ([`purrdf::gts::read_graph`]) — the flattened
//! dataset fold collapses named graphs away, so neither `gts_base_graph` nor
//! `flattened_dataset` can serve this read. Both readers in this crate therefore go
//! through [`catalog_triples`]: one structural read, one graph-name filter, one shared
//! failure posture.
//!
//! No-optionality: an absent or empty catalog graph is a HARD FAIL for either reader — a
//! shipped bundle MUST carry the catalog. What is NOT a failure is a catalog that carries
//! no concept nodes; see [`crate::concept_lattice`].

use gmeow_errors::Diag;

pub use gmeow_bundle_view::graph_iris::GRAPH_DISTRIBUTION_CATALOG;

/// The RDF `rdf:type` predicate — both readers select their subjects by type.
pub(crate) const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";

/// Every `(subject, predicate, object)` string triple the [`GRAPH_DISTRIBUTION_CATALOG`]
/// named graph of `gts_bytes` carries.
///
/// `err` mints the caller's own diagnostic kind, so the distribution-matrix reader and the
/// concept-lattice reader each report a missing catalog under their own code while sharing
/// exactly one implementation of the read.
///
/// # Errors
///
/// The `Diag` `err` builds when the snapshot will not fold structurally, or when it
/// carries no (or an empty) distribution-catalog named graph.
pub(crate) fn catalog_triples(
    gts_bytes: &[u8],
    err: &dyn Fn(String) -> Diag,
) -> Result<Vec<(String, String, String)>, Diag> {
    let graph = purrdf::gts::read_graph(gts_bytes, true)
        .map_err(|e| err(format!("gts read_graph failed: {e}")))?;
    let term = |id: usize| -> String {
        graph
            .terms
            .get(id)
            .and_then(|t| t.value.clone())
            .unwrap_or_default()
    };
    // Resolve the catalog graph-name to its term-ID ONCE, then filter quads by a cheap
    // `usize` id compare — never a per-quad string clone/compare.
    let catalog_gid = graph
        .terms
        .iter()
        .position(|t| t.value.as_deref() == Some(GRAPH_DISTRIBUTION_CATALOG));
    let catalog: Vec<(String, String, String)> = catalog_gid
        .map(|cgid| {
            graph
                .quads
                .iter()
                .filter_map(|&(s, p, o, gname)| {
                    let gid = gname?;
                    (gid == cgid).then(|| (term(s), term(p), term(o)))
                })
                .collect()
        })
        .unwrap_or_default();
    if catalog.is_empty() {
        return Err(err(format!(
            "the shipped bundle carries no <{GRAPH_DISTRIBUTION_CATALOG}> named graph — the \
             distribution catalog is missing; re-materialize the bundle with `make check`"
        )));
    }
    Ok(catalog)
}
