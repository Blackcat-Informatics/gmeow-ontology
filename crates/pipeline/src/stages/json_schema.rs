// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The `json-schema` export leaf (#700): native SHACL → JSON Schema + OpenAPI.
//!
//! Replaces the Python LinkML `JsonSchemaGenerator` + OpenAPI derivation (which
//! went through the LinkML toolkit). This leaf compiles the SAME SHACL shape
//! union the live validator enforces (`gmeow_shacl::shape_union::load_shapes`)
//! into a closed-world JSON Schema (draft 2020-12) and an OpenAPI 3.1 document
//! via the native emitter (`gmeow_shacl::json_schema::compile`) — no external
//! toolkit, no Python.
//!
//! Like `frame_shapes`, this is a source-reading export leaf: it declares the
//! authored shape files as cache inputs and `consumes()` nothing, so it runs as
//! an independent phase-1 ExportLeaf. Output is byte-deterministic (the emitter
//! sorts every collection), so it is compared byte-for-byte to the committed
//! `generated/schemas/gmeow.schema.json` / `gmeow.openapi.json`.
//!
//! SHACL constructs with no JSON Schema equivalent (`sh:sparql` etc.) are never
//! silently skipped: the emitter records each as a `LossRecord`, which this leaf
//! reports on stderr in aggregated form (lossy-projection discipline).

use std::collections::BTreeMap;
use std::path::Path;

use crate::error::PipelineError;
use crate::node::{Stage, StageInput, StageOutput, StageProduct};

/// Committed logical path of the native JSON Schema (draft 2020-12).
pub const JSON_SCHEMA_PATH: &str = "generated/schemas/gmeow.schema.json";
/// Committed logical path of the native OpenAPI 3.1 document.
pub const OPENAPI_PATH: &str = "generated/schemas/gmeow.openapi.json";

/// The `stage-export-json-schema` export-leaf stage.
pub struct JsonSchemaStage;

impl Stage for JsonSchemaStage {
    fn id(&self) -> &str {
        "stage-export-json-schema"
    }
    fn consumes(&self) -> &[String] {
        &[]
    }
    fn impl_version(&self) -> &str {
        "json_schema.v1"
    }
    fn input_files(&self, root: &Path) -> Result<Vec<std::path::PathBuf>, PipelineError> {
        // The shape union (`shapes/*.ttl` minus lints + `generated/shapes/*.ttl`
        // + `slices/*/*/shapes.ttl`) is exactly the source set the emitter reads.
        // Declaring those as cache inputs keeps `consumes() == []` (no DAG edge)
        // while busting the cache whenever any shape file changes — the same
        // pattern frame_shapes uses for its authored sources.
        gmeow_shacl::shape_union::shape_files(root).map_err(PipelineError::Parse)
    }
    fn run(&self, input: StageInput<'_>) -> Result<StageOutput, PipelineError> {
        let (_store, shapes) =
            gmeow_shacl::shape_union::load_shapes(input.root).map_err(PipelineError::Parse)?;
        let compiled = gmeow_shacl::json_schema::compile(&shapes);
        report_losses(&compiled.losses);
        let mut artifacts: BTreeMap<String, Vec<u8>> = BTreeMap::new();
        artifacts.insert(
            JSON_SCHEMA_PATH.to_string(),
            compiled.schema_json.into_bytes(),
        );
        artifacts.insert(OPENAPI_PATH.to_string(), compiled.openapi_json.into_bytes());
        Ok(StageOutput {
            product: StageProduct::from_artifacts(self.id(), artifacts),
        })
    }
}

fn report_losses(losses: &[gmeow_shacl::json_schema::LossRecord]) {
    let mut grouped: BTreeMap<(&str, &str), Vec<&str>> = BTreeMap::new();
    for loss in losses {
        grouped
            .entry((loss.construct.as_str(), loss.reason.as_str()))
            .or_default()
            .push(loss.shape_iri.as_str());
    }
    for ((construct, reason), mut shapes) in grouped {
        shapes.sort_unstable();
        shapes.dedup();
        let examples = shapes
            .iter()
            .take(5)
            .copied()
            .collect::<Vec<_>>()
            .join(", ");
        let suffix = if shapes.len() > 5 {
            format!(" (+{} more)", shapes.len() - 5)
        } else {
            String::new()
        };
        eprintln!(
            "[json-schema] lossy drop: {construct} on {} shape(s) — {reason}; examples: {examples}{suffix}",
            shapes.len()
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repo_root() -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .canonicalize()
            .unwrap()
    }

    /// Run the native stage over the real repo and assert the two artifacts are
    /// present, are valid JSON, the schema carries a non-empty `$defs`, and the
    /// whole output is byte-deterministic across two runs.
    fn run_once(root: &Path) -> BTreeMap<String, Vec<u8>> {
        let stage = JsonSchemaStage;
        let (_store, shapes) =
            gmeow_shacl::shape_union::load_shapes(root).expect("load shape union");
        let compiled = gmeow_shacl::json_schema::compile(&shapes);
        let mut artifacts: BTreeMap<String, Vec<u8>> = BTreeMap::new();
        artifacts.insert(
            JSON_SCHEMA_PATH.to_string(),
            compiled.schema_json.into_bytes(),
        );
        artifacts.insert(OPENAPI_PATH.to_string(), compiled.openapi_json.into_bytes());
        // Touch `stage` so the id/impl coupling stays exercised.
        assert_eq!(stage.id(), "stage-export-json-schema");
        artifacts
    }

    #[test]
    fn json_schema_stage_emits_valid_deterministic_artifacts() {
        let root = repo_root();
        let first = run_once(&root);

        let schema_bytes = first
            .get(JSON_SCHEMA_PATH)
            .expect("schema artifact present");
        let openapi_bytes = first.get(OPENAPI_PATH).expect("openapi artifact present");

        // Both parse as JSON.
        let schema: serde_json::Value =
            serde_json::from_slice(schema_bytes).expect("schema is valid JSON");
        let _openapi: serde_json::Value =
            serde_json::from_slice(openapi_bytes).expect("openapi is valid JSON");

        // The schema has a non-empty `$defs` object.
        let defs = schema
            .get("$defs")
            .and_then(|v| v.as_object())
            .expect("schema has a $defs object");
        assert!(!defs.is_empty(), "$defs must be non-empty");

        // Byte-deterministic across two runs.
        let second = run_once(&root);
        assert_eq!(first, second, "json-schema output is non-deterministic");
    }

    /// Recursively collect every `#/$defs/<name>` ref reachable from a value.
    fn collect_def_refs(v: &serde_json::Value, out: &mut Vec<String>) {
        match v {
            serde_json::Value::Object(map) => {
                if let Some(serde_json::Value::String(r)) = map.get("$ref") {
                    if let Some(name) = r.strip_prefix("#/$defs/") {
                        out.push(name.to_owned());
                    }
                }
                for child in map.values() {
                    collect_def_refs(child, out);
                }
            }
            serde_json::Value::Array(items) => {
                for child in items {
                    collect_def_refs(child, out);
                }
            }
            _ => {}
        }
    }

    /// Corpus self-consistency invariant (#700): compiling over the REAL repo
    /// shape union must produce ZERO dangling `$ref`s — every `#/$defs/<name>`
    /// the schema references must resolve to an emitted `$def`. This guards the
    /// real corpus against the dangling-ref bug a draft-2020-12 validator rejects
    /// (`Pointer '/$defs/<name>' does not exist`).
    #[test]
    fn json_schema_corpus_has_no_dangling_refs() {
        let root = repo_root();
        let artifacts = run_once(&root);
        let schema: serde_json::Value = serde_json::from_slice(
            artifacts
                .get(JSON_SCHEMA_PATH)
                .expect("schema artifact present"),
        )
        .expect("schema is valid JSON");

        let defs: std::collections::BTreeSet<String> = schema
            .get("$defs")
            .and_then(|v| v.as_object())
            .expect("$defs object")
            .keys()
            .cloned()
            .collect();

        let mut refs = Vec::new();
        collect_def_refs(&schema, &mut refs);
        assert!(!refs.is_empty(), "expected refs in the real corpus schema");

        let dangling: Vec<&String> = refs.iter().filter(|r| !defs.contains(*r)).collect();
        assert!(
            dangling.is_empty(),
            "schema references {} dangling $defs over the real corpus: {:?}",
            dangling.len(),
            dangling
        );
    }
}
