// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! SHACL → JSON Schema (draft 2020-12) + OpenAPI 3.1 emitter (#700).
//!
//! Compiles a parsed [`Shapes`] graph into a closed-world JSON Schema describing
//! the JSON-LD projection of GMEOW instance data (see [`crate::instance`]). The
//! emitter and the projector share ONE CURIE-compaction / value-shaping
//! convention so a projected node always validates against the schema this
//! module produces (Task 6 proves the round trip over every slice example).
//!
//! # Conventions (must stay in lock-step with `instance.rs`)
//!
//! * **IRI compaction** — [`compact_iri`] maps a known namespace prefix to
//!   `prefix:LocalName`; otherwise the full IRI is kept verbatim.
//! * **Object (node) value** — a JSON object `{"@id": "<compacted-iri>"}`.
//! * **Typed literal value** — `{"@value": "<lexical>", "@type": "<compacted-datatype>"}`.
//!   For numeric / boolean datatypes the projector MAY also emit a bare JSON
//!   scalar, so the value schema accepts BOTH the scalar and the object form
//!   (`anyOf`).
//! * **Language-tagged literal** — `{"@value": "<lexical>", "@language": "<tag>"}`.
//! * **Plain string** — a bare JSON string.
//! * **Statement metadata** — an optional `@annotation` key on any property value
//!   object, referencing `#/$defs/Annotation` (RDF-1.2 reifier metadata, #699).
//!
//! # SPARQL losses
//!
//! `sh:sparql` / `sh:SPARQLTarget` constraints have no JSON Schema equivalent.
//! They are never silently skipped: each one is dropped, recorded as a
//! [`LossRecord`], and annotated with a `$comment` on the affected schema.

use oxigraph::model::Term;
use serde_json::{json, Map, Value};

use crate::shapes::{Constraint, NodeKindValue, Path, Shape, Shapes, Target};

/// The GMEOW namespace (matches `crate::model::gmeow`).
const GMEOW_NS: &str = "https://blackcatinformatics.ca/gmeow/";
const XSD_NS: &str = "http://www.w3.org/2001/XMLSchema#";
const RDF_NS: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#";
const RDFS_NS: &str = "http://www.w3.org/2000/01/rdf-schema#";
const OWL_NS: &str = "http://www.w3.org/2002/07/owl#";
const SH_NS: &str = "http://www.w3.org/ns/shacl#";

/// The well-known prefix map, highest-specificity-first so e.g. the gmeow
/// namespace is matched before any shorter prefix could.  `(prefix, namespace)`.
pub const PREFIXES: &[(&str, &str)] = &[
    ("gmeow", GMEOW_NS),
    ("xsd", XSD_NS),
    ("rdf", RDF_NS),
    ("rdfs", RDFS_NS),
    ("owl", OWL_NS),
    ("sh", SH_NS),
];

/// Compact an IRI to `prefix:LocalName` when it begins with a known namespace;
/// otherwise return the full IRI unchanged.
///
/// This is the single shared compaction helper used by BOTH the schema emitter
/// and the instance projector ([`crate::instance`]).
pub fn compact_iri(iri: &str) -> String {
    for (prefix, ns) in PREFIXES {
        if let Some(local) = iri.strip_prefix(ns) {
            return format!("{prefix}:{local}");
        }
    }
    iri.to_owned()
}

/// The bare local name of an IRI: the substring after the last `#` or `/`.
pub fn local_name(iri: &str) -> String {
    let after_hash = iri.rsplit('#').next().unwrap_or(iri);
    // `rsplit('#')` returns the whole string when there is no `#`, so split on
    // `/` over that remainder.
    let local = after_hash.rsplit('/').next().unwrap_or(after_hash);
    local.to_owned()
}

/// Whether an IRI is in the GMEOW namespace (object refs to gmeow classes get a
/// `$ref`; external classes get a permissive node-ref / string).
fn is_gmeow(iri: &str) -> bool {
    iri.starts_with(GMEOW_NS)
}

/// A single un-mappable SHACL construct, recorded rather than silently dropped.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LossRecord {
    /// The SHACL construct that could not be mapped (e.g. `"sh:sparql"`).
    pub construct: String,
    /// The IRI (or blank-node id) of the shape that carried it.
    pub shape_iri: String,
    /// A human-readable reason for the drop.
    pub reason: String,
}

/// The compiled artifacts: a JSON Schema document, an OpenAPI document, and the
/// list of constructs that could not be expressed.
#[derive(Debug, Clone)]
pub struct CompiledSchema {
    /// The JSON Schema (draft 2020-12), pretty-printed with a trailing newline.
    pub schema_json: String,
    /// The OpenAPI 3.1 document embedding the same `$defs`, same convention.
    pub openapi_json: String,
    /// Every dropped, un-mappable construct (never silently skipped).
    pub losses: Vec<LossRecord>,
}

// ── Compilation context ──────────────────────────────────────────────────────

/// Accumulates losses while compiling so every emitter helper can record one.
struct Ctx {
    losses: Vec<LossRecord>,
}

impl Ctx {
    fn new() -> Self {
        Self { losses: Vec::new() }
    }

    fn record(&mut self, construct: &str, shape_iri: &str, reason: &str) {
        self.losses.push(LossRecord {
            construct: construct.to_owned(),
            shape_iri: shape_iri.to_owned(),
            reason: reason.to_owned(),
        });
    }
}

// ── Public entry points ──────────────────────────────────────────────────────

/// Compile a parsed [`Shapes`] graph into a closed-world JSON Schema + OpenAPI.
pub fn compile(shapes: &Shapes) -> CompiledSchema {
    let mut ctx = Ctx::new();

    // Build $defs: one entry per `sh:targetClass` of every active node shape,
    // keyed by the class local name; the body is the shape compiled as an object
    // schema.  Multiple target classes on one shape reuse the same body.
    let mut defs: Map<String, Value> = Map::new();
    for shape in &shapes.node_shapes {
        if shape.deactivated {
            continue;
        }
        let body = compile_object_schema(shape, &mut ctx);
        for target in &shape.targets {
            if let Target::Class(c) = target {
                let name = local_name(c.as_str());
                // First writer wins for a given class name; bodies are identical
                // per shape so this only matters if two shapes target the same
                // class (last one would otherwise clobber). Keep deterministic by
                // not overwriting an existing identical-by-construction entry.
                defs.entry(name).or_insert_with(|| body.clone());
            }
        }
    }

    // The shared statement-metadata fragment (#699).
    defs.insert("Annotation".to_owned(), annotation_def());

    let class_names: Vec<String> = defs
        .keys()
        .filter(|k| *k != "Annotation")
        .cloned()
        .collect();
    // `class_names` is already sorted because `defs` is a BTree-ordered Map iter.

    let schema = root_schema(&defs, &class_names);
    let openapi = openapi_doc(&defs);

    CompiledSchema {
        schema_json: to_pretty(&schema),
        openapi_json: to_pretty(&openapi),
        losses: ctx.losses,
    }
}

// ── Root envelope ────────────────────────────────────────────────────────────

/// Build the top-level JSON Schema envelope.
fn root_schema(defs: &Map<String, Value>, class_names: &[String]) -> Value {
    // anyOf branch list (one $ref per class def), sorted by $ref for stability.
    let mut class_refs: Vec<Value> = class_names
        .iter()
        .map(|name| json!({ "$ref": format!("#/$defs/{name}") }))
        .collect();
    class_refs.sort_by_key(ref_key);

    // A single bare node: anyOf over every class def.
    let bare_node = json!({ "anyOf": class_refs.clone() });

    // The @graph envelope object.
    let graph_envelope = json!({
        "type": "object",
        "properties": {
            "@context": true,
            "@graph": {
                "type": "array",
                "items": { "anyOf": class_refs.clone() }
            }
        }
    });

    json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "$id": "https://blackcatinformatics.ca/gmeow/schema/instance.schema.json",
        "title": "GMEOW instance schema (SHACL-derived, closed-world)",
        "$defs": Value::Object(defs.clone()),
        "type": "object",
        "anyOf": [graph_envelope, bare_node],
        "properties": {
            "@context": true,
            "@graph": {
                "type": "array",
                "items": { "anyOf": class_refs }
            }
        }
    })
}

/// Sort key for an `anyOf` branch that is a `$ref` object.
fn ref_key(v: &Value) -> String {
    v.get("$ref")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_owned()
}

/// The OpenAPI 3.1 document embedding the same `$defs` as `components/schemas`.
fn openapi_doc(defs: &Map<String, Value>) -> Value {
    json!({
        "openapi": "3.1.0",
        "info": {
            "title": "GMEOW",
            "version": crate::VERSION
        },
        "paths": {
            "/entities/{id}": {
                "get": {
                    "summary": "Fetch a single GMEOW entity by id",
                    "parameters": [{
                        "name": "id",
                        "in": "path",
                        "required": true,
                        "schema": { "type": "string" }
                    }],
                    "responses": {
                        "200": {
                            "description": "The requested entity as a JSON-LD node.",
                            "content": {
                                "application/ld+json": {
                                    "schema": { "type": "object" }
                                }
                            }
                        }
                    }
                }
            }
        },
        "components": { "schemas": Value::Object(defs.clone()) }
    })
}

// ── The `@annotation` fragment (#699 statement metadata) ─────────────────────

/// The shared `$defs/Annotation` object schema: free-form statement metadata.
///
/// Permissive on purpose — #699 tightens it. Values may be node refs
/// (`{"@id":..}`), scalars, or typed literals (`{"@value":..,"@type":..}`).
fn annotation_def() -> Value {
    json!({
        "type": "object",
        "title": "RDF-1.2 statement metadata (reifier annotation)",
        "description": "Free-form metadata about an asserted triple (e.g. gmeow:accordingTo, gmeow:confidence, gmeow:assertedAt). Permissive; tightened by #699.",
        "additionalProperties": {
            "anyOf": [
                { "type": "string" },
                { "type": "number" },
                { "type": "boolean" },
                node_ref_schema(),
                typed_literal_schema()
            ]
        }
    })
}

/// The JSON-LD node-reference value schema: `{"@id": "<string>"}`.
fn node_ref_schema() -> Value {
    json!({
        "type": "object",
        "properties": { "@id": { "type": "string" } },
        "required": ["@id"]
    })
}

/// The JSON-LD typed-literal value schema: `{"@value":.., "@type":..}`.
fn typed_literal_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "@value": {},
            "@type": { "type": "string" }
        },
        "required": ["@value"]
    })
}

// ── Per-shape object schema ──────────────────────────────────────────────────

/// Compile a single node shape into a JSON Schema object schema (one `$defs`
/// body). Property shapes become `properties`; node-level logical/closed
/// constraints become `allOf`/`anyOf`/`oneOf`/`not`/`additionalProperties`.
fn compile_object_schema(shape: &Shape, ctx: &mut Ctx) -> Value {
    let shape_iri = shape.id.to_string();

    let mut properties: Map<String, Value> = Map::new();
    let mut required: Vec<String> = Vec::new();
    let mut comments: Vec<String> = Vec::new();

    // `@id` and `@type` are always allowed JSON-LD keywords.
    properties.insert("@id".to_owned(), json!({ "type": "string" }));
    properties.insert(
        "@type".to_owned(),
        json!({
            "anyOf": [
                { "type": "string" },
                { "type": "array", "items": { "type": "string" } }
            ]
        }),
    );

    // The optional statement-metadata key on the node itself.
    properties.insert(
        "@annotation".to_owned(),
        json!({ "$ref": "#/$defs/Annotation" }),
    );

    // Track declared property keys for sh:closed → additionalProperties: false.
    let mut declared_keys: Vec<String> = Vec::new();

    for ps in &shape.property_shapes {
        // Inverse paths do not shape outgoing JSON properties: skip but note it.
        let pred = match &ps.path {
            Path::Predicate(p) => p,
            Path::Inverse(_) => {
                comments.push(
                    "an inverse-path property shape was skipped (inverse paths do not constrain outgoing JSON properties)".to_owned(),
                );
                continue;
            }
        };
        let key = compact_iri(pred.as_str());
        declared_keys.push(key.clone());

        let (value_schema, is_required) = compile_property(&ps.constraints, &shape_iri, &key, ctx);
        if is_required {
            required.push(key.clone());
        }
        properties.insert(key, value_schema);
    }

    // ── Node-level constraints ──
    let mut all_of: Vec<Value> = Vec::new();
    let mut any_of: Vec<Value> = Vec::new();
    let mut one_of: Vec<Value> = Vec::new();
    let mut not_schema: Option<Value> = None;
    let mut additional_properties_false = false;
    let mut closed_ignored: Vec<String> = Vec::new();

    for c in &shape.constraints {
        match c {
            Constraint::And(members) => {
                for m in members {
                    all_of.push(compile_object_schema(m, ctx));
                }
            }
            Constraint::Or(members) => {
                for m in members {
                    any_of.push(compile_object_schema(m, ctx));
                }
            }
            Constraint::Xone(members) => {
                for m in members {
                    one_of.push(compile_object_schema(m, ctx));
                }
            }
            Constraint::Node(inner) => {
                all_of.push(compile_object_schema(inner, ctx));
            }
            Constraint::Not(inner) => {
                not_schema = Some(compile_object_schema(inner, ctx));
            }
            Constraint::Closed { ignored } => {
                additional_properties_false = true;
                for n in ignored {
                    closed_ignored.push(compact_iri(n.as_str()));
                }
            }
            Constraint::Sparql { .. } => {
                ctx.record(
                    "sh:sparql",
                    &shape_iri,
                    "SPARQL-AF constraint has no JSON Schema equivalent",
                );
                comments.push(
                    "a node-level sh:sparql constraint was dropped (no JSON Schema equivalent)"
                        .to_owned(),
                );
            }
            // Node-level value constraints (sh:class, sh:nodeKind, …) shape the
            // node identity rather than an object's JSON properties; for the
            // object-schema projection they are not expressed here.
            _ => {}
        }
    }

    // sh:closed: allow the ignored predicates as declared keys too.
    if additional_properties_false {
        for k in &closed_ignored {
            properties
                .entry(k.clone())
                .or_insert_with(|| Value::Bool(true));
        }
    }

    // Assemble.
    let mut obj: Map<String, Value> = Map::new();
    obj.insert("type".to_owned(), json!("object"));

    obj.insert("properties".to_owned(), Value::Object(properties));

    if !required.is_empty() {
        required.sort();
        required.dedup();
        obj.insert(
            "required".to_owned(),
            Value::Array(required.into_iter().map(Value::String).collect()),
        );
    }

    if additional_properties_false {
        obj.insert("additionalProperties".to_owned(), Value::Bool(false));
    }

    if !all_of.is_empty() {
        obj.insert("allOf".to_owned(), Value::Array(all_of));
    }
    if !any_of.is_empty() {
        obj.insert("anyOf".to_owned(), Value::Array(any_of));
    }
    if !one_of.is_empty() {
        obj.insert("oneOf".to_owned(), Value::Array(one_of));
    }
    if let Some(ns) = not_schema {
        obj.insert("not".to_owned(), ns);
    }

    if !comments.is_empty() {
        comments.sort();
        comments.dedup();
        obj.insert("$comment".to_owned(), json!(comments.join("; ")));
    }

    Value::Object(obj)
}

// ── Per-property value schema ────────────────────────────────────────────────

/// Compile one property shape's constraints into `(value_schema, is_required)`.
///
/// `value_schema` already accounts for cardinality: a single value when
/// `sh:maxCount 1`, otherwise an `array` wrapper with `minItems`/`maxItems`.
fn compile_property(
    constraints: &[Constraint],
    shape_iri: &str,
    key: &str,
    ctx: &mut Ctx,
) -> (Value, bool) {
    // The "scalar" value schema (a single value, pre-cardinality).
    let mut value: Map<String, Value> = Map::new();
    // anyOf alternatives accumulated across datatype/class/nodekind constraints.
    let mut alts: Vec<Value> = Vec::new();
    let mut enum_values: Vec<Value> = Vec::new();
    let mut comments: Vec<String> = Vec::new();

    let mut min_count: Option<u64> = None;
    let mut max_count: Option<u64> = None;

    for c in constraints {
        match c {
            Constraint::MinCount(n) => min_count = Some(*n),
            Constraint::MaxCount(n) => max_count = Some(*n),
            Constraint::Datatype(dt) => {
                alts.push(datatype_value_schema(dt.as_str()));
            }
            Constraint::Class(c) => {
                if is_gmeow(c.as_str()) {
                    // Object property: a node ref OR the class $ref.
                    alts.push(node_ref_schema());
                    alts.push(json!({ "$ref": format!("#/$defs/{}", local_name(c.as_str())) }));
                } else {
                    alts.push(json!({
                        "type": "string",
                        "$comment": format!("external class {}", c.as_str())
                    }));
                }
            }
            Constraint::NodeKind(nk) => match nk {
                NodeKindValue::Literal => {
                    alts.push(json!({ "type": "string" }));
                    alts.push(typed_literal_schema());
                }
                NodeKindValue::Iri | NodeKindValue::BlankNode | NodeKindValue::BlankNodeOrIri => {
                    alts.push(node_ref_schema());
                }
                NodeKindValue::IriOrLiteral | NodeKindValue::BlankNodeOrLiteral => {
                    alts.push(node_ref_schema());
                    alts.push(json!({ "type": "string" }));
                    alts.push(typed_literal_schema());
                }
            },
            Constraint::In(terms) => {
                for t in terms {
                    enum_values.push(json!(term_enum_value(t)));
                }
            }
            Constraint::HasValue(v) => {
                value.insert("const".to_owned(), term_const_value(v));
            }
            Constraint::Pattern { regex, .. } => {
                value.insert("pattern".to_owned(), json!(regex));
            }
            Constraint::MinLength(n) => {
                value.insert("minLength".to_owned(), json!(n));
            }
            Constraint::MaxLength(n) => {
                value.insert("maxLength".to_owned(), json!(n));
            }
            Constraint::MinInclusive(t) => {
                insert_numeric(&mut value, "minimum", t, &mut comments);
            }
            Constraint::MaxInclusive(t) => {
                insert_numeric(&mut value, "maximum", t, &mut comments);
            }
            Constraint::MinExclusive(t) => {
                insert_numeric(&mut value, "exclusiveMinimum", t, &mut comments);
            }
            Constraint::MaxExclusive(t) => {
                insert_numeric(&mut value, "exclusiveMaximum", t, &mut comments);
            }
            Constraint::LanguageIn(tags) => {
                alts.push(lang_literal_schema(tags));
            }
            Constraint::Sparql { .. } => {
                ctx.record(
                    "sh:sparql",
                    shape_iri,
                    "SPARQL-AF constraint has no JSON Schema equivalent",
                );
                comments.push(format!(
                    "a sh:sparql constraint on property {key} was dropped (no JSON Schema equivalent)"
                ));
            }
            // Counts handled above; node-shape-only constraints (Closed/And/…)
            // do not appear on a property shape's value schema.
            _ => {}
        }
    }

    if !enum_values.is_empty() {
        enum_values.sort_by_key(|a| a.to_string());
        enum_values.dedup();
        value.insert("enum".to_owned(), Value::Array(enum_values));
    }

    if !alts.is_empty() {
        // Stable order, de-duplicated.
        alts.sort_by_key(|a| a.to_string());
        alts.dedup();
        if alts.len() == 1 {
            // Fold the single alternative into the value map.
            if let Value::Object(only) = alts.remove(0) {
                for (k, v) in only {
                    value.entry(k).or_insert(v);
                }
            }
        } else {
            value.insert("anyOf".to_owned(), Value::Array(alts));
        }
    }

    if !comments.is_empty() {
        comments.sort();
        comments.dedup();
        value.insert("$comment".to_owned(), json!(comments.join("; ")));
    }

    let single = Value::Object(value);

    // Required iff minCount >= 1.
    let is_required = min_count.map(|n| n >= 1).unwrap_or(false);

    // Cardinality wrapping: maxCount==1 → single; else array.
    let schema = if max_count == Some(1) {
        single
    } else {
        let mut arr: Map<String, Value> = Map::new();
        arr.insert("type".to_owned(), json!("array"));
        arr.insert("items".to_owned(), single);
        if let Some(n) = min_count {
            if n > 0 {
                arr.insert("minItems".to_owned(), json!(n));
            }
        }
        if let Some(n) = max_count {
            arr.insert("maxItems".to_owned(), json!(n));
        }
        Value::Object(arr)
    };

    (schema, is_required)
}

/// Insert a numeric bound (`minimum`/`maximum`/…) parsed from a term's lexical
/// form. Non-numeric lexical values are skipped with a `$comment` note.
fn insert_numeric(
    value: &mut Map<String, Value>,
    key: &str,
    term: &Term,
    comments: &mut Vec<String>,
) {
    let lex = term_lexical(term);
    if let Ok(n) = lex.parse::<f64>() {
        if let Some(num) = serde_json::Number::from_f64(n) {
            value.insert(key.to_owned(), Value::Number(num));
            return;
        }
    }
    comments.push(format!(
        "{key} bound on non-numeric value {lex:?} was skipped"
    ));
}

// ── Datatype → JSON type/format mapping ──────────────────────────────────────

/// Map an xsd datatype IRI to a JSON value schema, accepting BOTH the bare
/// scalar form and the JSON-LD `{"@value":..,"@type":..}` typed-literal object.
fn datatype_value_schema(dt_iri: &str) -> Value {
    let scalar = scalar_schema_for_datatype(dt_iri);
    json!({
        "anyOf": [
            scalar,
            typed_literal_schema()
        ]
    })
}

/// The bare-scalar schema for an xsd datatype (no JSON-LD wrapper).
fn scalar_schema_for_datatype(dt_iri: &str) -> Value {
    let local = if let Some(l) = dt_iri.strip_prefix(XSD_NS) {
        l
    } else {
        // Non-xsd datatype: treat the lexical form as a string.
        return json!({ "type": "string" });
    };
    match local {
        "string" | "normalizedString" | "token" | "language" | "Name" | "NCName" => {
            json!({ "type": "string" })
        }
        "boolean" => json!({ "type": "boolean" }),
        "integer" | "int" | "long" | "short" | "byte" | "nonNegativeInteger"
        | "positiveInteger" | "nonPositiveInteger" | "negativeInteger" | "unsignedLong"
        | "unsignedInt" | "unsignedShort" | "unsignedByte" => json!({ "type": "integer" }),
        "decimal" | "double" | "float" => json!({ "type": "number" }),
        "dateTime" | "dateTimeStamp" => json!({ "type": "string", "format": "date-time" }),
        "date" => json!({ "type": "string", "format": "date" }),
        "time" => json!({ "type": "string", "format": "time" }),
        "anyURI" => json!({ "type": "string", "format": "uri" }),
        // Unknown xsd:* → string.
        _ => json!({ "type": "string" }),
    }
}

/// The language-tagged-literal value schema for a `sh:languageIn` tag set.
///
/// Tags use RFC4647 basic-filtering semantics: a value tag matches an entry iff
/// it equals it or is a subtag (`en` matches `en-US`). Expressed as a regex
/// `pattern` on `@language` like `^(en|fr)(-.*)?$`.
fn lang_literal_schema(tags: &[String]) -> Value {
    let mut sorted: Vec<String> = tags.iter().map(|t| regex::escape(t)).collect();
    sorted.sort();
    sorted.dedup();
    let alternation = sorted.join("|");
    let pattern = format!("^({alternation})(-.*)?$");
    json!({
        "type": "object",
        "properties": {
            "@value": { "type": "string" },
            "@language": { "type": "string", "pattern": pattern }
        },
        "required": ["@value", "@language"]
    })
}

// ── Term → JSON value helpers (must match instance.rs) ───────────────────────

/// The lexical form of a term (literal value, IRI string, or blank-node id).
fn term_lexical(term: &Term) -> String {
    match term {
        Term::Literal(lit) => lit.value().to_owned(),
        Term::NamedNode(n) => n.as_str().to_owned(),
        Term::BlankNode(b) => b.as_str().to_owned(),
        other => other.to_string(),
    }
}

/// The `sh:in` enum member value, matching what the projector emits.
///
/// IRIs project as the compacted CURIE/IRI string; literals as their lexical.
fn term_enum_value(term: &Term) -> Value {
    match term {
        Term::NamedNode(n) => Value::String(compact_iri(n.as_str())),
        Term::Literal(lit) => Value::String(lit.value().to_owned()),
        Term::BlankNode(b) => Value::String(b.as_str().to_owned()),
        other => Value::String(other.to_string()),
    }
}

/// The `sh:hasValue` const value (projected form).
fn term_const_value(term: &Term) -> Value {
    match term {
        Term::NamedNode(n) => json!({ "@id": compact_iri(n.as_str()) }),
        Term::Literal(lit) => {
            if let Some(lang) = lit.language() {
                json!({ "@value": lit.value(), "@language": lang })
            } else {
                let dt = lit.datatype();
                if dt.as_str() == format!("{RDF_NS}langString")
                    || dt.as_str() == format!("{XSD_NS}string")
                {
                    Value::String(lit.value().to_owned())
                } else {
                    json!({ "@value": lit.value(), "@type": compact_iri(dt.as_str()) })
                }
            }
        }
        Term::BlankNode(b) => json!({ "@id": format!("_:{}", b.as_str()) }),
        other => Value::String(other.to_string()),
    }
}

// ── Serialization ────────────────────────────────────────────────────────────

/// Pretty-print a JSON value with 2-space indent + a single trailing newline.
///
/// `serde_json::Value` uses a BTreeMap-backed `Map` (no `preserve_order`
/// feature), so object keys serialize in sorted order; arrays were sorted at
/// build time — output is therefore byte-stable run-to-run. UTF-8, LF only.
fn to_pretty(value: &Value) -> String {
    let mut s =
        serde_json::to_string_pretty(value).expect("serde_json::Value never fails to serialize");
    s.push('\n');
    s
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shapes::from_store;
    use oxigraph::io::RdfFormat;
    use oxigraph::store::Store;

    const PREFIXES: &str = r#"
        @prefix sh:    <http://www.w3.org/ns/shacl#> .
        @prefix xsd:   <http://www.w3.org/2001/XMLSchema#> .
        @prefix rdf:   <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .
        @prefix rdfs:  <http://www.w3.org/2000/01/rdf-schema#> .
        @prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
    "#;

    fn compile_ttl(body: &str) -> CompiledSchema {
        let ttl = format!("{PREFIXES}{body}");
        let store = Store::new().unwrap();
        store
            .load_from_reader(RdfFormat::Turtle, ttl.as_bytes())
            .expect("Turtle parse");
        let shapes = from_store(&store).expect("shape parse");
        compile(&shapes)
    }

    fn schema_of(c: &CompiledSchema) -> Value {
        serde_json::from_str(&c.schema_json).expect("schema is valid JSON")
    }

    fn def<'a>(schema: &'a Value, name: &str) -> &'a Value {
        &schema["$defs"][name]
    }

    #[test]
    fn test_curie_compaction_and_local_name() {
        assert_eq!(
            compact_iri("https://blackcatinformatics.ca/gmeow/Person"),
            "gmeow:Person"
        );
        assert_eq!(
            compact_iri("http://www.w3.org/2001/XMLSchema#integer"),
            "xsd:integer"
        );
        assert_eq!(
            compact_iri("http://example.org/Foo"),
            "http://example.org/Foo"
        );
        assert_eq!(
            local_name("https://blackcatinformatics.ca/gmeow/Person"),
            "Person"
        );
        assert_eq!(
            local_name("http://www.w3.org/2001/XMLSchema#integer"),
            "integer"
        );
    }

    #[test]
    fn test_required_from_min_count_and_array_vs_single() {
        let c = compile_ttl(
            r#"
            gmeow:PersonShape a sh:NodeShape ;
                sh:targetClass gmeow:Person ;
                sh:property [ sh:path gmeow:name ; sh:minCount 1 ; sh:maxCount 1 ; sh:datatype xsd:string ] ;
                sh:property [ sh:path gmeow:nickname ; sh:datatype xsd:string ] .
            "#,
        );
        let schema = schema_of(&c);
        let person = def(&schema, "Person");
        // required contains gmeow:name (minCount 1)
        let required = person["required"].as_array().expect("required array");
        assert!(required.iter().any(|v| v == "gmeow:name"));
        // name (maxCount 1) is a single value, NOT an array
        let name = &person["properties"]["gmeow:name"];
        assert_ne!(name["type"], json!("array"), "maxCount 1 → single value");
        // nickname (no maxCount) is an array
        let nickname = &person["properties"]["gmeow:nickname"];
        assert_eq!(nickname["type"], json!("array"), "no maxCount → array");
    }

    #[test]
    fn test_datatype_type_and_format() {
        let c = compile_ttl(
            r#"
            gmeow:EventShape a sh:NodeShape ;
                sh:targetClass gmeow:Event ;
                sh:property [ sh:path gmeow:at ; sh:maxCount 1 ; sh:datatype xsd:dateTime ] ;
                sh:property [ sh:path gmeow:count ; sh:maxCount 1 ; sh:datatype xsd:integer ] .
            "#,
        );
        let schema = schema_of(&c);
        let event = def(&schema, "Event");
        // dateTime → anyOf containing {type:string, format:date-time}
        let at = &event["properties"]["gmeow:at"];
        let at_alts = at["anyOf"].as_array().expect("anyOf");
        assert!(at_alts
            .iter()
            .any(|alt| alt["format"] == json!("date-time")));
        // integer → anyOf containing {type:integer}
        let count = &event["properties"]["gmeow:count"];
        let count_alts = count["anyOf"].as_array().expect("anyOf");
        assert!(count_alts.iter().any(|alt| alt["type"] == json!("integer")));
    }

    #[test]
    fn test_enum_from_sh_in() {
        let c = compile_ttl(
            r#"
            gmeow:ColorShape a sh:NodeShape ;
                sh:targetClass gmeow:Color ;
                sh:property [ sh:path gmeow:value ; sh:maxCount 1 ; sh:in ( "red" "green" "blue" ) ] .
            "#,
        );
        let schema = schema_of(&c);
        let value = &def(&schema, "Color")["properties"]["gmeow:value"];
        let en = value["enum"].as_array().expect("enum array");
        // sorted: blue, green, red
        assert_eq!(en.len(), 3);
        assert!(en.iter().any(|v| v == "red"));
        // Determinism: sorted ascending.
        let strs: Vec<&str> = en.iter().filter_map(|v| v.as_str()).collect();
        let mut sorted = strs.clone();
        sorted.sort_unstable();
        assert_eq!(strs, sorted, "enum must be sorted");
    }

    #[test]
    fn test_pattern() {
        let c = compile_ttl(
            r#"
            gmeow:CodeShape a sh:NodeShape ;
                sh:targetClass gmeow:Code ;
                sh:property [ sh:path gmeow:code ; sh:maxCount 1 ; sh:pattern "^[A-Z]+$" ] .
            "#,
        );
        let schema = schema_of(&c);
        let code = &def(&schema, "Code")["properties"]["gmeow:code"];
        assert_eq!(code["pattern"], json!("^[A-Z]+$"));
    }

    #[test]
    fn test_closed_additional_properties_false() {
        let c = compile_ttl(
            r#"
            gmeow:ClosedShape a sh:NodeShape ;
                sh:targetClass gmeow:Sealed ;
                sh:closed true ;
                sh:ignoredProperties ( rdf:type ) ;
                sh:property [ sh:path gmeow:only ; sh:maxCount 1 ; sh:datatype xsd:string ] .
            "#,
        );
        let schema = schema_of(&c);
        let sealed = def(&schema, "Sealed");
        assert_eq!(sealed["additionalProperties"], json!(false));
        // The single declared property key is present.
        assert!(sealed["properties"]["gmeow:only"].is_object());
    }

    #[test]
    fn test_not_constraint() {
        let c = compile_ttl(
            r#"
            gmeow:NotShape a sh:NodeShape ;
                sh:targetClass gmeow:Thing ;
                sh:not [ sh:nodeKind sh:Literal ] .
            "#,
        );
        let schema = schema_of(&c);
        let thing = def(&schema, "Thing");
        assert!(thing["not"].is_object(), "expected a `not` subschema");
    }

    #[test]
    fn test_sparql_constraint_records_loss_and_comment() {
        let c = compile_ttl(
            r#"
            gmeow:SparqlShape a sh:NodeShape ;
                sh:targetClass gmeow:Guarded ;
                sh:sparql [
                    sh:select "SELECT $this WHERE { $this <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <https://blackcatinformatics.ca/gmeow/Guarded> . }" ;
                ] .
            "#,
        );
        assert!(!c.losses.is_empty(), "sh:sparql must record a LossRecord");
        let loss = &c.losses[0];
        assert_eq!(loss.construct, "sh:sparql");
        assert!(loss.reason.contains("SPARQL"));
        // The affected schema carries a $comment noting the drop.
        let schema = schema_of(&c);
        let guarded = def(&schema, "Guarded");
        assert!(
            guarded["$comment"]
                .as_str()
                .unwrap_or("")
                .contains("sparql"),
            "expected a $comment noting the dropped sh:sparql, got {:?}",
            guarded["$comment"]
        );
    }

    #[test]
    fn test_object_property_uses_ref() {
        let c = compile_ttl(
            r#"
            gmeow:OrgShape a sh:NodeShape ;
                sh:targetClass gmeow:Organization ;
                sh:property [ sh:path gmeow:member ; sh:maxCount 1 ; sh:class gmeow:Person ] .
            gmeow:PersonShape a sh:NodeShape ;
                sh:targetClass gmeow:Person .
            "#,
        );
        let schema = schema_of(&c);
        let member = &def(&schema, "Organization")["properties"]["gmeow:member"];
        // anyOf includes a node ref {"@id":..} and a $ref to #/$defs/Person.
        let alts = member["anyOf"].as_array().expect("anyOf");
        assert!(alts.iter().any(|a| a["$ref"] == json!("#/$defs/Person")));
        assert!(alts.iter().any(|a| a["properties"]["@id"].is_object()));
    }

    #[test]
    fn test_annotation_def_present_and_root_envelope() {
        let c = compile_ttl(
            r#"
            gmeow:PersonShape a sh:NodeShape ;
                sh:targetClass gmeow:Person ;
                sh:property [ sh:path gmeow:name ; sh:datatype xsd:string ] .
            "#,
        );
        let schema = schema_of(&c);
        // $defs/Annotation exists.
        assert!(schema["$defs"]["Annotation"].is_object());
        // Root envelope keys.
        assert_eq!(
            schema["$schema"],
            json!("https://json-schema.org/draft/2020-12/schema")
        );
        assert!(schema["properties"]["@graph"].is_object());
        assert!(schema["anyOf"].is_array(), "root anyOf graph|bare-node");
        // Each node schema carries an @annotation key referencing the fragment.
        let person = def(&schema, "Person");
        assert_eq!(
            person["properties"]["@annotation"]["$ref"],
            json!("#/$defs/Annotation")
        );
    }

    #[test]
    fn test_deactivated_shape_skipped() {
        let c = compile_ttl(
            r#"
            gmeow:GoneShape a sh:NodeShape ;
                sh:targetClass gmeow:Gone ;
                sh:deactivated true ;
                sh:property [ sh:path gmeow:x ; sh:datatype xsd:string ] .
            "#,
        );
        let schema = schema_of(&c);
        assert!(
            schema["$defs"]["Gone"].is_null(),
            "deactivated shape must not produce a $def"
        );
    }

    #[test]
    fn test_openapi_embeds_components_schemas() {
        let c = compile_ttl(
            r#"
            gmeow:PersonShape a sh:NodeShape ;
                sh:targetClass gmeow:Person ;
                sh:property [ sh:path gmeow:name ; sh:datatype xsd:string ] .
            "#,
        );
        let openapi: Value = serde_json::from_str(&c.openapi_json).expect("openapi JSON");
        assert_eq!(openapi["openapi"], json!("3.1.0"));
        assert!(openapi["components"]["schemas"]["Person"].is_object());
        assert!(openapi["paths"]["/entities/{id}"]["get"].is_object());
        // trailing newline convention
        assert!(c.openapi_json.ends_with("}\n"));
    }

    #[test]
    fn test_determinism_byte_stable() {
        let body = r#"
            gmeow:PersonShape a sh:NodeShape ;
                sh:targetClass gmeow:Person ;
                sh:property [ sh:path gmeow:name ; sh:minCount 1 ; sh:datatype xsd:string ] ;
                sh:property [ sh:path gmeow:age ; sh:maxCount 1 ; sh:datatype xsd:integer ] .
        "#;
        let a = compile_ttl(body);
        let b = compile_ttl(body);
        assert_eq!(
            a.schema_json, b.schema_json,
            "schema output must be byte-stable"
        );
        assert_eq!(
            a.openapi_json, b.openapi_json,
            "openapi output must be byte-stable"
        );
        // pretty-printed (2-space) + trailing newline
        assert!(a.schema_json.ends_with("}\n"));
        assert!(a.schema_json.contains("\n  \""), "expected 2-space indent");
    }
}
