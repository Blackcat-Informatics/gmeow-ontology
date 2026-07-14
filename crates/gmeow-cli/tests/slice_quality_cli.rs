// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Production-surface proof that the SHIPPED `gmeow` binary exposes `slice
//! quality` — the real `Cli`/`Commands::Slice` clap dispatch in `src/lib.rs`,
//! never `gmeow_cli::commands::slice_quality` called in-process.
//!
//! Before this test, `score_external_slice`/`BundleStandards` were reachable only
//! from an integration test (`slice_quality_bundle.rs`) that calls the library
//! directly; `gmeow slice quality <dir>` returned `unrecognized subcommand`. This
//! drives the built binary through `assert_cmd`, exactly like `tests/cli.rs`, and
//! stages the fixture into a fresh `TempDir` (mirroring the staging pattern in
//! `slice_quality_bundle.rs`) to prove no `slices/` repo checkout is needed.

use std::fs;
use std::path::{Path, PathBuf};

use assert_cmd::Command;
use predicates::prelude::*;

/// The built `gmeow` binary.
fn gmeow() -> Command {
    Command::cargo_bin("gmeow").expect("gmeow binary builds")
}

/// The authored fixture root under this crate's `tests/` tree.
fn fixture_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/external-slice")
}

/// Recursively copy `src` into `dst` (creating `dst`). Deterministic, files + dirs.
fn copy_tree(src: &Path, dst: &Path) {
    fs::create_dir_all(dst).expect("create dest dir");
    for entry in fs::read_dir(src).expect("read source dir") {
        let entry = entry.expect("dir entry");
        let from = entry.path();
        let to = dst.join(entry.file_name());
        if entry.file_type().expect("file type").is_dir() {
            copy_tree(&from, &to);
        } else {
            fs::copy(&from, &to).expect("copy file");
        }
    }
}

/// Copy the fixture into a fresh temp dir with NO `slices/` ancestor and return the
/// (owned tempdir, scored slice path). The tempdir must be kept alive by the caller.
fn staged_fixture() -> (tempfile::TempDir, PathBuf) {
    let tmp = tempfile::TempDir::new().expect("temp dir");
    let slice_dir = tmp.path().join("external-slice");
    copy_tree(&fixture_root(), &slice_dir);
    assert!(
        !slice_dir.components().any(|c| c.as_os_str() == "slices"),
        "the staged slice path must have no slices/ ancestor: {}",
        slice_dir.display()
    );
    (tmp, slice_dir)
}

// ── `gmeow slice quality <dir>` (human, default format) ──────────────────────

#[test]
fn slice_quality_human_scores_staged_external_slice_non_vacuously() {
    let (_tmp, slice_dir) = staged_fixture();
    gmeow()
        .args(["slice", "quality"])
        .arg(&slice_dir)
        .assert()
        .success()
        // Non-vacuous: the fixture is measured strictly below the ceiling on both
        // environment-anchored axes, at the SAME values the library-level
        // `slice_quality_bundle.rs` test pins.
        .stdout(predicate::str::contains("axisGmn1Coverage").and(predicate::str::contains("0.96")))
        .stdout(predicate::str::contains("axisDocMaturity").and(predicate::str::contains("0.75")))
        // The named missing-dimension advisory (a FULL-anchor DocMaturity gap) is
        // present in the human render, not just internally computed.
        .stdout(predicate::str::contains(
            "slice-quality.doc-maturity.missing-dimension",
        ))
        .stdout(predicate::str::contains("dimRealizedState"))
        // The named gmn1 uncovered-quad advisory is present too.
        .stdout(predicate::str::contains(
            "slice-quality.gmn1-coverage.uncovered",
        ));
}

// ── `--format json` ───────────────────────────────────────────────────────────

#[test]
fn slice_quality_json_emits_a_parseable_report_with_the_same_advisories() {
    let (_tmp, slice_dir) = staged_fixture();
    let output = gmeow()
        .args(["slice", "quality", "--format", "json"])
        .arg(&slice_dir)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let json: serde_json::Value =
        serde_json::from_slice(&output).expect("stdout is parseable JSON");
    let findings = json
        .get("findings")
        .and_then(|f| f.as_array())
        .expect("report carries a findings array");
    assert!(!findings.is_empty(), "the JSON report is non-vacuous");
    let codes: Vec<&str> = findings
        .iter()
        .filter_map(|f| f.get("code").and_then(|c| c.as_str()))
        .collect();
    assert!(
        codes.contains(&"slice-quality.doc-maturity.missing-dimension"),
        "the missing-dimension advisory survives the JSON render: {codes:?}"
    );
    assert!(
        codes.contains(&"slice-quality.gmn1-coverage.uncovered"),
        "the gmn1-coverage advisory survives the JSON render: {codes:?}"
    );
}

// ── an unscorable directory hard-fails, never a vacuous passing report ───────

#[test]
fn slice_quality_on_a_junk_directory_hard_fails() {
    let tmp = tempfile::TempDir::new().expect("temp dir");
    let junk = tmp.path().join("junk-slice");
    fs::create_dir_all(&junk).expect("create junk dir");
    fs::write(
        junk.join("manifest.ttl"),
        b"@@@ this is not valid turtle and declares no gmeow:Slice @@@",
    )
    .expect("write malformed manifest");
    gmeow()
        .args(["slice", "quality"])
        .arg(&junk)
        .assert()
        .failure();
}

// ── an unknown --format hard-fails, never silently defaults ──────────────────

#[test]
fn slice_quality_unknown_format_hard_fails() {
    let (_tmp, slice_dir) = staged_fixture();
    gmeow()
        .args(["slice", "quality", "--format", "toon"])
        .arg(&slice_dir)
        .assert()
        .failure()
        .stderr(predicate::str::contains("unknown --format"));
}
