// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The `yaml_ld` export leaf (#699): RDF → YAML-LD-star / JSON-LD-star.
//!
//! Emits both the JSON-LD-star lead artifact and a deterministic YAML-LD-star
//! derivative, plus a small serialization-preservation ledger.

use std::collections::BTreeMap;

use gmeow_gts::model::{Graph, Term, TermKind};
use oxigraph::model::{Dataset, GraphName, NamedNode, NamedOrBlankNode, Quad, Term as OxTerm};
use oxigraph::store::Store;
use serde_json::Value;

use crate::error::PipelineError;
use crate::node::{Stage, StageInput, StageKind, StageOutput, StageProduct};

/// Logical path of the JSON-LD-star artifact emitted by this stage.
pub const JSON_LD_PATH: &str = "dist/gmeow.jsonld";
/// Logical path of the YAML-LD-star artifact emitted by this stage.
pub const YAML_LD_PATH: &str = "dist/gmeow.yamlld";
/// Logical path of the serialization-preservation ledger.
pub const PRESERVATION_PATH: &str = "generated/metadata/preservation.json";

const GMEOW_NAMESPACE: &str = "https://blackcatinformatics.ca/gmeow/";
const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
/// Schema reference for the YAML-LD language-server header when the output is
/// consumed from the bundled `gmeow.gts` snapshot. The schema is shipped as
/// `schemas-archive/gmeow.schema.json` (#700), so a bare member name resolves
/// inside the bundle.
const BUNDLED_SCHEMA_REF: &str = "gmeow.schema.json";

/// RDF 1.2 reifier predicate.
pub const RDF_REIFIES: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#reifies";
/// GMEOW statement-metadata class.
pub const GMEOW_STATEMENT_METADATA: &str = "https://blackcatinformatics.ca/gmeow/StatementMetadata";
/// GMEOW quoted subject property.
pub const GMEOW_QSUBJECT: &str = "https://blackcatinformatics.ca/gmeow/qSubject";
/// GMEOW quoted predicate property.
pub const GMEOW_QPREDICATE: &str = "https://blackcatinformatics.ca/gmeow/qPredicate";
/// GMEOW quoted object property (IRI / blank-node objects).
pub const GMEOW_QOBJECT: &str = "https://blackcatinformatics.ca/gmeow/qObject";
/// GMEOW quoted literal object property.
pub const GMEOW_QOBJECTLITERAL: &str = "https://blackcatinformatics.ca/gmeow/qObjectLiteral";

// Longest-namespace-first prefix table (mirrors `src/gmeow_tools/config.py`).
include!("lpg_prefixes.rs");

/// Default-graph and named-graph node maps returned by [`build_graphs`].
type GraphNodes = (BTreeMap<String, Value>, BTreeMap<String, Value>);
/// Reifier lookup: base triple (s,p,o) -> reifier ids that annotate it.
type ReifierIndex = BTreeMap<(usize, usize, usize), Vec<usize>>;
/// Annotation lookup: reifier id -> sorted annotation (predicate, value) rows.
type AnnotationIndex = BTreeMap<usize, Vec<(usize, usize)>>;
/// Quads grouped by graph name and then by subject.
type QuadGroups = BTreeMap<Option<usize>, BTreeMap<usize, Vec<(usize, usize)>>>;

/// The `yaml_ld` export-leaf stage.
pub struct YamlLdStage {
    consumes: Vec<String>,
}

impl YamlLdStage {
    /// Construct the stage; it consumes THIS run's snapshot fold.
    pub fn new() -> Self {
        Self {
            consumes: vec!["stage-snapshot".to_string()],
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
    fn kind(&self) -> StageKind {
        StageKind::ExportLeaf
    }
    fn consumes(&self) -> &[String] {
        &self.consumes
    }
    fn impl_version(&self) -> &str {
        // v2: adds deterministic YAML-LD-star output and the preservation ledger.
        "yaml_ld.jsonld_star.v2-yaml-ld"
    }
    fn run(&self, _input: StageInput<'_>) -> Result<StageOutput, PipelineError> {
        let gts = crate::stages::snapshot::snapshot_bytes(_input.upstream)?;
        let graph = gmeow_rdf::gts::read_graph(&gts, true)
            .map_err(|e| PipelineError::Parse(format!("read snapshot gmeow.gts: {e}")))?;
        let json = serialize_graph(&graph)?;
        let yaml = serialize_graph_yaml(&graph, None)?;
        let preservation = preservation_ledger();
        let mut artifacts: BTreeMap<String, Vec<u8>> = BTreeMap::new();
        artifacts.insert(JSON_LD_PATH.to_string(), json.into_bytes());
        artifacts.insert(YAML_LD_PATH.to_string(), yaml.into_bytes());
        artifacts.insert(PRESERVATION_PATH.to_string(), preservation.into_bytes());
        Ok(StageOutput {
            product: StageProduct::from_artifacts(self.id(), artifacts),
        })
    }
}

/// Convert a sorted BTreeMap into a serde_json object value.
fn to_json_object(map: BTreeMap<String, Value>) -> Value {
    Value::Object(map.into_iter().collect())
}

/// Serialize a folded GTS graph to a deterministic JSON-LD-star document.
pub fn serialize_graph(graph: &Graph) -> Result<String, PipelineError> {
    let mut doc = BTreeMap::new();
    doc.insert("@context".to_string(), build_context());

    let (default_nodes, named_graphs) = build_graphs(graph)?;

    let mut top_graph: Vec<Value> = default_nodes.into_values().collect();
    for (_, graph_obj) in named_graphs {
        top_graph.push(graph_obj);
    }
    // Deterministic order: default-graph nodes by @id, then named graphs by @id.
    top_graph.sort_by_key(json_key);

    if !top_graph.is_empty() {
        doc.insert("@graph".to_string(), Value::Array(top_graph));
    }

    let value = to_json_object(doc);
    serde_json::to_string_pretty(&value)
        .map_err(|e| PipelineError::Decode(format!("JSON-LD serialization: {e}")))
}

/// Serialize a folded GTS graph to deterministic YAML-LD-star bytes.
///
/// The JSON-LD-star document is re-serialized to YAML with sorted keys, block
/// style, no anchors/aliases, and an explicit `@context`. The header carries a
/// YAML language-server schema reference.
pub fn serialize_graph_yaml(
    graph: &Graph,
    schema_url: Option<&str>,
) -> Result<String, PipelineError> {
    let json = serialize_graph(graph)?;
    let value: Value = serde_json::from_str(&json)
        .map_err(|e| PipelineError::Decode(format!("parse JSON-LD for YAML: {e}")))?;
    let body = serde_yaml::to_string(&value)
        .map_err(|e| PipelineError::Decode(format!("YAML-LD serialization: {e}")))?;
    let url = schema_url.unwrap_or(BUNDLED_SCHEMA_REF);
    let header = format!(
        "# yaml-language-server: $schema={url}\n\
         # TODO(#700): default schema URL is bounded to the bundled gmeow.schema.json;\n\
         # replace with the canonical public URL once issue #700 finalizes the schema surface.\n"
    );
    Ok(header + &body)
}

/// Serialization-preservation ledger: records JSON-LD-star and YAML-LD-star as lossless.
fn preservation_ledger() -> String {
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

/// Build the JSON-LD `@context` from the GMEOW prefix registry plus `@vocab`.
fn build_context() -> Value {
    let mut ctx = BTreeMap::new();
    ctx.insert(
        "@vocab".to_string(),
        Value::String(GMEOW_NAMESPACE.to_string()),
    );
    for (prefix, namespace) in PREFIXES_BY_LEN.iter().rev() {
        // Reverse gives prefix-name order for deterministic insertion, but
        // BTreeMap sorts by key anyway.
        ctx.insert(prefix.to_string(), Value::String(namespace.to_string()));
    }
    to_json_object(ctx)
}

/// Build default-graph nodes and named-graph objects.
fn build_graphs(graph: &Graph) -> Result<GraphNodes, PipelineError> {
    // Reifier index: base triple (s,p,o) -> reifier ids that annotate it.
    let mut reifier_of: ReifierIndex = BTreeMap::new();
    for &(rid, (s, p, o)) in &graph.reifiers {
        reifier_of.entry((s, p, o)).or_default().push(rid);
    }
    for list in reifier_of.values_mut() {
        // Sort by the reifier's stable @id, not its input-order term id.
        list.sort_by(|a, b| {
            let a_id = term_id(&graph.terms[*a]).expect("reifier must be IRI or blank node");
            let b_id = term_id(&graph.terms[*b]).expect("reifier must be IRI or blank node");
            a_id.cmp(&b_id)
        });
    }

    // Annotation index: reifier id -> sorted annotation (predicate, value) rows.
    let mut annotations_of: AnnotationIndex = BTreeMap::new();
    for &(r, p, v) in &graph.annotations {
        annotations_of.entry(r).or_default().push((p, v));
    }
    for list in annotations_of.values_mut() {
        // Sort by stable predicate @id then stable value key, not raw term ids.
        list.sort_by(|(ap, av), (bp, bv)| {
            let a_pred = term_id(&graph.terms[*ap]).expect("annotation predicate must be IRI");
            let b_pred = term_id(&graph.terms[*bp]).expect("annotation predicate must be IRI");
            a_pred.cmp(&b_pred).then_with(|| {
                term_sort_key(graph, &graph.terms[*av])
                    .cmp(&term_sort_key(graph, &graph.terms[*bv]))
            })
        });
    }

    // Group quads by graph name (None = default graph) and then by subject.
    let mut by_graph: QuadGroups = BTreeMap::new();
    for &(s, p, o, g) in &graph.quads {
        by_graph
            .entry(g)
            .or_default()
            .entry(s)
            .or_default()
            .push((p, o));
    }

    let mut default_nodes: BTreeMap<String, Value> = BTreeMap::new();
    let mut named_graphs: BTreeMap<String, Value> = BTreeMap::new();

    for (g, subjects) in by_graph {
        let mut nodes: Vec<Value> = Vec::new();
        for (s, pos) in subjects {
            let node = build_node(graph, s, pos, &reifier_of, &annotations_of)?;
            nodes.push(node);
        }
        // Sort nodes by their @id (or lexical key for bnodes).
        nodes.sort_by_key(node_id_key);

        match g {
            None => {
                for node in nodes {
                    if let Some(Value::String(id)) = node.get("@id") {
                        default_nodes.insert(id.clone(), node);
                    } else {
                        // Bnode subject without @id should not happen because we always
                        // emit _:label; keep a stable fallback key.
                        default_nodes.insert(format!("__bnode:{node:?}"), node);
                    }
                }
            }
            Some(gid) => {
                let graph_term = &graph.terms[gid];
                let graph_id = term_id(graph_term)?;
                let mut graph_obj = BTreeMap::new();
                graph_obj.insert("@id".to_string(), Value::String(graph_id.clone()));
                graph_obj.insert("@graph".to_string(), Value::Array(nodes));
                named_graphs.insert(graph_id, to_json_object(graph_obj));
            }
        }
    }

    Ok((default_nodes, named_graphs))
}

/// Build one node object for a subject from its predicate/object rows.
fn build_node(
    graph: &Graph,
    subject: usize,
    pos: Vec<(usize, usize)>,
    reifier_of: &ReifierIndex,
    annotations_of: &AnnotationIndex,
) -> Result<Value, PipelineError> {
    let subject_term = &graph.terms[subject];
    let mut node = BTreeMap::new();
    node.insert("@id".to_string(), Value::String(term_id(subject_term)?));

    // Group predicate -> objects, preserving rdf:type separately.
    let mut types: Vec<Value> = Vec::new();
    let mut props: BTreeMap<String, Vec<Value>> = BTreeMap::new();

    for (p, o) in pos {
        let predicate_term = &graph.terms[p];
        let predicate_iri = predicate_term
            .value
            .as_deref()
            .ok_or_else(|| PipelineError::Parse("predicate missing IRI value".to_string()))?;
        let object_term = &graph.terms[o];

        if predicate_iri == RDF_TYPE {
            types.push(term_ref_value(object_term)?);
        } else {
            let key = curie(predicate_iri);
            let value = build_value_object(
                graph,
                subject,
                p,
                o,
                object_term,
                reifier_of,
                annotations_of,
            )?;
            props.entry(key).or_default().push(value);
        }
    }

    if !types.is_empty() {
        types.sort_by(cmp_value);
        node.insert("@type".to_string(), Value::Array(types));
    }

    for (key, mut values) in props {
        values.sort_by(cmp_value);
        let value = if values.len() == 1 {
            values.into_iter().next().unwrap()
        } else {
            Value::Array(values)
        };
        node.insert(key, value);
    }

    Ok(to_json_object(node))
}

/// Build a value object for a quad object, attaching `@annotation` when the
/// base triple is reified.
fn build_value_object(
    graph: &Graph,
    subject: usize,
    predicate: usize,
    object: usize,
    object_term: &Term,
    reifier_of: &ReifierIndex,
    annotations_of: &AnnotationIndex,
) -> Result<Value, PipelineError> {
    let mut value = if object_term.kind == TermKind::Triple {
        build_triple_term_value(graph, object_term, reifier_of, annotations_of)?
    } else {
        term_to_value(graph, object_term)?
    };

    if let Some(reifiers) = reifier_of.get(&(subject, predicate, object)) {
        let annotations: Result<Vec<Value>, _> = reifiers
            .iter()
            .map(|&rid| build_annotation_node(graph, rid, annotations_of))
            .collect();
        let annotations = annotations?;
        let ann_value = if annotations.len() == 1 {
            annotations.into_iter().next().unwrap()
        } else {
            Value::Array(annotations)
        };
        // Attach @annotation to the value object.
        if let Value::Object(ref mut map) = value {
            map.insert("@annotation".to_string(), ann_value);
        } else {
            // Wrap a non-object value (should not happen for annotated triples)
            // into a value object with @annotation.
            let mut wrapper = BTreeMap::new();
            wrapper.insert("@value".to_string(), value);
            wrapper.insert("@annotation".to_string(), ann_value);
            value = to_json_object(wrapper);
        }
    }

    Ok(value)
}

/// Render a triple term (object position) as its JSON-LD-star annotated node
/// object, using the term's own reifier binding.
fn build_triple_term_value(
    graph: &Graph,
    term: &Term,
    reifier_of: &ReifierIndex,
    annotations_of: &AnnotationIndex,
) -> Result<Value, PipelineError> {
    let (s, p, o) = term
        .reifier
        .and_then(|rid| graph.reifier(rid))
        .ok_or_else(|| PipelineError::Parse("triple term with no reifier binding".to_string()))?;
    build_nested_triple_node(graph, s, p, o, reifier_of, annotations_of)
}

/// Build the JSON-LD-star annotated node object for a quoted triple (s,p,o).
fn build_nested_triple_node(
    _graph: &Graph,
    _s: usize,
    _p: usize,
    _o: usize,
    _reifier_of: &ReifierIndex,
    _annotations_of: &AnnotationIndex,
) -> Result<Value, PipelineError> {
    // A bare quoted-triple in object position would have to be encoded as
    // `{"@id": s, <p-curie>: <object>}`, which (a) is indistinguishable from an
    // ordinary node object and (b) is not parseable back: the parser's `@id`
    // branch returns only the subject IRI and drops the predicate/object,
    // silently corrupting the triple term. Rather than emit that lossy,
    // ambiguous form we fail closed. The lossless, supported representation for
    // RDF-1.2-star here is the rdf:reifies / `@annotation` form, which is
    // unaffected by this guard. Full lossless nested-triple-term support
    // (object-position and annotation-value triple terms) is a deferred
    // extension requiring a distinguishable JSON-LD-star encoding.
    Err(PipelineError::Parse(
        "quoted-triple object terms are not yet losslessly serializable in JSON-LD-star; \
         use the rdf:reifies/@annotation form"
            .to_string(),
    ))
}

/// Convert a single RDF term to its JSON-LD value-object form.
fn term_to_value(graph: &Graph, term: &Term) -> Result<Value, PipelineError> {
    match term.kind {
        TermKind::Iri | TermKind::Bnode => {
            let mut map = BTreeMap::new();
            map.insert("@id".to_string(), Value::String(term_id(term)?));
            Ok(to_json_object(map))
        }
        TermKind::Literal => {
            let mut map = BTreeMap::new();
            map.insert(
                "@value".to_string(),
                Value::String(term.value.clone().unwrap_or_default()),
            );
            let datatype = graph.datatype_iri(term);
            if datatype == gmeow_gts::model::RDF_DIR_LANG_STRING {
                if let Some(lang) = &term.lang {
                    map.insert("@language".to_string(), Value::String(lang.clone()));
                }
                if let Some(dir) = &term.direction {
                    map.insert("@direction".to_string(), Value::String(dir.clone()));
                }
            } else if datatype == gmeow_gts::model::RDF_LANG_STRING {
                if let Some(lang) = &term.lang {
                    map.insert("@language".to_string(), Value::String(lang.clone()));
                }
            } else if datatype != gmeow_gts::model::XSD_STRING {
                map.insert("@type".to_string(), Value::String(curie(&datatype)));
            }
            Ok(to_json_object(map))
        }
        TermKind::Triple => Err(PipelineError::Parse(
            "term_to_value does not handle triple terms; caller should use build_value_object"
                .to_string(),
        )),
    }
}

/// Build the annotation node object for a single reifier.
fn build_annotation_node(
    graph: &Graph,
    reifier_id: usize,
    annotations_of: &AnnotationIndex,
) -> Result<Value, PipelineError> {
    let reifier_term = &graph.terms[reifier_id];
    let mut node = BTreeMap::new();
    node.insert("@id".to_string(), Value::String(term_id(reifier_term)?));

    if let Some(anns) = annotations_of.get(&reifier_id) {
        let mut props: BTreeMap<String, Vec<Value>> = BTreeMap::new();
        for &(p, v) in anns {
            let p_term = &graph.terms[p];
            let p_iri = p_term.value.as_deref().ok_or_else(|| {
                PipelineError::Parse("annotation predicate missing IRI".to_string())
            })?;
            let v_term = &graph.terms[v];
            let value = simple_term_value(graph, v_term)?;
            props.entry(curie(p_iri)).or_default().push(value);
        }
        for (key, mut values) in props {
            values.sort_by(cmp_value);
            let value = if values.len() == 1 {
                values.into_iter().next().unwrap()
            } else {
                Value::Array(values)
            };
            node.insert(key, value);
        }
    }

    Ok(to_json_object(node))
}

/// Convert a term to a value object without recursive triple-term handling.
fn simple_term_value(graph: &Graph, term: &Term) -> Result<Value, PipelineError> {
    match term.kind {
        TermKind::Iri | TermKind::Bnode => {
            let mut map = BTreeMap::new();
            map.insert("@id".to_string(), Value::String(term_id(term)?));
            Ok(to_json_object(map))
        }
        TermKind::Literal => {
            let mut map = BTreeMap::new();
            map.insert(
                "@value".to_string(),
                Value::String(term.value.clone().unwrap_or_default()),
            );
            let datatype = graph.datatype_iri(term);
            if datatype == gmeow_gts::model::RDF_DIR_LANG_STRING {
                if let Some(lang) = &term.lang {
                    map.insert("@language".to_string(), Value::String(lang.clone()));
                }
                if let Some(dir) = &term.direction {
                    map.insert("@direction".to_string(), Value::String(dir.clone()));
                }
            } else if datatype == gmeow_gts::model::RDF_LANG_STRING {
                if let Some(lang) = &term.lang {
                    map.insert("@language".to_string(), Value::String(lang.clone()));
                }
            } else if datatype != gmeow_gts::model::XSD_STRING {
                map.insert("@type".to_string(), Value::String(curie(&datatype)));
            }
            Ok(to_json_object(map))
        }
        // Triple-valued annotation objects (an annotation whose value is itself a
        // quoted triple term) have no distinguishable, losslessly parseable
        // JSON-LD-star encoding here yet. Emitting a placeholder literal would
        // silently corrupt RDF-1.2-star data, so we fail closed. Full lossless
        // nested-triple-term support (both object-position and annotation-value
        // triple terms) is a deferred extension that requires a distinguishable
        // JSON-LD-star encoding; until then "lossless or hard-fail" wins.
        TermKind::Triple => Err(PipelineError::Parse(
            "triple-valued annotation objects are not yet losslessly serializable; \
             refusing to emit a lossy placeholder"
                .to_string(),
        )),
    }
}

/// A value object for `rdf:type` targets (always IRI/bnode references).
fn term_ref_value(term: &Term) -> Result<Value, PipelineError> {
    let mut map = BTreeMap::new();
    map.insert("@id".to_string(), Value::String(term_id(term)?));
    Ok(to_json_object(map))
}

/// Return a stable `@id` string for an IRI or blank node term.
fn term_id(term: &Term) -> Result<String, PipelineError> {
    match term.kind {
        TermKind::Iri => Ok(term
            .value
            .as_deref()
            .map(curie)
            .unwrap_or_else(|| "_:missing-iri".to_string())),
        TermKind::Bnode => Ok(format!(
            "_:{}",
            term.value.as_deref().unwrap_or("missing-bnode")
        )),
        TermKind::Literal => Err(PipelineError::Parse(
            "expected IRI or blank node, got literal".to_string(),
        )),
        TermKind::Triple => Err(PipelineError::Parse(
            "expected IRI or blank node, got triple term".to_string(),
        )),
    }
}

/// Return a stable, lexical sort key for an RDF term.
///
/// Unlike raw term ids, this key is independent of the order in which terms
/// were appended to the graph, so it is safe to use when normalizing output.
fn term_sort_key(graph: &Graph, term: &Term) -> String {
    match term.kind {
        TermKind::Iri | TermKind::Bnode => term_id(term).unwrap_or_default(),
        TermKind::Literal => {
            let mut key = format!("lit:{}", term.value.as_deref().unwrap_or_default());
            if let Some(lang) = &term.lang {
                key.push_str(&format!("@{lang}"));
            }
            if let Some(dir) = &term.direction {
                key.push_str(&format!("^{dir}"));
            }
            key.push_str(&format!("^^{}", graph.datatype_iri(term)));
            key
        }
        TermKind::Triple => format!("triple:{}", term.reifier.unwrap_or(usize::MAX)),
    }
}

/// Compact an IRI to a CURIE using the longest matching prefix.
fn curie(iri: &str) -> String {
    for (prefix, ns) in PREFIXES_BY_LEN.iter() {
        if let Some(rest) = iri.strip_prefix(ns) {
            return format!("{prefix}:{rest}");
        }
    }
    iri.to_string()
}

/// Sort key for a top-level @graph entry (named graph object or default node).
fn json_key(value: &Value) -> String {
    match value {
        Value::Object(map) => map
            .get("@id")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        _ => String::new(),
    }
}

/// Sort key for a node object used while ordering the default-graph nodes.
fn node_id_key(value: &Value) -> String {
    json_key(value)
}

/// Deterministic comparison of JSON-LD value objects.
fn cmp_value(a: &Value, b: &Value) -> std::cmp::Ordering {
    let key = |v: &Value| -> String {
        if let Some(s) = v.as_str() {
            return format!("0:{s}");
        }
        if let Some(obj) = v.as_object() {
            let mut parts: Vec<String> = Vec::new();
            if let Some(id) = obj.get("@id").and_then(Value::as_str) {
                parts.push(format!("id={id}"));
            }
            if let Some(val) = obj.get("@value").and_then(Value::as_str) {
                parts.push(format!("value={val}"));
            }
            if let Some(lang) = obj.get("@language").and_then(Value::as_str) {
                parts.push(format!("lang={lang}"));
            }
            if let Some(dir) = obj.get("@direction").and_then(Value::as_str) {
                parts.push(format!("dir={dir}"));
            }
            if let Some(dt) = obj.get("@type").and_then(Value::as_str) {
                parts.push(format!("dt={dt}"));
            }
            parts.sort();
            return format!("1:{}", parts.join("|"));
        }
        format!("2:{v}")
    };
    key(a).cmp(&key(b))
}

/// Parse JSON-LD-star bytes into an oxigraph [`Dataset`].
///
/// This is the inverse of [`serialize_graph`]: it interprets the `@annotation`
/// idiom produced by the GMEOW JSON-LD-star emitter and reconstructs RDF 1.2
/// reifier quads (`rdf:reifies` with quoted triple objects) plus annotation
/// triples in the default graph. Named graphs and directional language strings
/// are preserved. Unsupported JSON-LD features hard-fail.
pub fn parse_jsonld_star(json_bytes: &[u8]) -> Result<Dataset, PipelineError> {
    let json = std::str::from_utf8(json_bytes)
        .map_err(|e| PipelineError::Decode(format!("JSON-LD-star bytes are not UTF-8: {e}")))?;
    let value: Value = serde_json::from_str(json)
        .map_err(|e| PipelineError::Decode(format!("parse JSON-LD-star: {e}")))?;
    let mut prefixes: BTreeMap<String, String> = BTreeMap::new();
    let mut vocab = String::new();
    if let Some(Value::Object(ctx)) = value.get("@context") {
        for (k, v) in ctx {
            if k == "@vocab" {
                if let Some(ns) = v.as_str() {
                    vocab = ns.to_string();
                }
            } else if let Some(ns) = v.as_str() {
                prefixes.insert(k.clone(), ns.to_string());
            }
        }
    }

    let expand = |curie_or_iri: &str| -> String {
        if curie_or_iri.starts_with("http://") || curie_or_iri.starts_with("https://") {
            return curie_or_iri.to_string();
        }
        if let Some((p, local)) = curie_or_iri.split_once(':') {
            if let Some(ns) = prefixes.get(p) {
                return format!("{ns}{local}");
            }
        }
        if !vocab.is_empty() && !curie_or_iri.contains(':') {
            return format!("{vocab}{curie_or_iri}");
        }
        curie_or_iri.to_string()
    };

    let store = Store::new().map_err(|e| PipelineError::Parse(e.to_string()))?;

    let emit_node = |node: &Value, graph_iri: Option<&str>| -> Result<(), PipelineError> {
        let id = node
            .get("@id")
            .and_then(Value::as_str)
            .ok_or_else(|| PipelineError::Decode("node without @id".to_string()))?;
        let subject: NamedOrBlankNode = if let Some(label) = id.strip_prefix("_:") {
            oxigraph::model::BlankNode::new(label.to_string())
                .map_err(|e| PipelineError::Decode(e.to_string()))?
                .into()
        } else {
            NamedNode::new(expand(id))
                .map_err(|e| PipelineError::Decode(e.to_string()))?
                .into()
        };
        let graph_name = graph_iri
            .map(|g| {
                NamedNode::new(expand(g))
                    .map(oxigraph::model::GraphName::from)
                    .map_err(|e| PipelineError::Decode(e.to_string()))
            })
            .transpose()?
            .unwrap_or(oxigraph::model::GraphName::DefaultGraph);

        if let Some(Value::Array(types)) = node.get("@type") {
            let rdf_type = NamedNode::new(RDF_TYPE).unwrap();
            for t in types {
                let t_id = t
                    .get("@id")
                    .and_then(Value::as_str)
                    .ok_or_else(|| PipelineError::Decode("@type value without @id".to_string()))?;
                let obj: oxigraph::model::Term = NamedNode::new(expand(t_id))
                    .map_err(|e| PipelineError::Decode(e.to_string()))?
                    .into();
                store
                    .insert(&Quad::new(
                        subject.clone(),
                        rdf_type.clone(),
                        obj,
                        graph_name.clone(),
                    ))
                    .map_err(|e| PipelineError::Parse(e.to_string()))?;
            }
        }

        for (key, val) in node.as_object().unwrap() {
            if matches!(key.as_str(), "@id" | "@type" | "@context" | "@graph") {
                continue;
            }
            let predicate =
                NamedNode::new(expand(key)).map_err(|e| PipelineError::Decode(e.to_string()))?;
            let values = if let Value::Array(arr) = val {
                arr.clone()
            } else {
                vec![val.clone()]
            };
            for v in values {
                emit_value_quad(
                    &store,
                    subject.clone(),
                    predicate.clone(),
                    graph_name.clone(),
                    &v,
                    &expand,
                )?;
            }
        }
        Ok(())
    };

    match &value {
        Value::Array(entries) => {
            for entry in entries {
                emit_graph_entry(entry, &emit_node)?;
            }
        }
        Value::Object(obj) if obj.contains_key("@graph") => {
            let graphs = obj
                .get("@graph")
                .and_then(Value::as_array)
                .ok_or_else(|| PipelineError::Decode("@graph must be an array".to_string()))?;
            for entry in graphs {
                emit_graph_entry(entry, &emit_node)?;
            }
        }
        Value::Object(_) => {
            emit_node(&value, None)?;
        }
        _ => {
            return Err(PipelineError::Decode(
                "JSON-LD document must be an object or array of objects".to_string(),
            ));
        }
    }

    store
        .iter()
        .collect::<Result<Dataset, _>>()
        .map_err(|e| PipelineError::Parse(e.to_string()))
}

type EmitNodeFn<'a> = dyn Fn(&Value, Option<&str>) -> Result<(), PipelineError> + 'a;

fn emit_graph_entry(entry: &Value, emit_node: &EmitNodeFn<'_>) -> Result<(), PipelineError> {
    if entry.get("@graph").is_some() {
        let graph_id = entry
            .get("@id")
            .and_then(Value::as_str)
            .ok_or_else(|| PipelineError::Decode("named graph object must have @id".to_string()))?;
        for node in entry
            .get("@graph")
            .and_then(Value::as_array)
            .ok_or_else(|| PipelineError::Decode("@graph must be an array".to_string()))?
        {
            emit_node(node, Some(graph_id))?;
        }
    } else {
        emit_node(entry, None)?;
    }
    Ok(())
}

fn emit_value_quad(
    store: &Store,
    subject: NamedOrBlankNode,
    predicate: NamedNode,
    graph_name: oxigraph::model::GraphName,
    value: &Value,
    expand: &dyn Fn(&str) -> String,
) -> Result<(), PipelineError> {
    let (object, annotation) = parse_value_object(value, expand)?;
    store
        .insert(&Quad::new(
            subject.clone(),
            predicate.clone(),
            object.clone(),
            graph_name.clone(),
        ))
        .map_err(|e| PipelineError::Parse(e.to_string()))?;

    if let Some(ann) = annotation {
        // The emitter may attach one annotation object or an array when several
        // distinct reifiers annotate the same base triple (gmeow-gts#213).
        let annotations: Vec<&Value> = match &ann {
            Value::Array(arr) => arr.iter().collect(),
            other => vec![other],
        };
        for ann_node in annotations {
            let reifier_subject = ann_node
                .get("@id")
                .and_then(Value::as_str)
                .ok_or_else(|| PipelineError::Decode("annotation without @id".to_string()))?;
            let reifier: NamedOrBlankNode = if let Some(label) = reifier_subject.strip_prefix("_:")
            {
                oxigraph::model::BlankNode::new(label.to_string())
                    .map_err(|e| PipelineError::Decode(e.to_string()))?
                    .into()
            } else {
                NamedNode::new(expand(reifier_subject))
                    .map_err(|e| PipelineError::Decode(e.to_string()))?
                    .into()
            };
            let reifies = NamedNode::new(RDF_REIFIES).unwrap();
            let quoted = oxigraph::model::Term::Triple(Box::new(oxigraph::model::Triple::new(
                subject.clone(),
                predicate.clone(),
                object.clone(),
            )));
            store
                .insert(&Quad::new(
                    reifier.clone(),
                    reifies,
                    quoted,
                    oxigraph::model::GraphName::DefaultGraph,
                ))
                .map_err(|e| PipelineError::Parse(e.to_string()))?;

            for (key, val) in ann_node.as_object().unwrap() {
                if key == "@id" {
                    continue;
                }
                let ann_predicate = NamedNode::new(expand(key))
                    .map_err(|e| PipelineError::Decode(e.to_string()))?;
                let vals = if let Value::Array(arr) = val {
                    arr.clone()
                } else {
                    vec![val.clone()]
                };
                for v in vals {
                    let (ann_object, _) = parse_value_object(&v, expand)?;
                    store
                        .insert(&Quad::new(
                            reifier.clone(),
                            ann_predicate.clone(),
                            ann_object,
                            oxigraph::model::GraphName::DefaultGraph,
                        ))
                        .map_err(|e| PipelineError::Parse(e.to_string()))?;
                }
            }
        }
    }

    Ok(())
}

fn parse_value_object(
    value: &Value,
    expand: &dyn Fn(&str) -> String,
) -> Result<(oxigraph::model::Term, Option<Value>), PipelineError> {
    if let Some(s) = value.as_str() {
        return Ok((
            NamedNode::new(expand(s))
                .map_err(|e| PipelineError::Decode(e.to_string()))?
                .into(),
            None,
        ));
    }
    let obj = value
        .as_object()
        .ok_or_else(|| PipelineError::Decode(format!("expected value object, got {value}")))?;
    let annotation = obj.get("@annotation").cloned();

    if let Some(id) = obj.get("@id").and_then(Value::as_str) {
        let term: oxigraph::model::Term = if let Some(label) = id.strip_prefix("_:") {
            oxigraph::model::BlankNode::new(label.to_string())
                .map_err(|e| PipelineError::Decode(e.to_string()))?
                .into()
        } else {
            NamedNode::new(expand(id))
                .map_err(|e| PipelineError::Decode(e.to_string()))?
                .into()
        };
        return Ok((term, annotation));
    }

    let lex = obj
        .get("@value")
        .and_then(Value::as_str)
        .ok_or_else(|| PipelineError::Decode("literal without @value".to_string()))?
        .to_string();
    let lang = obj.get("@language").and_then(Value::as_str);
    let direction = obj.get("@direction").and_then(Value::as_str);
    let datatype = obj.get("@type").and_then(Value::as_str);

    // Use the UNCHECKED language-tag constructors so the project's long private-use
    // subtags (`x-gmeow-norwegiannynorsk`, >8 chars) survive — strict oxigraph
    // validation rejects them, and #909 preserves them end-to-end (matching the
    // lenient gmeow-gts codecs that produced this JSON-LD-star input).
    let literal = match (lang, direction, datatype) {
        (Some(lang), Some(dir), _) => {
            let dir = match dir {
                "ltr" => oxigraph::model::BaseDirection::Ltr,
                "rtl" => oxigraph::model::BaseDirection::Rtl,
                _ => return Err(PipelineError::Decode(format!("invalid direction {dir}"))),
            };
            oxigraph::model::Literal::new_directional_language_tagged_literal_unchecked(
                &lex, lang, dir,
            )
        }
        (Some(lang), None, _) => {
            oxigraph::model::Literal::new_language_tagged_literal_unchecked(&lex, lang)
        }
        (None, _, Some(dt)) => oxigraph::model::Literal::new_typed_literal(
            &lex,
            NamedNode::new(expand(dt)).map_err(|e| PipelineError::Decode(e.to_string()))?,
        ),
        _ => oxigraph::model::Literal::new_simple_literal(&lex),
    };

    Ok((literal.into(), annotation))
}

/// Convert a JSON-LD-star document to GMEOW statement-metadata N-Quads.
///
/// RDF 1.2 quoted triples (`?r rdf:reifies <<( ?s ?p ?o )>>`) cannot be
/// represented by rdflib-based consumers, so this downcast re-expresses each
/// annotated statement as a native GMEOW statement-metadata cell:
///
/// ```turtle
/// ?r a gmeow:StatementMetadata ;
///    gmeow:qSubject ?s ;
///    gmeow:qPredicate ?p ;
///    gmeow:qObject ?o | gmeow:qObjectLiteral ?o ;
///    <annotation-pred> <annotation-value> .
/// ```
///
/// The base triple `?s ?p ?o` is retained, and every annotation triple on the
/// reifier is carried through unchanged. The output contains no quoted triples,
/// so it is safe for the rdflib-compat up-projection lane.
pub fn jsonld_star_to_gmeow_statement_metadata_nquads(
    json_bytes: &[u8],
) -> Result<String, PipelineError> {
    let dataset = parse_jsonld_star(json_bytes)?;
    let out = Store::new().map_err(|e| PipelineError::Parse(e.to_string()))?;

    // Work with owned quads so subjects/objects are not reference types.
    let quads: Vec<Quad> = dataset.iter().map(|q| q.into_owned()).collect();

    // Identify reifiers and the quoted triple each one refers to.
    let mut reifier_quotes: std::collections::HashMap<
        NamedOrBlankNode,
        (NamedOrBlankNode, NamedNode, OxTerm),
    > = std::collections::HashMap::new();
    for quad in &quads {
        if quad.predicate.as_str() == RDF_REIFIES {
            if let OxTerm::Triple(triple) = &quad.object {
                reifier_quotes.insert(
                    quad.subject.clone(),
                    (
                        triple.subject.clone(),
                        triple.predicate.clone(),
                        triple.object.clone(),
                    ),
                );
            }
        }
    }

    let rdf_type = NamedNode::new(RDF_TYPE).expect("valid rdf:type IRI");
    let statement_metadata =
        NamedNode::new(GMEOW_STATEMENT_METADATA).expect("valid gmeow:StatementMetadata IRI");
    let q_subject = NamedNode::new(GMEOW_QSUBJECT).expect("valid gmeow:qSubject IRI");
    let q_predicate = NamedNode::new(GMEOW_QPREDICATE).expect("valid gmeow:qPredicate IRI");
    let q_object = NamedNode::new(GMEOW_QOBJECT).expect("valid gmeow:qObject IRI");
    let q_object_literal =
        NamedNode::new(GMEOW_QOBJECTLITERAL).expect("valid gmeow:qObjectLiteral IRI");

    for quad in &quads {
        if quad.predicate.as_str() == RDF_REIFIES {
            // Emit the GMEOW statement-metadata skeleton for this reifier.
            let Some((s, p, o)) = reifier_quotes.get(&quad.subject) else {
                continue;
            };
            let r = quad.subject.clone();
            out.insert(&Quad::new(
                r.clone(),
                rdf_type.clone(),
                OxTerm::NamedNode(statement_metadata.clone()),
                GraphName::DefaultGraph,
            ))
            .map_err(|e| PipelineError::Parse(e.to_string()))?;
            out.insert(&Quad::new(
                r.clone(),
                q_subject.clone(),
                OxTerm::from(s.clone()),
                GraphName::DefaultGraph,
            ))
            .map_err(|e| PipelineError::Parse(e.to_string()))?;
            out.insert(&Quad::new(
                r.clone(),
                q_predicate.clone(),
                OxTerm::NamedNode(p.clone()),
                GraphName::DefaultGraph,
            ))
            .map_err(|e| PipelineError::Parse(e.to_string()))?;
            let q_object_pred = if matches!(o, OxTerm::Literal(_)) {
                q_object_literal.clone()
            } else {
                q_object.clone()
            };
            out.insert(&Quad::new(
                r.clone(),
                q_object_pred,
                o.clone(),
                GraphName::DefaultGraph,
            ))
            .map_err(|e| PipelineError::Parse(e.to_string()))?;
        } else if reifier_quotes.contains_key(&quad.subject) {
            // Annotation triple on a reifier: keep it, but move it to the default graph
            // so the downstream rdflib-compat graph (single-graph) sees it.
            out.insert(&Quad::new(
                quad.subject.clone(),
                quad.predicate.clone(),
                quad.object.clone(),
                GraphName::DefaultGraph,
            ))
            .map_err(|e| PipelineError::Parse(e.to_string()))?;
        } else {
            // Plain base triple or named-graph triple.
            out.insert(quad)
                .map_err(|e| PipelineError::Parse(e.to_string()))?;
        }
    }

    // `out` holds only the downcast-flat statement-metadata cells (no
    // object-position quoted triples), so the native N-Quads serializer applies.
    let ir = gmeow_rdf::oxigraph::dataset_from_store(&out)
        .map_err(|e| PipelineError::Decode(format!("store → IR: {e}")))?;
    let buf = gmeow_rdf::serialize_dataset(
        &ir,
        "application/n-quads",
        gmeow_rdf::SerializeGraph::Dataset,
    )
    .map_err(|e| PipelineError::Decode(format!("serialize N-Quads: {e}")))?;
    String::from_utf8(buf).map_err(|e| PipelineError::Decode(format!("N-Quads are not UTF-8: {e}")))
}

/// Convert YAML-LD-star bytes to JSON-LD-star JSON, hard-failing on YAML
/// anchors/aliases (extended YAML is out of scope, #699 non-goal).
///
/// The conversion is purely structural: YAML scalars/sequences/mappings map
/// one-to-one onto JSON, so the resulting JSON is consumable by
/// [`parse_jsonld_star`] and the statement-metadata downcast.
pub fn yaml_ld_star_to_json(yaml_bytes: &[u8]) -> Result<String, PipelineError> {
    let text = std::str::from_utf8(yaml_bytes)
        .map_err(|e| PipelineError::Decode(format!("YAML-LD-star bytes are not UTF-8: {e}")))?;
    // Reject anchors/aliases BEFORE deserializing, using the repo's trusted
    // heuristic: a whitespace-delimited token starting with `&` (anchor) or `*`
    // (alias) signals extended YAML, which is out of scope for #699.
    if text
        .split_whitespace()
        .any(|t| t.starts_with('&') || t.starts_with('*'))
    {
        return Err(PipelineError::Decode(
            "YAML-LD-star must not use anchors or aliases".into(),
        ));
    }
    let value: serde_yaml::Value = serde_yaml::from_str(text)
        .map_err(|e| PipelineError::Decode(format!("parse YAML-LD-star: {e}")))?;
    serde_json::to_string(&value)
        .map_err(|e| PipelineError::Decode(format!("YAML-LD-star -> JSON-LD-star: {e}")))
}

/// Downcast YAML-LD-star bytes to GMEOW statement-metadata N-Quads.
///
/// Routes through [`yaml_ld_star_to_json`] then the JSON-LD-star downcast, so
/// the output contains no quoted triple terms and is safe for the rdflib-compat
/// up-projection lane (#699).
pub fn yaml_ld_star_to_gmeow_statement_metadata_nquads(
    yaml_bytes: &[u8],
) -> Result<String, PipelineError> {
    jsonld_star_to_gmeow_statement_metadata_nquads(yaml_ld_star_to_json(yaml_bytes)?.as_bytes())
}

/// Return an RDFC-1.0 canonical, deterministically sorted quad representation.
///
/// Promoted out of the test module so the build-time round-trip gate
/// ([`roundtrip_isomorphic`]) and the tests share one canonicalizer (#699).
pub(crate) fn canonical_lines(dataset: &Dataset) -> Vec<String> {
    // Native full RDFC-1.0 (#910), replacing oxrdf `Dataset::canonicalize`: identical
    // blank labeling, unchanged term serialization.
    let quads: Vec<Quad> = dataset.iter().map(|q| q.into_owned()).collect();
    let canonical =
        gmeow_rdf::canonicalize_quads(quads).expect("RDFC-1.0 canonicalization of parsed quads");
    let mut lines: Vec<String> = canonical.iter().map(|q| q.to_string()).collect();
    lines.sort();
    lines
}

/// Parse N-Quads-star text into an oxigraph [`Dataset`], preserving quoted
/// triple terms (RDF 1.2-star). Used by [`roundtrip_isomorphic`].
fn dataset_from_nquads(nquads: &[u8]) -> Result<Dataset, PipelineError> {
    // The native codec folds the RDF 1.2 statement layer to the IR reifier table,
    // and `flat_oxigraph_quads_from_dataset` un-folds it back to the equivalent
    // `<reifier> rdf:reifies <<( s p o )>>` object-position quoted triples (the two
    // are exact inverses), so the star structure the RDFC-1.0 canonical comparison
    // depends on is preserved.
    let ir = gmeow_rdf::parse_dataset(nquads, "application/n-quads", None)
        .map_err(|e| PipelineError::Parse(format!("parse N-Quads: {e}")))?;
    let quads = gmeow_rdf::oxigraph::flat_oxigraph_quads_from_dataset(&ir)
        .map_err(|e| PipelineError::Parse(format!("IR → quads: {e}")))?;
    Ok(quads.iter().map(|q| q.as_ref()).collect::<Dataset>())
}

/// Return whether `star_bytes` (format `"jsonld"`|`"yamlld"`) re-parses to a
/// dataset isomorphic (RDFC-1.0 / oxigraph canonical) to the original
/// N-Quads-star input. This is the Rust authority for the build-time
/// serialization-isomorphism gate (#699), replacing the Python `_round_trip_star`.
pub fn roundtrip_isomorphic(
    original_nquads: &[u8],
    star_bytes: &[u8],
    format: &str,
) -> Result<bool, PipelineError> {
    let original = dataset_from_nquads(original_nquads)?;
    let roundtrip = match format {
        "jsonld" => parse_jsonld_star(star_bytes)?,
        "yamlld" => parse_jsonld_star(yaml_ld_star_to_json(star_bytes)?.as_bytes())?,
        other => {
            return Err(PipelineError::Decode(format!(
                "unknown star format {other:?}; expected 'jsonld' or 'yamlld'"
            )))
        }
    };
    Ok(canonical_lines(&original) == canonical_lines(&roundtrip))
}

#[cfg(test)]
mod tests {
    use super::*;

    use gmeow_gts::model::{Term, TermKind};
    use gmeow_rdf::oxigraph::rdf_quad_from_oxigraph;
    use gmeow_rdf::{
        BlankScope, RdfDatasetBuilder, RdfLiteral, RdfLookaside, RdfTerm, RdfTextDirection,
        RdfTriple, TermId,
    };

    use std::collections::HashSet;
    use std::path::PathBuf;
    use std::sync::Arc;

    fn iri_term(value: &str) -> Term {
        Term {
            kind: TermKind::Iri,
            value: Some(value.to_string()),
            datatype: None,
            lang: None,
            direction: None,
            reifier: None,
        }
    }

    fn bnode_term(label: &str) -> Term {
        Term {
            kind: TermKind::Bnode,
            value: Some(label.to_string()),
            datatype: None,
            lang: None,
            direction: None,
            reifier: None,
        }
    }

    fn literal_term(value: &str) -> Term {
        Term {
            kind: TermKind::Literal,
            value: Some(value.to_string()),
            datatype: None,
            lang: None,
            direction: None,
            reifier: None,
        }
    }

    #[allow(dead_code)]
    fn lang_term(value: &str, lang: &str) -> Term {
        Term {
            kind: TermKind::Literal,
            value: Some(value.to_string()),
            datatype: None,
            lang: Some(lang.to_string()),
            direction: None,
            reifier: None,
        }
    }

    fn dir_lang_term(value: &str, lang: &str, direction: &str) -> Term {
        Term {
            kind: TermKind::Literal,
            value: Some(value.to_string()),
            datatype: None,
            lang: Some(lang.to_string()),
            direction: Some(direction.to_string()),
            reifier: None,
        }
    }

    /// Parse N-Quads-star text into an oxigraph Dataset (native codec round-trip).
    fn parse_nquads(nq: &str) -> Dataset {
        super::dataset_from_nquads(nq.as_bytes()).unwrap()
    }

    fn minimal_graph() -> Graph {
        let mut graph = Graph::default();
        // 0: subject
        graph.terms.push(iri_term("https://example.org/s"));
        // 1: predicate
        graph.terms.push(iri_term("https://example.org/p"));
        // 2: object
        graph.terms.push(iri_term("https://example.org/o"));
        // 3: reifier
        graph.terms.push(iri_term("https://example.org/r"));
        // 4: annotation predicate
        graph.terms.push(iri_term("https://example.org/ap"));
        // 5: annotation value
        graph.terms.push(literal_term("meta"));

        graph.quads.push((0, 1, 2, None));
        graph.reifiers.push((3, (0, 1, 2)));
        graph.annotations.push((3, 4, 5));
        graph
    }

    fn ox_named_node(iri: &str) -> NamedNode {
        NamedNode::new(iri).expect("valid test IRI")
    }

    fn ox_simple_literal(lex: &str) -> oxigraph::model::Term {
        oxigraph::model::Literal::new_simple_literal(lex).into()
    }

    fn ox_quoted_triple(
        s: NamedOrBlankNode,
        p: NamedNode,
        o: oxigraph::model::Term,
    ) -> oxigraph::model::Term {
        oxigraph::model::Term::Triple(Box::new(oxigraph::model::Triple::new(s, p, o)))
    }

    fn dataset_has(
        dataset: &Dataset,
        subject: &NamedOrBlankNode,
        predicate: &NamedNode,
        object: &oxigraph::model::Term,
    ) -> bool {
        dataset.iter().any(|q| {
            NamedOrBlankNode::from(q.subject) == *subject
                && q.predicate == *predicate
                && oxigraph::model::Term::from(q.object) == *object
        })
    }

    fn assert_no_gmeow_at_id_leak(dataset: &Dataset, json: &str) {
        const GMEOW_NS: &str = "https://blackcatinformatics.ca/gmeow/";
        let at_id = format!("{GMEOW_NS}@id");
        assert!(
            !dataset.iter().any(|q| q.predicate.as_str() == at_id),
            "gmeow:@id must not leak as a property triple: {json}"
        );
        assert!(
            !dataset.iter().any(|q| {
                q.predicate.as_str().starts_with(GMEOW_NS)
                    && matches!(
                        oxigraph::model::Term::from(q.object),
                        oxigraph::model::Term::NamedNode(n)
                            if n.as_str() == "http://example.org/reifier"
                    )
            }),
            "reifier IRI must not appear as object of any gmeow-prefixed predicate: {json}"
        );
    }

    #[test]
    fn minimal_rdf12_roundtrips_through_oxigraph() {
        let graph = minimal_graph();
        let json = serialize_graph(&graph).expect("serialize");

        let expected = parse_nquads(&gmeow_gts::nquads::to_nquads(&graph));
        let actual = parse_jsonld_star(json.as_bytes()).expect("parse JSON-LD-star");

        assert_eq!(
            canonical_nquads(&expected),
            canonical_nquads(&actual),
            "JSON-LD-star round-trip diverged from N-Quads-star baseline"
        );
    }

    #[test]
    fn multiple_reifiers_on_same_triple_roundtrip() {
        // gmeow-gts#213: RDF 1.2 allows several distinct explicit reifiers for
        // the same triple content. The JSON-LD-star emitter serializes them as
        // an @annotation array; the parser must reconstruct each one.
        let mut graph = Graph::default();
        graph.terms.push(iri_term("https://example.org/s"));
        graph.terms.push(iri_term("https://example.org/p"));
        graph.terms.push(iri_term("https://example.org/o"));
        graph.terms.push(iri_term("https://example.org/r1"));
        graph.terms.push(iri_term("https://example.org/r2"));
        graph
            .terms
            .push(iri_term("https://example.org/accordingTo"));
        graph.terms.push(iri_term("https://example.org/source-a"));
        graph.terms.push(iri_term("https://example.org/confidence"));
        graph.terms.push(literal_term("0.9"));
        graph.terms.push(literal_term("0.7"));

        graph.quads.push((0, 1, 2, None));
        graph.reifiers.push((3, (0, 1, 2)));
        graph.reifiers.push((4, (0, 1, 2)));
        graph.annotations.push((3, 5, 6));
        graph.annotations.push((4, 7, 8));

        let json = serialize_graph(&graph).expect("serialize");
        let expected = parse_nquads(&gmeow_gts::nquads::to_nquads(&graph));
        let actual = parse_jsonld_star(json.as_bytes()).expect("parse JSON-LD-star");

        assert_eq!(
            canonical_nquads(&expected),
            canonical_nquads(&actual),
            "multiple reifiers must round-trip through JSON-LD-star"
        );
    }

    #[test]
    fn serialization_is_byte_deterministic() {
        let graph = minimal_graph();
        let first = serialize_graph(&graph).expect("serialize first");
        let second = serialize_graph(&graph).expect("serialize second");
        assert_eq!(first, second, "JSON-LD output must be byte-deterministic");
    }

    #[test]
    fn directional_language_string_emits_direction() {
        let mut graph = Graph::default();
        graph.terms.push(iri_term("https://example.org/s"));
        graph.terms.push(iri_term("https://example.org/p"));
        graph.terms.push(dir_lang_term("hello", "en", "ltr"));
        graph.quads.push((0, 1, 2, None));

        let json = serialize_graph(&graph).expect("serialize");
        assert!(
            json.contains("\"@direction\": \"ltr\""),
            "directional language literal must emit @direction: {json}"
        );
    }

    #[test]
    fn yaml_ld_is_byte_deterministic() {
        let graph = minimal_graph();
        let first = serialize_graph_yaml(&graph, None).expect("serialize first");
        let second = serialize_graph_yaml(&graph, None).expect("serialize second");
        assert_eq!(first, second, "YAML-LD output must be byte-deterministic");
    }

    /// Build a non-trivial graph through hash-map collections seeded with `seed`.
    ///
    /// The returned graph has the same RDF content regardless of seed, but the
    /// append order of terms, quads, reifiers, and annotations varies with the
    /// hash-map iteration order. This lets determinism tests prove that the
    /// serializer normalizes away any input-order dependency.
    fn build_nontrivial_graph_with_seed(seed: usize) -> Graph {
        use ahash::{AHashMap, RandomState};

        // Terms are collected in a seed-dependent map so their ids vary by seed.
        let mut term_inputs: AHashMap<&'static str, Term> =
            AHashMap::with_hasher(RandomState::with_seed(seed));
        term_inputs.insert("s", iri_term("https://example.org/s"));
        term_inputs.insert("p1", iri_term("https://example.org/p1"));
        term_inputs.insert("p2", iri_term("https://example.org/p2"));
        term_inputs.insert("o1", iri_term("https://example.org/o1"));
        term_inputs.insert("o2", dir_lang_term("bonjour", "fr", "rtl"));
        term_inputs.insert("r1", iri_term("https://example.org/r1"));
        term_inputs.insert("r2", iri_term("https://example.org/r2"));
        term_inputs.insert("ap", iri_term("https://example.org/ap"));
        term_inputs.insert("av1", literal_term("meta-one"));
        term_inputs.insert("av2", literal_term("meta-two"));
        term_inputs.insert("type", iri_term("https://example.org/SomeType"));
        term_inputs.insert("rdf_type", iri_term(RDF_TYPE));

        let mut graph = Graph::default();
        let mut term_idx: AHashMap<&'static str, usize> =
            AHashMap::with_hasher(RandomState::with_seed(seed));
        for (key, term) in term_inputs {
            let id = graph.terms.len();
            graph.terms.push(term);
            term_idx.insert(key, id);
        }

        // Quads are collected in a seed-dependent map so their row order varies by seed.
        let mut quad_inputs: AHashMap<&'static str, (usize, usize, usize, Option<usize>)> =
            AHashMap::with_hasher(RandomState::with_seed(seed));
        quad_inputs.insert(
            "type",
            (term_idx["s"], term_idx["rdf_type"], term_idx["type"], None),
        );
        quad_inputs.insert("q1", (term_idx["s"], term_idx["p1"], term_idx["o1"], None));
        quad_inputs.insert("q2", (term_idx["s"], term_idx["p2"], term_idx["o2"], None));

        let mut quad_idx: AHashMap<&'static str, usize> =
            AHashMap::with_hasher(RandomState::with_seed(seed));
        for (key, quad) in quad_inputs {
            let id = graph.quads.len();
            graph.quads.push(quad);
            quad_idx.insert(key, id);
        }

        // Reifiers are collected in a seed-dependent map.
        let mut reifier_inputs: AHashMap<&'static str, (usize, &'static str)> =
            AHashMap::with_hasher(RandomState::with_seed(seed));
        reifier_inputs.insert("r1", (term_idx["r1"], "q1"));
        reifier_inputs.insert("r2", (term_idx["r2"], "q1"));

        let mut reifier_idx: AHashMap<&'static str, usize> =
            AHashMap::with_hasher(RandomState::with_seed(seed));
        for (key, (term_id, quad_key)) in reifier_inputs {
            let q = graph.quads[quad_idx[quad_key]];
            let id = graph.reifiers.len();
            graph.reifiers.push((term_id, (q.0, q.1, q.2)));
            reifier_idx.insert(key, id);
        }

        // Annotations are collected in a seed-dependent map.
        let mut annotation_inputs: AHashMap<&'static str, (usize, usize, usize)> =
            AHashMap::with_hasher(RandomState::with_seed(seed));
        annotation_inputs.insert("a1", (term_idx["r1"], term_idx["ap"], term_idx["av1"]));
        annotation_inputs.insert("a2", (term_idx["r2"], term_idx["ap"], term_idx["av2"]));
        for (_, ann) in annotation_inputs {
            graph.annotations.push(ann);
        }

        graph
    }

    /// Acceptance criterion #6 (issue #699 / PR #978): JSON-LD-star output is
    /// byte-identical even when the input graph is constructed through hash maps
    /// seeded with different values. The serializer orders every map and array
    /// deterministically, so output must not depend on input append order.
    #[test]
    fn hash_seed_determinism_jsonld_star() {
        let seed_a = 0x1111_1111_1111_1111_usize;
        let seed_b = 0x2222_2222_2222_2222_usize;
        let graph_a = build_nontrivial_graph_with_seed(seed_a);
        let graph_b = build_nontrivial_graph_with_seed(seed_b);

        // The input graphs must differ in append order; otherwise the test is not
        // exercising hash-seed normalization.
        assert_ne!(
            graph_a.terms, graph_b.terms,
            "seeds must produce different term append orders"
        );
        assert_ne!(
            graph_a.quads, graph_b.quads,
            "seeds must produce different quad append orders"
        );

        let json_a = serialize_graph(&graph_a).expect("serialize graph A");
        let json_b = serialize_graph(&graph_b).expect("serialize graph B");
        assert_eq!(
            json_a, json_b,
            "JSON-LD-star output must be identical under different hash-map seeds"
        );
    }

    /// Acceptance criterion #6 (issue #699 / PR #978): YAML-LD-star output is
    /// byte-identical even when the input graph is constructed through hash maps
    /// seeded with different values.
    #[test]
    fn hash_seed_determinism_yaml_ld_star() {
        let seed_a = 0x1111_1111_1111_1111_usize;
        let seed_b = 0x2222_2222_2222_2222_usize;
        let graph_a = build_nontrivial_graph_with_seed(seed_a);
        let graph_b = build_nontrivial_graph_with_seed(seed_b);

        let yaml_a = serialize_graph_yaml(&graph_a, None).expect("serialize YAML-LD A");
        let yaml_b = serialize_graph_yaml(&graph_b, None).expect("serialize YAML-LD B");
        assert_eq!(
            yaml_a, yaml_b,
            "YAML-LD-star output must be identical under different hash-map seeds"
        );
    }

    #[test]
    fn yaml_ld_has_explicit_context_and_no_anchors() {
        let graph = minimal_graph();
        let yaml = serialize_graph_yaml(&graph, None).expect("serialize YAML-LD");
        assert!(
            yaml.contains("@context"),
            "YAML-LD must carry an explicit @context: {yaml}"
        );
        assert!(
            yaml.contains("@graph"),
            "YAML-LD must carry an explicit @graph: {yaml}"
        );
        // Anchor/alias tokens appear as whitespace-delimited `&id` or `*id`.
        assert!(
            !yaml
                .split_whitespace()
                .any(|t| t.starts_with('&') || t.starts_with('*')),
            "YAML-LD must not use anchors or aliases: {yaml}"
        );
        assert!(
            yaml.contains(&format!("yaml-language-server: $schema={BUNDLED_SCHEMA_REF}")),
            "YAML-LD must carry a language-server schema header pointing to the bundled schema: {yaml}"
        );
    }

    #[test]
    fn yaml_ld_roundtrips_through_oxigraph() {
        let graph = minimal_graph();
        let yaml = serialize_graph_yaml(&graph, None).expect("serialize YAML-LD");
        // The test parser works over JSON-LD-star; convert YAML back to JSON first.
        let yaml_value: serde_yaml::Value =
            serde_yaml::from_str(&yaml).expect("parse emitted YAML-LD");
        let json = serde_json::to_string(&yaml_value).expect("YAML -> JSON");

        let expected = parse_nquads(&gmeow_gts::nquads::to_nquads(&graph));
        let actual =
            parse_jsonld_star(json.as_bytes()).expect("parse JSON-LD-star from YAML round-trip");

        assert_eq!(
            canonical_nquads(&expected),
            canonical_nquads(&actual),
            "YAML-LD round-trip diverged from N-Quads-star baseline"
        );
    }

    #[test]
    fn annotation_reifier_explicit_id_on_node_object() {
        let mut graph = Graph::default();
        graph.terms.push(iri_term("http://example.org/s"));
        graph.terms.push(iri_term("http://example.org/p"));
        graph.terms.push(iri_term("http://example.org/o"));
        graph.terms.push(iri_term("http://example.org/reifier"));
        graph.terms.push(iri_term("http://example.org/confidence"));
        graph.terms.push(literal_term("0.9"));
        graph.quads.push((0, 1, 2, None));
        graph.reifiers.push((3, (0, 1, 2)));
        graph.annotations.push((3, 4, 5));

        let json = serialize_graph(&graph).expect("serialize");
        let dataset = parse_jsonld_star(json.as_bytes()).expect("parse JSON-LD-star");

        let s: NamedOrBlankNode = ox_named_node("http://example.org/s").into();
        let p = ox_named_node("http://example.org/p");
        let o: oxigraph::model::Term = ox_named_node("http://example.org/o").into();
        let reifier: NamedOrBlankNode = ox_named_node("http://example.org/reifier").into();
        let reifies = ox_named_node(RDF_REIFIES);
        let confidence = ox_named_node("http://example.org/confidence");
        let meta = ox_simple_literal("0.9");
        let quoted = ox_quoted_triple(s.clone(), p.clone(), o.clone());

        assert!(dataset_has(&dataset, &s, &p, &o));
        assert!(dataset_has(&dataset, &reifier, &reifies, &quoted));
        assert!(dataset_has(&dataset, &reifier, &confidence, &meta));
        assert_no_gmeow_at_id_leak(&dataset, &json);
    }

    #[test]
    fn annotation_reifier_explicit_id_on_value_object() {
        let mut graph = Graph::default();
        graph.terms.push(iri_term("http://example.org/s"));
        graph.terms.push(iri_term("http://example.org/p"));
        graph.terms.push(literal_term("hello"));
        graph.terms.push(iri_term("http://example.org/reifier"));
        graph.terms.push(iri_term("http://example.org/confidence"));
        graph.terms.push(literal_term("0.9"));
        graph.quads.push((0, 1, 2, None));
        graph.reifiers.push((3, (0, 1, 2)));
        graph.annotations.push((3, 4, 5));

        let json = serialize_graph(&graph).expect("serialize");
        let dataset = parse_jsonld_star(json.as_bytes()).expect("parse JSON-LD-star");

        let s: NamedOrBlankNode = ox_named_node("http://example.org/s").into();
        let p = ox_named_node("http://example.org/p");
        let o = ox_simple_literal("hello");
        let reifier: NamedOrBlankNode = ox_named_node("http://example.org/reifier").into();
        let reifies = ox_named_node(RDF_REIFIES);
        let confidence = ox_named_node("http://example.org/confidence");
        let meta = ox_simple_literal("0.9");
        let quoted = ox_quoted_triple(s.clone(), p.clone(), o.clone());

        assert!(dataset_has(&dataset, &s, &p, &o));
        assert!(dataset_has(&dataset, &reifier, &reifies, &quoted));
        assert!(dataset_has(&dataset, &reifier, &confidence, &meta));
        assert_no_gmeow_at_id_leak(&dataset, &json);
    }

    #[test]
    fn annotation_reifier_blank_fallback() {
        let mut graph = Graph::default();
        graph.terms.push(iri_term("http://example.org/s"));
        graph.terms.push(iri_term("http://example.org/p"));
        graph.terms.push(iri_term("http://example.org/o"));
        graph.terms.push(bnode_term("r1"));
        graph.terms.push(iri_term("http://example.org/confidence"));
        graph.terms.push(literal_term("0.9"));
        graph.quads.push((0, 1, 2, None));
        graph.reifiers.push((3, (0, 1, 2)));
        graph.annotations.push((3, 4, 5));

        let json = serialize_graph(&graph).expect("serialize");
        let dataset = parse_jsonld_star(json.as_bytes()).expect("parse JSON-LD-star");

        let s: NamedOrBlankNode = ox_named_node("http://example.org/s").into();
        let p = ox_named_node("http://example.org/p");
        let o: oxigraph::model::Term = ox_named_node("http://example.org/o").into();
        let reifies = ox_named_node(RDF_REIFIES);
        let confidence = ox_named_node("http://example.org/confidence");
        let meta = ox_simple_literal("0.9");
        let quoted = ox_quoted_triple(s.clone(), p.clone(), o.clone());

        assert!(dataset_has(&dataset, &s, &p, &o));

        let reifier_quads: Vec<_> = dataset
            .iter()
            .filter(|q| q.predicate == reifies && oxigraph::model::Term::from(q.object) == quoted)
            .collect();
        assert_eq!(
            reifier_quads.len(),
            1,
            "expected exactly one rdf:reifies quad for the base triple"
        );
        assert!(
            matches!(
                NamedOrBlankNode::from(reifier_quads[0].subject),
                NamedOrBlankNode::BlankNode(_)
            ),
            "blank reifier fallback must use a blank node subject: {json}"
        );
        assert!(dataset_has(
            &dataset,
            &NamedOrBlankNode::from(reifier_quads[0].subject),
            &confidence,
            &meta
        ));
    }

    #[test]
    fn jsonld_star_downcast_to_gmeow_statement_metadata() {
        let mut graph = Graph::default();
        graph.terms.push(iri_term("http://example.org/s"));
        graph.terms.push(iri_term("http://example.org/p"));
        graph.terms.push(iri_term("http://example.org/o"));
        graph.terms.push(iri_term("http://example.org/r"));
        graph.terms.push(iri_term("http://example.org/confidence"));
        graph.terms.push(literal_term("0.9"));
        graph.quads.push((0, 1, 2, None));
        graph.reifiers.push((3, (0, 1, 2)));
        graph.annotations.push((3, 4, 5));

        let json = serialize_graph(&graph).expect("serialize");
        let nquads = jsonld_star_to_gmeow_statement_metadata_nquads(json.as_bytes())
            .expect("downcast JSON-LD-star to GMEOW statement metadata");

        // The output must be parseable plain N-Quads (no quoted triple terms).
        let dataset = parse_nquads(&nquads);
        assert!(
            !dataset
                .iter()
                .any(|q| matches!(q.object, oxigraph::model::TermRef::Triple(_))),
            "downcast output must contain no quoted triple terms"
        );

        let s: NamedOrBlankNode = ox_named_node("http://example.org/s").into();
        let p = ox_named_node("http://example.org/p");
        let o: oxigraph::model::Term = ox_named_node("http://example.org/o").into();
        let r: NamedOrBlankNode = ox_named_node("http://example.org/r").into();
        let rdf_type = ox_named_node(RDF_TYPE);
        let statement_metadata = ox_named_node(GMEOW_STATEMENT_METADATA);
        let q_subject = ox_named_node(GMEOW_QSUBJECT);
        let q_predicate = ox_named_node(GMEOW_QPREDICATE);
        let q_object = ox_named_node(GMEOW_QOBJECT);
        let confidence = ox_named_node("http://example.org/confidence");
        let meta = ox_simple_literal("0.9");

        // Base triple is preserved.
        assert!(
            dataset_has(&dataset, &s, &p, &o),
            "base triple must survive downcast"
        );

        // GMEOW statement-metadata skeleton is emitted for the reifier.
        assert!(
            dataset_has(
                &dataset,
                &r,
                &rdf_type,
                &OxTerm::NamedNode(statement_metadata)
            ),
            "reifier must be typed gmeow:StatementMetadata"
        );
        assert!(
            dataset_has(&dataset, &r, &q_subject, &OxTerm::from(s.clone())),
            "gmeow:qSubject must point to quoted subject"
        );
        assert!(
            dataset_has(&dataset, &r, &q_predicate, &OxTerm::NamedNode(p.clone())),
            "gmeow:qPredicate must point to quoted predicate"
        );
        assert!(
            dataset_has(&dataset, &r, &q_object, &o),
            "gmeow:qObject must point to quoted IRI object"
        );

        // Annotation triple on the reifier is preserved.
        assert!(
            dataset_has(&dataset, &r, &confidence, &meta),
            "annotation triple must survive downcast"
        );
    }

    #[test]
    fn jsonld_star_downcast_preserves_literal_object() {
        let mut graph = Graph::default();
        graph.terms.push(iri_term("http://example.org/s"));
        graph.terms.push(iri_term("http://example.org/p"));
        graph.terms.push(lang_term("hello", "en"));
        graph.terms.push(iri_term("http://example.org/r"));
        graph.terms.push(iri_term("http://example.org/confidence"));
        graph.terms.push(literal_term("0.95"));
        graph.quads.push((0, 1, 2, None));
        graph.reifiers.push((3, (0, 1, 2)));
        graph.annotations.push((3, 4, 5));

        let json = serialize_graph(&graph).expect("serialize");
        let nquads = jsonld_star_to_gmeow_statement_metadata_nquads(json.as_bytes())
            .expect("downcast literal-valued JSON-LD-star");
        let dataset = parse_nquads(&nquads);

        let s: NamedOrBlankNode = ox_named_node("http://example.org/s").into();
        let p = ox_named_node("http://example.org/p");
        let o: oxigraph::model::Term =
            oxigraph::model::Literal::new_language_tagged_literal("hello", "en")
                .expect("valid lang literal")
                .into();
        let r: NamedOrBlankNode = ox_named_node("http://example.org/r").into();
        let q_object_literal = ox_named_node(GMEOW_QOBJECTLITERAL);

        assert!(
            dataset_has(&dataset, &s, &p, &o),
            "base literal triple must survive"
        );
        assert!(
            dataset_has(&dataset, &r, &q_object_literal, &o),
            "gmeow:qObjectLiteral must be the literal object"
        );
    }

    #[test]
    fn jsonld_star_downcast_preserves_simple_literal_object() {
        // Equivalent to the removed Python test
        // tests/test_yaml_ld_star.py::test_yamlld_annotated_to_graph_downcasts_to_statement_metadata.
        let mut graph = Graph::default();
        graph.terms.push(iri_term("http://example.org/s"));
        graph.terms.push(iri_term("http://example.org/p"));
        graph.terms.push(literal_term("hello"));
        graph.terms.push(iri_term("http://example.org/r"));
        graph
            .terms
            .push(iri_term("https://blackcatinformatics.ca/gmeow/confidence"));
        graph.terms.push(literal_term("0.9"));
        graph.quads.push((0, 1, 2, None));
        graph.reifiers.push((3, (0, 1, 2)));
        graph.annotations.push((3, 4, 5));

        let json = serialize_graph(&graph).expect("serialize");
        let nquads = jsonld_star_to_gmeow_statement_metadata_nquads(json.as_bytes())
            .expect("downcast simple-literal JSON-LD-star");

        // The output must be parseable plain N-Quads (no quoted triple terms).
        let dataset = parse_nquads(&nquads);
        assert!(
            !dataset
                .iter()
                .any(|q| matches!(q.object, oxigraph::model::TermRef::Triple(_))),
            "downcast output must contain no quoted triple terms"
        );

        let s: NamedOrBlankNode = ox_named_node("http://example.org/s").into();
        let p = ox_named_node("http://example.org/p");
        let o = ox_simple_literal("hello");
        let r: NamedOrBlankNode = ox_named_node("http://example.org/r").into();
        let rdf_type = ox_named_node(RDF_TYPE);
        let statement_metadata = ox_named_node(GMEOW_STATEMENT_METADATA);
        let q_subject = ox_named_node(GMEOW_QSUBJECT);
        let q_predicate = ox_named_node(GMEOW_QPREDICATE);
        let q_object_literal = ox_named_node(GMEOW_QOBJECTLITERAL);
        let confidence = ox_named_node("https://blackcatinformatics.ca/gmeow/confidence");
        let meta = ox_simple_literal("0.9");

        assert!(
            dataset_has(&dataset, &s, &p, &o),
            "base triple must survive"
        );
        assert!(
            dataset_has(
                &dataset,
                &r,
                &rdf_type,
                &OxTerm::NamedNode(statement_metadata)
            ),
            "reifier must be typed gmeow:StatementMetadata"
        );
        assert!(
            dataset_has(&dataset, &r, &q_subject, &OxTerm::from(s.clone())),
            "gmeow:qSubject must point to quoted subject"
        );
        assert!(
            dataset_has(&dataset, &r, &q_predicate, &OxTerm::NamedNode(p.clone())),
            "gmeow:qPredicate must point to quoted predicate"
        );
        assert!(
            dataset_has(&dataset, &r, &q_object_literal, &o),
            "gmeow:qObjectLiteral must point to quoted literal object"
        );
        assert!(
            dataset_has(&dataset, &r, &confidence, &meta),
            "annotation triple must survive downcast"
        );
    }

    #[test]
    fn jsonld_star_downcast_preserves_typed_literal_annotation() {
        // Equivalent to the removed Python test
        // tests/test_transpile.py::test_transpile_yaml_ld_star_preserves_statement_metadata.
        // The Rust side cannot run the Python up-projection lane, so this test
        // verifies the prerequisite: the JSON-LD-star downcast emits native
        // GMEOW statement-metadata structural terms and preserves the typed
        // annotation, which is what lets the up-projection pass them through.
        let mut graph = Graph::default();
        graph.terms.push(iri_term("https://example.org/alice"));
        graph.terms.push(iri_term("https://schema.org/name"));
        graph.terms.push(literal_term("Alice"));
        graph
            .terms
            .push(iri_term("https://example.org/claim-alice-name"));
        graph
            .terms
            .push(iri_term("https://blackcatinformatics.ca/gmeow/confidence"));
        graph
            .terms
            .push(iri_term("http://www.w3.org/2001/XMLSchema#decimal"));
        graph.terms.push(Term {
            kind: TermKind::Literal,
            value: Some("0.9".to_string()),
            datatype: Some(5),
            lang: None,
            direction: None,
            reifier: None,
        });
        graph.quads.push((0, 1, 2, None));
        graph.reifiers.push((3, (0, 1, 2)));
        graph.annotations.push((3, 4, 6));

        let json = serialize_graph(&graph).expect("serialize");
        let nquads = jsonld_star_to_gmeow_statement_metadata_nquads(json.as_bytes())
            .expect("downcast schema-org-like JSON-LD-star");

        let dataset = parse_nquads(&nquads);
        assert!(
            !dataset
                .iter()
                .any(|q| matches!(q.object, oxigraph::model::TermRef::Triple(_))),
            "downcast output must contain no quoted triple terms"
        );

        let alice: NamedOrBlankNode = ox_named_node("https://example.org/alice").into();
        let schema_name = ox_named_node("https://schema.org/name");
        let alice_name = ox_simple_literal("Alice");
        let claim: NamedOrBlankNode = ox_named_node("https://example.org/claim-alice-name").into();
        let rdf_type = ox_named_node(RDF_TYPE);
        let statement_metadata = ox_named_node(GMEOW_STATEMENT_METADATA);
        let q_subject = ox_named_node(GMEOW_QSUBJECT);
        let q_predicate = ox_named_node(GMEOW_QPREDICATE);
        let q_object_literal = ox_named_node(GMEOW_QOBJECTLITERAL);
        let confidence = ox_named_node("https://blackcatinformatics.ca/gmeow/confidence");
        let meta: oxigraph::model::Term = oxigraph::model::Literal::new_typed_literal(
            "0.9",
            ox_named_node("http://www.w3.org/2001/XMLSchema#decimal"),
        )
        .into();

        assert!(
            dataset_has(&dataset, &alice, &schema_name, &alice_name),
            "base triple must survive"
        );
        assert!(
            dataset_has(
                &dataset,
                &claim,
                &rdf_type,
                &OxTerm::NamedNode(statement_metadata)
            ),
            "reifier must be typed gmeow:StatementMetadata"
        );
        assert!(
            dataset_has(&dataset, &claim, &q_subject, &OxTerm::from(alice.clone())),
            "gmeow:qSubject must point to quoted subject"
        );
        assert!(
            dataset_has(
                &dataset,
                &claim,
                &q_predicate,
                &OxTerm::NamedNode(schema_name.clone())
            ),
            "gmeow:qPredicate must point to quoted predicate"
        );
        assert!(
            dataset_has(&dataset, &claim, &q_object_literal, &alice_name),
            "gmeow:qObjectLiteral must point to quoted literal object"
        );
        assert!(
            dataset_has(&dataset, &claim, &confidence, &meta),
            "typed annotation triple must survive downcast"
        );
    }

    /// Load a Turtle-star file into an oxigraph [`Dataset`] without Store
    /// canonicalization, preserving lexical forms from the committed artifact.
    fn load_turtle_dataset(path: &std::path::Path) -> Result<Dataset, PipelineError> {
        let bytes = std::fs::read(path)
            .map_err(|e| PipelineError::Parse(format!("read {}: {e}", path.display())))?;
        let ir = gmeow_rdf::parse_dataset(&bytes, "text/turtle", None)
            .map_err(|e| PipelineError::Parse(format!("Turtle parse: {e}")))?;
        let quads = gmeow_rdf::oxigraph::flat_oxigraph_quads_from_dataset(&ir)
            .map_err(|e| PipelineError::Parse(format!("IR → quads: {e}")))?;
        Ok(quads.into_iter().collect())
    }

    /// Convert an oxigraph term into the gmeow-rdf owned model.
    fn rdf_term_from_oxigraph_term(term: &OxTerm) -> RdfTerm {
        match term {
            OxTerm::NamedNode(node) => RdfTerm::iri(node.as_str()),
            OxTerm::BlankNode(node) => RdfTerm::blank_node(node.as_str()),
            OxTerm::Literal(literal) => RdfTerm::literal(RdfLiteral {
                lexical_form: literal.value().to_owned(),
                datatype: Some(literal.datatype().as_str().to_owned()),
                language: literal.language().map(str::to_owned),
                direction: literal.direction().map(|direction| match direction {
                    oxigraph::model::BaseDirection::Ltr => RdfTextDirection::Ltr,
                    oxigraph::model::BaseDirection::Rtl => RdfTextDirection::Rtl,
                }),
            }),
            OxTerm::Triple(triple) => RdfTerm::triple(rdf_triple_from_oxigraph_term(triple)),
        }
    }

    fn rdf_triple_from_oxigraph_term(triple: &oxigraph::model::Triple) -> RdfTriple {
        let subject = match &triple.subject {
            NamedOrBlankNode::NamedNode(node) => RdfTerm::iri(node.as_str()),
            NamedOrBlankNode::BlankNode(node) => RdfTerm::blank_node(node.as_str()),
        };
        RdfTriple::new(
            subject,
            triple.predicate.as_str(),
            rdf_term_from_oxigraph_term(&triple.object),
        )
    }

    /// Intern an owned RDF term into an [`RdfDatasetBuilder`], recursing into
    /// triple terms so nested quoted triples are preserved.
    fn intern_rdf_term(
        builder: &mut RdfDatasetBuilder,
        term: &RdfTerm,
    ) -> Result<TermId, PipelineError> {
        Ok(match term {
            RdfTerm::Iri(iri) => builder.intern_iri(iri.clone()),
            RdfTerm::BlankNode(label) => builder.intern_blank(label.clone(), BlankScope::DEFAULT),
            RdfTerm::Literal(lit) => builder.intern_literal(lit.clone()),
            RdfTerm::Triple(triple) => {
                let s = intern_rdf_term(builder, &triple.subject)?;
                let p = builder.intern_iri(triple.predicate.clone());
                let o = intern_rdf_term(builder, &triple.object)?;
                builder.intern_triple(s, p, o)
            }
        })
    }

    /// Build a gmeow-rdf [`RdfDataset`] from an oxigraph dataset, separating
    /// RDF 1.2 reifier bindings (`?r rdf:reifies << ?s ?p ?o >>`) and their
    /// annotation triples from regular quads.
    fn rdf_dataset_from_oxigraph_dataset(
        dataset: &Dataset,
    ) -> Result<Arc<gmeow_rdf::RdfDataset>, PipelineError> {
        let quads: Vec<Quad> = dataset.iter().map(|q| q.into_owned()).collect();

        let mut reifier_subjects: HashSet<NamedOrBlankNode> = HashSet::new();
        for quad in &quads {
            if quad.predicate.as_str() == RDF_REIFIES && matches!(&quad.object, OxTerm::Triple(_)) {
                reifier_subjects.insert(quad.subject.clone());
            }
        }

        let mut builder = RdfDatasetBuilder::new();
        for quad in &quads {
            // Reifier binding: move to the reifier table, not the quad table.
            if quad.predicate.as_str() == RDF_REIFIES {
                if let OxTerm::Triple(triple) = &quad.object {
                    let reifier = intern_rdf_term(
                        &mut builder,
                        &rdf_term_from_oxigraph_term(&OxTerm::from(quad.subject.clone())),
                    )?;
                    let s = intern_rdf_term(
                        &mut builder,
                        &rdf_term_from_oxigraph_term(&OxTerm::from(triple.subject.clone())),
                    )?;
                    let p = builder.intern_iri(triple.predicate.as_str().to_string());
                    let o = intern_rdf_term(
                        &mut builder,
                        &rdf_term_from_oxigraph_term(&triple.object),
                    )?;
                    let triple_id = builder.intern_triple(s, p, o);
                    builder.push_reifier(reifier, triple_id);
                    continue;
                }
            }

            // Annotation triple on a known reifier: move to the annotation table.
            if reifier_subjects.contains(&quad.subject) {
                let reifier = intern_rdf_term(
                    &mut builder,
                    &rdf_term_from_oxigraph_term(&OxTerm::from(quad.subject.clone())),
                )?;
                let p = builder.intern_iri(quad.predicate.as_str().to_string());
                let o = intern_rdf_term(&mut builder, &rdf_term_from_oxigraph_term(&quad.object))?;
                builder.push_annotation(reifier, p, o);
                continue;
            }

            // Regular quad.
            let q = rdf_quad_from_oxigraph(quad);
            let s = intern_rdf_term(&mut builder, &q.subject)?;
            let p = builder.intern_iri(q.predicate);
            let o = intern_rdf_term(&mut builder, &q.object)?;
            let g = match &q.graph_name {
                Some(term) => Some(intern_rdf_term(&mut builder, term)?),
                None => None,
            };
            builder.push_quad(s, p, o, g);
        }

        builder
            .freeze()
            .map_err(|e| PipelineError::Parse(format!("freeze RdfDataset: {e}")))
    }

    /// Fold an [`RdfDataset`] into a gmeow-gts [`Graph`] via the production GTS
    /// writer/reader seam.
    fn graph_from_rdf_dataset(
        dataset: &Arc<gmeow_rdf::RdfDataset>,
    ) -> Result<Graph, PipelineError> {
        let bytes = gmeow_rdf::gts_write::to_gts(
            dataset,
            &RdfLookaside::default(),
            "yaml-ld-full-roundtrip",
        )
        .map_err(|e| PipelineError::Parse(format!("to_gts: {e}")))?;
        gmeow_rdf::gts::read_graph(&bytes, false)
            .map_err(|e| PipelineError::Parse(format!("read_graph: {e}")))
    }

    fn repo_root() -> PathBuf {
        let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        manifest
            .parent()
            .expect("workspace parent")
            .parent()
            .expect("repository root")
            .to_path_buf()
    }

    /// Full-graph round-trip gate for the committed RDF 1.2 statement artifact
    /// (#699 acceptance criterion #2, PR #978).
    #[test]
    fn committed_rdf12_statements_roundtrip_through_jsonld_star() {
        let path = repo_root().join("generated/statements/gmeow.rdf12.ttl");

        let original = load_turtle_dataset(&path)
            .expect("load committed generated/statements/gmeow.rdf12.ttl");
        let dataset = rdf_dataset_from_oxigraph_dataset(&original)
            .expect("convert committed artifact to RdfDataset");
        let graph = graph_from_rdf_dataset(&dataset).expect("fold committed artifact to GTS graph");
        let json = serialize_graph(&graph).expect("serialize GTS graph to JSON-LD-star");
        let roundtrip = parse_jsonld_star(json.as_bytes())
            .expect("parse JSON-LD-star back to oxigraph dataset");

        assert_eq!(
            canonical_lines(&original),
            canonical_lines(&roundtrip),
            "committed generated/statements/gmeow.rdf12.ttl must round-trip through JSON-LD-star"
        );
    }

    /// Optional full-graph round-trip gate for the built `dist/gmeow.jsonld`
    /// artifact. Requires `make build` to have produced `dist/gmeow.jsonld`;
    /// skipped silently when the file is absent so `cargo test` still passes
    /// in a source-only checkout.
    #[test]
    fn dist_jsonld_roundtrips_through_oxigraph() {
        let path = repo_root().join("dist/gmeow.jsonld");
        if !path.exists() {
            eprintln!("dist/gmeow.jsonld not present; run `make build` to exercise this test");
            return;
        }

        let original = parse_jsonld_star(&std::fs::read(&path).expect("read dist/gmeow.jsonld"))
            .expect("parse built dist/gmeow.jsonld");
        let dataset = rdf_dataset_from_oxigraph_dataset(&original)
            .expect("convert dist JSON-LD-star to RdfDataset");
        let graph = graph_from_rdf_dataset(&dataset).expect("fold dist artifact to GTS graph");
        let json = serialize_graph(&graph).expect("re-serialize GTS graph to JSON-LD-star");
        let roundtrip = parse_jsonld_star(json.as_bytes())
            .expect("parse re-serialized JSON-LD-star back to oxigraph dataset");

        assert_eq!(
            canonical_lines(&original),
            canonical_lines(&roundtrip),
            "built dist/gmeow.jsonld must round-trip through JSON-LD-star"
        );
    }

    /// Convert a hand-authored YAML-LD-star file to its JSON-LD-star form so the
    /// narrow Rust parser path can consume it.
    fn yaml_file_to_jsonld_star(path: &std::path::Path) -> String {
        let yaml = std::fs::read_to_string(path)
            .unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
        let value: serde_yaml::Value =
            serde_yaml::from_str(&yaml).expect("YAML-LD-star fixture is valid YAML");
        serde_json::to_string(&value).expect("YAML-LD-star -> JSON-LD-star")
    }

    /// Acceptance criterion #5 (issue #699 / PR #978): a hand-authored YAML-LD-star
    /// statement-layer fixture losslessly transpiles into GMEOW through the Rust
    /// native downcast path exposed to Python as
    /// `parse_jsonld_star_to_gmeow_statement_metadata_nquads`.
    #[test]
    fn hand_authored_yaml_ld_star_fixture_transpiles_to_gmeow() {
        let path = repo_root().join("slices/core/standpoint/examples/claim-bullshit.yamlld");
        let json = yaml_file_to_jsonld_star(&path);
        let nquads = jsonld_star_to_gmeow_statement_metadata_nquads(json.as_bytes())
            .expect("downcast YAML-LD-star fixture to GMEOW statement metadata");

        let dataset = parse_nquads(&nquads);
        assert!(
            !dataset
                .iter()
                .any(|q| matches!(q.object, oxigraph::model::TermRef::Triple(_))),
            "transpiled output must contain no RDF 1.2 quoted triple terms"
        );

        let claim: NamedOrBlankNode = ox_named_node("https://example.org/claim-001").into();
        let alice: NamedOrBlankNode = ox_named_node("https://example.org/alice").into();
        let analyst: NamedOrBlankNode =
            ox_named_node("https://example.org/analyst-standpoint").into();
        let bullshit: oxigraph::model::Term =
            ox_named_node("https://blackcatinformatics.ca/gmeow/bullshit").into();

        let rdf_type = ox_named_node(RDF_TYPE);
        let standpoint_claim = ox_named_node(GMEOW_STATEMENT_METADATA);
        let claim_modality = ox_named_node("https://blackcatinformatics.ca/gmeow/claimModality");
        let observed_feature =
            ox_named_node("https://blackcatinformatics.ca/gmeow/observedFeature");
        let name = ox_named_node("https://blackcatinformatics.ca/gmeow/name");
        let q_subject = ox_named_node(GMEOW_QSUBJECT);
        let q_predicate = ox_named_node(GMEOW_QPREDICATE);
        let q_object = ox_named_node(GMEOW_QOBJECT);
        let q_object_literal = ox_named_node(GMEOW_QOBJECTLITERAL);
        let according_to = ox_named_node("https://blackcatinformatics.ca/gmeow/accordingTo");
        let confidence = ox_named_node("https://blackcatinformatics.ca/gmeow/confidence");
        let asserted_at = ox_named_node("https://blackcatinformatics.ca/gmeow/assertedAt");

        // Base triples survive.
        assert!(
            dataset_has(&dataset, &claim, &claim_modality, &bullshit),
            "claimModality base triple must survive transpile"
        );
        assert!(
            dataset_has(
                &dataset,
                &claim,
                &observed_feature,
                &OxTerm::from(alice.clone())
            ),
            "observedFeature base triple must survive transpile"
        );

        // Directional language string is preserved on the base literal triple.
        let alice_name: oxigraph::model::Term =
            oxigraph::model::Literal::new_directional_language_tagged_literal(
                "Alice",
                "en",
                oxigraph::model::BaseDirection::Ltr,
            )
            .expect("valid directional literal")
            .into();
        assert!(
            dataset_has(&dataset, &alice, &name, &alice_name),
            "directional language-tagged name must survive transpile"
        );

        // Explicit reifier for the claim modality is typed StatementMetadata and
        // carries the quoted subject/predicate/object skeleton.
        let claim_annotation: NamedOrBlankNode =
            ox_named_node("https://example.org/claim-001-annotation").into();
        assert!(
            dataset_has(
                &dataset,
                &claim_annotation,
                &rdf_type,
                &OxTerm::NamedNode(standpoint_claim.clone())
            ),
            "explicit reifier must be typed gmeow:StatementMetadata"
        );
        assert!(
            dataset_has(
                &dataset,
                &claim_annotation,
                &q_subject,
                &OxTerm::from(claim.clone())
            ),
            "gmeow:qSubject must point to the claim"
        );
        assert!(
            dataset_has(
                &dataset,
                &claim_annotation,
                &q_predicate,
                &OxTerm::NamedNode(claim_modality.clone())
            ),
            "gmeow:qPredicate must point to claimModality"
        );
        assert!(
            dataset_has(&dataset, &claim_annotation, &q_object, &bullshit),
            "gmeow:qObject must point to the IRI object"
        );

        // Annotation triples on the explicit reifier survive.
        assert!(
            dataset_has(
                &dataset,
                &claim_annotation,
                &according_to,
                &OxTerm::from(analyst.clone())
            ),
            "accordingTo annotation must survive transpile"
        );
        let confidence_value: oxigraph::model::Term = oxigraph::model::Literal::new_typed_literal(
            "0.65",
            ox_named_node("http://www.w3.org/2001/XMLSchema#decimal"),
        )
        .into();
        assert!(
            dataset_has(&dataset, &claim_annotation, &confidence, &confidence_value),
            "confidence annotation must survive transpile"
        );
        let asserted_value: oxigraph::model::Term = oxigraph::model::Literal::new_typed_literal(
            "2026-06-05T00:00:00Z",
            ox_named_node("http://www.w3.org/2001/XMLSchema#dateTime"),
        )
        .into();
        assert!(
            dataset_has(&dataset, &claim_annotation, &asserted_at, &asserted_value),
            "assertedAt annotation must survive transpile"
        );

        // Explicit reifier for the directional-language name uses qObjectLiteral.
        let name_annotation: NamedOrBlankNode =
            ox_named_node("https://example.org/alice-name-annotation").into();
        assert!(
            dataset_has(
                &dataset,
                &name_annotation,
                &rdf_type,
                &OxTerm::NamedNode(standpoint_claim)
            ),
            "name reifier must be typed gmeow:StatementMetadata"
        );
        assert!(
            dataset_has(
                &dataset,
                &name_annotation,
                &q_subject,
                &OxTerm::from(alice.clone())
            ),
            "name gmeow:qSubject must point to alice"
        );
        assert!(
            dataset_has(
                &dataset,
                &name_annotation,
                &q_predicate,
                &OxTerm::NamedNode(name)
            ),
            "name gmeow:qPredicate must point to name"
        );
        assert!(
            dataset_has(&dataset, &name_annotation, &q_object_literal, &alice_name),
            "name gmeow:qObjectLiteral must point to the directional literal"
        );
    }

    /// Issue #699 / PR #978 MEDIUM gap #7 item 4: a sample `@annotation` fragment
    /// shaped like serializer output must validate against the SHACL-derived JSON
    /// Schema `$defs/Annotation` from #700.
    #[test]
    fn annotation_fragment_validates_against_json_schema() {
        use std::path::Path;

        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .canonicalize()
            .expect("repo root");
        let schema_path = root.join("generated/schemas/gmeow.schema.json");
        let schema_bytes = std::fs::read(&schema_path)
            .unwrap_or_else(|e| panic!("cannot read {}: {e}", schema_path.display()));
        let mut schema: Value =
            serde_json::from_slice(&schema_bytes).expect("schema is valid JSON");

        // Validate a sample annotation object (the value inside `@annotation`) by
        // rooting the schema at `#/$defs/Annotation`.
        schema.as_object_mut().expect("schema is an object").insert(
            "$ref".to_string(),
            Value::String("#/$defs/Annotation".to_string()),
        );
        // Remove the anyOf at the root so the `$ref` is unambiguous.
        schema.as_object_mut().unwrap().remove("anyOf");
        schema.as_object_mut().unwrap().remove("properties");
        schema.as_object_mut().unwrap().remove("type");

        let validator = jsonschema::options()
            .with_draft(jsonschema::Draft::Draft202012)
            .build(&schema)
            .expect("annotation subschema compiles");

        // Sample fragment mirroring the annotation objects the serializer emits:
        // typed-literal value objects (string `@value` + `@type`) and IRI
        // objects. The serializer never emits a bare JSON number for a literal;
        // numeric values are emitted as typed-literal objects, so the sample
        // mirrors that real shape.
        let fragment = serde_json::json!({
            "gmeow:confidence": {"@value": "0.9", "@type": "xsd:decimal"},
            "gmeow:accordingTo": {"@id": "http://example.org/source"},
            "gmeow:assertedAt": {"@value": "2026-06-05T00:00:00Z", "@type": "xsd:dateTime"}
        });

        let errors: Vec<String> = validator
            .iter_errors(&fragment)
            .map(|e| e.to_string())
            .collect();
        assert!(
            errors.is_empty(),
            "sample @annotation fragment must validate against #700 $defs/Annotation: {errors:?}"
        );
    }

    fn canonical_nquads(dataset: &Dataset) -> String {
        let mut quads: Vec<String> = dataset.iter().map(|q| q.to_string()).collect();
        quads.sort();
        quads.join("\n")
    }

    #[test]
    fn yaml_ld_star_ingest_rejects_anchors() {
        let anchored = "anchor: &a {x: 1}\nalias: *a\n";
        let err = yaml_ld_star_to_json(anchored.as_bytes())
            .expect_err("YAML anchors/aliases must hard-fail");
        assert!(
            matches!(err, PipelineError::Decode(_)),
            "expected a Decode error, got {err:?}"
        );
    }

    #[test]
    fn roundtrip_isomorphic_accepts_emitted_jsonld() {
        let graph = minimal_graph();
        let json = serialize_graph(&graph).expect("serialize JSON-LD-star");
        let nquads = gmeow_gts::nquads::to_nquads(&graph);
        assert!(
            roundtrip_isomorphic(nquads.as_bytes(), json.as_bytes(), "jsonld")
                .expect("roundtrip_isomorphic for jsonld"),
            "emitted JSON-LD-star must round-trip isomorphic to the source N-Quads-star"
        );
    }

    #[test]
    fn roundtrip_isomorphic_accepts_emitted_yamlld() {
        let graph = minimal_graph();
        let yaml = serialize_graph_yaml(&graph, None).expect("serialize YAML-LD-star");
        let nquads = gmeow_gts::nquads::to_nquads(&graph);
        assert!(
            roundtrip_isomorphic(nquads.as_bytes(), yaml.as_bytes(), "yamlld")
                .expect("roundtrip_isomorphic for yamlld"),
            "emitted YAML-LD-star must round-trip isomorphic to the source N-Quads-star"
        );
    }

    /// Acceptance criterion #5 (issue #699 / PR #978): the YAML-LD-star lift
    /// through `yaml_ld_star_to_gmeow_statement_metadata_nquads` produces a
    /// graph that is RDFC-1.0 canonically equal to the native Turtle
    /// (StatementMetadata) authoring of the same claim.
    ///
    /// Uses an explicit reifier `@id` so the downcast emits a stable IRI
    /// reifier (not a fresh blank node), allowing the Turtle counterpart to
    /// match exactly.
    #[test]
    fn yaml_ld_star_lift_equals_turtle_lift() {
        // ── 1. Minimal YAML-LD-star document ─────────────────────────────────
        // One base triple ex:s ex:p ex:o annotated on reifier ex:r with two
        // metadata predicates: gmeow:claimModality and gmeow:accordingTo.
        const YAML_DOC: &str = r#"
"@context":
  ex: "https://example.org/"
  gmeow: "https://blackcatinformatics.ca/gmeow/"
  xsd: "http://www.w3.org/2001/XMLSchema#"
"@graph":
  - "@id": "ex:s"
    "ex:p":
      "@id": "ex:o"
      "@annotation":
        "@id": "ex:r"
        "gmeow:claimModality":
          "@id": "gmeow:assertion"
        "gmeow:accordingTo":
          "@id": "ex:source1"
"#;

        // ── 2. Run the YAML-LD-star lift ──────────────────────────────────────
        let nquads = yaml_ld_star_to_gmeow_statement_metadata_nquads(YAML_DOC.as_bytes())
            .expect("YAML-LD-star lift must succeed");

        // ── 3. Guard: no RDF-1.2 quoted-triple terms in the output ────────────
        let yaml_lift = parse_nquads(&nquads);
        assert!(
            !yaml_lift
                .iter()
                .any(|q| matches!(q.object, oxigraph::model::TermRef::Triple(_))),
            "transpiled output must contain no RDF 1.2 quoted triple terms"
        );

        // ── 4. Build the equivalent native Turtle (StatementMetadata) ─────────
        // This Turtle reproduces EXACTLY the triples the downcast emits:
        //   • the base triple ex:s ex:p ex:o
        //   • ex:r rdf:type gmeow:StatementMetadata
        //   • ex:r gmeow:qSubject ex:s
        //   • ex:r gmeow:qPredicate ex:p
        //   • ex:r gmeow:qObject ex:o
        //   • ex:r gmeow:claimModality gmeow:assertion
        //   • ex:r gmeow:accordingTo ex:source1
        const TURTLE_DOC: &str = r#"
@prefix ex:    <https://example.org/> .
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix rdf:   <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .

ex:s ex:p ex:o .

ex:r a gmeow:StatementMetadata ;
     gmeow:qSubject   ex:s ;
     gmeow:qPredicate ex:p ;
     gmeow:qObject    ex:o ;
     gmeow:claimModality gmeow:assertion ;
     gmeow:accordingTo   ex:source1 .
"#;

        let turtle_ir = gmeow_rdf::parse_dataset(TURTLE_DOC.as_bytes(), "text/turtle", None)
            .expect("Turtle parse must succeed");
        let turtle_quads = gmeow_rdf::oxigraph::flat_oxigraph_quads_from_dataset(&turtle_ir)
            .expect("IR → quads must succeed");
        let turtle_lift: Dataset = turtle_quads.into_iter().collect();

        // ── 5. RDFC-1.0 canonical equality: lift ≡ native Turtle ─────────────
        let yaml_lines = canonical_lines(&yaml_lift);
        let turtle_lines = canonical_lines(&turtle_lift);

        // Sanity: both graphs must be non-empty (guard against trivially-matching
        // empty datasets) and of the same size.
        assert!(
            !yaml_lines.is_empty(),
            "YAML-LD-star lift must produce at least one quad"
        );
        assert_eq!(
            yaml_lines.len(),
            turtle_lines.len(),
            "YAML-LD-star lift and native Turtle must have the same quad count"
        );

        assert_eq!(
            yaml_lines, turtle_lines,
            "YAML-LD-star lift must equal the native StatementMetadata Turtle authoring \
             (AC#5 lossless-into-GMEOW)"
        );
    }
}
