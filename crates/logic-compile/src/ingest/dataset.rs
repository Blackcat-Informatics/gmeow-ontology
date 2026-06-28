// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! A wasm-clean, value-space read layer over a parsed RDF dataset.
//!
//! The alignment-DSL emitters historically queried an `oxigraph::store::Store` with
//! `quads_for_pattern` over `NamedNode`/`Term` values. [`DslView`] reproduces exactly
//! that access surface over the oxigraph-free [`DatasetView`] read trait
//! (interned-id space), so the correspondence lowerings ingest the DSL/ontology with
//! no oxigraph dependency and build for `wasm32`. The owned [`DslTerm`] value mirrors
//! the subset of `oxigraph::model::Term` the emitters actually read (IRIs, blank
//! nodes, and the lexical/datatype/language of literals).
//!
//! All queries are scoped to the **default graph** — the DSL/ontology sources are
//! loaded flat, exactly as the historical store reads did.

use gmeow_rdf::dataset_view::{DatasetView, GraphMatch};
use gmeow_rdf::ir::BlankScope;
use gmeow_rdf::{RdfDataset, TermId, TermRef, TermValue};

const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
const RDF_FIRST: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#first";
const RDF_REST: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#rest";
const RDF_NIL: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#nil";

/// An owned RDF term in value space — the subset the alignment emitters read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DslTerm {
    /// An IRI by its full string.
    Iri(String),
    /// A blank node, carrying the `(label, scope)` needed to re-query it as a subject.
    Blank { label: String, scope: BlankScope },
    /// A literal: lexical form, datatype IRI, optional language tag.
    Literal {
        lexical: String,
        datatype: String,
        language: Option<String>,
    },
}

impl DslTerm {
    /// The IRI string if this is an IRI term.
    pub fn as_iri(&self) -> Option<&str> {
        match self {
            DslTerm::Iri(iri) => Some(iri),
            _ => None,
        }
    }

    /// The lexical form if this is a literal (mirrors oxigraph `Literal::value`).
    pub fn as_literal(&self) -> Option<&str> {
        match self {
            DslTerm::Literal { lexical, .. } => Some(lexical),
            _ => None,
        }
    }

    /// Whether this term is a blank node.
    pub fn is_blank(&self) -> bool {
        matches!(self, DslTerm::Blank { .. })
    }
}

/// A value-space read view over a parsed [`RdfDataset`] (default graph only).
pub struct DslView<'a> {
    ds: &'a RdfDataset,
}

impl<'a> DslView<'a> {
    /// Wrap a parsed dataset.
    pub fn new(ds: &'a RdfDataset) -> Self {
        Self { ds }
    }

    /// The interned id of an IRI, or `None` if the dataset has no such term (an
    /// absent term yields no quads, exactly as a missing oxigraph node would).
    fn iri_id(&self, iri: &str) -> Option<TermId> {
        self.ds.term_id_by_value(&TermValue::Iri(iri.to_owned()))
    }

    /// The interned id of a (named or blank) subject term, or `None`.
    fn subject_id(&self, term: &DslTerm) -> Option<TermId> {
        match term {
            DslTerm::Iri(iri) => self.iri_id(iri),
            DslTerm::Blank { label, scope } => self.ds.term_id_by_value(&TermValue::Blank {
                label: label.clone(),
                scope: *scope,
            }),
            DslTerm::Literal { .. } => None,
        }
    }

    /// Materialize a resolved term id into an owned [`DslTerm`].
    fn term_of(&self, id: TermId) -> DslTerm {
        match self.ds.resolve(id) {
            TermRef::Iri(iri) => DslTerm::Iri(iri.to_owned()),
            TermRef::Blank { label, scope } => DslTerm::Blank {
                label: label.to_owned(),
                scope,
            },
            TermRef::Literal {
                lexical,
                datatype,
                language,
                ..
            } => {
                let dt = match self.ds.resolve(datatype) {
                    TermRef::Iri(iri) => iri.to_owned(),
                    _ => String::new(),
                };
                DslTerm::Literal {
                    lexical: lexical.to_owned(),
                    datatype: dt,
                    language: language.map(str::to_owned),
                }
            }
            // A triple-term object is never read by the alignment DSL; surface it as a
            // blank placeholder so callers that only match Iri/Literal skip it.
            TermRef::Triple { .. } => DslTerm::Blank {
                label: String::new(),
                scope: BlankScope::DEFAULT,
            },
        }
    }

    /// Every named-node subject of `?s a <type_iri>`, sorted by IRI for a
    /// deterministic, interning-order-independent iteration.
    pub fn subjects_of_type(&self, type_iri: &str) -> Vec<String> {
        let (Some(rdf_type), Some(class)) = (self.iri_id(RDF_TYPE), self.iri_id(type_iri)) else {
            return Vec::new();
        };
        let mut subjects: Vec<String> = self
            .ds
            .quads_for_pattern(None, Some(rdf_type), Some(class), GraphMatch::Default)
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
        self.first_object_of(&DslTerm::Iri(subject_iri.to_owned()), pred)
    }

    /// All object terms of `<subject_iri> <pred> ?o`, in dataset order.
    pub fn objects_of(&self, subject_iri: &str, pred: &str) -> Vec<DslTerm> {
        self.objects_of_term(&DslTerm::Iri(subject_iri.to_owned()), pred)
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
            Some(DslTerm::Literal { lexical, .. }) => Some(lexical),
            _ => None,
        }
    }

    /// The first object term of `<term> <pred> ?o` where `term` is a (named or blank)
    /// subject, or `None`.
    pub fn first_object_of(&self, subject: &DslTerm, pred: &str) -> Option<DslTerm> {
        let (Some(subj), Some(p)) = (self.subject_id(subject), self.iri_id(pred)) else {
            return None;
        };
        self.ds
            .quads_for_pattern(Some(subj), Some(p), None, GraphMatch::Default)
            .next()
            .map(|q| self.term_of(q.o))
    }

    /// All object terms of `<term> <pred> ?o`, in dataset order.
    pub fn objects_of_term(&self, subject: &DslTerm, pred: &str) -> Vec<DslTerm> {
        let (Some(subj), Some(p)) = (self.subject_id(subject), self.iri_id(pred)) else {
            return Vec::new();
        };
        self.ds
            .quads_for_pattern(Some(subj), Some(p), None, GraphMatch::Default)
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
            Some(DslTerm::Literal { lexical, .. }) => Some(lexical),
            _ => None,
        }
    }

    /// Every `(subject, object)` pair of `?s <pred> ?o` in the default graph, in
    /// dataset order.
    pub fn quads_with_predicate(&self, pred: &str) -> Vec<(DslTerm, DslTerm)> {
        let Some(p) = self.iri_id(pred) else {
            return Vec::new();
        };
        self.ds
            .quads_for_pattern(None, Some(p), None, GraphMatch::Default)
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

    /// The members of an `rdf:List` headed by `head` (empty if `head` is `None`),
    /// following `rdf:first`/`rdf:rest` to `rdf:nil`.
    pub fn rdf_list(&self, head: Option<&DslTerm>) -> Vec<DslTerm> {
        let mut out: Vec<DslTerm> = Vec::new();
        let mut node = head.cloned();
        while let Some(cur) = node {
            if let DslTerm::Iri(iri) = &cur {
                if iri == RDF_NIL {
                    break;
                }
            }
            if let Some(first) = self.first_object_of(&cur, RDF_FIRST) {
                out.push(first);
            }
            node = self.first_object_of(&cur, RDF_REST);
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gmeow_rdf::RdfDatasetBuilder;

    const EX: &str = "http://example.org/";

    /// Build a small default-graph dataset from `(s, p, o)` triples where each term is
    /// `i:<iri>`, `l:<lexical>` (a plain literal), or `b:<label>`.
    fn dataset(triples: &[(&str, &str, &str)]) -> std::sync::Arc<RdfDataset> {
        let mut b = RdfDatasetBuilder::new();
        let intern = |b: &mut RdfDatasetBuilder, t: &str| -> TermId {
            if let Some(rest) = t.strip_prefix("i:") {
                b.intern_iri(rest.to_owned())
            } else if let Some(rest) = t.strip_prefix("b:") {
                b.intern_blank(rest.to_owned(), BlankScope::DEFAULT)
            } else if let Some(rest) = t.strip_prefix("l:") {
                b.intern_literal(gmeow_rdf::RdfLiteral::simple(rest.to_owned()))
            } else {
                panic!("bad test term {t}")
            }
        };
        for (s, p, o) in triples {
            let s = intern(&mut b, s);
            let pid = match p.strip_prefix("i:") {
                Some(rest) => b.intern_iri(rest.to_owned()),
                None => panic!("predicate must be i:<iri>"),
            };
            let o = intern(&mut b, o);
            b.push_quad(s, pid, o, None);
        }
        b.freeze().expect("freeze")
    }

    #[test]
    fn subjects_of_type_sorted_and_deduped() {
        let t = format!("i:{RDF_TYPE}");
        let cls = format!("i:{EX}Cls");
        let ds = dataset(&[
            (&format!("i:{EX}b"), &t, &cls),
            (&format!("i:{EX}a"), &t, &cls),
            (&format!("i:{EX}a"), &t, &cls),
        ]);
        let v = DslView::new(&ds);
        assert_eq!(
            v.subjects_of_type(&format!("{EX}Cls")),
            vec![format!("{EX}a"), format!("{EX}b")]
        );
    }

    #[test]
    fn object_accessors() {
        let ds = dataset(&[
            (
                &format!("i:{EX}s"),
                &format!("i:{EX}p"),
                &format!("i:{EX}o"),
            ),
            (&format!("i:{EX}s"), &format!("i:{EX}lit"), "l:hello"),
        ]);
        let v = DslView::new(&ds);
        assert_eq!(
            v.object_iri(&format!("{EX}s"), &format!("{EX}p")),
            Some(format!("{EX}o"))
        );
        assert_eq!(
            v.object_literal(&format!("{EX}s"), &format!("{EX}lit")),
            Some("hello".to_owned())
        );
        assert_eq!(
            v.object_iri(&format!("{EX}s"), &format!("{EX}missing")),
            None
        );
    }

    #[test]
    fn rdf_list_in_order_through_blanks() {
        let first = format!("i:{RDF_FIRST}");
        let rest = format!("i:{RDF_REST}");
        let nil = format!("i:{RDF_NIL}");
        // ( ex:x ex:y ) as _:l1 -> _:l2 -> nil
        let ds = dataset(&[
            ("b:l1", &first, &format!("i:{EX}x")),
            ("b:l1", &rest, "b:l2"),
            ("b:l2", &first, &format!("i:{EX}y")),
            ("b:l2", &rest, &nil),
        ]);
        let v = DslView::new(&ds);
        let head = DslTerm::Blank {
            label: "l1".to_owned(),
            scope: BlankScope::DEFAULT,
        };
        let items: Vec<String> = v
            .rdf_list(Some(&head))
            .into_iter()
            .filter_map(|t| t.as_iri().map(str::to_owned))
            .collect();
        assert_eq!(items, vec![format!("{EX}x"), format!("{EX}y")]);
    }
}
