// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Canonical intrinsic predicates over original RDF values, not class membership.

/// The authored term-kind predicates available to integrity constraints.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize,
)]
pub enum TermKindPredicate {
    /// An RDF IRI.
    Iri,
    /// An RDF literal, retaining its datatype, language and direction.
    Literal,
    /// An RDF blank node or IRI; a triple term is neither.
    BlankOrIri,
    /// A well-formed finite literal in the native RDF numeric value space.
    FiniteNumeric,
}

impl TermKindPredicate {
    /// Read the existing canonical node-kind authoring tokens.
    #[must_use]
    pub fn from_authored_token(token: &str) -> Option<Self> {
        match token.trim() {
            "IRI" | "iri" => Some(Self::Iri),
            "Literal" | "literal" => Some(Self::Literal),
            "BlankNodeOrIRI" | "BlankNodeOrIri" | "blankNodeOrIri" => Some(Self::BlankOrIri),
            _ => None,
        }
    }

    /// The predicate's canonical local name in the logic namespace.
    #[must_use]
    pub const fn local_name(self) -> &'static str {
        match self {
            Self::Iri => "termIsIri",
            Self::Literal => "termIsLiteral",
            Self::BlankOrIri => "termIsBlankOrIri",
            Self::FiniteNumeric => "termIsFiniteNumeric",
        }
    }

    /// Recognize a canonical intrinsic relation, without accepting a lookalike namespace.
    #[must_use]
    pub fn from_relation(relation: &str) -> Option<Self> {
        match relation.strip_prefix(super::LOGIC_NAMESPACE)? {
            "termIsIri" => Some(Self::Iri),
            "termIsLiteral" => Some(Self::Literal),
            "termIsBlankOrIri" => Some(Self::BlankOrIri),
            "termIsFiniteNumeric" => Some(Self::FiniteNumeric),
            _ => None,
        }
    }

    /// Decide directly on the native RDF value; no asserted typing fact is consulted.
    #[must_use]
    pub fn holds(self, value: &purrdf::TermValue) -> bool {
        match self {
            Self::Iri => value.is_iri(),
            Self::Literal => value.is_literal(),
            Self::BlankOrIri => value.is_blank() || value.is_iri(),
            Self::FiniteNumeric => super::FiniteNumericLiteral::try_from(value.clone()).is_ok(),
        }
    }
}
