// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Pipeline diagnostic kinds.
//!
//! Every DAG defect is a HARD failure surfaced *before* (or during) stage
//! execution (no-optionality). Each defect is a [`gmeow_errors::DiagKind`] minted
//! by [`gmeow_errors::define_diag_kind!`], so a raised diagnostic carries a stable
//! registered [`Code`](gmeow_errors::Code), a [`Grade`], and stays downcastable to
//! its typed value off the [`Diag`](gmeow_errors::Diag) source. There is no
//! hand-rolled error `enum`: the substrate is the single content-bound carrier.

use gmeow_errors::{Code, FindingCategory, Grade, Severity, Standpoint, define_diag_kind};

define_diag_kind! {
    /// An I/O error reading a source artifact or the cache.
    pub struct Io { message: String }
    code = "pipeline.io";
    grade = Grade::new(Severity::Error, FindingCategory::ModelingDisciplineViolation, Standpoint::Binding);
    message = "I/O error: {}", message;
}

define_diag_kind! {
    /// An RDF parse error reading the dogfooded DAG individuals.
    pub struct Parse { message: String }
    code = "pipeline.rdf.parse";
    grade = Grade::new(Severity::Error, FindingCategory::ModelingDisciplineViolation, Standpoint::Binding);
    message = "RDF parse error: {}", message;
}

define_diag_kind! {
    /// A (de)serialization failure on the on-disk cache: a corrupt `index.json`,
    /// a corrupt cached `StageProduct` blob, or a JSON encode failure on persist.
    /// Distinct from [`Parse`] (RDF) — these are JSON, not RDF, decode failures.
    pub struct Decode { message: String }
    code = "pipeline.cache.decode";
    grade = Grade::new(Severity::Error, FindingCategory::ModelingDisciplineViolation, Standpoint::Binding);
    message = "pipeline cache decode error: {}", message;
}

define_diag_kind! {
    /// A dogfooded RDF declaration carries a value outside its closed set
    /// (e.g. an unrecognized declaration literal). The RDF parsed cleanly — the
    /// *value* is invalid — so this is distinct from [`Parse`].
    pub struct InvalidDeclaration { message: String }
    code = "pipeline.declaration.invalid";
    grade = Grade::new(Severity::Error, FindingCategory::ModelingDisciplineViolation, Standpoint::Binding);
    message = "invalid gmeow declaration: {}", message;
}

define_diag_kind! {
    /// The DAG is structurally invalid: a cycle, a dangling `dataflowConsumes`
    /// reference, no `Sink`, or more than one `Sink`.
    pub struct InvalidDag { message: String }
    code = "pipeline.dag.invalid";
    grade = Grade::new(Severity::Error, FindingCategory::ModelingDisciplineViolation, Standpoint::Binding);
    message = "invalid pipeline DAG: {}", message;
}

define_diag_kind! {
    /// A `gmeow:stageImpl` key has no entry in the `STAGE_REGISTRY`.
    pub struct UnknownStageImpl { stage: String, impl_key: String }
    code = "pipeline.dag.unknown-stage-impl";
    grade = Grade::new(Severity::Error, FindingCategory::ModelingDisciplineViolation, Standpoint::Binding);
    message = "stage {} binds gmeow:stageImpl \"{}\" which is not in the STAGE_REGISTRY", stage, impl_key;
}

define_diag_kind! {
    /// The registry stage's `resources()` disagrees with the RDF
    /// `gmeow:requiresResource` declaration (Rust/RDF resource agreement).
    pub struct ResourceMismatch { stage: String, rdf: Vec<String>, rust: Vec<String> }
    code = "pipeline.contract.resource-mismatch";
    grade = Grade::new(Severity::Error, FindingCategory::ModelingDisciplineViolation, Standpoint::Binding);
    message = "stage {}: RDF gmeow:requiresResource {:?} disagrees with the Rust impl resources() {:?}", stage, rdf, rust;
}

define_diag_kind! {
    /// The registry stage's `consumed_entities()` disagrees with the RDF
    /// `gmeow:DataFlow` typed-dataflow declaration (Rust/RDF dataflow agreement).
    pub struct DataFlowMismatch {
        stage: String,
        rdf: Vec<(String, Vec<String>)>,
        rust: Vec<(String, Vec<String>)>,
    }
    code = "pipeline.contract.dataflow-mismatch";
    grade = Grade::new(Severity::Error, FindingCategory::ModelingDisciplineViolation, Standpoint::Binding);
    message = "stage {}: RDF gmeow:DataFlow typed entities {:?} disagree with the Rust impl consumed_entities() {:?}", stage, rdf, rust;
}

define_diag_kind! {
    /// The registry stage's `consumes()` disagrees with the RDF
    /// `dataflowConsumes` declaration (Rust/RDF consumes agreement).
    pub struct ConsumesMismatch { stage: String, rdf: Vec<String>, rust: Vec<String> }
    code = "pipeline.contract.consumes-mismatch";
    grade = Grade::new(Severity::Error, FindingCategory::ModelingDisciplineViolation, Standpoint::Binding);
    message = "stage {}: RDF dataflowConsumes {:?} disagrees with the Rust impl consumes() {:?}", stage, rdf, rust;
}

define_diag_kind! {
    /// The registry stage's `capabilities()` disagrees with the RDF
    /// `gmeow:hasCapability` declaration (Rust/RDF capability agreement).
    pub struct CapabilityMismatch { stage: String, rdf: Vec<String>, rust: Vec<String> }
    code = "pipeline.contract.capability-mismatch";
    grade = Grade::new(Severity::Error, FindingCategory::ModelingDisciplineViolation, Standpoint::Binding);
    message = "stage {}: RDF gmeow:hasCapability {:?} disagrees with the Rust impl capabilities() {:?}", stage, rdf, rust;
}

define_diag_kind! {
    /// A cached `StageProduct` failed its self-verifying digest recheck. The
    /// cache is never silently repaired (no-optionality).
    pub struct CacheMismatch { expected: String, actual: String }
    code = "pipeline.cache.mismatch";
    grade = Grade::new(Severity::Error, FindingCategory::ModelingDisciplineViolation, Standpoint::Binding);
    message = "pipeline cache digest mismatch: expected {}, got {}", expected, actual;
}

define_diag_kind! {
    /// A stage's `run` failed.
    pub struct StageFailed { stage: String, message: String }
    code = "pipeline.stage.failed";
    grade = Grade::new(Severity::Error, FindingCategory::ModelingDisciplineViolation, Standpoint::Binding);
    message = "stage {} failed: {}", stage, message;
}

/// The complete pipeline diagnostic-code catalog, in registration order. Every
/// [`DiagKind`](gmeow_errors::DiagKind) minted anywhere in the crate appears here
/// exactly once — [`register_all`] seeds them and the collision test proves the
/// code strings are distinct.
pub const PIPELINE_DIAG_CODES: &[&str] = &[
    Io::CODE,
    Parse::CODE,
    Decode::CODE,
    InvalidDeclaration::CODE,
    InvalidDag::CODE,
    UnknownStageImpl::CODE,
    ResourceMismatch::CODE,
    DataFlowMismatch::CODE,
    ConsumesMismatch::CODE,
    CapabilityMismatch::CODE,
    CacheMismatch::CODE,
    StageFailed::CODE,
    crate::transcode::UnknownCodec::CODE,
    crate::transcode::NonInvertibleSource::CODE,
    crate::transcode::UndecodableInput::CODE,
    crate::transcode::CodecError::CODE,
    crate::bundle_blobs::BundleParse::CODE,
    crate::bundle_blobs::BundleDecode::CODE,
    crate::bundle_blobs::BundleUntar::CODE,
    crate::bundle_blobs::BundleJson::CODE,
    crate::stages::rule_severity::UnknownRuleSeverity::CODE,
];

/// Eagerly intern every pipeline diagnostic code, seeding the process-wide code
/// registry before any `intern` against it. Idempotent (each `register()` is a
/// `LazyLock`), and interning is the single enumeration authority — a duplicate
/// code literal would collapse two kinds onto one handle, which the collision
/// test forbids.
pub fn register_all() -> Vec<Code> {
    vec![
        Io::register(),
        Parse::register(),
        Decode::register(),
        InvalidDeclaration::register(),
        InvalidDag::register(),
        UnknownStageImpl::register(),
        ResourceMismatch::register(),
        DataFlowMismatch::register(),
        ConsumesMismatch::register(),
        CapabilityMismatch::register(),
        CacheMismatch::register(),
        StageFailed::register(),
        crate::transcode::UnknownCodec::register(),
        crate::transcode::NonInvertibleSource::register(),
        crate::transcode::UndecodableInput::register(),
        crate::transcode::CodecError::register(),
        crate::bundle_blobs::BundleParse::register(),
        crate::bundle_blobs::BundleDecode::register(),
        crate::bundle_blobs::BundleUntar::register(),
        crate::bundle_blobs::BundleJson::register(),
        crate::stages::rule_severity::UnknownRuleSeverity::register(),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use gmeow_errors::intern_code;
    use std::collections::HashSet;

    #[test]
    fn every_pipeline_code_interns_with_no_collision() {
        let handles = register_all();
        // register_all() and the catalog enumerate the same kinds in the same order.
        assert_eq!(
            handles.len(),
            PIPELINE_DIAG_CODES.len(),
            "register_all() and PIPELINE_DIAG_CODES must enumerate the same kinds"
        );

        // Every catalogued code interns (register_all seeded the registry).
        for code in PIPELINE_DIAG_CODES {
            assert!(
                intern_code(code).is_ok(),
                "pipeline code `{code}` did not intern after register_all()"
            );
        }

        // No two kinds may share a code literal: distinct strings AND distinct
        // interned handles. A duplicate `code = "..."` would fail loudly here.
        let distinct_strings: HashSet<&&str> = PIPELINE_DIAG_CODES.iter().collect();
        assert_eq!(
            distinct_strings.len(),
            PIPELINE_DIAG_CODES.len(),
            "duplicate pipeline diagnostic code string detected"
        );
        let distinct_handles: HashSet<Code> = handles.iter().copied().collect();
        assert_eq!(
            distinct_handles.len(),
            handles.len(),
            "two pipeline diagnostic kinds interned to the same code handle"
        );
    }
}
