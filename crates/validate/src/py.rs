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

use gmeow_diagnostics::py::PyReport;
use pyo3::prelude::*;
use pyo3::types::{PyCapsule, PyDict, PyList};

use crate::constitution;
use crate::coverage;
use crate::crossref;
use crate::dsl;
use crate::gufo::{self, GufoConfig};
use crate::instance::{self, InstanceFormat};
use crate::language_tags;
use crate::lint::{self, LintConfig, LintReport};
use crate::slice_ownership;
use crate::statement;
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
    #[pyo3(signature = (
        namespace,
        ontology_iri,
        selector_tokens,
        core_slice_iris,
        annotation_predicates = None,
    ))]
    fn new(
        namespace: String,
        ontology_iri: String,
        selector_tokens: Vec<String>,
        core_slice_iris: Vec<String>,
        annotation_predicates: Option<Vec<String>>,
    ) -> Self {
        Self {
            namespace,
            ontology_iri,
            selector_tokens,
            core_slice_iris,
            // The annotation-predicate registry is owned by this crate (#630):
            // when the caller omits it, fall back to the canonical Rust set.
            annotation_predicates: annotation_predicates
                .unwrap_or_else(crate::lint::default_annotation_predicates),
        }
    }
}

/// The canonical annotation predicates the Check-2 language-tag policy polices.
///
/// This crate is the single source of truth (#630); the Python `language_tags`
/// helpers read the set from here instead of maintaining a parallel constant.
#[pyfunction]
fn annotation_predicates() -> Vec<String> {
    crate::lint::default_annotation_predicates()
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

/// Signature/trust policy configuration for the GTS verification pre-gate.
///
/// Mirrors [`crate::validate_all::SignatureConfig`] and is constructed in Python
/// from a policy TOML or CLI flags, then passed into [`PyValidateOptions`].
#[pyclass(name = "SignatureConfig", from_py_object)]
#[derive(Clone)]
struct PySignatureConfig {
    trusted_signers: Vec<String>,
    require_signatures: bool,
    require_trusted_signer: bool,
    trusted_key: Option<String>,
}

#[pymethods]
impl PySignatureConfig {
    #[new]
    #[allow(clippy::too_many_arguments)]
    #[pyo3(signature = (
        trusted_signers = None,
        require_signatures = false,
        require_trusted_signer = false,
        trusted_key = None,
    ))]
    fn new(
        trusted_signers: Option<Vec<String>>,
        require_signatures: bool,
        require_trusted_signer: bool,
        trusted_key: Option<String>,
    ) -> Self {
        Self {
            trusted_signers: trusted_signers.unwrap_or_default(),
            require_signatures,
            require_trusted_signer,
            trusted_key,
        }
    }
}

impl PySignatureConfig {
    fn to_engine(&self) -> crate::validate_all::SignatureConfig {
        crate::validate_all::SignatureConfig {
            trusted_signers: self.trusted_signers.clone(),
            require_signatures: self.require_signatures,
            require_trusted_signer: self.require_trusted_signer,
            trusted_key: self.trusted_key.clone(),
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
    /// Optional signature/trust policy configuration for the GTS verification
    /// pre-gate (#646). When `None`, signature verification is disabled.
    signature_config: Option<PySignatureConfig>,
}

#[pymethods]
impl PyValidateOptions {
    #[new]
    #[allow(clippy::too_many_arguments)]
    #[pyo3(signature = (
        timings = false,
        sameas_allowlist = None,
        slices_dir = None,
        mapping_shapes_ttl = None,
        statement_shapes_ttl = None,
        project_root = None,
        gts_bytes = None,
        signature_config = None,
    ))]
    fn new(
        timings: bool,
        sameas_allowlist: Option<Vec<(String, String)>>,
        slices_dir: Option<String>,
        mapping_shapes_ttl: Option<String>,
        statement_shapes_ttl: Option<String>,
        project_root: Option<String>,
        gts_bytes: Option<Vec<u8>>,
        signature_config: Option<PySignatureConfig>,
    ) -> Self {
        Self {
            timings,
            sameas_allowlist: sameas_allowlist.unwrap_or_default(),
            slices_dir,
            mapping_shapes_ttl,
            statement_shapes_ttl,
            project_root,
            gts_bytes,
            signature_config,
        }
    }
}

impl PyValidateOptions {
    fn to_engine(&self) -> ValidateOptions {
        ValidateOptions {
            timings: self.timings,
            sameas_allowlist: self.sameas_allowlist.clone(),
            slices_dir: self.slices_dir.clone(),
            mapping_shapes_ttl: self.mapping_shapes_ttl.clone(),
            statement_shapes_ttl: self.statement_shapes_ttl.clone(),
            project_root: self.project_root.as_ref().map(PathBuf::from),
            gts_bytes: self.gts_bytes.clone(),
            signature_config: self.signature_config.as_ref().map(|c| c.to_engine()),
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

    /// Internal protocol: a capsule exposing the wrapped oxigraph store by
    /// address.
    ///
    /// The capsule is consumed by `gmeow_shacl.Shapes.validate_store` so the
    /// SHACL engine can validate the store directly without an N-Triples
    /// round-trip. Do not call this directly from Python.
    ///
    /// The capsule's destructor owns a strong reference to `self`, so the store
    /// is kept alive for the capsule's entire lifetime — the borrow is *enforced*
    /// rather than merely assumed, closing the use-after-free a stray Python
    /// reference to the capsule would otherwise open.
    fn _store_capsule<'py>(slf: &Bound<'py, Self>) -> PyResult<Bound<'py, PyCapsule>> {
        let py = slf.py();
        let addr = &slf.borrow().store as *const oxigraph::store::Store as usize;
        // Strong ref to the Python store; dropped (under the GIL) only when the
        // capsule itself is collected, so `self.store` cannot dangle beneath it.
        let keepalive: Py<Self> = slf.clone().unbind();
        // SAFETY: the capsule's value is the address of `self.store`, whose
        // storage is stable for the lifetime of the pyclass instance that
        // `keepalive` pins. The consumer reads only the value, never the context.
        PyCapsule::new_with_value_and_destructor(
            py,
            addr,
            c"gmeow-validation-store",
            move |_addr, _ctx| drop(keepalive),
        )
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
/// `declared_terms`, and `report` — the single canonical diagnostics report as a
/// **live `Report` pyclass** (not a JSON string), from which `errors`/`warnings`
/// are derived and which carries SHACL focus nodes and GTS wire coordinates
/// (#654). Returning the live object lets Python fold its filesystem-bound lints
/// in and render SARIF directly, with no JSON round-trip — sound now that all
/// bindings share one `Report` type in the `gmeow_native` cdylib (#630).
/// Optional phases (example coverage/SHACL, DSL SHACL) are enabled by the
/// corresponding fields in `options`.
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

    let out = PyDict::new(py);
    out.set_item("errors", PyList::new(py, run.errors())?)?;
    out.set_item("warnings", PyList::new(py, run.warnings())?)?;
    out.set_item("declared_terms", PyList::new(py, &run.declared_terms)?)?;

    let timings = PyList::empty(py);
    for t in &run.timings {
        let d = PyDict::new(py);
        d.set_item("phase", &t.phase)?;
        d.set_item("elapsed_ms", t.elapsed_ms)?;
        d.set_item("metadata", t.metadata.as_deref().unwrap_or(""))?;
        timings.append(d)?;
    }
    out.set_item("timings", timings)?;

    // Hand Python the single canonical report as a LIVE pyclass, not a JSON
    // string — Python folds its few filesystem-bound lints straight into it and
    // renders SARIF/JSON/HTML from it directly. No serialize→`from_json`→
    // `to_json` round-trip (#630). Built last: it moves `run.report`, after the
    // borrows above are done.
    out.set_item("report", Py::new(py, PyReport::from_engine(run.report))?)?;

    Ok(out.into_any().unbind())
}

/// Load a Turtle string into `store` (lenient parsing, matching the rest of the
/// validation path), mapping a parse failure to a Python `ValueError`.
fn insert_turtle(store: &oxigraph::store::Store, ttl: &str) -> PyResult<()> {
    use oxigraph::io::{RdfFormat, RdfParser};
    for triple in RdfParser::from_format(RdfFormat::Turtle)
        .lenient()
        .for_reader(ttl.as_bytes())
    {
        let triple = triple.map_err(|e| {
            pyo3::exceptions::PyValueError::new_err(format!("Turtle parse error: {e}"))
        })?;
        store
            .insert(&triple)
            .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))?;
    }
    Ok(())
}

/// Load an N-Triples string into `store` (lenient parsing), mapping a parse
/// failure to a Python `ValueError`.
fn insert_ntriples(store: &oxigraph::store::Store, data_nt: &str) -> PyResult<()> {
    use oxigraph::io::{RdfFormat, RdfParser};
    for triple in RdfParser::from_format(RdfFormat::NTriples)
        .lenient()
        .for_reader(data_nt.as_bytes())
    {
        let triple = triple.map_err(|e| {
            pyo3::exceptions::PyValueError::new_err(format!("N-Triples parse error: {e}"))
        })?;
        store
            .insert(&triple)
            .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))?;
    }
    Ok(())
}

/// The statement-metadata invariants over the emitted OWL downcast + ontology
/// (mirrors `statement_lint.statement_invariants`, #630 Gap B3).
///
/// `statement_owl_ttl` is the native statement-stage OWL downcast as Turtle;
/// `ontology_nt` is the merged ontology as N-Triples. Both are loaded into ONE
/// oxigraph store (their default-graph union), the four invariants run natively,
/// and the violations are returned as a single canonical `Report` pyclass (every
/// finding is an `Error` — each blocks statement compilation).
/// Returning the live `Report` lets Python join the messages, render SARIF, or
/// fold the findings in without a JSON round-trip (#630/#654).
#[pyfunction]
fn check_statement_invariants(
    py: Python<'_>,
    statement_owl_ttl: &str,
    ontology_nt: &str,
) -> PyResult<Py<PyAny>> {
    let store = oxigraph::store::Store::new()
        .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))?;
    insert_turtle(&store, statement_owl_ttl)?;
    insert_ntriples(&store, ontology_nt)?;

    let mut report = gmeow_diagnostics::Report::new("statement");
    for finding in statement::check_statement_invariants(&store) {
        report.add_finding(finding);
    }
    Ok(Py::new(py, PyReport::from_engine(report))?.into_any())
}

/// Native RDF-1.2 ↔ OWL round-trip lossless check (#809).
///
/// `authored_owl_ttl` is the OWL downcast emitted from the statement DSL;
/// `normalized_owl_ttl` is the RDF 1.2 lead artifact normalized back to the OWL
/// normal form (via `gmeow_rdf.normalize_rdf12_to_owl`). Returns a `Report` whose
/// findings are the diverging triples (empty == lossless); the divergence is
/// computed natively over oxigraph quad sets, not rdflib `graph_diff`.
#[pyfunction]
fn check_statement_lossless(
    py: Python<'_>,
    authored_owl_ttl: &str,
    normalized_owl_ttl: &str,
) -> PyResult<Py<PyAny>> {
    let authored = oxigraph::store::Store::new()
        .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))?;
    insert_turtle(&authored, authored_owl_ttl)?;
    let normalized = oxigraph::store::Store::new()
        .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))?;
    insert_turtle(&normalized, normalized_owl_ttl)?;

    let mut report = gmeow_diagnostics::Report::new("statement-compile");
    for finding in statement::check_statement_lossless(&authored, &normalized) {
        report.add_finding(finding);
    }
    Ok(Py::new(py, PyReport::from_engine(report))?.into_any())
}

/// Native constitution enforcement-coverage check → diagnostics `Report` (#809).
///
/// Parses the manifest Turtle (`governance/constitution.ttl`) into a Store and
/// runs the graph-resident enforcement-coverage check, emitting granular
/// `constitution.{principle-unenforced,honor-system,orphaned-enforcement,
/// undeclared-enforcement}` findings. The non-graph constitution checks remain in
/// Python (they introspect the filesystem / Typer app / generator registry).
#[pyfunction]
fn constitution_enforcement_report(py: Python<'_>, manifest_ttl: &str) -> PyResult<Py<PyAny>> {
    let store = oxigraph::store::Store::new()
        .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))?;
    insert_turtle(&store, manifest_ttl)?;

    let mut report = gmeow_diagnostics::Report::new("constitution");
    for finding in constitution::check_enforcement_coverage(&store) {
        report.add_finding(finding);
    }
    Ok(Py::new(py, PyReport::from_engine(report))?.into_any())
}

/// Native full constitution-as-code report → diagnostics `Report` (#939).
///
/// Runs every constitution check: enforcement coverage, principle/heading sync,
/// cited artifact/symbol/target/CLI existence, and supersession marker sync.
#[pyfunction]
fn constitution_full_report(
    py: Python<'_>,
    manifest_path: &str,
    constitution_path: &str,
    root: &str,
) -> PyResult<Py<PyAny>> {
    let manifest = std::path::Path::new(manifest_path);
    let constitution = std::path::Path::new(constitution_path);
    let root = std::path::Path::new(root);
    let findings = constitution::constitution_full_report(manifest, constitution, root);
    let mut report = gmeow_diagnostics::Report::new("constitution");
    for finding in findings {
        report.add_finding(finding);
    }
    Ok(Py::new(py, PyReport::from_engine(report))?.into_any())
}

/// Native slice-ownership analysis projected into a diagnostics `Report` (#809).
///
/// Discovers the slice catalog under `slices_root`, runs the native `gmeow-slice`
/// ownership + dependency analysis, and projects the structured `OwnershipReport`
/// into canonical findings — replacing the old `ownership_errors() -> list[str]`
/// collapse. Ownership defects (conflict / mismatch / unowned) are error findings
/// (the validate gate); dependency observations are warnings (previously dropped).
#[pyfunction]
fn slice_ownership_report(py: Python<'_>, slices_root: &str) -> PyResult<Py<PyAny>> {
    let catalog = gmeow_slice::SliceCatalog::discover(std::path::Path::new(slices_root))
        .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))?;
    let analysis = gmeow_slice::OwnershipAnalyzer::new(&catalog)
        .analyze()
        .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))?;

    let mut report = gmeow_diagnostics::Report::new("slice-ownership");
    for finding in slice_ownership::ownership_findings(&analysis) {
        report.add_finding(finding);
    }
    Ok(Py::new(py, PyReport::from_engine(report))?.into_any())
}

/// Return whether `lang` is a GMEOW internal private-use tag (``x-gmeow-*``).
///
/// Matches ``^x-gmeow-[a-z0-9\-]+$`` case-insensitively. This is the Rust
/// authority for the policy defined in ``language_tags.py``.
#[pyfunction]
fn is_internal_tag(lang: &str) -> bool {
    language_tags::is_internal_tag(lang)
}

/// The shared language-preference sort key.
///
/// Returns ``(0, lang.lower())`` for ``x-gmeow-english`` and
/// ``(1, lang.lower())`` for everything else so the carrier language wins
/// deterministically. Mirrors ``language_tags.rank_language``.
#[pyfunction]
fn rank_language(lang: &str) -> (u8, String) {
    language_tags::rank_language(lang)
}

/// Parse RDF bytes and build the ``{internal_tag: bcp47_tag}`` mapping.
///
/// `rdf_bytes` is raw RDF data; `format` is a format string accepted by
/// [`crate::language_tags::load_tag_map`] (``"turtle"``, ``"ntriples"``, etc.).
/// Maps ambiguous tags (> 1 distinct value) to a ``ValueError``; missing tags
/// silently skip the individual (SHACL enforces completeness).
///
/// Returns a Python ``dict[str, str]``.
#[pyfunction]
fn load_tag_map(py: Python<'_>, rdf_bytes: &[u8], format: &str) -> PyResult<Py<PyAny>> {
    let map = language_tags::load_tag_map(rdf_bytes, format)
        .map_err(pyo3::exceptions::PyValueError::new_err)?;
    let d = PyDict::new(py);
    for (k, v) in &map {
        d.set_item(k, v)?;
    }
    Ok(d.into_any().unbind())
}

/// Build the CrossRef deposit XML from a JSON-serialised ``DepositInput``.
///
/// The JSON string is produced by the Python helper
/// ``crossref._to_deposit_input_json``, which bundles the ``SelfDescription``
/// and all ``config.py`` constants the generator needs. Returns the deposit
/// document as a UTF-8 string (with XML declaration).
#[pyfunction]
fn build_deposit_xml_native(
    self_description_json: String,
    timestamp: String,
    batch_id: String,
) -> PyResult<String> {
    crossref::build_deposit_xml(&self_description_json, &timestamp, &batch_id)
        .map_err(pyo3::exceptions::PyValueError::new_err)
}

/// Validate a JSON/YAML instance document against a JSON Schema (#700 Task 4).
///
/// `instance_bytes` is the raw instance file; `format` is ``"json"`` or
/// ``"yaml"`` (an unknown value maps to a Python ``ValueError``); `schema_bytes`
/// is the draft-2020-12 JSON Schema (the SHACL-derived `gmeow.schema.json`, or a
/// user-supplied schema). Returns the standard ``{"errors": [...],
/// "warnings": []}`` report dict — each violation is an error string, warnings is
/// always empty. A hard failure (malformed schema, unparsable instance) raises a
/// ``ValueError``.
#[pyfunction]
fn validate_instance(
    py: Python<'_>,
    instance_bytes: &[u8],
    format: &str,
    schema_bytes: &[u8],
) -> PyResult<Py<PyAny>> {
    let fmt = match format {
        "json" => InstanceFormat::Json,
        "yaml" => InstanceFormat::Yaml,
        other => {
            return Err(pyo3::exceptions::PyValueError::new_err(format!(
                "validate_instance: unknown format {other:?} (expected \"json\" or \"yaml\")"
            )));
        }
    };
    let errors = instance::validate_instance(instance_bytes, fmt, schema_bytes)
        .map_err(pyo3::exceptions::PyValueError::new_err)?;
    report_dict(py, errors, Vec::new())
}

/// Return DOI consistency problems from a JSON-serialised ``LintInput``.
///
/// The JSON string is produced by the Python helper
/// ``crossref._to_lint_input_json``, which bundles the ``SelfDescription``,
/// config constants, and the pre-read CITATION.cff / ontology-file texts so
/// the Rust side never does I/O. Returns ``[]`` when the deposit is sound.
#[pyfunction]
fn lint_deposit_native(self_description_json: String) -> PyResult<Vec<String>> {
    crossref::lint_deposit(&self_description_json).map_err(pyo3::exceptions::PyValueError::new_err)
}

/// Register the `gmeow-validate` surface on a Python module.
///
/// Exposes the syntax / sameAs lints (Task 1) plus the structural, naming,
/// ownership, and declared-term lints (Task 2, #579), and the `LintConfig` type
/// that carries their typed configuration across the FFI boundary. Called by the
/// unified `gmeow_native` cdylib (#630) to populate `gmeow_native.validate`.
pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PyLintConfig>()?;
    m.add_class::<PySignatureConfig>()?;
    m.add_class::<PyValidateOptions>()?;
    m.add_class::<PyValidationStore>()?;
    m.add_function(wrap_pyfunction!(annotation_predicates, m)?)?;
    m.add_function(wrap_pyfunction!(is_internal_tag, m)?)?;
    m.add_function(wrap_pyfunction!(rank_language, m)?)?;
    m.add_function(wrap_pyfunction!(load_tag_map, m)?)?;
    m.add_function(wrap_pyfunction!(check_syntax, m)?)?;
    m.add_function(wrap_pyfunction!(check_sameas_ban, m)?)?;
    m.add_function(wrap_pyfunction!(structural_lint, m)?)?;
    m.add_function(wrap_pyfunction!(term_naming_lint, m)?)?;
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
    m.add_function(wrap_pyfunction!(check_statement_invariants, m)?)?;
    m.add_function(wrap_pyfunction!(check_statement_lossless, m)?)?;
    m.add_function(wrap_pyfunction!(slice_ownership_report, m)?)?;
    m.add_function(wrap_pyfunction!(constitution_enforcement_report, m)?)?;
    m.add_function(wrap_pyfunction!(constitution_full_report, m)?)?;
    m.add_function(wrap_pyfunction!(build_deposit_xml_native, m)?)?;
    m.add_function(wrap_pyfunction!(lint_deposit_native, m)?)?;
    m.add_function(wrap_pyfunction!(validate_instance, m)?)?;
    Ok(())
}
