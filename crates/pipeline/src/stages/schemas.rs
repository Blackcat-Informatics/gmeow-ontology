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

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use purrdf::shapes::graphql::{
    GRAPHQL_NAME_MAP_PATH as GRAPHQL_NAME_MAP_ARTIFACT,
    GRAPHQL_SCHEMA_PATH as GRAPHQL_SCHEMA_ARTIFACT, GraphqlConfig, emit_graphql,
};
use purrdf::shapes::linkml::{LinkmlConfig, emit_linkml};
use purrdf::shapes::typescript::{
    TYPESCRIPT_DECLARATION_PATH as TYPESCRIPT_DECLARATION_ARTIFACT, TypeScriptConfig,
    emit_typescript,
};

use crate::node::{Stage, StageInput, StageOutput, StageProduct};

/// The committed logical paths of the four schema artifacts owned by this stage.
pub const LINKML_PATH: &str = "generated/schemas/gmeow.linkml.yaml";
pub const TYPESCRIPT_PATH: &str = "generated/schemas/gmeow.ts";
pub const GRAPHQL_PATH: &str = "generated/schemas/gmeow.graphql";
/// Committed logical path of the GraphQL canonical name map (`name-map.json`):
/// the source-field/enum-value → GraphQL-name codec `emit_graphql` also
/// produces. Shipped rather than dropped — a value-vocabulary field/enum name
/// the GraphQL SDL renames is otherwise unrecoverable (no-optionality forbids
/// silently discarding a produced artifact).
pub const GRAPHQL_NAME_MAP_PATH: &str = "generated/schemas/gmeow.graphql.name-map.json";
pub const SCHEMA_PATHS: [&str; 4] = [
    LINKML_PATH,
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
/// A LinkML prefix map narrower than this set hard-fails `emit_linkml` on the
/// first property/class from an unregistered namespace.
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
///
/// `prefixes` is threaded in by the caller (rather than recomputed here) so the
/// SAME [`linkml_prefixes`] map drives both the config and
/// [`sanitize_linkml_property_names`] — they can never drift apart.
fn linkml_config(prefixes: BTreeMap<String, String>) -> Result<LinkmlConfig, gmeow_errors::Diag> {
    LinkmlConfig::new(
        "https://blackcatinformatics.ca/gmeow/linkml",
        "gmeow",
        DESCRIPTION,
        "gmeow",
        prefixes,
    )
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

/// Whether `value` is a valid LinkML 1.11 NCName — the exact predicate
/// `purrdf::shapes::linkml` enforces on every compacted-CURIE slot local part
/// (`purrdf-shapes/src/linkml.rs::is_linkml_identifier`, re-derived here because
/// it is a private helper of that crate).
fn is_linkml_identifier(value: &str) -> bool {
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    (first == '_' || first.is_alphabetic())
        && chars.all(|c| {
            c == '_'
                || c == '-'
                || c == '.'
                || c.is_alphanumeric()
                || matches!(c, '\u{300}'..='\u{36f}' | '\u{203f}'..='\u{2040}' | '\u{b7}')
        })
}

/// Rewrite an NCName-unsafe CURIE local part into a valid LinkML NCName:
/// replace every disallowed character with `_`, then guard the first character
/// (must be `_` or alphabetic).
fn linkml_ncname_local(local: &str) -> String {
    let mut out: String = local
        .chars()
        .map(|c| {
            if c == '_'
                || c == '-'
                || c == '.'
                || c.is_alphanumeric()
                || matches!(c, '\u{300}'..='\u{36f}' | '\u{203f}'..='\u{2040}' | '\u{b7}')
            {
                c
            } else {
                '_'
            }
        })
        .collect();
    if out.is_empty() {
        out.push('_');
    }
    match out.chars().next() {
        Some(c) if c == '_' || c.is_alphabetic() => {}
        _ => out.insert(0, '_'),
    }
    out
}

/// Rename every `$defs/*/properties` key (and its `required` twin) `purrdf`'s
/// LinkML `slot_uri` classifier would reject:
///
/// * a CURIE under a REGISTERED prefix (`prefixes`) whose local part is not a
///   valid NCName — e.g. the openEHR OPT-lifted cardinality helper properties
///   (`gmeow:openehr/bloodpressure/occurrences/at0005`), whose local part
///   embeds `/`-separated archetype path segments. Sanitized in place, keeping
///   the registered prefix.
/// * a property under an UNREGISTERED prefix, or a bare absolute IRI from a
///   namespace `linkml_prefixes()` never declares (e.g. an unregistered
///   `skos:definition` annotation property riding in as the literal
///   `http://www.w3.org/2004/02/skos/core#definition`) — `slot_uri` accepts a
///   CURIE only under a caller-registered prefix and an absolute IRI only
///   under a caller-registered NAMESPACE, so anything else is re-homed under
///   our OWN default `gmeow:` prefix, keyed by the property's trailing
///   local-name segment (the legacy hand-rolled renderer's slot-naming
///   convention).
///
/// `purrdf::shapes::json_schema::compile` keys `$defs` properties by compacted
/// CURIE (or a bare absolute IRI when no declared namespace matches) with NO
/// NCName requirement (valid for JSON Schema/OpenAPI/Pydantic property names),
/// but `purrdf::shapes::linkml::emit_linkml` requires every slot to resolve
/// under a registered prefix with an NCName local part (LinkML slot names
/// double as code-generation identifiers) and hard-fails the ENTIRE document
/// on the first violation — so this pre-pass renames rather than lets the
/// whole LinkML surface go dark. TypeScript/GraphQL need no such pass
/// (arbitrary string property names / an internal field-name codec), so they
/// render from the UNMODIFIED shared compiled schema. Returns the renamed
/// `(def_name, old_property, new_property)` triples for loss reporting — the
/// rename is real information loss (the LinkML slot no longer names the exact
/// source property), never silent.
///
/// **Retirement condition.** This pre-pass exists only because `emit_linkml`
/// hard-fails the whole document on the first non-NCName slot instead of
/// reporting the offending slots. When the emitter reports them — or performs a
/// declared, loss-reported rename itself — this function is deleted rather than
/// kept as a second place that decides LinkML slot naming; the doctrine is that
/// purrdf owns the output formats. The condition is stated rather than a tracker
/// id because the issue-refs policy forbids tracker provenance in authored prose,
/// and a condition stays true after the tracker item is renumbered or superseded.
fn sanitize_linkml_property_names(
    schema: &mut serde_json::Value,
    prefixes: &BTreeMap<String, String>,
) -> Vec<(String, String, String)> {
    let mut renames = Vec::new();
    let Some(defs) = schema
        .get_mut("$defs")
        .and_then(serde_json::Value::as_object_mut)
    else {
        return renames;
    };
    let def_names: Vec<String> = defs.keys().cloned().collect();
    for def_name in def_names {
        let Some(properties) = defs
            .get(&def_name)
            .and_then(|d| d.get("properties"))
            .and_then(serde_json::Value::as_object)
        else {
            continue;
        };
        let mut used: BTreeSet<String> = properties.keys().cloned().collect();
        let mut local_renames: Vec<(String, String)> = Vec::new();
        for key in properties.keys() {
            if key.starts_with('@') {
                continue;
            }
            let mut candidate = if let Some((prefix, local)) = key.split_once(':')
                && prefixes.contains_key(prefix)
            {
                // A CURIE under a prefix purrdf's slot_uri WILL resolve — the only
                // possible rejection is a non-NCName local part.
                if is_linkml_identifier(local) {
                    continue;
                }
                format!("{prefix}:{}", linkml_ncname_local(local))
            } else {
                // Not a CURIE under a registered prefix. purrdf's slot_uri accepts an
                // absolute IRI only when it starts with a REGISTERED namespace; rather
                // than replicate that IRI-vs-namespace matching here (and risk drifting
                // from `linkml_prefixes()` set), always re-home under our own default
                // prefix keyed by the property's trailing local-name segment — safe
                // whether `key` was an absolute IRI (`https://…#definition`), a CURIE
                // under an unregistered prefix, or a bare non-identifier string.
                let local = crate::stages::schema_ident::local_name(key);
                format!("gmeow:{}", linkml_ncname_local(local))
            };
            while used.contains(&candidate) {
                candidate.push('_');
            }
            used.insert(candidate.clone());
            local_renames.push((key.clone(), candidate));
        }
        if local_renames.is_empty() {
            continue;
        }
        let def_obj = defs
            .get_mut(&def_name)
            .and_then(serde_json::Value::as_object_mut)
            .expect("presence checked above");
        if let Some(props_obj) = def_obj
            .get_mut("properties")
            .and_then(serde_json::Value::as_object_mut)
        {
            for (old, new) in &local_renames {
                if let Some(v) = props_obj.remove(old) {
                    props_obj.insert(new.clone(), v);
                }
            }
        }
        if let Some(required) = def_obj
            .get_mut("required")
            .and_then(serde_json::Value::as_array_mut)
        {
            for entry in required.iter_mut() {
                if let serde_json::Value::String(s) = entry
                    && let Some((_, new)) = local_renames.iter().find(|(old, _)| old == s)
                {
                    *s = new.clone();
                }
            }
        }
        for (old, new) in local_renames {
            renames.push((def_name.clone(), old, new));
        }
    }
    renames
}

/// Log every LinkML NCName-safety rename ([`sanitize_linkml_property_names`]) as a
/// single aggregated `tracing` event, mirroring [`report_losses`] so a renamed
/// property is never a silent divergence from the shared compiled schema.
fn report_linkml_renames(renames: &[(String, String, String)]) {
    if renames.is_empty() {
        return;
    }
    let examples: Vec<String> = renames
        .iter()
        .take(5)
        .map(|(def, old, new)| format!("{def}: {old} -> {new}"))
        .collect();
    let suffix = if renames.len() > 5 {
        format!(" (+{} more)", renames.len() - 5)
    } else {
        String::new()
    };
    tracing::info!(
        target: "schemas_loss",
        surface = "linkml",
        construct = "property-name",
        shapes = renames.len(),
        reason = "compacted-CURIE local part is not a valid LinkML NCName",
        examples = %format!("{}{suffix}", examples.join(", ")),
        "renamed a non-NCName-safe property for the LinkML surface",
    );
}

/// Compile the fresh SHACL shape union and render the three `purrdf`-native
/// developer schema surfaces (LinkML YAML, TypeScript declarations, GraphQL SDL
/// + its name map), returning the four committed artifacts.
fn render_schemas(
    root: &Path,
    shapes: &purrdf::shapes::shapes::Shapes,
) -> Result<BTreeMap<String, Vec<u8>>, gmeow_errors::Diag> {
    let compiled = crate::stages::schema_compile::enriched_compiled_schema(root, shapes)?;

    // LinkML alone requires every slot local part to be an NCName; sanitize a
    // PRIVATE copy of the schema for it so TypeScript/GraphQL keep rendering the
    // exact shared `$defs` json-schema/pydantic also compile.
    let mut linkml_schema: serde_json::Value = serde_json::from_str(&compiled.schema_json)
        .map_err(|e| {
            schema_error(format!(
                "parse compiled schema for LinkML NCName sanitization: {e}"
            ))
        })?;
    let prefixes = linkml_prefixes();
    let renames = sanitize_linkml_property_names(&mut linkml_schema, &prefixes);
    report_linkml_renames(&renames);
    let linkml_schema_json = serde_json::to_string(&linkml_schema)
        .map_err(|e| schema_error(format!("serialize LinkML-sanitized compiled schema: {e}")))?;
    let linkml_compiled = purrdf::shapes::json_schema::CompiledSchema {
        schema_json: linkml_schema_json,
        openapi_json: compiled.openapi_json.clone(),
        losses: purrdf::LossLedger::new(),
    };

    let linkml_package = emit_linkml(&linkml_compiled, &linkml_config(prefixes)?)
        .map_err(|e| schema_error(format!("emit_linkml: {e}")))?;
    report_losses("linkml", &linkml_package.losses);

    let typescript_package = emit_typescript(&compiled, &typescript_config()?)
        .map_err(|e| schema_error(format!("emit_typescript: {e}")))?;
    report_losses("typescript", &typescript_package.losses);

    let graphql_package = emit_graphql(&compiled, &graphql_config()?)
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

    let mut artifacts = BTreeMap::new();
    artifacts.insert(LINKML_PATH.to_string(), linkml_package.yaml.into_bytes());
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
        // v4: cut over from the hand-rolled OWL→LinkML model (which read the composed
        // carrier dataset) to purrdf's native SHACL-derived LinkML/TypeScript/GraphQL
        // emitters over the SAME fresh shape union json-schema/pydantic compile.
        "schemas.v4-purrdf"
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
        let artifacts = render_schemas(input.root, &shapes)?;
        Ok(StageOutput::new(StageProduct::from_artifacts(
            self.id(),
            artifacts,
        )))
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

    #[test]
    fn schemas_stage_emits_all_authenticated_artifacts() {
        let stage = SchemasStage::new();
        assert_eq!(stage.id(), "stage-export-schemas");
        let root = repo_root();
        let first = crate::fixture::stage_artifacts(&root, 1, "stage-export-schemas")
            .expect("authenticated developer-schema projection");
        for path in SCHEMA_PATHS {
            assert!(first.contains_key(path), "missing {path}");
            assert!(!first[path].is_empty(), "{path} is empty");
        }
    }

    /// A value-vocabulary enum (e.g. `gmeow:TermStability`'s members) must reach
    /// the emitted LinkML YAML — proving the enrichment
    /// ([`crate::stages::schema_compile::enriched_compiled_schema`]) reached the
    /// developer-surface emitters, not just the JSON-Schema leaf.
    #[test]
    fn value_vocab_enum_reaches_linkml_output() {
        let root = repo_root();
        let artifacts = crate::fixture::stage_artifacts(&root, 1, "stage-export-schemas")
            .expect("authenticated developer-schema projection");
        let linkml_yaml =
            String::from_utf8(artifacts[LINKML_PATH].clone()).expect("linkml yaml is utf8");
        // The `gmeow:TermStability` value vocabulary's seed members are
        // `gmeow:stabilityStable` / `gmeow:stabilityExperimental` /
        // `gmeow:stabilityDeprecated` (slices/core/versions/module.ttl) — assert the
        // enum class name and a real member CURIE both reached the LinkML output.
        assert!(
            linkml_yaml.contains("TermStability") && linkml_yaml.contains("stabilityStable"),
            "expected the TermStability value vocabulary to reach the LinkML output:\n{linkml_yaml}"
        );
    }
}
