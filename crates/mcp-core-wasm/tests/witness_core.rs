// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Native half of the LEAN-CORE engine's native↔wasm parity WITNESS.
//!
//! Pattern followed: **`crates/mcp-wasm/tests/witness_mcp.rs`** — the native test drives
//! this shim's OWN exported functions ([`gmeow_mcp_core_wasm::init`] /
//! [`gmeow_mcp_core_wasm::mcp`], which the `#[wasm_bindgen]` macro also compiles
//! natively), so the pinned attestation is produced by the very code the browser runs
//! rather than by a native look-alike of it. The Node lane
//! (`crates/mcp-core-wasm/js/tests/witness.test.mjs`) drives the WASM `init`/`mcp` over the
//! SAME snapshot and the SAME frames and asserts byte-identity with the same attestations;
//! both matching proves native ≡ wasm. Refresh is an explicit maintainer producer.
//!
//! ## Two frames, because this image has two behaviours to pin
//!
//! * [`CORE_REQUEST`] is a real `convert` call — the first-load image answering for real,
//!   through the whole engine (JSON-RPC decode, total dispatch, tool execution, envelope
//!   encode). It is byte-for-byte the frame `crates/mcp-wasm`'s witness pins, which is the
//!   point: a core tool must answer IDENTICALLY in both tiers, so the same attestation
//!   bytes are expected from the lean image and the full one.
//! * [`DEFERRED_REQUEST`] is a real `recall` call — a reasoning-segment tool the core image
//!   defers and the reasoning image ANSWERS, which is what makes the re-dispatch half of the
//!   witness a round trip rather than a second refusal. (The whole-bundle chase is its own
//!   tier and no wasm image serves it, so it could not close that loop.)
//!   against an image that does not link the reasoner. Pinning its response is what makes
//!   the deferral signal a contract rather than an implementation detail: the host's
//!   demand-loader parses exactly these bytes.
//!
//! Both are pure functions of the request (a transcode, and a fixed routing signal), so the
//! attestations stay valid across bundle regeneration rather than re-freezing a slice of
//! `gmeow.gts` content that a different gate already owns. The bundle is still fully
//! exercised: neither half can answer a frame at all until a real 30-MB-plus snapshot has
//! been imported, folded, and assembled into a surface by `init`.

use std::path::PathBuf;

use gmeow_mcp_core_wasm::{init, mcp, ready};
use serde_json::Value;

/// The `convert` frame, byte-for-byte the one `crates/mcp-wasm/tests/witness_mcp.rs` pins.
const CORE_REQUEST: &str = concat!(
    r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"convert","#,
    r#""arguments":{"data":"<http://example.org/r> <http://www.w3.org/1999/02/22-rdf-syntax-ns#reifies> "#,
    r#"<<( <http://example.org/s> <http://example.org/p> <http://example.org/o> )>> .\n","#,
    r#""from":"nt","to":"turtle"}}}"#,
);

/// A reasoning-segment frame: input-free, so both halves provably send the same bytes.
const DEFERRED_REQUEST: &str = concat!(
    r#"{"jsonrpc":"2.0","id":2,"method":"tools/call","#,
    r#""params":{"name":"recall","arguments":{"query":"anything"}}}"#,
);

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("repo root")
}

fn attestation_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join(name)
}

/// The `gmeow.gts` snapshot both halves load. A generated artifact; without it (a bare
/// checkout that has not run `make regen`) the parity witness cannot run. That is
/// unfinished work for the sync gate, not a pass — surface it loudly.
fn snapshot() -> Vec<u8> {
    gmeow_bundle_import::load_authenticated_source_bytes(&repo_root())
        .expect("authenticated bundle; tests never produce it")
}

/// Compare against the committed attestation. Tests are strictly read-only.
fn pin(frame: &str, name: &str) {
    let path = attestation_path(name);
    let committed = std::fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!(
            "core witness attestation {} missing; refresh it through the explicit maintainer producer: {e}",
            path.display()
        )
    });
    assert_eq!(
        frame, committed,
        "the native core-engine response frame drifted from the committed witness \
         attestation"
    );
}

#[test]
fn lean_core_native_witnesses_share_one_authenticated_import() {
    let bundle = snapshot();
    // Only the STATE is asserted before init: constructing the wasm-bindgen `JsError`
    // returned by an early `mcp` call is intentionally unavailable on a native target.
    assert!(!ready(), "no snapshot is loaded before init");
    init(&bundle).expect("the lean core MCP engine builds over the generated snapshot");
    assert!(ready(), "init installs the engine");

    let out = mcp(CORE_REQUEST).expect("the loaded engine answers the frame");
    assert_eq!(
        out,
        mcp(CORE_REQUEST).expect("the loaded engine answers the frame"),
        "frame handling is deterministic"
    );

    // A REAL answered tools/call, not an error envelope — otherwise the witness would pin
    // a failure and prove nothing about the first-load surface.
    let frame: Value = serde_json::from_str(&out).expect("the response is a JSON-RPC frame");
    assert_eq!(frame["jsonrpc"], "2.0", "JSON-RPC envelope: {out}");
    assert_eq!(
        frame["result"]["isError"],
        Value::Bool(false),
        "the convert tool must succeed in the lean image: {out}"
    );
    let text = frame["result"]["content"][0]["text"]
        .as_str()
        .unwrap_or_else(|| panic!("the tool envelope carries text content: {out}"));
    let payload: Value = serde_json::from_str(text).expect("the convert payload is JSON");
    assert_eq!(
        payload["ok"],
        Value::Bool(true),
        "convert reports ok: {out}"
    );
    assert!(
        payload["output"]
            .as_str()
            .is_some_and(|o| o.contains("<<(")),
        "the RDF-1.2 quoted triple must survive the transcode: {out}"
    );

    pin(&out, "WITNESS.core.json");

    // Keep the deferred-segment half in this same process: both contracts use one
    // immutable engine, so a second 140 MiB pack restore and multi-gigabyte index is
    // duplicate setup rather than independent evidence.
    let out = mcp(DEFERRED_REQUEST).expect("the loaded engine answers the frame");
    assert_eq!(
        out,
        mcp(DEFERRED_REQUEST).expect("the loaded engine answers the frame"),
        "the routing signal is deterministic"
    );

    let frame: Value = serde_json::from_str(&out).expect("the response is a JSON-RPC frame");
    assert!(
        frame.get("error").is_none(),
        "a deferral rides IN the result envelope, never as a protocol error: {out}"
    );
    let text = frame["result"]["content"][0]["text"]
        .as_str()
        .unwrap_or_else(|| panic!("the tool envelope carries text content: {out}"));
    let payload: Value = serde_json::from_str(text).expect("the deferral payload is JSON");
    assert_eq!(
        payload["code"], "mcp.segment-not-loaded",
        "the lean image must route, not refuse: {out}"
    );
    assert_eq!(
        payload["tool"], "recall",
        "the signal names the tool asked for: {out}"
    );
    assert_eq!(
        payload["segment"],
        gmeow_mcp::SegmentSet::segment_of(
            payload["tool"].as_str().expect("the signal names its tool")
        ),
        "the signal names the segment to load: {out}"
    );

    pin(&out, "WITNESS.core-deferral.json");
}
