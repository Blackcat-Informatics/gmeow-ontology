// SPDX-FileCopyrightText: 2026 Blackcat Informatics Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The mutable quad-store surface for the `gmeow_rdf` Python extension: the
//! SPARQL-capable `Store`, the canonicalization-capable `Dataset`, and the
//! `QuadIter` snapshot iterator they share.

use oxigraph::io::{RdfParser, RdfSerializer};
use oxigraph::model::{Dataset, Quad};
use oxigraph::sparql::SparqlEvaluator;
use oxigraph::store::Store;
use pyo3::exceptions::{PyTypeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyCapsule, PyDict};

use super::canon::PyCanonicalizationAlgorithm;
use super::io::{read_input, PyRdfFormat};
use super::query::materialize_results;
use super::term::{extract_graph_name, extract_term, PyQuad, PyVariable};

/// An in-memory RDF 1.2 quad store with SPARQL. Mirrors the oxigraph Python `Store`.
#[pyclass(name = "Store")]
pub struct PyStore {
    inner: Store,
}

#[pymethods]
impl PyStore {
    #[new]
    fn new() -> PyResult<Self> {
        Ok(Self {
            inner: Store::new().map_err(store_err)?,
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
        self.inner
            .load_from_slice(RdfParser::from_format(format.to_ox()).lenient(), &data)
            .map_err(|e| PyValueError::new_err(format!("load error: {e}")))
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
        let ox_format = format.to_ox();
        let mut buf: Vec<u8> = Vec::new();
        if ox_format.supports_datasets() && from_graph.is_none() {
            self.inner
                .dump_to_writer(RdfSerializer::from_format(ox_format), &mut buf)
                .map_err(|e| PyValueError::new_err(format!("dump error: {e}")))?;
        } else {
            let graph = extract_graph_name(from_graph)?;
            self.inner
                .dump_graph_to_writer(
                    graph.as_ref(),
                    RdfSerializer::from_format(ox_format),
                    &mut buf,
                )
                .map_err(|e| PyValueError::new_err(format!("dump error: {e}")))?;
        }
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
