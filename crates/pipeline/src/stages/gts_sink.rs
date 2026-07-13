// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The `gts_sink` stage: the sole serialization exit — the gts
//! narrow waist.
//!
//! Exactly one Sink per pipeline. The STRUCTURED multi-named-graph `dist`
//! snapshot is ASSEMBLED upstream by [`crate::stages::carrier::SnapshotStage`]
//! (fold-isomorphic to the committed `generated/dist/gmeow.gts`, the parity
//! gate). This sink consumes that one `stage-snapshot` product and re-emits its
//! `gmeow.gts` bytes as the sink artifact — the single, well-defined disk-write
//! the `run_full` orchestration performs. Splitting the assembly (a Transform)
//! from the serialization exit (this Sink) is what lets every fold-reading export
//! leaf consume THIS run's freshly-composed fold rather than the stale committed
//! file (the single-pass invariant).

use std::collections::BTreeMap;

use crate::node::{SINK_CAPABILITY, Stage, StageInput, StageOutput, StageProduct};
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
    /// by-reference blob sources it folds into the terminal `gmeow.gts` package:
    /// the in-memory JSON-Schema/axiom/reasoning/SHACL-report products plus the
    /// byte-decorated RDF 1.2 statement lanes (`stage-statements`).
    /// It holds [`SINK_CAPABILITY`] — the sole serialization exit the loader requires
    /// exactly one stage to hold (mirrored by the slice
    /// `gmeow:stage-gts-sink gmeow:hasCapability gmeow:sinkCapability`).
    pub fn new() -> Self {
        Self {
            consumes: vec![
                "stage-snapshot".to_string(),
                // The executable-docs "try it" surface reasons over the object-level EDB,
                // whose authored / imports / alignments graphs ride on the source-load
                // product (read, not re-loaded from disk).
                "stage-source-load".to_string(),
                "stage-export-json-schema".to_string(),
                "stage-compile-logic".to_string(),
                "stage-mappings".to_string(),
                "stage-reason".to_string(),
                "stage-statements".to_string(),
                "stage-validate".to_string(),
                // The opaque fanout members ride in from their producing export leaves
                // (each rendered once, in the leaf); `build_fanout_opaque_blob` reads them
                // off these products instead of re-rendering from disk (§3.2/§4).
                "stage-export-agreement".to_string(),
                "stage-export-apache".to_string(),
                "stage-export-bench".to_string(),
                "stage-export-cost-ledger".to_string(),
                "stage-export-evals".to_string(),
                "stage-export-matrix".to_string(),
                "stage-export-metadata".to_string(),
                // The Pydantic model package, folded into REP_MODELS_PYTHON by
                // build_archive_blobs from this run's fresh product.
                "stage-export-pydantic".to_string(),
                "stage-export-references".to_string(),
                "stage-export-research-objects".to_string(),
                // The generated shape surfaces (P11 frame shapes + the ResultShape
                // SHACL projection): `serialize_carrier_snapshot` folds REP_SHAPES'
                // generated members from THESE runs' in-memory products, never a
                // stale disk read (the same freshness rule as validation-shapes.ttl).
                // Without these edges a new competency ResultShape could never reach
                // the bundle — the fanout would rewrite the stale committed
                // generated/shapes bytes forever. constraint-shapes.ttl (the logic:
                // FOL-axiom projection) folds the same way, and on a first run does not
                // yet exist on disk, so only the fresh product can carry it (H8).
                "stage-export-frame-shapes".to_string(),
                "stage-export-result-shapes".to_string(),
                "stage-export-constraint-shapes".to_string(),
                // The two slice-quality floor TSVs (P17 projection of the ontology
                // gmeow:AxisFloorCommitment / gmeow:SliceTierFloor individuals): opaque
                // REP_GENERATED fanout members read off this leaf's product.
                "stage-export-governance-floors".to_string(),
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
        // v4: the opaque fanout members (references / bench / apache / matrix / eval +
        // research-object sidecars / metadata) ride in from their producing export leaves;
        // `build_fanout_opaque_blob` reads them off those products instead of re-rendering
        // from disk, and statements / dsl-stats / context ride off the already-consumed
        // stage-statements / stage-mappings products (§3.2 transform-once, §4 pure terminal).
        // v5: REP_SHAPES' generated members (result-shapes.ttl + frame-shapes.ttl)
        // are folded from the consumed export-leaf products instead of a stale
        // disk read, matching the validation-shapes.ttl freshness rule.
        "gts_sink.v5-fresh-generated-shape-surfaces"
    }
    fn run(&self, input: StageInput<'_>) -> Result<StageOutput, gmeow_errors::Diag> {
        // The terminal gts ARCHIVE writer: serialize THIS run's carrier
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
        Ok(StageOutput::new(StageProduct::from_artifacts(
            self.id(),
            artifacts,
        )))
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
        let carrier = purrdf::parse_dataset(
            b"<https://blackcatinformatics.ca/gmeow> <http://purl.org/dc/terms/title> \"GMEOW\" .\n\
              <https://blackcatinformatics.ca/gmeow> <http://purl.org/dc/terms/description> \"test bundle\" .\n\
              <https://blackcatinformatics.ca/gmeow> <http://www.w3.org/2002/07/owl#versionInfo> \"test\" .\n\
              <https://example.org/s> <https://example.org/p> <https://example.org/o> .\n",
            "application/n-triples",
            None,
        )
        .expect("minimal carrier dataset");
        let snapshot =
            StageProduct::from_artifacts_over("stage-snapshot", carrier, BTreeMap::new());

        // A minimal source-load product: the executable-docs "try it" EDB reads the
        // authored / imports / alignments graphs off it (empty here — this unit test
        // pins the sink's fail-closed wiring, not a real reasoned closure).
        let mut source_load_artifacts: BTreeMap<String, Vec<u8>> = BTreeMap::new();
        source_load_artifacts.insert(
            crate::stages::carrier::SLICE_QUALITY_REPORT_HTML_ARTIFACT.to_string(),
            b"<!doctype html><title>slice-quality</title>\n".to_vec(),
        );
        let source_load = StageProduct::from_artifacts_over(
            "stage-source-load",
            purrdf::parse_dataset(b"", "application/n-quads", None)
                .expect("empty source-load dataset"),
            source_load_artifacts,
        );

        let mut compile_artifacts: BTreeMap<String, Vec<u8>> = BTreeMap::new();
        for path in [
            "generated/owl/gmeow-dl.ttl",
            "generated/owl/gmeow-el.ttl",
            "generated/logic/gmeow.logic.rdf12.ttl",
            "generated/datalog/gmeow.dl",
            // The SHACL-AF rule (computation) surface the generated-fanout archive pulls
            // from the compile-logic product (design/LOGIC-SHACL-AF.md).
            crate::stages::compile_logic::SHACL_AF_PATH,
            // The validation-shape surfaces (SHACL Core + ShEx) — the OPT/ADL constraints axis.
            crate::stages::compile_logic::VALIDATION_SHAPES_TTL_PATH,
            crate::stages::compile_logic::VALIDATION_SHAPES_SHEX_PATH,
            crate::stages::compile_logic::PROCEDURAL_CONSTRAINTS_PATH,
            crate::stages::compile_logic::N3_PATH,
            crate::stages::compile_logic::CLIF_PATH,
            crate::stages::compile_logic::CGIF_PATH,
            crate::stages::compile_logic::XCL_PATH,
            crate::stages::compile_logic::GUFO_PATH,
            crate::stages::compile_logic::RELATIONAL_CORE_PATH,
            crate::stages::compile_logic::CORRESPONDENCE_PATH,
            crate::stages::compile_logic::DIAG_JSON_PATH,
            crate::stages::compile_logic::DIAG_SARIF_PATH,
            crate::stages::compile_logic::DIAG_HTML_PATH,
            crate::stages::compile_logic::DIAG_RDF_PATH,
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
        json_artifacts.insert(
            crate::stages::json_schema::CARD_SCHEMA_PATH.to_string(),
            b"{}".to_vec(),
        );
        json_artifacts.insert(
            crate::stages::json_schema::FINDING_SCHEMA_PATH.to_string(),
            b"{}".to_vec(),
        );
        let json_schema = StageProduct::from_artifacts("stage-export-json-schema", json_artifacts);

        let mut mapping_artifacts: BTreeMap<String, Vec<u8>> = BTreeMap::new();
        mapping_artifacts.insert(
            crate::stages::compile_logic::PROJECTION_REPORT_PATH.to_string(),
            b"@prefix owl: <http://www.w3.org/2002/07/owl#> .\n<https://example.org/projection-report> a owl:Ontology .\n".to_vec(),
        );
        // REP_MAPPINGS folds the SSSOM surface from this product (fail-closed: an
        // empty match is a hard error, so the minimal set carries one file).
        mapping_artifacts.insert(
            "generated/mappings/gmeow-test.sssom.tsv".to_string(),
            b"# minimal sssom\n".to_vec(),
        );
        // REP_QUERIES folds the generated SPARQL surface from this product (fail-closed,
        // same rule as SSSOM: a `.rq` edit must reach the bundle in one regenerate).
        mapping_artifacts.insert(
            "generated/queries/gmeow-test.rq".to_string(),
            b"# minimal query\n".to_vec(),
        );
        // The fanout presenter reads dsl-stats + the JSON-LD context + the EmotionML XML
        // projection off this product.
        mapping_artifacts.insert(
            crate::stages::mappings::DSL_STATS_PATH.to_string(),
            b"{}".to_vec(),
        );
        mapping_artifacts.insert(
            crate::stages::mappings::JSONLD_CONTEXT_PATH.to_string(),
            b"{}".to_vec(),
        );
        mapping_artifacts.insert(
            crate::stages::mappings::EMOTIONML_PATH.to_string(),
            b"<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<emotionml/>\n".to_vec(),
        );
        let mappings = StageProduct::from_artifacts("stage-mappings", mapping_artifacts);

        let mut reason_artifacts: BTreeMap<String, Vec<u8>> = BTreeMap::new();
        reason_artifacts.insert(
            crate::stages::reason::CLOSURE_PATH.to_string(),
            b"# closure".to_vec(),
        );
        reason_artifacts.insert(
            crate::stages::reason::EXPLANATIONS_PATH.to_string(),
            b"# explanations".to_vec(),
        );
        reason_artifacts.insert(
            crate::stages::reason::LEDGER_PATH.to_string(),
            b"# ledger".to_vec(),
        );
        reason_artifacts.insert(
            crate::stages::reason::PERF_LEDGER_PATH.to_string(),
            b"# perf".to_vec(),
        );
        let reason = StageProduct::from_artifacts("stage-reason", reason_artifacts);

        let mut statement_artifacts: BTreeMap<String, Vec<u8>> = BTreeMap::new();
        statement_artifacts.insert(
            crate::stages::statements::OWL_PATH.to_string(),
            b"# statements owl".to_vec(),
        );
        statement_artifacts.insert(
            crate::stages::statements::RDF12_PATH.to_string(),
            b"# statements rdf12".to_vec(),
        );
        let statements = StageProduct::from_artifacts("stage-statements", statement_artifacts);

        let mut validate_artifacts: BTreeMap<String, Vec<u8>> = BTreeMap::new();
        validate_artifacts.insert(
            crate::stages::validate::SHACL_JSON_PATH.to_string(),
            b"{}".to_vec(),
        );
        validate_artifacts.insert(
            crate::stages::validate::SHACL_SARIF_PATH.to_string(),
            b"{}".to_vec(),
        );
        validate_artifacts.insert(
            crate::stages::validate::SHACL_HTML_PATH.to_string(),
            b"".to_vec(),
        );
        let validate = StageProduct::from_artifacts("stage-validate", validate_artifacts);

        // The generated shape surfaces are required products (fail-closed):
        // REP_SHAPES folds them from the in-memory products, never a disk read.
        let mut result_shapes_artifacts: BTreeMap<String, Vec<u8>> = BTreeMap::new();
        result_shapes_artifacts.insert(
            crate::stages::result_shapes::RESULT_SHAPES_PATH.to_string(),
            b"# result shapes".to_vec(),
        );
        let result_shapes =
            StageProduct::from_artifacts("stage-export-result-shapes", result_shapes_artifacts);
        let mut frame_shapes_artifacts: BTreeMap<String, Vec<u8>> = BTreeMap::new();
        frame_shapes_artifacts.insert(
            crate::stages::frame_shapes::FRAME_SHAPES_PATH.to_string(),
            b"# frame shapes".to_vec(),
        );
        let frame_shapes =
            StageProduct::from_artifacts("stage-export-frame-shapes", frame_shapes_artifacts);
        let mut constraint_shapes_artifacts: BTreeMap<String, Vec<u8>> = BTreeMap::new();
        constraint_shapes_artifacts.insert(
            crate::stages::constraint_shapes::CONSTRAINT_SHAPES_PATH.to_string(),
            b"# constraint shapes".to_vec(),
        );
        let constraint_shapes = StageProduct::from_artifacts(
            "stage-export-constraint-shapes",
            constraint_shapes_artifacts,
        );

        // The opaque-fanout export leaves: the presenter reads their rendered members off
        // these products (empty here — this unit test pins the sink's fail-closed wiring,
        // not a real fanout; the superset gate is exercised end-to-end in fanout_parity).
        let export_leaves: Vec<StageProduct> = [
            "stage-export-agreement",
            "stage-export-bench",
            "stage-export-cost-ledger",
            "stage-export-apache",
            "stage-export-matrix",
            "stage-export-evals",
            "stage-export-research-objects",
            "stage-export-metadata",
            "stage-export-governance-floors",
        ]
        .into_iter()
        .map(|id| StageProduct::from_artifacts(id, BTreeMap::new()))
        .collect();

        // The references export leaf carries THIS run's generated `references.bib`, which
        // `build_docs_print_blob` folds into the print PDF's bibliography — a minimal valid
        // BibTeX database so the print-blob wiring is exercised fail-closed.
        let mut references_artifacts: BTreeMap<String, Vec<u8>> = BTreeMap::new();
        references_artifacts.insert(
            crate::stages::references::BIB_PATH.to_string(),
            b"@article{gmeow2026,\n  title = {The GMEOW Ontology},\n  author = {Audley, Patrick},\n  year = {2026},\n}\n".to_vec(),
        );
        let references =
            StageProduct::from_artifacts("stage-export-references", references_artifacts);

        // The Pydantic model package product: a minimal non-empty member so the
        // models-python blob fold clears its fail-closed guard (the carrier folds
        // this run's fresh package, never a stale disk read).
        let mut pydantic_artifacts: BTreeMap<String, Vec<u8>> = BTreeMap::new();
        pydantic_artifacts.insert(
            format!(
                "{}gmeow_models/__init__.py",
                crate::stages::pydantic::PACKAGE_DISK_PREFIX
            ),
            b"# gmeow_models\n".to_vec(),
        );
        let pydantic = StageProduct::from_artifacts("stage-export-pydantic", pydantic_artifacts);

        let mut upstream: BTreeMap<String, StageProduct> = BTreeMap::new();
        upstream.insert("stage-export-references".to_string(), references);
        upstream.insert("stage-export-pydantic".to_string(), pydantic);
        upstream.insert("stage-compile-logic".to_string(), compile);
        upstream.insert("stage-export-json-schema".to_string(), json_schema);
        upstream.insert("stage-mappings".to_string(), mappings);
        upstream.insert("stage-reason".to_string(), reason);
        upstream.insert("stage-snapshot".to_string(), snapshot);
        upstream.insert("stage-source-load".to_string(), source_load);
        upstream.insert("stage-statements".to_string(), statements);
        upstream.insert("stage-validate".to_string(), validate);
        upstream.insert("stage-export-result-shapes".to_string(), result_shapes);
        upstream.insert("stage-export-frame-shapes".to_string(), frame_shapes);
        upstream.insert(
            "stage-export-constraint-shapes".to_string(),
            constraint_shapes,
        );
        for product in export_leaves {
            upstream.insert(product.stage_id.clone(), product);
        }
        // Drive the docs-model-injectable serializer directly with an EMPTY DocsModel so the
        // OKF-coverage gate is scoped to the fixture (no documented terms → no dangling links),
        // exercising the sink's fail-closed blob/serialization wiring without paying for the
        // whole-ontology docs-corpus discovery. This test's subject is the serialization wiring,
        // NOT OKF coverage — that gate stays non-vacuously exercised by
        // `okf_link_targets_missing_from` (populated links) and the end-to-end pipeline test over
        // the real full carrier. The thin `run()` wrapper (carrier off the snapshot product then
        // `serialize_carrier_snapshot`) is exercised by that end-to-end test.
        let carrier =
            crate::stages::carrier::snapshot_dataset(&upstream).expect("snapshot carrier");
        let empty_docs = gmeow_docs::model::DocsModel::default();
        let emitted = crate::stages::carrier::serialize_carrier_snapshot_with_docs_model(
            &root,
            &upstream,
            carrier.as_ref(),
            &empty_docs,
        )
        .expect("sink serializes the carrier");
        assert!(
            emitted.len() > 1024,
            "GTS bundle implausibly small: {} bytes",
            emitted.len()
        );

        // Round-trips through the kernel GTS importer (the bundle is well-formed).
        let _ = purrdf::import_gts_events(&emitted).expect("import_gts_events");
    }
}
