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
use pyo3::types::{PyCapsule, PyDict, PyList};

use crate::coverage;
use crate::dsl;
use crate::gufo::{self, GufoConfig};
use crate::lint::{self, LintConfig, LintReport, ModuleSpec};
use crate::store;
use crate::validate_all::{self, ValidateOptions};

/// Build the standard `{"errors": [...], "warnings": [...]}` report dict.
fn report_dict(py: Python<'_>, errors: Vec<String>, warnings: Vec<String>) -> PyResult<Py<PyAny>> {
    let out = PyDict::new(py);
    out.set_item("errors", PyList::new(py, &errors)?)?;
    out.set_item("warnings", PyList::new(py, &warnings)?)?;
    Ok(out.into_any().unbind())
}

/// Convert a [`LintReport`] into the standard report dict.
fn lint_report_dict(py: Python<'_>, report: LintReport) -> PyResult<Py<PyAny>> {
    report_dict(py, report.errors, report.warnings)
}

/// Strongly-typed lint configuration crossing the FFI boundary.
///
/// Constructed in Python from `config.NAMESPACE` / `config.ONTOLOGY_IRI`, the
/// `_SELECTOR_TOKENS` set, the core-slice IRI list, and the annotation-predicate
/// list — no untyped dict bag (#579). Shared by `structural_lint`,
/// `term_naming_lint`, and `declared_terms`.
#[pyclass(name = "LintConfig", from_py_object)]
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

/// Options for the native validation orchestration.
///
/// Carries the boolean timing flag plus the optional inputs some phases need
/// (`sameas_allowlist`, slice-ownership registry, slices directory, and DSL
/// shape texts). Every field has a sensible default so callers can opt into
/// phases by setting the relevant fields.
#[pyclass(name = "ValidateOptions", from_py_object)]
#[derive(Clone)]
struct PyValidateOptions {
    timings: bool,
    sameas_allowlist: Vec<(String, String)>,
    module_specs: Vec<(String, String)>,
    slices_dir: Option<String>,
    mapping_shapes_ttl: Option<String>,
    statement_shapes_ttl: Option<String>,
    /// Project root used to locate `.cache/validate`. When `None`, the cache is
    /// disabled. Task 4 wires Python to pass `PROJECT_ROOT`.
    project_root: Option<String>,
    /// Optional GTS byte bundle. When present, the orchestration builds the
    /// shared store from the bundle instead of from `source_paths`, and the
    /// per-file Turtle phases (syntax check, `owl:sameAs` ban) are skipped.
    gts_bytes: Option<Vec<u8>>,
}

#[pymethods]
impl PyValidateOptions {
    #[new]
    #[allow(clippy::too_many_arguments)]
    #[pyo3(signature = (
        timings = false,
        sameas_allowlist = None,
        module_specs = None,
        slices_dir = None,
        mapping_shapes_ttl = None,
        statement_shapes_ttl = None,
        project_root = None,
        gts_bytes = None,
    ))]
    fn new(
        timings: bool,
        sameas_allowlist: Option<Vec<(String, String)>>,
        module_specs: Option<Vec<(String, String)>>,
        slices_dir: Option<String>,
        mapping_shapes_ttl: Option<String>,
        statement_shapes_ttl: Option<String>,
        project_root: Option<String>,
        gts_bytes: Option<Vec<u8>>,
    ) -> Self {
        Self {
            timings,
            sameas_allowlist: sameas_allowlist.unwrap_or_default(),
            module_specs: module_specs.unwrap_or_default(),
            slices_dir,
            mapping_shapes_ttl,
            statement_shapes_ttl,
            project_root,
            gts_bytes,
        }
    }
}

impl PyValidateOptions {
    fn to_engine(&self) -> ValidateOptions {
        ValidateOptions {
            timings: self.timings,
            sameas_allowlist: self.sameas_allowlist.clone(),
            module_specs: self.module_specs.clone(),
            slices_dir: self.slices_dir.clone(),
            mapping_shapes_ttl: self.mapping_shapes_ttl.clone(),
            statement_shapes_ttl: self.statement_shapes_ttl.clone(),
            project_root: self.project_root.as_ref().map(PathBuf::from),
            gts_bytes: self.gts_bytes.clone(),
        }
    }
}

/// A reusable oxigraph store built once from canonical source paths.
///
/// Python hands the source paths once; the store can then be validated against
/// parsed SHACL shapes across multiple phases without re-parsing the sources
/// (#634).
#[pyclass(name = "ValidationStore")]
struct PyValidationStore {
    store: oxigraph::store::Store,
    #[allow(dead_code)]
    source_paths: Vec<String>,
}

#[pymethods]
impl PyValidationStore {
    #[new]
    fn new(source_paths: Vec<String>) -> PyResult<Self> {
        if source_paths.is_empty() {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "ValidationStore: source_paths must not be empty",
            ));
        }
        let paths: Vec<PathBuf> = source_paths.iter().map(PathBuf::from).collect();
        let store = store::build_store(&paths).map_err(pyo3::exceptions::PyValueError::new_err)?;
        Ok(Self {
            store,
            source_paths,
        })
    }

    /// Build a store from a GTS byte bundle instead of from Turtle source paths.
    #[staticmethod]
    fn from_gts_bytes(gts_bytes: Vec<u8>) -> PyResult<Self> {
        let store = store::build_store_from_gts(&gts_bytes)
            .map_err(pyo3::exceptions::PyValueError::new_err)?;
        Ok(Self {
            store,
            source_paths: Vec::new(),
        })
    }

    /// Internal protocol: return a transient capsule that borrows the wrapped
    /// oxigraph store.
    ///
    /// The capsule is consumed immediately by `gmeow_shacl.Shapes.validate_store`
    /// so the SHACL engine can validate the store directly without an N-Triples
    /// round-trip. Keeping the capsule alive after the store is dropped is
    /// undefined behaviour; do not call this directly from Python.
    fn _store_capsule<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyCapsule>> {
        let addr = &self.store as *const oxigraph::store::Store as usize;
        // SAFETY: the capsule borrows `self.store`. It must not outlive `self`.
        PyCapsule::new_with_value(py, addr, c"gmeow-validation-store")
    }

    /// Validate this store against a parsed SHACL shapes model.
    ///
    /// Convenience wrapper that delegates to
    /// `gmeow_shacl.Shapes.validate_store(self)` so the two classes can live in
    /// their respective extension modules while still sharing the underlying
    /// oxigraph store directly.
    fn validate(slf: &Bound<'_, Self>, shapes: &Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
        let report = shapes.call_method1("validate_store", (slf,))?;
        Ok(report.unbind())
    }
}

/// Build the merged store from `source_paths`, mapping a parse failure to a
/// Python `ValueError` (a hard failure that must surface — the validation path
/// has no rdflib fallback, #579).
fn build_store_or_err(source_paths: &[String]) -> PyResult<oxigraph::store::Store> {
    let paths: Vec<PathBuf> = source_paths.iter().map(PathBuf::from).collect();
    store::build_store(&paths).map_err(pyo3::exceptions::PyValueError::new_err)
}

/// Build the store from an N-Triples string (the rdflib-free data seam, #579),
/// mapping a parse failure to a Python `ValueError`. The reasoning checks accept
/// graphs as N-Triples now (test shims build a synthetic graph and serialize it),
/// so this is their ingestion primitive.
fn build_store_from_nt_or_err(data_nt: &str) -> PyResult<oxigraph::store::Store> {
    store::build_store_from_nt(data_nt).map_err(pyo3::exceptions::PyValueError::new_err)
}

/// Structural lint over the merged sources (mirrors `validate.structural_lint`).
#[pyfunction]
fn structural_lint(
    py: Python<'_>,
    source_paths: Vec<String>,
    cfg: PyLintConfig,
) -> PyResult<Py<PyAny>> {
    if source_paths.is_empty() {
        return Err(pyo3::exceptions::PyValueError::new_err(
            "structural_lint: paths to lint must not be empty",
        ));
    }
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
) -> PyResult<Py<PyAny>> {
    if source_paths.is_empty() {
        return Err(pyo3::exceptions::PyValueError::new_err(
            "term_naming_lint: paths to lint must not be empty",
        ));
    }
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
) -> PyResult<Py<PyAny>> {
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
fn typed_terms(
    py: Python<'_>,
    source_paths: Vec<String>,
    cfg: PyLintConfig,
) -> PyResult<Py<PyAny>> {
    if source_paths.is_empty() {
        return Err(pyo3::exceptions::PyValueError::new_err(
            "typed_terms: paths to scan must not be empty",
        ));
    }
    let store = build_store_or_err(&source_paths)?;
    let pairs: Vec<(String, String)> = lint::collect_typed_terms(&store, &cfg.to_engine())
        .into_iter()
        .collect();
    Ok(PyList::new(py, &pairs)?.into_any().unbind())
}

/// The declared GMEOW-term IRI set over the merged sources (mirrors
/// `set(_collect_typed_terms(graph))`), for `guide_anchor_lint`.
#[pyfunction]
fn declared_terms(
    py: Python<'_>,
    source_paths: Vec<String>,
    cfg: PyLintConfig,
) -> PyResult<Py<PyAny>> {
    if source_paths.is_empty() {
        return Err(pyo3::exceptions::PyValueError::new_err(
            "declared_terms: paths to scan must not be empty",
        ));
    }
    let store = build_store_or_err(&source_paths)?;
    let terms = lint::declared_terms(&store, &cfg.to_engine());
    Ok(PyList::new(py, &terms)?.into_any().unbind())
}

/// Parse every source Turtle file individually to catch syntax errors.
///
/// Mirrors `validate.check_syntax`: on a parse failure for `path`, appends
/// `"syntax error in {path}: {exc}"`. Returns `{"errors": [...], "warnings": []}`.
#[pyfunction]
fn check_syntax(py: Python<'_>, paths: Vec<String>) -> PyResult<Py<PyAny>> {
    if paths.is_empty() {
        return Err(pyo3::exceptions::PyValueError::new_err(
            "check_syntax: paths to check must not be empty",
        ));
    }
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
) -> PyResult<Py<PyAny>> {
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
fn errors_dict(py: Python<'_>, errors: Vec<String>) -> PyResult<Py<PyAny>> {
    report_dict(py, errors, Vec::new())
}

/// A gUFO anti-pattern check: `(store, cfg) -> errors`.
type GufoCheck = fn(&oxigraph::store::Store, &GufoConfig) -> Vec<String>;

/// Run one gUFO check over the merged sources (the production `validate_all`
/// path passes file paths directly — no rdflib graph, #579).
fn run_reasoning_paths(
    py: Python<'_>,
    check: GufoCheck,
    source_paths: Vec<String>,
    namespace: String,
) -> PyResult<Py<PyAny>> {
    let store = build_store_or_err(&source_paths)?;
    let cfg = GufoConfig { namespace };
    errors_dict(py, check(&store, &cfg))
}

/// Run one gUFO check over an N-Triples graph string (the test-shim seam: a
/// synthetic graph serialized to N-Triples, no rdflib in the validation-path
/// source files, #579).
fn run_reasoning_nt(
    py: Python<'_>,
    check: GufoCheck,
    data_nt: &str,
    namespace: String,
) -> PyResult<Py<PyAny>> {
    let store = build_store_from_nt_or_err(data_nt)?;
    let cfg = GufoConfig { namespace };
    errors_dict(py, check(&store, &cfg))
}

/// The aggregate gUFO/UFO reasoning invariants over the merged sources (mirrors
/// `reasoning_lint.reasoning_invariants`). Runs all six anti-pattern checks and
/// flattens their errors in declaration order.
#[pyfunction]
fn reasoning_invariants(
    py: Python<'_>,
    source_paths: Vec<String>,
    namespace: String,
) -> PyResult<Py<PyAny>> {
    if source_paths.is_empty() {
        return Err(pyo3::exceptions::PyValueError::new_err(
            "reasoning_invariants: paths to check must not be empty",
        ));
    }
    run_reasoning_paths(py, gufo::reasoning_invariants, source_paths, namespace)
}

/// The aggregate reasoning invariants over an N-Triples graph (test-shim seam).
#[pyfunction]
fn reasoning_invariants_nt(
    py: Python<'_>,
    data_nt: &str,
    namespace: String,
) -> PyResult<Py<PyAny>> {
    run_reasoning_nt(py, gufo::reasoning_invariants, data_nt, namespace)
}

/// `exactly_one_stereotype` over an N-Triples graph (test-shim seam).
#[pyfunction]
fn reasoning_exactly_one_stereotype_nt(
    py: Python<'_>,
    data_nt: &str,
    namespace: String,
) -> PyResult<Py<PyAny>> {
    run_reasoning_nt(py, gufo::exactly_one_stereotype, data_nt, namespace)
}

/// `identity_overlap` (MixIden) over an N-Triples graph (test-shim seam).
#[pyfunction]
fn reasoning_identity_overlap_nt(
    py: Python<'_>,
    data_nt: &str,
    namespace: String,
) -> PyResult<Py<PyAny>> {
    run_reasoning_nt(py, gufo::identity_overlap, data_nt, namespace)
}

/// `anti_rigidity_discipline` (MixRig / FreeRole) over an N-Triples graph.
#[pyfunction]
fn reasoning_anti_rigidity_discipline_nt(
    py: Python<'_>,
    data_nt: &str,
    namespace: String,
) -> PyResult<Py<PyAny>> {
    run_reasoning_nt(py, gufo::anti_rigidity_discipline, data_nt, namespace)
}

/// `relator_mediation` (RelComp) over an N-Triples graph (test-shim seam).
#[pyfunction]
fn reasoning_relator_mediation_nt(
    py: Python<'_>,
    data_nt: &str,
    namespace: String,
) -> PyResult<Py<PyAny>> {
    run_reasoning_nt(py, gufo::relator_mediation, data_nt, namespace)
}

/// `coequal_facet_orthogonality` (P9 #281) over an N-Triples graph.
#[pyfunction]
fn reasoning_coequal_facet_orthogonality_nt(
    py: Python<'_>,
    data_nt: &str,
    namespace: String,
) -> PyResult<Py<PyAny>> {
    run_reasoning_nt(py, gufo::coequal_facet_orthogonality, data_nt, namespace)
}

/// `frame_declaration_completeness` (P11 #283) over an N-Triples graph.
#[pyfunction]
fn reasoning_frame_declaration_completeness_nt(
    py: Python<'_>,
    data_nt: &str,
    namespace: String,
) -> PyResult<Py<PyAny>> {
    run_reasoning_nt(py, gufo::frame_declaration_completeness, data_nt, namespace)
}

/// Coverage analysis over the vendored entity-slice fixtures (mirrors
/// `coverage.analyze`).
///
/// `fixture_paths` is the discovered fixture file list; `aligned` is the
/// SSSOM-derived external-IRI set (`coverage.covered_iris()`, computed in
/// Python); `namespace` is `config.NAMESPACE`. Returns a dict with the four
/// sorted IRI lists: `covered_classes`, `gap_classes`, `covered_predicates`,
/// `gap_predicates`. A parse failure maps to a Python `ValueError` (a hard
/// failure that must surface — the coverage path has no rdflib fallback, #579).
#[pyfunction]
fn coverage_analyze(
    py: Python<'_>,
    fixture_paths: Vec<String>,
    aligned: Vec<String>,
    namespace: String,
) -> PyResult<Py<PyAny>> {
    let paths: Vec<PathBuf> = fixture_paths.iter().map(PathBuf::from).collect();
    let aligned_set: BTreeSet<String> = aligned.into_iter().collect();
    let sets = coverage::coverage_analyze(&paths, &aligned_set, &namespace)
        .map_err(pyo3::exceptions::PyValueError::new_err)?;
    let out = PyDict::new(py);
    out.set_item(
        "covered_classes",
        PyList::new(py, sets.covered_classes.iter())?,
    )?;
    out.set_item("gap_classes", PyList::new(py, sets.gap_classes.iter())?)?;
    out.set_item(
        "covered_predicates",
        PyList::new(py, sets.covered_predicates.iter())?,
    )?;
    out.set_item(
        "gap_predicates",
        PyList::new(py, sets.gap_predicates.iter())?,
    )?;
    Ok(out.into_any().unbind())
}

/// Build the merged graph from `source_paths` and dump it as canonical
/// N-Triples (legacy/test-only seam, #579/#634).
///
/// The production `make validate` path now validates the shared oxigraph store
/// directly in Rust and no longer uses N-Triples as internal transport. This
/// function remains exposed for tests and legacy callers that still need an
/// N-Triples serialization of a merged Turtle corpus. A parse failure maps to a
/// Python `ValueError` (hard-fail, no fallback).
#[pyfunction]
fn merge_to_ntriples(source_paths: Vec<String>) -> PyResult<String> {
    if source_paths.is_empty() {
        return Err(pyo3::exceptions::PyValueError::new_err(
            "merge_to_ntriples: paths to merge must not be empty",
        ));
    }
    let paths: Vec<PathBuf> = source_paths.iter().map(PathBuf::from).collect();
    dsl::merge_to_ntriples(&paths).map_err(pyo3::exceptions::PyValueError::new_err)
}

/// Build the merged DSL graph from `dsl_paths` as N-Triples, plus the focus→file
/// provenance map (mirrors the legacy `node_to_file` walk, #579).
///
/// Returns `(data_nt, [(named_subject_iri, source_file_path), ...])` where the
/// pairs record the FIRST `.ttl` file each named subject appears in, in
/// first-seen order. `dsl_validate.py` validates `data_nt` via `gmeow_shacl` and
/// enriches each violation with `source=` from the map — no rdflib. A parse
/// failure maps to a Python `ValueError`.
#[pyfunction]
fn dsl_merge_with_provenance(
    py: Python<'_>,
    dsl_paths: Vec<String>,
) -> PyResult<(String, Py<PyAny>)> {
    if dsl_paths.is_empty() {
        return Err(pyo3::exceptions::PyValueError::new_err(
            "dsl_merge_with_provenance: paths to merge must not be empty",
        ));
    }
    let paths: Vec<PathBuf> = dsl_paths.iter().map(PathBuf::from).collect();
    let merge =
        dsl::merge_with_provenance(&paths).map_err(pyo3::exceptions::PyValueError::new_err)?;
    let pairs = PyList::new(py, &merge.focus_to_file)?;
    Ok((merge.data_nt, pairs.into_any().unbind()))
}

/// Native validation orchestration entrypoint (#634).
///
/// Builds the ontology store once, parses the SHACL shapes once, runs every
/// validation phase, and returns a dict with `errors`, `warnings`, `timings`,
/// `declared_terms`, and `report_json` — the single canonical diagnostics
/// report serialized to JSON, from which `errors`/`warnings` are derived and
/// which carries SHACL focus nodes and GTS wire coordinates (#654). Optional
/// phases (example coverage/SHACL, DSL SHACL) are enabled by the corresponding
/// fields in `options`.
#[pyfunction]
fn validate_all_native(
    py: Python<'_>,
    source_paths: Vec<String>,
    shapes_ttl: String,
    mapping_dsl_dir: String,
    statement_dsl_dir: String,
    config: PyLintConfig,
    options: PyValidateOptions,
) -> PyResult<Py<PyAny>> {
    // Empty source_paths is only valid when a GTS bundle supplies the store
    // (mirrors ValidationRun::run's own contract, #644).
    if source_paths.is_empty() && options.gts_bytes.is_none() {
        return Err(pyo3::exceptions::PyValueError::new_err(
            "validate_all_native: source_paths must not be empty unless gts_bytes is provided",
        ));
    }

    let run = validate_all::ValidationRun::run(
        &source_paths,
        &shapes_ttl,
        &mapping_dsl_dir,
        &statement_dsl_dir,
        &config.to_engine(),
        &options.to_engine(),
    )
    .map_err(pyo3::exceptions::PyValueError::new_err)?;

    let report_json = run
        .report_json()
        .map_err(|e| pyo3::exceptions::PyValueError::new_err(format!("serialize report: {e}")))?;

    let out = PyDict::new(py);
    out.set_item("errors", PyList::new(py, run.errors())?)?;
    out.set_item("warnings", PyList::new(py, run.warnings())?)?;
    out.set_item("declared_terms", PyList::new(py, &run.declared_terms)?)?;
    out.set_item("report_json", report_json)?;

    let timings = PyList::empty(py);
    for t in &run.timings {
        let d = PyDict::new(py);
        d.set_item("phase", &t.phase)?;
        d.set_item("elapsed_ms", t.elapsed_ms)?;
        d.set_item("metadata", t.metadata.as_deref().unwrap_or(""))?;
        timings.append(d)?;
    }
    out.set_item("timings", timings)?;

    Ok(out.into_any().unbind())
}

/// Python extension module `gmeow_validate`.
///
/// Exposes the syntax / sameAs lints (Task 1) plus the structural, naming,
/// ownership, and declared-term lints (Task 2, #579), and the `LintConfig` type
/// that carries their typed configuration across the FFI boundary.
#[pymodule]
fn gmeow_validate(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PyLintConfig>()?;
    m.add_class::<PyValidateOptions>()?;
    m.add_class::<PyValidationStore>()?;
    m.add_function(wrap_pyfunction!(check_syntax, m)?)?;
    m.add_function(wrap_pyfunction!(check_sameas_ban, m)?)?;
    m.add_function(wrap_pyfunction!(structural_lint, m)?)?;
    m.add_function(wrap_pyfunction!(term_naming_lint, m)?)?;
    m.add_function(wrap_pyfunction!(slice_ownership_lint, m)?)?;
    m.add_function(wrap_pyfunction!(typed_terms, m)?)?;
    m.add_function(wrap_pyfunction!(declared_terms, m)?)?;
    m.add_function(wrap_pyfunction!(reasoning_invariants, m)?)?;
    m.add_function(wrap_pyfunction!(reasoning_invariants_nt, m)?)?;
    m.add_function(wrap_pyfunction!(reasoning_exactly_one_stereotype_nt, m)?)?;
    m.add_function(wrap_pyfunction!(reasoning_identity_overlap_nt, m)?)?;
    m.add_function(wrap_pyfunction!(reasoning_anti_rigidity_discipline_nt, m)?)?;
    m.add_function(wrap_pyfunction!(reasoning_relator_mediation_nt, m)?)?;
    m.add_function(wrap_pyfunction!(
        reasoning_coequal_facet_orthogonality_nt,
        m
    )?)?;
    m.add_function(wrap_pyfunction!(
        reasoning_frame_declaration_completeness_nt,
        m
    )?)?;
    m.add_function(wrap_pyfunction!(coverage_analyze, m)?)?;
    m.add_function(wrap_pyfunction!(merge_to_ntriples, m)?)?;
    m.add_function(wrap_pyfunction!(dsl_merge_with_provenance, m)?)?;
    m.add_function(wrap_pyfunction!(validate_all_native, m)?)?;
    Ok(())
}
