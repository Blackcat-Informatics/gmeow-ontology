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
//! The attestation is refreshed only by an explicit maintainer producer.
//!
//! ## Why `conjecture_test`, and why the attestation is bundle-stable
//!
//! [`REQUEST`] is a REAL `tools/call` frame: it goes through `handle_message`'s JSON-RPC
//! decode, the total tool dispatch over the assembled surface, the tool's own execution,
//! and the response-envelope encode — the entire engine, not a probe. `conjecture_test` is
//! chosen because its RESPONSE is a pure function of the request (the candidate formula is
//! evaluated against the KB carried IN the frame, in an isolated scenario world), so this
//! witness pins the ENGINE's frame handling and stays valid across bundle regeneration,
//! rather than re-freezing a slice of `gmeow.gts` content that a different gate already
//! owns. The bundle is still fully exercised: neither half can answer a frame at all until
//! a real 30-MB-plus snapshot has been imported, folded, and assembled into a surface by
//! `init`.
//!
//! It is also the RIGHT probe for what this crate now IS. The frame used to be a `convert`
//! — a CORE tool — from when this image was the whole engine. It is now the demand-loaded
//! reasoning segment, so `convert` correctly answers with `mcp.segment-not-loaded` here and
//! a witness pinned to it would pin the DEFERRAL rather than an answer. Pinning a reasoning
//! tool instead means the attestation proves the thing this segment exists to do: run the
//! native structured-DL conjecture engine, in the browser, byte-identically to native.
//!
//! ## Why it is compared against the FULL native engine
//!
//! `direct` drives [`McpServer::from_snapshot`] — every segment linked and served — while
//! the shim drives [`gmeow_mcp::SegmentSet::reasoning_only`]. Their byte-identity is the
//! deferral contract's fourth claim measured at the IMAGE boundary: a frame answered by the
//! demand-loaded segment is the SAME answer the undivided engine gives. The tiering is a
//! deployment shape, not a different engine.

use std::path::PathBuf;

use gmeow_mcp::McpServer;
use gmeow_mcp_wasm::{init, mcp, ready};
use serde_json::Value;

/// The exact JSON-RPC 2.0 request frame both halves send. A literal (not a `json!`
/// build) so the two halves provably send the same bytes: the JS test carries this
/// same string.
///
/// The candidate is a reified ground atom `ex:a rdf:type ex:B` and the KB already asserts
/// it, so the proof leg `KB ⊨ φ` fires and the verdict is CORROBORATED — the same
/// PROOF-leg demo `crates/reason-wasm/tests/witness_conjecture.rs` pins for the standalone
/// reasoner, so the two witnesses agree about the engine by construction.
const REQUEST: &str = concat!(
    r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"conjecture_test","#,
    r#""arguments":{"formula":"@prefix logic: <https://blackcatinformatics.ca/logic/> .\n"#,
    r#"@prefix ex: <http://ex/> .\n"#,
    r#"@prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .\n"#,
    r#"ex:phi a logic:Formula ;\n"#,
    r#"    logic:relation rdf:type ;\n"#,
    r#"    logic:argument [ logic:termIndex 0 ; logic:termIri ex:a ] ;\n"#,
    r#"    logic:argument [ logic:termIndex 1 ; logic:termIri ex:B ] .\n","#,
    r#""kb":"@prefix ex: <http://ex/> .\n"#,
    r#"@prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .\n"#,
    r#"ex:a rdf:type ex:B .\n","#,
    r#""standpoint":"https://blackcatinformatics.ca/gmeow/examples/conjecture/demo-standpoint"}}}"#,
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
    gmeow_bundle_import::load_authenticated_source_bytes(&repo_root())
        .expect("authenticated bundle; tests never produce it")
}

#[test]
fn no_snapshot_is_loaded_before_init() {
    // Each test gets its own engine slot — the handle is thread-local and nextest runs
    // one process per test — so this observes the genuine pre-`init` state.
    //
    // Only the STATE is asserted here, not the refusal itself: `mcp` before `init`
    // throws a `JsError` in wasm and deliberately panics with the same diagnostic in a
    // native parity process, where constructing `JsError` would invoke a missing wasm
    // import. The refusal is therefore asserted where it is real — the Node lane's
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
        "the conjecture_test tool must succeed: {out}"
    );
    let text = frame["result"]["content"][0]["text"]
        .as_str()
        .unwrap_or_else(|| panic!("the tool envelope carries text content: {out}"));
    let payload: Value = serde_json::from_str(text).expect("the conjecture_test payload is JSON");
    assert_eq!(
        payload["ok"],
        Value::Bool(true),
        "conjecture_test reports ok: {out}"
    );
    // The REASONER actually ran: this is the proof leg, so the verdict must be the
    // corroborated one. A witness that merely parsed the frame would pass on an engine
    // that answered "open" for everything.
    assert!(
        !text.contains("mcp.segment-not-loaded"),
        "conjecture_test is a REASONING-segment tool and this image IS that segment — it \
         must answer, never defer: {out}"
    );
    assert_eq!(
        payload["verdict"]["lifecycle"], "corroborated",
        "the proof leg `KB ⊨ φ` must corroborate: {out}"
    );
    assert_eq!(
        payload["verdict"]["evaluation"], "completed",
        "the evaluation must COMPLETE, not exhaust its budget: {out}"
    );
    let judgment = payload["judgment_nquads"]
        .as_str()
        .unwrap_or_else(|| panic!("the answered payload carries judgment_nquads: {out}"));
    assert!(
        !judgment
            .chars()
            .any(|c| c.is_control() && !matches!(c, '\n' | '\r' | '\t')),
        "the judgment transport must not contain raw control scalars: {out}"
    );
    purrdf::parse_dataset(judgment.as_bytes(), "application/n-quads", None)
        .unwrap_or_else(|error| panic!("judgment_nquads must be valid RDF: {error}\n{out}"));

    let path = attestation_path();
    let committed = std::fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!(
            "mcp witness attestation {} missing; refresh it through the explicit maintainer producer: {e}",
            path.display()
        )
    });
    assert_eq!(
        out, committed,
        "the native MCP response frame drifted from the committed witness attestation"
    );
}
