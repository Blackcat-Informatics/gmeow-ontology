// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! A small owned, queryable view over a frozen [`RdfDataset`], reproducing the
//! rdflib `Graph` navigation the Python projections rely on
//! (`subjects`/`objects`/`value`/`subject_objects` and `(s,p,o) in graph`).
//!
//! Each quad is resolved into an owned `(subject_iri, predicate_iri, Object)`
//! triple. Objects are either IRIs or literals (lexical + datatype IRI + optional
//! language). This is enough for the projections, which never traverse into blank
//! nodes or graph names.

use std::collections::BTreeSet;

use gmeow_rdf::ir::TermId;
use gmeow_rdf::prelude::{RdfDataset, TermRef};

/// An owned RDF object — an IRI or a literal.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum Object {
    /// An IRI object.
    Iri(String),
    /// A literal: lexical form, datatype IRI, optional language tag.
    Literal {
        /// The lexical form.
        lexical: String,
        /// The datatype IRI (xsd:string for plain/language literals).
        datatype: String,
        /// The optional language tag.
        language: Option<String>,
    },
}

impl Object {
    /// Render as the Python `str(...)` of an rdflib node: the IRI or the literal
    /// lexical form.
    pub fn as_str(&self) -> &str {
        match self {
            Object::Iri(s) => s,
            Object::Literal { lexical, .. } => lexical,
        }
    }
    /// Return the IRI string if this object is an IRI.
    pub fn iri(&self) -> Option<&str> {
        match self {
            Object::Iri(s) => Some(s),
            _ => None,
        }
    }
}

/// An owned triple `(subject_iri, predicate_iri, object)`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Triple {
    /// The subject IRI (blank-node subjects are skipped at build time).
    pub s: String,
    /// The predicate IRI.
    pub p: String,
    /// The object.
    pub o: Object,
}

/// An owned view over the dataset, supporting the projections' navigation.
pub struct GraphView {
    triples: Vec<Triple>,
}

impl GraphView {
    /// Build a view from a frozen dataset. Quads whose subject or predicate are
    /// not IRIs (none arise in this importer) are skipped.
    pub fn from_dataset(dataset: &RdfDataset) -> Self {
        let mut triples = Vec::new();
        for q in dataset.quads() {
            let s = match resolve_iri(dataset, q.s) {
                Some(s) => s,
                None => continue,
            };
            let p = match resolve_iri(dataset, q.p) {
                Some(p) => p,
                None => continue,
            };
            let o = match dataset.resolve(q.o) {
                TermRef::Iri(iri) => Object::Iri(iri.to_string()),
                TermRef::Literal {
                    lexical,
                    datatype,
                    language,
                    ..
                } => {
                    let dt = resolve_iri(dataset, datatype).unwrap_or_default();
                    Object::Literal {
                        lexical: lexical.to_string(),
                        datatype: dt,
                        language: language.map(|l| l.to_string()),
                    }
                }
                // Blank/triple objects do not arise in the foundation graph.
                _ => continue,
            };
            triples.push(Triple { s, p, o });
        }
        Self { triples }
    }

    /// All triples (read-only).
    pub fn triples(&self) -> &[Triple] {
        &self.triples
    }

    /// `(s, p, o)` membership test where `o` is an IRI (`(s, rdf:type, T) in graph`).
    pub fn has_iri(&self, s: &str, p: &str, o: &str) -> bool {
        self.triples
            .iter()
            .any(|t| t.s == s && t.p == p && t.o == Object::Iri(o.to_string()))
    }

    /// `graph.subjects(p, None)`: subjects with predicate `p` (as a set).
    pub fn subjects_of(&self, p: &str) -> BTreeSet<String> {
        self.triples
            .iter()
            .filter(|t| t.p == p)
            .map(|t| t.s.clone())
            .collect()
    }

    /// `graph.subjects(p, o)` where `o` is an IRI: subjects with predicate `p`,
    /// object `o`.
    pub fn subjects_with_object(&self, p: &str, o: &str) -> BTreeSet<String> {
        self.triples
            .iter()
            .filter(|t| t.p == p && t.o.iri() == Some(o))
            .map(|t| t.s.clone())
            .collect()
    }

    /// `graph.objects(None, p)`: all objects of predicate `p` (as a set of
    /// IRI strings; literals are ignored — the projections only use this for
    /// IRI-valued predicates).
    pub fn object_iris_of_predicate(&self, p: &str) -> BTreeSet<String> {
        self.triples
            .iter()
            .filter(|t| t.p == p)
            .filter_map(|t| t.o.iri().map(|s| s.to_string()))
            .collect()
    }

    /// `graph.objects(s, p)` IRI objects for a given subject+predicate.
    pub fn object_iris(&self, s: &str, p: &str) -> BTreeSet<String> {
        self.triples
            .iter()
            .filter(|t| t.s == s && t.p == p)
            .filter_map(|t| t.o.iri().map(|x| x.to_string()))
            .collect()
    }

    /// `graph.value(s, p)`: a single object for `(s, p)`. The fixture has exactly
    /// one in every use; for determinism we pick the minimum object.
    pub fn value(&self, s: &str, p: &str) -> Option<Object> {
        self.triples
            .iter()
            .filter(|t| t.s == s && t.p == p)
            .map(|t| t.o.clone())
            .min()
    }

    /// `graph.value(s, p)` returning the IRI string only.
    pub fn value_iri(&self, s: &str, p: &str) -> Option<String> {
        self.value(s, p)
            .and_then(|o| o.iri().map(|x| x.to_string()))
    }

    /// `graph.subject_objects(p)`: `(subject, object)` pairs for predicate `p`,
    /// objects rendered as strings (IRI or lexical).
    pub fn subject_objects(&self, p: &str) -> Vec<(String, String)> {
        self.triples
            .iter()
            .filter(|t| t.p == p)
            .map(|t| (t.s.clone(), t.o.as_str().to_string()))
            .collect()
    }
}

fn resolve_iri(dataset: &RdfDataset, id: TermId) -> Option<String> {
    match dataset.resolve(id) {
        TermRef::Iri(iri) => Some(iri.to_string()),
        _ => None,
    }
}
