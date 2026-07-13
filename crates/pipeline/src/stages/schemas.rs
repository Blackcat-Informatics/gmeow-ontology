// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Native schema export leaf: LinkML YAML plus TypeScript and GraphQL developer
//! surfaces.
//!
//! This stage replaces the former lane-only `gmeow_tools.schema_compile`
//! subprocess. It consumes the freshly folded GTS bytes from `stage-gts-sink`,
//! reads them through the Rust GTS reader, builds the lossy LinkML-compatible
//! schema model in Rust, and renders the three committed schema artifacts with no
//! Python and no external LinkML toolkit.
//!
//! The Pydantic surface is NOT rendered here: it is a SHACL-derived,
//! per-slice package ([`crate::stages::pydantic`]) co-derived from the SAME shape
//! compilation as the JSON-Schema stage, not an OWL→LinkML projection.
//!
//! The emitted TypeScript/GraphQL files are native GMEOW developer surfaces, not
//! byte-for-byte clones of LinkML's generators. Their contract is deterministic,
//! structurally useful output over the same lossy model: classes, slots,
//! value-vocabulary enums, bounded XSD integer metadata, and rangeless object
//! properties as `uriorcurie`.

use std::collections::{BTreeMap, BTreeSet};

use purrdf::RdfDataset;
use serde::Serialize;

use crate::node::{Stage, StageInput, StageOutput, StageProduct};
use crate::stages::export::{DEFAULT_SCOPE, FoldView};
use crate::stages::schema_ident::{finish_text, local_name, sanitize_identifier, sanitize_type};

/// The carrier producer this leaf reads (GTS is exit-only; no gts re-parse).
const SNAPSHOT_STAGE: &str = "stage-snapshot";

/// The committed logical paths of the four schema artifacts owned by this stage.
pub const LINKML_PATH: &str = "generated/schemas/gmeow.linkml.yaml";
pub const TYPESCRIPT_PATH: &str = "generated/schemas/gmeow.ts";
pub const GRAPHQL_PATH: &str = "generated/schemas/gmeow.graphql";
pub const SCHEMA_PATHS: [&str; 3] = [LINKML_PATH, TYPESCRIPT_PATH, GRAPHQL_PATH];

const GMEOW: &str = "https://blackcatinformatics.ca/gmeow/";
const RDFS_LITERAL: &str = "http://www.w3.org/2000/01/rdf-schema#Literal";
const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
const XSD_DURATION: &str = "http://www.w3.org/2001/XMLSchema#duration";
const OWL_CLASS: &str = "http://www.w3.org/2002/07/owl#Class";
const OWL_FUNCTIONAL_PROPERTY: &str = "http://www.w3.org/2002/07/owl#FunctionalProperty";
const OWL_OBJECT_PROPERTY: &str = "http://www.w3.org/2002/07/owl#ObjectProperty";
const OWL_DATATYPE_PROPERTY: &str = "http://www.w3.org/2002/07/owl#DatatypeProperty";
const OWL_ANNOTATION_PROPERTY: &str = "http://www.w3.org/2002/07/owl#AnnotationProperty";
const RDFS_COMMENT: &str = "http://www.w3.org/2000/01/rdf-schema#comment";
const RDFS_DOMAIN: &str = "http://www.w3.org/2000/01/rdf-schema#domain";
const RDFS_LABEL: &str = "http://www.w3.org/2000/01/rdf-schema#label";
const RDFS_RANGE: &str = "http://www.w3.org/2000/01/rdf-schema#range";
const RDFS_SUBCLASS_OF: &str = "http://www.w3.org/2000/01/rdf-schema#subClassOf";

const DESCRIPTION: &str = "GMEOW developer schema generated from canonical OWL. Lossy by design: restrictions, reification, standpoint, inverseOf, and temporal scope are dropped.";

fn skip_string(v: &str) -> bool {
    v.is_empty()
}
fn skip_option<T>(v: &Option<T>) -> bool {
    v.is_none()
}
fn skip_vec<T>(v: &[T]) -> bool {
    v.is_empty()
}
fn skip_map<K, V>(v: &BTreeMap<K, V>) -> bool {
    v.is_empty()
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
struct LinkmlSchema {
    id: String,
    name: String,
    description: String,
    prefixes: BTreeMap<String, String>,
    imports: Vec<String>,
    default_range: String,
    #[serde(skip_serializing_if = "skip_map")]
    types: BTreeMap<String, LinkmlType>,
    classes: BTreeMap<String, LinkmlClass>,
    slots: BTreeMap<String, LinkmlSlot>,
    #[serde(skip_serializing_if = "skip_map")]
    enums: BTreeMap<String, LinkmlEnum>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
struct LinkmlType {
    uri: String,
    #[serde(rename = "typeof")]
    typeof_: String,
}

#[derive(Debug, Clone, Default, Serialize, PartialEq, Eq)]
struct LinkmlClass {
    class_uri: String,
    #[serde(skip_serializing_if = "skip_string")]
    title: String,
    #[serde(skip_serializing_if = "skip_string")]
    description: String,
    #[serde(skip_serializing_if = "skip_string")]
    is_a: String,
    #[serde(skip_serializing_if = "skip_vec")]
    slots: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, PartialEq, Eq)]
struct LinkmlSlot {
    slot_uri: String,
    #[serde(skip_serializing_if = "skip_string")]
    title: String,
    #[serde(skip_serializing_if = "skip_string")]
    description: String,
    range: String,
    #[serde(skip_serializing_if = "skip_string")]
    domain: String,
    #[serde(skip_serializing_if = "skip_option")]
    minimum_value: Option<i64>,
    #[serde(skip_serializing_if = "skip_option")]
    maximum_value: Option<i64>,
    #[serde(skip_serializing_if = "skip_option")]
    multivalued: Option<bool>,
}

#[derive(Debug, Clone, Default, Serialize, PartialEq, Eq)]
struct LinkmlEnum {
    enum_uri: String,
    permissible_values: BTreeMap<String, PermissibleValue>,
}

#[derive(Debug, Clone, Default, Serialize, PartialEq, Eq)]
struct PermissibleValue {
    meaning: String,
    #[serde(skip_serializing_if = "skip_string")]
    title: String,
    #[serde(skip_serializing_if = "skip_string")]
    description: String,
}

fn schema_error(message: impl Into<String>) -> gmeow_errors::Diag {
    gmeow_errors::Diag::of_kind(crate::error::StageFailed {
        stage: "stage-export-schemas".into(),
        message: message.into(),
    })
}

fn is_gmeow_iri(view: &FoldView<'_>, tid: usize) -> bool {
    view.is_iri(tid) && view.lex(tid).starts_with(GMEOW)
}

fn xsd_to_linkml(iri: &str) -> Option<&'static str> {
    match iri {
        "http://www.w3.org/2001/XMLSchema#string" => Some("string"),
        "http://www.w3.org/2001/XMLSchema#boolean" => Some("boolean"),
        "http://www.w3.org/2001/XMLSchema#integer"
        | "http://www.w3.org/2001/XMLSchema#int"
        | "http://www.w3.org/2001/XMLSchema#long"
        | "http://www.w3.org/2001/XMLSchema#short"
        | "http://www.w3.org/2001/XMLSchema#byte"
        | "http://www.w3.org/2001/XMLSchema#nonNegativeInteger"
        | "http://www.w3.org/2001/XMLSchema#positiveInteger"
        | "http://www.w3.org/2001/XMLSchema#nonPositiveInteger"
        | "http://www.w3.org/2001/XMLSchema#negativeInteger"
        | "http://www.w3.org/2001/XMLSchema#unsignedByte"
        | "http://www.w3.org/2001/XMLSchema#unsignedShort"
        | "http://www.w3.org/2001/XMLSchema#unsignedInt"
        | "http://www.w3.org/2001/XMLSchema#unsignedLong" => Some("integer"),
        "http://www.w3.org/2001/XMLSchema#decimal" => Some("decimal"),
        "http://www.w3.org/2001/XMLSchema#float" => Some("float"),
        "http://www.w3.org/2001/XMLSchema#double" => Some("double"),
        "http://www.w3.org/2001/XMLSchema#dateTime" => Some("datetime"),
        "http://www.w3.org/2001/XMLSchema#date" => Some("date"),
        "http://www.w3.org/2001/XMLSchema#time" => Some("time"),
        "http://www.w3.org/2001/XMLSchema#duration" => Some("duration"),
        "http://www.w3.org/2001/XMLSchema#anyURI" => Some("uri"),
        RDFS_LITERAL => Some("string"),
        _ => None,
    }
}

fn xsd_integer_bounds(iri: &str) -> Option<(Option<i64>, Option<i64>)> {
    match iri {
        "http://www.w3.org/2001/XMLSchema#nonNegativeInteger" => Some((Some(0), None)),
        "http://www.w3.org/2001/XMLSchema#positiveInteger" => Some((Some(1), None)),
        "http://www.w3.org/2001/XMLSchema#nonPositiveInteger" => Some((None, Some(0))),
        "http://www.w3.org/2001/XMLSchema#negativeInteger" => Some((None, Some(-1))),
        "http://www.w3.org/2001/XMLSchema#byte" => Some((Some(-128), Some(127))),
        "http://www.w3.org/2001/XMLSchema#unsignedByte" => Some((Some(0), Some(255))),
        "http://www.w3.org/2001/XMLSchema#unsignedShort" => Some((Some(0), Some(65_535))),
        "http://www.w3.org/2001/XMLSchema#unsignedInt" => Some((Some(0), Some(4_294_967_295))),
        "http://www.w3.org/2001/XMLSchema#unsignedLong" => Some((Some(0), None)),
        _ => None,
    }
}

fn description(view: &FoldView<'_>, tid: usize) -> String {
    let mut comments: Vec<String> = view
        .objects(tid, RDFS_COMMENT, DEFAULT_SCOPE)
        .into_iter()
        .filter(|&o| view.is_literal(o))
        .map(|o| view.lex(o).to_string())
        .collect();
    comments.sort();
    comments.into_iter().next().unwrap_or_default()
}

fn public_text(view: &FoldView<'_>, tid: usize, p_iri: &str) -> String {
    view.public_text_with_fallback(tid, p_iri).0
}

fn range_for(iri: &str, class_names: &BTreeSet<String>) -> String {
    if let Some(linkml) = xsd_to_linkml(iri) {
        return linkml.to_string();
    }
    let local = local_name(iri);
    if iri.starts_with(GMEOW) && class_names.contains(local) {
        return local.to_string();
    }
    "string".to_string()
}

fn emit_linkml_model(dataset: &RdfDataset) -> LinkmlSchema {
    let view = FoldView::new(dataset);
    let mut schema = LinkmlSchema {
        id: "https://blackcatinformatics.ca/gmeow/linkml".into(),
        name: "gmeow".into(),
        description: DESCRIPTION.into(),
        prefixes: BTreeMap::from([
            ("gmeow".into(), GMEOW.into()),
            ("linkml".into(), "https://w3id.org/linkml/".into()),
        ]),
        imports: vec!["linkml:types".into()],
        default_range: "string".into(),
        types: BTreeMap::from([(
            "duration".into(),
            LinkmlType {
                uri: XSD_DURATION.into(),
                typeof_: "string".into(),
            },
        )]),
        classes: BTreeMap::new(),
        slots: BTreeMap::new(),
        enums: BTreeMap::new(),
    };

    let mut class_names = BTreeSet::new();
    let mut class_iris = BTreeMap::new();
    let mut pending_is_a: BTreeMap<String, String> = BTreeMap::new();
    let mut class_terms = view.subjects_by_type(OWL_CLASS, DEFAULT_SCOPE);
    class_terms.sort_by(|&a, &b| view.lex(a).cmp(view.lex(b)));

    for cls in class_terms {
        if !is_gmeow_iri(&view, cls) {
            continue;
        }
        let iri = view.lex(cls);
        let local = local_name(iri);
        if local.is_empty() {
            continue;
        }
        class_names.insert(local.to_string());
        class_iris.insert(local.to_string(), iri.to_string());

        let mut cls_def = LinkmlClass {
            class_uri: iri.to_string(),
            title: public_text(&view, cls, RDFS_LABEL),
            description: description(&view, cls),
            ..LinkmlClass::default()
        };

        let mut supers: Vec<String> = view
            .objects(cls, RDFS_SUBCLASS_OF, DEFAULT_SCOPE)
            .into_iter()
            .filter(|&o| view.is_iri(o))
            .map(|o| view.lex(o).to_string())
            .collect();
        supers.sort();
        if let Some(chosen) = supers
            .iter()
            .find(|s| s.starts_with(GMEOW) && class_names.contains(local_name(s)))
            .or_else(|| supers.first())
        {
            let super_local = local_name(chosen);
            if super_local != local
                && chosen.starts_with(GMEOW)
                && class_names.contains(super_local)
            {
                cls_def.is_a = super_local.to_string();
            } else if chosen.starts_with(GMEOW) && super_local != local {
                pending_is_a.insert(local.to_string(), super_local.to_string());
            }
        }

        schema.classes.insert(local.to_string(), cls_def);
    }

    for (local, super_local) in pending_is_a {
        if class_names.contains(&super_local)
            && let Some(cls) = schema.classes.get_mut(&local)
        {
            cls.is_a = super_local;
        }
    }

    let functional: BTreeSet<usize> = view
        .subjects_by_type(OWL_FUNCTIONAL_PROPERTY, DEFAULT_SCOPE)
        .into_iter()
        .collect();
    let object_tid = view.tid_of_iri(OWL_OBJECT_PROPERTY);
    let datatype_tid = view.tid_of_iri(OWL_DATATYPE_PROPERTY);
    let mut props = BTreeSet::new();
    for kind in [
        OWL_OBJECT_PROPERTY,
        OWL_DATATYPE_PROPERTY,
        OWL_ANNOTATION_PROPERTY,
    ] {
        props.extend(view.subjects_by_type(kind, DEFAULT_SCOPE));
    }
    let mut props: Vec<usize> = props.into_iter().collect();
    props.sort_by(|&a, &b| view.lex(a).cmp(view.lex(b)));

    for prop in props {
        if !is_gmeow_iri(&view, prop) {
            continue;
        }
        let iri = view.lex(prop);
        let local = local_name(iri);
        if local.is_empty() {
            continue;
        }
        let is_object = object_tid
            .map(|tid| view.has(prop, RDF_TYPE, tid, DEFAULT_SCOPE))
            .unwrap_or(false);
        let is_datatype = datatype_tid
            .map(|tid| view.has(prop, RDF_TYPE, tid, DEFAULT_SCOPE))
            .unwrap_or(false);

        let mut slot = LinkmlSlot {
            slot_uri: iri.to_string(),
            title: public_text(&view, prop, RDFS_LABEL),
            description: description(&view, prop),
            ..LinkmlSlot::default()
        };

        let mut ranges: Vec<String> = view
            .objects(prop, RDFS_RANGE, DEFAULT_SCOPE)
            .into_iter()
            .filter(|&r| view.is_iri(r))
            .map(|r| view.lex(r).to_string())
            .collect();
        ranges.sort();
        if let Some(first) = ranges.first() {
            slot.range = range_for(first, &class_names);
            // An ObjectProperty's range is always a class/IRI, so its LinkML
            // range must be an IRI type. When `range_for` cannot resolve the
            // declared range to a known gmeow class (e.g. `logic:ActionSchema`,
            // `prov:*`) it falls back to the `string` literal type; for an
            // object property that fallback is wrong — project it as
            // `uriorcurie` instead. Datatype properties keep their literal
            // ranges (including a genuine xsd:string).
            if is_object && slot.range == "string" {
                slot.range = "uriorcurie".into();
            }
            if let Some((min, max)) = xsd_integer_bounds(first) {
                slot.minimum_value = min;
                slot.maximum_value = max;
            }
        } else if is_object {
            slot.range = "uriorcurie".into();
        } else {
            slot.range = "string".into();
        }

        let mut domains: Vec<String> = view
            .objects(prop, RDFS_DOMAIN, DEFAULT_SCOPE)
            .into_iter()
            .filter(|&d| view.is_iri(d))
            .map(|d| view.lex(d).to_string())
            .collect();
        domains.sort();
        if let Some(first) = domains.first() {
            let domain_local = local_name(first);
            if first.starts_with(GMEOW) && class_names.contains(domain_local) {
                slot.domain = domain_local.to_string();
            }
        }

        if functional.contains(&prop) {
            slot.multivalued = Some(false);
        } else if is_object || is_datatype {
            slot.multivalued = Some(true);
        }

        schema.slots.insert(local.to_string(), slot);
    }

    // The class→individuals enumeration shared with the JSON-Schema/Pydantic value-
    // vocabulary enums, so both surfaces read the same `gmeow:` individuals off the
    // same store (individuals arrive sorted by lexical IRI).
    let individuals_by_class =
        crate::stages::value_vocab::gmeow_individuals_by_class(&view, &class_iris);

    for (cls_local, inds) in individuals_by_class {
        if inds.is_empty() {
            continue;
        }
        let Some(enum_uri) = class_iris.get(&cls_local) else {
            continue;
        };
        let mut enum_def = LinkmlEnum {
            enum_uri: enum_uri.clone(),
            permissible_values: BTreeMap::new(),
        };
        for ind in inds {
            let ind_iri = view.lex(ind);
            let ind_local = local_name(ind_iri);
            enum_def.permissible_values.insert(
                ind_local.to_string(),
                PermissibleValue {
                    meaning: ind_iri.to_string(),
                    title: public_text(&view, ind, RDFS_LABEL),
                    description: description(&view, ind),
                },
            );
        }
        schema.enums.insert(format!("{cls_local}Enum"), enum_def);
    }

    let slot_domains: Vec<(String, String)> = schema
        .slots
        .iter()
        .filter(|(_, slot)| !slot.domain.is_empty())
        .map(|(slot_name, slot)| (slot_name.clone(), slot.domain.clone()))
        .collect();
    for (slot_name, domain) in slot_domains {
        if let Some(cls) = schema.classes.get_mut(&domain) {
            cls.slots.push(slot_name);
        }
    }
    for cls in schema.classes.values_mut() {
        cls.slots.sort();
        cls.slots.dedup();
    }

    schema
}

fn render_linkml_yaml(schema: &LinkmlSchema) -> Result<Vec<u8>, gmeow_errors::Diag> {
    let mut yaml = serde_yaml::to_string(schema).map_err(|e| schema_error(e.to_string()))?;
    if yaml.starts_with("---\n") {
        yaml = yaml[4..].to_string();
    }
    let text = format!(
        "# GENERATED by gmeow schemas — DO NOT EDIT.\n# https://github.com/Blackcat-Informatics/gmeow-ontology\n\n{yaml}"
    );
    Ok(text.into_bytes())
}

fn ts_string(s: &str) -> String {
    serde_json::to_string(s).unwrap_or_else(|_| "\"\"".to_string())
}

fn ts_type(range: &str, multivalued: bool, schema: &LinkmlSchema) -> String {
    let base = if schema.enums.contains_key(range) || schema.classes.contains_key(range) {
        sanitize_type(range, "GmeowValue")
    } else {
        match range {
            "integer" | "decimal" | "float" | "double" => "number".into(),
            "boolean" => "boolean".into(),
            "datetime" | "date" | "time" | "duration" | "uri" | "uriorcurie" | "string" => {
                "string".into()
            }
            _ => "string".into(),
        }
    };
    if multivalued {
        format!("{base}[]")
    } else {
        base
    }
}

fn graphql_type(range: &str, multivalued: bool, schema: &LinkmlSchema) -> String {
    let base = if schema.enums.contains_key(range) || schema.classes.contains_key(range) {
        sanitize_identifier(range, "GmeowValue")
    } else {
        match range {
            "integer" => "Int".into(),
            "decimal" | "float" | "double" => "Float".into(),
            "boolean" => "Boolean".into(),
            _ => "String".into(),
        }
    };
    if multivalued {
        format!("[{base}!]")
    } else {
        base
    }
}

fn render_typescript(schema: &LinkmlSchema) -> Vec<u8> {
    let mut out = String::new();
    out.push_str("// GENERATED by gmeow schemas - DO NOT EDIT.\n\n");
    for (enum_name, enum_def) in &schema.enums {
        out.push_str(&format!(
            "export enum {} {{\n",
            sanitize_type(enum_name, "GmeowEnum")
        ));
        let mut used = BTreeSet::new();
        for (pv_name, pv) in &enum_def.permissible_values {
            let mut ident = sanitize_identifier(pv_name, "value");
            while !used.insert(ident.clone()) {
                ident.push('_');
            }
            out.push_str(&format!(
                "    {ident} = {},\n",
                ts_string(local_name(&pv.meaning))
            ));
        }
        out.push_str("}\n\n");
    }

    for (class_name, class_def) in &schema.classes {
        let cls = sanitize_type(class_name, "GmeowClass");
        if class_def.is_a.is_empty() {
            out.push_str(&format!("export interface {cls} {{\n"));
        } else {
            out.push_str(&format!(
                "export interface {cls} extends {} {{\n",
                sanitize_type(&class_def.is_a, "GmeowClass")
            ));
        }
        for slot_name in &class_def.slots {
            let Some(slot) = schema.slots.get(slot_name) else {
                continue;
            };
            let field = sanitize_identifier(slot_name, "field");
            let multivalued = slot.multivalued.unwrap_or(false);
            let ty = ts_type(&slot.range, multivalued, schema);
            out.push_str(&format!("    {field}?: {ty},\n"));
        }
        out.push_str("}\n\n");
    }
    finish_text(out)
}

fn render_graphql(schema: &LinkmlSchema) -> Vec<u8> {
    let mut out = String::new();
    out.push_str("# GENERATED by gmeow schemas - DO NOT EDIT.\n\n");
    for (enum_name, enum_def) in &schema.enums {
        out.push_str(&format!(
            "enum {} {{\n",
            sanitize_identifier(enum_name, "GmeowEnum")
        ));
        let mut used = BTreeSet::new();
        for pv_name in enum_def.permissible_values.keys() {
            let mut ident = sanitize_identifier(pv_name, "VALUE");
            while !used.insert(ident.clone()) {
                ident.push('_');
            }
            out.push_str(&format!("  {ident}\n"));
        }
        out.push_str("}\n\n");
    }
    for (class_name, class_def) in &schema.classes {
        out.push_str(&format!(
            "type {} {{\n",
            sanitize_identifier(class_name, "GmeowClass")
        ));
        out.push_str("  id: String\n  iri: String\n");
        for slot_name in &class_def.slots {
            let Some(slot) = schema.slots.get(slot_name) else {
                continue;
            };
            let field = sanitize_identifier(slot_name, "field");
            let multivalued = slot.multivalued.unwrap_or(false);
            let ty = graphql_type(&slot.range, multivalued, schema);
            out.push_str(&format!("  {field}: {ty}\n"));
        }
        out.push_str("}\n\n");
    }
    finish_text(out)
}

pub(crate) fn render_schemas_from_dataset(
    dataset: &RdfDataset,
) -> Result<BTreeMap<String, Vec<u8>>, gmeow_errors::Diag> {
    let schema = emit_linkml_model(dataset);
    let mut artifacts = BTreeMap::new();
    artifacts.insert(LINKML_PATH.to_string(), render_linkml_yaml(&schema)?);
    artifacts.insert(TYPESCRIPT_PATH.to_string(), render_typescript(&schema));
    artifacts.insert(GRAPHQL_PATH.to_string(), render_graphql(&schema));
    Ok(artifacts)
}

/// The `stage-export-schemas` export-leaf stage.
pub struct SchemasStage {
    consumes: Vec<String>,
}

impl SchemasStage {
    pub fn new() -> Self {
        Self {
            consumes: vec![SNAPSHOT_STAGE.to_string()],
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
        "schemas.v3-native"
    }
    fn run(&self, input: StageInput<'_>) -> Result<StageOutput, gmeow_errors::Diag> {
        // Read THIS run's carrier dataset directly off the snapshot product's bundle
        //  — GTS is exit-only, never re-parsed by an export leaf.
        let dataset = crate::stages::carrier::snapshot_dataset(input.upstream)?;
        Ok(StageOutput::new(StageProduct::from_artifacts(
            self.id(),
            render_schemas_from_dataset(dataset.as_ref())?,
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repo_root() -> std::path::PathBuf {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .canonicalize()
            .unwrap()
    }

    fn committed_dataset() -> std::sync::Arc<RdfDataset> {
        let root = repo_root();
        let gts =
            std::fs::read(root.join("generated/dist/gmeow.gts")).expect("read committed gmeow.gts");
        purrdf::import_gts_events(&gts)
            .expect("import committed gmeow.gts")
            .dataset
    }

    #[test]
    fn native_schema_stage_emits_all_artifacts_deterministically() {
        let dataset = committed_dataset();
        let first = render_schemas_from_dataset(dataset.as_ref()).expect("render schemas");
        let second = render_schemas_from_dataset(dataset.as_ref()).expect("render schemas again");
        assert_eq!(first, second, "native schema output is non-deterministic");
        for path in SCHEMA_PATHS {
            assert!(first.contains_key(path), "missing {path}");
            assert!(!first[path].is_empty(), "{path} is empty");
        }
    }

    #[test]
    fn native_schema_preserves_known_slot_semantics() {
        let dataset = committed_dataset();
        let schema = emit_linkml_model(dataset.as_ref());
        let pixel_width = schema.slots.get("pixelWidth").expect("pixelWidth slot");
        assert_eq!(pixel_width.range, "integer");
        assert_eq!(pixel_width.minimum_value, Some(0));

        let resent_date = schema.slots.get("resentDate").expect("resentDate slot");
        let resent_message_id = schema
            .slots
            .get("resentMessageId")
            .expect("resentMessageId slot");
        assert_eq!(resent_date.multivalued, Some(true));
        assert_eq!(resent_message_id.multivalued, Some(true));

        let ts = String::from_utf8(render_typescript(&schema)).expect("utf8 TS");
        assert!(ts.contains("pixelWidth?: number,"));
    }

    #[test]
    fn object_property_with_non_gmeow_range_projects_uriorcurie() {
        // An ObjectProperty whose declared range is neither an xsd type nor a
        // known gmeow class (here `logic:ActionSchema`) must still project as an
        // IRI (`uriorcurie`), never the `string` literal fallback. A datatype
        // property with an xsd:string range must remain `string`.
        let ttl = concat!(
            "@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n",
            "@prefix owl: <http://www.w3.org/2002/07/owl#> .\n",
            "@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .\n",
            "@prefix logic: <https://blackcatinformatics.ca/gmeow/logic/> .\n",
            "@prefix xsd: <http://www.w3.org/2001/XMLSchema#> .\n",
            "gmeow:usedCapability a owl:ObjectProperty ;\n",
            "    rdfs:range logic:ActionSchema .\n",
            "gmeow:label a owl:DatatypeProperty ;\n",
            "    rdfs:range xsd:string .\n",
        );
        let dataset = purrdf::parse_dataset(
            ttl.as_bytes(),
            purrdf::NativeRdfFormat::Turtle.media_type(),
            None,
        )
        .expect("parse synthetic turtle");
        let schema = emit_linkml_model(dataset.as_ref());

        let used_capability = schema
            .slots
            .get("usedCapability")
            .expect("usedCapability slot");
        assert_eq!(
            used_capability.range, "uriorcurie",
            "ObjectProperty with a non-gmeow range must project to uriorcurie, not string",
        );

        let label = schema.slots.get("label").expect("label slot");
        assert_eq!(
            label.range, "string",
            "DatatypeProperty with an xsd:string range must remain string",
        );
    }
}
