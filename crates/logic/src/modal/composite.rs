// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Physical lowering of the shared FOL standard translation to a finite modal DAG.
//!
//! This is an execution plan, never a second authored or serialized formula IR.
//! The world binder is admitted structurally, before evaluation, so an ordinary
//! first-order quantifier cannot silently become a finite-world quantifier.

mod monitor_cache;
mod temporal_eval;
mod temporal_lower;
#[cfg(test)]
mod tests;

use std::collections::BTreeMap;

use gmeow_logic_compile::ir::{ContentKey, Formula, Term};

use super::{DEONTICALLY_IDEAL, TYPED_ACCESSIBILITY};
use crate::physical::StepGovernor;
use crate::provenance::mint_derivation_id;
use crate::runtime::IncompleteCause;
use purrdf::sparql::{StopCause, StopSignal};

pub(super) use monitor_cache::JudgmentCache;

/// The physical address of an already-admitted child instruction.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
pub(super) struct NodeId(usize);

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
pub(super) enum Quantifier {
    Every,
    Some,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
pub(super) enum TemporalOperator {
    Next,
    Eventually,
    Globally,
}

/// A finite context binder may also occur in an atom's subject or object slot.
/// It resolves to the selected context IRI before evidence lookup; other free
/// individual variables remain outside this fragment.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(super) enum AtomTerm {
    Ground(Term),
    Context,
}

impl AtomTerm {
    fn lower(term: &Term, context: &Term, literal: bool) -> Result<Self, AdmissionError> {
        if term == context {
            Ok(Self::Context)
        } else if matches!(term, Term::Iri(_)) || (literal && matches!(term, Term::Literal(_))) {
            Ok(Self::Ground(term.clone()))
        } else {
            Err(AdmissionError::OutsideFragment(
                "finite modal atoms require ground RDF terms or their bound context".into(),
            ))
        }
    }

    fn resolve<'a>(&'a self, context: &str) -> std::borrow::Cow<'a, Term> {
        match self {
            Self::Ground(term) => std::borrow::Cow::Borrowed(term),
            Self::Context => std::borrow::Cow::Owned(Term::Iri(context.into())),
        }
    }
}

/// One typed accessibility axis, selected from the compiler's closed vocabulary.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
pub(super) struct Accessibility(usize);

impl Accessibility {
    pub(super) fn parse(iri: &str) -> Option<Self> {
        TYPED_ACCESSIBILITY
            .iter()
            .position(|known| *known == iri)
            .map(Self)
    }

    pub(super) fn iri(self) -> &'static str {
        TYPED_ACCESSIBILITY[self.0]
    }

    pub(super) fn requires_successor(self) -> bool {
        self.iri() == DEONTICALLY_IDEAL
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(super) enum Instruction {
    AtContext {
        context: String,
        body: NodeId,
    },
    Atom {
        relation: String,
        subject: AtomTerm,
        object: AtomTerm,
    },
    Not(NodeId),
    And(Vec<NodeId>),
    Or(Vec<NodeId>),
    Implies(NodeId, NodeId),
    Iff(NodeId, NodeId),
    Modal {
        quantifier: Quantifier,
        axis: Accessibility,
        body: NodeId,
    },
    Temporal {
        operator: TemporalOperator,
        body: NodeId,
    },
    Until {
        left: NodeId,
        right: NodeId,
    },
}

/// Refusal is separate from a negative or incomplete semantic evaluation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum AdmissionError {
    Malformed(String),
    OutsideFragment(String),
    JournalLimit { limit: usize },
}

/// Immutable child-before-parent instructions compiled from one canonical formula.
#[derive(Debug)]
pub(super) struct Program {
    pub(super) instructions: Vec<Instruction>,
    pub(super) root: NodeId,
    pub(super) formula_key: ContentKey,
    selected_context: String,
}

impl Program {
    pub(super) fn lower(formula: &Formula, world: &str) -> Result<Self, AdmissionError> {
        let selected_world = world.to_owned();
        let world = Term::iri(world)
            .map_err(|error| AdmissionError::Malformed(error.message().to_owned()))?;
        let mut instructions = Vec::new();
        let mut intern = BTreeMap::new();
        let root = lower(formula, &world, &mut instructions, &mut intern)?;
        Ok(Self {
            instructions,
            root,
            formula_key: formula.content_key(),
            selected_context: selected_world,
        })
    }
}

fn lower(
    formula: &Formula,
    world: &Term,
    instructions: &mut Vec<Instruction>,
    intern: &mut BTreeMap<String, NodeId>,
) -> Result<NodeId, AdmissionError> {
    // Admit and erase only the selected world slot before interning. Shared
    // subexpressions at different modal depths share an instruction; their
    // evaluations remain indexed by the actual selected context.
    let explicit_context = match formula {
        Formula::Atom { args, .. } if args.len() == 3 => args.first(),
        Formula::Forall { body, .. } | Formula::Exists { body, .. } => modal_guard(formula, body)
            .ok()
            .and_then(|(_, guard, _)| match guard {
                Formula::Atom { args, .. } if args.len() == 2 => args.first(),
                _ => None,
            }),
        _ => None,
    }
    .filter(|selected| matches!(selected, Term::Iri(_)) && *selected != world);
    let instruction = if let Some(Term::Iri(context)) = explicit_context {
        Instruction::AtContext {
            context: context.clone(),
            body: lower(formula, &Term::Iri(context.clone()), instructions, intern)?,
        }
    } else {
        match formula {
            Formula::Atom {
                relation: Term::Iri(relation),
                args,
            } if args.len() == 3 && &args[0] == world => Instruction::Atom {
                relation: relation.clone(),
                subject: AtomTerm::lower(&args[1], world, false)?,
                object: AtomTerm::lower(&args[2], world, true)?,
            },
            Formula::Atom { .. } => {
                return Err(AdmissionError::OutsideFragment(
                    "finite modal atoms must be world-indexed binary predicates".into(),
                ));
            }
            Formula::Not(body) => Instruction::Not(lower(body, world, instructions, intern)?),
            Formula::And(children) | Formula::Or(children) => {
                if children.len() < 2 {
                    return Err(AdmissionError::Malformed(
                        "a connective requires at least two operands".into(),
                    ));
                }
                // Canonical operand order makes budget frontiers independent of RDF
                // statement order. The data IR intentionally preserves authored order.
                let mut children = children.iter().collect::<Vec<_>>();
                children.sort_by_key(|child| child.content_key());
                let operands = children
                    .into_iter()
                    .map(|child| lower(child, world, instructions, intern))
                    .collect::<Result<Vec<_>, _>>()?;
                if matches!(formula, Formula::And(_)) {
                    Instruction::And(operands)
                } else {
                    Instruction::Or(operands)
                }
            }
            Formula::Implies(left, right) => Instruction::Implies(
                lower(left, world, instructions, intern)?,
                lower(right, world, instructions, intern)?,
            ),
            Formula::Iff(left, right) => {
                let (left, right) = if left.content_key() <= right.content_key() {
                    (left, right)
                } else {
                    (right, left)
                };
                Instruction::Iff(
                    lower(left, world, instructions, intern)?,
                    lower(right, world, instructions, intern)?,
                )
            }
            Formula::Forall { vars, body } | Formula::Exists { vars, body } => {
                let (quantifier, guard, body) = modal_guard(formula, body)?;
                if vars.len() != 1 {
                    return Err(AdmissionError::OutsideFragment(
                        "a modal binder binds exactly one world".into(),
                    ));
                }
                let next_world = Term::Var(vars[0].clone());
                if &next_world == world {
                    return Err(AdmissionError::Malformed(
                        "a modal binder captures its source world".into(),
                    ));
                }
                let Formula::Atom {
                    relation: Term::Iri(relation),
                    args,
                } = guard
                else {
                    return Err(AdmissionError::OutsideFragment(
                        "missing typed modal accessibility guard".into(),
                    ));
                };
                if args.as_slice() != [world.clone(), next_world.clone()] {
                    return Err(AdmissionError::OutsideFragment(
                        "the accessibility guard does not bind this source and successor world"
                            .into(),
                    ));
                }
                if temporal_lower::is_guard(relation) {
                    temporal_lower::lower_temporal(
                        quantifier,
                        relation,
                        body,
                        world,
                        &next_world,
                        instructions,
                        intern,
                    )?
                } else {
                    let Some(axis) = Accessibility::parse(relation) else {
                        return Err(AdmissionError::OutsideFragment(
                            "the accessibility guard is not a typed modal relation".into(),
                        ));
                    };
                    Instruction::Modal {
                        quantifier,
                        axis,
                        body: lower(body, &next_world, instructions, intern)?,
                    }
                }
            }
        }
    };
    // Serialize only the admitted leaf terms. Child IDs already represent
    // interned instructions, so this key has no recursively cloned expression.
    let key = serde_json::to_string(&instruction)
        .map_err(|error| AdmissionError::Malformed(error.to_string()))?;
    if let Some(id) = intern.get(&key) {
        return Ok(*id);
    }
    let id = NodeId(instructions.len());
    instructions.push(instruction);
    intern.insert(key, id);
    Ok(id)
}

fn modal_guard<'a>(
    formula: &Formula,
    body: &'a Formula,
) -> Result<(Quantifier, &'a Formula, &'a Formula), AdmissionError> {
    match (formula, body) {
        (Formula::Forall { .. }, Formula::Implies(guard, inner)) => {
            Ok((Quantifier::Every, guard, inner))
        }
        (Formula::Exists { .. }, Formula::And(children)) if children.len() == 2 => {
            let is_guard = |candidate: &Formula| {
                matches!(candidate,
                Formula::Atom { relation: Term::Iri(relation), args }
                if args.len() == 2 && (Accessibility::parse(relation).is_some()
                    || temporal_lower::is_guard(relation)))
            };
            match (is_guard(&children[0]), is_guard(&children[1])) {
                (true, false) => Ok((Quantifier::Some, &children[0], &children[1])),
                (false, true) => Ok((Quantifier::Some, &children[1], &children[0])),
                _ => Err(AdmissionError::OutsideFragment(
                    "ambiguous existential accessibility guard".into(),
                )),
            }
        }
        _ => Err(AdmissionError::OutsideFragment(
            "individual quantification is outside the finite ground modal fragment".into(),
        )),
    }
}

/// Attributed coordinates of one materialized context. A transition selects a
/// complete destination context, rather than discarding its non-world axes.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(super) struct Context {
    pub(super) world: String,
    pub(super) standpoint: String,
    pub(super) enactment: Option<String>,
    pub(super) journal_position: Option<(String, u64)>,
    pub(super) norm_scope: Option<NormScope>,
    pub(super) protocol_scope: Option<ProtocolScope>,
}

impl Context {
    pub(super) fn validate(&self) -> Result<(), AdmissionError> {
        let mut coordinates = vec![self.world.as_str(), self.standpoint.as_str()];
        coordinates.extend(self.enactment.as_deref());
        if let Some((journal, _)) = &self.journal_position {
            if self.enactment.is_none() {
                return Err(AdmissionError::Malformed(
                    "a journal position requires its enactment occurrence".into(),
                ));
            }
            coordinates.push(journal);
        }
        if let Some(scope) = &self.norm_scope {
            coordinates.extend([
                scope.issuer.as_str(),
                scope.bearer.as_str(),
                scope.policy.as_str(),
            ]);
        }
        if let Some(scope) = &self.protocol_scope {
            coordinates.extend([scope.protocol.as_str(), scope.role.as_str()]);
        }
        for coordinate in coordinates {
            Term::iri(coordinate)
                .map_err(|error| AdmissionError::Malformed(error.message().to_owned()))?;
        }
        Ok(())
    }

    pub(super) fn validate_transition(
        &self,
        axis: Accessibility,
        next: &Self,
    ) -> Result<(), AdmissionError> {
        next.validate()?;
        // A typed edge changes only its selected coordinate. An authored
        // inContext modifier is the explicit door to selecting another tuple.
        let standpoint_axis = axis.iri() == TYPED_ACCESSIBILITY[5];
        let temporal_axis = axis.iri() == TYPED_ACCESSIBILITY[3];
        if self.enactment != next.enactment
            || self.norm_scope != next.norm_scope
            || self.protocol_scope != next.protocol_scope
            || (!temporal_axis && self.journal_position != next.journal_position)
            || (!standpoint_axis && self.standpoint != next.standpoint)
            || (standpoint_axis && self.world != next.world)
        {
            return Err(AdmissionError::Malformed(format!(
                "{} transition changes an unselected context coordinate",
                axis.iri()
            )));
        }
        if temporal_axis {
            match (&self.journal_position, &next.journal_position) {
                (None, None) => {}
                (Some((journal, position)), Some((next_journal, next_position)))
                    if journal == next_journal && position < next_position => {}
                _ => {
                    return Err(AdmissionError::Malformed(
                        "temporal transition must advance within the same selected journal".into(),
                    ));
                }
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(super) struct NormScope {
    pub(super) issuer: String,
    pub(super) bearer: String,
    pub(super) policy: String,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(super) struct ProtocolScope {
    pub(super) protocol: String,
    pub(super) role: String,
}

/// Independent support coordinates. The IDs identify actual input or derived
/// witnesses; absence of support does not manufacture a counter-witness.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct Evidence {
    pub(super) support: Option<String>,
    pub(super) opposition: Option<String>,
    pub(super) complete: bool,
}

#[derive(Debug, Clone)]
pub(super) struct Transition {
    pub(super) destination: String,
    pub(super) witness: String,
}

#[derive(Debug, Clone, Default)]
pub(super) struct Successors {
    pub(super) transitions: Vec<Transition>,
    /// A positive, explicit witness that this selected successor inventory is
    /// closed. No witness means an open inventory, even when the vector is empty.
    pub(super) closure_witness: Option<String>,
}

#[derive(Debug, Clone, Copy)]
pub(super) enum TemporalSelection {
    Next,
    Suffix,
}

/// One observed state attached to an authenticated committed position.
#[derive(Debug, Clone)]
pub(super) struct TemporalPoint {
    pub(super) context: String,
    pub(super) witnesses: Vec<String>,
}

/// The exact journal prefix used by a temporal judgment. State observations
/// remain separately attributed; the transition hash covers its defined fields.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct TemporalPrefix {
    /// Content-addressed witness binding this selected prefix and its boundary.
    pub identity: String,
    /// Authored journal IRI.
    pub journal: String,
    /// Owning enactment IRI.
    pub enactment: String,
    /// Selected last committed entry IRI.
    pub head: String,
    /// Explicit open or finalized boundary evidence.
    pub finalized: bool,
    /// Canonical genesis digest, retained for independent prefix inspection.
    pub initial_head: String,
    /// Exact digest established by the selected final committed entry.
    pub head_hash: String,
}

/// An explicitly selected observation of an ordered state path. This record
/// makes no claim that its states are committed operations in a journal.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct PathObservation {
    /// Content address binding the selected input, ordered states and boundary.
    pub identity: String,
    /// Authored path IRI.
    pub path: String,
    /// The world whose state observations were selected.
    pub world: String,
    /// The standpoint supplying positive and opposing situation evidence.
    pub standpoint: String,
    /// Exact content identity of the selected observation inputs.
    pub source_digest: String,
    /// Explicit evidence that no further state belongs to this observation.
    pub finalized: bool,
}

/// Distinct admitted sources of finite position order. Selecting a journal never
/// falls back to a path; each adapter requires its own declared evidence.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "kind", content = "evidence")]
pub enum TemporalBasis {
    /// Hash-verified committed operations with separately attributed states.
    Journal(TemporalPrefix),
    /// An explicit state-path observation, carrying no fabricated commit history.
    Path(PathObservation),
}

impl TemporalBasis {
    pub(super) fn identity(&self) -> &str {
        match self {
            Self::Journal(prefix) => &prefix.identity,
            Self::Path(observation) => &observation.identity,
        }
    }

    pub(super) fn finalized(&self) -> bool {
        match self {
            Self::Journal(prefix) => prefix.finalized,
            Self::Path(observation) => observation.finalized,
        }
    }
}

#[derive(Debug, Clone)]
pub(super) struct TemporalTrace {
    pub(super) points: Vec<TemporalPoint>,
    pub(super) prefix: TemporalBasis,
}

/// Read-only selected evidence. The RDF adapter owns admission and indexing;
/// the evaluator has no way to create worlds, facts, receipts, or journal events.
pub(super) trait Frame {
    fn context(&self, identity: &str) -> Result<&Context, AdmissionError>;
    fn atom(
        &self,
        context: &str,
        relation: &str,
        subject: &Term,
        object: &Term,
    ) -> Result<Evidence, AdmissionError>;
    fn successors(&self, context: &str, axis: Accessibility) -> Result<Successors, AdmissionError>;
    fn temporal(&self, context: &str) -> Result<TemporalTrace, AdmissionError>;
}

/// One content-addressed finite-modal firing and its actual evidence dependencies.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Inference {
    /// The shared provenance minter's derivation IRI.
    pub identity: String,
    /// The applied calculus rule.
    pub rule: String,
    /// The complete attributed-context identity at this firing.
    pub context: String,
    /// Input claim, derived evidence, and assessment-anchor identities consumed.
    pub antecedents: Vec<String>,
}

/// A judgment address in the deterministic physical plan and its selected context.
/// It identifies the assessment being proved; it does not assert its truth.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct AssessmentAnchor {
    /// Content-addressed identity used by inference dependencies.
    pub identity: String,
    /// The shared compiler's canonical formula key.
    pub formula_key: String,
    /// Child-before-parent instruction index in the certified physical plan.
    pub instruction: usize,
    /// The authored attributed context.
    pub context: String,
    /// Digest binding every admitted coordinate of that context.
    pub context_digest: String,
}

#[derive(Debug)]
pub(super) struct Evaluation {
    pub(super) evidence: Evidence,
    pub(super) inferences: Vec<Inference>,
    pub(super) anchors: Vec<AssessmentAnchor>,
    pub(super) consumed: u64,
    pub(super) interrupted: Option<IncompleteCause>,
    pub(super) temporal_prefixes: Vec<TemporalBasis>,
}

enum EvaluationError {
    Admission(AdmissionError),
    Interrupted(IncompleteCause),
}

#[derive(Clone, Copy)]
enum Fold {
    And,
    Or,
    Necessity { closed: bool },
    Possibility { closed: bool },
}

impl Fold {
    fn every(self) -> bool {
        matches!(self, Self::And | Self::Necessity { .. })
    }
    fn closed(self) -> bool {
        !matches!(
            self,
            Self::Necessity { closed: false } | Self::Possibility { closed: false }
        )
    }
    fn rule(self) -> &'static str {
        match self {
            Self::And => "conjunction",
            Self::Or => "disjunction",
            Self::Necessity { .. } => "necessity",
            Self::Possibility { .. } => "possibility",
        }
    }
}

impl From<AdmissionError> for EvaluationError {
    fn from(error: AdmissionError) -> Self {
        Self::Admission(error)
    }
}

impl Program {
    pub(super) fn evaluate(
        &self,
        frame: &impl Frame,
        context: &str,
        max_steps: Option<u64>,
        stop: Option<&dyn StopSignal>,
    ) -> Result<Evaluation, AdmissionError> {
        self.evaluate_with_cache(
            frame,
            context,
            max_steps,
            stop,
            &JudgmentCache::default(),
            0,
        )
        .map(|(evaluation, _)| evaluation)
    }

    /// Reuse only judgments whose non-temporal input has been authenticated as
    /// unchanged by the caller. Temporal judgments and every ancestor of one
    /// are recomputed against the newly observed prefix.
    pub(super) fn evaluate_with_cache(
        &self,
        frame: &impl Frame,
        context: &str,
        max_steps: Option<u64>,
        stop: Option<&dyn StopSignal>,
        cache: &JudgmentCache,
        capacity: usize,
    ) -> Result<(Evaluation, JudgmentCache), AdmissionError> {
        if context != self.selected_context {
            return Err(AdmissionError::Malformed(
                "the compiled query and selected context have different identities".into(),
            ));
        }
        cache.validate_program(self)?;
        let mut evaluator = Evaluator {
            program: self,
            frame,
            governor: StepGovernor::new(max_steps),
            stop,
            memo: cache.memo.clone(),
            inferences: cache.inferences.clone(),
            anchors: cache.anchors.clone(),
            context_anchors: cache.context_anchors.clone(),
            temporal_prefixes: BTreeMap::new(),
        };
        let (evidence, interrupted) = match evaluator.evaluate(self.root, context) {
            Ok(evidence) => (evidence, None),
            Err(EvaluationError::Interrupted(cause)) => (Evidence::default(), Some(cause)),
            Err(EvaluationError::Admission(error)) => return Err(error),
        };
        // A stop between local proof construction and the judgment commit may
        // leave staged proof rows. Publish only judgments charged to the shared
        // governor; completed children survive an interrupted parent.
        evaluator.anchors.retain(|_, anchor| {
            evaluator
                .memo
                .contains_key(&(NodeId(anchor.instruction), anchor.context.clone()))
        });
        evaluator.inferences.retain(|_, inference| {
            inference.antecedents.iter().all(|antecedent| {
                !antecedent.starts_with("urn:gmeow:modal-assessment:")
                    || evaluator.anchors.contains_key(antecedent)
            })
        });
        let cache = JudgmentCache::retain(self, &evaluator, capacity);
        Ok((
            Evaluation {
                evidence,
                interrupted,
                consumed: evaluator.governor.consumed,
                inferences: evaluator.inferences.into_values().collect(),
                anchors: evaluator.anchors.into_values().collect(),
                temporal_prefixes: evaluator.temporal_prefixes.into_values().collect(),
            },
            cache,
        ))
    }
}

struct Evaluator<'a, F> {
    program: &'a Program,
    frame: &'a F,
    governor: StepGovernor,
    stop: Option<&'a dyn StopSignal>,
    memo: BTreeMap<(NodeId, String), Evidence>,
    inferences: BTreeMap<String, Inference>,
    anchors: BTreeMap<String, AssessmentAnchor>,
    context_anchors: BTreeMap<String, String>,
    temporal_prefixes: BTreeMap<String, TemporalBasis>,
}

impl<F: Frame> Evaluator<'_, F> {
    fn admit_context(&mut self, context: &str) -> Result<(), EvaluationError> {
        if !self.context_anchors.contains_key(context) {
            let coordinates = self.frame.context(context)?;
            coordinates.validate()?;
            let identity = serde_json::to_vec(&(context, coordinates))
                .map_err(|error| AdmissionError::Malformed(error.to_string()))?;
            self.context_anchors.insert(
                context.to_owned(),
                blake3::hash(&identity).to_hex().to_string(),
            );
        }
        Ok(())
    }

    fn cancellation_checkpoint(&self) -> Result<(), EvaluationError> {
        if let Some(cause) = self.stop.and_then(StopSignal::poll) {
            return Err(EvaluationError::Interrupted(match cause {
                StopCause::Cancelled => IncompleteCause::Cancelled,
                StopCause::Deadline => IncompleteCause::Deadline,
            }));
        }
        Ok(())
    }

    fn checkpoint(&self) -> Result<(), EvaluationError> {
        self.cancellation_checkpoint()?;
        if self.governor.spent() {
            return Err(EvaluationError::Interrupted(IncompleteCause::StepBudget));
        }
        Ok(())
    }

    fn evaluate(&mut self, node: NodeId, context: &str) -> Result<Evidence, EvaluationError> {
        self.cancellation_checkpoint()?;
        let key = (node, context.to_owned());
        if let Some(evidence) = self.memo.get(&key) {
            return Ok(evidence.clone());
        }
        self.checkpoint()?;
        self.admit_context(context)?;
        let program = self.program;
        let evidence = match &program.instructions[node.0] {
            Instruction::Temporal { operator, body } => {
                self.temporal(node, context, *operator, *body)?
            }
            Instruction::Until { left, right } => self.until(node, context, *left, *right)?,
            Instruction::AtContext {
                context: selected,
                body,
            } => {
                let evidence = self.evaluate(*body, selected)?;
                Evidence {
                    support: evidence.support.map(|witness| {
                        self.infer(node, context, "context-selection-support", vec![witness])
                    }),
                    opposition: evidence.opposition.map(|witness| {
                        self.infer(node, context, "context-selection-opposition", vec![witness])
                    }),
                    complete: evidence.complete,
                }
            }
            Instruction::Atom {
                relation,
                subject,
                object,
            } => {
                let evidence = self.frame.atom(
                    context,
                    relation,
                    &subject.resolve(context),
                    &object.resolve(context),
                )?;
                Evidence {
                    support: evidence
                        .support
                        .map(|witness| self.infer(node, context, "atomic-support", vec![witness])),
                    opposition: evidence.opposition.map(|witness| {
                        self.infer(node, context, "atomic-opposition", vec![witness])
                    }),
                    complete: evidence.complete,
                }
            }
            Instruction::Not(body) => {
                let evidence = self.evaluate(*body, context)?;
                self.negate(node, context, evidence)
            }
            Instruction::And(children) | Instruction::Or(children) => {
                let every = matches!(self.program.instructions[node.0], Instruction::And(_));
                let children = children
                    .iter()
                    .map(|child| self.evaluate(*child, context))
                    .collect::<Result<Vec<_>, _>>()?;
                self.combine(
                    node,
                    context,
                    if every { Fold::And } else { Fold::Or },
                    children,
                    &[],
                )
            }
            Instruction::Implies(left, right) => {
                let left = self.evaluate(*left, context)?;
                let right = self.evaluate(*right, context)?;
                let left = self.negate(node, context, left);
                self.combine(node, context, Fold::Or, vec![left, right], &[])
            }
            Instruction::Iff(left, right) => {
                let left = self.evaluate(*left, context)?;
                let right = self.evaluate(*right, context)?;
                let not_left = self.negate(node, context, left.clone());
                let not_right = self.negate(node, context, right.clone());
                let forward = self.combine(node, context, Fold::Or, vec![not_left, right], &[]);
                let reverse = self.combine(node, context, Fold::Or, vec![not_right, left], &[]);
                self.combine(node, context, Fold::And, vec![forward, reverse], &[])
            }
            Instruction::Modal {
                quantifier,
                axis,
                body,
            } => {
                let (quantifier, axis, body) = (*quantifier, *axis, *body);
                let frame = self.frame;
                let scope = frame.context(context)?.norm_scope.as_ref();
                if axis.requires_successor() && scope.is_none() {
                    return Err(AdmissionError::Malformed(
                        "deontic evaluation requires issuer, bearer, and policy bindings".into(),
                    )
                    .into());
                }
                let mut successors = self.frame.successors(context, axis)?;
                successors.transitions.sort_by(|left, right| {
                    left.destination
                        .cmp(&right.destination)
                        .then_with(|| left.witness.cmp(&right.witness))
                });
                if axis.requires_successor() && successors.transitions.is_empty() {
                    Evidence::default()
                } else {
                    let mut children = Vec::new();
                    let mut witnesses = Vec::new();
                    for transition in successors.transitions {
                        frame
                            .context(context)?
                            .validate_transition(axis, frame.context(&transition.destination)?)?;
                        children.push(self.evaluate(body, &transition.destination)?);
                        witnesses.push(transition.witness);
                    }
                    let closed = successors.closure_witness.is_some();
                    witnesses.extend(successors.closure_witness);
                    // Existential support and universal counter-support can be
                    // witnessed in an open inventory. Their duals require closure.
                    let fold = match quantifier {
                        Quantifier::Every => Fold::Necessity { closed },
                        Quantifier::Some => Fold::Possibility { closed },
                    };
                    self.combine(node, context, fold, children, &witnesses)
                }
            }
        };
        self.checkpoint()?;
        self.governor.charge();
        self.memo.insert(key, evidence.clone());
        Ok(evidence)
    }

    fn negate(&mut self, node: NodeId, context: &str, evidence: Evidence) -> Evidence {
        Evidence {
            support: evidence
                .opposition
                .map(|witness| self.infer(node, context, "strong-negation-support", vec![witness])),
            opposition: evidence.support.map(|witness| {
                self.infer(node, context, "strong-negation-opposition", vec![witness])
            }),
            complete: evidence.complete,
        }
    }

    fn combine(
        &mut self,
        node: NodeId,
        context: &str,
        fold: Fold,
        children: Vec<Evidence>,
        extra: &[String],
    ) -> Evidence {
        let select = |positive: bool, all: bool| {
            let witnesses = children.iter().map(|child| {
                if positive {
                    child.support.as_ref()
                } else {
                    child.opposition.as_ref()
                }
            });
            if all {
                witnesses
                    .map(|witness| witness.cloned())
                    .collect::<Option<Vec<String>>>()
            } else {
                witnesses
                    .flatten()
                    .min()
                    .map(|witness| vec![witness.clone()])
            }
        };
        let support = if matches!(fold, Fold::Necessity { closed: false }) {
            None
        } else {
            select(true, fold.every())
        };
        let opposition = if matches!(fold, Fold::Possibility { closed: false }) {
            None
        } else {
            select(false, !fold.every())
        };
        let rule = fold.rule();
        let mut derive = |side: &str, witnesses: Vec<String>| {
            let mut antecedents = witnesses;
            antecedents.extend_from_slice(extra);
            self.infer(node, context, &format!("{rule}-{side}"), antecedents)
        };
        Evidence {
            support: support.map(|witnesses| derive("support", witnesses)),
            opposition: opposition.map(|witnesses| derive("opposition", witnesses)),
            complete: fold.closed() && children.iter().all(|child| child.complete),
        }
    }

    fn infer(
        &mut self,
        node: NodeId,
        context: &str,
        rule: &str,
        mut antecedents: Vec<String>,
    ) -> String {
        // The conclusion anchor includes the selected context and instruction;
        // equal primitive evidence in two standpoints cannot collapse assessments.
        let anchor = serde_json::to_vec(&(
            self.program.formula_key.to_string(),
            node.0,
            &self.context_anchors[context],
        ))
        .expect("strings and integer serialize");
        let anchor_identity = format!("urn:gmeow:modal-assessment:{}", blake3::hash(&anchor));
        self.anchors
            .entry(anchor_identity.clone())
            .or_insert_with(|| AssessmentAnchor {
                identity: anchor_identity.clone(),
                formula_key: self.program.formula_key.to_string(),
                instruction: node.0,
                context: context.to_owned(),
                context_digest: self.context_anchors[context].clone(),
            });
        antecedents.push(anchor_identity);
        antecedents.sort();
        antecedents.dedup();
        let rule = format!("https://blackcatinformatics.ca/logic/rule/finite-modal/{rule}");
        let refs = antecedents.iter().map(String::as_str).collect::<Vec<_>>();
        let identity = mint_derivation_id(&rule, &refs);
        self.inferences
            .entry(identity.clone())
            .or_insert(Inference {
                identity: identity.clone(),
                rule,
                context: context.to_owned(),
                antecedents,
            });
        identity
    }
}
