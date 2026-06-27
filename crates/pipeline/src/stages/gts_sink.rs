// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The `gts_sink` stage (#861 P4/P6): the sole serialization exit — the gts
//! narrow waist.
//!
//! Exactly one Sink per pipeline. The STRUCTURED multi-named-graph `dist`
//! snapshot is ASSEMBLED upstream by [`crate::stages::snapshot::SnapshotStage`]
//! (fold-isomorphic to the committed `generated/dist/gmeow.gts`, #861 P6 parity
//! gate). This sink consumes that one `stage-snapshot` product and re-emits its
//! `gmeow.gts` bytes as the sink artifact — the single, well-defined disk-write
//! the `run_full` orchestration performs. Splitting the assembly (a Transform)
//! from the serialization exit (this Sink) is what lets every fold-reading export
//! leaf consume THIS run's freshly-composed fold rather than the stale committed
//! file (the single-pass invariant).

use std::collections::BTreeMap;

use crate::error::PipelineError;
use crate::node::{Stage, StageInput, StageKind, StageOutput, StageProduct};
use crate::stages::snapshot::{snapshot_bytes, SNAPSHOT_PATH};

/// Committed logical path of the serialized GTS bundle.
pub const GTS_PATH: &str = SNAPSHOT_PATH;

// ── Stage impl ───────────────────────────────────────────────────────────────

/// The `gts_sink` pipeline stage — the single serialization exit.
pub struct GtsSinkStage {
    consumes: Vec<String>,
}

impl GtsSinkStage {
    /// Construct the sink. It consumes the single `stage-snapshot` product (the
    /// fully-assembled structured snapshot) and re-emits its `gmeow.gts` bytes.
    pub fn new() -> Self {
        Self {
            consumes: vec!["stage-snapshot".to_string()],
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
        "gts_sink.v3-snapshot"
    }
    fn run(&self, input: StageInput<'_>) -> Result<StageOutput, PipelineError> {
        let gts = snapshot_bytes(input.upstream)?;
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
    fn sink_re_emits_the_snapshot_bytes_verbatim() {
        // The sink now consumes the assembled `stage-snapshot` product and just
        // re-emits its `gmeow.gts` bytes. Build a snapshot product the way the
        // SnapshotStage would, run the sink over it, and assert byte-equality.
        let root = repo_root();
        let (_, rdf12) = crate::stages::statements::compile_statements(&root).unwrap();
        let docs = crate::stages::docs_render::render_docs_graph(&root).unwrap();

        let mut snap_upstream: BTreeMap<String, StageProduct> = BTreeMap::new();
        let mut st: BTreeMap<String, Vec<u8>> = BTreeMap::new();
        st.insert(
            crate::stages::statements::RDF12_PATH.to_string(),
            rdf12.into_bytes(),
        );
        snap_upstream.insert(
            "stage-statements".to_string(),
            StageProduct::from_artifacts("stage-statements", st),
        );
        let mut dc: BTreeMap<String, Vec<u8>> = BTreeMap::new();
        dc.insert(
            crate::stages::docs_render::DOCS_GRAPH_PATH.to_string(),
            docs.into_bytes(),
        );
        snap_upstream.insert(
            "stage-docs-render".to_string(),
            StageProduct::from_artifacts("stage-docs-render", dc),
        );
        let mut vd: BTreeMap<String, Vec<u8>> = BTreeMap::new();
        vd.insert(
            crate::stages::validate::SHACL_RDF_PATH.to_string(),
            Vec::new(),
        );
        snap_upstream.insert(
            "stage-validate".to_string(),
            StageProduct::from_artifacts("stage-validate", vd),
        );
        // The snapshot now folds the compiler's loss ledger + diagnostics; provide the
        // real stage-compile-logic product the SnapshotStage would consume.
        let compile = crate::stages::compile_logic::CompileLogicStage::new()
            .run(StageInput {
                root: &root,
                upstream: &BTreeMap::new(),
            })
            .expect("compile-logic stage");
        snap_upstream.insert("stage-compile-logic".to_string(), compile.product);
        // The snapshot folds the external-corpus divergence Findings; provide the
        // real stage-conformance product the SnapshotStage would consume.
        let conformance = crate::stages::conformance::ConformanceStage
            .run(StageInput {
                root: &root,
                upstream: &BTreeMap::new(),
            })
            .expect("conformance stage");
        snap_upstream.insert("stage-conformance".to_string(), conformance.product);

        let gts =
            crate::stages::snapshot::build_snapshot(&root, &snap_upstream, Vec::new(), Vec::new())
                .expect("build_snapshot");
        assert!(
            gts.len() > 1024,
            "GTS bundle implausibly small: {} bytes",
            gts.len()
        );

        // Hand the assembled snapshot to the sink as the `stage-snapshot` product.
        let mut snap_art: BTreeMap<String, Vec<u8>> = BTreeMap::new();
        snap_art.insert(GTS_PATH.to_string(), gts.clone());
        let mut upstream: BTreeMap<String, StageProduct> = BTreeMap::new();
        upstream.insert(
            "stage-snapshot".to_string(),
            StageProduct::from_artifacts("stage-snapshot", snap_art),
        );
        let out = GtsSinkStage::new()
            .run(StageInput {
                root: &root,
                upstream: &upstream,
            })
            .expect("sink runs");
        let emitted = out
            .product
            .artifact(GTS_PATH)
            .expect("sink emits gmeow.gts");
        assert_eq!(emitted, gts.as_slice(), "sink must re-emit verbatim");

        // Round-trips through the kernel GTS importer (the bundle is well-formed).
        let _ = gmeow_rdf::import_gts_events(emitted).expect("import_gts_events");
    }
}
