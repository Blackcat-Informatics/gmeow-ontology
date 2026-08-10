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

use purrdf::dataset_view::{DatasetView, GraphMatch};
use purrdf::ir::BlankScope;
use purrdf::{RdfDataset, RdfTerm, TermId, TermRef, TermValue};

const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
const RDF_FIRST: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#first";
const RDF_REST: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#rest";
const RDF_NIL: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#nil";

/// An owned RDF term in value space — the subset the alignment emitters read.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
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

/// One RDF-1.2 reified statement: a reifier node binding a base `(subject, predicate,
/// object)` triple. The value-space twin of an `owned_reifiers()` row the alignment
/// reader keys its native-form annotation reads off (`s p o {| … |}` in Turtle 1.2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReifiedStatement {
    /// The reifier handle naming this triple occurrence — the key the `annotation_*`
    /// accessors read the attached `(predicate, object)` pairs off.
    pub reifier: DslTerm,
    /// The reified triple's subject, in string form (an IRI, or a blank-node label).
    pub subject: String,
    /// The reified triple's predicate IRI.
    pub predicate: String,
    /// The reified triple's object term.
    pub object: DslTerm,
}

/// The IRI / blank-label key of a value-space reifier term (a literal can never be a
/// reifier, so it has no key). Keying on `I:`/`B:` string form makes annotation matching
/// robust to the blank-node scope the owned `RdfTerm::BlankNode(label)` does not carry.
fn reifier_key(term: &DslTerm) -> Option<String> {
    match term {
        DslTerm::Iri(iri) => Some(format!("I:{iri}")),
        DslTerm::Blank { label, .. } => Some(format!("B:{label}")),
        DslTerm::Literal { .. } => None,
    }
}

/// The reifier key of an owned [`RdfTerm`] (the counterpart to [`reifier_key`]).
fn reifier_key_rdf(term: &RdfTerm) -> Option<String> {
    match term {
        RdfTerm::Iri(iri) => Some(format!("I:{iri}")),
        RdfTerm::BlankNode(label) => Some(format!("B:{label}")),
        _ => None,
    }
}

/// Lower an owned [`RdfTerm`] into the value-space [`DslTerm`] the emitters read.
fn dslterm_of_rdf(term: &RdfTerm) -> DslTerm {
    match term {
        RdfTerm::Iri(iri) => DslTerm::Iri(iri.clone()),
        RdfTerm::BlankNode(label) => DslTerm::Blank {
            label: label.clone(),
            scope: BlankScope::DEFAULT,
        },
        RdfTerm::Literal(lit) => DslTerm::Literal {
            lexical: lit.lexical_form.clone(),
            datatype: lit.datatype.clone().unwrap_or_default(),
            language: lit.language.clone(),
        },
        // A triple-term subject/object is never read by the alignment DSL; surface it as
        // an empty blank so Iri/Literal matchers skip it (mirrors `term_of`).
        RdfTerm::Triple(_) => DslTerm::Blank {
            label: String::new(),
            scope: BlankScope::DEFAULT,
        },
    }
}

/// The subject/predicate string of an owned [`RdfTerm`] in reified-statement position
/// (an IRI or a blank-node label; a triple-term collapses to empty).
fn subject_string_of_rdf(term: &RdfTerm) -> String {
    match term {
        RdfTerm::Iri(iri) => iri.clone(),
        RdfTerm::BlankNode(label) => label.clone(),
        RdfTerm::Literal(lit) => lit.lexical_form.clone(),
        RdfTerm::Triple(_) => String::new(),
    }
}

/// The deterministic sort key of a value-space object term.
fn object_sort_key(term: &DslTerm) -> String {
    match term {
        DslTerm::Iri(iri) => iri.clone(),
        DslTerm::Blank { label, .. } => label.clone(),
        DslTerm::Literal { lexical, .. } => lexical.clone(),
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

    /// The lexical forms of ALL literal objects of `<subject_iri> <pred> ?o`, in dataset
    /// order — the multi-valued counterpart of [`Self::object_literal`].
    pub fn object_literals(&self, subject_iri: &str, pred: &str) -> Vec<String> {
        self.objects_of(subject_iri, pred)
            .into_iter()
            .filter_map(|t| match t {
                DslTerm::Literal { lexical, .. } => Some(lexical),
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

    /// Every IRI subject of `?s <pred> <object_iri>` in the default graph, in dataset
    /// order (the inverse-direction of [`Self::object_iris`]). Mirrors the historical
    /// oxigraph `subjects_iri(store, pred, object)` read the alignment lint used to walk
    /// `owl:inverseOf` / `schema:inverseOf` both ways.
    pub fn subjects_with_object_iri(&self, pred: &str, object_iri: &str) -> Vec<String> {
        let (Some(p), Some(obj)) = (self.iri_id(pred), self.iri_id(object_iri)) else {
            return Vec::new();
        };
        self.ds
            .quads_for_pattern(None, Some(p), Some(obj), GraphMatch::Default)
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

    /// The lexical form + datatype IRI of the first literal object of
    /// `<term> <pred> ?o`.
    pub fn literal_of_term(
        &self,
        subject: &DslTerm,
        pred: &str,
    ) -> Option<(String, Option<String>)> {
        match self.first_object_of(subject, pred) {
            Some(DslTerm::Literal {
                lexical, datatype, ..
            }) => Some((lexical, Some(datatype))),
            _ => None,
        }
    }

    /// Parse an RDF boolean object of `<term> <pred> ?o`: a literal `true`/`1` (case-
    /// and whitespace-insensitive) is `true`; any other literal is `false`; a present
    /// non-literal object is `true`; absence is `false`.
    pub fn object_bool_of_term(&self, subject: &DslTerm, pred: &str) -> bool {
        match self.first_object_of(subject, pred) {
            Some(DslTerm::Literal { lexical, .. }) => {
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
        let mut out: Vec<DslTerm> = Vec::new();
        let mut seen: std::collections::HashSet<DslTerm> = std::collections::HashSet::new();
        let mut node = head.cloned();
        while let Some(cur) = node {
            if let DslTerm::Iri(iri) = &cur
                && iri == RDF_NIL
            {
                break;
            }
            // Guard against a cyclic rest chain over any list node (IRI or blank):
            // break on the first back-edge to a node already visited.
            if !seen.insert(cur.clone()) {
                break;
            }
            if let Some(first) = self.first_object_of(&cur, RDF_FIRST) {
                out.push(first);
            }
            node = self.first_object_of(&cur, RDF_REST);
        }
        out
    }

    // ── RDF-1.2 reifier / annotation read surface ────────────────────────────────
    //
    // The RDF-1.2 asserting-annotation form `s p o {| pred obj ; … |}` records the base
    // triple in a SEPARATE reifier side-table (a reifier node bound to the `(s,p,o)`
    // triple) and each annotation as a `(reifier, pred, obj)` row — NOT as plain quads.
    // These accessors twin `object_iri` / `object_literal` / `subjects_of_type` but keyed
    // on a reifier, scoped (like the rest of `DslView`) to the DEFAULT graph.

    /// Every RDF-1.2 reified statement in the default graph, sorted deterministically by
    /// `(subject, predicate, object-string)` — the native-form alignment reader's input.
    pub fn reified_statements(&self) -> Vec<ReifiedStatement> {
        let mut out: Vec<ReifiedStatement> = self
            .ds
            .owned_reifiers()
            .filter(|r| r.graph.is_none())
            .map(|r| ReifiedStatement {
                reifier: dslterm_of_rdf(&r.reifier),
                subject: subject_string_of_rdf(&r.statement.subject),
                predicate: r.statement.predicate.clone(),
                object: dslterm_of_rdf(&r.statement.object),
            })
            .collect();
        out.sort_by(|a, b| {
            (&a.subject, &a.predicate, object_sort_key(&a.object)).cmp(&(
                &b.subject,
                &b.predicate,
                object_sort_key(&b.object),
            ))
        });
        out
    }

    /// The first IRI object of the annotation `<reifier> <pred> ?o`, or `None`.
    pub fn annotation_iri(&self, reifier: &DslTerm, pred: &str) -> Option<String> {
        let key = reifier_key(reifier)?;
        self.ds.owned_annotations().find_map(|a| {
            if a.graph.is_some()
                || a.predicate != pred
                || reifier_key_rdf(&a.reifier).as_deref() != Some(key.as_str())
            {
                return None;
            }
            match a.object {
                RdfTerm::Iri(iri) => Some(iri),
                _ => None,
            }
        })
    }

    /// The lexical form of the first literal object of the annotation `<reifier> <pred>
    /// ?o`, or `None`.
    pub fn annotation_literal(&self, reifier: &DslTerm, pred: &str) -> Option<String> {
        let key = reifier_key(reifier)?;
        self.ds.owned_annotations().find_map(|a| {
            if a.graph.is_some()
                || a.predicate != pred
                || reifier_key_rdf(&a.reifier).as_deref() != Some(key.as_str())
            {
                return None;
            }
            match a.object {
                RdfTerm::Literal(lit) => Some(lit.lexical_form),
                _ => None,
            }
        })
    }

    /// The lexical forms of ALL literal objects of the annotation `<reifier> <pred> ?o`,
    /// sorted — the multi-valued counterpart of [`Self::annotation_literal`].
    pub fn annotation_literals(&self, reifier: &DslTerm, pred: &str) -> Vec<String> {
        let Some(key) = reifier_key(reifier) else {
            return Vec::new();
        };
        let mut out: Vec<String> = self
            .ds
            .owned_annotations()
            .filter_map(|a| {
                if a.graph.is_some()
                    || a.predicate != pred
                    || reifier_key_rdf(&a.reifier).as_deref() != Some(key.as_str())
                {
                    return None;
                }
                match a.object {
                    RdfTerm::Literal(lit) => Some(lit.lexical_form),
                    _ => None,
                }
            })
            .collect();
        out.sort();
        out
    }

    /// Whether the reifier carries the annotation `<reifier> rdf:type <type_iri>` — the
    /// value-space check the grounding-flag read (`a logic:GroundingCorrespondence`) uses.
    pub fn annotation_has_type(&self, reifier: &DslTerm, type_iri: &str) -> bool {
        let Some(key) = reifier_key(reifier) else {
            return false;
        };
        self.ds.owned_annotations().any(|a| {
            a.graph.is_none()
                && a.predicate == RDF_TYPE
                && reifier_key_rdf(&a.reifier).as_deref() == Some(key.as_str())
                && matches!(&a.object, RdfTerm::Iri(iri) if iri == type_iri)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use purrdf::RdfDatasetBuilder;

    const EX: &str = "http://example.org/";

    /// Build a small default-graph dataset from `(s, p, o)` triples where each term is
    /// `i:<iri>`, `l:<lexical>` (a plain literal), or `b:<label>`.
    fn dataset(triples: &[(&str, &str, &str)]) -> std::sync::Arc<RdfDataset> {
        let mut b = RdfDatasetBuilder::new();
        let intern = |b: &mut RdfDatasetBuilder, t: &str| -> TermId {
            if let Some(rest) = t.strip_prefix("i:") {
                b.intern_iri(rest)
            } else if let Some(rest) = t.strip_prefix("b:") {
                b.intern_blank(rest, BlankScope::DEFAULT)
            } else if let Some(rest) = t.strip_prefix("l:") {
                b.intern_literal(purrdf::RdfLiteral::simple(rest.to_owned()))
            } else {
                panic!("bad test term {t}")
            }
        };
        for (s, p, o) in triples {
            let s = intern(&mut b, s);
            let pid = match p.strip_prefix("i:") {
                Some(rest) => b.intern_iri(rest),
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

    #[test]
    fn reified_statement_and_annotation_accessors_round_trip() {
        use purrdf::{RdfAnnotation, RdfLiteral, RdfReifier, RdfTerm, RdfTriple};

        let subj = format!("{EX}VirtualLocation");
        let pred = "http://www.w3.org/2004/02/skos/core#closeMatch";
        let obj = format!("{EX}Target");
        let reifier = RdfTerm::iri(format!("{EX}cell1"));
        let base = RdfTriple::new(RdfTerm::iri(&subj), pred, RdfTerm::iri(&obj));

        let mut b = RdfDatasetBuilder::new();
        b.push_owned_reifier(&RdfReifier::new(reifier.clone(), base));
        // An IRI annotation, two literal annotations under one predicate (multi), a typed
        // annotation (the grounding flag), and a numeric literal.
        b.push_owned_annotation(&RdfAnnotation::new(
            reifier.clone(),
            format!("{EX}justification"),
            RdfTerm::iri("https://w3id.org/semapv/vocab/ManualMappingCuration"),
        ));
        b.push_owned_annotation(&RdfAnnotation::new(
            reifier.clone(),
            format!("{EX}lossyDrop"),
            RdfTerm::literal(RdfLiteral::simple("beta")),
        ));
        b.push_owned_annotation(&RdfAnnotation::new(
            reifier.clone(),
            format!("{EX}lossyDrop"),
            RdfTerm::literal(RdfLiteral::simple("alpha")),
        ));
        b.push_owned_annotation(&RdfAnnotation::new(
            reifier.clone(),
            format!("{EX}confidence"),
            RdfTerm::literal(RdfLiteral::simple("0.9")),
        ));
        b.push_owned_annotation(&RdfAnnotation::new(
            reifier.clone(),
            RDF_TYPE,
            RdfTerm::iri(format!("{EX}GroundingCorrespondence")),
        ));
        let ds = b.freeze().expect("freeze rdf-1.2 dataset");
        let v = DslView::new(&ds);

        let stmts = v.reified_statements();
        assert_eq!(stmts.len(), 1);
        let stmt = &stmts[0];
        assert_eq!(stmt.reifier, DslTerm::Iri(format!("{EX}cell1")));
        assert_eq!(stmt.subject, subj);
        assert_eq!(stmt.predicate, pred);
        assert_eq!(stmt.object, DslTerm::Iri(obj.clone()));

        assert_eq!(
            v.annotation_iri(&stmt.reifier, &format!("{EX}justification")),
            Some("https://w3id.org/semapv/vocab/ManualMappingCuration".to_owned())
        );
        assert_eq!(
            v.annotation_literal(&stmt.reifier, &format!("{EX}confidence")),
            Some("0.9".to_owned())
        );
        assert_eq!(
            v.annotation_literals(&stmt.reifier, &format!("{EX}lossyDrop")),
            vec!["alpha".to_owned(), "beta".to_owned()]
        );
        assert!(v.annotation_has_type(&stmt.reifier, &format!("{EX}GroundingCorrespondence")));
        assert!(!v.annotation_has_type(&stmt.reifier, &format!("{EX}Nope")));
        // A predicate/reifier miss yields nothing.
        assert_eq!(
            v.annotation_iri(&stmt.reifier, &format!("{EX}missing")),
            None
        );
        assert_eq!(
            v.annotation_literal(
                &DslTerm::Iri(format!("{EX}other")),
                &format!("{EX}confidence")
            ),
            None
        );
    }
}
