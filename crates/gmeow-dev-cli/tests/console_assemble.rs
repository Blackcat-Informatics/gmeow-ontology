// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! `gmeow-dev console-assemble` acceptance.
//!
//! These tests prove the command REFUSES an `--out` equal to or inside a base that
//! the repository producer owns, and that the refusal NAMES that writer. The
//! deterministic rendering contract is covered at the map level by focused synthetic
//! tests; no test may assemble the repository console corpus.
//!
//! The refusal needs no corpus: it is decided before the bundle is opened, precisely so a
//! wrong `--out` is reported instead of being paid for.

use std::path::PathBuf;
use std::process::Command;

use assert_cmd::prelude::*;
use predicates::prelude::*;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .canonicalize()
        .expect("canonicalize repo root")
}

fn dev_cmd() -> Command {
    let mut cmd = Command::cargo_bin("gmeow-dev").expect("gmeow-dev binary");
    cmd.env("GMEOW_ROOT", repo_root());
    cmd
}

/// The refusal fires for the base itself AND for a path inside it, and names the writer.
#[test]
fn console_assemble_refuses_the_regen_owned_bases() {
    for out in [
        "ontology-docs",
        "ontology-docs/console",
        "./ontology-docs",
        "dist/gmeow-docs",
        "dist/gmeow-docs/site/console",
    ] {
        dev_cmd()
            .args(["console-assemble", "--out", out])
            .assert()
            .failure()
            .stderr(
                predicate::str::contains("refusing to write the console into")
                    .and(predicate::str::contains("make regen SYNC_OUTPUTS=docs")),
            );
    }
}

/// A sibling directory whose NAME merely starts with a refused base is NOT refused —
/// the guard compares normalized path components, not string prefixes.
#[test]
fn console_assemble_does_not_refuse_a_sibling_name() {
    // Anchored at a TEMPORARY root, not the repository. The guard resolves `--out` against
    // `GMEOW_ROOT`, so `dist/gmeow-docs-scratch` under a temp root is a genuine sibling of
    // that root's `dist/gmeow-docs` — the exact discrimination under test — while a
    // non-refused path that gets PAST the guard can no longer drop the full console tree
    // (engine wasm and all, ~13 MB) into the working checkout with nothing to clean it up.
    // The temp root carries no bundle, so the run fails on the bundle read that follows;
    // asserting only that the REFUSAL message is absent is exactly the claim.
    let root = tempfile::tempdir().expect("tempdir");
    let mut cmd = Command::cargo_bin("gmeow-dev").expect("gmeow-dev binary");
    cmd.env("GMEOW_ROOT", root.path());
    cmd.args(["console-assemble", "--out", "dist/gmeow-docs-scratch"])
        .assert()
        .stderr(predicate::str::contains("refusing to write the console into").not());
    assert!(
        !root.path().join("dist/gmeow-docs-scratch").exists(),
        "a run that cannot read a bundle must write no console tree at all"
    );
}
