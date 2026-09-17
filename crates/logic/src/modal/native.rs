// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Fixed modal producers in the shared native dependency graph. Original frame
//! admission and indexed world reads replace post-closure dataset reconstruction.

use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use purrdf::TermValue;

use crate::physical::{
    CompletedWorldReads, ProducerEffect, ReadDependency, StatementPattern, WorldProducerEffect,
    WorldStatementObservation,
};
use crate::physical::{LogicalGraph, NativeWorldSnapshot, WitnessStatement};
use crate::reason::refute::{NativeProofId, NativeProofNode, RefutationPremise};
use crate::rule_ir::{EvalTerm, Fact};

use super::{
    ATOM_OBJECT, ATOM_PREDICATE, ATOM_SUBJECT, MODAL_COUNTEREXAMPLE_WORLD, MODAL_EVAL_WORLD,
    MODAL_NECESSITY_FAILS, MODAL_NECESSITY_HOLDS, MODAL_NECESSITY_UNDETERMINED,
    MODAL_POSSIBILITY_FAILS, MODAL_POSSIBILITY_HOLDS, ModalEvaluation, ModalFact, ModalFrame,
    ModalFrontier, ModalOp, ModalWorldEvidence, NECESSARILY, OVER_ACCESSIBILITY, POSSIBLY,
    evaluate_frame, iri_binding, modal_err, prepare_frames,
};

/// One exact native proof occurrence. Its world is independent of the graph
/// receiving the modal conclusion.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeModalSupport {
    /// Actual execution world owning this proof node.
    pub world: String,
    /// Scope-bound native proof identity in that world's ledger.
    pub proof: NativeProofId,
}

/// Cross-world runtime evidence. Contextual metadata shares one receipt across
/// its complete publication; the serialized ledger owns that receipt once.
#[derive(Debug, Clone)]
pub(crate) enum NativeCrossWorldEvidence {
    Modal(Box<NativeModalEvidence>),
    Contextual(Arc<super::contextual::native::NativeContextualReceipt>),
}

/// The semantic producer owns publication cost, never a caller-supplied number.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum NativePublicationCost {
    Derivation,
    /// Judgment work is already charged. All metadata for this receipt must be
    /// committed atomically, including a budget-exhausted partial assessment.
    ContextualMetadata {
        receipt: [u8; 32],
    },
}

impl NativeCrossWorldEvidence {
    pub(crate) fn fixed_modal(&self) -> Option<&NativeModalEvidence> {
        match self {
            Self::Modal(evidence) => Some(evidence),
            Self::Contextual(_) => None,
        }
    }
    pub(crate) fn publication_cost(&self) -> NativePublicationCost {
        match self {
            Self::Modal(_) => NativePublicationCost::Derivation,
            Self::Contextual(receipt) => NativePublicationCost::ContextualMetadata {
                receipt: receipt.id,
            },
        }
    }
    pub(crate) fn presentation_premises(&self) -> Vec<(String, String, String)> {
        match self {
            Self::Modal(evidence) => evidence
                .evaluation
                .positive_premises()
                .into_iter()
                .map(|premise| (premise.subject, premise.predicate, premise.object))
                .collect(),
            Self::Contextual(receipt) => receipt.judgment.presentation_premises(),
        }
    }
}

/// Native support for one governed modal firing, including the completed finite
/// predecessor. This certifies the selected evaluation, never an unrestricted rewrite.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeModalEvidence {
    /// Actual source/definition contract of the shared native execution.
    pub input_contract: [u8; 32],
    /// Full context-preserving bounded judgment.
    pub evaluation: ModalEvaluation,
    /// Exact frame, edge and positive atom support, in evaluation premise order.
    pub supports: Vec<NativeModalSupport>,
}

impl NativeModalEvidence {
    /// Validate the local conclusion and the shape of its cross-world references.
    /// The enclosing native execution must also validate the referenced proofs.
    ///
    /// # Errors
    /// Rejects a changed owner/head, invalid evaluation or omitted premise scope.
    pub fn validate_structure(
        &self,
        owner: &str,
        head: &WitnessStatement,
    ) -> gmeow_errors::Result<()> {
        self.evaluation.validate()?;
        let evaluation = &self.evaluation;
        if owner != evaluation.context
            || head.subject.as_iri() != Some(evaluation.formula.as_str())
            || head.predicate != evaluation.conclusion_predicate
            || head.object.as_iri() != Some(evaluation.conclusion_object.as_str())
        {
            return Err(modal_err(
                "native modal receipt changes its owning conclusion".into(),
            ));
        }
        let premises = expected_premises(evaluation);
        if premises.len() != self.supports.len()
            || premises
                .iter()
                .zip(&self.supports)
                .any(|((world, _), support)| *world != support.world)
        {
            return Err(modal_err(
                "native modal receipt omits or reroutes a positive premise".into(),
            ));
        }
        Ok(())
    }

    /// Validate world-qualified references against the enclosing run's shared
    /// proof index. No source scan, source assertion or inference occurs here.
    pub(crate) fn validate_supports(
        &self,
        input_contract: &[u8; 32],
        proofs: &BTreeMap<(&str, NativeProofId), &NativeProofNode>,
    ) -> gmeow_errors::Result<()> {
        if self.input_contract != *input_contract {
            return Err(modal_err(
                "native modal support belongs to a different execution contract".into(),
            ));
        }
        let premises = expected_premises(&self.evaluation);
        if premises.len() != self.supports.len() {
            return Err(modal_err(
                "native modal support omits an expected premise".into(),
            ));
        }
        for ((world, statement), support) in premises.into_iter().zip(&self.supports) {
            if support.world != world {
                return Err(modal_err(
                    "native modal support changes a premise world".into(),
                ));
            }
            let proof = proofs.get(&(world, support.proof)).ok_or_else(|| {
                modal_err("native modal support names an absent world proof".into())
            })?;
            if proof.statement != statement {
                return Err(modal_err(
                    "native modal support proves a different statement".into(),
                ));
            }
        }
        Ok(())
    }
}

/// Typed premise values come directly from the admitted evaluation fields. No
/// RDF lexical serialization or parsing is involved in execution support.
fn expected_premises(evaluation: &ModalEvaluation) -> Vec<(&str, WitnessStatement)> {
    let mut premises = Vec::new();
    let mut push = |world, subject: &str, predicate: &str, object: &str| {
        premises.push((
            world,
            WitnessStatement {
                subject: TermValue::iri(subject),
                predicate: predicate.to_owned(),
                object: TermValue::iri(object),
            },
        ));
    };
    let owner = evaluation.context.as_str();
    push(
        owner,
        &evaluation.formula,
        match evaluation.operator {
            ModalOp::Box => NECESSARILY,
            ModalOp::Diamond => POSSIBLY,
        },
        &evaluation.body,
    );
    push(
        owner,
        &evaluation.formula,
        OVER_ACCESSIBILITY,
        &evaluation.accessibility_relation,
    );
    push(
        owner,
        &evaluation.formula,
        MODAL_EVAL_WORLD,
        &evaluation.evaluation_world,
    );
    push(
        owner,
        &evaluation.body,
        ATOM_SUBJECT,
        &evaluation.atom_subject,
    );
    push(
        owner,
        &evaluation.body,
        ATOM_PREDICATE,
        &evaluation.atom_predicate,
    );
    push(
        owner,
        &evaluation.body,
        ATOM_OBJECT,
        &evaluation.atom_object,
    );
    let ModalFrontier::CompletedFinitePredecessor { worlds } = &evaluation.frontier;
    for world in worlds {
        push(
            owner,
            &evaluation.evaluation_world,
            &evaluation.accessibility_relation,
            &world.world,
        );
        if world.atom_present {
            push(
                &world.world,
                &evaluation.atom_subject,
                &evaluation.atom_predicate,
                &evaluation.atom_object,
            );
        }
    }
    premises
}

pub(crate) struct NativeModalCandidate {
    pub(crate) owner: String,
    pub(crate) head: Fact,
    pub(crate) evidence: NativeModalEvidence,
}

/// Original, explicitly selected modal frames plus their admitted finite world
/// index. Empty referenced worlds are real empty stores, never the default graph.
pub(crate) struct NativeModalProgram {
    frames: Vec<ModalFrame>,
    worlds: BTreeSet<String>,
    additional_worlds: BTreeMap<String, LogicalGraph>,
    effects: Vec<WorldProducerEffect>,
    possible_heads: Arc<[(String, Fact)]>,
}

struct SourceFact<'a> {
    world: &'a str,
    source: &'a RefutationPremise,
    subject_text: std::cell::OnceCell<String>,
}
impl ModalFact for SourceFact<'_> {
    fn graph(&self) -> &str {
        self.world
    }
    fn subject(&self) -> &str {
        self.source.subject.as_iri().unwrap_or_else(|| {
            self.subject_text
                .get_or_init(|| crate::provenance::term_display(&self.source.subject))
        })
    }
    fn predicate(&self) -> &str {
        &self.source.predicate
    }
    fn object(&self) -> Cow<'_, str> {
        self.source.object.as_iri().map_or_else(
            || Cow::Owned(crate::provenance::term_display(&self.source.object)),
            Cow::Borrowed,
        )
    }
}

fn statement(subject: &str, predicate: &str, object: Option<&str>) -> StatementPattern {
    StatementPattern::statement(&[
        EvalTerm::named(subject),
        EvalTerm::named(predicate),
        object.map_or_else(|| EvalTerm::var("modal_value"), EvalTerm::named),
    ])
}

const DEFINITION_PREDICATES: [&str; 7] = [
    NECESSARILY,
    POSSIBLY,
    OVER_ACCESSIBILITY,
    MODAL_EVAL_WORLD,
    ATOM_SUBJECT,
    ATOM_PREDICATE,
    ATOM_OBJECT,
];

/// Fixed cells that identify modal source grammar. Registering them in the
/// shared value-flow universe lets immutable-source admission compare exact
/// protected roles with source-refined producer ranges.
pub(crate) fn definition_vocabulary() -> impl Iterator<Item = StatementPattern> {
    DEFINITION_PREDICATES
        .into_iter()
        .map(|predicate| StatementPattern::relation(Some(predicate), None))
}

fn conclusion_predicates(frame: &ModalFrame) -> &'static [&'static str] {
    match frame.op {
        ModalOp::Box if frame.relation == super::DEONTICALLY_IDEAL => &[
            MODAL_NECESSITY_HOLDS,
            MODAL_NECESSITY_FAILS,
            MODAL_NECESSITY_UNDETERMINED,
            MODAL_COUNTEREXAMPLE_WORLD,
        ],
        ModalOp::Box => &[
            MODAL_NECESSITY_HOLDS,
            MODAL_NECESSITY_FAILS,
            MODAL_COUNTEREXAMPLE_WORLD,
        ],
        ModalOp::Diamond => &[MODAL_POSSIBILITY_HOLDS, MODAL_POSSIBILITY_FAILS],
    }
}

impl NativeModalProgram {
    pub(crate) fn prepare(
        sources: &BTreeMap<String, Arc<[RefutationPremise]>>,
    ) -> gmeow_errors::Result<Self> {
        let frames = prepare_frames(&|| {
            sources.iter().flat_map(|(world, facts)| {
                facts.iter().map(move |source| SourceFact {
                    world,
                    source,
                    subject_text: std::cell::OnceCell::new(),
                })
            })
        })?;
        let mut worlds: BTreeSet<_> = sources.keys().cloned().collect();
        let mut additional_worlds = BTreeMap::new();
        let mut admit = |world: String| {
            if worlds.insert(world.clone()) {
                additional_worlds.insert(world.clone(), LogicalGraph::Named(TermValue::iri(world)));
            }
        };
        for frame in &frames {
            admit(frame.w0.clone());
            for source in sources[&frame.context].iter().filter(|source| {
                source.subject.as_iri() == Some(frame.w0.as_str())
                    && source.predicate == frame.relation
            }) {
                let target = source.object.as_iri().ok_or_else(|| {
                    modal_err("selected modal accessibility endpoint must be an IRI".into())
                })?;
                admit(iri_binding(
                    target,
                    "typed accessibility edge target world",
                )?);
            }
        }
        let mut program = Self {
            frames,
            worlds,
            additional_worlds,
            effects: Vec::new(),
            possible_heads: Arc::from([]),
        };
        program.effects = program.build_effects();
        program.possible_heads = program.build_possible_heads().into();
        Ok(program)
    }

    /// Add these exact empty graph identities before binding the native contract.
    /// This selects no nonempty object domain or other intrinsic authority.
    pub(crate) fn required_worlds(&self) -> &BTreeMap<String, LogicalGraph> {
        &self.additional_worlds
    }

    /// Frame grammar is original input. Reachable writes to these roles must
    /// pass the shared immutable-definition admission before execution, including
    /// writes that would otherwise introduce a silently unselected new frame.
    pub(crate) fn definition_patterns(&self) -> Vec<WorldStatementObservation> {
        self.worlds
            .iter()
            .flat_map(|world| {
                DEFINITION_PREDICATES
                    .into_iter()
                    .map(move |predicate| WorldStatementObservation {
                        world: world.clone(),
                        pattern: StatementPattern::relation(Some(predicate), None),
                    })
            })
            .collect()
    }

    /// Finite abstract heads for the shared source-bound value-flow analysis.
    /// These are possibilities, never asserted or inserted into native stores.
    pub(crate) fn possible_heads(&self) -> Arc<[(String, Fact)]> {
        Arc::clone(&self.possible_heads)
    }

    fn build_possible_heads(&self) -> Vec<(String, Fact)> {
        let mut heads = Vec::new();
        for frame in &self.frames {
            for predicate in conclusion_predicates(frame) {
                if *predicate == MODAL_COUNTEREXAMPLE_WORLD {
                    heads.extend(self.worlds.iter().map(|world| {
                        (
                            frame.context.clone(),
                            Fact {
                                subject: TermValue::iri(&frame.formula),
                                predicate: (*predicate).into(),
                                object: TermValue::iri(world),
                            },
                        )
                    }));
                } else {
                    heads.push((
                        frame.context.clone(),
                        Fact {
                            subject: TermValue::iri(&frame.formula),
                            predicate: (*predicate).into(),
                            object: TermValue::iri(&frame.body),
                        },
                    ));
                }
            }
        }
        heads
    }

    pub(crate) fn effects(&self) -> &[WorldProducerEffect] {
        &self.effects
    }

    fn build_effects(&self) -> Vec<WorldProducerEffect> {
        self.frames
            .iter()
            .enumerate()
            .map(|(index, frame)| {
                let predicates = conclusion_predicates(frame);
                let writes = predicates
                    .iter()
                    .map(|predicate| {
                        statement(
                            &frame.formula,
                            predicate,
                            (*predicate != MODAL_COUNTEREXAMPLE_WORLD)
                                .then_some(frame.body.as_str()),
                        )
                    })
                    .collect();
                let mut reads = vec![(
                    statement(&frame.w0, &frame.relation, None),
                    ReadDependency::Completed,
                )];
                let mut read_worlds = vec![frame.context.clone()];
                for world in &self.worlds {
                    reads.push((
                        statement(&frame.atom_s, &frame.atom_p, Some(&frame.atom_o)),
                        ReadDependency::Completed,
                    ));
                    read_worlds.push(world.clone());
                }
                let name = format!("native-modal:{index}:{}", frame.formula);
                WorldProducerEffect {
                    owner: frame.context.clone(),
                    effect: ProducerEffect::new(name, writes, reads).requiring_source_admission(),
                    read_worlds,
                }
            })
            .collect()
    }

    /// Evaluate only effects admitted by the global schedule against frozen native
    /// stores. Proof recording mutates ledgers only; the shared round owns commits.
    pub(crate) fn evaluate(
        &self,
        indices: &[usize],
        worlds: &mut BTreeMap<&str, NativeWorldSnapshot<'_>>,
        completed: &CompletedWorldReads,
    ) -> gmeow_errors::Result<Vec<NativeModalCandidate>> {
        let mut candidates = Vec::new();
        for index in indices {
            let frame = self
                .frames
                .get(*index)
                .ok_or_else(|| modal_err("unknown selected native modal effect".into()))?;
            let input_contract = *worlds
                .get(frame.context.as_str())
                .ok_or_else(|| modal_err("native modal owner has no admitted store".into()))?
                .input()
                .input_contract();
            if !completed.covers(&self.effects[*index]) {
                return Err(modal_err(
                    "native modal evaluation lacks its exact scheduled read completion".into(),
                ));
            }
            let mut reached = BTreeSet::new();
            let owner = worlds[frame.context.as_str()].input();
            for row in owner.store.facts_for_predicate(&frame.relation) {
                let fact = &owner.store.facts()[*row];
                if fact.subject.as_iri() != Some(frame.w0.as_str()) {
                    continue;
                }
                let endpoint = fact.object.as_iri().ok_or_else(|| {
                    modal_err("derived modal accessibility endpoint must be an IRI".into())
                })?;
                if !self.worlds.contains(endpoint) {
                    return Err(modal_err(
                        "derived modal edge leaves the admitted finite world index".into(),
                    ));
                }
                reached.insert(endpoint);
            }
            let observations = reached
                .into_iter()
                .map(|world| {
                    let input = worlds[world].input();
                    ModalWorldEvidence {
                        world: world.to_owned(),
                        atom_present: input.rel.contains(
                            &frame.atom_p,
                            &TermValue::iri(&frame.atom_s),
                            &TermValue::iri(&frame.atom_o),
                        ),
                    }
                })
                .collect();
            for verdict in evaluate_frame(frame, observations)? {
                let mut supports = Vec::new();
                for (world, statement) in expected_premises(&verdict.evaluation) {
                    let snapshot = worlds
                        .get_mut(world)
                        .ok_or_else(|| modal_err("modal premise lost its native world".into()))?;
                    if snapshot.input().input_contract() != &input_contract {
                        return Err(modal_err(
                            "native modal worlds carry different execution contracts".into(),
                        ));
                    }
                    let fact = Fact {
                        subject: statement.subject,
                        predicate: statement.predicate,
                        object: statement.object,
                    };
                    let ids = snapshot.support(&[fact])?;
                    let [proof] = ids.as_slice() else {
                        return Err(modal_err(
                            "native modal premise lacks one exact proof".into(),
                        ));
                    };
                    supports.push(NativeModalSupport {
                        world: world.to_owned(),
                        proof: *proof,
                    });
                }
                let head = Fact {
                    subject: TermValue::iri(verdict.subject),
                    predicate: verdict.predicate,
                    object: TermValue::iri(verdict.object),
                };
                let evidence = NativeModalEvidence {
                    input_contract,
                    evaluation: verdict.evaluation,
                    supports,
                };
                evidence.validate_structure(&verdict.graph, &WitnessStatement::from(&head))?;
                candidates.push(NativeModalCandidate {
                    owner: verdict.graph,
                    head,
                    evidence,
                });
            }
        }
        Ok(candidates)
    }
}

#[path = "native.tests.rs"]
#[cfg(test)]
mod tests;
