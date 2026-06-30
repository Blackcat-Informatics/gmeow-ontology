// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The `lpg` export leaf (#861 P4): RDF → Labeled Property Graph.
//!
//! A genuine port of `src/gmeow_tools/lpg.py` (no Rust existed): reads the
//! statement-layer quads + reifier/annotation fold tables from an `RdfDataset`
//! (the composed dataset / committed `gmeow.gts`) and emits nodes + edges, with
//! statement metadata as edge properties, to generic CSV, Neo4j typed CSV,
//! openCypher, and GraphML. Byte-deterministic; compared to `generated/lpg/**`.

use std::collections::{BTreeMap, BTreeSet};

use gmeow_rdf::model::{RdfLiteral, RdfTerm};
use gmeow_rdf::RdfDataset;
use sha2::{Digest, Sha256};

use crate::error::PipelineError;
use crate::node::{Stage, StageInput, StageOutput, StageProduct};

/// Logical-path prefix of the generated LPG artifacts.
pub const LPG_DIR: &str = "generated/lpg";

const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
const XSD: &str = "http://www.w3.org/2001/XMLSchema#";
const STATEMENTS_GRAPH: &str = "https://blackcatinformatics.ca/gmeow/graph/statements";

const SKIP_LABELS: &[&str] = &[
    "http://www.w3.org/1999/02/22-rdf-syntax-ns#Statement",
    "http://www.w3.org/2002/07/owl#Axiom",
    "http://www.w3.org/2002/07/owl#NamedIndividual",
];
const TBOX_PREDICATES: &[&str] = &[
    "http://www.w3.org/2000/01/rdf-schema#subClassOf",
    "http://www.w3.org/2000/01/rdf-schema#subPropertyOf",
    "http://www.w3.org/2000/01/rdf-schema#domain",
    "http://www.w3.org/2000/01/rdf-schema#range",
    "http://www.w3.org/2002/07/owl#equivalentClass",
    "http://www.w3.org/2002/07/owl#equivalentProperty",
    "http://www.w3.org/2002/07/owl#disjointWith",
    "http://www.w3.org/2002/07/owl#imports",
    "http://www.w3.org/2002/07/owl#inverseOf",
    "http://www.w3.org/2002/07/owl#propertyChainAxiom",
    "http://www.w3.org/2002/07/owl#allValuesFrom",
    "http://www.w3.org/2002/07/owl#someValuesFrom",
    "http://www.w3.org/2002/07/owl#hasValue",
    "http://www.w3.org/2002/07/owl#cardinality",
    "http://www.w3.org/2002/07/owl#minCardinality",
    "http://www.w3.org/2002/07/owl#maxCardinality",
    "http://www.w3.org/2002/07/owl#onProperty",
    "http://www.w3.org/2002/07/owl#onClass",
];

include!("lpg_prefixes.rs");

/// A property value, modelling the Python scalar/`list`/`dict` forms so JSON and
/// Cypher/CSV rendering match byte-for-byte.
#[derive(Clone, PartialEq)]
enum Val {
    Int(i64),
    Float(f64),
    Bool(bool),
    Str(String),
    Lang { value: String, lang: String },
    List(Vec<Val>),
}

impl Val {
    /// Python `json.dumps(v, sort_keys=True, ensure_ascii=False)` of this value.
    fn json(&self) -> String {
        match self {
            Val::Int(i) => i.to_string(),
            Val::Float(f) => fmt_float(*f),
            Val::Bool(b) => {
                if *b {
                    "true".into()
                } else {
                    "false".into()
                }
            }
            Val::Str(s) => json_str(s),
            // sort_keys → "lang" before "value".
            Val::Lang { value, lang } => {
                format!(
                    "{{{}: {}, {}: {}}}",
                    json_str("lang"),
                    json_str(lang),
                    json_str("value"),
                    json_str(value)
                )
            }
            Val::List(items) => {
                let inner: Vec<String> = items.iter().map(Val::json).collect();
                format!("[{}]", inner.join(", "))
            }
        }
    }

    /// CSV/GraphML cell text (`_escape_csv_value`): scalars as `str(value)`,
    /// list/dict as `json.dumps(ensure_ascii=False)` (default separators).
    fn cell(&self) -> String {
        match self {
            Val::Int(i) => i.to_string(),
            Val::Float(f) => fmt_float(*f),
            Val::Bool(b) => {
                if *b {
                    "True".into()
                } else {
                    "False".into()
                }
            }
            Val::Str(s) => s.clone(),
            Val::Lang { .. } | Val::List(_) => self.json(),
        }
    }

    /// Cypher literal (`_cypher_escape`).
    fn cypher(&self) -> String {
        match self {
            Val::Int(i) => i.to_string(),
            Val::Float(f) => fmt_float(*f),
            Val::Bool(b) => {
                if *b {
                    "true".into()
                } else {
                    "false".into()
                }
            }
            Val::Str(s) => format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\"")),
            Val::Lang { value, lang } => {
                format!("{{lang: {}, value: {}}}", cy_str(lang), cy_str(value))
            }
            Val::List(items) => {
                let inner: Vec<String> = items.iter().map(Val::cypher).collect();
                format!("[{}]", inner.join(", "))
            }
        }
    }
}

fn cy_str(s: &str) -> String {
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
}

/// Python `repr`/`json` float formatting (shortest round-trip).
fn fmt_float(f: f64) -> String {
    if f == f.trunc() && f.is_finite() {
        format!("{f:.1}")
    } else {
        format!("{f}")
    }
}

/// `json.dumps(str, ensure_ascii=False)` of a string.
fn json_str(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn curie(iri: &str) -> String {
    // Longest namespace match (PREFIXES pre-sorted longest-first).
    for (prefix, ns) in PREFIXES_BY_LEN.iter() {
        if let Some(rest) = iri.strip_prefix(ns) {
            return format!("{prefix}:{rest}");
        }
    }
    iri.to_string()
}

fn short_key(iri: &str) -> String {
    let c = curie(iri);
    match c.split_once(':') {
        Some((_, local)) => local.to_string(),
        None => c,
    }
}

fn fold_value(term: &RdfTerm) -> Val {
    match term {
        RdfTerm::Iri(i) => Val::Str(curie(i)),
        RdfTerm::BlankNode(b) => Val::Str(format!("_bnode:{b}")),
        RdfTerm::Literal(RdfLiteral {
            lexical_form,
            datatype,
            language,
            ..
        }) => {
            let dt = datatype.as_deref().unwrap_or("");
            if dt == format!("{XSD}integer") {
                lexical_form
                    .parse::<i64>()
                    .map(Val::Int)
                    .unwrap_or_else(|_| Val::Str(lexical_form.clone()))
            } else if dt == format!("{XSD}decimal")
                || dt == format!("{XSD}double")
                || dt == format!("{XSD}float")
            {
                lexical_form
                    .parse::<f64>()
                    .map(Val::Float)
                    .unwrap_or_else(|_| Val::Str(lexical_form.clone()))
            } else if dt == format!("{XSD}boolean") {
                Val::Bool(matches!(lexical_form.to_lowercase().as_str(), "true" | "1"))
            } else if let Some(lang) = language {
                Val::Lang {
                    value: lexical_form.clone(),
                    lang: lang.clone(),
                }
            } else {
                Val::Str(lexical_form.clone())
            }
        }
        RdfTerm::Triple(_) => Val::Str(String::new()),
    }
}

fn term_iri(term: &RdfTerm) -> Option<&str> {
    match term {
        RdfTerm::Iri(i) => Some(i),
        _ => None,
    }
}

struct Node {
    id: String,
    labels: BTreeSet<String>,
    props: BTreeMap<String, PropVal>,
}

/// A property accumulates to a single value or a list (Python `_accumulate`).
#[derive(Clone)]
enum PropVal {
    One(Val),
    Many(Vec<Val>),
}

impl PropVal {
    fn to_val(&self) -> Val {
        match self {
            PropVal::One(v) => v.clone(),
            // The objects of a property are an RDF SET — no inherent order. Emit them in
            // a canonical (deterministic, content-keyed) order so the projection is
            // independent of carrier-assembly vs gts-round-trip quad ordering (#1132):
            // the in-memory carrier and a re-imported `gmeow.gts` then yield identical
            // bytes. Sort by each value's stable JSON rendering.
            PropVal::Many(vs) => {
                let mut sorted = vs.clone();
                sorted.sort_by_key(Val::json);
                Val::List(sorted)
            }
        }
    }
}

fn accumulate(map: &mut BTreeMap<String, PropVal>, key: String, value: Val) {
    match map.remove(&key) {
        None => {
            map.insert(key, PropVal::One(value));
        }
        Some(PropVal::One(existing)) => {
            map.insert(key, PropVal::Many(vec![existing, value]));
        }
        Some(PropVal::Many(mut vs)) => {
            vs.push(value);
            map.insert(key, PropVal::Many(vs));
        }
    }
}

struct Edge {
    id: String,
    source: String,
    target: String,
    etype: String,
    props: BTreeMap<String, Val>,
}

/// Build the LPG (nodes + edges) from an `RdfDataset` fold.
fn build_lpg(store: &RdfDataset) -> Result<(Vec<Node>, Vec<Edge>), PipelineError> {
    // Reifier tables.
    let mut reifier_triple: BTreeMap<String, (String, String, String)> = BTreeMap::new();
    let mut reifier_iris: BTreeSet<String> = BTreeSet::new();
    for r in store.owned_reifiers() {
        if let RdfTerm::Iri(rid) = &r.reifier {
            reifier_iris.insert(rid.clone());
            let s = lex(&r.statement.subject);
            let p = r.statement.predicate.clone();
            let o = lex(&r.statement.object);
            reifier_triple.insert(rid.clone(), (s, p, o));
        }
    }
    let mut reifier_meta: BTreeMap<String, BTreeMap<String, PropVal>> = BTreeMap::new();
    let mut ann_iri_values: BTreeSet<String> = BTreeSet::new();
    for a in store.owned_annotations() {
        if let RdfTerm::Iri(rid) = &a.reifier {
            reifier_iris.insert(rid.clone());
            accumulate(
                reifier_meta.entry(rid.clone()).or_default(),
                a.predicate.clone(),
                fold_value(&a.object),
            );
        }
        if let RdfTerm::Iri(v) = &a.object {
            ann_iri_values.insert(v.clone());
        }
    }
    // triple_meta keyed by (s,p,o) → list of short-keyed meta maps.
    let mut triple_meta: BTreeMap<(String, String, String), Vec<BTreeMap<String, Val>>> =
        BTreeMap::new();
    for (rid, (ts, tp, to)) in &reifier_triple {
        let meta = reifier_meta.get(rid).cloned().unwrap_or_default();
        let short: BTreeMap<String, Val> = meta
            .iter()
            .map(|(k, v)| (short_key(k), v.to_val()))
            .collect();
        triple_meta
            .entry((ts.clone(), tp.clone(), to.clone()))
            .or_default()
            .push(short);
    }

    // One pass over the statement-graph quads.
    let mut node_labels: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut node_props: BTreeMap<String, BTreeMap<String, PropVal>> = BTreeMap::new();
    let mut object_rows: Vec<(String, String, String)> = Vec::new();

    for q in store.owned_quads() {
        // Scope to the statement named graph.
        if term_iri_opt(&q.graph_name).as_deref() != Some(STATEMENTS_GRAPH) {
            continue;
        }
        let RdfTerm::Iri(subject) = &q.subject else {
            continue; // skip bnode subjects
        };
        if q.predicate == RDF_TYPE {
            if reifier_iris.contains(subject) {
                continue;
            }
            if let Some(type_iri) = term_iri(&q.object) {
                if !SKIP_LABELS.contains(&type_iri) {
                    node_labels
                        .entry(subject.clone())
                        .or_default()
                        .insert(curie(type_iri));
                }
            }
            node_labels.entry(subject.clone()).or_default();
        } else if let RdfTerm::Literal(_) = &q.object {
            if reifier_iris.contains(subject) {
                continue;
            }
            accumulate(
                node_props.entry(subject.clone()).or_default(),
                short_key(&q.predicate),
                fold_value(&q.object),
            );
            node_labels.entry(subject.clone()).or_default();
        } else if let Some(obj) = term_iri(&q.object) {
            if reifier_iris.contains(subject) || reifier_iris.contains(obj) {
                continue;
            }
            node_labels.entry(subject.clone()).or_default();
            node_labels.entry(obj.to_string()).or_default();
            object_rows.push((subject.clone(), q.predicate.clone(), obj.to_string()));
        }
    }
    for v in &ann_iri_values {
        if !reifier_iris.contains(v) {
            node_labels.entry(v.clone()).or_default();
        }
    }

    // Nodes (merge by curie id, sorted).
    let mut nodes_by_id: BTreeMap<String, Node> = BTreeMap::new();
    for (resource, labels) in &node_labels {
        let mut props = node_props.get(resource).cloned().unwrap_or_default();
        props.insert("uri".to_string(), PropVal::One(Val::Str(resource.clone())));
        if !labels.is_empty() {
            let types: Vec<Val> = labels.iter().map(|l| Val::Str(l.clone())).collect();
            // sorted labels list
            props.insert("types".to_string(), PropVal::One(Val::List(types)));
        }
        let id = curie(resource);
        let node = Node {
            id: id.clone(),
            labels: labels.clone(),
            props,
        };
        merge_node(&mut nodes_by_id, node);
    }

    // Edges.
    let mut edges: Vec<Edge> = Vec::new();
    for (subject, predicate, obj) in &object_rows {
        if TBOX_PREDICATES.contains(&predicate.as_str()) {
            continue;
        }
        let source_id = curie(subject);
        let target_id = curie(obj);
        let etype = short_key(predicate);
        let metas = triple_meta
            .get(&(subject.clone(), predicate.clone(), obj.clone()))
            .cloned()
            .unwrap_or_else(|| vec![BTreeMap::new()]);
        for meta in metas {
            let id = edge_id(&source_id, &target_id, &etype, &meta);
            edges.push(Edge {
                id,
                source: source_id.clone(),
                target: target_id.clone(),
                etype: etype.clone(),
                props: meta,
            });
        }
    }

    let mut nodes: Vec<Node> = nodes_by_id.into_values().collect();
    nodes.sort_by(|a, b| a.id.cmp(&b.id));
    edges.sort_by(|a, b| a.id.cmp(&b.id));
    Ok((nodes, edges))
}

fn merge_node(map: &mut BTreeMap<String, Node>, node: Node) {
    match map.get_mut(&node.id) {
        None => {
            map.insert(node.id.clone(), node);
        }
        Some(existing) => {
            existing.labels.extend(node.labels);
            for (k, v) in node.props {
                existing.props.insert(k, v);
            }
        }
    }
}

fn lex(term: &RdfTerm) -> String {
    match term {
        RdfTerm::Iri(i) => i.clone(),
        RdfTerm::BlankNode(b) => b.clone(),
        RdfTerm::Literal(l) => l.lexical_form.clone(),
        RdfTerm::Triple(_) => String::new(),
    }
}

fn term_iri_opt(t: &Option<RdfTerm>) -> Option<String> {
    match t {
        Some(RdfTerm::Iri(i)) => Some(i.clone()),
        _ => None,
    }
}

fn edge_id(source: &str, target: &str, etype: &str, props: &BTreeMap<String, Val>) -> String {
    // json.dumps(props, sort_keys=True, ensure_ascii=False): {"k": v, ...} sorted.
    let entries: Vec<String> = props
        .iter()
        .map(|(k, v)| format!("{}: {}", json_str(k), v.json()))
        .collect();
    let props_json = format!("{{{}}}", entries.join(", "));
    let payload = format!("{source}|{target}|{etype}|{props_json}");
    let digest = Sha256::digest(payload.as_bytes());
    let hex: String = digest.iter().take(8).map(|b| format!("{b:02x}")).collect();
    format!("edge:{hex}")
}

// ── CSV (csv.DictWriter QUOTE_MINIMAL, lineterminator "\n") ───────────────────

fn csv_field(s: &str) -> String {
    if s.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

fn csv_row(cols: &[String]) -> String {
    cols.iter()
        .map(|c| csv_field(c))
        .collect::<Vec<_>>()
        .join(",")
        + "\n"
}

fn render_all(nodes: &[Node], edges: &[Edge]) -> BTreeMap<String, Vec<u8>> {
    let mut out: BTreeMap<String, Vec<u8>> = BTreeMap::new();

    // --- generic nodes.csv ---
    let mut node_keys: BTreeSet<String> = BTreeSet::new();
    for n in nodes {
        node_keys.extend(n.props.keys().cloned());
    }
    let extra_node: Vec<String> = node_keys
        .iter()
        .filter(|k| !matches!(k.as_str(), "id" | "labels" | "uri"))
        .cloned()
        .collect();
    let mut node_cols = vec!["id".to_string(), "labels".to_string(), "uri".to_string()];
    node_cols.extend(extra_node.clone());
    let mut s = String::new();
    s.push_str(&csv_row(&node_cols));
    for n in nodes {
        let mut row = vec![
            n.id.clone(),
            n.labels.iter().cloned().collect::<Vec<_>>().join(";"),
            prop_cell(&n.props, "uri"),
        ];
        for k in &extra_node {
            row.push(prop_cell(&n.props, k));
        }
        s.push_str(&csv_row(&row));
    }
    out.insert(format!("{LPG_DIR}/nodes.csv"), s.into_bytes());

    // --- generic edges.csv ---
    let mut edge_keys: BTreeSet<String> = BTreeSet::new();
    for e in edges {
        edge_keys.extend(e.props.keys().cloned());
    }
    let extra_edge: Vec<String> = edge_keys
        .iter()
        .filter(|k| !matches!(k.as_str(), "id" | "source" | "target" | "type"))
        .cloned()
        .collect();
    let mut edge_cols = vec![
        "id".to_string(),
        "source".to_string(),
        "target".to_string(),
        "type".to_string(),
    ];
    edge_cols.extend(extra_edge.clone());
    let mut s = String::new();
    s.push_str(&csv_row(&edge_cols));
    for e in edges {
        let mut row = vec![
            e.id.clone(),
            e.source.clone(),
            e.target.clone(),
            e.etype.clone(),
        ];
        for k in &extra_edge {
            row.push(e.props.get(k).map(|v| v.cell()).unwrap_or_default());
        }
        s.push_str(&csv_row(&row));
    }
    out.insert(format!("{LPG_DIR}/edges.csv"), s.into_bytes());

    // --- neo4j/nodes.csv ---
    let neo_node_cols: Vec<String> = {
        let mut c = vec!["id:ID".to_string(), ":LABEL".to_string()];
        let mut keys: BTreeSet<String> = node_keys.clone();
        keys.remove("uri");
        c.extend(keys);
        c
    };
    let mut s = String::new();
    s.push_str(&csv_row(&neo_node_cols));
    for n in nodes {
        let mut row = vec![
            n.id.clone(),
            if n.labels.is_empty() {
                "Resource".to_string()
            } else {
                n.labels.iter().cloned().collect::<Vec<_>>().join(";")
            },
        ];
        for k in &neo_node_cols[2..] {
            row.push(prop_cell(&n.props, k));
        }
        s.push_str(&csv_row(&row));
    }
    out.insert(format!("{LPG_DIR}/neo4j/nodes.csv"), s.into_bytes());

    // --- neo4j/edges_<type>.csv (per edge type) ---
    let edge_prop_cols: Vec<String> = edge_keys.iter().cloned().collect();
    let mut by_type: BTreeMap<String, Vec<&Edge>> = BTreeMap::new();
    for e in edges {
        by_type.entry(e.etype.clone()).or_default().push(e);
    }
    for (type_name, type_edges) in &by_type {
        let safe = type_name.replace(':', "_");
        let mut cols = vec![
            "id:ID".to_string(),
            ":START_ID".to_string(),
            ":END_ID".to_string(),
            ":TYPE".to_string(),
        ];
        cols.extend(edge_prop_cols.clone());
        let mut s = String::new();
        s.push_str(&csv_row(&cols));
        for e in type_edges {
            let mut row = vec![
                e.id.clone(),
                e.source.clone(),
                e.target.clone(),
                type_name.clone(),
            ];
            for k in &edge_prop_cols {
                row.push(e.props.get(k).map(|v| v.cell()).unwrap_or_default());
            }
            s.push_str(&csv_row(&row));
        }
        out.insert(format!("{LPG_DIR}/neo4j/edges_{safe}.csv"), s.into_bytes());
    }

    // --- cypher ---
    out.insert(
        format!("{LPG_DIR}/gmeow.cypher"),
        render_cypher(nodes, edges).into_bytes(),
    );
    // --- graphml ---
    out.insert(
        format!("{LPG_DIR}/gmeow.graphml"),
        render_graphml(nodes, edges).into_bytes(),
    );
    out
}

fn prop_cell(props: &BTreeMap<String, PropVal>, key: &str) -> String {
    match props.get(key) {
        None => String::new(),
        Some(p) => p.to_val().cell(),
    }
}

fn render_cypher(nodes: &[Node], edges: &[Edge]) -> String {
    let mut lines: Vec<String> = vec![
        "// GMEOW LPG export — generated by `gmeow export lpg`".to_string(),
        "// DO NOT EDIT — regenerate from canonical sources.".to_string(),
        String::new(),
    ];
    for n in nodes {
        let labels: String = if n.labels.is_empty() {
            ":Resource".to_string()
        } else {
            n.labels
                .iter()
                .map(|l| format!(":{}", l.replace(':', "_")))
                .collect()
        };
        // props incl uri, sorted
        let mut props: BTreeMap<String, Val> = n
            .props
            .iter()
            .map(|(k, v)| (k.clone(), v.to_val()))
            .collect();
        props.insert("uri".to_string(), Val::Str(n.id.clone()));
        let prop_str: Vec<String> = props
            .iter()
            .map(|(k, v)| format!("{k}: {}", v.cypher()))
            .collect();
        lines.push(format!("CREATE (n{labels} {{{}}});", prop_str.join(", ")));
        lines.push(String::new());
    }
    for e in edges {
        let rel = e.etype.replace(':', "_");
        if e.props.is_empty() {
            lines.push(format!(
                "MATCH (a), (b) WHERE a.uri = {} AND b.uri = {} CREATE (a)-[:{rel}]->(b);",
                cy_str(&e.source),
                cy_str(&e.target)
            ));
        } else {
            let prop_str: Vec<String> = e
                .props
                .iter()
                .map(|(k, v)| format!("{k}: {}", v.cypher()))
                .collect();
            lines.push(format!(
                "MATCH (a), (b) WHERE a.uri = {} AND b.uri = {} CREATE (a)-[:{rel} {{{}}}]->(b);",
                cy_str(&e.source),
                cy_str(&e.target),
                prop_str.join(", ")
            ));
        }
    }
    lines.join("\n") + "\n"
}

fn xml_attr(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}
fn xml_text(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn render_graphml(nodes: &[Node], edges: &[Edge]) -> String {
    let mut node_keys: BTreeSet<String> = BTreeSet::from(["label".to_string()]);
    let mut edge_keys: BTreeSet<String> = BTreeSet::from(["label".to_string()]);
    for n in nodes {
        node_keys.extend(n.props.keys().cloned());
    }
    for e in edges {
        edge_keys.extend(e.props.keys().cloned());
    }
    let mut s = String::new();
    s.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    s.push_str("<graphml xmlns=\"http://graphml.graphdrawing.org/xmlns\">");
    for k in &node_keys {
        s.push_str(&format!(
            "<key id=\"{0}\" for=\"node\" attr.name=\"{0}\" attr.type=\"string\" />",
            xml_attr(k)
        ));
    }
    for k in &edge_keys {
        s.push_str(&format!(
            "<key id=\"{0}\" for=\"edge\" attr.name=\"{0}\" attr.type=\"string\" />",
            xml_attr(k)
        ));
    }
    s.push_str("<graph id=\"G\" edgedefault=\"directed\">");
    for n in nodes {
        s.push_str(&format!("<node id=\"{}\">", xml_attr(&n.id)));
        for label in &n.labels {
            s.push_str(&format!("<data key=\"label\">{}</data>", xml_text(label)));
        }
        for (k, v) in &n.props {
            s.push_str(&format!(
                "<data key=\"{}\">{}</data>",
                xml_attr(k),
                xml_text(&v.to_val().cell())
            ));
        }
        s.push_str("</node>");
    }
    for e in edges {
        s.push_str(&format!(
            "<edge id=\"{}\" source=\"{}\" target=\"{}\">",
            xml_attr(&e.id),
            xml_attr(&e.source),
            xml_attr(&e.target)
        ));
        s.push_str(&format!(
            "<data key=\"label\">{}</data>",
            xml_text(&e.etype)
        ));
        for (k, v) in &e.props {
            s.push_str(&format!(
                "<data key=\"{}\">{}</data>",
                xml_attr(k),
                xml_text(&v.cell())
            ));
        }
        s.push_str("</edge>");
    }
    s.push_str("</graph></graphml>");
    s.push('\n');
    s
}

// ── Stage impl ───────────────────────────────────────────────────────────────

/// The `lpg` export-leaf stage.
pub struct LpgStage {
    consumes: Vec<String>,
}

impl LpgStage {
    /// Construct the stage; it consumes THIS run's snapshot fold.
    pub fn new() -> Self {
        Self {
            consumes: vec!["stage-snapshot".to_string()],
        }
    }
}

impl Default for LpgStage {
    fn default() -> Self {
        Self::new()
    }
}

impl Stage for LpgStage {
    fn id(&self) -> &str {
        "stage-export-lpg"
    }
    fn consumes(&self) -> &[String] {
        &self.consumes
    }
    fn impl_version(&self) -> &str {
        "lpg.v1"
    }
    fn run(&self, input: StageInput<'_>) -> Result<StageOutput, PipelineError> {
        // Consume THIS run's snapshot carrier dataset DIRECTLY off the product bundle —
        // no re-parse of the gmeow.gts bytes (GTS is exit-only).
        let dataset = crate::stages::carrier::snapshot_dataset(input.upstream)?;
        Ok(StageOutput {
            product: StageProduct::from_artifacts(
                self.id(),
                render_from_dataset(dataset.as_ref())?,
            ),
        })
    }
}

/// Project the LPG (neo4j CSV tree + generic CSV + Cypher + GraphML) from the
/// carrier `dataset`. The snapshot gatherer calls this to attach the opaque LPG
/// fanout as a blob (superset law); the export leaf calls it for the disk fanout.
pub(crate) fn render_from_dataset(
    dataset: &RdfDataset,
) -> Result<BTreeMap<String, Vec<u8>>, PipelineError> {
    let (nodes, edges) = build_lpg(dataset)?;
    Ok(render_all(&nodes, &edges))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn repo_root() -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .canonicalize()
            .unwrap()
    }

    #[test]
    fn lpg_is_byte_identical_to_committed() {
        let root = repo_root();
        let gts = std::fs::read(root.join("generated/dist/gmeow.gts")).unwrap();
        let bundle = gmeow_rdf::import_gts_events(&gts).unwrap();
        let (nodes, edges) = build_lpg(bundle.dataset.as_ref()).unwrap();
        let arts = render_all(&nodes, &edges);
        let mut checked = 0;
        for (path, bytes) in &arts {
            let committed = std::fs::read(root.join(path))
                .unwrap_or_else(|_| panic!("committed missing: {path}"));
            assert_eq!(
                bytes, &committed,
                "{path} drifted from committed (lpg byte-parity)"
            );
            checked += 1;
        }
        assert!(checked >= 37, "expected 37+ lpg files, got {checked}");
    }
}
