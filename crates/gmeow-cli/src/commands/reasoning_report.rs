// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! One machine and gate contract for every installed reasoning command.
//!
//! The operation-specific verdict remains explicit: consistency, entailment,
//! classification and realization do not answer the same proposition. The shared
//! axes describe whether the input was admitted, what completed, what was preserved,
//! and whether the evidence may control a gate.

use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::OutputFormat;

pub(crate) const EXIT_NEGATIVE: i32 = 1;
pub(crate) const EXIT_UNDECIDED: i32 = 3;
pub(crate) const EXIT_MALFORMED: i32 = 4;
pub(crate) const EXIT_EXECUTION: i32 = 5;

/// The proposition family answered by one report.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum ReasoningOperation {
    Consistency,
    Entailment,
    Classification,
    Realization,
    AxiomConsistency,
}

impl ReasoningOperation {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Consistency => "consistency",
            Self::Entailment => "entailment",
            Self::Classification => "classification",
            Self::Realization => "realization",
            Self::AxiomConsistency => "axiom-consistency",
        }
    }
}

/// Operation-neutral decision class. `verdict` retains the operation-specific token.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum DecisionClass {
    Positive,
    Negative,
    Undecided,
    Unsupported,
    Malformed,
    ExecutionFailure,
}

impl DecisionClass {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Positive => "positive",
            Self::Negative => "negative",
            Self::Undecided => "undecided",
            Self::Unsupported => "unsupported",
            Self::Malformed => "malformed",
            Self::ExecutionFailure => "execution-failure",
        }
    }
}

/// Strength of the evidence behind the operation-specific verdict.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum EvidenceGrade {
    Certificate,
    Attestation,
    Refused,
}

impl EvidenceGrade {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Certificate => "certificate",
            Self::Attestation => "attestation",
            Self::Refused => "refused",
        }
    }
}

/// The shared completeness gate: conclusive certificate, bounded/unchecked
/// attestation, or refusal to issue an affirmative artifact.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum GateAdmission {
    Certificate,
    Attestation,
    Refused,
}

impl GateAdmission {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Certificate => "certificate",
            Self::Attestation => "attestation",
            Self::Refused => "refused",
        }
    }
}

/// Exact identity of one selected input document.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct InputIdentity {
    pub(crate) path: String,
    pub(crate) role: String,
    pub(crate) media_type: String,
    pub(crate) base_iri: Option<String>,
    pub(crate) content_digest: String,
    pub(crate) bytes: u64,
}

impl InputIdentity {
    pub(crate) fn from_bytes(
        path: &Path,
        role: impl Into<String>,
        media_type: impl Into<String>,
        base_iri: Option<String>,
        bytes: &[u8],
    ) -> Self {
        Self {
            path: path.display().to_string(),
            role: role.into(),
            media_type: media_type.into(),
            base_iri,
            content_digest: blake3::hash(bytes).to_hex().to_string(),
            bytes: u64::try_from(bytes.len()).unwrap_or(u64::MAX),
        }
    }
}

/// Content identity of the selected operation/profile implementation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct ProgramIdentity {
    pub(crate) profile: String,
    pub(crate) content_digest: String,
}

impl ProgramIdentity {
    pub(crate) fn selected(profile: impl Into<String>, components: &[&str]) -> Self {
        let profile = profile.into();
        let mut hasher = blake3::Hasher::new();
        frame(&mut hasher, b"gmeow-reasoning-program-v1");
        frame(&mut hasher, profile.as_bytes());
        for component in components {
            frame(&mut hasher, component.as_bytes());
        }
        Self {
            profile,
            content_digest: hasher.finalize().to_hex().to_string(),
        }
    }
}

/// Exact executable identity where one exists.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct ExecutableIdentity {
    pub(crate) path: String,
    pub(crate) content_digest: String,
    pub(crate) version: String,
}

/// Engine identity separated from the process that happens to host it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct EngineIdentity {
    pub(crate) implementation: String,
    pub(crate) version: String,
    pub(crate) executable: Option<ExecutableIdentity>,
}

impl EngineIdentity {
    pub(crate) fn gmeow_native() -> Self {
        Self {
            implementation: "gmeow-logic".to_owned(),
            version: gmeow_logic::counterfactual::SOLVER_VERSION.to_owned(),
            executable: current_executable_identity(),
        }
    }

    pub(crate) fn purrdf_dl() -> Self {
        Self {
            implementation: "purrdf-owl-dl".to_owned(),
            version: purrdf::entail::DL_CALCULUS_VERSION.to_owned(),
            executable: current_executable_identity(),
        }
    }

    pub(crate) fn external_host() -> Self {
        Self {
            implementation: "gmeow-cli".to_owned(),
            version: env!("CARGO_PKG_VERSION").to_owned(),
            executable: current_executable_identity(),
        }
    }
}

/// Canonical preservation projection for JSON/text. The logic enums themselves
/// intentionally retain their Rust names for RDF serialization, so this wire surface
/// renders their canonical vocabulary local names explicitly.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct PreservationReport {
    pub(crate) polarities: Vec<String>,
    pub(crate) unsupported_constructs: Vec<String>,
}

impl PreservationReport {
    pub(crate) fn exact() -> Self {
        Self {
            polarities: vec!["ExactPreservation".to_owned()],
            unsupported_constructs: Vec::new(),
        }
    }

    pub(crate) fn sound_under(unsupported_constructs: Vec<String>) -> Self {
        Self {
            polarities: vec!["SoundUnderApproximation".to_owned()],
            unsupported_constructs,
        }
    }

    pub(crate) fn unsupported(unsupported_constructs: Vec<String>) -> Self {
        Self {
            polarities: vec!["Unsupported".to_owned()],
            unsupported_constructs,
        }
    }
}

/// Measured work in the operation's native budget units. A field is `null` only
/// when the underlying operation does not expose that measurement.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize)]
pub(crate) struct ReasoningMetrics {
    pub(crate) decisions: Option<u64>,
    pub(crate) steps: Option<u64>,
    pub(crate) budget: Option<u64>,
}

/// One capability or coverage boundary.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct ReasoningBoundary {
    pub(crate) code: String,
    pub(crate) detail: String,
}

/// One stable machine diagnostic.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct ReportDiagnostic {
    pub(crate) code: String,
    pub(crate) detail: String,
}

/// The fields every reasoning-family report carries.
#[derive(Debug, Clone, Serialize)]
pub(crate) struct ReasoningReportCore {
    pub(crate) schema_version: u32,
    pub(crate) operation: ReasoningOperation,
    /// Operation-specific token (`true`, `entailed`, `classified`, ...).
    pub(crate) verdict: String,
    pub(crate) decision: DecisionClass,
    pub(crate) input_status: String,
    pub(crate) evaluation_status: String,
    pub(crate) completeness: String,
    pub(crate) information_state: String,
    pub(crate) preservation: PreservationReport,
    pub(crate) evidence_grade: EvidenceGrade,
    pub(crate) gate_admission: GateAdmission,
    pub(crate) declared_fragment: Option<String>,
    pub(crate) inputs: Vec<InputIdentity>,
    pub(crate) program: ProgramIdentity,
    pub(crate) engine: EngineIdentity,
    pub(crate) metrics: ReasoningMetrics,
    pub(crate) boundaries: Vec<ReasoningBoundary>,
    pub(crate) diagnostics: Vec<ReportDiagnostic>,
}

/// One ordered binary answer row.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct TermPair {
    pub(crate) left: String,
    pub(crate) right: String,
}

/// The operation-specific payload below the shared result contract.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub(crate) enum ReasoningPayload {
    Consistency {
        has_model: Option<bool>,
    },
    Entailment {
        entailed: Option<bool>,
        gap_shape: Option<String>,
        gap_detail: Option<String>,
    },
    Classification {
        subsumptions: Vec<TermPair>,
        direct_subsumptions: Vec<TermPair>,
        equivalences: Vec<TermPair>,
        unsatisfiable: Vec<String>,
    },
    Realization {
        types: Vec<TermPair>,
        direct_types: Vec<TermPair>,
    },
    Empty,
}

/// Shared envelope used by all native installed reasoning commands.
#[derive(Debug, Clone, Serialize)]
pub(crate) struct NativeReasoningReport {
    #[serde(flatten)]
    pub(crate) common: ReasoningReportCore,
    pub(crate) result: ReasoningPayload,
}

impl NativeReasoningReport {
    pub(crate) fn publish(&self, format: OutputFormat, gate: bool) -> i32 {
        match format {
            OutputFormat::Json => println!(
                "{}",
                serde_json::to_string_pretty(self)
                    .expect("typed native reasoning report serialization is infallible")
            ),
            OutputFormat::Text => self.render_text(),
        }
        self.common.exit_code(gate)
    }

    fn render_text(&self) {
        println!("verdict {}", self.common.verdict);
        println!("decision {}", self.common.decision.as_str());
        println!("input-status {}", self.common.input_status);
        println!("evaluation-status {}", self.common.evaluation_status);
        println!("completeness {}", self.common.completeness);
        println!("information-state {}", self.common.information_state);
        println!("evidence-grade {}", self.common.evidence_grade.as_str());
        println!("gate-admission {}", self.common.gate_admission.as_str());
        if let Some(fragment) = &self.common.declared_fragment {
            println!("declared-fragment {fragment}");
        }
        println!("program-digest {}", self.common.program.content_digest);
        println!("engine {}", self.common.engine.implementation);
        println!("engine-version {}", self.common.engine.version);
        if let Some(decisions) = self.common.metrics.decisions {
            println!("decisions {decisions}");
        }
        if let Some(steps) = self.common.metrics.steps {
            println!("steps {steps}");
        }
        if let Some(budget) = self.common.metrics.budget {
            println!("budget {budget}");
        }
        for boundary in &self.common.boundaries {
            println!("boundary {}: {}", boundary.code, boundary.detail);
        }
        match &self.result {
            ReasoningPayload::Consistency { .. } | ReasoningPayload::Empty => {}
            ReasoningPayload::Entailment {
                gap_shape,
                gap_detail,
                ..
            } => {
                if let Some(shape) = gap_shape {
                    println!("gap-shape {shape}");
                }
                if let Some(detail) = gap_detail {
                    println!("gap-detail {detail}");
                }
            }
            ReasoningPayload::Classification {
                subsumptions,
                direct_subsumptions,
                equivalences,
                unsatisfiable,
            } => {
                for row in subsumptions {
                    println!("subsumption {} {}", row.left, row.right);
                }
                for row in direct_subsumptions {
                    println!("direct {} {}", row.left, row.right);
                }
                for row in equivalences {
                    println!("equivalence {} {}", row.left, row.right);
                }
                for class in unsatisfiable {
                    println!("unsatisfiable {class}");
                }
            }
            ReasoningPayload::Realization {
                types,
                direct_types,
            } => {
                for row in types {
                    println!("type {} {}", row.left, row.right);
                }
                for row in direct_types {
                    println!("direct-type {} {}", row.left, row.right);
                }
            }
        }
        for diagnostic in &self.common.diagnostics {
            println!("diagnostic {} {}", diagnostic.code, diagnostic.detail);
        }
    }
}

impl ReasoningReportCore {
    pub(crate) fn refused(
        operation: ReasoningOperation,
        decision: DecisionClass,
        verdict: impl Into<String>,
        input_status: &str,
        inputs: Vec<InputIdentity>,
        program: ProgramIdentity,
        engine: EngineIdentity,
        diagnostic: ReportDiagnostic,
    ) -> Self {
        let evaluation_status = match decision {
            DecisionClass::Malformed => "not-evaluated",
            DecisionClass::ExecutionFailure => "failed",
            _ => "unsupported",
        };
        Self {
            schema_version: 1,
            operation,
            verdict: verdict.into(),
            decision,
            input_status: input_status.to_owned(),
            evaluation_status: evaluation_status.to_owned(),
            completeness: "unknown".to_owned(),
            information_state: "not-evaluated".to_owned(),
            preservation: PreservationReport::unsupported(Vec::new()),
            evidence_grade: EvidenceGrade::Refused,
            gate_admission: GateAdmission::Refused,
            declared_fragment: Some(program.profile.clone()),
            inputs,
            program,
            engine,
            metrics: ReasoningMetrics::default(),
            boundaries: Vec::new(),
            diagnostics: vec![diagnostic],
        }
    }

    pub(crate) fn exit_code(&self, gate: bool) -> i32 {
        match self.decision {
            DecisionClass::Malformed => EXIT_MALFORMED,
            DecisionClass::ExecutionFailure => EXIT_EXECUTION,
            DecisionClass::Negative
                if !gate
                    && matches!(
                        self.operation,
                        ReasoningOperation::Classification | ReasoningOperation::Realization
                    ) =>
            {
                EXIT_NEGATIVE
            }
            _ if !gate => 0,
            DecisionClass::Positive if self.gate_admission == GateAdmission::Certificate => 0,
            DecisionClass::Negative if self.evidence_grade == EvidenceGrade::Certificate => {
                EXIT_NEGATIVE
            }
            _ => EXIT_UNDECIDED,
        }
    }
}

pub(crate) fn digest_file(path: &Path) -> Result<String, String> {
    let bytes = std::fs::read(path)
        .map_err(|error| format!("cannot read {} for identity: {error}", path.display()))?;
    Ok(blake3::hash(&bytes).to_hex().to_string())
}

fn current_executable_identity() -> Option<ExecutableIdentity> {
    let path: PathBuf = std::env::current_exe().ok()?.canonicalize().ok()?;
    let content_digest = digest_file(&path).ok()?;
    Some(ExecutableIdentity {
        path: path.display().to_string(),
        content_digest,
        version: env!("CARGO_PKG_VERSION").to_owned(),
    })
}

fn frame(hasher: &mut blake3::Hasher, bytes: &[u8]) {
    hasher.update(&u64::try_from(bytes.len()).unwrap_or(u64::MAX).to_le_bytes());
    hasher.update(bytes);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn core(
        decision: DecisionClass,
        evidence: EvidenceGrade,
        gate: GateAdmission,
    ) -> ReasoningReportCore {
        let mut report = ReasoningReportCore::refused(
            ReasoningOperation::Consistency,
            decision,
            decision.as_str(),
            "valid",
            Vec::new(),
            ProgramIdentity::selected("test-profile", &["test-engine"]),
            EngineIdentity::gmeow_native(),
            ReportDiagnostic {
                code: "TEST".to_owned(),
                detail: "synthetic".to_owned(),
            },
        );
        report.evidence_grade = evidence;
        report.gate_admission = gate;
        report
    }

    #[test]
    fn gate_exit_contract_distinguishes_all_decision_classes() {
        assert_eq!(
            core(
                DecisionClass::Positive,
                EvidenceGrade::Certificate,
                GateAdmission::Certificate
            )
            .exit_code(true),
            0
        );
        assert_eq!(
            core(
                DecisionClass::Negative,
                EvidenceGrade::Certificate,
                GateAdmission::Refused
            )
            .exit_code(true),
            EXIT_NEGATIVE
        );
        assert_eq!(
            core(
                DecisionClass::Undecided,
                EvidenceGrade::Attestation,
                GateAdmission::Attestation
            )
            .exit_code(true),
            EXIT_UNDECIDED
        );
        assert_eq!(
            core(
                DecisionClass::Unsupported,
                EvidenceGrade::Refused,
                GateAdmission::Refused
            )
            .exit_code(true),
            EXIT_UNDECIDED
        );
        assert_eq!(
            core(
                DecisionClass::Malformed,
                EvidenceGrade::Refused,
                GateAdmission::Refused
            )
            .exit_code(false),
            EXIT_MALFORMED
        );
        assert_eq!(
            core(
                DecisionClass::ExecutionFailure,
                EvidenceGrade::Refused,
                GateAdmission::Refused
            )
            .exit_code(false),
            EXIT_EXECUTION
        );
    }
}
