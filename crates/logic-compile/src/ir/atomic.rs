// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! One native atomic term representation across compact source IR and lowering.

use purrdf::RdfLiteral;

/// The shared atomic term of compact axioms and relational-core rules: a logical variable (`?x`), an IRI constant, a blank node
/// (an existential the IR carries as a canonical `c14nN` label), or a literal value
/// (object position only). The relational core is **binary** — a fact/atom is a
/// `subject predicate object` triple.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum AtomicTerm {
    /// A logical variable carrying its `?` sigil.
    Var(String),
    /// An IRI constant (absolute IRI).
    Iri(String),
    /// A blank node carrying its canonical label (no `_:` prefix), e.g. a Skolem-style
    /// existential the lowering keeps by reference.
    Blank(String),
    /// A literal value (legal only in the object position).
    Literal(#[serde(with = "crate::ir::literal_serde::single")] RdfLiteral),
}

impl Ord for AtomicTerm {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        let tag = |term: &AtomicTerm| match term {
            Self::Var(_) => 0,
            Self::Iri(_) => 1,
            Self::Blank(_) => 2,
            Self::Literal(_) => 3,
        };
        tag(self)
            .cmp(&tag(other))
            .then_with(|| match (self, other) {
                (Self::Var(a), Self::Var(b))
                | (Self::Iri(a), Self::Iri(b))
                | (Self::Blank(a), Self::Blank(b)) => a.cmp(b),
                (Self::Literal(a), Self::Literal(b)) => crate::ir::literal_serde::sort_key(a)
                    .cmp(&crate::ir::literal_serde::sort_key(b)),
                _ => std::cmp::Ordering::Equal,
            })
    }
}
impl PartialOrd for AtomicTerm {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
impl PartialEq for AtomicTerm {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other).is_eq()
    }
}
impl Eq for AtomicTerm {}

impl AtomicTerm {
    /// Parse a compact resource token: an explicit variable, absolute IRI or blank label.
    /// Literal values never pass through this resource-only constructor.
    pub fn resource(value: impl Into<String>) -> Self {
        let value = value.into();
        if value.starts_with('?') {
            Self::Var(value)
        } else if is_absolute_iri(&value) {
            Self::Iri(value)
        } else {
            Self::Blank(value.trim_start_matches("_:").to_owned())
        }
    }

    /// The complete literal, when this term is literal-valued.
    pub fn as_literal(&self) -> Option<&RdfLiteral> {
        if let Self::Literal(value) = self {
            Some(value)
        } else {
            None
        }
    }

    /// Whether this term is literal-valued. Its lexical prefix never changes its kind.
    pub fn is_literal(&self) -> bool {
        matches!(self, Self::Literal(_))
    }

    /// An IRI constant, without accepting variables or blank labels as IRIs.
    pub fn as_iri(&self) -> Option<&str> {
        if let Self::Iri(value) = self {
            Some(value)
        } else {
            None
        }
    }

    /// An explicitly typed variable, including its compact question-mark sigil.
    pub fn as_variable(&self) -> Option<&str> {
        if let Self::Var(value) = self {
            Some(value)
        } else {
            None
        }
    }

    /// A native RDF value, when this term is not a logical variable.
    pub fn rdf_term(&self) -> Option<purrdf::RdfTerm> {
        match self {
            Self::Var(_) => None,
            Self::Iri(iri) => Some(purrdf::RdfTerm::iri(iri.clone())),
            Self::Blank(label) => Some(purrdf::RdfTerm::blank_node(label.clone())),
            Self::Literal(literal) => Some(purrdf::RdfTerm::literal(literal.clone())),
        }
    }

    /// A deterministic, type-tagged content key for this term — the single authority shared
    /// by the projection, the parse inverse, and the engine adapter's rule-IRI minting.
    pub fn key(&self) -> String {
        match self {
            Self::Var(v) => frame("V", [v.as_str()]),
            Self::Iri(i) => frame("I", [i.as_str()]),
            Self::Blank(b) => frame("B", [b.as_str()]),
            Self::Literal(l) => frame("L", [crate::ir::literal_serde::key(l)]),
        }
    }
}

/// Whether `value` is an absolute IRI (carries a `scheme:` prefix). A bare token (a
/// canonicalized blank-node label like `c14n44`) is NOT absolute.
pub(crate) fn is_absolute_iri(value: &str) -> bool {
    // An absolute IRI has a scheme: an alpha followed by alnum/+/-/. then ':'.
    match value.find(':') {
        Some(0) => false,
        Some(idx) => {
            let scheme = &value[..idx];
            scheme.starts_with(|c: char| c.is_ascii_alphabetic())
                && scheme
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'))
        }
        None => false,
    }
}

/// Unambiguous length framing shared by atomic terms and enclosing content keys.
pub(crate) fn frame(tag: &str, fields: impl IntoIterator<Item = impl AsRef<str>>) -> String {
    let mut result = format!("{tag}:");
    for field in fields {
        let field = field.as_ref();
        result.push_str(&field.len().to_string());
        result.push(':');
        result.push_str(field);
    }
    result
}
