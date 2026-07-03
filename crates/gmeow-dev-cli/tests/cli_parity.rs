// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! End-to-end parity tests for the `gmeow-dev` binary.
//!
//! These mirror the Python `tests/test_*` suites, one Rust test per behaviour:
//!
//! | Rust test                         | mirrors Python                          |
//! |-----------------------------------|-----------------------------------------|
//! | `version_prints_package_version`  | `test_cli_dev` version surface          |
//! | `logic_query_recursive_ancestor`  | `test_logic_cli::test_logic_query_*`    |
//! | `logic_compile_unknown_mode_fails`| `test_logic_cli::…_unknown_mode_fails`  |
//! | `external_tool_mirrors_child_exit`| `test_external_tool`                    |
//! | `external_tool_success_is_clean`  | `test_external_tool`                    |
//! | `slice_fix_deps_runs`             | `test_slice_fix_deps`                   |
//! | `feedback_writes_artifacts` (ign) | `test_cli_feedback`                     |
//! | `logic_compile_check` (ignored)   | `test_logic_cli::…_compile_check_*`     |
//!
//! The whole-pipeline / whole-gate commands (`logic compile --check`, `feedback`,
//! `regenerate`) exceed the 25s per-test budget, so they ride an OFF-GATE
//! `#[ignore]` lane behind `GMEOW_DEV_CLI_HEAVY=1` — the default `cargo nextest` /
//! `make check` never runs them; a maintainer opts in explicitly.

use std::path::PathBuf;
use std::process::Command;

use assert_cmd::prelude::*;
use predicates::prelude::*;

/// The repository root (this crate's manifest is `<root>/crates/gmeow-dev-cli`).
fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .canonicalize()
        .expect("canonicalize repo root")
}

/// A `gmeow-dev` command anchored at the repo root via `GMEOW_ROOT`.
fn dev_cmd() -> Command {
    let mut cmd = Command::cargo_bin("gmeow-dev").expect("gmeow-dev binary");
    cmd.env("GMEOW_ROOT", repo_root());
    cmd
}

#[test]
fn version_prints_package_version() {
    dev_cmd()
        .arg("version")
        .assert()
        .success()
        .stdout(predicate::str::starts_with(env!("CARGO_PKG_VERSION")));
}

#[test]
fn logic_query_recursive_ancestor() {
    let case = repo_root().join("conformance/logic/cases/profiles/goal-recursive-ancestor");
    if !case.is_dir() {
        eprintln!("conformance case absent; skipping");
        return;
    }
    let assert = dev_cmd()
        .arg("logic")
        .arg("query")
        .arg(case.join("input.nq"))
        .arg(case.join("queries/ancestor.logic"))
        .arg("--json")
        .assert()
        .success();
    let out = assert.get_output();
    let payload: serde_json::Value =
        serde_json::from_slice(&out.stdout).expect("query emits JSON on stdout");
    assert_eq!(payload["status"], "ok");
    let ys: std::collections::BTreeSet<String> = payload["bindings"]
        .as_array()
        .expect("bindings array")
        .iter()
        .map(|b| b["Y"].as_str().unwrap().to_owned())
        .collect();
    let expect: std::collections::BTreeSet<String> = [
        "<https://example.org/profiles/goal-recursive-ancestor/b>",
        "<https://example.org/profiles/goal-recursive-ancestor/c>",
        "<https://example.org/profiles/goal-recursive-ancestor/d>",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect();
    assert_eq!(ys, expect);
}

#[test]
fn logic_compile_unknown_mode_fails() {
    dev_cmd()
        .arg("logic")
        .arg("compile")
        .arg("--mode")
        .arg("bogus")
        .assert()
        .failure()
        .stderr(predicate::str::contains("unknown --mode"));
}

#[test]
fn external_tool_success_is_clean() {
    // A tool that succeeds yields a clean report and a zero exit.
    dev_cmd()
        .arg("external-tool")
        .arg("--name")
        .arg("truth")
        .arg("--diagnostics-artifacts")
        .arg("none")
        .arg("--")
        .arg("true")
        .assert()
        .success();
}

#[test]
fn external_tool_mirrors_child_exit() {
    // `false` exits 1; the wrapper MIRRORS that exact non-zero code.
    dev_cmd()
        .arg("external-tool")
        .arg("--name")
        .arg("falsehood")
        .arg("--diagnostics-artifacts")
        .arg("none")
        .arg("--")
        .arg("false")
        .assert()
        .failure()
        .code(1);
}

#[test]
fn external_tool_writes_artifacts() {
    let dir = tempdir();
    dev_cmd()
        .arg("external-tool")
        .arg("--name")
        .arg("falsehood")
        .arg("--diagnostics-dir")
        .arg(&dir)
        .arg("--diagnostics-artifacts")
        .arg("json,sarif")
        .arg("--diagnostics-stem")
        .arg("ext")
        .arg("--")
        .arg("false")
        .assert()
        .failure();
    assert!(dir.join("ext.json").is_file(), "wrote the JSON artifact");
    assert!(dir.join("ext.sarif").is_file(), "wrote the SARIF artifact");
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn slice_fix_deps_runs() {
    // Over the committed tree, slice-fix-deps runs to completion (proposes diffs
    // or reports none) without erroring.
    dev_cmd().arg("slice-fix-deps").assert().success();
}

#[test]
fn build_writes_serializations() {
    // `build` folds the committed snapshot and writes the derived serializations.
    dev_cmd()
        .arg("build")
        .assert()
        .success()
        .stdout(predicate::str::contains("gmeow.ttl"));
    assert!(repo_root().join("dist/gmeow.ttl").is_file());
}

#[test]
fn project_view_over_the_snapshot() {
    // `project --profile gmeow` filters the committed snapshot to the pure-GMEOW
    // view and writes a Turtle artifact.
    dev_cmd()
        .arg("project")
        .arg("--profile")
        .arg("gmeow")
        .assert()
        .success()
        .stdout(predicate::str::contains("gmeow.ttl"));
}

#[test]
fn extract_reference_only_target_is_refused() {
    // The license guard refuses a reference-only (CC-BY-SA) target (exit 1) …
    dev_cmd()
        .arg("extract")
        .arg("--target")
        .arg("ontouml")
        .assert()
        .failure()
        .stderr(predicate::str::contains("reference-only"));
    // … and permits an import-ok (MIT) target.
    dev_cmd()
        .arg("extract")
        .arg("--target")
        .arg("gufo")
        .assert()
        .success()
        .stdout(predicate::str::contains("import-ok"));
}

// ── OFF-GATE lane: whole-pipeline / whole-gate commands exceed the 25s budget ──

/// Whether the heavy off-gate lane is enabled (`GMEOW_DEV_CLI_HEAVY=1`).
fn heavy_enabled() -> bool {
    std::env::var("GMEOW_DEV_CLI_HEAVY").as_deref() == Ok("1")
}

#[test]
#[ignore = "off-gate: live Wikidata network lookup (set GMEOW_RUN_NETWORK=1)"]
fn wikidata_existence_live_lookup() {
    if std::env::var("GMEOW_RUN_NETWORK").as_deref() != Ok("1") {
        return;
    }
    // Every referenced QID/PID must resolve on the live Wikidata endpoint.
    dev_cmd()
        .arg("wikidata")
        .arg("--existence")
        .assert()
        .success()
        .stdout(predicate::str::contains("resolve on Wikidata"));
}

#[test]
#[ignore = "off-gate: runs the whole pipeline; exceeds the 25s budget"]
fn logic_compile_check_no_drift() {
    if !heavy_enabled() {
        return;
    }
    dev_cmd()
        .arg("logic")
        .arg("compile")
        .arg("--check")
        .assert()
        .success();
}

#[test]
#[ignore = "off-gate: whole-ontology SHACL over the sources; exceeds the 25s budget"]
fn validate_passes_on_the_clean_repo() {
    if !heavy_enabled() {
        return;
    }
    dev_cmd()
        .arg("validate")
        .assert()
        .success()
        .stdout(predicate::str::contains("validation passed"));
}

#[test]
#[ignore = "off-gate: reads the whole bundle + corpus; exceeds the 25s budget"]
fn up_projection_audit_runs() {
    if !heavy_enabled() {
        return;
    }
    dev_cmd()
        .arg("up-projection-audit")
        .assert()
        .success()
        .stdout(predicate::str::contains("liftable"));
}

#[test]
#[ignore = "off-gate: folds every gate surface; exceeds the 25s budget"]
fn feedback_writes_artifacts() {
    if !heavy_enabled() {
        return;
    }
    let dir = tempdir();
    let _ = dev_cmd()
        .arg("feedback")
        .arg("--diagnostics-dir")
        .arg(&dir)
        .arg("--diagnostics-stem")
        .arg("fb")
        .arg("--diagnostics-console")
        .arg("silent")
        .assert();
    assert!(
        dir.join("fb.gts").is_file(),
        "always writes the .gts bundle"
    );
    std::fs::remove_dir_all(&dir).ok();
}

/// A unique temp directory under the system temp dir (removed by the caller).
fn tempdir() -> PathBuf {
    let mut path = std::env::temp_dir();
    path.push(format!(
        "gmeow-dev-cli-test-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    std::fs::create_dir_all(&path).expect("create temp dir");
    path
}
