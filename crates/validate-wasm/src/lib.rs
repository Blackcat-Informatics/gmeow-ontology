// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! # gmeow-validate-wasm — the repo-free Tier-1 validator, in the browser
//!
//! This crate compiles the wasm-clean [`gmeow-validate`](gmeow_validate) Tier-1 core to
//! `wasm32-unknown-unknown` and exposes it to JavaScript/TypeScript, so editor
//! plugins, browsers, and LLM clients can check authored GMEOW RDF against a
//! `gmeow.gts` bundle **client-side** — before submitting it — with no server, no
//! repository, and no Docker.
//!
//! ## Scope (by charter)
//!
//! - **Tier-1 only.** SHACL against the bundle's data-graph shape union plus the
//!   OntoUML disciplines — the checks that carry no reasoner. The Tier-2 `--deep`
//!   semantic pass reasons via the native DL engine, which does not compile to wasm;
//!   it is excluded here by contract, not degraded, so this surface exposes exactly
//!   the deep-less [`gmeow_validate::data_validate::validate_json`] core.
//! - **JSON boundary.** [`validate`] takes the RDF text, its format, the bundle
//!   bytes, the GMEOW namespace, and the data file's display path, and returns the
//!   canonical diagnostics `Report` serialized to JSON — the same shape the native
//!   CLI and the SARIF bridge project from.
//!
//! ## Architecture
//!
//! The `#[wasm_bindgen]` surface is a thin shim: `gmeow-validate` already returns a
//! JSON string with no Python or filesystem coupling, so this crate only marshals the
//! JS strings/bytes across the boundary and maps the validator's `String` error onto a
//! JS exception. The validation logic lives in `gmeow-validate` so it unit-tests on
//! the native workspace gate; the wasm-bindgen wrapper is exercised as real wasm by
//! the Node round-trip lane.

use wasm_bindgen::prelude::*;

/// The validator version (the crate's SemVer), exposed to JS as `version()`.
///
/// A liveness probe for the wasm build + the npm package: importing the module and
/// calling `version()` proves it instantiated and the validator core linked.
#[wasm_bindgen]
pub fn version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

/// Run Tier-1 conformance of `data` (RDF text in `format`) against the SHACL shapes
/// and OntoUML disciplines carried in the `gts` bundle bytes, returning the
/// diagnostics `Report` as a JSON string.
///
/// - `data` — the RDF document to validate (UTF-8 text).
/// - `format` — a media type or short id understood by the validator
///   (`turtle`/`ttl`, `trig`, `n-triples`/`nt`, `n-quads`/`nq`, `rdf+xml`, or the
///   JSON-LD ids `json-ld`/`jsonld`).
/// - `gts` — the `gmeow.gts` bundle bytes (carrying the `shapes-archive`).
/// - `namespace` — the GMEOW IRI prefix the discipline checks key on.
/// - `origin` — the data file's display path, recorded as each finding's location.
///
/// The returned JSON is the canonical `Report`: `{ "tool": "validate", "findings":
/// [ { "severity": "error"|"warning"|"note", "code": ..., ... } ] }`, with `findings`
/// omitted when the graph conforms.
///
/// # Errors
///
/// Throws a JS exception if the bundle carries no `shapes-archive`, the archive or
/// shapes are malformed, or the data graph fails to parse.
#[wasm_bindgen]
pub fn validate(
    data: &str,
    format: &str,
    gts: &[u8],
    namespace: &str,
    origin: &str,
) -> Result<String, JsError> {
    gmeow_validate::data_validate::validate_json(data.as_bytes(), format, gts, namespace, origin)
        .map_err(|e| JsError::new(e.message()))
}

/// Extract a `gmeow.gts` bundle's RDF as **graph-preserving N-Quads text**, so an
/// in-browser RDF engine (the vendored purrdf wasm) can parse and query the SAME
/// bundle the pipeline shipped — the browser source of truth for the documentation
/// playground and bundle explorer, replacing any second curated data path.
///
/// - `gts` — the `gmeow.gts` bundle bytes (the single canonical browser-query
///   bundle; the container is read, not re-embedded).
///
/// Returns N-Quads (`application/n-quads`) covering every named graph in the bundle
/// (the graph component of each quad is retained — the query surface sees the
/// bundle's real graph structure, not a flattened union).
///
/// # Errors
///
/// Throws a JS exception if the container cannot be read, the statement layer cannot
/// be folded, or the dataset cannot be serialized.
#[wasm_bindgen]
pub fn bundle_dataset(gts: &[u8]) -> Result<String, JsError> {
    gmeow_validate::store::dataset_nquads_from_gts(gts).map_err(|e| JsError::new(e.message()))
}
