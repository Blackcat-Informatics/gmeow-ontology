// SPDX-FileCopyrightText: 2026 Blackcat Informatics Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Python-facing copy-on-write mutable dataset for the RDFLib compat shim.
//!
//! The canonical mutation semantics live in `gmeow-rdf-core::MutableDataset`.
//! This adapter keeps Python on that COW surface and materializes to oxigraph only
//! for the existing SPARQL and serializer engines.

use std::sync::Arc;

use oxigraph::io::{RdfFormat, RdfSerializer};
use oxigraph::model::{
    BlankNode, GraphName, Literal, NamedNode, NamedOrBlankNode, Quad, Term, Triple,
};
use oxigraph::sparql::SparqlEvaluator;
use oxigraph::store::Store;
use pyo3::exceptions::{PyTypeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict};

use super::io::{parse_quads, read_input, PyRdfFormat};
use super::query::materialize_results;
use super::store::PyQuadIter;
use super::term::{extract_graph_name, extract_term, PyQuad, PyVariable};
use crate::dataset_view::{DatasetMut, GraphMatchValue};
use crate::ir::{MutableDataset, QuadValues};
use crate::{BlankScope, RdfDataset, RdfDatasetBuilder, RdfLiteral, RdfTextDirection, TermValue};

const XSD_STRING: &str = "http://www.w3.org/2001/XMLSchema#string";
const RDF_LANG_STRING: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#langString";

/// A COW mutable RDF dataset over the native `gmeow-rdf-core` IR.
#[pyclass(name = "MutableDataset")]
pub struct PyMutableDataset {
    inner: MutableDataset,
    next_blank_scope: u32,
}

#[pymethods]
impl PyMutableDataset {
    #[new]
    fn new() -> PyResult<Self> {
        Ok(Self {
            inner: empty_mutable()?,
            next_blank_scope: 1,
        })
    }

    /// Load RDF into the mutable dataset.
    #[pyo3(signature = (input=None, format=None, *, path=None))]
    fn load(
        &mut self,
        input: Option<&Bound<'_, PyAny>>,
        format: Option<PyRdfFormat>,
        path: Option<String>,
    ) -> PyResult<()> {
        let format = format.ok_or_else(|| PyValueError::new_err("load: format is required"))?;
        let data = read_input(input, path)?;
        let blank_scope = self.allocate_blank_scope();
        for quad in parse_quads(&data, format.to_ox())
            .map_err(|e| PyValueError::new_err(format!("load parse error: {e}")))?
        {
            self.inner
                .insert(quad_to_values_scoped(&quad, blank_scope)?);
        }
        Ok(())
    }

    /// Add a single quad. Returns whether the effective set changed.
    fn add(&mut self, quad: &PyQuad) -> PyResult<bool> {
        Ok(self.inner.insert(quad_to_values(&quad.inner)?))
    }

    /// Remove a single quad. Returns whether the effective set changed.
    fn remove(&mut self, quad: &PyQuad) -> PyResult<bool> {
        Ok(self.inner.remove(&quad_to_values(&quad.inner)?))
    }

    /// Return whether the exact quad is effective.
    fn contains(&self, quad: &PyQuad) -> PyResult<bool> {
        Ok(self.inner.contains(&quad_to_values(&quad.inner)?))
    }

    /// Effective quads matching a value pattern.
    #[pyo3(signature = (subject=None, predicate=None, object=None, graph_name=None, *, any_graph=false))]
    fn quads_for_pattern(
        &self,
        py: Python<'_>,
        subject: Option<&Bound<'_, PyAny>>,
        predicate: Option<&Bound<'_, PyAny>>,
        object: Option<&Bound<'_, PyAny>>,
        graph_name: Option<&Bound<'_, PyAny>>,
        any_graph: bool,
    ) -> PyResult<Vec<Py<PyQuad>>> {
        let s = optional_term(subject)?;
        let p = optional_term(predicate)?;
        let o = optional_term(object)?;
        let g_value = optional_graph_value(graph_name)?;
        let graph_match = if any_graph {
            GraphMatchValue::Any
        } else {
            match g_value.as_ref() {
                Some(g) => GraphMatchValue::Named(g),
                None => GraphMatchValue::Default,
            }
        };
        self.inner
            .quads_for_pattern(s.as_ref(), p.as_ref(), o.as_ref(), graph_match)
            .into_iter()
            .map(|q| {
                Py::new(
                    py,
                    PyQuad {
                        inner: values_to_quad(&q)?,
                    },
                )
            })
            .collect()
    }

    /// Dump the effective dataset (or one graph) in `format`.
    #[pyo3(signature = (output=None, format=None, *, from_graph=None))]
    fn dump(
        &self,
        py: Python<'_>,
        output: Option<&Bound<'_, PyAny>>,
        format: Option<PyRdfFormat>,
        from_graph: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<Option<Py<PyBytes>>> {
        let format = format.ok_or_else(|| PyValueError::new_err("dump: format is required"))?;
        let store = self.materialize_store()?;
        let mut buf = Vec::new();
        if format.to_ox().supports_datasets() && from_graph.is_none() {
            store
                .dump_to_writer(RdfSerializer::from_format(format.to_ox()), &mut buf)
                .map_err(|e| PyValueError::new_err(format!("dump error: {e}")))?;
        } else {
            let graph = extract_graph_name(from_graph)?;
            store
                .dump_graph_to_writer(
                    graph.as_ref(),
                    RdfSerializer::from_format(format.to_ox()),
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

    /// Run a SPARQL query over the effective dataset.
    #[pyo3(signature = (query, *, substitutions=None))]
    fn query(
        &self,
        py: Python<'_>,
        query: &str,
        substitutions: Option<&Bound<'_, PyDict>>,
    ) -> PyResult<Py<PyAny>> {
        let store = self.materialize_store()?;
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
            .on_store(&store)
            .execute()
            .map_err(|e| PyValueError::new_err(format!("query evaluation error: {e}")))?;
        materialize_results(py, results)
    }

    /// Run a SPARQL UPDATE by materializing, updating, and rebuilding the COW set.
    fn update(&mut self, update: &str) -> PyResult<()> {
        let store = self.materialize_store()?;
        store
            .update(update)
            .map_err(|e| PyValueError::new_err(format!("update evaluation error: {e}")))?;
        let mut data = Vec::new();
        store
            .dump_to_writer(RdfSerializer::from_format(RdfFormat::NQuads), &mut data)
            .map_err(|e| PyValueError::new_err(format!("update dump error: {e}")))?;
        self.inner = mutable_from_quads(
            parse_quads(&data, RdfFormat::NQuads)
                .map_err(|e| PyValueError::new_err(format!("update rebuild parse error: {e}")))?,
        )?;
        Ok(())
    }

    /// Compact the effective set into a fresh frozen base.
    fn compact(&mut self) -> PyResult<()> {
        let frozen = self
            .inner
            .freeze()
            .map_err(|e| PyValueError::new_err(e.to_string()))?;
        self.inner = MutableDataset::new(frozen);
        Ok(())
    }

    fn __len__(&self) -> usize {
        self.inner
            .quads_for_pattern(None, None, None, GraphMatchValue::Any)
            .len()
    }

    fn __iter__(slf: PyRef<'_, Self>, py: Python<'_>) -> PyResult<Py<PyQuadIter>> {
        let quads = slf
            .inner
            .quads_for_pattern(None, None, None, GraphMatchValue::Any)
            .into_iter()
            .map(|q| values_to_quad(&q))
            .collect::<PyResult<Vec<_>>>()?;
        Py::new(py, PyQuadIter { quads, pos: 0 })
    }
}

impl PyMutableDataset {
    fn allocate_blank_scope(&mut self) -> BlankScope {
        let scope = BlankScope(self.next_blank_scope);
        self.next_blank_scope = self.next_blank_scope.checked_add(1).unwrap_or(1);
        scope
    }

    fn materialize_store(&self) -> PyResult<Store> {
        let store = Store::new().map_err(store_err)?;
        for quad in self
            .inner
            .quads_for_pattern(None, None, None, GraphMatchValue::Any)
        {
            store.insert(&values_to_quad(&quad)?).map_err(store_err)?;
        }
        Ok(store)
    }
}

fn empty_mutable() -> PyResult<MutableDataset> {
    let base = RdfDatasetBuilder::new()
        .freeze()
        .map_err(|e| PyValueError::new_err(e.to_string()))?;
    Ok(MutableDataset::new(base))
}

fn mutable_from_quads(quads: Vec<Quad>) -> PyResult<MutableDataset> {
    let mut mutable = empty_mutable()?;
    for quad in quads {
        mutable.insert(quad_to_values(&quad)?);
    }
    Ok(mutable)
}

fn optional_term(obj: Option<&Bound<'_, PyAny>>) -> PyResult<Option<TermValue>> {
    let Some(obj) = obj else {
        return Ok(None);
    };
    if obj.is_none() {
        return Ok(None);
    }
    term_to_value(&extract_term(obj)?).map(Some)
}

fn optional_graph_value(obj: Option<&Bound<'_, PyAny>>) -> PyResult<Option<TermValue>> {
    let Some(obj) = obj else {
        return Ok(None);
    };
    if obj.is_none() {
        return Ok(None);
    }
    match extract_graph_name(Some(obj))? {
        GraphName::DefaultGraph => Ok(None),
        GraphName::NamedNode(n) => Ok(Some(TermValue::Iri(n.as_str().to_owned()))),
        GraphName::BlankNode(b) => Ok(Some(blank_value_scoped(b.as_str(), BlankScope::DEFAULT))),
    }
}

fn quad_to_values(quad: &Quad) -> PyResult<QuadValues> {
    quad_to_values_scoped(quad, BlankScope::DEFAULT)
}

fn quad_to_values_scoped(quad: &Quad, scope: BlankScope) -> PyResult<QuadValues> {
    Ok(QuadValues {
        s: subject_to_value(&quad.subject, scope),
        p: TermValue::Iri(quad.predicate.as_str().to_owned()),
        o: term_to_value_scoped(&quad.object, scope)?,
        g: match &quad.graph_name {
            GraphName::DefaultGraph => None,
            GraphName::NamedNode(n) => Some(TermValue::Iri(n.as_str().to_owned())),
            GraphName::BlankNode(b) => Some(blank_value_scoped(b.as_str(), scope)),
        },
    })
}

fn subject_to_value(subject: &NamedOrBlankNode, scope: BlankScope) -> TermValue {
    match subject {
        NamedOrBlankNode::NamedNode(n) => TermValue::Iri(n.as_str().to_owned()),
        NamedOrBlankNode::BlankNode(b) => blank_value_scoped(b.as_str(), scope),
    }
}

fn term_to_value(term: &Term) -> PyResult<TermValue> {
    term_to_value_scoped(term, BlankScope::DEFAULT)
}

fn term_to_value_scoped(term: &Term, scope: BlankScope) -> PyResult<TermValue> {
    Ok(match term {
        Term::NamedNode(n) => TermValue::Iri(n.as_str().to_owned()),
        Term::BlankNode(b) => blank_value_scoped(b.as_str(), scope),
        Term::Literal(l) => TermValue::Literal {
            lexical_form: l.value().to_owned(),
            datatype: if l.language().is_some() {
                RDF_LANG_STRING.to_owned()
            } else {
                l.datatype().as_str().to_owned()
            },
            language: l.language().map(str::to_owned),
            direction: None,
        },
        Term::Triple(t) => TermValue::Triple {
            s: Box::new(subject_to_value(&t.subject, scope)),
            p: Box::new(TermValue::Iri(t.predicate.as_str().to_owned())),
            o: Box::new(term_to_value_scoped(&t.object, scope)?),
        },
    })
}

fn blank_value(label: &str, scope: BlankScope) -> TermValue {
    TermValue::Blank {
        label: label.to_owned(),
        scope,
    }
}

fn blank_value_scoped(label: &str, scope: BlankScope) -> TermValue {
    if scope == BlankScope::DEFAULT {
        blank_value_from_external_label(label)
    } else {
        blank_value(label, scope)
    }
}

fn blank_value_from_external_label(label: &str) -> TermValue {
    if let Some((raw_label, raw_scope)) = label.rsplit_once(".s") {
        if !raw_label.is_empty() {
            if let Ok(scope) = raw_scope.parse::<u32>() {
                if scope > 0 {
                    return blank_value(raw_label, BlankScope(scope));
                }
            }
        }
    }
    blank_value(label, BlankScope::DEFAULT)
}

fn values_to_quad(values: &QuadValues) -> PyResult<Quad> {
    Ok(Quad::new(
        value_to_subject(&values.s)?,
        value_to_named_node(&values.p)?,
        value_to_term(&values.o)?,
        match &values.g {
            None => GraphName::DefaultGraph,
            Some(g) => value_to_graph_name(g)?,
        },
    ))
}

fn value_to_subject(value: &TermValue) -> PyResult<NamedOrBlankNode> {
    match value {
        TermValue::Iri(iri) => NamedNode::new(iri.as_str())
            .map(NamedOrBlankNode::NamedNode)
            .map_err(|e| PyValueError::new_err(format!("invalid subject IRI `{iri}`: {e}"))),
        TermValue::Blank { label, scope } => {
            let qualified = scope.qualify_label(label);
            BlankNode::new(qualified.as_ref())
                .map(NamedOrBlankNode::BlankNode)
                .map_err(|e| {
                    PyValueError::new_err(format!("invalid blank node `{qualified}`: {e}"))
                })
        }
        _ => Err(PyTypeError::new_err(
            "a subject must be an IRI or blank node",
        )),
    }
}

fn value_to_named_node(value: &TermValue) -> PyResult<NamedNode> {
    match value {
        TermValue::Iri(iri) => NamedNode::new(iri.as_str())
            .map_err(|e| PyValueError::new_err(format!("invalid predicate IRI `{iri}`: {e}"))),
        _ => Err(PyTypeError::new_err("a predicate must be an IRI")),
    }
}

fn value_to_graph_name(value: &TermValue) -> PyResult<GraphName> {
    match value {
        TermValue::Iri(iri) => NamedNode::new(iri.as_str())
            .map(GraphName::NamedNode)
            .map_err(|e| PyValueError::new_err(format!("invalid graph IRI `{iri}`: {e}"))),
        TermValue::Blank { label, scope } => {
            let qualified = scope.qualify_label(label);
            BlankNode::new(qualified.as_ref())
                .map(GraphName::BlankNode)
                .map_err(|e| {
                    PyValueError::new_err(format!("invalid graph blank node `{qualified}`: {e}"))
                })
        }
        _ => Err(PyTypeError::new_err(
            "a graph name must be an IRI or blank node",
        )),
    }
}

fn value_to_term(value: &TermValue) -> PyResult<Term> {
    Ok(match value {
        TermValue::Iri(iri) => Term::NamedNode(
            NamedNode::new(iri.as_str())
                .map_err(|e| PyValueError::new_err(format!("invalid IRI `{iri}`: {e}")))?,
        ),
        TermValue::Blank { label, scope } => {
            let qualified = scope.qualify_label(label);
            Term::BlankNode(BlankNode::new(qualified.as_ref()).map_err(|e| {
                PyValueError::new_err(format!("invalid blank node `{qualified}`: {e}"))
            })?)
        }
        TermValue::Literal {
            lexical_form,
            datatype,
            language,
            direction: _,
        } => {
            let literal = if let Some(language) = language {
                Literal::new_language_tagged_literal_unchecked(lexical_form.clone(), language)
            } else if datatype == XSD_STRING {
                Literal::new_simple_literal(lexical_form.clone())
            } else {
                Literal::new_typed_literal(
                    lexical_form.clone(),
                    NamedNode::new(datatype.as_str()).map_err(|e| {
                        PyValueError::new_err(format!("invalid datatype `{datatype}`: {e}"))
                    })?,
                )
            };
            Term::Literal(literal)
        }
        TermValue::Triple { s, p, o } => Term::Triple(Box::new(Triple::new(
            value_to_subject(s)?,
            value_to_named_node(p)?,
            value_to_term(o)?,
        ))),
    })
}

#[allow(dead_code)]
fn freeze_values(quads: &[QuadValues]) -> PyResult<Arc<RdfDataset>> {
    let mut builder = RdfDatasetBuilder::new();
    for quad in quads {
        let s = intern_subject_value(&mut builder, &quad.s)?;
        let p = intern_iri_value(&mut builder, &quad.p)?;
        let o = intern_term_value(&mut builder, &quad.o)?;
        let g = quad
            .g
            .as_ref()
            .map(|g| intern_graph_value(&mut builder, g))
            .transpose()?;
        builder.push_quad(s, p, o, g);
    }
    builder
        .freeze()
        .map_err(|e| PyValueError::new_err(e.to_string()))
}

fn intern_subject_value(
    builder: &mut RdfDatasetBuilder,
    value: &TermValue,
) -> PyResult<crate::TermId> {
    match value {
        TermValue::Iri(iri) => Ok(builder.intern_iri(iri.clone())),
        TermValue::Blank { label, scope } => Ok(builder.intern_blank(label.clone(), *scope)),
        _ => Err(PyTypeError::new_err(
            "a subject must be an IRI or blank node",
        )),
    }
}

fn intern_iri_value(builder: &mut RdfDatasetBuilder, value: &TermValue) -> PyResult<crate::TermId> {
    match value {
        TermValue::Iri(iri) => Ok(builder.intern_iri(iri.clone())),
        _ => Err(PyTypeError::new_err("a predicate must be an IRI")),
    }
}

fn intern_graph_value(
    builder: &mut RdfDatasetBuilder,
    value: &TermValue,
) -> PyResult<crate::TermId> {
    intern_subject_value(builder, value)
}

fn intern_term_value(
    builder: &mut RdfDatasetBuilder,
    value: &TermValue,
) -> PyResult<crate::TermId> {
    Ok(match value {
        TermValue::Iri(iri) => builder.intern_iri(iri.clone()),
        TermValue::Blank { label, scope } => builder.intern_blank(label.clone(), *scope),
        TermValue::Literal {
            lexical_form,
            datatype,
            language,
            direction,
        } => builder.intern_literal(RdfLiteral {
            lexical_form: lexical_form.clone(),
            datatype: Some(datatype.clone()),
            language: language.clone(),
            direction: direction.map(|d| match d {
                RdfTextDirection::Ltr => RdfTextDirection::Ltr,
                RdfTextDirection::Rtl => RdfTextDirection::Rtl,
            }),
        }),
        TermValue::Triple { s, p, o } => {
            let s = intern_subject_value(builder, s)?;
            let p = intern_iri_value(builder, p)?;
            let o = intern_term_value(builder, o)?;
            builder.intern_triple(s, p, o)
        }
    })
}

fn store_err(e: impl std::fmt::Display) -> PyErr {
    PyValueError::new_err(format!("store error: {e}"))
}
