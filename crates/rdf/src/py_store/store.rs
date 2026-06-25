// SPDX-FileCopyrightText: 2026 Blackcat Informatics Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The mutable quad-store surface for the `gmeow_rdf` Python extension: the
//! SPARQL-capable `Store`, the canonicalization-capable `Dataset`, and the
//! `QuadIter` snapshot iterator they share.

use std::sync::atomic::{AtomicU64, Ordering};

use oxigraph::model::{
    BlankNode, Dataset, GraphName, GraphNameRef, NamedOrBlankNode, Quad, Term, Triple,
};
use oxigraph::sparql::SparqlEvaluator;
use oxigraph::store::Store;
use pyo3::exceptions::{PyTypeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyCapsule, PyDict};

use super::canon::PyCanonicalizationAlgorithm;
use super::io::{parse_quads, read_input, PyRdfFormat};
use super::query::materialize_results;
use super::term::{extract_graph_name, extract_term, PyQuad, PyVariable};
use crate::{serialize_dataset, SerializeGraph};

/// An in-memory RDF 1.2 quad store with SPARQL. Mirrors the oxigraph Python `Store`.
#[pyclass(name = "Store")]
pub struct PyStore {
    inner: Store,
    /// Monotonic per-load counter that isolates blank-node label scopes across
    /// separate [`load`](PyStore::load) calls (see [`load`](PyStore::load) for why).
    next_load_scope: AtomicU64,
}

#[pymethods]
impl PyStore {
    #[new]
    fn new() -> PyResult<Self> {
        Ok(Self {
            inner: Store::new().map_err(store_err)?,
            next_load_scope: AtomicU64::new(1),
        })
    }

    /// Load RDF into the store. Either `input` (bytes/str data) or the keyword
    /// `path` (a file to read) must be given, together with `format`.
    #[pyo3(signature = (input=None, format=None, *, path=None))]
    fn load(
        &self,
        input: Option<&Bound<'_, PyAny>>,
        format: Option<PyRdfFormat>,
        path: Option<String>,
    ) -> PyResult<()> {
        let format = format.ok_or_else(|| PyValueError::new_err("load: format is required"))?;
        let data = read_input(input, path)?;
        // Parse natively (#909) into the flat quad stream, then insert into the store.
        //
        // Blank-node labels in a serialized document are document-local: two distinct
        // documents may reuse the same label (`_:b0`, or a content-addressed hash an
        // anonymous node lands on) for *different* nodes, and the same store loaded from
        // many files must keep those distinct. oxigraph's prior `Store::load_from_slice`
        // gave each load call a fresh blank scope for exactly this reason; the native
        // codec preserves labels verbatim, so we restore that isolation here by rewriting
        // every parsed blank node's label with a per-load-call-unique prefix before
        // insertion (the COW `MutableDataset::load` does the equivalent via `BlankScope`).
        // `parse` / `parse_quads` keep labels verbatim — that path round-trips a single
        // document, where verbatim labels are correct and canonicalization needs them.
        let scope = self.next_load_scope.fetch_add(1, Ordering::Relaxed);
        for quad in parse_quads(&data, format.to_native())
            .map_err(|e| PyValueError::new_err(format!("load error: {e}")))?
        {
            self.inner
                .insert(&scope_quad_blanks(&quad, scope))
                .map_err(store_err)?;
        }
        Ok(())
    }

    /// Alias of [`load`] — oxigraph's bulk loader is a throughput optimization,
    /// not a different semantics, so the in-memory store path is identical.
    #[pyo3(signature = (input=None, format=None, *, path=None))]
    fn bulk_load(
        &self,
        input: Option<&Bound<'_, PyAny>>,
        format: Option<PyRdfFormat>,
        path: Option<String>,
    ) -> PyResult<()> {
        self.load(input, format, path)
    }

    /// Add a single quad.
    fn add(&self, quad: &PyQuad) -> PyResult<()> {
        self.inner.insert(&quad.inner).map_err(store_err)?;
        Ok(())
    }

    /// Remove a single quad. No-op if the quad is absent (matches the RDFLib
    /// `Graph.remove` contract, which silently ignores misses).
    fn remove(&self, quad: &PyQuad) -> PyResult<()> {
        self.inner.remove(&quad.inner).map_err(store_err)?;
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
        let mut evaluator = SparqlEvaluator::new()
            .parse_query(query)
            .map_err(|e| PyValueError::new_err(format!("query parse error: {e}")))?;
        if let Some(subs) = substitutions {
            for (key, value) in subs.iter() {
                let var = key
                    .cast::<PyVariable>()
                    .map_err(|_| PyTypeError::new_err("substitution keys must be Variable"))?
                    .get()
                    .inner
                    .clone();
                evaluator = evaluator.substitute_variable(var, extract_term(&value)?);
            }
        }
        let results = evaluator
            .on_store(&self.inner)
            .execute()
            .map_err(|e| PyValueError::new_err(format!("query evaluation error: {e}")))?;
        materialize_results(py, results)
    }

    /// Run a SPARQL UPDATE against the store.
    fn update(&self, update: &str) -> PyResult<()> {
        self.inner
            .update(update)
            .map_err(|e| PyValueError::new_err(format!("update evaluation error: {e}")))
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
        // (preserving literal lexical forms the oxigraph serializer would canonicalize)
        // and dispatch to the native codec.
        let (quads, selection) = if native.supports_datasets() && from_graph.is_none() {
            (self.collect_quads(None)?, SerializeGraph::Dataset)
        } else {
            let graph = extract_graph_name(from_graph)?;
            // Project a single graph: emit its triples in the default graph.
            (
                self.collect_quads(Some(&graph))?,
                SerializeGraph::DefaultGraph,
            )
        };
        let dataset = super::io::dataset_from_ox_quads_verbatim(&quads)
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

    fn __len__(&self) -> PyResult<usize> {
        self.inner.len().map_err(store_err)
    }

    /// Iterate the store's quads (a snapshot taken at iteration time).
    fn __iter__(slf: PyRef<'_, Self>, py: Python<'_>) -> PyResult<Py<PyQuadIter>> {
        let mut quads = Vec::new();
        for quad in slf.inner.iter() {
            quads.push(quad.map_err(store_err)?);
        }
        Py::new(py, PyQuadIter { quads, pos: 0 })
    }

    /// Internal protocol: a capsule exposing the wrapped oxigraph store by
    /// address, consumed by `gmeow_shacl.Shapes.validate_store` so the SHACL
    /// engine validates this store with no N-Triples round-trip. Do not call
    /// from Python directly. The capsule name and pointee type match exactly
    /// what `gmeow_shacl` consumes from `gmeow_validate.ValidationStore`.
    ///
    /// The capsule's destructor owns a strong reference to `self`, so the store
    /// is kept alive for the capsule's entire lifetime — the borrow is *enforced*
    /// rather than merely assumed, closing the use-after-free a stray Python
    /// reference to the capsule would otherwise open.
    fn _store_capsule<'py>(slf: &Bound<'py, Self>) -> PyResult<Bound<'py, PyCapsule>> {
        let py = slf.py();
        let addr = &slf.borrow().inner as *const Store as usize;
        // Strong ref to the Python `Store`; dropped (under the GIL) only when the
        // capsule itself is collected, so `self.inner` cannot dangle beneath it.
        let keepalive: Py<Self> = slf.clone().unbind();
        // SAFETY: the capsule's value is the address of `self.inner`, whose
        // storage is stable for the lifetime of the pyclass instance that
        // `keepalive` pins. The consumer reads only the value, never the context.
        PyCapsule::new_with_value_and_destructor(
            py,
            addr,
            c"gmeow-validation-store",
            move |_addr, _ctx| drop(keepalive),
        )
    }
}

impl PyStore {
    /// Snapshot the store's quads. When `graph` is `Some`, only that graph's quads are
    /// returned, re-homed to the default graph (so a single-graph dump serializes as
    /// triples); when `None`, every quad is returned with its graph name intact.
    fn collect_quads(&self, graph: Option<&GraphName>) -> PyResult<Vec<Quad>> {
        let mut quads = Vec::new();
        for quad in self.inner.iter() {
            let quad = quad.map_err(store_err)?;
            match graph {
                Some(g) => {
                    if &quad.graph_name == g {
                        quads.push(Quad::new(
                            quad.subject,
                            quad.predicate,
                            quad.object,
                            GraphNameRef::DefaultGraph,
                        ));
                    }
                }
                None => quads.push(quad),
            }
        }
        Ok(quads)
    }
}

/// An in-memory quad set supporting RDFC-1.0 canonicalization. Mirrors
/// the oxigraph Python `Dataset`.
#[pyclass(name = "Dataset")]
pub struct PyDataset {
    inner: Dataset,
}

#[pymethods]
impl PyDataset {
    /// Build a dataset, optionally seeding it from an iterable of `Quad`.
    #[new]
    #[pyo3(signature = (quads=None))]
    fn new(quads: Option<&Bound<'_, PyAny>>) -> PyResult<Self> {
        let mut inner = Dataset::new();
        if let Some(quads) = quads {
            for item in quads.try_iter()? {
                let item = item?;
                let quad = item
                    .cast::<PyQuad>()
                    .map_err(|_| PyTypeError::new_err("Dataset accepts an iterable of Quad"))?;
                inner.insert(&quad.get().inner);
            }
        }
        Ok(Self { inner })
    }

    /// Add a single quad.
    fn add(&mut self, quad: &PyQuad) {
        self.inner.insert(&quad.inner);
    }

    /// Canonicalize blank-node labels in place under `algorithm` (native RDFC-1.0).
    fn canonicalize(&mut self, algorithm: PyCanonicalizationAlgorithm) {
        let quads: Vec<Quad> = self.inner.iter().map(|q| q.into_owned()).collect();
        self.inner = super::canon::canonicalize_quads(quads, algorithm)
            .into_iter()
            .collect();
    }

    fn __len__(&self) -> usize {
        self.inner.len()
    }

    fn __iter__(slf: PyRef<'_, Self>, py: Python<'_>) -> PyResult<Py<PyQuadIter>> {
        let quads: Vec<Quad> = slf.inner.iter().map(|q| q.into_owned()).collect();
        Py::new(py, PyQuadIter { quads, pos: 0 })
    }
}

/// Iterator over a [`PyDataset`]'s quads (snapshot at iteration time).
#[pyclass(name = "QuadIter")]
pub struct PyQuadIter {
    pub(crate) quads: Vec<Quad>,
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

fn store_err(e: impl std::fmt::Display) -> PyErr {
    PyValueError::new_err(format!("store error: {e}"))
}

/// Rewrite every blank node in `quad` with a `scope`-local label so blank nodes from
/// separate [`load`](PyStore::load) calls cannot collide. IRIs and literals pass
/// through unchanged; the label rewrite is applied recursively through RDF 1.2
/// quoted-triple terms and to the (rare) blank-node graph name.
fn scope_quad_blanks(quad: &Quad, scope: u64) -> Quad {
    Quad::new(
        scope_subject_blanks(&quad.subject, scope),
        quad.predicate.clone(),
        scope_term_blanks(&quad.object, scope),
        match &quad.graph_name {
            GraphName::DefaultGraph => GraphName::DefaultGraph,
            GraphName::NamedNode(n) => GraphName::NamedNode(n.clone()),
            GraphName::BlankNode(b) => GraphName::BlankNode(scoped_blank(b.as_str(), scope)),
        },
    )
}

/// A `scope`-local blank node: prefix the document-local label so it is unique to one
/// load call while staying deterministic within it (a label reused inside the same
/// document still resolves to the same node — only cross-load reuse is separated).
fn scoped_blank(label: &str, scope: u64) -> BlankNode {
    // `BlankNode::new` rejects labels with characters illegal in an N-Triples blank id;
    // the parsed label is already a valid blank id and the prefix uses only `[A-Za-z0-9]`,
    // so the composed label is always valid — `new_unchecked` is sound and avoids the
    // fallible path on a value we know is well-formed.
    BlankNode::new_unchecked(format!("ld{scope}x{label}"))
}

fn scope_subject_blanks(subject: &NamedOrBlankNode, scope: u64) -> NamedOrBlankNode {
    match subject {
        NamedOrBlankNode::NamedNode(n) => NamedOrBlankNode::NamedNode(n.clone()),
        NamedOrBlankNode::BlankNode(b) => {
            NamedOrBlankNode::BlankNode(scoped_blank(b.as_str(), scope))
        }
    }
}

fn scope_term_blanks(term: &Term, scope: u64) -> Term {
    match term {
        Term::NamedNode(n) => Term::NamedNode(n.clone()),
        Term::BlankNode(b) => Term::BlankNode(scoped_blank(b.as_str(), scope)),
        Term::Literal(l) => Term::Literal(l.clone()),
        Term::Triple(t) => Term::Triple(Box::new(Triple::new(
            scope_subject_blanks(&t.subject, scope),
            t.predicate.clone(),
            scope_term_blanks(&t.object, scope),
        ))),
    }
}

#[cfg(test)]
mod tests {
    use oxigraph::model::{Literal, NamedNode};

    use super::*;

    fn iri(s: &str) -> NamedNode {
        NamedNode::new(s).unwrap()
    }

    #[test]
    fn scoping_keeps_iris_and_literals_verbatim() {
        let quad = Quad::new(
            iri("https://e/s"),
            iri("https://e/p"),
            Literal::new_simple_literal("v"),
            GraphName::DefaultGraph,
        );
        let scoped = scope_quad_blanks(&quad, 7);
        assert_eq!(scoped, quad, "no blank node: the quad is unchanged");
    }

    #[test]
    fn scoping_rewrites_blank_subject_and_object_under_one_scope() {
        let quad = Quad::new(
            BlankNode::new_unchecked("b0"),
            iri("https://e/p"),
            Term::BlankNode(BlankNode::new_unchecked("b1")),
            GraphName::DefaultGraph,
        );
        let scoped = scope_quad_blanks(&quad, 3);
        assert_eq!(
            scoped.subject,
            NamedOrBlankNode::BlankNode(BlankNode::new_unchecked("ld3xb0"))
        );
        assert_eq!(
            scoped.object,
            Term::BlankNode(BlankNode::new_unchecked("ld3xb1"))
        );
    }

    #[test]
    fn same_label_different_scopes_yields_distinct_nodes() {
        // The regression guard (#909): the SAME document-local blank label loaded under
        // two different scopes (two `Store::load` calls) MUST become two distinct nodes,
        // mirroring oxigraph's prior per-load blank isolation.
        let a = scoped_blank("b0", 1);
        let b = scoped_blank("b0", 2);
        assert_ne!(a, b);
        // …but the same label within one scope is the SAME node (intra-document joins).
        assert_eq!(scoped_blank("b0", 1), scoped_blank("b0", 1));
    }

    #[test]
    fn scoping_recurses_into_quoted_triple_terms() {
        let quad = Quad::new(
            BlankNode::new_unchecked("r"),
            iri("https://e/p"),
            Term::Triple(Box::new(Triple::new(
                BlankNode::new_unchecked("s"),
                iri("https://e/q"),
                Term::BlankNode(BlankNode::new_unchecked("o")),
            ))),
            GraphName::DefaultGraph,
        );
        let scoped = scope_quad_blanks(&quad, 5);
        let Term::Triple(t) = scoped.object else {
            panic!("object must stay a quoted triple");
        };
        assert_eq!(
            t.subject,
            NamedOrBlankNode::BlankNode(BlankNode::new_unchecked("ld5xs"))
        );
        assert_eq!(t.object, Term::BlankNode(BlankNode::new_unchecked("ld5xo")));
    }
}
