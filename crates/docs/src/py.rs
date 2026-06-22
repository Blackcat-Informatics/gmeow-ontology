// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! PyO3 Python bindings for `gmeow-docs`.
//!
//! Task 1 (#853) exposes [`model_json`]: serialize the typed
//! [`DocsModel`](crate::model::DocsModel) built from the slice catalog under
//! `<root>/slices` to a deterministic JSON string.
//!
//! Task 3 (#853) adds [`DocSet`]: the rust-first static-site renderer surface
//! the `gts` bundle generator consumes in place of the legacy Python
//! `ontology_docs.py`. Markdown/HTML/RDF projections and lint are added to this
//! type in later tasks.

use std::fs;
use std::path::{Path, PathBuf};

use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict, PyList};

use crate::model::DocsModel;
use crate::render::{self, Site};
use crate::{lint, rdf};
use gmeow_diagnostics::py::PyReport;

/// Build the documentation model from the repo `root` and return it as a
/// deterministic JSON string.
#[pyfunction]
fn model_json(root: String) -> PyResult<String> {
    let model = DocsModel::discover(Path::new(&root))
        .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))?;
    serde_json::to_string(&model)
        .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))
}

/// A fully rendered ontology-docs static site.
///
/// Wraps the engine [`Site`] (a deterministic, sorted `path -> bytes` tree) and
/// hands it to Python either as an in-memory dict (for tar packing in the `gts`
/// generator) or written deterministically to disk.
#[pyclass(name = "DocSet", skip_from_py_object)]
pub struct DocSet {
    inner: Site,
    /// The typed model the site was rendered from, retained so the RDF
    /// projection and lint surfaces operate on the SAME source of truth the
    /// `files()` tree was built from (no re-discovery).
    model: DocsModel,
}

impl DocSet {
    /// Wrap an engine [`Site`] + its source [`DocsModel`] in the `DocSet`
    /// pyclass.
    pub fn from_engine(inner: Site, model: DocsModel) -> Self {
        Self { inner, model }
    }
}

#[pymethods]
impl DocSet {
    /// Discover the slice catalog under `<root>/slices`, build the docs model,
    /// and render the full static site.
    #[staticmethod]
    fn from_root(root: String) -> PyResult<Self> {
        let model = DocsModel::discover(Path::new(&root))
            .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))?;
        let site = render::render_site(&model);
        Ok(Self::from_engine(site, model))
    }

    /// The full rendered tree as a Python `dict[str, bytes]` (site-relative path
    /// → file bytes), inserted in the engine's sorted `BTreeMap` order.
    fn files(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        let out = PyDict::new(py);
        for (path, data) in &self.inner.files {
            out.set_item(path, PyBytes::new(py, data))?;
        }
        Ok(out.into_any().unbind())
    }

    /// Deterministically write the whole tree under `directory`, creating parent
    /// directories as needed, in fixed sorted order. Returns the list of written
    /// absolute-or-joined paths.
    fn write_artifacts(&self, py: Python<'_>, directory: String) -> PyResult<Py<PyAny>> {
        let directory = PathBuf::from(directory);
        let written = PyList::empty(py);
        // The engine `BTreeMap` is already sorted; iterating it yields a fixed,
        // deterministic write order.
        for (rel, data) in &self.inner.files {
            let path = directory.join(rel);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)
                    .map_err(|e| pyo3::exceptions::PyOSError::new_err(e.to_string()))?;
            }
            fs::write(&path, data)
                .map_err(|e| pyo3::exceptions::PyOSError::new_err(e.to_string()))?;
            written.append(path.to_string_lossy().to_string())?;
        }
        Ok(written.into_any().unbind())
    }

    /// Project the documentation model into the `gmeow:` RDF vocabulary as
    /// deterministic N-Quads in the `gmeow:graph/documentation` named graph.
    ///
    /// This is the in-bundle, SPARQL-queryable form of the docs surface (#853
    /// T5): the `gts` snapshot generator folds it beside the ontology it
    /// describes. A pure function of the retained model.
    fn to_gmeow_rdf(&self) -> String {
        rdf::to_gmeow_rdf(&self.model)
    }

    /// Lint the rendered site + model and return the shared diagnostics
    /// [`Report`](gmeow_diagnostics::py::PyReport) pyclass.
    ///
    /// Errors are integrity defects (dangling links / broken anchors);
    /// warnings are coverage gaps. The caller (`doc-lint`) fails the gate when
    /// `error_count > 0`.
    fn lint(&self) -> PyReport {
        PyReport::from_engine(lint::lint(&self.model, &self.inner))
    }
}

/// Register the `gmeow-docs` surface on a Python module.
///
/// Called by the unified `gmeow_native` cdylib (#630) to populate the
/// `gmeow_native.docs` submodule; the legacy `import gmeow_docs` name resolves
/// to that same submodule object via a Python shim.
pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(model_json, m)?)?;
    m.add_class::<DocSet>()?;
    Ok(())
}
