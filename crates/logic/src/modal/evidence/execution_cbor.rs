// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Exact owned execution-receipt fields; no semantic reconstruction at decoding.

use std::io::{self, Write};

use crate::execution_cbor::{ExecutionCbor, Reader, Result, descend};
use purrdf_lex::cbor::head;

use super::{ModalEvaluation, ModalFrontier, ModalOp, ModalWorldEvidence};

impl ExecutionCbor for ModalWorldEvidence {
    /// Preflight the same record visitor and fields used by the typed reader.
    fn check_depth(&self, remaining: usize) -> Result<()> {
        let remaining = descend(remaining)?;
        self.world.check_depth(remaining)?;
        self.atom_present.check_depth(remaining)?;
        Ok(())
    }
    /// Stream every field in this record's serialized declaration order.
    fn write_cbor<W: Write + ?Sized>(&self, out: &mut W) -> io::Result<()> {
        head::write_head(out, head::MAP, 2)?;
        head::write_text(out, "world")?;
        self.world.write_cbor(out)?;
        head::write_text(out, "atom_present")?;
        self.atom_present.write_cbor(out)?;
        Ok(())
    }

    /// Restore exact recorded fields without admission, sorting or identity minting.
    fn read_cbor(input: &mut Reader<'_>) -> Result<Self> {
        input.map(|input, length| {
            if length != 2 {
                return Err(input.invalid("wrong ModalWorldEvidence field count"));
            }
            input.field("world")?;
            let world = ExecutionCbor::read_cbor(input)?;
            input.field("atom_present")?;
            let atom_present = ExecutionCbor::read_cbor(input)?;
            Ok(Self {
                world,
                atom_present,
            })
        })
    }
}

impl ExecutionCbor for ModalEvaluation {
    /// Preflight the same record visitor and fields used by the typed reader.
    fn check_depth(&self, remaining: usize) -> Result<()> {
        let remaining = descend(remaining)?;
        self.context.check_depth(remaining)?;
        self.formula.check_depth(remaining)?;
        self.operator.check_depth(remaining)?;
        self.body.check_depth(remaining)?;
        self.evaluation_world.check_depth(remaining)?;
        self.accessibility_relation.check_depth(remaining)?;
        self.atom_subject.check_depth(remaining)?;
        self.atom_predicate.check_depth(remaining)?;
        self.atom_object.check_depth(remaining)?;
        self.frontier.check_depth(remaining)?;
        self.conclusion_predicate.check_depth(remaining)?;
        self.conclusion_object.check_depth(remaining)?;
        Ok(())
    }
    /// Stream every field in this record's serialized declaration order.
    fn write_cbor<W: Write + ?Sized>(&self, out: &mut W) -> io::Result<()> {
        head::write_head(out, head::MAP, 12)?;
        head::write_text(out, "context")?;
        self.context.write_cbor(out)?;
        head::write_text(out, "formula")?;
        self.formula.write_cbor(out)?;
        head::write_text(out, "operator")?;
        self.operator.write_cbor(out)?;
        head::write_text(out, "body")?;
        self.body.write_cbor(out)?;
        head::write_text(out, "evaluation_world")?;
        self.evaluation_world.write_cbor(out)?;
        head::write_text(out, "accessibility_relation")?;
        self.accessibility_relation.write_cbor(out)?;
        head::write_text(out, "atom_subject")?;
        self.atom_subject.write_cbor(out)?;
        head::write_text(out, "atom_predicate")?;
        self.atom_predicate.write_cbor(out)?;
        head::write_text(out, "atom_object")?;
        self.atom_object.write_cbor(out)?;
        head::write_text(out, "frontier")?;
        self.frontier.write_cbor(out)?;
        head::write_text(out, "conclusion_predicate")?;
        self.conclusion_predicate.write_cbor(out)?;
        head::write_text(out, "conclusion_object")?;
        self.conclusion_object.write_cbor(out)?;
        Ok(())
    }

    /// Restore exact recorded fields without admission, sorting or identity minting.
    fn read_cbor(input: &mut Reader<'_>) -> Result<Self> {
        input.map(|input, length| {
            if length != 12 {
                return Err(input.invalid("wrong ModalEvaluation field count"));
            }
            input.field("context")?;
            let context = ExecutionCbor::read_cbor(input)?;
            input.field("formula")?;
            let formula = ExecutionCbor::read_cbor(input)?;
            input.field("operator")?;
            let operator = ExecutionCbor::read_cbor(input)?;
            input.field("body")?;
            let body = ExecutionCbor::read_cbor(input)?;
            input.field("evaluation_world")?;
            let evaluation_world = ExecutionCbor::read_cbor(input)?;
            input.field("accessibility_relation")?;
            let accessibility_relation = ExecutionCbor::read_cbor(input)?;
            input.field("atom_subject")?;
            let atom_subject = ExecutionCbor::read_cbor(input)?;
            input.field("atom_predicate")?;
            let atom_predicate = ExecutionCbor::read_cbor(input)?;
            input.field("atom_object")?;
            let atom_object = ExecutionCbor::read_cbor(input)?;
            input.field("frontier")?;
            let frontier = ExecutionCbor::read_cbor(input)?;
            input.field("conclusion_predicate")?;
            let conclusion_predicate = ExecutionCbor::read_cbor(input)?;
            input.field("conclusion_object")?;
            let conclusion_object = ExecutionCbor::read_cbor(input)?;
            Ok(Self {
                context,
                formula,
                operator,
                body,
                evaluation_world,
                accessibility_relation,
                atom_subject,
                atom_predicate,
                atom_object,
                frontier,
                conclusion_predicate,
                conclusion_object,
            })
        })
    }
}

impl ExecutionCbor for ModalOp {
    crate::__gmeow_execution_unit_depth!(Box, Diamond);
    /// Retain the original unit-variant text, independent of projection vocabularies.
    fn write_cbor<W: Write + ?Sized>(&self, out: &mut W) -> io::Result<()> {
        head::write_text(
            out,
            match self {
                Self::Box => "Box",
                Self::Diamond => "Diamond",
            },
        )
    }

    /// Match the original unit enum visitor, including its typed recursion charge.
    fn read_cbor(input: &mut Reader<'_>) -> Result<Self> {
        input.variant(|input, name, tagged| match (name, tagged) {
            ("Box", false) => Ok(Self::Box),
            ("Diamond", false) => Ok(Self::Diamond),
            _ => Err(input.invalid("unknown or non-unit ModalOp variant")),
        })
    }
}

impl ExecutionCbor for ModalFrontier {
    /// The enum visitor remains open during a tagged struct payload visitor.
    fn check_depth(&self, remaining: usize) -> Result<()> {
        let remaining = descend(remaining)?;
        match self {
            Self::CompletedFinitePredecessor { worlds } => {
                let remaining = descend(remaining)?;
                worlds.check_depth(remaining)?;
                Ok(())
            }
        }
    }
    /// Preserve the original unit text or externally tagged struct-variant wire.
    fn write_cbor<W: Write + ?Sized>(&self, out: &mut W) -> io::Result<()> {
        match self {
            Self::CompletedFinitePredecessor { worlds } => {
                head::write_head(out, head::MAP, 1)?;
                head::write_text(out, "CompletedFinitePredecessor")?;
                head::write_head(out, head::MAP, 1)?;
                head::write_text(out, "worlds")?;
                worlds.write_cbor(out)?;
                Ok(())
            }
        }
    }

    /// The tagged enum and its struct payload each consume their original visitor.
    fn read_cbor(input: &mut Reader<'_>) -> Result<Self> {
        input.variant(|input, name, tagged| match (name, tagged) {
            ("CompletedFinitePredecessor", true) => input.map(|input, length| {
                if length != 1 {
                    return Err(input
                        .invalid("wrong ModalFrontier::CompletedFinitePredecessor field count"));
                }
                input.field("worlds")?;
                let worlds = ExecutionCbor::read_cbor(input)?;
                Ok(Self::CompletedFinitePredecessor { worlds })
            }),
            _ => Err(input.invalid("unknown or incorrectly framed ModalFrontier variant")),
        })
    }
}
