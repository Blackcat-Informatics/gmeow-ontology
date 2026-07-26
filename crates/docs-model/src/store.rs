// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Oxigraph-free RDF query surface for the docs model + i18n extractors.
//!
//! The docs crate used to parse each slice's `module.ttl` / example / mapping
//! Turtle into an `oxigraph::store::Store` and pattern-match it. Every store/term
//! type is now native: parsing folds the RDF text into the frozen
//! [`purrdf::RdfDataset`] IR via the native codecs
//! ([`purrdf::parse_dataset`]), and pattern queries route through the IR's
//! indexed [`purrdf::DatasetView::quads_for_pattern`].
//!
//! The wrapper here is a *thin* twin of the slice crate's `rdf_query::Dataset`
//! (`crates/slice/src/rdf_query.rs`), specialised to the docs extractors' exact
//! query shapes:
//!
//! * **deterministic literal reads** — [`Store::first_literal`] returns the
//!   *lowest lexical form* (the old `min()` over literal objects), not merely the
//!   first; this matches the prior oxigraph code byte-for-byte.
//! * **blank-or-named subject reads** — reified changelog / competency rows are
//!   read off blank-node subjects ([`Store::first_literal_of`]).
//! * **whole-quad scans** — example term harvesting and shape-message walks scan
//!   every quad ([`Store::for_each_quad`], [`Store::pattern_subjects_objects`]).
//!
//! All committed byte output is keyed on IRI subjects/predicates/objects and
//! literal *values*; blank-node *labels* never reach a committed artifact (they
//! only thread shape/competency walks within one parse), so the native
//! per-source blank scoping is byte-transparent here.

use std::sync::Arc;

use purrdf::slice::SliceError;
use purrdf::{DatasetView, GraphMatch, RdfDataset, TermId, TermRef, TermValue, parse_dataset};

const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";

/// The kinds of RDF subject/object node a quad can carry, surfaced from the native
/// IR. Mirrors the `oxigraph::model::NamedOrBlankNode` discrimination the docs
/// extractors relied on.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub(crate) enum Node {
    /// An IRI node.
    Named(String),
    /// A blank node, by its (scope-qualified) label.
    Blank(String),
}

impl Node {
    /// The IRI, if this node is a named node.
    pub(crate) fn as_named(&self) -> Option<&str> {
        match self {
            Node::Named(iri) => Some(iri.as_str()),
            Node::Blank(_) => None,
        }
    }
}

/// An RDF object term, surfaced from the native IR as an owned value — the
/// oxigraph-free replacement for `oxigraph::model::Term` in object position.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub(crate) enum Object {
    /// An IRI object.
    Named(String),
    /// A blank-node object, by its (scope-qualified) label.
    Blank(String),
    /// A literal: its lexical form (the `.value()` of the old oxigraph literal).
    Literal(String),
    /// A quoted triple term (RDF 1.2); the docs extractors never inspect inside
    /// one — surfaced only so it is not silently mis-bucketed.
    Triple,
}

/// A frozen RDF dataset plus the IRI-pattern query surface the docs extractors
/// use. Wraps [`purrdf::RdfDataset`]; query helpers resolve a query IRI to a
/// dataset-local term id and pattern-scan via the indexed
/// [`purrdf::DatasetView::quads_for_pattern`].
#[derive(Debug)]
pub(crate) struct Store {
    ds: Arc<RdfDataset>,
}

impl Store {
    /// Parse Turtle `bytes` into a fresh store via the native codecs (lenient on
    /// GMEOW's `@x-gmeow-*` language tags). Errors on a syntax fault.
    pub(crate) fn parse_turtle(bytes: &[u8]) -> Result<Self, SliceError> {
        let ds = parse_dataset(bytes, "text/turtle", None)
            .map_err(|e| SliceError::Parse(format!("syntax error: {e}")))?;
        Ok(Self { ds })
    }

    /// The underlying parsed dataset, including its RDF-1.2 reifier/annotation side tables —
    /// needed by consumers that read native alignment cells through
    /// [`gmeow_logic_compile::ingest::DslView`] (the flat query helpers cannot see reifier
    /// annotations).
    pub(crate) fn dataset(&self) -> &RdfDataset {
        &self.ds
    }

    /// Parse N-Quads `bytes` into a fresh store via the native codecs. Unlike
    /// [`Store::parse_turtle`], the parsed quads may live in a *named* graph
    /// (the constraint-catalog fanout artifact carries every triple in the
    /// `gmeow:graph/fanout/catalog/…` named graph), so the catalog reader queries
    /// through the graph-agnostic [`Store::objects_any`] /
    /// [`Store::subjects_of_type_any`] helpers rather than the default-graph
    /// [`Store::objects`] family. Errors on a syntax fault.
    pub(crate) fn parse_nquads(bytes: &[u8]) -> Result<Self, SliceError> {
        let ds = parse_dataset(bytes, "application/n-quads", None)
            .map_err(|e| SliceError::Parse(format!("syntax error: {e}")))?;
        Ok(Self { ds })
    }

    /// Resolve an IRI to its dataset-local term id, if interned.
    fn iri_id(&self, iri: &str) -> Option<TermId> {
        self.ds.term_id_by_value(&TermValue::iri(iri))
    }

    /// Resolve a (named or blank) node to its dataset-local term id, if interned.
    fn node_id(&self, node: &Node) -> Option<TermId> {
        let value = match node {
            Node::Named(iri) => TermValue::iri(iri.clone()),
            Node::Blank(label) => TermValue::blank(label.clone()),
        };
        self.ds.term_id_by_value(&value)
    }

    /// Resolve a quad slot to a [`Node`] (named or blank); `None` for a literal /
    /// triple (which never stand in subject position in well-formed RDF).
    fn node_of(&self, id: TermId) -> Option<Node> {
        match self.ds.resolve(id) {
            TermRef::Iri(iri) => Some(Node::Named(iri.to_owned())),
            TermRef::Blank { label, scope } => {
                Some(Node::Blank(scope.qualify_label(label).into_owned()))
            }
            _ => None,
        }
    }

    /// Resolve a quad object slot to an owned [`Object`].
    fn object_of(&self, id: TermId) -> Object {
        match self.ds.resolve(id) {
            TermRef::Iri(iri) => Object::Named(iri.to_owned()),
            TermRef::Blank { label, scope } => {
                Object::Blank(scope.qualify_label(label).into_owned())
            }
            TermRef::Literal { lexical, .. } => Object::Literal(lexical.to_owned()),
            TermRef::Triple { .. } => Object::Triple,
        }
    }

    /// Every object term of `<subject> <pred> ?o` in the default graph, where the
    /// subject is named, in dataset order.
    pub(crate) fn objects(&self, subject_iri: &str, pred: &str) -> Vec<Object> {
        let (Some(s), Some(p)) = (self.iri_id(subject_iri), self.iri_id(pred)) else {
            return Vec::new();
        };
        self.ds
            .quads_for_pattern(Some(s), Some(p), None, GraphMatch::Default)
            .map(|q| self.object_of(q.o))
            .collect()
    }

    /// Every object term of `<node> <pred> ?o` in the default graph, where `node`
    /// may be a blank-node subject.
    pub(crate) fn objects_of_node(&self, subject: &Node, pred: &str) -> Vec<Object> {
        let (Some(s), Some(p)) = (self.node_id(subject), self.iri_id(pred)) else {
            return Vec::new();
        };
        self.ds
            .quads_for_pattern(Some(s), Some(p), None, GraphMatch::Default)
            .map(|q| self.object_of(q.o))
            .collect()
    }

    /// Every object term of `<subject> <pred> ?o` across *any* graph (default or
    /// named), where the subject is named, in dataset order. The graph-agnostic
    /// twin of [`Store::objects`] — used by the N-Quads constraint-catalog reader,
    /// whose quads live in a named fanout graph.
    pub(crate) fn objects_any(&self, subject_iri: &str, pred: &str) -> Vec<Object> {
        let (Some(s), Some(p)) = (self.iri_id(subject_iri), self.iri_id(pred)) else {
            return Vec::new();
        };
        self.ds
            .quads_for_pattern(Some(s), Some(p), None, GraphMatch::Any)
            .map(|q| self.object_of(q.o))
            .collect()
    }

    /// The lowest lexical literal value of `<subject> <pred> ?o` across *any*
    /// graph (deterministic), or `None`. Named-graph twin of
    /// [`Store::first_literal`].
    pub(crate) fn first_literal_any(&self, subject_iri: &str, pred: &str) -> Option<String> {
        self.objects_any(subject_iri, pred)
            .into_iter()
            .filter_map(|o| match o {
                Object::Literal(v) => Some(v),
                _ => None,
            })
            .min()
    }

    /// All named-node object IRIs of `<subject> <pred> ?o` across *any* graph,
    /// sorted + deduped. Named-graph twin of [`Store::named_objects`].
    pub(crate) fn named_objects_any(&self, subject_iri: &str, pred: &str) -> Vec<String> {
        let mut out: Vec<String> = self
            .objects_any(subject_iri, pred)
            .into_iter()
            .filter_map(|o| match o {
                Object::Named(iri) => Some(iri),
                _ => None,
            })
            .collect();
        out.sort();
        out.dedup();
        out
    }

    /// Every distinct named subject carrying `?s <pred> ?o` across *any* graph
    /// (sorted, deduped). The manifest reader uses it to enumerate the terms that
    /// carry a `gmeow:definitionDigest` in the named fanout graph.
    pub(crate) fn subjects_with_predicate_any(&self, pred: &str) -> Vec<String> {
        let Some(p) = self.iri_id(pred) else {
            return Vec::new();
        };
        let mut out: Vec<String> = self
            .ds
            .quads_for_pattern(None, Some(p), None, GraphMatch::Any)
            .filter_map(|q| match self.node_of(q.s) {
                Some(Node::Named(iri)) => Some(iri),
                _ => None,
            })
            .collect();
        out.sort();
        out.dedup();
        out
    }

    /// Every object term of `<node> <pred> ?o` across *any* graph, where `node` may
    /// be a blank-node subject (reified manifest changelog rows live in the named
    /// fanout graph). Graph-agnostic twin of [`Store::objects_of_node`].
    pub(crate) fn objects_of_node_any(&self, subject: &Node, pred: &str) -> Vec<Object> {
        let (Some(s), Some(p)) = (self.node_id(subject), self.iri_id(pred)) else {
            return Vec::new();
        };
        self.ds
            .quads_for_pattern(Some(s), Some(p), None, GraphMatch::Any)
            .map(|q| self.object_of(q.o))
            .collect()
    }

    /// The lowest lexical literal value of `<node> <pred> ?o` across *any* graph
    /// (deterministic), or `None`. Graph-agnostic twin of [`Store::first_literal_of`].
    pub(crate) fn first_literal_of_any(&self, subject: &Node, pred: &str) -> Option<String> {
        self.objects_of_node_any(subject, pred)
            .into_iter()
            .filter_map(|o| match o {
                Object::Literal(v) => Some(v),
                _ => None,
            })
            .min()
    }

    /// All NamedNode subjects of `?s a <type>` across *any* graph (sorted,
    /// deduped). Named-graph twin of [`Store::subjects_of_type`].
    pub(crate) fn subjects_of_type_any(&self, type_iri: &str) -> Vec<String> {
        let (Some(p), Some(o)) = (self.iri_id(RDF_TYPE), self.iri_id(type_iri)) else {
            return Vec::new();
        };
        let mut out: Vec<String> = self
            .ds
            .quads_for_pattern(None, Some(p), Some(o), GraphMatch::Any)
            .filter_map(|q| match self.node_of(q.s) {
                Some(Node::Named(iri)) => Some(iri),
                _ => None,
            })
            .collect();
        out.sort();
        out.dedup();
        out
    }

    /// The lowest lexical literal value of `<subject> <pred> ?o` (deterministic),
    /// or `None`. The oxigraph-free twin of the old `first_literal` (`min()`).
    pub(crate) fn first_literal(&self, subject_iri: &str, pred: &str) -> Option<String> {
        self.objects(subject_iri, pred)
            .into_iter()
            .filter_map(|o| match o {
                Object::Literal(v) => Some(v),
                _ => None,
            })
            .min()
    }

    /// The lowest lexical literal value of `<node> <pred> ?o` where `node` may be a
    /// blank-node subject (reified changelog / competency rows), or `None`.
    pub(crate) fn first_literal_of(&self, subject: &Node, pred: &str) -> Option<String> {
        self.objects_of_node(subject, pred)
            .into_iter()
            .filter_map(|o| match o {
                Object::Literal(v) => Some(v),
                _ => None,
            })
            .min()
    }

    /// All literal values of `<subject> <pred> ?o`, sorted + deduped.
    pub(crate) fn literals(&self, subject_iri: &str, pred: &str) -> Vec<String> {
        let mut out: Vec<String> = self
            .objects(subject_iri, pred)
            .into_iter()
            .filter_map(|o| match o {
                Object::Literal(v) => Some(v),
                _ => None,
            })
            .collect();
        out.sort();
        out.dedup();
        out
    }

    /// All literal values of `<subject> <pred> ?o`, sorted + deduped (alias of
    /// [`Store::literals`] for the guides extractors, which named it
    /// `sorted_literals`).
    pub(crate) fn sorted_literals(&self, subject_iri: &str, pred: &str) -> Vec<String> {
        self.literals(subject_iri, pred)
    }

    /// All literal values of `<subject> <pred> ?o` across *any* graph, sorted +
    /// deduped (deterministic). Named-graph twin of [`Store::literals`] — the
    /// advice-catalog reader uses it to collect the multi-valued advice prose
    /// carried in the catalog fanout named graph.
    pub(crate) fn literals_any(&self, subject_iri: &str, pred: &str) -> Vec<String> {
        let mut out: Vec<String> = self
            .objects_any(subject_iri, pred)
            .into_iter()
            .filter_map(|o| match o {
                Object::Literal(v) => Some(v),
                _ => None,
            })
            .collect();
        out.sort();
        out.dedup();
        out
    }

    /// All named-node object IRIs of `<subject> <pred> ?o`, in dataset order
    /// (un-sorted, matching the old `named_objects`).
    pub(crate) fn named_objects(&self, subject_iri: &str, pred: &str) -> Vec<String> {
        self.objects(subject_iri, pred)
            .into_iter()
            .filter_map(|o| match o {
                Object::Named(iri) => Some(iri),
                _ => None,
            })
            .collect()
    }

    /// All blank-node object labels of `<subject> <pred> ?o`, in dataset order.
    pub(crate) fn blank_objects(&self, subject_iri: &str, pred: &str) -> Vec<String> {
        self.objects(subject_iri, pred)
            .into_iter()
            .filter_map(|o| match o {
                Object::Blank(label) => Some(label),
                _ => None,
            })
            .collect()
    }

    /// Every distinct named subject carrying `?s <pred> ?o` in the default graph
    /// (sorted, deduped). Default-graph twin of [`Store::subjects_with_predicate_any`]
    /// — the projection-loss-ledger extractor uses it to find every example
    /// subject declaring `logic:preservationKind`.
    pub(crate) fn subjects_with_predicate(&self, pred: &str) -> Vec<String> {
        let Some(p) = self.iri_id(pred) else {
            return Vec::new();
        };
        let mut out: Vec<String> = self
            .ds
            .quads_for_pattern(None, Some(p), None, GraphMatch::Default)
            .filter_map(|q| match self.node_of(q.s) {
                Some(Node::Named(iri)) => Some(iri),
                _ => None,
            })
            .collect();
        out.sort();
        out.dedup();
        out
    }

    /// All NamedNode subjects of `?s a <type>` in the default graph (sorted,
    /// deduped) — the docs `subjects_of_type`.
    pub(crate) fn subjects_of_type(&self, type_iri: &str) -> Vec<String> {
        let (Some(p), Some(o)) = (self.iri_id(RDF_TYPE), self.iri_id(type_iri)) else {
            return Vec::new();
        };
        let mut out: Vec<String> = self
            .ds
            .quads_for_pattern(None, Some(p), Some(o), GraphMatch::Default)
            .filter_map(|q| match self.node_of(q.s) {
                Some(Node::Named(iri)) => Some(iri),
                _ => None,
            })
            .collect();
        out.sort();
        out.dedup();
        out
    }

    /// Every `(subject-node, object)` pair of `?s <pred> ?o` in the default graph,
    /// in dataset order. Used by the shape / formalizes / concerns scans that need
    /// both named AND blank subjects (and, for shapes, named objects).
    pub(crate) fn pattern_subjects_objects(&self, pred: &str) -> Vec<(Node, Object)> {
        let Some(p) = self.iri_id(pred) else {
            return Vec::new();
        };
        self.ds
            .quads_for_pattern(None, Some(p), None, GraphMatch::Default)
            .filter_map(|q| self.node_of(q.s).map(|s| (s, self.object_of(q.o))))
            .collect()
    }

    /// Every object term of `<node> ?p ?o` in the default graph, as
    /// `(predicate-IRI, Object)` — the shape-message blank-node walk (follows any
    /// predicate off a blank/named subject).
    pub(crate) fn predicate_objects_of(&self, subject: &Node) -> Vec<(String, Object)> {
        let Some(s) = self.node_id(subject) else {
            return Vec::new();
        };
        self.ds
            .quads_for_pattern(Some(s), None, None, GraphMatch::Default)
            .filter_map(|q| {
                let TermRef::Iri(p) = self.ds.resolve(q.p) else {
                    return None;
                };
                Some((p.to_owned(), self.object_of(q.o)))
            })
            .collect()
    }

    /// Scan every quad as `(subject-node, predicate-IRI, Object)` in dataset order
    /// — the whole-graph example term harvest (`store.iter()` twin). Quads whose
    /// subject is a literal/triple (never well-formed) or whose predicate is not an
    /// IRI are skipped.
    pub(crate) fn for_each_quad(&self, mut f: impl FnMut(&Node, &str, &Object)) {
        for q in self.ds.quads() {
            let Some(s) = self.node_of(q.s) else {
                continue;
            };
            let TermRef::Iri(p) = self.ds.resolve(q.p) else {
                continue;
            };
            let p = p.to_owned();
            let o = self.object_of(q.o);
            f(&s, &p, &o);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_literal_returns_lowest_lexical_form() {
        let ttl = "@prefix ex: <https://example.org/> .\n\
                   ex:a ex:label \"zebra\" ;\n\
                       ex:label \"apple\" .\n";
        let store = Store::parse_turtle(ttl.as_bytes()).unwrap();
        // min() semantics: the lexically-lowest literal, NOT dataset order.
        assert_eq!(
            store.first_literal("https://example.org/a", "https://example.org/label"),
            Some("apple".to_owned())
        );
    }

    #[test]
    fn subjects_of_type_finds_named_subjects_sorted() {
        let ttl = "@prefix ex: <https://example.org/> .\n\
                   ex:b a ex:Thing .\n\
                   ex:a a ex:Thing .\n";
        let store = Store::parse_turtle(ttl.as_bytes()).unwrap();
        assert_eq!(
            store.subjects_of_type("https://example.org/Thing"),
            vec![
                "https://example.org/a".to_owned(),
                "https://example.org/b".to_owned()
            ]
        );
    }

    #[test]
    fn blank_subject_literal_read_works() {
        let ttl = "@prefix ex: <https://example.org/> .\n\
                   ex:a ex:has [ ex:version \"1.2\" ] .\n";
        let store = Store::parse_turtle(ttl.as_bytes()).unwrap();
        let blanks = store.blank_objects("https://example.org/a", "https://example.org/has");
        assert_eq!(blanks.len(), 1);
        let node = Node::Blank(blanks[0].clone());
        assert_eq!(
            store.first_literal_of(&node, "https://example.org/version"),
            Some("1.2".to_owned())
        );
    }
}
