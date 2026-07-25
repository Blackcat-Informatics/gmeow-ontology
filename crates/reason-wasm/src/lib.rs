// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! # gmeow-reason-wasm — the native GMEOW reasoner, in the browser
//!
//! Compiles the native `gmeow-logic` structured-DL reasoner to
//! `wasm32-unknown-unknown` and exposes it to JavaScript/TypeScript, so the live
//! documentation entailment panel + conjecture playground run the SAME chase the
//! on-gate authority runs — client-side, no server, no repository.
//!
//! ## Scope
//!
//! - **The real chase.** `reason` parses authored RDF, runs
//!   [`gmeow_logic::reason::reason_closure_dataset`] (the structured-DL closure), and
//!   serializes the INFERRED triples to N-Quads. On wasm the chase runs SERIALLY (the
//!   single-threaded rayon fallback + the `should_parallelize` gate), byte-identical
//!   to the parallel path (proven natively by the rule-parallel evidence probe), so
//!   the browser reasoner is functionally complete — it sheds only performance.
//! - **Thin shim.** All reasoning logic lives in `gmeow-logic` (native-tested); this
//!   crate only marshals strings/bytes across the JS boundary, exactly as
//!   `gmeow-validate-wasm` wraps the validator.

use wasm_bindgen::prelude::*;

/// The reasoner version (the crate's SemVer), exposed to JS as `version()` — a
/// liveness probe proving the wasm module instantiated and the reasoner core linked.
#[wasm_bindgen]
pub fn version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

/// Run the structured-DL chase over `data` (RDF text in `format`) and return the
/// **reasoned closure** — the inferred triples — as N-Quads text.
///
/// - `data` — the RDF document to reason over (UTF-8 text).
/// - `format` — a media type / short id purrdf understands (`turtle`/`ttl`,
///   `n-triples`/`nt`, `n-quads`/`nq`, `trig`, `rdf+xml`, `json-ld`).
///
/// # Errors
///
/// Throws a JS exception if the data cannot be parsed, reasoning fails, or the
/// closure cannot be serialized.
#[wasm_bindgen]
pub fn reason(data: &str, format: &str) -> Result<String, JsError> {
    let edb = purrdf::parse_dataset(data.as_bytes(), format, None)
        .map_err(|e| JsError::new(&e.to_string()))?;
    let closure =
        gmeow_logic::reason::reason_closure_dataset(&edb).map_err(|e| JsError::new(e.message()))?;
    let bytes = purrdf::serialize_dataset(
        &*closure,
        "application/n-quads",
        purrdf::SerializeGraph::Dataset,
    )
    .map_err(|e| JsError::new(&e.to_string()))?;
    String::from_utf8(bytes).map_err(|e| JsError::new(&format!("closure N-Quads not UTF-8: {e}")))
}

/// Test a candidate `logic:` formula against a KB with the native SYMMETRIC conjecture
/// engine and return the **deterministic verdict** as N-Triples text — the SAME projection
/// the on-gate MCP / CLI surface emits (proven byte-identical by the native≡wasm conjecture
/// witness). Powers the live documentation conjecture playground (the WASM-interactive docs W4 deliverable).
///
/// - `kb` — the knowledge base to test against (RDF text in `kb_format`).
/// - `kb_format` — a media type / short id purrdf understands (`turtle`/`ttl`,
///   `n-triples`/`nt`, `n-quads`/`nq`, `trig`, `rdf+xml`, `json-ld`).
/// - `formula` — the candidate `logic:` document naming exactly one `logic:Formula` / axiom.
/// - `standpoint` — the reified standpoint IRI the verdict is scoped to (REQUIRED; a
///   conjecture verdict is always standpoint-scoped, never global — Principle 9).
///
/// The symmetric two legs (proof `KB ⊨ φ` and counterproof `KB ∪ {φ} ⊨ ⊥`) and the Belnap
/// classification are all readable from the returned N-Triples; the JS controller renders
/// them side-by-side.
///
/// # Errors
///
/// Throws a JS exception if the candidate does not name exactly one formula, if the KB
/// cannot be parsed, or if the native conjecture engine fails.
#[wasm_bindgen]
pub fn conjecture(
    kb: &str,
    kb_format: &str,
    formula: &str,
    standpoint: &str,
) -> Result<String, JsError> {
    let projection =
        gmeow_logic::conjecture_eval::evaluate_conjecture_kb(formula, kb, kb_format, standpoint)
            .map_err(|e| JsError::new(e.message()))?;
    Ok(projection.verdict_nt)
}
