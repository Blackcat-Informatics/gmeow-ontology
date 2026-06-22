// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The `gts_sink` stage (#861 P4): the sole serialization exit — the gts narrow
//! waist.
//!
//! Exactly one Sink per pipeline. It unions every upstream RDF product (the
//! composed dataset + the reasoned closure + the documentation graph + every
//! export leaf's RDF) into one store and serializes it ONCE to `gmeow.gts` via
//! the pub `gmeow_rdf::gts_write::to_gts` over a `gmeow_rdf::oxigraph::OxigraphStore`.
//!
//! Scope note (honest): this wires the existing pub quad serializer. Folding the
//! non-RDF doc/slice BLOBS into the bundle (byte-for-byte fold-parity with the
//! committed `gmeow.gts`) needs the `compile_gts_native` compose core lifted out
//! of the `python`-gated `py_gts.rs` into a pyo3-free pub module — tracked as the
//! fold-parity step; not done here.

use std::collections::BTreeMap;

use gmeow_rdf::gts_write::to_gts;
use gmeow_rdf::oxigraph::OxigraphStore;
use oxigraph::io::{RdfFormat, RdfParser};
use oxigraph::store::Store;

use crate::error::PipelineError;
use crate::node::{Stage, StageInput, StageKind, StageOutput, StageProduct};

/// Committed logical path of the serialized GTS bundle.
pub const GTS_PATH: &str = "generated/dist/gmeow.gts";
/// The GTS profile the producer stamps (matches `compile_gts_native`).
const GTS_PROFILE: &str = "dist";

/// Union every RDF artifact across all upstream products into one store, then
/// serialize once to canonical GTS bytes.
pub fn serialize(upstream: &BTreeMap<String, StageProduct>) -> Result<Vec<u8>, PipelineError> {
    let store =
        Store::new().map_err(|e| PipelineError::Parse(format!("store creation failed: {e}")))?;
    for product in upstream.values() {
        for (path, bytes) in &product.artifacts {
            let format = if path.ends_with(".nq") {
                RdfFormat::NQuads
            } else if path.ends_with(".nt") {
                RdfFormat::NTriples
            } else if path.ends_with(".ttl") {
                RdfFormat::Turtle
            } else {
                continue; // non-RDF artifact (TSV/parquet/etc.) — folded as a blob (deferred)
            };
            for quad in RdfParser::from_format(format)
                .lenient()
                .for_reader(bytes.as_slice())
            {
                let quad =
                    quad.map_err(|e| PipelineError::Parse(format!("gts_sink {path}: {e}")))?;
                store
                    .insert(&quad)
                    .map_err(|e| PipelineError::Parse(format!("store insert failed: {e}")))?;
            }
        }
    }
    let view = OxigraphStore::new(&store);
    to_gts(&view, GTS_PROFILE).map_err(|e| PipelineError::Stage {
        stage: "stage-gts-sink".to_string(),
        message: format!("GTS serialization failed: {e}"),
    })
}

// ── Stage impl ───────────────────────────────────────────────────────────────

/// The `gts_sink` pipeline stage — the single serialization exit.
pub struct GtsSinkStage {
    consumes: Vec<String>,
}

impl GtsSinkStage {
    /// Construct the sink. It folds every upstream RDF product; the slice DAG's
    /// stage-gts-sink dataflowConsumes set is reconciled at P6 wiring.
    pub fn new() -> Self {
        Self {
            consumes: vec![
                "stage-docs-render".to_string(),
                "stage-gts-compose".to_string(),
                "stage-reason".to_string(),
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
        "gts_sink.v1-quads"
    }
    fn run(&self, input: StageInput<'_>) -> Result<StageOutput, PipelineError> {
        let gts = serialize(input.upstream)?;
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
    use crate::stages::source_load::{load_authored_store, store_to_nquads, BASE_GRAPH_PATH};
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
        // Feed the authored base graph as one upstream product; the sink must
        // serialize a GTS bundle that the kernel can read back.
        let root = repo_root();
        let nq = store_to_nquads(&load_authored_store(&root).unwrap()).unwrap();
        let mut upstream: BTreeMap<String, StageProduct> = BTreeMap::new();
        let mut a: BTreeMap<String, Vec<u8>> = BTreeMap::new();
        a.insert(BASE_GRAPH_PATH.to_string(), nq);
        upstream.insert(
            "stage-gts-compose".to_string(),
            StageProduct::from_artifacts("stage-gts-compose", a),
        );

        let gts = serialize(&upstream).expect("serialize");
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
