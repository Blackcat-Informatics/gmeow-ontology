// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The pipeline node model: the [`Stage`] trait, the [`StageKind`] taxonomy, and
//! the in-memory [`StageInput`] / [`StageOutput`] / [`StageProduct`] handles a
//! stage exchanges (#861).
//!
//! A stage is re-cut for in-memory dataflow: it consumes the products of its
//! upstream stages (live handles, not re-parsed files) and emits one product.
//! The kind selects the scheduler's treatment — only [`StageKind::Reason`] runs
//! under the process-wide engine lock; everything else is parallel within its
//! topological level.

use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::error::PipelineError;

/// The GMEOW namespace prefix that every pipeline term lives under.
pub(crate) const GMEOW: &str = "https://blackcatinformatics.ca/gmeow/";

/// The kind of a pipeline stage. The kind drives scheduling and, crucially,
/// *derives* whether the stage carries the engine lock — the RDF
/// `gmeow:carriesEngineLock` flag is validated against this, never trusted
/// independently (single source of truth, #861).
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

    /// Whether a stage of this kind must run under the process-wide engine lock.
    /// This is the SINGLE source of truth for the lock; the RDF
    /// `gmeow:carriesEngineLock` flag is validated to equal this (#861).
    pub fn carries_engine_lock(self) -> bool {
        matches!(self, StageKind::Reason)
    }
}

/// The product of one stage: its id, the hex content digest of the value it
/// produced (the cache key contribution downstream stages fold in — Merkle
/// composition, #861 P2), and the named artifacts it emitted (logical path →
/// bytes). Transform/export stages carry their compiled outputs here; the
/// `gts_compose` / `gts_sink` stages fold every upstream artifact into the one
/// bundle (#861 P3/P4).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StageProduct {
    /// The id of the stage that produced this.
    pub stage_id: String,
    /// The hex SHA-256 digest of the produced value (content-addressed).
    pub digest: String,
    /// The named artifacts this stage emitted, by logical path (sorted).
    #[serde(default)]
    pub artifacts: std::collections::BTreeMap<String, Vec<u8>>,
}

impl StageProduct {
    /// Construct an artifact-free product with an explicit digest (abstract
    /// stages / tests). Real transform stages use [`Self::from_artifacts`].
    pub fn new(stage_id: impl Into<String>, digest: impl Into<String>) -> Self {
        Self {
            stage_id: stage_id.into(),
            digest: digest.into(),
            artifacts: BTreeMap::new(),
        }
    }

    /// Construct a product from emitted artifacts; the digest is derived from the
    /// sorted `(logical_path, content-digest)` pairs (order-independent).
    pub fn from_artifacts(
        stage_id: impl Into<String>,
        artifacts: BTreeMap<String, Vec<u8>>,
    ) -> Self {
        let mut hasher = Sha256::new();
        for (path, bytes) in &artifacts {
            hasher.update(path.as_bytes());
            hasher.update(b"\x1f");
            hasher.update(Sha256::digest(bytes));
            hasher.update(b"\x1e");
        }
        let digest = hex_lower(&hasher.finalize());
        Self {
            stage_id: stage_id.into(),
            digest,
            artifacts,
        }
    }

    /// The bytes of one emitted artifact by logical path.
    pub fn artifact(&self, logical_path: &str) -> Option<&[u8]> {
        self.artifacts.get(logical_path).map(Vec::as_slice)
    }
}

fn hex_lower(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{b:02x}"));
    }
    s
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
/// `gmeow:stageImpl` and HARD-fails if their `kind` / `consumes` disagree.
pub trait Stage: Send + Sync {
    /// The stable stage id — matches the `gmeow:PipelineStage` individual.
    fn id(&self) -> &str;
    /// The stage kind (drives scheduling + the engine-lock derivation).
    fn kind(&self) -> StageKind;
    /// The ids of the upstream stages this stage consumes, sorted.
    fn consumes(&self) -> &[String];
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
