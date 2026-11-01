// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Exact owned execution-receipt fields; no semantic reconstruction at decoding.

use std::io::{self, Write};

use crate::execution_cbor::{ExecutionCbor, Reader, Result, descend};
use purrdf_lex::cbor::head;

use super::{AssessmentAnchor, Inference, PathObservation, TemporalBasis, TemporalPrefix};

impl ExecutionCbor for TemporalPrefix {
    /// Preflight the same record visitor and fields used by the typed reader.
    fn check_depth(&self, remaining: usize) -> Result<()> {
        let remaining = descend(remaining)?;
        self.identity.check_depth(remaining)?;
        self.journal.check_depth(remaining)?;
        self.enactment.check_depth(remaining)?;
        self.head.check_depth(remaining)?;
        self.finalized.check_depth(remaining)?;
        self.initial_head.check_depth(remaining)?;
        self.head_hash.check_depth(remaining)?;
        Ok(())
    }
    /// Stream every field in this record's serialized declaration order.
    fn write_cbor<W: Write + ?Sized>(&self, out: &mut W) -> io::Result<()> {
        head::write_head(out, head::MAP, 7)?;
        head::write_text(out, "identity")?;
        self.identity.write_cbor(out)?;
        head::write_text(out, "journal")?;
        self.journal.write_cbor(out)?;
        head::write_text(out, "enactment")?;
        self.enactment.write_cbor(out)?;
        head::write_text(out, "head")?;
        self.head.write_cbor(out)?;
        head::write_text(out, "finalized")?;
        self.finalized.write_cbor(out)?;
        head::write_text(out, "initial_head")?;
        self.initial_head.write_cbor(out)?;
        head::write_text(out, "head_hash")?;
        self.head_hash.write_cbor(out)?;
        Ok(())
    }

    /// Restore exact recorded fields without admission, sorting or identity minting.
    fn read_cbor(input: &mut Reader<'_>) -> Result<Self> {
        input.map(|input, length| {
            if length != 7 {
                return Err(input.invalid("wrong TemporalPrefix field count"));
            }
            input.field("identity")?;
            let identity = ExecutionCbor::read_cbor(input)?;
            input.field("journal")?;
            let journal = ExecutionCbor::read_cbor(input)?;
            input.field("enactment")?;
            let enactment = ExecutionCbor::read_cbor(input)?;
            input.field("head")?;
            let head = ExecutionCbor::read_cbor(input)?;
            input.field("finalized")?;
            let finalized = ExecutionCbor::read_cbor(input)?;
            input.field("initial_head")?;
            let initial_head = ExecutionCbor::read_cbor(input)?;
            input.field("head_hash")?;
            let head_hash = ExecutionCbor::read_cbor(input)?;
            Ok(Self {
                identity,
                journal,
                enactment,
                head,
                finalized,
                initial_head,
                head_hash,
            })
        })
    }
}

impl ExecutionCbor for PathObservation {
    /// Preflight the same record visitor and fields used by the typed reader.
    fn check_depth(&self, remaining: usize) -> Result<()> {
        let remaining = descend(remaining)?;
        self.identity.check_depth(remaining)?;
        self.path.check_depth(remaining)?;
        self.world.check_depth(remaining)?;
        self.standpoint.check_depth(remaining)?;
        self.source_digest.check_depth(remaining)?;
        self.finalized.check_depth(remaining)?;
        Ok(())
    }
    /// Stream every field in this record's serialized declaration order.
    fn write_cbor<W: Write + ?Sized>(&self, out: &mut W) -> io::Result<()> {
        head::write_head(out, head::MAP, 6)?;
        head::write_text(out, "identity")?;
        self.identity.write_cbor(out)?;
        head::write_text(out, "path")?;
        self.path.write_cbor(out)?;
        head::write_text(out, "world")?;
        self.world.write_cbor(out)?;
        head::write_text(out, "standpoint")?;
        self.standpoint.write_cbor(out)?;
        head::write_text(out, "source_digest")?;
        self.source_digest.write_cbor(out)?;
        head::write_text(out, "finalized")?;
        self.finalized.write_cbor(out)?;
        Ok(())
    }

    /// Restore exact recorded fields without admission, sorting or identity minting.
    fn read_cbor(input: &mut Reader<'_>) -> Result<Self> {
        input.map(|input, length| {
            if length != 6 {
                return Err(input.invalid("wrong PathObservation field count"));
            }
            input.field("identity")?;
            let identity = ExecutionCbor::read_cbor(input)?;
            input.field("path")?;
            let path = ExecutionCbor::read_cbor(input)?;
            input.field("world")?;
            let world = ExecutionCbor::read_cbor(input)?;
            input.field("standpoint")?;
            let standpoint = ExecutionCbor::read_cbor(input)?;
            input.field("source_digest")?;
            let source_digest = ExecutionCbor::read_cbor(input)?;
            input.field("finalized")?;
            let finalized = ExecutionCbor::read_cbor(input)?;
            Ok(Self {
                identity,
                path,
                world,
                standpoint,
                source_digest,
                finalized,
            })
        })
    }
}

impl ExecutionCbor for Inference {
    /// Preflight the same record visitor and fields used by the typed reader.
    fn check_depth(&self, remaining: usize) -> Result<()> {
        let remaining = descend(remaining)?;
        self.identity.check_depth(remaining)?;
        self.rule.check_depth(remaining)?;
        self.context.check_depth(remaining)?;
        self.antecedents.check_depth(remaining)?;
        Ok(())
    }
    /// Stream every field in this record's serialized declaration order.
    fn write_cbor<W: Write + ?Sized>(&self, out: &mut W) -> io::Result<()> {
        head::write_head(out, head::MAP, 4)?;
        head::write_text(out, "identity")?;
        self.identity.write_cbor(out)?;
        head::write_text(out, "rule")?;
        self.rule.write_cbor(out)?;
        head::write_text(out, "context")?;
        self.context.write_cbor(out)?;
        head::write_text(out, "antecedents")?;
        self.antecedents.write_cbor(out)?;
        Ok(())
    }

    /// Restore exact recorded fields without admission, sorting or identity minting.
    fn read_cbor(input: &mut Reader<'_>) -> Result<Self> {
        input.map(|input, length| {
            if length != 4 {
                return Err(input.invalid("wrong Inference field count"));
            }
            input.field("identity")?;
            let identity = ExecutionCbor::read_cbor(input)?;
            input.field("rule")?;
            let rule = ExecutionCbor::read_cbor(input)?;
            input.field("context")?;
            let context = ExecutionCbor::read_cbor(input)?;
            input.field("antecedents")?;
            let antecedents = ExecutionCbor::read_cbor(input)?;
            Ok(Self {
                identity,
                rule,
                context,
                antecedents,
            })
        })
    }
}

impl ExecutionCbor for AssessmentAnchor {
    /// Preflight the same record visitor and fields used by the typed reader.
    fn check_depth(&self, remaining: usize) -> Result<()> {
        let remaining = descend(remaining)?;
        self.identity.check_depth(remaining)?;
        self.formula_key.check_depth(remaining)?;
        self.instruction.check_depth(remaining)?;
        self.context.check_depth(remaining)?;
        self.context_digest.check_depth(remaining)?;
        Ok(())
    }
    /// Stream every field in this record's serialized declaration order.
    fn write_cbor<W: Write + ?Sized>(&self, out: &mut W) -> io::Result<()> {
        head::write_head(out, head::MAP, 5)?;
        head::write_text(out, "identity")?;
        self.identity.write_cbor(out)?;
        head::write_text(out, "formula_key")?;
        self.formula_key.write_cbor(out)?;
        head::write_text(out, "instruction")?;
        self.instruction.write_cbor(out)?;
        head::write_text(out, "context")?;
        self.context.write_cbor(out)?;
        head::write_text(out, "context_digest")?;
        self.context_digest.write_cbor(out)?;
        Ok(())
    }

    /// Restore exact recorded fields without admission, sorting or identity minting.
    fn read_cbor(input: &mut Reader<'_>) -> Result<Self> {
        input.map(|input, length| {
            if length != 5 {
                return Err(input.invalid("wrong AssessmentAnchor field count"));
            }
            input.field("identity")?;
            let identity = ExecutionCbor::read_cbor(input)?;
            input.field("formula_key")?;
            let formula_key = ExecutionCbor::read_cbor(input)?;
            input.field("instruction")?;
            let instruction = ExecutionCbor::read_cbor(input)?;
            input.field("context")?;
            let context = ExecutionCbor::read_cbor(input)?;
            input.field("context_digest")?;
            let context_digest = ExecutionCbor::read_cbor(input)?;
            Ok(Self {
                identity,
                formula_key,
                instruction,
                context,
                context_digest,
            })
        })
    }
}

impl ExecutionCbor for TemporalBasis {
    /// The adjacent map and temporary kind enum are visited before evidence.
    /// The kind visitor releases its budget; evidence starts at the map's depth.
    fn check_depth(&self, remaining: usize) -> Result<()> {
        let remaining = descend(remaining)?;
        descend(remaining)?;
        match self {
            Self::Journal(value) => value.check_depth(remaining),
            Self::Path(value) => value.check_depth(remaining),
        }
    }
    /// Retain Serde's adjacent kind/evidence map, not an externally tagged variant.
    fn write_cbor<W: Write + ?Sized>(&self, out: &mut W) -> io::Result<()> {
        head::write_head(out, head::MAP, 2)?;
        head::write_text(out, "kind")?;
        match self {
            Self::Journal(value) => {
                head::write_text(out, "Journal")?;
                head::write_text(out, "evidence")?;
                value.write_cbor(out)
            }
            Self::Path(value) => {
                head::write_text(out, "Path")?;
                head::write_text(out, "evidence")?;
                value.write_cbor(out)
            }
        }
    }

    /// The adjacent map consumes one map visitor. Serde's
    /// AdjacentlyTaggedEnumVariantSeed invokes deserialize_enum for the kind's
    /// text, so that temporary unit-enum visitor is charged and released before
    /// evidence decoding. The evidence then consumes its own record visitor.
    fn read_cbor(input: &mut Reader<'_>) -> Result<Self> {
        input.map(|input, length| {
            if length != 2 {
                return Err(input.invalid("wrong TemporalBasis field count"));
            }
            input.field("kind")?;
            let journal = input.variant(|input, name, tagged| match (name, tagged) {
                ("Journal", false) => Ok(true),
                ("Path", false) => Ok(false),
                _ => Err(input.invalid("unknown or non-unit TemporalBasis kind")),
            })?;
            input.field("evidence")?;
            if journal {
                ExecutionCbor::read_cbor(input).map(Self::Journal)
            } else {
                ExecutionCbor::read_cbor(input).map(Self::Path)
            }
        })
    }
}
