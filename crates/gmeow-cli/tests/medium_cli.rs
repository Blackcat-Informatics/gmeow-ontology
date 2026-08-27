// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The consumer `gmeow medium` verbs, driven against the producer-materialized bundle.
//!
//! # Why the materialized bundle is authoritative
//!
//! `crates/gmeow-cli/build.rs` embeds `generated/dist/gmeow.gts`, which is a git-ignored
//! producer output required before this test can compile. `make check` materializes the
//! exact fixed point before the Rust DAG; CI downloads and verifies the producer receipt
//! before building or running tests. Reading that mandatory artifact exercises the same
//! shipped bytes without launching a second whole-repository DAG inside the test. A
//! missing or stale-capability bundle hard-fails the assertions; there is no fallback.
//!
//! # One whole-bundle audit, one test
//!
//! Every clause lives in one test function so the bundle is folded only once per CLI
//! operation family rather than multiplying corpus-wide setup across tiny tests.

use std::path::{Path, PathBuf};
use std::process::Command;

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

fn materialized_bundle(root: &Path) -> PathBuf {
    gmeow_bundle_import::authenticated_source_path(root)
        .expect("authenticated producer bundle path; tests never produce it")
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

#[test]
fn the_medium_verbs_read_verify_and_explain_the_materialized_bundle() {
    let root = repo_root();
    let bundle_path = materialized_bundle(&root);
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
    // …and each measured population is reported ONCE. The bundle carries every
    // measurement twice — in `graph/medium-measurement` and in its `graph/fanout/…`
    // reconstruction twin — so a reader that scanned every graph printed one dictionary's
    // single two-part code as two findings.
    let mut populations: Vec<&str> = stdout
        .lines()
        .filter_map(|line| line.trim_start().strip_prefix("population "))
        .filter_map(|rest| rest.split(':').next())
        .collect();
    assert!(
        !populations.is_empty(),
        "medium explain reported no measured population:\n{stdout}"
    );
    let reported = populations.len();
    populations.sort_unstable();
    populations.dedup();
    assert_eq!(
        populations.len(),
        reported,
        "medium explain reported a measured population more than once — the shipped rows \
         and their fanout twin are the SAME measurement:\n{stdout}"
    );

    // An unknown dictionary id is a NAMED failure, not an empty report.
    let (status, _, stderr) = invoke(&["medium", "explain", "gmeow-not-a-dictionary", bundle_arg]);
    assert_ne!(status, 0, "an unknown dictionary id must fail");
    assert!(
        stderr.contains("pipeline.medium.unknown-dictionary"),
        "an unknown dictionary id must be reported under its named class:\n{stderr}"
    );

    // The authenticated, producer-owned bundle verifies in place. The test does not copy,
    // rewrite, tamper, or derive another corpus artifact.
    let (status, stdout, stderr) = invoke(&["medium", "verify", bundle_arg]);
    assert_eq!(
        status, 0,
        "verifying the materialized bundle failed:\n{stderr}"
    );
    assert!(stdout.contains("SelfDescribing"), "{stdout}");
    assert!(stdout.contains("verified:"), "{stdout}");
}
