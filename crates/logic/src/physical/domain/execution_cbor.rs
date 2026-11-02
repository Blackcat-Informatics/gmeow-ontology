// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Original logical worlds and explicit caller domain selections.

use std::io;

use purrdf_lex::cbor::head;

use crate::execution_cbor::{ExecutionCbor, Reader, Result, descend};

use super::{DomainProfile, LogicalGraph, SelectedDomains, SelectedLogicalWorld};

impl ExecutionCbor for LogicalGraph {
    /// Retain the original default graph or complete retained named-graph term.
    fn write_cbor<W: crate::execution_cbor::LeafWrite + ?Sized>(
        &self,
        out: &mut W,
    ) -> io::Result<()> {
        match self {
            Self::Default => head::write_text(out, "Default"),
            Self::Named(graph) => {
                head::write_head(out, head::MAP, 1)?;
                head::write_text(out, "Named")?;
                ExecutionCbor::write_cbor(graph, out)
            }
        }
    }

    /// Check the exact typed-reader depth on borrowed fields without allocation.
    fn check_depth(&self, remaining: usize) -> Result<()> {
        let remaining = descend(remaining)?;
        match self {
            Self::Default => Ok(()),
            Self::Named(graph) => ExecutionCbor::check_depth(graph, remaining),
        }
    }

    /// Restore the retained graph without projection, skolemization or selection.
    fn read_cbor(input: &mut Reader<'_>) -> Result<Self> {
        input.variant(|input, name, tagged| match (name, tagged) {
            ("Default", false) => Ok(Self::Default),
            ("Named", true) => Ok(Self::Named(ExecutionCbor::read_cbor(input)?)),
            _ => Err(input.invalid("unknown LogicalGraph variant or form")),
        })
    }
}

impl ExecutionCbor for DomainProfile {
    /// Retain the original enum variant spelling as a text item.
    fn write_cbor<W: crate::execution_cbor::LeafWrite + ?Sized>(
        &self,
        out: &mut W,
    ) -> io::Result<()> {
        head::write_text(
            out,
            match self {
                Self::NonemptyObjectDomainV1 => "NonemptyObjectDomainV1",
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
            ("NonemptyObjectDomainV1", false) => Ok(Self::NonemptyObjectDomainV1),
            _ => Err(input.invalid("unknown DomainProfile variant or form")),
        })
    }
}

impl ExecutionCbor for SelectedLogicalWorld {
    /// Retain the complete SelectedLogicalWorld in its original declaration order.
    fn write_cbor<W: crate::execution_cbor::LeafWrite + ?Sized>(
        &self,
        out: &mut W,
    ) -> io::Result<()> {
        head::write_head(out, head::MAP, 4)?;
        head::write_text(out, "graph")?;
        ExecutionCbor::write_cbor(&self.graph, out)?;
        head::write_text(out, "profile")?;
        ExecutionCbor::write_cbor(&self.profile, out)?;
        head::write_text(out, "authority")?;
        ExecutionCbor::write_cbor(&self.authority, out)?;
        head::write_text(out, "selection")?;
        ExecutionCbor::write_cbor(&self.selection, out)?;
        Ok(())
    }

    /// Check the exact typed-reader depth on borrowed fields without allocation.
    fn check_depth(&self, remaining: usize) -> Result<()> {
        let remaining = descend(remaining)?;
        ExecutionCbor::check_depth(&self.graph, remaining)?;
        ExecutionCbor::check_depth(&self.profile, remaining)?;
        ExecutionCbor::check_depth(&self.authority, remaining)?;
        ExecutionCbor::check_depth(&self.selection, remaining)?;
        Ok(())
    }

    /// Reconstruct the exact stored fields without validation or normalization.
    fn read_cbor(input: &mut Reader<'_>) -> Result<Self> {
        input.map(|input, length| {
            if length != 4 {
                return Err(input.invalid("wrong SelectedLogicalWorld field count"));
            }
            input.field("graph")?;
            let graph = ExecutionCbor::read_cbor(input)?;
            input.field("profile")?;
            let profile = ExecutionCbor::read_cbor(input)?;
            input.field("authority")?;
            let authority = ExecutionCbor::read_cbor(input)?;
            input.field("selection")?;
            let selection = ExecutionCbor::read_cbor(input)?;
            Ok(Self {
                graph,
                profile,
                authority,
                selection,
            })
        })
    }
}

impl ExecutionCbor for SelectedDomains {
    /// Retain the complete SelectedDomains in its original declaration order.
    fn write_cbor<W: crate::execution_cbor::LeafWrite + ?Sized>(
        &self,
        out: &mut W,
    ) -> io::Result<()> {
        head::write_head(out, head::MAP, 1)?;
        head::write_text(out, "worlds")?;
        ExecutionCbor::write_cbor(&self.worlds, out)?;
        Ok(())
    }

    /// Check the exact typed-reader depth on borrowed fields without allocation.
    fn check_depth(&self, remaining: usize) -> Result<()> {
        let remaining = descend(remaining)?;
        ExecutionCbor::check_depth(&self.worlds, remaining)?;
        Ok(())
    }

    /// Reconstruct the exact stored fields without validation or normalization.
    fn read_cbor(input: &mut Reader<'_>) -> Result<Self> {
        input.map(|input, length| {
            if length != 1 {
                return Err(input.invalid("wrong SelectedDomains field count"));
            }
            input.field("worlds")?;
            let worlds = ExecutionCbor::read_cbor(input)?;
            Ok(Self { worlds })
        })
    }
}
