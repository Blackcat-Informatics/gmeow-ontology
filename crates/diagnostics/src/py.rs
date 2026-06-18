// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! PyO3 Python bindings for `gmeow-diagnostics`.

use std::fs;
use std::path::PathBuf;

use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList};

use crate::model::{Finding, Location, Report, Severity};
use crate::render;

#[pyclass(name = "Finding", skip_from_py_object)]
#[derive(Clone)]
pub struct PyFinding {
    inner: Finding,
}

impl PyFinding {
    fn from_engine(inner: Finding) -> Self {
        Self { inner }
    }
}

#[pymethods]
impl PyFinding {
    #[new]
    #[pyo3(signature = (
        severity,
        code,
        message,
        tool = None,
        path = None,
        line = None,
        column = None,
        logical = None,
        detail = None,
        tags = None,
        suggestions = None,
    ))]
    #[allow(clippy::too_many_arguments)]
    fn new(
        severity: String,
        code: String,
        message: String,
        tool: Option<String>,
        path: Option<String>,
        line: Option<u32>,
        column: Option<u32>,
        logical: Option<String>,
        detail: Option<String>,
        tags: Option<Vec<String>>,
        suggestions: Option<Vec<String>>,
    ) -> PyResult<Self> {
        let severity =
            Severity::parse(&severity).map_err(pyo3::exceptions::PyValueError::new_err)?;
        let mut finding = Finding::new(severity, code, message);
        finding.tool = tool;
        finding.detail = detail;
        finding.tags = tags.unwrap_or_default();
        finding.suggestions = suggestions.unwrap_or_default();
        finding.add_location(Location::new(path, line, column, logical));
        Ok(Self::from_engine(finding))
    }

    #[getter]
    fn severity(&self) -> &'static str {
        self.inner.severity.as_str()
    }

    #[getter]
    fn code(&self) -> &str {
        &self.inner.code
    }

    #[getter]
    fn message(&self) -> &str {
        &self.inner.message
    }

    fn to_dict(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        finding_to_dict(py, &self.inner)
    }
}

#[pyclass(name = "Report", skip_from_py_object)]
#[derive(Clone)]
pub struct PyReport {
    inner: Report,
}

impl PyReport {
    fn from_engine(inner: Report) -> Self {
        Self { inner }
    }
}

#[pymethods]
impl PyReport {
    #[new]
    #[pyo3(signature = (tool = "gmeow".to_owned()))]
    fn new(tool: String) -> Self {
        Self::from_engine(Report::new(tool))
    }

    fn add(&mut self, finding: PyRef<'_, PyFinding>) {
        self.inner.add_finding(finding.inner.clone());
    }

    fn extend(&mut self, other: PyRef<'_, PyReport>) {
        for finding in &other.inner.findings {
            self.inner.add_finding(finding.clone());
        }
        for rule in &other.inner.rules {
            self.inner.add_rule(rule.clone());
        }
        for (key, value) in &other.inner.metadata {
            self.inner.metadata.insert(key.clone(), value.clone());
        }
    }

    fn set_metadata_json(&mut self, key: String, value_json: String) -> PyResult<()> {
        let value = serde_json::from_str(&value_json)
            .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))?;
        self.inner.metadata.insert(key, value);
        Ok(())
    }

    #[getter]
    fn tool(&self) -> &str {
        &self.inner.tool
    }

    #[getter]
    fn ok(&self) -> bool {
        self.inner.ok()
    }

    #[getter]
    fn error_count(&self) -> usize {
        self.inner.error_count()
    }

    #[getter]
    fn warning_count(&self) -> usize {
        self.inner.warning_count()
    }

    #[getter]
    fn finding_count(&self) -> usize {
        self.inner.findings.len()
    }

    #[getter]
    fn errors(&self) -> Vec<String> {
        self.inner.legacy_errors()
    }

    #[getter]
    fn warnings(&self) -> Vec<String> {
        self.inner.legacy_warnings()
    }

    #[getter]
    fn findings(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        let list = PyList::empty(py);
        for finding in &self.inner.normalized().findings {
            list.append(finding_to_dict(py, finding)?)?;
        }
        Ok(list.into_any().unbind())
    }

    fn to_json(&self) -> PyResult<String> {
        render::to_json(&self.inner)
            .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))
    }

    fn to_sarif(&self) -> PyResult<String> {
        render::to_sarif(&self.inner)
            .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))
    }

    fn to_html(&self) -> String {
        render::to_html(&self.inner)
    }

    fn render_text(&self) -> String {
        render::to_text(&self.inner)
    }

    #[pyo3(signature = (directory, stem = "gmeow-feedback".to_owned()))]
    fn write_artifacts(
        &self,
        py: Python<'_>,
        directory: String,
        stem: String,
    ) -> PyResult<Py<PyAny>> {
        let directory = PathBuf::from(directory);
        fs::create_dir_all(&directory)
            .map_err(|e| pyo3::exceptions::PyOSError::new_err(e.to_string()))?;

        let json_path = directory.join(format!("{stem}.json"));
        let sarif_path = directory.join(format!("{stem}.sarif"));
        let html_path = directory.join(format!("{stem}.html"));

        fs::write(
            &json_path,
            render::to_json(&self.inner)
                .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))?,
        )
        .map_err(|e| pyo3::exceptions::PyOSError::new_err(e.to_string()))?;
        fs::write(
            &sarif_path,
            render::to_sarif(&self.inner)
                .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))?,
        )
        .map_err(|e| pyo3::exceptions::PyOSError::new_err(e.to_string()))?;
        fs::write(&html_path, render::to_html(&self.inner))
            .map_err(|e| pyo3::exceptions::PyOSError::new_err(e.to_string()))?;

        let out = PyDict::new(py);
        out.set_item("json", json_path.to_string_lossy().to_string())?;
        out.set_item("sarif", sarif_path.to_string_lossy().to_string())?;
        out.set_item("html", html_path.to_string_lossy().to_string())?;
        Ok(out.into_any().unbind())
    }
}

#[pyfunction]
fn from_legacy(tool: String, errors: Vec<String>, warnings: Vec<String>) -> PyReport {
    PyReport::from_engine(Report::from_legacy(tool, errors, warnings))
}

fn finding_to_dict(py: Python<'_>, finding: &Finding) -> PyResult<Py<PyAny>> {
    let out = PyDict::new(py);
    out.set_item("severity", finding.severity.as_str())?;
    out.set_item("code", &finding.code)?;
    out.set_item("message", &finding.message)?;
    out.set_item("tool", finding.tool.clone())?;
    if let Some(detail) = &finding.detail {
        out.set_item("detail", detail)?;
    }
    let locations = PyList::empty(py);
    for location in &finding.locations {
        let d = PyDict::new(py);
        d.set_item("path", location.path.clone())?;
        d.set_item("line", location.line)?;
        d.set_item("column", location.column)?;
        d.set_item("logical", location.logical.clone())?;
        locations.append(d)?;
    }
    out.set_item("locations", locations)?;
    out.set_item("tags", finding.tags.clone())?;
    out.set_item("suggestions", finding.suggestions.clone())?;
    Ok(out.into_any().unbind())
}

/// Python extension module `gmeow_diagnostics`.
#[pymodule]
fn gmeow_diagnostics(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PyFinding>()?;
    m.add_class::<PyReport>()?;
    m.add_function(wrap_pyfunction!(from_legacy, m)?)?;
    Ok(())
}
