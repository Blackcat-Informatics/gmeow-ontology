// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The `gts_sink` stage (#861 P4/P6): the sole serialization exit — the gts
//! narrow waist.
//!
//! Exactly one Sink per pipeline. The STRUCTURED multi-named-graph `dist`
//! snapshot is ASSEMBLED upstream by [`crate::stages::carrier::SnapshotStage`]
//! (fold-isomorphic to the committed `generated/dist/gmeow.gts`, #861 P6 parity
//! gate). This sink consumes that one `stage-snapshot` product and re-emits its
//! `gmeow.gts` bytes as the sink artifact — the single, well-defined disk-write
//! the `run_full` orchestration performs. Splitting the assembly (a Transform)
//! from the serialization exit (this Sink) is what lets every fold-reading export
//! leaf consume THIS run's freshly-composed fold rather than the stale committed
//! file (the single-pass invariant).

use std::collections::BTreeMap;

use crate::error::PipelineError;
use crate::node::{Stage, StageInput, StageOutput, StageProduct, SINK_CAPABILITY};
use crate::stages::carrier::SNAPSHOT_PATH;

/// Committed logical path of the serialized GTS bundle.
pub const GTS_PATH: &str = SNAPSHOT_PATH;

// ── Stage impl ───────────────────────────────────────────────────────────────

/// The `gts_sink` pipeline stage — the single serialization exit.
pub struct GtsSinkStage {
    consumes: Vec<String>,
    capabilities: Vec<String>,
}

impl GtsSinkStage {
    /// Construct the sink. It consumes the assembled carrier (`stage-snapshot`) and the
    /// by-reference blob sources it folds into the terminal `gmeow.gts` package
    /// (#1132 Stage C): the in-memory JSON-Schema/axiom/reasoning/SHACL-report products.
    /// It holds [`SINK_CAPABILITY`] — the sole serialization exit the loader requires
    /// exactly one stage to hold (mirrored by the slice
    /// `gmeow:stage-gts-sink gmeow:hasCapability gmeow:sinkCapability`).
    pub fn new() -> Self {
        Self {
            consumes: vec![
                "stage-snapshot".to_string(),
                "stage-export-json-schema".to_string(),
                "stage-compile-logic".to_string(),
                "stage-reason".to_string(),
                "stage-validate".to_string(),
            ],
            capabilities: vec![SINK_CAPABILITY.to_string()],
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
    fn consumes(&self) -> &[String] {
        &self.consumes
    }
    fn capabilities(&self) -> &[String] {
        &self.capabilities
    }
    fn impl_version(&self) -> &str {
        "gts_sink.v3-snapshot"
    }
    fn run(&self, input: StageInput<'_>) -> Result<StageOutput, PipelineError> {
        // The terminal gts ARCHIVE writer (#1132 Stage C): serialize THIS run's carrier
        // into the single `gmeow.gts` package. GTS is exit-only — produced HERE and
        // nowhere else; every internal export leaf reads the carrier dataset off the
        // snapshot product's bundle, never these bytes. The carrier is taken off the
        // bundle (no re-assembly — the razor: transform transport→form once), and the
        // by-reference blob archives are folded in alongside it.
        let carrier = crate::stages::carrier::snapshot_dataset(input.upstream)?;
        let gts = crate::stages::carrier::serialize_carrier_snapshot(
            input.root,
            input.upstream,
            carrier.as_ref(),
        )?;
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
    fn sink_serializes_the_snapshot_carrier_with_blob_inputs() {
        // Build the minimal upstream product set the sink requires: a snapshot
        // carrier plus by-reference blob sources. Full real-DAG coverage lives in
        // the end-to-end pipeline test; this unit test pins the sink's fail-closed
        // artifact wiring without paying for reasoning and snapshot assembly.
        let root = repo_root();
        let carrier = gmeow_rdf::parse_dataset(
            b"<https://blackcatinformatics.ca/gmeow> <http://purl.org/dc/terms/title> \"GMEOW\" .\n\
              <https://blackcatinformatics.ca/gmeow> <http://www.w3.org/2002/07/owl#versionInfo> \"test\" .\n\
              <https://example.org/s> <https://example.org/p> <https://example.org/o> .\n",
            "application/n-triples",
            None,
        )
        .expect("minimal carrier dataset");
        let snapshot =
            StageProduct::from_artifacts_over("stage-snapshot", carrier, BTreeMap::new());

        let mut compile_artifacts: BTreeMap<String, Vec<u8>> = BTreeMap::new();
        for path in [
            "generated/owl/gmeow-dl.ttl",
            "generated/owl/gmeow-el.ttl",
            "generated/logic/gmeow.logic.rdf12.ttl",
            "generated/logic/gmeow.rls",
            "generated/datalog/gmeow.dl",
        ] {
            compile_artifacts.insert(path.to_string(), Vec::new());
        }
        let compile = StageProduct::from_artifacts("stage-compile-logic", compile_artifacts);

        let mut json_artifacts: BTreeMap<String, Vec<u8>> = BTreeMap::new();
        json_artifacts.insert(
            crate::stages::json_schema::JSON_SCHEMA_PATH.to_string(),
            b"{}".to_vec(),
        );
        json_artifacts.insert(
            crate::stages::json_schema::OPENAPI_PATH.to_string(),
            b"{}".to_vec(),
        );
        let json_schema = StageProduct::from_artifacts("stage-export-json-schema", json_artifacts);

        let mut reason_artifacts: BTreeMap<String, Vec<u8>> = BTreeMap::new();
        reason_artifacts.insert(
            crate::stages::reason::EXPLANATIONS_PATH.to_string(),
            b"# explanations".to_vec(),
        );
        reason_artifacts.insert(
            crate::stages::reason::LEDGER_PATH.to_string(),
            b"# ledger".to_vec(),
        );
        let reason = StageProduct::from_artifacts("stage-reason", reason_artifacts);

        let mut validate_artifacts: BTreeMap<String, Vec<u8>> = BTreeMap::new();
        validate_artifacts.insert(
            crate::stages::validate::SHACL_JSON_PATH.to_string(),
            b"{}".to_vec(),
        );
        validate_artifacts.insert(
            crate::stages::validate::SHACL_SARIF_PATH.to_string(),
            b"{}".to_vec(),
        );
        let validate = StageProduct::from_artifacts("stage-validate", validate_artifacts);

        let mut upstream: BTreeMap<String, StageProduct> = BTreeMap::new();
        upstream.insert("stage-compile-logic".to_string(), compile);
        upstream.insert("stage-export-json-schema".to_string(), json_schema);
        upstream.insert("stage-reason".to_string(), reason);
        upstream.insert("stage-snapshot".to_string(), snapshot);
        upstream.insert("stage-validate".to_string(), validate);
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
        assert!(
            emitted.len() > 1024,
            "GTS bundle implausibly small: {} bytes",
            emitted.len()
        );

        // Round-trips through the kernel GTS importer (the bundle is well-formed).
        let _ = gmeow_rdf::import_gts_events(emitted).expect("import_gts_events");
    }
}
