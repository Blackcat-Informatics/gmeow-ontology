// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! `gmeow-errors` — the GMEOW diagnostic substrate.
//!
//! The data model and renderers are Rust-owned so every developer tool can
//! project the same findings to terminal text, JSON, SARIF, and HTML without
//! duplicating output logic. Python bindings are kept in the optional `py` module; the model and
//! render modules are PyO3-free.
//!
//! [`grade`] holds the `Grade` bilattice and the single [`gate`]
//! policy morphism that decides fatality — two independent orderings (truth and
//! knowledge) over one carrier, so severity merges by lattice join (order-free)
//! and contradictory evidence surfaces as a Belnap glut rather than an
//! overwrite.

/// Render-test snapshot helper (U5): a thin wrapper over
/// `insta::assert_snapshot!` so every renderer golden goes through one
/// substrate-owned entry point. The wrapper forwards its tokens verbatim, so
/// the auto-derived snapshot name and rendered body are byte-identical to a
/// direct `insta::assert_snapshot!` call.
#[macro_export]
macro_rules! assert_diag_snapshot {
    ($($tokens:tt)*) => {
        ::insta::assert_snapshot!($($tokens)*)
    };
}

pub mod code;
pub mod dag;
pub mod diag;
pub mod error;
pub mod grade;
pub mod ledger;
pub mod lower;
pub mod model;
pub mod project;
pub mod rdf;
pub mod render;

/// The crate result alias: an error defaults to [`Diag`].
pub type Result<T, E = diag::Diag> = std::result::Result<T, E>;

pub use code::{Code, CodeRegistry, UnknownCode, intern_code, register_code, seed_codes};
pub use dag::{DagError, DagNode, walk};
pub use diag::{
    Advice, Diag, DiagInner, DiagKind, DiagRef, DiagSink, Focus, Label, PipelineLocus, ResultExt,
    ResultIterExt, Slot, SourceContext, StageId, TermRole,
};
pub use grade::{
    Belnap, Blocking, BoundedLattice, GateVerdict, Grade, GradeMerge, Standpoint, gate,
};
pub use ledger::{
    DiagFingerprint, DiagLedger, DiagNode, Observation, SerFrame, SerLocation, fingerprint_iri,
};
pub use model::{
    DiagnosticAttribution, Finding, FindingCategory, Location, RelatedLabel, Report, Rule, Severity,
};
pub use rdf::severity_from_rdf;
