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
    /// Discover the slice catalog under `<root>/slices`, build the docs model
    /// (including its translation index), and render the full English static
    /// site. Per-language trees are rendered on demand from the retained model
    /// via [`files_for_lang`](Self::files_for_lang).
    #[staticmethod]
    fn from_root(root: String) -> PyResult<Self> {
        let model = DocsModel::discover(Path::new(&root))
            .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))?;
        let site = render::render_site(&model);
        Ok(Self::from_engine(site, model))
    }

    /// The available documentation languages: the English carrier (`"english"`)
    /// first, then the BCP-47 codes of every slice translation catalog (`"fr"`,
    /// `"zh"`), sorted. Deterministic.
    fn languages(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        let out = PyList::empty(py);
        for lang in &self.model.available_languages {
            out.append(lang)?;
        }
        Ok(out.into_any().unbind())
    }

    /// The full rendered English tree as a Python `dict[str, bytes]`
    /// (site-relative path → file bytes), in the engine's sorted `BTreeMap`
    /// order.
    fn files(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        for_lang_dict(py, &self.inner)
    }

    /// The full rendered tree for `lang` as a Python `dict[str, bytes]`.
    ///
    /// `lang` is `"english"` (the carrier, identical to [`files`](Self::files))
    /// or a BCP-47 code present in [`languages`](Self::languages). Every
    /// localizable string is resolved to its translation with English fallback;
    /// the file/path graph is identical across languages. Re-renders the tree
    /// from the retained model on each call (deterministic).
    fn files_for_lang(&self, py: Python<'_>, lang: String) -> PyResult<Py<PyAny>> {
        let site = render::render_site_lang(&self.model, &lang);
        for_lang_dict(py, &site)
    }

    /// The gts archive prefix (the internal `x-gmeow-*` tag) for `lang`, e.g.
    /// `"english"` → `x-gmeow-english`, `"fr"` → `x-gmeow-french`. The bundle
    /// generator folds each language tree under this prefix and the docs
    /// consumer (`create_docs`) selects by it.
    fn archive_prefix(&self, lang: String) -> String {
        if lang == crate::i18n::ENGLISH {
            "x-gmeow-english".to_string()
        } else {
            self.model.translations.internal_tag(&lang)
        }
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

/// Build a Python `dict[str, bytes]` from a rendered [`Site`]'s sorted tree.
fn for_lang_dict(py: Python<'_>, site: &Site) -> PyResult<Py<PyAny>> {
    let out = PyDict::new(py);
    for (path, data) in &site.files {
        out.set_item(path, PyBytes::new(py, data))?;
    }
    Ok(out.into_any().unbind())
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
