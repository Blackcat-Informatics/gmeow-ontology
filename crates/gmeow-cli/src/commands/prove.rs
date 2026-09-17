// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Installed external-FOF consumer: one native source composition, one compiler
//! admission, one deterministic projection, and bounded prover subprocesses.

use std::collections::BTreeSet;
use std::io::{Read, Write as _};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::time::{Duration, Instant};

use gmeow_cli_core::Reporter;
use gmeow_logic::external_evidence::{
    ArtifactCheck, ArtifactCheckDisposition, SzsArtifact, check_external_artifact,
    extract_szs_artifacts,
};
use gmeow_logic_compile::frontend::{
    LogicParseError, PreparedLogicSource, SourceAdmission, SourceBase, SourceBaseOrigin,
    SourceDocument, SourceSelection,
};
use gmeow_logic_compile::tptp::{
    FofProjection, FofProjectionBlocker, FofProjectionStatus, FofSentence, FofTask, SzsAdmission,
    SzsOutcome, admit_szs_transcript, project_tptp_fof,
};
use purrdf::{CompositeDatasetView, DatasetView, RdfDataset, TermRef, ViewLimits};
use serde::Serialize;

use super::reasoning_report::{
    DecisionClass, EngineIdentity, EvidenceGrade, ExecutableIdentity, GateAdmission, InputIdentity,
    PreservationReport, ProgramIdentity, ReasoningBoundary, ReasoningMetrics, ReasoningOperation,
    ReasoningReportCore, ReportDiagnostic, digest_file,
};
use super::{emit_error, emit_warning};
use crate::{OutputFormat, ProverChoice};

const MAX_SOURCE_DOCUMENTS: usize = 4_096;
const MAX_PROVER_OUTPUT_BYTES: usize = 1_048_576;
const MAX_VERSION_OUTPUT_BYTES: usize = 65_536;
const PROCESS_POLL_INTERVAL: Duration = Duration::from_millis(10);
const VERSION_DEADLINE: Duration = Duration::from_secs(5);

const FOF_PROFILE: &str = "https://blackcatinformatics.ca/logic/profile/tptp-fof-source-v1";

#[derive(Debug, Clone, Serialize)]
struct ProjectionReceipt {
    status: FofProjectionStatus,
    profile_id: String,
    source_digest: String,
    program_digest: String,
    selection_digest: String,
    problem_digest: Option<String>,
    problem_bytes: Option<usize>,
    sentences: Vec<FofSentence>,
    blockers: Vec<FofProjectionBlocker>,
}

impl From<&FofProjection> for ProjectionReceipt {
    fn from(projection: &FofProjection) -> Self {
        Self {
            status: projection.status,
            profile_id: projection.profile_id.clone(),
            source_digest: projection.source_digest.clone(),
            program_digest: projection.program_digest.clone(),
            selection_digest: projection.selection_digest.clone(),
            problem_digest: projection.problem_digest.clone(),
            problem_bytes: projection.problem.as_ref().map(String::len),
            sentences: projection.sentences.clone(),
            blockers: projection.blockers.clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
struct CapturedStream {
    digest: String,
    total_bytes: u64,
    retained_bytes: usize,
    truncated: bool,
    utf8_valid: bool,
    text: String,
}

#[derive(Debug, Clone, Serialize)]
struct ProcessReceipt {
    pass: String,
    arguments: Vec<String>,
    elapsed_ms: u64,
    exit_code: Option<i32>,
    signal: Option<i32>,
    timed_out: bool,
    stdout: CapturedStream,
    stderr: CapturedStream,
    szs: Option<SzsAdmission>,
    artifacts: Vec<CheckedArtifactReceipt>,
}

#[derive(Debug, Clone, Serialize)]
struct CheckedArtifactReceipt {
    artifact: SzsArtifact,
    check: ArtifactCheck,
}

#[derive(Debug, Clone, Serialize)]
struct ExternalExecution {
    prover: String,
    executable: ExecutableIdentity,
    deadline_ms_per_pass: u64,
    output_limit_bytes_per_stream: usize,
    problem_file: String,
    passes: Vec<ProcessReceipt>,
}

#[derive(Debug, Clone, Serialize)]
struct ProveReport {
    #[serde(flatten)]
    common: ReasoningReportCore,
    task: FofTask,
    admission: Option<SourceAdmission>,
    projection: Option<ProjectionReceipt>,
    execution: Option<ExternalExecution>,
}

impl ProveReport {
    fn blank(decision: DecisionClass, code: &str, detail: impl Into<String>) -> Self {
        let detail = detail.into();
        Self {
            common: ReasoningReportCore::refused(
                ReasoningOperation::AxiomConsistency,
                decision,
                decision.as_str(),
                if decision == DecisionClass::Malformed {
                    "invalid"
                } else {
                    "valid"
                },
                Vec::new(),
                ProgramIdentity::selected(FOF_PROFILE, &["source-admission-pending"]),
                EngineIdentity::external_host(),
                ReportDiagnostic {
                    code: code.to_owned(),
                    detail,
                },
            ),
            task: FofTask::AxiomConsistency,
            admission: None,
            projection: None,
            execution: None,
        }
    }

    fn execution_failure(&mut self, code: &str, detail: impl Into<String>) {
        self.common.verdict = "execution-failure".to_owned();
        self.common.decision = DecisionClass::ExecutionFailure;
        self.common.evaluation_status = "failed".to_owned();
        self.common.completeness = "unknown".to_owned();
        self.common.information_state = "not-evaluated".to_owned();
        self.common.evidence_grade = EvidenceGrade::Refused;
        self.common.gate_admission = GateAdmission::Refused;
        self.common.diagnostics.push(ReportDiagnostic {
            code: code.to_owned(),
            detail: detail.into(),
        });
    }
}

struct LoadedTheory {
    identities: Vec<InputIdentity>,
    // The source-local IDs retained in provenance bindings belong to these exact
    // immutable parses. Keep them alive through admission and projection.
    _documents: Vec<Arc<RdfDataset>>,
    theory: gmeow_logic_compile::frontend::CompiledTheory,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ProverFlavor {
    EProver,
    Vampire,
}

impl ProverFlavor {
    fn as_str(self) -> &'static str {
        match self {
            Self::EProver => "eprover",
            Self::Vampire => "vampire",
        }
    }
}

struct ResolvedProver {
    flavor: ProverFlavor,
    path: PathBuf,
    digest: String,
}

struct StreamBytes {
    digest: String,
    total_bytes: u64,
    retained: Vec<u8>,
    truncated: bool,
}

impl StreamBytes {
    fn into_report(self) -> CapturedStream {
        let utf8_valid = std::str::from_utf8(&self.retained).is_ok();
        let text = String::from_utf8_lossy(&self.retained).into_owned();
        CapturedStream {
            digest: self.digest,
            total_bytes: self.total_bytes,
            retained_bytes: self.retained.len(),
            truncated: self.truncated,
            utf8_valid,
            text,
        }
    }
}

struct RawProcessReceipt {
    elapsed_ms: u64,
    exit_code: Option<i32>,
    signal: Option<i32>,
    timed_out: bool,
    stdout: StreamBytes,
    stderr: StreamBytes,
}

/// Run the complete installed `gmeow prove` operation.
#[allow(clippy::too_many_arguments)]
pub(crate) fn prove(
    reporter: &dyn Reporter,
    inputs: &[PathBuf],
    dir: Option<&Path>,
    prover: ProverChoice,
    timeout_secs: u64,
    out: Option<&Path>,
    format: OutputFormat,
    gate: bool,
    evidence_out: Option<&Path>,
) -> i32 {
    let paths = match source_paths(inputs, dir) {
        Ok(paths) => paths,
        Err((code, detail)) => {
            return publish_failure(
                reporter,
                ProveReport::blank(DecisionClass::Malformed, code, detail),
                format,
                gate,
                evidence_out,
            );
        }
    };
    let loaded = match load_theory(&paths) {
        Ok(loaded) => loaded,
        Err((code, detail, identities)) => {
            let mut report = ProveReport::blank(DecisionClass::Malformed, code, detail);
            report.common.inputs = identities;
            return publish_failure(reporter, report, format, gate, evidence_out);
        }
    };

    let selection = SourceSelection::all_classical_roots(&loaded.theory);
    let admission = SourceAdmission::classical_fof(&loaded.theory, selection);
    let projection = project_tptp_fof(&loaded.theory, &admission);
    let boundaries: Vec<_> = projection
        .blockers
        .iter()
        .map(|blocker| ReasoningBoundary {
            code: blocker.code.clone(),
            detail: blocker.reason.clone(),
        })
        .collect();
    let mut report = ProveReport {
        common: ReasoningReportCore {
            schema_version: 1,
            operation: ReasoningOperation::AxiomConsistency,
            verdict: "unsupported".to_owned(),
            decision: DecisionClass::Unsupported,
            input_status: "valid".to_owned(),
            evaluation_status: "unsupported".to_owned(),
            completeness: "unknown".to_owned(),
            information_state: "not-evaluated".to_owned(),
            preservation: PreservationReport::unsupported(
                boundaries.iter().map(|row| row.code.clone()).collect(),
            ),
            evidence_grade: EvidenceGrade::Refused,
            gate_admission: GateAdmission::Refused,
            declared_fragment: Some(projection.profile_id.clone()),
            inputs: loaded.identities.clone(),
            program: ProgramIdentity {
                profile: projection.profile_id.clone(),
                content_digest: projection.program_digest.clone(),
            },
            engine: EngineIdentity::external_host(),
            metrics: ReasoningMetrics::default(),
            boundaries,
            diagnostics: Vec::new(),
        },
        task: FofTask::AxiomConsistency,
        admission: Some(admission),
        projection: Some(ProjectionReceipt::from(&projection)),
        execution: None,
    };
    if projection.status != FofProjectionStatus::Complete {
        report
            .common
            .diagnostics
            .extend(projection.blockers.iter().map(|blocker| ReportDiagnostic {
                code: blocker.code.clone(),
                detail: blocker.reason.clone(),
            }));
        emit_warning(
            reporter,
            "gmeow-cli.prove.unsupported",
            format!(
                "source admission refused the external problem with {} blocker(s)",
                projection.blockers.len()
            ),
        );
        return publish_report(report, format, gate, evidence_out, reporter);
    }
    report.common.preservation = PreservationReport::exact();
    report.common.boundaries.clear();

    let problem = projection
        .problem
        .as_deref()
        .expect("complete projection has problem text");
    let problem_digest = projection
        .problem_digest
        .as_deref()
        .expect("complete projection has problem identity");
    let (problem_path, temp_guard) = match write_problem(problem, problem_digest, out) {
        Ok(result) => result,
        Err(detail) => {
            report.execution_failure("PROBLEM_WRITE_FAILED", detail);
            return publish_failure(reporter, report, format, gate, evidence_out);
        }
    };
    let resolved = match resolve_prover(prover) {
        Ok(resolved) => resolved,
        Err(detail) => {
            report.execution_failure("PROVER_NOT_AVAILABLE", detail);
            return publish_failure(reporter, report, format, gate, evidence_out);
        }
    };
    let version = match prover_version(&resolved) {
        Ok(version) => version,
        Err(detail) => {
            report.execution_failure("PROVER_VERSION_UNAVAILABLE", detail);
            return publish_failure(reporter, report, format, gate, evidence_out);
        }
    };
    let executable = ExecutableIdentity {
        path: resolved.path.display().to_string(),
        content_digest: resolved.digest.clone(),
        version,
    };
    report.common.engine = EngineIdentity {
        implementation: format!("external-{}", resolved.flavor.as_str()),
        version: executable.version.clone(),
        executable: Some(executable.clone()),
    };
    let mut execution = ExternalExecution {
        prover: resolved.flavor.as_str().to_owned(),
        executable,
        deadline_ms_per_pass: timeout_secs.saturating_mul(1_000),
        output_limit_bytes_per_stream: MAX_PROVER_OUTPUT_BYTES,
        problem_file: problem_path.display().to_string(),
        passes: Vec::new(),
    };
    let aliases = problem_aliases(&problem_path, problem_digest);
    let deadline = Duration::from_secs(timeout_secs);
    let pass_specs = prover_passes(resolved.flavor, timeout_secs);
    let mut decisive_szs: Option<SzsAdmission> = None;
    let mut last_szs: Option<SzsAdmission> = None;
    for (pass_name, arguments) in pass_specs {
        let raw = match run_bounded_process(
            &resolved.path,
            &arguments,
            Some(&problem_path),
            deadline,
            MAX_PROVER_OUTPUT_BYTES,
        ) {
            Ok(raw) => raw,
            Err(detail) => {
                report.execution_failure("PROVER_SPAWN_FAILED", detail);
                report.execution = Some(execution);
                return publish_failure(reporter, report, format, gate, evidence_out);
            }
        };
        let stdout = raw.stdout.into_report();
        let stderr = raw.stderr.into_report();
        let mut receipt = ProcessReceipt {
            pass: pass_name,
            arguments,
            elapsed_ms: raw.elapsed_ms,
            exit_code: raw.exit_code,
            signal: raw.signal,
            timed_out: raw.timed_out,
            stdout,
            stderr,
            szs: None,
            artifacts: Vec::new(),
        };
        if receipt.timed_out {
            report.execution_failure(
                "PROVER_PARENT_DEADLINE",
                format!(
                    "{} exceeded the parent wall deadline of {timeout_secs}s and was reaped",
                    resolved.path.display()
                ),
            );
            execution.passes.push(receipt);
            report.execution = Some(execution);
            return publish_failure(reporter, report, format, gate, evidence_out);
        }
        if receipt.exit_code != Some(0) || receipt.signal.is_some() {
            report.execution_failure(
                "PROVER_CHILD_FAILED",
                format!(
                    "{} exited with code {:?} and signal {:?}",
                    resolved.path.display(),
                    receipt.exit_code,
                    receipt.signal
                ),
            );
            execution.passes.push(receipt);
            report.execution = Some(execution);
            return publish_failure(reporter, report, format, gate, evidence_out);
        }
        if receipt.stdout.truncated || receipt.stderr.truncated {
            report.execution_failure(
                "PROVER_OUTPUT_LIMIT",
                format!(
                    "prover output exceeded the {}-byte per-stream evidence bound",
                    MAX_PROVER_OUTPUT_BYTES
                ),
            );
            execution.passes.push(receipt);
            report.execution = Some(execution);
            return publish_failure(reporter, report, format, gate, evidence_out);
        }
        let szs = match admit_szs_transcript(
            &receipt.stdout.text,
            &receipt.stderr.text,
            &aliases,
            FofTask::AxiomConsistency,
        ) {
            Ok(szs) => szs,
            Err(error) => {
                report.execution_failure(&error.code, error.detail);
                execution.passes.push(receipt);
                report.execution = Some(execution);
                return publish_failure(reporter, report, format, gate, evidence_out);
            }
        };
        let outcome = szs.outcome;
        receipt.szs = Some(szs.clone());
        let artifacts =
            match extract_szs_artifacts(&receipt.stdout.text, &receipt.stderr.text, &aliases) {
                Ok(artifacts) => artifacts,
                Err(error) => {
                    report.execution_failure(&error.code, error.detail);
                    execution.passes.push(receipt);
                    report.execution = Some(execution);
                    return publish_failure(reporter, report, format, gate, evidence_out);
                }
            };
        receipt.artifacts = artifacts
            .into_iter()
            .map(|artifact| CheckedArtifactReceipt {
                check: check_external_artifact(&artifact, &projection.sentences, problem_digest),
                artifact,
            })
            .collect();
        if let Some(invalid) = receipt
            .artifacts
            .iter()
            .find(|artifact| artifact.check.disposition == ArtifactCheckDisposition::Invalid)
        {
            report.execution_failure(&invalid.check.code, invalid.check.detail.clone());
            execution.passes.push(receipt);
            report.execution = Some(execution);
            return publish_failure(reporter, report, format, gate, evidence_out);
        }
        if let Some(mismatched) = receipt.artifacts.iter().find(|artifact| {
            artifact_concludes_outcome(&artifact.artifact.kind)
                && !artifact_matches_outcome(&artifact.artifact.kind, outcome)
        }) {
            report.execution_failure(
                "SZS_ARTIFACT_OUTCOME_MISMATCH",
                format!(
                    "SZS status {} is incompatible with the captured {} artifact",
                    szs.status, mismatched.artifact.kind
                ),
            );
            execution.passes.push(receipt);
            report.execution = Some(execution);
            return publish_failure(reporter, report, format, gate, evidence_out);
        }
        if outcome != SzsOutcome::Undecided {
            if let Some(previous) = decisive_szs.as_ref()
                && previous.outcome != outcome
            {
                report.execution_failure(
                    "CONFLICTING_PROVER_PASS_OUTCOME",
                    format!(
                        "separate prover passes reported incompatible outcomes {:?} and {:?}",
                        previous.outcome, outcome
                    ),
                );
                execution.passes.push(receipt);
                report.execution = Some(execution);
                return publish_failure(reporter, report, format, gate, evidence_out);
            }
            decisive_szs.get_or_insert_with(|| szs.clone());
        }
        let has_matching_certificate = receipt.artifacts.iter().any(|artifact| {
            artifact_matches_outcome(&artifact.artifact.kind, outcome)
                && artifact.check.disposition == ArtifactCheckDisposition::Certificate
        });
        execution.passes.push(receipt);
        last_szs = Some(szs);
        let needs_vampire_model_pass = resolved.flavor == ProverFlavor::Vampire
            && outcome == SzsOutcome::Consistent
            && !has_matching_certificate;
        if outcome == SzsOutcome::Inconsistent
            || (outcome == SzsOutcome::Consistent && !needs_vampire_model_pass)
        {
            break;
        }
    }
    drop(temp_guard);

    match digest_file(&resolved.path) {
        Ok(after) if after == resolved.digest => {}
        Ok(after) => {
            report.execution_failure(
                "PROVER_EXECUTABLE_CHANGED",
                format!(
                    "selected executable changed during the run ({} -> {after})",
                    resolved.digest
                ),
            );
            report.execution = Some(execution);
            return publish_failure(reporter, report, format, gate, evidence_out);
        }
        Err(detail) => {
            report.execution_failure("PROVER_EXECUTABLE_RECHECK_FAILED", detail);
            report.execution = Some(execution);
            return publish_failure(reporter, report, format, gate, evidence_out);
        }
    }

    let final_szs = decisive_szs.or(last_szs).expect("at least one prover pass");
    let (verdict, decision, information_state, completeness) = match final_szs.outcome {
        SzsOutcome::Consistent => (
            "consistent",
            DecisionClass::Positive,
            "supported",
            "unknown",
        ),
        SzsOutcome::Inconsistent => (
            "inconsistent",
            DecisionClass::Negative,
            "opposed",
            "unknown",
        ),
        SzsOutcome::Undecided => (
            "undecided",
            DecisionClass::Undecided,
            "undetermined",
            "incomplete",
        ),
    };
    report.common.verdict = verdict.to_owned();
    report.common.decision = decision;
    report.common.evaluation_status = "completed".to_owned();
    report.common.completeness = completeness.to_owned();
    report.common.information_state = information_state.to_owned();
    let checked = execution
        .passes
        .iter()
        .filter(|pass| {
            pass.szs
                .as_ref()
                .is_some_and(|szs| szs.outcome == final_szs.outcome)
        })
        .flat_map(|pass| &pass.artifacts)
        .filter(|artifact| artifact_matches_outcome(&artifact.artifact.kind, final_szs.outcome))
        .max_by_key(|artifact| artifact_disposition_rank(artifact.check.disposition));
    if let Some(checked) = checked {
        report.common.metrics.decisions = Some(checked.check.evaluations);
        report.common.metrics.steps = Some(checked.check.units);
        report.common.metrics.budget = checked.check.budget;
        report.common.diagnostics.push(ReportDiagnostic {
            code: checked.check.code.clone(),
            detail: checked.check.detail.clone(),
        });
        if checked.check.disposition == ArtifactCheckDisposition::Certificate {
            report.common.evidence_grade = EvidenceGrade::Certificate;
            report.common.gate_admission = GateAdmission::Certificate;
            report.common.completeness = "complete".to_owned();
        } else {
            report.common.evidence_grade = EvidenceGrade::Attestation;
            report.common.gate_admission = GateAdmission::Attestation;
        }
    } else {
        report.common.evidence_grade = EvidenceGrade::Attestation;
        report.common.gate_admission = GateAdmission::Attestation;
        report.common.diagnostics.push(ReportDiagnostic {
            code: "MISSING_CHECKABLE_EXTERNAL_ARTIFACT".to_owned(),
            detail: match final_szs.outcome {
                SzsOutcome::Consistent => {
                    "the satisfiable status carries no admitted finite-model artifact"
                }
                SzsOutcome::Inconsistent => {
                    "the unsatisfiable status carries no admitted refutation artifact"
                }
                SzsOutcome::Undecided => "an undecided status has no operation-concluding artifact",
            }
            .to_owned(),
        });
    }
    report.execution = Some(execution);
    publish_report(report, format, gate, evidence_out, reporter)
}

fn artifact_matches_outcome(kind: &str, outcome: SzsOutcome) -> bool {
    match outcome {
        SzsOutcome::Consistent => matches!(kind, "FiniteModel" | "Model"),
        SzsOutcome::Inconsistent => matches!(kind, "Proof" | "Refutation" | "CNFRefutation"),
        SzsOutcome::Undecided => false,
    }
}

fn artifact_concludes_outcome(kind: &str) -> bool {
    matches!(
        kind,
        "FiniteModel" | "Model" | "Proof" | "Refutation" | "CNFRefutation"
    )
}

fn artifact_disposition_rank(disposition: ArtifactCheckDisposition) -> u8 {
    match disposition {
        ArtifactCheckDisposition::Certificate => 3,
        ArtifactCheckDisposition::Structural => 2,
        ArtifactCheckDisposition::Unsupported => 1,
        ArtifactCheckDisposition::Invalid => 0,
    }
}

fn source_paths(
    inputs: &[PathBuf],
    dir: Option<&Path>,
) -> Result<Vec<PathBuf>, (&'static str, String)> {
    let mut candidates = inputs.to_vec();
    if let Some(root) = dir {
        candidates.extend(collect_semantic_sources(root)?);
    }
    if candidates.is_empty() {
        return Err((
            "NO_SOURCE_INPUT",
            "pass one or more RDF files and/or --dir <corpus-root>".to_owned(),
        ));
    }
    let mut seen = BTreeSet::new();
    let mut paths = Vec::new();
    for path in candidates {
        let canonical = std::fs::canonicalize(&path).map_err(|error| {
            (
                "SOURCE_PATH_UNAVAILABLE",
                format!("cannot resolve {}: {error}", path.display()),
            )
        })?;
        if !canonical.is_file() {
            return Err((
                "SOURCE_NOT_FILE",
                format!("{} is not a regular file", canonical.display()),
            ));
        }
        if seen.insert(canonical.clone()) {
            paths.push(canonical);
        }
    }
    if paths.len() > MAX_SOURCE_DOCUMENTS {
        return Err((
            "SOURCE_COUNT_LIMIT",
            format!(
                "selected {} documents; the bounded limit is {MAX_SOURCE_DOCUMENTS}",
                paths.len()
            ),
        ));
    }
    paths.sort();
    Ok(paths)
}

fn collect_semantic_sources(root: &Path) -> Result<Vec<PathBuf>, (&'static str, String)> {
    let root = std::fs::canonicalize(root).map_err(|error| {
        (
            "SOURCE_ROOT_UNAVAILABLE",
            format!("cannot resolve {}: {error}", root.display()),
        )
    })?;
    if !root.is_dir() {
        return Err((
            "SOURCE_ROOT_NOT_DIRECTORY",
            format!("{} is not a directory", root.display()),
        ));
    }
    let root_display = root.display().to_string();
    let mut pending = vec![root];
    let mut sources = Vec::new();
    while let Some(directory) = pending.pop() {
        let entries = std::fs::read_dir(&directory).map_err(|error| {
            (
                "SOURCE_ROOT_READ_FAILED",
                format!("cannot inspect {}: {error}", directory.display()),
            )
        })?;
        for entry in entries {
            let entry = entry.map_err(|error| {
                (
                    "SOURCE_ROOT_READ_FAILED",
                    format!("cannot inspect {}: {error}", directory.display()),
                )
            })?;
            let file_type = entry.file_type().map_err(|error| {
                (
                    "SOURCE_ROOT_READ_FAILED",
                    format!("cannot inspect {}: {error}", entry.path().display()),
                )
            })?;
            let path = entry.path();
            if file_type.is_dir() {
                pending.push(path);
            } else if file_type.is_file() && is_semantic_source(&path) {
                sources.push(path);
                if sources.len() > MAX_SOURCE_DOCUMENTS {
                    return Err((
                        "SOURCE_COUNT_LIMIT",
                        format!(
                            "corpus contains more than {MAX_SOURCE_DOCUMENTS} semantic documents"
                        ),
                    ));
                }
            }
        }
    }
    sources.sort();
    if sources.is_empty() {
        return Err((
            "EMPTY_SOURCE_ROOT",
            format!("no module.ttl or *.logic.ttl source exists below {root_display}"),
        ));
    }
    Ok(sources)
}

fn is_semantic_source(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name == "module.ttl" || name.ends_with(".logic.ttl"))
}

fn load_theory(
    paths: &[PathBuf],
) -> Result<LoadedTheory, (&'static str, String, Vec<InputIdentity>)> {
    let mut receipts = Vec::with_capacity(paths.len());
    let mut identities = Vec::with_capacity(paths.len());
    let mut documents = Vec::with_capacity(paths.len());
    for path in paths {
        let bytes = std::fs::read(path).map_err(|error| {
            (
                "SOURCE_READ_FAILED",
                format!("cannot read {}: {error}", path.display()),
                identities.clone(),
            )
        })?;
        let base = format!("file://{}", path.display());
        let media = rdf_media(path).ok_or_else(|| {
            identities.push(InputIdentity::from_bytes(
                path,
                "selected-semantic-source",
                "unknown",
                Some(base.clone()),
                &bytes,
            ));
            (
                "SOURCE_SYNTAX_UNKNOWN",
                format!(
                    "cannot infer RDF syntax for {}; expected .ttl/.nt/.nq/.rdf/.owl/.xml/.trig",
                    path.display()
                ),
                identities.clone(),
            )
        })?;
        identities.push(InputIdentity::from_bytes(
            path,
            "selected-semantic-source",
            media,
            Some(base.clone()),
            &bytes,
        ));
        let dataset = purrdf::parse_dataset(&bytes, media, Some(&base)).map_err(|error| {
            (
                "SOURCE_PARSE_FAILED",
                format!("cannot parse {} as {media}: {error}", path.display()),
                identities.clone(),
            )
        })?;
        receipts.push(SourceDocument {
            path: path.display().to_string(),
            content_digest: blake3::hash(&bytes).to_hex().to_string(),
            role: "selected-semantic-source".to_owned(),
            base: Some(SourceBase {
                iri: base,
                origin: SourceBaseOrigin::Caller,
            }),
        });
        documents.push(dataset);
    }
    let composite = CompositeDatasetView::new(
        documents.clone(),
        ViewLimits {
            max_sources: MAX_SOURCE_DOCUMENTS,
            ..ViewLimits::default()
        },
    )
    .map_err(|error| {
        (
            "SOURCE_COMPOSITION_REFUSED",
            format!("cannot compose selected documents: {error}"),
            identities.clone(),
        )
    })?;
    let materialized = composite.materialize().map_err(|error| {
        (
            "SOURCE_MATERIALIZATION_REFUSED",
            format!("cannot materialize selected documents: {error}"),
            identities.clone(),
        )
    })?;
    let mut prepared = PreparedLogicSource::new(&materialized)
        .map_err(|error| ("SOURCE_PREPARATION_FAILED", error.0, identities.clone()))?;
    for (index, (receipt, original)) in receipts.iter().zip(&documents).enumerate() {
        prepared
            .record_document(receipt.clone(), original, |term| {
                let mapped = match original.resolve(term) {
                    TermRef::Iri(iri) => materialized.term_id_by_iri(iri),
                    _ => {
                        let value = composite.term_value(composite.source_id(index, term));
                        materialized.term_id_by_value(&value)
                    }
                };
                mapped.ok_or_else(|| {
                    LogicParseError(format!(
                        "source term in {:?} has no materialized binding",
                        receipt.path
                    ))
                })
            })
            .map_err(|error| ("SOURCE_BINDING_FAILED", error.0, identities.clone()))?;
    }
    let theory = prepared
        .into_compiled(None)
        .map_err(|error| ("SOURCE_COMPILATION_FAILED", error.0, identities.clone()))?;
    Ok(LoadedTheory {
        identities,
        _documents: documents,
        theory,
    })
}

fn rdf_media(path: &Path) -> Option<&'static str> {
    match path.extension()?.to_str()?.to_ascii_lowercase().as_str() {
        "ttl" => Some("text/turtle"),
        "nt" => Some("application/n-triples"),
        "nq" => Some("application/n-quads"),
        "trig" => Some("application/trig"),
        "rdf" | "owl" | "xml" => Some("application/rdf+xml"),
        _ => None,
    }
}

fn write_problem(
    problem: &str,
    digest: &str,
    out: Option<&Path>,
) -> Result<(PathBuf, Option<tempfile::TempPath>), String> {
    if let Some(path) = out {
        std::fs::write(path, problem)
            .map_err(|error| format!("cannot write {}: {error}", path.display()))?;
        let read_back = std::fs::read(path)
            .map_err(|error| format!("cannot verify {}: {error}", path.display()))?;
        if read_back != problem.as_bytes() {
            return Err(format!(
                "problem file {} does not contain the emitted bytes",
                path.display()
            ));
        }
        return Ok((path.to_path_buf(), None));
    }
    let prefix = format!("gmeow_{}_", &digest[..digest.len().min(16)]);
    let mut file = tempfile::Builder::new()
        .prefix(&prefix)
        .suffix(".p")
        .tempfile()
        .map_err(|error| format!("cannot create managed problem file: {error}"))?;
    file.write_all(problem.as_bytes())
        .map_err(|error| format!("cannot write managed problem file: {error}"))?;
    file.flush()
        .map_err(|error| format!("cannot flush managed problem file: {error}"))?;
    let temp = file.into_temp_path();
    Ok((temp.to_path_buf(), Some(temp)))
}

fn resolve_prover(choice: ProverChoice) -> Result<ResolvedProver, String> {
    let (flavor, path) = if let Some(path) = std::env::var_os("GMEOW_PROVER_PATH") {
        let path = PathBuf::from(path);
        let flavor = match choice {
            ProverChoice::Eprover => ProverFlavor::EProver,
            ProverChoice::Vampire => ProverFlavor::Vampire,
            ProverChoice::Auto => path
                .file_name()
                .and_then(|name| name.to_str())
                .filter(|name| name.to_ascii_lowercase().contains("vampire"))
                .map_or(ProverFlavor::EProver, |_| ProverFlavor::Vampire),
        };
        (flavor, path)
    } else {
        match choice {
            ProverChoice::Eprover => (
                ProverFlavor::EProver,
                find_on_path("eprover").ok_or_else(|| "no eprover on PATH".to_owned())?,
            ),
            ProverChoice::Vampire => (
                ProverFlavor::Vampire,
                find_on_path("vampire").ok_or_else(|| "no vampire on PATH".to_owned())?,
            ),
            ProverChoice::Auto => find_on_path("eprover")
                .map(|path| (ProverFlavor::EProver, path))
                .or_else(|| find_on_path("vampire").map(|path| (ProverFlavor::Vampire, path)))
                .ok_or_else(|| "no eprover or vampire on PATH".to_owned())?,
        }
    };
    let path = std::fs::canonicalize(&path)
        .map_err(|error| format!("cannot resolve prover {}: {error}", path.display()))?;
    if !path.is_file() {
        return Err(format!("prover {} is not a regular file", path.display()));
    }
    let digest = digest_file(&path)?;
    Ok(ResolvedProver {
        flavor,
        path,
        digest,
    })
}

fn find_on_path(name: &str) -> Option<PathBuf> {
    std::env::var_os("PATH").and_then(|paths| {
        std::env::split_paths(&paths)
            .map(|directory| directory.join(name))
            .find(|candidate| candidate.is_file())
    })
}

fn prover_version(prover: &ResolvedProver) -> Result<String, String> {
    let arguments = vec!["--version".to_owned()];
    let raw = run_bounded_process(
        &prover.path,
        &arguments,
        None,
        VERSION_DEADLINE,
        MAX_VERSION_OUTPUT_BYTES,
    )?;
    if raw.timed_out {
        return Err(format!(
            "{} --version exceeded {VERSION_DEADLINE:?}",
            prover.path.display()
        ));
    }
    if raw.exit_code != Some(0) || raw.signal.is_some() {
        return Err(format!(
            "{} --version exited with code {:?} and signal {:?}",
            prover.path.display(),
            raw.exit_code,
            raw.signal
        ));
    }
    if raw.stdout.truncated || raw.stderr.truncated {
        return Err("prover version output exceeded its evidence bound".to_owned());
    }
    let stdout = String::from_utf8_lossy(&raw.stdout.retained);
    let stderr = String::from_utf8_lossy(&raw.stderr.retained);
    let version = format!("{stdout}\n{stderr}").trim().to_owned();
    if version.is_empty() {
        return Err("prover --version produced no identity text".to_owned());
    }
    Ok(version)
}

fn prover_passes(flavor: ProverFlavor, timeout_secs: u64) -> Vec<(String, Vec<String>)> {
    match flavor {
        ProverFlavor::EProver => vec![(
            "saturation".to_owned(),
            vec![
                "--auto".to_owned(),
                "--proof-object".to_owned(),
                format!("--cpu-limit={timeout_secs}"),
            ],
        )],
        ProverFlavor::Vampire => vec![
            (
                "refutation".to_owned(),
                vec![
                    "--mode".to_owned(),
                    "portfolio".to_owned(),
                    "--schedule".to_owned(),
                    "casc".to_owned(),
                    "--proof".to_owned(),
                    "tptp".to_owned(),
                    "--output_mode".to_owned(),
                    "szs".to_owned(),
                    "--input_syntax".to_owned(),
                    "tptp".to_owned(),
                    "-t".to_owned(),
                    timeout_secs.to_string(),
                ],
            ),
            (
                "finite-model".to_owned(),
                vec![
                    "--mode".to_owned(),
                    "portfolio".to_owned(),
                    "--schedule".to_owned(),
                    "casc_sat".to_owned(),
                    "--proof".to_owned(),
                    "tptp".to_owned(),
                    "--output_mode".to_owned(),
                    "szs".to_owned(),
                    "--input_syntax".to_owned(),
                    "tptp".to_owned(),
                    "-t".to_owned(),
                    timeout_secs.to_string(),
                ],
            ),
        ],
    }
}

fn problem_aliases(path: &Path, digest: &str) -> BTreeSet<String> {
    let mut aliases = BTreeSet::from([format!("gmeow_{digest}"), format!("gmeow_{digest}.p")]);
    if let Some(name) = path.file_name().and_then(|name| name.to_str()) {
        aliases.insert(name.to_owned());
    }
    if let Some(stem) = path.file_stem().and_then(|name| name.to_str()) {
        aliases.insert(stem.to_owned());
    }
    aliases
}

fn run_bounded_process(
    binary: &Path,
    arguments: &[String],
    problem: Option<&Path>,
    deadline: Duration,
    output_limit: usize,
) -> Result<RawProcessReceipt, String> {
    let mut command = Command::new(binary);
    command
        .args(arguments)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(problem) = problem {
        command.arg(problem);
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt as _;
        command.process_group(0);
    }
    let started = Instant::now();
    let mut child = command
        .spawn()
        .map_err(|error| format!("cannot run {}: {error}", binary.display()))?;
    let Some(stdout) = child.stdout.take() else {
        kill_process_tree(&mut child);
        let _ = child.wait();
        return Err("selected child has no stdout pipe".to_owned());
    };
    let Some(stderr) = child.stderr.take() else {
        kill_process_tree(&mut child);
        let _ = child.wait();
        return Err("selected child has no stderr pipe".to_owned());
    };
    let stdout_reader = std::thread::spawn(move || drain_stream(stdout, output_limit));
    let stderr_reader = std::thread::spawn(move || drain_stream(stderr, output_limit));
    let expires = started + deadline;
    let mut timed_out = false;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() < expires => std::thread::sleep(PROCESS_POLL_INTERVAL),
            Ok(None) => {
                timed_out = true;
                kill_process_tree(&mut child);
                break child.wait().map_err(|error| {
                    format!("cannot reap timed-out child {}: {error}", binary.display())
                })?;
            }
            Err(error) => {
                kill_process_tree(&mut child);
                let _ = child.wait();
                return Err(format!(
                    "cannot poll selected child {}: {error}",
                    binary.display()
                ));
            }
        }
    };
    let stdout = stdout_reader
        .join()
        .map_err(|_| "selected child stdout reader panicked".to_owned())?
        .map_err(|error| format!("cannot read selected child stdout: {error}"))?;
    let stderr = stderr_reader
        .join()
        .map_err(|_| "selected child stderr reader panicked".to_owned())?
        .map_err(|error| format!("cannot read selected child stderr: {error}"))?;
    #[cfg(unix)]
    use std::os::unix::process::ExitStatusExt as _;
    Ok(RawProcessReceipt {
        elapsed_ms: u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
        exit_code: status.code(),
        #[cfg(unix)]
        signal: status.signal(),
        #[cfg(not(unix))]
        signal: None,
        timed_out,
        stdout,
        stderr,
    })
}

fn drain_stream(mut stream: impl Read, limit: usize) -> std::io::Result<StreamBytes> {
    let mut hasher = blake3::Hasher::new();
    let mut retained = Vec::with_capacity(limit.min(64 * 1024));
    let mut total = 0_u64;
    let mut buffer = [0_u8; 16 * 1024];
    loop {
        let read = stream.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
        total = total.saturating_add(u64::try_from(read).unwrap_or(u64::MAX));
        let available = limit.saturating_sub(retained.len());
        retained.extend_from_slice(&buffer[..read.min(available)]);
    }
    Ok(StreamBytes {
        digest: hasher.finalize().to_hex().to_string(),
        total_bytes: total,
        truncated: total > u64::try_from(retained.len()).unwrap_or(u64::MAX),
        retained,
    })
}

fn kill_process_tree(child: &mut std::process::Child) {
    #[cfg(unix)]
    {
        if let Ok(pid) = i32::try_from(child.id()) {
            // The child was placed in its own process group before spawn. Killing
            // that group closes descendant-held pipes before reader threads join.
            unsafe {
                libc::kill(-pid, libc::SIGKILL);
            }
        }
    }
    let _ = child.kill();
}

fn publish_failure(
    reporter: &dyn Reporter,
    report: ProveReport,
    format: OutputFormat,
    gate: bool,
    evidence_out: Option<&Path>,
) -> i32 {
    if let Some(diagnostic) = report.common.diagnostics.last() {
        emit_error(
            reporter,
            &format!("gmeow-cli.prove.{}", diagnostic.code.to_ascii_lowercase()),
            diagnostic.detail.clone(),
        );
    }
    publish_report(report, format, gate, evidence_out, reporter)
}

fn publish_report(
    mut report: ProveReport,
    format: OutputFormat,
    gate: bool,
    evidence_out: Option<&Path>,
    reporter: &dyn Reporter,
) -> i32 {
    let mut json = serde_json::to_string_pretty(&report)
        .expect("typed prove report serialization is infallible");
    if let Some(path) = evidence_out
        && let Err(error) = std::fs::write(path, json.as_bytes())
    {
        report.execution_failure(
            "EVIDENCE_WRITE_FAILED",
            format!("cannot write {}: {error}", path.display()),
        );
        emit_error(
            reporter,
            "gmeow-cli.prove.evidence-write-failed",
            format!("cannot write {}: {error}", path.display()),
        );
        json = serde_json::to_string_pretty(&report)
            .expect("typed prove report serialization is infallible");
    }
    match format {
        OutputFormat::Json => println!("{json}"),
        OutputFormat::Text => render_text(&report),
    }
    report.common.exit_code(gate)
}

fn render_text(report: &ProveReport) {
    println!("verdict {}", report.common.verdict);
    println!("decision {}", report.common.decision.as_str());
    println!("input-status {}", report.common.input_status);
    println!("evaluation-status {}", report.common.evaluation_status);
    println!("completeness {}", report.common.completeness);
    println!("information-state {}", report.common.information_state);
    println!("evidence-grade {}", report.common.evidence_grade.as_str());
    println!("gate-admission {}", report.common.gate_admission.as_str());
    println!("sources {}", report.common.inputs.len());
    if let Some(admission) = &report.admission {
        println!("source-digest {}", admission.source_digest);
        println!("program-digest {}", admission.program_digest);
        println!("selection-digest {}", admission.selection_digest);
        println!("semantic-units {}", admission.units.len());
        println!("source-admission {:?}", admission.status);
    }
    if let Some(projection) = &report.projection {
        println!("projection {:?}", projection.status);
        println!("sentences {}", projection.sentences.len());
        if let Some(digest) = &projection.problem_digest {
            println!("problem-digest {digest}");
        }
        for blocker in &projection.blockers {
            println!("blocker {} {}", blocker.code, blocker.reason);
        }
    }
    if let Some(execution) = &report.execution {
        println!("prover {}", execution.prover);
        println!("prover-path {}", execution.executable.path);
        println!(
            "prover-executable-digest {}",
            execution.executable.content_digest
        );
        for pass in &execution.passes {
            println!("pass {} elapsed-ms {}", pass.pass, pass.elapsed_ms);
            if let Some(szs) = &pass.szs {
                println!("szs-status {}", szs.status);
            }
        }
    }
    for diagnostic in &report.common.diagnostics {
        println!("diagnostic {} {}", diagnostic.code, diagnostic.detail);
    }
}

#[cfg(test)]
#[path = "prove.tests.rs"]
mod tests;
