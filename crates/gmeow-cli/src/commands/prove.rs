// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Installed `gmeow prove`: one native source composition, one source inventory,
//! and two in-process engines over that exact selection.
//!
//! The native rule engine reads the whole `logic:` selection and names what it
//! could not carry. The OWL 2 DL tableau reads the selection's OWL view and returns
//! a replayable proof. A clash from either engine refutes the whole selection.
//! Only the native engine may conclude `consistent`, and only from a completed
//! closure with executor-issued source completion and an empty drop ledger. Every other run is
//! `not-decided` with each drop named. Nothing here spawns a process, opens a
//! socket or reads a clock, so two runs over the same inputs print the same bytes.

mod files;
use files::{FilePlan, OpenedInput, Sink};

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use gmeow_cli_core::Reporter;
use gmeow_errors::Diag;
use gmeow_logic::reason::dl::DlCoverage;
use gmeow_logic::reason::{
    DomainProfile, InconsistencyWitness, LogicalGraph, NativeSourceAdmission,
    NativeSourceDisposition, NativeSourceDrop as DropRecord, SelectedDomains, SelectedLogicalWorld,
    UnsatClass,
};
use gmeow_logic::reasoner_services::DlReasoner;
use gmeow_logic_compile::frontend::{
    PreparedLogicSource, SourceAdmission, SourceBase, SourceBaseOrigin, SourceDocument,
    SourceSelection,
};
use gmeow_logic_compile::ir::TheorySelection;
use gmeow_logic_compile::source_inventory::{
    ResolvedTheorySelection, SourceInventory, SourceObligationOrigin, TheorySelectionObservation,
};
use gmeow_logic_compile::tptp::{FofProjectionBlocker, FofProjectionStatus, project_tptp_fof};
use purrdf::entail::Question;
use purrdf::{CompositeDatasetView, DatasetView, RdfDataset, TermId, TermRef, ViewLimits};
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

const ENGINE_NATIVE: &str = "native";
const ENGINE_OWL_DL: &str = "owl-dl";

/// Exit status of a replayed proof check.
const EXIT_PROOF_REJECTED: i32 = 1;

/// One complete `gmeow prove` invocation as selected on the command line.
#[derive(Clone, Copy)]
pub(crate) struct ProveRequest<'a> {
    pub(crate) inputs: &'a [PathBuf],
    pub(crate) dir: Option<&'a Path>,
    pub(crate) modules: &'a [String],
    pub(crate) max_steps: Option<u64>,
    pub(crate) step_cap: Option<u64>,
    pub(crate) format: OutputFormat,
    pub(crate) gate: bool,
    pub(crate) evidence_out: Option<&'a Path>,
    pub(crate) proof_out: Option<&'a Path>,
    pub(crate) check_proof: Option<&'a Path>,
    pub(crate) out: Option<&'a Path>,
}

/// The native source-obligation inventory and everything the engine did not decide.
#[derive(Debug, Clone, Serialize)]
struct SelectionReport {
    /// Observation of a completed native run with executor-validated full source
    /// accounting and an empty drop ledger. The OWL leg reports its own status.
    /// Output publication failures preserve the native selection already observed;
    /// an engine execution failure marks this selection incomplete and retains all
    /// boundaries and completed native evidence established before it.
    complete: bool,
    source_digest: String,
    program_digest: String,
    selection_digest: String,
    theory_selection: TheorySelectionObservation,
    documents: usize,
    units: usize,
    unit_basis: &'static str,
    /// How many scoped claims the native engine read as facts. Every other scoped
    /// claim is a `SCOPED_CLAIM_NOT_READ` drop.
    claims_read: usize,
    /// Each claim read as a fact, in program order, with its source. The statement is
    /// in the compiler's spelling, the one a `SCOPED_CLAIM_NOT_READ` drop uses; a
    /// refutation premise renders its terms differently, so the two are matched by
    /// reading, not by string equality.
    claims: Vec<ReadClaimReport>,
    /// Admission and native-accounting drops, sorted and de-duplicated. The OWL
    /// leg's own state is reported under that leg, never here.
    drops: Vec<DropRecord>,
}

/// One scoped claim the native engine read as a fact of the selected world, with the
/// evidence its carrier states.
#[derive(Debug, Clone, Serialize)]
struct ReadClaimReport {
    /// `program:axiom:N`, the claim's place in the compiled program.
    anchor: String,
    /// Original source carriers that independently admitted this canonical claim.
    carriers: Vec<String>,
    subject: String,
    predicate: String,
    object: String,
    /// The `logic:provenance` of the report, when stated.
    provenance: Option<String>,
    /// The `logic:confidence` of the report, when stated. It is metadata, never a
    /// truth degree.
    confidence: Option<f64>,
}

impl SelectionReport {
    /// The selection block of a run: the admission's identities, the claims the
    /// native engine read and the drops named so far.
    fn new(
        inventory: &SourceInventory<'_>,
        selection: &ResolvedTheorySelection,
        units: usize,
        documents: usize,
        complete: bool,
        claims: Vec<ReadClaimReport>,
        drops: Vec<DropRecord>,
    ) -> Self {
        Self {
            complete,
            source_digest: inventory.source_digest().to_owned(),
            program_digest: inventory.program_digest().to_owned(),
            selection_digest: inventory.selection_digest().to_owned(),
            theory_selection: selection.observation().clone(),
            documents,
            units,
            unit_basis: "compiler source obligations and exact lowering routes",
            claims_read: claims.len(),
            claims,
            drops,
        }
    }
}

/// A failed engine, retaining any native leg and ingress completed before it.
struct EngineFailure {
    native: Option<NativeLeg>,
    diagnostic: Diag,
    claims_read: Vec<ReadClaimReport>,
}

/// The native rule engine's own account of the selection.
#[derive(Debug, Clone, Serialize)]
struct NativeLeg {
    #[serde(skip)]
    source_complete: bool,
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
    /// Scoped claims read as facts; reported under `selection.claims`.
    #[serde(skip)]
    claims_read: Vec<ReadClaimReport>,
}

impl NativeLeg {
    /// The leg of an engine that declined the program: it executed nothing.
    fn refused(claims_read: Vec<ReadClaimReport>) -> Self {
        Self {
            source_complete: false,
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
            claims_read,
        }
    }

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
    /// deciding; `completeness` says why), `view-not-built`, or `execution-failure`.
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
    fn failed(diagnostic: &Diag) -> Self {
        let diagnostic = ReportDiagnostic::from_diag(diagnostic);
        let mut leg = Self::not_built(&diagnostic.code, diagnostic.detail);
        leg.state = "execution-failure";
        leg
    }

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
const PROVE_SEMANTICS: &str = "GMEOW native logic: semantics of one selected default-graph theory, containing supplied top-level obligations and explicitly requested modules with their actual transitive imports. The native engine decides under source accounting. Ground reified claims use separate closed evidence-only and source-resolved module-assertion routes. Envelope checks read every original graph; additional qualifications, mentions and graph-name uses keep a claim unread. Excluded modules contribute no premise; other unread scoped claims remain named boundaries. Bare quotations assert no payload. The OWL 2 DL tableau is an independent refuter over the faithful asserted-only filtered raw view; native admitted claim facts are absent from that view. Authored reasoning-contract configuration is catalogued, not executed. Native verdicts are attestations: a consistent verdict is reproduced on the same inputs and an inconsistent verdict carries its law and witness premises. Top-level tableau proofs bind their exact reader dataset; composed proof export/replay and FOF output refuse. Classical FOF export has independent admission and receives no verdict from native completion.";

#[derive(Serialize)]
struct ProveReport<'inventory> {
    #[serde(flatten)]
    common: ReasoningReportCore,
    semantics: &'static str,
    /// Every retained clash, including native evidence preceding a later engine or output failure.
    refutations: Vec<Refutation>,
    admission: Option<SourceAdmission>,
    native_admission: Option<NativeSourceAdmission<'inventory>>,
    selection: Option<SelectionReport>,
    engines: Option<EngineReports>,
    export: Option<TptpExport>,
}

impl<'inventory> ProveReport<'inventory> {
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
            native_admission: None,
            selection: None,
            engines: None,
            export: None,
        }
    }

    /// The report of an engine that failed to execute. The selection block keeps
    /// what was established before the failure, marked incomplete, so the drops
    /// already named are not lost with the verdict.
    fn engine_failure(
        failure: EngineFailure,
        inputs: &[InputIdentity],
        admission: Option<SourceAdmission>,
        native_admission: NativeSourceAdmission<'inventory>,
        inventory: &SourceInventory<'_>,
        documents: usize,
        drops: BTreeSet<DropRecord>,
    ) -> Self {
        let EngineFailure {
            diagnostic,
            claims_read,
            native,
        } = failure;
        let completed = native.map(|native| (native, OwlDlLeg::failed(&diagnostic)));
        let mut report = Self::blank(
            DecisionClass::ExecutionFailure,
            with_inputs(diagnostic, inputs),
        );
        report.common.program = ProgramIdentity {
            profile: PROVE_PROFILE.to_owned(),
            content_digest: inventory.program_digest().to_owned(),
        };
        report.selection = Some(SelectionReport::new(
            inventory,
            native_admission.resolved_selection(),
            native_admission.units(),
            documents,
            false,
            claims_read,
            drops.into_iter().collect(),
        ));
        if let Some((native, owl_dl)) = completed {
            report.refutations = native.refutations.clone();
            report.common.metrics = ReasoningMetrics {
                decisions: None,
                steps: native.consumed_steps,
                budget: native.allowance,
            };
            report.engines = Some(EngineReports { native, owl_dl });
        }
        report.admission = admission;
        report.native_admission = Some(native_admission);
        report
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

/// Why a selected source or output boundary cannot be admitted.
enum LoadFailure {
    /// The selected documents are not a well-formed source.
    Malformed(Diag),
    /// I/O or a well-formed operation cannot complete.
    Execution(Diag),
}

impl LoadFailure {
    fn into_report(self) -> ProveReport<'static> {
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

fn resolve_requested_theory(
    inventory: &SourceInventory<'_>,
    modules: &[String],
) -> gmeow_errors::Result<ResolvedTheorySelection> {
    let request = if modules.is_empty() {
        TheorySelection::top_level_only()
    } else {
        TheorySelection::composed_modules(modules.iter().cloned())?
    };
    inventory.resolve_theory_selection(&request)
}

fn remove_stale_required_output(
    files: &FilePlan,
    sink: Sink,
    path: &Path,
) -> gmeow_errors::Result<()> {
    match files.remove(sink) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(caused_failure(
            "COMPOSED_OUTPUT_STALE_REMOVE_FAILED",
            error,
            format!("remove refused composed output {}", path.display()),
        )),
    }
}

fn refuse_composed_outputs(
    reporter: &dyn Reporter,
    request: &ProveRequest<'_>,
    inventory: &SourceInventory<'_>,
    admission: NativeSourceAdmission<'_>,
    inputs: &[InputIdentity],
    files: &FilePlan,
) -> i32 {
    let selection = admission.resolved_selection();
    let mut report = ProveReport::blank(
        DecisionClass::ExecutionFailure,
        with_inputs(
            failure(
                "COMPOSED_REQUIRED_OUTPUT_UNSUPPORTED",
                "the resolved composed theory has no selected FOF export or proof-file contract",
            ),
            inputs,
        ),
    );
    report.selection = Some(SelectionReport::new(
        inventory,
        selection,
        admission.units(),
        inventory.theory().source().origins().len(),
        false,
        Vec::new(),
        admission.drops().collect(),
    ));
    if let Some(path) = request.out {
        report.export = Some(TptpExport {
            path: path.display().to_string(),
            written: false,
            status: FofProjectionStatus::Blocked,
            profile_id: "gmeow-composed-theory-fof-unimplemented-v1".into(),
            problem_digest: None,
            sentences: 0,
            blockers: vec![FofProjectionBlocker {
                unit_id: None,
                code: "COMPOSED_FOF_EXPORT_UNSUPPORTED".into(),
                reason: "native module admission does not authorize a classical FOF composition"
                    .into(),
            }],
        });
        if let Err(diagnostic) = remove_stale_required_output(files, Sink::Fof, path) {
            report.execution_failure(diagnostic);
        }
    }
    if let Some(path) = request.proof_out {
        report.execution_failure(failure(
            "COMPOSED_PROOF_OUTPUT_UNSUPPORTED",
            "composed theory proof files require a composition-bound proof contract",
        ));
        if let Err(diagnostic) = remove_stale_required_output(files, Sink::Proof, path) {
            report.execution_failure(diagnostic);
        }
    }
    report.native_admission = Some(admission);
    publish_failure(reporter, report, request, files)
}

/// Run the complete installed `gmeow prove` operation.
pub(crate) fn prove(reporter: &dyn Reporter, request: &ProveRequest<'_>) -> i32 {
    let files = match FilePlan::open(request) {
        Ok(files) => files,
        Err(failure) => {
            let safe = ProveRequest {
                evidence_out: None,
                ..*request
            };
            if let Some(proof) = request.check_proof {
                return check_admission_failure(reporter, &safe, proof, failure);
            }
            return publish_failure(reporter, failure.into_report(), &safe, &FilePlan::default());
        }
    };
    if let Some(proof) = request.check_proof {
        return check_proof(reporter, request, proof, &files);
    }
    prove_admitted(reporter, request, &files)
}

fn prove_admitted(reporter: &dyn Reporter, request: &ProveRequest<'_>, files: &FilePlan) -> i32 {
    let loaded = match load_theory(&files.sources) {
        Ok(loaded) => loaded,
        Err(failure) => return publish_failure(reporter, failure.into_report(), request, files),
    };

    let selection = SourceSelection::all_classical_roots(&loaded.theory);
    let inventory = SourceInventory::new(&loaded.theory, selection);
    let resolved = match resolve_requested_theory(&inventory, request.modules) {
        Ok(selection) => selection,
        Err(diagnostic) => {
            return publish_failure(
                reporter,
                ProveReport::blank(
                    DecisionClass::Malformed,
                    with_inputs(diagnostic, &loaded.identities),
                ),
                request,
                files,
            );
        }
    };
    let mut native_admission = match NativeSourceAdmission::inspect_selected(&inventory, &resolved)
    {
        Ok(admission) => admission,
        Err(diagnostic) => {
            return publish_failure(
                reporter,
                ProveReport::blank(
                    DecisionClass::ExecutionFailure,
                    with_inputs(diagnostic, &loaded.identities),
                ),
                request,
                files,
            );
        }
    };
    if resolved.request().is_composed() && (request.out.is_some() || request.proof_out.is_some()) {
        return refuse_composed_outputs(
            reporter,
            request,
            &inventory,
            native_admission,
            &loaded.identities,
            files,
        );
    }
    if native_admission.malformed() {
        let detail = native_admission
            .drops()
            .map(|drop| format!("{}: {}", drop.code, drop.detail))
            .collect::<Vec<_>>()
            .join("; ");
        let mut report = ProveReport::blank(
            DecisionClass::Malformed,
            with_inputs(
                failure("SOURCE_ADMISSION_MALFORMED", detail),
                &loaded.identities,
            ),
        );
        report.native_admission = Some(native_admission);
        return publish_failure(reporter, report, request, files);
    }
    // Classical export is a separate selected operation. Native completion never
    // authorizes FOF, and native-only runs never construct its owned admission.
    let admission = request
        .out
        .map(|_| SourceAdmission::classical_fof_from_inventory(&inventory));
    let canonical = match resolved.source_view(&inventory) {
        Ok(view) => view,
        Err(diagnostic) => {
            return publish_failure(
                reporter,
                ProveReport::blank(
                    DecisionClass::ExecutionFailure,
                    with_inputs(diagnostic, &loaded.identities),
                ),
                request,
                files,
            );
        }
    };
    let mut drops = BTreeSet::new();
    let documents = loaded.theory.source().origins().len();
    let legs = native_leg(
        &inventory,
        &resolved,
        &mut native_admission,
        request.max_steps,
        &mut drops,
    )
    .and_then(|native| match owl_dl_leg(&canonical, request.step_cap) {
        Ok(owl) => Ok((native, owl)),
        Err(diagnostic) => Err(EngineFailure {
            diagnostic,
            claims_read: native.claims_read.clone(),
            native: Some(native),
        }),
    });
    let (native, (owl_dl, proof)) = match legs {
        Ok(legs) => legs,
        Err(failure) => {
            let report = ProveReport::engine_failure(
                failure,
                &loaded.identities,
                admission,
                native_admission,
                &inventory,
                documents,
                drops,
            );
            return publish_failure(reporter, report, request, files);
        }
    };

    // The one computation that decides `complete`; the JSON field, the text line,
    // the preservation axis and the verdict all read it. It covers the native
    // accounting only: the OWL leg's own state is reported under that leg.
    let drops: Vec<DropRecord> = drops.into_iter().collect();
    let complete = native.source_complete && drops.is_empty();
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
            EvidenceGrade::Attestation,
            GateAdmission::Attestation,
        ),
        VERDICT_INCONSISTENT => (
            DecisionClass::Negative,
            "opposed",
            if complete {
                "complete-for-fragment"
            } else {
                "incomplete"
            },
            EvidenceGrade::Attestation,
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
                content_digest: inventory.program_digest().to_owned(),
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
        selection: Some(SelectionReport::new(
            &inventory,
            &resolved,
            native_admission.units(),
            documents,
            complete,
            native.claims_read.clone(),
            drops,
        )),
        semantics: PROVE_SEMANTICS,
        refutations,
        engines: Some(EngineReports { native, owl_dl }),
        export: None,
        admission: None,
        native_admission: Some(native_admission),
    };

    if let Some(path) = request.out {
        match export_tptp(
            &loaded.theory,
            admission
                .as_ref()
                .expect("selected export has its independent classical admission"),
            path,
            files,
        ) {
            Ok(export) => {
                let written = export.written;
                report.export = Some(export);
                if !written {
                    report.admission = admission;
                    report.execution_failure(failure("SELECTED_EXPORT_BLOCKED", "--out selected a classical FOF output that its source admission cannot produce; inspect export.blockers and admission"));
                    return publish_failure(reporter, report, request, files);
                }
            }
            Err(diagnostic) => {
                report.admission = admission;
                report.execution_failure(diagnostic);
                return publish_failure(reporter, report, request, files);
            }
        }
    }
    report.admission = admission;
    if let Some(path) = request.proof_out {
        let written = match &proof {
            Some(bytes) => files.write(Sink::Proof, bytes).map_err(|error| {
                caused_failure(
                    "PROOF_WRITE_FAILED",
                    error,
                    format!("write proof {}", path.display()),
                )
            }),
            None => {
                report.execution_failure(failure(
                    "PROOF_NOT_RECORDED",
                    "--proof-out was selected but the OWL 2 DL leg recorded no proof for this selection",
                ));
                if let Err(error) = files.remove(Sink::Proof) {
                    report.execution_failure(caused_failure(
                        "PROOF_STALE_REMOVE_FAILED",
                        error,
                        format!("remove stale proof {}", path.display()),
                    ));
                }
                return publish_failure(reporter, report, request, files);
            }
        };
        if let Err(diagnostic) = written {
            report.execution_failure(diagnostic);
            return publish_failure(reporter, report, request, files);
        }
    }
    publish_report(report, request, reporter, files)
}

/// The drop code naming an analysis bound that coarsened or stopped part of the termination
/// analysis, so a finer analysis might have decided. It does not say the bound caused a
/// refusal. It sits beside the refusal reasons (`NATIVE_REFUSAL_REASON`) or, on a budgeted
/// run, beside `CHASE_UNCERTIFIED`, where nothing may have been refused. With a bound present
/// those reasons are the cycles the coarser analysis found, which a finer one might have ruled
/// out, so neither the bound nor the reasons alone say whether the program cycles.
const ANALYSIS_BOUND: &str = "NATIVE_ANALYSIS_BOUND";

/// Name the authored formula behind a compiler-produced rule identity, wherever `text`
/// writes the identity as `<iri>`. A rule with no named authored formula is left as is.
fn name_authored_rules(
    text: &str,
    authored: &std::collections::BTreeMap<String, Vec<String>>,
) -> String {
    if authored.is_empty() {
        return text.to_owned();
    }
    // One pass over the `<...>` terms. Every reason that reaches here names a rule by its
    // merged identity, so each term is looked up whole.
    let mut named = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(open) = rest.find('<') {
        let Some(length) = rest[open..].find('>') else {
            break;
        };
        let close = open + length;
        named.push_str(&rest[..=close]);
        let rule = &rest[open + 1..close];
        if let Some(formulas) = authored.get(rule) {
            let formulas = formulas
                .iter()
                .map(|formula| format!("<{formula}>"))
                .collect::<Vec<_>>()
                .join(", ");
            named.push_str(&format!(" (authored formula {formulas})"));
        }
        rest = &rest[close + 1..];
    }
    named.push_str(rest);
    named
}

/// Every construct the lowering could not carry is a drop, unless a later check already
/// names it. A completed run and a refused run read the same preparation through this.
fn insert_unsupported_constructs(
    drops: &mut BTreeSet<DropRecord>,
    constructs: &BTreeSet<String>,
    already_named: impl Fn(&String) -> bool,
) {
    for construct in constructs {
        if !already_named(construct) {
            drops.insert(DropRecord::new(
                ENGINE_NATIVE,
                "UNSUPPORTED_CONSTRUCT",
                construct.clone(),
            ));
        }
    }
}

/// The lowering residue of a refused run, from the preparation the run used.
fn insert_lowering_residue(
    drops: &mut BTreeSet<DropRecord>,
    selected: Option<&gmeow_logic::reason::SelectedProgram>,
) {
    if let Some(selected) = selected {
        insert_unsupported_constructs(drops, selected.lowering_residue(), |_| false);
    }
}

/// Run the native rule engine over the whole selection and add every construct it
/// did not decide to `drops`. It ingests compiler-filtered raw assertions and
/// admitted claims through the closed evidence-only and typed module routes.
/// Claim envelopes inspect every original graph; other context coordinates,
/// qualifiers and mentions retain their boundaries. A bare quotation is never read.
/// An engine that declines the program for a reason it names did not fail: the leg
/// is `refused` and the reasons are drops, so the selection is not decided. A rule
/// head the chase refuses is declined the same way, as an `UNSUPPORTED_CONSTRUCT`
/// drop that names the rule and its head relations. Any
/// other engine error is an execution failure, never a drop: a canonicalization
/// search that reached its limit is `SOURCE_CANONICALIZATION_LIMIT`, and every other
/// error is `NATIVE_EXECUTION_FAILED`. The failure carries the claims read before it,
/// and `drops` keeps what was named before it.
fn native_leg(
    inventory: &SourceInventory<'_>,
    selection: &ResolvedTheorySelection,
    admission: &mut NativeSourceAdmission<'_>,
    max_steps: Option<u64>,
    drops: &mut BTreeSet<DropRecord>,
) -> Result<NativeLeg, EngineFailure> {
    let theory = inventory.theory();
    // Every scoped claim of the compiled theory is read into the selected world or
    // named here before the engine starts; the engine then ignores axiom scopes.
    let prepared = gmeow_logic::reason::prepare_selected_theory_input(
        inventory,
        &LogicalGraph::Default,
        selection,
    );
    let mut claims_read = Vec::new();
    if let Ok((_, ingress)) = &prepared {
        // The ledger classifies every scoped axiom of this same compiled program, and
        // the engine entry refuses a ledger that does not cover the program it runs,
        // so no separate coverage check is repeated here.
        claims_read = ingress
            .read()
            .iter()
            .map(|claim| {
                let axiom = &theory.program().axioms[claim.axiom];
                ReadClaimReport {
                    anchor: format!("program:axiom:{}", claim.axiom),
                    carriers: claim
                        .carriers
                        .iter()
                        .map(|carrier| carrier.name.clone())
                        .collect(),
                    subject: axiom.subject.clone(),
                    predicate: axiom.predicate.clone(),
                    object: axiom.obj.key(),
                    provenance: axiom.scope.provenance.clone(),
                    confidence: axiom.scope.confidence,
                }
            })
            .collect();
        for claim in ingress.unread() {
            let axiom = &theory.program().axioms[claim.axiom];
            drops.insert(DropRecord::new(
                ENGINE_NATIVE,
                "SCOPED_CLAIM_NOT_READ",
                format!(
                    "{}: {} {} {}: {}",
                    claim.source_detail(),
                    axiom.subject,
                    axiom.predicate,
                    axiom.obj.key(),
                    claim.reason
                ),
            ));
        }
    }
    // The lowering is prepared once and kept: a run that is refused reports the same
    // lowering residue a completed run does, from the same preparation.
    let mut selected: Option<gmeow_logic::reason::SelectedProgram> = None;
    let run = (|| {
        let (input, ingress) = prepared?;
        let program = gmeow_logic::reason::prepare_selected_theory(inventory, &ingress)?;
        selected = Some(program.clone());
        let domains = SelectedDomains::new([SelectedLogicalWorld::new(
            LogicalGraph::Default,
            DomainProfile::NonemptyObjectDomainV1,
            PROVE_WORLD_AUTHORITY.to_owned(),
            *input.ingress_contract(),
        )?])?;
        let result = gmeow_logic::reason::reason_admitted_theory_within(
            admission,
            &program,
            input,
            &domains,
            &ingress,
            &gmeow_logic::query_ir::Budget {
                max_answers: None,
                max_steps,
            },
        )?;
        let verdict = result.native_verdict()?;
        Ok::<_, Diag>((result, verdict))
    })();
    drops.extend(admission.drops());
    let (result, verdict) = match run {
        Ok(outcome) => outcome,
        Err(error) => {
            if let Some(budget) = diag_canonicalization_limit(&error).copied() {
                return Err(EngineFailure {
                    native: None,
                    diagnostic: canonicalization_limit_failure(
                        &budget,
                        "native closure",
                        error,
                        &[],
                    ),
                    claims_read,
                });
            }
            let authored = match selected
                .as_ref()
                .map(|program| program.authored_rule_formulas(theory))
                .transpose()
            {
                Ok(names) => names.unwrap_or_default(),
                Err(diagnostic) => {
                    return Err(EngineFailure {
                        native: None,
                        diagnostic,
                        claims_read,
                    });
                }
            };
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
                        name_authored_rules(gap, &authored),
                    ));
                }
                for bound in &refusal.bounds {
                    drops.insert(DropRecord::new(
                        ENGINE_NATIVE,
                        ANALYSIS_BOUND,
                        bound.clone(),
                    ));
                }
                insert_lowering_residue(drops, selected.as_ref());
                return Ok(NativeLeg::refused(claims_read));
            }
            // A rule head the chase cannot carry is a construct the engine does not
            // support, wherever the rule was authored. The chase's own check decides
            // it; the whole program is then unexecuted, so the selection cannot be
            // read as decided without this rule.
            if let Some(refused) = error.downcast_ref::<gmeow_logic::error::RefusedRuleHead>() {
                drops.insert(DropRecord::new(
                    ENGINE_NATIVE,
                    "UNSUPPORTED_CONSTRUCT",
                    name_authored_rules(&refused.to_string(), &authored),
                ));
                insert_lowering_residue(drops, selected.as_ref());
                return Ok(NativeLeg::refused(claims_read));
            }
            return Err(EngineFailure {
                native: None,
                diagnostic: caused_diagnostic(
                    "NATIVE_EXECUTION_FAILED",
                    error,
                    "run the native engine over the selection",
                    serde_json::Value::Null,
                ),
                claims_read,
            });
        }
    };
    let source_complete = admission
        .complete_for(&result)
        .map_err(|diagnostic| EngineFailure {
            native: None,
            diagnostic,
            claims_read: claims_read.clone(),
        })?;
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
        source_complete,
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
        claims_read,
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
    canonical: &impl DatasetView<Id = TermId, ReadError = std::convert::Infallible>,
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
/// of this leg. The OWL view holds the asserted statements only; it does not see the
/// scoped claims the native leg reads. This leg is a second refuter, so deciding
/// over fewer premises cannot make it report a clash the selection does not have.
fn owl_dl_leg(
    canonical: &impl DatasetView<Id = TermId, ReadError = std::convert::Infallible>,
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
/// remains available in the retained native execution evidence.
fn export_tptp(
    theory: &gmeow_logic_compile::frontend::CompiledTheory,
    admission: &SourceAdmission,
    path: &Path,
    files: &FilePlan,
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
        match files.remove(Sink::Fof) {
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
    let mut file = files.writer(Sink::Fof).map_err(|error| {
        caused_failure(
            "PROBLEM_WRITE_FAILED",
            error,
            format!("open TPTP export {}", path.display()),
        )
    })?;
    std::io::Write::write_all(&mut file, problem.as_bytes())
        .and_then(|()| std::io::Write::flush(&mut file))
        .map_err(|error| {
            caused_failure(
                "PROBLEM_WRITE_FAILED",
                error,
                format!("write TPTP export {}", path.display()),
            )
        })?;
    let read_back = (|| -> std::io::Result<Vec<u8>> {
        std::io::Seek::rewind(&mut file)?;
        let mut bytes = Vec::new();
        std::io::Read::read_to_end(&mut file, &mut bytes)?;
        Ok(bytes)
    })()
    .map_err(|error| {
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
    theory_selection: Option<TheorySelectionObservation>,
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

/// Report a filesystem admission failure without opening any output sink.
fn check_admission_failure(
    reporter: &dyn Reporter,
    request: &ProveRequest<'_>,
    proof_path: &Path,
    error: LoadFailure,
) -> i32 {
    let (verdict, diagnostic) = match error {
        LoadFailure::Malformed(diagnostic) => ("malformed", diagnostic),
        LoadFailure::Execution(diagnostic) => ("execution-failure", diagnostic),
    };
    let report = ProofCheckReport {
        schema_version: 1,
        operation: "proof-check",
        verdict,
        inputs: recorded_inputs(&diagnostic),
        theory_selection: None,
        proof_path: proof_path.display().to_string(),
        proof_digest: None,
        claims: Vec::new(),
        runs: None,
        replayed_runs: None,
        diagnostics: Vec::new(),
    };
    publish_check(
        reporter,
        report,
        verdict,
        &diagnostic,
        request,
        &FilePlan::default(),
    )
}

/// Re-prepare the selection and replay a recorded tableau proof against its OWL
/// view. The verifier binds the proof to the view and consistency question and
/// checks every claim against the recorded runs, without making a solver decision.
fn check_proof(
    reporter: &dyn Reporter,
    request: &ProveRequest<'_>,
    proof_path: &Path,
    files: &FilePlan,
) -> i32 {
    let mut report = ProofCheckReport {
        schema_version: 1,
        operation: "proof-check",
        verdict: "malformed",
        inputs: Vec::new(),
        theory_selection: None,
        proof_path: proof_path.display().to_string(),
        proof_digest: None,
        claims: Vec::new(),
        runs: None,
        replayed_runs: None,
        diagnostics: Vec::new(),
    };
    let loaded = load_theory(&files.sources);
    let loaded = match loaded {
        Ok(loaded) => loaded,
        Err(LoadFailure::Malformed(diagnostic)) => {
            report.inputs = recorded_inputs(&diagnostic);
            return publish_check(reporter, report, "malformed", &diagnostic, request, files);
        }
        Err(LoadFailure::Execution(diagnostic)) => {
            report.inputs = recorded_inputs(&diagnostic);
            return publish_check(
                reporter,
                report,
                "execution-failure",
                &diagnostic,
                request,
                files,
            );
        }
    };
    report.inputs.clone_from(&loaded.identities);
    let inventory = SourceInventory::new(
        &loaded.theory,
        SourceSelection::all_classical_roots(&loaded.theory),
    );
    let resolved = match resolve_requested_theory(&inventory, request.modules) {
        Ok(resolved) => resolved,
        Err(diagnostic) => {
            return publish_check(reporter, report, "malformed", &diagnostic, request, files);
        }
    };
    report.theory_selection = Some(resolved.observation().clone());
    if resolved.request().is_composed() {
        let diagnostic = failure(
            "COMPOSED_PROOF_REPLAY_UNSUPPORTED",
            "a raw tableau proof cannot authorize replay of a resolved composed theory",
        );
        return publish_check(reporter, report, "rejected", &diagnostic, request, files);
    }
    let input = match files.replay_input() {
        Ok(input) => input,
        Err(diagnostic) => {
            return publish_check(reporter, report, "malformed", diagnostic, request, files);
        }
    };
    let bytes = match input.read() {
        Ok(bytes) => bytes,
        Err(error) => {
            let diagnostic = caused_failure(
                "PROOF_READ_FAILED",
                error,
                format!("read proof {}", proof_path.display()),
            );
            return publish_check(reporter, report, "malformed", &diagnostic, request, files);
        }
    };
    report.inputs.push(InputIdentity::from_bytes(
        proof_path,
        "selected-proof-replay",
        "application/cbor",
        None,
        &bytes,
    ));
    let proof = match purrdf::entail::ServiceProof::decode(&bytes) {
        Ok(proof) => proof,
        Err(error) => {
            let diagnostic = caused_failure(
                "PROOF_DECODE_REJECTED",
                error,
                format!("decode proof {}", proof_path.display()),
            );
            return publish_check(reporter, report, "rejected", &diagnostic, request, files);
        }
    };
    report.proof_digest = Some(proof.digest_hex());
    report.runs = Some(proof.runs().len());
    report.claims = proof_claims(&proof);
    let canonical = match resolved.source_view(&inventory) {
        Ok(view) => view,
        Err(diagnostic) => {
            return publish_check(
                reporter,
                report,
                "execution-failure",
                &diagnostic,
                request,
                files,
            );
        }
    };
    let (view, reasoner) = match open_owl_view(&canonical, request.step_cap) {
        Ok(opened) => opened,
        Err(OwlOpenFailure::Limit(diagnostic)) => {
            return publish_check(
                reporter,
                report,
                "execution-failure",
                &diagnostic,
                request,
                files,
            );
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
            return publish_check(
                reporter,
                report,
                "execution-failure",
                &diagnostic,
                request,
                files,
            );
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
            return publish_check(
                reporter,
                report,
                "execution-failure",
                &diagnostic,
                request,
                files,
            );
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
            publish_check_report(report, request, files)
        }
        Err(error) => {
            if let Some(budget) = canonicalization_limit(&error).copied() {
                let diagnostic = canonicalization_limit_failure(
                    &budget,
                    "proof source binding",
                    failure("PROOF_SOURCE_BINDING", error.to_string()),
                    &loaded.identities,
                );
                return publish_check(
                    reporter,
                    report,
                    "execution-failure",
                    &diagnostic,
                    request,
                    files,
                );
            }
            let diagnostic = caused_failure(
                "PROOF_REJECTED",
                error,
                format!("replay proof {}", proof_path.display()),
            );
            publish_check(reporter, report, "rejected", &diagnostic, request, files)
        }
    }
}

fn publish_check(
    reporter: &dyn Reporter,
    mut report: ProofCheckReport,
    verdict: &'static str,
    diagnostic: &Diag,
    request: &ProveRequest<'_>,
    files: &FilePlan,
) -> i32 {
    let projected = ReportDiagnostic::from_diag(diagnostic);
    emit_error(reporter, &projected.code, projected.detail.clone());
    report.verdict = verdict;
    report.diagnostics.push(projected);
    publish_check_report(report, request, files)
}

fn publish_check_report(
    report: ProofCheckReport,
    request: &ProveRequest<'_>,
    files: &FilePlan,
) -> i32 {
    if let Some(path) = request.evidence_out
        && let Err(error) = write_evidence(files, &report)
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
            if let Some(selection) = &report.theory_selection {
                render_theory_selection(selection);
            }
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
fn load_theory(paths: &[OpenedInput]) -> Result<LoadedTheory, LoadFailure> {
    let mut receipts = Vec::with_capacity(paths.len());
    let mut identities = Vec::with_capacity(paths.len());
    let mut documents = Vec::with_capacity(paths.len());
    for input in paths {
        let path = &input.path;
        let bytes = input.read().map_err(|error| {
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
    report: ProveReport<'_>,
    request: &ProveRequest<'_>,
    files: &FilePlan,
) -> i32 {
    if let Some(diagnostic) = report.common.diagnostics.last() {
        emit_error(reporter, &diagnostic.code, diagnostic.detail.clone());
    }
    publish_report(report, request, reporter, files)
}

fn publish_report(
    mut report: ProveReport<'_>,
    request: &ProveRequest<'_>,
    reporter: &dyn Reporter,
    files: &FilePlan,
) -> i32 {
    if let Some(path) = request.evidence_out {
        let written = write_evidence(files, &report);
        if let Err(error) = written {
            let diagnostic = caused_failure(
                "EVIDENCE_WRITE_FAILED",
                error,
                format!("write evidence report {}", path.display()),
            );
            let detail = diagnostic.message().to_owned();
            report.execution_failure(diagnostic);
            emit_error(reporter, "gmeow-cli.prove.evidence-write-failed", detail);
        }
    }
    match request.format {
        OutputFormat::Json => {
            let written = (|| -> Result<(), std::io::Error> {
                let mut output = std::io::stdout().lock();
                serde_json::to_writer_pretty(&mut output, &report)
                    .map_err(std::io::Error::other)?;
                std::io::Write::write_all(&mut output, b"\n")
            })();
            if let Err(error) = written {
                emit_error(
                    reporter,
                    "gmeow-cli.prove.report-write-failed",
                    error.to_string(),
                );
                return 2;
            }
        }
        OutputFormat::Text => render_text(&report),
    }
    report.common.exit_code(request.gate)
}

/// Stream either evidence report through the same admitted filesystem boundary.
fn write_evidence(files: &FilePlan, report: &impl Serialize) -> std::io::Result<()> {
    let file = files.writer(Sink::Evidence)?;
    let mut writer = std::io::BufWriter::new(file);
    serde_json::to_writer_pretty(&mut writer, report).map_err(std::io::Error::other)?;
    std::io::Write::flush(&mut writer)
}

fn render_theory_selection(selection: &TheorySelectionObservation) {
    println!(
        "theory-composition {}",
        if selection.request.is_composed() {
            "composed-modules"
        } else {
            "top-level"
        }
    );
    println!("theory-selection-digest {}", selection.identity);
    println!(
        "theory-effective-masks {}",
        selection.effective_masks_digest
    );
    println!("theory-includes-top-level {}", selection.includes_top_level);
    for root in selection.request.module_roots() {
        println!("theory-root {}", root.as_str());
    }
    for module in &selection.modules {
        println!("theory-module {}", module.as_str());
    }
    for edge in &selection.imports {
        println!(
            "theory-import {} {} {}",
            edge.source.as_str(),
            edge.target.as_str(),
            edge.carrier
        );
    }
}

fn render_text(report: &ProveReport<'_>) {
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
    if let Some(admission) = &report.native_admission {
        let evidence_origins: BTreeSet<_> = admission
            .source_dispositions()
            .filter_map(|(_, disposition)| match disposition {
                NativeSourceDisposition::FormulaEvidence { formula_origins } => {
                    Some(formula_origins)
                }
                _ => None,
            })
            .flatten()
            .collect();
        for (unit, disposition) in admission.source_dispositions() {
            let anchor = unit.anchor();
            if !evidence_origins.is_empty()
                && matches!(unit.origin(), SourceObligationOrigin::Formula(_))
            {
                let id = unit.identity();
                if evidence_origins.contains(&id) {
                    println!("formula-origin {} {anchor}", id.as_str());
                }
            }
            let (status, code, detail) = match disposition {
                NativeSourceDisposition::Accounted { route } => ("Accounted", "-", *route),
                NativeSourceDisposition::StructuralSupport { route } => {
                    ("StructuralSupport", "-", *route)
                }
                NativeSourceDisposition::FormulaEvidence { formula_origins } => {
                    let evidence = unit.identity();
                    for formula in formula_origins {
                        println!(
                            "formula-evidence {} {} {anchor}",
                            evidence.as_str(),
                            formula.as_str()
                        );
                    }
                    (
                        "FormulaEvidence",
                        "-",
                        "source provenance of an accounted formula",
                    )
                }
                NativeSourceDisposition::ConfigurationDeclaration { route } => {
                    ("ConfigurationDeclaration", "-", *route)
                }
                NativeSourceDisposition::MetatheoryDeclaration { route } => {
                    ("MetatheoryDeclaration", "-", *route)
                }
                NativeSourceDisposition::OutsideSelection { reason } => {
                    ("OutsideSelection", "OUTSIDE_SELECTION", *reason)
                }
                NativeSourceDisposition::OutsideRequestedTheory { reason } => {
                    ("OutsideRequestedTheory", "-", *reason)
                }
                NativeSourceDisposition::Blocked { code, reason } => {
                    ("Blocked", *code, reason.as_str())
                }
                NativeSourceDisposition::Malformed { code, reason } => {
                    ("Malformed", *code, reason.as_str())
                }
            };
            println!("source-unit {status} {anchor} {code} {detail}");
        }
    }
    if let Some(selection) = &report.selection {
        render_theory_selection(&selection.theory_selection);
        println!("source-digest {}", selection.source_digest);
        println!("program-digest {}", selection.program_digest);
        println!("selection-digest {}", selection.selection_digest);
        println!("semantic-units {}", selection.units);
        println!("claims-read {}", selection.claims_read);
        for claim in &selection.claims {
            for carrier in &claim.carriers {
                println!("claim-carrier {} {carrier}", claim.anchor);
            }
        }
        // `complete` is the one computed value; these lines only spell it. It covers
        // the native accounting; the OWL leg's state is printed under that leg.
        if selection.complete {
            println!("selection complete zero-drops (native accounting)");
        } else if report.engines.is_none() {
            // A failed operation can report its selection before engines run.
            println!(
                "selection incomplete drops={} (named before the operation failed)",
                selection.drops.len()
            );
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
