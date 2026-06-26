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
    /// Wrap an engine [`Report`] in the Python `Report` pyclass.
    ///
    /// `pub` so the sibling binding modules (`validate`, `logic`) — now compiled
    /// into the same `gmeow_native` cdylib, so this is the ONE shared `Report`
    /// type — can hand Python the canonical report as a **live object** instead
    /// of a JSON string, eliminating the serialize→`from_json`→`to_json`
    /// round-trip the orchestration layer used to pay (#630, #654).
    pub fn from_engine(inner: Report) -> Self {
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

    /// Project the report into the gmeow: RDF vocabulary as N-Quads, all in the
    /// gmeow:graph/diagnostics named graph (#654).
    fn to_gmeow_rdf(&self) -> String {
        render::to_gmeow_rdf(&self.inner)
    }

    fn render_text(&self) -> String {
        render::to_text(&self.inner)
    }

    /// Render only the advisory (Note/Info) findings as text (#760 F1).
    fn render_advisory_text(&self) -> String {
        render::to_text_advisories(&self.inner)
    }

    /// Write the report's projections to `directory/<stem>.<ext>` and return the
    /// `{kind: path}` map of what was written.
    ///
    /// `kinds` selects which projections to write (`#662` artifact selection).
    /// `None` is a deliberate **maximal default** — write all three, the same as
    /// before this argument existed — not a back-compat shim; the Python facade
    /// always passes an explicit, config-resolved selection. The fixed write
    /// order (`json`, `sarif`, `html`) is preserved regardless of the order in
    /// `kinds`, so output is deterministic. An unknown kind is a hard error
    /// (`ValueError`), never a silent skip.
    #[pyo3(signature = (directory, stem = "gmeow-feedback".to_owned(), kinds = None))]
    fn write_artifacts(
        &self,
        py: Python<'_>,
        directory: String,
        stem: String,
        kinds: Option<Vec<String>>,
    ) -> PyResult<Py<PyAny>> {
        let want =
            kinds.unwrap_or_else(|| vec!["json".to_owned(), "sarif".to_owned(), "html".to_owned()]);
        for kind in &want {
            if !matches!(kind.as_str(), "json" | "sarif" | "html") {
                return Err(pyo3::exceptions::PyValueError::new_err(format!(
                    "unknown artifact kind: {kind:?} (expected json, sarif, or html)"
                )));
            }
        }

        let directory = PathBuf::from(directory);
        fs::create_dir_all(&directory)
            .map_err(|e| pyo3::exceptions::PyOSError::new_err(e.to_string()))?;

        let out = PyDict::new(py);
        // Fixed order, independent of `kinds` ordering, for deterministic writes.
        if want.iter().any(|k| k == "json") {
            let path = directory.join(format!("{stem}.json"));
            fs::write(
                &path,
                render::to_json(&self.inner)
                    .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))?,
            )
            .map_err(|e| pyo3::exceptions::PyOSError::new_err(e.to_string()))?;
            out.set_item("json", path.to_string_lossy().to_string())?;
        }
        if want.iter().any(|k| k == "sarif") {
            let path = directory.join(format!("{stem}.sarif"));
            fs::write(
                &path,
                render::to_sarif(&self.inner)
                    .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))?,
            )
            .map_err(|e| pyo3::exceptions::PyOSError::new_err(e.to_string()))?;
            out.set_item("sarif", path.to_string_lossy().to_string())?;
        }
        if want.iter().any(|k| k == "html") {
            let path = directory.join(format!("{stem}.html"));
            fs::write(&path, render::to_html(&self.inner))
                .map_err(|e| pyo3::exceptions::PyOSError::new_err(e.to_string()))?;
            out.set_item("html", path.to_string_lossy().to_string())?;
        }
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

/// Register the `gmeow-diagnostics` surface on a Python module.
///
/// Called by the unified `gmeow_native` cdylib (#630) to populate the
/// `gmeow_native.diagnostics` submodule; the legacy `import gmeow_diagnostics`
/// name resolves to that same submodule object via a Python shim, so `PyReport`
/// / `PyFinding` are a single shared type across the whole extension.
pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PyFinding>()?;
    m.add_class::<PyReport>()?;
    m.add_function(wrap_pyfunction!(from_legacy, m)?)?;
    Ok(())
}
