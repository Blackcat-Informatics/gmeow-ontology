// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Native RDF text ingress/egress for the slice emitters and linters (#909 S5).
//!
//! Every slice module that used to parse Turtle/N-Triples with `oxigraph::io`
//! (`RdfParser::from_format(...).for_reader(...)`) now routes through the
//! oxigraph-free native codecs in `gmeow_rdf`: [`parse_dataset`] folds the RDF 1.2
//! statement layer and `store_from_dataset` materialises the result into the
//! oxigraph store the SPARQL/extraction code still queries. The store stays for
//! pattern matching and RDFC-1.0 canonicalisation (scope-OUT of #909); only the
//! TEXT parse/serialize calls are replaced.
//!
//! The native parse is always lenient on GMEOW's long private-use `@x-gmeow-*`
//! language tags (the gmeow-gts codecs are now fully lenient), preserving the old
//! `.lenient()` behaviour without a flag.

use std::path::Path;

use gmeow_rdf::oxigraph::{
    flat_oxigraph_quads_from_dataset_scoped, store_from_dataset, GraphPolicy,
};
use gmeow_rdf::{parse_dataset, serialize_dataset, NativeRdfFormat, SerializeGraph};
use oxigraph::store::Store;

use crate::error::SliceError;

/// Map a file extension to the native RDF media type, defaulting to Turtle.
///
/// Mirrors the historical `rdf_format_for_path` extension routing (`.nt` →
/// N-Triples, `.nq` → N-Quads, `.trig` → TriG, everything else Turtle).
pub(crate) fn media_type_for_path(path: &Path) -> &'static str {
    match path.extension().and_then(|e| e.to_str()) {
        Some("nt") => NativeRdfFormat::NTriples.media_type(),
        Some("nq") => NativeRdfFormat::NQuads.media_type(),
        Some("trig") => NativeRdfFormat::TriG.media_type(),
        _ => NativeRdfFormat::Turtle.media_type(),
    }
}

/// Parse RDF text `bytes` of `media_type` into a fresh oxigraph store via the
/// native codecs, preserving named graphs. `context` labels parse errors.
pub(crate) fn rdf_bytes_to_store(
    bytes: &[u8],
    media_type: &str,
    context: &str,
) -> Result<Store, SliceError> {
    let dataset = parse_dataset(bytes, media_type, None)
        .map_err(|e| SliceError::Parse(format!("syntax error in {context}: {e}")))?;
    store_from_dataset(&dataset, GraphPolicy::PreserveNamedGraphs)
        .map_err(|e| SliceError::Parse(format!("store build for {context}: {e}")))
}

/// Parse RDF text `bytes` of `media_type` and insert the resulting quads into an
/// existing `store`, accumulating across calls. `context` labels parse errors.
pub(crate) fn rdf_bytes_into_store(
    store: &Store,
    bytes: &[u8],
    media_type: &str,
    context: &str,
) -> Result<(), SliceError> {
    let dataset = parse_dataset(bytes, media_type, None)
        .map_err(|e| SliceError::Parse(format!("syntax error in {context}: {e}")))?;
    // SCOPE blanks by `context` (the source path): distinct documents accumulated into
    // one store keep disjoint anonymous blanks (`_:gts_<counter>` restarts per parse),
    // while every stage re-parsing the same source derives the same labels.
    for quad in flat_oxigraph_quads_from_dataset_scoped(&dataset, context)
        .map_err(|e| SliceError::Parse(format!("IR → quads for {context}: {e}")))?
    {
        store
            .insert(&quad)
            .map_err(|e| SliceError::Parse(format!("store insert failed: {e}")))?;
    }
    Ok(())
}

/// Parse Turtle `bytes` into a fresh oxigraph store via the native codecs.
pub(crate) fn turtle_bytes_to_store(bytes: &[u8], context: &str) -> Result<Store, SliceError> {
    rdf_bytes_to_store(bytes, NativeRdfFormat::Turtle.media_type(), context)
}

/// Parse Turtle `bytes` and insert the resulting quads into an existing `store`.
pub(crate) fn turtle_bytes_into_store(
    store: &Store,
    bytes: &[u8],
    context: &str,
) -> Result<(), SliceError> {
    rdf_bytes_into_store(store, bytes, NativeRdfFormat::Turtle.media_type(), context)
}

/// Serialize an oxigraph store to N-Triples text via the native codecs, going
/// store → IR → text (no `oxigraph::io`). Used by the EDOAL re-dump that must
/// reproduce the historical `store.dump(NT)` blank-node normalisation.
pub(crate) fn store_to_ntriples(store: &Store, context: &str) -> Result<String, SliceError> {
    let dataset = gmeow_rdf::oxigraph::dataset_from_store(store)
        .map_err(|e| SliceError::Parse(format!("store → IR for {context}: {e}")))?;
    let bytes = serialize_dataset(
        &dataset,
        NativeRdfFormat::NTriples.media_type(),
        SerializeGraph::DefaultGraph,
    )
    .map_err(|e| SliceError::Parse(format!("serialize {context}: {e}")))?;
    String::from_utf8(bytes).map_err(|e| SliceError::Parse(format!("non-utf8 {context}: {e}")))
}
