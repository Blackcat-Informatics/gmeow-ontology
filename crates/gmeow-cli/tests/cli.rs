// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Parity acceptance tests for the consumer `gmeow` binary.
//!
//! These mirror the behaviors pinned by the Python `tests/test_cli.py` and
//! `tests/test_validate_rdf.py` against the same embedded/bundled snapshot an
//! installed wheel uses. Each test drives the built binary through `assert_cmd`,
//! so the split (product → stdout, diagnostics → stderr) and the `0`/`1`/`2` exit
//! convention are exercised end to end.
//!
//! (The `gmeow-dev logic … / reason …` surface belongs to the repo-maintenance
//! `gmeow-dev` bin and is covered by that crate's parity tests, so it is not
//! mirrored here.)

use std::path::{Path, PathBuf};
use std::process::Command as StdCommand;

use assert_cmd::Command;
use predicates::prelude::*;

/// The repo-root path of a committed validate fixture.
fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/validate")
        .join(name)
}

/// A fresh, unique, empty scratch directory under the system temp dir.
fn scratch(tag: &str) -> PathBuf {
    let mut dir = std::env::temp_dir();
    dir.push(format!(
        "gmeow-cli-test-{tag}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).expect("create scratch dir");
    dir
}

/// The built `gmeow` binary.
fn gmeow() -> Command {
    Command::cargo_bin("gmeow").expect("gmeow binary builds")
}

// ── version / describe (test_cli.py) ─────────────────────────────────────────

#[test]
fn version_prints_the_package_version() {
    gmeow()
        .arg("version")
        .assert()
        .success()
        .stdout(predicate::str::contains(env!("CARGO_PKG_VERSION")));
}

#[test]
fn describe_known_term_renders_prose() {
    // Mirrors the describe prose expectations: a known kernel term resolves to a
    // rendered card on stdout with exit 0.
    gmeow()
        .args(["describe", "Entity"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Entity"));
}

#[test]
fn describe_unknown_language_fails_with_available_list() {
    // test_describe_unknown_language_fails: a bad --lang exits non-zero and lists
    // the available languages.
    gmeow()
        .args(["describe", "Person", "--lang", "notatag"])
        .assert()
        .failure()
        .stderr(
            predicate::str::contains("unknown language tag")
                .and(predicate::str::contains("Available languages")),
        );
}

#[test]
fn describe_env_language_rejected_if_unknown() {
    // test_describe_env_language_rejected_if_unknown: GMEOW_LANG feeds the same
    // resolution and a bad value hard-fails.
    gmeow()
        .args(["describe", "Person"])
        .env("GMEOW_LANG", "notatag")
        .assert()
        .failure()
        .stderr(predicate::str::contains("unknown language tag"));
}

// ── validate (test_validate_rdf.py) ──────────────────────────────────────────

#[test]
fn validate_clean_file_passes() {
    // test_validate_rdf_clean_file_passes.
    gmeow()
        .arg("validate")
        .arg(fixture("clean.ttl"))
        .assert()
        .success()
        .stdout(predicate::str::contains("validation passed"));
}

#[test]
fn validate_fail_file_exits_one_human() {
    // test_validate_rdf_human_format_exits_nonzero: names the offending constraints
    // on stderr.
    gmeow()
        .arg("validate")
        .arg(fixture("fail.nq"))
        .assert()
        .code(1)
        .stderr(
            predicate::str::contains("identity axis")
                .or(predicate::str::contains("reference frame")),
        );
}

#[test]
fn validate_fail_file_json_is_well_formed() {
    // test_validate_rdf_reports_two_errors_one_warning_with_locations: JSON on
    // stdout carries the findings; exit 1.
    let output = gmeow()
        .arg("validate")
        .arg(fixture("fail.nq"))
        .args(["--format", "json"])
        .assert()
        .code(1)
        .get_output()
        .stdout
        .clone();
    let text = String::from_utf8(output).expect("utf-8 json");
    let parsed: serde_json::Value = serde_json::from_str(&text).expect("valid JSON report");
    let findings = parsed["findings"].as_array().expect("findings array");
    assert_eq!(findings.len(), 3, "two errors + one warning: {findings:?}");
    let errors = findings.iter().filter(|f| f["severity"] == "error").count();
    let warnings = findings
        .iter()
        .filter(|f| f["severity"] == "warning")
        .count();
    assert_eq!(errors, 2, "{findings:?}");
    assert_eq!(warnings, 1, "{findings:?}");
}

#[test]
fn validate_fail_file_sarif_is_well_formed() {
    // test_validate_rdf_sarif_is_well_formed.
    let output = gmeow()
        .arg("validate")
        .arg(fixture("fail.nq"))
        .args(["--format", "sarif"])
        .assert()
        .code(1)
        .get_output()
        .stdout
        .clone();
    let text = String::from_utf8(output).expect("utf-8 sarif");
    let sarif: serde_json::Value = serde_json::from_str(&text).expect("valid SARIF");
    let results = sarif["runs"][0]["results"]
        .as_array()
        .expect("sarif results array");
    assert_eq!(results.len(), 3, "two errors + one warning");
}

#[test]
fn validate_unknown_extension_hard_fails() {
    // test_validate_unknown_extension_hard_fails.
    let dir = scratch("badext");
    let bogus = dir.join("data.csv");
    std::fs::write(&bogus, "a,b,c\n").unwrap();
    gmeow()
        .arg("validate")
        .arg(&bogus)
        .assert()
        .failure()
        .stderr(predicate::str::contains("cannot infer format"));
}

// ── export / project / convert ───────────────────────────────────────────────

#[test]
fn export_respects_language_selector() {
    // test_export_respects_language_selector: --lang fr yields a label_fr column.
    let out = scratch("export");
    gmeow()
        .arg("export")
        .arg("--out")
        .arg(&out)
        .args(["--lang", "fr"])
        .assert()
        .success();
    let csv = out.join("gmeow-classes.csv");
    assert!(csv.exists(), "gmeow-classes.csv written");
    let text = std::fs::read_to_string(&csv).unwrap();
    assert!(text.contains("label_fr"), "french label column present");
    assert!(text.contains("label_fallback"), "fallback column present");
}

#[test]
fn project_schema_org_view_filter() {
    // Mirrors `gmeow project --profile schema.org`: the schema.org VIEW filter over
    // the bundle (the registry name is `schema-org`). Writes a Turtle projection.
    let out = scratch("project");
    gmeow()
        .args(["project", "--profile", "schema-org"])
        .arg("--out")
        .arg(&out)
        .assert()
        .success()
        .stdout(predicate::str::contains("schema-org.ttl"));
    assert!(out.join("schema-org.ttl").exists());
}

#[test]
fn project_unknown_view_fails() {
    let out = scratch("project-bad");
    gmeow()
        .args(["project", "--profile", "definitely-not-a-view"])
        .arg("--out")
        .arg(&out)
        .assert()
        .failure()
        .stderr(predicate::str::contains("unknown view"));
}

#[test]
fn convert_turtle_to_ntriples() {
    // `gmeow convert --from turtle --to ntriples` round-trips a triple to stdout.
    let dir = scratch("convert");
    let src = dir.join("in.ttl");
    std::fs::write(
        &src,
        "@prefix ex: <http://example.org/> .\nex:a ex:p ex:b .\n",
    )
    .unwrap();
    gmeow()
        .arg("convert")
        .arg(&src)
        .args(["--from", "turtle", "--to", "ntriples"])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "<http://example.org/a> <http://example.org/p> <http://example.org/b> .",
        ));
}

#[test]
fn convert_unknown_codec_fails() {
    let dir = scratch("convert-bad");
    let src = dir.join("in.ttl");
    std::fs::write(
        &src,
        "@prefix ex: <http://example.org/> .\nex:a ex:p ex:b .\n",
    )
    .unwrap();
    gmeow()
        .arg("convert")
        .arg(&src)
        .args(["--from", "turtle", "--to", "not-a-codec"])
        .assert()
        .failure();
}

// ── info / mcp / passthrough ─────────────────────────────────────────────────

#[test]
fn info_summarizes_the_bundle() {
    gmeow()
        .arg("info")
        .assert()
        .success()
        .stdout(predicate::str::contains("terms").and(predicate::str::contains("quads")));
}

#[test]
fn mcp_serves_a_native_jsonrpc_initialize() {
    // The consumer `gmeow mcp` runs the native stdio MCP server over the
    // embedded bundle: a JSON-RPC `initialize` request returns a well-formed
    // response identifying the server, and the transport closes cleanly on EOF.
    gmeow()
        .arg("mcp")
        .write_stdin("{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"initialize\",\"params\":{}}\n")
        .assert()
        .success()
        .stdout(
            predicate::str::contains("\"jsonrpc\":\"2.0\"")
                .and(predicate::str::contains("protocolVersion"))
                .and(predicate::str::contains("serverInfo")),
        );
}

#[test]
fn gts_shim_hard_fails_when_binary_missing() {
    // test_gts_shim_fails_when_binary_missing: with no `gts` on PATH and no
    // GMEOW_GTS_BIN, the shim hard-fails with an install hint.
    let bin = assert_cmd::cargo::cargo_bin("gmeow");
    let output = StdCommand::new(bin)
        .args(["gts", "info"])
        .env_clear()
        .env("PATH", "/nonexistent-path-for-tests")
        .output()
        .expect("run gmeow gts info");
    assert!(!output.status.success(), "must hard-fail without gts");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("gts binary not found"),
        "install hint on stderr: {stderr}"
    );
}

// ── affect intensity (Q10 production surface) ────────────────────────────────

/// Absolute path of a committed slice file, relative to this crate.
fn slice_path(relative: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../slices")
        .join(relative)
}

/// Build a fixture `.gts` at test time from the REAL committed affect slice
/// files: the canonical `module.ttl` (carries `gmeow:coreAffectGram` +
/// `gmeow:coreAxisIndex` AND the shipped schadenfreude worked A-Box) merged with
/// the intensity-discriminating worked example. This exercises the shipped instance
/// end to end, proving intensity is COMPUTED from the Gram matrix and appraisal
/// vectors — not read from a hand-authored magnitude.
fn affect_fixture_gts() -> PathBuf {
    use purrdf::gts_compose::{DEFAULT_RSYNCABLE_THRESHOLD, SnapshotBuilder, emit_gts};
    use purrdf::{NativeRdfFormat, parse_dataset};

    let mut builder = SnapshotBuilder::default();
    for relative in [
        "core/affect/module.ttl",
        "core/affect/examples/intensity-discriminating.ttl",
    ] {
        let text = std::fs::read(slice_path(relative)).expect("read slice file");
        let dataset = parse_dataset(&text, NativeRdfFormat::Turtle.media_type(), None)
            .unwrap_or_else(|e| panic!("parse {relative}: {e}"));
        builder.add_dataset(&dataset).expect("add dataset");
    }
    let bytes = emit_gts(
        &builder,
        "dist",
        None,
        Vec::new(),
        Vec::new(),
        None,
        None,
        None,
        DEFAULT_RSYNCABLE_THRESHOLD,
    )
    .expect("emit gts");

    let dir = scratch("affect-fixture");
    let path = dir.join("affect.gts");
    std::fs::write(&path, bytes).expect("write fixture gts");
    path
}

#[test]
fn affect_intensity_schadenfreude_is_computed_from_the_metric() {
    // Q10: the CLI computes √(xᵀGx) over the canonical coreAffectGram
    // (diagonal 1, valence–arousal coupling 1/4) for the schadenfreude vector
    // (valence 0.7, arousal 0.4): Q = 79/100, intensity 0.888819, dominant
    // valence. The metric-tensor norm is the load-bearing computation.
    let fixture = affect_fixture_gts();
    gmeow()
        .args(["affect", "intensity"])
        .arg(&fixture)
        .args([
            "--observation",
            "https://blackcatinformatics.ca/gmeow/schadenfreudeIntensity",
        ])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("intensity 0.888819").and(predicate::str::contains(
                "dominant-axis https://blackcatinformatics.ca/gmeow/dimensionValence",
            )),
        );
}

#[test]
fn affect_intensity_discriminating_dominant_axis_is_metric_aware() {
    // The discriminating case: raw-max axis is arousal (0.6 > 0.5), but the
    // computed G-weighted dominant is valence (diag(2,1): 2·0.5² = 0.5 >
    // 1·0.6² = 0.36) — the compute is load-bearing, not a raw-max read.
    let fixture = affect_fixture_gts();
    gmeow()
        .args(["affect", "intensity"])
        .arg(&fixture)
        .args([
            "--observation",
            "https://blackcatinformatics.ca/gmeow/examples/affect/tests/vdIntensity",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "dominant-axis https://blackcatinformatics.ca/gmeow/dimensionValence",
        ));
}

#[test]
fn affect_intensity_missing_source_is_a_runtime_error() {
    // Mirror the music runtime-error path: an unreadable source → exit 1 with an
    // `Error:` prefix on stderr.
    gmeow()
        .args(["affect", "intensity", "/nonexistent-affect-fixture.gts"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("Error:"));
}

#[test]
fn affect_intensity_to_without_observation_is_a_clap_usage_error() {
    // `--to` declares `requires = "observation"` at the clap layer, so omitting
    // `--observation` fails fast with a usage error (exit 2) before the source is
    // ever read — no runtime `Error:` fallback.
    gmeow()
        .args(["affect", "intensity", "/dev/null", "--to", "urn:x"])
        .assert()
        .failure()
        .code(2)
        .stderr(predicate::str::contains("--observation").and(predicate::str::contains("Usage:")));
}

#[test]
fn music_render_missing_source_is_a_runtime_error() {
    // The music passthrough maps a runtime failure (unreadable source) to exit 1
    // with an `Error:` prefix; the unsupported-format → exit 2 mapping is pinned by
    // the gmeow-music crate's own tests (it needs a valid piece to reach the format
    // check).
    let dir = scratch("music");
    let missing = dir.join("nope.gts");
    gmeow()
        .args(["music", "render"])
        .arg(&missing)
        .args(["--to", "midi"])
        .arg("--out")
        .arg(dir.join("out.mid"))
        .assert()
        .failure()
        .stderr(predicate::str::contains("Error:"));
}
