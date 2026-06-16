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

use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList};

use crate::engine;

/// Validate a data graph (N-Triples) against a shapes graph (Turtle).
///
/// Returns a dict with keys:
/// - `"conforms"` — bool
/// - `"results"` — list of dicts, each with keys:
///   `"focus"`, `"path"`, `"value"`, `"severity"`, `"component"`,
///   `"source_shape"`, `"message"`.
#[pyfunction]
fn validate(py: Python<'_>, shapes_ttl: &str, data_nt: &str) -> PyResult<PyObject> {
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

    Ok(out.into())
}

/// Python extension module `gmeow_shacl`.
///
/// Exposes the `validate(shapes_ttl, data_nt)` function.
#[pymodule]
fn gmeow_shacl(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(validate, m)?)?;
    Ok(())
}
