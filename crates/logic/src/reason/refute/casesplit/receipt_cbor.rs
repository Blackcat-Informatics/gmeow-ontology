// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Complete owner-local native execution receipt records, without semantic reconstruction.

use std::io::{self, Write};

use purrdf_lex::cbor::head;

use crate::execution_cbor::{ExecutionCbor, Reader, Result as WireResult, descend};

use super::{
    ClassAdmissionObservation, ClassAdmissionWorld, ClassExecutionOutcome, ClassSourceRefusal,
};

impl ExecutionCbor for ClassExecutionOutcome {
    fn write_cbor<W: Write + ?Sized>(&self, out: &mut W) -> io::Result<()> {
        head::write_head(out, head::MAP, 6)?;
        head::write_text(out, "world")?;
        ExecutionCbor::write_cbor(&self.world, out)?;
        head::write_text(out, "graph")?;
        ExecutionCbor::write_cbor(&self.graph, out)?;
        head::write_text(out, "input_contract")?;
        ExecutionCbor::write_cbor(&self.input_contract, out)?;
        head::write_text(out, "completion")?;
        ExecutionCbor::write_cbor(&self.completion, out)?;
        head::write_text(out, "contextual_conflicts")?;
        ExecutionCbor::write_cbor(&self.contextual_conflicts, out)?;
        head::write_text(out, "obstructions")?;
        ExecutionCbor::write_cbor(&self.obstructions, out)?;
        Ok(())
    }

    fn check_depth(&self, remaining: usize) -> WireResult<()> {
        let remaining = descend(remaining)?;
        ExecutionCbor::check_depth(&self.world, remaining)?;
        ExecutionCbor::check_depth(&self.graph, remaining)?;
        ExecutionCbor::check_depth(&self.input_contract, remaining)?;
        ExecutionCbor::check_depth(&self.completion, remaining)?;
        ExecutionCbor::check_depth(&self.contextual_conflicts, remaining)?;
        ExecutionCbor::check_depth(&self.obstructions, remaining)?;
        Ok(())
    }

    fn read_cbor(input: &mut Reader<'_>) -> WireResult<Self> {
        input.map(|input, length| {
            if length != 6 {
                return Err(input.invalid("expected complete ClassExecutionOutcome record"));
            }
            input.field("world")?;
            let world = ExecutionCbor::read_cbor(input)?;
            input.field("graph")?;
            let graph = ExecutionCbor::read_cbor(input)?;
            input.field("input_contract")?;
            let input_contract = ExecutionCbor::read_cbor(input)?;
            input.field("completion")?;
            let completion = ExecutionCbor::read_cbor(input)?;
            input.field("contextual_conflicts")?;
            let contextual_conflicts = ExecutionCbor::read_cbor(input)?;
            input.field("obstructions")?;
            let obstructions = ExecutionCbor::read_cbor(input)?;
            Ok(Self {
                world,
                graph,
                input_contract,
                completion,
                contextual_conflicts,
                obstructions,
            })
        })
    }
}

// The source-admission profile has a narrower boundary writer. This complete
// receipt owner retains every represented variant; admission stays elsewhere.
impl ExecutionCbor for ClassAdmissionObservation {
    fn write_cbor<W: Write + ?Sized>(&self, out: &mut W) -> io::Result<()> {
        head::write_head(out, head::MAP, 3)?;
        head::write_text(out, "contract")?;
        ExecutionCbor::write_cbor(&self.contract, out)?;
        head::write_text(out, "source_worlds")?;
        ExecutionCbor::write_cbor(&self.source_worlds, out)?;
        head::write_text(out, "selected_worlds")?;
        ExecutionCbor::write_cbor(&self.selected_worlds, out)?;
        Ok(())
    }

    fn check_depth(&self, remaining: usize) -> WireResult<()> {
        let remaining = descend(remaining)?;
        ExecutionCbor::check_depth(&self.contract, remaining)?;
        ExecutionCbor::check_depth(&self.source_worlds, remaining)?;
        ExecutionCbor::check_depth(&self.selected_worlds, remaining)?;
        Ok(())
    }

    fn read_cbor(input: &mut Reader<'_>) -> WireResult<Self> {
        input.map(|input, length| {
            if length != 3 {
                return Err(input.invalid("expected complete ClassAdmissionObservation record"));
            }
            input.field("contract")?;
            let contract = ExecutionCbor::read_cbor(input)?;
            input.field("source_worlds")?;
            let source_worlds = ExecutionCbor::read_cbor(input)?;
            input.field("selected_worlds")?;
            let selected_worlds = ExecutionCbor::read_cbor(input)?;
            Ok(Self {
                contract,
                source_worlds,
                selected_worlds,
            })
        })
    }
}

impl ExecutionCbor for ClassAdmissionWorld {
    fn write_cbor<W: Write + ?Sized>(&self, out: &mut W) -> io::Result<()> {
        head::write_head(out, head::MAP, 3)?;
        head::write_text(out, "graph")?;
        ExecutionCbor::write_cbor(&self.graph, out)?;
        head::write_text(out, "definitions")?;
        ExecutionCbor::write_cbor(&self.definitions, out)?;
        head::write_text(out, "refusal")?;
        ExecutionCbor::write_cbor(&self.refusal, out)?;
        Ok(())
    }

    fn check_depth(&self, remaining: usize) -> WireResult<()> {
        let remaining = descend(remaining)?;
        ExecutionCbor::check_depth(&self.graph, remaining)?;
        ExecutionCbor::check_depth(&self.definitions, remaining)?;
        ExecutionCbor::check_depth(&self.refusal, remaining)?;
        Ok(())
    }

    fn read_cbor(input: &mut Reader<'_>) -> WireResult<Self> {
        input.map(|input, length| {
            if length != 3 {
                return Err(input.invalid("expected complete ClassAdmissionWorld record"));
            }
            input.field("graph")?;
            let graph = ExecutionCbor::read_cbor(input)?;
            input.field("definitions")?;
            let definitions = ExecutionCbor::read_cbor(input)?;
            input.field("refusal")?;
            let refusal = ExecutionCbor::read_cbor(input)?;
            Ok(Self {
                graph,
                definitions,
                refusal,
            })
        })
    }
}

impl ExecutionCbor for ClassSourceRefusal {
    fn write_cbor<W: Write + ?Sized>(&self, out: &mut W) -> io::Result<()> {
        match self {
            Self::Invalid => head::write_text(out, "Invalid"),
            Self::Unsupported => head::write_text(out, "Unsupported"),
        }
    }

    fn check_depth(&self, remaining: usize) -> WireResult<()> {
        descend(remaining)?;
        match self {
            Self::Invalid | Self::Unsupported => Ok(()),
        }
    }

    fn read_cbor(input: &mut Reader<'_>) -> WireResult<Self> {
        input.variant(|input, name, tagged| match (name, tagged) {
            ("Invalid", false) => Ok(Self::Invalid),
            ("Unsupported", false) => Ok(Self::Unsupported),
            _ => Err(input.invalid("invalid ClassSourceRefusal variant or payload form")),
        })
    }
}
