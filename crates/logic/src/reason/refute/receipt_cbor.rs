// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Complete owner-local native execution receipt records, without semantic reconstruction.

use std::io;

use purrdf_lex::cbor::head;

use crate::execution_cbor::{ExecutionCbor, Reader, Result as WireResult, descend};

use super::{BoundKind, FragmentBoundary, FragmentFamily};

impl ExecutionCbor for FragmentFamily {
    fn write_cbor<W: crate::execution_cbor::LeafWrite + ?Sized>(
        &self,
        out: &mut W,
    ) -> io::Result<()> {
        match self {
            Self::DatatypeValueSpace => head::write_text(out, "DatatypeValueSpace"),
            Self::Counting => head::write_text(out, "Counting"),
            Self::CaseSplit => head::write_text(out, "CaseSplit"),
        }
    }

    fn check_depth(&self, remaining: usize) -> WireResult<()> {
        descend(remaining)?;
        match self {
            Self::DatatypeValueSpace | Self::Counting | Self::CaseSplit => Ok(()),
        }
    }

    fn read_cbor(input: &mut Reader<'_>) -> WireResult<Self> {
        input.variant(|input, name, tagged| match (name, tagged) {
            ("DatatypeValueSpace", false) => Ok(Self::DatatypeValueSpace),
            ("Counting", false) => Ok(Self::Counting),
            ("CaseSplit", false) => Ok(Self::CaseSplit),
            _ => Err(input.invalid("invalid FragmentFamily variant or payload form")),
        })
    }
}

impl ExecutionCbor for BoundKind {
    fn write_cbor<W: crate::execution_cbor::LeafWrite + ?Sized>(
        &self,
        out: &mut W,
    ) -> io::Result<()> {
        match self {
            Self::Min => head::write_text(out, "Min"),
            Self::Max => head::write_text(out, "Max"),
            Self::Exact => head::write_text(out, "Exact"),
        }
    }

    fn check_depth(&self, remaining: usize) -> WireResult<()> {
        descend(remaining)?;
        match self {
            Self::Min | Self::Max | Self::Exact => Ok(()),
        }
    }

    fn read_cbor(input: &mut Reader<'_>) -> WireResult<Self> {
        input.variant(|input, name, tagged| match (name, tagged) {
            ("Min", false) => Ok(Self::Min),
            ("Max", false) => Ok(Self::Max),
            ("Exact", false) => Ok(Self::Exact),
            _ => Err(input.invalid("invalid BoundKind variant or payload form")),
        })
    }
}

impl ExecutionCbor for FragmentBoundary {
    fn write_cbor<W: crate::execution_cbor::LeafWrite + ?Sized>(
        &self,
        out: &mut W,
    ) -> io::Result<()> {
        match self {
            Self::SourceAdmission {
                world,
                premises,
                issue,
            } => {
                head::write_head(out, head::MAP, 1)?;
                head::write_text(out, "SourceAdmission")?;
                head::write_head(out, head::MAP, 3)?;
                head::write_text(out, "world")?;
                ExecutionCbor::write_cbor(world, out)?;
                head::write_text(out, "premises")?;
                ExecutionCbor::write_cbor(premises, out)?;
                head::write_text(out, "issue")?;
                ExecutionCbor::write_cbor(issue, out)?;
                Ok(())
            }
            Self::Uncertified {
                family,
                obstructions,
            } => {
                head::write_head(out, head::MAP, 1)?;
                head::write_text(out, "Uncertified")?;
                head::write_head(out, head::MAP, 2)?;
                head::write_text(out, "family")?;
                ExecutionCbor::write_cbor(family, out)?;
                head::write_text(out, "obstructions")?;
                ExecutionCbor::write_cbor(obstructions, out)?;
                Ok(())
            }
            Self::Combined(value) => {
                head::write_head(out, head::MAP, 1)?;
                head::write_text(out, "Combined")?;
                ExecutionCbor::write_cbor(value, out)
            }
        }
    }

    fn check_depth(&self, remaining: usize) -> WireResult<()> {
        let remaining = descend(remaining)?;
        match self {
            Self::SourceAdmission {
                world,
                premises,
                issue,
            } => {
                let remaining = descend(remaining)?;
                ExecutionCbor::check_depth(world, remaining)?;
                ExecutionCbor::check_depth(premises, remaining)?;
                ExecutionCbor::check_depth(issue, remaining)?;
                Ok(())
            }
            Self::Uncertified {
                family,
                obstructions,
            } => {
                let remaining = descend(remaining)?;
                ExecutionCbor::check_depth(family, remaining)?;
                ExecutionCbor::check_depth(obstructions, remaining)?;
                Ok(())
            }
            Self::Combined(value) => ExecutionCbor::check_depth(value, remaining),
        }
    }

    fn read_cbor(input: &mut Reader<'_>) -> WireResult<Self> {
        input.variant(|input, name, tagged| match (name, tagged) {
            ("SourceAdmission", true) => input.map(|input, length| {
                if length != 3 {
                    return Err(
                        input.invalid("expected complete FragmentBoundary::SourceAdmission record")
                    );
                }
                input.field("world")?;
                let world = ExecutionCbor::read_cbor(input)?;
                input.field("premises")?;
                let premises = ExecutionCbor::read_cbor(input)?;
                input.field("issue")?;
                let issue = ExecutionCbor::read_cbor(input)?;
                Ok(Self::SourceAdmission {
                    world,
                    premises,
                    issue,
                })
            }),
            ("Uncertified", true) => input.map(|input, length| {
                if length != 2 {
                    return Err(
                        input.invalid("expected complete FragmentBoundary::Uncertified record")
                    );
                }
                input.field("family")?;
                let family = ExecutionCbor::read_cbor(input)?;
                input.field("obstructions")?;
                let obstructions = ExecutionCbor::read_cbor(input)?;
                Ok(Self::Uncertified {
                    family,
                    obstructions,
                })
            }),
            ("Combined", true) => Ok(Self::Combined(ExecutionCbor::read_cbor(input)?)),
            _ => Err(input.invalid("invalid FragmentBoundary variant or payload form")),
        })
    }
}
