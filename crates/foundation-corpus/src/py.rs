// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! PyO3 binding for the foundation-corpus importer — the
//! `gmeow_native.foundation` submodule (#944).
//!
//! Only this module imports `pyo3`, gated by the `python` feature, so the engine
//! core stays PyO3-free. [`import_foundation`] is the single Python surface that
//! replaces the retired `gmeow_tools.foundation_import`: it runs the full import
//! into `out_dir` and returns the budget-report text (the CLI prints it).

use std::path::Path;

use pyo3::prelude::*;
use pyo3::types::PyModule;
use pyo3::wrap_pyfunction;

/// Import the foundation corpus, writing foundation.ttl + budget-report.txt +
/// the six projections (+ optional .nq reconciliation) into `out_dir`. Returns
/// the budget-report text (the CLI prints it). A run_import I/O error maps to
/// ValueError.
#[pyfunction]
#[pyo3(signature = (jsonl, out_dir, nq=None))]
fn import_foundation(jsonl: String, out_dir: String, nq: Option<String>) -> PyResult<String> {
    let nq_path = nq.as_deref().map(Path::new);
    let (_dataset, budget) = crate::run_import(Path::new(&jsonl), Path::new(&out_dir), nq_path)
        .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))?;
    Ok(budget.as_text())
}

/// Register the `gmeow_native.foundation` submodule. Called by the unified
/// `gmeow_native` cdylib (#630); exposes [`import_foundation`].
pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(import_foundation, m)?)?;
    Ok(())
}
