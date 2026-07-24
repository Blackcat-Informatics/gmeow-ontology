// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! # gmeow-gmn-wasm — the GMEOW Model Notation codec, in the browser
//!
//! Compiles the shipped GMN-0↔GMN-1 codec (`gmeow-lang-bridge`) to
//! `wasm32-unknown-unknown` and exposes it to JavaScript/TypeScript, so the docs GMN
//! transcode widget turns authored RDF into the token-compact GMN-1 surface — and
//! back — client-side, using the SAME codec + glyph symbology the on-gate authority
//! ships. GMN-2 (lossy compaction) and the zstd-dictionary transport are NOT here —
//! that notation is still being built in epic #1371.
//!
//! Thin shim: all codec logic lives in `gmeow-lang-bridge` (native-tested with the
//! byte-exact round-trip witness); this only marshals across the JS boundary.

use gmeow_lang_bridge::{Gmn0Model, Gmn1Document, GmnDictionary, gmn1_read, gmn1_write};
use wasm_bindgen::prelude::*;

/// The codec version (the crate's SemVer), exposed to JS as `version()`.
#[wasm_bindgen]
pub fn version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

/// Transcode `data` (RDF text in `format`) — its GMN-0 normal form — into the
/// token-compact **GMN-1** surface text.
///
/// # Errors
///
/// Throws if the RDF cannot be parsed or the GMN-1 write fails.
#[wasm_bindgen]
pub fn to_gmn1(data: &str, format: &str) -> Result<String, JsError> {
    let ds = purrdf::parse_dataset(data.as_bytes(), format, None)
        .map_err(|e| JsError::new(&e.to_string()))?;
    let model = Gmn0Model::from_dataset(&ds);
    let doc = gmn1_write(&model, &GmnDictionary::default())
        .map_err(|e| JsError::new(&e.to_string()))?;
    Ok(doc.text)
}

/// Read `gmn1_text` (a GMN-1 surface) back to **GMN-0** as canonical N-Quads — the
/// other leg of the round-trip. `to_gmn1` followed by `from_gmn1` is the byte-exact
/// GMN-1 round-trip witness the docs widget shows.
///
/// # Errors
///
/// Throws if the GMN-1 text cannot be read back.
#[wasm_bindgen]
pub fn from_gmn1(gmn1_text: &str) -> Result<String, JsError> {
    let doc = Gmn1Document::from_text(gmn1_text);
    let model = gmn1_read(&doc, &GmnDictionary::default())
        .map_err(|e| JsError::new(&e.to_string()))?;
    Ok(model.canonical_nquads())
}
