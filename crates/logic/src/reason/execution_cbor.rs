// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! World-scoped native chase certificates in complete execution receipts.

use std::io;

use purrdf_lex::cbor::head;

use crate::execution_cbor::{ExecutionCbor, Reader, Result, descend};

use super::ChaseCertificate;

impl ExecutionCbor for ChaseCertificate {
    /// Retain the complete ChaseCertificate in its original declaration order.
    fn write_cbor<W: crate::execution_cbor::LeafWrite + ?Sized>(
        &self,
        out: &mut W,
    ) -> io::Result<()> {
        head::write_head(out, head::MAP, 3)?;
        head::write_text(out, "world")?;
        ExecutionCbor::write_cbor(&self.world, out)?;
        head::write_text(out, "input_contract")?;
        ExecutionCbor::write_cbor(&self.input_contract, out)?;
        head::write_text(out, "admission")?;
        ExecutionCbor::write_cbor(&self.admission, out)?;
        Ok(())
    }

    /// Check the exact typed-reader depth on borrowed fields without allocation.
    fn check_depth(&self, remaining: usize) -> Result<()> {
        let remaining = descend(remaining)?;
        ExecutionCbor::check_depth(&self.world, remaining)?;
        ExecutionCbor::check_depth(&self.input_contract, remaining)?;
        ExecutionCbor::check_depth(&self.admission, remaining)?;
        Ok(())
    }

    /// Reconstruct the exact stored fields without validation or normalization.
    fn read_cbor(input: &mut Reader<'_>) -> Result<Self> {
        input.map(|input, length| {
            if length != 3 {
                return Err(input.invalid("wrong ChaseCertificate field count"));
            }
            input.field("world")?;
            let world = ExecutionCbor::read_cbor(input)?;
            input.field("input_contract")?;
            let input_contract = ExecutionCbor::read_cbor(input)?;
            input.field("admission")?;
            let admission = ExecutionCbor::read_cbor(input)?;
            Ok(Self {
                world,
                input_contract,
                admission,
            })
        })
    }
}
