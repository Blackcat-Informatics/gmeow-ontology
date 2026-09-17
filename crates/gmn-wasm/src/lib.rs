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
    Gmn0Model, Gmn1Document, glyph_legend_json as bridge_glyph_legend_json,
    gmn1_codec::native::{self, NativeCodebook},
    gmn1_read, gmn1_write,
};
use wasm_bindgen::prelude::*;

/// Complete native tables projected by the optimized producer before this crate builds.
/// The browser embeds no authored RDF and performs no source compilation.
const NATIVE_CODEBOOK: &[u8] =
    include_bytes!("../../../generated/projections/lang/gmn-codebook.cbor");

/// Hydrate the source-pinned native codebook once for all browser codec operations.
///
/// Both transcode legs and the legend borrow the same prepared tables. A corrupted or
/// stale embedded artifact is a build-integrity failure, never a fallback to parsing RDF.
fn codebook() -> &'static NativeCodebook {
    static CODEBOOK: std::sync::OnceLock<NativeCodebook> = std::sync::OnceLock::new();
    CODEBOOK.get_or_init(|| {
        native::decode(NATIVE_CODEBOOK, native::SOURCE_BLAKE3)
            .expect("embedded native GMN codebook must match its exact producer-selected source")
    })
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
    let doc =
        gmn1_write(&model, codebook().dictionary()).map_err(|e| JsError::new(&e.to_string()))?;
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
    let model =
        gmn1_read(&doc, codebook().dictionary()).map_err(|e| JsError::new(&e.to_string()))?;
    Ok(model.canonical_nquads())
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
/// Returns a `JsError` (thrown to JS at the boundary) if the codebook carries a glyph
/// the pinned cost table does not price. Invalid embedded native tables hard-fail.
pub fn glyph_legend_json() -> Result<String, JsError> {
    bridge_glyph_legend_json(codebook().dictionary().glyph_registry())
        .map_err(|e| JsError::new(&e.to_string()))
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
/// Throws if an admitted glyph lacks its required pinned token cost. Invalid
/// embedded native tables hard-fail as a build-integrity violation.
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
    use std::collections::BTreeSet;

    /// The pinned per-glyph token cost table MUST equal the real `cl100k_base` BPE cost
    /// for every glyph the codebook registry can emit, and carry no stale entry. The
    /// exact source legend is prepared before tests start from the SAME authored
    /// codebook the browser embeds. Every observed glyph is measured independently,
    /// and the reverse inventory check rejects stale pinned entries. No test rebuilds
    /// the authored registry. A new glyph, shifted cost or removed glyph fails here.
    #[test]
    fn pinned_glyph_costs_match_the_real_bpe() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let bytes = gmeow_action_cache::selection::source_artifacts::load(
            &root,
            "stage-conformance",
            "pipeline/source-glyph-legend.json",
        )
        .expect("browser source legend has an authenticated producer selection");
        let legend: serde_json::Value = serde_json::from_slice(&bytes).expect("source legend JSON");
        let rows = legend.as_array().expect("source legend is an array");
        assert!(!rows.is_empty(), "the source registry must contain glyphs");
        let mut glyphs = BTreeSet::new();
        for row in rows {
            let glyph = row["glyph"].as_str().expect("source legend glyph");
            assert!(glyphs.insert(glyph), "source legend repeats {glyph:?}");
            let pinned = gmeow_lang_bridge::pinned_glyph_token_cost(glyph)
                .expect("every source glyph has a pinned token cost");
            assert_eq!(row["tokenCost"].as_u64(), Some(pinned as u64));
            assert_eq!(
                pinned,
                gmeow_lang_bridge::gmn_glyph_token_cost(glyph),
                "pinned cost differs from real cl100k_base BPE for {glyph:?}"
            );
        }
        for (glyph, _) in gmeow_lang_bridge::GLYPH_TOKEN_COSTS {
            assert!(
                glyphs.contains(glyph),
                "pinned table contains stale glyph {glyph:?}"
            );
        }
    }
}
