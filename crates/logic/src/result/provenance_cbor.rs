// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Exact owned execution-receipt fields; no semantic reconstruction at decoding.

use std::io::{self, Write};

use crate::execution_cbor::{ExecutionCbor, Reader, Result, descend};
use purrdf_lex::cbor::head;

use super::{
    Assumption, BudgetLimit, BudgetUsage, CompletenessStatus, ContradictionWitness, DerivationRef,
    EvaluationStatus, InformationState, InputStatus, ResultClaim, ResultContext, ResultProvenance,
};

impl ExecutionCbor for BudgetUsage {
    /// Preflight the same record visitor and fields used by the typed reader.
    fn check_depth(&self, remaining: usize) -> Result<()> {
        let remaining = descend(remaining)?;
        self.consumed.check_depth(remaining)?;
        self.allowance.check_depth(remaining)?;
        self.limit.check_depth(remaining)?;
        Ok(())
    }
    /// Stream every field in this record's serialized declaration order.
    fn write_cbor<W: Write + ?Sized>(&self, out: &mut W) -> io::Result<()> {
        head::write_head(out, head::MAP, 3)?;
        head::write_text(out, "consumed")?;
        self.consumed.write_cbor(out)?;
        head::write_text(out, "allowance")?;
        self.allowance.write_cbor(out)?;
        head::write_text(out, "limit")?;
        self.limit.write_cbor(out)?;
        Ok(())
    }

    /// Restore exact recorded fields without admission, sorting or identity minting.
    fn read_cbor(input: &mut Reader<'_>) -> Result<Self> {
        input.map(|input, length| {
            if length != 3 {
                return Err(input.invalid("wrong BudgetUsage field count"));
            }
            input.field("consumed")?;
            let consumed = ExecutionCbor::read_cbor(input)?;
            input.field("allowance")?;
            let allowance = ExecutionCbor::read_cbor(input)?;
            input.field("limit")?;
            let limit = ExecutionCbor::read_cbor(input)?;
            Ok(Self {
                consumed,
                allowance,
                limit,
            })
        })
    }
}

impl ExecutionCbor for DerivationRef {
    /// Preflight the same record visitor and fields used by the typed reader.
    fn check_depth(&self, remaining: usize) -> Result<()> {
        let remaining = descend(remaining)?;
        self.derivation_id.check_depth(remaining)?;
        self.cited_iris.check_depth(remaining)?;
        Ok(())
    }
    /// Stream every field in this record's serialized declaration order.
    fn write_cbor<W: Write + ?Sized>(&self, out: &mut W) -> io::Result<()> {
        head::write_head(out, head::MAP, 2)?;
        head::write_text(out, "derivation_id")?;
        self.derivation_id.write_cbor(out)?;
        head::write_text(out, "cited_iris")?;
        self.cited_iris.write_cbor(out)?;
        Ok(())
    }

    /// Restore exact recorded fields without admission, sorting or identity minting.
    fn read_cbor(input: &mut Reader<'_>) -> Result<Self> {
        input.map(|input, length| {
            if length != 2 {
                return Err(input.invalid("wrong DerivationRef field count"));
            }
            input.field("derivation_id")?;
            let derivation_id = ExecutionCbor::read_cbor(input)?;
            input.field("cited_iris")?;
            let cited_iris = ExecutionCbor::read_cbor(input)?;
            Ok(Self {
                derivation_id,
                cited_iris,
            })
        })
    }
}

impl ExecutionCbor for ResultContext {
    /// Preflight the same record visitor and fields used by the typed reader.
    fn check_depth(&self, remaining: usize) -> Result<()> {
        let remaining = descend(remaining)?;
        self.world.check_depth(remaining)?;
        self.standpoint.check_depth(remaining)?;
        self.attributed.check_depth(remaining)?;
        self.time.check_depth(remaining)?;
        self.path.check_depth(remaining)?;
        Ok(())
    }
    /// Stream every field in this record's serialized declaration order.
    fn write_cbor<W: Write + ?Sized>(&self, out: &mut W) -> io::Result<()> {
        head::write_head(out, head::MAP, 5)?;
        head::write_text(out, "world")?;
        self.world.write_cbor(out)?;
        head::write_text(out, "standpoint")?;
        self.standpoint.write_cbor(out)?;
        head::write_text(out, "attributed")?;
        self.attributed.write_cbor(out)?;
        head::write_text(out, "time")?;
        self.time.write_cbor(out)?;
        head::write_text(out, "path")?;
        self.path.write_cbor(out)?;
        Ok(())
    }

    /// Restore exact recorded fields without admission, sorting or identity minting.
    fn read_cbor(input: &mut Reader<'_>) -> Result<Self> {
        input.map(|input, length| {
            if length != 5 {
                return Err(input.invalid("wrong ResultContext field count"));
            }
            input.field("world")?;
            let world = ExecutionCbor::read_cbor(input)?;
            input.field("standpoint")?;
            let standpoint = ExecutionCbor::read_cbor(input)?;
            input.field("attributed")?;
            let attributed = ExecutionCbor::read_cbor(input)?;
            input.field("time")?;
            let time = ExecutionCbor::read_cbor(input)?;
            input.field("path")?;
            let path = ExecutionCbor::read_cbor(input)?;
            Ok(Self {
                world,
                standpoint,
                attributed,
                time,
                path,
            })
        })
    }
}

impl ExecutionCbor for ContradictionWitness {
    /// Preflight the same record visitor and fields used by the typed reader.
    fn check_depth(&self, remaining: usize) -> Result<()> {
        let remaining = descend(remaining)?;
        self.individual.check_depth(remaining)?;
        self.world.check_depth(remaining)?;
        self.premises.check_depth(remaining)?;
        Ok(())
    }
    /// Stream every field in this record's serialized declaration order.
    fn write_cbor<W: Write + ?Sized>(&self, out: &mut W) -> io::Result<()> {
        head::write_head(out, head::MAP, 3)?;
        head::write_text(out, "individual")?;
        self.individual.write_cbor(out)?;
        head::write_text(out, "world")?;
        self.world.write_cbor(out)?;
        head::write_text(out, "premises")?;
        self.premises.write_cbor(out)?;
        Ok(())
    }

    /// Restore exact recorded fields without admission, sorting or identity minting.
    fn read_cbor(input: &mut Reader<'_>) -> Result<Self> {
        input.map(|input, length| {
            if length != 3 {
                return Err(input.invalid("wrong ContradictionWitness field count"));
            }
            input.field("individual")?;
            let individual = ExecutionCbor::read_cbor(input)?;
            input.field("world")?;
            let world = ExecutionCbor::read_cbor(input)?;
            input.field("premises")?;
            let premises = ExecutionCbor::read_cbor(input)?;
            Ok(Self {
                individual,
                world,
                premises,
            })
        })
    }
}

// Native execution stays recursive in this exact wire, even where a semantic
// producer forbids nesting it inside its contextual judgment.
impl ExecutionCbor for ResultProvenance {
    /// Preflight the same record visitor and fields used by the typed reader.
    fn check_depth(&self, remaining: usize) -> Result<()> {
        let remaining = descend(remaining)?;
        self.claim.check_depth(remaining)?;
        self.contract_hash.check_depth(remaining)?;
        self.query.check_depth(remaining)?;
        self.conclusion.check_depth(remaining)?;
        self.proof.check_depth(remaining)?;
        self.counterproof.check_depth(remaining)?;
        self.context.check_depth(remaining)?;
        self.engine.check_depth(remaining)?;
        self.consumed_budget.check_depth(remaining)?;
        self.certified_fragment.check_depth(remaining)?;
        self.projection_class.check_depth(remaining)?;
        self.contradiction_witnesses.check_depth(remaining)?;
        self.native_execution.check_depth(remaining)?;
        self.assumptions.check_depth(remaining)?;
        Ok(())
    }
    /// Stream every field in this record's serialized declaration order.
    fn write_cbor<W: Write + ?Sized>(&self, out: &mut W) -> io::Result<()> {
        head::write_head(out, head::MAP, 14)?;
        head::write_text(out, "claim")?;
        self.claim.write_cbor(out)?;
        head::write_text(out, "contract_hash")?;
        self.contract_hash.write_cbor(out)?;
        head::write_text(out, "query")?;
        self.query.write_cbor(out)?;
        head::write_text(out, "conclusion")?;
        self.conclusion.write_cbor(out)?;
        head::write_text(out, "proof")?;
        self.proof.write_cbor(out)?;
        head::write_text(out, "counterproof")?;
        self.counterproof.write_cbor(out)?;
        head::write_text(out, "context")?;
        self.context.write_cbor(out)?;
        head::write_text(out, "engine")?;
        self.engine.write_cbor(out)?;
        head::write_text(out, "consumed_budget")?;
        self.consumed_budget.write_cbor(out)?;
        head::write_text(out, "certified_fragment")?;
        self.certified_fragment.write_cbor(out)?;
        head::write_text(out, "projection_class")?;
        self.projection_class.write_cbor(out)?;
        head::write_text(out, "contradiction_witnesses")?;
        self.contradiction_witnesses.write_cbor(out)?;
        head::write_text(out, "native_execution")?;
        self.native_execution.write_cbor(out)?;
        head::write_text(out, "assumptions")?;
        self.assumptions.write_cbor(out)?;
        Ok(())
    }

    /// Restore exact recorded fields without admission, sorting or identity minting.
    fn read_cbor(input: &mut Reader<'_>) -> Result<Self> {
        input.map(|input, length| {
            if length != 14 {
                return Err(input.invalid("wrong ResultProvenance field count"));
            }
            input.field("claim")?;
            let claim = ExecutionCbor::read_cbor(input)?;
            input.field("contract_hash")?;
            let contract_hash = ExecutionCbor::read_cbor(input)?;
            input.field("query")?;
            let query = ExecutionCbor::read_cbor(input)?;
            input.field("conclusion")?;
            let conclusion = ExecutionCbor::read_cbor(input)?;
            input.field("proof")?;
            let proof = ExecutionCbor::read_cbor(input)?;
            input.field("counterproof")?;
            let counterproof = ExecutionCbor::read_cbor(input)?;
            input.field("context")?;
            let context = ExecutionCbor::read_cbor(input)?;
            input.field("engine")?;
            let engine = ExecutionCbor::read_cbor(input)?;
            input.field("consumed_budget")?;
            let consumed_budget = ExecutionCbor::read_cbor(input)?;
            input.field("certified_fragment")?;
            let certified_fragment = ExecutionCbor::read_cbor(input)?;
            input.field("projection_class")?;
            let projection_class = ExecutionCbor::read_cbor(input)?;
            input.field("contradiction_witnesses")?;
            let contradiction_witnesses = ExecutionCbor::read_cbor(input)?;
            input.field("native_execution")?;
            let native_execution = ExecutionCbor::read_cbor(input)?;
            input.field("assumptions")?;
            let assumptions = ExecutionCbor::read_cbor(input)?;
            Ok(Self {
                claim,
                contract_hash,
                query,
                conclusion,
                proof,
                counterproof,
                context,
                engine,
                consumed_budget,
                certified_fragment,
                projection_class,
                contradiction_witnesses,
                native_execution,
                assumptions,
            })
        })
    }
}

impl ExecutionCbor for InputStatus {
    crate::__gmeow_execution_unit_depth!(Valid, Invalid);
    /// Retain the original unit-variant text, independent of projection vocabularies.
    fn write_cbor<W: Write + ?Sized>(&self, out: &mut W) -> io::Result<()> {
        head::write_text(
            out,
            match self {
                Self::Valid => "Valid",
                Self::Invalid => "Invalid",
            },
        )
    }

    /// Match the original unit enum visitor, including its typed recursion charge.
    fn read_cbor(input: &mut Reader<'_>) -> Result<Self> {
        input.variant(|input, name, tagged| match (name, tagged) {
            ("Valid", false) => Ok(Self::Valid),
            ("Invalid", false) => Ok(Self::Invalid),
            _ => Err(input.invalid("unknown or non-unit InputStatus variant")),
        })
    }
}

impl ExecutionCbor for EvaluationStatus {
    crate::__gmeow_execution_unit_depth!(Completed, BudgetExhausted, Unsupported);
    /// Retain the original unit-variant text, independent of projection vocabularies.
    fn write_cbor<W: Write + ?Sized>(&self, out: &mut W) -> io::Result<()> {
        head::write_text(
            out,
            match self {
                Self::Completed => "Completed",
                Self::BudgetExhausted => "BudgetExhausted",
                Self::Unsupported => "Unsupported",
            },
        )
    }

    /// Match the original unit enum visitor, including its typed recursion charge.
    fn read_cbor(input: &mut Reader<'_>) -> Result<Self> {
        input.variant(|input, name, tagged| match (name, tagged) {
            ("Completed", false) => Ok(Self::Completed),
            ("BudgetExhausted", false) => Ok(Self::BudgetExhausted),
            ("Unsupported", false) => Ok(Self::Unsupported),
            _ => Err(input.invalid("unknown or non-unit EvaluationStatus variant")),
        })
    }
}

impl ExecutionCbor for CompletenessStatus {
    crate::__gmeow_execution_unit_depth!(CompleteForFragment, Incomplete, Unknown);
    /// Retain the original unit-variant text, independent of projection vocabularies.
    fn write_cbor<W: Write + ?Sized>(&self, out: &mut W) -> io::Result<()> {
        head::write_text(
            out,
            match self {
                Self::CompleteForFragment => "CompleteForFragment",
                Self::Incomplete => "Incomplete",
                Self::Unknown => "Unknown",
            },
        )
    }

    /// Match the original unit enum visitor, including its typed recursion charge.
    fn read_cbor(input: &mut Reader<'_>) -> Result<Self> {
        input.variant(|input, name, tagged| match (name, tagged) {
            ("CompleteForFragment", false) => Ok(Self::CompleteForFragment),
            ("Incomplete", false) => Ok(Self::Incomplete),
            ("Unknown", false) => Ok(Self::Unknown),
            _ => Err(input.invalid("unknown or non-unit CompletenessStatus variant")),
        })
    }
}

impl ExecutionCbor for InformationState {
    crate::__gmeow_execution_unit_depth!(
        Supported,
        Opposed,
        Both,
        Neither,
        Undetermined,
        NotEvaluated
    );
    /// Retain the original unit-variant text, independent of projection vocabularies.
    fn write_cbor<W: Write + ?Sized>(&self, out: &mut W) -> io::Result<()> {
        head::write_text(
            out,
            match self {
                Self::Supported => "Supported",
                Self::Opposed => "Opposed",
                Self::Both => "Both",
                Self::Neither => "Neither",
                Self::Undetermined => "Undetermined",
                Self::NotEvaluated => "NotEvaluated",
            },
        )
    }

    /// Match the original unit enum visitor, including its typed recursion charge.
    fn read_cbor(input: &mut Reader<'_>) -> Result<Self> {
        input.variant(|input, name, tagged| match (name, tagged) {
            ("Supported", false) => Ok(Self::Supported),
            ("Opposed", false) => Ok(Self::Opposed),
            ("Both", false) => Ok(Self::Both),
            ("Neither", false) => Ok(Self::Neither),
            ("Undetermined", false) => Ok(Self::Undetermined),
            ("NotEvaluated", false) => Ok(Self::NotEvaluated),
            _ => Err(input.invalid("unknown or non-unit InformationState variant")),
        })
    }
}

impl ExecutionCbor for BudgetLimit {
    crate::__gmeow_execution_unit_depth!(Answers, Inference, Depth);
    /// Retain the original unit-variant text, independent of projection vocabularies.
    fn write_cbor<W: Write + ?Sized>(&self, out: &mut W) -> io::Result<()> {
        head::write_text(
            out,
            match self {
                Self::Answers => "Answers",
                Self::Inference => "Inference",
                Self::Depth => "Depth",
            },
        )
    }

    /// Match the original unit enum visitor, including its typed recursion charge.
    fn read_cbor(input: &mut Reader<'_>) -> Result<Self> {
        input.variant(|input, name, tagged| match (name, tagged) {
            ("Answers", false) => Ok(Self::Answers),
            ("Inference", false) => Ok(Self::Inference),
            ("Depth", false) => Ok(Self::Depth),
            _ => Err(input.invalid("unknown or non-unit BudgetLimit variant")),
        })
    }
}

impl ExecutionCbor for Assumption {
    crate::__gmeow_execution_unit_depth!(
        ClosedWorld,
        OpenWorld,
        UniqueName,
        EntrenchmentRevision,
        SkolemWitness
    );
    /// Retain the original unit-variant text, independent of projection vocabularies.
    fn write_cbor<W: Write + ?Sized>(&self, out: &mut W) -> io::Result<()> {
        head::write_text(
            out,
            match self {
                Self::ClosedWorld => "ClosedWorld",
                Self::OpenWorld => "OpenWorld",
                Self::UniqueName => "UniqueName",
                Self::EntrenchmentRevision => "EntrenchmentRevision",
                Self::SkolemWitness => "SkolemWitness",
            },
        )
    }

    /// Match the original unit enum visitor, including its typed recursion charge.
    fn read_cbor(input: &mut Reader<'_>) -> Result<Self> {
        input.variant(|input, name, tagged| match (name, tagged) {
            ("ClosedWorld", false) => Ok(Self::ClosedWorld),
            ("OpenWorld", false) => Ok(Self::OpenWorld),
            ("UniqueName", false) => Ok(Self::UniqueName),
            ("EntrenchmentRevision", false) => Ok(Self::EntrenchmentRevision),
            ("SkolemWitness", false) => Ok(Self::SkolemWitness),
            _ => Err(input.invalid("unknown or non-unit Assumption variant")),
        })
    }
}

impl ExecutionCbor for ResultClaim {
    crate::__gmeow_execution_unit_depth!(Conclusion, WorldConsistency);
    /// Retain the original unit-variant text, independent of projection vocabularies.
    fn write_cbor<W: Write + ?Sized>(&self, out: &mut W) -> io::Result<()> {
        head::write_text(
            out,
            match self {
                Self::Conclusion => "Conclusion",
                Self::WorldConsistency => "WorldConsistency",
            },
        )
    }

    /// Match the original unit enum visitor, including its typed recursion charge.
    fn read_cbor(input: &mut Reader<'_>) -> Result<Self> {
        input.variant(|input, name, tagged| match (name, tagged) {
            ("Conclusion", false) => Ok(Self::Conclusion),
            ("WorldConsistency", false) => Ok(Self::WorldConsistency),
            _ => Err(input.invalid("unknown or non-unit ResultClaim variant")),
        })
    }
}
