// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! # gmeow-query-wasm — the RDF 1.2 query engine, in the browser
//!
//! Compiles the pinned `purrdf` RDF 1.2 / SPARQL engine to
//! `wasm32-unknown-unknown` and exposes it to JavaScript, so the documentation
//! site's offline SPARQL playground and bundle explorer run the SAME engine the
//! native gate runs — client-side, no server, no repository.
//!
//! ## Why this crate exists
//!
//! The playground engine used to be a **prebuilt blob vendored from the sibling
//! `purrdf` repository**, pinned only by BLAKE3 of its bytes — no version, no
//! revision, and a refresh target that did not exist. It could not be rebuilt
//! here, so it drifted from the `purrdf` the workspace actually pins and nothing
//! detected it. Building the engine in this repository, from the workspace pin,
//! removes that drift class outright: the shipped engine IS the pinned engine.
//!
//! ## Scope
//!
//! - **The real engine.** Parsing, serialization, and SPARQL evaluation are
//!   `purrdf`'s; this crate only marshals strings and bytes across the JS
//!   boundary, exactly as `gmeow-reason-wasm` wraps the reasoner.
//! - **Bundle-wide and graph-aware.** [`Dataset::from_gts`] reads a `gmeow.gts`
//!   bundle's every named graph, so the playground queries the shipped bundle
//!   rather than a flattened core extract. RDF 1.2 quoted triples and the
//!   statement layer survive, because nothing is flattened on the way in.
//! - **No silent empties.** A malformed document, an unevaluable query, or a
//!   `SERVICE` / `LOAD` clause the browser cannot resolve throws — never an empty
//!   result that reads like "no matches".

use purrdf::sparql::{NativeSparqlEngine, ResultProvenance, SparqlResultsFormat};
use purrdf::{
    DatasetView, GraphMatch, RdfDataset, SerializeGraph, SparqlEngine, SparqlRequest, SparqlResult,
};
use std::sync::Arc;
use wasm_bindgen::prelude::*;

/// The engine version (this crate's SemVer), exposed to JS as `version()` — a
/// liveness probe proving the wasm module instantiated and the engine linked.
#[wasm_bindgen]
pub fn version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

/// The blake3 content address of `bytes`, lowercase hex — the SAME hash the emitted
/// `bundle-manifest.json` records for every shipped asset.
///
/// The docs site fetches a 45 MB `gmeow.gts` and must prove it received the bytes the
/// build published. A byte-length comparison cannot: a same-length substitution passes it.
/// The browser has no native blake3, and verifying under a second algorithm would mean the
/// manifest's recorded address is not the one anybody checks — so the engine that is
/// already booted before the fetch exposes the real one.
#[wasm_bindgen(js_name = blake3Hex)]
#[must_use]
pub fn blake3_hex(bytes: &[u8]) -> String {
    blake3::hash(bytes).to_hex().to_string()
}

/// An in-memory RDF 1.2 dataset the browser can parse, serialize, and query.
#[wasm_bindgen]
pub struct Dataset {
    inner: Arc<RdfDataset>,
}

#[wasm_bindgen]
impl Dataset {
    /// `Dataset.parse(text, format)` → parse an RDF document.
    ///
    /// `format` is any media type or short format id `purrdf` understands
    /// (`turtle`/`ttl`, `trig`, `ntriples`/`nt`, `nquads`/`nq`, `rdfxml`,
    /// `jsonld`, …).
    ///
    /// # Errors
    ///
    /// Throws if the format is unrecognized or the document does not parse.
    /// There is no degraded fallback codec.
    #[wasm_bindgen(js_name = parse)]
    pub fn parse(text: &str, format: &str) -> Result<Dataset, JsError> {
        let inner = purrdf::parse_dataset(text.as_bytes(), format, None)
            .map_err(|e| JsError::new(&e.to_string()))?;
        Ok(Self { inner })
    }

    /// `Dataset.fromGts(bytes)` → read every named graph of a `gmeow.gts` bundle.
    ///
    /// This is the bundle-wide entry: the reader preserves the bundle's real graph
    /// structure rather than folding it into a single union, so a query can select
    /// a named graph and the RDF 1.2 statement layer stays addressable.
    ///
    /// # Errors
    ///
    /// Throws if the container cannot be read or its segments do not fold into a
    /// dataset.
    #[wasm_bindgen(js_name = fromGts)]
    pub fn from_gts(gts: &[u8]) -> Result<Dataset, JsError> {
        let graph =
            purrdf::gts::read_all_segments(gts).map_err(|e| JsError::new(&e.to_string()))?;
        let inner = purrdf::gts::dataset_from_gts_graph(&graph)
            .map_err(|e| JsError::new(&e.to_string()))?;
        Ok(Self { inner })
    }

    /// The number of quads in the dataset, across every graph.
    #[wasm_bindgen(getter)]
    pub fn size(&self) -> usize {
        self.inner
            .quads_for_pattern(None, None, None, GraphMatch::Any)
            .count()
    }

    /// `query(sparql, base?)` → run a SPARQL query against this dataset, offline.
    ///
    /// Returns **SPARQL Results JSON** for SELECT / ASK and **Turtle** for
    /// CONSTRUCT / DESCRIBE — the same contract `purrdf`'s own binding presents,
    /// which is what the documentation playground is written against.
    ///
    /// # Errors
    ///
    /// A parse error, an evaluation error, or a `SERVICE` / `LOAD` clause
    /// (unresolvable in-browser) throws — never a silent empty result.
    #[wasm_bindgen(js_name = query)]
    pub fn query(&self, sparql: &str, base: Option<String>) -> Result<String, JsError> {
        let result = NativeSparqlEngine::new()
            .query(
                &self.inner,
                SparqlRequest {
                    query: sparql,
                    base_iri: base.as_deref(),
                    substitutions: &[],
                },
            )
            .map_err(|e| JsError::new(&e.to_string()))?;

        match result {
            SparqlResult::Graph(graph) => {
                let bytes = purrdf::serialize_dataset(
                    &*graph,
                    purrdf::NativeRdfFormat::Turtle.media_type(),
                    SerializeGraph::DefaultGraph,
                )
                .map_err(|e| JsError::new(&e.to_string()))?;
                String::from_utf8(bytes).map_err(|e| {
                    JsError::new(&format!("CONSTRUCT/DESCRIBE Turtle is not UTF-8: {e}"))
                })
            }
            solutions => {
                let outcome = purrdf::sparql::serialize(
                    &solutions,
                    SparqlResultsFormat::Json,
                    &ResultProvenance::default(),
                )
                .map_err(|e| JsError::new(&e.to_string()))?;
                String::from_utf8(outcome.bytes)
                    .map_err(|e| JsError::new(&format!("SPARQL Results JSON is not UTF-8: {e}")))
            }
        }
    }

    /// `serialize(format)` → re-encode the dataset.
    ///
    /// Dataset-capable formats (TriG / N-Quads / TriX / JSON-LD / YAML-LD) carry
    /// every named graph; single-graph syntaxes carry the default graph, which is
    /// `purrdf`'s documented behaviour rather than a silent truncation here.
    ///
    /// # Errors
    ///
    /// Throws if the format is unrecognized or the dataset cannot be encoded.
    #[wasm_bindgen(js_name = serialize)]
    pub fn serialize(&self, format: &str) -> Result<String, JsError> {
        let bytes = purrdf::serialize_dataset(&*self.inner, format, SerializeGraph::Dataset)
            .map_err(|e| JsError::new(&e.to_string()))?;
        String::from_utf8(bytes)
            .map_err(|e| JsError::new(&format!("serialized output is not UTF-8: {e}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TURTLE: &str = concat!(
        "@prefix ex: <https://example.org/> .\n",
        "ex:s ex:p ex:o .\n",
        "ex:s ex:q \"lit\" .\n",
    );

    #[test]
    fn version_is_the_crate_semver() {
        assert_eq!(version(), env!("CARGO_PKG_VERSION"));
    }

    #[test]
    fn parse_then_size_counts_every_quad() {
        let ds = Dataset::parse(TURTLE, "turtle").expect("parse turtle");
        assert_eq!(ds.size(), 2);
    }

    #[test]
    fn select_returns_sparql_results_json() {
        let ds = Dataset::parse(TURTLE, "turtle").expect("parse turtle");
        let out = ds
            .query("SELECT ?p WHERE { <https://example.org/s> ?p ?o }", None)
            .expect("select evaluates");
        assert!(
            out.contains("\"bindings\""),
            "not SPARQL Results JSON: {out}"
        );
    }

    #[test]
    fn ask_returns_sparql_results_json() {
        let ds = Dataset::parse(TURTLE, "turtle").expect("parse turtle");
        let out = ds
            .query("ASK { <https://example.org/s> ?p ?o }", None)
            .expect("ask evaluates");
        assert!(out.contains("true"), "ASK did not report true: {out}");
    }

    #[test]
    fn construct_returns_turtle_not_json() {
        let ds = Dataset::parse(TURTLE, "turtle").expect("parse turtle");
        let out = ds
            .query("CONSTRUCT { ?s ?p ?o } WHERE { ?s ?p ?o }", None)
            .expect("construct evaluates");
        assert!(
            !out.contains("\"bindings\""),
            "CONSTRUCT must be Turtle, not results JSON: {out}"
        );
        assert!(
            out.contains("example.org"),
            "CONSTRUCT Turtle is empty: {out}"
        );
    }

    // The REFUSAL contract (malformed document, unknown format, malformed query,
    // SERVICE clause) is asserted in `js/tests/witness.test.mjs`, not here.
    // `JsError` is a wasm-bindgen imported type: constructing one on a native target
    // panics with "cannot call wasm-bindgen imported functions on non-wasm targets",
    // so a native error-path test would prove nothing about the boundary it guards.
    // The Node lane runs those four cases against the SHIPPED wasm, which is the only
    // place the JS throw is observable at all.

    #[test]
    fn named_graphs_survive_a_trig_round_trip() {
        let trig = concat!(
            "@prefix ex: <https://example.org/> .\n",
            "ex:g { ex:s ex:p ex:o }\n",
        );
        let ds = Dataset::parse(trig, "trig").expect("parse trig");
        let out = ds.serialize("nquads").expect("serialize nquads");
        assert!(
            out.contains("https://example.org/g"),
            "the named graph was flattened away: {out}"
        );
    }
}
