// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Developer-CLI diagnostic kinds.
//!
//! `gmeow-dev` drives the regeneration pipeline over the working-tree snapshot:
//! the shared `purrdf` RDF pipeline, bundle-blob reads, source ingestion, native
//! reasoning / logic-query resolution, and the feedback-bundle fold. Each
//! failure surface is a HARD fail (no-optionality)
//! minted as a [`DiagKind`](gmeow_errors::DiagKind) by
//! [`define_diag_kind!`](gmeow_errors::define_diag_kind). The underlying
//! pipeline/validate helpers still report `String`/`Display` errors, whose text
//! is preserved verbatim in the kind's `detail` at the crate boundary — the
//! `Display`-generic constructors below are the single conversion seam.

use std::fmt::Display;

use gmeow_errors::{Code, Diag, FindingCategory, Grade, Severity, Standpoint, define_diag_kind};

define_diag_kind! {
    /// A shared RDF-pipeline step (fold, parse, project, serialize) failed.
    pub struct RdfPipelineFailed { detail: String }
    code = "gmeow-dev-cli.rdf.pipeline-failed";
    grade = Grade::new(Severity::Error, FindingCategory::ModelingDisciplineViolation, Standpoint::Binding);
    message = "{}", detail;
}

define_diag_kind! {
    /// Reading a blob out of the working-tree `gmeow.gts` snapshot failed.
    pub struct BundleReadFailed { detail: String }
    code = "gmeow-dev-cli.bundle.read-failed";
    grade = Grade::new(Severity::Error, FindingCategory::ModelingDisciplineViolation, Standpoint::Binding);
    message = "{}", detail;
}

define_diag_kind! {
    /// Reading or parsing a user-supplied source (file / stdin / GTS) failed.
    pub struct SourceReadFailed { detail: String }
    code = "gmeow-dev-cli.source.read-failed";
    grade = Grade::new(Severity::Error, FindingCategory::DataShapeViolation, Standpoint::Binding);
    message = "{}", detail;
}

define_diag_kind! {
    /// A projected byte stream was not valid UTF-8 for a text output.
    pub struct OutputEncodingFailed { detail: String }
    code = "gmeow-dev-cli.output.encoding-failed";
    grade = Grade::new(Severity::Error, FindingCategory::DataShapeViolation, Standpoint::Binding);
    message = "{}", detail;
}

define_diag_kind! {
    /// Native reasoning failed, or the shipped `graph/reasoning` verdict is
    /// unusable (absent, unparsable, or minted under a different contract hash).
    pub struct ReasoningFailed { detail: String }
    code = "gmeow-dev-cli.reason.failed";
    grade = Grade::new(Severity::Error, FindingCategory::ModelingDisciplineViolation, Standpoint::Binding);
    message = "{}", detail;
}

define_diag_kind! {
    /// A native `logic:` query could not be resolved (load / parse / evaluation).
    pub struct LogicQueryFailed { detail: String }
    code = "gmeow-dev-cli.logic.query-failed";
    grade = Grade::new(Severity::Error, FindingCategory::ModelingDisciplineViolation, Standpoint::Binding);
    message = "{}", detail;
}

define_diag_kind! {
    /// Building or reading the self-describing feedback `.gts` bundle failed, or a
    /// folded gate surface hard-failed while re-running.
    pub struct FeedbackBundleFailed { detail: String }
    code = "gmeow-dev-cli.feedback.bundle-failed";
    grade = Grade::new(Severity::Error, FindingCategory::ModelingDisciplineViolation, Standpoint::Binding);
    message = "{}", detail;
}

define_diag_kind! {
    /// Refreshing a generated target artifact failed.
    pub struct TargetRefreshFailed { detail: String }
    code = "gmeow-dev-cli.project.target-refresh-failed";
    grade = Grade::new(Severity::Error, FindingCategory::ModelingDisciplineViolation, Standpoint::Binding);
    message = "{}", detail;
}

define_diag_kind! {
    /// A vendored-corpus `corpus.json` descriptor is unreadable, unparsable, or
    /// missing a required license-policy field.
    pub struct VendoredCorpusDescriptorInvalid { detail: String }
    code = "gmeow-dev-cli.gates.vendored-corpus-descriptor-invalid";
    grade = Grade::new(Severity::Error, FindingCategory::DataShapeViolation, Standpoint::Binding);
    message = "{}", detail;
}

define_diag_kind! {
    /// Unified synchronization could not acquire its worktree boundary or
    /// reconcile one of its owned output trees.
    pub struct SyncFailed { detail: String }
    code = "gmeow-dev-cli.sync.failed";
    grade = Grade::new(Severity::Error, FindingCategory::ModelingDisciplineViolation, Standpoint::Binding);
    message = "{}", detail;
}

/// The single conversion seam: fold any `Display` error (a `String`, a foreign
/// `Error`, or a `Diag`) into a typed developer-CLI [`Diag`]. One constructor per
/// kind keeps each call site a terse `.map_err(error::rdf)?`.
macro_rules! ctor {
    ($name:ident, $kind:ident) => {
        pub fn $name(e: impl Display) -> Diag {
            Diag::of_kind($kind {
                detail: e.to_string(),
            })
        }
    };
}

ctor!(rdf, RdfPipelineFailed);
ctor!(bundle, BundleReadFailed);
ctor!(source, SourceReadFailed);
ctor!(encoding, OutputEncodingFailed);
ctor!(reasoning, ReasoningFailed);
ctor!(logic, LogicQueryFailed);
ctor!(feedback, FeedbackBundleFailed);
ctor!(refresh, TargetRefreshFailed);
ctor!(vendored_corpus, VendoredCorpusDescriptorInvalid);
ctor!(sync, SyncFailed);

/// The complete developer-CLI diagnostic-code catalog, in registration order.
/// (Consumed by the collision test; the running CLI reaches its kinds directly.)
#[allow(dead_code)]
pub const GMEOW_DEV_CLI_DIAG_CODES: &[&str] = &[
    RdfPipelineFailed::CODE,
    BundleReadFailed::CODE,
    SourceReadFailed::CODE,
    OutputEncodingFailed::CODE,
    ReasoningFailed::CODE,
    LogicQueryFailed::CODE,
    FeedbackBundleFailed::CODE,
    TargetRefreshFailed::CODE,
    VendoredCorpusDescriptorInvalid::CODE,
    SyncFailed::CODE,
];

/// Eagerly intern every developer-CLI diagnostic code (idempotent). Reachable for
/// startup seeding and exercised by the collision test.
#[allow(dead_code)]
pub fn register_all() -> Vec<Code> {
    vec![
        RdfPipelineFailed::register(),
        BundleReadFailed::register(),
        SourceReadFailed::register(),
        OutputEncodingFailed::register(),
        ReasoningFailed::register(),
        LogicQueryFailed::register(),
        FeedbackBundleFailed::register(),
        TargetRefreshFailed::register(),
        VendoredCorpusDescriptorInvalid::register(),
        SyncFailed::register(),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use gmeow_errors::intern_code;
    use std::collections::HashSet;

    #[test]
    fn every_gmeow_dev_cli_code_interns_with_no_collision() {
        let handles = register_all();
        assert_eq!(
            handles.len(),
            GMEOW_DEV_CLI_DIAG_CODES.len(),
            "register_all() and GMEOW_DEV_CLI_DIAG_CODES must enumerate the same kinds"
        );
        for code in GMEOW_DEV_CLI_DIAG_CODES {
            assert!(
                intern_code(code).is_ok(),
                "gmeow-dev-cli code `{code}` did not intern after register_all()"
            );
        }
        let distinct_strings: HashSet<&&str> = GMEOW_DEV_CLI_DIAG_CODES.iter().collect();
        assert_eq!(
            distinct_strings.len(),
            GMEOW_DEV_CLI_DIAG_CODES.len(),
            "duplicate gmeow-dev-cli diagnostic code string detected"
        );
        let distinct_handles: HashSet<Code> = handles.iter().copied().collect();
        assert_eq!(distinct_handles.len(), handles.len());
    }
}
