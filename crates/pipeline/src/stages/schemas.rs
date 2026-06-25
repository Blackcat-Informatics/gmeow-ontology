// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Native schema export leaf (#941): LinkML YAML plus Python, TypeScript, and
//! GraphQL developer surfaces.
//!
//! This stage replaces the former lane-only `gmeow_tools.schema_compile`
//! subprocess. It consumes the freshly folded GTS bytes from `stage-gts-sink`,
//! reads them through the Rust GTS reader, builds the lossy LinkML-compatible
//! schema model in Rust, and renders all four committed schema artifacts with no
//! Python and no external LinkML toolkit.
//!
//! The emitted Pydantic/TypeScript/GraphQL files are native GMEOW developer
//! surfaces, not byte-for-byte clones of LinkML's Python generators. Their
//! contract is deterministic, structurally useful output over the same lossy
//! model: classes, slots, value-vocabulary enums, bounded XSD integer metadata,
//! and rangeless object properties as `uriorcurie`.

use std::collections::{BTreeMap, BTreeSet};

use gmeow_gts::model::Graph;
use serde::Serialize;

use crate::error::PipelineError;
use crate::node::{Stage, StageInput, StageKind, StageOutput, StageProduct};
use crate::stages::export::{FoldView, DEFAULT_SCOPE};

const GTS_PATH: &str = "generated/dist/gmeow.gts";
const SINK_STAGE: &str = "stage-gts-sink";

/// The committed logical paths of the four schema artifacts owned by this stage.
pub const LINKML_PATH: &str = "generated/schemas/gmeow.linkml.yaml";
pub const PYDANTIC_PATH: &str = "generated/schemas/gmeow.py";
pub const TYPESCRIPT_PATH: &str = "generated/schemas/gmeow.ts";
pub const GRAPHQL_PATH: &str = "generated/schemas/gmeow.graphql";
pub const SCHEMA_PATHS: [&str; 4] = [LINKML_PATH, PYDANTIC_PATH, TYPESCRIPT_PATH, GRAPHQL_PATH];

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

fn schema_error(message: impl Into<String>) -> PipelineError {
    PipelineError::Stage {
        stage: "stage-export-schemas".into(),
        message: message.into(),
    }
}

fn local_name(iri: &str) -> &str {
    iri.rsplit(['#', '/']).next().unwrap_or(iri)
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

fn emit_linkml_model(graph: &Graph) -> LinkmlSchema {
    let view = FoldView::new(graph);
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
        if class_names.contains(&super_local) {
            if let Some(cls) = schema.classes.get_mut(&local) {
                cls.is_a = super_local;
            }
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

    let mut individuals_by_class: BTreeMap<String, BTreeSet<usize>> = BTreeMap::new();
    for (cls_local, cls_iri) in &class_iris {
        for ind in view.subjects_by_type(cls_iri, DEFAULT_SCOPE) {
            if is_gmeow_iri(&view, ind) {
                individuals_by_class
                    .entry(cls_local.clone())
                    .or_default()
                    .insert(ind);
            }
        }
    }

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
        let mut inds: Vec<usize> = inds.into_iter().collect();
        inds.sort_by(|&a, &b| view.lex(a).cmp(view.lex(b)));
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

fn render_linkml_yaml(schema: &LinkmlSchema) -> Result<Vec<u8>, PipelineError> {
    let mut yaml = serde_yaml::to_string(schema).map_err(|e| schema_error(e.to_string()))?;
    if yaml.starts_with("---\n") {
        yaml = yaml[4..].to_string();
    }
    let text = format!(
        "# GENERATED by gmeow schemas — DO NOT EDIT.\n# https://github.com/Blackcat-Informatics/gmeow-ontology\n\n{yaml}"
    );
    Ok(text.into_bytes())
}

fn finish_text(mut out: String) -> Vec<u8> {
    while out.ends_with("\n\n") {
        out.pop();
    }
    if !out.ends_with('\n') {
        out.push('\n');
    }
    out.into_bytes()
}

fn py_string(s: &str) -> String {
    format!("{s:?}")
}

fn ts_string(s: &str) -> String {
    serde_json::to_string(s).unwrap_or_else(|_| "\"\"".to_string())
}

fn sanitize_identifier(raw: &str, fallback: &str) -> String {
    let mut out = String::new();
    for (i, ch) in raw.chars().enumerate() {
        let valid = ch == '_' || ch.is_ascii_alphanumeric();
        if valid {
            if i == 0 && ch.is_ascii_digit() {
                out.push('_');
            }
            out.push(ch);
        } else {
            out.push('_');
        }
    }
    while out.contains("__") {
        out = out.replace("__", "_");
    }
    out = out.trim_matches('_').to_string();
    if out.is_empty() {
        fallback.to_string()
    } else if matches!(
        out.as_str(),
        "class" | "def" | "enum" | "from" | "import" | "None" | "True" | "False" | "type"
    ) {
        format!("{out}_")
    } else {
        out
    }
}

fn sanitize_type(raw: &str, fallback: &str) -> String {
    let ident = sanitize_identifier(raw, fallback);
    let mut chars = ident.chars();
    match chars.next() {
        Some(first) => {
            let mut out = String::with_capacity(ident.len());
            out.push(first.to_ascii_uppercase());
            out.extend(chars);
            out
        }
        None => fallback.to_string(),
    }
}

fn class_render_order(classes: &BTreeMap<String, LinkmlClass>) -> Vec<String> {
    fn visit(
        name: &str,
        classes: &BTreeMap<String, LinkmlClass>,
        temporary: &mut BTreeSet<String>,
        permanent: &mut BTreeSet<String>,
        out: &mut Vec<String>,
    ) {
        if permanent.contains(name) || temporary.contains(name) {
            return;
        }
        temporary.insert(name.to_string());
        if let Some(class_def) = classes.get(name) {
            if !class_def.is_a.is_empty() && classes.contains_key(&class_def.is_a) {
                visit(&class_def.is_a, classes, temporary, permanent, out);
            }
        }
        temporary.remove(name);
        permanent.insert(name.to_string());
        out.push(name.to_string());
    }

    let mut out = Vec::with_capacity(classes.len());
    let mut temporary = BTreeSet::new();
    let mut permanent = BTreeSet::new();
    for name in classes.keys() {
        visit(name, classes, &mut temporary, &mut permanent, &mut out);
    }
    out
}

fn py_type(range: &str, multivalued: bool, schema: &LinkmlSchema) -> String {
    let base = if schema.enums.contains_key(range) || schema.classes.contains_key(range) {
        sanitize_type(range, "GmeowValue")
    } else {
        match range {
            "integer" => "int".into(),
            "decimal" | "float" | "double" => "float".into(),
            "boolean" => "bool".into(),
            "datetime" | "date" | "time" | "duration" | "uri" | "uriorcurie" | "string" => {
                "str".into()
            }
            _ => "str".into(),
        }
    };
    if multivalued {
        format!("list[{base}]")
    } else {
        base
    }
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

fn render_pydantic(schema: &LinkmlSchema) -> Vec<u8> {
    let mut out = String::new();
    out.push_str("from __future__ import annotations\n\n");
    out.push_str("from enum import Enum\n");
    out.push_str("from typing import ClassVar\n\n");
    out.push_str("from pydantic import BaseModel, ConfigDict, Field\n\n\n");
    out.push_str("# GENERATED by gmeow schemas - DO NOT EDIT.\n\n");
    out.push_str("metamodel_version = \"native\"\n");
    out.push_str("version = \"None\"\n\n\n");
    out.push_str("class ConfiguredBaseModel(BaseModel):\n");
    out.push_str("    model_config = ConfigDict(\n");
    out.push_str("        serialize_by_alias=True,\n");
    out.push_str("        validate_by_name=True,\n");
    out.push_str("        validate_assignment=True,\n");
    out.push_str("        validate_default=True,\n");
    out.push_str("        extra=\"forbid\",\n");
    out.push_str("        arbitrary_types_allowed=True,\n");
    out.push_str("        use_enum_values=True,\n");
    out.push_str("    )\n\n\n");

    for (enum_name, enum_def) in &schema.enums {
        out.push_str(&format!(
            "class {}(str, Enum):\n",
            sanitize_type(enum_name, "GmeowEnum")
        ));
        if enum_def.permissible_values.is_empty() {
            out.push_str("    pass\n\n\n");
            continue;
        }
        let mut used = BTreeSet::new();
        for (pv_name, pv) in &enum_def.permissible_values {
            let mut ident = sanitize_identifier(pv_name, "value");
            while !used.insert(ident.clone()) {
                ident.push('_');
            }
            out.push_str(&format!(
                "    {ident} = {}\n",
                py_string(local_name(&pv.meaning))
            ));
        }
        out.push_str("\n\n");
    }

    for class_name in class_render_order(&schema.classes) {
        let class_def = &schema.classes[&class_name];
        let cls = sanitize_type(&class_name, "GmeowClass");
        let parent = if class_def.is_a.is_empty() {
            "ConfiguredBaseModel".to_string()
        } else {
            sanitize_type(&class_def.is_a, "ConfiguredBaseModel")
        };
        out.push_str(&format!("class {cls}({parent}):\n"));
        out.push_str(&format!(
            "    class_uri: ClassVar[str] = {}\n",
            py_string(&class_def.class_uri)
        ));
        if !class_def.is_a.is_empty() {
            out.push_str(&format!(
                "    is_a: ClassVar[str] = {}\n",
                py_string(&class_def.is_a)
            ));
        }
        if class_def.slots.is_empty() {
            out.push_str("    pass\n\n\n");
            continue;
        }
        let mut used = BTreeSet::new();
        for slot_name in &class_def.slots {
            let Some(slot) = schema.slots.get(slot_name) else {
                continue;
            };
            let mut field = sanitize_identifier(slot_name, "field");
            while !used.insert(field.clone()) {
                field.push('_');
            }
            let multivalued = slot.multivalued.unwrap_or(false);
            let ty = py_type(&slot.range, multivalued, schema);
            let mut args = vec!["default=None".to_string()];
            if field != *slot_name {
                args.push(format!("alias={}", py_string(slot_name)));
            }
            if !slot.description.is_empty() {
                args.push(format!("description={}", py_string(&slot.description)));
            }
            out.push_str(&format!(
                "    {field}: {ty} | None = Field({})\n",
                args.join(", ")
            ));
        }
        out.push_str("\n\n");
    }

    finish_text(out)
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

pub(crate) fn render_schemas_from_graph(
    graph: &Graph,
) -> Result<BTreeMap<String, Vec<u8>>, PipelineError> {
    let schema = emit_linkml_model(graph);
    let mut artifacts = BTreeMap::new();
    artifacts.insert(LINKML_PATH.to_string(), render_linkml_yaml(&schema)?);
    artifacts.insert(PYDANTIC_PATH.to_string(), render_pydantic(&schema));
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
            consumes: vec![SINK_STAGE.to_string()],
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
    fn kind(&self) -> StageKind {
        StageKind::ExportLeaf
    }
    fn consumes(&self) -> &[String] {
        &self.consumes
    }
    fn impl_version(&self) -> &str {
        "schemas.v3-native"
    }
    fn run(&self, input: StageInput<'_>) -> Result<StageOutput, PipelineError> {
        let sink = input
            .upstream
            .get(SINK_STAGE)
            .ok_or_else(|| schema_error(format!("missing upstream product {SINK_STAGE}")))?;
        let gts = sink
            .artifact(GTS_PATH)
            .ok_or_else(|| schema_error(format!("{SINK_STAGE} did not emit {GTS_PATH}")))?;
        let graph = gmeow_rdf::gts::read_graph(gts, true)
            .map_err(|e| schema_error(format!("could not read upstream GTS graph: {e}")))?;
        Ok(StageOutput {
            product: StageProduct::from_artifacts(self.id(), render_schemas_from_graph(&graph)?),
        })
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

    fn committed_graph() -> Graph {
        let root = repo_root();
        let gts = std::fs::read(root.join(GTS_PATH)).expect("read committed gmeow.gts");
        gmeow_rdf::gts::read_graph(&gts, true).expect("read committed graph")
    }

    #[test]
    fn native_schema_stage_emits_all_artifacts_deterministically() {
        let graph = committed_graph();
        let first = render_schemas_from_graph(&graph).expect("render schemas");
        let second = render_schemas_from_graph(&graph).expect("render schemas again");
        assert_eq!(first, second, "native schema output is non-deterministic");
        for path in SCHEMA_PATHS {
            assert!(first.contains_key(path), "missing {path}");
            assert!(!first[path].is_empty(), "{path} is empty");
        }
    }

    #[test]
    fn native_schema_preserves_known_slot_semantics() {
        let graph = committed_graph();
        let schema = emit_linkml_model(&graph);
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
    fn pydantic_classes_inherit_declared_parent_classes() {
        let graph = committed_graph();
        let schema = emit_linkml_model(&graph);
        let (child_name, child_def) = schema
            .classes
            .iter()
            .find(|(_, class_def)| !class_def.is_a.is_empty())
            .expect("at least one class with is_a");
        let child = sanitize_type(child_name, "GmeowClass");
        let parent = sanitize_type(&child_def.is_a, "GmeowClass");

        let py = String::from_utf8(render_pydantic(&schema)).expect("utf8 Python");
        let parent_decl = format!("class {parent}(");
        let child_decl = format!("class {child}({parent}):");
        let parent_pos = py.find(&parent_decl).expect("parent class declaration");
        let child_pos = py.find(&child_decl).expect("child inherits parent");

        assert!(
            parent_pos < child_pos,
            "parent class must render before child class"
        );
    }
}
