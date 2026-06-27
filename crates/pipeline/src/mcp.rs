// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The MCP consumer surface (#1031): the Rust authority behind the consumer-safe
//! MCP server `gmeow_tools.mcp_server_consumer`.
//!
//! `McpView` loads the bundled `gmeow.gts` snapshot ONCE (the narrow waist #267,
//! bundle-only — never the repo) and serves the `export`-backed surfaces —
//! `lookup_term`, `llms_txt`, `llms_full`, `doc_card`, `okf_index` — over a
//! per-language [`FoldView`]. The standard `llms.txt`/`doc_card` surfaces (#1027)
//! make the docs themselves agent-consumable: the index links into the published
//! site (URLs recovered from the `gmeow:graph/documentation` graph) and the card
//! is the per-term, context-window-ready twin of the site's `card.md`. The
//! Python side stays thin FastMCP wiring: it resolves the `LangSelector` (reusing
//! the shared `language_tags` layer, so the behavioral-contract tests are
//! unchanged) and threads `requested` (public BCP-47 tags in precedence order)
//! into these methods. Collected terms are cached per requested-tag list,
//! mirroring the Python `_TERMS_CACHE`.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;

use gmeow_gts::model::Graph;

use crate::stages::export::{self, FoldView, Term};

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

#[pymethods]
impl McpView {
    /// Load and fold the bundled `gmeow.gts` snapshot bytes. Hard-fails if the
    /// snapshot does not read or lacks the ontology header (`fold_meta`).
    #[new]
    fn new(snapshot: &[u8]) -> PyResult<Self> {
        let graph = gmeow_rdf::gts::read_graph(snapshot, true)
            .map_err(|e| PyValueError::new_err(format!("read snapshot gmeow.gts: {e}")))?;
        let (title, version) = {
            let view = FoldView::new(&graph);
            export::fold_meta(&view).map_err(|e| PyValueError::new_err(e.to_string()))?
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
    fn lookup_term(&self, term: &str, requested: Vec<String>) -> String {
        self.with_terms(requested, |terms| export::lookup_envelope(terms, term))
    }

    /// The standard llmstxt.org vocabulary index (`llms.txt`) for `requested`,
    /// with bullets linking into the published docs site (#1027).
    fn llms_txt(&self, requested: Vec<String>) -> String {
        let title = self.title.clone();
        let version = self.version.clone();
        let doc_urls = self.doc_urls();
        self.with_terms(requested, |terms| {
            export::consumer_llms_txt(terms, &title, &version, &doc_urls)
        })
    }

    /// The complete inlined index (`llms-full.txt`) for `requested` (#1027) — the
    /// single-file, link-free surface an agent can ingest whole.
    fn llms_full(&self, requested: Vec<String>) -> String {
        let title = self.title.clone();
        let version = self.version.clone();
        self.with_terms(requested, |terms| {
            export::consumer_llms_full(terms, &title, &version)
        })
    }

    /// A prompt-ready Markdown card for one term (#1027) for `requested` — the
    /// live twin of the docs-site `terms/{slug}/card.md`.
    fn doc_card(&self, term: &str, requested: Vec<String>) -> String {
        self.with_terms(requested, |terms| export::doc_card_md(terms, term))
    }

    /// The OKF manifest JSON envelope for `requested`.
    fn okf_index(&self, requested: Vec<String>) -> String {
        self.with_terms(requested, export::okf_index_envelope)
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
