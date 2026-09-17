// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::collect_ttl;

/// A fresh scratch directory owned by the returned [`tempfile::TempDir`]: it is
/// unique per test by construction (so parallel tests never collide), and dropping
/// the guard removes it and everything under it — on success, on early return, and
/// on panic alike.
fn scratch_dir() -> tempfile::TempDir {
    tempfile::Builder::new()
        .prefix("gmeow-slice-brief-collect_ttl-")
        .tempdir()
        .expect("create scratch dir")
}

/// A missing directory is a legitimate "absent" input: `Ok(())`, nothing
/// collected — never an error, and never silently treated as anything but
/// empty.
#[test]
fn absent_directory_is_ok_and_empty() {
    let tmp = scratch_dir();
    // A path INSIDE the fresh scratch directory that is deliberately never created.
    let dir = tmp.path().join("absent");
    assert!(!dir.exists(), "precondition: {dir:?} must not exist");

    let mut out = Vec::new();
    let result = collect_ttl(&dir, &mut out);

    assert!(
        result.is_ok(),
        "a NotFound read_dir must be treated as absent (Ok), got {result:?}"
    );
    assert!(
        out.is_empty(),
        "an absent directory must collect zero paths, got {out:?}"
    );
}

/// A `read_dir` failure that is NOT `NotFound` (here: the parent path
/// component is a plain file, so the OS refuses with `NotADirectory`/`ENOTDIR`)
/// MUST propagate as an `Err`, never be laundered into "no .ttl files here".
/// This is deterministic and does not depend on running as non-root (unlike a
/// permission-bits test, which root would bypass).
#[test]
fn unreadable_non_directory_parent_errors() {
    let tmp = scratch_dir();
    let marker_file = tmp.path().join("marker");
    std::fs::write(&marker_file, b"not a directory").expect("write marker file");

    // `marker_file` is a plain file, so `marker_file/mappings` cannot be a
    // directory: `read_dir` must fail with something other than `NotFound`.
    let bogus_dir = marker_file.join("mappings");
    let mut out = Vec::new();
    let result = collect_ttl(&bogus_dir, &mut out);

    assert!(
        result.is_err(),
        "a non-NotFound read_dir error must propagate as Err, got {result:?}"
    );
    assert!(
        out.is_empty(),
        "no paths must be collected on the error path, got {out:?}"
    );
}
