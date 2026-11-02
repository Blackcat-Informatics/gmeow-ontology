// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Source-bound reachability analysis in the complete execution receipt.

use std::io;

use purrdf_lex::cbor::head;

use crate::execution_cbor::{ExecutionCbor, Reader, Result, descend};

use super::{ProgramReachability, ReachabilityEvidence};

impl ExecutionCbor for ReachabilityEvidence {
    /// Retain a selected program analysis or the explicit class-only variant.
    fn write_cbor<W: crate::execution_cbor::LeafWrite + ?Sized>(
        &self,
        out: &mut W,
    ) -> io::Result<()> {
        match self {
            Self::ClassDiagnostic => head::write_text(out, "ClassDiagnostic"),
            Self::Program(program) => {
                head::write_head(out, head::MAP, 1)?;
                head::write_text(out, "Program")?;
                ExecutionCbor::write_cbor(program, out)
            }
        }
    }

    /// Check the exact typed-reader depth on borrowed fields without allocation.
    fn check_depth(&self, remaining: usize) -> Result<()> {
        let remaining = descend(remaining)?;
        match self {
            Self::ClassDiagnostic => Ok(()),
            Self::Program(program) => ExecutionCbor::check_depth(program, remaining),
        }
    }

    /// Preserve the enum visitor and transparent boxed analysis payload.
    fn read_cbor(input: &mut Reader<'_>) -> Result<Self> {
        input.variant(|input, name, tagged| match (name, tagged) {
            ("ClassDiagnostic", false) => Ok(Self::ClassDiagnostic),
            ("Program", true) => Ok(Self::Program(ExecutionCbor::read_cbor(input)?)),
            _ => Err(input.invalid("unknown ReachabilityEvidence variant or form")),
        })
    }
}

impl ExecutionCbor for ProgramReachability {
    /// Retain the complete ProgramReachability in its original declaration order.
    fn write_cbor<W: crate::execution_cbor::LeafWrite + ?Sized>(
        &self,
        out: &mut W,
    ) -> io::Result<()> {
        head::write_head(out, head::MAP, 8)?;
        head::write_text(out, "identity")?;
        ExecutionCbor::write_cbor(&self.identity, out)?;
        head::write_text(out, "input_contract")?;
        ExecutionCbor::write_cbor(&self.input_contract, out)?;
        head::write_text(out, "program")?;
        ExecutionCbor::write_cbor(&self.program, out)?;
        head::write_text(out, "engine")?;
        ExecutionCbor::write_cbor(&self.engine, out)?;
        head::write_text(out, "engine_contract")?;
        ExecutionCbor::write_cbor(&self.engine_contract, out)?;
        head::write_text(out, "lowering")?;
        ExecutionCbor::write_cbor(&self.lowering, out)?;
        head::write_text(out, "foundation")?;
        ExecutionCbor::write_cbor(&self.foundation, out)?;
        head::write_text(out, "worlds")?;
        ExecutionCbor::write_cbor(&self.worlds, out)?;
        Ok(())
    }

    /// Check the exact typed-reader depth on borrowed fields without allocation.
    fn check_depth(&self, remaining: usize) -> Result<()> {
        let remaining = descend(remaining)?;
        ExecutionCbor::check_depth(&self.identity, remaining)?;
        ExecutionCbor::check_depth(&self.input_contract, remaining)?;
        ExecutionCbor::check_depth(&self.program, remaining)?;
        ExecutionCbor::check_depth(&self.engine, remaining)?;
        ExecutionCbor::check_depth(&self.engine_contract, remaining)?;
        ExecutionCbor::check_depth(&self.lowering, remaining)?;
        ExecutionCbor::check_depth(&self.foundation, remaining)?;
        ExecutionCbor::check_depth(&self.worlds, remaining)?;
        Ok(())
    }

    /// Reconstruct the exact stored fields without validation or normalization.
    fn read_cbor(input: &mut Reader<'_>) -> Result<Self> {
        input.map(|input, length| {
            if length != 8 {
                return Err(input.invalid("wrong ProgramReachability field count"));
            }
            input.field("identity")?;
            let identity = ExecutionCbor::read_cbor(input)?;
            input.field("input_contract")?;
            let input_contract = ExecutionCbor::read_cbor(input)?;
            input.field("program")?;
            let program = ExecutionCbor::read_cbor(input)?;
            input.field("engine")?;
            let engine = ExecutionCbor::read_cbor(input)?;
            input.field("engine_contract")?;
            let engine_contract = ExecutionCbor::read_cbor(input)?;
            input.field("lowering")?;
            let lowering = ExecutionCbor::read_cbor(input)?;
            input.field("foundation")?;
            let foundation = ExecutionCbor::read_cbor(input)?;
            input.field("worlds")?;
            let worlds = ExecutionCbor::read_cbor(input)?;
            Ok(Self {
                identity,
                input_contract,
                program,
                engine,
                engine_contract,
                lowering,
                foundation,
                worlds,
            })
        })
    }
}
