// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Native MCP surfaces over the bundled GMEOW snapshot.
//!
//! `McpView` loads the bundled `gmeow.gts` snapshot ONCE (the narrow waist #267,
//! bundle-only — never the repo) and serves the `export`-backed surfaces —
//! `lookup_term`, `llms_txt`, `llms_full`, `doc_card`, `okf_index` — over a
//! per-language [`FoldView`]. The standard `llms.txt`/`doc_card` surfaces (#1027)
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

use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use serde_json::{json, Value};

use gmeow_gts::examples::agent_memory::{
    Memory, RecallOptions, RevisionOptions, StoreOptions, ToolCallOptions,
};
use gmeow_gts::model::Graph;

use crate::stages::export::{self, FoldView, Term};

const LANGUAGE_CLASS: &str = "https://blackcatinformatics.ca/gmeow/Language";
const LANGUAGE_TAG: &str = "https://blackcatinformatics.ca/gmeow/languageTag";
const BCP47_TAG: &str = "https://blackcatinformatics.ca/gmeow/bcp47Tag";
const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
const TOOL_AGENT_NS: &str = "urn:gmeow:tool:";

/// A loaded, bundle-backed view over the GMEOW snapshot for the MCP consumer.
#[pyclass(name = "McpView", skip_from_py_object)]
pub struct McpView {
    graph: Graph,
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
    fn from_snapshot(snapshot: &[u8]) -> Result<Self, String> {
        let graph = gmeow_rdf::gts::read_graph(snapshot, true)
            .map_err(|e| format!("read snapshot gmeow.gts: {e}"))?;
        Self::from_graph(graph)
    }

    fn from_graph(graph: Graph) -> Result<Self, String> {
        let (title, version) = {
            let view = FoldView::new(&graph);
            export::fold_meta(&view).map_err(|e| e.to_string())?
        };
        Ok(Self {
            graph,
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
}

#[pymethods]
impl McpView {
    /// Load and fold the bundled `gmeow.gts` snapshot bytes. Hard-fails if the
    /// snapshot does not read or lacks the ontology header (`fold_meta`).
    #[new]
    fn new(snapshot: &[u8]) -> PyResult<Self> {
        Self::from_snapshot(snapshot).map_err(PyValueError::new_err)
    }

    /// Resolve a CURIE / local name / IRI / unambiguous prefix to its public
    /// metadata record (JSON envelope with `"ok"`), or a not-found envelope.
    fn lookup_term(&self, term: &str, requested: Vec<String>) -> String {
        self.lookup_term_json(term, requested)
    }

    /// The standard llmstxt.org vocabulary index (`llms.txt`) for `requested`,
    /// with bullets linking into the published docs site (#1027).
    fn llms_txt(&self, requested: Vec<String>) -> String {
        self.llms_txt_text(requested)
    }

    /// The complete inlined index (`llms-full.txt`) for `requested` (#1027) — the
    /// single-file, link-free surface an agent can ingest whole.
    fn llms_full(&self, requested: Vec<String>) -> String {
        self.llms_full_text(requested)
    }

    /// A prompt-ready Markdown card for one term (#1027) for `requested` — the
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
            let view = FoldView::new(&self.graph);
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
                let view = FoldView::with_requested(&self.graph, requested);
                Arc::new(export::collect_terms(&view))
            }))
        };
        f(terms.as_slice())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum McpMode {
    Consumer,
    Dev,
}

impl McpMode {
    fn from_bool(dev: bool) -> Self {
        if dev {
            Self::Dev
        } else {
            Self::Consumer
        }
    }

    fn includes_dev_tools(self) -> bool {
        self == Self::Dev
    }
}

/// A Rust MCP server over the bundled snapshot and optional repository root.
#[pyclass(name = "McpServer", skip_from_py_object)]
pub struct McpServer {
    view: McpView,
    mode: McpMode,
    root: Option<PathBuf>,
    tag_map: BTreeMap<String, String>,
    available: BTreeSet<String>,
    startup_requested: Vec<String>,
}

#[pymethods]
impl McpServer {
    /// Build a Rust MCP server. `dev=true` exposes repository-maintenance tools.
    #[new]
    #[pyo3(signature = (snapshot, root = None, dev = false))]
    fn new(snapshot: &[u8], root: Option<String>, dev: bool) -> PyResult<Self> {
        Self::from_snapshot(snapshot, root.map(PathBuf::from), McpMode::from_bool(dev))
            .map_err(PyValueError::new_err)
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
    fn from_snapshot(
        snapshot: &[u8],
        root: Option<PathBuf>,
        mode: McpMode,
    ) -> Result<Self, String> {
        let graph = gmeow_rdf::gts::read_graph(snapshot, true)
            .map_err(|e| format!("read snapshot gmeow.gts: {e}"))?;
        let tag_map = language_tag_map(&graph);
        let mut available: BTreeSet<String> =
            tag_map.values().map(|v| v.to_ascii_lowercase()).collect();
        available.insert("en".to_string());
        let startup_requested =
            resolve_lang(env::var("GMEOW_LANG").ok().as_deref(), &tag_map, &available)?;
        Ok(Self {
            view: McpView::from_graph(graph)?,
            mode,
            root,
            tag_map,
            available,
            startup_requested,
        })
    }

    fn requested_from_args(&self, args: &Value) -> Result<Vec<String>, String> {
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
                "store_claim",
                "Append one attributed memory claim.",
                &[
                    ("text", "string"),
                    ("source", "string"),
                    ("confidence", "number"),
                    ("according_to", "string"),
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
                "Suppress a stored claim without deleting history.",
                &[
                    ("claim_id", "string"),
                    ("reason", "string"),
                    ("superseded_by", "string"),
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
            "store_claim" => self.tool_store_claim(args),
            "recall" => self.tool_recall(args),
            "revise_belief" => self.tool_revise_belief(args),
            "validate" if self.mode.includes_dev_tools() => self.tool_validate(),
            "reason" if self.mode.includes_dev_tools() => self.tool_reason(),
            "regenerate" if self.mode.includes_dev_tools() => self.tool_regenerate(),
            "constitution" if self.mode.includes_dev_tools() => self.tool_constitution(),
            _ => Err(format!("unknown tool: {name}")),
        };
        match result {
            Ok(text) => tool_text(text, false),
            Err(err) => tool_text(json!({"ok": false, "error": err}).to_string(), true),
        }
    }

    fn read_resource_result(&self, uri: &str) -> Value {
        match self.read_resource_text(uri) {
            Ok((mime, text)) => json!({
                "contents": [{"uri": uri, "mimeType": mime, "text": text}],
            }),
            Err(err) => json!({
                "contents": [{"uri": uri, "mimeType": "application/json", "text": json!({"ok": false, "error": err}).to_string()}],
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

    fn run_stdio(&self) -> Result<(), String> {
        let stdin = io::stdin();
        let mut stdout = io::stdout();
        for line in stdin.lock().lines() {
            let line = line.map_err(|e| e.to_string())?;
            if line.trim().is_empty() {
                continue;
            }
            let response = self.handle_message(&line);
            if !response.is_empty() {
                writeln!(stdout, "{response}").map_err(|e| e.to_string())?;
                stdout.flush().map_err(|e| e.to_string())?;
            }
        }
        Ok(())
    }

    fn tool_lookup_term(&self, args: &Value) -> Result<String, String> {
        let term = required_str(args, "term")?;
        let requested = self.requested_from_args(args)?;
        Ok(self.view.lookup_term_json(term, requested))
    }

    fn tool_llms_txt(&self, args: &Value) -> Result<String, String> {
        let requested = self.requested_from_args(args)?;
        Ok(self.view.llms_txt_text(requested))
    }

    fn tool_llms_full(&self, args: &Value) -> Result<String, String> {
        let requested = self.requested_from_args(args)?;
        Ok(self.view.llms_full_text(requested))
    }

    fn tool_doc_card(&self, args: &Value) -> Result<String, String> {
        let term = required_str(args, "term")?;
        let requested = self.requested_from_args(args)?;
        Ok(self.view.doc_card_text(term, requested))
    }

    fn tool_okf_index(&self, args: &Value) -> Result<String, String> {
        let requested = self.requested_from_args(args)?;
        Ok(self.view.okf_index_json(requested))
    }

    fn tool_store_claim(&self, args: &Value) -> Result<String, String> {
        let text = required_str(args, "text")?;
        let confidence = optional_f64(args, "confidence")?;
        let memory = self.memory()?;
        let claim = memory
            .store(
                text,
                StoreOptions {
                    source: optional_str(args, "source"),
                    confidence,
                    according_to: optional_str(args, "according_to"),
                },
            )
            .map_err(|e| e.to_string())?;
        let response = json!({"ok": true, "claim": claim_json(&claim)}).to_string();
        let generated = [claim.id.as_str()];
        let _ = memory.record_tool_call(
            &format!("{TOOL_AGENT_NS}store_claim"),
            ToolCallOptions {
                arguments: Some(&tool_arguments(
                    args,
                    &["text", "source", "confidence", "according_to"],
                )),
                result: Some(&response),
                invocation: None,
                generated: &generated,
            },
        );
        Ok(response)
    }

    fn tool_recall(&self, args: &Value) -> Result<String, String> {
        let limit = optional_limit(args, "limit")?.unwrap_or(10);
        let claims = self
            .memory()?
            .recall(RecallOptions {
                query: optional_str(args, "query").unwrap_or(""),
                min_confidence: optional_f64(args, "min_confidence")?,
                limit,
                include_suppressed: optional_bool(args, "include_suppressed").unwrap_or(false),
            })
            .map_err(|e| e.to_string())?;
        Ok(json!({
            "ok": true,
            "claims": claims.iter().map(claim_json).collect::<Vec<_>>(),
        })
        .to_string())
    }

    fn tool_revise_belief(&self, args: &Value) -> Result<String, String> {
        let claim_id = required_str(args, "claim_id")?;
        let memory = self.memory()?;
        let known: BTreeSet<String> = memory
            .claims()
            .map_err(|e| e.to_string())?
            .into_iter()
            .map(|claim| claim.id)
            .collect();
        if !known.contains(claim_id) {
            return Ok(
                json!({"ok": false, "error": format!("unknown claim id: {claim_id}")}).to_string(),
            );
        }
        if let Some(successor) = optional_str(args, "superseded_by") {
            if !known.contains(successor) {
                return Ok(json!({
                    "ok": false,
                    "error": format!("unknown superseded_by id: {successor}"),
                })
                .to_string());
            }
        }
        memory
            .revise(
                claim_id,
                RevisionOptions {
                    reason: optional_str(args, "reason"),
                    superseded_by: optional_str(args, "superseded_by"),
                },
            )
            .map_err(|e| e.to_string())?;
        let response = json!({
            "ok": true,
            "suppressed": claim_id,
            "superseded_by": optional_str(args, "superseded_by"),
        })
        .to_string();
        let _ = memory.record_tool_call(
            &format!("{TOOL_AGENT_NS}revise_belief"),
            ToolCallOptions {
                arguments: Some(&tool_arguments(
                    args,
                    &["claim_id", "reason", "superseded_by"],
                )),
                result: Some(&response),
                invocation: None,
                generated: &[],
            },
        );
        Ok(response)
    }

    fn tool_validate(&self) -> Result<String, String> {
        let root = self.root_path()?;
        let report = crate::run::run_full(&root, 1, crate::run::RunMode::Check)
            .map_err(|e| e.to_string())?;
        Ok(json!({
            "ok": report.is_clean(),
            "mode": "check",
            "produced": report.produced,
            "reproduced": report.reproduced,
            "drifted": report.drifted,
        })
        .to_string())
    }

    fn tool_regenerate(&self) -> Result<String, String> {
        let root = self.root_path()?;
        let report = crate::run::run_full(&root, 1, crate::run::RunMode::Regenerate)
            .map_err(|e| e.to_string())?;
        Ok(json!({
            "ok": true,
            "mode": "regenerate",
            "produced": report.produced,
            "reproduced": report.reproduced,
        })
        .to_string())
    }

    fn tool_reason(&self) -> Result<String, String> {
        let result = gmeow_logic::reason::reason_all(self.view.graph_dataset()?.as_ref())
            .map_err(|e| format!("native reasoning failed: {e}"))?;
        Ok(json!({
            "ok": true,
            "input": result.input.wire(),
            "evaluation": result.evaluation.wire(),
            "completeness": result.completeness.wire(),
            "information": result.information.wire(),
        })
        .to_string())
    }

    fn tool_constitution(&self) -> Result<String, String> {
        let root = self.root_path()?;
        fs::read_to_string(root.join("CONSTITUTION.md")).map_err(|e| e.to_string())
    }

    fn read_resource_text(&self, uri: &str) -> Result<(&'static str, String), String> {
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
            _ => Err(format!("unknown resource: {uri}")),
        }
    }

    fn memory(&self) -> Result<Memory, String> {
        let path = memory_path()?;
        if let Some(parent) = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
        {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        Ok(Memory::new(path))
    }

    fn root_path(&self) -> Result<PathBuf, String> {
        self.root
            .clone()
            .ok_or_else(|| "repository root is required for dev MCP tools".to_string())
    }
}

impl McpView {
    fn graph_dataset(&self) -> Result<Arc<gmeow_rdf::RdfDataset>, String> {
        let bytes = gmeow_gts::nquads::to_nquads(&self.graph);
        gmeow_rdf::dataset_from_bytes(bytes.as_bytes(), gmeow_rdf::NativeRdfFormat::NQuads)
    }
}

#[pyfunction]
pub fn run_consumer_mcp(snapshot: &[u8]) -> PyResult<()> {
    let server = McpServer::from_snapshot(snapshot, None, McpMode::Consumer)
        .map_err(PyValueError::new_err)?;
    server.run_stdio().map_err(PyValueError::new_err)
}

#[pyfunction]
pub fn run_dev_mcp(snapshot: &[u8], root: String) -> PyResult<()> {
    let server = McpServer::from_snapshot(snapshot, Some(PathBuf::from(root)), McpMode::Dev)
        .map_err(PyValueError::new_err)?;
    server.run_stdio().map_err(PyValueError::new_err)
}

fn language_tag_map(graph: &Graph) -> BTreeMap<String, String> {
    let iri_index: HashMap<&str, usize> = graph
        .terms
        .iter()
        .enumerate()
        .filter_map(|(idx, term)| {
            (term.kind == gmeow_gts::model::TermKind::Iri)
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
) -> Result<Vec<String>, String> {
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
                .ok_or_else(|| unknown_language(token, available))?
        } else {
            token.to_ascii_lowercase()
        };
        if !available.contains(&normalized) {
            return Err(unknown_language(token, available));
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

fn memory_path() -> Result<PathBuf, String> {
    if let Ok(path) = env::var("GMEOW_MEMORY_PATH") {
        if !path.trim().is_empty() {
            return Ok(PathBuf::from(path).expand_home());
        }
    }
    let home =
        home_dir().ok_or("neither HOME nor USERPROFILE is set and GMEOW_MEMORY_PATH is empty")?;
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
        if let Some(rest) = raw.strip_prefix("~/") {
            if let Some(home) = home_dir() {
                return Path::new(&home).join(rest);
            }
        }
        self
    }
}

fn tool(name: &str, description: &str, properties: &[(&str, &str)]) -> Value {
    let required: Vec<&str> = properties
        .iter()
        .filter_map(|(name, _)| matches!(*name, "term" | "text" | "claim_id").then_some(*name))
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

fn required_str<'a>(args: &'a Value, key: &str) -> Result<&'a str, String> {
    optional_str(args, key).ok_or_else(|| format!("{key} is required"))
}

fn optional_str<'a>(args: &'a Value, key: &str) -> Option<&'a str> {
    args.get(key).and_then(Value::as_str)
}

fn optional_bool(args: &Value, key: &str) -> Option<bool> {
    args.get(key).and_then(Value::as_bool)
}

fn optional_f64(args: &Value, key: &str) -> Result<Option<f64>, String> {
    match args.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Number(n)) => n
            .as_f64()
            .map(Some)
            .ok_or_else(|| format!("{key} must be a finite number")),
        Some(_) => Err(format!("{key} must be a number")),
    }
}

fn optional_limit(args: &Value, key: &str) -> Result<Option<usize>, String> {
    match args.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Number(n)) => {
            let value = n
                .as_i64()
                .ok_or_else(|| format!("{key} must be an integer"))?;
            usize::try_from(value)
                .map(Some)
                .map_err(|_| format!("{key} must be non-negative"))
        }
        Some(_) => Err(format!("{key} must be an integer")),
    }
}

fn tool_arguments(args: &Value, keys: &[&str]) -> String {
    let mut out = serde_json::Map::new();
    for key in keys {
        if let Some(value) = args.get(*key) {
            if !value.is_null() {
                out.insert((*key).to_string(), value.clone());
            }
        }
    }
    Value::Object(out).to_string()
}

fn claim_json(claim: &gmeow_gts::examples::agent_memory::Claim) -> Value {
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
                match value {
                    Some(value) => env::set_var(key, value),
                    None => env::remove_var(key),
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
        env::set_var("GMEOW_MEMORY_PATH", &path);
        (dir, path)
    }

    #[test]
    fn modes_advertise_consumer_and_dev_surfaces() {
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let _env = EnvRestore::capture(&["GMEOW_LANG", "GMEOW_MEMORY_PATH", "HOME", "USERPROFILE"]);
        env::remove_var("GMEOW_LANG");
        let bytes = snapshot();
        let consumer = McpServer::from_snapshot(&bytes, None, McpMode::Consumer).unwrap();
        let consumer_tools = consumer.tools_result().to_string();
        assert!(consumer_tools.contains("\"lookup_term\""));
        assert!(consumer_tools.contains("\"llms_txt\""));
        assert!(consumer_tools.contains("\"llms_full\""));
        assert!(consumer_tools.contains("\"okf_index\""));
        assert!(consumer_tools.contains("\"store_claim\""));
        assert!(!consumer_tools.contains("\"validate\""));
        assert!(!consumer
            .resources_result()
            .to_string()
            .contains("constitution"));

        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let dev = McpServer::from_snapshot(&bytes, Some(root), McpMode::Dev).unwrap();
        let dev_tools = dev.tools_result().to_string();
        assert!(dev_tools.contains("\"validate\""));
        assert!(dev_tools.contains("\"reason\""));
        assert!(dev_tools.contains("\"regenerate\""));
        assert!(dev_tools.contains("\"constitution\""));
        assert!(dev.resources_result().to_string().contains("constitution"));
    }

    #[test]
    fn memory_triad_preserves_suppression_on_every_default_recall_path() {
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let _env = EnvRestore::capture(&["GMEOW_LANG", "GMEOW_MEMORY_PATH", "HOME", "USERPROFILE"]);
        env::remove_var("GMEOW_LANG");
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
        env::remove_var("GMEOW_LANG");
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
    fn startup_language_is_validated_and_json_rpc_dispatches() {
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let _env = EnvRestore::capture(&["GMEOW_LANG", "GMEOW_MEMORY_PATH", "HOME", "USERPROFILE"]);
        let bytes = snapshot();
        env::set_var("GMEOW_LANG", "notatag");
        let err = match McpServer::from_snapshot(&bytes, None, McpMode::Consumer) {
            Ok(_) => panic!("invalid startup language must fail"),
            Err(err) => err,
        };
        assert!(err.contains("unknown language tag 'notatag'"));

        env::set_var("GMEOW_LANG", "fr");
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
        assert!(tools["result"]["tools"]
            .as_array()
            .unwrap()
            .iter()
            .any(|tool| tool["name"] == "lookup_term"));

        env::set_var("GMEOW_LANG", "X-GMEOW-FRENCH");
        let server = McpServer::from_snapshot(&bytes, None, McpMode::Consumer).unwrap();
        let fr = text_payload(
            server.call_tool_result("lookup_term", &json!({"term": "gmeow:langFrench"})),
        );
        assert_eq!(fr["label"], "fran\u{e7}ais");
        env::remove_var("GMEOW_LANG");
    }

    #[test]
    fn default_memory_path_lives_under_home() {
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let _env = EnvRestore::capture(&["GMEOW_LANG", "GMEOW_MEMORY_PATH", "HOME", "USERPROFILE"]);
        env::remove_var("GMEOW_LANG");
        env::remove_var("GMEOW_MEMORY_PATH");
        let dir = tempfile::tempdir().expect("tempdir");
        env::set_var("HOME", dir.path());
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
        env::remove_var("GMEOW_LANG");
        env::remove_var("GMEOW_MEMORY_PATH");
        env::remove_var("HOME");
        let dir = tempfile::tempdir().expect("tempdir");
        env::set_var("USERPROFILE", dir.path());
        let bytes = snapshot();
        let server = McpServer::from_snapshot(&bytes, None, McpMode::Consumer).unwrap();
        text_payload(
            server.call_tool_result("store_claim", &json!({"text": "profile fallback belief"})),
        );
        assert!(dir.path().join(".gmeow/memory.gts").exists());

        let relative_dir = tempfile::tempdir().expect("relative tempdir");
        env::set_current_dir(relative_dir.path()).expect("set current dir");
        env::set_var("GMEOW_MEMORY_PATH", "memory.gts");
        let server = McpServer::from_snapshot(&bytes, None, McpMode::Consumer).unwrap();
        text_payload(
            server.call_tool_result("store_claim", &json!({"text": "relative path belief"})),
        );
        assert!(relative_dir.path().join("memory.gts").exists());
    }
}
