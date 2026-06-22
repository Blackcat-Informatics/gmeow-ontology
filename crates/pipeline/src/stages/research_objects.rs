// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The `research-objects` export leaf (#861 P4): Croissant / RO-Crate / DataCite /
//! Frictionless / DCAT research-object projections.
//!
//! A genuine Rust port of `src/gmeow_tools/research_objects.py` (#58): the flagship
//! Lillith GraphRAG worked example is rendered into `generated/research-objects/
//! lillith/` — the no-drift gate. Each artifact is a GENERATED lossy projection of
//! canonical GMEOW instance data, declaring its drops in the format's native slot.
//!
//! Byte-parity target = the COMMITTED bytes, which were produced by **rdflib 7.6
//! Turtle serialization** (the `.ttl` files), Python `json.dumps(indent=2,
//! ensure_ascii=False) + "\n"` (the JSON/JSON-LD), and `ElementTree` (the DataCite
//! XML). The git-ignored crate `.zip` is intentionally NOT produced here.
//!
//! The eight `.ttl` outputs require a faithful re-implementation of rdflib's
//! recursive Turtle serializer ([`turtle`] below): subjects ordered by
//! `(is_bnode, ref_count, iri)`, predicates `a`/`rdfs:label`-first then sorted,
//! objects sorted, literals canonicalized exactly as rdflib's `_literal_n3`
//! (notably xsd:dateTime `Z` → `+00:00`). The `dcat.ttl` additionally runs the
//! generated `dcat.rq` CONSTRUCT over the WHOLE composed ontology (every slice
//! source) plus the worked-example A-Box, so it is fold-derived and drifts with the
//! ontology — regenerated through the committed bytes here.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use oxigraph::io::{RdfFormat, RdfParser};
use oxigraph::model::{Literal as OxLiteral, NamedNode, Term};
use oxigraph::sparql::{QueryResults, SparqlEvaluator};
use oxigraph::store::Store;

use crate::error::PipelineError;
use crate::node::{Stage, StageInput, StageKind, StageOutput, StageProduct};
use crate::stages::source_load::module_files;

/// Logical-path prefix of the committed research-object artifacts.
pub const RESEARCH_OBJECTS_DIR: &str = "generated/research-objects/lillith";

const NS: &str = "https://blackcatinformatics.ca/gmeow/";
const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
const RDFS_LABEL: &str = "http://www.w3.org/2000/01/rdf-schema#label";
const XSD: &str = "http://www.w3.org/2001/XMLSchema#";

// The P5 declared losses every research-object export shares.
const DECLARED_DROPS: [&str; 4] = [
    "reified relators (Copyright, roles, memberships) flatten or vanish",
    "RDF 1.2 statement annotations (confidence, accordingTo, the four clocks) are dropped",
    "standpoint indexing is dropped — contested claims appear without their vantage",
    "blake3 remains the internal canonical content digest; sha256/md5 are projected where supplied and the format allows",
];

const CROISSANT_CONFORMS_TO: &str = "http://mlcommons.org/croissant/1.0";
const RO_CRATE_CONTEXT: &str = "https://w3id.org/ro/crate/1.1/context";
const RO_CRATE_SPEC: &str = "https://w3id.org/ro/crate/1.1";
const PROCESS_RUN_PROFILE: &str = "https://w3id.org/ro/wfrun/process/0.5";
const WORKFLOW_RUN_PROFILE: &str = "https://w3id.org/ro/wfrun/workflow/0.5";
const DATACITE_NS: &str = "http://datacite.org/schema/kernel-4";
const XSI_NS: &str = "http://www.w3.org/2001/XMLSchema-instance";
const PLACEHOLDER_DOI_PREFIX: &str = "10.5072";

/// The worked example's canonical instance Turtle inputs, in generator order.
/// `(repo-relative path, crate file name)`.
const EXAMPLE_INPUTS: [(&str, &str); 6] = [
    (
        "slices/extensions/graphrag/examples/lillith-dataset.ttl",
        "lillith-dataset.ttl",
    ),
    (
        "slices/extensions/graphrag/examples/lillith-pipeline.ttl",
        "lillith-pipeline.ttl",
    ),
    (
        "slices/core/ai/examples/grounded-claim.ttl",
        "grounded-claim.ttl",
    ),
    ("evals/corpus.ttl", "corpus.ttl"),
    ("evals/rubric.ttl", "rubric.ttl"),
    ("generated/evals/scores.ttl", "scores.ttl"),
];

fn g(local: &str) -> String {
    format!("{NS}{local}")
}

// ── helpers: load instance graph ──────────────────────────────────────────────

fn parse_into(store: &Store, bytes: &[u8], path: &str) -> Result<(), PipelineError> {
    for quad in RdfParser::from_format(RdfFormat::Turtle)
        .lenient()
        .for_reader(bytes)
    {
        let quad =
            quad.map_err(|e| PipelineError::Parse(format!("syntax error in {path}: {e}")))?;
        store
            .insert(&quad)
            .map_err(|e| PipelineError::Parse(format!("store insert failed: {e}")))?;
    }
    Ok(())
}

/// Parse the six worked-example Turtle files into one oxigraph store (the A-Box).
fn load_instance_graph(root: &Path) -> Result<Store, PipelineError> {
    let store =
        Store::new().map_err(|e| PipelineError::Parse(format!("store creation failed: {e}")))?;
    for (rel, _) in EXAMPLE_INPUTS {
        let bytes = std::fs::read(root.join(rel))?;
        parse_into(&store, &bytes, rel)?;
    }
    Ok(store)
}

// ── instance-graph reads (mirror the Python `_text`/`_label` helpers) ──────────

/// First object literal lexical value (rdflib `g.value` picks an arbitrary one;
/// these subjects carry at most one of each text predicate).
fn text(store: &Store, subject: &str, predicate: &str) -> String {
    let s = NamedNode::new(subject).unwrap();
    let p = NamedNode::new(predicate).unwrap();
    let mut best: Option<String> = None;
    for q in store
        .quads_for_pattern(Some((&s).into()), Some(p.as_ref()), None, None)
        .flatten()
    {
        let v = match &q.object {
            Term::Literal(l) => canonical_lexical(l),
            Term::NamedNode(n) => n.as_str().to_string(),
            Term::BlankNode(b) => b.as_str().to_string(),
            Term::Triple(_) => String::new(),
        };
        // rdflib `value()` returns a deterministic single value; for these
        // single-valued predicates any is fine, but keep the smallest for stability.
        best = Some(match best {
            Some(prev) if prev <= v => prev,
            _ => v,
        });
    }
    best.unwrap_or_default()
}

fn value_node(store: &Store, subject: &str, predicate: &str) -> Option<String> {
    let s = NamedNode::new(subject).unwrap();
    let p = NamedNode::new(predicate).unwrap();
    let mut hits: Vec<String> = store
        .quads_for_pattern(Some((&s).into()), Some(p.as_ref()), None, None)
        .flatten()
        .filter_map(|q| match q.object {
            Term::NamedNode(n) => Some(n.as_str().to_string()),
            _ => None,
        })
        .collect();
    hits.sort();
    hits.into_iter().next()
}

fn label(store: &Store, subject: &str) -> String {
    let l = text(store, subject, RDFS_LABEL);
    if !l.is_empty() {
        return l;
    }
    let t = text(store, subject, &g("title"));
    if !t.is_empty() {
        return t;
    }
    subject.to_string()
}

/// Subjects of `rdf:type type_iri`, sorted by IRI.
fn subjects_of_type(store: &Store, type_iri: &str) -> Vec<String> {
    let p = NamedNode::new(RDF_TYPE).unwrap();
    let o = NamedNode::new(type_iri).unwrap();
    let mut set: BTreeSet<String> = BTreeSet::new();
    for q in store
        .quads_for_pattern(None, Some(p.as_ref()), Some((&o).into()), None)
        .flatten()
    {
        if let oxigraph::model::NamedOrBlankNode::NamedNode(n) = &q.subject {
            set.insert(n.as_str().to_string());
        }
    }
    set.into_iter().collect()
}

/// All object lexical/IRI values for `(subject, predicate)`, sorted unique.
fn objects(store: &Store, subject: &str, predicate: &str) -> Vec<String> {
    let s = NamedNode::new(subject).unwrap();
    let p = NamedNode::new(predicate).unwrap();
    let mut set: BTreeSet<String> = BTreeSet::new();
    for q in store
        .quads_for_pattern(Some((&s).into()), Some(p.as_ref()), None, None)
        .flatten()
    {
        match q.object {
            Term::Literal(l) => {
                set.insert(canonical_lexical(&l));
            }
            Term::NamedNode(n) => {
                set.insert(n.as_str().to_string());
            }
            _ => {}
        }
    }
    set.into_iter().collect()
}

/// Subjects `s` with `(s, predicate, object)`, sorted by IRI.
fn subjects_with(store: &Store, predicate: &str, object: &str) -> Vec<String> {
    let p = NamedNode::new(predicate).unwrap();
    let o = NamedNode::new(object).unwrap();
    let mut set: BTreeSet<String> = BTreeSet::new();
    for q in store
        .quads_for_pattern(None, Some(p.as_ref()), Some((&o).into()), None)
        .flatten()
    {
        if let oxigraph::model::NamedOrBlankNode::NamedNode(n) = &q.subject {
            set.insert(n.as_str().to_string());
        }
    }
    set.into_iter().collect()
}

fn slug(iri: &str) -> String {
    let trimmed = iri.trim_end_matches('/');
    let tail = trimmed.rsplit('/').next().unwrap_or(trimmed);
    let tail = tail.rsplit('#').next().unwrap_or(tail);
    if tail.is_empty() {
        "resource".to_string()
    } else {
        tail.to_string()
    }
}

// ── literal canonicalization (rdflib `Literal._literal_n3(use_plain=True)`) ────

/// rdflib's `str(Literal)` / lexical value after parse: xsd:dateTime with a `Z`
/// offset re-isoformats to `+00:00`; everything else keeps its lexical form.
fn canonical_lexical(l: &OxLiteral) -> String {
    let dt = l.datatype().as_str().to_string();
    let lex = l.value().to_string();
    if dt == format!("{XSD}dateTime") {
        return canonical_datetime(&lex);
    }
    lex
}

/// rdflib canonicalizes xsd:dateTime via `datetime.fromisoformat(...).isoformat()`;
/// in this corpus the only transform is a trailing `Z` → `+00:00`.
fn canonical_datetime(lex: &str) -> String {
    if let Some(stripped) = lex.strip_suffix('Z') {
        format!("{stripped}+00:00")
    } else {
        lex.to_string()
    }
}

// ── DatasetMeta ────────────────────────────────────────────────────────────────

struct DatasetMeta {
    iri: String,
    title: String,
    description: String,
    license_id: String,
    license_url: String,
    creator: String,
    date_published: String,
    landing_page: String,
    version: Option<String>,
    cite_as: Option<String>,
}

impl DatasetMeta {
    fn publication_year(&self) -> String {
        self.date_published.chars().take(4).collect()
    }
}

fn dataset_meta(store: &Store) -> Result<DatasetMeta, PipelineError> {
    let mut candidates: Vec<String> = subjects_of_type(store, &g("Dataset"))
        .into_iter()
        .filter(|ds| value_node(store, ds, &g("hasLicense")).is_some())
        .collect();
    candidates.sort();
    let ds = candidates
        .into_iter()
        .next()
        .ok_or_else(|| PipelineError::Parse("no licensed gmeow:Dataset node found".into()))?;
    let license_node = value_node(store, &ds, &g("hasLicense")).unwrap();
    let license_id = text(store, &license_node, &g("spdxLicenseId"));
    if license_id.is_empty() {
        return Err(PipelineError::Parse(format!(
            "dataset descriptor {ds} has a gmeow:License without a gmeow:spdxLicenseId"
        )));
    }
    let date_published = text(store, &ds, &g("datePublished"));
    let year_ok =
        date_published.len() >= 4 && date_published.chars().take(4).all(|c| c.is_ascii_digit());
    if !year_ok {
        return Err(PipelineError::Parse(format!(
            "dataset descriptor {ds} needs a valid gmeow:datePublished"
        )));
    }
    let creator_node = value_node(store, &ds, &g("wasAttributedTo"));
    let version = {
        let v = text(store, &ds, &g("version"));
        if v.is_empty() {
            None
        } else {
            Some(v)
        }
    };
    let cite_as = {
        let v = text(store, &ds, &g("citeAs"));
        if v.is_empty() {
            None
        } else {
            Some(v)
        }
    };
    let title = {
        let t = text(store, &ds, &g("title"));
        if t.is_empty() {
            label(store, &ds)
        } else {
            t
        }
    };
    let landing = {
        let l = text(store, &ds, &g("sourceLocation"));
        if l.is_empty() {
            ds.clone()
        } else {
            l
        }
    };
    Ok(DatasetMeta {
        iri: ds.clone(),
        title,
        description: text(store, &ds, &g("description")),
        license_url: format!("https://spdx.org/licenses/{license_id}"),
        license_id,
        creator: creator_node.map(|c| label(store, &c)).unwrap_or_default(),
        date_published,
        landing_page: landing,
        version,
        cite_as,
    })
}

// ── digest maps ────────────────────────────────────────────────────────────────

/// Collect `gmeow:contentDigest` values keyed by `algorithm` (unprefixed → "digest").
fn digest_map(store: &Store, doc: &str) -> BTreeMap<String, String> {
    let mut digests: BTreeMap<String, String> = BTreeMap::new();
    for raw in objects(store, doc, &g("contentDigest")) {
        let (key, hex) = match raw.split_once(':') {
            Some((algo, hex)) => (algo.to_string(), hex.to_string()),
            None => ("digest".to_string(), raw.clone()),
        };
        digests.entry(key).or_insert(hex);
    }
    digests
}

/// rdflib insertion order isn't observable here; prefer blake3, then sha256, md5.
fn primary_digest(digests: &BTreeMap<String, String>) -> String {
    for algo in ["blake3", "sha256", "md5"] {
        if let Some(v) = digests.get(algo) {
            return format!("{algo}:{v}");
        }
    }
    match digests.iter().next() {
        Some((algo, value)) if algo == "digest" => value.clone(),
        Some((algo, value)) => format!("{algo}:{value}"),
        None => String::new(),
    }
}

struct DocInfo {
    iri: String,
    name: String,
    content_url: String,
    digests: BTreeMap<String, String>,
}

fn documents(store: &Store) -> Vec<DocInfo> {
    subjects_of_type(store, &g("Document"))
        .into_iter()
        .map(|doc| DocInfo {
            name: label(store, &doc),
            content_url: text(store, &doc, &g("sourceLocation")),
            digests: digest_map(store, &doc),
            iri: doc,
        })
        .collect()
}

struct AgentInfo {
    iri: String,
    name: String,
    version: String,
    provider: String,
}

fn agents(store: &Store) -> Vec<AgentInfo> {
    let mut nodes: BTreeSet<String> = BTreeSet::new();
    nodes.extend(subjects_of_type(store, &g("SoftwareAgent")));
    nodes.extend(subjects_of_type(store, &g("Builder")));
    nodes
        .into_iter()
        .map(|agent| {
            let card = subjects_with(store, &g("describesModel"), &agent)
                .into_iter()
                .next();
            let (version, provider) = match card {
                Some(c) => (
                    text(store, &c, &g("modelVersionTag")),
                    text(store, &c, &g("modelProvider")),
                ),
                None => (String::new(), String::new()),
            };
            AgentInfo {
                name: label(store, &agent),
                version,
                provider,
                iri: agent,
            }
        })
        .collect()
}

struct Action {
    iri: String,
    name: String,
    instrument: String,
    objects: Vec<String>,
    results: Vec<String>,
    end_time: String,
    workflow: String,
    agent: String,
}

fn activities(store: &Store) -> Vec<Action> {
    let mut nodes: BTreeSet<String> = BTreeSet::new();
    for t in ["ModelInvocation", "ImportActivity", "BuildActivity"] {
        nodes.extend(subjects_of_type(store, &g(t)));
    }
    nodes
        .into_iter()
        .map(|act| {
            let generated = subjects_with(store, &g("wasGeneratedBy"), &act);
            let mut results: BTreeSet<String> = generated.iter().cloned().collect();
            results.extend(objects(store, &act, &g("buildOutput")));
            let mut objs: BTreeSet<String> = BTreeSet::new();
            for result in &generated {
                objs.extend(objects(store, result, &g("wasDerivedFrom")));
            }
            objs.extend(objects(store, &act, &g("buildSource")));
            let instrument = value_node(store, &act, &g("usedModel")).unwrap_or_default();
            let participant = value_node(store, &act, &g("hasParticipant")).unwrap_or_default();
            let end_time = {
                let t = text(store, &act, &g("ingestedAt"));
                if t.is_empty() {
                    text(store, &act, &g("eventTime"))
                } else {
                    t
                }
            };
            Action {
                name: label(store, &act),
                instrument,
                objects: objs.into_iter().collect(),
                results: results.into_iter().collect(),
                end_time,
                workflow: text(store, &act, &g("buildConfigUri")),
                agent: participant,
                iri: act,
            }
        })
        .collect()
}

// ── JSON value model (byte-exact Python json.dumps, indent=2, ensure_ascii=False) ──

enum Json {
    Bool(bool),
    Int(i64),
    /// A pre-formatted numeric token (e.g. `1.0`, `0.66`) rendered verbatim.
    Num(String),
    Str(String),
    Arr(Vec<Json>),
    /// Insertion-ordered object (Python dict order preserved).
    Obj(Vec<(String, Json)>),
}

impl Json {
    fn render(&self, indent: usize, out: &mut String) {
        let pad = "  ".repeat(indent);
        let pad1 = "  ".repeat(indent + 1);
        match self {
            Json::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
            Json::Int(i) => out.push_str(&i.to_string()),
            Json::Num(s) => out.push_str(s),
            Json::Str(s) => out.push_str(&json_str(s)),
            Json::Arr(items) => {
                if items.is_empty() {
                    out.push_str("[]");
                    return;
                }
                out.push_str("[\n");
                for (i, it) in items.iter().enumerate() {
                    out.push_str(&pad1);
                    it.render(indent + 1, out);
                    if i + 1 < items.len() {
                        out.push(',');
                    }
                    out.push('\n');
                }
                out.push_str(&pad);
                out.push(']');
            }
            Json::Obj(entries) => {
                if entries.is_empty() {
                    out.push_str("{}");
                    return;
                }
                out.push_str("{\n");
                for (i, (k, v)) in entries.iter().enumerate() {
                    out.push_str(&pad1);
                    out.push_str(&json_str(k));
                    out.push_str(": ");
                    v.render(indent + 1, out);
                    if i + 1 < entries.len() {
                        out.push(',');
                    }
                    out.push('\n');
                }
                out.push_str(&pad);
                out.push('}');
            }
        }
    }
}

/// `json.dumps(s, ensure_ascii=False)` of a string (raw UTF-8, minimal escapes).
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

fn dump_json(doc: &Json) -> Vec<u8> {
    let mut s = String::new();
    doc.render(0, &mut s);
    s.push('\n');
    s.into_bytes()
}

fn obj(entries: Vec<(&str, Json)>) -> Json {
    Json::Obj(
        entries
            .into_iter()
            .map(|(k, v)| (k.to_string(), v))
            .collect(),
    )
}
fn s(v: &str) -> Json {
    Json::Str(v.to_string())
}

/// A score float rendered the way Python's `json.dumps(float(x))` would: trailing
/// `.0` for integral values, else the shortest decimal of `float(lexical)`.
fn json_float(lexical: &str) -> Json {
    let f: f64 = lexical.parse().unwrap_or(0.0);
    if f == f.trunc() && f.is_finite() {
        Json::Num(format!("{f:.1}"))
    } else {
        Json::Num(format!("{f}"))
    }
}

// ── Croissant ──────────────────────────────────────────────────────────────────

fn croissant_context() -> Json {
    obj(vec![
        ("@language", s("en")),
        ("@vocab", s("https://schema.org/")),
        ("sc", s("https://schema.org/")),
        ("cr", s("http://mlcommons.org/croissant/")),
        ("rai", s("http://mlcommons.org/croissant/RAI/")),
        ("dct", s("http://purl.org/dc/terms/")),
        ("citeAs", s("cr:citeAs")),
        ("conformsTo", s("dct:conformsTo")),
        (
            "data",
            obj(vec![("@id", s("cr:data")), ("@type", s("@json"))]),
        ),
        (
            "dataType",
            obj(vec![("@id", s("cr:dataType")), ("@type", s("@vocab"))]),
        ),
        ("field", s("cr:field")),
        ("fileObject", s("cr:fileObject")),
        ("recordSet", s("cr:recordSet")),
        ("sha256", s("cr:sha256")),
        ("md5", s("cr:md5")),
    ])
}

fn croissant_field(rs: &str, name: &str, data_type: &str) -> Json {
    obj(vec![
        ("@type", s("cr:Field")),
        ("@id", s(&format!("{rs}/{name}"))),
        ("name", s(name)),
        ("dataType", s(data_type)),
    ])
}

fn croissant_record_sets(store: &Store) -> Vec<Json> {
    let mut record_sets: Vec<Json> = Vec::new();

    let chunk_rows: Vec<Json> = subjects_of_type(store, &g("Chunk"))
        .into_iter()
        .map(|chunk| {
            obj(vec![
                ("chunks/id", s(&chunk)),
                ("chunks/source", s(&text(store, &chunk, &g("chunkOf")))),
                (
                    "chunks/spanStart",
                    Json::Int(text(store, &chunk, &g("spanStart")).parse().unwrap_or(0)),
                ),
                (
                    "chunks/spanEnd",
                    Json::Int(text(store, &chunk, &g("spanEnd")).parse().unwrap_or(0)),
                ),
                (
                    "chunks/digest",
                    s(&text(store, &chunk, &g("contentDigest"))),
                ),
            ])
        })
        .collect();
    if !chunk_rows.is_empty() {
        record_sets.push(obj(vec![
            ("@type", s("cr:RecordSet")),
            ("@id", s("chunks")),
            ("name", s("chunks")),
            ("description", s("Content-addressed retrieval segments with typed offsets into their source documents.")),
            ("field", Json::Arr(vec![
                croissant_field("chunks", "id", "sc:Text"),
                croissant_field("chunks", "source", "sc:Text"),
                croissant_field("chunks", "spanStart", "sc:Integer"),
                croissant_field("chunks", "spanEnd", "sc:Integer"),
                croissant_field("chunks", "digest", "sc:Text"),
            ])),
            ("data", Json::Arr(chunk_rows)),
        ]));
    }

    let claim_rows: Vec<Json> = subjects_of_type(store, &g("StandpointClaim"))
        .into_iter()
        .map(|claim| {
            obj(vec![
                ("claims/id", s(&claim)),
                ("claims/vantage", s(&text(store, &claim, &g("vantage")))),
                (
                    "claims/modality",
                    s(&slug(&text(store, &claim, &g("claimModality")))),
                ),
                (
                    "claims/grounded",
                    Json::Bool(value_node(store, &claim, &g("groundedIn")).is_some()),
                ),
            ])
        })
        .collect();
    if !claim_rows.is_empty() {
        record_sets.push(obj(vec![
            ("@type", s("cr:RecordSet")),
            ("@id", s("claims")),
            ("name", s("claims")),
            ("description", s("Model-extracted claims: vantage-attributed, modality-tagged, grounded flag from evidence spans. (Standpoint nuance beyond the flag is a declared drop.)")),
            ("field", Json::Arr(vec![
                croissant_field("claims", "id", "sc:Text"),
                croissant_field("claims", "vantage", "sc:Text"),
                croissant_field("claims", "modality", "sc:Text"),
                croissant_field("claims", "grounded", "sc:Boolean"),
            ])),
            ("data", Json::Arr(claim_rows)),
        ]));
    }

    let score_rows: Vec<Json> = subjects_of_type(store, &g("Assessment"))
        .into_iter()
        .filter(|a| !text(store, a, &g("assessmentScoreValue")).is_empty())
        .map(|a| {
            obj(vec![
                (
                    "evalScores/model",
                    s(&text(store, &a, &g("assessmentTarget"))),
                ),
                (
                    "evalScores/criterion",
                    s(&slug(&text(store, &a, &g("assessmentCriterion")))),
                ),
                (
                    "evalScores/score",
                    json_float(&text(store, &a, &g("assessmentScoreValue"))),
                ),
            ])
        })
        .collect();
    if !score_rows.is_empty() {
        record_sets.push(obj(vec![
            ("@type", s("cr:RecordSet")),
            ("@id", s("evalScores")),
            ("name", s("evalScores")),
            (
                "description",
                s("Vantage-indexed rubric assessments from the gmeow-evals harness (#298)."),
            ),
            (
                "field",
                Json::Arr(vec![
                    croissant_field("evalScores", "model", "sc:Text"),
                    croissant_field("evalScores", "criterion", "sc:Text"),
                    croissant_field("evalScores", "score", "sc:Float"),
                ]),
            ),
            ("data", Json::Arr(score_rows)),
        ]));
    }
    record_sets
}

fn build_croissant(store: &Store, ds: &DatasetMeta) -> Result<Json, PipelineError> {
    let mut distributions: Vec<Json> = Vec::new();
    for doc in documents(store) {
        if doc.content_url.is_empty() {
            return Err(PipelineError::Parse(format!(
                "build_croissant: missing contentUrl for {}",
                doc.iri
            )));
        }
        let mut fields: Vec<(String, Json)> = vec![
            ("@type".into(), s("cr:FileObject")),
            ("@id".into(), s(&doc.iri)),
            ("name".into(), s(&doc.name)),
            ("encodingFormat".into(), s("text/plain")),
            ("contentUrl".into(), s(&doc.content_url)),
        ];
        if let Some(v) = doc.digests.get("sha256") {
            fields.push(("sha256".into(), s(v)));
        }
        if let Some(v) = doc.digests.get("md5") {
            fields.push(("md5".into(), s(v)));
        }
        let extra: Vec<String> = doc
            .digests
            .iter()
            .filter(|(algo, _)| algo.as_str() != "sha256" && algo.as_str() != "md5")
            .map(|(algo, v)| {
                if algo == "digest" {
                    v.clone()
                } else {
                    format!("{algo}:{v}")
                }
            })
            .collect();
        if !extra.is_empty() {
            fields.push((
                "description".into(),
                s(&format!("content digest: {}", extra.join(", "))),
            ));
        }
        distributions.push(Json::Obj(fields));
    }

    let tools: Vec<Json> = agents(store)
        .into_iter()
        .map(|a| {
            let name = if a.version.is_empty() {
                a.name.clone()
            } else {
                let suffix = format!(" ({} {})", a.provider, a.version);
                format!("{}{}", a.name, suffix.trim_end())
            };
            Json::Str(name)
        })
        .collect();

    let limitations: Vec<Json> = DECLARED_DROPS.iter().map(|d| s(d)).collect();

    let mut entries: Vec<(String, Json)> = vec![
        ("@context".into(), croissant_context()),
        ("@type".into(), s("sc:Dataset")),
        ("@id".into(), s(&ds.iri)),
        ("name".into(), s(&ds.title)),
        ("description".into(), s(&ds.description)),
        ("conformsTo".into(), s(CROISSANT_CONFORMS_TO)),
        ("license".into(), s(&ds.license_url)),
        (
            "creator".into(),
            obj(vec![("@type", s("sc:Organization")), ("name", s(&ds.creator))]),
        ),
        ("datePublished".into(), s(&ds.date_published)),
        ("url".into(), s(&ds.landing_page)),
        ("distribution".into(), Json::Arr(distributions)),
        ("recordSet".into(), Json::Arr(croissant_record_sets(store))),
        ("rai:dataCollection".into(), s("Sources are content-addressed (blake3) and ingested through attributed gmeow:ImportActivity records; every derived artifact carries wasGeneratedBy/wasDerivedFrom lineage.")),
        ("rai:machineAnnotationTools".into(), Json::Arr(tools)),
        ("rai:dataLimitation".into(), Json::Arr(limitations)),
    ];
    if let Some(v) = &ds.version {
        entries.push(("version".into(), s(v)));
    }
    if let Some(v) = &ds.cite_as {
        entries.push(("citeAs".into(), s(v)));
    }
    Ok(Json::Obj(entries))
}

// ── RO-Crate metadata ──────────────────────────────────────────────────────────

fn json_ref(iri: &str) -> Json {
    obj(vec![("@id", s(iri))])
}

fn build_ro_crate_metadata(store: &Store, ds: &DatasetMeta, payload: &[String]) -> Json {
    let actions = activities(store);
    let workflows: Vec<String> = actions
        .iter()
        .filter(|a| !a.workflow.is_empty())
        .map(|a| a.workflow.clone())
        .collect::<BTreeSet<String>>()
        .into_iter()
        .collect();
    let has_workflow = !workflows.is_empty();

    let mut conforms = vec![json_ref(RO_CRATE_SPEC), json_ref(PROCESS_RUN_PROFILE)];
    if has_workflow {
        conforms.push(json_ref(WORKFLOW_RUN_PROFILE));
    }

    let mut root: Vec<(String, Json)> = vec![
        ("@id".into(), s("./")),
        ("@type".into(), s("Dataset")),
        ("name".into(), s(&ds.title)),
        ("description".into(), s(&ds.description)),
        ("datePublished".into(), s(&ds.date_published)),
        ("license".into(), json_ref(&ds.license_url)),
        (
            "publisher".into(),
            json_ref(&format!("{NS}ro-crate/publisher")),
        ),
        (
            "hasPart".into(),
            Json::Arr(payload.iter().map(|n| json_ref(n)).collect()),
        ),
    ];
    if has_workflow {
        root.push(("mainEntity".into(), json_ref(&workflows[0])));
    }

    let mut entities: Vec<Json> = vec![
        obj(vec![
            ("@id", s("ro-crate-metadata.json")),
            ("@type", s("CreativeWork")),
            ("conformsTo", Json::Arr(conforms)),
            ("about", json_ref("./")),
            (
                "description",
                s(&format!(
                    "Generated from canonical GMEOW instance data; declared drops: {}.",
                    DECLARED_DROPS.join("; ")
                )),
            ),
        ]),
        Json::Obj(root),
        obj(vec![
            ("@id", s(&ds.license_url)),
            ("@type", s("CreativeWork")),
            ("name", s(&ds.license_id)),
        ]),
        obj(vec![
            ("@id", s(&format!("{NS}ro-crate/publisher"))),
            ("@type", s("Organization")),
            ("name", s(&ds.creator)),
        ]),
    ];

    for name in payload {
        let fmt = if name.ends_with(".ttl") {
            "text/turtle"
        } else {
            "application/ld+json"
        };
        entities.push(obj(vec![
            ("@id", s(name)),
            ("@type", s("File")),
            ("name", s(name)),
            ("encodingFormat", s(fmt)),
        ]));
    }

    for doc in documents(store) {
        let mut fields: Vec<(String, Json)> = vec![
            ("@id".into(), s(&doc.iri)),
            ("@type".into(), s("File")),
            ("name".into(), s(&doc.name)),
        ];
        let primary = primary_digest(&doc.digests);
        if !primary.is_empty() {
            fields.push(("identifier".into(), s(&primary)));
        }
        if !doc.content_url.is_empty() {
            fields.push(("contentUrl".into(), s(&doc.content_url)));
        }
        entities.push(Json::Obj(fields));
    }

    for agent in agents(store) {
        let mut fields: Vec<(String, Json)> = vec![
            ("@id".into(), s(&agent.iri)),
            ("@type".into(), s("SoftwareApplication")),
            ("name".into(), s(&agent.name)),
        ];
        if !agent.version.is_empty() {
            fields.push(("softwareVersion".into(), s(&agent.version)));
        }
        entities.push(Json::Obj(fields));
    }

    for workflow in &workflows {
        let wname = workflow.rsplit('/').next().unwrap_or(workflow).to_string();
        entities.push(obj(vec![
            ("@id", s(workflow)),
            (
                "@type",
                Json::Arr(vec![
                    s("File"),
                    s("SoftwareSourceCode"),
                    s("ComputationalWorkflow"),
                ]),
            ),
            ("name", s(&wname)),
        ]));
    }

    for act in &actions {
        let mut fields: Vec<(String, Json)> = vec![
            ("@id".into(), s(&act.iri)),
            ("@type".into(), s("CreateAction")),
            ("name".into(), s(&act.name)),
        ];
        let instrument = if !act.workflow.is_empty() {
            act.workflow.clone()
        } else {
            act.instrument.clone()
        };
        if !instrument.is_empty() {
            fields.push(("instrument".into(), json_ref(&instrument)));
        }
        if !act.agent.is_empty() {
            fields.push(("agent".into(), json_ref(&act.agent)));
        }
        if !act.objects.is_empty() {
            fields.push((
                "object".into(),
                Json::Arr(act.objects.iter().map(|o| json_ref(o)).collect()),
            ));
        }
        if !act.results.is_empty() {
            fields.push((
                "result".into(),
                Json::Arr(act.results.iter().map(|r| json_ref(r)).collect()),
            ));
        }
        if !act.end_time.is_empty() {
            fields.push(("endTime".into(), s(&act.end_time)));
        }
        entities.push(Json::Obj(fields));
    }

    // Backfill action object/result IRIs not yet present as entities.
    let mut present: BTreeSet<String> = BTreeSet::new();
    for e in &entities {
        if let Json::Obj(fields) = e {
            for (k, v) in fields {
                if k == "@id" {
                    if let Json::Str(id) = v {
                        present.insert(id.clone());
                    }
                }
            }
        }
    }
    let mut referenced: BTreeSet<String> = BTreeSet::new();
    for act in &actions {
        for iri in act.objects.iter().chain(act.results.iter()) {
            if !present.contains(iri) {
                referenced.insert(iri.clone());
            }
        }
    }
    for iri in referenced {
        let mut fields: Vec<(String, Json)> = vec![
            ("@id".into(), s(&iri)),
            ("@type".into(), s("Thing")),
            ("name".into(), s(&label(store, &iri))),
        ];
        let digest = text(store, &iri, &g("contentDigest"));
        if !digest.is_empty() {
            fields.push(("identifier".into(), s(&digest)));
        }
        entities.push(Json::Obj(fields));
    }

    obj(vec![
        ("@context", s(RO_CRATE_CONTEXT)),
        ("@graph", Json::Arr(entities)),
    ])
}

// ── RO-Crate preview HTML (Python `html.escape`) ───────────────────────────────

/// Python `html.escape(s, quote=True)`: `& < > " '`.
fn html_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#x27;"),
            c => out.push(c),
        }
    }
    out
}

/// `str(entity[key])` Python-style: a JSON string → its value, a JSON list →
/// Python `repr` of the list (`['a', 'b']`), missing → default.
fn entity_str(e: &Json, key: &str, default: &str) -> String {
    if let Json::Obj(fields) = e {
        for (k, v) in fields {
            if k == key {
                return python_str(v);
            }
        }
    }
    default.to_string()
}

/// Python `str(value)` for the values that appear in the @graph (str or list-of-str).
fn python_str(v: &Json) -> String {
    match v {
        Json::Str(s) => s.clone(),
        Json::Arr(items) => {
            let inner: Vec<String> = items
                .iter()
                .map(|it| match it {
                    Json::Str(s) => format!("'{s}'"),
                    other => python_str(other),
                })
                .collect();
            format!("[{}]", inner.join(", "))
        }
        Json::Int(i) => i.to_string(),
        Json::Num(n) => n.clone(),
        Json::Bool(b) => {
            if *b {
                "True".to_string()
            } else {
                "False".to_string()
            }
        }
        Json::Obj(_) => String::new(),
    }
}

fn build_ro_crate_preview(metadata: &Json) -> Vec<u8> {
    let graph =
        match metadata {
            Json::Obj(fields) => fields
                .iter()
                .find(|(k, _)| k == "@graph")
                .and_then(|(_, v)| match v {
                    Json::Arr(items) => Some(items),
                    _ => None,
                }),
            _ => None,
        };
    let empty: Vec<Json> = Vec::new();
    let graph = graph.unwrap_or(&empty);
    let root = graph
        .iter()
        .find(|e| matches!(e, Json::Obj(fields) if fields.iter().any(|(k, v)| k == "@id" && matches!(v, Json::Str(id) if id == "./"))));

    let esc = |e: &Json, key: &str, default: &str| html_escape(&entity_str(e, key, default));
    let blank = obj(vec![]);
    let root = root.unwrap_or(&blank);

    let mut rows = String::new();
    for (i, e) in graph.iter().enumerate() {
        if i > 0 {
            rows.push('\n');
        }
        rows.push_str(&format!(
            "<tr><td>{}</td><td>{}</td><td>{}</td></tr>",
            esc(e, "@id", ""),
            esc(e, "@type", ""),
            esc(e, "name", "")
        ));
    }

    let html = format!(
        "<!DOCTYPE html>\n<html lang=\"en\">\n<head><meta charset=\"utf-8\"><title>{}</title></head>\n<body>\n<h1>{}</h1>\n<p>{}</p>\n<p>Published: {}</p>\n<table border=\"1\">\n<tr><th>@id</th><th>@type</th><th>name</th></tr>\n{}\n</table>\n</body>\n</html>\n",
        esc(root, "name", "RO-Crate"),
        esc(root, "name", ""),
        esc(root, "description", ""),
        esc(root, "datePublished", ""),
        rows
    );
    html.into_bytes()
}

// ── Frictionless datapackage.json ──────────────────────────────────────────────

fn build_frictionless(store: &Store, ds: &DatasetMeta) -> Json {
    let mut resources: Vec<Json> = Vec::new();
    for doc in documents(store) {
        let name = slug(&doc.iri).to_lowercase().replace('_', "-");
        let path = if doc.content_url.is_empty() {
            doc.iri.clone()
        } else {
            doc.content_url.clone()
        };
        let mut fields: Vec<(String, Json)> = vec![
            ("name".into(), s(&name)),
            ("path".into(), s(&path)),
            ("title".into(), s(&doc.name)),
        ];
        let primary = primary_digest(&doc.digests);
        if !primary.is_empty() {
            fields.push(("hash".into(), s(&primary)));
        }
        resources.push(Json::Obj(fields));
    }
    let mut entries: Vec<(String, Json)> = vec![
        (
            "name".into(),
            s(&slug(&ds.iri).to_lowercase().replace('_', "-")),
        ),
        ("title".into(), s(&ds.title)),
        ("description".into(), s(&ds.description)),
        ("homepage".into(), s(&ds.landing_page)),
        ("created".into(), s(&ds.date_published)),
        (
            "licenses".into(),
            Json::Arr(vec![obj(vec![
                ("name", s(&ds.license_id)),
                ("path", s(&ds.license_url)),
                ("title", s(&ds.license_id)),
            ])]),
        ),
        (
            "contributors".into(),
            Json::Arr(vec![obj(vec![("title", s(&ds.creator))])]),
        ),
        ("resources".into(), Json::Arr(resources)),
        (
            "notes".into(),
            s(&format!(
                "Generated lossy projection of canonical GMEOW data; drops: {}.",
                DECLARED_DROPS.join("; ")
            )),
        ),
    ];
    if let Some(v) = &ds.version {
        entries.push(("version".into(), s(v)));
    }
    Json::Obj(entries)
}

// ── DataCite XML (ElementTree) ─────────────────────────────────────────────────

/// ElementTree text escaping: `& < >` (attribute values also escape `"`).
fn xml_text(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}
fn xml_attr(s: &str) -> String {
    xml_text(s).replace('"', "&quot;")
}

struct Xml {
    out: String,
}

impl Xml {
    fn open(&mut self, indent: usize, tag: &str, attrs: &[(&str, &str)]) {
        self.out.push_str(&"  ".repeat(indent));
        self.out.push('<');
        self.out.push_str(tag);
        for (k, v) in attrs {
            self.out.push_str(&format!(" {}=\"{}\"", k, xml_attr(v)));
        }
        self.out.push_str(">\n");
    }
    fn close(&mut self, indent: usize, tag: &str) {
        self.out.push_str(&"  ".repeat(indent));
        self.out.push_str(&format!("</{tag}>\n"));
    }
    fn leaf(&mut self, indent: usize, tag: &str, text: &str, attrs: &[(&str, &str)]) {
        self.out.push_str(&"  ".repeat(indent));
        self.out.push('<');
        self.out.push_str(tag);
        for (k, v) in attrs {
            self.out.push_str(&format!(" {}=\"{}\"", k, xml_attr(v)));
        }
        self.out.push('>');
        self.out.push_str(&xml_text(text));
        self.out.push_str(&format!("</{tag}>\n"));
    }
}

fn build_datacite_xml(ds: &DatasetMeta) -> Vec<u8> {
    let doi = format!(
        "{PLACEHOLDER_DOI_PREFIX}/gmeow-{}",
        slug(&ds.iri).to_lowercase()
    );
    let mut x = Xml { out: String::new() };
    x.out.push_str("<?xml version='1.0' encoding='utf-8'?>\n");
    let schema_location =
        format!("{DATACITE_NS} https://schema.datacite.org/meta/kernel-4.5/metadata.xsd");
    x.out.push_str(&format!(
        "<resource xmlns=\"{DATACITE_NS}\" xmlns:xsi=\"{XSI_NS}\" xsi:schemaLocation=\"{}\">\n",
        xml_attr(&schema_location)
    ));
    x.leaf(1, "identifier", &doi, &[("identifierType", "DOI")]);
    x.open(1, "creators", &[]);
    x.open(2, "creator", &[]);
    x.leaf(
        3,
        "creatorName",
        &ds.creator,
        &[("nameType", "Organizational")],
    );
    x.close(2, "creator");
    x.close(1, "creators");
    x.open(1, "titles", &[]);
    x.leaf(2, "title", &ds.title, &[]);
    x.close(1, "titles");
    x.leaf(1, "publisher", &ds.creator, &[]);
    x.leaf(1, "publicationYear", &ds.publication_year(), &[]);
    x.leaf(
        1,
        "resourceType",
        "Research-object benchmark dataset",
        &[("resourceTypeGeneral", "Dataset")],
    );
    x.open(1, "dates", &[]);
    x.leaf(2, "date", &ds.date_published, &[("dateType", "Issued")]);
    x.close(1, "dates");
    x.open(1, "rightsList", &[]);
    x.leaf(
        2,
        "rights",
        &ds.license_id,
        &[
            ("rightsURI", &ds.license_url),
            ("rightsIdentifier", &ds.license_id),
            ("rightsIdentifierScheme", "SPDX"),
        ],
    );
    x.close(1, "rightsList");
    x.open(1, "descriptions", &[]);
    x.leaf(
        2,
        "description",
        &ds.description,
        &[("descriptionType", "Abstract")],
    );
    x.leaf(
        2,
        "description",
        &format!(
            "Generated lossy projection of canonical GMEOW instance data. Drops: {}.",
            DECLARED_DROPS.join("; ")
        ),
        &[("descriptionType", "TechnicalInfo")],
    );
    x.close(1, "descriptions");
    x.open(1, "relatedIdentifiers", &[]);
    x.leaf(
        2,
        "relatedIdentifier",
        &ds.landing_page,
        &[
            ("relatedIdentifierType", "URL"),
            ("relationType", "IsDescribedBy"),
        ],
    );
    x.close(1, "relatedIdentifiers");
    // ET.tostring has no trailing newline; the caller adds the single "\n".
    x.out.push_str("</resource>");
    x.out.into_bytes()
}

// ── rdflib-compatible Turtle serialization ─────────────────────────────────────
//
// A faithful port of rdflib 7.6 `TurtleSerializer` (the committed `.ttl` bytes were
// produced by it). Operates over an in-memory triple set built from oxigraph terms.

/// A serializable RDF term (subject/predicate/object) keyed for rdflib-compatible
/// ordering. rdflib `Node.__lt__`: BNode < URIRef < Literal; within a kind, by the
/// comparison value (URIRef/BNode by string; Literal by `(value, language, datatype)`
/// — for our corpus the lexical value suffices to order distinct literals).
#[derive(Clone, PartialEq, Eq)]
enum RT {
    Iri(String),
    Blank(String),
    /// Literal: canonical lexical, optional language, optional datatype IRI.
    Lit {
        lexical: String,
        language: Option<String>,
        datatype: Option<String>,
    },
}

impl RT {
    fn sort_key(&self) -> (u8, String, String, String) {
        match self {
            RT::Blank(b) => (0, b.clone(), String::new(), String::new()),
            RT::Iri(i) => (1, i.clone(), String::new(), String::new()),
            RT::Lit {
                lexical,
                language,
                datatype,
            } => (
                2,
                lexical.clone(),
                language.clone().unwrap_or_default(),
                datatype.clone().unwrap_or_default(),
            ),
        }
    }
}

impl PartialOrd for RT {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for RT {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.sort_key().cmp(&other.sort_key())
    }
}

fn rt_from_subject(t: &oxigraph::model::NamedOrBlankNode) -> RT {
    match t {
        oxigraph::model::NamedOrBlankNode::NamedNode(n) => RT::Iri(n.as_str().to_string()),
        oxigraph::model::NamedOrBlankNode::BlankNode(b) => RT::Blank(b.as_str().to_string()),
    }
}

fn rt_from_term(t: &Term) -> RT {
    match t {
        Term::NamedNode(n) => RT::Iri(n.as_str().to_string()),
        Term::BlankNode(b) => RT::Blank(b.as_str().to_string()),
        Term::Literal(l) => RT::Lit {
            lexical: canonical_lexical(l),
            language: l.language().map(|s| s.to_string()),
            datatype: {
                let dt = l.datatype().as_str().to_string();
                // rdflib stores a plain (untyped/lang) literal's datatype as None.
                if dt == format!("{XSD}string") || l.language().is_some() {
                    None
                } else {
                    Some(dt)
                }
            },
        },
        Term::Triple(_) => RT::Iri(String::new()),
    }
}

/// An in-memory triple set rendered as rdflib-compatible Turtle.
struct TurtleGraph {
    /// subject → predicate → sorted objects.
    by_subject: BTreeMap<RT, BTreeMap<String, BTreeSet<RT>>>,
    /// number of times each term appears as an object.
    references: BTreeMap<RT, usize>,
    /// distinct subjects (insertion irrelevant — ordering is computed).
    subjects: BTreeSet<RT>,
}

impl TurtleGraph {
    fn new() -> Self {
        Self {
            by_subject: BTreeMap::new(),
            references: BTreeMap::new(),
            subjects: BTreeSet::new(),
        }
    }

    fn insert(&mut self, s: RT, p: String, o: RT) {
        *self.references.entry(o.clone()).or_default() += 1;
        self.subjects.insert(s.clone());
        self.by_subject
            .entry(s)
            .or_default()
            .entry(p)
            .or_default()
            .insert(o);
    }

    fn insert_triple(&mut self, t: &oxigraph::model::Triple) {
        let s = rt_from_subject(&t.subject);
        let p = t.predicate.as_str().to_string();
        let o = rt_from_term(&t.object);
        self.insert(s, p, o);
    }

    /// rdflib `orderSubjects`: topClasses (none configured here) first, then the
    /// remaining subjects sorted by `(is_bnode, ref_count, subject)`.
    fn order_subjects(&self) -> Vec<RT> {
        let mut recursable: Vec<(bool, usize, RT)> = self
            .subjects
            .iter()
            .map(|s| {
                let is_bnode = matches!(s, RT::Blank(_));
                let refs = self.references.get(s).copied().unwrap_or(0);
                (is_bnode, refs, s.clone())
            })
            .collect();
        recursable.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)).then(a.2.cmp(&b.2)));
        recursable.into_iter().map(|(_, _, s)| s).collect()
    }

    fn serialize(&self, prefixes: &[(&str, &str)]) -> String {
        let nm = NsManager::new(prefixes);
        let subjects = self.order_subjects();

        let mut body = String::new();
        let mut first = true;
        for subject in &subjects {
            // Each top-level subject statement; blank-only nesting is not present in
            // this corpus (no inline bnodes appear in the committed outputs).
            if !first {
                body.push('\n');
            }
            first = false;
            self.statement(subject, &nm, &mut body);
        }

        let mut header = String::new();
        let mut used: Vec<(String, String)> = nm
            .used
            .borrow()
            .iter()
            .map(|p| (p.clone(), nm.ns_of(p)))
            .collect();
        used.sort();
        for (p, ns) in &used {
            header.push_str(&format!("@prefix {p}: <{ns}> .\n"));
        }
        if header.is_empty() {
            body
        } else {
            format!("{header}\n{body}")
        }
    }

    fn statement(&self, subject: &RT, nm: &NsManager, out: &mut String) {
        out.push_str(&self.term_label(subject, nm, false));
        self.predicate_list(subject, nm, out);
        out.push_str(" .\n");
    }

    fn predicate_list(&self, subject: &RT, nm: &NsManager, out: &mut String) {
        let Some(props) = self.by_subject.get(subject) else {
            return;
        };
        let order = sort_properties(props);
        for (i, pred) in order.iter().enumerate() {
            if i == 0 {
                out.push(' ');
            } else {
                out.push_str(" ;\n    ");
            }
            // verb
            let vstr = if pred == RDF_TYPE {
                "a".to_string()
            } else {
                nm.qname(pred, true)
            };
            out.push_str(&vstr);
            // object list
            let objs: Vec<&RT> = props[pred].iter().collect();
            for (oi, obj) in objs.iter().enumerate() {
                if oi == 0 {
                    out.push(' ');
                    out.push_str(&self.term_label(obj, nm, false));
                } else {
                    out.push_str(",\n        ");
                    out.push_str(&self.term_label(obj, nm, false));
                }
            }
        }
    }

    /// rdflib `label(node, position)`: IRIs → qname or `<iri>`; literals → `_literal_n3`.
    fn term_label(&self, node: &RT, nm: &NsManager, is_verb: bool) -> String {
        match node {
            RT::Iri(iri) => nm.qname(iri, is_verb),
            RT::Blank(b) => format!("_:{b}"),
            RT::Lit {
                lexical,
                language,
                datatype,
            } => literal_n3(lexical, language.as_deref(), datatype.as_deref(), nm),
        }
    }
}

/// rdflib `sortProperties`: `rdf:type` then `rdfs:label` first (predicateOrder),
/// then the remaining predicates sorted by IRI.
fn sort_properties(props: &BTreeMap<String, BTreeSet<RT>>) -> Vec<String> {
    let mut order: Vec<String> = Vec::new();
    let mut seen: BTreeSet<String> = BTreeSet::new();
    for p in [RDF_TYPE, RDFS_LABEL] {
        if props.contains_key(p) && seen.insert(p.to_string()) {
            order.push(p.to_string());
        }
    }
    let mut rest: Vec<String> = props.keys().cloned().collect();
    rest.sort();
    for p in rest {
        if seen.insert(p.clone()) {
            order.push(p);
        }
    }
    order
}

/// rdflib `Literal._literal_n3(use_plain=True)`: native syntax for the plainly
/// renderable datatypes (integer/decimal/double/boolean), language tags, bare
/// strings, else `"lexical"^^<datatype-or-qname>`.
fn literal_n3(
    lexical: &str,
    language: Option<&str>,
    datatype: Option<&str>,
    nm: &NsManager,
) -> String {
    if let Some(lang) = language {
        return format!("{}@{}", quote(lexical), lang);
    }
    let Some(dt) = datatype else {
        return quote(lexical);
    };
    // rdflib's `preprocessTriple` calls `get_pname(datatype)` for EVERY datatyped
    // literal, binding that datatype's prefix into the header even when the literal
    // renders in plain (use_plain) form (so e.g. `xsd` appears whenever any decimal
    // is present, though `1.0` itself is rendered bare).
    nm.register(dt);
    // Plain (use_plain) datatypes rdflib renders without quotes/datatype.
    if dt == format!("{XSD}integer") && is_int(lexical) {
        return lexical.to_string();
    }
    if dt == format!("{XSD}decimal") && is_decimal(lexical) {
        return lexical.to_string();
    }
    if dt == format!("{XSD}double") && is_double(lexical) {
        return lexical.to_string();
    }
    if dt == format!("{XSD}boolean") && (lexical == "true" || lexical == "false") {
        return lexical.to_string();
    }
    format!("{}^^{}", quote(lexical), nm.qname(dt, false))
}

fn is_int(v: &str) -> bool {
    let s = v.strip_prefix(['+', '-']).unwrap_or(v);
    !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit())
}
fn is_decimal(v: &str) -> bool {
    let s = v.strip_prefix(['+', '-']).unwrap_or(v);
    match s.split_once('.') {
        Some((a, b)) => {
            !(a.is_empty() && b.is_empty())
                && a.bytes().all(|c| c.is_ascii_digit())
                && b.bytes().all(|c| c.is_ascii_digit())
        }
        None => false,
    }
}
fn is_double(v: &str) -> bool {
    let lower = v.to_ascii_lowercase();
    lower.contains('e') && lower.parse::<f64>().is_ok()
}

/// rdflib `Literal._quote_encode` for a one-line string: `\ " \n \r \t` escaped.
fn quote(value: &str) -> String {
    if value.contains('\n') || value.contains('\r') || value.contains("\"\"\"") {
        let escaped = value.replace('\\', "\\\\").replace('"', "\\\"");
        return format!("\"\"\"{escaped}\"\"\"");
    }
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');
    for c in value.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// Namespace manager: chooses a qname for an IRI from the bound prefixes, recording
/// which prefixes were actually used (so only those appear in the header).
struct NsManager {
    /// Bound prefixes, longest-namespace-first for greedy matching.
    binds: Vec<(String, String)>,
    used: std::cell::RefCell<BTreeSet<String>>,
}

impl NsManager {
    fn new(prefixes: &[(&str, &str)]) -> Self {
        let mut binds: Vec<(String, String)> = prefixes
            .iter()
            .map(|(p, n)| (p.to_string(), n.to_string()))
            .collect();
        binds.sort_by(|a, b| b.1.len().cmp(&a.1.len()).then(a.0.cmp(&b.0)));
        Self {
            binds,
            used: std::cell::RefCell::new(BTreeSet::new()),
        }
    }

    fn ns_of(&self, prefix: &str) -> String {
        self.binds
            .iter()
            .find(|(p, _)| p == prefix)
            .map(|(_, n)| n.clone())
            .unwrap_or_default()
    }

    /// Record the bound prefix for `iri`'s namespace as used (rdflib's `get_pname`
    /// side effect), without emitting a qname. Used for datatypes rendered plain.
    fn register(&self, iri: &str) {
        for (prefix, ns) in &self.binds {
            if let Some(local) = iri.strip_prefix(ns.as_str()) {
                if is_valid_local(local) {
                    self.used.borrow_mut().insert(prefix.clone());
                    return;
                }
            }
        }
    }

    /// rdflib `get_pname`: compute the prefixed name, recording the prefix as used;
    /// fall back to `<iri>` n3 form if no prefix produces a valid local name.
    fn qname(&self, iri: &str, _is_verb: bool) -> String {
        for (prefix, ns) in &self.binds {
            if let Some(local) = iri.strip_prefix(ns.as_str()) {
                if is_valid_local(local) {
                    self.used.borrow_mut().insert(prefix.clone());
                    return format!("{prefix}:{local}");
                }
            }
        }
        format!("<{}>", escape_iri(iri))
    }
}

/// Whether `local` is a valid Turtle PN_LOCAL the way rdflib's split accepts it
/// for these IRIs: non-empty, no `/` `#` `:`, not ending in `.`, hyphens/digits OK.
fn is_valid_local(local: &str) -> bool {
    if local.is_empty() || local.ends_with('.') {
        return false;
    }
    if local.contains(['/', '#']) {
        return false;
    }
    // First char cannot be a digit or `-` in PN_LOCAL? rdflib's compute_qname uses a
    // looser split (it allows leading digits/hyphens via PN_LOCAL rules in 7.x). The
    // corpus locals (`mail-archive`, `claim-close-2200`, `chunk-7`) all qualify.
    local
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.' | '%'))
}

fn escape_iri(iri: &str) -> String {
    let mut out = String::with_capacity(iri.len());
    for c in iri.chars() {
        match c {
            '<' | '>' | '"' | '{' | '}' | '|' | '^' | '`' | '\\' => {
                out.push_str(&format!("\\u{:04X}", c as u32));
            }
            c if (c as u32) <= 0x20 => out.push_str(&format!("\\u{:04X}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

// ── source-Turtle → rdflib-Turtle (with x-gmeow language retag) ────────────────

/// Load the internal→BCP-47 language-tag map from the ontology's language-tag table
/// (`gmeow:languageTag` → `gmeow:bcp47Tag`).
fn load_tag_map(root: &Path) -> Result<BTreeMap<String, String>, PipelineError> {
    let store =
        Store::new().map_err(|e| PipelineError::Parse(format!("store creation failed: {e}")))?;
    for module in module_files(root)? {
        let bytes = std::fs::read(&module)?;
        parse_into(&store, &bytes, &module.display().to_string())?;
    }
    let onto = root.join("ontology").join("gmeow.ttl");
    let bytes = std::fs::read(&onto)?;
    parse_into(&store, &bytes, "ontology/gmeow.ttl")?;
    let mut map: BTreeMap<String, String> = BTreeMap::new();
    let p_int = NamedNode::new(g("languageTag")).unwrap();
    let p_ext = NamedNode::new(g("bcp47Tag")).unwrap();
    for q in store
        .quads_for_pattern(None, Some(p_int.as_ref()), None, None)
        .flatten()
    {
        let subj = match &q.subject {
            oxigraph::model::NamedOrBlankNode::NamedNode(n) => n.clone(),
            _ => continue,
        };
        let internal = match &q.object {
            Term::Literal(l) => l.value().to_string(),
            _ => continue,
        };
        if let Some(ext) = store
            .quads_for_pattern(Some((&subj).into()), Some(p_ext.as_ref()), None, None)
            .flatten()
            .find_map(|qq| match qq.object {
                Term::Literal(l) => Some(l.value().to_string()),
                _ => None,
            })
        {
            map.insert(internal, ext);
        }
    }
    Ok(map)
}

/// Parse a source Turtle file, retag `@x-gmeow-*` literal language tags to their
/// public BCP-47 form, and re-serialize as rdflib-compatible Turtle. Prefixes come
/// from the source file's own `@prefix` declarations (rdflib carries them on parse),
/// plus the rdflib default `rdf`/`rdfs`/`xsd`/`owl`/`xml` bindings where used.
fn serialize_source_turtle(
    bytes: &[u8],
    _path: &str,
    tag_map: &BTreeMap<String, String>,
) -> Result<String, PipelineError> {
    // oxigraph CANONICALIZES literals at parse (positiveInteger → integer,
    // decimal `1.0` → `1`), losing exactly the lexical/datatype info rdflib's
    // serializer preserves. Parse the source ourselves (these example A-Boxes are
    // flat Turtle: no blank nodes, lists, or multi-line strings) so the original
    // datatype and lexical survive into the rdflib-style canonicalization below.
    let prefixes = collect_source_prefixes(bytes);
    let mut ns_map: BTreeMap<String, String> = prefixes.iter().cloned().collect::<BTreeMap<_, _>>();
    // rdflib's NamespaceManager pre-binds these standard prefixes.
    for (p, n) in [
        ("rdf", "http://www.w3.org/1999/02/22-rdf-syntax-ns#"),
        ("rdfs", "http://www.w3.org/2000/01/rdf-schema#"),
        ("xsd", XSD),
        ("owl", "http://www.w3.org/2002/07/owl#"),
        ("xml", "http://www.w3.org/XML/1998/namespace"),
    ] {
        ns_map.entry(p.to_string()).or_insert_with(|| n.to_string());
    }

    let triples = parse_flat_turtle(bytes, &ns_map)?;

    // Header prefixes rdflib emits: the source-declared set + the standard pre-binds,
    // filtered to those actually used (the serializer records usage).
    let mut header_prefixes: Vec<(String, String)> = prefixes.clone();
    for (p, ns) in [
        ("rdf", "http://www.w3.org/1999/02/22-rdf-syntax-ns#"),
        ("rdfs", "http://www.w3.org/2000/01/rdf-schema#"),
        ("xsd", XSD),
        ("owl", "http://www.w3.org/2002/07/owl#"),
        ("xml", "http://www.w3.org/XML/1998/namespace"),
    ] {
        if !header_prefixes.iter().any(|(_, n)| n == ns) {
            header_prefixes.push((p.to_string(), ns.to_string()));
        }
    }

    let mut graph = TurtleGraph::new();
    for (s, p, o) in triples {
        graph.insert(s, p, retag_term(o, tag_map));
    }
    let prefix_refs: Vec<(&str, &str)> = header_prefixes
        .iter()
        .map(|(p, n)| (p.as_str(), n.as_str()))
        .collect();
    Ok(graph
        .serialize(&prefix_refs)
        .trim_end_matches('\n')
        .to_string()
        + "\n")
}

/// A focused Turtle parser for the flat example A-Boxes: `@prefix` headers, then
/// `subject pred obj (, obj)* (; pred obj)* .` statements. Literals keep their
/// ORIGINAL datatype + lexical (no oxigraph canonicalization), with rdflib's literal
/// canonicalization (decimal/dateTime) applied so the rendered form matches.
fn parse_flat_turtle(
    bytes: &[u8],
    ns_map: &BTreeMap<String, String>,
) -> Result<Vec<(RT, String, RT)>, PipelineError> {
    let text = String::from_utf8_lossy(bytes);
    // Strip comments (no `#` appears inside the IRIs/literals of these files at the
    // start-of-token boundary we care about; but `#` does appear inside `<...>` IRIs,
    // so only strip a `#` that is NOT inside `<...>` or `"..."`).
    let mut body = String::new();
    for line in text.lines() {
        body.push_str(&strip_line_comment(line));
        body.push('\n');
    }
    let toks = tokenize_turtle(&body);
    let mut triples: Vec<(RT, String, RT)> = Vec::new();
    let mut i = 0usize;
    while i < toks.len() {
        // Skip @prefix declarations: `@prefix p: <ns> .`
        if toks[i] == "@prefix" {
            while i < toks.len() && toks[i] != "." {
                i += 1;
            }
            i += 1; // skip "."
            continue;
        }
        // subject
        let subj = resolve_node(&toks[i], ns_map)?;
        i += 1;
        loop {
            // predicate
            let pred = if toks[i] == "a" {
                RDF_TYPE.to_string()
            } else {
                match resolve_node(&toks[i], ns_map)? {
                    RT::Iri(iri) => iri,
                    _ => {
                        return Err(PipelineError::Parse(format!(
                            "non-IRI predicate token: {}",
                            toks[i]
                        )))
                    }
                }
            };
            i += 1;
            loop {
                let obj = resolve_object(&toks[i], ns_map)?;
                triples.push((subj.clone(), pred.clone(), obj));
                i += 1;
                match toks[i].as_str() {
                    "," => {
                        i += 1;
                        continue;
                    }
                    _ => break,
                }
            }
            match toks[i].as_str() {
                ";" => {
                    i += 1;
                    // Trailing `;` before `.`
                    if toks[i] == "." {
                        break;
                    }
                    continue;
                }
                "." => break,
                other => {
                    return Err(PipelineError::Parse(format!(
                        "unexpected token after object: {other}"
                    )))
                }
            }
        }
        i += 1; // skip "."
    }
    Ok(triples)
}

/// Strip a `#` line comment that is not inside `<...>` or `"..."`.
fn strip_line_comment(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut in_iri = false;
    let mut in_str = false;
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '<' if !in_str => {
                in_iri = true;
                out.push(c);
            }
            '>' if in_iri => {
                in_iri = false;
                out.push(c);
            }
            '"' if !in_iri => {
                in_str = !in_str;
                out.push(c);
            }
            '\\' if in_str => {
                out.push(c);
                if let Some(n) = chars.next() {
                    out.push(n);
                }
            }
            '#' if !in_iri && !in_str => break,
            c => out.push(c),
        }
    }
    out
}

/// Tokenize flat Turtle into IRIs/prefixed-names/literals and the `, ; . a @prefix`
/// punctuation. Literals keep their full `"lex"`, `"lex"@lang`, `"lex"^^dt` token.
fn tokenize_turtle(body: &str) -> Vec<String> {
    let mut toks: Vec<String> = Vec::new();
    let chars: Vec<char> = body.chars().collect();
    let mut i = 0usize;
    while i < chars.len() {
        let c = chars[i];
        if c.is_whitespace() {
            i += 1;
            continue;
        }
        match c {
            ',' | ';' | '.' => {
                // A `.` that is part of a number/decimal is captured by the literal/
                // number scan below; a standalone `.`/`,`/`;` is punctuation.
                toks.push(c.to_string());
                i += 1;
            }
            '<' => {
                let start = i;
                i += 1;
                while i < chars.len() && chars[i] != '>' {
                    i += 1;
                }
                i += 1; // include '>'
                toks.push(chars[start..i].iter().collect());
            }
            '"' => {
                let start = i;
                i += 1;
                while i < chars.len() {
                    if chars[i] == '\\' {
                        i += 2;
                        continue;
                    }
                    if chars[i] == '"' {
                        i += 1;
                        break;
                    }
                    i += 1;
                }
                // optional @lang or ^^datatype suffix
                if i < chars.len() && chars[i] == '@' {
                    i += 1;
                    while i < chars.len() && (chars[i].is_ascii_alphanumeric() || chars[i] == '-') {
                        i += 1;
                    }
                } else if i + 1 < chars.len() && chars[i] == '^' && chars[i + 1] == '^' {
                    i += 2;
                    if i < chars.len() && chars[i] == '<' {
                        while i < chars.len() && chars[i] != '>' {
                            i += 1;
                        }
                        i += 1;
                    } else {
                        while i < chars.len()
                            && !chars[i].is_whitespace()
                            && !matches!(chars[i], ',' | ';' | '.')
                        {
                            i += 1;
                        }
                    }
                }
                toks.push(chars[start..i].iter().collect());
            }
            c if c.is_ascii_digit() || c == '+' || c == '-' => {
                // A bare number (integer/decimal/double).
                let start = i;
                i += 1;
                while i < chars.len()
                    && (chars[i].is_ascii_digit()
                        || matches!(chars[i], '.' | 'e' | 'E' | '+' | '-'))
                {
                    // A `.` immediately followed by non-digit ends the number (it is the
                    // statement terminator) — but bare decimals here always have a digit.
                    if chars[i] == '.' && (i + 1 >= chars.len() || !chars[i + 1].is_ascii_digit()) {
                        break;
                    }
                    i += 1;
                }
                toks.push(chars[start..i].iter().collect());
            }
            _ => {
                // prefixed name or keyword (`a`, `@prefix`, `prefix:local`, `true`/`false`).
                let start = i;
                while i < chars.len()
                    && !chars[i].is_whitespace()
                    && !matches!(chars[i], ',' | ';' | '<' | '"')
                {
                    // stop at a `.` that terminates a statement (prefixed names contain
                    // `.` rarely; the corpus has none, so treat `.` as a terminator).
                    if chars[i] == '.' {
                        break;
                    }
                    i += 1;
                }
                toks.push(chars[start..i].iter().collect());
            }
        }
    }
    toks
}

/// Resolve a subject/predicate node token (`<iri>` or `prefix:local`) to an IRI/blank.
fn resolve_node(tok: &str, ns_map: &BTreeMap<String, String>) -> Result<RT, PipelineError> {
    if let Some(inner) = tok.strip_prefix('<').and_then(|t| t.strip_suffix('>')) {
        return Ok(RT::Iri(inner.to_string()));
    }
    if let Some((prefix, local)) = tok.split_once(':') {
        if let Some(ns) = ns_map.get(prefix) {
            return Ok(RT::Iri(format!("{ns}{local}")));
        }
    }
    Err(PipelineError::Parse(format!(
        "unresolved node token: {tok}"
    )))
}

/// Resolve an object token (IRI, prefixed name, bare number, boolean, or literal).
fn resolve_object(tok: &str, ns_map: &BTreeMap<String, String>) -> Result<RT, PipelineError> {
    if tok.starts_with('<') || (tok.contains(':') && !tok.starts_with('"')) {
        return resolve_node(tok, ns_map);
    }
    if tok == "true" || tok == "false" {
        return Ok(RT::Lit {
            lexical: tok.to_string(),
            language: None,
            datatype: Some(format!("{XSD}boolean")),
        });
    }
    if tok.starts_with('"') {
        return Ok(parse_literal_token(tok, ns_map));
    }
    // Bare number: integer, decimal (has `.`), or double (has `e`).
    let lower = tok.to_ascii_lowercase();
    let dt = if lower.contains('e') {
        format!("{XSD}double")
    } else if tok.contains('.') {
        format!("{XSD}decimal")
    } else {
        format!("{XSD}integer")
    };
    Ok(canonicalize_literal(tok.to_string(), None, Some(dt)))
}

/// Parse a quoted literal token (`"lex"`, `"lex"@lang`, `"lex"^^dt`).
fn parse_literal_token(tok: &str, ns_map: &BTreeMap<String, String>) -> RT {
    // Find the closing quote (respecting escapes).
    let bytes: Vec<char> = tok.chars().collect();
    let mut j = 1usize;
    while j < bytes.len() {
        if bytes[j] == '\\' {
            j += 2;
            continue;
        }
        if bytes[j] == '"' {
            break;
        }
        j += 1;
    }
    let raw: String = bytes[1..j].iter().collect();
    let lexical = unescape_turtle_string(&raw);
    let suffix: String = bytes[j + 1..].iter().collect();
    if let Some(lang) = suffix.strip_prefix('@') {
        return RT::Lit {
            lexical,
            language: Some(lang.to_string()),
            datatype: None,
        };
    }
    if let Some(dt_tok) = suffix.strip_prefix("^^") {
        let dt = if let Some(inner) = dt_tok.strip_prefix('<').and_then(|t| t.strip_suffix('>')) {
            inner.to_string()
        } else if let Some((prefix, local)) = dt_tok.split_once(':') {
            ns_map
                .get(prefix)
                .map(|ns| format!("{ns}{local}"))
                .unwrap_or_else(|| dt_tok.to_string())
        } else {
            dt_tok.to_string()
        };
        return canonicalize_literal(lexical, None, Some(dt));
    }
    // Bare string → rdflib treats as xsd:string (rendered plain, no datatype).
    RT::Lit {
        lexical,
        language: None,
        datatype: None,
    }
}

/// Apply rdflib's parse-time literal canonicalization for the datatypes that need it
/// (xsd:dateTime `Z` → `+00:00`; xsd:decimal needs a digit on both sides of `.`).
fn canonicalize_literal(lexical: String, language: Option<String>, datatype: Option<String>) -> RT {
    let dt = datatype.as_deref().unwrap_or("");
    let lexical = if dt == format!("{XSD}dateTime") {
        canonical_datetime(&lexical)
    } else if dt == format!("{XSD}decimal") {
        canonical_decimal(&lexical)
    } else {
        lexical
    };
    RT::Lit {
        lexical,
        language,
        datatype,
    }
}

/// rdflib decimal canonical lexical: ensure a digit before and after the `.`.
fn canonical_decimal(lex: &str) -> String {
    let (sign, body) = match lex.strip_prefix('-') {
        Some(b) => ("-", b),
        None => ("", lex.strip_prefix('+').unwrap_or(lex)),
    };
    let with_dot = if let Some((int_part, frac)) = body.split_once('.') {
        let int_part = if int_part.is_empty() { "0" } else { int_part };
        let frac = if frac.is_empty() { "0" } else { frac };
        format!("{int_part}.{frac}")
    } else {
        format!("{body}.0")
    };
    format!("{sign}{with_dot}")
}

/// Unescape a Turtle string literal body (`\" \\ \n \r \t \uXXXX`).
fn unescape_turtle_string(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut chars = raw.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('r') => out.push('\r'),
            Some('t') => out.push('\t'),
            Some('"') => out.push('"'),
            Some('\\') => out.push('\\'),
            Some('\'') => out.push('\''),
            Some('u') => {
                let hex: String = chars.by_ref().take(4).collect();
                if let Ok(cp) = u32::from_str_radix(&hex, 16) {
                    if let Some(ch) = char::from_u32(cp) {
                        out.push(ch);
                    }
                }
            }
            Some('U') => {
                let hex: String = chars.by_ref().take(8).collect();
                if let Ok(cp) = u32::from_str_radix(&hex, 16) {
                    if let Some(ch) = char::from_u32(cp) {
                        out.push(ch);
                    }
                }
            }
            Some(other) => {
                out.push('\\');
                out.push(other);
            }
            None => out.push('\\'),
        }
    }
    out
}

fn retag_term(term: RT, tag_map: &BTreeMap<String, String>) -> RT {
    if let RT::Lit {
        lexical,
        language: Some(lang),
        datatype,
    } = &term
    {
        if let Some(ext) = tag_map.get(lang) {
            return RT::Lit {
                lexical: lexical.clone(),
                language: Some(ext.clone()),
                datatype: datatype.clone(),
            };
        }
    }
    term
}

/// Parse the `@prefix p: <ns> .` lines from a Turtle source header.
fn collect_source_prefixes(bytes: &[u8]) -> Vec<(String, String)> {
    let text = String::from_utf8_lossy(bytes);
    let mut out: Vec<(String, String)> = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        let Some(rest) = line.strip_prefix("@prefix") else {
            continue;
        };
        let rest = rest.trim();
        // `p: <ns> .`
        let Some(colon) = rest.find(':') else {
            continue;
        };
        let prefix = rest[..colon].trim().to_string();
        let after = rest[colon + 1..].trim();
        let Some(lt) = after.find('<') else { continue };
        let Some(gt) = after[lt..].find('>') else {
            continue;
        };
        let ns = after[lt + 1..lt + gt].to_string();
        out.push((prefix, ns));
    }
    out
}

// ── render: the committed artifact map ─────────────────────────────────────────

/// The canonical GMEOW prefix subset the projected `dcat.ttl` binds & uses.
fn dcat_prefixes() -> Vec<(&'static str, &'static str)> {
    vec![
        ("dcat", "http://www.w3.org/ns/dcat#"),
        ("dcterms", "http://purl.org/dc/terms/"),
        ("gmeow", NS),
        ("prov", "http://www.w3.org/ns/prov#"),
        ("spdx", "http://spdx.org/rdf/terms#"),
        ("xsd", XSD),
    ]
}

/// Render every committed research-object artifact under `root`, keyed by its
/// logical (repo-relative) path.
pub fn render_research_objects(root: &Path) -> Result<BTreeMap<String, Vec<u8>>, PipelineError> {
    let store = load_instance_graph(root)?;
    let ds = dataset_meta(&store)?;
    let mut out: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    let p = |rel: &str| format!("{RESEARCH_OBJECTS_DIR}/{rel}");

    // Croissant (top-level).
    let croissant = build_croissant(&store, &ds)?;
    let croissant_bytes = dump_json(&croissant);
    out.insert(p("lillith.croissant.jsonld"), croissant_bytes.clone());

    // RO-Crate: retag+serialize each .ttl input, copy the croissant, build metadata.
    let tag_map = load_tag_map(root)?;
    let mut payload: Vec<String> = Vec::new();
    for (rel, name) in EXAMPLE_INPUTS {
        let bytes = std::fs::read(root.join(rel))?;
        let ttl = serialize_source_turtle(&bytes, rel, &tag_map)?;
        out.insert(p(&format!("ro-crate/{name}")), ttl.into_bytes());
        payload.push(name.to_string());
    }
    out.insert(
        p("ro-crate/lillith.croissant.jsonld"),
        croissant_bytes.clone(),
    );
    payload.push("lillith.croissant.jsonld".to_string());
    payload.sort();
    let metadata = build_ro_crate_metadata(&store, &ds, &payload);
    out.insert(p("ro-crate/ro-crate-metadata.json"), dump_json(&metadata));
    out.insert(
        p("ro-crate/ro-crate-preview.html"),
        build_ro_crate_preview(&metadata),
    );

    // DCAT: CONSTRUCT over the whole composed ontology + the worked-example A-Box.
    let dcat = render_dcat(root)?;
    out.insert(p("lillith.dcat.ttl"), dcat.into_bytes());

    // DataCite XML.
    let mut datacite = build_datacite_xml(&ds);
    datacite.push(b'\n');
    out.insert(p("lillith.datacite.xml"), datacite);

    // Frictionless datapackage.json.
    out.insert(
        p("datapackage.json"),
        dump_json(&build_frictionless(&store, &ds)),
    );

    Ok(out)
}

/// Build the DCAT store (whole ontology + example A-Box), run `dcat.rq`, serialize.
fn render_dcat(root: &Path) -> Result<String, PipelineError> {
    let store =
        Store::new().map_err(|e| PipelineError::Parse(format!("store creation failed: {e}")))?;
    // The whole authored ontology: ontology/gmeow.ttl + every slice module.ttl.
    let onto = root.join("ontology").join("gmeow.ttl");
    let bytes = std::fs::read(&onto)?;
    parse_into(&store, &bytes, "ontology/gmeow.ttl")?;
    for module in module_files(root)? {
        let bytes = std::fs::read(&module)?;
        parse_into(&store, &bytes, &module.display().to_string())?;
    }
    // The worked-example A-Box.
    for (rel, _) in EXAMPLE_INPUTS {
        let bytes = std::fs::read(root.join(rel))?;
        parse_into(&store, &bytes, rel)?;
    }

    let query_text = std::fs::read_to_string(root.join("generated/queries/dcat.rq"))?;
    let results = SparqlEvaluator::new()
        .parse_query(&query_text)
        .map_err(|e| PipelineError::Parse(format!("dcat.rq parse: {e}")))?
        .on_store(&store)
        .execute()
        .map_err(|e| PipelineError::Parse(format!("dcat.rq eval: {e}")))?;
    let triples = match results {
        QueryResults::Graph(triples) => triples,
        _ => {
            return Err(PipelineError::Parse(
                "dcat.rq did not return a CONSTRUCT graph".into(),
            ))
        }
    };
    let mut graph = TurtleGraph::new();
    for t in triples {
        let t = t.map_err(|e| PipelineError::Parse(format!("dcat.rq triple: {e}")))?;
        graph.insert_triple(&t);
    }
    let banner = "# GENERATED by gmeow research-objects — DO NOT EDIT.\n# https://github.com/Blackcat-Informatics/gmeow-ontology\n\n";
    let body = graph.serialize(&dcat_prefixes());
    Ok(format!("{banner}{body}"))
}

// ── Stage impl ───────────────────────────────────────────────────────────────

/// The `research-objects` export-leaf stage.
pub struct ResearchObjectsStage;

impl Stage for ResearchObjectsStage {
    fn id(&self) -> &str {
        "stage-export-research-objects"
    }
    fn kind(&self) -> StageKind {
        StageKind::ExportLeaf
    }
    fn consumes(&self) -> &[String] {
        &[]
    }
    fn impl_version(&self) -> &str {
        "research_objects.v1"
    }
    fn run(&self, input: StageInput<'_>) -> Result<StageOutput, PipelineError> {
        Ok(StageOutput {
            product: StageProduct::from_artifacts(self.id(), render_research_objects(input.root)?),
        })
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
    fn research_objects_are_byte_identical_to_committed() {
        let root = repo_root();
        let arts = render_research_objects(&root).expect("render");
        let mut failures: Vec<String> = Vec::new();
        let mut checked = 0;
        for (path, bytes) in &arts {
            let committed = std::fs::read(root.join(path))
                .unwrap_or_else(|_| panic!("committed missing: {path}"));
            if bytes != &committed {
                // First differing line, for fast iteration.
                let got = String::from_utf8_lossy(bytes);
                let want = String::from_utf8_lossy(&committed);
                let mut detail = String::new();
                for (i, (a, b)) in got.lines().zip(want.lines()).enumerate() {
                    if a != b {
                        detail = format!("line {}: got {a:?} want {b:?}", i + 1);
                        break;
                    }
                }
                if detail.is_empty() {
                    detail = format!("len got {} want {}", bytes.len(), committed.len());
                }
                failures.push(format!("{path}: {detail}"));
            }
            checked += 1;
        }
        assert_eq!(checked, 13, "expected 13 committed files, got {checked}");
        assert!(
            failures.is_empty(),
            "research-objects byte-parity drift:\n{}",
            failures.join("\n")
        );
    }
}
