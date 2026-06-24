// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! PyO3 bindings for the pipeline — the `gmeow_native.pipeline` submodule (#861).
//!
//! Only this module imports `pyo3`, gated by the `python` feature, so the engine
//! core stays PyO3-free. [`run_pipeline`] is the single Python surface that
//! replaces the Python build orchestrator: it runs the WHOLE dogfooded DAG
//! single-pass (`crate::run::run_full`) and either WRITES every committed
//! artifact (regenerate) or COMPARES each against the committed bytes and reports
//! drift (check).

use std::path::Path;

use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList, PyModule};

use gmeow_diagnostics::py::PyReport;

use crate::run::{run_full, RunMode};

/// Run the full dogfooded build single-pass.
///
/// * `root` — the repository root.
/// * `jobs` — per-level parallelism budget (clamped to `>= 1` internally).
/// * `check` — when `true`, COMPARE each produced artifact against the committed
///   bytes and report drift (no writes); when `false`, WRITE every produced
///   artifact to disk (regenerate).
///
/// Returns a summary `dict`:
///
/// ```text
/// {
///   "mode":       "check" | "regenerate",
///   "produced":   int,        # committed-artifact paths the run produced
///   "reproduced": int,        # reproduced byte/iso-for-byte (check) / written
///   "drifted":    list[str],  # drifted committed paths (check); empty on regen
///   "findings":   list[{severity, code, message}],  # drift / write findings
///   "clean":      bool,       # True ⇒ zero drift, full parity
/// }
/// ```
///
/// In CHECK mode the caller fails the gate when `drifted` is non-empty (or
/// `clean` is `False`). A [`crate::error::PipelineError`] (a hard build failure —
/// a malformed DAG, an unknown stage impl, an I/O error) maps to `ValueError`.
#[pyfunction]
#[pyo3(signature = (root, jobs, check))]
fn run_pipeline(py: Python<'_>, root: String, jobs: usize, check: bool) -> PyResult<Py<PyAny>> {
    let mode = if check {
        RunMode::Check
    } else {
        RunMode::Regenerate
    };
    let report = run_full(Path::new(&root), jobs, mode)
        .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))?;

    let out = PyDict::new(py);
    out.set_item(
        "mode",
        match report.mode {
            RunMode::Check => "check",
            RunMode::Regenerate => "regenerate",
        },
    )?;
    out.set_item("produced", report.produced)?;
    out.set_item("reproduced", report.reproduced)?;
    out.set_item("clean", report.is_clean())?;

    let drifted = PyList::empty(py);
    for path in &report.drifted {
        drifted.append(path)?;
    }
    out.set_item("drifted", drifted)?;

    let findings = PyList::empty(py);
    for finding in &report.findings {
        let f = PyDict::new(py);
        f.set_item("severity", finding.severity.as_str())?;
        f.set_item("code", &finding.code)?;
        f.set_item("message", &finding.message)?;
        findings.append(f)?;
    }
    out.set_item("findings", findings)?;

    Ok(out.into_any().unbind())
}

/// Compile only the statement layer through the native Rust statements stage.
///
/// This is an interface hook for developer feedback and oracle checks. The
/// compiler authority remains [`crate::stages::statements::compile_statements`];
/// Python receives the already-rendered OWL downcast and RDF 1.2 lead strings.
#[pyfunction]
#[pyo3(signature = (root))]
fn compile_statements(py: Python<'_>, root: String) -> PyResult<Py<PyAny>> {
    let (owl_ttl, rdf12_ttl) = crate::stages::statements::compile_statements(Path::new(&root))
        .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))?;

    let out = PyDict::new(py);
    out.set_item("owl_ttl", owl_ttl)?;
    out.set_item("rdf12_ttl", rdf12_ttl)?;
    Ok(out.into_any().unbind())
}

/// Compile statements and return the structured feedback diagnostics report.
///
/// Python supplies `ontology_nt` because it already owns the merged-ontology
/// loading surface; the compiler, invariant checks, lossless check, and
/// `statement-compile.dsl-error` mapping remain Rust-owned.
#[pyfunction]
#[pyo3(signature = (root, ontology_nt))]
fn compile_statements_report(
    py: Python<'_>,
    root: String,
    ontology_nt: String,
) -> PyResult<Py<PyAny>> {
    let report =
        crate::stages::statements::compile_diagnostics_report(Path::new(&root), &ontology_nt);
    Ok(Py::new(py, PyReport::from_engine(report))?.into_any())
}

/// Compile mappings and return the structured feedback diagnostics report.
///
/// The compiler, SSSOM validation, and projection linting remain Rust-owned.
/// Python receives only the canonical report object for CLI/SARIF/HTML folding.
#[pyfunction]
#[pyo3(signature = (root))]
fn compile_mappings_report(py: Python<'_>, root: String) -> PyResult<Py<PyAny>> {
    let report = crate::stages::mappings::compile_diagnostics_report(Path::new(&root));
    Ok(Py::new(py, PyReport::from_engine(report))?.into_any())
}

/// Register the `gmeow_native.pipeline` submodule. Called by the unified
/// `gmeow_native` cdylib (#630); exposes [`run_pipeline`].
pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(run_pipeline, m)?)?;
    m.add_function(wrap_pyfunction!(compile_statements, m)?)?;
    m.add_function(wrap_pyfunction!(compile_statements_report, m)?)?;
    m.add_function(wrap_pyfunction!(compile_mappings_report, m)?)?;
    Ok(())
}
