// SPDX-FileCopyrightText: 2026 Blackcat Informatics Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! PyO3 bindings for `gmeow-rdf` — the `gmeow_rdf` Python extension module.
//!
//! # Kernel-clean separation
//!
//! Only this module imports pyo3, and it is compiled **only under the `python`
//! feature** (enabled by maturin via `crates/rdf/pyproject.toml`). The gmeow-rdf
//! kernel itself stays PyO3-free so its rlib links into `gmeow-logic`,
//! `gmeow-shacl`, and `gmeow-validate` — and a future pure-Rust toolchain —
//! without pulling a Python dependency (the deliberate kernel-clean design,
//! #648). The `python` feature is the standard maturin extension-module switch,
//! not a degraded-fallback capability cfg: the Rust functionality
//! ([`crate::statements`]) is identical with or without it.

use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;

use crate::{loss, statements};

/// Project the OWL axiom-annotation downcast → the RDF 1.2 / RDF* lead form.
///
/// The native (no Jena, no Docker, no SPARQL) replacement for the Jena codec on
/// the `gmeow regenerate` / `check-generated` statement path.
#[pyfunction]
fn project_statements_rdf12(owl_ttl: &str) -> PyResult<String> {
    statements::project_owl_to_rdf12(owl_ttl).map_err(|e| PyValueError::new_err(e.to_string()))
}

/// Normalize the RDF 1.2 / RDF* lead form → the OWL axiom-annotation normal form.
///
/// Used by the round-trip isomorphism proof (the normal form rdflib can parse).
#[pyfunction]
fn normalize_rdf12_to_owl(rdf12_ttl: &str) -> PyResult<String> {
    statements::normalize_rdf12_to_owl(rdf12_ttl).map_err(|e| PyValueError::new_err(e.to_string()))
}

/// The machine-readable RDF↔GTS loss matrix as deterministic JSON (#819 C0).
///
/// Mirrors the committed `generated/rdf-loss-matrix.json` artifact so Python
/// consumers read the same enumerated, intentional conversion losses the Rust
/// fidelity gate enforces.
#[pyfunction]
fn loss_matrix_json() -> String {
    loss::loss_matrix_json()
}

/// Canonical, review-friendly Turtle serialization over the gmeow-rdf IR (#819
/// Task 9) — the native replacement for rdflib `longturtle` in `gmeow normalize`.
///
/// `extra_prefixes` is the project's standard prefix set; only prefixes the
/// document actually uses appear in the header. The output is a pure function of
/// the graph (RDFC-aware blank handling), so re-running is idempotent.
#[pyfunction]
#[pyo3(signature = (turtle_bytes, extra_prefixes=Vec::new()))]
fn canonicalize_turtle(
    turtle_bytes: &[u8],
    extra_prefixes: Vec<(String, String)>,
) -> PyResult<Vec<u8>> {
    crate::turtle_normalize::canonical_turtle(turtle_bytes, &extra_prefixes)
        .map(String::into_bytes)
        .map_err(PyValueError::new_err)
}

/// Register the `gmeow-rdf` surface on a Python module.
///
/// Called by the unified `gmeow_native` cdylib (#630) to populate the
/// `gmeow_native.rdf` submodule; the legacy `import gmeow_rdf` name resolves to
/// that same submodule object via a Python shim.
pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(project_statements_rdf12, m)?)?;
    m.add_function(wrap_pyfunction!(normalize_rdf12_to_owl, m)?)?;
    m.add_function(wrap_pyfunction!(loss_matrix_json, m)?)?;
    m.add_function(wrap_pyfunction!(canonicalize_turtle, m)?)?;
    // The native oxigraph Store / SPARQL / parse / canonicalize surface that
    // replaces the external `pyoxigraph` package (#667).
    crate::py_store::register(m)?;
    // The native RDF → GTS producer surface (snapshot author + compile_gts) and
    // the `PyRdfDataset` Arc handle (#819 Task 8 / C7).
    crate::py_gts::register(m)?;
    // The native SSSOM codec surface (parse + validate + RDF serialize) that
    // replaces the external `sssom` package on the mapping-compile path (#848).
    crate::py_sssom::register(m)?;
    Ok(())
}
