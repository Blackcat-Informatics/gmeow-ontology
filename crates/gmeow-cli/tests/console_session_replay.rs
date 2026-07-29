// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Replaying an exported console session against the NATIVE `gmeow mcp` engine.
//!
//! A console session is a proof-carrying artifact only if someone else can re-run it. The
//! export carries two things for exactly that: the recorded trajectory, and the store the
//! trajectory ran against — the `store_segment` the engine serialized at export time. This
//! lane drives the real `gmeow` binary's stdio MCP server over a session's own invocation
//! list and compares each answer to the one the session recorded, in both directions:
//!
//! * SEEDED — the native memory package is re-seeded from the session's own exported store
//!   segment (through `purrdf`'s public `Memory::store()`, which is what makes the seeded
//!   package a real, cold-auditable `memory.gts` rather than a transcript), and every
//!   answer must come back byte-identical.
//! * UNSEEDED — the native package is empty, and the replay must HARD FAIL naming the
//!   divergent tool. A store-dependent answer that quietly differed would make a "replay"
//!   that proves nothing, which is the failure this pair exists to make impossible.
//!
//! The session's browser side is the SAME engine: `gmeow_mcp::storage::InMemoryClaimStore`
//! is the backend the wasm console runs on, compiled on every target precisely so the
//! native suite can drive it. Nothing here is mocked and nothing is skipped.

use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use gmeow_mcp::storage::{ClaimStore, InMemoryClaimStore, seed_claim_store};
use purrdf::gts::examples::agent_memory::{StoreOptions, ToolCallOptions};
use serde_json::{Value, json};

/// A fresh, unique, empty scratch directory under the system temp dir.
fn scratch(tag: &str) -> PathBuf {
    let mut dir = std::env::temp_dir();
    dir.push(format!(
        "gmeow-session-replay-{tag}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("the host clock is after the epoch")
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).expect("create scratch dir");
    dir
}

/// One recorded invocation of a console session: the tool, its arguments, and the payload
/// the session recorded as the answer.
struct Recorded {
    tool: &'static str,
    args: Value,
    result: String,
}

/// Drive ONE `gmeow mcp` process over `calls` and return each call's text payload.
///
/// The real binary, the real stdio JSON-RPC loop, one frame per line — the same transport
/// an agent talks to. The three `GMEOW_*_PATH` variables are pinned into the scratch
/// directory so a replay can never read or write the developer's own `~/.gmeow`.
fn replay_against_gmeow_mcp(memory: &Path, calls: &[Recorded]) -> Vec<String> {
    let home = memory.parent().expect("the memory package has a directory");
    let mut child = Command::new(assert_cmd::cargo::cargo_bin("gmeow"))
        .arg("mcp")
        .env("GMEOW_MEMORY_PATH", memory)
        .env("GMEOW_CONJECTURE_PATH", home.join("conjectures.gts"))
        .env("GMEOW_CANDIDATE_PATH", home.join("candidates.gts"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("gmeow mcp starts");

    {
        let stdin = child.stdin.as_mut().expect("stdin is piped");
        for (index, call) in calls.iter().enumerate() {
            let frame = json!({
                "jsonrpc": "2.0",
                "id": index + 1,
                "method": "tools/call",
                "params": {"name": call.tool, "arguments": call.args},
            });
            writeln!(stdin, "{frame}").expect("write one frame");
        }
    }
    let output = child.wait_with_output().expect("gmeow mcp exits");
    assert!(
        output.status.success(),
        "gmeow mcp failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8(output.stdout).expect("responses are UTF-8");
    let answers: Vec<String> = stdout
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            let frame: Value = serde_json::from_str(line).expect("each response is one JSON frame");
            assert!(
                frame.get("error").is_none(),
                "the replay must not raise a protocol error: {frame}"
            );
            frame["result"]["content"][0]["text"]
                .as_str()
                .unwrap_or_else(|| panic!("a tools/call response carries a text payload: {frame}"))
                .to_owned()
        })
        .collect();
    assert_eq!(
        answers.len(),
        calls.len(),
        "every recorded invocation must get exactly one answer"
    );
    answers
}

/// The tools whose replayed answer differs from the one the session recorded.
///
/// The whole point of the replay is to NAME them: a divergence report that said only "the
/// replay differed" would leave a reader with nothing to act on, and one that reported
/// every tool would be useless for locating the cause.
fn divergent_tools(calls: &[Recorded], answers: &[String]) -> Vec<&'static str> {
    calls
        .iter()
        .zip(answers)
        .filter(|(call, answer)| call.result != **answer)
        .map(|(call, _)| call.tool)
        .collect()
}

/// The browser side of the session: a claim written into the in-process store the wasm
/// console runs on, plus the tool call that wrote it, exactly as the engine records one.
fn browser_session_store() -> InMemoryClaimStore {
    let store = InMemoryClaimStore::default();
    let claim = store
        .store_claim(
            "the launch window closes on the 14th",
            StoreOptions {
                source: Some("console:test"),
                confidence: Some(0.8),
                according_to: Some("urn:gmeow:party:flight-ops"),
            },
        )
        .expect("the browser store accepts a well-formed claim");
    store
        .store_claim(
            "the backup window opens on the 20th",
            StoreOptions {
                source: None,
                confidence: None,
                according_to: None,
            },
        )
        .expect("stores");
    store
        .record_tool_call(
            "urn:gmeow:tool:store_claim",
            ToolCallOptions {
                arguments: Some(r#"{"text":"the launch window closes on the 14th"}"#),
                result: Some(r#"{"ok":true}"#),
                invocation: None,
                generated: &[claim.id.as_str()],
            },
        )
        .expect("the browser store records the call that wrote the claim");
    store
}

/// The session's invocation list: one bundle read (store-independent) and the store read
/// (store-determined), each with the answer the browser session recorded.
///
/// `gmn_glyph_legend` is in the list on purpose. It answers off the bundle alone, so
/// seeding a store cannot perturb it — which is what makes the negative case below a real
/// assertion: the divergence report must name the store read and NOT this one. Its
/// recorded answer is taken from the seeded pass, so what it pins is that a store seeding
/// leaves a bundle read untouched.
fn session_calls(store_segment: String, glyph_legend: String) -> Vec<Recorded> {
    vec![
        Recorded {
            tool: "gmn_glyph_legend",
            args: json!({}),
            result: glyph_legend,
        },
        Recorded {
            tool: "store_segment",
            args: json!({}),
            result: store_segment,
        },
    ]
}

/// SEEDED — a native package re-seeded from the session's own exported store segment
/// answers `store_segment` BYTE-IDENTICALLY to the browser session it came from.
///
/// That byte identity is the replay contract. It holds because the transport segment
/// carries what the store's PUBLIC write API can accept back — the claim's text,
/// confidence, attribution and source, and the recorded call's tool, payloads and
/// generated entities — and deliberately not the two fields the two backends mint
/// differently by construction (the record id and the creation stamp). Seeding really does
/// go through that public API: natively `store_claim` IS `purrdf`'s `Memory::store()`, so
/// the package under test is a genuine `memory.gts` written by its owner.
#[test]
fn a_seeded_native_replay_answers_byte_identically_to_the_browser_session() {
    let browser = browser_session_store();
    let exported = browser
        .segment_nquads()
        .expect("the browser session exports its store segment");
    assert!(
        !exported.trim().is_empty(),
        "the session under replay must actually hold store state, or the lane proves nothing"
    );

    let dir = scratch("seeded");
    let memory = dir.join("memory.gts");
    let native = gmeow_mcp::storage::fs_claim_store(&memory).expect("a native claim package");
    let (claims, calls) = seed_claim_store(native.as_ref(), &exported)
        .expect("the exported segment re-seeds a native package");
    assert_eq!(
        (claims, calls),
        (2, 1),
        "every record in the exported segment is replayed into the package"
    );
    assert!(
        memory.exists(),
        "seeding writes a real memory.gts through purrdf's own writer"
    );

    // The seeded package re-serializes to the SAME segment the browser exported. This is
    // the byte-identity claim, made against the native store rather than a copy of it.
    assert_eq!(
        native
            .segment_nquads()
            .expect("the native package serializes"),
        exported,
        "a package seeded from a session's store segment must re-export those exact bytes"
    );

    // …and the shipped `gmeow mcp` binary serves that same answer over the wire.
    let probe = replay_against_gmeow_mcp(
        &memory,
        &[Recorded {
            tool: "store_segment",
            args: json!({}),
            result: String::new(),
        }],
    );
    let served: Value = serde_json::from_str(&probe[0]).expect("store_segment answers JSON");
    assert_eq!(
        served["nquads"].as_str(),
        Some(exported.as_str()),
        "`gmeow mcp` must serve the seeded store byte-identically to the browser session"
    );
    assert_eq!(served["claim_count"], 2, "{served}");
    assert_eq!(served["tool_call_count"], 1, "{served}");

    // The whole recorded invocation list replays with ZERO divergence.
    let legend = replay_against_gmeow_mcp(
        &memory,
        &[Recorded {
            tool: "gmn_glyph_legend",
            args: json!({}),
            result: String::new(),
        }],
    );
    let calls = session_calls(probe[0].clone(), legend[0].clone());
    let answers = replay_against_gmeow_mcp(&memory, &calls);
    assert_eq!(
        divergent_tools(&calls, &answers),
        Vec::<&str>::new(),
        "a seeded replay of the session must reproduce every recorded answer"
    );

    std::fs::remove_dir_all(&dir).ok();
}

/// UNSEEDED — the same session replayed against an EMPTY native package hard-fails, and
/// the failure NAMES the divergent tool.
///
/// The negative half of the pair, and the one that keeps the positive half honest: without
/// it, a replay harness that compared nothing would pass the seeded case too. The report
/// must be selective — the bundle read cannot diverge on a store seeding, so naming it
/// would mean the comparison is noise rather than a diagnosis.
#[test]
fn an_unseeded_native_replay_diverges_and_names_the_tool() {
    let browser = browser_session_store();
    let exported = browser.segment_nquads().expect("exports");

    let dir = scratch("unseeded");
    let memory = dir.join("memory.gts");

    // Record the bundle read against THIS empty package, so the only difference between
    // the two rows below is whether the store was seeded.
    let legend = replay_against_gmeow_mcp(
        &memory,
        &[Recorded {
            tool: "gmn_glyph_legend",
            args: json!({}),
            result: String::new(),
        }],
    );
    let recorded_store = serde_json::to_string(&json!({
        "ok": true,
        "claim_count": 2,
        "tool_call_count": 1,
        "nquads": exported,
    }))
    .expect("the recorded browser answer serializes");

    let calls = session_calls(recorded_store, legend[0].clone());
    let answers = replay_against_gmeow_mcp(&memory, &calls);

    assert_eq!(
        divergent_tools(&calls, &answers),
        vec!["store_segment"],
        "an unseeded replay must diverge, and the report must name the store read alone"
    );
    let served: Value = serde_json::from_str(&answers[1]).expect("store_segment answers JSON");
    assert_eq!(
        served["claim_count"], 0,
        "the unseeded package genuinely holds nothing: {served}"
    );
    assert_eq!(served["nquads"], "", "{served}");

    std::fs::remove_dir_all(&dir).ok();
}
