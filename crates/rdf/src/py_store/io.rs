// SPDX-FileCopyrightText: 2026 Blackcat Informatics Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Parse / serialize surface for the `gmeow_rdf` Python extension: the
//! `RdfFormat` pyclass, the `parse` / `serialize` module functions, and the
//! pure-Rust `parse_quads` / `serialize_triples` cores plus the `read_input`
//! helper the store seam shares.

use oxigraph::model::{BaseDirection, GraphName, NamedOrBlankNode, Quad, Term as OxTerm, Triple};
use pyo3::exceptions::{PyTypeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyString};

use super::query::PyQueryTriples;
use super::term::PyQuad;
use crate::oxigraph::flat_oxigraph_quads_from_dataset;
use crate::{
    parse_dataset, serialize_dataset, BlankScope, NativeRdfFormat, RdfDatasetBuilder, RdfLiteral,
    RdfTextDirection, SerializeGraph, TermId,
};

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
    /// The native codec format selector (#909): the always-on replacement for the
    /// oxigraph `RdfFormat` router on the parse/serialize path.
    pub(crate) fn to_native(self) -> NativeRdfFormat {
        match self {
            PyRdfFormat::TURTLE => NativeRdfFormat::Turtle,
            PyRdfFormat::N_TRIPLES => NativeRdfFormat::NTriples,
            PyRdfFormat::N_QUADS => NativeRdfFormat::NQuads,
            PyRdfFormat::TRIG => NativeRdfFormat::TriG,
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
    let quads = parse_quads(&data, format.to_native())
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
    let bytes = serialize_triples(&input.triples, format.to_native())
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

/// Parse RDF bytes into owned quads via the native codec (#909) with no blank-node
/// renaming. Routes through [`parse_dataset`](crate::parse_dataset) → IR → the flat
/// quad un-fold ([`flat_oxigraph_quads_from_dataset`]) so the `rdf:reifies` /
/// annotation rows of the RDF 1.2 statement layer reappear in the quad stream exactly
/// as a flat parse would yield them. Private-use language tags such as
/// `@x-gmeow-*` are valid BCP-47 `x-…` privateuse tags and survive the native parse.
pub fn parse_quads(data: &[u8], format: NativeRdfFormat) -> Result<Vec<Quad>, String> {
    let dataset = parse_dataset(data, format.media_type(), None).map_err(|e| e.to_string())?;
    flat_oxigraph_quads_from_dataset(&dataset).map_err(|e| e.to_string())
}

pub(crate) fn serialize_triples(
    triples: &[Triple],
    format: NativeRdfFormat,
) -> Result<Vec<u8>, String> {
    // Build the IR verbatim — every triple is a default-graph quad, RDF-star
    // triple-term objects preserved as triple-term objects (no statement-layer fold)
    // — then serialize the default graph through the native codec.
    let mut builder = RdfDatasetBuilder::new();
    for triple in triples {
        let s = intern_ox_subject(&mut builder, &triple.subject);
        let p = builder.intern_iri(triple.predicate.as_str().to_owned());
        let o = intern_ox_term(&mut builder, &triple.object);
        builder.push_quad(s, p, o, None);
    }
    let dataset = builder.freeze().map_err(|e| e.to_string())?;
    serialize_dataset(&dataset, format.media_type(), SerializeGraph::DefaultGraph)
        .map_err(|e| e.to_string())
}

/// Freeze a flat oxigraph quad list into the IR verbatim — RDF-star triple-term
/// objects preserved as triple-term objects (no statement-layer fold), named graphs
/// kept — for native serialization (#909). Shared by the `Store`/`MutableDataset`
/// dump paths.
pub(super) fn dataset_from_ox_quads_verbatim(
    quads: &[Quad],
) -> Result<std::sync::Arc<crate::RdfDataset>, String> {
    let mut builder = RdfDatasetBuilder::new();
    for quad in quads {
        let s = intern_ox_subject(&mut builder, &quad.subject);
        let p = builder.intern_iri(quad.predicate.as_str().to_owned());
        let o = intern_ox_term(&mut builder, &quad.object);
        let g = match &quad.graph_name {
            GraphName::DefaultGraph => None,
            GraphName::NamedNode(n) => Some(builder.intern_iri(n.as_str().to_owned())),
            GraphName::BlankNode(b) => {
                Some(builder.intern_blank(b.as_str().to_owned(), BlankScope::DEFAULT))
            }
        };
        builder.push_quad(s, p, o, g);
    }
    builder.freeze().map_err(|e| e.to_string())
}

/// Intern an oxigraph subject (IRI / blank) into the IR builder, verbatim.
fn intern_ox_subject(builder: &mut RdfDatasetBuilder, subject: &NamedOrBlankNode) -> TermId {
    match subject {
        NamedOrBlankNode::NamedNode(n) => builder.intern_iri(n.as_str().to_owned()),
        NamedOrBlankNode::BlankNode(b) => {
            builder.intern_blank(b.as_str().to_owned(), BlankScope::DEFAULT)
        }
    }
}

/// Intern an oxigraph term (IRI / blank / literal / triple term) into the IR builder
/// verbatim — RDF-star triple-term objects stay triple-term objects (no fold).
fn intern_ox_term(builder: &mut RdfDatasetBuilder, term: &OxTerm) -> TermId {
    match term {
        OxTerm::NamedNode(n) => builder.intern_iri(n.as_str().to_owned()),
        OxTerm::BlankNode(b) => builder.intern_blank(b.as_str().to_owned(), BlankScope::DEFAULT),
        OxTerm::Literal(l) => builder.intern_literal(RdfLiteral {
            lexical_form: l.value().to_owned(),
            datatype: Some(l.datatype().as_str().to_owned()),
            language: l.language().map(str::to_owned),
            direction: l.direction().map(|d| match d {
                BaseDirection::Ltr => RdfTextDirection::Ltr,
                BaseDirection::Rtl => RdfTextDirection::Rtl,
            }),
        }),
        OxTerm::Triple(t) => {
            let s = intern_ox_subject(builder, &t.subject);
            let p = builder.intern_iri(t.predicate.as_str().to_owned());
            let o = intern_ox_term(builder, &t.object);
            builder.intern_triple(s, p, o)
        }
    }
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

    const NQUADS_LANG: &str =
        "<https://example.org/s> <https://example.org/p> \"hallo\"@x-gmeow-afrikaans .";

    #[test]
    fn parse_quads_accepts_private_language_tag_in_nquads() {
        // The project's private-use language tags (`@x-gmeow-*`) must survive the
        // parse. The native N-Quads codec (gmeow-gts's own lenient tokenizer) accepts
        // them, including the >8-char subtag `afrikaans` that strict BCP-47 rejects.
        let quads = parse_quads(NQUADS_LANG.as_bytes(), NativeRdfFormat::NQuads)
            .expect("private-use language tags must parse via N-Quads");
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
    fn parse_quads_accepts_private_language_tag_in_turtle_and_ntriples() {
        // #909: the project's `@x-gmeow-*` private-use tags exceed BCP-47's 8-char
        // subtag limit (`afrikaans` is 9 chars). gmeow-gts 0.9.6 (PR
        // Blackcat-Informatics/gmeow-gts#358) runs its Turtle / N-Triples codecs in
        // `lenient` mode — matching the prior oxigraph `RdfParser::lenient()` path — so
        // these tags now parse in EVERY format, not just N-Quads. (Was a strict-reject
        // gap in 0.9.5; pinned here so the leniency is explicit and not silently lost.)
        let ttl = concat!(
            "<https://example.org/s> <https://example.org/p> ",
            "\"hallo\"@x-gmeow-afrikaans ."
        );
        for format in [NativeRdfFormat::Turtle, NativeRdfFormat::NTriples] {
            let quads = parse_quads(ttl.as_bytes(), format)
                .unwrap_or_else(|e| panic!("{format:?} must accept the private-use tag: {e}"));
            assert_eq!(quads.len(), 1);
            match &quads[0].object {
                Term::Literal(lit) => {
                    assert_eq!(lit.value(), "hallo");
                    assert_eq!(lit.language(), Some("x-gmeow-afrikaans"));
                }
                other => panic!("expected a literal, got {other:?}"),
            }
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
        let quads = parse_quads(ttl.as_bytes(), NativeRdfFormat::Turtle).expect("parse");
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
        let quads =
            parse_quads(ttl.as_bytes(), NativeRdfFormat::Turtle).expect("RDF 1.2 must parse");
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
        let bytes =
            serialize_triples(std::slice::from_ref(&triple), NativeRdfFormat::NTriples).unwrap();
        let reparsed = parse_quads(&bytes, NativeRdfFormat::NTriples).unwrap();
        assert_eq!(reparsed.len(), 1);
        assert_eq!(reparsed[0].subject.to_string(), "<https://example.org/s>");
    }

    #[test]
    fn rdfformat_maps_to_native() {
        assert_eq!(PyRdfFormat::TURTLE.to_native(), NativeRdfFormat::Turtle);
        assert_eq!(
            PyRdfFormat::N_TRIPLES.to_native(),
            NativeRdfFormat::NTriples
        );
        assert_eq!(PyRdfFormat::N_QUADS.to_native(), NativeRdfFormat::NQuads);
        assert_eq!(PyRdfFormat::TRIG.to_native(), NativeRdfFormat::TriG);
    }
}
