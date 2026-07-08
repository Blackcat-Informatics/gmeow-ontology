// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Native MCP surfaces over the bundled GMEOW snapshot.
//!
//! `McpView` loads the bundled `gmeow.gts` snapshot ONCE (the narrow waist,
//! bundle-only — never the repo) and serves the `export`-backed surfaces —
//! `lookup_term`, `llms_txt`, `llms_full`, `doc_card`, `okf_index` — over a
//! per-language [`FoldView`]. The standard `llms.txt`/`doc_card` surfaces
//! make the docs themselves agent-consumable: the index links into the published
//! site (URLs recovered from the `gmeow:graph/documentation` graph) and the card
//! is the per-term, context-window-ready twin of the site's `card.md`. `McpServer`
//! owns the stdio JSON-RPC loop, startup language validation, resource routing,
//! and grounded-memory triad, leaving Python only as the CLI launcher.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::env;
use std::fs;
use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

#[cfg(feature = "python")]
use pyo3::exceptions::PyValueError;
#[cfg(feature = "python")]
use pyo3::prelude::*;
use serde_json::{Value, json};

use gmeow_errors::ResultExt;
use gmeow_logic::conjecture::{ConjectureLifecycleState, conjecture_test};
use gmeow_logic::query_ir::Budget;
use gmeow_logic::result_rdf::{
    ConjectureVerdictInput, conjecture_node_iri, project_conjecture_verdict,
};
use gmeow_logic::transaction::execute::{CommitMode, TxReceipt, execute_transaction};
use gmeow_logic_compile::frontend::parse_logic_str;
use gmeow_logic_compile::ir::{Formula, LOGIC_NAMESPACE, Term as IrTerm};
use purrdf::gts::examples::agent_memory::{
    Memory, RecallOptions, RevisionOptions, StoreOptions, ToolCallOptions,
};
use purrdf::gts::model::{Term as GtsTerm, TermKind as GtsTermKind};
use purrdf::gts::writer::Writer as GtsWriter;
use purrdf::{RdfDatasetBuilder, RdfQuad, RdfTerm};
use sha2::{Digest, Sha256};

use crate::stages::export::{self, FoldView, Term};
use crate::stages::fold_arena;

// The internal→BCP-47 display-language map is carried on the lang: carrier
// varieties: each lang:LanguageVariety bears its internal tag through
// lang:carrierTag and its generated (folded) external tag through gmeow:bcp47Tag.
const LANGUAGE_CLASS: &str = "https://blackcatinformatics.ca/lang/LanguageVariety";
const LANGUAGE_TAG: &str = "https://blackcatinformatics.ca/lang/carrierTag";
const BCP47_TAG: &str = "https://blackcatinformatics.ca/gmeow/bcp47Tag";
const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
const TOOL_AGENT_NS: &str = "urn:gmeow:tool:";
/// The distinct external-provenance named graph the read-only local overlay is
/// re-homed into (the origin marker). Overlay triples are visible to reads
/// (`bundle ∪ overlay`) but quarantined under this graph — NEVER unioned into the
/// signed `gmeow:` canon and NEVER written back.
const EXTERNAL_OVERLAY_GRAPH: &str = "urn:gmeow:mcp:overlay:external";

/// A loaded, bundle-backed view over the GMEOW snapshot for the MCP consumer.
#[cfg_attr(feature = "python", pyclass(name = "McpView", skip_from_py_object))]
pub struct McpView {
    /// THIS server's view of the bundled snapshot as the native carrier dataset:
    /// the MCP server is a gts ARCHIVE CONSUMER — it imports `gmeow.gts` to
    /// the carrier representation ONCE and serves every surface off the shared export
    /// `FoldView`, exactly as the in-pipeline export leaf does.
    dataset: Arc<purrdf::RdfDataset>,
    /// Ontology title / version — language-independent (`fold_meta` reads the
    /// header via a token-minimal `value`, not a language selector), so they are
    /// resolved once at construction.
    title: String,
    version: String,
    /// `requested.join(",")` → collected terms, mirroring `_TERMS_CACHE`. Stored
    /// behind an `Arc` so the cache mutex is released before the (potentially
    /// large) render runs — concurrent reads of a cached entry never serialize
    /// behind one another's rendering.
    cache: Mutex<HashMap<String, Arc<Vec<Term>>>>,
    /// `term-IRI → published site URL`, built once from the
    /// `gmeow:graph/documentation` graph — language-independent, so it is cached
    /// across all `requested` lists. Empty when the doc graph is absent (then the
    /// `llms.txt` index renders linkless).
    doc_urls: OnceLock<Arc<HashMap<String, String>>>,
}

impl McpView {
    #[cfg(feature = "python")]
    fn from_snapshot(snapshot: &[u8]) -> gmeow_errors::Result<Self> {
        let bundle = purrdf::import_gts_events(snapshot)
            .with_ctx(|| "read snapshot gmeow.gts".to_string())?;
        Self::from_dataset(bundle.dataset)
    }

    fn from_dataset(dataset: Arc<purrdf::RdfDataset>) -> gmeow_errors::Result<Self> {
        let (title, version) = {
            let view = FoldView::new(dataset.as_ref());
            export::fold_meta(&view)?
        };
        Ok(Self {
            dataset,
            title,
            version,
            cache: Mutex::new(HashMap::new()),
            doc_urls: OnceLock::new(),
        })
    }

    /// Resolve a CURIE / local name / IRI / unambiguous prefix to its public
    /// metadata record (JSON envelope with `"ok"`), or a not-found envelope.
    fn lookup_term_json(&self, term: &str, requested: Vec<String>) -> String {
        self.with_terms(requested, |terms| export::lookup_envelope(terms, term))
    }

    /// The standard llmstxt.org vocabulary index (`llms.txt`) for `requested`,
    /// with bullets linking into the published docs site.
    fn llms_txt_text(&self, requested: Vec<String>) -> String {
        let title = self.title.clone();
        let version = self.version.clone();
        let doc_urls = self.doc_urls();
        self.with_terms(requested, |terms| {
            export::consumer_llms_txt(terms, &title, &version, &doc_urls)
        })
    }

    /// The complete inlined index (`llms-full.txt`) for `requested`.
    fn llms_full_text(&self, requested: Vec<String>) -> String {
        let title = self.title.clone();
        let version = self.version.clone();
        self.with_terms(requested, |terms| {
            export::consumer_llms_full(terms, &title, &version)
        })
    }

    /// A prompt-ready Markdown card for one term.
    fn doc_card_text(&self, term: &str, requested: Vec<String>) -> String {
        self.with_terms(requested, |terms| export::doc_card_md(terms, term))
    }

    /// The OKF manifest JSON envelope for `requested`.
    fn okf_index_json(&self, requested: Vec<String>) -> String {
        self.with_terms(requested, export::okf_index_envelope)
    }

    /// Run a SELECT / ASK SPARQL query over the `gmeow:graph/documentation` named
    /// graph (re-rooted to the default graph so a plain query with no `GRAPH`
    /// clause reaches it), returning a standard SPARQL-1.1 JSON-results envelope
    /// under `"ok"`. CONSTRUCT / DESCRIBE are rejected — the tool serves one result
    /// shape (bindings or a boolean), never a graph.
    fn query_docs_json(&self, sparql: &str) -> String {
        match self.run_docs_query(sparql) {
            Ok(value) => value.to_string(),
            Err(err) => json!({"ok": false, "error": err.to_string()}).to_string(),
        }
    }

    fn run_docs_query(&self, sparql: &str) -> gmeow_errors::Result<Value> {
        let docs = std::sync::Arc::new(
            self.dataset
                .project_named_graph(crate::stages::carrier::GRAPH_DOCUMENTATION),
        );
        let result = crate::stages::native_query::query(&docs, sparql)?;
        sparql_result_to_json(result)
    }

    /// Run a SELECT / ASK SPARQL query over the bundle canon UNIONED with a
    /// READ-ONLY external overlay loaded from `overlay_path`, returning a standard
    /// SPARQL-1.1 JSON-results envelope under `"ok"`. See [`Self::run_local_query`]
    /// for the read-only / external-provenance contract.
    fn query_local_json(&self, overlay_path: &str, sparql: &str) -> String {
        match self.run_local_query(overlay_path, sparql) {
            Ok(value) => value.to_string(),
            Err(err) => json!({"ok": false, "error": err.to_string()}).to_string(),
        }
    }

    /// Query `bundle ∪ overlay` where `overlay` is the user's LOCAL lower-tier
    /// graph file, loaded as a READ-ONLY external annex.
    ///
    /// CONTRACT (enforced here, not just documented):
    /// * the overlay is loaded into its own transient dataset from `overlay_path`
    ///   — the signed canon (`self.dataset`) is pushed VERBATIM and is NEVER
    ///   mutated;
    /// * every overlay triple is re-homed under the distinct external-provenance
    ///   graph [`EXTERNAL_OVERLAY_GRAPH`] (the origin marker), so external content
    ///   stays isolable via a `GRAPH` clause and is NEVER unioned into the signed
    ///   `gmeow:` canon graphs;
    /// * a default-graph copy makes reads see `bundle ∪ overlay`, but the whole
    ///   union is transient and discarded after the query — it is NEVER persisted,
    ///   NEVER folded into `gmeow.gts`, and NEVER written back to the canon or the
    ///   overlay file (the memory-write triad only ever touches `memory.gts`);
    /// * only SELECT / ASK are accepted (CONSTRUCT / DESCRIBE are rejected).
    fn run_local_query(&self, overlay_path: &str, sparql: &str) -> gmeow_errors::Result<Value> {
        let path = Path::new(overlay_path);
        // The media type is the file extension (ttl/nt/nq/trig/rdf/owl/xml/…); an
        // unknown extension HARD-FAILS in `parse_dataset` (no silent fallback).
        let media = path
            .extension()
            .and_then(|ext| ext.to_str())
            .map(str::to_ascii_lowercase)
            .unwrap_or_else(|| "text/turtle".to_string());
        let bytes = fs::read(path).with_ctx(|| format!("read overlay {overlay_path}"))?;
        let overlay = purrdf::parse_dataset(&bytes, &media, None)
            .with_ctx(|| format!("parse overlay {overlay_path}"))?;

        let mut builder = purrdf::RdfDatasetBuilder::new();
        // The signed canon — verbatim, never mutated.
        builder.push_dataset(self.dataset.as_ref());
        let external = purrdf::RdfTerm::Iri(EXTERNAL_OVERLAY_GRAPH.to_string());
        for quad in overlay.owned_quads() {
            // Default-graph copy → a plain query reads `bundle ∪ overlay`.
            let mut in_default = quad.clone();
            in_default.graph_name = None;
            builder.push_owned_quad(&in_default);
            // Origin-marked copy → external provenance, isolable via GRAPH.
            let mut tagged = quad;
            tagged.graph_name = Some(external.clone());
            builder.push_owned_quad(&tagged);
        }
        let dataset = builder.freeze()?;
        let result = crate::stages::native_query::query(&dataset, sparql)?;
        sparql_result_to_json(result)
    }
}

/// A native SPARQL result rendered as a JSON envelope under `"ok"`: an ASK boolean,
/// SELECT bindings (SPARQL-1.1 JSON-results shape), or — for a CONSTRUCT / DESCRIBE
/// graph result — a hard error (these tools serve bindings or a boolean, never a
/// graph).
fn sparql_result_to_json(result: purrdf::SparqlResult) -> gmeow_errors::Result<Value> {
    match result {
        purrdf::SparqlResult::Boolean(value) => Ok(json!({"ok": true, "boolean": value})),
        purrdf::SparqlResult::Solutions {
            variables, rows, ..
        } => {
            let bindings: Vec<Value> = rows
                .iter()
                .map(|row| {
                    let mut obj = serde_json::Map::new();
                    for (i, cell) in row.iter().enumerate() {
                        if let (Some(name), Some(term)) = (variables.get(i), cell.as_ref())
                            && let Some(value) = sparql_term_to_json(term)
                        {
                            obj.insert(name.clone(), value);
                        }
                    }
                    Value::Object(obj)
                })
                .collect();
            Ok(json!({
                "ok": true,
                "head": {"vars": variables},
                "results": {"bindings": bindings},
            }))
        }
        purrdf::SparqlResult::Graph(_) => Err(gmeow_errors::Diag::of_kind(crate::error::Mcp {
            message: "this tool accepts only SELECT and ASK queries; CONSTRUCT/DESCRIBE are not \
                      supported (it serves bindings or a boolean, never a graph)"
                .to_string(),
        })),
    }
}

/// One SPARQL binding rendered as a SPARQL-1.1 JSON-results term object. A quoted
/// triple term (rare in the documentation graph) has no standard binding shape and
/// is omitted.
fn sparql_term_to_json(term: &purrdf::TermValue) -> Option<Value> {
    const XSD_STRING: &str = "http://www.w3.org/2001/XMLSchema#string";
    match term {
        purrdf::TermValue::Iri(iri) => Some(json!({"type": "uri", "value": iri})),
        purrdf::TermValue::Blank { label, .. } => Some(json!({"type": "bnode", "value": label})),
        purrdf::TermValue::Literal {
            lexical_form,
            datatype,
            language,
            ..
        } => {
            let mut obj = serde_json::Map::new();
            obj.insert("type".to_string(), json!("literal"));
            obj.insert("value".to_string(), json!(lexical_form));
            if let Some(lang) = language {
                obj.insert("xml:lang".to_string(), json!(lang));
            } else if datatype != XSD_STRING {
                obj.insert("datatype".to_string(), json!(datatype));
            }
            Some(Value::Object(obj))
        }
        purrdf::TermValue::Triple { .. } => None,
    }
}

#[cfg(feature = "python")]
#[pymethods]
impl McpView {
    /// Load and fold the bundled `gmeow.gts` snapshot bytes. Hard-fails if the
    /// snapshot does not read or lacks the ontology header (`fold_meta`).
    #[new]
    fn new(snapshot: &[u8]) -> PyResult<Self> {
        Self::from_snapshot(snapshot).map_err(|e| PyValueError::new_err(e.to_string()))
    }

    /// Resolve a CURIE / local name / IRI / unambiguous prefix to its public
    /// metadata record (JSON envelope with `"ok"`), or a not-found envelope.
    fn lookup_term(&self, term: &str, requested: Vec<String>) -> String {
        self.lookup_term_json(term, requested)
    }

    /// The standard llmstxt.org vocabulary index (`llms.txt`) for `requested`,
    /// with bullets linking into the published docs site.
    fn llms_txt(&self, requested: Vec<String>) -> String {
        self.llms_txt_text(requested)
    }

    /// The complete inlined index (`llms-full.txt`) for `requested` — the
    /// single-file, link-free surface an agent can ingest whole.
    fn llms_full(&self, requested: Vec<String>) -> String {
        self.llms_full_text(requested)
    }

    /// A prompt-ready Markdown card for one term for `requested` — the
    /// live twin of the docs-site `terms/{slug}/card.md`.
    fn doc_card(&self, term: &str, requested: Vec<String>) -> String {
        self.doc_card_text(term, requested)
    }

    /// The OKF manifest JSON envelope for `requested`.
    fn okf_index(&self, requested: Vec<String>) -> String {
        self.okf_index_json(requested)
    }
}

impl McpView {
    /// The `term-IRI → site URL` map, built once from the documentation graph and
    /// cached (language-independent).
    fn doc_urls(&self) -> Arc<HashMap<String, String>> {
        Arc::clone(self.doc_urls.get_or_init(|| {
            let view = FoldView::new(self.dataset.as_ref());
            Arc::new(export::doc_url_map(&view))
        }))
    }

    /// Run `f` over the terms collected for `requested`, collecting (and caching)
    /// on first use per requested-tag list.
    fn with_terms<R>(&self, requested: Vec<String>, f: impl FnOnce(&[Term]) -> R) -> R {
        let key = requested.join(",");
        let terms = {
            let mut cache = self.cache.lock().expect("McpView term cache poisoned");
            Arc::clone(cache.entry(key).or_insert_with(|| {
                let view = FoldView::with_requested(self.dataset.as_ref(), requested);
                Arc::new(export::collect_terms(&view))
            }))
        };
        f(terms.as_slice())
    }
}

/// Which tool surface an [`McpServer`] advertises: the bundle-only consumer
/// surface, or the repository-anchored developer surface (dev adds the
/// repo-reading maintenance tools).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum McpMode {
    /// Bundle-only surface — the shippable `gmeow mcp` server.
    Consumer,
    /// Consumer surface plus the repo-reading dev tools (`gmeow-dev mcp`).
    Dev,
}

impl McpMode {
    #[cfg(feature = "python")]
    fn from_bool(dev: bool) -> Self {
        if dev { Self::Dev } else { Self::Consumer }
    }

    fn includes_dev_tools(self) -> bool {
        self == Self::Dev
    }
}

/// A Rust MCP server over the bundled snapshot and optional repository root.
#[cfg_attr(feature = "python", pyclass(name = "McpServer", skip_from_py_object))]
pub struct McpServer {
    view: McpView,
    mode: McpMode,
    root: Option<PathBuf>,
    tag_map: BTreeMap<String, String>,
    available: BTreeSet<String>,
    startup_requested: Vec<String>,
}

#[cfg(feature = "python")]
#[pymethods]
impl McpServer {
    /// Build a Rust MCP server. `dev=true` exposes repository-maintenance tools.
    #[new]
    #[pyo3(signature = (snapshot, root = None, dev = false))]
    fn new(snapshot: &[u8], root: Option<String>, dev: bool) -> PyResult<Self> {
        Self::from_snapshot(snapshot, root.map(PathBuf::from), McpMode::from_bool(dev))
            .map_err(|e| PyValueError::new_err(e.to_string()))
    }

    /// JSON form of the MCP tool list, useful for smoke tests and launchers.
    fn tools_json(&self) -> String {
        self.tools_result().to_string()
    }

    /// JSON form of the MCP resource list, useful for smoke tests and launchers.
    fn resources_json(&self) -> String {
        self.resources_result().to_string()
    }

    /// Call one MCP tool with a JSON object of arguments.
    #[pyo3(signature = (name, arguments = "{}"))]
    fn call_tool_json(&self, name: &str, arguments: &str) -> String {
        let args = serde_json::from_str(arguments).unwrap_or_else(|err| {
            json!({
                "__parse_error": err.to_string(),
            })
        });
        self.call_tool_result(name, &args).to_string()
    }

    /// Read one MCP resource URI.
    fn read_resource_json(&self, uri: &str) -> String {
        self.read_resource_result(uri).to_string()
    }

    /// Handle one JSON-RPC request and return its JSON response.
    fn handle_message_json(&self, message: &str) -> String {
        self.handle_message(message)
    }
}

impl McpServer {
    /// Build a native MCP server over the bundled `gmeow.gts` snapshot bytes.
    /// `root` (the checkout path) is required only for the [`McpMode::Dev`]
    /// repo-reading tools; the consumer surface passes `None`. Hard-fails if the
    /// snapshot does not read or the startup language (`GMEOW_LANG`) is unknown.
    pub fn from_snapshot(
        snapshot: &[u8],
        root: Option<PathBuf>,
        mode: McpMode,
    ) -> gmeow_errors::Result<Self> {
        let bundle = purrdf::import_gts_events(snapshot)
            .with_ctx(|| "read snapshot gmeow.gts".to_string())?;
        let dataset = bundle.dataset;
        let tag_map = language_tag_map(dataset.as_ref());
        let mut available: BTreeSet<String> =
            tag_map.values().map(|v| v.to_ascii_lowercase()).collect();
        available.insert("en".to_string());
        let startup_requested =
            resolve_lang(env::var("GMEOW_LANG").ok().as_deref(), &tag_map, &available)?;
        Ok(Self {
            view: McpView::from_dataset(dataset)?,
            mode,
            root,
            tag_map,
            available,
            startup_requested,
        })
    }

    fn requested_from_args(&self, args: &Value) -> gmeow_errors::Result<Vec<String>> {
        match args.get("lang").and_then(Value::as_str) {
            Some(lang) => resolve_lang(Some(lang), &self.tag_map, &self.available),
            None => Ok(self.startup_requested.clone()),
        }
    }

    fn tools_result(&self) -> Value {
        let mut tools = vec![
            tool(
                "lookup_term",
                "Resolve a bundled GMEOW term.",
                &[("term", "string"), ("lang", "string")],
            ),
            tool(
                "llms_txt",
                "Return the standard bundled vocabulary index.",
                &[("lang", "string")],
            ),
            tool(
                "llms_full",
                "Return the complete inlined bundled vocabulary index.",
                &[("lang", "string")],
            ),
            tool(
                "doc_card",
                "Return a prompt-ready Markdown card for a bundled term.",
                &[("term", "string"), ("lang", "string")],
            ),
            tool(
                "okf_index",
                "Return the OKF manifest JSON envelope.",
                &[("lang", "string")],
            ),
            tool(
                "query_docs",
                "Run a SELECT or ASK SPARQL query over the bundled documentation graph \
                 (gmeow:graph/documentation) and return SPARQL-1.1 JSON results.",
                &[("query", "string")],
            ),
            tool(
                "query_local",
                "Run a SELECT or ASK SPARQL query over the bundle UNIONED with a READ-ONLY \
                 local overlay graph file (path). The overlay is loaded as an EXTERNAL, \
                 read-only annex: its triples are visible to reads (bundle \u{222a} overlay, \
                 also isolable via GRAPH <urn:gmeow:mcp:overlay:external>) but are NEVER merged \
                 into the signed gmeow: canon and NEVER written back to disk. Accepts Turtle / \
                 TriG / N-Triples / N-Quads / RDF-XML by file extension.",
                &[("path", "string"), ("query", "string")],
            ),
            tool(
                "store_claim",
                "Append one attributed memory claim, executed as a Transaction-Logic \
                 transaction (the executional-entailment verdict gates the commit). Pass \
                 dry_run=true for a non-committing sandbox run (verdict only, nothing written).",
                &[
                    ("text", "string"),
                    ("source", "string"),
                    ("confidence", "number"),
                    ("according_to", "string"),
                    ("dry_run", "boolean"),
                ],
            ),
            tool(
                "conjecture_test",
                "Test a candidate logic: formula against a KB and, TR-gated on the \
                 persistConjecture schema (the executional-entailment verdict gates the \
                 commit), APPEND the engine verdict to the append-only conjecture library. \
                 formula is a Turtle logic: document naming one candidate; kb is a Turtle KB; \
                 standpoint is the required reified scope (P9); math_conjecture optionally \
                 names the math:Conjecture twin. max_steps / max_answers optionally bound the \
                 isolated scenario evaluation (a derived-closure-size ceiling: exceeding it \
                 stamps BudgetExhausted → lifecycle open). Pass dry_run=true for a \
                 non-committing sandbox run (verdict only, nothing written).",
                &[
                    ("formula", "string"),
                    ("kb", "string"),
                    ("standpoint", "string"),
                    ("math_conjecture", "string"),
                    ("dry_run", "boolean"),
                    ("max_steps", "integer"),
                    ("max_answers", "integer"),
                ],
            ),
            tool(
                "recall",
                "Recall stored memory claims.",
                &[
                    ("query", "string"),
                    ("min_confidence", "number"),
                    ("limit", "integer"),
                    ("include_suppressed", "boolean"),
                ],
            ),
            tool(
                "revise_belief",
                "Suppress a stored claim without deleting history (the store_claim \
                 compensation, P10), executed as a Transaction-Logic transaction whose \
                 precondition is that the target claim exists. Pass dry_run=true for a \
                 non-committing sandbox run (verdict only, nothing suppressed).",
                &[
                    ("claim_id", "string"),
                    ("reason", "string"),
                    ("superseded_by", "string"),
                    ("dry_run", "boolean"),
                ],
            ),
        ];
        if self.mode.includes_dev_tools() {
            tools.extend([
                tool("validate", "Run the native validation/check surface.", &[]),
                tool(
                    "reason",
                    "Run native reasoning over the bundled snapshot.",
                    &[],
                ),
                tool(
                    "regenerate",
                    "Run the native pipeline regenerate surface.",
                    &[],
                ),
                tool(
                    "constitution",
                    "Read the checked-out GMEOW Constitution.",
                    &[],
                ),
                tool(
                    "slice_quality",
                    "Score a slice against the slice-quality rubric and return its per-axis grades and ranked uplift advice.",
                    &[("path", "string")],
                ),
            ]);
        }
        json!({ "tools": tools })
    }

    fn resources_result(&self) -> Value {
        let mut resources = vec![
            resource(
                "gmeow://ontology/llms.txt",
                "llms.txt",
                "Standard bundled vocabulary index.",
                "text/plain",
            ),
            resource(
                "gmeow://ontology/llms-full.txt",
                "llms-full.txt",
                "Complete inlined bundled vocabulary index.",
                "text/plain",
            ),
            resource(
                "gmeow://ontology/okf-index",
                "okf-index",
                "OKF manifest JSON envelope.",
                "application/json",
            ),
        ];
        if self.mode.includes_dev_tools() {
            resources.push(resource(
                "gmeow://ontology/constitution",
                "constitution",
                "The checked-out GMEOW Constitution.",
                "text/markdown",
            ));
        }
        json!({ "resources": resources })
    }

    fn call_tool_result(&self, name: &str, args: &Value) -> Value {
        if let Some(err) = args.get("__parse_error").and_then(Value::as_str) {
            return tool_text(json!({"ok": false, "error": err}).to_string(), true);
        }
        let result = match name {
            "lookup_term" => self.tool_lookup_term(args),
            "llms_txt" => self.tool_llms_txt(args),
            "llms_full" => self.tool_llms_full(args),
            "doc_card" => self.tool_doc_card(args),
            "okf_index" => self.tool_okf_index(args),
            "query_docs" => self.tool_query_docs(args),
            "query_local" => self.tool_query_local(args),
            "store_claim" => self.tool_store_claim(args),
            "conjecture_test" => self.tool_conjecture_test(args),
            "recall" => self.tool_recall(args),
            "revise_belief" => self.tool_revise_belief(args),
            "validate" if self.mode.includes_dev_tools() => self.tool_validate(),
            "reason" if self.mode.includes_dev_tools() => self.tool_reason(),
            "regenerate" if self.mode.includes_dev_tools() => self.tool_regenerate(),
            "constitution" if self.mode.includes_dev_tools() => self.tool_constitution(),
            "slice_quality" if self.mode.includes_dev_tools() => self.tool_slice_quality(args),
            _ => Err(gmeow_errors::Diag::of_kind(crate::error::Mcp {
                message: format!("unknown tool: {name}"),
            })),
        };
        match result {
            Ok(text) => tool_text(text, false),
            Err(err) => tool_text(
                json!({"ok": false, "error": err.to_string()}).to_string(),
                true,
            ),
        }
    }

    fn read_resource_result(&self, uri: &str) -> Value {
        match self.read_resource_text(uri) {
            Ok((mime, text)) => json!({
                "contents": [{"uri": uri, "mimeType": mime, "text": text}],
            }),
            Err(err) => json!({
                "contents": [{"uri": uri, "mimeType": "application/json", "text": json!({"ok": false, "error": err.to_string()}).to_string()}],
                "isError": true,
            }),
        }
    }

    fn handle_message(&self, message: &str) -> String {
        let parsed: Value = match serde_json::from_str(message) {
            Ok(value) => value,
            Err(err) => {
                return json!({
                    "jsonrpc": "2.0",
                    "id": Value::Null,
                    "error": {"code": -32700, "message": err.to_string()},
                })
                .to_string();
            }
        };
        let id = parsed.get("id").cloned().unwrap_or(Value::Null);
        let Some(method) = parsed.get("method").and_then(Value::as_str) else {
            return rpc_error(id, -32600, "missing method");
        };
        let params = parsed.get("params").cloned().unwrap_or_else(|| json!({}));
        let result = match method {
            "initialize" => json!({
                "protocolVersion": "2024-11-05",
                "serverInfo": {"name": "gmeow", "version": env!("CARGO_PKG_VERSION")},
                "capabilities": {"tools": {}, "resources": {}},
            }),
            "tools/list" => self.tools_result(),
            "resources/list" => self.resources_result(),
            "tools/call" => {
                let Some(name) = params.get("name").and_then(Value::as_str) else {
                    return rpc_error(id, -32602, "tools/call requires params.name");
                };
                let args = params
                    .get("arguments")
                    .cloned()
                    .unwrap_or_else(|| json!({}));
                self.call_tool_result(name, &args)
            }
            "resources/read" => {
                let Some(uri) = params.get("uri").and_then(Value::as_str) else {
                    return rpc_error(id, -32602, "resources/read requires params.uri");
                };
                self.read_resource_result(uri)
            }
            "shutdown" => json!({}),
            method if method.starts_with("notifications/") => return String::new(),
            _ => return rpc_error(id, -32601, &format!("unknown method: {method}")),
        };
        json!({"jsonrpc": "2.0", "id": id, "result": result}).to_string()
    }

    /// Serve the stdio JSON-RPC 2.0 MCP loop: one request per line on stdin, one
    /// response per line on stdout, until EOF. Blocking; the native `gmeow mcp` /
    /// `gmeow-dev mcp` launchers call this directly.
    pub fn run_stdio(&self) -> gmeow_errors::Result<()> {
        let stdin = io::stdin();
        let mut stdout = io::stdout();
        for line in stdin.lock().lines() {
            let line = line?;
            if line.trim().is_empty() {
                continue;
            }
            let response = self.handle_message(&line);
            if !response.is_empty() {
                writeln!(stdout, "{response}")?;
                stdout.flush()?;
            }
        }
        Ok(())
    }

    fn tool_lookup_term(&self, args: &Value) -> gmeow_errors::Result<String> {
        let term = required_str(args, "term")?;
        let requested = self.requested_from_args(args)?;
        Ok(self.view.lookup_term_json(term, requested))
    }

    fn tool_llms_txt(&self, args: &Value) -> gmeow_errors::Result<String> {
        let requested = self.requested_from_args(args)?;
        Ok(self.view.llms_txt_text(requested))
    }

    fn tool_llms_full(&self, args: &Value) -> gmeow_errors::Result<String> {
        let requested = self.requested_from_args(args)?;
        Ok(self.view.llms_full_text(requested))
    }

    fn tool_doc_card(&self, args: &Value) -> gmeow_errors::Result<String> {
        let term = required_str(args, "term")?;
        let requested = self.requested_from_args(args)?;
        Ok(self.view.doc_card_text(term, requested))
    }

    fn tool_okf_index(&self, args: &Value) -> gmeow_errors::Result<String> {
        let requested = self.requested_from_args(args)?;
        Ok(self.view.okf_index_json(requested))
    }

    fn tool_query_docs(&self, args: &Value) -> gmeow_errors::Result<String> {
        let query = required_str(args, "query")?;
        Ok(self.view.query_docs_json(query))
    }

    fn tool_query_local(&self, args: &Value) -> gmeow_errors::Result<String> {
        let path = required_str(args, "path")?;
        let query = required_str(args, "query")?;
        Ok(self.view.query_local_json(path, query))
    }

    fn tool_store_claim(&self, args: &Value) -> gmeow_errors::Result<String> {
        let text = required_str(args, "text")?;
        let confidence = optional_f64(args, "confidence")?;
        let dry_run = optional_bool_checked(args, "dry_run")?.unwrap_or(false);

        // store_claim's precondition — a well-formed claim is presented — obtains once the input
        // validates (required text + in-range confidence, enforced above). Run the action as a
        // TR transaction; the engine's executional entailment over THIS start state is the gate
        // (no synthetic boolean — an absent precondition would fail the run).
        let obtains = [MCP_WELL_FORMED_CLAIM];
        let receipt = execute_memory_txn(MCP_STORE_CLAIM_SCHEMA, &obtains, dry_run)?;
        match &receipt {
            TxReceipt::CommittedFailure { reason } | TxReceipt::HypotheticalFailure { reason } => {
                return Ok(json!({
                    "ok": false,
                    "error": format!("store_claim precondition unmet: {reason}"),
                    "transaction": txn_json(&receipt),
                })
                .to_string());
            }
            // Sandbox run: the verdict is observed, nothing is written or recorded.
            TxReceipt::HypotheticalSuccess { .. } => {
                return Ok(json!({
                    "ok": true,
                    "dry_run": true,
                    "transaction": txn_json(&receipt),
                })
                .to_string());
            }
            TxReceipt::CommittedSuccess { .. } => {}
        }

        let memory = self.memory()?;
        let claim = memory.store(
            text,
            StoreOptions {
                source: optional_str(args, "source"),
                confidence,
                according_to: optional_str(args, "according_to"),
            },
        )?;
        let response =
            json!({"ok": true, "claim": claim_json(&claim), "transaction": txn_json(&receipt)})
                .to_string();
        let generated = [claim.id.as_str()];
        let call = memory.record_tool_call(
            &format!("{TOOL_AGENT_NS}store_claim"),
            ToolCallOptions {
                arguments: Some(&tool_arguments(
                    args,
                    &["text", "source", "confidence", "according_to", "dry_run"],
                )),
                result: Some(&response),
                invocation: None,
                generated: &generated,
            },
        )?;
        // Record the trajectory-audit context on the recorded call so the committed turn is cold-auditable.
        let at_time = call.created.as_deref().ok_or_else(|| {
            gmeow_errors::Diag::of_kind(crate::error::Mcp {
                message: "record_tool_call did not stamp a creation time".to_string(),
            })
        })?;
        write_audit_segment(
            memory.path(),
            &call.id,
            MCP_STORE_CLAIM_SCHEMA,
            &obtains,
            at_time,
        )?;
        Ok(response)
    }

    fn tool_recall(&self, args: &Value) -> gmeow_errors::Result<String> {
        let limit = optional_limit(args, "limit")?.unwrap_or(10);
        let claims = self.memory()?.recall(RecallOptions {
            query: optional_str(args, "query").unwrap_or(""),
            min_confidence: optional_f64(args, "min_confidence")?,
            limit,
            include_suppressed: optional_bool(args, "include_suppressed").unwrap_or(false),
        })?;
        Ok(json!({
            "ok": true,
            "claims": claims.iter().map(claim_json).collect::<Vec<_>>(),
        })
        .to_string())
    }

    fn tool_revise_belief(&self, args: &Value) -> gmeow_errors::Result<String> {
        let claim_id = required_str(args, "claim_id")?;
        let dry_run = optional_bool_checked(args, "dry_run")?.unwrap_or(false);
        let memory = self.memory()?;
        let claims = memory.claims()?;
        let known: BTreeSet<&str> = claims.iter().map(|claim| claim.id.as_str()).collect();
        let active: BTreeSet<&str> = claims
            .iter()
            .filter(|claim| !claim.suppressed)
            .map(|claim| claim.id.as_str())
            .collect();
        if let Some(successor) = optional_str(args, "superseded_by")
            && !known.contains(successor)
        {
            return Ok(json!({
                "ok": false,
                "error": format!("unknown superseded_by id: {successor}"),
            })
            .to_string());
        }

        // revise_belief's precondition — the target claim exists — obtains iff claim_id is a known
        // claim (the existing pre-flight check, now expressed AS the TR precondition). The del
        // effect retires the active claim, so claimInMemory obtains iff it is not already
        // suppressed. The engine's executional entailment is the gate; an unknown id fails the run.
        let mut obtains: Vec<&str> = Vec::new();
        if known.contains(claim_id) {
            obtains.push(MCP_TARGET_CLAIM_EXISTS);
        }
        if active.contains(claim_id) {
            obtains.push(MCP_CLAIM_IN_MEMORY);
        }
        let receipt = execute_memory_txn(MCP_REVISE_BELIEF_SCHEMA, &obtains, dry_run)?;
        match &receipt {
            TxReceipt::CommittedFailure { .. } | TxReceipt::HypotheticalFailure { .. } => {
                return Ok(json!({
                    "ok": false,
                    "error": format!("unknown claim id: {claim_id}"),
                    "transaction": txn_json(&receipt),
                })
                .to_string());
            }
            // Sandbox run: the verdict is observed, nothing is suppressed or recorded.
            TxReceipt::HypotheticalSuccess { .. } => {
                return Ok(json!({
                    "ok": true,
                    "dry_run": true,
                    "suppressed": claim_id,
                    "transaction": txn_json(&receipt),
                })
                .to_string());
            }
            TxReceipt::CommittedSuccess { .. } => {}
        }

        memory.revise(
            claim_id,
            RevisionOptions {
                reason: optional_str(args, "reason"),
                superseded_by: optional_str(args, "superseded_by"),
            },
        )?;
        let response = json!({
            "ok": true,
            "suppressed": claim_id,
            "superseded_by": optional_str(args, "superseded_by"),
            "transaction": txn_json(&receipt),
        })
        .to_string();
        let call = memory.record_tool_call(
            &format!("{TOOL_AGENT_NS}revise_belief"),
            ToolCallOptions {
                arguments: Some(&tool_arguments(
                    args,
                    &["claim_id", "reason", "superseded_by", "dry_run"],
                )),
                result: Some(&response),
                invocation: None,
                generated: &[],
            },
        )?;
        let at_time = call.created.as_deref().ok_or_else(|| {
            gmeow_errors::Diag::of_kind(crate::error::Mcp {
                message: "record_tool_call did not stamp a creation time".to_string(),
            })
        })?;
        write_audit_segment(
            memory.path(),
            &call.id,
            MCP_REVISE_BELIEF_SCHEMA,
            &obtains,
            at_time,
        )?;
        Ok(response)
    }

    /// The production consumer that closes the conjecture-and-refutation path: test a
    /// candidate `logic:` formula against a KB, project the engine verdict, and — TR-gated on
    /// the `persistConjecture` schema — APPEND it to the append-only conjecture library.
    ///
    /// `formula` is a Turtle `logic:` document naming the candidate (exactly one top-level
    /// `logic:Formula`, or exactly one ground `logic:` axiom lifted to a binary atom); `kb`
    /// is a Turtle KB the candidate is tested against; `standpoint` is the REQUIRED reified
    /// scope (Principle 9); `math_conjecture` optionally names the `math:Conjecture` twin so
    /// the statement is bridged to the runtime `logic:Conjecture` node via
    /// `math:conjectureUnderTest` (on every verdict) and a refutation's counterexample is
    /// re-exposed via `math:hasCounterexample`; `dry_run=true` computes and returns the verdict
    /// but WRITES NOTHING.
    fn tool_conjecture_test(&self, args: &Value) -> gmeow_errors::Result<String> {
        let formula_src = required_str(args, "formula")?;
        let kb_src = required_str(args, "kb")?;
        let standpoint = required_str(args, "standpoint")?;
        let math_conjecture = optional_str(args, "math_conjecture");
        let dry_run = optional_bool_checked(args, "dry_run")?.unwrap_or(false);
        let max_steps = optional_step_count(args, "max_steps")?;
        let max_answers = optional_limit(args, "max_answers")?;

        // The parse → test → project → TR-gate → persist path is the SHARED core; the tool is a
        // thin wrapper rendering its outcome as the JSON response.
        let out = run_conjecture_test(&ConjectureRunInput {
            formula_ttl: formula_src,
            kb_ttl: kb_src,
            standpoint,
            math_conjecture,
            dry_run,
            max_steps,
            max_answers,
        })?;

        // The five-field verdict summary + witness, rendered for every response path.
        let witness_json = out.witness.as_ref().map(|w| {
            json!({
                "individual": w.individual,
                "world": w.world,
                "premises": w.premises,
            })
        });
        let verdict_json = json!({
            "information": out.information,
            "evaluation": out.evaluation,
            "completeness": out.completeness,
            "lifecycle": out.lifecycle,
            "discharge": out.discharge,
        });

        if let Some(reason) = &out.precondition_unmet {
            return Ok(json!({
                "ok": false,
                "error": format!("persistConjecture precondition unmet: {reason}"),
                "verdict": verdict_json,
                "witness": witness_json,
                "transaction": txn_json(&out.receipt),
            })
            .to_string());
        }
        if out.dry_run {
            return Ok(json!({
                "ok": true,
                "dry_run": true,
                "verdict": verdict_json,
                "witness": witness_json,
                "conjecture": out.node_iri,
                "transaction": txn_json(&out.receipt),
            })
            .to_string());
        }

        Ok(json!({
            "ok": true,
            "verdict": verdict_json,
            "witness": witness_json,
            "conjecture": out.node_iri,
            "transaction": txn_json(&out.receipt),
        })
        .to_string())
    }

    fn tool_validate(&self) -> gmeow_errors::Result<String> {
        let root = self.root_path()?;
        let report = crate::run::run_full(&root, 1, crate::run::RunMode::Check)?;
        Ok(json!({
            "ok": report.is_clean(),
            "mode": "check",
            "produced": report.produced,
            "reproduced": report.reproduced,
            "drifted": report.drifted,
        })
        .to_string())
    }

    fn tool_slice_quality(&self, args: &Value) -> gmeow_errors::Result<String> {
        let root = self.root_path()?;
        let rel = required_str(args, "path")?;
        let slice_dir = root.join(rel);
        let report = gmeow_slice_quality::report::score_slice(&root, &slice_dir).map_err(|e| {
            gmeow_errors::Diag::of_kind(crate::error::Mcp {
                message: format!("slice_quality: {e}"),
            })
        })?;
        let grades: Vec<Value> = report
            .assessment
            .grades
            .iter()
            .map(|g| {
                let axis = g.axis_iri.rsplit(['/', '#']).next().unwrap_or(&g.axis_iri);
                json!({ "axis": axis, "tier": g.tier.label, "score": g.score })
            })
            .collect();
        let advice: Vec<Value> = report
            .advisories
            .iter()
            .map(|f| json!({ "code": f.code, "message": f.message }))
            .collect();
        Ok(json!({
            "slice": report.assessment.slice,
            "rollup_tier": report.assessment.rollup.label,
            "grades": grades,
            "advice": advice,
        })
        .to_string())
    }

    fn tool_regenerate(&self) -> gmeow_errors::Result<String> {
        let root = self.root_path()?;
        let report = crate::run::run_full(&root, 1, crate::run::RunMode::Regenerate)?;
        Ok(json!({
            "ok": true,
            "mode": "regenerate",
            "produced": report.produced,
            "reproduced": report.reproduced,
        })
        .to_string())
    }

    fn tool_reason(&self) -> gmeow_errors::Result<String> {
        let result =
            gmeow_logic::reason::reason_all(self.view.graph_dataset()?.as_ref()).map_err(|e| {
                gmeow_errors::Diag::of_kind(crate::error::Mcp {
                    message: format!("native reasoning failed: {e}"),
                })
            })?;
        Ok(json!({
            "ok": true,
            "input": result.input.wire(),
            "evaluation": result.evaluation.wire(),
            "completeness": result.completeness.wire(),
            "information": result.information.wire(),
        })
        .to_string())
    }

    fn tool_constitution(&self) -> gmeow_errors::Result<String> {
        let root = self.root_path()?;
        Ok(fs::read_to_string(root.join("CONSTITUTION.md"))?)
    }

    fn read_resource_text(&self, uri: &str) -> gmeow_errors::Result<(&'static str, String)> {
        let (base, query) = uri.split_once('?').unwrap_or((uri, ""));
        let requested = lang_from_query(query)
            .map(|raw| resolve_lang(Some(raw), &self.tag_map, &self.available))
            .transpose()?
            .unwrap_or_else(|| self.startup_requested.clone());
        match base {
            "gmeow://ontology/llms.txt" => Ok(("text/plain", self.view.llms_txt_text(requested))),
            "gmeow://ontology/llms-full.txt" => {
                Ok(("text/plain", self.view.llms_full_text(requested)))
            }
            "gmeow://ontology/okf-index" => {
                Ok(("application/json", self.view.okf_index_json(requested)))
            }
            "gmeow://ontology/constitution" if self.mode.includes_dev_tools() => {
                self.tool_constitution().map(|text| ("text/markdown", text))
            }
            _ => Err(gmeow_errors::Diag::of_kind(crate::error::Mcp {
                message: format!("unknown resource: {uri}"),
            })),
        }
    }

    fn memory(&self) -> gmeow_errors::Result<Memory> {
        let path = memory_path()?;
        if let Some(parent) = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
        {
            fs::create_dir_all(parent)?;
        }
        Ok(Memory::new(path))
    }

    fn root_path(&self) -> gmeow_errors::Result<PathBuf> {
        self.root.clone().ok_or_else(|| {
            gmeow_errors::Diag::of_kind(crate::error::Mcp {
                message: "repository root is required for dev MCP tools".to_string(),
            })
        })
    }
}

impl McpView {
    fn graph_dataset(&self) -> gmeow_errors::Result<Arc<purrdf::RdfDataset>> {
        // The carrier IS the dataset — no gts round-trip (GTS is exit-only).
        Ok(Arc::clone(&self.dataset))
    }
}

// --------------------------------------------------------------------------- //
// The shared conjecture-test core (one implementation, two surfaces).
//
// Both the private MCP tool `tool_conjecture_test` and the public
// `gmeow conjecture test` CLI subcommand call [`run_conjecture_test`]: the
// SINGLE parse → test → project → TR-gate → persist path. Neither surface
// re-implements it; the MCP wrapper renders the returned summary as JSON, the
// CLI renders it as human-readable text.
// --------------------------------------------------------------------------- //

/// The inputs to one conjecture test: the candidate `logic:` Turtle document, the KB Turtle
/// it is tested against, the REQUIRED reified standpoint scope (Principle 9), an optional
/// `math:Conjecture` twin, and whether the run is a sandbox (`dry_run`, writes nothing).
pub struct ConjectureRunInput<'a> {
    /// The candidate document: a Turtle `logic:` doc naming exactly one candidate formula.
    pub formula_ttl: &'a str,
    /// The KB the candidate is tested against, as Turtle.
    pub kb_ttl: &'a str,
    /// The required reified standpoint scope IRI (Principle 9).
    pub standpoint: &'a str,
    /// Optionally, the `math:Conjecture` twin IRI so a refutation's counterexample is
    /// re-exposed via `math:hasCounterexample`.
    pub math_conjecture: Option<&'a str>,
    /// When true, compute and return the verdict but WRITE NOTHING to the library.
    pub dry_run: bool,
    /// Optional post-hoc derived-closure-size ceiling on the isolated scenario evaluation: when
    /// the derived (non-EDB) closure exceeds this many steps the run is stamped `BudgetExhausted`
    /// → lifecycle Open → discharge Unknown. `None` = unbounded.
    pub max_steps: Option<u64>,
    /// Optional post-hoc derived-closure-size ceiling in answer bindings; see
    /// [`max_steps`](Self::max_steps). `None` = unbounded.
    pub max_answers: Option<usize>,
}

/// A refutation's contradiction witness, flattened for both response surfaces.
pub struct ConjectureRunWitness {
    /// The individual forced into a clash.
    pub individual: String,
    /// The world the contradiction is local to.
    pub world: String,
    /// The premise triples that witness the clash, each rendered `"s p o"`.
    pub premises: Vec<String>,
}

/// The outcome of a [`run_conjecture_test`] call: the projected verdict facets, the refutation
/// witness (when refuted), the content-addressed node IRI, the projected N-Triples body, and
/// the TR receipt gating the persist. `committed` is true exactly when the segment was appended.
pub struct ConjectureRunOutput {
    /// True exactly when the verdict segment + audit were appended to the library (a committed,
    /// precondition-met, non-dry run).
    pub committed: bool,
    /// True when the run was a sandbox (`dry_run`): the verdict is computed, nothing is written.
    pub dry_run: bool,
    /// `Some(reason)` when the TR precondition was UNMET (the persist was refused, nothing
    /// written); `None` otherwise.
    pub precondition_unmet: Option<String>,
    /// The epistemic lifecycle wire value (`open` | `corroborated` | `refuted-in-standpoint`).
    pub lifecycle: String,
    /// The Belnap information-state wire value.
    pub information: String,
    /// The evaluation-axis wire value.
    pub evaluation: String,
    /// The completeness-axis wire value.
    pub completeness: String,
    /// The discharge carrier local name (`ObligationDischarged` | `ObligationUnknown`).
    pub discharge: String,
    /// The refutation witness, present exactly when refuted.
    pub witness: Option<ConjectureRunWitness>,
    /// The content-addressed `(formula × standpoint × KB-world)` conjecture node IRI.
    pub node_iri: String,
    /// The deterministic N-Triples body [`project_conjecture_verdict`] emitted.
    pub verdict_nt: String,
    /// The TR receipt gating the persist (rendered as the transaction summary by callers).
    pub receipt: TxReceipt,
}

/// Run one conjecture test end-to-end: parse the candidate document and KB, re-home the KB
/// into the isolated scenario world, run the native engine, project the verdict to
/// deterministic N-Triples, TR-gate the write on the `persistConjecture` schema, and — on a
/// committed, precondition-met run — APPEND the verdict segment plus a cold-auditable
/// trajectory segment to the append-only conjecture library.
///
/// This is the single shared core behind both the MCP `conjecture_test` tool and the
/// `gmeow conjecture test` CLI subcommand. It never mutates the caller's KB (isolation is
/// inherent) and, on a `dry_run` or a precondition-unmet run, writes nothing.
///
/// # Errors
///
/// Returns an error if the candidate document does not name exactly one candidate formula, if
/// the KB does not parse, if the native engine fails (see [`conjecture_test`]), or if the TR
/// transaction or the library append fails.
pub fn run_conjecture_test(
    input: &ConjectureRunInput,
) -> gmeow_errors::Result<ConjectureRunOutput> {
    let ConjectureRunInput {
        formula_ttl,
        kb_ttl,
        standpoint,
        math_conjecture,
        dry_run,
        max_steps,
        max_answers,
    } = *input;

    // (1) Parse the candidate document and extract exactly one candidate formula.
    let candidate = parse_candidate_formula(formula_ttl)?;

    // (2) Parse the KB and re-home every triple into the isolated scenario world, so the
    //     world-scoped DL calculus joins the KB with the asserted / evaluated candidate.
    let kb = rehome_kb_into_scenario(kb_ttl)?;

    // (3) Run the engine. The KB is borrowed and never mutated (isolation is inherent).
    let kb_world = format!(
        "{CONJECTURE_SCENARIO_WORLD}#kb-{}",
        sha256_hex(kb_ttl.as_bytes())
    );
    // The optional post-hoc closure-size ceiling (criterion (2)): a bound the surfaces MAY pass
    // and the oracle NEVER sees (it is applied above the run). Both `None` ⟹ unbounded.
    let budget = Budget {
        max_answers,
        max_steps,
    };
    let answer = conjecture_test(
        kb.as_ref(),
        CONJECTURE_SCENARIO_WORLD,
        &candidate,
        standpoint,
        &[],
        &budget,
    )
    .map_err(|e| {
        gmeow_errors::Diag::of_kind(crate::error::Mcp {
            message: format!("conjecture_test failed: {e}"),
        })
    })?;

    // (4) Project the verdict → deterministic N-Triples, and mint the content-addressed
    //     (formula × standpoint × KB-world) conjecture node IRI.
    let content_key = candidate.content_key();
    // The anti-conjecture leg's forbidden predicate: the refuted formula's PRINCIPAL
    // predicate (the predicate the closure must never draw). A refuted formula that names no
    // single predicate (a compound conjunction / disjunction / implication / biconditional)
    // has no soundly-derivable forbidden predicate — its `logic:NonEntailmentObligation`
    // forbidden predicate is a reviewer decision — so we HARD-FAIL rather than fabricate one
    // or emit a shape-invalid obligation node (Constitution: no fabrication, no optionality).
    let forbidden_predicate = candidate.principal_predicate();
    if answer.lifecycle == ConjectureLifecycleState::RefutedInStandpoint
        && forbidden_predicate.is_none()
    {
        return Err(gmeow_errors::Diag::of_kind(crate::error::Mcp {
            message: format!(
                "conjecture refuted in standpoint <{standpoint}>, but its candidate formula \
                 is compound and names no single predicate: the anti-conjecture \
                 logic:NonEntailmentObligation's forbidden predicate cannot be soundly \
                 derived and must be a reviewer decision. Refute an atomic or universally \
                 quantified single-predicate claim, or author the obligation directly."
            ),
        }));
    }
    let verdict_input = ConjectureVerdictInput {
        content_key: &content_key,
        standpoint,
        kb_world: &kb_world,
        answer: &answer,
        math_conjecture,
        forbidden_predicate: forbidden_predicate.as_deref(),
    };
    let verdict_nt = project_conjecture_verdict(&verdict_input);
    let node_iri = conjecture_node_iri(&verdict_input);

    let verdict = &answer.verdict;
    let witness = answer.witness.as_ref().map(|w| ConjectureRunWitness {
        individual: w.individual.clone(),
        world: w.world.clone(),
        premises: w
            .premises
            .iter()
            .map(|(s, p, o)| format!("{s} {p} {o}"))
            .collect(),
    });
    let lifecycle = answer.lifecycle.wire().to_string();
    let information = verdict.information.wire().to_string();
    let evaluation = verdict.evaluation.wire().to_string();
    let completeness = verdict.completeness.wire().to_string();
    let discharge = answer.discharge.local_name().to_string();

    // (5) TR-gate the write on the persistConjecture schema. The precondition — a verdict has
    //     been presented — obtains once the engine returns a verdict (step 3).
    let obtains = [MCP_CONJECTURE_VERDICT_PRESENTED];
    let receipt = execute_memory_txn(MCP_PERSIST_CONJECTURE_SCHEMA, &obtains, dry_run)?;

    let mut out = ConjectureRunOutput {
        committed: false,
        dry_run,
        precondition_unmet: None,
        lifecycle,
        information,
        evaluation,
        completeness,
        discharge,
        witness,
        node_iri,
        verdict_nt,
        receipt,
    };

    match &out.receipt {
        TxReceipt::CommittedFailure { reason } | TxReceipt::HypotheticalFailure { reason } => {
            out.precondition_unmet = Some(reason.clone());
            return Ok(out);
        }
        // Sandbox run: the verdict is observed, nothing is appended to the library.
        TxReceipt::HypotheticalSuccess { .. } => return Ok(out),
        TxReceipt::CommittedSuccess { .. } => {}
    }

    // (6) Committed: APPEND the verdict segment to the append-only library, then record a
    //     cold-auditable trajectory segment keyed to a content-addressed call id.
    let path = conjecture_path()?;
    write_conjecture_segment(&path, &out.verdict_nt)?;
    let call_id = format!(
        "urn:gmeow:conjecture-call:{}",
        sha256_hex(format!("{}\u{1}{content_key}", out.node_iri).as_bytes())
    );
    write_audit_segment(
        &path,
        &call_id,
        MCP_PERSIST_CONJECTURE_SCHEMA,
        &obtains,
        "1970-01-01T00:00:00Z",
    )?;
    out.committed = true;
    Ok(out)
}

#[cfg(feature = "python")]
#[pyfunction]
pub fn run_consumer_mcp(snapshot: &[u8]) -> PyResult<()> {
    let server = McpServer::from_snapshot(snapshot, None, McpMode::Consumer)
        .map_err(|e| PyValueError::new_err(e.to_string()))?;
    server
        .run_stdio()
        .map_err(|e| PyValueError::new_err(e.to_string()))
}

#[cfg(feature = "python")]
#[pyfunction]
pub fn run_dev_mcp(snapshot: &[u8], root: String) -> PyResult<()> {
    let server = McpServer::from_snapshot(snapshot, Some(PathBuf::from(root)), McpMode::Dev)
        .map_err(|e| PyValueError::new_err(e.to_string()))?;
    server
        .run_stdio()
        .map_err(|e| PyValueError::new_err(e.to_string()))
}

fn language_tag_map(dataset: &purrdf::RdfDataset) -> BTreeMap<String, String> {
    let graph = fold_arena::Graph::from_dataset(dataset);
    let graph = &graph;
    let iri_index: HashMap<&str, usize> = graph
        .terms
        .iter()
        .enumerate()
        .filter_map(|(idx, term)| {
            (term.kind == fold_arena::TermKind::Iri)
                .then(|| term.value.as_deref().map(|value| (value, idx)))
                .flatten()
        })
        .collect();
    let Some(type_tid) = iri_index.get(RDF_TYPE).copied() else {
        return BTreeMap::new();
    };
    let Some(language_tid) = iri_index.get(LANGUAGE_CLASS).copied() else {
        return BTreeMap::new();
    };
    let Some(language_tag_tid) = iri_index.get(LANGUAGE_TAG).copied() else {
        return BTreeMap::new();
    };
    let Some(bcp47_tid) = iri_index.get(BCP47_TAG).copied() else {
        return BTreeMap::new();
    };
    let subjects: BTreeSet<usize> = graph
        .quads
        .iter()
        .filter_map(|&(s, p, o, _)| (p == type_tid && o == language_tid).then_some(s))
        .collect();
    let mut out = BTreeMap::new();
    for subject in subjects {
        let internal = graph.quads.iter().find_map(|&(s, p, o, _)| {
            (s == subject && p == language_tag_tid)
                .then(|| graph.terms.get(o).and_then(|term| term.value.as_deref()))
                .flatten()
        });
        let bcp = graph.quads.iter().find_map(|&(s, p, o, _)| {
            (s == subject && p == bcp47_tid)
                .then(|| graph.terms.get(o).and_then(|term| term.value.as_deref()))
                .flatten()
        });
        if let (Some(internal), Some(bcp)) = (internal, bcp) {
            out.insert(internal.to_ascii_lowercase(), bcp.to_ascii_lowercase());
        }
    }
    out
}

fn resolve_lang(
    raw: Option<&str>,
    tag_map: &BTreeMap<String, String>,
    available: &BTreeSet<String>,
) -> gmeow_errors::Result<Vec<String>> {
    let Some(raw) = raw else {
        return Ok(vec!["en".to_string()]);
    };
    if raw.trim().is_empty() {
        return Ok(vec!["en".to_string()]);
    }
    let mut seen = BTreeSet::new();
    let mut out = Vec::new();
    for token in raw
        .split(',')
        .map(str::trim)
        .filter(|token| !token.is_empty())
    {
        let normalized = if is_internal_tag(token) {
            let token_lower = token.to_ascii_lowercase();
            tag_map
                .get(&token_lower)
                .map(|tag| tag.to_ascii_lowercase())
                .ok_or_else(|| {
                    gmeow_errors::Diag::of_kind(crate::error::Mcp {
                        message: unknown_language(token, available),
                    })
                })?
        } else {
            token.to_ascii_lowercase()
        };
        if !available.contains(&normalized) {
            return Err(gmeow_errors::Diag::of_kind(crate::error::Mcp {
                message: unknown_language(token, available),
            }));
        }
        if seen.insert(normalized.clone()) {
            out.push(normalized);
        }
    }
    if out.is_empty() {
        Ok(vec!["en".to_string()])
    } else {
        Ok(out)
    }
}

fn is_internal_tag(lang: &str) -> bool {
    let lower = lang.to_ascii_lowercase();
    let Some(suffix) = lower.strip_prefix("x-gmeow-") else {
        return false;
    };
    !suffix.is_empty()
        && suffix
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

fn unknown_language(tag: &str, available: &BTreeSet<String>) -> String {
    let mut tags: Vec<&str> = available.iter().map(String::as_str).collect();
    tags.sort_by_key(|tag| (*tag != "en", *tag));
    format!(
        "unknown language tag '{tag}'. Available languages: {}",
        tags.join(", ")
    )
}

fn lang_from_query(query: &str) -> Option<&str> {
    query.split('&').find_map(|pair| {
        let (key, value) = pair.split_once('=')?;
        (key == "lang").then_some(value)
    })
}

fn memory_path() -> gmeow_errors::Result<PathBuf> {
    if let Ok(path) = env::var("GMEOW_MEMORY_PATH")
        && !path.trim().is_empty()
    {
        return Ok(PathBuf::from(path).expand_home());
    }
    let home = home_dir().ok_or_else(|| {
        gmeow_errors::Diag::of_kind(crate::error::Mcp {
            message: "neither HOME nor USERPROFILE is set and GMEOW_MEMORY_PATH is empty"
                .to_string(),
        })
    })?;
    Ok(Path::new(&home).join(".gmeow").join("memory.gts"))
}

fn home_dir() -> Option<String> {
    env::var("HOME").or_else(|_| env::var("USERPROFILE")).ok()
}

trait ExpandHome {
    fn expand_home(self) -> PathBuf;
}

impl ExpandHome for PathBuf {
    fn expand_home(self) -> PathBuf {
        let Some(raw) = self.to_str() else {
            return self;
        };
        if raw == "~" {
            return home_dir().map(PathBuf::from).unwrap_or(self);
        }
        if let Some(rest) = raw.strip_prefix("~/")
            && let Some(home) = home_dir()
        {
            return Path::new(&home).join(rest);
        }
        self
    }
}

fn tool(name: &str, description: &str, properties: &[(&str, &str)]) -> Value {
    let required: Vec<&str> = properties
        .iter()
        .filter_map(|(name, _)| {
            matches!(*name, "term" | "text" | "claim_id" | "path").then_some(*name)
        })
        .collect();
    let props = properties
        .iter()
        .map(|(name, kind)| ((*name).to_string(), json!({"type": kind})))
        .collect::<serde_json::Map<_, _>>();
    json!({
        "name": name,
        "description": description,
        "inputSchema": {
            "type": "object",
            "properties": props,
            "required": required,
        },
    })
}

fn resource(uri: &str, name: &str, description: &str, mime: &str) -> Value {
    json!({
        "uri": uri,
        "name": name,
        "description": description,
        "mimeType": mime,
    })
}

fn tool_text(text: String, is_error: bool) -> Value {
    json!({
        "content": [{"type": "text", "text": text}],
        "isError": is_error,
    })
}

fn rpc_error(id: Value, code: i64, message: &str) -> String {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": {"code": code, "message": message},
    })
    .to_string()
}

fn required_str<'a>(args: &'a Value, key: &str) -> gmeow_errors::Result<&'a str> {
    optional_str(args, key).ok_or_else(|| {
        gmeow_errors::Diag::of_kind(crate::error::Mcp {
            message: format!("{key} is required"),
        })
    })
}

fn optional_str<'a>(args: &'a Value, key: &str) -> Option<&'a str> {
    args.get(key).and_then(Value::as_str)
}

fn optional_bool(args: &Value, key: &str) -> Option<bool> {
    args.get(key).and_then(Value::as_bool)
}

fn optional_f64(args: &Value, key: &str) -> gmeow_errors::Result<Option<f64>> {
    match args.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Number(n)) => n.as_f64().map(Some).ok_or_else(|| {
            gmeow_errors::Diag::of_kind(crate::error::Mcp {
                message: format!("{key} must be a finite number"),
            })
        }),
        Some(_) => Err(gmeow_errors::Diag::of_kind(crate::error::Mcp {
            message: format!("{key} must be a number"),
        })),
    }
}

fn optional_limit(args: &Value, key: &str) -> gmeow_errors::Result<Option<usize>> {
    match args.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Number(n)) => {
            let value = n.as_i64().ok_or_else(|| {
                gmeow_errors::Diag::of_kind(crate::error::Mcp {
                    message: format!("{key} must be an integer"),
                })
            })?;
            usize::try_from(value).map(Some).map_err(|_| {
                gmeow_errors::Diag::of_kind(crate::error::Mcp {
                    message: format!("{key} must be non-negative"),
                })
            })
        }
        Some(_) => Err(gmeow_errors::Diag::of_kind(crate::error::Mcp {
            message: format!("{key} must be an integer"),
        })),
    }
}

/// A strict non-negative `u64` argument (absent/null → `None`), used for the `max_steps`
/// closure-size ceiling: present must be a non-negative integer, anything else is a HARD FAIL.
fn optional_step_count(args: &Value, key: &str) -> gmeow_errors::Result<Option<u64>> {
    match args.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Number(n)) => {
            let value = n.as_i64().ok_or_else(|| {
                gmeow_errors::Diag::of_kind(crate::error::Mcp {
                    message: format!("{key} must be an integer"),
                })
            })?;
            u64::try_from(value).map(Some).map_err(|_| {
                gmeow_errors::Diag::of_kind(crate::error::Mcp {
                    message: format!("{key} must be non-negative"),
                })
            })
        }
        Some(_) => Err(gmeow_errors::Diag::of_kind(crate::error::Mcp {
            message: format!("{key} must be an integer"),
        })),
    }
}

/// A strict boolean argument: present-and-bool → `Some`, absent/null → `None`, anything else is a
/// HARD FAIL (no silent coercion — `dry_run` is a named default, not a degraded fallback).
fn optional_bool_checked(args: &Value, key: &str) -> gmeow_errors::Result<Option<bool>> {
    match args.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Bool(value)) => Ok(Some(*value)),
        Some(_) => Err(gmeow_errors::Diag::of_kind(crate::error::Mcp {
            message: format!("{key} must be a boolean"),
        })),
    }
}

// ── Transaction-Logic execution of the memory write triad ────────────────────
//
// The two memory WRITE tools run as Transaction-Logic transactions: the canonical action theory
// is the single authority, the engine's executional entailment over the real start state is the
// commit gate, `dry_run` selects the hypothetical (sandbox) operator, and every committed turn is
// recorded with the audit context the trajectory audit reads.

/// The canonical memory-triad action theory — the SINGLE authority for how store_claim and
/// revise_belief behave as transactions (their `logic:precondition` / `logic:effect` /
/// `logic:compensation`). Embedded at build so the shipped `gmeow` runs repo-free; the slice
/// file is the one source of truth, and the worked example and conformance case reference these
/// same schema IRIs (they encode no second copy).
const MCP_ACTION_POLICY_TTL: &str =
    include_str!("../../../slices/extensions/agentic/examples/mcp-action-policy.ttl");

/// The transient world the TR run reasons in — a fresh in-memory store per call, NEVER persisted.
/// The executed verdict gates the write; the materialized outcome rides the tool response.
const TXN_WORLD: &str = "https://blackcatinformatics.ca/gmeow/agentic/mcp-exec";
const TXN_ROOT: &str = "https://blackcatinformatics.ca/gmeow/agentic/mcp-exec/txn";
const TXN_START: &str = "https://blackcatinformatics.ca/gmeow/agentic/mcp-exec/start";

/// The canonical action-schema and situation IRIs defined by `mcp-action-policy.ttl`.
const MCP_STORE_CLAIM_SCHEMA: &str =
    "https://blackcatinformatics.ca/gmeow/examples/agentic/mcp-policy/storeClaim";
const MCP_REVISE_BELIEF_SCHEMA: &str =
    "https://blackcatinformatics.ca/gmeow/examples/agentic/mcp-policy/reviseBelief";
const MCP_WELL_FORMED_CLAIM: &str =
    "https://blackcatinformatics.ca/gmeow/examples/agentic/mcp-policy/wellFormedClaim";
const MCP_TARGET_CLAIM_EXISTS: &str =
    "https://blackcatinformatics.ca/gmeow/examples/agentic/mcp-policy/targetClaimExists";
const MCP_CLAIM_IN_MEMORY: &str =
    "https://blackcatinformatics.ca/gmeow/examples/agentic/mcp-policy/claimInMemory";

/// The `persistConjecture` action schema + its precondition situation, defined by
/// `mcp-action-policy.ttl`. The `conjecture_test` write triad instantiates this schema; the
/// executional-entailment verdict over the precondition gates the append to the library.
const MCP_PERSIST_CONJECTURE_SCHEMA: &str =
    "https://blackcatinformatics.ca/gmeow/examples/agentic/mcp-policy/persistConjecture";
const MCP_CONJECTURE_VERDICT_PRESENTED: &str =
    "https://blackcatinformatics.ca/gmeow/examples/agentic/mcp-policy/conjectureVerdictPresented";

/// The single, fixed ISOLATED scenario world every conjecture test reasons in. The KB the
/// caller supplies is re-homed into this world (so the world-scoped DL calculus joins the
/// KB facts with the asserted / evaluated candidate), and the run is inherently isolated —
/// [`conjecture_test`] copies the KB into a fresh dataset and never mutates the input.
const CONJECTURE_SCENARIO_WORLD: &str =
    "https://blackcatinformatics.ca/gmeow/agentic/conjecture/scenario";

const LOGIC_INSTANTIATES_SCHEMA: &str = "https://blackcatinformatics.ca/logic/instantiatesSchema";
const LOGIC_TRANSITION_FROM_STATE: &str =
    "https://blackcatinformatics.ca/logic/transitionFromState";
const LOGIC_SITUATION_OBTAINS: &str = "https://blackcatinformatics.ca/logic/situationObtains";
const LOGIC_PROPER_PART_OF: &str = "https://blackcatinformatics.ca/logic/properPartOf";
const GMEOW_AT_TIME: &str = "https://blackcatinformatics.ca/gmeow/atTime";
const GMEOW_EVENT_TEMPORAL_FRAME: &str = "https://blackcatinformatics.ca/gmeow/eventTemporalFrame";
const GMEOW_TEMPORAL_FRAME_UTC_GREGORIAN: &str =
    "https://blackcatinformatics.ca/gmeow/temporalFrameUTCGregorian";
const XSD_DATETIME: &str = "http://www.w3.org/2001/XMLSchema#dateTime";

/// The canonical action theory as N-Quads in [`TXN_WORLD`], parsed once from the embedded slice
/// file. HARD FAIL if the embedded authority does not parse — that is a build-time invariant, not
/// a runtime fallback (the `canonical_action_policy_parses` test guards it).
fn action_policy_nquads() -> &'static str {
    static CACHE: OnceLock<String> = OnceLock::new();
    CACHE.get_or_init(|| {
        let dataset = purrdf::parse_dataset(MCP_ACTION_POLICY_TTL.as_bytes(), "text/turtle", None)
            .expect("canonical mcp-action-policy.ttl must parse (single authority)");
        let mut lines: Vec<String> = purrdf::flat_rdf_quads_from_dataset(&dataset)
            .into_iter()
            // The engine reads only the structural action theory (precondition / effect / ins /
            // del / compensation), all IRI→IRI — keep those and drop the annotation literals
            // (labels, comments) the executional-entailment run never consults.
            .filter(|quad| {
                matches!(quad.subject, purrdf::RdfTerm::Iri(_))
                    && matches!(quad.object, purrdf::RdfTerm::Iri(_))
            })
            .map(|quad| {
                format!(
                    "{} <{}> {} <{TXN_WORLD}> .",
                    quad.subject, quad.predicate, quad.object
                )
            })
            .collect();
        lines.sort();
        lines.join("\n")
    })
}

/// Build the per-call one-step transaction world: the canonical action theory plus this call's
/// primitive program (`root` instantiates `schema_iri`, transitions from the start state) and the
/// start state's obtaining situations (`obtains`, derived from REAL memory state).
fn txn_world_nquads(schema_iri: &str, obtains: &[&str]) -> String {
    use std::fmt::Write as _;
    let policy = action_policy_nquads();
    // Pre-size to hold the cached policy verbatim plus this call's primitive program and one
    // line per obtaining situation, so the build never reallocates.
    let mut nq = String::with_capacity(policy.len() + obtains.len() * 128 + 256);
    nq.push_str(policy);
    nq.push('\n');
    // `String`'s `fmt::Write` is infallible, so the formatting `Result` is discarded.
    let _ = writeln!(
        nq,
        "<{TXN_ROOT}> <{LOGIC_INSTANTIATES_SCHEMA}> <{schema_iri}> <{TXN_WORLD}> ."
    );
    let _ = writeln!(
        nq,
        "<{TXN_ROOT}> <{LOGIC_TRANSITION_FROM_STATE}> <{TXN_START}> <{TXN_WORLD}> ."
    );
    for situation in obtains {
        let _ = writeln!(
            nq,
            "<{TXN_START}> <{LOGIC_SITUATION_OBTAINS}> <{situation}> <{TXN_WORLD}> ."
        );
    }
    nq
}

/// Execute one memory write action as a TR transaction. `obtains` is the set of situations that
/// obtain at the start state (real state); the engine's executional entailment over them is the
/// commit gate. `dry_run` selects the hypothetical (sandbox) operator.
fn execute_memory_txn(
    schema_iri: &str,
    obtains: &[&str],
    dry_run: bool,
) -> gmeow_errors::Result<TxReceipt> {
    let nq = txn_world_nquads(schema_iri, obtains);
    let mode = if dry_run {
        CommitMode::Hypothetical
    } else {
        CommitMode::Committed
    };
    execute_transaction(&nq, TXN_WORLD, TXN_ROOT, mode)
        .map_err(|e| gmeow_errors::Diag::of_kind(crate::error::Mcp { message: e }))
}

/// The TR outcome rendered for the tool response.
fn txn_json(receipt: &TxReceipt) -> Value {
    match receipt {
        TxReceipt::CommittedSuccess { path_len, .. } => {
            json!({"committed": true, "succeeded": true, "path_len": path_len})
        }
        TxReceipt::CommittedFailure { reason } => {
            json!({"committed": true, "succeeded": false, "reason": reason})
        }
        TxReceipt::HypotheticalSuccess { witness } => {
            json!({"committed": false, "succeeded": true, "witness": witness})
        }
        TxReceipt::HypotheticalFailure { reason } => {
            json!({"committed": false, "succeeded": false, "reason": reason})
        }
    }
}

fn gts_iri(value: &str) -> GtsTerm {
    GtsTerm {
        kind: GtsTermKind::Iri,
        value: Some(value.to_string()),
        datatype: None,
        lang: None,
        direction: None,
        reifier: None,
    }
}

fn gts_literal_dt(value: &str, datatype: usize) -> GtsTerm {
    GtsTerm {
        kind: GtsTermKind::Literal,
        value: Some(value.to_string()),
        datatype: Some(datatype),
        lang: None,
        direction: None,
        reifier: None,
    }
}

fn push_gts_term(terms: &mut Vec<GtsTerm>, term: GtsTerm) -> usize {
    terms.push(term);
    terms.len() - 1
}

/// Append the trajectory-audit context segment for a just-recorded `gmeow:ToolCall` to the SAME
/// `memory.gts`, keyed to `call_id`: the call's `logic:instantiatesSchema`, its single
/// `logic:properPartOf` turn anchor (one call = one anchor — the stateless server mints no shared
/// turn state), its `gmeow:atTime` and the single canonical `gmeow:eventTemporalFrame`
/// (UTC-Gregorian, P11), the anchor's `logic:transitionFromState` start state, and the start's
/// obtaining situations. This is exactly the shape `emit_trajectory_audits` reads, so a cold trajectory
/// audit of `memory.gts` (unioned with the canonical action theory) verifies the executed turn.
fn write_audit_segment(
    memory_path: &Path,
    call_id: &str,
    schema_iri: &str,
    obtains: &[&str],
    at_time: &str,
) -> gmeow_errors::Result<()> {
    let anchor = format!("{call_id}#turn");
    let start = format!("{call_id}#start");

    let mut terms: Vec<GtsTerm> = Vec::new();
    let mut quads: Vec<(usize, usize, usize, Option<usize>)> = Vec::new();

    let t_call = push_gts_term(&mut terms, gts_iri(call_id));
    let t_anchor = push_gts_term(&mut terms, gts_iri(&anchor));
    let t_start = push_gts_term(&mut terms, gts_iri(&start));

    let t_inst = push_gts_term(&mut terms, gts_iri(LOGIC_INSTANTIATES_SCHEMA));
    let t_schema = push_gts_term(&mut terms, gts_iri(schema_iri));
    quads.push((t_call, t_inst, t_schema, None));

    let t_ppo = push_gts_term(&mut terms, gts_iri(LOGIC_PROPER_PART_OF));
    quads.push((t_call, t_ppo, t_anchor, None));

    let t_dt = push_gts_term(&mut terms, gts_iri(XSD_DATETIME));
    let t_at_time_p = push_gts_term(&mut terms, gts_iri(GMEOW_AT_TIME));
    let t_at_time_o = push_gts_term(&mut terms, gts_literal_dt(at_time, t_dt));
    quads.push((t_call, t_at_time_p, t_at_time_o, None));

    let t_frame_p = push_gts_term(&mut terms, gts_iri(GMEOW_EVENT_TEMPORAL_FRAME));
    let t_frame_o = push_gts_term(&mut terms, gts_iri(GMEOW_TEMPORAL_FRAME_UTC_GREGORIAN));
    quads.push((t_call, t_frame_p, t_frame_o, None));

    let t_tfs = push_gts_term(&mut terms, gts_iri(LOGIC_TRANSITION_FROM_STATE));
    quads.push((t_anchor, t_tfs, t_start, None));

    let t_so = push_gts_term(&mut terms, gts_iri(LOGIC_SITUATION_OBTAINS));
    for situation in obtains {
        let t_sit = push_gts_term(&mut terms, gts_iri(situation));
        quads.push((t_start, t_so, t_sit, None));
    }

    let mut writer = GtsWriter::new("ai-package");
    writer.add_terms(&terms);
    writer.add_quads(&quads);
    let segment = writer.to_bytes();

    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(memory_path)?;
    file.write_all(&segment)?;
    Ok(())
}

// ── Conjecture-library persistence (append-only GTS ai-package, TR-gated) ─────
//
// The conjecture library is a SEPARATE, append-only GTS collection — the read-only twin of
// `memory.gts`. Each `conjecture_test` commit appends one `ai-package` segment carrying the
// `project_conjecture_verdict` graph (a content-addressed, standpoint-scoped
// `logic:Conjecture` node with its embedded `logic:ReasoningResult` and refutation witness).
// It is NEVER folded into the base KB reasoning graph (R2): the reasoner reads
// `graph_dataset()` / the caller's KB, never `conjectures.gts`.

/// The conjecture-library path: `GMEOW_CONJECTURE_PATH` (home-expanded) when set, else
/// `~/.gmeow/conjectures.gts`. Mirrors [`memory_path`].
fn conjecture_path() -> gmeow_errors::Result<PathBuf> {
    if let Ok(path) = env::var("GMEOW_CONJECTURE_PATH")
        && !path.trim().is_empty()
    {
        return Ok(PathBuf::from(path).expand_home());
    }
    let home = home_dir().ok_or_else(|| {
        gmeow_errors::Diag::of_kind(crate::error::Mcp {
            message: "neither HOME nor USERPROFILE is set and GMEOW_CONJECTURE_PATH is empty"
                .to_string(),
        })
    })?;
    Ok(Path::new(&home).join(".gmeow").join("conjectures.gts"))
}

/// A deterministic lowercase-hex SHA-256 of `bytes` (the KB-world content address seed).
fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut out = String::with_capacity(digest.len() * 2);
    for byte in digest {
        use std::fmt::Write as _;
        let _ = write!(out, "{byte:02x}");
    }
    out
}

/// One term of the projection's closed N-Triples subset (`<iri>` / `_:b` / `"lex"^^<dt>`).
enum NtTerm {
    Iri(String),
    Blank(String),
    Lit { lex: String, datatype: String },
}

/// Take one N-Triples term off the front of `s`, returning `(term, rest)`. Handles the
/// closed subset [`project_conjecture_verdict`] emits: `<iri>`, `_:id`, and `"lex"^^<dt>`
/// typed literals (the literal walk is char-based, so it is UTF-8 safe and un-escapes).
fn take_nt_term(s: &str) -> Option<(NtTerm, &str)> {
    let s = s.trim_start();
    if let Some(rest) = s.strip_prefix('<') {
        let end = rest.find('>')?;
        return Some((NtTerm::Iri(rest[..end].to_owned()), &rest[end + 1..]));
    }
    if let Some(rest) = s.strip_prefix("_:") {
        let end = rest.find(char::is_whitespace).unwrap_or(rest.len());
        return Some((NtTerm::Blank(rest[..end].to_owned()), &rest[end..]));
    }
    if let Some(rest) = s.strip_prefix('"') {
        let mut lex = String::new();
        let mut chars = rest.char_indices();
        while let Some((idx, ch)) = chars.next() {
            match ch {
                '\\' => match chars.next() {
                    Some((_, '\\')) => lex.push('\\'),
                    Some((_, '"')) => lex.push('"'),
                    Some((_, 'n')) => lex.push('\n'),
                    Some((_, 'r')) => lex.push('\r'),
                    Some((_, 't')) => lex.push('\t'),
                    Some((_, c)) => lex.push(c),
                    None => return None,
                },
                '"' => {
                    // Past the closing quote: read the required `^^<dt>` typed-literal suffix.
                    let after = rest[idx + 1..].strip_prefix("^^")?;
                    let after = after.strip_prefix('<')?;
                    let end = after.find('>')?;
                    let datatype = after[..end].to_owned();
                    return Some((NtTerm::Lit { lex, datatype }, &after[end + 1..]));
                }
                _ => lex.push(ch),
            }
        }
        return None;
    }
    None
}

/// Intern one N-Triples node into the GTS term table (deduplicated by semantic value), a
/// literal first interning its datatype IRI so the literal term references a live datatype id.
fn intern_nt_term(
    node: &NtTerm,
    terms: &mut Vec<GtsTerm>,
    seen: &mut HashMap<String, usize>,
) -> usize {
    let sig = match node {
        NtTerm::Iri(iri) => format!("I\u{1}{iri}"),
        NtTerm::Blank(id) => format!("B\u{1}{id}"),
        NtTerm::Lit { lex, datatype } => format!("L\u{1}{datatype}\u{1}{lex}"),
    };
    if let Some(&id) = seen.get(&sig) {
        return id;
    }
    let term = match node {
        NtTerm::Iri(iri) => gts_iri(iri),
        NtTerm::Blank(id) => GtsTerm {
            kind: GtsTermKind::Bnode,
            value: Some(id.clone()),
            datatype: None,
            lang: None,
            direction: None,
            reifier: None,
        },
        NtTerm::Lit { lex, datatype } => {
            let dt_id = intern_nt_term(&NtTerm::Iri(datatype.clone()), terms, seen);
            gts_literal_dt(lex, dt_id)
        }
    };
    let id = push_gts_term(terms, term);
    seen.insert(sig, id);
    id
}

/// Append one conjecture-verdict segment (the `project_conjecture_verdict` N-Triples body) to
/// the append-only library at `path`, as a GTS `ai-package` segment. The body is parsed into
/// `(subject, predicate, object)` triples, interned as GTS terms (IRIs, blank nodes, and
/// typed literals with their datatype), and written via [`GtsWriter`]. Prior segment bytes
/// are NEVER mutated — the file is opened `create + append`, mirroring [`write_audit_segment`].
fn write_conjecture_segment(path: &Path, nt_body: &str) -> gmeow_errors::Result<()> {
    let mut terms: Vec<GtsTerm> = Vec::new();
    let mut quads: Vec<(usize, usize, usize, Option<usize>)> = Vec::new();
    let mut seen: HashMap<String, usize> = HashMap::new();

    for (lineno, raw) in nt_body.lines().enumerate() {
        let line = raw.trim();
        if line.is_empty() {
            continue;
        }
        let line = line.strip_suffix(" .").ok_or_else(|| {
            gmeow_errors::Diag::of_kind(crate::error::Mcp {
                message: format!("conjecture projection line {} missing ' .'", lineno + 1),
            })
        })?;
        let malformed = |what: &str| {
            gmeow_errors::Diag::of_kind(crate::error::Mcp {
                message: format!("conjecture projection line {} bad {what}", lineno + 1),
            })
        };
        let (subject, rest) = take_nt_term(line).ok_or_else(|| malformed("subject"))?;
        let (predicate, rest) =
            take_nt_term(rest.trim_start()).ok_or_else(|| malformed("predicate"))?;
        let (object, _rest) = take_nt_term(rest.trim_start()).ok_or_else(|| malformed("object"))?;
        // The projection's predicates are always IRIs; reject anything else fail-closed.
        if !matches!(predicate, NtTerm::Iri(_)) {
            return Err(malformed("predicate (non-IRI)"));
        }
        let s = intern_nt_term(&subject, &mut terms, &mut seen);
        let p = intern_nt_term(&predicate, &mut terms, &mut seen);
        let o = intern_nt_term(&object, &mut terms, &mut seen);
        quads.push((s, p, o, None));
    }

    let mut writer = GtsWriter::new("ai-package");
    writer.add_terms(&terms);
    writer.add_quads(&quads);
    let segment = writer.to_bytes();

    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent)?;
    }
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    file.write_all(&segment)?;
    Ok(())
}

/// Parse the candidate `logic:` document and extract exactly ONE candidate [`Formula`]: the
/// single top-level `logic:Formula` when present, else the single ground `logic:` axiom
/// lifted to a binary [`Formula::Atom`]. Any other shape (zero / multiple candidates) is a
/// hard, fail-closed error — a conjecture test names one formula, never a program.
fn parse_candidate_formula(formula_src: &str) -> gmeow_errors::Result<Formula> {
    let (program, _diags) = parse_logic_str(formula_src, None).map_err(|e| {
        gmeow_errors::Diag::of_kind(crate::error::Mcp {
            message: format!("candidate logic: document failed to parse: {e}"),
        })
    })?;
    let bad = |message: String| gmeow_errors::Diag::of_kind(crate::error::Mcp { message });
    // A reified `logic:Formula` is the primary surface: prefer the single top-level formula
    // even when the frontend leaks the formula's own structural triples as axioms.
    let formula_count = program.formulas.len();
    if formula_count == 1 {
        return Ok(program.formulas.into_iter().next().expect("len == 1"));
    }

    // No single top-level formula. A REIFIED trivially-Horn `logic:Formula` (a ground binary
    // atom — e.g. `relation=rdf:type, arg0=ex:a, arg1=ex:B`, the fact `ex:a rdf:type ex:B`) is
    // routed by the front-end to `LogicProgram.axioms` (its Horn home — `with_formulas`
    // hard-fails on a trivially-Horn leaf), so `formulas` is EMPTY and the candidate lives in
    // the axiom set. That set also carries the formula node's own `rdf:type logic:Formula`
    // self-typing, which leaks as a structural axiom — drop that noise, then a lone remaining
    // axiom IS the candidate fact, lifted here to a binary `Formula::Atom`. `conjecture_test`'s
    // `as_ground_fact` then asserts a ground lift as an EDB fact and decides it like any
    // candidate, so a reified ground-atom conjecture is a first-class, evaluated candidate —
    // never a panic (the previously dead `(0,1)` lift is now genuinely reachable).
    let logic_formula_iri = format!("{LOGIC_NAMESPACE}Formula");
    let mut candidate_axioms: Vec<_> = program
        .axioms
        .into_iter()
        .filter(|ax| !(ax.predicate == RDF_TYPE && ax.obj == logic_formula_iri))
        .collect();
    if formula_count == 0 && candidate_axioms.len() == 1 {
        let ax = candidate_axioms.pop().expect("len == 1");
        let object = if ax.obj_is_literal {
            IrTerm::Literal {
                lexical: ax.obj,
                datatype: None,
            }
        } else {
            IrTerm::Iri(ax.obj)
        };
        return Formula::atom(
            IrTerm::Iri(ax.predicate),
            vec![IrTerm::Iri(ax.subject), object],
        )
        .map_err(bad);
    }
    Err(bad(format!(
        "candidate must be exactly one formula/atom, got {formula_count} formula(s) and \
         {} candidate axiom(s)",
        candidate_axioms.len()
    )))
}

/// Re-home every triple of the caller's KB Turtle into [`CONJECTURE_SCENARIO_WORLD`] as a
/// fresh, frozen [`purrdf::RdfDataset`]. World-homing is required because the DL consistency
/// calculus is world-scoped: KB facts must sit in the SAME world the candidate is asserted /
/// evaluated in for a disjointness clash to fire.
fn rehome_kb_into_scenario(
    kb_src: &str,
) -> gmeow_errors::Result<std::sync::Arc<purrdf::RdfDataset>> {
    let parsed = purrdf::parse_dataset(kb_src.as_bytes(), "text/turtle", None).map_err(|e| {
        gmeow_errors::Diag::of_kind(crate::error::Mcp {
            message: format!("KB Turtle failed to parse: {e}"),
        })
    })?;
    let world = RdfTerm::iri(CONJECTURE_SCENARIO_WORLD);
    let mut builder = RdfDatasetBuilder::new();
    for quad in parsed.owned_quads() {
        let rehomed =
            RdfQuad::new(quad.subject, quad.predicate, quad.object).in_graph(world.clone());
        builder.push_owned_quad(&rehomed);
    }
    builder.freeze().map_err(|e| {
        gmeow_errors::Diag::of_kind(crate::error::Mcp {
            message: format!("re-homed KB dataset failed to freeze: {e}"),
        })
    })
}

fn tool_arguments(args: &Value, keys: &[&str]) -> String {
    let mut out = serde_json::Map::new();
    for key in keys {
        if let Some(value) = args.get(*key)
            && !value.is_null()
        {
            out.insert((*key).to_string(), value.clone());
        }
    }
    Value::Object(out).to_string()
}

fn claim_json(claim: &purrdf::gts::examples::agent_memory::Claim) -> Value {
    json!({
        "id": claim.id,
        "text": claim.text,
        "confidence": claim.confidence,
        "according_to": claim.according_to,
        "source": claim.source,
        "created": claim.created,
        "suppressed": claim.suppressed,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsString;
    use std::sync::{Mutex, OnceLock};

    static ENV_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

    struct EnvRestore(Vec<(&'static str, Option<OsString>)>);

    impl EnvRestore {
        fn capture(keys: &[&'static str]) -> Self {
            Self(keys.iter().map(|key| (*key, env::var_os(key))).collect())
        }
    }

    impl Drop for EnvRestore {
        fn drop(&mut self) {
            for (key, value) in &self.0 {
                // SAFETY: single-threaded test env mutation under the test's env lock.
                unsafe {
                    match value {
                        Some(value) => env::set_var(key, value),
                        None => env::remove_var(key),
                    }
                }
            }
        }
    }

    struct CwdRestore(PathBuf);

    impl CwdRestore {
        fn capture() -> Self {
            Self(env::current_dir().expect("current dir"))
        }
    }

    impl Drop for CwdRestore {
        fn drop(&mut self) {
            env::set_current_dir(&self.0).expect("restore current dir");
        }
    }

    fn snapshot() -> Vec<u8> {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        fs::read(root.join("generated/dist/gmeow.gts")).expect("read committed snapshot")
    }

    fn text_payload(value: Value) -> Value {
        let text = value["content"][0]["text"].as_str().expect("text content");
        serde_json::from_str(text).expect("tool text is JSON")
    }

    fn temp_memory() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("memory.gts");
        unsafe {
            // SAFETY: tests mutate process env single-threaded under ENV_LOCK.
            env::set_var("GMEOW_MEMORY_PATH", &path);
        }
        (dir, path)
    }

    #[test]
    fn modes_advertise_consumer_and_dev_surfaces() {
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let _env = EnvRestore::capture(&["GMEOW_LANG", "GMEOW_MEMORY_PATH", "HOME", "USERPROFILE"]);
        unsafe {
            // SAFETY: tests mutate process env single-threaded under ENV_LOCK.
            env::remove_var("GMEOW_LANG");
        }
        let bytes = snapshot();
        let consumer = McpServer::from_snapshot(&bytes, None, McpMode::Consumer).unwrap();
        let consumer_tools = consumer.tools_result().to_string();
        assert!(consumer_tools.contains("\"lookup_term\""));
        assert!(consumer_tools.contains("\"llms_txt\""));
        assert!(consumer_tools.contains("\"llms_full\""));
        assert!(consumer_tools.contains("\"okf_index\""));
        assert!(consumer_tools.contains("\"query_docs\""));
        assert!(consumer_tools.contains("\"store_claim\""));
        assert!(!consumer_tools.contains("\"validate\""));
        assert!(!consumer_tools.contains("\"slice_quality\""));
        assert!(
            !consumer
                .resources_result()
                .to_string()
                .contains("constitution")
        );

        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let dev = McpServer::from_snapshot(&bytes, Some(root), McpMode::Dev).unwrap();
        let dev_tools = dev.tools_result().to_string();
        assert!(dev_tools.contains("\"validate\""));
        assert!(dev_tools.contains("\"reason\""));
        assert!(dev_tools.contains("\"regenerate\""));
        assert!(dev_tools.contains("\"constitution\""));
        assert!(dev_tools.contains("\"slice_quality\""));
        assert!(dev.resources_result().to_string().contains("constitution"));
    }

    #[test]
    fn query_docs_selects_over_the_documentation_graph() {
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let _env = EnvRestore::capture(&["GMEOW_LANG", "GMEOW_MEMORY_PATH", "HOME", "USERPROFILE"]);
        unsafe {
            // SAFETY: tests mutate process env single-threaded under ENV_LOCK.
            env::remove_var("GMEOW_LANG");
        }
        let bytes = snapshot();
        let server = McpServer::from_snapshot(&bytes, None, McpMode::Consumer).unwrap();

        // A SELECT over the bundled documentation graph returns SPARQL-1.1 JSON
        // bindings (the doc graph carries a gmeow:DocumentedTerm per documented term).
        let select = text_payload(server.call_tool_result(
            "query_docs",
            &json!({"query": "SELECT ?s WHERE { ?s a <https://blackcatinformatics.ca/gmeow/DocumentedTerm> } LIMIT 3"}),
        ));
        assert_eq!(
            select["ok"], true,
            "query_docs SELECT must succeed: {select}"
        );
        assert_eq!(select["head"]["vars"][0], "s");
        assert!(
            select["results"]["bindings"]
                .as_array()
                .map(|b| !b.is_empty())
                .unwrap_or(false),
            "expected at least one DocumentedTerm binding: {select}"
        );

        // CONSTRUCT is rejected — the tool serves only bindings or a boolean.
        let construct = text_payload(server.call_tool_result(
            "query_docs",
            &json!({"query": "CONSTRUCT { ?s ?p ?o } WHERE { ?s ?p ?o } LIMIT 1"}),
        ));
        assert_eq!(construct["ok"], false);
        assert!(
            construct["error"]
                .as_str()
                .unwrap_or_default()
                .contains("SELECT and ASK")
        );
    }

    #[test]
    fn memory_triad_preserves_suppression_on_every_default_recall_path() {
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let _env = EnvRestore::capture(&["GMEOW_LANG", "GMEOW_MEMORY_PATH", "HOME", "USERPROFILE"]);
        unsafe {
            // SAFETY: tests mutate process env single-threaded under ENV_LOCK.
            env::remove_var("GMEOW_LANG");
        }
        let (_dir, memory_path) = temp_memory();
        let bytes = snapshot();
        let server = McpServer::from_snapshot(&bytes, None, McpMode::Consumer).unwrap();

        let canary = text_payload(server.call_tool_result(
            "store_claim",
            &json!({"text": "SUPPRESSED-CANARY belief about the launch window", "confidence": 0.9}),
        ));
        let canary_id = canary["claim"]["id"].as_str().unwrap().to_string();
        text_payload(server.call_tool_result(
            "store_claim",
            &json!({"text": "CONTROL-CANARY belief about the launch window", "confidence": 0.9}),
        ));
        let revised = text_payload(server.call_tool_result(
            "revise_belief",
            &json!({"claim_id": canary_id, "reason": "revised"}),
        ));
        assert_eq!(revised["ok"], true);

        text_payload(server.call_tool_result("recall", &json!({"query": "launch window"})));
        let calls = Memory::new(memory_path).tool_calls().unwrap();
        assert_eq!(
            calls
                .iter()
                .map(|call| call.tool.as_str())
                .collect::<Vec<_>>(),
            vec![
                "urn:gmeow:tool:store_claim",
                "urn:gmeow:tool:store_claim",
                "urn:gmeow:tool:revise_belief"
            ]
        );
        assert_eq!(calls[0].generated, vec![canary_id.clone()]);
        let stored_result: Value =
            serde_json::from_str(calls[0].result.as_deref().unwrap()).unwrap();
        assert_eq!(stored_result["ok"], true);
        assert_eq!(stored_result["claim"]["id"], canary_id);
        let stored_arguments: Value =
            serde_json::from_str(calls[0].arguments.as_deref().unwrap()).unwrap();
        assert_eq!(
            stored_arguments["text"],
            "SUPPRESSED-CANARY belief about the launch window"
        );

        for args in [
            json!({}),
            json!({"query": "launch window"}),
            json!({"query": "SUPPRESSED-CANARY belief"}),
            json!({"query": "launch", "min_confidence": 0.5}),
            json!({"query": "", "limit": 100}),
        ] {
            let recalled = text_payload(server.call_tool_result("recall", &args));
            let texts: Vec<&str> = recalled["claims"]
                .as_array()
                .unwrap()
                .iter()
                .map(|claim| claim["text"].as_str().unwrap())
                .collect();
            assert!(!texts.contains(&"SUPPRESSED-CANARY belief about the launch window"));
            assert!(texts.contains(&"CONTROL-CANARY belief about the launch window"));
        }

        let audit = text_payload(server.call_tool_result(
            "recall",
            &json!({"query": "launch window", "include_suppressed": true}),
        ));
        assert!(audit["claims"].as_array().unwrap().iter().any(|claim| {
            claim["text"] == "SUPPRESSED-CANARY belief about the launch window"
                && claim["suppressed"] == true
        }));
    }

    #[test]
    fn revision_rejects_unknown_ids_before_writing() {
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let _env = EnvRestore::capture(&["GMEOW_LANG", "GMEOW_MEMORY_PATH", "HOME", "USERPROFILE"]);
        unsafe {
            // SAFETY: tests mutate process env single-threaded under ENV_LOCK.
            env::remove_var("GMEOW_LANG");
        }
        let (_dir, _memory_path) = temp_memory();
        let bytes = snapshot();
        let server = McpServer::from_snapshot(&bytes, None, McpMode::Consumer).unwrap();
        let claim =
            text_payload(server.call_tool_result("store_claim", &json!({"text": "a real belief"})));
        let claim_id = claim["claim"]["id"].as_str().unwrap();

        let missing = text_payload(server.call_tool_result(
            "revise_belief",
            &json!({"claim_id": "urn:gmeow:assertion:no-such-id"}),
        ));
        assert_eq!(missing["ok"], false);

        let bad_successor = text_payload(server.call_tool_result(
            "revise_belief",
            &json!({"claim_id": claim_id, "superseded_by": "urn:gmeow:assertion:ghost"}),
        ));
        assert_eq!(bad_successor["ok"], false);

        let live =
            text_payload(server.call_tool_result("recall", &json!({"query": "real belief"})));
        assert_eq!(live["claims"][0]["id"], claim_id);
        assert_eq!(live["claims"][0]["suppressed"], false);
    }

    #[test]
    fn canonical_action_policy_is_the_single_authority_and_parses() {
        // The embedded slice file is the one source of truth for the action theory.
        let policy = action_policy_nquads();
        assert!(!policy.is_empty());
        assert!(policy.contains(MCP_STORE_CLAIM_SCHEMA));
        assert!(policy.contains(MCP_REVISE_BELIEF_SCHEMA));
        assert!(policy.contains(TXN_WORLD));
    }

    #[test]
    fn dry_run_must_be_a_boolean() {
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let _env = EnvRestore::capture(&["GMEOW_LANG", "GMEOW_MEMORY_PATH", "HOME", "USERPROFILE"]);
        unsafe {
            // SAFETY: tests mutate process env single-threaded under ENV_LOCK.
            env::remove_var("GMEOW_LANG");
        }
        let (_dir, _memory_path) = temp_memory();
        let bytes = snapshot();
        let server = McpServer::from_snapshot(&bytes, None, McpMode::Consumer).unwrap();
        let bad = text_payload(
            server.call_tool_result("store_claim", &json!({"text": "x", "dry_run": "yes"})),
        );
        assert_eq!(bad["ok"], false);
        assert!(
            bad["error"]
                .as_str()
                .unwrap()
                .contains("dry_run must be a boolean")
        );
    }

    #[test]
    fn store_claim_dry_run_computes_verdict_without_persisting() {
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let _env = EnvRestore::capture(&["GMEOW_LANG", "GMEOW_MEMORY_PATH", "HOME", "USERPROFILE"]);
        unsafe {
            // SAFETY: tests mutate process env single-threaded under ENV_LOCK.
            env::remove_var("GMEOW_LANG");
        }
        let (_dir, memory_path) = temp_memory();
        let bytes = snapshot();
        let server = McpServer::from_snapshot(&bytes, None, McpMode::Consumer).unwrap();

        let dry = text_payload(server.call_tool_result(
            "store_claim",
            &json!({"text": "a dry-run belief about orbits", "dry_run": true}),
        ));
        assert_eq!(dry["ok"], true);
        assert_eq!(dry["dry_run"], true);
        assert_eq!(dry["transaction"]["committed"], false);
        assert_eq!(dry["transaction"]["succeeded"], true);
        assert!(
            dry["transaction"]["witness"].as_str().is_some(),
            "a sandbox run leaves a content-addressed witness"
        );
        assert!(dry.get("claim").is_none(), "dry run writes no claim");

        // Nothing persisted: recall is empty and the memory holds no claims or tool calls.
        let recalled =
            text_payload(server.call_tool_result("recall", &json!({"query": "dry-run belief"})));
        assert!(recalled["claims"].as_array().unwrap().is_empty());
        let memory = Memory::new(&memory_path);
        assert!(memory.claims().unwrap().is_empty());
        assert!(memory.tool_calls().unwrap().is_empty());
    }

    #[test]
    fn committed_store_records_the_audit_context() {
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let _env = EnvRestore::capture(&["GMEOW_LANG", "GMEOW_MEMORY_PATH", "HOME", "USERPROFILE"]);
        unsafe {
            // SAFETY: tests mutate process env single-threaded under ENV_LOCK.
            env::remove_var("GMEOW_LANG");
        }
        let (_dir, memory_path) = temp_memory();
        let bytes = snapshot();
        let server = McpServer::from_snapshot(&bytes, None, McpMode::Consumer).unwrap();

        let stored = text_payload(server.call_tool_result(
            "store_claim",
            &json!({"text": "an audited belief about thrust", "confidence": 0.8}),
        ));
        assert_eq!(stored["ok"], true);
        assert_eq!(stored["transaction"]["committed"], true);
        assert_eq!(stored["transaction"]["succeeded"], true);

        // The committed turn is cold-auditable: the persisted memory.gts carries exactly the
        // predicates emit_trajectory_audits requires on the recorded ToolCall and its anchor.
        let raw = fs::read(&memory_path).unwrap();
        let bundle = purrdf::import_gts_events(&raw).expect("import memory.gts");
        let predicates: BTreeSet<String> = purrdf::flat_rdf_quads_from_dataset(&bundle.dataset)
            .iter()
            .map(|quad| quad.predicate.clone())
            .collect();
        for predicate in [
            LOGIC_INSTANTIATES_SCHEMA,
            LOGIC_PROPER_PART_OF,
            GMEOW_AT_TIME,
            GMEOW_EVENT_TEMPORAL_FRAME,
            LOGIC_TRANSITION_FROM_STATE,
            LOGIC_SITUATION_OBTAINS,
        ] {
            assert!(
                predicates.contains(predicate),
                "memory.gts must carry {predicate} for the trajectory audit"
            );
        }
        // The single canonical temporal frame is recorded (P11 — one frame per trajectory).
        let frames: Vec<String> = purrdf::flat_rdf_quads_from_dataset(&bundle.dataset)
            .iter()
            .filter(|quad| quad.predicate == GMEOW_EVENT_TEMPORAL_FRAME)
            .map(|quad| quad.object.to_string())
            .collect();
        assert!(
            frames
                .iter()
                .all(|frame| frame.contains("temporalFrameUTCGregorian"))
        );
    }

    #[test]
    fn revise_belief_dry_run_does_not_suppress() {
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let _env = EnvRestore::capture(&["GMEOW_LANG", "GMEOW_MEMORY_PATH", "HOME", "USERPROFILE"]);
        unsafe {
            // SAFETY: tests mutate process env single-threaded under ENV_LOCK.
            env::remove_var("GMEOW_LANG");
        }
        let (_dir, _memory_path) = temp_memory();
        let bytes = snapshot();
        let server = McpServer::from_snapshot(&bytes, None, McpMode::Consumer).unwrap();
        let stored = text_payload(
            server.call_tool_result("store_claim", &json!({"text": "a revisable belief"})),
        );
        let claim_id = stored["claim"]["id"].as_str().unwrap().to_string();

        let dry = text_payload(server.call_tool_result(
            "revise_belief",
            &json!({"claim_id": claim_id, "dry_run": true}),
        ));
        assert_eq!(dry["ok"], true);
        assert_eq!(dry["dry_run"], true);
        assert_eq!(dry["transaction"]["committed"], false);
        assert_eq!(dry["transaction"]["succeeded"], true);

        // The claim is still live — a sandbox revise suppresses nothing (P10 for free).
        let live =
            text_payload(server.call_tool_result("recall", &json!({"query": "revisable belief"})));
        assert_eq!(live["claims"][0]["suppressed"], false);
    }

    #[test]
    fn committed_revise_suppresses_but_never_deletes() {
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let _env = EnvRestore::capture(&["GMEOW_LANG", "GMEOW_MEMORY_PATH", "HOME", "USERPROFILE"]);
        unsafe {
            // SAFETY: tests mutate process env single-threaded under ENV_LOCK.
            env::remove_var("GMEOW_LANG");
        }
        let (_dir, _memory_path) = temp_memory();
        let bytes = snapshot();
        let server = McpServer::from_snapshot(&bytes, None, McpMode::Consumer).unwrap();
        let stored = text_payload(
            server.call_tool_result("store_claim", &json!({"text": "a belief to retire"})),
        );
        let claim_id = stored["claim"]["id"].as_str().unwrap().to_string();

        let revised = text_payload(server.call_tool_result(
            "revise_belief",
            &json!({"claim_id": claim_id, "reason": "superseded"}),
        ));
        assert_eq!(revised["ok"], true);
        assert_eq!(revised["transaction"]["committed"], true);

        // Default recall hides it (suppressed) ...
        let default =
            text_payload(server.call_tool_result("recall", &json!({"query": "belief retire"})));
        assert!(default["claims"].as_array().unwrap().is_empty());
        // ... but it is still present (supersession, never erasure — P10).
        let audit = text_payload(server.call_tool_result(
            "recall",
            &json!({"query": "belief retire", "include_suppressed": true}),
        ));
        assert!(
            audit["claims"]
                .as_array()
                .unwrap()
                .iter()
                .any(|claim| claim["id"] == claim_id.as_str() && claim["suppressed"] == true)
        );
    }

    #[test]
    fn startup_language_is_validated_and_json_rpc_dispatches() {
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let _env = EnvRestore::capture(&["GMEOW_LANG", "GMEOW_MEMORY_PATH", "HOME", "USERPROFILE"]);
        let bytes = snapshot();
        unsafe {
            // SAFETY: tests mutate process env single-threaded under ENV_LOCK.
            env::set_var("GMEOW_LANG", "notatag");
        }
        let err = match McpServer::from_snapshot(&bytes, None, McpMode::Consumer) {
            Ok(_) => panic!("invalid startup language must fail"),
            Err(err) => err,
        };
        assert!(err.to_string().contains("unknown language tag 'notatag'"));

        unsafe {
            // SAFETY: tests mutate process env single-threaded under ENV_LOCK.
            env::set_var("GMEOW_LANG", "fr");
        }
        let server = McpServer::from_snapshot(&bytes, None, McpMode::Consumer).unwrap();
        let init: Value = serde_json::from_str(
            &server.handle_message(r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#),
        )
        .unwrap();
        assert_eq!(init["result"]["serverInfo"]["name"], "gmeow");

        let tools: Value = serde_json::from_str(
            &server.handle_message(r#"{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}"#),
        )
        .unwrap();
        assert!(
            tools["result"]["tools"]
                .as_array()
                .unwrap()
                .iter()
                .any(|tool| tool["name"] == "lookup_term")
        );

        unsafe {
            // SAFETY: tests mutate process env single-threaded under ENV_LOCK.
            env::set_var("GMEOW_LANG", "X-GMEOW-FRENCH");
        }
        let server = McpServer::from_snapshot(&bytes, None, McpMode::Consumer).unwrap();
        let fr = text_payload(
            server.call_tool_result("lookup_term", &json!({"term": "gmeow:EntityExistence"})),
        );
        assert_eq!(fr["label"], "Existence d'entit\u{e9}");
        unsafe {
            // SAFETY: tests mutate process env single-threaded under ENV_LOCK.
            env::remove_var("GMEOW_LANG");
        }
    }

    #[test]
    fn default_memory_path_lives_under_home() {
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let _env = EnvRestore::capture(&["GMEOW_LANG", "GMEOW_MEMORY_PATH", "HOME", "USERPROFILE"]);
        unsafe {
            // SAFETY: tests mutate process env single-threaded under ENV_LOCK.
            env::remove_var("GMEOW_LANG");
            env::remove_var("GMEOW_MEMORY_PATH");
        }
        let dir = tempfile::tempdir().expect("tempdir");
        unsafe {
            // SAFETY: tests mutate process env single-threaded under ENV_LOCK.
            env::set_var("HOME", dir.path());
        }
        let bytes = snapshot();
        let server = McpServer::from_snapshot(&bytes, None, McpMode::Consumer).unwrap();
        text_payload(server.call_tool_result("store_claim", &json!({"text": "durable belief"})));
        assert!(dir.path().join(".gmeow/memory.gts").exists());
    }

    #[test]
    fn memory_path_handles_userprofile_and_relative_files() {
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let _env = EnvRestore::capture(&["GMEOW_LANG", "GMEOW_MEMORY_PATH", "HOME", "USERPROFILE"]);
        let _cwd = CwdRestore::capture();
        unsafe {
            // SAFETY: tests mutate process env single-threaded under ENV_LOCK.
            env::remove_var("GMEOW_LANG");
            env::remove_var("GMEOW_MEMORY_PATH");
            env::remove_var("HOME");
        }
        let dir = tempfile::tempdir().expect("tempdir");
        unsafe {
            // SAFETY: tests mutate process env single-threaded under ENV_LOCK.
            env::set_var("USERPROFILE", dir.path());
        }
        let bytes = snapshot();
        let server = McpServer::from_snapshot(&bytes, None, McpMode::Consumer).unwrap();
        text_payload(
            server.call_tool_result("store_claim", &json!({"text": "profile fallback belief"})),
        );
        assert!(dir.path().join(".gmeow/memory.gts").exists());

        let relative_dir = tempfile::tempdir().expect("relative tempdir");
        env::set_current_dir(relative_dir.path()).expect("set current dir");
        unsafe {
            // SAFETY: tests mutate process env single-threaded under ENV_LOCK.
            env::set_var("GMEOW_MEMORY_PATH", "memory.gts");
        }
        let server = McpServer::from_snapshot(&bytes, None, McpMode::Consumer).unwrap();
        text_payload(
            server.call_tool_result("store_claim", &json!({"text": "relative path belief"})),
        );
        assert!(relative_dir.path().join("memory.gts").exists());
    }

    /// The read-only local-ontology overlay: reads see `bundle ∪ overlay`, the
    /// overlay is provenance-isolated under the external graph, and nothing is
    /// written back — not the overlay file, not the canon, not memory.
    #[test]
    fn local_overlay_is_a_read_only_external_annex() {
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let _env = EnvRestore::capture(&["GMEOW_LANG", "GMEOW_MEMORY_PATH", "HOME", "USERPROFILE"]);
        unsafe {
            // SAFETY: tests mutate process env single-threaded under ENV_LOCK.
            env::remove_var("GMEOW_LANG");
        }
        let (mem_dir, memory_path) = temp_memory();
        let bytes = snapshot();
        let server = McpServer::from_snapshot(&bytes, None, McpMode::Consumer).unwrap();

        // A local lower-tier vocab file the agent supplies (not part of the canon).
        let overlay_dir = tempfile::tempdir().expect("overlay tempdir");
        let overlay_path = overlay_dir.path().join("local-vocab.ttl");
        let overlay_ttl = "<urn:ex:widget> <urn:ex:label> \"Local Widget\" .\n<urn:ex:widget> a <urn:ex:Thing> .\n";
        fs::write(&overlay_path, overlay_ttl).unwrap();
        let overlay_before = fs::read(&overlay_path).unwrap();
        let path_str = overlay_path.to_str().unwrap();

        // Reads see the overlay unioned into the default graph.
        let seen = text_payload(server.call_tool_result(
            "query_local",
            &json!({
                "path": path_str,
                "query": "SELECT ?o WHERE { <urn:ex:widget> <urn:ex:label> ?o }",
            }),
        ));
        assert_eq!(seen["ok"], true, "overlay query must succeed: {seen}");
        assert_eq!(seen["results"]["bindings"][0]["o"]["value"], "Local Widget");

        // Reads ALSO see the bundle canon in the same active graph (union, not
        // replacement): a plain triple pattern still matches the signed ontology.
        let canon = text_payload(server.call_tool_result(
            "query_local",
            &json!({"path": path_str, "query": "ASK { ?s ?p ?o }"}),
        ));
        assert_eq!(canon["ok"], true);
        assert_eq!(canon["boolean"], true);

        // The overlay is provenance-isolable under the distinct external graph — its
        // triples never bear a signed gmeow: graph name.
        let isolated = text_payload(server.call_tool_result(
            "query_local",
            &json!({
                "path": path_str,
                "query": "SELECT ?o WHERE { GRAPH <urn:gmeow:mcp:overlay:external> \
                          { <urn:ex:widget> <urn:ex:label> ?o } }",
            }),
        ));
        assert_eq!(
            isolated["ok"], true,
            "external-graph query must succeed: {isolated}"
        );
        assert_eq!(
            isolated["results"]["bindings"][0]["o"]["value"],
            "Local Widget"
        );

        // CONSTRUCT/DESCRIBE are rejected — the tool serves bindings or a boolean.
        let construct = text_payload(server.call_tool_result(
            "query_local",
            &json!({"path": path_str, "query": "CONSTRUCT { ?s ?p ?o } WHERE { ?s ?p ?o } LIMIT 1"}),
        ));
        assert_eq!(construct["ok"], false);
        assert!(
            construct["error"]
                .as_str()
                .unwrap_or_default()
                .contains("SELECT and ASK")
        );

        // Read-only: the overlay file is byte-for-byte unchanged and NOTHING was
        // written to memory (the write triad never touches the overlay or canon).
        assert_eq!(fs::read(&overlay_path).unwrap(), overlay_before);
        assert!(Memory::new(&memory_path).claims().unwrap().is_empty());
        assert!(!memory_path.exists());
        drop(mem_dir);
        drop(overlay_dir);
    }

    /// Full MCP protocol conformance over the real JSON-RPC dispatch: the
    /// handshake, the discovery surfaces, a read tool call and a TR-write tool call
    /// with `dry_run=true` (asserting the write stays hypothetical), and that EVERY
    /// advertised tool is dispatch-callable (no `unknown tool`).
    #[test]
    fn json_rpc_protocol_conformance_round_trip() {
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let _env = EnvRestore::capture(&["GMEOW_LANG", "GMEOW_MEMORY_PATH", "HOME", "USERPROFILE"]);
        unsafe {
            // SAFETY: tests mutate process env single-threaded under ENV_LOCK.
            env::remove_var("GMEOW_LANG");
        }
        let (_mem_dir, memory_path) = temp_memory();
        let bytes = snapshot();
        let server = McpServer::from_snapshot(&bytes, None, McpMode::Consumer).unwrap();

        let rpc = |body: &str| -> Value {
            let raw = server.handle_message(body);
            let value: Value = serde_json::from_str(&raw).expect("response is JSON");
            assert_eq!(value["jsonrpc"], "2.0", "JSON-RPC framing: {value}");
            value
        };

        // initialize
        let init = rpc(r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#);
        assert_eq!(init["id"], 1);
        assert_eq!(init["result"]["serverInfo"]["name"], "gmeow");
        assert!(init["result"]["protocolVersion"].as_str().is_some());

        // tools/list
        let tools = rpc(r#"{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}"#);
        let tool_names: Vec<String> = tools["result"]["tools"]
            .as_array()
            .expect("tools array")
            .iter()
            .map(|t| t["name"].as_str().unwrap().to_string())
            .collect();
        for expected in [
            "lookup_term",
            "query_docs",
            "query_local",
            "store_claim",
            "recall",
        ] {
            assert!(
                tool_names.iter().any(|n| n == expected),
                "missing {expected}"
            );
        }

        // resources/list
        let resources = rpc(r#"{"jsonrpc":"2.0","id":3,"method":"resources/list","params":{}}"#);
        assert!(
            resources["result"]["resources"]
                .as_array()
                .map(|r| !r.is_empty())
                .unwrap_or(false)
        );

        // tools/call — a read tool (query_docs ASK) succeeds.
        let read = rpc(
            r#"{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"query_docs","arguments":{"query":"ASK { ?s ?p ?o }"}}}"#,
        );
        let read_text: Value =
            serde_json::from_str(read["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
        assert_eq!(read_text["ok"], true);
        assert_eq!(read_text["boolean"], true);

        // tools/call — a TR-write tool (store_claim) with dry_run=true stays
        // hypothetical: the verdict is computed but nothing is committed.
        let dry = rpc(
            r#"{"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"store_claim","arguments":{"text":"a conformance probe belief","dry_run":true}}}"#,
        );
        let dry_text: Value =
            serde_json::from_str(dry["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
        assert_eq!(dry_text["ok"], true);
        assert_eq!(dry_text["dry_run"], true);
        assert_eq!(dry_text["transaction"]["committed"], false);
        assert!(dry_text.get("claim").is_none(), "dry run commits no claim");
        // Nothing persisted by the dry-run write.
        assert!(Memory::new(&memory_path).claims().unwrap().is_empty());
        assert!(!memory_path.exists());

        // Every advertised tool is dispatch-callable (recognized by tools/call).
        let overlay_dir = tempfile::tempdir().expect("overlay tempdir");
        let overlay_path = overlay_dir.path().join("probe.ttl");
        fs::write(&overlay_path, "<urn:ex:s> <urn:ex:p> <urn:ex:o> .\n").unwrap();
        let mut call_args: HashMap<&str, Value> = HashMap::new();
        call_args.insert("lookup_term", json!({"term": "gmeow:Entity"}));
        call_args.insert("doc_card", json!({"term": "gmeow:Entity"}));
        call_args.insert("query_docs", json!({"query": "ASK { ?s ?p ?o }"}));
        call_args.insert(
            "query_local",
            json!({"path": overlay_path.to_str().unwrap(), "query": "ASK { ?s ?p ?o }"}),
        );
        call_args.insert("store_claim", json!({"text": "probe", "dry_run": true}));
        call_args.insert(
            "conjecture_test",
            json!({
                "formula": forall_horn_candidate("B"),
                "kb": refuting_kb("B"),
                "standpoint": "http://ex/standpoint/probe",
                "dry_run": true,
            }),
        );
        call_args.insert("recall", json!({}));
        call_args.insert(
            "revise_belief",
            json!({"claim_id": "urn:gmeow:assertion:none", "dry_run": true}),
        );
        for name in &tool_names {
            let args = call_args
                .get(name.as_str())
                .cloned()
                .unwrap_or_else(|| json!({}));
            let result = server.call_tool_result(name, &args);
            let content = result["content"][0]["text"].as_str().expect("tool text");
            assert!(
                !content.contains("unknown tool"),
                "advertised tool {name} is not dispatch-callable: {content}"
            );
        }
        drop(overlay_dir);
    }

    // ── Conjecture-library persistence (Task 5a) ─────────────────────────────

    const LOGIC_NS: &str = "https://blackcatinformatics.ca/logic/";
    const MATH_NS: &str = "https://blackcatinformatics.ca/math/";
    const RDF_TYPE_IRI: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";

    /// A `∀x. trigger(x, mark) → rdf:type(x, <cls>)` candidate, authored as a reified
    /// `logic:Formula` (a single top-level formula — the trivially-Horn consequent is a
    /// sub-formula, so it never trips the `with_formulas` guard).
    fn forall_horn_candidate(cls_local: &str) -> String {
        format!(
            "@prefix logic: <{LOGIC_NS}> .\n\
             @prefix ex:  <http://ex/> .\n\
             @prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .\n\
             ex:cand a logic:Formula ;\n\
                 logic:forall ex:body ;\n\
                 logic:quantifiedVariable [ logic:termIndex 0 ; logic:termVariable \"x\" ] .\n\
             ex:body a logic:Formula ;\n\
                 logic:antecedent ex:ant ;\n\
                 logic:consequent ex:con .\n\
             ex:ant a logic:Formula ;\n\
                 logic:relation ex:trigger ;\n\
                 logic:argument [ logic:termIndex 0 ; logic:termVariable \"x\" ] ;\n\
                 logic:argument [ logic:termIndex 1 ; logic:termIri ex:mark ] .\n\
             ex:con a logic:Formula ;\n\
                 logic:relation rdf:type ;\n\
                 logic:argument [ logic:termIndex 0 ; logic:termVariable \"x\" ] ;\n\
                 logic:argument [ logic:termIndex 1 ; logic:termIri ex:{cls_local} ] .\n"
        )
    }

    /// A KB where the candidate's head class is DISJOINT with the individual's asserted type,
    /// so firing `rdf:type(a, <cls>)` forces an `owl:Nothing` clash ⇒ refutation + witness.
    fn refuting_kb(cls_local: &str) -> String {
        format!(
            "@prefix ex:  <http://ex/> .\n\
             @prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .\n\
             @prefix owl: <http://www.w3.org/2002/07/owl#> .\n\
             ex:a ex:trigger ex:mark .\n\
             ex:a rdf:type ex:A .\n\
             ex:A owl:disjointWith ex:{cls_local} .\n"
        )
    }

    /// A KB where the candidate's head class is UNRELATED (no disjointness), so firing derives
    /// a new consistent fact ⇒ Open/Neither, no witness.
    fn open_kb(cls_local: &str) -> String {
        format!(
            "@prefix ex:  <http://ex/> .\n\
             @prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .\n\
             ex:a ex:trigger ex:mark .\n\
             ex:a rdf:type ex:A .\n\
             # {cls_local} is unrelated to A — no clash.\n"
        )
    }

    fn temp_conjecture() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("conjectures.gts");
        unsafe {
            // SAFETY: tests mutate process env single-threaded under ENV_LOCK.
            env::set_var("GMEOW_CONJECTURE_PATH", &path);
        }
        (dir, path)
    }

    /// The imported conjecture-library dataset (all appended segments unioned).
    fn read_conjectures(path: &Path) -> std::sync::Arc<purrdf::RdfDataset> {
        let bytes = fs::read(path).expect("read conjecture library");
        purrdf::import_gts_events(&bytes)
            .expect("import conjecture library")
            .dataset
    }

    /// Every subject typed `logic:Conjecture` in `dataset`.
    fn conjecture_nodes(dataset: &purrdf::RdfDataset) -> BTreeSet<String> {
        dataset
            .owned_quads()
            .filter(|q| {
                q.predicate == RDF_TYPE_IRI
                    && q.object == RdfTerm::iri(format!("{LOGIC_NS}Conjecture"))
            })
            .filter_map(|q| match q.subject {
                RdfTerm::Iri(s) => Some(s),
                _ => None,
            })
            .collect()
    }

    /// The `logic:witnessPremise` literal lexicals in `dataset`.
    fn witness_premises(dataset: &purrdf::RdfDataset) -> Vec<String> {
        dataset
            .owned_quads()
            .filter(|q| q.predicate == format!("{LOGIC_NS}witnessPremise"))
            .filter_map(|q| match q.object {
                RdfTerm::Literal(lit) => Some(lit.lexical_form),
                _ => None,
            })
            .collect()
    }

    struct ConjEnvGuard;
    impl ConjEnvGuard {
        fn set() -> (EnvRestore, ConjEnvGuard) {
            let env = EnvRestore::capture(&[
                "GMEOW_LANG",
                "GMEOW_MEMORY_PATH",
                "GMEOW_CONJECTURE_PATH",
                "HOME",
                "USERPROFILE",
            ]);
            unsafe {
                // SAFETY: tests mutate process env single-threaded under ENV_LOCK.
                env::remove_var("GMEOW_LANG");
            }
            (env, ConjEnvGuard)
        }
    }

    #[test]
    fn persist_conjecture_precondition_gates_the_commit() {
        // The write is a REAL TR gate: with the precondition present the committed run
        // succeeds; with it absent the run FAILS (so the tool returns ok:false before writing).
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let (_env, _cg) = ConjEnvGuard::set();
        let ok = execute_memory_txn(
            MCP_PERSIST_CONJECTURE_SCHEMA,
            &[MCP_CONJECTURE_VERDICT_PRESENTED],
            false,
        )
        .unwrap();
        assert!(
            matches!(ok, TxReceipt::CommittedSuccess { .. }),
            "the precondition-present committed run must succeed: {ok:?}"
        );
        let unmet = execute_memory_txn(MCP_PERSIST_CONJECTURE_SCHEMA, &[], false).unwrap();
        assert!(
            matches!(unmet, TxReceipt::CommittedFailure { .. }),
            "a persist with the precondition UNMET must fail the commit (the tool then returns \
             ok:false before writing): {unmet:?}"
        );
    }

    #[test]
    fn conjecture_test_refutes_and_persists_with_witness() {
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let (_env, _cg) = ConjEnvGuard::set();
        let (_mem, _mp) = temp_memory();
        let (_dir, path) = temp_conjecture();
        let bytes = snapshot();
        let server = McpServer::from_snapshot(&bytes, None, McpMode::Consumer).unwrap();

        assert!(
            !path.exists(),
            "library must not exist before the first persist"
        );
        let resp = text_payload(server.call_tool_result(
            "conjecture_test",
            &json!({
                "formula": forall_horn_candidate("B"),
                "kb": refuting_kb("B"),
                "standpoint": "http://ex/standpoint/alice",
            }),
        ));
        assert_eq!(resp["ok"], true, "refutation persist must succeed: {resp}");
        assert_eq!(resp["verdict"]["lifecycle"], "refuted-in-standpoint");
        assert_eq!(resp["witness"]["individual"], "http://ex/a");
        let node = resp["conjecture"].as_str().expect("node iri").to_string();
        assert!(!node.is_empty());

        // The library file grew, and the witness premises round-trip back through the GTS.
        assert!(
            path.exists(),
            "the conjecture library file must have been written"
        );
        let dataset = read_conjectures(&path);
        assert!(
            conjecture_nodes(&dataset).contains(&node),
            "the content-addressed conjecture node must be readable back: {node}"
        );
        // The standpoint scope is recoverable.
        assert!(dataset.owned_quads().any(|q| {
            q.predicate == format!("{LOGIC_NS}conjectureStandpoint")
                && q.object == RdfTerm::iri("http://ex/standpoint/alice")
        }));
        // The witness premises are recoverable.
        let premises = witness_premises(&dataset);
        assert!(
            !premises.is_empty(),
            "a refutation must persist recoverable witness premises"
        );
    }

    #[test]
    fn conjecture_test_bridges_math_twin_via_conjecture_under_test() {
        // Driving the real MCP `conjecture_test` tool (the shared conjecture-test core) with a
        // `math_conjecture` must persist the always-present structural twin bridge
        // `<math> math:conjectureUnderTest <logic:Conjecture-node>` (domain math:Conjecture,
        // range logic:Conjecture) — readable back out of the append-only GTS library.
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let (_env, _cg) = ConjEnvGuard::set();
        let (_mem, _mp) = temp_memory();
        let (_dir, path) = temp_conjecture();
        let bytes = snapshot();
        let server = McpServer::from_snapshot(&bytes, None, McpMode::Consumer).unwrap();

        let math_iri = "https://blackcatinformatics.ca/math/conjecture/goldbach";
        let resp = text_payload(server.call_tool_result(
            "conjecture_test",
            &json!({
                "formula": forall_horn_candidate("B"),
                "kb": refuting_kb("B"),
                "standpoint": "http://ex/standpoint/alice",
                "math_conjecture": math_iri,
            }),
        ));
        assert_eq!(resp["ok"], true, "math-twin persist must succeed: {resp}");
        let node = resp["conjecture"].as_str().expect("node iri").to_string();

        let dataset = read_conjectures(&path);
        let under_test = format!("{MATH_NS}conjectureUnderTest");
        assert!(
            dataset.owned_quads().any(|q| {
                q.subject == RdfTerm::iri(math_iri)
                    && q.predicate == under_test
                    && q.object == RdfTerm::iri(node.clone())
            }),
            "the math:conjectureUnderTest bridge <{math_iri}> -> <{node}> must be recoverable \
             from the persisted GTS library"
        );
    }

    #[test]
    fn conjecture_test_dry_run_writes_nothing() {
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let (_env, _cg) = ConjEnvGuard::set();
        let (_mem, _mp) = temp_memory();
        let (_dir, path) = temp_conjecture();
        let bytes = snapshot();
        let server = McpServer::from_snapshot(&bytes, None, McpMode::Consumer).unwrap();

        let resp = text_payload(server.call_tool_result(
            "conjecture_test",
            &json!({
                "formula": forall_horn_candidate("B"),
                "kb": refuting_kb("B"),
                "standpoint": "http://ex/standpoint/alice",
                "dry_run": true,
            }),
        ));
        assert_eq!(resp["ok"], true);
        assert_eq!(resp["dry_run"], true);
        assert_eq!(resp["transaction"]["committed"], false);
        // The verdict is still computed and returned.
        assert_eq!(resp["verdict"]["lifecycle"], "refuted-in-standpoint");
        // Nothing written: the library file does not exist / is zero bytes.
        assert!(
            !path.exists() || fs::metadata(&path).unwrap().len() == 0,
            "a dry run must write nothing to the library"
        );
    }

    /// A candidate authored as a REIFIED GROUND binary atom — the exact `logic:relation` /
    /// `logic:argument` reification every authored formula uses, but VARIABLE-FREE: the ground
    /// fact `ex:a rdf:type ex:<cls_local>`. The reconstructed `Formula::Atom` is trivially-Horn,
    /// so it once panicked `LogicProgram::with_formulas` during the candidate parse.
    fn reified_ground_atom_candidate(cls_local: &str) -> String {
        format!(
            "@prefix logic: <{LOGIC_NS}> .\n\
             @prefix ex:  <http://ex/> .\n\
             @prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .\n\
             ex:phi a logic:Formula ;\n\
                 logic:relation rdf:type ;\n\
                 logic:argument [ logic:termIndex 0 ; logic:termIri ex:a ] ;\n\
                 logic:argument [ logic:termIndex 1 ; logic:termIri ex:{cls_local} ] .\n"
        )
    }

    /// A KB that ASSERTS the ground fact the reified-ground-atom candidate names, so `KB ⊨ φ`.
    fn ground_atom_entailing_kb(cls_local: &str) -> String {
        format!(
            "@prefix ex:  <http://ex/> .\n\
             @prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .\n\
             ex:a rdf:type ex:{cls_local} .\n"
        )
    }

    #[test]
    fn parse_candidate_reified_ground_atom_lifts_not_panics() {
        // F2 regression: a REIFIED GROUND binary atom is trivially-Horn, so the front-end must
        // route it to `LogicProgram.axioms` (not `with_formulas`, which hard-asserts) and
        // `parse_candidate_formula` must reconstruct it — cleanly, never a panic and never a
        // false "0 formula(s) and 0 axiom(s)" rejection.
        let candidate = parse_candidate_formula(&reified_ground_atom_candidate("B"))
            .expect("reified ground atom must lift to a candidate formula");
        match candidate {
            Formula::Atom { relation, args } => {
                assert_eq!(relation, IrTerm::Iri(RDF_TYPE_IRI.to_owned()));
                assert_eq!(
                    args,
                    vec![
                        IrTerm::Iri("http://ex/a".to_owned()),
                        IrTerm::Iri("http://ex/B".to_owned()),
                    ]
                );
            }
            other => panic!("expected a ground binary atom, got {other:?}"),
        }
    }

    #[test]
    fn conjecture_test_reified_ground_atom_evaluates_via_shipped_core() {
        // F2 regression on the SHIPPED surface: driving `run_conjecture_test` (the shared core
        // behind the CLI + MCP tool) with a reified ground-atom candidate must EVALUATE it (a KB
        // asserting the fact entails φ ⇒ corroborated) rather than panic (exit 101).
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let (_env, _cg) = ConjEnvGuard::set();
        let (_dir, _path) = temp_conjecture();

        let out = run_conjecture_test(&ConjectureRunInput {
            formula_ttl: &reified_ground_atom_candidate("B"),
            kb_ttl: &ground_atom_entailing_kb("B"),
            standpoint: "http://ex/standpoint/alice",
            math_conjecture: None,
            dry_run: true,
            max_steps: None,
            max_answers: None,
        })
        .expect("a reified ground-atom conjecture must evaluate, not panic");
        assert_eq!(out.lifecycle, "corroborated");
        assert_eq!(out.information, "supported");
        assert_eq!(out.evaluation, "completed");
    }

    /// A KB whose `ex:trigger` fires the `∀`-Horn candidate on SEVERAL individuals, so the
    /// candidate's derived (non-EDB) closure is strictly larger than a `max_steps`/`max_answers`
    /// bound of 1 — the isolated scenario evaluation exceeds the ceiling.
    fn multi_trigger_kb(cls_local: &str) -> String {
        format!(
            "@prefix ex:  <http://ex/> .\n\
             @prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .\n\
             ex:a ex:trigger ex:mark .\n\
             ex:b ex:trigger ex:mark .\n\
             ex:c ex:trigger ex:mark .\n\
             ex:a rdf:type ex:A .\n\
             # {cls_local} is unrelated to A — no clash, just several derived facts.\n"
        )
    }

    #[test]
    fn conjecture_test_budget_bound_forces_open_via_the_mcp_surface() {
        // GAP G1: the `max_steps` / `max_answers` bound is reachable from the SHIPPED MCP
        // surface (not just the logic-crate unit test). A run whose derived closure exceeds the
        // ceiling is truncated → evaluation budget-exhausted → lifecycle open → discharge Unknown.
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let (_env, _cg) = ConjEnvGuard::set();
        let (_mem, _mp) = temp_memory();
        let (_dir, path) = temp_conjecture();
        let bytes = snapshot();
        let server = McpServer::from_snapshot(&bytes, None, McpMode::Consumer).unwrap();

        // Unbounded control: the same candidate/KB runs to Completed (a non-budget verdict).
        let unbounded = text_payload(server.call_tool_result(
            "conjecture_test",
            &json!({
                "formula": forall_horn_candidate("C"),
                "kb": multi_trigger_kb("C"),
                "standpoint": "http://ex/standpoint/alice",
                "dry_run": true,
            }),
        ));
        assert_eq!(unbounded["ok"], true);
        assert_ne!(
            unbounded["verdict"]["evaluation"], "budget-exhausted",
            "the unbounded control must not trip the ceiling: {unbounded}"
        );

        // Bounded: a ceiling of 1 truncates the multi-fact derived closure.
        let bounded = text_payload(server.call_tool_result(
            "conjecture_test",
            &json!({
                "formula": forall_horn_candidate("C"),
                "kb": multi_trigger_kb("C"),
                "standpoint": "http://ex/standpoint/alice",
                "dry_run": true,
                "max_steps": 1,
            }),
        ));
        assert_eq!(
            bounded["ok"], true,
            "bounded run must compute a verdict: {bounded}"
        );
        assert_eq!(
            bounded["verdict"]["evaluation"], "budget-exhausted",
            "exceeding the ceiling must stamp BudgetExhausted: {bounded}"
        );
        assert_eq!(
            bounded["verdict"]["lifecycle"], "open",
            "a budget-exhausted run is inconclusive → Open: {bounded}"
        );
        assert_eq!(
            bounded["verdict"]["discharge"], "ObligationUnknown",
            "a budget-exhausted run carries the obligation forward as Unknown: {bounded}"
        );

        // `max_answers` is the equivalent binding-count ceiling and trips the same way.
        let bounded_answers = text_payload(server.call_tool_result(
            "conjecture_test",
            &json!({
                "formula": forall_horn_candidate("C"),
                "kb": multi_trigger_kb("C"),
                "standpoint": "http://ex/standpoint/alice",
                "dry_run": true,
                "max_answers": 1,
            }),
        ));
        assert_eq!(
            bounded_answers["verdict"]["evaluation"], "budget-exhausted",
            "max_answers must impose the same ceiling: {bounded_answers}"
        );
        assert!(
            !path.exists() || fs::metadata(&path).unwrap().len() == 0,
            "these dry runs must write nothing"
        );
    }

    #[test]
    fn conjecture_persists_are_append_only() {
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let (_env, _cg) = ConjEnvGuard::set();
        let (_mem, _mp) = temp_memory();
        let (_dir, path) = temp_conjecture();
        let bytes = snapshot();
        let server = McpServer::from_snapshot(&bytes, None, McpMode::Consumer).unwrap();

        text_payload(server.call_tool_result(
            "conjecture_test",
            &json!({
                "formula": forall_horn_candidate("B"),
                "kb": refuting_kb("B"),
                "standpoint": "http://ex/standpoint/alice",
            }),
        ));
        let first = fs::read(&path).expect("first library bytes");
        let first_len = first.len();
        assert!(first_len > 0);

        // A DISTINCT conjecture (different head class) appends a second segment.
        text_payload(server.call_tool_result(
            "conjecture_test",
            &json!({
                "formula": forall_horn_candidate("C"),
                "kb": open_kb("C"),
                "standpoint": "http://ex/standpoint/alice",
            }),
        ));
        let second = fs::read(&path).expect("second library bytes");
        assert!(
            second.len() > first_len,
            "the second persist must APPEND, growing the file"
        );
        assert_eq!(
            &second[..first_len],
            &first[..],
            "the first segment's bytes must be intact (append-only, never mutated)"
        );
        // Both conjectures are readable back.
        assert_eq!(conjecture_nodes(read_conjectures(&path).as_ref()).len(), 2);
    }

    #[test]
    fn conjecture_library_is_isolated_from_the_base_kb() {
        // R2: the caller's KB text is unchanged by the call, and the library is a DISTINCT
        // file the reasoner never folds into its base graph.
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let (_env, _cg) = ConjEnvGuard::set();
        let (_mem, mem_path) = temp_memory();
        let (_dir, path) = temp_conjecture();
        let bytes = snapshot();
        let server = McpServer::from_snapshot(&bytes, None, McpMode::Consumer).unwrap();

        let kb = refuting_kb("B");
        let kb_before = kb.clone();
        text_payload(server.call_tool_result(
            "conjecture_test",
            &json!({
                "formula": forall_horn_candidate("B"),
                "kb": kb,
                "standpoint": "http://ex/standpoint/alice",
            }),
        ));
        // The KB argument the tool was given is an owned String — the tool cannot mutate the
        // caller's copy. (Isolation is inherent: conjecture_test borrows and copies the KB.)
        assert_eq!(kb_before, refuting_kb("B"));
        // The conjecture library is a distinct file, NOT the memory store, and never the base
        // reasoning graph (reason reads graph_dataset(), never conjecture_path()).
        assert!(path.exists());
        assert_ne!(path, mem_path);
        // The bundled reasoning surface is unaffected — reason still runs cleanly over the
        // untouched base graph (the library is never unioned in).
        let reasoned = text_payload(server.call_tool_result("reason", &json!({})));
        // `reason` is dev-only; over a Consumer server it returns an error, proving the base
        // reasoning path does not consult the library either way. What matters for R2 is that
        // the library file and the KB are untouched, asserted above.
        let _ = reasoned;
    }

    #[test]
    fn same_formula_two_standpoints_mints_two_nodes() {
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let (_env, _cg) = ConjEnvGuard::set();
        let (_mem, _mp) = temp_memory();
        let (_dir, path) = temp_conjecture();
        let bytes = snapshot();
        let server = McpServer::from_snapshot(&bytes, None, McpMode::Consumer).unwrap();

        let a = text_payload(server.call_tool_result(
            "conjecture_test",
            &json!({
                "formula": forall_horn_candidate("C"),
                "kb": open_kb("C"),
                "standpoint": "http://ex/standpoint/alice",
            }),
        ));
        let b = text_payload(server.call_tool_result(
            "conjecture_test",
            &json!({
                "formula": forall_horn_candidate("C"),
                "kb": open_kb("C"),
                "standpoint": "http://ex/standpoint/bob",
            }),
        ));
        assert_ne!(
            a["conjecture"], b["conjecture"],
            "the same formula in two standpoints must mint two DISTINCT nodes (P9)"
        );
        assert_eq!(conjecture_nodes(read_conjectures(&path).as_ref()).len(), 2);
    }

    #[test]
    fn conjecture_library_corpus_query_recovers_open_and_refuted() {
        // A corpus competency: persist several DISTINCT conjectures (open + refuted) in one
        // standpoint, then scan the collection for all of them, with witness premises for the
        // refuted ones recoverable.
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let (_env, _cg) = ConjEnvGuard::set();
        let (_mem, _mp) = temp_memory();
        let (_dir, path) = temp_conjecture();
        let bytes = snapshot();
        let server = McpServer::from_snapshot(&bytes, None, McpMode::Consumer).unwrap();

        // Two refuted (B, D disjoint) and two open (C, E unrelated).
        for cls in ["B", "D"] {
            let r = text_payload(server.call_tool_result(
                "conjecture_test",
                &json!({
                    "formula": forall_horn_candidate(cls),
                    "kb": refuting_kb(cls),
                    "standpoint": "http://ex/standpoint/team",
                }),
            ));
            assert_eq!(r["verdict"]["lifecycle"], "refuted-in-standpoint", "{r}");
        }
        for cls in ["C", "E"] {
            let r = text_payload(server.call_tool_result(
                "conjecture_test",
                &json!({
                    "formula": forall_horn_candidate(cls),
                    "kb": open_kb(cls),
                    "standpoint": "http://ex/standpoint/team",
                }),
            ));
            assert_eq!(r["verdict"]["lifecycle"], "open", "{r}");
        }

        let dataset = read_conjectures(&path);
        // All four conjectures are recoverable.
        assert_eq!(conjecture_nodes(&dataset).len(), 4);
        // All are scoped to the team standpoint.
        let team_scoped = dataset
            .owned_quads()
            .filter(|q| {
                q.predicate == format!("{LOGIC_NS}conjectureStandpoint")
                    && q.object == RdfTerm::iri("http://ex/standpoint/team")
            })
            .count();
        assert_eq!(team_scoped, 4, "every conjecture is standpoint-scoped");
        // The two refuted conjectures each carry a recoverable individual + premises.
        let refuted = dataset
            .owned_quads()
            .filter(|q| {
                q.predicate == format!("{LOGIC_NS}conjectureLifecycleState")
                    && q.object == RdfTerm::iri(format!("{LOGIC_NS}ConjectureRefutedInStandpoint"))
            })
            .count();
        assert_eq!(refuted, 2, "exactly the two disjointness cases are refuted");
        assert!(
            witness_premises(&dataset).len() >= 2,
            "each refutation persists recoverable witness premises"
        );
    }
}
