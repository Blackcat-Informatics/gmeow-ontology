// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The consumer `gmeow medium` verbs, driven against a bundle the REAL DAG emitted.
//!
//! # Why the bundle is emitted here rather than read off disk
//!
//! `crates/gmeow-cli/build.rs` embeds `generated/dist/gmeow.gts`, which is a git-ignored
//! LOCAL product: in any checkout that has not re-run `make regen` since the medium axis
//! landed, the embedded bundle predates the axis entirely and carries no
//! `graph/medium-registry` at all. A test that read the embedded bytes would therefore
//! either fail for a reason that is not about the code under test, or — much worse —
//! quietly assert something weaker.
//!
//! So this suite runs the production DAG once, in memory, over a temp cache, writes the
//! bundle the terminal sink emits to a temp path, and passes that path as the verb's
//! `FILE` argument. That is the SAME emitter, the SAME terminal, and the real CLI binary
//! reading a real file — not a hand-built fixture, and not a skip-if-absent branch.
//!
//! # One DAG run, one test
//!
//! A whole-pipeline execution is minutes, so every clause lives in one test function
//! rather than multiplying that cost by the number of things being asserted.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use gmeow_pipeline::node::StageProduct;
use gmeow_pipeline::{PipelineCache, RunContext, bind, default_registry, full_spec, run};

#[path = "../../pipeline/tests/support/medium_tamper.rs"]
mod tamper;

/// The five dictionaries `slices/core/gts/module.ttl` declares, spelled out rather than
/// read back off the artifact under test: a dictionary silently dropped from the
/// declaration must be a FAILURE here, not a smaller expectation.
const SHIPPED_DICTIONARIES: [&str; 5] = [
    "gmeow-core-v1",
    "gmeow-logic-v1",
    "gmeow-memory-compact-v1",
    "gmeow-memory-hot-v1",
    "gmeow-prooftrace-v1",
];

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .canonicalize()
        .expect("workspace root")
}

/// Run the REAL production DAG (`full_spec`, the same spec `make regen` executes) once,
/// in memory, over a temp cache.
fn run_the_dag(root: &Path) -> BTreeMap<String, StageProduct> {
    let spec = full_spec();
    let graph = spec.validate().expect("the production DAG validates");
    let bound = bind(&spec, &graph, &default_registry()).expect("every production stage binds");
    let cache_dir = tempfile::tempdir().expect("tempdir");
    let jobs = std::thread::available_parallelism()
        .map(std::num::NonZeroUsize::get)
        .unwrap_or(4);
    let mut ctx = RunContext::open(root, jobs).expect("run context");
    ctx.cache = PipelineCache::open(cache_dir.path()).expect("temp cache");
    run(&graph, &bound, &mut ctx)
        .expect("the production DAG runs end to end")
        .products
}

/// The `gmeow` consumer binary under test.
fn gmeow() -> Command {
    Command::new(assert_cmd::cargo::cargo_bin("gmeow"))
}

/// One CLI invocation's `(exit code, stdout, stderr)`.
fn invoke(args: &[&str]) -> (i32, String, String) {
    let output = gmeow()
        .args(args)
        .output()
        .unwrap_or_else(|err| panic!("running `gmeow {}`: {err}", args.join(" ")));
    (
        output.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

/// The integer following `label` in a `gmeow medium` report header line.
fn counted(stdout: &str, label: &str) -> usize {
    stdout
        .lines()
        .find_map(|line| {
            let trimmed = line.trim_start();
            let rest = trimmed.strip_prefix(label)?;
            rest.trim().parse::<usize>().ok()
        })
        .unwrap_or_else(|| panic!("no {label:?} count in:\n{stdout}"))
}

/// Assert that `gmeow medium verify` refuses `bundle` under exactly `code`.
///
/// The CODE is what is asserted, never merely "it failed". The six medium classes are the
/// vocabulary a caller dispatches on, so a fixture that exited non-zero for the wrong
/// reason would look identical to one the gate caught correctly.
fn assert_breach(dir: &Path, name: &str, bytes: &[u8], code: &str) {
    let path = dir.join(name);
    std::fs::write(&path, bytes).expect("write the breach fixture");
    let (status, stdout, stderr) = invoke(&["medium", "verify", path.to_str().expect("utf-8")]);
    assert_ne!(
        status, 0,
        "the {name} breach fixture must exit non-zero\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(
        stderr.contains(code),
        "the {name} breach fixture must be reported under {code}, got:\n{stderr}"
    );
    // No fixture may be reported as a successful dictionary-less fallback: there is none.
    assert!(
        !stdout.contains("verified:"),
        "the {name} breach fixture must not report a successful verification:\n{stdout}"
    );
}

#[test]
fn the_medium_verbs_read_verify_and_explain_a_freshly_emitted_bundle() {
    let root = repo_root();
    let products = run_the_dag(&root);
    let bundle = products
        .get("stage-gts-sink")
        .expect("the terminal sink produced a product")
        .artifact(gmeow_pipeline::stages::gts_sink::GTS_PATH)
        .expect("the sink product carries the gmeow.gts artifact")
        .to_vec();
    assert!(
        bundle.len() > 1024,
        "the emitted bundle is implausibly small: {} bytes",
        bundle.len()
    );

    let home = tempfile::tempdir().expect("tempdir");
    let bundle_path = home.path().join("gmeow.gts");
    std::fs::write(&bundle_path, &bundle).expect("stage the emitted bundle");
    let bundle_arg = bundle_path.to_str().expect("utf-8");

    // ── (a) `gmeow medium list` ──────────────────────────────────────────────
    let (status, stdout, stderr) = invoke(&["medium", "list", bundle_arg]);
    assert_eq!(status, 0, "medium list failed:\n{stderr}");
    let envelopes = counted(&stdout, "envelopes");
    let frames = counted(&stdout, "payload frames");
    assert!(
        envelopes > 0,
        "the bundle must carry at least one gmeow:MediumEnvelope:\n{stdout}"
    );
    assert_eq!(
        envelopes, frames,
        "one envelope per payload-bearing frame is the whole claim:\n{stdout}"
    );
    for id in SHIPPED_DICTIONARIES {
        assert!(
            stdout.contains(id),
            "medium list must report the shipped dictionary {id}:\n{stdout}"
        );
    }
    // …and it must report the coordinates a consumer actually needs to prime with,
    // not merely the ids: the content digest, the zstd Dictionary_ID, the assignment.
    assert!(
        stdout.contains("blake3:") && stdout.contains("Dictionary_ID"),
        "medium list must report each dictionary's content digest and zstd \
         Dictionary_ID:\n{stdout}"
    );
    assert!(
        stdout.contains("gmeow:snapshot/wire"),
        "medium list must report the per-rep assignment, snapshot slot included:\n{stdout}"
    );

    // ── (d) `gmeow medium explain` ───────────────────────────────────────────
    let (status, stdout, stderr) = invoke(&["medium", "explain", "gmeow-core-v1", bundle_arg]);
    assert_eq!(status, 0, "medium explain failed:\n{stderr}");
    assert!(
        stdout.contains("measured MDL contribution"),
        "medium explain must report the measured MDL contribution:\n{stdout}"
    );
    for required in [
        "two-part code",
        "in-band",
        "baseline",
        "bounded gain fraction",
        "pays for itself",
        "gmeow:corpusSelects",
    ] {
        assert!(
            stdout.contains(required),
            "medium explain must report {required:?}:\n{stdout}"
        );
    }
    // The numbers are the SHIPPED ones, not a placeholder: the two-part code is strictly
    // below the baseline, which is the criterion the emission itself enforced.
    assert!(
        stdout.contains("pays for itself: true"),
        "the shipped gmeow-core-v1 must be reported as paying for itself:\n{stdout}"
    );

    // An unknown dictionary id is a NAMED failure, not an empty report.
    let (status, _, stderr) = invoke(&["medium", "explain", "gmeow-not-a-dictionary", bundle_arg]);
    assert_ne!(status, 0, "an unknown dictionary id must fail");
    assert!(
        stderr.contains("pipeline.medium.unknown-dictionary"),
        "an unknown dictionary id must be reported under its named class:\n{stderr}"
    );

    // ── (b) `gmeow medium verify` over a REAL runtime store ──────────────────
    let store = write_a_runtime_store(home.path(), &bundle);
    let (status, stdout, stderr) = invoke(&[
        "medium",
        "verify",
        store.to_str().expect("utf-8"),
        "--registry",
        bundle_arg,
    ]);
    assert_eq!(status, 0, "verifying the runtime store failed:\n{stderr}");
    assert!(
        stdout.contains("HeaderDict"),
        "a runtime store is audited under the header-dict branch:\n{stdout}"
    );
    assert!(
        stdout.contains("gmeow-memory-hot-v1"),
        "the store's segments must be reported as primed under \
         gmeow-memory-hot-v1:\n{stdout}"
    );
    assert!(
        stdout.contains("gmeow/mediumProfileStoreL12"),
        "the store must be audited against the medium its producer declares:\n{stdout}"
    );
    // Every frame of the store is primed — a store that pinned the dictionary but left
    // its records unprimed would satisfy "the file carries the dictionary" while
    // discarding every byte of the density the dictionary exists to provide.
    let primed = stdout
        .lines()
        .filter(|line| line.trim_start().starts_with("frame @"))
        .count();
    assert!(primed > 0, "the store carries no payload frames:\n{stdout}");
    assert_eq!(
        stdout
            .lines()
            .filter(|line| line.trim_start().starts_with("frame @"))
            .filter(|line| line.contains("gmeow-memory-hot-v1"))
            .count(),
        primed,
        "every frame of the store must be dict-primed:\n{stdout}"
    );

    // ── the healthy bundle verifies, and the harness itself is inert ─────────
    let (status, stdout, stderr) = invoke(&["medium", "verify", bundle_arg]);
    assert_eq!(status, 0, "verifying the emitted bundle failed:\n{stderr}");
    assert!(stdout.contains("SelfDescribing"), "{stdout}");
    assert!(stdout.contains("verified:"), "{stdout}");

    tamper::assert_wire_is_intact(&bundle);
    let rewritten = tamper::identity_rewrite(&bundle);
    let rewritten_path = home.path().join("identity-rewrite.gts");
    std::fs::write(&rewritten_path, &rewritten).expect("write the identity rewrite");
    let (status, _, stderr) =
        invoke(&["medium", "verify", rewritten_path.to_str().expect("utf-8")]);
    assert_eq!(
        status, 0,
        "a re-serialize + re-stamp with NO edit must still verify, or every breach \
         fixture below would be testing the harness rather than the clause it \
         names:\n{stderr}"
    );

    // ── (c) the five breach fixtures, each under its own named class ─────────
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

/// Write a runtime store through the PRODUCTION `Memory::store` path, primed from the
/// freshly emitted bundle.
///
/// Driven through `McpServer`'s `store_claim` tool rather than by calling purrdf's
/// `Memory` directly: the medium wiring lives on the production store path, and a test
/// that opened its own writer would prove nothing about the path a consumer takes.
fn write_a_runtime_store(home: &Path, bundle: &[u8]) -> PathBuf {
    use gmeow_pipeline::mcp::{McpMode, McpServer};

    let memory_path = home.join("memory.gts");
    // SAFETY: this test binary runs one test, single-threaded, and restores nothing
    // because the process exits with it.
    unsafe {
        std::env::set_var("GMEOW_MEMORY_PATH", &memory_path);
        std::env::set_var("GMEOW_CONJECTURE_PATH", home.join("conjectures.gts"));
        std::env::remove_var("GMEOW_LANG");
    }
    let server = McpServer::from_snapshot(bundle, None, McpMode::Consumer)
        .expect("the freshly emitted bundle serves an MCP session");
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
