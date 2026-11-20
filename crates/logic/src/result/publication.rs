// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Constructor-owned assessments and exact retained-receipt admission.

use std::fmt;
use std::sync::{Arc, OnceLock};

use purrdf_hash::{Domain, blake3};
use serde::{Deserialize, Serialize};

use super::{NativeExecutionEvidence, ReasoningResult, ResultPayload, result_err};
use crate::result_rdf::NativeReceiptAddress;

const ASSESSMENT_DOMAIN: Domain = Domain::new(b"gmeow-native-assessment/v1\0");

/// Generic data transport has exactly the established eight semantic fields.
/// The owned wire has no runtime capability for a derived in-place visitor to retain.
#[derive(Deserialize)]
#[serde(rename = "ReasoningResult")]
struct ReasoningResultWire {
    input: super::InputStatus,
    evaluation: super::EvaluationStatus,
    completeness: super::CompletenessStatus,
    preservation: super::PreservationClaim,
    information: super::InformationState,
    provenance: super::ResultProvenance,
    payload: ResultPayload,
    row_schema: Option<crate::result_shape::ResultShape>,
}

impl<'de> Deserialize<'de> for ReasoningResult {
    fn deserialize<D: serde::Deserializer<'de>>(input: D) -> Result<Self, D::Error> {
        let wire = ReasoningResultWire::deserialize(input)?;
        Ok(Self {
            input: wire.input,
            evaluation: wire.evaluation,
            completeness: wire.completeness,
            preservation: wire.preservation,
            information: wire.information,
            provenance: wire.provenance,
            payload: wire.payload,
            row_schema: wire.row_schema,
            native_authority: OnceLock::new(),
        })
    }
    // Keep Serde's default in-place replacement: a failed owned decode cannot
    // partially change private rows while leaving an earlier Original alive.
}

/// The complete projection-stable proposition, independently of its execution owner.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct AssessmentCommitment(pub(crate) [u8; 32]);

#[derive(Clone)]
pub(super) enum NativeAuthority {
    Original {
        assessment: AssessmentCommitment,
        execution: Arc<NativeExecutionEvidence>,
    },
    ReceiptAdmitted {
        assessment: AssessmentCommitment,
        execution: Arc<NativeExecutionEvidence>,
        receipt: NativeReceiptAddress,
        closure: Option<NativeReceiptAddress>,
    },
}

impl fmt::Debug for NativeAuthority {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        out.write_str(match self {
            Self::Original { .. } => "Original",
            Self::ReceiptAdmitted { .. } => "ReceiptAdmitted",
        })
    }
}

/// Borrow every stable semantic field. Native evidence is bound by its exact Arc
/// separately; including it here would traverse the entire proof DAG again.
#[derive(Serialize)]
struct Assessment<'a> {
    input: &'a super::InputStatus,
    evaluation: &'a super::EvaluationStatus,
    completeness: &'a super::CompletenessStatus,
    preservation: &'a super::PreservationClaim,
    information: &'a super::InformationState,
    provenance: AssessmentProvenance<'a>,
    row_schema: &'a Option<crate::result_shape::ResultShape>,
}

#[derive(Serialize)]
struct AssessmentProvenance<'a> {
    claim: &'a super::ResultClaim,
    contract_hash: &'a str,
    query: &'a str,
    conclusion: &'a str,
    proof: &'a Option<super::DerivationRef>,
    counterproof: &'a Option<super::DerivationRef>,
    context: &'a super::ResultContext,
    engine: &'a super::EngineId,
    consumed_budget: &'a super::BudgetUsage,
    certified_fragment: &'a Option<String>,
    projection_class: &'a super::PreservationClaim,
    contradiction_witnesses: &'a [super::ContradictionWitness],
    assumptions: &'a std::collections::BTreeSet<super::Assumption>,
}

impl ReasoningResult {
    fn assessment_commitment(&self) -> gmeow_errors::Result<AssessmentCommitment> {
        let provenance = &self.provenance;
        let assessment = Assessment {
            input: &self.input,
            evaluation: &self.evaluation,
            completeness: &self.completeness,
            preservation: &self.preservation,
            information: &self.information,
            provenance: AssessmentProvenance {
                claim: &provenance.claim,
                contract_hash: &provenance.contract_hash,
                query: &provenance.query,
                conclusion: &provenance.conclusion,
                proof: &provenance.proof,
                counterproof: &provenance.counterproof,
                context: &provenance.context,
                engine: &provenance.engine,
                consumed_budget: &provenance.consumed_budget,
                certified_fragment: &provenance.certified_fragment,
                projection_class: &provenance.projection_class,
                contradiction_witnesses: &provenance.contradiction_witnesses,
                assumptions: &provenance.assumptions,
            },
            row_schema: &self.row_schema,
        };
        let mut writer = blake3::RecordHasher::new();
        writer.update(ASSESSMENT_DOMAIN.as_bytes());
        ciborium::ser::into_writer(&assessment, &mut writer).map_err(|cause| {
            gmeow_errors::Diag::of_kind(crate::error::Result::caused_by(
                "encode native assessment commitment".into(),
                cause,
            ))
        })?;
        Ok(AssessmentCommitment(*writer.finalize().as_bytes()))
    }

    /// Only the semantic native constructor in the parent module issues this state.
    pub(super) fn issue_original_authority(&self) -> gmeow_errors::Result<()> {
        let execution = self.provenance.native_execution.as_ref().ok_or_else(|| {
            result_err("original native assessment requires its execution owner".into())
        })?;
        let authority = NativeAuthority::Original {
            assessment: self.assessment_commitment()?,
            execution: Arc::clone(execution),
        };
        self.native_authority
            .set(authority)
            .map_err(|_| result_err("original native assessment was already issued".into()))
    }

    /// Authenticate the immutable original payload before minting any publication.
    pub(crate) fn original_assessment(&self) -> gmeow_errors::Result<AssessmentCommitment> {
        let Some(NativeAuthority::Original {
            assessment,
            execution,
        }) = self.native_authority.get()
        else {
            return Err(result_err(
                "original native closure requires its constructor-owned assessment".into(),
            ));
        };
        if !self
            .provenance
            .native_execution
            .as_ref()
            .is_some_and(|current| Arc::ptr_eq(current, execution))
            || self.assessment_commitment()? != *assessment
        {
            return Err(result_err(
                "original native closure differs from its constructor-owned assessment".into(),
            ));
        }
        Ok(*assessment)
    }

    /// Reuse authority is restored only by the exact admitted binary record. This
    /// operation works through a shared result without cloning its owned row set.
    pub(crate) fn admit_native_receipt(
        &self,
        assessment: AssessmentCommitment,
        execution: &Arc<NativeExecutionEvidence>,
        receipt: &NativeReceiptAddress,
        closure: Option<&NativeReceiptAddress>,
    ) -> gmeow_errors::Result<()> {
        let current = self.provenance.native_execution.as_ref().ok_or_else(|| {
            result_err("retained native receipt requires its complete execution".into())
        })?;
        if current.as_ref() != execution.as_ref() || self.assessment_commitment()? != assessment {
            return Err(result_err(
                "retained native receipt differs from the complete execution or assessment".into(),
            ));
        }
        match self.native_authority.get() {
            Some(NativeAuthority::Original { .. }) => {
                self.original_assessment()?;
                return Ok(());
            }
            Some(NativeAuthority::ReceiptAdmitted {
                assessment: previous,
                execution: owner,
                receipt: address,
                closure: attachment,
            }) => {
                if *previous != assessment
                    || !Arc::ptr_eq(current, owner)
                    || address != receipt
                    || attachment.as_ref() != closure
                {
                    return Err(result_err(
                        "retained native receipt authority cannot be replaced".into(),
                    ));
                }
                return Ok(());
            }
            None => {}
        }
        self.validate()?;
        let authority = NativeAuthority::ReceiptAdmitted {
            assessment,
            execution: Arc::clone(current),
            receipt: receipt.clone(),
            closure: closure.cloned(),
        };
        if self.native_authority.set(authority).is_err() {
            // A concurrent admission must establish exactly the same immutable record.
            return self.admit_native_receipt(assessment, execution, receipt, closure);
        }
        Ok(())
    }

    /// Already admitted immutable owners can reuse their exact bytes without
    /// decoding another copy of the retained execution DAG.
    pub(crate) fn retained_receipt_selection(
        &self,
        receipt: &NativeReceiptAddress,
    ) -> gmeow_errors::Result<Option<&Option<NativeReceiptAddress>>> {
        let Some(NativeAuthority::ReceiptAdmitted {
            assessment,
            execution,
            receipt: admitted,
            closure,
        }) = self.native_authority.get()
        else {
            return Ok(None);
        };
        if admitted != receipt
            || !self
                .provenance
                .native_execution
                .as_ref()
                .is_some_and(|current| Arc::ptr_eq(current, execution))
            || self.assessment_commitment()? != *assessment
        {
            return Err(result_err(
                "retained native receipt authority cannot be replaced".into(),
            ));
        }
        Ok(Some(closure))
    }

    /// A summary transform always gives up original-payload authority, even when
    /// its selected normalization leaves every term unchanged.
    pub(crate) fn summary_rows_mut(
        &mut self,
    ) -> Option<&mut Vec<crate::reason::el::InferredAxiom>> {
        if matches!(
            self.native_authority.get(),
            Some(NativeAuthority::Original { .. })
        ) {
            self.native_authority = OnceLock::new();
        }
        match &mut self.payload {
            ResultPayload::Inferred(rows) => Some(rows),
            _ => None,
        }
    }

    /// Reuse a verified immutable execution owner after decoding a summary.
    pub(crate) fn share_admitted_execution(
        &mut self,
        execution: Option<Arc<NativeExecutionEvidence>>,
    ) -> gmeow_errors::Result<()> {
        if self.provenance.native_execution != execution {
            return Err(result_err(
                "authenticated native receipt differs from complete original execution evidence"
                    .into(),
            ));
        }
        if let Some(execution) = &execution {
            let Some(NativeAuthority::ReceiptAdmitted {
                execution: owner, ..
            }) = self.native_authority.get_mut()
            else {
                return Err(result_err(
                    "shared native execution requires receipt-admitted summary authority".into(),
                ));
            };
            *owner = Arc::clone(execution);
        }
        self.provenance.native_execution = execution;
        Ok(())
    }
}
