// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Validation-host diagnostic kinds.
//!
//! The validation host discovers slices, parses Turtle / JSON / YAML data
//! graphs, decodes the `gmeow.gts` bundle, drives the native SHACL and reasoning
//! engines, and audits the repository layout — each a HARD failure surface
//! (no-optionality): an unreadable path, a malformed document, a bundle that will
//! not decode, an engine that will not run, or a self-description that is missing
//! a required field must surface as a typed diagnostic rather than a bare string.
//! Each defect is a [`DiagKind`](gmeow_errors::DiagKind) minted by
//! [`define_diag_kind!`](gmeow_errors::define_diag_kind) under the `validate.*`
//! code namespace, so the host reports on the shared substrate.
//!
//! Every kind carries a single `detail` string that preserves the authored
//! condition text verbatim (including any `<path>:` / `example <name>:` prefix
//! the producer keys on); discrimination is by code + grade, and the message is
//! the preserved detail.

use gmeow_errors::{Code, FindingCategory, Grade, Severity, Standpoint, define_diag_kind};

define_diag_kind! {
    /// A filesystem read/write/read-dir/create-dir/metadata/rename failed while
    /// resolving a fixture, a slice source, a manifest, a cache entry, or an
    /// armored key. The message carries the offending path alongside the
    /// underlying error text.
    pub struct Io { detail: String }
    code = "validate.io";
    grade = Grade::new(Severity::Error, FindingCategory::ModelingDisciplineViolation, Standpoint::Binding);
    message = "{}", detail;
}

define_diag_kind! {
    /// A source document failed to parse through the native codecs: a Turtle /
    /// N-Triples / JSON-LD data graph, a `Cargo.toml` manifest, a JSON Schema or
    /// instance, or a YAML instance. The underlying parser message (with any
    /// line/column locator) is preserved verbatim.
    pub struct Parse { detail: String }
    code = "validate.parse";
    grade = Grade::new(Severity::Error, FindingCategory::ModelingDisciplineViolation, Standpoint::Binding);
    message = "{}", detail;
}

define_diag_kind! {
    /// A produced artifact could not be serialized: an N-Triples / RDF
    /// serialization, a JSON (`serde_json`) value, or an in-memory RDF dataset
    /// that would not freeze into its immutable form.
    pub struct Serialize { detail: String }
    code = "validate.serialize";
    grade = Grade::new(Severity::Error, FindingCategory::ModelingDisciplineViolation, Standpoint::Binding);
    message = "{}", detail;
}

define_diag_kind! {
    /// The `gmeow.gts` bundle would not yield a data graph: a decode / segment
    /// read failed, a declared blob's bytes are missing or not valid UTF-8, or a
    /// dataset could not be flattened to its default graph or materialized.
    pub struct Dataset { detail: String }
    code = "validate.dataset";
    grade = Grade::new(Severity::Error, FindingCategory::ModelingDisciplineViolation, Standpoint::Binding);
    message = "{}", detail;
}

define_diag_kind! {
    /// A native validation engine step failed: reasoning over the data graph,
    /// contract resolution, coherence-certificate construction, a SHACL shape
    /// projection, class-membership materialization, or a SHACL validation run.
    pub struct Engine { detail: String }
    code = "validate.engine";
    grade = Grade::new(Severity::Error, FindingCategory::ModelingDisciplineViolation, Standpoint::Binding);
    message = "{}", detail;
}

define_diag_kind! {
    /// A slice-catalog surface failed while computing the merged-SHACL Merkle
    /// key: catalog discovery, ownership analysis, or the product-key computation.
    pub struct Catalog { detail: String }
    code = "validate.catalog";
    grade = Grade::new(Severity::Error, FindingCategory::ModelingDisciplineViolation, Standpoint::Binding);
    message = "{}", detail;
}

define_diag_kind! {
    /// The ontology self-description is malformed: a required literal or IRI is
    /// absent, a manifestation / work subject is missing or not an IRI, or a
    /// declared field carries the wrong shape.
    pub struct SelfDescription { detail: String }
    code = "validate.self-description";
    grade = Grade::new(Severity::Error, FindingCategory::DataShapeViolation, Standpoint::Binding);
    message = "{}", detail;
}

define_diag_kind! {
    /// A Crossref deposit input is invalid: a date that is not ISO-8601, or a
    /// required deposit field that is absent or ill-formed.
    pub struct Crossref { detail: String }
    code = "validate.crossref";
    grade = Grade::new(Severity::Error, FindingCategory::DataShapeViolation, Standpoint::Binding);
    message = "{}", detail;
}

define_diag_kind! {
    /// A Wikidata mapping surface is invalid: a row that is not a CURIE, an
    /// unknown prefix, a path without a UTF-8 stem, or a Wikidata API response
    /// that is missing its `success` flag / `entities` object or reports an error.
    pub struct Mapping { detail: String }
    code = "validate.mapping";
    grade = Grade::new(Severity::Error, FindingCategory::DataShapeViolation, Standpoint::Binding);
    message = "{}", detail;
}

define_diag_kind! {
    /// A language-tag map could not be built: two definitions disagree on the same
    /// tag, or a required tag component is absent.
    pub struct LanguageTag { detail: String }
    code = "validate.language-tag";
    grade = Grade::new(Severity::Error, FindingCategory::DataShapeViolation, Standpoint::Binding);
    message = "{}", detail;
}

define_diag_kind! {
    /// An RDF serialization format is not one the host supports at this seam
    /// (no known media type, or an unsupported serializer target).
    pub struct Format { detail: String }
    code = "validate.format";
    grade = Grade::new(Severity::Error, FindingCategory::DataShapeViolation, Standpoint::Binding);
    message = "{}", detail;
}

define_diag_kind! {
    /// An invocation carried an invalid argument surface: an empty required path
    /// list, or a numeric parameter that is not a non-negative finite value.
    pub struct Argument { detail: String }
    code = "validate.argument";
    grade = Grade::new(Severity::Error, FindingCategory::DataShapeViolation, Standpoint::Binding);
    message = "{}", detail;
}

/// The complete validation diagnostic-code catalog, in registration order. Every
/// [`DiagKind`](gmeow_errors::DiagKind) minted in the crate appears here exactly
/// once — [`register_all`] seeds them and the collision test proves the code
/// strings are distinct.
pub const VALIDATE_DIAG_CODES: &[&str] = &[
    Io::CODE,
    Parse::CODE,
    Serialize::CODE,
    Dataset::CODE,
    Engine::CODE,
    Catalog::CODE,
    SelfDescription::CODE,
    Crossref::CODE,
    Mapping::CODE,
    LanguageTag::CODE,
    Format::CODE,
    Argument::CODE,
];

/// Eagerly intern every validation diagnostic code (idempotent).
pub fn register_all() -> Vec<Code> {
    vec![
        Io::register(),
        Parse::register(),
        Serialize::register(),
        Dataset::register(),
        Engine::register(),
        Catalog::register(),
        SelfDescription::register(),
        Crossref::register(),
        Mapping::register(),
        LanguageTag::register(),
        Format::register(),
        Argument::register(),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use gmeow_errors::intern_code;
    use std::collections::HashSet;

    #[test]
    fn every_validate_code_interns_with_no_collision() {
        let handles = register_all();
        assert_eq!(
            handles.len(),
            VALIDATE_DIAG_CODES.len(),
            "register_all() and VALIDATE_DIAG_CODES must enumerate the same kinds"
        );
        for code in VALIDATE_DIAG_CODES {
            assert!(
                intern_code(code).is_ok(),
                "validate code `{code}` did not intern after register_all()"
            );
        }
        let distinct_strings: HashSet<&&str> = VALIDATE_DIAG_CODES.iter().collect();
        assert_eq!(
            distinct_strings.len(),
            VALIDATE_DIAG_CODES.len(),
            "duplicate validate diagnostic code string detected"
        );
        let distinct_handles: HashSet<Code> = handles.iter().copied().collect();
        assert_eq!(distinct_handles.len(), handles.len());
    }
}
