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
    let tmp_dir = tempfile::Builder::new()
        .prefix("gmeow-fanout-parity-")
        .tempdir()
        .unwrap();
    let tmp = tmp_dir.path();
    let dist = tmp.join("generated/dist");
    fs::create_dir_all(&dist).unwrap();
    fs::copy(&gts_src, dist.join("gmeow.gts")).unwrap();

    // Fanout writes the whole `generated/` tree from the bundle alone.
    let report = gmeow_pipeline::fanout(tmp, 4).expect("fanout runs");
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
    walk(&tmp.join("generated"), tmp, &mut written_paths);
    for path in &written_paths {
        if path == "generated/dist/gmeow.gts" {
            continue; // the seed bundle we copied in
        }
        assert!(
            projected.contains(path.as_str()),
            "fanout wrote an unprojected file: {path}"
        );
    }
}

/// Seed a fresh temp root with ONLY the shipped bundle and return it. The caller must
/// keep the returned `TempDir` alive for as long as the root is needed — it removes the
/// directory when dropped.
fn seed_bundle_only_root(tag: &str) -> tempfile::TempDir {
    let root = repo_root();
    let gts_src = root.join("generated/dist/gmeow.gts");
    let tmp_dir = tempfile::Builder::new()
        .prefix(&format!("gmeow-fanout-{tag}-"))
        .tempdir()
        .unwrap();
    let dist = tmp_dir.path().join("generated/dist");
    fs::create_dir_all(&dist).unwrap();
    fs::copy(&gts_src, dist.join("gmeow.gts")).unwrap();
    tmp_dir
}

/// The parallel write path (§6 "embarrassingly parallel") must be deterministic: the
/// produced tree and the [`FanoutReport`] counters cannot depend on `jobs`. Fanning out
/// serially (`jobs=1`) and highly-parallel (`jobs=8`) into two independently-seeded roots
/// must yield the SAME report and BYTE-IDENTICAL trees. Both roots start in the same
/// state (bundle only), so both are all-fresh writes: `written == produced, skipped == 0`.
#[test]
fn fanout_is_deterministic_regardless_of_jobs() {
    let serial_dir = seed_bundle_only_root("det-serial");
    let parallel_dir = seed_bundle_only_root("det-parallel");
    let serial_root = serial_dir.path();
    let parallel_root = parallel_dir.path();

    let serial = gmeow_pipeline::fanout(serial_root, 1).expect("serial fanout runs");
    let parallel = gmeow_pipeline::fanout(parallel_root, 8).expect("parallel fanout runs");

    // Same counters regardless of parallelism, and both empty-tree runs are all-fresh.
    assert_eq!(
        serial, parallel,
        "FanoutReport differs between jobs=1 and jobs=8"
    );
    assert_eq!(serial.written, serial.produced);
    assert_eq!(serial.skipped, 0);

    // Byte-identical trees: every file fanout wrote into the serial root matches the
    // same relative path under the parallel root. Enumerate via `walk()` on the serial
    // tree instead of re-projecting the bundle a third time (that projection dominates
    // this test's runtime and is redundant — `fanout` already ran the projection twice).
    let mut written_paths = Vec::new();
    walk(
        &serial_root.join("generated"),
        serial_root,
        &mut written_paths,
    );
    assert!(
        written_paths.len() > 50,
        "serial fanout produced implausibly few files ({}); the walk would vacuously pass",
        written_paths.len()
    );
    for path in &written_paths {
        if path == "generated/dist/gmeow.gts" {
            continue; // the seed bundle we copied in identically to both roots
        }
        let a = fs::read(serial_root.join(path)).unwrap();
        let b = fs::read(parallel_root.join(path))
            .unwrap_or_else(|e| panic!("parallel fanout did not write {path}: {e}"));
        assert_eq!(a, b, "parallel fanout diverged from serial at {path}");
    }
}

/// Fanout is idempotent: a SECOND run over an already-projected tree rewrites nothing.
/// This exercises the `skipped` counter deterministically under the parallel path —
/// `write_artifact` sees byte-identical files and reports no rewrite.
#[test]
fn fanout_second_run_skips_every_file() {
    let tmp_dir = seed_bundle_only_root("idempotent");
    let tmp = tmp_dir.path();

    let first = gmeow_pipeline::fanout(tmp, 8).expect("first fanout runs");
    assert_eq!(first.written, first.produced);
    assert_eq!(first.skipped, 0);

    // Nothing changed on disk, so the second run must rewrite nothing.
    let second = gmeow_pipeline::fanout(tmp, 8).expect("second fanout runs");
    assert_eq!(second.produced, first.produced);
    assert_eq!(
        second.written, 0,
        "second fanout rewrote {} file(s); projection is not idempotent",
        second.written
    );
    assert_eq!(second.skipped, second.produced);
}

#[test]
fn fanout_removes_stale_owned_artifacts() {
    let tmp_dir = seed_bundle_only_root("stale");
    let tmp = tmp_dir.path();
    let stale = tmp.join("generated/obsolete/nested/old.ttl");
    fs::create_dir_all(stale.parent().unwrap()).unwrap();
    fs::write(&stale, b"stale").unwrap();

    let report = gmeow_pipeline::fanout(tmp, 8).expect("fanout reconciles stale files");
    assert_eq!(report.removed, 1);
    assert!(!stale.exists());
    assert!(!tmp.join("generated/obsolete").exists());
}
