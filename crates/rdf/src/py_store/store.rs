// SPDX-FileCopyrightText: 2026 Blackcat Informatics Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The mutable quad-store surface for the `gmeow_rdf` Python extension: the
//! SPARQL-capable `Store`, the canonicalization-capable `Dataset`, and the
//! `QuadIter` snapshot iterator they share.
//!
//! # Native backing (EPIC #906)
//!
//! `Store` wraps a copy-on-write [`MutableDataset`] over the oxigraph-free
//! `gmeow-rdf-core` IR — never `oxigraph::store::Store`. Mutation (`add` / `remove`
//! / `load`) edits the COW delta; `query` freezes a snapshot and runs the native
//! `NativeSparqlEngine`; `update` runs the engine's COW-atomic UPDATE. The
//! `_store_capsule` hands `gmeow_shacl` / `gmeow_validate` a stable
//! `Arc<RdfDataset>` snapshot under the `c"gmeow-validation-dataset"` capsule name.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use gmeow_rdf_core::ir::{MutableDataset, QuadValues};
use gmeow_sparql_eval::NativeSparqlEngine;
use pyo3::exceptions::{PyTypeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyCapsule, PyDict};

use super::canon::PyCanonicalizationAlgorithm;
use super::io::{dataset_from_quads_verbatim, parse_quads, read_input, PyRdfFormat};
use super::query::materialize_results;
use super::term::{extract_graph_name, extract_term, PyQuad, PyVariable};
use crate::{
    serialize_dataset, BlankScope, DatasetMut, GraphMatchValue, RdfDataset, RdfDatasetBuilder,
    RdfLiteral, RdfQuad, RdfTerm, RdfTriple, SerializeGraph, SparqlEngine, SparqlRequest,
    TermValue,
};

/// An in-memory RDF 1.2 quad store with SPARQL. Mirrors the oxigraph Python `Store`.
#[pyclass(name = "Store")]
pub struct PyStore {
    inner: MutableDataset,
    /// Monotonic per-load counter that isolates blank-node label scopes across
    /// separate [`load`](PyStore::load) calls (see [`load`](PyStore::load) for why).
    next_load_scope: AtomicU64,
}

#[pymethods]
impl PyStore {
    #[new]
    fn new() -> PyResult<Self> {
        Ok(Self {
            inner: empty_mutable()?,
            next_load_scope: AtomicU64::new(1),
        })
    }

    /// Load RDF into the store. Either `input` (bytes/str data) or the keyword
    /// `path` (a file to read) must be given, together with `format`.
    #[pyo3(signature = (input=None, format=None, *, path=None))]
    fn load(
        &mut self,
        input: Option<&Bound<'_, PyAny>>,
        format: Option<PyRdfFormat>,
        path: Option<String>,
    ) -> PyResult<()> {
        let format = format.ok_or_else(|| PyValueError::new_err("load: format is required"))?;
        let data = read_input(input, path)?;
        // Parse natively (#909) into the flat quad stream, then insert into the COW set.
        //
        // Blank-node labels in a serialized document are document-local: two distinct
        // documents may reuse the same label (`_:b0`) for *different* nodes, and the same
        // store loaded from many files must keep those distinct. oxigraph's prior per-load
        // blank scope gave each load call a fresh blank scope; the native codec preserves
        // labels verbatim, so we restore that isolation by tagging every parsed blank
        // node's label with a per-load-call-unique `BlankScope` before insertion.
        // `parse` / `parse_quads` keep labels verbatim — that path round-trips a single
        // document, where verbatim labels are correct and canonicalization needs them.
        let scope = BlankScope(self.next_load_scope() as u32);
        for quad in parse_quads(&data, format.to_native())
            .map_err(|e| PyValueError::new_err(format!("load error: {e}")))?
        {
            self.inner.insert(rdf_quad_to_values_scoped(&quad, scope));
        }
        Ok(())
    }

    /// Alias of [`load`] — oxigraph's bulk loader is a throughput optimization,
    /// not a different semantics, so the in-memory store path is identical.
    #[pyo3(signature = (input=None, format=None, *, path=None))]
    fn bulk_load(
        &mut self,
        input: Option<&Bound<'_, PyAny>>,
        format: Option<PyRdfFormat>,
        path: Option<String>,
    ) -> PyResult<()> {
        self.load(input, format, path)
    }

    /// Add a single quad.
    fn add(&mut self, quad: &PyQuad) -> PyResult<()> {
        self.inner.insert(rdf_quad_to_values(&quad.inner));
        Ok(())
    }

    /// Remove a single quad. No-op if the quad is absent (matches the RDFLib
    /// `Graph.remove` contract, which silently ignores misses).
    fn remove(&mut self, quad: &PyQuad) -> PyResult<()> {
        self.inner.remove(&rdf_quad_to_values(&quad.inner));
        Ok(())
    }

    /// Run a SPARQL query. Returns `QuerySolutions` (SELECT), `QueryTriples`
    /// (CONSTRUCT/DESCRIBE), or `QueryBoolean` (ASK). Optional `substitutions`
    /// is a `{Variable: term}` mapping applied natively (never string-spliced).
    #[pyo3(signature = (query, *, substitutions=None))]
    fn query(
        &self,
        py: Python<'_>,
        query: &str,
        substitutions: Option<&Bound<'_, PyDict>>,
    ) -> PyResult<Py<PyAny>> {
        let dataset = self.snapshot()?;
        let subs = collect_substitutions(substitutions)?;
        let engine = NativeSparqlEngine::new();
        let result = engine
            .query(
                &dataset,
                SparqlRequest {
                    query,
                    base_iri: None,
                    substitutions: &subs,
                },
            )
            .map_err(|e| PyValueError::new_err(format!("query evaluation error: {e}")))?;
        materialize_results(py, result)
    }

    /// Run a SPARQL UPDATE against the store (COW-atomic: a failed update leaves the
    /// store unchanged).
    fn update(&mut self, update: &str) -> PyResult<()> {
        let mut dataset = self.snapshot()?;
        let engine = NativeSparqlEngine::new();
        engine
            .update(
                &mut dataset,
                SparqlRequest {
                    query: update,
                    base_iri: None,
                    substitutions: &[],
                },
            )
            .map_err(|e| PyValueError::new_err(format!("update evaluation error: {e}")))?;
        // The UPDATE produced a fresh frozen base; adopt it as the new COW base.
        self.inner = MutableDataset::new(dataset);
        Ok(())
    }

    /// Dump the whole store (or one graph, via `from_graph`) in `format`. Mirrors
    /// the oxigraph Python `Store.dump`: when `output` (a file-like with `.write`) is given
    /// the bytes are written to it and `None` is returned; otherwise the bytes are
    /// returned directly.
    #[pyo3(signature = (output=None, format=None, *, from_graph=None))]
    fn dump(
        &self,
        py: Python<'_>,
        output: Option<&Bound<'_, PyAny>>,
        format: Option<PyRdfFormat>,
        from_graph: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<Option<Py<PyBytes>>> {
        let format = format.ok_or_else(|| PyValueError::new_err("dump: format is required"))?;
        let native = format.to_native();
        // Serialize natively (#909): materialize the store's quads into the IR verbatim
        // (preserving literal lexical forms) and dispatch to the native codec.
        let (quads, selection) = if native.supports_datasets() && from_graph.is_none() {
            (self.collect_all_quads(), SerializeGraph::Dataset)
        } else {
            // `from_graph` selects one graph (a NamedNode/BlankNode → that graph; an
            // explicit DefaultGraph, or no `from_graph` on a non-dataset format → the
            // default graph). Project its triples into the default graph.
            let graph = extract_graph_name(from_graph)?;
            (
                self.collect_graph_quads(graph.as_ref()),
                SerializeGraph::DefaultGraph,
            )
        };
        let dataset = dataset_from_quads_verbatim(&quads)
            .map_err(|e| PyValueError::new_err(format!("dump error: {e}")))?;
        let buf = serialize_dataset(&dataset, native.media_type(), selection)
            .map_err(|e| PyValueError::new_err(format!("dump error: {e}")))?;
        match output {
            Some(output) => {
                output.call_method1("write", (PyBytes::new(py, &buf),))?;
                Ok(None)
            }
            None => Ok(Some(PyBytes::new(py, &buf).unbind())),
        }
    }

    fn __len__(&self) -> usize {
        self.inner
            .quads_for_pattern(None, None, None, GraphMatchValue::Any)
            .len()
    }

    /// Iterate the store's quads (a snapshot taken at iteration time).
    fn __iter__(slf: PyRef<'_, Self>, py: Python<'_>) -> PyResult<Py<PyQuadIter>> {
        let quads = slf
            .inner
            .quads_for_pattern(None, None, None, GraphMatchValue::Any)
            .iter()
            .map(values_to_rdf_quad)
            .collect();
        Py::new(py, PyQuadIter { quads, pos: 0 })
    }

    /// Internal protocol: a capsule exposing a frozen `Arc<RdfDataset>` snapshot of
    /// this store by address, consumed by `gmeow_shacl.Shapes.validate_store` so the
    /// SHACL engine validates this store natively with no N-Triples round-trip. Do
    /// not call from Python directly. The capsule name and pointee type match exactly
    /// what `gmeow_shacl` consumes from `gmeow_validate.ValidationStore`.
    ///
    /// The capsule's destructor owns the `Arc<RdfDataset>`, so the dataset is kept
    /// alive for the capsule's entire lifetime. Because the snapshot is an immutable
    /// frozen `Arc` taken now, a later `add`/`remove`/`update` on this `Store` leaves
    /// the snapshot a consumer already holds untouched (snapshot-vs-mutation aliasing
    /// safety).
    fn _store_capsule<'py>(slf: &Bound<'py, Self>) -> PyResult<Bound<'py, PyCapsule>> {
        let py = slf.py();
        let snapshot: Arc<RdfDataset> = slf.borrow().snapshot()?;
        // Heap-box the Arc so its address is stable; the destructor reclaims the box
        // (dropping the held Arc) when the capsule is collected.
        let boxed: Box<Arc<RdfDataset>> = Box::new(snapshot);
        let addr = (&*boxed as *const Arc<RdfDataset>) as usize;
        let keepalive = boxed;
        // SAFETY: `addr` is the address of the `Arc<RdfDataset>` owned by `keepalive`,
        // moved into the destructor closure; it stays live and at a stable address for
        // the capsule's entire lifetime. The consumer reads the `Arc<RdfDataset>` at
        // that address (cloning it to extend the lifetime as needed).
        PyCapsule::new_with_value_and_destructor(
            py,
            addr,
            c"gmeow-validation-dataset",
            move |_addr, _ctx| drop(keepalive),
        )
    }
}

impl PyStore {
    /// The next per-load blank scope ordinal (monotonic, wrapping past 1).
    fn next_load_scope(&self) -> u64 {
        self.next_load_scope.fetch_add(1, Ordering::Relaxed)
    }

    /// Freeze the effective COW quad set into an immutable `Arc<RdfDataset>` snapshot.
    fn snapshot(&self) -> PyResult<Arc<RdfDataset>> {
        self.inner
            .freeze()
            .map_err(|e| PyValueError::new_err(format!("store snapshot failed: {e}")))
    }

    /// Every quad in the store, graph names intact (for the dataset-format dump path).
    fn collect_all_quads(&self) -> Vec<RdfQuad> {
        self.inner
            .quads_for_pattern(None, None, None, GraphMatchValue::Any)
            .iter()
            .map(values_to_rdf_quad)
            .collect()
    }

    /// The quads of ONE graph, re-homed to the default graph (so a single-graph dump
    /// serializes as triples). `graph` is the selected graph term, or `None` for the
    /// default graph — matching the oxigraph `Store.dump(from_graph=…)` projection.
    fn collect_graph_quads(&self, graph: Option<&RdfTerm>) -> Vec<RdfQuad> {
        self.inner
            .quads_for_pattern(None, None, None, GraphMatchValue::Any)
            .iter()
            .filter_map(|values| {
                let quad = values_to_rdf_quad(values);
                (quad.graph_name.as_ref() == graph).then(|| {
                    let mut projected = quad;
                    projected.graph_name = None;
                    projected
                })
            })
            .collect()
    }
}

/// An in-memory quad set supporting RDFC-1.0 canonicalization. Mirrors
/// the oxigraph Python `Dataset`.
#[pyclass(name = "Dataset")]
pub struct PyDataset {
    /// The accumulated quads, deduplicated by content (set semantics).
    quads: Vec<RdfQuad>,
}

#[pymethods]
impl PyDataset {
    /// Build a dataset, optionally seeding it from an iterable of `Quad`.
    #[new]
    #[pyo3(signature = (quads=None))]
    fn new(quads: Option<&Bound<'_, PyAny>>) -> PyResult<Self> {
        let mut out = Self { quads: Vec::new() };
        if let Some(quads) = quads {
            for item in quads.try_iter()? {
                let item = item?;
                let quad = item
                    .cast::<PyQuad>()
                    .map_err(|_| PyTypeError::new_err("Dataset accepts an iterable of Quad"))?;
                out.insert(quad.get().inner.clone());
            }
        }
        Ok(out)
    }

    /// Add a single quad.
    fn add(&mut self, quad: &PyQuad) {
        self.insert(quad.inner.clone());
    }

    /// Canonicalize blank-node labels in place under `algorithm` (native RDFC-1.0).
    fn canonicalize(&mut self, algorithm: PyCanonicalizationAlgorithm) {
        self.quads = super::canon::canonicalize_quads(std::mem::take(&mut self.quads), algorithm);
    }

    fn __len__(&self) -> usize {
        self.quads.len()
    }

    fn __iter__(slf: PyRef<'_, Self>, py: Python<'_>) -> PyResult<Py<PyQuadIter>> {
        let quads = slf.quads.clone();
        Py::new(py, PyQuadIter { quads, pos: 0 })
    }
}

impl PyDataset {
    /// Insert a quad with set semantics (no duplicate content).
    fn insert(&mut self, quad: RdfQuad) {
        if !self.quads.contains(&quad) {
            self.quads.push(quad);
        }
    }
}

/// Iterator over a [`PyDataset`]'s / [`PyStore`]'s quads (snapshot at iteration time).
#[pyclass(name = "QuadIter")]
pub struct PyQuadIter {
    pub(crate) quads: Vec<RdfQuad>,
    pub(crate) pos: usize,
}

#[pymethods]
impl PyQuadIter {
    fn __iter__(slf: PyRef<'_, Self>) -> PyRef<'_, Self> {
        slf
    }

    fn __next__(mut slf: PyRefMut<'_, Self>, py: Python<'_>) -> PyResult<Option<Py<PyQuad>>> {
        if slf.pos >= slf.quads.len() {
            return Ok(None);
        }
        let quad = slf.quads[slf.pos].clone();
        slf.pos += 1;
        Ok(Some(Py::new(py, PyQuad { inner: quad })?))
    }
}

// ── conversion helpers (native owned model ⇄ MutableDataset value model) ──────────

const XSD_STRING: &str = "http://www.w3.org/2001/XMLSchema#string";
const RDF_LANG_STRING: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#langString";

fn empty_mutable() -> PyResult<MutableDataset> {
    let base = RdfDatasetBuilder::new()
        .freeze()
        .map_err(|e| PyValueError::new_err(e.to_string()))?;
    Ok(MutableDataset::new(base))
}

/// Collect the `{Variable: term}` substitution dict into the native
/// `(name, TermValue)` pre-binding slice the SPARQL request carries.
fn collect_substitutions(
    substitutions: Option<&Bound<'_, PyDict>>,
) -> PyResult<Vec<(String, TermValue)>> {
    let Some(subs) = substitutions else {
        return Ok(Vec::new());
    };
    let mut out = Vec::with_capacity(subs.len());
    for (key, value) in subs.iter() {
        let name = key
            .cast::<PyVariable>()
            .map_err(|_| PyTypeError::new_err("substitution keys must be Variable"))?
            .get()
            .inner
            .clone();
        out.push((name, rdf_term_to_value(&extract_term(&value)?)));
    }
    Ok(out)
}

/// Convert a native owned [`RdfQuad`] into the `MutableDataset` [`QuadValues`] model
/// under the default blank scope.
fn rdf_quad_to_values(quad: &RdfQuad) -> QuadValues {
    rdf_quad_to_values_scoped(quad, BlankScope::DEFAULT)
}

/// Convert a native owned [`RdfQuad`] into [`QuadValues`], tagging every blank node
/// with `scope` (the per-load isolation scope).
fn rdf_quad_to_values_scoped(quad: &RdfQuad, scope: BlankScope) -> QuadValues {
    QuadValues {
        s: rdf_term_to_value_scoped(&quad.subject, scope),
        p: TermValue::Iri(quad.predicate.clone()),
        o: rdf_term_to_value_scoped(&quad.object, scope),
        g: quad
            .graph_name
            .as_ref()
            .map(|g| rdf_term_to_value_scoped(g, scope)),
    }
}

fn rdf_term_to_value(term: &RdfTerm) -> TermValue {
    rdf_term_to_value_scoped(term, BlankScope::DEFAULT)
}

fn rdf_term_to_value_scoped(term: &RdfTerm, scope: BlankScope) -> TermValue {
    match term {
        RdfTerm::Iri(iri) => TermValue::Iri(iri.clone()),
        RdfTerm::BlankNode(label) => TermValue::Blank {
            label: label.clone(),
            scope,
        },
        RdfTerm::Literal(lit) => TermValue::Literal {
            lexical_form: lit.lexical_form.clone(),
            datatype: match (&lit.datatype, &lit.language) {
                (Some(dt), _) => dt.clone(),
                (None, Some(_)) => RDF_LANG_STRING.to_owned(),
                (None, None) => XSD_STRING.to_owned(),
            },
            language: lit.language.clone(),
            direction: lit.direction,
        },
        RdfTerm::Triple(t) => TermValue::Triple {
            s: Box::new(rdf_term_to_value_scoped(&t.subject, scope)),
            p: Box::new(TermValue::Iri(t.predicate.clone())),
            o: Box::new(rdf_term_to_value_scoped(&t.object, scope)),
        },
    }
}

/// Convert a [`QuadValues`] back into the native owned [`RdfQuad`] model. Blank labels
/// are scope-qualified so a per-load scope is reflected in the surfaced label
/// (matching the prior oxigraph store's scoped blanks).
fn values_to_rdf_quad(values: &QuadValues) -> RdfQuad {
    let mut quad = RdfQuad::new(
        value_to_rdf_term(&values.s),
        predicate_iri(&values.p),
        value_to_rdf_term(&values.o),
    );
    quad.graph_name = values.g.as_ref().map(value_to_rdf_term);
    quad
}

fn predicate_iri(value: &TermValue) -> String {
    match value {
        TermValue::Iri(iri) => iri.clone(),
        other => value_to_rdf_term(other).to_string(),
    }
}

fn value_to_rdf_term(value: &TermValue) -> RdfTerm {
    match value {
        TermValue::Iri(iri) => RdfTerm::Iri(iri.clone()),
        TermValue::Blank { label, scope } => {
            RdfTerm::BlankNode(scope.qualify_label(label).into_owned())
        }
        TermValue::Literal {
            lexical_form,
            datatype,
            language,
            direction,
        } => RdfTerm::Literal(RdfLiteral {
            datatype: collapse_synthetic_datatype(datatype, language),
            lexical_form: lexical_form.clone(),
            language: language.clone(),
            direction: *direction,
        }),
        TermValue::Triple { s, p, o } => RdfTerm::triple(RdfTriple::new(
            value_to_rdf_term(s),
            predicate_iri(p),
            value_to_rdf_term(o),
        )),
    }
}

/// Drop the synthetic `xsd:string` / `rdf:langString` datatype the value model always
/// carries, leaving the owned model's plain / lang literals datatype-less.
fn collapse_synthetic_datatype(datatype: &str, language: &Option<String>) -> Option<String> {
    if language.is_some() {
        return (datatype != RDF_LANG_STRING).then(|| datatype.to_owned());
    }
    (datatype != XSD_STRING).then(|| datatype.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn iri(s: &str) -> RdfTerm {
        RdfTerm::iri(s)
    }

    #[test]
    fn scoping_keeps_iris_and_literals_verbatim() {
        let quad = RdfQuad::new(
            iri("https://e/s"),
            "https://e/p",
            RdfTerm::literal(RdfLiteral::simple("v")),
        );
        let values = rdf_quad_to_values_scoped(&quad, BlankScope(7));
        let back = values_to_rdf_quad(&values);
        assert_eq!(back, quad, "no blank node: the quad is unchanged");
    }

    #[test]
    fn same_label_different_scopes_yields_distinct_nodes() {
        // The regression guard (#909): the SAME document-local blank label loaded
        // under two different scopes (two `Store::load` calls) MUST become two
        // distinct nodes once surfaced.
        let quad = RdfQuad::new(RdfTerm::blank_node("b0"), "https://e/p", iri("https://e/o"));
        let a = values_to_rdf_quad(&rdf_quad_to_values_scoped(&quad, BlankScope(1)));
        let b = values_to_rdf_quad(&rdf_quad_to_values_scoped(&quad, BlankScope(2)));
        assert_ne!(a.subject, b.subject);
        // …but the same label within one scope is the SAME node (intra-document joins).
        let a2 = values_to_rdf_quad(&rdf_quad_to_values_scoped(&quad, BlankScope(1)));
        assert_eq!(a.subject, a2.subject);
    }

    #[test]
    fn scoping_recurses_into_quoted_triple_terms() {
        let quad = RdfQuad::new(
            RdfTerm::blank_node("r"),
            "https://e/p",
            RdfTerm::triple(RdfTriple::new(
                RdfTerm::blank_node("s"),
                "https://e/q",
                RdfTerm::blank_node("o"),
            )),
        );
        let values = rdf_quad_to_values_scoped(&quad, BlankScope(5));
        let back = values_to_rdf_quad(&values);
        let RdfTerm::Triple(t) = &back.object else {
            panic!("object must stay a quoted triple");
        };
        // Both the reifier subject and the inner triple's blanks carry the scope.
        assert!(matches!(&back.subject, RdfTerm::BlankNode(l) if l.contains('5')));
        assert!(matches!(&t.subject, RdfTerm::BlankNode(l) if l.contains('5')));
        assert!(matches!(&t.object, RdfTerm::BlankNode(l) if l.contains('5')));
    }

    #[test]
    fn plain_literal_round_trips_without_synthetic_datatype() {
        let values = QuadValues {
            s: TermValue::Iri("https://e/s".to_owned()),
            p: TermValue::Iri("https://e/p".to_owned()),
            o: TermValue::Literal {
                lexical_form: "hi".to_owned(),
                datatype: XSD_STRING.to_owned(),
                language: None,
                direction: None,
            },
            g: None,
        };
        let quad = values_to_rdf_quad(&values);
        let RdfTerm::Literal(lit) = &quad.object else {
            panic!("expected a literal");
        };
        assert_eq!(lit.datatype, None, "plain literal stays datatype-less");
    }
}
