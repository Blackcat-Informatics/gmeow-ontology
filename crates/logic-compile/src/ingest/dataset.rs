// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Native RDF 1.2 values and indexed reads for correspondence lowering.
//!
//! The caller supplies an already parsed dataset. Reads select its default graph;
//! neither graph labels nor quoted propositions become assertions implicitly.

use crate::graphutil::default_graph_pattern;
use purrdf::{RdfDataset, TermId, TermRef, TermValue};

/// The alignment adapter uses PurRDF's complete native value identity, including
/// blank scope, literal direction and recursively quoted propositions.
pub type DslTerm = TermValue;

/// Borrow the lexical component of a native literal for a lexical DSL field.
pub fn literal_lexical(term: &DslTerm) -> Option<&str> {
    match term {
        DslTerm::Literal { lexical_form, .. } => Some(lexical_form),
        _ => None,
    }
}

const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
const RDF_FIRST: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#first";
const RDF_REST: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#rest";
const RDF_NIL: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#nil";

/// A quoted statement bound to its exact source dataset and reifier identity.
/// IDs stay private and invocation-local; the record cannot be re-keyed against
/// another dataset or lose a blank node's scope through lexical matching.
#[derive(Clone, Copy)]
pub struct ReifiedStatement<'a> {
    dataset: &'a RdfDataset,
    reifier: TermId,
    statement: TermId,
}

impl<'a> ReifiedStatement<'a> {
    /// Full native identity of the reifier, borrowed from the source.
    pub fn reifier(&self) -> TermRef<'a> {
        self.dataset.resolve(self.reifier)
    }

    /// Full native value of the quoted proposition, for typed IR consumers.
    pub fn value(&self) -> TermValue {
        self.dataset.term_value(self.statement)
    }

    /// Borrow the quoted triple's components without materializing owned records.
    ///
    /// # Errors
    /// Rejects an `rdf:reifies` binding whose object is not a proposition term.
    pub fn triple(&self) -> gmeow_errors::Result<(TermRef<'a>, TermRef<'a>, TermRef<'a>)> {
        match self.dataset.resolve(self.statement) {
            TermRef::Triple { s, p, o } => Ok((
                self.dataset.resolve(s),
                self.dataset.resolve(p),
                self.dataset.resolve(o),
            )),
            _ => Err(self.field_error("rdf:reifies", "requires a proposition term")),
        }
    }

    fn field_error(&self, predicate: &str, detail: &str) -> gmeow_errors::Diag {
        gmeow_errors::Diag::of_kind(crate::error::Sssom {
            detail: format!(
                "alignment reifier {:?}, {predicate}: {detail}",
                self.reifier()
            ),
        })
    }

    fn annotation_objects(&self, predicate: &str) -> impl Iterator<Item = TermRef<'a>> + '_ {
        self.dataset
            .term_id_by_iri(predicate)
            .into_iter()
            .flat_map(move |p| {
                default_graph_pattern(self.dataset, Some(self.reifier), Some(p), None)
            })
            .map(|q| self.dataset.resolve(q.o))
    }

    /// One complete native annotation value. Multiplicity is rejected at this boundary.
    ///
    /// # Errors
    /// Refuses multiple distinct values for the selected scalar coordinate.
    pub fn scalar_annotation(&self, predicate: &str) -> gmeow_errors::Result<Option<TermRef<'a>>> {
        let mut values = self.annotation_objects(predicate);
        let first = values.next();
        if values.next().is_some() {
            return Err(self.field_error(predicate, "multiple distinct values for a scalar field"));
        }
        Ok(first)
    }

    /// An IRI-valued coordinate. Wrong kind or multiplicity is an admission error.
    ///
    /// # Errors
    /// Rejects a present non-IRI or multiple distinct values.
    pub fn annotation_iri(&self, predicate: &str) -> gmeow_errors::Result<Option<&'a str>> {
        match self.scalar_annotation(predicate)? {
            None => Ok(None),
            Some(TermRef::Iri(iri)) => Ok(Some(iri)),
            Some(_) => Err(self.field_error(predicate, "requires an IRI")),
        }
    }

    /// The lexical component of a scalar literal-valued coordinate.
    ///
    /// # Errors
    /// Rejects a present non-literal or multiple distinct values.
    pub fn annotation_literal(&self, predicate: &str) -> gmeow_errors::Result<Option<&'a str>> {
        match self.scalar_annotation(predicate)? {
            None => Ok(None),
            Some(TermRef::Literal { lexical, .. }) => Ok(Some(lexical)),
            Some(_) => Err(self.field_error(predicate, "requires a literal")),
        }
    }

    /// All literal values of a multivalued coordinate, preserving every member.
    ///
    /// # Errors
    /// Rejects any non-literal member instead of silently dropping it.
    pub fn annotation_literals(&self, predicate: &str) -> gmeow_errors::Result<Vec<&'a str>> {
        let mut values = self
            .annotation_objects(predicate)
            .map(|term| match term {
                TermRef::Literal { lexical, .. } => Ok(lexical),
                _ => Err(self.field_error(predicate, "requires literal values")),
            })
            .collect::<gmeow_errors::Result<Vec<_>>>()?;
        values.sort();
        Ok(values)
    }

    /// All native RDF literal values of a multivalued coordinate.
    ///
    /// Unlike [`Self::annotation_literals`], this preserves datatype, language, and RDF 1.2
    /// base direction for typed IR fields whose literal identity is semantically relevant.
    ///
    /// # Errors
    /// Rejects any non-literal member or malformed internal datatype reference.
    pub fn annotation_rdf_literals(
        &self,
        predicate: &str,
    ) -> gmeow_errors::Result<Vec<purrdf::RdfLiteral>> {
        let mut values = self
            .annotation_objects(predicate)
            .map(|term| match term {
                TermRef::Literal {
                    lexical,
                    datatype,
                    language,
                    direction,
                } => {
                    let TermRef::Iri(datatype) = self.dataset.resolve(datatype) else {
                        return Err(
                            self.field_error(predicate, "literal datatype must resolve to an IRI")
                        );
                    };
                    Ok(purrdf::RdfLiteral {
                        lexical_form: lexical.to_owned(),
                        datatype: Some(datatype.to_owned()),
                        language: language.map(str::to_owned),
                        direction,
                    })
                }
                _ => Err(self.field_error(predicate, "requires literal values")),
            })
            .collect::<gmeow_errors::Result<Vec<_>>>()?;
        values.sort_by(|a, b| {
            (
                &a.lexical_form,
                &a.datatype,
                &a.language,
                a.direction.map(|direction| direction.as_str()),
            )
                .cmp(&(
                    &b.lexical_form,
                    &b.datatype,
                    &b.language,
                    b.direction.map(|direction| direction.as_str()),
                ))
        });
        Ok(values)
    }

    /// Structural annotation typing is multivalued and never a first-winner field.
    pub fn annotation_has_type(&self, class: &str) -> bool {
        self.annotation_objects(RDF_TYPE)
            .any(|term| matches!(term, TermRef::Iri(iri) if iri == class))
    }
}

/// A value-space read view over a parsed [`RdfDataset`] (default graph only).
pub struct DslView<'a> {
    ds: &'a RdfDataset,
}

impl<'a> DslView<'a> {
    /// The exact native dataset whose borrowed term IDs this view returns.
    pub fn dataset(&self) -> &'a RdfDataset {
        self.ds
    }

    /// Wrap a parsed dataset.
    pub fn new(ds: &'a RdfDataset) -> Self {
        Self { ds }
    }

    /// The interned id of an IRI, or `None` if the dataset has no such term (an
    /// absent term yields no quads, exactly as a missing oxigraph node would).
    fn iri_id(&self, iri: &str) -> Option<TermId> {
        self.ds.term_id_by_iri(iri)
    }

    /// The interned id of a (named or blank) subject term, or `None`.
    fn subject_id(&self, term: &DslTerm) -> Option<TermId> {
        match term {
            DslTerm::Iri(iri) => self.iri_id(iri),
            DslTerm::Blank { .. } => self.ds.term_id_by_value(term),
            DslTerm::Literal { .. } | DslTerm::Triple { .. } => None,
        }
    }

    /// Materialize a resolved term id into an owned [`DslTerm`].
    fn term_of(&self, id: TermId) -> DslTerm {
        self.ds.term_value(id)
    }

    /// Every named-node subject of `?s a <type_iri>`, sorted by IRI for a
    /// deterministic, interning-order-independent iteration.
    pub fn subjects_of_type(&self, type_iri: &str) -> Vec<String> {
        let (Some(rdf_type), Some(class)) = (self.iri_id(RDF_TYPE), self.iri_id(type_iri)) else {
            return Vec::new();
        };
        let mut subjects: Vec<String> =
            default_graph_pattern(self.ds, None, Some(rdf_type), Some(class))
                .filter_map(|q| match self.ds.resolve(q.s) {
                    TermRef::Iri(iri) => Some(iri.to_owned()),
                    _ => None,
                })
                .collect();
        subjects.sort();
        subjects.dedup();
        subjects
    }

    /// The first object term of `<subject_iri> <pred> ?o`, or `None`.
    pub fn first_object(&self, subject_iri: &str, pred: &str) -> Option<DslTerm> {
        let subject = self.iri_id(subject_iri)?;
        let predicate = self.iri_id(pred)?;
        default_graph_pattern(self.ds, Some(subject), Some(predicate), None)
            .next()
            .map(|q| self.term_of(q.o))
    }

    /// All object terms of `<subject_iri> <pred> ?o`, in dataset order.
    pub fn objects_of(&self, subject_iri: &str, pred: &str) -> Vec<DslTerm> {
        let (Some(subject), Some(predicate)) = (self.iri_id(subject_iri), self.iri_id(pred)) else {
            return Vec::new();
        };
        default_graph_pattern(self.ds, Some(subject), Some(predicate), None)
            .map(|q| self.term_of(q.o))
            .collect()
    }

    /// The first IRI object of `<subject_iri> <pred> ?o`, or `None`.
    pub fn object_iri(&self, subject_iri: &str, pred: &str) -> Option<String> {
        self.first_object(subject_iri, pred)
            .and_then(|t| t.as_iri().map(str::to_owned))
    }

    /// All IRI objects of `<subject_iri> <pred> ?o`, in dataset order.
    pub fn object_iris(&self, subject_iri: &str, pred: &str) -> Vec<String> {
        self.objects_of(subject_iri, pred)
            .into_iter()
            .filter_map(|t| match t {
                DslTerm::Iri(iri) => Some(iri),
                _ => None,
            })
            .collect()
    }

    /// The lexical form of the first literal object of `<subject_iri> <pred> ?o`.
    pub fn object_literal(&self, subject_iri: &str, pred: &str) -> Option<String> {
        match self.first_object(subject_iri, pred) {
            Some(DslTerm::Literal {
                lexical_form: lexical,
                ..
            }) => Some(lexical),
            _ => None,
        }
    }

    /// The lexical forms of ALL literal objects of `<subject_iri> <pred> ?o`, in dataset
    /// order — the multi-valued counterpart of [`Self::object_literal`].
    pub fn object_literals(&self, subject_iri: &str, pred: &str) -> Vec<String> {
        self.objects_of(subject_iri, pred)
            .into_iter()
            .filter_map(|t| match t {
                DslTerm::Literal {
                    lexical_form: lexical,
                    ..
                } => Some(lexical),
                _ => None,
            })
            .collect()
    }

    /// The first object term of `<term> <pred> ?o` where `term` is a (named or blank)
    /// subject, or `None`.
    pub fn first_object_of(&self, subject: &DslTerm, pred: &str) -> Option<DslTerm> {
        let (Some(subj), Some(p)) = (self.subject_id(subject), self.iri_id(pred)) else {
            return None;
        };
        default_graph_pattern(self.ds, Some(subj), Some(p), None)
            .next()
            .map(|q| self.term_of(q.o))
    }

    /// All object terms of `<term> <pred> ?o`, in dataset order.
    pub fn objects_of_term(&self, subject: &DslTerm, pred: &str) -> Vec<DslTerm> {
        let (Some(subj), Some(p)) = (self.subject_id(subject), self.iri_id(pred)) else {
            return Vec::new();
        };
        default_graph_pattern(self.ds, Some(subj), Some(p), None)
            .map(|q| self.term_of(q.o))
            .collect()
    }

    /// The first IRI object of `<term> <pred> ?o`, or `None`.
    pub fn object_iri_of_term(&self, subject: &DslTerm, pred: &str) -> Option<String> {
        self.first_object_of(subject, pred)
            .and_then(|t| t.as_iri().map(str::to_owned))
    }

    /// The lexical form of the first literal object of `<term> <pred> ?o`.
    pub fn object_literal_of_term(&self, subject: &DslTerm, pred: &str) -> Option<String> {
        match self.first_object_of(subject, pred) {
            Some(DslTerm::Literal {
                lexical_form: lexical,
                ..
            }) => Some(lexical),
            _ => None,
        }
    }

    /// Every IRI subject of `?s <pred> <object_iri>` in the default graph, in dataset
    /// order (the inverse-direction of [`Self::object_iris`]). Mirrors the historical
    /// oxigraph `subjects_iri(store, pred, object)` read the alignment lint used to walk
    /// `owl:inverseOf` / `schema:inverseOf` both ways.
    pub fn subjects_with_object_iri(&self, pred: &str, object_iri: &str) -> Vec<String> {
        let (Some(p), Some(obj)) = (self.iri_id(pred), self.iri_id(object_iri)) else {
            return Vec::new();
        };
        default_graph_pattern(self.ds, None, Some(p), Some(obj))
            .filter_map(|q| match self.ds.resolve(q.s) {
                TermRef::Iri(iri) => Some(iri.to_owned()),
                _ => None,
            })
            .collect()
    }

    /// Every `(subject, object)` pair of `?s <pred> ?o` in the default graph, in
    /// dataset order.
    pub fn quads_with_predicate(&self, pred: &str) -> Vec<(DslTerm, DslTerm)> {
        let Some(p) = self.iri_id(pred) else {
            return Vec::new();
        };
        default_graph_pattern(self.ds, None, Some(p), None)
            .map(|q| (self.term_of(q.s), self.term_of(q.o)))
            .collect()
    }

    /// The `rdf:type` IRIs of a (named or blank) term.
    pub fn types_of_term(&self, subject: &DslTerm) -> Vec<String> {
        self.objects_of_term(subject, RDF_TYPE)
            .into_iter()
            .filter_map(|t| match t {
                DslTerm::Iri(iri) => Some(iri),
                _ => None,
            })
            .collect()
    }

    /// The lexical form + datatype IRI of the first literal object of
    /// `<term> <pred> ?o`.
    pub fn literal_of_term(
        &self,
        subject: &DslTerm,
        pred: &str,
    ) -> Option<(String, Option<String>)> {
        match self.first_object_of(subject, pred) {
            Some(DslTerm::Literal {
                lexical_form: lexical,
                datatype,
                ..
            }) => Some((lexical, Some(datatype))),
            _ => None,
        }
    }

    /// Parse an RDF boolean object of `<term> <pred> ?o`: a literal `true`/`1` (case-
    /// and whitespace-insensitive) is `true`; any other literal is `false`; a present
    /// non-literal object is `true`; absence is `false`.
    pub fn object_bool_of_term(&self, subject: &DslTerm, pred: &str) -> bool {
        match self.first_object_of(subject, pred) {
            Some(DslTerm::Literal {
                lexical_form: lexical,
                ..
            }) => {
                let v = lexical.trim().to_lowercase();
                v == "true" || v == "1"
            }
            Some(_) => true,
            None => false,
        }
    }

    /// The members of an `rdf:List` headed by `head` (empty if `head` is `None`),
    /// following `rdf:first`/`rdf:rest` to `rdf:nil`. A visited-set over ALL list
    /// nodes (IRI and blank alike) guards a cyclic `rdf:rest` chain so traversal
    /// terminates even on a malformed IRI cycle such as `<x> rdf:rest <x>`.
    pub fn rdf_list(&self, head: Option<&DslTerm>) -> Vec<DslTerm> {
        let mut out = Vec::new();
        let mut seen = std::collections::HashSet::new();
        let first = self.iri_id(RDF_FIRST);
        let rest = self.iri_id(RDF_REST);
        let nil = self.iri_id(RDF_NIL);
        let mut node = head.and_then(|head| self.subject_id(head));
        while let Some(current) = node {
            if Some(current) == nil || !seen.insert(current) {
                break;
            }
            if let Some(p) = first
                && let Some(row) =
                    default_graph_pattern(self.ds, Some(current), Some(p), None).next()
            {
                out.push(self.term_of(row.o));
            }
            node = rest
                .and_then(|p| default_graph_pattern(self.ds, Some(current), Some(p), None).next())
                .map(|q| q.o);
        }
        out
    }

    /// Stream default-graph reifier bindings in native table order. The selected
    /// alignment cells are sorted at their IR boundary; unrelated quoted statements
    /// never require owned records or a whole-source sorting/materialization pass.
    pub fn reified_statements(&self) -> impl Iterator<Item = ReifiedStatement<'a>> + '_ {
        self.iri_id(crate::graphutil::RDF_REIFIES)
            .into_iter()
            .flat_map(|predicate| default_graph_pattern(self.ds, None, Some(predicate), None))
            .map(|q| ReifiedStatement {
                dataset: self.ds,
                reifier: q.s,
                statement: q.o,
            })
    }
}

#[path = "dataset.tests.rs"]
#[cfg(test)]
mod tests;
