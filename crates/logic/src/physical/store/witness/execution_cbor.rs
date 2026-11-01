// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Committed witness scopes, heads and transparent native frontier values.

use std::io::{self, Write};

use purrdf_lex::cbor::head;

use crate::execution_cbor::{ExecutionCbor, Reader, Result, descend};

use super::{
    WitnessDerivation, WitnessHead, WitnessOrigin, WitnessPosition, WitnessScope, WitnessStatement,
};

impl ExecutionCbor for WitnessOrigin {
    /// Keep rule minting distinct from the explicitly selected nonempty-domain law.
    fn write_cbor<W: Write + ?Sized>(&self, out: &mut W) -> io::Result<()> {
        match self {
            Self::Rule => head::write_text(out, "Rule"),
            Self::NonemptyDomain(world) => {
                head::write_head(out, head::MAP, 1)?;
                head::write_text(out, "NonemptyDomain")?;
                ExecutionCbor::write_cbor(world, out)
            }
        }
    }

    /// Check the exact typed-reader depth on borrowed fields without allocation.
    fn check_depth(&self, remaining: usize) -> Result<()> {
        let remaining = descend(remaining)?;
        match self {
            Self::Rule => Ok(()),
            Self::NonemptyDomain(world) => ExecutionCbor::check_depth(world, remaining),
        }
    }

    /// Restore origin data without selecting or validating a domain on its behalf.
    fn read_cbor(input: &mut Reader<'_>) -> Result<Self> {
        input.variant(|input, name, tagged| match (name, tagged) {
            ("Rule", false) => Ok(Self::Rule),
            ("NonemptyDomain", true) => Ok(Self::NonemptyDomain(ExecutionCbor::read_cbor(input)?)),
            _ => Err(input.invalid("unknown WitnessOrigin variant or form")),
        })
    }
}

impl ExecutionCbor for WitnessPosition {
    /// Retain the original enum variant spelling as a text item.
    fn write_cbor<W: Write + ?Sized>(&self, out: &mut W) -> io::Result<()> {
        head::write_text(
            out,
            match self {
                Self::Subject => "Subject",
                Self::Object => "Object",
            },
        )
    }

    /// Check the exact typed-reader depth on borrowed fields without allocation.
    fn check_depth(&self, remaining: usize) -> Result<()> {
        descend(remaining)?;
        Ok(())
    }

    /// Charge the original typed enum visit even though its wire is text.
    fn read_cbor(input: &mut Reader<'_>) -> Result<Self> {
        input.variant(|input, name, tagged| match (name, tagged) {
            ("Subject", false) => Ok(Self::Subject),
            ("Object", false) => Ok(Self::Object),
            _ => Err(input.invalid("unknown WitnessPosition variant or form")),
        })
    }
}

impl ExecutionCbor for WitnessScope {
    /// Retain the complete WitnessScope in its original declaration order.
    fn write_cbor<W: Write + ?Sized>(&self, out: &mut W) -> io::Result<()> {
        head::write_head(out, head::MAP, 4)?;
        head::write_text(out, "world")?;
        ExecutionCbor::write_cbor(&self.world, out)?;
        head::write_text(out, "contract")?;
        ExecutionCbor::write_cbor(&self.contract, out)?;
        head::write_text(out, "source_rule")?;
        ExecutionCbor::write_cbor(&self.source_rule, out)?;
        head::write_text(out, "origin")?;
        ExecutionCbor::write_cbor(&self.origin, out)?;
        Ok(())
    }

    /// Check the exact typed-reader depth on borrowed fields without allocation.
    fn check_depth(&self, remaining: usize) -> Result<()> {
        let remaining = descend(remaining)?;
        ExecutionCbor::check_depth(&self.world, remaining)?;
        ExecutionCbor::check_depth(&self.contract, remaining)?;
        ExecutionCbor::check_depth(&self.source_rule, remaining)?;
        ExecutionCbor::check_depth(&self.origin, remaining)?;
        Ok(())
    }

    /// Reconstruct the exact stored fields without validation or normalization.
    fn read_cbor(input: &mut Reader<'_>) -> Result<Self> {
        input.map(|input, length| {
            if length != 4 {
                return Err(input.invalid("wrong WitnessScope field count"));
            }
            input.field("world")?;
            let world = ExecutionCbor::read_cbor(input)?;
            input.field("contract")?;
            let contract = ExecutionCbor::read_cbor(input)?;
            input.field("source_rule")?;
            let source_rule = ExecutionCbor::read_cbor(input)?;
            input.field("origin")?;
            let origin = ExecutionCbor::read_cbor(input)?;
            Ok(Self {
                world,
                contract,
                source_rule,
                origin,
            })
        })
    }
}

impl ExecutionCbor for WitnessStatement {
    /// Retain the complete WitnessStatement in its original declaration order.
    fn write_cbor<W: Write + ?Sized>(&self, out: &mut W) -> io::Result<()> {
        head::write_head(out, head::MAP, 3)?;
        head::write_text(out, "subject")?;
        ExecutionCbor::write_cbor(&self.subject, out)?;
        head::write_text(out, "predicate")?;
        ExecutionCbor::write_cbor(&self.predicate, out)?;
        head::write_text(out, "object")?;
        ExecutionCbor::write_cbor(&self.object, out)?;
        Ok(())
    }

    /// Check the exact typed-reader depth on borrowed fields without allocation.
    fn check_depth(&self, remaining: usize) -> Result<()> {
        let remaining = descend(remaining)?;
        ExecutionCbor::check_depth(&self.subject, remaining)?;
        ExecutionCbor::check_depth(&self.predicate, remaining)?;
        ExecutionCbor::check_depth(&self.object, remaining)?;
        Ok(())
    }

    /// Reconstruct the exact stored fields without validation or normalization.
    fn read_cbor(input: &mut Reader<'_>) -> Result<Self> {
        input.map(|input, length| {
            if length != 3 {
                return Err(input.invalid("wrong WitnessStatement field count"));
            }
            input.field("subject")?;
            let subject = ExecutionCbor::read_cbor(input)?;
            input.field("predicate")?;
            let predicate = ExecutionCbor::read_cbor(input)?;
            input.field("object")?;
            let object = ExecutionCbor::read_cbor(input)?;
            Ok(Self {
                subject,
                predicate,
                object,
            })
        })
    }
}

impl ExecutionCbor for WitnessHead {
    /// Retain the complete WitnessHead in its original declaration order.
    fn write_cbor<W: Write + ?Sized>(&self, out: &mut W) -> io::Result<()> {
        head::write_head(out, head::MAP, 4)?;
        head::write_text(out, "statement")?;
        ExecutionCbor::write_cbor(&self.statement, out)?;
        head::write_text(out, "positions")?;
        ExecutionCbor::write_cbor(&self.positions, out)?;
        head::write_text(out, "premises")?;
        ExecutionCbor::write_cbor(&self.premises, out)?;
        head::write_text(out, "derivation_id")?;
        ExecutionCbor::write_cbor(&self.derivation_id, out)?;
        Ok(())
    }

    /// Check the exact typed-reader depth on borrowed fields without allocation.
    fn check_depth(&self, remaining: usize) -> Result<()> {
        let remaining = descend(remaining)?;
        ExecutionCbor::check_depth(&self.statement, remaining)?;
        ExecutionCbor::check_depth(&self.positions, remaining)?;
        ExecutionCbor::check_depth(&self.premises, remaining)?;
        ExecutionCbor::check_depth(&self.derivation_id, remaining)?;
        Ok(())
    }

    /// Reconstruct the exact stored fields without validation or normalization.
    fn read_cbor(input: &mut Reader<'_>) -> Result<Self> {
        input.map(|input, length| {
            if length != 4 {
                return Err(input.invalid("wrong WitnessHead field count"));
            }
            input.field("statement")?;
            let statement = ExecutionCbor::read_cbor(input)?;
            input.field("positions")?;
            let positions = ExecutionCbor::read_cbor(input)?;
            input.field("premises")?;
            let premises = ExecutionCbor::read_cbor(input)?;
            input.field("derivation_id")?;
            let derivation_id = ExecutionCbor::read_cbor(input)?;
            Ok(Self {
                statement,
                positions,
                premises,
                derivation_id,
            })
        })
    }
}

impl ExecutionCbor for WitnessDerivation {
    /// Retain the complete witness and its declaration-ordered evidence.
    /// The custom frontier adapter is a transparent vector of retained terms;
    /// it adds neither a wrapper item nor an extra typed visitor.
    fn write_cbor<W: Write + ?Sized>(&self, out: &mut W) -> io::Result<()> {
        head::write_head(out, head::MAP, 6)?;
        head::write_text(out, "witness")?;
        ExecutionCbor::write_cbor(&self.witness, out)?;
        head::write_text(out, "rule_iri")?;
        ExecutionCbor::write_cbor(&self.rule_iri, out)?;
        head::write_text(out, "scope")?;
        ExecutionCbor::write_cbor(&self.scope, out)?;
        head::write_text(out, "ordinal")?;
        ExecutionCbor::write_cbor(&self.ordinal, out)?;
        head::write_text(out, "frontier")?;
        ExecutionCbor::write_cbor(&self.frontier, out)?;
        head::write_text(out, "heads")?;
        ExecutionCbor::write_cbor(&self.heads, out)?;
        Ok(())
    }

    /// Check the exact typed-reader depth on borrowed fields without allocation.
    fn check_depth(&self, remaining: usize) -> Result<()> {
        let remaining = descend(remaining)?;
        ExecutionCbor::check_depth(&self.witness, remaining)?;
        ExecutionCbor::check_depth(&self.rule_iri, remaining)?;
        ExecutionCbor::check_depth(&self.scope, remaining)?;
        ExecutionCbor::check_depth(&self.ordinal, remaining)?;
        ExecutionCbor::check_depth(&self.frontier, remaining)?;
        ExecutionCbor::check_depth(&self.heads, remaining)?;
        Ok(())
    }

    /// Reconstruct the exact stored fields without validation or normalization.
    fn read_cbor(input: &mut Reader<'_>) -> Result<Self> {
        input.map(|input, length| {
            if length != 6 {
                return Err(input.invalid("wrong WitnessDerivation field count"));
            }
            input.field("witness")?;
            let witness = ExecutionCbor::read_cbor(input)?;
            input.field("rule_iri")?;
            let rule_iri = ExecutionCbor::read_cbor(input)?;
            input.field("scope")?;
            let scope = ExecutionCbor::read_cbor(input)?;
            input.field("ordinal")?;
            let ordinal = ExecutionCbor::read_cbor(input)?;
            input.field("frontier")?;
            let frontier = ExecutionCbor::read_cbor(input)?;
            input.field("heads")?;
            let heads = ExecutionCbor::read_cbor(input)?;
            Ok(Self {
                witness,
                rule_iri,
                scope,
                ordinal,
                frontier,
                heads,
            })
        })
    }
}
