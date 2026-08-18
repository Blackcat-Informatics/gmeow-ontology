// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Bundle-view diagnostic kinds.
//!
//! Every read-side defect is a HARD failure (no-optionality): a snapshot that will
//! not fold, a Turtle source that will not parse, a SPARQL request that is not the
//! SELECT the caller demanded, a flat-export projection that cannot be built. Each
//! is a [`DiagKind`](gmeow_errors::DiagKind) minted by
//! [`define_diag_kind!`](gmeow_errors::define_diag_kind) under the `bundle-view.*`
//! code namespace, so the read side reports on the same content-bound substrate as
//! every other crate rather than raising bare strings.
//!
//! This crate is a LEAF: it has no stages, so there is deliberately no
//! `StageFailed`-shaped kind here. A renderer failure raises [`Export`]; the
//! pipeline stage driving the renderer lifts that `Diag` unchanged.
//!
//! The four bundle-blob kinds ([`BundleParse`](crate::bundle_blobs::BundleParse) and
//! friends) are declared next to the fold they guard, in
//! [`crate::bundle_blobs`], and enumerated here — [`BUNDLE_VIEW_DIAG_CODES`] and
//! [`register_all`] are the crate's single, complete catalog.

use gmeow_errors::{Code, FindingCategory, Grade, Severity, Standpoint, define_diag_kind};

define_diag_kind! {
    /// An I/O error reading a source artifact from disk (a Turtle source handed to
    /// the native query substrate, a materialized `gmeow.gts`).
    pub struct Io { message: String }
    code = "bundle-view.io";
    grade = Grade::new(Severity::Error, FindingCategory::ModelingDisciplineViolation, Standpoint::Binding);
    message = "I/O error: {}", message;
    failure_class = "https://blackcatinformatics.ca/gmeow/BundleArtifactUnreadable";
}

define_diag_kind! {
    /// An RDF parse or query defect on the read side: a syntactically invalid Turtle
    /// source, a SPARQL evaluation failure, or a query whose result form is not the
    /// one the caller requires.
    pub struct Parse { message: String }
    code = "bundle-view.rdf.parse";
    grade = Grade::new(Severity::Error, FindingCategory::ModelingDisciplineViolation, Standpoint::Binding);
    message = "RDF parse error: {}", message;
    failure_class = "https://blackcatinformatics.ca/gmeow/BundleSourceUnparsable";
}

define_diag_kind! {
    /// A hard defect inside a flat-export projection (CSVW / SKOS / OBO Graphs /
    /// N-Quads / TriG): a configuration the projector refuses, or a term the encoding
    /// cannot carry. The projection is refused rather than degraded.
    pub struct Export { message: String }
    code = "bundle-view.export";
    grade = Grade::new(Severity::Error, FindingCategory::ModelingDisciplineViolation, Standpoint::Binding);
    message = "export projection error: {}", message;
    failure_class = "https://blackcatinformatics.ca/gmeow/FlatExportRefused";
}

/// The complete bundle-view diagnostic-code catalog, in registration order.
pub const BUNDLE_VIEW_DIAG_CODES: &[&str] = &[
    Io::CODE,
    Parse::CODE,
    Export::CODE,
    crate::bundle_blobs::BundleParse::CODE,
    crate::bundle_blobs::BundleDecode::CODE,
    crate::bundle_blobs::BundleUntar::CODE,
    crate::bundle_blobs::BundleJson::CODE,
];

/// Eagerly intern every bundle-view diagnostic code (idempotent).
pub fn register_all() -> Vec<Code> {
    vec![
        Io::register(),
        Parse::register(),
        Export::register(),
        crate::bundle_blobs::BundleParse::register(),
        crate::bundle_blobs::BundleDecode::register(),
        crate::bundle_blobs::BundleUntar::register(),
        crate::bundle_blobs::BundleJson::register(),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use gmeow_errors::intern_code;
    use std::collections::HashSet;

    #[test]
    fn every_bundle_view_code_interns_with_no_collision() {
        let handles = register_all();
        assert_eq!(
            handles.len(),
            BUNDLE_VIEW_DIAG_CODES.len(),
            "register_all() and BUNDLE_VIEW_DIAG_CODES must enumerate the same kinds"
        );
        for code in BUNDLE_VIEW_DIAG_CODES {
            assert!(
                intern_code(code).is_ok(),
                "bundle-view code `{code}` did not intern after register_all()"
            );
        }
        let distinct_strings: HashSet<&&str> = BUNDLE_VIEW_DIAG_CODES.iter().collect();
        assert_eq!(
            distinct_strings.len(),
            BUNDLE_VIEW_DIAG_CODES.len(),
            "duplicate bundle-view diagnostic code string detected"
        );
        let distinct_handles: HashSet<Code> = handles.iter().copied().collect();
        assert_eq!(
            distinct_handles.len(),
            handles.len(),
            "two bundle-view diagnostic kinds interned to the same code handle"
        );
    }
}
