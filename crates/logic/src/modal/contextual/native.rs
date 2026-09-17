// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Contextual judgments as completed-read producers in the shared native run.
//! Preparation retains native source identities and compiled formulas; execution
//! reads frozen stores and emits one indivisible set of assessment metadata.

mod evidence;
mod frame;
mod source;

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use super::{
    FRAGMENT, RULE_IRI, RdfFrame, diagnostic, finish_assessment, prepare_request,
    request_coordinates, request_sources,
};
use crate::modal::composite::{Frame, Program};
use crate::modal::native::NativeModalSupport;
use crate::physical::{
    CompletedWorldReads, LogicalGraph, NativeWorldSnapshot, ProducerEffect, ReadDependency,
    StatementPattern, StepGovernor, WitnessStatement, WorldProducerEffect,
    WorldStatementObservation,
};
use crate::reason::refute::RefutationPremise;
use crate::rule_ir::Fact;
use purrdf::TermValue;

pub use evidence::{
    NativeContextualAnalysisStop, NativeContextualReceipt, NativeContextualUpstreamStop,
};

const OWNER: &str = "https://blackcatinformatics.ca/gmeow/accordingTo";
const STATUS: &str = "https://blackcatinformatics.ca/gmeow/standpointSupportStatus";
const REIFIES: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#reifies";
const LOGIC: &str = "https://blackcatinformatics.ca/logic/";

const DEFINITION_TYPES: [&str; 9] = [
    "ContextualEvaluationRequest",
    "AttributedContext",
    "ContextSuccessorSet",
    "Formula",
    "NormScope",
    "ProtocolScope",
    "TransitionJournal",
    "JournalEntry",
    "Enactment",
];

const DEFINITION_RELATIONS: [&str; 57] = [
    "queryFormula",
    "queryContext",
    "contextWorld",
    "contextStandpoint",
    "contextEnactment",
    "contextJournal",
    "contextPosition",
    "contextNormScope",
    "contextProtocolScope",
    "evidenceClosure",
    "normIssuer",
    "normBearer",
    "normPolicy",
    "protocolIdentity",
    "protocolRole",
    "successorContext",
    "successorAxis",
    "successorClosure",
    "successorMember",
    "relation",
    "argument",
    "termIndex",
    "termIri",
    "termLiteral",
    "termLiteralDatatype",
    "termVariable",
    "termSequenceMarker",
    "termApplication",
    "functionSymbol",
    "not",
    "and",
    "or",
    "iff",
    "antecedent",
    "consequent",
    "forall",
    "exists",
    "quantifiedVariable",
    "inContext",
    "necessarily",
    "possibly",
    "overAccessibility",
    "next",
    "eventually",
    "globally",
    "until",
    "untilLeft",
    "enactmentJournal",
    "journalEntry",
    "journalBoundary",
    "journalInitialHead",
    "journalHead",
    "journalPredecessor",
    "journalPrevHead",
    "journalDeltaIdentity",
    "journalNewHead",
    "journalOutcomeTag",
];

/// Fixed cells used by contextual source grammar. Dynamic basis subjects are
/// still guarded per selected request; this vocabulary makes fixed marker and
/// predicate comparisons precise in the shared abstract interpreter.
pub(crate) fn definition_vocabulary() -> impl Iterator<Item = StatementPattern> {
    DEFINITION_TYPES
        .into_iter()
        .map(|local| {
            StatementPattern::relation(Some(super::RDF_TYPE), Some(&format!("{LOGIC}{local}")))
        })
        .chain(
            DEFINITION_RELATIONS
                .into_iter()
                .map(|local| StatementPattern::relation(Some(&format!("{LOGIC}{local}")), None)),
        )
        .chain(std::iter::once(StatementPattern::relation(
            Some(REIFIES),
            None,
        )))
}

struct Request {
    identity: String,
    formula: String,
    selected: String,
    formula_key: String,
    program: Arc<Program>,
    frame: Arc<frame::PreparedFrame>,
    metadata: Arc<[(String, WitnessStatement)]>,
}

/// Shared immutable contextual preparation for one exact original source.
pub(crate) struct NativeContextualProgram {
    requests: Vec<Request>,
    effects: Vec<WorldProducerEffect>,
    worlds: BTreeMap<String, LogicalGraph>,
    definitions: Vec<WorldStatementObservation>,
}

/// Run-local completion state; repeated native rounds never re-charge a query.
#[derive(Default)]
pub(crate) struct NativeContextualExecution {
    evaluated: BTreeSet<usize>,
}

/// Already charged judgments and their complete metadata publication groups.
pub(crate) struct NativeContextualBatch {
    pub(crate) candidates: Vec<NativeContextualCandidate>,
    /// Evaluated requests survive fact deduplication, including a publication
    /// whose every metadata statement is already asserted in the output world.
    pub(crate) receipts: Vec<Arc<NativeContextualReceipt>>,
    pub(crate) interrupted: bool,
}

pub(crate) struct NativeContextualCandidate {
    pub(crate) owner: String,
    pub(crate) head: Fact,
    pub(crate) receipt: Arc<NativeContextualReceipt>,
}

impl NativeContextualProgram {
    /// Read retained native source columns without a copied RDF dataset. This is
    /// also the preparation path after a source-grammar delta.
    pub(crate) fn prepare(
        sources: &BTreeMap<String, Arc<[RefutationPremise]>>,
    ) -> gmeow_errors::Result<Self> {
        let mut prepared = Self {
            requests: Vec::new(),
            effects: Vec::new(),
            worlds: BTreeMap::new(),
            definitions: Vec::new(),
        };
        let has_request = sources
            .values()
            .flat_map(|sources| sources.iter())
            .any(|row| {
                row.predicate == super::RDF_TYPE
                    && row.object.as_iri()
                        == Some("https://blackcatinformatics.ca/logic/ContextualEvaluationRequest")
            });
        if has_request {
            let view = source::SourceView::new(sources)?;
            let requests = request_sources(&view, None).map_err(diagnostic)?;
            let mut frames = BTreeMap::new();
            let mut rdf_frames = BTreeMap::new();
            let mut programs = BTreeMap::new();
            let mut source_metadata = BTreeMap::<String, Arc<[(String, WitnessStatement)]>>::new();
            for (identity, (request, source_graph)) in requests {
                let rdf = match rdf_frames.entry(source_graph) {
                    std::collections::btree_map::Entry::Occupied(entry) => entry.into_mut(),
                    std::collections::btree_map::Entry::Vacant(entry) => entry.insert(
                        RdfFrame::prepare_in_graph(&view, source_graph).map_err(diagnostic)?,
                    ),
                };
                let (formula, selected) = request_coordinates(rdf, request)?;
                let (program, formula_key) = match programs.entry((
                    source_graph,
                    formula.clone(),
                    selected.clone(),
                )) {
                    std::collections::btree_map::Entry::Occupied(entry) => entry.into_mut(),
                    std::collections::btree_map::Entry::Vacant(entry) => {
                        let request = prepare_request(rdf, request, None)?;
                        let program = Program::lower(&request.source, &request.selected).map_err(|error| {
                            match error {
                                crate::modal::composite::AdmissionError::OutsideFragment(detail) => gmeow_errors::Diag::of_kind(crate::error::ContextualFragment { detail: format!("contextual request {identity} requires a formula outside {FRAGMENT}: {}: {detail}", request.formula) }).with_focus(&identity),
                                other => diagnostic(other).with_focus(&identity),
                            }
                        })?;
                        entry.insert((Arc::new(program), request.source.content_key().to_string()))
                    }
                };
                let frame = match frames.entry(source_graph) {
                    std::collections::btree_map::Entry::Occupied(entry) => Arc::clone(entry.get()),
                    std::collections::btree_map::Entry::Vacant(entry) => {
                        Arc::clone(entry.insert(Arc::new(frame::PreparedFrame::new(rdf)?)))
                    }
                };
                for world in frame.read_worlds() {
                    prepared
                        .worlds
                        .insert(world.clone(), LogicalGraph::Named(TermValue::iri(world)));
                }
                let metadata = source_metadata
                    .entry(frame.source_world.clone())
                    .or_insert_with(|| {
                        sources[&frame.source_world]
                            .iter()
                            .map(|source| {
                                (
                                    frame.source_world.clone(),
                                    WitnessStatement {
                                        subject: crate::facts::skolemize(&source.subject)
                                            .into_owned(),
                                        predicate: source.predicate.clone(),
                                        object: crate::facts::skolemize(&source.object)
                                            .into_owned(),
                                    },
                                )
                            })
                            .collect::<Vec<_>>()
                            .into()
                    });
                prepared.requests.push(Request {
                    identity,
                    formula,
                    selected,
                    formula_key: formula_key.clone(),
                    program: Arc::clone(program),
                    metadata: Arc::clone(metadata),
                    frame,
                });
            }
            prepared.worlds.insert(
                crate::result_rdf::GRAPH_REASONING.into(),
                LogicalGraph::Named(TermValue::iri(crate::result_rdf::GRAPH_REASONING)),
            );
        }
        // Grammar fields are read only in the prepared request metadata worlds.
        // Root admission is guarded separately over every admitted runtime world,
        // including empty output worlds introduced after source preparation.
        let metadata_worlds = prepared
            .requests
            .iter()
            .map(|request| request.frame.source_world.clone())
            .collect::<BTreeSet<_>>();
        for world in metadata_worlds {
            let subjects = prepared
                .requests
                .iter()
                .filter(|request| request.frame.source_world == world)
                .flat_map(|request| request.frame.basis_subjects.iter().cloned())
                .collect::<BTreeSet<_>>();
            for subject in subjects {
                prepared.definitions.push(WorldStatementObservation {
                    world: world.clone(),
                    pattern: StatementPattern::subject(subject),
                });
            }
            for local in DEFINITION_TYPES {
                prepared.definitions.push(WorldStatementObservation {
                    world: world.clone(),
                    pattern: StatementPattern::relation(
                        Some(super::RDF_TYPE),
                        Some(&format!("{LOGIC}{local}")),
                    ),
                });
            }
            for local in DEFINITION_RELATIONS {
                prepared.definitions.push(WorldStatementObservation {
                    world: world.clone(),
                    pattern: StatementPattern::relation(Some(&format!("{LOGIC}{local}")), None),
                });
            }
        }
        let evidence_worlds = prepared
            .requests
            .iter()
            .flat_map(|request| request.frame.read_worlds())
            .collect::<BTreeSet<_>>();
        for world in evidence_worlds {
            prepared.definitions.push(WorldStatementObservation {
                world,
                pattern: StatementPattern::relation(Some(REIFIES), None),
            });
        }
        for request in &prepared.requests {
            let mut reads = Vec::new();
            let mut read_worlds = Vec::new();
            for world in request.frame.read_worlds() {
                for predicate in [OWNER, STATUS] {
                    reads.push((
                        StatementPattern::relation(Some(predicate), None),
                        ReadDependency::Completed,
                    ));
                    read_worlds.push(world.clone());
                }
            }
            // Source grammar is fixed. Positive source reads also bind the request
            // metadata world into the cross-world schedule and its proof index.
            reads.push((
                StatementPattern::relation(None, None),
                ReadDependency::Positive,
            ));
            read_worlds.push(request.frame.source_world.clone());
            prepared.effects.push(WorldProducerEffect {
                owner: crate::result_rdf::GRAPH_REASONING.into(),
                effect: ProducerEffect::new(
                    format!("native-contextual:{}", request.identity),
                    crate::result_rdf::contextual_projection_effects(),
                    reads,
                )
                .requiring_source_admission(),
                read_worlds,
            });
        }
        Ok(prepared)
    }

    pub(crate) fn effects(&self) -> &[WorldProducerEffect] {
        &self.effects
    }
    pub(crate) fn definition_patterns<'a>(
        &'a self,
        worlds: impl IntoIterator<Item = &'a String> + 'a,
    ) -> impl Iterator<Item = WorldStatementObservation> + 'a {
        self.definitions
            .iter()
            .cloned()
            .chain(worlds.into_iter().map(|world| WorldStatementObservation {
                world: world.clone(),
                pattern: StatementPattern::relation(
                    Some(super::RDF_TYPE),
                    Some("https://blackcatinformatics.ca/logic/ContextualEvaluationRequest"),
                ),
            }))
    }
    pub(crate) fn required_worlds(&self) -> &BTreeMap<String, LogicalGraph> {
        &self.worlds
    }
    pub(crate) fn start(&self) -> NativeContextualExecution {
        NativeContextualExecution::default()
    }

    pub(crate) fn evaluate(
        &self,
        state: &mut NativeContextualExecution,
        indices: &[usize],
        worlds: &mut BTreeMap<&str, NativeWorldSnapshot<'_>>,
        completed: &CompletedWorldReads,
        governor: &mut StepGovernor,
    ) -> gmeow_errors::Result<NativeContextualBatch> {
        let mut batch = NativeContextualBatch {
            candidates: Vec::new(),
            receipts: Vec::new(),
            interrupted: false,
        };
        for &index in indices {
            if state.evaluated.contains(&index) {
                continue;
            }
            let request = self
                .requests
                .get(index)
                .ok_or_else(|| diagnostic(super::malformed("unknown contextual producer")))?;
            if !completed.covers(&self.effects[index]) {
                return Err(diagnostic(super::malformed(
                    "contextual producer lacks its exact scheduled read completion",
                )));
            }
            // A request scheduled in the same round that exhausts the allowance
            // has the same unvisited contract as one interrupted before its
            // stratum. Neither path may inspect attribution or collect evidence.
            let frame = if governor.remaining() == Some(0) {
                request.frame.unvisited()
            } else {
                request.frame.bind(worlds)?
            };
            Self::evaluate_one(request, &frame, worlds, governor, &mut batch)?;
            state.evaluated.insert(index);
        }
        Ok(batch)
    }

    /// Finalize every unvisited selected request after the actual upstream stop.
    /// This reads admitted request metadata only. It never evaluates a program,
    /// binds dynamic attribution, observes absence, or charges inference work.
    pub(crate) fn finalize_remaining(
        &self,
        state: &mut NativeContextualExecution,
        worlds: &mut BTreeMap<&str, NativeWorldSnapshot<'_>>,
        governor: &StepGovernor,
        stop: NativeContextualUpstreamStop,
    ) -> gmeow_errors::Result<NativeContextualBatch> {
        use crate::result::{
            Assumption, BudgetLimit, BudgetUsage, CompletenessStatus, EvaluationStatus,
            InformationState, InputStatus, PreservationClaim, ReasoningResult, ResultPayload,
        };
        let mut batch = NativeContextualBatch {
            candidates: Vec::new(),
            receipts: Vec::new(),
            interrupted: false,
        };
        if (0..self.requests.len()).all(|index| state.evaluated.contains(&index)) {
            return Ok(batch);
        }
        stop.validate()?;
        for (index, request) in self.requests.iter().enumerate() {
            if state.evaluated.contains(&index) {
                continue;
            }
            let frame = request.frame.unvisited();
            let allowance = governor.remaining();
            let mut provenance = Self::provenance(request, &frame, allowance, Some(&stop))?;
            let inference = matches!(&stop, NativeContextualUpstreamStop::InferenceExhausted);
            let blocked = matches!(&stop, NativeContextualUpstreamStop::Blocked { .. });
            provenance.consumed_budget = BudgetUsage {
                consumed: 0,
                allowance,
                limit: inference.then_some(BudgetLimit::Inference),
            };
            let preservation = PreservationClaim::exact();
            provenance.projection_class = preservation.clone();
            provenance.certified_fragment = Some(FRAGMENT.into());
            provenance.assumptions.insert(Assumption::OpenWorld);
            let assessment = super::ContextualAssessment {
                request: request.identity.clone(),
                formula: request.formula.clone(),
                result: ReasoningResult::new(
                    InputStatus::Valid,
                    if blocked {
                        EvaluationStatus::Unsupported
                    } else {
                        EvaluationStatus::BudgetExhausted
                    },
                    CompletenessStatus::Incomplete,
                    preservation,
                    if blocked {
                        InformationState::NotEvaluated
                    } else {
                        InformationState::Undetermined
                    },
                    provenance,
                    ResultPayload::Empty,
                ),
                interrupted: inference.then_some(crate::runtime::IncompleteCause::StepBudget),
                inferences: Vec::new(),
                anchors: Vec::new(),
                temporal_prefixes: Vec::new(),
                native_evidence: Vec::new(),
                diagnostics: gmeow_errors::DiagLedger::new(),
            };
            Self::publish_assessment(
                request,
                &frame,
                worlds,
                assessment,
                Some(stop.clone()),
                &mut batch,
            )?;
            state.evaluated.insert(index);
        }
        Ok(batch)
    }

    fn provenance(
        request: &Request,
        frame: &frame::NativeFrame<'_>,
        allowance: Option<u64>,
        stop: Option<&NativeContextualUpstreamStop>,
    ) -> gmeow_errors::Result<crate::result::ResultProvenance> {
        let coordinates = frame.context(&request.selected).map_err(diagnostic)?;
        let basis = serde_json::to_vec(&(
            FRAGMENT,
            crate::runtime::EngineContract::current().descriptor_hash,
            &request.formula_key,
            frame.basis_digest(),
            &request.selected,
            coordinates,
            allowance,
            stop,
        ))
        .expect("native contextual request contract serializes");
        let mut provenance = crate::result::ResultProvenance::native(
            blake3::hash(&basis).to_hex().to_string(),
            &coordinates.world,
        );
        provenance.query.clone_from(&request.formula);
        provenance.conclusion.clone_from(&request.formula);
        provenance.context.standpoint = Some(coordinates.standpoint.clone());
        provenance.context.attributed = Some(request.selected.clone());
        Ok(provenance)
    }

    fn evaluate_one(
        request: &Request,
        frame: &frame::NativeFrame<'_>,
        worlds: &mut BTreeMap<&str, NativeWorldSnapshot<'_>>,
        governor: &mut StepGovernor,
        batch: &mut NativeContextualBatch,
    ) -> gmeow_errors::Result<()> {
        let allowance = governor.remaining();
        let provenance = Self::provenance(request, frame, allowance, None)?;
        let evaluation = request
            .program
            .evaluate(frame, &request.selected, allowance, None)
            .map_err(|error| diagnostic(error).with_focus(&request.identity))?;
        let assessment = finish_assessment(
            frame,
            request.identity.clone(),
            request.formula.clone(),
            provenance,
            allowance,
            evaluation,
        )?;
        let consumed = assessment.result.provenance.consumed_budget.consumed;
        if allowance.is_some_and(|allowance| consumed > allowance) {
            return Err(diagnostic(super::malformed(
                "contextual producer exceeded its shared allowance",
            )));
        }
        Self::publish_assessment(request, frame, worlds, assessment, None, batch)?;
        governor.consumed = governor.consumed.saturating_add(consumed);
        Ok(())
    }

    fn publish_assessment(
        request: &Request,
        frame: &frame::NativeFrame<'_>,
        worlds: &mut BTreeMap<&str, NativeWorldSnapshot<'_>>,
        assessment: super::ContextualAssessment,
        upstream_stop: Option<NativeContextualUpstreamStop>,
        batch: &mut NativeContextualBatch,
    ) -> gmeow_errors::Result<()> {
        let input_contract = *worlds
            .get(crate::result_rdf::GRAPH_REASONING)
            .ok_or_else(|| {
                diagnostic(super::malformed("contextual output world was not admitted"))
            })?
            .input()
            .input_contract();
        let mut premises: BTreeSet<_> = request.metadata.iter().cloned().collect();
        premises.extend(frame.support.iter().cloned());
        let premises = premises.into_iter().collect::<Vec<_>>();
        let mut supports = Vec::new();
        for (world, statement) in &premises {
            let snapshot = worlds.get_mut(world.as_str()).ok_or_else(|| {
                diagnostic(super::malformed("contextual premise world is absent"))
            })?;
            if snapshot.input().input_contract() != &input_contract {
                return Err(diagnostic(super::malformed(
                    "contextual worlds carry different native contracts",
                )));
            }
            let fact = Fact {
                subject: statement.subject.clone(),
                predicate: statement.predicate.clone(),
                object: statement.object.clone(),
            };
            let proof = snapshot.support(&[fact])?;
            let [proof] = proof.as_slice() else {
                return Err(diagnostic(super::malformed(
                    "contextual source lacks one exact native proof",
                )));
            };
            supports.push(NativeModalSupport {
                world: world.clone(),
                proof: *proof,
            });
        }
        let receipt = Arc::new(NativeContextualReceipt::new(
            input_contract,
            assessment,
            premises,
            supports,
            upstream_stop,
        )?);
        for statement in &receipt.statements {
            let head = Fact {
                subject: statement.subject.clone(),
                predicate: statement.predicate.clone(),
                object: statement.object.clone(),
            };
            batch.candidates.push(NativeContextualCandidate {
                owner: crate::result_rdf::GRAPH_REASONING.into(),
                head,
                receipt: Arc::clone(&receipt),
            });
        }
        batch.interrupted |= receipt.judgment.interrupted;
        batch.receipts.push(receipt);
        Ok(())
    }
}
