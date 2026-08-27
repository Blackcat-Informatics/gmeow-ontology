// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! `gmeow-dev medium-gate`, driven against the producer-materialized bundle.
//!
//! The subject is the mandatory `generated/dist/gmeow.gts` producer output. `make check`
//! materializes the exact fixed point before the Rust DAG; CI downloads the independently
//! reproduced tree and verifies its producer receipt before compiling or running tests.
//! Reusing those authenticated bytes avoids a redundant whole-repository DAG execution.
//! Missing bytes or missing medium capabilities hard-fail; there is no weaker fallback.
//!
//! The test authenticates and reads that producer-owned path in place. It does not copy,
//! rewrite, tamper, or derive another corpus artifact.

use std::path::{Path, PathBuf};
use std::process::Command;

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .canonicalize()
        .expect("workspace root")
}

fn materialized_bundle(root: &Path) -> PathBuf {
    gmeow_bundle_import::authenticated_source_path(root)
        .expect("authenticated producer bundle path; tests never produce it")
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

#[test]
fn the_medium_gate_passes_the_authenticated_materialized_bundle() {
    let root = repo_root();
    let bundle_path = materialized_bundle(&root);
    let bundle_arg = bundle_path.to_str().expect("utf-8").to_string();

    // ── the producer-materialized bundle passes every clause ────────────────
    let (status, stdout, stderr) = medium_gate(&[&bundle_arg]);
    assert_eq!(status, 0, "the materialized bundle must pass:\n{stderr}");
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
}
