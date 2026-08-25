// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! `gmeow-dev medium-gate`, driven against a bundle the REAL DAG emitted and against a
//! REAL runtime store written through the production `Memory::store` path.
//!
//! The subject is emitted here rather than read off `generated/dist/gmeow.gts` for the
//! same reason the consumer suite does it: that file is a git-ignored local product, so a
//! checkout that has not re-materialized it since the medium axis landed would make this
//! suite assert something weaker (or fail for a reason unrelated to the gate). Emitting it
//! in memory keeps the subject the SAME emitter's output while making the suite
//! independent of what happens to be on disk. Bounded intermediary products reuse the
//! exact persistent receipts primed before test fanout; aggregate stages still execute.
//!
//! The runtime-store leg is the one this gate exists for. A `~/.gmeow/*.gts` agent memory
//! is not a build artifact, so no `generated/` gate ever reaches it — and it is written
//! through a declared medium, primed with a dictionary the shipped bundle owns, and is
//! exactly as capable of silently losing that priming as anything the build emits.
//!
//! A whole-pipeline execution is minutes, so every clause lives in one test function.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use gmeow_pipeline::node::StageProduct;
use gmeow_pipeline::{CarrierRetention, RunContext, bind, default_registry, full_spec, run};

#[path = "../../pipeline/tests/support/medium_tamper.rs"]
mod tamper;

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .canonicalize()
        .expect("workspace root")
}

/// Run the REAL production DAG once, reusing only verified bounded contributions.
fn run_the_dag(root: &Path) -> BTreeMap<String, StageProduct> {
    let spec = full_spec();
    let graph = spec.validate().expect("the production DAG validates");
    let bound = bind(&spec, &graph, &default_registry()).expect("every production stage binds");
    let jobs = std::thread::available_parallelism()
        .map(std::num::NonZeroUsize::get)
        .unwrap_or(4);
    let mut ctx = RunContext::open(root, jobs).expect("run context");
    ctx.carrier_retention = CarrierRetention::DropAfterLastConsumer;
    run(&graph, &bound, &mut ctx)
        .expect("the production DAG runs end to end")
        .products
}

/// One `gmeow-dev medium-gate` invocation's `(exit code, stdout, stderr)`.
fn medium_gate(args: &[&str]) -> (i32, String, String) {
    let output = Command::new(assert_cmd::cargo::cargo_bin("gmeow-dev"))
        .arg("medium-gate")
        .args(args)
        .output()
        .unwrap_or_else(|err| panic!("running `gmeow-dev medium-gate {}`: {err}", args.join(" ")));
    (
        output.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

/// Assert the gate refuses `bytes` under exactly `code`.
fn assert_breach(dir: &Path, name: &str, bytes: &[u8], code: &str) {
    let path = dir.join(name);
    std::fs::write(&path, bytes).expect("write the breach fixture");
    let (status, stdout, stderr) = medium_gate(&[path.to_str().expect("utf-8")]);
    assert_ne!(
        status, 0,
        "the {name} breach fixture must exit non-zero\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(
        stderr.contains(code),
        "the {name} breach fixture must be reported under {code}, got:\n{stderr}"
    );
    assert!(
        !stdout.contains("medium gate passed"),
        "the {name} breach fixture must not report a pass:\n{stdout}"
    );
}

#[test]
fn the_medium_gate_passes_the_emitted_bundle_and_a_real_runtime_store_and_reds_every_breach() {
    let root = repo_root();
    let products = run_the_dag(&root);
    let bundle = products
        .get("stage-gts-sink")
        .expect("the terminal sink produced a product")
        .artifact(gmeow_pipeline::stages::gts_sink::GTS_PATH)
        .expect("the sink product carries the gmeow.gts artifact")
        .to_vec();

    let home = tempfile::tempdir().expect("tempdir");
    let bundle_path = home.path().join("gmeow.gts");
    std::fs::write(&bundle_path, &bundle).expect("stage the emitted bundle");
    let bundle_arg = bundle_path.to_str().expect("utf-8").to_string();

    // ── the freshly emitted bundle passes every clause ───────────────────────
    let (status, stdout, stderr) = medium_gate(&[&bundle_arg]);
    assert_eq!(status, 0, "the emitted bundle must pass:\n{stderr}");
    assert!(stdout.contains("medium gate passed"), "{stdout}");
    assert!(
        stdout.contains("SelfDescribing"),
        "the dist bundle is audited per-rep against the registry it carries:\n{stdout}"
    );
    assert!(
        stdout.contains("gmeow/mediumProfileDistL12"),
        "the bundle must be audited against the medium its producer declares:\n{stdout}"
    );
    // The reader contract the gate compared against the wire, published in the pass line
    // (Principle 13: it is a property of the deliverable, not a private detail).
    for capability in ["zstd-dictionary", "zstd-rsyncable"] {
        assert!(
            stdout.contains(capability),
            "the pass line must publish the declared reader capability {capability}:\n{stdout}"
        );
    }
    for id in ["gmeow-core-v1", "gmeow-logic-v1", "gmeow-prooftrace-v1"] {
        assert!(
            stdout.contains(id),
            "the pass line must name the dictionaries that actually primed a frame, \
             including {id}:\n{stdout}"
        );
    }

    // ── a REAL runtime store, written through the production Memory::store path ──
    let store = write_a_runtime_store(home.path(), &bundle);
    let (status, stdout, stderr) =
        medium_gate(&[store.to_str().expect("utf-8"), "--registry", &bundle_arg]);
    assert_eq!(
        status, 0,
        "a runtime store primed from this bundle must pass:\n{stderr}"
    );
    assert!(stdout.contains("medium gate passed"), "{stdout}");
    assert!(
        stdout.contains("HeaderDict"),
        "a runtime store is audited under the header-dict branch:\n{stdout}"
    );
    assert!(
        stdout.contains("gmeow-memory-hot-v1"),
        "the store's frames must be reported primed under gmeow-memory-hot-v1:\n{stdout}"
    );
    assert!(
        stdout.contains("gmeow/mediumProfileStoreL12"),
        "the store must be audited against the medium ITS producer declares, not the \
         bundle's:\n{stdout}"
    );

    // A store audited against a bundle that carries NO medium registry — here, itself —
    // has nothing to resolve its dictionary id against, and that is a hard failure rather
    // than a pass with the resolution quietly skipped.
    let store_arg = store.to_str().expect("utf-8").to_string();
    let (status, stdout, stderr) = medium_gate(&[&store_arg, "--registry", &store_arg]);
    assert_ne!(
        status, 0,
        "a header-dict store audited against a registry-less bundle must fail rather than \
         skip the resolution:\n{stdout}"
    );
    assert!(stderr.contains("medium gate failed"), "{stderr}");

    // ── the harness is inert, so the fixtures below test the CLAUSES ─────────
    tamper::assert_wire_is_intact(&bundle);
    let rewritten_path = home.path().join("identity-rewrite.gts");
    std::fs::write(&rewritten_path, tamper::identity_rewrite(&bundle))
        .expect("write the identity rewrite");
    let (status, _, stderr) = medium_gate(&[rewritten_path.to_str().expect("utf-8")]);
    assert_eq!(
        status, 0,
        "a re-serialize + re-stamp with NO edit must still pass the gate:\n{stderr}"
    );

    // ── every breach fixture reds, under its own named class ─────────────────
    assert_breach(
        home.path(),
        "flipped-payload-byte.gts",
        &tamper::flipped_payload_byte(&bundle),
        "pipeline.medium.digest-mismatch",
    );
    assert_breach(
        home.path(),
        "unknown-dictionary.gts",
        &tamper::unknown_dictionary_id(&bundle),
        "pipeline.medium.unknown-dictionary",
    );
    assert_breach(
        home.path(),
        "undeclared-dictionary.gts",
        &tamper::undeclared_dictionary(&bundle),
        "pipeline.medium.undeclared-dictionary",
    );
    assert_breach(
        home.path(),
        "unknown-schema.gts",
        &tamper::unregistered_rep(&bundle),
        "pipeline.medium.unknown-schema",
    );
    assert_breach(
        home.path(),
        "opaque-frame.gts",
        &tamper::undecodable_payload(&bundle),
        "pipeline.medium.opaque-frame",
    );
}

/// The ONE new MCP resource: the medium registry, served off the loaded bundle alone.
///
/// Exactly one, and only a medium one — `model_facing_invariance`'s leg 4 enumerates the
/// delta this change is allowed against the merge base, and a second added resource reds
/// it. What is asserted here is the other half of that claim: that the one resource is
/// real, is served from the bundle's OWN graphs rather than from a repository, and
/// carries the coordinates a consumer needs to prime a decode.
fn assert_medium_resource(server: &gmeow_mcp::McpServer) {
    const URI: &str = "gmeow://ontology/medium";
    let list = server.resources_result();
    let resources = list["resources"].as_array().expect("resources array");
    let advertised: Vec<&str> = resources
        .iter()
        .filter_map(|entry| entry["uri"].as_str())
        .collect();
    assert!(
        advertised.contains(&URI),
        "the medium resource must be advertised in resources/list: {advertised:?}"
    );
    assert_eq!(
        advertised
            .iter()
            .filter(|uri| uri.contains("medium"))
            .count(),
        1,
        "exactly ONE medium resource is the enumerated delta: {advertised:?}"
    );

    let read = server.read_resource_result(URI);
    assert!(
        read.get("isError").is_none(),
        "the medium resource must read cleanly: {read}"
    );
    let text = read["contents"][0]["text"]
        .as_str()
        .expect("the medium resource carries text");
    let payload: serde_json::Value =
        serde_json::from_str(text).expect("the medium resource is a JSON envelope");
    assert_eq!(
        payload["graph"], "https://blackcatinformatics.ca/gmeow/graph/medium-registry",
        "the envelope must name the graph it projects: {payload}"
    );
    let ids: Vec<&str> = payload["dictionaries"]
        .as_array()
        .expect("dictionaries array")
        .iter()
        .filter_map(|row| row["id"].as_str())
        .collect();
    for id in [
        "gmeow-core-v1",
        "gmeow-logic-v1",
        "gmeow-memory-compact-v1",
        "gmeow-memory-hot-v1",
        "gmeow-prooftrace-v1",
    ] {
        assert!(
            ids.contains(&id),
            "the medium resource must report the shipped dictionary {id}: {ids:?}"
        );
    }
    let envelopes = payload["envelopes"].as_u64().expect("envelope count");
    assert_eq!(
        envelopes,
        payload["payloadFrames"].as_u64().expect("frame count"),
        "one envelope per payload-bearing frame: {payload}"
    );
    assert!(envelopes > 0, "the bundle carries no envelopes: {payload}");
    let first = &payload["dictionaries"][0];
    assert!(
        first["contentDigest"]
            .as_str()
            .is_some_and(|digest| digest.starts_with("blake3:")),
        "every dictionary row must carry its canonical content digest: {payload}"
    );
    assert!(
        first["zstdDictionaryId"].as_u64().is_some_and(|id| id != 0),
        "every dictionary row must carry the zstd Dictionary_ID a primed frame \
         cites: {payload}"
    );
    assert!(
        first["inBandBytes"].as_u64().is_some(),
        "every dictionary row must report the byte length the bundle pins in band — the \
         header is the ONLY channel a consumer can obtain it from: {payload}"
    );
    assert!(
        !payload["assignment"]
            .as_array()
            .expect("assignment array")
            .is_empty(),
        "the medium resource must serve the total rep to medium assignment: {payload}"
    );
}

/// Write a runtime store through the PRODUCTION `Memory::store` path, primed from the
/// freshly emitted bundle.
fn write_a_runtime_store(home: &Path, bundle: &[u8]) -> PathBuf {
    use gmeow_mcp::McpServer;

    let memory_path = home.join("memory.gts");
    // SAFETY: this test binary runs one test, single-threaded, and restores nothing
    // because the process exits with it.
    unsafe {
        std::env::set_var("GMEOW_MEMORY_PATH", &memory_path);
        std::env::set_var("GMEOW_CONJECTURE_PATH", home.join("conjectures.gts"));
        std::env::remove_var("GMEOW_LANG");
    }
    // The medium registry rides an Extension: the MCP engine is a leaf that does not link the
    // build executor, so the host that owns the reader registers the surface. Asserting against
    // the bare leaf would assert a capability no leaf can have.
    let server = McpServer::from_snapshot_with(bundle, gmeow_mcp_dev::medium_extension())
        .expect("the freshly emitted bundle serves an MCP session");
    assert_medium_resource(&server);
    for text in [
        "a claim stored through the production memory path",
        "a second claim, so the store carries more than one record",
    ] {
        let stored = server
            .call_tool_result("store_claim", &serde_json::json!({ "text": text }))
            .to_string();
        assert!(
            stored.contains("\\\"ok\\\":true") || stored.contains("\"ok\":true"),
            "store_claim must commit: {stored}"
        );
    }
    memory_path
}
