// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Ephemeral native source ownership and shared compilation for producer stages.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Instant;

use gmeow_logic_compile::frontend::{
    CompiledTheory, LogicParseError, PreparedLogicSource, SourceBase, SourceBaseOrigin,
    SourceDocument,
};
use purrdf::{CompositeDatasetView, DatasetView, RdfDataset, TermId, ViewLimits};

use crate::bundle::PipelineHandle;
use crate::node::{CachePolicy, Stage, StageInput, StageOutput, StageProduct, StageRunTiming};
use crate::stages::source_load::ParsedAuthoredSources;

pub(crate) mod language;

/// The deterministic producer owning original native document parses.
pub const STAGE_ID: &str = "stage-parse-sources";
/// Small generated receipt graph binding the ephemeral catalog's source identity.
pub const GRAPH_SOURCE_CATALOG: &str = "https://blackcatinformatics.ca/gmeow/graph/source-catalog";

type DocumentSelection = (String, Option<String>);

/// Original documents and one shared native aggregate for a producer invocation.
///
/// This is an inventory, not permission to assert all source roles in every logical
/// context. Original source scopes and positions remain available until the last
/// declared consumer finishes. Persistent products carry compact source bindings;
/// this catalog itself is never serialized or admitted as a test fixture.
pub struct SourceCatalog {
    sources: ParsedAuthoredSources,
    receipts: Vec<SourceDocument>,
    composite: CompositeDatasetView,
    materialized: Arc<RdfDataset>,
    identity: String,
    compiled: Mutex<Option<Arc<CompiledTheory>>>,
    documents: Mutex<std::collections::BTreeMap<DocumentSelection, Arc<CompiledTheory>>>,
    language: OnceLock<language::LanguageContext>,
    medium: OnceLock<crate::medium::registry::MediumRegistry>,
    reasoned_gates: OnceLock<Arc<gmeow_logic::verify::PreparedReasonedGates>>,
}

impl std::fmt::Debug for SourceCatalog {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SourceCatalog")
            .field("documents", &self.receipts.len())
            .field("identity", &self.identity)
            .finish_non_exhaustive()
    }
}

impl SourceCatalog {
    pub(crate) fn load(root: &Path) -> gmeow_errors::Result<Self> {
        Self::from_sources(ParsedAuthoredSources::load(root)?)
    }

    pub(crate) fn from_sources(sources: ParsedAuthoredSources) -> gmeow_errors::Result<Self> {
        let receipts: Vec<_> = sources
            .sources()
            .iter()
            .map(|source| SourceDocument {
                path: source.relative_path.clone(),
                content_digest: source.content_digest.clone(),
                role: source.kind.to_string(),
                base: source
                    .ingested
                    .document_base
                    .as_ref()
                    .map(|base| SourceBase {
                        iri: base.iri().as_str().to_owned(),
                        origin: match base.origin() {
                            purrdf::iri::BaseOrigin::Caller => SourceBaseOrigin::Caller,
                            purrdf::iri::BaseOrigin::Directive { line, column } => {
                                SourceBaseOrigin::Directive { line, column }
                            }
                            purrdf::iri::BaseOrigin::Enclosing => SourceBaseOrigin::Enclosing,
                        },
                    }),
            })
            .collect();
        let identity = crate::handle_identity::typed_digest(&("source-catalog-v1", &receipts));
        let composite = CompositeDatasetView::new(
            sources
                .sources()
                .iter()
                .map(|source| source.ingested.dataset.clone())
                .collect(),
            ViewLimits {
                max_sources: 4096,
                ..ViewLimits::default()
            },
        )
        .map_err(|error| stage_error(format!("admit source catalog: {error}")))?;
        let materialized = composite
            .materialize()
            .map_err(|error| stage_error(format!("materialize source catalog: {error}")))?;
        Ok(Self {
            sources,
            receipts,
            composite,
            materialized,
            identity,
            compiled: Mutex::new(None),
            documents: Mutex::new(std::collections::BTreeMap::new()),
            language: OnceLock::new(),
            medium: OnceLock::new(),
            reasoned_gates: OnceLock::new(),
        })
    }

    pub(crate) fn sources(&self) -> &ParsedAuthoredSources {
        &self.sources
    }

    /// One shared native dictionary, codebook, operator index and ring lattice.
    /// A missing or invalid selected source is terminal; no fallback dictionary.
    pub(crate) fn language(&self) -> gmeow_errors::Result<&language::LanguageContext> {
        self.language
            .get_or_try_init(|| language::LanguageContext::compile(self))
    }
    /// Prepare the original GTS module's medium registry once for source-local consumers.
    /// This does not substitute that source role for the broader runtime carrier registry.
    pub(crate) fn medium_registry(
        &self,
    ) -> gmeow_errors::Result<&crate::medium::registry::MediumRegistry> {
        self.medium.get_or_try_init(|| {
            let dataset = self.document("slices/core/gts/module.ttl")?;
            crate::medium::registry::MediumRegistry::from_dataset(dataset)
        })
    }

    pub(crate) fn identity(&self) -> &str {
        &self.identity
    }

    /// Share native verification laws prepared from the exact original module roles.
    /// Preparation is lazy and bounded to this catalog; synthetic catalogs need no
    /// authored modules unless they explicitly select the verification operation.
    pub(crate) fn prepared_reasoned_gates(
        &self,
    ) -> gmeow_errors::Result<Arc<gmeow_logic::verify::PreparedReasonedGates>> {
        self.reasoned_gates
            .get_or_try_init(|| {
                let [(math_path, math_iri), (logic_path, logic_iri)] =
                    gmeow_logic::verify::GATE_SOURCES;
                let math = self.compiled_document(math_path, Some(math_iri.to_owned()))?;
                let logic = self.compiled_document(logic_path, Some(logic_iri.to_owned()))?;
                gmeow_logic::verify::PreparedReasonedGates::from_compiled_sources(
                    self.document(math_path)?,
                    math.as_ref(),
                    self.document_digest(math_path)?,
                    logic.as_ref(),
                    self.document_digest(logic_path)?,
                )
                .map(Arc::new)
            })
            .map(Arc::clone)
    }

    /// Borrow the unchanged native parse of one explicitly selected document.
    ///
    /// # Errors
    /// Refuses a document absent from the admitted catalog; never rereads a file.
    pub fn document(&self, relative_path: &str) -> gmeow_errors::Result<&RdfDataset> {
        self.sources
            .sources()
            .iter()
            .find(|source| source.relative_path == relative_path)
            .map(|source| source.ingested.dataset.as_ref())
            .ok_or_else(|| stage_error(format!("source catalog has no document {relative_path:?}")))
    }

    /// Exact authored bytes identity of a retained native document.
    pub fn document_digest(&self, path: &str) -> gmeow_errors::Result<&str> {
        self.receipts
            .iter()
            .find(|source| source.path == path)
            .map(|source| source.content_digest.as_str())
            .ok_or_else(|| stage_error(format!("unknown source document {path:?}")))
    }

    /// Borrow the BLAKE3 captured from original bytes at the same one-parse boundary.
    ///
    /// # Errors
    /// Rejects a document absent from this catalog; never rereads source bytes.
    pub fn document_blake3_digest(&self, path: &str) -> gmeow_errors::Result<&str> {
        self.sources
            .sources()
            .iter()
            .find(|source| source.relative_path == path)
            .map(|source| source.blake3_digest.as_str())
            .ok_or_else(|| stage_error(format!("unknown source document {path:?}")))
    }

    /// Borrow the one aggregate materialization, retaining original graph placement.
    #[must_use]
    pub fn materialized(&self) -> &Arc<RdfDataset> {
        &self.materialized
    }

    /// Compile the entire admitted source/import catalog once per invocation.
    /// All readers share the same immutable program, source anchors and original
    /// diagnostics. This does not authorize flattening graph, role or context
    /// distinctions for execution. No cumulative carrier or persistent cache is
    /// added; the value dies with this catalog's last declared consumer.
    ///
    /// # Errors
    /// Propagates source preparation and compilation failures. The initializer
    /// lock prevents concurrent consumers from duplicating expensive compilation.
    pub fn compiled_logic(&self) -> gmeow_errors::Result<Arc<CompiledTheory>> {
        let mut cached = self
            .compiled
            .lock()
            .map_err(|_| stage_error("source compilation cache lock poisoned".into()))?;
        if let Some(compiled) = cached.as_ref() {
            return Ok(compiled.clone());
        }
        let compiled =
            Arc::new(self.prepare_logic()?.into_compiled(None).map_err(|error| {
                stage_error(format!("compile selected source catalog: {error}"))
            })?);
        *cached = Some(compiled.clone());
        Ok(compiled)
    }

    /// Share standalone source compilation by exact document and source IRI. This
    /// bounded, invocation-local cache never serializes the source dataset. A full
    /// cache recomputes the selected document with identical semantics.
    pub fn compiled_document(
        &self,
        path: &str,
        source_iri: Option<String>,
    ) -> gmeow_errors::Result<Arc<CompiledTheory>> {
        const MAX_DOCUMENT_COMPILATIONS: usize = 16;
        let key = (path.to_owned(), source_iri.clone());
        let mut cache = self
            .documents
            .lock()
            .map_err(|_| stage_error("document compilation cache lock poisoned".into()))?;
        if let Some(compiled) = cache.get(&key) {
            return Ok(compiled.clone());
        }
        let compiled = Arc::new(
            self.prepare_document(path)?
                .into_compiled(source_iri)
                .map_err(|error| {
                    stage_error(format!("compile selected document {path}: {error}"))
                })?,
        );
        if cache.len() < MAX_DOCUMENT_COMPILATIONS {
            cache.insert(key, compiled.clone());
        }
        Ok(compiled)
    }

    /// Prepare one explicitly selected document, retaining its original structural
    /// occurrences through that same native canonicalization.
    pub fn prepare_document(&self, path: &str) -> gmeow_errors::Result<PreparedLogicSource> {
        let index = self
            .sources
            .sources()
            .iter()
            .position(|source| source.relative_path == path)
            .ok_or_else(|| stage_error(format!("unknown source document {path:?}")))?;
        let mut prepared =
            PreparedLogicSource::new(&self.sources.sources()[index].ingested.dataset)
                .map_err(|error| stage_error(error.0))?;
        self.record_document(&mut prepared, index, Ok)?;
        Ok(prepared)
    }

    /// Prepare the aggregate catalog without asserting its source roles globally.
    /// Original document and graph occurrences survive union deduplication. Every
    /// mapping uses the actual composite and canonicalization maps, never labels.
    pub fn prepare_logic(&self) -> gmeow_errors::Result<PreparedLogicSource> {
        let mut prepared =
            PreparedLogicSource::new(&self.materialized).map_err(|error| stage_error(error.0))?;
        for index in 0..self.receipts.len() {
            self.record_document(&mut prepared, index, |term| {
                self.materialized_term_at(index, term)
            })?;
        }
        Ok(prepared)
    }

    fn record_document(
        &self,
        prepared: &mut PreparedLogicSource,
        index: usize,
        mut selection_term: impl FnMut(TermId) -> gmeow_errors::Result<TermId>,
    ) -> gmeow_errors::Result<()> {
        let original = &self.sources.sources()[index].ingested.dataset;
        prepared
            .record_document(self.receipts[index].clone(), original, |term| {
                selection_term(term).map_err(|error| LogicParseError(error.to_string()))
            })
            .map_err(|error| stage_error(error.0))
    }

    /// Resolve a term from the exact original document into the aggregate selection.
    ///
    /// Apply the aggregate frontend's canonical mapping to this returned ID, never
    /// to the original document ID. All IDs here are invocation-local references.
    ///
    /// # Errors
    /// Refuses unknown documents, out-of-range terms and absent materialized terms.
    pub fn materialized_term(
        &self,
        relative_path: &str,
        original: TermId,
    ) -> gmeow_errors::Result<TermId> {
        let (index, _) = self
            .sources
            .sources()
            .iter()
            .enumerate()
            .find(|(_, source)| source.relative_path == relative_path)
            .ok_or_else(|| stage_error(format!("unknown source document {relative_path:?}")))?;
        self.materialized_term_at(index, original)
    }

    fn materialized_term_at(&self, index: usize, original: TermId) -> gmeow_errors::Result<TermId> {
        let source = &self.sources.sources()[index];
        let relative_path = &source.relative_path;
        if original.index() >= source.ingested.dataset.term_count() {
            return Err(stage_error(format!(
                "source term is outside {relative_path:?}"
            )));
        }
        let mapped = match source.ingested.dataset.resolve(original) {
            // IRIs retain their identity across composition. Borrow the native
            // spelling instead of allocating an owned term on every cache miss.
            purrdf::TermRef::Iri(iri) => self.materialized.term_id_by_iri(iri),
            _ => {
                let value = self
                    .composite
                    .term_value(self.composite.source_id(index, original));
                self.materialized.term_id_by_value(&value)
            }
        };
        mapped.ok_or_else(|| {
            stage_error(format!(
                "source term from {relative_path:?} has no materialized binding"
            ))
        })
    }
}

fn stage_error(message: String) -> gmeow_errors::Diag {
    gmeow_errors::Diag::of_kind(crate::error::StageFailed {
        stage: STAGE_ID.into(),
        message,
    })
}

/// Borrow an upstream catalog through its mandatory typed product binding.
pub(crate) fn catalog<'a>(input: &StageInput<'a>) -> gmeow_errors::Result<&'a SourceCatalog> {
    let product = input
        .upstream
        .get(STAGE_ID)
        .ok_or_else(|| stage_error("missing native parse-stage product".into()))?;
    let _ = product.dataset();
    match product
        .bundle()
        .handle(GRAPH_SOURCE_CATALOG)
        .map(|entry| &entry.payload)
    {
        Some(PipelineHandle::SourceCatalog(catalog)) => Ok(catalog),
        _ => Err(stage_error(
            "parse-stage product has no native source catalog".into(),
        )),
    }
}

/// Explicit recomputed source producer; tests consume downstream persistent receipts.
pub struct ParseSourcesStage;

impl Stage for ParseSourcesStage {
    fn id(&self) -> &str {
        STAGE_ID
    }
    fn consumes(&self) -> &[String] {
        &[]
    }
    fn cache_policy(&self) -> CachePolicy {
        CachePolicy::Recompute
    }
    fn impl_version(&self) -> &str {
        "parse-sources.v2-structural-source-occurrences"
    }
    fn attaches_graphs(&self) -> &[String] {
        crate::stages::attach::graphs(STAGE_ID)
    }
    fn input_files(&self, root: &Path) -> gmeow_errors::Result<Vec<PathBuf>> {
        crate::stages::source_load::authored_files(root)
    }
    fn run(&self, input: StageInput<'_>) -> gmeow_errors::Result<StageOutput> {
        let started = Instant::now();
        let catalog = Arc::new(SourceCatalog::load(input.root)?);
        let product = source_catalog_product(catalog.clone())?;
        Ok(StageOutput {
            product,
            diags: Vec::new(),
            timings: vec![StageRunTiming {
                phase: "native-source-catalog".into(),
                elapsed_ms: started.elapsed().as_millis(),
                metadata: Some(format!(
                    "documents={};materialized-quads={};view={:?}",
                    catalog.receipts.len(),
                    catalog.materialized.quad_count(),
                    catalog.composite.stats()
                )),
            }],
        })
    }
}

/// Bind the native catalog to the same receipt used by cache keys and consumers.
fn source_catalog_product(catalog: Arc<SourceCatalog>) -> gmeow_errors::Result<StageProduct> {
    let mut builder = purrdf::RdfDatasetBuilder::new();
    let subject = purrdf::RdfTerm::iri(format!("urn:gmeow:source-catalog:{}", catalog.identity()));
    let graph = purrdf::RdfTerm::iri(GRAPH_SOURCE_CATALOG);
    for (predicate, object) in [
        (
            "http://www.w3.org/1999/02/22-rdf-syntax-ns#type",
            purrdf::RdfTerm::iri("https://blackcatinformatics.ca/gmeow/StageReceipt"),
        ),
        (
            "https://blackcatinformatics.ca/gmeow/receiptOfStage",
            purrdf::RdfTerm::iri("https://blackcatinformatics.ca/gmeow/stage-parse-sources"),
        ),
        (
            "https://blackcatinformatics.ca/gmeow/receiptDigest",
            purrdf::RdfTerm::literal(purrdf::RdfLiteral::simple(catalog.identity())),
        ),
    ] {
        let mut quad = purrdf::RdfQuad::new(subject.clone(), predicate, object);
        quad.graph_name = Some(graph.clone());
        builder.push_owned_quad(&quad);
    }
    let dataset = builder
        .freeze()
        .map_err(|error| stage_error(error.to_string()))?;
    let mut bundle =
        crate::bundle::bundle_from_artifacts_over(dataset, Default::default(), Default::default());
    let pin = bundle.graph_digest(GRAPH_SOURCE_CATALOG);
    bundle
        .pin_handle(
            GRAPH_SOURCE_CATALOG,
            PipelineHandle::SourceCatalog(catalog.clone()),
            pin,
        )
        .map_err(|error| stage_error(error.to_string()))?;
    Ok(StageProduct::from_bundle(STAGE_ID, Arc::new(bundle)))
}

#[path = "parse_sources.document_cache_tests.rs"]
#[cfg(test)]
mod document_cache_tests;

#[cfg(test)]
#[path = "parse_sources_test_support.rs"]
mod test_support;
#[cfg(test)]
pub(crate) use test_support::synthetic_product;
