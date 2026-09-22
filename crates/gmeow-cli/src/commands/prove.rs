// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Installed external-FOF consumer: one native source composition, one compiler
//! admission, one deterministic projection, and bounded prover subprocesses.

use std::collections::BTreeSet;
use std::io::{Read, Write as _};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use gmeow_cli_core::Reporter;
use gmeow_errors::Diag;
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
    retained: Vec<u8>,
    digest: String,
    total_bytes: u64,
    retained_bytes: usize,
    truncated: bool,
    utf8_valid: bool,
    text: String,
}

#[derive(Debug, Clone, Serialize)]
struct ProcessReceipt {
    selection: ProcessSelection,
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
    fn blank(decision: DecisionClass, diagnostic: Diag) -> Self {
        let inputs = diagnostic
            .inner()
            .observed
            .as_ref()
            .filter(|slot| {
                slot.datatype.as_deref()
                    == Some("https://blackcatinformatics.ca/gmeow/SelectedInputIdentitiesJson")
            })
            .map(|slot| {
                serde_json::from_str(&slot.lexical).expect("internally serialized input identities")
            })
            .unwrap_or_default();
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
                inputs,
                ProgramIdentity::selected(FOF_PROFILE, &["source-admission-pending"]),
                EngineIdentity::external_host(),
                ReportDiagnostic::from_diag(&diagnostic),
            ),
            task: FofTask::AxiomConsistency,
            admission: None,
            projection: None,
            execution: None,
        }
    }

    fn execution_failure(&mut self, diagnostic: Diag) {
        self.common.verdict = "execution-failure".to_owned();
        self.common.decision = DecisionClass::ExecutionFailure;
        self.common.evaluation_status = "failed".to_owned();
        self.common.completeness = "unknown".to_owned();
        self.common.information_state = "not-evaluated".to_owned();
        self.common.evidence_grade = EvidenceGrade::Refused;
        self.common.gate_admission = GateAdmission::Refused;
        self.common
            .diagnostics
            .push(ReportDiagnostic::from_diag(&diagnostic));
    }
}

fn failure(reason: &str, detail: impl Into<String>) -> Diag {
    Diag::of_kind(crate::error::ProverFailed {
        reason: reason.to_owned(),
        detail: detail.into(),
        evidence: serde_json::Value::Null,
        cause: None,
    })
}

fn caused_failure(
    reason: &'static str,
    error: impl std::error::Error + Send + Sync + 'static,
    context: impl Into<String>,
) -> Diag {
    let context = context.into();
    Diag::of_kind(crate::error::ProverFailed {
        reason: reason.to_owned(),
        detail: format!("{context}: {error}"),
        evidence: serde_json::Value::Null,
        cause: Some(Box::new(error)),
    })
    .with_context(context)
}

fn diagnostic_snapshot(diagnostic: &Diag) -> serde_json::Value {
    serde_json::json!({
        "code": gmeow_errors::code::code_str(diagnostic.code()),
        "grade": diagnostic.grade(),
        "message": diagnostic.message(),
        "source_context": diagnostic.inner().source_ctx,
        "context": diagnostic.inner().context.iter().map(|frame| frame.label.as_str()).collect::<Vec<_>>(),
        "emitted_at": {
            "file": diagnostic.emitted_at().file(),
            "line": diagnostic.emitted_at().line(),
            "column": diagnostic.emitted_at().column(),
        },
        "observed": diagnostic.inner().observed,
        "expected": diagnostic.inner().expected,
    })
}

fn caused_diagnostic(
    reason: &'static str,
    diagnostic: Diag,
    context: impl Into<String>,
    mut evidence: serde_json::Value,
) -> Diag {
    let context = context.into();
    let source = diagnostic_snapshot(&diagnostic);
    if let Some(object) = evidence.as_object_mut() {
        object.insert("cause_diagnostic".to_owned(), source);
    } else {
        evidence = serde_json::json!({"value": evidence, "cause_diagnostic": source});
    }
    let detail = format!("{context}: {diagnostic:#}");
    Diag::of_kind(crate::error::ProverFailed {
        reason: reason.to_owned(),
        detail,
        evidence,
        cause: Some(Box::new(crate::error::PreservedDiagnostic(diagnostic))),
    })
    .with_context(context)
}

fn with_process(diagnostic: Diag, receipt: &ProcessReceipt) -> Diag {
    diagnostic.with_observed(gmeow_errors::Slot::typed(
        serde_json::to_string(receipt).expect("process receipt serializes"),
        "https://blackcatinformatics.ca/gmeow/ProverProcessReceiptJson",
    ))
}

fn with_inputs(mut diagnostic: Diag, inputs: &[InputIdentity]) -> Diag {
    diagnostic.inner_mut().observed = Some(gmeow_errors::Slot::typed(
        serde_json::to_string(inputs).expect("input identities serialize"),
        "https://blackcatinformatics.ca/gmeow/SelectedInputIdentitiesJson",
    ));
    diagnostic
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

#[derive(Debug, Clone, Serialize)]
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
            retained: self.retained,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
struct RawProcessReceipt {
    selection: ProcessSelection,
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
        Err(diagnostic) => {
            return publish_failure(
                reporter,
                ProveReport::blank(DecisionClass::Malformed, diagnostic),
                format,
                gate,
                evidence_out,
            );
        }
    };
    let loaded = match load_theory(&paths) {
        Ok(loaded) => loaded,
        Err(diagnostic) => {
            let report = ProveReport::blank(DecisionClass::Malformed, diagnostic);
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
                diagnostic: None,
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
            report.execution_failure(detail);
            return publish_failure(reporter, report, format, gate, evidence_out);
        }
    };
    let resolved = match resolve_prover(prover) {
        Ok(resolved) => resolved,
        Err(detail) => {
            report.execution_failure(detail);
            return publish_failure(reporter, report, format, gate, evidence_out);
        }
    };
    let version = match prover_version(&resolved) {
        Ok(version) => version,
        Err(detail) => {
            report.execution_failure(detail);
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
                report.execution_failure(detail);
                report.execution = Some(execution);
                return publish_failure(reporter, report, format, gate, evidence_out);
            }
        };
        let stdout = raw.stdout.into_report();
        let stderr = raw.stderr.into_report();
        let mut receipt = ProcessReceipt {
            selection: raw.selection,
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
            report.execution_failure(with_process(
                failure(
                    "PROVER_PARENT_DEADLINE",
                    format!(
                        "{} exceeded the parent wall deadline of {timeout_secs}s and was reaped",
                        resolved.path.display()
                    ),
                ),
                &receipt,
            ));
            execution.passes.push(receipt);
            report.execution = Some(execution);
            return publish_failure(reporter, report, format, gate, evidence_out);
        }
        if receipt.exit_code != Some(0) || receipt.signal.is_some() {
            report.execution_failure(with_process(
                failure(
                    "PROVER_CHILD_FAILED",
                    format!(
                        "{} exited with code {:?} and signal {:?}",
                        resolved.path.display(),
                        receipt.exit_code,
                        receipt.signal
                    ),
                ),
                &receipt,
            ));
            execution.passes.push(receipt);
            report.execution = Some(execution);
            return publish_failure(reporter, report, format, gate, evidence_out);
        }
        if receipt.stdout.truncated || receipt.stderr.truncated {
            report.execution_failure(with_process(
                failure(
                    "PROVER_OUTPUT_LIMIT",
                    format!(
                        "prover output exceeded the {}-byte per-stream evidence bound",
                        MAX_PROVER_OUTPUT_BYTES
                    ),
                ),
                &receipt,
            ));
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
                report.execution_failure(with_process(error.into(), &receipt));
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
                    report.execution_failure(with_process(error, &receipt));
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
            report.execution_failure(with_process(
                failure(&invalid.check.code, invalid.check.detail.clone()),
                &receipt,
            ));
            execution.passes.push(receipt);
            report.execution = Some(execution);
            return publish_failure(reporter, report, format, gate, evidence_out);
        }
        if let Some(mismatched) = receipt.artifacts.iter().find(|artifact| {
            artifact_concludes_outcome(&artifact.artifact.kind)
                && !artifact_matches_outcome(&artifact.artifact.kind, outcome)
        }) {
            report.execution_failure(with_process(
                failure(
                    "SZS_ARTIFACT_OUTCOME_MISMATCH",
                    format!(
                        "SZS status {} is incompatible with the captured {} artifact",
                        szs.status, mismatched.artifact.kind
                    ),
                ),
                &receipt,
            ));
            execution.passes.push(receipt);
            report.execution = Some(execution);
            return publish_failure(reporter, report, format, gate, evidence_out);
        }
        if outcome != SzsOutcome::Undecided {
            if let Some(previous) = decisive_szs.as_ref()
                && previous.outcome != outcome
            {
                report.execution_failure(with_process(
                    failure(
                        "CONFLICTING_PROVER_PASS_OUTCOME",
                        format!(
                            "separate prover passes reported incompatible outcomes {:?} and {:?}",
                            previous.outcome, outcome
                        ),
                    ),
                    &receipt,
                ));
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
            report.execution_failure(failure(
                "PROVER_EXECUTABLE_CHANGED",
                format!(
                    "selected executable changed during the run ({} -> {after})",
                    resolved.digest
                ),
            ));
            report.execution = Some(execution);
            return publish_failure(reporter, report, format, gate, evidence_out);
        }
        Err(detail) => {
            report.execution_failure(caused_diagnostic(
                "PROVER_EXECUTABLE_RECHECK_FAILED",
                detail,
                "recheck prover executable",
                serde_json::Value::Null,
            ));
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
            diagnostic: None,
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
            diagnostic: None,
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

fn source_paths(inputs: &[PathBuf], dir: Option<&Path>) -> gmeow_errors::Result<Vec<PathBuf>> {
    let mut candidates = inputs.to_vec();
    if let Some(root) = dir {
        candidates.extend(collect_semantic_sources(root)?);
    }
    if candidates.is_empty() {
        return Err(failure(
            "NO_SOURCE_INPUT",
            "pass one or more RDF files and/or --dir <corpus-root>".to_owned(),
        ));
    }
    let mut seen = BTreeSet::new();
    let mut paths = Vec::new();
    for path in candidates {
        let canonical = std::fs::canonicalize(&path).map_err(|error| {
            caused_failure(
                "SOURCE_PATH_UNAVAILABLE",
                error,
                format!("cannot resolve {}", path.display()),
            )
        })?;
        if !canonical.is_file() {
            return Err(failure(
                "SOURCE_NOT_FILE",
                format!("{} is not a regular file", canonical.display()),
            ));
        }
        if seen.insert(canonical.clone()) {
            paths.push(canonical);
        }
    }
    if paths.len() > MAX_SOURCE_DOCUMENTS {
        return Err(failure(
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

fn collect_semantic_sources(root: &Path) -> gmeow_errors::Result<Vec<PathBuf>> {
    let root = std::fs::canonicalize(root).map_err(|error| {
        caused_failure(
            "SOURCE_ROOT_UNAVAILABLE",
            error,
            format!("cannot resolve {}", root.display()),
        )
    })?;
    if !root.is_dir() {
        return Err(failure(
            "SOURCE_ROOT_NOT_DIRECTORY",
            format!("{} is not a directory", root.display()),
        ));
    }
    let root_display = root.display().to_string();
    let mut pending = vec![root];
    let mut sources = Vec::new();
    while let Some(directory) = pending.pop() {
        let entries = std::fs::read_dir(&directory).map_err(|error| {
            caused_failure(
                "SOURCE_ROOT_READ_FAILED",
                error,
                format!("cannot inspect {}", directory.display()),
            )
        })?;
        for entry in entries {
            let entry = entry.map_err(|error| {
                caused_failure(
                    "SOURCE_ROOT_READ_FAILED",
                    error,
                    format!("cannot inspect {}", directory.display()),
                )
            })?;
            let file_type = entry.file_type().map_err(|error| {
                caused_failure(
                    "SOURCE_ROOT_READ_FAILED",
                    error,
                    format!("cannot inspect {}", entry.path().display()),
                )
            })?;
            let path = entry.path();
            if file_type.is_dir() {
                pending.push(path);
            } else if file_type.is_file() && is_semantic_source(&path) {
                sources.push(path);
                if sources.len() > MAX_SOURCE_DOCUMENTS {
                    return Err(failure(
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
        return Err(failure(
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

fn load_theory(paths: &[PathBuf]) -> gmeow_errors::Result<LoadedTheory> {
    let mut receipts = Vec::with_capacity(paths.len());
    let mut identities = Vec::with_capacity(paths.len());
    let mut documents = Vec::with_capacity(paths.len());
    for path in paths {
        let bytes = std::fs::read(path).map_err(|error| {
            with_inputs(
                caused_failure(
                    "SOURCE_READ_FAILED",
                    error,
                    format!("cannot read {}", path.display()),
                ),
                &identities,
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
            with_inputs(failure(
                "SOURCE_SYNTAX_UNKNOWN",
                format!(
                    "cannot infer RDF syntax for {}; expected .ttl/.nt/.nq/.rdf/.owl/.xml/.trig",
                    path.display()
                ),
            ), &identities)
        })?;
        identities.push(InputIdentity::from_bytes(
            path,
            "selected-semantic-source",
            media,
            Some(base.clone()),
            &bytes,
        ));
        let dataset = purrdf::parse_dataset(&bytes, media, Some(&base)).map_err(|error| {
            with_inputs(
                caused_failure(
                    "SOURCE_PARSE_FAILED",
                    error,
                    format!("cannot parse {} as {media}", path.display()),
                ),
                &identities,
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
        with_inputs(
            caused_failure(
                "SOURCE_COMPOSITION_REFUSED",
                error,
                "compose selected documents",
            ),
            &identities,
        )
    })?;
    let materialized = composite.materialize().map_err(|error| {
        with_inputs(
            caused_failure(
                "SOURCE_MATERIALIZATION_REFUSED",
                error,
                "materialize selected documents",
            ),
            &identities,
        )
    })?;
    let mut prepared = PreparedLogicSource::new(&materialized).map_err(|error| {
        with_inputs(
            caused_failure(
                "SOURCE_PREPARATION_FAILED",
                error,
                "compile selected theory",
            ),
            &identities,
        )
    })?;
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
            .map_err(|error| {
                with_inputs(
                    caused_failure("SOURCE_BINDING_FAILED", error, "compile selected theory"),
                    &identities,
                )
            })?;
    }
    let theory = prepared.into_compiled(None).map_err(|error| {
        with_inputs(
            caused_failure(
                "SOURCE_COMPILATION_FAILED",
                error,
                "compile selected theory",
            ),
            &identities,
        )
    })?;
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
) -> gmeow_errors::Result<(PathBuf, Option<tempfile::TempPath>)> {
    if let Some(path) = out {
        std::fs::write(path, problem).map_err(|error| {
            caused_failure(
                "PROBLEM_WRITE_FAILED",
                error,
                format!("write problem {}", path.display()),
            )
        })?;
        let read_back = std::fs::read(path).map_err(|error| {
            caused_failure(
                "PROBLEM_VERIFY_READ_FAILED",
                error,
                format!("verify problem {}", path.display()),
            )
        })?;
        if read_back != problem.as_bytes() {
            return Err(failure(
                "PROBLEM_CONTENT_CHANGED",
                format!(
                    "problem file {} does not contain the emitted bytes",
                    path.display()
                ),
            ));
        }
        return Ok((path.to_path_buf(), None));
    }
    let prefix = format!("gmeow_{}_", &digest[..digest.len().min(16)]);
    let mut file = tempfile::Builder::new()
        .prefix(&prefix)
        .suffix(".p")
        .tempfile()
        .map_err(|error| {
            caused_failure(
                "PROBLEM_TEMPFILE_CREATE_FAILED",
                error,
                "create managed problem file",
            )
        })?;
    file.write_all(problem.as_bytes()).map_err(|error| {
        caused_failure(
            "PROBLEM_TEMPFILE_WRITE_FAILED",
            error,
            "write managed problem file",
        )
    })?;
    file.flush().map_err(|error| {
        caused_failure(
            "PROBLEM_TEMPFILE_FLUSH_FAILED",
            error,
            "flush managed problem file",
        )
    })?;
    let temp = file.into_temp_path();
    Ok((temp.to_path_buf(), Some(temp)))
}

fn resolve_prover(choice: ProverChoice) -> gmeow_errors::Result<ResolvedProver> {
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
                find_on_path("eprover")
                    .ok_or_else(|| failure("PROVER_NOT_AVAILABLE", "no eprover on PATH"))?,
            ),
            ProverChoice::Vampire => (
                ProverFlavor::Vampire,
                find_on_path("vampire")
                    .ok_or_else(|| failure("PROVER_NOT_AVAILABLE", "no vampire on PATH"))?,
            ),
            ProverChoice::Auto => find_on_path("eprover")
                .map(|path| (ProverFlavor::EProver, path))
                .or_else(|| find_on_path("vampire").map(|path| (ProverFlavor::Vampire, path)))
                .ok_or_else(|| failure("PROVER_NOT_AVAILABLE", "no eprover or vampire on PATH"))?,
        }
    };
    let path = std::fs::canonicalize(&path).map_err(|error| {
        caused_failure(
            "PROVER_PATH_UNAVAILABLE",
            error,
            format!("resolve prover {}", path.display()),
        )
    })?;
    if !path.is_file() {
        return Err(failure(
            "PROVER_NOT_FILE",
            format!("prover {} is not a regular file", path.display()),
        ));
    }
    let digest = digest_file(&path).map_err(|diagnostic| {
        caused_diagnostic(
            "PROVER_EXECUTABLE_IDENTITY_FAILED",
            diagnostic,
            format!("hash prover {}", path.display()),
            serde_json::Value::Null,
        )
    })?;
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

fn prover_version(prover: &ResolvedProver) -> gmeow_errors::Result<String> {
    let arguments = vec!["--version".to_owned()];
    let raw = run_bounded_process(
        &prover.path,
        &arguments,
        None,
        VERSION_DEADLINE,
        MAX_VERSION_OUTPUT_BYTES,
    )?;
    let reason = if raw.timed_out {
        Some("PROVER_VERSION_DEADLINE")
    } else if raw.exit_code != Some(0) || raw.signal.is_some() {
        Some("PROVER_VERSION_CHILD_FAILED")
    } else if raw.stdout.truncated || raw.stderr.truncated {
        Some("PROVER_VERSION_OUTPUT_LIMIT")
    } else {
        None
    };
    if let Some(reason) = reason {
        return Err(process_failure(
            reason,
            "prover version discovery failed",
            &raw,
            None,
        ));
    }
    let stdout = String::from_utf8_lossy(&raw.stdout.retained);
    let stderr = String::from_utf8_lossy(&raw.stderr.retained);
    let version = format!("{stdout}\n{stderr}").trim().to_owned();
    if version.is_empty() {
        return Err(process_failure(
            "PROVER_VERSION_EMPTY",
            "prover --version produced no identity text",
            &raw,
            None,
        ));
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

/// Exact selected subprocess inputs, including the final problem argument.
#[derive(Debug, Clone, Serialize)]
struct ProcessSelection {
    executable: String,
    executable_digest: Option<String>,
    arguments: Vec<String>,
    problem: Option<String>,
    problem_digest: Option<String>,
    deadline_ms: u64,
    output_limit: usize,
}

fn process_failure(
    reason: &'static str,
    detail: impl Into<String>,
    receipt: &RawProcessReceipt,
    cause: Option<Box<dyn std::error::Error + Send + Sync>>,
) -> Diag {
    Diag::of_kind(crate::error::ProverFailed {
        reason: reason.to_owned(),
        detail: detail.into(),
        evidence: serde_json::to_value(receipt).expect("process receipt serializes"),
        cause,
    })
}

fn process_diagnostic_failure(
    reason: &'static str,
    detail: impl Into<String>,
    receipt: &RawProcessReceipt,
    diagnostic: Diag,
) -> Diag {
    caused_diagnostic(
        reason,
        diagnostic,
        detail,
        serde_json::to_value(receipt).expect("process receipt serializes"),
    )
}

fn empty_stream() -> StreamBytes {
    StreamBytes {
        digest: blake3::hash(&[]).to_hex().to_string(),
        total_bytes: 0,
        retained: Vec::new(),
        truncated: false,
    }
}

fn run_bounded_process(
    binary: &Path,
    arguments: &[String],
    problem: Option<&Path>,
    deadline: Duration,
    output_limit: usize,
) -> gmeow_errors::Result<RawProcessReceipt> {
    let mut argv = arguments.to_vec();
    if let Some(problem) = problem {
        argv.push(problem.display().to_string());
    }
    let mut receipt = RawProcessReceipt {
        selection: ProcessSelection {
            executable: binary.display().to_string(),
            executable_digest: None,
            arguments: argv,
            problem: problem.map(|path| path.display().to_string()),
            problem_digest: None,
            deadline_ms: u64::try_from(deadline.as_millis()).unwrap_or(u64::MAX),
            output_limit,
        },
        elapsed_ms: 0,
        exit_code: None,
        signal: None,
        timed_out: false,
        stdout: empty_stream(),
        stderr: empty_stream(),
    };
    receipt.selection.executable_digest = Some(digest_file(binary).map_err(|diagnostic| {
        process_diagnostic_failure(
            "PROVER_EXECUTABLE_IDENTITY_FAILED",
            "cannot hash selected prover",
            &receipt,
            diagnostic,
        )
    })?);
    if let Some(problem) = problem {
        receipt.selection.problem_digest = Some(digest_file(problem).map_err(|diagnostic| {
            process_diagnostic_failure(
                "PROVER_PROBLEM_IDENTITY_FAILED",
                "cannot hash selected problem",
                &receipt,
                diagnostic,
            )
        })?);
    }
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
    let mut child = command.spawn().map_err(|error| {
        process_failure(
            "PROVER_SPAWN_FAILED",
            "cannot spawn selected prover",
            &receipt,
            Some(Box::new(error)),
        )
    })?;
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let (Some(stdout), Some(stderr)) = (stdout, stderr) else {
        kill_process_tree(&mut child);
        let status = child.wait();
        if let Ok(status) = status {
            record_status(&mut receipt, status);
        }
        return Err(process_failure(
            "PROVER_PIPE_MISSING",
            "selected child has a missing output pipe",
            &receipt,
            None,
        ));
    };
    #[cfg(unix)]
    if let Err(error) = set_nonblocking(&stdout).and_then(|()| set_nonblocking(&stderr)) {
        kill_process_tree(&mut child);
        if let Ok(status) = child.wait() {
            record_status(&mut receipt, status);
        }
        receipt.elapsed_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
        return Err(process_failure(
            "PROVER_PIPE_NONBLOCK_FAILED",
            "cannot make selected prover output pipes deadline-aware",
            &receipt,
            Some(Box::new(error)),
        ));
    }
    let stdout_capture = Arc::new(std::sync::Mutex::new(empty_stream()));
    let stderr_capture = Arc::new(std::sync::Mutex::new(empty_stream()));
    let stdout_shared = Arc::clone(&stdout_capture);
    let stderr_shared = Arc::clone(&stderr_capture);
    let stop_readers = Arc::new(AtomicBool::new(false));
    let stdout_stop = Arc::clone(&stop_readers);
    let stderr_stop = Arc::clone(&stop_readers);
    let stdout_reader = std::thread::spawn(move || {
        drain_stream_until_stopped(stdout, output_limit, &stdout_shared, &stdout_stop)
    });
    let stderr_reader = std::thread::spawn(move || {
        drain_stream_until_stopped(stderr, output_limit, &stderr_shared, &stderr_stop)
    });
    let mut failures: Vec<(&'static str, std::io::Error)> = Vec::new();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                record_status(&mut receipt, status);
                break;
            }
            Ok(None) if started.elapsed() < deadline => std::thread::sleep(PROCESS_POLL_INTERVAL),
            Ok(None) => {
                receipt.timed_out = true;
                break;
            }
            Err(error) => {
                failures.push(("PROVER_POLL_FAILED", error));
                break;
            }
        }
    }
    // Also close descendant-held pipes after the direct child exits. Only this
    // invocation's process group is selected; unrelated processes are untouched.
    kill_process_tree(&mut child);
    stop_readers.store(true, Ordering::Release);
    match child.wait() {
        Ok(status) => record_status(&mut receipt, status),
        Err(error) => failures.push(("PROVER_REAP_FAILED", error)),
    }
    // Join BOTH readers even when the first fails. Shared incremental captures
    // preserve every observed byte through a read error or a reader panic.
    for (name, reader) in [
        ("PROVER_STDOUT_READ_FAILED", stdout_reader),
        ("PROVER_STDERR_READ_FAILED", stderr_reader),
    ] {
        match reader.join() {
            Ok(Ok(())) => {}
            Ok(Err(error)) => failures.push((name, error)),
            Err(_) => failures.push((name, std::io::Error::other("prover stream reader panicked"))),
        }
    }
    receipt.stdout = stdout_capture
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .clone();
    receipt.stderr = stderr_capture
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .clone();
    receipt.elapsed_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
    if !failures.is_empty() {
        let (reason, cause) = failures.remove(0);
        let mut diagnostic = process_failure(
            reason,
            "selected prover process did not complete cleanly",
            &receipt,
            Some(Box::new(cause)),
        );
        for (operation, error) in failures {
            diagnostic = diagnostic.with_context(format!("{operation}: {error}"));
        }
        return Err(diagnostic);
    }
    Ok(receipt)
}

fn record_status(receipt: &mut RawProcessReceipt, status: std::process::ExitStatus) {
    receipt.exit_code = status.code();
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt as _;
        receipt.signal = status.signal();
    }
}

#[cfg(test)]
fn drain_stream(
    mut stream: impl Read,
    limit: usize,
    capture: &std::sync::Mutex<StreamBytes>,
) -> std::io::Result<()> {
    drain_stream_inner(&mut stream, limit, capture, None)
}

fn drain_stream_until_stopped(
    mut stream: impl Read,
    limit: usize,
    capture: &std::sync::Mutex<StreamBytes>,
    stop: &AtomicBool,
) -> std::io::Result<()> {
    drain_stream_inner(&mut stream, limit, capture, Some(stop))
}

fn drain_stream_inner(
    stream: &mut impl Read,
    limit: usize,
    capture: &std::sync::Mutex<StreamBytes>,
    stop: Option<&AtomicBool>,
) -> std::io::Result<()> {
    let mut hasher = blake3::Hasher::new();
    let mut buffer = [0_u8; 16 * 1024];
    loop {
        let read = match stream.read(&mut buffer) {
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                if stop.is_some_and(|stop| stop.load(Ordering::Acquire)) {
                    return Ok(());
                }
                std::thread::sleep(PROCESS_POLL_INTERVAL);
                continue;
            }
            value => value?,
        };
        if read == 0 {
            return Ok(());
        }
        hasher.update(&buffer[..read]);
        let mut bytes = capture
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        bytes.total_bytes = bytes
            .total_bytes
            .saturating_add(u64::try_from(read).unwrap_or(u64::MAX));
        let available = limit.saturating_sub(bytes.retained.len());
        bytes
            .retained
            .extend_from_slice(&buffer[..read.min(available)]);
        bytes.digest = hasher.finalize().to_hex().to_string();
        bytes.truncated =
            bytes.total_bytes > u64::try_from(bytes.retained.len()).unwrap_or(u64::MAX);
    }
}

#[cfg(unix)]
fn set_nonblocking(stream: &impl std::os::fd::AsRawFd) -> std::io::Result<()> {
    let descriptor = stream.as_raw_fd();
    let flags = unsafe { libc::fcntl(descriptor, libc::F_GETFL) };
    if flags < 0 {
        return Err(std::io::Error::last_os_error());
    }
    if unsafe { libc::fcntl(descriptor, libc::F_SETFL, flags | libc::O_NONBLOCK) } < 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(())
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
        emit_error(reporter, &diagnostic.code, diagnostic.detail.clone());
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
        let diagnostic = caused_failure(
            "EVIDENCE_WRITE_FAILED",
            error,
            format!("write evidence report {}", path.display()),
        );
        let detail = diagnostic.message().to_owned();
        report.execution_failure(diagnostic);
        emit_error(reporter, "gmeow-cli.prove.evidence-write-failed", detail);
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
