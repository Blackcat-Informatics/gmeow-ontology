// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Indexed RDF 1.2 admission for the finite attributed-context evaluator.
//!
//! Each request selects the single source graph containing its declaration.
//! Formula and context records are confined to that graph. Evidence lives in
//! the explicitly selected named world, as attributed RDF 1.2 claims. An
//! unannotated positive quad is not an attributed claim.

mod monitor;
pub(crate) mod native;
mod path;
mod temporal;
#[cfg(test)]
mod test_support;
#[cfg(test)]
mod tests;

use std::collections::{BTreeMap, BTreeSet};
use std::sync::OnceLock;

use gmeow_errors::{DiagLedger, StageId};
use gmeow_logic_compile::ir::{Formula, LOGIC_NAMESPACE, Term};
use purrdf::sparql::StopSignal;
use purrdf::{DatasetView, GraphMatch, RdfDataset, TermRef, TermValue};

use super::composite::Program;
use crate::result::{
    Assumption, BudgetLimit, BudgetUsage, CompletenessStatus, DerivationRef, EvaluationStatus,
    InformationState, InputStatus, PreservationClaim, ReasoningResult, ResultPayload,
    ResultProvenance,
};
use crate::runtime::IncompleteCause;

pub use super::composite::AssessmentAnchor;
pub use super::composite::Inference as ContextualInference;
pub use super::composite::{PathObservation, TemporalBasis, TemporalPrefix};
pub use monitor::ContextualMonitor;
pub use path::{PathQuery, PathSelection, PreparedPath, PreparedPathQuery, evaluate_path};

use super::GMEOW_NS;
use super::composite::{
    Accessibility, AdmissionError, Context, Evidence, Frame, NormScope, ProtocolScope, Successors,
    TemporalTrace, Transition,
};

const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";

/// The exact finite attributed modal fragment selected by this evaluator.
pub const FRAGMENT: &str = "https://blackcatinformatics.ca/logic/FiniteAttributedModalFragment";

pub(crate) const RULE_IRI: &str = "https://blackcatinformatics.ca/logic/rule/contextual-evaluation";

/// One explicit query's shared result, interruption cause, and evidence DAG.
#[derive(Debug)]
pub struct ContextualAssessment {
    /// The authored evaluation request; it is never asserted as an axiom.
    pub request: String,
    /// The authored formula root, compiled through the shared standard translation.
    pub formula: String,
    /// The shared five-axis reasoning result, including its attributed context.
    pub result: ReasoningResult,
    /// The exact operational stop cause, distinct from missing semantic evidence.
    pub interrupted: Option<IncompleteCause>,
    /// Successful physical firings, including partial work preceding an interruption.
    pub inferences: Vec<ContextualInference>,
    /// Every assessment address referenced by the evidence DAG, fully described.
    pub anchors: Vec<AssessmentAnchor>,
    /// Exact observed journal prefixes consumed by finite temporal judgments.
    pub temporal_prefixes: Vec<TemporalBasis>,
    /// Native attribution receipts actually reached by the emitted evidence DAG.
    pub native_evidence: Vec<NativeEvidence>,
    /// Source-anchored diagnostics for explicit fragment refusal.
    pub diagnostics: DiagLedger,
}

/// One borrowed-closure attribution fact, addressed with its world and exact
/// native receipt. The shared receipt itself is never re-minted or relabeled.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct NativeEvidence {
    identity: String,
    receipt: crate::explain::AxiomReceipt,
    object: String,
}

impl NativeEvidence {
    /// World-scoped evidence address used by contextual proof dependencies.
    pub fn identity(&self) -> &str {
        &self.identity
    }
    /// The native explanation authority's exact row.
    pub fn row(&self) -> &crate::explain::Row {
        &self.receipt.row
    }
    /// The admitted IRI object of this attribution metadata fact.
    pub fn object(&self) -> &str {
        &self.object
    }
    /// Exact firing label used in the native derivation hash.
    pub fn raw_rule_identity(&self) -> &str {
        &self.receipt.raw_rule_identity
    }

    fn from_axiom(axiom: &crate::reason::InferredAxiom, object: String) -> Self {
        let receipt = crate::explain::receipt_for_axiom(axiom);
        let conclusion = crate::explain::reifier_from_row(&receipt.row);
        let identity = crate::provenance::mint_derivation_id(
            "https://blackcatinformatics.ca/logic/rule/contextual-native-evidence",
            &[&axiom.world, &conclusion, &receipt.row.derivation_id],
        );
        Self {
            identity,
            receipt,
            object,
        }
    }
}

fn diagnostic(error: AdmissionError) -> gmeow_errors::Diag {
    match error {
        AdmissionError::Malformed(detail) => {
            super::modal_err(format!("invalid contextual input: {detail}"))
        }
        AdmissionError::OutsideFragment(detail) => {
            gmeow_errors::Diag::of_kind(crate::error::ContextualFragment {
                detail: format!("outside {FRAGMENT}: {detail}"),
            })
        }
        AdmissionError::JournalLimit { limit } => {
            gmeow_errors::Diag::of_kind(crate::error::TemporalJournalAdmission {
                detail: format!("finite journal exceeds its {limit}-entry admission bound"),
            })
        }
    }
}

/// Evaluate every explicitly typed contextual request in an already supplied
/// RDF 1.2 dataset. No request means no context/evidence scan and no evaluation.
///
/// The optional budget counts memoized formula/context judgments across the
/// selected requests, in lexical request order. It does not claim to bound RDF
/// parsing or formula admission. Cancellation uses the shared PurRDF signal.
/// No source, world, effect receipt, or journal event is produced by this API.
///
/// # Errors
/// Rejects malformed context records, incomplete required bindings, ambiguous
/// successor inventories, and invalid formula syntax.
pub fn evaluate_requests(
    dataset: &RdfDataset,
    max_steps: Option<u64>,
    stop: Option<&dyn StopSignal>,
) -> gmeow_errors::Result<Vec<ContextualAssessment>> {
    evaluate_requests_with_closure(dataset, &[], max_steps, stop)
}

/// Native post-pass over an already-computed closure. Only attributed claim
/// metadata is selected; context declarations and formula ownership stay explicit
/// source input. This boundary never computes or copies a cumulative closure.
pub(crate) fn evaluate_requests_with_closure(
    dataset: &RdfDataset,
    closure: &[crate::reason::InferredAxiom],
    max_steps: Option<u64>,
    stop: Option<&dyn StopSignal>,
) -> gmeow_errors::Result<Vec<ContextualAssessment>> {
    let requests = request_sources(dataset, None).map_err(diagnostic)?;
    let mut frames = BTreeMap::new();
    let mut remaining = max_steps;
    let mut results = Vec::new();
    for (identity, (request, source_graph)) in requests {
        let frame = match frames.entry(source_graph) {
            std::collections::btree_map::Entry::Occupied(entry) => entry.into_mut(),
            std::collections::btree_map::Entry::Vacant(entry) => entry.insert(
                RdfFrame::load_in_graph(dataset, closure, source_graph).map_err(diagnostic)?,
            ),
        };
        let assessment = assess(frame, request, identity, remaining, stop)?;
        remaining = remaining.map(|budget| {
            budget.saturating_sub(assessment.result.provenance.consumed_budget.consumed)
        });
        results.push(assessment);
    }
    Ok(results)
}

/// Evaluate exactly one authored request against caller-supplied RDF 1.2 data.
///
/// # Errors
/// The selector must name a typed request in exactly one source graph. Its
/// formula and context records must be present in that same graph. The same
/// required-input and evidence-world checks apply as in [`evaluate_requests`].
pub fn evaluate_request(
    dataset: &RdfDataset,
    request: &str,
    max_steps: Option<u64>,
    stop: Option<&dyn StopSignal>,
) -> gmeow_errors::Result<ContextualAssessment> {
    let (selected, source_graph) = request_sources(dataset, Some(request))
        .map_err(diagnostic)?
        .remove(request)
        .ok_or_else(|| diagnostic(malformed("the selected contextual request is absent")))?;
    let frame = RdfFrame::load_in_graph(dataset, &[], source_graph).map_err(diagnostic)?;
    assess(&frame, selected, request.to_owned(), max_steps, stop)
}

struct PreparedRequest {
    formula: String,
    selected: String,
    source: Formula,
    provenance: ResultProvenance,
}

fn prepare_request<D: DatasetView + ?Sized>(
    frame: &RdfFrame<'_, D>,
    request: D::Id,
    max_steps: Option<u64>,
) -> gmeow_errors::Result<PreparedRequest> {
    let (formula, selected) = request_coordinates(frame, request)?;
    let coordinates = frame.context(&selected).map_err(diagnostic)?;
    let source = match frame.source_graph {
        None => gmeow_logic_compile::frontend::reconstruct_formula_in_context(
            frame.dataset,
            &formula,
            &selected,
        )?,
        Some(graph) => gmeow_logic_compile::frontend::reconstruct_formula_in_named_context(
            frame.dataset,
            &iri(frame.dataset, graph).map_err(diagnostic)?,
            &formula,
            &selected,
        )?,
    };
    let basis = serde_json::to_vec(&(
        FRAGMENT,
        crate::runtime::EngineContract::current().descriptor_hash,
        source.content_key().to_string(),
        frame.basis_digest(),
        &selected,
        coordinates,
        max_steps,
    ))
    .expect("finite query contract serializes");
    let mut provenance = ResultProvenance::native(
        blake3::hash(&basis).to_hex().to_string(),
        &coordinates.world,
    );
    provenance.query.clone_from(&formula);
    provenance.conclusion.clone_from(&formula);
    provenance.context.standpoint = Some(coordinates.standpoint.clone());
    provenance.context.attributed = Some(selected.clone());
    Ok(PreparedRequest {
        formula,
        selected,
        source,
        provenance,
    })
}

fn request_coordinates<D: DatasetView + ?Sized>(
    frame: &RdfFrame<'_, D>,
    request: D::Id,
) -> gmeow_errors::Result<(String, String)> {
    let metadata = Metadata {
        dataset: frame.dataset,
        graph: frame
            .source_graph
            .map_or(GraphMatch::Default, GraphMatch::Named),
    };
    let formula = required_iri(&metadata, request, "queryFormula").map_err(diagnostic)?;
    let selected = required_iri(&metadata, request, "queryContext").map_err(diagnostic)?;
    frame.context(&selected).map_err(diagnostic)?;
    Ok((formula, selected))
}

fn assess<D: DatasetView + ?Sized>(
    frame: &RdfFrame<'_, D>,
    request: D::Id,
    identity: String,
    max_steps: Option<u64>,
    stop: Option<&dyn StopSignal>,
) -> gmeow_errors::Result<ContextualAssessment> {
    let PreparedRequest {
        formula,
        selected,
        source,
        mut provenance,
    } = prepare_request(frame, request, max_steps)?;
    let program = match Program::lower(&source, &selected) {
        Ok(program) => program,
        Err(AdmissionError::Malformed(detail)) => return Err(diagnostic(malformed(detail))),
        Err(error @ AdmissionError::JournalLimit { .. }) => return Err(diagnostic(error)),
        Err(AdmissionError::OutsideFragment(detail)) => {
            let preservation = PreservationClaim::unsupported_with([formula.clone()]);
            provenance.projection_class = preservation.clone();
            let mut diagnostics = DiagLedger::new();
            diagnostics.attach(
                diagnostic(AdmissionError::OutsideFragment(detail)).with_focus(&identity),
                StageId("finite-attributed-modal".into()),
            );
            return Ok(ContextualAssessment {
                request: identity,
                formula,
                result: ReasoningResult::new(
                    InputStatus::Valid,
                    EvaluationStatus::Unsupported,
                    CompletenessStatus::Unknown,
                    preservation,
                    InformationState::NotEvaluated,
                    provenance,
                    ResultPayload::Empty,
                ),
                interrupted: None,
                inferences: Vec::new(),
                anchors: Vec::new(),
                temporal_prefixes: Vec::new(),
                native_evidence: Vec::new(),
                diagnostics,
            });
        }
    };
    let evaluation = program
        .evaluate(frame, &selected, max_steps, stop)
        .map_err(|error| diagnostic(error).with_focus(&identity))?;
    finish_assessment(frame, identity, formula, provenance, max_steps, evaluation)
}

trait AssessmentEvidence {
    fn attribution_inferences(&self) -> &BTreeMap<String, ContextualInference>;
    fn native_evidence(&self) -> &BTreeMap<String, NativeEvidence>;
}

impl<D: DatasetView + ?Sized> AssessmentEvidence for RdfFrame<'_, D> {
    fn attribution_inferences(&self) -> &BTreeMap<String, ContextualInference> {
        &self.attribution_inferences
    }
    fn native_evidence(&self) -> &BTreeMap<String, NativeEvidence> {
        &self.native_evidence
    }
}

fn finish_assessment(
    frame: &impl AssessmentEvidence,
    identity: String,
    formula: String,
    mut provenance: ResultProvenance,
    max_steps: Option<u64>,
    mut evaluation: super::composite::Evaluation,
) -> gmeow_errors::Result<ContextualAssessment> {
    // Attach only native evidence reachable from committed judgments. Unvisited
    // claims and interrupted parent work must not become successful proof rows.
    let mut native_evidence = BTreeMap::new();
    let mut seen = BTreeSet::new();
    let mut pending = evaluation
        .inferences
        .iter()
        .flat_map(|inference| inference.antecedents.iter().cloned())
        .collect::<Vec<_>>();
    while let Some(identity) = pending.pop() {
        if !seen.insert(identity.clone()) {
            continue;
        }
        if let Some(inference) = frame.attribution_inferences().get(&identity) {
            pending.extend(inference.antecedents.iter().cloned());
            evaluation.inferences.push(inference.clone());
        }
        if let Some(receipt) = frame.native_evidence().get(&identity) {
            native_evidence.insert(identity, receipt.clone());
        }
    }
    evaluation
        .inferences
        .sort_by(|left, right| left.identity.cmp(&right.identity));
    let evidence = evaluation.evidence;
    let derivation = |identity: String| {
        let by_id: BTreeMap<_, _> = evaluation
            .inferences
            .iter()
            .map(|inference| (inference.identity.as_str(), inference))
            .collect();
        let mut cited_iris = BTreeSet::new();
        let mut pending = vec![identity.as_str()];
        while let Some(antecedent) = pending.pop() {
            if !cited_iris.insert(antecedent.to_owned()) {
                continue;
            }
            if let Some(inference) = by_id.get(antecedent) {
                cited_iris.insert(inference.rule.clone());
                cited_iris.insert(inference.context.clone());
                pending.extend(inference.antecedents.iter().map(String::as_str));
            }
            if let Some(evidence) = native_evidence.get(antecedent) {
                let row = evidence.row();
                cited_iris.extend([
                    row.graph.clone(),
                    row.rule_iri.clone(),
                    row.derivation_id.clone(),
                ]);
                cited_iris.extend(row.source_quad_ids.iter().cloned());
            }
        }
        DerivationRef {
            derivation_id: identity,
            cited_iris,
        }
    };
    provenance.proof = evidence.support.clone().map(derivation);
    provenance.counterproof = evidence.opposition.clone().map(derivation);
    provenance.consumed_budget = BudgetUsage {
        consumed: evaluation.consumed,
        allowance: max_steps,
        limit: (evaluation.interrupted == Some(IncompleteCause::StepBudget))
            .then_some(BudgetLimit::Inference),
    };
    let preservation = PreservationClaim::exact();
    provenance.projection_class = preservation.clone();
    provenance.certified_fragment = Some(FRAGMENT.into());
    provenance.assumptions.insert(Assumption::OpenWorld);
    let information = match (
        evidence.support.is_some(),
        evidence.opposition.is_some(),
        evidence.complete,
    ) {
        (true, true, _) => InformationState::Both,
        (true, false, _) => InformationState::Supported,
        (false, true, _) => InformationState::Opposed,
        (false, false, true) => InformationState::Neither,
        (false, false, false) => InformationState::Undetermined,
    };
    let result = ReasoningResult::new(
        InputStatus::Valid,
        if evaluation.interrupted.is_some() {
            EvaluationStatus::BudgetExhausted
        } else {
            EvaluationStatus::Completed
        },
        if evidence.complete {
            CompletenessStatus::CompleteForFragment
        } else {
            CompletenessStatus::Incomplete
        },
        preservation,
        information,
        provenance,
        ResultPayload::Empty,
    );
    result.validate()?;
    Ok(ContextualAssessment {
        request: identity,
        formula,
        result,
        interrupted: evaluation.interrupted,
        inferences: evaluation.inferences,
        anchors: evaluation.anchors,
        temporal_prefixes: evaluation.temporal_prefixes,
        native_evidence: native_evidence.into_values().collect(),
        diagnostics: DiagLedger::new(),
    })
}

fn malformed(message: impl Into<String>) -> AdmissionError {
    AdmissionError::Malformed(message.into())
}

fn iri_id<D: DatasetView + ?Sized>(dataset: &D, iri: &str) -> Option<D::Id> {
    dataset.term_id_by_value(&TermValue::iri(iri))
}

fn iri<D: DatasetView + ?Sized>(dataset: &D, term: D::Id) -> Result<String, AdmissionError> {
    match dataset.resolve(term) {
        TermRef::Iri(value) => Ok(value.to_owned()),
        _ => Err(malformed("a contextual identity or binding must be an IRI")),
    }
}

/// Borrow one metadata graph without merging the evidence worlds it describes.
struct Metadata<'a, D: DatasetView + ?Sized = RdfDataset> {
    dataset: &'a D,
    graph: GraphMatch<D::Id>,
}

impl<D: DatasetView + ?Sized> std::ops::Deref for Metadata<'_, D> {
    type Target = D;
    fn deref(&self) -> &Self::Target {
        self.dataset
    }
}

/// Request identity selects one graph. Repeating an identity in another graph
/// is ambiguous and cannot silently choose whichever graph happens to sort first.
fn request_sources<D: DatasetView + ?Sized>(
    dataset: &D,
    selected: Option<&str>,
) -> Result<BTreeMap<String, (D::Id, Option<D::Id>)>, AdmissionError> {
    let (Some(predicate), Some(class)) = (
        iri_id(dataset, RDF_TYPE),
        iri_id(
            dataset,
            &format!("{LOGIC_NAMESPACE}ContextualEvaluationRequest"),
        ),
    ) else {
        return Ok(BTreeMap::new());
    };
    let subject = match selected {
        Some(identity) => match iri_id(dataset, identity) {
            Some(subject) => Some(subject),
            None => return Ok(BTreeMap::new()),
        },
        None => None,
    };
    let mut requests = BTreeMap::new();
    for quad in gmeow_logic_compile::frontend::selected_source_statements(
        dataset,
        subject,
        Some(predicate),
        Some(class),
        GraphMatch::Any,
    ) {
        let identity = iri(dataset, quad.s)?;
        if let Some(graph) = quad.g {
            iri(dataset, graph)?;
        }
        if requests
            .insert(identity.clone(), (quad.s, quad.g))
            .is_some()
        {
            return Err(malformed(format!(
                "contextual request {identity} is declared in multiple source graphs"
            )));
        }
    }
    Ok(requests)
}

fn objects<D: DatasetView + ?Sized>(
    dataset: &Metadata<'_, D>,
    subject: D::Id,
    predicate: &str,
) -> BTreeSet<D::Id> {
    let Some(predicate) = iri_id(dataset.dataset, predicate) else {
        return BTreeSet::new();
    };
    gmeow_logic_compile::frontend::selected_source_statements(
        dataset.dataset,
        Some(subject),
        Some(predicate),
        None,
        dataset.graph,
    )
    .map(|quad| quad.o)
    .collect()
}

fn optional<D: DatasetView + ?Sized>(
    dataset: &Metadata<'_, D>,
    subject: D::Id,
    property: &str,
) -> Result<Option<D::Id>, AdmissionError> {
    let values = objects(dataset, subject, &format!("{LOGIC_NAMESPACE}{property}"));
    if values.len() > 1 {
        return Err(malformed(format!("logic:{property} has multiple bindings")));
    }
    Ok(values.first().copied())
}

fn required<D: DatasetView + ?Sized>(
    dataset: &Metadata<'_, D>,
    subject: D::Id,
    property: &str,
) -> Result<D::Id, AdmissionError> {
    optional(dataset, subject, property)?
        .ok_or_else(|| malformed(format!("missing required logic:{property}")))
}

fn required_iri<D: DatasetView + ?Sized>(
    dataset: &Metadata<'_, D>,
    subject: D::Id,
    property: &str,
) -> Result<String, AdmissionError> {
    iri(dataset.dataset, required(dataset, subject, property)?)
}

fn optional_iri<D: DatasetView + ?Sized>(
    dataset: &Metadata<'_, D>,
    subject: D::Id,
    property: &str,
) -> Result<Option<String>, AdmissionError> {
    optional(dataset, subject, property)?
        .map(|value| iri(dataset.dataset, value))
        .transpose()
}

fn instances<D: DatasetView + ?Sized>(dataset: &Metadata<'_, D>, class: &str) -> BTreeSet<D::Id> {
    let (Some(predicate), Some(class)) = (
        iri_id(dataset.dataset, RDF_TYPE),
        iri_id(dataset.dataset, &format!("{LOGIC_NAMESPACE}{class}")),
    ) else {
        return BTreeSet::new();
    };
    gmeow_logic_compile::frontend::selected_source_statements(
        dataset.dataset,
        None,
        Some(predicate),
        Some(class),
        dataset.graph,
    )
    .map(|quad| quad.s)
    .collect()
}

fn require_type<D: DatasetView + ?Sized>(
    dataset: &Metadata<'_, D>,
    subject: D::Id,
    class: &str,
) -> Result<(), AdmissionError> {
    iri(dataset.dataset, subject)?;
    let types = objects(dataset, subject, RDF_TYPE);
    if !iri_id(dataset.dataset, &format!("{LOGIC_NAMESPACE}{class}"))
        .is_some_and(|class| types.contains(&class))
    {
        return Err(malformed(format!(
            "contextual record requires type logic:{class}"
        )));
    }
    Ok(())
}

fn closure_value<D: DatasetView + ?Sized>(
    dataset: &Metadata<'_, D>,
    subject: D::Id,
    property: &str,
) -> Result<bool, AdmissionError> {
    match required_iri(dataset, subject, property)?.strip_prefix(LOGIC_NAMESPACE) {
        Some("ClosedWorldClosure") => Ok(true),
        Some("OpenWorldClosure") => Ok(false),
        _ => Err(malformed(format!(
            "logic:{property} requires a typed closure value"
        ))),
    }
}

fn position<D: DatasetView + ?Sized>(dataset: &D, term: D::Id) -> Result<u64, AdmissionError> {
    match dataset.resolve(term) {
        TermRef::Literal {
            lexical,
            datatype,
            language: None,
            direction: None,
        } if matches!(
            dataset.resolve(datatype),
            TermRef::Iri("http://www.w3.org/2001/XMLSchema#nonNegativeInteger")
        ) =>
        {
            lexical
                .parse()
                .map_err(|_| malformed("contextPosition requires a nonnegative integer"))
        }
        _ => Err(malformed(
            "contextPosition requires an xsd:nonNegativeInteger literal",
        )),
    }
}

fn context<D: DatasetView + ?Sized>(
    dataset: &Metadata<'_, D>,
    subject: D::Id,
) -> Result<Context, AdmissionError> {
    let journal = optional_iri(dataset, subject, "contextJournal")?;
    let index = optional(dataset, subject, "contextPosition")?
        .map(|term| position(dataset.dataset, term))
        .transpose()?;
    let journal_position = match (journal, index) {
        (None, None) => None,
        (Some(journal), Some(index)) => Some((journal, index)),
        _ => {
            return Err(malformed(
                "contextJournal and contextPosition must be selected together",
            ));
        }
    };
    let norm_scope = optional(dataset, subject, "contextNormScope")?
        .map(|scope| {
            require_type(dataset, scope, "NormScope")?;
            Ok(NormScope {
                issuer: required_iri(dataset, scope, "normIssuer")?,
                bearer: required_iri(dataset, scope, "normBearer")?,
                policy: required_iri(dataset, scope, "normPolicy")?,
            })
        })
        .transpose()?;
    let protocol_scope = optional(dataset, subject, "contextProtocolScope")?
        .map(|scope| {
            require_type(dataset, scope, "ProtocolScope")?;
            Ok(ProtocolScope {
                protocol: required_iri(dataset, scope, "protocolIdentity")?,
                role: required_iri(dataset, scope, "protocolRole")?,
            })
        })
        .transpose()?;
    let context = Context {
        world: required_iri(dataset, subject, "contextWorld")?,
        standpoint: required_iri(dataset, subject, "contextStandpoint")?,
        enactment: optional_iri(dataset, subject, "contextEnactment")?,
        journal_position,
        norm_scope,
        protocol_scope,
    };
    context.validate()?;
    Ok(context)
}

/// The indexed input is borrowed. Only selected context metadata and attributed
/// evidence handles are retained; no cumulative closure is copied or regenerated.
pub(super) struct RdfFrame<'a, D: DatasetView + ?Sized = RdfDataset> {
    dataset: &'a D,
    source_graph: Option<D::Id>,
    contexts: BTreeMap<String, Context>,
    evidence_closed: BTreeMap<String, bool>,
    claims: BTreeMap<(String, D::Id, D::Id, D::Id), Evidence>,
    edges: BTreeMap<(String, Accessibility), Successors>,
    attribution_inferences: BTreeMap<String, ContextualInference>,
    native_evidence: BTreeMap<String, NativeEvidence>,
    journals: BTreeMap<String, OnceLock<Result<super::journal::FiniteJournal, AdmissionError>>>,
}

impl<'a, D: DatasetView + ?Sized> RdfFrame<'a, D> {
    /// Identity of the admitted evidence basis, using resolved RDF values and
    /// canonical set order. Dataset-local IDs never cross this boundary.
    fn basis_digest(&self) -> String {
        let mut records = BTreeSet::new();
        self.journal_basis(&mut records);
        records.insert(
            serde_json::to_vec(&(
                "source-graph",
                self.source_graph.map(|graph| {
                    crate::reason::dataset::native(self.dataset, graph).to_canonical_bytes()
                }),
            ))
            .expect("selected source graph serializes"),
        );
        for (identity, context) in &self.contexts {
            records.insert(
                serde_json::to_vec(&("context", identity, context, self.evidence_closed[identity]))
                    .expect("admitted context serializes"),
            );
        }
        for ((context, subject, predicate, object), evidence) in &self.claims {
            records.insert(
                serde_json::to_vec(&(
                    "claim",
                    context,
                    crate::reason::dataset::native(self.dataset, *subject).to_canonical_bytes(),
                    crate::reason::dataset::native(self.dataset, *predicate).to_canonical_bytes(),
                    crate::reason::dataset::native(self.dataset, *object).to_canonical_bytes(),
                    &evidence.support,
                    &evidence.opposition,
                ))
                .expect("resolved evidence serializes"),
            );
        }
        for ((context, axis), successors) in &self.edges {
            let members = successors
                .transitions
                .iter()
                .map(|transition| (&transition.destination, &transition.witness))
                .collect::<BTreeSet<_>>();
            records.insert(
                serde_json::to_vec(&(
                    "successors",
                    context,
                    axis.iri(),
                    members,
                    &successors.closure_witness,
                ))
                .expect("admitted successor inventory serializes"),
            );
        }
        let mut hash = blake3::Hasher::new();
        hash.update(b"gmeow-finite-attributed-modal-basis-v1\0");
        for record in records {
            hash.update(&(record.len() as u64).to_le_bytes());
            hash.update(&record);
        }
        hash.finalize().to_hex().to_string()
    }

    fn load_in_graph(
        dataset: &'a D,
        closure: &[crate::reason::InferredAxiom],
        source_graph: Option<D::Id>,
    ) -> Result<Self, AdmissionError> {
        let mut frame = Self::prepare_in_graph(dataset, source_graph)?;
        frame.index_claims(closure)?;
        Ok(frame)
    }

    /// Source grammar is admitted before native execution. Attribution fields
    /// are indexed only after their declared native writers have completed.
    fn prepare_in_graph(
        dataset: &'a D,
        source_graph: Option<D::Id>,
    ) -> Result<Self, AdmissionError> {
        let metadata = Metadata {
            dataset,
            graph: source_graph.map_or(GraphMatch::Default, GraphMatch::Named),
        };
        let mut frame = Self {
            dataset,
            source_graph,
            contexts: BTreeMap::new(),
            evidence_closed: BTreeMap::new(),
            claims: BTreeMap::new(),
            edges: BTreeMap::new(),
            attribution_inferences: BTreeMap::new(),
            native_evidence: BTreeMap::new(),
            journals: BTreeMap::new(),
        };
        for subject in instances(&metadata, "AttributedContext") {
            let identity = iri(dataset, subject)?;
            frame.evidence_closed.insert(
                identity.clone(),
                closure_value(&metadata, subject, "evidenceClosure")?,
            );
            let coordinates = context(&metadata, subject)?;
            if let Some((journal, _)) = &coordinates.journal_position {
                frame.journals.entry(journal.clone()).or_default();
            }
            frame.contexts.insert(identity, coordinates);
        }
        for subject in instances(&metadata, "ContextSuccessorSet") {
            let witness = iri(dataset, subject)?;
            let source = required_iri(&metadata, subject, "successorContext")?;
            let coordinates = frame.context(&source)?;
            let axis = required_iri(&metadata, subject, "successorAxis")?;
            let axis = Accessibility::parse(&axis).ok_or_else(|| {
                malformed("successorAxis is not a typed modal accessibility relation")
            })?;
            let closed = closure_value(&metadata, subject, "successorClosure")?;
            let mut transitions = Vec::new();
            for destination in objects(
                &metadata,
                subject,
                &format!("{LOGIC_NAMESPACE}successorMember"),
            ) {
                let destination = iri(dataset, destination)?;
                coordinates.validate_transition(axis, frame.context(&destination)?)?;
                transitions.push(Transition {
                    destination,
                    witness: witness.clone(),
                });
            }
            if frame
                .edges
                .insert(
                    (source, axis),
                    Successors {
                        transitions,
                        closure_witness: closed.then_some(witness),
                    },
                )
                .is_some()
            {
                return Err(malformed(
                    "multiple successor inventories select the same context and axis",
                ));
            }
        }
        Ok(frame)
    }

    fn index_claims(
        &mut self,
        closure: &[crate::reason::InferredAxiom],
    ) -> Result<(), AdmissionError> {
        let dataset = self.dataset;
        let according_to = iri_id(dataset, &format!("{GMEOW_NS}accordingTo"));
        let status = iri_id(dataset, &format!("{GMEOW_NS}standpointSupportStatus"));
        let selected_worlds = self
            .contexts
            .values()
            .map(|context| context.world.as_str())
            .collect::<BTreeSet<_>>();
        let reifiers = dataset
            .reifier_quads()
            .map(|quad| (quad.s, quad.o, quad.g))
            .map(|(reifier, _, graph)| (reifier, graph))
            .collect::<BTreeSet<_>>();
        let owner_property = format!("{GMEOW_NS}accordingTo");
        let status_property = format!("{GMEOW_NS}standpointSupportStatus");
        // Borrow only relevant derived annotation rows. Match their named world
        // and existing RDF 1.2 reifier; a raw inferred proposition is never a
        // substitute for an explicitly attributed support/opposition assertion.
        let mut derived: BTreeMap<_, BTreeMap<String, &crate::reason::InferredAxiom>> =
            BTreeMap::new();
        for axiom in closure.iter().filter(|axiom| !axiom.is_edb) {
            if axiom.predicate != owner_property && axiom.predicate != status_property {
                continue;
            }
            let (Some(reifier), Some(world)) = (
                iri_id(dataset, &axiom.subject),
                iri_id(dataset, &axiom.world),
            ) else {
                continue;
            };
            if !selected_worlds.contains(axiom.world.as_str()) {
                continue;
            }
            if !reifiers.contains(&(reifier, Some(world))) {
                continue;
            }
            let TermValue::Iri(value) = &axiom.object else {
                return Err(malformed("derived attribution metadata must be IRI-valued"));
            };
            if axiom.rule_name.is_none() || axiom.premises.is_empty() {
                return Err(malformed(
                    "derived attribution metadata requires its native firing rule and premises",
                ));
            }
            let candidates = derived
                .entry((reifier, axiom.predicate.as_str(), Some(world)))
                .or_default();
            let selected = candidates.entry(value.clone()).or_insert(axiom);
            if axiom < *selected {
                *selected = axiom;
            }
        }
        // RDF 1.2 annotations have their own side table. Keep both physical
        // spellings in one logical lookup; neither can silently shadow the other.
        let mut annotations: BTreeMap<_, BTreeSet<_>> = BTreeMap::new();
        for (reifier, predicate, object, graph) in dataset
            .annotation_quads()
            .map(|quad| (quad.s, quad.p, quad.o, quad.g))
        {
            if Some(predicate) == according_to || Some(predicate) == status {
                annotations
                    .entry((reifier, predicate, graph))
                    .or_default()
                    .insert(object);
            }
        }
        let values = |subject,
                      predicate,
                      property: &str,
                      graph: Option<D::Id>|
         -> Result<
            BTreeMap<String, Option<&crate::reason::InferredAxiom>>,
            AdmissionError,
        > {
            let mut values = BTreeSet::new();
            if let Some(predicate) = predicate {
                values.extend(
                    dataset
                        .quads_for_pattern(
                            Some(subject),
                            Some(predicate),
                            None,
                            graph.map_or(GraphMatch::Default, GraphMatch::Named),
                        )
                        .map(|quad| quad.o),
                );
                values.extend(
                    annotations
                        .get(&(subject, predicate, graph))
                        .into_iter()
                        .flatten()
                        .copied(),
                );
            }
            let mut values = values
                .into_iter()
                .map(|value| iri(dataset, value).map(|value| (value, None)))
                .collect::<Result<BTreeMap<_, _>, _>>()?;
            if let Some(additional) = derived.get(&(subject, property, graph)) {
                for (value, axiom) in additional {
                    // An explicit source assertion is already a complete witness
                    // for this field. Do not attach a redundant derived receipt.
                    values.entry(value.clone()).or_insert(Some(*axiom));
                }
            }
            Ok(values)
        };
        let mut by_world: BTreeMap<D::Id, Vec<(&str, &str)>> = BTreeMap::new();
        for (identity, coordinates) in &self.contexts {
            let world = iri_id(dataset, &coordinates.world).expect("admitted world binding");
            by_world
                .entry(world)
                .or_default()
                .push((identity, &coordinates.standpoint));
        }
        for (reifier, statement, graph) in
            dataset.reifier_quads().map(|quad| (quad.s, quad.o, quad.g))
        {
            let Some(contexts) = graph.and_then(|world| by_world.get(&world)) else {
                continue;
            };
            let owners = values(
                reifier,
                according_to,
                &format!("{GMEOW_NS}accordingTo"),
                graph,
            )?;
            if !contexts
                .iter()
                .any(|(_, standpoint)| owners.contains_key(*standpoint))
            {
                continue;
            }
            let polarities = values(
                reifier,
                status,
                &format!("{GMEOW_NS}standpointSupportStatus"),
                graph,
            )?;
            if polarities.len() != 1 {
                return Err(malformed(
                    "an attributed claim requires exactly one support status",
                ));
            }
            let (polarity, polarity_axiom) = polarities.first_key_value().expect("one status");
            let (support, opposition) = match polarity.strip_prefix(GMEOW_NS) {
                Some("supportSupported") => (true, false),
                Some("supportOpposed") => (false, true),
                Some("supportBoth") => (true, true),
                Some("supportNeither") => (false, false),
                _ => return Err(malformed("unrecognized attributed support status")),
            };
            let witness = iri(dataset, reifier)?;
            let TermRef::Triple { s, p, o } = dataset.resolve(statement) else {
                return Err(malformed("rdf:reifies must bind an RDF 1.2 triple term"));
            };
            for &(identity, standpoint) in contexts {
                let Some(owner_axiom) = owners.get(standpoint) else {
                    continue;
                };
                let mut antecedents = vec![witness.clone()];
                for axiom in [owner_axiom, polarity_axiom].into_iter().flatten() {
                    let value = if axiom.predicate.ends_with("accordingTo") {
                        standpoint
                    } else {
                        polarity.as_str()
                    };
                    let receipt = NativeEvidence::from_axiom(axiom, value.to_owned());
                    antecedents.push(receipt.identity.clone());
                    self.native_evidence
                        .insert(receipt.identity.clone(), receipt);
                }
                let witness = if antecedents.len() > 1 {
                    antecedents.push(identity.to_owned());
                    antecedents.sort();
                    antecedents.dedup();
                    let rule = "https://blackcatinformatics.ca/logic/rule/contextual-attribution";
                    let refs = antecedents.iter().map(String::as_str).collect::<Vec<_>>();
                    let address = crate::provenance::mint_derivation_id(rule, &refs);
                    self.attribution_inferences.insert(
                        address.clone(),
                        ContextualInference {
                            identity: address.clone(),
                            rule: rule.into(),
                            context: identity.into(),
                            antecedents,
                        },
                    );
                    address
                } else {
                    witness.clone()
                };
                let evidence = self
                    .claims
                    .entry((identity.to_owned(), s, p, o))
                    .or_default();
                for (present, slot) in [
                    (support, &mut evidence.support),
                    (opposition, &mut evidence.opposition),
                ] {
                    if present && slot.as_ref().is_none_or(|known| witness < *known) {
                        *slot = Some(witness.clone());
                    }
                }
            }
        }
        Ok(())
    }
}

fn atom_term(term: &Term) -> Result<TermValue, AdmissionError> {
    match term {
        Term::Iri(iri) => Ok(TermValue::iri(iri)),
        Term::Literal(literal) => Ok(crate::rule_ir::literal_value(literal)),
        _ => Err(AdmissionError::OutsideFragment(
            "a contextual atom requires ground RDF terms".into(),
        )),
    }
}

impl<D: DatasetView + ?Sized> Frame for RdfFrame<'_, D> {
    fn temporal(&self, context: &str) -> Result<TemporalTrace, AdmissionError> {
        self.temporal_trace(context)
    }

    fn context(&self, identity: &str) -> Result<&Context, AdmissionError> {
        self.contexts
            .get(identity)
            .ok_or_else(|| malformed(format!("missing attributed context {identity}")))
    }

    fn atom(
        &self,
        context: &str,
        relation: &str,
        subject: &Term,
        object: &Term,
    ) -> Result<Evidence, AdmissionError> {
        self.context(context)?;
        let mut evidence = if let (Some(subject), Some(predicate), Some(object)) = (
            self.dataset.term_id_by_value(&atom_term(subject)?),
            iri_id(self.dataset, relation),
            self.dataset.term_id_by_value(&atom_term(object)?),
        ) {
            self.claims
                .get(&(context.to_owned(), subject, predicate, object))
                .cloned()
                .unwrap_or_default()
        } else {
            Evidence::default()
        };
        evidence.complete = self.evidence_closed[context];
        Ok(evidence)
    }

    fn successors(&self, context: &str, axis: Accessibility) -> Result<Successors, AdmissionError> {
        self.edges
            .get(&(context.to_owned(), axis))
            .cloned()
            .ok_or_else(|| {
                malformed(format!(
                    "missing selected successor inventory for {context} over {}",
                    axis.iri()
                ))
            })
    }
}
