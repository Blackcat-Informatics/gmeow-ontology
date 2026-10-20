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
//! carries through the shared ledger, cache replay and terminal diagnostics graph.

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
    entities: Vec<(String, Vec<String>)>,
}

impl JsonSchemaStage {
    /// Construct the stage. It reads the AUTHORED shape/ontology sources from disk
    /// and consumes the four generated-shape producers so the compiled union folds
    /// THIS run's fresh `generated/shapes/*.ttl` bytes (never the stale committed
    /// files — the stale-disk-fold class).
    pub fn new() -> Self {
        Self {
            consumes: crate::stages::shape_union_fresh::producer_consumes(),
            entities: vec![(
                "stage-compile-logic".to_owned(),
                crate::stages::compile_logic::carrier_entity_list(),
            )],
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
    fn consumed_entities(&self) -> &[(String, Vec<String>)] {
        &self.entities
    }
    fn attaches_graphs(&self) -> &[String] {
        crate::stages::attach::graphs(self.id())
    }
    fn attaches_blob_reps(&self) -> &[String] {
        crate::stages::attach::blob_reps(self.id())
    }
    fn impl_version(&self) -> &str {
        "json_schema.v3-carried-projection-losses"
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
        // Compile + enrich via the ONE shared builder json-schema and schemas (LinkML/
        // TypeScript/GraphQL) both compile through, so their `$defs` stay byte-identical.
        // Already serialized via the exact `schema_bytes` convention (serde pretty +
        // trailing LF), so the artifact bytes are used as-is — no re-parse round trip.
        let compiled =
            crate::stages::schema_compile::enriched_compiled_schema(input.root, &shapes)?;
        publish_schema(self.id(), compiled)
    }
}

/// Publish the native compilation's schemas and complete located loss witnesses.
fn publish_schema(
    stage: &str,
    compiled: purrdf::shapes::json_schema::CompiledSchema,
) -> Result<StageOutput, gmeow_errors::Diag> {
    let mut ledger = gmeow_errors::DiagLedger::new();
    for loss in compiled.losses.entries() {
        ledger.attach(
            gmeow_errors::Diag::from_rdf_loss(loss.clone()),
            gmeow_errors::StageId::new(stage),
        );
    }
    let nodes = ledger
        .emit_sorted()
        .into_iter()
        .cloned()
        .collect::<Vec<_>>();
    let report = ledger.project_report("json-schema");
    let mut builder = purrdf::RdfDatasetBuilder::new();
    let graph = builder.intern_iri(crate::stages::carrier::GRAPH_DIAGNOSTICS)?;
    builder.declare_named_graph(graph)?;
    gmeow_errors::render::append_gmeow_findings(
        &report,
        crate::stages::carrier::GRAPH_DIAGNOSTICS,
        &mut builder,
    )?;
    let dataset = builder.freeze()?;
    let diag_blob = crate::node::encode_diag_nodes(stage, &nodes).map_err(|error| {
        gmeow_errors::Diag::of_kind(crate::error::StageFailed {
            stage: stage.to_owned(),
            message: format!("encode schema projection diagnostics: {error}"),
        })
    })?;
    let mut artifacts: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    artifacts.insert(
        JSON_SCHEMA_PATH.to_string(),
        compiled.schema_json.into_bytes(),
    );
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
    let bundle = crate::bundle::bundle_from_artifacts_over_with_rep_blob(
        dataset,
        artifacts,
        purrdf::provenance::DatasetProvenance::new(),
        crate::stages::carrier::REP_DIAG_NODES,
        "application/json",
        diag_blob,
    );
    Ok(StageOutput {
        product: StageProduct::from_bundle(stage, std::sync::Arc::new(bundle)),
        diags: nodes,
        timings: Vec::new(),
    })
}

#[path = "json_schema.tests.rs"]
#[cfg(test)]
mod tests;
