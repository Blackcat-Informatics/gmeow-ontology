// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The DEFERRAL CONTRACT: a lean deployment is a smaller image, never a smaller engine.
//!
//! The browser console ships in two tiers — a core image loaded eagerly and a reasoning
//! segment fetched on first use. That is a legitimate deployment choice only if the
//! deferral is *invisible except in latency*. The failure mode this file exists to make
//! impossible is the tempting one: quietly dropping the reasoning tools from the lean
//! image and calling the result "the console".
//!
//! Four claims, each falsifiable:
//!
//! 1. A reasoning-segment tool called against a core deployment returns EXACTLY the typed
//!    routing signal — `mcp.segment-not-loaded`, naming the tool and the segment — and not
//!    a generic error, an empty result, or an unknown-tool refusal.
//! 2. A core tool called against a core deployment returns a real answer, so the tier is a
//!    partition of *where work runs*, not a global degradation.
//! 3. `tools/list` from a core deployment advertises the WHOLE surface, and that surface is
//!    still exactly the action theory's `logic:mcpToolName` set — discovery, and the theory
//!    that governs dispatch, cannot tell the tiers apart.
//! 4. The very frame a core deployment deferred, replayed against a deployment that serves
//!    the segment, returns a real answer byte-identical to the one the DEFAULT native
//!    constructor gives. Re-dispatch is lossless: the caller waited, it did not settle.
//!
//! Claims 1–3 hold on either feature set (a core deployment is expressible on a build that
//! links the segment, which is what makes them testable at all under cargo's feature
//! unification). Claim 4 needs a build that links the segment, and says so with a `cfg`
//! rather than by silently passing vacuously when it does not.

use std::path::PathBuf;

use gmeow_mcp::{McpServer, REASONING_SEGMENT, REASONING_SEGMENT_TOOLS, SegmentSet};
use serde_json::Value;

/// A `tools/call` frame for one of the reasoning-segment tools.
///
/// `coherence_certificate` is the chosen probe because it is INPUT-FREE (the frame is a
/// literal, so both tiers provably receive the same bytes) and because on a deployment
/// that serves the segment it answers from the bundle's carried certificate rather than by
/// re-reasoning — so claim 4 exercises a real answer without a multi-minute closure.
const HEAVY_FRAME: &str = concat!(
    r#"{"jsonrpc":"2.0","id":7,"method":"tools/call","#,
    r#""params":{"name":"coherence_certificate","arguments":{}}}"#,
);

/// A `tools/call` frame for a CORE tool, answered in the first-load image.
///
/// `convert` is a pure function of the request (a transcode of the inline document), so
/// its success says something about the engine rather than about the bundle's contents.
const CORE_FRAME: &str = concat!(
    r#"{"jsonrpc":"2.0","id":8,"method":"tools/call","params":{"name":"convert","#,
    r#""arguments":{"data":"<http://example.org/s> <http://example.org/p> <http://example.org/o> .\n","#,
    r#""from":"nt","to":"turtle"}}}"#,
);

/// A `tools/call` frame for `reason_graph`, the OTHER kind of reasoning-segment tool.
///
/// `coherence_certificate` above is input-free and answers from a carried artifact;
/// `reason_graph` takes caller data and actually runs the chase. Both must defer
/// identically, so the routing is proven over a tool whose deferral costs the caller a real
/// computation rather than a lookup — the case where a silent degradation would be most
/// tempting and least visible. The input is two triples, so the replay is fast.
const REASON_GRAPH_FRAME: &str = concat!(
    r#"{"jsonrpc":"2.0","id":11,"method":"tools/call","params":{"name":"reason_graph","#,
    r#""arguments":{"data":"<http://ex/a> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <http://ex/B> .\n","#,
    r#""format":"nt"}}}"#,
);

/// A `tools/call` frame for `encode_gmn1`, the codec's WRITE leg — a CORE tool that takes
/// caller data, so the core tier is exercised on a computed answer and not only on a
/// transcode.
const ENCODE_GMN1_FRAME: &str = concat!(
    r#"{"jsonrpc":"2.0","id":12,"method":"tools/call","params":{"name":"encode_gmn1","#,
    r#""arguments":{"data":"<http://ex/a> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <http://ex/B> .\n","#,
    r#""format":"nt"}}}"#,
);

const TOOLS_LIST_FRAME: &str = r#"{"jsonrpc":"2.0","id":9,"method":"tools/list","params":{}}"#;

const ACTION_POLICY_FRAME: &str = concat!(
    r#"{"jsonrpc":"2.0","id":10,"method":"tools/call","#,
    r#""params":{"name":"action_policy","arguments":{}}}"#,
);

/// The datatype property tying an action schema to the MCP wire name it governs.
const LOGIC_MCP_TOOL_NAME: &str = "https://blackcatinformatics.ca/logic/mcpToolName";

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("repo root")
}

/// The `gmeow.gts` snapshot every engine here is built over. A generated artifact; without
/// it (a bare checkout that has not run `make regen`) the contract cannot be exercised.
/// That is unfinished work for the sync gate, not a pass — surface it loudly.
fn snapshot() -> Vec<u8> {
    let path = repo_root().join("generated/dist/gmeow.gts");
    std::fs::read(&path).unwrap_or_else(|e| {
        panic!(
            "the segment-deferral contract needs the generated bundle {} (run `make regen`): {e}",
            path.display()
        )
    })
}

/// The LEAN deployment: every segment tool deferred.
fn core_engine(bundle: &[u8]) -> McpServer {
    McpServer::from_snapshot_segmented(bundle, SegmentSet::core())
        .expect("the core deployment constructs over the generated snapshot")
}

/// The `result` member of a JSON-RPC response frame, parsed.
fn result_of(frame: &str) -> Value {
    let parsed: Value = serde_json::from_str(frame)
        .unwrap_or_else(|e| panic!("the response is a JSON-RPC frame ({e}): {frame}"));
    assert_eq!(parsed["jsonrpc"], "2.0", "JSON-RPC envelope: {frame}");
    assert!(
        parsed.get("error").is_none(),
        "a tool outcome must ride IN the result envelope, never as a protocol error: {frame}"
    );
    parsed["result"].clone()
}

/// The tool envelope's text payload, parsed as the JSON every tool returns.
fn payload_of(frame: &str) -> Value {
    let result = result_of(frame);
    let text = result["content"][0]["text"]
        .as_str()
        .unwrap_or_else(|| panic!("the tool envelope carries text content: {frame}"));
    serde_json::from_str(text).unwrap_or_else(|e| panic!("the tool payload is JSON ({e}): {text}"))
}

#[test]
fn a_segment_tool_against_a_core_deployment_returns_the_typed_routing_signal() {
    let bundle = snapshot();
    let server = core_engine(&bundle);

    let frame = server.handle_message(HEAVY_FRAME);
    let result = result_of(&frame);
    // `isError` stays true: the call produced no answer, and a client that only reads that
    // flag must not be told otherwise. The DISTINCTION lives in the payload.
    assert_eq!(
        result["isError"],
        Value::Bool(true),
        "a deferred call produced no answer, so the envelope must say so: {frame}"
    );

    let payload = payload_of(&frame);
    assert_eq!(
        payload["code"], "mcp.segment-not-loaded",
        "the deferral must carry its own stable code, distinguishable from every other \
         failure — and in particular from `mcp.unknown-tool`, which means the opposite \
         thing: {frame}"
    );
    assert_eq!(
        payload["tool"], "coherence_certificate",
        "the signal names the tool the caller asked for: {frame}"
    );
    assert_eq!(
        payload["segment"], REASONING_SEGMENT,
        "the signal names the segment that SERVES that tool, so the host knows what to \
         load rather than guessing: {frame}"
    );
    let advertised: Vec<&str> = payload["segment_tools"]
        .as_array()
        .unwrap_or_else(|| panic!("the signal carries the segment's tool list: {frame}"))
        .iter()
        .map(|v| v.as_str().expect("segment tool names are strings"))
        .collect();
    assert_eq!(
        advertised, REASONING_SEGMENT_TOOLS,
        "the signal's segment inventory is the engine's own declaration, not a copy: {frame}"
    );
    assert_eq!(
        payload["ok"],
        Value::Bool(false),
        "nothing was computed, so `ok` is false — a deferral must never look like a \
         successful empty result: {frame}"
    );
}

/// The deferral contract over a reasoning tool that COMPUTES rather than looks up, and its
/// lossless replay — claims 1 and 4 again, on the tool the console's live entailment panel
/// runs. `reason_graph` is the tool the retired `gmeow-reason-wasm` engine's `reason` export
/// became, so this is the lane that proves the consolidation kept the capability rather than
/// dropping it into a tier that cannot serve it.
#[test]
fn reason_graph_defers_from_core_and_replays_losslessly_against_the_full_engine() {
    let bundle = snapshot();

    let deferred = core_engine(&bundle).handle_message(REASON_GRAPH_FRAME);
    let payload = payload_of(&deferred);
    assert_eq!(
        payload["code"], "mcp.segment-not-loaded",
        "a computing reasoning tool defers exactly like a looking-up one: {deferred}"
    );
    assert_eq!(
        payload["tool"], "reason_graph",
        "the signal names the tool asked for: {deferred}"
    );
    assert_eq!(
        payload["segment"], REASONING_SEGMENT,
        "reason_graph is served by the reasoning segment: {deferred}"
    );

    let native = McpServer::from_snapshot(&bundle).expect("the native engine constructs");
    let replayed = native.handle_message(REASON_GRAPH_FRAME);
    assert_eq!(
        replayed,
        native.handle_message(REASON_GRAPH_FRAME),
        "the native answer is deterministic, so the byte-claims below are real"
    );
    let result = result_of(&replayed);
    assert_eq!(
        result["isError"],
        Value::Bool(false),
        "the replay produced a real closure, not a second refusal: {replayed}"
    );
    let answer = payload_of(&replayed);
    assert_eq!(answer["ok"], Value::Bool(true), "{replayed}");
    assert!(
        answer["closure_nquads"].is_string(),
        "the tool returns the ENTAILED CLOSURE — the thing verify_graph does not give: \
         {replayed}"
    );
    assert!(
        answer.get("code").is_none(),
        "the replayed answer carries no routing code — the deferral is over: {replayed}"
    );
    assert_ne!(
        replayed, deferred,
        "the two tiers must differ on this frame, or the deferral proves nothing"
    );
}

/// `encode_gmn1` — the GMN codec's write leg — answers in the FIRST-LOAD image, because the
/// codec is core. The console's transcode widget must never pay a segment load for it.
#[test]
fn encode_gmn1_answers_in_the_core_deployment() {
    let bundle = snapshot();
    let frame = core_engine(&bundle).handle_message(ENCODE_GMN1_FRAME);
    let result = result_of(&frame);
    assert_eq!(
        result["isError"],
        Value::Bool(false),
        "the GMN codec is core, so its write leg answers without a segment load: {frame}"
    );
    let payload = payload_of(&frame);
    assert_eq!(payload["ok"], Value::Bool(true), "{frame}");
    assert!(
        payload["gmn1"].is_string(),
        "the tool emits the GMN-1 surface — the direction no other GMN tool serves: {frame}"
    );
    assert_eq!(
        payload["round_trip"],
        Value::Bool(true),
        "the emitted surface must read back to the input model: {frame}"
    );
    assert!(
        payload.get("code").is_none(),
        "a real answer carries no routing code: {frame}"
    );
}

#[test]
fn a_core_tool_against_a_core_deployment_returns_a_real_result() {
    let bundle = snapshot();
    let server = core_engine(&bundle);

    let frame = server.handle_message(CORE_FRAME);
    let result = result_of(&frame);
    assert_eq!(
        result["isError"],
        Value::Bool(false),
        "a core tool answers in the first-load image: {frame}"
    );

    let payload = payload_of(&frame);
    assert_eq!(
        payload["ok"],
        Value::Bool(true),
        "convert reports ok: {frame}"
    );
    assert!(
        payload["output"]
            .as_str()
            .is_some_and(|o| o.contains("http://example.org/s")),
        "the transcode produced the real document, not a stub: {frame}"
    );
    assert!(
        payload.get("code").is_none(),
        "a real answer carries no routing code: {frame}"
    );
}

#[test]
fn a_core_deployment_advertises_the_whole_surface_and_the_whole_theory() {
    let bundle = snapshot();
    let server = core_engine(&bundle);

    // ── discovery ────────────────────────────────────────────────────────────────
    let listed = result_of(&server.handle_message(TOOLS_LIST_FRAME));
    let tools = listed["tools"]
        .as_array()
        .expect("tools/list returns an array");
    let advertised: std::collections::BTreeSet<String> = tools
        .iter()
        .map(|t| {
            t["name"]
                .as_str()
                .expect("every descriptor carries a name")
                .to_owned()
        })
        .collect();
    assert_eq!(
        advertised.len(),
        38,
        "the consumer surface is 38 tools in EVERY deployment tier; a lean image that \
         advertised fewer would be a reduced engine, not a reduced download"
    );
    for tool in REASONING_SEGMENT_TOOLS {
        assert!(
            advertised.contains(*tool),
            "`{tool}` is deferred, and deferral must be invisible to discovery — it is \
             still advertised, described, and dispatchable here"
        );
    }

    // The pre-flight predicate a host uses to decide whether to warm a segment BEFORE
    // dispatching must partition the advertised surface exactly the way dispatch does:
    // deferred iff in the segment. A host that pre-loaded on a wrong answer here would
    // either stall on a tool that runs locally or discover the need mid-frame anyway.
    for tool in &advertised {
        assert_eq!(
            SegmentSet::core().serves(tool),
            !REASONING_SEGMENT_TOOLS.contains(&tool.as_str()),
            "`{tool}`: SegmentSet::core().serves() must agree with the segment inventory"
        );
    }

    // ── the theory that governs dispatch ─────────────────────────────────────────
    // The action policy is a CORE tool, so a lean deployment serves the whole theory. Its
    // `logic:mcpToolName` set must equal what the surface advertises: a tool with no schema
    // is an action the theory does not describe, a schema with no tool is an action the
    // engine cannot perform, and either one would mean the tier changed the theory.
    let policy = payload_of(&server.handle_message(ACTION_POLICY_FRAME));
    let nquads = policy["nquads"]
        .as_str()
        .unwrap_or_else(|| panic!("the action policy projection carries its N-Quads: {policy}"));
    let named: std::collections::BTreeSet<String> = nquads
        .lines()
        .filter_map(|line| {
            let rest = line.split_once(&format!("<{LOGIC_MCP_TOOL_NAME}>"))?.1;
            let start = rest.find('"')? + 1;
            let end = rest[start..].find('"')? + start;
            Some(rest[start..end].to_owned())
        })
        .collect();
    assert_eq!(
        named, advertised,
        "the action theory a lean deployment serves is the SAME total theory: every \
         advertised tool has exactly one schema and every schema exactly one tool"
    );
}

/// Claim 4 needs a deployment that actually serves the segment, which only a build linking
/// it can produce. Gated rather than written to pass vacuously: a test that quietly proves
/// nothing on half the build matrix is worse than one that is honestly absent there.
#[cfg(feature = "reasoning")]
#[test]
fn the_deferred_frame_replayed_against_the_full_engine_is_lossless() {
    let bundle = snapshot();

    // What the core deployment says about this frame — the signal the host routes on.
    let deferred = {
        let core = core_engine(&bundle);
        core.handle_message(HEAVY_FRAME)
    };
    assert_eq!(
        payload_of(&deferred)["code"],
        "mcp.segment-not-loaded",
        "the premise of the replay: the core deployment deferred this frame"
    );

    // The SAME bytes, re-dispatched to the engine the host loads the segment into.
    //
    // That engine is built by `from_snapshot` — the DEFAULT native constructor, the one
    // `gmeow mcp` and `gmeow-mcp-dev` use, which knows nothing about tiers. Using it here
    // (rather than an explicit `SegmentSet::linked()`, which it delegates to anyway) is
    // what makes the comparison a NATIVE one: the re-dispatched answer is not merely
    // "what a full deployment says", it is what the shipped native engine says.
    let replayed = {
        let native = McpServer::from_snapshot(&bundle).expect("the native engine constructs");
        let answer = native.handle_message(HEAVY_FRAME);
        assert_eq!(
            answer,
            native.handle_message(HEAVY_FRAME),
            "the native answer is deterministic, so byte-equality below is a real claim"
        );
        assert!(
            native.segments().serves("coherence_certificate"),
            "the native engine serves the reasoning segment, which is the premise of the \
             replay"
        );
        answer
    };

    let result = result_of(&replayed);
    assert_eq!(
        result["isError"],
        Value::Bool(false),
        "the replay produced a real answer, not a second refusal: {replayed}"
    );
    let payload = payload_of(&replayed);
    assert_eq!(
        payload["ok"],
        Value::Bool(true),
        "the coherence certificate is served for real: {replayed}"
    );
    assert!(
        payload.get("code").is_none(),
        "the replayed answer carries no routing code — the deferral is over: {replayed}"
    );
    assert_ne!(
        replayed, deferred,
        "the two tiers must differ on this frame, or the deferral test above proves nothing"
    );
}
