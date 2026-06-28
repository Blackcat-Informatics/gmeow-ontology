// SPDX-FileCopyrightText: 2026 Blackcat Informatics Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! PyO3 boundary for the Rust-owned GTS fold view.

use std::path::PathBuf;

use gmeow_gts::model::{Graph, Term, TermKind};
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::{PyBool, PyBytes, PyDict, PyList};

use crate::gts_view::{GtsFoldView, PublicValue, RelationalRows, ALL_SCOPE};

type PyTermRow = (
    u8,
    Option<String>,
    Option<usize>,
    Option<String>,
    Option<String>,
    Option<usize>,
);

#[pyclass(name = "GtsFoldViewNative")]
pub struct PyGtsFoldView {
    inner: GtsFoldView,
}

#[pymethods]
impl PyGtsFoldView {
    #[staticmethod]
    fn from_bytes(data: &[u8]) -> Self {
        let graph = gmeow_gts::reader::read(data, true, None);
        Self {
            inner: GtsFoldView::new(graph),
        }
    }

    #[staticmethod]
    fn from_parts(
        terms: Vec<PyTermRow>,
        quads: Vec<(usize, usize, usize, Option<usize>)>,
        reifiers: Vec<(usize, (usize, usize, usize))>,
        annotations: Vec<(usize, usize, usize)>,
    ) -> PyResult<Self> {
        let graph = graph_from_parts(terms, quads, reifiers, annotations)?;
        Ok(Self {
            inner: GtsFoldView::new(graph),
        })
    }

    fn term_count(&self) -> usize {
        self.inner.graph().terms.len()
    }

    fn quad_count(&self) -> usize {
        self.inner.graph().quads.len()
    }

    fn reifier_count(&self) -> usize {
        self.inner.reifiers().len()
    }

    fn annotation_count(&self) -> usize {
        self.inner.annotations().len()
    }

    fn term_tuple(&self, tid: usize) -> PyResult<PyTermRow> {
        let term = self
            .inner
            .graph()
            .terms
            .get(tid)
            .ok_or_else(|| PyValueError::new_err(format!("term id out of range: {tid}")))?;
        Ok((
            term_kind_int(term.kind),
            term.value.clone(),
            term.datatype,
            term.lang.clone(),
            term.direction.clone(),
            term.reifier,
        ))
    }

    fn is_iri(&self, tid: usize) -> bool {
        self.inner.is_iri(tid)
    }

    fn is_bnode(&self, tid: usize) -> bool {
        self.inner.is_bnode(tid)
    }

    fn is_literal(&self, tid: usize) -> bool {
        self.inner.is_literal(tid)
    }

    fn iri(&self, tid: usize) -> Option<String> {
        self.inner.iri(tid).map(str::to_string)
    }

    fn lex(&self, tid: usize) -> String {
        self.inner.lex(tid).to_string()
    }

    fn lang(&self, tid: usize) -> Option<String> {
        self.inner.lang(tid).map(str::to_string)
    }

    fn datatype(&self, tid: usize) -> String {
        self.inner.datatype(tid)
    }

    fn nq_token(&self, tid: usize) -> String {
        self.inner.nq_token(tid)
    }

    fn python_value<'py>(&self, py: Python<'py>, tid: usize) -> PyResult<Py<PyAny>> {
        match self.inner.public_value(tid) {
            PublicValue::Iri(value) | PublicValue::Blank(value) | PublicValue::String(value) => {
                Ok(value.into_pyobject(py)?.unbind().into())
            }
            PublicValue::Integer(value) => Ok(value.into_pyobject(py)?.unbind().into()),
            PublicValue::Float(value) => Ok(value.into_pyobject(py)?.unbind().into()),
            PublicValue::Boolean(value) => Ok(PyBool::new(py, value).to_owned().unbind().into()),
            PublicValue::LanguageString { value, lang } => {
                let d = PyDict::new(py);
                d.set_item("value", value)?;
                d.set_item("lang", lang)?;
                Ok(d.unbind().into())
            }
        }
    }

    fn tid_of_iri(&self, iri: &str) -> Option<usize> {
        self.inner.tid_of_iri(iri)
    }

    fn curie(&self, iri: &str) -> String {
        self.inner.curie(iri)
    }

    fn quads(&self, scope: Option<String>) -> Vec<(usize, usize, usize, Option<usize>)> {
        self.inner.quads(scope.as_deref())
    }

    fn subjects_by_type(&self, class_iri: &str, scope: Option<String>) -> Vec<usize> {
        self.inner.subjects_by_type(class_iri, scope.as_deref())
    }

    fn objects(&self, s_tid: usize, p_iri: &str, scope: Option<String>) -> Vec<usize> {
        self.inner.objects(s_tid, p_iri, scope.as_deref())
    }

    fn value(&self, s_tid: usize, p_iri: &str, scope: Option<String>) -> Option<usize> {
        self.inner.value(s_tid, p_iri, scope.as_deref())
    }

    fn predicate_objects(&self, s_tid: usize, scope: Option<String>) -> Vec<(usize, usize)> {
        self.inner.predicate_objects(s_tid, scope.as_deref())
    }

    fn has(&self, s_tid: usize, p_iri: &str, o_tid: usize, scope: Option<String>) -> bool {
        self.inner.has(s_tid, p_iri, o_tid, scope.as_deref())
    }

    fn rdf_list(&self, head_tid: usize, scope: Option<String>) -> Vec<usize> {
        self.inner.rdf_list(head_tid, scope.as_deref())
    }

    fn reifiers(&self) -> Vec<(usize, (usize, usize, usize))> {
        self.inner.reifiers().to_vec()
    }

    fn annotations(&self) -> Vec<(usize, usize, usize)> {
        self.inner.annotations().to_vec()
    }

    fn tag_map(&self) -> BTreeMapString {
        BTreeMapString(self.inner.tag_map().clone())
    }

    fn available_languages(&self) -> Vec<String> {
        self.inner.available_languages().into_iter().collect()
    }

    fn public_text(&self, s_tid: usize, p_iri: &str, scope: Option<String>) -> String {
        self.inner.public_text(s_tid, p_iri, scope.as_deref())
    }

    fn public_literal(
        &self,
        s_tid: usize,
        p_iri: &str,
        scope: Option<String>,
    ) -> (String, Option<String>) {
        self.inner.public_literal(s_tid, p_iri, scope.as_deref())
    }

    fn public_literal_with_fallback(
        &self,
        s_tid: usize,
        p_iri: &str,
        requested: Vec<String>,
        scope: Option<String>,
    ) -> (String, Option<String>, bool) {
        self.inner
            .public_literal_with_fallback(s_tid, p_iri, &requested, scope.as_deref())
    }

    fn public_text_with_fallback(
        &self,
        s_tid: usize,
        p_iri: &str,
        requested: Vec<String>,
        scope: Option<String>,
    ) -> (String, bool) {
        let (text, _lang, fallback) =
            self.inner
                .public_literal_with_fallback(s_tid, p_iri, &requested, scope.as_deref());
        (text, fallback)
    }

    fn public_texts(
        &self,
        s_tid: usize,
        p_iri: &str,
        requested: Vec<String>,
        scope: Option<String>,
    ) -> Vec<(String, Option<String>, bool)> {
        self.inner
            .public_texts(s_tid, p_iri, &requested, scope.as_deref())
    }

    fn relational_rows<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyDict>> {
        relational_rows_dict(py, self.inner.relational_rows())
    }
}

#[pyfunction]
fn gts_relational_rows_from_bytes<'py>(
    py: Python<'py>,
    data: &[u8],
) -> PyResult<Bound<'py, PyDict>> {
    let graph = gmeow_gts::reader::read(data, true, None);
    relational_rows_dict(py, crate::gts_view::relational_rows(&graph))
}

#[pyfunction]
fn gts_to_sqlite(data: &[u8], path: &str) -> PyResult<String> {
    let graph = gmeow_gts::reader::read(data, true, None);
    let out = gmeow_gts::db::to_sqlite(&graph, PathBuf::from(path))
        .map_err(|err| PyValueError::new_err(err.to_string()))?;
    Ok(out.to_string_lossy().into_owned())
}

#[pyfunction]
fn gts_to_duckdb(data: &[u8], path: &str) -> PyResult<String> {
    let graph = gmeow_gts::reader::read(data, true, None);
    let out = gmeow_gts::db::to_duckdb(&graph, PathBuf::from(path))
        .map_err(|err| PyValueError::new_err(err.to_string()))?;
    Ok(out.to_string_lossy().into_owned())
}

#[pyfunction]
fn gts_to_parquet(data: &[u8], out_dir: &str) -> PyResult<Vec<String>> {
    let graph = gmeow_gts::reader::read(data, true, None);
    let paths = gmeow_gts::db::to_parquet(&graph, PathBuf::from(out_dir))
        .map_err(|err| PyValueError::new_err(err.to_string()))?;
    Ok(paths
        .into_iter()
        .map(|path| path.to_string_lossy().into_owned())
        .collect())
}

pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PyGtsFoldView>()?;
    m.add("GTS_ALL_SCOPE", ALL_SCOPE)?;
    m.add_function(wrap_pyfunction!(gts_relational_rows_from_bytes, m)?)?;
    m.add_function(wrap_pyfunction!(gts_to_sqlite, m)?)?;
    m.add_function(wrap_pyfunction!(gts_to_duckdb, m)?)?;
    m.add_function(wrap_pyfunction!(gts_to_parquet, m)?)?;
    Ok(())
}

fn graph_from_parts(
    terms: Vec<PyTermRow>,
    quads: Vec<(usize, usize, usize, Option<usize>)>,
    reifiers: Vec<(usize, (usize, usize, usize))>,
    annotations: Vec<(usize, usize, usize)>,
) -> PyResult<Graph> {
    Ok(Graph {
        terms: terms
            .into_iter()
            .map(|(kind, value, datatype, lang, direction, reifier)| {
                Ok(Term {
                    kind: term_kind(kind)?,
                    value,
                    datatype,
                    lang,
                    direction,
                    reifier,
                })
            })
            .collect::<PyResult<Vec<_>>>()?,
        quads,
        reifiers,
        annotations,
        ..Graph::default()
    })
}

fn term_kind(kind: u8) -> PyResult<TermKind> {
    match kind {
        0 => Ok(TermKind::Iri),
        1 => Ok(TermKind::Literal),
        2 => Ok(TermKind::Bnode),
        3 => Ok(TermKind::Triple),
        _ => Err(PyValueError::new_err(format!(
            "unknown GTS term kind: {kind}"
        ))),
    }
}

fn term_kind_int(kind: TermKind) -> u8 {
    match kind {
        TermKind::Iri => 0,
        TermKind::Literal => 1,
        TermKind::Bnode => 2,
        TermKind::Triple => 3,
    }
}

fn relational_rows_dict<'py>(
    py: Python<'py>,
    rows: RelationalRows,
) -> PyResult<Bound<'py, PyDict>> {
    let out = PyDict::new(py);
    out.set_item("terms", rows.terms)?;
    out.set_item("quads", rows.quads)?;
    out.set_item("reifiers", rows.reifiers)?;
    out.set_item("annotations", rows.annotations)?;
    let blobs = PyList::empty(py);
    for (digest, bytes) in rows.blobs {
        blobs.append((digest, PyBytes::new(py, &bytes)))?;
    }
    out.set_item("blobs", blobs)?;
    Ok(out)
}

struct BTreeMapString(std::collections::BTreeMap<String, String>);

impl<'py> IntoPyObject<'py> for BTreeMapString {
    type Target = PyDict;
    type Output = Bound<'py, PyDict>;
    type Error = PyErr;

    fn into_pyobject(self, py: Python<'py>) -> Result<Self::Output, Self::Error> {
        let out = PyDict::new(py);
        for (key, value) in self.0 {
            out.set_item(key, value)?;
        }
        Ok(out)
    }
}
