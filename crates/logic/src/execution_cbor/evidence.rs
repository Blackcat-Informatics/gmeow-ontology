// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Complete declaration-ordered native execution evidence record.

use std::io::{self, Write};

use purrdf_lex::cbor::head;

use crate::result::NativeExecutionEvidence;

use super::{ExecutionCbor, Reader, Result};

impl ExecutionCbor for NativeExecutionEvidence {
    fn write_cbor<W: Write + ?Sized>(&self, out: &mut W) -> io::Result<()> {
        head::write_head(out, head::MAP, 12)?;
        head::write_text(out, "input_contract")?;
        ExecutionCbor::write_cbor(&self.input_contract, out)?;
        head::write_text(out, "reachability")?;
        ExecutionCbor::write_cbor(&self.reachability, out)?;
        head::write_text(out, "worlds")?;
        ExecutionCbor::write_cbor(&self.worlds, out)?;
        head::write_text(out, "selected_domains")?;
        ExecutionCbor::write_cbor(&self.selected_domains, out)?;
        head::write_text(out, "frontier")?;
        ExecutionCbor::write_cbor(&self.frontier, out)?;
        head::write_text(out, "status")?;
        ExecutionCbor::write_cbor(&self.status, out)?;
        head::write_text(out, "families")?;
        ExecutionCbor::write_cbor(&self.families, out)?;
        head::write_text(out, "class_admission")?;
        ExecutionCbor::write_cbor(&self.class_admission, out)?;
        head::write_text(out, "source_coverage")?;
        ExecutionCbor::write_cbor(&self.source_coverage, out)?;
        head::write_text(out, "classes")?;
        ExecutionCbor::write_cbor(&self.classes, out)?;
        head::write_text(out, "chase_certificates")?;
        ExecutionCbor::write_cbor(&self.chase_certificates, out)?;
        head::write_text(out, "witness_derivations")?;
        ExecutionCbor::write_cbor(&self.witness_derivations, out)?;
        Ok(())
    }

    fn check_depth(&self, remaining: usize) -> Result<()> {
        let remaining = super::descend(remaining)?;
        ExecutionCbor::check_depth(&self.input_contract, remaining)?;
        ExecutionCbor::check_depth(&self.reachability, remaining)?;
        ExecutionCbor::check_depth(&self.worlds, remaining)?;
        ExecutionCbor::check_depth(&self.selected_domains, remaining)?;
        ExecutionCbor::check_depth(&self.frontier, remaining)?;
        ExecutionCbor::check_depth(&self.status, remaining)?;
        ExecutionCbor::check_depth(&self.families, remaining)?;
        ExecutionCbor::check_depth(&self.class_admission, remaining)?;
        ExecutionCbor::check_depth(&self.source_coverage, remaining)?;
        ExecutionCbor::check_depth(&self.classes, remaining)?;
        ExecutionCbor::check_depth(&self.chase_certificates, remaining)?;
        ExecutionCbor::check_depth(&self.witness_derivations, remaining)?;
        Ok(())
    }

    fn read_cbor(input: &mut Reader<'_>) -> Result<Self> {
        input.map(|input, count| {
            if count != 12 {
                return Err(input.invalid("wrong native execution field count"));
            }
            input.field("input_contract")?;
            let input_contract = ExecutionCbor::read_cbor(input)?;
            input.field("reachability")?;
            let reachability = ExecutionCbor::read_cbor(input)?;
            input.field("worlds")?;
            let worlds = ExecutionCbor::read_cbor(input)?;
            input.field("selected_domains")?;
            let selected_domains = ExecutionCbor::read_cbor(input)?;
            input.field("frontier")?;
            let frontier = ExecutionCbor::read_cbor(input)?;
            input.field("status")?;
            let status = ExecutionCbor::read_cbor(input)?;
            input.field("families")?;
            let families = ExecutionCbor::read_cbor(input)?;
            input.field("class_admission")?;
            let class_admission = ExecutionCbor::read_cbor(input)?;
            input.field("source_coverage")?;
            let source_coverage = ExecutionCbor::read_cbor(input)?;
            input.field("classes")?;
            let classes = ExecutionCbor::read_cbor(input)?;
            input.field("chase_certificates")?;
            let chase_certificates = ExecutionCbor::read_cbor(input)?;
            input.field("witness_derivations")?;
            let witness_derivations = ExecutionCbor::read_cbor(input)?;
            Ok(Self {
                input_contract,
                reachability,
                worlds,
                selected_domains,
                frontier,
                status,
                families,
                class_admission,
                source_coverage,
                classes,
                chase_certificates,
                witness_derivations,
            })
        })
    }
}
