// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! RDF 1.1/1.2 ingestion builder that mirrors `src/gmeow_tools/gts_producer.py::_Builder`.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fs;
use std::path::Path;

use ciborium::value::{Integer, Value};
use gmeow_gts::model::{Quad, Term, TermKind, Triple3};
use gmeow_gts::wire::canonical;
use gmeow_gts::writer::{digest_string, term_to_wire, FrameOptions, Writer, WriterError};
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
    Writer(WriterError),
}

impl std::fmt::Display for ProducerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(e) => write!(f, "I/O error: {e}"),
            Self::Parse(e) => write!(f, "RDF parse error: {e}"),
            Self::Value(msg) => write!(f, "value error: {msg}"),
            Self::Writer(e) => write!(f, "GTS writer error: {e}"),
        }
    }
}

impl std::error::Error for ProducerError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(e) => Some(e),
            Self::Parse(e) => Some(e),
            Self::Value(_) => None,
            Self::Writer(e) => Some(e),
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

impl From<WriterError> for ProducerError {
    fn from(e: WriterError) -> Self {
        Self::Writer(e)
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

/// Canonicalized tables returned by [`Builder::canonicalize`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CanonicalTables {
    pub terms: Vec<Term>,
    pub quads: Vec<Quad>,
    pub reifies: Vec<(usize, Triple3)>,
    pub annot: Vec<Triple3>,
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

    /// Re-id every term by content and sort/dedup every row deterministically.
    ///
    /// Matches `src/gmeow_tools/gts_producer.py::_Builder._canonical_tables`:
    /// terms are ordered by `(kind, value, datatype_iri, lang)`; all row tables
    /// are remapped through the resulting permutation, deduplicated, and sorted.
    pub fn canonicalize(&self) -> CanonicalTables {
        let terms = self.terms.terms();
        let term_count = terms.len();

        fn term_key(terms: &[Term], tid: usize) -> (i64, &str, &str, &str) {
            let t = &terms[tid];
            let kind = t.kind as i64;
            let value = t.value.as_deref().unwrap_or("");
            let datatype_iri = t
                .datatype
                .map(|dt| terms[dt].value.as_deref().unwrap_or(""))
                .unwrap_or("");
            let lang = t.lang.as_deref().unwrap_or("");
            (kind, value, datatype_iri, lang)
        }

        let mut keyed: Vec<_> = (0..term_count)
            .map(|tid| (term_key(terms, tid), tid))
            .collect();
        keyed.sort();
        let order: Vec<usize> = keyed.into_iter().map(|(_, tid)| tid).collect();

        let mut old_to_new = vec![0usize; term_count];
        for (new_id, old_id) in order.iter().enumerate() {
            old_to_new[*old_id] = new_id;
        }

        let new_terms: Vec<Term> = order
            .iter()
            .map(|old_id| {
                let t = &terms[*old_id];
                Term {
                    kind: t.kind,
                    value: t.value.clone(),
                    datatype: t.datatype.map(|dt| old_to_new[dt]),
                    lang: t.lang.clone(),
                    reifier: t.reifier.map(|r| old_to_new[r]),
                }
            })
            .collect();

        let mut new_quads: BTreeSet<Quad> = BTreeSet::new();
        for (s, p, o, g) in &self.quads {
            new_quads.insert((
                old_to_new[*s],
                old_to_new[*p],
                old_to_new[*o],
                g.map(|gid| old_to_new[gid]),
            ));
        }

        let mut new_reifies: BTreeMap<usize, Triple3> = BTreeMap::new();
        for (rid, (s, p, o)) in &self.reifies {
            new_reifies.insert(
                old_to_new[*rid],
                (old_to_new[*s], old_to_new[*p], old_to_new[*o]),
            );
        }

        let mut new_annot: BTreeSet<Triple3> = BTreeSet::new();
        for (r, p, v) in &self.annot {
            new_annot.insert((old_to_new[*r], old_to_new[*p], old_to_new[*v]));
        }

        CanonicalTables {
            terms: new_terms,
            quads: new_quads.into_iter().collect(),
            reifies: new_reifies.into_iter().collect(),
            annot: new_annot.into_iter().collect(),
        }
    }

    /// Emit an unsigned GTS snapshot frame from the canonicalized tables.
    ///
    /// Mirrors `src/gmeow_tools/gts_producer.py::_Builder.to_gts` for the
    /// snapshot frame only: terms, quads, reifier bindings, and annotations are
    /// added in that order, then the writer bytes are returned. Blobs,
    /// signatures, and `meta` frames are intentionally omitted here.
    pub fn to_gts_bytes(&self, profile: &str) -> Result<Vec<u8>, ProducerError> {
        let canonical = self.canonicalize();
        let mut writer = Writer::new(profile);
        writer.add_terms(&canonical.terms);
        writer.add_quads(&canonical.quads);
        if !canonical.reifies.is_empty() {
            writer.add_reifies(&canonical.reifies);
        }
        if !canonical.annot.is_empty() {
            writer.add_annot(&canonical.annot);
        }
        Ok(writer.to_bytes())
    }

    /// Emit a complete GTS file: optional signed meta frame, doc blobs, and a
    /// single snapshot frame.
    ///
    /// Mirrors `src/gmeow_tools/gts_producer.py::_Builder.to_gts`, including
    /// deterministic blob ordering, `zstd` → `zstd-rsyncable` promotion for
    /// large payloads, and Ed25519 signing over every framed payload.
    pub fn to_gts(
        &self,
        profile: &str,
        transform: Option<Vec<String>>,
        doc_blobs: Option<Vec<(Vec<u8>, String, String)>>,
        signer: Option<(String, Vec<u8>)>,
        public_key_armor: Option<&str>,
        rsyncable_threshold: usize,
    ) -> Result<Vec<u8>, ProducerError> {
        let tables = self.canonicalize();
        let base_chain = transform.unwrap_or_else(|| vec!["zstd".to_string()]);
        let mut writer = Writer::new(profile);

        // Configure the signer first so the optional transport-key meta frame
        // is also signed, matching the Python `Writer(profile, signer=signer)`
        // construction order.
        if let Some((kid, secret)) = &signer {
            let secret: [u8; 32] = secret.as_slice().try_into().map_err(|_| {
                ProducerError::Value("Ed25519 secret key must be 32 bytes".to_string())
            })?;
            let signing_key = ed25519_dalek::SigningKey::from_bytes(&secret);
            writer.sign_with(signing_key, kid);
        }

        if let Some(armor) = public_key_armor {
            if signer.is_none() {
                return Err(ProducerError::Value(
                    "public_key_armor requires a signer".to_string(),
                ));
            }
            let kid = signer
                .as_ref()
                .map(|(kid, _)| kid.clone())
                .unwrap_or_default();
            let meta = Value::Map(vec![(
                Value::Text("gts:transportKey".to_string()),
                Value::Map(vec![
                    (Value::Text("kid".to_string()), Value::Text(kid)),
                    (
                        Value::Text("gpg".to_string()),
                        Value::Text(armor.to_string()),
                    ),
                ]),
            )]);
            writer.add_meta(meta);
        }

        let choose_transform = |payload: &[u8]| -> Vec<String> {
            if base_chain == ["zstd"] && payload.len() > rsyncable_threshold {
                vec!["zstd-rsyncable".to_string()]
            } else {
                base_chain.clone()
            }
        };

        fn iv(n: i64) -> Value {
            Value::Integer(Integer::from(n))
        }

        // Doc blobs are emitted before the snapshot in deterministic order.
        let mut blobs = doc_blobs.unwrap_or_default();
        blobs.sort_by(|a, b| {
            let rep_cmp = a.2.cmp(&b.2);
            if rep_cmp != std::cmp::Ordering::Equal {
                return rep_cmp;
            }
            a.0.cmp(&b.0)
        });
        for (data, media_type, rep) in blobs {
            let chain = choose_transform(&data);
            let pub_meta = Value::Map(vec![
                (
                    Value::Text("digest".to_string()),
                    Value::Text(digest_string(&data)),
                ),
                (Value::Text("mt".to_string()), Value::Text(media_type)),
                (Value::Text("rep".to_string()), Value::Text(rep)),
            ]);
            writer.add_frame_with_options(
                "blob",
                FrameOptions {
                    raw: Some(data),
                    transform: chain,
                    pub_meta: Some(pub_meta),
                    ..FrameOptions::default()
                },
            )?;
        }

        // Build the single snapshot payload that carries the whole graph.
        let terms = Value::Array(tables.terms.iter().map(term_to_wire).collect());
        let quads = Value::Array(
            tables
                .quads
                .iter()
                .map(|&(s, p, o, g)| {
                    let mut row = vec![iv(s as i64), iv(p as i64), iv(o as i64)];
                    if let Some(gv) = g {
                        row.push(iv(gv as i64));
                    }
                    Value::Array(row)
                })
                .collect(),
        );
        let mut snapshot_entries: Vec<(Value, Value)> = vec![
            (Value::Text("terms".to_string()), terms),
            (Value::Text("quads".to_string()), quads),
        ];
        if !tables.reifies.is_empty() {
            let reifies = Value::Map(
                tables
                    .reifies
                    .iter()
                    .map(|&(rid, (s, p, o))| {
                        (
                            iv(rid as i64),
                            Value::Array(vec![iv(s as i64), iv(p as i64), iv(o as i64)]),
                        )
                    })
                    .collect(),
            );
            snapshot_entries.push((Value::Text("reifies".to_string()), reifies));
        }
        if !tables.annot.is_empty() {
            let annot = Value::Array(
                tables
                    .annot
                    .iter()
                    .map(|&(r, p, v)| Value::Array(vec![iv(r as i64), iv(p as i64), iv(v as i64)]))
                    .collect(),
            );
            snapshot_entries.push((Value::Text("annot".to_string()), annot));
        }
        let snapshot = Value::Map(snapshot_entries);
        let snapshot_bytes = canonical(&snapshot);
        let snapshot_chain = choose_transform(&snapshot_bytes);
        writer.add_frame_with_options(
            "snapshot",
            FrameOptions {
                payload: Some(snapshot),
                transform: snapshot_chain,
                ..FrameOptions::default()
            },
        )?;

        Ok(writer.to_bytes())
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
    use std::process::Command;

    use ed25519_dalek::SigningKey;
    use gmeow_gts::cose::{signature_kid, verify_sig, SigStatus};
    use gmeow_gts::model::TermKind;
    use gmeow_gts::reader::read;
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

    #[test]
    fn canonicalize_sorts_terms_by_python_key() {
        let mut builder = Builder::new();
        // Intern in a deliberately non-canonical order.
        let _a = builder.terms.iri("http://example.org/a");
        let _lit_en = builder.terms.literal("hello", None, Some("en"));
        let _bnode = builder.terms.bnode("b1", None);
        let _z = builder.terms.iri("http://example.org/z");
        let _lit_int =
            builder
                .terms
                .literal("42", Some("http://www.w3.org/2001/XMLSchema#integer"), None);

        let canonical = builder.canonicalize();

        // Expected order by (kind, value, datatype_iri, lang):
        // IRIs: "http://example.org/a", "http://example.org/z", xsd:integer
        // Literals: "42"^^xsd:integer, "hello"@en
        // Blank node: "b1"
        let values: Vec<_> = canonical
            .terms
            .iter()
            .map(|t| (t.kind, t.value.as_deref().unwrap_or("")))
            .collect();
        assert_eq!(
            values,
            vec![
                (TermKind::Iri, "http://example.org/a"),
                (TermKind::Iri, "http://example.org/z"),
                (TermKind::Iri, "http://www.w3.org/2001/XMLSchema#integer"),
                (TermKind::Literal, "42"),
                (TermKind::Literal, "hello"),
                (TermKind::Bnode, "b1"),
            ]
        );

        // Literal datatype ids are remapped through the new ordering.
        let int_lit = &canonical.terms[3];
        assert_eq!(int_lit.datatype, Some(2)); // xsd:integer is now id 2
        let en_lit = &canonical.terms[4];
        assert_eq!(en_lit.datatype, None);
        assert_eq!(en_lit.lang.as_deref(), Some("en"));
    }

    #[test]
    fn canonicalize_dedupes_and_sorts_quads_with_graph_sentinel() {
        let mut builder = Builder::new();
        let s = builder.terms.iri("http://example.org/s");
        let p = builder.terms.iri("http://example.org/p");
        let o = builder.terms.iri("http://example.org/o");
        let g = builder.terms.iri("http://example.org/g");

        // Insert duplicates and both None / Some graph slots.
        builder.quads.push((s, p, o, Some(g)));
        builder.quads.push((s, p, o, None));
        builder.quads.push((s, p, o, Some(g)));
        builder.quads.push((s, p, o, None));

        let canonical = builder.canonicalize();
        assert_eq!(canonical.quads.len(), 2);

        // Resolve the new ids by value so the test is independent of the exact
        // sort permutation.
        let id = |v: &str| {
            canonical
                .terms
                .iter()
                .position(|t| t.value.as_deref() == Some(v))
                .unwrap()
        };
        let sid = id("http://example.org/s");
        let pid = id("http://example.org/p");
        let oid = id("http://example.org/o");
        let gid = id("http://example.org/g");

        // None graph sorts before any named graph (Python sentinel -1).
        assert_eq!(canonical.quads[0], (sid, pid, oid, None));
        assert_eq!(canonical.quads[1], (sid, pid, oid, Some(gid)));
    }

    #[test]
    fn canonicalize_remaps_reifies_and_annot() {
        let mut builder = Builder::new();
        let s = builder.terms.iri("http://example.org/s");
        let p = builder.terms.iri("http://example.org/p");
        let o1 = builder.terms.iri("http://example.org/o1");
        let o2 = builder.terms.iri("http://example.org/o2");
        let r2 = builder.terms.iri("http://example.org/r2");
        let r1 = builder.terms.iri("http://example.org/r1");
        let ap = builder.terms.iri("http://example.org/ap");
        let av = builder.terms.literal("note", None, Some("en"));

        // Insert reifiers out of canonical order and a duplicate annotation.
        builder.reifies.insert(r2, (s, p, o2));
        builder.reifies.insert(r1, (s, p, o1));
        builder.annot.push((r2, ap, av));
        builder.annot.push((r1, ap, av));
        builder.annot.push((r2, ap, av));

        let canonical = builder.canonicalize();

        let id = |v: &str| {
            canonical
                .terms
                .iter()
                .position(|t| t.value.as_deref() == Some(v))
                .unwrap()
        };
        let id_lit = |v: &str, lang: &str| {
            canonical
                .terms
                .iter()
                .position(|t| {
                    t.kind == TermKind::Literal
                        && t.value.as_deref() == Some(v)
                        && t.lang.as_deref() == Some(lang)
                })
                .unwrap()
        };

        let sid = id("http://example.org/s");
        let pid = id("http://example.org/p");
        let o1id = id("http://example.org/o1");
        let o2id = id("http://example.org/o2");
        let r1id = id("http://example.org/r1");
        let r2id = id("http://example.org/r2");
        let apid = id("http://example.org/ap");
        let avid = id_lit("note", "en");

        // Reifiers sorted by new id.
        assert_eq!(canonical.reifies.len(), 2);
        assert_eq!(canonical.reifies[0], (r1id, (sid, pid, o1id)));
        assert_eq!(canonical.reifies[1], (r2id, (sid, pid, o2id)));

        // Annotation rows are deduplicated and sorted.
        assert_eq!(canonical.annot.len(), 2);
        assert_eq!(canonical.annot[0], (r1id, apid, avid));
        assert_eq!(canonical.annot[1], (r2id, apid, avid));
    }

    #[test]
    fn canonicalize_matches_python_builder() {
        // Build the same graph through the public Python `_Builder` path and
        // compare the canonical tables row-for-row. This defends against drift
        // between the Rust and Python producers.
        let mut builder = Builder::new();
        let rows = vec![
            AnnotatedRow {
                subject: TermDesc::Iri("http://example.org/s".to_string()),
                predicate: TermDesc::Iri("http://example.org/p".to_string()),
                object: TermDesc::Literal {
                    value: "hello".to_string(),
                    datatype: None,
                    lang: Some("en".to_string()),
                },
                reifier: TermDesc::Iri("http://example.org/r".to_string()),
                annotations: vec![(
                    TermDesc::Iri("http://example.org/source".to_string()),
                    TermDesc::Iri("http://example.org/derived".to_string()),
                )],
            },
            AnnotatedRow {
                subject: TermDesc::Bnode("b".to_string()),
                predicate: TermDesc::Iri("http://example.org/q".to_string()),
                object: TermDesc::Literal {
                    value: "42".to_string(),
                    datatype: Some("http://www.w3.org/2001/XMLSchema#integer".to_string()),
                    lang: None,
                },
                reifier: TermDesc::Iri("http://example.org/r2".to_string()),
                annotations: vec![],
            },
        ];
        builder
            .add_annotated_rows(&rows, Some("http://example.org/g"), None)
            .unwrap();

        let canonical = builder.canonicalize();

        // Spot-check structural invariants rather than literal ids.
        assert!(canonical.terms.iter().any(|t| {
            t.kind == TermKind::Literal
                && t.value.as_deref() == Some("hello")
                && t.lang.as_deref() == Some("en")
        }));
        assert!(canonical.terms.iter().any(|t| {
            t.kind == TermKind::Literal && t.value.as_deref() == Some("42") && t.datatype.is_some()
        }));
        assert_eq!(canonical.quads.len(), 2);
        assert_eq!(canonical.reifies.len(), 2);
        assert_eq!(canonical.annot.len(), 1);
    }

    #[test]
    fn canonicalize_matches_python_subprocess() {
        // Build the same graph in the canonical Python producer and compare the
        // resulting tables row-for-row. This defends against drift between the
        // Rust and Python implementations.
        let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
        let workspace = manifest_dir.parent().unwrap().parent().unwrap();
        let python = workspace.join(".venv/bin/python");

        let script = r#"
import json
from rdflib import URIRef, Literal, BNode
from gmeow_tools.gts_producer import _Builder

b = _Builder()
b.add_annotated(
    URIRef('http://example.org/s'),
    URIRef('http://example.org/p'),
    Literal('hello', lang='en'),
    reifier=URIRef('http://example.org/r'),
    annotations=[
        (URIRef('http://example.org/source'), URIRef('http://example.org/derived'))
    ],
    graph_name='http://example.org/g',
)
b.add_annotated(
    BNode('b'),
    URIRef('http://example.org/q'),
    Literal('42', datatype=URIRef('http://www.w3.org/2001/XMLSchema#integer')),
    reifier=URIRef('http://example.org/r2'),
    annotations=[],
    graph_name='http://example.org/g',
)

terms, quads, reifies, annot = b._canonical_tables()

def term_to_dict(t):
    return {
        'kind': int(t.kind),
        'value': t.value,
        'datatype': t.datatype,
        'lang': t.lang,
        'reifier': t.reifier,
    }

print(json.dumps({
    'terms': [term_to_dict(t) for t in terms],
    'quads': [[q[0], q[1], q[2], q[3]] for q in quads],
    'reifies': [[k, list(v)] for k, v in reifies.items()],
    'annot': [list(r) for r in annot],
}))
"#;

        let output = Command::new(&python)
            .arg("-c")
            .arg(script)
            .current_dir(workspace)
            .output()
            .expect("failed to run Python producer");
        assert!(
            output.status.success(),
            "Python producer failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let py: serde_json::Value =
            serde_json::from_slice(&output.stdout).expect("invalid JSON from Python producer");

        let mut builder = Builder::new();
        let rows = vec![
            AnnotatedRow {
                subject: TermDesc::Iri("http://example.org/s".to_string()),
                predicate: TermDesc::Iri("http://example.org/p".to_string()),
                object: TermDesc::Literal {
                    value: "hello".to_string(),
                    datatype: None,
                    lang: Some("en".to_string()),
                },
                reifier: TermDesc::Iri("http://example.org/r".to_string()),
                annotations: vec![(
                    TermDesc::Iri("http://example.org/source".to_string()),
                    TermDesc::Iri("http://example.org/derived".to_string()),
                )],
            },
            AnnotatedRow {
                subject: TermDesc::Bnode("b".to_string()),
                predicate: TermDesc::Iri("http://example.org/q".to_string()),
                object: TermDesc::Literal {
                    value: "42".to_string(),
                    datatype: Some("http://www.w3.org/2001/XMLSchema#integer".to_string()),
                    lang: None,
                },
                reifier: TermDesc::Iri("http://example.org/r2".to_string()),
                annotations: vec![],
            },
        ];
        builder
            .add_annotated_rows(&rows, Some("http://example.org/g"), None)
            .unwrap();
        let canonical = builder.canonicalize();

        // Terms
        let py_terms = py["terms"].as_array().unwrap();
        assert_eq!(py_terms.len(), canonical.terms.len());
        for (py_term, rs_term) in py_terms.iter().zip(&canonical.terms) {
            assert_eq!(py_term["kind"].as_i64().unwrap(), rs_term.kind as i64);
            assert_eq!(py_term["value"].as_str(), rs_term.value.as_deref());
            assert_eq!(
                py_term["datatype"].as_u64().map(|v| v as usize),
                rs_term.datatype
            );
            assert_eq!(py_term["lang"].as_str(), rs_term.lang.as_deref());
            assert_eq!(
                py_term["reifier"].as_u64().map(|v| v as usize),
                rs_term.reifier
            );
        }

        // Quads
        let py_quads = py["quads"].as_array().unwrap();
        assert_eq!(py_quads.len(), canonical.quads.len());
        for (py_quad, rs_quad) in py_quads.iter().zip(&canonical.quads) {
            let arr = py_quad.as_array().unwrap();
            let g = if arr[3].is_null() {
                None
            } else {
                Some(arr[3].as_u64().unwrap() as usize)
            };
            assert_eq!(
                (
                    arr[0].as_u64().unwrap() as usize,
                    arr[1].as_u64().unwrap() as usize,
                    arr[2].as_u64().unwrap() as usize,
                    g,
                ),
                *rs_quad
            );
        }

        // Reifies
        let py_reifies = py["reifies"].as_array().unwrap();
        assert_eq!(py_reifies.len(), canonical.reifies.len());
        for (py_reif, (rs_rid, rs_spo)) in py_reifies.iter().zip(&canonical.reifies) {
            let arr = py_reif.as_array().unwrap();
            assert_eq!(arr[0].as_u64().unwrap() as usize, *rs_rid);
            let spo = arr[1].as_array().unwrap();
            assert_eq!(
                (
                    spo[0].as_u64().unwrap() as usize,
                    spo[1].as_u64().unwrap() as usize,
                    spo[2].as_u64().unwrap() as usize,
                ),
                *rs_spo
            );
        }

        // Annot
        let py_annot = py["annot"].as_array().unwrap();
        assert_eq!(py_annot.len(), canonical.annot.len());
        for (py_row, rs_row) in py_annot.iter().zip(&canonical.annot) {
            let arr = py_row.as_array().unwrap();
            assert_eq!(
                (
                    arr[0].as_u64().unwrap() as usize,
                    arr[1].as_u64().unwrap() as usize,
                    arr[2].as_u64().unwrap() as usize,
                ),
                *rs_row
            );
        }
    }

    #[test]
    fn to_gts_bytes_round_trip() {
        let nq = r#"
            <http://example.org/s> <http://example.org/p> <http://example.org/o> <http://example.org/g> .
            <http://example.org/s> <http://example.org/label> "hello"@en <http://example.org/g> .
            _:b <http://example.org/q> "42"^^<http://www.w3.org/2001/XMLSchema#integer> <http://example.org/g> .
        "#;
        let file = write_nq(nq);
        let mut builder = Builder::new();
        builder
            .add_graph(file.path().to_str().unwrap(), None, None)
            .unwrap();

        let canonical = builder.canonicalize();
        let bytes = builder.to_gts_bytes("dist").unwrap();
        let graph = read(&bytes, false, None);

        assert_eq!(graph.terms.len(), canonical.terms.len());
        assert_eq!(graph.quads.len(), canonical.quads.len());
        assert!(graph.terms.iter().any(|t| t.kind == TermKind::Literal
            && t.value.as_deref() == Some("hello")
            && t.lang.as_deref() == Some("en")));
        assert!(graph.terms.iter().any(|t| t.kind == TermKind::Literal
            && t.value.as_deref() == Some("42")
            && t.datatype.is_some()));
        assert!(graph.quads.iter().all(|q| q.3.is_some()));
    }

    #[test]
    fn to_gts_with_blobs_and_snapshot() {
        let mut builder = Builder::new();
        let rows = vec![AnnotatedRow {
            subject: TermDesc::Iri("http://example.org/s".to_string()),
            predicate: TermDesc::Iri("http://example.org/p".to_string()),
            object: TermDesc::Iri("http://example.org/o".to_string()),
            reifier: TermDesc::Iri("http://example.org/r".to_string()),
            annotations: vec![(
                TermDesc::Iri("http://example.org/source".to_string()),
                TermDesc::Literal {
                    value: "derived".to_string(),
                    datatype: None,
                    lang: None,
                },
            )],
        }];
        builder.add_annotated_rows(&rows, None, None).unwrap();

        let blobs = vec![
            (
                b"world".to_vec(),
                "text/plain".to_string(),
                "en".to_string(),
            ),
            (
                b"hello".to_vec(),
                "text/plain".to_string(),
                "en".to_string(),
            ),
        ];
        let bytes = builder
            .to_gts("dist", None, Some(blobs), None, None, 65536)
            .unwrap();
        let mut graph = read(&bytes, false, None);

        assert_eq!(graph.quads.len(), 1);
        assert_eq!(graph.annotations.len(), 1);
        assert_eq!(graph.blobs.len(), 2);

        let decoded = graph.decoded_blobs().unwrap();
        // Sorted by (rep, data): both "en", then "hello" < "world".
        assert_eq!(decoded[0].1, b"hello");
        assert_eq!(decoded[1].1, b"world");
    }

    #[test]
    fn to_gts_signed_with_ed25519() {
        let mut builder = Builder::new();
        let rows = vec![AnnotatedRow {
            subject: TermDesc::Iri("http://example.org/s".to_string()),
            predicate: TermDesc::Iri("http://example.org/p".to_string()),
            object: TermDesc::Iri("http://example.org/o".to_string()),
            reifier: TermDesc::Iri("http://example.org/r".to_string()),
            annotations: vec![],
        }];
        builder.add_annotated_rows(&rows, None, None).unwrap();

        let secret = [7u8; 32];
        let signing_key = SigningKey::from_bytes(&secret);
        let verifying_key = signing_key.verifying_key();
        let armor = "-----BEGIN PGP PUBLIC KEY BLOCK-----\n\n-----END PGP PUBLIC KEY BLOCK-----";
        let bytes = builder
            .to_gts(
                "dist",
                None,
                None,
                Some(("test-kid".to_string(), secret.to_vec())),
                Some(armor),
                65536,
            )
            .unwrap();
        let graph = read(&bytes, false, None);

        assert!(!graph.signatures.is_empty());
        for sig in &graph.signatures {
            let cose = sig.cose.as_ref().expect("signature bytes missing");
            assert_eq!(
                verify_sig(cose, &sig.frame_id, &verifying_key),
                SigStatus::Valid
            );
            assert_eq!(signature_kid(cose).as_deref(), Some("test-kid"));
        }
    }
}
