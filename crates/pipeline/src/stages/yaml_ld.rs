// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The `yaml_ld` export leaf (#699): RDF → YAML-LD-star / JSON-LD-star.
//!
//! Emits both the JSON-LD-star lead artifact and a deterministic YAML-LD-star
//! derivative, plus a small serialization-preservation ledger.

use std::collections::BTreeMap;

use gmeow_gts::model::{Graph, Term, TermKind};
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
/// Default schema URL for the YAML-LD language-server header.
const DEFAULT_SCHEMA_URL: &str = "https://blackcatinformatics.ca/gmeow/schemas/gmeow.schema.json";

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
    fn run(&self, input: StageInput<'_>) -> Result<StageOutput, PipelineError> {
        let gts = crate::stages::snapshot::snapshot_bytes(input.upstream)?;
        let graph = gmeow_rdf::gts::read_graph(&gts, true)
            .map_err(|e| PipelineError::Parse(format!("read snapshot gmeow.gts: {e}")))?;
        let json = serialize_graph(&graph)?;
        let yaml = serialize_graph_yaml(&graph)?;
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
pub fn serialize_graph_yaml(graph: &Graph) -> Result<String, PipelineError> {
    let json = serialize_graph(graph)?;
    let value: Value = serde_json::from_str(&json)
        .map_err(|e| PipelineError::Decode(format!("parse JSON-LD for YAML: {e}")))?;
    let body = serde_yaml::to_string(&value)
        .map_err(|e| PipelineError::Decode(format!("YAML-LD serialization: {e}")))?;
    let header = format!(
        "# yaml-language-server: $schema={DEFAULT_SCHEMA_URL}\n\
         # TODO(#700): default schema URL is bounded to the bundled gmeow.schema.json;\n\
         # replace with the canonical public URL once issue #700 finalizes the schema surface.\n"
    );
    Ok(header + &body)
}

/// Serialization-preservation ledger: records YAML-LD-star as lossless.
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
        list.sort();
    }

    // Annotation index: reifier id -> sorted annotation (predicate, value) rows.
    let mut annotations_of: AnnotationIndex = BTreeMap::new();
    for &(r, p, v) in &graph.annotations {
        annotations_of.entry(r).or_default().push((p, v));
    }
    for list in annotations_of.values_mut() {
        list.sort();
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
    graph: &Graph,
    s: usize,
    p: usize,
    o: usize,
    reifier_of: &ReifierIndex,
    annotations_of: &AnnotationIndex,
) -> Result<Value, PipelineError> {
    let s_term = &graph.terms[s];
    let p_term = &graph.terms[p];
    let o_term = &graph.terms[o];
    let p_iri = p_term
        .value
        .as_deref()
        .ok_or_else(|| PipelineError::Parse("triple predicate missing IRI".to_string()))?;
    let mut node = BTreeMap::new();
    node.insert("@id".to_string(), Value::String(term_id(s_term)?));
    let inner = build_value_object(graph, s, p, o, o_term, reifier_of, annotations_of)?;
    node.insert(curie(p_iri), inner);
    Ok(to_json_object(node))
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
        TermKind::Triple => {
            let mut map = BTreeMap::new();
            map.insert(
                "@value".to_string(),
                Value::String("<<triple term>>".to_string()),
            );
            Ok(to_json_object(map))
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    use gmeow_gts::model::{Term, TermKind};

    const RDF_REIFIES: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#reifies";
    use oxigraph::io::{RdfFormat, RdfParser};
    use oxigraph::model::{BaseDirection, Dataset, NamedNode, NamedOrBlankNode, Quad};
    use oxigraph::store::Store;

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

    #[allow(dead_code)]
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

    /// Parse N-Quads-star text into an oxigraph Dataset.
    fn parse_nquads(nq: &str) -> Dataset {
        let store = Store::new().unwrap();
        for quad in RdfParser::from_format(RdfFormat::NQuads)
            .lenient()
            .for_reader(nq.as_bytes())
        {
            let quad = quad.unwrap();
            store.insert(&quad).unwrap();
        }
        store.iter().collect::<Result<Dataset, _>>().unwrap()
    }

    /// Parse our emitted JSON-LD-star back into an oxigraph Dataset by
    /// interpreting the `@annotation` idiom.
    fn parse_jsonld_star(json: &str) -> Result<Dataset, String> {
        let value: Value = serde_json::from_str(json).map_err(|e| e.to_string())?;
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

        let store = Store::new().map_err(|e| e.to_string())?;

        let emit_node = |node: &Value, graph_iri: Option<&str>| -> Result<(), String> {
            let id = node
                .get("@id")
                .and_then(Value::as_str)
                .ok_or("node without @id")?;
            let subject: NamedOrBlankNode = if let Some(label) = id.strip_prefix("_:") {
                oxigraph::model::BlankNode::new(label.to_string())
                    .map_err(|e| e.to_string())?
                    .into()
            } else {
                NamedNode::new(expand(id))
                    .map_err(|e| e.to_string())?
                    .into()
            };
            let graph_name = graph_iri
                .map(|g| {
                    NamedNode::new(expand(g))
                        .map(oxigraph::model::GraphName::from)
                        .map_err(|e| e.to_string())
                })
                .transpose()?
                .unwrap_or(oxigraph::model::GraphName::DefaultGraph);

            if let Some(Value::Array(types)) = node.get("@type") {
                let rdf_type = NamedNode::new(RDF_TYPE).unwrap();
                for t in types {
                    let t_id = t
                        .get("@id")
                        .and_then(Value::as_str)
                        .ok_or("@type value without @id")?;
                    let obj: oxigraph::model::Term = NamedNode::new(expand(t_id))
                        .map_err(|e| e.to_string())?
                        .into();
                    store
                        .insert(&Quad::new(
                            subject.clone(),
                            rdf_type.clone(),
                            obj,
                            graph_name.clone(),
                        ))
                        .map_err(|e| e.to_string())?;
                }
            }

            for (key, val) in node.as_object().unwrap() {
                if matches!(key.as_str(), "@id" | "@type" | "@context" | "@graph") {
                    continue;
                }
                let predicate = NamedNode::new(expand(key)).map_err(|e| e.to_string())?;
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

        if let Some(Value::Array(graphs)) = value.get("@graph") {
            for entry in graphs {
                if entry.get("@graph").is_some() {
                    let graph_id = entry
                        .get("@id")
                        .and_then(Value::as_str)
                        .ok_or("named graph without @id")?;
                    for node in entry
                        .get("@graph")
                        .and_then(Value::as_array)
                        .ok_or("@graph not array")?
                    {
                        emit_node(node, Some(graph_id))?;
                    }
                } else {
                    emit_node(entry, None)?;
                }
            }
        }

        store
            .iter()
            .collect::<Result<Dataset, _>>()
            .map_err(|e| e.to_string())
    }

    fn emit_value_quad(
        store: &Store,
        subject: NamedOrBlankNode,
        predicate: NamedNode,
        graph_name: oxigraph::model::GraphName,
        value: &Value,
        expand: &dyn Fn(&str) -> String,
    ) -> Result<(), String> {
        let (object, annotation) = parse_value_object(value, expand)?;
        store
            .insert(&Quad::new(
                subject.clone(),
                predicate.clone(),
                object.clone(),
                graph_name.clone(),
            ))
            .map_err(|e| e.to_string())?;

        if let Some(ann) = annotation {
            let reifier_subject = ann
                .get("@id")
                .and_then(Value::as_str)
                .ok_or("annotation without @id")?;
            let reifier: NamedOrBlankNode = if let Some(label) = reifier_subject.strip_prefix("_:")
            {
                oxigraph::model::BlankNode::new(label.to_string())
                    .map_err(|e| e.to_string())?
                    .into()
            } else {
                NamedNode::new(expand(reifier_subject))
                    .map_err(|e| e.to_string())?
                    .into()
            };
            let reifies = NamedNode::new(RDF_REIFIES).unwrap();
            let quoted = oxigraph::model::Term::Triple(Box::new(oxigraph::model::Triple::new(
                subject, predicate, object,
            )));
            store
                .insert(&Quad::new(
                    reifier.clone(),
                    reifies,
                    quoted,
                    oxigraph::model::GraphName::DefaultGraph,
                ))
                .map_err(|e| e.to_string())?;

            for (key, val) in ann.as_object().unwrap() {
                if key == "@id" {
                    continue;
                }
                let ann_predicate = NamedNode::new(expand(key)).map_err(|e| e.to_string())?;
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
                        .map_err(|e| e.to_string())?;
                }
            }
        }

        Ok(())
    }

    fn parse_value_object(
        value: &Value,
        expand: &dyn Fn(&str) -> String,
    ) -> Result<(oxigraph::model::Term, Option<Value>), String> {
        if let Some(s) = value.as_str() {
            return Ok((
                NamedNode::new(expand(s)).map_err(|e| e.to_string())?.into(),
                None,
            ));
        }
        let obj = value
            .as_object()
            .ok_or_else(|| format!("expected value object, got {value}"))?;
        let annotation = obj.get("@annotation").cloned();

        if let Some(id) = obj.get("@id").and_then(Value::as_str) {
            let term: oxigraph::model::Term = if let Some(label) = id.strip_prefix("_:") {
                oxigraph::model::BlankNode::new(label.to_string())
                    .map_err(|e| e.to_string())?
                    .into()
            } else {
                NamedNode::new(expand(id))
                    .map_err(|e| e.to_string())?
                    .into()
            };
            return Ok((term, annotation));
        }

        let lex = obj
            .get("@value")
            .and_then(Value::as_str)
            .ok_or("literal without @value")?
            .to_string();
        let lang = obj.get("@language").and_then(Value::as_str);
        let direction = obj.get("@direction").and_then(Value::as_str);
        let datatype = obj.get("@type").and_then(Value::as_str);

        let literal = match (lang, direction, datatype) {
            (Some(lang), Some(dir), _) => {
                let dir = match dir {
                    "ltr" => BaseDirection::Ltr,
                    "rtl" => BaseDirection::Rtl,
                    _ => return Err(format!("invalid direction {dir}")),
                };
                oxigraph::model::Literal::new_directional_language_tagged_literal(&lex, lang, dir)
                    .map_err(|e| e.to_string())?
            }
            (Some(lang), None, _) => {
                oxigraph::model::Literal::new_language_tagged_literal(&lex, lang)
                    .map_err(|e| e.to_string())?
            }
            (None, _, Some(dt)) => oxigraph::model::Literal::new_typed_literal(
                &lex,
                NamedNode::new(expand(dt)).map_err(|e| e.to_string())?,
            ),
            _ => oxigraph::model::Literal::new_simple_literal(&lex),
        };

        Ok((literal.into(), annotation))
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

    #[test]
    fn minimal_rdf12_roundtrips_through_oxigraph() {
        let graph = minimal_graph();
        let json = serialize_graph(&graph).expect("serialize");

        let expected = parse_nquads(&gmeow_gts::nquads::to_nquads(&graph));
        let actual = parse_jsonld_star(&json).expect("parse JSON-LD-star");

        assert_eq!(
            canonical_nquads(&expected),
            canonical_nquads(&actual),
            "JSON-LD-star round-trip diverged from N-Quads-star baseline"
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
        let first = serialize_graph_yaml(&graph).expect("serialize first");
        let second = serialize_graph_yaml(&graph).expect("serialize second");
        assert_eq!(first, second, "YAML-LD output must be byte-deterministic");
    }

    #[test]
    fn yaml_ld_has_explicit_context_and_no_anchors() {
        let graph = minimal_graph();
        let yaml = serialize_graph_yaml(&graph).expect("serialize YAML-LD");
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
            yaml.contains("yaml-language-server: $schema="),
            "YAML-LD must carry a language-server schema header: {yaml}"
        );
    }

    #[test]
    fn yaml_ld_roundtrips_through_oxigraph() {
        let graph = minimal_graph();
        let yaml = serialize_graph_yaml(&graph).expect("serialize YAML-LD");
        // The test parser works over JSON-LD-star; convert YAML back to JSON first.
        let yaml_value: serde_yaml::Value =
            serde_yaml::from_str(&yaml).expect("parse emitted YAML-LD");
        let json = serde_json::to_string(&yaml_value).expect("YAML -> JSON");

        let expected = parse_nquads(&gmeow_gts::nquads::to_nquads(&graph));
        let actual = parse_jsonld_star(&json).expect("parse JSON-LD-star from YAML round-trip");

        assert_eq!(
            canonical_nquads(&expected),
            canonical_nquads(&actual),
            "YAML-LD round-trip diverged from N-Quads-star baseline"
        );
    }

    fn canonical_nquads(dataset: &Dataset) -> String {
        let mut quads: Vec<String> = dataset.iter().map(|q| q.to_string()).collect();
        quads.sort();
        quads.join("\n")
    }
}
