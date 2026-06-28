// SPDX-FileCopyrightText: 2026 Blackcat Informatics Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! PyO3 surface for the Rust music-package toolchain.

use std::path::Path;

use pyo3::prelude::*;
use pyo3::types::PyModule;
use pyo3::wrap_pyfunction;

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
        .map_err(pyo3::exceptions::PyValueError::new_err)
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
        .map_err(pyo3::exceptions::PyValueError::new_err)
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
