// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The fanout parity gate (PIPELINE_SPINE §6): `fanout` reproduces every committed
//! `generated/` file from `gmeow.gts` ALONE.
//!
//! Seed a temp root with ONLY the shipped bundle, run [`gmeow_pipeline::fanout`], and
//! assert the projected tree is byte-identical to the committed `generated/` tree — the
//! "wipe → fanout → assert byte-identical tree" property. This proves fanout is pure
//! projection: no repository, no pipeline, no computation, just the bundle. It does not
//! run a build (that is `full_parity.rs`), so it stays cheap enough for the on-gate lane.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .canonicalize()
        .unwrap()
}

/// Every file under `dir`, repo-relative to `root`, skipping dot-entries.
fn walk(dir: &Path, root: &Path, out: &mut Vec<String>) {
    for entry in fs::read_dir(dir).unwrap() {
        let entry = entry.unwrap();
        if entry.file_name().to_string_lossy().starts_with('.') {
            continue;
        }
        let path = entry.path();
        if path.is_dir() {
            walk(&path, root, out);
        } else if path.is_file() {
            out.push(
                path.strip_prefix(root)
                    .unwrap()
                    .to_string_lossy()
                    .into_owned(),
            );
        }
    }
}

#[test]
fn fanout_reproduces_generated_from_the_bundle_alone() {
    let root = repo_root();
    let gts_src = root.join("generated/dist/gmeow.gts");

    // Temp root seeded with ONLY the shipped bundle — no other `generated/` file.
    let tmp = std::env::temp_dir().join(format!("gmeow-fanout-parity-{}", std::process::id()));
    let _ = fs::remove_dir_all(&tmp);
    let dist = tmp.join("generated/dist");
    fs::create_dir_all(&dist).unwrap();
    fs::copy(&gts_src, dist.join("gmeow.gts")).unwrap();

    // Fanout writes the whole `generated/` tree from the bundle alone.
    let report = gmeow_pipeline::fanout(&tmp, 4).expect("fanout runs");
    assert!(
        report.produced > 50,
        "fanout produced implausibly few files ({}); a rep class was dropped",
        report.produced
    );
    // The temp tree started empty (only the seed bundle), so every projected file is a
    // fresh write and nothing is skipped.
    assert_eq!(
        report.written, report.produced,
        "every projected file must be written into the empty temp tree"
    );
    assert_eq!(report.skipped, 0);

    // The projected set == the fanout write set (the one reconstruction authority).
    let gts = fs::read(&gts_src).unwrap();
    let projection = gmeow_pipeline::stages::superset::project_bundle(&gts).unwrap();
    let projected: BTreeSet<&str> = projection.files.keys().map(String::as_str).collect();

    // 1. Every projected file was written into `tmp` byte-identically to the committed
    //    file in the real repo. fanout(gmeow.gts) == committed bytes, for every file.
    for path in &projected {
        let produced =
            fs::read(tmp.join(path)).unwrap_or_else(|e| panic!("fanout did not write {path}: {e}"));
        let committed = fs::read(root.join(path))
            .unwrap_or_else(|e| panic!("committed file {path} missing: {e}"));
        assert_eq!(produced, committed, "fanout byte drift at {path}");
    }

    // 2. Fanout wrote NOTHING outside the projected set (besides the seed bundle) —
    //    the projection is exactly the committed tree, no orphan writes.
    let mut written_paths = Vec::new();
    walk(&tmp.join("generated"), &tmp, &mut written_paths);
    for path in &written_paths {
        if path == "generated/dist/gmeow.gts" {
            continue; // the seed bundle we copied in
        }
        assert!(
            projected.contains(path.as_str()),
            "fanout wrote an unprojected file: {path}"
        );
    }

    let _ = fs::remove_dir_all(&tmp);
}

/// Seed a fresh temp root with ONLY the shipped bundle and return its path.
fn seed_bundle_only_root(tag: &str) -> PathBuf {
    let root = repo_root();
    let gts_src = root.join("generated/dist/gmeow.gts");
    let tmp = std::env::temp_dir().join(format!("gmeow-fanout-{}-{}", tag, std::process::id()));
    let _ = fs::remove_dir_all(&tmp);
    let dist = tmp.join("generated/dist");
    fs::create_dir_all(&dist).unwrap();
    fs::copy(&gts_src, dist.join("gmeow.gts")).unwrap();
    tmp
}

/// The parallel write path (§6 "embarrassingly parallel") must be deterministic: the
/// produced tree and the [`FanoutReport`] counters cannot depend on `jobs`. Fanning out
/// serially (`jobs=1`) and highly-parallel (`jobs=8`) into two independently-seeded roots
/// must yield the SAME report and BYTE-IDENTICAL trees. Both roots start in the same
/// state (bundle only), so both are all-fresh writes: `written == produced, skipped == 0`.
#[test]
fn fanout_is_deterministic_regardless_of_jobs() {
    let serial_root = seed_bundle_only_root("det-serial");
    let parallel_root = seed_bundle_only_root("det-parallel");

    let serial = gmeow_pipeline::fanout(&serial_root, 1).expect("serial fanout runs");
    let parallel = gmeow_pipeline::fanout(&parallel_root, 8).expect("parallel fanout runs");

    // Same counters regardless of parallelism, and both empty-tree runs are all-fresh.
    assert_eq!(
        serial, parallel,
        "FanoutReport differs between jobs=1 and jobs=8"
    );
    assert_eq!(serial.written, serial.produced);
    assert_eq!(serial.skipped, 0);

    // Byte-identical trees: every projected path matches across the two roots.
    let gts = fs::read(repo_root().join("generated/dist/gmeow.gts")).unwrap();
    let projection = gmeow_pipeline::stages::superset::project_bundle(&gts).unwrap();
    for path in projection.files.keys() {
        let a = fs::read(serial_root.join(path)).unwrap();
        let b = fs::read(parallel_root.join(path)).unwrap();
        assert_eq!(a, b, "parallel fanout diverged from serial at {path}");
    }

    let _ = fs::remove_dir_all(&serial_root);
    let _ = fs::remove_dir_all(&parallel_root);
}

/// Fanout is idempotent: a SECOND run over an already-projected tree rewrites nothing.
/// This exercises the `skipped` counter deterministically under the parallel path —
/// `write_artifact` sees byte-identical files and reports no rewrite.
#[test]
fn fanout_second_run_skips_every_file() {
    let tmp = seed_bundle_only_root("idempotent");

    let first = gmeow_pipeline::fanout(&tmp, 8).expect("first fanout runs");
    assert_eq!(first.written, first.produced);
    assert_eq!(first.skipped, 0);

    // Nothing changed on disk, so the second run must rewrite nothing.
    let second = gmeow_pipeline::fanout(&tmp, 8).expect("second fanout runs");
    assert_eq!(second.produced, first.produced);
    assert_eq!(
        second.written, 0,
        "second fanout rewrote {} file(s); projection is not idempotent",
        second.written
    );
    assert_eq!(second.skipped, second.produced);

    let _ = fs::remove_dir_all(&tmp);
}
