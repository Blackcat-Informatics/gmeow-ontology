// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! # gmeow-gmn-wasm — the GMEOW Model Notation codec, in the browser
//!
//! Compiles the shipped GMN-0↔GMN-1 codec (`gmeow-lang-bridge`) to
//! `wasm32-unknown-unknown` and exposes it to JavaScript/TypeScript, so the docs GMN
//! transcode widget turns authored RDF into the token-compact GMN-1 surface — and
//! back — client-side, using the SAME codec + glyph symbology the on-gate authority
//! ships. GMN-2 (lossy compaction) and the zstd-dictionary transport are NOT here —
//! that notation is still being built in a later notation epic.
//!
//! Thin shim: all codec logic lives in `gmeow-lang-bridge` (native-tested with the
//! byte-exact round-trip witness); this only marshals across the JS boundary.

use gmeow_lang_bridge::{
    Gmn0Model, Gmn1Document, GmnDictionary, GmnGlyphRegistry,
    glyph_legend_json as bridge_glyph_legend_json, gmn1_read, gmn1_write,
};
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
fn codebook_dict() -> Result<GmnDictionary, JsError> {
    let ds = purrdf::parse_dataset(LANG_CODEBOOK.as_bytes(), "text/turtle", None)
        .map_err(|e| JsError::new(&e.to_string()))?;
    GmnDictionary::from_dataset(&ds).map_err(|e| JsError::new(&e.0))
}

/// Transcode `data` (RDF text in `format`) — its GMN-0 normal form — into the
/// token-compact **GMN-1** surface text. The native witness (`witness_gmn.rs`) calls
/// THIS function, so the browser output is byte-identical to the pinned attestation.
///
/// # Errors
///
/// Returns a `JsError` (thrown to JS at the boundary) if the RDF cannot be parsed or the
/// GMN-1 write fails.
pub fn transcode_to_gmn1(data: &str, format: &str) -> Result<String, JsError> {
    let ds = purrdf::parse_dataset(data.as_bytes(), format, None)
        .map_err(|e| JsError::new(&e.to_string()))?;
    let model = Gmn0Model::from_dataset(&ds);
    let doc = gmn1_write(&model, &codebook_dict()?).map_err(|e| JsError::new(&e.to_string()))?;
    Ok(doc.text)
}

/// Read `gmn1_text` (a GMN-1 surface) back to **GMN-0** as canonical N-Quads — the
/// other leg of the round-trip. `transcode_to_gmn1` then `transcode_from_gmn1` is the
/// byte-exact GMN-1 round-trip the docs widget shows and the witness pins.
///
/// # Errors
///
/// Returns a `JsError` (thrown to JS at the boundary) if the GMN-1 text cannot be read back.
pub fn transcode_from_gmn1(gmn1_text: &str) -> Result<String, JsError> {
    let doc = Gmn1Document::from_text(gmn1_text);
    let model = gmn1_read(&doc, &codebook_dict()?).map_err(|e| JsError::new(&e.to_string()))?;
    Ok(model.canonical_nquads())
}

/// The glyph registry of the embedded codebook — the inventory half of the legend.
///
/// # Errors
///
/// Returns a `JsError` if the embedded codebook cannot be parsed or the registry cannot be
/// built from it.
fn codebook_glyph_registry() -> Result<GmnGlyphRegistry, JsError> {
    let ds = purrdf::parse_dataset(LANG_CODEBOOK.as_bytes(), "text/turtle", None)
        .map_err(|e| JsError::new(&e.to_string()))?;
    GmnGlyphRegistry::from_dataset(&ds).map_err(|e| JsError::new(&format!("{e:?}")))
}

/// The GMN-1 glyph legend for the codebook, as a deterministic JSON array of
/// `{ "glyph": <token>, "tokenCost": <n> }` — the two machine primitives the symbology
/// plane defines (the glyph inventory + each glyph's real LLM-token cost). The widget
/// renders it as a hover legend beside the live transcode, so a reader can see which
/// glyphs the codec may emit and what each costs on the token channel.
///
/// Thin marshal, like every other function here: the pinned cost table, the row order, and
/// the JSON shape all live in [`gmeow_lang_bridge::gmn_legend`], so the browser legend and
/// the MCP `gmn_glyph_legend` tool are ONE implementation over the same glyph registry
/// rather than two that could drift.
///
/// # Errors
///
/// Returns a `JsError` (thrown to JS at the boundary) if the embedded codebook cannot be
/// read, or if it carries a glyph the pinned cost table does not price.
pub fn glyph_legend_json() -> Result<String, JsError> {
    let registry = codebook_glyph_registry()?;
    bridge_glyph_legend_json(&registry).map_err(|e| JsError::new(&e.to_string()))
}

/// The codec version (the crate's SemVer), exposed to JS as `version()`.
#[wasm_bindgen]
pub fn version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

/// wasm export: the GMN-1 glyph legend as JSON. Thin marshal over
/// [`glyph_legend_json`].
///
/// # Errors
///
/// Throws if the embedded codebook cannot be read.
#[wasm_bindgen]
pub fn glyph_legend() -> Result<String, JsError> {
    glyph_legend_json()
}

/// wasm export: transcode RDF text to the GMN-1 surface. Thin marshal over
/// [`transcode_to_gmn1`].
///
/// # Errors
///
/// Throws if the RDF cannot be parsed or the GMN-1 write fails.
#[wasm_bindgen]
pub fn to_gmn1(data: &str, format: &str) -> Result<String, JsError> {
    transcode_to_gmn1(data, format)
}

/// wasm export: read a GMN-1 surface back to canonical N-Quads. Thin marshal over
/// [`transcode_from_gmn1`].
///
/// # Errors
///
/// Throws if the GMN-1 text cannot be read back.
#[wasm_bindgen]
pub fn from_gmn1(gmn1_text: &str) -> Result<String, JsError> {
    transcode_from_gmn1(gmn1_text)
}

// The token-cost anti-rot gate. Native-only because the ground truth
// (`gmn_glyph_token_cost`) embeds a ~1.7 MB tiktoken vocabulary this crate keeps out of
// the shipped wasm image by taking `gmeow-lang-bridge` with `default-features = false`;
// the measurement comes back in through the dev-dependency that re-enables `glyph-cost`,
// and the shipped `glyph_legend_json` still reads the pinned `GLYPH_TOKEN_COSTS`.
#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::codebook_glyph_registry;

    /// The pinned per-glyph token cost table MUST equal the real `cl100k_base` BPE cost
    /// for every glyph the codebook registry can emit, and carry no stale entry. The
    /// assertion itself lives with the table it guards
    /// (`gmeow_lang_bridge::gmn_legend::assert_pinned_costs_match_the_real_bpe`); what THIS
    /// crate contributes is the registry, bound from the SAME embedded codebook
    /// `glyph_legend_json` serves, so the browser's pinned costs can never drift from the
    /// tokenizer the native authority measures. A new glyph, a shifted cost, or a removed
    /// glyph fails here until the table is re-pinned.
    #[test]
    fn pinned_glyph_costs_match_the_real_bpe() {
        let registry = codebook_glyph_registry().expect("embedded codebook builds a registry");
        gmeow_lang_bridge::assert_pinned_costs_match_the_real_bpe(&registry);
    }
}
