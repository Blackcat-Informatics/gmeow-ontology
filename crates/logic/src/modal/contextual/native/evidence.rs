// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! One shared typed receipt per contextual assessment. Proof nodes reference its
//! identity instead of serializing the same metadata graph once for every row.

use super::*;
use crate::contextual::{
    AssessmentAnchor, ContextualAssessment, ContextualInference, NativeEvidence, TemporalBasis,
};
use crate::reason::refute::{NativeProofId, NativeProofNode};
use crate::result::{
    CompletenessStatus, EvaluationStatus, InformationState, InputStatus, PreservationClaim,
    ReasoningResult, ResultPayload, ResultProvenance,
};

/// Exact upstream analysis accounting which prevented a selected query from starting.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeContextualAnalysisStop {
    /// Owning native execution key.
    pub world: String,
    /// Original graph identity, including the selected default graph.
    #[serde(with = "crate::term_serde::optional")]
    pub graph: Option<TermValue>,
    /// Exact analysis usage at the stop; no inference allowance is substituted.
    pub usage: crate::reason::refute::native::NativeAnalysisUsage,
    /// Exact class resource boundaries, including depth cuts that consume no
    /// further analysis units and therefore do not exhaust the shared allowance.
    pub class_resource_obstructions: Vec<crate::reason::refute::native::NativeFamilyObstruction>,
}

/// Why the native graph could not visit a selected contextual producer. This is
/// retained in the mandatory native execution ledger, independently of the
/// contextual query's own inference allowance and coarse result axes.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum NativeContextualUpstreamStop {
    /// Inference work ended, including an atomic cost which did not fit.
    InferenceExhausted,
    /// Independent family analysis ended before required reads completed.
    AnalysisExhausted {
        /// Sorted, unique triggering worlds. Later finalization may exhaust others.
        analyses: Vec<NativeContextualAnalysisStop>,
    },
    /// An upstream capability obstruction withheld these exact completed reads.
    Blocked {
        /// Exact canonical completed-read roles from the native terminal state.
        reads: Vec<crate::reason::refute::native::NativeRead>,
    },
}

impl NativeContextualUpstreamStop {
    pub(super) fn validate(&self) -> gmeow_errors::Result<()> {
        let valid = match self {
            Self::InferenceExhausted => true,
            Self::AnalysisExhausted { analyses } => !analyses.is_empty()
                && analyses.windows(2).all(|pair| pair[0].world < pair[1].world)
                && analyses.iter().all(|analysis| {
                    (analysis.usage.exhausted || analysis.usage.allowance == Some(0)
                        || !analysis.class_resource_obstructions.is_empty())
                        && analysis.class_resource_obstructions.iter().all(|obstruction| {
                            obstruction.kind == crate::reason::refute::native::NativeObstructionKind::ResourceLimit
                        })
                        && !analysis.usage.allowance.is_some_and(|limit| analysis.usage.consumed > limit)
                        && LogicalGraph::from_graph(analysis.graph.clone()).world().is_ok_and(|world| world == analysis.world)
                }),
            Self::Blocked { reads } => !reads.is_empty()
                && reads.windows(2).all(|pair| pair[0] < pair[1])
                && reads.iter().all(|read| read.kind == crate::reason::refute::native::NativeReadKind::Completed),
        };
        if !valid {
            return Err(diagnostic(super::super::malformed(
                "contextual upstream stop has no exact native cause",
            )));
        }
        Ok(())
    }
}

/// A selected contextual judgment, independent of its terminal RDF projection.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeContextualJudgment {
    request: String,
    formula: String,
    evaluation: EvaluationStatus,
    completeness: CompletenessStatus,
    preservation: PreservationClaim,
    information: InformationState,
    provenance: ResultProvenance,
    /// A query's own operational interruption is the shared inference allowance.
    pub(crate) interrupted: bool,
    upstream_stop: Option<NativeContextualUpstreamStop>,
    inferences: Vec<ContextualInference>,
    anchors: Vec<AssessmentAnchor>,
    temporal_prefixes: Vec<TemporalBasis>,
    native_evidence: Vec<NativeEvidence>,
}

impl NativeContextualJudgment {
    pub(super) fn from_assessment(
        assessment: ContextualAssessment,
        upstream_stop: Option<NativeContextualUpstreamStop>,
    ) -> gmeow_errors::Result<Self> {
        assessment.result.validate()?;
        if assessment.result.input != InputStatus::Valid
            || !matches!(assessment.result.payload, ResultPayload::Empty)
            || assessment.result.row_schema.is_some()
            || assessment.result.provenance.native_execution.is_some()
            || !assessment
                .result
                .provenance
                .contradiction_witnesses
                .is_empty()
            || !assessment.diagnostics.emit_sorted().is_empty()
            || assessment
                .interrupted
                .is_some_and(|cause| cause != crate::runtime::IncompleteCause::StepBudget)
        {
            return Err(diagnostic(super::super::malformed(
                "native contextual receipt must retain an admitted finite assessment",
            )));
        }
        Ok(Self {
            request: assessment.request,
            formula: assessment.formula,
            evaluation: assessment.result.evaluation,
            completeness: assessment.result.completeness,
            preservation: assessment.result.preservation,
            information: assessment.result.information,
            provenance: assessment.result.provenance,
            interrupted: assessment.interrupted.is_some(),
            upstream_stop,
            inferences: assessment.inferences,
            anchors: assessment.anchors,
            temporal_prefixes: assessment.temporal_prefixes,
            native_evidence: assessment.native_evidence,
        })
    }

    /// Exact terminal cause, authenticated by the enclosing native execution.
    pub fn upstream_stop(&self) -> Option<&NativeContextualUpstreamStop> {
        self.upstream_stop.as_ref()
    }

    fn validate_stop(&self) -> gmeow_errors::Result<()> {
        let invalid = || {
            diagnostic(super::super::malformed(
                "contextual stop changes its unvisited assessment",
            ))
        };
        let Some(stop) = &self.upstream_stop else {
            if (self.evaluation == EvaluationStatus::BudgetExhausted) != self.interrupted
                || self.evaluation == EvaluationStatus::Unsupported
            {
                return Err(invalid());
            }
            return Ok(());
        };
        stop.validate()?;
        let (evaluation, information, interrupted, limit) = match stop {
            NativeContextualUpstreamStop::InferenceExhausted => (
                EvaluationStatus::BudgetExhausted,
                InformationState::Undetermined,
                true,
                Some(crate::result::BudgetLimit::Inference),
            ),
            NativeContextualUpstreamStop::AnalysisExhausted { .. } => (
                EvaluationStatus::BudgetExhausted,
                InformationState::Undetermined,
                false,
                None,
            ),
            NativeContextualUpstreamStop::Blocked { .. } => (
                EvaluationStatus::Unsupported,
                InformationState::NotEvaluated,
                false,
                None,
            ),
        };
        if self.evaluation != evaluation
            || self.information != information
            || self.interrupted != interrupted
            || self.completeness != CompletenessStatus::Incomplete
            || self.provenance.consumed_budget.consumed != 0
            || self.provenance.consumed_budget.limit != limit
            || self.provenance.proof.is_some()
            || self.provenance.counterproof.is_some()
            || !self.inferences.is_empty()
            || !self.anchors.is_empty()
            || !self.temporal_prefixes.is_empty()
            || !self.native_evidence.is_empty()
        {
            return Err(invalid());
        }
        Ok(())
    }

    pub(crate) fn validate_upstream(
        &self,
        status: &crate::reason::refute::native::NativeClosureStatus,
        families: &[crate::reason::refute::native::NativeFamilyLedger],
        classes: &[crate::reason::refute::ClassExecutionOutcome],
    ) -> gmeow_errors::Result<()> {
        use crate::reason::refute::native::NativeClosureStatus;
        let valid = match &self.upstream_stop {
            None => true,
            Some(NativeContextualUpstreamStop::InferenceExhausted) => {
                *status == NativeClosureStatus::Exhausted
            }
            Some(NativeContextualUpstreamStop::AnalysisExhausted { analyses }) => {
                let expected: BTreeMap<_, _> = families
                    .iter()
                    .map(|ledger| (ledger.world.as_str(), ledger))
                    .collect();
                *status == NativeClosureStatus::Exhausted
                    && analyses.iter().all(|analysis| expected.get(analysis.world.as_str()).is_some_and(|ledger| {
                        ledger.graph == analysis.graph
                            && ledger.work.allowance == analysis.usage.allowance
                            && ledger.work.consumed == analysis.usage.consumed
                            // A selected zero allowance already stops unvisited
                            // families; final admission can record its first
                            // failed charge without changing allowance or work.
                            && (!analysis.usage.exhausted || ledger.work.exhausted)
                            && (analysis.class_resource_obstructions.is_empty() || classes.iter().any(|class| {
                                class.world == analysis.world && class.graph == analysis.graph
                                    && class.input_contract == ledger.input_contract
                                    && class.completion == crate::reason::refute::native::NativeFamilyCompletion::Exhausted
                                    && analysis.class_resource_obstructions.iter().all(|obstruction| class.obstructions.contains(obstruction))
                            }))
                    }))
            }
            Some(NativeContextualUpstreamStop::Blocked { reads }) => {
                matches!(status, NativeClosureStatus::Blocked { reads: actual } if reads == actual)
            }
        };
        if !valid {
            return Err(diagnostic(super::super::malformed(
                "contextual stop does not match the actual upstream execution",
            )));
        }
        Ok(())
    }

    pub(crate) fn validate_upstream_allowance(
        &self,
        budget: &crate::result::BudgetUsage,
    ) -> gmeow_errors::Result<()> {
        if let Some(stop) = &self.upstream_stop {
            if self.provenance.consumed_budget.allowance
                != budget
                    .allowance
                    .map(|limit| limit.saturating_sub(budget.consumed))
                || (matches!(stop, NativeContextualUpstreamStop::InferenceExhausted)
                    && budget.limit != Some(crate::result::BudgetLimit::Inference))
            {
                return Err(diagnostic(super::super::malformed(
                    "contextual unvisited allowance changes the actual native inference frontier",
                )));
            }
        }
        Ok(())
    }

    fn assessment(&self) -> ContextualAssessment {
        ContextualAssessment {
            request: self.request.clone(),
            formula: self.formula.clone(),
            result: ReasoningResult::new(
                InputStatus::Valid,
                self.evaluation,
                self.completeness,
                self.preservation.clone(),
                self.information,
                self.provenance.clone(),
                ResultPayload::Empty,
            ),
            interrupted: self
                .interrupted
                .then_some(crate::runtime::IncompleteCause::StepBudget),
            inferences: self.inferences.clone(),
            anchors: self.anchors.clone(),
            temporal_prefixes: self.temporal_prefixes.clone(),
            native_evidence: self.native_evidence.clone(),
            diagnostics: gmeow_errors::DiagLedger::new(),
        }
    }

    pub(crate) fn presentation_premises(&self) -> Vec<(String, String, String)> {
        let mut premises = vec![
            (
                self.request.clone(),
                format!("{LOGIC}queryFormula"),
                format!("<{}>", self.formula),
            ),
            (
                self.request.clone(),
                format!("{LOGIC}queryContext"),
                format!(
                    "<{}>",
                    self.provenance
                        .context
                        .attributed
                        .as_deref()
                        .expect("validated attributed request")
                ),
            ),
        ];
        premises.sort();
        premises
    }
}

/// Full execution evidence shared by every metadata head of one assessment.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeContextualReceipt {
    /// Content address checked against every field before publication or reuse.
    pub id: [u8; 32],
    /// Exact selected native execution contract.
    pub input_contract: [u8; 32],
    /// Typed assessment; its graph is a projection, never another semantic owner.
    pub judgment: NativeContextualJudgment,
    /// Exact ordered world-qualified positive observations used by this receipt.
    pub premises: Vec<(String, WitnessStatement)>,
    /// References into the shared native proof index, in premise order.
    pub supports: Vec<NativeModalSupport>,
    /// The complete canonical metadata publication, sorted and duplicate-free.
    pub statements: Vec<WitnessStatement>,
}

impl NativeContextualReceipt {
    pub(super) fn new(
        input_contract: [u8; 32],
        assessment: ContextualAssessment,
        premises: Vec<(String, WitnessStatement)>,
        supports: Vec<NativeModalSupport>,
        upstream_stop: Option<NativeContextualUpstreamStop>,
    ) -> gmeow_errors::Result<Self> {
        let judgment = NativeContextualJudgment::from_assessment(assessment, upstream_stop)?;
        let mut statements =
            crate::result_rdf::contextual_assessment_facts(&judgment.assessment())?
                .iter()
                .map(WitnessStatement::from)
                .collect::<Vec<_>>();
        statements.sort();
        statements.dedup();
        let mut receipt = Self {
            id: [0; 32],
            input_contract,
            judgment,
            premises,
            supports,
            statements,
        };
        receipt.id = receipt.content_identity();
        receipt.validate()?;
        Ok(receipt)
    }

    fn content_identity(&self) -> [u8; 32] {
        crate::physical::metadata_identity(
            "gmeow-native-contextual-receipt-v1",
            &(
                &self.input_contract,
                &self.judgment,
                &self.premises,
                &self.supports,
                &self.statements,
            ),
        )
    }

    /// Validate once per shared ledger receipt, before any proof reference is read.
    pub fn validate(&self) -> gmeow_errors::Result<()> {
        let invalid = |detail| diagnostic(super::super::malformed(detail));
        let assessment = self.judgment.assessment();
        assessment.result.validate()?;
        self.judgment.validate_stop()?;
        if self.id != self.content_identity()
            || self.premises.len() != self.supports.len()
            || self.premises.windows(2).any(|pair| pair[0] >= pair[1])
            || self
                .premises
                .iter()
                .zip(&self.supports)
                .any(|((world, _), support)| *world != support.world)
            || self.judgment.provenance.context.attributed.is_none()
            || self.judgment.provenance.context.standpoint.is_none()
            || self.judgment.provenance.native_execution.is_some()
            || !self.judgment.provenance.contradiction_witnesses.is_empty()
        {
            return Err(invalid(
                "contextual receipt changes its selected assessment or native support",
            ));
        }
        let mut expected = crate::result_rdf::contextual_assessment_facts(&assessment)?
            .iter()
            .map(WitnessStatement::from)
            .collect::<Vec<_>>();
        expected.sort();
        expected.dedup();
        if self.statements != expected {
            return Err(invalid(
                "contextual receipt changes its canonical metadata publication",
            ));
        }
        Ok(())
    }

    pub(crate) fn validate_head(
        &self,
        owner: &str,
        head: &WitnessStatement,
    ) -> gmeow_errors::Result<()> {
        if owner != crate::result_rdf::GRAPH_REASONING
            || self.statements.binary_search(head).is_err()
        {
            return Err(diagnostic(super::super::malformed(
                "contextual proof changes its owning metadata conclusion",
            )));
        }
        Ok(())
    }

    pub(crate) fn validate_supports(
        &self,
        input_contract: &[u8; 32],
        proofs: &BTreeMap<(&str, NativeProofId), &NativeProofNode>,
    ) -> gmeow_errors::Result<()> {
        if self.input_contract != *input_contract {
            return Err(diagnostic(super::super::malformed(
                "contextual receipt belongs to another native execution",
            )));
        }
        for ((world, statement), support) in self.premises.iter().zip(&self.supports) {
            let proof = proofs
                .get(&(world.as_str(), support.proof))
                .ok_or_else(|| {
                    diagnostic(super::super::malformed(
                        "contextual receipt names an absent native proof",
                    ))
                })?;
            if proof.statement != *statement || support.world != *world {
                return Err(diagnostic(super::super::malformed(
                    "contextual support proves a different world or statement",
                )));
            }
        }
        Ok(())
    }

    pub(crate) fn source_quad_ids(&self) -> Vec<String> {
        self.judgment
            .presentation_premises()
            .iter()
            .map(|(s, p, o)| crate::provenance::reifier_from_strings(s, p, o))
            .collect()
    }
    pub(crate) fn derivation_id(&self) -> String {
        crate::provenance::mint_derivation_id(
            RULE_IRI,
            &self
                .source_quad_ids()
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>(),
        )
    }
}
