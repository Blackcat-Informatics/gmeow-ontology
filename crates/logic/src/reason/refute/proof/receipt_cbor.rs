// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Complete owner-local native execution receipt records, without semantic reconstruction.

use std::io::{self, Write};

use purrdf_lex::cbor::head;

use crate::execution_cbor::{ExecutionCbor, Reader, Result as WireResult, descend};

use super::{
    ContextualConflict, RefutationAssumption, RefutationBranch, RefutationClash,
    RefutationExpression, RefutationProof, RefutationSourceIssue,
};

impl ExecutionCbor for RefutationClash {
    fn write_cbor<W: Write + ?Sized>(&self, out: &mut W) -> io::Result<()> {
        match self {
            Self::Bottom => head::write_text(out, "Bottom"),
            Self::OpposedClass(value) => {
                head::write_head(out, head::MAP, 1)?;
                head::write_text(out, "OpposedClass")?;
                ExecutionCbor::write_cbor(value, out)
            }
            Self::EqualityDistinctness => head::write_text(out, "EqualityDistinctness"),
            Self::EmptyEnumeration => head::write_text(out, "EmptyEnumeration"),
            Self::ExhaustedDisjunction => head::write_text(out, "ExhaustedDisjunction"),
        }
    }

    fn check_depth(&self, remaining: usize) -> WireResult<()> {
        let remaining = descend(remaining)?;
        match self {
            Self::Bottom
            | Self::EqualityDistinctness
            | Self::EmptyEnumeration
            | Self::ExhaustedDisjunction => Ok(()),
            Self::OpposedClass(value) => ExecutionCbor::check_depth(value, remaining),
        }
    }

    fn read_cbor(input: &mut Reader<'_>) -> WireResult<Self> {
        input.variant(|input, name, tagged| match (name, tagged) {
            ("Bottom", false) => Ok(Self::Bottom),
            ("OpposedClass", true) => Ok(Self::OpposedClass(ExecutionCbor::read_cbor(input)?)),
            ("EqualityDistinctness", false) => Ok(Self::EqualityDistinctness),
            ("EmptyEnumeration", false) => Ok(Self::EmptyEnumeration),
            ("ExhaustedDisjunction", false) => Ok(Self::ExhaustedDisjunction),
            _ => Err(input.invalid("invalid RefutationClash variant or payload form")),
        })
    }
}

impl ExecutionCbor for RefutationExpression {
    fn write_cbor<W: Write + ?Sized>(&self, out: &mut W) -> io::Result<()> {
        match self {
            Self::Top => head::write_text(out, "Top"),
            Self::Bottom => head::write_text(out, "Bottom"),
            Self::Pos(value) => {
                head::write_head(out, head::MAP, 1)?;
                head::write_text(out, "Pos")?;
                ExecutionCbor::write_cbor(value, out)
            }
            Self::Neg(value) => {
                head::write_head(out, head::MAP, 1)?;
                head::write_text(out, "Neg")?;
                ExecutionCbor::write_cbor(value, out)
            }
            Self::And(value) => {
                head::write_head(out, head::MAP, 1)?;
                head::write_text(out, "And")?;
                ExecutionCbor::write_cbor(value, out)
            }
            Self::Or(value) => {
                head::write_head(out, head::MAP, 1)?;
                head::write_text(out, "Or")?;
                ExecutionCbor::write_cbor(value, out)
            }
            Self::Nominals(value) => {
                head::write_head(out, head::MAP, 1)?;
                head::write_text(out, "Nominals")?;
                ExecutionCbor::write_cbor(value, out)
            }
            Self::Blocked => head::write_text(out, "Blocked"),
        }
    }

    fn check_depth(&self, remaining: usize) -> WireResult<()> {
        let remaining = descend(remaining)?;
        match self {
            Self::Top | Self::Bottom | Self::Blocked => Ok(()),
            Self::Pos(value) | Self::Neg(value) => ExecutionCbor::check_depth(value, remaining),
            Self::And(value) | Self::Or(value) => ExecutionCbor::check_depth(value, remaining),
            Self::Nominals(value) => ExecutionCbor::check_depth(value, remaining),
        }
    }

    fn read_cbor(input: &mut Reader<'_>) -> WireResult<Self> {
        input.variant(|input, name, tagged| match (name, tagged) {
            ("Top", false) => Ok(Self::Top),
            ("Bottom", false) => Ok(Self::Bottom),
            ("Pos", true) => Ok(Self::Pos(ExecutionCbor::read_cbor(input)?)),
            ("Neg", true) => Ok(Self::Neg(ExecutionCbor::read_cbor(input)?)),
            ("And", true) => Ok(Self::And(ExecutionCbor::read_cbor(input)?)),
            ("Or", true) => Ok(Self::Or(ExecutionCbor::read_cbor(input)?)),
            ("Nominals", true) => Ok(Self::Nominals(ExecutionCbor::read_cbor(input)?)),
            ("Blocked", false) => Ok(Self::Blocked),
            _ => Err(input.invalid("invalid RefutationExpression variant or payload form")),
        })
    }
}

impl ExecutionCbor for RefutationAssumption {
    fn write_cbor<W: Write + ?Sized>(&self, out: &mut W) -> io::Result<()> {
        match self {
            Self::Membership {
                subject,
                expression,
            } => {
                head::write_head(out, head::MAP, 1)?;
                head::write_text(out, "Membership")?;
                head::write_head(out, head::MAP, 2)?;
                head::write_text(out, "subject")?;
                ExecutionCbor::write_cbor(subject, out)?;
                head::write_text(out, "expression")?;
                ExecutionCbor::write_cbor(expression, out)?;
                Ok(())
            }
            Self::Equality { subject, member } => {
                head::write_head(out, head::MAP, 1)?;
                head::write_text(out, "Equality")?;
                head::write_head(out, head::MAP, 2)?;
                head::write_text(out, "subject")?;
                ExecutionCbor::write_cbor(subject, out)?;
                head::write_text(out, "member")?;
                ExecutionCbor::write_cbor(member, out)?;
                Ok(())
            }
        }
    }

    fn check_depth(&self, remaining: usize) -> WireResult<()> {
        let remaining = descend(remaining)?;
        match self {
            Self::Membership {
                subject,
                expression,
            } => {
                let remaining = descend(remaining)?;
                ExecutionCbor::check_depth(subject, remaining)?;
                ExecutionCbor::check_depth(expression, remaining)?;
                Ok(())
            }
            Self::Equality { subject, member } => {
                let remaining = descend(remaining)?;
                ExecutionCbor::check_depth(subject, remaining)?;
                ExecutionCbor::check_depth(member, remaining)?;
                Ok(())
            }
        }
    }

    fn read_cbor(input: &mut Reader<'_>) -> WireResult<Self> {
        input.variant(|input, name, tagged| match (name, tagged) {
            ("Membership", true) => input.map(|input, length| {
                if length != 2 {
                    return Err(
                        input.invalid("expected complete RefutationAssumption::Membership record")
                    );
                }
                input.field("subject")?;
                let subject = ExecutionCbor::read_cbor(input)?;
                input.field("expression")?;
                let expression = ExecutionCbor::read_cbor(input)?;
                Ok(Self::Membership {
                    subject,
                    expression,
                })
            }),
            ("Equality", true) => input.map(|input, length| {
                if length != 2 {
                    return Err(
                        input.invalid("expected complete RefutationAssumption::Equality record")
                    );
                }
                input.field("subject")?;
                let subject = ExecutionCbor::read_cbor(input)?;
                input.field("member")?;
                let member = ExecutionCbor::read_cbor(input)?;
                Ok(Self::Equality { subject, member })
            }),
            _ => Err(input.invalid("invalid RefutationAssumption variant or payload form")),
        })
    }
}

impl ExecutionCbor for RefutationBranch {
    fn write_cbor<W: Write + ?Sized>(&self, out: &mut W) -> io::Result<()> {
        head::write_head(out, head::MAP, 2)?;
        head::write_text(out, "assumption")?;
        ExecutionCbor::write_cbor(&self.assumption, out)?;
        head::write_text(out, "proof")?;
        ExecutionCbor::write_cbor(&self.proof, out)?;
        Ok(())
    }

    fn check_depth(&self, remaining: usize) -> WireResult<()> {
        let remaining = descend(remaining)?;
        ExecutionCbor::check_depth(&self.assumption, remaining)?;
        ExecutionCbor::check_depth(&self.proof, remaining)?;
        Ok(())
    }

    fn read_cbor(input: &mut Reader<'_>) -> WireResult<Self> {
        input.map(|input, length| {
            if length != 2 {
                return Err(input.invalid("expected complete RefutationBranch record"));
            }
            input.field("assumption")?;
            let assumption = ExecutionCbor::read_cbor(input)?;
            input.field("proof")?;
            let proof = ExecutionCbor::read_cbor(input)?;
            Ok(Self { assumption, proof })
        })
    }
}

impl ExecutionCbor for RefutationSourceIssue {
    fn write_cbor<W: Write + ?Sized>(&self, out: &mut W) -> io::Result<()> {
        match self {
            Self::MalformedNil => head::write_text(out, "MalformedNil"),
            Self::ConflictingListField { subject, predicate } => {
                head::write_head(out, head::MAP, 1)?;
                head::write_text(out, "ConflictingListField")?;
                head::write_head(out, head::MAP, 2)?;
                head::write_text(out, "subject")?;
                ExecutionCbor::write_cbor(subject, out)?;
                head::write_text(out, "predicate")?;
                ExecutionCbor::write_cbor(predicate, out)?;
                Ok(())
            }
            Self::ExpressionMultiplicity { subject, predicate } => {
                head::write_head(out, head::MAP, 1)?;
                head::write_text(out, "ExpressionMultiplicity")?;
                head::write_head(out, head::MAP, 2)?;
                head::write_text(out, "subject")?;
                ExecutionCbor::write_cbor(subject, out)?;
                head::write_text(out, "predicate")?;
                ExecutionCbor::write_cbor(predicate, out)?;
                Ok(())
            }
            Self::UnsupportedExpressionOwner { owner, predicate } => {
                head::write_head(out, head::MAP, 1)?;
                head::write_text(out, "UnsupportedExpressionOwner")?;
                head::write_head(out, head::MAP, 2)?;
                head::write_text(out, "owner")?;
                ExecutionCbor::write_cbor(owner, out)?;
                head::write_text(out, "predicate")?;
                ExecutionCbor::write_cbor(predicate, out)?;
                Ok(())
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
                ExecutionCbor::write_cbor(owner, out)?;
                head::write_text(out, "predicate")?;
                ExecutionCbor::write_cbor(predicate, out)?;
                head::write_text(out, "operand")?;
                ExecutionCbor::write_cbor(operand, out)?;
                Ok(())
            }
            Self::UnsupportedListMember { owner, head } => {
                head::write_head(out, head::MAP, 1)?;
                head::write_text(out, "UnsupportedListMember")?;
                head::write_head(out, head::MAP, 2)?;
                head::write_text(out, "owner")?;
                ExecutionCbor::write_cbor(owner, out)?;
                head::write_text(out, "head")?;
                ExecutionCbor::write_cbor(head, out)?;
                Ok(())
            }
            Self::CyclicList { owner, head } => {
                head::write_head(out, head::MAP, 1)?;
                head::write_text(out, "CyclicList")?;
                head::write_head(out, head::MAP, 2)?;
                head::write_text(out, "owner")?;
                ExecutionCbor::write_cbor(owner, out)?;
                head::write_text(out, "head")?;
                ExecutionCbor::write_cbor(head, out)?;
                Ok(())
            }
            Self::IncompleteList { owner, head } => {
                head::write_head(out, head::MAP, 1)?;
                head::write_text(out, "IncompleteList")?;
                head::write_head(out, head::MAP, 2)?;
                head::write_text(out, "owner")?;
                ExecutionCbor::write_cbor(owner, out)?;
                head::write_text(out, "head")?;
                ExecutionCbor::write_cbor(head, out)?;
                Ok(())
            }
        }
    }

    fn check_depth(&self, remaining: usize) -> WireResult<()> {
        let remaining = descend(remaining)?;
        match self {
            Self::MalformedNil => Ok(()),
            Self::ConflictingListField { subject, predicate }
            | Self::ExpressionMultiplicity { subject, predicate } => {
                let remaining = descend(remaining)?;
                ExecutionCbor::check_depth(subject, remaining)?;
                ExecutionCbor::check_depth(predicate, remaining)?;
                Ok(())
            }
            Self::UnsupportedExpressionOwner { owner, predicate } => {
                let remaining = descend(remaining)?;
                ExecutionCbor::check_depth(owner, remaining)?;
                ExecutionCbor::check_depth(predicate, remaining)?;
                Ok(())
            }
            Self::UnsupportedExpressionOperand {
                owner,
                predicate,
                operand,
            } => {
                let remaining = descend(remaining)?;
                ExecutionCbor::check_depth(owner, remaining)?;
                ExecutionCbor::check_depth(predicate, remaining)?;
                ExecutionCbor::check_depth(operand, remaining)?;
                Ok(())
            }
            Self::UnsupportedListMember { owner, head }
            | Self::CyclicList { owner, head }
            | Self::IncompleteList { owner, head } => {
                let remaining = descend(remaining)?;
                ExecutionCbor::check_depth(owner, remaining)?;
                ExecutionCbor::check_depth(head, remaining)?;
                Ok(())
            }
        }
    }

    fn read_cbor(input: &mut Reader<'_>) -> WireResult<Self> {
        input.variant(|input, name, tagged| match (name, tagged) {
            ("MalformedNil", false) => Ok(Self::MalformedNil),
            ("ConflictingListField", true) => input.map(|input, length| {
                if length != 2 { return Err(input.invalid("expected complete RefutationSourceIssue::ConflictingListField record")); }
                input.field("subject")?;
                let subject = ExecutionCbor::read_cbor(input)?;
                input.field("predicate")?;
                let predicate = ExecutionCbor::read_cbor(input)?;
                Ok(Self::ConflictingListField { subject, predicate })
            }),
            ("ExpressionMultiplicity", true) => input.map(|input, length| {
                if length != 2 { return Err(input.invalid("expected complete RefutationSourceIssue::ExpressionMultiplicity record")); }
                input.field("subject")?;
                let subject = ExecutionCbor::read_cbor(input)?;
                input.field("predicate")?;
                let predicate = ExecutionCbor::read_cbor(input)?;
                Ok(Self::ExpressionMultiplicity { subject, predicate })
            }),
            ("UnsupportedExpressionOwner", true) => input.map(|input, length| {
                if length != 2 { return Err(input.invalid("expected complete RefutationSourceIssue::UnsupportedExpressionOwner record")); }
                input.field("owner")?;
                let owner = ExecutionCbor::read_cbor(input)?;
                input.field("predicate")?;
                let predicate = ExecutionCbor::read_cbor(input)?;
                Ok(Self::UnsupportedExpressionOwner { owner, predicate })
            }),
            ("UnsupportedExpressionOperand", true) => input.map(|input, length| {
                if length != 3 { return Err(input.invalid("expected complete RefutationSourceIssue::UnsupportedExpressionOperand record")); }
                input.field("owner")?;
                let owner = ExecutionCbor::read_cbor(input)?;
                input.field("predicate")?;
                let predicate = ExecutionCbor::read_cbor(input)?;
                input.field("operand")?;
                let operand = ExecutionCbor::read_cbor(input)?;
                Ok(Self::UnsupportedExpressionOperand { owner, predicate, operand })
            }),
            ("UnsupportedListMember", true) => input.map(|input, length| {
                if length != 2 { return Err(input.invalid("expected complete RefutationSourceIssue::UnsupportedListMember record")); }
                input.field("owner")?;
                let owner = ExecutionCbor::read_cbor(input)?;
                input.field("head")?;
                let head = ExecutionCbor::read_cbor(input)?;
                Ok(Self::UnsupportedListMember { owner, head })
            }),
            ("CyclicList", true) => input.map(|input, length| {
                if length != 2 { return Err(input.invalid("expected complete RefutationSourceIssue::CyclicList record")); }
                input.field("owner")?;
                let owner = ExecutionCbor::read_cbor(input)?;
                input.field("head")?;
                let head = ExecutionCbor::read_cbor(input)?;
                Ok(Self::CyclicList { owner, head })
            }),
            ("IncompleteList", true) => input.map(|input, length| {
                if length != 2 { return Err(input.invalid("expected complete RefutationSourceIssue::IncompleteList record")); }
                input.field("owner")?;
                let owner = ExecutionCbor::read_cbor(input)?;
                input.field("head")?;
                let head = ExecutionCbor::read_cbor(input)?;
                Ok(Self::IncompleteList { owner, head })
            }),
            _ => Err(input.invalid("invalid RefutationSourceIssue variant or payload form")),
        })
    }
}

impl ExecutionCbor for RefutationProof {
    fn write_cbor<W: Write + ?Sized>(&self, out: &mut W) -> io::Result<()> {
        match self {
            Self::Conflict {
                kind,
                subjects,
                support,
                premises,
            } => {
                head::write_head(out, head::MAP, 1)?;
                head::write_text(out, "Conflict")?;
                head::write_head(out, head::MAP, 4)?;
                head::write_text(out, "kind")?;
                ExecutionCbor::write_cbor(kind, out)?;
                head::write_text(out, "subjects")?;
                ExecutionCbor::write_cbor(subjects, out)?;
                head::write_text(out, "support")?;
                ExecutionCbor::write_cbor(support, out)?;
                head::write_text(out, "premises")?;
                ExecutionCbor::write_cbor(premises, out)?;
                Ok(())
            }
            Self::Cases {
                support,
                alternatives,
                choice,
                branches,
            } => {
                head::write_head(out, head::MAP, 1)?;
                head::write_text(out, "Cases")?;
                head::write_head(out, head::MAP, 4)?;
                head::write_text(out, "support")?;
                ExecutionCbor::write_cbor(support, out)?;
                head::write_text(out, "alternatives")?;
                ExecutionCbor::write_cbor(alternatives, out)?;
                head::write_text(out, "choice")?;
                ExecutionCbor::write_cbor(choice, out)?;
                head::write_text(out, "branches")?;
                ExecutionCbor::write_cbor(branches, out)?;
                Ok(())
            }
        }
    }

    fn check_depth(&self, remaining: usize) -> WireResult<()> {
        let remaining = descend(remaining)?;
        match self {
            Self::Conflict {
                kind,
                subjects,
                support,
                premises,
            } => {
                let remaining = descend(remaining)?;
                ExecutionCbor::check_depth(kind, remaining)?;
                ExecutionCbor::check_depth(subjects, remaining)?;
                ExecutionCbor::check_depth(support, remaining)?;
                ExecutionCbor::check_depth(premises, remaining)?;
                Ok(())
            }
            Self::Cases {
                support,
                alternatives,
                choice,
                branches,
            } => {
                let remaining = descend(remaining)?;
                ExecutionCbor::check_depth(support, remaining)?;
                ExecutionCbor::check_depth(alternatives, remaining)?;
                ExecutionCbor::check_depth(choice, remaining)?;
                ExecutionCbor::check_depth(branches, remaining)?;
                Ok(())
            }
        }
    }

    fn read_cbor(input: &mut Reader<'_>) -> WireResult<Self> {
        input.variant(|input, name, tagged| match (name, tagged) {
            ("Conflict", true) => input.map(|input, length| {
                if length != 4 {
                    return Err(input.invalid("expected complete RefutationProof::Conflict record"));
                }
                input.field("kind")?;
                let kind = ExecutionCbor::read_cbor(input)?;
                input.field("subjects")?;
                let subjects = ExecutionCbor::read_cbor(input)?;
                input.field("support")?;
                let support = ExecutionCbor::read_cbor(input)?;
                input.field("premises")?;
                let premises = ExecutionCbor::read_cbor(input)?;
                Ok(Self::Conflict {
                    kind,
                    subjects,
                    support,
                    premises,
                })
            }),
            ("Cases", true) => input.map(|input, length| {
                if length != 4 {
                    return Err(input.invalid("expected complete RefutationProof::Cases record"));
                }
                input.field("support")?;
                let support = ExecutionCbor::read_cbor(input)?;
                input.field("alternatives")?;
                let alternatives = ExecutionCbor::read_cbor(input)?;
                input.field("choice")?;
                let choice = ExecutionCbor::read_cbor(input)?;
                input.field("branches")?;
                let branches = ExecutionCbor::read_cbor(input)?;
                Ok(Self::Cases {
                    support,
                    alternatives,
                    choice,
                    branches,
                })
            }),
            _ => Err(input.invalid("invalid RefutationProof variant or payload form")),
        })
    }
}

crate::__gmeow_execution_record!(
    ContextualConflict { world, proof },
    "expected complete ContextualConflict record"
);
