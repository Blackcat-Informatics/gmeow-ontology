// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Exact owned execution-receipt fields; no semantic reconstruction at decoding.

use std::io;

use crate::execution_cbor::{ExecutionCbor, Reader, Result, descend};
use purrdf_lex::cbor::head;

use super::{NativeModalEvidence, NativeModalSupport};

crate::__gmeow_execution_record!(
    NativeModalSupport { world, proof },
    "wrong NativeModalSupport field count"
);

impl ExecutionCbor for NativeModalEvidence {
    /// Preflight the same record visitor and fields used by the typed reader.
    fn check_depth(&self, remaining: usize) -> Result<()> {
        let remaining = descend(remaining)?;
        self.input_contract.check_depth(remaining)?;
        self.evaluation.check_depth(remaining)?;
        self.supports.check_depth(remaining)?;
        Ok(())
    }
    /// Stream every field in this record's serialized declaration order.
    fn write_cbor<W: crate::execution_cbor::LeafWrite + ?Sized>(
        &self,
        out: &mut W,
    ) -> io::Result<()> {
        head::write_head(out, head::MAP, 3)?;
        head::write_text(out, "input_contract")?;
        self.input_contract.write_cbor(out)?;
        head::write_text(out, "evaluation")?;
        self.evaluation.write_cbor(out)?;
        head::write_text(out, "supports")?;
        self.supports.write_cbor(out)?;
        Ok(())
    }

    /// Restore exact recorded fields without admission, sorting or identity minting.
    fn read_cbor(input: &mut Reader<'_>) -> Result<Self> {
        input.map(|input, length| {
            if length != 3 {
                return Err(input.invalid("wrong NativeModalEvidence field count"));
            }
            input.field("input_contract")?;
            let input_contract = ExecutionCbor::read_cbor(input)?;
            input.field("evaluation")?;
            let evaluation = ExecutionCbor::read_cbor(input)?;
            input.field("supports")?;
            let supports = ExecutionCbor::read_cbor(input)?;
            Ok(Self {
                input_contract,
                evaluation,
                supports,
            })
        })
    }
}
