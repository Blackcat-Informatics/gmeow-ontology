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

/// The GMN codebook — the `lang:` slice's authored glyph/alias/prefix declarations
/// the GMN-1 dictionary is minted from. Embedded so the browser widget is
/// self-contained (no runtime codebook fetch); refreshed when the vendored engine is.
const LANG_CODEBOOK: &str = include_str!("../../../slices/grounding/lang/module.ttl");

/// Build the pinned GMN-1 dictionary from the embedded codebook.
///
/// Both legs of the transcode consult the SAME dictionary; the browser and the native
/// witness both mint it from these exact bytes, so their glyph/alias resolution is
/// identical by construction.
fn codebook_dict() -> Result<GmnDictionary, String> {
    let ds = purrdf::parse_dataset(LANG_CODEBOOK.as_bytes(), "text/turtle", None)
        .map_err(|e| e.to_string())?;
    GmnDictionary::from_dataset(&ds).map_err(|e| e.0)
}

/// Transcode `data` (RDF text in `format`) — its GMN-0 normal form — into the
/// token-compact **GMN-1** surface text. The native witness (`witness_gmn.rs`) calls
/// THIS function, so the browser output is byte-identical to the pinned attestation.
///
/// # Errors
///
/// Returns the codec error string if the RDF cannot be parsed or the GMN-1 write fails.
pub fn transcode_to_gmn1(data: &str, format: &str) -> Result<String, String> {
    let ds = purrdf::parse_dataset(data.as_bytes(), format, None).map_err(|e| e.to_string())?;
    let model = Gmn0Model::from_dataset(&ds);
    let doc = gmn1_write(&model, &codebook_dict()?).map_err(|e| e.to_string())?;
    Ok(doc.text)
}

/// Read `gmn1_text` (a GMN-1 surface) back to **GMN-0** as canonical N-Quads — the
/// other leg of the round-trip. `transcode_to_gmn1` then `transcode_from_gmn1` is the
/// byte-exact GMN-1 round-trip the docs widget shows and the witness pins.
///
/// # Errors
///
/// Returns the codec error string if the GMN-1 text cannot be read back.
pub fn transcode_from_gmn1(gmn1_text: &str) -> Result<String, String> {
    let doc = Gmn1Document::from_text(gmn1_text);
    let model = gmn1_read(&doc, &codebook_dict()?).map_err(|e| e.to_string())?;
    Ok(model.canonical_nquads())
}

/// The codec version (the crate's SemVer), exposed to JS as `version()`.
#[wasm_bindgen]
pub fn version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

/// wasm export: transcode RDF text to the GMN-1 surface. Thin marshal over
/// [`transcode_to_gmn1`].
///
/// # Errors
///
/// Throws if the RDF cannot be parsed or the GMN-1 write fails.
#[wasm_bindgen]
pub fn to_gmn1(data: &str, format: &str) -> Result<String, JsError> {
    transcode_to_gmn1(data, format).map_err(|e| JsError::new(&e))
}

/// wasm export: read a GMN-1 surface back to canonical N-Quads. Thin marshal over
/// [`transcode_from_gmn1`].
///
/// # Errors
///
/// Throws if the GMN-1 text cannot be read back.
#[wasm_bindgen]
pub fn from_gmn1(gmn1_text: &str) -> Result<String, JsError> {
    transcode_from_gmn1(gmn1_text).map_err(|e| JsError::new(&e))
}
