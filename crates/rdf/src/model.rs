// SPDX-FileCopyrightText: 2026 Blackcat Informatics Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use crate::RdfLocation;

/// RDF term category.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RdfTermKind {
    Iri,
    BlankNode,
    Literal,
    Triple,
}

/// RDF 1.2 base direction for directional language-tagged literals.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RdfTextDirection {
    Ltr,
    Rtl,
}

impl RdfTextDirection {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Ltr => "ltr",
            Self::Rtl => "rtl",
        }
    }
}

/// An RDF literal, including RDF 1.2 language direction when available.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RdfLiteral {
    pub lexical_form: String,
    pub datatype: Option<String>,
    pub language: Option<String>,
    pub direction: Option<RdfTextDirection>,
}

impl RdfLiteral {
    pub fn simple(lexical_form: impl Into<String>) -> Self {
        Self {
            lexical_form: lexical_form.into(),
            datatype: None,
            language: None,
            direction: None,
        }
    }

    pub fn typed(lexical_form: impl Into<String>, datatype: impl Into<String>) -> Self {
        Self {
            lexical_form: lexical_form.into(),
            datatype: Some(datatype.into()),
            language: None,
            direction: None,
        }
    }

    pub fn language_tagged(lexical_form: impl Into<String>, language: impl Into<String>) -> Self {
        Self {
            lexical_form: lexical_form.into(),
            datatype: None,
            language: Some(language.into()),
            direction: None,
        }
    }
}

/// Owned RDF 1.2 term.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum RdfTerm {
    Iri(String),
    BlankNode(String),
    Literal(RdfLiteral),
    Triple(Box<RdfTriple>),
}

impl RdfTerm {
    pub fn iri(value: impl Into<String>) -> Self {
        Self::Iri(value.into())
    }

    pub fn blank_node(value: impl Into<String>) -> Self {
        Self::BlankNode(value.into())
    }

    pub fn literal(literal: RdfLiteral) -> Self {
        Self::Literal(literal)
    }

    pub fn triple(triple: RdfTriple) -> Self {
        Self::Triple(Box::new(triple))
    }

    pub fn kind(&self) -> RdfTermKind {
        match self {
            Self::Iri(_) => RdfTermKind::Iri,
            Self::BlankNode(_) => RdfTermKind::BlankNode,
            Self::Literal(_) => RdfTermKind::Literal,
            Self::Triple(_) => RdfTermKind::Triple,
        }
    }
}

/// Owned RDF 1.2 triple. The model keeps triple-term subjects representable;
/// downstream adapters decide whether a target store can encode them.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RdfTriple {
    pub subject: RdfTerm,
    pub predicate: String,
    pub object: RdfTerm,
    pub location: Option<RdfLocation>,
}

impl RdfTriple {
    pub fn new(subject: RdfTerm, predicate: impl Into<String>, object: RdfTerm) -> Self {
        Self {
            subject,
            predicate: predicate.into(),
            object,
            location: None,
        }
    }

    pub fn with_location(mut self, location: RdfLocation) -> Self {
        if !location.is_empty() {
            self.location = Some(location);
        }
        self
    }
}

/// Owned RDF 1.2 quad with optional adapter/source context.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RdfQuad {
    pub subject: RdfTerm,
    pub predicate: String,
    pub object: RdfTerm,
    pub graph_name: Option<RdfTerm>,
    pub location: Option<RdfLocation>,
}

impl RdfQuad {
    pub fn new(subject: RdfTerm, predicate: impl Into<String>, object: RdfTerm) -> Self {
        Self {
            subject,
            predicate: predicate.into(),
            object,
            graph_name: None,
            location: None,
        }
    }

    pub fn in_graph(mut self, graph_name: RdfTerm) -> Self {
        self.graph_name = Some(graph_name);
        self
    }

    pub fn with_location(mut self, location: RdfLocation) -> Self {
        if !location.is_empty() {
            self.location = Some(location);
        }
        self
    }
}

/// RDF 1.2 reifier binding.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RdfReifier {
    pub reifier: RdfTerm,
    pub statement: RdfTriple,
    pub location: Option<RdfLocation>,
}

impl RdfReifier {
    pub fn new(reifier: RdfTerm, statement: RdfTriple) -> Self {
        Self {
            reifier,
            statement,
            location: None,
        }
    }

    pub fn with_location(mut self, location: RdfLocation) -> Self {
        if !location.is_empty() {
            self.location = Some(location);
        }
        self
    }
}

/// RDF 1.2 statement annotation.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RdfAnnotation {
    pub reifier: RdfTerm,
    pub predicate: String,
    pub object: RdfTerm,
    pub location: Option<RdfLocation>,
}

impl RdfAnnotation {
    pub fn new(reifier: RdfTerm, predicate: impl Into<String>, object: RdfTerm) -> Self {
        Self {
            reifier,
            predicate: predicate.into(),
            object,
            location: None,
        }
    }

    pub fn with_location(mut self, location: RdfLocation) -> Self {
        if !location.is_empty() {
            self.location = Some(location);
        }
        self
    }
}
