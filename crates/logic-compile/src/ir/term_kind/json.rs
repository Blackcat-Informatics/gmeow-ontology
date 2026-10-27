// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Native persistent spellings of canonical intrinsic predicates.
use super::TermKindPredicate;
use purrdf_lex::json::{
    Value,
    record::{DecodeError, FromJson, ToJson},
};
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
