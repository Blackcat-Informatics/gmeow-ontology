// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Complete owner-local native execution receipt records, without semantic reconstruction.

use std::io;

use purrdf_lex::cbor::head;

use crate::execution_cbor::{ExecutionCbor, Reader, Result as WireResult, descend};

use super::{
    NativeAnalysisUsage, NativeBoundEvidence, NativeClosureStatus, NativeFamilyCompletion,
    NativeFamilyLedger, NativeFamilyObstruction, NativeFamilyOutcome, NativeObligationScope,
    NativeObstructionKind, NativeProducerObstruction, NativeProofId, NativeProofNode,
    NativeProofOrigin, NativeRead, NativeReadKind, NativeRefutationFamily, NativeSourceTerms,
    NativeSupportedClash,
};

// NativeProofId is a Serde newtype over the retained integer-array digest.
impl ExecutionCbor for NativeProofId {
    fn write_cbor<W: crate::execution_cbor::LeafWrite + ?Sized>(
        &self,
        out: &mut W,
    ) -> io::Result<()> {
        ExecutionCbor::write_cbor(&self.0, out)
    }
    fn check_depth(&self, remaining: usize) -> WireResult<()> {
        ExecutionCbor::check_depth(&self.0, remaining)
    }

    fn read_cbor(input: &mut Reader<'_>) -> WireResult<Self> {
        Ok(Self(ExecutionCbor::read_cbor(input)?))
    }
}

impl ExecutionCbor for NativeProofOrigin {
    fn write_cbor<W: crate::execution_cbor::LeafWrite + ?Sized>(
        &self,
        out: &mut W,
    ) -> io::Result<()> {
        match self {
            Self::Asserted { sources, terms } => {
                head::write_head(out, head::MAP, 1)?;
                head::write_text(out, "Asserted")?;
                head::write_head(out, head::MAP, 2)?;
                head::write_text(out, "sources")?;
                ExecutionCbor::write_cbor(sources, out)?;
                head::write_text(out, "terms")?;
                ExecutionCbor::write_cbor(terms, out)?;
                Ok(())
            }
            Self::Derived {
                rule,
                derivation_id,
                premises,
            } => {
                head::write_head(out, head::MAP, 1)?;
                head::write_text(out, "Derived")?;
                head::write_head(out, head::MAP, 3)?;
                head::write_text(out, "rule")?;
                ExecutionCbor::write_cbor(rule, out)?;
                head::write_text(out, "derivation_id")?;
                ExecutionCbor::write_cbor(derivation_id, out)?;
                head::write_text(out, "premises")?;
                ExecutionCbor::write_cbor(premises, out)?;
                Ok(())
            }
            Self::Modal { evidence } => {
                head::write_head(out, head::MAP, 1)?;
                head::write_text(out, "Modal")?;
                head::write_head(out, head::MAP, 1)?;
                head::write_text(out, "evidence")?;
                ExecutionCbor::write_cbor(evidence, out)?;
                Ok(())
            }
            Self::Contextual { receipt } => {
                head::write_head(out, head::MAP, 1)?;
                head::write_text(out, "Contextual")?;
                head::write_head(out, head::MAP, 1)?;
                head::write_text(out, "receipt")?;
                ExecutionCbor::write_cbor(receipt, out)?;
                Ok(())
            }
            Self::Intrinsic { witness } => {
                head::write_head(out, head::MAP, 1)?;
                head::write_text(out, "Intrinsic")?;
                head::write_head(out, head::MAP, 1)?;
                head::write_text(out, "witness")?;
                ExecutionCbor::write_cbor(witness, out)?;
                Ok(())
            }
        }
    }

    fn check_depth(&self, remaining: usize) -> WireResult<()> {
        let remaining = descend(remaining)?;
        match self {
            Self::Asserted { sources, terms } => {
                let remaining = descend(remaining)?;
                ExecutionCbor::check_depth(sources, remaining)?;
                ExecutionCbor::check_depth(terms, remaining)?;
                Ok(())
            }
            Self::Derived {
                rule,
                derivation_id,
                premises,
            } => {
                let remaining = descend(remaining)?;
                ExecutionCbor::check_depth(rule, remaining)?;
                ExecutionCbor::check_depth(derivation_id, remaining)?;
                ExecutionCbor::check_depth(premises, remaining)?;
                Ok(())
            }
            Self::Modal { evidence } => {
                let remaining = descend(remaining)?;
                ExecutionCbor::check_depth(evidence, remaining)?;
                Ok(())
            }
            Self::Contextual { receipt } => {
                let remaining = descend(remaining)?;
                ExecutionCbor::check_depth(receipt, remaining)?;
                Ok(())
            }
            Self::Intrinsic { witness } => {
                let remaining = descend(remaining)?;
                ExecutionCbor::check_depth(witness, remaining)?;
                Ok(())
            }
        }
    }

    fn read_cbor(input: &mut Reader<'_>) -> WireResult<Self> {
        input.variant(|input, name, tagged| match (name, tagged) {
            ("Asserted", true) => input.map(|input, length| {
                if length != 2 {
                    return Err(
                        input.invalid("expected complete NativeProofOrigin::Asserted record")
                    );
                }
                input.field("sources")?;
                let sources = ExecutionCbor::read_cbor(input)?;
                input.field("terms")?;
                let terms = ExecutionCbor::read_cbor(input)?;
                Ok(Self::Asserted { sources, terms })
            }),
            ("Derived", true) => input.map(|input, length| {
                if length != 3 {
                    return Err(
                        input.invalid("expected complete NativeProofOrigin::Derived record")
                    );
                }
                input.field("rule")?;
                let rule = ExecutionCbor::read_cbor(input)?;
                input.field("derivation_id")?;
                let derivation_id = ExecutionCbor::read_cbor(input)?;
                input.field("premises")?;
                let premises = ExecutionCbor::read_cbor(input)?;
                Ok(Self::Derived {
                    rule,
                    derivation_id,
                    premises,
                })
            }),
            ("Modal", true) => input.map(|input, length| {
                if length != 1 {
                    return Err(input.invalid("expected complete NativeProofOrigin::Modal record"));
                }
                input.field("evidence")?;
                let evidence = ExecutionCbor::read_cbor(input)?;
                Ok(Self::Modal { evidence })
            }),
            ("Contextual", true) => input.map(|input, length| {
                if length != 1 {
                    return Err(
                        input.invalid("expected complete NativeProofOrigin::Contextual record")
                    );
                }
                input.field("receipt")?;
                let receipt = ExecutionCbor::read_cbor(input)?;
                Ok(Self::Contextual { receipt })
            }),
            ("Intrinsic", true) => input.map(|input, length| {
                if length != 1 {
                    return Err(
                        input.invalid("expected complete NativeProofOrigin::Intrinsic record")
                    );
                }
                input.field("witness")?;
                let witness = ExecutionCbor::read_cbor(input)?;
                Ok(Self::Intrinsic { witness })
            }),
            _ => Err(input.invalid("invalid NativeProofOrigin variant or payload form")),
        })
    }
}

impl ExecutionCbor for NativeSourceTerms {
    fn write_cbor<W: crate::execution_cbor::LeafWrite + ?Sized>(
        &self,
        out: &mut W,
    ) -> io::Result<()> {
        match self {
            Self::RdfSkolem => head::write_text(out, "RdfSkolem"),
            Self::NativeFacts => head::write_text(out, "NativeFacts"),
        }
    }

    fn check_depth(&self, remaining: usize) -> WireResult<()> {
        descend(remaining)?;
        match self {
            Self::RdfSkolem | Self::NativeFacts => Ok(()),
        }
    }

    fn read_cbor(input: &mut Reader<'_>) -> WireResult<Self> {
        input.variant(|input, name, tagged| match (name, tagged) {
            ("RdfSkolem", false) => Ok(Self::RdfSkolem),
            ("NativeFacts", false) => Ok(Self::NativeFacts),
            _ => Err(input.invalid("invalid NativeSourceTerms variant or payload form")),
        })
    }
}

impl ExecutionCbor for NativeProofNode {
    fn write_cbor<W: crate::execution_cbor::LeafWrite + ?Sized>(
        &self,
        out: &mut W,
    ) -> io::Result<()> {
        head::write_head(out, head::MAP, 3)?;
        head::write_text(out, "id")?;
        ExecutionCbor::write_cbor(&self.id, out)?;
        head::write_text(out, "statement")?;
        ExecutionCbor::write_cbor(&self.statement, out)?;
        head::write_text(out, "origin")?;
        ExecutionCbor::write_cbor(&self.origin, out)?;
        Ok(())
    }

    fn check_depth(&self, remaining: usize) -> WireResult<()> {
        let remaining = descend(remaining)?;
        ExecutionCbor::check_depth(&self.id, remaining)?;
        ExecutionCbor::check_depth(&self.statement, remaining)?;
        ExecutionCbor::check_depth(&self.origin, remaining)?;
        Ok(())
    }

    fn read_cbor(input: &mut Reader<'_>) -> WireResult<Self> {
        input.map(|input, length| {
            if length != 3 {
                return Err(input.invalid("expected complete NativeProofNode record"));
            }
            input.field("id")?;
            let id = ExecutionCbor::read_cbor(input)?;
            input.field("statement")?;
            let statement = ExecutionCbor::read_cbor(input)?;
            input.field("origin")?;
            let origin = ExecutionCbor::read_cbor(input)?;
            Ok(Self {
                id,
                statement,
                origin,
            })
        })
    }
}

impl ExecutionCbor for NativeRefutationFamily {
    fn write_cbor<W: crate::execution_cbor::LeafWrite + ?Sized>(
        &self,
        out: &mut W,
    ) -> io::Result<()> {
        match self {
            Self::Cardinality => head::write_text(out, "Cardinality"),
            Self::Identity => head::write_text(out, "Identity"),
            Self::HasSelf => head::write_text(out, "HasSelf"),
            Self::Datatype => head::write_text(out, "Datatype"),
        }
    }

    fn check_depth(&self, remaining: usize) -> WireResult<()> {
        descend(remaining)?;
        match self {
            Self::Cardinality | Self::Identity | Self::HasSelf | Self::Datatype => Ok(()),
        }
    }

    fn read_cbor(input: &mut Reader<'_>) -> WireResult<Self> {
        input.variant(|input, name, tagged| match (name, tagged) {
            ("Cardinality", false) => Ok(Self::Cardinality),
            ("Identity", false) => Ok(Self::Identity),
            ("HasSelf", false) => Ok(Self::HasSelf),
            ("Datatype", false) => Ok(Self::Datatype),
            _ => Err(input.invalid("invalid NativeRefutationFamily variant or payload form")),
        })
    }
}

impl ExecutionCbor for NativeObligationScope {
    fn write_cbor<W: crate::execution_cbor::LeafWrite + ?Sized>(
        &self,
        out: &mut W,
    ) -> io::Result<()> {
        match self {
            Self::World => head::write_text(out, "World"),
            Self::Definition { owner } => {
                head::write_head(out, head::MAP, 1)?;
                head::write_text(out, "Definition")?;
                head::write_head(out, head::MAP, 1)?;
                head::write_text(out, "owner")?;
                ExecutionCbor::write_cbor(owner, out)?;
                Ok(())
            }
            Self::Property {
                individual,
                property,
                restrictions,
            } => {
                head::write_head(out, head::MAP, 1)?;
                head::write_text(out, "Property")?;
                head::write_head(out, head::MAP, 3)?;
                head::write_text(out, "individual")?;
                ExecutionCbor::write_cbor(individual, out)?;
                head::write_text(out, "property")?;
                ExecutionCbor::write_cbor(property, out)?;
                head::write_text(out, "restrictions")?;
                ExecutionCbor::write_cbor(restrictions, out)?;
                Ok(())
            }
        }
    }

    fn check_depth(&self, remaining: usize) -> WireResult<()> {
        let remaining = descend(remaining)?;
        match self {
            Self::World => Ok(()),
            Self::Definition { owner } => {
                let remaining = descend(remaining)?;
                ExecutionCbor::check_depth(owner, remaining)?;
                Ok(())
            }
            Self::Property {
                individual,
                property,
                restrictions,
            } => {
                let remaining = descend(remaining)?;
                ExecutionCbor::check_depth(individual, remaining)?;
                ExecutionCbor::check_depth(property, remaining)?;
                ExecutionCbor::check_depth(restrictions, remaining)?;
                Ok(())
            }
        }
    }

    fn read_cbor(input: &mut Reader<'_>) -> WireResult<Self> {
        input.variant(|input, name, tagged| match (name, tagged) {
            ("World", false) => Ok(Self::World),
            ("Definition", true) => input.map(|input, length| {
                if length != 1 {
                    return Err(
                        input.invalid("expected complete NativeObligationScope::Definition record")
                    );
                }
                input.field("owner")?;
                let owner = ExecutionCbor::read_cbor(input)?;
                Ok(Self::Definition { owner })
            }),
            ("Property", true) => input.map(|input, length| {
                if length != 3 {
                    return Err(
                        input.invalid("expected complete NativeObligationScope::Property record")
                    );
                }
                input.field("individual")?;
                let individual = ExecutionCbor::read_cbor(input)?;
                input.field("property")?;
                let property = ExecutionCbor::read_cbor(input)?;
                input.field("restrictions")?;
                let restrictions = ExecutionCbor::read_cbor(input)?;
                Ok(Self::Property {
                    individual,
                    property,
                    restrictions,
                })
            }),
            _ => Err(input.invalid("invalid NativeObligationScope variant or payload form")),
        })
    }
}

impl ExecutionCbor for NativeBoundEvidence {
    fn write_cbor<W: crate::execution_cbor::LeafWrite + ?Sized>(
        &self,
        out: &mut W,
    ) -> io::Result<()> {
        head::write_head(out, head::MAP, 8)?;
        head::write_text(out, "owner")?;
        ExecutionCbor::write_cbor(&self.owner, out)?;
        head::write_text(out, "predicate")?;
        ExecutionCbor::write_cbor(&self.predicate, out)?;
        head::write_text(out, "value")?;
        ExecutionCbor::write_cbor(&self.value, out)?;
        head::write_text(out, "kind")?;
        ExecutionCbor::write_cbor(&self.kind, out)?;
        head::write_text(out, "interpreted")?;
        ExecutionCbor::write_cbor(&self.interpreted, out)?;
        head::write_text(out, "qualifier")?;
        ExecutionCbor::write_cbor(&self.qualifier, out)?;
        head::write_text(out, "capacity")?;
        ExecutionCbor::write_cbor(&self.capacity, out)?;
        head::write_text(out, "support")?;
        ExecutionCbor::write_cbor(&self.support, out)?;
        Ok(())
    }

    fn check_depth(&self, remaining: usize) -> WireResult<()> {
        let remaining = descend(remaining)?;
        ExecutionCbor::check_depth(&self.owner, remaining)?;
        ExecutionCbor::check_depth(&self.predicate, remaining)?;
        ExecutionCbor::check_depth(&self.value, remaining)?;
        ExecutionCbor::check_depth(&self.kind, remaining)?;
        ExecutionCbor::check_depth(&self.interpreted, remaining)?;
        ExecutionCbor::check_depth(&self.qualifier, remaining)?;
        ExecutionCbor::check_depth(&self.capacity, remaining)?;
        ExecutionCbor::check_depth(&self.support, remaining)?;
        Ok(())
    }

    fn read_cbor(input: &mut Reader<'_>) -> WireResult<Self> {
        input.map(|input, length| {
            if length != 8 {
                return Err(input.invalid("expected complete NativeBoundEvidence record"));
            }
            input.field("owner")?;
            let owner = ExecutionCbor::read_cbor(input)?;
            input.field("predicate")?;
            let predicate = ExecutionCbor::read_cbor(input)?;
            input.field("value")?;
            let value = ExecutionCbor::read_cbor(input)?;
            input.field("kind")?;
            let kind = ExecutionCbor::read_cbor(input)?;
            input.field("interpreted")?;
            let interpreted = ExecutionCbor::read_cbor(input)?;
            input.field("qualifier")?;
            let qualifier = ExecutionCbor::read_cbor(input)?;
            input.field("capacity")?;
            let capacity = ExecutionCbor::read_cbor(input)?;
            input.field("support")?;
            let support = ExecutionCbor::read_cbor(input)?;
            Ok(Self {
                owner,
                predicate,
                value,
                kind,
                interpreted,
                qualifier,
                capacity,
                support,
            })
        })
    }
}

impl ExecutionCbor for NativeSupportedClash {
    fn write_cbor<W: crate::execution_cbor::LeafWrite + ?Sized>(
        &self,
        out: &mut W,
    ) -> io::Result<()> {
        head::write_head(out, head::MAP, 4)?;
        head::write_text(out, "subject")?;
        ExecutionCbor::write_cbor(&self.subject, out)?;
        head::write_text(out, "rule")?;
        ExecutionCbor::write_cbor(&self.rule, out)?;
        head::write_text(out, "committed")?;
        ExecutionCbor::write_cbor(&self.committed, out)?;
        head::write_text(out, "support")?;
        ExecutionCbor::write_cbor(&self.support, out)?;
        Ok(())
    }

    fn check_depth(&self, remaining: usize) -> WireResult<()> {
        let remaining = descend(remaining)?;
        ExecutionCbor::check_depth(&self.subject, remaining)?;
        ExecutionCbor::check_depth(&self.rule, remaining)?;
        ExecutionCbor::check_depth(&self.committed, remaining)?;
        ExecutionCbor::check_depth(&self.support, remaining)?;
        Ok(())
    }

    fn read_cbor(input: &mut Reader<'_>) -> WireResult<Self> {
        input.map(|input, length| {
            if length != 4 {
                return Err(input.invalid("expected complete NativeSupportedClash record"));
            }
            input.field("subject")?;
            let subject = ExecutionCbor::read_cbor(input)?;
            input.field("rule")?;
            let rule = ExecutionCbor::read_cbor(input)?;
            input.field("committed")?;
            let committed = ExecutionCbor::read_cbor(input)?;
            input.field("support")?;
            let support = ExecutionCbor::read_cbor(input)?;
            Ok(Self {
                subject,
                rule,
                committed,
                support,
            })
        })
    }
}

impl ExecutionCbor for NativeReadKind {
    fn write_cbor<W: crate::execution_cbor::LeafWrite + ?Sized>(
        &self,
        out: &mut W,
    ) -> io::Result<()> {
        match self {
            Self::Positive => head::write_text(out, "Positive"),
            Self::Completed => head::write_text(out, "Completed"),
        }
    }

    fn check_depth(&self, remaining: usize) -> WireResult<()> {
        descend(remaining)?;
        match self {
            Self::Positive | Self::Completed => Ok(()),
        }
    }

    fn read_cbor(input: &mut Reader<'_>) -> WireResult<Self> {
        input.variant(|input, name, tagged| match (name, tagged) {
            ("Positive", false) => Ok(Self::Positive),
            ("Completed", false) => Ok(Self::Completed),
            _ => Err(input.invalid("invalid NativeReadKind variant or payload form")),
        })
    }
}

impl ExecutionCbor for NativeRead {
    fn write_cbor<W: crate::execution_cbor::LeafWrite + ?Sized>(
        &self,
        out: &mut W,
    ) -> io::Result<()> {
        head::write_head(out, head::MAP, 3)?;
        head::write_text(out, "predicate")?;
        ExecutionCbor::write_cbor(&self.predicate, out)?;
        head::write_text(out, "marker")?;
        ExecutionCbor::write_cbor(&self.marker, out)?;
        head::write_text(out, "kind")?;
        ExecutionCbor::write_cbor(&self.kind, out)?;
        Ok(())
    }

    fn check_depth(&self, remaining: usize) -> WireResult<()> {
        let remaining = descend(remaining)?;
        ExecutionCbor::check_depth(&self.predicate, remaining)?;
        ExecutionCbor::check_depth(&self.marker, remaining)?;
        ExecutionCbor::check_depth(&self.kind, remaining)?;
        Ok(())
    }

    fn read_cbor(input: &mut Reader<'_>) -> WireResult<Self> {
        input.map(|input, length| {
            if length != 3 {
                return Err(input.invalid("expected complete NativeRead record"));
            }
            input.field("predicate")?;
            let predicate = ExecutionCbor::read_cbor(input)?;
            input.field("marker")?;
            let marker = ExecutionCbor::read_cbor(input)?;
            input.field("kind")?;
            let kind = ExecutionCbor::read_cbor(input)?;
            Ok(Self {
                predicate,
                marker,
                kind,
            })
        })
    }
}

impl ExecutionCbor for NativeObstructionKind {
    fn write_cbor<W: crate::execution_cbor::LeafWrite + ?Sized>(
        &self,
        out: &mut W,
    ) -> io::Result<()> {
        match self {
            Self::ResourceLimit => head::write_text(out, "ResourceLimit"),
            Self::SourceShape => head::write_text(out, "SourceShape"),
            Self::UnsupportedValue => head::write_text(out, "UnsupportedValue"),
            Self::UnsupportedCombination => head::write_text(out, "UnsupportedCombination"),
            Self::UnmodeledFeature => head::write_text(out, "UnmodeledFeature"),
            Self::IncompleteDefinition => head::write_text(out, "IncompleteDefinition"),
        }
    }

    fn check_depth(&self, remaining: usize) -> WireResult<()> {
        descend(remaining)?;
        match self {
            Self::ResourceLimit
            | Self::SourceShape
            | Self::UnsupportedValue
            | Self::UnsupportedCombination
            | Self::UnmodeledFeature
            | Self::IncompleteDefinition => Ok(()),
        }
    }

    fn read_cbor(input: &mut Reader<'_>) -> WireResult<Self> {
        input.variant(|input, name, tagged| match (name, tagged) {
            ("ResourceLimit", false) => Ok(Self::ResourceLimit),
            ("SourceShape", false) => Ok(Self::SourceShape),
            ("UnsupportedValue", false) => Ok(Self::UnsupportedValue),
            ("UnsupportedCombination", false) => Ok(Self::UnsupportedCombination),
            ("UnmodeledFeature", false) => Ok(Self::UnmodeledFeature),
            ("IncompleteDefinition", false) => Ok(Self::IncompleteDefinition),
            _ => Err(input.invalid("invalid NativeObstructionKind variant or payload form")),
        })
    }
}

impl ExecutionCbor for NativeFamilyObstruction {
    fn write_cbor<W: crate::execution_cbor::LeafWrite + ?Sized>(
        &self,
        out: &mut W,
    ) -> io::Result<()> {
        head::write_head(out, head::MAP, 3)?;
        head::write_text(out, "kind")?;
        ExecutionCbor::write_cbor(&self.kind, out)?;
        head::write_text(out, "detail")?;
        ExecutionCbor::write_cbor(&self.detail, out)?;
        head::write_text(out, "support")?;
        ExecutionCbor::write_cbor(&self.support, out)?;
        Ok(())
    }

    fn check_depth(&self, remaining: usize) -> WireResult<()> {
        let remaining = descend(remaining)?;
        ExecutionCbor::check_depth(&self.kind, remaining)?;
        ExecutionCbor::check_depth(&self.detail, remaining)?;
        ExecutionCbor::check_depth(&self.support, remaining)?;
        Ok(())
    }

    fn read_cbor(input: &mut Reader<'_>) -> WireResult<Self> {
        input.map(|input, length| {
            if length != 3 {
                return Err(input.invalid("expected complete NativeFamilyObstruction record"));
            }
            input.field("kind")?;
            let kind = ExecutionCbor::read_cbor(input)?;
            input.field("detail")?;
            let detail = ExecutionCbor::read_cbor(input)?;
            input.field("support")?;
            let support = ExecutionCbor::read_cbor(input)?;
            Ok(Self {
                kind,
                detail,
                support,
            })
        })
    }
}

impl ExecutionCbor for NativeProducerObstruction {
    fn write_cbor<W: crate::execution_cbor::LeafWrite + ?Sized>(
        &self,
        out: &mut W,
    ) -> io::Result<()> {
        head::write_head(out, head::MAP, 2)?;
        head::write_text(out, "producer")?;
        ExecutionCbor::write_cbor(&self.producer, out)?;
        head::write_text(out, "reason")?;
        ExecutionCbor::write_cbor(&self.reason, out)?;
        Ok(())
    }

    fn check_depth(&self, remaining: usize) -> WireResult<()> {
        let remaining = descend(remaining)?;
        ExecutionCbor::check_depth(&self.producer, remaining)?;
        ExecutionCbor::check_depth(&self.reason, remaining)?;
        Ok(())
    }

    fn read_cbor(input: &mut Reader<'_>) -> WireResult<Self> {
        input.map(|input, length| {
            if length != 2 {
                return Err(input.invalid("expected complete NativeProducerObstruction record"));
            }
            input.field("producer")?;
            let producer = ExecutionCbor::read_cbor(input)?;
            input.field("reason")?;
            let reason = ExecutionCbor::read_cbor(input)?;
            Ok(Self { producer, reason })
        })
    }
}

impl ExecutionCbor for NativeFamilyCompletion {
    fn write_cbor<W: crate::execution_cbor::LeafWrite + ?Sized>(
        &self,
        out: &mut W,
    ) -> io::Result<()> {
        match self {
            Self::NotEngaged => head::write_text(out, "NotEngaged"),
            Self::Complete => head::write_text(out, "Complete"),
            Self::Awaiting { reads } => {
                head::write_head(out, head::MAP, 1)?;
                head::write_text(out, "Awaiting")?;
                head::write_head(out, head::MAP, 1)?;
                head::write_text(out, "reads")?;
                ExecutionCbor::write_cbor(reads, out)?;
                Ok(())
            }
            Self::Obstructed => head::write_text(out, "Obstructed"),
            Self::Blocked { reads } => {
                head::write_head(out, head::MAP, 1)?;
                head::write_text(out, "Blocked")?;
                head::write_head(out, head::MAP, 1)?;
                head::write_text(out, "reads")?;
                ExecutionCbor::write_cbor(reads, out)?;
                Ok(())
            }
            Self::Exhausted => head::write_text(out, "Exhausted"),
        }
    }

    fn check_depth(&self, remaining: usize) -> WireResult<()> {
        let remaining = descend(remaining)?;
        match self {
            Self::NotEngaged | Self::Complete | Self::Obstructed | Self::Exhausted => Ok(()),
            Self::Awaiting { reads } | Self::Blocked { reads } => {
                let remaining = descend(remaining)?;
                ExecutionCbor::check_depth(reads, remaining)?;
                Ok(())
            }
        }
    }

    fn read_cbor(input: &mut Reader<'_>) -> WireResult<Self> {
        input.variant(|input, name, tagged| match (name, tagged) {
            ("NotEngaged", false) => Ok(Self::NotEngaged),
            ("Complete", false) => Ok(Self::Complete),
            ("Awaiting", true) => input.map(|input, length| {
                if length != 1 {
                    return Err(
                        input.invalid("expected complete NativeFamilyCompletion::Awaiting record")
                    );
                }
                input.field("reads")?;
                let reads = ExecutionCbor::read_cbor(input)?;
                Ok(Self::Awaiting { reads })
            }),
            ("Obstructed", false) => Ok(Self::Obstructed),
            ("Blocked", true) => input.map(|input, length| {
                if length != 1 {
                    return Err(
                        input.invalid("expected complete NativeFamilyCompletion::Blocked record")
                    );
                }
                input.field("reads")?;
                let reads = ExecutionCbor::read_cbor(input)?;
                Ok(Self::Blocked { reads })
            }),
            ("Exhausted", false) => Ok(Self::Exhausted),
            _ => Err(input.invalid("invalid NativeFamilyCompletion variant or payload form")),
        })
    }
}

impl ExecutionCbor for NativeFamilyOutcome {
    fn write_cbor<W: crate::execution_cbor::LeafWrite + ?Sized>(
        &self,
        out: &mut W,
    ) -> io::Result<()> {
        head::write_head(out, head::MAP, 6)?;
        head::write_text(out, "family")?;
        ExecutionCbor::write_cbor(&self.family, out)?;
        head::write_text(out, "obligation")?;
        ExecutionCbor::write_cbor(&self.obligation, out)?;
        head::write_text(out, "bounds")?;
        ExecutionCbor::write_cbor(&self.bounds, out)?;
        head::write_text(out, "conclusions")?;
        ExecutionCbor::write_cbor(&self.conclusions, out)?;
        head::write_text(out, "obstructions")?;
        ExecutionCbor::write_cbor(&self.obstructions, out)?;
        head::write_text(out, "completion")?;
        ExecutionCbor::write_cbor(&self.completion, out)?;
        Ok(())
    }

    fn check_depth(&self, remaining: usize) -> WireResult<()> {
        let remaining = descend(remaining)?;
        ExecutionCbor::check_depth(&self.family, remaining)?;
        ExecutionCbor::check_depth(&self.obligation, remaining)?;
        ExecutionCbor::check_depth(&self.bounds, remaining)?;
        ExecutionCbor::check_depth(&self.conclusions, remaining)?;
        ExecutionCbor::check_depth(&self.obstructions, remaining)?;
        ExecutionCbor::check_depth(&self.completion, remaining)?;
        Ok(())
    }

    fn read_cbor(input: &mut Reader<'_>) -> WireResult<Self> {
        input.map(|input, length| {
            if length != 6 {
                return Err(input.invalid("expected complete NativeFamilyOutcome record"));
            }
            input.field("family")?;
            let family = ExecutionCbor::read_cbor(input)?;
            input.field("obligation")?;
            let obligation = ExecutionCbor::read_cbor(input)?;
            input.field("bounds")?;
            let bounds = ExecutionCbor::read_cbor(input)?;
            input.field("conclusions")?;
            let conclusions = ExecutionCbor::read_cbor(input)?;
            input.field("obstructions")?;
            let obstructions = ExecutionCbor::read_cbor(input)?;
            input.field("completion")?;
            let completion = ExecutionCbor::read_cbor(input)?;
            Ok(Self {
                family,
                obligation,
                bounds,
                conclusions,
                obstructions,
                completion,
            })
        })
    }
}

impl ExecutionCbor for NativeClosureStatus {
    fn write_cbor<W: crate::execution_cbor::LeafWrite + ?Sized>(
        &self,
        out: &mut W,
    ) -> io::Result<()> {
        match self {
            Self::Completed => head::write_text(out, "Completed"),
            Self::Exhausted => head::write_text(out, "Exhausted"),
            Self::Blocked { reads } => {
                head::write_head(out, head::MAP, 1)?;
                head::write_text(out, "Blocked")?;
                head::write_head(out, head::MAP, 1)?;
                head::write_text(out, "reads")?;
                ExecutionCbor::write_cbor(reads, out)?;
                Ok(())
            }
        }
    }

    fn check_depth(&self, remaining: usize) -> WireResult<()> {
        let remaining = descend(remaining)?;
        match self {
            Self::Completed | Self::Exhausted => Ok(()),
            Self::Blocked { reads } => {
                let remaining = descend(remaining)?;
                ExecutionCbor::check_depth(reads, remaining)?;
                Ok(())
            }
        }
    }

    fn read_cbor(input: &mut Reader<'_>) -> WireResult<Self> {
        input.variant(|input, name, tagged| match (name, tagged) {
            ("Completed", false) => Ok(Self::Completed),
            ("Exhausted", false) => Ok(Self::Exhausted),
            ("Blocked", true) => input.map(|input, length| {
                if length != 1 {
                    return Err(
                        input.invalid("expected complete NativeClosureStatus::Blocked record")
                    );
                }
                input.field("reads")?;
                let reads = ExecutionCbor::read_cbor(input)?;
                Ok(Self::Blocked { reads })
            }),
            _ => Err(input.invalid("invalid NativeClosureStatus variant or payload form")),
        })
    }
}

impl ExecutionCbor for NativeAnalysisUsage {
    fn write_cbor<W: crate::execution_cbor::LeafWrite + ?Sized>(
        &self,
        out: &mut W,
    ) -> io::Result<()> {
        head::write_head(out, head::MAP, 3)?;
        head::write_text(out, "allowance")?;
        ExecutionCbor::write_cbor(&self.allowance, out)?;
        head::write_text(out, "consumed")?;
        ExecutionCbor::write_cbor(&self.consumed, out)?;
        head::write_text(out, "exhausted")?;
        ExecutionCbor::write_cbor(&self.exhausted, out)?;
        Ok(())
    }

    fn check_depth(&self, remaining: usize) -> WireResult<()> {
        let remaining = descend(remaining)?;
        ExecutionCbor::check_depth(&self.allowance, remaining)?;
        ExecutionCbor::check_depth(&self.consumed, remaining)?;
        ExecutionCbor::check_depth(&self.exhausted, remaining)?;
        Ok(())
    }

    fn read_cbor(input: &mut Reader<'_>) -> WireResult<Self> {
        input.map(|input, length| {
            if length != 3 {
                return Err(input.invalid("expected complete NativeAnalysisUsage record"));
            }
            input.field("allowance")?;
            let allowance = ExecutionCbor::read_cbor(input)?;
            input.field("consumed")?;
            let consumed = ExecutionCbor::read_cbor(input)?;
            input.field("exhausted")?;
            let exhausted = ExecutionCbor::read_cbor(input)?;
            Ok(Self {
                allowance,
                consumed,
                exhausted,
            })
        })
    }
}

impl ExecutionCbor for NativeFamilyLedger {
    fn write_cbor<W: crate::execution_cbor::LeafWrite + ?Sized>(
        &self,
        out: &mut W,
    ) -> io::Result<()> {
        head::write_head(out, head::MAP, 9)?;
        head::write_text(out, "source_terms")?;
        ExecutionCbor::write_cbor(&self.source_terms, out)?;
        head::write_text(out, "world")?;
        ExecutionCbor::write_cbor(&self.world, out)?;
        head::write_text(out, "graph")?;
        ExecutionCbor::write_cbor(&self.graph, out)?;
        head::write_text(out, "input_contract")?;
        ExecutionCbor::write_cbor(&self.input_contract, out)?;
        head::write_text(out, "outcomes")?;
        ExecutionCbor::write_cbor(&self.outcomes, out)?;
        head::write_text(out, "producer_obstructions")?;
        ExecutionCbor::write_cbor(&self.producer_obstructions, out)?;
        head::write_text(out, "proofs")?;
        ExecutionCbor::write_cbor(&self.proofs, out)?;
        head::write_text(out, "contextual_receipts")?;
        ExecutionCbor::write_cbor(&self.contextual_receipts, out)?;
        head::write_text(out, "work")?;
        ExecutionCbor::write_cbor(&self.work, out)?;
        Ok(())
    }

    fn check_depth(&self, remaining: usize) -> WireResult<()> {
        let remaining = descend(remaining)?;
        ExecutionCbor::check_depth(&self.source_terms, remaining)?;
        ExecutionCbor::check_depth(&self.world, remaining)?;
        ExecutionCbor::check_depth(&self.graph, remaining)?;
        ExecutionCbor::check_depth(&self.input_contract, remaining)?;
        ExecutionCbor::check_depth(&self.outcomes, remaining)?;
        ExecutionCbor::check_depth(&self.producer_obstructions, remaining)?;
        ExecutionCbor::check_depth(&self.proofs, remaining)?;
        ExecutionCbor::check_depth(&self.contextual_receipts, remaining)?;
        ExecutionCbor::check_depth(&self.work, remaining)?;
        Ok(())
    }

    fn read_cbor(input: &mut Reader<'_>) -> WireResult<Self> {
        input.map(|input, length| {
            if length != 9 {
                return Err(input.invalid("expected complete NativeFamilyLedger record"));
            }
            input.field("source_terms")?;
            let source_terms = ExecutionCbor::read_cbor(input)?;
            input.field("world")?;
            let world = ExecutionCbor::read_cbor(input)?;
            input.field("graph")?;
            let graph = ExecutionCbor::read_cbor(input)?;
            input.field("input_contract")?;
            let input_contract = ExecutionCbor::read_cbor(input)?;
            input.field("outcomes")?;
            let outcomes = ExecutionCbor::read_cbor(input)?;
            input.field("producer_obstructions")?;
            let producer_obstructions = ExecutionCbor::read_cbor(input)?;
            input.field("proofs")?;
            let proofs = ExecutionCbor::read_cbor(input)?;
            input.field("contextual_receipts")?;
            let contextual_receipts = ExecutionCbor::read_cbor(input)?;
            input.field("work")?;
            let work = ExecutionCbor::read_cbor(input)?;
            Ok(Self {
                source_terms,
                world,
                graph,
                input_contract,
                outcomes,
                producer_obstructions,
                proofs,
                contextual_receipts,
                work,
            })
        })
    }
}
