// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The `research-objects` export leaf (P4): Croissant / RO-Crate / DataCite
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
//! recursive Turtle serializer (`turtle` below): subjects ordered by
//! `(is_bnode, ref_count, iri)`, predicates `a`/`rdfs:label`-first then sorted,
//! objects sorted, literals canonicalized exactly as rdflib's `_literal_n3`
//! (notably xsd:dateTime `Z` → `+00:00`). The `dcat.ttl` additionally runs the
//! generated `dcat.rq` CONSTRUCT over the WHOLE composed ontology (every slice
//! source) plus the worked-example A-Box, so it is fold-derived and drifts with the
//! ontology — regenerated through the committed bytes here.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::sync::Arc;

use purrdf::{RdfDataset, RdfLiteral, RdfTerm, SparqlResult};

use crate::node::{Stage, StageInput, StageOutput, StageProduct};
use crate::stages::native_query;
use crate::stages::source_load::module_files;

/// The native instance graph: a frozen dataset paired with its flat default-graph quad
/// stream (collected once for the many linear-scan reads the projection performs).
struct Store {
    quads: Vec<purrdf::RdfQuad>,
}

impl Store {
    fn from_dataset(dataset: &RdfDataset) -> Self {
        // The research-object inputs are Turtle (default graph only); keep the default-
        // graph quads in source-faithful form (statement layer re-materialized so a
        // `gmeow:contentDigest` etc. is visible exactly as authored).
        let quads = purrdf::native_quads::flat_rdf_quads_from_dataset(dataset)
            .into_iter()
            .filter(|q| q.graph_name.is_none())
            .collect();
        Self { quads }
    }

    /// Iterate `(subject, predicate, object)` of every default-graph quad.
    fn triples(&self) -> impl Iterator<Item = &purrdf::RdfQuad> {
        self.quads.iter()
    }
}

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

/// The worked example's AUTHORED instance Turtle inputs, in generator order.
/// `(repo-relative path, crate file name)`. These are pure authored-source reads
/// (`slices/…`, `evals/…`); the sixth worked-example input — `scores.ttl` — is NOT
/// authored: it is the `stage-export-evals` product (see [`SCORES_INPUT_LABEL`]),
/// threaded in from the consumed evals product rather than read off the git-ignored
/// `generated/` tree (the stale-disk-fold class).
const AUTHORED_EXAMPLE_INPUTS: [(&str, &str); 5] = [
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
];

/// The logical label of the sixth worked-example input, `generated/evals/scores.ttl`.
/// It is the `stage-export-evals` product ([`crate::stages::evals::SCORES_PATH`]); the
/// research-objects stage sources its bytes from that consumed product, never a disk read
/// of the git-ignored file. Kept identical to the producer's path so the parsed A-Box is
/// byte-identical regardless of whether the bytes came from disk or the carrier.
const SCORES_INPUT_LABEL: &str = crate::stages::evals::SCORES_PATH;
/// The crate file name of the scores input (its RO-Crate member basename).
const SCORES_INPUT_NAME: &str = "scores.ttl";

/// One worked-example A-Box input in generator order: `(logical-label, crate-name, bytes)`.
type ExampleInput = (&'static str, &'static str, Vec<u8>);

/// The six worked-example A-Box inputs in generator order: the five authored Turtle files
/// read off disk plus `scores.ttl`, whose bytes are threaded in via `scores_ttl` (the
/// consumed `stage-export-evals` product) — never re-read off the git-ignored `generated/`
/// tree. `scores.ttl` stays LAST, preserving the union order the artifacts were generated under.
fn example_inputs(root: &Path, scores_ttl: &[u8]) -> Result<Vec<ExampleInput>, gmeow_errors::Diag> {
    let mut out: Vec<ExampleInput> = Vec::with_capacity(AUTHORED_EXAMPLE_INPUTS.len() + 1);
    for (rel, name) in AUTHORED_EXAMPLE_INPUTS {
        out.push((rel, name, std::fs::read(root.join(rel))?));
    }
    out.push((SCORES_INPUT_LABEL, SCORES_INPUT_NAME, scores_ttl.to_vec()));
    Ok(out)
}

fn g(local: &str) -> String {
    format!("{NS}{local}")
}

// ── helpers: load instance graph ──────────────────────────────────────────────

/// Parse `bytes` into a frozen native dataset (the canonical native codec).
fn parse_into(bytes: &[u8], path: &str) -> Result<Arc<RdfDataset>, gmeow_errors::Diag> {
    native_query::dataset_from_turtle(bytes, path)
}

/// Parse the six worked-example Turtle files into one native A-Box `Store` (each parsed
/// through the native codec then unioned, blanks standardized apart per source). The five
/// authored inputs are read off disk; `scores.ttl` rides in via `scores_ttl` (the consumed
/// `stage-export-evals` product), never a disk read of the git-ignored generated tree.
fn load_instance_graph(root: &Path, scores_ttl: &[u8]) -> Result<Store, gmeow_errors::Diag> {
    let inputs = example_inputs(root, scores_ttl)?;
    let mut parsed: Vec<Arc<RdfDataset>> = Vec::with_capacity(inputs.len());
    for (label, _name, bytes) in &inputs {
        parsed.push(parse_into(bytes, label)?);
    }
    let refs: Vec<&RdfDataset> = parsed.iter().map(AsRef::as_ref).collect();
    Ok(Store::from_dataset(&RdfDataset::union(&refs)))
}

// ── instance-graph reads (mirror the Python `_text`/`_label` helpers) ──────────

/// First object literal lexical value (rdflib `g.value` picks an arbitrary one;
/// these subjects carry at most one of each text predicate).
fn text(store: &Store, subject: &str, predicate: &str) -> String {
    let mut best: Option<String> = None;
    for q in store.triples() {
        if !iri_is(&q.subject, subject) || q.predicate != predicate {
            continue;
        }
        let v = match &q.object {
            RdfTerm::Literal(l) => canonical_lexical(l),
            RdfTerm::Iri(n) => n.clone(),
            RdfTerm::BlankNode(b) => b.clone(),
            RdfTerm::Triple(_) => String::new(),
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
    let mut hits: Vec<String> = store
        .triples()
        .filter(|q| iri_is(&q.subject, subject) && q.predicate == predicate)
        .filter_map(|q| match &q.object {
            RdfTerm::Iri(n) => Some(n.clone()),
            _ => None,
        })
        .collect();
    hits.sort();
    hits.into_iter().next()
}

/// True if `term` is the IRI `iri`.
fn iri_is(term: &RdfTerm, iri: &str) -> bool {
    matches!(term, RdfTerm::Iri(n) if n == iri)
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
    let mut set: BTreeSet<String> = BTreeSet::new();
    for q in store.triples() {
        if q.predicate == RDF_TYPE
            && iri_is(&q.object, type_iri)
            && let RdfTerm::Iri(n) = &q.subject
        {
            set.insert(n.clone());
        }
    }
    set.into_iter().collect()
}

/// All object lexical/IRI values for `(subject, predicate)`, sorted unique.
fn objects(store: &Store, subject: &str, predicate: &str) -> Vec<String> {
    let mut set: BTreeSet<String> = BTreeSet::new();
    for q in store.triples() {
        if !iri_is(&q.subject, subject) || q.predicate != predicate {
            continue;
        }
        match &q.object {
            RdfTerm::Literal(l) => {
                set.insert(canonical_lexical(l));
            }
            RdfTerm::Iri(n) => {
                set.insert(n.clone());
            }
            _ => {}
        }
    }
    set.into_iter().collect()
}

/// Subjects `s` with `(s, predicate, object)`, sorted by IRI.
fn subjects_with(store: &Store, predicate: &str, object: &str) -> Vec<String> {
    let mut set: BTreeSet<String> = BTreeSet::new();
    for q in store.triples() {
        if q.predicate == predicate
            && iri_is(&q.object, object)
            && let RdfTerm::Iri(n) = &q.subject
        {
            set.insert(n.clone());
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
fn canonical_lexical(l: &RdfLiteral) -> String {
    let lex = l.lexical_form.clone();
    if l.datatype.as_deref() == Some(&format!("{XSD}dateTime")[..]) {
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

/// The JSON / JSON-LD / XML / HTML canonical form of an xsd:dateTime: a trailing
/// `+00:00` UTC offset collapses to `Z`. The CI-canonical artifacts emit `Z`;
/// a locally-regenerated fold may carry `+00:00` (Python isoformat). Normalizing
/// here makes the text outputs byte-identical to the committed artifacts whether
/// the input `gmeow.gts` carries `…+00:00` or `…Z`.
fn json_datetime(lex: &str) -> String {
    if let Some(stripped) = lex.strip_suffix("+00:00") {
        format!("{stripped}Z")
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

fn dataset_meta(store: &Store) -> Result<DatasetMeta, gmeow_errors::Diag> {
    let mut candidates: Vec<String> = subjects_of_type(store, &g("Dataset"))
        .into_iter()
        .filter(|ds| value_node(store, ds, &g("hasLicense")).is_some())
        .collect();
    candidates.sort();
    let ds = candidates.into_iter().next().ok_or_else(|| {
        gmeow_errors::Diag::of_kind(crate::error::Parse {
            message: "no licensed gmeow:Dataset node found".into(),
        })
    })?;
    let license_node = value_node(store, &ds, &g("hasLicense")).unwrap();
    let license_id = text(store, &license_node, &g("spdxLicenseId"));
    if license_id.is_empty() {
        return Err(gmeow_errors::Diag::of_kind(crate::error::Parse {
            message: format!(
                "dataset descriptor {ds} has a gmeow:License without a gmeow:spdxLicenseId"
            ),
        }));
    }
    // Canonicalize the UTC offset to `Z` for the JSON / JSON-LD / XML / HTML
    // emitters (datapackage `created`, croissant/ro-crate `datePublished`,
    // datacite `<date>`, ro-crate-preview HTML). The fold may carry the lexical
    // dateTime as either `…+00:00` (local Python isoformat) or `…Z` (the
    // CI-canonical form); collapsing `+00:00` → `Z` here makes these text
    // outputs byte-identical to the committed artifacts regardless of which
    // form the input `gmeow.gts` happens to use. The `.ttl` outputs are
    // serialized through the rdflib-faithful path (which uses the raw lexical
    // form via `canonical_lexical`) and are unaffected by this field.
    let date_published = json_datetime(&text(store, &ds, &g("datePublished")));
    let year_ok =
        date_published.len() >= 4 && date_published.chars().take(4).all(|c| c.is_ascii_digit());
    if !year_ok {
        return Err(gmeow_errors::Diag::of_kind(crate::error::Parse {
            message: format!("dataset descriptor {ds} needs a valid gmeow:datePublished"),
        }));
    }
    let creator_node = value_node(store, &ds, &g("wasAttributedTo"));
    let version = {
        let v = text(store, &ds, &g("version"));
        if v.is_empty() { None } else { Some(v) }
    };
    let cite_as = {
        let v = text(store, &ds, &g("citeAs"));
        if v.is_empty() { None } else { Some(v) }
    };
    let title = {
        let t = text(store, &ds, &g("title"));
        if t.is_empty() { label(store, &ds) } else { t }
    };
    let landing = {
        let l = text(store, &ds, &g("sourceLocation"));
        if l.is_empty() { ds.clone() } else { l }
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
                let raw = if t.is_empty() {
                    text(store, &act, &g("eventTime"))
                } else {
                    t
                };
                // Emitted as JSON `endTime`; canonicalize the UTC offset to `Z`
                // (see `json_datetime`) so the text output matches the committed
                // artifact regardless of the fold's `+00:00`/`Z` lexical form.
                json_datetime(&raw)
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
/// `.0` for integral values, else the shortest decimal of `float(lexical)`. A
/// non-numeric score lexical HARD-fails — invalid source data is never silently
/// coerced to `0.0` (no-optionality).
fn json_float(lexical: &str) -> Result<Json, gmeow_errors::Diag> {
    let f: f64 = lexical.trim().parse().map_err(|e| {
        gmeow_errors::Diag::of_kind(crate::error::Parse {
            message: format!("assessmentScoreValue {lexical:?} is not a valid float: {e}"),
        })
    })?;
    if !f.is_finite() {
        return Err(gmeow_errors::Diag::of_kind(crate::error::Parse {
            message: format!("assessmentScoreValue {lexical:?} is not finite"),
        }));
    }
    if f == f.trunc() {
        Ok(Json::Num(format!("{f:.1}")))
    } else {
        Ok(Json::Num(format!("{f}")))
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

fn croissant_record_sets(store: &Store) -> Result<Vec<Json>, gmeow_errors::Diag> {
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

    let mut score_rows: Vec<Json> = Vec::new();
    for a in subjects_of_type(store, &g("Assessment")) {
        let lexical = text(store, &a, &g("assessmentScoreValue"));
        if lexical.is_empty() {
            continue;
        }
        score_rows.push(obj(vec![
            (
                "evalScores/model",
                s(&text(store, &a, &g("assessmentTarget"))),
            ),
            (
                "evalScores/criterion",
                s(&slug(&text(store, &a, &g("assessmentCriterion")))),
            ),
            ("evalScores/score", json_float(&lexical)?),
        ]));
    }
    if !score_rows.is_empty() {
        record_sets.push(obj(vec![
            ("@type", s("cr:RecordSet")),
            ("@id", s("evalScores")),
            ("name", s("evalScores")),
            (
                "description",
                s("Vantage-indexed rubric assessments from the gmeow-evals harness."),
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
    Ok(record_sets)
}

fn build_croissant(store: &Store, ds: &DatasetMeta) -> Result<Json, gmeow_errors::Diag> {
    let mut distributions: Vec<Json> = Vec::new();
    for doc in documents(store) {
        if doc.content_url.is_empty() {
            return Err(gmeow_errors::Diag::of_kind(crate::error::Parse {
                message: format!("build_croissant: missing contentUrl for {}", doc.iri),
            }));
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
            obj(vec![
                ("@type", s("sc:Organization")),
                ("name", s(&ds.creator)),
            ]),
        ),
        ("datePublished".into(), s(&ds.date_published)),
        ("url".into(), s(&ds.landing_page)),
        ("distribution".into(), Json::Arr(distributions)),
        ("recordSet".into(), Json::Arr(croissant_record_sets(store)?)),
        (
            "rai:dataCollection".into(),
            s(
                "Sources are content-addressed (blake3) and ingested through attributed gmeow:ImportActivity records; every derived artifact carries wasGeneratedBy/wasDerivedFrom lineage.",
            ),
        ),
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
                if k == "@id"
                    && let Json::Str(id) = v
                {
                    present.insert(id.clone());
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

// ── source-Turtle → rdflib-Turtle (with x-gmeow language retag) ────────────────

/// Load the internal→BCP-47 language-tag map from the carrier varieties in the
/// module surfaces. The internal `x-gmeow-*` tag rides `lang:carrierTag` on a
/// carrier variety since the lang: graft; its public BCP-47 code is DERIVED over
/// the model (never authored per language) — the variety's `lang:varietyOf`
/// parent sign system carries the ISO 639 primary subtag as `skos:notation`
/// (script suppressed for the carriers), matching the tag the `bcp47` projection
/// folds.
fn load_tag_map(root: &Path) -> Result<BTreeMap<String, String>, gmeow_errors::Diag> {
    const P_CARRIER: &str = "https://blackcatinformatics.ca/lang/carrierTag";
    const P_VARIETY_OF: &str = "https://blackcatinformatics.ca/lang/varietyOf";
    const P_NOTATION: &str = "http://www.w3.org/2004/02/skos/core#notation";

    let mut parsed: Vec<Arc<RdfDataset>> = Vec::new();
    for module in module_files(root)? {
        let bytes = std::fs::read(&module)?;
        parsed.push(parse_into(&bytes, &module.display().to_string())?);
    }
    let onto = root.join("ontology").join("gmeow.ttl");
    let bytes = std::fs::read(&onto)?;
    parsed.push(parse_into(&bytes, "ontology/gmeow.ttl")?);
    let refs: Vec<&RdfDataset> = parsed.iter().map(AsRef::as_ref).collect();
    let store = Store::from_dataset(&RdfDataset::union(&refs));

    let mut map: BTreeMap<String, String> = BTreeMap::new();
    for q in store.triples() {
        if q.predicate != P_CARRIER {
            continue;
        }
        let RdfTerm::Iri(subj) = &q.subject else {
            continue;
        };
        let RdfTerm::Literal(internal_lit) = &q.object else {
            continue;
        };
        let internal = internal_lit.lexical_form.clone();
        // The carrier variety's parent sign system (lang:varietyOf).
        let Some(parent) = store.triples().find_map(|qq| {
            (iri_is(&qq.subject, subj) && qq.predicate == P_VARIETY_OF)
                .then(|| match &qq.object {
                    RdfTerm::Iri(p) => Some(p.clone()),
                    _ => None,
                })
                .flatten()
        }) else {
            continue;
        };
        // The parent's ISO 639 primary subtag (skos:notation) is the derived BCP-47 tag.
        if let Some(ext) = store.triples().find_map(|qq| {
            (iri_is(&qq.subject, &parent) && qq.predicate == P_NOTATION)
                .then(|| match &qq.object {
                    RdfTerm::Literal(l) => Some(l.lexical_form.clone()),
                    _ => None,
                })
                .flatten()
        }) {
            map.insert(internal, ext.trim().to_ascii_lowercase());
        }
    }
    Ok(map)
}

/// Parse a source Turtle file, retag `@x-gmeow-*` literal language tags to their
/// public BCP-47 form, and re-serialize through the canonical Turtle serializer.
///
/// This is the byte-for-byte mirror of the canonical Python path
/// (`research_objects.export_research_objects`): the source A-Box is parsed with
/// the native store (oxigraph — which canonicalizes literals exactly as the
/// published artifacts require, e.g. decimal `1.0` → `"1"^^xsd:decimal`), its
/// `@x-gmeow-*` literal language tags retag to public BCP-47, and the result is
/// rendered with an EMPTY prefix set: fully-expanded full IRIs, no `@prefix`
/// header (matching the committed RO-Crate A-Box copies).
fn serialize_source_turtle(
    bytes: &[u8],
    path: &str,
    tag_map: &BTreeMap<String, String>,
) -> Result<String, gmeow_errors::Diag> {
    let dataset = parse_into(bytes, path)?;

    // Re-emit each triple, retagging `@x-gmeow-*` literal language tags to their public
    // BCP-47 form on the way through; the flat quad stream re-materializes the RDF 1.2
    // statement layer so the source A-Box round-trips through the canonical serializer.
    let mut retagged: Vec<purrdf::RdfQuad> =
        purrdf::native_quads::flat_rdf_quads_from_dataset(&dataset)
            .into_iter()
            .map(|mut quad| {
                if let RdfTerm::Literal(lit) = &quad.object {
                    quad.object = RdfTerm::Literal(retag_native_literal(lit, tag_map));
                }
                quad
            })
            .collect();
    // Canonicalize every typed-literal lexical form to the W3C XSD canonical mapping
    // (the native codecs preserve raw lexical forms, so without this the round-trip
    // would drift) — the SAME normalization the snapshot carrier applies.
    for quad in &mut retagged {
        canonicalize_term_xsd(&mut quad.object)?;
    }
    let flat = purrdf::native_quads::flat_dataset_from_quads(&retagged).map_err(|e| {
        gmeow_errors::Diag::of_kind(crate::error::Parse {
            message: format!("{path}: re-freeze retagged quads: {e}"),
        })
    })?;
    let nt = purrdf::serialize_dataset(
        &flat,
        "application/n-triples",
        purrdf::SerializeGraph::Dataset,
    )
    .map_err(|e| {
        gmeow_errors::Diag::of_kind(crate::error::Parse {
            message: format!("{path}: serialize N-Triples: {e}"),
        })
    })?;

    // Emit EXACTLY the canonical fold (shared prefix authority, no trailing fixup):
    // the file IS `render(graph)`, the same bytes the superset gate reconstructs.
    // `nt` is already bytes from the native serializer, so pass it by reference.
    purrdf::turtle_normalize::canonical_turtle(&nt, &crate::stages::superset::rdf_prefixes())
        .map_err(|m| gmeow_errors::Diag::of_kind(crate::error::Parse { message: m }))
}

/// Retag a native literal's `@x-gmeow-*` language tag to its public BCP-47 form.
fn retag_native_literal(lit: &RdfLiteral, tag_map: &BTreeMap<String, String>) -> RdfLiteral {
    if let Some(lang) = &lit.language
        && let Some(ext) = tag_map.get(lang)
    {
        let mut out = lit.clone();
        out.language = Some(ext.clone());
        return out;
    }
    lit.clone()
}

/// Canonicalize a single owned [`RdfTerm`] in place to the W3C XSD canonical mapping
/// (the native twin of the literal value-space the transient oxigraph store used to
/// apply on parse): a typed literal with a recognized XSD datatype is rewritten to its
/// canonical lexical form, a quoted-triple term recurses, and every other term is left
/// VERBATIM. A malformed lexical for a RECOGNIZED XSD datatype HARD-fails. Mirrors the
/// snapshot carrier's `canonicalize_term_xsd` exactly so the two paths cannot drift.
fn canonicalize_term_xsd(term: &mut RdfTerm) -> Result<(), gmeow_errors::Diag> {
    match term {
        RdfTerm::Literal(literal) => {
            if literal.language.is_some() {
                return Ok(());
            }
            if let Some(datatype_iri) = literal.datatype.as_deref() {
                match purrdf::xsd::parse_by_iri(&literal.lexical_form, datatype_iri) {
                    Ok(Some(value)) => literal.lexical_form = value.canonical_lexical(),
                    Ok(None) => {}
                    Err(e) => {
                        return Err(gmeow_errors::Diag::of_kind(crate::error::Parse {
                            message: format!(
                                "malformed typed literal {:?}^^<{datatype_iri}>: {e:?}",
                                literal.lexical_form
                            ),
                        }));
                    }
                }
            }
            Ok(())
        }
        RdfTerm::Triple(triple) => {
            canonicalize_term_xsd(&mut triple.subject)?;
            canonicalize_term_xsd(&mut triple.object)?;
            Ok(())
        }
        RdfTerm::Iri(_) | RdfTerm::BlankNode(_) => Ok(()),
    }
}

// ── render: the committed artifact map ─────────────────────────────────────────

/// Render every committed research-object artifact under `root`, keyed by its
/// logical (repo-relative) path.
pub fn render_research_objects(
    root: &Path,
    dcat_rq: &str,
    scores_ttl: &[u8],
) -> Result<BTreeMap<String, Vec<u8>>, gmeow_errors::Diag> {
    let store = load_instance_graph(root, scores_ttl)?;
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
    for (label, name, bytes) in example_inputs(root, scores_ttl)? {
        let ttl = serialize_source_turtle(&bytes, label, &tag_map)?;
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
    let dcat = render_dcat(root, dcat_rq, scores_ttl)?;
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
/// `dcat_rq` is the CONSTRUCT query text, threaded in from the consumed stage-mappings
/// product (`generated/queries/dcat.rq`) — never re-read off disk (the stale-disk-fold class).
fn render_dcat(
    root: &Path,
    dcat_rq: &str,
    scores_ttl: &[u8],
) -> Result<String, gmeow_errors::Diag> {
    let mut parsed: Vec<Arc<RdfDataset>> = Vec::new();
    // The whole authored ontology: ontology/gmeow.ttl + every slice module.ttl.
    let onto = root.join("ontology").join("gmeow.ttl");
    let bytes = std::fs::read(&onto)?;
    parsed.push(parse_into(&bytes, "ontology/gmeow.ttl")?);
    for module in module_files(root)? {
        let bytes = std::fs::read(&module)?;
        parsed.push(parse_into(&bytes, &module.display().to_string())?);
    }
    // The worked-example A-Box (scores.ttl rides in from the consumed evals product).
    for (label, _name, bytes) in example_inputs(root, scores_ttl)? {
        parsed.push(parse_into(&bytes, label)?);
    }
    let refs: Vec<&RdfDataset> = parsed.iter().map(AsRef::as_ref).collect();
    let dataset = Arc::new(RdfDataset::union(&refs));

    let graph = match native_query::query(&dataset, dcat_rq)? {
        SparqlResult::Graph(graph) => graph,
        _ => {
            return Err(gmeow_errors::Diag::of_kind(crate::error::Parse {
                message: "dcat.rq did not return a CONSTRUCT graph".into(),
            }));
        }
    };
    // The CONSTRUCT result is a native dataset; canonicalize its typed-literal lexical
    // forms to the W3C XSD mapping (the native codecs preserve raw lexical forms),
    // serialize to N-Triples (NO gts round-trip), then canonicalize to Turtle.
    let mut quads = purrdf::native_quads::flat_rdf_quads_from_dataset(&graph);
    for quad in &mut quads {
        canonicalize_term_xsd(&mut quad.object)?;
    }
    let canon = purrdf::native_quads::flat_dataset_from_quads(&quads).map_err(|e| {
        gmeow_errors::Diag::of_kind(crate::error::Parse {
            message: format!("dcat.rq re-freeze: {e}"),
        })
    })?;
    let nt = purrdf::serialize_dataset(
        &canon,
        "application/n-triples",
        purrdf::SerializeGraph::Dataset,
    )
    .map_err(|e| {
        gmeow_errors::Diag::of_kind(crate::error::Parse {
            message: format!("dcat.rq serialize N-Triples: {e}"),
        })
    })?;
    // Emit EXACTLY the canonical fold (shared prefix authority, no banner): the file
    // IS `render(graph)`, the bytes the superset gate reconstructs from the bundle.
    // `nt` is already bytes from the native serializer, so pass it by reference.
    purrdf::turtle_normalize::canonical_turtle(&nt, &crate::stages::superset::rdf_prefixes())
        .map_err(|m| gmeow_errors::Diag::of_kind(crate::error::Parse { message: m }))
}

// ── Stage impl ───────────────────────────────────────────────────────────────

/// The committed path of the DCAT CONSTRUCT query — a `stage-mappings` product artifact
/// (a generated projection), consumed from that product, never re-read off disk.
const DCAT_QUERY_PATH: &str = "generated/queries/dcat.rq";

/// The `research-objects` export-leaf stage.
pub struct ResearchObjectsStage {
    consumes: Vec<String>,
}

impl ResearchObjectsStage {
    /// Construct the stage. It consumes:
    ///
    /// * `stage-export-evals` — to obtain `generated/evals/scores.ttl` (a product of the
    ///   SAME run, written to the git-ignored generated tree only by the post-pipeline
    ///   fanout) from that stage's in-memory product, and
    /// * `stage-mappings` — to obtain the generated DCAT CONSTRUCT query
    ///   (`generated/queries/dcat.rq`) from that stage's in-memory product.
    ///
    /// Both are sourced from the consumed product rather than re-reading the stale/absent
    /// committed files off disk (the stale-disk-fold class): a scores or `dcat.rq` edit then
    /// reaches the research objects in a single regenerate, and a cold clone (no materialized
    /// generated tree) still builds.
    /// Kept in sorted order to match the registry `consumes()` and the module.ttl
    /// `dataflowConsumes`.
    pub fn new() -> Self {
        Self {
            consumes: vec![
                "stage-export-evals".to_string(),
                "stage-mappings".to_string(),
            ],
        }
    }
}

impl Default for ResearchObjectsStage {
    fn default() -> Self {
        Self::new()
    }
}

impl Stage for ResearchObjectsStage {
    fn id(&self) -> &str {
        "stage-export-research-objects"
    }
    fn consumes(&self) -> &[String] {
        &self.consumes
    }
    fn impl_version(&self) -> &str {
        // v3: `generated/evals/scores.ttl` rides in from the consumed stage-export-evals
        // product (never a disk read of the git-ignored generated tree); the DCAT CONSTRUCT
        // query rides in from the consumed stage-mappings product.
        "research_objects.v3"
    }
    fn input_files(&self, root: &Path) -> Result<Vec<std::path::PathBuf>, gmeow_errors::Diag> {
        // Pure authored-source reads: the FIVE authored worked-example A-Box inputs and the
        // language-tag map (root ontology + slice modules). NONE are in the composed fold, so
        // declare them so any edit busts the cache. Two inputs are NOT declared here — they are
        // generated projections consumed from upstream products (whose digests cover their
        // edits), never read off disk: `generated/evals/scores.ttl` (stage-export-evals) and
        // `generated/queries/dcat.rq` (stage-mappings). A generated/ path in input_files would
        // itself be the stale-disk-fold class.
        let mut files: Vec<std::path::PathBuf> = Vec::new();
        for (rel, _) in AUTHORED_EXAMPLE_INPUTS {
            files.push(root.join(rel));
        }
        files.push(root.join("ontology").join("gmeow.ttl"));
        files.extend(module_files(root)?);
        Ok(files)
    }
    fn run(&self, input: StageInput<'_>) -> Result<StageOutput, gmeow_errors::Diag> {
        // The generated DCAT CONSTRUCT query, sourced from THIS run's stage-mappings
        // product (fail-closed: a missing artifact is a hard error, never a disk fallback).
        let dcat_rq = input
            .upstream
            .get("stage-mappings")
            .and_then(|p| p.artifact(DCAT_QUERY_PATH))
            .ok_or_else(|| {
                gmeow_errors::Diag::of_kind(crate::error::StageFailed {
                    stage: self.id().to_owned(),
                    message: format!(
                        "missing {DCAT_QUERY_PATH} in the stage-mappings product; refusing to \
                         re-read the stale committed query off disk (fail-closed)"
                    ),
                })
            })?;
        let dcat_rq = std::str::from_utf8(dcat_rq).map_err(|e| {
            gmeow_errors::Diag::of_kind(crate::error::StageFailed {
                stage: self.id().to_owned(),
                message: format!("{DCAT_QUERY_PATH} is not utf-8: {e}"),
            })
        })?;
        // `generated/evals/scores.ttl` from THIS run's stage-export-evals product
        // (fail-closed: a missing artifact is a hard error, never a disk fallback of the
        // git-ignored generated tree).
        let scores_ttl = input
            .upstream
            .get("stage-export-evals")
            .and_then(|p| p.artifact(SCORES_INPUT_LABEL))
            .ok_or_else(|| {
                gmeow_errors::Diag::of_kind(crate::error::StageFailed {
                    stage: self.id().to_owned(),
                    message: format!(
                        "missing {SCORES_INPUT_LABEL} in the stage-export-evals product; refusing \
                         to re-read the git-ignored generated file off disk (fail-closed)"
                    ),
                })
            })?;
        Ok(StageOutput::new(StageProduct::from_artifacts(
            self.id(),
            render_research_objects(input.root, dcat_rq, scores_ttl)?,
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
    fn scores_ttl_rides_the_evals_product_not_disk() {
        // The scores bytes are threaded from the (consumed) evals product, never read off
        // the git-ignored generated/evals/scores.ttl: a sentinel passed as the scores bytes
        // appears verbatim as the LAST example input, labelled by the producer's SCORES_PATH.
        let root = repo_root();
        let sentinel = b"# sentinel scores\n".as_slice();
        let inputs = example_inputs(&root, sentinel).expect("example inputs");
        assert_eq!(inputs.len(), 6, "five authored inputs + scores.ttl");
        let (label, name, bytes) = inputs.last().expect("scores input present");
        assert_eq!(*label, crate::stages::evals::SCORES_PATH);
        assert_eq!(*label, "generated/evals/scores.ttl");
        assert_eq!(*name, "scores.ttl");
        assert_eq!(bytes.as_slice(), sentinel);
    }

    #[test]
    fn input_files_omit_generated_scores_and_the_dag_edge_binds() {
        let root = repo_root();
        let stage = ResearchObjectsStage::default();
        // The generated evals product no longer rides input_files() (the stale-disk-fold
        // class): its freshness rides the consumed stage-export-evals product digest.
        let files = stage.input_files(&root).expect("input files");
        assert!(
            files
                .iter()
                .all(|f| !f.ends_with("generated/evals/scores.ttl")),
            "generated/evals/scores.ttl must not be an input_files() disk read"
        );
        // The DAG edge binds: the stage consumes both producers, in sorted order.
        assert_eq!(
            stage.consumes(),
            &[
                "stage-export-evals".to_string(),
                "stage-mappings".to_string()
            ]
        );
    }

    #[test]
    fn research_objects_are_byte_identical_to_committed() {
        let root = repo_root();
        // The DCAT query is a stage-mappings product artifact; in production the stage
        // reads it off that product. This byte-parity test drives the pure renderer
        // directly, so it supplies the committed query text (the same bytes the mappings
        // stage would emit) — asserting the rendered bundle is byte-identical to committed.
        let dcat_rq = std::fs::read_to_string(root.join(DCAT_QUERY_PATH))
            .expect("committed generated/queries/dcat.rq");
        // scores.ttl is a stage-export-evals product; produce it FRESH from the evals
        // renderer (the same bytes the evals leaf emits and the fanout writes to disk)
        // rather than reading the git-ignored generated/evals/scores.ttl — the production
        // product-sourcing path, not a stale disk read.
        let evals = crate::stages::evals::render_evals(&root).expect("render evals");
        let scores_ttl = evals
            .get(crate::stages::evals::SCORES_PATH)
            .expect("evals product carries scores.ttl");
        let arts = render_research_objects(&root, &dcat_rq, scores_ttl).expect("render");
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
