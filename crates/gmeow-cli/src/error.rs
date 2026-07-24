// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Shippable-CLI diagnostic kinds.
//!
//! The `gmeow` front door drives the shared `purrdf` RDF pipeline, reads the
//! embedded bundle, and ingests user sources. Each failure surface is a HARD fail
//! (no-optionality) minted as a [`DiagKind`](gmeow_errors::DiagKind) by
//! [`define_diag_kind!`](gmeow_errors::define_diag_kind); the underlying
//! `purrdf`/bundle helpers still report `String`/`Display` errors, whose text is
//! preserved verbatim in the kind's `detail` at the crate boundary.

use gmeow_errors::{Code, FindingCategory, Grade, Severity, Standpoint, define_diag_kind};

define_diag_kind! {
    /// A shared RDF-pipeline step (fold, parse, project, serialize) failed.
    pub struct RdfPipelineFailed { detail: String }
    code = "gmeow-cli.rdf.pipeline-failed";
    grade = Grade::new(Severity::Error, FindingCategory::ModelingDisciplineViolation, Standpoint::Binding);
    message = "{}", detail;
}

define_diag_kind! {
    /// Reading a blob out of the embedded `gmeow.gts` bundle failed.
    pub struct BundleReadFailed { detail: String }
    code = "gmeow-cli.bundle.read-failed";
    grade = Grade::new(Severity::Error, FindingCategory::ModelingDisciplineViolation, Standpoint::Binding);
    message = "{}", detail;
}

define_diag_kind! {
    /// Reading or parsing the user-supplied source (file / stdin / GTS) failed.
    pub struct SourceReadFailed { detail: String }
    code = "gmeow-cli.source.read-failed";
    grade = Grade::new(Severity::Error, FindingCategory::DataShapeViolation, Standpoint::Binding);
    message = "{}", detail;
}

define_diag_kind! {
    /// A projected byte stream was not valid UTF-8 for a text output.
    pub struct OutputEncodingFailed { detail: String }
    code = "gmeow-cli.output.encoding-failed";
    grade = Grade::new(Severity::Error, FindingCategory::DataShapeViolation, Standpoint::Binding);
    message = "{}", detail;
}

define_diag_kind! {
    /// `gmeow describe` could not resolve the query to any bundle term (across all
    /// registered namespaces). The `detail` carries the backend's message, which
    /// includes any near-miss suggestions.
    pub struct DescribeUnresolved { detail: String }
    code = "gmeow-cli.describe.unresolved";
    grade = Grade::new(Severity::Error, FindingCategory::ModelingDisciplineViolation, Standpoint::Binding);
    message = "{}", detail;
}

define_diag_kind! {
    /// `gmeow describe` matched a bare local name in more than one namespace — a
    /// HARD fail (no silent `gmeow:` precedence). The `detail` lists the candidate
    /// CURIEs the caller must disambiguate between.
    pub struct DescribeAmbiguous { detail: String }
    code = "gmeow-cli.describe.ambiguous";
    grade = Grade::new(Severity::Error, FindingCategory::ModelingDisciplineViolation, Standpoint::Binding);
    message = "{}", detail;
}

define_diag_kind! {
    /// A `gmeow hybrid-query --purremb` selection input (a hex identity, an enum code,
    /// or the term kind) was malformed or unknown — a HARD fail (no silent
    /// reinterpretation). The `detail` carries the specific mismatch.
    pub struct PurrembSelectionInvalid { detail: String }
    code = "gmeow-cli.hybrid-query.purremb-selection";
    grade = Grade::new(Severity::Error, FindingCategory::DataShapeViolation, Standpoint::Binding);
    message = "{}", detail;
}

/// The complete shippable-CLI diagnostic-code catalog, in registration order —
/// the kinds minted here plus the `explain`-command kinds defined beside their
/// use site. (Consumed by the collision test; the running CLI reaches its kinds
/// directly.)
#[allow(dead_code)]
pub const GMEOW_CLI_DIAG_CODES: &[&str] = &[
    RdfPipelineFailed::CODE,
    BundleReadFailed::CODE,
    SourceReadFailed::CODE,
    OutputEncodingFailed::CODE,
    DescribeUnresolved::CODE,
    DescribeAmbiguous::CODE,
    PurrembSelectionInvalid::CODE,
    crate::commands::UnknownExplainTarget::CODE,
    crate::commands::ExplainWalkFailed::CODE,
];

/// Eagerly intern every shippable-CLI diagnostic code (idempotent). Reachable for
/// startup seeding and exercised by the collision test.
#[allow(dead_code)]
pub fn register_all() -> Vec<Code> {
    vec![
        RdfPipelineFailed::register(),
        BundleReadFailed::register(),
        SourceReadFailed::register(),
        OutputEncodingFailed::register(),
        DescribeUnresolved::register(),
        DescribeAmbiguous::register(),
        PurrembSelectionInvalid::register(),
        crate::commands::UnknownExplainTarget::register(),
        crate::commands::ExplainWalkFailed::register(),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use gmeow_errors::intern_code;
    use std::collections::HashSet;

    #[test]
    fn every_gmeow_cli_code_interns_with_no_collision() {
        let handles = register_all();
        assert_eq!(
            handles.len(),
            GMEOW_CLI_DIAG_CODES.len(),
            "register_all() and GMEOW_CLI_DIAG_CODES must enumerate the same kinds"
        );
        for code in GMEOW_CLI_DIAG_CODES {
            assert!(
                intern_code(code).is_ok(),
                "gmeow-cli code `{code}` did not intern after register_all()"
            );
        }
        let distinct_strings: HashSet<&&str> = GMEOW_CLI_DIAG_CODES.iter().collect();
        assert_eq!(
            distinct_strings.len(),
            GMEOW_CLI_DIAG_CODES.len(),
            "duplicate gmeow-cli diagnostic code string detected"
        );
        let distinct_handles: HashSet<Code> = handles.iter().copied().collect();
        assert_eq!(distinct_handles.len(), handles.len());
    }
}
