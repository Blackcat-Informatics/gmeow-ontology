// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! `gmeow-gts-producer` — Rust-native GTS producer for GMEOW.
//!
//! This crate builds GTS files directly from Oxigraph stores using the
//! public `gmeow_gts::writer::Writer` API. It is exposed to Python as the
//! `gmeow_gts_producer` extension module.

pub mod builder;
pub mod interner;

use builder::{AnnotatedRow, Builder, ProducerError, TermDesc};
use pyo3::prelude::*;
use pyo3::types::{PyAnyMethods, PyDict, PyDictMethods};

/// PyO3 input shape for a single annotated row.
type AnnotatedRowInput = (
    TermDesc,
    TermDesc,
    TermDesc,
    TermDesc,
    Vec<(TermDesc, TermDesc)>,
);

/// PyO3 wrapper around [`Builder`].
#[pyclass(name = "Builder")]
pub struct PyBuilder {
    inner: Builder,
}

#[pymethods]
impl PyBuilder {
    /// Create a new empty builder.
    #[new]
    fn new() -> Self {
        Self {
            inner: Builder::new(),
        }
    }

    /// Parse `path` and append its quads to the builder.
    #[pyo3(signature = (path, graph_name = None, bnode_scope = None))]
    fn add_graph(
        &mut self,
        path: &str,
        graph_name: Option<&str>,
        bnode_scope: Option<&str>,
    ) -> PyResult<()> {
        self.inner
            .add_graph(path, graph_name, bnode_scope)
            .map_err(into_py_value_err)
    }

    /// Parse an RDF 1.2 artifact from `path` and append its statement layer.
    #[pyo3(signature = (path, graph_name = None, bnode_scope = None))]
    fn add_rdf12(
        &mut self,
        path: &str,
        graph_name: Option<&str>,
        bnode_scope: Option<&str>,
    ) -> PyResult<()> {
        self.inner
            .add_rdf12(path, graph_name, bnode_scope)
            .map_err(into_py_value_err)
    }

    /// Add annotated base triples from a list of structured term rows.
    ///
    /// Each row is a 5-tuple `(subject, predicate, object, reifier, annotations)`
    /// where every term is a dict `{"kind": "iri"|"bnode"|"literal", "value": ...,
    /// "datatype": ..., "lang": ...}`. The reifier is bound to the base triple and
    /// its annotations are recorded in the statement layer.
    #[pyo3(signature = (rows, graph_name = None, bnode_scope = None))]
    fn add_annotated_rows(
        &mut self,
        rows: Vec<AnnotatedRowInput>,
        graph_name: Option<&str>,
        bnode_scope: Option<&str>,
    ) -> PyResult<()> {
        let rows: Vec<AnnotatedRow> = rows
            .into_iter()
            .map(
                |(subject, predicate, object, reifier, annotations)| AnnotatedRow {
                    subject,
                    predicate,
                    object,
                    reifier,
                    annotations,
                },
            )
            .collect();
        self.inner
            .add_annotated_rows(&rows, graph_name, bnode_scope)
            .map_err(into_py_value_err)
    }

    /// Number of accumulated terms.
    #[getter]
    fn term_count(&self) -> usize {
        self.inner.terms().len()
    }

    /// Number of accumulated quads.
    #[getter]
    fn quad_count(&self) -> usize {
        self.inner.quads().len()
    }

    /// Number of accumulated reifier bindings.
    #[getter]
    fn reifier_count(&self) -> usize {
        self.inner.reifies().len()
    }

    /// Number of accumulated annotation triples.
    #[getter]
    fn annot_count(&self) -> usize {
        self.inner.annot().len()
    }

    /// Emit an unsigned GTS snapshot frame from the accumulated tables.
    #[pyo3(signature = (profile = "dist"))]
    fn to_gts_unsigned(&self, profile: &str) -> PyResult<Vec<u8>> {
        self.inner.to_gts_bytes(profile).map_err(into_py_value_err)
    }

    /// Emit a complete GTS file with optional blobs, signing, and transform chain.
    #[pyo3(signature = (
        profile = "dist",
        transform = None,
        doc_blobs = None,
        signer_kid = None,
        signer_secret = None,
        public_key_armor = None,
        rsyncable_threshold = 65536
    ))]
    #[allow(clippy::too_many_arguments)]
    fn to_gts(
        &self,
        profile: &str,
        transform: Option<Vec<String>>,
        doc_blobs: Option<Vec<(Vec<u8>, String, String)>>,
        signer_kid: Option<String>,
        signer_secret: Option<Vec<u8>>,
        public_key_armor: Option<String>,
        rsyncable_threshold: usize,
    ) -> PyResult<Vec<u8>> {
        let signer = match (signer_kid, signer_secret) {
            (Some(kid), Some(secret)) => Some((kid, secret)),
            (None, None) => None,
            _ => {
                return Err(pyo3::exceptions::PyValueError::new_err(
                    "signer_kid and signer_secret must be supplied together",
                ))
            }
        };
        self.inner
            .to_gts(
                profile,
                transform,
                doc_blobs,
                signer,
                public_key_armor.as_deref(),
                rsyncable_threshold,
            )
            .map_err(into_py_value_err)
    }
}

fn into_py_value_err(e: ProducerError) -> PyErr {
    pyo3::exceptions::PyValueError::new_err(e.to_string())
}

impl<'a, 'py> FromPyObject<'a, 'py> for TermDesc {
    type Error = PyErr;

    fn extract(obj: pyo3::Borrowed<'a, 'py, PyAny>) -> PyResult<Self> {
        let dict = obj.cast::<PyDict>()?;
        let kind: String = dict
            .get_item("kind")?
            .ok_or_else(|| pyo3::exceptions::PyValueError::new_err("term dict missing 'kind'"))?
            .extract()?;
        match kind.as_str() {
            "iri" => {
                let value: String = dict
                    .get_item("value")?
                    .ok_or_else(|| {
                        pyo3::exceptions::PyValueError::new_err("IRI term dict missing 'value'")
                    })?
                    .extract()?;
                Ok(TermDesc::Iri(value))
            }
            "bnode" => {
                let value: String = dict
                    .get_item("value")?
                    .ok_or_else(|| {
                        pyo3::exceptions::PyValueError::new_err("bnode term dict missing 'value'")
                    })?
                    .extract()?;
                Ok(TermDesc::Bnode(value))
            }
            "literal" => {
                let value: String = dict
                    .get_item("value")?
                    .ok_or_else(|| {
                        pyo3::exceptions::PyValueError::new_err("literal term dict missing 'value'")
                    })?
                    .extract()?;
                let datatype: Option<String> =
                    dict.get_item("datatype")?.and_then(|v| v.extract().ok());
                let lang: Option<String> = dict.get_item("lang")?.and_then(|v| v.extract().ok());
                Ok(TermDesc::Literal {
                    value,
                    datatype,
                    lang,
                })
            }
            _ => Err(pyo3::exceptions::PyValueError::new_err(format!(
                "unknown term kind: {kind}"
            ))),
        }
    }
}

/// Python module entry point.
#[pymodule]
fn gmeow_gts_producer(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PyBuilder>()?;
    Ok(())
}
