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

/// The product of one stage: its id plus the hex content digest of the value it
/// produced. The digest is the cache key contribution downstream stages fold in
/// (Merkle composition, #861 P2); richer dataset / bundle handles are attached
/// to this struct as later parcels wire the in-memory dataflow.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StageProduct {
    /// The id of the stage that produced this.
    pub stage_id: String,
    /// The hex SHA-256 digest of the produced value (content-addressed).
    pub digest: String,
}

impl StageProduct {
    /// Construct a product for `stage_id` with the given hex digest.
    pub fn new(stage_id: impl Into<String>, digest: impl Into<String>) -> Self {
        Self {
            stage_id: stage_id.into(),
            digest: digest.into(),
        }
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
    /// Execute the stage over its upstream products.
    fn run(&self, input: StageInput<'_>) -> Result<StageOutput, PipelineError>;
}
