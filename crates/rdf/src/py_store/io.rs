// SPDX-FileCopyrightText: 2026 Blackcat Informatics Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Parse / serialize surface for the `gmeow_rdf` Python extension: the
//! `RdfFormat` pyclass, the `parse` / `serialize` module functions, and the
//! pure-Rust `parse_quads` / `serialize_triples` cores plus the `read_input`
//! helper the store seam shares.

use oxigraph::io::{RdfFormat, RdfParser, RdfSerializer};
use oxigraph::model::{Quad, Triple};
use pyo3::exceptions::{PyTypeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyString};

use super::query::PyQueryTriples;
use super::term::PyQuad;

// ── RDF serialization format enum ───────────────────────────────────────────────

/// The RDF serialization formats the codebase loads/parses/serializes.
///
/// Mirrors the oxigraph Python `RdfFormat`; the members keep the SCREAMING_SNAKE Python
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
    pub(crate) fn to_ox(self) -> RdfFormat {
        match self {
            PyRdfFormat::TURTLE => RdfFormat::Turtle,
            PyRdfFormat::N_TRIPLES => RdfFormat::NTriples,
            PyRdfFormat::N_QUADS => RdfFormat::NQuads,
            PyRdfFormat::TRIG => RdfFormat::TriG,
        }
    }
}

// ── Module-level functions ──────────────────────────────────────────────────────

/// Parse RDF bytes/str into a list of `Quad`. Mirrors the oxigraph Python `parse`.
///
/// Unlike `Store.load`, blank-node labels are preserved verbatim (no renaming),
/// so canonicalization over the parsed quads is meaningful.
#[pyfunction]
#[pyo3(signature = (input, format))]
pub(crate) fn parse(
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

/// Serialize `QueryTriples` in `format`. Mirrors the oxigraph Python `serialize`: when
/// `output` (a file-like with `.write`) is given the bytes are written to it and
/// `None` is returned; when `output` is omitted the serialized `bytes` are
/// returned directly.
#[pyfunction]
#[pyo3(signature = (input, output=None, format=None))]
pub(crate) fn serialize(
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

pub(crate) fn serialize_triples(triples: &[Triple], format: RdfFormat) -> Result<Vec<u8>, String> {
    let mut serializer = RdfSerializer::from_format(format).for_writer(Vec::new());
    for triple in triples {
        serializer
            .serialize_triple(triple.as_ref())
            .map_err(|e| e.to_string())?;
    }
    serializer.finish().map_err(|e| e.to_string())
}

pub(crate) fn read_input(
    input: Option<&Bound<'_, PyAny>>,
    path: Option<String>,
) -> PyResult<Vec<u8>> {
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

#[cfg(test)]
mod tests {
    use oxigraph::model::{NamedNode, Term};

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
