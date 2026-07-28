// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Native half of the MCP-engine native↔wasm parity WITNESS.
//!
//! Pattern followed: **`witness_gmn.rs`** — the native test drives the shim crate's OWN
//! exported functions (here [`gmeow_mcp_wasm::init`] / [`gmeow_mcp_wasm::mcp`], which the
//! `#[wasm_bindgen]` macro also compiles natively), so the pinned attestation is produced
//! by the very code the browser runs rather than by a native look-alike of it. The Node
//! lane (`crates/mcp-wasm/js/tests/witness.test.mjs`) drives the WASM `init`/`mcp` over
//! the SAME snapshot and the SAME frame and asserts byte-identity with the same
//! attestation; both matching the one attestation proves native ≡ wasm. Refreshed via
//! `GMEOW_WITNESS_BLESS=1`.
//!
//! ## Why `convert`, and why the attestation is bundle-stable
//!
//! [`REQUEST`] is a REAL `tools/call` frame: it goes through `handle_message`'s JSON-RPC
//! decode, the total tool dispatch over the assembled surface, the tool's own execution,
//! and the response-envelope encode — the entire engine, not a probe. `convert` is chosen
//! because its RESPONSE is a pure function of the request (a transcode of the inline
//! document), so this witness pins the ENGINE's frame handling and stays valid across
//! bundle regeneration, rather than re-freezing a slice of `gmeow.gts` content that a
//! different gate already owns. The bundle is still fully exercised: neither half can
//! answer a frame at all until a real 30-MB-plus snapshot has been imported, folded, and
//! assembled into a surface by `init`.
//!
//! The document carries an RDF-1.2 **quoted triple**, so the transcode leg that must
//! survive the boundary is the star-capable one — the part of the codec most likely to
//! diverge if the wasm image ever linked a different substrate.

use std::path::PathBuf;

use gmeow_mcp::McpServer;
use gmeow_mcp_wasm::{init, mcp, ready};
use serde_json::Value;

/// The exact JSON-RPC 2.0 request frame both halves send. A literal (not a `json!`
/// build) so the two halves provably send the same bytes: the JS test carries this
/// same string.
const REQUEST: &str = concat!(
    r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"convert","#,
    r#""arguments":{"data":"<http://example.org/r> <http://www.w3.org/1999/02/22-rdf-syntax-ns#reifies> "#,
    r#"<<( <http://example.org/s> <http://example.org/p> <http://example.org/o> )>> .\n","#,
    r#""from":"nt","to":"turtle"}}}"#,
);

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("repo root")
}

fn attestation_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/WITNESS.mcp.json")
}

/// The `gmeow.gts` snapshot both halves load. A generated artifact; without it (a bare
/// checkout that has not run `make regen`) the parity witness cannot run. That is
/// unfinished work for the sync gate, not a pass — surface it loudly.
fn snapshot() -> Vec<u8> {
    let path = repo_root().join("generated/dist/gmeow.gts");
    std::fs::read(&path).unwrap_or_else(|e| {
        panic!(
            "the MCP parity witness needs the generated bundle {} (run `make regen`): {e}",
            path.display()
        )
    })
}

#[test]
fn no_snapshot_is_loaded_before_init() {
    // Each test gets its own engine slot — the handle is thread-local and nextest runs
    // one process per test — so this observes the genuine pre-`init` state.
    //
    // Only the STATE is asserted here, not the refusal itself: `mcp` before `init`
    // returns a `JsError`, and CONSTRUCTING a `JsError` calls a wasm-bindgen imported
    // function, which panics by design on a non-wasm target. The refusal is therefore
    // asserted where it is real — the Node lane's
    // `mcp() refuses frames before a snapshot is loaded` — rather than faked here.
    assert!(!ready(), "no snapshot is loaded before init");
}

#[test]
fn native_mcp_frame_matches_the_witness_attestation() {
    let bundle = snapshot();

    // The engine driven DIRECTLY, in its own scope so the server is dropped before the
    // shim builds its own: two folded views of a multi-megabyte bundle alive at once
    // would double this proof's peak footprint for no added evidence.
    let direct = {
        let server = McpServer::from_snapshot(&bundle).expect("consumer server constructs");
        server.handle_message(REQUEST)
    };

    // The shim's lifecycle: hand the snapshot over once, then drive frames.
    assert!(!ready(), "no snapshot is loaded before init");
    init(&bundle).expect("the consumer MCP engine builds over the generated snapshot");
    assert!(ready(), "init installs the engine");

    let out = mcp(REQUEST).expect("the loaded engine answers the frame");

    // Deterministic, and identical to the engine driven directly — the shim adds a
    // snapshot handle and a string marshal, never a transformation of the frame.
    assert_eq!(
        out,
        mcp(REQUEST).expect("the loaded engine answers the frame"),
        "frame handling is deterministic"
    );
    assert_eq!(
        out, direct,
        "the shim's frame must be byte-identical to McpServer::handle_message driven directly"
    );

    // It must be a REAL answered tools/call, not an error envelope — otherwise the
    // witness would pin a failure and prove nothing about the tool surface.
    let frame: Value = serde_json::from_str(&out).expect("the response is a JSON-RPC frame");
    assert_eq!(frame["jsonrpc"], "2.0", "JSON-RPC envelope: {out}");
    assert_eq!(frame["id"], 1, "the response echoes the request id: {out}");
    assert_eq!(
        frame["result"]["isError"],
        Value::Bool(false),
        "the convert tool must succeed: {out}"
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

    let path = attestation_path();
    // Require the EXACT documented value: only `GMEOW_WITNESS_BLESS=1` may overwrite the
    // committed witness (an empty or `=0` value must not silently replace it).
    if std::env::var("GMEOW_WITNESS_BLESS").as_deref() == Ok("1") {
        std::fs::write(&path, &out).expect("write mcp attestation");
        eprintln!("blessed mcp witness at {}", path.display());
        return;
    }
    let committed = std::fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!(
            "mcp witness attestation {} missing (bless with GMEOW_WITNESS_BLESS=1): {e}",
            path.display()
        )
    });
    assert_eq!(
        out, committed,
        "the native MCP response frame drifted from the committed witness attestation — re-bless"
    );
}
