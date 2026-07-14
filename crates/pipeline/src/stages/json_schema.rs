// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The `json-schema` export leaf: native SHACL → JSON Schema + OpenAPI.
//!
//! Replaces the Python LinkML `JsonSchemaGenerator` + OpenAPI derivation (which
//! went through the LinkML toolkit). This leaf compiles the SAME SHACL shape
//! union the live validator enforces (`purrdf::shapes::shape_union::load_shapes`)
//! into a closed-world JSON Schema (draft 2020-12) and an OpenAPI 3.1 document
//! via the native emitter (`purrdf::shapes::json_schema::compile`) — no external
//! toolkit, no Python.
//!
//! A fresh-union export leaf: it declares the AUTHORED shape files as cache
//! inputs and `consumes()` the four generated-shape producers
//! ([`crate::stages::shape_union_fresh::GENERATED_SHAPE_PRODUCERS`]) so the
//! union's `generated/shapes/*.ttl` members are THIS run's product bytes, never
//! the previous run's committed files (the stale-disk-fold class). Output is
//! byte-deterministic (the emitter sorts every collection), so it is compared
//! byte-for-byte to the committed `generated/schemas/gmeow.schema.json` /
//! `gmeow.openapi.json`.
//!
//! SHACL constructs with no JSON Schema equivalent (`sh:sparql` etc.) are never
//! silently skipped: the emitter records each as a `LossRecord`, which this leaf
//! reports on stderr in aggregated form (lossy-projection discipline).

use std::collections::BTreeMap;
use std::path::Path;

use crate::node::{Stage, StageInput, StageOutput, StageProduct};

/// Committed logical path of the native JSON Schema (draft 2020-12).
pub const JSON_SCHEMA_PATH: &str = "generated/schemas/gmeow.schema.json";
/// Committed logical path of the native OpenAPI 3.1 document.
pub const OPENAPI_PATH: &str = "generated/schemas/gmeow.openapi.json";
/// Committed logical path of the term-`Card` JSON Schema (draft 2020-12) — the
/// self-describing schema for the packed `terms/{slug}/card.json` member and the
/// live MCP `doc_card format=json` payload. Hand-authored beside the `Card` type
/// ([`gmeow_docs::card::card_json_schema`]).
pub const CARD_SCHEMA_PATH: &str = "generated/schemas/card.schema.json";
/// Committed logical path of the `validate_local` [`EnrichedReport`] JSON Schema
/// (draft 2020-12) — the self-describing schema for the enriched-finding envelope.
/// Hand-authored beside the type
/// ([`gmeow_validate::local_oracle::finding_json_schema`]).
///
/// [`EnrichedReport`]: gmeow_validate::local_oracle::EnrichedReport
pub const FINDING_SCHEMA_PATH: &str = "generated/schemas/validate-finding.schema.json";

/// Serialize a hand-authored JSON Schema `Value` to deterministic pretty-printed
/// bytes with a trailing newline (matching the emitter's committed-file convention).
/// `serde_json::Value` objects are key-sorted without the `preserve_order` feature,
/// so the output is byte-stable across runs.
fn schema_bytes(schema: &serde_json::Value) -> Result<Vec<u8>, gmeow_errors::Diag> {
    let mut bytes = serde_json::to_vec_pretty(schema).map_err(|e| {
        gmeow_errors::Diag::of_kind(crate::error::Parse {
            message: format!("serialize hand-authored JSON Schema: {e}"),
        })
    })?;
    bytes.push(b'\n');
    Ok(bytes)
}

/// The `stage-export-json-schema` export-leaf stage.
pub struct JsonSchemaStage {
    consumes: Vec<String>,
}

impl JsonSchemaStage {
    /// Construct the stage. It reads the AUTHORED shape/ontology sources from disk
    /// and consumes the four generated-shape producers so the compiled union folds
    /// THIS run's fresh `generated/shapes/*.ttl` bytes (never the stale committed
    /// files — the stale-disk-fold class).
    pub fn new() -> Self {
        Self {
            consumes: crate::stages::shape_union_fresh::producer_consumes(),
        }
    }
}

impl Default for JsonSchemaStage {
    fn default() -> Self {
        Self::new()
    }
}

impl Stage for JsonSchemaStage {
    fn id(&self) -> &str {
        "stage-export-json-schema"
    }
    fn consumes(&self) -> &[String] {
        &self.consumes
    }
    fn impl_version(&self) -> &str {
        // v2: the union's generated/shapes/*.ttl members are product-sourced from the
        // consumed producer stages (shape_union_fresh) instead of read off disk, so a
        // shape-source edit reaches the compiled schema in ONE regenerate.
        "json_schema.v2-fresh-shape-union"
    }
    fn input_files(&self, root: &Path) -> Result<Vec<std::path::PathBuf>, gmeow_errors::Diag> {
        // The AUTHORED half of the shape union (`shapes/*.ttl` minus lints +
        // `slices/*/*/shapes.ttl`) is the disk source set the emitter reads — declared
        // as cache inputs so an authored-shape edit busts the cache. The GENERATED
        // members are NOT declared: they are product-sourced off the consumed producer
        // stages, whose product digests already key the cache (declaring a `generated/`
        // path here would itself be the stale-disk-fold bug class). The
        // value-vocabulary enrichment ALSO reads the ontology ABox
        // (`slices/**/module.ttl`), so those sources bust the cache too — a new
        // vocabulary member must reflow the schema.
        let mut files = crate::stages::shape_union_fresh::authored_shape_files(root)?;
        files.extend(crate::stages::value_vocab::ontology_module_files(root));
        files.sort();
        files.dedup();
        Ok(files)
    }
    fn run(&self, input: StageInput<'_>) -> Result<StageOutput, gmeow_errors::Diag> {
        let fresh = crate::stages::shape_union_fresh::fresh_generated_shape_members(
            self.id(),
            input.upstream,
        )?;
        let (_store, shapes) =
            crate::stages::shape_union_fresh::load_shapes_fresh(input.root, &fresh)?;
        let ns = crate::gmeow_ns::gmeow_json_schema_namespaces();
        let compiled = purrdf::shapes::json_schema::compile(&shapes, &ns);
        report_losses(&compiled.losses);
        // Enrich the shipped JSON Schema with the ontology's open value vocabularies —
        // the SAME enrichment the Pydantic surface applies, so both agree. Re-serialized
        // via `schema_bytes` (serde pretty + trailing LF), which reproduces `purrdf`'s
        // own `to_pretty` byte convention exactly.
        let mut schema: serde_json::Value =
            serde_json::from_str(&compiled.schema_json).map_err(|e| {
                gmeow_errors::Diag::of_kind(crate::error::Parse {
                    message: format!("parse compiled JSON Schema: {e}"),
                })
            })?;
        let onto = crate::stages::value_vocab::load_ontology_store(input.root)?;
        let onto_view = crate::stages::export::FoldView::new(&onto);
        crate::stages::value_vocab::enrich_value_vocab_enums(&mut schema, &ns, &onto_view);
        let mut artifacts: BTreeMap<String, Vec<u8>> = BTreeMap::new();
        artifacts.insert(JSON_SCHEMA_PATH.to_string(), schema_bytes(&schema)?);
        artifacts.insert(OPENAPI_PATH.to_string(), compiled.openapi_json.into_bytes());
        // The two hand-authored self-describing schemas, co-located with their Rust
        // types (drift-resistance). They ride REP_SCHEMAS so a repo-free consumer can
        // validate a `card.json` / a `validate_local` envelope straight from the
        // bundle.
        artifacts.insert(
            CARD_SCHEMA_PATH.to_string(),
            schema_bytes(&gmeow_docs::card::card_json_schema())?,
        );
        artifacts.insert(
            FINDING_SCHEMA_PATH.to_string(),
            schema_bytes(&gmeow_validate::local_oracle::finding_json_schema())?,
        );
        Ok(StageOutput::new(StageProduct::from_artifacts(
            self.id(),
            artifacts,
        )))
    }
}

fn report_losses(losses: &[purrdf::shapes::json_schema::LossRecord]) {
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
        tracing::info!(
            target: "json_schema_loss",
            construct = construct,
            shapes = shapes.len(),
            reason = reason,
            examples = %format!("{examples}{suffix}"),
            "lossy drop projecting SHACL to JSON Schema",
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

    /// The fresh `generated/shapes/*.ttl` byte map a hermetic test builds from the
    /// COMMITTED files — the same members [`fresh_generated_shape_members`] pulls
    /// off the producer products in production
    /// ([`crate::stages::shape_union_fresh::fresh_generated_shape_members`]), so
    /// the test exercises the stage's real fresh-union path without a pipeline run.
    fn committed_fresh_map(root: &Path) -> BTreeMap<String, Vec<u8>> {
        [
            crate::stages::compile_logic::VALIDATION_SHAPES_TTL_PATH,
            crate::stages::compile_logic::PROCEDURAL_CONSTRAINTS_PATH,
            crate::stages::constraint_shapes::CONSTRAINT_SHAPES_PATH,
            crate::stages::frame_shapes::FRAME_SHAPES_PATH,
            crate::stages::result_shapes::RESULT_SHAPES_PATH,
        ]
        .into_iter()
        .map(|rel| {
            (
                rel.to_string(),
                std::fs::read(root.join(rel)).unwrap_or_else(|e| panic!("read {rel}: {e}")),
            )
        })
        .collect()
    }

    /// Run the native stage over the real repo and assert the two artifacts are
    /// present, are valid JSON, the schema carries a non-empty `$defs`, and the
    /// whole output is byte-deterministic across two runs.
    fn run_once(root: &Path) -> BTreeMap<String, Vec<u8>> {
        let stage = JsonSchemaStage::new();
        let fresh = committed_fresh_map(root);
        let (_store, shapes) = crate::stages::shape_union_fresh::load_shapes_fresh(root, &fresh)
            .expect("load fresh shape union");
        let compiled = purrdf::shapes::json_schema::compile(
            &shapes,
            &crate::gmeow_ns::gmeow_json_schema_namespaces(),
        );
        let mut artifacts: BTreeMap<String, Vec<u8>> = BTreeMap::new();
        artifacts.insert(
            JSON_SCHEMA_PATH.to_string(),
            compiled.schema_json.into_bytes(),
        );
        artifacts.insert(OPENAPI_PATH.to_string(), compiled.openapi_json.into_bytes());
        artifacts.insert(
            CARD_SCHEMA_PATH.to_string(),
            schema_bytes(&gmeow_docs::card::card_json_schema()).unwrap(),
        );
        artifacts.insert(
            FINDING_SCHEMA_PATH.to_string(),
            schema_bytes(&gmeow_validate::local_oracle::finding_json_schema()).unwrap(),
        );
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
                if let Some(serde_json::Value::String(r)) = map.get("$ref")
                    && let Some(name) = r.strip_prefix("#/$defs/")
                {
                    out.push(name.to_owned());
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

    /// Corpus self-consistency invariant: compiling over the REAL repo
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
