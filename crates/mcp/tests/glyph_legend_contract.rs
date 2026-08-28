// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! `gmn_glyph_legend` serves the SAME legend the browser codec shim serves.
//!
//! The legend used to be composed inside `gmeow-gmn-wasm` — the pinned cost table, the row
//! order, and the JSON shape all lived next to the `wasm_bindgen` export — so the docs
//! widget was the only consumer that could have one. `gmeow-mcp` may not depend on a
//! `cdylib` wasm shim, so the composition was HOISTED into `gmeow-lang-bridge` (which both
//! already depend on) and the shim became the thin marshal it always claimed to be.
//!
//! That hoist is only worth anything if the two callers genuinely agree, so this asserts
//! it two ways:
//!
//! * **Behaviour** — the tool's legend equals `gmeow_lang_bridge::glyph_legend_json` over a
//!   registry built from the shipped `lang:` codebook, which is EXACTLY what
//!   `gmeow_gmn_wasm::glyph_legend_json` returns (it parses that same file into that same
//!   registry and calls that same function). Byte equality, not "same set".
//! * **Structure** — the shim carries no cost table, no legend JSON assembly, and no
//!   second implementation to drift back into.
//!
//! It also proves something the wasm shim cannot: the legend the MCP tool serves is built
//! from the BUNDLE's `gmeow:gmnDictV3` registry, so agreeing with the codebook-derived
//! legend means the shipped bundle and the shipped codebook carry the same alphabet.

use std::path::{Path, PathBuf};

use gmeow_lang_bridge::{GmnGlyphRegistry, glyph_legend_json};
use gmeow_mcp::McpServer;
use serde_json::{Value, json};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("crates/mcp has a workspace root two levels up")
        .to_path_buf()
}

fn read(rel: &str) -> String {
    std::fs::read_to_string(repo_root().join(rel)).unwrap_or_else(|e| panic!("read {rel}: {e}"))
}

fn server() -> McpServer {
    let snapshot = gmeow_bundle_import::load_authenticated_source_bytes(&repo_root())
        .expect("authenticated snapshot; tests never produce it");
    McpServer::from_snapshot(&snapshot).expect("consumer server constructs")
}

/// The legend the wasm shim returns, reproduced through the SAME path it takes: parse the
/// embedded `lang:` codebook, build the glyph registry, compose through the one hoisted
/// implementation. `gmeow_gmn_wasm::glyph_legend_json` is these three lines plus a
/// `JsError` marshal, so this IS the shim's answer without linking a `cdylib`.
fn shim_legend() -> String {
    let codebook = read("slices/grounding/lang/module.ttl");
    let dataset = purrdf::parse_dataset(codebook.as_bytes(), "text/turtle", None)
        .expect("the lang: codebook parses");
    let registry = GmnGlyphRegistry::from_dataset(&dataset).expect("glyph registry builds");
    glyph_legend_json(&registry).expect("legend composes")
}

fn tool_legend(server: &McpServer) -> Value {
    let envelope = server.call_tool_result("gmn_glyph_legend", &json!({}));
    let text = envelope["content"][0]["text"]
        .as_str()
        .unwrap_or_else(|| panic!("gmn_glyph_legend returned no text: {envelope}"));
    let payload: Value = serde_json::from_str(text).expect("tool text is JSON");
    assert_eq!(
        envelope.get("isError"),
        Some(&json!(false)),
        "gmn_glyph_legend hard-failed: {payload}"
    );
    assert_eq!(payload["ok"], json!(true), "{payload}");
    payload["legend"].clone()
}

/// The load-bearing equality: the agent's legend IS the browser's legend.
#[test]
fn the_mcp_tool_returns_the_same_legend_the_wasm_shim_returns() {
    let legend = tool_legend(&server());
    let shim: Value = serde_json::from_str(&shim_legend()).expect("shim legend is JSON");
    assert_eq!(
        legend, shim,
        "the MCP legend (built from the bundle's gmnDictV3 registry) must equal the wasm \
         shim's legend (built from the shipped lang: codebook) — one implementation, one \
         alphabet"
    );
}

/// The legend is a non-empty array of `{glyph, tokenCost}` rows with real, positive costs
/// — never an empty list, and never a row missing its price.
#[test]
fn the_legend_prices_every_glyph_it_lists() {
    let legend = tool_legend(&server());
    let rows = legend.as_array().expect("the legend is a JSON array");
    assert!(
        !rows.is_empty(),
        "the legend must list the glyphs the codec may emit, not an empty alphabet"
    );
    for row in rows {
        let glyph = row["glyph"].as_str().unwrap_or_else(|| {
            panic!("every legend row carries a `glyph` string: {row}");
        });
        assert!(!glyph.is_empty(), "a legend row carries an empty glyph");
        let cost = row["tokenCost"]
            .as_u64()
            .unwrap_or_else(|| panic!("every legend row carries a `tokenCost` number: {row}"));
        assert!(cost > 0, "glyph {glyph:?} is priced at zero tokens: {row}");
    }
}

/// The legend is deterministic: the same server answers the same bytes every time, so a
/// cached agent context never sees the alphabet shuffle under it.
#[test]
fn the_legend_is_deterministic() {
    let server = server();
    assert_eq!(tool_legend(&server), tool_legend(&server));
}

/// The hoist is structural, not just behavioural: the shim must carry NO cost table and NO
/// legend JSON assembly of its own. A copy that agrees today is still a copy.
#[test]
fn the_wasm_shim_carries_no_second_legend_implementation() {
    let shim = read("crates/gmn-wasm/src/lib.rs");
    assert!(
        !shim.contains("GLYPH_TOKEN_COSTS: &[(&str, usize)]"),
        "the pinned cost table must live in gmeow-lang-bridge, not be redeclared in the \
         wasm shim"
    );
    assert!(
        !shim.contains("\\\"tokenCost\\\""),
        "the legend's JSON shape must be assembled in gmeow-lang-bridge, not re-assembled \
         in the wasm shim"
    );
    assert!(
        shim.contains("bridge_glyph_legend_json"),
        "the wasm shim must marshal over the hoisted gmeow-lang-bridge implementation"
    );

    let mcp = read("crates/mcp/src/lib.rs");
    assert!(
        mcp.contains("gmeow_lang_bridge::glyph_legend_json(dict.glyph_registry())"),
        "the MCP tool must call the hoisted implementation over the BUNDLE's glyph \
         registry, never re-compose a legend"
    );
    assert!(
        !mcp.contains("gmeow_gmn_wasm"),
        "gmeow-mcp must never depend on the cdylib wasm shim"
    );
}
