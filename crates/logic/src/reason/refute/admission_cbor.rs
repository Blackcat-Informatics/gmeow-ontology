// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Borrowed CBOR for the selected class-source admission receipt.

use std::io::{self, Write};

use purrdf_lex::cbor::{self, Value, head};

use super::{
    ClassAdmissionObservation, ClassAdmissionSourceWorld, ClassAdmissionWorld, FragmentBoundary,
    RefutationPremise, RefutationSourceIssue,
};

impl ClassAdmissionObservation {
    /// Stream an already validated source observation in its persisted field order.
    /// No source is parsed, admitted or evaluated by this transport.
    ///
    /// # Errors
    /// Returns sink failures and refuses non-source boundaries outside this profile.
    pub fn write_cbor<W: Write + ?Sized>(&self, out: &mut W) -> io::Result<()> {
        self.write_fields(
            &mut crate::execution_cbor::InlineWriter(out),
            |world, out| world.write_fields(out, FragmentBoundary::write_admission_cbor),
        )
    }

    /// One field owner serves the raw source profile and complete terminal profile.
    pub(super) fn write_fields<W: crate::execution_cbor::LeafWrite + ?Sized>(
        &self,
        out: &mut W,
        write_world: impl Fn(&ClassAdmissionWorld, &mut W) -> io::Result<()>,
    ) -> io::Result<()> {
        head::write_head(out, head::MAP, 3)?;
        head::write_text(out, "contract")?;
        out.string(&self.contract)?;
        head::write_text(out, "source_worlds")?;
        head::write_head(out, head::MAP, self.source_worlds.len() as u64)?;
        for (world, source) in &self.source_worlds {
            out.string(world)?;
            source.write_fields(out)?;
        }
        head::write_text(out, "selected_worlds")?;
        head::write_head(out, head::MAP, self.selected_worlds.len() as u64)?;
        for (world, selected) in &self.selected_worlds {
            out.string(world)?;
            write_world(selected, out)?;
        }
        Ok(())
    }
}

impl ClassAdmissionSourceWorld {
    /// Retain nullable graph identity and the complete unsigned assertion census.
    fn write_fields<W: crate::execution_cbor::LeafWrite + ?Sized>(
        &self,
        out: &mut W,
    ) -> io::Result<()> {
        head::write_head(out, head::MAP, 2)?;
        head::write_text(out, "graph")?;
        ExecutionCbor::write_cbor(&self.graph, out)?;
        head::write_text(out, "assertions")?;
        head::write_head(out, head::UNSIGNED, self.assertions)
    }
}

impl ClassAdmissionWorld {
    /// Select the narrower source boundary or full terminal variants explicitly.
    pub(super) fn write_fields<W: crate::execution_cbor::LeafWrite + ?Sized>(
        &self,
        out: &mut W,
        write_boundary: impl Fn(&FragmentBoundary, &mut W) -> io::Result<()>,
    ) -> io::Result<()> {
        head::write_head(out, head::MAP, 3)?;
        head::write_text(out, "graph")?;
        ExecutionCbor::write_cbor(&self.graph, out)?;
        head::write_text(out, "definitions")?;
        head::write_head(out, head::ARRAY, self.definitions.len() as u64)?;
        for definition in &self.definitions {
            definition.write_fields(out)?;
        }
        head::write_text(out, "refusal")?;
        match &self.refusal {
            Some(boundary) => write_boundary(boundary, out),
            None => cbor::write(&Value::Null, out),
        }
    }
}

impl RefutationPremise {
    /// The row field owner retains original graph and complete term identity.
    fn write_fields<W: crate::execution_cbor::LeafWrite + ?Sized>(
        &self,
        out: &mut W,
    ) -> io::Result<()> {
        head::write_head(out, head::MAP, 4)?;
        head::write_text(out, "subject")?;
        out.term(&self.subject)?;
        head::write_text(out, "predicate")?;
        out.string(&self.predicate)?;
        head::write_text(out, "object")?;
        out.term(&self.object)?;
        head::write_text(out, "graph")?;
        ExecutionCbor::write_cbor(&self.graph, out)
    }
}

impl FragmentBoundary {
    /// Stream source-supported boundary variants in their retained set order.
    fn write_admission_cbor<W: crate::execution_cbor::LeafWrite + ?Sized>(
        &self,
        out: &mut W,
    ) -> io::Result<()> {
        let mut pending = vec![self];
        while let Some(boundary) = pending.pop() {
            match boundary {
                Self::SourceAdmission {
                    world,
                    premises,
                    issue,
                } => {
                    head::write_head(out, head::MAP, 1)?;
                    head::write_text(out, "SourceAdmission")?;
                    head::write_head(out, head::MAP, 3)?;
                    head::write_text(out, "world")?;
                    out.string(world)?;
                    head::write_text(out, "premises")?;
                    head::write_head(out, head::ARRAY, premises.len() as u64)?;
                    for premise in premises {
                        premise.write_fields(out)?;
                    }
                    head::write_text(out, "issue")?;
                    issue.write_admission_cbor(out)?;
                }
                Self::Combined(boundaries) => {
                    head::write_head(out, head::MAP, 1)?;
                    head::write_text(out, "Combined")?;
                    head::write_head(out, head::ARRAY, boundaries.len() as u64)?;
                    // The retained set's own order defines the persisted sequence.
                    pending.extend(boundaries.iter().rev());
                }
                Self::Uncertified { .. } => {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidInput,
                        "source-admission CBOR refuses a non-source fragment boundary",
                    ));
                }
            }
        }
        Ok(())
    }
}

impl RefutationSourceIssue {
    /// Preserve every selected source-issue tag and its original payload fields.
    fn write_admission_cbor<W: crate::execution_cbor::LeafWrite + ?Sized>(
        &self,
        out: &mut W,
    ) -> io::Result<()> {
        match self {
            Self::MalformedNil => head::write_text(out, "MalformedNil"),
            Self::ConflictingListField { subject, predicate } => {
                head::write_head(out, head::MAP, 1)?;
                head::write_text(out, "ConflictingListField")?;
                head::write_head(out, head::MAP, 2)?;
                head::write_text(out, "subject")?;
                out.string(subject)?;
                head::write_text(out, "predicate")?;
                out.string(predicate)
            }
            Self::ExpressionMultiplicity { subject, predicate } => {
                head::write_head(out, head::MAP, 1)?;
                head::write_text(out, "ExpressionMultiplicity")?;
                head::write_head(out, head::MAP, 2)?;
                head::write_text(out, "subject")?;
                out.string(subject)?;
                head::write_text(out, "predicate")?;
                out.string(predicate)
            }
            Self::UnsupportedExpressionOwner { owner, predicate } => {
                head::write_head(out, head::MAP, 1)?;
                head::write_text(out, "UnsupportedExpressionOwner")?;
                head::write_head(out, head::MAP, 2)?;
                head::write_text(out, "owner")?;
                out.term(owner)?;
                head::write_text(out, "predicate")?;
                out.string(predicate)
            }
            Self::UnsupportedExpressionOperand {
                owner,
                predicate,
                operand,
            } => {
                head::write_head(out, head::MAP, 1)?;
                head::write_text(out, "UnsupportedExpressionOperand")?;
                head::write_head(out, head::MAP, 3)?;
                head::write_text(out, "owner")?;
                out.string(owner)?;
                head::write_text(out, "predicate")?;
                out.string(predicate)?;
                head::write_text(out, "operand")?;
                out.term(operand)
            }
            Self::UnsupportedListMember {
                owner,
                head: list_head,
            } => {
                head::write_head(out, head::MAP, 1)?;
                head::write_text(out, "UnsupportedListMember")?;
                head::write_head(out, head::MAP, 2)?;
                head::write_text(out, "owner")?;
                out.string(owner)?;
                head::write_text(out, "head")?;
                out.string(list_head)
            }
            Self::CyclicList {
                owner,
                head: list_head,
            } => {
                head::write_head(out, head::MAP, 1)?;
                head::write_text(out, "CyclicList")?;
                head::write_head(out, head::MAP, 2)?;
                head::write_text(out, "owner")?;
                out.string(owner)?;
                head::write_text(out, "head")?;
                out.string(list_head)
            }
            Self::IncompleteList {
                owner,
                head: list_head,
            } => {
                head::write_head(out, head::MAP, 1)?;
                head::write_text(out, "IncompleteList")?;
                head::write_head(out, head::MAP, 2)?;
                head::write_text(out, "owner")?;
                out.string(owner)?;
                head::write_text(out, "head")?;
                out.string(list_head)
            }
        }
    }
}

// Complete receipt hydration shares only the genuinely identical source-row
// and source-world writers above. No semantic constructor runs at this boundary.
use crate::execution_cbor::{ExecutionCbor, Reader, Result as WireResult, descend};

impl ExecutionCbor for RefutationPremise {
    fn write_cbor<W: crate::execution_cbor::LeafWrite + ?Sized>(
        &self,
        out: &mut W,
    ) -> io::Result<()> {
        RefutationPremise::write_fields(self, out)
    }

    fn check_depth(&self, remaining: usize) -> WireResult<()> {
        let remaining = descend(remaining)?;
        ExecutionCbor::check_depth(&self.subject, remaining)?;
        ExecutionCbor::check_depth(&self.predicate, remaining)?;
        ExecutionCbor::check_depth(&self.object, remaining)?;
        ExecutionCbor::check_depth(&self.graph, remaining)?;
        Ok(())
    }

    fn read_cbor(input: &mut Reader<'_>) -> WireResult<Self> {
        input.map(|input, length| {
            if length != 4 {
                return Err(input.invalid("expected complete RefutationPremise record"));
            }
            input.field("subject")?;
            let subject = ExecutionCbor::read_cbor(input)?;
            input.field("predicate")?;
            let predicate = ExecutionCbor::read_cbor(input)?;
            input.field("object")?;
            let object = ExecutionCbor::read_cbor(input)?;
            input.field("graph")?;
            let graph = ExecutionCbor::read_cbor(input)?;
            Ok(Self {
                subject,
                predicate,
                object,
                graph,
            })
        })
    }
}

impl ExecutionCbor for ClassAdmissionSourceWorld {
    fn write_cbor<W: crate::execution_cbor::LeafWrite + ?Sized>(
        &self,
        out: &mut W,
    ) -> io::Result<()> {
        ClassAdmissionSourceWorld::write_fields(self, out)
    }

    fn check_depth(&self, remaining: usize) -> WireResult<()> {
        let remaining = descend(remaining)?;
        ExecutionCbor::check_depth(&self.graph, remaining)?;
        ExecutionCbor::check_depth(&self.assertions, remaining)?;
        Ok(())
    }

    fn read_cbor(input: &mut Reader<'_>) -> WireResult<Self> {
        input.map(|input, length| {
            if length != 2 {
                return Err(input.invalid("expected complete ClassAdmissionSourceWorld record"));
            }
            input.field("graph")?;
            let graph = ExecutionCbor::read_cbor(input)?;
            input.field("assertions")?;
            let assertions = ExecutionCbor::read_cbor(input)?;
            Ok(Self { graph, assertions })
        })
    }
}
