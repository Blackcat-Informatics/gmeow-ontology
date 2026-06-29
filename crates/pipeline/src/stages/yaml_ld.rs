// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The `yaml_ld` export leaf (#699): RDF → YAML-LD-star / JSON-LD-star.
//!
//! Emits both the JSON-LD-star lead artifact and a deterministic YAML-LD-star
//! derivative, plus a small serialization-preservation ledger.

use std::collections::BTreeMap;
use std::sync::Arc;

use gmeow_rdf::{RdfDataset, RdfLiteral, RdfQuad, RdfTerm, RdfTextDirection, RdfTriple};
use serde_json::Value;

use crate::error::PipelineError;
use crate::node::{Stage, StageInput, StageKind, StageOutput, StageProduct};
// The gts-`Graph` arena read shape, materialized over the native carrier — the SAME
// adapter the `parquet` leaf uses (no per-leaf shim). GTS is exit-only.
use crate::stages::fold_arena::{Graph, Term, TermKind};

// Literal datatype sentinels (formerly `gmeow_gts::model::*`). Read off the native
// carrier instead of the gts model — GTS is exit-only.
const RDF_DIR_LANG_STRING: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#dirLangString";
const RDF_LANG_STRING: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#langString";
const XSD_STRING: &str = "http://www.w3.org/2001/XMLSchema#string";

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
        // THIS run's carrier dataset, read directly off the snapshot product's bundle
        // (#1132) — no re-parse of the gmeow.gts bytes (GTS is exit-only).
        let dataset = crate::stages::carrier::snapshot_dataset(_input.upstream)?;
        let json = serialize_graph(dataset.as_ref())?;
        let yaml = serialize_graph_yaml(dataset.as_ref(), None)?;
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

/// Serialize the carrier dataset to a deterministic JSON-LD-star document.
pub fn serialize_graph(dataset: &RdfDataset) -> Result<String, PipelineError> {
    serialize_graph_arena(&Graph::from_dataset(dataset))
}

/// Serialize an already-materialized fold arena to a deterministic JSON-LD-star
/// document. The dataset entrypoint above is the production path; this inner form lets
/// the serializer unit tests drive synthetic arenas directly.
pub(crate) fn serialize_graph_arena(graph: &Graph) -> Result<String, PipelineError> {
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
    dataset: &RdfDataset,
    schema_url: Option<&str>,
) -> Result<String, PipelineError> {
    let json = serialize_graph(dataset)?;
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
        .triple
        .ok_or_else(|| PipelineError::Parse("triple term with no components".to_string()))?;
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
            // Key @language / @direction off the carrier's FIRST-CLASS language /
            // direction fields, not solely the datatype IRI: the native model carries a
            // directional-language string as `rdf:langString` + a separate `direction`,
            // so a datatype-only test would drop @direction.
            if datatype == RDF_DIR_LANG_STRING || term.direction.is_some() {
                if let Some(lang) = &term.lang {
                    map.insert("@language".to_string(), Value::String(lang.clone()));
                }
                if let Some(dir) = &term.direction {
                    map.insert("@direction".to_string(), Value::String(dir.clone()));
                }
            } else if datatype == RDF_LANG_STRING || term.lang.is_some() {
                if let Some(lang) = &term.lang {
                    map.insert("@language".to_string(), Value::String(lang.clone()));
                }
            } else if datatype != XSD_STRING {
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
            // Same first-class language/direction handling as `term_to_value`.
            if datatype == RDF_DIR_LANG_STRING || term.direction.is_some() {
                if let Some(lang) = &term.lang {
                    map.insert("@language".to_string(), Value::String(lang.clone()));
                }
                if let Some(dir) = &term.direction {
                    map.insert("@direction".to_string(), Value::String(dir.clone()));
                }
            } else if datatype == RDF_LANG_STRING || term.lang.is_some() {
                if let Some(lang) = &term.lang {
                    map.insert("@language".to_string(), Value::String(lang.clone()));
                }
            } else if datatype != XSD_STRING {
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
        TermKind::Triple => match term.triple {
            Some((s, p, o)) => format!("triple:{s}:{p}:{o}"),
            None => "triple:none".to_string(),
        },
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

/// Parse JSON-LD-star bytes into the native carrier [`RdfDataset`].
///
/// This is the inverse of [`serialize_graph`]: it interprets the `@annotation`
/// idiom produced by the GMEOW JSON-LD-star emitter and reconstructs RDF 1.2
/// reifier quads (`rdf:reifies` with quoted triple objects) plus annotation
/// triples in the default graph. Those reifier/annotation rows are FOLDED into
/// the dataset's RDF 1.2 statement layer at freeze time (`dataset_from_quads`),
/// exactly as the prior oxigraph `Store::iter().collect::<Dataset>()` + downstream
/// fold did. Named graphs and directional language strings are preserved.
/// Unsupported JSON-LD features hard-fail.
pub fn parse_jsonld_star(json_bytes: &[u8]) -> Result<Arc<RdfDataset>, PipelineError> {
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

    // Accumulate native quads (including un-folded `rdf:reifies` rows); the fold to the
    // RDF 1.2 statement layer happens at `dataset_from_quads` freeze time.
    let quads: std::cell::RefCell<Vec<RdfQuad>> = std::cell::RefCell::new(Vec::new());

    let emit_node = |node: &Value, graph_iri: Option<&str>| -> Result<(), PipelineError> {
        let id = node
            .get("@id")
            .and_then(Value::as_str)
            .ok_or_else(|| PipelineError::Decode("node without @id".to_string()))?;
        let subject: RdfTerm = node_id_term(id, &expand)?;
        // Validate the named-graph IRI (mirrors the old `NamedNode::new` Result path).
        let graph_name: Option<RdfTerm> = graph_iri
            .map(|g| validated_iri_term(&expand(g)))
            .transpose()?;

        if let Some(Value::Array(types)) = node.get("@type") {
            for t in types {
                let t_id = t
                    .get("@id")
                    .and_then(Value::as_str)
                    .ok_or_else(|| PipelineError::Decode("@type value without @id".to_string()))?;
                let obj = validated_iri_term(&expand(t_id))?;
                push_quad(&quads, subject.clone(), RDF_TYPE, obj, graph_name.clone());
            }
        }

        for (key, val) in node.as_object().unwrap() {
            if matches!(key.as_str(), "@id" | "@type" | "@context" | "@graph") {
                continue;
            }
            let predicate = expand(key);
            // Validate the predicate IRI (mirrors the old `NamedNode::new` Result path).
            validated_iri_term(&predicate)?;
            let values = if let Value::Array(arr) = val {
                arr.clone()
            } else {
                vec![val.clone()]
            };
            for v in values {
                emit_value_quad(
                    &quads,
                    subject.clone(),
                    &predicate,
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

    // Freeze + fold the RDF 1.2 statement layer (a `rdf:reifies` triple-term object
    // becomes a reifier binding; a reifier subject's other triples become annotations).
    gmeow_rdf::dataset_from_quads(&quads.into_inner())
        .map_err(|e| PipelineError::Parse(format!("freeze JSON-LD-star quads: {e}")))
}

/// Build an [`RdfTerm`] for a node `@id` (`_:label` blank node or expanded IRI),
/// validating the IRI through the SPARQL-algebra parser (mirrors the old
/// `oxigraph::model::NamedNode::new` Result discrimination).
fn node_id_term(id: &str, expand: &dyn Fn(&str) -> String) -> Result<RdfTerm, PipelineError> {
    if let Some(label) = id.strip_prefix("_:") {
        Ok(RdfTerm::blank_node(label.to_string()))
    } else {
        validated_iri_term(&expand(id))
    }
}

/// Validate `iri` as an absolute IRI (preserving the old `NamedNode::new` Ok/Err
/// discrimination) and return it as an [`RdfTerm`].
fn validated_iri_term(iri: &str) -> Result<RdfTerm, PipelineError> {
    gmeow_sparql_algebra::NamedNode::new(iri.to_string())
        .map_err(|e| PipelineError::Decode(e.to_string()))?;
    Ok(RdfTerm::iri(iri.to_string()))
}

/// Push a base quad (optionally in a named graph) into the native accumulator.
fn push_quad(
    quads: &std::cell::RefCell<Vec<RdfQuad>>,
    subject: RdfTerm,
    predicate: &str,
    object: RdfTerm,
    graph_name: Option<RdfTerm>,
) {
    let mut quad = RdfQuad::new(subject, predicate, object);
    if let Some(g) = graph_name {
        quad = quad.in_graph(g);
    }
    quads.borrow_mut().push(quad);
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
    quads: &std::cell::RefCell<Vec<RdfQuad>>,
    subject: RdfTerm,
    predicate: &str,
    graph_name: Option<RdfTerm>,
    value: &Value,
    expand: &dyn Fn(&str) -> String,
) -> Result<(), PipelineError> {
    let (object, annotation) = parse_value_object(value, expand)?;
    push_quad(
        quads,
        subject.clone(),
        predicate,
        object.clone(),
        graph_name,
    );

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
            let reifier: RdfTerm = node_id_term(reifier_subject, expand)?;
            // The `rdf:reifies` quoted-triple row is pushed un-folded; the
            // `dataset_from_quads` freeze folds it into the reifier table.
            let quoted =
                RdfTerm::triple(RdfTriple::new(subject.clone(), predicate, object.clone()));
            // Reifier bindings + annotations always land in the DEFAULT graph.
            push_quad(quads, reifier.clone(), RDF_REIFIES, quoted, None);

            for (key, val) in ann_node.as_object().unwrap() {
                if key == "@id" {
                    continue;
                }
                let ann_predicate = expand(key);
                validated_iri_term(&ann_predicate)?;
                let vals = if let Value::Array(arr) = val {
                    arr.clone()
                } else {
                    vec![val.clone()]
                };
                for v in vals {
                    let (ann_object, _) = parse_value_object(&v, expand)?;
                    push_quad(quads, reifier.clone(), &ann_predicate, ann_object, None);
                }
            }
        }
    }

    Ok(())
}

fn parse_value_object(
    value: &Value,
    expand: &dyn Fn(&str) -> String,
) -> Result<(RdfTerm, Option<Value>), PipelineError> {
    if let Some(s) = value.as_str() {
        return Ok((validated_iri_term(&expand(s))?, None));
    }
    let obj = value
        .as_object()
        .ok_or_else(|| PipelineError::Decode(format!("expected value object, got {value}")))?;
    let annotation = obj.get("@annotation").cloned();

    if let Some(id) = obj.get("@id").and_then(Value::as_str) {
        return Ok((node_id_term(id, expand)?, annotation));
    }

    let lex = obj
        .get("@value")
        .and_then(Value::as_str)
        .ok_or_else(|| PipelineError::Decode("literal without @value".to_string()))?
        .to_string();
    let lang = obj.get("@language").and_then(Value::as_str);
    let direction = obj.get("@direction").and_then(Value::as_str);
    let datatype = obj.get("@type").and_then(Value::as_str);

    // The native model preserves the project's long private-use language subtags
    // (`x-gmeow-norwegiannynorsk`, >8 chars) verbatim — there is no strict tag
    // validation to reject them, matching #909's end-to-end preservation and the
    // lenient gmeow-gts codecs that produced this JSON-LD-star input.
    let literal = match (lang, direction, datatype) {
        (Some(lang), Some(dir), _) => {
            let dir = match dir {
                "ltr" => RdfTextDirection::Ltr,
                "rtl" => RdfTextDirection::Rtl,
                _ => return Err(PipelineError::Decode(format!("invalid direction {dir}"))),
            };
            RdfLiteral {
                lexical_form: lex,
                datatype: None,
                language: Some(lang.to_string()),
                direction: Some(dir),
            }
        }
        (Some(lang), None, _) => RdfLiteral::language_tagged(lex, lang),
        (None, _, Some(dt)) => {
            let dt = expand(dt);
            validated_iri_term(&dt)?;
            RdfLiteral::typed(lex, dt)
        }
        _ => RdfLiteral::simple(lex),
    };

    Ok((RdfTerm::literal(literal), annotation))
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

    // Flatten the carrier back to the source-faithful quad stream, re-materializing the
    // RDF 1.2 statement overlay as un-folded `rdf:reifies` reifier rows + annotation
    // rows (the exact inverse of the `dataset_from_quads` fold). This is the native twin
    // of the prior `dataset.iter().map(into_owned)` over the oxigraph dataset.
    let quads = gmeow_rdf::flat_rdf_quads_from_dataset(&dataset);

    // Identify reifiers and the quoted triple each one refers to.
    let mut reifier_quotes: std::collections::HashMap<RdfTerm, (RdfTerm, String, RdfTerm)> =
        std::collections::HashMap::new();
    for quad in &quads {
        if quad.predicate == RDF_REIFIES {
            if let RdfTerm::Triple(triple) = &quad.object {
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

    let mut out: Vec<RdfQuad> = Vec::new();

    for quad in &quads {
        if quad.predicate == RDF_REIFIES {
            // Emit the GMEOW statement-metadata skeleton for this reifier.
            let Some((s, p, o)) = reifier_quotes.get(&quad.subject) else {
                continue;
            };
            let r = quad.subject.clone();
            out.push(RdfQuad::new(
                r.clone(),
                RDF_TYPE,
                RdfTerm::iri(GMEOW_STATEMENT_METADATA),
            ));
            out.push(RdfQuad::new(r.clone(), GMEOW_QSUBJECT, s.clone()));
            out.push(RdfQuad::new(
                r.clone(),
                GMEOW_QPREDICATE,
                RdfTerm::iri(p.clone()),
            ));
            let q_object_pred = if matches!(o, RdfTerm::Literal(_)) {
                GMEOW_QOBJECTLITERAL
            } else {
                GMEOW_QOBJECT
            };
            out.push(RdfQuad::new(r.clone(), q_object_pred, o.clone()));
        } else if reifier_quotes.contains_key(&quad.subject) {
            // Annotation triple on a reifier: keep it, but in the default graph so the
            // downstream rdflib-compat graph (single-graph) sees it.
            out.push(RdfQuad::new(
                quad.subject.clone(),
                quad.predicate.clone(),
                quad.object.clone(),
            ));
        } else {
            // Plain base triple or named-graph triple (graph name preserved).
            out.push(quad.clone());
        }
    }

    // `out` holds only the downcast-flat statement-metadata cells (no object-position
    // quoted triples), so the native N-Quads serializer applies.
    let ir = gmeow_rdf::dataset_from_quads(&out)
        .map_err(|e| PipelineError::Decode(format!("quads → IR: {e}")))?;
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
pub(crate) fn canonical_lines(dataset: &RdfDataset) -> Vec<String> {
    // Native full RDFC-1.0 over the FLATTENED carrier (#910): `canonical_flat_nquads`
    // re-materializes the RDF 1.2 statement overlay to plain `rdf:reifies` / annotation
    // triples before canonicalizing, byte-identical to the prior oxigraph-flat path.
    let canonical = gmeow_rdf::canonical_flat_nquads(dataset)
        .expect("RDFC-1.0 canonicalization of parsed dataset");
    let mut lines: Vec<String> = canonical.lines().map(str::to_owned).collect();
    lines.sort();
    lines
}

/// Parse N-Quads-star text into the native carrier [`RdfDataset`], preserving the
/// RDF 1.2 statement layer (quoted triple terms fold to the reifier table). Used by
/// [`roundtrip_isomorphic`].
fn dataset_from_nquads(nquads: &[u8]) -> Result<Arc<RdfDataset>, PipelineError> {
    // The native codec folds the RDF 1.2 statement layer to the IR reifier table at parse
    // time; `canonical_lines` un-folds it back to the equivalent flat `<reifier> rdf:reifies
    // <<( s p o )>>` rows (exact inverses), so the star structure the RDFC-1.0 canonical
    // comparison depends on is preserved.
    gmeow_rdf::parse_dataset(nquads, "application/n-quads", None)
        .map_err(|e| PipelineError::Parse(format!("parse N-Quads: {e}")))
}

/// Return whether `star_bytes` (format `"jsonld"`|`"yamlld"`) re-parses to a
/// dataset isomorphic (RDFC-1.0 canonical) to the original N-Quads-star input.
/// This is the Rust authority for the build-time serialization-isomorphism gate
/// (#699), replacing the Python `_round_trip_star`.
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

    // The serializer-fixture builders construct synthetic gts `Graph`s as ground
    // truth (gts is the archive boundary; these are test fixtures, not a transport)
    // and bridge them to the native carrier via [`gts_graph_to_dataset`] before
    // feeding the production `serialize_graph` entrypoint.
    use gmeow_gts::model::{Graph, Term, TermKind};
    use gmeow_rdf::{
        RdfDataset, RdfLiteral, RdfLookaside, RdfQuad, RdfTerm, RdfTextDirection, RdfTriple,
    };

    use std::path::PathBuf;
    use std::sync::Arc;

    /// Bridge a synthetic gts `Graph` fixture into the native carrier dataset the
    /// production serializer consumes (round-trip through the lossless N-Quads codec).
    fn gts_graph_to_dataset(g: &Graph) -> Arc<RdfDataset> {
        let nq = gmeow_gts::nquads::to_nquads(g);
        gmeow_rdf::parse_dataset(
            nq.as_bytes(),
            gmeow_rdf::NativeRdfFormat::NQuads.media_type(),
            None,
        )
        .expect("gts fixture N-Quads parse into carrier dataset")
    }

    /// The flattened source-faithful quad stream of a carrier dataset: the RDF 1.2
    /// statement overlay (reifier bindings + annotations) is re-materialized to plain
    /// `rdf:reifies` / annotation quads so test assertions match the way the prior
    /// oxigraph `Dataset` exposed those rows.
    fn flat_quads(dataset: &RdfDataset) -> Vec<RdfQuad> {
        gmeow_rdf::flat_rdf_quads_from_dataset(dataset)
    }

    /// Assert no quad object is an RDF 1.2 quoted triple term (over the flattened
    /// carrier — the downcast output must be plain N-Quads).
    fn assert_no_triple_terms(dataset: &RdfDataset) {
        assert!(
            !flat_quads(dataset)
                .iter()
                .any(|q| matches!(q.object, RdfTerm::Triple(_))),
            "downcast output must contain no quoted triple terms"
        );
    }

    /// A language-tagged literal term (native replacement for an oxigraph
    /// `Literal::new_language_tagged_literal`).
    fn ox_lang_literal(lex: &str, lang: &str) -> RdfTerm {
        RdfTerm::literal(RdfLiteral::language_tagged(lex, lang))
    }

    /// A typed literal term (native replacement for `Literal::new_typed_literal`).
    fn ox_typed_literal(lex: &str, datatype: &str) -> RdfTerm {
        RdfTerm::literal(RdfLiteral::typed(lex, datatype))
    }

    /// A directional language-tagged literal term (native replacement for
    /// `Literal::new_directional_language_tagged_literal`).
    fn ox_dir_lang_literal(lex: &str, lang: &str, direction: RdfTextDirection) -> RdfTerm {
        RdfTerm::literal(RdfLiteral {
            lexical_form: lex.to_string(),
            datatype: None,
            language: Some(lang.to_string()),
            direction: Some(direction),
        })
    }

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

    #[allow(dead_code)]
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

    /// Parse N-Quads-star text into the native carrier dataset (native codec round-trip).
    fn parse_nquads(nq: &str) -> Arc<RdfDataset> {
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

    /// The IRI lexical form of an IRI term.
    fn iri_str(term: &RdfTerm) -> &str {
        match term {
            RdfTerm::Iri(iri) => iri.as_str(),
            other => panic!("expected an IRI term, got {other:?}"),
        }
    }

    /// An IRI term (the native replacement for an oxigraph `NamedNode`).
    fn ox_named_node(iri: &str) -> RdfTerm {
        RdfTerm::iri(iri)
    }

    fn ox_simple_literal(lex: &str) -> RdfTerm {
        RdfTerm::literal(RdfLiteral::simple(lex))
    }

    fn ox_quoted_triple(s: RdfTerm, p: RdfTerm, o: RdfTerm) -> RdfTerm {
        let predicate = iri_str(&p).to_string();
        RdfTerm::triple(RdfTriple::new(s, predicate, o))
    }

    /// Normalize a term so a hand-built `RdfLiteral` compares equal to the carrier's
    /// fully-materialized literal. The carrier ALWAYS resolves a literal's datatype
    /// explicitly (a hand-built one may leave it `None`), and it stores a
    /// language-tagged literal as `rdf:langString` even when a base DIRECTION is
    /// present (the direction is carried out-of-band). So the canonical datatype is
    /// keyed off language presence ALONE, and a hand-built `rdf:dirLangString` is
    /// dropped to `rdf:langString` to match. Recurses into triple terms.
    fn normalize_term(term: &RdfTerm) -> RdfTerm {
        match term {
            RdfTerm::Literal(lit) => {
                let datatype = match &lit.datatype {
                    Some(dt) if dt == RDF_DIR_LANG_STRING => RDF_LANG_STRING.to_string(),
                    Some(dt) => dt.clone(),
                    None if lit.language.is_some() => RDF_LANG_STRING.to_string(),
                    None => XSD_STRING.to_string(),
                };
                RdfTerm::literal(RdfLiteral {
                    lexical_form: lit.lexical_form.clone(),
                    datatype: Some(datatype),
                    language: lit.language.clone(),
                    direction: lit.direction,
                })
            }
            RdfTerm::Triple(triple) => RdfTerm::triple(RdfTriple::new(
                normalize_term(&triple.subject),
                triple.predicate.clone(),
                normalize_term(&triple.object),
            )),
            other => other.clone(),
        }
    }

    /// Membership test over the flattened carrier quad stream (base quads + the
    /// re-materialized `rdf:reifies` / annotation rows). `predicate` is an IRI term.
    fn dataset_has(
        dataset: &RdfDataset,
        subject: &RdfTerm,
        predicate: &RdfTerm,
        object: &RdfTerm,
    ) -> bool {
        let pred_iri = iri_str(predicate);
        let want_subject = normalize_term(subject);
        let want_object = normalize_term(object);
        flat_quads(dataset).iter().any(|q| {
            normalize_term(&q.subject) == want_subject
                && q.predicate == pred_iri
                && normalize_term(&q.object) == want_object
        })
    }

    fn assert_no_gmeow_at_id_leak(dataset: &RdfDataset, json: &str) {
        const GMEOW_NS: &str = "https://blackcatinformatics.ca/gmeow/";
        let at_id = format!("{GMEOW_NS}@id");
        let quads = flat_quads(dataset);
        assert!(
            !quads.iter().any(|q| q.predicate == at_id),
            "gmeow:@id must not leak as a property triple: {json}"
        );
        assert!(
            !quads.iter().any(|q| {
                q.predicate.starts_with(GMEOW_NS)
                    && matches!(
                        &q.object,
                        RdfTerm::Iri(n) if n == "http://example.org/reifier"
                    )
            }),
            "reifier IRI must not appear as object of any gmeow-prefixed predicate: {json}"
        );
    }

    #[test]
    fn minimal_rdf12_roundtrips_through_oxigraph() {
        let graph = minimal_graph();
        let json = serialize_graph(gts_graph_to_dataset(&graph).as_ref()).expect("serialize");

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

        let json = serialize_graph(gts_graph_to_dataset(&graph).as_ref()).expect("serialize");
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
        let first =
            serialize_graph(gts_graph_to_dataset(&graph).as_ref()).expect("serialize first");
        let second =
            serialize_graph(gts_graph_to_dataset(&graph).as_ref()).expect("serialize second");
        assert_eq!(first, second, "JSON-LD output must be byte-deterministic");
    }

    #[test]
    fn directional_language_string_emits_direction() {
        // Build the carrier directly from RDF 1.2 directional-language N-Quads
        // (`"lex"@lang--ltr`) — the production path. (The gts→N-Quads fixture bridge
        // does not carry base direction, so this exercises the native carrier instead.)
        let nq = b"<https://example.org/s> <https://example.org/p> \"hello\"@en--ltr .\n";
        let dataset = gmeow_rdf::dataset_from_bytes(nq, gmeow_rdf::NativeRdfFormat::NQuads)
            .expect("parse directional-language N-Quads into the carrier");

        let json = serialize_graph(dataset.as_ref()).expect("serialize");
        assert!(
            json.contains("\"@direction\": \"ltr\""),
            "directional language literal must emit @direction: {json}"
        );
        assert!(
            json.contains("\"@language\": \"en\""),
            "directional language literal must also emit @language: {json}"
        );
    }

    #[test]
    fn yaml_ld_is_byte_deterministic() {
        let graph = minimal_graph();
        let first = serialize_graph_yaml(gts_graph_to_dataset(&graph).as_ref(), None)
            .expect("serialize first");
        let second = serialize_graph_yaml(gts_graph_to_dataset(&graph).as_ref(), None)
            .expect("serialize second");
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

        let json_a =
            serialize_graph(gts_graph_to_dataset(&graph_a).as_ref()).expect("serialize graph A");
        let json_b =
            serialize_graph(gts_graph_to_dataset(&graph_b).as_ref()).expect("serialize graph B");
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

        let yaml_a = serialize_graph_yaml(gts_graph_to_dataset(&graph_a).as_ref(), None)
            .expect("serialize YAML-LD A");
        let yaml_b = serialize_graph_yaml(gts_graph_to_dataset(&graph_b).as_ref(), None)
            .expect("serialize YAML-LD B");
        assert_eq!(
            yaml_a, yaml_b,
            "YAML-LD-star output must be identical under different hash-map seeds"
        );
    }

    #[test]
    fn yaml_ld_has_explicit_context_and_no_anchors() {
        let graph = minimal_graph();
        let yaml = serialize_graph_yaml(gts_graph_to_dataset(&graph).as_ref(), None)
            .expect("serialize YAML-LD");
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
        let yaml = serialize_graph_yaml(gts_graph_to_dataset(&graph).as_ref(), None)
            .expect("serialize YAML-LD");
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

        let json = serialize_graph(gts_graph_to_dataset(&graph).as_ref()).expect("serialize");
        let dataset = parse_jsonld_star(json.as_bytes()).expect("parse JSON-LD-star");

        let s = ox_named_node("http://example.org/s");
        let p = ox_named_node("http://example.org/p");
        let o = ox_named_node("http://example.org/o");
        let reifier = ox_named_node("http://example.org/reifier");
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

        let json = serialize_graph(gts_graph_to_dataset(&graph).as_ref()).expect("serialize");
        let dataset = parse_jsonld_star(json.as_bytes()).expect("parse JSON-LD-star");

        let s = ox_named_node("http://example.org/s");
        let p = ox_named_node("http://example.org/p");
        let o = ox_simple_literal("hello");
        let reifier = ox_named_node("http://example.org/reifier");
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

        let json = serialize_graph(gts_graph_to_dataset(&graph).as_ref()).expect("serialize");
        let dataset = parse_jsonld_star(json.as_bytes()).expect("parse JSON-LD-star");

        let s = ox_named_node("http://example.org/s");
        let p = ox_named_node("http://example.org/p");
        let o = ox_named_node("http://example.org/o");
        let reifies_iri = RDF_REIFIES;
        let confidence = ox_named_node("http://example.org/confidence");
        let meta = ox_simple_literal("0.9");
        let quoted = ox_quoted_triple(s.clone(), p.clone(), o.clone());

        assert!(dataset_has(&dataset, &s, &p, &o));

        let flat = flat_quads(&dataset);
        let reifier_quads: Vec<&RdfQuad> = flat
            .iter()
            .filter(|q| q.predicate == reifies_iri && q.object == quoted)
            .collect();
        assert_eq!(
            reifier_quads.len(),
            1,
            "expected exactly one rdf:reifies quad for the base triple"
        );
        assert!(
            matches!(reifier_quads[0].subject, RdfTerm::BlankNode(_)),
            "blank reifier fallback must use a blank node subject: {json}"
        );
        assert!(dataset_has(
            &dataset,
            &reifier_quads[0].subject,
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

        let json = serialize_graph(gts_graph_to_dataset(&graph).as_ref()).expect("serialize");
        let nquads = jsonld_star_to_gmeow_statement_metadata_nquads(json.as_bytes())
            .expect("downcast JSON-LD-star to GMEOW statement metadata");

        // The output must be parseable plain N-Quads (no quoted triple terms).
        let dataset = parse_nquads(&nquads);
        assert_no_triple_terms(&dataset);

        let s = ox_named_node("http://example.org/s");
        let p = ox_named_node("http://example.org/p");
        let o = ox_named_node("http://example.org/o");
        let r = ox_named_node("http://example.org/r");
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
            dataset_has(&dataset, &r, &rdf_type, &statement_metadata),
            "reifier must be typed gmeow:StatementMetadata"
        );
        assert!(
            dataset_has(&dataset, &r, &q_subject, &s),
            "gmeow:qSubject must point to quoted subject"
        );
        assert!(
            dataset_has(&dataset, &r, &q_predicate, &p),
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

        let json = serialize_graph(gts_graph_to_dataset(&graph).as_ref()).expect("serialize");
        let nquads = jsonld_star_to_gmeow_statement_metadata_nquads(json.as_bytes())
            .expect("downcast literal-valued JSON-LD-star");
        let dataset = parse_nquads(&nquads);

        let s = ox_named_node("http://example.org/s");
        let p = ox_named_node("http://example.org/p");
        let o = ox_lang_literal("hello", "en");
        let r = ox_named_node("http://example.org/r");
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

        let json = serialize_graph(gts_graph_to_dataset(&graph).as_ref()).expect("serialize");
        let nquads = jsonld_star_to_gmeow_statement_metadata_nquads(json.as_bytes())
            .expect("downcast simple-literal JSON-LD-star");

        // The output must be parseable plain N-Quads (no quoted triple terms).
        let dataset = parse_nquads(&nquads);
        assert_no_triple_terms(&dataset);

        let s = ox_named_node("http://example.org/s");
        let p = ox_named_node("http://example.org/p");
        let o = ox_simple_literal("hello");
        let r = ox_named_node("http://example.org/r");
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
            dataset_has(&dataset, &r, &rdf_type, &statement_metadata),
            "reifier must be typed gmeow:StatementMetadata"
        );
        assert!(
            dataset_has(&dataset, &r, &q_subject, &s),
            "gmeow:qSubject must point to quoted subject"
        );
        assert!(
            dataset_has(&dataset, &r, &q_predicate, &p),
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

        let json = serialize_graph(gts_graph_to_dataset(&graph).as_ref()).expect("serialize");
        let nquads = jsonld_star_to_gmeow_statement_metadata_nquads(json.as_bytes())
            .expect("downcast schema-org-like JSON-LD-star");

        let dataset = parse_nquads(&nquads);
        assert_no_triple_terms(&dataset);

        let alice = ox_named_node("https://example.org/alice");
        let schema_name = ox_named_node("https://schema.org/name");
        let alice_name = ox_simple_literal("Alice");
        let claim = ox_named_node("https://example.org/claim-alice-name");
        let rdf_type = ox_named_node(RDF_TYPE);
        let statement_metadata = ox_named_node(GMEOW_STATEMENT_METADATA);
        let q_subject = ox_named_node(GMEOW_QSUBJECT);
        let q_predicate = ox_named_node(GMEOW_QPREDICATE);
        let q_object_literal = ox_named_node(GMEOW_QOBJECTLITERAL);
        let confidence = ox_named_node("https://blackcatinformatics.ca/gmeow/confidence");
        let meta = ox_typed_literal("0.9", "http://www.w3.org/2001/XMLSchema#decimal");

        assert!(
            dataset_has(&dataset, &alice, &schema_name, &alice_name),
            "base triple must survive"
        );
        assert!(
            dataset_has(&dataset, &claim, &rdf_type, &statement_metadata),
            "reifier must be typed gmeow:StatementMetadata"
        );
        assert!(
            dataset_has(&dataset, &claim, &q_subject, &alice),
            "gmeow:qSubject must point to quoted subject"
        );
        assert!(
            dataset_has(&dataset, &claim, &q_predicate, &schema_name),
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

    /// Load a Turtle-star file into the native carrier [`RdfDataset`], preserving
    /// lexical forms (and the folded RDF 1.2 statement layer) from the committed artifact.
    fn load_turtle_dataset(path: &std::path::Path) -> Result<Arc<RdfDataset>, PipelineError> {
        let bytes = std::fs::read(path)
            .map_err(|e| PipelineError::Parse(format!("read {}: {e}", path.display())))?;
        gmeow_rdf::parse_dataset(&bytes, "text/turtle", None)
            .map_err(|e| PipelineError::Parse(format!("Turtle parse: {e}")))
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
        let graph =
            graph_from_rdf_dataset(&original).expect("fold committed artifact to GTS graph");
        let json = serialize_graph(gts_graph_to_dataset(&graph).as_ref())
            .expect("serialize GTS graph to JSON-LD-star");
        let roundtrip =
            parse_jsonld_star(json.as_bytes()).expect("parse JSON-LD-star back to carrier dataset");

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
        let graph = graph_from_rdf_dataset(&original).expect("fold dist artifact to GTS graph");
        let json = serialize_graph(gts_graph_to_dataset(&graph).as_ref())
            .expect("re-serialize GTS graph to JSON-LD-star");
        let roundtrip = parse_jsonld_star(json.as_bytes())
            .expect("parse re-serialized JSON-LD-star back to carrier dataset");

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
            !flat_quads(&dataset)
                .iter()
                .any(|q| matches!(q.object, RdfTerm::Triple(_))),
            "transpiled output must contain no RDF 1.2 quoted triple terms"
        );

        let claim = ox_named_node("https://example.org/claim-001");
        let alice = ox_named_node("https://example.org/alice");
        let analyst = ox_named_node("https://example.org/analyst-standpoint");
        let bullshit = ox_named_node("https://blackcatinformatics.ca/gmeow/bullshit");

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
            dataset_has(&dataset, &claim, &observed_feature, &alice),
            "observedFeature base triple must survive transpile"
        );

        // Directional language string is preserved on the base literal triple.
        let alice_name = ox_dir_lang_literal("Alice", "en", RdfTextDirection::Ltr);
        assert!(
            dataset_has(&dataset, &alice, &name, &alice_name),
            "directional language-tagged name must survive transpile"
        );

        // Explicit reifier for the claim modality is typed StatementMetadata and
        // carries the quoted subject/predicate/object skeleton.
        let claim_annotation = ox_named_node("https://example.org/claim-001-annotation");
        assert!(
            dataset_has(&dataset, &claim_annotation, &rdf_type, &standpoint_claim),
            "explicit reifier must be typed gmeow:StatementMetadata"
        );
        assert!(
            dataset_has(&dataset, &claim_annotation, &q_subject, &claim),
            "gmeow:qSubject must point to the claim"
        );
        assert!(
            dataset_has(&dataset, &claim_annotation, &q_predicate, &claim_modality),
            "gmeow:qPredicate must point to claimModality"
        );
        assert!(
            dataset_has(&dataset, &claim_annotation, &q_object, &bullshit),
            "gmeow:qObject must point to the IRI object"
        );

        // Annotation triples on the explicit reifier survive.
        assert!(
            dataset_has(&dataset, &claim_annotation, &according_to, &analyst),
            "accordingTo annotation must survive transpile"
        );
        let confidence_value = ox_typed_literal("0.65", "http://www.w3.org/2001/XMLSchema#decimal");
        assert!(
            dataset_has(&dataset, &claim_annotation, &confidence, &confidence_value),
            "confidence annotation must survive transpile"
        );
        let asserted_value = ox_typed_literal(
            "2026-06-05T00:00:00Z",
            "http://www.w3.org/2001/XMLSchema#dateTime",
        );
        assert!(
            dataset_has(&dataset, &claim_annotation, &asserted_at, &asserted_value),
            "assertedAt annotation must survive transpile"
        );

        // Explicit reifier for the directional-language name uses qObjectLiteral.
        let name_annotation = ox_named_node("https://example.org/alice-name-annotation");
        assert!(
            dataset_has(&dataset, &name_annotation, &rdf_type, &standpoint_claim),
            "name reifier must be typed gmeow:StatementMetadata"
        );
        assert!(
            dataset_has(&dataset, &name_annotation, &q_subject, &alice),
            "name gmeow:qSubject must point to alice"
        );
        assert!(
            dataset_has(&dataset, &name_annotation, &q_predicate, &name),
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

    /// RDFC-1.0 canonical, sorted line representation of a carrier dataset, over the
    /// flattened (un-folded) star layer — the comparison key for the round-trip tests.
    fn canonical_nquads(dataset: &RdfDataset) -> String {
        canonical_lines(dataset).join("\n")
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
        let json =
            serialize_graph(gts_graph_to_dataset(&graph).as_ref()).expect("serialize JSON-LD-star");
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
        let yaml = serialize_graph_yaml(gts_graph_to_dataset(&graph).as_ref(), None)
            .expect("serialize YAML-LD-star");
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
        assert_no_triple_terms(&yaml_lift);

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

        let turtle_lift = gmeow_rdf::parse_dataset(TURTLE_DOC.as_bytes(), "text/turtle", None)
            .expect("Turtle parse must succeed");

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
