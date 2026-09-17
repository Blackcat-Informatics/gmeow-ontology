// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The `yaml_ld` export leaf: RDF → YAML-LD-star / JSON-LD-star.
//!
//! Emits both the JSON-LD-star lead artifact and a deterministic YAML-LD-star
//! derivative, plus a small serialization-preservation ledger.
//!
//! The JSON-LD-star / YAML-LD-star CODEC now lives in the lowest crate the rdf /
//! validate / pipeline consumers share (`purrdf::native_codecs::jsonld`). The
//! production functions in this stage are thin wrappers over it; only the
//! stage-specific code (the stage entry, the preservation ledger, the build-time
//! round-trip gate) lives here.
//!
//! # Peak residency
//!
//! Both codec entry points ([`jsonld::serialize_dataset_to_jsonld`] /
//! [`jsonld::serialize_dataset_to_yamlld`]) build their own whole-carrier
//! intermediate and return the finished document as one `String`, so this leaf's
//! measured allocation peak is 8.37 GiB. gmeow cannot share the intermediate between
//! the two calls (purrdf's `build_ser_graph` is crate-private) and must not grow a
//! second serializer to work around it, so the stage instead declares
//! [`crate::node::SERIALIZATION_BUFFER_RESOURCE`] and serializes against the other
//! whole-dataset leaf. Retiring the peak rather than scheduling around it is a purrdf
//! change: either expose the built serialization graph so one build feeds both
//! documents, or give the codecs an incremental `io::Write` sink.

use std::collections::BTreeMap;
use std::sync::Arc;

use purrdf::RdfDataset;
use purrdf::native_codecs::jsonld;
use serde_json::Value;

use crate::node::{
    CachePolicy, SERIALIZATION_BUFFER_RESOURCE, Stage, StageInput, StageOutput, StageProduct,
};

pub(crate) mod source_observation;

/// Logical path of the JSON-LD-star artifact emitted by this stage.
pub const JSON_LD_PATH: &str = "dist/gmeow.jsonld";
/// Logical path of the YAML-LD-star artifact emitted by this stage.
pub const YAML_LD_PATH: &str = "dist/gmeow.yamlld";
/// Logical path of the serialization-preservation ledger.
pub const PRESERVATION_PATH: &str = "generated/metadata/preservation.json";

// The statement-metadata reification vocabulary is gmeow's OWN ontology surface
// (`gmeow:StatementMetadata` / `gmeow:qSubject…`, defined in the kernel/provenance/
// standpoint slices and SKOS-aligned). purrdf's JSON-LD-star codec is namespace-
// parametric (`StatementMetadataVocab`); gmeow supplies these gmeow: IRIs so the
// downcast emits the ontology's terms, not purrdf's neutral default.

/// GMEOW quoted subject property.
pub const GMEOW_QSUBJECT: &str = "https://blackcatinformatics.ca/gmeow/qSubject";
/// GMEOW quoted predicate property.
pub const GMEOW_QPREDICATE: &str = "https://blackcatinformatics.ca/gmeow/qPredicate";
/// GMEOW quoted object property (IRI / blank-node objects).
pub const GMEOW_QOBJECT: &str = "https://blackcatinformatics.ca/gmeow/qObject";
/// GMEOW quoted literal object property.
pub const GMEOW_QOBJECTLITERAL: &str = "https://blackcatinformatics.ca/gmeow/qObjectLiteral";
/// GMEOW statement-metadata class.
pub const GMEOW_STATEMENT_METADATA: &str = "https://blackcatinformatics.ca/gmeow/StatementMetadata";
/// RDF 1.2 reifier predicate (re-exported from the codec so the tests + downcast share
/// one definition).
pub use jsonld::RDF_REIFIES;

/// The gmeow-namespace reification vocab handed to purrdf's parametric downcast.
fn gmeow_statement_metadata_vocab() -> jsonld::StatementMetadataVocab<'static> {
    jsonld::StatementMetadataVocab {
        statement_metadata: GMEOW_STATEMENT_METADATA,
        q_subject: GMEOW_QSUBJECT,
        q_predicate: GMEOW_QPREDICATE,
        q_object: GMEOW_QOBJECT,
        q_object_literal: GMEOW_QOBJECTLITERAL,
    }
}

/// Map a JSON-LD/YAML-LD codec diagnostic onto a pipeline decode error.
fn codec_err(e: purrdf::RdfDiagnostic) -> gmeow_errors::Diag {
    gmeow_errors::Diag::of_kind(crate::error::Decode {
        message: e.to_string(),
    })
}

/// The `yaml_ld` export-leaf stage.
pub struct YamlLdStage {
    consumes: Vec<String>,
    resources: Vec<String>,
}

impl YamlLdStage {
    /// Construct the stage; it consumes THIS run's snapshot fold.
    ///
    /// It requires [`SERIALIZATION_BUFFER_RESOURCE`]: both codec calls below build a
    /// whole-carrier intermediate and return a whole-document `String` (870 MB of
    /// JSON-LD-star + 673 MB of YAML-LD-star on the shipped corpus), so its measured
    /// peak allocation is 8.37 GiB — second only to `stage-export-export`, and fatal
    /// on a 16 GB runner if the two overlap. Mirrored by
    /// `gmeow:stage-export-yaml-ld gmeow:requiresResource
    /// gmeow:serializationBufferResource` in `slices/core/pipeline/module.ttl`; the
    /// loader HARD-fails on disagreement.
    pub fn new() -> Self {
        Self {
            consumes: vec!["stage-snapshot".to_string()],
            resources: vec![SERIALIZATION_BUFFER_RESOURCE.to_string()],
        }
    }
}

impl Default for YamlLdStage {
    fn default() -> Self {
        Self::new()
    }
}

impl Stage for YamlLdStage {
    fn id(&self) -> &str {
        "stage-export-yaml-ld"
    }
    fn consumes(&self) -> &[String] {
        &self.consumes
    }
    fn resources(&self) -> &[String] {
        &self.resources
    }
    fn cache_policy(&self) -> CachePolicy {
        // Measured contribution: 1.555 GB serialized / ~79.5 s rebuild, with an
        // 8.37-GiB renderer peak. The whole-document pair is not a bounded cache unit.
        CachePolicy::Recompute
    }
    fn impl_version(&self) -> &str {
        // v2: adds deterministic YAML-LD-star output and the preservation ledger.
        "yaml_ld.jsonld_star.v2-yaml-ld"
    }
    fn run(&self, _input: StageInput<'_>) -> Result<StageOutput, gmeow_errors::Diag> {
        // THIS run's carrier dataset, read directly off the snapshot product's bundle
        // — no re-parse of the gmeow.gts bytes (GTS is exit-only).
        let dataset = crate::stages::carrier::snapshot_dataset(_input.upstream)?;
        let json = serialize_graph(dataset.as_ref())?;
        let yaml = serialize_graph_yaml(dataset.as_ref(), None)?;
        let preservation = preservation_ledger();
        let mut artifacts: BTreeMap<String, Vec<u8>> = BTreeMap::new();
        artifacts.insert(JSON_LD_PATH.to_string(), json.into_bytes());
        artifacts.insert(YAML_LD_PATH.to_string(), yaml.into_bytes());
        artifacts.insert(PRESERVATION_PATH.to_string(), preservation.into_bytes());
        Ok(StageOutput::new(StageProduct::from_artifacts(
            self.id(),
            artifacts,
        )))
    }
}

/// Convert a sorted BTreeMap into a serde_json object value.
fn to_json_object(map: BTreeMap<String, Value>) -> Value {
    Value::Object(map.into_iter().collect())
}

/// Serialize the carrier dataset to a deterministic JSON-LD-star document (thin wrapper
/// over the first-party rdf codec).
pub fn serialize_graph(dataset: &RdfDataset) -> Result<String, gmeow_errors::Diag> {
    jsonld::serialize_dataset_to_jsonld(dataset).map_err(codec_err)
}

/// Serialize the carrier dataset to deterministic YAML-LD-star bytes (thin wrapper over
/// the first-party rdf codec).
///
/// The JSON-LD-star document is re-serialized to YAML with sorted keys, block style, no
/// anchors/aliases, and an explicit `@context`. The header carries a YAML
/// language-server schema reference.
pub fn serialize_graph_yaml(
    dataset: &RdfDataset,
    schema_url: Option<&str>,
) -> Result<String, gmeow_errors::Diag> {
    // purrdf is namespace-neutral: with no schema_url it stamps its own
    // `purrdf.schema.json` header. gmeow's bundled YAML-LD schema is
    // `gmeow.schema.json`, so default `None` to it (the consumer's schema).
    let schema_url = schema_url.or(Some(GMEOW_BUNDLED_SCHEMA));
    jsonld::serialize_dataset_to_yamlld(dataset, schema_url).map_err(codec_err)
}

/// gmeow's bundled YAML-LD language-server schema reference (resolves inside the
/// `gmeow.gts` snapshot as a bare member name).
pub const GMEOW_BUNDLED_SCHEMA: &str = "gmeow.schema.json";

/// Serialization-preservation ledger: records JSON-LD-star and YAML-LD-star as lossless.
pub(crate) fn preservation_ledger() -> String {
    // A deliberately simple, versioned JSON ledger. It is intentionally NOT
    // conflated with the logic-projection PreservationKind vocabulary.
    let mut map: BTreeMap<String, Value> = BTreeMap::new();
    let mut entry: BTreeMap<String, Value> = BTreeMap::new();
    entry.insert(
        "preservation".to_string(),
        Value::String("lossless".to_string()),
    );
    entry.insert("roundTrips".to_string(), Value::Bool(true));
    entry.insert(
        "note".to_string(),
        Value::String("RDF 1.2-star quoted triples and annotations round-trip through the JSON-LD-star / YAML-LD-star surface.".to_string()),
    );
    map.insert("json-ld-star".to_string(), to_json_object(entry.clone()));
    map.insert("yaml-ld-star".to_string(), to_json_object(entry));
    serde_json::to_string_pretty(&to_json_object(map))
        .expect("preservation ledger is serializable JSON")
}

/// Parse JSON-LD-star bytes into the native carrier [`RdfDataset`] (thin wrapper over
/// the first-party rdf codec).
pub fn parse_jsonld_star(json_bytes: &[u8]) -> Result<Arc<RdfDataset>, gmeow_errors::Diag> {
    jsonld::parse_jsonld(json_bytes, None).map_err(codec_err)
}

/// Convert a JSON-LD-star document to GMEOW statement-metadata N-Quads (thin wrapper
/// over the first-party rdf codec). The output contains no quoted triple terms, so it is
/// safe for the rdflib-compat up-projection lane.
pub fn jsonld_star_to_gmeow_statement_metadata_nquads(
    json_bytes: &[u8],
) -> Result<String, gmeow_errors::Diag> {
    jsonld::jsonld_to_statement_metadata_nquads(
        json_bytes,
        None,
        Some(&gmeow_statement_metadata_vocab()),
    )
    .map_err(codec_err)
}

/// Convert YAML-LD-star bytes to JSON-LD-star JSON (thin wrapper over the first-party
/// rdf codec), hard-failing on YAML anchors/aliases (extended YAML is out of scope).
pub fn yaml_ld_star_to_json(yaml_bytes: &[u8]) -> Result<String, gmeow_errors::Diag> {
    jsonld::yamlld_to_jsonld(yaml_bytes).map_err(codec_err)
}

/// Downcast YAML-LD-star bytes to GMEOW statement-metadata N-Quads (thin wrapper over
/// the first-party rdf codec).
pub fn yaml_ld_star_to_gmeow_statement_metadata_nquads(
    yaml_bytes: &[u8],
) -> Result<String, gmeow_errors::Diag> {
    jsonld::yamlld_to_statement_metadata_nquads(
        yaml_bytes,
        None,
        Some(&gmeow_statement_metadata_vocab()),
    )
    .map_err(codec_err)
}

/// Return an RDFC-1.0 canonical, deterministically sorted quad representation.
///
/// The build-time round-trip gate ([`roundtrip_isomorphic`]) and the tests share one
/// canonicalizer.
pub(crate) fn canonical_lines(dataset: &RdfDataset) -> Vec<String> {
    // Native full RDFC-1.0 over the FLATTENED carrier: `canonical_flat_nquads`
    // re-materializes the RDF 1.2 statement overlay to plain `rdf:reifies` / annotation
    // triples before canonicalizing.
    let canonical = purrdf::canonical_flat_nquads(dataset)
        .expect("RDFC-1.0 canonicalization of parsed dataset");
    let mut lines: Vec<String> = canonical.lines().map(str::to_owned).collect();
    lines.sort();
    lines
}

/// Parse N-Quads-star text into the native carrier [`RdfDataset`], preserving the
/// RDF 1.2 statement layer (quoted triple terms fold to the reifier table). Used by
/// [`roundtrip_isomorphic`].
fn dataset_from_nquads(nquads: &[u8]) -> Result<Arc<RdfDataset>, gmeow_errors::Diag> {
    // The native codec folds the RDF 1.2 statement layer to the IR reifier table at parse
    // time; `canonical_lines` un-folds it back to the equivalent flat `<reifier> rdf:reifies
    // <<( s p o )>>` rows (exact inverses), so the star structure the RDFC-1.0 canonical
    // comparison depends on is preserved.
    purrdf::parse_dataset(nquads, "application/n-quads", None).map_err(|e| {
        gmeow_errors::Diag::of_kind(crate::error::Parse {
            message: format!("parse N-Quads: {e}"),
        })
    })
}

/// Return whether `star_bytes` (format `"jsonld"`|`"yamlld"`) re-parses to a
/// dataset isomorphic (RDFC-1.0 canonical) to the original N-Quads-star input.
/// This is the Rust authority for the build-time serialization-isomorphism gate,
/// replacing the Python `_round_trip_star`.
pub fn roundtrip_isomorphic(
    original_nquads: &[u8],
    star_bytes: &[u8],
    format: &str,
) -> Result<bool, gmeow_errors::Diag> {
    let original = dataset_from_nquads(original_nquads)?;
    let roundtrip = match format {
        "jsonld" => parse_jsonld_star(star_bytes)?,
        "yamlld" => parse_jsonld_star(yaml_ld_star_to_json(star_bytes)?.as_bytes())?,
        other => {
            return Err(gmeow_errors::Diag::of_kind(crate::error::Decode {
                message: format!("unknown star format {other:?}; expected 'jsonld' or 'yamlld'"),
            }));
        }
    };
    Ok(canonical_lines(&original) == canonical_lines(&roundtrip))
}

#[path = "yaml_ld.tests.rs"]
#[cfg(test)]
mod tests;
