// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Complete potential-writer inventory transport without source recomputation.

use std::io::{self, Write};

use purrdf_lex::cbor::head;

use crate::execution_cbor::{ExecutionCbor, Reader, Result, descend};

use super::WriterInventory;

impl ExecutionCbor for WriterInventory {
    /// Retain the complete WriterInventory in its original declaration order.
    fn write_cbor<W: Write + ?Sized>(&self, out: &mut W) -> io::Result<()> {
        head::write_head(out, head::MAP, 5)?;
        head::write_text(out, "theory")?;
        ExecutionCbor::write_cbor(&self.theory, out)?;
        head::write_text(out, "producers")?;
        ExecutionCbor::write_cbor(&self.producers, out)?;
        head::write_text(out, "predicates")?;
        ExecutionCbor::write_cbor(&self.predicates, out)?;
        head::write_text(out, "unbounded")?;
        ExecutionCbor::write_cbor(&self.unbounded, out)?;
        head::write_text(out, "requires_source_admission")?;
        ExecutionCbor::write_cbor(&self.requires_source_admission, out)?;
        Ok(())
    }

    /// Check the exact typed-reader depth on borrowed fields without allocation.
    fn check_depth(&self, remaining: usize) -> Result<()> {
        let remaining = descend(remaining)?;
        ExecutionCbor::check_depth(&self.theory, remaining)?;
        ExecutionCbor::check_depth(&self.producers, remaining)?;
        ExecutionCbor::check_depth(&self.predicates, remaining)?;
        ExecutionCbor::check_depth(&self.unbounded, remaining)?;
        ExecutionCbor::check_depth(&self.requires_source_admission, remaining)?;
        Ok(())
    }

    /// Reconstruct the exact stored fields without validation or normalization.
    fn read_cbor(input: &mut Reader<'_>) -> Result<Self> {
        input.map(|input, length| {
            if length != 5 {
                return Err(input.invalid("wrong WriterInventory field count"));
            }
            input.field("theory")?;
            let theory = ExecutionCbor::read_cbor(input)?;
            input.field("producers")?;
            let producers = ExecutionCbor::read_cbor(input)?;
            input.field("predicates")?;
            let predicates = ExecutionCbor::read_cbor(input)?;
            input.field("unbounded")?;
            let unbounded = ExecutionCbor::read_cbor(input)?;
            input.field("requires_source_admission")?;
            let requires_source_admission = ExecutionCbor::read_cbor(input)?;
            Ok(Self {
                theory,
                producers,
                predicates,
                unbounded,
                requires_source_admission,
            })
        })
    }
}
