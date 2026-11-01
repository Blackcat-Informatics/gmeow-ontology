// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Complete selected chase termination evidence, including uncertified states.

use std::io::{self, Write};

use purrdf_lex::cbor::head;

use crate::execution_cbor::{ExecutionCbor, Reader, Result, descend};

use super::ChaseAdmission;

impl ExecutionCbor for ChaseAdmission {
    /// Retain the exact certificate variant and its original evidence payload.
    fn write_cbor<W: Write + ?Sized>(&self, out: &mut W) -> io::Result<()> {
        head::write_head(out, head::MAP, 1)?;
        match self {
            Self::WeaklyAcyclic { evidence } => {
                head::write_text(out, "WeaklyAcyclic")?;
                head::write_head(out, head::MAP, 1)?;
                head::write_text(out, "evidence")?;
                ExecutionCbor::write_cbor(evidence, out)
            }
            Self::JointlyAcyclic { evidence } => {
                head::write_text(out, "JointlyAcyclic")?;
                head::write_head(out, head::MAP, 1)?;
                head::write_text(out, "evidence")?;
                ExecutionCbor::write_cbor(evidence, out)
            }
            Self::SuperWeaklyAcyclic { evidence } => {
                head::write_text(out, "SuperWeaklyAcyclic")?;
                head::write_head(out, head::MAP, 1)?;
                head::write_text(out, "evidence")?;
                ExecutionCbor::write_cbor(evidence, out)
            }
            Self::RestrictedJointlyAcyclic { evidence } => {
                head::write_text(out, "RestrictedJointlyAcyclic")?;
                head::write_head(out, head::MAP, 1)?;
                head::write_text(out, "evidence")?;
                ExecutionCbor::write_cbor(evidence, out)
            }
            Self::ModelSummarizingAcyclic { evidence } => {
                head::write_text(out, "ModelSummarizingAcyclic")?;
                head::write_head(out, head::MAP, 1)?;
                head::write_text(out, "evidence")?;
                ExecutionCbor::write_cbor(evidence, out)
            }
            Self::Uncertified { violations } => {
                head::write_text(out, "Uncertified")?;
                head::write_head(out, head::MAP, 1)?;
                head::write_text(out, "violations")?;
                ExecutionCbor::write_cbor(violations, out)
            }
        }
    }

    /// Check the exact typed-reader depth on borrowed fields without allocation.
    fn check_depth(&self, remaining: usize) -> Result<()> {
        // Every variant consumes an enum visit and its one-field payload map.
        let remaining = descend(descend(remaining)?)?;
        match self {
            Self::WeaklyAcyclic { evidence }
            | Self::JointlyAcyclic { evidence }
            | Self::SuperWeaklyAcyclic { evidence }
            | Self::RestrictedJointlyAcyclic { evidence }
            | Self::ModelSummarizingAcyclic { evidence } => {
                ExecutionCbor::check_depth(evidence, remaining)
            }
            Self::Uncertified { violations } => ExecutionCbor::check_depth(violations, remaining),
        }
    }

    /// Preserve separate enum and struct-payload visitor charges.
    fn read_cbor(input: &mut Reader<'_>) -> Result<Self> {
        input.variant(|input, name, tagged| {
            if !tagged {
                return Err(input.invalid("ChaseAdmission requires a tagged payload"));
            }
            match name {
                "WeaklyAcyclic" => input.map(|input, length| {
                    if length != 1 {
                        return Err(input.invalid("wrong ChaseAdmission payload field count"));
                    }
                    input.field("evidence")?;
                    Ok(Self::WeaklyAcyclic {
                        evidence: ExecutionCbor::read_cbor(input)?,
                    })
                }),
                "JointlyAcyclic" => input.map(|input, length| {
                    if length != 1 {
                        return Err(input.invalid("wrong ChaseAdmission payload field count"));
                    }
                    input.field("evidence")?;
                    Ok(Self::JointlyAcyclic {
                        evidence: ExecutionCbor::read_cbor(input)?,
                    })
                }),
                "SuperWeaklyAcyclic" => input.map(|input, length| {
                    if length != 1 {
                        return Err(input.invalid("wrong ChaseAdmission payload field count"));
                    }
                    input.field("evidence")?;
                    Ok(Self::SuperWeaklyAcyclic {
                        evidence: ExecutionCbor::read_cbor(input)?,
                    })
                }),
                "RestrictedJointlyAcyclic" => input.map(|input, length| {
                    if length != 1 {
                        return Err(input.invalid("wrong ChaseAdmission payload field count"));
                    }
                    input.field("evidence")?;
                    Ok(Self::RestrictedJointlyAcyclic {
                        evidence: ExecutionCbor::read_cbor(input)?,
                    })
                }),
                "ModelSummarizingAcyclic" => input.map(|input, length| {
                    if length != 1 {
                        return Err(input.invalid("wrong ChaseAdmission payload field count"));
                    }
                    input.field("evidence")?;
                    Ok(Self::ModelSummarizingAcyclic {
                        evidence: ExecutionCbor::read_cbor(input)?,
                    })
                }),
                "Uncertified" => input.map(|input, length| {
                    if length != 1 {
                        return Err(input.invalid("wrong ChaseAdmission payload field count"));
                    }
                    input.field("violations")?;
                    Ok(Self::Uncertified {
                        violations: ExecutionCbor::read_cbor(input)?,
                    })
                }),
                _ => Err(input.invalid("unknown ChaseAdmission variant")),
            }
        })
    }
}
