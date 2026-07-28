// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! `gmeow-dev console-assemble` acceptance.
//!
//! Two claims, both gate blockers:
//!
//! 1. the command REFUSES an `--out` equal to or inside a base that
//!    `make regen SYNC_OUTPUTS=docs` owns, and the refusal NAMES that writer;
//! 2. assembling twice into two directories yields byte-identical trees.
//!
//! (2) needs a `generated/dist/gmeow.gts` that carries the `examples-archive` blob (the
//! fold this branch adds), which only `make regen` produces, so it rides the maintainer
//! `GMEOW_DEV_CLI_HEAVY` lane like every other bundle-reading parity test. A snapshot that
//! predates the fold makes the command HARD-FAIL naming the regenerate — which is the
//! intended behaviour, not a test-harness accident. The map-level half of the same claim
//! (`console_files` twice is byte-identical) runs unconditionally in
//! `gmeow-docs::console_producer`, and `console-assemble` is a deterministic write of
//! exactly that map.
//!
//! (1) needs nothing: the refusal is decided before the bundle is opened, precisely so a
//! wrong `--out` is reported instead of being paid for.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
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
    // The refusal is decided before any bundle read, so a non-refused path gets past the
    // guard; whether it then succeeds depends on the bundle, which this test does not
    // require. Asserting only that the REFUSAL message is absent keeps it hermetic.
    dev_cmd()
        .args(["console-assemble", "--out", "dist/gmeow-docs-scratch"])
        .assert()
        .stderr(predicate::str::contains("refusing to write the console into").not());
}

/// Read a directory tree into `{relative path: bytes}`.
fn read_tree(root: &Path) -> BTreeMap<String, Vec<u8>> {
    let mut out = BTreeMap::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).expect("read_dir") {
            let path = entry.expect("dir entry").path();
            if path.is_dir() {
                stack.push(path);
            } else {
                let rel = path
                    .strip_prefix(root)
                    .expect("under root")
                    .to_string_lossy()
                    .into_owned();
                out.insert(rel, std::fs::read(&path).expect("read file"));
            }
        }
    }
    out
}

/// Assembling twice into two directories yields byte-identical trees.
#[test]
#[ignore = "needs a gmeow.gts carrying the examples archive: run `make regen` first (maintainer lane)"]
fn console_assemble_twice_is_byte_identical() {
    let base = repo_root().join("target").join("console-assemble-parity");
    let _ = std::fs::remove_dir_all(&base);
    let a = base.join("a");
    let b = base.join("b");
    for out in [&a, &b] {
        dev_cmd()
            .args(["console-assemble", "--out"])
            .arg(out)
            .assert()
            .success();
    }
    let tree_a = read_tree(&a);
    let tree_b = read_tree(&b);
    assert!(!tree_a.is_empty(), "the assembled console tree is empty");
    assert_eq!(
        tree_a.keys().collect::<Vec<_>>(),
        tree_b.keys().collect::<Vec<_>>(),
        "two assemblies produced different key sets"
    );
    assert_eq!(tree_a, tree_b, "two assemblies produced different bytes");
    assert!(
        tree_a.contains_key("console/index.html"),
        "the assembled tree carries no console shell: {:?}",
        tree_a.keys().take(20).collect::<Vec<_>>()
    );
    assert!(
        tree_a.keys().any(|k| k.starts_with("assets/mcp-core/")),
        "the assembled tree carries no engine"
    );
    let _ = std::fs::remove_dir_all(&base);
}
