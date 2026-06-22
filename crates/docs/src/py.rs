// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! PyO3 Python bindings for `gmeow-docs`.
//!
//! Task 1 (#853) exposes a single entry point: serialize the typed
//! [`DocsModel`](crate::model::DocsModel) built from the slice catalog under
//! `<root>/slices` to a deterministic JSON string. Renderers (Markdown/HTML),
//! lint, and bundle wiring are added to this submodule in later tasks.

use std::path::Path;

use pyo3::prelude::*;

use crate::model::DocsModel;

/// Build the documentation model from the repo `root` and return it as a
/// deterministic JSON string.
#[pyfunction]
fn model_json(root: String) -> PyResult<String> {
    let model = DocsModel::discover(Path::new(&root))
        .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))?;
    serde_json::to_string(&model)
        .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))
}

/// Register the `gmeow-docs` surface on a Python module.
///
/// Called by the unified `gmeow_native` cdylib (#630) to populate the
/// `gmeow_native.docs` submodule; the legacy `import gmeow_docs` name resolves
/// to that same submodule object via a Python shim.
pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(model_json, m)?)?;
    Ok(())
}
