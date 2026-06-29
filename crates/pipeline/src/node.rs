// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The pipeline node model: the [`Stage`] trait, the [`StageKind`] taxonomy, and
//! the in-memory [`StageInput`] / [`StageOutput`] / [`StageProduct`] handles a
//! stage exchanges (#861).
//!
//! A stage is re-cut for in-memory dataflow: it consumes the products of its
//! upstream stages (live handles, not re-parsed files) and emits one product.
//! Each resource a stage [`Stage::resources`] declares is held exclusively while
//! it runs — two stages competing for the same resource serialize; everything
//! else is parallel within its topological level. The reasoning stage requires
//! [`ENGINE_RESOURCE`] (the Nemo/Scryer engines are not concurrency-safe).

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;

use gmeow_rdf::provenance::DatasetProvenance;
use gmeow_rdf::{PipelineBundle, RdfDataset};

use crate::bundle::{
    bundle_artifact, bundle_artifacts, bundle_from_artifacts, bundle_from_artifacts_over,
    PipelineHandle,
};
use crate::error::PipelineError;

/// The GMEOW namespace prefix that every pipeline term lives under.
pub(crate) const GMEOW: &str = "https://blackcatinformatics.ca/gmeow/";

/// The `gmeow:engineResource` IRI: the process-wide reasoning engine, declared as
/// an exclusive [`Stage::resources`] requirement by the sole `Reason` stage. The
/// scheduler serializes any two stages requiring the same resource (the
/// declarative replacement for a hardcoded engine mutex).
pub const ENGINE_RESOURCE: &str = "https://blackcatinformatics.ca/gmeow/engineResource";

/// The kind of a pipeline stage. The kind drives scheduling treatment; serialization
/// is no longer kind-derived — a stage declares the shared resources it competes for
/// through [`Stage::resources`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum StageKind {
    /// Parse authored sources (statement-dsl, slices, imports) into a dataset.
    SourceLoad,
    /// A pure in-memory dataset → dataset transform (e.g. statements, mappings).
    Transform,
    /// Reasoning (native EL/DL, logic closure). Serialized under the engine lock
    /// because the underlying Nemo/Scryer engines hold process-wide locks.
    Reason,
    /// A validation pass (SHACL / lints) producing diagnostics, no new data.
    Validate,
    /// Documentation rendering over the composed dataset (#853).
    DocsRender,
    /// An independent output-format leaf folding one artifact into the bundle.
    ExportLeaf,
    /// The sole serialization exit — the gts narrow waist. Exactly one per DAG.
    Sink,
}

impl StageKind {
    /// The `gmeow:StageKind` individual IRI for this kind.
    pub fn iri(self) -> &'static str {
        match self {
            StageKind::SourceLoad => "https://blackcatinformatics.ca/gmeow/kindSourceLoad",
            StageKind::Transform => "https://blackcatinformatics.ca/gmeow/kindTransform",
            StageKind::Reason => "https://blackcatinformatics.ca/gmeow/kindReason",
            StageKind::Validate => "https://blackcatinformatics.ca/gmeow/kindValidate",
            StageKind::DocsRender => "https://blackcatinformatics.ca/gmeow/kindDocsRender",
            StageKind::ExportLeaf => "https://blackcatinformatics.ca/gmeow/kindExportLeaf",
            StageKind::Sink => "https://blackcatinformatics.ca/gmeow/kindSink",
        }
    }

    /// A short, stable tag for diagnostics.
    pub fn tag(self) -> &'static str {
        match self {
            StageKind::SourceLoad => "source-load",
            StageKind::Transform => "transform",
            StageKind::Reason => "reason",
            StageKind::Validate => "validate",
            StageKind::DocsRender => "docs-render",
            StageKind::ExportLeaf => "export-leaf",
            StageKind::Sink => "sink",
        }
    }

    /// Resolve a `gmeow:StageKind` individual IRI to a kind.
    pub fn from_iri(iri: &str) -> Option<Self> {
        let suffix = iri.strip_prefix(GMEOW)?;
        Some(match suffix {
            "kindSourceLoad" => StageKind::SourceLoad,
            "kindTransform" => StageKind::Transform,
            "kindReason" => StageKind::Reason,
            "kindValidate" => StageKind::Validate,
            "kindDocsRender" => StageKind::DocsRender,
            "kindExportLeaf" => StageKind::ExportLeaf,
            "kindSink" => StageKind::Sink,
            _ => return None,
        })
    }
}

/// The product of one stage: its id, the hex content digest of the value it
/// produced (the cache-key contribution downstream stages fold in — Merkle
/// composition, #861 P2), and the structured [`PipelineBundle`] it emitted.
///
/// # The carrier (#1132 C4)
///
/// The carrier is an [`Arc<PipelineBundle<PipelineHandle>>`]: the frozen RDF
/// dataset + lookaside + content-addressed blob store + provenance + typed-handle
/// lane. The pre-C4 named byte artifacts (logical path → bytes) ride the bundle's
/// byte-artifact lane (see [`crate::bundle`]); `gts_compose` / `gts_sink` fold the
/// upstream lane into the one bundle (#861 P3/P4). C2/C3/C5 progressively replace
/// the byte reads with dataset/lane reads and retire the lane per stage.
///
/// `digest` is the cache key: for a freshly produced bundle it is
/// `bundle.digest().to_hex()` (the handle-excluded content fold), so `combined()`
/// over stages stays an order-independent Merkle fold; abstract/test products may
/// carry an explicit digest decoupled from the (empty) carrier.
#[derive(Debug, Clone)]
pub struct StageProduct {
    /// The id of the stage that produced this.
    pub stage_id: String,
    /// The hex SHA-256 digest of the produced value (content-addressed cache key).
    pub digest: String,
    /// The structured carrier this stage emitted: the frozen dataset, lookaside
    /// (including the byte-artifact lane), blob store, provenance, and handle lane.
    pub bundle: Arc<PipelineBundle<PipelineHandle>>,
}

impl StageProduct {
    /// Construct an artifact-free product with an explicit digest (abstract
    /// stages / tests). Real transform stages use [`Self::from_artifacts`].
    ///
    /// The carrier is an empty bundle; the explicit `digest` is the cache key
    /// (decoupled from the empty carrier so existing abstract stages keep their
    /// declared digest).
    pub fn new(stage_id: impl Into<String>, digest: impl Into<String>) -> Self {
        Self {
            stage_id: stage_id.into(),
            digest: digest.into(),
            bundle: Arc::new(bundle_from_artifacts(
                BTreeMap::new(),
                DatasetProvenance::new(),
            )),
        }
    }

    /// Construct a product from emitted named byte artifacts; they ride the
    /// bundle's byte-artifact lane and the digest is the bundle's content fold.
    pub fn from_artifacts(
        stage_id: impl Into<String>,
        artifacts: BTreeMap<String, Vec<u8>>,
    ) -> Self {
        let bundle = bundle_from_artifacts(artifacts, DatasetProvenance::new());
        Self::from_bundle(stage_id, Arc::new(bundle))
    }

    /// Construct a product from named byte artifacts riding over an explicit
    /// backing `dataset` (the lane travels alongside the frozen graph).
    pub fn from_artifacts_over(
        stage_id: impl Into<String>,
        dataset: Arc<RdfDataset>,
        artifacts: BTreeMap<String, Vec<u8>>,
    ) -> Self {
        let bundle = bundle_from_artifacts_over(dataset, artifacts, DatasetProvenance::new());
        Self::from_bundle(stage_id, Arc::new(bundle))
    }

    /// Construct a product wrapping an already-assembled carrier; the digest is the
    /// bundle's content fold (handle lane excluded).
    pub fn from_bundle(
        stage_id: impl Into<String>,
        bundle: Arc<PipelineBundle<PipelineHandle>>,
    ) -> Self {
        let digest = bundle.digest().to_hex();
        Self {
            stage_id: stage_id.into(),
            digest,
            bundle,
        }
    }

    /// Borrow the structured carrier this product emitted.
    pub fn bundle(&self) -> &Arc<PipelineBundle<PipelineHandle>> {
        &self.bundle
    }

    /// Borrow the frozen RDF dataset this product's bundle carries.
    pub fn dataset(&self) -> &RdfDataset {
        self.bundle.dataset()
    }

    /// The bytes of one named byte-artifact-lane entry by logical path.
    pub fn artifact(&self, logical_path: &str) -> Option<&[u8]> {
        bundle_artifact(&self.bundle, logical_path)
    }

    /// The full `(logical_path → bytes)` map of this product's byte-artifact lane,
    /// sorted by path. The surface `run_full` writes / compares against committed.
    pub fn artifacts(&self) -> BTreeMap<String, Vec<u8>> {
        bundle_artifacts(&self.bundle)
    }
}

/// The input handed to a stage's `run`: the repo root and the products of every
/// stage it `consumes` (live, in-memory — never re-parsed from disk).
pub struct StageInput<'a> {
    /// The repository root the build operates over.
    pub root: &'a Path,
    /// Upstream products keyed by producing-stage id. A stage reads only the
    /// ids it declared in `consumes()`.
    pub upstream: &'a BTreeMap<String, StageProduct>,
}

/// The output a stage's `run` returns.
pub struct StageOutput {
    /// The single product this stage produced.
    pub product: StageProduct,
}

/// A pipeline stage: one node in the build DAG. The Rust impl is the executable
/// twin of a `gmeow:PipelineStage` individual; the loader binds them by
/// `gmeow:stageImpl` and HARD-fails if their `kind` / `consumes` / `resources`
/// disagree.
pub trait Stage: Send + Sync {
    /// The stable stage id — matches the `gmeow:PipelineStage` individual.
    fn id(&self) -> &str;
    /// The stage kind (drives scheduling treatment).
    fn kind(&self) -> StageKind;
    /// The ids of the upstream stages this stage consumes, sorted.
    fn consumes(&self) -> &[String];
    /// The IRIs of the shared resources this stage must hold exclusively while it
    /// runs (`gmeow:requiresResource`), sorted. Two stages declaring the same
    /// resource serialize; the default is none (parallel-eligible). The reasoning
    /// stage declares [`ENGINE_RESOURCE`]. The loader HARD-fails if this disagrees
    /// with the RDF `gmeow:requiresResource` declaration.
    fn resources(&self) -> &[String] {
        &[]
    }
    /// The typed dataflow (`gmeow:DataFlow` reified edges): for each upstream producer
    /// the stage reads only SPECIFIC named-graph entities from, a
    /// `(producer_id, sorted entity-graph IRIs)` pair, the whole list sorted by
    /// producer id. A producer ABSENT here (the default for every producer) is a
    /// WHOLE-PRODUCT dependency — the cache key folds that producer's entire bundle
    /// digest, so any change re-runs this stage. A producer PRESENT narrows the
    /// dependency to those named graphs' content digests, so a change to a graph this
    /// stage does NOT read no longer re-runs it (artifact-level incremental rebuild).
    ///
    /// Narrowing is a CORRECTNESS ASSERTION: declare an entity set only when the stage
    /// provably reads nothing else from that producer's product — a too-small set would
    /// serve a stale build. The loader HARD-fails if this disagrees with the RDF
    /// `gmeow:DataFlow` declaration (single source of truth).
    fn consumed_entities(&self) -> &[(String, Vec<String>)] {
        &[]
    }
    /// A version string folded into the cache key; bump to invalidate this
    /// stage's cached products when its logic changes.
    fn impl_version(&self) -> &str;
    /// The RAW source files this stage reads directly from disk that are NOT
    /// reflected in any upstream product digest (e.g. an export leaf that reads
    /// `metadata/references.ttl`, the eval corpus, or the slice manifests rather
    /// than consuming the composed fold). Their CONTENT is folded into the cache
    /// key so a source change busts the cache — the cache-soundness guarantee for
    /// non-`SourceLoad` stages that legitimately consume nothing (#861, #863).
    ///
    /// The default is empty: a stage whose every input is an upstream product
    /// (Merkle-composed) or whose file reads are already covered by a consumed
    /// `SourceLoad`/`stage-snapshot` product declares nothing here. Paths are
    /// resolved relative to the repo root; the scheduler reads each file's bytes
    /// and folds a content digest into the key (a missing file HARD-fails).
    fn input_files(&self, _root: &Path) -> Result<Vec<std::path::PathBuf>, PipelineError> {
        Ok(Vec::new())
    }
    /// Execute the stage over its upstream products.
    fn run(&self, input: StageInput<'_>) -> Result<StageOutput, PipelineError>;
}
