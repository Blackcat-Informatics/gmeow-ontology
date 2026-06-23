// SPDX-FileCopyrightText: 2026 Blackcat Informatics Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! SPARQL result model for the `gmeow_rdf` Python extension: the materialized
//! `QuerySolutions` / `QuerySolution` (SELECT), `QueryTriples` (CONSTRUCT), and
//! `QueryBoolean` (ASK) pyclasses, plus the `materialize_results` adapter the
//! store seam uses to turn an oxigraph `QueryResults` into these objects.

use oxigraph::model::{Term, Triple, Variable};
use oxigraph::sparql::QueryResults;
use pyo3::exceptions::{PyKeyError, PyTypeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyString};

use super::io::{serialize_triples, PyRdfFormat};
use super::term::{term_to_py, PyTriple, PyVariable};

/// SELECT results, materialized. Mirrors the oxigraph Python `QuerySolutions`.
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

/// A single SELECT solution row. Mirrors the oxigraph Python `QuerySolution`.
#[pyclass(name = "QuerySolution")]
pub struct PyQuerySolution {
    variables: Vec<Variable>,
    row: Vec<Option<Term>>,
}

#[pymethods]
impl PyQuerySolution {
    /// Look a binding up by variable name (`str`), `Variable`, or position
    /// (`int`). An unbound variable yields `None`; an unknown name is a
    /// `KeyError`, matching the oxigraph Python API.
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

/// CONSTRUCT results, materialized. Mirrors the oxigraph Python `QueryTriples`.
#[pyclass(name = "QueryTriples")]
pub struct PyQueryTriples {
    pub(crate) triples: Vec<Triple>,
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

/// An ASK result. Mirrors the oxigraph Python `QueryBoolean`.
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

pub(crate) fn materialize_results(
    py: Python<'_>,
    results: QueryResults<'_>,
) -> PyResult<Py<PyAny>> {
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
