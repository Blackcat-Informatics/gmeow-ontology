// SPDX-FileCopyrightText: 2026 Blackcat Informatics Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! PyO3 surface for the Rust music-package toolchain.

use std::path::Path;

use pyo3::prelude::*;
use pyo3::types::PyModule;
use pyo3::wrap_pyfunction;

fn music_error(err: String) -> PyErr {
    use gmeow_errors::{Diag, FindingCategory, Grade, Severity, Standpoint};
    // Funnel through the ONE `Diag` → `PyErr` bridge, preserving the exception
    // CLASS each case has always surfaced. A filesystem failure is carried as a
    // live `std::io::Error` so the bridge keeps the `OSError` class every I/O
    // failure has surfaced; every other failure is a value/data problem
    // (`DataShapeViolation` → `ValueError`).
    let diag = if err.starts_with("failed to read ")
        || err.starts_with("failed to write ")
        || err.starts_with("failed to create ")
    {
        Diag::from(std::io::Error::other(err))
    } else {
        Diag::new(
            gmeow_errors::code::foreign_code(),
            Grade::new(
                Severity::Error,
                FindingCategory::DataShapeViolation,
                Standpoint::Binding,
            ),
            err,
        )
    };
    gmeow_errors::py::diag_to_pyerr(&diag)
}

#[pyfunction]
pub fn list_formats() -> Vec<&'static str> {
    crate::list_formats()
}

#[pyfunction]
pub fn render_file(source: &str, to: &str, out: &str) -> PyResult<Vec<String>> {
    crate::render_file(Path::new(source), to, Path::new(out))
        .map(|paths| {
            paths
                .into_iter()
                .map(|path| path.display().to_string())
                .collect()
        })
        .map_err(music_error)
}

#[pyfunction(name = "import_file")]
pub fn import_file_py(source: &str, out: &str) -> PyResult<Vec<String>> {
    crate::import_file(Path::new(source), Path::new(out))
        .map(|paths| {
            paths
                .into_iter()
                .map(|path| path.display().to_string())
                .collect()
        })
        .map_err(music_error)
}

#[pyfunction]
pub fn manifest_turtle(format_name: &str, provenance: Option<&str>) -> PyResult<String> {
    crate::manifest_turtle(format_name, provenance).map_err(pyo3::exceptions::PyValueError::new_err)
}

pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(list_formats, m)?)?;
    m.add_function(wrap_pyfunction!(render_file, m)?)?;
    m.add_function(wrap_pyfunction!(import_file_py, m)?)?;
    m.add_function(wrap_pyfunction!(manifest_turtle, m)?)?;
    Ok(())
}
