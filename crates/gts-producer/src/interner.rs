// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Term interner that mirrors `src/gmeow_tools/gts_producer.py::_Interner`.

use std::collections::HashMap;

use gmeow_gts::model::{Term, TermKind};

/// Deduplication key for a term in the builder.
///
/// The variants intentionally use owned strings so that `Option<&str>` scopes,
/// labels, lexical forms, and tags can all participate in a single `HashMap`.
#[derive(Clone, Debug, Eq, PartialEq, Hash)]
enum InternKey {
    Iri(String),
    Bnode {
        scope: Option<String>,
        label: String,
    },
    Lit {
        lexical: String,
        datatype: String,
        lang: String,
    },
}

/// Assigns stable, append-order term-ids and de-duplicates terms (§7.2).
#[derive(Clone, Debug, Default)]
pub struct Interner {
    terms: Vec<Term>,
    index: HashMap<InternKey, usize>,
}

impl Interner {
    /// Create an empty interner.
    pub fn new() -> Self {
        Self::default()
    }

    /// Return the accumulated term table.
    pub fn terms(&self) -> &[Term] {
        &self.terms
    }

    /// Intern an IRI term.
    pub fn iri(&mut self, iri: &str) -> usize {
        let key = InternKey::Iri(iri.to_owned());
        if let Some(&id) = self.index.get(&key) {
            return id;
        }
        let id = self.terms.len();
        self.terms.push(Term {
            kind: TermKind::Iri,
            value: Some(iri.to_owned()),
            datatype: None,
            lang: None,
            reifier: None,
        });
        self.index.insert(key, id);
        id
    }

    /// Intern a blank node, optionally scoped to an ingest source.
    ///
    /// Sources are canonicalized independently, so two different existential
    /// nodes in different sources can carry the same canonical label — scoping
    /// prevents them collapsing into one term. `None` preserves the raw label.
    pub fn bnode(&mut self, label: &str, scope: Option<&str>) -> usize {
        let value = match scope {
            Some(s) => format!("{s}-{label}"),
            None => label.to_owned(),
        };
        let key = InternKey::Bnode {
            scope: scope.map(str::to_owned),
            label: label.to_owned(),
        };
        if let Some(&id) = self.index.get(&key) {
            return id;
        }
        let id = self.terms.len();
        self.terms.push(Term {
            kind: TermKind::Bnode,
            value: Some(value),
            datatype: None,
            lang: None,
            reifier: None,
        });
        self.index.insert(key, id);
        id
    }

    /// Intern a literal term.
    ///
    /// `datatype` is the explicit datatype IRI, or `None` for plain/language
    /// literals. When a datatype is explicit, its IRI is interned first and the
    /// resulting term-id is stored in the literal's `datatype` field.
    pub fn literal(&mut self, lex: &str, datatype: Option<&str>, lang: Option<&str>) -> usize {
        let datatype_or_empty = datatype.unwrap_or("");
        let lang_or_empty = lang.unwrap_or("");
        let datatype_id = datatype.map(|dt| self.iri(dt));
        let key = InternKey::Lit {
            lexical: lex.to_owned(),
            datatype: datatype_or_empty.to_owned(),
            lang: lang_or_empty.to_owned(),
        };
        if let Some(&id) = self.index.get(&key) {
            return id;
        }
        let id = self.terms.len();
        self.terms.push(Term {
            kind: TermKind::Literal,
            value: Some(lex.to_owned()),
            datatype: datatype_id,
            lang: lang.map(str::to_owned),
            reifier: None,
        });
        self.index.insert(key, id);
        id
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn iri_deduplication() {
        let mut interner = Interner::new();
        let a = interner.iri("http://example.org/a");
        let b = interner.iri("http://example.org/a");
        let c = interner.iri("http://example.org/b");
        assert_eq!(a, b);
        assert_ne!(a, c);
        assert_eq!(interner.terms().len(), 2);
    }

    #[test]
    fn bnode_scope_prevents_collapse() {
        let mut interner = Interner::new();
        let a = interner.bnode("b1", Some("s1"));
        let b = interner.bnode("b1", Some("s2"));
        let c = interner.bnode("b1", None);
        let d = interner.bnode("b1", Some("s1"));
        assert_ne!(a, b);
        assert_ne!(a, c);
        assert_ne!(b, c);
        assert_eq!(a, d);
        assert_eq!(interner.terms()[a].value, Some("s1-b1".to_owned()));
        assert_eq!(interner.terms()[c].value, Some("b1".to_owned()));
    }

    #[test]
    fn literal_interns_datatype_first() {
        let mut interner = Interner::new();
        let lit_id = interner.literal("42", Some("http://www.w3.org/2001/XMLSchema#integer"), None);
        let lit = &interner.terms()[lit_id];
        assert_eq!(lit.kind, TermKind::Literal);
        assert_eq!(lit.value, Some("42".to_owned()));
        assert!(lit.datatype.is_some());
        let dt_id = lit.datatype.unwrap();
        assert!(dt_id < lit_id);
        assert_eq!(
            interner.terms()[dt_id].value,
            Some("http://www.w3.org/2001/XMLSchema#integer".to_owned())
        );
    }

    #[test]
    fn language_tagged_literal_has_no_datatype() {
        let mut interner = Interner::new();
        let id = interner.literal("chat", None, Some("fr"));
        let term = &interner.terms()[id];
        assert_eq!(term.kind, TermKind::Literal);
        assert_eq!(term.value, Some("chat".to_owned()));
        assert_eq!(term.datatype, None);
        assert_eq!(term.lang, Some("fr".to_owned()));
    }
}
