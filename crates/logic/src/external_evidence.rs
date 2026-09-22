// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! External prover artifacts are evidence only after their framing, problem
//! identity, syntax and admitted semantics have been checked independently.
//!
//! The checker intentionally has two different ceilings:
//!
//! - E/Vampire TSTP refutations are parsed and bound to exact emitted input
//!   formulae. Their dependency DAG is checked by `gmeow-math-lift`, but prover
//!   inference rules are not replayed here, so they remain structural evidence.
//! - Vampire's single-sorted finite-model table is reconstructed and used to
//!   evaluate every emitted FOF sentence. A total table that satisfies the exact
//!   problem is certificate evidence for model existence.
//!
//! Unsupported artifact families remain typed attestations. No status text or
//! structural proof is silently promoted to a certificate.

use std::collections::{BTreeMap, BTreeSet};

use gmeow_errors::{Diag, DiagKind, Grade};
use gmeow_logic_compile::tptp::FofSentence;
use gmeow_logic_compile::tptp::problem_name_matches;
use gmeow_math_lift::proof::{
    Conclusion, Connective, Document, Formula, Quantifier, Role, Source, Term,
    parse as parse_derivation, parse_document,
};
use serde::Serialize;

/// Version of the independently executable artifact-checking contract.
pub const EXTERNAL_EVIDENCE_CHECKER_VERSION: &str = "gmeow-tptp-evidence-v1";

/// Maximum semantic evaluation operations admitted for one finite model.
pub const MODEL_EVALUATION_BUDGET: u64 = 10_000_000;

/// Maximum finite domain size admitted by the checker.
pub const MAX_MODEL_DOMAIN: usize = 256;

/// Transcript stream carrying one framed artifact.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ArtifactChannel {
    /// Child standard output.
    Stdout,
    /// Child standard error.
    Stderr,
}

/// One exact SZS-framed artifact captured from a child transcript.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SzsArtifact {
    /// SZS output-form token, for example `Proof` or `FiniteModel`.
    pub kind: String,
    /// Explicit problem name on the framing marker, when supplied.
    pub problem: Option<String>,
    /// Stream on which both framing markers and the artifact appeared.
    pub channel: ArtifactChannel,
    /// One-based start-marker line.
    pub start_line: usize,
    /// One-based end-marker line.
    pub end_line: usize,
    /// BLAKE3 digest of the exact bytes between the two framing lines.
    pub digest: String,
    /// Exact artifact byte count.
    pub bytes: usize,
    /// Exact UTF-8 artifact text between the framing lines.
    pub text: String,
}

/// Stable refusal emitted by strict SZS artifact framing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ArtifactProtocolError {
    /// Transcript channel which supplied the rejected framing.
    pub channel: Option<ArtifactChannel>,
    /// One-based offending framing line, when identified.
    pub line: Option<usize>,
    /// Exact rejected marker or transcript.
    pub source_text: Option<String>,
    /// Admitted problem names against which the marker was checked.
    pub expected_problems: BTreeSet<String>,
    /// Stable machine code.
    pub code: String,
    /// Human-readable explanation.
    pub detail: String,
}

/// The checker operation which refused the supplied evidence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum EvidenceOperation {
    /// Reparse the exact emitted problem.
    ProblemParsing,
    /// Parse an external finite-model document.
    ModelParsing,
    /// Parse and validate an external refutation.
    RefutationParsing,
    /// Recover the problem's function and predicate signature.
    Signature,
    /// Validate the finite interpretation tables.
    ModelTable,
    /// Validate the finite domain and its distinctness.
    Domain,
    /// Evaluate the problem against the supplied interpretation.
    Evaluation,
}

/// Why the selected evidence operation cannot certify its input.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum EvidenceFailureClass {
    /// Malformed or semantically false evidence.
    Invalid,
    /// Well-formed evidence outside the admitted checker fragment.
    Unsupported,
    /// A declared finite resource bound was reached.
    Resource,
}

/// Machine-readable evidence retained through diagnostic report projection.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct EvidenceFailure {
    /// Selected checker operation.
    pub operation: EvidenceOperation,
    /// Typed refusal category, independent of prose.
    pub classification: EvidenceFailureClass,
    /// Original refusal detail.
    pub detail: String,
    /// The input unit, symbol, or semantic locus when known.
    pub locus: Option<String>,
    /// Exact source document available at the checker boundary.
    pub source_text: Option<String>,
    /// Exact transformed text supplied to the parser when it differs from the
    /// retained source document.
    pub parsed_text: Option<String>,
    /// Resource usage or inventory size observed at refusal.
    pub observed: Option<u64>,
    /// Selected resource ceiling.
    pub limit: Option<u64>,
}

/// A serialized view of a shared diagnostic at the artifact-report boundary.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ArtifactDiagnostic {
    /// Stable shared diagnostic code.
    pub code: String,
    /// The diagnostic's complete grade.
    pub grade: Grade,
    /// Typed checker evidence.
    pub evidence: EvidenceFailure,
    /// Ordered causal error descriptions, preserved at serialization.
    pub causes: Vec<String>,
    /// Original parser diagnostic code, before checker context was attached.
    pub source_code: Option<String>,
    /// Original parser diagnostic grade.
    pub source_grade: Option<Grade>,
    /// Original parser source coordinates and focus.
    pub source_context: Option<gmeow_errors::SourceContext>,
    /// Ordered propagation contexts.
    pub context: Vec<String>,
    /// Rust source location at which the failure was observed.
    pub emitted_file: String,
    /// One-based Rust source line.
    pub emitted_line: u32,
}

#[derive(Debug)]
struct ExternalEvidenceFailure {
    evidence: EvidenceFailure,
    cause: Option<Diag>,
}

impl std::fmt::Display for ExternalEvidenceFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.evidence.detail)
    }
}

impl std::error::Error for ExternalEvidenceFailure {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.cause
            .as_ref()
            .and_then(|cause| cause.inner().source.as_deref().map(|source| source as _))
    }
}

impl DiagKind for ExternalEvidenceFailure {
    fn code(&self) -> gmeow_errors::Code {
        let code = match self.evidence.classification {
            EvidenceFailureClass::Unsupported => "logic.external-evidence.unsupported",
            EvidenceFailureClass::Resource => "logic.external-evidence.resource",
            EvidenceFailureClass::Invalid => match self.evidence.operation {
                EvidenceOperation::ProblemParsing => "logic.external-evidence.problem-parse",
                EvidenceOperation::ModelParsing => "logic.external-evidence.model-parse",
                EvidenceOperation::RefutationParsing => "logic.external-evidence.refutation-parse",
                EvidenceOperation::Signature => "logic.external-evidence.signature",
                EvidenceOperation::ModelTable => "logic.external-evidence.model-table",
                EvidenceOperation::Domain => "logic.external-evidence.domain",
                EvidenceOperation::Evaluation => "logic.external-evidence.evaluation",
            },
        };
        gmeow_errors::register_code(code)
    }
}

impl std::fmt::Display for ArtifactProtocolError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.detail)
    }
}

impl std::error::Error for ArtifactProtocolError {}

impl DiagKind for ArtifactProtocolError {
    fn code(&self) -> gmeow_errors::Code {
        gmeow_errors::register_code(&self.code)
    }
}

#[track_caller]
fn failure(operation: EvidenceOperation, detail: impl Into<String>) -> Diag {
    Diag::of_kind(ExternalEvidenceFailure {
        evidence: EvidenceFailure {
            operation,
            classification: EvidenceFailureClass::Invalid,
            detail: detail.into(),
            locus: None,
            source_text: None,
            parsed_text: None,
            observed: None,
            limit: None,
        },
        cause: None,
    })
}

#[track_caller]
fn unsupported(operation: EvidenceOperation, detail: impl Into<String>) -> Diag {
    let mut diagnostic = failure(operation, detail);
    let source = diagnostic
        .inner_mut()
        .source
        .as_mut()
        .expect("typed failure");
    let payload = source
        .downcast_mut::<ExternalEvidenceFailure>()
        .expect("typed failure");
    payload.evidence.classification = EvidenceFailureClass::Unsupported;
    let code = payload.code();
    diagnostic.inner_mut().code = code;
    diagnostic
}

#[track_caller]
fn resource_failure(
    operation: EvidenceOperation,
    detail: impl Into<String>,
    observed: u64,
    limit: u64,
) -> Diag {
    let mut diagnostic = failure(operation, detail);
    let source = diagnostic
        .inner_mut()
        .source
        .as_mut()
        .expect("typed failure");
    let payload = source
        .downcast_mut::<ExternalEvidenceFailure>()
        .expect("typed failure");
    payload.evidence.classification = EvidenceFailureClass::Resource;
    payload.evidence.observed = Some(observed);
    payload.evidence.limit = Some(limit);
    let code = payload.code();
    diagnostic.inner_mut().code = code;
    diagnostic
}

fn at_locus(mut diagnostic: Diag, locus: &str) -> Diag {
    if let Some(payload) = diagnostic
        .inner_mut()
        .source
        .as_mut()
        .and_then(|source| source.downcast_mut::<ExternalEvidenceFailure>())
    {
        payload.evidence.locus = Some(locus.to_owned());
    }
    diagnostic.with_context(format!("external evidence unit {locus}"))
}

#[track_caller]
fn evaluation_failure(detail: impl Into<String>) -> Diag {
    failure(EvidenceOperation::Evaluation, detail)
}

#[track_caller]
fn parser_failure(
    operation: EvidenceOperation,
    source: &str,
    parsed: Option<&str>,
    cause: Diag,
) -> Diag {
    Diag::of_kind(ExternalEvidenceFailure {
        evidence: EvidenceFailure {
            operation,
            classification: EvidenceFailureClass::Invalid,
            detail: cause.to_string(),
            locus: None,
            source_text: Some(source.to_owned()),
            parsed_text: parsed.map(str::to_owned),
            observed: None,
            limit: None,
        },
        cause: Some(cause),
    })
}

fn failed_check(
    artifact: &SzsArtifact,
    problem_digest: &str,
    code: &str,
    diagnostic: Diag,
    units: u64,
    evaluations: u64,
    budget: Option<u64>,
) -> ArtifactCheck {
    let failure = diagnostic
        .downcast_ref::<ExternalEvidenceFailure>()
        .expect("checker failure is typed");
    let mut evidence = failure.evidence.clone();
    if evidence.source_text.is_none()
        && matches!(
            evidence.operation,
            EvidenceOperation::ModelParsing
                | EvidenceOperation::RefutationParsing
                | EvidenceOperation::ModelTable
                | EvidenceOperation::Domain
                | EvidenceOperation::Evaluation
        )
    {
        evidence.source_text = Some(artifact.text.clone());
    }
    let disposition = match evidence.classification {
        EvidenceFailureClass::Invalid => ArtifactCheckDisposition::Invalid,
        EvidenceFailureClass::Unsupported | EvidenceFailureClass::Resource => {
            ArtifactCheckDisposition::Unsupported
        }
    };
    let mut causes = failure
        .cause
        .as_ref()
        .map(|cause| vec![cause.message().to_owned()])
        .unwrap_or_default();
    let mut cause = std::error::Error::source(failure);
    while let Some(value) = cause {
        causes.push(value.to_string());
        cause = value.source();
    }
    let code = match (evidence.classification, evidence.operation) {
        (EvidenceFailureClass::Unsupported, _) => "UNSUPPORTED_FINITE_MODEL",
        (EvidenceFailureClass::Resource, EvidenceOperation::Domain) => "FINITE_MODEL_DOMAIN_LIMIT",
        _ => code,
    };
    let mut result = check(
        disposition,
        artifact,
        problem_digest,
        units,
        evaluations,
        budget,
        code,
        diagnostic.message(),
    );
    result.diagnostic = Some(ArtifactDiagnostic {
        code: gmeow_errors::code::code_str(diagnostic.code()).to_owned(),
        grade: diagnostic.grade(),
        evidence,
        causes,
        source_code: failure
            .cause
            .as_ref()
            .map(|cause| gmeow_errors::code::code_str(cause.code()).to_owned()),
        source_grade: failure.cause.as_ref().map(Diag::grade),
        source_context: failure
            .cause
            .as_ref()
            .map(|cause| cause.inner().source_ctx.clone()),
        context: failure
            .cause
            .iter()
            .flat_map(|cause| cause.inner().context.iter())
            .chain(diagnostic.inner().context.iter())
            .map(|frame| frame.label.clone())
            .collect(),
        emitted_file: diagnostic.emitted_at().file().to_owned(),
        emitted_line: diagnostic.emitted_at().line(),
    });
    result
}

/// Strength established by the independent artifact checker.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ArtifactCheckDisposition {
    /// The artifact independently proves the operation-specific result.
    Certificate,
    /// Syntax, source binding and dependency structure were checked, while
    /// inference steps were not independently replayed.
    Structural,
    /// The artifact family or construct is outside the admitted checker fragment.
    Unsupported,
    /// The selected child emitted malformed or false evidence.
    Invalid,
}

/// Independent judgment over one exact captured artifact.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ArtifactCheck {
    /// Checker's admitted evidence strength.
    pub disposition: ArtifactCheckDisposition,
    /// Stable checker contract identity.
    pub checker: String,
    /// Exact emitted problem digest supplied by the caller.
    pub problem_digest: String,
    /// Exact captured artifact digest.
    pub artifact_digest: String,
    /// Number of artifact units inspected.
    pub units: u64,
    /// Number of semantic evaluation operations performed, when applicable.
    pub evaluations: u64,
    /// Finite semantic operation budget, when applicable.
    pub budget: Option<u64>,
    /// Stable outcome code.
    pub code: String,
    /// Human-readable explanation.
    pub detail: String,
    /// Full typed failure evidence retained at the report boundary.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub diagnostic: Option<ArtifactDiagnostic>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Marker {
    start: bool,
    kind: String,
    problem: Option<String>,
}

#[derive(Debug)]
struct OpenArtifact {
    marker: Marker,
    start_line: usize,
    text: String,
}

/// Extract every strictly paired SZS output block from both child streams.
///
/// Framing must be unnested and wholly contained in one stream. An explicit
/// problem name must equal one of `expected_problem_names`; omitted names are
/// accepted because both E and Vampire emit them in supported configurations.
///
/// # Errors
/// Returns a typed protocol error for malformed, nested, unmatched, conflicting,
/// or foreign-problem framing.
pub fn extract_szs_artifacts(
    stdout: &str,
    stderr: &str,
    expected_problem_names: &BTreeSet<String>,
) -> gmeow_errors::Result<Vec<SzsArtifact>> {
    let mut artifacts = Vec::new();
    scan_artifact_channel(
        stdout,
        ArtifactChannel::Stdout,
        expected_problem_names,
        &mut artifacts,
    )
    .map_err(|mut diagnostic| {
        let payload = diagnostic
            .inner_mut()
            .source
            .as_mut()
            .and_then(|source| source.downcast_mut::<ArtifactProtocolError>())
            .expect("protocol diagnostic");
        payload.expected_problems = expected_problem_names.clone();
        diagnostic
    })?;
    scan_artifact_channel(
        stderr,
        ArtifactChannel::Stderr,
        expected_problem_names,
        &mut artifacts,
    )
    .map_err(|mut diagnostic| {
        let payload = diagnostic
            .inner_mut()
            .source
            .as_mut()
            .and_then(|source| source.downcast_mut::<ArtifactProtocolError>())
            .expect("protocol diagnostic");
        payload.expected_problems = expected_problem_names.clone();
        diagnostic
    })?;
    Ok(artifacts)
}

/// Check one artifact against the exact emitted FOF sentence inventory.
#[must_use]
pub fn check_external_artifact(
    artifact: &SzsArtifact,
    sentences: &[FofSentence],
    problem_digest: &str,
) -> ArtifactCheck {
    match artifact.kind.as_str() {
        "Proof" | "Refutation" | "CNFRefutation" => {
            check_refutation(artifact, sentences, problem_digest)
        }
        "FiniteModel" | "Model" => check_finite_model(artifact, sentences, problem_digest),
        kind => check(
            ArtifactCheckDisposition::Unsupported,
            artifact,
            problem_digest,
            0,
            0,
            None,
            "UNSUPPORTED_SZS_ARTIFACT_KIND",
            format!("the independent checker does not admit SZS output form `{kind}`"),
        ),
    }
}

fn scan_artifact_channel(
    transcript: &str,
    channel: ArtifactChannel,
    expected_problem_names: &BTreeSet<String>,
    output: &mut Vec<SzsArtifact>,
) -> gmeow_errors::Result<()> {
    let mut open: Option<OpenArtifact> = None;
    for (index, line) in transcript.split_inclusive('\n').enumerate() {
        let line_number = index + 1;
        let marker =
            parse_marker(line).map_err(|error| protocol_at(error, channel, line_number, line))?;
        match (marker, open.as_mut()) {
            (None, Some(active)) => active.text.push_str(line),
            (None, None) => {}
            (Some(marker), None) if marker.start => {
                validate_marker_problem(&marker, expected_problem_names)
                    .map_err(|error| protocol_at(error, channel, line_number, line))?;
                open = Some(OpenArtifact {
                    marker,
                    start_line: line_number,
                    text: String::new(),
                });
            }
            (Some(marker), None) => {
                return Err(protocol_error_at(
                    channel,
                    line_number,
                    line,
                    "UNMATCHED_SZS_ARTIFACT_END",
                    format!(
                        "{:?} line {line_number} ends `{}` without a matching start marker",
                        channel, marker.kind
                    ),
                ));
            }
            (Some(marker), Some(active)) if marker.start => {
                return Err(protocol_error_at(
                    channel,
                    line_number,
                    line,
                    "NESTED_SZS_ARTIFACT",
                    format!(
                        "{:?} line {line_number} starts `{}` before `{}` from line {} ended",
                        channel, marker.kind, active.marker.kind, active.start_line
                    ),
                ));
            }
            (Some(marker), Some(_)) => {
                validate_marker_problem(&marker, expected_problem_names)
                    .map_err(|error| protocol_at(error, channel, line_number, line))?;
                let active = open.take().expect("matched open artifact");
                if marker.kind != active.marker.kind {
                    return Err(protocol_error_at(
                        channel,
                        line_number,
                        line,
                        "MISMATCHED_SZS_ARTIFACT_KIND",
                        format!(
                            "{:?} line {line_number} ends `{}` but line {} started `{}`",
                            channel, marker.kind, active.start_line, active.marker.kind
                        ),
                    ));
                }
                if marker.problem.is_some()
                    && active.marker.problem.is_some()
                    && marker.problem != active.marker.problem
                {
                    return Err(protocol_error_at(
                        channel,
                        line_number,
                        line,
                        "MISMATCHED_SZS_ARTIFACT_PROBLEM",
                        format!(
                            "{:?} artifact start/end markers name different problems",
                            channel
                        ),
                    ));
                }
                let digest = blake3::hash(active.text.as_bytes()).to_hex().to_string();
                output.push(SzsArtifact {
                    kind: active.marker.kind,
                    problem: active.marker.problem.or(marker.problem),
                    channel,
                    start_line: active.start_line,
                    end_line: line_number,
                    digest,
                    bytes: active.text.len(),
                    text: active.text,
                });
            }
        }
    }
    if let Some(active) = open {
        return Err(protocol_error_at(
            channel,
            active.start_line,
            transcript,
            "UNTERMINATED_SZS_ARTIFACT",
            format!(
                "{:?} line {} starts `{}` without a matching end marker",
                channel, active.start_line, active.marker.kind
            ),
        ));
    }
    Ok(())
}

fn parse_marker(line: &str) -> gmeow_errors::Result<Option<Marker>> {
    let mut body = line.trim();
    let Some(first) = body.chars().next() else {
        return Ok(None);
    };
    if first != '%' && first != '#' {
        return Ok(None);
    }
    body = body[first.len_utf8()..].trim_start();
    if body.starts_with('(')
        && let Some(close) = body.find(')')
        && body[1..close].chars().all(|c| c.is_ascii_digit())
    {
        body = body[close + 1..].trim_start();
    }
    if !body.starts_with("SZS output") {
        return Ok(None);
    }
    let rest = body["SZS output".len()..].trim_start();
    let (start, rest) = if let Some(rest) = rest.strip_prefix("start") {
        (true, rest.trim_start())
    } else if let Some(rest) = rest.strip_prefix("end") {
        (false, rest.trim_start())
    } else {
        return Err(protocol_error(
            "MALFORMED_SZS_ARTIFACT_MARKER",
            format!("artifact marker has neither `start` nor `end`: {body}"),
        ));
    };
    let rest = rest.trim_end_matches('.').trim();
    if rest.is_empty() {
        return Err(protocol_error(
            "MALFORMED_SZS_ARTIFACT_MARKER",
            "artifact marker has no SZS output-form token",
        ));
    }
    let (kind, problem) = rest.rsplit_once(" for ").map_or_else(
        || (rest, None),
        |(kind, problem)| (kind.trim(), Some(problem.trim().to_owned())),
    );
    if kind.is_empty() || problem.as_ref().is_some_and(String::is_empty) {
        return Err(protocol_error(
            "MALFORMED_SZS_ARTIFACT_MARKER",
            format!("artifact marker is incomplete: {body}"),
        ));
    }
    Ok(Some(Marker {
        start,
        kind: kind.to_owned(),
        problem,
    }))
}

fn validate_marker_problem(
    marker: &Marker,
    expected_problem_names: &BTreeSet<String>,
) -> gmeow_errors::Result<()> {
    if let Some(problem) = marker.problem.as_deref()
        && !problem_name_matches(problem, expected_problem_names)
    {
        return Err(protocol_error(
            "FOREIGN_SZS_ARTIFACT_PROBLEM",
            format!("artifact marker names foreign problem `{problem}`"),
        ));
    }
    Ok(())
}

fn protocol_at(mut diagnostic: Diag, channel: ArtifactChannel, line: usize, source: &str) -> Diag {
    let payload = diagnostic
        .inner_mut()
        .source
        .as_mut()
        .and_then(|source| source.downcast_mut::<ArtifactProtocolError>())
        .expect("protocol diagnostic");
    payload.channel = Some(channel);
    payload.line = Some(line);
    payload.source_text = Some(source.to_owned());
    diagnostic.with_context(format!("{channel:?} line {line}"))
}

#[track_caller]
fn protocol_error_at(
    channel: ArtifactChannel,
    line: usize,
    source: &str,
    code: &str,
    detail: impl Into<String>,
) -> Diag {
    protocol_at(protocol_error(code, detail), channel, line, source)
}

#[track_caller]
fn protocol_error(code: &str, detail: impl Into<String>) -> Diag {
    Diag::of_kind(ArtifactProtocolError {
        channel: None,
        line: None,
        source_text: None,
        expected_problems: BTreeSet::new(),
        code: code.to_owned(),
        detail: detail.into(),
    })
}

fn check_refutation(
    artifact: &SzsArtifact,
    sentences: &[FofSentence],
    problem_digest: &str,
) -> ArtifactCheck {
    let derivation = match parse_derivation(artifact.text.as_bytes()) {
        Ok(derivation) => derivation,
        Err(error) => {
            return failed_check(
                artifact,
                problem_digest,
                "MALFORMED_TSTP_REFUTATION",
                parser_failure(
                    EvidenceOperation::RefutationParsing,
                    &artifact.text,
                    None,
                    error,
                ),
                0,
                0,
                None,
            );
        }
    };
    if !is_false_conclusion(&derivation.conclusion().conclusion) {
        return invalid(
            artifact,
            problem_digest,
            "TSTP_REFUTATION_NOT_FALSE",
            EvidenceOperation::RefutationParsing,
            None,
            "the unique terminal step does not conclude `$false`",
        );
    }
    let problem = match parsed_problem(sentences) {
        Ok(problem) => problem,
        Err(detail) => {
            return failed_check(
                artifact,
                problem_digest,
                "EMITTED_PROBLEM_REPARSE_FAILED",
                detail,
                0,
                0,
                None,
            );
        }
    };
    let mut leaves = 0_u64;
    for step in derivation.steps().iter().filter(|step| !step.is_derived()) {
        leaves = leaves.saturating_add(1);
        if !step.role.is_foundational() {
            return invalid(
                artifact,
                problem_digest,
                "FOREIGN_TSTP_LEAF_ROLE",
                EvidenceOperation::RefutationParsing,
                Some(&step.name),
                format!(
                    "leaf `{}` has role `{}` rather than an admitted problem-premise role",
                    step.name,
                    step.role.as_str()
                ),
            );
        }
        if !matches!(step.source, Source::External(_)) {
            return invalid(
                artifact,
                problem_digest,
                "UNBOUND_TSTP_LEAF",
                EvidenceOperation::RefutationParsing,
                Some(&step.name),
                format!(
                    "leaf `{}` is not bound to an external problem source",
                    step.name
                ),
            );
        }
        if !problem
            .steps()
            .iter()
            .any(|input| conclusions_alpha_equivalent(&step.conclusion, &input.conclusion))
        {
            return invalid(
                artifact,
                problem_digest,
                "FOREIGN_TSTP_LEAF_FORMULA",
                EvidenceOperation::RefutationParsing,
                Some(&step.name),
                format!(
                    "leaf `{}` is not one of the exact emitted problem formulae",
                    step.name
                ),
            );
        }
    }
    if leaves == 0 {
        return invalid(
            artifact,
            problem_digest,
            "TSTP_REFUTATION_HAS_NO_INPUT_LEAF",
            EvidenceOperation::RefutationParsing,
            None,
            "the refutation contains no exact emitted problem premise",
        );
    }
    check(
        ArtifactCheckDisposition::Structural,
        artifact,
        problem_digest,
        u64::try_from(derivation.steps().len()).unwrap_or(u64::MAX),
        0,
        None,
        "TSTP_STRUCTURE_CHECKED_INFERENCES_UNREPLAYED",
        "the refutation is a well-founded `$false` derivation whose leaves match exact emitted formulae; its prover-specific inference rules were not independently replayed",
    )
}

fn conclusions_alpha_equivalent(left: &Conclusion, right: &Conclusion) -> bool {
    let (Conclusion::Formula(left), Conclusion::Formula(right)) = (left, right) else {
        return left == right;
    };
    formula_alpha_equivalent(left, right, &mut Vec::new())
}

fn formula_alpha_equivalent(
    left: &Formula,
    right: &Formula,
    bindings: &mut Vec<(String, String)>,
) -> bool {
    match (left, right) {
        (Formula::Atom(left), Formula::Atom(right)) => term_alpha_equivalent(left, right, bindings),
        (
            Formula::Equation {
                negated: left_negated,
                left: left_left,
                right: left_right,
            },
            Formula::Equation {
                negated: right_negated,
                left: right_left,
                right: right_right,
            },
        ) => {
            left_negated == right_negated
                && term_alpha_equivalent(left_left, right_left, bindings)
                && term_alpha_equivalent(left_right, right_right, bindings)
        }
        (Formula::Not(left), Formula::Not(right)) => {
            formula_alpha_equivalent(left, right, bindings)
        }
        (
            Formula::Binary {
                connective: left_connective,
                left: left_left,
                right: left_right,
            },
            Formula::Binary {
                connective: right_connective,
                left: right_left,
                right: right_right,
            },
        ) => {
            left_connective == right_connective
                && formula_alpha_equivalent(left_left, right_left, bindings)
                && formula_alpha_equivalent(left_right, right_right, bindings)
        }
        (
            Formula::Quantified {
                quantifier: left_quantifier,
                variables: left_variables,
                body: left_body,
            },
            Formula::Quantified {
                quantifier: right_quantifier,
                variables: right_variables,
                body: right_body,
            },
        ) if left_quantifier == right_quantifier
            && left_variables.len() == right_variables.len() =>
        {
            let checkpoint = bindings.len();
            bindings.extend(
                left_variables
                    .iter()
                    .cloned()
                    .zip(right_variables.iter().cloned()),
            );
            let equivalent = formula_alpha_equivalent(left_body, right_body, bindings);
            bindings.truncate(checkpoint);
            equivalent
        }
        _ => false,
    }
}

fn term_alpha_equivalent(left: &Term, right: &Term, bindings: &[(String, String)]) -> bool {
    match (left, right) {
        (Term::Variable(left), Term::Variable(right)) => bindings
            .iter()
            .rev()
            .find(|(bound, _)| bound == left)
            .map_or_else(
                || !bindings.iter().rev().any(|(_, bound)| bound == right) && left == right,
                |(_, bound)| bound == right,
            ),
        (
            Term::Apply {
                functor: left_functor,
                args: left_args,
            },
            Term::Apply {
                functor: right_functor,
                args: right_args,
            },
        ) => {
            left_functor == right_functor
                && left_args.len() == right_args.len()
                && left_args
                    .iter()
                    .zip(right_args)
                    .all(|(left, right)| term_alpha_equivalent(left, right, bindings))
        }
        _ => false,
    }
}

fn is_false_conclusion(conclusion: &Conclusion) -> bool {
    match conclusion {
        Conclusion::Formula(Formula::Atom(Term::Apply { functor, args })) => {
            functor == "$false" && args.is_empty()
        }
        Conclusion::Clause(clause) => {
            let [literal] = clause.literals.as_slice() else {
                return false;
            };
            !literal.negated
                && literal.equated.is_none()
                && matches!(
                    &literal.atom,
                    Term::Apply { functor, args } if functor == "$false" && args.is_empty()
                )
        }
        _ => false,
    }
}

fn check_finite_model(
    artifact: &SzsArtifact,
    sentences: &[FofSentence],
    problem_digest: &str,
) -> ArtifactCheck {
    check_finite_model_with_budget(artifact, sentences, problem_digest, MODEL_EVALUATION_BUDGET)
}

fn check_finite_model_with_budget(
    artifact: &SzsArtifact,
    sentences: &[FofSentence],
    problem_digest: &str,
    evaluation_limit: u64,
) -> ArtifactCheck {
    let problem = match parsed_problem(sentences) {
        Ok(problem) => problem,
        Err(detail) => {
            return failed_check(
                artifact,
                problem_digest,
                "EMITTED_PROBLEM_REPARSE_FAILED",
                detail,
                0,
                0,
                None,
            );
        }
    };
    let model_document = match parse_model_document(&artifact.text) {
        Ok(document) => document,
        Err(detail) => {
            return failed_check(
                artifact,
                problem_digest,
                "MALFORMED_FINITE_MODEL",
                detail,
                0,
                0,
                None,
            );
        }
    };
    let model = match FiniteModel::from_document(&model_document, &problem) {
        Ok(model) => model,
        Err(detail) => {
            return failed_check(
                artifact,
                problem_digest,
                "INVALID_FINITE_MODEL_TABLE",
                detail,
                0,
                0,
                None,
            );
        }
    };
    let mut budget = EvaluationBudget::new(evaluation_limit);
    for step in problem.steps() {
        let Conclusion::Formula(formula) = &step.conclusion else {
            return invalid(
                artifact,
                problem_digest,
                "NON_FOF_EMITTED_SENTENCE",
                EvidenceOperation::ProblemParsing,
                Some(&step.name),
                format!("emitted sentence `{}` is not FOF", step.name),
            );
        };
        match model
            .evaluate(formula, &mut budget)
            .map_err(|error| at_locus(error, &step.name))
        {
            Ok(true) => {}
            Ok(false) => {
                let detail = format!(
                    "finite interpretation makes emitted sentence `{}` false",
                    step.name
                );
                return failed_check(
                    artifact,
                    problem_digest,
                    "FINITE_MODEL_FALSIFIES_PROBLEM",
                    at_locus(evaluation_failure(detail), &step.name),
                    u64::try_from(model_document.steps().len()).unwrap_or(u64::MAX),
                    budget.used(),
                    Some(evaluation_limit),
                );
            }
            Err(error) => {
                let resource =
                    error
                        .downcast_ref::<ExternalEvidenceFailure>()
                        .is_some_and(|failure| {
                            failure.evidence.classification == EvidenceFailureClass::Resource
                        });
                return failed_check(
                    artifact,
                    problem_digest,
                    if resource {
                        "FINITE_MODEL_CHECK_BUDGET_EXHAUSTED"
                    } else {
                        "FINITE_MODEL_EVALUATION_FAILED"
                    },
                    error,
                    u64::try_from(model_document.steps().len()).unwrap_or(u64::MAX),
                    budget.used(),
                    Some(evaluation_limit),
                );
            }
        }
    }
    check(
        ArtifactCheckDisposition::Certificate,
        artifact,
        problem_digest,
        u64::try_from(model_document.steps().len()).unwrap_or(u64::MAX),
        budget.used(),
        Some(evaluation_limit),
        "FINITE_MODEL_CHECKED",
        "the total single-sorted finite interpretation satisfies every exact emitted FOF sentence",
    )
}

fn parsed_problem(sentences: &[FofSentence]) -> gmeow_errors::Result<Document> {
    if sentences.is_empty() {
        return Err(failure(
            EvidenceOperation::ProblemParsing,
            "the emitted problem sentence inventory is empty",
        ));
    }
    let mut source = String::new();
    for sentence in sentences {
        source.push_str("fof(");
        source.push_str(&sentence.name);
        source.push_str(", axiom, ");
        source.push_str(&sentence.body);
        source.push_str(").\n");
    }
    parse_document(source.as_bytes())
        .map_err(|error| parser_failure(EvidenceOperation::ProblemParsing, &source, None, error))
}

fn check(
    disposition: ArtifactCheckDisposition,
    artifact: &SzsArtifact,
    problem_digest: &str,
    units: u64,
    evaluations: u64,
    budget: Option<u64>,
    code: &str,
    detail: impl Into<String>,
) -> ArtifactCheck {
    ArtifactCheck {
        disposition,
        checker: EXTERNAL_EVIDENCE_CHECKER_VERSION.to_owned(),
        problem_digest: problem_digest.to_owned(),
        artifact_digest: artifact.digest.clone(),
        units,
        evaluations,
        budget,
        code: code.to_owned(),
        detail: detail.into(),
        diagnostic: None,
    }
}

fn invalid(
    artifact: &SzsArtifact,
    problem_digest: &str,
    code: &str,
    operation: EvidenceOperation,
    locus: Option<&str>,
    detail: impl Into<String>,
) -> ArtifactCheck {
    let diagnostic = failure(operation, detail);
    let diagnostic = match locus {
        Some(locus) => at_locus(diagnostic, locus),
        None => diagnostic,
    };
    failed_check(artifact, problem_digest, code, diagnostic, 0, 0, None)
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct TableKey {
    symbol: String,
    arguments: Vec<String>,
}

#[derive(Debug)]
struct Signature {
    functions: BTreeMap<String, usize>,
    predicates: BTreeMap<String, usize>,
}

impl Signature {
    fn from_problem(problem: &Document) -> gmeow_errors::Result<Self> {
        let mut signature = Self {
            functions: BTreeMap::new(),
            predicates: BTreeMap::new(),
        };
        for step in problem.steps() {
            let Conclusion::Formula(formula) = &step.conclusion else {
                return Err(failure(
                    EvidenceOperation::Signature,
                    format!("problem sentence `{}` is not FOF", step.name),
                ));
            };
            signature
                .formula(formula)
                .map_err(|error| at_locus(error, &step.name))?;
        }
        Ok(signature)
    }

    fn formula(&mut self, formula: &Formula) -> gmeow_errors::Result<()> {
        match formula {
            Formula::Atom(term) => {
                let Term::Apply { functor, args } = term else {
                    return Err(failure(
                        EvidenceOperation::Signature,
                        "a predicate position contains a variable",
                    ));
                };
                if !matches!(functor.as_str(), "$true" | "$false") {
                    insert_arity(&mut self.predicates, functor, args.len(), "predicate")
                        .map_err(|error| at_locus(error, functor))?;
                    if self.functions.contains_key(functor) {
                        return Err(failure(
                            EvidenceOperation::Signature,
                            format!("symbol `{functor}` is used as both function and predicate"),
                        ));
                    }
                }
                for argument in args {
                    self.term(argument)?;
                }
            }
            Formula::Equation { left, right, .. } => {
                self.term(left)?;
                self.term(right)?;
            }
            Formula::Not(inner) => self.formula(inner)?,
            Formula::Binary { left, right, .. } => {
                self.formula(left)?;
                self.formula(right)?;
            }
            Formula::Quantified { body, .. } => self.formula(body)?,
        }
        Ok(())
    }

    fn term(&mut self, term: &Term) -> gmeow_errors::Result<()> {
        let Term::Apply { functor, args } = term else {
            return Ok(());
        };
        insert_arity(&mut self.functions, functor, args.len(), "function")
            .map_err(|error| at_locus(error, functor))?;
        if self.predicates.contains_key(functor) {
            return Err(failure(
                EvidenceOperation::Signature,
                format!("symbol `{functor}` is used as both predicate and function"),
            ));
        }
        for argument in args {
            self.term(argument)?;
        }
        Ok(())
    }
}

fn insert_arity(
    arities: &mut BTreeMap<String, usize>,
    symbol: &str,
    arity: usize,
    kind: &str,
) -> gmeow_errors::Result<()> {
    if let Some(previous) = arities.insert(symbol.to_owned(), arity)
        && previous != arity
    {
        return Err(failure(
            EvidenceOperation::Signature,
            format!("{kind} `{symbol}` occurs at arities {previous} and {arity}"),
        ));
    }
    Ok(())
}

#[derive(Debug)]
struct FiniteModel {
    domain: Vec<String>,
    functions: BTreeMap<TableKey, String>,
    predicates: BTreeMap<TableKey, bool>,
}

impl FiniteModel {
    fn from_document(model: &Document, problem: &Document) -> gmeow_errors::Result<Self> {
        let signature = Signature::from_problem(problem)?;
        let mut domain_formula: Option<&Formula> = None;
        let mut distinct = Vec::new();
        let mut definitions = Vec::new();
        for step in model.steps() {
            if !matches!(step.source, Source::Asserted) {
                return Err(failure(
                    EvidenceOperation::ModelTable,
                    format!(
                        "model unit `{}` carries proof provenance rather than a direct table entry",
                        step.name
                    ),
                ));
            }
            let Conclusion::Formula(formula) = &step.conclusion else {
                return Err(failure(
                    EvidenceOperation::ModelTable,
                    format!("model unit `{}` is not FOF", step.name),
                ));
            };
            if step.role == Role::FiDomain || step.name.starts_with("finite_domain") {
                if domain_formula.replace(formula).is_some() {
                    return Err(failure(
                        EvidenceOperation::ModelTable,
                        "finite model has more than one domain formula",
                    ));
                }
            } else if step.name.starts_with("distinct_domain") {
                distinct.push(formula);
            } else if matches!(
                step.role,
                Role::FiFunctors | Role::FiPredicates | Role::Axiom
            ) {
                flatten_and(formula, &mut definitions);
            } else {
                return Err(unsupported(
                    EvidenceOperation::ModelTable,
                    format!(
                        "model unit `{}` has unsupported role `{}`",
                        step.name,
                        step.role.as_str()
                    ),
                ));
            }
        }
        let domain = extract_domain(domain_formula.ok_or_else(|| {
            failure(
                EvidenceOperation::ModelTable,
                "finite model has no domain formula",
            )
        })?)?;
        if domain.len() > MAX_MODEL_DOMAIN {
            return Err(resource_failure(
                EvidenceOperation::Domain,
                format!(
                    "finite model domain has {} elements; checker limit is {MAX_MODEL_DOMAIN}",
                    domain.len()
                ),
                u64::try_from(domain.len()).unwrap_or(u64::MAX),
                u64::try_from(MAX_MODEL_DOMAIN).unwrap_or(u64::MAX),
            ));
        }
        verify_distinctness(&domain, &distinct)?;
        let domain_set: BTreeSet<_> = domain.iter().cloned().collect();
        let mut functions = BTreeMap::new();
        let mut predicates = BTreeMap::new();
        for definition in definitions {
            match definition {
                Formula::Equation {
                    negated: false,
                    left,
                    right,
                } => insert_function_definition(left, right, &domain_set, &mut functions)?,
                Formula::Atom(atom) => {
                    insert_predicate_definition(atom, true, &domain_set, &mut predicates)?
                }
                Formula::Not(inner) => {
                    let Formula::Atom(atom) = inner.as_ref() else {
                        return Err(failure(
                            EvidenceOperation::ModelTable,
                            "a negated model table entry must be a predicate atom",
                        ));
                    };
                    insert_predicate_definition(atom, false, &domain_set, &mut predicates)?;
                }
                _ => {
                    return Err(failure(EvidenceOperation::ModelTable, "finite-model tables admit only ground function equations and signed predicate atoms"
                            .to_owned()));
                }
            }
        }
        for (symbol, arity) in &signature.functions {
            if *arity == 0 && domain_set.contains(symbol) {
                functions
                    .entry(TableKey {
                        symbol: symbol.clone(),
                        arguments: Vec::new(),
                    })
                    .or_insert_with(|| symbol.clone());
            }
            verify_total_table(symbol, *arity, domain.len(), functions.keys())
                .map_err(|error| at_locus(error, symbol))?;
        }
        for (symbol, arity) in &signature.predicates {
            verify_total_table(symbol, *arity, domain.len(), predicates.keys())
                .map_err(|error| at_locus(error, symbol))?;
        }
        Ok(Self {
            domain,
            functions,
            predicates,
        })
    }

    fn evaluate(
        &self,
        formula: &Formula,
        budget: &mut EvaluationBudget,
    ) -> gmeow_errors::Result<bool> {
        let mut bindings = BTreeMap::new();
        self.eval_formula(formula, &mut bindings, budget)
    }

    fn eval_formula(
        &self,
        formula: &Formula,
        bindings: &mut BTreeMap<String, String>,
        budget: &mut EvaluationBudget,
    ) -> gmeow_errors::Result<bool> {
        budget.charge()?;
        match formula {
            Formula::Atom(Term::Apply { functor, args })
                if functor == "$true" && args.is_empty() =>
            {
                Ok(true)
            }
            Formula::Atom(Term::Apply { functor, args })
                if functor == "$false" && args.is_empty() =>
            {
                Ok(false)
            }
            Formula::Atom(Term::Apply { functor, args }) => {
                let arguments = self.eval_terms(args, bindings, budget)?;
                self.predicates
                    .get(&TableKey {
                        symbol: functor.clone(),
                        arguments,
                    })
                    .copied()
                    .ok_or_else(|| {
                        evaluation_failure(format!(
                            "finite model has no predicate value for `{functor}`"
                        ))
                    })
            }
            Formula::Atom(Term::Variable(variable)) => Err(evaluation_failure(format!(
                "variable `{variable}` appears in predicate position"
            ))),
            Formula::Equation {
                negated,
                left,
                right,
            } => {
                let equal = self.eval_term(left, bindings, budget)?
                    == self.eval_term(right, bindings, budget)?;
                Ok(if *negated { !equal } else { equal })
            }
            Formula::Not(inner) => Ok(!self.eval_formula(inner, bindings, budget)?),
            Formula::Binary {
                connective,
                left,
                right,
            } => {
                let left = self.eval_formula(left, bindings, budget)?;
                let right = self.eval_formula(right, bindings, budget)?;
                Ok(match connective {
                    Connective::And => left && right,
                    Connective::Or => left || right,
                    Connective::Imply => !left || right,
                    Connective::RevImply => !right || left,
                    Connective::Iff => left == right,
                    Connective::Xor => left != right,
                    Connective::Nor => !(left || right),
                    Connective::Nand => !(left && right),
                })
            }
            Formula::Quantified {
                quantifier,
                variables,
                body,
            } => self.eval_quantified(*quantifier, variables, body, bindings, budget),
        }
    }

    fn eval_quantified(
        &self,
        quantifier: Quantifier,
        variables: &[String],
        body: &Formula,
        bindings: &mut BTreeMap<String, String>,
        budget: &mut EvaluationBudget,
    ) -> gmeow_errors::Result<bool> {
        fn visit(
            model: &FiniteModel,
            quantifier: Quantifier,
            variables: &[String],
            body: &Formula,
            bindings: &mut BTreeMap<String, String>,
            budget: &mut EvaluationBudget,
            cursor: usize,
        ) -> gmeow_errors::Result<bool> {
            if cursor == variables.len() {
                return model.eval_formula(body, bindings, budget);
            }
            let variable = &variables[cursor];
            let previous = bindings.get(variable).cloned();
            let mut result = matches!(quantifier, Quantifier::ForAll);
            for element in &model.domain {
                bindings.insert(variable.clone(), element.clone());
                let value = visit(
                    model,
                    quantifier,
                    variables,
                    body,
                    bindings,
                    budget,
                    cursor + 1,
                )?;
                match quantifier {
                    Quantifier::ForAll if !value => {
                        result = false;
                        break;
                    }
                    Quantifier::Exists if value => {
                        result = true;
                        break;
                    }
                    _ => {}
                }
            }
            if let Some(previous) = previous {
                bindings.insert(variable.clone(), previous);
            } else {
                bindings.remove(variable);
            }
            Ok(result)
        }
        visit(self, quantifier, variables, body, bindings, budget, 0)
    }

    fn eval_terms(
        &self,
        terms: &[Term],
        bindings: &BTreeMap<String, String>,
        budget: &mut EvaluationBudget,
    ) -> gmeow_errors::Result<Vec<String>> {
        terms
            .iter()
            .map(|term| self.eval_term(term, bindings, budget))
            .collect()
    }

    fn eval_term(
        &self,
        term: &Term,
        bindings: &BTreeMap<String, String>,
        budget: &mut EvaluationBudget,
    ) -> gmeow_errors::Result<String> {
        budget.charge()?;
        match term {
            Term::Variable(variable) => bindings.get(variable).cloned().ok_or_else(|| {
                evaluation_failure(format!("free variable `{variable}` in emitted problem"))
            }),
            Term::Apply { functor, args } => {
                let arguments = self.eval_terms(args, bindings, budget)?;
                self.functions
                    .get(&TableKey {
                        symbol: functor.clone(),
                        arguments,
                    })
                    .cloned()
                    .ok_or_else(|| {
                        evaluation_failure(format!(
                            "finite model has no function value for `{functor}`"
                        ))
                    })
            }
        }
    }
}

fn verify_total_table<'a>(
    symbol: &str,
    arity: usize,
    domain_size: usize,
    keys: impl Iterator<Item = &'a TableKey>,
) -> gmeow_errors::Result<()> {
    let exponent = u32::try_from(arity).map_err(|_| {
        at_locus(
            resource_failure(
                EvidenceOperation::ModelTable,
                format!("symbol `{symbol}` arity {arity} does not fit u32"),
                u64::try_from(arity).unwrap_or(u64::MAX),
                u64::from(u32::MAX),
            ),
            symbol,
        )
    })?;
    let expected = domain_size.checked_pow(exponent).ok_or_else(|| {
        at_locus(
            resource_failure(
                EvidenceOperation::ModelTable,
                format!("table size for `{symbol}/{arity}` over {domain_size} elements overflows"),
                u64::try_from(domain_size).unwrap_or(u64::MAX),
                u64::try_from(usize::MAX).unwrap_or(u64::MAX),
            ),
            symbol,
        )
    })?;
    let mut actual = 0_usize;
    for key in keys.filter(|key| key.symbol == symbol) {
        if key.arguments.len() != arity {
            return Err(failure(
                EvidenceOperation::ModelTable,
                format!(
                    "model defines `{symbol}` at arity {} but the problem uses arity {arity}",
                    key.arguments.len()
                ),
            ));
        }
        actual = actual.saturating_add(1);
    }
    if actual != expected {
        return Err(failure(
            EvidenceOperation::ModelTable,
            format!(
                "model table for `{symbol}/{arity}` has {actual} entries; a total interpretation over {domain_size} elements requires {expected}"
            ),
        ));
    }
    Ok(())
}

fn extract_domain(formula: &Formula) -> gmeow_errors::Result<Vec<String>> {
    let Formula::Quantified {
        quantifier: Quantifier::ForAll,
        variables,
        body,
    } = formula
    else {
        return Err(failure(
            EvidenceOperation::Domain,
            "domain formula must universally quantify one variable",
        ));
    };
    let [variable] = variables.as_slice() else {
        return Err(failure(
            EvidenceOperation::Domain,
            "domain formula must bind exactly one variable",
        ));
    };
    let mut alternatives = Vec::new();
    flatten_or(body, &mut alternatives);
    let mut domain = Vec::new();
    let mut seen = BTreeSet::new();
    for alternative in alternatives {
        let Formula::Equation {
            negated: false,
            left,
            right,
        } = alternative
        else {
            return Err(failure(
                EvidenceOperation::Domain,
                "domain alternatives must be positive equalities",
            ));
        };
        let element = match (left, right) {
            (Term::Variable(name), term) | (term, Term::Variable(name)) if name == variable => {
                domain_constant(term)?
            }
            _ => {
                return Err(failure(
                    EvidenceOperation::Domain,
                    "every domain equality must compare the bound variable with a ground constant"
                        .to_owned(),
                ));
            }
        };
        if !seen.insert(element.clone()) {
            return Err(failure(
                EvidenceOperation::Domain,
                format!("domain element `{element}` appears more than once"),
            ));
        }
        domain.push(element);
    }
    if domain.is_empty() {
        return Err(failure(
            EvidenceOperation::Domain,
            "finite model domain is empty",
        ));
    }
    Ok(domain)
}

fn verify_distinctness(domain: &[String], formulas: &[&Formula]) -> gmeow_errors::Result<()> {
    if domain.len() == 1 {
        if formulas.is_empty() {
            return Ok(());
        }
    } else if formulas.is_empty() {
        return Err(failure(
            EvidenceOperation::Domain,
            "multi-element model has no distinct-domain formula",
        ));
    }
    let domain_set: BTreeSet<_> = domain.iter().cloned().collect();
    let mut actual = BTreeSet::new();
    for formula in formulas {
        let mut conjuncts = Vec::new();
        flatten_and(formula, &mut conjuncts);
        for conjunct in conjuncts {
            let Formula::Equation {
                negated: true,
                left,
                right,
            } = conjunct
            else {
                return Err(failure(
                    EvidenceOperation::Domain,
                    "distinct-domain formula must contain only disequalities",
                ));
            };
            let left = domain_constant(left)?;
            let right = domain_constant(right)?;
            if left == right || !domain_set.contains(&left) || !domain_set.contains(&right) {
                return Err(failure(
                    EvidenceOperation::Domain,
                    "distinct-domain formula names an invalid pair",
                ));
            }
            let pair = if left < right {
                (left, right)
            } else {
                (right, left)
            };
            if !actual.insert(pair) {
                return Err(failure(
                    EvidenceOperation::Domain,
                    "distinct-domain formula repeats a pair",
                ));
            }
        }
    }
    let mut expected = BTreeSet::new();
    for (index, left) in domain.iter().enumerate() {
        for right in &domain[index + 1..] {
            let pair = if left < right {
                (left.clone(), right.clone())
            } else {
                (right.clone(), left.clone())
            };
            expected.insert(pair);
        }
    }
    if actual != expected {
        return Err(failure(
            EvidenceOperation::Domain,
            "distinct-domain formula does not cover every domain pair exactly once",
        ));
    }
    Ok(())
}

fn insert_function_definition(
    left: &Term,
    right: &Term,
    domain: &BTreeSet<String>,
    functions: &mut BTreeMap<TableKey, String>,
) -> gmeow_errors::Result<()> {
    let left_domain = domain_term(left, domain);
    let right_domain = domain_term(right, domain);
    let (application, result) = match (left_domain, right_domain) {
        (None, Some(result)) => (left, result),
        (Some(result), None) => (right, result),
        _ => {
            return Err(failure(
                EvidenceOperation::ModelTable,
                "function definition must equate one application with one domain element"
                    .to_owned(),
            ));
        }
    };
    let Term::Apply { functor, args } = application else {
        return Err(failure(
            EvidenceOperation::ModelTable,
            "function-definition left side must be an application",
        ));
    };
    let arguments = args
        .iter()
        .map(|argument| {
            domain_term(argument, domain).ok_or_else(|| {
                failure(
                    EvidenceOperation::ModelTable,
                    "function table argument is not a domain element",
                )
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    let key = TableKey {
        symbol: functor.clone(),
        arguments,
    };
    if functions.insert(key, result).is_some() {
        return Err(failure(
            EvidenceOperation::ModelTable,
            format!("function `{functor}` has a duplicate table entry"),
        ));
    }
    Ok(())
}

fn insert_predicate_definition(
    atom: &Term,
    value: bool,
    domain: &BTreeSet<String>,
    predicates: &mut BTreeMap<TableKey, bool>,
) -> gmeow_errors::Result<()> {
    let Term::Apply { functor, args } = atom else {
        return Err(failure(
            EvidenceOperation::ModelTable,
            "predicate table entry has a variable in predicate position",
        ));
    };
    if matches!(functor.as_str(), "$true" | "$false") {
        return Err(failure(
            EvidenceOperation::ModelTable,
            "predicate table may not redefine a TPTP truth constant",
        ));
    }
    let arguments = args
        .iter()
        .map(|argument| {
            domain_term(argument, domain).ok_or_else(|| {
                failure(
                    EvidenceOperation::ModelTable,
                    "predicate table argument is not a domain element",
                )
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    let key = TableKey {
        symbol: functor.clone(),
        arguments,
    };
    if predicates.insert(key, value).is_some() {
        return Err(failure(
            EvidenceOperation::ModelTable,
            format!("predicate `{functor}` has a duplicate table entry"),
        ));
    }
    Ok(())
}

fn domain_constant(term: &Term) -> gmeow_errors::Result<String> {
    let Term::Apply { functor, args } = term else {
        return Err(failure(
            EvidenceOperation::Domain,
            "domain element is a variable",
        ));
    };
    if !args.is_empty() {
        return Err(failure(
            EvidenceOperation::Domain,
            "domain element is not a constant",
        ));
    }
    Ok(functor.clone())
}

fn domain_term(term: &Term, domain: &BTreeSet<String>) -> Option<String> {
    let Term::Apply { functor, args } = term else {
        return None;
    };
    (args.is_empty() && domain.contains(functor)).then(|| functor.clone())
}

fn flatten_and<'a>(formula: &'a Formula, output: &mut Vec<&'a Formula>) {
    if let Formula::Binary {
        connective: Connective::And,
        left,
        right,
    } = formula
    {
        flatten_and(left, output);
        flatten_and(right, output);
    } else {
        output.push(formula);
    }
}

fn flatten_or<'a>(formula: &'a Formula, output: &mut Vec<&'a Formula>) {
    if let Formula::Binary {
        connective: Connective::Or,
        left,
        right,
    } = formula
    {
        flatten_or(left, output);
        flatten_or(right, output);
    } else {
        output.push(formula);
    }
}

#[derive(Debug)]
struct EvaluationBudget {
    remaining: u64,
    used: u64,
}

impl EvaluationBudget {
    fn new(limit: u64) -> Self {
        Self {
            remaining: limit,
            used: 0,
        }
    }

    fn charge(&mut self) -> gmeow_errors::Result<()> {
        if self.remaining == 0 {
            return Err(resource_failure(
                EvidenceOperation::Evaluation,
                "finite interpretation exceeds the bounded semantic checker budget",
                self.used,
                self.used,
            ));
        }
        self.remaining -= 1;
        self.used += 1;
        Ok(())
    }

    fn used(&self) -> u64 {
        self.used
    }
}

fn parse_model_document(source: &str) -> gmeow_errors::Result<Document> {
    parse_model_document_inner(source).map_err(|mut diagnostic| {
        if let Some(payload) = diagnostic
            .inner_mut()
            .source
            .as_mut()
            .and_then(|source| source.downcast_mut::<ExternalEvidenceFailure>())
        {
            payload.evidence.source_text = Some(source.to_owned());
        }
        diagnostic
    })
}

fn parse_model_document_inner(source: &str) -> gmeow_errors::Result<Document> {
    let clean = strip_comments(source)?;
    let units = split_annotated_units(&clean)?;
    let mut normalized = String::new();
    for unit in units {
        let (dialect, fields) = annotated_fields(unit)?;
        match dialect {
            "fof" | "cnf" => {
                normalized.push_str(unit.trim());
                normalized.push('\n');
            }
            "tff" => {
                if fields.len() < 3 {
                    return Err(failure(
                        EvidenceOperation::ModelParsing,
                        "typed model unit has fewer than three fields",
                    ));
                }
                let role = fields[1].trim();
                if role == "type" {
                    continue;
                }
                if role != "axiom" || fields.len() != 3 {
                    return Err(unsupported(
                        EvidenceOperation::ModelParsing,
                        format!(
                            "typed model unit has unsupported role or annotation shape `{role}`"
                        ),
                    ));
                }
                normalized.push_str("fof(");
                normalized.push_str(fields[0].trim());
                normalized.push_str(", axiom, ");
                normalized.push_str(&strip_default_sort_annotations(fields[2])?);
                normalized.push_str(").\n");
            }
            other => {
                return Err(unsupported(
                    EvidenceOperation::ModelParsing,
                    format!("finite-model artifact uses unsupported TPTP dialect `{other}`"),
                ));
            }
        }
    }
    parse_document(normalized.as_bytes()).map_err(|error| {
        parser_failure(
            EvidenceOperation::ModelParsing,
            source,
            Some(&normalized),
            error,
        )
    })
}

fn strip_comments(source: &str) -> gmeow_errors::Result<String> {
    let chars: Vec<char> = source.chars().collect();
    let mut output = String::with_capacity(source.len());
    let mut index = 0;
    let mut quote = None;
    let mut escaped = false;
    while index < chars.len() {
        let character = chars[index];
        if let Some(active) = quote {
            output.push(character);
            if escaped {
                escaped = false;
            } else if character == '\\' {
                escaped = true;
            } else if character == active {
                quote = None;
            }
            index += 1;
            continue;
        }
        if matches!(character, '\'' | '"') {
            quote = Some(character);
            output.push(character);
            index += 1;
            continue;
        }
        if matches!(character, '%' | '#') {
            while index < chars.len() && chars[index] != '\n' {
                output.push(' ');
                index += 1;
            }
            continue;
        }
        if character == '/' && chars.get(index + 1) == Some(&'*') {
            output.push(' ');
            output.push(' ');
            index += 2;
            let mut closed = false;
            while index < chars.len() {
                if chars[index] == '*' && chars.get(index + 1) == Some(&'/') {
                    output.push(' ');
                    output.push(' ');
                    index += 2;
                    closed = true;
                    break;
                }
                output.push(if chars[index] == '\n' { '\n' } else { ' ' });
                index += 1;
            }
            if !closed {
                return Err(failure(
                    EvidenceOperation::ModelParsing,
                    "finite-model artifact has an unterminated block comment",
                ));
            }
            continue;
        }
        output.push(character);
        index += 1;
    }
    if quote.is_some() {
        return Err(failure(
            EvidenceOperation::ModelParsing,
            "finite-model artifact has an unterminated quoted atom",
        ));
    }
    Ok(output)
}

fn split_annotated_units(source: &str) -> gmeow_errors::Result<Vec<&str>> {
    let mut units = Vec::new();
    let mut start = None;
    let mut depth = 0_i64;
    let mut quote = None;
    let mut escaped = false;
    for (offset, character) in source.char_indices() {
        if start.is_none() {
            if character.is_whitespace() {
                continue;
            }
            start = Some(offset);
        }
        if let Some(active) = quote {
            if escaped {
                escaped = false;
            } else if character == '\\' {
                escaped = true;
            } else if character == active {
                quote = None;
            }
            continue;
        }
        if matches!(character, '\'' | '"') {
            quote = Some(character);
            continue;
        }
        match character {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => {
                depth -= 1;
                if depth < 0 {
                    return Err(failure(
                        EvidenceOperation::ModelParsing,
                        "finite-model artifact has an unmatched closing delimiter",
                    ));
                }
            }
            '.' if depth == 0 => {
                let begin = start.take().expect("unit start exists");
                units.push(&source[begin..offset + 1]);
            }
            _ => {}
        }
    }
    if depth != 0 || start.is_some() || quote.is_some() {
        return Err(failure(
            EvidenceOperation::ModelParsing,
            "finite-model artifact ends inside an annotated unit",
        ));
    }
    if units.is_empty() {
        return Err(failure(
            EvidenceOperation::ModelParsing,
            "finite-model artifact contains no annotated formula",
        ));
    }
    Ok(units)
}

fn annotated_fields(unit: &str) -> gmeow_errors::Result<(&str, Vec<&str>)> {
    let unit = unit.trim();
    let open = unit.find('(').ok_or_else(|| {
        failure(
            EvidenceOperation::ModelParsing,
            "annotated model unit has no opening parenthesis",
        )
    })?;
    if !unit.ends_with(".)") && !unit.ends_with(").") {
        return Err(failure(
            EvidenceOperation::ModelParsing,
            "annotated model unit has no `).` terminator",
        ));
    }
    let dialect = unit[..open].trim();
    let close = unit.rfind(')').ok_or_else(|| {
        failure(
            EvidenceOperation::ModelParsing,
            "annotated model unit has no closing parenthesis",
        )
    })?;
    let inner = &unit[open + 1..close];
    Ok((dialect, split_top_level(inner)?))
}

fn split_top_level(source: &str) -> gmeow_errors::Result<Vec<&str>> {
    let mut fields = Vec::new();
    let mut start = 0;
    let mut depth = 0_i64;
    let mut quote = None;
    let mut escaped = false;
    for (offset, character) in source.char_indices() {
        if let Some(active) = quote {
            if escaped {
                escaped = false;
            } else if character == '\\' {
                escaped = true;
            } else if character == active {
                quote = None;
            }
            continue;
        }
        if matches!(character, '\'' | '"') {
            quote = Some(character);
            continue;
        }
        match character {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth -= 1,
            ',' if depth == 0 => {
                fields.push(&source[start..offset]);
                start = offset + 1;
            }
            _ => {}
        }
        if depth < 0 {
            return Err(failure(
                EvidenceOperation::ModelParsing,
                "annotated model fields have unmatched delimiters",
            ));
        }
    }
    if depth != 0 || quote.is_some() {
        return Err(failure(
            EvidenceOperation::ModelParsing,
            "annotated model fields end inside a delimiter",
        ));
    }
    fields.push(&source[start..]);
    Ok(fields)
}

fn strip_default_sort_annotations(formula: &str) -> gmeow_errors::Result<String> {
    let chars: Vec<char> = formula.chars().collect();
    let mut output = String::with_capacity(formula.len());
    let mut index = 0;
    let mut bracket_depth = 0_u32;
    let mut quote = None;
    let mut escaped = false;
    while index < chars.len() {
        let character = chars[index];
        if let Some(active) = quote {
            output.push(character);
            if escaped {
                escaped = false;
            } else if character == '\\' {
                escaped = true;
            } else if character == active {
                quote = None;
            }
            index += 1;
            continue;
        }
        if matches!(character, '\'' | '"') {
            quote = Some(character);
            output.push(character);
            index += 1;
            continue;
        }
        match character {
            '[' => {
                bracket_depth += 1;
                output.push(character);
                index += 1;
            }
            ']' => {
                bracket_depth = bracket_depth.saturating_sub(1);
                output.push(character);
                index += 1;
            }
            ':' if bracket_depth > 0 => {
                index += 1;
                while chars.get(index).is_some_and(|c| c.is_whitespace()) {
                    index += 1;
                }
                if chars.get(index) != Some(&'$')
                    || chars.get(index + 1) != Some(&'i')
                    || chars
                        .get(index + 2)
                        .is_some_and(|character| character.is_alphanumeric() || *character == '_')
                {
                    return Err(unsupported(
                        EvidenceOperation::ModelParsing,
                        "Vampire finite-model checker admits only the single default `$i` sort"
                            .to_owned(),
                    ));
                }
                index += 2;
            }
            _ => {
                output.push(character);
                index += 1;
            }
        }
    }
    Ok(output)
}

#[cfg(test)]
#[path = "external_evidence.tests.rs"]
mod tests;
