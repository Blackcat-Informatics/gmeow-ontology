// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! `purrdf_query` (typed result + row cursor) and `purrdf_query_json` (the
//! SPARQL 1.1/1.2 Query Results JSON convenience path).

use std::os::raw::c_char;

use gmeow_rdf::oxigraph::{store_from_dataset, GraphPolicy};
use gmeow_rdf::{
    format_from_media_type, serialize_dataset_to_format, OxigraphBackend, SparqlEngine,
    SparqlRequest, SparqlResult,
};
use gmeow_rdf_core::{RdfDiagnostic, TermValue};

use crate::buffer::PurrdfBuffer;
use crate::error::PurrdfError;
use crate::handles::PurrdfDataset;
use crate::rowcursor::PurrdfRowCursor;
use crate::status::PurrdfStatus;
use crate::{cstr_to_str, opt_cstr_to_str};

/// The discriminant written to `purrdf_query`'s `out_kind`.
const KIND_SOLUTIONS: i32 = 0;
const KIND_GRAPH: i32 = 1;
const KIND_BOOLEAN: i32 = 2;

/// Run a SPARQL query over a frozen dataset, materializing the result.
unsafe fn run_query(
    dataset: *const PurrdfDataset,
    query: *const c_char,
    base_iri: *const c_char,
) -> Result<SparqlResult, PurrdfError> {
    let query = cstr_to_str(query)?;
    let base_iri = opt_cstr_to_str(base_iri)?;
    let store = store_from_dataset(
        PurrdfDataset::dataset(dataset),
        GraphPolicy::PreserveNamedGraphs,
    )
    .map_err(|diagnostic| PurrdfError::from_diagnostic(PurrdfStatus::QueryError, &diagnostic))?;
    OxigraphBackend
        .query(&store, SparqlRequest { query, base_iri })
        .map_err(|diagnostic| PurrdfError::from_diagnostic(PurrdfStatus::QueryError, &diagnostic))
}

/// Execute a SPARQL query. The result shape is reported in `*out_kind`:
/// `0` = SELECT → `*out_rows` is a `PurrdfRowCursor` (free with
/// `purrdf_rowcursor_free`); `1` = CONSTRUCT/DESCRIBE → `*out_graph` is a
/// `PurrdfDataset` (free with `purrdf_dataset_free`); `2` = ASK → `*out_boolean`
/// is `0`/`1`. Exactly one output is set per kind. `base_iri` may be null.
///
/// # Safety
/// `dataset` must be a live handle; `query` must be a NUL-terminated C string;
/// the out-params must be writable.
#[no_mangle]
pub unsafe extern "C" fn purrdf_query(
    dataset: *const PurrdfDataset,
    query: *const c_char,
    base_iri: *const c_char,
    out_kind: *mut i32,
    out_rows: *mut *mut PurrdfRowCursor,
    out_graph: *mut *mut PurrdfDataset,
    out_boolean: *mut u8,
    out_error: *mut *mut PurrdfError,
) -> i32 {
    ffi_try!(out_error, {
        if dataset.is_null() || query.is_null() || out_kind.is_null() {
            return Err(PurrdfError::new(
                PurrdfStatus::NullPointer,
                "null pointer argument to purrdf_query",
            ));
        }
        match run_query(dataset, query, base_iri)? {
            SparqlResult::Solutions { variables, rows } => {
                if out_rows.is_null() {
                    return Err(PurrdfError::new(
                        PurrdfStatus::NullPointer,
                        "out_rows is null for a SELECT result",
                    ));
                }
                *out_kind = KIND_SOLUTIONS;
                *out_rows = PurrdfRowCursor::new(variables, rows).into_raw();
            }
            SparqlResult::Graph(graph) => {
                if out_graph.is_null() {
                    return Err(PurrdfError::new(
                        PurrdfStatus::NullPointer,
                        "out_graph is null for a CONSTRUCT/DESCRIBE result",
                    ));
                }
                *out_kind = KIND_GRAPH;
                *out_graph = PurrdfDataset::into_raw(graph);
            }
            SparqlResult::Boolean(value) => {
                *out_kind = KIND_BOOLEAN;
                if !out_boolean.is_null() {
                    *out_boolean = value as u8;
                }
            }
        }
        Ok(PurrdfStatus::Ok)
    })
}

/// Execute a SPARQL query and serialize the result to the SPARQL 1.1 Query
/// Results JSON format (SELECT and ASK) into `*out_buffer` (UTF-8). A
/// CONSTRUCT/DESCRIBE graph is rendered as N-Triples inside a documented
/// `{"graph": "..."}` envelope. The simple/robust path — no row cursor needed.
///
/// # Safety
/// `dataset` must be a live handle; `query` must be a NUL-terminated C string;
/// the out-params must be writable.
#[no_mangle]
pub unsafe extern "C" fn purrdf_query_json(
    dataset: *const PurrdfDataset,
    query: *const c_char,
    base_iri: *const c_char,
    out_buffer: *mut *mut PurrdfBuffer,
    out_error: *mut *mut PurrdfError,
) -> i32 {
    ffi_try!(out_error, {
        if dataset.is_null() || query.is_null() || out_buffer.is_null() {
            return Err(PurrdfError::new(
                PurrdfStatus::NullPointer,
                "null pointer argument to purrdf_query_json",
            ));
        }
        let result = run_query(dataset, query, base_iri)?;
        let json = result_to_json(&result).map_err(|diagnostic| {
            PurrdfError::from_diagnostic(PurrdfStatus::QueryError, &diagnostic)
        })?;
        *out_buffer = PurrdfBuffer::into_raw(json.into_bytes());
        Ok(PurrdfStatus::Ok)
    })
}

/// Serialize a materialized SPARQL result to SPARQL-JSON (or the graph envelope).
fn result_to_json(result: &SparqlResult) -> Result<String, RdfDiagnostic> {
    let mut out = String::new();
    match result {
        SparqlResult::Boolean(value) => {
            out.push_str("{\"head\":{},\"boolean\":");
            out.push_str(if *value { "true" } else { "false" });
            out.push('}');
        }
        SparqlResult::Solutions { variables, rows } => {
            out.push_str("{\"head\":{\"vars\":[");
            for (i, var) in variables.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                json_string(var, &mut out);
            }
            out.push_str("]},\"results\":{\"bindings\":[");
            for (row_index, row) in rows.iter().enumerate() {
                if row_index > 0 {
                    out.push(',');
                }
                out.push('{');
                let mut first = true;
                for (column, cell) in row.iter().enumerate() {
                    if let Some(value) = cell {
                        if !first {
                            out.push(',');
                        }
                        first = false;
                        // `variables[column]` always exists (rows are dense over vars).
                        json_string(&variables[column], &mut out);
                        out.push(':');
                        json_binding(value, &mut out);
                    }
                }
                out.push('}');
            }
            out.push_str("]}}");
        }
        SparqlResult::Graph(graph) => {
            // No native SPARQL-JSON shape for a graph result; render N-Triples in a
            // documented envelope so the caller still gets the full triples.
            let format = format_from_media_type("application/n-triples")?;
            let outcome = serialize_dataset_to_format(graph.as_ref(), format, None)?;
            let nt = String::from_utf8_lossy(&outcome.bytes);
            out.push_str("{\"graph\":");
            json_string(&nt, &mut out);
            out.push('}');
        }
    }
    Ok(out)
}

/// Append a JSON-escaped string literal (including the surrounding quotes).
fn json_string(value: &str, out: &mut String) {
    out.push('"');
    for ch in value.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                out.push_str(&format!("\\u{:04x}", c as u32));
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

/// Append a SPARQL-JSON binding object for a term value (recursive for triples).
fn json_binding(value: &TermValue, out: &mut String) {
    match value {
        TermValue::Iri(iri) => {
            out.push_str("{\"type\":\"uri\",\"value\":");
            json_string(iri, out);
            out.push('}');
        }
        TermValue::Blank { label, .. } => {
            out.push_str("{\"type\":\"bnode\",\"value\":");
            json_string(label, out);
            out.push('}');
        }
        TermValue::Literal {
            lexical_form,
            datatype,
            language,
            ..
        } => {
            out.push_str("{\"type\":\"literal\",\"value\":");
            json_string(lexical_form, out);
            if let Some(language) = language {
                out.push_str(",\"xml:lang\":");
                json_string(language, out);
            } else {
                out.push_str(",\"datatype\":");
                json_string(datatype, out);
            }
            out.push('}');
        }
        TermValue::Triple { s, p, o } => {
            out.push_str("{\"type\":\"triple\",\"value\":{\"subject\":");
            json_binding(s, out);
            out.push_str(",\"predicate\":");
            json_binding(p, out);
            out.push_str(",\"object\":");
            json_binding(o, out);
            out.push_str("}}");
        }
    }
}
