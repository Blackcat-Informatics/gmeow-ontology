// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Native persistent spellings of canonical intrinsic predicates.
use super::{TermComponent, TermKindPredicate};
use purrdf_lex::json::{
    Value,
    record::{DecodeError, FromJson, ToJson},
};

impl ToJson for TermComponent {
    fn to_json(&self) -> Value {
        match self {
            Self::Whole => "Whole",
            Self::Datatype => "Datatype",
            Self::TripleSubject => "TripleSubject",
            Self::TriplePredicate => "TriplePredicate",
            Self::TripleObject => "TripleObject",
        }
        .to_json()
    }
}

impl FromJson for TermComponent {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        match gmeow_errors::json::read_unit_name(value, "TermComponent")? {
            "Whole" => Ok(Self::Whole),
            "Datatype" => Ok(Self::Datatype),
            "TripleSubject" => Ok(Self::TripleSubject),
            "TriplePredicate" => Ok(Self::TriplePredicate),
            "TripleObject" => Ok(Self::TripleObject),
            name => Err(DecodeError::unknown_variant(
                name,
                &[
                    "Whole",
                    "Datatype",
                    "TripleSubject",
                    "TriplePredicate",
                    "TripleObject",
                ],
            )),
        }
    }
}
impl ToJson for TermKindPredicate {
    fn to_json(&self) -> Value {
        match self {
            Self::Iri => "Iri",
            Self::Literal => "Literal",
            Self::BlankOrIri => "BlankOrIri",
            Self::FiniteNumeric => "FiniteNumeric",
            Self::LanguageTagged => "LanguageTagged",
        }
        .to_json()
    }
}
impl FromJson for TermKindPredicate {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        match gmeow_errors::json::read_unit_name(value, "TermKindPredicate")? {
            "Iri" => Ok(Self::Iri),
            "Literal" => Ok(Self::Literal),
            "BlankOrIri" => Ok(Self::BlankOrIri),
            "FiniteNumeric" => Ok(Self::FiniteNumeric),
            "LanguageTagged" => Ok(Self::LanguageTagged),
            name => Err(DecodeError::unknown_variant(
                name,
                &[
                    "Iri",
                    "Literal",
                    "BlankOrIri",
                    "FiniteNumeric",
                    "LanguageTagged",
                ],
            )),
        }
    }
}
