// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Exact governed completion frontier in native execution receipts.

use std::io;

use purrdf_lex::cbor::head;

use crate::execution_cbor::{ExecutionCbor, Reader, Result, descend};

use super::CompletionFrontier;

impl ExecutionCbor for CompletionFrontier {
    /// Retain the complete CompletionFrontier in its original declaration order.
    fn write_cbor<W: crate::execution_cbor::LeafWrite + ?Sized>(
        &self,
        out: &mut W,
    ) -> io::Result<()> {
        head::write_head(out, head::MAP, 4)?;
        head::write_text(out, "completed")?;
        ExecutionCbor::write_cbor(&self.completed, out)?;
        head::write_text(out, "total")?;
        ExecutionCbor::write_cbor(&self.total, out)?;
        head::write_text(out, "saturated_preds")?;
        ExecutionCbor::write_cbor(&self.saturated_preds, out)?;
        head::write_text(out, "consumed_steps")?;
        ExecutionCbor::write_cbor(&self.consumed_steps, out)?;
        Ok(())
    }

    /// Check the exact typed-reader depth on borrowed fields without allocation.
    fn check_depth(&self, remaining: usize) -> Result<()> {
        let remaining = descend(remaining)?;
        ExecutionCbor::check_depth(&self.completed, remaining)?;
        ExecutionCbor::check_depth(&self.total, remaining)?;
        ExecutionCbor::check_depth(&self.saturated_preds, remaining)?;
        ExecutionCbor::check_depth(&self.consumed_steps, remaining)?;
        Ok(())
    }

    /// Reconstruct the exact stored fields without validation or normalization.
    fn read_cbor(input: &mut Reader<'_>) -> Result<Self> {
        input.map(|input, length| {
            if length != 4 {
                return Err(input.invalid("wrong CompletionFrontier field count"));
            }
            input.field("completed")?;
            let completed = ExecutionCbor::read_cbor(input)?;
            input.field("total")?;
            let total = ExecutionCbor::read_cbor(input)?;
            input.field("saturated_preds")?;
            let saturated_preds = ExecutionCbor::read_cbor(input)?;
            input.field("consumed_steps")?;
            let consumed_steps = ExecutionCbor::read_cbor(input)?;
            Ok(Self {
                completed,
                total,
                saturated_preds,
                consumed_steps,
            })
        })
    }
}
