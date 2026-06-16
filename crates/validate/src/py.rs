// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! PyO3 Python bindings for `gmeow-validate`.
//!
//! # Engine core separation
//!
//! Only this file imports pyo3. The engine modules ([`crate::store`],
//! [`crate::model`]) are PyO3-free so the rlib links into the future Rust
//! compiler without any Python dependency.
//!
//! # Platform note
//!
//! There is no architecture cfg guard on this module: the crate is native-only
//! by construction (a capability cfg would be optionality, not compliance,
//! #579). pyo3 cannot link into wasm, so the crate is simply never built for
//! wasm.

use std::collections::{BTreeSet, HashSet};
use std::path::PathBuf;

use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList};

use crate::gufo::{self, GufoConfig};
use crate::lint::{self, LintConfig, LintReport, ModuleSpec};
use crate::store;

/// Build the standard `{"errors": [...], "warnings": [...]}` report dict.
fn report_dict(py: Python<'_>, errors: Vec<String>, warnings: Vec<String>) -> PyResult<PyObject> {
    let out = PyDict::new(py);
    out.set_item("errors", PyList::new(py, &errors)?)?;
    out.set_item("warnings", PyList::new(py, &warnings)?)?;
    Ok(out.into())
}

/// Convert a [`LintReport`] into the standard report dict.
fn lint_report_dict(py: Python<'_>, report: LintReport) -> PyResult<PyObject> {
    report_dict(py, report.errors, report.warnings)
}

/// Strongly-typed lint configuration crossing the FFI boundary.
///
/// Constructed in Python from `config.NAMESPACE` / `config.ONTOLOGY_IRI`, the
/// `_SELECTOR_TOKENS` set, the core-slice IRI list, and the annotation-predicate
/// list — no untyped dict bag (#579). Shared by `structural_lint`,
/// `term_naming_lint`, and `declared_terms`.
#[pyclass(name = "LintConfig")]
#[derive(Clone)]
struct PyLintConfig {
    namespace: String,
    ontology_iri: String,
    selector_tokens: Vec<String>,
    core_slice_iris: Vec<String>,
    annotation_predicates: Vec<String>,
}

#[pymethods]
impl PyLintConfig {
    #[new]
    fn new(
        namespace: String,
        ontology_iri: String,
        selector_tokens: Vec<String>,
        core_slice_iris: Vec<String>,
        annotation_predicates: Vec<String>,
    ) -> Self {
        Self {
            namespace,
            ontology_iri,
            selector_tokens,
            core_slice_iris,
            annotation_predicates,
        }
    }
}

impl PyLintConfig {
    fn to_engine(&self) -> LintConfig {
        LintConfig {
            namespace: self.namespace.clone(),
            ontology_iri: self.ontology_iri.clone(),
            selector_tokens: self
                .selector_tokens
                .iter()
                .cloned()
                .collect::<BTreeSet<_>>(),
            core_slice_iris: self.core_slice_iris.iter().cloned().collect::<HashSet<_>>(),
            annotation_predicates: self
                .annotation_predicates
                .iter()
                .cloned()
                .collect::<HashSet<_>>(),
        }
    }
}

/// Build the merged store from `source_paths`, mapping a parse failure to a
/// Python `ValueError` (a hard failure that must surface — the validation path
/// has no rdflib fallback, #579).
fn build_store_or_err(source_paths: &[String]) -> PyResult<oxigraph::store::Store> {
    let paths: Vec<PathBuf> = source_paths.iter().map(PathBuf::from).collect();
    store::build_store(&paths).map_err(pyo3::exceptions::PyValueError::new_err)
}

/// Structural lint over the merged sources (mirrors `validate.structural_lint`).
#[pyfunction]
fn structural_lint(
    py: Python<'_>,
    source_paths: Vec<String>,
    cfg: PyLintConfig,
) -> PyResult<PyObject> {
    let store = build_store_or_err(&source_paths)?;
    let report = lint::structural_lint(&store, &cfg.to_engine());
    lint_report_dict(py, report)
}

/// Term-naming lint over the merged sources (mirrors `validate.term_naming_lint`).
#[pyfunction]
fn term_naming_lint(
    py: Python<'_>,
    source_paths: Vec<String>,
    cfg: PyLintConfig,
) -> PyResult<PyObject> {
    let store = build_store_or_err(&source_paths)?;
    let report = lint::term_naming_lint(&store, &cfg.to_engine());
    lint_report_dict(py, report)
}

/// Slice-ownership lint (mirrors `validate.slice_ownership_lint`).
///
/// `module_specs` is a list of `(module_path, expected_slice_iri)` pairs; each
/// module is parsed ALONE so a term's ownership claim is checked against its own
/// containing slice (#329).
#[pyfunction]
fn slice_ownership_lint(
    py: Python<'_>,
    module_specs: Vec<(String, String)>,
    cfg: PyLintConfig,
) -> PyResult<PyObject> {
    let mut modules: Vec<(ModuleSpec, oxigraph::store::Store)> = Vec::new();
    for (module_path, expected_slice_iri) in module_specs {
        let store = build_store_or_err(std::slice::from_ref(&module_path))?;
        modules.push((
            ModuleSpec {
                module_path,
                expected_slice_iri,
            },
            store,
        ));
    }
    let report = lint::slice_ownership_lint(&modules, &cfg.to_engine());
    lint_report_dict(py, report)
}

/// The typed GMEOW terms over the merged sources as `[(iri, kind)]` (mirrors
/// `_collect_typed_terms`), for the Python `_collect_typed_terms`/`_term_kind`
/// routes.
#[pyfunction]
fn typed_terms(py: Python<'_>, source_paths: Vec<String>, cfg: PyLintConfig) -> PyResult<PyObject> {
    let store = build_store_or_err(&source_paths)?;
    let pairs: Vec<(String, String)> = lint::collect_typed_terms(&store, &cfg.to_engine())
        .into_iter()
        .collect();
    Ok(PyList::new(py, &pairs)?.into())
}

/// The declared GMEOW-term IRI set over the merged sources (mirrors
/// `set(_collect_typed_terms(graph))`), for `guide_anchor_lint`.
#[pyfunction]
fn declared_terms(
    py: Python<'_>,
    source_paths: Vec<String>,
    cfg: PyLintConfig,
) -> PyResult<PyObject> {
    let store = build_store_or_err(&source_paths)?;
    let terms = lint::declared_terms(&store, &cfg.to_engine());
    Ok(PyList::new(py, &terms)?.into())
}

/// Parse every source Turtle file individually to catch syntax errors.
///
/// Mirrors `validate.check_syntax`: on a parse failure for `path`, appends
/// `"syntax error in {path}: {exc}"`. Returns `{"errors": [...], "warnings": []}`.
#[pyfunction]
fn check_syntax(py: Python<'_>, paths: Vec<String>) -> PyResult<PyObject> {
    let mut errors: Vec<String> = Vec::new();
    for path in &paths {
        if let Err(exc) = store::parse_file(std::path::Path::new(path)) {
            errors.push(format!("syntax error in {path}: {exc}"));
        }
    }
    report_dict(py, errors, Vec::new())
}

/// Enforce Principle 5: no `owl:sameAs` merge with external entities.
///
/// Mirrors `validate.check_sameas_ban`. For each `owl:sameAs` triple whose
/// object is a NamedNode NOT starting with `namespace`, unless
/// `(subject_display, object)` is in `allowlist`, appends the exact error string
/// the Python path emits. A file that fails to parse yields a
/// `"failed to parse {path}: {exc}"` error, matching the Python contract.
///
/// Returns `{"errors": [...], "warnings": []}`.
#[pyfunction]
fn check_sameas_ban(
    py: Python<'_>,
    paths: Vec<String>,
    namespace: String,
    allowlist: Vec<(String, String)>,
) -> PyResult<PyObject> {
    if paths.is_empty() {
        return Err(pyo3::exceptions::PyValueError::new_err(
            "check_sameas_ban: paths to audit must not be empty",
        ));
    }

    let mut errors: Vec<String> = Vec::new();
    for path in &paths {
        let quads = match store::parse_file(std::path::Path::new(path)) {
            Ok(q) => q,
            Err(exc) => {
                errors.push(format!("failed to parse {path}: {exc}"));
                continue;
            }
        };
        for (subject_text, obj) in store::sameas_violations(&quads, &namespace, &allowlist) {
            errors.push(format!(
                "{path}: banned owl:sameAs to external entity \
                 {subject_text} owl:sameAs {obj} (Principle 5); \
                 use skos:exactMatch or gmeow:authorityLink"
            ));
        }
    }
    report_dict(py, errors, Vec::new())
}

/// Build the `{"errors": [...], "warnings": []}` report dict from a flat error
/// list (the reasoning checks only ever produce errors today; the dict shape is
/// kept consistent with every other lint).
fn errors_dict(py: Python<'_>, errors: Vec<String>) -> PyResult<PyObject> {
    report_dict(py, errors, Vec::new())
}

/// The aggregate gUFO/UFO reasoning invariants over the merged sources (mirrors
/// `reasoning_lint.reasoning_invariants`). Runs all six anti-pattern checks and
/// flattens their errors in declaration order.
#[pyfunction]
fn reasoning_invariants(
    py: Python<'_>,
    source_paths: Vec<String>,
    namespace: String,
) -> PyResult<PyObject> {
    let store = build_store_or_err(&source_paths)?;
    let cfg = GufoConfig { namespace };
    errors_dict(py, gufo::reasoning_invariants(&store, &cfg))
}

/// `exactly_one_stereotype` over the merged sources (mirrors the same-named
/// Python check; exposed for the test shim that builds a graph per check).
#[pyfunction]
fn reasoning_exactly_one_stereotype(
    py: Python<'_>,
    source_paths: Vec<String>,
    namespace: String,
) -> PyResult<PyObject> {
    let store = build_store_or_err(&source_paths)?;
    let cfg = GufoConfig { namespace };
    errors_dict(py, gufo::exactly_one_stereotype(&store, &cfg))
}

/// `identity_overlap` (MixIden) over the merged sources.
#[pyfunction]
fn reasoning_identity_overlap(
    py: Python<'_>,
    source_paths: Vec<String>,
    namespace: String,
) -> PyResult<PyObject> {
    let store = build_store_or_err(&source_paths)?;
    let cfg = GufoConfig { namespace };
    errors_dict(py, gufo::identity_overlap(&store, &cfg))
}

/// `anti_rigidity_discipline` (MixRig / FreeRole) over the merged sources.
#[pyfunction]
fn reasoning_anti_rigidity_discipline(
    py: Python<'_>,
    source_paths: Vec<String>,
    namespace: String,
) -> PyResult<PyObject> {
    let store = build_store_or_err(&source_paths)?;
    let cfg = GufoConfig { namespace };
    errors_dict(py, gufo::anti_rigidity_discipline(&store, &cfg))
}

/// `relator_mediation` (RelComp) over the merged sources.
#[pyfunction]
fn reasoning_relator_mediation(
    py: Python<'_>,
    source_paths: Vec<String>,
    namespace: String,
) -> PyResult<PyObject> {
    let store = build_store_or_err(&source_paths)?;
    let cfg = GufoConfig { namespace };
    errors_dict(py, gufo::relator_mediation(&store, &cfg))
}

/// `coequal_facet_orthogonality` (P9 #281) over the merged sources.
#[pyfunction]
fn reasoning_coequal_facet_orthogonality(
    py: Python<'_>,
    source_paths: Vec<String>,
    namespace: String,
) -> PyResult<PyObject> {
    let store = build_store_or_err(&source_paths)?;
    let cfg = GufoConfig { namespace };
    errors_dict(py, gufo::coequal_facet_orthogonality(&store, &cfg))
}

/// `frame_declaration_completeness` (P11 #283) over the merged sources.
#[pyfunction]
fn reasoning_frame_declaration_completeness(
    py: Python<'_>,
    source_paths: Vec<String>,
    namespace: String,
) -> PyResult<PyObject> {
    let store = build_store_or_err(&source_paths)?;
    let cfg = GufoConfig { namespace };
    errors_dict(py, gufo::frame_declaration_completeness(&store, &cfg))
}

/// Python extension module `gmeow_validate`.
///
/// Exposes the syntax / sameAs lints (Task 1) plus the structural, naming,
/// ownership, and declared-term lints (Task 2, #579), and the `LintConfig` type
/// that carries their typed configuration across the FFI boundary.
#[pymodule]
fn gmeow_validate(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PyLintConfig>()?;
    m.add_function(wrap_pyfunction!(check_syntax, m)?)?;
    m.add_function(wrap_pyfunction!(check_sameas_ban, m)?)?;
    m.add_function(wrap_pyfunction!(structural_lint, m)?)?;
    m.add_function(wrap_pyfunction!(term_naming_lint, m)?)?;
    m.add_function(wrap_pyfunction!(slice_ownership_lint, m)?)?;
    m.add_function(wrap_pyfunction!(typed_terms, m)?)?;
    m.add_function(wrap_pyfunction!(declared_terms, m)?)?;
    m.add_function(wrap_pyfunction!(reasoning_invariants, m)?)?;
    m.add_function(wrap_pyfunction!(reasoning_exactly_one_stereotype, m)?)?;
    m.add_function(wrap_pyfunction!(reasoning_identity_overlap, m)?)?;
    m.add_function(wrap_pyfunction!(reasoning_anti_rigidity_discipline, m)?)?;
    m.add_function(wrap_pyfunction!(reasoning_relator_mediation, m)?)?;
    m.add_function(wrap_pyfunction!(reasoning_coequal_facet_orthogonality, m)?)?;
    m.add_function(wrap_pyfunction!(
        reasoning_frame_declaration_completeness,
        m
    )?)?;
    Ok(())
}
