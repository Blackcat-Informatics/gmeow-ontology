// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! WebAssembly entry points for `gmeow-logic`.
//!
//! This module is compiled only when targeting `wasm32-unknown-unknown`.  It
//! exposes a `wasm-bindgen` surface that lets JavaScript hosts call into the
//! in-memory oxigraph world store.
//!
//! # What is exposed
//!
//! - [`version`]     — returns the crate version string; a lightweight smoke
//!   test that wasm-bindgen linkage and the JS runtime bridge are wired.
//! - [`materialize`] — round-trips an N-Quads string through the in-memory
//!   oxigraph [`Store`], then serialises the result back to N-Quads.  This
//!   exercises the real store code path (load → iterate → write) on the wasm
//!   target, proving that oxigraph with `features = ["rdf-12", "js"]` builds
//!   and operates correctly inside a wasm module.
//!
//! Nemo rule evaluation is not exposed here — Nemo's OS-level dependencies
//! (reqwest, tower-lsp) are unavailable on wasm32; the chase arrives in #501
//! on the native path only.

use wasm_bindgen::prelude::*;

use oxigraph::io::RdfFormat;
use oxigraph::store::Store;

// ── version ──────────────────────────────────────────────────────────────────

/// Return the `gmeow-logic` crate version string.
///
/// Useful as a zero-cost smoke test: if this returns the right string the
/// wasm-bindgen bridge, the wasm runtime, and basic Rust-to-JS marshalling are
/// all working.
#[wasm_bindgen]
pub fn version() -> String {
    env!("CARGO_PKG_VERSION").to_owned()
}

// ── materialize ───────────────────────────────────────────────────────────────

/// Load `input` N-Quads into an in-memory oxigraph [`Store`] and return the
/// round-tripped N-Quads output.
///
/// This proves that:
/// 1. oxigraph with `features = ["rdf-12", "js"]` compiles to wasm.
/// 2. The in-memory store (the "world" abstraction) works on wasm32.
/// 3. RDF 1.2 N-Quads parsing and serialisation work on wasm32.
///
/// Rule materialization (Nemo chase) is native-only; this wasm entry is the
/// store round-trip only.  An empty `input` string returns an empty string.
///
/// # Errors
///
/// Returns a JavaScript `Error` if the input cannot be parsed as N-Quads.
#[wasm_bindgen]
pub fn materialize(input: &str) -> Result<String, JsValue> {
    let store = Store::new().map_err(|e| JsValue::from_str(&format!("store error: {e}")))?;

    if !input.trim().is_empty() {
        store
            .load_from_reader(RdfFormat::NQuads, input.as_bytes())
            .map_err(|e| JsValue::from_str(&format!("N-Quads parse error: {e}")))?;
    }

    // Serialise all quads back to N-Quads.
    let mut out = Vec::new();
    store
        .dump_to_writer(RdfFormat::NQuads, &mut out)
        .map_err(|e| JsValue::from_str(&format!("serialisation error: {e}")))?;

    String::from_utf8(out).map_err(|e| JsValue::from_str(&format!("utf-8 error: {e}")))
}
