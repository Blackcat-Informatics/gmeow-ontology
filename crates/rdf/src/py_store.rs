// SPDX-FileCopyrightText: 2026 Blackcat Informatics Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Native oxigraph-backed Store / SPARQL / parse / canonicalize surface for the
//! `gmeow_rdf` Python extension — the in-repo replacement for the external
//! `pyoxigraph` package (#667).
//!
//! # Why this exists
//!
//! `pyoxigraph` is *literally the Python binding to oxigraph*, the same engine
//! every gmeow-* crate already links (`oxigraph 0.5`, `rdf-12`). Depending on it
//! is depending on an externally-versioned copy of an engine we own. This module
//! exposes the Store + SPARQL (SELECT / ASK / CONSTRUCT, variable substitution) +
//! `parse` / `serialize` + RDFC-1.0 canonicalization surface our Python layer
//! needs, so `make check` / CI / the build run with **no external RDF runtime**
//! (CONSTITUTION Principle 18).
//!
//! # Kernel-clean separation
//!
//! Like [`crate::py`], this module is compiled **only under the `python`
//! feature**. The RDF kernel ([`crate::model`], [`crate::store`],
//! [`crate::oxigraph`]) stays PyO3-free.
//!
//! # Design
//!
//! * **Eager materialization** — `Store.query` collects results into owned
//!   `Vec`s before returning, because oxigraph's `QueryResults<'a>` borrows the
//!   store and cannot live inside a `'static` `#[pyclass]`.
//! * **Pure-Rust cores** — [`parse_quads`] and [`canonicalize_quads`] hold the
//!   load-bearing logic and are unit-tested without a Python interpreter; the
//!   `#[pymethods]` are thin wrappers over them.
//! * **Faithful object model** — the term/result classes mirror the slice of the
//!   `pyoxigraph` API the codebase relies on, so the Python migration is a
//!   mechanical import swap rather than a rewrite of ~150 call sites.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use oxigraph::io::{RdfFormat, RdfParser, RdfSerializer};
use oxigraph::model::dataset::{CanonicalizationAlgorithm, CanonicalizationHashAlgorithm};
use oxigraph::model::{
    BlankNode, Dataset, GraphName, Literal, NamedNode, NamedOrBlankNode, Quad, Term, Triple,
    Variable,
};
use oxigraph::sparql::{QueryResults, SparqlEvaluator};
use oxigraph::store::Store;
use pyo3::exceptions::{PyKeyError, PyTypeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyCapsule, PyDict, PyString};

// ── RDF serialization format enum ───────────────────────────────────────────────

/// The RDF serialization formats the codebase loads/parses/serializes.
///
/// Mirrors `pyoxigraph.RdfFormat`; the members keep the SCREAMING_SNAKE Python
/// spelling (`RdfFormat.TURTLE`).
#[pyclass(name = "RdfFormat", eq, eq_int, from_py_object)]
#[derive(Clone, Copy, PartialEq, Eq)]
#[allow(non_camel_case_types)]
pub enum PyRdfFormat {
    TURTLE,
    N_TRIPLES,
    N_QUADS,
    TRIG,
}

impl PyRdfFormat {
    fn to_ox(self) -> RdfFormat {
        match self {
            PyRdfFormat::TURTLE => RdfFormat::Turtle,
            PyRdfFormat::N_TRIPLES => RdfFormat::NTriples,
            PyRdfFormat::N_QUADS => RdfFormat::NQuads,
            PyRdfFormat::TRIG => RdfFormat::TriG,
        }
    }
}

/// The graph canonicalization algorithms. Mirrors
/// `pyoxigraph.CanonicalizationAlgorithm`.
#[pyclass(name = "CanonicalizationAlgorithm", eq, eq_int, from_py_object)]
#[derive(Clone, Copy, PartialEq, Eq)]
#[allow(non_camel_case_types)]
pub enum PyCanonicalizationAlgorithm {
    /// The standard RDF Canonicalization 1.0 algorithm (SHA-256).
    RDFC_1_0,
    /// OxRDF's faster non-stable algorithm (canonical *within* a build/version).
    UNSTABLE,
}

impl PyCanonicalizationAlgorithm {
    fn to_ox(self) -> CanonicalizationAlgorithm {
        match self {
            PyCanonicalizationAlgorithm::RDFC_1_0 => CanonicalizationAlgorithm::Rdfc10 {
                hash_algorithm: CanonicalizationHashAlgorithm::Sha256,
            },
            PyCanonicalizationAlgorithm::UNSTABLE => CanonicalizationAlgorithm::Unstable,
        }
    }
}

// ── Term model ──────────────────────────────────────────────────────────────────

fn hash_str(value: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    value.hash(&mut hasher);
    hasher.finish()
}

/// An IRI node. Mirrors `pyoxigraph.NamedNode`.
#[pyclass(name = "NamedNode", frozen, skip_from_py_object)]
#[derive(Clone)]
pub struct PyNamedNode {
    pub(crate) inner: NamedNode,
}

#[pymethods]
impl PyNamedNode {
    #[new]
    fn new(value: &str) -> PyResult<Self> {
        Ok(Self {
            inner: NamedNode::new(value)
                .map_err(|e| PyValueError::new_err(format!("invalid IRI `{value}`: {e}")))?,
        })
    }

    /// The IRI string (no angle brackets).
    #[getter]
    fn value(&self) -> &str {
        self.inner.as_str()
    }

    fn __str__(&self) -> String {
        self.inner.to_string()
    }

    fn __repr__(&self) -> String {
        format!("<NamedNode value={}>", self.inner.as_str())
    }

    fn __eq__(&self, other: &Self) -> bool {
        self.inner == other.inner
    }

    fn __hash__(&self) -> u64 {
        hash_str(self.inner.as_str())
    }
}

/// A blank node. Mirrors `pyoxigraph.BlankNode`.
#[pyclass(name = "BlankNode", frozen, skip_from_py_object)]
#[derive(Clone)]
pub struct PyBlankNode {
    pub(crate) inner: BlankNode,
}

#[pymethods]
impl PyBlankNode {
    #[new]
    fn new(value: &str) -> PyResult<Self> {
        Ok(Self {
            inner: BlankNode::new(value)
                .map_err(|e| PyValueError::new_err(format!("invalid blank node `{value}`: {e}")))?,
        })
    }

    /// The blank-node id (no `_:` prefix).
    #[getter]
    fn value(&self) -> &str {
        self.inner.as_str()
    }

    fn __str__(&self) -> String {
        self.inner.to_string()
    }

    fn __repr__(&self) -> String {
        format!("<BlankNode value={}>", self.inner.as_str())
    }

    fn __eq__(&self, other: &Self) -> bool {
        self.inner == other.inner
    }

    fn __hash__(&self) -> u64 {
        hash_str(self.inner.as_str())
    }
}

/// An RDF literal. Mirrors `pyoxigraph.Literal`.
#[pyclass(name = "Literal", frozen, skip_from_py_object)]
#[derive(Clone)]
pub struct PyLiteral {
    pub(crate) inner: Literal,
}

#[pymethods]
impl PyLiteral {
    #[new]
    #[pyo3(signature = (value, *, datatype=None, language=None))]
    fn new(
        value: String,
        datatype: Option<&PyNamedNode>,
        language: Option<String>,
    ) -> PyResult<Self> {
        let inner = if let Some(language) = language {
            if datatype.is_some() {
                return Err(PyValueError::new_err(
                    "a language-tagged literal cannot also carry an explicit datatype",
                ));
            }
            Literal::new_language_tagged_literal(value, &language)
                .map_err(|e| PyValueError::new_err(format!("invalid language tag: {e}")))?
        } else if let Some(datatype) = datatype {
            Literal::new_typed_literal(value, datatype.inner.clone())
        } else {
            Literal::new_simple_literal(value)
        };
        Ok(Self { inner })
    }

    /// The lexical form (no datatype/language decoration).
    #[getter]
    fn value(&self) -> &str {
        self.inner.value()
    }

    /// The language tag, or `None` for a non-language-tagged literal.
    #[getter]
    fn language(&self) -> Option<&str> {
        self.inner.language()
    }

    /// The datatype IRI (always present — `xsd:string` for a plain literal,
    /// `rdf:langString` for a language-tagged one), matching pyoxigraph.
    #[getter]
    fn datatype(&self) -> PyNamedNode {
        PyNamedNode {
            inner: self.inner.datatype().into_owned(),
        }
    }

    fn __str__(&self) -> String {
        self.inner.to_string()
    }

    fn __repr__(&self) -> String {
        format!("<Literal {}>", self.inner)
    }

    fn __eq__(&self, other: &Self) -> bool {
        self.inner == other.inner
    }

    fn __hash__(&self) -> u64 {
        hash_str(&self.inner.to_string())
    }
}

/// A quoted triple term (RDF 1.2 / RDF-star). Mirrors `pyoxigraph.Triple`.
#[pyclass(name = "Triple", frozen, skip_from_py_object)]
#[derive(Clone)]
pub struct PyTriple {
    pub(crate) inner: Triple,
}

#[pymethods]
impl PyTriple {
    #[new]
    fn new(
        py: Python<'_>,
        subject: &Bound<'_, PyAny>,
        predicate: &Bound<'_, PyAny>,
        object: &Bound<'_, PyAny>,
    ) -> PyResult<Self> {
        let _ = py;
        Ok(Self {
            inner: Triple::new(
                extract_subject(subject)?,
                extract_named_node(predicate)?,
                extract_term(object)?,
            ),
        })
    }

    #[getter]
    fn subject(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        subject_to_py(py, &self.inner.subject)
    }

    #[getter]
    fn predicate(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        Py::new(
            py,
            PyNamedNode {
                inner: self.inner.predicate.clone(),
            },
        )
        .map(|n| n.into_any())
    }

    #[getter]
    fn object(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        term_to_py(py, &self.inner.object)
    }

    fn __str__(&self) -> String {
        self.inner.to_string()
    }

    fn __repr__(&self) -> String {
        format!("<Triple {}>", self.inner)
    }

    fn __eq__(&self, other: &Self) -> bool {
        self.inner == other.inner
    }

    fn __hash__(&self) -> u64 {
        hash_str(&self.inner.to_string())
    }
}

/// An RDF quad. Mirrors `pyoxigraph.Quad`.
#[pyclass(name = "Quad", frozen, skip_from_py_object)]
#[derive(Clone)]
pub struct PyQuad {
    pub(crate) inner: Quad,
}

#[pymethods]
impl PyQuad {
    #[new]
    #[pyo3(signature = (subject, predicate, object, graph_name=None))]
    fn new(
        subject: &Bound<'_, PyAny>,
        predicate: &Bound<'_, PyAny>,
        object: &Bound<'_, PyAny>,
        graph_name: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<Self> {
        Ok(Self {
            inner: Quad::new(
                extract_subject(subject)?,
                extract_named_node(predicate)?,
                extract_term(object)?,
                extract_graph_name(graph_name)?,
            ),
        })
    }

    #[getter]
    fn subject(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        subject_to_py(py, &self.inner.subject)
    }

    #[getter]
    fn predicate(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        Py::new(
            py,
            PyNamedNode {
                inner: self.inner.predicate.clone(),
            },
        )
        .map(|n| n.into_any())
    }

    #[getter]
    fn object(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        term_to_py(py, &self.inner.object)
    }

    #[getter]
    fn graph_name(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        graph_name_to_py(py, &self.inner.graph_name)
    }

    fn __str__(&self) -> String {
        self.inner.to_string()
    }

    fn __repr__(&self) -> String {
        format!("<Quad {}>", self.inner)
    }

    fn __eq__(&self, other: &Self) -> bool {
        self.inner == other.inner
    }

    fn __hash__(&self) -> u64 {
        hash_str(&self.inner.to_string())
    }
}

/// A default-graph marker term. Mirrors `pyoxigraph.DefaultGraph`.
#[pyclass(name = "DefaultGraph", frozen, skip_from_py_object)]
#[derive(Clone, Default)]
pub struct PyDefaultGraph;

#[pymethods]
impl PyDefaultGraph {
    #[new]
    fn new() -> Self {
        Self
    }

    fn __str__(&self) -> &'static str {
        "DEFAULT"
    }

    fn __eq__(&self, _other: &Self) -> bool {
        true
    }

    fn __hash__(&self) -> u64 {
        0
    }
}

/// A SPARQL variable, used to key query substitutions. Mirrors
/// `pyoxigraph.Variable`.
#[pyclass(name = "Variable", frozen, skip_from_py_object)]
#[derive(Clone)]
pub struct PyVariable {
    pub(crate) inner: Variable,
}

#[pymethods]
impl PyVariable {
    #[new]
    fn new(value: &str) -> PyResult<Self> {
        Ok(Self {
            inner: Variable::new(value)
                .map_err(|e| PyValueError::new_err(format!("invalid variable `{value}`: {e}")))?,
        })
    }

    #[getter]
    fn value(&self) -> &str {
        self.inner.as_str()
    }

    fn __str__(&self) -> String {
        self.inner.to_string()
    }

    fn __eq__(&self, other: &Self) -> bool {
        self.inner == other.inner
    }

    fn __hash__(&self) -> u64 {
        hash_str(self.inner.as_str())
    }
}

// ── Term ⇄ Python conversions ────────────────────────────────────────────────────

fn term_to_py(py: Python<'_>, term: &Term) -> PyResult<Py<PyAny>> {
    Ok(match term {
        Term::NamedNode(n) => Py::new(py, PyNamedNode { inner: n.clone() })?.into_any(),
        Term::BlankNode(b) => Py::new(py, PyBlankNode { inner: b.clone() })?.into_any(),
        Term::Literal(l) => Py::new(py, PyLiteral { inner: l.clone() })?.into_any(),
        Term::Triple(t) => Py::new(
            py,
            PyTriple {
                inner: (**t).clone(),
            },
        )?
        .into_any(),
    })
}

fn subject_to_py(py: Python<'_>, subject: &NamedOrBlankNode) -> PyResult<Py<PyAny>> {
    Ok(match subject {
        NamedOrBlankNode::NamedNode(n) => Py::new(py, PyNamedNode { inner: n.clone() })?.into_any(),
        NamedOrBlankNode::BlankNode(b) => Py::new(py, PyBlankNode { inner: b.clone() })?.into_any(),
    })
}

fn graph_name_to_py(py: Python<'_>, graph_name: &GraphName) -> PyResult<Py<PyAny>> {
    Ok(match graph_name {
        GraphName::NamedNode(n) => Py::new(py, PyNamedNode { inner: n.clone() })?.into_any(),
        GraphName::BlankNode(b) => Py::new(py, PyBlankNode { inner: b.clone() })?.into_any(),
        GraphName::DefaultGraph => Py::new(py, PyDefaultGraph)?.into_any(),
    })
}

fn extract_term(obj: &Bound<'_, PyAny>) -> PyResult<Term> {
    if let Ok(n) = obj.cast::<PyNamedNode>() {
        return Ok(Term::NamedNode(n.get().inner.clone()));
    }
    if let Ok(b) = obj.cast::<PyBlankNode>() {
        return Ok(Term::BlankNode(b.get().inner.clone()));
    }
    if let Ok(l) = obj.cast::<PyLiteral>() {
        return Ok(Term::Literal(l.get().inner.clone()));
    }
    if let Ok(t) = obj.cast::<PyTriple>() {
        return Ok(Term::Triple(Box::new(t.get().inner.clone())));
    }
    Err(PyTypeError::new_err(
        "expected an RDF term (NamedNode, BlankNode, Literal, or Triple)",
    ))
}

fn extract_subject(obj: &Bound<'_, PyAny>) -> PyResult<NamedOrBlankNode> {
    if let Ok(n) = obj.cast::<PyNamedNode>() {
        return Ok(NamedOrBlankNode::NamedNode(n.get().inner.clone()));
    }
    if let Ok(b) = obj.cast::<PyBlankNode>() {
        return Ok(NamedOrBlankNode::BlankNode(b.get().inner.clone()));
    }
    Err(PyTypeError::new_err(
        "a subject must be a NamedNode or BlankNode",
    ))
}

fn extract_named_node(obj: &Bound<'_, PyAny>) -> PyResult<NamedNode> {
    obj.cast::<PyNamedNode>()
        .map(|n| n.get().inner.clone())
        .map_err(|_| PyTypeError::new_err("a predicate must be a NamedNode"))
}

fn extract_graph_name(obj: Option<&Bound<'_, PyAny>>) -> PyResult<GraphName> {
    let Some(obj) = obj else {
        return Ok(GraphName::DefaultGraph);
    };
    if obj.is_none() || obj.cast::<PyDefaultGraph>().is_ok() {
        return Ok(GraphName::DefaultGraph);
    }
    if let Ok(n) = obj.cast::<PyNamedNode>() {
        return Ok(GraphName::NamedNode(n.get().inner.clone()));
    }
    if let Ok(b) = obj.cast::<PyBlankNode>() {
        return Ok(GraphName::BlankNode(b.get().inner.clone()));
    }
    Err(PyTypeError::new_err(
        "a graph name must be a NamedNode, BlankNode, or DefaultGraph",
    ))
}

// ── Query result types ──────────────────────────────────────────────────────────

/// SELECT results, materialized. Mirrors `pyoxigraph.QuerySolutions`.
#[pyclass(name = "QuerySolutions")]
pub struct PyQuerySolutions {
    variables: Vec<Variable>,
    rows: Vec<Vec<Option<Term>>>,
    pos: usize,
}

#[pymethods]
impl PyQuerySolutions {
    /// The bound variables, in projection order.
    #[getter]
    fn variables(&self, py: Python<'_>) -> PyResult<Vec<Py<PyVariable>>> {
        self.variables
            .iter()
            .map(|v| Py::new(py, PyVariable { inner: v.clone() }))
            .collect()
    }

    fn __iter__(slf: PyRef<'_, Self>) -> PyRef<'_, Self> {
        slf
    }

    fn __next__(
        mut slf: PyRefMut<'_, Self>,
        py: Python<'_>,
    ) -> PyResult<Option<Py<PyQuerySolution>>> {
        if slf.pos >= slf.rows.len() {
            return Ok(None);
        }
        let row = slf.rows[slf.pos].clone();
        let variables = slf.variables.clone();
        slf.pos += 1;
        Ok(Some(Py::new(py, PyQuerySolution { variables, row })?))
    }

    fn __len__(&self) -> usize {
        self.rows.len()
    }
}

/// A single SELECT solution row. Mirrors `pyoxigraph.QuerySolution`.
#[pyclass(name = "QuerySolution")]
pub struct PyQuerySolution {
    variables: Vec<Variable>,
    row: Vec<Option<Term>>,
}

#[pymethods]
impl PyQuerySolution {
    /// Look a binding up by variable name (`str`), `Variable`, or position
    /// (`int`). An unbound variable yields `None`; an unknown name is a
    /// `KeyError`, matching pyoxigraph.
    fn __getitem__(&self, py: Python<'_>, key: &Bound<'_, PyAny>) -> PyResult<Option<Py<PyAny>>> {
        let index = if let Ok(i) = key.extract::<usize>() {
            if i >= self.row.len() {
                return Err(PyKeyError::new_err(format!("no variable at position {i}")));
            }
            i
        } else {
            let name = if let Ok(var) = key.cast::<PyVariable>() {
                var.get().inner.as_str().to_owned()
            } else if let Ok(s) = key.cast::<PyString>() {
                s.to_str()?.to_owned()
            } else {
                return Err(PyTypeError::new_err(
                    "solution key must be a str, Variable, or int",
                ));
            };
            self.variables
                .iter()
                .position(|v| v.as_str() == name)
                .ok_or_else(|| PyKeyError::new_err(format!("no variable named `{name}`")))?
        };
        match &self.row[index] {
            Some(term) => Ok(Some(term_to_py(py, term)?)),
            None => Ok(None),
        }
    }
}

/// CONSTRUCT results, materialized. Mirrors `pyoxigraph.QueryTriples`.
#[pyclass(name = "QueryTriples")]
pub struct PyQueryTriples {
    triples: Vec<Triple>,
    pos: usize,
}

#[pymethods]
impl PyQueryTriples {
    fn __iter__(slf: PyRef<'_, Self>) -> PyRef<'_, Self> {
        slf
    }

    fn __next__(mut slf: PyRefMut<'_, Self>, py: Python<'_>) -> PyResult<Option<Py<PyTriple>>> {
        if slf.pos >= slf.triples.len() {
            return Ok(None);
        }
        let triple = slf.triples[slf.pos].clone();
        slf.pos += 1;
        Ok(Some(Py::new(py, PyTriple { inner: triple })?))
    }

    fn __len__(&self) -> usize {
        self.triples.len()
    }

    /// Serialize the constructed triples to bytes in `format` (the N-Triples
    /// fast path the `sparql` seam uses for its rdflib hand-off).
    fn serialize<'py>(
        &self,
        py: Python<'py>,
        format: PyRdfFormat,
    ) -> PyResult<Bound<'py, PyBytes>> {
        let bytes = serialize_triples(&self.triples, format.to_ox())
            .map_err(|e| PyValueError::new_err(format!("serialize error: {e}")))?;
        Ok(PyBytes::new(py, &bytes))
    }
}

/// An ASK result. Mirrors `pyoxigraph.QueryBoolean`.
#[pyclass(name = "QueryBoolean")]
pub struct PyQueryBoolean {
    value: bool,
}

#[pymethods]
impl PyQueryBoolean {
    fn __bool__(&self) -> bool {
        self.value
    }

    fn __str__(&self) -> String {
        self.value.to_string()
    }

    fn __eq__(&self, other: bool) -> bool {
        self.value == other
    }

    fn __hash__(&self) -> u64 {
        u64::from(self.value)
    }
}

// ── Store ────────────────────────────────────────────────────────────────────────

/// An in-memory RDF 1.2 quad store with SPARQL. Mirrors `pyoxigraph.Store`.
#[pyclass(name = "Store")]
pub struct PyStore {
    inner: Store,
}

#[pymethods]
impl PyStore {
    #[new]
    fn new() -> PyResult<Self> {
        Ok(Self {
            inner: Store::new().map_err(store_err)?,
        })
    }

    /// Load RDF into the store. Either `input` (bytes/str data) or the keyword
    /// `path` (a file to read) must be given, together with `format`.
    #[pyo3(signature = (input=None, format=None, *, path=None))]
    fn load(
        &self,
        input: Option<&Bound<'_, PyAny>>,
        format: Option<PyRdfFormat>,
        path: Option<String>,
    ) -> PyResult<()> {
        let format = format.ok_or_else(|| PyValueError::new_err("load: format is required"))?;
        let data = read_input(input, path)?;
        self.inner
            .load_from_slice(RdfParser::from_format(format.to_ox()).lenient(), &data)
            .map_err(|e| PyValueError::new_err(format!("load error: {e}")))
    }

    /// Alias of [`load`] — oxigraph's bulk loader is a throughput optimization,
    /// not a different semantics, so the in-memory store path is identical.
    #[pyo3(signature = (input=None, format=None, *, path=None))]
    fn bulk_load(
        &self,
        input: Option<&Bound<'_, PyAny>>,
        format: Option<PyRdfFormat>,
        path: Option<String>,
    ) -> PyResult<()> {
        self.load(input, format, path)
    }

    /// Add a single quad.
    fn add(&self, quad: &PyQuad) -> PyResult<()> {
        self.inner.insert(&quad.inner).map_err(store_err)?;
        Ok(())
    }

    /// Run a SPARQL query. Returns `QuerySolutions` (SELECT), `QueryTriples`
    /// (CONSTRUCT/DESCRIBE), or `QueryBoolean` (ASK). Optional `substitutions`
    /// is a `{Variable: term}` mapping applied natively (never string-spliced).
    #[pyo3(signature = (query, *, substitutions=None))]
    fn query(
        &self,
        py: Python<'_>,
        query: &str,
        substitutions: Option<&Bound<'_, PyDict>>,
    ) -> PyResult<Py<PyAny>> {
        let mut evaluator = SparqlEvaluator::new()
            .parse_query(query)
            .map_err(|e| PyValueError::new_err(format!("query parse error: {e}")))?;
        if let Some(subs) = substitutions {
            for (key, value) in subs.iter() {
                let var = key
                    .cast::<PyVariable>()
                    .map_err(|_| PyTypeError::new_err("substitution keys must be Variable"))?
                    .get()
                    .inner
                    .clone();
                evaluator = evaluator.substitute_variable(var, extract_term(&value)?);
            }
        }
        let results = evaluator
            .on_store(&self.inner)
            .execute()
            .map_err(|e| PyValueError::new_err(format!("query evaluation error: {e}")))?;
        materialize_results(py, results)
    }

    /// Dump the whole store (or one graph, via `from_graph`) in `format`. Mirrors
    /// `pyoxigraph.Store.dump`: when `output` (a file-like with `.write`) is given
    /// the bytes are written to it and `None` is returned; otherwise the bytes are
    /// returned directly.
    #[pyo3(signature = (output=None, format=None, *, from_graph=None))]
    fn dump(
        &self,
        py: Python<'_>,
        output: Option<&Bound<'_, PyAny>>,
        format: Option<PyRdfFormat>,
        from_graph: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<Option<Py<PyBytes>>> {
        let format = format.ok_or_else(|| PyValueError::new_err("dump: format is required"))?;
        let ox_format = format.to_ox();
        let mut buf: Vec<u8> = Vec::new();
        if ox_format.supports_datasets() && from_graph.is_none() {
            self.inner
                .dump_to_writer(RdfSerializer::from_format(ox_format), &mut buf)
                .map_err(|e| PyValueError::new_err(format!("dump error: {e}")))?;
        } else {
            let graph = extract_graph_name(from_graph)?;
            self.inner
                .dump_graph_to_writer(
                    graph.as_ref(),
                    RdfSerializer::from_format(ox_format),
                    &mut buf,
                )
                .map_err(|e| PyValueError::new_err(format!("dump error: {e}")))?;
        }
        match output {
            Some(output) => {
                output.call_method1("write", (PyBytes::new(py, &buf),))?;
                Ok(None)
            }
            None => Ok(Some(PyBytes::new(py, &buf).unbind())),
        }
    }

    fn __len__(&self) -> PyResult<usize> {
        self.inner.len().map_err(store_err)
    }

    /// Iterate the store's quads (a snapshot taken at iteration time).
    fn __iter__(slf: PyRef<'_, Self>, py: Python<'_>) -> PyResult<Py<PyQuadIter>> {
        let mut quads = Vec::new();
        for quad in slf.inner.iter() {
            quads.push(quad.map_err(store_err)?);
        }
        Py::new(py, PyQuadIter { quads, pos: 0 })
    }

    /// Internal protocol: a transient capsule borrowing the wrapped oxigraph
    /// store, consumed immediately by `gmeow_shacl.Shapes.validate_store` so the
    /// SHACL engine validates this store with no N-Triples round-trip. Keeping
    /// the capsule alive past `self` is undefined behaviour — do not call from
    /// Python directly. The capsule name and pointee type match exactly what
    /// `gmeow_shacl` consumes from `gmeow_validate.ValidationStore`.
    fn _store_capsule<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyCapsule>> {
        let addr = &self.inner as *const Store as usize;
        // SAFETY: the capsule borrows `self.inner`; it must not outlive `self`.
        PyCapsule::new_with_value(py, addr, c"gmeow-validation-store")
    }
}

// ── Dataset (canonicalization) ───────────────────────────────────────────────────

/// An in-memory quad set supporting RDFC-1.0 canonicalization. Mirrors
/// `pyoxigraph.Dataset`.
#[pyclass(name = "Dataset")]
pub struct PyDataset {
    inner: Dataset,
}

#[pymethods]
impl PyDataset {
    /// Build a dataset, optionally seeding it from an iterable of `Quad`.
    #[new]
    #[pyo3(signature = (quads=None))]
    fn new(quads: Option<&Bound<'_, PyAny>>) -> PyResult<Self> {
        let mut inner = Dataset::new();
        if let Some(quads) = quads {
            for item in quads.try_iter()? {
                let item = item?;
                let quad = item
                    .cast::<PyQuad>()
                    .map_err(|_| PyTypeError::new_err("Dataset accepts an iterable of Quad"))?;
                inner.insert(&quad.get().inner);
            }
        }
        Ok(Self { inner })
    }

    /// Add a single quad.
    fn add(&mut self, quad: &PyQuad) {
        self.inner.insert(&quad.inner);
    }

    /// Canonicalize blank-node labels in place under `algorithm`.
    fn canonicalize(&mut self, algorithm: PyCanonicalizationAlgorithm) {
        self.inner.canonicalize(algorithm.to_ox());
    }

    fn __len__(&self) -> usize {
        self.inner.len()
    }

    fn __iter__(slf: PyRef<'_, Self>, py: Python<'_>) -> PyResult<Py<PyQuadIter>> {
        let quads: Vec<Quad> = slf.inner.iter().map(|q| q.into_owned()).collect();
        Py::new(py, PyQuadIter { quads, pos: 0 })
    }
}

/// Iterator over a [`PyDataset`]'s quads (snapshot at iteration time).
#[pyclass(name = "QuadIter")]
pub struct PyQuadIter {
    quads: Vec<Quad>,
    pos: usize,
}

#[pymethods]
impl PyQuadIter {
    fn __iter__(slf: PyRef<'_, Self>) -> PyRef<'_, Self> {
        slf
    }

    fn __next__(mut slf: PyRefMut<'_, Self>, py: Python<'_>) -> PyResult<Option<Py<PyQuad>>> {
        if slf.pos >= slf.quads.len() {
            return Ok(None);
        }
        let quad = slf.quads[slf.pos].clone();
        slf.pos += 1;
        Ok(Some(Py::new(py, PyQuad { inner: quad })?))
    }
}

// ── Module-level functions ──────────────────────────────────────────────────────

/// Parse RDF bytes/str into a list of `Quad`. Mirrors `pyoxigraph.parse`.
///
/// Unlike `Store.load`, blank-node labels are preserved verbatim (no renaming),
/// so canonicalization over the parsed quads is meaningful.
#[pyfunction]
#[pyo3(signature = (input, format))]
fn parse(
    py: Python<'_>,
    input: &Bound<'_, PyAny>,
    format: PyRdfFormat,
) -> PyResult<Vec<Py<PyQuad>>> {
    let data = read_input(Some(input), None)?;
    let quads = parse_quads(&data, format.to_ox())
        .map_err(|e| PyValueError::new_err(format!("parse error: {e}")))?;
    quads
        .into_iter()
        .map(|inner| Py::new(py, PyQuad { inner }))
        .collect()
}

/// Serialize `QueryTriples` in `format`. Mirrors `pyoxigraph.serialize`: when
/// `output` (a file-like with `.write`) is given the bytes are written to it and
/// `None` is returned; when `output` is omitted the serialized `bytes` are
/// returned directly.
#[pyfunction]
#[pyo3(signature = (input, output=None, format=None))]
fn serialize(
    py: Python<'_>,
    input: &PyQueryTriples,
    output: Option<&Bound<'_, PyAny>>,
    format: Option<PyRdfFormat>,
) -> PyResult<Option<Py<PyBytes>>> {
    let format = format.ok_or_else(|| PyValueError::new_err("serialize: format is required"))?;
    let bytes = serialize_triples(&input.triples, format.to_ox())
        .map_err(|e| PyValueError::new_err(format!("serialize error: {e}")))?;
    match output {
        Some(output) => {
            output.call_method1("write", (PyBytes::new(py, &bytes),))?;
            Ok(None)
        }
        None => Ok(Some(PyBytes::new(py, &bytes).unbind())),
    }
}

// ── Pure-Rust cores (unit-tested without a Python interpreter) ───────────────────

/// Parse RDF bytes into owned quads with lenient parsing (private-use language
/// tags such as `@x-gmeow-*` and version-specific strictness differences must
/// not break the load) and no blank-node renaming.
pub fn parse_quads(data: &[u8], format: RdfFormat) -> Result<Vec<Quad>, String> {
    let mut quads = Vec::new();
    for quad in RdfParser::from_format(format).lenient().for_slice(data) {
        quads.push(quad.map_err(|e| e.to_string())?);
    }
    Ok(quads)
}

/// Canonicalize a quad set's blank-node labels under `algorithm`, returning the
/// canonicalized quads (sorted by their N-Quads string for a stable order).
pub fn canonicalize_quads(quads: Vec<Quad>, algorithm: CanonicalizationAlgorithm) -> Vec<Quad> {
    let mut dataset: Dataset = quads.into_iter().collect();
    dataset.canonicalize(algorithm);
    let mut out: Vec<Quad> = dataset.iter().map(|q| q.into_owned()).collect();
    out.sort_by_key(Quad::to_string);
    out
}

fn serialize_triples(triples: &[Triple], format: RdfFormat) -> Result<Vec<u8>, String> {
    let mut serializer = RdfSerializer::from_format(format).for_writer(Vec::new());
    for triple in triples {
        serializer
            .serialize_triple(triple.as_ref())
            .map_err(|e| e.to_string())?;
    }
    serializer.finish().map_err(|e| e.to_string())
}

fn materialize_results(py: Python<'_>, results: QueryResults<'_>) -> PyResult<Py<PyAny>> {
    match results {
        QueryResults::Solutions(solutions) => {
            let variables = solutions.variables().to_vec();
            let mut rows: Vec<Vec<Option<Term>>> = Vec::new();
            for solution in solutions {
                let solution =
                    solution.map_err(|e| PyValueError::new_err(format!("solution error: {e}")))?;
                let row = variables
                    .iter()
                    .map(|v| solution.get(v.as_str()).cloned())
                    .collect();
                rows.push(row);
            }
            Ok(Py::new(
                py,
                PyQuerySolutions {
                    variables,
                    rows,
                    pos: 0,
                },
            )?
            .into_any())
        }
        QueryResults::Graph(triples) => {
            let mut out = Vec::new();
            for triple in triples {
                out.push(triple.map_err(|e| PyValueError::new_err(format!("triple error: {e}")))?);
            }
            Ok(Py::new(
                py,
                PyQueryTriples {
                    triples: out,
                    pos: 0,
                },
            )?
            .into_any())
        }
        QueryResults::Boolean(value) => Ok(Py::new(py, PyQueryBoolean { value })?.into_any()),
    }
}

fn read_input(input: Option<&Bound<'_, PyAny>>, path: Option<String>) -> PyResult<Vec<u8>> {
    if let Some(path) = path {
        return std::fs::read(&path)
            .map_err(|e| PyValueError::new_err(format!("cannot read `{path}`: {e}")));
    }
    let Some(input) = input else {
        return Err(PyValueError::new_err(
            "either `input` data or the `path` keyword must be given",
        ));
    };
    if let Ok(bytes) = input.cast::<PyBytes>() {
        return Ok(bytes.as_bytes().to_vec());
    }
    if let Ok(text) = input.cast::<PyString>() {
        return Ok(text.to_str()?.as_bytes().to_vec());
    }
    Err(PyTypeError::new_err("input must be bytes or str"))
}

fn store_err(e: impl std::fmt::Display) -> PyErr {
    PyValueError::new_err(format!("store error: {e}"))
}

/// Register the native oxigraph surface on the `gmeow_rdf` module.
pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PyRdfFormat>()?;
    m.add_class::<PyCanonicalizationAlgorithm>()?;
    m.add_class::<PyNamedNode>()?;
    m.add_class::<PyBlankNode>()?;
    m.add_class::<PyLiteral>()?;
    m.add_class::<PyTriple>()?;
    m.add_class::<PyQuad>()?;
    m.add_class::<PyDefaultGraph>()?;
    m.add_class::<PyVariable>()?;
    m.add_class::<PyQuerySolutions>()?;
    m.add_class::<PyQuerySolution>()?;
    m.add_class::<PyQueryTriples>()?;
    m.add_class::<PyQueryBoolean>()?;
    m.add_class::<PyStore>()?;
    m.add_class::<PyDataset>()?;
    m.add_class::<PyQuadIter>()?;
    m.add_function(wrap_pyfunction!(parse, m)?)?;
    m.add_function(wrap_pyfunction!(serialize, m)?)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const TURTLE_LANG: &str =
        "<https://example.org/s> <https://example.org/p> \"hallo\"@x-gmeow-afrikaans .";

    #[test]
    fn parse_quads_accepts_private_language_tag() {
        let quads = parse_quads(TURTLE_LANG.as_bytes(), RdfFormat::Turtle)
            .expect("private-use language tags must parse leniently");
        assert_eq!(quads.len(), 1);
        match &quads[0].object {
            Term::Literal(lit) => {
                assert_eq!(lit.value(), "hallo");
                assert_eq!(lit.language(), Some("x-gmeow-afrikaans"));
            }
            other => panic!("expected a literal, got {other:?}"),
        }
    }

    #[test]
    fn parse_quads_preserves_literal_lexical_form() {
        // A Store round-trip canonicalizes `+00:00` → `Z` and `0.70` → `0.7`;
        // the parse path must NOT, so the codec preserves the source lexical form.
        let ttl = concat!(
            "<https://example.org/s> <https://example.org/p> ",
            "\"2026-06-19T00:00:00+00:00\"^^<http://www.w3.org/2001/XMLSchema#dateTime> ."
        );
        let quads = parse_quads(ttl.as_bytes(), RdfFormat::Turtle).expect("parse");
        match &quads[0].object {
            Term::Literal(lit) => assert_eq!(lit.value(), "2026-06-19T00:00:00+00:00"),
            other => panic!("expected a literal, got {other:?}"),
        }
    }

    #[test]
    fn canonicalize_quads_is_deterministic_rdfc10() {
        // Two isomorphic graphs with different blank-node labels must canonicalize
        // to byte-identical quad strings under RDFC-1.0.
        let g1 = "_:a <https://example.org/p> _:b .\n_:b <https://example.org/q> _:a .";
        let g2 = "_:x <https://example.org/p> _:y .\n_:y <https://example.org/q> _:x .";
        let alg = CanonicalizationAlgorithm::Rdfc10 {
            hash_algorithm: CanonicalizationHashAlgorithm::Sha256,
        };
        let c1 = canonicalize_quads(
            parse_quads(g1.as_bytes(), RdfFormat::NTriples).unwrap(),
            alg,
        );
        let c2 = canonicalize_quads(
            parse_quads(g2.as_bytes(), RdfFormat::NTriples).unwrap(),
            alg,
        );
        let s1: Vec<String> = c1.iter().map(Quad::to_string).collect();
        let s2: Vec<String> = c2.iter().map(Quad::to_string).collect();
        assert_eq!(s1, s2, "isomorphic graphs must canonicalize identically");
    }

    #[test]
    fn canonicalize_quads_unstable_is_self_consistent() {
        let g = "_:a <https://example.org/p> _:b .";
        let alg = CanonicalizationAlgorithm::Unstable;
        let c1 = canonicalize_quads(parse_quads(g.as_bytes(), RdfFormat::NTriples).unwrap(), alg);
        let c2 = canonicalize_quads(parse_quads(g.as_bytes(), RdfFormat::NTriples).unwrap(), alg);
        let s1: Vec<String> = c1.iter().map(Quad::to_string).collect();
        let s2: Vec<String> = c2.iter().map(Quad::to_string).collect();
        assert_eq!(s1, s2);
    }

    #[test]
    fn parse_quads_reads_rdf12_quoted_triple() {
        // RDF 1.2 reifier: `<< s p o >>` as a quoted-triple object via rdf:reifies.
        let ttl = concat!(
            "<https://example.org/r> ",
            "<http://www.w3.org/1999/02/22-rdf-syntax-ns#reifies> ",
            "<<( <https://example.org/s> <https://example.org/p> <https://example.org/o> )>> ."
        );
        let quads = parse_quads(ttl.as_bytes(), RdfFormat::Turtle).expect("RDF 1.2 must parse");
        assert_eq!(quads.len(), 1);
        assert!(
            matches!(&quads[0].object, Term::Triple(_)),
            "object must be a quoted triple"
        );
    }

    #[test]
    fn serialize_triples_round_trips_ntriples() {
        let triple = Triple::new(
            NamedNode::new("https://example.org/s").unwrap(),
            NamedNode::new("https://example.org/p").unwrap(),
            NamedNode::new("https://example.org/o").unwrap(),
        );
        let bytes = serialize_triples(std::slice::from_ref(&triple), RdfFormat::NTriples).unwrap();
        let reparsed = parse_quads(&bytes, RdfFormat::NTriples).unwrap();
        assert_eq!(reparsed.len(), 1);
        assert_eq!(reparsed[0].subject.to_string(), "<https://example.org/s>");
    }

    #[test]
    fn rdfformat_maps_to_oxigraph() {
        assert_eq!(PyRdfFormat::TURTLE.to_ox(), RdfFormat::Turtle);
        assert_eq!(PyRdfFormat::N_TRIPLES.to_ox(), RdfFormat::NTriples);
        assert_eq!(PyRdfFormat::N_QUADS.to_ox(), RdfFormat::NQuads);
    }
}
