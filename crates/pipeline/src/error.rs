// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Error type for the gmeow-pipeline crate.
//!
//! Convention (mirrors `gmeow-slice`): a hand-written `enum` with manual
//! `Display` + `std::error::Error` impls and `From<std::io::Error>` — no
//! `thiserror` dependency. Every DAG defect is a HARD failure surfaced *before*
//! any stage runs (no-optionality, #861).

/// All errors that can arise from loading, validating, scheduling, and running
/// a pipeline DAG.
#[derive(Debug)]
pub enum PipelineError {
    /// An I/O error reading a source artifact or the cache.
    Io(std::io::Error),
    /// An RDF parse error reading the dogfooded DAG individuals.
    Parse(String),
    /// The DAG is structurally invalid: a cycle, a dangling `dataflowConsumes`
    /// reference, no `Sink`, or more than one `Sink`.
    InvalidDag(String),
    /// A `gmeow:stageImpl` key has no entry in the `STAGE_REGISTRY`.
    UnknownStageImpl { stage: String, impl_key: String },
    /// The RDF `gmeow:carriesEngineLock` flag disagrees with the stage kind.
    /// `carriesEngineLock` is DERIVED (`== kind is Reason`), never independent —
    /// RDF and Rust cannot disagree (single source of truth, ETHOS one-path).
    EngineLockMismatch {
        stage: String,
        rdf: bool,
        derived: bool,
    },
    /// The registry stage's `consumes()` disagrees with the RDF
    /// `dataflowConsumes` declaration (Rust/RDF consumes agreement).
    ConsumesMismatch {
        /// The stage whose declarations disagree.
        stage: String,
        /// The dependency ids declared in RDF (`dataflowConsumes`), sorted.
        rdf: Vec<String>,
        /// The dependency ids the Rust impl declares via `consumes()`, sorted.
        rust: Vec<String>,
    },
    /// The registry stage's `kind()` disagrees with the RDF `gmeow:stageKind`.
    KindMismatch {
        /// The stage whose kind disagrees.
        stage: String,
        /// The RDF-declared kind tag.
        rdf: String,
        /// The Rust-declared kind tag.
        rust: String,
    },
    /// A cached `StageProduct` failed its self-verifying digest recheck. The
    /// cache is never silently repaired (no-optionality, #861 P2).
    CacheMismatch { expected: String, actual: String },
    /// A stage's `run` failed.
    Stage { stage: String, message: String },
}

impl std::fmt::Display for PipelineError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PipelineError::Io(e) => write!(f, "I/O error: {e}"),
            PipelineError::Parse(msg) => write!(f, "RDF parse error: {msg}"),
            PipelineError::InvalidDag(msg) => write!(f, "invalid pipeline DAG: {msg}"),
            PipelineError::UnknownStageImpl { stage, impl_key } => write!(
                f,
                "stage {stage} binds gmeow:stageImpl \"{impl_key}\" which is not in the STAGE_REGISTRY"
            ),
            PipelineError::EngineLockMismatch {
                stage,
                rdf,
                derived,
            } => write!(
                f,
                "stage {stage}: gmeow:carriesEngineLock={rdf} disagrees with the kind-derived value {derived} \
                 (carriesEngineLock must equal `kind is Reason`)"
            ),
            PipelineError::ConsumesMismatch { stage, rdf, rust } => write!(
                f,
                "stage {stage}: RDF dataflowConsumes {rdf:?} disagrees with the Rust impl consumes() {rust:?}"
            ),
            PipelineError::KindMismatch { stage, rdf, rust } => write!(
                f,
                "stage {stage}: RDF gmeow:stageKind {rdf} disagrees with the Rust impl kind() {rust}"
            ),
            PipelineError::CacheMismatch { expected, actual } => {
                write!(f, "pipeline cache digest mismatch: expected {expected}, got {actual}")
            }
            PipelineError::Stage { stage, message } => {
                write!(f, "stage {stage} failed: {message}")
            }
        }
    }
}

impl std::error::Error for PipelineError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            PipelineError::Io(e) => Some(e),
            _ => None,
        }
    }
}

impl From<std::io::Error> for PipelineError {
    fn from(e: std::io::Error) -> Self {
        PipelineError::Io(e)
    }
}
