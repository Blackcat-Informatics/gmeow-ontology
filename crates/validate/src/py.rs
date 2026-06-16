// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! PyO3 Python bindings for `gmeow-validate`.
//!
//! # Engine core separation
//!
//! Only this file imports pyo3. The engine modules ([`crate::store`],
//! [`crate::model`]) are PyO3-free so the rlib links into the future Rust
//! compiler without any Python dependency.
//!
//! # Platform note
//!
//! There is no architecture cfg guard on this module: the crate is native-only
//! by construction (a capability cfg would be optionality, not compliance,
//! #579). pyo3 cannot link into wasm, so the crate is simply never built for
//! wasm.

use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList};

use crate::store;

/// Build the standard `{"errors": [...], "warnings": [...]}` report dict.
fn report_dict(py: Python<'_>, errors: Vec<String>, warnings: Vec<String>) -> PyResult<PyObject> {
    let out = PyDict::new(py);
    out.set_item("errors", PyList::new(py, &errors)?)?;
    out.set_item("warnings", PyList::new(py, &warnings)?)?;
    Ok(out.into())
}

/// Parse every source Turtle file individually to catch syntax errors.
///
/// Mirrors `validate.check_syntax`: on a parse failure for `path`, appends
/// `"syntax error in {path}: {exc}"`. Returns `{"errors": [...], "warnings": []}`.
#[pyfunction]
fn check_syntax(py: Python<'_>, paths: Vec<String>) -> PyResult<PyObject> {
    let mut errors: Vec<String> = Vec::new();
    for path in &paths {
        if let Err(exc) = store::parse_file(std::path::Path::new(path)) {
            errors.push(format!("syntax error in {path}: {exc}"));
        }
    }
    report_dict(py, errors, Vec::new())
}

/// Enforce Principle 5: no `owl:sameAs` merge with external entities.
///
/// Mirrors `validate.check_sameas_ban`. For each `owl:sameAs` triple whose
/// object is a NamedNode NOT starting with `namespace`, unless
/// `(subject_display, object)` is in `allowlist`, appends the exact error string
/// the Python path emits. A file that fails to parse yields a
/// `"failed to parse {path}: {exc}"` error, matching the Python contract.
///
/// Returns `{"errors": [...], "warnings": []}`.
#[pyfunction]
fn check_sameas_ban(
    py: Python<'_>,
    paths: Vec<String>,
    namespace: String,
    allowlist: Vec<(String, String)>,
) -> PyResult<PyObject> {
    if paths.is_empty() {
        return Err(pyo3::exceptions::PyValueError::new_err(
            "check_sameas_ban: paths to audit must not be empty",
        ));
    }

    let mut errors: Vec<String> = Vec::new();
    for path in &paths {
        let quads = match store::parse_file(std::path::Path::new(path)) {
            Ok(q) => q,
            Err(exc) => {
                errors.push(format!("failed to parse {path}: {exc}"));
                continue;
            }
        };
        for (subject_text, obj) in store::sameas_violations(&quads, &namespace, &allowlist) {
            errors.push(format!(
                "{path}: banned owl:sameAs to external entity \
                 {subject_text} owl:sameAs {obj} (Principle 5); \
                 use skos:exactMatch or gmeow:authorityLink"
            ));
        }
    }
    report_dict(py, errors, Vec::new())
}

/// Python extension module `gmeow_validate`.
///
/// Exposes `check_syntax(paths)` and
/// `check_sameas_ban(paths, namespace, allowlist)`.
#[pymodule]
fn gmeow_validate(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(check_syntax, m)?)?;
    m.add_function(wrap_pyfunction!(check_sameas_ban, m)?)?;
    Ok(())
}
