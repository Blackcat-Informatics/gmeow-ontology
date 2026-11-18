// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! One strict receipt binding over the unchanged native execution record.

use std::io;
use std::sync::Arc;

use purrdf_lex::cbor::head;

use crate::execution_cbor::{ExecutionCbor, LeafWrite, Reader, Result, descend};
use crate::result::{AssessmentCommitment, NativeExecutionEvidence, ReasoningResult};
use crate::result_rdf::NativeReceiptAddress;

const NATIVE_SCHEMA: &str = "gmeow-native-execution-v4";

/// The writer borrows this small shared owner; decoding owns its admitted Arc.
#[derive(Debug, Clone, PartialEq)]
pub(in crate::result_rdf) struct ReceiptRecord {
    pub(in crate::result_rdf) execution: Arc<NativeExecutionEvidence>,
    pub(in crate::result_rdf) assessment: AssessmentCommitment,
    pub(in crate::result_rdf) closure: Option<NativeReceiptAddress>,
}

impl ReceiptRecord {
    pub(in crate::result_rdf) fn original(
        result: &ReasoningResult,
        closure: Option<&NativeReceiptAddress>,
    ) -> gmeow_errors::Result<Self> {
        let assessment = result.original_assessment()?;
        let execution = result.provenance.native_execution.as_ref().ok_or_else(|| {
            super::super::result_err("native receipt requires its original execution owner".into())
        })?;
        Ok(Self {
            execution: Arc::clone(execution),
            assessment,
            closure: closure.cloned(),
        })
    }
}

impl ExecutionCbor for ReceiptRecord {
    fn check_depth(&self, remaining: usize) -> Result<()> {
        let remaining = descend(remaining)?;
        self.execution.check_depth(remaining)?;
        self.assessment.0.check_depth(remaining)?;
        if self.closure.is_some() {
            descend(remaining)?;
        }
        Ok(())
    }

    fn write_cbor<'data, W: LeafWrite<'data> + ?Sized>(&'data self, out: &mut W) -> io::Result<()> {
        head::write_head(out, head::MAP, 4)?;
        head::write_text(out, "native_schema")?;
        head::write_text(out, NATIVE_SCHEMA)?;
        head::write_text(out, "execution")?;
        self.execution.write_cbor(out)?;
        head::write_text(out, "assessment")?;
        self.assessment.0.write_cbor(out)?;
        head::write_text(out, "closure")?;
        match &self.closure {
            Some(address) => out.string(address.as_str()),
            None => head::write_head(out, head::SIMPLE, 22),
        }
    }

    fn read_cbor(input: &mut Reader<'_>) -> Result<Self> {
        input.map(|input, length| {
            if length != 4 {
                return Err(input.invalid("wrong native receipt binding field count"));
            }
            input.field("native_schema")?;
            if input.text()? != NATIVE_SCHEMA {
                return Err(input.invalid("native receipt requires the selected native v4 record"));
            }
            input.field("execution")?;
            let execution = ExecutionCbor::read_cbor(input)?;
            input.field("assessment")?;
            let assessment = AssessmentCommitment(<[u8; 32]>::read_cbor(input)?);
            input.field("closure")?;
            let closure = Option::<String>::read_cbor(input)?
                .map(|address| {
                    NativeReceiptAddress::parse(&address).map_err(|cause| {
                        input.invalid(format!("invalid native closure address: {cause}"))
                    })
                })
                .transpose()?;
            Ok(Self {
                execution,
                assessment,
                closure,
            })
        })
    }
}
