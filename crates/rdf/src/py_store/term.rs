// SPDX-FileCopyrightText: 2026 Blackcat Informatics Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The RDF term object model for the `gmeow_rdf` Python extension: the
//! `NamedNode` / `BlankNode` / `Literal` / `Triple` / `Quad` / `DefaultGraph` /
//! `Variable` pyclasses, plus the Python ⇄ oxigraph term converters and
//! extractors the store, query, and io seams share.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use oxigraph::model::{
    BlankNode, GraphName, Literal, NamedNode, NamedOrBlankNode, Quad, Term, Triple, Variable,
};
use pyo3::exceptions::{PyTypeError, PyValueError};
use pyo3::prelude::*;

// ── Term model ──────────────────────────────────────────────────────────────────

fn hash_str(value: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    value.hash(&mut hasher);
    hasher.finish()
}

/// An IRI node. Mirrors the oxigraph Python `NamedNode`.
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

/// A blank node. Mirrors the oxigraph Python `BlankNode`.
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

/// An RDF literal. Mirrors the oxigraph Python `Literal`.
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
    /// `rdf:langString` for a language-tagged one), matching the oxigraph Python API.
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

/// A quoted triple term (RDF 1.2 / RDF-star). Mirrors the oxigraph Python `Triple`.
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

/// An RDF quad. Mirrors the oxigraph Python `Quad`.
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

/// A default-graph marker term. Mirrors the oxigraph Python `DefaultGraph`.
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
/// the oxigraph Python `Variable`.
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

/// Build a Python `Quad` object from an oxigraph [`Quad`].
///
/// Cross-crate constructor for the engine crates that produce quads natively (the
/// RL closure in `gmeow-logic`, issue #630): they assemble an oxigraph `Quad` and
/// hand Python a live `gmeow_rdf.Quad` directly, so the closure result never makes
/// a round-trip through an intermediate N-Triples string the Python side has to
/// re-parse. The returned object is the same `PyQuad` the parser/SPARQL surface
/// yields, so downstream code (rdflib adapters, comparators) treats it uniformly.
pub fn quad_to_py(py: Python<'_>, quad: &Quad) -> PyResult<Py<PyAny>> {
    Ok(Py::new(
        py,
        PyQuad {
            inner: quad.clone(),
        },
    )?
    .into_any())
}

// ── Term ⇄ Python conversions ────────────────────────────────────────────────────

pub(crate) fn term_to_py(py: Python<'_>, term: &Term) -> PyResult<Py<PyAny>> {
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

pub(crate) fn extract_term(obj: &Bound<'_, PyAny>) -> PyResult<Term> {
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

/// Coerce a Python term to an RDF 1.2 subject. RDF 1.2 (unlike the obsolete
/// RDF-star) allows triple terms in the OBJECT position only — a subject is an
/// IRI or blank node, never a quoted triple — which is exactly oxigraph's
/// `NamedOrBlankNode`. A `Triple` therefore reaches `extract_term`, not here.
fn extract_subject(obj: &Bound<'_, PyAny>) -> PyResult<NamedOrBlankNode> {
    if let Ok(n) = obj.cast::<PyNamedNode>() {
        return Ok(NamedOrBlankNode::NamedNode(n.get().inner.clone()));
    }
    if let Ok(b) = obj.cast::<PyBlankNode>() {
        return Ok(NamedOrBlankNode::BlankNode(b.get().inner.clone()));
    }
    Err(PyTypeError::new_err(
        "a subject must be a NamedNode or BlankNode \
         (RDF 1.2 triple terms are object-position only)",
    ))
}

fn extract_named_node(obj: &Bound<'_, PyAny>) -> PyResult<NamedNode> {
    obj.cast::<PyNamedNode>()
        .map(|n| n.get().inner.clone())
        .map_err(|_| PyTypeError::new_err("a predicate must be a NamedNode"))
}

pub(crate) fn extract_graph_name(obj: Option<&Bound<'_, PyAny>>) -> PyResult<GraphName> {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rdf12_triple_terms_are_object_position_only() {
        // RDF 1.2 (unlike obsolete RDF-star) permits quoted triples in the OBJECT
        // slot only; a subject is always a NamedOrBlankNode. This pins the model
        // that the Python `extract_subject` and the `_Subject` stub rely on — no
        // subject-position quoted triples.
        let inner = Triple::new(
            NamedNode::new("https://example.org/s").unwrap(),
            NamedNode::new("https://example.org/p").unwrap(),
            NamedNode::new("https://example.org/o").unwrap(),
        );
        let quad = Quad::new(
            NamedNode::new("https://example.org/r").unwrap(),
            NamedNode::new("http://www.w3.org/1999/02/22-rdf-syntax-ns#reifies").unwrap(),
            Term::Triple(Box::new(inner)),
            GraphName::DefaultGraph,
        );
        assert!(
            matches!(quad.object, Term::Triple(_)),
            "a quoted triple is a valid object"
        );
        // The subject type itself forbids a quoted triple — the compiler enforces
        // it, and this asserts the constructed value is a plain node.
        assert!(matches!(quad.subject, NamedOrBlankNode::NamedNode(_)));
    }
}
