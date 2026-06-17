// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! PyO3 Python bindings for `gmeow-shacl`.
//!
//! # Platform note
//!
//! This module is compiled only on native (non-wasm32) targets because pyo3
//! physically cannot link into a wasm binary — the CPython C extension ABI is
//! unavailable there. The `#[cfg(not(target_arch = "wasm32"))]` guard in
//! `lib.rs` is platform-correct, not an optionality toggle: there are zero
//! degraded fallbacks and zero feature flags controlling this.
//!
//! # Engine core separation
//!
//! Only this file imports pyo3. All engine modules (`engine`, `shapes`,
//! `constraints`, `path`, `report`, `model`) are PyO3-free so the rlib links
//! into the future Rust compiler without any Python dependency.

use oxigraph::io::{RdfFormat, RdfParser};
use oxigraph::store::Store;
use pyo3::prelude::*;
use pyo3::types::{PyAny, PyCapsule, PyCapsuleMethods, PyDict, PyList};

use crate::engine;

/// Validate a data graph (N-Triples) against a shapes graph (Turtle).
///
/// Returns a dict with keys:
/// - `"conforms"` — bool
/// - `"results"` — list of dicts, each with keys:
///   `"focus"`, `"path"`, `"value"`, `"severity"`, `"component"`,
///   `"source_shape"`, `"message"`.
#[pyfunction]
fn validate(py: Python<'_>, shapes_ttl: &str, data_nt: &str) -> PyResult<Py<PyAny>> {
    let report = engine::validate_graphs(data_nt, shapes_ttl)
        .map_err(pyo3::exceptions::PyValueError::new_err)?;

    let out = PyDict::new(py);
    out.set_item("conforms", report.conforms)?;

    let results = PyList::empty(py);
    for r in &report.results {
        let d = PyDict::new(py);
        d.set_item("focus", r.focus_node.to_string())?;
        d.set_item("path", r.result_path.as_ref().map(|t| t.to_string()))?;
        d.set_item("value", r.value.as_ref().map(|t| t.to_string()))?;
        d.set_item("severity", r.severity.iri())?;
        d.set_item("component", r.source_constraint_component.as_str())?;
        d.set_item("source_shape", r.source_shape.to_string())?;
        d.set_item("message", r.message.clone())?;
        results.append(d)?;
    }
    out.set_item("results", results)?;

    Ok(out.into_any().unbind())
}

/// Parsed SHACL shapes that can be reused to validate multiple data graphs.
///
/// Construct from a Turtle shapes graph with `PyShapes(shapes_ttl)`, then call
/// `validate_nt(data_nt)` for each data graph. The Rust orchestration path in
/// `gmeow-validate` borrows the parsed shapes via [`Self::validate_against_store`].
#[pyclass(name = "Shapes")]
pub struct PyShapes {
    inner: crate::shapes::Shapes,
}

impl PyShapes {
    /// Validate a borrowed oxigraph store against these parsed shapes.
    ///
    /// This is the Rust-side primitive used by `gmeow-validate::PyValidationStore`
    /// so the data store does not have to be re-serialized to N-Triples.
    pub fn validate_against_store(&self, data: &Store) -> crate::report::ValidationReport {
        engine::validate(data, &self.inner)
    }
}

#[pymethods]
impl PyShapes {
    #[new]
    fn new(shapes_ttl: String) -> PyResult<Self> {
        let inner =
            engine::parse_shapes(&shapes_ttl).map_err(pyo3::exceptions::PyValueError::new_err)?;
        Ok(Self { inner })
    }

    /// Validate an N-Triples data graph against these parsed shapes.
    fn validate_nt(&self, data_nt: String) -> PyResult<PyValidationReport> {
        let data = Store::new().map_err(|e| {
            pyo3::exceptions::PyValueError::new_err(format!("data store creation failed: {e}"))
        })?;
        if !data_nt.is_empty() {
            data.load_from_reader(
                RdfParser::from_format(RdfFormat::NTriples).lenient(),
                data_nt.as_bytes(),
            )
            .map_err(|e| {
                pyo3::exceptions::PyValueError::new_err(format!("N-Triples parse error: {e}"))
            })?;
        }
        Ok(PyValidationReport::new(engine::validate(
            &data,
            &self.inner,
        )))
    }

    /// Validate a borrowed oxigraph store against these parsed shapes.
    ///
    /// `data` must be an object (typically `gmeow_validate.ValidationStore`) that
    /// exposes an internal `_store_capsule()` method returning a capsule borrowing
    /// its oxigraph store. This avoids serialising the store to N-Triples for each
    /// validation phase (#634).
    ///
    /// # Errors
    ///
    /// Returns `AttributeError` if `data` has no `_store_capsule` method, and
    /// `ValueError` if the capsule cannot be read.
    fn validate_store(&self, data: &Bound<'_, PyAny>) -> PyResult<PyValidationReport> {
        let capsule = data.call_method0("_store_capsule")?;
        let capsule = capsule.cast::<PyCapsule>()?;
        let ptr = capsule
            .pointer_checked(Some(c"gmeow-validation-store"))
            .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))?;
        let addr = unsafe { *ptr.cast::<usize>().as_ref() };
        let store = unsafe { &*(addr as *const Store) };
        Ok(PyValidationReport::new(engine::validate(
            store,
            &self.inner,
        )))
    }
}

/// A SHACL validation report.
///
/// Wraps the Rust [`ValidationReport`] and exposes `conforms`, the list of
/// result dicts, and a canonical N-Triples serialization.
#[pyclass(name = "ValidationReport")]
pub struct PyValidationReport {
    inner: crate::report::ValidationReport,
}

impl PyValidationReport {
    /// Construct from a Rust [`ValidationReport`].
    pub fn new(inner: crate::report::ValidationReport) -> Self {
        Self { inner }
    }
}

#[pymethods]
impl PyValidationReport {
    #[getter]
    fn conforms(&self) -> bool {
        self.inner.conforms
    }

    #[getter]
    fn results(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        let list = PyList::empty(py);
        for r in &self.inner.results {
            let d = PyDict::new(py);
            d.set_item("focus", r.focus_node.to_string())?;
            d.set_item("path", r.result_path.as_ref().map(|t| t.to_string()))?;
            d.set_item("value", r.value.as_ref().map(|t| t.to_string()))?;
            d.set_item("severity", r.severity.iri())?;
            d.set_item("component", r.source_constraint_component.as_str())?;
            d.set_item("source_shape", r.source_shape.to_string())?;
            d.set_item("message", r.message.clone())?;
            list.append(d)?;
        }
        Ok(list.into_any().unbind())
    }

    /// Serialize the report to canonical N-Triples.
    fn to_ntriples(&self) -> String {
        self.inner.to_ntriples()
    }
}

/// Python extension module `gmeow_shacl`.
///
/// Exposes the legacy `validate(shapes_ttl, data_nt)` function and the reusable
/// `Shapes` / `ValidationReport` wrappers used by the Rust-native orchestration
/// in `gmeow-validate`.
#[pymodule]
fn gmeow_shacl(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(validate, m)?)?;
    m.add_class::<PyShapes>()?;
    m.add_class::<PyValidationReport>()?;
    Ok(())
}
