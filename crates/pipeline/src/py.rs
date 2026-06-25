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
use crate::up_projection::{self, AuditReport, LiftMap, UpProjectionReport};

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

/// Classify one SSSOM row for the native up-projection audit.
#[pyfunction]
#[pyo3(signature = (subject_id, predicate_id, object_id))]
fn up_projection_classify_sssom(
    subject_id: String,
    predicate_id: String,
    object_id: String,
) -> PyResult<(String, String, String)> {
    let class = up_projection::classify_sssom(&subject_id, &predicate_id, &object_id);
    Ok((class.bucket, class.gmeow, class.target))
}

/// Compute the best combined up-projection class for one target term.
#[pyfunction]
#[pyo3(signature = (term, sssom, structural))]
fn up_projection_combined_class(
    term: String,
    sssom: std::collections::BTreeMap<String, String>,
    structural: std::collections::BTreeMap<String, String>,
) -> PyResult<String> {
    Ok(up_projection::combined_class(&term, &sssom, &structural))
}

/// Build the native lift map from serialized SSSOM, projection TTL, and ontology NT.
#[pyfunction]
#[pyo3(signature = (sssom_texts, projection_ttls, ontology_nt))]
fn up_projection_build_lift_map(
    py: Python<'_>,
    sssom_texts: Vec<String>,
    projection_ttls: Vec<String>,
    ontology_nt: String,
) -> PyResult<Py<PyAny>> {
    let lift = py
        .detach(move || up_projection::build_lift_map(&sssom_texts, &projection_ttls, &ontology_nt))
        .map_err(pyo3::exceptions::PyValueError::new_err)?;
    lift_map_to_py(py, &lift)
}

/// Up-project N-Triples through the native Rust kernel.
#[pyfunction]
#[pyo3(signature = (source_nt, sssom_texts, projection_ttls, ontology_nt, descend = false))]
fn up_projection_project_nt(
    py: Python<'_>,
    source_nt: String,
    sssom_texts: Vec<String>,
    projection_ttls: Vec<String>,
    ontology_nt: String,
    descend: bool,
) -> PyResult<Py<PyAny>> {
    let report = py
        .detach(move || {
            up_projection::up_project_nt(
                &source_nt,
                &sssom_texts,
                &projection_ttls,
                &ontology_nt,
                descend,
            )
        })
        .map_err(pyo3::exceptions::PyValueError::new_err)?;
    up_projection_report_to_py(py, &report)
}

/// Run the native up-projection invertibility audit over serialized corpus graphs.
#[pyfunction]
#[pyo3(signature = (sssom_texts, projection_ttls, corpus_nts))]
fn up_projection_audit_nt(
    py: Python<'_>,
    sssom_texts: Vec<String>,
    projection_ttls: Vec<String>,
    corpus_nts: Vec<(String, String)>,
) -> PyResult<Py<PyAny>> {
    let report = py
        .detach(move || up_projection::run_audit_nt(&sssom_texts, &projection_ttls, &corpus_nts))
        .map_err(pyo3::exceptions::PyValueError::new_err)?;
    audit_report_to_py(py, &report)
}

/// Resolve one context-aware up-projection candidate through the native descent index.
#[pyfunction]
#[pyo3(signature = (predicate, subject_types, sssom_texts, projection_ttls, ontology_nt))]
fn up_projection_resolve_context(
    py: Python<'_>,
    predicate: String,
    subject_types: Vec<String>,
    sssom_texts: Vec<String>,
    projection_ttls: Vec<String>,
    ontology_nt: String,
) -> PyResult<Py<PyAny>> {
    let subject_types: std::collections::BTreeSet<String> = subject_types.into_iter().collect();
    let resolved = py
        .detach(move || {
            up_projection::resolve_context_candidate(
                &predicate,
                &subject_types,
                &sssom_texts,
                &projection_ttls,
                &ontology_nt,
            )
        })
        .map_err(pyo3::exceptions::PyValueError::new_err)?;
    let Some((gmeow, context_type, relation, confidence)) = resolved else {
        return Ok(py.None());
    };
    let out = PyDict::new(py);
    out.set_item("gmeow", gmeow)?;
    out.set_item("context_type", context_type)?;
    out.set_item("relation", relation)?;
    out.set_item("confidence", confidence)?;
    Ok(out.into_any().unbind())
}

/// Run only the hand-authored/native reverse-projection minting layer.
#[pyfunction]
#[pyo3(signature = (source_nt))]
fn up_projection_reverse_nt(py: Python<'_>, source_nt: String) -> PyResult<Py<PyAny>> {
    let graph_nt = py
        .detach(move || up_projection::reverse_nt(&source_nt))
        .map_err(pyo3::exceptions::PyValueError::new_err)?;
    Ok(PyBytes::new(py, graph_nt.as_bytes()).into_any().unbind())
}

fn lift_map_to_py(py: Python<'_>, lift: &LiftMap) -> PyResult<Py<PyAny>> {
    let out = PyDict::new(py);
    out.set_item("rules", string_map(py, &lift.rules)?)?;
    out.set_item("ambiguous", string_set_map(py, &lift.ambiguous)?)?;
    out.set_item("inverse_rules", string_map(py, &lift.inverse_rules)?)?;
    out.set_item("claim_rules", tuple_map(py, &lift.claim_rules)?)?;
    out.set_item(
        "object_properties",
        PyList::new(py, lift.object_properties.iter())?,
    )?;

    let value_rules = PyList::empty(py);
    for ((source_predicate, source_value), (gmeow_predicate, gmeow_value)) in &lift.value_rules {
        let row = PyDict::new(py);
        row.set_item("source_predicate", source_predicate)?;
        row.set_item("source_value", source_value)?;
        row.set_item("gmeow_predicate", gmeow_predicate)?;
        row.set_item("gmeow_value", gmeow_value)?;
        value_rules.append(row)?;
    }
    out.set_item("value_rules", value_rules)?;
    Ok(out.into_any().unbind())
}

fn up_projection_report_to_py(py: Python<'_>, report: &UpProjectionReport) -> PyResult<Py<PyAny>> {
    let out = PyDict::new(py);
    out.set_item("graph_nt", &report.graph_nt)?;
    out.set_item("lifted", report.lifted)?;
    out.set_item("claimed", report.claimed)?;
    out.set_item("gap_terms", usize_map(py, &report.gap_terms)?)?;
    out.set_item("ambiguous_terms", usize_map(py, &report.ambiguous_terms)?)?;
    out.set_item("claim_terms", usize_map(py, &report.claim_terms)?)?;
    out.set_item("context_resolved", report.context_resolved)?;
    out.set_item("context_terms", usize_map(py, &report.context_terms)?)?;
    out.set_item("tag_resolved", report.tag_resolved)?;
    out.set_item(
        "tag_resolved_terms",
        usize_map(py, &report.tag_resolved_terms)?,
    )?;
    out.set_item("minted", report.minted)?;
    Ok(out.into_any().unbind())
}

fn audit_report_to_py(py: Python<'_>, report: &AuditReport) -> PyResult<Py<PyAny>> {
    let out = PyDict::new(py);
    let files = PyList::empty(py);
    for file in &report.files {
        let f = PyDict::new(py);
        f.set_item("name", &file.name)?;
        f.set_item("per_term", string_map(py, &file.per_term)?)?;
        let per_vocab = PyDict::new(py);
        for (vocab, counts) in &file.per_vocab {
            per_vocab.set_item(vocab, usize_map(py, counts)?)?;
        }
        f.set_item("per_vocab", per_vocab)?;
        f.set_item("liftable", file.liftable())?;
        f.set_item("total", file.total())?;
        files.append(f)?;
    }
    out.set_item("files", files)?;
    out.set_item("gaps", PyList::new(py, report.gaps.iter())?)?;
    out.set_item("sssom_total", report.sssom_total)?;
    out.set_item("struct_total", report.struct_total)?;
    out.set_item("liftable", report.liftable())?;
    out.set_item("total", report.total())?;
    Ok(out.into_any().unbind())
}

fn string_map(
    py: Python<'_>,
    map: &std::collections::BTreeMap<String, String>,
) -> PyResult<Py<PyAny>> {
    let out = PyDict::new(py);
    for (key, value) in map {
        out.set_item(key, value)?;
    }
    Ok(out.into_any().unbind())
}

fn tuple_map(
    py: Python<'_>,
    map: &std::collections::BTreeMap<String, (String, String)>,
) -> PyResult<Py<PyAny>> {
    let out = PyDict::new(py);
    for (key, (left, right)) in map {
        out.set_item(key, (left, right))?;
    }
    Ok(out.into_any().unbind())
}

fn string_set_map(
    py: Python<'_>,
    map: &std::collections::BTreeMap<String, std::collections::BTreeSet<String>>,
) -> PyResult<Py<PyAny>> {
    let out = PyDict::new(py);
    for (key, values) in map {
        out.set_item(key, PyList::new(py, values.iter())?)?;
    }
    Ok(out.into_any().unbind())
}

fn usize_map(
    py: Python<'_>,
    map: &std::collections::BTreeMap<String, usize>,
) -> PyResult<Py<PyAny>> {
    let out = PyDict::new(py);
    for (key, value) in map {
        out.set_item(key, *value)?;
    }
    Ok(out.into_any().unbind())
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
    m.add_function(wrap_pyfunction!(up_projection_classify_sssom, m)?)?;
    m.add_function(wrap_pyfunction!(up_projection_combined_class, m)?)?;
    m.add_function(wrap_pyfunction!(up_projection_build_lift_map, m)?)?;
    m.add_function(wrap_pyfunction!(up_projection_project_nt, m)?)?;
    m.add_function(wrap_pyfunction!(up_projection_audit_nt, m)?)?;
    m.add_function(wrap_pyfunction!(up_projection_resolve_context, m)?)?;
    m.add_function(wrap_pyfunction!(up_projection_reverse_nt, m)?)?;
    Ok(())
}
