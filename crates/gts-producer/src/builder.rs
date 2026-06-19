// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! RDF 1.1 ingestion builder that mirrors `src/gmeow_tools/gts_producer.py::_Builder`.

use std::fs;
use std::path::Path;

use gmeow_gts::model::{Quad, Term};
use oxigraph::io::{RdfFormat, RdfParser};
use oxigraph::model::{GraphName, NamedNode, NamedOrBlankNode, Term as OxTerm};

use crate::interner::Interner;

const XSD_STRING: &str = "http://www.w3.org/2001/XMLSchema#string";

/// Errors that can occur while ingesting RDF into a [`Builder`].
#[derive(Debug)]
pub enum ProducerError {
    Io(std::io::Error),
    Parse(oxigraph::io::RdfSyntaxError),
}

impl std::fmt::Display for ProducerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(e) => write!(f, "I/O error: {e}"),
            Self::Parse(e) => write!(f, "RDF parse error: {e}"),
        }
    }
}

impl std::error::Error for ProducerError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(e) => Some(e),
            Self::Parse(e) => Some(e),
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

/// Accumulates terms and quads from one or more RDF 1.1 sources.
#[derive(Clone, Debug, Default)]
pub struct Builder {
    terms: Interner,
    quads: Vec<Quad>,
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
}
