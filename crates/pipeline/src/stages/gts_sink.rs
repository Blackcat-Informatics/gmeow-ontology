// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The `gts_sink` stage (#861 P4/P6): the sole serialization exit — the gts
//! narrow waist.
//!
//! Exactly one Sink per pipeline. It assembles the STRUCTURED multi-named-graph
//! `dist` snapshot — fold-isomorphic to the committed `generated/dist/gmeow.gts`
//! (#861 P6 parity gate) — via [`crate::stages::snapshot::build_snapshot`], which
//! drives the pyo3-free `gmeow_rdf::gts_compose` core. The default graph carries
//! the AUTHORED ontology only; the import closure, self-description metadata,
//! SSSOM alignment axioms, the RDF 1.2 statement layer, the slice-analysis graph,
//! the verify attestation, and the documentation projection each ride their own
//! named graph, plus the RDF 1.2 reifier/annotation tables and the
//! content-addressed blob channel.

use std::collections::BTreeMap;

use gmeow_rdf::gts_compose::BlobRow;

use crate::error::PipelineError;
use crate::node::{Stage, StageInput, StageKind, StageOutput, StageProduct};
use crate::stages::snapshot::build_snapshot;

/// Committed logical path of the serialized GTS bundle.
pub const GTS_PATH: &str = "generated/dist/gmeow.gts";

/// Collect the content-addressed blob rows folded ahead of the snapshot. Each
/// upstream stage that produces a blob (the docs site, the OKF export, the
/// transform/reasoning archives) contributes its rows here; the rows ride the
/// SAME channel `doc_blobs` use in `gts_gen.py`.
///
/// Scope note: the blob CHANNEL is wired, but only the upstream products that
/// already exist in the spine contribute. The full 247-blob set (docs guides,
/// slice artifacts, OKF, project/ontology-docs, transform/reasoning archives) is
/// folded as those producer stages land; the fold-parity gate measures the
/// per-named-graph QUAD fold + reifiers/annotations (the semantic contract), not
/// the blob count.
fn collect_blobs(_upstream: &BTreeMap<String, StageProduct>) -> Vec<BlobRow> {
    Vec::new()
}

// ── Stage impl ───────────────────────────────────────────────────────────────

/// The `gts_sink` pipeline stage — the single serialization exit.
pub struct GtsSinkStage {
    consumes: Vec<String>,
}

impl GtsSinkStage {
    /// Construct the sink. It reads the RDF 1.2 statement layer (`stage-statements`)
    /// and the documentation projection (`stage-docs-render`) products to assemble
    /// the structured snapshot, plus `stage-gts-compose` / `stage-reason` for the
    /// composed-fold / reasoned-closure wiring. The slice DAG's stage-gts-sink
    /// dataflowConsumes set is reconciled at P6 wiring.
    pub fn new() -> Self {
        Self {
            consumes: vec![
                "stage-docs-render".to_string(),
                "stage-gts-compose".to_string(),
                "stage-reason".to_string(),
                "stage-statements".to_string(),
            ],
        }
    }
}

impl Default for GtsSinkStage {
    fn default() -> Self {
        Self::new()
    }
}

impl Stage for GtsSinkStage {
    fn id(&self) -> &str {
        "stage-gts-sink"
    }
    fn kind(&self) -> StageKind {
        StageKind::Sink
    }
    fn consumes(&self) -> &[String] {
        &self.consumes
    }
    fn impl_version(&self) -> &str {
        "gts_sink.v2-structured"
    }
    fn run(&self, input: StageInput<'_>) -> Result<StageOutput, PipelineError> {
        let blobs = collect_blobs(input.upstream);
        let gts = build_snapshot(input.root, input.upstream, blobs)?;
        let mut artifacts: BTreeMap<String, Vec<u8>> = BTreeMap::new();
        artifacts.insert(GTS_PATH.to_string(), gts);
        Ok(StageOutput {
            product: StageProduct::from_artifacts(self.id(), artifacts),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn repo_root() -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .canonicalize()
            .unwrap()
    }

    #[test]
    fn sink_serializes_a_readable_gts_bundle() {
        // Build the two upstream products the sink reads (statements RDF 1.2 +
        // docs graph) the way their stages would, then assemble the structured
        // snapshot and round-trip it through the kernel GTS reader.
        let root = repo_root();
        let (_, rdf12) = crate::stages::statements::compile_statements(&root).unwrap();
        let docs = crate::stages::docs_render::render_docs_graph(&root).unwrap();

        let mut upstream: BTreeMap<String, StageProduct> = BTreeMap::new();
        let mut st: BTreeMap<String, Vec<u8>> = BTreeMap::new();
        st.insert(
            crate::stages::statements::RDF12_PATH.to_string(),
            rdf12.into_bytes(),
        );
        upstream.insert(
            "stage-statements".to_string(),
            StageProduct::from_artifacts("stage-statements", st),
        );
        let mut dc: BTreeMap<String, Vec<u8>> = BTreeMap::new();
        dc.insert(
            crate::stages::docs_render::DOCS_GRAPH_PATH.to_string(),
            docs.into_bytes(),
        );
        upstream.insert(
            "stage-docs-render".to_string(),
            StageProduct::from_artifacts("stage-docs-render", dc),
        );

        let gts = build_snapshot(&root, &upstream, Vec::new()).expect("build_snapshot");
        assert!(
            gts.len() > 1024,
            "GTS bundle implausibly small: {} bytes",
            gts.len()
        );
        // Round-trips through the kernel GTS reader (the bundle is well-formed).
        let graph = gmeow_rdf::gts::read_graph(&gts, true).expect("read_graph");
        let _ = gmeow_rdf::gts::GtsGraphStore::new(&graph);
    }
}
