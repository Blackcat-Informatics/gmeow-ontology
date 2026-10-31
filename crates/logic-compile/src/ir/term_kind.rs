// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Canonical intrinsic predicates over original RDF values, not class membership.

mod json;

/// The closed set of borrowed RDF components available to bound identity tests.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum TermComponent {
    /// The complete native RDF term.
    Whole,
    /// A literal's exact datatype IRI.
    Datatype,
    /// A quoted triple's subject term.
    TripleSubject,
    /// A quoted triple's predicate term.
    TriplePredicate,
    /// A quoted triple's object term.
    TripleObject,
}

impl TermComponent {
    /// The canonical selector individual in the logic namespace.
    #[must_use]
    pub const fn local_name(self) -> &'static str {
        match self {
            Self::Whole => "componentWhole",
            Self::Datatype => "componentDatatype",
            Self::TripleSubject => "componentTripleSubject",
            Self::TriplePredicate => "componentTriplePredicate",
            Self::TripleObject => "componentTripleObject",
        }
    }

    /// Admit only an exact IRI from the closed canonical selector vocabulary.
    #[must_use]
    pub fn from_iri(iri: &str) -> Option<Self> {
        match iri.strip_prefix(super::LOGIC_NAMESPACE)? {
            "componentWhole" => Some(Self::Whole),
            "componentDatatype" => Some(Self::Datatype),
            "componentTripleSubject" => Some(Self::TripleSubject),
            "componentTriplePredicate" => Some(Self::TriplePredicate),
            "componentTripleObject" => Some(Self::TripleObject),
            _ => None,
        }
    }

    /// Compare borrowed components by RDF identity. An unavailable component is false.
    #[must_use]
    pub fn same_component(
        self,
        left: &purrdf::TermValue,
        right_component: Self,
        right: &purrdf::TermValue,
    ) -> bool {
        match (self.project(left), right_component.project(right)) {
            (Some(ComponentRef::Term(a)), Some(ComponentRef::Term(b))) => a == b,
            (Some(ComponentRef::Iri(a)), Some(ComponentRef::Iri(b))) => a == b,
            (Some(ComponentRef::Iri(a)), Some(ComponentRef::Term(purrdf::TermValue::Iri(b))))
            | (Some(ComponentRef::Term(purrdf::TermValue::Iri(b))), Some(ComponentRef::Iri(a))) => {
                a == b
            }
            _ => false,
        }
    }

    fn project(self, term: &purrdf::TermValue) -> Option<ComponentRef<'_>> {
        match (self, term) {
            (Self::Whole, value) => Some(ComponentRef::Term(value)),
            (Self::Datatype, purrdf::TermValue::Literal { datatype, .. }) => {
                Some(ComponentRef::Iri(datatype))
            }
            (Self::TripleSubject, purrdf::TermValue::Triple { s, .. }) => {
                Some(ComponentRef::Term(s))
            }
            (Self::TriplePredicate, purrdf::TermValue::Triple { p, .. }) => {
                Some(ComponentRef::Term(p))
            }
            (Self::TripleObject, purrdf::TermValue::Triple { o, .. }) => {
                Some(ComponentRef::Term(o))
            }
            _ => None,
        }
    }
}

enum ComponentRef<'a> {
    Term(&'a purrdf::TermValue),
    Iri(&'a str),
}

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
    /// A literal with a nonempty native language tag, including base direction.
    LanguageTagged,
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
            Self::LanguageTagged => "termHasLang",
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
            "termHasLang" => Some(Self::LanguageTagged),
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
            Self::LanguageTagged => matches!(
                value,
                purrdf::TermValue::Literal { language: Some(tag), .. } if !tag.is_empty()
            ),
        }
    }
}
