// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Exact owned execution-receipt fields; no semantic reconstruction at decoding.

use std::io::{self, Write};

use crate::execution_cbor::{ExecutionCbor, Reader, Result, descend};
use purrdf_lex::cbor::head;

use super::{AxiomReceipt, Row};

impl ExecutionCbor for Row {
    /// Preflight the same record visitor and fields used by the typed reader.
    fn check_depth(&self, remaining: usize) -> Result<()> {
        let remaining = descend(remaining)?;
        self.modal_evaluation.check_depth(remaining)?;
        self.graph.check_depth(remaining)?;
        self.subject.check_depth(remaining)?;
        self.predicate.check_depth(remaining)?;
        self.obj.check_depth(remaining)?;
        self.derivation_id.check_depth(remaining)?;
        self.rule_iri.check_depth(remaining)?;
        self.source_quad_ids.check_depth(remaining)?;
        Ok(())
    }
    /// Stream every field in this record's serialized declaration order.
    fn write_cbor<W: Write + ?Sized>(&self, out: &mut W) -> io::Result<()> {
        head::write_head(out, head::MAP, 8)?;
        head::write_text(out, "modal_evaluation")?;
        self.modal_evaluation.write_cbor(out)?;
        head::write_text(out, "graph")?;
        self.graph.write_cbor(out)?;
        head::write_text(out, "subject")?;
        self.subject.write_cbor(out)?;
        head::write_text(out, "predicate")?;
        self.predicate.write_cbor(out)?;
        head::write_text(out, "obj")?;
        self.obj.write_cbor(out)?;
        head::write_text(out, "derivation_id")?;
        self.derivation_id.write_cbor(out)?;
        head::write_text(out, "rule_iri")?;
        self.rule_iri.write_cbor(out)?;
        head::write_text(out, "source_quad_ids")?;
        self.source_quad_ids.write_cbor(out)?;
        Ok(())
    }

    /// Restore exact recorded fields without admission, sorting or identity minting.
    fn read_cbor(input: &mut Reader<'_>) -> Result<Self> {
        input.map(|input, length| {
            if length != 8 {
                return Err(input.invalid("wrong Row field count"));
            }
            input.field("modal_evaluation")?;
            let modal_evaluation = ExecutionCbor::read_cbor(input)?;
            input.field("graph")?;
            let graph = ExecutionCbor::read_cbor(input)?;
            input.field("subject")?;
            let subject = ExecutionCbor::read_cbor(input)?;
            input.field("predicate")?;
            let predicate = ExecutionCbor::read_cbor(input)?;
            input.field("obj")?;
            let obj = ExecutionCbor::read_cbor(input)?;
            input.field("derivation_id")?;
            let derivation_id = ExecutionCbor::read_cbor(input)?;
            input.field("rule_iri")?;
            let rule_iri = ExecutionCbor::read_cbor(input)?;
            input.field("source_quad_ids")?;
            let source_quad_ids = ExecutionCbor::read_cbor(input)?;
            Ok(Self {
                modal_evaluation,
                graph,
                subject,
                predicate,
                obj,
                derivation_id,
                rule_iri,
                source_quad_ids,
            })
        })
    }
}

impl ExecutionCbor for AxiomReceipt {
    /// Preflight the same record visitor and fields used by the typed reader.
    fn check_depth(&self, remaining: usize) -> Result<()> {
        let remaining = descend(remaining)?;
        self.row.check_depth(remaining)?;
        self.raw_rule_identity.check_depth(remaining)?;
        Ok(())
    }
    /// Stream every field in this record's serialized declaration order.
    fn write_cbor<W: Write + ?Sized>(&self, out: &mut W) -> io::Result<()> {
        head::write_head(out, head::MAP, 2)?;
        head::write_text(out, "row")?;
        self.row.write_cbor(out)?;
        head::write_text(out, "raw_rule_identity")?;
        self.raw_rule_identity.write_cbor(out)?;
        Ok(())
    }

    /// Restore exact recorded fields without admission, sorting or identity minting.
    fn read_cbor(input: &mut Reader<'_>) -> Result<Self> {
        input.map(|input, length| {
            if length != 2 {
                return Err(input.invalid("wrong AxiomReceipt field count"));
            }
            input.field("row")?;
            let row = ExecutionCbor::read_cbor(input)?;
            input.field("raw_rule_identity")?;
            let raw_rule_identity = ExecutionCbor::read_cbor(input)?;
            Ok(Self {
                row,
                raw_rule_identity,
            })
        })
    }
}
