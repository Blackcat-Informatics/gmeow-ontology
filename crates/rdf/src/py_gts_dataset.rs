// SPDX-FileCopyrightText: 2026 Blackcat Informatics Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! `PyRdfDataset` — a Python handle to a frozen, immutable [`crate::RdfDataset`]
//! (#819 C7 foundation).
//!
//! This pyclass wraps an `Arc<RdfDataset>` so a parsed RDF artifact can cross the
//! FFI boundary ONCE (as bytes), be frozen into the validated IR, and then be
//! consumed natively (count, GTS emission) WITHOUT re-serializing back to text.
//! A later commit (#819 C7) migrates the text-exchange call sites onto this
//! handle so the producer/consumer seam stops round-tripping through N-Quads.
//!
//! Construction parses Turtle/N-Quads/TriG natively (the same lenient oxigraph
//! parser the rest of the kernel uses), extracts the RDF 1.2 statement layer
//! (`rdf:reifies` triple-terms → reifier bindings; a reifier's other triples →
//! annotations), and freezes the result through [`crate::RdfDatasetBuilder`].

use std::sync::Arc;

use oxigraph::io::RdfFormat;
use oxigraph::model::{GraphName, NamedOrBlankNode, Term as OxTerm};
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyString};

use crate::py_store::{parse_quads, PyRdfFormat};
use crate::{gts_write, BlankScope, RdfDataset, RdfDatasetBuilder, RdfLiteral, TermId};

const RDF_REIFIES: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#reifies";

/// A Python handle to a frozen [`RdfDataset`].
#[pyclass(name = "RdfDataset", frozen)]
pub struct PyRdfDataset {
    inner: Arc<RdfDataset>,
}

#[pymethods]
impl PyRdfDataset {
    /// Build a frozen dataset by parsing RDF `data` (bytes or str) in `format`.
    #[new]
    #[pyo3(signature = (data, format))]
    fn new(data: &Bound<'_, PyAny>, format: PyRdfFormat) -> PyResult<Self> {
        let bytes = read_bytes(data)?;
        let inner = build_dataset(&bytes, rdf_format(format)).map_err(PyValueError::new_err)?;
        Ok(Self { inner })
    }

    /// The number of deduplicated quads.
    fn quad_count(&self) -> usize {
        self.inner.quad_count()
    }

    /// The number of distinct interned terms.
    fn term_count(&self) -> usize {
        self.inner.term_count()
    }

    fn __len__(&self) -> usize {
        self.inner.quad_count()
    }

    /// Emit a GTS byte stream for this dataset under `profile`. Uses the
    /// [`gts_write`] encoder (separate `terms`/`quads`/`reifies`/`annot` frames via
    /// `Writer::deterministic`); the folded graph is semantically identical to the
    /// snapshot-frame producer (the SEMANTIC-FOLD gate, #819 STEP 1).
    #[pyo3(signature = (profile="dist"))]
    fn to_gts(&self, py: Python<'_>, profile: &str) -> PyResult<Py<PyBytes>> {
        let store = self.inner.as_ref();
        let bytes =
            gts_write::to_gts(&store, profile).map_err(|e| PyValueError::new_err(e.to_string()))?;
        Ok(PyBytes::new(py, &bytes).unbind())
    }
}

fn read_bytes(data: &Bound<'_, PyAny>) -> PyResult<Vec<u8>> {
    if let Ok(bytes) = data.cast::<PyBytes>() {
        return Ok(bytes.as_bytes().to_vec());
    }
    if let Ok(text) = data.cast::<PyString>() {
        return Ok(text.to_str()?.as_bytes().to_vec());
    }
    Err(PyValueError::new_err("data must be bytes or str"))
}

fn rdf_format(format: PyRdfFormat) -> RdfFormat {
    match format {
        PyRdfFormat::TURTLE => RdfFormat::Turtle,
        PyRdfFormat::N_TRIPLES => RdfFormat::NTriples,
        PyRdfFormat::N_QUADS => RdfFormat::NQuads,
        PyRdfFormat::TRIG => RdfFormat::TriG,
    }
}

/// Parse RDF bytes and freeze into a validated [`RdfDataset`].
///
/// The RDF 1.2 statement layer is folded in: a `rdf:reifies` triple-term object
/// becomes a reifier binding, and a reifier subject's other triples become
/// annotations (matching the GTS producer's `add_rdf12` pass structure).
fn build_dataset(bytes: &[u8], format: RdfFormat) -> Result<Arc<RdfDataset>, String> {
    let quads = parse_quads(bytes, format).map_err(|e| format!("parse error: {e}"))?;

    let mut builder = RdfDatasetBuilder::new();

    // Pass 1: bind reifiers; collect the rest as pending base/annotation rows.
    let mut reifier_ids: std::collections::HashSet<TermId> = std::collections::HashSet::new();
    let mut pending: Vec<(TermId, TermId, TermId, Option<TermId>)> = Vec::new();
    for quad in &quads {
        let is_reifies = quad.predicate.as_str() == RDF_REIFIES;
        if let (true, OxTerm::Triple(triple)) = (is_reifies, &quad.object) {
            let rid = intern_subject(&mut builder, &quad.subject);
            let qs = intern_subject(&mut builder, &triple.subject);
            let qp = builder.intern_iri(triple.predicate.as_str().to_owned());
            let qo = intern_term(&mut builder, &triple.object)?;
            let triple_term = builder.intern_triple(qs, qp, qo);
            builder.push_reifier(rid, triple_term);
            reifier_ids.insert(rid);
        } else {
            let sid = intern_subject(&mut builder, &quad.subject);
            let pid = builder.intern_iri(quad.predicate.as_str().to_owned());
            let oid = intern_term(&mut builder, &quad.object)?;
            let gid = intern_graph(&mut builder, &quad.graph_name);
            pending.push((sid, pid, oid, gid));
        }
    }

    // Pass 2: a reifier subject's other triples are annotations; the rest base quads.
    for (sid, pid, oid, gid) in pending {
        if reifier_ids.contains(&sid) {
            builder.push_annotation(sid, pid, oid);
        } else {
            builder.push_quad(sid, pid, oid, gid);
        }
    }

    builder.freeze().map_err(|e| e.to_string())
}

fn intern_subject(builder: &mut RdfDatasetBuilder, subject: &NamedOrBlankNode) -> TermId {
    match subject {
        NamedOrBlankNode::NamedNode(n) => builder.intern_iri(n.as_str().to_owned()),
        NamedOrBlankNode::BlankNode(b) => {
            builder.intern_blank(b.as_str().to_owned(), BlankScope::DEFAULT)
        }
    }
}

fn intern_graph(builder: &mut RdfDatasetBuilder, graph: &GraphName) -> Option<TermId> {
    match graph {
        GraphName::DefaultGraph => None,
        GraphName::NamedNode(n) => Some(builder.intern_iri(n.as_str().to_owned())),
        GraphName::BlankNode(b) => {
            Some(builder.intern_blank(b.as_str().to_owned(), BlankScope::DEFAULT))
        }
    }
}

fn intern_term(builder: &mut RdfDatasetBuilder, term: &OxTerm) -> Result<TermId, String> {
    Ok(match term {
        OxTerm::NamedNode(n) => builder.intern_iri(n.as_str().to_owned()),
        OxTerm::BlankNode(b) => builder.intern_blank(b.as_str().to_owned(), BlankScope::DEFAULT),
        OxTerm::Literal(l) => {
            let lit = if let Some(lang) = l.language() {
                RdfLiteral::language_tagged(l.value().to_owned(), lang.to_owned())
            } else {
                RdfLiteral::typed(l.value().to_owned(), l.datatype().as_str().to_owned())
            };
            builder.intern_literal(lit)
        }
        OxTerm::Triple(triple) => {
            let s = intern_subject(builder, &triple.subject);
            let p = builder.intern_iri(triple.predicate.as_str().to_owned());
            let o = intern_term(builder, &triple.object)?;
            builder.intern_triple(s, p, o)
        }
    })
}

// PyRdfDataset is registered via `py_gts::register`; no standalone `register` here
// beyond the class add, which `register` performs.
pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PyRdfDataset>()?;
    Ok(())
}

/// Build a frozen dataset directly from RDF bytes — the Rust-side helper a later
/// C7 commit reuses when migrating text-exchange FFI off intermediate strings.
pub fn dataset_from_bytes(bytes: &[u8], format: RdfFormat) -> Result<Arc<RdfDataset>, String> {
    build_dataset(bytes, format)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dataset_from_bytes_counts_quads() {
        let nt = "<https://e/s> <https://e/p> <https://e/o> .\n\
                  <https://e/s> <https://e/p2> \"lit\" .\n";
        let ds = dataset_from_bytes(nt.as_bytes(), RdfFormat::NTriples).expect("build");
        assert_eq!(ds.quad_count(), 2);
        assert!(ds.term_count() >= 4);
    }

    #[test]
    fn dataset_from_bytes_classifies_rdf12_statement_layer() {
        // A reifier's reifies binding + an annotation: the base quad table is
        // empty, the reifier binding and annotation land in their own tables.
        let nt = concat!(
            "<https://e/r> <http://www.w3.org/1999/02/22-rdf-syntax-ns#reifies> ",
            "<<( <https://e/s> <https://e/p> <https://e/o> )>> .\n",
            "<https://e/r> <https://e/confidence> \"0.9\" .\n",
        );
        let ds = dataset_from_bytes(nt.as_bytes(), RdfFormat::NTriples).expect("build");
        assert_eq!(ds.quad_count(), 0, "reifier rows are not base quads");
        assert_eq!(ds.reifiers().count(), 1);
        assert_eq!(ds.annotations().count(), 1);
    }

    #[test]
    fn dataset_to_gts_folds_back() {
        let nt = "<https://e/s> <https://e/p> <https://e/o> .\n";
        let ds = dataset_from_bytes(nt.as_bytes(), RdfFormat::NTriples).expect("build");
        let store = ds.as_ref();
        let bytes = gts_write::to_gts(&store, "dist").expect("to_gts");
        let graph = gmeow_gts::reader::read(&bytes, false, None);
        assert!(graph.diagnostics.is_empty(), "{:?}", graph.diagnostics);
        assert_eq!(graph.quads.len(), 1);
    }
}
