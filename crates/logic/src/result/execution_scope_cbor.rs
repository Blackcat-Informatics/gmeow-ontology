// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Shared execution identity and declared preservation data without regeneration.

use std::io::{self, Write};

use purrdf_lex::cbor::head;

use crate::execution_cbor::{ExecutionCbor, Reader, Result, descend};

use gmeow_logic_compile::ir::PreservationKind;

use super::{EngineId, PreservationClaim};

impl ExecutionCbor for PreservationKind {
    /// Delegate the exact retained variant wire to its compiler owner.
    fn write_cbor<W: Write + ?Sized>(&self, out: &mut W) -> io::Result<()> {
        PreservationKind::write_cbor(self, out)
    }

    /// Check the exact typed-reader depth on borrowed fields without allocation.
    fn check_depth(&self, remaining: usize) -> Result<()> {
        descend(remaining)?;
        Ok(())
    }

    /// Charge the original typed enum visit even though its wire is text.
    fn read_cbor(input: &mut Reader<'_>) -> Result<Self> {
        input.variant(|input, name, tagged| match (name, tagged) {
            ("Exact", false) => Ok(Self::Exact),
            ("SoundUnder", false) => Ok(Self::SoundUnder),
            ("CompleteOver", false) => Ok(Self::CompleteOver),
            ("ValidationOnly", false) => Ok(Self::ValidationOnly),
            ("InconsistencyPreserving", false) => Ok(Self::InconsistencyPreserving),
            ("InconsistencyReflecting", false) => Ok(Self::InconsistencyReflecting),
            ("Unsupported", false) => Ok(Self::Unsupported),
            _ => Err(input.invalid("unknown PreservationKind variant or form")),
        })
    }
}

impl ExecutionCbor for EngineId {
    /// Retain the complete EngineId in its original declaration order.
    fn write_cbor<W: Write + ?Sized>(&self, out: &mut W) -> io::Result<()> {
        head::write_head(out, head::MAP, 2)?;
        head::write_text(out, "name")?;
        ExecutionCbor::write_cbor(&self.name, out)?;
        head::write_text(out, "version")?;
        ExecutionCbor::write_cbor(&self.version, out)?;
        Ok(())
    }

    /// Check the exact typed-reader depth on borrowed fields without allocation.
    fn check_depth(&self, remaining: usize) -> Result<()> {
        let remaining = descend(remaining)?;
        ExecutionCbor::check_depth(&self.name, remaining)?;
        ExecutionCbor::check_depth(&self.version, remaining)?;
        Ok(())
    }

    /// Reconstruct the exact stored fields without validation or normalization.
    fn read_cbor(input: &mut Reader<'_>) -> Result<Self> {
        input.map(|input, length| {
            if length != 2 {
                return Err(input.invalid("wrong EngineId field count"));
            }
            input.field("name")?;
            let name = ExecutionCbor::read_cbor(input)?;
            input.field("version")?;
            let version = ExecutionCbor::read_cbor(input)?;
            Ok(Self { name, version })
        })
    }
}

impl ExecutionCbor for PreservationClaim {
    /// Retain the complete PreservationClaim in its original declaration order.
    fn write_cbor<W: Write + ?Sized>(&self, out: &mut W) -> io::Result<()> {
        head::write_head(out, head::MAP, 2)?;
        head::write_text(out, "polarities")?;
        ExecutionCbor::write_cbor(&self.polarities, out)?;
        head::write_text(out, "unsupported_constructs")?;
        ExecutionCbor::write_cbor(&self.unsupported_constructs, out)?;
        Ok(())
    }

    /// Check the exact typed-reader depth on borrowed fields without allocation.
    fn check_depth(&self, remaining: usize) -> Result<()> {
        let remaining = descend(remaining)?;
        ExecutionCbor::check_depth(&self.polarities, remaining)?;
        ExecutionCbor::check_depth(&self.unsupported_constructs, remaining)?;
        Ok(())
    }

    /// Reconstruct the exact stored fields without validation or normalization.
    fn read_cbor(input: &mut Reader<'_>) -> Result<Self> {
        input.map(|input, length| {
            if length != 2 {
                return Err(input.invalid("wrong PreservationClaim field count"));
            }
            input.field("polarities")?;
            let polarities = ExecutionCbor::read_cbor(input)?;
            input.field("unsupported_constructs")?;
            let unsupported_constructs = ExecutionCbor::read_cbor(input)?;
            Ok(Self {
                polarities,
                unsupported_constructs,
            })
        })
    }
}
