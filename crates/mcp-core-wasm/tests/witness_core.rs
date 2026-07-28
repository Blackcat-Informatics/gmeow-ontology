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
//! both matching proves native ≡ wasm. Refreshed via `GMEOW_WITNESS_BLESS=1`.
//!
//! ## Two frames, because this image has two behaviours to pin
//!
//! * [`CORE_REQUEST`] is a real `convert` call — the first-load image answering for real,
//!   through the whole engine (JSON-RPC decode, total dispatch, tool execution, envelope
//!   encode). It is byte-for-byte the frame `crates/mcp-wasm`'s witness pins, which is the
//!   point: a core tool must answer IDENTICALLY in both tiers, so the same attestation
//!   bytes are expected from the lean image and the full one.
//! * [`DEFERRED_REQUEST`] is a real `coherence_certificate` call — a reasoning-segment tool
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
    r#""params":{"name":"coherence_certificate","arguments":{}}}"#,
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
    let path = repo_root().join("generated/dist/gmeow.gts");
    std::fs::read(&path).unwrap_or_else(|e| {
        panic!(
            "the core-engine parity witness needs the generated bundle {} (run `make regen`): {e}",
            path.display()
        )
    })
}

/// Compare against the committed attestation, or rewrite it under the EXACT documented
/// value `GMEOW_WITNESS_BLESS=1` (an empty or `=0` value must not silently replace it).
fn pin(frame: &str, name: &str) {
    let path = attestation_path(name);
    if std::env::var("GMEOW_WITNESS_BLESS").as_deref() == Ok("1") {
        std::fs::write(&path, frame).unwrap_or_else(|e| panic!("write {}: {e}", path.display()));
        eprintln!("blessed core witness at {}", path.display());
        return;
    }
    let committed = std::fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!(
            "core witness attestation {} missing (bless with GMEOW_WITNESS_BLESS=1): {e}",
            path.display()
        )
    });
    assert_eq!(
        frame, committed,
        "the native core-engine response frame drifted from the committed witness \
         attestation — re-bless"
    );
}

#[test]
fn no_snapshot_is_loaded_before_init() {
    // Each test gets its own engine slot — the handle is thread-local and nextest runs
    // one process per test — so this observes the genuine pre-`init` state.
    //
    // Only the STATE is asserted here, not the refusal itself: `mcp` before `init`
    // returns a `JsError`, and CONSTRUCTING a `JsError` calls a wasm-bindgen imported
    // function, which panics by design on a non-wasm target. The refusal is therefore
    // asserted where it is real — the Node lane — rather than faked here.
    assert!(!ready(), "no snapshot is loaded before init");
}

#[test]
fn the_lean_core_answers_a_core_frame_and_matches_the_witness_attestation() {
    let bundle = snapshot();
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
}

#[test]
fn the_lean_core_defers_a_segment_frame_and_matches_the_witness_attestation() {
    let bundle = snapshot();
    init(&bundle).expect("the lean core MCP engine builds over the generated snapshot");

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
        payload["tool"], "coherence_certificate",
        "the signal names the tool asked for: {out}"
    );
    assert_eq!(
        payload["segment"],
        gmeow_mcp::REASONING_SEGMENT,
        "the signal names the segment to load: {out}"
    );

    pin(&out, "WITNESS.core-deferral.json");
}
