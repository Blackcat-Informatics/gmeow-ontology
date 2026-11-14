// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Installed `gmeow prove`: one native source composition, one compiler admission,
//! and two in-process engines over that exact selection.
//!
//! The native rule engine reads the whole `logic:` selection and names what it
//! could not carry. The OWL 2 DL tableau reads the selection's OWL view and returns
//! a replayable proof. A clash from either engine refutes the whole selection.
//! Only the native engine may conclude `consistent`, and only from a completed
//! closure over a complete admission with an empty drop ledger. Every other run is
//! `not-decided` with each drop named. Nothing here spawns a process, opens a
//! socket or reads a clock, so two runs over the same inputs print the same bytes.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use gmeow_cli_core::Reporter;
use gmeow_errors::Diag;
use gmeow_logic::reason::dl::DlCoverage;
use gmeow_logic::reason::{
    DomainProfile, InconsistencyWitness, LogicalGraph, SelectedDomains, SelectedLogicalWorld,
    UnsatClass,
};
use gmeow_logic::reasoner_services::DlReasoner;
use gmeow_logic_compile::frontend::{
    AxiomSource, PreparedLogicSource, SemanticUnitDisposition, SourceAdmission,
    SourceAdmissionStatus, SourceBase, SourceBaseOrigin, SourceDocument, SourceSelection,
};
use gmeow_logic_compile::ir::EvidenceCoordinate;
use gmeow_logic_compile::tptp::{FofProjectionBlocker, FofProjectionStatus, project_tptp_fof};
use purrdf::entail::Question;
use purrdf::{CompositeDatasetView, RdfDataset, TermRef, ViewLimits};
use serde::Serialize;

use super::emit_error;
use super::reasoning_report::{
    DecisionClass, EngineIdentity, EvidenceGrade, GateAdmission, InputIdentity, PreservationReport,
    ProgramIdentity, ReasoningBoundary, ReasoningMetrics, ReasoningOperation, ReasoningReportCore,
    ReportDiagnostic,
};
use crate::OutputFormat;

const MAX_SOURCE_DOCUMENTS: usize = 4_096;

/// Declared decision procedure of this operation: the native closure decides,
/// the OWL view refutes or supports, and the drop ledger bounds both.
const PROVE_PROFILE: &str = "gmeow-native-selection-consistency-v1";

/// Authority recorded for the single default-graph world `prove` selects.
const PROVE_WORLD_AUTHORITY: &str = "gmeow.prove.default-graph-selection.v1";

const VERDICT_CONSISTENT: &str = "consistent";
const VERDICT_INCONSISTENT: &str = "inconsistent";
const VERDICT_NOT_DECIDED: &str = "not-decided";

const ENGINE_ADMISSION: &str = "admission";
const ENGINE_NATIVE: &str = "native";
const ENGINE_OWL_DL: &str = "owl-dl";

/// Exit status of a replayed proof check.
const EXIT_PROOF_REJECTED: i32 = 1;

/// One complete `gmeow prove` invocation as selected on the command line.
pub(crate) struct ProveRequest<'a> {
    pub(crate) inputs: &'a [PathBuf],
    pub(crate) dir: Option<&'a Path>,
    pub(crate) max_steps: Option<u64>,
    pub(crate) step_cap: Option<u64>,
    pub(crate) format: OutputFormat,
    pub(crate) gate: bool,
    pub(crate) evidence_out: Option<&'a Path>,
    pub(crate) proof_out: Option<&'a Path>,
    pub(crate) check_proof: Option<&'a Path>,
    pub(crate) out: Option<&'a Path>,
}

/// One construct, unit or obligation an engine did not read or did not decide.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize)]
struct DropRecord {
    engine: &'static str,
    code: String,
    detail: String,
}

impl DropRecord {
    fn new(engine: &'static str, code: impl Into<String>, detail: impl Into<String>) -> Self {
        Self {
            engine,
            code: code.into(),
            detail: detail.into(),
        }
    }
}

/// The selection inventory and everything the deciding engine did not decide.
#[derive(Debug, Clone, Serialize)]
struct SelectionReport {
    /// True exactly when the admission is complete and `drops` is empty.
    complete: bool,
    source_digest: String,
    program_digest: String,
    selection_digest: String,
    documents: usize,
    units: usize,
    /// How many units were admitted with evidence left out of the problem. This is
    /// not a drop: the claim is fully in the problem and only its attribution is
    /// not, so it never makes the selection incomplete.
    units_with_evidence_left_out: usize,
    /// The evidence coordinates left out across those units, sorted.
    evidence_left_out: Vec<&'static str>,
    /// Admission and native-accounting drops, sorted and de-duplicated. The OWL
    /// leg's own state is reported under that leg, never here.
    drops: Vec<DropRecord>,
}

/// The native rule engine's own account of the selection.
#[derive(Debug, Clone, Serialize)]
struct NativeLeg {
    /// `clash`, `no-clash`, or `refused` when the engine declined the program.
    verdict: &'static str,
    information_state: Option<String>,
    evaluation: Option<String>,
    completeness: Option<String>,
    consumed_steps: Option<u64>,
    allowance: Option<u64>,
    inconsistencies: Vec<InconsistencyWitness>,
    unsatisfiable_classes: Vec<UnsatClass>,
    coverage: Option<DlCoverage>,
    /// Each clash: the law that derived it and its witness premises.
    refutations: Vec<Refutation>,
}

impl NativeLeg {
    fn clash(&self) -> bool {
        self.verdict == "clash"
    }
}

/// One derived clash, named by its engine and law, with the premises it used.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize)]
struct Refutation {
    engine: &'static str,
    law: String,
    subject: Option<String>,
    world: Option<String>,
    premises: Vec<(String, String, String)>,
    proof_digest: Option<String>,
}

/// One construct boundary or state note of the OWL view.
#[derive(Debug, Clone, Serialize)]
struct OwlNote {
    code: String,
    detail: String,
}

/// The OWL view's state as the tableau left it.
const OWL_CONSISTENT: &str = "consistent";
const OWL_INCONSISTENT: &str = "inconsistent";
const OWL_NOT_DECIDED: &str = "not-decided";
const OWL_VIEW_NOT_BUILT: &str = "view-not-built";

/// The OWL 2 DL tableau's account of the selection's OWL view. It is a second
/// refuter: its `inconsistent` refutes the selection, while every other state is
/// reported here and never changes the native engine's verdict. Its notes are not
/// selection drops, because the native accounting already names everything the
/// native engine did not decide.
#[derive(Debug, Clone, Serialize)]
struct OwlDlLeg {
    /// `consistent`, `inconsistent`, `not-decided` (the search stopped before
    /// deciding; `completeness` says why) or `view-not-built`.
    state: &'static str,
    /// The tableau's answer `true`, `false` or `unknown`; `null` without a view.
    answer: Option<String>,
    completeness: Option<String>,
    steps: Option<u64>,
    budget: Option<u64>,
    work: Option<u64>,
    work_budget: Option<u64>,
    decisions: Option<u64>,
    /// Constructs of the selection the OWL view does not carry.
    boundaries: Vec<OwlNote>,
    proof_digest: Option<String>,
    /// The claim the recorded proof certifies, as `--check-proof` prints it.
    proof_claim: Option<String>,
    /// Why the leg did not reach a decided state, when it did not.
    notes: Vec<OwlNote>,
}

impl OwlDlLeg {
    fn not_built(code: &str, detail: String) -> Self {
        Self {
            state: OWL_VIEW_NOT_BUILT,
            answer: None,
            completeness: None,
            steps: None,
            budget: None,
            work: None,
            work_budget: None,
            decisions: None,
            boundaries: Vec::new(),
            proof_digest: None,
            proof_claim: None,
            notes: vec![OwlNote {
                code: code.to_owned(),
                detail,
            }],
        }
    }

    fn clash(&self) -> bool {
        self.state == OWL_INCONSISTENT
    }
}

/// The claims a recorded tableau proof states, spelled for a second reader.
fn proof_claims(proof: &purrdf::entail::ServiceProof) -> Vec<String> {
    proof
        .claims()
        .iter()
        .map(|claim| match (claim.subject(), claim.verdict()) {
            (purrdf::entail::ClaimSubject::Consistent, purrdf::entail::Verdict::True) => {
                "owl-view-consistent".to_owned()
            }
            (purrdf::entail::ClaimSubject::Consistent, purrdf::entail::Verdict::False) => {
                "owl-view-inconsistent".to_owned()
            }
            (purrdf::entail::ClaimSubject::Consistent, purrdf::entail::Verdict::Unknown) => {
                "owl-view-not-decided".to_owned()
            }
            (subject, verdict) => format!("{subject:?} {verdict}"),
        })
        .collect()
}

#[derive(Debug, Clone, Serialize)]
struct EngineReports {
    native: NativeLeg,
    owl_dl: OwlDlLeg,
}

/// The plain TPTP export selected with `--out`. No prover is run on it.
#[derive(Debug, Clone, Serialize)]
struct TptpExport {
    path: String,
    written: bool,
    status: FofProjectionStatus,
    profile_id: String,
    problem_digest: Option<String>,
    sentences: usize,
    blockers: Vec<FofProjectionBlocker>,
}

/// The reference semantics of every verdict this command prints.
const PROVE_SEMANTICS: &str = "GMEOW native logic: semantics of the selection's default graph, decided by the native rule engine's accounting; the OWL 2 DL tableau over the selection's OWL view is a second refuter only. Authored reasoning-contract configuration is catalogued, not executed. A native consistent verdict has no replayable proof and is reproduced by re-running on the same inputs; a native inconsistent verdict carries its law and witness premises; only the tableau's answer carries a replayable proof. This is not a verdict about the classical first-order export written by --out.";

#[derive(Debug, Clone, Serialize)]
struct ProveReport {
    #[serde(flatten)]
    common: ReasoningReportCore,
    semantics: &'static str,
    /// Every clash either engine derived; empty unless the verdict is `inconsistent`.
    refutations: Vec<Refutation>,
    admission: Option<SourceAdmission>,
    selection: Option<SelectionReport>,
    engines: Option<EngineReports>,
    export: Option<TptpExport>,
}

impl ProveReport {
    fn blank(decision: DecisionClass, diagnostic: Diag) -> Self {
        let inputs = recorded_inputs(&diagnostic);
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
                ProgramIdentity::selected(PROVE_PROFILE, &["source-admission-pending"]),
                EngineIdentity::gmeow_native(),
                ReportDiagnostic::from_diag(&diagnostic),
            ),
            semantics: PROVE_SEMANTICS,
            refutations: Vec::new(),
            admission: None,
            selection: None,
            engines: None,
            export: None,
        }
    }

    fn execution_failure(&mut self, diagnostic: Diag) {
        self.common.verdict = DecisionClass::ExecutionFailure.as_str().to_owned();
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

fn recorded_inputs(diagnostic: &Diag) -> Vec<InputIdentity> {
    diagnostic
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
        .unwrap_or_default()
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

/// Retain a lower diagnostic's complete wire report and original live typed cause.
/// The outer reason identifies the prove operation without replacing lower evidence.
fn caused_diagnostic(
    reason: &'static str,
    diagnostic: Diag,
    context: impl Into<String>,
    mut evidence: serde_json::Value,
) -> Diag {
    let context = context.into();
    let source = serde_json::to_value(ReportDiagnostic::from_diag(&diagnostic))
        .expect("native diagnostic report serializes");
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
        cause: Some(Box::new(gmeow_errors::PreservedDiagnostic(diagnostic))),
    })
    .with_context(context)
}

fn with_inputs(mut diagnostic: Diag, inputs: &[InputIdentity]) -> Diag {
    diagnostic.inner_mut().observed = Some(gmeow_errors::Slot::typed(
        serde_json::to_string(inputs).expect("input identities serialize"),
        "https://blackcatinformatics.ca/gmeow/SelectedInputIdentitiesJson",
    ));
    diagnostic
}

/// The finite blank-node canonicalization search limit, when it caused `error`.
/// The limit belongs to PurRDF's canonicalization policy; it is reported, never
/// raised, and never answered with a weaker labeling.
fn canonicalization_limit<'a>(
    error: &'a (dyn std::error::Error + 'static),
) -> Option<&'a purrdf::BudgetExceeded> {
    let mut current = Some(error);
    while let Some(error) = current {
        if let Some(budget) = error.downcast_ref::<purrdf::BudgetExceeded>() {
            return Some(budget);
        }
        if let Some(purrdf::CanonError::BudgetExceeded(budget)) =
            error.downcast_ref::<purrdf::CanonError>()
        {
            return Some(budget);
        }
        current = error.source();
    }
    None
}

fn diag_canonicalization_limit(diagnostic: &Diag) -> Option<&purrdf::BudgetExceeded> {
    let source = diagnostic.inner().source.as_deref()?;
    canonicalization_limit(source)
}

/// The named execution failure for a canonicalization search that reached its limit.
fn canonicalization_limit_failure(
    budget: &purrdf::BudgetExceeded,
    stage: &str,
    cause: Diag,
    inputs: &[InputIdentity],
) -> Diag {
    let evidence = serde_json::json!({
        "stage": stage,
        "limit": budget.limit,
        "blank_nodes": budget.blank_count,
        "recursive_frames": budget.work.count(purrdf::SearchEvent::RdfcFrame),
        "permutation_attempts": budget.work.count(purrdf::SearchEvent::RdfcPermutation),
    });
    with_inputs(
        caused_diagnostic(
            "SOURCE_CANONICALIZATION_LIMIT",
            cause,
            format!(
                "{stage}: blank-node canonicalization reached its finite search limit of {} work units over {} blank nodes",
                budget.limit, budget.blank_count
            ),
            evidence,
        ),
        inputs,
    )
}

/// Why a selection could not be loaded.
enum LoadFailure {
    /// The selected documents are not a well-formed source.
    Malformed(Diag),
    /// A well-formed source exceeded an execution limit.
    Execution(Diag),
}

impl LoadFailure {
    fn into_report(self) -> ProveReport {
        match self {
            Self::Malformed(diagnostic) => ProveReport::blank(DecisionClass::Malformed, diagnostic),
            Self::Execution(diagnostic) => {
                ProveReport::blank(DecisionClass::ExecutionFailure, diagnostic)
            }
        }
    }
}

struct LoadedTheory {
    identities: Vec<InputIdentity>,
    // The source-local IDs retained in provenance bindings belong to these exact
    // immutable parses. Keep them alive through admission and both engines.
    _documents: Vec<Arc<RdfDataset>>,
    theory: gmeow_logic_compile::frontend::CompiledTheory,
}

/// Run the complete installed `gmeow prove` operation.
pub(crate) fn prove(reporter: &dyn Reporter, request: &ProveRequest<'_>) -> i32 {
    if let Some(proof) = request.check_proof {
        return check_proof(reporter, request, proof);
    }
    let loaded = match source_paths(request.inputs, request.dir) {
        Ok(paths) => match load_theory(&paths) {
            Ok(loaded) => loaded,
            Err(failure) => return publish_failure(reporter, failure.into_report(), request),
        },
        Err(diagnostic) => {
            return publish_failure(
                reporter,
                ProveReport::blank(DecisionClass::Malformed, diagnostic),
                request,
            );
        }
    };

    let selection = SourceSelection::all_classical_roots(&loaded.theory);
    let admission = SourceAdmission::classical_fof(&loaded.theory, selection);
    if admission.status == SourceAdmissionStatus::Malformed {
        let detail = admission
            .blockers()
            .filter_map(|unit| match &unit.disposition {
                SemanticUnitDisposition::Malformed { code, reason } => {
                    Some(format!("{} {code}: {reason}", unit.anchor))
                }
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("; ");
        let mut report = ProveReport::blank(
            DecisionClass::Malformed,
            with_inputs(
                failure("SOURCE_ADMISSION_MALFORMED", detail),
                &loaded.identities,
            ),
        );
        report.admission = Some(admission);
        return publish_failure(reporter, report, request);
    }

    // The admission and the engine read one compiled theory; a mismatch is a broken
    // invariant of this command, not a property of the selection.
    if !admission.matches(&loaded.theory) {
        let mut report = ProveReport::blank(
            DecisionClass::ExecutionFailure,
            with_inputs(
                failure(
                    "ADMISSION_THEORY_MISMATCH",
                    "the admission does not bind the compiled theory the native engine executes",
                ),
                &loaded.identities,
            ),
        );
        report.admission = Some(admission);
        return publish_failure(reporter, report, request);
    }

    let canonical = loaded.theory.source().dataset();
    let mut drops = admission_drops(&admission);
    drops.extend(unread_program_axioms(&loaded.theory));
    drops.extend(accounting_drops(&admission, &loaded.theory));
    let legs = native_leg(&loaded.theory, canonical, request.max_steps, &mut drops)
        .and_then(|native| owl_dl_leg(canonical, request.step_cap).map(|owl| (native, owl)));
    let (native, (owl_dl, proof)) = match legs {
        Ok(legs) => legs,
        Err(diagnostic) => {
            let mut report = ProveReport::blank(
                DecisionClass::ExecutionFailure,
                with_inputs(diagnostic, &loaded.identities),
            );
            report.admission = Some(admission);
            return publish_failure(reporter, report, request);
        }
    };

    // The one computation that decides `complete`; the JSON field, the text line,
    // the preservation axis and the verdict all read it. It covers the native
    // accounting only: the OWL leg's own state is reported under that leg.
    let drops: Vec<DropRecord> = drops.into_iter().collect();
    let complete = admission.status == SourceAdmissionStatus::Complete && drops.is_empty();
    let mut refutations = native.refutations.clone();
    if owl_dl.clash() {
        refutations.push(Refutation {
            engine: ENGINE_OWL_DL,
            law: "owl2-direct-semantics-tableau-refutation".to_owned(),
            subject: None,
            world: None,
            premises: Vec::new(),
            proof_digest: owl_dl.proof_digest.clone(),
        });
    }
    let verdict = if native.clash() || owl_dl.clash() {
        VERDICT_INCONSISTENT
    } else if complete {
        VERDICT_CONSISTENT
    } else {
        VERDICT_NOT_DECIDED
    };
    let (decision, information, completeness, evidence, gate_admission) = match verdict {
        VERDICT_CONSISTENT => (
            DecisionClass::Positive,
            "supported",
            "complete-for-fragment",
            EvidenceGrade::Certificate,
            GateAdmission::Certificate,
        ),
        VERDICT_INCONSISTENT => (
            DecisionClass::Negative,
            "opposed",
            if complete {
                "complete-for-fragment"
            } else {
                "incomplete"
            },
            EvidenceGrade::Certificate,
            GateAdmission::Refused,
        ),
        _ => (
            DecisionClass::Undecided,
            "undetermined",
            "incomplete",
            EvidenceGrade::Attestation,
            GateAdmission::Attestation,
        ),
    };
    let boundaries: Vec<ReasoningBoundary> = drops
        .iter()
        .map(|drop| ReasoningBoundary {
            code: drop.code.clone(),
            detail: format!("{}: {}", drop.engine, drop.detail),
        })
        .collect();
    let preservation = if complete {
        PreservationReport::exact()
    } else {
        PreservationReport::sound_under(
            drops
                .iter()
                .map(|drop| drop.code.clone())
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect(),
        )
    };
    let documents = loaded.theory.source().origins().len();
    let mut report = ProveReport {
        common: ReasoningReportCore {
            schema_version: 1,
            operation: ReasoningOperation::AxiomConsistency,
            verdict: verdict.to_owned(),
            decision,
            input_status: "valid".to_owned(),
            evaluation_status: if native.evaluation.as_deref() == Some("completed") {
                "completed"
            } else {
                "incomplete"
            }
            .to_owned(),
            completeness: completeness.to_owned(),
            information_state: information.to_owned(),
            preservation,
            evidence_grade: evidence,
            gate_admission,
            declared_fragment: Some(PROVE_PROFILE.to_owned()),
            inputs: loaded.identities.clone(),
            program: ProgramIdentity {
                profile: PROVE_PROFILE.to_owned(),
                content_digest: admission.program_digest.clone(),
            },
            engine: EngineIdentity::gmeow_native(),
            metrics: ReasoningMetrics {
                decisions: owl_dl.decisions,
                steps: native.consumed_steps,
                budget: native.allowance,
            },
            boundaries,
            diagnostics: Vec::new(),
        },
        selection: Some(SelectionReport {
            complete,
            source_digest: admission.source_digest.clone(),
            program_digest: admission.program_digest.clone(),
            selection_digest: admission.selection_digest.clone(),
            documents,
            units: admission.units.len(),
            units_with_evidence_left_out: admission
                .units
                .iter()
                .filter(|unit| unit.evidence_left_out.is_some())
                .count(),
            evidence_left_out: evidence_left_out_coordinates(&admission),
            drops,
        }),
        semantics: PROVE_SEMANTICS,
        refutations,
        engines: Some(EngineReports { native, owl_dl }),
        export: None,
        admission: None,
    };

    if let Some(path) = request.out {
        match export_tptp(&loaded.theory, &admission, path) {
            Ok(export) => report.export = Some(export),
            Err(diagnostic) => {
                report.admission = Some(admission);
                report.execution_failure(diagnostic);
                return publish_failure(reporter, report, request);
            }
        }
    }
    report.admission = Some(admission);
    if let Some(path) = request.proof_out {
        let written = match &proof {
            Some(bytes) => std::fs::write(path, bytes).map_err(|error| {
                caused_failure(
                    "PROOF_WRITE_FAILED",
                    error,
                    format!("write proof {}", path.display()),
                )
            }),
            None => Err(failure(
                "PROOF_NOT_RECORDED",
                "--proof-out was selected but the OWL 2 DL leg recorded no proof for this selection",
            )),
        };
        if let Err(diagnostic) = written {
            report.execution_failure(diagnostic);
            return publish_failure(reporter, report, request);
        }
    }
    publish_report(report, request, reporter)
}

/// The evidence coordinates any unit of the admission left out of the problem,
/// sorted. Evidence is attribution, not a claim, so naming it here is accounting
/// and not a drop.
fn evidence_left_out_coordinates(admission: &SourceAdmission) -> Vec<&'static str> {
    admission
        .units
        .iter()
        .filter_map(|unit| unit.evidence_left_out.as_ref())
        .flat_map(|left_out| left_out.coordinates.iter().copied())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .map(EvidenceCoordinate::name)
        .collect()
}

/// Every admission unit the selection did not admit as a theory member and did not
/// account for as structure, quotation or metatheory configuration.
fn admission_drops(admission: &SourceAdmission) -> BTreeSet<DropRecord> {
    let mut drops = BTreeSet::new();
    for unit in &admission.units {
        let drop = match &unit.disposition {
            SemanticUnitDisposition::Blocked { code, reason } => DropRecord::new(
                ENGINE_ADMISSION,
                code.clone(),
                format!("{}: {reason}", unit.anchor),
            ),
            SemanticUnitDisposition::OutsideSelection { reason } => DropRecord::new(
                ENGINE_ADMISSION,
                "OUTSIDE_SELECTION",
                format!("{}: {reason}", unit.anchor),
            ),
            // A compiler warning names source content the compiler skipped or
            // could not lower; it is never silently part of a decided selection.
            SemanticUnitDisposition::Observed { detail } => DropRecord::new(
                ENGINE_ADMISSION,
                "COMPILER_OBSERVATION",
                format!("{}: {detail}", unit.anchor),
            ),
            // Accounted unit by unit against THE list in `accounting_drops`.
            SemanticUnitDisposition::Admitted(_) => continue,
            // Accounted: a reasoning-contract declaration states how a theory may be
            // reasoned over and asserts no object-level fact or law. `prove` decides
            // the object theory under its own fixed profile (`semantics`), so the
            // declaration cannot add or remove a model of that theory.
            SemanticUnitDisposition::ConfigurationDeclaration { .. } => continue,
            // Accounted: list cells, term carriers, imports and formula nodes carry no
            // truth claim of their own; the unit that owns them is accounted instead.
            SemanticUnitDisposition::StructuralSupport { .. } => continue,
            // Accounted: a quoted, unasserted triple term is mentioned, not asserted,
            // so it constrains no model.
            SemanticUnitDisposition::QuotedUnasserted { .. } => continue,
            // Unreachable here: a malformed unit makes the admission Malformed, which
            // `prove` reports as malformed input before any engine runs.
            SemanticUnitDisposition::Malformed { .. } => continue,
        };
        drops.insert(drop);
    }
    if admission.status == SourceAdmissionStatus::Empty {
        drops.insert(DropRecord::new(
            ENGINE_ADMISSION,
            "EMPTY_SELECTION",
            "the selection admitted no theory unit to decide",
        ));
    }
    drops
}

/// Axioms the compiler produced only from a top-level formula. The native engine
/// reads asserted statements, rules and full formulas; a formula routed to the
/// axiom table has no asserted statement for it to read.
fn unread_program_axioms(
    theory: &gmeow_logic_compile::frontend::CompiledTheory,
) -> BTreeSet<DropRecord> {
    theory
        .axiom_sources()
        .iter()
        .enumerate()
        .filter(|(_, sources)| {
            sources
                .iter()
                .any(|source| matches!(source, AxiomSource::Formula(_)))
                && !sources.iter().any(|source| {
                    matches!(
                        source,
                        AxiomSource::Statement { .. } | AxiomSource::ClassExpression(_)
                    )
                })
        })
        .map(|(index, _)| {
            let axiom = &theory.program().axioms[index];
            DropRecord::new(
                ENGINE_NATIVE,
                "FORMULA_AXIOM_NOT_READ",
                format!(
                    "program:axiom:{index}: atomic formula {} {} {} is not an asserted statement the native engine reads",
                    axiom.subject,
                    axiom.predicate,
                    axiom.obj.key()
                ),
            )
        })
        .collect()
}

/// Run the native rule engine over the whole selection and add every construct it
/// did not decide to `drops`. An engine that declines the program for a reason it
/// names did not fail: the leg is `refused` and the reasons are drops, so the
/// selection is not decided. Any other engine error is an execution failure, never
/// a drop: a canonicalization search that reached its limit is
/// `SOURCE_CANONICALIZATION_LIMIT`, and every other error is
/// `NATIVE_EXECUTION_FAILED`.
fn native_leg(
    theory: &gmeow_logic_compile::frontend::CompiledTheory,
    canonical: &RdfDataset,
    max_steps: Option<u64>,
    drops: &mut BTreeSet<DropRecord>,
) -> Result<NativeLeg, Diag> {
    let run = (|| {
        let input = gmeow_logic::reason::prepare_reasoning_input(canonical)?;
        let domains = SelectedDomains::new([SelectedLogicalWorld::new(
            LogicalGraph::Default,
            DomainProfile::NonemptyObjectDomainV1,
            PROVE_WORLD_AUTHORITY.to_owned(),
            *input.ingress_contract(),
        )?])?;
        let result = gmeow_logic::reason::reason_program_within(
            theory.program(),
            input,
            &domains,
            &gmeow_logic::query_ir::Budget {
                max_answers: None,
                max_steps,
            },
        )?;
        let verdict = result.native_verdict()?;
        let chase_certificates = result.native_execution()?.chase_certificates.clone();
        Ok::<_, Diag>((result, verdict, chase_certificates))
    })();
    let (result, verdict, chase_certificates) = match run {
        Ok(outcome) => outcome,
        Err(error) => {
            if let Some(budget) = diag_canonicalization_limit(&error).copied() {
                return Err(canonicalization_limit_failure(
                    &budget,
                    "native closure",
                    error,
                    &[],
                ));
            }
            if let Some(refusal) = error.downcast_ref::<gmeow_logic::error::NativeRefusal>() {
                drops.insert(DropRecord::new(
                    ENGINE_NATIVE,
                    "NATIVE_REFUSED",
                    refusal.reason.clone(),
                ));
                for gap in &refusal.gaps {
                    drops.insert(DropRecord::new(
                        ENGINE_NATIVE,
                        "NATIVE_REFUSAL_REASON",
                        gap.clone(),
                    ));
                }
                return Ok(NativeLeg {
                    verdict: "refused",
                    information_state: None,
                    evaluation: None,
                    completeness: None,
                    consumed_steps: None,
                    allowance: None,
                    inconsistencies: Vec::new(),
                    unsatisfiable_classes: Vec::new(),
                    coverage: None,
                    refutations: Vec::new(),
                });
            }
            return Err(caused_diagnostic(
                "NATIVE_EXECUTION_FAILED",
                error,
                "run the native engine over the selection",
                serde_json::Value::Null,
            ));
        }
    };
    let gap_lines: BTreeSet<String> = verdict
        .gaps
        .iter()
        .map(|gap| format!("{}: {}", gap.code, gap.message))
        .collect();
    for gap in &verdict.gaps {
        drops.insert(DropRecord::new(
            ENGINE_NATIVE,
            gap.code.clone(),
            gap.message.clone(),
        ));
    }
    for family in &verdict.coverage.unsupported {
        drops.insert(DropRecord::new(
            ENGINE_NATIVE,
            "UNDECIDED_CONSTRUCT_FAMILY",
            family.clone(),
        ));
    }
    for finding in &verdict.boundary_findings {
        drops.insert(DropRecord::new(
            ENGINE_NATIVE,
            finding.code.clone(),
            finding.message.clone(),
        ));
    }
    // Lowering residue and withheld reads are on the preservation axis only.
    for construct in &result.preservation.unsupported_constructs {
        if !verdict.coverage.unsupported.contains(construct) && !gap_lines.contains(construct) {
            drops.insert(DropRecord::new(
                ENGINE_NATIVE,
                "UNSUPPORTED_CONSTRUCT",
                construct.clone(),
            ));
        }
    }
    if result.evaluation != gmeow_logic::result::EvaluationStatus::Completed {
        drops.insert(DropRecord::new(
            ENGINE_NATIVE,
            "NATIVE_EVALUATION_INCOMPLETE",
            format!(
                "the native closure ended {} after {} of {} allowed steps",
                result.evaluation.wire(),
                result.provenance.consumed_budget.consumed,
                result
                    .provenance
                    .consumed_budget
                    .allowance
                    .map_or_else(|| "unbounded".to_owned(), |steps| steps.to_string())
            ),
        ));
    }
    for certificate in &chase_certificates {
        if let gmeow_logic::materialize::ChaseAdmission::Uncertified { violations } =
            &certificate.admission
        {
            drops.insert(DropRecord::new(
                ENGINE_NATIVE,
                "CHASE_UNCERTIFIED",
                format!(
                    "world {}: the existential chase has no termination certificate: {}",
                    certificate.world,
                    violations.join("; ")
                ),
            ));
        }
    }
    for assumption in &result.provenance.assumptions {
        if !matches!(
            assumption,
            gmeow_logic::result::Assumption::OpenWorld
                | gmeow_logic::result::Assumption::SkolemWitness
        ) {
            drops.insert(DropRecord::new(
                ENGINE_NATIVE,
                "NONCLASSICAL_ASSUMPTION",
                format!("the native result rests on the {assumption} assumption"),
            ));
        }
    }
    drops.extend(equality_drops(result.inferred()));
    // The engine's own conclusive state is required; the drops above only name why.
    if verdict.consistent && result.information != gmeow_logic::result::InformationState::Supported
    {
        drops.insert(DropRecord::new(
            ENGINE_NATIVE,
            "NATIVE_NOT_SUPPORTED",
            format!(
                "the native closure reports information state {} rather than supported",
                result.information.wire()
            ),
        ));
    }
    let mut refutations: Vec<Refutation> = result
        .inferred()
        .iter()
        .filter(|axiom| {
            axiom.object.as_iri().and_then(|iri| {
                gmeow_logic::reason::dl::EmptyClassAssertion::classify(&axiom.predicate, iri)
            }) == Some(gmeow_logic::reason::dl::EmptyClassAssertion::Membership)
        })
        .map(|axiom| Refutation {
            engine: ENGINE_NATIVE,
            law: axiom
                .rule_name
                .clone()
                .unwrap_or_else(|| "asserted-empty-class-membership".to_owned()),
            subject: Some(axiom.subject.clone()),
            world: Some(axiom.world.clone()),
            premises: axiom.premises.clone(),
            proof_digest: None,
        })
        .collect();
    if !verdict.consistent && refutations.is_empty() {
        refutations.push(Refutation {
            engine: ENGINE_NATIVE,
            law: "native-family-conflict".to_owned(),
            subject: None,
            world: None,
            premises: verdict
                .inconsistencies
                .iter()
                .flat_map(|witness| witness.premises.iter().cloned())
                .collect(),
            proof_digest: None,
        });
    }
    refutations.sort();
    refutations.dedup();
    let mut inconsistencies = verdict.inconsistencies;
    inconsistencies.sort_by(|left, right| {
        (&left.world, &left.individual, &left.premises).cmp(&(
            &right.world,
            &right.individual,
            &right.premises,
        ))
    });
    let mut unsatisfiable_classes = verdict.unsatisfiable_classes;
    unsatisfiable_classes.sort_by(|left, right| {
        (&left.world, &left.class, &left.premises).cmp(&(
            &right.world,
            &right.class,
            &right.premises,
        ))
    });
    Ok(NativeLeg {
        verdict: if verdict.consistent {
            "no-clash"
        } else {
            "clash"
        },
        information_state: Some(result.information.wire().to_owned()),
        evaluation: Some(result.evaluation.wire().to_owned()),
        completeness: Some(result.completeness.wire().to_owned()),
        consumed_steps: Some(result.provenance.consumed_budget.consumed),
        allowance: result.provenance.consumed_budget.allowance,
        inconsistencies,
        unsatisfiable_classes,
        coverage: Some(verdict.coverage),
        refutations,
    })
}

/// Why the OWL view could not be opened for the tableau.
enum OwlOpenFailure {
    /// A canonicalization search reached its finite limit: an execution failure.
    Limit(Diag),
    /// The view could not be projected or given a proof identity.
    NotBuilt(&'static str, Diag),
}

/// Open the recording tableau over the selection's OWL view and prepare it for the
/// consistency question.
fn open_owl_view(
    canonical: &RdfDataset,
    step_cap: Option<u64>,
) -> Result<(Arc<RdfDataset>, DlReasoner), OwlOpenFailure> {
    let classify =
        |code: &'static str, stage: &str, error: Diag| match diag_canonicalization_limit(&error)
            .copied()
        {
            Some(budget) => {
                OwlOpenFailure::Limit(canonicalization_limit_failure(&budget, stage, error, &[]))
            }
            None => OwlOpenFailure::NotBuilt(code, error),
        };
    let view = gmeow_logic_compile::projections::reader_view::with_owl_rdfs_projection(canonical)
        .map_err(|error| classify("OWL_VIEW_FAILED", "OWL view projection", error))?;
    let mut reasoner = DlReasoner::with_proofs_and_step_cap(view.as_ref(), step_cap)
        .map_err(|error| classify("OWL_VIEW_FAILED", "OWL view proof identity", error))?;
    reasoner
        .prepare(&Question::Consistency)
        .map_err(|error| classify("OWL_VIEW_REFUSED", "OWL view preparation", error))?;
    Ok((view, reasoner))
}

/// Ask the tableau for consistency of the OWL view and record its proof bytes. Only
/// a canonicalization limit is an execution failure; every other outcome is a state
/// of this leg.
fn owl_dl_leg(
    canonical: &RdfDataset,
    step_cap: Option<u64>,
) -> Result<(OwlDlLeg, Option<Vec<u8>>), Diag> {
    let reasoner = match open_owl_view(canonical, step_cap) {
        Ok((_view, reasoner)) => reasoner,
        Err(OwlOpenFailure::Limit(diagnostic)) => return Err(diagnostic),
        Err(OwlOpenFailure::NotBuilt(code, error)) => return Ok((owl_refusal(code, &error), None)),
    };
    let certified = reasoner.consistency();
    let certificate = certified.certificate();
    let boundaries: Vec<OwlNote> = certificate
        .boundaries()
        .iter()
        .map(|boundary| OwlNote {
            code: boundary.construct().to_string(),
            detail: boundary.reason().to_owned(),
        })
        .collect();
    let decided = certificate.completeness().is_decided();
    let state = match certified.answer() {
        purrdf::entail::Verdict::True if decided => OWL_CONSISTENT,
        purrdf::entail::Verdict::False if decided => OWL_INCONSISTENT,
        _ => OWL_NOT_DECIDED,
    };
    let mut notes = Vec::new();
    if state == OWL_NOT_DECIDED {
        notes.push(OwlNote {
            code: "OWL_SEARCH_NOT_DECIDED".to_owned(),
            detail: format!(
                "the tableau ended {} at {} of {} steps and {} of {} work units without deciding",
                certificate.completeness(),
                certificate.steps(),
                certificate.budget(),
                certificate.work(),
                certificate.work_budget()
            ),
        });
    }
    let proof = certified.proof().map(purrdf::entail::ServiceProof::encode);
    let leg = OwlDlLeg {
        state,
        answer: Some(certified.answer().to_string()),
        completeness: Some(certificate.completeness().to_string()),
        steps: Some(certificate.steps()),
        budget: Some(certificate.budget()),
        work: Some(certificate.work()),
        work_budget: Some(certificate.work_budget()),
        decisions: Some(certificate.decisions()),
        boundaries,
        proof_digest: certified
            .proof()
            .map(purrdf::entail::ServiceProof::digest_hex),
        proof_claim: certified.proof().map(|proof| proof_claims(proof).join(" ")),
        notes,
    };
    Ok((leg, proof))
}

/// An OWL view that could not be opened. An ontology the reverse mapping proves has
/// no model is a refutation; every other refusal leaves the view unread.
fn owl_refusal(code: &str, error: &Diag) -> OwlDlLeg {
    let kind = error
        .downcast_ref::<gmeow_logic::error::DlService>()
        .map(|failure| failure.kind);
    if kind == Some(gmeow_logic::reasoner_services::DlServiceFailureKind::NoModel) {
        let mut leg =
            OwlDlLeg::not_built("OWL_NO_MODEL_AT_PREPARATION", error.message().to_owned());
        leg.state = OWL_INCONSISTENT;
        leg.answer = Some("false".to_owned());
        leg.completeness = Some("no-model-at-preparation".to_owned());
        return leg;
    }
    OwlDlLeg::not_built(code, error.message().to_owned())
}

/// Export the selection's TPTP FOF projection. A blocked projection writes no
/// file and removes one left at the path by an earlier run, so the path never holds
/// stale bytes beside `written: false`; its blockers are reported and the verdict
/// is unaffected.
fn export_tptp(
    theory: &gmeow_logic_compile::frontend::CompiledTheory,
    admission: &SourceAdmission,
    path: &Path,
) -> gmeow_errors::Result<TptpExport> {
    let projection = project_tptp_fof(theory, admission);
    let mut export = TptpExport {
        path: path.display().to_string(),
        written: false,
        status: projection.status,
        profile_id: projection.profile_id.clone(),
        problem_digest: projection.problem_digest.clone(),
        sentences: projection.sentences.len(),
        blockers: projection.blockers.clone(),
    };
    let (FofProjectionStatus::Complete, Some(problem)) =
        (projection.status, projection.problem.as_deref())
    else {
        match std::fs::remove_file(path) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(caused_failure(
                    "PROBLEM_STALE_REMOVE_FAILED",
                    error,
                    format!("remove stale TPTP export {}", path.display()),
                ));
            }
        }
        return Ok(export);
    };
    std::fs::write(path, problem).map_err(|error| {
        caused_failure(
            "PROBLEM_WRITE_FAILED",
            error,
            format!("write TPTP export {}", path.display()),
        )
    })?;
    let read_back = std::fs::read(path).map_err(|error| {
        caused_failure(
            "PROBLEM_VERIFY_READ_FAILED",
            error,
            format!("verify TPTP export {}", path.display()),
        )
    })?;
    if read_back != problem.as_bytes() {
        return Err(failure(
            "PROBLEM_CONTENT_CHANGED",
            format!(
                "TPTP export {} does not contain the emitted bytes",
                path.display()
            ),
        ));
    }
    export.written = true;
    Ok(export)
}

/// Report of one `--check-proof` replay.
#[derive(Debug, Clone, Serialize)]
struct ProofCheckReport {
    schema_version: u32,
    operation: &'static str,
    /// `replayed`, `rejected`, `malformed` or `execution-failure`.
    verdict: &'static str,
    inputs: Vec<InputIdentity>,
    proof_path: String,
    proof_digest: Option<String>,
    /// The claims the proof states, for example `owl-view-consistent`. They are
    /// verified exactly when `verdict` is `replayed`.
    claims: Vec<String>,
    runs: Option<usize>,
    replayed_runs: Option<usize>,
    diagnostics: Vec<ReportDiagnostic>,
}

impl ProofCheckReport {
    fn exit_code(&self) -> i32 {
        match self.verdict {
            "replayed" => 0,
            "rejected" => EXIT_PROOF_REJECTED,
            "malformed" => super::reasoning_report::EXIT_MALFORMED,
            _ => super::reasoning_report::EXIT_EXECUTION,
        }
    }
}

/// Re-prepare the same selection and replay a recorded tableau proof against its
/// OWL view without making a new solver decision. PurRDF's verifier owns the check:
/// it binds the proof to the canonical identity of this OWL view and to the
/// consistency question, and checks every stated claim against the runs it replays.
/// The report prints those claims. The native engine has no replayable proof; its
/// verdict is reproduced by re-running `gmeow prove` on the same inputs.
fn check_proof(reporter: &dyn Reporter, request: &ProveRequest<'_>, proof_path: &Path) -> i32 {
    let mut report = ProofCheckReport {
        schema_version: 1,
        operation: "proof-check",
        verdict: "malformed",
        inputs: Vec::new(),
        proof_path: proof_path.display().to_string(),
        proof_digest: None,
        claims: Vec::new(),
        runs: None,
        replayed_runs: None,
        diagnostics: Vec::new(),
    };
    let loaded = match source_paths(request.inputs, request.dir) {
        Ok(paths) => load_theory(&paths),
        Err(diagnostic) => Err(LoadFailure::Malformed(diagnostic)),
    };
    let loaded = match loaded {
        Ok(loaded) => loaded,
        Err(LoadFailure::Malformed(diagnostic)) => {
            report.inputs = recorded_inputs(&diagnostic);
            return publish_check(reporter, report, "malformed", &diagnostic, request);
        }
        Err(LoadFailure::Execution(diagnostic)) => {
            report.inputs = recorded_inputs(&diagnostic);
            return publish_check(reporter, report, "execution-failure", &diagnostic, request);
        }
    };
    report.inputs.clone_from(&loaded.identities);
    let bytes = match std::fs::read(proof_path) {
        Ok(bytes) => bytes,
        Err(error) => {
            let diagnostic = caused_failure(
                "PROOF_READ_FAILED",
                error,
                format!("read proof {}", proof_path.display()),
            );
            return publish_check(reporter, report, "malformed", &diagnostic, request);
        }
    };
    let proof = match purrdf::entail::ServiceProof::decode(&bytes) {
        Ok(proof) => proof,
        Err(error) => {
            let diagnostic = caused_failure(
                "PROOF_DECODE_REJECTED",
                error,
                format!("decode proof {}", proof_path.display()),
            );
            return publish_check(reporter, report, "rejected", &diagnostic, request);
        }
    };
    report.proof_digest = Some(proof.digest_hex());
    report.runs = Some(proof.runs().len());
    report.claims = proof_claims(&proof);
    let canonical = loaded.theory.source().dataset();
    let (view, reasoner) = match open_owl_view(canonical, request.step_cap) {
        Ok(opened) => opened,
        Err(OwlOpenFailure::Limit(diagnostic)) => {
            return publish_check(reporter, report, "execution-failure", &diagnostic, request);
        }
        Err(OwlOpenFailure::NotBuilt(code, error)) => {
            let diagnostic = with_inputs(
                caused_diagnostic(
                    code,
                    error,
                    "open the OWL view for proof replay",
                    serde_json::Value::Null,
                ),
                &loaded.identities,
            );
            return publish_check(reporter, report, "execution-failure", &diagnostic, request);
        }
    };
    let context = match reasoner.borrowed_proof_context() {
        Ok(context) => context,
        Err(error) => {
            let diagnostic = caused_diagnostic(
                "PROOF_CONTEXT_FAILED",
                error,
                "borrow the replay context",
                serde_json::Value::Null,
            );
            return publish_check(reporter, report, "execution-failure", &diagnostic, request);
        }
    };
    let replay = proof.verify(
        view.as_ref(),
        &Question::Consistency,
        None,
        &context,
        &mut purrdf::CanonicalizationControl::new(purrdf::RDFC_V2_CORPUS_POLICY, None),
    );
    match replay {
        Ok(replay) => {
            report.replayed_runs = Some(replay.replayed());
            report.verdict = "replayed";
            publish_check_report(report, request)
        }
        Err(error) => {
            if let Some(budget) = canonicalization_limit(&error).copied() {
                let diagnostic = canonicalization_limit_failure(
                    &budget,
                    "proof source binding",
                    failure("PROOF_SOURCE_BINDING", error.to_string()),
                    &loaded.identities,
                );
                return publish_check(reporter, report, "execution-failure", &diagnostic, request);
            }
            let diagnostic = caused_failure(
                "PROOF_REJECTED",
                error,
                format!("replay proof {}", proof_path.display()),
            );
            publish_check(reporter, report, "rejected", &diagnostic, request)
        }
    }
}

fn publish_check(
    reporter: &dyn Reporter,
    mut report: ProofCheckReport,
    verdict: &'static str,
    diagnostic: &Diag,
    request: &ProveRequest<'_>,
) -> i32 {
    let projected = ReportDiagnostic::from_diag(diagnostic);
    emit_error(reporter, &projected.code, projected.detail.clone());
    report.verdict = verdict;
    report.diagnostics.push(projected);
    publish_check_report(report, request)
}

fn publish_check_report(report: ProofCheckReport, request: &ProveRequest<'_>) -> i32 {
    let json = serde_json::to_string_pretty(&report)
        .expect("typed proof-check report serialization is infallible");
    if let Some(path) = request.evidence_out
        && let Err(error) = std::fs::write(path, json.as_bytes())
    {
        let diagnostic = caused_failure(
            "EVIDENCE_WRITE_FAILED",
            error,
            format!("write evidence report {}", path.display()),
        );
        let mut report = report;
        report.verdict = "execution-failure";
        report
            .diagnostics
            .push(ReportDiagnostic::from_diag(&diagnostic));
        return render_check(&report, request.format);
    }
    render_check(&report, request.format)
}

fn render_check(report: &ProofCheckReport, format: OutputFormat) -> i32 {
    match format {
        OutputFormat::Json => println!(
            "{}",
            serde_json::to_string_pretty(report)
                .expect("typed proof-check report serialization is infallible")
        ),
        OutputFormat::Text => {
            println!("verdict {}", report.verdict);
            println!("operation {}", report.operation);
            println!("sources {}", report.inputs.len());
            println!("proof {}", report.proof_path);
            if let Some(digest) = &report.proof_digest {
                println!("proof-digest {digest}");
            }
            for claim in &report.claims {
                println!("proof-claim {claim}");
            }
            if let Some(runs) = report.runs {
                println!("proof-runs {runs}");
            }
            if let Some(replayed) = report.replayed_runs {
                println!("replayed-runs {replayed}");
            }
            for diagnostic in &report.diagnostics {
                println!("diagnostic {} {}", diagnostic.code, diagnostic.detail);
            }
        }
    }
    report.exit_code()
}

const RDF_NS: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#";
const RDFS_NS: &str = "http://www.w3.org/2000/01/rdf-schema#";
const OWL_NS: &str = "http://www.w3.org/2002/07/owl#";
const LOGIC_NS: &str = "https://blackcatinformatics.ca/logic/";

/// Namespaces whose terms the native engine may interpret. A term outside them is
/// an ordinary constant or relation with no engine law, so its facts are a model.
const INTERPRETED_NAMESPACES: [&str; 4] = [RDF_NS, RDFS_NS, OWL_NS, LOGIC_NS];

/// How an accounted predicate constrains the terms it relates.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Positions {
    /// Membership: the subject is any term; the object is a class or an accounted marker.
    Membership,
    /// Both positions name classes.
    Classes,
    /// Both positions name properties.
    Properties,
    /// A property and a class.
    PropertyClass,
    /// Two individuals related by equality, accounted only when they are identical.
    Equality,
    /// Two individuals related by explicit difference.
    Individuals,
    /// An annotation with no logical law.
    Annotation,
}

/// THE accounting list: every interpreted predicate whose law the completed native
/// closure satisfies, so that a completed closure without a clash is a model of the
/// units that use it. A predicate in an interpreted namespace that is not listed
/// here is a named drop. When in doubt a construct is not on this list.
const ACCOUNTED_PREDICATES: &[(&str, Positions, &str)] = &[
    (
        "http://www.w3.org/1999/02/22-rdf-syntax-ns#type",
        Positions::Membership,
        "membership fact; the object is checked against ACCOUNTED_MARKERS",
    ),
    (
        "https://blackcatinformatics.ca/logic/instanceOf",
        Positions::Membership,
        "membership fact; the object is checked against ACCOUNTED_MARKERS",
    ),
    (
        "https://blackcatinformatics.ca/logic/subClassOf",
        Positions::Classes,
        "Horn law closed by el:type-propagation and el:subClassOf-transitive",
    ),
    (
        "http://www.w3.org/2000/01/rdf-schema#subClassOf",
        Positions::Classes,
        "Horn law closed by el:type-propagation and el:subClassOf-transitive",
    ),
    (
        "https://blackcatinformatics.ca/logic/equivalentClass",
        Positions::Classes,
        "Horn law closed by el:equivalentClass-fwd and el:equivalentClass-bwd",
    ),
    (
        "http://www.w3.org/2002/07/owl#equivalentClass",
        Positions::Classes,
        "Horn law closed by el:equivalentClass-fwd and el:equivalentClass-bwd",
    ),
    (
        "https://blackcatinformatics.ca/logic/disjointWith",
        Positions::Classes,
        "denial checked by dl:individual-clash and dl:unsatisfiable-class",
    ),
    (
        "http://www.w3.org/2002/07/owl#disjointWith",
        Positions::Classes,
        "denial checked by dl:individual-clash and dl:unsatisfiable-class",
    ),
    (
        "https://blackcatinformatics.ca/logic/subPropertyOf",
        Positions::Properties,
        "Horn law closed by subproperty propagation and el:subPropertyOf-transitive",
    ),
    (
        "http://www.w3.org/2000/01/rdf-schema#subPropertyOf",
        Positions::Properties,
        "Horn law closed by subproperty propagation and el:subPropertyOf-transitive",
    ),
    (
        "https://blackcatinformatics.ca/logic/equivalentProperty",
        Positions::Properties,
        "Horn law closed by dl:equivalentProperty-subproperty",
    ),
    (
        "http://www.w3.org/2002/07/owl#equivalentProperty",
        Positions::Properties,
        "Horn law closed by dl:equivalentProperty-subproperty",
    ),
    (
        "https://blackcatinformatics.ca/logic/inverseOf",
        Positions::Properties,
        "Horn law closed by dl:inverseOf and dl:inverseOf-symmetry",
    ),
    (
        "http://www.w3.org/2002/07/owl#inverseOf",
        Positions::Properties,
        "Horn law closed by dl:inverseOf and dl:inverseOf-symmetry",
    ),
    (
        "https://blackcatinformatics.ca/logic/domain",
        Positions::PropertyClass,
        "Horn law closed by dl:domain",
    ),
    (
        "http://www.w3.org/2000/01/rdf-schema#domain",
        Positions::PropertyClass,
        "Horn law closed by dl:domain",
    ),
    (
        "https://blackcatinformatics.ca/logic/range",
        Positions::PropertyClass,
        "Horn law closed by dl:range",
    ),
    (
        "http://www.w3.org/2000/01/rdf-schema#range",
        Positions::PropertyClass,
        "Horn law closed by dl:range",
    ),
    (
        "https://blackcatinformatics.ca/logic/sameAs",
        Positions::Equality,
        "reflexive equality needs no substitution; any other equality is a drop",
    ),
    (
        "http://www.w3.org/2002/07/owl#sameAs",
        Positions::Equality,
        "reflexive equality needs no substitution; any other equality is a drop",
    ),
    (
        "https://blackcatinformatics.ca/logic/differentFrom",
        Positions::Individuals,
        "denial checked by dl:same-different-clash; without equality named terms stay distinct",
    ),
    (
        "http://www.w3.org/2002/07/owl#differentFrom",
        Positions::Individuals,
        "denial checked by dl:same-different-clash; without equality named terms stay distinct",
    ),
    (
        "http://www.w3.org/2000/01/rdf-schema#label",
        Positions::Annotation,
        "annotation without a law",
    ),
    (
        "http://www.w3.org/2000/01/rdf-schema#comment",
        Positions::Annotation,
        "annotation without a law",
    ),
    (
        "http://www.w3.org/2000/01/rdf-schema#isDefinedBy",
        Positions::Annotation,
        "annotation without a law",
    ),
    (
        "http://www.w3.org/2000/01/rdf-schema#seeAlso",
        Positions::Annotation,
        "annotation without a law",
    ),
];

/// Membership objects in an interpreted namespace that the accounting admits, with
/// the law the completed closure satisfies for each.
const ACCOUNTED_MARKERS: &[(&str, &str)] = &[
    (
        "http://www.w3.org/2002/07/owl#Class",
        "declaration without a law",
    ),
    (
        "http://www.w3.org/2000/01/rdf-schema#Class",
        "declaration without a law",
    ),
    (
        "https://blackcatinformatics.ca/logic/Class",
        "declaration without a law",
    ),
    (
        "http://www.w3.org/1999/02/22-rdf-syntax-ns#Property",
        "declaration without a law",
    ),
    (
        "http://www.w3.org/2002/07/owl#ObjectProperty",
        "declaration without a law",
    ),
    (
        "http://www.w3.org/2002/07/owl#NamedIndividual",
        "declaration without a law",
    ),
    (
        "http://www.w3.org/2002/07/owl#Thing",
        "the universal class holds of every element",
    ),
    (
        "https://blackcatinformatics.ca/logic/Thing",
        "the universal class holds of every element",
    ),
    (
        "http://www.w3.org/2002/07/owl#Nothing",
        "denial checked by dl:nothing-membership",
    ),
    (
        "https://blackcatinformatics.ca/logic/Nothing",
        "denial checked by dl:nothing-membership",
    ),
    (
        "http://www.w3.org/2002/07/owl#TransitiveProperty",
        "Horn law closed by dl:transitive-property",
    ),
    (
        "https://blackcatinformatics.ca/logic/transitiveProperty",
        "Horn law closed by dl:transitive-property",
    ),
    (
        "http://www.w3.org/2002/07/owl#SymmetricProperty",
        "Horn law closed by dl:symmetric-property",
    ),
    (
        "https://blackcatinformatics.ca/logic/symmetricProperty",
        "Horn law closed by dl:symmetric-property",
    ),
    (
        "http://www.w3.org/2002/07/owl#IrreflexiveProperty",
        "denial checked by dl:irreflexive-property-clash",
    ),
    (
        "https://blackcatinformatics.ca/logic/irreflexiveProperty",
        "denial checked by dl:irreflexive-property-clash",
    ),
    (
        "http://www.w3.org/2002/07/owl#AsymmetricProperty",
        "denial checked by dl:asymmetric-property-clash",
    ),
    (
        "https://blackcatinformatics.ca/logic/asymmetricProperty",
        "denial checked by dl:asymmetric-property-clash",
    ),
    (
        "http://www.w3.org/2002/07/owl#FunctionalProperty",
        "equality-generating dl:functional-equality; every derived equality is checked",
    ),
    (
        "https://blackcatinformatics.ca/logic/functionalProperty",
        "equality-generating dl:functional-equality; every derived equality is checked",
    ),
    (
        "http://www.w3.org/2002/07/owl#InverseFunctionalProperty",
        "equality-generating dl:inverse-functional-equality; every derived equality is checked",
    ),
    (
        "https://blackcatinformatics.ca/logic/inverseFunctionalProperty",
        "equality-generating dl:inverse-functional-equality; every derived equality is checked",
    ),
];

/// Record-form property characteristics the native engine reads from the record
/// itself. The other sorts are read only in marker form.
const ACCOUNTED_CHARACTERISTIC_RECORDS: &[(&str, &str)] = &[
    (
        "https://blackcatinformatics.ca/logic/functionalProperty",
        "read from the record by the functional characteristic schema",
    ),
    (
        "https://blackcatinformatics.ca/logic/inverseFunctionalProperty",
        "read from the record by the inverse-functional characteristic schema",
    ),
];

const UNIVERSAL_CLASSES: [&str; 2] = [
    "http://www.w3.org/2002/07/owl#Thing",
    "https://blackcatinformatics.ca/logic/Thing",
];
const EMPTY_CLASSES: [&str; 2] = [
    "http://www.w3.org/2002/07/owl#Nothing",
    "https://blackcatinformatics.ca/logic/Nothing",
];
const EQUALITY_PREDICATES: [&str; 2] = [
    "https://blackcatinformatics.ca/logic/sameAs",
    "http://www.w3.org/2002/07/owl#sameAs",
];

fn interpreted(iri: &str) -> bool {
    INTERPRETED_NAMESPACES
        .iter()
        .any(|namespace| iri.starts_with(namespace))
}

/// A term of an accounted unit, reduced to what accounting needs.
#[derive(Clone, Copy)]
enum UnitTerm<'a> {
    Iri(&'a str),
    Variable,
    Other,
}

impl<'a> UnitTerm<'a> {
    fn atomic(term: &'a gmeow_logic_compile::ir::AtomicTerm) -> Self {
        match term {
            gmeow_logic_compile::ir::AtomicTerm::Iri(iri) => Self::Iri(iri),
            gmeow_logic_compile::ir::AtomicTerm::Var(_) => Self::Variable,
            _ => Self::Other,
        }
    }

    fn formula(term: &'a gmeow_logic_compile::ir::Term) -> Self {
        match term {
            gmeow_logic_compile::ir::Term::Iri(iri) => Self::Iri(iri),
            gmeow_logic_compile::ir::Term::Var(_) => Self::Variable,
            _ => Self::Other,
        }
    }
}

/// Whether a class position holds a class the accounting admits in that role.
/// `universal_ok`/`empty_ok` admit the trivially satisfied and the denial readings.
fn class_accounted(term: UnitTerm<'_>, universal_ok: bool, empty_ok: bool) -> bool {
    match term {
        UnitTerm::Iri(iri) if UNIVERSAL_CLASSES.contains(&iri) => universal_ok,
        UnitTerm::Iri(iri) if EMPTY_CLASSES.contains(&iri) => empty_ok,
        UnitTerm::Iri(iri) => !interpreted(iri),
        UnitTerm::Variable | UnitTerm::Other => false,
    }
}

fn property_accounted(term: UnitTerm<'_>) -> bool {
    matches!(term, UnitTerm::Iri(iri) if !interpreted(iri))
}

/// Account one binary predication against THE list. `None` when accounted,
/// otherwise the reason it is not.
fn account_predication(
    subject: UnitTerm<'_>,
    predicate: UnitTerm<'_>,
    object: UnitTerm<'_>,
) -> Option<String> {
    let UnitTerm::Iri(predicate) = predicate else {
        return Some("a variable or non-IRI relation has no accounted law".to_owned());
    };
    if !interpreted(predicate) {
        return None;
    }
    let Some((_, positions, _)) = ACCOUNTED_PREDICATES
        .iter()
        .find(|(iri, _, _)| *iri == predicate)
    else {
        return Some(format!(
            "interpreted predicate {predicate} is not on the accounting list"
        ));
    };
    let accounted = match positions {
        Positions::Membership => match object {
            UnitTerm::Iri(class) if interpreted(class) => {
                ACCOUNTED_MARKERS.iter().any(|(marker, _)| *marker == class)
            }
            UnitTerm::Iri(_) => true,
            UnitTerm::Variable | UnitTerm::Other => false,
        },
        Positions::Classes => match predicate.rsplit(['/', '#']).next() {
            Some("subClassOf") => {
                class_accounted(subject, false, true) && class_accounted(object, true, true)
            }
            _ => class_accounted(subject, false, false) && class_accounted(object, false, false),
        },
        Positions::Properties => property_accounted(subject) && property_accounted(object),
        Positions::PropertyClass => {
            property_accounted(subject) && class_accounted(object, true, false)
        }
        Positions::Equality => matches!(
            (subject, object),
            (UnitTerm::Iri(left), UnitTerm::Iri(right)) if left == right
        ),
        Positions::Individuals | Positions::Annotation => true,
    };
    (!accounted)
        .then(|| format!("{predicate} is accounted only for the positions on its list entry"))
}

/// Account every relation a formula uses. Horn lowering decides which atoms become
/// heads; every interpreted relation in the tree must therefore be accounted.
fn account_formula(formula: &gmeow_logic_compile::ir::Formula, refusals: &mut Vec<String>) {
    use gmeow_logic_compile::ir::Formula;
    match formula {
        Formula::Atom { relation, args } => {
            let refusal = match args.as_slice() {
                [subject, object] => account_predication(
                    UnitTerm::formula(subject),
                    UnitTerm::formula(relation),
                    UnitTerm::formula(object),
                ),
                _ => match UnitTerm::formula(relation) {
                    UnitTerm::Iri(iri) if !interpreted(iri) => None,
                    _ => Some(
                        "a non-binary interpreted or variable relation has no accounted law"
                            .to_owned(),
                    ),
                },
            };
            refusals.extend(refusal);
        }
        Formula::Not(inner) => account_formula(inner, refusals),
        Formula::And(parts) | Formula::Or(parts) => {
            for part in parts {
                account_formula(part, refusals);
            }
        }
        Formula::Implies(left, right) | Formula::Iff(left, right) => {
            account_formula(left, refusals);
            account_formula(right, refusals);
        }
        Formula::Forall { body, .. } | Formula::Exists { body, .. } => {
            account_formula(body, refusals);
        }
    }
}

fn account_axiom(axiom: &gmeow_logic_compile::ir::LogicAxiom) -> Option<String> {
    let predicate = if axiom.predicate.starts_with('?') {
        UnitTerm::Variable
    } else {
        UnitTerm::Iri(&axiom.predicate)
    };
    let subject = if axiom.subject.starts_with('?') {
        UnitTerm::Variable
    } else {
        UnitTerm::Iri(&axiom.subject)
    };
    account_predication(subject, predicate, UnitTerm::atomic(&axiom.obj))
}

/// Account every admitted unit of the selection against THE list. Each unit that
/// does not map to an accounted family is a named drop.
fn accounting_drops(
    admission: &SourceAdmission,
    theory: &gmeow_logic_compile::frontend::CompiledTheory,
) -> BTreeSet<DropRecord> {
    use gmeow_logic_compile::frontend::AdmittedSemanticUnit;
    let mut drops = BTreeSet::new();
    for unit in &admission.units {
        let SemanticUnitDisposition::Admitted(admitted) = &unit.disposition else {
            continue;
        };
        let refusals: Vec<String> = match admitted {
            AdmittedSemanticUnit::Assertion(assertion) => account_predication(
                UnitTerm::atomic(&assertion.subject),
                UnitTerm::Iri(&assertion.predicate),
                UnitTerm::atomic(&assertion.object),
            )
            .into_iter()
            .collect(),
            AdmittedSemanticUnit::Rule { index } => match theory.program().rules.get(*index) {
                Some(rule) => std::iter::once(&rule.head)
                    .chain(&rule.body)
                    .filter_map(account_axiom)
                    .collect(),
                None => vec!["the admitted rule index is outside the compiled program".to_owned()],
            },
            AdmittedSemanticUnit::Formula { index } => {
                let mut refusals = Vec::new();
                match theory.program().formulas.get(*index) {
                    Some(formula) => account_formula(formula, &mut refusals),
                    None => refusals.push(
                        "the admitted formula index is outside the compiled program".to_owned(),
                    ),
                }
                refusals
            }
            AdmittedSemanticUnit::PropertyCharacteristic(record) => {
                if ACCOUNTED_CHARACTERISTIC_RECORDS
                    .iter()
                    .any(|(sort, _)| *sort == record.characteristic)
                {
                    Vec::new()
                } else {
                    drops.insert(DropRecord::new(
                        ENGINE_NATIVE,
                        "CHARACTERISTIC_RECORD_NOT_READ",
                        format!(
                            "{}: the native engine does not read a {} record for {}",
                            unit.anchor, record.characteristic, record.property
                        ),
                    ));
                    continue;
                }
            }
            AdmittedSemanticUnit::ClassDefinition(_) => {
                drops.insert(DropRecord::new(
                    ENGINE_NATIVE,
                    "CLASS_DEFINITION_NOT_ACCOUNTED",
                    format!(
                        "{}: a class-expression law is not on the accounting list; the native engine does not show its completed closure is a model of it",
                        unit.anchor
                    ),
                ));
                continue;
            }
        };
        for refusal in refusals {
            drops.insert(DropRecord::new(
                ENGINE_NATIVE,
                "UNIT_NOT_ACCOUNTED",
                format!("{}: {refusal}", unit.anchor),
            ));
        }
    }
    drops
}

/// Equality between two different terms, asserted or derived. The native engine has
/// no substitution, so a closure containing one is not known to be a model.
fn equality_drops(inferred: &[gmeow_logic::reason::InferredAxiom]) -> BTreeSet<DropRecord> {
    inferred
        .iter()
        .filter(|axiom| EQUALITY_PREDICATES.contains(&axiom.predicate.as_str()))
        .filter_map(|axiom| {
            let subject = axiom
                .subject
                .strip_prefix('<')
                .and_then(|inner| inner.strip_suffix('>'))
                .unwrap_or(&axiom.subject);
            match axiom.object.as_iri() {
                Some(object) if object == subject => None,
                object => Some(DropRecord::new(
                    ENGINE_NATIVE,
                    "EQUALITY_WITHOUT_SUBSTITUTION",
                    format!(
                        "{subject} = {} ({}); the native engine has no equality substitution",
                        object.map_or_else(|| format!("{:?}", axiom.object), ToOwned::to_owned),
                        if axiom.is_edb { "asserted" } else { "derived" }
                    ),
                )),
            }
        })
        .collect()
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

/// The canonicalization limit when it caused `error`, else the caller's malformed
/// source.
fn limit_or_malformed(
    error: Diag,
    stage: &str,
    identities: &[InputIdentity],
    malformed: impl FnOnce(Diag) -> Diag,
) -> LoadFailure {
    match diag_canonicalization_limit(&error).copied() {
        Some(budget) => LoadFailure::Execution(canonicalization_limit_failure(
            &budget, stage, error, identities,
        )),
        None => LoadFailure::Malformed(malformed(error)),
    }
}

/// Compose, canonicalize and compile the selection. Each failure is classified by
/// its cause: the caller's unreadable, unparsable or uncompilable source is
/// malformed input; a canonicalization search that reached its finite limit, a
/// storage refusal, or a broken source-binding invariant is an execution failure.
fn load_theory(paths: &[PathBuf]) -> Result<LoadedTheory, LoadFailure> {
    let mut receipts = Vec::with_capacity(paths.len());
    let mut identities = Vec::with_capacity(paths.len());
    let mut documents = Vec::with_capacity(paths.len());
    for path in paths {
        let bytes = std::fs::read(path).map_err(|error| {
            LoadFailure::Malformed(with_inputs(
                caused_failure(
                    "SOURCE_READ_FAILED",
                    error,
                    format!("cannot read {}", path.display()),
                ),
                &identities,
            ))
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
            LoadFailure::Malformed(with_inputs(failure(
                "SOURCE_SYNTAX_UNKNOWN",
                format!(
                    "cannot infer RDF syntax for {}; expected .ttl/.nt/.nq/.rdf/.owl/.xml/.trig",
                    path.display()
                ),
            ), &identities))
        })?;
        identities.push(InputIdentity::from_bytes(
            path,
            "selected-semantic-source",
            media,
            Some(base.clone()),
            &bytes,
        ));
        let dataset = purrdf::parse_dataset(&bytes, media, Some(&base)).map_err(|error| {
            LoadFailure::Malformed(with_inputs(
                caused_failure(
                    "SOURCE_PARSE_FAILED",
                    error,
                    format!("cannot parse {} as {media}", path.display()),
                ),
                &identities,
            ))
        })?;
        receipts.push(SourceDocument {
            path: path.display().to_string(),
            content_digest: purrdf_hash::hex::encode(purrdf_hash::blake3::hash(&bytes).as_bytes()),
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
        // The selected documents cannot be composed into one view.
        LoadFailure::Malformed(with_inputs(
            caused_failure(
                "SOURCE_COMPOSITION_REFUSED",
                error,
                "compose selected documents",
            ),
            &identities,
        ))
    })?;
    let materialized = composite.materialize().map_err(|error| {
        // A storage or capacity refusal over a well-formed composition.
        LoadFailure::Execution(with_inputs(
            caused_failure(
                "SOURCE_MATERIALIZATION_REFUSED",
                error,
                "materialize selected documents",
            ),
            &identities,
        ))
    })?;
    let mut prepared = PreparedLogicSource::new(&materialized).map_err(|error| {
        limit_or_malformed(error, "source canonicalization", &identities, |error| {
            with_inputs(
                caused_diagnostic(
                    "SOURCE_PREPARATION_FAILED",
                    error,
                    "compile selected theory",
                    serde_json::Value::Null,
                ),
                &identities,
            )
        })
    })?;
    for (index, (receipt, original)) in receipts.iter().zip(&documents).enumerate() {
        prepared
            .record_document(receipt.clone(), original, |term| {
                let mapped = match original.as_ref().resolve(term) {
                    TermRef::Iri(iri) => materialized.term_id_by_iri(iri),
                    _ => {
                        let value = composite.term_value(composite.source_id(index, term));
                        materialized.as_ref().term_id_by_value(&value)
                    }
                };
                mapped.ok_or_else(|| {
                    failure(
                        "SOURCE_BINDING_FAILED",
                        format!(
                            "source term in {:?} has no materialized binding",
                            receipt.path
                        ),
                    )
                })
            })
            .map_err(|error| {
                // Every recorded term was parsed from the selected documents, so a
                // missing binding is a broken invariant, never malformed input.
                if let Some(budget) = diag_canonicalization_limit(&error).copied() {
                    return LoadFailure::Execution(canonicalization_limit_failure(
                        &budget,
                        "source document binding",
                        error,
                        &identities,
                    ));
                }
                LoadFailure::Execution(with_inputs(
                    caused_diagnostic(
                        "SOURCE_BINDING_FAILED",
                        error,
                        "bind selected documents to the composed source",
                        serde_json::Value::Null,
                    ),
                    &identities,
                ))
            })?;
    }
    let theory = prepared.into_compiled(None).map_err(|error| {
        limit_or_malformed(error, "source compilation", &identities, |error| {
            with_inputs(
                caused_diagnostic(
                    "SOURCE_COMPILATION_FAILED",
                    error,
                    "compile selected theory",
                    serde_json::Value::Null,
                ),
                &identities,
            )
        })
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

fn publish_failure(
    reporter: &dyn Reporter,
    report: ProveReport,
    request: &ProveRequest<'_>,
) -> i32 {
    if let Some(diagnostic) = report.common.diagnostics.last() {
        emit_error(reporter, &diagnostic.code, diagnostic.detail.clone());
    }
    publish_report(report, request, reporter)
}

fn publish_report(
    mut report: ProveReport,
    request: &ProveRequest<'_>,
    reporter: &dyn Reporter,
) -> i32 {
    let mut json = serde_json::to_string_pretty(&report)
        .expect("typed prove report serialization is infallible");
    if let Some(path) = request.evidence_out
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
    match request.format {
        OutputFormat::Json => println!("{json}"),
        OutputFormat::Text => render_text(&report),
    }
    report.common.exit_code(request.gate)
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
    println!("semantics {}", report.semantics);
    println!("sources {}", report.common.inputs.len());
    if let Some(admission) = &report.admission {
        println!("source-admission {:?}", admission.status);
    }
    if let Some(selection) = &report.selection {
        println!("source-digest {}", selection.source_digest);
        println!("program-digest {}", selection.program_digest);
        println!("selection-digest {}", selection.selection_digest);
        println!("semantic-units {}", selection.units);
        println!(
            "evidence-left-out-units {}",
            selection.units_with_evidence_left_out
        );
        if !selection.evidence_left_out.is_empty() {
            println!(
                "evidence-left-out {}",
                selection.evidence_left_out.join(" ")
            );
        }
        // `complete` is the one computed value; these lines only spell it. It covers
        // the native accounting; the OWL leg's state is printed under that leg.
        if selection.complete {
            println!("selection complete zero-drops (native accounting)");
        } else {
            println!(
                "selection incomplete drops={} (native accounting)",
                selection.drops.len()
            );
        }
        for drop in &selection.drops {
            println!("drop {} {} {}", drop.engine, drop.code, drop.detail);
        }
    }
    if let Some(engines) = &report.engines {
        println!("native-verdict {}", engines.native.verdict);
        if let Some(state) = &engines.native.information_state {
            println!("native-information-state {state}");
        }
        if let Some(steps) = engines.native.consumed_steps {
            println!("native-consumed-steps {steps}");
        }
        if let Some(allowance) = engines.native.allowance {
            println!("native-allowance {allowance}");
        }
        println!("owl-dl-state {}", engines.owl_dl.state);
        println!(
            "owl-dl-answer {}",
            engines.owl_dl.answer.as_deref().unwrap_or("none")
        );
        if let Some(completeness) = &engines.owl_dl.completeness {
            println!("owl-dl-completeness {completeness}");
        }
        if let Some(digest) = &engines.owl_dl.proof_digest {
            println!("owl-dl-proof-digest {digest}");
        }
        if let Some(claim) = &engines.owl_dl.proof_claim {
            println!("owl-dl-proof-claim {claim}");
        }
        for note in &engines.owl_dl.notes {
            println!("owl-dl-note {} {}", note.code, note.detail);
        }
        println!("owl-dl-boundaries {}", engines.owl_dl.boundaries.len());
        for boundary in &engines.owl_dl.boundaries {
            println!("owl-dl-boundary {} {}", boundary.code, boundary.detail);
        }
    }
    for refutation in &report.refutations {
        println!(
            "refutation {} {} premises={}",
            refutation.engine,
            refutation.law,
            refutation.premises.len()
        );
    }
    if let Some(export) = &report.export {
        println!("tptp-export {} written={}", export.path, export.written);
        if let Some(digest) = &export.problem_digest {
            println!("tptp-problem-digest {digest}");
        }
        for blocker in &export.blockers {
            println!("tptp-blocker {} {}", blocker.code, blocker.reason);
        }
    }
    for diagnostic in &report.common.diagnostics {
        println!("diagnostic {} {}", diagnostic.code, diagnostic.detail);
    }
}

#[cfg(test)]
#[path = "prove.tests.rs"]
mod tests;
