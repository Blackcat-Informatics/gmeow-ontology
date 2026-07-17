// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Production-surface proof that the SHIPPED `gmeow` binary exposes `slice
//! lint` — the real `Cli`/`Commands::Slice` clap dispatch in `src/lib.rs`,
//! never `gmeow_slice_quality::lint_report` called in-process.
//!
//! Mirrors `slice_quality_cli.rs`'s staging pattern: the fixture is copied
//! into a fresh `TempDir` with NO `slices/` ancestor before every drive, so
//! each test proves `gmeow slice lint <dir>` works zero-config against an
//! external slice OUTSIDE any checkout, exactly the consumer's situation.
//! Every assertion below is on STABLE, non-localized tokens — finding
//! CODES and the English render literals `lint OK` / `lint FAILED` — never
//! on tier LABELS, which are localized (e.g. the roll-up prints
//! "Enregistré" under some locales).

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

// ── exit 0: no bar declared, advisories surface as graded warnings ──────────

#[test]
fn slice_lint_undeclared_fixture_passes_with_graded_advisories() {
    let (_tmp, slice_dir) = staged_fixture();
    gmeow()
        .args(["slice", "lint"])
        .arg(&slice_dir)
        .assert()
        .success()
        .code(0)
        .stdout(predicate::str::contains(
            "slice-quality.doc-maturity.missing-dimension",
        ))
        .stdout(predicate::str::contains(
            "slice-quality.gmn1-coverage.uncovered",
        ))
        .stdout(predicate::str::contains("lint OK"));
}

// ── exit 1: an explicit --min-tier above the measured roll-up fails the gate ─

#[test]
fn slice_lint_min_tier_above_rollup_fails_exit_1() {
    let (_tmp, slice_dir) = staged_fixture();
    gmeow()
        .args(["slice", "lint", "--min-tier", "maximal"])
        .arg(&slice_dir)
        .assert()
        .failure()
        .code(1)
        .stdout(predicate::str::contains("lint FAILED"));
}

// ── the bottom rung of the ladder is always met ──────────────────────────────
//
// NOTE: `--min-tier` is resolved case-insensitively against EITHER the tier's
// `rdfs:label` or its IRI local name (`resolve_min_tier` in
// `crates/slice-quality/src/lint.rs`). The bundled rubric's bottom two rungs
// (`tierRegistered`, `tierGrounded`) currently carry a localized (French)
// `rdfs:label` in the embedded bundle — a real, observed engine fact, not a
// test artifact — so the bare English label fragment `"registered"` does NOT
// resolve. The IRI local name `tierRegistered` is stable regardless of label
// localization, so it is used here (the resolver's other accepted form).

#[test]
fn slice_lint_min_tier_at_bottom_rung_passes() {
    let (_tmp, slice_dir) = staged_fixture();
    gmeow()
        .args(["slice", "lint", "--min-tier", "tierRegistered"])
        .arg(&slice_dir)
        .assert()
        .success()
        .code(0)
        .stdout(predicate::str::contains(
            "slice-quality.doc-maturity.missing-dimension",
        ))
        .stdout(predicate::str::contains(
            "slice-quality.gmn1-coverage.uncovered",
        ))
        .stdout(predicate::str::contains("lint OK"));
}

// ── --format json is parseable and non-vacuous ───────────────────────────────

#[test]
fn slice_lint_json_is_parseable_and_non_vacuous() {
    let (_tmp, slice_dir) = staged_fixture();
    let output = gmeow()
        .args(["slice", "lint", "--format", "json"])
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

// ── exit 2: an unscorable directory hard-fails ───────────────────────────────

#[test]
fn slice_lint_junk_dir_hard_fails_exit_2() {
    let tmp = tempfile::TempDir::new().expect("temp dir");
    let junk = tmp.path().join("junk-slice");
    fs::create_dir_all(&junk).expect("create junk dir");
    fs::write(
        junk.join("manifest.ttl"),
        b"@@@ this is not valid turtle and declares no gmeow:Slice @@@",
    )
    .expect("write malformed manifest");
    gmeow()
        .args(["slice", "lint"])
        .arg(&junk)
        .assert()
        .failure()
        .code(2);
}

// ── exit 2: an unknown --format hard-fails, never silently defaults ─────────

#[test]
fn slice_lint_unknown_format_hard_fails_exit_2() {
    let (_tmp, slice_dir) = staged_fixture();
    gmeow()
        .args(["slice", "lint", "--format", "toon"])
        .arg(&slice_dir)
        .assert()
        .failure()
        .code(2)
        .stderr(predicate::str::contains("unknown --format"));
}

// ── exit 2: an unknown --min-tier hard-fails and names the rungs ────────────

#[test]
fn slice_lint_unknown_min_tier_hard_fails_exit_2() {
    let (_tmp, slice_dir) = staged_fixture();
    gmeow()
        .args(["slice", "lint", "--min-tier", "bogus"])
        .arg(&slice_dir)
        .assert()
        .failure()
        .code(2)
        .stderr(predicate::str::contains("unknown --min-tier"));
}

// ── lint and quality derive from ONE SliceReport — advisory-code parity ─────

#[test]
fn slice_lint_and_slice_quality_share_one_assessment() {
    let (_tmp, slice_dir) = staged_fixture();

    let quality_out = gmeow()
        .args(["slice", "quality", "--format", "json"])
        .arg(&slice_dir)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let quality_json: serde_json::Value =
        serde_json::from_slice(&quality_out).expect("quality stdout is parseable JSON");
    let quality_findings = quality_json
        .get("findings")
        .and_then(|f| f.as_array())
        .expect("quality report carries a findings array");

    let lint_out = gmeow()
        .args(["slice", "lint", "--format", "json"])
        .arg(&slice_dir)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let lint_json: serde_json::Value =
        serde_json::from_slice(&lint_out).expect("lint stdout is parseable JSON");
    let lint_findings = lint_json
        .get("findings")
        .and_then(|f| f.as_array())
        .expect("lint report carries a findings array");

    // quality's per-axis grade + roll-up notes are quality-only; lint's
    // synthetic below-min-tier finding is lint-only (and absent here — no
    // bar is set) — exclude both so what remains is the shared advisory set.
    let is_excluded = |code: &str| {
        code == "slice-quality.grade"
            || code == "slice-quality.rollup"
            || code == "slice-quality.lint.below-min-tier"
    };

    let quality_codes: std::collections::BTreeSet<&str> = quality_findings
        .iter()
        .filter_map(|f| f.get("code").and_then(|c| c.as_str()))
        .filter(|c| !is_excluded(c))
        .collect();
    let lint_codes: std::collections::BTreeSet<&str> = lint_findings
        .iter()
        .filter_map(|f| f.get("code").and_then(|c| c.as_str()))
        .filter(|c| !is_excluded(c))
        .collect();

    assert!(
        !quality_codes.is_empty(),
        "quality's advisory-code set is non-vacuous"
    );
    assert_eq!(
        quality_codes, lint_codes,
        "lint and quality derive from ONE score_external_slice_bytes SliceReport: \
         quality={quality_codes:?} lint={lint_codes:?}"
    );
}
