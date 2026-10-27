// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Native schema export leaf: LinkML YAML plus TypeScript and GraphQL developer
//! surfaces, emitted by `purrdf`'s SHACL-derived shape emitters.
//!
//! This stage cuts over from the former hand-rolled OWL→LinkML model (which read
//! the composed carrier dataset) to the SAME fresh SHACL shape-union compilation
//! [`crate::stages::json_schema`] and [`crate::stages::pydantic`] run: it
//! consumes [`crate::stages::shape_union_fresh::producer_consumes`], compiles
//! the union through the ONE shared [`crate::stages::schema_compile`] builder
//! (compile + value-vocab enrich), and renders it with `purrdf::shapes::linkml`,
//! `purrdf::shapes::typescript`, and `purrdf::shapes::graphql` — no hand-rolled
//! renderer, no Python, no external LinkML toolkit. The three surfaces therefore
//! agree with the packed JSON Schema / Pydantic package by construction (same
//! compiled `$defs`).
//!
//! The Pydantic surface is NOT rendered here: it is a SHACL-derived,
//! per-slice package ([`crate::stages::pydantic`]) co-derived from the SAME shape
//! compilation as the JSON-Schema stage, not an OWL→LinkML projection.

use std::collections::BTreeMap;
use std::path::Path;

use purrdf::shapes::graphql::{
    GRAPHQL_NAME_MAP_PATH as GRAPHQL_NAME_MAP_ARTIFACT,
    GRAPHQL_SCHEMA_PATH as GRAPHQL_SCHEMA_ARTIFACT, GraphqlConfig, emit_graphql,
};
use purrdf::shapes::linkml::{LinkmlConfig, SanitizePolicy, emit_linkml};
use purrdf::shapes::typescript::{
    TYPESCRIPT_DECLARATION_PATH as TYPESCRIPT_DECLARATION_ARTIFACT, TypeScriptConfig,
    emit_typescript,
};
use purrdf_lex::json::{Object, Value, record::ToJson};

use crate::node::{Stage, StageInput, StageOutput, StageProduct};

/// The committed logical paths of the six schema artifacts owned by this stage.
pub const LINKML_PATH: &str = "generated/schemas/gmeow.linkml.yaml";
/// Complete source-to-LinkML element and slot naming evidence.
pub const LINKML_NAME_MAP_PATH: &str = "generated/schemas/gmeow.linkml.name-map.json";
/// The native versioned LinkML projection loss ledger, including locations.
pub const LINKML_LOSSES_PATH: &str = "generated/schemas/gmeow.linkml.losses.json";
pub const TYPESCRIPT_PATH: &str = "generated/schemas/gmeow.ts";
pub const GRAPHQL_PATH: &str = "generated/schemas/gmeow.graphql";
/// Committed logical path of the GraphQL canonical name map (`name-map.json`):
/// the source-field/enum-value → GraphQL-name codec `emit_graphql` also
/// produces. Shipped rather than dropped — a value-vocabulary field/enum name
/// the GraphQL SDL renames is otherwise unrecoverable (no-optionality forbids
/// silently discarding a produced artifact).
pub const GRAPHQL_NAME_MAP_PATH: &str = "generated/schemas/gmeow.graphql.name-map.json";
pub const SCHEMA_PATHS: [&str; 6] = [
    LINKML_PATH,
    LINKML_NAME_MAP_PATH,
    LINKML_LOSSES_PATH,
    TYPESCRIPT_PATH,
    GRAPHQL_PATH,
    GRAPHQL_NAME_MAP_PATH,
];

const DESCRIPTION: &str = "GMEOW developer schema generated from canonical OWL. Lossy by design: restrictions, reification, standpoint, inverseOf, and temporal scope are dropped.";

fn schema_error(message: impl Into<String>) -> gmeow_errors::Diag {
    gmeow_errors::Diag::of_kind(crate::error::StageFailed {
        stage: "stage-export-schemas".into(),
        message: message.into(),
    })
}

/// Every prefix a `$defs` property/class CURIE in the compiled schema can use:
/// the gmeow-owned ecosystem prefixes ([`gmeow_ns::gmeow_profile`] —
/// `gmeow`/`logic`/`lang`/`math`) plus the W3C builtins
/// `purrdf::shapes::json_schema::compile` ALWAYS merges in for CURIE compaction
/// (`xsd`/`rdf`/`rdfs`/`owl`/`sh` — e.g. an annotation property compacts to
/// `rdfs:label`), plus the `linkml` metamodel prefix [`LinkmlConfig`] requires.
/// These caller-owned namespaces drive compaction; the native slot policy
/// preserves unmatched source IRIs and reports their emitted names.
fn linkml_prefixes() -> BTreeMap<String, String> {
    let profile = gmeow_ns::gmeow_profile();
    let mut prefixes: BTreeMap<String, String> = profile.prefixes.into_iter().collect();
    prefixes.insert(profile.prefix, profile.namespace);
    prefixes.insert(
        "xsd".to_string(),
        "http://www.w3.org/2001/XMLSchema#".to_string(),
    );
    prefixes.insert(
        "rdf".to_string(),
        "http://www.w3.org/1999/02/22-rdf-syntax-ns#".to_string(),
    );
    prefixes.insert(
        "rdfs".to_string(),
        "http://www.w3.org/2000/01/rdf-schema#".to_string(),
    );
    prefixes.insert(
        "owl".to_string(),
        "http://www.w3.org/2002/07/owl#".to_string(),
    );
    prefixes.insert("sh".to_string(), "http://www.w3.org/ns/shacl#".to_string());
    prefixes.insert("linkml".to_string(), "https://w3id.org/linkml/".to_string());
    prefixes
}

/// The gmeow-owned LinkML emitter configuration (gmeow's explicit identity
/// + vocabulary — never fabricated by `purrdf`, which is namespace-neutral).
fn linkml_config(prefixes: BTreeMap<String, String>) -> Result<LinkmlConfig, gmeow_errors::Diag> {
    LinkmlConfig::new(
        "https://blackcatinformatics.ca/gmeow/linkml",
        "gmeow",
        DESCRIPTION,
        "gmeow",
        prefixes,
    )
    .map(|config| config.with_sanitize_policy(SanitizePolicy::Rename))
    .map_err(|e| schema_error(format!("build LinkmlConfig: {e}")))
}

/// The gmeow-owned TypeScript emitter configuration.
fn typescript_config() -> Result<TypeScriptConfig, gmeow_errors::Diag> {
    TypeScriptConfig::new("@blackcatinformatics/gmeow", DESCRIPTION, DESCRIPTION)
        .map_err(|e| schema_error(format!("build TypeScriptConfig: {e}")))
}

/// The gmeow-owned GraphQL emitter configuration. `GmeowScalar` is the
/// caller-owned fallback custom scalar for a value with no exact built-in
/// GraphQL carrier.
fn graphql_config() -> Result<GraphqlConfig, gmeow_errors::Diag> {
    GraphqlConfig::new("gmeow", DESCRIPTION, DESCRIPTION, "GmeowScalar")
        .map_err(|e| schema_error(format!("build GraphqlConfig: {e}")))
}

/// Aggregate one emitter's [`purrdf::LossLedger`] into a single `tracing` event
/// per `(code, note)` pair — mirrors [`crate::stages::json_schema::report_losses`]
/// so no emitter's projection losses are ever silently dropped. `surface`
/// identifies which of the three projections (`linkml` / `typescript` /
/// `graphql`) the ledger belongs to.
fn report_losses(surface: &str, losses: &purrdf::LossLedger) {
    let mut grouped: BTreeMap<(&str, &str), Vec<&str>> = BTreeMap::new();
    for loss in losses.entries() {
        let subject = loss
            .location
            .as_deref()
            .and_then(|location| location.subject.as_deref())
            .unwrap_or("<unlocated>");
        grouped
            .entry((loss.code.as_ref(), loss.note.as_ref()))
            .or_default()
            .push(subject);
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
            target: "schemas_loss",
            surface = surface,
            construct = construct,
            shapes = shapes.len(),
            reason = reason,
            examples = %format!("{examples}{suffix}"),
            "lossy drop projecting compiled JSON Schema to a developer schema surface",
        );
    }
}

/// Compile the fresh SHACL shape union and render the three `purrdf`-native
/// developer schema surfaces (LinkML YAML, TypeScript declarations, GraphQL SDL
/// + its name map), retaining complete LinkML naming and loss evidence.
fn render_schemas(
    root: &Path,
    shapes: &purrdf::shapes::shapes::Shapes,
) -> Result<BTreeMap<String, Vec<u8>>, gmeow_errors::Diag> {
    let compiled = crate::stages::schema_compile::enriched_compiled_schema(root, shapes)?;

    render_compiled_schemas(&compiled)
}

/// Render the same compiled document through all three upstream emitters.
fn render_compiled_schemas(
    compiled: &purrdf::shapes::json_schema::CompiledSchema,
) -> Result<BTreeMap<String, Vec<u8>>, gmeow_errors::Diag> {
    let linkml_config = linkml_config(linkml_prefixes())?;
    let linkml_package = emit_linkml(compiled, &linkml_config)
        .map_err(|e| schema_error(format!("emit_linkml: {e}")))?;
    report_losses("linkml", &linkml_package.losses);

    let typescript_package = emit_typescript(compiled, &typescript_config()?)
        .map_err(|e| schema_error(format!("emit_typescript: {e}")))?;
    report_losses("typescript", &typescript_package.losses);

    let graphql_package = emit_graphql(compiled, &graphql_config()?)
        .map_err(|e| schema_error(format!("emit_graphql: {e}")))?;
    report_losses("graphql", &graphql_package.losses);

    let typescript_bytes = typescript_package
        .artifacts
        .get(TYPESCRIPT_DECLARATION_ARTIFACT)
        .ok_or_else(|| {
            schema_error(format!(
                "emit_typescript did not produce {TYPESCRIPT_DECLARATION_ARTIFACT:?}"
            ))
        })?
        .clone();
    let graphql_schema_bytes = graphql_package
        .artifacts
        .get(GRAPHQL_SCHEMA_ARTIFACT)
        .ok_or_else(|| {
            schema_error(format!(
                "emit_graphql did not produce {GRAPHQL_SCHEMA_ARTIFACT:?}"
            ))
        })?
        .clone();
    let graphql_name_map_bytes = graphql_package
        .artifacts
        .get(GRAPHQL_NAME_MAP_ARTIFACT)
        .ok_or_else(|| {
            schema_error(format!(
                "emit_graphql did not produce {GRAPHQL_NAME_MAP_ARTIFACT:?}"
            ))
        })?
        .clone();

    let name_map = Value::from(
        Object::new()
            .with("schema_version", 1_u32)
            .with("sanitize_policy", linkml_config.sanitize_policy().to_json())
            .with("element_names", linkml_package.element_names.to_json())
            .with(
                "slot_renames",
                Value::Array(
                    linkml_package
                        .slot_renames
                        .iter()
                        .map(|row| row.to_json())
                        .collect(),
                ),
            )
            .with(
                "slot_diagnostics",
                Value::Array(
                    linkml_package
                        .slot_diagnostics
                        .iter()
                        .map(|row| row.to_json())
                        .collect(),
                ),
            ),
    );
    let mut name_map_bytes = purrdf_lex::json::write_pretty(&name_map).into_bytes();
    name_map_bytes.push(b'\n');
    let losses_bytes = linkml_package.losses.render_json().into_bytes();

    let mut artifacts = BTreeMap::new();
    artifacts.insert(LINKML_PATH.to_string(), linkml_package.yaml.into_bytes());
    artifacts.insert(LINKML_NAME_MAP_PATH.to_string(), name_map_bytes);
    artifacts.insert(LINKML_LOSSES_PATH.to_string(), losses_bytes);
    artifacts.insert(TYPESCRIPT_PATH.to_string(), typescript_bytes);
    artifacts.insert(GRAPHQL_PATH.to_string(), graphql_schema_bytes);
    artifacts.insert(GRAPHQL_NAME_MAP_PATH.to_string(), graphql_name_map_bytes);
    Ok(artifacts)
}

/// The `stage-export-schemas` export-leaf stage.
pub struct SchemasStage {
    consumes: Vec<String>,
}

impl SchemasStage {
    /// Construct the stage. It reads the AUTHORED shape/ontology sources from disk
    /// and consumes the four generated-shape producers so the compiled union folds
    /// THIS run's fresh `generated/shapes/*.ttl` bytes (never the stale committed
    /// files — the stale-disk-fold class). Mirrors [`crate::stages::json_schema::JsonSchemaStage`].
    pub fn new() -> Self {
        Self {
            consumes: crate::stages::shape_union_fresh::producer_consumes(),
        }
    }
}

impl Default for SchemasStage {
    fn default() -> Self {
        Self::new()
    }
}

impl Stage for SchemasStage {
    fn id(&self) -> &str {
        "stage-export-schemas"
    }
    fn consumes(&self) -> &[String] {
        &self.consumes
    }
    fn impl_version(&self) -> &str {
        "schemas.v5-native-linkml-audit"
    }
    fn input_files(&self, root: &Path) -> Result<Vec<std::path::PathBuf>, gmeow_errors::Diag> {
        // The AUTHORED half of the shape union (`shapes/*.ttl` minus lints +
        // `slices/*/*/shapes.ttl`) is the disk source set the emitter reads — declared
        // as cache inputs so an authored-shape edit busts the cache. The GENERATED
        // members are NOT declared: they are product-sourced off the consumed producer
        // stages (the stale-disk-fold-bug-class guard). The value-vocabulary enrichment
        // ALSO reads the ontology ABox (`slices/**/module.ttl`), so those sources bust
        // the cache too — a new vocabulary member must reflow the schema.
        let mut files = crate::stages::shape_union_fresh::authored_shape_files(root)?;
        files.extend(crate::stages::value_vocab::ontology_module_files(root)?);
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
        let artifacts = render_schemas(input.root, &shapes)?;
        Ok(StageOutput::new(StageProduct::from_artifacts(
            self.id(),
            artifacts,
        )))
    }
}

#[path = "schemas.tests.rs"]
#[cfg(test)]
mod tests;
