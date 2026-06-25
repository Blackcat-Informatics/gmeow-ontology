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
use pyo3::types::{PyBytes, PyDict, PyList, PyModule};

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

/// Serialize N-Quads-star bytes to RDF-1.2-star JSON-LD or YAML-LD-star.
///
/// * `nquads_bytes` — a UTF-8 N-Quads-star document (plain N-Quads is accepted).
/// * `format` — `"jsonld"` for JSON-LD-star, `"yamlld"` for YAML-LD-star.
///
/// Returns the serialized bytes. This is the Python surface for the serializer
/// used by the `stage-export-yaml-ld` leaf (#699).
#[pyfunction]
#[pyo3(signature = (nquads_bytes, format = "jsonld"))]
fn serialize_yaml_ld(py: Python<'_>, nquads_bytes: &[u8], format: &str) -> PyResult<Py<PyAny>> {
    let gts =
        gmeow_gts::from_nquads::from_nquads(std::str::from_utf8(nquads_bytes).map_err(|e| {
            pyo3::exceptions::PyValueError::new_err(format!("N-Quads bytes are not UTF-8: {e}"))
        })?)
        .map_err(|e| pyo3::exceptions::PyValueError::new_err(format!("parse N-Quads: {e}")))?;
    let graph = gmeow_rdf::gts::read_graph(&gts, true)
        .map_err(|e| pyo3::exceptions::PyValueError::new_err(format!("read GTS graph: {e}")))?;
    let text = match format {
        "jsonld" => crate::stages::yaml_ld::serialize_graph(&graph)
            .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))?,
        "yamlld" => crate::stages::yaml_ld::serialize_graph_yaml(&graph, None)
            .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))?,
        _ => {
            return Err(pyo3::exceptions::PyValueError::new_err(format!(
                "unknown format {format:?}; expected 'jsonld' or 'yamlld'"
            )))
        }
    };
    Ok(PyBytes::new(py, text.as_bytes()).into_any().unbind())
}

/// Parse JSON-LD-star bytes and downcast RDF 1.2 quoted triples to GMEOW
/// statement-metadata N-Quads.
///
/// The GMEOW JSON-LD-star emitter represents statement metadata with the
/// `@annotation` idiom, which parses to `?r rdf:reifies <<( ?s ?p ?o )>>`
/// plus annotation triples on `?r`. Those quoted triples cannot be carried
/// through the rdflib-compat up-projection lane, so this function re-expresses
/// each annotation as a native GMEOW statement-metadata cell:
///
/// ```turtle
/// ?r a gmeow:StatementMetadata ;
///    gmeow:qSubject ?s ;
///    gmeow:qPredicate ?p ;
///    gmeow:qObject ?o | gmeow:qObjectLiteral ?o ;
///    <annotation-pred> <annotation-value> .
/// ```
///
/// Returns UTF-8 N-Quads bytes with no quoted triple terms. Hard-fails on
/// unsupported JSON-LD features.
#[pyfunction]
#[pyo3(signature = (json_bytes))]
fn parse_jsonld_star_to_gmeow_statement_metadata_nquads(
    py: Python<'_>,
    json_bytes: &[u8],
) -> PyResult<Py<PyAny>> {
    let nquads = crate::stages::yaml_ld::jsonld_star_to_gmeow_statement_metadata_nquads(json_bytes)
        .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))?;
    Ok(PyBytes::new(py, nquads.as_bytes()).into_any().unbind())
}

/// Parse YAML-LD-star bytes and downcast RDF 1.2 quoted triples to GMEOW
/// statement-metadata N-Quads.
///
/// Routes the YAML-LD-star document through the Rust native JSON-LD-star
/// downcast (anchors/aliases hard-fail), so the rdflib-compat up-projection lane
/// receives quoted-triple-free N-Quads (#699). The Python YAML codec is retired
/// in favor of this single Rust authority.
#[pyfunction]
#[pyo3(signature = (yaml_bytes))]
fn parse_yaml_ld_star_to_gmeow_statement_metadata_nquads(
    py: Python<'_>,
    yaml_bytes: &[u8],
) -> PyResult<Py<PyAny>> {
    let nquads =
        crate::stages::yaml_ld::yaml_ld_star_to_gmeow_statement_metadata_nquads(yaml_bytes)
            .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))?;
    Ok(PyBytes::new(py, nquads.as_bytes()).into_any().unbind())
}

/// Verify a serialized RDF-1.2-star document round-trips isomorphic to its
/// source N-Quads-star.
///
/// * `nquads_bytes` — the original UTF-8 N-Quads-star document.
/// * `star_bytes` — the serialized RDF-1.2-star bytes to verify.
/// * `format` — `"jsonld"` for JSON-LD-star, `"yamlld"` for YAML-LD-star.
///
/// Returns `True` iff the re-parsed dataset is RDFC-1.0 canonical-equal to the
/// original. This is the Rust authority for the build-time serialization
/// isomorphism gate (#699), replacing the Python `_round_trip_star`.
#[pyfunction]
#[pyo3(signature = (nquads_bytes, star_bytes, format))]
fn roundtrip_isomorphic(nquads_bytes: &[u8], star_bytes: &[u8], format: &str) -> PyResult<bool> {
    crate::stages::yaml_ld::roundtrip_isomorphic(nquads_bytes, star_bytes, format)
        .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))
}

/// Register the `gmeow_native.pipeline` submodule. Called by the unified
/// `gmeow_native` cdylib (#630); exposes [`run_pipeline`].
pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(run_pipeline, m)?)?;
    m.add_function(wrap_pyfunction!(compile_statements, m)?)?;
    m.add_function(wrap_pyfunction!(compile_statements_report, m)?)?;
    m.add_function(wrap_pyfunction!(compile_mappings_report, m)?)?;
    m.add_function(wrap_pyfunction!(serialize_yaml_ld, m)?)?;
    m.add_function(wrap_pyfunction!(
        parse_jsonld_star_to_gmeow_statement_metadata_nquads,
        m
    )?)?;
    m.add_function(wrap_pyfunction!(
        parse_yaml_ld_star_to_gmeow_statement_metadata_nquads,
        m
    )?)?;
    m.add_function(wrap_pyfunction!(roundtrip_isomorphic, m)?)?;
    Ok(())
}
