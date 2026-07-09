// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Conformance-harness diagnostic kinds.
//!
//! The native logic-conformance harness discovers cases, parses `profile.json` /
//! `corpus.json` / W3C manifests / TPTP SZS problems, drives the native engine
//! cores, compares produced artifacts against committed goldens, and vendors /
//! grades external corpora — each a HARD failure surface (no-optionality): a
//! malformed case anatomy, an invalid profile, an unreadable golden, a codec
//! error, an out-of-fragment external problem, or a filesystem read/write that
//! fails must surface as a typed diagnostic rather than a bare string. Each defect
//! is a [`DiagKind`](gmeow_errors::DiagKind) minted by
//! [`define_diag_kind!`](gmeow_errors::define_diag_kind) under the
//! `conformance.*` code namespace, so the harness reports on the shared substrate.
//!
//! Every kind carries a single `detail` string that preserves the authored
//! condition text verbatim (including any `case <id>:` / `<path>:` prefix the
//! producer keys on); discrimination is by code + grade, and the message is the
//! preserved detail.

use gmeow_errors::{Code, FindingCategory, Grade, Severity, Standpoint, define_diag_kind};

define_diag_kind! {
    /// A filesystem read/write/read-dir/create-dir failed while resolving a case,
    /// a golden, an external source, or a vendored output. The message carries the
    /// offending path alongside the underlying error text.
    pub struct Io { detail: String }
    code = "conformance.io";
    grade = Grade::new(Severity::Error, FindingCategory::ModelingDisciplineViolation, Standpoint::Binding);
    message = "{}", detail;
}

define_diag_kind! {
    /// A discovered directory is not a runnable case, or an external-corpus tree is
    /// missing a required artifact: no `input.logic.ttl`, no `source/problem.p`, no
    /// `source/model.ttl`, an empty corpus / ontology set, or a non-UTF-8 path name.
    pub struct CaseAnatomy { detail: String }
    code = "conformance.case.anatomy";
    grade = Grade::new(Severity::Error, FindingCategory::DataShapeViolation, Standpoint::Binding);
    message = "{}", detail;
}

define_diag_kind! {
    /// A `profile.json` is malformed: it does not parse, is not a JSON object,
    /// declares a retired / unknown key, or carries a field of the wrong type or a
    /// non-positive budget ceiling.
    pub struct ProfileInvalid { detail: String }
    code = "conformance.profile.invalid";
    grade = Grade::new(Severity::Error, FindingCategory::DataShapeViolation, Standpoint::Binding);
    message = "{}", detail;
}

define_diag_kind! {
    /// A vendored external corpus's `corpus.json` is malformed: it does not parse,
    /// is not a JSON object, declares an unknown key or lane, or is missing a
    /// required string field.
    pub struct CorpusInvalid { detail: String }
    code = "conformance.corpus.invalid";
    grade = Grade::new(Severity::Error, FindingCategory::DataShapeViolation, Standpoint::Binding);
    message = "{}", detail;
}

define_diag_kind! {
    /// A corpus declares a license that is REFERENCE_ONLY (or unknown), so it may
    /// not be vendored under `cases/external/` — only fetched live in the heavy lane.
    pub struct LicenseNotVendorable { detail: String }
    code = "conformance.corpus.license-not-vendorable";
    grade = Grade::new(Severity::Error, FindingCategory::ModelingDisciplineViolation, Standpoint::Binding);
    message = "{}", detail;
}

define_diag_kind! {
    /// A W3C test manifest (Turtle or RDF/XML) failed to parse through the native
    /// codecs. The underlying parser message is preserved verbatim.
    pub struct ManifestParse { detail: String }
    code = "conformance.manifest.parse";
    grade = Grade::new(Severity::Error, FindingCategory::ModelingDisciplineViolation, Standpoint::Binding);
    message = "{}", detail;
}

define_diag_kind! {
    /// A parsed W3C manifest is invalid: a recognized entry has no premise document,
    /// an inline RDF/XML premise literal is empty, or a manifest carries no
    /// entailment entries at all.
    pub struct ManifestInvalid { detail: String }
    code = "conformance.manifest.invalid";
    grade = Grade::new(Severity::Error, FindingCategory::DataShapeViolation, Standpoint::Binding);
    message = "{}", detail;
}

define_diag_kind! {
    /// A TPTP SZS status line is missing or carries no status token (neither the
    /// `% SZS status` result comment nor the distribution `% Status :` header).
    pub struct SzsStatus { detail: String }
    code = "conformance.szs.status";
    grade = Grade::new(Severity::Error, FindingCategory::DataShapeViolation, Standpoint::Binding);
    message = "{}", detail;
}

define_diag_kind! {
    /// A TPTP SZS status token is not one of the model-theoretically well-defined
    /// values the mapping table recognises.
    pub struct SzsUnknownStatus { detail: String }
    code = "conformance.szs.unknown-status";
    grade = Grade::new(Severity::Error, FindingCategory::DataShapeViolation, Standpoint::Binding);
    message = "{}", detail;
}

define_diag_kind! {
    /// World-scoping a premise dataset into N-Quads failed: the native N-Triples
    /// serialization failed, its output was not UTF-8, or a produced N-Triples line
    /// carries no trailing `.`.
    pub struct NquadsLowering { detail: String }
    code = "conformance.lower.nquads";
    grade = Grade::new(Severity::Error, FindingCategory::ModelingDisciplineViolation, Standpoint::Binding);
    message = "{}", detail;
}

define_diag_kind! {
    /// An RDF comparison surface failed a codec step: a serialized projection /
    /// materialized document would not parse, canonicalize, serialize, or decode as
    /// UTF-8. The underlying codec message is preserved verbatim.
    pub struct RdfCompare { detail: String }
    code = "conformance.compare.rdf";
    grade = Grade::new(Severity::Error, FindingCategory::ModelingDisciplineViolation, Standpoint::Binding);
    message = "{}", detail;
}

define_diag_kind! {
    /// A committed JSON golden failed to parse during the case diff.
    pub struct JsonRead { detail: String }
    code = "conformance.compare.json";
    grade = Grade::new(Severity::Error, FindingCategory::DataShapeViolation, Standpoint::Binding);
    message = "{}", detail;
}

define_diag_kind! {
    /// A per-case run failed: a malformed input, an unsound-contract firewall
    /// refusal, a native engine core (compile / certify / materialize / explain /
    /// query / consistency) error, or a latent-invariant guard tripping. The full
    /// authored condition (including the `case <id>:` prefix) is preserved.
    pub struct RunFailed { detail: String }
    code = "conformance.run.failed";
    grade = Grade::new(Severity::Error, FindingCategory::ModelingDisciplineViolation, Standpoint::Binding);
    message = "{}", detail;
}

define_diag_kind! {
    /// A canonical-JSON artifact could not be serialized (a `serde_json` failure on
    /// a produced profile / verdict / report value).
    pub struct Serialize { detail: String }
    code = "conformance.serialize";
    grade = Grade::new(Severity::Error, FindingCategory::ModelingDisciplineViolation, Standpoint::Binding);
    message = "{}", detail;
}

define_diag_kind! {
    /// An external-corpus vendor / grade step refused a case: a malformed source, an
    /// out-of-fragment construct, a native decision that disagrees with the external
    /// ground truth, a clean control that fired a discipline, or a soundness gate
    /// failure. Lane-A is agreeing-by-construction, so each such condition is a hard
    /// error, never silently vendored.
    pub struct Vendor { detail: String }
    code = "conformance.vendor";
    grade = Grade::new(Severity::Error, FindingCategory::ModelingDisciplineViolation, Standpoint::Binding);
    message = "{}", detail;
}

define_diag_kind! {
    /// An `ingest-external` / `conformance-report` invocation carried an invalid
    /// argument surface: an unknown flag, a flag missing its value, a mutually
    /// exclusive combination, or a mis-applied option.
    pub struct Cli { detail: String }
    code = "conformance.cli.args";
    grade = Grade::new(Severity::Error, FindingCategory::DataShapeViolation, Standpoint::Binding);
    message = "{}", detail;
}

/// The complete conformance diagnostic-code catalog, in registration order. Every
/// [`DiagKind`](gmeow_errors::DiagKind) minted in the crate appears here exactly
/// once — [`register_all`] seeds them and the collision test proves the code
/// strings are distinct.
pub const CONFORMANCE_DIAG_CODES: &[&str] = &[
    Io::CODE,
    CaseAnatomy::CODE,
    ProfileInvalid::CODE,
    CorpusInvalid::CODE,
    LicenseNotVendorable::CODE,
    ManifestParse::CODE,
    ManifestInvalid::CODE,
    SzsStatus::CODE,
    SzsUnknownStatus::CODE,
    NquadsLowering::CODE,
    RdfCompare::CODE,
    JsonRead::CODE,
    RunFailed::CODE,
    Serialize::CODE,
    Vendor::CODE,
    Cli::CODE,
];

/// Eagerly intern every conformance diagnostic code (idempotent).
pub fn register_all() -> Vec<Code> {
    vec![
        Io::register(),
        CaseAnatomy::register(),
        ProfileInvalid::register(),
        CorpusInvalid::register(),
        LicenseNotVendorable::register(),
        ManifestParse::register(),
        ManifestInvalid::register(),
        SzsStatus::register(),
        SzsUnknownStatus::register(),
        NquadsLowering::register(),
        RdfCompare::register(),
        JsonRead::register(),
        RunFailed::register(),
        Serialize::register(),
        Vendor::register(),
        Cli::register(),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use gmeow_errors::intern_code;
    use std::collections::HashSet;

    #[test]
    fn every_conformance_code_interns_with_no_collision() {
        let handles = register_all();
        assert_eq!(
            handles.len(),
            CONFORMANCE_DIAG_CODES.len(),
            "register_all() and CONFORMANCE_DIAG_CODES must enumerate the same kinds"
        );
        for code in CONFORMANCE_DIAG_CODES {
            assert!(
                intern_code(code).is_ok(),
                "conformance code `{code}` did not intern after register_all()"
            );
        }
        let distinct_strings: HashSet<&&str> = CONFORMANCE_DIAG_CODES.iter().collect();
        assert_eq!(
            distinct_strings.len(),
            CONFORMANCE_DIAG_CODES.len(),
            "duplicate conformance diagnostic code string detected"
        );
        let distinct_handles: HashSet<Code> = handles.iter().copied().collect();
        assert_eq!(distinct_handles.len(), handles.len());
    }
}
