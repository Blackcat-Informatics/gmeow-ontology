// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Complete owner-local native execution receipt records, without semantic reconstruction.

use std::io::{self, Write};

use purrdf_lex::cbor::head;

use crate::execution_cbor::{ExecutionCbor, Reader, Result as WireResult, descend};

use super::{
    DlConstructFamily, NativeConstructAdmission, NativeConstructOccurrence,
    NativeSourceGrammarBasis, SourceCoverageObservation, SourceCoverageWorld,
};

impl ExecutionCbor for DlConstructFamily {
    fn write_cbor<W: Write + ?Sized>(&self, out: &mut W) -> io::Result<()> {
        match self {
            Self::ComplementOf => head::write_text(out, "ComplementOf"),
            Self::SomeValuesFrom => head::write_text(out, "SomeValuesFrom"),
            Self::AllValuesFrom => head::write_text(out, "AllValuesFrom"),
            Self::Cardinality => head::write_text(out, "Cardinality"),
            Self::MinCardinality => head::write_text(out, "MinCardinality"),
            Self::MaxCardinality => head::write_text(out, "MaxCardinality"),
            Self::QualifiedCardinality => head::write_text(out, "QualifiedCardinality"),
            Self::MinQualifiedCardinality => head::write_text(out, "MinQualifiedCardinality"),
            Self::MaxQualifiedCardinality => head::write_text(out, "MaxQualifiedCardinality"),
            Self::DisjointUnionOf => head::write_text(out, "DisjointUnionOf"),
            Self::UnionOf => head::write_text(out, "UnionOf"),
            Self::OneOf => head::write_text(out, "OneOf"),
            Self::HasValue => head::write_text(out, "HasValue"),
            Self::Domain => head::write_text(out, "Domain"),
            Self::Range => head::write_text(out, "Range"),
            Self::PropertyChainAxiom => head::write_text(out, "PropertyChainAxiom"),
            Self::BottomObjectProperty => head::write_text(out, "BottomObjectProperty"),
            Self::BottomDataProperty => head::write_text(out, "BottomDataProperty"),
            Self::HasKey => head::write_text(out, "HasKey"),
            Self::NegativePropertyAssertion => head::write_text(out, "NegativePropertyAssertion"),
            Self::FunctionalProperty => head::write_text(out, "FunctionalProperty"),
            Self::AsymmetricProperty => head::write_text(out, "AsymmetricProperty"),
            Self::IrreflexiveProperty => head::write_text(out, "IrreflexiveProperty"),
            Self::PropertyDisjointWith => head::write_text(out, "PropertyDisjointWith"),
            Self::AllDisjointProperties => head::write_text(out, "AllDisjointProperties"),
            Self::AllDisjointClasses => head::write_text(out, "AllDisjointClasses"),
            Self::AllDifferent => head::write_text(out, "AllDifferent"),
            Self::InverseFunctionalProperty => head::write_text(out, "InverseFunctionalProperty"),
            Self::DatatypeComplementOf => head::write_text(out, "DatatypeComplementOf"),
            Self::WithRestrictions => head::write_text(out, "WithRestrictions"),
            Self::OnDatatype => head::write_text(out, "OnDatatype"),
            Self::MinInclusive => head::write_text(out, "MinInclusive"),
            Self::MaxInclusive => head::write_text(out, "MaxInclusive"),
            Self::MinExclusive => head::write_text(out, "MinExclusive"),
            Self::MaxExclusive => head::write_text(out, "MaxExclusive"),
            Self::Pattern => head::write_text(out, "Pattern"),
            Self::Length => head::write_text(out, "Length"),
            Self::MinLength => head::write_text(out, "MinLength"),
            Self::MaxLength => head::write_text(out, "MaxLength"),
            Self::TotalDigits => head::write_text(out, "TotalDigits"),
            Self::FractionDigits => head::write_text(out, "FractionDigits"),
            Self::LangRange => head::write_text(out, "LangRange"),
            Self::TopObjectProperty => head::write_text(out, "TopObjectProperty"),
            Self::TopDataProperty => head::write_text(out, "TopDataProperty"),
            Self::HasSelf => head::write_text(out, "HasSelf"),
            Self::IntersectionOf => head::write_text(out, "IntersectionOf"),
            Self::WhiteSpace => head::write_text(out, "WhiteSpace"),
            Self::ExplicitTimezone => head::write_text(out, "ExplicitTimezone"),
        }
    }

    fn check_depth(&self, remaining: usize) -> WireResult<()> {
        descend(remaining)?;
        match self {
            Self::ComplementOf
            | Self::SomeValuesFrom
            | Self::AllValuesFrom
            | Self::Cardinality
            | Self::MinCardinality
            | Self::MaxCardinality
            | Self::QualifiedCardinality
            | Self::MinQualifiedCardinality
            | Self::MaxQualifiedCardinality
            | Self::DisjointUnionOf
            | Self::UnionOf
            | Self::OneOf
            | Self::HasValue
            | Self::Domain
            | Self::Range
            | Self::PropertyChainAxiom
            | Self::BottomObjectProperty
            | Self::BottomDataProperty
            | Self::HasKey
            | Self::NegativePropertyAssertion
            | Self::FunctionalProperty
            | Self::AsymmetricProperty
            | Self::IrreflexiveProperty
            | Self::PropertyDisjointWith
            | Self::AllDisjointProperties
            | Self::AllDisjointClasses
            | Self::AllDifferent
            | Self::InverseFunctionalProperty
            | Self::DatatypeComplementOf
            | Self::WithRestrictions
            | Self::OnDatatype
            | Self::MinInclusive
            | Self::MaxInclusive
            | Self::MinExclusive
            | Self::MaxExclusive
            | Self::Pattern
            | Self::Length
            | Self::MinLength
            | Self::MaxLength
            | Self::TotalDigits
            | Self::FractionDigits
            | Self::LangRange
            | Self::TopObjectProperty
            | Self::TopDataProperty
            | Self::HasSelf
            | Self::IntersectionOf
            | Self::WhiteSpace
            | Self::ExplicitTimezone => Ok(()),
        }
    }

    fn read_cbor(input: &mut Reader<'_>) -> WireResult<Self> {
        input.variant(|input, name, tagged| match (name, tagged) {
            ("ComplementOf", false) => Ok(Self::ComplementOf),
            ("SomeValuesFrom", false) => Ok(Self::SomeValuesFrom),
            ("AllValuesFrom", false) => Ok(Self::AllValuesFrom),
            ("Cardinality", false) => Ok(Self::Cardinality),
            ("MinCardinality", false) => Ok(Self::MinCardinality),
            ("MaxCardinality", false) => Ok(Self::MaxCardinality),
            ("QualifiedCardinality", false) => Ok(Self::QualifiedCardinality),
            ("MinQualifiedCardinality", false) => Ok(Self::MinQualifiedCardinality),
            ("MaxQualifiedCardinality", false) => Ok(Self::MaxQualifiedCardinality),
            ("DisjointUnionOf", false) => Ok(Self::DisjointUnionOf),
            ("UnionOf", false) => Ok(Self::UnionOf),
            ("OneOf", false) => Ok(Self::OneOf),
            ("HasValue", false) => Ok(Self::HasValue),
            ("Domain", false) => Ok(Self::Domain),
            ("Range", false) => Ok(Self::Range),
            ("PropertyChainAxiom", false) => Ok(Self::PropertyChainAxiom),
            ("BottomObjectProperty", false) => Ok(Self::BottomObjectProperty),
            ("BottomDataProperty", false) => Ok(Self::BottomDataProperty),
            ("HasKey", false) => Ok(Self::HasKey),
            ("NegativePropertyAssertion", false) => Ok(Self::NegativePropertyAssertion),
            ("FunctionalProperty", false) => Ok(Self::FunctionalProperty),
            ("AsymmetricProperty", false) => Ok(Self::AsymmetricProperty),
            ("IrreflexiveProperty", false) => Ok(Self::IrreflexiveProperty),
            ("PropertyDisjointWith", false) => Ok(Self::PropertyDisjointWith),
            ("AllDisjointProperties", false) => Ok(Self::AllDisjointProperties),
            ("AllDisjointClasses", false) => Ok(Self::AllDisjointClasses),
            ("AllDifferent", false) => Ok(Self::AllDifferent),
            ("InverseFunctionalProperty", false) => Ok(Self::InverseFunctionalProperty),
            ("DatatypeComplementOf", false) => Ok(Self::DatatypeComplementOf),
            ("WithRestrictions", false) => Ok(Self::WithRestrictions),
            ("OnDatatype", false) => Ok(Self::OnDatatype),
            ("MinInclusive", false) => Ok(Self::MinInclusive),
            ("MaxInclusive", false) => Ok(Self::MaxInclusive),
            ("MinExclusive", false) => Ok(Self::MinExclusive),
            ("MaxExclusive", false) => Ok(Self::MaxExclusive),
            ("Pattern", false) => Ok(Self::Pattern),
            ("Length", false) => Ok(Self::Length),
            ("MinLength", false) => Ok(Self::MinLength),
            ("MaxLength", false) => Ok(Self::MaxLength),
            ("TotalDigits", false) => Ok(Self::TotalDigits),
            ("FractionDigits", false) => Ok(Self::FractionDigits),
            ("LangRange", false) => Ok(Self::LangRange),
            ("TopObjectProperty", false) => Ok(Self::TopObjectProperty),
            ("TopDataProperty", false) => Ok(Self::TopDataProperty),
            ("HasSelf", false) => Ok(Self::HasSelf),
            ("IntersectionOf", false) => Ok(Self::IntersectionOf),
            ("WhiteSpace", false) => Ok(Self::WhiteSpace),
            ("ExplicitTimezone", false) => Ok(Self::ExplicitTimezone),
            _ => Err(input.invalid("invalid DlConstructFamily variant or payload form")),
        })
    }
}

impl ExecutionCbor for NativeConstructOccurrence {
    fn write_cbor<W: Write + ?Sized>(&self, out: &mut W) -> io::Result<()> {
        head::write_head(out, head::MAP, 3)?;
        head::write_text(out, "family")?;
        ExecutionCbor::write_cbor(&self.family, out)?;
        head::write_text(out, "statement")?;
        ExecutionCbor::write_cbor(&self.statement, out)?;
        head::write_text(out, "support")?;
        ExecutionCbor::write_cbor(&self.support, out)?;
        Ok(())
    }

    fn check_depth(&self, remaining: usize) -> WireResult<()> {
        let remaining = descend(remaining)?;
        ExecutionCbor::check_depth(&self.family, remaining)?;
        ExecutionCbor::check_depth(&self.statement, remaining)?;
        ExecutionCbor::check_depth(&self.support, remaining)?;
        Ok(())
    }

    fn read_cbor(input: &mut Reader<'_>) -> WireResult<Self> {
        input.map(|input, length| {
            if length != 3 {
                return Err(input.invalid("expected complete NativeConstructOccurrence record"));
            }
            input.field("family")?;
            let family = ExecutionCbor::read_cbor(input)?;
            input.field("statement")?;
            let statement = ExecutionCbor::read_cbor(input)?;
            input.field("support")?;
            let support = ExecutionCbor::read_cbor(input)?;
            Ok(Self {
                family,
                statement,
                support,
            })
        })
    }
}

impl ExecutionCbor for SourceCoverageWorld {
    fn write_cbor<W: Write + ?Sized>(&self, out: &mut W) -> io::Result<()> {
        head::write_head(out, head::MAP, 3)?;
        head::write_text(out, "graph")?;
        ExecutionCbor::write_cbor(&self.graph, out)?;
        head::write_text(out, "constructs")?;
        ExecutionCbor::write_cbor(&self.constructs, out)?;
        head::write_text(out, "admissions")?;
        ExecutionCbor::write_cbor(&self.admissions, out)?;
        Ok(())
    }

    fn check_depth(&self, remaining: usize) -> WireResult<()> {
        let remaining = descend(remaining)?;
        ExecutionCbor::check_depth(&self.graph, remaining)?;
        ExecutionCbor::check_depth(&self.constructs, remaining)?;
        ExecutionCbor::check_depth(&self.admissions, remaining)?;
        Ok(())
    }

    fn read_cbor(input: &mut Reader<'_>) -> WireResult<Self> {
        input.map(|input, length| {
            if length != 3 {
                return Err(input.invalid("expected complete SourceCoverageWorld record"));
            }
            input.field("graph")?;
            let graph = ExecutionCbor::read_cbor(input)?;
            input.field("constructs")?;
            let constructs = ExecutionCbor::read_cbor(input)?;
            input.field("admissions")?;
            let admissions = ExecutionCbor::read_cbor(input)?;
            Ok(Self {
                graph,
                constructs,
                admissions,
            })
        })
    }
}

impl ExecutionCbor for NativeSourceGrammarBasis {
    fn write_cbor<W: Write + ?Sized>(&self, out: &mut W) -> io::Result<()> {
        match self {
            Self::AwaitingWriters => head::write_text(out, "AwaitingWriters"),
            Self::CompletedWriters => head::write_text(out, "CompletedWriters"),
            Self::ConstructorClosedLists {
                commitment,
                world,
                producers,
            } => {
                head::write_head(out, head::MAP, 1)?;
                head::write_text(out, "ConstructorClosedLists")?;
                head::write_head(out, head::MAP, 3)?;
                head::write_text(out, "commitment")?;
                ExecutionCbor::write_cbor(commitment, out)?;
                head::write_text(out, "world")?;
                ExecutionCbor::write_cbor(world, out)?;
                head::write_text(out, "producers")?;
                ExecutionCbor::write_cbor(producers, out)?;
                Ok(())
            }
        }
    }

    fn check_depth(&self, remaining: usize) -> WireResult<()> {
        let remaining = descend(remaining)?;
        match self {
            Self::AwaitingWriters | Self::CompletedWriters => Ok(()),
            Self::ConstructorClosedLists {
                commitment,
                world,
                producers,
            } => {
                let remaining = descend(remaining)?;
                ExecutionCbor::check_depth(commitment, remaining)?;
                ExecutionCbor::check_depth(world, remaining)?;
                ExecutionCbor::check_depth(producers, remaining)?;
                Ok(())
            }
        }
    }

    fn read_cbor(input: &mut Reader<'_>) -> WireResult<Self> {
        input.variant(|input, name, tagged| match (name, tagged) {
            ("AwaitingWriters", false) => Ok(Self::AwaitingWriters),
            ("CompletedWriters", false) => Ok(Self::CompletedWriters),
            ("ConstructorClosedLists", true) => input.map(|input, length| {
                if length != 3 {
                    return Err(input.invalid(
                        "expected complete NativeSourceGrammarBasis::ConstructorClosedLists record",
                    ));
                }
                input.field("commitment")?;
                let commitment = ExecutionCbor::read_cbor(input)?;
                input.field("world")?;
                let world = ExecutionCbor::read_cbor(input)?;
                input.field("producers")?;
                let producers = ExecutionCbor::read_cbor(input)?;
                Ok(Self::ConstructorClosedLists {
                    commitment,
                    world,
                    producers,
                })
            }),
            _ => Err(input.invalid("invalid NativeSourceGrammarBasis variant or payload form")),
        })
    }
}

impl ExecutionCbor for NativeConstructAdmission {
    fn write_cbor<W: Write + ?Sized>(&self, out: &mut W) -> io::Result<()> {
        head::write_head(out, head::MAP, 7)?;
        head::write_text(out, "family")?;
        ExecutionCbor::write_cbor(&self.family, out)?;
        head::write_text(out, "owner")?;
        ExecutionCbor::write_cbor(&self.owner, out)?;
        head::write_text(out, "selectors")?;
        ExecutionCbor::write_cbor(&self.selectors, out)?;
        head::write_text(out, "support")?;
        ExecutionCbor::write_cbor(&self.support, out)?;
        head::write_text(out, "grammar_basis")?;
        ExecutionCbor::write_cbor(&self.grammar_basis, out)?;
        head::write_text(out, "completion")?;
        ExecutionCbor::write_cbor(&self.completion, out)?;
        head::write_text(out, "obstructions")?;
        ExecutionCbor::write_cbor(&self.obstructions, out)?;
        Ok(())
    }

    fn check_depth(&self, remaining: usize) -> WireResult<()> {
        let remaining = descend(remaining)?;
        ExecutionCbor::check_depth(&self.family, remaining)?;
        ExecutionCbor::check_depth(&self.owner, remaining)?;
        ExecutionCbor::check_depth(&self.selectors, remaining)?;
        ExecutionCbor::check_depth(&self.support, remaining)?;
        ExecutionCbor::check_depth(&self.grammar_basis, remaining)?;
        ExecutionCbor::check_depth(&self.completion, remaining)?;
        ExecutionCbor::check_depth(&self.obstructions, remaining)?;
        Ok(())
    }

    fn read_cbor(input: &mut Reader<'_>) -> WireResult<Self> {
        input.map(|input, length| {
            if length != 7 {
                return Err(input.invalid("expected complete NativeConstructAdmission record"));
            }
            input.field("family")?;
            let family = ExecutionCbor::read_cbor(input)?;
            input.field("owner")?;
            let owner = ExecutionCbor::read_cbor(input)?;
            input.field("selectors")?;
            let selectors = ExecutionCbor::read_cbor(input)?;
            input.field("support")?;
            let support = ExecutionCbor::read_cbor(input)?;
            input.field("grammar_basis")?;
            let grammar_basis = ExecutionCbor::read_cbor(input)?;
            input.field("completion")?;
            let completion = ExecutionCbor::read_cbor(input)?;
            input.field("obstructions")?;
            let obstructions = ExecutionCbor::read_cbor(input)?;
            Ok(Self {
                family,
                owner,
                selectors,
                support,
                grammar_basis,
                completion,
                obstructions,
            })
        })
    }
}

impl ExecutionCbor for SourceCoverageObservation {
    fn write_cbor<W: Write + ?Sized>(&self, out: &mut W) -> io::Result<()> {
        head::write_head(out, head::MAP, 2)?;
        head::write_text(out, "input_contract")?;
        ExecutionCbor::write_cbor(&self.input_contract, out)?;
        head::write_text(out, "worlds")?;
        ExecutionCbor::write_cbor(&self.worlds, out)?;
        Ok(())
    }

    fn check_depth(&self, remaining: usize) -> WireResult<()> {
        let remaining = descend(remaining)?;
        ExecutionCbor::check_depth(&self.input_contract, remaining)?;
        ExecutionCbor::check_depth(&self.worlds, remaining)?;
        Ok(())
    }

    fn read_cbor(input: &mut Reader<'_>) -> WireResult<Self> {
        input.map(|input, length| {
            if length != 2 {
                return Err(input.invalid("expected complete SourceCoverageObservation record"));
            }
            input.field("input_contract")?;
            let input_contract = ExecutionCbor::read_cbor(input)?;
            input.field("worlds")?;
            let worlds = ExecutionCbor::read_cbor(input)?;
            Ok(Self {
                input_contract,
                worlds,
            })
        })
    }
}
