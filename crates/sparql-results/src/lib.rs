// SPDX-FileCopyrightText: 2026 Blackcat Informatics Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only
#![forbid(unsafe_code)]

//! Native, wasm-clean serializer for the SPARQL result model.
//!
//! This crate is the canonical authority for turning a `gmeow-rdf-core`
//! [`SparqlResult`] (SELECT solutions, ASK boolean, or CONSTRUCT graph) into the
//! four W3C SPARQL Results formats — JSON (SRJ), XML, CSV, and TSV — plus an
//! additive, provenance-carrying `gmeow` extension. It replaces the
//! oxigraph-family `sparesults` on the results path (purrdf S9, EPIC #906).
//!
//! It depends **only** on `gmeow-rdf-core` (with `default-features = false`) so
//! it stays oxigraph-free and wasm-clean. Term and N-Triples syntax are produced
//! exclusively by the rdf-core kernel `emit_*` primitives (see [`term`],
//! [`graph`]); this crate adds no term-syntax of its own.
//!
//! Scope of the current task: the shared infrastructure (error type, provenance
//! carrier, term lexicalization bridge, CONSTRUCT-graph N-Triples writer) plus
//! the public result-format and outcome types. The format dispatcher and the
//! per-format document writers (JSON/XML/CSV/TSV) land in later tasks.

mod error;
mod graph;
mod model;
mod term;

pub use error::Error;
pub use model::{ResultProvenance, SolutionProvenance};

/// Re-export of the egress result model this crate serializes, so consumers name
/// a single path (`gmeow_sparql_results::SparqlResult`).
pub use gmeow_rdf_core::SparqlResult;

/// The four W3C SPARQL Results serialization formats this crate targets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SparqlResultsFormat {
    /// SPARQL Results JSON (a.k.a. SRJ).
    Json,
    /// SPARQL Results XML.
    Xml,
    /// SPARQL Results CSV.
    Csv,
    /// SPARQL Results TSV.
    Tsv,
}

/// The result of a serialization: the encoded bytes plus an exit-gate flag.
#[derive(Debug, Clone)]
pub struct SerializeOutcome {
    /// The serialized result document.
    pub bytes: Vec<u8>,
    /// True when a non-empty [`ResultProvenance`] was requested but the chosen
    /// format could not carry it. CSV and TSV are pure-W3C value-only formats
    /// with no extension point, so a populated provenance is trimmed at the exit
    /// gate and this flag is set, letting the caller detect the lossy projection.
    pub provenance_dropped: bool,
}
