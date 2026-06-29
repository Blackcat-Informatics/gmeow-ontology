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
//! # Single-responsibility layout (#835)
//!
//! This module is the thin facade over five focused submodules, split along the
//! P2 backend-trait seams so the trait extraction (#836) is a clean lift:
//!
//! * [`term`] — the term object model (`NamedNode` … `Quad`, `Variable`) and the
//!   Python ⇄ oxigraph term converters/extractors (`TermFactory` seam).
//! * [`io`] — `parse` / `serialize` + the pure-Rust `parse_quads` /
//!   `serialize_triples` cores (`RdfParserBackend` / `RdfSerializer` seams).
//! * [`query`] — the materialized SPARQL result model (`SparqlEngine` seam).
//! * [`store`] — the mutable `Store` / `Dataset` / `QuadIter` (`MutableStore` /
//!   `Dataset` seams).
//! * [`canon`] — `CanonicalizationAlgorithm` + the `canonicalize_quads` core.
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

mod canon;
mod io;
mod mutable;
mod query;
mod store;
mod term;
mod xsd;

pub use canon::{canonicalize_quads, PyCanonicalizationAlgorithm};
pub use io::{parse_quads, PyRdfFormat};
pub use mutable::PyMutableDataset;
pub use query::{PyQueryBoolean, PyQuerySolution, PyQuerySolutions, PyQueryTriples};
pub use store::{PyDataset, PyQuadIter, PyStore};
#[cfg(feature = "oxigraph")]
pub use term::dataset_quads_to_py;
pub use term::{
    quad_to_py, PyBlankNode, PyDefaultGraph, PyLiteral, PyNamedNode, PyQuad, PyTriple, PyVariable,
};

use pyo3::prelude::*;

/// Register the native oxigraph surface on the `gmeow_rdf` module.
pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<io::PyRdfFormat>()?;
    m.add_class::<canon::PyCanonicalizationAlgorithm>()?;
    m.add_class::<term::PyNamedNode>()?;
    m.add_class::<term::PyBlankNode>()?;
    m.add_class::<term::PyLiteral>()?;
    m.add_class::<term::PyTriple>()?;
    m.add_class::<term::PyQuad>()?;
    m.add_class::<term::PyDefaultGraph>()?;
    m.add_class::<term::PyVariable>()?;
    m.add_class::<query::PyQuerySolutions>()?;
    m.add_class::<query::PyQuerySolution>()?;
    m.add_class::<query::PyQueryTriples>()?;
    m.add_class::<query::PyQueryBoolean>()?;
    m.add_class::<store::PyStore>()?;
    m.add_class::<store::PyDataset>()?;
    m.add_class::<mutable::PyMutableDataset>()?;
    m.add_class::<store::PyQuadIter>()?;
    m.add_function(wrap_pyfunction!(io::parse, m)?)?;
    m.add_function(wrap_pyfunction!(io::serialize, m)?)?;
    m.add_function(wrap_pyfunction!(xsd::xsd_value_compare, m)?)?;
    m.add_function(wrap_pyfunction!(xsd::xsd_canonical_lexical, m)?)?;
    Ok(())
}
