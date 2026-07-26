// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Documentation-surface diagnostic kinds.
//!
//! The Rust documentation model reads GTS bundles, parses RDF/PO/XLIFF, and
//! rewrites authored Turtle for the i18n family — each a HARD failure surface
//! (no-optionality): a bundle that will not read, an RDF/PO parse defect, a
//! malformed Turtle escape, an inconsistent translation catalog, an unsupported
//! source/format token, or a filesystem write that fails must surface as a typed
//! diagnostic rather than a bare string. Each defect is a
//! [`DiagKind`](gmeow_errors::DiagKind) minted by
//! [`define_diag_kind!`](gmeow_errors::define_diag_kind) under the `docs.*` code
//! namespace, so the documentation surface reports on the shared substrate.

use gmeow_errors::{Code, FindingCategory, Grade, Severity, Standpoint, define_diag_kind};

define_diag_kind! {
    /// A GTS bundle could not be read or folded into a describe-ready graph. Its
    /// message (the reader diagnostic text) is preserved verbatim.
    pub struct GtsRead { detail: String }
    code = "docs.describe.gts-read";
    grade = Grade::new(Severity::Error, FindingCategory::ModelingDisciplineViolation, Standpoint::Binding);
    message = "{}", detail;
}

define_diag_kind! {
    /// A format token (name or media type) is not one of the RDF serializations the
    /// native codecs accept.
    pub struct RdfFormat { detail: String }
    code = "docs.i18n.rdf-format";
    grade = Grade::new(Severity::Error, FindingCategory::DataShapeViolation, Standpoint::Binding);
    message = "{}", detail;
}

define_diag_kind! {
    /// A native RDF parse over an i18n source (Turtle/N-Triples/…) failed. The
    /// underlying parser message is preserved verbatim.
    pub struct RdfParse { detail: String }
    code = "docs.i18n.rdf-parse";
    grade = Grade::new(Severity::Error, FindingCategory::ModelingDisciplineViolation, Standpoint::Binding);
    message = "{}", detail;
}

define_diag_kind! {
    /// A gettext PO/POT document is malformed: an invalid string token, a missing
    /// `msgid`, or a continuation line with no owning field.
    pub struct PoParse { detail: String }
    code = "docs.i18n.po-parse";
    grade = Grade::new(Severity::Error, FindingCategory::DataShapeViolation, Standpoint::Binding);
    message = "{}", detail;
}

define_diag_kind! {
    /// A Turtle literal carries a malformed escape sequence and cannot be decoded.
    pub struct TurtleUnescape { detail: String }
    code = "docs.i18n.turtle-unescape";
    grade = Grade::new(Severity::Error, FindingCategory::DataShapeViolation, Standpoint::Binding);
    message = "{}", detail;
}

define_diag_kind! {
    /// The translation catalog is inconsistent with the authored sources: multiple
    /// distinct English values for one key, an unknown term/predicate, a missing
    /// internal language-tag mapping, a malformed `msgctxt`, or a literal-rewrite
    /// conflict. `detail` carries the full authored condition (including any
    /// `conflict:` prefix the sync reconciler keys on).
    pub struct CatalogInconsistent { detail: String }
    code = "docs.i18n.catalog-inconsistent";
    grade = Grade::new(Severity::Error, FindingCategory::ModelingDisciplineViolation, Standpoint::Binding);
    message = "{}", detail;
}

define_diag_kind! {
    /// A filesystem read/write failed while the path context matters — the message
    /// carries the offending path alongside the underlying error text.
    pub struct FileIo { detail: String }
    code = "docs.i18n.file-io";
    grade = Grade::new(Severity::Error, FindingCategory::ModelingDisciplineViolation, Standpoint::Binding);
    message = "{}", detail;
}

define_diag_kind! {
    /// A source file handed to the English-sync engine is neither Markdown nor
    /// Turtle, so there is no reconciler for it.
    pub struct UnsupportedSource { detail: String }
    code = "docs.i18n.unsupported-source";
    grade = Grade::new(Severity::Error, FindingCategory::DataShapeViolation, Standpoint::Binding);
    message = "{}", detail;
}

/// The complete docs diagnostic-code catalog, in registration order.
pub const DOCS_DIAG_CODES: &[&str] = &[
    GtsRead::CODE,
    RdfFormat::CODE,
    RdfParse::CODE,
    PoParse::CODE,
    TurtleUnescape::CODE,
    CatalogInconsistent::CODE,
    FileIo::CODE,
    UnsupportedSource::CODE,
];

/// Eagerly intern every docs diagnostic code (idempotent).
pub fn register_all() -> Vec<Code> {
    vec![
        GtsRead::register(),
        RdfFormat::register(),
        RdfParse::register(),
        PoParse::register(),
        TurtleUnescape::register(),
        CatalogInconsistent::register(),
        FileIo::register(),
        UnsupportedSource::register(),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use gmeow_errors::intern_code;
    use std::collections::HashSet;

    #[test]
    fn every_docs_code_interns_with_no_collision() {
        let handles = register_all();
        assert_eq!(
            handles.len(),
            DOCS_DIAG_CODES.len(),
            "register_all() and DOCS_DIAG_CODES must enumerate the same kinds"
        );
        for code in DOCS_DIAG_CODES {
            assert!(
                intern_code(code).is_ok(),
                "docs code `{code}` did not intern after register_all()"
            );
        }
        let distinct_strings: HashSet<&&str> = DOCS_DIAG_CODES.iter().collect();
        assert_eq!(
            distinct_strings.len(),
            DOCS_DIAG_CODES.len(),
            "duplicate docs diagnostic code string detected"
        );
        let distinct_handles: HashSet<Code> = handles.iter().copied().collect();
        assert_eq!(distinct_handles.len(), handles.len());
    }
}
