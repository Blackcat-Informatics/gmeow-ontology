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

use crate::statements;

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

/// The `gmeow_rdf` extension module.
#[pymodule]
fn gmeow_rdf(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(project_statements_rdf12, m)?)?;
    m.add_function(wrap_pyfunction!(normalize_rdf12_to_owl, m)?)?;
    Ok(())
}
