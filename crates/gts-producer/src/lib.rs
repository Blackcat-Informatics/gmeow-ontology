// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! `gmeow-gts-producer` — Rust-native GTS producer for GMEOW.
//!
//! This crate builds GTS files directly from Oxigraph stores using the
//! public `gmeow_gts::writer::Writer` API. It is exposed to Python as the
//! `gmeow_gts_producer` extension module.

pub mod builder;
pub mod interner;

use builder::Builder;
use pyo3::prelude::*;

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
            .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))
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
}

/// Python module entry point.
#[pymodule]
fn gmeow_gts_producer(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PyBuilder>()?;
    Ok(())
}
