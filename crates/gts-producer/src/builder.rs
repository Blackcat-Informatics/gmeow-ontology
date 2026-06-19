// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! RDF 1.1/1.2 ingestion builder that mirrors `src/gmeow_tools/gts_producer.py::_Builder`.

use std::collections::HashMap;
use std::fs;
use std::path::Path;

use gmeow_gts::model::{Quad, Term, TermKind, Triple3};
use oxigraph::io::{RdfFormat, RdfParser};
use oxigraph::model::{GraphName, NamedNode, NamedOrBlankNode, Term as OxTerm};

use crate::interner::Interner;

const XSD_STRING: &str = "http://www.w3.org/2001/XMLSchema#string";
/// RDF 1.2 reifier binding predicate.
pub const RDF_REIFIES: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#reifies";

/// Errors that can occur while ingesting RDF into a [`Builder`].
#[derive(Debug)]
pub enum ProducerError {
    Io(std::io::Error),
    Parse(oxigraph::io::RdfSyntaxError),
    Value(String),
}

impl std::fmt::Display for ProducerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(e) => write!(f, "I/O error: {e}"),
            Self::Parse(e) => write!(f, "RDF parse error: {e}"),
            Self::Value(msg) => write!(f, "value error: {msg}"),
        }
    }
}

impl std::error::Error for ProducerError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(e) => Some(e),
            Self::Parse(e) => Some(e),
            Self::Value(_) => None,
        }
    }
}

impl From<std::io::Error> for ProducerError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}

impl From<oxigraph::io::RdfSyntaxError> for ProducerError {
    fn from(e: oxigraph::io::RdfSyntaxError) -> Self {
        Self::Parse(e)
    }
}

/// Description of a term for the [`Builder::add_annotated_rows`] path.
///
/// This is a language-agnostic representation that the PyO3 wrapper builds
/// from Python dicts (`{"kind": "iri"|"bnode"|"literal", ...}`).
#[derive(Clone, Debug)]
pub enum TermDesc {
    Iri(String),
    Bnode(String),
    Literal {
        value: String,
        datatype: Option<String>,
        lang: Option<String>,
    },
}

/// One annotated base triple plus its reifier and annotation triples.
#[derive(Clone, Debug)]
pub struct AnnotatedRow {
    pub subject: TermDesc,
    pub predicate: TermDesc,
    pub object: TermDesc,
    pub reifier: TermDesc,
    pub annotations: Vec<(TermDesc, TermDesc)>,
}

/// Accumulates terms, quads, reifier bindings, and annotation triples from
/// RDF 1.1 and RDF 1.2 sources.
#[derive(Clone, Debug, Default)]
pub struct Builder {
    terms: Interner,
    quads: Vec<Quad>,
    reifies: HashMap<usize, Triple3>,
    annot: Vec<Triple3>,
}

impl Builder {
    /// Create an empty builder.
    pub fn new() -> Self {
        Self::default()
    }

    /// Return the accumulated term table.
    pub fn terms(&self) -> &[Term] {
        self.terms.terms()
    }

    /// Return the accumulated quads.
    pub fn quads(&self) -> &[Quad] {
        &self.quads
    }

    /// Return the accumulated reifier bindings (reifier id → nested triple).
    pub fn reifies(&self) -> &HashMap<usize, Triple3> {
        &self.reifies
    }

    /// Return the accumulated annotation triples.
    pub fn annot(&self) -> &[Triple3] {
        &self.annot
    }

    /// Parse `path` with `oxigraph` and append its quads to this builder.
    ///
    /// The format is inferred from the file extension: `.ttl`/`.turtle` → Turtle,
    /// `.nq`/`.nquads` → N-Quads, default Turtle. `graph_name` assigns rows that
    /// carry no name of their own to a named graph. `bnode_scope` scopes blank
    /// nodes to the ingest source so that labels from different sources do not
    /// collapse.
    pub fn add_graph(
        &mut self,
        path: &str,
        graph_name: Option<&str>,
        bnode_scope: Option<&str>,
    ) -> Result<(), ProducerError> {
        let format = format_from_path(path);
        let data = fs::read(path)?;
        let default_gid = graph_name.map(|g| self.terms.iri(g));

        // We iterate the parser directly rather than using `Store::load_from_slice`,
        // because the store helper enables blank-node renaming by default. Preserving
        // the source labels is required so that `bnode_scope` can scope them
        // deterministically across multiple ingest calls.
        for quad in RdfParser::from_format(format).for_slice(&data) {
            let quad = quad?;
            let Some(sid) = self.subject_id(quad.subject, bnode_scope) else {
                continue;
            };
            let pid = self.named_node_id(quad.predicate);
            let Some(oid) = self.object_id(quad.object, bnode_scope) else {
                continue;
            };
            let gid = self.graph_name_id(quad.graph_name, bnode_scope, default_gid);
            self.quads.push((sid, pid, oid, gid));
        }
        Ok(())
    }

    /// Parse an RDF 1.2 artifact from `path` and append its statement layer.
    ///
    /// `rdf:reifies` bindings populate [`Self::reifies`]; the reifier's other
    /// triples populate [`Self::annot`]; remaining triples become base quads.
    /// `graph_name` assigns base triples with no explicit graph name to a named
    /// graph. `bnode_scope` scopes blank-node labels to the ingest source.
    pub fn add_rdf12(
        &mut self,
        path: &str,
        graph_name: Option<&str>,
        bnode_scope: Option<&str>,
    ) -> Result<(), ProducerError> {
        let format = format_from_path(path);
        let data = fs::read(path)?;
        let default_gid = graph_name.map(|g| self.terms.iri(g));

        let statements: Vec<_> = RdfParser::from_format(format)
            .for_slice(&data)
            .collect::<Result<Vec<_>, _>>()?;

        // Pass 1: collect rdf:reifies bindings and intern the nested triples.
        for quad in &statements {
            if quad.predicate.as_str() != RDF_REIFIES {
                continue;
            }
            let OxTerm::Triple(triple) = &quad.object else {
                continue;
            };
            let Some(rid) = self.subject_id(quad.subject.clone(), bnode_scope) else {
                continue;
            };
            let Some(qs) = self.subject_id(triple.subject.clone(), bnode_scope) else {
                continue;
            };
            let qp = self.named_node_id(triple.predicate.clone());
            let Some(qo) = self.object_id(triple.object.clone(), bnode_scope) else {
                continue;
            };
            if let Some(&existing) = self.reifies.get(&rid) {
                if existing != (qs, qp, qo) {
                    let term = &self.terms.terms()[rid];
                    let msg = format!(
                        "conflicting reifier rebind for {} ({:?})",
                        term.value.as_deref().unwrap_or(""),
                        term.kind
                    );
                    return Err(ProducerError::Value(msg));
                }
            }
            self.reifies.insert(rid, (qs, qp, qo));
        }

        // Pass 2: remaining triples become base quads or annotation rows.
        for quad in &statements {
            if quad.predicate.as_str() == RDF_REIFIES && matches!(quad.object, OxTerm::Triple(_)) {
                continue;
            }
            let Some(sid) = self.subject_id(quad.subject.clone(), bnode_scope) else {
                continue;
            };
            let pid = self.named_node_id(quad.predicate.clone());
            let Some(oid) = self.object_id(quad.object.clone(), bnode_scope) else {
                continue;
            };
            if self.reifies.contains_key(&sid) {
                self.annot.push((sid, pid, oid));
            } else {
                self.quads.push((sid, pid, oid, default_gid));
            }
        }

        Ok(())
    }

    /// Add annotated base triples from structured term descriptions.
    ///
    /// Each row asserts its base triple as a quad, binds the reifier to that
    /// triple, and records the annotation triples. This is the Rust-side
    /// equivalent of Python `_Builder.add_annotated`, exposed to Python as
    /// `add_annotated_rows` so callers can serialize rdflib terms to plain
    /// dicts.
    pub fn add_annotated_rows(
        &mut self,
        rows: &[AnnotatedRow],
        graph_name: Option<&str>,
        bnode_scope: Option<&str>,
    ) -> Result<(), ProducerError> {
        for row in rows {
            self.add_annotated_row(row, graph_name, bnode_scope)?;
        }
        Ok(())
    }

    fn add_annotated_row(
        &mut self,
        row: &AnnotatedRow,
        graph_name: Option<&str>,
        bnode_scope: Option<&str>,
    ) -> Result<(), ProducerError> {
        let sid = self.term_desc_id(&row.subject, bnode_scope)?;
        let pid = self.term_desc_id(&row.predicate, bnode_scope)?;
        if self.terms.terms()[pid].kind != TermKind::Iri {
            return Err(ProducerError::Value("predicate must be an IRI".to_string()));
        }
        let oid = self.term_desc_id(&row.object, bnode_scope)?;
        let gid = graph_name.map(|g| self.terms.iri(g));
        self.quads.push((sid, pid, oid, gid));

        let rid = self.term_desc_id(&row.reifier, bnode_scope)?;
        if let Some(&existing) = self.reifies.get(&rid) {
            if existing != (sid, pid, oid) {
                let term = &self.terms.terms()[rid];
                let msg = format!(
                    "conflicting reifier rebind for {} ({:?})",
                    term.value.as_deref().unwrap_or(""),
                    term.kind
                );
                return Err(ProducerError::Value(msg));
            }
        }
        self.reifies.insert(rid, (sid, pid, oid));

        for (ann_p, ann_o) in &row.annotations {
            let ap = self.term_desc_id(ann_p, bnode_scope)?;
            if self.terms.terms()[ap].kind != TermKind::Iri {
                return Err(ProducerError::Value(
                    "annotation predicate must be an IRI".to_string(),
                ));
            }
            let av = self.term_desc_id(ann_o, bnode_scope)?;
            self.annot.push((rid, ap, av));
        }

        Ok(())
    }

    fn term_desc_id(
        &mut self,
        term: &TermDesc,
        bnode_scope: Option<&str>,
    ) -> Result<usize, ProducerError> {
        match term {
            TermDesc::Iri(iri) => Ok(self.terms.iri(iri)),
            TermDesc::Bnode(label) => Ok(self.terms.bnode(label, bnode_scope)),
            TermDesc::Literal {
                value,
                datatype,
                lang,
            } => {
                let datatype = datatype.as_deref();
                let lang = lang.as_deref();
                // Match Python/_Builder and add_graph: language-tagged literals
                // carry no explicit datatype, and xsd:string is normalized away.
                let datatype = if lang.is_some() {
                    None
                } else {
                    datatype.filter(|dt| *dt != XSD_STRING)
                };
                Ok(self.terms.literal(value, datatype, lang))
            }
        }
    }

    fn named_node_id(&mut self, node: NamedNode) -> usize {
        self.terms.iri(node.as_str())
    }

    fn subject_id(
        &mut self,
        subject: NamedOrBlankNode,
        bnode_scope: Option<&str>,
    ) -> Option<usize> {
        match subject {
            NamedOrBlankNode::NamedNode(n) => Some(self.terms.iri(n.as_str())),
            NamedOrBlankNode::BlankNode(b) => Some(self.terms.bnode(b.as_str(), bnode_scope)),
        }
    }

    fn object_id(&mut self, object: OxTerm, bnode_scope: Option<&str>) -> Option<usize> {
        match object {
            OxTerm::NamedNode(n) => Some(self.terms.iri(n.as_str())),
            OxTerm::BlankNode(b) => Some(self.terms.bnode(b.as_str(), bnode_scope)),
            OxTerm::Literal(l) => {
                let (value, datatype, lang, _direction) = l.destruct();
                let datatype = datatype.and_then(|dt| {
                    let dt_iri = dt.into_string();
                    if dt_iri == XSD_STRING {
                        None
                    } else {
                        Some(dt_iri)
                    }
                });
                Some(
                    self.terms
                        .literal(&value, datatype.as_deref(), lang.as_deref()),
                )
            }
            OxTerm::Triple(_) => None,
        }
    }

    fn graph_name_id(
        &mut self,
        graph_name: GraphName,
        bnode_scope: Option<&str>,
        default_gid: Option<usize>,
    ) -> Option<usize> {
        match graph_name {
            GraphName::DefaultGraph => default_gid,
            GraphName::NamedNode(n) => Some(self.terms.iri(n.as_str())),
            GraphName::BlankNode(b) => Some(self.terms.bnode(b.as_str(), bnode_scope)),
        }
    }
}

fn format_from_path(path: &str) -> RdfFormat {
    Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .and_then(|ext| {
            let ext = ext.to_ascii_lowercase();
            match ext.as_str() {
                "ttl" | "turtle" => Some(RdfFormat::Turtle),
                "nq" | "nquads" => Some(RdfFormat::NQuads),
                _ => RdfFormat::from_extension(&ext),
            }
        })
        .unwrap_or(RdfFormat::Turtle)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    use gmeow_gts::model::TermKind;
    use tempfile::NamedTempFile;

    fn write_ttl(content: &str) -> NamedTempFile {
        let mut file = NamedTempFile::with_suffix(".ttl").unwrap();
        file.write_all(content.as_bytes()).unwrap();
        file
    }

    fn write_nq(content: &str) -> NamedTempFile {
        let mut file = NamedTempFile::with_suffix(".nq").unwrap();
        file.write_all(content.as_bytes()).unwrap();
        file
    }

    #[test]
    fn ingest_turtle_with_iri_bnode_and_literal() {
        let ttl = r#"
            @prefix ex: <http://example.org/> .
            ex:s ex:p ex:o .
            _:a ex:name "Alice" .
            _:a ex:age "30"^^<http://www.w3.org/2001/XMLSchema#integer> .
        "#;
        let file = write_ttl(ttl);
        let mut builder = Builder::new();
        builder
            .add_graph(file.path().to_str().unwrap(), None, None)
            .unwrap();

        // Terms: ex:s, ex:p, ex:o, _:a, ex:name, "Alice", ex:age, xsd:integer, "30"
        assert_eq!(builder.terms().len(), 9);
        assert_eq!(builder.quads().len(), 3);

        let quads = builder.quads();
        assert!(quads.iter().all(|q| q.3.is_none()));

        // Verify blank node label is preserved unscoped.
        let bnode_term = builder
            .terms()
            .iter()
            .find(|t| t.kind == TermKind::Bnode)
            .unwrap();
        assert_eq!(bnode_term.value.as_deref().unwrap(), "a");
    }

    #[test]
    fn ingest_nquads_named_graph() {
        let nq = r#"
            <http://example.org/s> <http://example.org/p> <http://example.org/o> <http://example.org/g> .
            <http://example.org/s> <http://example.org/p2> "hello"@en <http://example.org/g> .
        "#;
        let file = write_nq(nq);
        let mut builder = Builder::new();
        builder
            .add_graph(file.path().to_str().unwrap(), None, None)
            .unwrap();

        assert_eq!(builder.quads().len(), 2);
        assert!(builder.quads().iter().all(|q| q.3.is_some()));

        let graph_term = builder
            .terms()
            .iter()
            .find(|t| t.value.as_deref() == Some("http://example.org/g"))
            .unwrap();
        assert_eq!(graph_term.kind, TermKind::Iri);

        let lang_lit = builder
            .terms()
            .iter()
            .find(|t| t.lang.as_deref() == Some("en"))
            .unwrap();
        assert_eq!(lang_lit.datatype, None);
    }

    #[test]
    fn default_graph_name_parameter() {
        let ttl = "<http://example.org/s> <http://example.org/p> <http://example.org/o> .";
        let file = write_ttl(ttl);
        let mut builder = Builder::new();
        builder
            .add_graph(
                file.path().to_str().unwrap(),
                Some("http://example.org/graph"),
                None,
            )
            .unwrap();

        assert_eq!(builder.quads().len(), 1);
        let quad = builder.quads()[0];
        assert!(quad.3.is_some());
        let graph_term = &builder.terms()[quad.3.unwrap()];
        assert_eq!(
            graph_term.value.as_deref(),
            Some("http://example.org/graph")
        );
    }

    #[test]
    fn scoped_blank_nodes_do_not_collapse() {
        let ttl_a = "_:x <http://example.org/p> <http://example.org/o> .";
        let ttl_b = "_:x <http://example.org/p> <http://example.org/o> .";

        let file_a = write_ttl(ttl_a);
        let file_b = write_ttl(ttl_b);

        let mut builder = Builder::new();
        builder
            .add_graph(file_a.path().to_str().unwrap(), None, Some("scope-a"))
            .unwrap();
        builder
            .add_graph(file_b.path().to_str().unwrap(), None, Some("scope-b"))
            .unwrap();

        // Two distinct blank-node terms plus the shared IRI terms.
        let bnode_terms: Vec<_> = builder
            .terms()
            .iter()
            .filter(|t| t.kind == TermKind::Bnode)
            .collect();
        assert_eq!(bnode_terms.len(), 2);
        assert_ne!(bnode_terms[0].value, bnode_terms[1].value);

        // Two quads, each with a different subject id.
        assert_eq!(builder.quads().len(), 2);
        assert_ne!(builder.quads()[0].0, builder.quads()[1].0);
    }

    #[test]
    fn ingest_rdf12_reifier_and_annotations() {
        // Use RDF 1.2 triple-term syntax <<( s p o )>>, not RDF-star << s p o >>,
        // so that the reifier binds directly without introducing an intermediate
        // blank node.
        let ttl = r#"
            @prefix ex: <http://example.org/> .
            @prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .

            ex:s ex:p ex:o .
            ex:reifier rdf:reifies <<( ex:s ex:p ex:o )>> .
            ex:reifier ex:source "derived" .
            ex:reifier ex:confidence "0.9"^^<http://www.w3.org/2001/XMLSchema#decimal> .
        "#;
        let file = write_ttl(ttl);
        let mut builder = Builder::new();
        builder
            .add_rdf12(file.path().to_str().unwrap(), None, None)
            .unwrap();

        // Terms: ex:s, ex:p, ex:o, ex:reifier, ex:source, "derived",
        //        ex:confidence, xsd:decimal, "0.9". Note that rdf:reifies
        // itself is never interned because the reifies binding is consumed in
        // pass 1 and skipped in pass 2.
        assert_eq!(builder.terms().len(), 9);

        // One reifier binding.
        assert_eq!(builder.reifies().len(), 1);
        let (&rid, &(sid, pid, oid)) = builder.reifies().iter().next().unwrap();
        assert_eq!(
            builder.terms()[rid].value.as_deref(),
            Some("http://example.org/reifier")
        );
        assert_eq!(
            builder.terms()[sid].value.as_deref(),
            Some("http://example.org/s")
        );
        assert_eq!(
            builder.terms()[pid].value.as_deref(),
            Some("http://example.org/p")
        );
        assert_eq!(
            builder.terms()[oid].value.as_deref(),
            Some("http://example.org/o")
        );

        // The base triple is asserted as a quad.
        assert_eq!(builder.quads().len(), 1);
        assert_eq!(builder.quads()[0], (sid, pid, oid, None));

        // Two annotation rows attached to the reifier.
        assert_eq!(builder.annot().len(), 2);
        assert!(builder.annot().iter().all(|(r, _p, _o)| *r == rid));
    }

    #[test]
    fn ingest_rdf12_named_graph() {
        let ttl = r#"
            @prefix ex: <http://example.org/> .
            @prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .

            ex:s ex:p ex:o .
            ex:reifier rdf:reifies <<( ex:s ex:p ex:o )>> .
        "#;
        let file = write_ttl(ttl);
        let mut builder = Builder::new();
        builder
            .add_rdf12(
                file.path().to_str().unwrap(),
                Some("http://example.org/graph"),
                None,
            )
            .unwrap();

        assert_eq!(builder.quads().len(), 1);
        assert!(builder.quads()[0].3.is_some());
        let graph_term = &builder.terms()[builder.quads()[0].3.unwrap()];
        assert_eq!(
            graph_term.value.as_deref(),
            Some("http://example.org/graph")
        );
    }

    #[test]
    fn ingest_rdf12_conflicting_rebind_errors() {
        let ttl = r#"
            @prefix ex: <http://example.org/> .
            @prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .

            ex:reifier rdf:reifies <<( ex:s ex:p ex:o1 )>> .
            ex:reifier rdf:reifies <<( ex:s ex:p ex:o2 )>> .
        "#;
        let file = write_ttl(ttl);
        let mut builder = Builder::new();
        let err = builder
            .add_rdf12(file.path().to_str().unwrap(), None, None)
            .unwrap_err();
        assert!(matches!(err, ProducerError::Value(_)));
        let msg = err.to_string();
        assert!(msg.contains("conflicting reifier rebind"));
    }

    #[test]
    fn add_annotated_rows_binds_reifier_and_annotations() {
        let mut builder = Builder::new();
        let rows = vec![AnnotatedRow {
            subject: TermDesc::Iri("http://example.org/s".to_string()),
            predicate: TermDesc::Iri("http://example.org/p".to_string()),
            object: TermDesc::Iri("http://example.org/o".to_string()),
            reifier: TermDesc::Iri("http://example.org/reifier".to_string()),
            annotations: vec![
                (
                    TermDesc::Iri("http://example.org/source".to_string()),
                    TermDesc::Literal {
                        value: "derived".to_string(),
                        datatype: None,
                        lang: None,
                    },
                ),
                (
                    TermDesc::Iri("http://example.org/bnode-annot".to_string()),
                    TermDesc::Bnode("b1".to_string()),
                ),
            ],
        }];
        builder
            .add_annotated_rows(&rows, None, Some("scope"))
            .unwrap();

        assert_eq!(builder.quads().len(), 1);
        assert_eq!(builder.reifies().len(), 1);
        assert_eq!(builder.annot().len(), 2);

        let (&rid, &spo) = builder.reifies().iter().next().unwrap();
        assert_eq!(builder.quads()[0].0, spo.0);
        assert_eq!(builder.quads()[0].1, spo.1);
        assert_eq!(builder.quads()[0].2, spo.2);
        assert!(builder.annot().iter().all(|(r, _p, _o)| *r == rid));
    }

    #[test]
    fn add_annotated_rows_conflicting_rebind_errors() {
        let mut builder = Builder::new();
        let rows = vec![
            AnnotatedRow {
                subject: TermDesc::Iri("http://example.org/s".to_string()),
                predicate: TermDesc::Iri("http://example.org/p".to_string()),
                object: TermDesc::Iri("http://example.org/o1".to_string()),
                reifier: TermDesc::Iri("http://example.org/reifier".to_string()),
                annotations: vec![],
            },
            AnnotatedRow {
                subject: TermDesc::Iri("http://example.org/s".to_string()),
                predicate: TermDesc::Iri("http://example.org/p".to_string()),
                object: TermDesc::Iri("http://example.org/o2".to_string()),
                reifier: TermDesc::Iri("http://example.org/reifier".to_string()),
                annotations: vec![],
            },
        ];
        let err = builder.add_annotated_rows(&rows, None, None).unwrap_err();
        assert!(matches!(err, ProducerError::Value(_)));
    }
}
