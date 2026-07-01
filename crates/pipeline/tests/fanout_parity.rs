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
    let report = gmeow_pipeline::fanout(&tmp).expect("fanout runs");
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
