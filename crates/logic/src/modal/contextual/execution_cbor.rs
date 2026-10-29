// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Exact owned execution-receipt fields; no semantic reconstruction at decoding.

use std::io::{self, Write};

use crate::execution_cbor::{ExecutionCbor, Reader, Result, descend};
use purrdf_lex::cbor::head;

use super::NativeEvidence;

impl ExecutionCbor for NativeEvidence {
    /// Preflight the same record visitor and fields used by the typed reader.
    fn check_depth(&self, remaining: usize) -> Result<()> {
        let remaining = descend(remaining)?;
        self.identity.check_depth(remaining)?;
        self.receipt.check_depth(remaining)?;
        self.object.check_depth(remaining)?;
        Ok(())
    }
    /// Stream every field in this record's serialized declaration order.
    fn write_cbor<W: Write + ?Sized>(&self, out: &mut W) -> io::Result<()> {
        head::write_head(out, head::MAP, 3)?;
        head::write_text(out, "identity")?;
        self.identity.write_cbor(out)?;
        head::write_text(out, "receipt")?;
        self.receipt.write_cbor(out)?;
        head::write_text(out, "object")?;
        self.object.write_cbor(out)?;
        Ok(())
    }

    /// Restore exact recorded fields without admission, sorting or identity minting.
    fn read_cbor(input: &mut Reader<'_>) -> Result<Self> {
        input.map(|input, length| {
            if length != 3 {
                return Err(input.invalid("wrong NativeEvidence field count"));
            }
            input.field("identity")?;
            let identity = ExecutionCbor::read_cbor(input)?;
            input.field("receipt")?;
            let receipt = ExecutionCbor::read_cbor(input)?;
            input.field("object")?;
            let object = ExecutionCbor::read_cbor(input)?;
            Ok(Self {
                identity,
                receipt,
                object,
            })
        })
    }
}
