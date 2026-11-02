// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Exact owned execution-receipt fields; no semantic reconstruction at decoding.

use std::io;

use crate::execution_cbor::{ExecutionCbor, Reader, Result, descend};
use purrdf_lex::cbor::head;

use super::{
    NativeContextualAnalysisStop, NativeContextualJudgment, NativeContextualReceipt,
    NativeContextualUpstreamStop,
};

impl ExecutionCbor for NativeContextualAnalysisStop {
    /// Preflight the same record visitor and fields used by the typed reader.
    fn check_depth(&self, remaining: usize) -> Result<()> {
        let remaining = descend(remaining)?;
        self.world.check_depth(remaining)?;
        self.graph.check_depth(remaining)?;
        self.usage.check_depth(remaining)?;
        self.class_resource_obstructions.check_depth(remaining)?;
        Ok(())
    }
    /// Stream every field in this record's serialized declaration order.
    fn write_cbor<W: crate::execution_cbor::LeafWrite + ?Sized>(
        &self,
        out: &mut W,
    ) -> io::Result<()> {
        head::write_head(out, head::MAP, 4)?;
        head::write_text(out, "world")?;
        self.world.write_cbor(out)?;
        head::write_text(out, "graph")?;
        self.graph.write_cbor(out)?;
        head::write_text(out, "usage")?;
        self.usage.write_cbor(out)?;
        head::write_text(out, "class_resource_obstructions")?;
        self.class_resource_obstructions.write_cbor(out)?;
        Ok(())
    }

    /// Restore exact recorded fields without admission, sorting or identity minting.
    fn read_cbor(input: &mut Reader<'_>) -> Result<Self> {
        input.map(|input, length| {
            if length != 4 {
                return Err(input.invalid("wrong NativeContextualAnalysisStop field count"));
            }
            input.field("world")?;
            let world = ExecutionCbor::read_cbor(input)?;
            input.field("graph")?;
            let graph = ExecutionCbor::read_cbor(input)?;
            input.field("usage")?;
            let usage = ExecutionCbor::read_cbor(input)?;
            input.field("class_resource_obstructions")?;
            let class_resource_obstructions = ExecutionCbor::read_cbor(input)?;
            Ok(Self {
                world,
                graph,
                usage,
                class_resource_obstructions,
            })
        })
    }
}

impl ExecutionCbor for NativeContextualJudgment {
    /// Preflight the same record visitor and fields used by the typed reader.
    fn check_depth(&self, remaining: usize) -> Result<()> {
        let remaining = descend(remaining)?;
        self.request.check_depth(remaining)?;
        self.formula.check_depth(remaining)?;
        self.evaluation.check_depth(remaining)?;
        self.completeness.check_depth(remaining)?;
        self.preservation.check_depth(remaining)?;
        self.information.check_depth(remaining)?;
        self.provenance.check_depth(remaining)?;
        self.interrupted.check_depth(remaining)?;
        self.upstream_stop.check_depth(remaining)?;
        self.inferences.check_depth(remaining)?;
        self.anchors.check_depth(remaining)?;
        self.temporal_prefixes.check_depth(remaining)?;
        self.native_evidence.check_depth(remaining)?;
        Ok(())
    }
    /// Stream every field in this record's serialized declaration order.
    fn write_cbor<W: crate::execution_cbor::LeafWrite + ?Sized>(
        &self,
        out: &mut W,
    ) -> io::Result<()> {
        head::write_head(out, head::MAP, 13)?;
        head::write_text(out, "request")?;
        self.request.write_cbor(out)?;
        head::write_text(out, "formula")?;
        self.formula.write_cbor(out)?;
        head::write_text(out, "evaluation")?;
        self.evaluation.write_cbor(out)?;
        head::write_text(out, "completeness")?;
        self.completeness.write_cbor(out)?;
        head::write_text(out, "preservation")?;
        self.preservation.write_cbor(out)?;
        head::write_text(out, "information")?;
        self.information.write_cbor(out)?;
        head::write_text(out, "provenance")?;
        self.provenance.write_cbor(out)?;
        head::write_text(out, "interrupted")?;
        self.interrupted.write_cbor(out)?;
        head::write_text(out, "upstream_stop")?;
        self.upstream_stop.write_cbor(out)?;
        head::write_text(out, "inferences")?;
        self.inferences.write_cbor(out)?;
        head::write_text(out, "anchors")?;
        self.anchors.write_cbor(out)?;
        head::write_text(out, "temporal_prefixes")?;
        self.temporal_prefixes.write_cbor(out)?;
        head::write_text(out, "native_evidence")?;
        self.native_evidence.write_cbor(out)?;
        Ok(())
    }

    /// Restore exact recorded fields without admission, sorting or identity minting.
    fn read_cbor(input: &mut Reader<'_>) -> Result<Self> {
        input.map(|input, length| {
            if length != 13 {
                return Err(input.invalid("wrong NativeContextualJudgment field count"));
            }
            input.field("request")?;
            let request = ExecutionCbor::read_cbor(input)?;
            input.field("formula")?;
            let formula = ExecutionCbor::read_cbor(input)?;
            input.field("evaluation")?;
            let evaluation = ExecutionCbor::read_cbor(input)?;
            input.field("completeness")?;
            let completeness = ExecutionCbor::read_cbor(input)?;
            input.field("preservation")?;
            let preservation = ExecutionCbor::read_cbor(input)?;
            input.field("information")?;
            let information = ExecutionCbor::read_cbor(input)?;
            input.field("provenance")?;
            let provenance = ExecutionCbor::read_cbor(input)?;
            input.field("interrupted")?;
            let interrupted = ExecutionCbor::read_cbor(input)?;
            input.field("upstream_stop")?;
            let upstream_stop = ExecutionCbor::read_cbor(input)?;
            input.field("inferences")?;
            let inferences = ExecutionCbor::read_cbor(input)?;
            input.field("anchors")?;
            let anchors = ExecutionCbor::read_cbor(input)?;
            input.field("temporal_prefixes")?;
            let temporal_prefixes = ExecutionCbor::read_cbor(input)?;
            input.field("native_evidence")?;
            let native_evidence = ExecutionCbor::read_cbor(input)?;
            Ok(Self {
                request,
                formula,
                evaluation,
                completeness,
                preservation,
                information,
                provenance,
                interrupted,
                upstream_stop,
                inferences,
                anchors,
                temporal_prefixes,
                native_evidence,
            })
        })
    }
}

impl ExecutionCbor for NativeContextualReceipt {
    /// Preflight the same record visitor and fields used by the typed reader.
    fn check_depth(&self, remaining: usize) -> Result<()> {
        let remaining = descend(remaining)?;
        self.id.check_depth(remaining)?;
        self.input_contract.check_depth(remaining)?;
        self.judgment.check_depth(remaining)?;
        self.premises.check_depth(remaining)?;
        self.supports.check_depth(remaining)?;
        self.statements.check_depth(remaining)?;
        Ok(())
    }
    /// Stream every field in this record's serialized declaration order.
    fn write_cbor<W: crate::execution_cbor::LeafWrite + ?Sized>(
        &self,
        out: &mut W,
    ) -> io::Result<()> {
        head::write_head(out, head::MAP, 6)?;
        head::write_text(out, "id")?;
        self.id.write_cbor(out)?;
        head::write_text(out, "input_contract")?;
        self.input_contract.write_cbor(out)?;
        head::write_text(out, "judgment")?;
        self.judgment.write_cbor(out)?;
        head::write_text(out, "premises")?;
        self.premises.write_cbor(out)?;
        head::write_text(out, "supports")?;
        self.supports.write_cbor(out)?;
        head::write_text(out, "statements")?;
        self.statements.write_cbor(out)?;
        Ok(())
    }

    /// Restore exact recorded fields without admission, sorting or identity minting.
    fn read_cbor(input: &mut Reader<'_>) -> Result<Self> {
        input.map(|input, length| {
            if length != 6 {
                return Err(input.invalid("wrong NativeContextualReceipt field count"));
            }
            input.field("id")?;
            let id = ExecutionCbor::read_cbor(input)?;
            input.field("input_contract")?;
            let input_contract = ExecutionCbor::read_cbor(input)?;
            input.field("judgment")?;
            let judgment = ExecutionCbor::read_cbor(input)?;
            input.field("premises")?;
            let premises = ExecutionCbor::read_cbor(input)?;
            input.field("supports")?;
            let supports = ExecutionCbor::read_cbor(input)?;
            input.field("statements")?;
            let statements = ExecutionCbor::read_cbor(input)?;
            Ok(Self {
                id,
                input_contract,
                judgment,
                premises,
                supports,
                statements,
            })
        })
    }
}

impl ExecutionCbor for NativeContextualUpstreamStop {
    /// The enum visitor remains open during a tagged struct payload visitor.
    fn check_depth(&self, remaining: usize) -> Result<()> {
        let remaining = descend(remaining)?;
        match self {
            Self::InferenceExhausted => Ok(()),
            Self::AnalysisExhausted { analyses } => {
                let remaining = descend(remaining)?;
                analyses.check_depth(remaining)?;
                Ok(())
            }
            Self::Blocked { reads } => {
                let remaining = descend(remaining)?;
                reads.check_depth(remaining)?;
                Ok(())
            }
        }
    }
    /// Preserve the original unit text or externally tagged struct-variant wire.
    fn write_cbor<W: crate::execution_cbor::LeafWrite + ?Sized>(
        &self,
        out: &mut W,
    ) -> io::Result<()> {
        match self {
            Self::InferenceExhausted => head::write_text(out, "InferenceExhausted"),
            Self::AnalysisExhausted { analyses } => {
                head::write_head(out, head::MAP, 1)?;
                head::write_text(out, "AnalysisExhausted")?;
                head::write_head(out, head::MAP, 1)?;
                head::write_text(out, "analyses")?;
                analyses.write_cbor(out)?;
                Ok(())
            }
            Self::Blocked { reads } => {
                head::write_head(out, head::MAP, 1)?;
                head::write_text(out, "Blocked")?;
                head::write_head(out, head::MAP, 1)?;
                head::write_text(out, "reads")?;
                reads.write_cbor(out)?;
                Ok(())
            }
        }
    }

    /// The tagged enum and its struct payload each consume their original visitor.
    fn read_cbor(input: &mut Reader<'_>) -> Result<Self> {
        input.variant(|input, name, tagged| match (name, tagged) {
            ("InferenceExhausted", false) => Ok(Self::InferenceExhausted),
            ("AnalysisExhausted", true) => input.map(|input, length| {
                if length != 1 {
                    return Err(input.invalid(
                        "wrong NativeContextualUpstreamStop::AnalysisExhausted field count",
                    ));
                }
                input.field("analyses")?;
                let analyses = ExecutionCbor::read_cbor(input)?;
                Ok(Self::AnalysisExhausted { analyses })
            }),
            ("Blocked", true) => input.map(|input, length| {
                if length != 1 {
                    return Err(
                        input.invalid("wrong NativeContextualUpstreamStop::Blocked field count")
                    );
                }
                input.field("reads")?;
                let reads = ExecutionCbor::read_cbor(input)?;
                Ok(Self::Blocked { reads })
            }),
            _ => {
                Err(input
                    .invalid("unknown or incorrectly framed NativeContextualUpstreamStop variant"))
            }
        })
    }
}
