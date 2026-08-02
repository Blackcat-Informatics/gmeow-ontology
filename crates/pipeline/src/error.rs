// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Pipeline diagnostic kinds.
//!
//! Every DAG defect is a HARD failure surfaced *before* (or during) stage
//! execution (no-optionality). Each defect is a [`gmeow_errors::DiagKind`] minted
//! by [`gmeow_errors::define_diag_kind!`], so a raised diagnostic carries a stable
//! registered [`gmeow_errors::Code`], a [`gmeow_errors::Grade`], and stays
//! downcastable to its typed value off the [`gmeow_errors::Diag`] source. There is no
//! hand-rolled error `enum`: the substrate is the single content-bound carrier.

use gmeow_errors::{Code, FindingCategory, Grade, Severity, Standpoint, define_diag_kind};

/// The GTS-authorship transform defect, minted by the `gmeow-gts-profile` LEAF
/// crate: both the mandated emitter and the wire-level frame validator raise it,
/// and neither may depend back on `gmeow-pipeline`. Re-exported here so
/// `crate::error::Transform` keeps naming the one registered kind (code
/// `pipeline.transform`, unchanged), and enumerated in [`PIPELINE_DIAG_CODES`] /
/// [`register_all`] below by its owning path.
pub use gmeow_gts_profile::Transform;

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
    /// `gmeow:BuildDataFlow` typed-dataflow declaration (Rust/RDF dataflow agreement).
    pub struct DataFlowMismatch {
        stage: String,
        rdf: Vec<(String, Vec<String>)>,
        rust: Vec<(String, Vec<String>)>,
    }
    code = "pipeline.contract.dataflow-mismatch";
    grade = Grade::new(Severity::Error, FindingCategory::ModelingDisciplineViolation, Standpoint::Binding);
    message = "stage {}: RDF gmeow:BuildDataFlow typed entities {:?} disagree with the Rust impl consumed_entities() {:?}", stage, rdf, rust;
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
    /// The registry stage's `attaches_graphs()` / `attaches_blob_reps()` disagrees
    /// with the RDF `gmeow:attachesGraph` / `gmeow:attachesBlobRep` declaration
    /// (Rust/RDF attach-declaration agreement, proved at load time).
    pub struct AttachDeclMismatch {
        stage: String,
        lane: String,
        rdf: Vec<String>,
        rust: Vec<String>,
    }
    code = "pipeline.contract.attach-decl-mismatch";
    grade = Grade::new(Severity::Error, FindingCategory::ModelingDisciplineViolation, Standpoint::Binding);
    message = "stage {}: RDF {} {:?} disagrees with the Rust impl declaration {:?}", stage, lane, rdf, rust;
}

define_diag_kind! {
    /// A stage's ACTUAL run-time attach delta (the named graphs / content-identified
    /// blob-rep records its output product carries beyond its assembled input) diverges
    /// from its DECLARED attach set — either an undeclared attachment (attached but not
    /// declared) or an unfulfilled declaration (declared but not attached). Fires on
    /// BOTH the cache-hit and cache-miss paths (a stale cached product with drifted
    /// declarations must not sail through). A HARD FAIL — no optionality, no fallback.
    pub struct AttachDrift {
        stage: String,
        lane: String,
        attached_undeclared: Vec<String>,
        declared_unattached: Vec<String>,
    }
    code = "pipeline.contract.attach-drift";
    grade = Grade::new(Severity::Error, FindingCategory::ModelingDisciplineViolation, Standpoint::Binding);
    message = "stage {}: {} attach drift — attached-but-undeclared {:?}, declared-but-not-attached {:?}", stage, lane, attached_undeclared, declared_unattached;
}

define_diag_kind! {
    /// The `gmeow:fanoutExtracts` map read from the pipeline graph is not a bijection
    /// against the generated paths the superset gate reconstructs: a generated path with
    /// no `fanoutExtracts` row (an unmapped path — silently dropped from fanout), or a
    /// row whose path no reconstruction claims. A HARD FAIL (completeness), so promoting
    /// the path↔representative branches to data never trades a total match for a lossy
    /// lookup.
    pub struct FanoutBijection { message: String }
    code = "pipeline.contract.fanout-bijection";
    grade = Grade::new(Severity::Error, FindingCategory::ModelingDisciplineViolation, Standpoint::Binding);
    message = "gmeow:fanoutExtracts is not a bijection over the reconstructed paths: {}", message;
}

define_diag_kind! {
    /// The independently-authored expected-output inventory (`gmeow:expectsGeneratedOutput`,
    /// hand-written TTL in the pipeline slice) and the bundle's reconstructed projection
    /// disagree: a declared `generated/` output the bundle no longer produces (a deterministic
    /// carrier drop the two-generation determinism gate cannot see), or a derivable prefix
    /// family whose authored members do not exactly equal the members the carrier's
    /// reconstruction graphs yield. A HARD FAIL (completeness) — the authored inventory is a
    /// DIFFERENT source from the carrier's `files.keys()`, so a silent capability degradation
    /// (a dropped consumed output on a clean clone) is caught here, not hidden.
    pub struct ExpectedOutputMissing { message: String }
    code = "pipeline.contract.expected-output";
    grade = Grade::new(Severity::Error, FindingCategory::ModelingDisciplineViolation, Standpoint::Binding);
    message = "authored expected-output inventory and the reconstructed bundle disagree: {}", message;
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

define_diag_kind! {
    /// A hard defect raised while assembling or evaluating a scoreboard / acceptance
    /// gate (dataset build, SPARQL projection, corpus glob, or gate arithmetic).
    pub struct Scoreboard { message: String }
    code = "pipeline.scoreboard";
    grade = Grade::new(Severity::Error, FindingCategory::ModelingDisciplineViolation, Standpoint::Binding);
    message = "scoreboard error: {}", message;
}

define_diag_kind! {
    /// A hard defect raised by the dogfooding MCP server surface (snapshot decode,
    /// query dispatch, memory access, or transaction append).
    pub struct Mcp { message: String }
    code = "pipeline.mcp";
    grade = Grade::new(Severity::Error, FindingCategory::ModelingDisciplineViolation, Standpoint::Binding);
    message = "mcp error: {}", message;
}

define_diag_kind! {
    /// A consumer query matched a bare local name in more than one namespace on the
    /// MCP surface — a HARD fail (no silent namespace precedence), the twin of the
    /// shippable-CLI `gmeow-cli.describe.ambiguous`. Minted DISTINCT from the generic
    /// unknown-term [`Mcp`] so an ambiguous term is greppable as its own code. The
    /// message names the query and lists the sorted candidate CURIEs the caller must
    /// disambiguate between.
    pub struct McpAmbiguousTerm { message: String }
    code = "pipeline.mcp.ambiguous-term";
    grade = Grade::new(Severity::Error, FindingCategory::ModelingDisciplineViolation, Standpoint::Binding);
    message = "{}", message;
}

define_diag_kind! {
    /// A hard defect raised while projecting the GTS base graph into a lossy
    /// surface (flat-quad decode, namespace scan, or transpile).
    pub struct Projection { message: String }
    code = "pipeline.projection";
    grade = Grade::new(Severity::Error, FindingCategory::ModelingDisciplineViolation, Standpoint::Binding);
    message = "projection error: {}", message;
}

define_diag_kind! {
    /// A hard defect raised by the up-projection audit / corpus lane (lift-program
    /// build, SSSOM/EDOAL corpus parse, tier resolution, or object-property scan).
    pub struct UpProjection { message: String }
    code = "pipeline.up-projection";
    grade = Grade::new(Severity::Error, FindingCategory::ModelingDisciplineViolation, Standpoint::Binding);
    message = "up-projection error: {}", message;
}

define_diag_kind! {
    /// A hard defect raised by the lawful-put executor (rule build, lift, or the
    /// round-trip quad emission).
    pub struct Put { message: String }
    code = "pipeline.put";
    grade = Grade::new(Severity::Error, FindingCategory::ModelingDisciplineViolation, Standpoint::Binding);
    message = "put executor error: {}", message;
}

define_diag_kind! {
    /// A hard defect raised while hashing generator source or collecting generator
    /// metadata for the provenance manifest.
    pub struct Generator { message: String }
    code = "pipeline.generator";
    grade = Grade::new(Severity::Error, FindingCategory::ModelingDisciplineViolation, Standpoint::Binding);
    message = "generator registry error: {}", message;
}

define_diag_kind! {
    /// A hard defect raised while building or verifying a release snapshot
    /// (evidence assembly, blob replay, or the verify report).
    pub struct Release { message: String }
    code = "pipeline.release";
    grade = Grade::new(Severity::Error, FindingCategory::ModelingDisciplineViolation, Standpoint::Binding);
    message = "release error: {}", message;
}

define_diag_kind! {
    /// A JSON-Schema instance failed validation against an evals schema (a closed
    /// contract violation in the evals stage). The message is the jsonschema
    /// `validate()` first-line wording VERBATIM (no prefix) — the scorecard `notes`
    /// are byte-identical to the reference validator, so the diagnostic carries the
    /// raw message unadorned.
    pub struct EvalSchema { message: String }
    code = "pipeline.eval.schema";
    grade = Grade::new(Severity::Error, FindingCategory::ModelingDisciplineViolation, Standpoint::Binding);
    message = "{}", message;
}

define_diag_kind! {
    /// A stage reached for `stage-source-load`'s source-span table AFTER the
    /// drop-after-last-consumer point stripped it — i.e. a stage later than the last
    /// declared span-table consumer tried to read spans that no longer exist. The blob
    /// being ABSENT means it was dropped, so any later reader is a HARD FAIL: the drop
    /// level is COMPUTED as the max consumer level, so the real consumers
    /// (`stage-validate` / `stage-compile-logic`) always run before the drop and this can
    /// never fire spuriously — it fires only on a genuine after-drop read.
    pub struct SpanTableConsumedAfterDrop { detail: String }
    code = "pipeline.spans.consumed-after-drop";
    grade = Grade::new(Severity::Error, FindingCategory::ModelingDisciplineViolation, Standpoint::Binding);
    message = "source-span table read after drop-after-last-consumer: {}", detail;
}

define_diag_kind! {
    /// A hard defect raised by the diagnostic meta-fold — the reasoner meta pass that
    /// derives root-cause / cluster / cross-node-glut findings over the projected finding
    /// graph. Fires on a malformed source or finding graph, an authored meta-rule that fails
    /// to parse, or a chase failure (e.g. an unstratifiable program). A real defect in a
    /// REQUIRED input stops the fold — never a silent collapse to a byte-unchanged projection.
    pub struct MetaFold { message: String }
    code = "pipeline.meta-fold";
    grade = Grade::new(Severity::Error, FindingCategory::ModelingDisciplineViolation, Standpoint::Binding);
    message = "diagnostic meta-fold error: {}", message;
}

define_diag_kind! {
    /// A hard defect raised while measuring the documentation-distribution
    /// designs (`docs_measure`): a renderer failure, a missing upstream
    /// pipeline product, or a GTS-framing failure while computing a
    /// per-format L12 delta.
    pub struct DocsMeasure { message: String }
    code = "pipeline.docs-measure";
    grade = Grade::new(Severity::Error, FindingCategory::ModelingDisciplineViolation, Standpoint::Binding);
    message = "docs-measure error: {}", message;
}

define_diag_kind! {
    /// A hard defect raised while rendering the external documentation /
    /// serialization distributions or building the release-time DCAT
    /// distribution manifest (`docs_distribution`): a
    /// serializer failure, a malformed OKF member path, a missing bundled
    /// `dcat.rq` projection query, or a DCAT projection failure over the
    /// release-time distribution instance graph.
    pub struct DocsDistribution { message: String }
    code = "pipeline.docs-distribution";
    grade = Grade::new(Severity::Error, FindingCategory::ModelingDisciplineViolation, Standpoint::Binding);
    message = "docs-distribution error: {}", message;
}

// --- The medium axis's six named failure classes -----------------------------
//
// Each of these six kinds is the SOLE Rust producer of one `gmeow:Medium*`
// failure-class individual in `slices/core/gts/module.ttl`, bound to it by the
// `failure_class` clause. The binding is not decoration: the repo-static
// bijection gate (`crates/validate/src/repo_static.rs`) proves each declared IRI
// resolves to a real failure-class individual AND that every `gmeow:Medium*`
// failure class has exactly one producer, so a kind added without its ontology
// individual — or an individual minted with no kind to raise it — reds. Every one
// is a HARD FAIL: none of them ever authorizes a fallback to a dictionary-less
// decode, because a payload written through a primed medium is not readable at
// lower fidelity without its dictionary, it is not readable AT ALL.

define_diag_kind! {
    /// A payload representation the medium registry declares no medium for: the
    /// rep→medium assignment is TOTAL, so a rep with no row is a missing
    /// declaration, never permission to encode it baseline. Also raised when a
    /// declared dictionary's corpus selects nothing — an empty training set leaves
    /// the dictionary undeclared in the only sense that matters (no bytes).
    pub struct MediumUndeclaredDictionary { detail: String }
    code = "pipeline.medium.undeclared-dictionary";
    grade = Grade::new(Severity::Error, FindingCategory::ModelingDisciplineViolation, Standpoint::Binding);
    message = "medium: undeclared dictionary — {}", detail;
    failure_class = "https://blackcatinformatics.ca/gmeow/MediumUndeclaredDictionary";
}

define_diag_kind! {
    /// A declared dictionary that does not resolve to a registered
    /// `gmeow:CompressionDictionary` — an unknown id, an unknown version, or a
    /// dictionary the reader does not hold.
    pub struct MediumUnknownDictionary { detail: String }
    code = "pipeline.medium.unknown-dictionary";
    grade = Grade::new(Severity::Error, FindingCategory::ModelingDisciplineViolation, Standpoint::Binding);
    message = "medium: unknown dictionary — {}", detail;
    failure_class = "https://blackcatinformatics.ca/gmeow/MediumUnknownDictionary";
}

define_diag_kind! {
    /// A blob rep with no registered `gmeow:PayloadSchema`. There is no defaultable
    /// answer: an unregistered rep would decode as an unclassified blob with an
    /// UNDEFINED medium assignment, which is exactly the silent capability
    /// degradation no-optionality forbids.
    pub struct MediumUnknownSchema { detail: String }
    code = "pipeline.medium.unknown-schema";
    grade = Grade::new(Severity::Error, FindingCategory::ModelingDisciplineViolation, Standpoint::Binding);
    message = "medium: unknown payload schema — {}", detail;
    failure_class = "https://blackcatinformatics.ca/gmeow/MediumUnknownSchema";
}

define_diag_kind! {
    /// A digest a medium envelope carries disagrees with the bytes it describes —
    /// the frame content digest, the stratum digest against its named
    /// `gmeow:DigestStratum`, or a dictionary's content digest against the
    /// dictionary at hand. Refuses BEFORE any decode is attempted: priming against
    /// a different dictionary with the same id yields plausible-looking garbage
    /// rather than an error.
    pub struct MediumDigestMismatch { detail: String }
    code = "pipeline.medium.digest-mismatch";
    grade = Grade::new(Severity::Error, FindingCategory::ModelingDisciplineViolation, Standpoint::Binding);
    message = "medium: digest mismatch — {}", detail;
    failure_class = "https://blackcatinformatics.ca/gmeow/MediumDigestMismatch";
}

define_diag_kind! {
    /// A frame whose medium demands a reader capability
    /// (`gmeow:requiresReaderCapability`) that is not held — rsyncable framing or
    /// dictionary priming, both non-baseline. A hard fail for a producer, and for a
    /// reader the region is SURFACED through the existing `gmeow:OpaqueFrame` /
    /// `gmeow:opacityUnknownCodec` vocabulary rather than silently skipped.
    pub struct MediumOpaqueFrame { detail: String }
    code = "pipeline.medium.opaque-frame";
    grade = Grade::new(Severity::Error, FindingCategory::ModelingDisciplineViolation, Standpoint::Binding);
    message = "medium: opaque frame — {}", detail;
    failure_class = "https://blackcatinformatics.ca/gmeow/MediumOpaqueFrame";
}

define_diag_kind! {
    /// An emitted dictionary version no authored `gmeow:CompressionDictionary`
    /// still declares. Retiring a version is a DATA-DESTROYING change: every
    /// artifact already primed with it becomes undecodable, so this is never
    /// resolved by deleting the realization.
    pub struct MediumDictionaryRegression { detail: String }
    code = "pipeline.medium.dictionary-regression";
    grade = Grade::new(Severity::Error, FindingCategory::ModelingDisciplineViolation, Standpoint::Binding);
    message = "medium: dictionary regression — {}", detail;
    failure_class = "https://blackcatinformatics.ca/gmeow/MediumDictionaryRegression";
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
    AttachDeclMismatch::CODE,
    AttachDrift::CODE,
    FanoutBijection::CODE,
    ExpectedOutputMissing::CODE,
    CacheMismatch::CODE,
    StageFailed::CODE,
    gmeow_gts_profile::Transform::CODE,
    Scoreboard::CODE,
    Mcp::CODE,
    McpAmbiguousTerm::CODE,
    Projection::CODE,
    UpProjection::CODE,
    Put::CODE,
    Generator::CODE,
    Release::CODE,
    EvalSchema::CODE,
    MetaFold::CODE,
    SpanTableConsumedAfterDrop::CODE,
    DocsMeasure::CODE,
    DocsDistribution::CODE,
    MediumUndeclaredDictionary::CODE,
    MediumUnknownDictionary::CODE,
    MediumUnknownSchema::CODE,
    MediumDigestMismatch::CODE,
    MediumOpaqueFrame::CODE,
    MediumDictionaryRegression::CODE,
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
        AttachDeclMismatch::register(),
        AttachDrift::register(),
        FanoutBijection::register(),
        ExpectedOutputMissing::register(),
        CacheMismatch::register(),
        StageFailed::register(),
        gmeow_gts_profile::Transform::register(),
        Scoreboard::register(),
        Mcp::register(),
        McpAmbiguousTerm::register(),
        Projection::register(),
        UpProjection::register(),
        Put::register(),
        Generator::register(),
        Release::register(),
        EvalSchema::register(),
        MetaFold::register(),
        SpanTableConsumedAfterDrop::register(),
        DocsMeasure::register(),
        DocsDistribution::register(),
        MediumUndeclaredDictionary::register(),
        MediumUnknownDictionary::register(),
        MediumUnknownSchema::register(),
        MediumDigestMismatch::register(),
        MediumOpaqueFrame::register(),
        MediumDictionaryRegression::register(),
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

    /// The six medium-axis kinds carry their ontology failure-class IRI on the
    /// GENERATED constant AND through the `DiagKind` accessor — the link the
    /// repo-static bijection gate reads statically must be the same one a live
    /// `Diag` producer exposes at run time, or the gate would be proving something
    /// about a string the code never uses.
    #[test]
    fn every_medium_kind_carries_its_ontology_failure_class() {
        use gmeow_errors::DiagKind;

        const GMEOW: &str = "https://blackcatinformatics.ca/gmeow/";
        let bound: [(&str, Option<&'static str>, Option<&'static str>); 6] = [
            (
                "MediumUndeclaredDictionary",
                MediumUndeclaredDictionary::FAILURE_CLASS,
                MediumUndeclaredDictionary {
                    detail: String::new(),
                }
                .failure_class(),
            ),
            (
                "MediumUnknownDictionary",
                MediumUnknownDictionary::FAILURE_CLASS,
                MediumUnknownDictionary {
                    detail: String::new(),
                }
                .failure_class(),
            ),
            (
                "MediumUnknownSchema",
                MediumUnknownSchema::FAILURE_CLASS,
                MediumUnknownSchema {
                    detail: String::new(),
                }
                .failure_class(),
            ),
            (
                "MediumDigestMismatch",
                MediumDigestMismatch::FAILURE_CLASS,
                MediumDigestMismatch {
                    detail: String::new(),
                }
                .failure_class(),
            ),
            (
                "MediumOpaqueFrame",
                MediumOpaqueFrame::FAILURE_CLASS,
                MediumOpaqueFrame {
                    detail: String::new(),
                }
                .failure_class(),
            ),
            (
                "MediumDictionaryRegression",
                MediumDictionaryRegression::FAILURE_CLASS,
                MediumDictionaryRegression {
                    detail: String::new(),
                }
                .failure_class(),
            ),
        ];
        for (local, constant, accessor) in bound {
            let expected = format!("{GMEOW}{local}");
            assert_eq!(
                constant,
                Some(expected.as_str()),
                "{local}: FAILURE_CLASS must name its ontology individual"
            );
            assert_eq!(
                accessor, constant,
                "{local}: the DiagKind accessor must agree with the constant"
            );
        }
    }
}
