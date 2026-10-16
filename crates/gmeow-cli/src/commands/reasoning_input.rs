// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Native RDF input admission for the installed reasoning commands.
//! The shared Diag owns every refusal, its input receipts and its original cause.

use std::path::Path;
use std::sync::Arc;

use gmeow_errors::{Diag, FindingCategory, Grade, Severity, Standpoint};

use super::reasoning_report::InputIdentity;

pub(super) struct LoadedReasoningInput {
    pub dataset: Arc<purrdf::RdfDataset>,
    pub identity: InputIdentity,
}

/// Evidence payload carried inside Diag, never a parallel function error type.
#[derive(Debug, Clone)]
pub(crate) struct ReasoningInputFailure {
    pub code: &'static str,
    pub operation: &'static str,
    pub identities: Vec<InputIdentity>,
    pub cause: Arc<crate::error::PreservedDiagnostic>,
}

gmeow_errors::impl_diag_kind! {
    ReasoningInputFailure;
    code = "gmeow-cli.reasoning.input-failed";
    grade = Grade::new(
        Severity::Error,
        FindingCategory::DataShapeViolation,
        Standpoint::Binding,
    );
}

impl std::fmt::Display for ReasoningInputFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}: {}", self.operation, self.cause)
    }
}

impl std::error::Error for ReasoningInputFailure {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(self.cause.as_ref())
    }
}

fn refused(
    path: &Path,
    operation: &'static str,
    code: &'static str,
    identities: Vec<InputIdentity>,
    cause: Diag,
) -> Diag {
    Diag::of_kind(ReasoningInputFailure {
        code,
        operation,
        identities,
        cause: Arc::new(crate::error::PreservedDiagnostic(cause)),
    })
    .with_focus(path.display().to_string())
}

pub(super) fn load_reasoning_input(
    path: &Path,
    role: &'static str,
    operation: &'static str,
) -> gmeow_errors::Result<LoadedReasoningInput> {
    let bytes = std::fs::read(path).map_err(|error| {
        refused(
            path,
            operation,
            "INPUT_READ_FAILED",
            Vec::new(),
            Diag::from(error).with_context("read RDF reasoning input"),
        )
    })?;
    let media = match path.extension().and_then(|extension| extension.to_str()) {
        Some(extension) => extension.to_lowercase(),
        None => {
            return Err(refused(
                path,
                operation,
                "INPUT_SYNTAX_UNKNOWN",
                vec![InputIdentity::from_bytes(
                    path, role, "unknown", None, &bytes,
                )],
                Diag::of_kind(crate::error::SourceReadFailed {
                    detail: format!(
                        "cannot infer RDF syntax for {} (no extension); expected one of \
                     .ttl/.nt/.nq/.rdf/.owl/.xml/.trig",
                        path.display()
                    ),
                }),
            ));
        }
    };
    let absolute = std::path::absolute(path).map_err(|error| {
        refused(
            path,
            operation,
            "INPUT_BASE_FAILED",
            vec![InputIdentity::from_bytes(
                path,
                role,
                media.clone(),
                None,
                &bytes,
            )],
            Diag::from(error).with_context("resolve RDF reasoning input base"),
        )
    })?;
    let base = Some(format!("file://{}", absolute.display()));
    let identity = InputIdentity::from_bytes(path, role, media.clone(), base.clone(), &bytes);
    let dataset = purrdf::parse_dataset(&bytes, &media, base.as_deref()).map_err(|error| {
        refused(
            path,
            operation,
            "INPUT_PARSE_FAILED",
            vec![identity.clone()],
            Diag::from(error).with_context(format!("parse RDF reasoning input as {media}")),
        )
    })?;
    Ok(LoadedReasoningInput { dataset, identity })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    /// An input refusal keeps its operation identity, registered grade and typed cause.
    fn input_refusal_retains_registered_grade_identity_and_original_diagnostic() {
        let cause = Diag::of_kind(crate::error::SourceReadFailed {
            detail: "selected input is unavailable".to_owned(),
        })
        .with_context("read selected theory");
        let cause_code = cause.code();
        let identity = InputIdentity::from_bytes(
            Path::new("theory.ttl"),
            "theory",
            "ttl",
            None,
            b"selected bytes",
        );
        let diag = refused(
            Path::new("theory.ttl"),
            "classify",
            "INPUT_READ_FAILED",
            vec![identity.clone()],
            cause,
        );
        assert_eq!(diag.code(), ReasoningInputFailure::register());
        assert_eq!(diag.grade().category, FindingCategory::DataShapeViolation);
        let payload = diag
            .downcast_ref::<ReasoningInputFailure>()
            .expect("typed refusal");
        assert_eq!(payload.code, "INPUT_READ_FAILED");
        assert_eq!(payload.operation, "classify");
        assert_eq!(
            payload.identities[0].content_digest,
            identity.content_digest
        );
        assert_eq!(payload.cause.0.code(), cause_code);
        assert!(
            payload
                .cause
                .0
                .downcast_ref::<crate::error::SourceReadFailed>()
                .is_some()
        );
        assert!(std::error::Error::source(payload).is_some());
    }
}
